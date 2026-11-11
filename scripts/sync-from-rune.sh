#!/usr/bin/env bash
# sync-from-rune.sh — this repo's src/ is a generated copy of rune's
# crates/rune-record. Rune is the source of truth; never edit src/ here.
#
# To change something:
#   1. edit it in rune (crates/rune-record or vectors/receipt-v1) and commit;
#   2. scripts/sync-from-rune.sh sync <rune-commit>
#   3. commit the result (src/, vectors/receipt-v1/, the crawl vector, RUNE_SYNC).
#
# sync <commit>  reads rune with `git archive` only (never checks out or writes
#   there), applies scripts/rune-mirror.patch, then replaces exactly src/,
#   vectors/receipt-v1/ and tests/vectors/crawl_record_v1.json. Nothing else in
#   this repo is touched. RUNE_SYNC records the rune sha, the patch sha256 and
#   a sha256 of every mirrored file.
#
# rune-mirror.patch holds the public repo's few needs, against rune's files:
#   the `manifest` feature gate, repo-local paths for the receipt vectors and
#   the manifest fixture, and conformance.rs's TM_REQUIRE_JS_INTEROP skip().
#   If a hunk stops applying, sync fails: refresh the patch by hand
#   (diff -u rune's file against the wanted one) and sync again.
#
# --check  fails if RUNE_SYNC is missing, the patch changed, any mirrored file
#   was edited, or a file was added/removed under the mirrored paths. When
#   $RUNE_DIR holds the recorded commit it also regenerates and `diff -r`s.
#   In CI there is no rune checkout, so --check is hash mode only: it proves
#   the tree matches RUNE_SYNC, not that RUNE_SYNC matches rune.
set -euo pipefail

RUNE_DIR=${RUNE_DIR:-$HOME/rune}
cd "$(dirname "$0")/.."
PATCH=scripts/rune-mirror.patch
PATHS=(src vectors/receipt-v1 tests/vectors/crawl_record_v1.json)

die() { echo "sync-from-rune: $*" >&2; exit 1; }
file_list() { find "${PATHS[@]}" -type f 2>/dev/null | LC_ALL=C sort; }

# generate <commit> <dir>: rune's files, patched, laid out as this repo's paths.
generate() {
  git -C "$RUNE_DIR" archive "$1" crates/rune-record/src \
    crates/rune-record/tests/vectors/crawl_record_v1.json vectors/receipt-v1 | tar -x -C "$2"
  mkdir -p "$2/out/tests/vectors" "$2/out/vectors"
  mv "$2/crates/rune-record/src" "$2/out/src"
  mv "$2/crates/rune-record/tests/vectors/crawl_record_v1.json" "$2/out/tests/vectors/"
  mv "$2/vectors/receipt-v1" "$2/out/vectors/"
  patch -s -d "$2/out" -p1 --forward --no-backup-if-mismatch -r - < "$PATCH" \
    || die "rune-mirror.patch does not apply at $1"
}

tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT

case "${1:-}" in
  sync)
    [ -n "${2:-}" ] || die "usage: $0 sync <rune-commit>"
    sha=$(git -C "$RUNE_DIR" rev-parse --verify "$2^{commit}")
    generate "$sha" "$tmp"
    rm -rf src vectors/receipt-v1 && mkdir -p vectors
    cp -r "$tmp/out/src" src
    cp -r "$tmp/out/vectors/receipt-v1" vectors/
    cp "$tmp/out/tests/vectors/crawl_record_v1.json" tests/vectors/
    { echo "rune $sha"
      echo "patch $(sha256sum < "$PATCH" | cut -d' ' -f1)"
      echo
      file_list | xargs -d '\n' sha256sum; } > RUNE_SYNC
    echo "synced rune $sha" ;;
  --check)
    [ -f RUNE_SYNC ] || die "RUNE_SYNC missing: run '$0 sync <commit>'"
    sha=$(sed -n '1s/^rune //p' RUNE_SYNC)
    [ "$(sed -n '2s/^patch //p' RUNE_SYNC)" = "$(sha256sum < "$PATCH" | cut -d' ' -f1)" ] \
      || die "$PATCH changed since the last sync"
    tail -n +4 RUNE_SYNC | sha256sum -c --quiet - >&2 || die "mirrored files differ from RUNE_SYNC"
    diff <(tail -n +4 RUNE_SYNC | cut -c67-) <(file_list) >&2 || die "mirrored file set differs from RUNE_SYNC"
    if git -C "$RUNE_DIR" cat-file -e "$sha^{commit}" 2>/dev/null; then
      generate "$sha" "$tmp"
      for p in "${PATHS[@]}"; do diff -r "$tmp/out/$p" "$p" >&2 || die "$p differs from rune $sha"; done
      echo "ok: matches rune $sha"
    else
      echo "ok: hash mode only (rune $sha not in $RUNE_DIR)"
    fi ;;
  *) die "usage: $0 sync <rune-commit> | --check" ;;
esac
