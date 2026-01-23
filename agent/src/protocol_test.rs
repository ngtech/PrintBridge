#[cfg(test)]
mod tests {
    use crate::protocol::*;
    use crate::models::*;

    #[test]
    fn test_client_message_get_status_serialization() {
        let msg = ClientMessage::GetStatus {
            id: "msg-123".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"getStatus\""));
        assert!(json.contains("\"id\":\"msg-123\""));
    }

    #[test]
    fn test_client_message_get_printers_serialization() {
        let msg = ClientMessage::GetPrinters {
            id: "msg-456".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"getPrinters\""));
    }

    #[test]
    fn test_client_message_print_deserialization() {
        let json = r#"{
            "type": "print",
            "id": "msg-789",
            "request": {
                "url": "https://example.com/doc.pdf",
                "printer": "HP LaserJet"
            }
        }"#;

        let msg: ClientMessage = serde_json::from_str(json).unwrap();

        match msg {
            ClientMessage::Print { id, request } => {
                assert_eq!(id, "msg-789");
                assert_eq!(request.url, Some("https://example.com/doc.pdf".to_string()));
                assert_eq!(request.printer, Some("HP LaserJet".to_string()));
            }
            _ => panic!("Expected Print message"),
        }
    }

    #[test]
    fn test_client_message_ping() {
        let msg = ClientMessage::Ping {
            id: "ping-1".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        let parsed: ClientMessage = serde_json::from_str(&json).unwrap();

        match parsed {
            ClientMessage::Ping { id } => assert_eq!(id, "ping-1"),
            _ => panic!("Expected Ping message"),
        }
    }

    #[test]
    fn test_agent_message_welcome() {
        let msg = AgentMessage::Welcome {
            version: "1.0.0".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"welcome\""));
        assert!(json.contains("\"version\":\"1.0.0\""));
    }

    #[test]
    fn test_agent_message_status() {
        let msg = AgentMessage::Status {
            id: "msg-123".to_string(),
            status: AgentStatus {
                version: "1.0.0".to_string(),
                platform: "windows".to_string(),
                printer_count: 3,
                queue_length: 0,
                uptime: 12345,
            },
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"status\""));
        assert!(json.contains("\"printerCount\":3"));
    }

    #[test]
    fn test_agent_message_printers() {
        let msg = AgentMessage::Printers {
            id: "msg-456".to_string(),
            printers: vec![
                PrinterInfo {
                    name: "Printer 1".to_string(),
                    status: PrinterStatus::Ready,
                    is_default: true,
                    capabilities: PrinterCapabilities::default(),
                },
                PrinterInfo {
                    name: "Printer 2".to_string(),
                    status: PrinterStatus::Offline,
                    is_default: false,
                    capabilities: PrinterCapabilities::default(),
                },
            ],
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"printers\""));
        assert!(json.contains("\"Printer 1\""));
        assert!(json.contains("\"Printer 2\""));
    }

    #[test]
    fn test_agent_message_error() {
        let msg = AgentMessage::error(
            "msg-error".to_string(),
            error_codes::PRINTER_NOT_FOUND,
            "Printer 'Unknown' not found",
        );

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"error\""));
        assert!(json.contains("PRINTER_NOT_FOUND"));
        assert!(json.contains("Printer 'Unknown' not found"));
    }

    #[test]
    fn test_agent_message_pong() {
        let msg = AgentMessage::Pong {
            id: "ping-1".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"pong\""));
        assert!(json.contains("\"id\":\"ping-1\""));
    }

    #[test]
    fn test_upload_start_message() {
        let json = r#"{
            "type": "uploadStart",
            "id": "msg-upload",
            "uploadId": "upload-123",
            "filename": "document.pdf",
            "totalSize": 1048576
        }"#;

        let msg: ClientMessage = serde_json::from_str(json).unwrap();

        match msg {
            ClientMessage::UploadStart { id, upload_id, filename, total_size, .. } => {
                assert_eq!(id, "msg-upload");
                assert_eq!(upload_id, "upload-123");
                assert_eq!(filename, "document.pdf");
                assert_eq!(total_size, 1048576);
            }
            _ => panic!("Expected UploadStart message"),
        }
    }

    #[test]
    fn test_batch_progress_message() {
        let progress = BatchProgress {
            batch_id: "batch-123".to_string(),
            completed: 5,
            failed: 1,
            total: 10,
            current_job: None,
        };

        let msg = AgentMessage::BatchProgress {
            id: "msg-batch".to_string(),
            progress,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"batchProgress\""));
        assert!(json.contains("\"completed\":5"));
        assert!(json.contains("\"failed\":1"));
        assert!(json.contains("\"total\":10"));
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(error_codes::INVALID_REQUEST, "INVALID_REQUEST");
        assert_eq!(error_codes::PRINTER_NOT_FOUND, "PRINTER_NOT_FOUND");
        assert_eq!(error_codes::DOWNLOAD_FAILED, "DOWNLOAD_FAILED");
        assert_eq!(error_codes::PRINT_FAILED, "PRINT_FAILED");
        assert_eq!(error_codes::JOB_NOT_FOUND, "JOB_NOT_FOUND");
        assert_eq!(error_codes::UPLOAD_FAILED, "UPLOAD_FAILED");
        assert_eq!(error_codes::UNSUPPORTED_FORMAT, "UNSUPPORTED_FORMAT");
        assert_eq!(error_codes::INTERNAL_ERROR, "INTERNAL_ERROR");
    }

    #[test]
    fn test_cancel_job_message() {
        let msg = ClientMessage::CancelJob {
            id: "msg-cancel".to_string(),
            job_id: "job-to-cancel".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"cancelJob\""));
        assert!(json.contains("\"jobId\":\"job-to-cancel\""));
    }

    #[test]
    fn test_print_batch_message() {
        let json = r#"{
            "type": "printBatch",
            "id": "msg-batch",
            "request": {
                "id": "batch-456",
                "jobs": [
                    {"url": "https://example.com/doc1.pdf"},
                    {"url": "https://example.com/doc2.pdf"}
                ],
                "onError": "continue",
                "maxRetries": 3,
                "retryDelay": 1000
            }
        }"#;

        let msg: ClientMessage = serde_json::from_str(json).unwrap();

        match msg {
            ClientMessage::PrintBatch { id, request } => {
                assert_eq!(id, "msg-batch");
                assert_eq!(request.id, "batch-456");
                assert_eq!(request.jobs.len(), 2);
                assert_eq!(request.on_error, BatchErrorMode::Continue);
            }
            _ => panic!("Expected PrintBatch message"),
        }
    }
}
