//! LXE Packer - Create .lxe packages from directories
//!
//! This tool compresses a directory with zstd and attaches it to the
//! LXE runtime to create a self-extracting installer.
//!
//! ## Ed25519 Signing
//!
//! Packages can be signed with Ed25519 for security:
//! ```bash
//! # Generate a keypair (once)
//! lxe-pack --generate-key --key-output ~/.lxe/signing.key
//!
//! # Sign a package
//! lxe-pack --private-key ~/.lxe/signing.key --input ./app --output app.lxe
//! ```

use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use clap::Parser;
use ed25519_dalek::{SigningKey, Signer};
use lxe::metadata::SignableMetadata;  // Import from shared library
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;

/// LXE Packer - Create self-extracting Linux packages
#[derive(Parser, Debug)]
#[command(name = "lxe-pack")]
#[command(about = "Package applications into .lxe self-extracting installers")]
#[command(version)]
struct Args {
    /// Application ID (reverse DNS format, e.g., com.discord.Discord)
    #[arg(long, required_unless_present = "generate_key")]
    app_id: Option<String>,

    /// Application name
    #[arg(long, required_unless_present = "generate_key")]
    name: Option<String>,

    /// Application version
    #[arg(long, required_unless_present = "generate_key")]
    version: Option<String>,

    /// Relative path to executable within the input directory
    #[arg(long, required_unless_present = "generate_key")]
    exec: Option<String>,

    /// Input directory containing the application files
    #[arg(long, short, required_unless_present = "generate_key")]
    input: Option<PathBuf>,

    /// Output .lxe file path
    #[arg(long, short, required_unless_present = "generate_key")]
    output: Option<PathBuf>,

    /// Path to application icon (relative to input directory)
    #[arg(long)]
    icon: Option<String>,

    /// Application description
    #[arg(long)]
    description: Option<String>,

    /// XDG desktop categories (comma-separated)
    #[arg(long, default_value = "Application")]
    categories: String,

    /// Zstd compression level (1-22, higher = better compression but slower)
    #[arg(long, default_value = "19")]
    compression_level: i32,

    /// Path to pre-built lxe-runtime binary (optional)
    #[arg(long)]
    runtime: Option<PathBuf>,
    
    // ========== Ed25519 Signing Options ==========
    
    /// Generate a new Ed25519 keypair
    #[arg(long)]
    generate_key: bool,
    
    /// Output path for generated key (used with --generate-key)
    #[arg(long)]
    key_output: Option<PathBuf>,
    
    /// Path to Ed25519 private key for signing
    #[arg(long)]
    private_key: Option<PathBuf>,
}

/// Magic bytes for LXE payload (must match metadata.rs)
const LXE_MAGIC: &[u8] = b"\x00LXE\xF0\x9F\x93\x01";

// NOTE: SignableMetadata is imported from lxe::metadata
// This ensures both packer and runtime use the exact same struct definition
// to prevent signature verification failures due to JSON field order mismatch


