use super::{
    backend::{print_settings, process_result, validate_media, BackendStatus, PreparedPrint,
        PrintDocument, PrintFailure, PrinterBackend, SUMATRA_VERSION},
    config::{LabelSettings, Profile},
    jobs::{DocumentSource, Jobs, Submit},
    pdf::{mm_to_points, validate},
};
use anyhow::Result;
use async_trait::async_trait;
use lopdf::{dictionary, Document, Object};
use reqwest::Url;
use serde_json::{json, Value};
use std::{
    path::Path,
    sync::{atomic::{AtomicUsize, Ordering}, Arc},
    time::Duration,
};
use tokio::{sync::Notify, time::{sleep, timeout}};

const TRUSTED: &str = "https://stockman.example.com";
const OTHER: &str = "https://other.example.com";
const DOCUMENT: &str = "https://labels.example.com/labels/test.pdf";

fn settings() -> Arc<LabelSettings> {
    Arc::new(serde_json::from_value(json!({
        "trustedOrigins": [TRUSTED, OTHER],
        "documentOrigins": ["https://labels.example.com"],
        "profiles": {
            "test-small": {"printer": "Test Queue", "widthMm": 68, "heightMm": 31},
            "test-large": {"printer": "Other Queue", "widthMm": 100, "heightMm": 45}
        }
    })).unwrap())
}

fn pdf(pages: &[(f64, f64)], rotation: i64, cropped: bool) -> Vec<u8> {
    let box_array = |(width, height)| vec![
        Object::Integer(0), Object::Integer(0), Object::Real(mm_to_points(width) as f32),
        Object::Real(mm_to_points(height) as f32),
    ];
    let mut document = Document::with_version("1.4");
    let parent = document.new_object_id();
    let mut page_ids = Vec::new();
    for (index, &size) in pages.iter().enumerate() {
        let mut page = dictionary! { "Type" => "Page", "Parent" => parent };
        if index != 0 {
            page.set("MediaBox", box_array(size));
        }
        if cropped {
            page.set("CropBox", box_array((size.0 - 2.0, size.1)));
        }
        page_ids.push(Object::Reference(document.add_object(page)));
    }
    document.objects.insert(parent, Object::Dictionary(dictionary! {
        "Type" => "Pages", "Kids" => page_ids, "Count" => pages.len() as i64,
        "MediaBox" => box_array(pages[0]), "Rotate" => rotation,
    }));
    let root = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => parent });
    document.trailer.set("Root", root);
    let mut output = Vec::new();
    document.save_to(&mut output).unwrap();
    output
}

#[test]
fn validates_inherited_geometry_of_every_page_without_resizing() {
    assert!((mm_to_points(25.4) - 72.0).abs() < 1e-10);
    assert!((mm_to_points(50.0) - 141.732_283_46).abs() < 1e-7);
    let settings = settings();
    let profile = settings.profile("test-small").unwrap();
    assert!(validate(&pdf(&[(68.0, 31.0), (68.04, 31.0)], 0, false), profile, 1).is_ok());
    let mismatch = validate(&pdf(&[(68.0, 31.0), (100.0, 45.0)], 0, false), profile, 1).unwrap_err();
    assert!(mismatch.to_string().starts_with("Page 2"));
    assert!(validate(&pdf(&[(31.0, 68.0)], 0, false), profile, 1).is_err());
    assert!(validate(&pdf(&[(68.0, 31.0)], 90, false), profile, 1).is_err());
    assert!(validate(&pdf(&[(68.0, 31.0)], 0, true), profile, 1).is_err());
    assert!(validate(b"<html>Not a label</html>", profile, 1).is_err());
}

#[test]
fn named_profiles_driver_media_and_sumatra_options() {
    let settings = settings();
    settings.validate().unwrap();
    assert!(settings.profile("../test-small").is_err());
    assert_eq!(settings.profile("test-large").unwrap().printer, "Other Queue");
    let mut profile = settings.profile("test-small").unwrap().clone();
    assert_eq!(profile.copies(None).unwrap(), 1);
    assert!(profile.copies(Some(0)).is_err());
    assert!(profile.copies(Some(21)).is_err());
    assert!(validate_media(&profile, 68.0, 31.2).is_ok());
    assert!(validate_media(&profile, 31.0, 68.0).is_err());
    assert!(validate_media(&profile, 210.0, 297.0).is_err());
    let plan = PreparedPrint { paper_kind: 260 };
    assert_eq!(print_settings(plan, &profile, 2), "noscale,portrait,paperkind=260,2x,disable-auto-rotation");
    profile.auto_rotate = true;
    assert!(validate_media(&profile, 31.0, 68.0).is_ok());
    assert_eq!(print_settings(plan, &profile, 1), "noscale,portrait,paperkind=260,1x");
}

