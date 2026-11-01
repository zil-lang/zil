# Scalar author and independent controls

This package retains the author proposal receipt, initial and final tests/Clippy,
release/build identity, and the scalar review's original logs, source differential,
rows, machine inventory and output. It retains 51 paths; eight executable outputs
are omitted with their observed hashes. The review's `exact/` checkout is outside
scope and can be reconstructed from its preserved ca79 commit/source identities.

The independent review's Pass is limited to its correctness controls. Release and
probe ELF identities are distinct and remain in their original receipts. Collection
does not relabel earlier tests or assert that correctness counts prove performance.

`VERIFY.json` records complete schema-2 manifest/blob validation. Use
`stage6_archive_large.py --verify <this-directory>` or `--extract
<this-directory> --dest <new-dir>` to restore retained bytes and ordinary permissions.
