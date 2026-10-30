#!/usr/bin/env python3
"""Read-only reconciliation of retained acceptance evidence; executes no tests."""
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import subprocess

ROOT = Path('/workspace/scratch/037bacfed72a')
REPO = ROOT / 'zil-toolchain-llvm23'
OUT = ROOT / 'llvm23-audit/candidate/48fa354'
CANDIDATE = '48fa354559be0bc7f6330b388a367e30c7184312'
BASELINE = 'ad509b5003d9d2a303428f9d01732292988084d8'
CRATES = '00f82b6884c647269c76de05340cc4f82b98b0ea'
inputs = {}


def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def note(path):
    path = Path(path)
    inputs[str(path)] = {'bytes': path.stat().st_size, 'sha256': sha(path)}
    return inputs[str(path)]


def read(path):
    path = Path(path)
    note(path)
    return json.loads(path.read_text())


def verify_identity(item):
    actual = note(item['path'])
    assert actual['sha256'] == item['sha256'], item
    if 'bytes' in item:
        assert actual['bytes'] == item['bytes'], item


def git(*args):
    return subprocess.check_output(['git', *args], cwd=REPO).decode().strip()


def summaries(path):
    note(path)
    matches = re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out', Path(path).read_text())
    assert matches, path
    keys = ('passed', 'failed', 'ignored', 'measured', 'filtered')
    return dict(zip(keys, (sum(int(row[i]) for row in matches) for i in range(5))), summaries=len(matches))


def local_gate(name, expected_status='Pass', expected_head=CANDIDATE):
    folder = ROOT / 'llvm23-validation' / name
    report = read(folder / 'result.json')
    assert report['status'] == expected_status
    assert report['before']['head'] == report['after']['head'] == expected_head
    assert report['before']['crates_tree'] == report['after']['crates_tree'] == CRATES
    assert report['before']['dirty'] == report['after']['dirty'] == ''
    if expected_status == 'Pass':
        assert report['exit_code'] == 0
    for filename, item in report['logs'].items():
        verify_identity({'path': str(folder / filename), **item})
    return {'source': expected_head, 'status': report['status'], 'exit_code': report['exit_code'], 'command': report['command'], 'receipt': str(folder / 'result.json')}