#[test]
fn validates_configuration_and_requires_no_scaling() {
    let mut settings = settings().as_ref().clone();
    settings.trusted_origins.push("*".into());
    assert!(settings.validate().is_err());
    settings.trusted_origins.pop();
    settings.document_origins.push("https://127.0.0.1".into());
    assert!(settings.validate().is_err());
    assert!(serde_json::from_value::<Profile>(json!({
        "printer": "Test Queue", "widthMm": 68, "heightMm": 31, "scale": "fit"
    })).is_err());
}

#[test]
fn process_failures_and_temporary_document_lifetime() {
    let directory = tempfile::tempdir().unwrap();
    let document = PrintDocument::new(directory.path(), b"synthetic PDF bytes").unwrap();
    assert_eq!(std::fs::read(document.pdf()).unwrap(), b"synthetic PDF bytes");
    assert!(process_result(Some(0), b"", document.app_data()).is_ok());
    let stderr = format!("cannot open {}\n", document.pdf().display());
    let error = process_result(Some(7), stderr.as_bytes(), document.app_data()).unwrap_err();
    assert_eq!(error.exit_code, Some(7));
    assert!(error.may_have_printed && error.message.contains("<job>"));
    let pdf = document.pdf().to_owned();
    drop(document);
    assert!(!pdf.exists());
}

struct TestDocuments(Vec<u8>);

#[async_trait]
impl DocumentSource for TestDocuments {
    async fn fetch(&self, _: Url, _: &[String]) -> Result<Vec<u8>> {
        Ok(self.0.clone())
    }
}

#[derive(Default)]
struct TestPrinter {
    prints: AtomicUsize,
    pause: bool,
    fails: bool,
    entered: Notify,
    proceed: Notify,
}

#[async_trait]
impl PrinterBackend for TestPrinter {
    async fn printers(&self) -> Result<Vec<String>> {
        Ok(vec!["Test Queue".into()])
    }

    async fn status(&self) -> BackendStatus {
        BackendStatus { available: true, expected_version: SUMATRA_VERSION, error: None }
    }

    async fn prepare(&self, _: &Profile, _: u16) -> std::result::Result<PreparedPrint, PrintFailure> {
        if self.pause {
            self.entered.notify_one();
            self.proceed.notified().await;
        }
        Ok(PreparedPrint { paper_kind: 260 })
    }

    async fn print(&self, _: PreparedPrint, _: &Profile, _: u16, file: &Path, data: &Path)
        -> std::result::Result<(), PrintFailure> {
        assert!(file.is_file());
        self.prints.fetch_add(1, Ordering::SeqCst);
        process_result(Some(if self.fails { 7 } else { 0 }), b"", data)
    }
}

fn request() -> Submit {
    Submit { url: DOCUMENT.into(), profile: "test-small".into(), copies: Some(1) }
}

fn jobs(root: &Path, printer: Arc<TestPrinter>, bytes: Vec<u8>) -> Jobs {
    Jobs::open(root, settings(), printer, Arc::new(TestDocuments(bytes))).unwrap()
}

fn view(jobs: &Jobs, id: &str) -> Value {
    json!(jobs.status(TRUSTED, id).unwrap())
}

async fn wait_status(jobs: &Jobs, id: &str, expected: &str) -> Value {
    timeout(Duration::from_secs(3), async {
        loop {
            let job = view(jobs, id);
            if job["status"] == expected { break job; }
            sleep(Duration::from_millis(5)).await;
        }
    }).await.unwrap()
}

