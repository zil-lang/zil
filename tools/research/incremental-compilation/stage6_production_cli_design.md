# New production quality CLI measurement epoch: proposed contract

This is a design contract, not an execution receipt or a performance result. The
existing private-tag pair, real/generated/opaque readers and all old results
remain frozen. The old incremental-policy NoGo remains attached to its original
compiler/source identities.

## Shared production identity

The runtime and CLI experiments consume one immutable production manifest.
`stage6_production_identity.py` should own its strict reader; the runtime agent
is preparing that shared validation. Neither benchmark invents a second source
identity. The source/build receipts are produced after the final production
commit, from an immutable checkout/archive and a new target directory.

The agreed implementation directly consumes the author's authoritative
`quality-production-a1ab/BUILD.json`; its `source`, `build`, compiler/LLVM and
protocol evidence are validated by the shared loader. The JSON below describes
the loader's canonical result, not a separately hand-authored Pass receipt.

Identity objects contain only `path`, `sha256`, and `bytes`. Supplemental LLVM
version/build-ID metadata lives beside the library identity, not inside it.

```json
{
  "schema": 1,
  "kind": "production-quality-closure-v1",
  "status": "Pass",
  "revision": "FULL_PRODUCTION_COMMIT",
  "compiler": {"path": "...", "sha256": "...", "bytes": 0},
  "source_receipt": {"path": "...", "sha256": "...", "bytes": 0},
  "build_receipt": {"path": "...", "sha256": "...", "bytes": 0},
  "llvm": {
    "library": {"path": "...", "sha256": "...", "bytes": 0},
    "receipt": {"path": "...", "sha256": "...", "bytes": 0},
    "version": "23.1.4",
    "build_id": "RECORDED_ELF_BUILD_ID"
  },
  "clang": {"path": "...", "sha256": "...", "bytes": 0},
  "strategy": {
    "namespace": "ZIL-OBJECT-ACTION-212-v2-complete-direct-call-closures",
    "source_identity": {"path": ".../crates/zil-backend-llvm/src/object_cache.rs", "sha256": "...", "bytes": 0},
    "flags": ["--incremental-o2"]
  },
  "expected_pipeline": "default<O2>|merge-functions=after|aliases=false"
}
```

These are canonical-shape placeholders, not real identities. The loader must verify:

1. The full commit and original snapshot inventory against the explicit Git
   tree; the actual frozen build-input bytes against that inventory. A
   `source_matches_revision: true` boolean alone is insufficient.
2. The build receipt's successful command/environment, source snapshot,
   resulting compiler ELF and LLVM/clang identities. A current worktree HEAD
   never substitutes for the source used to build an older ELF.
3. The namespace constant in the frozen `object_cache.rs`, and that file's
   membership in the verified source inventory. It is a source-bound cache
   protocol, not a new per-request field. Existing namespace construction also
   includes compiler identity, loaded LLVM identity, target and exact pipeline.
4. The ordinary production pipeline above, with no research suffix or arbitrary
   caller-supplied pipeline accepted as an equivalent form.
5. A generated, hashed launcher with exactly the pinned library directory and
   clang. Clear loader interposition/debug variables before the benchmark and
   timed subprocesses. A fresh wrapper-`exec` actual PID `--help` loader probe
   must load exactly the pinned DSO. This is a window binding probe, not a
   per-request loader trace.

The production compiler remains explicitly paired with the measured four-TU
LLVM DSO; a canonical pipeline string does not imply a stock LLVM binary.

## CLI strategies and independent evidence

The new target is **quality-incremental-default** and
**quality-incremental-closed**, each versus Rust. Here default means the export
contract; all ZIL options remain explicit: LLVM API, O2, after-MergeFunctions,
`--incremental-o2`, eight requested CGUs, two requested codegen jobs, independent
object and semantic caches. The closed strategy additionally passes
`--closed-executable`. Rust retains O2, eight CGUs, overflow checks, abort panic
and its own persistent per-sequence incremental cache.

The runtime companion uses the same final compiler/DSO for monolithic-after,
monolithic-after-closed, quality-after, quality-after-closed and Rust. Only the
quality arms pass `--incremental-o2`. This isolates the quality policy from a
compiler/DSO change and keeps runtime evidence distinct from CLI correctness
oracle executions.

Each profile records production manifest identity, benchmark/tool revision and
bytes, source corpus identity, actual command, actual backend and object
pipeline, cache observations, current Verify, failures and history. Production
revision and benchmark revision are separate fields. Tools or documentation
committed after the compiler build do not relabel that compiler's source.

For successful ZIL requests, require current Verify exactly once; every object
must have the exact canonical O2/after pipeline and expected LLVM version.
New objects require pre/post LLVM verification; cache hits instead retain
verified-producer provenance and explicitly report no repeated LLVM Verify.
Requested units/jobs, actual units/jobs, object-hit/miss accounting and the
complete `native_codegen.fallback` value remain visible. Any whole-module
fallback stays a valid measured execution if its other checks pass, and is
reported as a fallback; it is not renamed a partition success or removed as an
outlier. Existing ResourceLimit refusals stay in all request and failure tables.

## New coordinator and opaque profile

- A new production CLI coordinator validates the manifest and lease, creates
  a new destination, binds the launcher and tools, and runs only one window.
  It always retains a completion/failure/interruption receipt, raw log and
  request counts. It does not widen the old pair coordinator's identity rules.
- The 23-shape generator/timer can be reused by exact frozen bytes through its
  generic `--zil` and repeated `--zil-arg` interface. A new production reader
  handles its rows with the new identity contract. The old generated reader's
  private-tag validation is not weakened.
- The six-family opaque benchmark needs a new production profile/entry point,
  using the frozen source generator, five-seed native oracle, complete-CLI
  timer and five-request sequencing. Replace pair-specific identity binding
  with the common production loader; require the canonical pipeline. Do not
  reinterpret old arms, old runner hashes or old source revisions. The profile
  and its adapter receive independent static and functional review before any
  formal timing window.

All source preparation, source/identity hashing, native oracles and analysis
are outside the CLI timer. Each case/block/arm has fresh private cache roots;
`cold -> unchanged -> early_edit -> late_edit -> restore` retains real edits
and cache history. Six alternating AB/BA blocks give 1380 compiler requests per
23-shape window and 360 per opaque window. The two export contracts therefore
require four separate, identity-bound formal windows, not a pooled result from
old tools or compilers.

On timeout/interruption, kill and reap the whole compiler process group and
preserve attempted and NotRun records. Failed execution invalidates the rest
of that arm's cache-history sequence for comparisons, even if later individual
requests execute successfully. A complete comparison cell requires all six
observations from both arms; partial success produces no favorable ratio.

## Statistical and freeze boundary

Readers reuse the frozen paired-statistics function: ratio of arm medians,
2000 bootstrap draws, fixed seed 6302026, three resampled forward and three
reverse blocks, shared across both arms and all requests within each case.
The corpus stays fixed; conditional valid-cell geometric means do not establish
random workload-population performance. All cells, adverse cells, failed cells,
fallbacks and resources are exported. Native correctness observations never
substitute for the separate runtime-quality matrix.

Freeze order: final source and source inventory; actual build receipt/ELF/DSO;
common production manifest; reviewed coordinator/profile/analyzers; lightweight
positive/negative control receipts; then exclusive formal leases. Each formal
lease prohibits concurrent compiler builds/tests/profiling, analysis/hashing
and archives. After release, independent analysis freezes a new output epoch;
only then may the separately inventoried Rust caches be reclaimed. No final
cache directory is claimed to reproduce every prior intermediate state.
