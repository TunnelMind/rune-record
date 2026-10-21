//! ADR-001 attestation receipts: build → id → sign → verify.
//! Receipts are handled as serde_json::Value so optional per-action blocks
//! ride along without schema churn (verifiers must ignore unknown fields).

use anyhow::{bail, Context, Result};
use base64::Engine;
use serde_json::{json, Map, Value};

use crate::signing::{verify_ed25519, RuneSigner};

pub const VERSION: &str = "1.0";
pub const TIERS: [&str; 6] = [
    "self-asserted",
    "software",
    "platform",
    "platform-enclave",
    "tee-tpm",
    "silicon-root",
];
pub const RESULTS: [&str; 3] = ["ok", "error", "degraded"];

/// signed_by prefix → tiers that prefix can prove (ADR-001 honesty rule).
pub fn tiers_for_prefix(signed_by: &str) -> &'static [&'static str] {
    match signed_by.split(':').next().unwrap_or("") {
        "software" => &["self-asserted", "software"],
        "platform" => &["platform", "platform-enclave"],
        "tpm-ek" | "tpm" => &["tee-tpm", "silicon-root"],
        _ => &[],
    }
}

/// RFC 8785 canonical bytes — the one canonicalizer every signed structure
/// in the workspace goes through (receipts, strand certs).
pub fn jcs_bytes(v: &Value) -> Result<Vec<u8>> {
    serde_json_canonicalizer::to_vec(v).context("JCS canonicalization failed")
}

/// Lowercase hex SHA-256 — the one spelling for every digest in the
/// workspace (receipt ids, payload hashes, manifest hash, strand ids).
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(ring::digest::digest(&ring::digest::SHA256, bytes).as_ref())
}

/// "sha256:{hex}" over the JCS canonicalization of a JSON value — the hash
/// form receipts use for query_hash/guidance_hash and friends.
pub fn sha256_jcs(v: &Value) -> Result<String> {
    Ok(format!("sha256:{}", sha256_hex(&jcs_bytes(v)?)))
}

/// ISO 8601 UTC, Z suffix, millisecond precision (ADR-001 timestamp rule).
pub fn iso8601_now_ms() -> String {
    let now = time::OffsetDateTime::now_utc();
    let ms = now.millisecond();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
        ms
    )
}

fn str_at<'a>(v: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut cur = v;
    for p in path {
        cur = cur.get(p)?;
    }
    cur.as_str()
}

fn check_required(r: &Value) -> Result<()> {
    let need: [(&[&str], &str); 7] = [
        (&["version"], "version"),
        (&["tier"], "tier"),
        (&["alg"], "alg"),
        (&["agent", "name"], "agent.name"),
        (&["agent", "manifest_hash"], "agent.manifest_hash"),
        (&["action", "type"], "action.type"),
        (&["action", "result"], "action.result"),
    ];
    for (path, name) in need {
        if str_at(r, path).is_none_or(|s| s.is_empty()) {
            bail!("receipt missing required field: {name}");
        }
    }
    let tier = str_at(r, &["tier"]).unwrap();
    if !TIERS.contains(&tier) {
        bail!("invalid tier: {tier}");
    }
    let result = str_at(r, &["action", "result"]).unwrap();
    if !RESULTS.contains(&result) {
        bail!("invalid action.result: {result}");
    }
    let alg = str_at(r, &["alg"]).unwrap();
    if alg != "ed25519" && alg != "ecdsa-p256" {
        bail!("invalid alg: {alg}");
    }
    Ok(())
}

fn check_tier_prefix(r: &Value) -> Result<()> {
    let tier = str_at(r, &["tier"]).context("missing tier")?;
    let signed_by = str_at(r, &["signed_by"]).context("missing signed_by")?;
    if !tiers_for_prefix(signed_by).contains(&tier) {
        bail!("signed_by prefix '{signed_by}' cannot prove tier '{tier}'");
    }
    Ok(())
}

