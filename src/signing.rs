//! Software-tier Ed25519 signing — generalized from familiar-record's NodeSigner.
//! Key file stores the PKCS8 document as hex (one line). Same scheme Familiar
//! ships today, so a Familiar node key IS a valid software-tier Rune key.

use anyhow::{Context, Result};
use ring::signature::{Ed25519KeyPair, KeyPair, UnparsedPublicKey, ED25519};
use std::path::Path;

pub struct RuneSigner {
    keypair: Ed25519KeyPair,
    pubkey_hex: String,
}

impl RuneSigner {
    /// Load the key at `key_path`; generate and write one (0600) if absent.
    pub fn load_or_generate(key_path: &Path) -> Result<Self> {
        if key_path.exists() {
            return Self::load(key_path);
        }
        let rng = ring::rand::SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng)
            .map_err(|_| anyhow::anyhow!("Ed25519 key generation failed"))?;
        if let Some(parent) = key_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        write_new_0600(key_path, hex::encode(pkcs8.as_ref()).as_bytes())
            .with_context(|| format!("writing key file {}", key_path.display()))?;
        Self::from_pkcs8(key_path, pkcs8.as_ref())
    }

    /// Load an existing key; a missing file is an error, never a new key —
    /// for callers that must only ever speak as a known identity
    /// (`runecraft receipt sign`).
    pub fn load(key_path: &Path) -> Result<Self> {
        let hx = std::fs::read_to_string(key_path)
            .with_context(|| format!("reading key file {}", key_path.display()))?;
        let bytes = hex::decode(hx.trim()).context("decoding key file hex")?;
        Self::from_pkcs8(key_path, &bytes)
    }

    fn from_pkcs8(key_path: &Path, pkcs8_bytes: &[u8]) -> Result<Self> {
        let keypair = Ed25519KeyPair::from_pkcs8(pkcs8_bytes)
            .map_err(|e| anyhow::anyhow!("{}: not an Ed25519 PKCS8 key: {e}", key_path.display()))?;
        let pubkey_hex = hex::encode(keypair.public_key().as_ref());

        // Pubkey is the node's public identity — companion file for one-shot enrollment.
        // Non-fatal, but said out loud: familiar-record's NodeSigner (this loader's
        // origin) warns the same way; re-synced 2026-10-19.
        let pub_path = key_path.with_extension("pub");
        if let Err(e) = std::fs::write(&pub_path, &pubkey_hex) {
            eprintln!("warning: could not write {}: {e}", pub_path.display());
        }

        Ok(RuneSigner {
            keypair,
            pubkey_hex,
        })
    }

    pub fn sign(&self, msg: &[u8]) -> Vec<u8> {
        self.keypair.sign(msg).as_ref().to_vec()
    }

    pub fn pubkey_hex(&self) -> &str {
        &self.pubkey_hex
    }

    /// ADR-001 software-tier fingerprint: "software:ed25519:{first16 hex of pubkey}".
    pub fn signed_by(&self) -> String {
        format!("software:ed25519:{}", &self.pubkey_hex[..16])
    }
}

pub fn verify_ed25519(pubkey_hex: &str, msg: &[u8], sig: &[u8]) -> Result<()> {
    let pk = hex::decode(pubkey_hex).context("decoding pubkey hex")?;
    UnparsedPublicKey::new(&ED25519, pk)
        .verify(msg, sig)
        .map_err(|_| anyhow::anyhow!("signature verification failed"))
}

/// Create `path` holding a secret at mode 0600 from the first byte — no
/// window in which the file exists world-readable (write-then-chmod left
/// one) — and never over a file that appeared since the caller looked: an
/// identity key must not be silently replaced. Node key and strand key
/// (rune-weave) both go through here, 2026-10-20.
pub fn write_new_0600(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path)?.write_all(contents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_key_is_private_from_creation_and_reloads() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("node.key");
        let s = RuneSigner::load_or_generate(&path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "key file mode {mode:o}");
        }
        assert_eq!(
            std::fs::read_to_string(path.with_extension("pub")).unwrap(),
            s.pubkey_hex()
        );
        let again = RuneSigner::load_or_generate(&path).unwrap();
        assert_eq!(again.pubkey_hex(), s.pubkey_hex());
    }

    #[test]
    fn load_never_generates_and_names_a_bad_key() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("node.key");
        assert!(RuneSigner::load(&path).is_err());
        assert!(!path.exists(), "load must not create a key");
        std::fs::write(&path, "abcd").unwrap();
        let Err(err) = RuneSigner::load(&path) else {
            panic!("4 hex chars loaded as a key");
        };
        assert!(err.to_string().contains("not an Ed25519 PKCS8 key"), "{err}");
    }

    #[test]
    fn write_new_0600_refuses_an_existing_file() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("k");
        write_new_0600(&path, b"one").unwrap();
        assert!(write_new_0600(&path, b"two").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"one");
    }
}
