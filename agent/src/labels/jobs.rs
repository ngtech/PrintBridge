use super::{backend::{PrintDocument, PrintFailure, PrinterBackend}, config::{valid_name, LabelSettings, Profile}, download, pdf};
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Url;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{path::{Path, PathBuf}, sync::{Arc, Mutex, MutexGuard}};
use tokio::sync::mpsc;

const QUEUE_CAPACITY: usize = 16;

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub code: &'static str,
    pub message: String,
}

impl ApiError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }

    fn storage(_: anyhow::Error) -> Self {
        Self::new("STORAGE_ERROR", "Print job history is unavailable; do not create a new request until checked")
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    job_id: String,
    profile: String,
    status: String,
    error_code: Option<String>,
    error: Option<String>,
    exit_code: Option<i32>,
    may_have_printed: bool,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Submit {
    pub url: String,
    pub profile: String,
    pub copies: Option<u16>,
}

#[derive(Clone)]
struct Key {
    origin: String,
    id: String,
}

struct Ledger(Mutex<Connection>);

impl Ledger {
    fn open(database: &Path) -> Result<Self> {
        let connection = Connection::open(database)?;
        connection.busy_timeout(std::time::Duration::from_secs(1))?;
        connection.execute_batch(
            "PRAGMA synchronous = FULL;
             CREATE TABLE IF NOT EXISTS label_jobs (
                origin TEXT NOT NULL, id TEXT NOT NULL, fingerprint BLOB NOT NULL,
                profile TEXT NOT NULL, state TEXT NOT NULL, error_code TEXT, error TEXT,
                exit_code INTEGER, may_have_printed INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (origin, id)
             );
             UPDATE label_jobs SET state = 'interrupted', error_code = 'AGENT_RESTARTED',
                 error = 'Agent restarted before a final result; check the Windows queue before reprinting'
             WHERE state IN ('queued', 'preparing', 'printing');"
        )?;
        Ok(Self(Mutex::new(connection)))
    }

    fn lock(&self) -> Result<MutexGuard<'_, Connection>> {
        self.0.lock().map_err(|_| anyhow::anyhow!("Print job ledger lock failed"))
    }

    fn read(connection: &Connection, key: &Key) -> Result<Option<(Vec<u8>, Job)>> {
        Ok(connection.query_row(
            "SELECT fingerprint, profile, state, error_code, error, exit_code, may_have_printed
             FROM label_jobs WHERE origin = ?1 AND id = ?2",
            params![key.origin, key.id],
            |row| Ok((row.get(0)?, Job {
                job_id: key.id.clone(), profile: row.get(1)?, status: row.get(2)?,
                error_code: row.get(3)?, error: row.get(4)?, exit_code: row.get(5)?,
                may_have_printed: row.get(6)?,
            }))
        ).optional()?)
    }

    fn advance(&self, key: &Key, from: &str, to: &str) -> Result<bool> {
        Ok(self.lock()?.execute(
            "UPDATE label_jobs SET state = ?4, may_have_printed = (?4 = 'printing')
             WHERE origin = ?1 AND id = ?2 AND state = ?3",
            params![key.origin, key.id, from, to],
        )? == 1)
    }

    fn finish(&self, key: &Key, from: &str, result: std::result::Result<(), PrintFailure>) -> Result<()> {
        let (state, code, message, exit_code, may_have_printed) = match result {
            Ok(()) => ("sumatraAccepted", None, None, Some(0), true),
            Err(error) => ("failed", Some(error.code), Some(error.message), error.exit_code, error.may_have_printed),
        };
        self.lock()?.execute(
            "UPDATE label_jobs SET state = ?4, error_code = ?5, error = ?6, exit_code = ?7, may_have_printed = ?8
             WHERE origin = ?1 AND id = ?2 AND state = ?3",
            params![key.origin, key.id, from, state, code, message, exit_code, may_have_printed],
        )?;
        Ok(())
    }
}

