# CP04 independent review plan

Reviewer: `/root/backend_audit`. Implementation author: `/root/frontend_audit`.
Authority: `/workspace/scratch/037bacfed72a/zil/docs/research/compile-performance/CP04-PLAN.md`, root coordination commit `0a8fcce1f`.
Implementation baseline: `9614aac90ee58359fef05da4dcfe92011b7c91c2`.

This is a review and evidence plan. No implementation approval or performance decision is recorded here. The reviewer does not edit the candidate implementation and has not run cargo.

## Static review against a fixed candidate SHA

| Boundary | Required evidence |
| --- | --- |
| Exact change | Diff is limited to the authorized O0 pass recipe, shared recipe accessor, cache namespace, and necessary tests. No unrelated optimization or LLVM version change. |
| Actual recipe | The string supplied to `LLVMRunPasses`, `receipt.pipeline`, and object-cache recipe hash comes from one accessor. O0 is exactly `default<O0>,function(mem2reg)`; O2 remains exactly `default<O2>`. |
| Codegen target | Generic x86-64 CPU/features, target triple/layout, PIC/default code model and TargetMachine optlevel 0/2 are unchanged. |
| Verification | ZIL independent Verify, machine preflight, and LLVM Verify before/after passes remain in the same control flow. No new noalias/assume or custom alloca deletion. |
| Failure/cancellation | Error category, cooperative cancellation points, failed build cleanup, current verification on object hit, and success-only cache/publication boundaries remain intact. |
| Cache authority | Namespace protocol version changes; actual compiler and LLVM shared-library identities and all previous recipe components remain. A test holds optimization and other namespace inputs fixed while changing the recipe. |
| Observation | `llvm_optimize` includes the new pass cost. Timing fields do not claim that pre-optimization `.ll` proves promotion. Parent/child scope and default-disabled timing behavior remain unchanged. |
| Public surface | Only the authorized additive recipe accessor is introduced. Language IR/schema, Cargo.lock, budgets, acceptance and ABI are unchanged. |

## Directed production validation coverage

Author tests and root-run affected closure are separate from this review. Their actual commands, exit codes and exact source SHA must be attached before an execution conclusion.

| Case | Purpose and required oracle |
| --- | --- |
| Loop and branch merge | Exercise loop-carried and branch-merged locals that need PHI construction; compare actual exit/output against a fixed expected result, Clang O0 reference, API O0 with one/multiple CGUs, and API O2 control. |
| Actual address use | Borrowed/mutated local passed through a real pointer-using call; verify changes remain visible, rather than using a source whose address operation lowers away. |
| Resource and field cleanup | Reuse a source fixture such as `177/field_exit` with observable cleanup; compare output/order and exit result across the same implementations. |
| Checked arithmetic trap | An input that really reaches overflow; compare the expected trap/signal, while positive near-boundary input still completes. LLVM optimization must not turn this into wraparound or successful return. |
| Cache miss/hit | New namespace recipe is exact; cold compile then hit still executes current ZIL Verify and machine preflight. Old compiler identity and old recipe must not be accepted as the new recipe. |
| Unchanged O2 | Exact pipeline/accessor assertion plus real native behavior control; no secondary O2 pass or target option change. |
| Failure publication | Existing affected native controls for link failure/cancellation and preservation of old published output/cache remain applicable; run only the required affected closure under root coordination. |

Do not infer runtime quality from instruction counts, byte size, shared LLVM implementation, or passing LLVM Verify. The separate frozen runtime protocol is required. Compile-time measurement uses timings off and accounts for additional pass cost in the complete request.

## Independent text diagnostics already executed

`diagnose_opt.py` executes the exact LLVM 17.0.6 `opt` executable from the same package build as the installed toolchain. `ldd` confirms that this executable loads the existing production `libLLVM-17.so.1`. Target triple/CPU/features/data layout are read from each CP02 production report, with PIC selected explicitly.

Both semantic pipelines were wrapped with explicit `verify` passes and `--verify-each`. No LLVM object generation, linking, native program execution, or ZIL Builder compilation occurs in this diagnostic. Each stdout stream is hashed and counted without persisting full post-pass IR.

| Input | Recipe | alloca | load | store | phi | Total instructions | Exit |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| tree1024x8 | default O0 | 10,246 | 19,465 | 19,464 | 0 | 242,774 | 0 |
| tree1024x8 | O0 plus mem2reg | 0 | 0 | 0 | 3 | 193,602 | 0 |
| retained | default O0 | 27 | 60 | 70 | 0 | 414 | 0 |
| retained | O0 plus mem2reg | 6 | 26 | 18 | 2 | 309 | 0 |

All four invocations completed with verification enabled, and all instruction lines were classified (unknown count zero). Within each input, block count, `llvm.trap` call count, overflow intrinsic call count and every opcode count other than alloca/load/store/phi were unchanged. The retained input leaves six aggregate stack slots; their printed forms are recorded in the JSON. These are structural observations, not a semantic equivalence or performance verdict.

## Evidence inventory

- `opt-diagnostic.json`: exact commands, source/production-report hashes, target settings, semantic and invocation pipelines, tool/shared-library/script/wrapper hashes, stdout hash/size/counts, stderr hash, and actual per-command exits.
- `diagnose_opt.py`: frozen independent diagnostic implementation, whose own SHA-256 is in the receipt.
- `opt-version.stdout` and `opt-ldd.stdout`: actual tool version and loaded shared-library path.
- `*-baseline.stderr` and `*-candidate.stderr`: raw diagnostic stderr (including any errors if present).
- `opt-provision.json`: official package URL, indexed/actual package SHA-256 and size, extracted executable SHA-256 and size; the archive was removed after successful extraction.
- `opt-provision-01-network-restricted.json`: preserved initial network-restriction failure; the explicit network retry succeeded. No shared toolchain file was changed.

No large post-pass IR is retained. These files are scratch review evidence for root integration, not a product gate receipt.
