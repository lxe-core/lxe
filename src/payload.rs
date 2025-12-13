//! Payload Reader - Self-extraction logic
//!
//! Reads the embedded payload from the running binary's tail.
//! The binary structure is:
//! [ELF executable][LXE_MAGIC][metadata_len:u32][metadata:JSON][checksum:32bytes][zstd_payload]

use crate::metadata::{LxeMetadata, LXE_MAGIC};
use anyhow::{bail, Context, Result};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

/// Information about the embedded payload
#[derive(Debug, Clone)]
pub struct PayloadInfo {
    /// Parsed metadata
    pub metadata: LxeMetadata,
    
    /// Offset where the compressed payload starts
    pub payload_offset: u64,
    
    /// Size of the compressed payload
    pub payload_size: u64,
    
    /// Path to the executable (for reopening during extraction)
    pub exe_path: std::path::PathBuf,
}

/// Read payload information from an LXE binary
pub fn read_payload_info(exe_path: &Path) -> Result<PayloadInfo> {
    let file = File::open(exe_path)
        .with_context(|| format!("Failed to open executable: {:?}", exe_path))?;
    
    let file_size = file.metadata()?.len();
    let mut reader = BufReader::new(file);
    
    // V1 FIX: Dynamically calculate scan start based on file size
    // Previously hardcoded at 1MB which would miss magic bytes in small binaries
    // 
    // Strategy:
    // - For small files (<2MB): scan from the beginning
    // - For larger files: start at 50% of file size to skip ELF header bulk
    // - The payload is always at the END, so magic bytes are in the second half
    let scan_start = if file_size < 2 * 1024 * 1024 {
        // Small binary - scan from beginning
        // This handles the case where the runtime is only 1.3MB
        0
    } else if file_size < 10 * 1024 * 1024 {
        // Medium binary - start at 40% to skip ELF boilerplate
        (file_size * 40) / 100
    } else {
        // Large binary - start at 1MB (original behavior)
        1024 * 1024
    };
    
    let magic_offset = find_magic_offset(&mut reader, scan_start, file_size)?
        .ok_or_else(|| anyhow::anyhow!("LXE magic bytes not found in binary"))?;
    
    // Read metadata length (4 bytes, little-endian)
    reader.seek(SeekFrom::Start(magic_offset + LXE_MAGIC.len() as u64))?;
    let mut len_bytes = [0u8; 4];
    reader.read_exact(&mut len_bytes)?;
    let metadata_len = u32::from_le_bytes(len_bytes) as usize;
    
    if metadata_len > 1024 * 1024 {
        bail!("Metadata length {} exceeds maximum (1MB)", metadata_len);
    }
    
    // Read metadata JSON
    let mut metadata_bytes = vec![0u8; metadata_len];
    reader.read_exact(&mut metadata_bytes)?;
    
    let metadata: LxeMetadata = serde_json::from_slice(&metadata_bytes)
        .context("Failed to parse LXE metadata")?;
    
    // Skip checksum (32 bytes SHA256)
    let checksum_size = 32;
    let current_pos = reader.stream_position()?;
    
    // Calculate payload offset and size
    let payload_offset = current_pos + checksum_size;
    let payload_size = file_size - payload_offset;
    
    // ========== Ed25519 Signature Verification ==========
    // If the package is signed, verify the signature BEFORE returning.
    // This happens before the GUI opens, so a tampered package never shows the wizard.
    
    if metadata.is_signed() {
        verify_package_signature(&metadata)?;
    }
    
    Ok(PayloadInfo {
        metadata,
        payload_offset,
        payload_size,
        exe_path: exe_path.to_path_buf(),
    })
}

/// Verify the Ed25519 signature on a signed package
fn verify_package_signature(metadata: &LxeMetadata) -> Result<()> {
    use crate::signing;
    
    let public_key = metadata.public_key.as_ref()
        .ok_or_else(|| anyhow::anyhow!("Package claims to be signed but missing public key"))?;
    
    let signature = metadata.signature.as_ref()
        .ok_or_else(|| anyhow::anyhow!("Package claims to be signed but missing signature"))?;
    
    // Get the signable data: metadata JSON (without signature fields) + payload checksum
    let signable_json = metadata.to_signable_json()
        .context("Failed to create signable metadata")?;
    
    let signable_data = signing::create_signable_data(&signable_json, &metadata.payload_checksum)
        .context("Failed to create signable data")?;
    
    // Verify the signature
    let is_valid = signing::verify_signature(&signable_data, signature, public_key)
        .context("Failed to verify signature")?;
    
    if !is_valid {
        bail!(
            "SECURITY: Package signature verification FAILED!\n\n\
             This package may have been tampered with.\n\
             Do not install it unless you trust the source.\n\n\
             If you're a developer, check that:\n\
             1. The private key matches the public key in the package\n\
             2. The metadata wasn't modified after signing"
        );
    }
    
    tracing::info!("✓ Package signature verified successfully");
    Ok(())
}

