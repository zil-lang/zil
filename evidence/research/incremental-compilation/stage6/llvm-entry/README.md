# Stage 6 LLVM entry-preserving v1 evidence

This frozen package covers the independent v1 JumpThreading candidate, its original
control, actual native LLVM DSOs, exact ac4 API compiler pair, and two independent
correctness reviews. It is not the later SSA/LVI/CFG combination, a formal timing
matrix, a runtime-quality gate, or a default production selection.

The 22 delivered plugin/native tools match commit `09f1bb90e` byte for byte; the full
commit and each source SHA are in the retained `llvm-entry-plan.json`. Historical
review records that say final Git binding was pending remain unchanged. Original
reviewed content hashes and initial/revised source copies retain their own identity.

The plugin capture snapshot belongs to ca79. The actual API pair belongs to
`ac4ebb9e5fe668b160cbc4b1fe30db735858fdce`, with only private cache/pipeline identity
strings changed. It is not relabeled as later 888 or a combined compiler. Source
manifests, patches, commands/build logs, launchers, actual-PID loader logs and exact
DSO/compiler SHA identities remain. The actual LLVMRunPasses pipeline is unchanged.

Native libraries are paired rebuilt-original / entry:

- Original SHA256: `e187ef33fadaa29e004a1e84be687b70677f540a17775c2945fe88d1527840cc`.
- Entry SHA256: `4c61a96c4e5110a9c09449f32bc16535dafa3f433070b35bda3232cd2f7bd56c`.

Both rebuild one translation unit with identical flags and preserve the public
class/header/layout. Their receipts bind 61 component inputs, 81 unchanged other
ScalarOpts members, dependency identities, build IDs, source and complete argv.
The official installed prefix remains unchanged. Native C API correctness comes
from `stage6-native-jt-review`, not from running the statically linked opt plugin.

Key raw roots after extraction:

- `research/stage6/jump-threading-*`: generated clone/native sources and patches,
  plugin ABI/pipeline/real/opaque/resource controls, all early failures and author
  receipts, SDK identity/logs, native DSO and ac4 API pair receipts.
- `research/stage6-jt-review/`: initial protocol Fail and repaired publication/cache
  wrapper, generated LLVM and real ZIL controls, all fixture failures and raw output.
- `research/stage6-native-jt-review/`: exact source/header/ABI checks, 45 true C API
  requests, 12 successful real ZIL builds plus 1 current type rejection, 105 total
  native invocations, current Verify/cache/loader/address controls, and three initial
  reviewer cache-schema assumptions. This remains limited functional validation.

All 42 cache ZIPs are kept byte-exact, including forged/CRC/truncated/duplicate-member
negative controls and a complete actual three-CGU resource-source-v3 cache set.
Two input.bin and two full expected.bin oracle files are kept. Four original perf.data
files are kept, including three empty initial permission failures and the successful
4731204-byte trace. Those four attempts are the old official LLVM baseline profile,
not entry measurements, and duplicate archived diagnostics are not new evidence.
All recorded elapsed observations here are functional/diagnostic, not a new exclusive
performance result. Existing negative opaque shapes remain.

Exclusions are explicit in `llvm-entry-plan.json`:

- The 1190060068-byte official SDK archive and 2355 extracted headers are not copied.
  Exact archive URL/bytes/SHA, extraction commands and every header SHA are retained
  in headers-receipt.json; the initial failed extraction receipt is also retained.
- API/capture checkout copies are reconstructible from their respective exact Git
  revision plus retained source manifests and RESEARCH.patch. No target is copied.
- Selected ELF/object/static-library/symlink products have per-path omission records;
  external compiler targets/SDK inputs have full identity in their original receipts
  and the explicit plan. Identity records alone do not restore omitted bytes.
- Official install-receipt plus release/install scripts are retained. Generated
  candidate C++/headers/link scripts/IR/commands and raw controls are retained.
- Active SSA/LVI/CFG/combined/final-runtime roots and later ca79 opaque perf/early
  diagnostics are excluded. No root from those candidates is silently included.

Schema 2 deduplicates exact blobs and splits each compressed stream at 40 MiB.
This package contains 2922 retained paths, 713 omitted paths and 1322 unique blobs;
raw.tar.gz is 6511399 bytes. Existing packages are unchanged.

```sh
python3 tools/research/incremental-compilation/stage6_archive_large.py --verify evidence/research/incremental-compilation/stage6/llvm-entry
python3 tools/research/incremental-compilation/stage6_archive_large.py --extract evidence/research/incremental-compilation/stage6/llvm-entry --dest /tmp/stage6-llvm-entry-restored
```
