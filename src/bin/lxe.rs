//! LXE CLI - The Universal Linux Package Builder
//!
//! Usage:
//!   lxe build              Build package from lxe.toml in current directory
//!   lxe init               Create a template lxe.toml
//!   lxe key generate       Generate Ed25519 signing keypair
//!   lxe verify <file.lxe>  Verify package signature

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Command;

// Import from the library crate
use lxe::config::{LxeConfig, generate_template};

// Re-use signing and compression from the library
use ed25519_dalek::SigningKey;
use base64::prelude::*;
use rand::rngs::OsRng;
use sha2::{Sha256, Digest};

/// Magic bytes identifying LXE payload (must match runtime)
const LXE_MAGIC: &[u8; 8] = b"\x00LXE\xF0\x9F\x93\x01";

#[derive(Parser)]
#[command(name = "lxe")]
#[command(version = "0.2.0")]
#[command(about = "LXE - The Universal Linux Package Builder")]
#[command(author = "LXE Project")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build an LXE package from lxe.toml
    Build {
        /// Path to lxe.toml (default: ./lxe.toml)
        #[arg(short, long)]
        config: Option<PathBuf>,
        
        /// Skip running the build script
        #[arg(long)]
        no_script: bool,
        
        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },
    
    /// Create a template lxe.toml in current directory
    Init {
        /// Framework preset: tauri, python, electron, generic
        #[arg(short, long)]
        preset: Option<String>,
        
        /// Application name (for generic template)
        #[arg(short, long, default_value = "My App")]
        name: String,
        
        /// Main executable name (for generic template)
        #[arg(short, long, default_value = "run.sh")]
        executable: String,
    },
    
    /// Key management
    Key {
        #[command(subcommand)]
        action: KeyAction,
    },
    
    /// Verify a signed package
    Verify {
        /// Path to .lxe file
        file: PathBuf,
    },
}

#[derive(Subcommand)]
enum KeyAction {
    /// Generate a new Ed25519 signing keypair
    Generate {
        /// Output path for the key file
        #[arg(short, long, default_value = "lxe-signing.key")]
        output: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Build { config, no_script, verbose } => {
            cmd_build(config, no_script, verbose)
        }
        Commands::Init { preset, name, executable } => {
            cmd_init(preset.as_deref(), &name, &executable)
        }
        Commands::Key { action } => {
            match action {
                KeyAction::Generate { output } => cmd_key_generate(&output),
            }
        }
        Commands::Verify { file } => {
            cmd_verify(&file)
        }
    }
}

