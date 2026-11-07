# 108: complete-process external-call binding comparison

The main-tree `.task/impl/108.md` is authoritative. Production baseline is
`fe2e650038062641dc2b467018243f1566fecff4`. The implementation candidate is
`7a219f2d782532324e1ea7401cab68c9eada7c82`. The first static protocol review
snapshot is `e4af26c7417b686fc478cb105de368a69302aaba`; it is not an execution
result. Any subsequent tool correction must be frozen before sampling.

## Same work and distinct identities

The programs, independent Python reference model, allocation observer and Rust
settings come byte for byte from 104 commit
`38ec6bc2d267b44bb7edab8a0d894af9cdb23954`. The small native memory launcher and
its control come from `9f5dd1188bf363c61d08d1840c97ee3f7b3a2218`.
`tools/product/108/reference-provenance.json` records each original Git path,
commit, size and SHA256. The supplied 104 identity is checked against its
registered SHA before any preparation.

| Arm | Compiler/source identity | Measured program |
| --- | --- | --- |
| BASE | Exact 108 production baseline, release compiler | Fresh ordinary O2 ZIL build |
| candidate | Exact proposed change to `foreign.rs`, same release settings | Fresh ordinary O2 ZIL build |
| Rust | Original 104 Rust source/settings and sealed executable | Byte-identical original 104 Rust O2 program, re-executed |

Both ZIL compilers are built in the same source path. The baseline executable
is copied and hashed before the single production source file is changed to
the candidate Git blob; the candidate executable is then copied and hashed.
All other production inputs and release options are identical. Intermediate
compiler build durations are provenance, not runtime observations or a new
compilation-speed comparison.

