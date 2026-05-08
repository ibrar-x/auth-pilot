//! Local CA certificate generation.
//!
//! Keychain trust installation is available only through explicit commands.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use rcgen::{
    date_time_ymd, BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, KeyPair,
};
use serde::Serialize;

use crate::auth::storage;
use crate::settings;

const CA_CERT_FILENAME: &str = "authpilot-ca.crt";
const CA_KEY_FILENAME: &str = "authpilot-ca.key";

#[derive(Debug, Clone)]
pub struct Ca {
    pub cert_pem: String,
    pub key_pem: String,
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct CaStatus {
    pub cert_path: String,
    pub key_path: String,
    pub cert_exists: bool,
    pub key_exists: bool,
    pub ready: bool,
    pub trusted_by_authpilot: bool,
}

pub fn generate_or_load_ca() -> Result<Ca> {
    let config_dir = storage::get_config_dir()?;
    let (cert_path, key_path) = ca_paths_for_config_dir(&config_dir);
    fs::create_dir_all(&config_dir).with_context(|| {
        format!(
            "failed to create AuthPilot config dir {}",
            config_dir.display()
        )
    })?;

    if cert_path.exists() && key_path.exists() {
        return Ok(Ca {
            cert_pem: fs::read_to_string(&cert_path)
                .with_context(|| format!("failed to read CA cert {}", cert_path.display()))?,
            key_pem: fs::read_to_string(&key_path)
                .with_context(|| format!("failed to read CA key {}", key_path.display()))?,
            cert_path,
            key_path,
        });
    }

    generate_ca_at_paths(cert_path, key_path)
}

pub fn regenerate_ca() -> Result<Ca> {
    let config_dir = storage::get_config_dir()?;
    let (cert_path, key_path) = ca_paths_for_config_dir(&config_dir);
    fs::create_dir_all(&config_dir).with_context(|| {
        format!(
            "failed to create AuthPilot config dir {}",
            config_dir.display()
        )
    })?;
    generate_ca_at_paths(cert_path, key_path)
}

pub fn ca_status() -> Result<CaStatus> {
    let config_dir = storage::get_config_dir()?;
    let (cert_path, key_path) = ca_paths_for_config_dir(&config_dir);
    let trusted_by_authpilot = settings::load_settings()
        .map(|settings| settings.proxy_ca_trusted)
        .unwrap_or(false);
    Ok(ca_status_for_paths(
        cert_path,
        key_path,
        trusted_by_authpilot,
    ))
}

#[cfg(target_os = "macos")]
pub fn install_ca_trust() -> Result<CaStatus> {
    let ca = generate_or_load_ca()?;
    let args = install_ca_trust_args(&ca.cert_path);
    let output = Command::new("security")
        .args(&args)
        .output()
        .context("failed to run macOS security command")?;

    if !output.status.success() {
        anyhow::bail!(
            "CA trust install failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let mut app_settings = settings::load_settings().unwrap_or_default();
    app_settings.proxy_ca_trusted = true;
    settings::save_settings(&app_settings)?;
    tracing::info!("[cert] local CA trusted in macOS System Keychain");
    ca_status()
}

#[cfg(not(target_os = "macos"))]
pub fn install_ca_trust() -> Result<CaStatus> {
    anyhow::bail!("CA trust installation is only supported on macOS")
}

fn generate_ca_at_paths(cert_path: PathBuf, key_path: PathBuf) -> Result<Ca> {
    let key_pair = KeyPair::generate().context("failed to generate AuthPilot CA key pair")?;
    let mut params = CertificateParams::default();
    params.distinguished_name = DistinguishedName::new();
    params
        .distinguished_name
        .push(DnType::CommonName, "AuthPilot Local CA");
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.not_after = date_time_ymd(2035, 1, 1);

    let cert = params
        .self_signed(&key_pair)
        .context("failed to generate AuthPilot CA certificate")?;
    let cert_pem = cert.pem();
    let key_pem = key_pair.serialize_pem();

    fs::write(&cert_path, &cert_pem)
        .with_context(|| format!("failed to write CA cert {}", cert_path.display()))?;
    fs::write(&key_path, &key_pem)
        .with_context(|| format!("failed to write CA key {}", key_path.display()))?;
    restrict_key_permissions(&key_path)?;

    tracing::info!("[cert] local CA generated at {}", cert_path.display());
    Ok(Ca {
        cert_pem,
        key_pem,
        cert_path,
        key_path,
    })
}

fn restrict_key_permissions(key_path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(key_path, fs::Permissions::from_mode(0o600)).with_context(|| {
            format!(
                "failed to restrict CA key permissions {}",
                key_path.display()
            )
        })?;
    }
    Ok(())
}

fn ca_status_for_paths(
    cert_path: PathBuf,
    key_path: PathBuf,
    trusted_by_authpilot: bool,
) -> CaStatus {
    let cert_exists = cert_path.exists();
    let key_exists = key_path.exists();
    CaStatus {
        cert_path: cert_path.display().to_string(),
        key_path: key_path.display().to_string(),
        cert_exists,
        key_exists,
        ready: cert_exists && key_exists,
        trusted_by_authpilot,
    }
}

fn install_ca_trust_args(cert_path: &Path) -> Vec<String> {
    vec![
        "add-trusted-cert".to_string(),
        "-d".to_string(),
        "-r".to_string(),
        "trustRoot".to_string(),
        "-k".to_string(),
        "/Library/Keychains/System.keychain".to_string(),
        cert_path.display().to_string(),
    ]
}

fn ca_paths_for_config_dir(config_dir: &Path) -> (PathBuf, PathBuf) {
    (
        config_dir.join(CA_CERT_FILENAME),
        config_dir.join(CA_KEY_FILENAME),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn ca_paths_live_under_config_dir() {
        let (cert_path, key_path) = ca_paths_for_config_dir(Path::new("/tmp/authpilot"));
        assert_eq!(cert_path, PathBuf::from("/tmp/authpilot/authpilot-ca.crt"));
        assert_eq!(key_path, PathBuf::from("/tmp/authpilot/authpilot-ca.key"));
    }

    #[test]
    fn generated_ca_writes_cert_and_private_key() {
        let temp = tempdir().unwrap();
        let cert_path = temp.path().join("test-ca.crt");
        let key_path = temp.path().join("test-ca.key");

        let ca = generate_ca_at_paths(cert_path.clone(), key_path.clone()).unwrap();

        assert!(cert_path.exists());
        assert!(key_path.exists());
        assert!(ca.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(ca.key_pem.contains("BEGIN PRIVATE KEY"));
        assert_eq!(ca.cert_path, cert_path);
        assert_eq!(ca.key_path, key_path);
    }

    #[test]
    fn status_is_ready_only_when_cert_and_key_exist() {
        let temp = tempdir().unwrap();
        let cert_path = temp.path().join("ca.crt");
        let key_path = temp.path().join("ca.key");

        let missing = ca_status_for_paths(cert_path.clone(), key_path.clone(), false);
        assert!(!missing.ready);
        assert!(!missing.trusted_by_authpilot);

        fs::write(&cert_path, "cert").unwrap();
        fs::write(&key_path, "key").unwrap();
        let ready = ca_status_for_paths(cert_path, key_path, true);
        assert!(ready.ready);
        assert!(ready.trusted_by_authpilot);
    }

    #[test]
    fn builds_macos_security_trust_install_args() {
        assert_eq!(
            install_ca_trust_args(Path::new("/tmp/authpilot-ca.crt")),
            vec![
                "add-trusted-cert",
                "-d",
                "-r",
                "trustRoot",
                "-k",
                "/Library/Keychains/System.keychain",
                "/tmp/authpilot-ca.crt"
            ]
        );
    }
}
