#!/usr/bin/env python3
"""Validate existing scalar diagnostic profile evidence; never compile or measure."""
import argparse
import collections
import hashlib
import json
from pathlib import Path
import shlex
import subprocess

STABLE = 'tools/research/incremental-demand/profile_validate.py'
ORACLE_PATH = 'tools/research/reactive-query-graph/native_workloads.py'
METRICS = ('source_ingress', 'frontend_dependency_reads', 'frontend_stats', 'function_cache_encoding',
           'function_cache_publication', 'semantic_cache_publication', 'verification_work', 'backend_work',
           'phase_timings_ns', 'driver_timings_ns', 'materialization_ns', 'exclusive_costs')
REQUESTS = ('cold', 'unchanged', 'private', 'multi', 'broad', 'positions', 'error', 'recovery', 'restore')

def sha(path):
    value = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            value.update(chunk)
    return value.hexdigest()

def read(path):
    return json.loads(path.read_text())

def lines(path):
    return [json.loads(line) for line in path.read_text().splitlines()]

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('repo', 'run-dir', 'build-receipt', 'output-stem'):
        parser.add_argument('--'+name, type=Path, required=True)
    parser.add_argument('--source', required=True)
    parser.add_argument('--controller-exit', type=lambda value: None if value == 'unknown' else int(value), required=True, help='externally observed tool exit, or unknown; retained separately')
    parser.add_argument('--main-binary', type=Path)
    parser.add_argument('--reference-fixtures', type=Path, help='compare source identities only; never compare its times')
    parser.add_argument('--expect-plan-fragments', action='store_true')
    parser.add_argument('--expect-body-revision', action='store_true')
    parser.add_argument('--expect-relocated-input-revision', action='store_true')
    parser.add_argument('--expect-whole-machine', action='store_true')
    parser.add_argument('--scope-note', default='Only the supplied frozen source is represented.')
    args = parser.parse_args()
    repo = args.repo.resolve()
    def local(path):
        return path.resolve() if path.is_absolute() else (repo/path).resolve()
    def relative(path):
        return str(path.relative_to(repo))
    raw, build_path = local(args.run_dir), local(args.build_receipt)
    row_path = raw/'rows.jsonl'
    raw_sha = sha(row_path)
    rows, manifest, summary = lines(row_path), read(raw/'manifest.json'), read(raw/'summary.json')
    build = read(build_path)
    log_path, exit_path = raw.with_suffix('.log'), raw.with_suffix('.exit')
    log, helper_exit = lines(log_path), int(exit_path.read_text())
    assert len(rows) == 34 and sum(r['measured'] for r in rows) == 18
    assert manifest['requests'] == list(REQUESTS)
    assert manifest['source_shas']['candidate'] == build['source'] == args.source
    assert manifest['source_shas']['baseline'] == '99035931d358add0eaed6d4535035c38413701dd'
    assert build['exit_code'] == helper_exit == 0
    assert manifest['arguments']['candidate_extra_arg'] == ['--producer-reuse', '--profile-compilation']
    for field, expected in [('case','scalar'),('modules',20),('functions',20),('steps',16),('blocks',1),('mode','O2'),('backend','llvm-api'),('cache','all'),('capacity',256),('codegen_units',16),('codegen_jobs',1)]:
        assert manifest['arguments'][field] == expected
    assert sha(raw/'native_measure.py') == manifest['runner_sha256']
    assert sha(raw/'native_workloads.py') == manifest['generator_sha256']
    # Execute only the generator bytes bound to this exact source commit, never
    # arbitrary raw archive code or the current checkout's potentially newer file.
    generator_bytes = (raw/'native_workloads.py').read_bytes()
    generator_spec = f'{args.source}:{ORACLE_PATH}'
    pinned_bytes = subprocess.check_output(['git','show',generator_spec], cwd=repo)
    assert generator_bytes == pinned_bytes
    generator_blob = subprocess.check_output(['git','rev-parse',generator_spec], cwd=repo, text=True).strip()
    oracle_namespace = {'__name__':'pinned_profile_oracle','__file__':str(repo/ORACLE_PATH)}
    exec(compile(pinned_bytes,generator_spec,'exec'),oracle_namespace)
    oracle = oracle_namespace['oracle']
    tree = subprocess.check_output(['git','show','--no-patch','--format=%T',args.source], cwd=repo, text=True).strip()
    assert tree == build['tree']
    assert build['sha256'] == manifest['binaries']['candidate']['sha256']
    assert build['binary'] == manifest['binaries']['candidate']['path']
    candidate_binary = build_path.parent/Path(build['binary']).name
    assert candidate_binary.is_file() and sha(candidate_binary) == build['sha256']
    main_rechecked = args.main_binary is not None
    if main_rechecked:
        assert sha(args.main_binary) == manifest['binaries']['baseline']['sha256']
    pairs, native_n, sample_paths, sample_counts = collections.defaultdict(dict), 0, [], collections.Counter()
    reduced = []
    for row in rows:
        key = (row['block'], row['request_index'], row['measured'])
        assert row['arm'] not in pairs[key]
        pairs[key][row['arm']] = row
        report, resident = row['resident']['report'], row['resident']
        assert row['case'] == 'scalar' and row['mode'] == 'O2'
        assert row['valid'] is True and row['paired_semantics_equal'] is True
        assert report['status'] == row['status']
        assert resident['active_queries'] == resident['session_epoch'] == 0 and not resident['session_cleared']
        if row['status'] == 'Success':
            assert row['paired_executable_equal'] is True and len(row['native']) == 6
            assert all(len(row[k]) == 64 for k in ('executable_sha256','llvm_text_sha256'))
            for test in row['native']:
                expected = oracle(row['case'],bytes.fromhex(test['stdin_hex']),row['request'],20,20,16)
                assert test['valid'] is True and test['actual'] == test['expected'] == expected
                native_n += 1
        else:
            assert row['status'] == 'SafeReject' and row['request'] == 'error' and report['phase'] == 'types'
            assert [d['code'] for d in report['diagnostics']] == ['E-UNKNOWN-NAME'] and not row.get('native')
        label = ('prepare-' if not row['measured'] else '')+row['request']
        path = raw/f"block-{row['block']}-{row['arm']}"/f"{row['request_index']}-{label}.memory-samples.json"
        samples = read(path)
        assert len(samples) == row['memory_sample_count']
        assert max(v['tree_rss_sum_kib'] for v in samples) == row['tree_peak_sampled_rss_sum_kib']
        sample_counts.update(v['processes'] for v in samples)
        sample_paths.append(path)
        reduced.append({k: row.get(k) for k in ('block','request_index','request','measured','arm','status','phase','valid','wall_ms','cpu_seconds','source_identity_sha256','executable_sha256','llvm_text_sha256','tree_peak_sampled_rss_sum_kib','memory_sample_count')})
    assert len(pairs) == len(log) == 17 and all(v['pair_valid'] is True for v in log)
    assert native_n == 180 and set(sample_counts) == {1}
    assert (summary['rows'],summary['measured_rows'],summary['invalid_rows'],summary['native_executions']) == (34,18,0,180)
    pair_receipts = []
    for key, arms in sorted(pairs.items()):
        assert set(arms) == {'baseline','candidate'}
        main_row, candidate = arms['baseline'], arms['candidate']
        assert all(main_row[k] == candidate[k] for k in ('request','status','source_identity_sha256'))
        if candidate['status'] == 'Success':
            assert all(main_row[k] == candidate[k] for k in ('executable_sha256','llvm_text_sha256','native'))
        pair_receipts.append({'block':key[0], 'request_index':key[1], 'measured':key[2], **{k:candidate.get(k) for k in ('request','status','source_identity_sha256','executable_sha256','llvm_text_sha256')}})
    assert sum(v['status']=='Success' for v in pair_receipts) == 15
    fixtures = None
    if args.reference_fixtures:
        reference = local(args.reference_fixtures)
        old = lines(reference)
        assert len(old) == len(rows)
        assert all(all(a[k] == b[k] for k in ('block','request_index','request','measured','arm','source_identity_sha256')) for a,b in zip(rows,old))
        fixtures = {'path':relative(reference),'sha256':sha(reference),'all_34_source_identities_equal':True,'comparison_scope':'Source fixture identity only; its timing data is neither extracted nor compared.'}
    reports = {r['request']:r['resident']['report'] for r in rows if r['measured'] and r['arm']=='candidate'}
    cold, private, positions = (reports[k] for k in ('cold','private','positions'))
    assert cold['frontend_stats']['function_types']['executed_functions'] == 425
    assert cold['frontend_stats']['initialization']['candidate_executed_functions'] == 425
    assert cold['backend_work']['lowered_functions'] == 425
    assert (private['verification_work']['function_proofs_checked'],private['verification_work']['function_proofs_reused']) == (1,422)
    assert (private['verification_work']['empty_borrow_proofs_checked'],private['verification_work']['empty_borrow_proofs_reused'],private['verification_work']['empty_borrow_values_avoided']) == (1,424,112197)
    assert reports['unchanged']['verification_work']['full_receipts_reused'] == 1
    restore = reports['restore']
    assert restore['source_ingress']['resident_frontend_hits'] == 1 and restore['source_ingress']['semantic_load_attempts'] == 0
    assert all(restore['frontend_stats'][k] is None for k in ('producer','function_types','initialization'))
    assert (restore['verification_work']['full_receipts_reused'],restore['verification_work']['function_proofs_checked'],restore['verification_work']['function_proofs_reused'],restore['verification_work']['empty_borrow_proofs_checked'],restore['verification_work']['empty_borrow_proofs_reused']) == (0,1,422,1,424)
    for report in reports.values():
        if report.get('backend_work') is not None:
            assert all(report['backend_work'][k] == 0 for k in ('source_projection_traversals','source_projection_instruction_visits','source_projection_reuses'))
    if args.expect_plan_fragments:
        assert cold['semantic_cache_publication']['serialized_function_plans'] == 425
        for request in ('private','multi','broad','positions','recovery'):
            value = reports[request]['semantic_cache_publication']
            assert (value['serialized_function_plans'],value['reused_function_plans'],value['serialized_function_plan_bytes'],value['reused_function_plan_bytes']) == (0,425,0,1447096)
        value = private['semantic_cache_publication']
        assert (value['written_segments'],value['written_bytes'],value['physical_pack_writes'],value['physical_pack_bytes']) == (4,15214,1,15314)
    if args.expect_body_revision or args.expect_relocated_input_revision:
        for request in ('private','error','recovery'):
            value = reports[request]['frontend_stats']['function_types']
            assert (value['body_key_executions'],value['body_key_reuses'],value['body_key_visited_nodes']) == (1,420,389)
        assert (positions['function_cache_encoding']['serialized_functions'],positions['function_cache_encoding']['reused_functions']) == (0,421)
        position_types = positions['frontend_stats']['function_types']
        if args.expect_relocated_input_revision:
            assert (position_types['body_key_executions'],position_types['body_key_reuses'],position_types['body_key_visited_nodes']) == (0,421,0)
            producer = positions['frontend_stats']['producer']
            assert (producer['function_input_hashes'],producer['function_input_reuses'],producer['function_input_binding_hashes']) == (0,425,425)
            assert (producer['lower_modules'],producer['reused_lower_modules']) == (22,0)
            materializer = positions['frontend_stats']['materialization']
            assert (materializer['executed_modules'],materializer['reused_modules'],materializer['cloned_modules']) == (22,0,22)
        else:
            # Keep the previous profile's negative control unless the new
            # relocated-input mechanism is explicitly expected by this run.
            assert (position_types['body_key_executions'],position_types['body_key_reuses']) == (421,0)
    if args.expect_whole_machine:
        machine = reports['unchanged']['backend_work']
        assert machine['reused_machine_module'] is True
        assert (machine['lowered_functions'],machine['reused_functions'],machine['whole_machine_input_functions_compared']) == (0,0,425)
    cases = []
    for index, request in enumerate(REQUESTS):
        a,b = pairs[(0,index,True)]['baseline'],pairs[(0,index,True)]['candidate']
        cell = next(v for v in summary['cells'] if v['request']==request)
        assert cell['valid_pairs'] == cell['expected_pairs'] == 1
        assert (cell['median_baseline_ms'],cell['median_candidate_ms']) == (a['wall_ms'],b['wall_ms'])
        cases.append({'request':request,'status':b['status'],'main_ms':a['wall_ms'],'candidate_ms':b['wall_ms'],'candidate_minus_main_ms':b['wall_ms']-a['wall_ms'],'candidate_over_main':b['wall_ms']/a['wall_ms'],'candidate_counters':{k:reports[request].get(k) for k in METRICS}})
    evidence = [row_path,raw/'manifest.json',raw/'summary.json',log_path,exit_path,build_path,local(Path(build['log'])),raw/'native_measure.py',raw/'native_workloads.py']+sample_paths
    receipts = [{'path':relative(p),'sha256':sha(p),'bytes':p.stat().st_size} for p in evidence]
    command = ['python',STABLE,'--repo','.', '--run-dir',relative(raw),'--source',args.source,'--build-receipt',relative(build_path),'--controller-exit',('unknown' if args.controller_exit is None else str(args.controller_exit)),'--output-stem',str(args.output_stem)]
    if args.reference_fixtures: command += ['--reference-fixtures',relative(local(args.reference_fixtures))]
    if args.expect_plan_fragments: command += ['--expect-plan-fragments']
    if args.expect_body_revision: command += ['--expect-body-revision']
    if args.expect_relocated_input_revision: command += ['--expect-relocated-input-revision']
    if args.expect_whole_machine: command += ['--expect-whole-machine']
    command += ['--scope-note',args.scope_note]
    validation = {'rows':34,'measured_rows':18,'preparation_rows':16,'pairs':17,'successful_pairs':15,'safe_reject_pairs':2,'native_executions':180,'all_semantically_valid':True,'all_successful_pairs_recorded_elf_llvm_sha_equal':True,'all_native_actual_equal_expected':True,'native_oracles_independently_recomputed':180,'oracle_bound_to_source_git_blob':True,'full_cold_functions':425,'candidate_binary_physically_rechecked':True,'main_binary_physically_rechecked':main_rechecked}
    output = {'schema':'single-block-profile-v1','purpose':'Single-block scalar diagnosis against same-run main only, not a formal Rust comparison, statistical result, acceptance, or improvement versus any prior candidate.',
              'source':{'main':manifest['source_shas']['baseline'],'candidate':args.source,'candidate_tree':tree},
              'oracle_binding':{'source':args.source,'path':ORACLE_PATH,'git_blob':generator_blob,'archived_sha256':manifest['generator_sha256'],'archive_equals_pinned_git_bytes':True,'recomputed_native_expectations':180},
              'scope_note':args.scope_note,'workload':{k:manifest['arguments'][k] for k in ('case','modules','functions','steps','blocks','mode','backend','cache','codegen_units','codegen_jobs','capacity')},
              'binaries':{'main':{'path_base':'main_source_checkout','path':'target/release/zil','sha256':manifest['binaries']['baseline']['sha256']},'candidate':{'path_base':'candidate_source_checkout','path':relative(candidate_binary),'sha256':build['sha256']}},
              'candidate_build':{'receipt':relative(build_path),'source':build['source'],'tree':tree,'toolchain':build['toolchain'],'command':build['command'],'exit_code':build['exit_code'],'log':build['log']},
              'validation':validation,'expectations':{'plan_fragments':args.expect_plan_fragments,'body_revision':args.expect_body_revision,'relocated_input_revision':args.expect_relocated_input_revision,'whole_machine':args.expect_whole_machine},'controller':{'persisted_helper_exit':helper_exit,'externally_reported_tool_exit':args.controller_exit,'reported_tool_exit_origin':'Root observation supplied via --controller-exit; null means the controller response was unavailable after context truncation/session expiry. Not independently encoded by runner.','complete_log_pairs':17,'agreement':None if args.controller_exit is None else helper_exit==args.controller_exit},
              'legacy_memory_scope':{'raw_field':'tree_peak_sampled_rss_sum_kib','sample_files':34,'samples':sum(sample_counts.values()),'process_counts':dict(sample_counts),'observed_scope':'root-only in every archived sample; no child RSS observed. Sampler swallows unavailable /proc/<pid>/task/<pid>/children. Not process-tree or total build memory, and no memory advantage claim.'},
              'fixture_reference':fixtures,'cases':cases,'all_rows':reduced,'pair_receipts':pair_receipts,'raw_receipts':receipts,
              'validation_script':{'path':STABLE,'sha256':sha(Path(__file__))},'reproduce':shlex.join(command),
              'limitations':['One pair per measured case, one block; OS page cache not flushed. No comparison against prior candidate timings.','Recorded ELF/LLVM pair hashes and actual native results are checked; overwritten historical artifact files are not reconstructed.','Default quotas and current independent Verify remain part of the source pipeline; narrow counters do not imply all work is item-local.','Raw durability requires a separate remote archive receipt.']}
    assert sha(row_path) == raw_sha
    args.output_stem.with_suffix('.json').write_text(json.dumps(output,indent=2)+'\n')
    md = [f'# Frozen {args.source[:3]} single-block scalar diagnostic profile','',output['purpose'], '', f'Source `{args.source}`, tree `{tree}`. {args.scope_note}', '',
          'Validated 34 rows, 17 pairs (15 successful, 2 paired SafeReject), 180 exact native outcomes independently recomputed by the archived generator after its bytes are matched to the supplied source Git blob; every successful pair has equal recorded ELF and LLVM SHA-256. The cold pipeline processes all 425 functions with asserted CGU=16/jobs=1. No sample or fixture was reduced.', '',
          '| Case | Main ms | Candidate ms | Candidate − main ms | Ratio |','|---|---:|---:|---:|---:|']
    for c in cases: md += [f'| {c["request"]} | {c["main_ms"]:.3f} | {c["candidate_ms"]:.3f} | {c["candidate_minus_main_ms"]:+.3f} | {c["candidate_over_main"]:.3f} |']
    if args.expect_plan_fragments:
        md += ['', 'Actual publication: private serializes 1 function/reuses 424 and serializes 0 FunctionPlans/reuses all 425 (1,447,096 cached row bytes). It writes 4 new logical segments / 15,214 payload bytes in 1 physical pack of 15,314 bytes; the manifest is 113,401 bytes. Cold has 856 ordered logical entries/855 unique items: identical 12-byte module/plan suffixes are deduplicated within that pack (`existing_segments=1` is not prior-epoch reuse). It serializes all 425 plans and writes 6,738,230 unique payload bytes. Segment counts describe logical items; physical pack writes remain separately reported. Multi/broad/positions/recovery also reuse all 425 raw plan rows.']
    if args.expect_body_revision or args.expect_relocated_input_revision:
        parts = []
        for request in ('private','error','recovery','positions'):
            values = reports[request]['frontend_stats']['function_types']
            producer = reports[request]['frontend_stats']['producer']
            part = (f"{request}: BodyKey {values['body_key_executions']} executed/{values['body_key_reuses']} reused, "
                    f"{values['body_key_visited_nodes']} visited nodes; producer text hashes/reuses "
                    f"{producer['function_input_hashes']}/{producer['function_input_reuses']}")
            if 'function_input_binding_hashes' in producer:
                part += f", current binding hashes {producer['function_input_binding_hashes']}"
            parts.append(part)
        md += ['', 'Input revision counters: '+ '; '.join(parts)+'. FunctionStore positions serialization/reuse is 0/421; these are separate counters.']
    if args.expect_relocated_input_revision:
        producer = positions['frontend_stats']['producer']
        materializer = positions['frontend_stats']['materialization']
        md += ['', f"Relocation preserves raw text revisions while renewing current bindings: positions Lower {producer['lower_modules']} executed/{producer['reused_lower_modules']} reused and materialization {materializer['executed_modules']} executed/{materializer['reused_modules']} reused/{materializer['cloned_modules']} cloned. The revision shortcut does not reuse old lowering or materialization outputs."]
    if args.expect_whole_machine:
        machine = reports['unchanged']['backend_work']
        md += ['', f"Unchanged whole-machine reuse: reused_machine_module={machine['reused_machine_module']}, per-function lowered/reused={machine['lowered_functions']}/{machine['reused_functions']}, whole input functions compared={machine['whole_machine_input_functions_compared']}. The 425 compatibility comparisons remain real work. This does not assert zero full equality or zero input checking for cold or edited requests; their reported counters remain in JSON."]
    clone_parts = []
    for request in ('private','multi','broad','positions'):
        producer = reports[request]['frontend_stats'].get('producer') or {}
        if all(k in producer for k in ('normalized_item_deep_clones','restored_module_deep_clones')):
            clone_parts.append(f"{request} {producer['normalized_item_deep_clones']} normalized-item/{producer['restored_module_deep_clones']} restored-module clones")
    md += ['', 'Reported remaining cloning: '+('; '.join(clone_parts) if clone_parts else 'not reported')+'.']
    phases, initialization, proofs = private['phase_timings_ns'], private['frontend_stats']['initialization'], private['verification_work']
    md += ['', f"Private Analyze/Verify take {phases['analyze']/1e6:.3f}/{phases['verify']/1e6:.3f} ms. Analyze candidate {initialization['candidate_executed_functions']} executed/{initialization['candidate_reused_functions']} reused, {initialization['equal_function_results']} equal outputs; Verify function proofs {proofs['function_proofs_checked']} checked/{proofs['function_proofs_reused']} reused, empty-borrow {proofs['empty_borrow_proofs_checked']}/{proofs['empty_borrow_proofs_reused']}, {proofs['empty_borrow_values_avoided']} values avoided. Current module/interface checks remain."]
    negative = [f"{c['request']} {c['candidate_minus_main_ms']:+.3f} ms" for c in cases if c['candidate_minus_main_ms']>0]
    multi = next(c for c in cases if c['request']=='multi')
    broad = reports['broad']['frontend_stats']['function_types']
    md += ['', 'Wall regressions versus this run’s main: '+(', '.join(negative) if negative else 'none in these single samples')+f". Multi remains {multi['candidate_ms']:.3f} ms; broad executes {broad['executed_functions']} Types functions. Residual position work is represented by its actual counters above and in JSON. No improvement versus a prior candidate is inferred from this one block.", '',
          f'Memory caveat: `{output["legacy_memory_scope"]["raw_field"]}` is a legacy field. All {sum(sample_counts.values())} samples in 34 files report exactly one process, with per-row sample lengths/maxima verified. Only root RSS was observed; do not call it process-tree RSS, total build memory, or a memory advantage.', '',
          'History: unchanged reuses one full Verify receipt. Restore is a historical raw frontend hit (`resident_frontend_hits=1`, no producer/Types/Analyze counters), but still checks 1 Verify function and reuses 422 (empty-borrow 1/424); it is not a full Verify hit or disk semantic restore. Backend source-projection counters remain zero where reported. Error and recovery results remain part of the 34-row validation.', '',
          f'Controller: persisted helper exit {helper_exit}, root-reported tool exit {args.controller_exit}, complete 17-pair valid log. These observations are recorded separately.', '',
          f'Compiler SHA-256 candidate `{build["sha256"]}`, main `{manifest["binaries"]["baseline"]["sha256"]}`. Build receipt source/tree and candidate binary bytes were verified. Script SHA-256 `{output["validation_script"]["sha256"]}`.', '', 'Core raw receipts:', '']
    md += [f'- `{r["path"]}`: `{r["sha256"]}`.' for r in receipts[:9]]
    md += ['', 'JSON includes all 34 reduced rows, 17 pair identity receipts, all nine measured candidate counter sets, and individual SHA-256 receipts for all 34 sample files. Fixture-reference comparison checks source identities only, never its timing values.', '', 'Reproduce from the repository root:', '', '```sh',shlex.join(command),'```', '', 'Add `--main-binary MAIN_SOURCE_CHECKOUT/target/release/zil` to physically recheck main as well as the saved candidate binary; recorded compiler identities and receipt consistency are always checked. No build or measurement is run by this validator. No root source was edited to produce this report.']
    args.output_stem.with_suffix('.md').write_text('\n'.join(md)+'\n')
    print(json.dumps(validation,sort_keys=True))

if __name__ == '__main__':
    main()
