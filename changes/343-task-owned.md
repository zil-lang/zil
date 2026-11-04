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
that lane. General partial products, borrowed
frame ports, user defers, external wakers, actual Pending close and public owning
SABI remain follow-on work. Synchronous default Drop is justified by the checked
actual closed child/teardown profile, not by an unchecked producer flag. This
checkpoint makes no Transfer/Share, general Mutex, cross-thread retained
registration or product-performance claim.

Owned static child continuation follow-up: `task_await` retains the actual
concrete worker parameter and result types, including Managed<R>, through the
existing SuspendCall125 path. Arguments are prepared once using ordinary staged
pre-call cleanup and committed once to the embedded child. Pending resumes only
that child's poll PC. Ready transfers its self-contained result once, then closes
and destroys the child before continuing; each actual teardown graph must prove
synchronous close. Sequential Await admits at most one active direct child per
parent; owning values returned by earlier completed children remain parent debts.
No borrowed child result, concurrent sibling handle, external waker, or Pending
close is admitted by this extension. These remain implementation obligations.

This follow-up also admits actual Create-mode owning results; ordinary resource
freshness independently proves that returned values were created. Merely changing
a Give worker's result mode to Create rejects in the actual resource gate. A
Direct owning worker may have both an ordinary call body and a task adapter.
Native loop reuse, actual argument/body invocation counts, two owning argument
transfers, first-child result retention, and later-argument failure are controls.

| Existing comparison goal | This follow-up's evidence and limit |
| --- | --- |
| Rust-class safety | Both kernels rebuild full actual child types, ownership and closed effects; six malformed raw/Wire neighbors reject. Borrowed results and external roots remain unimplemented. |
| Runtime and memory | Child frames are embedded in the one allocated parent frame. Actual fault observation distinguishes parent allocation failure and proves no per-Await allocation. No Rust latency/RSS comparison was run. |
| Expression and maintenance | The same concrete Managed<T> crosses ordinary calls, task creation, Await, loop-site reuse and ordinary Poll<T>. General results/defers and full framework maintenance remain open. |
| Compile cost | Actual result-width and owner-action growth use the existing shared budget with N/N−1/fresh controls. Default limits are unchanged; no Rust compile-cost advantage is claimed. |
| Concrete gain | Previously unavailable owning static child composition now executes on both native backends. Product gates and all remaining task343 obligations are unchanged. |

The next closed-data result step admits Unit and recursively finite Copy
Tuple/Record/Enum/ErrorUnion data, including the original later-child-argument
recoverable-error program. Inputs and all saved storage still exclude external
addresses and hidden dependencies. Owning ErrorUnion payloads remain a separate
canonical debt-mapping obligation, rather than inheriting Copy admission.

A Unit worker's native Direct body still returns void. Its private task poll uses
an i8 zero carrier so persistent structs never contain LLVM void; ordinary
source Poll<Unit>.Ready carries Unit, with no invented owning value or public
ABI1 change. Unit arguments have no physical call argument. Pending/error tags
never expose a source payload. Implicit owning-input cleanup runs on both Unit
completion and ordinary error return before the child/parent can become Closed.
