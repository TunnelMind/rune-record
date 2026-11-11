//! Strict JSON intake for Receipt v1.0: text → `Value`, refusing what Rust
//! and JS could read differently.
//!
//! Why not `serde_json::from_str`: it silently keeps the last of two equal
//! object keys (so does JS `JSON.parse`, but another reader may keep the
//! first), and once text is a `Value` the duplicate is gone and nobody
//! downstream can see it. Numbers are classified here too, so a refusal can
//! say `non_integer_number` or `unsafe_integer` (the verdict names in spec
//! 002's table) rather than `check_numbers`'s one message.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;
use serde_json::{Map, Number, Value};
use std::cell::Cell;
use std::fmt;
use time::{Date, Month, PrimitiveDateTime, Time, UtcOffset};

use super::{sha256_0x, signing_bytes, RECEIPT_VERSION};
use crate::receipt::jcs_bytes;
use crate::signing::verify_ed25519;

/// Every verdict name `parse_strict` / the pinned verifier can return, in
/// the spec's table order (the first failing row wins). `valid` is the
/// absence of a failure and is not listed.
pub const VERDICTS: &[&str] = &[
    "malformed",
    "duplicate_key",
    "non_integer_number",
    "unsafe_integer",
    "unsupported_version",
    "payload_hash_mismatch",
    "missing_key_id",
    "unknown_key",
    "bad_timestamp",
    "missing_public_key",
    "key_mismatch",
    "key_inactive",
    "bad_bundle",
    "key_not_yet_valid",
    "key_expired",
    "unsupported_algorithm",
    "bad_signature",
    "unknown_strength",
    "tier_too_high",
    "key_revoked",
    "receipt_revoked",
    "missing_receipt_id",
];

/// A refusal: one name from `VERDICTS` plus a human detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejection {
    pub verdict: &'static str,
    pub detail: String,
}

impl fmt::Display for Rejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.verdict, self.detail)
    }
}

impl std::error::Error for Rejection {}

/// Parse JSON text, refusing duplicate keys at any depth (`duplicate_key`),
/// numbers with a fraction or exponent (`non_integer_number`, so `1.0` too,
/// as `check_numbers` does) and integers outside ±(2^53−1)
/// (`unsafe_integer`). Anything else that is not one JSON value is
/// `malformed`. On success the `Value` equals `serde_json::from_str`'s.
pub fn parse_strict(text: &str) -> Result<Value, Rejection> {
    // serde_json wraps visitor errors in its own type; the visitor records
    // which verdict it raised here so it survives the trip.
    let verdict = Cell::new(None);
    let mut de = serde_json::Deserializer::from_str(text);
    let result = Strict(&verdict)
        .deserialize(&mut de)
        .and_then(|v| de.end().map(|()| v));
    result.map_err(|e| Rejection {
        verdict: verdict.get().unwrap_or("malformed"),
        detail: e.to_string(),
    })
}

const MAX_SAFE: u64 = (1 << 53) - 1;

#[derive(Clone, Copy)]
struct Strict<'a>(&'a Cell<Option<&'static str>>);

impl Strict<'_> {
    fn refuse<E: de::Error>(self, verdict: &'static str, detail: String) -> E {
        self.0.set(Some(verdict));
        E::custom(detail)
    }
}

impl<'de> DeserializeSeed<'de> for Strict<'_> {
    type Value = Value;
    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Strict<'_> {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_bool<E>(self, b: bool) -> Result<Value, E> {
        Ok(Value::Bool(b))
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_str<E>(self, s: &str) -> Result<Value, E> {
        Ok(Value::String(s.to_owned()))
    }

    fn visit_string<E>(self, s: String) -> Result<Value, E> {
        Ok(Value::String(s))
    }

    fn visit_u64<E: de::Error>(self, n: u64) -> Result<Value, E> {
        if n > MAX_SAFE {
            return Err(self.refuse("unsafe_integer", format!("{n} is outside ±(2^53-1)")));
        }
        Ok(n.into())
    }

    fn visit_i64<E: de::Error>(self, n: i64) -> Result<Value, E> {
        if n.unsigned_abs() > MAX_SAFE {
            return Err(self.refuse("unsafe_integer", format!("{n} is outside ±(2^53-1)")));
        }
        Ok(n.into())
    }

    /// serde_json lands here for any token with a fraction or exponent, and
    /// for an integer token too big for u64/i64.
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        if v.fract() == 0.0 && v.abs() > MAX_SAFE as f64 {
            return Err(self.refuse("unsafe_integer", format!("{v} is outside ±(2^53-1)")));
        }
        let shown = Number::from_f64(v).map_or_else(|| v.to_string(), |n| n.to_string());
        Err(self.refuse("non_integer_number", format!("{shown} is not an integer")))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut out = Vec::new();
        while let Some(v) = seq.next_element_seed(self)? {
            out.push(v);
        }
        Ok(Value::Array(out))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut out = Map::new();
        while let Some(k) = map.next_key::<String>()? {
            if out.contains_key(&k) {
                return Err(self.refuse("duplicate_key", format!("key {k:?} appears twice")));
            }
            let v = map.next_value_seed(self)?;
            out.insert(k, v);
        }
        Ok(Value::Object(out))
    }
}

