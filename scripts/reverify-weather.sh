#!/usr/bin/env bash
# Re-verify the published tm-weather log with two rune-record builds:
#   C = the rev tm-weather pins (6302282), A = this repo's tree.
# Both wx-receipt builds and a library-only pass run on ONE mirror of
# https://tunnelmind.ai/weather/log/v1/, so any difference is the library's.
#
# Writes only to a fresh mktemp -d ($TMP, removed on success, kept on failure)
# and $OUT. Never writes into $TM_WEATHER, ~/rune or this repo besides $OUT.
#
# Env: TM_WEATHER (~/tm-weather), RUNE_RECORD_A (repo root), OUT (docs/reverify),
#      BASE (https://tunnelmind.ai/weather/log/v1), RUNE_GIT (~/rune),
#      MIRROR_JOBS (parallel curls, 4).
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
TM_WEATHER=${TM_WEATHER:-$HOME/tm-weather}
RUNE_GIT=${RUNE_GIT:-$HOME/rune}
RUNE_RECORD_A=$(cd "${RUNE_RECORD_A:-$ROOT}" && pwd)
OUT=${OUT:-$ROOT/docs/reverify}
BASE=${BASE:-https://tunnelmind.ai/weather/log/v1}
RUNE_URL=https://github.com/TunnelMind/rune
C_REV=63022824c8a0e9a7afb64080f5994573e75bb821

mkdir -p "$OUT"
OUT=$(cd "$OUT" && pwd)
TMP=$(mktemp -d)
# Keep $TMP when anything fails so the run can be inspected.
trap 'rc=$?; if [ "$rc" -eq 0 ]; then rm -rf "$TMP"; else echo "FAILED (rc=$rc); temp kept at $TMP" >&2; fi' EXIT
echo "temp: $TMP"

# ── 1. mirror ────────────────────────────────────────────────────────────────
V1=$TMP/v1
mkdir -p "$V1"
curl --fail -sS --retry 3 "$BASE/files.txt" -o "$TMP/files.txt"
# The list is remote input: refuse anything that could escape $V1.
if grep -vqE '^[A-Za-z0-9._/-]+$' "$TMP/files.txt" || grep -qE '(^|/)\.\.(/|$)|^/' "$TMP/files.txt"; then
  echo "files.txt holds an unsafe path; refusing" >&2; exit 1
fi
grep -qx index.json "$TMP/files.txt" || echo index.json >>"$TMP/files.txt"
want=$(grep -c . "$TMP/files.txt")
echo "mirroring $want files from $BASE"
# Three args per file (-o, dest, url); -n 300 keeps triples aligned, 100 files
# per curl. --fail-early makes curl exit non-zero on the first miss, xargs then
# exits 123, and pipefail + set -e abort the script. The origin answers some
# requests with 502/520 under load, so transient errors are retried first.
awk -v d="$V1" -v b="$BASE" 'NF { print "-o"; print d "/" $0; print b "/" $0 }' "$TMP/files.txt" |
  xargs -d '\n' -P "${MIRROR_JOBS:-4}" -n 300 curl --fail --fail-early -sS --retry 5 --retry-all-errors --retry-delay 2 --create-dirs
got=$(find "$V1" -type f | wc -l)
if [ "$got" -ne "$want" ]; then
  echo "mirror count mismatch: files.txt lists $want, disk has $got" >&2; exit 1
fi
while IFS= read -r p; do
  [ -f "$V1/$p" ] || { echo "missing after mirror: $p" >&2; exit 1; }
done <"$TMP/files.txt"
(cd "$V1" && find . -type f | sed 's|^\./||' | LC_ALL=C sort | xargs -d '\n' sha256sum) >"$OUT/files.sha256"
receipts=$(find "$V1/receipts" -maxdepth 1 -name '*.json' -type f | wc -l)
read -r generated_at pubkey < <(python3 -I -c '
import json, sys
d = json.load(open(sys.argv[1]))
print(d["generated_at"], d["key"]["public_key_hex"])' "$V1/index.json")
printf 'base: %s\nfiles: %s\ngenerated_at: %s\nreceipts: %s\npubkey: %s\n' \
  "$BASE" "$got" "$generated_at" "$receipts" "$pubkey" >"$OUT/mirror.txt"

# ── 2. tm-weather source (git archive: no checkout, nothing written there) ──
TMW=$TMP/tmw
mkdir -p "$TMW"
TMW_HEAD=$(git -C "$TM_WEATHER" rev-parse HEAD)
git -C "$TM_WEATHER" archive HEAD | tar -x -C "$TMW"
echo "tm-weather HEAD: $TMW_HEAD" | tee "$OUT/tm-weather-head.txt"

# ── 3. build wx-receipt twice ────────────────────────────────────────────────
patch_cfg() { printf 'patch."%s".rune-record.path="%s"' "$RUNE_URL" "$1"; }

# Assert which rune-record source cargo resolved: first line of `cargo tree -i`
# is "rune-record vX (<source>)".
assert_source() { # label pattern cargo-flags...
  local label=$1 pat=$2 line; shift 2
  line=$( (cd "$TMW" && cargo tree -i rune-record "$@") | head -1)
  echo "$label tree: $line" | tee -a "$OUT/sources.txt"
  if ! grep -qF -- "$pat" <<<"$line"; then
    echo "$label resolved the wrong rune-record source (want $pat)" >&2; exit 1
  fi
}

# C: as pinned, from the cargo git cache; else an extracted copy of the rev.
C_TARGET=$TMP/target-C
C_ARGS=(--offline --locked)
if (cd "$TMW" && CARGO_TARGET_DIR=$C_TARGET cargo build --release -p wx-receipt "${C_ARGS[@]}"); then
  C_SOURCE=git-cache
  C_DEP="{ git = \"$RUNE_URL\", rev = \"$C_REV\" }"
  assert_source C "($RUNE_URL?rev=$C_REV#" "${C_ARGS[@]}"
else
  C_SOURCE=extracted-tree
  mkdir -p "$TMP/rune-C"
  git -C "$RUNE_GIT" archive "$C_REV" crates/rune-record | tar -x -C "$TMP/rune-C"
  C_TREE=$TMP/rune-C/crates/rune-record
  # A path patch rewrites the lock's source line, so --locked cannot hold here.
  C_ARGS=(--config "$(patch_cfg "$C_TREE")")
  (cd "$TMW" && CARGO_TARGET_DIR=$C_TARGET cargo build --release -p wx-receipt "${C_ARGS[@]}")
  C_DEP="{ path = \"$C_TREE\" }"
  assert_source C "($C_TREE)" "${C_ARGS[@]}"
fi
echo "C-source: $C_SOURCE" | tee "$OUT/C-source.txt"

# A: this repo's tree. The patch changes the lock's rune-record source line, so
# --locked is tried first and dropped only if cargo refuses it.
A_TARGET=$TMP/target-A
A_ARGS=(--config "$(patch_cfg "$RUNE_RECORD_A")" --locked)
if ! (cd "$TMW" && CARGO_TARGET_DIR=$A_TARGET cargo build --release -p wx-receipt "${A_ARGS[@]}"); then
  echo "A: --locked refused (the path patch changes Cargo.lock); building without it" | tee "$OUT/A-locked.txt"
  A_ARGS=(--config "$(patch_cfg "$RUNE_RECORD_A")")
  (cd "$TMW" && CARGO_TARGET_DIR=$A_TARGET cargo build --release -p wx-receipt "${A_ARGS[@]}")
fi
assert_source A "($RUNE_RECORD_A)" "${A_ARGS[@]}"

# ── 4. run both on the same mirror, with publish.mjs:157's args ─────────────
for v in C A; do
  t=${v}_TARGET
  rc=0
  "${!t}/release/wx-receipt" verify "$V1" --pubkey "$pubkey" --stations "$V1/stations.json" \
    --tsa-chain "$V1/tsa/freetsa.pem" "$V1/tsa/digicert.pem" >"$OUT/$v.txt" 2>&1 || rc=$?
  echo "exit $rc" >>"$OUT/$v.txt"
done

# ── 5. library pass: receipt_v1::verify on every receipt ────────────────────
# serde_json is needed only to parse; it resolves to the lock's version.
LIB=$TMP/libcheck
mkdir -p "$LIB/src"
cat >"$LIB/src/main.rs" <<'RS'
// Prints "<file> ok" or "<file> err <msg>" for each receipts/*.json, sorted.
fn main() {
    let dir = std::env::args().nth(1).expect("usage: libcheck <receipts dir>");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("read receipts dir")
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    for p in files {
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        let res = std::fs::read(&p)
            .map_err(|e| e.to_string())
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).map_err(|e| e.to_string()))
            .and_then(|v| rune_record::receipt_v1::verify(&v).map_err(|e| format!("{e:#}")));
        match res {
            Ok(_) => println!("{name} ok"),
            Err(m) => println!("{name} err {}", m.replace('\n', " ")),
        }
    }
}
RS
for v in C A; do
  if [ "$v" = A ]; then dep="{ path = \"$RUNE_RECORD_A\" }"; else dep=$C_DEP; fi
  cat >"$LIB/Cargo.toml" <<TOML
[package]
name = "libcheck"
version = "0.0.0"
edition = "2021"

[dependencies]
rune-record = $dep
serde_json = "1"

[workspace]
TOML
  # Seed with tm-weather's lock so third-party versions match the wx-receipt builds.
  cp "$TMW/Cargo.lock" "$LIB/Cargo.lock"
  (cd "$LIB" && CARGO_TARGET_DIR=$TMP/libtarget-$v cargo build --release --offline)
  "$TMP/libtarget-$v/release/libcheck" "$V1/receipts" >"$OUT/lib-$v.txt"
done

# ── 6. summary ───────────────────────────────────────────────────────────────
echo "== summary =="
echo "mirror: $got files, $receipts receipts, generated_at $generated_at"
echo "tm-weather: $TMW_HEAD  C-source: $C_SOURCE"
for v in C A; do
  echo "$v: $(tail -1 "$OUT/$v.txt"), $(grep -c '^FAIL' "$OUT/$v.txt" || true) FAIL lines;" \
    "lib-$v: $(grep -c ' ok$' "$OUT/lib-$v.txt" || true) ok, $(grep -c ' err ' "$OUT/lib-$v.txt" || true) err"
done
echo "outputs in $OUT"
