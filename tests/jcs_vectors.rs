//! The shared RFC 8785 wire vectors (receipt-verify test/vectors/jcs.json,
//! vendored byte-identical). Every TunnelMind canonicalizer must agree with
//! them; this pins ours.

use rune_record::receipt::{jcs_bytes, sha256_hex};
use serde::Deserialize;
use serde_json::value::RawValue;
use serde_json::Value;

const VECTORS: &[u8] = include_bytes!("vectors/jcs.json");
/// Full sha256 of the vendored file; a drifted copy fails here first.
const VECTORS_SHA256: &str = "81a79410874c5ed24ea0468a3d684e3a03111dc0a2fd2e8317ed3e3dfb978cdd";

#[derive(Deserialize)]
struct File<'a> {
    #[serde(borrow)]
    vectors: Vec<&'a RawValue>,
}

#[derive(Deserialize)]
struct Vector<'a> {
    name: String,
    // Kept raw: a lone-surrogate input is not representable as a Value.
    #[serde(borrow)]
    input: &'a RawValue,
    canonical: Option<String>,
    #[serde(default)]
    throws: bool,
}

#[test]
fn vendored_file_is_byte_identical() {
    assert_eq!(sha256_hex(VECTORS), VECTORS_SHA256);
}

#[test]
fn jcs_matches_every_vector() {
    let file: File = serde_json::from_slice(VECTORS).expect("vector file envelope parses");
    let (mut matched, mut refused) = (0, 0);
    for raw in file.vectors {
        let v: Vector = serde_json::from_str(raw.get()).expect("vector object parses");
        // serde_json refusing a lone surrogate at parse time counts as a refusal.
        let out = serde_json::from_str::<Value>(v.input.get())
            .map_err(anyhow::Error::from)
            .and_then(|input| jcs_bytes(&input));
        if v.throws {
            assert!(out.is_err(), "{}: lone surrogate must be refused, got {:?}", v.name, out);
            refused += 1;
        } else {
            let canonical = v.canonical.unwrap_or_else(|| panic!("{}: no canonical", v.name));
            let got = out.unwrap_or_else(|e| panic!("{}: {e:#}", v.name));
            // Byte-for-byte: no normalisation on either side.
            assert_eq!(got, canonical.as_bytes(), "{}", v.name);
            matched += 1;
        }
    }
    // A silently skipped vector fails here.
    assert_eq!((matched, refused), (6, 3));
}
