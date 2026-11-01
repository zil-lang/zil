# Independent Stage13 review

**Scoped code, controls, statistics and saved-artifact review: Pass.** Production source is `4bbbce204bb468d83eaebb78573c98104c3de348`; the measured baseline is Stage12 delivery `4cb3687127a1980b2ace965af9fdc8c8e5de93c5`, production `743c9699eda7b5ef79b0759702cb48a17acfb83d`. This is not full-language, industrial-workload, full-workspace CI or product-gate approval. Final archive/delivery binding is a separate check.

The non-author reviewer and delegated non-author reviewer read the source, existing control logs, measurement records and saved artifacts. They did not build, run tests or compiler/native workloads, invoke native inspection tools, or create timing samples. Independent calculations and ELF reads used existing data only. This review file is the lead reviewer's sole authorized repository write.

## Code and compatibility boundary

- The safe SHA wrapper preserves new/update/Clone/consuming-finish and 32-byte digest interfaces. FingerprintBuilder Default, observational Hasher::finish, Hash byte streams, buffered reset, cache fields and verification authority are unchanged. The old scalar source is byte-identical to the baseline (`722ba20d7403374f52d914a4380155d9bf5d7402536125b02c33627feeea7464`) and compiled only for tests.
- Exact registry `sha2 0.11.1` uses no default features; actual resolved metadata reports `features: []`. The lock adds nine registry packages, including dependencies for other targets. The fixed upstream source audit covers the safe API, state/buffer Clone, finalization, CPU detection and software fallback; it is not a new independent cryptographic security audit. No ZIL unsafe allowance or internal dependency-policy exception was added. The existing policy permits registry dependencies, and the authorized integrator owns the lock update. README/delivery instructions now require the matching registry cache or initial `cargo fetch --locked` before offline builds.
- Byte compatibility is scoped to existing bounded compiler inputs and the FIPS SHA-256 message domain below 2^64 bits. Upstream ordinary length arithmetic can panic with overflow checking for a cumulative stream at or beyond 2^61 bytes, whereas the former implementation wrapped explicitly. This extreme public FingerprintBuilder difference is disclosed, not claimed equivalent. Compiler budgets and acceptance checks are unchanged. Software-path correctness controls do not establish performance on machines without hardware SHA.
- Preflight Definition/result/projection types borrow from the same immutable Module. Result lookup, type validation, identifier/duplicate checks, GEP operand/index/projection checks, CFG/SSA/dominance and first-error order remain intact. Full current preflight is retained; no cached schedule or former proof authorizes another model.
- Checked-add candidates borrow immutable block names and match the exact same pair/type shape. Only accepted owned replacement plans allocate their names. Shape, sign/width/literal rules, adjacency, uses, effects, cycle/Phi checks, work bounds and validation-before-mutation remain unchanged.

The reviewed wrapper SHA is `9b8034f60c93b5cf76ebed3a68b96362ee1209f0bfc0cda59117f1cc6fc08138`; Cargo.lock SHA is `99b70f5eae3be5aec5d26185cf3ac4a875e3e508751767f11c2204dc2a806fde`. They match the frozen source. No unresolved code blocker was found within the stated domain.

## Bounded validation

Actual saved stdout and command receipts confirm **62 different scoped controls passed**: base 10 (8 unit + 2 integration), preflight 20, checked-add chain 13, object cache 10, object patches 5, and four selected real-source/native controls. The forced `sha2_backend="soft"` invocation passed the same 10 base tests separately; those are repeat executions, not 10 additional different tests. Existing known vectors and the retained scalar oracle provide different checks; streaming boundaries, forks, non-destructive Hasher::finish and continued updates are covered without infeasible huge-stream testing.

Four-crate all-target Clippy with `zil-driver/llvm-api`, fmt, diff, actual Cargo metadata and architecture checks all completed successfully. The architecture receipt checks 15 crates and 38 edges with zero errors. These are scoped local checks, not a full-workspace test run. The reviewer inspected the existing logs and did not execute them again.

## Measurement identity and independent recomputation

All three final-window manifests bind the production source and an empty production working diff. Independently read artifact hashes match every manifest:

| Artifact | Bytes | SHA256 |
|---|---:|---|
| Candidate compiler | 8,326,112 | `1203a4a526031a666a14b68489f8954dbe24ae257cbab65a29587aa3c2a41d70` |
| Baseline compiler | 8,339,928 | `725aaa5558107d421c81518c4b97fe4734b7057d7be4f94157ac9e2f04118baf` |
| Rust compiler | 21,139,856 | `5be191e8c70a5007074254cb74d78dbfdd25cd5a61c27de7b8e5410122ec24f4` |
| LLVM DSO | 72,097,832 | `7b5c5ef8b204e019c58da7bbb01853dc56fa68fcec08364c5d07b10494fa9a98` |

