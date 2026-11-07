# Independent 328 source probes

These are the exact inputs recovered from the original local runner, not new
coverage. The review binds backend commit `0c297f7fae81143cc33083cee03e8fb2f6c2bf66`.
Build that commit's CLI with `--features llvm-api`; supply these fixture paths
from this source-recovery commit. The later e622 frontend migration leaves the
328 backend unchanged, but changes the nested-closure diagnostic as noted below.

```sh
source /workspace/.zil-env/env.sh
export CARGO_TARGET_DIR=/tmp/thread328-review-target CARGO_BUILD_JOBS=1
# Run this build in a checkout of the reviewed compiler commit.
cargo build -p zil-cli --features llvm-api --offline
review_zil="$CARGO_TARGET_DIR/debug/zil"
review_fixtures=/absolute/path/to/tests/fixtures/thread328-review
review_out=$(mktemp -d /tmp/thread328-review.XXXXXX)
```

For each of `negative_payload.zil` and `payload_body_order.zil`, run all four
`clang|llvm-api` × `O0|O2` combinations using this command, with a fresh output
directory for each combination:

```sh
mkdir -p "$review_out/case"
(ulimit -c 0; ulimit -t 20; ulimit -v 2097152
 timeout -s KILL 25s "$review_zil" build "$review_fixtures/negative_payload.zil" \
   --backend clang --opt O0 --compile-timeout-ms 10000 \
   --emit-dir "$review_out/case" -o "$review_out/case/program" --json \
   > "$review_out/case/compile.json")
timeout -s KILL 5s "$review_out/case/program"
```

Each build must report Success, verification_runs=1, the requested backend,
fallback=null, and a link command containing `-pthread`. Each executable must
exit 0 with empty stdout/stderr. These are eight native executions, not eight
different input sources.

For the three cache executions, copy `negative_payload.zil` to one temporary
`cache-input.zil` path. Build/run it twice with the same command above, selecting
`--backend llvm-api --opt O2 --object-cache "$review_out/cache"`. Start with a
fresh cache directory. Expect cold misses=1/exit0, then warm hits=1/exit0; both
requests must still have verification_runs=1 and the warm request must record
current machine preflight. Then overwrite that *same input path* with the exact
bytes of `cache_worker_after.zil`, leaving the cache and main body unchanged.
Build/run once more: expect misses=1 and native exit 3 with empty stdout/stderr.
Exit 3 is intentional: unchanged main detects the changed worker result.

`nested_parent_scope.zil` preserves the rejected exploratory input separately.
At original 0c297f7 it reports Types E-UNKNOWN-NAME; with frontend migration
e62240223 it reports Unsupported. It is outside the finite caller-position
profile and is excluded from all eleven native successes above.

SHA256 values and the original observed groups are committed in
`https://github.com/zil-lang/zil/blob/dd766f513bd8022e4d2c703f51a9343824150da8/docs/validation/review328-by324.json`. No raw logs, binaries or new Cargo target
are needed to recover these sources and commands.
