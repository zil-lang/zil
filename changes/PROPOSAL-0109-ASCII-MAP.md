# 0109: Ordinary ASCII value mapping and explicit mutable-window writes

The authoritative contract is the main-tree `.task/impl/109.md`. The baseline is
`ab285414c9bf97cdd2a37680c087f31db1623e5c`. This proposal implements an ordinary
public ZIL library operation and a separately measured two-factor composition.
It does not add a compiler transform or language permission.

## Source contract

`sequence.ascii_upper(byte: u8) -> u8` returns byte minus 32 exactly for ASCII
97 through 122, and returns the original byte otherwise. It has no writes,
allocation, I/O or foreign calls. The subtraction remains inside that branch;
bytes below 32 do not acquire a checked-subtraction failure. The complete u8
domain, including NUL and non-ASCII bytes, is part of the functional contract.

`sequence.uppercase(items: &mut [u8])` reads each element, calls that ordinary
value function, and explicitly writes the returned value through the existing
`put` operation. Every visited element is assigned, including an unchanged
value. Empty windows have no element access. Only the supplied window is
visited. Existing shared/mutable permissions, indexing, arithmetic and owner
lifetime checks apply through ordinary Types, Analyze and independent Verify.
No readonly/volatile/raw-foreign window permission is inferred by a backend.
The generic tail, tail_mut and put APIs keep their current signatures.

This changes the ordinary source write behavior from conditional writes to
explicit writes through an already requested mutable slice. It does not claim
an arbitrary conditional store may be speculated. The general backend route
has unresolved and demonstrably false variants: a successful load alone does
not prove permission to write, and mutable-address type alone does not prove
an active exclusive loan. Task 109 does not use that route or edit LIR/Verify.

## Independent factors and unchanged reference

N is byte-for-byte restoration of task 108 candidate
`7a219f2d782532324e1ea7401cab68c9eada7c82` foreign declarations and its four
ABI/binding tests. Its Linux-only eager resolution may change IFUNC/startup
observations. No new effect attribute is introduced. S is the ordinary sequence
module above. The original 104 sources and sealed original Rust executables
remain the reference; new 109 source snapshots make the S change explicit.
Only the buffer consumer receives S; stream and retained remain negative
controls with identical sources across the two S settings.

The five arms are BASE, N, S, NS and the original Rust. NS is the only
preassigned performance admission candidate. The twelve cells, 1,920 time
observations, separate 300 target-memory observations, balanced schedule,
unchanged 0.9 per-case/0.95 geometric-mean gates and root-only exclusive timing
window are fixed by the task. Source-shape and functional probes collect no
performance samples. The earlier 108 NoGo is retained without reinterpretation.

## Validation and bounded delivery

Source tests import the actual public module and exercise all 256 byte values,
empty and bounded mutable windows, complete binary I/O, and nearest shared
permission and live-loan rejection controls. Positive sources go through the
full ordinary pipeline to real O0/O2 native programs; source, reports, emitted
artifacts and process output remain under `ZIL_TEST_REPORT_DIR` as `ascii109-*`.
The separate tool author will validate all original output, allocation/failure,
cleanup and memory contracts before any formal measurement. Actual optimized
machine code will be retained to check the intended vector map/store shape.

If NS fails its frozen admission rules, foreign binding and the changed
uppercase implementation are restored. The pure value helper can be retained
only as an independently tested ordinary functional API, without a performance
admission claim. The work does not assert full language completion or all four
Rust product objectives from this bounded experiment.
