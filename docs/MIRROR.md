# MIRROR — this repo vs rune `crates/rune-record`

G001 of spec `001-rune-record-mirror-1`. Compares this repo `origin/main` `0959ac5` with rune
`0018299` (`00182997c6b8ec98ff990abcae7a123970bc26de`), paths `crates/rune-record/**` and
`vectors/receipt-v1/**`. Rune is the source; this repo becomes its generated copy
(`scripts/sync-from-rune.sh`, T003). Every row is **done** (T003 sync at `0018299`, recorded in `RUNE_SYNC`).
Supersedes `docs/DIVERGENCE.md` (older revs).

Measured 2026-11-10 with:

```sh
# path set (this command's every output line appears in the table below)
comm -3 <(git ls-tree -r --name-only origin/main | sort) \
  <(git -C /home/o2k/rune ls-tree -r --name-only 0018299 crates/rune-record vectors/receipt-v1 \
    | sed 's#^crates/rune-record/##' | sort)
# shared files: rune read by git archive into /tmp, never checked out
git -C /home/o2k/rune archive 0018299 crates/rune-record vectors/receipt-v1 | tar -x -C "$R"
git archive origin/main | tar -x -C "$P"
diff -u "$P/$f" "$R/crates/rune-record/$f"
diff <(grep -oE '\bfn [a-z_0-9]+' "$P/$f" | sort) <(grep -oE '\bfn [a-z_0-9]+' "$R/crates/rune-record/$f" | sort)
```

## Path and fn table

Disposition: take rune / public-only kept / patch hunk (in `scripts/rune-mirror.patch`) /
move to tests / dropped + reason. Changed-line counts are `diff | grep -c '^[<>]'`.

### Shared paths (both sides)

| path / fn | here | rune | which side is right | disposition |
|---|---|---|---|---|
| `src/lib.rs` header doc | consumer list ("tm-crawl, wgd, xcheck-rune; tm and familiar after spec 026") | module map, untrusted-input rule | rune (review §2: the consumer list is false) | take rune (done) |
| `src/lib.rs` `pub mod manifest` gate | `#[cfg(feature = "manifest")]` | no gate, `toml` not optional | here (wx-receipt-verify uses `default-features = false`) | patch hunk 1 (done) |
| `src/manifest.rs` header doc | "`runecraft build`…the escalation path" | "runecraft `build`…rune-escalate" | rune (wording only) | take rune (done) |
| `src/manifest.rs` `tests::EXAMPLE` | `include_str!("../tests/fixtures/scry-rune-001.toml")` | `include_str!("../../../manifests/scry-rune-001.toml")` (escapes this repo) | here for the path (file bytes identical: `git -C ~/rune show 0018299:manifests/scry-rune-001.toml \| diff - tests/fixtures/scry-rune-001.toml` = 0) | patch hunk 2 (done) — **not in plan/FR-004's hunk list; needed or the 8 manifest tests fail to build** |
| `src/receipt.rs` header doc | 2-line data-flow note | names callers (runecraft, rune-escalate, rune-weave) | rune (doc only, 8 lines) | take rune (done) |
| `src/receipt_v1.rs` docs on module, `Verified`, `check_numbers`, `sign`, `verify` | "trust is the caller's job" | "signature only, no trust decision; use `verify_pinned`" | rune (`verify_pinned` exists there) | take rune (done) |
| `src/receipt_v1.rs` `mod vectors` (`#[cfg(test)]`), `mod trust; pub use trust::*` | absent | present | rune | take rune (done) |
| `src/receipt_v1.rs` `tests::standard_engine_refuses_non_canonical_base64` | present (K002) | absent | here (no rune test covers it) | moved in T002 to `tests/receipt_v1_base64.rs` (done: the T003 sync leaves `tests/` alone) |
| `src/receipt_v1.rs` `tests::respellings` (helper) | present | absent | here | moved in T002 to `tests/receipt_v1_base64.rs` with its caller (done: the T003 sync leaves `tests/` alone) |
| `src/receipt_v1.rs` `tests::respelled_signature_fields_fail_verify` | present | absent | here | moved in T002 to `tests/receipt_v1_base64.rs` (done: the T003 sync leaves `tests/` alone) |
| `src/receipt_v1/conformance.rs` module doc | "Last synced 2026-10-19…Set TM_REQUIRE_JS_INTEROP=1…" | no `TM_REQUIRE_JS_INTEROP` sentence | here for the sentence, rune otherwise | patch hunk 4 (done; doc lines with `skip()`) |
| `src/receipt_v1/conformance.rs` `verify_with_node` | two `return skip(..)` | two inline `eprintln!` + `return None` | here (`TM_REQUIRE_JS_INTEROP=1` turns a skip into a panic) | patch hunk 4 (done) |
| `src/receipt_v1/conformance.rs` `skip` | present | absent | here | patch hunk 4 (done) |
| `src/signing.rs` docs (module, `RuneSigner`, `load`, `write_new_0600`) | older wording, "written…on every load" | rewrite-only-if-changed wording | rune | take rune (done) |
| `src/signing.rs` `load` body | `std::fs::write(&pub_path, ..)` | `write_pub_if_changed(&pub_path, ..)` | rune (atomic `.pub`) | take rune (done) |
| `src/signing.rs` `write_pub_if_changed` | absent | present | rune | take rune (done) |
| `src/signing.rs` `tests::new_key` (helper) | absent | present | rune | take rune (done) |
| `src/signing.rs` `tests::pub_untouched_when_unchanged` | absent | present | rune | take rune (done) |
| `src/signing.rs` `tests::pub_stale_is_replaced` | absent | present | rune | take rune (done) |
| `src/signing.rs` `tests::pub_reader_never_sees_partial` | absent | present | rune | take rune (done) |
| `tests/vectors/crawl_record_v1.json` | same bytes | same bytes (0 changed lines) | equal | take rune (done; mirrored so drift is caught) |
| `Cargo.toml` | see table below | see table below | here for metadata/licence/feature | not mirrored; public-only kept (version → 0.2.0 in T004) |

