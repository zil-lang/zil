# Isolated post-IPSCCP CFG cleanup evidence

Author `post-ipsccp-cfg*` roots plus arithmetic IR/stage/CFG diagnostics, both
stage6-cfg-review and stage6-cfg-toolchain-review are frozen. The seven-line insertion
adds SimplifyCFG after O2 IPSCCP/global cleanup before costly later function work;
independent pipeline comparison retains unchanged O0/O3 and one additional O2 pass.

Original/cfg profile epochs failed with missing-clang E-LINK-IO; original-v2/cfg-v2
are the valid retry, with original failure logs intact. Arithmetic InstCombine
53.5422→0.3395 CPU seconds occurs together with JT 0.0979→1.2684 seconds; it is a
functional diagnostic, not formal net benefit. Same-case executable bytes and five
native seeds agree. Scalar control retains essentially unchanged JT cost.

Independent review has 17 successful ZIL builds, two current type rejections, 68
native invocations and six pipeline requests, separate from component identity review.
Original 8192-arithmetic main/input/five-seed oracle and scalar8192 source are retained.

The 113798082-byte arithmetic-stage-profile/opt.stderr is retained as authoritative
raw output. Its redundant 380782521-byte pass-tables.json is explicitly omitted with
exact SHA fdec576320d0ad53d34caeeb29ed74139cef2b016245a01656a9bcf8ab34bb63;
parser/tools and all other raw stage/IR artifacts remain. This package has no perf.data.

These are frozen isolated functional/diagnostic observations, not an exclusive
performance comparison, final combined toolchain, broad runtime-quality proof or
production-default promotion. Historical source/compiler/library identities remain
unchanged. New combined/runtime/residual roots are excluded.

Per-path bytes/SHA/dispositions are in compressed FILES/OMITTED manifests; generated
source, patches, source licenses/provenance, command argv, raw logs, failures and
controls remain byte-exact. Selected ELF/object/static archives (including small
harness executables) are omitted by byte identity. Their source/commands are kept.
Official SDK installation/header receipts identify reconstructible external inputs;
no SDK archive or compiler target is copied. An omission SHA is not restored bytes.
Collection-time tools have explicit exact-source inventories in the adjacent plan;
collection HEAD is not a measured production/compiler identity.

Retained paths: 594; omitted paths: 197;
unique blobs: 272; archive bytes: 12621028.
Each compressed stream is split at 40 MiB.

```sh
python3 tools/research/incremental-compilation/stage6_archive_large.py --verify evidence/research/incremental-compilation/stage6/llvm-cfg
python3 tools/research/incremental-compilation/stage6_archive_large.py --extract evidence/research/incremental-compilation/stage6/llvm-cfg --dest /tmp/llvm-cfg-restored
```
