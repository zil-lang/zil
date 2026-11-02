# LVI value-index experiment

`prepare_patch.py --out <new-directory>` creates untouched and indexed LLVM
`LazyValueInfo.cpp` variants, their exact diff, source manifest, and two recipes
for the separate `jump_threading_plugin/build_component_toolchain.py` research
builder. Preparation performs no C++ compilation or compiler workload.

The patch adds a conservative value-to-block reverse relation to the private LVI
cache. It preserves every lattice fact and all existing invalidation paths while
limiting value invalidation to registered block slots. The original public LLVM
headers remain unchanged; the anonymous cache implementation layout changes.

See `changes/research-lvi-value-index.md` for invariants, evidence, measurement
boundaries and required controls. Do not run the recipes outside the integrator's
functional window, overwrite the official LLVM prefix, or attribute timings to
this change before the rebuilt-original and patched arms pass actual API and
cache controls. This directory does not change production ZIL or its defaults.

The vendored LLVM source and license are unmodified and pinned in
`upstream/PROVENANCE.json`. Generated modifications retain their upstream
copyright and SPDX notices.

`c_api_controls.py` loads each isolated DSO in a fresh `c_api_child.py` process,
checks the actual loaded path, and runs the fixed IR/native differential inputs
with per-predecessor ranges both disabled and enabled. `cache_controls.py`
extracts the actual private cache implementation and compiles its unchanged
methods with assertions, exposing fields only for full-scan relation checks.
That internal test is reported separately from real C API and ZIL requests.
`profile_branch.py` runs one identified ca79 opaque-input request through the
existing timing-only probe, records exact-child `wait4` RSS, and verifies five
native input oracles. Its timings are functional diagnostics unless an exclusive
performance window is explicitly assigned.
