# 059 results — exact finite relations, current candidate NoGo

## Decision

**Do not install this prototype in the compiler core.** The finite joint query and
its conditional equality-port substitution passed independent checks, but this
implementation has no robust full-cost advantage. On the real four-module error
cleanup example, candidate medians are 193.2264 ms (mono), 202.4921 ms (cut) and
209.4107 ms (wire). Complete process medians are 199.3034, 210.0596 and 216.2608 ms.
The four-independent-guard resource case exhausts the 300,000-node BDD cap while
the explicit oracle completes. Neither failure is converted to an empty relation,
safety success or zero-cost sample.

The useful positive result is **preservation of an actual joint relation from real
TypedIR**, including the same Bool selecting complementary ownership transfers.
The finite query detects a nearest changed-guard violation while avoiding the
Cartesian product of independent may sets. This establishes a bounded precision
opportunity and an executable bridge to investigate. It does not establish a
production acceptance change, a whole-language safety theorem, or a speedup.
This NoGo concerns the measured representation and event algorithm, not every
possible relational decomposition or the user's broader structural hypothesis.

The implemented cuts operate on **one event's simultaneous bit equations** using
reverse last incidence; Wire adds conditional equality/complement substitution.
The whole function still uses a joint-relation least fixed point. There is no
cross-CFG region-summary reuse, full graph hierarchy, or general alias-permission
interface in this implementation.

## 1. Exact identity and measurement boundary

| Item | Frozen value |
| --- | --- |
| Production source | `6acda08c72dbd3e387893b4260770f0834dcdbe0` |
| TypedIR schema | 8 |
| Research code, driver and fixtures | `c0c1dc7746bfb4f5f819b10bc68aff579e8fbf46` |
| Driver SHA-256 | `df9c1f7635346a46541bf9c663cfa6a2ce2549a56f7ac7bd04af5e738e36f0c0` |
| Release executable SHA-256 | `6522093cff4369c2c33c1b01ae8e6915ccad4bc3d909547979d984f1431d28c9` |
| Rust / Cargo | rustc 1.99.1 (58cf8f9c3, 2026-10-13); cargo 1.99.1 (5f94df478) |
| Host / compiler LLVM | x86_64-unknown-linux-gnu; LLVM 23.1.1 |
| Profile | release; codegen-units=1; debug=0; incremental=0; jobs=1 |
| Root-coordinated window | `impl059-exclusive-20261023T172255Z` |
| Actual sampling interval | 2026-10-23T17:26:40Z through 17:26:52Z |
| Recipe | 2 warmups + 7 measured repetitions, fresh process per method/case |
| Order | `[mono,cut,wire,production]` cyclic rotation, offset `(repeat+2+case_index)%4` |

The driver archives the production commit and overlays only this research commit's
paths. All seven production path dependencies resolve inside that frozen tree.
The standalone Rust workspace does not use external clang. The seven rlib hashes
are in `identity.json`. Newer schema-9 mainline work and 055's older schema-7 timing
are independent evidence; none is mixed into these results. A documentation-only
delivery tail does not reidentify the sampled code or source manifest.

`identity.json` deliberately retains `formal_samples: "NotRun"`: it is the
**prepare-time snapshot**, not the final status. The unchanged start/finish
receipts and raw samples document the completed formal run. `/proc` was readable,
but `self_visible=false`; its process-name list cannot prove isolation. The basis
for the window is root coordination of the other agents' builds, native tests and
measurement work, not a claim of exclusive physical-host control. OS caches were
not flushed. Seven samples and exploratory paired bootstrap intervals cannot
establish a broad workload distribution or stable production effect.

### What the clocks include

* `candidate_total_ns`: extraction, event equations, planning, least fixed point,
  complete oracle comparison, support census, actual full-row materialization,
  canonical serialization/content observation and candidate/result destruction.
  Ordinary bookkeeping between phase clocks remains in this total.
* Oracle construction and destruction are separately reported. Freeze-only oracle
  canonical artifact encoding has its own Work and is absent from sample mode.
