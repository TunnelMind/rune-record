//! Shared cross-language vectors for `verify_pinned` (spec 002, T005):
//! `vectors/receipt-v1/*.json` at the repo root. The runner checks every
//! committed file's `expected` verdict against the Rust verifier; the
//! `#[ignore]` generator below wrote them (see the README there).

use std::path::PathBuf;

use serde_json::Value;

use super::*;

fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vectors/receipt-v1")
}

#[test]
fn receipt_v1_vectors_agree() {
    let dir = vectors_dir();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    assert!(
        files.len() >= 10,
        "want >= 10 vectors, found {}",
        files.len()
    );
    for f in &files {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(f).unwrap()).unwrap();
        let field = |k: &str| {
            v[k].as_str()
                .unwrap_or_else(|| panic!("{}: `{k}` must be a string", f.display()))
        };
        let (name, _desc, _js) = (
            field("name"),
            field("description"),
            field("js_receipt_verify_2_0_0"),
        );
        let (receipt, expected) = (field("receipt"), field("expected"));
        assert!(
            expected == "valid" || VERDICTS.contains(&expected),
            "{name}: expected `{expected}` is not a verdict"
        );
        let trust: TrustBundle = serde_json::from_value(v["trust"].clone())
            .unwrap_or_else(|e| panic!("{name}: trust: {e}"));
        match verify_pinned(receipt, &trust) {
            Ok(_) => assert_eq!(expected, "valid", "{name}: verified, expected {expected}"),
            Err(r) => assert_eq!(r.verdict, expected, "{name}: {r}"),
        }
    }
}

/// Writes the ten vectors. Run by hand only:
/// `cargo test -p rune-record -- --ignored write_receipt_v1_vectors`.
/// The key lives in a tempdir and dies with it; only its public half lands in
/// `trust`. Every fault that touches a signed field is re-signed after the
/// edit, so the named fault is the receipt's only one.
#[test]
#[ignore]
fn write_receipt_v1_vectors() {
    use crate::receipt::{jcs_bytes, sha256_hex};
    use crate::signing::RuneSigner;
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine;
    use serde_json::json;

    let tmp = tempfile::tempdir().unwrap();
    let s = RuneSigner::load_or_generate(&tmp.path().join("node.key")).unwrap();
    let source = Source {
        lens: "rune".into(),
        endpoint: "/boot".into(),
        node_id: "scry-rune-001".into(),
    };
    let opts = Options {
        key_id: Some("k1".into()),
        ..Options::default()
    };
    let base = sign(json!({"ok": true, "n": [1, -2]}), &s, source, opts).unwrap();
    let resign = |mut r: Value| {
        r["payload_hash"] = format!("0x{}", sha256_hex(&jcs_bytes(&r["payload"]).unwrap())).into();
        r["signature"]["value"] = B64.encode(s.sign(&signing_bytes(&r).unwrap())).into();
        r.to_string()
    };
    let edit = |f: &dyn Fn(&mut Value)| {
        let mut r = base.clone();
        f(&mut r);
        resign(r)
    };
    let key = |extra: Value| {
        let mut k = json!({
            "key_id": "k1",
            "algorithm": "Ed25519",
            "status": "active",
            "public_key": B64.encode(hex::decode(s.pubkey_hex()).unwrap()),
        });
        for (n, v) in extra.as_object().unwrap() {
            k[n] = v.clone();
        }
        k
    };
    let trust = |keys: Vec<Value>, revoked_keys: Value| {
        json!({"keys": {"keys": keys},
               "revocations": {"revoked_keys": revoked_keys, "revoked_receipts": []}})
    };
    let plain = trust(vec![key(json!({}))], json!([]));
    let valid = base.to_string();

    // (name, description, receipt text, trust, expected, js_receipt_verify_2_0_0)
    let vectors = [
        ("valid", "A fresh receipt under its own active key.",
         valid.clone(), plain.clone(), "valid", "valid"),
        ("unknown_key", "The bundle does not hold the receipt's key_id (k1).",
         valid.clone(), trust(vec![key(json!({"key_id": "someone-else"}))], json!([])),
         "unknown_key", "invalid"),
        ("expired_key", "The key's rotation_after (2026-01-01) precedes the receipt's timestamp.",
         valid.clone(), trust(vec![key(json!({"rotation_after": "2026-01-01T00:00:00Z"}))], json!([])),
         "key_expired", "invalid"),
        ("revoked_key", "The revocation feed revokes k1 for compromise (rejects whatever the date).",
         valid.clone(),
         trust(vec![key(json!({}))],
               json!([{"key_id": "k1", "revoked_at": "2026-01-01T00:00:00Z", "reason": "compromise"}])),
         "key_revoked", "invalid"),
        ("tier_too_high", "The receipt claims tee-tpm; the bundle caps the key at software. Re-signed.",
         edit(&|r| r["attestation_strength"] = "tee-tpm".into()),
         trust(vec![key(json!({"attestation_strength": "software"}))], json!([])),
         "tier_too_high", "invalid"),
        ("missing_public_key", "signature.public_key removed, then re-signed; JS 2.0.0 skips the comparison.",
         edit(&|r| { r["signature"].as_object_mut().unwrap().remove("public_key"); }),
         plain.clone(), "missing_public_key", "valid"),
        ("missing_receipt_id", "receipt_id removed, then re-signed; JS 2.0.0 accepts it.",
         edit(&|r| { r.as_object_mut().unwrap().remove("receipt_id"); }),
         plain.clone(), "missing_receipt_id", "valid"),
        ("float_payload", "Payload holds 1.5; payload_hash recomputed and re-signed; JS 2.0.0 accepts it.",
         edit(&|r| r["payload"] = json!({"x": 1.5})),
         plain.clone(), "non_integer_number", "valid"),
        ("duplicate_key", "Text edit of the valid receipt: a second \"receipt_version\":\"1.0\" pair. JSON.parse keeps the last.",
         valid.replacen('{', r#"{"receipt_version":"1.0","#, 1),
         plain.clone(), "duplicate_key", "valid"),
        ("bad_timestamp", "timestamp is \"not a date\", then re-signed.",
         edit(&|r| r["timestamp"] = "not a date".into()),
         plain.clone(), "bad_timestamp", "invalid"),
    ];
    let dir = vectors_dir();
    std::fs::create_dir_all(&dir).unwrap();
    for (name, description, receipt, trust, expected, js) in vectors {
        let v = json!({"name": name, "description": description, "receipt": receipt,
                       "trust": trust, "expected": expected, "js_receipt_verify_2_0_0": js});
        let text = serde_json::to_string_pretty(&v).unwrap() + "\n";
        std::fs::write(dir.join(format!("{name}.json")), text).unwrap();
    }
}
