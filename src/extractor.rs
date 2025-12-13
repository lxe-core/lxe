//! Async Streaming Extractor
//!
//! Handles decompression and extraction of the zstd-compressed payload
//! using async I/O to prevent UI blocking.

use crate::metadata::LxeMetadata;
use crate::payload::PayloadInfo;
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::fs::{self, File};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::watch;

/// Progress information sent to the UI
#[derive(Debug, Clone)]
pub struct ExtractProgress {
    /// Total bytes to extract (uncompressed)
    pub total_bytes: u64,
    
    /// Bytes extracted so far
    pub extracted_bytes: u64,
    
    /// Number of files extracted
    pub files_extracted: u32,
    
    /// Current file being extracted
    pub current_file: String,
    
    /// Whether extraction is complete
    pub complete: bool,
    
    /// Error message if extraction failed
    pub error: Option<String>,
}

impl ExtractProgress {
    pub fn new(total_bytes: u64) -> Self {
        Self {
            total_bytes,
            extracted_bytes: 0,
            files_extracted: 0,
            current_file: String::new(),
            complete: false,
            error: None,
        }
    }
    
    /// Progress as a fraction 0.0 - 1.0
    pub fn fraction(&self) -> f64 {
        if self.total_bytes == 0 {
            0.0
        } else {
            self.extracted_bytes as f64 / self.total_bytes as f64
        }
    }
}

/// Extract the payload to a target directory
/// Returns a watch receiver for progress updates
pub fn extract_async(
    payload_info: PayloadInfo,
    target_dir: PathBuf,
) -> (watch::Receiver<ExtractProgress>, tokio::task::JoinHandle<Result<()>>) {
    let (tx, rx) = watch::channel(ExtractProgress::new(payload_info.metadata.install_size));
    
    let handle = tokio::spawn(async move {
        extract_inner(payload_info, target_dir, tx).await
    });
    
    (rx, handle)
}

async fn extract_inner(
    payload_info: PayloadInfo,
    target_dir: PathBuf,
    progress_tx: watch::Sender<ExtractProgress>,
) -> Result<()> {
    let mut progress = ExtractProgress::new(payload_info.metadata.install_size);
    
    // Ensure target directory exists
    fs::create_dir_all(&target_dir).await
        .context("Failed to create target directory")?;
    
    // Create a temporary directory for extraction
    let temp_dir = target_dir.join(".lxe-extracting");
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir).await?;
    }
    fs::create_dir_all(&temp_dir).await?;
    
    // Open the payload for reading
    let file = std::fs::File::open(&payload_info.exe_path)?;
    let mut reader = std::io::BufReader::new(file);
    std::io::Seek::seek(&mut reader, std::io::SeekFrom::Start(payload_info.payload_offset))?;
    
    // Create a streaming zstd decoder using ruzstd (pure Rust)
    let decoder = ruzstd::StreamingDecoder::new(&mut reader)
        .context("Failed to initialize zstd decoder")?;
    
    // Wrap in a tar archive reader
    let mut archive = tar::Archive::new(decoder);
    
    // Extract entries
    for entry in archive.entries()? {
        let mut entry = entry.context("Failed to read tar entry")?;
        let path = entry.path()?.to_path_buf();
        let path_str = path.to_string_lossy().to_string();
        
        progress.current_file = path_str.clone();
        let _ = progress_tx.send(progress.clone());
        
        // Determine target path
        let target_path = temp_dir.join(&path);
        
        // Create parent directories
        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        // Extract the file
        entry.unpack(&target_path)?;
        
        // Update progress
        progress.extracted_bytes += entry.size();
        progress.files_extracted += 1;
        let _ = progress_tx.send(progress.clone());
    }
    
    // Atomic move from temp to final location
    // First, remove any existing installation
    let final_app_dir = target_dir.join(&payload_info.metadata.app_id);
    if final_app_dir.exists() {
        fs::remove_dir_all(&final_app_dir).await?;
    }
    
    // Move temp to final
    fs::rename(&temp_dir, &final_app_dir).await
        .context("Failed to move extracted files to final location")?;
    
    // Mark complete
    progress.complete = true;
    progress.current_file = "Complete!".to_string();
    let _ = progress_tx.send(progress);
    
    Ok(())
}

/// Verify the payload checksum before extraction
pub async fn verify_checksum(payload_info: &PayloadInfo) -> Result<bool> {
    let expected = &payload_info.metadata.payload_checksum;
    
    let file = std::fs::File::open(&payload_info.exe_path)?;
    let mut reader = std::io::BufReader::new(file);
    std::io::Seek::seek(&mut reader, std::io::SeekFrom::Start(payload_info.payload_offset))?;
    
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024]; // 64KB buffer
    let mut remaining = payload_info.payload_size;
    
    while remaining > 0 {
        let to_read = std::cmp::min(buffer.len() as u64, remaining) as usize;
        let bytes_read = std::io::Read::read(&mut reader, &mut buffer[..to_read])?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
        remaining -= bytes_read as u64;
    }
    
    let result = hasher.finalize();
    let computed = hex::encode(result);
    
    Ok(computed == *expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_fraction() {
        let mut progress = ExtractProgress::new(100);
        assert_eq!(progress.fraction(), 0.0);
        
        progress.extracted_bytes = 50;
        assert_eq!(progress.fraction(), 0.5);
        
        progress.extracted_bytes = 100;
        assert_eq!(progress.fraction(), 1.0);
    }

    #[test]
    fn test_progress_zero_total() {
        let progress = ExtractProgress::new(0);
        assert_eq!(progress.fraction(), 0.0);
    }
}
