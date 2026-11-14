# Native1000 full-ELF diagnostic

Both preserved windows fail on **the same eight real machine-code changes**. Stock 30c, without push switches and using the same LLVM DSO as main9170, produces exactly the same 16 object payloads and final ELF as push 1de. The leading cause is the ordinary early scalar SSA/guard/CFG lowering shared by these candidates, followed by different LLVM optimization/codegen choices. This is not metadata-only, a linker difference, or a discrepancy requiring research reuse to occur. **Keep both windows failed and the full-ELF gate unchanged.**

Scope: only preserved artifacts, source files authenticated against archived tree inventories, Python byte inspection, `readelf`, and `objdump`. No compiler/build/native execution, benchmark, Git command, or production/evidence edit. The sole output is this report. d71ce was not examined; its current root-owned window remains independent.

## Identities and preserved results

Paths below are relative to `/workspace/zil-push-main/out/runs/`:

- **P:** `large-project-large1000-edits-02-o2`, push candidate 1de; 3/246 rows, 243 unexecuted.
- **S:** `large-project-stock-large1000-02-o2`, stock candidate 30c; 3/102 rows, 99 unexecuted.

Both stop at block-00/000-cold, `valid=false`, `comparisons=[]`. Each ZIL arm reports Success/link, diagnostics `[]`, verification_runs=1 and one executed Verify event. Each of the three arms has 12 valid native observations; 16 physical ZIL units and exports match. These observations do not override `executable_equal=false`. The failure's `arm=rust` is loop context, not a Rust native failure.

| Compiler | Source SHA | Tree SHA | Archived compiler SHA256, independently rehashed |
| --- | --- | --- | --- |
| Both baselines | `9170bb4210554392e36eb7528596612596ebef80` | `9aabdb9ebf917528ffa3f0bfc2d834809fe6b2f3` | `08fcf4bc3b31590068585b63f4f4bc18def552234b9c57295b9a4b0fb4f199f0` |
| P candidate | `1de00f63f80cf4534ec995dc51e106005b270ddb` | `850ff626b6e9cc227d29b2cbe823fa769df52c8c` | `4d040bb3dea0d3ca8b4ec06e38d47c359289a74c54ab3069b7bc90fd9f0fa5f7` |
| S candidate | `30c0009ff94b3476c15f4afe2ba5d067f7f78cbb` | `9d15a0b3e70e4e0a15138d54eb7ffb7956ad7f82` | `7826460c9609fc558ac7aac461cfa329ca8f809d11df6e480de30df9881abb6d` |

The identical ZIL input is 102 files / 148,770 bytes, source identity `dcb7d753d49cee5f38c4c9cee739b5df5e6e8f05304bb0e0049cc5dc824a8a58`. Tool source is `ad81486b976634e71bf1c61544d0fd28a783fbae`.

## Exact ELF/object difference

All four ZIL ELF archives independently rehash to these two values; each is 202,752 bytes, stored at `<window>/blobs/<sha>.elf`:

| Artifact | SHA256 |
| --- | --- |
| Both main ELFs | `8b3ba23551d3a54ab07151897eb521298e2ab2a9458169d5801a30ffa851f647` |
| Both candidate ELFs | `c5b994710fee0561196ab9bf497bdbb2c7ebd687dfc3012d579b643c91eca316` |
| Main `.text` | `d78d2f4704ebbd9f0e046212ba10d605f2b414d180bb1b54513c25af30b60126` |
| Candidate `.text` | `84fb51cc55eddabaf79fe57d2481a3934af0e35103ef0d0508a7dd4eb513c250` |

There are **1,584 differing file bytes**, first/last offsets `0x7ae3` / `0x25d21`. All are inside `.text` (offset/address `0x1040`, size 155,438). Every byte outside `.text` is identical. `readelf -W -h -l -S -s -r -n` tables are identical; section layout, symbols, relocations, notes and unwind data do not explain the failure.

Of 1,005 nonempty STT_FUNC symbols, exactly eight differ. Their addresses and sizes are identical across arms. Source ownership follows main's ordered module calls, not a guess from symbol numbers:

| Symbol | Definition | CGU | Address |
| --- | --- | ---: | --- |
| `zil.fn.73` | m6.value | 8 | `0x159d0` |
| `zil.fn.203` | m19.value | 5 | `0xf050` |
| `zil.fn.333` | m32.value | 2 | `0x7ad0` |
| `zil.fn.463` | m45.value | 15 | `0x25ab0` |
| `zil.fn.593` | m58.value | 12 | `0x1f760` |
| `zil.fn.723` | m71.value | 9 | `0x19450` |
| `zil.fn.853` | m84.value | 6 | `0x130c0` |
| `zil.fn.983` | m97.value | 3 | `0xbb60` |

