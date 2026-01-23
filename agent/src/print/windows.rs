use crate::models::*;
use super::PrintEngine;
use anyhow::{Result, Context};
use std::path::Path;
use std::process::Command;

#[cfg(windows)]
use windows::{
    core::{PCWSTR, PWSTR},
    Win32::Graphics::Printing::*,
    Win32::Foundation::*,
};

/// Windows print engine implementation
pub struct WindowsPrintEngine {
    sumatra_path: Option<std::path::PathBuf>,
}

impl WindowsPrintEngine {
    pub fn new() -> Result<Self> {
        let sumatra_path = Self::find_sumatra();
        Ok(Self { sumatra_path })
    }

    /// Find SumatraPDF executable
    fn find_sumatra() -> Option<std::path::PathBuf> {
        // Check bundled location first
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let bundled = exe_dir.join("SumatraPDF.exe");
                if bundled.exists() {
                    return Some(bundled);
                }
                // Also check binaries subfolder
                let bundled = exe_dir.join("binaries").join("SumatraPDF.exe");
                if bundled.exists() {
                    return Some(bundled);
                }
            }
        }

        // Check common installation paths
        let paths = [
            r"C:\Program Files\SumatraPDF\SumatraPDF.exe",
            r"C:\Program Files (x86)\SumatraPDF\SumatraPDF.exe",
        ];

        for path in paths {
            let p = std::path::PathBuf::from(path);
            if p.exists() {
                return Some(p);
            }
        }

        // Check PATH
        if let Ok(output) = Command::new("where").arg("SumatraPDF.exe").output() {
            if output.status.success() {
                if let Ok(path) = String::from_utf8(output.stdout) {
                    let path = path.lines().next().unwrap_or("").trim();
                    if !path.is_empty() {
                        return Some(std::path::PathBuf::from(path));
                    }
                }
            }
        }

        None
    }

    /// Print a document using SumatraPDF
    fn print_with_sumatra(&self, path: &Path, printer: &str, options: &PrintOptions) -> Result<()> {
        let sumatra = self.sumatra_path.as_ref()
            .ok_or_else(|| anyhow::anyhow!("SumatraPDF not found. Please install SumatraPDF or bundle it with the application."))?;

        let scale_setting = match options.scaling {
            ScalingMode::None => "noscale",
            ScalingMode::Fit => "fit",
            ScalingMode::Shrink => "shrink",
        };

        let color_setting = match options.color_mode {
            ColorMode::Color => "color",
            ColorMode::Monochrome | ColorMode::Grayscale => "monochrome",
        };

        let print_settings = format!("{},{}", scale_setting, color_setting);

        let mut cmd = Command::new(sumatra);
        cmd.args([
            "-print-to",
            printer,
            "-print-settings",
            &print_settings,
            "-silent",
            "-exit-when-done",
        ]);

        // Add copies if more than 1
        if options.copies > 1 {
            cmd.arg("-print-settings");
            cmd.arg(format!("{},{}x", print_settings, options.copies));
        }

        cmd.arg(path.to_string_lossy().as_ref());

        // Hide the console window on Windows
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let output = cmd.output()
            .context("Failed to execute SumatraPDF")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("SumatraPDF failed: {}", stderr);
        }

        Ok(())
    }

    /// Print an image file
    fn print_image(&self, path: &Path, printer: &str, _options: &PrintOptions) -> Result<()> {
        // Use Windows built-in printing via mspaint /pt
        let mut cmd = Command::new("mspaint");
        cmd.args([
            "/pt",
            &path.to_string_lossy(),
            printer,
        ]);

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let output = cmd.output()
            .context("Failed to execute mspaint for image printing")?;

        if !output.status.success() {
            // mspaint sometimes returns non-zero even on success, so we just log it
            log::warn!("mspaint returned non-zero status, but print may have succeeded");
        }

        Ok(())
    }
}

