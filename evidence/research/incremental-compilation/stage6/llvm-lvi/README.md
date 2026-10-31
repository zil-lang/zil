# Isolated LLVM LVI value-index evidence

All `stage6/lvi-value-index*` author roots and `stage6-lvi-review` are frozen.
The conservative value→hashed block-slot relation replaces full-cache erase scans;
three registration sites and callback lifetime ordering are controlled. Historical
stale memberships cost memory until value erase or clear, so space is not bounded
by current live cache contents. No solver or public-header change is inferred.

Retained author epochs include cache and cache-v2: the latter closes the live
predecessor-fact invalidation coverage gap without erasing the first epoch. Profiles
original/original-v2 failed before compiler launch (missing time tool, then unsuitable
inherited child-max accounting); original-v3/indexed are the two successful exact-child
wait4 diagnostics. Their CVP 18.8509→0.5466 CPU seconds and equal executable bytes are
one-input diagnostic evidence, not formal speedup. JT remains expensive.

Independent review preserves 48000 random cache mutations/invariant checks, 52
boundary checkpoints, four expected epoch rejections, true C API and real CLI/cache
controls. Raw generated full-u32 oracle bytes, original 2048-branch main.zil/input.bin
and retained five-seed source oracle are included. This package has no perf.data.

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

Retained paths: 590; omitted paths: 196;
unique blobs: 222; archive bytes: 1283889.
Each compressed stream is split at 40 MiB.

```sh
python3 tools/research/incremental-compilation/stage6_archive_large.py --verify evidence/research/incremental-compilation/stage6/llvm-lvi
python3 tools/research/incremental-compilation/stage6_archive_large.py --extract evidence/research/incremental-compilation/stage6/llvm-lvi --dest /tmp/llvm-lvi-restored
```
