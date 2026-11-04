# Task39 owning continuation checkpoint

This checkpoint adds real persistent responsibility for moved inline resources,
prepared owning SSA values, complete owning tuples, and the existing one-owner
Package storage used for mutable resource bindings. Source
`Task<Managed<Token>>` and ordinary `std.task.Poll<Managed<Token>>` retain the
same complete owning type. Bare resource names keep their descriptor meaning.
The unrelated `Envelope<T>` fixture also constructs, moves, matches and drops
`Managed<Token>` through ordinary generic Enum checking.

Verify derives canonical local/value/field slots, committed input order,
saved-boundary live debt, and transfer actions inside its ordinary resource
transfer. The result is private immutable VerifiedModule data; raw IR and Wire
have no ownership-plan field. Analyze independently checks actual initial
storage and the entire closed worker/child/teardown graph. Pointer-valued local
views can originate only from independently checked frame places; borrowed
initial arguments, external environments and unsupported effects reject.

The native frame holds an intrusive live-responsibility list. Ordinary whole
Store registers new cleanup age. Take forgets the old local age and registers
the successful prepared temporary. A Package field refill attaches at its real
shell's existing age. Source field spelling and LocalId sorting do not choose
cleanup order. Canonical ownership follows ordinary live debt, not SSA aliases.
Init commits transferred owning parameters before the first poll. Allocation
failure cleans them once in reverse committed order and returns Allocation.

Poll uses its actual complete result type internally. The result slot has a
separate owning record; only the first Ready transfer discharges it. Pending,
Complete, Cancelled and Invalid never read or publish a source T. Close notifies
and drains children before settling parent obligations. The existing closed,
sequential child profile has at most one active direct child per parent.
Ordinary DropPlan leaf/aggregate recipes perform teardown; no C observer
implements a task frame or ownership state machine. Direct workers referenced
by TaskCreate retain their ordinary body with private continuation markers
removed. External scalar ABI1 remains separate and unchanged in shape.

The lexical oracles are ABC for older A / newer B / refill A, and BADCABDC for
the branch pair. The recoverable later-argument failure and suspended prepared
argument both clean AB. The historical manual BACDABDC fixture remains
separate: explicit dispose calls are not a lexical cancellation oracle.

Current admitted owning payloads are finite inline scalar-record resources;
complete tuple responsibility and one-owned-field Package storage compose with
that lane. General partial products, owning child arguments/results, borrowed
frame ports, user defers, external wakers, actual Pending close and public owning
SABI remain follow-on work. Synchronous default Drop is justified by the checked
actual closed child/teardown profile, not by an unchecked producer flag. This
checkpoint makes no Transfer/Share, general Mutex, cross-thread retained
registration or product-performance claim.