struct Work {
    key: Key,
    url: Url,
    profile: Profile,
    copies: u16,
}

#[async_trait]
pub trait DocumentSource: Send + Sync + 'static {
    async fn fetch(&self, url: Url, origins: &[String]) -> Result<Vec<u8>>;
}

pub struct HttpDocuments;

#[async_trait]
impl DocumentSource for HttpDocuments {
    async fn fetch(&self, url: Url, origins: &[String]) -> Result<Vec<u8>> {
        download::fetch(url, origins).await
    }
}

#[derive(Clone)]
pub struct Jobs {
    ledger: Arc<Ledger>,
    queue: mpsc::Sender<Work>,
    settings: Arc<LabelSettings>,
}

impl Jobs {
    pub fn open(directory: &Path, settings: Arc<LabelSettings>, backend: Arc<dyn PrinterBackend>,
        documents: Arc<dyn DocumentSource>) -> Result<Self> {
        let temporary = directory.join("print-tmp");
        std::fs::create_dir_all(&temporary)?;
        for entry in std::fs::read_dir(&temporary)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() && entry.file_name().to_string_lossy().starts_with("label-") {
                if std::fs::remove_dir_all(entry.path()).is_err() {
                    log::warn!("Could not remove an interrupted label job's temporary files");
                }
            }
        }
        let ledger = Arc::new(Ledger::open(&directory.join("label-jobs.sqlite3"))?);
        let (queue, items) = mpsc::channel(QUEUE_CAPACITY);
        tokio::spawn(work_loop(items, ledger.clone(), settings.clone(), backend, documents, temporary));
        Ok(Self { ledger, queue, settings })
    }

    pub fn submit(&self, origin: &str, id: &str, request: Submit) -> std::result::Result<Job, ApiError> {
        let key = key(origin, id)?;
        let serialized = serde_json::to_vec(&request)
            .map_err(|_| ApiError::new("INVALID_REQUEST", "Invalid job payload"))?;
        let fingerprint = Sha256::digest(serialized);
        let connection = self.ledger.lock().map_err(ApiError::storage)?;
        if let Some((original, job)) = Ledger::read(&connection, &key).map_err(ApiError::storage)? {
            if original.as_slice() != &fingerprint[..] {
                return Err(ApiError::new("REQUEST_ID_CONFLICT", "Request ID already belongs to a different submission"));
            }
            return Ok(job);
        }
        let profile = self.settings.profile(&request.profile)
            .map_err(|error| ApiError::new("INVALID_PROFILE", error.to_string()))?.clone();
        let copies = profile.copies(request.copies)
            .map_err(|error| ApiError::new("INVALID_COPIES", error.to_string()))?;
        let url = download::allowed_url(&request.url, &self.settings.document_origins)
            .map_err(|error| ApiError::new("BLOCKED_URL", error.to_string()))?;
        let permit = self.queue.try_reserve()
            .map_err(|_| ApiError::new("QUEUE_UNAVAILABLE", "Print queue is full or unavailable; retry the same request ID"))?;
        connection.execute(
            "INSERT INTO label_jobs (origin, id, fingerprint, profile, state) VALUES (?1, ?2, ?3, ?4, 'queued')",
            params![key.origin, key.id, &fingerprint[..], request.profile],
        ).map_err(|error| ApiError::storage(error.into()))?;
        let job = Job {
            job_id: key.id.clone(), profile: request.profile, status: "queued".into(),
            error_code: None, error: None, exit_code: None, may_have_printed: false,
        };
        permit.send(Work { key, url, profile, copies });
        Ok(job)
    }

    pub fn status(&self, origin: &str, id: &str) -> std::result::Result<Job, ApiError> {
        let key = key(origin, id)?;
        let connection = self.ledger.lock().map_err(ApiError::storage)?;
        Ledger::read(&connection, &key).map_err(ApiError::storage)?
            .map(|(_, job)| job)
            .ok_or_else(|| ApiError::new("JOB_NOT_FOUND", "Print job not found"))
    }

    pub fn cancel(&self, origin: &str, id: &str) -> std::result::Result<Job, ApiError> {
        let key = key(origin, id)?;
        let connection = self.ledger.lock().map_err(ApiError::storage)?;
        connection.execute(
            "UPDATE label_jobs SET state = 'cancelled' WHERE origin = ?1 AND id = ?2
             AND state IN ('queued', 'preparing')", params![key.origin, key.id],
        ).map_err(|error| ApiError::storage(error.into()))?;
        let (_, job) = Ledger::read(&connection, &key).map_err(ApiError::storage)?
            .ok_or_else(|| ApiError::new("JOB_NOT_FOUND", "Print job not found"))?;
        if job.status != "cancelled" {
            return Err(ApiError::new("TOO_LATE_TO_CANCEL", "Job is terminal or has reached SumatraPDF dispatch; check the Windows queue"));
        }
        Ok(job)
    }
}