Each body is 853 bytes, with 198 differing bytes. Within each arm all eight bodies are byte-identical (module index modulo 13 is 6). Common body SHA256: main `f456b4c88fab545059b6a6f2f5d3e7a948c4738be558c98b97d37ef91592e588`; candidate `9901bfb8c4f2d0522576b71bb6847ce9939f0195e27b4da581efae50acbf98ff`.

`objdump -d --no-show-raw-insn --disassemble=zil.fn.333` shows 236 instructions inside the symbol in both arms, with identical mnemonic histograms, including 13 `jo`, one `js`, one `ud2`. Nevertheless the actual instructions/dataflow/layout differ:

| Address | Main | Candidate |
| --- | --- | --- |
| `0x7ae2` | `jbe 0x7b58` | `jbe 0x7b5a` |
| `0x7b18` | `mov %edi,%r10d` | `mov %edi,%r9d` |
| From `0x7b48` | `lea 3(%rdi),%r11d; lea 9(%rdi),%r9d; add $6,%edi; jmp 0x7be4` | `lea 6(%rdi),%ecx; lea 3(%rdi),%r11d; add $9,%edi; mov %edi,%r10d; jmp 0x7be4` |
| `0x7be2` | `mov %ecx,%edi` | `inc %edx` (the old join-copy is absent here) |
| `0x7be4` | `movslq %edi,%rcx` | `movslq %ecx,%rcx` |

The trap remains at `0x7e23`; main's `inc %edx` was at `0x7be0`. This combines register reassignment with changed LEA/ADD selection, copy placement and branch layout. Equal size/count and six input oracles do not prove equal machine quality or full-domain semantics.

All 16 object payload hashes match between P and S for each corresponding arm. Exactly CGUs 2,3,5,6,8,9,12,15 differ between main/candidate, each in its listed function. Extracted object bodies already equal their respective final ELF bodies. Representative CGU2 objects are main `cccd41c3b96d624c89bbc2f587b9a2ecc59ef9936607b86024b6109175af5279.o` and candidate `db3fe21ccd29f18b07a0bd0fd50ed8c255f16e20b1a88bd5e8838537a08e4d4c.o`, under each window's `blobs/`. Thus the difference predates linking.

## Probable source cause and implementation direction

S maps the **same** LLVM DSO for main and candidate: SHA256 `2bf890dac0d0a9e2bf2a81abf0a611e6893a06cbd827c525abaa1114f1ec8250`. P's candidate maps custom DSO `785a2413d05149ba84789a588f5ca2d4d6b1ef34248ca282789acd4f3133c1d2`, yet produces exactly S's objects. S uses no push switches and no native-link profile. Static receipts retain `default<O2>`, LLVM 23.1.4, x86-64, PIC and model-builder input. Missing per-stage reuse observations are unknown, not zero.

Archived LLVM isolates a concrete upstream change. `zil.fn.333`'s own 129-line definition is identical, SHA256 `9532b368a8e205ede2f1dc8823f8b560a4b19571a35159ac79eabc6bf11dbb4b`. Its helper `zil.fn.325` changes from 70 lines (alloca/store/load, constructed constants and remainder-guard branches) to 33 lines (direct argument/immediates and `%typed.phi.3.1`). Checked multiply/add overflow operations remain. Comparing only the equal aggregate definition misses its changed inlining dependencies.

| Raw LLVM archive SHA256 | P | S |
| --- | --- | --- |
| Main, 2,351,528 bytes | `d5f40782a520c48d468cbc7c6228fd1f8f57c05d4c5c7dc0332e30a45a6bd830` | `84a2804d0e511f6aff7b00c8cfae54cd92d06ba4b48ce5b006633f2b5a1060fb` |
| Candidate, 1,409,218 bytes | `4c266047015d3d42ebe5530bc60ebf9e9db4b9745bfe630b927e2bb7d2eb7c19` | `9a128502c1bf93d3c8d1aeb333d882c6e1b5092fbb88d2ce3b3739b3fc86f962` |

After the runner's exact identity-comment normalization, main IR hashes match across windows, as do candidate IR hashes; main/candidate IR still differ. This normalization is an IR observation only, never an ELF acceptance exception.

