//! TunnelMind Receipt Format v1.0 — the one receipt format across TunnelMind.
//! Same envelope and bytes as the data API's `wrapReceipt` and crawl-record's
//! `to_receipt_for`, so `@tunnelmindai/receipt-verify` checks Rune receipts too.
//!
//! `payload_hash` is "0x" + hex SHA-256 of the payload's JCS bytes; the
//! Ed25519 signature covers the JCS bytes of the receipt with `payload` and
//! `signature.value` removed (receipt-verify's `buildSigningObject`).

use anyhow::{bail, ensure, Context, Result};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde_json::{json, Value};

use crate::receipt::{iso8601_now_ms, jcs_bytes};
use crate::signing::{verify_ed25519, RuneSigner};

pub const RECEIPT_VERSION: &str = "1.0";

/// `source` block: which tool (lens) and part of it (endpoint) produced the
/// receipt, on which node.
pub struct Source {
    pub lens: String,
    pub endpoint: String,
    pub node_id: String,
}

#[derive(Default)]
pub struct Options {
    /// Defaults to "rune-" + first 8 hex of sha256(raw pubkey).
    pub key_id: Option<String>,
    /// Defaults to "software".
    pub attestation_strength: Option<String>,
    pub subject: Option<Value>,
    pub extensions: Option<Value>,
    /// `signature.value` of the previous receipt in this chain, if any.
    pub previous_signature: Option<String>,
    pub sequence: u64,
}

/// What a successful [`verify`] vouches for. Whether `public_key` is a
/// trusted TunnelMind key is the caller's call (key bundle, revocations).
#[derive(Debug)]
pub struct Verified {
    pub receipt_id: String,
    pub key_id: String,
    pub public_key: String,
}

fn sha256_0x(bytes: &[u8]) -> String {
    format!(
        "0x{}",
        hex::encode(ring::digest::digest(&ring::digest::SHA256, bytes).as_ref())
    )
}

/// Refuse what JS would format or round differently from Rust: non-integers
/// and integers outside ±(2^53-1). Same rule as crawl-record's `jcs_into`.
fn check_numbers(v: &Value) -> Result<()> {
    const MAX_SAFE: u64 = (1 << 53) - 1;
    match v {
        Value::Number(n) => {
            let safe = n.as_u64().is_some_and(|u| u <= MAX_SAFE)
                || n.as_i64().is_some_and(|i| i.unsigned_abs() <= MAX_SAFE);
            ensure!(safe, "receipt: number {n} is not a JS-safe integer");
        }
        Value::Array(a) => a.iter().try_for_each(check_numbers)?,
        Value::Object(m) => m.values().try_for_each(check_numbers)?,
        _ => {}
    }
    Ok(())
}

/// The bytes the signature covers.
fn signing_bytes(receipt: &Value) -> Result<Vec<u8>> {
    let mut signing = receipt.clone();
    let obj = signing
        .as_object_mut()
        .context("receipt must be an object")?;
    obj.remove("payload");
    if let Some(sig) = obj.get_mut("signature").and_then(Value::as_object_mut) {
        sig.remove("value");
    }
    jcs_bytes(&signing)
}

