# 0109: Explicit ASCII value mapping in the ordinary library

The buffer consumer's old source conditionally wrote lowercase bytes. Its
actual optimized code expanded a vector predicate into per-lane branches and
scalar stores. The new ordinary public `sequence.ascii_upper(u8) -> u8` returns
the mapped value, and `uppercase` explicitly writes that value through the
existing mutable-window `put` operation. Existing generic slice algorithms
remain available. This exposes a vector map/store shape without inventing
false-path write permission in LIR or bypassing independent Verify.

The five real source test groups cover all 256 byte values, empty/interior
windows and untouched sentinels, binary multi-module I/O at vector boundaries,
shared argument/live-loan rejection, and initialized-window bounds traps.
The passing attempt contains eight O0/O2 native builds and 36 executions,
including two expected traps. Each accepted build records one Verify run.
Artifacts use the stable `ascii109-` prefix under `ZIL_TEST_REPORT_DIR`.
The initial diagnostic-expectation failure is preserved with its actual
compiler/test executable and source snapshot; no production fix was needed.

Task 108's exact historical foreign declaration and four-test files are
restored as N, without additional attributes. The S source factor is frozen
separately from the 22 unchanged original ZIL/Rust sources. The mapped buffer
sequence file is byte-identical to the actual public module. Its actual O2
binary has the intended two-vector mask/map/store loop; ordinary runtime
checks, error/ownership paths and loader-timing qualifications remain.

Root independently rebuilt all five new source groups and four restored binding
groups. The fresh five-arm preparation passes 450 ordinary/observer checks,
including 80 controlled allocator failures, plus twelve target-memory controls.
The fixed formal campaign completes 1,920 time observations and 300 separate
target-memory observations. Root's independent arithmetic and graph's complete
original-record audit both pass. All three predeclared numerical groups pass;
runtime geometric-mean efficiency is 1.0695972769484003.

Root selects the exact NS implementation for limited production admission,
conditional on independent archive verification, integration and the unchanged
35 required gates. The three 4 MiB programs remain slower than Rust; retained's
0.907300115 efficiency is near the 0.9 line. The same-byte retained N and NS
programs differ by about 17.9% in their observed arm medians, showing substantial
non-algorithmic variation. Three NS/BASE median points and four p95 points are
adverse and remain fully reported. Linux IFUNC/startup timing differences remain
part of the admitted binding policy. See `docs/research/109/RESULTS.md` for every
arm/cell, factor relation, interaction, RSS result and qualification.

No performance observations are repeated or pooled with 104/108. The separate
108 NoGo and rollback remain unchanged. This bounded selection does not assert
stable or universal Rust superiority, whole-language completion, all four
product objectives, or a new general graph/conditional-store compiler theorem.
