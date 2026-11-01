# Frozen default23-shape formal CLI evidence

Six exclusive paired blocks,23 fixed shape families and five requests per arm:1380 total requests,1320 valid and60 retained ZIL budget refusals. Source, complete request reports/stdout/stderr/native outputs, source identities, original runner, command/execution/pair identity and released lease remain in this package. All1320 valid rows are retained; refusal rows are not imputed or removed.

`FILES.jsonl.gz` maps7519 retained paths to1082 unique blobs; `OMITTED.jsonl.gz` records6829 generated artifacts/cache paths by exact bytes and SHA. The138 Rust incremental directories contain5172 files totaling4593946026 logical bytes. Every final cache file has an explicit omission reason. The experiment did not save intermediate cache state after each request, and the analysis does not read internal cache contents. Replaying the frozen source/toolchain/sequence can regenerate caches, but byte-identical regenerated cache bytes are not promised.

Native program/objects/semantic-cache identities remain available even where byte payloads are omitted. The subsequent authorized disk cleanup is limited to the exact Rust incremental files proven against this package; it does not delete program, source, object or semantic-cache paths.

The original v1 analysis is retained under its original identity. Its SourceFileLoader generated a post-measurement `__pycache__/runner.cpython-312.pyc`; this derived file is SHA-inventoried as omitted. It is not a timed request or evidence of raw source/report mutation. A later no-bytecode analysis and side-effect receipt are separate evidence and do not overwrite v1.

Verify with `python3 tools/research/incremental-compilation/stage6_archive_large.py --verify evidence/research/incremental-compilation/stage6/confirmation-default`. Restore retained bytes with `--extract <package> --dest <new-directory>`; omitted cache bytes and executable binaries require the recorded toolchain and source reconstruction.

Historical-identity boundary: the runner retained source SHA sidecars and final restore bytes, not every edited source snapshot. Analysis reproduces edit bytes from the frozen runner and checks those sidecars. It recorded no per-request ELF SHA; collection hashes the final existing artifacts only. DSO evidence uses frozen pair loader probes/identities and object reports, not per-request LD_DEBUG traces.
