# Closeout — 001-rune-record-mirror-1

This repo (`rune-record`) is now a generated mirror of rune `crates/rune-record` at
`00182997c6b8ec98ff990abcae7a123970bc26de` (`0018299`). Branch `crew/rune-record-mirror-1`, base
`origin/main` `0959ac5`. Every number below was measured on 2026-11-10 while writing this file.

## Per gap

### G001 — diff this repo against rune `0018299` (done, T001 `f2773f7`)

`docs/MIRROR.md`: one row per path and per changed fn, with the side that is right and the
disposition. Rows are marked done, since T003 synced the tree.

- Command: the "Verify this file" loop at the end of `docs/MIRROR.md` (every path from
  `comm -3` of the two `git ls-tree` lists must appear in the file).
- Result: exit 0, no `missing:` lines.

### G002 — replace the sources with rune's, keep this repo's CI (done, T002–T005)

| task | commit | what |
|---|---|---|
| T002 | `739f584` | the public-only base64 tests (`standard_engine_refuses_non_canonical_base64`, `respelled_signature_fields_fail_verify` and the `respellings` helper) moved from `src/receipt_v1.rs` to `tests/receipt_v1_base64.rs`, outside the synced tree |
| T003 | `a53e080` | `scripts/sync-from-rune.sh` (sync + `--check`), `scripts/rune-mirror.patch` (4 changes in 4 files (6 hunks): `manifest` feature gate, `manifest.rs` fixture path, `vectors.rs` vectors dir, conformance `skip()`), `RUNE_SYNC`, and the sync itself: `src/`, `vectors/receipt-v1/`, `tests/vectors/crawl_record_v1.json` |
| T004 | `a8ef2bb` | `Cargo.toml` `version = "0.2.0"`, `Cargo.lock`, CHANGELOG v0.2.0, README, DIVERGENCE "superseded" header |
| T005 | `317e02a` | `.github/workflows/ci.yml` |

"Keep this repo's CI": there was none — `origin/main` has no `.github/`. T005 adds the first
workflow: on push and pull_request it runs `cargo test --locked`,
`cargo test --locked --no-default-features` and `scripts/sync-from-rune.sh --check`. CI has no rune
checkout, so `--check` there is hash mode only: it proves the tree matches `RUNE_SYNC`, not that
`RUNE_SYNC` matches rune.

- `scripts/sync-from-rune.sh --check` → `ok: matches rune 00182997c6b8ec98ff990abcae7a123970bc26de`, exit 0
  (run locally, where `~/rune` holds the commit, so it also regenerated and `diff -r`'d).
- `RUNE_SYNC`: `rune 00182997c6b8ec98ff990abcae7a123970bc26de`, `patch 3d54e582…4c99`
  (= `sha256sum scripts/rune-mirror.patch`), plus a sha256 for each of the 20 mirrored files.
- Test results: see "Test counts" below.

### G003 — rename Tollbooth → Fareseal (SKIPPED on purpose)

Not edited. `grep -rni tollbooth . --exclude-dir=target --exclude-dir=.git --exclude-dir=specs`
finds one code/data hit, `tests/vectors/jcs.json:3` (the `"about"` string: "…tollbooth core,
atap-js…"); the only other hits are `docs/MIRROR.md`'s own G003 note. That file:

- is vendored verbatim from receipt-verify `test/vectors/jcs.json` (its own text: "Other repos
  vendor this file verbatim: change it here first");
- has its sha256 pinned at `tests/jcs_vectors.rs:12`,
  `VECTORS_SHA256 = "81a79410874c5ed24ea0468a3d684e3a03111dc0a2fd2e8317ed3e3dfb978cdd"`
  (`sha256sum tests/vectors/jcs.json` gives the same value now);
- has the same sha in tollbooth and atap.

Renaming it here alone fails `vendored_file_is_byte_identical` in `tests/jcs_vectors.rs` and
breaks the vendor-verbatim rule. Rune's side has no hit:
`git -C ~/rune grep -il tollbooth 0018299 -- crates/rune-record vectors/receipt-v1` → no output,
exit 1. Follow-up (a) below.

### G004 — tag and publish (owner, T007)

Not done by the crew. T007: after merge, Josh tags `v0.2.0`, pushes the tag himself, and
publishes to crates.io if he wants a release.

## Test counts

Method: `cargo test --locked [--no-default-features]` "test result" lines, and
`cargo test --locked -q [--no-default-features] -- --list | grep -c ': test$'` (lists `#[ignore]`d
tests too). "Before" was measured at `origin/main` `0959ac5` in a clean scratch checkout
(`~/agent-crew/bin/crew-scratch --rev 0959ac5 -- …`), not in this worktree.

| tree | feature set | passed | ignored | listed | per binary |
|---|---|---|---|---|---|
| before `0959ac5` | default | 40 | 0 | 40 | lib 38, jcs_vectors 2 |
| before `0959ac5` | `--no-default-features` | 32 | 0 | 32 | lib 30, jcs_vectors 2 |
| after (this branch) | default | 82 | 1 | 83 | lib 78 + 1 ignored, jcs_vectors 2, receipt_v1_base64 2 |
| after (this branch) | `--no-default-features` | 74 | 1 | 75 | lib 70 + 1 ignored, jcs_vectors 2, receipt_v1_base64 2 |

All runs: 0 failed. The ignored test is rune's `write_receipt_v1_vectors` (regenerates the
vectors). The no-default set drops the 8 `manifest::tests` behind the kept feature gate. 83/75
match `docs/MIRROR.md`'s predicted union (rune 79 + 4 public-only). No test fn was dropped.

## Verify

```sh
cargo test --locked && cargo test --locked --no-default-features && scripts/sync-from-rune.sh --check
```

Ran 2026-11-10: exit 0 (results as above).

## Follow-ups for the owner

- **T007** (G004): tag `v0.2.0` after merge, push the tag, publish to crates.io if wanted.
- **(a)** Rename Tollbooth → Fareseal in receipt-verify `test/vectors/jcs.json` **first**, then
  re-vendor that file here (update `VECTORS_SHA256` in `tests/jcs_vectors.rs`), in tollbooth and in
  atap. Doing it in any other order breaks the byte-identical pin in every vendoring repo.
- **(b)** A rune run that upstreams the patch's 4 changes in 4 files (6 hunks) into `crates/rune-record` (feature gate,
  repo-local fixture and vectors paths, conformance `skip()`), so `scripts/rune-mirror.patch` empties,
  and aligns rune's `license` (`UNLICENSED` there, `Apache-2.0` here).
- **(c)** Re-pin wx-receipt-verify to the merged commit of this repo (rune proposal §4 step 5).
