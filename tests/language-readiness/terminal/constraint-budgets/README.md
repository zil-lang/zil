# API stack and source-budget controls

The original immutable `let` and legal mutable `var` fixtures remain unchanged
under `codegen-pending/large-copy-enum-error-borrow-capture*.zil`.

The existing `aggregates` target contains the isolated default-stack regression:

```sh
export CARGO_TARGET_DIR=/tmp/zil-aggregate-stack-target CARGO_BUILD_JOBS=1
export TMPDIR=/tmp
cargo test -p zil-driver --features llvm-api --test aggregates large_copy_enum_error_default_stack_api_reliability -- --exact --nocapture --test-threads=1
```

It removes `RUST_MIN_STACK` in each subprocess and calls `std::thread::spawn`
without changing the default stack. Frontend controls and all four actual native
backend/optimization profiles run on that worker. An abort cannot kill the
parent test harness.

`locate-stack.rs` is a separate diagnostic consumer of existing rlibs; link
`abort-trace.c` as an object with `-C link-arg=OBJECT -C link-arg=-rdynamic`, add
`--extern zil_driver=DRIVER_RLIB --extern zil_base=BASE_RLIB`, and add dependency
and configured LLVM native search directories. It accepts `SOURCE [default|BYTES]
[parse|core|types|analyze|verify]`. The diagnostic handler intercepts SIGABRT on
an alternate signal stack and emits native addresses; `addr2line -Cf -e EXE`
resolves the reported offsets. This signal handler is outside the compiler.

`constraint-budget.rs` links the same target's zil_base/zil_syntax/zil_expand/
zil_types rlibs. It runs on the ordinary process main thread and prints the exact
source-inference work boundary. The frozen comparison printed 2520 in each bound compiler. Recheck the boundary for a new compiler.

`depth20` targets Types closure-inference depth; `depth32` and `projection32` target Builder/aggregate frame depth.
The48-layer controls exercise both nested literals and ladder initialization.
All are actual source, preserve ordinary Copy semantics, and now reach Verify.
