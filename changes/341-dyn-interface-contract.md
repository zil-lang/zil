# Ordinary dyn interface and static vtable contract

Status: WIP private catalog and external public39 handoff after the corrected
receiver-method checkpoint e5b3e9ae6 (independent review d556e0ece). This file
does not claim dyn implementation or acceptance.
Authority is the current main-tree design §§9.4, 15.3, 18.1 and the explicit
parent dispatch to341/342. Root owns public AST/IR/Wire/version publication;
341 owns trait resolution, exact static providers and vtable candidates;342 owns
storage/life consumers. Types, Analyze, Verify, Lower and both backends require
separate consumers before source promotion.

## Public data

Use dense module-local `DynInterfaceId`/`DynVtableId` plus slot ordinals as
references into explicit untrusted tables. An ID, fingerprint, trait spelling or
producer witness flag grants no authority. Declaration identity is actual
module plus registered declaration/member identity; spans remain diagnostics.
The published candidate uses the shared LifeDeclarationId/Identity namespace;
stable identity must come from actual package/logical-module/declaration context.
Raw module/index numbers remain relocation only. Export/cache Type keys must
resolve the full bounded declaration/interface/nominal-instance context.
Canonical actuals retain the complete ordered type/const/life binder sequence,
phantom arguments, nominal declaration identity, unsigned constant bits and the
selected target width. Do not use a host usize or a nominal display string as an
actual-argument representation.

`CoreDynInterface` and the canonical typed `DynInterface` carry:

- Actual nominal trait declaration and ordered actual trait arguments, including
  all inherited trait applications used by exposed slots.
- A sorted unique list of fixed associated type equations and fixed associated
  constant values that the interface exposes or that occur in slot types/ABI.
  Values are normalized only after the selected premises are proved. Missing,
  ambiguous, cyclic or still symbolic equations cannot be erased into a slot.
- Ordered slots identified by the original trait member, plus a complete ordered
  method actual list. Type/const method actuals are concrete; life binders refer
  to the slot's invocation domain. Each generic method specialization is its own
  finite slot. No slot contains an executable runtime type/const quantifier.
- Each slot's receiver mode, actual parameter positions (receiver is0), complete
  parameter/result types and passing modes, management/error ABI and CallKind.
  Self may survive only as the erased receiver; a result/ordinary argument that
  still depends on unfixed Self is not an admissible slot.
- A complete normalized contract: prerequisites and guarantees, fixed Hold,
  invocation Call/read/write roots, result source/lending relation, Cleanup,
  Resume and structural thread requirements. Missing required declarations stay
  missing and reject; absence does not mean an empty set.
- Shared RetainedLifeShape including nested class-once retained maps, nominal
  phantom occurrences and typed ports. ContractProjection distinguishes actual
  Place paths, terminal RetainedPort access substitution and bind-only terminal
  RetainedClass. Neither retained selector grants a physical byte path.

Life domains must reuse342's explicit `Fixed` versus `Invocation` shapes.
Environment Hold/storage requirements are fixed for the dyn value's lifetime.
A slot invocation creates only its declared fresh invocation relation and never
narrows, freshens or repairs an existing fixed environment/storage bound.
FunctionLifeContract may remain the invocation-local relation when sufficient;
fixed environment rows are separate. Alpha-equivalent life binder spellings
compare by domain and ordinal, never by text or Span.

Interface equality deliberately erases concrete Self while preserving all
observable slot/equation/contract/ABI information. A `DynVtable` additionally fixes
that exact interface, complete concrete Self, selected impl declaration and
ordered substitutions, entry targets, environment layout/alignment and owned
cleanup/drop routes. Same-sized Self types or same-shaped methods do not share
identity. Target ABI/pointer width participate in publication/cache identity.

## Source and static producer

The existing `TypeKind::Dyn {path,args}` supports the first ordinary surface:
`fn use(p:&dyn Measure)->usize { return p.length() }`, with explicit borrowed
coercion from an actual `&Point` (and the mutable counterpart). This first source
lane exposes closed ordinary receiver slots. The external public39 patch adds
an explicit direct dyn alias RHS block with type/const equations and finite
`fn pick::<i32>;` method applications, without bodies. Root publication and
ordinary semantic consumers are still required. Inferring a slot set from a
callee's body, or treating an unrestricted generic method as one vtable entry,
is not an acceptable replacement. Private candidate records can already retain
those full fields while the ordinary source lane is constructed.

