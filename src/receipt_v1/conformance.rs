//! Conformance with the other Receipt v1.0 implementations: crawl-record
//! (a committed vector) and the JS reference verifier (`~/receipt-verify`).
//! A child module rather than `tests/*.rs` so it can reach the private
//! `signing_bytes` without widening the public API.
//!
//! The vector was made once with crawl-record's `Signer::from_seed([7u8; 32])`
//! and `to_receipt`; `signing_bytes_jcs` is crawl-record's `jcs` of the
//! receipt minus `payload` and `signature.value`.
//!
//! SCRIPT is a synced copy of the one in
//! `tm-crawl/crawl-record/tests/verify_js.rs` (a separate repo with no
//! dependency edge to here, so the harness cannot be shared). When
//! receipt-verify's contract changes, change both. Last synced 2026-10-19
//! (receipt-verify 2.0.0: the key must be pinned as a bundle). The runners
//! differ on purpose: this one skips with a notice when node or
//! ~/receipt-verify is missing (CI has node but not the sibling checkout),
//! crawl-record fails unless TM_SKIP_JS_INTEROP=1.

use std::path::Path;
use std::process::Command;

use super::*;

const VECTOR: &str = include_str!("../../tests/vectors/crawl_record_v1.json");

const SCRIPT: &str = r#"
import { readFileSync } from 'node:fs';
const { verifyReceipt } = await import(process.env.HOME + '/receipt-verify/dist/index.js');
const receipt = JSON.parse(readFileSync(process.argv[1], 'utf8'));
// receipt-verify 2.0.0 never trusts a receipt's embedded key on its own, so the
// signer's key is pinned as the bundle, the way an offline verifier would. The
// bare call (no bundle) must refuse the record. An empty revocation feed keeps
// both calls offline.
const keys = { keys: [{ key_id: receipt.signature.key_id, algorithm: 'Ed25519',
  public_key: receipt.signature.public_key, status: 'active' }] };
const revocations = { feed_version: 1, updated_at: receipt.timestamp, revoked_keys: [], revoked_receipts: [] };
const pinned = await verifyReceipt(receipt, { keys, revocations, noFetchKeys: true });
const bare = await verifyReceipt(receipt, { revocations, noFetchKeys: true });
const pick = (v) => ({ valid: v.valid, errors: v.errors });
console.log(JSON.stringify({ pinned: pick(pinned), bare: pick(bare) }));
"#;

fn vector() -> Value {
    serde_json::from_str(VECTOR).unwrap()
}

/// None (with a notice) when node or the built `~/receipt-verify` checkout is
/// missing (CI runners have node but not the sibling repo); otherwise
/// receipt-verify's `{pinned, bare}` results, each `{valid, errors}`. The
/// receipt goes to a temp file, never under the repo.
fn verify_with_node(receipt: &Value) -> Option<Value> {
    if Command::new("node").arg("--version").output().is_err() {
        eprintln!("notice: node not on PATH; skipping receipt-verify interop test");
        return None;
    }
    let dist = std::env::var_os("HOME")
        .map(|h| Path::new(&h).join("receipt-verify/dist/index.js"))
        .filter(|p| p.exists());
    if dist.is_none() {
        eprintln!(
            "notice: ~/receipt-verify/dist/index.js missing; skipping receipt-verify interop test"
        );
        return None;
    }
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("receipt.json");
    std::fs::write(&path, receipt.to_string()).unwrap();
    let out = Command::new("node")
        .args(["--input-type=module", "-e", SCRIPT])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "node failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Some(serde_json::from_slice(&out.stdout).unwrap())
}

fn rune_signed(dir: &Path) -> Value {
    let s = RuneSigner::load_or_generate(&dir.join("node.key")).unwrap();
    let source = Source {
        lens: "rune".into(),
        endpoint: "/boot".into(),
        node_id: "scry-rune-001".into(),
    };
    let payload = json!({"title": "Ünïcode \u{1f600} \"quoted\"\n", "n": [1, -2], "ok": true});
    sign(payload, &s, source, Options::default()).unwrap()
}

#[test]
fn crawl_record_vector_verifies() {
    let v = vector();
    let got = verify(&v["receipt"]).unwrap();
    assert_eq!(got.key_id, "tm-crawl-fe812c12");
}

#[test]
fn signing_bytes_match_crawl_record_jcs() {
    let v = vector();
    let want = v["signing_bytes_jcs"].as_str().unwrap().as_bytes();
    assert_eq!(signing_bytes(&v["receipt"]).unwrap(), want);
}

#[test]
fn rune_signed_verifies_in_js() {
    let d = tempfile::tempdir().unwrap();
    let mut r = rune_signed(d.path());
    let Some(out) = verify_with_node(&r) else {
        return;
    };
    assert_eq!(out["pinned"]["valid"], true, "{out}");
    assert_eq!(
        out["bare"]["valid"], false,
        "unpinned receipt must be refused: {out}"
    );

    r["payload"]["ok"] = json!(false);
    let out = verify_with_node(&r).unwrap();
    assert_eq!(out["pinned"]["valid"], false, "{out}");
}

#[test]
fn crawl_record_vector_verifies_in_js() {
    if let Some(out) = verify_with_node(&vector()["receipt"]) {
        assert_eq!(out["pinned"]["valid"], true, "{out}");
    }
}