/// Wrap `payload` as a signed Receipt v1.0. Errors (rather than panics) on
/// payloads with non-integer or unsafe numbers.
pub fn sign(payload: Value, signer: &RuneSigner, source: Source, opts: Options) -> Result<Value> {
    let raw_pubkey = hex::decode(signer.pubkey_hex()).context("decoding signer pubkey hex")?;
    let key_id = opts
        .key_id
        .unwrap_or_else(|| format!("rune-{}", &sha256_0x(&raw_pubkey)[2..10]));
    let mut receipt = json!({
        "receipt_version": RECEIPT_VERSION,
        "receipt_id": uuid::Uuid::now_v7().to_string(),
        "timestamp": iso8601_now_ms(),
        "timestamp_proof": {"method": "none"},
        "source": {"lens": source.lens, "endpoint": source.endpoint, "node_id": source.node_id},
        "attestation_strength": opts.attestation_strength.as_deref().unwrap_or("software"),
        "payload_hash": sha256_0x(&jcs_bytes(&payload)?),
        "payload": payload,
        "chain": {
            "previous_receipt_hash": opts.previous_signature.map(|s| sha256_0x(s.as_bytes())),
            "sequence": opts.sequence,
        },
        "signature": {
            "algorithm": "Ed25519",
            "key_id": key_id,
            "public_key": B64.encode(&raw_pubkey),
        },
    });
    if let Some(subject) = opts.subject {
        receipt["subject"] = subject;
    }
    if let Some(extensions) = opts.extensions {
        receipt["extensions"] = extensions;
    }
    // Over the whole receipt, not just payload: subject, extensions and
    // sequence are signed too.
    check_numbers(&receipt)?;

    let sig = signer.sign(&signing_bytes(&receipt)?);
    receipt["signature"]["value"] = B64.encode(sig).into();
    Ok(receipt)
}

