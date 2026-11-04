# General synchronization continuation proposal (343)

This is an implementation proposal under design §11, not a replacement for the
main-tree authority or an acceptance record. Public tags/opcodes remain for root
allocation. Task38 implementation has priority; the probes preserve the next
synchronization obligations without changing the reviewed scalar runtime.

## Real capability and ordinary library surface

Proposed source operations are `mutex_create(value)`, `mutex_lock(m)`,
`mutex_value(&mut guard)`, `mutex_unlock(guard)` and `mutex_with(m, body)`.
The constructor follows ordinary value transfer: Managed heads require `give`,
while an ordinary non-Copy Package moves normally. It returns ordinary
`MutexCreateError ! Mutex<T>` with Allocation/Initialization error alternatives;
success is a complete non-null owner. Allocation/OS-init failure consumes and
cleans transferred T once, with no rollback promise and no invalid success owner.
Effects include actual allocation and failure cleanup, including Suspend cleanup
when applicable; a generic interface cannot silently change ABI per instance.

Mutex<T> is a sealed Managed/Desc family containing actual T in stable explicit
storage. Guard<T> should use an ordinary std.sync Package containing a sealed
affine unlock lease and an ordinary qualified &mut T view, or create that view
from the lease through the same checked primitive. The exact physical choice
must preserve the same obligation. The Guard package uses ordinary value moves,
not a second ownership category; `mutex_unlock` consumes it. The lease alone is
the independently checked native unlock obligation. Generic construction,
ReadDesc/WriteDesc, a same-spelled record, c_handle bits or a copied outer
view cannot manufacture the lease or protected mutable qualification.

Proposed raw operations identify actual inputs: MutexCreate{value},
MutexAcquire{mutex}, MutexBorrow{lease,mutable}, MutexRelease{lease}. Result types
carry full T and ordinary declared Package/Enum identity. Acquisition must tie
one actual native lock instance, its actual payload place, one live affine lease
and the resulting view together. This is a sealed grant, separately rebuilt by
Analyze and Verify; a shared Mutex<T> shape cannot become an ordinary mutable
projection. Existing342 PlacePath/storage/root/loan machinery then handles field,
array, wrapper, call and borrow propagation. Do not replace it with a T whitelist.

Every derived payload loan must end before release. A view cannot escape a
helper whose local Guard is destroyed, even though the Mutex allocation remains
alive. Guard moves preserve the lease/payload identity; they do not imply pinning
or duplicate unlock. T must satisfy its complete initialized lock invariant
before release; a move-out requires a legal replacement or an explicit consuming
operation. Partial cleanup flags and ordinary successful-initialization order
must describe actual storage, not copied SSA aliases. A move, return or unlock
is checked where it occurs, not repaired later by deferred cleanup.

## Native lock, teardown and failure

Use a real target native mutex with acquire/release. The current Linux x64
pthread_mutex_t contract is40 bytes/alignment8; a general frame must derive T's
actual target layout/alignment and use a matching allocation/free pair. Never
infer Windows or over-aligned allocation from the Rust host layout. Publication
occurs only after native init and actual T transfer complete. A failed native
init is not followed by destroy on an uninitialized native mutex.

The initial lock policy can retain the current no-poison/fixed-abort policy for
unexpected native lock/unlock/destroy failure. It must explicitly document
blocking/reentry rather than promise deadlock freedom. A successful acquisition
creates one lease; normal/recoverable callback exits settle it exactly once.
Current fixed Trap/abort does not promise process-exit cleanup. General recoverable
panic/Suspend cleanup requires the real frame protocol, not a C-stack unwind
claim. A blocking lock call is never described as asynchronous suspension.

Owner close rejects new payload admission while actual T teardown runs, keeps
the native context alive through teardown/reentry, and destroys/frees only after
all guard/borrow obligations end. A counter or unlocked bit alone cannot stand
in for foreign raw-entry no-future/drain. Ordinary checked borrowing must prevent
owner destruction while Guard or view is live; retained external access needs
its own real root protocol. Direct default Drop additionally requires actual
teardown/allocator/child cleanup to be synchronous, or an explicitly Closed owner.

## Transfer and Share are structural obligations

An owning Mutex<T> can move across threads only when actual T, its cleanup,
allocator/deallocation dependency and driver/TLS contracts permit Transfer.
Shared Mutex<T> mutation requires protected T Transfer plus the audited native
lock/storage contract; T need not itself be Share. ByteBuffer under its audited
global allocator is an intended non-Copy positive. Borrowed data inside T retains
its actual external roots/lives; heap placement does not erase those obligations.

