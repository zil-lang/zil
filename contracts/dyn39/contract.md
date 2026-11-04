# Dyn39 public handoff

External patch only. Root owns publication and the joint schema/Wire39. Diff base is the exact common39 runtime/inventory package in
`/tmp/342-public39` (b377f8e9f plus the final shared models and complete39
codecs). The sibling `dynamic_wire.rs` uses that same runtime
Writer/Reader ledger, never a reset CT/runtime budget. Public data is untrusted.
This patch is a schema/transport/source-parser candidate, not dyn acceptance.

## Representation and source

Type36 is unsized `Dyn { interface }`. Its borrowed Address is the actual fat
address `(concrete environment address, static vtable address)`. No temporary
record is allocated and no pointer to a temporary pair is formed. `DynReceiver`
represents full signature position0 without a self-reference in interface type
tables: public receiver type is Address<Dyn(this interface)>, selected entry
receiver is Address<the vtable's concrete Self>, with the same exact mode and
mutability. Parameters in DynSlot and CallDynSlot.arguments exclude receiver0.
FunctionLifeContract.parameter_lives still includes receiver0, and every
StorageBase::Parameter index is in the full signature domain.

131 takes one actual borrowed source and exact interface/vtable references.132
evaluates one callee before ordinary arguments; that callee alone supplies the
receiver and actual retained-port roots. It contains no independently forgeable
second receiver operand. Entry target function IDs are code leaves. A target
with a closure environment requires the matching real capture environment;
ordinary function-pointer signature equality cannot manufacture it.

An explicit alias block is parsed only after a direct dyn alias RHS, preserving
ordinary `fn f()->&dyn T { body }` grammar:

```zil
type Reader = dyn Read<bool> {
    type Item = i32;
    const COUNT:usize = 3;
    fn pick::<i32>;
    fn pick::<bool>;
};
```

Type/const equations and finite method applications have no bodies. A method
application is one exact concrete slot, not a runtime generic entry. Bare dyn
automatically selects eligible nongeneric receiver methods. Generic methods
require explicit applications; never infer a public slot set from caller body
uses. Parser success is not formation/visibility/impl/coherence proof. Semantic
consumers reject duplicate/conflicting/inherited-ambiguous equations, unknown
members, extra/missing args, wrong binder kinds and unsatisfied premises.

## Independent validation rules

* Dyn IDs are request-local `(module,index)` relocation only. Check every
  reference against a unique actual row in the complete module/program context;
  missing rows, duplicate keys, wrong owner modules and dangling entries reject.
  CoreProgram carries each actual source module's inventory. TypedProgram's
  unified module retains every source table and relocates all entry/cleanup
  `(module,NodeId)` targets through the same actual function map as ordinary
  calls. No missing lookup defaults to0.
* DynDeclaration shares LifeDeclarationId and LifeDeclarationIdentity with the
  nominal inventory. Check raw keys and stable identities jointly across both
  inventories. Stable identity comes from actual Origins/SourceIndex, not
  nominal/generated names or spans. Trait/Impl kind, original complete ordered
  parameter kinds, member kind and method binder order are explicit. A compiler
  flag cannot establish a canonical builtin declaration or permission.
* Application/inherited Life actuals use ONLY the enclosing interface's Fixed
  domain. Method Life actuals use ONLY that slot's Invocation domain. Check
  original full generic ordinals/counts/kinds, including phantom and const
  domains/target bits, independently. Numeric equality across domains never
  means equal lifetime. No type/const binder remains quantified at runtime.
* Interface identity includes actual trait/declaration, full ordered arguments,
  inherited applications, associated equations/exact scalar bits, finite slot
  signatures, receiver/modes/CallKind and complete contracts/retained shape.
  The current scalar associated-constant lane does not claim schema-bearing CT
  values. Each admitted associated type/value and inherited application must
  agree with the independently formed concrete implementation; no equation can
  establish the candidate's own premise.
* Each runtime slot has exactly one compatible actual entry, and every entry is
  used. Check implementation declaration/application, concrete Self, actuals,
  entry signature and reachable CFG independently. Defaults keep their trait
  declaration context; impl overrides keep the impl member's actual context.
  A stronger implementation precondition is invalid. Extra slots, duplicate
  slot rows, missing entries and incompatible cleanup reject.
* RetainedLifeShape is the shared342 fixed class/port model, including nested
  retained/nominal rows and TypeArgument/Port/AssociatedType metadata selectors.
  Ports are full closed Address/Desc leaves with exact mode and existing fixed
  upper, not field paths or size-only claims. Every port has exactly one actual
  binding, only to concrete Self Parameter0 storage or real Closure capture;
  ordinary args and Result cannot ground the hidden environment.
* RetainedClass is terminal and bind-only. RetainedPort is terminal typed
  access/effect substitution at an actual opaque carrier boundary; neither is
  a GEP or LoadStoredBorrow path. Physical paths remain PlaceProjection only.
  Several ports in one fixed class do not prove target equality, apartness or
  mutable permission. Preserve concrete targets, owners, parent loans, epochs,
  description readonly restrictions and actual receiver permission. Mutable
  output substitution filters real mutable positions, not all roots sharing a
  class. Invocation binds an existing retained upper without narrowing/freshening.
* None in DynContract means UNKNOWN; Some(empty) is a finite empty claim. Any
  construct/call needing an unknown obligation fails closed until independently
  derived from declarations/actual CFG. Unknown is not permission top. Both
  independent kernels reconstruct actual access, result loans, Hold/Call,
  cleanup, effects, resume and thread requirements before verified publication.
* Cross-request/export/type/cache semantic keys resolve the FULL bounded
  declaration/interface/nominal-instance DAG. Type::fingerprint is now explicitly
  raw structural identity for table references. It must not serve as the
  independent semantic key. Relocation changes no semantic identity; changing a
  phantom actual, associated binding, slot mode/effect or selected declaration
  changes it. A structural digest alone grants no permission; consumers retain
  the full independently validated context. Missing/cyclic context fails closed.
* Prepay inventory, table lookup, key, path-prefix, nested carrier and vtable
  work from one cumulative ledger BEFORE scan/clone/growth. Recursive resolution
  has both active-cycle rejection and paid depth/work limits; static tables are
  not free traversal. Runtime Wire counts collections before allocation, charges
  all rows/args, bounds bytes, rejects unknown tags/noncanonical bool/truncation,
  and carries unknown claims unchanged for independent validation.

## Actual checks and outstanding consumers

The staged parser compiles with rustc against the existing zil-base dependency;
its existing parser unit target's new dyn binding test ran1/0 (exact alias rows,
return-body disambiguation, method-body and let-binding rejection). No new Cargo
target was added. The actual combined IR compiled with rustc against the atomic39 common
package with zero warnings/errors; the raw transport probe is separate from
ordinary source and independent-kernel acceptance.

Ordinary dyn Types/Analyze/Verify/Lower/dual backends, contextual semantic keys,
joint CT graph representation and all source/native/raw/Wire/limit controls
remain implementation work after root publication. Existing method checkpoint
e5b3e9ae6/d556e0ece is separate. The original pending dyn source has not yet been
promoted and no dyn terminal completion is claimed here.
