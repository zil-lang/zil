# Frozen default-window cache cleanup and analysis correction

The original raw archive was fully verified before any cleanup. Only138 exact Rust incremental directories were authorized;5172 paths totaling4593946026 logical bytes match its frozen omitted manifest. Source/program/object/semantic-cache and all raw request files were excluded from deletion.

The first epoch verified every file, then safely stopped after22 deletions because unlinking a hardlink changed another alias's ctime. Its script,FailedOrPartial receipt,complete audit and diagnosis remain unchanged. V2 binds those22 exact deleted paths and the original archive, fully rehashes5150 remaining files, and allows an own-unlink ctime change only for the same previously deleted inode with unchanged dev/inode/mode/size/mtime and a fresh immediate full SHA check. It completed all138 roots;9160 outside files had identical before/after stat sets. Actual free disk after cleanup was14726086656 bytes; summing logical/hardlinked path sizes is not a claim of exact unique disk blocks reclaimed.

This package also retains frozen analysisv2 and the separate SourceFileLoader side-effect correction. All v1 cells/groups/source inventory/compiler audit/statistics match v2; v1 and the post-measurement pyc were not deleted or overwritten. Correction evidence preserves old producer/bytecode bytes and before/after raw identities.

Verify using `stage6_archive_large.py --verify <this-directory>`; restore retained evidence using `--extract <this-directory> --dest <new-directory>`. Cleanup scripts are intentionally one-shot, refuse existing receipts and refer to already-deleted cache paths; they are records of the operation, not scripts to rerun against current cache state.