fn key(origin: &str, id: &str) -> std::result::Result<Key, ApiError> {
    if !valid_name(id) {
        return Err(ApiError::new("INVALID_REQUEST_ID", "Job ID must be 1–64 ASCII letters, digits, underscores or hyphens"));
    }
    Ok(Key { origin: origin.into(), id: id.into() })
}

async fn work_loop(mut items: mpsc::Receiver<Work>, ledger: Arc<Ledger>, settings: Arc<LabelSettings>,
    backend: Arc<dyn PrinterBackend>, documents: Arc<dyn DocumentSource>, temporary: PathBuf) {
    while let Some(work) = items.recv().await {
        if run_job(&work, &ledger, &settings, backend.as_ref(), documents.as_ref(), &temporary).await.is_err() {
            log::error!("Print job storage failed; stopping dispatch to avoid unsafe retries");
            break;
        }
    }
}

async fn run_job(work: &Work, ledger: &Ledger, settings: &LabelSettings, backend: &dyn PrinterBackend,
    documents: &dyn DocumentSource, temporary: &Path) -> Result<()> {
    if !ledger.advance(&work.key, "queued", "preparing")? {
        return Ok(());
    }
    let prepared = prepare(work, settings, backend, documents, temporary).await;
    let (plan, document) = match prepared {
        Ok(value) => value,
        Err(error) => return ledger.finish(&work.key, "preparing", Err(error)),
    };
    if !ledger.advance(&work.key, "preparing", "printing")? {
        return Ok(());
    }
    let result = backend.print(plan, &work.profile, work.copies, document.pdf(), document.app_data()).await;
    drop(document);
    ledger.finish(&work.key, "printing", result)
}

async fn prepare(work: &Work, settings: &LabelSettings, backend: &dyn PrinterBackend,
    documents: &dyn DocumentSource, temporary: &Path)
    -> std::result::Result<(super::backend::PreparedPrint, PrintDocument), PrintFailure> {
    let bytes = documents.fetch(work.url.clone(), &settings.document_origins).await
        .map_err(|error| PrintFailure::before_print("DOWNLOAD_FAILED", error.to_string()))?;
    let profile = work.profile.clone();
    let copies = work.copies;
    let temporary = temporary.to_owned();
    let document = tokio::task::spawn_blocking(move || {
        pdf::validate(&bytes, &profile, copies)
            .map_err(|error| PrintFailure::before_print("INVALID_PDF", error.to_string()))?;
        PrintDocument::new(&temporary, &bytes)
            .map_err(|_| PrintFailure::before_print("TEMP_FILE_FAILED", "Cannot write temporary label PDF"))
    }).await.context("PDF validation worker failed")
        .map_err(|error| PrintFailure::before_print("INVALID_PDF", error.to_string()))??;
    let plan = backend.prepare(&work.profile, work.copies).await?;
    Ok((plan, document))
}
