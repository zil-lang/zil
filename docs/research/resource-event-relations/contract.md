# 059 contract — finite joint responsibility query

This contract precedes the prototype. Changes to its admitted fragment or costs
must be recorded with the implementation and frozen identity. The completed
measurement and its current-candidate NoGo decision are in [RESULTS](RESULTS.md).

## 1. Input and claim

Input is a real schema-8 TypedFunction from the public source pipeline, with its
TypedModule's nominal resource and closure definitions. Production baseline:
`6acda08c72dbd3e387893b4260770f0834dcdbe0`. Production Verify remains the authority
for language acceptance. This experiment computes a different, explicitly scoped
joint query and must not silently replace production Verify's conservative merge.

For each instruction/terminator point the query returns the **entire reachable
finite relation**, not independent may sets: resource-carrier lifecycle and token
identities; description roots; finite branch/tag values; initialization ordering;
and token retirement class. It also returns every represented failure and terminal
relation with its original point and exit tag. Source-call effects are interpreted
under the documented finite caller abstraction, not an asserted whole-program
path-feasibility theorem. "Entire" means exact for the abstract transition system
listed below, not complete concrete ZIL execution semantics. Structural
well-formedness/types come from the public TypedIR pipeline; raw negative mutations
exercise specified invariants, not every malformed wire or scope-stack proof.

There are three independent outcomes: supported computation with complete agreement;
`Unsupported` for semantics outside the fragment; and `ResourceLimit` for exhausted
declared bounds. Neither of the latter is a safety success or an empty relation.
A supported computation can report a nonempty violation relation. Agreement on
an illegal input means both algorithms found that same violation, not acceptance.

## 2. Responsibility and identity

Managed values/locals and owning closure environments are actual carriers. Each
input or construction site contributes a named token; an owning closure retains
the complete set of captured tokens, plus an environment token when required for
affine invocation. Moving a carrier transfers its whole set and invalidates the
source. A description records source positions and never creates a token.

Site names denote **single active incarnations only**. Before reusing a creation
site on a backedge, the query checks that none of its previous token remains in any
carrier. An overlapping incarnation is reported as unsupported multiplicity, not
equated with the old object and not declared a program error merely by site name.
This is a checked fragment boundary, not the invalid inference “one candidate
allocation site implies One”. Retired identities can be reused only after this
check; the result describes current episodes, not an unbounded execution trace.
The whole function is marked `UnsupportedMultiplicity` if any reachable state
crosses this boundary. Its partial rows are diagnostic only, never Complete or a
safety pass. Reuse also requires that no old Desc/borrowed-closure future use
survives retirement: the invalidation check below rejects such paths before the
same position can make an old description look live again.

Lifecycle distinctions include never initialized, live, moved and cleaned. The
identity set is empty exactly when that carrier is not live. Duplicate ownership,
use after move, reinitializing a live carrier, invalid cleanup order and residual
normal-exit debt are separate observable violations. Abort exits retain debt and
are not treated as successful cleanup. A synchronous error Return keeps its error
scope tags and satisfies the same caller-local responsibility conservation as a
normal Return.

Managed call inputs leave the caller; a declared create result has a new call-site
token. A give result is admitted only with exactly one owned Managed argument
whose full nominal/payload type equals the return type. Its abstraction includes
that input bundle or a fresh call-site token; these are caller-contract alternatives,
not proof that the callee can realize both. An ordinary call returning an owned
closure, or a closure call with affine arguments/results, is Unsupported. Calls are not inlined by
pretending their control flow or side effects were independently proved here.
Owned-closure calls preserve or consume the actual carrier according to the IR
operation, while closure-entry captured fields are tracked as separate inputs.
Repeatable closure bodies must return their captured Managed inputs live. FnOnce
body captures that remain live are delegated to the call boundary's specified
environment cleanup; they are not caller-local normal-exit debt in this query.
This assumes that public closure-call contract and does not re-prove backend
environment destruction or a callee's actual implementation.

## 3. Branches, aliases and finite abstraction

