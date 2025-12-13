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
use indicatif::{ProgressBar, ProgressStyle};

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
#[command(version = "2.0.0")]
#[command(about = "LXE - The Universal Linux Package Builder")]
#[command(author = "LXE Project")]
struct Cli {
    /// Suppress all output except errors
    #[arg(short, long, global = true)]
    silent: bool,

    /// Verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

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

    /// Uninstall an LXE application
    Uninstall {
        /// App ID to uninstall (e.g., com.example.app)
        id: String,

        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,

        /// Uninstall system-wide installation (requires privileges)
        #[arg(long)]
        system: bool,
    },

    /// Update the LXE tool itself
    SelfUpdate {
        /// Check for updates without installing
        #[arg(long)]
        check: bool,
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

// Console helper for output control
struct Console {
    silent: bool,
    verbose: bool,
}

impl Console {
    fn new(silent: bool, verbose: bool) -> Self {
        Self { silent, verbose }
    }

    fn log(&self, msg: impl std::fmt::Display) {
        if !self.silent {
            println!("{}", msg);
        }
    }

    fn verbose(&self, msg: impl std::fmt::Display) {
        if self.verbose && !self.silent {
            println!("  {}", msg);
        }
    }

    fn success(&self, msg: impl std::fmt::Display) {
        if !self.silent {
            println!("✅ {}", msg);
        }
    }

    fn warn(&self, msg: impl std::fmt::Display) {
        if !self.silent {
            eprintln!("⚠️  {}", msg);
        }
    }

    fn error(&self, msg: impl std::fmt::Display) {
        eprintln!("❌ {}", msg); // Always print errors
    }

    fn progress_bar(&self, len: u64) -> Option<ProgressBar> {
        if self.silent {
            None
        } else {
            let pb = ProgressBar::new(len);
            pb.set_style(ProgressStyle::with_template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}"
            ).unwrap().progress_chars("=>-"));
            Some(pb)
        }
    }

    fn spinner(&self, msg: &str) -> Option<ProgressBar> {
        if self.silent {
            None
        } else {
            let pb = ProgressBar::new_spinner();
            pb.set_style(ProgressStyle::with_template(
                "{spinner:.green} [{elapsed_precise}] {msg}"
            ).unwrap());
            pb.set_message(msg.to_string());
            pb.enable_steady_tick(std::time::Duration::from_millis(100));
            Some(pb)
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let console = Console::new(cli.silent, cli.verbose);
    
    match cli.command {
        Commands::Build { config, no_script } => {
            cmd_build(config, no_script, &console)
        }
        Commands::Init { preset, name, executable } => {
            cmd_init(preset.as_deref(), &name, &executable, &console)
        }
        Commands::Key { action } => {
            match action {
                KeyAction::Generate { output } => cmd_key_generate(&output, &console),
            }
        }
        Commands::Verify { file } => {
            cmd_verify(&file, &console)
        }
        Commands::Uninstall { id, yes, system } => {
            cmd_uninstall(&id, yes, system, &console)
        }
        Commands::SelfUpdate { check } => {
            cmd_self_update(check, &console)
        }
    }
}

/// Build an LXE package
fn cmd_build(config_path: Option<PathBuf>, no_script: bool, console: &Console) -> Result<()> {
    console.log("🔧 LXE Builder v2.0.0\n");
    
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
    
    console.log(format!("📦 Package: {} v{}", config.package.name, config.package.version));
    console.log(format!("   App ID: {}", config.package.id));
    
    // Run build script if specified
    if let Some(ref script) = config.build.script {
        if no_script {
            console.log("   ⏭️  Skipping build script (--no-script)");
        } else {
            console.log(format!("\n🔨 Running build script: {}", script));
            
            let status = Command::new("sh")
                .arg("-c")
                .arg(script)
                .current_dir(&base_dir)
                .status()
                .context("Failed to run build script")?;
            
            if !status.success() {
                anyhow::bail!("Build script failed with exit code: {:?}", status.code());
            }
            
            console.log("   ✓ Build script completed successfully");
            
            // Validate now that input should exist
            config.validate(&base_dir)?;
        }
    }
    
    let input_path = config.input_path(&base_dir);
    let output_path = config.output_path(&base_dir);
    
    console.log(format!("\n📁 Input: {}", input_path.display()));
    console.log(format!("📄 Output: {}", output_path.display()));
    
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
    console.log("\n📁 Creating archive...");
    let tar_data = create_tar_archive(&input_path)?;
    console.log(format!("   Uncompressed: {} bytes ({:.1} MB)", 
             tar_data.len(), 
             tar_data.len() as f64 / 1024.0 / 1024.0));
    
    // Compress with zstd (with spinner)
    let spinner = console.spinner(&format!("Compressing (level {})...", config.build.compression));
    let compressed = compress_zstd(&tar_data, config.build.compression)?;
    let ratio = tar_data.len() as f64 / compressed.len() as f64;
    if let Some(pb) = spinner {
        pb.finish_with_message(format!("Compressed: {} bytes ({:.1}x ratio)", compressed.len(), ratio));
    } else {
        console.log(format!("   Compressed: {} bytes ({:.1}x ratio)", compressed.len(), ratio));
    }
    
    // Calculate checksum
    let checksum = calculate_sha256(&compressed);
    console.verbose(format!("SHA256: {}", checksum));
    
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
            console.log("🔏 Signing package...");
            sign_metadata(&mut metadata, &key_path, &checksum)?;
            console.log("   ✓ Package signed");
        } else {
            console.warn(format!("Key file not found: {}", key_path.display()));
        }
    }
    
