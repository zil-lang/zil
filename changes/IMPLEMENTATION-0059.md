# 0059: finite joint responsibility relations and exact event cuts

Status: completed bounded research, current candidate **NoGo for production core**;
no production acceptance, schema, ABI or gate change.

Authority: main `.task/impl/059.md`. Source baseline is
`6acda08c72dbd3e387893b4260770f0834dcdbe0` (schema 8). The independent workspace
uses production parsing/expansion/Types only to obtain real TypedIR, plus public
Analyze/Verify for separately labelled background checks. It does not call private
resource kernels, install a pass, or change 055's frozen measurements.

## Problem and proposed experiment

The production resource kernel merges per-carrier debt and provenance sets. This
finite conservative operation can forget which branch selected which owner. The
research asks whether complete finite joint relations can instead be manipulated
through small event interfaces, and whether doing so pays for its extraction,
planning, checking and output costs. The frozen experiment now answers that
question negatively for this implementation; the broader structural hypothesis
remains open.

The hypothesis comes from the user's physical-flow intuition and is tested against
Deng/Hani/Ma, arXiv:2408.07818v3. Its free/fixed interface preserves equality of the
same variable; it is not an ownership theorem. The compiler bridge and its extra
premises are stated in `docs/research/059/BRIDGE.md` before implementation.

Implemented an independent research executable with (1) an exact symbolic relation
for a bounded finite responsibility abstraction, (2) monolithic and early-cut
relational image algorithms at identical precision, and (3) a separate raw TypedIR
explicit-state oracle. Preserve complete joint rows, error/normal/abort exits,
identity sets, branch variables and counterexamples. Report unsupported semantics
and exhausted budgets separately from agreement or safety.

## Delivered evidence

The implementation/driver/input identity is
`c0c1dc7746bfb4f5f819b10bc68aff579e8fbf46`. Preparation passed 21 tests, including
3,840 exhaustive small transition images, actual raw-IR counterexamples and
N-1/N/N+1 budget boundaries, plus clippy and release build. All complete point,
violation and exit relations are checked against the independent raw oracle and
actually materialized. A nonempty violation relation is not a safety success;
UnsupportedMultiplicity and ResourceLimit remain separate outcomes.

The root-coordinated frozen run contains 224 measured processes and 64 warmups
over eight eligible module cases / 43 functions. The measured processes comprise
168 shadow module runs (903 function comparisons) and 56 separately labelled
production-context runs. Two incomplete modules are retained and excluded: the
four-independent-guard resource case hits 300,000 BDD nodes while its oracle
completes, and a 17-Bool case exceeds the explicit oracle row bound.

On the real four-module error-cleanup source, 19 functions / 765 query points all
complete. Candidate mono/cut/wire medians are 193.2264 / 202.4921 / 209.4107 ms;
full process medians are 199.3034 / 210.0596 / 216.2608 ms. These are research
experiment costs, not compiler gains. The correlated-guard example demonstrates
finite-query precision relative to the production conservative rejection, without
changing production acceptance. The current candidate is therefore NoGo.

See `docs/research/059/RESULTS.md`, `CONTRACT.md`, `BRIDGE.md`, and the lightweight
`evidence/impl/059/acceptance.json`. The portable raw archive retains source,
original commands, canonical rows, executable and identity-hashed rlibs; the
README supplies a cold replay that always restores schema-8 dependencies from
Git. The final independent review binds this documentation/evidence tail by byte
identity with the frozen implementation. Schema-9 mainline and research055's
schema-7 measurements remain separate.

## Non-goals

No general path-feasibility theorem, no arbitrary dynamic heap multiplicity, no
claim that UP proves low treewidth for a compiler CFG, and no claim that BDDs,
existential variable elimination or minimum-fill are new inventions. No production
benefit is inferred from an isolated solver ratio.