* `total_ns`: an **in-process observation interval**. It stops before final module
  JSON assembly, destruction of the module/source-file containers, stdout and
  process teardown. It is not an all-inclusive process total.
* `wall_ns`: the external fresh-process boundary, including startup, frontend,
  oracle, candidate, all releases, final formatting/output, and parent launch /
  capture overhead. This is the complete measured **research experiment** cost.
* `production_ns`: separately run public Analyze/Verify after the frontend. It is
  context at different precision and can stop at an early diagnostic. It is not a
  same-precision baseline; no LLVM/emission/native or compiler-wide benefit was
  measured.

## 2. What was checked

Preparation passed 21 research tests, clippy with `--all-targets -- -D warnings`,
and the release build. The tests include 256 possible three-bit input relations,
five transfers and three methods: **3,840 transition-image checks**, including
actual row enumeration and truth-table comparison. Nearest raw-TypedIR mutations
cover duplicate cleanup, reversed cleanup order, changed shared guards, dropped
scope tags, stale Desc/borrowed closures, overlapping construction-site activation,
and a unique carrier that still leaks at return. Real N-1/N/N+1 tests traverse the
solver, raw oracle and complete comparison/census/materialization/canonical/
fingerprint budget tail. The budget test's census visits every result relation,
which is broader than the report's point-only census.

The oracle independently interprets raw TypedIR. It does not call the candidate
extractor, operand traversal, event interpreter or BDD implementation. Every
candidate relation key is checked by exact cardinality plus membership of every
oracle row; canonical full relations are also actually materialized. Raw ordered
instruction, operand, scope-kind and CFG labels are independently extracted and
compared. This is stronger than comparing only hashes or marginals, but both
interpreters intentionally implement the **same documented finite abstraction**.
They do not constitute two independent specifications of all ZIL semantics.

| Evidence level | Count and interpretation |
| --- | --- |
| Frozen corpus | 10 module entry cases, 48 executable functions; includes all closure bodies |
| Complete frozen function-method pairs | 138 (46 functions × 3 methods); 276 candidate/oracle canonical files |
| Excluded incomplete function-method pairs | 6: three BDD-node failures and three oracle-choice failures |
| Eligible corpus | 8 module entry cases, 43 functions, 1,356 instruction/terminator points, 2,446 complete relation rows per method |
| Formal measured processes | 224 = 168 shadow module processes + 56 production-context processes |
| Complete measured function comparisons | 903 = 43 functions × 3 methods × 7 repetitions |
| Warmup processes | 64 = 48 shadow + 16 production; separate from the 224 |
| Frozen integrity | 1,817 source members; 357 frozen members including eligibility |
| Original sample command receipts | 288, including warmups |
| Derived summary | 2,990 seven-sample statistic groups; 1,112 paired-bootstrap ratio groups |

A module is eligible only if all three shadow methods have the same nonempty
function census and every function has outcome Complete. **Complete means the
query finished and agreed, not that its violation relation is empty.** In the
timed corpus 42 functions have no represented fault, while the deliberately
conservative `guarded_desc` function has a nonempty fault relation. Three complete
helper functions inside the two excluded modules remain in freeze evidence but
are not sampled as a substitute for their failed modules.

| Case | Functions | Points | Rows | Fault relation keys | Production context |
| --- | --- | --- | --- | --- | --- |
| tiny | 2 | 23 | 35 | 0 | Accepted |
| branch | 3 | 73 | 109 | 0 | Accepted |
| correlated | 3 | 97 | 135 | 0 | Rejected / E-UNINITIALIZED |
| closure | 6 | 155 | 193 | 0 | Accepted |
| error | 4 | 110 | 198 | 0 | Accepted |
| loop | 3 | 72 | 89 | 0 | Accepted |
| guarded_desc | 3 | 61 | 73 | 1 | Rejected / E-ANALYZE-IR |
| real_error_cleanup | 19 | 765 | 1614 | 0 | Accepted |

