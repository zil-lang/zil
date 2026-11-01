# Stage 8 independent review

Reviewer: `/root/stage7_scalar_review`, separate from implementation authors. This record is transcribed by root from the independent review messages; the reviewer performed read-only review and statistical recomputation, without duplicating the test/build matrix.

Exact implementation: `8c5dcc072318f9c496bdc19e14cbbbaadccccc2e`. Intermediate `f289037feaf49adccdaa9ae01ab340c2e599bd8e` separately passed review; its timings are not final-candidate evidence.

Code and focused validation: **Pass**. Review checked the current immutable Core ownership, identity-only inference result, exact Limits guard, CT classifier fallback, shared call hints, ordered declaration/statement/expression work and first diagnostics. The plain-prefix/CT fallback test with max_functions=2 detects missing allowance restoration. The final charge helper relocation is equivalent. Reviewer read all 24 focused test results and final all-targets Clippy logs; no extra execution count is claimed.

Final data: **75/75 requests and native oracles valid**, complete unique paired identities, clean crate source identity and exact compiler/LLVM hashes. Independently recomputed all cell ratios with product/root arithmetic and checked analysis.json. Target has 3 repetitions; guards have 1 each. Only 15 actual Rust observations are shared across the two contracts.

Required qualifications: target edit new/old ratios are default0.945173 and closed0.898440; new/Rust1.980363 and1.173982. Default warm1.083521, all three pairs regress (1.0396/1.0988/1.1136), CPU1.0719. Closed warm1.000506. Skew default one-pair cold/warm/edit1.048444/1.157829/1.055582. Both single8192 warm cells regress about2.5%. Do not claim comprehensive net improvement or a general breakthrough.

Target edit maximum process RSS medians are default286.2→244.5 MiB and closed269.6→228.6 MiB; paired CPU0.968993/0.895246. wait4 RSS is not simultaneous tree-wide peak memory. Warm requests have semantic hits and all object hits (default8/8, closed1/1), so cache invalidation does not explain the observed regression; attribution remains unresolved.

Final report/resource audit: Pass. The reviewer independently checked all 18 resource cells (medians, paired geometric means and ranges) against rows.jsonl and reviewed STAGE8-RESULTS.md. Default8/8 and closed1/1 warm object hits were verified. No material numerical or conclusion error found; old CI success is clearly separated from the new head pending actual execution. No new tests or builds were requested.
