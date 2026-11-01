# Entry-preserving JumpThreading research

This directory contains two distinct experiments. Neither changes the production
compiler or the installed LLVM prefix.

* The **pass plugin** registers an upstream clone and an entry-preserving clone
  in the pinned, statically linked `opt`. It isolates the algorithm, using actual
  checked CGU input captured by the Clang research harness. It does not exercise
  the compiler's LLVM C API path.
* The **native toolchain pair** recompiles one `JumpThreading.cpp.o` and relinks
  the existing PIC archives into separate LLVM shared libraries. The real ZIL
  C API compiler loads these libraries. No new C++ interface enters ZIL.

There are three LLVM identities: the installed SDK, a same-flags rebuild of the
upstream source, and a same-flags rebuild of the entry fix. The upstream rebuild
is necessary because its object bytes differ from the SDK build. Comparing only
the installed SDK with the fix would mix compiler-build differences into the
algorithm experiment.

## Run the frozen API candidates

The prepared workspace has launchers with an explicit library directory:

```sh
/workspace/.zil-env/research/stage6/jump-threading-api-pair/zil-original --help
/workspace/.zil-env/research/stage6/jump-threading-api-pair/zil-entry --help
```

Use either launcher as the `--zil` argument to `stage6_bench.py`, or pass ordinary
ZIL build arguments directly. The sibling `api-pair-receipt.json` records the
launcher, ELF, common source, pipeline tag, DSO, producer logs and exact hashes.
Each process must load only its own LLVM. These launchers select the library;
they do not change optimization flags or enable instrumentation.

For runners that manage process environments themselves, use each receipt arm's
`compiler.path` and `env`. The runtime runner additionally checks the actual
compiler PID's `LD_DEBUG=libs` initialization records. `LD_PRELOAD` timing probes
disable the ordinary object cache and are unsuitable for formal cache timing.

The frozen private snapshots are based on
`ac4ebb9e5fe668b160cbc4b1fe30db735858fdce`, with four private pipeline/cache
identity strings changed. The real pass pipeline remains unchanged. The source
manifest and patch are retained under `jump-threading-api-{original,entry}-snapshot`.
Upstream integration later restored a stale DB **test module** in
`888af53a0a7389d10a508250059db6b631c61e4a`; ordinary release production files
are identical. These historical snapshots retain their original bytes and are
not relabeled as the later commit. New source snapshots can use `888af53a...`
to obtain the repaired test tree.

## Rebuild the native pair

Required inputs are the repository's verified LLVM 23.1.4 install and exact SDK
headers, including generated headers. The upstream commit is
`7a941dfb2877a0b8cf30323fefa85ad019e0b0f5`; `upstream/PROVENANCE.json` and
`LICENSE.TXT` bind the source and LLVM license. `prepare_headers.py` extracts and
validates the pinned official SDK archive; it never substitutes system headers.
The existing workspace headers receipt is
`/workspace/.zil-env/research/stage6/jump-threading-plugin-sdk/headers-receipt.json`.

Run the following in an available functional build window. All output paths must
be new; the tools refuse replacement of an existing prefix.

```sh
cd /workspace/zil
jt_tools=tools/research/incremental-compilation/jump_threading_plugin
jt_run=/workspace/.zil-env/research/jt-reproduction
jt_llvm=/workspace/.zil-env/llvm-23.1.4
jt_headers=/workspace/.zil-env/research/stage6/jump-threading-plugin-sdk
python3 "$jt_tools/prepare_native_patch.py" --out "$jt_run/source"
for jt_arm in original entry; do
  python3 "$jt_tools/build_native_toolchain.py" \
    --llvm-prefix "$jt_llvm" --headers-prefix "$jt_headers" \
    --sources "$jt_run/source" --arm "$jt_arm" \
    --out "$jt_run/$jt_arm" --jobs 2 --execute
  python3 "$jt_tools/prepare_api_snapshot.py" --repo /workspace/zil \
    --revision 888af53a0a7389d10a508250059db6b631c61e4a \
    --toolchain-receipt "$jt_run/$jt_arm/native-toolchain-receipt.json" \
    --out "$jt_run/$jt_arm-source"
  ZIL_LLVM_LIB_DIR="$jt_run/$jt_arm/lib" \
  LD_LIBRARY_PATH="$jt_run/$jt_arm/lib" \
  RUSTFLAGS="-C link-arg=-Wl,-rpath,$jt_run/$jt_arm/lib" \
  CARGO_TARGET_DIR="$jt_run/$jt_arm-target" CARGO_BUILD_JOBS=2 \
    cargo build --offline --locked --release -p zil-cli \
      --manifest-path "$jt_run/$jt_arm-source/Cargo.toml"
done
LD_LIBRARY_PATH="$jt_run/entry/lib" "$jt_run/entry-target/release/zil" --help
```

`native-toolchain-receipt.json` records all 61 input archives, exact headers,
compiler/linker identities and commands, the replacement object, unchanged
members, SONAME/build ID, library hash and dependency hashes. No full LLVM build
is performed. The original SDK directory is not modified. The component builder
is a separate extension for later explicitly enumerated TU experiments; its
results must not be attributed to this frozen pair.

## Plugin and wrapper controls

`prepare_clone.py`, `build_plugin.sh`, `prepare_pipeline.py` and `controls.py`
produce the original/fixed pass plugin and small IR controls. The plugin must
resolve LLVM symbols from host `opt`; linking another `libLLVM` into it would
create duplicate registries. The canonical O2 pipeline is printed by the exact
target/version, with only the two JumpThreading nodes replaced.

`run_request.py` invokes the checked capture compiler and `clang_wrapper.py` on
each actual `unit-N.ll`. `preflight.json` binds the structured machine unit and
capture; public `program.ll` is not a substitute. `cache_bundle.py` stores each
object, optimized bitcode and receipt in one checked atomic ZIP bundle. Pending
entries are published only after the requested native oracle passes. Publication
is atomic per key, not across all keys in a request; per-key failures are recorded.
Driver/native launch failures, timeouts and malformed reports retain a result;
native execution without an explicit input receives EOF.

Author controls and independent review are separate evidence:

* `/workspace/.zil-env/research/stage6-jt-review/review.json`: plugin algorithm,
  semantic differential, failure handling and cache publication review.
* `/workspace/.zil-env/research/stage6-native-jt-review/review.json`: actual three
  DSO C API controls, matching source/tag compilers, loader and cache isolation.
* `stage6/jump-threading-real-controls`, `jump-threading-opaque-controls-v2`, and
  `jump-threading-resource-source-controls-v3`: author actual-source controls.

## Findings and limits

The scalar 8192-function case repeatedly replaced the entry block and rebuilt
dominators. The entry fix removed that path; the three plugin arms produced the
same object bytes in that control. These instrumented functional timings are
diagnostics, not formal performance results.

The fix **does not resolve** opaque 2048-branch SSA reconstruction or opaque
8192-arithmetic InstCombine costs. Both negative controls retain identical
objects and native outputs, and the whole-function tracking worklist may add
cost to unaffected shapes. New SSA/LVI experiments use separate sources,
libraries and receipts; their success must not be inferred from this review.

Earlier failed capture/link/option-order epochs, an interrupted expensive
`-verify-each` request, and cache fault controls remain retained. The ordinary
production optimization defaults are unchanged; no runtime-quality or formal
speedup claim follows from these functional controls alone.
