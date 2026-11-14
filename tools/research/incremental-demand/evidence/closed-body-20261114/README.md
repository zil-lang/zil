# Exact ClosedBody evidence recovery

This local nine-part archive preserves 10,375 regular files (1,346,293,214 bytes), including complete raw check/O2 windows, failures, compiler/runtime blobs, immutable source snapshots, gate logs, SDK recovery receipts and the separate test-only followup. Its compressed SHA256 is `2e20a6e3965f56c0b423034149346df0f585b92d9d405236d1dd5070c71880bc`.

With Python3 and zstandard, run from this directory:

```sh
python3 archive_evidence.py verify --archive-dir . --restore-dir /absolute/new/empty-destination
```

The verifier is the unchanged fixed V23 verifier, SHA256 `4a65fe492d5d5454e62d3965581a53e02e7ab6ca61214181919be996d256a18a`. It checks ordered compressed parts, joined archive, every regular-file size/hash/mode and exact restored membership. It executes no payload. A new destination is required.

Compiler/performance source is 021997; e5e14 is a separately tested fixture-only followup. SDK18/native-link are recovered via the prior fixed V23/V27 dependencies recorded in the checkpoint; this archive does not pretend to contain a complete LLVM installation. The first restored native-link contract failed because empty directories were absent; the recorded fixed lexical structure was replayed and the unique second run passed. Both receipts are retained.

Local custody has been verified; GitHub write authentication blocked publication. Do not treat this local archive or checkpoint as a received remote backup. No historical98fa performance number is relabeled. Performance parent remains open.
