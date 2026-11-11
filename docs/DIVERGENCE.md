> Superseded by [docs/MIRROR.md](MIRROR.md) (2026-11-10).
# rune-record: what differs between A, B and C

Measured 2026-11-04 for spec 125 (D4, task T001). This file records facts and the
commands behind them. It does not decide anything. GATE 1 reads the conclusion at the end.

| Tree | Source | Commit |
|---|---|---|
| A | this repo, `TunnelMind/rune-record` | `f386e8f` (working tree at measurement) |
| B | `~/rune` `crates/rune-record` | `91652a7af041e712469637349ae4e31821cfc1df` (2026-10-22) |
| C | `~/rune` `crates/rune-record` | `63022824c8a0e9a7afb64080f5994573e75bb821` (2026-10-07), pinned by tm-weather |

C is an ancestor of B (`git -C ~/rune merge-base --is-ancestor 6302282 91652a7` exits 0).
`git -C ~/rune rev-list --count 6302282..91652a7 -- crates/rune-record` prints `1`, so one
commit touched the crate between C and B.

## How to read the tables

- **A / B / C** says whether the function exists in that tree.
- **Differs how** compares the trees after comments and blank lines are stripped (see
  Reproduce). It can say *identical code*, *comments only*, *test only* or *code*.
- **Changes signed bytes** answers this question: for the same input and the same key, would
  this tree's code put different bytes under an Ed25519 signature than A's code? That covers
  the canonical JCS bytes, `payload_hash`, the ADR-001 `id`, and the key that gets loaded.
- **Changes verification outcome** answers this question: for the same receipt and the same
  key, would a verify call in this tree return a different ok, `Err` or panic than A's?
- When a cell says "A vs C: yes", the difference is between A (or B) and C. A and B never
  differ in production code (see the diff output below).

## Reproduce

Run everything from the root of A. Nothing is written into A or into `~/rune`.

### 1. Extract B and C

```sh
B=$(mktemp -d); C=$(mktemp -d)
git -C ~/rune archive 91652a7 crates/rune-record | tar -x -C "$B"
git -C ~/rune archive 6302282 crates/rune-record | tar -x -C "$C"
B=$B/crates/rune-record; C=$C/crates/rune-record
```

### 2. The vector file is the same in all three trees

```sh
sha256sum tests/vectors/crawl_record_v1.json $B/tests/vectors/crawl_record_v1.json $C/tests/vectors/crawl_record_v1.json
```

Output, 2026-11-04 (the tmp paths will differ on a rerun):

```
89c759f21f6f87b83ea921df4664bfe855f72869b473657b4920d712c07bb79b  tests/vectors/crawl_record_v1.json
89c759f21f6f87b83ea921df4664bfe855f72869b473657b4920d712c07bb79b  /tmp/divB.O9Hq/crates/rune-record/tests/vectors/crawl_record_v1.json
89c759f21f6f87b83ea921df4664bfe855f72869b473657b4920d712c07bb79b  /tmp/divC.DFmt/crates/rune-record/tests/vectors/crawl_record_v1.json
```

Only A has `tests/jcs_vectors.rs` and `tests/vectors/jcs.json`
(sha256 `81a79410874c5ed24ea0468a3d684e3a03111dc0a2fd2e8317ed3e3dfb978cdd`). Neither B nor C
has a `tests/*.rs`. Their only file under `tests/` is the crawl vector.

### 3. Strip comments and split production code from tests

Save this as `split.sh` outside the repo, for example in `T=$(mktemp -d)`.

```sh
#!/bin/sh
# usage: split.sh <crate-root> <out-dir>
# Every src/**/*.rs -> <out>/prod/<f> (lines above the `#[cfg(test)]` that
# precedes `mod tests {`) and <out>/test/<f> (that line to EOF). Both drop
# whole-line // comments (incl. //! and ///), trailing ` // ...` comments with
# no quote after them, and blank lines.
set -e
cd "$1"
find src -name '*.rs' | sort | while read -r f; do
  mkdir -p "$2/prod/$(dirname "$f")" "$2/test/$(dirname "$f")"
  n=$(grep -n '^mod tests {' "$f" | head -1 | cut -d: -f1)
  if [ -z "$n" ]; then cp "$f" "$2/p.tmp"; : > "$2/t.tmp"
  else head -n $((n-2)) "$f" > "$2/p.tmp"; tail -n +$((n-1)) "$f" > "$2/t.tmp"; fi
  for k in p:prod t:test; do
    sed -E -e '/^[[:space:]]*\/\//d' -e 's@[[:space:]]+//[^"]*$@@' -e '/^[[:space:]]*$/d' \
      "$2/${k%%:*}.tmp" > "$2/${k#*:}/$f"
  done
done
rm -f "$2/p.tmp" "$2/t.tmp"
```

The split point is the `#[cfg(test)]` line directly above `mod tests {`. Anything above it
counts as production code. `src/receipt_v1/conformance.rs` has no `mod tests`, so the script
puts all of it under `prod/`. The whole module is still test code: each tree's
`receipt_v1.rs` declares it as `#[cfg(test)] mod conformance;`. This doc lists it in the
tests table.

```sh
S=$(mktemp -d)
for t in A:$PWD B:$B C:$C; do sh $T/split.sh "${t#*:}" "$S/${t%%:*}"; done
diff -r $S/A/prod $S/B/prod   # A vs B, production
diff -r $S/A/prod $S/C/prod   # A vs C, production
diff -r $S/B/prod $S/C/prod   # B vs C, production
diff -r $S/A/test $S/B/test; diff -r $S/A/test $S/C/test; diff -r $S/B/test $S/C/test
diff -r src $B/src; diff -r src $C/src   # raw, to see which differences are comments only
```

### 4. Production diff output (comments stripped)

**A vs B**: the only difference is the feature gate.

```
diff -r /tmp/divS/A/prod/src/lib.rs /tmp/divS/B/prod/src/lib.rs
1d0
< #[cfg(feature = "manifest")]
```

**B vs C** (A vs C is the same plus the `lib.rs` line above), in full except for the
`conformance.rs` hunks, which are test code and appear in the tests section:

```
diff -r /tmp/divS/B/prod/src/manifest.rs /tmp/divS/C/prod/src/manifest.rs
4c4
< use crate::receipt::{sha256_hex, tiers_for_prefix, TIERS};
---
> use crate::receipt::{tiers_for_prefix, TIERS};
93c93
<     sha256_hex(out.as_bytes())
---
>     hex::encode(ring::digest::digest(&ring::digest::SHA256, out.as_bytes()).as_ref())
diff -r /tmp/divS/B/prod/src/receipt.rs /tmp/divS/C/prod/src/receipt.rs
23c23
< pub fn jcs_bytes(v: &Value) -> Result<Vec<u8>> {
---
> fn jcs(v: &Value) -> Result<Vec<u8>> {
26c26
< pub fn sha256_hex(bytes: &[u8]) -> String {
---
> fn sha256_hex(bytes: &[u8]) -> String {
30c30,33
<     Ok(format!("sha256:{}", sha256_hex(&jcs_bytes(v)?)))
---
>     Ok(format!("sha256:{}", sha256_hex(&jcs(v)?)))
> }
> pub fn jcs_bytes(v: &Value) -> Result<Vec<u8>> {
>     jcs(v)
124c127
<     let hex8 = &sha256_hex(&jcs_bytes(&receipt)?)[..8];
---
>     let hex8 = &sha256_hex(&jcs(&receipt)?)[..8];
126c129
<     let sig = signer.sign(&jcs_bytes(&receipt)?);
---
>     let sig = signer.sign(&jcs(&receipt)?);
133,136d135
<     let alg = str_at(receipt, &["alg"]).unwrap();
<     if alg != "ed25519" {
<         bail!("cannot verify alg '{alg}': only ed25519 is implemented");
<     }
139c138
<         if pubkey_hex.get(..16) != Some(fp) {
---
>         if pubkey_hex.len() < 16 || fp != &pubkey_hex[..16] {
149c148
<     verify_ed25519(pubkey_hex, &jcs_bytes(&unsigned)?, &sig)?;
---
>     verify_ed25519(pubkey_hex, &jcs(&unsigned)?, &sig)?;
152c151
<     let hex8 = &sha256_hex(&jcs_bytes(&unsigned)?)[..8];
---
>     let hex8 = &sha256_hex(&jcs(&unsigned)?)[..8];
diff -r /tmp/divS/B/prod/src/receipt_v1.rs /tmp/divS/C/prod/src/receipt_v1.rs
5c5
< use crate::receipt::{iso8601_now_ms, jcs_bytes, sha256_hex};
---
> use crate::receipt::{iso8601_now_ms, jcs_bytes};
29c29,32
<     format!("0x{}", sha256_hex(bytes))
---
>     format!(
>         "0x{}",
>         hex::encode(ring::digest::digest(&ring::digest::SHA256, bytes).as_ref())
>     )
31c34
< pub fn check_numbers(v: &Value) -> Result<()> {
---
> fn check_numbers(v: &Value) -> Result<()> {
diff -r /tmp/divS/B/prod/src/signing.rs /tmp/divS/C/prod/src/signing.rs
10,31c10,31
<         if key_path.exists() {
<             return Self::load(key_path);
<         }
<         let rng = ring::rand::SystemRandom::new();
<         let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng)
<             .map_err(|_| anyhow::anyhow!("Ed25519 key generation failed"))?;
<         if let Some(parent) = key_path.parent() {
<             std::fs::create_dir_all(parent)?;
<         }
<         write_new_0600(key_path, hex::encode(pkcs8.as_ref()).as_bytes())
<             .with_context(|| format!("writing key file {}", key_path.display()))?;
<         Self::from_pkcs8(key_path, pkcs8.as_ref())
<     }
<     pub fn load(key_path: &Path) -> Result<Self> {
<         let hx = std::fs::read_to_string(key_path)
<             .with_context(|| format!("reading key file {}", key_path.display()))?;
<         let bytes = hex::decode(hx.trim()).context("decoding key file hex")?;
<         Self::from_pkcs8(key_path, &bytes)
<     }
<     fn from_pkcs8(key_path: &Path, pkcs8_bytes: &[u8]) -> Result<Self> {
<         let keypair = Ed25519KeyPair::from_pkcs8(pkcs8_bytes)
<             .map_err(|e| anyhow::anyhow!("{}: not an Ed25519 PKCS8 key: {e}", key_path.display()))?;
---
>         let pkcs8_bytes = if key_path.exists() {
>             let hx = std::fs::read_to_string(key_path)
>                 .with_context(|| format!("reading key file {}", key_path.display()))?;
>             hex::decode(hx.trim()).context("decoding key file hex")?
>         } else {
>             let rng = ring::rand::SystemRandom::new();
>             let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng)
>                 .map_err(|_| anyhow::anyhow!("Ed25519 key generation failed"))?;
>             let bytes = pkcs8.as_ref().to_vec();
>             if let Some(parent) = key_path.parent() {
>                 std::fs::create_dir_all(parent)?;
>             }
>             std::fs::write(key_path, hex::encode(&bytes))?;
>             #[cfg(unix)]
>             {
>                 use std::os::unix::fs::PermissionsExt;
>                 std::fs::set_permissions(key_path, std::fs::Permissions::from_mode(0o600))?;
>             }
>             bytes
>         };
>         let keypair = Ed25519KeyPair::from_pkcs8(&pkcs8_bytes)
>             .map_err(|_| anyhow::anyhow!("failed to load Ed25519 keypair from PKCS8"))?;
33,36c33
<         let pub_path = key_path.with_extension("pub");
<         if let Err(e) = std::fs::write(&pub_path, &pubkey_hex) {
<             eprintln!("warning: could not write {}: {e}", pub_path.display());
<         }
---
>         let _ = std::fs::write(key_path.with_extension("pub"), &pubkey_hex);
57,67d53
< }
< pub fn write_new_0600(path: &Path, contents: &[u8]) -> std::io::Result<()> {
<     use std::io::Write;
<     let mut opts = std::fs::OpenOptions::new();
<     opts.write(true).create_new(true);
<     #[cfg(unix)]
<     {
<         use std::os::unix::fs::OpenOptionsExt;
<         opts.mode(0o600);
<     }
<     opts.open(path)?.write_all(contents)
```

