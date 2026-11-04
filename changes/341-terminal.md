# 341: complete ordinary const and nominal trait abstraction

Authority is current `_zil_design.md` §16.1–16.2 and §18.1, together with its
named grammar, diagnostics, interfaces and dependency attachments. This proposal
implements already authorized terminal semantics; it does not redefine the
permanent language boundary or raise any request limit.

The baseline accepts concrete generic trait applications and ordinary default
methods. It rejects const binders, associated types/constants, symbolic
projections, generic or conditional impls, supertraits and generic methods.
Those rejections remain implementation gaps, not terminal negative examples.

Implementation proceeds in reviewable milestones:

1. Preserve declaration identity and ordered argument identity while extending
   substitution and instance keys to typed const arguments. Check every unused
   definition under its declared binders before generating concrete instances.
2. Add first-order nominal impl-head matching, scoped substitutions, overlap and
   ownership checks. A selected candidate is provisional. Prove all its where,
   inline and supertrait premises using independent already available evidence
   before committing its witness or associated equations.
3. Normalize associated types/constants from committed evidence. Symbolic
   projections in generic definitions require the actual declared trait bound;
   concrete instances cannot rescue an invalid unused definition. Distinguish
   an active repeated projection cycle from nonrepeating growth, which exhausts
   deterministic request budgets.
4. Specialize generic methods with distinct trait/type/method binder scopes and
   publish existing ordinary calls and machine signatures. Preserve default
   declaration context and check unused or overridden bodies.

No runtime dictionary, new allocation, permission, or trusted solver boolean is
introduced. Expanded programs still pass ordinary Types, Analyze, independent
Verify and existing Wire/native backends. Shared schema or syntax changes are
owned by the integrator and will be requested as exact patches when needed.

Closest required counterexamples include an impl whose associated equation
would prove its own premise; overlapping generic/concrete heads; foreign trait
and foreign outer Self through a local wrapper argument; two distinct trait
instances exposing the same member; missing declared projection bounds in an
unused definition; overridden invalid defaults; binder shadowing; repeated
projection cycles; and strictly growing obligations at low budgets.

Validation uses existing `generics` and `impl162_trait_bounds` test targets,
retained source positives and nearest negatives, unused definitions, raw/Wire
round trips, actual Clang/LLVM API O0/O2 where enabled, and N/N−1/fresh-cache
budget controls. No formal whole-workspace acceptance or terminal-completion
claim is made by a milestone commit. Rust comparison evidence distinguishes
actual safety/runtime/expression/cost checks from unmeasured product gates.

The first source milestone `87c30248a` implements exact supported scalar const
function arguments and fixed-array substitution. Independent review replayed
its Wire/native/budget tests, then identified missing per-arm substitution-map
prepayment and unused runtime-shadow array-length identity. The follow-up fixes
both: every potentially copied arm map is prepaid, and definition checking uses
the original declaration's const ordinal plus its actual immutable local ID.
Source coordinates carry diagnostics only. Generated ASTs with shared spans
are retained as a separate interface-level control. Consts remain values rather
than addressable storage, including unused definitions.

The next milestone adds supertrait closure and proof-before-commit for concrete
impl witnesses, inherited ordinary methods/defaults, structural Copy premises,
diamonds, cross-module identity, actual cycles, and bounded nonrepeating growth.
Generic/conditional impl heads, symbolic associated projections, general const
CT expressions, and generic methods remain active implementation work.

The next source milestone adds first-order applied nominal impl recipes and
conditional inline/where premises. Matching, overlap unification and occurs
checks agree on the admitted type constructors, including function, error,
optional, tuple, reference and fixed-array terms. Coherence compares heads
independently of premises. A selected recipe proves its complete premise tree
before publishing a witness; exact active goal cycles reject, while growing
where subjects exhaust the ordinary deterministic budget. Unused impl methods
are checked symbolically under their declared premises, including shorthand
receiver bodies. Materialization emits existing ordinary concrete impls/calls.
Foreign trait plus foreign outer Self cannot gain ownership through a local
argument. Blanket/primitive/enum impls and broader ownership composition remain
explicit gaps.