def main():
    assert git('rev-parse', 'HEAD') == CANDIDATE
    dirty = git('diff', '--name-only', CANDIDATE).splitlines()
    assert all(path in {'.task/toolchain/llvm23.md', 'docs/toolchain/llvm23/RESULT.md'} or path.startswith('evidence/toolchain/llvm23/') for path in dirty), dirty
    untracked = git('ls-files', '--others', '--exclude-standard').splitlines()
    assert all(path.startswith('evidence/toolchain/llvm23/') for path in untracked), untracked
    invariants = read(ROOT / 'llvm23-validation/source-invariants.json')
    for path in invariants['unchanged_paths']:
        assert git('rev-parse', f'{BASELINE}:{path}') == git('rev-parse', f'{CANDIDATE}:{path}') == invariants['candidate_git_objects'][path], path
    gates = {name: local_gate(name) for name in ('workspace-complete-inputs', 'release', 'clippy')}
    gates['format'] = local_gate('format', expected_head='27a474ed07d1f51682c4f700c2173b3602729c35')
    workspace_counts = summaries(ROOT / 'llvm23-validation/workspace-complete-inputs/stdout.log')
    assert workspace_counts == {'passed':2743,'failed':0,'ignored':0,'measured':0,'filtered':0,'summaries':285}, workspace_counts
    gates['workspace-complete-inputs']['counts'] = workspace_counts
    gates['format']['qualification'] = 'Local formatting run was on prior 27a with the identical crates tree; candidate-exact formatting is provided by CI.'

    failures = {}
    failed_workspace = read(ROOT / 'llvm23-validation/workspace/result.json')
    assert failed_workspace['status'] == 'Fail' and failed_workspace['exit_code'] != 0
    for filename, item in failed_workspace['logs'].items():
        verify_identity({'path':str(ROOT/'llvm23-validation/workspace'/filename),**item})
    fail_counts = summaries(ROOT/'llvm23-validation/workspace/stdout.log')
    assert fail_counts['failed'] == 5
    failures['workspace'] = {'status':'Fail','counts':fail_counts,'scope':'Earlier incomplete sparse checkout; accepted full run is separate.'}
    for name, expected in [('reference-workspace','InfrastructureFail'),('reference-workspace-after-archive','InfrastructureInterrupted')]:
        folder = ROOT / 'llvm23-validation' / name
        report = read(folder/'infrastructure-failure.json')
        assert report['status'] == expected and report['cargo_exit'] is None
        for filename, item in report['logs'].items():
            verify_identity({'path':str(folder/filename),**item})
        assert name != 'reference-workspace-after-archive' or not (folder/'result.json').exists()
        failures[name] = {'status':expected,'cargo_exit':None,'runner_exit':report['runner_exit'],'reason':report['reason'],'accepted_as_pass':False}

    candidate_binary = read(ROOT/'llvm23-candidate-build/binary.json')
    assert candidate_binary['source'] == CANDIDATE and candidate_binary['crates_tree'] == CRATES
    verify_identity({'path':candidate_binary['binary'],'bytes':candidate_binary['bytes'],'sha256':candidate_binary['sha256']})
    assert (ROOT/'llvm23-candidate-build/result.json').read_bytes() == (ROOT/'llvm23-validation/release/result.json').read_bytes()
    note(ROOT/'llvm23-candidate-build/result.json')
    baseline_build = read(ROOT/'llvm23-baseline-build/build-receipt.json')
    assert baseline_build['source_head'] == baseline_build['head_after'] == BASELINE
    assert baseline_build['source_clean_before'] and baseline_build['source_clean_after'] and baseline_build['exit_code'] == 0
    baseline_binary = baseline_build['binary']
    verify_identity({'path':baseline_binary['frozen_path'],'bytes':baseline_binary['bytes'],'sha256':baseline_binary['sha256']})
    for item in baseline_build['logs']:
        verify_identity(item)

    controls = read(ROOT/'llvm23-validation/migration-controls/result.json')
    assert controls['status'] == 'Pass' and controls['performance'] == 'NotMeasured'
    assert len(controls['commands']) == 33 and len(controls['builds']) == 10
    commands = {c['name']:c for c in controls['commands']}
    for c in commands.values():
        assert c['status'] == 'Pass' and not c['timed_out'] and c['exit_code'] == c['expected_exit'], c['name']
        verify_identity(c['stdout']); verify_identity(c['stderr'])
    for arm, source, expected_version, binary_hash in [('baseline',BASELINE,'17.0.6',baseline_binary['sha256']),('candidate',CANDIDATE,'23.1.4',candidate_binary['sha256'])]:
        frozen = controls['frozen'][arm]
        assert frozen['declared_source'] == source and frozen['cli']['sha256'] == binary_hash
        for name in ('cli','clang','llvm'):
            verify_identity(frozen[name])
        version = read(commands[arm+'-llvm-version']['stdout']['path'])
        assert version['version'] == expected_version
        link_text = Path(commands[arm+'-linked-library']['stdout']['path']).read_text()
        assert frozen['llvm']['path'] in link_text, link_text
    cells = []
    for opt, units, requested, jobs in [('O0',3,4,2),('O2',1,1,1)]:
        sequence = ['baseline-cold','baseline-warm','candidate-cold','baseline-warm-after-candidate','candidate-warm']
        previous = {}
        for stage in sequence:
            name = opt+'-'+stage
            b = next(x for x in controls['builds'] if x['name']==name)
            command = commands[name]
            raw = read(command['stdout']['path'])
            cold = stage.endswith('-cold')
            version = '17.0.6' if stage.startswith('baseline') else '23.1.4'
            assert b['status']=='Pass' and b['version']==version and b['native_exit']==42 and b['verification_runs']==1
            assert raw['status']=='Success' and raw['verification_runs']==1 and len(raw['commands'])==1
            cg=raw['native_codegen']; cache=cg['object_cache']
            assert cg['backend']=='llvm-api' and cg['actual_units']==units and cg['requested_units']==requested and cg['jobs']==jobs and cg['fallback'] is None
            assert cache['hits']==(0 if cold else units) and cache['misses']==(units if cold else 0) and cache['stored']==(units if cold else 0)
            assert cache['corrupt']==cache['write_failures']==0 and b['cache']==cache
            assert len(cg['object_reports'])==units
            for obj in cg['object_reports']:
                assert obj['llvm_version']==version and obj['parse_ns']==0
                if cold:
                    assert obj['input_kind']=='builder' and obj['verify_before_after'] is True and obj['pipeline']=='default<'+opt+'>'
                else:
                    assert obj['input_kind']=='object-cache' and obj['llvm_verification_this_request'] is False and obj['producer_verified_before_after'] is True
            current=b['cache_files']
            assert all(current.get(k)==v for k,v in previous.items())
            assert len(current)-len(previous)==(units if cold else 0)
            previous=current
            native=commands[name+'-native']; assert native['exit_code']==native['expected_exit']==42
            cells.append({'name':name,'version':version,'units':units,'hits':cache['hits'],'misses':cache['misses'],'stored':cache['stored'],'native_exit':42,'current_verify':1})
        for path, expected in previous.items():
            assert sha(ROOT/'llvm23-validation/migration-controls'/opt/'objects'/path)==expected
    wrong=read(commands['wrong-llvm-version']['stdout']['path'])
    assert commands['wrong-llvm-version']['exit_code']==2 and wrong['verification_runs']==1
    assert any(d['code']=='E-LLVM-API-VERSION' for d in wrong['diagnostics']) and wrong['commands']==[] and wrong['artifacts']=={} and wrong['native_codegen'] is None
    wc=read(commands['wrong-clang-version']['stdout']['path'])
    assert commands['wrong-clang-version']['exit_code']==1 and wc['status']=='Fail' and wc['checks']['llvm']['version']=='23.1.4'
    assert '17.0.6' in wc['error'] and '23.1.4' in wc['error']
    assert 'llvm17-baseline' in commands['wrong-clang-version']['environment']['ZIL_CLANG']
    bad_control=next(x for x in controls['controls'] if x['name']=='wrong-llvm-version')
    assert bad_control['kind']=='version-function interposition'
    for item in bad_control['preserved'].values():
        verify_identity(item)

    folder=ROOT/'llvm23-validation/consumer208'
    consumer=read(folder/'summary.json')
    assert consumer['status']=='Pass' and consumer['source_commit']==CANDIDATE and consumer['cli_sha256']==candidate_binary['sha256']
    assert len(consumer['builds'])==16 and len(consumer['observations'])==128 and consumer['commands']==145
    assert consumer['oracle_sha256']==sha(REPO/'tools/language/177/consumer.py')
    for name, expected in consumer['source_hashes'].items():
        assert sha(REPO/'examples/mainline/zui_language177'/name)==expected
    objects=0
    for build in consumer['builds']:
        p=folder/build['name']/'build'; record=read(p/'record.json'); raw=read(p/'stdout'); note(p/'stderr')
        assert record['returncode']==0 and 'timeout' not in record and raw['status']=='Success' and raw['verification_runs']==1
        assert raw['native_codegen']==build['codegen'] and build['verification_runs']==1
        assert len(raw['commands'])==1 and 'ir' not in raw['commands'][0]
        for obj in build['codegen']['object_reports']:
            assert obj['llvm_version']=='23.1.4' and obj['input_kind']=='builder' and obj['parse_ns']==0 and obj['verify_before_after'] is True
            objects+=1
        exe=folder/(build['name']+'-program')
        assert sha(exe)==build['executable_sha256'] and sha(str(exe)+'.ll')==build['llvm_sha256']
    injected=0
    for observation in consumer['observations']:
        path=folder/(observation['case']+'-fail'+str(observation['fail_at']))
        record=read(path/'record.json'); note(path/'stdout'); note(path/'stderr')
        assert 'timeout' not in record and record['returncode']==observation['exit']
        assert (path/'stdout').read_text()==observation['stdout']
        allocation=observation['allocation']
        if allocation is not None:
            assert read(path/'stderr')==allocation and allocation['live']==0 and allocation['allocated']==allocation['frees']
            assert allocation['failures']==int(observation['fail_at']>0)
            assert record['environment']['ZIL_TEST_FAIL_AT']==str(observation['fail_at'])
        else:
            assert (path/'stderr').read_bytes()==b'' and observation['fail_at']==0
        injected+=observation['fail_at']>0
    assert objects==40 and injected==56
    summary=read(ROOT/'llvm23-validation/controls-execution-summary.json')
    assert summary['independent_execution'] is False and summary['candidate']==CANDIDATE
    for relative,item in summary['raw_summaries'].items():
        verify_identity({'path':str(ROOT/'llvm23-validation'/relative),**item})

    bundle_folder=REPO/'evidence/toolchain/llvm23'
    index=read(bundle_folder/'bundle-index.json')
    assert index['source_candidate']==CANDIDATE
    for bundle in index['bundles']:
        verify_identity({'path':str(bundle_folder/bundle['name']),'sha256':bundle['sha256'],'bytes':bundle['bytes']})
    report_path=REPO/'docs/toolchain/llvm23/RESULT.md'; note(report_path)
    report = {
        'schema_version':1,'recorded_at_utc':datetime.now(timezone.utc).isoformat(),
        'reviewer':'/root/llvm_migration_audit','implementation_commit':CANDIDATE,
        'status':'Pass','scope':'Read-only reconciliation of retained local gate, migration and consumer evidence. No test or compiler invocation performed.',
        'source':{'crates_tree':CRATES,'unchanged_paths_verified':invariants['unchanged_paths'],'current_tracked_changes':dirty,'untracked_files_are_evidence_only':True},
        'local_gates':gates,'preserved_failures':failures,
        'migration':{'builds':10,'native_runs':10,'all_native_exit':42,'commands':33,'cells':cells,'real_clang_mismatch':'Pass','version_function_interposition':'Pass','cache_and_previous_output_preserved':True,'independent_execution':False},
        'consumer':{'builds':16,'native_runs':128,'allocation_failure_injections':56,'object_receipts':40,'llvm_version':'23.1.4','all_builder_parse_zero':True,'current_verify_every_build':True,'raw_observations_reconciled':128,'independent_execution':False},
        'binary_binding':{'candidate_binary_sha256':candidate_binary['sha256'],'baseline_binary_sha256':baseline_binary['sha256'],'separate_clean_source_build_receipts_verified':True},
        'compact_archive_hashes_verified':len(index['bundles']),
        'limitations':['Local reference attempts are not accepted as Pass; complete reference acceptance must come from exact-tree CI.','Author-generated migration and consumer runs remain author executions; this is independent evidence review.','Cache coexistence covers complete compiler-plus-DSO identities rather than isolating one key field.','Large native archives remain local; compact archive digests were checked, not another full native archive replay.','No performance measurement, native Windows execution or product-level Go is inferred.'],
        'inputs':inputs,'capture_script_sha256':sha(__file__),
    }
    output=OUT/'acceptance-local-checks.json'
    assert not output.exists(), 'Do not overwrite an existing acceptance receipt'
    output.write_text(json.dumps(report,indent=2,ensure_ascii=False)+'\n')
    print(json.dumps({'status':'Pass','receipt':str(output),'sha256':sha(output),'inputs':len(inputs),'workspace':workspace_counts,'migration_builds':10,'migration_native':10,'consumer_builds':16,'consumer_native':128,'injections':56}))


if __name__ == '__main__':
    main()
