//! rune-record — ADR-001 receipts + ADR-002 manifests for Rune nodes.
//! Software tier today (file-held Ed25519, Familiar-compatible key scheme);
//! ponytail: tee-tpm signing lands as a second RuneSigner backend when the
//! TPM HATs arrive — the receipt/verify path does not change.

pub mod manifest;
pub mod receipt;
pub mod receipt_v1;
pub mod signing;
