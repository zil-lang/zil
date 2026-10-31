# Earlier stage6 screening evidence

These are the original `screen-default`, `screen-closed` and `screen-isolation`
runs: 540 requests and 135 case/request summary cells across the three windows.
The archive retains source generation, commands, source identities, raw rows,
stdout/stderr, native results, final LLVM text and the original window receipts.
Default/closed compare ZIL with Rust; isolation compares the candidate ZIL with its
recorded reference compiler. They are two-block screens, not final acceptance.

`FILES.json` maps original paths to exact SHA-addressed blobs in `raw.tar.gz`.
Repeated bytes are stored once. `OMITTED.json` lists each excluded ELF, compiled
object and Rust binary cache with its size and observed byte hash. The archive
includes the Rust-source comparison as it existed at collection; later Merge/SLP
source-reference additions belong to the separate correctness supplement.

Use `stage6_archive.py --extract <this-directory> --dest <new-directory>` to restore
retained files, or `--verify <this-directory>` to check every blob and sidecar.
`VERIFY.json` records verification of all 2,274 retained paths. Collection-time
source copies remain distinct from each run's original `runner.py` and manifest.
