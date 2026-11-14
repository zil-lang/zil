# Full production-chain exact evidence recovery

Fifteen parts preserve 28,238 regular files / 3,150,124,982 bytes. Compressed size is 372,988,770 bytes; joined SHA256 is `b2080de501386b23ff124004dc22ee58584493a5b239b13657fdf91b0802bb0d`.

With Python3 and zstandard, from this directory:

```sh
python3 archive_evidence.py verify --archive-dir . --restore-dir /absolute/new/destination
```

The unchanged fixed V23 verifier has SHA256 `4a65fe492d5d5454e62d3965581a53e02e7ab6ca61214181919be996d256a18a`. It verifies compressed parts/join, exact restored membership and every file's bytes/size/mode without executing any payload. The destination must not exist.

Provenance stays separate: compiler/release b6e019894986b43a1bf1a81cf549a0850d66b9b4; measured tools 37fb410dd00e1f92b6ec660f313f8cf57d84fd71; strict postprocess c59a3878654efe589e5d9da723d0a17d2411b29c. Embedded source tarballs omit only the prior historical raw-evidence directory to avoid duplicating225MB, with the boundary and full Git tree listings recorded. The complete Git delta bundle preserves the actual source/evidence commits.

Full six-block check/O2 raw windows, parents, outputs, binaries/runtime blobs, first failures, corrected tests,65metadata positive/negative cases and every Astra review are retained. Original b6 Expand/driver failures remain failures; test-only retries and reused receipts have explicit identities. SDK18/native-link fixed V23/V27 dependencies remain in the preceding021997 checkpoint/recovery; LLVM was not rebuilt. This archive is not a full LLVM toolchain installation.

This window's18 median/pairedGM cells beat Rust, but individual slow samples and check-broad/main negative remain. No universal stable Rust lead, full global propagation, main merge or default research activation is claimed. Local archive custody is verified by the separate fresh restore receipt; GitHub write authentication blocked remote publication, so local custody is not a received remote backup.