/// Build an LXE package
fn cmd_build(config_path: Option<PathBuf>, no_script: bool, verbose: bool) -> Result<()> {
    println!("🔧 LXE Builder v0.2.0\n");
    
    // Load configuration
    let base_dir = std::env::current_dir()?;
    let config = if let Some(path) = config_path {
        LxeConfig::from_file(&path)?
    } else {
        LxeConfig::from_current_dir()?
    };
    
    // Validate
    if !no_script && config.build.script.is_some() {
        // Will validate after script runs
    } else {
        config.validate(&base_dir)?;
    }
    
    println!("📦 Package: {} v{}", config.package.name, config.package.version);
    println!("   App ID: {}", config.package.id);
    
    // Run build script if specified
    if let Some(ref script) = config.build.script {
        if no_script {
            println!("   ⏭️  Skipping build script (--no-script)");
        } else {
            println!("\n🔨 Running build script: {}", script);
            
            let status = Command::new("sh")
                .arg("-c")
                .arg(script)
                .current_dir(&base_dir)
                .status()
                .context("Failed to run build script")?;
            
            if !status.success() {
                anyhow::bail!("Build script failed with exit code: {:?}", status.code());
            }
            
            println!("   ✓ Build script completed successfully");
            
            // Validate now that input should exist
            config.validate(&base_dir)?;
        }
    }
    
    let input_path = config.input_path(&base_dir);
    let output_path = config.output_path(&base_dir);
    
    println!("\n📁 Input: {}", input_path.display());
    println!("📄 Output: {}", output_path.display());
    
    // Check executable exists in input
    let exec_path = input_path.join(&config.package.executable);
    if !exec_path.exists() {
        anyhow::bail!(
            "Executable not found: {}\n\
             Make sure '{}' exists in '{}'",
            exec_path.display(),
            config.package.executable,
            input_path.display()
        );
    }
    
    // Create tar archive
    println!("\n📁 Creating archive...");
    let tar_data = create_tar_archive(&input_path)?;
    println!("   Uncompressed: {} bytes ({:.1} MB)", 
             tar_data.len(), 
             tar_data.len() as f64 / 1024.0 / 1024.0);
    
    // Compress with zstd
    println!("🗜️  Compressing (level {})...", config.build.compression);
    let compressed = compress_zstd(&tar_data, config.build.compression)?;
    let ratio = tar_data.len() as f64 / compressed.len() as f64;
    println!("   Compressed: {} bytes ({:.1}x ratio)", compressed.len(), ratio);
    
    // Calculate checksum
    let checksum = calculate_sha256(&compressed);
    if verbose {
        println!("   SHA256: {}", checksum);
    }
    
    // Build metadata JSON
    let categories: Vec<String> = config.package.categories.clone();
    let mut metadata = serde_json::json!({
        "format_version": 1,
        "app_id": config.package.id,
        "name": config.package.name,
        "version": config.package.version,
        "arch": std::env::consts::ARCH,
        "install_size": tar_data.len(),
        "exec": config.package.executable,
        "icon": config.package.icon,
        "description": config.package.description,
        "categories": categories,
        "terminal": config.package.terminal,
        "payload_checksum": checksum,
    });
    
    // Sign if key provided
    if let Some(key_path) = config.key_path(&base_dir) {
        if key_path.exists() {
            println!("🔏 Signing package...");
            sign_metadata(&mut metadata, &key_path, &checksum)?;
            println!("   ✓ Package signed");
        } else {
            println!("   ⚠️  Key file not found: {}", key_path.display());
        }
    }
    
    let metadata_json = serde_json::to_vec(&metadata)?;
    println!("   Metadata: {} bytes", metadata_json.len());
    
    // Get runtime binary
    println!("🔗 Preparing runtime...");
    let runtime_data = get_runtime_binary(&config.runtime_path(&base_dir))?;
    println!("   Runtime: {} bytes ({:.1} MB)", 
             runtime_data.len(),
             runtime_data.len() as f64 / 1024.0 / 1024.0);
    
    // Assemble final package
    println!("🔨 Assembling package...");
    let mut output_file = File::create(&output_path)?;
    
    // [Runtime Binary]
    output_file.write_all(&runtime_data)?;
    
    // [Magic Bytes] - header marker
    output_file.write_all(LXE_MAGIC)?;
    
    // [Metadata Length (u32 LE)]
    let metadata_len = metadata_json.len() as u32;
    output_file.write_all(&metadata_len.to_le_bytes())?;
    
    // [Metadata JSON]
    output_file.write_all(&metadata_json)?;
    
    // [Checksum (32 bytes)]
    let checksum_bytes = hex::decode(&checksum)?;
    output_file.write_all(&checksum_bytes)?;
    
    // [Compressed Payload]
    output_file.write_all(&compressed)?;
    
    // [Footer: HeaderOffset (u64 LE) + Magic]
    let header_offset = runtime_data.len() as u64;
    output_file.write_all(&header_offset.to_le_bytes())?;
    output_file.write_all(LXE_MAGIC)?;
    
    output_file.flush()?;
    
    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&output_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&output_path, perms)?;
    }
    
    let total_size = fs::metadata(&output_path)?.len();
    
    println!("\n✅ Package created successfully!");
    println!("   📄 {}", output_path.display());
    println!("   📊 {} bytes ({:.2} MB)", total_size, total_size as f64 / 1024.0 / 1024.0);
    
    if metadata.get("signature").is_some() {
        println!("   🔐 Signed: Yes");
    } else {
        println!("   🔐 Signed: No");
    }
    
    println!("\n💡 To install: ./{}", output_path.file_name().unwrap().to_string_lossy());
    
    Ok(())
}

