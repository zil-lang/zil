# Ordinary owned Sum consumer

This is a five-module synchronous source consumer, not a ZUI framework or a
retained callback/OS registration implementation. Its source and expected
operations are frozen before the first consumer compilation. Compiler output
is never an input to the oracle.

`frozen-probes/` preserves six Source probes byte for byte with their semantic and native expectations. In particular,
The unit-store-session case remains a Success obligation with `SRA`/exit0 and `SA`/exit1; The owned-option-session case remains a
Success obligation with `SRA`/exit0 and `SR`/exit0. Their Q Unsupported reports
remain historical failures, not expected rejections.

The project owns real subscription and node Stores with Mark/ByteBuffer
elements. Model constructors preserve private fields across module APIs.
`Fault` owns an actual partial Store. `Phase` contains a Session, an existing
error union of new complete Products, and a right-nested managed enum pattern.
`App` is a child-only Product with an owning Sum field and private Copy data.

Input is two bytes: count `0..3`, mode `0..7`. EOF, one-byte input, or an
out-of-range byte returns 9 before resources are created. Modes are:

| Mode | Source behavior | Count 2 normal output | Exit |
| --- | --- | --- | --- |
| 0 | Empty owning variant | `SENLQ` | 0 |
| 1 | Consume Ready by match; arm owns complete Session | `SRabABNLQ` | 0 |
| 2 | Enum owns error union; propagate its owning error | `SLQabE` | 1 |
| 3 | Independent App RHS fails; keep old App until normal error cleanup | `SLQabABmnE` | 1 |
| 4 | Opaque ordinary static self-call; then consume by match | `SRabABNLQ` | 0 |
| 5 | Right-nested pattern; full tests precede Unpack; arm cleanup a,c,d | `SCacdNLQ` | 0 |
| 6 | Return with unused App; recursive actual-tag cleanup | `SNLQabAB` | 0 |
| 7 | Mutable Enum replacement; old cleanup then preserved-shell exit order | `SabABNLQmnMN` | 0 |

Mode 5 count 0 selects the nested Empty branch (`SEaNLQ`); count 1 selects the
specific Copy-pattern arm (`SBacdNLQ`); counts 2/3 must fail that Copy test
without consuming the original value, then select the following arm.

`L` is a real resource created after `defer Q`. a/b/c and A/B/C are actual
subscription/node resources, and m/n/o, M/N/O distinguish replacement owners.
The same Mark type at different payload positions has separate responsibility.

`tools/research/owner-join-review/oracle.py` models allocation requests and source transfers
only. `ORACLE.json` freezes 32 valid inputs, five invalid inputs, every actual
allocation failure point, zero and one-past-last controls at O0/O2. This gives
190 allocation probes per optimization. It asserts request/free/live-block
counts; actual payload peak bytes are observed, not guessed as RSS or a Rust
comparison. An oracle prediction alone claims no native execution.

Safety evidence distinguishes Source, Analyze, independent Verify raw/Wire, and machine behavior; full language safety, Rust runtime/RSS/maintenance or
compile-cost superiority, and a product-level four-goal breakthrough remain
unproved. Existing capture/shared/StoreElement/array boundaries stay closed.
