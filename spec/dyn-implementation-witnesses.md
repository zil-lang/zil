# Complete dynamic declaration and implementation witnesses

Dynamic transport is untrusted. Source declaration identities, complete recipes,
actual function headers, actual SSA and the independent ownership proof must
agree before a table may be used. A compiler token, matching name, layout,
function ABI, fingerprint or entry's asserted origin does not establish an
implementation or a source permission. Missing recipes remain Unknown.

A complete type value is `(ty, life, classes)`. `life` is the complete local
`StorageLifeShape`, independently reconstructed from the actual complete type,
nominal instance and opaque inventory. It includes outer Address/Desc leaves,
nested retained domains, ordered nominal occurrences and phantom classes.
`classes` maps every local class exactly once into the current Context Fixed
domain. A method actual may instead use that method's declared Invocation
domain. Root receivers, trait applications and persistent witness actuals cannot
use Invocation classes. A bare `DynGenericActual::Type` is valid only for a
complete independently proved zero-domain type; raw Address and tuples containing
Address are not zero-domain types.

Recipe constructors preserve Source's canonical complete Typed form. A Tuple
with a nonzero whole persistent domain, including retained or phantom classes,
is a BorrowedStorage<Tuple> with that complete joint shape. A Known0 anonymous
Tuple remains raw Tuple. Never strip an existing nominal BorrowedStorage wrapper
merely because its class count is zero. A root Address remains raw Address whose
inner type retains its complete qualified form; its outer borrow occurrence is
represented in the type value's external life domain. Do not substitute a
BorrowedStorage<Address> or compare only physical layouts against actual headers.

For a root BorrowedStorage type, the external `life` must equal the complete
embedded shape, and the domain is counted once. Nested BorrowedStorage types
retain their local domains and compose through explicit retained occurrences.
Equal slots alone do not establish equal shapes. Preserve all nominal arguments,
retained rows and phantoms. No unknown Managed, Closure, Task or erased-owner
shell counts as an empty domain.

A type argument's child class is related to its parent through the exact original
`TypeArgument(full_ordinal)` occurrence, then through the parent's Self class map
into Context. Do not look up an occurrence by same type, same spelling or equal
local class numbers. Repeated type arguments remain distinct occurrences.
Multiple source classes may map to one actual class only when the complete
actual joint relation proves that equality. This does not make targets equal or
apart, and it does not erase the original occurrences.

Type, constant and lifetime parameters distinguish the full original declaration
ordinal from a method ordinal. `Bound { depth, index }` names an enclosing
for-life binder, while `Invocation` names an ordinary method call domain. These
namespaces do not coincide. Universally quantified callable requirements are
checked with fresh skolems, rather than one convenient Fixed or Invocation
instance. A signature recipe evaluates its complete scoped type patterns and
thereby derives every outer, nested and phantom lifetime occurrence; it cannot
claim an arbitrary constant number of classes for an unknown Type parameter.

`SelfType` may occur only after establishing the actual receiver, including in
method signatures and associated definitions. It is forbidden in an impl's own
Self pattern. A projection specifies its complete receiver pattern, complete
trait application and original member ordinal. `T::Item` therefore resolves
through the witness for T, rather than the root `P<T>` or a same-named trait in
another module.

An implementation witness has the exact key `(complete receiver type value,
complete trait application)`. It names one actual impl and its complete original
ordered Type/Const/Life actuals. Substituting the impl Self pattern and implemented
trait pattern must produce that key exactly. The proof also discharges trait
parameter bounds, impl where requirements, direct super obligations and associated
bounds. Each reachable key has one legal impl and complete associated results.
Missing nodes, conflicting duplicates, unreachable extra nodes and proof cycles
reject. A dependency witness's methods do not become root dyn slots merely
because that witness is needed for a generic bound.

Associated definitions keep their original scope. Trait defaults substitute the
trait's actuals and established Self; overrides substitute the impl's actuals.
A constant has a complete carrier recipe even without a default RHS. A resolved
associated type carries a full type value, including outer borrow classes.
Resolved constants preserve exact carrier and bits. Untyped integer and float
literals preserve their source text until the substituted carrier is known, so
that generic width, range and rounding do not depend on a prematurely selected
i32 or f64. Unary/binary/conversion recipes preserve their actual operators and
checked or wrapping behavior. Projections use their real witness. Hashes and
per-specialization literals do not prove an original generic definition.

Every direct super edge and original argument occurrence is retained. Reject a
self edge or SCC in the declaration-level super graph, including growing edges
such as `A<T>: A<List<T>>`. Instantiate edges into one actual Context domain and
then deduplicate complete applications. A diamond reaching the same actual
`Base<a>` has one public member per `(application, member ordinal, method
actuals)`, while preserving proof of both incoming edges. `Base<a>` and `Base<b>`
remain different when their actual classes differ; genuinely equal actuals may
merge them after complete joint normalization. Different declarations' same-named
methods do not merge. All incoming witnesses must agree on the actual impl and
associated results; traversal order never selects a winner.