/// Create template lxe.toml
fn cmd_init(preset: Option<&str>, name: &str, executable: &str) -> Result<()> {
    let config_path = std::env::current_dir()?.join("lxe.toml");
    
    if config_path.exists() {
        anyhow::bail!("lxe.toml already exists in this directory");
    }
    
    let template = match preset {
        Some("tauri") => TAURI_TEMPLATE.to_string(),
        Some("python") => PYTHON_TEMPLATE.to_string(),
        Some("electron") => ELECTRON_TEMPLATE.to_string(),
        Some("generic") | None => generate_template(name, executable),
        Some(other) => {
            anyhow::bail!(
                "Unknown preset: '{}'\n\
                 Available presets: tauri, python, electron, generic",
                other
            );
        }
    };
    
    fs::write(&config_path, &template)?;
    
    println!("✅ Created lxe.toml");
    
    if let Some(p) = preset {
        println!("   Using preset: {}", p);
    }
    
    println!("\nNext steps:");
    if preset == Some("tauri") {
        println!("  1. Update [package] section with your app details");
        println!("  2. Replace 'my-app' with your binary name in the build script");
        println!("  3. Run: npm run tauri build && lxe build");
    } else {
        println!("  1. Edit lxe.toml with your app details");
        println!("  2. Put your app files in ./dist/");
        println!("  3. Run: lxe build");
    }
    
    Ok(())
}

// Embedded templates for presets
const TAURI_TEMPLATE: &str = r#"# LXE Configuration for Tauri 2.x Applications
#
# IMPORTANT: Tauri sidecars have specific requirements!
# This template handles them correctly.

[package]
name = "My Tauri App"
id = "com.example.myapp"
version = "1.0.0"
executable = "my-app"
icon = "icon.png"
description = "A Tauri desktop application"
categories = ["Utility"]
terminal = false

[build]
input = "./dist"

# Build script that correctly handles Tauri structure:
# - Main binary from target/release
# - Sidecars renamed WITHOUT target triple (critical!)
script = """
rm -rf dist && mkdir -p dist && \
cp src-tauri/target/release/my-app dist/ && \
if [ -f src-tauri/binaries/server-x86_64-unknown-linux-gnu ]; then \
    cp src-tauri/binaries/server-x86_64-unknown-linux-gnu dist/server && \
    chmod +x dist/server; \
fi && \
cp src-tauri/icons/128x128.png dist/icon.png 2>/dev/null || \
cp src-tauri/icons/icon.png dist/icon.png 2>/dev/null || echo "No icon"
"""

compression = 19
"#;

const PYTHON_TEMPLATE: &str = r#"# LXE Configuration for Python Applications

[package]
name = "My Python App"
id = "com.example.myapp"
version = "1.0.0"
executable = "main"
icon = "icon.png"
description = "A Python application"
categories = ["Utility"]
terminal = false

[build]
input = "./dist"

script = """
rm -rf dist && mkdir -p dist && \
python3 -m venv venv 2>/dev/null || true && \
source venv/bin/activate && \
pip install -q pyinstaller && \
pip install -q -r requirements.txt 2>/dev/null || true && \
pyinstaller --onefile --name main --clean main.py --distpath dist && \
cp icon.png dist/ 2>/dev/null || echo "No icon"
"""

compression = 19
"#;

const ELECTRON_TEMPLATE: &str = r#"# LXE Configuration for Electron Applications

[package]
name = "My Electron App"
id = "com.example.myapp"
version = "1.0.0"
executable = "my-app"
icon = "icon.png"
description = "An Electron application"
categories = ["Utility"]
terminal = false

[build]
input = "./dist"

script = """
rm -rf dist && \
cp -r release/linux-unpacked dist && \
cp build/icon.png dist/ 2>/dev/null || echo "No icon"
"""

compression = 10
"#;

