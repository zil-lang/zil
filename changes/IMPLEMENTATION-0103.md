# 103 — transparent generic aliases delivered

This implements the closed type-parameter contract in
`PROPOSAL-0103-GENERIC-ALIASES.md` on production baseline
`24ca9583d8789cfdfe3ccdbfe3eeff614bf56429`. The authoritative assignment remains
the main tree's `.task/impl/103.md`; this document does not complete B0.

## Behavior and safety boundary

An alias can take distinct unbounded type parameters and compose existing
nominal types, tuples, arrays, addresses/slices, function signatures and error
unions. Substitution is simultaneous, lexical and transparent. Ordinary generic
specialization, Types, Analyze and the independent Verify still establish all
type/ownership facts. The implementation changes only Expand, plus tests and
this example/evidence; it adds no runtime representation or trusted wrapper.

All definitions and all supplied arguments are checked, even if unused or
phantom. Active declaration identity catches direct, mutual and argument-growing
alias cycles. Project normalization preserves the definition module and field
privacy. The single-module API additionally uses that existing lexical visitor
to protect nominal names which collide with generic binders before substitution.
Every lexical type binder also receives a fresh internal identity before its
scoped references are rewritten. This protects builtin types through nested
aliases and later function specialization without reserving new binder names.
Separate value/function names retain their ordinary identity. Concrete aliases retain
enum-pattern support. A generic constructor namespace is explicit `Alias::<T>`;
bare generic alias patterns require a concrete alias, as specified in the
proposal. Const/lifetime/bounded parameters remain explicitly unsupported.

The budget pass checks both the original body and the entire substituted tree
at the current depth before cloning. Subsequent syntax checks do not reset depth
or grant authority. Defaults and product gates are unchanged.

## Executed evidence

The final author command runs `impl103_generic_aliases`, `impl039` and `impl043`:
33/33 test groups pass. The extra targeted Expand regressions (`generics` and
`impl090_slice_formation`) pass 5/5. The revised suite retains 105 compilation reports:
47 successes (40 linked native programs and 7 Lower results), 13 Unsupported at
Expand, 2 Unsupported at Types, 31 SafeReject at Expand, 6 SafeReject at Types,
2 SafeReject at Analyze, and 4 SafeReject at Verify. Successful source requests assert an independent
verification run. Raw stage dumps and the actual native files are retained.

The 40 linked programs produce 52 native executions across O0/O2. This includes
the four-module example's six dynamic bytes and EOF at both optimization levels,
alias uses in CT and callable bounds, resource result/create, nominal/generic
composition, receiver aliases, and main success/error exit paths. The example
combines generic records/enums/functions and a CT-produced tuple without adding
runtime alias storage. Its independent output rule is `(2 * byte + 42) % 256`;
EOF exits 1 with no output.

The exact Expand fixture succeeds at work 2,044, nodes 407 and depth 6, with the
other limits at defaults. Each N-1 request is ResourceLimit at Expand and each
N result equals the full-budget Core. A 12-level duplicate expansion is also
rejected under its fixed 4,000-node cap. Formatting and targeted Clippy with
warnings denied pass.

These counts describe the final successful run only. They do not add earlier
failed attempts or unrelated regressions' native executions to the 103 totals.

## First failures retained

`tests-001` failed four test groups. Three were source-fixture grammar mistakes
(unsupported alias-bound placement, a newline-leading `+`, and reserved name
`use`); their source/reports remain in the raw archive. The fourth exposed an
existing generic-body limitation: constructing a generic record directly inside
an unused symbolic generic function body is Unsupported. The consumer was
expressed using an ordinary constructor plus the existing generic identity
helper; this task did not silently enlarge that separate accepted language.

`tests-002` failed one expectation: distinct same-layout generic nominals reject
at Expand/E-GENERIC-ARGUMENT, earlier than the expected Types/E-TYPE-MISMATCH.
The rejection remains; only the expected phase/code changed. `tests-003` stopped
at the explicitly authorized historical impl039 migration because `>=>` at the
return boundary tokenizes incorrectly; a block-return fixture replaced that
spelling. Its 8/9 result remains. `tests-004` is the later 30/30 pass. The exact
logs, commands, source fixtures and outputs are preserved, not overwritten.

The independent reviewer then found a real implementation defect in candidate
`4d228321`: the direct `compile_source` API allowed a nominal `T` in
`type Fixed<U>=T; type Wrap<T>=Fixed<T>` to be captured by the caller's binder.
The project API happened to avoid it through module-name normalization. This
caused both a false rejection of the correct record and a false acceptance of
the caller's primitive. Protecting only alias parameters would also have left
ordinary aliases exposed to a later `fn<T>` substitution. The revision therefore
uses the shared lexical module visitor to hygienically identify colliding record
and enum declarations before alias resolution, preserving ordinary local and
generic shadowing. New native record/enum/function cases and corresponding
primitive/coercion rejections cover those paths. The original review reports are
preserved with their original candidate identity in the revision archive.

The revision's first test run had 11/12 passing groups and a malformed missing
comma in the new enum-pattern fixture; correcting that syntax led to 32/32
driver groups. Clippy then found a single-pattern `match` left by the refactor;
the equivalent `if let` is used and the final exact source is re-executed. These
failed attempts remain distinct from the review's real nominal-capture defect.
`raw.tar.gz` and `raw-revision2.tar.gz` retain those runs with their actual
identities; they are not relabeled as the final successful implementation.

A second independent control on `b1dea3ba` found the remaining capture of builtin
`i64` by an alias binder legally named `i64`. Nominal-name protection alone was
insufficient. The final revision gives each lexical type binder an independent
identity and rewrites its type references under an explicit scoped map, before
alias substitution. This includes function and aggregate binders, keeps repeated
binder names repeated for ordinary rejection, restores outer scopes and preserves
separate value bindings. No new reserved words were added. Source and project
positive/negative builtin controls now pass. The new value-parameter fixture
initially used the already-reserved bare conversion name `i64`; that existing
rejection is retained as a negative, and a legal same-spelling `T` value/type
fixture checks the intended namespace behavior. This fixture failure is separate
from the reviewer-discovered implementation defect.

`raw-revision3.tar.gz` and the current author receipt bind the final version.
The new identity work is actually charged: the frozen Expand fixture moved from
740/377 work/nodes to 2,044/407, with depth 6 unchanged. Defaults were not enlarged
and N-1 remains a real ResourceLimit. This correctness fix is no compile-speed
claim; its extra normalization work must be included in any future measurement.

## Rust goals and scope

| Goal | This slice establishes | Still unproved |
|---|---|---|
| Safety comparable to Rust | Aliases reach unchanged independent checks; nearest ownership, address, field privacy and constraint rejections are preserved | Full-language safety and all generic forms |
| Runtime and memory cost | Real O0/O2 behavior with ordinary nominal representations; no alias runtime node | Paired Rust timing, RSS and retention advantages |
| Expression and maintenance | Reusable type composition works across four source modules and with CT/helpers | Measured maintenance effort or all six evolution families |
| Compile cost | Bounded precharged expansion, exact N/N-1 and failure evidence | Faster compile cost than Rust or than the previous ZIL |
| Net breakthrough | A previously Unsupported source capability is usable end to end | Product-level four-goal acceptance or a new graph theorem |

No performance sample was collected by 103. Current full-language/product/B0
statuses are not changed by this implementation. Independent review and root
integration are still required; the author receipt binds the implementation
commit and byte-checked archive separately to avoid hash self-reference.
