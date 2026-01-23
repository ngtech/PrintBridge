use anyhow::{Result, Context};
use reqwest::Client;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use futures_util::StreamExt;

/// Download manager for fetching documents
pub struct DownloadManager {
    client: Client,
    cache_dir: PathBuf,
    https_only: bool,
}

impl DownloadManager {
    pub fn new(cache_dir: PathBuf, https_only: bool) -> Result<Self> {
        let client = Client::builder()
            .user_agent("PrintBridge/1.0")
            .timeout(std::time::Duration::from_secs(300)) // 5 min timeout
            .build()
            .context("Failed to create HTTP client")?;

        // Ensure cache directory exists
        std::fs::create_dir_all(&cache_dir)?;

        Ok(Self {
            client,
            cache_dir,
            https_only,
        })
    }

    /// Download a file from URL to the cache directory
    pub async fn download(
        &self,
        url: &str,
        headers: Option<&HashMap<String, String>>,
        filename: Option<&str>,
    ) -> Result<PathBuf> {
        // Validate URL scheme
        if self.https_only && !url.starts_with("https://") {
            anyhow::bail!("Only HTTPS URLs are allowed");
        }

        if !url.starts_with("http://") && !url.starts_with("https://") {
            anyhow::bail!("Invalid URL scheme");
        }

        log::info!("Downloading: {}", url);

        // Build request
        let mut request = self.client.get(url);

        // Add custom headers
        if let Some(hdrs) = headers {
            for (key, value) in hdrs {
                request = request.header(key, value);
            }
        }

        // Send request
        let response = request.send().await
            .context("Failed to send download request")?;

        // Check status
        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("Download failed with status: {}", status);
        }

        // Determine filename
        let file_name = if let Some(name) = filename {
            name.to_string()
        } else {
            // Try to get from Content-Disposition header
            let from_header = response.headers()
                .get("content-disposition")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| {
                    // Parse filename from header
                    v.split(';')
                        .find(|p| p.trim().starts_with("filename"))
                        .and_then(|p| p.split('=').nth(1))
                        .map(|s| s.trim().trim_matches('"').to_string())
                });

            if let Some(name) = from_header {
                name
            } else {
                // Extract from URL path
                url.split('/')
                    .last()
                    .and_then(|s| s.split('?').next())
                    .unwrap_or("download")
                    .to_string()
            }
        };

        // Generate unique filename to avoid collisions
        let unique_name = format!(
            "{}_{}_{}",
            chrono::Utc::now().timestamp_millis(),
            uuid::Uuid::new_v4().as_simple(),
            file_name
        );

        let file_path = self.cache_dir.join(&unique_name);

        // Stream response body to file
        let mut file = File::create(&file_path).await
            .context("Failed to create download file")?;

        let mut stream = response.bytes_stream();
        let mut total_bytes = 0u64;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("Error reading download stream")?;
            file.write_all(&chunk).await
                .context("Error writing to file")?;
            total_bytes += chunk.len() as u64;
        }

        file.flush().await?;

        log::info!("Downloaded {} bytes to {:?}", total_bytes, file_path);

        Ok(file_path)
    }

    /// Get cache directory
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Clean up old cached files
    pub async fn cleanup(&self, max_age_secs: u64) -> Result<u32> {
        let cutoff = std::time::SystemTime::now()
            .checked_sub(std::time::Duration::from_secs(max_age_secs))
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

        let mut deleted = 0u32;

        let mut entries = tokio::fs::read_dir(&self.cache_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            if let Ok(metadata) = entry.metadata().await {
                if let Ok(modified) = metadata.modified() {
                    if modified < cutoff {
                        if tokio::fs::remove_file(entry.path()).await.is_ok() {
                            deleted += 1;
                            log::debug!("Cleaned up old cache file: {:?}", entry.path());
                        }
                    }
                }
            }
        }

        if deleted > 0 {
            log::info!("Cleaned up {} old cache files", deleted);
        }

        Ok(deleted)
    }

    /// Delete a specific file
    pub async fn delete(&self, path: &Path) -> Result<()> {
        if path.starts_with(&self.cache_dir) {
            tokio::fs::remove_file(path).await?;
        }
        Ok(())
    }
}
