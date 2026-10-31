# Current-frontend relation experiment

This isolated research crate reuses the exact 076/071 relation algorithms and their
independent raw-instruction oracle. It does not replace production Analyze/Verify.
Timed shadow-query processes run Parse/Expand/Types and the relation algorithms;
Analyze/Verify is a separate preparation control. They do not run Lower, LLVM or
linking and are not full CLI compilation measurements.
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

The historical files referenced by path remain unchanged. The cut-stage6 research
budget limits remain at the original values. Initial failed compatibility/build
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
ExpansionDeferred functions do not run the full independent oracle: their evidence
is the structural support/initial-BDD equality check plus finite membership and
projection controls, not exhaustive enumeration.
This candidate still inherits the original slot/bit limits and is not a general
CFG factor solver or a production proof consumer.
It uses a distinct event-support-factor-v1 work model and an additional retained
product-index budget; its work counts cannot be compared with historical work counts.

Build with cargo build --offline --locked --release --manifest-path followed by
this directory's Cargo.toml. Test with cargo test using the same manifest and
--bin cut-stage6. Use graph_revisit.py prepare before the explicitly coordinated
sample window. Failures, unsupported functions and full-output limits are never
speed-ratio denominators. The cut-support candidate requires its own source controls
and timing window.

The formal factor window used the runner frozen at 7963d69ff. The later
recorded-requests-v2 revision of factor_bench.py writes the complete planned request
list before execution, checkpoints each request, and retains raw stdout/stderr and
receipts for nonzero exits, timeouts, launch errors and protocol failures. Failed
runs retain a partial-manifest.json and never produce a success summary. These
logging changes do not relabel or replace the original 120 successful measurements.
Interruption and persistence failures receive a best-effort runner-level failure
record. If the filesystem itself cannot write, stderr explains that the last
checkpoint may still say Running and that some request artifacts are incomplete.
factor_bench_fault_probe.py exercises this evidence path with mock executables;
its process times are functional controls, not compiler benchmark observations.
All 12 mock scenarios passed. factor_artifact_audit.py separately revalidated the
120 preserved formal outputs with strict function inventories and compared all
10 intermediate consumer-support metadata entries with their final frozen entries.
This audit executes no compiler and retains the original measurement identities.
