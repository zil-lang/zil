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
| `ir_key_ns` | After supported entry-key admission: eligibility, current analysis facts, complete Input/World, path and pipeline key construction, including early-return failure cleanup. Stops before prior-slot lookup, unpack or replay. |
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