`real_error_cleanup` loads four actual source modules and includes 19 executable
functions, all supported here. It retains 20 Error scope exits as well as Return,
Fallthrough, Break and Continue tags and separate error/success terminal classes.
Its 765 instruction-or-terminator points are this query's counting unit; they
must not be relabelled as 055's different 853 graph-node count.

## 3. Precision result and its limits

`correlated.zil` uses the same Bool for complementary transfers. The joint owner
relation has `(LIVE,MOVED)` and `(MOVED,LIVE)` alternatives, not the four rows
produced by multiplying the two marginal sets. All three exact methods and the
raw oracle preserve that fact; changing one guard exposes the illegal transfer.
Frozen production reports `Analyze / SafeReject / E-UNINITIALIZED` before the
complete pipeline accepts the source. Therefore this is a **precision witness
relative to the documented resource query**, not permission to accept the program
or proof that only the resource kernel caused the production rejection.

The production context is visible in the frozen source:
`crates/zil-verify/src/resources.rs` joins per-carrier debt/fresh/provenance and
ordering information, while `crates/zil-analyze/src/resources.rs` joins cell facts.
These are plausible future integration sites for a separately proved joint query.
This research changes neither implementation nor the production accepted set.

The finite abstraction has material boundaries, detailed in [CONTRACT](CONTRACT.md):

* Numeric 0–15 values are exact; Top=16 includes those values too. Arithmetic,
  projections, aggregate payloads, ReadDesc and call results can become unknown.
  Payload/tag or original-Bool correlations through such operations are not kept.
* Future Desc/borrowed-closure use ignores guards. In `if b { move r }; if !b {
  use(d) }`, it can conservatively report invalidation on a path whose later use
  cannot execute. `guarded_desc` retains this false positive. The full relation is
  exact for this conservative query, not exact concrete path semantics.
* ScopeEnter/ScopeExit transfer the modeled state identically. Exact tags and
  operands are audited, but this is not an independent scope-stack proof. Error
  return tags are represented; language scope/error-cleanup validity remains with
  production Verify.
* Site tokens represent one active incarnation. Any reachable overlap yields
  whole-function `UnsupportedMultiplicity`; partial rows are diagnostic. Old Desc
  or borrowed-closure uses must be rejected before retirement/site reuse can make
  a stale position appear live. Raw mutation tests exercise this ABA boundary;
  schema-8 source-level resource replacement itself is unsupported.
* Caller resource effects are admitted abstractions. Give-result identity flow is
  limited to the unique Managed argument with the same nominal/payload type as
  the Managed result; different-type conversion is Unsupported. Arbitrary dynamic
  multiplicity, heap/permission histories and general affine callback returns
  remain outside the model.

At most one carrier, no unpaid debt at normal/error return, and cleanup ordering
are separate obligations. A sole leaked carrier is a counterexample to equating
uniqueness with completed cleanup. Abort retains its unpaid debt rather than being
labelled a normal paid exit.

## 4. Full-cost results

All medians below are milliseconds over seven measured processes. Candidate columns
include full comparison, materialization and candidate destruction. Wall columns
include the oracle and all process overhead. The production column contains only
the separately timed Analyze/Verify interval; rejected cases may stop early.

| Case | Candidate mono | Candidate cut | Candidate wire | Wall mono | Wall cut | Wall wire | Production interval |
| --- | --- | --- | --- | --- | --- | --- | --- |
| tiny | 0.2057 | 0.1791 | 0.1996 | 1.8942 | 1.7195 | 1.8392 | 0.0986 |
| branch | 10.6887 | 11.9133 | 12.8579 | 12.9242 | 13.9384 | 14.9984 | 0.2718 |
| correlated | 29.5231 | 30.6763 | 29.7835 | 32.0806 | 33.0489 | 32.9008 | 0.0755 |
| closure | 22.9488 | 24.7365 | 26.5132 | 25.9255 | 27.9777 | 29.9447 | 0.4662 |
| error | 14.6345 | 15.5232 | 16.6053 | 17.3933 | 18.2777 | 19.4719 | 0.3765 |
| loop | 4.8094 | 4.6156 | 4.8448 | 7.0641 | 6.9175 | 6.6050 | 0.2858 |
| guarded_desc | 2.4623 | 2.4275 | 2.5689 | 4.8376 | 4.4360 | 4.5420 | 0.1368 |
| real_error_cleanup | 193.2264 | 202.4921 | 209.4107 | 199.3034 | 210.0596 | 216.2608 | 1.4256 |

