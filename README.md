# rune-record

Reference Rust implementation of TunnelMind's signed records: ADR-001 attestation receipts, Receipt v1 envelopes and
ADR-002 Rune node manifests. It is the one canonicaliser (RFC 8785 JCS) and Ed25519 signer that TunnelMind's Rust tools
share, so a receipt made by any of them verifies the same way everywhere.

- `receipt_v1` builds and verifies Receipt v1 envelopes; `receipt_v1::conformance` replays the published test vectors.
- `signing` wraps Ed25519 (ring) keys; `receipt` holds ADR-001 receipts; `manifest` validates ADR-002 manifests.
- JavaScript verifier for the same formats: [receipt-verify](https://github.com/TunnelMind/receipt-verify).

Split out of TunnelMind's private Rune repository (history kept) so anyone can build and check the receipt code.
The Rune OS and its mesh stay closed; the record format is public, like the data it attests.

```
cargo test
```

Licensed under the Apache License, Version 2.0 ([LICENSE](LICENSE)).
