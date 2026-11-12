# Incremental demand continuation raw evidence

Production source e91d16480d5e31c620d2b776245973c24329840f. This archive preserves new measured evidence, not a main merge or general compiler acceptance. The old environment raw archive remains unavailable.

Read manifest.json, verify each part SHA256, concatenate lexically sorted raw.tar.zst.part.* to raw.tar.zst, verify archive_sha256, then `tar --zstd -xf raw.tar.zst` in a new directory. It restores `out/runs/`, including check-r1, native first-failure-r1 and completed native-r2, profile, binaries, per-unit object/symbol inventories and validation/review receipts.

Use the source branch matched_validate.py on the recovered completed check/o2 directories. Do not run archived executables merely to validate archive integrity. Run manifests retain original absolute paths as provenance; archived hashes, fixtures and reports can be checked without rebuilding or running binaries. This is a generated synthetic workload and contains no credentials.
