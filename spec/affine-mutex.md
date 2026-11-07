# Mutex ownership and affine guards

`Mutex<T>` owns stable storage for one initialized `T`. It does not make `T`
Copy or permit unsynchronized access. Creation allocates and initializes the
native synchronization object before transferring the value to its payload.
Every recoverable construction failure settles the actual input responsibility
once; a payload requiring asynchronous close cannot acquire a synchronous
failure-cleanup proof merely by entering a mutex.

A lock operation borrows the actual mutex head and produces one affine guard.
The guard owns exactly one unlock obligation. Its payload projection is an
exclusive child borrow whose parent is that guard. Explicit unlock and implicit
guard cleanup require all children to have ended, consume the obligation once,
and restore the complete lock invariant before native release. Taking a value
out of the payload requires replacement before unlock. A successful lock has
actual acquire synchronization; unlock has actual release synchronization.

Moving a guard through locals, calls or returns moves its obligation and keeps
its actual mutex alive. The type's retained shape describes positions and
classes; it does not prove which runtime object supplies a position. Each
checker reconstructs actual sources and parent loans through the callee and
substitutes them at the caller. Matching a type or joining unrelated argument
roots cannot prove the required relation. Unknown relations reject instead of
becoming empty sets.

Destroying the mutex requires no live guard, projected borrow or worker that
can access it. Native synchronization is destroyed before the owned payload
and allocation are released. Sharing a mutex between threads requires the
payload's complete transfer capability, including its allocator and cleanup
dependencies; `T:Share` alone is insufficient. A guard gains neither thread
transfer nor suspension permission by being affine. These are distinct
capabilities and require their own actual protocol and target evidence.

Types constructs candidates. Analyze and Verify independently reconstruct
ownership, initialization, origins, loans, effects and capability requirements,
including raw/Wire inputs and unused declarations. Lower and each backend use
only verified operations. Joint schema/Wire 46 reserves type tags 38/39 for mutex and
guard, and instruction tags 141–145 for create/lock/project/unlock/guard cleanup.
Transport alone does not establish executable support or release acceptance.

Authority: `_zil_design.md` §§8, 11; acceptance:
`validation/language-release-gates.json` and `validation/product-gates.json`.
