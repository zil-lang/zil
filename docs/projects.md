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

`root` is required. `tests` and `[dependencies]` are optional. Each dependency path names a directory containing its own `Zil.toml`. Dependency aliases must be ZIL identifiers. Entries and listed test files must be `.zil` files inside their project root. Unknown keys, duplicate keys, dependency cycles, symlinked source-tree entries, and imports outside declared project roots are rejected. There is no registry or network resolution.

Run `zil lock PROJECT` after creating or changing the project. It writes `Zil.lock`, a content fingerprint of the manifests and `.zil` files in the declared project graph. `zil check PROJECT`, `zil build PROJECT`, `zil run PROJECT`, and `zil test PROJECT` require an exact lock match. `PROJECT` may be a directory or its `Zil.toml` path. Single-file commands still work without a manifest.

`zil test PROJECT` discovers ordinary, defined, zero-argument `#[test]` functions in the root and listed test files. Each test compiles and verifies its whole import graph, then runs as a native executable. `unit` returns success, `bool` returns success for `true`, and `i32` becomes the process status. Compilation errors, nonzero exits, aborts, empty selection, and timeouts fail the command. `--test-timeout-ms N` sets the per-test deadline (default 5000 ms).

Imported modules currently share a project-wide logical path namespace. If two packages use the same logical import path for different files, compilation reports `E-MODULE-IDENTITY`; give the modules distinct logical paths. This avoids silent rebinding while per-package namespace resolution is not yet implemented.
