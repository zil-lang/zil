# Stage 11 independent review

Reviewer: `/root/stage7_scalar_review`, separate from the implementation authors and root integration. Read-only cache-lifecycle and offline-quality subreviews were delegated to `/root/stage7_scalar_review/stage11_cache_review`. This record was written by the reviewer. No compiler build, test, native-tool invocation, workload execution, or new timing was performed by either reviewer.

## Source and limited controls — Pass

Final production: `082efcfc0f8e0408844a22af8bcc34d29c0d9df4`. Baseline delivery: `21ecc60fffe396920ddc4ed3aa9269b536a34433`, production `8cf48baf2b5fd6fa944e4c65a6116907f1cf53d7`.

The bounded source review covered old/current reverse dependency closure, unchanged ABI/declaration/order skeletons, rejection of outgoing patch-body dependencies, preservation of old LLVM merging thunks, exact canonical symbols, current full-machine preflight, patch LLVM verification, cache provenance, cancellation and publication. No remaining blocking correctness issue was found in this admitted scalar domain.

The reviewer identified a real full-base provenance race: a delayed derived object could replace a complete object under the same action key after its base record was published. The final implementation binds the record to the SHA-256 of the exact full object bytes and checks the loaded bytes before staging or external tools. The deterministic delayed-overwrite cache control and separate digest-mismatch/no-tools/no-output control cover this boundary. Derived objects never update the full-base record. Optional metadata failures do not revoke a successful compile; pending writes remain subject to existing successful publication and cancellation boundaries.

The ELF reader bounds offsets, sections, strings, symbol counts and cumulative name bytes. Full base, patch, renamed base and combined output checks require the appropriate exact global function set; retired definitions must be unique local functions. Unknown globals, weak symbols, COMDAT/extended-index shapes and pre-existing private patch chains cause fallback. The reader checks symbols rather than rewriting or independently proving relocations; transformation semantics rely on the identified installed GNU tools and the retained complete old object.

Existing logs show 16 distinct controls passing: 4 patch, 9 object-cache, and 3 real-source controls. The real-source export control selects an actual disassembled thunk jumping to the edited merging representative, checks current exported results and overflow, cumulative edits against the same full base, restoration, damaged metadata and current-source rejection. Existing caller/cache and resource/address controls also pass. Following the tool-selection change, the affected export control was rerun successfully and all-targets Clippy passed. Repeated execution of the same control is not counted as an additional distinct test.

Intermediate `a4cadf3e8ba2d59c4030b15129adee75f56e147a` used the SDK linker without a GNU build ID. Its activated mechanism window took 2213.4 / 1158.1 / 1297.9 ms for cold/warm/edit and must remain a negative intermediate, not final evidence. Final `082efcfc` selects canonical `/usr/bin/ld.bfd`, retaining the same complete installed-identity rules, namespace binding, `-r --build-id=none`, full-byte digest and ELF checks. No persistent metadata-only tool-identity cache was introduced.

## Final statistics and identities — Pass

The reviewer independently recomputed all 12 cells from `final-target`, `final-guards` and `final-closed-guard`: 54/54 requests and native observations are valid, all 36 ZIL requests have current Verify=1, and 18 Rust observations are unshared between windows/contracts and come from six independent cache histories. Cold/warm/edit within a history are dependent observations. Every expected case/contract/history/request/arm tuple is present exactly once. Medians, paired geometric means and ranges, CPU and maximum process RSS agree with all saved analysis/resources JSON.

All three manifests agree on final source, empty crate diff, runner and tool identities. Independently read file SHA-256 values match:

- Candidate: `e871f7da42854e29ec4a3118c711e9546cd48ebbb63b712c950930bf0d5d8594`.
- Baseline: `15a988bcb1b42c6f01be3659728fdb074c2369955ef48b2395c8a408863e66dd`.
- Rust: `5be191e8c70a5007074254cb74d78dbfdd25cd5a61c27de7b8e5410122ec24f4`.
- LLVM DSO: `7b5c5ef8b204e019c58da7bbb01853dc56fa68fcec08364c5d07b10494fa9a98`.
- Runner: `7216f6a1c7f14d6d37af0e4fe6deffd96fb35c16e9150785660a7a2609481ef5`.

Uniform default new/baseline cold/warm/edit = **1.020382532 / 0.988928584 / 0.712722138**. Paired ranges = **[0.996802008, 1.068163548] / [0.919171088, 1.038775370] / [0.665218716, 0.751853039]**. Thus all three edit pairs improve versus ZIL baseline, while cold and warm contain adverse pairs.

Uniform default new/Rust = **0.952728455 / 0.674489918 / 0.977641765**. Edit new/Rust pairs are **1.020803053 / 0.968344659 / 0.945294906**; candidate median is **531.459 ms**, above Rust's **520.628 ms**. This supports near parity in this window, not a stable win over Rust. Three histories do not establish a population confidence interval or a product gate.

Keep the single-history adverse observations: single-module default cold **+3.14%**; skew default cold **+11.92%** and warm **+3.74%**; closed uniform cold **+0.32%** and edit **+1.51%**. Do not pool different repetition counts or relabel old broad matrices as current. Target CPU ratios cold/warm/edit = **1.033180 / 1.000507 / 0.581199**. Target RSS medians = **212.11→212.46 / 135.53→135.79 / 211.65→198.63 MiB**; these are process maxima from wait4, not simultaneous whole-tree peaks.

## Existing native-quality observations — bounded Pass

The reviewers inspected `stage11_quality.py` and existing saved nm, objdump, section and JSON records; no native inspection commands were rerun. Six pairs retain identical canonical symbol sets and kinds; old private symbols are local. The method follows only bounded, single-instruction direct jumps to sized function entries, detects cycles/ambiguity, preserves immediates and symbolic/self-relative control destinations, and explicitly leaves indirect control unclassified.

Comparable effective bodies have no instruction or effective-size differences: 8201/8203 functions for even/skew default, 8193/8195 for single-module default, and 0/1 for closed. Default main and its canonical thunk, and closed main, remain uncompared because of indirect putchar calls. Their retained normalized instruction hashes match, which does not upgrade them to fully compared bodies.

Effective-body equality deliberately excludes thunk hops. In the inspected single-module output, `zil.fn.105` changes from a 9-byte checked-increment body to a 5-byte jump to a local retired body with those same checked instructions; 146 canonical chains reach that retained body. This is a real extra-jump/layout tradeoff, not proof of unchanged runtime quality. The report retains 293 raw body-label differences and two raw-size differences in the single-module pair; even/skew default each retain one raw body and size difference.

Each default pair grows `.text` by **80 bytes** and the ELF by **712 bytes** for single-module or **904 bytes** for even/skew. Independently read bytes of all 12 existing executables match their recorded SHA-256 values and lengths. Exactly one pair, closed, is byte-identical; its sizes are unchanged. These observations support the bounded implementation's semantic and export checks; they do not prove general runtime speed, locality, address or ABI equivalence.

## Interpretation and next priority

The measured edit improvement is substantial, with the target now near Rust, but there are cold-request regressions and modest code-size/thunk costs. Prioritize identifying and reducing new cold/metadata/tool overhead before expanding the admitted patch domain; the single-history skew regression alone does not establish its exact cause. Preserve full-base digest binding, old/current closure and current checks. Choose further backend or persistent semantic-query work from remaining measured phase costs, not from a claim that this subset already represents universal Rust parity. Final report/link/archive and delivery-commit review remain separate from this source/statistics/quality record.
