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

Independent review of `fffdbb111` is PassLimited (`d2b9558c6`): the exact
eleven controls and retained source/harness bindings passed. The branch then
merged the complete numeric/lifetime/task base `e953fd381`; all 19 + 54 targeted
tests passed on that combined production source.

The next checkpoint extends scalar function const domains to `u64` and `usize`.
The target-aware expansion entry points retain the requested pointer width
through source, project and CT snapshot specialization. Normalization uses
that target domain's full unsigned range, never a host `usize` or signed i64
carrier. Definition integer and pattern checking retain that full range too.
Native profiles are x64, while 32-bit CT/frontend requests independently admit
usize MAX and reject MAX+1 even when the const value is erased. Overflow,
wrong suffix/domain and invalid unused
definitions remain rejecting controls. Signed negative const arguments, general
CT const expressions and aggregate const parameters remain separate work.

Ordinary generic functions returning Type, generic record field reflection,
generic Syntax<Expr>/Syntax<Type> return values and full nominal/phantom
arguments are exercised by actual source through the existing CT bridge.
`syntax.literal(T)` under only `T:Copy` remains invalid: that bound also admits
records, whereas literal requires Scalar/Text. A concrete scalar field provides
the nearest valid generic Syntax producer. This checkpoint adds evidence and
unsigned const handling; it does not relax CT definition checking or change
public IR, Wire or syntax.

Associated constants will preserve the same host, actual trait application and
member identity as associated types. Symbolic uses obtain only the declared
type; concrete values may be read only after all selected impl premises pass.
Declaration/default and impl equations must be checked even when unused or
overridden. Scalar closed values form the first concrete checkpoint; general
verified CT equations remain a separate obligation until they enter the existing
CT executor. Value shadowing, missing/duplicate equations, ambiguous trait
instances, bad unused defaults and active expansion cycles are retained controls.

Closed scalar constant equations with an explicit scalar receiving type will
also use the existing driver CT artifact compiler, independent verifier and VM
before ordinary expansion. This includes unused and overridden equations.
Equations depending on symbolic Self/type arguments remain subject to their
separate definition and committed-instance lanes; they will not be evaluated
with an invented i32 substitution. Expand itself will not execute CT code.

The associated-constant checkpoint passes 21 generic + 56 trait target tests.
Required/default constants compose with associated result types, inherited
bounds, generic nominal impls and ordinary generic functions. Bound-selected
and direct/applied reads retain the full actual trait instance. A generated AST
shares trait/impl/member/function/value/type spans and still produces distinct
values through independent raw/Wire verification and all four native profiles.
Closed explicit-scalar equations use the verified CT artifact executor, with
unused overflow, overridden divide-by-zero, forbidden foreign calls and failed
Build publication controls. Cross-module defaults call their declaring module's
private helper. Both 32/64-bit usize boundaries and grouped types/equations are
retained. Actual new classifiers and namespace-map passes are prepaid; exact
current ledgers and hashes are in the receipt, with limits unchanged.

This remains a bounded checkpoint. Symbolic/dependent equations, active constant
cycle normalization, constants in generic arguments/lengths, inherited/aliased
CT receiving contexts, aggregate/Type/Syntax associated values and generic
methods remain active work. Safety and static/native behavior have the evidence
above; Rust runtime/RSS, maintenance and compile-cost comparisons and the joint
product gates remain unmeasured. Latest exact checkpoint review is pending.

Generic methods will alpha-normalize type and const binders by owning declaration,
member and parameter ordinals before any outer substitution. Lifetime binders
retain their own function scope and ordered header positions. Original lexical
spellings are retained in a private scope map for associated value references;
source spans remain diagnostic data. Impl/default providers become private
ordinary generic helpers, with every original definition checked before removal
from runtime trait tables. Calls preserve exact trait actuals and Self, prove
impl and method premises, and keep receiver argument zero. Impl methods cannot
strengthen the required generic contract. Generic default delegation, cross-module
same-spelled traits and outer/method binder shadowing are required real controls.

The receiver-method checkpoint now specializes method type/const arguments into
ordinary functions. A private ledger assigned from actual input module/item/
member traversal survives aliases, helper insertion, instance queues and output
placement. Scoped owner context is restored on every Result path around body and
formal-premise checks; caller argument lookup stays in the caller's context.
Captured actual trait references use declaration-local expression ordinals;
impl-member captures have a separate recipe/member table, so synthetic function
names cannot select their evidence. Canonical trait declaration symbols and
ordered arguments identify keys; diagnostic Span no longer enters equality,
ordering or method/default hashes. Compiler and source declaration ordinal
spaces are distinct.

Unused defaults, impl bodies, required signatures and method premises are checked
before generic members leave runtime tables. Impl methods may omit/weaken direct
premises and may spell an equivalent inline/where premise; inherited qualification
is installed before associated projection checking. Actual source composes
inferred/explicit calls, generic nominal impls, const usize slots, associated
results/constants, default and nongeneric member delegation, mutable receivers,
outer/method binder shadowing and private-module contexts. Raw repeated-Span
receiver/member/function controls reach independent raw/Wire verification and
native Clang/LLVM API O0/O2. The exact receiver-method fixture request boundaries
are work69617/nodes10074, including N-1 and cold/warm/fresh/persisted cache controls.
Existing source boundaries and the final executable/source binding are retained
in the receipt. The targeted existing suites pass 23+58 with llvm-api enabled;
Expand tests compile, and targeted Expand Clippy passes.

This is a receiver-method checkpoint, not task completion. General life/lending
method headers, static associated functions and type-qualified symbolic method
calls, semantic implication beyond canonical direct premise subsets, remaining
const/associated equation consumers and the full dyn boundary remain open.
No runtime/RSS, maintenance-cost or compile-cost comparison or joint product-gate
claim follows from these tests. Safety and static/native behavior have the
bounded evidence above; independent checkpoint review is pending.

Post-review corrective probe: the exact0cb method checkpoint compiled and
verified generic-method-substitution-growth.zil, but native exited4 instead of0.
A type actual with four array lengths inserted four Expr nodes before later
method calls; replaying the old receiver ordinals selected another trait's
same-signature target. The original19/0 review is preserved and was reopened in
d79799ee5. The source and old machine artifact are retained.

Correction attaches dispatch to the actual MEMBER/CALLEE expression Box in the
owned tree. Definition-only Self/equation rewriting first records source-node
origins, and const-definition clones explicitly compose those origins. Concrete
substitution anchors the actual sites and derives fresh published ordinals only
after mutation. Those tracked root Boxes remain live: leaf replacements have no
descendant method site, and implicit ReceiverMember grows children in place.
No arbitrary clone or rewriting pass may intervene; missing sites reject. Later
const-call lowering may remove argument expression structure after pointer-frame
binding without changing the selected call sites.

The original now exits0. Growth counts0/1/2/4/7, both call orders and preceding /
between-call placement pass80 native profiles; generic impl, default-provider
and forwarded const controls pass12 more. The exact failing source also shares
callee/receiver/member/function spans and passes independent raw/Wire/native
checks. Full llvm-api targeted suites pass23+59; Clippy and Expand test build
pass. Current request work/node boundaries: method69941/10121,
no-outer9204/2667, type-growth15862/4038, const-growth16238/4034, with N-1,
cold/warm/fresh/persisted controls and unchanged limits. This corrective
checkpoint remains bounded and awaits independent exact review.