#[tokio::test]
async fn validates_submission_and_deduplicates_across_worker_restarts() {
    let root = tempfile::tempdir().unwrap();
    let printer = Arc::new(TestPrinter::default());
    let bytes = pdf(&[(68.0, 31.0)], 0, false);
    let first = jobs(root.path(), printer.clone(), bytes.clone());
    assert_eq!(first.submit(TRUSTED, "/bad", request()).unwrap_err().code, "INVALID_REQUEST_ID");
    assert_eq!(first.submit(TRUSTED, "bad-profile", Submit { profile: "missing".into(), ..request() })
        .unwrap_err().code, "INVALID_PROFILE");
    assert_eq!(first.submit(TRUSTED, "bad-copies", Submit { copies: Some(21), ..request() })
        .unwrap_err().code, "INVALID_COPIES");
    assert_eq!(first.submit(TRUSTED, "blocked-url", Submit { url: "https://10.0.0.1/label".into(), ..request() })
        .unwrap_err().code, "BLOCKED_URL");
    assert_eq!(json!(first.submit(TRUSTED, "same", request()).unwrap())["status"], "queued");
    assert_eq!(first.status(OTHER, "same").unwrap_err().code, "JOB_NOT_FOUND");
    assert_eq!(first.submit(TRUSTED, "same", Submit { copies: Some(2), ..request() })
        .unwrap_err().code, "REQUEST_ID_CONFLICT");
    first.submit(TRUSTED, "same", request()).unwrap();
    assert_eq!(wait_status(&first, "same", "sumatraAccepted").await["exitCode"], 0);
    drop(first);
    let restarted = jobs(root.path(), printer.clone(), bytes);
    assert_eq!(json!(restarted.submit(TRUSTED, "same", request()).unwrap())["status"], "sumatraAccepted");
    assert_eq!(printer.prints.load(Ordering::SeqCst), 1);
    assert_eq!(std::fs::read_dir(root.path().join("print-tmp")).unwrap().count(), 0);
}

#[tokio::test]
async fn cancel_while_preparing_prevents_dispatch_and_cleans_up() {
    let root = tempfile::tempdir().unwrap();
    let printer = Arc::new(TestPrinter { pause: true, ..Default::default() });
    let jobs = jobs(root.path(), printer.clone(), pdf(&[(68.0, 31.0)], 0, false));
    jobs.submit(TRUSTED, "cancel", request()).unwrap();
    timeout(Duration::from_secs(3), printer.entered.notified()).await.unwrap();
    assert_eq!(view(&jobs, "cancel")["status"], "preparing");
    assert_eq!(json!(jobs.cancel(TRUSTED, "cancel").unwrap())["status"], "cancelled");
    printer.proceed.notify_one();
    timeout(Duration::from_secs(3), async {
        while std::fs::read_dir(root.path().join("print-tmp")).unwrap().count() != 0 {
            sleep(Duration::from_millis(5)).await;
        }
    }).await.unwrap();
    assert_eq!(printer.prints.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn reports_preflight_and_process_failures_without_reprinting() {
    let root = tempfile::tempdir().unwrap();
    let printer = Arc::new(TestPrinter { fails: true, ..Default::default() });
    let wrong = jobs(root.path(), printer.clone(), pdf(&[(100.0, 45.0)], 0, false));
    wrong.submit(TRUSTED, "wrong", request()).unwrap();
    let failed = wait_status(&wrong, "wrong", "failed").await;
    assert_eq!(failed["errorCode"], "INVALID_PDF");
    assert_eq!(failed["mayHavePrinted"], false);
    assert_eq!(printer.prints.load(Ordering::SeqCst), 0);
    drop(wrong);
    let jobs = jobs(root.path(), printer.clone(), pdf(&[(68.0, 31.0)], 0, false));
    jobs.submit(TRUSTED, "fail", request()).unwrap();
    let failed = wait_status(&jobs, "fail", "failed").await;
    assert_eq!(failed["errorCode"], "SUMATRA_FAILED");
    assert_eq!(failed["exitCode"], 7);
    assert_eq!(failed["mayHavePrinted"], true);
    assert_eq!(jobs.cancel(TRUSTED, "fail").unwrap_err().code, "TOO_LATE_TO_CANCEL");
    assert_eq!(printer.prints.load(Ordering::SeqCst), 1);
    assert_eq!(std::fs::read_dir(root.path().join("print-tmp")).unwrap().count(), 0);
}

#[tokio::test]
async fn interrupted_dispatch_is_not_requeued() {
    let root = tempfile::tempdir().unwrap();
    let printer = Arc::new(TestPrinter::default());
    let initial = jobs(root.path(), printer.clone(), pdf(&[(68.0, 31.0)], 0, false));
    initial.submit(TRUSTED, "restart", request()).unwrap();
    wait_status(&initial, "restart", "sumatraAccepted").await;
    drop(initial);
    let connection = rusqlite::Connection::open(root.path().join("label-jobs.sqlite3")).unwrap();
    connection.execute("UPDATE label_jobs SET state = 'printing', exit_code = NULL WHERE id = 'restart'", []).unwrap();
    drop(connection);
    let restarted = jobs(root.path(), printer.clone(), pdf(&[(68.0, 31.0)], 0, false));
    let interrupted = json!(restarted.submit(TRUSTED, "restart", request()).unwrap());
    assert_eq!(interrupted["status"], "interrupted");
    assert_eq!(interrupted["mayHavePrinted"], true);
    assert_eq!(printer.prints.load(Ordering::SeqCst), 1);
}
