# Feature: rune-record-mirror-1 — the public rune-record is generated from the crate production pins

Spec: `001-rune-record-mirror-1` · Repo: `TunnelMind/rune-record` (public) · Planned 2026-11-10

## Intent

The public `TunnelMind/rune-record` has drifted from `TunnelMind/rune` `crates/rune-record`, the crate
every production consumer pins, while both claim `name = "rune-record"`, `version = "0.1.0"`. This run
makes the public repo a generated copy of the rune crate, taken from a named rune commit, with a check
that fails when the two stop matching, and loses nothing that exists only in the public repo.

Key sentences from the owner's prompt:

> "Today this repo has diverged from /home/o2k/rune/crates/rune-record under the same crate name and version."

> "Diff the two trees […] and list every difference with which side is right; nothing unique to this repo may be lost silently."

> "Replace the sources with the rune crate's (including verify_pinned and the shared vectors), keep this repo's
> README/LICENSE/CI, bump the version above both, CHANGELOG entry. `scripts/sync-from-rune.sh` documents and
> repeats the copy from a named rune commit; a CI check fails if the tree does not match the recorded commit."

> "One line renamed: Tollbooth → Fareseal (Josh 2026-11-10)."

Source: the 2026-11-10 review, `~/.crew/coord/review-2026-11-10/01-receipt-core.md` §2 (verdict REPURPOSE:
"make it the only copy, or delete it. Do not keep two."), and the rune repo's measured proposal
`docs/rune-record-mirror.md` (rune `origin/main`, added by receipt-pin-1 T006).

## Precondition (checked at planning time)

- receipt-pin-1 **is merged** in rune: `origin/main` = `0018299` "Merge pull request #6 from
  TunnelMind/crew/receipt-pin-1" (full sha `00182997c6b8ec98ff990abcae7a123970bc26de`). It adds
  `src/receipt_v1/trust.rs` (`TrustBundle`, `verify_pinned`), `src/receipt_v1/vectors.rs` and the shared
  vectors at the rune repo root, `vectors/receipt-v1/` (10 `*.json` + `README.md`).
- This run's branch `crew/rune-record-mirror-1` was cut at stale local `main` `f386e8f`, **9 commits behind
  `origin/main` `0959ac5`** (CHANGELOG.md, docs/, scripts/reverify-weather.sh and ported tests are missing
  from the worktree). GATE 1 fast-forwards it before any task runs.

## Claims checked against origin/main (run rule: skip and report what is done or wrong)