/// What `verify_pinned` trusts: receipt-verify's `KeyBundle` plus its
/// `RevocationFeed`, as `{"keys": {"keys": [..]}, "revocations": {..}}`.
/// Unknown fields are ignored (the JS shapes carry more than Rust reads).
/// `revocations` is required: a missing feed fails closed, as JS does when
/// the feed cannot be fetched.
#[derive(Debug, Clone, Deserialize)]
pub struct TrustBundle {
    pub keys: KeyBundle,
    pub revocations: RevocationFeed,
}

/// receipt-verify `KeyBundle` (keys.ts), only the field Rust reads.
#[derive(Debug, Clone, Deserialize)]
pub struct KeyBundle {
    pub keys: Vec<KeyEntry>,
}

/// receipt-verify `SigningKeyEntry` (keys.ts). `public_key` is base64 of the
/// raw 32 bytes, compared as a string to the receipt's embedded one.
#[derive(Debug, Clone, Deserialize)]
pub struct KeyEntry {
    pub key_id: String,
    pub public_key: String,
    pub algorithm: Option<String>,
    /// Missing counts as `active`; `active` and `rotated` may sign.
    pub status: Option<String>,
    pub created_at: Option<String>,
    pub rotation_after: Option<String>,
    pub attestation_strength: Option<String>,
}

/// receipt-verify `RevocationFeed` (revocation.ts). The arrays default to
/// empty, as JS normalises them.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RevocationFeed {
    #[serde(default)]
    pub revoked_keys: Vec<RevokedKey>,
    #[serde(default)]
    pub revoked_receipts: Vec<RevokedReceipt>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RevokedKey {
    pub key_id: String,
    pub revoked_at: String,
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RevokedReceipt {
    pub receipt_id: String,
    pub revoked_at: String,
    pub reason: String,
}

impl TrustBundle {
    /// One raw public key (64 hex) as `key_id`: active, no window, no tier,
    /// empty revocation feed — what `--pubkey` means. Not-hex input becomes
    /// an empty key that matches no receipt (fail closed); callers validate
    /// the hex first to give a usable error.
    pub fn pin(pubkey_hex: &str, key_id: &str) -> TrustBundle {
        let public_key = hex::decode(pubkey_hex)
            .map(|raw| B64.encode(raw))
            .unwrap_or_default();
        TrustBundle {
            keys: KeyBundle {
                keys: vec![KeyEntry {
                    key_id: key_id.to_owned(),
                    public_key,
                    algorithm: Some("Ed25519".into()),
                    status: None,
                    created_at: None,
                    rotation_after: None,
                    attestation_strength: None,
                }],
            },
            revocations: RevocationFeed::default(),
        }
    }
}

/// What a passing `verify_pinned` vouches for: the receipt is intact, signed
/// by `public_key`, which the bundle names `key_id` and allowed to sign at
/// the receipt's timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trusted {
    pub receipt_id: String,
    pub key_id: String,
    pub public_key: String,
    /// Non-fatal findings (JS `warnings`); a pass may carry some.
    pub warnings: Vec<String>,
}

/// The four attestation tiers in rank order (index.ts STRENGTH_RANK).
const STRENGTH_RANK: [&str; 4] = ["self-asserted", "software", "tee-tpm", "silicon-root"];

fn reject<T>(verdict: &'static str, detail: impl Into<String>) -> Result<T, Rejection> {
    Err(Rejection {
        verdict,
        detail: detail.into(),
    })
}