In both A and B, `sha256_hex` is `hex::encode(ring::digest::digest(&ring::digest::SHA256, bytes).as_ref())`
(`src/receipt.rs:40-42` in A). C inlines that same expression at the two call sites shown
above.

### 5. Scratch build: A's vectors and a cross-tree probe, run against A, B and C

`xcheck.sh` copies A and extracts B and C into one new `mktemp -d`. In that scratch tree it
does four things:

1. Runs each tree's own `cargo test`.
2. Runs A's `tests/jcs_vectors.rs` with `tests/vectors/jcs.json` against B's and C's source.
   It appends A's `raw_value` dev-dependency line, and for C it computes `sha256_hex`
   locally because C keeps that function private.
3. Has every tree sign three Receipt v1 receipts with one shared key.
4. Has every tree run the same probe (`xbytes.rs`) over all nine receipts, the crawl
   vector and 5000 generated JSON values.

It also extracts `manifests/scry-rune-001.toml` because B's and C's manifest tests
`include_str!("../../../manifests/…")`. Save both files side by side outside the repo and
run `sh xcheck.sh "$PWD"` from A's root.

`xcheck.sh`:

```sh
#!/bin/sh
# usage: xcheck.sh <A-repo-root>   (xbytes.rs sits next to this script)
# Builds scratch copies of A, B (91652a7), C (6302282) under one mktemp dir,
# runs each tree's own tests, A's JCS vector suite against B and C, and the
# xbytes probe in every tree. Never writes into A or ~/rune.
set -e
A=$1; X=$(mktemp -d /tmp/divX.XXXX); echo "scratch: $X"
mkdir -p $X/A $X/B $X/C
(cd "$A" && tar -c Cargo.toml src tests) | tar -x -C $X/A
git -C ~/rune archive 91652a7 crates/rune-record manifests/scry-rune-001.toml | tar -x -C $X/B
git -C ~/rune archive 6302282 crates/rune-record manifests/scry-rune-001.toml | tar -x -C $X/C
for t in B C; do
  d=$X/$t/crates/rune-record
  cp "$A/tests/jcs_vectors.rs" $d/tests/; cp "$A/tests/vectors/jcs.json" $d/tests/vectors/
  # A's dev-dep line for raw_value (needed by jcs_vectors.rs); test-only.
  printf 'serde_json = { version = "1", features = ["raw_value"] }\n' >> $d/Cargo.toml
done
# C keeps sha256_hex private; compute the same digest via ring (C's own spelling).
sed -i -e 's/use rune_record::receipt::{jcs_bytes, sha256_hex};/use rune_record::receipt::jcs_bytes;\nfn sha256_hex(b: \&[u8]) -> String { hex::encode(ring::digest::digest(\&ring::digest::SHA256, b)) }/' \
  $X/C/crates/rune-record/tests/jcs_vectors.rs
for t in A B C; do
  d=$X/$t; [ $t = A ] || d=$X/$t/crates/rune-record
  mkdir -p $d/examples; cp "$(dirname "$0")/xbytes.rs" $d/examples/
done
export CARGO_TARGET_DIR=$X/target
cp "$A/tests/vectors/crawl_record_v1.json" $X/crawl.json
# Every tree signs three Receipt v1 receipts with the same key; every tree then
# verifies all nine (r0-r2 from A, r3-r5 from B, r6-r8 from C).
for t in A B C; do
  d=$X/$t; [ $t = A ] || d=$X/$t/crates/rune-record
  (cd $d && cargo run -q --example xbytes -- make $X/key $X/v1-$t.json)
done
python3 -I -c 'import json,sys; print(json.dumps(sum((json.load(open(f)) for f in sys.argv[1:]), [])))' \
  $X/v1-A.json $X/v1-B.json $X/v1-C.json > $X/v1.json
for t in A B C; do
  d=$X/$t; [ $t = A ] || d=$X/$t/crates/rune-record
  echo "===== $t: cargo test (own suite + jcs_vectors)"
  (cd $d && cargo test --no-fail-fast 2>&1 | grep -E "^test result|FAILED|panicked|^     Running|^running|^error|not present" ) || true
  (cd $d && cargo run -q --example xbytes -- check $X/key $X/v1.json $X/crawl.json > $X/$t.out)
done
echo "===== probe output (A)"; cat $X/A.out
echo "===== diff A.out B.out"; diff $X/A.out $X/B.out && echo identical
echo "===== diff A.out C.out"; diff $X/A.out $X/C.out && echo identical
```

`xbytes.rs`:

```rust
// Cross-tree probe: dropped into examples/ of each tree's scratch copy.
// Uses only API that is public in A, B and C.
//   xbytes make <key> <out.json>   sign receipt_v1 receipts (A only)
//   xbytes check <key> <in.json> <crawl_record_v1.json>
use rune_record::receipt::{self, jcs_bytes};
use rune_record::receipt_v1::{self, Options, Source};
use rune_record::signing::RuneSigner;
use serde_json::{json, Value};
use std::path::Path;

fn sha(b: &[u8]) -> String { hex::encode(ring::digest::digest(&ring::digest::SHA256, b)) }

// Same xorshift + value shape as B's jcs_is_a_fixed_point_on_random_values.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 { self.0 ^= self.0 >> 12; self.0 ^= self.0 << 25; self.0 ^= self.0 >> 27; self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) }
}
fn random_value(rng: &mut Rng, depth: u32) -> Value {
    const P: &[&str] = &["a","Z","0"," ","\"","\\","/","\n","\t","\u{0}","\u{1f}","\u{7f}","é","\u{301}","\u{2028}","\u{e000}","\u{ffff}","😀","\u{10ffff}","中"];
    let s = |rng: &mut Rng| -> String { (0..rng.next() % 6).map(|_| P[(rng.next() % P.len() as u64) as usize]).collect() };
    match (depth, rng.next() % 8) {
        (_, 0) => Value::Null,
        (_, 1) => Value::Bool(rng.next() & 1 == 0),
        (_, 2) => { let m = (rng.next() % (1 << 53)) as i64; json!(if rng.next() & 1 == 0 { m } else { -m }) }
        (_, 7) => json!((rng.next() as f64) / 7.0), // floats too: JCS number formatting
        (0, _) | (_, 3) => Value::String(s(rng)),
        (_, 4) => (0..rng.next() % 4).map(|_| random_value(rng, depth - 1)).collect(),
        _ => Value::Object((0..rng.next() % 5).map(|_| (s(rng), random_value(rng, depth - 1))).collect()),
    }
}
fn outcome<T, E: std::fmt::Display>(f: impl FnOnce() -> Result<T, E> + std::panic::UnwindSafe) -> String {
    match std::panic::catch_unwind(f) { Ok(Ok(_)) => "ok".into(), Ok(Err(e)) => format!("err: {e}"), Err(_) => "PANIC".into() }
}
fn source() -> Source { Source { lens: "weather".into(), endpoint: "x".into(), node_id: "n".into() } }

fn main() {
    std::panic::set_hook(Box::new(|_| {}));
    let a: Vec<String> = std::env::args().collect();
    let s = RuneSigner::load_or_generate(Path::new(&a[2])).unwrap();
    if a[1] == "make" {
        let mut out = vec![];
        let mut prev: Option<String> = None;
        for (i, p) in [json!({"ok": true}), json!({"t": "é😀\u{e000}", "n": [1, -2, 9007199254740991i64]}), json!({})].into_iter().enumerate() {
            let r = receipt_v1::sign(p, &s, source(), Options { sequence: i as u64, previous_signature: prev.clone(),
                subject: Some(json!({"station": "KNYC"})), extensions: Some(json!({"z": 1, "a": "b"})), ..Default::default() }).unwrap();
            prev = r["signature"]["value"].as_str().map(str::to_string);
            out.push(r);
        }
        std::fs::write(&a[3], serde_json::to_vec(&out).unwrap()).unwrap();
        return;
    }
    // 1. JCS bytes on 5000 generated values (sha256 of each, or err).
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut all = Vec::new();
    for _ in 0..5000 {
        let v = random_value(&mut rng, 4);
        all.extend(match jcs_bytes(&v) { Ok(b) => sha(&b), Err(e) => format!("err {e}") }.into_bytes());
        all.push(b'\n');
    }
    println!("jcs 5000 values: sha256 of per-value digests = {}", sha(&all));
    // 2. The committed crawl-record vector.
    let crawl: Value = serde_json::from_slice(&std::fs::read(&a[4]).unwrap()).unwrap();
    let cr = crawl["receipt"].clone();
    println!("crawl vector receipt jcs sha256 = {}", sha(&jcs_bytes(&cr).unwrap()));
    println!("crawl vector receipt_v1::verify = {}", outcome(move || receipt_v1::verify(&cr)));
    // 3. ADR-001 build_and_sign with a fixed timestamp is deterministic (Ed25519 is).
    let attest = |extra: Option<Value>| receipt::build_and_sign(&s, "software", "scry-rune-001", "sha256:abc",
        json!({"type": "boot", "result": "ok", "timestamp": "2026-11-04T00:00:00.000Z"}), extra.map(|e| e.as_object().unwrap().clone())).unwrap();
    let r = attest(None);
    println!("build_and_sign sha256 = {}", sha(&serde_json::to_vec(&r).unwrap()));
    let pk = s.pubkey_hex().to_string();
    println!("receipt::verify good = {}", outcome(|| receipt::verify(&r, &pk)));
    let relabelled = attest(Some(json!({"alg": "ecdsa-p256"})));
    println!("receipt::verify alg=ecdsa-p256 (validly signed) = {}", outcome(|| receipt::verify(&relabelled, &pk)));
    for bad in ["", "abc", "€€€€€€"] { let r = r.clone(); println!("receipt::verify pubkey {bad:?} = {}", outcome(move || receipt::verify(&r, bad))); }
    // 4. receipt_v1::verify on receipts A signed, and on mutations of them.
    let rs: Vec<Value> = serde_json::from_slice(&std::fs::read(&a[3]).unwrap()).unwrap();
    for (i, r) in rs.iter().enumerate() {
        let mut cases: Vec<(&str, Value)> = vec![("as signed", r.clone())];
        let mut m = r.clone(); m["payload"]["x"] = json!(1); cases.push(("payload edited", m));
        let mut m = r.clone(); m["extensions"]["z"] = json!(2); cases.push(("extensions edited", m));
        let mut m = r.clone(); m["chain"]["sequence"] = json!(99); cases.push(("sequence edited", m));
        let mut m = r.clone(); m["signature"]["key_id"] = json!("other"); cases.push(("key_id edited", m));
        let mut m = r.clone(); let v = m["signature"]["value"].as_str().unwrap().to_string(); m["signature"]["value"] = json!(format!("{v}\n")); cases.push(("value + \\n", m));
        let mut m = r.clone(); m["signature"]["algorithm"] = json!("RSA"); cases.push(("algorithm RSA", m));
        let mut m = r.clone(); m["receipt_version"] = json!("2.0"); cases.push(("version 2.0", m));
        let mut m = r.clone(); m.as_object_mut().unwrap().remove("payload_hash"); cases.push(("no payload_hash", m));
        for (name, c) in cases { println!("receipt_v1::verify r{i} {name} = {}", outcome(move || receipt_v1::verify(&c))); }
    }
    for c in [json!(null), json!([]), json!(1)] { let n = c.to_string(); println!("receipt_v1::verify {n} = {}", outcome(move || receipt_v1::verify(&c))); }
}
```

Output, 2026-11-04 (scratch dir `/tmp/divX.hnMG`). The script exits 1 because its last
`diff` (A vs C) finds a difference. That result is reported below, not hidden.

