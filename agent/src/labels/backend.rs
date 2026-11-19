use super::config::Profile;
use anyhow::{ensure, Result};
use async_trait::async_trait;
use serde::Serialize;
use std::path::{Path, PathBuf};

pub const SUMATRA_VERSION: &str = "3.6.1";
const MEDIA_TOLERANCE_MM: f64 = 0.30;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendStatus {
    pub available: bool,
    pub expected_version: &'static str,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintFailure {
    pub code: &'static str,
    pub message: String,
    pub exit_code: Option<i32>,
    pub may_have_printed: bool,
}

impl PrintFailure {
    pub fn before_print(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), exit_code: None, may_have_printed: false }
    }
}

#[derive(Clone, Copy)]
pub struct PreparedPrint {
    pub paper_kind: i16,
}

#[async_trait]
pub trait PrinterBackend: Send + Sync + 'static {
    async fn printers(&self) -> Result<Vec<String>>;
    async fn status(&self) -> BackendStatus;
    async fn prepare(&self, profile: &Profile, copies: u16) -> std::result::Result<PreparedPrint, PrintFailure>;
    async fn print(&self, plan: PreparedPrint, profile: &Profile, copies: u16, pdf: &Path, app_data: &Path)
        -> std::result::Result<(), PrintFailure>;
}

pub fn validate_media(profile: &Profile, width: f64, height: f64) -> Result<()> {
    let (expected_width, expected_height) = profile.expected_media();
    ensure!((width - expected_width).abs() <= MEDIA_TOLERANCE_MM
        && (height - expected_height).abs() <= MEDIA_TOLERANCE_MM,
        "Driver reports {:.2} × {:.2} mm; profile requires {:.2} × {:.2} mm in this orientation (±{MEDIA_TOLERANCE_MM:.2} mm)",
        width, height, expected_width, expected_height);
    Ok(())
}

pub fn print_settings(plan: PreparedPrint, profile: &Profile, copies: u16) -> String {
    let rotation = if profile.auto_rotate { "" } else { ",disable-auto-rotation" };
    format!("noscale,portrait,paperkind={},{}x{rotation}", plan.paper_kind, copies)
}

pub struct PrintDocument {
    directory: tempfile::TempDir,
    pdf: PathBuf,
}

impl PrintDocument {
    pub fn new(parent: &Path, bytes: &[u8]) -> Result<Self> {
        use std::io::Write;
        let directory = tempfile::Builder::new().prefix("label-").tempdir_in(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))?;
        }
        let pdf = directory.path().join("label.pdf");
        let mut file = std::fs::OpenOptions::new().create_new(true).write(true).open(&pdf)?;
        file.write_all(bytes)?;
        file.flush()?;
        Ok(Self { directory, pdf })
    }

    pub fn pdf(&self) -> &Path {
        &self.pdf
    }

    pub fn app_data(&self) -> &Path {
        self.directory.path()
    }
}

pub fn process_result(code: Option<i32>, stderr: &[u8], job_directory: &Path)
    -> std::result::Result<(), PrintFailure> {
    if code == Some(0) {
        return Ok(());
    }
    let location = job_directory.to_string_lossy();
    let diagnostic = String::from_utf8_lossy(stderr).replace(location.as_ref(), "<job>");
    let diagnostic: String = diagnostic.chars().filter(|character| !character.is_control()).take(240).collect();
    let exit = code.map_or_else(|| "with no exit code".to_owned(), |code| format!("with exit code {code}"));
    let message = if diagnostic.trim().is_empty() {
        format!("SumatraPDF exited {exit}; check the Windows queue before reprinting")
    } else {
        format!("SumatraPDF exited {exit}: {diagnostic}; check the Windows queue before reprinting")
    };
    Err(PrintFailure { code: "SUMATRA_FAILED", message, exit_code: code, may_have_printed: true })
}

#[cfg(not(windows))]
pub struct UnsupportedBackend;

#[cfg(not(windows))]
#[async_trait]
impl PrinterBackend for UnsupportedBackend {
    async fn printers(&self) -> Result<Vec<String>> {
        anyhow::bail!("Printer discovery requires Windows")
    }

    async fn status(&self) -> BackendStatus {
        BackendStatus { available: false, expected_version: SUMATRA_VERSION, error: Some("Printing requires Windows".into()) }
    }

    async fn prepare(&self, _: &Profile, _: u16) -> std::result::Result<PreparedPrint, PrintFailure> {
        Err(PrintFailure::before_print("UNSUPPORTED_PLATFORM", "Printing requires Windows"))
    }

    async fn print(&self, _: PreparedPrint, _: &Profile, _: u16, _: &Path, _: &Path) -> std::result::Result<(), PrintFailure> {
        Err(PrintFailure::before_print("UNSUPPORTED_PLATFORM", "Printing requires Windows"))
    }
}

pub fn platform_backend() -> std::sync::Arc<dyn PrinterBackend> {
    #[cfg(windows)]
    return std::sync::Arc::new(super::windows::WindowsBackend::new());
    #[cfg(not(windows))]
    return std::sync::Arc::new(UnsupportedBackend);
}
