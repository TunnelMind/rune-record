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
    // Only Ed25519 is implemented; a record labelled otherwise must not pass
    // on the strength of an Ed25519 signature it did not claim to carry.
    let alg = str_at(receipt, &["alg"]).unwrap();
    if alg != "ed25519" {
        bail!("cannot verify alg '{alg}': only ed25519 is implemented");
    }

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

    /// xorshift64*: reproducible random inputs without a dev-dependency.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }
        fn pick<'a>(&mut self, from: &[&'a str]) -> &'a str {
            from[(self.next() % from.len() as u64) as usize]
        }
    }

    /// A random JSON value up to `depth` levels deep. Strings mix the shapes
    /// canonicalisers disagree on: escapes, control characters, non-BMP
    /// characters (UTF-16 surrogate pairs), U+2028, combining marks. Numbers
    /// are JS-safe integers only, as every signed TunnelMind record is.
    fn random_value(rng: &mut Rng, depth: u32) -> Value {
        const PIECES: &[&str] = &[
            "a", "Z", "0", " ", "\"", "\\", "/", "\n", "\t", "\u{0}", "\u{1f}", "\u{7f}", "é",
            "\u{301}", "\u{2028}", "\u{e000}", "\u{ffff}", "😀", "\u{10ffff}", "中",
        ];
        let s = |rng: &mut Rng| -> String {
            (0..rng.next() % 6).map(|_| rng.pick(PIECES)).collect()
        };
        match (depth, rng.next() % 7) {
            (_, 0) => Value::Null,
            (_, 1) => Value::Bool(rng.next() & 1 == 0),
            (_, 2) => {
                let magnitude = (rng.next() % (1 << 53)) as i64;
                json!(if rng.next() & 1 == 0 { magnitude } else { -magnitude })
            }
            (0, _) | (_, 3) => Value::String(s(rng)),
            (_, 4) => (0..rng.next() % 4).map(|_| random_value(rng, depth - 1)).collect(),
            _ => Value::Object(
                (0..rng.next() % 5)
                    .map(|_| (s(rng), random_value(rng, depth - 1)))
                    .collect(),
            ),
        }
    }

    /// The canonical bytes are a fixed point: parsing them back gives the
    /// same value, and canonicalising that again gives the same bytes. If
    /// either failed, a verifier that re-parses a receipt would hash
    /// different bytes from the ones that were signed.
    #[test]
    fn jcs_is_a_fixed_point_on_random_values() {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        for _ in 0..2000 {
            let v = random_value(&mut rng, 4);
            let canon = jcs_bytes(&v).unwrap();
            let back: Value = serde_json::from_slice(&canon).unwrap();
            assert_eq!(back, v);
            assert_eq!(jcs_bytes(&back).unwrap(), canon, "{v}");
        }
    }

    /// RFC 8785 sorts keys by UTF-16 code units, not by bytes or chars: a
    /// non-BMP key (a surrogate pair, 0xD83D...) sorts before U+E000, though
    /// its UTF-8 bytes sort after. A canonicaliser that sorted bytes would
    /// sign other bytes than receipt-verify checks. Lone surrogates cannot
    /// reach the canonicaliser at all: the JSON parser refuses them.
    #[test]
    fn jcs_sorts_keys_by_utf16_and_lone_surrogates_never_parse() {
        let v = json!({"\u{e000}": 1, "😀": 2, "a": 3});
        assert_eq!(
            String::from_utf8(jcs_bytes(&v).unwrap()).unwrap(),
            "{\"a\":3,\"😀\":2,\"\u{e000}\":1}"
        );
        for lone in [r#""\ud800""#, r#""\udc00x""#, r#"{"\ud83d":1}"#] {
            assert!(serde_json::from_str::<Value>(lone).is_err(), "{lone}");
        }
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

    /// The signature covers every byte: flipping one bit of it, of the id,
    /// or removing the signature all fail, each with its own reason.
    #[test]
    fn one_bit_or_one_field_off_does_not_verify() {
        let (_d, s) = signer();
        let r = boot_receipt(&s);
        let err = |r: &Value| verify(r, s.pubkey_hex()).unwrap_err().to_string();

        let mut flipped = r.clone();
        let mut sig = base64::engine::general_purpose::STANDARD
            .decode(r["signature"].as_str().unwrap())
            .unwrap();
        sig[17] ^= 0x01;
        flipped["signature"] = json!(base64::engine::general_purpose::STANDARD.encode(sig));
        assert!(err(&flipped).contains("signature verification failed"));

        // The id is inside the signed bytes, so a changed id fails on the
        // signature before the hex8 recomputation gets a turn.
        let mut renamed = r.clone();
        renamed["id"] = json!("attest:scry-rune-001:2026-01-01T00:00:00.000Z:00000000");
        assert!(err(&renamed).contains("signature verification failed"));

        let mut missing = r.clone();
        missing.as_object_mut().unwrap().remove("signature");
        assert!(err(&missing).contains("missing signature"));

        let mut not_b64 = r.clone();
        not_b64["signature"] = json!("***");
        assert!(err(&not_b64).contains("base64"));

        for (path, value) in [("tier", "silicon-root"), ("tier", "bogus")] {
            let mut bad = r.clone();
            bad[path] = json!(value);
            assert!(verify(&bad, s.pubkey_hex()).is_err(), "{path}={value}");
        }
        verify(&r, s.pubkey_hex()).unwrap();
    }

    #[test]
    fn prefix_tier_table() {
        assert_eq!(tiers_for_prefix("software:ed25519:abcd"), ["self-asserted", "software"]);
        assert_eq!(tiers_for_prefix("platform:x"), ["platform", "platform-enclave"]);
        assert_eq!(tiers_for_prefix("tpm-ek:x"), ["tee-tpm", "silicon-root"]);
        assert_eq!(tiers_for_prefix("tpm"), ["tee-tpm", "silicon-root"]);
        for unknown in ["", ":", "SOFTWARE:ed25519:x", "hardware:x"] {
            assert!(tiers_for_prefix(unknown).is_empty(), "{unknown:?}");
        }
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
    fn mislabelled_alg_does_not_verify() {
        let (_d, s) = signer();
        // `extra` overwrites envelope keys, so this is the attack input: an
        // Ed25519 signature under an "ecdsa-p256" label.
        let r = build_and_sign(
            &s,
            "software",
            "x",
            "sha256:abc",
            json!({"type": "boot", "result": "ok"}),
            Some(json!({"alg": "ecdsa-p256"}).as_object().unwrap().clone()),
        )
        .unwrap();
        assert_eq!(r["alg"], "ecdsa-p256");
        let err = verify(&r, s.pubkey_hex()).unwrap_err().to_string();
        assert!(err.contains("only ed25519"), "{err}");
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