/// RFC 3339 (`YYYY-MM-DDTHH:MM:SS[.frac](Z|±HH:MM)`) → unix milliseconds,
/// floored, as JS `Date.parse` sees it, so a sub-millisecond digit never
/// flips a bound JS calls equal. Stricter than `Date.parse` (which also
/// takes `2026-11-10`): see spec 002 Edge Cases. Parsed by hand because the
/// `time` crate's `parsing` feature would change Cargo.lock.
fn unix_ms(s: &str) -> Option<i128> {
    let b = s.as_bytes();
    let num = |r: std::ops::Range<usize>| -> Option<u32> {
        let d = b.get(r)?;
        d.iter()
            .all(u8::is_ascii_digit)
            .then(|| d.iter().fold(0, |n, c| n * 10 + u32::from(c - b'0')))
    };
    let at = |i: usize, set: &[u8]| b.get(i).is_some_and(|c| set.contains(c));
    if !(at(4, b"-") && at(7, b"-") && at(10, b"Tt ") && at(13, b":") && at(16, b":")) {
        return None;
    }
    let month = Month::try_from(u8::try_from(num(5..7)?).ok()?).ok()?;
    let date =
        Date::from_calendar_date(i32::try_from(num(0..4)?).ok()?, month, num(8..10)? as u8).ok()?;
    let time = Time::from_hms(num(11..13)? as u8, num(14..16)? as u8, num(17..19)? as u8).ok()?;
    // Optional fraction: any number of digits, only the first three count.
    let mut i = 19;
    let mut ms = 0;
    if at(i, b".") {
        let digits = b[i + 1..].iter().take_while(|c| c.is_ascii_digit()).count();
        if digits == 0 {
            return None;
        }
        let first3 = format!("{:0<3}", &s[i + 1..i + 1 + digits.min(3)]);
        ms = i128::from(first3.parse::<u16>().ok()?);
        i += 1 + digits;
    }
    let offset = match b.get(i..)? {
        b"Z" | b"z" => UtcOffset::UTC,
        [sign @ (b'+' | b'-'), _, _, b':', _, _] => {
            let (h, m) = (num(i + 1..i + 3)? as i8, num(i + 4..i + 6)? as i8);
            let k = if *sign == b'-' { -1 } else { 1 };
            UtcOffset::from_hms(k * h, k * m, 0).ok()?
        }
        _ => return None,
    };
    let t = PrimitiveDateTime::new(date, time).assume_offset(offset);
    Some(i128::from(t.unix_timestamp()) * 1000 + ms)
}

