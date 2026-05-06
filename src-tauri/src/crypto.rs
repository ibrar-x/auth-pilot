use anyhow::{Context, Result};
use base64::Engine;
use pbkdf2::pbkdf2_hmac;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const PBKDF2_ITERATIONS: u32 = 100_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedBlob {
    pub v: u32,
    pub salt: String,
    pub nonce: String,
    pub ciphertext: String,
}

fn get_machine_id_file_path() -> Result<std::path::PathBuf> {
    let config_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?
        .join(".authpilot");
    Ok(config_dir.join(".machine_id"))
}

pub fn compute_machine_id() -> Result<String> {
    #[cfg(target_os = "macos")]
    {
        match std::process::Command::new("sysctl")
            .args(["-n", "hw.uuid"])
            .output()
        {
            Ok(output) if output.status.success() => {
                let uuid = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !uuid.is_empty() {
                    return Ok(uuid);
                }
            }
            _ => {}
        }
    }

    let hostname = std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let username = dirs::home_dir()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "unknown".to_string());

    let mut hasher = Sha256::new();
    hasher.update(hostname.as_bytes());
    hasher.update(username.as_bytes());
    let hash = base64::engine::general_purpose::STANDARD.encode(hasher.finalize());
    Ok(hash)
}

pub fn get_machine_id() -> Result<String> {
    let path = get_machine_id_file_path()?;

    // If we already persisted a machine ID, use it for stability
    if path.exists() {
        let id = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read machine ID from {}", path.display()))?;
        let id = id.trim().to_string();
        if !id.is_empty() {
            return Ok(id);
        }
    }

    // First run: compute and persist
    let id = compute_machine_id()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config dir: {}", parent.display()))?;
    }
    std::fs::write(&path, &id)
        .with_context(|| format!("Failed to write machine ID to {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        let _ = std::fs::set_permissions(&path, perms);
    }

    Ok(id)
}

pub fn derive_key(salt: &[u8], machine_id: &str) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(machine_id.as_bytes(), salt, PBKDF2_ITERATIONS, &mut key);
    key
}

pub fn encrypt(plaintext: &str, machine_id: &str) -> Result<EncryptedBlob> {
    use chacha20poly1305::aead::generic_array::GenericArray;
    use chacha20poly1305::{
        aead::{Aead, KeyInit},
        XChaCha20Poly1305,
    };

    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 24];
    rand::rng().fill_bytes(&mut salt);
    rand::rng().fill_bytes(&mut nonce);

    let key = derive_key(&salt, machine_id);
    let cipher = XChaCha20Poly1305::new_from_slice(&key)
        .map_err(|e| anyhow::anyhow!("Failed to initialize cipher: {:?}", e))?;

    let ciphertext = cipher
        .encrypt(GenericArray::from_slice(&nonce), plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!("Encryption failed: {:?}", e))?;

    Ok(EncryptedBlob {
        v: 1,
        salt: base64::engine::general_purpose::STANDARD.encode(salt),
        nonce: base64::engine::general_purpose::STANDARD.encode(nonce),
        ciphertext: base64::engine::general_purpose::STANDARD.encode(&ciphertext),
    })
}

pub fn decrypt(blob: &EncryptedBlob, machine_id: &str) -> Result<String> {
    use chacha20poly1305::aead::generic_array::GenericArray;
    use chacha20poly1305::{
        aead::{Aead, KeyInit},
        XChaCha20Poly1305,
    };

    let salt = base64::engine::general_purpose::STANDARD
        .decode(&blob.salt)
        .context("Invalid salt encoding")?;
    let nonce = base64::engine::general_purpose::STANDARD
        .decode(&blob.nonce)
        .context("Invalid nonce encoding")?;
    let ciphertext = base64::engine::general_purpose::STANDARD
        .decode(&blob.ciphertext)
        .context("Invalid ciphertext encoding")?;

    let key = derive_key(&salt, machine_id);
    let cipher = XChaCha20Poly1305::new_from_slice(&key)
        .map_err(|e| anyhow::anyhow!("Failed to initialize cipher: {:?}", e))?;

    let plaintext = cipher
        .decrypt(GenericArray::from_slice(&nonce), ciphertext.as_ref())
        .map_err(|e| anyhow::anyhow!("Decryption failed: {:?}", e))?;

    String::from_utf8(plaintext).context("Invalid UTF-8 in decrypted plaintext")
}
