# rune-record

Reference Rust implementation of TunnelMind's signed records: ADR-001 attestation receipts, Receipt v1 envelopes and
ADR-002 Rune node manifests. It is the one canonicaliser (RFC 8785 JCS) and Ed25519 signer that TunnelMind's Rust tools
share, so a receipt made by any of them verifies the same way everywhere.

- `receipt_v1` builds and verifies Receipt v1 envelopes; `receipt_v1::conformance` replays the published test vectors.
- `signing` wraps Ed25519 (ring) keys; `receipt` holds ADR-001 receipts; `manifest` validates ADR-002 manifests.
- JavaScript verifier for the same formats: [receipt-verify](https://github.com/TunnelMind/receipt-verify).

Split out of TunnelMind's private Rune repository (history kept) so anyone can build and check the receipt code.
The Rune OS and its mesh stay closed; the record format is public, like the data it attests.

## Receipts

rune-record is the estate's one Rust implementation of three things:

- **JCS canonicaliser** (RFC 8785): `receipt::jcs_bytes`, backed by `serde_json_canonicalizer`.
- **Receipt v1.0 signer/verifier**: `receipt_v1::sign` and `receipt_v1::verify`.
- **Node-key loader**: `signing::RuneSigner::load` never generates a key (a missing file is an error);
  `RuneSigner::load_or_generate` is the explicit generator.

Consumers: tm-crawl, wgd and xcheck-rune today; tm and familiar after spec 026. Each pins a git rev, never a floating
branch. tm and familiar take `default-features = false`, which drops the `manifest` feature (ADR-002 validation, the
only user of toml and its tree) and keeps receipts:

```toml
rune-record = { git = "https://github.com/TunnelMind/rune-record", rev = "<sha>", default-features = false }
```

Test vectors: `tests/vectors/jcs.json` is vendored from [receipt-verify](https://github.com/TunnelMind/receipt-verify),
the JavaScript verifier. Change it in receipt-verify first, then re-vendor everywhere. `tests/jcs_vectors.rs` pins the
file's sha256 (`81a79410…8cdd`), so a drifted copy fails before any vector runs.

Security: `receipt_v1::verify` proves integrity only (the receipt is well-formed and signed by the key it names). The
caller must resolve the returned `key_id`/`public_key` against a pinned key bundle; a valid signature alone does not
say the signer is trusted.

```
cargo test
```

Licensed under the Apache License, Version 2.0 ([LICENSE](LICENSE)).