The authenticated stock implementation supplies the mechanism:

- `/workspace/zil-compiler-integration/crates/zil-backend-llvm/src/emit.rs:661` calls `typed_scalar_facts::prepare` independently of research switches; lines 1724/1808/1971 bypass local storage/Load/Store for admitted scalar SSA; lines 724–732 apply early range/cleanup processing. Main9170 lacks this path.
- `emit/typed_scalar_facts.rs:281` completes actual source SSA and emits `typed.phi` at line 341. This file's blob identity is identical in 30c and 1de. SHA256 `262fcc4f1c5bcab71c1524e6d007852b6f77f6775939d6f4b092245c1ecbe920`.
- `emit/early_scalar.rs:36` / `:123` and its SSA/range/cleanup helpers change the model LLVM receives; `zil-llvm-api/src/native/builder.rs:633` / `:693` construct and connect real PHIs. The native builder also has identical blob identity in both candidates.

These stock source files were checked against the archived 30c tree's blob IDs; no live-branch identity was assumed. Stock `emit.rs` SHA256 is `664c89301cd6d2ee4d0c6f282c1bc3d4204f06d62660f34db358433a133f348b` (main `d4a3b478f2b8870715299926b8f96a0edcf6a0316c86e81c0ab28ec1f0f1edd1`).

**Implementation direction:** localize the first divergent optimized IR/MachineIR in the ordinary scalar lowering/inlined-helper pipeline, starting with `zil.fn.333` and its changed helpers; then make SSA/guard/CFG emission produce the baseline-compatible machine result, or retain the established ordinary emission path while optimizing host-side work. Do not special-case these eight symbols/modulo classes or weaken source/overflow checks. A reuse-only change with unchanged primary IR is not evidence that this ordinary codegen gap is fixed; root must compare d71ce's actual ELF under the existing gate. Identifying a specific LLVM pass or proving wrong-code requires further root-owned evidence; neither is established here.

Both `requests/block-00/000-cold/pair-validation.json` files hash to `e61d9fa2ac8887b9c4dc6575fafd02309a69af4d39bb063cf8667614413ddd37`. Preserve their failure and the original summaries; do not accept timing/disk ratios from these stopped windows.

## Addendum: existing optimization/runtime-quality policy

**Yes: there is an established separate runtime-quality evaluation for optimizations that change machine code. Byte identity is not a universal language/product requirement. No: that policy does not turn these frozen full-ELF windows into passes, and no existing drop-in runtime-only replacement is wired into `large_project_matrix`.** The earlier baseline-compatible implementation direction concerns satisfying that experiment's existing gate, not a claim that every legitimate compiler optimization must preserve every machine byte.

### Authorities and exact numerical boundaries

References below use the inspected `/workspace/zil-push-main` source unless an absolute goal path is given.

| Existing authority | Requirement / scope |
| --- | --- |
| `docs/guide/compiler-cost-contract.md:13`, `:15`, `:19`, `:25` | Generated runtime quality is independent of safety/functional CI. Preserve inactive/low-activity, moved-field, repeat-call, allocation-failure and cleanup neighbors. Freeze corpus/frequencies/runtime constraints before measurement; retain failures without deleting/reweighting cells. Preserve current independent Verify and machine preflight. New workloads must be reported separately from the original protocol. |
| `docs/guide/build.md:79`, `:85`, `:100` | Full O2/import policies may trade compile cost against runtime optimization opportunity. Measure generated runtime quality and weighted **complete-request** waiting/absolute savings; research results do not automatically change defaults. There is no general machine-byte-identity requirement here. |
| `/workspace/zil-compile-goal-continuation/records/compiler-next-optimization.json:216`–`:244` | The actual private-postlink runtime protocol explicitly retains **B/A ≤ 1.10 per case** and **R/B ≥ 0.90**, where A=baseline, B=candidate, R=Rust runtime. It records 6 compilations/54 native runs, 36 formal, and runtime Pass; this alone did not establish compiler-speed adoption. Source `1cc866a08bac100012e41726067711c94caf1899`, protocol SHA256 `208adf6f5a3e904182afdaf1ef1c6b30a173fb559400ab14d05b2f59e4803a44`. |
| `/workspace/zil-compile-goal-continuation/records/compiler-multi-edit-continuation.json:482`–`:509` | A concrete separate runtime screen: 64/256 source files × edits 0/1/16/64; unchanged pure function bodies, bounded dynamic loop added only to main, independent periodic numerical oracle. 100,000 iterations, seed period 251, accumulator modulus 1,000,003; one warmup/arm, six formal samples/arm, all six A/B/R permutations. Same per-case 1.10 regression / 0.90 Rust-efficiency limits. Preserve all eight cases; failed runtime gate stops downstream expansion. These are that finite protocol's settings, not automatic settings for native1000. |
| `validation/product-gates.json:10`–`:14`, `:50`–`:66` | Product runtime efficiency is **T_Rust/T_ZIL ≥ 0.90 per case**, geometric mean **≥ 0.95**, with memory/tail-latency profiles. Exclusive timing, cold/warm separation, declared sample counts and actual execution remain required. This Rust-relative product threshold is distinct from candidate/main ≤1.10; 0.90 efficiency is not mathematically the same as a 10% slowdown limit. |

