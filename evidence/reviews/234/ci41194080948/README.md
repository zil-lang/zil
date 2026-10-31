# 234 — first-run LLVM23 artifact review

This is a read-only review of artifacts already produced by CI run **41194080948**.
No compiler test, object, or program was executed during this review. The whole
workspace remains **Fail: 3020 passed, 4 failed, 0 ignored, 0 filtered**. These
selected successful stages do not constitute final 234 or construction11 acceptance.

The actual checkout was `89406b33b43fc17fe967b5bb9f629a16cb44b763`.
Its archived complete `git ls-tree -r` output matches candidate
`77b30b4e13187ca9e5ddf8b830f11675fcaffa5c` byte for byte. Toolchain receipts identify
LLVM API and Clang **23.1.4**. Original artifact download receipts are retained.

## Scope and observations

- **Builder width control:** the original `builder210` suite reports 9/0 and
  includes the passing `signed_and_unsigned_spellings_of_integer_bits_are_identical_at_all_widths`.
  Its frozen source covers widths 1, 8, 16, 32, and 64, two signed boundary patterns,
  equivalent unsigned spellings and a different neighbor, at O0/O2: 40 generated
  model executions. `execute` asserts builder input, zero parsing time, and LLVM
  verification before/after optimization, then links and executes the object.
  The test removes its temporary directory. Therefore this part has exact source
  plus the original CI success log; it has no retained per-case object receipts.
- **`impl234_narrow_constants`:** 2/0, 24 compilation reports and 88 retained
  runtime observations, across both backends, O0/O2, cold/warm. The audit checks
  64 negative-return observations and 24 byte-stream observations. The original
  inline SourceUnit strings were not written as `.zil` by the test. Their exact
  pinned Rust test source is retained; byte-stream source comes from the exact
  candidate. Do not describe these as captured inline source files.
- **Independent `review234_full_width`:** 1/0, 8 compilation reports and 56
  runtime observations. Every captured source equals the frozen test source.
  For each input, a separate artifact-audit oracle recomputes all 12 output bytes
  as little-endian signed i32 plus i64. The saved expected and observed bytes,
  empty stderr, and exit zero all match. This does not rely on the process exit
  byte to identify the returned integers.
- **Backend/cache:** all reports name the actual backend and have no fallback.
  Eight API cold/warm pairs preserve the same object action key. Cold reports
  use the LLVM23 builder with zero parsing time; warm reports use the object
  cache, have current source `verification_runs=1`, and correctly record that
  LLVM verification occurred in the producer, not again on the cache hit.

The audit verified size and SHA256 for all **708** files under the selected 234
case directories against the uploaded 68,300-file manifest. **684** small raw
files are retained. Executables and object-cache payloads are represented in the
verified manifest subset but are not copied. `OBSERVATIONS.json` contains the
per-case checks; `SELECTED-NATIVE-MANIFEST.json` separates verified and retained
files. Logs are verbatim extracts from the original workspace log.

The joint consumer review is stored at
`evidence/reviews/construction11/ci41194080948` in the same evidence commit.

## Reproduce the artifact audit

This command reads existing artifacts; it does not run tests or native code:

```sh
python3 evidence/reviews/234/ci41194080948/audit_artifacts.py \
  --artifacts /workspace/scratch/994319f2ba02/z11-ci41194080948 \
  --repo /workspace/scratch/994319f2ba02/zil-review234-ci
```

The first audit invocation stopped at a source-snapshot locator error:
`support210.rs` is a directory module, `support210/mod.rs`. The locator was
corrected before the successful complete artifact audit; no compiler result or
CI artifact was modified. Final 234 acceptance remains assigned to
`/root/next_language_slices` on the final green CI candidate.