/// The trust decision receipt-verify's `verifyReceipt` makes
/// (index.ts:208-378, keys.ts:108-129, revocation.ts:86-103): the spec 002 verdict table top to
/// bottom, returning the first failing row. Takes text so duplicate keys are
/// still visible. The signature is checked against the BUNDLE's key, never
/// the one the receipt embeds.
pub fn verify_pinned(text: &str, bundle: &TrustBundle) -> Result<Trusted, Rejection> {
    // Rows 0a-0d.
    let receipt = parse_strict(text)?;
    if !receipt.is_object() {
        return reject("malformed", "receipt must be a JSON object");
    }
    let str_at = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_owned);
    let sig = receipt.get("signature").cloned().unwrap_or(Value::Null);

    // Row 1 (index.ts:230).
    let version = receipt.get("receipt_version");
    if version.and_then(Value::as_str) != Some(RECEIPT_VERSION) {
        return reject(
            "unsupported_version",
            format!(
                "expected {RECEIPT_VERSION}, got {}",
                version.unwrap_or(&Value::Null)
            ),
        );
    }

    // Row 2 (index.ts:238). A missing payload or payload_hash is a mismatch.
    let computed = match receipt.get("payload").map(jcs_bytes) {
        Some(Ok(bytes)) => sha256_0x(&bytes),
        _ => {
            return reject(
                "payload_hash_mismatch",
                "payload missing or not canonicalizable",
            )
        }
    };
    if Some(&computed) != str_at(&receipt, "payload_hash").as_ref() {
        return reject("payload_hash_mismatch", format!("computed {computed}"));
    }

    // Rows 3-4 (index.ts:253-266): exact, case-sensitive key_id match.
    let key_id = match str_at(&sig, "key_id") {
        Some(k) if !k.is_empty() => k,
        _ => {
            return reject(
                "missing_key_id",
                "signature.key_id: required non-empty string",
            )
        }
    };
    let Some(key) = bundle.keys.keys.iter().find(|k| k.key_id == key_id) else {
        return reject(
            "unknown_key",
            format!("signature.key_id {key_id} not present in key bundle"),
        );
    };

    // Row 5 (index.ts:274): checked whenever the key resolves, windowed or not.
    let timestamp = str_at(&receipt, "timestamp").unwrap_or_default();
    let Some(ts) = unix_ms(&timestamp) else {
        return reject(
            "bad_timestamp",
            format!("not a valid RFC 3339 date: {timestamp:?}"),
        );
    };

    // Rows 6-7 (index.ts:276). Rust-only: an absent public_key is refused
    // rather than skipped (JS 2.0.0 skips the comparison, RV-6).
    match sig.get("public_key") {
        None => return reject("missing_public_key", "signature.public_key is absent"),
        Some(pk) if pk.as_str() != Some(key.public_key.as_str()) => {
            return reject(
                "key_mismatch",
                format!("key bundle public_key for {key_id} does not match the receipt's embedded public_key"),
            )
        }
        Some(_) => {}
    }

    // Rows 8-11: keyUsableAt (keys.ts:108-129), only reached when the
    // embedded key equals the bundle key (JS's `else` branch). Bounds are
    // inclusive; created_at is parsed and compared before rotation_after.
    let status = key.status.as_deref().unwrap_or("active");
    if status != "active" && status != "rotated" {
        return reject("key_inactive", format!("key {key_id} status is {status}"));
    }
    if let Some(from) = &key.created_at {
        let Some(from_ms) = unix_ms(from) else {
            return reject(
                "bad_bundle",
                format!("key {key_id} created_at is not a valid date"),
            );
        };
        if ts < from_ms {
            return reject(
                "key_not_yet_valid",
                format!("receipt timestamp precedes key {key_id} created_at {from}"),
            );
        }
    }
    if let Some(until) = &key.rotation_after {
        let Some(until_ms) = unix_ms(until) else {
            return reject(
                "bad_bundle",
                format!("key {key_id} rotation_after is not a valid date"),
            );
        };
        if ts > until_ms {
            return reject(
                "key_expired",
                format!("receipt timestamp is after key {key_id} rotation_after {until}"),
            );
        }
    }

    // Row 12 (index.ts:298).
    let algorithm = sig.get("algorithm");
    if algorithm.and_then(Value::as_str) != Some("Ed25519") {
        return reject(
            "unsupported_algorithm",
            format!(
                "unsupported signature algorithm: {}",
                algorithm.unwrap_or(&Value::Null)
            ),
        );
    }

    // Row 13 (index.ts:300-323), against the bundle key. A bundle key that
    // is not base64 is `bad_bundle` too (JS's atob throw lands in its
    // catch-all "signature verification failed"; naming the bundle is
    // clearer and refuses either way).
    let raw = match B64.decode(&key.public_key) {
        Ok(raw) if raw.len() == 32 => raw,
        Ok(raw) => {
            return reject(
                "bad_bundle",
                format!(
                    "key bundle public_key: expected 32 raw bytes, got {}",
                    raw.len()
                ),
            )
        }
        Err(e) => {
            return reject(
                "bad_bundle",
                format!("key bundle public_key is not base64: {e}"),
            )
        }
    };
    // Canonical spelling only (bytes.ts signatureBytes): the signature does
    // not cover its own value, so a lenient decode lets one signature travel
    // under many strings.
    let value = str_at(&sig, "value").unwrap_or_default();
    let sig_bytes = match B64.decode(&value) {
        Ok(b) if B64.encode(&b) == value => b,
        _ => {
            return reject(
                "bad_signature",
                "signature.value is not canonical standard base64",
            )
        }
    };
    let msg = signing_bytes(&receipt).or_else(|e| reject("bad_signature", e.to_string()))?;
    if verify_ed25519(&hex::encode(&raw), &msg, &sig_bytes).is_err() {
        return reject("bad_signature", "Ed25519 signature did not verify");
    }

    // Rows 14-15 (index.ts:331-344). Exact match against the four tier
    // names, so "constructor" or "root" is no tier; a key tier that is no
    // tier ranks -1 and every claim exceeds it.
    let rank = |s: Option<&str>| -> i32 {
        s.and_then(|s| STRENGTH_RANK.iter().position(|t| *t == s))
            .map_or(-1, |i| i as i32)
    };
    let claimed = receipt.get("attestation_strength");
    let claimed_rank = rank(claimed.and_then(Value::as_str));
    if claimed_rank < 0 {
        return reject(
            "unknown_strength",
            format!(
                "attestation_strength: unrecognized value {}",
                claimed.unwrap_or(&Value::Null)
            ),
        );
    }
    // JS skips the ceiling for a falsy key tier, so "" skips it too.
    if let Some(ceiling) = key
        .attestation_strength
        .as_deref()
        .filter(|t| !t.is_empty())
    {
        if claimed_rank > rank(Some(ceiling)) {
            return reject(
                "tier_too_high",
                format!(
                    "attestation_strength ceiling violated: receipt={} > key={ceiling}",
                    claimed.unwrap_or(&Value::Null)
                ),
            );
        }
    }

    // Row 16 (index.ts:354-371; first matching entry, as isKeyRevoked).
    // A compromised key always rejects: whoever holds it signs the timestamp
    // and can backdate it past revoked_at.
    let mut warnings = Vec::new();
    if let Some(k) = bundle
        .revocations
        .revoked_keys
        .iter()
        .find(|k| k.key_id == key_id)
    {
        let survives =
            k.reason != "compromise" && unix_ms(&k.revoked_at).is_some_and(|revoked| ts < revoked);
        if !survives {
            return reject(
                "key_revoked",
                format!(
                    "signing key {key_id} revoked at {}: {}",
                    k.revoked_at, k.reason
                ),
            );
        }
        warnings.push(format!(
            "signing key was rotated out of service at {} (receipt predates rotation); reason: {}",
            k.revoked_at, k.reason
        ));
    }

    // Row 17 (index.ts:372-377). Case-insensitive: receipt ids are UUIDs
    // and only the unsigned feed side can differ in case. JS skips this for
    // a missing id; row 18 then refuses it anyway.
    let receipt_id = str_at(&receipt, "receipt_id").filter(|id| !id.is_empty());
    if let Some(id) = &receipt_id {
        if let Some(r) = bundle
            .revocations
            .revoked_receipts
            .iter()
            .find(|r| r.receipt_id.eq_ignore_ascii_case(id))
        {
            return reject(
                "receipt_revoked",
                format!("receipt {id} revoked at {}: {}", r.revoked_at, r.reason),
            );
        }
    }

    // Row 18 (last): `Trusted` names a receipt, so it must have an id.
    let Some(receipt_id) = receipt_id else {
        return reject(
            "missing_receipt_id",
            "receipt_id: required non-empty string",
        );
    };
    Ok(Trusted {
        receipt_id,
        key_id,
        public_key: key.public_key.clone(),
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verdict(text: &str) -> &'static str {
        let r = parse_strict(text).expect_err(text);
        assert!(VERDICTS.contains(&r.verdict), "{} not in VERDICTS", r.verdict);
        assert_eq!(r.to_string(), format!("{}: {}", r.verdict, r.detail));
        r.verdict
    }

    #[test]
    fn parse_strict_refuses_duplicate_keys_at_any_depth() {
        for t in [
            r#"{"a":1,"a":2}"#,
            r#"{"x":{"a":1,"b":true,"a":1}}"#,
            r#"{"x":[1,{"a":null,"a":null}]}"#,
            r#"[[{"k":"v","k":"w"}]]"#,
        ] {
            assert_eq!(verdict(t), "duplicate_key", "{t}");
        }
    }

    #[test]
    fn parse_strict_refuses_non_integer_numbers() {
        for t in ["1.0", "0.0", "1e2", "-0.5", r#"{"a":[1.5]}"#] {
            assert_eq!(verdict(t), "non_integer_number", "{t}");
        }
    }

    #[test]
    fn parse_strict_refuses_unsafe_integers() {
        for t in [
            "9007199254740992",
            "-9007199254740992",
            "18446744073709551616",
            r#"{"a":[18446744073709551616]}"#,
        ] {
            assert_eq!(verdict(t), "unsafe_integer", "{t}");
        }
    }

    #[test]
    fn parse_strict_accepts_what_from_str_reads_the_same() {
        for t in [
            "9007199254740991",
            "-9007199254740991",
            "0",
            r#"[[1,[2,[]]],{"a":{"b":[null,true,false]}}]"#,
            r#"{"s":"héllo \"q\"","n":null,"t":true,"f":false,"e":{}}"#,
            " \"just a string\" ",
        ] {
            let want: Value = serde_json::from_str(t).unwrap();
            assert_eq!(parse_strict(t).unwrap(), want, "{t}");
        }
    }

    #[test]
    fn parse_strict_not_json_is_malformed() {
        for t in ["", "not json", "{", r#"{"a":1} x"#, "[1,]", "{\"a\" 1}"] {
            assert_eq!(verdict(t), "malformed", "{t:?}");
        }
    }

    // ---- verify_pinned (spec 002 rows 0a-13) ----

    use super::super::{sign, signing_bytes, Options, Source};
    use crate::receipt::{jcs_bytes, sha256_hex};
    use crate::signing::RuneSigner;
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine;
    use serde_json::json;

    /// A tempdir signer (the private key dies with the tempdir) and a fresh
    /// receipt it signed under key_id "k1".
    fn fixture() -> (tempfile::TempDir, RuneSigner, Value) {
        let d = tempfile::tempdir().unwrap();
        let s = RuneSigner::load_or_generate(&d.path().join("node.key")).unwrap();
        let source = Source {
            lens: "rune".into(),
            endpoint: "/boot".into(),
            node_id: "scry-rune-001".into(),
        };
        let opts = Options {
            key_id: Some("k1".into()),
            ..Options::default()
        };
        let r = sign(json!({"ok": true, "n": [1, -2]}), &s, source, opts).unwrap();
        (d, s, r)
    }

    /// A deserialized bundle holding `s`'s key as "k1", with `extra` fields
    /// merged into the key entry, and an empty revocation feed.
    fn bundle(s: &RuneSigner, extra: Value) -> TrustBundle {
        let mut key = json!({
            "key_id": "k1",
            "algorithm": "Ed25519",
            "public_key": B64.encode(hex::decode(s.pubkey_hex()).unwrap()),
        });
        for (k, v) in extra.as_object().unwrap() {
            key[k] = v.clone();
        }
        serde_json::from_value(json!({"keys": {"keys": [key]}, "revocations": {}})).unwrap()
    }

    /// Re-sign an edited receipt so the edit is its only fault.
    fn resign(s: &RuneSigner, r: &mut Value) {
        r["payload_hash"] = format!("0x{}", sha256_hex(&jcs_bytes(&r["payload"]).unwrap())).into();
        r["signature"]["value"] = B64.encode(s.sign(&signing_bytes(r).unwrap())).into();
    }

    fn refused(text: &str, b: &TrustBundle) -> &'static str {
        let r = verify_pinned(text, b).expect_err(text);
        assert!(
            VERDICTS.contains(&r.verdict),
            "{} not in VERDICTS",
            r.verdict
        );
        r.verdict
    }

    fn refused_v(r: &Value, b: &TrustBundle) -> &'static str {
        refused(&r.to_string(), b)
    }

    #[test]
    fn pinned_valid_under_pin_and_deserialized_bundle() {
        let (_d, s, r) = fixture();
        let embedded = r["signature"]["public_key"].as_str().unwrap();
        let bundle_json = format!(
            r#"{{"keys": {{"service": "x", "keys": [{{"key_id": "k1", "public_key": "{embedded}",
                 "status": "active", "operator": "ignored"}}]}},
                "revocations": {{"feed_version": 1, "updated_at": "2026-01-01T00:00:00Z"}},
                "unknown_top_level": true}}"#
        );
        let from_json: TrustBundle = serde_json::from_str(&bundle_json).unwrap();
        for b in [TrustBundle::pin(s.pubkey_hex(), "k1"), from_json] {
            let t = verify_pinned(&r.to_string(), &b).unwrap();
            assert_eq!(t.receipt_id, r["receipt_id"].as_str().unwrap());
            assert_eq!(t.key_id, "k1");
            assert_eq!(t.public_key, embedded);
            assert!(t.warnings.is_empty());
        }
    }

    #[test]
    fn pinned_bundle_without_revocations_does_not_deserialize() {
        let e = serde_json::from_str::<TrustBundle>(r#"{"keys": {"keys": []}}"#);
        assert!(e.is_err());
    }

    #[test]
    fn pinned_malformed() {
        let (_d, s, _r) = fixture();
        let b = bundle(&s, json!({}));
        for t in ["not json", "[]", "null", "\"s\""] {
            assert_eq!(refused(t, &b), "malformed", "{t}");
        }
    }

    #[test]
    fn pinned_duplicate_key() {
        let (_d, s, r) = fixture();
        let t = r
            .to_string()
            .replacen('{', r#"{"receipt_version":"1.0","#, 1);
        assert_eq!(refused(&t, &bundle(&s, json!({}))), "duplicate_key");
    }

    #[test]
    fn pinned_non_integer_number() {
        let (_d, s, mut r) = fixture();
        r["payload"] = json!({"x": 1.5});
        resign(&s, &mut r);
        assert_eq!(refused_v(&r, &bundle(&s, json!({}))), "non_integer_number");
    }

    #[test]
    fn pinned_unsafe_integer() {
        let (_d, s, mut r) = fixture();
        r["payload"] = json!({"x": 9007199254740992u64});
        resign(&s, &mut r);
        assert_eq!(refused_v(&r, &bundle(&s, json!({}))), "unsafe_integer");
    }

    #[test]
    fn pinned_unsupported_version() {
        let (_d, s, mut r) = fixture();
        r["receipt_version"] = "2.0".into();
        resign(&s, &mut r);
        assert_eq!(refused_v(&r, &bundle(&s, json!({}))), "unsupported_version");
    }

    #[test]
    fn pinned_payload_hash_mismatch() {
        let (_d, s, mut r) = fixture();
        r["payload"]["ok"] = false.into();
        assert_eq!(
            refused_v(&r, &bundle(&s, json!({}))),
            "payload_hash_mismatch"
        );
    }

    #[test]
    fn pinned_missing_key_id() {
        let (_d, s, mut r) = fixture();
        for bad in [json!(""), json!(null)] {
            r["signature"]["key_id"] = bad;
            resign(&s, &mut r);
            assert_eq!(refused_v(&r, &bundle(&s, json!({}))), "missing_key_id");
        }
    }

    #[test]
    fn pinned_unknown_key() {
        let (_d, s, r) = fixture();
        let b = TrustBundle::pin(s.pubkey_hex(), "someone-else");
        assert_eq!(refused_v(&r, &b), "unknown_key");
    }

    #[test]
    fn pinned_bad_timestamp_windowed_and_unwindowed() {
        let (_d, s, mut r) = fixture();
        r["timestamp"] = "not a date".into();
        resign(&s, &mut r);
        assert_eq!(refused_v(&r, &bundle(&s, json!({}))), "bad_timestamp");
        let windowed = bundle(&s, json!({"created_at": "2000-01-01T00:00:00Z"}));
        assert_eq!(refused_v(&r, &windowed), "bad_timestamp");
    }

    #[test]
    fn pinned_missing_public_key() {
        let (_d, s, mut r) = fixture();
        r["signature"].as_object_mut().unwrap().remove("public_key");
        resign(&s, &mut r);
        assert_eq!(refused_v(&r, &bundle(&s, json!({}))), "missing_public_key");
    }

    #[test]
    fn pinned_key_mismatch() {
        let (_d, _s, r) = fixture();
        let (_d2, other, _) = fixture();
        assert_eq!(
            refused_v(&r, &TrustBundle::pin(other.pubkey_hex(), "k1")),
            "key_mismatch"
        );
    }

    #[test]
    fn pinned_key_inactive() {
        let (_d, s, r) = fixture();
        let b = bundle(&s, json!({"status": "revoked"}));
        assert_eq!(refused_v(&r, &b), "key_inactive");
    }

    #[test]
    fn pinned_bad_bundle() {
        let (_d, s, r) = fixture();
        for extra in [
            json!({"created_at": "yesterday"}),
            json!({"rotation_after": "2999-13-01T00:00:00Z"}),
        ] {
            assert_eq!(
                refused_v(&r, &bundle(&s, extra.clone())),
                "bad_bundle",
                "{extra}"
            );
        }
        // A 31-byte bundle key, with the receipt embedding the same string so
        // the key_mismatch row passes and the signature step sees it.
        let (_d, s, mut r) = fixture();
        let short = B64.encode([7u8; 31]);
        r["signature"]["public_key"] = short.clone().into();
        resign(&s, &mut r);
        let b = bundle(&s, json!({"public_key": short}));
        assert_eq!(refused_v(&r, &b), "bad_bundle");
    }

    #[test]
    fn pinned_key_not_yet_valid() {
        let (_d, s, r) = fixture();
        let b = bundle(&s, json!({"created_at": "2999-01-01T00:00:00Z"}));
        assert_eq!(refused_v(&r, &b), "key_not_yet_valid");
    }

    #[test]
    fn pinned_key_expired() {
        let (_d, s, r) = fixture();
        let b = bundle(&s, json!({"rotation_after": "2000-01-01T00:00:00Z"}));
        assert_eq!(refused_v(&r, &b), "key_expired");
    }

    #[test]
    fn pinned_unsupported_algorithm() {
        let (_d, s, mut r) = fixture();
        r["signature"]["algorithm"] = "RSA".into();
        resign(&s, &mut r);
        assert_eq!(
            refused_v(&r, &bundle(&s, json!({}))),
            "unsupported_algorithm"
        );
    }

    #[test]
    fn pinned_bad_signature_flipped_and_non_canonical() {
        let (_d, s, r) = fixture();
        let b = bundle(&s, json!({}));
        let value = r["signature"]["value"].as_str().unwrap().to_string();
        let mut sig = B64.decode(&value).unwrap();
        sig[0] ^= 1;
        let mut flipped = r.clone();
        flipped["signature"]["value"] = B64.encode(&sig).into();
        assert_eq!(refused_v(&flipped, &b), "bad_signature");
        // 64 bytes encode to 86 chars + "==": the last char carries 2 unused
        // bits, so bumping it spells the same bytes non-canonically.
        let mut chars: Vec<char> = value.chars().collect();
        let last = chars.len() - 3;
        let alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let i = alphabet.find(chars[last]).unwrap();
        chars[last] = alphabet.chars().nth(i ^ 1).unwrap();
        let mut non_canonical = r.clone();
        non_canonical["signature"]["value"] = chars.into_iter().collect::<String>().into();
        assert_eq!(refused_v(&non_canonical, &b), "bad_signature");
        let mut unpadded = r.clone();
        unpadded["signature"]["value"] = value.trim_end_matches('=').into();
        assert_eq!(refused_v(&unpadded, &b), "bad_signature");
    }

    #[test]
    fn pinned_rfc3339_to_unix_ms() {
        for (t, want) in [
            ("1970-01-01T00:00:00Z", Some(0)),
            ("1970-01-01T00:00:00.9999Z", Some(999)),
            ("1970-01-01t00:00:01.5z", Some(1500)),
            ("1970-01-01T01:00:00+01:00", Some(0)),
            ("1969-12-31T23:00:00.001-01:00", Some(1)),
            ("2026-11-10T12:34:56.789Z", Some(1_794_314_096_789)), // node Date.parse
            ("2026-11-10", None),
            ("2026-02-30T00:00:00Z", None),
            ("2026-11-10T24:00:00Z", None),
            ("2026-11-10T00:00:00", None),
            ("2026-11-10T00:00:00.Z", None),
            ("2026-11-10T00:00:00+0100", None),
            ("2026-11-10T00:00:00Zx", None),
            ("+026-11-10T00:00:00Z", None),
        ] {
            assert_eq!(unix_ms(t), want, "{t}");
        }
    }

    #[test]
    fn pinned_window_bounds_are_inclusive() {
        let (_d, s, r) = fixture();
        let ts = r["timestamp"].clone();
        for extra in [json!({"created_at": ts}), json!({"rotation_after": ts})] {
            let got = verify_pinned(&r.to_string(), &bundle(&s, extra.clone()));
            assert!(got.is_ok(), "{extra}: {got:?}");
        }
    }

    #[test]
    fn pinned_rotated_key_in_window_passes() {
        let (_d, s, r) = fixture();
        let b = bundle(
            &s,
            json!({"status": "rotated", "created_at": "2000-01-01T00:00:00Z",
                   "rotation_after": "2999-01-01T00:00:00Z"}),
        );
        verify_pinned(&r.to_string(), &b).unwrap();
    }

    #[test]
    fn pinned_order_unknown_key_before_bad_timestamp() {
        let (_d, s, mut r) = fixture();
        r["timestamp"] = "not a date".into();
        resign(&s, &mut r);
        assert_eq!(
            refused_v(&r, &TrustBundle::pin(s.pubkey_hex(), "nope")),
            "unknown_key"
        );
    }

    #[test]
    fn pinned_order_bad_timestamp_before_key_expired() {
        let (_d, s, mut r) = fixture();
        r["timestamp"] = "not a date".into();
        resign(&s, &mut r);
        let b = bundle(&s, json!({"rotation_after": "2000-01-01T00:00:00Z"}));
        assert_eq!(refused_v(&r, &b), "bad_timestamp");
    }

    // ---- verify_pinned (spec 002 rows 14-18) ----

    /// `bundle` plus one revoked-key entry for "k1".
    fn revoked(s: &RuneSigner, extra: Value, revoked_at: &str, reason: &str) -> TrustBundle {
        let mut b = bundle(s, extra);
        b.revocations.revoked_keys.push(RevokedKey {
            key_id: "k1".into(),
            revoked_at: revoked_at.into(),
            reason: reason.into(),
        });
        b
    }

    /// The fixture re-signed claiming `tier`.
    fn claiming(tier: &str) -> (tempfile::TempDir, RuneSigner, Value) {
        let (d, s, mut r) = fixture();
        r["attestation_strength"] = tier.into();
        resign(&s, &mut r);
        (d, s, r)
    }

    #[test]
    fn pinned_unknown_strength_missing_root_constructor() {
        let (_d, s, mut r) = fixture();
        r.as_object_mut().unwrap().remove("attestation_strength");
        resign(&s, &mut r);
        assert_eq!(refused_v(&r, &bundle(&s, json!({}))), "unknown_strength");
        for tier in ["root", "constructor"] {
            let (_d, s, r) = claiming(tier);
            assert_eq!(refused_v(&r, &bundle(&s, json!({}))), "unknown_strength");
        }
    }

    #[test]
    fn pinned_tier_too_high() {
        let (_d, s, r) = claiming("tee-tpm");
        let b = bundle(&s, json!({"attestation_strength": "software"}));
        assert_eq!(refused_v(&r, &b), "tier_too_high");
        // A key tier that is no tier ranks -1: every claim exceeds it.
        let (_d, s, r) = claiming("self-asserted");
        let b = bundle(&s, json!({"attestation_strength": "constructor"}));
        assert_eq!(refused_v(&r, &b), "tier_too_high");
    }

    #[test]
    fn pinned_tier_ceiling_skipped_without_key_strength() {
        let (_d, s, r) = claiming("silicon-root");
        verify_pinned(&r.to_string(), &bundle(&s, json!({}))).unwrap();
    }

    #[test]
    fn pinned_key_revoked() {
        // Compromise rejects even though the receipt predates revoked_at.
        let (_d, s, r) = fixture();
        let b = revoked(&s, json!({}), "2999-01-01T00:00:00Z", "compromise");
        assert_eq!(refused_v(&r, &b), "key_revoked");
        // Any reason rejects once revoked_at precedes the receipt.
        let b = revoked(&s, json!({}), "2000-01-01T00:00:00Z", "rotation");
        assert_eq!(refused_v(&r, &b), "key_revoked");
    }

    #[test]
    fn pinned_rotation_after_receipt_passes_with_one_warning() {
        let (_d, s, r) = fixture();
        let b = revoked(&s, json!({}), "2999-01-01T00:00:00Z", "rotation");
        let t = verify_pinned(&r.to_string(), &b).unwrap();
        assert_eq!(t.warnings.len(), 1, "{:?}", t.warnings);
    }

    #[test]
    fn pinned_receipt_revoked_case_insensitive() {
        let (_d, s, r) = fixture();
        let mut b = bundle(&s, json!({}));
        b.revocations.revoked_receipts.push(RevokedReceipt {
            receipt_id: r["receipt_id"].as_str().unwrap().to_uppercase(),
            revoked_at: "2000-01-01T00:00:00Z".into(),
            reason: "retracted".into(),
        });
        assert_eq!(refused_v(&r, &b), "receipt_revoked");
    }

    #[test]
    fn pinned_missing_receipt_id() {
        let (_d, s, mut r) = fixture();
        r.as_object_mut().unwrap().remove("receipt_id");
        resign(&s, &mut r);
        assert_eq!(refused_v(&r, &bundle(&s, json!({}))), "missing_receipt_id");
    }

    #[test]
    fn pinned_order_key_expired_before_tier_too_high() {
        let (_d, s, r) = claiming("tee-tpm");
        let b = bundle(
            &s,
            json!({"attestation_strength": "software", "rotation_after": "2000-01-01T00:00:00Z"}),
        );
        assert_eq!(refused_v(&r, &b), "key_expired");
    }

    #[test]
    fn pinned_order_tier_too_high_before_key_revoked() {
        let (_d, s, r) = claiming("tee-tpm");
        let b = revoked(
            &s,
            json!({"attestation_strength": "software"}),
            "2000-01-01T00:00:00Z",
            "compromise",
        );
        assert_eq!(refused_v(&r, &b), "tier_too_high");
    }
}
