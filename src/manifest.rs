//! ADR-002 agent manifest: TOML parse + the six validation rules.
//! Build- and wake-fatal errors come back as Err; forward-compat unknown
//! sections come back as warnings.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;

use crate::receipt::{sha256_hex, tiers_for_prefix, TIERS};

pub const SECTIONS: [&str; 8] = [
    "identity",
    "job",
    "requirements",
    "modes",
    "escalation",
    "migration",
    "mesh",
    "receipt",
];

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub identity: Identity,
    pub job: Job,
    pub requirements: Requirements,
    pub modes: BTreeMap<String, String>,
    pub escalation: Escalation,
    pub migration: Migration,
    pub mesh: Mesh,
    pub receipt: ReceiptCfg,
}

#[derive(Debug, Deserialize)]
pub struct Identity {
    pub name: String,
    pub purpose: String,
    pub version: String,
    pub manifest_hash: String,
    pub pubkey: String,
}

#[derive(Debug, Deserialize)]
pub struct Job {
    pub description: String,
    pub owns: Vec<String>,
    pub reads: Vec<String>,
    pub calls: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Requirements {
    pub min_ram_gb: u32,
    pub preferred_ram_gb: u32,
    pub min_storage_gb: u32,
    pub compute_tier: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub model_hash: String,
}

#[derive(Debug, Deserialize)]
pub struct Escalation {
    #[serde(default)]
    pub local_model: String,
    pub escalate_model: String,
    pub escalate_when: Vec<String>,
    pub escalation_receipt: bool,
}

#[derive(Debug, Deserialize)]
pub struct Migration {
    pub state_format: String,
    pub serializes: Vec<String>,
    pub migration_time_s: u32,
    pub resumable: bool,
    pub broadcast_on_migrate: bool,
}

#[derive(Debug, Deserialize)]
pub struct Mesh {
    pub advertises_to: String,
    pub fallback_for_absent_node: String,
    pub heartbeat_interval_s: u32,
    pub rejects_unattested: bool,
}

#[derive(Debug, Deserialize)]
pub struct ReceiptCfg {
    pub attestation_tier: String,
    pub signs_with: String,
    pub log_to: Vec<String>,
}

/// ADR-002 hash rule: SHA-256 over the literal file bytes with the
/// `manifest_hash` value emptied. Line-anchored, exactly like the sed check:
///   sed 's/^manifest_hash = .*/manifest_hash = ""/' rune.toml | sha256sum
pub fn manifest_hash(file: &str) -> String {
    let mut out = String::with_capacity(file.len());
    for line in file.split_inclusive('\n') {
        if line.starts_with("manifest_hash = ") {
            out.push_str("manifest_hash = \"\"");
            if line.ends_with('\n') {
                out.push('\n');
            }
        } else {
            out.push_str(line);
        }
    }
    sha256_hex(out.as_bytes())
}

/// Parse + validate a manifest file's contents.
/// Err = build/wake-fatal. Ok((manifest, warnings)) otherwise.
pub fn load(file: &str) -> Result<(Manifest, Vec<String>)> {
    // Rule 1 (warnings half): unknown top-level sections are forward-compat warnings.
    let raw: toml::Value = file.parse().context("manifest is not valid TOML")?;
    let mut warnings = Vec::new();
    if let Some(t) = raw.as_table() {
        for k in t.keys() {
            if !SECTIONS.contains(&k.as_str()) {
                warnings.push(format!("unknown section [{k}] ignored"));
            }
        }
    }

    // Rule 1 (errors half): missing sections/keys fail typed deserialization.
    let m: Manifest = toml::from_str(file).context("manifest missing required sections/keys")?;

    // Rule 2: the two non-negotiables. No override flag exists.
    if !m.escalation.escalation_receipt {
        bail!("escalation.escalation_receipt must be true (non-negotiable)");
    }
    if !m.mesh.rejects_unattested {
        bail!("mesh.rejects_unattested must be true (non-negotiable)");
    }

    // Rule 3: modes = full + offline + at least one degraded.
    if !m.modes.contains_key("full") || !m.modes.contains_key("offline") {
        bail!("[modes] must define 'full' and 'offline'");
    }
    if !m.modes.keys().any(|k| k != "full" && k != "offline") {
        bail!("[modes] must define at least one degraded mode");
    }

    // Rule 4: tier consistent with key-custody prefixes (ADR-001 rule).
    let tier = m.receipt.attestation_tier.as_str();
    if !TIERS.contains(&tier) {
        bail!("receipt.attestation_tier '{tier}' is not a valid tier");
    }
    for (field, val) in [
        ("identity.pubkey", &m.identity.pubkey),
        ("receipt.signs_with", &m.receipt.signs_with),
    ] {
        if !tiers_for_prefix(val).contains(&tier) {
            bail!("{field} prefix '{val}' cannot prove tier '{tier}'");
        }
    }

    // Rule 5: manifest_hash verifies against the file bytes ("" = not yet stamped, build fills it).
    let expect = manifest_hash(file);
    if !m.identity.manifest_hash.is_empty()
        && m.identity.manifest_hash != format!("sha256:{expect}")
    {
        bail!(
            "manifest_hash mismatch: file hashes to sha256:{expect}, manifest claims {}",
            m.identity.manifest_hash
        );
    }

    // Rule 6: model set ⇒ model_hash set.
    if !m.requirements.model.is_empty() && m.requirements.model_hash.is_empty() {
        bail!("requirements.model is set but model_hash is empty");
    }

    if m.requirements.min_ram_gb > m.requirements.preferred_ram_gb {
        warnings.push("min_ram_gb exceeds preferred_ram_gb".into());
    }
    Ok((m, warnings))
}

/// `runecraft build` half of rule 5: stamp the self-referential hash into the file text.
pub fn stamp_hash(file: &str) -> String {
    let h = manifest_hash(file);
    let mut out = String::with_capacity(file.len() + 80);
    for line in file.split_inclusive('\n') {
        if line.starts_with("manifest_hash = ") {
            out.push_str(&format!("manifest_hash = \"sha256:{h}\""));
            if line.ends_with('\n') {
                out.push('\n');
            }
        } else {
            out.push_str(line);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = include_str!("../tests/fixtures/scry-rune-001.toml");

    #[test]
    fn example_manifest_validates() {
        let (m, warnings) = load(EXAMPLE).unwrap();
        assert_eq!(m.identity.name, "scry-rune-001");
        assert_eq!(m.receipt.attestation_tier, "software");
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn stamped_hash_survives_reload() {
        let stamped = stamp_hash(EXAMPLE);
        let (m, _) = load(&stamped).unwrap();
        assert!(m.identity.manifest_hash.starts_with("sha256:"));
        // Any byte edit after stamping breaks the hash.
        let tampered = stamped.replace("heartbeat_interval_s = 60", "heartbeat_interval_s = 61");
        assert!(load(&tampered)
            .unwrap_err()
            .to_string()
            .contains("manifest_hash mismatch"));
    }

    #[test]
    fn non_negotiables_are_fatal() {
        let bad = EXAMPLE.replace("rejects_unattested = true", "rejects_unattested = false");
        assert!(load(&bad)
            .unwrap_err()
            .to_string()
            .contains("rejects_unattested"));
        let bad = EXAMPLE.replace("escalation_receipt = true", "escalation_receipt = false");
        assert!(load(&bad)
            .unwrap_err()
            .to_string()
            .contains("escalation_receipt"));
    }

    #[test]
    fn missing_degraded_mode_is_fatal() {
        let bad = EXAMPLE.replace(
            "degraded-spool = \"ingest unreachable — spool observations locally, forward on recovery\"\n",
            "",
        );
        assert!(load(&bad).unwrap_err().to_string().contains("degraded"));
    }

    #[test]
    fn software_key_cannot_claim_tee_tpm_tier() {
        let bad = EXAMPLE.replace(
            "attestation_tier = \"software\"",
            "attestation_tier = \"tee-tpm\"",
        );
        assert!(load(&bad)
            .unwrap_err()
            .to_string()
            .contains("cannot prove tier"));
    }

    #[test]
    fn unknown_section_warns_not_fails() {
        let extended = format!("{EXAMPLE}\n[future_section]\nx = 1\n");
        let (_, warnings) = load(&extended).unwrap();
        assert!(warnings.iter().any(|w| w.contains("future_section")));
    }
}
