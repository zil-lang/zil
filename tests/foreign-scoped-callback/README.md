# Scoped foreign callback consumer

The three ordinary ZIL modules and `driver.c` are inputs to the actual compiler
and native linker. `bridge.run` is a public generic wrapper; it borrows the real
callback location and consumes a distinct body. The C driver uses a bounded
same-thread stack to simulate registrations and clears both code and userdata
roots before returning from close. It never calls through a cleared root.

`ORACLE.json` comes only from reading source operations and the frozen protocol
in design §12.4.1. `tools/language/foreign-scoped-callback/oracle.py` does not read
compiler reports or runtime observations. It predicts source cleanup, callback
counts, packed status/result bits, stdout, exit and six real allocator counters.
Preparing this oracle does not establish compiler or native success.

Inputs `0` through `7` cover Fn, owning FnMut, register-null before and after a
synchronous callback, two actual instances of the same concrete closure type,
nested distinct concrete closures, signed i32 boundaries, and repeated scopes
in a loop. FnMut bodies call the callback repeatedly; an active callback invokes
the same entry recursively, which must return status 1 without entering f.
Every close also calls its entry and requires the same rejection. The source
calls f after close to check restored access and persistent mutable state.

Owning cases prepare a real Mark and a nonempty ByteBuffer for f and the body.
The body is passed as the last argument, so its allocation can fail after f is
prepared/borrowed. The nested body handles its own allocation failures normally,
then closes the outer registration. Null registration never enters the body and
drops its remaining environment exactly once. Body cleanup precedes close;
callback environment cleanup follows close. Every returning allocator path has
zero live allocations. The disabled-injection normal run serves as its control;
fault runs include each request and one beyond the complete operation.

Inputs `8` and `9` trap inside f and body respectively. These are subprocess
abort controls, not recoverable error paths. The oracle expects Linux LLVM trap
signal 4 and the flushed prefix only; it claims no destructor/atexit or live-zero
guarantee after abort. The C driver obeys the external protocol in these controls.
It does not invoke undefined C behavior as a language-safety test.

Temporary output directories use input-byte hex labels. No historical evidence,
full-output archive, transport round trip or permanent audit store is required.

`negative/` contains 13 standalone source controls. Their risk descriptions are
frozen in RISKS.json; the one functional Rust target also matches source phase,
code and relevant reason, with zero machine execution for illegal cases. These
are obligations, not a claim of rejection before actual execution. The raw
verifier's own codes are not substituted for the first source diagnostic.
