# Re-verify of the published weather log — PASS

**PASS**: A (this tree) and C (the rev tm-weather pins, `6302282`) give the same
result on one mirror: same exit code, same FAIL lines, same library verdicts.
`A.txt` and `C.txt` are byte-identical.

"PASS" means *identical*, not *clean*. Both builds exit 1 with the same 27 FAIL lines,
as the live `verify.txt` does (`index.json` → `verify.exit: 1`). Those failures are
about the log's data (late schedules, missing commits, score recomputation,
source labels). They are not about rune-record, so they are the same under both trees.

Raw outputs: `docs/reverify/`. Run date: 2026-11-04.

## 1. Mirror

From `docs/reverify/mirror.txt`:

| field        | value |
|--------------|-------|
| base         | https://tunnelmind.ai/weather/log/v1 |
| generated_at | 2026-11-04T17:07:09.079Z |
| files        | 26315 (every path in `files.txt`, sha256 in `files.sha256`) |
| receipts     | 234 (`receipts/*.json`) |
| pubkey       | ef841443be57759d325c790518a297963b6f3d9b6c5bb63304718457236ed15d |

## 2. Receipt count: 234, not 269

The prompt says 269. The mirror has **234**, and `index.json` agrees with that.
After the run I fetched `index.json` again into a `mktemp -d` outside the repo.
It had the same `generated_at` (2026-11-04T17:07:09.079Z), so it was the same snapshot.

| stream | published | held |
|--------|-----------|------|
| KATL   | 42 | 0 |
| KDEN   | 66 | 0 |
| KHOU   | 43 | 0 |
| KMDW   | 43 | 0 |
| KMIA   | 40 | 0 |
| **sum**| **234** | 0 |

Two other counts give the same number:
- `index.json` `days[]`: 12 + 30 + 42 + 36 + 33 + 35 + 41 + 5 = 234 (2026-10-28 … 2026-11-04).
- `files.sha256`: 234 under `receipts/` and 234 under `proofs/`.

No receipts are held (`held` is 0 on every stream), so 269 is not a published-plus-held total.
Nothing in the log matches 269, and the log does not say where that number came from.
The archive only grows, so it was not a count of an earlier, larger snapshot.
The 234 above is the count from the published log.

## 3. Exit codes

Last line of each file:

| run | file    | last line |
|-----|---------|-----------|
| A   | `A.txt` | `exit 1`  |
| C   | `C.txt` | `exit 1`  |

They are equal. Each file has 27 FAIL lines: 17 `FAIL 3 schedule` and 10 `FAIL 6 nws`.

## 4. FAIL-line diff

```
$ diff <(grep '^FAIL' docs/reverify/A.txt) <(grep '^FAIL' docs/reverify/C.txt)
$ echo $?
0
```

Empty. `diff docs/reverify/A.txt docs/reverify/C.txt` is also empty: the full outputs match.

## 5. Library-level diff

`receipt_v1::verify` runs on every `receipts/*.json` (step 5 of the script):

```
$ diff docs/reverify/lib-A.txt docs/reverify/lib-C.txt
$ echo $?
0
```

Empty. Both files show 234 `ok` and 0 `err`.

## 6. How each build got rune-record

- **C: `git-cache`** (`docs/reverify/C-source.txt`). C built `--offline --locked` from
  cargo's git cache. `cargo tree` resolved
  `rune-record v0.1.0 (https://github.com/TunnelMind/rune?rev=63022824c8a0e9a7afb64080f5994573e75bb821#63022824)`.
- **A: path patch to this worktree.** `cargo tree` resolved
  `rune-record v0.1.0 (/home/o2k/.crew/work/dec-rune-record-1/code)`.
  **`--locked` was dropped** (`docs/reverify/A-locked.txt` exists). The path patch
  changes the rune-record source line in `Cargo.lock`, so cargo refuses `--locked`.
  This change only touches the temporary copy of tm-weather; `~/tm-weather` is not modified.

Both resolutions are in `docs/reverify/sources.txt`.

## 7. Reproduce

The tm-weather source came from `git archive` of HEAD
**f4882f8913a8c086a3847e5750c69fec4450cd31** (`docs/reverify/tm-weather-head.txt`).

From the repo root:

```
OUT=docs/reverify bash scripts/reverify-weather.sh
```

The log grows every hour, so a later run will have a newer `generated_at` and more
receipts. The comparison that matters is A against C on the same mirror, not
against these numbers.
