# Frozen pass, SLP and perf diagnostics

This package contains the frozen pass-timing/SLP/perf inventory, exact LLVM/Rust
reference sources, the later ca79 pass profile and the frozen ca79 opaque8192
profile. Active jump-threading plugin candidates, SDKs and build targets are
outside scope. The earlier diagnostics-plan remains a historical selection plan;
this package is the actual collection.

The evidence preserves distinct outcomes: initial missing-table instrumentation
failures; corrected instrumentation; nine 8192 pass diagnostics; 18 semantic builds
and 90 native runs whose SLP setter did not actually disable SLP; the effective
ablation's 20 valid builds and four NotRun cases; three perf permission failures
before compilation; and attempt4's successful sampling. Adaptive coordinator exit
1 is a research stop, not a compilation failure. These diagnostics do not establish
an exclusive, general compile-time improvement.

All five one-byte native `input.bin` files and the 4,731,204-byte attempt4
`perf.data` are retained. `COVERAGE.json` checks them, the adaptive-stop receipt,
earlier failed perf paths and absence of active plugin paths. ca79 profiles retain
their own source/ELF/request identities; collection does not relabel older results.

`VERIFY.json` validates all 1,187 retained paths, compressed manifests and 1,043
content blobs. Sixty-three omitted build/cache artifacts have individual byte
hashes. Use `stage6_archive_large.py --verify <this-directory>` or `--extract
<this-directory> --dest <new-directory>` for schema-2 verification or restoration.