Ratios are ratios of medians; **greater than 1 is slower**. Brackets are the
frozen driver's exploratory 95% paired-bootstrap interval (1,000 resamples,
seed 59), not a theorem or an adjusted multiple-comparison significance test.

| Case | Candidate cut/mono | Candidate wire/mono | Wall cut/mono | Wall wire/mono |
| --- | --- | --- | --- | --- |
| tiny | 0.8706 [0.5400, 1.8719] | 0.9704 [0.5385, 1.3782] | 0.9078 [0.7787, 1.2290] | 0.9710 [0.7470, 1.3229] |
| branch | 1.1146 [0.8204, 1.5273] | 1.2029 [0.9083, 1.2170] | 1.0785 [0.7955, 1.5302] | 1.1605 [0.9289, 1.2080] |
| correlated | 1.0391 [1.0210, 1.1691] | 1.0088 [0.7594, 1.0690] | 1.0302 [1.0073, 1.1666] | 1.0256 [0.7721, 1.0671] |
| closure | 1.0779 [0.8045, 1.1322] | 1.1553 [0.8806, 1.2837] | 1.0792 [0.7999, 1.1348] | 1.1550 [0.8879, 1.2810] |
| error | 1.0607 [0.7642, 1.2609] | 1.1347 [0.9491, 1.1542] | 1.0508 [0.8037, 1.2101] | 1.1195 [0.9335, 1.1332] |
| loop | 0.9597 [0.8057, 1.1285] | 1.0074 [0.8473, 1.0799] | 0.9792 [0.7886, 1.2022] | 0.9350 [0.8799, 1.4007] |
| guarded_desc | 0.9859 [0.9346, 1.0837] | 1.0433 [0.9611, 1.2971] | 0.9170 [0.8301, 0.9815] | 0.9389 [0.8272, 1.2875] |
| real_error_cleanup | 1.0480 [1.0070, 1.1884] | 1.0838 [1.0003, 1.3082] | 1.0540 [1.0077, 1.1874] | 1.0851 [0.9952, 1.3088] |

Only `tiny` has a Wire candidate median below mono, with a wide interval crossing
1. Pure cut sometimes lowers small/loop medians, and `guarded_desc` wall time has
an exploratory interval below 1, but this does not offset the slower real case or
establish robust gain. Correlated precision does not automatically imply efficient
BDD representation. No aggregate geometric mean hides the two excluded modules.

The following function-level phase medians expose where cost occurs in
`correlated` itself. Phase medians need not add to the median total, and bookkeeping
also remains in the total. Oracle here is construction only; its destruction is a
separate raw metric.

| Method | Extract | Event | Plan | Solve | Check | Census | Materialize | Drop | Candidate total | Oracle |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| mono | 0.0668 | 0.5996 | 0.0949 | 25.1316 | 1.2246 | 0.5876 | 1.3430 | 0.2586 | 29.2149 | 0.2366 |
| cut | 0.0635 | 0.5855 | 0.2286 | 25.7262 | 1.1645 | 0.5538 | 1.3999 | 0.2895 | 30.4359 | 0.3031 |
| wire | 0.0628 | 0.5626 | 0.2212 | 25.3135 | 1.2001 | 0.5259 | 1.2864 | 0.2212 | 29.5934 | 0.2356 |

## 5. Structure and two opposite failure shapes

The table uses actual frozen function facts. Allocated BDD nodes are shown in
mono / cut / wire order. `plan count` covers one transition schedule; it excludes
unmodified incoming relation support and is **not** full interface width or CFG
treewidth. Reachable point BDD nodes and allocated intermediate nodes measure
different objects.