Shared fns not listed have identical bodies (checked: the non-comment `diff -u` hunks are only the
rows above). No fn exists only here except `respellings`, the two base64 tests and `skip`; none is dropped.

### Rune-only paths

| path / fn | here | rune | which side is right | disposition |
|---|---|---|---|---|
| `src/receipt_v1/trust.rs` (`TrustBundle`, `KeyBundle`, `KeyEntry`, `RevocationFeed`, `Rejection`, `parse_strict`, `verify_pinned`, 38 tests) | absent | present | rune | take rune (done) |
| `src/receipt_v1/vectors.rs` (`receipt_v1_vectors_agree`, `#[ignore] write_receipt_v1_vectors`) | absent | present | rune | take rune (done) |
| `src/receipt_v1/vectors.rs` `vectors_dir` | — | `CARGO_MANIFEST_DIR/../../vectors/receipt-v1` (escapes this repo) | here needs `vectors/receipt-v1` | patch hunk 3 (done) |
| `vectors/receipt-v1/README.md` | absent | present | rune | take rune (done) |
| `vectors/receipt-v1/bad_timestamp.json` | absent | present | rune | take rune (done) |
| `vectors/receipt-v1/duplicate_key.json` | absent | present | rune | take rune (done) |
| `vectors/receipt-v1/expired_key.json` | absent | present | rune | take rune (done) |
| `vectors/receipt-v1/float_payload.json` | absent | present | rune | take rune (done) |
| `vectors/receipt-v1/missing_public_key.json` | absent | present | rune | take rune (done) |
| `vectors/receipt-v1/missing_receipt_id.json` | absent | present | rune | take rune (done) |
| `vectors/receipt-v1/revoked_key.json` | absent | present | rune | take rune (done) |
| `vectors/receipt-v1/tier_too_high.json` | absent | present | rune | take rune (done) |
| `vectors/receipt-v1/unknown_key.json` | absent | present | rune | take rune (done) |
| `vectors/receipt-v1/valid.json` | absent | present | rune | take rune (done) |