impl PrintEngine for WindowsPrintEngine {
    fn get_printers(&self) -> Result<Vec<PrinterInfo>> {
        let mut printers = Vec::new();

        // Use wmic to get printer list (more reliable than direct API for listing)
        let output = Command::new("wmic")
            .args(["printer", "get", "name,default,printerstatus", "/format:csv"])
            .output()
            .context("Failed to execute wmic")?;

        if !output.status.success() {
            anyhow::bail!("wmic command failed");
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        for line in stdout.lines().skip(1) {
            // Skip header
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() >= 4 {
                // Format: Node,Default,Name,PrinterStatus
                let is_default = parts[1].trim().eq_ignore_ascii_case("true");
                let name = parts[2].trim().to_string();
                let status_code: u32 = parts[3].trim().parse().unwrap_or(0);

                if name.is_empty() {
                    continue;
                }

                let status = match status_code {
                    0 => PrinterStatus::Ready, // Idle
                    1 => PrinterStatus::Busy,  // Paused
                    2 => PrinterStatus::Error, // Error
                    3 => PrinterStatus::Busy,  // Pending Deletion
                    4 => PrinterStatus::Error, // Paper Jam
                    5 => PrinterStatus::Error, // Paper Out
                    6 => PrinterStatus::Busy,  // Manual Feed
                    7 => PrinterStatus::Error, // Paper Problem
                    _ => PrinterStatus::Ready,
                };

                printers.push(PrinterInfo {
                    name,
                    status,
                    is_default,
                    capabilities: PrinterCapabilities::default(),
                });
            }
        }

        // If wmic fails or returns nothing, try PowerShell
        if printers.is_empty() {
            let output = Command::new("powershell")
                .args([
                    "-Command",
                    "Get-Printer | Select-Object Name, PrinterStatus, Type | ConvertTo-Json"
                ])
                .output();

            if let Ok(output) = output {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
                        let items = match &json {
                            serde_json::Value::Array(arr) => arr.clone(),
                            obj => vec![obj.clone()],
                        };

                        for item in items {
                            if let Some(name) = item.get("Name").and_then(|v| v.as_str()) {
                                printers.push(PrinterInfo {
                                    name: name.to_string(),
                                    status: PrinterStatus::Ready,
                                    is_default: false,
                                    capabilities: PrinterCapabilities::default(),
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(printers)
    }

    fn get_default_printer(&self) -> Result<Option<String>> {
        let printers = self.get_printers()?;
        Ok(printers.into_iter().find(|p| p.is_default).map(|p| p.name))
    }

    fn print_document(&self, path: &Path, printer: &str, options: &PrintOptions) -> Result<()> {
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        match ext.as_str() {
            "pdf" => self.print_with_sumatra(path, printer, options),
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "tiff" | "tif" => {
                self.print_image(path, printer, options)
            }
            _ => anyhow::bail!("Unsupported document type: {}", ext),
        }
    }

    fn print_raw(&self, data: &[u8], printer: &str) -> Result<()> {
        #[cfg(windows)]
        unsafe {
            use std::ffi::OsStr;
            use std::os::windows::ffi::OsStrExt;

            // Convert printer name to wide string
            let printer_wide: Vec<u16> = OsStr::new(printer)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();

            // Open printer
            let mut printer_handle = HANDLE::default();
            let printer_name = PCWSTR::from_raw(printer_wide.as_ptr());

            OpenPrinterW(
                printer_name,
                &mut printer_handle,
                None,
            ).map_err(|e| anyhow::anyhow!("Failed to open printer {}: {}", printer, e))?;

            // Set up document info
            let mut doc_name: Vec<u16> = OsStr::new("PrintBridge Raw Document")
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let mut data_type: Vec<u16> = OsStr::new("RAW")
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();

            let doc_info = DOC_INFO_1W {
                pDocName: PWSTR::from_raw(doc_name.as_mut_ptr()),
                pOutputFile: PWSTR::null(),
                pDatatype: PWSTR::from_raw(data_type.as_mut_ptr()),
            };

            // Start document
            let job_id = StartDocPrinterW(printer_handle, 1, &doc_info as *const _ as *const _);
            if job_id == 0 {
                let _ = ClosePrinter(printer_handle);
                anyhow::bail!("Failed to start print document");
            }

            // Start page
            if !StartPagePrinter(printer_handle).as_bool() {
                let _ = EndDocPrinter(printer_handle);
                let _ = ClosePrinter(printer_handle);
                anyhow::bail!("Failed to start print page");
            }

            // Write data
            let mut bytes_written: u32 = 0;
            if !WritePrinter(
                printer_handle,
                data.as_ptr() as *const _,
                data.len() as u32,
                &mut bytes_written,
            ).as_bool() {
                let _ = EndPagePrinter(printer_handle);
                let _ = EndDocPrinter(printer_handle);
                let _ = ClosePrinter(printer_handle);
                anyhow::bail!("Failed to write to printer");
            }

            // End page and document
            let _ = EndPagePrinter(printer_handle);
            let _ = EndDocPrinter(printer_handle);
            let _ = ClosePrinter(printer_handle);

            log::info!("Sent {} bytes to printer {}", bytes_written, printer);
            Ok(())
        }

        #[cfg(not(windows))]
        {
            let _ = (data, printer);
            anyhow::bail!("Raw printing not yet supported on this platform")
        }
    }

    fn printer_exists(&self, name: &str) -> Result<bool> {
        let printers = self.get_printers()?;
        Ok(printers.iter().any(|p| p.name.eq_ignore_ascii_case(name)))
    }
}
