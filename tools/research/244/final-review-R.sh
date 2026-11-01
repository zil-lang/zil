#!/usr/bin/env bash
# Independent scoped review only. Never writes the shared core tree.
. /workspace/.zil-env/env.sh
set -euo pipefail
candidate=${1:?exact common candidate required}
report=${2:?absolute tree-external report directory required}
core=/workspace/zil-core
target=/workspace/.zil-env/target-review-sum-Q
cd "$core"
[[ $(git rev-parse HEAD) == "$candidate" ]]
[[ -z $(git status --porcelain) ]]
[[ $report = /* && $report != "$core" && $report != "$core/"* ]]
# This target was first created for exact b51 Q and has only built this cwd.
# R reuses it under the same source root; preserve the first cold-build receipt.
[[ $(cat /tmp/final-owned-sum-Q-review/HEAD-before) == b51dc2043ae51ec42c1bec964bec0623df152431 ]]
[[ $(cat /tmp/final-owned-sum-Q-review/cargo-exit.txt) == 0 ]]
[[ -d "$target" ]]
[[ ! -e "$report" ]]
mkdir -p "$report/raw"
export CARGO_TARGET_DIR="$target" CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0
export ZIL_TEST_REPORT_DIR="$report/raw"
git rev-parse HEAD > "$report/HEAD-before"
git status --porcelain > "$report/status-before"
python3 tools/check-plan.py --print-hashes > "$report/normative-hashes.json"
git ls-tree -r HEAD -- crates Cargo.toml Cargo.lock rust-toolchain.toml data/normative-inputs.json spec/interfaces.json > "$report/dependency-blobs-before.txt"
rustc --version > "$report/rustc.txt"
cargo --version > "$report/cargo.txt"
clang --version > "$report/clang.txt"
targets=(
    impl244_consumer review239_generation review240_helper_sum
    review240_match_exits review241_freshness review241_store_windows
    review242_tag_edges review242_capture_profiles
    impl166_owned_results impl183_owned_resource_results impl206_analyze
    impl206_independent impl223_product_store impl235_product_store
    review223_boundaries review235_contracts
)
printf '%s\n' "${targets[@]}" > "$report/targets.txt"
command=(cargo test -p zil-driver --features llvm-api)
for test in "${targets[@]}"; do command+=(--test "$test"); done
command+=(--locked --no-fail-fast -- --nocapture)
printf '%q ' "${command[@]}" > "$report/COMMAND.txt"
printf '\n' >> "$report/COMMAND.txt"
set +e
"${command[@]}" 2>&1 | tee "$report/cargo.log"
result=${PIPESTATUS[0]}
set -e
printf '%s\n' "$result" > "$report/cargo-exit.txt"
git rev-parse HEAD > "$report/HEAD-after"
git status --porcelain > "$report/status-after"
git ls-tree -r HEAD -- crates Cargo.toml Cargo.lock rust-toolchain.toml data/normative-inputs.json spec/interfaces.json > "$report/dependency-blobs-after.txt"
[[ $(cat "$report/HEAD-after") == "$candidate" ]]
[[ -z $(cat "$report/status-after") ]]
cmp "$report/dependency-blobs-before.txt" "$report/dependency-blobs-after.txt"
python3 - "$report" "$candidate" <<'PY'
import pathlib,json,re,sys,hashlib
root=pathlib.Path(sys.argv[1]); log=(root/'cargo.log').read_text()
rows=[{'status':s,'passed':int(p),'failed':int(f),'ignored':int(i),'filtered':int(q)} for s,p,f,i,q in re.findall(r'test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; \d+ measured; (\d+) filtered out',log)]
consumer={'native_runs':0,'allocator_runs':0,'real_allocation_failures':0}
for path in (root/'raw').glob('consumer244-*/**/expected.json'):
    item=json.loads(path.read_text());consumer['native_runs']+=1
    if 'fail_at' in item:
        consumer['allocator_runs']+=1;consumer['real_allocation_failures']+=item['failures']
additional=sum(1 for path in (root/'raw').glob('*/exit.txt') if path.parent.name.startswith(('review239-generated-','review240-exits-','review240-helper-')))
summary={'candidate':sys.argv[2],'target_count':len(rows),'results':rows,'executed':sum(r['passed']+r['failed'] for r in rows),'passed':sum(r['passed'] for r in rows),'failed':sum(r['failed'] for r in rows),'consumer':consumer,'additional_independent_native_runs':additional,'legacy_native_counts':'Separate raw artifacts; no aggregate claimed by this summary.','log_sha256':hashlib.sha256((root/'cargo.log').read_bytes()).hexdigest()}
(root/'COUNTS.json').write_text(json.dumps(summary,indent=2)+'\n')
assert len(rows)==16,summary
assert summary['executed']==138,summary
PY
exit "$result"