`vectors/receipt-v1/README.md` names `crates/rune-record/src/receipt_v1/vectors.rs` and
`cargo test -p rune-record`; rune paths, kept verbatim (it is rune's doc).

### Public-only paths (this repo only)

| path | here | rune | which side is right | disposition |
|---|---|---|---|---|
| `.gitignore` | present | (workspace root's) | here | public-only kept |
| `CHANGELOG.md` | present | absent | here | public-only kept (v0.2.0 entry in T004) |
| `Cargo.lock` | present | (workspace root's) | here | public-only kept (updated in T004) |
| `LICENSE` | Apache-2.0 | absent (`UNLICENSED`) | here (prompt: keep LICENSE) | public-only kept |
| `README.md` | present | absent | here | public-only kept (draft rewrite in T004) |
| `docs/DIVERGENCE.md` | present | absent | here | public-only kept ("superseded" header in T004) |
| `docs/REVERIFY.md` | present | absent | here | public-only kept |
| `docs/reverify/A-locked.txt` | present | absent | here | public-only kept |
| `docs/reverify/A.txt` | present | absent | here | public-only kept |
| `docs/reverify/C-source.txt` | present | absent | here | public-only kept |
| `docs/reverify/C.txt` | present | absent | here | public-only kept |
| `docs/reverify/files.sha256` | present | absent | here | public-only kept |
| `docs/reverify/lib-A.txt` | present | absent | here | public-only kept |
| `docs/reverify/lib-C.txt` | present | absent | here | public-only kept |
| `docs/reverify/mirror.txt` | present | absent | here | public-only kept |
| `docs/reverify/sources.txt` | present | absent | here | public-only kept |
| `docs/reverify/tm-weather-head.txt` | present | absent | here | public-only kept |
| `docs/MIRROR.md` | this file (added after `0959ac5`) | absent | here | public-only kept |
| `scripts/reverify-weather.sh` | present | absent | here | public-only kept |
| `tests/fixtures/scry-rune-001.toml` | present | same bytes at rune `manifests/scry-rune-001.toml` (outside the mirrored set) | equal | public-only kept (read by the `EXAMPLE` patch hunk) |
| `tests/jcs_vectors.rs` (`vendored_file_is_byte_identical`, `jcs_matches_every_vector`) | present | absent | here | public-only kept |
| `tests/vectors/jcs.json` | present (vendored from receipt-verify) | absent | here | public-only kept (see G003) |

## Cargo.toml (not mirrored)

| field | here `0959ac5` | rune `0018299` | kept |
|---|---|---|---|
| `version` | `0.1.0` | `0.1.0` | here, bumped to `0.2.0` (T004) |
| `license` | `Apache-2.0` | `UNLICENSED` | here (rune-side alignment is a follow-up) |
| `description` | "TunnelMind signed records: Receipt v1 signer/verifier…" | "ADR-001 attestation receipts + ADR-002 manifest validation for Rune nodes" | here |
| `repository`, `readme`, `keywords`, `categories` | set | absent | here |
| `[features]` | `default = ["manifest"]`, `manifest = ["dep:toml"]` | none | here |
| `toml` | `optional = true` | required | here |
| other deps | serde (derive), serde_json, serde_json_canonicalizer 0.3, ring 0.17, anyhow, hex 0.4, base64 0.22, time 0.3, uuid (v7) | same | same set |
| dev-deps | `tempfile = "3"`, `serde_json` + `raw_value` | `tempfile = "3"` | here (`raw_value` is for `tests/jcs_vectors.rs`) |

## Test-fn count (SC-004)

Method: `cargo test --locked -q [--no-default-features] -- --list | grep -c ': test$'` (lists
`#[ignore]` tests too), cross-checked with `grep -c '#\[test\]'` per file. Rune was run from a
`git archive 0018299 crates/rune-record vectors/receipt-v1 manifests/scry-rune-001.toml` copy in `/tmp`
(`cargo test --offline`); rune has no features, so one set.

| tree | default | `--no-default-features` | per file (`#[test]`) |
|---|---|---|---|
| here `0959ac5` | 40 | 32 | manifest 8, receipt 10, receipt_v1 13, conformance 4, signing 3, jcs_vectors 2 |
| rune `0018299` | 79 (1 `#[ignore]`: `write_receipt_v1_vectors`) | 79 | manifest 8, receipt 10, receipt_v1 11, conformance 4, signing 6, trust 38, vectors 2 |
| **union after sync** | **83** | **75** | rune 79 + public-only 4 (`standard_engine_refuses_non_canonical_base64`, `respelled_signature_fields_fail_verify`, `vendored_file_is_byte_identical`, `jcs_matches_every_vector`); no-default drops the 8 `manifest::tests` behind the kept gate |

Name check: `sort -u` of both default lists = 83 lines; 4 names only here (above), 43 only rune.
No test fn is dropped.

## Public-API check for the three base64 items (T002 input)

`standard_engine_refuses_non_canonical_base64`, `respellings`, `respelled_signature_fields_fail_verify`
need **only the public API**: `base64::engine::general_purpose::STANDARD` + `base64::Engine` (base64 is a
normal dependency), `receipt_v1::{sign, verify, Source, Options}` (all `pub`, `Options: Default`),
`signing::RuneSigner::load_or_generate` (`pub`), `serde_json::json`, `tempfile` (dev-dep). The private
helpers they call (`tests::signer`, `tests::source`, `tests::signed`) are 3–5 lines over that API and get
copied into the integration test. So: move to `tests/`, no patch hunk.

## G003 (Tollbooth → Fareseal): not renamed here

`grep -rni tollbooth . --exclude-dir=target --exclude-dir=.git --exclude-dir=specs` → one hit,
`tests/vectors/jcs.json:3` (the `"about"` string: "…tollbooth core, atap-js…"). The file is vendored
verbatim from receipt-verify ("Other repos vendor this file verbatim: change it here first") and its
sha256 is pinned at `tests/jcs_vectors.rs:12`:
`const VECTORS_SHA256: &str = "81a79410874c5ed24ea0468a3d684e3a03111dc0a2fd2e8317ed3e3dfb978cdd";`.
Editing it here alone fails `vendored_file_is_byte_identical`. Rune's crate and vectors: 0 hits.
Follow-up (owner): rename in receipt-verify first, then re-vendor everywhere.

## Patch hunks (`scripts/rune-mirror.patch`, T003 — done)

All four apply with `patch -p1 --forward` to rune `0018299`; `scripts/sync-from-rune.sh --check` pins the
patch's sha256 in `RUNE_SYNC`. T002 moved every public-only test, so no test hunk is needed.

1. `src/lib.rs`: `#[cfg(feature = "manifest")]` on `pub mod manifest` — wx-receipt-verify builds with
   `default-features = false`, so `toml` must stay optional.
2. `src/manifest.rs`: `tests::EXAMPLE` → `../tests/fixtures/scry-rune-001.toml` — rune's path climbs out of
   the crate to `manifests/`, which this repo does not have (same bytes).
3. `src/receipt_v1/vectors.rs`: `vectors_dir` → `CARGO_MANIFEST_DIR/vectors/receipt-v1` — the vectors sit at
   the repo root here, two levels up in rune.
4. `src/receipt_v1/conformance.rs`: `skip()`, its two call sites in `verify_with_node`, and the
   `TM_REQUIRE_JS_INTEROP` module-doc sentence — a run that must prove JS interop cannot pass on a skip.
   Rune's other doc wording is taken as is (the old "Last synced 2026-10-19" phrasing is dropped).

## Verify this file

```sh
comm -3 <(git ls-tree -r --name-only origin/main | sort) \
  <(git -C /home/o2k/rune ls-tree -r --name-only 0018299 crates/rune-record vectors/receipt-v1 \
    | sed 's#^crates/rune-record/##' | sort) | sed 's/^\t//' \
  | while read -r p; do grep -qF "$p" docs/MIRROR.md || { echo "missing: $p"; exit 1; }; done
```