Trait-call rewrite identity is now an ordinal from one declaration's structural
expression traversal. A temporary address lookup maps a borrowed expression to
that ordinal; neither persisted dispatch identity nor clone replay uses source
coordinates. A retained generated-AST control gives two receivers the same Span
and checks their different method results through Wire and both native backends
at O0/O2. Required signature products use trait instance, actual Self and member
name, allowing multiple concrete instances of one generic impl without span
collisions. Independent review also prompted charged binder/member scans,
inherited argument formation checks and structural Self-premise admission.

This milestone does not complete associated types/constants, generic methods,
general const expressions or unsigned const domains. The limited existing
targets pass 19 + 53 tests with LLVM API;
exact Expand/request and cold/warm/fresh/persisted-cache boundaries are recorded
in the receipt. Rust runtime/RSS, maintenance comparison, compile-cost comparison
and the product-gates conjunction remain unmeasured.


The next milestone carries full trait references through generic call recording,
then substitutes their ordered arguments when producing each actual target.
Symbolic arguments can forward through generic functions and one generic impl
can supply distinct trait applications on the same Self, with different ordinary
machine signatures. An earlier finite-profile rejection of an otherwise valid
unused symbolic trait application is now a retained positive; its nearest
negative still rejects a body that exceeds the declared bounds.

Independent review marked `8ecd5eb6b` ChangesRequested after replaying its nine
focused tests. Its inherited downstream static dispatch tables still selected
impls and members by source Span, and unused unknown/builtin generic impl heads
could be silently filtered. The corrected code uses module-local top-level item
and member ordinals all the way through publication and validates the trait
before admitting a recipe. Retained unknown-trait and builtin-Drop negatives
accompany the nearest local-trait positive. The generated-AST control now also
shares impl, member and function spans and verifies distinct results through
Wire and both native backends, beyond the generic expression rewrite alone.
Ten focused tests and all 72 tests in the two existing targets pass. The exact
source/harness hashes and new symbolic-argument budget ledger are in the receipt;
independent review of this corrected source remains pending.


Review of `088cfc64d` independently replayed all ten controls and confirmed the
source/harness binding. It identified unpaid binder-by-bound scans in symbolic
argument detection and spelling-shadow checks. The correction uses one charged
type traversal against a prepaid binder set and prepays each shadow scan. Exact
boundary increases are reported in the receipt; request limits are unchanged.
Associated projection construction remains a separate work stream and is not
part of this accounting checkpoint.


The associated type checkpoint represents each projection as a private opaque
atom containing its actual symbolic host, trait identity, ordered arguments and
member. Generic definition checking obtains Copy, StoreElement or nominal
qualifications only from that associated declaration. Impl equations cannot
rescue unused bodies. Required/default equations and their declared bounds are
checked before use, including unused definitions, aliases, acyclic chains,
cycles, unbounded/non-Copy RHS values, duplicate members and argument formation.
Lexical module normalization preserves a renamed binder root through `T.Item`.
Trait and impl binder substitutions remain distinct when their spellings match.

The concrete normalizer uses the same active-goal set as impl premise solving.
It reads an equation only after the selected witness's complete premise tree
has succeeded. A separate definition-local equation map checks method
conformance under the declaration's assumptions and never enters candidate
proof evidence. Static publication freezes ordinary concrete signatures and
removes associated impl members; Core/Typed/Wire retain their existing schema.
The retained source selects two associated results and machine signatures on one
Self through distinct ordered trait instances. Local and project sources reach
actual Clang and LLVM API O0/O2, raw Types/Analyze/Verify and Wire verification.
Exact request N/N−1, cold/warm/fresh and persisted-cache controls are recorded.
The two existing targets pass 19 + 54 tests; eleven focused controls pass.

Associated constants, generic methods, generic associated items and remaining
qualified/applied/nested projection forms remain active obligations. Root also
requested explicit ordinary generic Type/Syntax-return and concrete generic
record CT/reflection coverage; the existing CT compiler controls and concrete
native Type selectors are not a claim that those whole bridges are complete.
Rust safety and actual static machine behavior have the limited evidence above;
runtime/RSS, maintenance and compile-cost comparisons and product gates remain
unmeasured. Independent review of this associated checkpoint is pending.
