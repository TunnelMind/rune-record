//! Software-tier Ed25519 signing: the node identity key, its loader and the
//! one signature verifier the workspace uses.
//!
//! Data flow: `RuneSigner` is built once per process (runecraft, rune-escalate,
//! rune-weave) from a key file and signs every receipt and strand cert;
//! `verify_ed25519` is the primitive under `receipt::verify`,
//! `receipt_v1::verify` and `StrandCert::verify`.
//!
//! Key file: the PKCS#8 v2 document as one line of hex, the scheme Familiar
//! ships, so a Familiar node key IS a valid software-tier Rune key. The file
//! is created 0600 from its first byte and never overwritten (`write_new_0600`).
//! Trust boundary: the key file is operator-owned; its contents are parsed by
//! ring and a malformed file is an error naming the path, never a panic.
//! `verify_ed25519`'s inputs are untrusted (hex and signature bytes from a
//! peer or a file) and fail closed. The public key is kept in a `.pub`
//! sibling (a convenience for enrollment, not a secret): each load rewrites
//! it only when its bytes differ, by an atomic rename (`write_pub_if_changed`).

use anyhow::{Context, Result};
use ring::signature::{Ed25519KeyPair, KeyPair, UnparsedPublicKey, ED25519};
use std::path::Path;

/// A loaded Ed25519 keypair plus its public key as lowercase hex (64 chars),
/// which is the node's identity everywhere (`signed_by` fingerprint, strand
/// certs, the resolver's `pubkey_ed25519`).
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

    /// Parse the PKCS#8 document and derive the public key. ring checks that
    /// the embedded public key matches the private half, so a spliced or
    /// truncated file cannot load as a different identity.
    fn from_pkcs8(key_path: &Path, pkcs8_bytes: &[u8]) -> Result<Self> {
        let keypair = Ed25519KeyPair::from_pkcs8(pkcs8_bytes)
            .map_err(|e| anyhow::anyhow!("{}: not an Ed25519 PKCS8 key: {e}", key_path.display()))?;
        let pubkey_hex = hex::encode(keypair.public_key().as_ref());

        // The pubkey is the node's public identity; the companion file lets an
        // operator read it over SSH without parsing PKCS#8 (one-shot
        // enrollment). Non-fatal, but said out loud on stderr: a library
        // choosing the channel is a known wart, kept because every caller is
        // a CLI today. Left alone when already right, so a reader (or an
        // mtime watcher) never sees it change; replaced atomically when stale.
        let pub_path = key_path.with_extension("pub");
        if let Err(e) = write_pub_if_changed(&pub_path, &pubkey_hex) {
            eprintln!("warning: could not write {}: {e}", pub_path.display());
        }

        Ok(RuneSigner {
            keypair,
            pubkey_hex,
        })
    }

    /// The 64-byte Ed25519 signature over `msg` (callers pass JCS bytes).
    pub fn sign(&self, msg: &[u8]) -> Vec<u8> {
        self.keypair.sign(msg).as_ref().to_vec()
    }

    /// Lowercase hex of the 32-byte public key.
    pub fn pubkey_hex(&self) -> &str {
        &self.pubkey_hex
    }

    /// ADR-001 software-tier fingerprint: "software:ed25519:{first16 hex of pubkey}".
    pub fn signed_by(&self) -> String {
        format!("software:ed25519:{}", &self.pubkey_hex[..16])
    }
}

/// Ed25519 verification of `sig` over `msg` under the hex-encoded public key.
/// All three inputs may come from a peer: bad hex, a wrong-length key or a
/// forged signature are all the same `Err`, never a panic.
pub fn verify_ed25519(pubkey_hex: &str, msg: &[u8], sig: &[u8]) -> Result<()> {
    let pk = hex::decode(pubkey_hex).context("decoding pubkey hex")?;
    UnparsedPublicKey::new(&ED25519, pk)
        .verify(msg, sig)
        .map_err(|_| anyhow::anyhow!("signature verification failed"))
}

/// Create `path` holding a secret at mode 0600 from the first byte — no
/// window in which the file exists world-readable (write-then-chmod leaves
/// one) — and never over a file that appeared since the caller looked
/// (`create_new`): an identity key must not be silently replaced. The node
/// key and rune-weave's strand key both go through here.
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