The unchanged complete-request runner SHA is `7216f6a1c7f14d6d37af0e4fe6deffd96fb35c16e9150785660a7a2609481ef5`. ZIL uses the recorded research LLVM23.1.4 DSO; Rust is 1.99.1, commit `58cf8f9c35b355199727627d07ae34a4e2846d67`, LLVM23.1.1. CPU affinity is [0,1,2,3,4]. The explicit O2/incremental/overflow settings and default versus closed retained-symbol contracts must accompany comparisons; the compilers do not use identical optimizer pipelines or export contracts.

Independent calculations checked unique request tuples, counts, exit/timeout/native validity, all 15 cell medians and paired geometric means/ranges, and all CPU/RSS summaries. Every supplied value matches. Existing ZIL stdout reports show one current Verify per request; saved native output agrees across all three arms within each observation, and native stderr is empty.

`final-target` has 27 requests, `final-guards` 27 and `final-closed-guard` 9: **63/63 valid build/native observations, 42 current ZIL Verify runs, 21 separately measured Rust requests, seven matched cache-history sets and 15 comparison cells**. There are three independent histories for target default; each other case/contract has one. Cold/warm/edit within a history are dependent. Rust rows are not reused between windows/contracts. These yield 21 candidate/baseline pairs, not 63 independent speed samples.

| Target default request | Candidate/baseline GM | Paired range | Candidate/Rust GM |
|---|---:|---:|---:|
| cold | 0.961949005 | 0.799707–1.141709 | 0.951030751 |
| warm | 0.860290277 | 0.837286–0.877439 | 0.535831112 |
| edit | 0.835667061 | 0.794770–0.860837 | 0.773488023 |

All three target warm and edit pairs improve versus baseline. Target edit/Rust pairs are 0.752228, 0.766931 and 0.802149; medians are candidate405.297/Rust511.209 ms. This supports repeated superiority for this fixed workload and configuration, not universal superiority. Cold includes a +14.171% baseline-relative pair and a 1.043253 Rust-relative pair; its broad range does not support a stable cold win.

Target CPU geometric ratios are 0.972989/0.851210/0.807212. Peak-RSS medians, baseline→candidate, are cold213.688→214.039, warm135.512→135.477 and edit200.461→199.871 MiB. wait4 maximum RSS is not simultaneous whole-process-tree memory. Three combined implementation changes were not ablated; neither private nested profile intervals nor these request totals identify each change's separate contribution.

The opaque cold guard is adverse versus baseline: ratio1.029591 (+2.959%). Its warm-before-edit request is 297.209 ms versus Rust255.096 ms, ratio1.165083. There is one adverse cell point estimate and two adverse individual pairs versus baseline. Fourteen of fifteen cell point estimates are faster than Rust; this count includes single-history guards and different export contracts, and is not a confidence or whole-language result.

## Focused warm follow-up

The follow-up is separate from the matrix. `stage13_warm.py` preserves the existing wait4 measurement function exactly, rotates arms, binds compiler/LLVM/source identities and current CPU affinity, and restores the seed ELF/LLVM files in finally. Review identified missing linker binding and a too-weak miss-only eligibility check before execution; the final tool fixes both. It resolves the same actual clang from both seed reports, records its SHA and forces ZIL_CLANG. The actual clang SHA is `c336d48383874cffaab0dc4c5c359456d871f21380b746520dde5c7f4c125a67`. Accepted ZIL requests require Analyze Reused, one current Verify, cache enabled and hits==actual_units==8 with zero misses. Invalid rows are retained and stop successful aggregation.

The reviewed and executed tool SHA is `ff8402e7d8095b7004ce0161f2bc6fb02b8f3b79a45ffacf56f110c3cbcb9210`. Independently recomputed results confirm **15/15 valid/native requests, 10 current ZIL Verify runs and five Rust requests**. All 15 binaries equal their own seed bytes. Current source hashes and all five restored seed ELF/LLVM hashes match the manifest; the original quality inputs remain intact.

There are **five repeated pairs on one edited seed history per arm**, not five new independent histories, and they are not pooled with warm-before-edit. Candidate/baseline GM is0.814215402 (range0.739202–0.913801); all five improve. Candidate/Rust GM is1.207349912 (range1.173591–1.250530); all five remain slower. Medians are baseline340.749, candidate269.444 and Rust220.021 ms. This confirms a remaining gap on that fixed warm path while avoiding new heavy cold/O2 samples.

## Finite native quality

Existing quality results cover seven edited pairs, with 42 recorded inspection commands completing successfully. The delegated reviewer independently read all fourteen ELF bytes and checked their lengths/SHA, actual section tables and whole-file differences. Canonical function/global sets, kinds and sizes agree; effective-body differences and all size deltas are zero.

