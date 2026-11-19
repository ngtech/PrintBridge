use super::{
    backend::{print_settings, process_result, validate_media, BackendStatus, PreparedPrint, PrintFailure,
        PrinterBackend, SUMATRA_VERSION},
    config::Profile,
};
use anyhow::{ensure, Context, Result};
use async_trait::async_trait;
use std::{
    ffi::OsStr, mem::size_of, os::windows::ffi::OsStrExt, path::{Path, PathBuf},
    process::Stdio, ptr::{null, null_mut}, slice, sync::Arc, time::Duration,
};
use tokio::io::AsyncReadExt;
use windows_sys::Win32::{
    Foundation::{GetLastError, ERROR_INSUFFICIENT_BUFFER, HANDLE},
    Graphics::{
        Gdi::{CreateDCW, DeleteDC, GetDeviceCaps, DEVMODEW, DM_COPIES, DM_IN_BUFFER, DM_OUT_BUFFER,
            DM_PAPERSIZE, DM_SCALE, LOGPIXELSX, LOGPIXELSY, PHYSICALHEIGHT, PHYSICALWIDTH, HORZRES, VERTRES},
        Printing::{ClosePrinter, DocumentPropertiesW, EnumPrintersW, GetPrinterW, OpenPrinterW, PRINTER_INFO_2W,
            PRINTER_INFO_4W, PRINTER_ENUM_CONNECTIONS, PRINTER_ENUM_LOCAL},
    },
    Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO},
};

pub struct WindowsBackend {
    driver_calls: Arc<tokio::sync::Semaphore>,
}

impl WindowsBackend {
    pub fn new() -> Self {
        Self { driver_calls: Arc::new(tokio::sync::Semaphore::new(2)) }
    }

    async fn driver_call<Output: Send + 'static>(&self, call: impl FnOnce() -> Output + Send + 'static)
        -> Result<Output> {
        let permit = self.driver_calls.clone().try_acquire_owned()
            .context("Windows printer driver is busy; check the spooler")?;
        let task = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            call()
        });
        Ok(tokio::time::timeout(Duration::from_secs(15), task).await
            .context("Windows printer driver did not respond in 15 seconds")??)
    }
}

fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain(std::iter::once(0)).collect()
}

fn buffer(bytes: u32) -> Result<Vec<usize>> {
    ensure!(bytes > 0 && bytes <= 16 * 1024 * 1024, "Windows returned an unsupported buffer size");
    Ok(vec![0; (bytes as usize).div_ceil(size_of::<usize>())])
}

fn buffer_string(buffer: &[usize], string: *const u16) -> Result<String> {
    let start = buffer.as_ptr() as usize;
    let bytes = std::mem::size_of_val(buffer);
    let offset = (string as usize).checked_sub(start).context("Invalid Windows string pointer")?;
    ensure!(offset < bytes && offset % 2 == 0, "Invalid Windows string pointer");
    let data = unsafe { slice::from_raw_parts(string, (bytes - offset) / 2) };
    let end = data.iter().position(|unit| *unit == 0).context("Unterminated Windows string")?;
    String::from_utf16(&data[..end]).context("Invalid Windows queue name or port")
}

fn enumerate() -> Result<Vec<String>> {
    let (mut needed, mut count) = (0, 0);
    let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
    let sizing = unsafe { EnumPrintersW(flags, null(), 4, null_mut(), 0, &mut needed, &mut count) };
    if sizing != 0 && count == 0 {
        return Ok(Vec::new());
    }
    ensure!(sizing == 0 && unsafe { GetLastError() } == ERROR_INSUFFICIENT_BUFFER,
        "Cannot enumerate Windows printer queues");
    let capacity = needed;
    let mut data = buffer(capacity)?;
    ensure!(unsafe { EnumPrintersW(flags, null(), 4, data.as_mut_ptr().cast(), capacity, &mut needed, &mut count) } != 0,
        "Cannot enumerate Windows printer queues");
    ensure!(count as usize <= capacity as usize / size_of::<PRINTER_INFO_4W>(), "Invalid Windows queue list");
    let entries = unsafe { slice::from_raw_parts(data.as_ptr().cast::<PRINTER_INFO_4W>(), count as usize) };
    let mut names = entries.iter().map(|item| buffer_string(&data, item.pPrinterName)).collect::<Result<Vec<_>>>()?;
    names.sort();
    names.dedup();
    Ok(names)
}

struct Printer(HANDLE);

impl Drop for Printer {
    fn drop(&mut self) {
        unsafe { ClosePrinter(self.0) };
    }
}