/// Make `pub_path` hold exactly `pubkey_hex`. Already-correct: untouched (no
/// write, same inode and mtime). Otherwise the bytes go to a fresh temp file
/// in the same directory, which is then renamed over `pub_path`, so a
/// concurrent reader sees the old file or the new one, never a truncated or
/// half-written one (`std::fs::write` truncates in place). Replacing a stale
/// `.pub` by rename discards any chmod, owner or symlink the operator set on it.
///
/// No fsync: a crash can leave the old or no `.pub`, and the next load heals
/// it — it is derived from the key, never the source of truth.
fn write_pub_if_changed(pub_path: &Path, pubkey_hex: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    if std::fs::read(pub_path).ok().as_deref() == Some(pubkey_hex.as_bytes()) {
        return Ok(());
    }
    // pid + per-process counter: unique across processes and across threads
    // loading keys that share a `.pub`; create_new refuses a leftover anyway.
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut tmp = pub_path.as_os_str().to_owned();
    tmp.push(format!(".tmp.{}.{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed)));
    let tmp = std::path::PathBuf::from(tmp);
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o644);
    }
    // One closure so every failure below (open, write, chmod, rename) falls
    // through to the temp-file cleanup.
    let res = (|| {
        let mut f = opts.open(&tmp)?;
        f.write_all(pubkey_hex.as_bytes())?;
        // The umask masks the create mode (077 would give 0600); an explicit
        // chmod is not masked, so the `.pub` is exactly 0644.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            f.set_permissions(std::fs::Permissions::from_mode(0o644))?;
        }
        std::fs::rename(&tmp, pub_path)
    })();
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
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

    /// A fresh key in `dir` under `name`; returns its path.
    fn new_key(dir: &Path, name: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        RuneSigner::load_or_generate(&path).unwrap();
        path
    }

    #[test]
    fn pub_untouched_when_unchanged() {
        let d = tempfile::tempdir().unwrap();
        let key = new_key(d.path(), "node.key");
        let pub_path = key.with_extension("pub");
        // Backdate the mtime so a rewrite is visible even on a coarse clock.
        let old = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        std::fs::File::options()
            .write(true)
            .open(&pub_path)
            .unwrap()
            .set_modified(old)
            .unwrap();
        let before = std::fs::metadata(&pub_path).unwrap();
        RuneSigner::load(&key).unwrap();
        let after = std::fs::metadata(&pub_path).unwrap();
        assert_eq!(after.modified().unwrap(), old, ".pub rewritten though unchanged");
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(after.ino(), before.ino(), ".pub replaced though unchanged");
        }
    }

    #[test]
    fn pub_stale_is_replaced() {
        let d = tempfile::tempdir().unwrap();
        let key = new_key(d.path(), "node.key");
        let pub_path = key.with_extension("pub");
        let want = RuneSigner::load(&key).unwrap().pubkey_hex().to_string();
        for seed in ["00".repeat(32), format!("{want}\n")] {
            std::fs::write(&pub_path, &seed).unwrap();
            RuneSigner::load(&key).unwrap();
            assert_eq!(std::fs::read_to_string(&pub_path).unwrap(), want, "seed {seed:?}");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(&pub_path).unwrap().permissions().mode() & 0o777;
                assert_eq!(mode, 0o644, ".pub mode {mode:o}");
            }
            let leftovers: Vec<_> = std::fs::read_dir(d.path())
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .filter(|n| n.contains(".tmp."))
                .collect();
            assert!(leftovers.is_empty(), "temp files left: {leftovers:?}");
        }
    }

    #[test]
    fn pub_reader_never_sees_partial() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        let d = tempfile::tempdir().unwrap();
        // `node.a` and `node.b` both map to `node.pub` via with_extension.
        let a = new_key(d.path(), "node.a");
        let b = new_key(d.path(), "node.b");
        let pubs = [
            RuneSigner::load(&a).unwrap().pubkey_hex().to_string(),
            RuneSigner::load(&b).unwrap().pubkey_hex().to_string(),
        ];
        assert_ne!(pubs[0], pubs[1]);
        let pub_path = d.path().join("node.pub");
        let stop = Arc::new(AtomicBool::new(false));
        let reader = {
            let (stop, pubs, pub_path) = (stop.clone(), pubs.clone(), pub_path.clone());
            std::thread::spawn(move || {
                let mut reads = 0u64;
                while !stop.load(Ordering::Relaxed) {
                    let got = std::fs::read_to_string(&pub_path).unwrap();
                    assert!(
                        got.len() == 64 && pubs.contains(&got),
                        "reader saw {got:?} after {reads} reads"
                    );
                    reads += 1;
                }
            })
        };
        for i in 0..1000 {
            RuneSigner::load(if i % 2 == 0 { &a } else { &b }).unwrap();
        }
        stop.store(true, Ordering::Relaxed);
        reader.join().expect("reader saw a partial .pub");
    }
}
