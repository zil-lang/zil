# 212 measurement contract

Base source: GitHub main b8e34416c2a1673a4be8955739cf4f10133fec1e (211).
Research changes: opt-in trusted local native-object reuse, not a change to frontend
acceptance, target semantics, partitioning, optimizations, or the numeric product gates.

Primary: the SAME frozen 212 release binary, A without `--object-cache`, B with it.
Same 4 logical CPUs/quota, 8 CGUs/4 workers, O0. Five ABBA/BAAB paired blocks, five
inherited cases. Each arm starts in an empty directory, then executes cold ->
unchanged -> private edit -> public edit; both retain the existing source-candidate
cache. Only B retains complete native object entries. No prewarming outside cold
measurement, no executable/link cache. Full wall clock includes tool identity,
key hashing, cache reads/writes, current Verify, lowering, IR rendering and linking.
Native behavior checks happen outside that clock and use independent inherited
oracles. Raw records are append-only and successful requests must run Verify once.

Inherited project weights small .05, medium .15, large .60 (two cases .30 each),
consumer .20. Request weights cold .10, unchanged .35, private .40, public .15.
Aggregate absolute times first, then ratios. For each case/request/arm, average
within a block and take the median across blocks. Bootstrap resamples paired blocks;
intervals are exploratory on a shared host, not general product guarantees. Every
positive-weight cell must be complete and pass; do not renormalize failures away.

Bridge/regression guard: 211 versus 212 with object cache DISABLED, cold O0/O2,
1 CGU/1 worker, all five projects, one ABBA per project/profile. Label these small
auxiliary samples; they are not incremental measurements or the primary estimator.

Rust reference: fresh frozen plan, current restored rustc 1.99.1/LLVM23.1.1 versus
212/LLVM17.0.6. Both 4-CPU allocation and 8 CGUs, O0, no debuginfo, checked overflow,
Rust panic=abort. Rust incremental ON in its own empty-to-warm directory; ZIL object
cache ON. Three ABBA/BAAB paired blocks and same four requests/positive-output
oracles. This is direct compiler invocation, NOT Cargo no-op; toolchains, frontend
capabilities, library/runtime implementations and partition heuristics differ.
The synthetic corpus and empirical weights are not industry sampling. Do not claim
Rust-equivalent language coverage, universal faster compilation, or equivalent
runtime/error behavior merely from positive-output matching.

Correctness before cost: machine hash mutation tests; real-source edits; target/
optimization separation; damaged/truncated cache rebuild; no authority reuse;
link failure/cancellation cannot populate cache; old outputs protected; concurrency;
source-owned State/Store/callback/native outputs. New timings require fixed source,
runner, tools and input SHA256. If a bug or instrumentation error is found, preserve
that run, correct it and create another plan, never silently replace old records.

Source semantics, default 1-CGU settings, budgets, policy and product gates stay
unchanged. Cache disabled by default: `--cache-dir` does NOT silently opt into a
trusted object cache. Native object caching is currently a Linux-host capability.
Windows object output may be separately checked; Windows-host and remote caches
are not advertised. Independent external review and formal product gates: NotRun.

Pre-measurement test correction: first full regression stopped on the new cached
State/Store test because it wrongly expected field_exit to return zero. Existing
177 source and llvm_api208 control expect exit 1 on that cleanup path. Correct only
the new test oracle; keep failed log, do not alter producer code. Also use a mutable
source in the borrow negative so it specifically checks E-BORROW-CONFLICT rather
than an earlier immutable-assignment diagnostic. No timings had been collected.

## Supplementary plans (separate frozen windows; no pooling)
After all original five plans finished, extend the runner without altering the
frozen production binaries. Preserve the initial script bytes with its original
records (and in Git f733e15). Add two three-block/four-request/240-build windows:
- upgrade: exact restored 211 uncached versus exact 212 with object cache, 8/4 O0.
  This measures the complete new version/configuration, rather than attributing
  a same-212 toggle result directly to the prior version.
- rust/native: keep ZIL at 8/4, but let Rust choose its normal incremental CGU
  count and set embed-bitcode=no as the official rustc guide recommends without
  LTO. Do not force a competitor to adopt ZIL's partition count. Resource quota
  stays four CPUs. Keep the original matched-eight-CGU reference, adverse results
  included; no best-cell cherry-picking. The reference is still direct rustc and
  not Cargo no-op, not whole-language or runtime-quality equivalence.
No production source or tested binary changed for these supplementary plans.
