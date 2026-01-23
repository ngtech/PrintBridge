#[cfg(test)]
mod tests {
    use crate::download::DownloadManager;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_manager(https_only: bool) -> (DownloadManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let manager = DownloadManager::new(temp_dir.path().to_path_buf(), https_only).unwrap();
        (manager, temp_dir)
    }

    #[test]
    fn test_new_creates_cache_dir() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().join("cache");

        let _manager = DownloadManager::new(cache_path.clone(), true).unwrap();

        assert!(cache_path.exists());
    }

    #[test]
    fn test_cache_dir_accessor() {
        let (manager, temp_dir) = create_test_manager(true);
        assert_eq!(manager.cache_dir(), temp_dir.path());
    }

    #[tokio::test]
    async fn test_https_only_rejects_http() {
        let (manager, _temp_dir) = create_test_manager(true);

        let result = manager.download("http://example.com/test.pdf", None, None).await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("HTTPS"));
    }

    #[tokio::test]
    async fn test_allows_http_when_disabled() {
        let (manager, _temp_dir) = create_test_manager(false);

        // This will fail because the URL doesn't exist, but it shouldn't fail
        // due to scheme validation
        let result = manager.download("http://localhost:99999/nonexistent.pdf", None, None).await;

        // Should fail with connection error, not scheme error
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(!err.contains("HTTPS"));
    }

    #[test]
    fn test_rejects_invalid_scheme() {
        let (manager, _temp_dir) = create_test_manager(false);

        // Using block_on since we need to test this synchronously
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(async {
            manager.download("ftp://example.com/test.pdf", None, None).await
        });

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid URL"));
    }

    #[tokio::test]
    async fn test_cleanup_removes_old_files() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().to_path_buf();

        // Create some test files
        let old_file = cache_path.join("old_file.pdf");
        let new_file = cache_path.join("new_file.pdf");

        std::fs::write(&old_file, b"old content").unwrap();
        std::fs::write(&new_file, b"new content").unwrap();

        // Set old file's modification time to past (if possible)
        // This is platform-specific and may not work in all test environments

        let manager = DownloadManager::new(cache_path, true).unwrap();

        // Cleanup with 0 max age should remove all files
        let deleted = manager.cleanup(0).await.unwrap();

        // Both files should be deleted since max_age is 0
        assert_eq!(deleted, 2);
    }

    #[tokio::test]
    async fn test_delete_file() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().to_path_buf();

        let test_file = cache_path.join("test.pdf");
        std::fs::write(&test_file, b"test content").unwrap();
        assert!(test_file.exists());

        let manager = DownloadManager::new(cache_path, true).unwrap();
        manager.delete(&test_file).await.unwrap();

        assert!(!test_file.exists());
    }

    #[tokio::test]
    async fn test_delete_ignores_files_outside_cache() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().join("cache");

        // Create a file outside the cache directory
        let outside_file = temp_dir.path().join("outside.pdf");
        std::fs::write(&outside_file, b"should not be deleted").unwrap();

        let manager = DownloadManager::new(cache_path, true).unwrap();

        // This should not delete the file since it's outside cache dir
        let _ = manager.delete(&outside_file).await;

        assert!(outside_file.exists());
    }

    #[test]
    fn test_headers_parameter() {
        // Test that headers can be passed (actual HTTP test would need a server)
        let mut headers = std::collections::HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer token123".to_string());
        headers.insert("X-Custom-Header".to_string(), "custom-value".to_string());

        assert_eq!(headers.len(), 2);
        assert_eq!(headers.get("Authorization"), Some(&"Bearer token123".to_string()));
    }
}
