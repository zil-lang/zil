# Exact current-input partition layout reuse

Stage15, baseline `014c5d301`. The optional native object cache currently repeats
the complete symbol-reference graph, linkage-component grouping, and complete
call-closure planning before it can discover that every object already exists.
Reuse that deterministic layout as a private work product, following the same
distinction between cached computation and current verification as object reuse.

The key binds the current complete machine model (including item order), current
LIR planning inputs (ordered function identity/file/name/foreign status, CFG block
lengths and direct-call IDs), entry, requested units, both partition policies,
target and installed compiler identity. No module diagnostic fingerprint is used
as a substitute for current content. Closed/stable-name partitioning keeps its
existing separate path; the ordinary layout precedes any closed internalization.

Store only original item indices: each unit's retained prelude, canonical
declarations, private imported implementations and owned definitions, plus its
original owner bucket. The miss planner and hit replay share the same final
construction routine, preserving declaration/definition order, signatures,
linkage and direct-call-only rewriting. No machine IR, proof or Verify capability
is serialized. Current declaration-prelude validation and all current native
unit preflight remain mandatory.

The trusted private record is versioned, checksummed and bound to its key. Decode
and structural validation are bounded (at most 256 units, 262144 original items,
1048576 references and 8 MiB); they verify indices, item kinds, strict ordering,
complete unique canonical ownership, named-type retention and import bounds.
Imported functions retain the original External-only eligibility and aggregate
copy-cost bound. Missing, damaged, unsupported or oversized entries fall back to
the original planner; limits and errors do not change. Like the existing trusted
object cache, this is not a defense against an attacker rewriting both payload
and checksum inside the trusted private directory. It does not recompute the
entire reference graph to prove a cache record.

New records stay pending until the existing final successful artifact publication
commit. Failure/cancellation drops them. They live below the private object-cache
root. The root is canonicalized after the ordinary object cache prepares it; the
driver registers the exact pending layout path against current source/output
destinations and repeats that check before commit. A destination conflict drops
only this optional record. Optional I/O failure cannot revoke a successful request.
No public Wire or CLI format changes. Scoped controls compare full emitted modules
and LLVM text across miss/replay, including imports and local state, and exercise
key invalidation, malformed records and pending publication. Root alone runs
builds, tests and measurements; no performance claim precedes those results.
