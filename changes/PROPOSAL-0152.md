# 152 — growing single-thread FIFO, close versus cancel

Ordinary ZIL library only; no compiler, ABI/schema, permission or budget changes.
A unique ByteBuffer in an owned FnMut Package grows geometrically to an explicit
maximum. Reserve occurs before rotation or length publication; failure preserves
all queue bytes, count, head and capacity. Successful growth uses in-place rotation,
including wraparound and a clipped last allocation smaller than doubling.

Close rejects producers and drains FIFO, releasing immediately when empty. Cancel
rejects producers, discards pending byte events, and immediately releases storage.
Both are idempotent terminal actions. No concurrent producer, active callback,
asynchronous OS operation or resource-owning event is cancelled by this profile.

Verify via O0/O2 real native FIFO consumers, independent per-operation model,
actual allocator failure injection and allocation/free accounting. No speed claim
from allocation counts. Formal gates and independent review remain NotRun.

Validation: 5 native/negative tests, plus 32 deterministic deque-model traces × O0/O2 × 6 allocator schedules = 384 executions / 99840 commands; 230 actual allocator failures including 94 wrapped-growth failures, all released. This is allocator-fault byte FIFO validation, not async cancellation or generic owning element storage.
