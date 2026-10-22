//! rune-record — ADR-001 receipts + ADR-002 manifests for Rune nodes.
//! Software tier today (file-held Ed25519, Familiar-compatible key scheme);
//! ponytail: tee-tpm signing lands as a second RuneSigner backend when the
//! TPM HATs arrive — the receipt/verify path does not change.
//!
//! The estate's one Rust RFC 8785 JCS canonicaliser (`receipt::jcs_bytes`),
//! Receipt v1.0 signer/verifier (`receipt_v1::sign`/`verify`) and node-key
//! loader (`signing::RuneSigner::load`, never generates). Consumers: tm-crawl,
//! wgd, xcheck-rune; tm and familiar after spec 026. `receipt_v1::verify`
//! proves integrity only: resolve the returned `key_id`/`public_key` against a
//! pinned key bundle before trusting the signer.

#[cfg(feature = "manifest")]
pub mod manifest;
pub mod receipt;
pub mod receipt_v1;
pub mod signing;