```
===== A: cargo test (own suite + jcs_vectors)
running 30 tests
test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.12s
     Running tests/jcs_vectors.rs
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
===== B: cargo test (own suite + jcs_vectors)
running 36 tests
test result: ok. 36 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.29s
     Running tests/jcs_vectors.rs
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
===== C: cargo test (own suite + jcs_vectors)
running 24 tests
test receipt_v1::conformance::crawl_record_vector_verifies_in_js ... FAILED
test receipt_v1::conformance::rune_signed_verifies_in_js ... FAILED
assertion `left == right` failed: {"errors":["signature.key_id tm-crawl-fe812c12 not present in key bundle"],"valid":false}
assertion `left == right` failed: {"errors":["signature.key_id rune-21b7f403 not present in key bundle"],"valid":false}
test result: FAILED. 22 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.83s
     Running tests/jcs_vectors.rs
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
===== probe output (A), first 9 of 93 lines
jcs 5000 values: sha256 of per-value digests = e20547c01f2bc9e79492a0f8fe0b1c52c18037e7fc1f8cfc832d3cebf94c3aea
crawl vector receipt jcs sha256 = 2ba3eac05ffb7eb97ce019e380f5349a37fe1fe800b5c1e81f9680c45fd4b201
crawl vector receipt_v1::verify = ok
build_and_sign sha256 = e8179d1fd5765eb94ea11aeabb9d3e1e95185de692139ab8f7a7085ec10bb4c8
receipt::verify good = ok
receipt::verify alg=ecdsa-p256 (validly signed) = err: cannot verify alg 'ecdsa-p256': only ed25519 is implemented
receipt::verify pubkey "" = err: signed_by fingerprint does not match supplied pubkey
receipt::verify pubkey "abc" = err: signed_by fingerprint does not match supplied pubkey
receipt::verify pubkey "€€€€€€" = err: signed_by fingerprint does not match supplied pubkey
===== diff A.out B.out
identical
===== diff A.out C.out
6c6
< receipt::verify alg=ecdsa-p256 (validly signed) = err: cannot verify alg 'ecdsa-p256': only ed25519 is implemented
---
> receipt::verify alg=ecdsa-p256 (validly signed) = ok
9c9
< receipt::verify pubkey "€€€€€€" = err: signed_by fingerprint does not match supplied pubkey
---
> receipt::verify pubkey "€€€€€€" = PANIC
```

What the probe output measures:

- The 9 Receipt v1 receipts (3 signed in each tree) give `as signed = ok` in every tree:
  `grep -c 'as signed = ok'` prints 9 for each of A.out, B.out and C.out. Each tree also
  gives the same ok or `Err` text on 8 mutations of every receipt and on 3 non-objects. Lines
  10–93 match across all three outputs.
- `build_and_sign sha256` hashes a whole ADR-001 record built with a fixed timestamp and one
  key. Ed25519 is deterministic, so that hash covers the signature bytes too. It is the same
  in A, B and C. The value changes between runs because each run generates a new key.
- The two A-vs-C lines are both in the legacy ADR-001 `receipt::verify`. Neither is in
  Receipt v1.
- C's two failing tests are its JS interop harness. C's script calls
  `verifyReceipt(receipt, { noFetchKeys: true, noFetchRevocations: true })` without a key
  bundle. The `~/receipt-verify` installed on BEAST (2.0.0) refuses that call with "not
  present in key bundle". This is drift between C's test and today's JS verifier. It is not
  a difference in rune-record's bytes: in the same run, C's own `receipt_v1::verify` accepts
  the crawl vector (`crawl_record_vector_verifies` passed, and the probe printed
  `crawl vector receipt_v1::verify = ok` for C).
- A's own interop tests ran rather than skipped on this host. `cargo test -- --nocapture conformance`
  in A printed no skip notice and `test result: ok. 4 passed`.

## Production code

