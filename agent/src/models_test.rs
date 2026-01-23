#[cfg(test)]
mod tests {
    use crate::models::*;
    use std::collections::HashMap;

    #[test]
    fn test_content_type_from_extension() {
        assert_eq!(ContentType::from_extension("pdf"), Some(ContentType::Pdf));
        assert_eq!(ContentType::from_extension("PDF"), Some(ContentType::Pdf));
        assert_eq!(ContentType::from_extension("png"), Some(ContentType::Png));
        assert_eq!(ContentType::from_extension("jpg"), Some(ContentType::Jpeg));
        assert_eq!(ContentType::from_extension("jpeg"), Some(ContentType::Jpeg));
        assert_eq!(ContentType::from_extension("gif"), Some(ContentType::Gif));
        assert_eq!(ContentType::from_extension("bmp"), Some(ContentType::Bmp));
        assert_eq!(ContentType::from_extension("tiff"), Some(ContentType::Tiff));
        assert_eq!(ContentType::from_extension("tif"), Some(ContentType::Tiff));
        assert_eq!(ContentType::from_extension("zpl"), Some(ContentType::Zpl));
        assert_eq!(ContentType::from_extension("epl"), Some(ContentType::Epl));
        assert_eq!(ContentType::from_extension("unknown"), None);
    }

    #[test]
    fn test_content_type_from_mime() {
        assert_eq!(ContentType::from_mime("application/pdf"), Some(ContentType::Pdf));
        assert_eq!(ContentType::from_mime("image/png"), Some(ContentType::Png));
        assert_eq!(ContentType::from_mime("image/jpeg"), Some(ContentType::Jpeg));
        assert_eq!(ContentType::from_mime("application/x-escpos"), Some(ContentType::EscPos));
        assert_eq!(ContentType::from_mime("application/x-zpl"), Some(ContentType::Zpl));
        assert_eq!(ContentType::from_mime("application/x-epl"), Some(ContentType::Epl));
        assert_eq!(ContentType::from_mime("application/octet-stream"), Some(ContentType::Raw));
        assert_eq!(ContentType::from_mime("text/plain"), None);
    }

    #[test]
    fn test_content_type_is_raw() {
        assert!(ContentType::EscPos.is_raw());
        assert!(ContentType::Zpl.is_raw());
        assert!(ContentType::Epl.is_raw());
        assert!(ContentType::Raw.is_raw());
        assert!(!ContentType::Pdf.is_raw());
        assert!(!ContentType::Png.is_raw());
    }

    #[test]
    fn test_content_type_is_image() {
        assert!(ContentType::Png.is_image());
        assert!(ContentType::Jpeg.is_image());
        assert!(ContentType::Gif.is_image());
        assert!(ContentType::Bmp.is_image());
        assert!(ContentType::Tiff.is_image());
        assert!(!ContentType::Pdf.is_image());
        assert!(!ContentType::EscPos.is_image());
    }

    #[test]
    fn test_print_options_defaults() {
        let options = PrintOptions::default();
        assert_eq!(options.scaling, ScalingMode::Shrink);
        assert_eq!(options.color_mode, ColorMode::Color);
        assert_eq!(options.copies, 1);
        assert_eq!(options.duplex, DuplexMode::None);
        assert!(options.paper_size.is_none());
        assert!(options.orientation.is_none());
    }

    #[test]
    fn test_print_job_creation() {
        let request = PrintRequest {
            id: Some("test-job-123".to_string()),
            url: Some("https://example.com/doc.pdf".to_string()),
            headers: None,
            data: None,
            printer: Some("Test Printer".to_string()),
            content_type: Some(ContentType::Pdf),
            options: PrintOptions::default(),
        };

        let job = PrintJob::new(request, "Default Printer");

        assert_eq!(job.id, "test-job-123");
        assert_eq!(job.printer, "Test Printer");
        assert_eq!(job.status, JobStatus::Pending);
        assert!(job.error.is_none());
        assert_eq!(job.content_type, Some(ContentType::Pdf));
    }

    #[test]
    fn test_print_job_uses_default_printer() {
        let request = PrintRequest {
            id: None,
            url: Some("https://example.com/doc.pdf".to_string()),
            headers: None,
            data: None,
            printer: None, // No printer specified
            content_type: None,
            options: PrintOptions::default(),
        };

        let job = PrintJob::new(request, "Default Printer");

        assert_eq!(job.printer, "Default Printer");
        assert!(!job.id.is_empty()); // Should auto-generate ID
    }

    #[test]
    fn test_print_job_status_update() {
        let request = PrintRequest {
            id: Some("test".to_string()),
            url: None,
            headers: None,
            data: None,
            printer: None,
            content_type: None,
            options: PrintOptions::default(),
        };

        let mut job = PrintJob::new(request, "Printer");
        let initial_time = job.updated_at;

        // Small delay to ensure time difference
        std::thread::sleep(std::time::Duration::from_millis(10));

        job.set_status(JobStatus::Downloading);
        assert_eq!(job.status, JobStatus::Downloading);
        assert!(job.updated_at > initial_time);
    }

    #[test]
    fn test_print_job_error() {
        let request = PrintRequest {
            id: Some("test".to_string()),
            url: None,
            headers: None,
            data: None,
            printer: None,
            content_type: None,
            options: PrintOptions::default(),
        };

        let mut job = PrintJob::new(request, "Printer");
        job.set_error("Connection failed".to_string());

        assert_eq!(job.status, JobStatus::Failed);
        assert_eq!(job.error, Some("Connection failed".to_string()));
    }

    #[test]
    fn test_printer_info() {
        let printer = PrinterInfo {
            name: "HP LaserJet".to_string(),
            status: PrinterStatus::Ready,
            is_default: true,
            capabilities: PrinterCapabilities {
                color: true,
                duplex: true,
                paper_sizes: vec!["letter".to_string(), "a4".to_string()],
            },
        };

        assert_eq!(printer.name, "HP LaserJet");
        assert!(printer.is_default);
        assert!(printer.capabilities.color);
        assert!(printer.capabilities.duplex);
        assert_eq!(printer.capabilities.paper_sizes.len(), 2);
    }

    #[test]
    fn test_batch_error_mode_default() {
        let mode = BatchErrorMode::default();
        assert_eq!(mode, BatchErrorMode::Stop);
    }

    #[test]
    fn test_batch_request() {
        let batch = BatchRequest {
            id: "batch-123".to_string(),
            jobs: vec![],
            on_error: BatchErrorMode::Continue,
            max_retries: 5,
            retry_delay: 2000,
        };

        assert_eq!(batch.id, "batch-123");
        assert_eq!(batch.on_error, BatchErrorMode::Continue);
        assert_eq!(batch.max_retries, 5);
        assert_eq!(batch.retry_delay, 2000);
    }

    #[test]
    fn test_job_status_serialization() {
        let status = JobStatus::Downloading;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"downloading\"");

        let parsed: JobStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, JobStatus::Downloading);
    }

    #[test]
    fn test_print_request_with_headers() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer token123".to_string());
        headers.insert("X-Custom".to_string(), "value".to_string());

        let request = PrintRequest {
            id: None,
            url: Some("https://api.example.com/doc.pdf".to_string()),
            headers: Some(headers.clone()),
            data: None,
            printer: None,
            content_type: None,
            options: PrintOptions::default(),
        };

        assert!(request.headers.is_some());
        let hdrs = request.headers.unwrap();
        assert_eq!(hdrs.get("Authorization"), Some(&"Bearer token123".to_string()));
    }
}