Bool constants, copies, supported Boolean operators and ErrorUnion tags retain
their joint correlation. A definition executed again on a loop assigns a new
current value; a previous iteration's condition is never fixed globally. The
finite abstraction can make arithmetic or external-call results nondeterministic;
it must list those operations. Nondeterminism is shared by subsequent uses of that
SSA definition, not freshly chosen independently at every Branch.

Description roots and borrowed-closure root sets remain joint with ownership and
branch values. Resource access checks require the actual root to be live. Any
supported invalidation check uses independently reconstructed future description
uses; address permission proofs, arbitrary heap aliasing and unmodelled aggregate
payload correlations are outside this query and must be admitted or rejected
explicitly. This research is not a complete replacement for all Verify kernels.

Future use is **guard-insensitive may-use-before-redefinition**. The candidate
uses backward equations and the oracle independently searches forward. The query
can report invalidation for `if b { move a }; if !b { use(desc(a)) }` even though
the concrete paths are disjoint. Exact agreement therefore proves the complete
relation of this conservative query, not absence of false positives. The corpus
retains this nearest counterexample.

ScopeEnter/ScopeExit are identity transfers in the resource relation. Their exact
scope IDs and Fallthrough/Return/Error/Break/Continue tags, all instruction kinds,
operand/result IDs, and CFG terminators are independently extracted into an
ordered raw-event audit. The audit detects tag loss or change; it does not prove
scope-stack balance. ErrorUnion returns additionally split the state relation into
`error-return` and `success-return` by the retained tag. Production Verify retains
authority for lexical scope and error-cleanup validity.

### Scalar abstraction used by both independently written interpreters

| Operation/domain | Transfer represented in the finite query |
| --- | --- |
| Bool constants, local/capture Load/Store, Not, supported Boolean binary operators | Exact value and its joint correlation |
| Integer constants | 0–15 exact; all other constants map to `Top=16` |
| Integer copies and supported comparisons | Copies preserve the abstract value; comparisons of two exact values are exact; either Top gives a nondeterministic Bool |
| Arithmetic, shifts, bitwise numeric results, numeric unary | Result Top; checked arithmetic/shift operations also retain a conservative possible-trap relation |
| ErrorUnion construction, ErrorTag, local copies | Exact error/success tag; aggregate payload is not tracked |
| Enum construction/tag/copy | Exact variant 0–14; `Top=15` is the unknown tag; comparisons through a numeric Top are nondeterministic |
| ReadErrorUnion / ReadVariant | Checked tag guard/trap retained; projected scalar payload becomes unknown |
| ReadDesc, ExtractField/Index, UpdateField/Index, aggregate construction/projection | Resource root checks retained where applicable; scalar result unknown; aggregate payload-to-tag/Bool correlation is not tracked; indexing has a conservative possible trap |
| Ordinary, closure, indirect and foreign calls | Result scalar unknown; admitted caller resource effects as above; no callee inlining or exact path feasibility |
| Unknown scalar | Bool/error tag may be either value; integer Top includes 0–15 as well as all other integers, not merely values outside 0–15; enum Top includes all variants |

Definitions choose the current abstract value once, so later uses preserve that
choice. Top comparison nondeterminism is an abstraction step at each comparison;
even comparing two copies of numeric Top need not yield a known equality. No
claim of an exact arbitrary scalar or heap relation is made.

## 4. Algorithms and exactness

Each event supplies a predicate on the pre-state plus simultaneous output-bit
equations. Unmodified fields keep their identity. The fair symbolic baseline joins
all equations before eliminating overwritten pre-state variables. The cut method
uses the **same** relation and BDD representation but eliminates an internal
variable immediately after its last incident equation. Every boundary variable is
the same variable on both sides; neither identities nor shared guards are copied
into independent choices. A reverse or incidence-driven schedule is only a
heuristic; no UP theorem is claimed for arbitrary CFGs.

