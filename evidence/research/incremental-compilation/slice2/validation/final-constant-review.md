# Independent constant-canonicalization review

Reviewer: `/root/review_incremental_slice1`, independent of the implementer.

**Verdict: Pass, limited functional scope.** Exact commit `5cf1c417f5cbdc0895e5edceb3053d9d4da490c6`, relative to previously reviewed `646099a70105b6da3821ce7d8ea20bd8e28fe3e1`. No unresolved finding. The earlier combination/owner-release implementation is unchanged; the only production delta is Builder integer construction.

The mask preserves the intended modulo-2^width pattern for all admitted 1/8/16/32/64-bit integers. Width64 shifts by zero; signed narrow values lose only invalid high bits and admitted unsigned values stay unchanged. Preflight bounds, ABI, quotas, optimizer pipeline and current Verify remain intact. Global/array/GEP integer values use the same adapter.

Independent root-cause evidence is in `o2-independent/diagnosis.json`, with **90 direct C API controls** (17 noncanonical observations), portable probe source, disassembly and serialization controls. Out-of-range diagnostic neighbors do not imply source acceptance.

## Actual limited tests

After `. /workspace/.zil-env/env.sh`, from `/workspace/zil`:

- `cargo test --offline --locked -p zil-llvm-api --features native,text-reference --test builder210 signed_and_unsigned_spellings_of_integer_bits_are_identical_at_all_widths`: **1 passed**, 40 O0/O2 native controls across all supported widths and adjacent unequal values. Log: `final-constant-review-builder.log`.
- `cargo test --offline --locked -p zil-driver --features llvm-api --test narrow_constants`: **2 passed**, 24 real-source builds and 88 native controls. API/clang, O0/O2, cold/warm cache, Verify=1, runtime negative branches and exact empty/NUL/high-byte/600-byte streams are covered. Log: `final-constant-review-source.log`.

## Cross-binary cache evidence

An independent directory was shared by old and final compilers, using nonempty input `614263210a0080`.

| Request | Hits / misses | Native exit | Verify |
|---|---:|---:|---:|
| Old cold | 0 / 1 | 2, known defect | 1 |
| Final cold, shared cache | 0 / 1 | 0 | 1 |
| Final warm | 1 / 0 | 0 | 1 |

Two distinct namespaces remain. Final cold/warm ELF SHA256 is identical: `fdfc5f616b3ff59bf3fa75f5e19eac724176e2c1f1fd86a3515fd3961a3f54d3`. Final compiler SHA256: `64fd64caaec0cd1d543aa25ba2da05e12b4d925c730295871f10c003cde5f84d`. Three ELF copies, raw reports, full commands and inputs remain in `final-constant-cross-cache/`, summarized by its `result.json`.

## Benchmark filtering and limits

Static review confirms `mixed_bench.py` preserves wrong baseline outcomes and sets the corresponding case/optimization speed ratio to null. Candidate results must match consistent clang O0/O2 native outcomes. The old O2 failure therefore cannot count as a semantically equivalent speed comparison.

This receipt does not approve final timing results, a formal full suite or a Rust/product gate. Precise coverage, binary boundaries and evidence hashes are in `final-constant-review.json`; prior coverage remains in `combo-memory-review.json` and the preserved earlier receipts.
