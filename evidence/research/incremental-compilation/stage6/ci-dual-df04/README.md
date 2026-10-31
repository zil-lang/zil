# Two completed CI runs on exact df04

Both downloaded GitHub records bind `df04d4920b9d3ec618e660f8149063eb6019b317` and completed/success: research LLVM verification41212317882 and official main verification41212324365. Original run/jobs/artifact-list API JSON and complete raw log ZIPs are retained. ZIP CRC checks passed, each member is SHA-inventoried, and the audit points to original workspace/reference tests, Clippy, text-reference and research-toolchain build/identity Pass log lines.

The research run rebuilt the pinned candidate and performed its real workflow gates; this is separate from earlier fake-fixture protocol tests. These successes apply only to df04, not later quality-closure/current HEAD. The archive includes74 immutable df04 source files covering both workflows, exact producers, vendored C++/licenses, reconstruction helpers and pinned LLVM installer recipes. Full compiler source is restored by the recorded Git revision. Artifact-list metadata is included; downloading the separate artifact payloads is not claimed.

`stage6_archive_large.py --verify <package>` validates all blob/manifest hashes. Old2e8 CI packages remain unchanged.