341 builds an interface request from the actual parsed trait reference and
finite declared slots. It solves the ordinary candidate premises with the
existing shared active-goal/work ledger before reading any equation or method
payload. It selects the exact impl/default provider in its actual declaring
module, specializes every requested generic slot through the existing ordinary
helper path, and emits only symbolic static target references for later real
function-ID resolution. Existing declaration/member Origins remain attached to
providers and requests. Compiler insertion has a distinct identity domain.

Construction requires an explicit borrowed or owning storage operation. Proposed
public operations are `MakeDynBorrow {interface,vtable,source,mutable}` and
`CallDynSlot {callee,interface,slot,arguments}` with fully checked typed variants;
owned construction additionally transfers one actual environment responsibility
and its real cleanup route. The joint representation keeps `Type::Dyn {interface}` an unsized pointee,
like Slice. `MakeDynBorrow` returns `Address<Dyn>` whose actual machine value is
(environment address, static vtable address), never an address of a temporary
pair. This ordinary stored Address is covered by342 BorrowedStorage, with no dyn
wrapper exception or second loan model. Interface fixed environment dependencies
use canonical numbered ports; each vtable explicitly maps ports to the concrete
Self's actual StoragePosition rows. Ports are contract substitution positions,
not Place projections, private-field universal paths or raw GEP permissions.
Each slot has an explicit receiver descriptor at full signature position0;
nonreceiver parameter vectors contain no self-reference to the enclosing
interface. CallDynSlot.arguments excludes receiver0: the one callee is evaluated
before those arguments and supplies its actual receiver. Both kernels rebuild
actual receiver and port roots and validate the common
Fixed relation. Root reserves Type wire36 and opcodes131/132 for Dyn,
MakeDynBorrow and CallDynSlot, alongside existing Task/Storage allocations. No implicit allocation, RC, lock or global runtime registry is introduced.
Borrowed storage cannot call a consuming Self slot. Shared storage cannot acquire
an exclusive receiver by choosing a different vtable or life label.

## Independent consumers and ABI

Types checks actual declaration/slot/entry signature conformance and concrete
coercion, without trusting a candidate bool. Analyze and Verify independently
check real source roots, modes, stored windows, result origins, effects, cleanup,
resume/thread requirements and every reachable target entry CFG. Verify must
not invoke Expand's solver, Types or Analyze as a proof oracle. Candidate tables
and raw/Wire entry target IDs remain untrusted. A function pointer with the same
machine signature cannot substitute for an entry with a different life/effect/
management contract.

Lower receives only verified dyn tables. Borrowed representation is an actual
environment address plus static vtable address; values preserve real owners and
342 slot/target dependencies. Each entry uses an explicit erased receiver ABI
and the ordinary slot arguments; result/error/management and Suspend/resume
routes are fixed. Cleanup routes are explicit ordinary targets. Clang and LLVM
API emit immutable tables and typed indirect calls only after signature checks;
ordinary known static calls retain direct targets.

Root public write set: IDs and Core/Typed tables/types/ops, program/module
transport, Wire encoding and rejection bounds, schema/fingerprint/interface
versioning and source slot-binding AST when opened. Private consumers split as
341 trait/vtable producer,342 storage/life, assigned Types/Analyze/Verify kernels,
and assigned Lower/Clang/LLVM API. Each decoder/traversal/key/substitution/clone/
queue growth prepays the existing cumulative ledger; no fresh default limit.

## Actual controls required

Nearest source positive is two distinct concrete Self values passed to one
borrowed `&dyn Measure` function, with target selected by the actual vtable;
mutable calls and generic/default static providers follow. Retain modules with
same-spelled traits, phantom type/const/life actuals, different associated
results/constants, two explicit generic method specializations, private default
helpers and repeated source spans. Invalid unused definitions remain checked.

Raw/Wire near negatives alter interface/slot/entry IDs, receiver mutability,
parameter order, fixed versus invocation domains, result source sets, effect or
cleanup claims, resume/thread bounds, concrete Self or target pointer width.
Exercise local environment escape, overlapping mutable calls, stored overwrite /
move/cleanup invalidation, consuming calls through borrowed storage and failed
construction/argument preparation cleanup. Both backends O0/O2 must run actual
source and independent verified raw controls. Record work/node/instance N/N-1,
cold/warm/fresh/persisted cache identity and failed-Build nonpublication. Public
ABI/product acceptance remains open until those consumers and controls exist.

