# Checked resource graph

`std.graph` owns runtime-sized nodes with real ByteBuffer payloads and shared or
cyclic directed edges. `NodeKey {ticket:i64}` is a checked value interpreted in
the receiving Graph's namespace. It carries no borrow, address, generative brand
or cross-Graph provenance. Two graphs may issue the same key value, which refers
to each receiving graph's own matching node. A key alone never grants access to
an allocation in another graph. All operations validate it against the actual
receiving owner. This is an explicit runtime check, not a static descriptor graph.

`with_capacity(nodes,edges,tickets)` reserves fixed physical node/edge capacity
and exactly `tickets` metadata slots. Metadata slots hold physical positions or
an empty marker. Issued tickets increase monotonically and select metadata by
remainder; addition searches at most the metadata slot count for a free slot.
Validation additionally compares the node's full current ticket. Reusing a
metadata or physical slot cannot revive an old ticket in the same Graph.
Counter exhaustion reports Capacity before wrapping. `issued_count` counts
successful additions; `ticket_slots` reports retained metadata independent of
that historical count. Memory is bounded by the requested capacities, rather
than by all past additions.

`remove` logically invalidates a node; the actual owned payload remains in its
physical slot until `collect`, `clear` or Graph destruction. `collect` first
reserves a bounded node scratch Store; preparation failure leaves the graph
unchanged. It then drops dead payloads, compacts live nodes, repairs metadata and
removes invalid edges without allocating an edge scratch. Physical node slots
and edge indices can move; they are snapshots, not stable handles. No node
address remains valid across a structural mutation.

`take_payload` prepares suffix storage before mutation, transfers the actual
ByteBuffer and invalidates the node's key on success. Missing keys and allocation
failure leave the graph unchanged. `clear` drops all physical nodes and edges
without allocating, clears metadata and retains backing allocations and the
monotonic counter. Keys issued before clear remain invalid after reuse.

`add_with_payload` returns the original payload in AddError on capacity/allocation
failure. `recover_payload` transfers it back to the caller. No error, logical
deletion or collection duplicates the payload's cleanup responsibility. Shared
snapshots contain Copy keys and values, never payload ownership. The graph's
lifecycle is explicit: live, logically dead, physically reclaimed, then reused
with a new ticket. It is a reusable ordinary library protocol; it does not add
unrestricted user-defined state-transition syntax or foreign thread authority.
