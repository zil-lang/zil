# Complete ordinary Task products and sums

A Task retains the complete concrete result Type, including Package, Tuple,
Enum and ErrorUnion structure. `std.task.Poll<T>` remains an ordinary enum with
an unbounded Value parameter: the Pending alternative carries no T and Ready
transfers exactly one T. It does not impose an additional Copy requirement.
An explicit resource result mode on `E!R` wraps the successful R in its owning
carrier; it never wraps the whole error union or changes its selected tag.

The finite inline continuation profile accepts closed products and sums whose
owning leaves have actual canonical inline resource definitions and whose
complete reachable cleanup is synchronous Direct code. Alternatives are
exclusive, rather than simultaneous debts. Both kernels independently check
actual inputs, locals, SSA slots, results, leaf identity and cleanup. Nested
closed captures use the same actual leaf checks and retain the separate
registration ClosedEnvironment/no-future obligations.

Each complete owning result has a distinct live responsibility node in the
stable frame. Ordinary ownership transfer moves responsibility to this slot
before pending exit can drain other owners. Ready unlinks and transfers the
result once. Cancellation and unobserved results settle the actual live tag and
its owning leaves once. Allocation failure settles all committed inputs once,
without starting the worker. Ordinary and Task cleanup helpers sharing a symbol
must have equal complete Types, and emit one definition.

A helper taking a retained Task head can return an ordinary owned result while
preserving the actual input-port relation. A declared lifetime or equal pointee
Type does not identify an input root. Task port provenance follows actual
heads, local borrows and reborrows for all retained contexts, including workers
whose result owns its storage. The frame's retained input loans remain live
through Pending, Ready, cancellation and closing until actual destruction.

Task input-port enumeration follows the actual type tree in declaration order.
The outer order is the worker's complete parameter order. Within a parameter,
visit Tuple, Record and Package fields, Enum variants and their fields, Array
elements and closure captures in their respective declared orders. At an
opaque retained carrier, visit its independently checked port directory in
that directory's order at the carrier's position in the enclosing tree. Do not
collect all visible Address slots before visiting opaque carriers. Thus a Tuple
whose first field is a Task with retained ports and whose second field is an
Address enumerates that Task's ports before the Address field.

Port ordinals, actual worker parameter ordinals and logical class indices are
separate coordinates. Preserve all complete child domains, including phantom
classes and equal outside images. This port order does not change the canonical
logical-domain ordering or permit compacting worker parameter indices. An
Address is one physical leaf: traversal of its Pointee or TypeArgument metadata
does not create further physical ports. A Managed wrapper does not itself
create an address or authorize a byte projection; retained opaque ports still
require the actual carrier and its checked access operation. Variant-dependent
ports retain the actual tag, initialization and source obligations. Enumeration
alone grants no root, loan, access mode, cleanup or thread capability.

This continuation qualification does not grant Transfer, Share, guard
suspension, borrowed-capture escape, unknown cleanup, or permission to execute a
Task on another thread. A Task result's metadata does not make its owner head a
materialized ordinary Sum.


## Owning callable cleanup

A concrete inline callable can retain live child Tasks and asynchronous
registrations. The enclosing callable remains one affine responsibility;
cleanup segments for its captured owners are not additional owning roots.
Cleanup follows the actual capture order and live flags. A child whose close
returns Pending keeps its flag, the enclosing owner node, environment storage,
and saved continuation. Ready permits retirement of that child only; the
whole callable is retired after all remaining captures have been settled.
Nested callable environments obey the same rule. Their cleanup never invokes
the user callable body and must not repeat the ordinary recursive drop after
its capture segments have already been destroyed. Mid-body Drop and
cancellation use this same resumable cleanup contract.

Closed is a private fact about an actual Task occupant and its replacement
generation. It does not grant a type-level NoExternal or Transfer capability.
Only Ready from that occupant's canonical poll-close establishes Closed;
request-cancel, ordinary poll Ready and an equal-shaped constructed Ready do
not. Result loans and retained parent ports remain live until destruction.
Replacing the slot invalidates facts about the previous occupant.

A consuming callable invocation destroys only the environment responsibility
remaining after its actual body returns. Captures moved out by that body are
transported with their original frame and protocol state. In addition to the
existing independent NoExternal proof, remaining Task captures can use
synchronous destruction when every normal body exit proves those actual
occupants Closed and all other remaining teardown is synchronous. A conditional
relation between the returned Poll tag and Closed cannot justify this cleanup:
the environment is destroyed before the caller can inspect that tag.

A qualified Task continuation can instead retain that same affine environment
root while its remaining captures close asynchronously. The node follows the
actual frame-backed spill whose flags the body changed; no ordinary poll-stack
address survives a Pending return. Each returned owning value acquires its own
responsibility before cleanup can return Pending. Argument owners retain their
ordinary transfer order. Cancellation settles these newer result owners before
the remaining environment, and never invokes the body again. Ready retires the
environment node once, then permits delivery of the saved body result. A new
occupant at a reused SSA site starts from its canonical storage, independently
of the previous invocation's spill.

Authority: `_zil_design.md` §§12.1–12.3; `spec/threaded-foreign-registration.md`
and `spec/affine-mutex.md` retain their independent thread and guard contracts.
