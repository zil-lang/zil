# Byte payload views through a borrowed owning field

A synchronous shared ByteBuffer payload view may be derived from an actual `DescribeAddress` of a live ByteBuffer field. The field may be inside an initialized Store element reached through a checked helper result. A descriptor alias preserves that actual source; its type alone does not establish a local owner or a fresh allocation.

The stored solvers independently carry the actual manager root, inline Store header path, heap element path, field projection and delegated parent loans. A payload view pins the ByteBuffer head as its conservative retirement anchor. The head interval is not the physical byte address. Ordinary root adaptation may cross this distinction only for a value whose actual loan ancestry contains an independently reconstructed ByteBufferBorrow site and whose actual projected field is the ByteBuffer head. This marker remains private and is never serialized or read from CandidatePlan.

Popping, replacing, clearing, moving or destroying the actual owner must respect a still-live view. A distinct same-typed manager remains distinct. Shared head permissions cannot grant writes. The existing synchronous view consumer restrictions continue to apply; this rule does not establish escaping buffer views or a new storage lifetime contract.

Ordinary nested field observation preserves the original owning Package path. Reading a Copy field emits an actual address read; describing a Managed field emits an actual address description. Neither operation moves an owning intermediate Package. A later complete transfer remains subject to all initialization, descriptor retirement and loan checks.

No public type, Wire opcode, machine layout or ABI changes. New origin, alias, parent traversal and private bridge work is charged before growth under the existing limits.