The 10% gate has actual adoption/rejection history, not just aspirational text. `/workspace/zil-compile-goal-continuation/records/compiler-cleanup-merge.json:184`–`:192` records PR76 benchmark run **42785071418**, source `0caf3711441b9744ae1ba1b1699bb09c0ce839de`, and accepted owned32 runtime about **5% slower**, explicitly inside the predeclared 10% limit. `goal.md:543` retains that boundary; `:544`–`:545` retain rejected 10.6%–35.6% / 18.3% regressions. `compiler-next-optimization.json:542` onward preserves full-producer `2458def…` as runtime NoGo despite valid outputs and a compile-speed improvement. Functional CI alone was never the substitute.

**Confidence-interval qualification:** the inspected current authorities and goal runtime records specify the numerical ratios above and concrete balanced sampling, but do not establish a universal “upper 95% CI ≤1.10” rule. The bootstrap intervals in `records/compiler-cost-weighted.json:12`–`:40` concern frequency-weighted **compiler waiting-time reductions**, not a generic machine-runtime admission CI. A claimed replay of an original runtime/CI protocol must use that protocol's actual pinned estimator and decision rule; a new CI cutoff cannot be inferred from these records. This review did not recover/execute the historical archived runtime harness or inspect its CI job logs. The modern functional workflows are not themselves that runtime-quality measurement.

### What can be measured separately, and what remains open

A new, separately identified, predeclared paired runtime-quality window is consistent with the existing cost policy: bind actual current candidate/main binaries and tools, keep full O2/Verify/preflight and semantic/error contracts, retain all declared original and relevant neighboring cells, numerical outputs, runtime samples/ratios, memory/tails and negatives. It may establish bounded runtime quality for nonidentical code without claiming identical code or rewriting either failed window. The old finite runtime protocols provide a documented threshold/measurement precedent; they do not automatically qualify today's different corpus, compiler revision or product scope. This is an evidence distinction, not a new approval workflow or permission requirement.

The current gaps are specific:

1. The two native1000 windows contain functional six-input × two oracles, not a sustained, balanced paired runtime-quality acceptance at the existing regression thresholds. Equal 853-byte bodies/236-instruction counts do not supply the missing runtime result. Existing failures and unexecuted histories remain failures/missing evidence.
2. The separate `late-closed-v1` comparator is not an arbitrary different-code acceptance mechanism. `tools/research/incremental-demand/elf_instruction_quality.py:1`–`:6`, `:457`–`:458` require retained instruction/CFG/address-binding equality; `:482`–`:486` also require equal current normalized LLVM/source inputs. Today's differing helper IR and register/LEA/copy changes do not satisfy those premises.
3. `native_link_matched_measure.py:247`, `:255` explicitly requires complete ELF equality; `large_project_matrix.py:431`–`:437` stops on its failure. `native-link-matched.md:66` onward documents the same policy. There is no implicit runtime-threshold fallback in that frozen runner. Preserve both `valid=false`, their original `comparisons=[]`, and the exact-code experiment's unresolved result even if a future separate runtime-quality profile passes.
4. Current goal `records/compiler-incremental-demand-continuation.json:62`–`:67` explicitly leaves both native1000 failures, warm regressions, global push and main integration open. The accepted default-budget 5105 Source→Verify→native capacity result is not runtime-performance/code-quality acceptance for the changed native1000 bodies. A new bounded runtime Pass would also not by itself prove the whole product Rust-relative, memory/tail or compile-cost goals.

No new threshold, gate edit, blanket semantic-equivalence proof requirement, approval flow or merge authorization is introduced here. No new measurements were run. The original full-ELF failures and main-merge quality gap remain unchanged.
