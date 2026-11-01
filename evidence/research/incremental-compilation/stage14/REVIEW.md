# Stage14 scoped independent review

Result: **Pass within the stated scope; no blocking finding.** Reviewed production is `75178b1cfb42bfd37746b7573999d74d7d7668db`, compared with delivery baseline `5821120ac5952bcf799afe76de1030741d9ccb89`. The reviewed production files and `Cargo.lock` have no subsequent diff from that production commit. This is a research review, not final CI or a product acceptance decision.

Reviewer: `/root/stage8_rust_queries`, a non-author of the Stage14 changes reviewed below. The earlier direct-text and initial parallel-lowering static passes belong to `/root/stage7_scalar_review`; this review does not claim to repeat those complete reviews. My scope is preflight scratch reuse, the refined parallel gate, the single-file semantic discovery probe, the warm measurement tool, saved controls, and final evidence consistency.

## Implementation and controls

- Scratch reuse preserves both validation passes, instruction/error order, dominance algorithms and scheduling eligibility. Every function clears maps, operand uses, all retained CFG rows and dominator lengths before reuse. The ≤32-block / ≤256-parameter-plus-instruction selection only chooses storage; larger functions use fresh temporary scratch. Oversized operand capacity is discarded after success or error. No proof or logical state crosses functions or requests.
- The refined lowering gate selects the existing serial path when fewer than 128 non-summary functions remain, or the collected graph is small. It does not change the model or diagnostic order. It is an upper-bound scheduling heuristic, not a claim that every remaining function is expensive.
- The semantic probe retains canonical source reading, source-byte charges, root-stem identity, cancellation and artifact guards. Its raw `import`/`syntax` exclusion conservatively covers the parser's actual import tokens; comments and strings can only cause fallback. The existing source/target/options/full-Limits/compiler binding remains. Cached raw candidates still run current independent Verify. Misses, corrupt or rejected candidates return to discovery with the same read bytes; cancellation does not retry. Closed and active-query contexts remain excluded.
- The warm tool retains the Stage13 wait4 measurement function unchanged by AST comparison. Its separate Rust seed supplies a history and command, not elapsed observations. Current Rust SHA/-vV, source bytes, affinity and LLVM identity are bound. The linker claim is limited to the current ZIL pair; it does not establish a separately hashed Rust default linker. Fresh native checks, current ZIL Verify, object-hit checks and artifact restoration remain required.

Saved control logs and receipts show 30 initial passes plus 8 discovery, 7 semantic-cache and 1 repeated source-parallel pass: **46 executions, 45 distinct tests**. Affected all-targets Clippy, fmt and diff checks passed. The scratch tests compare fresh and reused instances of the new implementation; the old-algorithm equivalence conclusion also depends on the static diff review. No fresh build, test, native run or timing was performed for this final review.

## Evidence recomputation

I recomputed counts and paired statistics from `rows.jsonl` inside the final [raw archive](raw.tar.gz), rather than only accepting summary totals:

| Final window | Valid/native requests | Current ZIL Verify | Rust observations |
|---|---:|---:|---:|
| Three `confirmed-*` complete windows | 33 | 24 | 9 |
| `confirmed-warm` | 15 | 10 | 5 |
| `closed-recheck` | 12 | 12 | 0 |
| Total | 60 | 46 | 14 |

The separate intermediate `70ba1b271b19bd01f0c0c4803b490f2d218428d7` windows contain 36 complete plus 15 warm requests; they are not attributed to final production.

The five fresh warm pairs reproduce medians **316.180976 / 243.340532 / 239.154034 ms** for old ZIL / new ZIL / Rust. New/old geometric mean is **0.7948724354**, with all five pairs improving. New/Rust is **0.9645541795**, but only **2/5** pairs improve and the range is **0.668244856–1.154495989**. “Near parity” is supported; stable superiority is not. These repetitions share one edited cache history per arm.

The original closed edit +42.0% observation remains visible. The two additional independent ZIL histories reproduce edit ratios **1.012202548** and **0.951002170**, geometric mean **0.9811252823**. They neither erase the adverse point nor add new Rust observations. All 15 warm RSS observations equal **309.31640625 MiB**; the report correctly makes no compiler-memory comparison from that possible runner high-water floor and retains mixed full-window RSS results.

Archive identities were independently recomputed:

| Archive | Bytes | Regular files | SHA256 |
|---|---:|---:|---|
| `raw.tar.gz` | 7,688,157 | 1,136 | `cb307532bdba3c55aac4ee06f2c8832b0c742617d3623c44cd917f75e428a04a` |
| `native-elves.tar.gz` | 375,262 | 8 | `3c18d7e3da54b9ff52bc4820b24865e988db0633c5cbb485eb47a2d029777576` |

All eight archived ELF byte lengths and full SHA256 values match the current saved edited outputs and the paths/hashes in [native-section-bytes.json](native-section-bytes.json). Their `.text` byte hashes were recomputed from the archived payloads: all four pairs match. Two pairs are whole-file identical; for the other two, direct byte comparison finds no difference outside the recorded common `.strtab` range. The four extra closed-recheck outputs also hash to the already checked closed ELF (`5316fdd722c65fa7ef8325884508c5f5c3f19b193261ed6dbbd0c9d97970cafe`). All 15 warm rows record equality to their seed, and every restoration receipt is true. The ordinary complete-window rows do not themselves carry ELF hashes; their saved output paths are bound by the separate quality receipts and archive, whereas warm rows additionally record per-request hashes.

The [results report](../../../../docs/research/incremental-compilation/STAGE14-RESULTS.md), [provenance](provenance.json), controls and compact results agree on these counts and limits. Report local links resolve after this review file is added. Finite saved-byte equality does not establish general runtime, ABI, locality or memory equivalence. The report retains unsupported comparison shapes, default/closed export-contract differences, the initial adverse observations, the absence of a new broad corpus, the historical handoff integrity failure, and pending final CI. No universal Rust compile-time win or separate per-change causal benefit is established.