All seven `.text` pairs are byte-identical. Opaque and closed whole ELFs are identical; the other five differ only inside corresponding `.strtab` payloads, with zero differences in headers, gaps or other sections. Single8192 retains **146 raw instruction-label differences**, all independently checked to be single-jump private retired-symbol spelling changes. Addresses, sizes, jump terminal addresses and effective bodies agree. Existing Stage11 retired-body jump cost remains; this change does not remove or add it.

The bounded disassembly comparison retains its exclusions: even/skew8201/8203 and single8193/8195 functions comparable, with main+thunk excluded; opaque2050/2051, excluding the function with indirect getchar/putchar calls; closed0/1, excluding main with indirect putchar. Exact saved text bytes provide stronger evidence for these finite artifacts, not a general runtime, ABI, address/layout, locality or memory proof.

Full quality-results SHA is `81725f945ea4c6f1872536c74a1952e5ec7a9483f59a752fcbc863bb0e5bc0ad`. Review caught copied Stage12 labels in the first section-reader receipt. The author corrected the script/receipt identity to Stage13 production and reran only that read-only byte inspection; its pair results are identical to the original. The old-label receipt is retained as superseded evidence. Current script SHA is `9bb3eb3699500d2f1c99470cc4f2ba9f46c96c163062d8668d09b16d36e6bf1e`; current receipt SHA is `6b26200c7ff7d7f172885dfd1513d3e000cbef1857971b362669a335c681661c`.

## Claim and scope review

The current Stage13 report's numeric table, paired interpretation, adverse observations, fixed-workload conclusions, dependency/FIPS limits and quality exclusions agree with the independently checked evidence. Remaining work is the opaque warm machine-construction/key/preflight path and broader frontend/workload coverage; the evidence does not authorize omitting current Verify/preflight or silently dropping LLVM text artifacts. No new graph-cut/zit attribution, broad current-version check/O0 matrix, industrial-project claim or four-goal product approval follows from this patch.

Previous official41305054015 and research41305046739 both succeeded for Stage12 delivery `4cb3687127a1980b2ace965af9fdc8c8e5de93c5`. Those statuses do not approve this production or forthcoming delivery commit. Historical handoff completeness Fail remains. Main/environment publication and product gates are not changed by this review. Final archive/link/provenance and delivery identity checks remain to be recorded after the packet is complete.

## Final packet binding

**Packet review: Pass**, bound to production `4bbbce204bb468d83eaebb78573c98104c3de348`. The production/Cargo.lock diff against that source is empty. The report, index/task additions, provenance and this review retain matching counts, identities, paired estimates, negative observations and limits. All eleven local report links resolve, including the referenced Rust-reading heading. No material claim or numeric error was found.

The raw archive independently hashes to `4e49de4c95044378a9dcd7ff2825c39b479780ea73ca9f40541d7a7fbd2736b8`: **9,516,966 bytes and 1,002 unique regular files**. Twenty-seven selected critical archived records match their originals byte for byte, including every final/follow-up window's rows and metadata, effective and forced-software control receipts, architecture/metadata, prior CI, full quality results, and both corrected and superseded-label section receipts. Every public window metadata copy also matches its original. Archive paths are relative and contain no parent traversal; object/semantic/incremental caches, compiler targets, private profile-source trees and native program files are absent from this raw attachment.

Every provenance-bound current binary, tool and Cargo.lock length/SHA matches its actual file. All nine registry-package records exactly match Cargo.lock. Public control and prior-CI records match the original logs. The current section-byte receipt is archived and publicly copied byte-identically at `native-section-bytes.json`, SHA `6b26200c7ff7d7f172885dfd1513d3e000cbef1857971b362669a335c681661c`; its corrected production/script/results bindings are consistent. The public native-quality projection preserves all 1,790 retained scalar fields exactly, and all 28 added canonical-list counts/digests match the full record bound at SHA `81725f945ea4c6f1872536c74a1952e5ec7a9483f59a752fcbc863bb0e5bc0ad`.

The actual-native attachment independently hashes to `4548bb71da9eb7c0fc8057baaf10339c4f4eb4e3ecda320e379b423b5a6d6707`: **829,084 bytes and fourteen unique regular files, with no links**. Its paths exactly cover the seven reviewed baseline/candidate ELF pairs relative to the Stage13 root; every archived ELF length and SHA matches the complete quality record. It supplies the actual inputs needed to reproduce the archived byte-reader conclusions without rerunning compilation.

This final check added no build, test, native inspection or timing run. The old successful CI records remain baseline evidence only; the forthcoming delivery commit and its own CI status require a separate delivery receipt.
