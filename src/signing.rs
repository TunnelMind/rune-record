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
    pub fn load_or_generate(key_path: &Path) -> Result<Self> {
        let pkcs8_bytes = if key_path.exists() {
            let hx = std::fs::read_to_string(key_path)
                .with_context(|| format!("reading key file {}", key_path.display()))?;
            hex::decode(hx.trim()).context("decoding key file hex")?
        } else {
            let rng = ring::rand::SystemRandom::new();
            let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng)
                .map_err(|_| anyhow::anyhow!("Ed25519 key generation failed"))?;
            let bytes = pkcs8.as_ref().to_vec();
            if let Some(parent) = key_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(key_path, hex::encode(&bytes))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(key_path, std::fs::Permissions::from_mode(0o600))?;
            }
            bytes
        };

        let keypair = Ed25519KeyPair::from_pkcs8(&pkcs8_bytes)
            .map_err(|_| anyhow::anyhow!("failed to load Ed25519 keypair from PKCS8"))?;
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
