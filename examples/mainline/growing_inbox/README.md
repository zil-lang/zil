# Growing byte inbox (152)

This is an ordinary single-thread ZIL library composed with owned `FnMut` and a
`Package` owning one `ByteBuffer`. It is not a new runtime primitive, scheduler,
actor, generic owning-element container, or asynchronous cancellation protocol.

`queue.make(initial, maximum)` returns `impl FnMut(i32)->i64`. The current finite
language profile uses a command protocol, not a finished typed Queue public API.
Posting 0..255 returns pending count. -1 receives (empty -4); -2 closes and drains;
-3 queries count; -4 actual capacity; -5 discards and immediately releases; -6
queries closed. After closure posts return -1. Maximum reached is -2, invalid or
failed initialization -3, invalid command -5, allocation failure during growth -6.

Full ring growth first reserves, then rotates its existing logical order by
three in-place reversals and initializes additional slots. Failed reserve changes
no logical bytes, length, capacity, head or count. No second explicit buffer is
allocated; the system allocator may internally copy or temporarily retain memory
while reallocating, so this is not a promise about physical peak RSS.

Push/pop without growth are O(1). Each geometric growth is O(new capacity), and
all growth up to a fixed maximum is O(maximum), including a clipped final step.
Cancel uses the already supported Package field replacement and frees immediately.
Graceful close rejects new events, keeps pending events, and frees at last pop.

Validation includes independent deque traces, actual `realloc` fault injection,
wrapped-full growth failures, 65536-slot native traces, and a failed last growth
that retains/drains all 49152 pending bytes. See tools/research/152 and separate
raw evidence. The compiler still rejects copying an owning callback.
