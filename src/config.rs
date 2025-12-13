//! LXE Configuration Parser
//!
//! Parses lxe.toml files for declarative package configuration.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The main configuration structure matching lxe.toml
#[derive(Debug, Deserialize)]
pub struct LxeConfig {
    pub package: PackageConfig,
    #[serde(default)]
    pub build: BuildConfig,
    #[serde(default)]
    pub runtime: RuntimeConfig,
    #[serde(default)]
    pub security: SecurityConfig,
}

/// Package metadata
#[derive(Debug, Deserialize)]
pub struct PackageConfig {
    /// Human-readable application name
    pub name: String,
    
    /// Reverse-DNS application ID (e.g., "com.example.myapp")
    pub id: String,
    
    /// Semantic version
    pub version: String,
    
    /// Path to executable relative to input directory
    pub executable: String,
    
    /// Path to icon relative to input directory (optional)
    #[serde(default)]
    pub icon: Option<String>,
    
    /// Application description (optional)
    #[serde(default)]
    pub description: Option<String>,
    
    /// Desktop categories (optional)
    #[serde(default)]
    pub categories: Vec<String>,
    
    /// Run in terminal (default: false)
    #[serde(default)]
    pub terminal: bool,
}

/// Build configuration
#[derive(Debug, Deserialize)]
pub struct BuildConfig {
    /// Directory containing files to package
    #[serde(default = "default_input")]
    pub input: String,
    
    /// Optional build script to run before packaging
    #[serde(default)]
    pub script: Option<String>,
    
    /// Zstd compression level (1-22, default: 19)
    #[serde(default = "default_compression")]
    pub compression: i32,
    
    /// Output file path (default: ./<name>.lxe)
    #[serde(default)]
    pub output: Option<String>,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            input: default_input(),
            script: None,
            compression: default_compression(),
            output: None,
        }
    }
}

/// Runtime configuration
#[derive(Debug, Deserialize, Default)]
pub struct RuntimeConfig {
    /// Path to custom runtime binary (optional)
    #[serde(default)]
    pub path: Option<String>,
}

/// Security/signing configuration
#[derive(Debug, Deserialize, Default)]
pub struct SecurityConfig {
    /// Path to Ed25519 private key for signing (optional)
    #[serde(default)]
    pub key: Option<String>,
}

fn default_input() -> String {
    "./dist".to_string()
}

fn default_compression() -> i32 {
    19
}

impl LxeConfig {
    /// Load configuration from a file path
    pub fn from_file(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;
        
        Self::from_str(&contents)
    }
    
    /// Load configuration from the current directory
    pub fn from_current_dir() -> Result<Self> {
        let config_path = std::env::current_dir()?.join("lxe.toml");
        
        if !config_path.exists() {
            anyhow::bail!(
                "No lxe.toml found in current directory.\n\
                 Run 'lxe init' to create one, or specify a path with --config"
            );
        }
        
        Self::from_file(&config_path)
    }
    
    /// Parse configuration from a TOML string
    pub fn from_str(toml_str: &str) -> Result<Self> {
        toml::from_str(toml_str).context("Failed to parse lxe.toml")
    }
    
    /// Get the resolved input directory path
    pub fn input_path(&self, base_dir: &Path) -> PathBuf {
        base_dir.join(&self.build.input)
    }
    
    /// Get the resolved output file path
    pub fn output_path(&self, base_dir: &Path) -> PathBuf {
        if let Some(ref output) = self.build.output {
            base_dir.join(output)
        } else {
            // Default: <name>.lxe in current directory
            let filename = format!("{}.lxe", self.package.id.split('.').last().unwrap_or("app"));
            base_dir.join(filename)
        }
    }
    
    /// Get the resolved runtime path (if specified)
    pub fn runtime_path(&self, base_dir: &Path) -> Option<PathBuf> {
        self.runtime.path.as_ref().map(|p| base_dir.join(p))
    }
    
    /// Get the resolved signing key path (if specified)
    pub fn key_path(&self, base_dir: &Path) -> Option<PathBuf> {
        self.security.key.as_ref().map(|p| base_dir.join(p))
    }
    
