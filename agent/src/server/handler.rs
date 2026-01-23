use crate::cache::UploadManager;
use crate::config::Settings;
use crate::download::DownloadManager;
use crate::models::*;
use crate::print::{create_print_engine, detect_content_type, PrintEngine};
use crate::protocol::{error_codes, AgentMessage, ClientMessage};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

/// Handles incoming WebSocket messages
pub struct MessageHandler {
    settings: Arc<Settings>,
    print_engine: Arc<dyn PrintEngine>,
    download_manager: Arc<DownloadManager>,
    upload_manager: Arc<UploadManager>,
    jobs: Arc<Mutex<HashMap<String, PrintJob>>>,
    start_time: Instant,
}

impl MessageHandler {
    pub fn new(settings: Arc<Settings>) -> Result<Self> {
        let print_engine = create_print_engine()?;
        let cache_dir = settings.temp_dir()?;

        let download_manager = DownloadManager::new(
            cache_dir.clone(),
            settings.security.https_only,
        )?;

        let upload_manager = UploadManager::new(cache_dir);

        Ok(Self {
            settings,
            print_engine: Arc::from(print_engine),
            download_manager: Arc::new(download_manager),
            upload_manager: Arc::new(upload_manager),
            jobs: Arc::new(Mutex::new(HashMap::new())),
            start_time: Instant::now(),
        })
    }

    /// Handle an incoming message
    pub async fn handle_message(&self, msg: ClientMessage) -> AgentMessage {
        match msg {
            ClientMessage::GetStatus { id } => self.handle_get_status(id).await,
            ClientMessage::GetPrinters { id } => self.handle_get_printers(id).await,
            ClientMessage::Print { id, request } => self.handle_print(id, request, false).await,
            ClientMessage::PrintRaw { id, request } => self.handle_print(id, request, true).await,
            ClientMessage::PrintBatch { id, request } => self.handle_print_batch(id, request).await,
            ClientMessage::CancelJob { id, job_id } => self.handle_cancel_job(id, job_id).await,
            ClientMessage::CancelBatch { id, batch_id } => {
                self.handle_cancel_batch(id, batch_id).await
            }
            ClientMessage::UploadStart {
                id,
                upload_id,
                filename,
                total_size,
                ..
            } => self.handle_upload_start(id, upload_id, filename, total_size).await,
            ClientMessage::UploadChunk {
                id,
                upload_id,
                chunk_index,
                data,
            } => self.handle_upload_chunk(id, upload_id, chunk_index, data).await,
            ClientMessage::UploadComplete {
                id,
                upload_id,
                request,
            } => self.handle_upload_complete(id, upload_id, request).await,
            ClientMessage::Ping { id } => AgentMessage::Pong { id },
        }
    }

    async fn handle_get_status(&self, id: String) -> AgentMessage {
        let printers = self.print_engine.get_printers().unwrap_or_default();
        let jobs = self.jobs.lock().await;
        let queue_length = jobs.values().filter(|j| j.status == JobStatus::Pending).count();

        AgentMessage::Status {
            id,
            status: AgentStatus {
                version: env!("CARGO_PKG_VERSION").to_string(),
                platform: std::env::consts::OS.to_string(),
                printer_count: printers.len(),
                queue_length,
                uptime: self.start_time.elapsed().as_millis() as u64,
            },
        }
    }

    async fn handle_get_printers(&self, id: String) -> AgentMessage {
        match self.print_engine.get_printers() {
            Ok(printers) => AgentMessage::Printers { id, printers },
            Err(e) => AgentMessage::error(id, error_codes::INTERNAL_ERROR, &e.to_string()),
        }
    }

