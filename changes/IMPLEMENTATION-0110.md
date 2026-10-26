# 0110: Migrate the old enum-CT regression to the approved 105 contract

The first formal run `20261025T134620Z-6561d6f5` at baseline
`2bb82a80449fdbed431c479303b987ac0f5769f3` retained a real 34/35 failure:
`enum029_13` still expected every enum CT program to fail with
`E-CT-FORBIDDEN`. Task 105 had already approved and independently validated
the finite Copy-enum CT path. The original failed run remains preserved by
integration; this slice does not overwrite its result.

The test-only source commit is `0886b8331a0f85526902bde47f03be6feb1bc8b4`.
Its sole code-file change is `crates/zil-driver/tests/impl029.rs`: rename the
old regression to describe the finite supported path, require independent
`verify_ct_program` to succeed, then actually call `evaluate_program` and
assert the exact result `CompileTimeValue::Scalar(Constant::I64(42))`.
The raw program construction, target and every limit remain unchanged.

The second half of the same test is byte-for-byte unchanged. A runtime enum
pattern binding named `helper` still shadows the global function, and its CT
call must remain `SafeReject` with `E-CT-RUNTIME`. No test is removed, ignored,
filtered away or replaced by a weaker success-only assertion. No production,
budget, formal gate, build configuration or old research file is modified.

## Actual bounded validation

Using the existing shared toolchain environment and a fresh exclusive
`/tmp/zil-110-target`, the exact source commit passes all 16 `impl029` groups
and all 12 `impl105_ct_enum` groups. This includes the migrated positive and
its unchanged runtime-shadow rejection, raw/wire enum checks, non-Copy and
foreign-capability rejections, budget boundaries and real native consumers.
There are zero failed, ignored or filtered tests. `cargo fmt --check` also
passes. The run inherits the established environment settings; it changes
only the dedicated target/evidence destinations and selects these two test
targets, without changing the formal invocation or its parameters.

The exact commands, environment, source hashes, stdout/stderr and both actual
Rust test executables are retained under `evidence/impl/110/raw001`. Existing
105 report artifacts are also retained. The unchanged 029 helper's temporary
native artifacts retain their original cleanup behavior; this slice does not
claim to have changed that helper to persist those temporary files.

The storage archive reuses the already reviewed 109 archive codec without
modifying it. Its fresh destination and data are solely under task 110; the
receipt records the codec identity and complete per-path restoration checks.
Root must independently review the exact test delta, integrate it and rerun
the original 35 necessary gates. Passing this bounded repair is not reported
as a replacement for that formal run.

## Rust-goal accounting

This is consistency repair for an already approved language capability.
It adds no language feature, optimization or performance result. Runtime,
compile-speed, memory and comparative maintenance conclusions are unchanged;
the previously frozen research sources and measurements remain untouched.