Associated queries use the active key `(actual receiver, actual application,
member ordinal)`. Repeated declaration names at different actual receivers can
form a legitimate finite chain, such as `Map<I>::Item` resolving `I::Item`. An
actual active-query cycle rejects. Infinite expansion or lack of a finite proof
hits the explicit resource limit or remains Unknown; it never becomes empty.
All expansion, owned snapshots, comparisons and fingerprinting are prepaid.

A trait method retains its original complete signature and declaration contract,
including parameter/result modes, call kind, lifetime relation, prerequisites,
access, effects, result, cleanup, resume and thread bounds. Instantiating the
trait and method actuals must produce the public slot. A slot's unique executable
key includes its full method actuals. An impl/default definition has its original
scope, requirements and an explicit list of actual specialization targets.
Core function references relocate through the actual signature inventory; Typed
references name exact FunctionIds. Each target's receiver and complete ordered
specialization equal its actual function-instance record. The actual header and
CFG then satisfy that definition and the public slot. Retagging another function
or choosing another same-ABI member cannot authenticate the link.

The virtual `StorageBase::Receiver` belongs only to a checked actual borrowed
callable environment head. Ordinary trait Self remains Parameter(0). A nested
callable requirement does not grant a Receiver unless its actual callable context
has that head. No source/class metadata grants a loan, resource responsibility,
Transfer or Share capability; runtime construction, captures, calls and cleanup
still reconstruct and transport actual targets and every parent separately.

Minimum independent raw and Wire checks keep actual headers/SSA and original
declarations fixed while corrupting the claim: omit an outer borrow domain;
swap same-typed argument occurrences; change phantom or retained shape data;
omit or duplicate a class map; replace a Type argument's original occurrence;
resolve T's projection through root Self or another module's trait; substitute
impl actuals into a trait default; change an unused constant's carrier or bits;
remove, duplicate or add unreachable witness nodes; select one conflicting
diamond path; merge distinct Base applications early; introduce a growing super
cycle or an actual projection-query cycle; change a slot and entry ABI while
leaving the original method signature fixed; or retag another same-ABI function
without changing the original definition's exact specialization target list.

Copy, Value, SyncDrop and StoreElement are sealed structural predicates in bound
position. They are preserved explicitly as Structural obligations for original
trait parameters, impl where clauses, associated bounds and method requirements.
Prove them from the complete actual type, opaque inventory, actual cleanup and
ordinary language qualification rules. A specialized producer having passed a
bound does not let raw transport omit it. A structural predicate is not satisfied
by an unrelated same-named user trait; ordinary nominal types named Value or
SyncDrop in type position retain their actual declaration identity.

Original associated bindings remain explicit equality premises. A requirement
such as `T: A<Item = i32>` contains both the Trait obligation for the actual T
and a TypeEqual between its real A::Item projection and i32. Direct super edges
carry their associated bindings as requirements in the declaring trait's scope.
They are not extra generic arguments. TypeEqual compares complete type values,
including local life shapes and their actual class maps; equal layouts do not
suffice. ConstEqual independently evaluates both original recipes in its exact
substituted carrier and compares the complete canonical value and bits. A method
that never mentions Item or COUNT does not discharge these original premises.

Complete signature and Self recipes contain explicit scoped lifetimes on every
Address constructor. Source resolves elision before proposing these records;
None remains Unknown and cannot ask a kernel to invent a common or fresh class.
Type parameters substitute their complete type values instead of using an
unscoped Address placeholder.

Trait method recipe ordinals index the trait's original member list. An impl's
member list keeps its own original source order; its associated definitions and
method definition ordinals index that impl list. An ImplOverride resolution
names this own impl ordinal, while the resolved member and public slot keep the
actual trait application and original trait ordinal. Establish the complete
implemented-trait application, then match unique real member names and kinds
inside those declarations. Neither equal ordinals nor generated entry names
establish the association. Impl method signatures live in definition recipes;
the impl's method_signatures inventory is explicitly empty.

A definition specialization and its matching function-instance record use that
exact target function's own Fixed/Invocation domain. They agree there before
relating the target to an interface. The provider's actual complete Self class
relation maps entry Fixed classes into Context. Full original signature and
specialization substitution determine permissible entry Invocation mappings,
including a phantom argument instantiated into Context Fixed. An entry's local
binder number alone cannot be interpreted as an interface binder. This permits
one real entry to serve different correctly proved Context class mappings.