/// Generate signing keypair
fn cmd_key_generate(output: &PathBuf) -> Result<()> {
    if output.exists() {
        anyhow::bail!("Key file already exists: {}", output.display());
    }
    
    println!("🔑 Generating Ed25519 keypair...");
    
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    
    // Encode: 32-byte seed + 32-byte public key
    let mut key_bytes = [0u8; 64];
    key_bytes[..32].copy_from_slice(signing_key.as_bytes());
    key_bytes[32..].copy_from_slice(verifying_key.as_bytes());
    
    let encoded = BASE64_STANDARD.encode(&key_bytes);
    fs::write(output, &encoded)?;
    
    // Set restrictive permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(output, perms)?;
    }
    
    println!("\n✅ Keypair generated!");
    println!("   🔒 Private key: {}", output.display());
    println!("   🔓 Public key: {}", BASE64_STANDARD.encode(verifying_key.as_bytes()));
    println!("\n⚠️  Keep your private key secure and never commit it to git!");
    
    Ok(())
}

/// Verify package signature
fn cmd_verify(file: &PathBuf) -> Result<()> {
    println!("🔍 Verifying: {}\n", file.display());
    
    // Step 1: Read the package metadata
    let payload_info = lxe::payload::read_payload_info(file)
        .context("Failed to read package")?;
    
    let metadata = &payload_info.metadata;
    
    // Display package info
    println!("📦 Package Information");
    println!("   Name: {}", metadata.name);
    println!("   Version: {}", metadata.version);
    println!("   App ID: {}", metadata.app_id);
    println!();
    
    // Step 2: Check if package is signed
    let (has_signature, has_public_key) = (
        metadata.signature.is_some(),
        metadata.public_key.is_some(),
    );
    
    if !has_signature && !has_public_key {
        println!("⚠️  Package is UNSIGNED");
        println!("   This package was not signed by the publisher.");
        println!("   Only install if you trust the source.");
        return Ok(());
    }
    
    if has_signature != has_public_key {
        anyhow::bail!(
            "Invalid package: has {} but not {}",
            if has_signature { "signature" } else { "public_key" },
            if has_public_key { "signature" } else { "public_key" }
        );
    }
    
    // Step 3: Display signature info
    let public_key = metadata.public_key.as_ref().unwrap();
    let signature = metadata.signature.as_ref().unwrap();
    
    println!("🔑 Signature Information");
    println!("   Public Key: {}...", &public_key[..20.min(public_key.len())]);
    println!("   Signature: {}...", &signature[..20.min(signature.len())]);
    println!();
    
    // Step 4: Verify the signature
    println!("🔐 Verifying Signature...");
    
    // Get the signable data (metadata without sig fields + checksum)
    let signable_json = metadata.to_signable_json()
        .context("Failed to create signable metadata")?;
    
    let signable_data = lxe::signing::create_signable_data(&signable_json, &metadata.payload_checksum)
        .context("Failed to create signable data")?;
    
    let is_valid = lxe::signing::verify_signature(&signable_data, signature, public_key)
        .context("Failed to verify signature")?;
    
    if is_valid {
        println!("   ✅ Signature is VALID");
        println!();
        println!("📊 Payload Integrity");
        println!("   Checksum: {}...", &metadata.payload_checksum[..16.min(metadata.payload_checksum.len())]);
        println!("   Status: Verified by signature");
        println!();
        println!("✅ Package is authentic and signed by the publisher.");
        println!("   Public key: {}", public_key);
    } else {
        println!("   ❌ Signature is INVALID");
        println!();
        println!("🚨 SECURITY WARNING");
        println!("   This package may have been tampered with!");
        println!("   The signature does not match the package contents.");
        println!("   DO NOT INSTALL this package.");
        anyhow::bail!("Signature verification failed");
    }
    
    Ok(())
}

// === Helper Functions ===

fn create_tar_archive(input_dir: &PathBuf) -> Result<Vec<u8>> {
    let mut archive_data = Vec::new();
    
    {
        let mut builder = tar::Builder::new(&mut archive_data);
        builder.follow_symlinks(false);
        builder.append_dir_all(".", input_dir)
            .context("Failed to add directory to tar archive")?;
        builder.finish()
            .context("Failed to finish tar archive")?;
    }
    
    Ok(archive_data)
}

#[cfg(feature = "packer")]
fn compress_zstd(data: &[u8], level: i32) -> Result<Vec<u8>> {
    zstd::encode_all(std::io::Cursor::new(data), level)
        .context("Failed to compress with zstd")
}

