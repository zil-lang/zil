# Reactive query propagation adapted to compiler transactions

Baseline: 3d6babe17ee9d0a2f62d123063fb7af6a0886612. Research branch:
research/incremental-compilation. Parent task B0/W4 remains Doing.

The new distribution warrants remeasuring prior query-engine candidates. This
candidate applies zit's actual dependency edges, equal-result barrier and
transactional replacement to the existing compiler database. It does not copy
the JavaScript kernel's height threshold: an actual active dependency path is
checked, and repeated acyclic direction changes must remain accepted.

Write set: crates/zil-db/**, crates/zil-driver/src/session.rs, this proposal,
targeted tests and research evidence. No schema, default limit, language
acceptance, LLVM optimization level or verifier-capability change.

The database currently scans all stored queries on every publication and walks
the complete dependency closure on every read. Maintain exact reverse edges,
propagate changed fingerprint kinds only, detach obsolete edges only after a
successful publication, and collect reverse edges together with raw receipts.
Published graphs are closed and acyclic by construction: every input was already
published and checked while the producer was active. Therefore ordinary reads
can use that maintained invariant when no previously published node is active;
the original ordered traversal remains for active-existing-node cycle checks.
This is cache-metadata validation only, never language verification.

Controls: all existing kind-selective invalidation and budget/cancellation tests;
independent frozen full-scan invalidation and full-read oracles; actual cycle
versus >1000 acyclic edge reversals; duplicate edges, changed dependency lists,
failure rollback, overlapping active nodes, eviction and clear. Real source
Session cold/unchanged/private-edit/public-edit/error/restore sequences must match
fresh reports and current independent Verify. Compare full request time and
memory against this baseline on the enlarged distribution before claiming gain.
Whole-project semantic queries remain coarse; this candidate is not described as
function-level or cross-process semantic incrementality.
