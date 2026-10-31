#!/usr/bin/env python3
"""Read-only CP32/CP33 follow-up of the unchanged independent session audit.

Run only after both formal windows have stopped. Never resume/score a prefix.
The original CP29/CP30 script and its evidence are neither edited nor rewritten.
"""
import argparse
import datetime
import hashlib
import importlib.util
import json
from pathlib import Path
import sys

sys.dont_write_bytecode = True
ROOT = Path('/workspace/scratch/c3607737969d')
AUDIT = ROOT / 'review/compile7_session_independent_recompute.py'
EXPECTED_AUDIT = '4ae659fe9996473ba42673f3820f6bd19c64e53f29b954017d2541151996da01'
BUILD_REVIEW = ROOT / 'review/CP32-CP33-STATIC-BUILD-REVIEW.json'
EXPECTED_BUILD_REVIEW = '32c18657b24267737d4387bd0652aa70be32f0efee82e43c0dcfb442812f9fc1'
EXPECTED_RUNNER = '6ec96a3b6fdf0ade3e3ffaadff8a29c7c46d5b805941175fcf8d78e2d889a7c3'
CANDIDATES = {'cp32': '3ba131efdeffe85ba5fccd1c177745d812a6ce61',
              'cp33': '83854ae83f7fec4fc8f9c22d0751a1fd2725671f'}

def file_sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cp32', type=Path, default=Path('/tmp/zil-compile7-cp32-session'))
    parser.add_argument('--cp33', type=Path, default=Path('/tmp/zil-compile7-cp33-session'))
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    assert args.out.is_absolute() and args.cp32.is_absolute() and args.cp33.is_absolute()
    args.out.mkdir(parents=True, exist_ok=False)
    assert file_sha(AUDIT) == EXPECTED_AUDIT
    assert file_sha(BUILD_REVIEW) == EXPECTED_BUILD_REVIEW
    spec = importlib.util.spec_from_file_location('unchanged_independent_session_audit', AUDIT)
    audit = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(audit)
    assert audit.sha(audit.RUNNER) == EXPECTED_RUNNER
    audit.CANDIDATES.update(CANDIDATES)
    build = audit.read(BUILD_REVIEW)
    assert build['verdict'] == 'Pass'
    for path, record in build['verified_files'].items():
        p = Path(path)
        assert p.stat().st_size == record['size'] and audit.sha(p) == record['sha256'], path
    windows = {}
    for name, commit in CANDIDATES.items():
        root = getattr(args, name)
        plan = audit.read(root / 'plan.json')
        binding = build['candidates'][name]
        assert binding['source_commit'] == commit
        assert audit.sha(binding['receipt_path']) == binding['receipt_sha256']
        assert plan['binaries']['B']['path'] == binding['binary']
        assert plan['binaries']['B']['sha256'] == binding['binary_sha256']
        windows[name] = audit.audit_window(name, root)
    result = {
        'reviewer': 'cp25_evidence', 'verdict': 'Pass',
        'at_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
        'scope': 'Read-only complete-window independent recomputation and current artifact hashes; no compiler/native execution or new timing samples',
        'case_weights': audit.CASES, 'four_request_weights': audit.CORE_REQUESTS,
        'aggregation': 'Two observations per arm/cell/block are averaged; each absolute cost cell takes its median across five paired blocks; fixed absolute case/request weights are summed before B/A division. Capacities never pooled.',
        'bootstrap': 'All 5^5=3125 ordered paired whole-block resamples, 126 multisets with multiplicity. Same blocks for A/B and all cases/requests; recompute the same absolute-cost statistic and linear 2.5/97.5 percentiles. Descriptive/exploratory shared-host interval, not a population guarantee.',
        'seven_request_scope': 'Entire process wall/CPU/maxrss includes initialization, edits, seven fixed-order compile-through-Verify requests and reporting. CPU/RSS cannot be reassigned to the four-request mix. Weighted peak RSS is descriptive per-case median weighting, not simultaneous process-tree peak.',
        'reused_audit_script': str(AUDIT), 'reused_audit_sha256': EXPECTED_AUDIT,
        'runner_sha256': EXPECTED_RUNNER, 'entry_script_sha256': audit.sha(__file__),
        'build_review': str(BUILD_REVIEW), 'build_review_sha256': EXPECTED_BUILD_REVIEW,
        'build_verified_files_currently_rehashed': build['verified_files'],
        'windows': windows,
        'limitations': ['Compile-through-Verify, not LLVM emission/link/native runtime timing',
                       'Report/interface observables versus fresh controls, not whole-TypedIR/native comparison in the measurement window',
                       'Complete windows required; no incomplete prefix pooled or scored',
                       'Separately paired baseline comparisons, not a direct CP32-versus-CP33 or combined-candidate test',
                       'Shared host, five blocks and unflushed OS caches',
                       'No full language/product-gate conclusion']}
    (args.out / 'RESULT.json').write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
    compact = {name: {cap: {
        'four_request_mix_B_over_A': data['four_request_mix_compile']['B_over_A'],
        'four_request_mix_exploratory_CI95': data['four_request_mix_compile']['exploratory_paired_block_bootstrap95'],
        'seven_request_process_wall_B_over_A': data['case_weighted_seven_request_sequence']['process_wall']['B_over_A'],
        'seven_request_process_cpu_B_over_A': data['case_weighted_seven_request_sequence']['process_cpu']['B_over_A'],
        'case_weighted_process_peak_rss_B_over_A': data['case_weighted_seven_request_sequence']['process_maxrss']['B_over_A']}
        for cap, data in window['capacities'].items()} for name, window in windows.items()}
    print(json.dumps({'verdict': 'Pass', 'windows': compact}, ensure_ascii=False, indent=2))

if __name__ == '__main__':
    main()