fn check_physical_port(printer: &Printer) -> Result<()> {
    let mut needed = 0;
    unsafe { GetPrinterW(printer.0, 2, null_mut(), 0, &mut needed) };
    let capacity = needed;
    let mut data = buffer(capacity)?;
    ensure!(capacity as usize >= size_of::<PRINTER_INFO_2W>()
        && unsafe { GetPrinterW(printer.0, 2, data.as_mut_ptr().cast(), capacity, &mut needed) } != 0,
        "Cannot read printer port configuration");
    let info = unsafe { &*data.as_ptr().cast::<PRINTER_INFO_2W>() };
    let ports = buffer_string(&data, info.pPortName)?;
    ensure!(ports.split(',').all(|port| !port.trim().is_empty()
        && !matches!(port.trim().to_ascii_uppercase().as_str(), "FILE:" | "PORTPROMPT:" | "NUL:" | "FAX:")),
        "Select a physical queue; interactive/file-output ports are not supported");
    Ok(())
}

fn media(profile: &Profile, copies: u16) -> Result<PreparedPrint> {
    let name = wide(&profile.printer);
    let mut handle = null_mut();
    ensure!(unsafe { OpenPrinterW(name.as_ptr(), &mut handle, null()) } != 0 && !handle.is_null(),
        "Cannot open the configured queue");
    let printer = Printer(handle);
    check_physical_port(&printer)?;
    let size = unsafe { DocumentPropertiesW(null_mut(), printer.0, name.as_ptr(), null_mut(), null(), 0) };
    ensure!(size >= size_of::<DEVMODEW>() as i32, "Driver did not return printer defaults");
    let mut data = buffer(size as u32)?;
    let devmode = data.as_mut_ptr().cast::<DEVMODEW>();
    ensure!(unsafe { DocumentPropertiesW(null_mut(), printer.0, name.as_ptr(), devmode, null(), DM_OUT_BUFFER) } == 1,
        "Cannot read printer defaults; configure the queue under this Windows user");
    let paper_kind = unsafe {
        let settings = &mut *devmode;
        ensure!(usize::from(settings.dmSize) >= size_of::<DEVMODEW>(), "Unsupported printer defaults format");
        let paper_kind = profile.paper_kind.or_else(|| {
            (settings.dmFields & DM_PAPERSIZE != 0).then_some(settings.Anonymous1.Anonymous1.dmPaperSize)
        }).filter(|kind| *kind > 0)
            .context("Driver did not report a numeric media ID; select a named custom layout or set paperKind")?;
        settings.dmFields |= DM_PAPERSIZE | DM_COPIES;
        settings.Anonymous1.Anonymous1.dmPaperSize = paper_kind;
        settings.Anonymous1.Anonymous1.dmCopies = copies as i16;
        paper_kind
    };
    ensure!(unsafe {
        DocumentPropertiesW(null_mut(), printer.0, name.as_ptr(), devmode, devmode, DM_IN_BUFFER | DM_OUT_BUFFER)
    } == 1, "Driver did not accept the requested media/copies");
    unsafe {
        let settings = &*devmode;
        let print = settings.Anonymous1.Anonymous1;
        ensure!(settings.dmFields & DM_PAPERSIZE != 0 && print.dmPaperSize == paper_kind,
            "Driver did not retain the requested paperKind");
        ensure!(copies == 1 || (settings.dmFields & DM_COPIES != 0 && print.dmCopies == copies as i16),
            "Driver does not support the requested copy count");
        ensure!(settings.dmFields & DM_COPIES == 0 || print.dmCopies == copies as i16,
            "Driver changed the requested copy count");
        ensure!(settings.dmFields & DM_SCALE == 0 || print.dmScale == 100,
            "Disable scaling in Windows Printing Preferences");
    }
    let dc = unsafe { CreateDCW(null(), name.as_ptr(), null(), devmode) };
    ensure!(!dc.is_null(), "Cannot query the queue's effective media");
    let (dpi_x, dpi_y, width, height, printable_x, printable_y) = unsafe {
        let values = (GetDeviceCaps(dc, LOGPIXELSX as i32), GetDeviceCaps(dc, LOGPIXELSY as i32),
            GetDeviceCaps(dc, PHYSICALWIDTH as i32), GetDeviceCaps(dc, PHYSICALHEIGHT as i32),
            GetDeviceCaps(dc, HORZRES as i32), GetDeviceCaps(dc, VERTRES as i32));
        DeleteDC(dc);
        values
    };
    ensure!(dpi_x > 0 && dpi_x == dpi_y && width > 0 && height > 0 && printable_x > 0 && printable_y > 0,
        "Driver must report a valid media size and equal horizontal/vertical DPI");
    validate_media(profile, f64::from(width) * 25.4 / f64::from(dpi_x),
        f64::from(height) * 25.4 / f64::from(dpi_y))?;
    Ok(PreparedPrint { paper_kind })
}

