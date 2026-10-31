# ffe4 syntax correctness and build identity

This package retains all frozen author records, the independent review's four root
receipts, root build identity, and the baseline SOURCE/release receipts. Initial
author test failures remain alongside their budget-fixture corrections. The
independent review binds ffe4b814e22d8db79985fdd6fe397fbf77e6a243: 66 limited tests,
including 12 new real API/native builds, passed. This is a correctness conclusion,
not a performance result.

The author receipt's earlier ca79 checkout label is distinct from its per-file
hashes, which the review matched to ffe4. `root-identity.json` binds the actual ELF
and all 638 build inputs. Its Clippy/fmt statements are explicit root attestations,
not fabricated original logs; the initial misspelled-package failure is preserved.

The review's exact checkout/shared target and baseline source checkout are outside
scope; commit/source receipts identify the reconstructible versions. Formal syntax
Session windows are separate and are not counted as completed here.

`VERIFY.json` validates all schema-2 manifests and raw blobs. Use
`stage6_archive_large.py --verify <this-directory>` or `--extract
<this-directory> --dest <new-directory>` to inspect retained bytes and permissions.
