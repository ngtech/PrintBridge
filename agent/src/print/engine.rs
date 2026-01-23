use crate::models::*;
use anyhow::Result;
use std::path::Path;

#[cfg(windows)]
use super::windows::WindowsPrintEngine;

/// Print engine trait for cross-platform abstraction
pub trait PrintEngine: Send + Sync {
    /// Get list of available printers
    fn get_printers(&self) -> Result<Vec<PrinterInfo>>;

    /// Get the default printer name
    fn get_default_printer(&self) -> Result<Option<String>>;

    /// Print a rendered document (PDF, image) via the print spooler
    fn print_document(&self, path: &Path, printer: &str, options: &PrintOptions) -> Result<()>;

    /// Print raw data (ESC/POS, ZPL, EPL) directly to printer
    fn print_raw(&self, data: &[u8], printer: &str) -> Result<()>;

    /// Check if a printer exists
    fn printer_exists(&self, name: &str) -> Result<bool>;
}

/// Create the platform-specific print engine
pub fn create_print_engine() -> Result<Box<dyn PrintEngine>> {
    #[cfg(windows)]
    {
        Ok(Box::new(WindowsPrintEngine::new()?))
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("Printing not yet supported on this platform")
    }
}

/// Get file extension from path
pub fn get_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
}

/// Detect content type from file path
pub fn detect_content_type(path: &Path) -> Option<ContentType> {
    get_extension(path).and_then(|ext| ContentType::from_extension(&ext))
}
