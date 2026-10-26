# T04 query, cancellation, cache, and editor work

## Implemented paths

`zil_driver::CompilationSession` can be retained by an editor/build server. It
reuses immutable Parse, Core, evaluated-CT Core, TypedModule, and CandidatePlan
snapshots for exactly matching source identity, compiler rule/schema/cache
version, applicable target, limits, and explicit build inputs. Query events in
`CompileReport` distinguish executed phases from reused phases. Parse and Core
are target-independent; a target change recomputes the later stages.

The persistent ArtifactCache still stores **raw source bytes only**. A disk hit
is not an incremental type-check or an implementation cache. Source bytes must
match the current input; the whole one-shot pipeline still runs. No disk API
serializes or decodes VerifiedModule or VerifiedCTIR. In-memory sessions retain
no verified wrapper either: every request reaching Verify calls
`verify_module` again, and `verification_runs` records that call. A test mutates
an internal raw typed snapshot and observes the fresh verifier reject it.

The database tracks semantic, ABI, private-implementation, source-position, and
build-input edges, validates the complete dependency closure, rejects active
query/cached cycles, and automatically invalidates affected consumers on
publication. Five independent mutation scenarios demonstrate the different
actual reverse dependency closures. The current whole-module compiler queries
conservatively depend on complete implementation identity. **Cross-module
private-body editing, interface-only recompilation, and aggregate layout
incrementality remain pending.** PLAN-T04-001 is therefore not supplied as a
complete source-level fixture.

## Cancellation and artifact publication

CancellationToken supports a shared cancellation request and an optional
cooperative deadline. The driver checks it before/after phases and on cache
hits; an observer can cancel during real phase scheduling. Cancelled/incomplete
queries do not publish snapshots. The cache uses a bounded read, version/key/
length/digest checks, an actual temporary write and sync, and rename publication.
A cancellation injected after the temporary write leaves no new candidate and
preserves a previous committed candidate. Truncated, wrong-version, bad-key,
bad-length, bad-digest and oversized bytes are rejected and can be rebuilt.

Cancellation is cooperative, not preemptive inside one parser/verifier call or
an already-running clang process. Source/analysis/CT limits still bound the
internal phases. Cancellation observed after linking removes the newly built
executable and IR instead of returning an executable artifact. Runtime program
execution is not governed by the compiler deadline.

`--compile-timeout-ms 0` exercises an immediate real CLI cancellation, and
`--profile managed` returns Unsupported. It does not silently add a GC or RC.

## Diagnostics and native entry

EditorSession and the CLI use the same CompileOptions, target, limits and
semantic driver. Native main checks happen in the shared Types entry: duplicate
main is E-NAME-DUPLICATE; an invalid known main signature is E-ENTRY-SIGNATURE.
Original Core is emitted before explicit CT evaluation; `ct-core` shows the raw
result that re-enters ordinary type checking. The source `#(1 / 0)` error retains
Phase::Ct.

The `zil-ide` diagnostics example is a thin process adapter to the actual editor
API. `diagnostic-parity.py` compares eight inputs through the real CLI process
and that editor process, including success, syntax/name/type/entry errors, CT
failure and an Unsupported capability. It compares result categories, phases,
full diagnostics and success/failure process status.

`tests/script/native-protocol.py` separately exercises real native `build` and
`run`, a full 64-bit named return value, explicit managed rejection, cancellation
and missing clang. It is scalar sub-evidence only: imports, resource/error
cleanup and their native-run equivalence remain pending, so PLAN-T04-003 is not
supplied as a complete fixture.

## Limited validation commands

```sh
cargo test --locked --offline -p zil-db -p zil-driver -p zil-ide --lib
cargo build --locked --offline -p zil-cli
python3 tests/incremental/diagnostic-parity.py
python3 tests/script/native-protocol.py
cargo clippy --locked --offline --no-deps -p zil-db -p zil-driver -p zil-cli -p zil-ide --all-targets -- -D warnings
```

This work adds complete fixtures for PLAN-T04-002 and PLAN-T04-004 only. The
canonical T04 plan still selects all four cases: missing 001/003 must remain
non-Pass. SC01/SC03/SC04 are separately runnable source conformance cases.
Worker runs do not determine task completion; the integration owner records
final evidence against the merged commit.
