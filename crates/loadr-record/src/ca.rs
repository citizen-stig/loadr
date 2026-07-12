//! A local certificate authority for the recording proxy.
//!
//! On first use we generate a loadr CA (cert + key) and persist it under the
//! user's config dir. The user installs the CA cert once in their OS/browser
//! trust store; thereafter the proxy mints a short-lived leaf certificate per
//! host on the fly so it can terminate TLS (MITM) and capture the plaintext.
//!
//! Nothing here is a secret to protect a *remote* party: the CA only signs
//! certificates for traffic the user is deliberately routing through their own
//! recorder on localhost. The key is written 0600.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rcgen::{
    BasicConstraints, CertificateParams, CertifiedIssuer, DnType, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::ServerConfig;

#[derive(Debug, thiserror::Error)]
pub enum CaError {
    #[error("certificate error: {0}")]
    Rcgen(#[from] rcgen::Error),
    #[error("tls error: {0}")]
    Rustls(#[from] rustls::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// The recorder's certificate authority, able to mint per-host leaf certs.
pub struct Ca {
    issuer: Issuer<'static, KeyPair>,
    /// Cache of per-host rustls server configs, so we sign each host once.
    cache: Mutex<HashMap<String, Arc<ServerConfig>>>,
    /// The CA certificate in PEM, for the user to trust.
    ca_cert_pem: String,
}

impl Ca {
    /// Directory where the CA lives (`$XDG_CONFIG_HOME/loadr` or `~/.config/loadr`).
    pub fn default_dir() -> PathBuf {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            return PathBuf::from(xdg).join("loadr");
        }
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(".config").join("loadr");
        }
        PathBuf::from(".loadr")
    }

    fn cert_path(dir: &Path) -> PathBuf {
        dir.join("record-ca-cert.pem")
    }
    fn key_path(dir: &Path) -> PathBuf {
        dir.join("record-ca-key.pem")
    }

    /// Load the CA from `dir`, generating and persisting it on first use.
    pub fn load_or_create(dir: &Path) -> Result<Self, CaError> {
        let cert_path = Self::cert_path(dir);
        let key_path = Self::key_path(dir);

        let (cert_pem, key_pem) = if cert_path.exists() && key_path.exists() {
            (
                std::fs::read_to_string(&cert_path)?,
                std::fs::read_to_string(&key_path)?,
            )
        } else {
            let (cert_pem, key_pem) = Self::generate()?;
            std::fs::create_dir_all(dir)?;
            std::fs::write(&cert_path, &cert_pem)?;
            write_private(&key_path, &key_pem)?;
            (cert_pem, key_pem)
        };

        // Reconstruct the issuer from the same deterministic params + the
        // persisted key (avoids the x509-parser feature). The subject DN and
        // key match the persisted CA cert, so minted leaves chain to it.
        let key = KeyPair::from_pem(&key_pem)?;
        let issuer = Issuer::new(ca_params()?, key);
        Ok(Self {
            issuer,
            cache: Mutex::new(HashMap::new()),
            ca_cert_pem: cert_pem,
        })
    }

    /// Generate a fresh CA, returning `(cert_pem, key_pem)`.
    fn generate() -> Result<(String, String), CaError> {
        let key = KeyPair::generate()?;
        let key_pem = key.serialize_pem(); // serialize before the key is moved
        let ca = CertifiedIssuer::self_signed(ca_params()?, key)?;
        Ok((ca.pem(), key_pem))
    }

    /// The CA certificate PEM the user installs in their trust store.
    pub fn cert_pem(&self) -> &str {
        &self.ca_cert_pem
    }

    /// A rustls server config presenting a leaf cert valid for `host`,
    /// minted on first request and cached thereafter.
    pub fn server_config_for(&self, host: &str) -> Result<Arc<ServerConfig>, CaError> {
        if let Some(cfg) = self.cache.lock().unwrap().get(host) {
            return Ok(cfg.clone());
        }
        let leaf_key = KeyPair::generate()?;
        let leaf_key_der = leaf_key.serialize_der();
        let mut params = CertificateParams::new(vec![host.to_string()])?;
        params.distinguished_name.push(DnType::CommonName, host);
        let leaf = params.signed_by(&leaf_key, &self.issuer)?;

        let cert_chain: Vec<CertificateDer<'static>> = vec![leaf.der().clone()];
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(leaf_key_der));
        let cfg = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cert_chain, key)?;
        let cfg = Arc::new(cfg);
        self.cache
            .lock()
            .unwrap()
            .insert(host.to_string(), cfg.clone());
        Ok(cfg)
    }
}

/// The CA's certificate parameters — deterministic, so a reconstructed issuer
/// matches the persisted CA certificate.
fn ca_params() -> Result<CertificateParams, CaError> {
    let mut params = CertificateParams::new(Vec::<String>::new())?;
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params
        .distinguished_name
        .push(DnType::CommonName, "loadr record local CA");
    params
        .distinguished_name
        .push(DnType::OrganizationName, "loadr");
    params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    Ok(params)
}

#[cfg(unix)]
fn write_private(path: &Path, contents: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(contents.as_bytes())
}

#[cfg(not(unix))]
fn write_private(path: &Path, contents: &str) -> std::io::Result<()> {
    std::fs::write(path, contents)
}
