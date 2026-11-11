# Changelog

## v0.2.0 — 2026-11-10

**This repo is now a generated mirror.** The crate is generated from `crates/rune-record` in the
private `rune` repo at commit `0018299` (full sha in [RUNE_SYNC](RUNE_SYNC)) by
`scripts/sync-from-rune.sh`. This reverses v0.1.0's "this repo is the canonical Rune receipt
crate": change the code in rune, then sync. See [docs/MIRROR.md](docs/MIRROR.md).

- Adds `receipt_v1::verify_pinned` and `receipt_v1::TrustBundle`: verification that checks the
  signer against a pinned key bundle, not just the key the receipt names.
- Adds the 10 shared Receipt v1 vectors in `vectors/receipt-v1/`.
- Adds `signing::write_pub_if_changed` (private): the `.pub` file is rewritten only when its
  bytes differ, via a temp file and rename.
- Public-only tests are kept: the base64 respelling tests live in `tests/receipt_v1_base64.rs`,
  outside the synced tree.

## v0.1.0 — 2026-11-04

**Version stays 0.1.0.** [docs/DIVERGENCE.md](docs/DIVERGENCE.md) concludes that no row changes
signed bytes: A (this repo), B (`crates/rune-record` in the private `rune` repo at `91652a7`) and
C (the rev tm-weather pins, `6302282`) sign identical bytes for the same input and key. The only
verification-outcome difference is the legacy `receipt::verify` (ADR-001): A and B refuse a
non-`ed25519` `alg` and return `Err` for a pubkey with no char boundary at byte 16; C accepts that
`alg` and panics on that pubkey. Receipt v1 has no difference in signed bytes or verification
outcome across A, B and C.

- This repo is the canonical Rune receipt crate (spec 125, D4). Consumers repin to a tagged
  release of it in follow-up runs.
- B's tests that A lacked are ported verbatim: the random-value JCS tests in `receipt`
  (with their `Rng`/`random_value` helpers), and the extra `receipt_v1` and `manifest` tests.
  Measured: `cargo test` runs 40 tests, `cargo test --no-default-features` runs 32.
- B's `//!` module headers and `///` item docs are ported.
- `TM_REQUIRE_JS_INTEROP=1` turns the JS interop skip in `receipt_v1::conformance` into a
  failure. The default (skip with a notice) is unchanged.
- `scripts/reverify-weather.sh` added: mirrors the published weather log and re-verifies it
  under this tree and under C.
- Re-verify of the published weather log (234 receipts) under A vs C: **PASS**, identical
  results. Both exit 1 with the same 27 FAIL lines, which are about the log's data, not
  rune-record. See [docs/REVERIFY.md](docs/REVERIFY.md).
