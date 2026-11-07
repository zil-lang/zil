# Frontier loop 154–160: what to adopt, and what is still unproved

Baseline is exact compose153 production source. 154 reverse-user indexing passed database differential checks but its full-compiler request interval spans 1; it remains isolated. Merely improving theoretical invalidation complexity is insufficient when content-addressed query keys often create fresh entries instead of updating a stable logical query. The recorded tree1024 cache counts are 1029 -> 1029 -> 1034 -> 1551. These counts motivate investigation; they do not by themselves prove that no invalidation ever occurs.

158 attacks demonstrated repeated work before the query layer: file discovery parsed current immutable source before asking an already-cached Parse query. It now reads files/import paths afresh, validates exact query identity and quota axes, lends immutable syntax, retains the original capacity-clear boundary, and independently verifies every request. This is implemented and measured, not a new general graph-cut theorem.

## Current primary references

* Salsa official red-green algorithm, https://salsa-rs.github.io/salsa/reference/algorithm.html : reuse unchanged inputs; backdate only when recomputation produces equivalent output. Durability can reduce dependency traversal. Our exact-input syntax reuse is adopted; output-based caller cut is not yet implemented.
* FlowLog, arXiv 2607.23971v1 (27 July 2026), https://arxiv.org/html/2607.23971v1 : relational IR separates recursive control and rule plans, then uses Differential Dataflow for incremental maintenance and profiling. Relevant to existing effect-fact extraction and deletion research131. Do not import the paper's benchmark gains as ZIL evidence, or assume maintaining a cyclic support count is sound.
* DBSP, arXiv 2203.16684, https://arxiv.org/abs/2203.16684 : algebraic incremental view maintenance is a candidate for persistent compiler relations; not integrated here.
* Optimism in Equality Saturation, PLDI2026, https://pldi26.sigplan.org/details/pldi-2026-papers/59/Optimism-in-Equality-Saturation : naive optimism can be unsound; the paper supplies a corrected analysis for e-graphs. It is not evidence that optimistic safety checking or a larger e-graph will reduce ZIL compile latency. Candidate-generation only; independent Verify remains authoritative.

## 160 read-only interface-cut audit

`interface_audit.py` examines actual recorded private/public edit requests. It counts module bodies whose implementation fingerprint changed while public signatures, contract fingerprints and exports stayed equal. This locates potential cuts in real compiler inputs; it neither installs reuse nor proves current summaries complete.

Before a production cut, split stable logical query identity from versioned input/output evidence, prove every private lookup and generic specialization dependency, keep source-map/diagnostic remapping exact, and bind resource origins/backing identity, target/rules and limits. Rebuild the edited provider; never call an unchanged signature a proof that its old body is reusable. New function numbering and module discovery order need explicit relocation rather than guessed stable IDs. Every assembled module still enters fresh independent Verify.

A future benchmark must change called helper bodies, exported effects, source order, private visibility, aliases and failure/recovery paths. Freeze workload weights and successful coverage separately from timing. Neither the observational cut count nor any microbenchmark can stand in for full request, RSS and end-to-end measurements.

## Executed audit

The read-only audit observed 24 edit transitions; 19 had at least one changed
implementation fingerprint with unchanged observed signature, contract and
exports. In the 1,025-module private edit, all public triples remained equal
while one provider implementation changed. This is an opportunity count, not
proof that those triples cover every dependency, not implemented caller reuse,
and not a measured speed improvement. Independent Verify must remain fresh.