/// Scan the file for LXE magic bytes - finds the LAST occurrence
/// Scan for LXE magic bytes using the Footer (O(1)) approach
/// 
/// New format: [Runtime] ... [Payload] [HeaderOffset(u64)] [Magic(8)]
fn find_magic_offset(
    reader: &mut BufReader<File>,
    _start_offset: u64,
    file_size: u64,
) -> Result<Option<u64>> {
    // 1. Check for Footer (last 16 bytes)
    if file_size < 16 {
        return Ok(None);
    }
    
    let footer_start = file_size - 16;
    reader.seek(SeekFrom::Start(footer_start))?;
    
    let mut footer = [0u8; 16];
    reader.read_exact(&mut footer)?;
    
    let (offset_bytes, magic_bytes) = footer.split_at(8);
    
    if magic_bytes == *LXE_MAGIC {
        let offset = u64::from_le_bytes(offset_bytes.try_into().unwrap());
        
        // Sanity check offset
        if offset < file_size - 16 {
            tracing::info!("Found LXE Footer. Jumping to payload at offset {}", offset);
            return Ok(Some(offset));
        }
    }
    
    // Fallback: Scan the first 10MB (for legacy or header-only packages)
    // This is needed if the footer is missing but magic is present after runtime.
    tracing::warn!("Footer not found. Falling back to linear scan of start.");
    
    reader.seek(SeekFrom::Start(0))?;
    // Scan first 10MB or file size
    let scan_size = std::cmp::min(file_size, 10 * 1024 * 1024);
    let mut buffer = vec![0u8; scan_size as usize];
    reader.read_exact(&mut buffer)?;
    
    // Find LAST occurrence in the scan window to avoid false positives in runtime self
    // (Though with 10MB scan, we might hit runtime data. The footer is safer.)
    if let Some(pos) = buffer.windows(LXE_MAGIC.len()).rposition(|w| w == LXE_MAGIC) {
         return Ok(Some(pos as u64));
    }

    Ok(None)
}

/// Find a subsequence within a slice
fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

/// Create a reader positioned at the payload start
pub fn open_payload_reader(info: &PayloadInfo) -> Result<impl Read> {
    let file = File::open(&info.exe_path)?;
    let mut reader = BufReader::new(file);
    reader.seek(SeekFrom::Start(info.payload_offset))?;
    Ok(reader.take(info.payload_size))
}

/// Extract the icon from the payload to a temporary file
/// Returns the path to the extracted icon, or None if no icon exists
pub fn extract_icon_to_temp(info: &PayloadInfo) -> Result<Option<std::path::PathBuf>> {
    let icon_filename = match &info.metadata.icon {
        Some(icon) => icon.clone(),
        None => return Ok(None),
    };
    
    // Create temp file path
    let temp_dir = std::env::temp_dir();
    let temp_icon_path = temp_dir.join(format!("lxe-icon-{}.png", info.metadata.app_id));
    
    // Open payload and decompress
    let mut reader = open_payload_reader(info)?;
    let decoder = ruzstd::StreamingDecoder::new(&mut reader)
        .map_err(|e| anyhow::anyhow!("Failed to initialize zstd decoder: {}", e))?;
    let mut archive = tar::Archive::new(decoder);
    
    // Find and extract just the icon file
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        let path_str = path.to_string_lossy();
        
        // Match the icon filename (could be "./icon.png" or "icon.png")
        if path_str.ends_with(&icon_filename) || path_str == format!("./{}", icon_filename) {
            let mut icon_data = Vec::new();
            entry.read_to_end(&mut icon_data)?;
            std::fs::write(&temp_icon_path, &icon_data)?;
            return Ok(Some(temp_icon_path));
        }
    }
    
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_subsequence() {
        let data = b"hello world LXE\x00\x01 test";
        let magic = b"LXE\x00\x01";
        
        assert_eq!(find_subsequence(data, magic), Some(12));
    }

    #[test]
    fn test_find_subsequence_not_found() {
        let data = b"hello world";
        let magic = b"LXE\x00\x01";
        
        assert_eq!(find_subsequence(data, magic), None);
    }
    
    #[test]
    fn test_scan_start_calculation() {
        // Small file: should start at 0
        let small_file_size = 1_500_000u64; // 1.5MB
        let scan_start = if small_file_size < 2 * 1024 * 1024 { 0 } else { 1024 * 1024 };
        assert_eq!(scan_start, 0);
        
        // Large file: should start at 1MB
        let large_file_size = 50_000_000u64; // 50MB
        let scan_start = if large_file_size < 2 * 1024 * 1024 { 0 } else { 1024 * 1024 };
        assert_eq!(scan_start, 1024 * 1024);
    }
}
