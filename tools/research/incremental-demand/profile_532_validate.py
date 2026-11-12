#!/usr/bin/env python3
"""Read-only validation and summary of the completed single-block source-532 profile."""
import argparse
import collections
import hashlib
import json
import os
from pathlib import Path
import subprocess

SOURCE = '532e01141b644d13c6e350a7dd310c50226d19d3'
MAIN = '99035931d358add0eaed6d4535035c38413701dd'
STABLE_SCRIPT = 'tools/research/incremental-demand/profile_532_validate.py'

def sha(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()

def receipt(path, repo):
    return {'path': str(path.relative_to(repo)), 'bytes': path.stat().st_size, 'sha256': sha(path)}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, required=True, help='repository root containing the preserved raw evidence')
    parser.add_argument('--main-binary', type=Path, help='optional main compiler file to recheck against its recorded hash')
    parser.add_argument('--candidate-binary', type=Path, help='optional candidate compiler file to recheck against its recorded hash')
    parser.add_argument('--output-stem', type=Path, default=Path('/tmp/full-pipeline-profile-532'))
    args = parser.parse_args()
    repo = args.repo.resolve()
    raw = repo / 'out/runs/full-pipeline-profile-o2-r5'
    row_path = raw / 'rows.jsonl'
    before = sha(row_path)
    rows = [json.loads(line) for line in row_path.read_text().splitlines()]
    manifest = json.loads((raw / 'manifest.json').read_text())
    summary = json.loads((raw / 'summary.json').read_text())
    build_path = repo / 'out/runs/full-pipeline-proof-build/candidate-build.json'
    build = json.loads(build_path.read_text())
    log_path = repo / 'out/runs/full-pipeline-profile-o2-r5.log'
    logs = [json.loads(line) for line in log_path.read_text().splitlines()]
    exit_path = repo / 'out/runs/full-pipeline-profile-o2-r5.exit'
    assert len(rows) == 34 and sum(row['measured'] for row in rows) == 18
    assert manifest['source_shas'] == {'baseline': MAIN, 'candidate': SOURCE}
    assert build['source'] == SOURCE and build['exit_code'] == 0
    assert sha(raw / 'native_measure.py') == manifest['runner_sha256']
    assert sha(raw / 'native_workloads.py') == manifest['generator_sha256']
    trees = {name: subprocess.check_output(['git', 'show', '--no-patch', '--format=%T', source], cwd=repo, text=True).strip()
             for name, source in [('main', MAIN), ('candidate', SOURCE)]}
    assert build['tree'] == trees['candidate']
    # Raw receipts retain the original absolute paths. Summary paths are relative
    # to the named source checkout; replay never depends on /workspace names.
    binary_receipts = {
        'baseline': {'path': 'target/release/zil', 'path_base': 'main_source_checkout', 'sha256': manifest['binaries']['baseline']['sha256']},
        'candidate': {'path': 'out/runs/full-pipeline-proof-build/candidate-zil', 'path_base': 'candidate_source_checkout', 'sha256': manifest['binaries']['candidate']['sha256']},
    }
    binary_checks = {}
    for arm, supplied in [('baseline', args.main_binary), ('candidate', args.candidate_binary)]:
        location = supplied
        if location is None and arm == 'candidate':
            location = repo / binary_receipts[arm]['path']
        rechecked = location is not None and location.is_file()
        if supplied is not None:
            assert rechecked, (arm, 'explicit compiler binary missing')
        if rechecked:
            assert sha(location) == binary_receipts[arm]['sha256'], (arm, 'compiler binary changed')
        binary_checks[arm] = rechecked
    assert build['sha256'] == manifest['binaries']['candidate']['sha256']
    assert build['binary'] == manifest['binaries']['candidate']['path']
    pairs = collections.defaultdict(dict)
    native_count = 0
    for row in rows:
        key = (row['block'], row['request_index'], row['measured'])
        assert row['arm'] not in pairs[key]
        pairs[key][row['arm']] = row
        assert row['valid'] is True and row['paired_semantics_equal'] is True
        report = row['resident']['report']
        assert report['status'] == row['status']
        assert row['resident']['active_queries'] == 0
        assert not row['resident']['session_cleared'] and row['resident']['session_epoch'] == 0
        if row['status'] == 'Success':
            assert row['paired_executable_equal'] is True
            for kind in ['executable_sha256', 'llvm_text_sha256']:
                assert len(row[kind]) == 64
            assert len(row['native']) == 6
            for test in row['native']:
                assert test['valid'] is True and test['actual'] == test['expected']
                native_count += 1
        else:
            assert row['status'] == 'SafeReject' and row['request'] == 'error'
            assert report['phase'] == 'types'
            assert [d['code'] for d in report['diagnostics']] == ['E-UNKNOWN-NAME']
            assert not row.get('native')
    sample_paths = []
    sample_counts = collections.Counter()
    for row in rows:
        label = ('prepare-' if not row['measured'] else '') + row['request']
        path = raw / f"block-{row['block']}-{row['arm']}" / f"{row['request_index']}-{label}.memory-samples.json"
        samples = json.loads(path.read_text())
        assert len(samples) == row['memory_sample_count']
        assert max(s['tree_rss_sum_kib'] for s in samples) == row['tree_peak_sampled_rss_sum_kib']
        sample_counts.update(s['processes'] for s in samples)
        sample_paths.append(path)
    assert len(sample_paths) == 34 and dict(sample_counts) == {1: 4457}
    children_probe = Path(f'/proc/{os.getpid()}/task/{os.getpid()}/children')
    sampler_scope = {
        'raw_field': 'tree_peak_sampled_rss_sum_kib',
        'label': 'legacy sampled RSS field; all recorded samples cover one process only',
        'sample_files': 34, 'samples': 4457, 'recorded_process_counts': dict(sample_counts),
        'observed_scope': 'root-only in every archived sample; no child RSS observed',
        'archived_sampler_children_path': '/proc/<pid>/task/<pid>/children',
        'current_environment_children_path_exists': children_probe.exists(),
        'current_environment_children_path_readable': os.access(children_probe, os.R_OK),
        'limitation': 'Missing children path is swallowed by descendants(); root remains. This legacy value is not process-tree RSS, child peak, total build memory, or a total-memory advantage claim.',
    }
    pair_records = []
    for key, arms in sorted(pairs.items()):
        assert set(arms) == {'baseline', 'candidate'}
        baseline, candidate = arms['baseline'], arms['candidate']
        assert baseline['request'] == candidate['request']
        assert baseline['source_identity_sha256'] == candidate['source_identity_sha256']
        assert baseline['status'] == candidate['status']
        successful = baseline['status'] == 'Success'
        if successful:
            assert baseline['executable_sha256'] == candidate['executable_sha256']
            assert baseline['llvm_text_sha256'] == candidate['llvm_text_sha256']
            assert baseline['native'] == candidate['native']
        pair_records.append({'block': key[0], 'request_index': key[1], 'measured': key[2],
                             'request': baseline['request'], 'status': baseline['status'],
                             'source_identity_sha256': baseline['source_identity_sha256'],
                             'executable_sha256': baseline.get('executable_sha256'),
                             'llvm_text_sha256': baseline.get('llvm_text_sha256')})
    assert len(pairs) == 17 and sum(p['status'] == 'Success' for p in pair_records) == 15
    assert native_count == 180
    assert len(logs) == 17 and all(log['pair_valid'] is True for log in logs)
    assert int(exit_path.read_text()) == 0
    assert summary['rows'] == 34 and summary['measured_rows'] == 18 and summary['invalid_rows'] == 0
    assert summary['native_executions'] == native_count
    cases = []
    for index, request in enumerate(manifest['requests']):
        arms = pairs[(0, index, True)]
        baseline, candidate = arms['baseline'], arms['candidate']
        assert baseline['request'] == request
        cell = next(cell for cell in summary['cells'] if cell['request'] == request)
        assert cell['valid_pairs'] == cell['expected_pairs'] == 1
        assert cell['median_baseline_ms'] == baseline['wall_ms']
        assert cell['median_candidate_ms'] == candidate['wall_ms']
        cases.append({'request': request, 'status': candidate['status'], 'main_ms': baseline['wall_ms'],
                      'candidate_ms': candidate['wall_ms'],
                      'candidate_minus_main_ms': candidate['wall_ms'] - baseline['wall_ms'],
                      'candidate_over_main': candidate['wall_ms'] / baseline['wall_ms'],
                      'main_cpu_seconds': baseline['cpu_seconds'], 'candidate_cpu_seconds': candidate['cpu_seconds'],
                      'main_legacy_sampled_rss_field_kib': baseline['tree_peak_sampled_rss_sum_kib'],
                      'candidate_legacy_sampled_rss_field_kib': candidate['tree_peak_sampled_rss_sum_kib']})
    # Mechanism assertions are for actual reported source-532 work, not newer slices.
    measured = {r['request']: r for r in rows if r['measured'] and r['arm'] == 'candidate'}
    private = measured['private']['resident']['report']
    assert (private['frontend_stats']['function_types']['executed_functions'], private['frontend_stats']['function_types']['reused_functions']) == (5, 420)
    assert (private['frontend_stats']['initialization']['candidate_executed_functions'], private['frontend_stats']['initialization']['candidate_reused_functions']) == (5, 420)
    assert private['frontend_stats']['initialization']['equal_function_results'] == 425
    assert (private['verification_work']['empty_borrow_proofs_checked'], private['verification_work']['empty_borrow_proofs_reused']) == (1, 424)
    assert (private['semantic_cache_publication']['serialized_functions'], private['semantic_cache_publication']['reused_functions']) == (1, 424)
    assert private['semantic_cache_publication']['written_bytes'] == 1462334
    assert private['semantic_cache_publication']['physical_pack_writes'] == 1
    assert 'serialized_function_plans' not in private['semantic_cache_publication']
    unchanged = measured['unchanged']['resident']['report']
    restored = measured['restore']['resident']['report']
    assert unchanged['verification_work']['full_receipts_reused'] == 1
    assert unchanged['source_ingress']['resident_frontend_hits'] == restored['source_ingress']['resident_frontend_hits'] == 1
    assert restored['frontend_stats']['producer'] is None
    assert restored['verification_work']['full_receipts_reused'] == 0
    for row in measured.values():
        backend = row['resident']['report'].get('backend_work')
        if backend is not None:
            assert backend['source_projection_traversals'] == 0
            assert backend['source_projection_instruction_visits'] == 0
            assert backend['source_projection_reuses'] == 0
    metrics = ['source_ingress', 'frontend_dependency_reads', 'frontend_stats', 'function_cache_encoding',
               'function_cache_publication', 'semantic_cache_publication', 'verification_work', 'backend_work',
               'phase_timings_ns', 'driver_timings_ns', 'materialization_ns', 'exclusive_costs']
    compact_rows = []
    for row in rows:
        record = {key: row.get(key) for key in ['block', 'request_index', 'request', 'measured', 'transition_from', 'position', 'arm',
                  'source_identity_sha256', 'changed_files', 'wall_ms', 'cpu_seconds', 'own_cpu_seconds',
                  'tree_peak_sampled_rss_sum_kib', 'memory_sample_count', 'status', 'phase', 'valid',
                  'paired_semantics_equal', 'paired_executable_equal', 'executable_sha256', 'llvm_text_sha256']}
        record['native_executions'] = len(row.get('native', []))
        record['resident'] = {k: v for k, v in row['resident'].items() if k != 'report'}
        record['counters'] = {k: row['resident']['report'].get(k) for k in metrics}
        compact_rows.append(record)
    evidence_paths = [row_path, raw/'manifest.json', raw/'summary.json', log_path, exit_path, build_path,
                      repo/build['log'], raw/'native_measure.py', raw/'native_workloads.py'] + sample_paths
    portable_build = dict(build)
    portable_build['binary'] = binary_receipts['candidate']['path']
    portable_build['binary_path_base'] = 'candidate_source_checkout'
    portable_build['target_dir'] = 'target'
    portable_build['target_dir_path_base'] = 'candidate_source_checkout'
    output = {
        'schema': 'full-pipeline-profile-source-532-v1',
        'validation_script': {'path': STABLE_SCRIPT, 'sha256': sha(Path(__file__))},
        'purpose': 'One-block diagnostic profile against main only; not a formal Rust comparison, matched matrix, statistical result, or acceptance claim.',
        'source': {'main': MAIN, 'candidate': SOURCE, 'trees': trees},
        'compiler_binaries': binary_receipts, 'candidate_build': portable_build,
        'memory_sampler_scope': sampler_scope,
        'raw': [receipt(p, repo) for p in evidence_paths],
        'workload': {k: manifest['arguments'][k] for k in ['case','modules','functions','steps','blocks','mode','backend','cache','codegen_units','codegen_jobs','capacity']},
        'candidate_extra_args': manifest['arguments']['candidate_extra_arg'],
        'validation': {'rows': 34, 'measured_rows': 18, 'preparation_rows': 16, 'pairs': 17,
                       'successful_pairs': 15, 'safe_reject_pairs': 2, 'native_executions': 180,
                       'all_rows_valid': True, 'all_pairs_semantically_equal': True,
                       'all_successful_pairs_recorded_executable_and_llvm_sha_equal': True,
                       'all_native_actual_equal_expected': True, 'compiler_binary_hashes_rechecked': binary_checks},
        'controller_discrepancy': {'persisted_exit_file': {'path': str(exit_path.relative_to(repo)), 'value': 0},
                                   'complete_log_pairs': 17, 'log_all_pair_valid': True,
                                   'write_stdin_reported_exit': 1,
                                   'write_stdin_evidence_origin': 'Root reported tool-controller observation; not encoded in raw runner files.',
                                   'resolution': 'Unresolved controller/helper exit discrepancy preserved. Semantic data validation is independent; do not rewrite tool exit as 0.'},
        'measured_cases': cases, 'pairs': pair_records, 'rows': compact_rows,
        'findings': [
            'Private: Types 5 executed/420 reused, Analyze candidate 5/420 with all 425 equal outputs, Verify function proofs 1 checked/422 reused, empty-borrow proofs 1/424 and 112197 values avoided.',
            'Private publication still writes 4 semantic segments / 1462334 payload bytes, including the whole CandidatePlan tail. FunctionPlan-fragment publication is later source 693 and is absent here.',
            'All measured backend source-projection traversal/visit/reuse counters are zero: unconsumed projections are omitted. This is not a new dynamic dependency graph; producer graph counters also remain zero.',
            'Unchanged: exact full Verify receipt reused once. Historical restore: frontend raw hit, no producer/Types/Analyze counters reported, but Verify still checks 1 function and reuses 422; this is neither zero Verify work nor a full Verify receipt hit.',
            'Private still hashes 21 function inputs, deep-clones 21 normalized items, parses one module and relocates 20 CFGs. Input-revision and mixed-slice work from later source 693 must not be attributed here.',
            'Cold retains 425 function/22 module production and higher root-only legacy RSS field than main; its wall time happens to improve in this one sample. Broad is slower than main; multi still spends 147040985ns Analyze and 202154332ns Verify despite narrow final changes.',
            'Source532 unchanged/history paths use raw-owner checkpoints. Resident caches/retained memory persist through all 17 requests (no session clears); this single-block sequence does not establish a global memory or disk quota.'
        ],
        'limits': ['One block, one pair per measured case, fixed main/candidate order alternation; OS page cache not flushed.',
                   'Only same-run main comparisons; no other worker, Rust or cross-profile speedup attribution.',
                   'Historical executable/LLVM identities are validated from recorded SHA values; past overwritten artifact bytes are not reconstructed by this script.',
                   'Legacy tree_peak_sampled_rss_sum_kib observed only the root in all 4457 samples. No observed child or total process-tree memory coverage, and no total-memory advantage claim.',
                   'Raw archive v2 predates this completed r5 profile; do not imply remote durable inclusion without a new receipt.'],
        'reproduce': f'python {STABLE_SCRIPT} --repo . --output-stem /tmp/full-pipeline-profile-532',
        'optional_binary_recheck': 'Add --main-binary MAIN_SOURCE_CHECKOUT/target/release/zil --candidate-binary CANDIDATE_SOURCE_CHECKOUT/out/runs/full-pipeline-proof-build/candidate-zil to recheck both files; recorded identities and receipt consistency are always checked.'
    }
    assert sha(row_path) == before
    args.output_stem.with_suffix('.json').write_text(json.dumps(output, indent=2, ensure_ascii=False) + '\n')
    md = ['# Source 532 single-block diagnostic profile', '',
          f'Candidate `{SOURCE}` (tree `{trees["candidate"]}`) compared only with main `{MAIN}`.', '',
          'This is one diagnostic block with one measured pair per case. It is not a formal Rust comparison, matched matrix, statistical result, or acceptance claim. Plan fragments, input-revision work and mixed slices from later source 693 are absent.', '',
          'All 34 rows validate: 18 measured + 16 preparations, 17 pairs (15 successful, 2 paired SafeReject), 180 native executions. Every successful pair has equal recorded executable and LLVM SHA-256 identities, and every native result exactly matches the expected bytes/exit code. Compiler receipt identities are checked; physical binary rechecks for this replay: '+json.dumps(binary_checks, sort_keys=True)+'.', '',
          '| Case | Main ms | Candidate ms | Candidate − main ms | Candidate/main |', '|---|---:|---:|---:|---:|']
    for case in cases:
        md.append(f'| {case["request"]} | {case["main_ms"]:.3f} | {case["candidate_ms"]:.3f} | {case["candidate_minus_main_ms"]:+.3f} | {case["candidate_over_main"]:.3f} |')
    md += ['', 'Actual work and remaining costs:', ''] + ['- '+finding for finding in output['findings']]
    md += ['', 'The broad regression is retained (+840.769 ms, +14.44%). The legacy sampled RSS field grows from 457340 to 557672 KiB for cold; cold wall time is 301.541 ms lower in this sample, so no cold wall regression is invented. Multi remains 1461.955 ms despite three final changed functions; its Analyze/Verify costs and all exact counters are retained in JSON.', '',
           'Unchanged dependency reads validate 25 snapshots/24 dependencies with 2 shared validations. Source restore has `resident_frontend_hits=1`, `producer=null`, `function_types=null`, `initialization=null`; it still has one checked Verify function/empty-borrow proof, 422/424 reused respectively, and lowers one function. Restore is a historical raw-owner frontend hit, not a disk semantic restore (`semantic_load_attempts=0`) or a complete Verify hit.', '',
           'Controller discrepancy: the helper persisted `.exit=0` and a complete 17-pair valid log, while root observed `write_stdin` reporting exit 1. Both observations remain recorded; the discrepancy is unresolved and the semantic validation does not overwrite the controller exit.', '',
           f'Memory scope: all 34 archived sample files (4457 samples) report exactly one process. The archived sampler silently falls back to its root when `/proc/<pid>/task/<pid>/children` is missing; the current probe reports exists={sampler_scope["current_environment_children_path_exists"]}, readable={sampler_scope["current_environment_children_path_readable"]}. Thus `tree_peak_sampled_rss_sum_kib` is a legacy root-only observed field here, not process-tree or total build memory. No total-memory advantage is claimed.', '', 'Evidence SHA-256 receipts:', '']
    md += [f'- `{r["path"]}`: `{r["sha256"]}` ({r["bytes"]} bytes).' for r in output['raw']]
    md += ['', f'Candidate compiler SHA-256: `{manifest["binaries"]["candidate"]["sha256"]}`; main compiler: `{manifest["binaries"]["baseline"]["sha256"]}`.', '',
           f'Validation script SHA-256: `{output["validation_script"]["sha256"]}`. Reproduce the data assertions and summaries with `python {STABLE_SCRIPT} --repo .` (the repository root is required). The companion JSON contains all 34 reduced rows, all reported frontend/Verify/backend/publication counters, nine measured comparisons and all pair identity receipts.', '',
           'No build, measurement, raw mutation or source edit was performed by this summarizer. These r5 raw files are local; remote archive durability needs its own later receipt.']
    args.output_stem.with_suffix('.md').write_text('\n'.join(md)+'\n')
    print(json.dumps(output['validation'], sort_keys=True))

if __name__ == '__main__':
    main()
