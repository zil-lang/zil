#!/usr/bin/env bash
set -u
[[ "$PWD" == /workspace/zil-alpha-candidate ]] || { printf "Wrong immutable candidate directory\n" >&2; exit 2; }
[[ -z "$(git status --porcelain)" ]] || { printf "Dirty candidate source\n" >&2; exit 2; }
alpha_run_name="$1"
[[ "$alpha_run_name" =~ ^[a-z0-9-]+$ ]] || exit 2
source /workspace/.zil-env/language-env.sh
alpha_run_dir="out/runs/$alpha_run_name"
mkdir -p "$alpha_run_dir"
git rev-parse HEAD > "$alpha_run_dir/source.txt"
# Reject mechanical integration errors before code generation or source probes.
if cargo check --locked --keep-going --workspace --all-targets > "$alpha_run_dir/rust-precheck.log" 2>&1; then
    printf 'RUST_PRECHECK_EXIT=0\n' > "$alpha_run_dir/exit.txt"
else
    alpha_check_exit=$?
    printf 'RUST_PRECHECK_EXIT=%s\n' "$alpha_check_exit" > "$alpha_run_dir/exit.txt"
    exit "$alpha_check_exit"
fi
# Prioritize actual language feedback over compiling every regression target.
cargo build --locked -p zil-cli > "$alpha_run_dir/build.log" 2>&1
alpha_build_exit=$?
printf 'CLI_BUILD_EXIT=%s\n' "$alpha_build_exit" >> "$alpha_run_dir/exit.txt"
if [[ "$alpha_build_exit" -ne 0 ]]; then
    # A broken compiler cannot produce new language/runtime evidence.
    # Do not recompile the same errors through all-targets and probes.
    exit "$alpha_build_exit"
fi
python3 - "$PWD" "$alpha_run_dir" <<'PY'
import datetime, hashlib, json, pathlib, subprocess, sys
repo, output = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
cli = pathlib.Path('/workspace/.zil-env/target-language-alpha/debug/zil')
state = subprocess.check_output(['git', '-C', str(repo), 'status', '--porcelain'], text=True)
receipt = {
    'status': 'Pass' if not state else 'DirtySource', 'exit_code': 0,
    'git_status': state,
    'source_commit': subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip(),
    'cli_sha256': hashlib.sha256(cli.read_bytes()).hexdigest(),
    'command': ['cargo', 'build', '--locked', '-p', 'zil-cli'],
    'build_log': str((output / 'build.log').resolve()),
    'completed_at_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
}
(output / 'compiler-build-receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
PY
if [[ "${ALPHA_BUILD_ONLY:-0}" -eq 1 ]]; then
    exit 0
fi
if [[ "$alpha_build_exit" -eq 0 ]]; then
    ZIL_ANALYZE_PROFILE_COSTS=1 python3 tools/language/check-foundation-profile.py \
        --repo "$PWD" \
        --cli /workspace/.zil-env/target-language-alpha/debug/zil \
        --out "$PWD/$alpha_run_dir/core-coverage" \
        > "$alpha_run_dir/coverage.log" 2>&1 &
    alpha_profile_pid=$!
    python3 /workspace/.zil-env/check-alpha-core-chain.py --repo "$PWD" --cli /workspace/.zil-env/target-language-alpha/debug/zil --out "$PWD/$alpha_run_dir/chains" > "$alpha_run_dir/chains.log" 2>&1 &
    alpha_core_pid=$!
    if [[ -n "${ALPHA_ORIGINAL_APP:-}" ]]; then
        /workspace/.zil-env/target-language-alpha/debug/zil check "$ALPHA_ORIGINAL_APP" --emit-dir "$PWD/$alpha_run_dir/original-app/phases" --json > "$alpha_run_dir/original-app.json" 2> "$alpha_run_dir/original-app.stderr" &
        alpha_app_pid=$!
    fi
    if [[ -n "${ALPHA_EXTRA_SOURCE:-}" ]]; then
        /workspace/.zil-env/target-language-alpha/debug/zil check "$ALPHA_EXTRA_SOURCE" --emit-dir "$PWD/$alpha_run_dir/extra-source/phases" --json > "$alpha_run_dir/extra-source.json" 2> "$alpha_run_dir/extra-source.stderr" &
        alpha_extra_pid=$!
    fi
    wait "$alpha_profile_pid"
    printf 'FOUNDATION_PROFILE_EXIT=%s\n' "$?" >> "$alpha_run_dir/exit.txt"
    wait "$alpha_core_pid"
    printf 'CORE_CHAIN_EXIT=%s\n' "$?" >> "$alpha_run_dir/exit.txt"
    if [[ -n "${alpha_app_pid:-}" ]]; then
        wait "$alpha_app_pid"
        printf 'ORIGINAL_APPLICATION_EXIT=%s\n' "$?" >> "$alpha_run_dir/exit.txt"
    fi
    if [[ -n "${alpha_extra_pid:-}" ]]; then
        wait "$alpha_extra_pid"
        printf 'EXTRA_ORIGINAL_SOURCE_EXIT=%s\n' "$?" >> "$alpha_run_dir/exit.txt"
    fi
fi
if [[ "${ALPHA_FEEDBACK_ONLY:-0}" -eq 1 ]]; then
    printf 'REGRESSIONS_NOT_RUN=1\n' >> "$alpha_run_dir/exit.txt"
    exit 0
fi
cargo check --locked --keep-going --workspace --all-targets > "$alpha_run_dir/compile.log" 2>&1
printf 'WORKSPACE_ALLTARGETS_EXIT=%s\n' "$?" >> "$alpha_run_dir/exit.txt"
if [[ -d out/runs/semantic-types-source-probe ]]; then
    mv out/runs/semantic-types-source-probe "$alpha_run_dir/previous-probe-standard"
fi
cargo test --locked -p zil-types --lib semantics::source_probe:: -- --nocapture > "$alpha_run_dir/probe.log" 2>&1
printf 'TYPES_PROBE_EXIT=%s\n' "$?" >> "$alpha_run_dir/exit.txt"
if [[ -d out/runs/semantic-types-source-probe ]]; then
    cp -a out/runs/semantic-types-source-probe "$alpha_run_dir/semantic-types-source-probe"
fi
