use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// Print job status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Pending,
    Downloading,
    Printing,
    Completed,
    Failed,
    Cancelled,
}

/// Document content type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentType {
    #[serde(rename = "application/pdf")]
    Pdf,
    #[serde(rename = "image/png")]
    Png,
    #[serde(rename = "image/jpeg")]
    Jpeg,
    #[serde(rename = "image/gif")]
    Gif,
    #[serde(rename = "image/bmp")]
    Bmp,
    #[serde(rename = "image/tiff")]
    Tiff,
    #[serde(rename = "application/x-escpos")]
    EscPos,
    #[serde(rename = "application/x-zpl")]
    Zpl,
    #[serde(rename = "application/x-epl")]
    Epl,
    #[serde(rename = "application/octet-stream")]
    Raw,
}

impl ContentType {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "pdf" => Some(Self::Pdf),
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "gif" => Some(Self::Gif),
            "bmp" => Some(Self::Bmp),
            "tif" | "tiff" => Some(Self::Tiff),
            "zpl" => Some(Self::Zpl),
            "epl" => Some(Self::Epl),
            _ => None,
        }
    }

    pub fn from_mime(mime: &str) -> Option<Self> {
        match mime.to_lowercase().as_str() {
            "application/pdf" => Some(Self::Pdf),
            "image/png" => Some(Self::Png),
            "image/jpeg" => Some(Self::Jpeg),
            "image/gif" => Some(Self::Gif),
            "image/bmp" => Some(Self::Bmp),
            "image/tiff" => Some(Self::Tiff),
            "application/x-escpos" => Some(Self::EscPos),
            "application/x-zpl" => Some(Self::Zpl),
            "application/x-epl" => Some(Self::Epl),
            "application/octet-stream" => Some(Self::Raw),
            _ => None,
        }
    }

    pub fn is_raw(&self) -> bool {
        matches!(self, Self::EscPos | Self::Zpl | Self::Epl | Self::Raw)
    }

    pub fn is_image(&self) -> bool {
        matches!(self, Self::Png | Self::Jpeg | Self::Gif | Self::Bmp | Self::Tiff)
    }
}

/// Scaling mode for printing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ScalingMode {
    Fit,
    #[default]
    Shrink,
    None,
}

/// Color mode for printing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ColorMode {
    #[default]
    Color,
    Monochrome,
    Grayscale,
}

/// Duplex mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DuplexMode {
    #[default]
    None,
    Long,
    Short,
}

/// Print options
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintOptions {
    #[serde(default)]
    pub scaling: ScalingMode,
    #[serde(default)]
    pub color_mode: ColorMode,
    #[serde(default = "default_copies")]
    pub copies: u32,
    pub paper_size: Option<String>,
    pub orientation: Option<String>,
    #[serde(default)]
    pub duplex: DuplexMode,
}

fn default_copies() -> u32 {
    1
}

impl Default for PrintOptions {
    fn default() -> Self {
        Self {
            scaling: ScalingMode::default(),
            color_mode: ColorMode::default(),
            copies: 1,
            paper_size: None,
            orientation: None,
            duplex: DuplexMode::default(),
        }
    }
}

/// Print job request from client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintRequest {
    pub id: Option<String>,
    pub url: Option<String>,
    pub headers: Option<std::collections::HashMap<String, String>>,
    pub data: Option<String>, // Base64 encoded for chunked upload
    pub printer: Option<String>,
    pub content_type: Option<ContentType>,
    #[serde(default)]
    pub options: PrintOptions,
}

/// Print job
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintJob {
    pub id: String,
    pub status: JobStatus,
    pub printer: String,
    pub content_type: Option<ContentType>,
    pub options: PrintOptions,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip)]
    pub file_path: Option<std::path::PathBuf>,
    #[serde(skip)]
    pub url: Option<String>,
    #[serde(skip)]
    pub headers: Option<std::collections::HashMap<String, String>>,
}

impl PrintJob {
    pub fn new(request: PrintRequest, default_printer: &str) -> Self {
        let now = Utc::now();
        Self {
            id: request.id.unwrap_or_else(|| Uuid::new_v4().to_string()),
            status: JobStatus::Pending,
            printer: request.printer.unwrap_or_else(|| default_printer.to_string()),
            content_type: request.content_type,
            options: request.options,
            error: None,
            created_at: now,
            updated_at: now,
            file_path: None,
            url: request.url,
            headers: request.headers,
        }
    }

    pub fn set_status(&mut self, status: JobStatus) {
        self.status = status;
        self.updated_at = Utc::now();
    }

    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
        self.status = JobStatus::Failed;
        self.updated_at = Utc::now();
    }
}

/// Printer information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrinterInfo {
    pub name: String,
    pub status: PrinterStatus,
    pub is_default: bool,
    pub capabilities: PrinterCapabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrinterStatus {
    Ready,
    Offline,
    Error,
    Busy,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PrinterCapabilities {
    pub color: bool,
    pub duplex: bool,
    pub paper_sizes: Vec<String>,
}

/// Agent status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatus {
    pub version: String,
    pub platform: String,
    pub printer_count: usize,
    pub queue_length: usize,
    pub uptime: u64,
}

/// Batch print request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRequest {
    pub id: String,
    pub jobs: Vec<PrintRequest>,
    #[serde(default)]
    pub on_error: BatchErrorMode,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_retry_delay")]
    pub retry_delay: u64,
}

fn default_max_retries() -> u32 {
    3
}

fn default_retry_delay() -> u64 {
    1000
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BatchErrorMode {
    #[default]
    Stop,
    Continue,
    Retry,
}

/// Batch progress update
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchProgress {
    pub batch_id: String,
    pub completed: usize,
    pub failed: usize,
    pub total: usize,
    pub current_job: Option<PrintJob>,
}

/// Batch result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchResult {
    pub batch_id: String,
    pub completed: usize,
    pub failed: usize,
    pub cancelled: usize,
    pub total: usize,
    pub jobs: Vec<PrintJob>,
}
