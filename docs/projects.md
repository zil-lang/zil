# Local projects

A project has a `Zil.toml` in its root directory. The supported manifest is a strict, UTF-8 TOML subset:

```toml
[package]
root = "main.zil"
tests = ["tests.zil"]

[dependencies]
math = "../math"
# Equivalent: math = { path = "../math" }
```

`root` is required. `tests` and `[dependencies]` are optional. Paths use simple quoted strings without escapes. Each dependency path names a directory containing its own `Zil.toml`; dependencies may declare further local dependencies. Dependency aliases must be ZIL identifiers. Entries and listed test files must be `.zil` files inside their project root. Unknown keys, duplicate keys, dependency cycles, symlinks in dependency source trees, and imports outside declared project roots are rejected. There is no registry or network resolution.

Run `zil lock PROJECT` after creating or changing dependencies or manifests. It writes `Zil.lock`, a content fingerprint of the manifests and dependency `.zil` files in the declared project graph. Editing the root package's own source does not require a new lock; those bytes still participate in normal compiler and cache identity. `zil check PROJECT`, `zil build PROJECT`, `zil run PROJECT`, and `zil test PROJECT` require an exact dependency lock match. `PROJECT` may be a directory or its `Zil.toml` path. Single-file commands still work without a manifest.

`zil test PROJECT` discovers ordinary, defined, zero-argument `#[test]` functions in the root and listed test files. Each test compiles and verifies its whole import graph, then runs as a native executable. `unit` returns success, `bool` returns success for `true`, and `i32` becomes the process status. Compilation errors, nonzero exits, aborts, empty selection, and timeouts fail the command. `--test-timeout-ms N` sets the per-test deadline (default 5000 ms).

Imports are resolved from the importing package. Two dependencies may each `import helper` and reach their own `helper.zil`; a dependency cannot use another package's alias unless it declares that dependency itself. Every imported file keeps its own canonical source and module identity through parsing, compile-time evaluation, verification, and native code generation.