    async fn handle_print(&self, id: String, request: PrintRequest, is_raw: bool) -> AgentMessage {
        // Get default printer if not specified
        let default_printer = self
            .settings
            .printing
            .default_printer
            .clone()
            .or_else(|| self.print_engine.get_default_printer().ok().flatten())
            .unwrap_or_default();

        let mut job = PrintJob::new(request.clone(), &default_printer);

        // Validate printer exists
        if let Err(e) = self.print_engine.printer_exists(&job.printer) {
            return AgentMessage::error(id, error_codes::PRINTER_NOT_FOUND, &e.to_string());
        }

        // Store job
        {
            let mut jobs = self.jobs.lock().await;
            jobs.insert(job.id.clone(), job.clone());
        }

        // Send job accepted
        let job_accepted = AgentMessage::JobAccepted {
            id: id.clone(),
            job: job.clone(),
        };

        // Process job asynchronously
        let job_id = job.id.clone();
        let print_engine = self.print_engine.clone();
        let download_manager = self.download_manager.clone();
        let jobs = self.jobs.clone();

        tokio::spawn(async move {
            let result = process_print_job(
                &mut job,
                &request,
                is_raw,
                print_engine.as_ref(),
                download_manager.as_ref(),
            )
            .await;

            // Update job status
            let mut jobs = jobs.lock().await;
            if let Some(stored_job) = jobs.get_mut(&job_id) {
                match result {
                    Ok(()) => {
                        stored_job.set_status(JobStatus::Completed);
                        log::info!("Job {} completed successfully", job_id);
                    }
                    Err(e) => {
                        stored_job.set_error(e.to_string());
                        log::error!("Job {} failed: {}", job_id, e);
                    }
                }
            }
        });

        job_accepted
    }

    async fn handle_print_batch(&self, id: String, request: BatchRequest) -> AgentMessage {
        let batch_id = request.id.clone();
        let total = request.jobs.len();

        // Process batch asynchronously
        let settings = self.settings.clone();
        let print_engine = self.print_engine.clone();
        let download_manager = self.download_manager.clone();
        let jobs_store = self.jobs.clone();

        tokio::spawn(async move {
            let default_printer = settings
                .printing
                .default_printer
                .clone()
                .or_else(|| print_engine.get_default_printer().ok().flatten())
                .unwrap_or_default();

            let mut completed = 0usize;
            let mut failed = 0usize;
            let mut cancelled = 0usize;
            let mut result_jobs = Vec::new();

            for print_request in request.jobs {
                let mut job = PrintJob::new(print_request.clone(), &default_printer);

                // Store job
                {
                    let mut jobs = jobs_store.lock().await;
                    jobs.insert(job.id.clone(), job.clone());
                }

                // Process job
                let result = process_print_job(
                    &mut job,
                    &print_request,
                    false,
                    print_engine.as_ref(),
                    download_manager.as_ref(),
                )
                .await;

                match result {
                    Ok(()) => {
                        job.set_status(JobStatus::Completed);
                        completed += 1;
                    }
                    Err(e) => {
                        job.set_error(e.to_string());
                        failed += 1;

                        match request.on_error {
                            BatchErrorMode::Stop => {
                                cancelled = total - completed - failed;
                                result_jobs.push(job);
                                break;
                            }
                            BatchErrorMode::Continue => {
                                // Continue to next job
                            }
                            BatchErrorMode::Retry => {
                                // Retry logic
                                let mut retry_count = 0;
                                while retry_count < request.max_retries {
                                    tokio::time::sleep(tokio::time::Duration::from_millis(
                                        request.retry_delay,
                                    ))
                                    .await;

                                    let mut retry_job = PrintJob::new(
                                        print_request.clone(),
                                        &default_printer,
                                    );

                                    let retry_result = process_print_job(
                                        &mut retry_job,
                                        &print_request,
                                        false,
                                        print_engine.as_ref(),
                                        download_manager.as_ref(),
                                    )
                                    .await;

                                    if retry_result.is_ok() {
                                        job = retry_job;
                                        job.set_status(JobStatus::Completed);
                                        failed -= 1;
                                        completed += 1;
                                        break;
                                    }

                                    retry_count += 1;
                                }
                            }
                        }
                    }
                }

                // Update stored job
                {
                    let mut jobs = jobs_store.lock().await;
                    if let Some(stored) = jobs.get_mut(&job.id) {
                        *stored = job.clone();
                    }
                }

                result_jobs.push(job);
            }

            log::info!(
                "Batch {} completed: {}/{} successful, {} failed, {} cancelled",
                batch_id,
                completed,
                total,
                failed,
                cancelled
            );
        });

        AgentMessage::BatchAccepted {
            id,
            batch_id,
            total,
        }
    }