## Actual WIP handoff

The external public patch is
`/workspace/.zil-env/terminal-20261104/dyn39-public/dyn39-public.patch`, SHA256
`0be98fd837f657ca8d183b8d0a74901014a5f786e212a4d341acb3e18d378727`, based on
the exact342 common39 runtime/inventory package. All10 files dry-run apply
cleanly. Composed IR compiled with rustc without warnings. The parser's existing
unit target passes1/0 for the binding rows, function-body disambiguation and
body/let rejection. The separate raw transport probe passes2/0, exercises
tables/131/132/contracts/unknown claims, changed const bits, truncation/oldWire
rejection and exact encode/decode node N91 with90 rejection. These checks do not
prove any dyn permission or native behavior.

Private generic_trait_dyn inventories actual trait applications, inherited
members and concrete static provider recipes with declaration/member Origins.
Every provider reads a committed solver witness first. Defaults retain their
trait context; overrides retain their impl context. Paid lookup walks compare
full ordered keys. Ordinary source call hints preserve dyn member syntax for
the later actual dynamic consumer. Two focused existing Expand unit controls
cover inherited/default/override origins and unknown trait/Self/argument
obligations; failed conditional candidates publish no vtable. Dyn requests also
check monomorphic impl formation rather than skipping unused unknown Self.

This catalog is WIP: it emits no executable public vtable or call yet. Generic
concrete coercion requests, explicit binding consumption, helpers/function-ID
publication, contextual semantic keys, joint CT support and all downstream
source/safety/native consumers remain open. Driver ProjectSources.paths owns
logical module paths while the current Expand entry lacks package/logical
context; an explicit stable context bridge is required. Do not invent it from
generated spellings, spans or raw numeric module IDs. Root is composing the
atomic common39/CT4/dyn39 base before these consumers continue.

The unified public base is now c3c597820. The first private39 adapter checkpoint
adds complete binding payload budget visits and Dyn AST pattern adapters,
131/132 operand registration in Analyze/Verify, explicit unsupported instruction
and type gates, and module-table gates before fingerprinting. An unimplemented
or dangling dyn table is not ignored merely because no function uses it.
Bindings remain explicitly unsupported at module/trait semantic admission until
their consumer exists. The existing three-package compile check reaches only
the separately assigned Task/CT4/common constructor/lifetime adapters (and was
not a successful build); actual dyn source/safety behavior is still unfinished.

The Types adapter walks MakeDynBorrow.source and CallDynSlot.callee/arguments
in the existing managed-slot, Product-use and capture-effect traversals. Its
input type/closure preflight and scalar expression dispatcher reject the new
unimplemented semantic operations before cloning them. Captured Dyn environments
do not satisfy ClosedEnvironment. The targeted Types check after this adapter
has15 remaining Task retained-shape errors on d2ad18d81, all separately assigned;
there is no successful Types build or dyn source acceptance at this checkpoint.

Root5873c4aed publishes the contextual APIs and the snapshot namespace rule.
Driver now calls those APIs with the complete original source request and its
logical alias map. Unchanged parsed AST uses Original; selection/materialization
uses Unavailable until an aligned numeric lineage producer exists. This never
joins source spans or reconstructs original IDs from a transformed vector.

Four direct context controls pass: complete alias groups distinguish two
equal-byte modules from two aliases of one module; raw module-ID/source-vector /
native-path relocation preserves the default namespace; explicit package mode
keeps every module's canonical path, including entry; duplicate IDs/paths,
empty components and an empty nonroot alias reject. Exact standalone snapshot
namespace limits are work221/nodes5, with N-1 ResourceLimit. This is a naming
context check, not a performance result or a type/permission proof.

The actual Driver source control also passes: a life-only View factory reaches
Core and carries its closed-snapshot identity independently of its native
filename. A #if-transformed variant fails closed in Expand for missing lineage,
executes no native command and publishes no executable. These are frontend and
handoff controls. Genuine generated/filtered Aligned production, full dyn source
tables/coercions/calls, contextual type keys, independent safety/native consumers
and their raw/Wire/budget/cache controls remain unfinished.

