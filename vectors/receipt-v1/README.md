# Receipt v1.0 trust vectors

Shared cross-language vectors for the pinned-key trust check (spec
`002-receipt-pin-1`). Rust's `verify_pinned` must return each file's
`expected`; `crates/rune-record/src/receipt_v1/vectors.rs`
(`receipt_v1_vectors_agree`) checks every `*.json` here on each `cargo test`
and fails if there are fewer than 10.

## Schema

Each file is one JSON object:

| Field | Type | Meaning |
|---|---|---|
| `name` | string | The file stem. |
| `description` | string | The one fault, and how it was made. |
| `receipt` | string | The receipt's raw TEXT (not an embedded object), so text-level faults such as a repeated key survive. |
| `trust` | object | `{"keys": <receipt-verify KeyBundle>, "revocations": <RevocationFeed>}`, Rust's `TrustBundle`. |
| `expected` | string | `valid`, or the first failing verdict name from the table below. |
| `js_receipt_verify_2_0_0` | string | `valid` / `invalid`: what `@tunnelmindai/receipt-verify` 2.0.0 says today. |

Every fault that touches a signed field was edited and then re-signed (with
`payload_hash` recomputed when the payload changed), so the named fault is
the receipt's only one. `duplicate_key` is a plain text edit of `valid`.

## Verdicts, in check order

`verify_pinned` returns the FIRST failing name in this order; `valid` when none fails.

| # | Verdict | # | Verdict |
|---|---|---|---|
| 0a | `malformed` | 9 | `bad_bundle` |
| 0b | `duplicate_key` | 10 | `key_not_yet_valid` |
| 0c | `non_integer_number` | 11 | `key_expired` |
| 0d | `unsafe_integer` | 12 | `unsupported_algorithm` |
| 1 | `unsupported_version` | 13 | `bad_signature` |
| 2 | `payload_hash_mismatch` | 14 | `unknown_strength` |
| 3 | `missing_key_id` | 15 | `tier_too_high` |
| 4 | `unknown_key` | 16 | `key_revoked` |
| 5 | `bad_timestamp` | 17 | `receipt_revoked` |
| 6 | `missing_public_key` | 18 | `missing_receipt_id` |
| 7 | `key_mismatch` | | |
| 8 | `key_inactive` | | |

Rows 0b, 0c, 0d, 6 and 18 are Rust-only checks that JS 2.0.0 does not make,
which is why `duplicate_key`, `float_payload` (`non_integer_number`),
`missing_public_key` and `missing_receipt_id` record `valid` for JS. The full
table with JS error texts is in `specs/002-receipt-pin-1/spec.md`.

| File | `expected` | JS 2.0.0 |
|---|---|---|
| `valid.json` | `valid` | `valid` |
| `unknown_key.json` | `unknown_key` | `invalid` |
| `expired_key.json` | `key_expired` | `invalid` |
| `revoked_key.json` | `key_revoked` | `invalid` |
| `tier_too_high.json` | `tier_too_high` | `invalid` |
| `missing_public_key.json` | `missing_public_key` | `valid` |
| `missing_receipt_id.json` | `missing_receipt_id` | `valid` |
| `float_payload.json` | `non_integer_number` | `valid` |
| `duplicate_key.json` | `duplicate_key` | `valid` |
| `bad_timestamp.json` | `bad_timestamp` | `invalid` |

## Regenerate

```sh
cargo test -p rune-record -- --ignored write_receipt_v1_vectors
```

The generator is `#[ignore]`, so a normal `cargo test` never rewrites these
files. It overwrites all ten with a new key and fresh timestamps and receipt
ids; commit the result.

## The key is throwaway

Each run generates a fresh Ed25519 key in a tempdir; the private half never
leaves that tempdir and is deleted with it. Only the public key (base64, in
`trust`) is committed. Nothing here is trusted anywhere.

## Where `js_receipt_verify_2_0_0` comes from

From a run: on 2026-11-10 the T005 coder ran `~/receipt-verify/dist/index.js`
(package version 2.0.0) `verifyReceipt(JSON.parse(receipt), {keys:
trust.keys, revocations: trust.revocations})` over all ten files; every
result matched the field (and matches reading `src/index.ts`).