Both use a least fixed point over the same finite state space, including backedges.
The independent oracle directly interprets raw TypedIR with explicit concrete
finite states. It must not call the candidate extractor, event interpreter or BDD
solver. Shared wire types and the declared output coordinate names are allowed;
layout coverage is checked independently. Equality checks include all points and
all violation/exit relations. Membership of every oracle row plus checked exact
cardinality establishes equality; set comparison is also acceptable. Hashes alone
are not a semantic oracle.

## 5. Cost and resource bounds

All potentially large stages must have explicit bounds, with charges before
allocation, cloning, scanning, joins, diagram creation and serialization. Count
the BDD node/operation caches, state rows, relation bits, CFG/event size and oracle
work. A work unit is deterministic logical work, not a millisecond or an allocator
instruction. The following limits are implemented and exercised by actual query
N-1/N/N+1 work-boundary tests and pre-allocation size rejection controls:

| Domain | Bound and charging scope |
| --- | --- |
| Candidate function query | One shared Work of 100,000,000 for extraction, event construction, planning, fixed point, complete oracle comparison, support census, independent row materialization, serialization and content observation |
| Raw oracle function query | Its own Work of 100,000,000 for independent setup/future-use search, transfer interpretation, row insertion and output relation construction |
| Freeze-only oracle serialization | One additional Work of 100,000,000 per function; encodes already obtained raw rows for the audit artifact; absent from sample mode |
| Geometry | 4,096 local/value/capture slots; 20,000 instruction/terminator points; 4,096 bits. Point and slot checks precede their containers, predicted owner×token/Desc×root/order bit count precedes cross-product field allocation |
| BDD representation | 300,000 allocated nodes including terminals; 600,000 operation-cache entries; temporary traversal/memo entries are bounded by the represented diagram and charged before insertion; final object release is prepaid by creation charges and separately timed |
| Explicit output | 100,000 total oracle/materialized rows per function, including point/fault/exit rows; 200,000 candidate relation keys; canonical serialization upper bound 64 MiB checked and charged before allocation |
| Frontend | At most 32 local source modules, at most 1 MiB each; public production Limits also apply. Parsing/Types and Analyze/Verify have their own production budgets, not this Work counter |

Thus one three-method shadow sample family has at most six successful Work
invocations per executable function (three candidate plus three oracle); the
three-method freeze adds three oracle-serialization invocations. These are **not**
a single 100-million-unit budget for the whole module, filesystem or experiment.
Work units count declared logical operations/prepaid bounded container work; they
are not an exact count of CPU instructions, allocator bytes or a claim about total
process RSS. Failures remain ResourceLimit/Unsupported, never Pass or empty facts.

Measure same-precision monolithic versus cut, and retain the explicit oracle as a
separate reference. Report extraction, planning, solving, complete checking,
materialization/drop and end-to-end cost. Production Analyze/Verify is background
context, never a claimed same-precision baseline. Standalone solver speed is not
compiler speed. Record failures and bad shapes, including input relation support
and diagram size; small transition-factor width alone cannot bound a correlated
input relation or a dense ownership set.

The executable's `candidate_total_ns` covers extraction through full checking,
census, actual canonical materialization, content observation and candidate/result
destruction; oracle construction/destruction is separately reported. Individual
stage clocks leave ordinary bookkeeping in the total. `total_ns` is the in-process
measurement before final module-receipt assembly/printing. The external driver's
`wall_ns` includes process startup, source loading, the oracle, all candidate work,
receipt serialization, stdout, and process teardown and is the complete measured
experiment boundary. Freeze-only artifact writes are never used as formal samples.

`max_transition_plan_count` counts retained overwritten inputs plus introduced
outputs in a transition schedule. It excludes unmodified input-relation support
and is not treewidth or full semantic-interface width. `max_point_support`, maximum
reachable point-BDD size and all allocated BDD nodes are reported independently.
The comparison is an internal mono/cut/wire ablation; no optimized partitioned or
fused-relprod baseline, dynamic BDD ordering or production-kernel victory is claimed.

Only root-coordinated exclusive timing is formal. Freeze source/input/driver SHA,
toolchain/profile/commands, sample plan and raw evidence first. No formal samples
have been run when this contract is first committed.