Retained-original lineage now has a concrete producer. The bounded
capture_original API reuses the Original capture encoding before CT, returns
the actual rows and remaining limits, and avoids constructing a namespace when
the paid discovery finds no identity consumer. The materializer carries input
positions while it removes CT-only declarations. Import pruning moves the same
rows with the same AST items. Final Expand receives Aligned rows and continues
the capture/CT/move budget, including a second paid snapshot resolution from the
complete original graph. No source coordinate or output ordinal is an original
declaration identity.

This slice preserves original top-level declarations only. Generated item
expansion, selection and nested member-list changes invalidate the usable rows;
those routes still require a genuine generation/member ledger. Expression and
type reification preserve the enclosing original row. The actual source control
places a removed CT Type function before or after a used life-only View and
removes a std.meta import; View retains original declaration ordinals1/0/2.
The private capture unit pays exact work698/nodes33 and rejects N-1. These are
frontend lineage controls, not dyn/safety/native acceptance.

The first retained-row checkpoint34fc exposed an early-admission regression in
root's broader generic suite: its capture discovery used the cloning preflight,
which rejected an unmaterialized syntax island before CT. Correction8a22b6361
uses a paid borrowed visitor and only discovers metadata requirements. The
unchanged ordinary generic Type-return/record-reflection/Syntax source control
passes, as do the retained-row and prior context controls. Corrected capture
limits are work700/nodes35, including N-1 rejection; the earlier698/33 receipt
remains historical. Generated/member lineage and executable Dyn stay open.

The schema39 source-provider checkpoint emits actual Dyn declaration, interface
and vtable rows in Core. Explicit Dyn syntax resolves its module-normalized
registered trait and ordered type actuals; final declaration IDs and identities
come from the actual Origins inventory. Impl-member lowering records the actual
emitted ordinary function ID. Binding uses the selected impl origin, concrete
Self and declared member against committed catalog slots; inherited slots retain
their separately committed impl, and defaults target the emitted ordinary
wrapper. No generated target-name parsing or Span join selects a provider.

This is a narrow borrowed receiver/scalar profile. Trait actuals, nonreceiver
parameters and results are scalar; unresolved associated bindings, richer
retained/phantom arguments, lending/port relations and explicit generic slots
remain Unsupported. Unknown contract components remain None, including effects,
Hold and cleanup claims. The actual Types/kernel boundary remains closed before
Build publishes an executable; metadata publication is not dyn acceptance.

Actual Core source controls cover Read<i32>/Read<bool> on the same Self, an
inherited default with repeated declaration spans, and same-named a.Read/b.Read
project declarations. A mixed (Self,&dyn Read<i32>) ordinary inherent header
also checks contextual Self lowering. The final Self traversal and every actual
host copy are prepaid on the same ledger before substitution; this closes the
independent early accounting finding. The simple complete Core request has
work3475/nodes739, with N-1 ResourceLimit. Source context absence and a richer
nominal type actual fail explicitly. Existing Expand library tests pass35/0;
the focused Driver project control passes1/0. Clippy passes with the existing
WIP dead-code warnings explicitly allowed; no strict warnings-free crate claim
or runtime/performance comparison follows. Full contextual type keys, concrete
coercions,131/132 source operations, both safety kernels and both native backends
remain unfinished. The next common40 migration preserves this checkpoint as
schema39 evidence and removes the independent flat slot life field.

The first40 Expand boundary checkpoint compiles on public7ffbfc856. DynSlot now
uses only storage_life. Existing outer-only callable roles embed their declared
flat relation with prepaid input/output rows on the actual remaining member
ledger; higher-rank source programs retain that remainder through lowering.
BFP raw type data uses the common record, while its full contextual/rich source
rendering and accounting remain part of the next actual producer slice.

Ordinary stored signatures now emit SignatureLifeClassBinding rows. They visit
an outer Address first, then its pointee's qualified classes; tuples add actual
Place selectors. Every local qualified class maps once, including phantom
classes, while internal fields are reconstructed from the actual type and never
flattened into duplicate signature bindings. Fixed/Invocation numbering follows
one joint source traversal. The actual View factory/read, phantom full ordinals,
reversed fields and tuple boundary source controls pass. The full-common-prefix
source request now requires work1636871 with N-1 rejection. A simple callable
role publishes Parameter0/Result Invocation0 in the common record. Source
for<a>fn(&a Tag<a>) and same/other two-label forms remain closed before their
actual class producer. This is a compile/source-data checkpoint, not whole40
admission; Types/kernel/CT/native composition remains separately assigned.