fn version_matches(path: &Path) -> bool {
    let name = wide(path.as_os_str());
    let mut handle = 0;
    let size = unsafe { GetFileVersionInfoSizeW(name.as_ptr(), &mut handle) };
    let Ok(mut data) = buffer(size) else { return false };
    let mut value = null_mut();
    let mut length = 0;
    let root = wide("\\");
    unsafe {
        if GetFileVersionInfoW(name.as_ptr(), 0, size, data.as_mut_ptr().cast()) == 0
            || VerQueryValueW(data.as_ptr().cast(), root.as_ptr(), &mut value, &mut length) == 0
            || value.is_null() || (length as usize) < size_of::<VS_FIXEDFILEINFO>() {
            return false;
        }
        let version = value.cast::<VS_FIXEDFILEINFO>().read_unaligned();
        version.dwSignature == 0xfeef04bd && version.dwFileVersionMS == (3 << 16 | 6)
            && version.dwFileVersionLS == 1 << 16
    }
}

fn sumatra() -> std::result::Result<PathBuf, PrintFailure> {
    let mut candidates = Vec::new();
    if let Ok(executable) = std::env::current_exe() {
        if let Some(directory) = executable.parent() {
            candidates.push(directory.join("SumatraPDF.exe"));
        }
    }
    for name in ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
        if let Some(directory) = std::env::var_os(name) {
            candidates.push(PathBuf::from(directory).join("SumatraPDF").join("SumatraPDF.exe"));
        }
    }
    let mut other_version = false;
    for executable in candidates {
        if executable.is_absolute() && executable.is_file() {
            if version_matches(&executable) {
                return Ok(executable);
            }
            other_version = true;
        }
    }
    Err(PrintFailure::before_print(
        if other_version { "SUMATRA_VERSION" } else { "SUMATRA_NOT_FOUND" },
        format!("Install official SumatraPDF {SUMATRA_VERSION} or place its portable executable beside PrintBridge"),
    ))
}

#[async_trait]
impl PrinterBackend for WindowsBackend {
    async fn printers(&self) -> Result<Vec<String>> {
        self.driver_call(enumerate).await?
    }

    async fn status(&self) -> BackendStatus {
        let error = sumatra().err().map(|error| error.message);
        BackendStatus { available: error.is_none(), expected_version: SUMATRA_VERSION, error }
    }

    async fn prepare(&self, profile: &Profile, copies: u16) -> std::result::Result<PreparedPrint, PrintFailure> {
        sumatra()?;
        let profile = profile.clone();
        self.driver_call(move || {
            let names = enumerate().map_err(|error| PrintFailure::before_print("PRINTER_DISCOVERY_FAILED", error.to_string()))?;
            if !names.contains(&profile.printer) {
                return Err(PrintFailure::before_print("PRINTER_NOT_FOUND", "Configured queue was not found; use its exact name from printers.list"));
            }
            media(&profile, copies).map_err(|error| PrintFailure::before_print("UNSUPPORTED_MEDIA", error.to_string()))
        }).await.map_err(|error| PrintFailure::before_print("PRINTER_CHECK_FAILED", error.to_string()))?
    }

    async fn print(&self, plan: PreparedPrint, profile: &Profile, copies: u16, pdf: &Path, app_data: &Path)
        -> std::result::Result<(), PrintFailure> {
        let executable = sumatra()?;
        let mut command = tokio::process::Command::new(executable);
        command.creation_flags(0x08000000)
            .kill_on_drop(true)
            .arg("-silent").arg("-appdata").arg(app_data)
            .arg("-print-to").arg(&profile.printer)
            .arg("-print-settings").arg(print_settings(plan, profile, copies))
            .arg(pdf)
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|error| PrintFailure::before_print(
            "SUMATRA_START_FAILED", format!("Cannot start SumatraPDF (Windows error {:?})", error.raw_os_error())))?;
        let mut stderr = child.stderr.take().expect("Sumatra stderr is piped");
        let mut capture = tokio::spawn(async move {
            let mut captured = Vec::new();
            let mut chunk = [0; 1024];
            while let Ok(count) = stderr.read(&mut chunk).await {
                if count == 0 { break; }
                let take = count.min(4096 - captured.len());
                captured.extend_from_slice(&chunk[..take]);
            }
            captured
        });
        let exit = tokio::time::timeout(Duration::from_secs(60), child.wait()).await;
        let status = match exit {
            Ok(Ok(status)) => status,
            other => {
                let _ = child.start_kill();
                let _ = tokio::time::timeout(Duration::from_secs(3), child.wait()).await;
                capture.abort();
                let (code, message) = match other {
                    Err(_) => ("SUMATRA_TIMEOUT", "SumatraPDF did not finish within 60 seconds"),
                    _ => ("SUMATRA_WAIT_FAILED", "Could not obtain SumatraPDF's process result"),
                };
                return Err(PrintFailure { code, message: format!("{message}; check the Windows queue before reprinting"),
                    exit_code: None, may_have_printed: true });
            }
        };
        let diagnostic = match tokio::time::timeout(Duration::from_secs(1), &mut capture).await {
            Ok(Ok(bytes)) => bytes,
            _ => { capture.abort(); Vec::new() }
        };
        process_result(status.code(), &diagnostic, app_data)
    }
}
