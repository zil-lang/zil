# Direct scoped callback machine control

`direct.zil` checks synchronous registration and rejection while closing.
`direct-null.zil` checks a synchronous callback followed by null registration:
its body calls a C function that exits with status 94 if entered, and its close
function exits with status 93 if entered. Both C drivers finish or clear every
entry before returning, satisfying the external no-future promise for these
small same-thread controls.

A frozen native observation used compiler `1252dbb23164291cf3481c2adebd7716e3c1811f`. Rebuild that exact revision to reproduce its result; for a current checkout, use its own compiler binary and record a new result:

```sh
export CARGO_TARGET_DIR=/tmp/zil-scoped-direct-target
cargo build -p zil-cli --offline --locked -j 2
case_dir="$PWD/tests/foreign-scoped-callback/direct"
zil="$CARGO_TARGET_DIR/debug/zil"
out=$(mktemp -d)
for mode in direct direct-null; do
  if [ "$mode" = direct ]; then shim=clang-with-c.sh; else shim=clang-with-null-c.sh; fi
  "$zil" check "$case_dir/$mode.zil" --json
  for backend in clang llvm-api; do
    for opt in O0 O2; do
      binary="$out/$mode-$backend-$opt"
      "$zil" build "$case_dir/$mode.zil" --backend "$backend" --clang "$case_dir/$shim" --opt "$opt" -o "$binary" --json
      "$binary"
    done
  done
done
```

Both sources passed the production Parse→Analyze→Verify→Lower→Build chain;
all eight linked binaries exited 0. The LLVM API build reports
`input_kind=builder`, `parse_ns=0`, and `verify_before_after=true` for O0 and O2.
These controls cover primitive direct callbacks; the broader owning and loop
consumer cases live in the parent feature suite.