    /// Validate the configuration
    pub fn validate(&self, base_dir: &Path) -> Result<()> {
        let input = self.input_path(base_dir);
        
        // Check input directory exists (only if no build script)
        if self.build.script.is_none() && !input.exists() {
            anyhow::bail!(
                "Input directory does not exist: {}\n\
                 Either create it, or add a [build] script to generate it.",
                input.display()
            );
        }
        
        // Validate compression level
        if !(1..=22).contains(&self.build.compression) {
            anyhow::bail!(
                "Compression level must be between 1 and 22, got: {}",
                self.build.compression
            );
        }
        
        // Validate app ID format (basic check)
        if !self.package.id.contains('.') {
            anyhow::bail!(
                "App ID should be in reverse-DNS format (e.g., 'com.example.myapp'), got: {}",
                self.package.id
            );
        }
        
        // ICON VALIDATION: Ensure packages always have working icons
        if input.exists() {
            if let Some(ref icon) = self.package.icon {
                let icon_path = input.join(icon);
                if !icon_path.exists() {
                    anyhow::bail!(
                        "❌ Icon file not found: {}\n\n\
                         The icon '{}' was specified in lxe.toml but doesn't exist in the input directory.\n\n\
                         To fix this, either:\n\
                         1. Copy your icon to: {}\n\
                         2. Update the 'icon' field in lxe.toml to point to the correct file\n\
                         3. Add the icon copy to your build script\n\n\
                         Example lxe.toml:\n\
                         [package]\n\
                         icon = \"icon.png\"  # Must exist in input directory",
                        icon_path.display(),
                        icon,
                        icon_path.display()
                    );
                }
            } else {
                // No icon specified - warn the user
                eprintln!(
                    "⚠️  Warning: No icon specified in lxe.toml\n\
                     Your package will use a generic icon in the system menu.\n\
                     Add 'icon = \"icon.png\"' to [package] for better UX.\n"
                );
            }
        }
        
        Ok(())
    }
}

/// Generate a template lxe.toml file
pub fn generate_template(name: &str, executable: &str) -> String {
    format!(r#"# LXE Package Configuration
# Documentation: https://github.com/lxe-core/lxe

[package]
name = "{name}"
id = "com.example.{id}"
version = "1.0.0"
executable = "{executable}"
icon = "icon.png"
description = "A cross-platform application"
categories = ["Utility"]
terminal = false

[build]
# Directory containing your application files
input = "./dist"

# Optional: Command to run before packaging
# script = "npm run build"

# Compression level (1-22, higher = smaller but slower)
compression = 19

# Optional: Custom output path
# output = "./release/myapp.lxe"

[runtime]
# Optional: Path to custom LXE runtime
# path = "./lxe-runtime"

[security]
# Optional: Path to Ed25519 signing key
# key = "./lxe-signing.key"
"#,
        name = name,
        id = name.to_lowercase().replace(' ', "-"),
        executable = executable,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_minimal_config() {
        let toml = r#"
            [package]
            name = "Test App"
            id = "com.test.app"
            version = "1.0.0"
            executable = "run.sh"
        "#;
        
        let config = LxeConfig::from_str(toml).unwrap();
        assert_eq!(config.package.name, "Test App");
        assert_eq!(config.package.id, "com.test.app");
        assert_eq!(config.build.compression, 19); // default
    }
    
    #[test]
    fn test_parse_full_config() {
        let toml = r#"
            [package]
            name = "Full App"
            id = "com.full.app"
            version = "2.0.0"
            executable = "app"
            icon = "assets/icon.png"
            description = "A full test"
            categories = ["Development", "Utility"]
            terminal = true
            
            [build]
            input = "./build"
            script = "make build"
            compression = 22
            output = "./out/app.lxe"
            
            [security]
            key = "./keys/private.pem"
        "#;
        
        let config = LxeConfig::from_str(toml).unwrap();
        assert_eq!(config.package.terminal, true);
        assert_eq!(config.build.compression, 22);
        assert!(config.security.key.is_some());
    }
}
