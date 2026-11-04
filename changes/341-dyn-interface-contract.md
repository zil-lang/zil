# Ordinary dyn interface and static vtable contract

Status: proposed public seam for root coordination after receiver-method
checkpoint 0cb5e8da6. This file does not claim dyn implementation or acceptance.
Authority is the current main-tree design §§9.4, 15.3, 18.1 and the explicit
parent dispatch to341/342. Root owns public AST/IR/Wire/version publication;
341 owns trait resolution, exact static providers and vtable candidates;342 owns
storage/life consumers. Types, Analyze, Verify, Lower and both backends require
separate consumers before source promotion.

## Public data

Use dense module-local `DynInterfaceId`, `DynVtableId` and `DynSlotId` values as
references into explicit untrusted tables. An ID, fingerprint, trait spelling or
producer witness flag grants no authority. Declaration identity is actual
module plus registered declaration/member identity; spans remain diagnostics.
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
- A finite projection table using actual bases/field or active variant paths,
  shared with the342 PlaceProjection/storage contract. Contract paths are
  structural data, not arbitrary strings or source names.

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
lane exposes closed ordinary receiver slots. A public dyn signature with an
open associated item or generic method slot needs an explicit source binding /
finite slot declaration surface: the present AST has no associated-equality or
method-slot-set payload. Root must publish that surface against the normative
syntax before it is accepted. Inferring a hidden ABI-changing slot set from a
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
and its real cleanup route.342 determines the exact borrowed-storage Type wrapper
and fixed life rows, reusing BorrowedStorage rather than inventing a second loan
model. No implicit allocation, RC, lock or global runtime registry is introduced.
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