The builder inherits only the registered path/locale/cache environment keys,
pins the actual Rust compiler and linker, sets the release profile explicitly,
and supplies empty encoded flags and wrapper values. Unregistered ancestor or
Cargo-home configuration files are refused. This accounts for Cargo's
[documented configuration precedence](https://doc.rust-lang.org/cargo/reference/config.html#buildrustflags):
encoded Rust flags take precedence over ordinary `RUSTFLAGS`. Both the complete
effective build environment and each build command/stream hash are retained.
The candidate argument must resolve to the candidate SHA in the contract.

The three workloads remain stream, buffer and retained. Their exact I/O,
allocation growth, failure, history-reading and cleanup contract is preserved
in the copied 104 source and machine contract. The compiler optimization must
not use a benchmark-name whitelist or weaken ZIL's checked `u8` conversion.
The new evidence never rewrites the 104 production-24ca observations.

## Preparation and eligibility

Preparation runs all original ten input lengths at O0 and O2 for all three
arms: 180 plain executions. It also builds fresh observer programs for all
three arms and both optimization levels, then exercises failure indices
0, 1, 2, 7 and 17 at the original trace input sizes: 90 further executions.
These 270 checks must match the original independent model's output, exit,
allocation IDs, requests, failures, releases, final live bytes and peak bytes.
A failure index beyond the actual request count is explicitly untriggered.

Rust's plain binaries are copied without modification from the original 104
identity. Its observer binaries are freshly compiled from the exact same
source and pinned tools. Candidate and BASE builds use real source checking,
independent Verify, LLVM and native linking. The build reports must show one
ordinary verification run. Observation code is absent from every timed binary.

The sealed identity binds both compiler binaries and source manifests, every
program, input and expected output, all tools, source/reference files, contract,
validation records, CPU schedule and memory controls. Changing a bound file
invalidates the identity. Existing output directories are refused.

External compiler receipts are compared recursively, with exact JSON types,
against the embedded identity. Arm commit labels must match their registered
baseline/candidate commits. Process and memory-receipt paths are derived from
the result root and the registered slot, and every slot must name a distinct
original. The verifier refuses symlink aliases. Progress, raw receipts and
result records use the same strict comparison, including integer/float and
boolean/integer distinctions. Both Python entry points refuse `-O` and `-OO`
before any operation. Standalone summaries use exclusive file creation and
cannot overwrite an existing result or other original.

## Fixed runtime matrix and order

| Group | Consumers | Input bytes | Cells |
| --- | --- | --- | ---: |
| Original runtime | stream, buffer, retained | 65,536; 4,194,304 | 6 |
| Startup controls | stream, buffer, retained | 0; 1 | 6 |

Each of the twelve cells has two warmup triplets followed by thirty retained
triplets. The six permutations of BASE/candidate/Rust each occur exactly five
times among retained triplets, shuffled by seed 108. Warmup order is explicitly
registered. There are 1,152 actual process observations, including 72 warmups
and 1,080 retained observations. The complete order is stored in the identity.

Root grants the exclusive CPU 2 window. Spawn, program execution and final wait
are timed together. Input/output files are opened before the clock; expected
output and SHA checks occur after it for every arm. Page caches are not flushed.
Each observation retains the complete command, exit/timeout, wall/user/system
time, output bytes and hashes. Root's named window, actual affinity and UTC
start/end are recorded. No output rows are silently dropped.

One fixed campaign is planned. A failure, missing or duplicate row, changed
order, changed identity, wrong output, timeout or invalid duration prevents a
successful series result and preserves the available prefix. There is no
post-result expansion, slow-sample deletion or selection of favorable inputs.

## Statistics and admission scope

Each cell reports all three medians and nearest-rank p95s, the candidate/BASE
ratio of medians, Rust/candidate efficiency, and the distributions of paired
triplet ratios. Exact binomial median intervals are descriptive per-cell 95%
intervals; they do not provide simultaneous coverage for all twelve cells.
Every median and p95 regression against BASE is listed.
Passing the Rust numerical comparison does not erase a candidate/BASE startup
regression. Resolver timing and startup costs remain independent evidence for
the final production decision; no new favorable threshold is introduced.

The existing product runtime thresholds remain unchanged: efficiency is
`T_Rust/T_ZIL`, each case must reach 0.9, and the geometric mean must reach 0.95.
Original runtime cases, startup cases, and their union are reported separately.
A failed case cannot be offset by a faster different case. The protocol's
summary can report runtime evidence or NoGo; independent review and the other
product obligations remain necessary for any production/product decision.

## Separate memory and retention profile

Direct Python-parent `wait4` RSS is retained as a raw observation but is not
interpreted as target memory. 104 demonstrated that the pre-exec child's peak
can inherit the parent's much larger high-water mark.

The separate memory profile uses the reviewed native exec/fork launcher. Its
12 controls compare parent padding of 0 and 256 MiB with targets which touch
1 MiB and 32 MiB, three times each, under the existing frozen limits. Then
five observations per arm per cell produce 180 target-memory observations.
The order rotates arm position by repetition. Report every target RSS value,
median, p95, minimum and maximum; launcher times are excluded from runtime
comparison. These are individual-process peaks, not a process-tree sum.

Application allocation traces are a third, separate measurement. They describe
the explicit `run()` region's logical requested bytes, retained allocation IDs
and cleanup. They do not measure libc-internal storage or replace RSS, nor do
they substitute for source Types or independent Verify.

## Reproduction commands

First load the project's toolchain environment. The compiler and preparation
commands create new directories and preserve failures. The exact candidate
argument and final tool commit must be chosen from the frozen reviewed inputs.

```sh
python3 tools/product/108/build_compilers.py --candidate CANDIDATE_SHA --out NEW_BUILD_DIRECTORY
python3 tools/product/108/run.py selftest --out NEW_CONTROL_DIRECTORY
python3 tools/product/108/run.py prepare --reference ORIGINAL_104_IDENTITY --compilers NEW_BUILD_DIRECTORY/result.json --out NEW_PREPARATION_DIRECTORY
python3 tools/product/108/run.py verify --identity NEW_PREPARATION_DIRECTORY/identity.json
```

Only after root's explicit window grant:

```sh
python3 tools/product/108/run.py timed --identity NEW_PREPARATION_DIRECTORY/identity.json --window ROOT_WINDOW_ID --out NEW_TIME_DIRECTORY
python3 tools/product/108/run.py memory --identity NEW_PREPARATION_DIRECTORY/identity.json --window ROOT_WINDOW_ID --out NEW_MEMORY_DIRECTORY
```

The retained JSON and bytes support independent summary recomputation. The
synthetic harness refusal controls are labeled as parser/integrity controls;
they are neither performance data nor language acceptance evidence.
Original absolute paths are part of the identity: archived evidence can be
inspected anywhere, but this controller's executable recomputation requires
restoring the registered paths rather than rewriting a historical seal.

## Pre-sampling review history

The nonauthor static review of `e4af26c7` returned `NeedsFix`; its complete
receipt is retained as `evidence/impl/108/static-review-initial.json`. It
identified same-header observation reuse, permissive JSON equality, optimized
Python bypass, incomplete compiler-receipt joins/environment control and an
overwriting summary destination. No dynamic process or performance sample was
executed in that review. Corrections and narrowly targeted refusal fixtures
must pass a new review before sampling. The fixtures keep the original row
header while reusing a prior same-cell/arm payload, and separately modify only
the progress file's numeric type, so they exercise the actual discovered
boundaries rather than only duplicating a whole registered row.
