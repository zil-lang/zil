# Borrowed ByteBuffer metadata queries

`bytes_len` and `bytes_capacity` read the actual sealed ByteBuffer header and
return `i64`. An admitted qualified Package window, including a whole shared
window returned from its verified actual input, may supply the descriptor.
The manager remains responsible for cleanup. Queries neither create nor consume
an ownership obligation, and do not grant mutation through a shared head.

A ByteBuffer descriptor is weak: saving it does not retain an active shared
loan. Each query nevertheless requires a live actual manager and a permitted
shared read at that instruction. A description saved before an overlapping
exclusive window cannot read while that window remains active. A description
derived from the current exclusive window may read through that delegation.
An ended exclusive window or an unrelated manager does not prohibit the read.

Both kernels independently preserve complete origin alternatives. Branches do
not merge their delegation grants, even if their physical targets agree. Grants
are tied to the current execution of the actual loan producer: executing the
same loan site again revokes its grants in saved addresses and descriptors
before the new result inherits its current input grants. Loop backedges cannot
give an earlier descriptor authority from a later execution. Weak descriptors
remain outside the active-loan and public Hold projections.

Direct helpers export only their actual parameter-origin query reads. A helper
that ignores a descriptor does not read it. Each actual alternative is checked
at the caller against its current windows, including through successive Direct
relays. A final proved-empty read effect differs from an unproved or masked
summary. Unproved query-bearing edges across solver partitions, indirect
execution, captures and descriptor returns remain outside this stored profile;
ordinary queries retain their existing independently checked consumers.

A rich Direct signature in this profile may have an exact outer input
`Desc<ByteBuffer>` companion contributing zero borrow/class rows. It creates no
class, loan or lifetime source; every other actual position must still consume
its complete canonical rows. This entry requires that function's own reachable
query proof, not an unrelated or unreachable query elsewhere. It does not apply
to pointed descriptors, captures, results, other descriptions or Managed heads.
Formal separation still requires distinct actual origins where no cross-formal
delegation relation has been proved.

Canonical resource identity, physical field selection, manager liveness and
shared-head write restrictions remain independently checked. New point states,
origin alternatives, comparisons and Direct effects are charged before their
work; the default limits are unchanged. The implementation has no public IR or
Wire extension. Source, raw/Wire, loop-delegation, exact-budget and native
controls are in [the metadata fixtures](../tests/fixtures/resource-packages/borrowed_byte_queries.rs).
