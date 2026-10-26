# Native scalar C ABI: stdin → ZIL → stdout

`main.zil` imports the ordinary ZIL `stdio` module, reads real standard input,
converts ASCII lowercase letters to uppercase, and writes real standard output.
Only the two private C imports inside `stdio.zil` cross an unsafe boundary.

```sh
zil build examples/impl044/main.zil -o uppercase
printf 'Hello Zil 044!\n' | ./uppercase
python3 tests/integration/impl044.py --zil path/to/zil --out out/impl044
```

The executable returns 0 for nonempty valid input, 2 for an invalid byte, 3 for
initial EOF, and 4 for an output error observed by `putchar`. Printable ASCII,
tab, LF and CR are accepted. A valid prefix stays written if a later byte is
invalid. This is stream processing, with no transactional rollback.

## Explicit external contract

This adapter targets the checked x86_64 Linux C ABI. The linked libc must obey
the exact `int getchar(void)` and `int putchar(int)` signatures, return the
documented byte/EOF or output status, and not unwind into ZIL frames. No ZIL
address, ownership, callback, cancellation handle or retained storage crosses
the boundary. libc's stream locks, buffering and any internal allocations are
part of the external implementation, not static language guarantees.

The bounded scalar adapter cannot distinguish EOF from an input error, and it
does not observe a buffered flush failure occurring after main returns. The
`/dev/full` test supplies enough output to force an error during `putchar`.
The Rust baseline has precisely the same contract and limitations.

## Evidence and Rust goals

`tests/integration/impl044.py` builds actual ZIL and Rust programs at O0/O2,
checks fixed and seeded dynamic inputs, invalid/empty input, and `/dev/full`.
It records input/output hashes, exit codes, exact build commands and stage
artifact hashes. `evidence/impl/044/uppercase.ll` is the compiler-produced LLVM.

| Goal | Judgment, evidence and remaining gap |
|---|---|
| Rust-like safety | This bounded scalar call boundary has independent declaration/CFG checks and forged-IR rejections. Foreign implementation compliance remains trusted. Pointers, asynchronous I/O, resource transfer and full-language safety are outside this slice. |
| Runtime and memory | All 32 paired native behavior observations agree at O0/O2. No timings or peak-memory measurements were taken, and libc's internal costs remain in the program. |
| Expression and maintenance | A private unsafe adapter exposes ordinary module functions. This is a runnable capability, not proof of a maintenance advantage over the equally small Rust adapter. |
| Compilation cost | A real front-to-back build succeeds; no paired compile-time profile was measured. |
| Substantial breakthrough | Not demonstrated. This slice enables useful input-driven programs; the existing product gates remain unchanged. |
