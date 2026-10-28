# 205 Types loop crosspoint followup

Production checkpoint: `63d61c770e48a185f4257cafaf2882c8cd627066`.
Final limited budget/raw/Clippy candidate: `8092faff3e17bac435d9969ae57a3290fdd0a3ad`.
Normative hash: `9813f43ee7c86d786f197f0ed4b8e65938ee354d94b2cdaec858e09e7871ce5e`.

The original 242-byte unused definition and the 698-byte actual instantiation
are unchanged. The actual Types rejection at clean18211044 remains recorded.
The first correction exposed a separate continue snapshot precision failure at
clean5a58; that failed raw command is also retained. No native program was run
for a source which failed compilation.

At clean99a553ca, the three crosspoint groups pass with eight actual O0/O2
executions: the original actual source prints AB; the same recycle definition
with counted getchar input executes 0, 1 and 3 iterations, printing CCAB,
CCCAB and CCCCCAB. Eight raw positive/negative cases run through both public
Types APIs. Old same-generation expression snapshots no longer masquerade as
next-iteration local values; stale independent locals and freshly rebound then
invalidated sources still reject. Break/continue cleanup retires body locals
and invalidates any surviving alias of their actual parents.

At clean8092faff, six raw groups plus two budget groups pass (8/0), the original
independent legacy raw test passes (1/0), and limited Clippy with -D warnings
passes. The existing Types bounds are unchanged at 5293/88/4498; the new
refresh-plus-continue bound is 2935. N, N-1 ResourceLimit and fresh exact Typed
equality are checked. These overlap earlier crosspoint raw coverage; they are
not added together as distinct capabilities or an entire workspace result.

The external compact archive is 315,885 bytes, SHA-256
`56c77143233650b33caf0e00bd733ae530aa5b8d7a442ca1ba9b39126aa11877`.
The unchanged 204 evidence tool packed and independently verified it. Exact
sources, IR, commands, binary stdin and failed/successful outputs are retained;
execution products are hash-only. Read ARCHIVE.json for full path and counts.
The earlier 205-types-evidence-compact.zip remains unchanged.
