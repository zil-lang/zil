# Explicit O2 compilation with complete call closures

`--incremental-o2` remains opt-in and requires an O2 build. It partitions stable
source chunks, then gives each consumer a complete transitive closure of directly
called source implementations. A cross-unit copy has an Internal symbol and is
actually emitted if not inlined. Only direct call targets change: canonical
definitions, function values, callback arguments, global initializers and
indirect calls keep their original identity. The default export contract remains
in force unless `--closed-executable` is separately selected.

The total additional copy cost may not exceed the original source-function graph
cost, counting machine instructions and blocks. This bounds copied bodies, not
total process RSS. A closure exceeding that whole-plan guard, needing local
definitions from another unit, or crossing an unsupported compiler-owned body
falls back to the original whole-module optimization. It never takes a truncated
prefix and presents it as complete. `native_codegen.fallback` and `actual_units`
report the choice in the existing JSON output. Indirect-call optimization is not
covered by the direct-call closure guarantee.

Current independent Verify still runs on every build. Native cache hits receive
current machine preflight, while misses receive LLVM verification before and
after optimization. Complete generated bodies and canonical declarations enter
the normal machine cache key. The action namespace is explicitly versioned as
`ZIL-OBJECT-ACTION-212-v2-complete-direct-call-closures`; the LLVM pass-pipeline
receipt continues to describe the actual LLVM passes rather than a research
toolchain label.

```sh
zil build main.zil --backend llvm-api --opt O2 --function-merging after \
  --incremental-o2 --codegen-units 8 --codegen-jobs 2 \
  --object-cache .zil-objects --json -o program
```

The earlier bounded available-externally policy and its measured runtime failures
remain separate evidence. The new policy was motivated by actual-unit capture,
four-way closure/linkage ablations, and a private structured candidate. Production
performance and runtime conclusions must use the final production compiler and
library identity; old 888 or private-candidate results are not relabeled.

See [the proposal](../../../changes/research-incremental-o2-quality-closure.md)
and [research tooling](../../../tools/research/incremental-compilation/quality_closure/README.md).