/// Build and sign a receipt at software tier per ADR-001.
/// `agent_name`/`manifest_hash` identify the Rune; `action` is the per-type
/// fields object ("type" and "result" required, timestamp filled if absent);
/// `extra` carries optional per-action blocks (e.g. {"boot": {...}}).
pub fn build_and_sign(
    signer: &RuneSigner,
    tier: &str,
    agent_name: &str,
    manifest_hash: &str,
    action: Value,
    extra: Option<Map<String, Value>>,
) -> Result<Value> {
    let mut action = match action {
        Value::Object(m) => m,
        _ => bail!("action must be a JSON object"),
    };
    action
        .entry("timestamp")
        .or_insert_with(|| json!(iso8601_now_ms()));
    let timestamp = action["timestamp"]
        .as_str()
        .context("action.timestamp must be a string")?
        .to_string();

    let mut receipt = json!({
        "version": VERSION,
        "tier": tier,
        "alg": "ed25519",
        "agent": { "name": agent_name, "manifest_hash": manifest_hash },
        "action": Value::Object(action),
        "signed_by": signer.signed_by(),
    });
    if let Some(extra) = extra {
        for (k, v) in extra {
            receipt[k] = v;
        }
    }
    check_required(&receipt)?;
    check_tier_prefix(&receipt)?;

    // hex8 from the canonical receipt without id and signature (ADR-001 amended rule).
    let hex8 = &sha256_hex(&jcs_bytes(&receipt)?)[..8];
    receipt["id"] = json!(format!("attest:{agent_name}:{timestamp}:{hex8}"));

    let sig = signer.sign(&jcs_bytes(&receipt)?);
    receipt["signature"] = json!(base64::engine::general_purpose::STANDARD.encode(sig));
    Ok(receipt)
}

/// Full ADR-001 verification: signature, required fields, tier/prefix rule,
/// id recomputation, and signed_by fingerprint vs the supplied pubkey.
pub fn verify(receipt: &Value, pubkey_hex: &str) -> Result<()> {
    check_required(receipt)?;
    check_tier_prefix(receipt)?;

    let signed_by = str_at(receipt, &["signed_by"]).unwrap();
    if let Some(fp) = signed_by.strip_prefix("software:ed25519:") {
        // `get`, not a slice: the pubkey is caller input (argv, a .pub file)
        // and may not have a char boundary at byte 16.
        if pubkey_hex.get(..16) != Some(fp) {
            bail!("signed_by fingerprint does not match supplied pubkey");
        }
    }

    let sig_b64 = str_at(receipt, &["signature"]).context("missing signature")?;
    let sig = base64::engine::general_purpose::STANDARD
        .decode(sig_b64)
        .context("decoding signature base64")?;

    let mut unsigned = receipt.clone();
    unsigned.as_object_mut().unwrap().remove("signature");
    verify_ed25519(pubkey_hex, &jcs_bytes(&unsigned)?, &sig)?;

    let id = str_at(receipt, &["id"]).context("missing id")?.to_string();
    unsigned.as_object_mut().unwrap().remove("id");
    let hex8 = &sha256_hex(&jcs_bytes(&unsigned)?)[..8];
    if !id.ends_with(&format!(":{hex8}")) {
        bail!("id hex8 does not match receipt content");
    }
    let agent = str_at(receipt, &["agent", "name"]).unwrap();
    if !id.starts_with(&format!("attest:{agent}:")) {
        bail!("id does not match agent name");
    }
    Ok(())
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

    fn boot_receipt(s: &RuneSigner) -> Value {
        build_and_sign(
            s,
            "software",
            "scry-rune-001",
            "sha256:abc",
            json!({"type": "boot", "result": "ok"}),
            Some(
                json!({"boot": {"mode": "full", "manifest_hash": "sha256:abc"}})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .unwrap()
    }

    #[test]
    fn sign_verify_roundtrip() {
        let (_d, s) = signer();
        let r = boot_receipt(&s);
        assert!(r["id"]
            .as_str()
            .unwrap()
            .starts_with("attest:scry-rune-001:"));
        verify(&r, s.pubkey_hex()).unwrap();
    }

    #[test]
    fn tamper_is_detected() {
        let (_d, s) = signer();
        let mut r = boot_receipt(&s);
        r["action"]["result"] = json!("error");
        assert!(verify(&r, s.pubkey_hex()).is_err());
    }

    #[test]
    fn software_key_cannot_claim_tee_tpm() {
        let (_d, s) = signer();
        let err = build_and_sign(
            &s,
            "tee-tpm",
            "x",
            "sha256:abc",
            json!({"type": "boot", "result": "ok"}),
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("cannot prove tier"));
    }

    #[test]
    fn wrong_pubkey_fails_fingerprint_check() {
        let (_d, s) = signer();
        let (_d2, other) = signer();
        let r = boot_receipt(&s);
        assert!(verify(&r, other.pubkey_hex()).is_err());
        // Short or non-ASCII pubkeys are an error, not a panic.
        for bad in ["", "abc", "€€€€€€", &s.pubkey_hex()[..15]] {
            assert!(verify(&r, bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn oracle_failure_still_signs() {
        let (_d, s) = signer();
        let r = build_and_sign(
            &s,
            "software",
            "scry-rune-001",
            "sha256:abc",
            json!({"type": "oracle_call", "result": "error"}),
            Some(
                json!({"oracle": {"trigger": "spool-corruption", "model": "claude-sonnet-5"}})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .unwrap();
        verify(&r, s.pubkey_hex()).unwrap();
    }
}