/// Pure-crypto verification (receipt-verify §4 steps 1-3): version, payload
/// hash, and the signature against the embedded public key. No network.
pub fn verify(receipt: &Value) -> Result<Verified> {
    let str_at = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);

    // receipt-verify accepts exactly "1.0"; unknown majors MUST be rejected.
    let version = str_at(receipt, "receipt_version").unwrap_or_default();
    if version != RECEIPT_VERSION {
        bail!("unsupported receipt_version {version:?} (expected {RECEIPT_VERSION})");
    }

    let payload = receipt.get("payload").context("missing payload")?;
    let want = str_at(receipt, "payload_hash").context("missing payload_hash")?;
    ensure!(
        sha256_0x(&jcs_bytes(payload)?) == want,
        "payload_hash does not match payload"
    );

    let sig = receipt.get("signature").context("missing signature")?;
    ensure!(
        str_at(sig, "algorithm").as_deref() == Some("Ed25519"),
        "unsupported signature.algorithm"
    );
    let public_key = str_at(sig, "public_key").context("missing signature.public_key")?;
    let raw_pubkey = B64
        .decode(&public_key)
        .context("decoding signature.public_key base64")?;
    ensure!(
        raw_pubkey.len() == 32,
        "signature.public_key is not 32 bytes"
    );
    let value = B64
        .decode(str_at(sig, "value").context("missing signature.value")?)
        .context("decoding signature.value base64")?;
    verify_ed25519(&hex::encode(&raw_pubkey), &signing_bytes(receipt)?, &value)?;

    Ok(Verified {
        receipt_id: str_at(receipt, "receipt_id").context("missing receipt_id")?,
        key_id: str_at(sig, "key_id").context("missing signature.key_id")?,
        public_key,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn signer() -> (tempfile::TempDir, RuneSigner) {
        let d = tempdir().unwrap();
        let s = RuneSigner::load_or_generate(&d.path().join("node.key")).unwrap();
        (d, s)
    }

    fn source() -> Source {
        Source {
            lens: "rune".into(),
            endpoint: "/boot".into(),
            node_id: "scry-rune-001".into(),
        }
    }

    fn signed(s: &RuneSigner, payload: Value, opts: Options) -> Result<Value> {
        sign(payload, s, source(), opts)
    }

    #[test]
    fn round_trip() {
        let (_d, s) = signer();
        let r = signed(&s, json!({"ok": true, "n": [1, -2]}), Options::default()).unwrap();
        let mut keys: Vec<&str> = r.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(
            keys,
            [
                "attestation_strength",
                "chain",
                "payload",
                "payload_hash",
                "receipt_id",
                "receipt_version",
                "signature",
                "source",
                "timestamp",
                "timestamp_proof",
            ]
        );
        assert_eq!(r["attestation_strength"], "software");
        assert_eq!(
            r["chain"],
            json!({"previous_receipt_hash": null, "sequence": 0})
        );
        let ts = r["timestamp"].as_str().unwrap();
        assert!(ts.len() == 24 && ts.ends_with('Z') && &ts[19..20] == ".");
        assert_eq!(&r["receipt_id"].as_str().unwrap()[14..15], "7");

        let v = verify(&r).unwrap();
        assert_eq!(v.receipt_id, r["receipt_id"].as_str().unwrap());
        assert_eq!(
            B64.decode(&v.public_key).unwrap(),
            hex::decode(s.pubkey_hex()).unwrap()
        );

        // Subject and extensions are signed when present.
        let opts = Options {
            subject: Some(json!("ip:192.0.2.1")),
            extensions: Some(json!({"x": 1})),
            ..Default::default()
        };
        let mut r = signed(&s, json!({}), opts).unwrap();
        verify(&r).unwrap();
        r["extensions"]["x"] = json!(2);
        assert!(verify(&r).is_err());
    }

    #[test]
    fn tampered_payload_fails() {
        let (_d, s) = signer();
        let mut r = signed(&s, json!({"ok": true}), Options::default()).unwrap();
        r["payload"]["ok"] = json!(false);
        assert!(verify(&r).unwrap_err().to_string().contains("payload_hash"));
        // Recomputing the hash does not help: payload_hash is signed.
        r["payload_hash"] = sha256_0x(&jcs_bytes(&r["payload"]).unwrap()).into();
        assert!(verify(&r).is_err());
    }

    #[test]
    fn tampered_signature_fails() {
        let (_d, s) = signer();
        let mut r = signed(&s, json!({"ok": true}), Options::default()).unwrap();
        let mut sig = B64
            .decode(r["signature"]["value"].as_str().unwrap())
            .unwrap();
        sig[0] ^= 1;
        r["signature"]["value"] = B64.encode(sig).into();
        assert!(verify(&r).is_err());
    }

    #[test]
    fn unknown_major_version_rejected() {
        let (_d, s) = signer();
        let mut r = signed(&s, json!({}), Options::default()).unwrap();
        r["receipt_version"] = json!("2.0");
        assert!(verify(&r)
            .unwrap_err()
            .to_string()
            .contains("receipt_version"));
    }

    #[test]
    fn unsafe_numbers_refused() {
        let (_d, s) = signer();
        assert!(signed(&s, json!({"x": 1.5}), Options::default()).is_err());
        assert!(signed(&s, json!({"x": [1u64 << 53]}), Options::default()).is_err());
        assert!(signed(&s, json!({"x": -((1i64 << 53) - 1)}), Options::default()).is_ok());
    }

    #[test]
    fn default_and_explicit_key_id() {
        let (_d, s) = signer();
        let r = signed(&s, json!({}), Options::default()).unwrap();
        let raw = hex::decode(s.pubkey_hex()).unwrap();
        let want = format!("rune-{}", &sha256_0x(&raw)[2..10]);
        assert_eq!(r["signature"]["key_id"], want.as_str());
        assert_eq!(verify(&r).unwrap().key_id, want);

        let opts = Options {
            key_id: Some("tm-receipt-2026-10".into()),
            ..Default::default()
        };
        let r = signed(&s, json!({}), opts).unwrap();
        assert_eq!(verify(&r).unwrap().key_id, "tm-receipt-2026-10");
    }

    #[test]
    fn chain_hash_from_previous_signature() {
        let (_d, s) = signer();
        let first = signed(&s, json!({"n": 1}), Options::default()).unwrap();
        let prev = first["signature"]["value"].as_str().unwrap().to_string();
        let opts = Options {
            previous_signature: Some(prev.clone()),
            sequence: 1,
            ..Default::default()
        };
        let second = signed(&s, json!({"n": 2}), opts).unwrap();
        let want = format!(
            "0x{}",
            hex::encode(ring::digest::digest(&ring::digest::SHA256, prev.as_bytes()))
        );
        assert_eq!(
            second["chain"],
            json!({"previous_receipt_hash": want, "sequence": 1})
        );
        verify(&second).unwrap();
    }
}

#[cfg(test)]
mod conformance;