The native unlock lease is not Transfer on a same-thread-unlock backend. A
borrowed Guard may be Share only under actual T Share and operations that cannot
transfer/unlock the lease. Resume(transfer) with a live thread-bound lease rejects;
resume(local) still requires complete frame/loan/cleanup proof. Copy does not
imply Transfer/Share, and empty user impls cannot grant either. Owning migration
must include actual Drop/allocator/TLS effects; parent-only cleanup must not be
invented as worker cleanup. Opaque driver handles do not gain authority from
being bitwise Copy. An i32-only resource whose Drop calls an unqualified foreign
TLS/allocator function is a counterexample to data-only structural inference;
the callee's spelling is not evidence of its contract.

Unused generic definitions must declare every required structural/effect/life
obligation. For example Mutex<T> sharing implies the T Transfer requirement;
observed i32 instances cannot add it after definition checking. Both kernels
rebuild actual concrete fields/captures, teardown/callee graph and driver roots;
no public producer qualification flag is trusted. Qualifying stored references
or mutable payload fields does not weaken ordinary alias exclusion.

## Strong compare exchange

Propose dedicated AtomicCompareExchangeI32{address,expected,desired,success,failure}
with ordinary `(observed:i32,exchanged:bool)` result. Defaults are SeqCst/SeqCst.
Return the actual observed value on both paths; compare bit patterns, store desired
only on equality, and use strong non-spurious LLVM cmpxchg. Do not emulate it with
ordinary load/conditional store, an extra fence or an unrelated implicit lock.
The address must be the actual live sealed AtomicI32 cell, not &i32, MutexI32 or
an address-shaped metadata assertion. General Atomic<T> remains limited to an
explicit hardware AtomicValue domain; Copy descriptors/pointers do not acquire
object lifetime or reclamation permission through atomic bits.

The proposed C11 ordering subset uses this explicit partial matrix, not enum
ordinal comparison:

| Success | Admitted failure |
| --- | --- |
| Relaxed | Relaxed |
| Acquire | Relaxed, Acquire |
| Release | Relaxed |
| AcqRel | Relaxed, Acquire |
| SeqCst | Relaxed, Acquire, SeqCst |

MemoryOrder is an ordinary canonical std.sync enum. A future ordered source API
must define dynamic-order behavior explicitly (checked finite dispatch if
supported), not silently replace it with SeqCst. First static order literals
lower to exact immediate orders. Both kernels and decoded Wire must test every
matrix pair and unknown discriminants. Native model/preflight, text emitter,
LLVM API builder, operand visitors and machine/cache identity must represent the
actual cmpxchg, expected/desired types, natural alignment and both orders. Broader
integer widths, swap and weak CAS need their actual target operations and checks.

## Current source evidence and remaining bridge

The public imported scalar helper probe exposed a real Types interface-fingerprint
panic for SyncCell. Checkpoint5f37 adds its existing tag30/kind leaf; one targeted
test passed four real native configurations (Clang/API, O0/O2) with both mutex and
atomic contention through imported helpers, and same-name signature identity
separates MutexI32 from AtomicI32. Existing scalar operations therefore compose
through ordinary static library functions without granting general qualification.

The generic FnOnce scope wrapper currently rejects at parametric-body expansion
as an unknown function. Existing WithSyncCell transport can express that bounded
wrapper; ordinary source support needs the definition-time primitive/trace bridge
with zero calls plus cleanup on init failure, one call on success, real F transfer
and fresh per-invocation cell roots. It does not need a new permission bit.

General Mutex<T>/Guard cannot be supplied by a library wrapper around current
scalar load/store/add: those operations release their private lock before any
user callback and never grant an actual protected T place or unlock lease.
Current AtomicI32 likewise has no conditional-RMW primitive; load then store is
racy with existing atomic RMW users. Both need the real checked primitive/native
extensions above, followed by ordinary library wrappers.

All source probes are in tests/language-readiness/terminal/343/sync-probes.
They parse with the5f37 compiler; only the library control is currently admitted.
Unknown primitive/type and early Unsupported results are capability gaps, not
independent evidence that the future unsafe neighbor is rejected correctly.
Required future negatives must be replayed after their nearest positive works.
Further native evidence must include owned payload/guard returned through helpers,
owned field replacement and failed RHS cleanup, allocation/init/lock failure,
real two-thread contention, unlock once, actual TLS/allocator negatives and weak
memory ordering argument. Same-semantic Rust runtime/RSS/compile/maintenance
comparisons and the product four-goal conjunction remain unmeasured.

The independent native CAS adapter uses a strong cmpxchg instruction with typed
expected/desired operands and an exact {old integer,i1 success} result. The chosen
C11 order matrix is explicit: Relaxed permits Relaxed failure; Acquire permits
Relaxed/Acquire; Release permits Relaxed; AcqRel permits Relaxed/Acquire; SeqCst
permits Relaxed/Acquire/SeqCst. Every other pair rejects before LLVM building.
This adapter is machine-boundary evidence until the integrator publishes the
source/IR operation and independent kernel consumers. It grants no shared raw
address permission. Native execution must observe both success and actual
failure old values, plus contention through generated strong CAS on real threads.