| Prompt item | Status on origin/main | Plan |
|---|---|---|
| G001 diff | Not done for rune `0018299` (DIVERGENCE.md compares older revs `91652a7`/`6302282`; rune's proposal measured against `67aad37`). | T001 |
| G002 replace sources | Not done. Rune's `vectors.rs` reads `CARGO_MANIFEST_DIR/../../vectors/receipt-v1`, which escapes this repo, and rune's `lib.rs` has no `manifest` feature gate, so a byte-for-byte copy cannot build here with `--no-default-features` nor find the vectors. | T002–T005: a small recorded patch file, applied by the sync script |
| G002 "keep this repo's CI" | **There is no CI** on origin/main (no `.github/`). | T005 adds the first workflow |
| G003 Tollbooth → Fareseal | **Wrong place.** The only hit in the repo is `tests/vectors/jcs.json:3` ("…tollbooth core…"). That file is vendored verbatim from receipt-verify (`test/vectors/jcs.json:3`, still "tollbooth"), its sha256 `81a79410…8cdd` is pinned in `tests/jcs_vectors.rs:12`, and tollbooth/atap carry the same sha. Renaming it here alone fails `jcs_vectors.rs` and breaks the vendor-verbatim rule. Rune's crate and vectors have no hit. | Not edited here. Recorded in docs/MIRROR.md and closeout; follow-up (owner): rename in receipt-verify first, then re-vendor everywhere |
| G004 tag/publish | Owner only. | T007 (owner) |

## User Scenarios

### US1 — A maintainer changes the signer once (P1)
A change lands in rune `crates/rune-record`. The maintainer runs
`scripts/sync-from-rune.sh <rune-commit>` here and gets the new sources, the recorded commit and hashes; the
diff is reviewable as a normal PR.

- **Given** rune commit `0018299`, **when** `scripts/sync-from-rune.sh 0018299` runs, **then** `src/`,
  `vectors/receipt-v1/` and `tests/vectors/crawl_record_v1.json` equal rune's tree at that commit with
  `scripts/rune-mirror.patch` applied, and `RUNE_SYNC` names the full commit sha.
- **Given** the synced tree, **when** the script runs again with the same commit, **then** `git status
  --porcelain` is empty (idempotent).

### US2 — CI refuses a hand edit (P1)
- **Given** a PR that edits `src/lib.rs` by hand, **when** CI runs `scripts/sync-from-rune.sh --check`,
  **then** it exits non-zero naming the file.
- **Given** a PR that adds a file under `src/`, **then** `--check` fails (file set differs from `RUNE_SYNC`).

### US3 — Nothing unique to the public repo is lost (P1)
- **Given** the base64 respelling tests, `TM_REQUIRE_JS_INTEROP`, the `manifest` feature, the JCS vectors,
  the scry fixture, CHANGELOG, docs and scripts that exist only here, **then** each still exists after the run
  (moved to `tests/`, carried in the patch, or kept as a public-only file) and `docs/MIRROR.md` says where.

### US4 — A reader sees which version is which (P2)
- **Given** the merged tree, **then** `Cargo.toml` says `version = "0.2.0"` (above both 0.1.0s), CHANGELOG
  has a v0.2.0 entry naming rune commit `0018299`, and README says this repo is generated from rune.

## Functional Requirements

- **FR-001** `docs/MIRROR.md` lists every path and every `fn` that differs between this repo's `origin/main`
  (`0959ac5`) and rune `origin/main:crates/rune-record` + `vectors/receipt-v1` (`0018299`), with which side is
  right and the disposition (take rune / public-only, kept / carried in patch / moved to tests / G003 skip).
- **FR-002** `scripts/sync-from-rune.sh <commit>` reads rune with `git -C "$RUNE_DIR" archive <commit>` only
  (no checkout, fetch or write in rune; `RUNE_DIR` defaults to `$HOME/rune`), applies
  `scripts/rune-mirror.patch`, replaces exactly the mirrored set (`src/`, `vectors/receipt-v1/`,
  `tests/vectors/crawl_record_v1.json`) and writes `RUNE_SYNC` (full rune sha, patch sha256, one
  `sha256  path` line per mirrored file, sorted). Its header comment documents the procedure.
- **FR-003** `scripts/sync-from-rune.sh --check` exits 0 iff every mirrored file matches `RUNE_SYNC`, the file
  set under the mirrored paths equals the listed set, and the patch hash matches; when `$RUNE_DIR` has the
  recorded commit it also regenerates into a temp dir and `diff -r`s against the tree.
- **FR-004** `scripts/rune-mirror.patch` holds only what this repo needs and rune lacks: the
  `#[cfg(feature = "manifest")]` gate in `src/lib.rs`, the vectors path in `src/receipt_v1/vectors.rs`, and
  public-only test code that cannot live in `tests/` (e.g. `TM_REQUIRE_JS_INTEROP` in
  `src/receipt_v1/conformance.rs`). Each hunk is listed in docs/MIRROR.md.
- **FR-005** Public-only tests that use only the public API (`respelled_signature_fields_fail_verify`,
  `standard_engine_refuses_non_canonical_base64`) move to an integration test under `tests/`.
- **FR-006** `Cargo.toml` stays this repo's (Apache-2.0, `manifest` feature, metadata, `raw_value` dev
  feature) with `version = "0.2.0"`; `Cargo.lock` updated to match. `LICENSE`, `README.md`, `CHANGELOG.md`,
  `docs/`, `scripts/reverify-weather.sh`, `tests/jcs_vectors.rs`, `tests/vectors/jcs.json`,
  `tests/fixtures/scry-rune-001.toml` stay.