fn main() -> Result<()> {
    let args = Args::parse();

    println!("🔧 LXE Packer v{}", env!("CARGO_PKG_VERSION"));
    println!();
    
    // Handle key generation mode
    if args.generate_key {
        return generate_keypair(&args);
    }
    
    // Regular packaging mode - unwrap required args
    let app_id = args.app_id.as_ref().expect("app_id required");
    let name = args.name.as_ref().expect("name required");
    let version = args.version.as_ref().expect("version required");
    let exec = args.exec.as_ref().expect("exec required");
    let input = args.input.as_ref().expect("input required");
    let output = args.output.as_ref().expect("output required");

    // Validate input directory
    if !input.is_dir() {
        anyhow::bail!("Input path is not a directory: {:?}", input);
    }

    // Check executable exists
    let exec_path = input.join(exec);
    if !exec_path.exists() {
        anyhow::bail!("Executable not found: {:?}", exec_path);
    }

    println!("📦 Packaging: {} v{}", name, version);
    println!("   App ID: {}", app_id);
    println!("   Input: {:?}", input);
    println!("   Output: {:?}", output);
    
    // Load signing key if provided
    let signing_key = if let Some(key_path) = &args.private_key {
        println!("   🔐 Signing with: {:?}", key_path);
        Some(load_signing_key(key_path)?)
    } else {
        println!("   ⚠️  No signing key provided (package will be unsigned)");
        None
    };
    println!();

    // Step 1: Create tar archive of input directory
    println!("📁 Creating archive...");
    let tar_data = create_tar_archive(input)?;
    println!("   Uncompressed size: {} bytes", tar_data.len());

    // Step 2: Compress with zstd
    println!("🗜️  Compressing with zstd (level {})...", args.compression_level);
    let compressed_data = compress_zstd(&tar_data, args.compression_level)?;
    let compression_ratio = tar_data.len() as f64 / compressed_data.len() as f64;
    println!(
        "   Compressed size: {} bytes ({:.1}x ratio)",
        compressed_data.len(),
        compression_ratio
    );

    // Step 3: Calculate checksum
    let checksum = calculate_sha256(&compressed_data);
    println!("   SHA256: {}", checksum);

    // Step 4: Create metadata
    let categories: Vec<String> = args
        .categories
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    // Build base metadata (without signature)
    let mut metadata = serde_json::json!({
        "format_version": 1,
        "app_id": app_id,
        "name": name,
        "version": version,
        "arch": std::env::consts::ARCH,
        "install_size": tar_data.len(),
        "exec": exec,
        "icon": args.icon,
        "description": args.description,
        "categories": categories,
        "payload_checksum": checksum,
    });
    
    // Step 5: Sign if we have a key
    if let Some(ref key) = signing_key {
        println!("🔏 Signing package...");
        
        // Create signable metadata with SAME field order as runtime's SignableMetadata
        // This is critical - different field order = different JSON = invalid signature
        let signable_metadata = SignableMetadata {
            format_version: 1,
            app_id,
            name,
            version,
            arch: std::env::consts::ARCH,
            install_size: tar_data.len() as u64,
            exec,
            icon: args.icon.as_deref(),
            categories: &categories,
            description: args.description.as_deref(),
            payload_checksum: &checksum,
            min_runtime_version: None,
            license: None,
            homepage: None,
            exec_args: None,
            terminal: false,
        };
        
        let signable_json = serde_json::to_vec(&signable_metadata)?;
        
        // Signable data = metadata JSON + payload checksum bytes
        let checksum_bytes = hex::decode(&checksum)?;
        let mut signable_data = signable_json.clone();
        signable_data.extend_from_slice(&checksum_bytes);
        
        // Sign!
        let signature = key.sign(&signable_data);
        let signature_b64 = BASE64.encode(signature.to_bytes());
        let public_key_b64 = BASE64.encode(key.verifying_key().as_bytes());
        
        println!("   Public key: {}...", &public_key_b64[..20]);
        println!("   Signature: {}...", &signature_b64[..20]);
        
        // Add signature fields to metadata
        metadata["public_key"] = serde_json::Value::String(public_key_b64);
        metadata["signature"] = serde_json::Value::String(signature_b64);
    }

    let metadata_json = serde_json::to_vec(&metadata)?;
    println!("   Metadata size: {} bytes", metadata_json.len());

    // Step 6: Get runtime binary
    println!("🔗 Preparing runtime...");
    let runtime_data = get_runtime_binary(&args.runtime)?;
    println!("   Runtime size: {} bytes", runtime_data.len());

    // Step 7: Assemble the final binary
    println!("🔨 Assembling package...");
    let mut output_file = File::create(output)?;

    // Write runtime
    output_file.write_all(&runtime_data)?;

    // Write magic bytes
    output_file.write_all(LXE_MAGIC)?;

    // Write metadata length (u32 little-endian)
    let metadata_len = metadata_json.len() as u32;
    output_file.write_all(&metadata_len.to_le_bytes())?;

    // Write metadata
    output_file.write_all(&metadata_json)?;

    // Write checksum (32 bytes)
    let checksum_bytes = hex::decode(&checksum)?;
    output_file.write_all(&checksum_bytes)?;

    // Write compressed payload
    output_file.write_all(&compressed_data)?;

    // Write Footer: [HeaderOffset(u64)] + [Magic(8)]
    // This allows O(1) lookup of the payload start regardless of payload size
    let header_offset = runtime_data.len() as u64;
    output_file.write_all(&header_offset.to_le_bytes())?;
    output_file.write_all(LXE_MAGIC)?;

    output_file.flush()?;

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(output)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(output, perms)?;
    }

    let total_size = fs::metadata(output)?.len();
    
    let output_filename = output
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "package.lxe".to_string());
    
    println!();
    println!("✅ Package created successfully!");
    println!("   Output: {:?}", output);
    println!("   Total size: {} bytes ({:.2} MB)", total_size, total_size as f64 / 1024.0 / 1024.0);
    if signing_key.is_some() {
        println!("   🔐 Signed: Yes");
    } else {
        println!("   🔐 Signed: No");
    }
    println!();
    println!("💡 To install, run: ./{}", output_filename);

    Ok(())
}