| Function (module) | A | B | C | Differs how | Changes signed bytes | Changes verification outcome | Evidence |
|---|---|---|---|---|---|---|---|
| `lib.rs`: `pub mod manifest` gate | `#[cfg(feature = "manifest")]` | ungated | ungated | code (A only), comments (all three `//!` headers differ) | no | no | §4 A-vs-B hunk `< #[cfg(feature = "manifest")]` is the only production difference. It decides whether `manifest` compiles and touches no signing path. `cargo test --no-default-features` passes in A (see Verification). |
| `Cargo.toml` features and deps | `default = ["manifest"]`, `manifest = ["dep:toml"]`, `toml` optional; `time = "0.3"`; dev-dep `serde_json` `raw_value`; license Apache-2.0 | no features, `toml` required, `time = "0.3"`, license UNLICENSED | as B, but `time = { version = "0.3", features = ["formatting"] }` | build config | no | no | `iso8601_now_ms` has identical code in all three: it formats by hand with `format!` and does not use `time`'s `formatting` feature (no hunk in §4). The `toml` crate is used only by `manifest::load`. `raw_value` is dev-only (used by `tests/jcs_vectors.rs`). |
| `receipt::tiers_for_prefix` | yes | yes | yes | comments only (B has a longer `///`) | no | no | No hunk in §4 for A vs B or A vs C. B's test `prefix_tier_table` passes in B (§5). |
| `receipt::jcs_bytes` | yes (`pub`) | yes (`pub`) | yes (`pub` wrapper calling private `jcs`) | code: C's body moved into `jcs`, then forwarded; the call is the same | no | no | §4 hunk `23c23` / `30c30,33`. A's `jcs_vectors` (6 matched, 3 refused) passes against A, B and C (§5). The 5000-value digest `e20547c0…` and the crawl-receipt JCS digest `2ba3eac0…` are identical in all three (§5, `diff A.out B.out` identical; `diff A.out C.out` shows no `jcs` line). |
| `receipt::jcs` (C only) | no | no | yes (private) | code: renamed to `jcs_bytes` in B | no | no | Same body as `jcs_bytes` (§4 shows only the signature line changed). Same evidence as the `jcs_bytes` row. |
| `receipt::sha256_hex` | yes (`pub`) | yes (`pub`) | yes (private) | visibility only | no | no | §4 hunk `26c26`: same body, only the `pub` differs. |
| `receipt::sha256_jcs` | yes | yes | yes | code: calls `jcs` in C, `jcs_bytes` in A/B | no | no | §4 hunk `30c30,33` (one call renamed, same function). |
| `receipt::iso8601_now_ms` | yes | yes | yes | identical code | no | no | No hunk in §4. |
| `receipt::str_at` | yes | yes | yes | identical code (B adds `///`) | no | no | No hunk in §4. |
| `receipt::check_required` | yes | yes | yes | identical code (B adds `///`) | no | no | No hunk in §4. |
| `receipt::check_tier_prefix` | yes | yes | yes | identical code (B adds `///`) | no | no | No hunk in §4. |
| `receipt::build_and_sign` | yes | yes | yes | code: `jcs` vs `jcs_bytes` (same function) | no | no | §4 hunks `124c127`, `126c129`. In §5 every tree's `build_and_sign` hash is the same for one key and a fixed timestamp (`diff A.out C.out` has no `build_and_sign` line). |
| `receipt::verify` (ADR-001) | yes | yes | yes | **code**: A/B refuse `alg != "ed25519"`, and A/B use `pubkey_hex.get(..16)` where C slices `&pubkey_hex[..16]` | no (it signs nothing) | **A vs B: no. A vs C: yes.** C accepts a validly Ed25519-signed record whose `alg` says `ecdsa-p256`, where A returns `Err`. C panics on the caller-supplied pubkey `"€€€€€€"`, where A returns `Err`. | §4 hunks `133,136d135` and `139c138`. §5 `diff A.out C.out` lines 6 and 9. Tests: `mislabelled_alg_does_not_verify`, `wrong_pubkey_fails_fingerprint_check` (A, B). |
| `receipt_v1::sha256_0x` | yes | yes | yes | code: `sha256_hex(bytes)` vs the same `hex::encode(ring…SHA256…)` inline | no | no | §4 hunk `29c29,32` and the `sha256_hex` body quoted under §4. In §5, receipts signed in each tree (their `payload_hash` is `sha256_0x`) verify in every tree: 9 × `as signed = ok`. |
| `receipt_v1::check_numbers` | yes (`pub`) | yes (`pub`) | yes (private) | visibility only | no | no | §4 hunk `31c34`: same body. |
| `receipt_v1::signing_bytes` | yes (private) | yes | yes | identical code | no | no | No hunk in §4. In §5, receipts signed by A, B and C verify in all three (9 × `as signed = ok` per tree). Test `signing_bytes_match_crawl_record_jcs` passes in A, B and C (§5: C fails only its 2 `_in_js` tests). |
| `receipt_v1::sign` | yes | yes | yes | identical code (comments differ) | no | no | No hunk in §4 inside `sign`. §5 cross-verification as above. |
| `receipt_v1::from_attest` | yes | yes | yes | identical code | no | no | No hunk in §4. Tests `from_attest_wraps_unchanged` and `from_attest_rejects_non_attest` pass in all three (§5). |
| `receipt_v1::verify` | yes | yes | yes | identical code (B adds trust-boundary `///`) | no (it signs nothing) | no | No hunk in §4. §5 lines 10–93 (9 receipts × 9 cases + 3 non-objects) are identical across A.out, B.out and C.out. |
| `manifest::manifest_hash` | yes (feature `manifest`) | yes | yes | code: `sha256_hex(…)` vs the same expression inline | no | no | §4 hunk `93c93`: same digest expression. Test `stamped_hash_survives_reload` passes in all three (§5). |
| `manifest::load` | yes (feature `manifest`) | yes | yes | identical code (B's module `//!` is longer) | no | no | No hunk in §4 besides the `use` line. |
| `manifest::stamp_hash` | yes (feature `manifest`) | yes | yes | identical code (B's `///` adds "Idempotent…") | no | no | No hunk in §4. |
| `signing::RuneSigner::load_or_generate` | yes | yes | yes | **code**: A/B route an existing file to `load` and create a new key with `write_new_0600` (`create_new`, mode 0600 at open). C does `std::fs::write` then `set_permissions(0o600)`, which leaves a window and overwrites a file that appeared since the check. | no | no | Same key file format in all three: one line of hex PKCS#8, read with `hex::decode(hx.trim())` (§4 hunk `10,31c10,31`). In §5 one key file, created by A, was loaded by A, B and C; all signed receipts verify everywhere and `build_and_sign` hashes match. The difference is file-creation safety, not the bytes signed. |
| `signing::RuneSigner::load` | yes | yes | **no** | code (C has no load-only path) | no | no | §4 hunk `10,31c10,31`. Test `load_never_generates_and_names_a_bad_key` (A, B). |
| `signing::RuneSigner::from_pkcs8` | yes (private) | yes | **no** (inline in `load_or_generate`) | code: error text names the path in A/B; a `.pub` write failure warns on stderr in A/B and is silent in C | no | no | §4 hunks `10,31c10,31` and `33,36c33`. Same `Ed25519KeyPair::from_pkcs8` call; only the error message differs. |
| `signing::RuneSigner::sign` | yes | yes | yes | identical code | no | no | No hunk in §4. §5 `build_and_sign` hash identical. |
| `signing::RuneSigner::pubkey_hex` | yes | yes | yes | identical code | no | no | No hunk in §4. |
| `signing::RuneSigner::signed_by` | yes | yes | yes | identical code | no | no | No hunk in §4. |
| `signing::verify_ed25519` | yes | yes | yes | identical code (B adds `///`) | no | no | No hunk in §4. |
| `signing::write_new_0600` | yes | yes | **no** | code (A/B only) | no | no | §4 hunk `57,67d53`. It writes key files only and never touches what is signed. Test `write_new_0600_refuses_an_existing_file` (A, B). |

## Tests

Nothing in this table compiles into the library. Each tree's `mod tests` sits under
`#[cfg(test)]`, at these lines:

- A: `manifest.rs:200`, `receipt.rs:205`, `signing.rs:98`, `receipt_v1.rs:192`; `conformance` at `receipt_v1.rs:441`.
- B: 215, 232, 124, 206; `conformance` at 465.
- C: 200, 199, none (C's `signing.rs` has no tests), 194; `conformance` at 387.

Read them with `grep -n 'cfg(test)' src/*.rs`. `tests/jcs_vectors.rs` is an integration
test target. So for every row, "changes signed bytes" is **no**, with this evidence: the
function is test-only (line above), and §4 shows the full production diff. "Changes
verification outcome" is **no** for the same reason. A test can only *observe* a
verification outcome. Where the test shows a production difference, the row says so.

| Function (module) | A | B | C | Differs how | Changes signed bytes | Changes verification outcome | Evidence |
|---|---|---|---|---|---|---|---|
| `manifest::tests::example_manifest_validates` | yes | yes | yes | test only: fixture path `../tests/fixtures/scry-rune-001.toml` (A) vs `../../../manifests/scry-rune-001.toml` (B, C) | no | no | test-only, `manifest.rs:200`; passes in all three (§5) |
| `manifest::tests::stamped_hash_survives_reload` | yes | yes | yes | identical after strip | no | no | test-only; passes in all three (§5) |
| `manifest::tests::stamping_is_idempotent_and_matches_the_hash_rule` | **no** | yes | no | test only (B) | no | no | test-only, B `manifest.rs:215`; passes in B (§5) |
| `manifest::tests::garbage_is_an_error` | **no** | yes | no | test only (B) | no | no | test-only; passes in B (§5) |
| `manifest::tests::non_negotiables_are_fatal` | yes | yes | yes | identical after strip | no | no | test-only; passes in all three |
| `manifest::tests::missing_degraded_mode_is_fatal` | yes | yes | yes | identical after strip | no | no | test-only; passes in all three |
| `manifest::tests::software_key_cannot_claim_tee_tpm_tier` | yes | yes | yes | identical after strip | no | no | test-only; passes in all three |
| `manifest::tests::unknown_section_warns_not_fails` | yes | yes | yes | identical after strip | no | no | test-only; passes in all three |
| `receipt::tests::signer` (helper) | yes | yes | yes | identical after strip | no | no | test-only, A `receipt.rs:205` |
| `receipt::tests::boot_receipt` (helper) | yes | yes | yes | identical after strip | no | no | test-only |
| `receipt::tests::Rng::next`, `Rng::pick`, `random_value` (helpers) | **no** | yes | no | test only (B) | no | no | test-only, B `receipt.rs:232`; the probe in §5 reuses the same generator, with floats added |
| `receipt::tests::jcs_is_a_fixed_point_on_random_values` | **no** | yes | no | test only (B) | no | no | test-only; passes in B (§5) |
| `receipt::tests::jcs_sorts_keys_by_utf16_and_lone_surrogates_never_parse` | **no** | yes | no | test only (B) | no | no | test-only; passes in B |
| `receipt::tests::sign_verify_roundtrip` | yes | yes | yes | identical after strip | no | no | test-only; passes in all three |
| `receipt::tests::tamper_is_detected` | yes | yes | yes | identical after strip | no | no | test-only; passes in all three |
| `receipt::tests::one_bit_or_one_field_off_does_not_verify` | **no** | yes | no | test only (B) | no | no | test-only; passes in B |
| `receipt::tests::prefix_tier_table` | **no** | yes | no | test only (B) | no | no | test-only; passes in B |
| `receipt::tests::software_key_cannot_claim_tee_tpm` | yes | yes | yes | identical after strip | no | no | test-only; passes in all three |
| `receipt::tests::wrong_pubkey_fails_fingerprint_check` | yes | yes | yes | test only: A/B add a loop over `""`, `"abc"`, `"€€€€€€"` and a 15-char key (`diff $S/A/test/src/receipt.rs $S/C/test/src/receipt.rs` → `63,81d62`) | no | no | test-only. It observes the production difference recorded in the `receipt::verify` row: C panics on `"€€€€€€"` (§5 line 9). |
| `receipt::tests::mislabelled_alg_does_not_verify` | yes | yes | **no** | test only (A, B) | no | no | test-only. It observes the `alg` check that C lacks (§5 line 6). |
| `receipt::tests::oracle_failure_still_signs` | yes | yes | yes | identical after strip | no | no | test-only; passes in all three |
| `receipt_v1::tests::signer`, `source`, `signed` (helpers) | yes | yes | yes | identical after strip | no | no | test-only, A `receipt_v1.rs:192` |
| `receipt_v1::tests::round_trip` | yes | yes | yes | identical after strip | no | no | test-only; passes in all three |
| `receipt_v1::tests::tampered_payload_fails` | yes | yes | yes | identical after strip | no | no | test-only; passes in all three |
| `receipt_v1::tests::tampered_signature_fails` | yes | yes | yes | identical after strip | no | no | test-only; passes in all three |
| `receipt_v1::tests::standard_engine_refuses_non_canonical_base64` | yes | **no** | **no** | test only (A) | no | no | test-only; `diff $S/A/test/src/receipt_v1.rs $S/B/test/src/receipt_v1.rs` → `86,137d85`. It pins base64 0.22's `STANDARD` engine, which all three trees use with the same code (no hunk in §4). |
| `receipt_v1::tests::respellings` (helper) | yes | no | no | test only (A) | no | no | test-only; same hunk `86,137d85` |
| `receipt_v1::tests::respelled_signature_fields_fail_verify` | yes | no | no | test only (A) | no | no | test-only; same hunk. B and C get the same outcome on a respelled `signature.value`: §5 case `value + \n` = `err: decoding signature.value base64` in all three. |
| `receipt_v1::tests::unknown_major_version_rejected` | yes | yes | yes | identical after strip | no | no | test-only; passes in all three |
| `receipt_v1::tests::unsafe_numbers_refused` | yes | yes | yes | identical after strip | no | no | test-only; passes in all three |
| `receipt_v1::tests::number_policy_boundaries` | **no** | yes | no | test only (B) | no | no | test-only, B `receipt_v1.rs:206`; passes in B |
| `receipt_v1::tests::malformed_envelopes_are_errors_not_panics` | **no** | yes | no | test only (B) | no | no | test-only; passes in B |
| `receipt_v1::tests::default_and_explicit_key_id` | yes | yes | yes | identical after strip | no | no | test-only; passes in all three |
| `receipt_v1::tests::chain_hash_from_previous_signature` | yes | yes | yes | test only: `want` via `sha256_hex` (A, B) vs the same expression inline (C) | no | no | test-only; B-vs-C test hunk `188c128,131`; passes in all three |
| `receipt_v1::tests::attest` (helper) | yes | yes | yes | identical after strip | no | no | test-only |
| `receipt_v1::tests::from_attest_wraps_unchanged` | yes | yes | yes | identical after strip | no | no | test-only; passes in all three |
| `receipt_v1::tests::from_attest_rejects_non_attest` | yes | yes | yes | identical after strip | no | no | test-only; passes in all three |
| `signing::tests::generated_key_is_private_from_creation_and_reloads` | yes | yes | **no** | test only (A = B) | no | no | test-only, A `signing.rs:98`; `diff $S/A/test/src/signing.rs $S/B/test/src/signing.rs` prints nothing |
| `signing::tests::load_never_generates_and_names_a_bad_key` | yes | yes | **no** | test only (A = B) | no | no | test-only; same diff, empty |
| `signing::tests::write_new_0600_refuses_an_existing_file` | yes | yes | **no** | test only (A = B) | no | no | test-only; same diff, empty |
| `receipt_v1::conformance::vector` (helper) | yes | yes | yes | identical | no | no | whole module `#[cfg(test)] mod conformance;` (A `receipt_v1.rs:441`) |
| `receipt_v1::conformance::verify_with_node` (helper) | yes | yes | yes | test only: A/B's node script pins a key bundle and an empty revocation feed and returns `{pinned, bare}`. C's calls `verifyReceipt` with no bundle. A vs B differ in `//!` comments only. | no | no | test-only; §5 shows C's script refused by receipt-verify 2.0.0 ("not present in key bundle") |
| `receipt_v1::conformance::rune_signed` (helper) | yes | yes | yes | identical | no | no | test-only |
| `receipt_v1::conformance::crawl_record_vector_verifies` | yes | yes | yes | identical | no | no | test-only; passes in all three (§5) |
| `receipt_v1::conformance::signing_bytes_match_crawl_record_jcs` | yes | yes | yes | identical | no | no | test-only; passes in all three (§5). The vector file has the same sha256 in all three (§2). |
| `receipt_v1::conformance::rune_signed_verifies_in_js` | yes | yes | yes | test only: asserts `pinned` valid and `bare` invalid (A, B) vs `valid` (C) | no | no | test-only; passes in A and B, **fails in C** against receipt-verify 2.0.0 (§5) |
| `receipt_v1::conformance::crawl_record_vector_verifies_in_js` | yes | yes | yes | test only: same assert change | no | no | test-only; passes in A and B, **fails in C** (§5) |
| `tests/jcs_vectors.rs`: `vendored_file_is_byte_identical`, `jcs_matches_every_vector` | yes | **no** | **no** | test only (A) | no | no | integration test; run against B's and C's source in §5: `2 passed` in each |

## Verification

A's own suites, run in the repo on 2026-11-04:

```
$ cargo test
test result: ok. 30 passed; 0 failed; ...   (lib)
test result: ok. 2 passed; 0 failed; ...    (tests/jcs_vectors.rs)
$ cargo test --no-default-features
test result: ok. 24 passed; 0 failed; ...   (lib, manifest compiled out)
test result: ok. 2 passed; 0 failed; ...    (tests/jcs_vectors.rs)
```

The scripts in §3 and §5 were copied back out of this file and re-run, producing the same
§4 diff and the same `diff A.out B.out` / `diff A.out C.out` result.

## Who signed the published receipts

**What the evidence shows**

- tm-weather pins C. At tm-weather HEAD `f4882f8`, the rev is
  `rev = "63022824c8a0e9a7afb64080f5994573e75bb821"` in **three** manifests:
  `wx-record/Cargo.toml:14`, `wx-run/Cargo.toml:44` and `wx-receipt/Cargo.toml:27`
  (`grep -rn 'rev = "6302282' --include=Cargo.toml ~/tm-weather`).
  `Cargo.lock:694-696` resolves `rune-record 0.1.0` to `…?rev=63022824…#63022824…`.
  `git -C ~/tm-weather log -p -- Cargo.lock` shows that source line added once, in
  `61e6201` (2026-10-27, the first commit of `Cargo.lock`), and never changed afterwards.
  So every tm-weather commit with a lockfile builds against C.
- The weather receipts are Receipt v1. tm-weather calls `receipt_v1::sign` /
  `receipt_v1::verify`, `receipt::jcs_bytes` and `RuneSigner::load_or_generate`
  (`wx-record/src/lib.rs:13-18`, `wx-receipt/src/lib.rs:22-24`, `wx-run/src/main.rs:250`).
  It does not call the ADR-001 `receipt::verify`, the one function whose outcome differs
  between A and C (`grep -rn 'rune_record::' ~/tm-weather --include=*.rs`).
- On that path, A, B and C produce the same signed bytes for the same input and key, and
  give the same verification outcome. Evidence:
  - The JCS digests and vector suite match (§5).
  - Receipts signed in each tree verify in every tree (§5: 9 × `as signed = ok` per tree).
  - The 84 Receipt v1 verify outcomes per tree (9 receipts × 9 cases, plus 3 non-objects) are identical (§5).
  - The production code of `signing_bytes`, `sign` and `receipt_v1::verify` is identical
    (§4).
  - The key-file format is the same in all three (§4, the `load_or_generate` row).

  So a published weather receipt that verifies under C also verifies under A, and the
  reverse holds too. Which tree "signed" it cannot be told apart from the bytes.

**What the evidence cannot show**

- Which build signed any particular published receipt. A Receipt v1 carries `key_id`,
  `public_key`, `timestamp` and the signature, but no crate version, git rev or build id
  (see the `json!` block in `receipt_v1::sign`). Nothing in a receipt records the signing
  binary.
- What binary the publishing host actually ran. This doc read `~/tm-weather` on BEAST. It
  did not inspect the host or the binary that produced the live log, or that binary's
  `Cargo.lock`. That is **unknown**.
- Whether any receipt was signed before `61e6201` (2026-10-27) or from a tree with no
  `Cargo.lock`. This is **unknown** from this evidence.
- Whether C's key-file race (write, then chmod) was ever hit on the signing host. This is
  **unknown**. It would affect key secrecy, not the signed bytes.
- The re-verify of the live archive under A and C is T006/T007 (`docs/REVERIFY.md`). This doc
  makes no claim about the published files themselves.

## Conclusion

**No row changes signed bytes.** For the same input and key, A, B and C sign the same bytes
(§4 identical signing code; §5 vectors, cross-verification and the deterministic
`build_and_sign` hash).

**One function changes a verification outcome, and only between A/B and C:**
`receipt::verify` (the legacy ADR-001 `attest:` verifier). A and B refuse a record whose
`alg` is not `ed25519`; C accepts it if the Ed25519 signature is valid. A and B return `Err`
for a pubkey with no char boundary at byte 16; C panics. In both cases A is the stricter
tree, and B already has the same behaviour.

A and B have **no** production difference except the `manifest` feature gate. Receipt v1
(`receipt_v1::*`), the format the weather log uses, has no difference in signed bytes or
verification outcome across A, B and C.
