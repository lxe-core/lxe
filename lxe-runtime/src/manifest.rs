//! Manifest System for Tracking Installed Applications
//!
//! This module provides manifest file management for tracking what files
//! LXE has installed, enabling clean uninstallation.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;

/// Manifest data for an installed application
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallManifest {
    /// Application ID (e.g., "com.example.app")
    pub app_id: String,
    
    /// Application Display Name (e.g., "Visual Studio Code")
    #[serde(default)]
    pub name: Option<String>,
    
    /// Version that was installed
    pub version: String,
    
    /// Timestamp of installation (ISO 8601)
    pub installed_at: String,
    
    /// Whether this is a system-wide installation
    pub is_system: bool,
    
    /// List of all installed files and directories
    pub files: Vec<String>,
}

impl InstallManifest {
    /// Create a new manifest for an app
    pub fn new(app_id: String, name: Option<String>, version: String, is_system: bool) -> Self {
        Self {
            app_id,
            name,
            version,
            installed_at: chrono_lite_now(),
            is_system,
            files: Vec::new(),
        }
    }
    
    /// Add a file path to the manifest
    pub fn add_file(&mut self, path: impl AsRef<Path>) {
        self.files.push(path.as_ref().display().to_string());
    }
    
    /// Get the manifest directory for user installs
    pub fn manifests_dir() -> PathBuf {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("~/.local/share"))
            .join("lxe")
            .join("manifests")
    }
    
    /// Get the manifest file path for an app
    pub fn manifest_path(app_id: &str) -> PathBuf {
        Self::manifests_dir().join(format!("{}.json", app_id))
    }
    
    /// Save the manifest to disk
    pub async fn save(&self) -> Result<PathBuf> {
        let dir = Self::manifests_dir();
        fs::create_dir_all(&dir).await
            .context("Failed to create manifests directory")?;
        
        let path = Self::manifest_path(&self.app_id);
        let json = serde_json::to_string_pretty(self)
            .context("Failed to serialize manifest")?;
        
        fs::write(&path, json).await
            .context("Failed to write manifest file")?;
        
        tracing::info!("Saved manifest to {:?}", path);
        Ok(path)
    }
    
    /// Load a manifest from disk
    pub async fn load(app_id: &str) -> Result<Option<Self>> {
        let path = Self::manifest_path(app_id);
        
        if !path.exists() {
            return Ok(None);
        }
        
        let json = fs::read_to_string(&path).await
            .context("Failed to read manifest file")?;
        
        let manifest: Self = serde_json::from_str(&json)
            .context("Failed to parse manifest JSON")?;
        
        Ok(Some(manifest))
    }
    
    /// Delete the manifest file
    pub async fn delete(app_id: &str) -> Result<()> {
        let path = Self::manifest_path(app_id);
        
        if path.exists() {
            fs::remove_file(&path).await
                .context("Failed to delete manifest file")?;
            tracing::info!("Deleted manifest: {:?}", path);
        }
        
        Ok(())
    }
    
    /// List all installed app IDs
    pub async fn list_installed() -> Result<Vec<String>> {
        let dir = Self::manifests_dir();
        
        if !dir.exists() {
            return Ok(Vec::new());
        }
        
        let mut apps = Vec::new();
        let mut entries = fs::read_dir(&dir).await?;
        
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if let Some(stem) = path.file_stem() {
                    apps.push(stem.to_string_lossy().to_string());
                }
            }
        }
        
        Ok(apps)
    }
}

/// Simple ISO 8601-like timestamp without external crate
fn chrono_lite_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    
    // Simple Unix timestamp - good enough for our purposes
    format!("unix:{}", duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_manifest_creation() {
        let mut manifest = InstallManifest::new(
            "com.example.app".to_string(),
            Some("Example App".to_string()),
            "1.0.0".to_string(),
            false,
        );
        
        manifest.add_file("/home/user/.local/share/com.example.app/bin/app");
        manifest.add_file("/home/user/.local/share/applications/com.example.app.desktop");
        
        assert_eq!(manifest.files.len(), 2);
        assert!(manifest.installed_at.starts_with("unix:"));
    }
}