/// Generate a new Ed25519 keypair
fn generate_keypair(args: &Args) -> Result<()> {
    let output_path = args.key_output.clone()
        .unwrap_or_else(|| PathBuf::from("lxe-signing.key"));
    
    println!("🔑 Generating Ed25519 keypair...");
    
    // Generate keypair
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    
    // Encode: 32-byte seed + 32-byte public key
    let mut key_bytes = [0u8; 64];
    key_bytes[..32].copy_from_slice(signing_key.as_bytes());
    key_bytes[32..].copy_from_slice(verifying_key.as_bytes());
    
    let encoded = BASE64.encode(&key_bytes);
    
    // Write to file
    fs::write(&output_path, &encoded)?;
    
    // Set restrictive permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&output_path, perms)?;
    }
    
    println!();
    println!("✅ Keypair generated successfully!");
    println!("   Private key: {:?}", output_path);
    println!("   Public key (base64): {}", BASE64.encode(verifying_key.as_bytes()));
    println!();
    println!("⚠️  IMPORTANT: Keep your private key secure!");
    println!("   - Never share it or commit it to version control");
    println!("   - Back it up securely");
    println!("   - If lost, you'll need to generate a new key");
    println!();
    println!("To sign packages, use:");
    println!("   lxe-pack --private-key {:?} --input <dir> --output <file.lxe> ...", output_path);
    
    Ok(())
}

/// Load a signing key from file
fn load_signing_key(path: &PathBuf) -> Result<SigningKey> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("Failed to read key file: {:?}", path))?;
    
    let key_bytes = BASE64.decode(contents.trim())
        .context("Invalid base64 in key file")?;
    
    if key_bytes.len() != 64 {
        anyhow::bail!("Invalid key file: expected 64 bytes, got {}", key_bytes.len());
    }
    
    let seed: [u8; 32] = key_bytes[..32].try_into()
        .context("Failed to extract seed from key file")?;
    
    Ok(SigningKey::from_bytes(&seed))
}

/// Create a tar archive from a directory
fn create_tar_archive(input_dir: &PathBuf) -> Result<Vec<u8>> {
    let mut archive_data = Vec::new();

    {
        let mut builder = tar::Builder::new(&mut archive_data);
        builder.follow_symlinks(false);
        
        // Add all files from the directory
        builder.append_dir_all(".", input_dir)
            .context("Failed to add directory to tar archive")?;
        
        builder.finish()
            .context("Failed to finish tar archive")?;
    }

    Ok(archive_data)
}

/// Compress data with zstd
#[cfg(feature = "packer")]
fn compress_zstd(data: &[u8], level: i32) -> Result<Vec<u8>> {
    let compressed = zstd::encode_all(std::io::Cursor::new(data), level)
        .context("Failed to compress with zstd")?;
    Ok(compressed)
}

#[cfg(not(feature = "packer"))]
fn compress_zstd(_data: &[u8], _level: i32) -> Result<Vec<u8>> {
    anyhow::bail!(
        "Packer feature not enabled. Build with: cargo build --features packer"
    )
}

/// Calculate SHA256 checksum
fn calculate_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    hex::encode(result)
}

/// Get the runtime binary
fn get_runtime_binary(runtime_path: &Option<PathBuf>) -> Result<Vec<u8>> {
    if let Some(path) = runtime_path {
        // Use provided runtime
        let mut file = File::open(path)
            .context("Failed to open runtime binary")?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        Ok(data)
    } else {
        // Try to find the runtime in the same directory as this binary
        let current_exe = std::env::current_exe()?;
        let runtime_path = current_exe
            .parent()
            .map(|p| p.join("lxe-runtime"))
            .filter(|p| p.exists());

        if let Some(path) = runtime_path {
            let mut file = File::open(&path)?;
            let mut data = Vec::new();
            file.read_to_end(&mut data)?;
            Ok(data)
        } else {
            // Fall back to using current binary (for development)
            println!("   ⚠️  No separate runtime found, using current binary");
            let mut file = File::open(&current_exe)?;
            let mut data = Vec::new();
            file.read_to_end(&mut data)?;
            Ok(data)
        }
    }
}
