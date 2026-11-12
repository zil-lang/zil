# Native work-product observation scopes

These counters observe the explicit native-products path. They neither enable
reuse nor change O2, independent Verify, admission, keys or full result equality.
Clock reads and accounting overhead remain in request wall and native totals.

Observer schema 3 appends six `u64` fields after the original 26 C boundary
fields. The original field offsets stay fixed. Schemas 1 and 2 have no IR-scope
observations: the evidence decoder exposes those scopes as unknown, never zero.
Schema 3 requires all six nonnegative integer observations.

| Field | Actual scope |
|---|---|
| `ir_key_ns` | After supported entry-key admission: bounded economic-group lookup/refusal, eligibility, current analysis facts, complete Input/World and pipeline key construction, including early-return failure cleanup. Slot occurrence advancement happens before admission and is outside this scope. Stops before prior-slot lookup, unpack or replay. A later selected-slot negative-filter digest construction/comparison is also charged here. |
| `ir_unpack_ns` | Complete decoding of a selected prior Slot, including lossless frame/decompression checks and typed metadata reconstruction. |
| `ir_replay_ns` | Entire replay function: parsing, cloning, complete validation, analysis-manager work and owned local cleanup, including parsed Module destruction and failure returns. |
| `ir_parse_ns` | MemoryBuffer and `parseBitcodeFile` construction only. Stops before subsequent Module/Buffer destruction. **Already included in `ir_replay_ns`.** |
| `ir_capture_ns` | Complete `afterStage` callback: pending-token and supported-domain gates, current admission/world/facts, body snapshot, new-stage packing and callback-owned cleanup. Includes cheap early returns, not the original LLVM passes that precede the callback. |
| `ir_pack_ns` | New-stage retention: original admission, lossless frame/SHA/zstd, typed metadata, publication and local source-product cleanup. **Already included in `ir_capture_ns`.** Re-retaining an existing immutable packed record is not a new pack. |

These scopes are not a partition of optimizer or native time. Never add parse
to replay or pack to capture. Slot matching, callback dispatch, original module,
CGSCC and unsupported function-pass work, and other unassigned work remain in the
optimizer/native totals. Caller-owned decoded Stage cleanup is not the parsed
LLVM Module cleanup measured by replay.

Observations describe the first hooked native attempt. A rejected replay may
have recorded reuse before the invocation is abandoned. Subsequent fresh-context
ordinary work has no hook observations; its per-pass counts are unavailable.
Native receipt totals include that fallback, and request wall also includes
all driver retries. Counter hits and scoped costs do not by themselves establish
net latency savings. Compare exact timer definitions at each frozen source.

A packed Stage may retain a complete-entry **negative** summary: four ordered
text lengths and fast digests, plus the unchanged full typed entry-key list.
An unequal summary can avoid decoding a stale packet; an equal summary always
continues through complete decode and the original Input/World/Pipeline/
EntryFacts/ordered typed-key equality. Digest collisions cannot authorize reuse.
Summary metadata remains inside the existing admission caps. This removes
unnecessary miss decoding, not the current key projection or any LLVM check;
net wall effects still require measurement at the exact frozen source.


## Bounded optional FPM recording and replay

The original LLVM function-pass group always runs unless a candidate passes the
existing complete entry comparisons and replay checks. A separate economic gate
can refuse optional recording or a replay attempt; it never changes O2, analyses,
Verify, compiler work budgets or machine products. A failed replay still rejects
the entire native attempt. Economic samples are scheduling hints, not semantic
proofs, and do not enter the input, world or result equality.

A group is the full pipeline text, structural pass-manager path and current
instruction-count floor-log2 bucket. Function names and fixture identities do
not select a policy. A bounded Request-local PM-address map avoids repeating the
economic pipeline rendering; address reuse can affect only whether to try the
cache. Every actual Stage still freshly renders its full authoritative Pipeline
and checks it with the other original entry fields. Slots use the private
`fpm-slot-v3` prefix. Every effective phase-1 function hook advances its occurrence,
including refused, unsupported and nested-suppressed hooks.

Each successful Products snapshot freezes one of three modes for its children:

- Probe permits at most two complete key attempts and two capture opportunities
  per group per Request, in current call order. An absent old Slot is fresh work
  and may seed a new pilot; old pilots are not waited for indefinitely.
- Reuse permits the original complete replay attempt. Two consecutive misses,
  including absent Slots, stop this Request's later key/capture work for that
  group. A hit clears that consecutive-miss count. Captures remain capped at two.
- Bypass runs the original group and records nothing. After eight successful
  snapshot-lineage native epochs it can enter a fresh bounded Probe.

At most four original-group runtime samples are taken per group per Request.
Capture tokens own an entry snapshot; TimeOnly tokens do not, and their after
callback cannot create one. During such a sample all nested optional FPM cache
activity is suppressed, with original passes and Slot advancement retained.
Nested observer intervals are measured as a union and deducted once from the
sampled original runtime. The current group's own before/after callbacks lie
outside that runtime. Suppression is Request-owned, ends before capture, and
cannot escape Finish/Abort or survive a discarded request.

The stored minimum original runtime F, maximum key-plus-capture cost C and
maximum successful complete-hit cost H are updated conservatively. H includes
key, selected decode, replay and callback-owned entry cleanup; C includes entry
work plus the entire capture callback and pending cleanup. Parse is already in
replay and pack already in capture, so neither is added twice. At least two fresh,
two captured and two exact-hit samples are required before choosing Reuse for the
next successful epoch, with `F >= 2H` and `C <= 2(F-H)`. Promotion is never visible
partway through an invocation. Missing or invalid clock samples cannot promote a
policy; overflow is checked or saturated. These rules are heuristics, not a
promise of latency improvement.

There are at most 128 persistent group records and 512 KiB of their charged
metadata. Their complete key strings and sample structures are included in the
original decoded and encoded 64 MiB admission counters and retained-payload
receipt. Request pipeline hints have independent 128-entry/128 KiB string bounds;
pending tokens are capped at 128 and observer nesting at 64. Hitting a bound
refuses optional work. Native history retains the same 128 MiB limit. These are
logical payload bounds, not RSS measurements.

Policies publish only with successful Products. Abort, cancellation or failed
replay drops speculative samples. Restoring an old snapshot also restores its old
policy: this is progress along that snapshot lineage, not a global session-wide
learning table. Existing independent replay/owner counterexamples may explicitly
scope a test-only Always guard to disable economic refusal; no test configuration
enables it implicitly. Production paths and economic lifecycle fixtures use the
real gate. Bounded sampling and cache hits alone do not establish a full push
pipeline or an end-to-end performance result.
