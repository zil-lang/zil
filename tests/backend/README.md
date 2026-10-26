# B0 scalar LLVM backend

These tests construct public Typed IR and explicit initialization claims, run
the actual independent `zil_verify::verify_module`, and feed its immutable
result to LLVM emission. They are **typed IR / backend evidence**. They do not
establish that parsing, types, or analysis accepts a corresponding ZIL source.
The source-to-native acceptance owner must record that separate path.

## Public API

- `zil_lower::lower(&VerifiedModule) -> CompileResult<LirModule>` preserves the
  checked scalar CFG and attaches its storage layout.
- `zil_backend_llvm::emit(&VerifiedModule, &LirModule)` emits LLVM text without an
  executable wrapper.
- `emit_with_options(..., &EmitOptions { entry })` supports `EntryPoint::None`,
  `EntryPoint::Main`, and `EntryPoint::ReturnValue(FunctionId)`.
- `build(..., &BuildOptions)` feeds `.ll` directly to clang through a process
  argument vector. `BuildOptions::new(output)` chooses `clang`, O0, and Main.
  `Optimization::O2` is available; an explicit compiler path is configurable.
- A successful `BuildArtifact` records the executable, its `.ll` sidecar, and
  the real compiler command. A failed invocation cannot authorize executing an
  existing output from an earlier invocation.

The main wrapper accepts a zero-argument ZIL main returning i32 or unit. An
explicit return-value harness accepts a zero-argument i32, i64, or bool entry,
calls that real ZIL function, and prints the full result as a signed integer.
Harness stdout is the return-value observation. Main's process status is a
different observation and is never used as a substitute.

## Scope and machine contract

The current target is checked x86_64 Linux LLVM. Supported values are unit,
bool, i32, and i64. Locals, SSA values, branches, loops, direct calls and ordinary
recursion execute as native code. Unit storage and call arguments are erased
only after their definition, initialization, and uses have been verified.

Ordinary add/subtract/multiply and negation branch to `llvm.trap` on signed
overflow. Division and remainder guard zero and minimum-integer/-1 before
issuing the LLVM operation. Wrapping operators use ordinary LLVM bit-vector
instructions without poison-producing overflow flags. Shift operations and
non-scalar types remain Unsupported. There is no source evaluation shortcut,
C/Rust transpilation, implicit managed runtime, or unsupported alias attribute.

Raw LIR grants no authority. Emission compares its module identity, fingerprint,
complete function contents, target, schema version and exact scalar layouts to
the supplied immutable verifier result before producing executable text.
LLVM function symbols are generated from FunctionId, never source names.

## Limited validation

With the verified Rust and LLVM tools on PATH (or `ZIL_CLANG` pointing to clang):

```sh
cargo test --offline -p zil-lower -p zil-backend-llvm
```

The backend has ten test functions containing sixty real clang/native
invocations. Thirty-two invocations exercise arithmetic trap boundaries under
O0 and O2. Other cases cover full-width positive and negative observations,
ordinary main status, wrapping arithmetic, comparison and bit operations,
branch/loop dataflow, recursive factorial, nonzero CFG entry, unit erasure,
identity-only symbols, seven LIR mutations and unavailable-tool cleanup.
These tests require working clang and linking; missing tools fail the tests
rather than causing a skip or fabricated Pass. The lower crate has two runtime
tests and one compile-fail documentation test protecting the raw-IR boundary.

No result here marks T07 resource layouts, responsibility cleanup, aggregate
ABI, closure environments, or native-resource acceptance complete. Those
require their own checked schemas and source-level cases.
