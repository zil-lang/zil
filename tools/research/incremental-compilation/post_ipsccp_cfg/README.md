# Post-IPSCCP CFG cleanup experiment

This is an isolated LLVM 23.1.4 toolchain candidate, not a production ZIL pipeline
change. It adds the existing SimplifyCFG pass, with the existing downstream
options, between `GlobalCleanupPM`'s PromotePass and InstCombinePass, only at O2.
Every original pass remains. The source is pinned to LLVM commit
`7a941dfb2877a0b8cf30323fefa85ad019e0b0f5`; source, blob/hash provenance and
the LLVM license are in `upstream/`.

The actual opaque8192 API profile identified repeated InstCombine add analysis.
An external opt diagnosis of the actual checked CGU then located the expensive
invocation **before inlining**: IPSCCP had removed overflow branches but left
8193 unconditional blocks. InstCombine sank all sum adds into the final block,
revisiting their operands. Later post-inline IR already shared the 97 distinct
remainders, so additional post-inline CSE was not the measured missing step.

The opt candidate and original produced identical objects and five correct
opaque-input outputs. The isolated true C API libraries also produced identical
program bytes for the same arithmetic source; their diagnostic InstCombine
timers were 53.5422 s and 0.3395 s. These are concurrent, instrumented functional
diagnostics, not formal compile-speed ratios. The scalar8192 negative control
still spends about 1.7 s in JumpThreading in both arms, so this scheduling change
does not replace the independently tested entry-preserving algorithm.

## Reproduce the libraries

Use a new output directory and an available functional build window:

```sh
cd /workspace/zil
cfg_tools=tools/research/incremental-compilation/post_ipsccp_cfg
cfg_builder=tools/research/incremental-compilation/jump_threading_plugin/build_component_toolchain.py
cfg_run=/workspace/.zil-env/research/cfg-reproduction
python3 "$cfg_tools/prepare.py" --out "$cfg_run/source"
for cfg_arm in original cfg; do
  python3 "$cfg_builder" --llvm-prefix /workspace/.zil-env/llvm-23.1.4 \
    --headers-prefix /workspace/.zil-env/research/stage6/jump-threading-plugin-sdk \
    --recipe "$cfg_run/source/$cfg_arm-recipe.json" \
    --out "$cfg_run/$cfg_arm" --jobs 2 --execute
done
```

Both arms recompile `PassBuilderPipelines.cpp.o` with identical flags and replace
only that member in an independent copy of `libLLVMPasses.a`. The original SDK is
untouched. Receipts bind the 61 archive inputs, every unchanged member, exact SDK
headers, source/patch, compiler/linker, commands and actual DSO. A same-flags
upstream rebuild is essential; the installed SDK has different build provenance.

The existing prepared prefixes are
`stage6/post-ipsccp-cfg-{original,cfg}` under `/workspace/.zil-env/research`.
Their `native-toolchain-receipt.json` describes the exact library. A fresh
compiler process selects one library with `LD_LIBRARY_PATH=<prefix>/lib`.
Formal use also requires a private compiler source/pipeline tag, actual loader
verification and the ordinary DSO-aware cache namespace; do not silently switch
libraries under a receipt for another arm.

## Controls and retained failures

`controls.py` reuses frozen reviewer fixtures and the C API child as **author**
tests, not as a new independent review. The installed SDK, rebuilt original and
candidate completed 63 real C API requests and 81 native invocations. Installed
versus rebuilt-original IR matched for all 21 cases. Candidate O0 and standalone
JumpThreading outputs were unchanged. O2 semantic controls cover shuffled CFGs,
loops, PHIs, nonreturn/assume, guard, EH, convergence, blockaddress and profiles.

`profile.py` requires an explicit checked `--clang` path and library receipt.
It uses the existing timing probe, current Verify and LLVM before/after verify;
the existing interposition policy disables object-cache reuse. Native results
are compared with five independent recorded source oracles. It records exact
child/descendant RSS using `wait4`; a single functional observation is not a
memory benchmark. No global SLP option is changed.

Evidence roots under `/workspace/.zil-env/research/stage6`:

* `arithmetic-ir-comparison`: matching Rust input with LLVM passes disabled,
  actual ZIL CGU and final IR shape counts.
* `arithmetic-stage-profile`: one bounded opt stage diagnosis, full pass logs and
  16 selected-function IR snapshots. Its full per-run table is large; the raw
  stderr is the authoritative source.
* `arithmetic-cfg-control`: the one-pass opt candidate, identical object and five
  native outputs.
* `post-ipsccp-cfg-controls`: the 63/81 C API and native author matrix.
* `post-ipsccp-cfg-profiles/{original-v2,cfg-v2}`: valid true API arithmetic
  controls; `original` and `cfg` retain earlier `E-LINK-IO` failures caused by a
  missing explicit clang environment. Their optimizer timers are not valid
  end-to-end build results.
* `post-ipsccp-cfg-scalar`: the two valid scalar negative controls.

Remaining work includes independent review of this exact candidate, combined
SSA/LVI/entry interactions, current-source cache/resource/failure controls,
exclusive distribution timing and native runtime quality. No existing control
or profile is relabeled as a successful future combination.