| Function | Bits | Points | Rows | Owners/tokens | Max point support | Max factor support | Max plan count | Max point BDD nodes | Wire substitutions | Allocated BDD nodes M/C/W |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| correlated/correlated | 147 | 78 | 114 | 16/2 | 147 | 3 | 21 | 293 | 48 | 68507 / 73793 / 72417 |
| real_error_cleanup/loop_error | 165 | 97 | 293 | 7/3 | 165 | 11 | 17 | 496 | 95 | 124616 / 134538 / 131660 |
| real_error_cleanup/main | 199 | 180 | 527 | 0/0 | 199 | 11 | 5 | 393 | 0 | 55193 / 53613 / 53613 |

The correlated function has a factor-support maximum of 3 and plan count 21,
yet its complete point relation can depend on all 147 bits and allocates tens of
thousands of BDD nodes. In `loop_error`, 95 Wire substitutions still produce a
slower measured function total (mono 64.0369 ms, cut 66.1958 ms, wire 70.6805 ms).
The real `main` has zero owner tokens and zero Wire substitutions, yet its scalar
relations and output account for substantial cost. Its mono/cut/wire totals are
40.9694 / 37.4203 / 39.0764 ms. These facts prevent assigning every local scheduling
change to a unique-resource shortcut.

### Four independent guards: representation failure

All three methods first fail with **`ResourceLimit: BDD nodes`** at the configured
300,000 allocated-node bound; each receipt records `oracle_outcome: Computed`.
The module is excluded as a whole. The failure branch does not retain the oracle's
row count/work or the candidate's peak layout; those missing measurements are not
invented or silently replaced by successful helper-function counts.

As a stated inference from the fixture, four independent input Bools give 16
entry choices; the admitted acyclic deterministic body gives at most 16 concrete
finite states per point. This is a derived bound, not a recorded row count. The
contrast with oracle completion points toward representation/intermediate-diagram
pressure. The experiment does not isolate fixed BDD variable order from the
owner×token matrix or event construction. Further diagnosis must distinguish them
before asserting a structural cause.

### Seventeen Bools: oracle/output failure

`state_limit/too_many` has 2^17 = 131,072 entry choices, exceeding the 100,000 explicit
row cap. The oracle returns **`ResourceLimit: oracle choices`**. The symbolic method
can hold the unconstrained relation with two terminal nodes, 17 bits and zero
point support, but reports `OracleResourceLimit` and zero materialized bytes.
This is incomplete evidence, not agreement or a successful zero-cost relation.
It shows the opposite bottleneck: a small diagram can denote output too large for
the independent explicit oracle and required full materialization.

Neither failure's partial internal timer is used for performance claims. The
100-million Work limit is **per function candidate invocation**, separately per
function oracle invocation; freeze-only oracle serialization is another bounded
invocation. It is not a 100-million budget for a whole module or the experiment.
See the contract's phase table for pre-allocation geometry, nodes/caches and row
limits; logical Work is not CPU instructions, RSS or a time limit.

## 6. Paper bridge and what could qualify next

[BRIDGE](BRIDGE.md) states the proof premises. If a fresh unique post port y has a
last-incident equation y=±x, x is overwritten, and every external association stays
in R, then `exists x. R(x,B) and (y=±x) = R(±y,B)`. The implementation checks the
port/factor admission and uses ordered BDD substitution. A shared guard is one
coordinate throughout; replacing it with independent copies is not an admissible
cut. This equality and early existential quantification are established algebra.
Bryant, Dechter and Chauhan et al. are prior art, not discoveries of this project.
The mono/cut/wire comparison is an internal same-representation ablation, not a
win against optimized partitioned/fused relational product or dynamic ordering.

Deng/Hani/Ma's free/fixed ends preserve one original bond variable, and the UP /
forest results have specific geometric and analytic hypotheses. Their fixed-end
invariant does not prove ownership, low width for arbitrary compiler CFGs, or a
compiler complexity bound. Tree Borrows emphasizes identity plus access-history /
permission state; RustBelt's affine logic does not imply mandatory cleanup;
RefinedRust illustrates separate translation, IR reasoning and adequacy obligations.
The actual source sections and our narrower inferences are in [SOURCES](SOURCES.md).
The title of the user's earlier personal attachment remains unverified.

