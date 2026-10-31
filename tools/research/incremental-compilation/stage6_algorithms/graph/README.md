# Current-frontend relation experiment

This isolated research crate reuses the exact 076/071 relation algorithms and their
independent raw-instruction oracle. It does not replace production Analyze/Verify.
The five _current.rs files are auditable compatibility copies:

- source_current.rs retains the public Parse/Expand/Types path and simple local
  imports, raising only the research loader's module census from 32 to 2048.
- extract_current.rs and oracle_current.rs accept the current schema version;
  the oracle and symbolic evaluator explicitly reject newer constant variants.
- symbolic_current.rs otherwise retains the original event semantics.
- report_current.rs reports the current schema and restricts the research type
  profile before interpreting each function. Unsupported functions remain visible.
  It additionally emits the canonical result fingerprint after full row equality
  checks; the old schema, labels, relations and independent oracle are not replaced.

The historical files referenced by path remain unchanged. All research
budgets remain at the original values. Initial failed compatibility/build
attempts are preserved separately; they are not accepted samples.

The cut-stage6 binary runs the original dense/local direct, interned or BDD algorithms.
The 53 historical tests cover full joint rows, simultaneous assignments, loops,
duplicate operands/captures, faults, exact budgets and malformed inputs.

The cut-support binary is a distinct candidate: it builds connected components from actual
transfer expression supports, verifies independence of the initial relation by
canonical BDD equality, and transports exact Cartesian factors through a single
straight-line block. Entire encoded fields stay together. Fault/abort snapshots
retain every other component; assignments retain their common original prestate.
Branches, multi-transfer give outcomes and correlated initial components explicitly
reject rather than discard relationships. Repeated observable labels retain the union
of their complete product alternatives; full expansion deduplicates overlapping rows.

Its output contains every sum-of-products point/fault/exit relation, the original and
physical schemas, and the full location-code mapping. Thus deferred expansion
retains complete joint information. Small complete products expand and compare
every row with the independent oracle. A deferred product is a different output
format and cannot be advertised as a like-for-like speedup over full enumeration.
This candidate still inherits the original slot/bit limits and is not a general
CFG factor solver or a production proof consumer.

Build with cargo build --offline --locked --release --manifest-path followed by
this directory's Cargo.toml. Test with cargo test using the same manifest and
--bin cut-stage6. Use graph_revisit.py prepare before the explicitly coordinated
sample window. Failures, unsupported functions and full-output limits are never
speed-ratio denominators. The cut-support candidate requires its own source controls
and timing window.
