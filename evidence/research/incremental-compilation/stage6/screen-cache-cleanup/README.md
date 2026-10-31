# Exact archived old-screen cache reclamation

The original 36-directory plan, full 1112-file SHA verification/deletion audit, final
receipt and scripts/logs are retained. Before any deletion, every file matched the
original screens package FILES/OMITTED SHA. Exact canonical screen-default/closed
Rust cache paths were the only allowlist. Source/tests/incremental, reports, probes
and build targets were excluded. Reclaimed logical bytes: 1152760644.

The first script misread schema-1 manifest metadata and exited before any deletion;
its exact script/log are retained. The corrected attempt verified every file before
unlinking only those verified files and then empty directories. Existing evidence
packages were untouched. Cache identity records are not recoverable payloads;
repeating the original cold/edit builds regenerates those compiler caches.

Use stage6_archive_large.py --verify on this directory or --extract with a fresh
--dest to recover all nine retained artifacts. No archive input is omitted.
