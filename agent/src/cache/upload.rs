use anyhow::{Result, Context};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use base64::Engine;

/// Manages chunked file uploads from the SDK
pub struct UploadManager {
    cache_dir: PathBuf,
    uploads: Arc<Mutex<HashMap<String, UploadState>>>,
}

struct UploadState {
    file: File,
    path: PathBuf,
    filename: String,
    total_size: u64,
    received: u64,
    chunks_received: Vec<bool>,
}

impl UploadManager {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            cache_dir,
            uploads: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Start a new upload
    pub async fn start_upload(
        &self,
        upload_id: &str,
        filename: &str,
        total_size: u64,
    ) -> Result<()> {
        // Generate unique path
        let unique_name = format!(
            "{}_{}_{}",
            chrono::Utc::now().timestamp_millis(),
            upload_id,
            filename
        );
        let path = self.cache_dir.join(&unique_name);

        // Create file
        let file = File::create(&path).await
            .context("Failed to create upload file")?;

        // Calculate expected chunks (64KB each)
        let chunk_size = 64 * 1024u64;
        let num_chunks = ((total_size + chunk_size - 1) / chunk_size) as usize;

        let state = UploadState {
            file,
            path,
            filename: filename.to_string(),
            total_size,
            received: 0,
            chunks_received: vec![false; num_chunks],
        };

        let mut uploads = self.uploads.lock().await;
        uploads.insert(upload_id.to_string(), state);

        log::info!("Started upload {} for file {} ({} bytes, {} chunks)",
            upload_id, filename, total_size, num_chunks);

        Ok(())
    }

    /// Receive a chunk
    pub async fn receive_chunk(
        &self,
        upload_id: &str,
        chunk_index: u32,
        data: &str, // Base64 encoded
    ) -> Result<(u64, u64)> {
        // Decode base64
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(data)
            .context("Invalid base64 data")?;

        let mut uploads = self.uploads.lock().await;
        let state = uploads.get_mut(upload_id)
            .ok_or_else(|| anyhow::anyhow!("Upload not found: {}", upload_id))?;

        // Mark chunk as received
        if (chunk_index as usize) < state.chunks_received.len() {
            state.chunks_received[chunk_index as usize] = true;
        }

        // Write data
        state.file.write_all(&decoded).await
            .context("Failed to write chunk")?;

        state.received += decoded.len() as u64;

        log::debug!("Received chunk {} for upload {} ({}/{} bytes)",
            chunk_index, upload_id, state.received, state.total_size);

        Ok((state.received, state.total_size))
    }

    /// Complete an upload and return the file path
    pub async fn complete_upload(&self, upload_id: &str) -> Result<PathBuf> {
        let mut uploads = self.uploads.lock().await;
        let mut state = uploads.remove(upload_id)
            .ok_or_else(|| anyhow::anyhow!("Upload not found: {}", upload_id))?;

        // Flush and close file
        state.file.flush().await?;
        drop(state.file);

        log::info!("Completed upload {} ({} bytes)", upload_id, state.received);

        Ok(state.path)
    }

    /// Cancel an upload
    pub async fn cancel_upload(&self, upload_id: &str) -> Result<()> {
        let mut uploads = self.uploads.lock().await;
        if let Some(state) = uploads.remove(upload_id) {
            // Clean up partial file
            drop(state.file);
            let _ = tokio::fs::remove_file(&state.path).await;
            log::info!("Cancelled upload {}", upload_id);
        }
        Ok(())
    }

    /// Get upload progress
    pub async fn get_progress(&self, upload_id: &str) -> Option<(u64, u64)> {
        let uploads = self.uploads.lock().await;
        uploads.get(upload_id).map(|s| (s.received, s.total_size))
    }

    /// Clean up stale uploads (older than max_age_secs)
    pub async fn cleanup_stale(&self, max_age_secs: u64) -> Result<u32> {
        // For now, just clean up any orphaned upload files
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
                        }
                    }
                }
            }
        }

        Ok(deleted)
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }
}
