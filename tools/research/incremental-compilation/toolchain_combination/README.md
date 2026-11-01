# Combined LLVM research toolchain

The candidate composes four measured mechanisms in four pinned LLVM translation
units: entry-preserving JumpThreading, its batched SSA repair with exact
placement-query reuse, LVI value-to-block invalidation indexing, and an O2 CFG
cleanup before the first post-IPSCCP InstCombine. These are separate mechanisms;
their individual diagnostic reductions must not be added to predict a combined
speedup. No production ZIL default or public ABI is changed by this directory.

## Build from a clean Linux checkout

First install the repository's pinned official LLVM 23.1.4 SDK with
`tools/llvm/install.py` and extract its exact headers with
`jump_threading_plugin/prepare_headers.py`. The installation needs its recorded
PIC component archives and `install-receipt.json`; the header directory needs
`include/` and `headers-receipt.json` for that same archive. All four LLVM source
files and their license/provenance are vendored in this repository.

One command regenerates every patch and recipe and builds both libraries:

```sh
python3 tools/research/incremental-compilation/toolchain_combination/rebuild.py \
  --repo "$PWD" --llvm-prefix "$LLVM_PREFIX" \
  --headers-prefix "$HEADERS_PREFIX" --out "$NEW_OUTPUT" --jobs 2
```

It requires no prior research workspace or absolute-path recipe. Existing output
directories are refused. The SDK remains unchanged. The original arm recompiles
all four **unmodified** upstream units with identical flags; the candidate arm
recompiles the four composed units. Only those archive members differ. The
maximum compile concurrency is one; linker threads are at most the requested
two. The real tested source commit is LLVM
`7a941dfb2877a0b8cf30323fefa85ad019e0b0f5`.

Outputs:

* `rebuild-receipt.json`: overall status and exact producer commands.
* `source/`: original/candidate CPP, input provenance, complete patch and recipes.
* `original/native-toolchain-receipt.json` and
  `candidate/native-toolchain-receipt.json`: 61 archive inputs, unchanged-member
  controls, source/header/tool identities, build commands and DSO identity.
* `candidate/lib/libLLVM.so.23.1`: candidate library; the original control is at
  the corresponding `original/lib/` path.

The candidate receipt's `arm` is `entry-ssa-v4-lvi-cfg`; the original arm is
`four-tu-original`. Require `status == "Pass"` and verify `library.path` and
`library.sha256` before using either result. Select one library per fresh process
using `LD_LIBRARY_PATH=<arm>/lib`; use the same directory for `ZIL_LLVM_LIB_DIR`
when compiling ZIL. `LD_PRELOAD` is unnecessary and disables ordinary cache
reuse under the existing interposition policy.

Add `--build-cli --revision <common-ZIL-commit>` to build two private compilers
with explicit library/pipeline cache tags and launchers under `pair/`. Cargo may
fetch dependencies; add `--offline` to require an existing dependency cache.
Those compiler source snapshots differ only in four private identity strings.
The native pass pipeline is selected by the recorded library. Producer commands
and environment overrides are retained, including Cargo target/jobs settings.

For full repository validation, an external workflow can instead build and test
the ordinary repository against the candidate library using explicit
`ZIL_LLVM_LIB_DIR`, `LD_LIBRARY_PATH` and an independent `CARGO_TARGET_DIR`. Such
tests are distinct from the private tagged compiler pair. A real CLI's actual
PID `LD_DEBUG=libs` initialization record should match the receipt's canonical
library path and hash; checking the version string alone is insufficient.

## Frozen workspace candidate

The current prepared pair is
`/workspace/.zil-env/research/stage6/combined-v1-pair/api-pair-receipt.json`.
Its `zil-original` and `zil-candidate` launchers select both the exact LLVM prefix
and the explicit SDK clang. The compilers share ZIL production revision
`888af53a0a7389d10a508250059db6b631c61e4a`; their source receipts and complete
file manifests are retained. Old entry-only, individual SSA/LVI/CFG, and failed
epochs remain separate and are not relabeled as this combination.

`prepare.py` validates the frozen constituent manifests, verifies that the entry
transformation exactly recreates the independently tested entry source, then
applies it to the SSA source. It does not merge the rejected SSA v2/v3 candidates.
`prepare_api.py` adds private cache tags. `finalize_pair.py` verifies source bytes,
producer logs, ELF tag strings and actual per-PID loaded libraries before writing
the pair receipt. The ELF/library hashes in that receipt are the runnable
candidate identity, not the earlier opt plugin or another DSO with the same
version string.

## Author controls and boundaries

`controls.py` performs true C API differential controls against installed LLVM,
the rebuilt original and the candidate. The frozen combined author run passed
63 IR requests and 81 native invocations; the installed/rebuilt-original outputs
match for all 21 cases, and O0 candidate output is unchanged. Its reused reviewer
fixtures are explicitly author controls, not a new independent review.

`profile.py` binds the compiler source receipt, actual DSO, explicit clang and
timing probe. Five bounded diagnostic requests passed current Verify, LLVM
before/after verification and all native oracles: scalar after, opaque branch
after, opaque arithmetic after/early, and scalar closed early. The arithmetic
after/early programs are byte-identical. Early merging still leaves an expensive
IPSCCP case on closed scalar input; it is not promoted. Branch machine-code bytes
can differ, so final same-source runtime quality remains a separate requirement.

The candidate's O2 CFG change also applies to applicable LTO phases of the shared
module-simplification builder. Do not claim those pipelines are unchanged merely
because ZIL's current API experiment uses ordinary O2. The LVI index retains
conservative historical memberships until value deletion/cache clear, and the
entry traversal has a whole-function tracking worklist; measure memory and
unaffected distributions as well as hotspot cases.

Functional timers, concurrent builds and pass-probe runs are not formal speedup
measurements. Final acceptance requires independent review of this exact
combination, current-source cache/cleanup/failure controls, full repository
validation, exclusive representative compilation timing and runtime quality.
