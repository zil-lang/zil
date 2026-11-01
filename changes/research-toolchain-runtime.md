# Runtime quality across the final compiler and isolated LLVM toolchains

Add a new `stage6_toolchain_runtime.py` sibling. Existing measured runners and
their data remain frozen. The comparison has nine explicitly configured arms:
43037 MergeFunctions-off with default/closed linkage; ac4 rebuilt-original LLVM
with after-merging and default/closed linkage; the same ac4 compiler with the
entry-preserving LLVM, after-merging, default/closed linkage, and the two
corresponding incremental-O2 variants; Rust.

Retain the six opaque-input source families, five seeds, 20,000 iterations,
independent full-four-byte arithmetic oracle, and six alternating forward/reverse
blocks from the frozen MergeFunctions quality runner. This is 54 one-time
artifact builds and 1,620 runtime observations. Build latency is diagnostic; it
is not a six-block compilation benchmark. Preparation and execution are separate
commands so the formal runtime window can use only prebuilt artifacts.

An explicit JSON configuration binds each arm's compiler SHA, environment,
options, source receipt, and actual LLVM DSO/receipt SHA. Validate toolchain
receipts and dynamically loaded LLVM before preparation succeeds. Compiler
environment overrides apply only to that arm. Rust and produced executables do
not inherit another arm's LLVM selection. A prepared execution rechecks config,
tools, sources, compiler/DSO identities, and executable hashes.

Preserve commands, compiler stdout/stderr and reports, loader-audit output, nm
global definitions and dynamic definitions, size output, executable sizes/hashes,
all runtime results and incomplete/failed observations. Every successful ZIL
build requires current Verify=1, actual LLVM API emission, before/after LLVM
verification, and positive actual units; these fixed large incremental-O2 arms
must demonstrate multiple actual units rather than only requested units.
Every runtime success requires exit zero,
exactly the oracle's four bytes, and empty stderr. Invalid builds cannot become
successful runtime samples; any failure makes the final status nonzero.

Only scoped functional protocol/failure controls and a small old/current-binary
smoke are authorized before the integrator freezes the complete nine-arm config
and grants a formal CPU lease. Production compiler/toolchain changes and final
promotion decisions are outside this tool's write set.

The configuration has `schema: 1`, an ordered `arms` array, and `tools` entries
for clang/nm/size, full `baseline_revision`/`candidate_revision` commit IDs, and an
explicit `candidate_variant`. Every file identity is `{path, sha256}`. A ZIL arm has
`name`, `language: "zil"`, `compiler`, `env`, `options`, `source_receipt`, and
`llvm: {library, receipt}`. Toolchain receipts must have `status: "Pass"` and
matching library path/SHA; installer-relative library paths and native-prefix
absolute paths are supported. Source receipts bind provenance bytes rather than
pretending that the runner itself reconstructed the compiler build. The Rust arm
has `language: "rust"`, its compiler identity, empty env/options, and no ZIL LLVM
override. Formal arm order is the nine names in the runner's `ARM_OPTIONS`.

Formal native arms also declare `expected_toolchain_arm`: original slots require
`original`, while the historically named entry slots require the exact
`candidate_variant`. Both must equal the native receipt's `arm`. The manifest
displays the complete variant rather than disguising a combined LLVM experiment
under an old slot label. Baseline build receipts bind compiler SHA and revision;
native source receipts bind the common revision/original-source inventory,
toolchain receipt, DSO and private pipeline tag. The actual compiler's object
reports must contain that tag. Group members share compiler/provenance/env/DSO;
the original and candidate DSOs must differ. Future combined candidates require
new explicit identities and data; existing v1 receipts are never relabeled.

Each actual ZIL diagnostic build sets `LD_DEBUG=libs` and a private
`LD_DEBUG_OUTPUT` prefix. Audit only the actual compiler PID's loader file, require
one canonical loaded LLVM DSO matching the configured library SHA, and retain
separate child-process loader logs. No LD_PRELOAD/LD_AUDIT interposition is used.
Native runtime observations use a clean environment without any arm's loader
selection or diagnostic settings. The `--functional-smoke` option explicitly
marks reduced case/block/iteration protocols and cannot be confused with a
formal six-case run.