#[cfg(not(feature = "packer"))]
fn compress_zstd(_data: &[u8], _level: i32) -> Result<Vec<u8>> {
    anyhow::bail!("Packer feature not enabled. Build with: cargo build --features packer")
}

fn calculate_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

fn get_runtime_binary(custom_path: &Option<PathBuf>) -> Result<Vec<u8>> {
    // Check custom path first
    if let Some(path) = custom_path {
        if path.exists() {
            let mut file = File::open(path)?;
            let mut data = Vec::new();
            file.read_to_end(&mut data)?;
            return Ok(data);
        }
    }
    
    // Look for lxe-runtime in same directory as this binary
    let current_exe = std::env::current_exe()?;
    let runtime_path = current_exe.parent()
        .map(|p| p.join("lxe-runtime"))
        .filter(|p| p.exists());
    
    if let Some(path) = runtime_path {
        let mut file = File::open(&path)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        return Ok(data);
    }
    
    anyhow::bail!(
        "LXE runtime not found.\n\
         Either:\n\
         - Place 'lxe-runtime' in the same directory as 'lxe'\n\
         - Or specify [runtime] path in lxe.toml"
    )
}

fn sign_metadata(
    metadata: &mut serde_json::Value,
    key_path: &PathBuf,
    checksum: &str,
) -> Result<()> {
    // Load key
    let contents = fs::read_to_string(key_path)
        .with_context(|| format!("Failed to read key: {}", key_path.display()))?;
    
    let key_bytes = BASE64_STANDARD.decode(contents.trim())
        .context("Invalid base64 in key file")?;
    
    if key_bytes.len() != 64 {
        anyhow::bail!("Invalid key file format");
    }
    
    let seed: [u8; 32] = key_bytes[..32].try_into()?;
    let signing_key = SigningKey::from_bytes(&seed);
    
    // Create signable data using the EXACT same struct as verification
    // We need to extract fields from the Value because we're working with the raw JSON value here
    use lxe::metadata::SignableMetadata;
    
    let app_id = metadata["app_id"].as_str().ok_or(anyhow::anyhow!("Missing app_id"))?;
    let name = metadata["name"].as_str().ok_or(anyhow::anyhow!("Missing name"))?;
    let version = metadata["version"].as_str().ok_or(anyhow::anyhow!("Missing version"))?;
    let arch = metadata["arch"].as_str().ok_or(anyhow::anyhow!("Missing arch"))?;
    let install_size = metadata["install_size"].as_u64().ok_or(anyhow::anyhow!("Missing install_size"))?;
    let exec = metadata["exec"].as_str().ok_or(anyhow::anyhow!("Missing exec"))?;
    let icon = metadata["icon"].as_str();
    let description = metadata["description"].as_str();
    let payload_checksum = metadata["payload_checksum"].as_str().ok_or(anyhow::anyhow!("Missing payload_checksum"))?;
    let terminal = metadata["terminal"].as_bool().unwrap_or(false);
    
    // Convert categories array
    let categories: Vec<String> = metadata["categories"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
        
    let signable = SignableMetadata {
        format_version: 1,
        app_id,
        name,
        version,
        arch,
        install_size,
        exec,
        icon,
        categories: &categories,
        description,
        payload_checksum,
        min_runtime_version: None,
        license: None,
        homepage: None,
        exec_args: None,
        terminal,
    };
    
    let signable_json = serde_json::to_vec(&signable)?;
    
    // Create final blob to sign (metadata + checksum bytes)
    let checksum_bytes = hex::decode(checksum)?;
    let mut signable_data = signable_json;
    signable_data.extend_from_slice(&checksum_bytes);
    
    // Sign
    use ed25519_dalek::Signer;
    let signature = signing_key.sign(&signable_data);
    
    // Add to metadata
    metadata["signature"] = serde_json::Value::String(
        BASE64_STANDARD.encode(signature.to_bytes())
    );
    metadata["public_key"] = serde_json::Value::String(
        BASE64_STANDARD.encode(signing_key.verifying_key().as_bytes())
    );
    
    Ok(())
}
