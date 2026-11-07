# G5.2.1 · Executable finite shared dyn profile

Status: frozen implementation scope under the user instruction to proceed, not
acceptance. Base main15b9e09d8.
Reuse the published declaration/interface/vtable identities, DynContract,
Address<Dyn>, MakeDynBorrow and CallDynSlot and the existing schema/Wire.
The historical changes/341 contract and current normative design remain authority.

## Required path

- Actual Source has one closed ordinary trait interface, fixed scalar method
  slots and two distinct concrete finite Copy struct providers. Preserve the
  original trait/impl/member/module identities and provider function IDs.
- Coercion from a real shared concrete receiver yields the existing borrowed
  dyn carrier. Its machine value is the actual environment address plus immutable
  static table address, never an address of a temporary pair. Keep the real
  owner live and each derived borrow tied to its actual source.
- One ordinary function consumes the same shared dyn interface for either
  provider. Runtime input must select the provider in at least one consumer;
  record actual dynamic instructions and native output. Two unrelated direct
  static calls are not a sufficient dispatch witness.
- Derive the required contract from actual provider bodies/reachable targets.
  Prerequisites/access, guarantees, Hold, result source, cleanup, effects,
  resume and thread requirements must be known where construction/call needs
  them. Prove finite empty claims rather than replacing None mechanically.
  Provider reads fit the erased receiver contract; shared access never acquires
  mutable/owning rights. Scalar result creates no borrowing or management right.
- Types validates complete used/unused tables and Source coercions. Analyze and
  Verify independently validate actual roots, slot/entry conformance, body facts
  and lifetimes. Verify does not use the other stages' proof functions. Lower
  consumes verified facts; both native backends emit the actual pair, static
  table and checked indirect-call signature.

Finite providers may use only bodies whose access/effect/cleanup closure is
actually supported and independently reconstructed. Do not infer purity from
scalar types or a declaration's name. Unknown foreign, mutable, owning, Suspend
or unbounded callable effects stay closed unless separately proven in scope.
An implicit omission of a contract field is not a declared empty effect set.

## Controls and limits

Retain existing same-spelled cross-module traits, committed/default providers and
declaration-origin/relocation controls. Exercise wrong interface/slot/entry,
same-layout wrong Self, dangling table, unknown or forged contracts, root/parent
substitution, local escape, active borrow versus move/replacement and unused bad
metadata through raw and re-fingerprinted Wire inputs. Both kernels need legal
different receivers and nearest conflicting examples. Failures publish no native
artifact. Check Clang and explicit LLVM API O0/O2, cold/warm/fresh identities and
N/N−1/fresh budgets; keep old no-dyn fees and default limits.

This profile has shared borrowed receivers only. It does not admit consuming or
mutable slots, owned erasure, arbitrary associated equations or generic method
instances, resource/lending results, new retained ports, Transfer/Share or async.
Any missing fact in the current public representation needs an explicit bounded
handoff, not a guessed semantic default. Completion requires the whole Source
through machine path and independent review/common acceptance.
