# 169: reviewed contract boundaries and source-only delivery

This record aligns the current interface text with the implemented schema 19
candidate and the independent review findings. It does not open another accepted
shape or alter runtime semantics or default budgets.

- Finite owned ErrorUnion alternatives are Copy or complete Package. Direct
  Managed alternatives, whole-local owned-union reassignment, union capture and
  nested owning payloads remain unsupported. Native entry cleans a transferred
  Package error payload after the source function returns it.
- Fixed owning arrays have static indices and conservative whole-array borrow
  roots. Whole-array capture, nested owning arrays and owning slices remain
  unsupported. Closure elements still carry exact private factory identity.
- Self-contained closure forwarding and move recapture are admitted. A rebound
  owning callback local uses the existing private one-element array protocol,
  preserving source type identity and the storage shell's original cleanup
  position. Full type-tree width and inline multiplicity are independently
  budgeted. No opcode 80 is introduced.
- Current interface version/operation registries and older Copy-only capability
  descriptions are corrected. Frozen historical research identities are not
  relabeled. Only current 166–169 normative snapshots are refreshed.
- The source archive also excludes 13 explicitly listed historical generated
  reports/plots outside evidence/. Declarative fixtures, contracts and ordinary
  documentation remain. Git history is unchanged and belongs to the separate
  audit/recovery archive.
- The native consumer adds failure at allocation four in main. This occurs
  after taking an array element, while preparing its replacement through an
  owned error result. Expected cleanup is ACAB, three acquired allocations and
  three frees, no live allocation, at both O0 and O2. Existing 24 runs remain;
  this adds two executions rather than claiming all failure positions tested.
