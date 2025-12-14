//! Ed25519 Digital Signatures for LXE Packages
//!
//! This module provides cryptographic signing and verification for LXE packages.
//! The packer signs the metadata + payload checksum with a private key,
//! and the runtime verifies the signature before showing the wizard.
//!
//! # Security Model
//!
//! - **What is signed**: Metadata JSON (without signature/public_key fields) + payload checksum
//! - **Algorithm**: Ed25519 (fast, small signatures, high security)
//! - **Key format**: Base64-encoded in the metadata
//!
//! # Usage
//!
//! ## Generating a Keypair (for publishers)
//! ```bash
//! lxe-pack --generate-key --key-output ~/.lxe/signing.key
//! ```
//!
//! ## Signing a Package
//! ```bash
//! lxe-pack --private-key ~/.lxe/signing.key --input ./myapp --output myapp.lxe
//! ```
//!
//! ## Verification (automatic)
//! When the runtime opens a signed package, it automatically verifies the signature
//! before showing the wizard. If verification fails, the app exits with an error.

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{
    Signature, SigningKey, VerifyingKey,
    Signer, Verifier,
};
use rand::rngs::OsRng;
use std::fs;
use std::path::Path;

/// A keypair for signing packages
pub struct LxeKeyPair {
    pub signing_key: SigningKey,
    pub verifying_key: VerifyingKey,
}

impl LxeKeyPair {
    /// Generate a new random keypair
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        Self { signing_key, verifying_key }
    }
    
    /// Load a keypair from a file
    /// 
    /// File format: 64 bytes (32-byte seed + 32-byte public key) base64-encoded
    pub fn load(path: &Path) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed to read key file: {:?}", path))?;
        
        let key_bytes = BASE64.decode(contents.trim())
            .context("Invalid base64 in key file")?;
        
        if key_bytes.len() != 64 {
            bail!("Invalid key file: expected 64 bytes, got {}", key_bytes.len());
        }
        
        let seed: [u8; 32] = key_bytes[..32].try_into()
            .context("Failed to extract seed from key file")?;
        
        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();
        
        Ok(Self { signing_key, verifying_key })
    }
    
    /// Save the keypair to a file
    pub fn save(&self, path: &Path) -> Result<()> {
        // Combine seed (32 bytes) + public key (32 bytes)
        let mut key_bytes = [0u8; 64];
        key_bytes[..32].copy_from_slice(self.signing_key.as_bytes());
        key_bytes[32..].copy_from_slice(self.verifying_key.as_bytes());
        
        let encoded = BASE64.encode(&key_bytes);
        
        fs::write(path, &encoded)
            .with_context(|| format!("Failed to write key file: {:?}", path))?;
        
        // Set restrictive permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(path, perms)?;
        }
        
        Ok(())
    }
    
    /// Get the public key as base64 string (for embedding in metadata)
    pub fn public_key_base64(&self) -> String {
        BASE64.encode(self.verifying_key.as_bytes())
    }
    
    /// Sign data and return the signature as base64 string
    pub fn sign(&self, data: &[u8]) -> String {
        let signature = self.signing_key.sign(data);
        BASE64.encode(signature.to_bytes())
    }
}

/// Verify a signature against data and public key
/// 
/// # Arguments
/// * `data` - The data that was signed
/// * `signature_base64` - The signature as base64 string
/// * `public_key_base64` - The public key as base64 string
/// 
/// # Returns
/// * `Ok(true)` - Signature is valid
/// * `Ok(false)` - Signature is invalid
/// * `Err(_)` - Error parsing signature or public key
pub fn verify_signature(
    data: &[u8],
    signature_base64: &str,
    public_key_base64: &str,
) -> Result<bool> {
    // Decode public key
    let public_key_bytes = BASE64.decode(public_key_base64)
        .context("Invalid base64 in public key")?;
    
    if public_key_bytes.len() != 32 {
        bail!("Invalid public key: expected 32 bytes, got {}", public_key_bytes.len());
    }
    
    let public_key_array: [u8; 32] = public_key_bytes.try_into()
        .map_err(|_| anyhow::anyhow!("Failed to convert public key bytes"))?;
    
    let verifying_key = VerifyingKey::from_bytes(&public_key_array)
        .context("Invalid public key format")?;
    
    // Decode signature
    let signature_bytes = BASE64.decode(signature_base64)
        .context("Invalid base64 in signature")?;
    
    if signature_bytes.len() != 64 {
        bail!("Invalid signature: expected 64 bytes, got {}", signature_bytes.len());
    }
    
    let signature_array: [u8; 64] = signature_bytes.try_into()
        .map_err(|_| anyhow::anyhow!("Failed to convert signature bytes"))?;
    
    let signature = Signature::from_bytes(&signature_array);
    
    // Verify
    match verifying_key.verify(data, &signature) {
        Ok(()) => Ok(true),
        Err(_) => Ok(false),
    }
}

/// Create the signable data from metadata JSON and payload checksum
/// 
/// This concatenates:
/// 1. The metadata JSON bytes (WITHOUT signature and public_key fields)
/// 2. The payload checksum bytes (from hex string)
pub fn create_signable_data(
    signable_metadata_json: &[u8],
    payload_checksum_hex: &str,
) -> Result<Vec<u8>> {
    let checksum_bytes = hex::decode(payload_checksum_hex)
        .context("Invalid hex in payload checksum")?;
    
    let mut data = signable_metadata_json.to_vec();
    data.extend_from_slice(&checksum_bytes);
    
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_generate_and_sign() {
        let keypair = LxeKeyPair::generate();
        let data = b"Hello, LXE!";
        
        let signature = keypair.sign(data);
        let public_key = keypair.public_key_base64();
        
        // Verify should succeed
        let result = verify_signature(data, &signature, &public_key).unwrap();
        assert!(result, "Signature should be valid");
    }
    
    #[test]
    fn test_tampered_data_fails() {
        let keypair = LxeKeyPair::generate();
        let data = b"Hello, LXE!";
        
        let signature = keypair.sign(data);
        let public_key = keypair.public_key_base64();
        
        // Verify with tampered data should fail
        let tampered = b"Hello, HACKED!";
        let result = verify_signature(tampered, &signature, &public_key).unwrap();
        assert!(!result, "Tampered data should fail verification");
    }
    
    #[test]
    fn test_wrong_key_fails() {
        let keypair1 = LxeKeyPair::generate();
        let keypair2 = LxeKeyPair::generate();
        let data = b"Hello, LXE!";
        
        let signature = keypair1.sign(data);
        let wrong_public_key = keypair2.public_key_base64();
        
        // Verify with wrong public key should fail
        let result = verify_signature(data, &signature, &wrong_public_key).unwrap();
        assert!(!result, "Wrong public key should fail verification");
    }
    
    #[test]
    fn test_create_signable_data() {
        let metadata = b"{\"app_id\":\"com.test.App\"}";
        let checksum = "abcd1234";
        
        let data = create_signable_data(metadata, checksum).unwrap();
        
        // Should be metadata + checksum bytes
        assert_eq!(data.len(), metadata.len() + 4); // 4 bytes for "abcd1234" in hex
    }
}
