# Transparent generic aliases across four modules

`model` defines the actual generic record, enum and identity function. `facade`
exports generic aliases and a concrete enum alias; `transform` constructs and
matches those values through the facade. `main` uses a compile-time record table
and reads one input byte through the existing scalar C interface.

For a byte `x`, the program outputs one byte `(2*x + 42) % 256` and exits zero.
Empty input exits one without output. The `Maybe.None` branch handles byte zero;
the record table contributes `3*4 + 5*6 = 42`. It uses the production source,
type, independent verification, LLVM and linker chain. Alias names do not add
runtime types or storage.

```sh
zil build examples/generic_alias/main.zil -o alias-demo
printf 'A' | ./alias-demo
```

The task's retained tests execute empty input and bytes 0, 1, 65, 127, 128 and 255
at both O0 and O2, with independently computed expected bytes. The results are
functional evidence, not a Rust performance comparison.
