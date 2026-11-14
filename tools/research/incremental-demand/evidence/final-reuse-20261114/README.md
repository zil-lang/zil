# Final-product reuse complete-chain evidence

Candidate and measured tools: `8ce584f81a405b8c5aa69e555b80ede276f07143`. Binary SHA256: `584e5ec59ec89fa83e69f4afb9a0f2185523ea8d0ebc1b671204168ad03ea810`. Original425 check/O2, each two blocks of actual main/candidate/Rust, CGU16/O2/jobs1. Strict per-block validation and every native artifact blob hash passed: 1080 native outcomes, 30 complete ELF/export equality pairs, 178 unique blobs rehashed. All18 median and paired-GM cells shorter than Rust in this finite window; O2 recovery block1 is slower than Rust and check/broad block1 is slower than main; both are retained. No universal/stable superiority or cross-window speed attribution.

Nine parts preserve 10,359 regular files / 1,356,679,068 logical bytes. Compressed size: 217,597,018 bytes; joined SHA256 `3e00d02563d2ffa34dc20defd8f9391c21d8bb31bd8935f1595cb12002f6c725`. All selected file bytes, size, mode and membership were freshly restored and verified; receipt: `fresh-restore-receipt.json`.

```sh
python3 archive_evidence.py verify --archive-dir . --restore-dir /absolute/new/destination
```

The unchanged fixed V23 verifier SHA256 is `4a65fe492d5d5454e62d3965581a53e02e7ab6ca61214181919be996d256a18a`. Verification does not execute payloads. See `RESTORE.json` for exact source/binary identities and separately pinned V32 SDK18/native-link and prior baseline dependencies. No LLVM rebuild; this archive is not an entire LLVM/toolchain installation. Original absolute runtime paths need explicit reconstruction or rebinding to reproduce commands.

Current backend273/driver211/Source9/expanded retained1/strict Clippy/fmt/release passed; native86/Expand116 producer receipts are explicitly inherited from unchanged sources, not called reruns. Original compiler/review patch and launches are included. The final evidence review and archive verification metadata are adjacent files outside the compressed payload so the sealed archive remains unchanged. b6 six-block and older source windows retain their original identities. Research flags remain off by default; no main/goal/archive merge.
