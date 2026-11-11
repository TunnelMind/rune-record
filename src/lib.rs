//! rune-record — the trust primitives every other crate in this workspace
//! (and tm-crawl's crawl-record) builds on. No I/O beyond the key file.
//!
//! - `signing`: the Ed25519 node key (`RuneSigner`) and `verify_ed25519`;
//! - `receipt`: ADR-001 `attest:` records, `jcs_bytes` (the one RFC 8785
//!   canonicaliser) and `sha256_hex` (the one digest spelling);
//! - `receipt_v1`: TunnelMind Receipt v1.0, the estate-wide envelope;
//! - `manifest`: ADR-002 manifest parsing and its six validation rules.
//!
//! Software tier today (file-held key, Familiar-compatible scheme); a tee-tpm
//! backend would be a second `RuneSigner` constructor, the receipt and verify
//! paths unchanged. Everything that takes a receipt, manifest or key file as
//! input treats it as untrusted and returns `Err` rather than panicking.

#[cfg(feature = "manifest")]
pub mod manifest;
pub mod receipt;
pub mod receipt_v1;
pub mod signing;
