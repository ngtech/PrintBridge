use serde::{Deserialize, Serialize};
use crate::models::*;

/// Client to agent messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ClientMessage {
    /// Request agent status
    #[serde(rename = "getStatus")]
    GetStatus { id: String },

    /// Request list of printers
    #[serde(rename = "getPrinters")]
    GetPrinters { id: String },

    /// Submit a print job
    #[serde(rename = "print")]
    Print { id: String, request: PrintRequest },

    /// Submit a raw print job
    #[serde(rename = "printRaw")]
    PrintRaw { id: String, request: PrintRequest },

    /// Submit a batch of print jobs
    #[serde(rename = "printBatch")]
    PrintBatch { id: String, request: BatchRequest },

    /// Cancel a job
    #[serde(rename = "cancelJob")]
    CancelJob { id: String, job_id: String },

    /// Cancel a batch
    #[serde(rename = "cancelBatch")]
    CancelBatch { id: String, batch_id: String },

    /// Chunk upload start
    #[serde(rename = "uploadStart")]
    UploadStart {
        id: String,
        upload_id: String,
        filename: String,
        total_size: u64,
        content_type: Option<ContentType>,
    },

    /// Chunk upload data
    #[serde(rename = "uploadChunk")]
    UploadChunk {
        id: String,
        upload_id: String,
        chunk_index: u32,
        data: String, // Base64 encoded
    },

    /// Chunk upload complete
    #[serde(rename = "uploadComplete")]
    UploadComplete {
        id: String,
        upload_id: String,
        request: PrintRequest,
    },

    /// Ping for keepalive
    #[serde(rename = "ping")]
    Ping { id: String },
}

/// Agent to client messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentMessage {
    /// Welcome message on connection
    #[serde(rename = "welcome")]
    Welcome { version: String },

    /// Response to getStatus
    #[serde(rename = "status")]
    Status { id: String, status: AgentStatus },

    /// Response to getPrinters
    #[serde(rename = "printers")]
    Printers { id: String, printers: Vec<PrinterInfo> },

    /// Job accepted
    #[serde(rename = "jobAccepted")]
    JobAccepted { id: String, job: PrintJob },

    /// Job status update
    #[serde(rename = "jobStatus")]
    JobStatus { id: String, job: PrintJob },

    /// Job completed
    #[serde(rename = "jobCompleted")]
    JobCompleted { id: String, job: PrintJob },

    /// Job failed
    #[serde(rename = "jobFailed")]
    JobFailed {
        id: String,
        job: PrintJob,
        error: String,
    },

    /// Batch accepted
    #[serde(rename = "batchAccepted")]
    BatchAccepted { id: String, batch_id: String, total: usize },

    /// Batch progress update
    #[serde(rename = "batchProgress")]
    BatchProgress { id: String, progress: BatchProgress },

    /// Batch completed
    #[serde(rename = "batchCompleted")]
    BatchCompleted { id: String, result: BatchResult },

    /// Upload progress
    #[serde(rename = "uploadProgress")]
    UploadProgress {
        id: String,
        upload_id: String,
        received: u64,
        total: u64,
    },

    /// Upload ready (file saved, ready for print request)
    #[serde(rename = "uploadReady")]
    UploadReady { id: String, upload_id: String },

    /// Pong response
    #[serde(rename = "pong")]
    Pong { id: String },

    /// Error response
    #[serde(rename = "error")]
    Error {
        id: String,
        code: String,
        message: String,
    },
}

impl AgentMessage {
    pub fn error(id: String, code: &str, message: &str) -> Self {
        Self::Error {
            id,
            code: code.to_string(),
            message: message.to_string(),
        }
    }
}

/// Error codes
pub mod error_codes {
    pub const INVALID_REQUEST: &str = "INVALID_REQUEST";
    pub const PRINTER_NOT_FOUND: &str = "PRINTER_NOT_FOUND";
    pub const DOWNLOAD_FAILED: &str = "DOWNLOAD_FAILED";
    pub const PRINT_FAILED: &str = "PRINT_FAILED";
    pub const JOB_NOT_FOUND: &str = "JOB_NOT_FOUND";
    pub const BATCH_NOT_FOUND: &str = "BATCH_NOT_FOUND";
    pub const UPLOAD_FAILED: &str = "UPLOAD_FAILED";
    pub const UNSUPPORTED_FORMAT: &str = "UNSUPPORTED_FORMAT";
    pub const INTERNAL_ERROR: &str = "INTERNAL_ERROR";
}
