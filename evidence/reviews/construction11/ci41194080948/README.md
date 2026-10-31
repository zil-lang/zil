# Joint consumer — first-run LLVM23 artifact review

CI run **41194080948** remains a whole-workspace **Fail**. This review covers only
the two successful `construction11_integration` tests from the exact archived
checkout `89406b33b43fc17fe967b5bb9f629a16cb44b763`, whose complete source tree
matches candidate `77b30b4e13187ca9e5ddf8b830f11675fcaffa5c`.

All four positive cases (LLVM API/Clang 23.1.4 × O0/O2) have:

- Captured main/rules/data source byte-identical to the candidate.
- Successful link, `verification_runs=1`, correct actual backend, and no fallback.
- For API, actual builder input, zero parse time, and LLVM verification before/after.
- Full i32 comparison of the two callback results against **14** in the source,
  followed by actual exit zero. The result is not inferred from a truncated
  process exit byte.
- Exactly one output byte **0x41 (`A`)**, empty stderr, and exit zero. This observes
  the original owner cleanup after its shared callback's final use and move.

The nearest negative is the exact original main source with the owner move placed
before the final two callback calls. Its retained raw report is **SafeReject at
Verify**, `E-BORROW-CONFLICT`, `verification_runs=1`, with no executable and no
native commands. Main/rules/data, the rejection report, and selected real phase
artifacts are retained. This is source rejection evidence, not a raw-IR mutation.

All **91** files under these five case directories were checked against the
uploaded manifest by size/SHA256; **43** selected small files were copied.
Executable and omitted phase-file hashes remain in the verified manifest subset.
The original test source and verbatim successful CI group log are included.

The read-only audit script, exact toolchain/download receipts, and broader source
identity evidence are in `evidence/reviews/234/ci41194080948` in the same commit.
No tests or native executables were run during review. Complete construction11
acceptance still requires the corrected candidate and final green CI.
