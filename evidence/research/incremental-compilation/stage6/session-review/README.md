# Independent Session review

All original review files, the complete large inventory, recomputation script,
initial failed audit log and corrected audit results are preserved as raw blobs.
The approximately 101 MB inventory is inside the compressed archive, not a large
outer Git file. The review is limited to original observations and statistics;
its text explicitly predates packaging. It does not itself certify this archive.

`VERIFY.json` separately verifies this package's manifests, segments and raw blobs.
Use `stage6_archive_large.py --verify <this-directory>` or `--extract
<this-directory> --dest <new-dir>` to inspect the preserved evidence. No compiler
or performance rerun was performed during collection.