    let metadata_json = serde_json::to_vec(&metadata)?;
    console.verbose(format!("Metadata: {} bytes", metadata_json.len()));
    
    // Get runtime binary
    console.log("🔗 Preparing runtime...");
    let runtime_data = get_runtime_binary(&config.runtime_path(&base_dir))?;
    console.log(format!("   Runtime: {} bytes ({:.1} MB)", 
             runtime_data.len(),
             runtime_data.len() as f64 / 1024.0 / 1024.0));
    
    // Assemble final package
    console.log("🔨 Assembling package...");
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
    
    console.success("Package created successfully!");
    console.log(format!("   📄 {}", output_path.display()));
    console.log(format!("   📊 {} bytes ({:.2} MB)", total_size, total_size as f64 / 1024.0 / 1024.0));
    
    if metadata.get("signature").is_some() {
        console.log("   🔐 Signed: Yes");
    } else {
        console.log("   🔐 Signed: No");
    }
    
    console.log(format!("\n💡 To install: ./{}", output_path.file_name().unwrap().to_string_lossy()));
    
    Ok(())
}

/// Create template lxe.toml
fn cmd_init(preset: Option<&str>, name: &str, executable: &str, console: &Console) -> Result<()> {
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
    
    console.success("Created lxe.toml");
    
    if let Some(p) = preset {
        console.log(format!("   Using preset: {}", p));
    }
    
    console.log("\nNext steps:");
    if preset == Some("tauri") {
        console.log("  1. Update [package] section with your app details");
        console.log("  2. Replace 'my-app' with your binary name in the build script");
        console.log("  3. Run: npm run tauri build && lxe build");
    } else {
        console.log("  1. Edit lxe.toml with your app details");
        console.log("  2. Put your app files in ./dist/");
        console.log("  3. Run: lxe build");
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
fn cmd_key_generate(output: &PathBuf, console: &Console) -> Result<()> {
    if output.exists() {
        anyhow::bail!("Key file already exists: {}", output.display());
    }
    
    console.log("🔑 Generating Ed25519 keypair...");
    
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
    
    console.success("Keypair generated!");
    console.log(format!("   🔒 Private key: {}", output.display()));
    console.log(format!("   🔓 Public key: {}", BASE64_STANDARD.encode(verifying_key.as_bytes())));
    console.warn("Keep your private key secure and never commit it to git!");
    
    Ok(())
}

/// Verify package signature
fn cmd_verify(file: &PathBuf, console: &Console) -> Result<()> {
    console.log(format!("🔍 Verifying: {}\n", file.display()));
    
    // Step 1: Read the package metadata
    let payload_info = lxe::payload::read_payload_info(file)
        .context("Failed to read package")?;
    
    let metadata = &payload_info.metadata;
    
    // Display package info
    console.log("📦 Package Information");
    console.log(format!("   Name: {}", metadata.name));
    console.log(format!("   Version: {}", metadata.version));
    console.log(format!("   App ID: {}", metadata.app_id));
    console.log("");
    
    // Step 2: Check if package is signed
    let (has_signature, has_public_key) = (
        metadata.signature.is_some(),
        metadata.public_key.is_some(),
    );
    
    if !has_signature && !has_public_key {
        console.warn("Package is UNSIGNED");
        console.log("   This package was not signed by the publisher.");
        console.log("   Only install if you trust the source.");
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
    
    console.log("🔑 Signature Information");
    console.log(format!("   Public Key: {}...", &public_key[..20.min(public_key.len())]));
    console.log(format!("   Signature: {}...", &signature[..20.min(signature.len())]));
    console.log("");
    
    // Step 4: Verify the signature
    console.log("🔐 Verifying Signature...");
    
    // Get the signable data (metadata without sig fields + checksum)
    let signable_json = metadata.to_signable_json()
        .context("Failed to create signable metadata")?;
    
    let signable_data = lxe::signing::create_signable_data(&signable_json, &metadata.payload_checksum)
        .context("Failed to create signable data")?;
    
    let is_valid = lxe::signing::verify_signature(&signable_data, signature, public_key)
        .context("Failed to verify signature")?;
    
    if is_valid {
        console.log("   ✅ Signature is VALID");
        console.log("");
        console.log("📊 Payload Integrity");
        console.log(format!("   Checksum: {}...", &metadata.payload_checksum[..16.min(metadata.payload_checksum.len())]));
        console.log("   Status: Verified by signature");
        console.log("");
        console.success("Package is authentic and signed by the publisher.");
        console.log(format!("   Public key: {}", public_key));
    } else {
        console.error("Signature is INVALID");
        console.log("");
        console.error("SECURITY WARNING");
        console.log("   This package may have been tampered with!");
        console.log("   The signature does not match the package contents.");
        console.log("   DO NOT INSTALL this package.");
        anyhow::bail!("Signature verification failed");
    }
    
    Ok(())
}

/// Uninstall an LXE application
fn cmd_uninstall(app_id: &str, yes: bool, system: bool, console: &Console) -> Result<()> {
    console.log(format!("🧹 Uninstalling: {}\n", app_id));
    
    // Build config based on install type
    let config = if system {
        console.log("   Mode: System-wide");
        lxe::installer::InstallConfig::system()
    } else {
        console.log("   Mode: User-local");
        lxe::installer::InstallConfig::user_local()
    };
    
    // Check if installed
    let app_dir = config.app_dir(app_id);
    if !app_dir.exists() {
        anyhow::bail!("Application not found: {}\n\nNo installation found at: {:?}", app_id, app_dir);
    }
    
    console.log(format!("   Found: {:?}", app_dir));
    
    // Confirmation prompt (unless --yes or --silent)
    if !yes && !console.silent {
        print!("\n⚠️  Are you sure you want to uninstall {}? [y/N] ", app_id);
        std::io::Write::flush(&mut std::io::stdout())?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            console.log("\nCancelled.");
            return Ok(());
        }
    }
    
    // Execute uninstall (async runtime needed)
    console.log("\nRemoving files...");
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(lxe::installer::uninstall(app_id, &config))?;
    
    console.success(format!("{} has been uninstalled.", app_id));
    Ok(())
}

/// Self-update the LXE tool
fn cmd_self_update(check_only: bool, console: &Console) -> Result<()> {
    use self_update::cargo_crate_version;
    
    console.log("🔄 Checking for updates...\n");
    console.log(format!("   Current version: v{}", cargo_crate_version!()));
    
    let releases = self_update::backends::github::ReleaseList::configure()
        .repo_owner("lxe-core")
        .repo_name("lxe")
        .build()?
        .fetch()?;
    
    if releases.is_empty() {
        console.log("   No releases found on GitHub.");
        return Ok(());
    }
    
    let latest = &releases[0];
    console.log(format!("   Latest version: v{}", latest.version));
    
    if latest.version == cargo_crate_version!() {
        console.success("You are running the latest version.");
        return Ok(());
    }
    
    if check_only {
        console.log(format!("\n💡 Run `lxe self-update` to install v{}", latest.version));
        return Ok(());
    }
    
    console.log("\n📦 Downloading update...");
    
    let status = self_update::backends::github::Update::configure()
        .repo_owner("lxe-core")
        .repo_name("lxe")
        .bin_name("lxe")
        .show_download_progress(!console.silent)
        .current_version(cargo_crate_version!())
        .build()?
        .update()?;
    
    console.success(format!("Updated to v{}!", status.version()));
    console.log("\n🎉 Please restart the terminal to use the new version.");
    
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
