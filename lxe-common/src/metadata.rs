//! LXE Package Metadata
//!
//! Defines the structure of LXE package metadata embedded in the binary.

use serde::{Deserialize, Serialize};

/// Magic bytes identifying an LXE payload
/// 
/// The sequence is designed to be unique and not appear in the runtime binary:
/// - 0x00 bytes prevent accidental matches in string literals
/// - Version byte at the end for format compatibility
pub const LXE_MAGIC: &[u8; 8] = b"\x00LXE\xF0\x9F\x93\x01";

/// Current metadata format version
pub const METADATA_VERSION: u8 = 1;

/// Package metadata embedded in the LXE binary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LxeMetadata {
    /// Format version for forward compatibility
    pub format_version: u8,

    /// Application ID (reverse DNS, e.g., "com.discord.Discord")
    pub app_id: String,

    /// Human-readable application name
    pub name: String,

    /// Application version (semver)
    pub version: String,

    /// Target architecture (e.g., "x86_64", "aarch64")
    pub arch: String,

    /// Uncompressed installation size in bytes
    pub install_size: u64,

    /// Relative path to the main executable within the archive
    pub exec: String,

    /// Relative path to the application icon
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,

    /// XDG desktop categories
    #[serde(default)]
    pub categories: Vec<String>,

    /// Short description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// SHA256 checksum of the compressed payload
    pub payload_checksum: String,

    /// Optional: Minimum required LXE runtime version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_runtime_version: Option<String>,

    /// Optional: License identifier (SPDX)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,

    /// Optional: Homepage URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,

    /// Optional: Command-line arguments to pass to exec
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exec_args: Option<String>,

    /// Optional: Whether the app needs terminal
    #[serde(default)]
    pub terminal: bool,
    
    /// Optional: StartupWMClass for GNOME dock (default: derived from app ID)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wm_class: Option<String>,

    /// Optional: Custom installation hooks
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<InstallHooks>,
    
    /// Optional: Installer UI customization
    #[serde(default)]
    pub installer: InstallerMetadata,
    
    // ========== Digital Signature Fields ==========
    
    /// Ed25519 public key (base64-encoded, 32 bytes)
    /// Used to verify the package signature
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    
    /// Ed25519 signature (base64-encoded, 64 bytes)
    /// Signs: signable_metadata_json + payload_checksum_bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// Optional pre/post installation hooks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallHooks {
    /// Script to run before extraction
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre_install: Option<String>,

    /// Script to run after installation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_install: Option<String>,

    /// Script to run before uninstallation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre_uninstall: Option<String>,

    /// Script to run after uninstallation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_uninstall: Option<String>,
}

/// Installer UI customization embedded in the package
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InstallerMetadata {
    /// Custom welcome page title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub welcome_title: Option<String>,
    
    /// Custom welcome page description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub welcome_text: Option<String>,
    
    /// Custom completion page title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_title: Option<String>,
    
    /// Custom completion page description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_text: Option<String>,
    
    /// Accent color in hex format (e.g., "#007ACC")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent_color: Option<String>,
    
    /// Theme preference: "dark", "light", or "auto"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    
    /// Show "Launch" button on completion page (default: true)
    #[serde(default = "default_show_launch")]
    pub show_launch: bool,
    
    // === ADVANCED BRANDING ===
    /// License/EULA text content (embedded in package)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license_text: Option<String>,
    
    /// Banner image filename (embedded in payload)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<String>,
    
    /// Logo image filename (embedded in payload)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo: Option<String>,
    
    /// Allow user to choose custom install directory
    #[serde(default)]
    pub allow_custom_dir: bool,
}

fn default_show_launch() -> bool {
    true
}

impl LxeMetadata {
    /// Create a new metadata instance with required fields
    pub fn new(
        app_id: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
        exec: impl Into<String>,
        install_size: u64,
        payload_checksum: impl Into<String>,
    ) -> Self {
        Self {
            format_version: METADATA_VERSION,
            app_id: app_id.into(),
            name: name.into(),
            version: version.into(),
            arch: std::env::consts::ARCH.to_string(),
            install_size,
            exec: exec.into(),
            icon: None,
            categories: vec!["Application".to_string()],
            description: None,
            payload_checksum: payload_checksum.into(),
            min_runtime_version: None,
            license: None,
            homepage: None,
            exec_args: None,
            terminal: false,
            wm_class: None,
            hooks: None,
            installer: InstallerMetadata::default(),
            public_key: None,
            signature: None,
        }
    }

    /// Get the .desktop file name
    pub fn desktop_filename(&self) -> String {
        format!("{}.desktop", self.app_id)
    }

    /// Get XDG categories as semicolon-separated string
    pub fn categories_string(&self) -> String {
        let mut cats = self.categories.join(";");
        if !cats.is_empty() {
            cats.push(';');
        }
        cats
    }
    
    /// Check if this package is signed
    pub fn is_signed(&self) -> bool {
        self.public_key.is_some() && self.signature.is_some()
    }
    
    /// Create a copy of metadata without signature fields for signing/verification
    /// 
    /// This returns the metadata JSON bytes that should be signed.
    /// The signature covers this JSON + the payload checksum bytes.
    pub fn to_signable_json(&self) -> anyhow::Result<Vec<u8>> {
        // Create a copy without signature fields
        let signable = SignableMetadata {
            format_version: self.format_version,
            app_id: &self.app_id,
            name: &self.name,
            version: &self.version,
            arch: &self.arch,
            install_size: self.install_size,
            exec: &self.exec,
            icon: self.icon.as_deref(),
            categories: &self.categories,
            description: self.description.as_deref(),
            payload_checksum: &self.payload_checksum,
            min_runtime_version: self.min_runtime_version.as_deref(),
            license: self.license.as_deref(),
            homepage: self.homepage.as_deref(),
            exec_args: self.exec_args.as_deref(),
            terminal: self.terminal,
            // NOTE: hooks excluded from signing for simplicity
        };
        
        let json = serde_json::to_vec(&signable)?;
        Ok(json)
    }
}

/// A version of metadata without signature fields, used for signing
/// 
/// This struct is PUBLIC so that both the packer (lxe-pack) and runtime (lxe-runtime)
/// use the exact same definition. This is critical for signature verification -
/// different field order = different JSON = signature mismatch.
#[derive(Serialize)]
pub struct SignableMetadata<'a> {
    pub format_version: u8,
    pub app_id: &'a str,
    pub name: &'a str,
    pub version: &'a str,
    pub arch: &'a str,
    pub install_size: u64,
    pub exec: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<&'a str>,
    pub categories: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<&'a str>,
    pub payload_checksum: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_runtime_version: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exec_args: Option<&'a str>,
    pub terminal: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_serialization() {
        let meta = LxeMetadata::new(
            "com.example.Test",
            "Test App",
            "1.0.0",
            "test-app",
            1024 * 1024,
            "abcd1234",
        );

        let json = serde_json::to_string_pretty(&meta).unwrap();
        let parsed: LxeMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.app_id, "com.example.Test");
        assert_eq!(parsed.name, "Test App");
    }

    #[test]
    fn test_desktop_filename() {
        let meta = LxeMetadata::new(
            "com.discord.Discord",
            "Discord",
            "0.0.1",
            "Discord",
            0,
            "",
        );
        assert_eq!(meta.desktop_filename(), "com.discord.Discord.desktop");
    }
}