    async fn handle_cancel_job(&self, id: String, job_id: String) -> AgentMessage {
        let mut jobs = self.jobs.lock().await;
        if let Some(job) = jobs.get_mut(&job_id) {
            if job.status == JobStatus::Pending {
                job.set_status(JobStatus::Cancelled);
                return AgentMessage::JobStatus {
                    id,
                    job: job.clone(),
                };
            }
        }
        AgentMessage::error(id, error_codes::JOB_NOT_FOUND, "Job not found or already started")
    }

    async fn handle_cancel_batch(&self, id: String, _batch_id: String) -> AgentMessage {
        // Batch cancellation would need more sophisticated tracking
        AgentMessage::error(
            id,
            error_codes::INTERNAL_ERROR,
            "Batch cancellation not yet implemented",
        )
    }

    async fn handle_upload_start(
        &self,
        id: String,
        upload_id: String,
        filename: String,
        total_size: u64,
    ) -> AgentMessage {
        match self
            .upload_manager
            .start_upload(&upload_id, &filename, total_size)
            .await
        {
            Ok(()) => AgentMessage::UploadProgress {
                id,
                upload_id,
                received: 0,
                total: total_size,
            },
            Err(e) => AgentMessage::error(id, error_codes::UPLOAD_FAILED, &e.to_string()),
        }
    }

    async fn handle_upload_chunk(
        &self,
        id: String,
        upload_id: String,
        chunk_index: u32,
        data: String,
    ) -> AgentMessage {
        match self
            .upload_manager
            .receive_chunk(&upload_id, chunk_index, &data)
            .await
        {
            Ok((received, total)) => AgentMessage::UploadProgress {
                id,
                upload_id,
                received,
                total,
            },
            Err(e) => AgentMessage::error(id, error_codes::UPLOAD_FAILED, &e.to_string()),
        }
    }

    async fn handle_upload_complete(
        &self,
        id: String,
        upload_id: String,
        request: PrintRequest,
    ) -> AgentMessage {
        match self.upload_manager.complete_upload(&upload_id).await {
            Ok(path) => {
                // Create a new request with the file path
                let mut file_request = request;
                file_request.url = Some(format!("file://{}", path.display()));

                // Process as regular print job
                self.handle_print(id, file_request, false).await
            }
            Err(e) => AgentMessage::error(id, error_codes::UPLOAD_FAILED, &e.to_string()),
        }
    }
}

async fn process_print_job(
    job: &mut PrintJob,
    request: &PrintRequest,
    is_raw: bool,
    print_engine: &dyn PrintEngine,
    download_manager: &DownloadManager,
) -> Result<()> {
    // Download document if URL provided
    let file_path = if let Some(url) = &request.url {
        if url.starts_with("file://") {
            // Local file
            std::path::PathBuf::from(&url[7..])
        } else {
            // Download from URL
            job.set_status(JobStatus::Downloading);
            download_manager
                .download(url, request.headers.as_ref(), None)
                .await?
        }
    } else {
        anyhow::bail!("No document URL provided");
    };

    job.file_path = Some(file_path.clone());

    // Detect content type if not provided
    let content_type = request
        .content_type
        .clone()
        .or_else(|| detect_content_type(&file_path));

    job.content_type = content_type.clone();

    // Print the document
    job.set_status(JobStatus::Printing);

    if is_raw || content_type.as_ref().map(|c| c.is_raw()).unwrap_or(false) {
        // Raw printing
        let data = tokio::fs::read(&file_path).await?;
        print_engine.print_raw(&data, &job.printer)?;
    } else {
        // Rendered document printing
        print_engine.print_document(&file_path, &job.printer, &job.options)?;
    }

    // Clean up downloaded file
    if !request.url.as_ref().map(|u| u.starts_with("file://")).unwrap_or(false) {
        let _ = download_manager.delete(&file_path).await;
    }

    Ok(())
}
