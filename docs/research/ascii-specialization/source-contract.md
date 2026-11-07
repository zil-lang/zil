# 109 ordinary source contract and actual machine shape

The public module is `examples/mainline/byte_buffer/sequence.zil`. Its mapped
benchmark copy is byte-identical, including the existing generic `put`, `tail`
and `tail_mut` functions. This is a reusable ordinary ZIL module. No special
compiler name, intrinsic, trusted effect, generated capability or exempted
verification path implements the new value helper.

## Value and window obligations

For any u8 value b, `ascii_upper(b)` returns b-32 when 97 <= b <= 122 and b
otherwise. On the true branch the checked subtraction produces 65 through 90;
on the false branch no subtraction is evaluated. The native all-value control
compares the full 256-byte output with Rust's ordinary ASCII value operation,
at both O0 and O2. It includes every low value that would underflow an
unconditionally evaluated subtraction.

`uppercase` receives an ordinary mutable slice, loads one byte, obtains the
mapped value and assigns it through `put` at the same index. Assignment is
explicit in the source on every visited iteration. The compiler must still
establish all ordinary call, index, borrow and lifetime obligations. Empty
windows perform no element operation; the interior-window control surrounds
all 256 byte values with lowercase sentinels that must remain unchanged.
The invalid initialized-length control reserves capacity four but exposes
only one initialized byte, requests a two-byte window and must trap. Capacity
does not grant extra initialized elements.

The nearest permission controls reject a shared slice argument in Expand with
`E-GENERIC-ARGUMENT`, and reject a mutable window while an overlapping shared
loan remains live in independent Verify with `E-BORROW-CONFLICT`. An early
source rejection is not counted as an independent Verify success. Every
accepted native build records exactly one verification run.

## Why this does not introduce speculative stores

A general LLVM transformation may not introduce an unconditional store solely
because a load succeeded. A read-only page, an active shared loan, volatile
storage, another observer or an expired lifetime can invalidate that argument.
Even a mutable address type does not alone express the current active loan.
Task 109 leaves the backend and verified-LIR binding rule unchanged apart from
the separately restored task 108 foreign-declaration factor. The new ordinary
source explicitly requests its writes, so existing validation checks them.

This source change has an observable write contract. It is not advertised as
an automatic optimization of all conditionally written places. It applies to
the ordinary explicitly mutable byte windows accepted by the language.

## Frozen original and mapped consumer sources

`benches/rust-parity/109/provenance.json` records all 22 byte-exact original
files from task 104 commit `38ec6bc2d267b44bb7edab8a0d894af9cdb23954` and the
four-file mapped buffer consumer. Only its sequence module changes. Main,
buffer growth/ownership, I/O and every original Rust file remain byte-exact.
BASE and N consume `original`; S and NS use `mapped` only for buffer. Stream
and retained have the same original source in every corresponding arm.

N restores only the two exact files from task 108 production candidate
`7a219f2d782532324e1ea7401cab68c9eada7c82`: foreign declarations and their four
ABI/binding tests. That Linux eager-resolution policy retains the documented
IFUNC/startup timing difference. The 108 NoGo and complete rollback remain a
separate research delivery. NS is the only preassigned new admission object.

## Actual functional and machine evidence before timing

Five source test groups pass in author attempt `tests002`: eight complete
O0/O2 native builds, 34 successful native executions, two expected native
SIGILL bounds traps, and two source permission rejections. The first attempt
is retained: all four native groups passed, while the shared-argument test
incorrectly expected a Types diagnostic instead of the actual earlier Expand
diagnostic. Only that expectation was corrected. Formatting and the targeted
Clippy gate pass. No performance samples were collected.

The actual O2 mapped consumer has binary SHA256
`58067dc7411dbdf589ea6462847b94f57d7b330acff8736810cc884c29f71f91`.
Its inlined loop at 0x1270 through 0x12c5 loads two 16-byte vectors, computes
the byte-range mask and mapped values, stores both vectors, and advances by
32 bytes. The original per-lane conditional-store expansion is absent from
that loop. The emitted checked input-conversion and its failure path remain.
Full machine output and compile/source/Verify artifacts are retained under
`/tmp/zil-109-author001`; this establishes code shape and finite behavior,
not a runtime ratio. The separate five-arm controller must still freeze and
validate all actual original/mapped binaries and all failure/allocation cases
before root authorizes the sole formal sampling window.
