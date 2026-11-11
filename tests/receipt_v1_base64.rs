//! Public-API-only base64 tests for Receipt v1.0, moved out of
//! `src/receipt_v1.rs` (T002) so a byte-for-byte sync of `src/` from rune
//! does not drop them. Bodies are unchanged; only imports and the small
//! `signer`/`source`/`signed` helpers are re-created over the public API.

use anyhow::Result;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rune_record::receipt_v1::{sign, verify, Options, Source};
use rune_record::signing::RuneSigner;
use serde_json::{json, Value};
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
fn standard_engine_refuses_non_canonical_base64() {
    assert_eq!(B64.decode("Zg==").unwrap(), b"f");
    for s in [
        "Zh==",
        "Zm9=",
        "Zg=a",
        "Z=g=",
        "=Zg=",
        "Zg==Zg==",
        "Zm9v\n",
        "Zm9v\r\nYg==",
        "Zm9vYg==\n",
        "====",
    ] {
        assert!(B64.decode(s).is_err(), "STANDARD accepted {s:?}");
    }
}

/// Re-spellings of one canonical base64 string: trailing "\n", the last
/// data char swapped so only its unused trailing bits differ, and an
/// inner "\r\n". None is canonical, so each must fail decode and verify.
fn respellings(b64: &str) -> Vec<String> {
    const ALPHA: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bytes = b64.as_bytes().to_vec();
    let i = b64.trim_end_matches('=').len() - 1;
    let six = ALPHA.iter().position(|&c| c == bytes[i]).unwrap();
    // The low bit is always unused when the string ends in "=" or "==".
    bytes[i] = ALPHA[six ^ 1];
    let trailing_bits = String::from_utf8(bytes).unwrap();
    assert_ne!(trailing_bits, b64);
    let mid = b64.len() / 2;
    vec![
        format!("{b64}\n"),
        trailing_bits,
        format!("{}\r\n{}", &b64[..mid], &b64[mid..]),
    ]
}

#[test]
fn respelled_signature_fields_fail_verify() {
    let (_d, s) = signer();
    let r = signed(&s, json!({"ok": true}), Options::default()).unwrap();
    verify(&r).unwrap();
    for field in ["value", "public_key"] {
        let orig = r["signature"][field].as_str().unwrap().to_string();
        assert!(orig.ends_with('='));
        for spelling in respellings(&orig) {
            assert!(
                B64.decode(&spelling).is_err(),
                "{field}: {spelling:?} decoded"
            );
            let mut bad = r.clone();
            bad["signature"][field] = json!(spelling);
            assert!(verify(&bad).is_err(), "{field}: {spelling:?} verified");
        }
    }
}
