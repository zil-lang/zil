# Explicit owned byte storage

This is a ZIL language consumer, with all growth policy and sequence algorithms
written in ordinary ZIL. `bytes_reserve` is the explicit fallible allocator
operation; `bytes_push` uses existing capacity and never allocates. The managed
head is inline, contains no reference count, and is cleaned once on ordinary
and error exits. Shared or mutable slices borrow the actual storage.

`main.zil` reads an arbitrary finite byte stream, grows storage, transfers its
owner through an ordinary factory, uppercases ASCII through a returned mutable
window, and writes all bytes back. NUL and non-ASCII bytes are preserved. This
is a byte container, not yet a UTF-8 String or a generic owning-element Vec.
The same `sequence` generic routines also work on fixed-array slices.

`sequence.ascii_upper(byte)` is an ordinary public value function for all 256
byte values. It maps ASCII `a` through `z` to `A` through `Z`, preserving every
other value. Its checked subtraction runs only in the lowercase branch.
`sequence.uppercase(window)` takes an explicitly mutable window and assigns
each element the value returned by that helper. It does no access for an empty
window and visits no bytes outside a subslice. The existing Types, Analyze and
independent Verify checks establish the required access; this is ordinary ZIL
source, with no backend permission to speculate writes through shared pointers.

```sh
cargo run -p zil-cli -- build examples/mainline/byte_buffer/main.zil -o out/byte-buffer
out/byte-buffer < input.bin > output.bin
```

`main.rs` is an independent observable behavior oracle, not the safety checker.
`portable.zil` exercises the same storage, generic methods, borrowing and owner
transfer without libc I/O, for Linux and Win64 allocation ABI checks. Win64
uses an explicit kernel32 import archive containing process-heap functions.

Unsupported in this slice: ByteBuffer capture/package fields, ownership inside
error unions, custom allocators, FFI buffer transport, asynchronous users,
owning elements and UTF-8 encoding invariants. These remain language work;
they are not silently replaced by runtime ownership machinery.
