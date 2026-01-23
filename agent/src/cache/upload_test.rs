#[cfg(test)]
mod tests {
    use crate::cache::UploadManager;
    use tempfile::TempDir;
    use base64::Engine;

    fn create_test_manager() -> (UploadManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let manager = UploadManager::new(temp_dir.path().to_path_buf());
        (manager, temp_dir)
    }

    #[tokio::test]
    async fn test_start_upload() {
        let (manager, _temp_dir) = create_test_manager();

        let result = manager.start_upload("upload-1", "test.pdf", 1024).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_receive_chunk() {
        let (manager, _temp_dir) = create_test_manager();

        // Start upload
        manager.start_upload("upload-1", "test.pdf", 100).await.unwrap();

        // Send a chunk (base64 encoded)
        let data = b"Hello, World!";
        let encoded = base64::engine::general_purpose::STANDARD.encode(data);

        let (received, total) = manager.receive_chunk("upload-1", 0, &encoded).await.unwrap();

        assert_eq!(received, data.len() as u64);
        assert_eq!(total, 100);
    }

    #[tokio::test]
    async fn test_receive_chunk_invalid_upload() {
        let (manager, _temp_dir) = create_test_manager();

        let encoded = base64::engine::general_purpose::STANDARD.encode(b"data");
        let result = manager.receive_chunk("nonexistent", 0, &encoded).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Upload not found"));
    }

    #[tokio::test]
    async fn test_receive_chunk_invalid_base64() {
        let (manager, _temp_dir) = create_test_manager();

        manager.start_upload("upload-1", "test.pdf", 100).await.unwrap();

        let result = manager.receive_chunk("upload-1", 0, "not-valid-base64!!!").await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("base64"));
    }

    #[tokio::test]
    async fn test_complete_upload() {
        let (manager, _temp_dir) = create_test_manager();

        // Start upload
        manager.start_upload("upload-1", "test.pdf", 13).await.unwrap();

        // Send chunk
        let data = b"Hello, World!";
        let encoded = base64::engine::general_purpose::STANDARD.encode(data);
        manager.receive_chunk("upload-1", 0, &encoded).await.unwrap();

        // Complete upload
        let path = manager.complete_upload("upload-1").await.unwrap();

        assert!(path.exists());
        assert!(path.to_string_lossy().contains("upload-1"));

        // Verify content
        let content = std::fs::read(&path).unwrap();
        assert_eq!(content, data);
    }

    #[tokio::test]
    async fn test_complete_upload_not_found() {
        let (manager, _temp_dir) = create_test_manager();

        let result = manager.complete_upload("nonexistent").await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Upload not found"));
    }

    #[tokio::test]
    async fn test_cancel_upload() {
        let (manager, temp_dir) = create_test_manager();

        // Start upload
        manager.start_upload("upload-1", "test.pdf", 1024).await.unwrap();

        // Send some data
        let encoded = base64::engine::general_purpose::STANDARD.encode(b"partial data");
        manager.receive_chunk("upload-1", 0, &encoded).await.unwrap();

        // Cancel upload
        manager.cancel_upload("upload-1").await.unwrap();

        // Verify upload is gone
        let result = manager.complete_upload("upload-1").await;
        assert!(result.is_err());

        // Check that temp file was cleaned up (there should be no files with upload-1 in name)
        let files: Vec<_> = std::fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("upload-1"))
            .collect();
        assert!(files.is_empty());
    }

    #[tokio::test]
    async fn test_cancel_nonexistent_upload() {
        let (manager, _temp_dir) = create_test_manager();

        // Should not error when canceling nonexistent upload
        let result = manager.cancel_upload("nonexistent").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_progress() {
        let (manager, _temp_dir) = create_test_manager();

        // No progress for nonexistent upload
        assert!(manager.get_progress("nonexistent").await.is_none());

        // Start upload
        manager.start_upload("upload-1", "test.pdf", 1000).await.unwrap();

        // Check initial progress
        let (received, total) = manager.get_progress("upload-1").await.unwrap();
        assert_eq!(received, 0);
        assert_eq!(total, 1000);

        // Send some data
        let encoded = base64::engine::general_purpose::STANDARD.encode(b"0123456789"); // 10 bytes
        manager.receive_chunk("upload-1", 0, &encoded).await.unwrap();

        // Check updated progress
        let (received, total) = manager.get_progress("upload-1").await.unwrap();
        assert_eq!(received, 10);
        assert_eq!(total, 1000);
    }

    #[tokio::test]
    async fn test_multiple_chunks() {
        let (manager, _temp_dir) = create_test_manager();

        manager.start_upload("upload-1", "test.pdf", 30).await.unwrap();

        // Send multiple chunks
        let chunks = ["Hello, ", "World", "!"];
        for (i, chunk) in chunks.iter().enumerate() {
            let encoded = base64::engine::general_purpose::STANDARD.encode(chunk.as_bytes());
            manager.receive_chunk("upload-1", i as u32, &encoded).await.unwrap();
        }

        // Complete and verify
        let path = manager.complete_upload("upload-1").await.unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "Hello, World!");
    }

    #[tokio::test]
    async fn test_multiple_concurrent_uploads() {
        let (manager, _temp_dir) = create_test_manager();

        // Start multiple uploads
        manager.start_upload("upload-1", "file1.pdf", 10).await.unwrap();
        manager.start_upload("upload-2", "file2.pdf", 10).await.unwrap();

        // Send data to each
        let encoded1 = base64::engine::general_purpose::STANDARD.encode(b"file1data!");
        let encoded2 = base64::engine::general_purpose::STANDARD.encode(b"file2data!");

        manager.receive_chunk("upload-1", 0, &encoded1).await.unwrap();
        manager.receive_chunk("upload-2", 0, &encoded2).await.unwrap();

        // Complete both
        let path1 = manager.complete_upload("upload-1").await.unwrap();
        let path2 = manager.complete_upload("upload-2").await.unwrap();

        // Verify contents are separate
        let content1 = std::fs::read_to_string(&path1).unwrap();
        let content2 = std::fs::read_to_string(&path2).unwrap();

        assert_eq!(content1, "file1data!");
        assert_eq!(content2, "file2data!");
    }

    #[test]
    fn test_cache_dir_accessor() {
        let (manager, temp_dir) = create_test_manager();
        assert_eq!(manager.cache_dir(), temp_dir.path());
    }
}