A later candidate would need all of the following before a core proposal:

1. A production-relevant relation contract with preserved guard, identity,
   permission/borrow lifetime, debt and order at the whole semantic interface.
   The three ownership obligations must remain independently falsifiable.
2. Region summaries/reuse or a demonstrably compact representation with bounded
   construction and output. A token→owner functional encoding is a possible next
   experiment, but it must explicitly retain illegal duplicate-owner witnesses;
   silently overwriting a second owner would make the encoding unsound.
3. Whole-relation oracle agreement and nearest invalid controls across joins,
   loops, errors, captured identities and every admitted call boundary. Unsupported
   multiplicity and budget exhaustion must stay outside safety success.
4. Complete-cost gains including extraction, planning, materialization/destruction,
   and any necessary cache maintenance, over reasonable same-precision baselines
   including optimized partitioned approaches; independent review before installation.

The useful future mainline target is the Window/Model/Subscription interface with
callbacks surviving a frame or call. Today's package UI has only local view borrows
and moves the Model after last use. A persistent Subscription callback borrowing
Model would require cancellation/draining or a Hold contract proving no future
access before Model destruction. Its permission, identity and cleanup responsibility
must cross the same interface. Current declaration order and this event's plan
count cannot settle that future temporal problem. That scope is not implemented
in 059 or retroactively attributed to its schema-8 measurements.

## 7. Evidence, reproduction and preserved failure history

The raw run is `out/runs/research/059/schema8-c0c1dc7-r1` in the 059 worktree.
[acceptance.json](../../../evidence/impl/059/acceptance.json) binds the frozen
identities, counts, hashes and portable selection. [README](README.md) provides the
full driver commands and a small non-sampling replay that explicitly archives the
schema-8 source even when invoked from newer mainline history.

| Immutable raw member | SHA-256 |
| --- | --- |
| samples.jsonl | `34b4d7173d514434409cbe5459bc0b730bc800b7b726c7f082624f4329697759` |
| summary.json | `fd179e0f4f67d5f23848a867594aac721b8f3781e1aa0058045c84abde5df3e3` |
| identity.json | `5a274f32f1a3479d00f4c9c8bec9ebdeaf70a55fe2a124b3011587333e0f01de` |
| source-manifest.json | `a178c283cd51cbde76704276b82e04f21390ffd41b08e596f29c6189676e8060` |
| freeze-manifest.json | `78f725c055819008760f522526a6e4a5c42d1b94cc3a8a7f98fb0028da37c9a2` |
| sample-start.json | `30df3c9f74fc1444ad03fbd3e9bd7e1448ece3500535b3539900e3e9d01ff83c` |
| sample-finish.json | `3714449c9ae02b299898bd73872e5638940575ea902060badaed9f8c041f0c5c` |

The portable archive keeps the complete source, freeze/commands, every top-level
JSON/JSONL, original release executable and all seven identity-hashed rlibs. It
omits rebuildable debug/release intermediates and incremental caches; `run.py
verify` retains all files it actually checks. Original command absolute paths stay
unchanged as historical facts, while verification resolves members under the
restored run root. The separate member manifest additionally hashes retained rlibs.

Development fixture failures, the initial clippy style failures and one transient
exec transport failure are recorded in `development-history.json` as reconstructed
observations. Their original stdout was not saved as raw files; this is disclosed
rather than presenting the reconstruction as original evidence. The final prepare,
all freeze outcomes, and every formal/warmup command have original receipts.

The independent reviewer reported 21/21 tests on the exact code, exact Git/blob
checks for all 1,817 sources, 357 frozen members, 288 command receipts, all 138
canonical pairs, and independent recomputation of all 2,990 statistics and 1,112
ratios. An independent debug replay of the real 19-function case produced the same
38 canonical files byte for byte. The review's exact delivery binding and verdict
belong in the separately authored review receipt; this author does not issue that
verdict or treat a successful research check as a production admission.