- **FR-007** CHANGELOG v0.2.0 entry; README states the repo is generated from rune `crates/rune-record` at the
  commit in `RUNE_SYNC` (edit there, then sync) and drops the false "canonical: change it here" and consumer
  lines; `docs/DIVERGENCE.md` gets a one-line "superseded by docs/MIRROR.md" header. README text is a
  **draft** for Josh (public copy voice).
- **FR-008** `.github/workflows/ci.yml` runs on push and pull_request: `cargo test --locked`,
  `cargo test --locked --no-default-features`, `scripts/sync-from-rune.sh --check`. GitHub-hosted runner
  (public repo), `actions/checkout` pinned by full sha, no secrets.
- **FR-009** G003 is reported, not applied (see table above).

## Success Criteria

- **SC-001** `cargo test --locked && cargo test --locked --no-default-features` pass at the end of every task.
- **SC-002** `scripts/sync-from-rune.sh --check` exits 0 on the final tree; after `printf x >> src/lib.rs` it
  exits non-zero; after `touch src/extra.rs` it exits non-zero.
- **SC-003** `scripts/sync-from-rune.sh 00182997c6b8ec98ff990abcae7a123970bc26de` followed by
  `git status --porcelain` prints nothing on the final tree.
- **SC-004** `cargo test` lists `receipt_v1_vectors_agree` (the 10 shared vectors) and the `verify_pinned`
  tests from `trust.rs`; total test count ≥ the union count recorded in docs/MIRROR.md (no test fn from either
  side missing, except ones MIRROR.md names as dropped with a reason).
- **SC-005** `grep -n '^version' Cargo.toml` → `version = "0.2.0"`; `grep -c 0018299 CHANGELOG.md` ≥ 1.
- **SC-006** CI green on the PR (checked at GATE F).

## Edge Cases

- `$RUNE_DIR` missing or lacking the commit: write mode errors out; `--check` falls back to hash mode and says so.
- Rune's local working tree is on another branch: the script never checks out; `git archive <commit>` reads the object store.
- The patch no longer applies to a newer rune commit: script fails loudly; the next sync updates the patch.
- `.pub`/key files: none are mirrored; no secrets in the tree.
- `tests/vectors/crawl_record_v1.json` is identical on both sides today (0 changed lines); it is still mirrored so drift is caught.
- 9P exec bits (WSL): `scripts/sync-from-rune.sh` must be committed mode 100755 (`git update-index --chmod=+x` if needed).

## Assumptions

- Direction is decided by the prompt: rune `crates/rune-record` is the source, this repo the mirror (rune's
  proposal §4 step 1).
- Licence is decided by the prompt ("keep this repo's … LICENSE"): the mirrored code is published under this
  repo's Apache-2.0. Rune's crate says `UNLICENSED`; aligning that field is a rune-side follow-up. Flagged at GATE F.
- Version 0.2.0: `verify_pinned`/`TrustBundle` are new public API, a minor bump in 0.x.
- CHANGELOG.md, docs/ and scripts/reverify-weather.sh carry over as public-only files (nothing lost silently).
- Rune is private, so CI cannot fetch it; CI's check is against the hashes recorded with the commit. The full
  regenerate-and-diff check runs wherever rune is present (BEAST, reviewer).

## Out of Scope

- Any change in the rune repo (upstreaming the patch hunks so the patch empties, `license` field) — follow-up run.
- Re-pinning `wx-receipt-verify` or any consumer (rune proposal §4 steps 5–6).
- Tagging, publishing to crates.io, pushing, GitHub settings (G004 is owner).
- Renaming Tollbooth in the vendored `jcs.json` (wrong place; see table).
- Rewriting history or the published weather re-verify.
