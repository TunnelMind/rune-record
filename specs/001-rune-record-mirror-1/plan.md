# Plan: rune-record-mirror-1

## Constitution check

No `.specify/` in this repo (no constitution, no templates). Owner house rules apply instead: review-first,
never break behaviour, comments for readers, ponytail (smallest diff), retroactive; tests and their change in
one task; nothing merges, tags, publishes or pushes without Josh's word. No ADR gate is required. One stop is
structural: GATE 1, because the run branch is 9 commits behind `origin/main`.

## Inputs (read-only)

- This repo `origin/main` = `0959ac5`. Files: `Cargo.toml`, `Cargo.lock`, `LICENSE`, `README.md`,
  `CHANGELOG.md`, `.gitignore`, `src/{lib,manifest,receipt,receipt_v1,signing}.rs`,
  `src/receipt_v1/conformance.rs`, `tests/jcs_vectors.rs`, `tests/vectors/{jcs,crawl_record_v1}.json`,
  `tests/fixtures/scry-rune-001.toml`, `docs/{DIVERGENCE,REVERIFY}.md`, `docs/reverify/*`,
  `scripts/reverify-weather.sh`. No `.github/`.
- Rune `/home/o2k/rune`, commit `00182997c6b8ec98ff990abcae7a123970bc26de` (`origin/main`, receipt-pin-1
  merge). Read with `git -C /home/o2k/rune show|archive|ls-tree 0018299 …` only. **Never check out, fetch or
  write in /home/o2k/rune** (its working tree is on another branch). Paths: `crates/rune-record/src/**`
  (adds `receipt_v1/trust.rs`, `receipt_v1/vectors.rs`), `crates/rune-record/tests/vectors/crawl_record_v1.json`,
  `vectors/receipt-v1/*` (10 json + README.md). Background: rune `docs/rune-record-mirror.md`.

## Differences already measured (T001 confirms and completes)

- `signing.rs`: rune adds `write_pub_if_changed` (atomic `.pub`) + 3 tests (`pub_untouched_when_unchanged`,
  `pub_stale_is_replaced`, `pub_reader_never_sees_partial`) and `new_key`. **Rune right.**
- `receipt_v1.rs`: public-only `respelled_signature_fields_fail_verify`, `respellings`,
  `standard_engine_refuses_non_canonical_base64` (K002). **Keep: move to `tests/receipt_v1_base64.rs`** if
  they need only `receipt_v1::{sign,verify}`, `signing::RuneSigner`, `base64`; else a patch hunk.
- `receipt_v1/conformance.rs`: public-only `skip()` with `TM_REQUIRE_JS_INTEROP=1`. **Keep via patch hunk.**
- `lib.rs`: rune header right (public line 8's consumer list is false per review §2); public
  `#[cfg(feature = "manifest")]` gate **kept via patch hunk** (wx-receipt-verify uses `default-features = false`).
- `receipt_v1/vectors.rs`: `../../vectors/receipt-v1` → `vectors/receipt-v1` **via patch hunk**.
- `Cargo.toml`: not mirrored. Public metadata, licence, feature, `raw_value` stay; deps are the same set.
- Public-only files kept: JCS vectors + test + fixture, CHANGELOG, docs, reverify script, LICENSE, README.
- G003: only hit is vendored, sha-pinned `tests/vectors/jcs.json:3`; not edited (spec table).

## Approach

1. GATE 1: coordinator fast-forwards the branch to `origin/main` (no rewrite: the branch has no own commits).
2. T001 writes `docs/MIRROR.md` (the G001 table, the union test count, the G003 finding).
3. T002 moves the public-only base64 tests into `tests/` (pure move, same count) so the sync cannot drop them.
4. T003 adds `scripts/sync-from-rune.sh`, `scripts/rune-mirror.patch`, runs the sync at `0018299`, writes
   `RUNE_SYNC`. Script outline (bash, `set -euo pipefail`, ~60 lines):
   - `sync <commit>`: `tmp=$(mktemp -d)`; `git -C "$RUNE_DIR" archive "$commit" crates/rune-record/src
     crates/rune-record/tests/vectors/crawl_record_v1.json vectors/receipt-v1 | tar -x -C "$tmp"`; lay out as
     this repo's paths in `$tmp/out`; `patch -d "$tmp/out" -p1 --forward < scripts/rune-mirror.patch`;
     `rm -rf src vectors/receipt-v1`; copy in; write `RUNE_SYNC` from `git rev-parse "$commit^{commit}"`,
     `sha256sum scripts/rune-mirror.patch`, `sha256sum` of `find src vectors/receipt-v1
     tests/vectors/crawl_record_v1.json -type f | LC_ALL=C sort`.
   - `--check`: hash lines via `sha256sum -c --quiet`, file set via the same `find | sort` vs RUNE_SYNC,
     patch hash; if `git -C "$RUNE_DIR" cat-file -e <sha>` succeeds, regenerate into a temp dir and `diff -r`.
5. T004 version 0.2.0, Cargo.lock, CHANGELOG, README draft, DIVERGENCE header.
6. T005 CI workflow.
7. T006 closeout.

## Risks

- **Patch drift**: future rune changes to the patched lines break the patch → script fails loudly (wanted).
  Follow-up shrinks it by upstreaming the gate and a vectors path that works in both trees.
- **Public release of rune code**: the mirrored files become Apache-2.0 here; rune says `UNLICENSED`.
  Prompt says keep LICENSE; Josh confirms at GATE F by merging.
- **`--locked`** fails if T004 bumps the version without updating `Cargo.lock`: same task, run `cargo update
  -p rune-record --offline` (or plain `cargo build`) then `--locked`.
- **9P exec bits**: commit the script as 100755; T003 checks `git ls-files -s scripts/sync-from-rune.sh`.
- **No JS interop locally**: conformance tests skip with a notice unless `TM_REQUIRE_JS_INTEROP=1`; fine.
- CI's check cannot see private rune: it proves "tree = recorded hashes", the full regenerate check is local.

## Verify (every task)

```sh
cargo test --locked && cargo test --locked --no-default-features
```

From T003 on, also `scripts/sync-from-rune.sh --check`.
