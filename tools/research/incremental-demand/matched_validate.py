#!/usr/bin/env python3
"""Read-only audit of a complete matched_measure.py archive, including raw data.

This audit recomputes counts, identities, timing/native/diagnostic oracles and
object hashes. It does not claim an independent review of the runner/compiler.
"""
import argparse
from collections import Counter
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import statistics
import sys

import rust_compare as rc
import push_counters as pc

LABELS = rc.LABELS
ARMS = ['baseline', 'candidate', 'rust']


def file_hash(path):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024*1024), b''):
            digest.update(chunk)
    return digest.hexdigest()


def audit(out):
    manifest = json.loads((out/'manifest.json').read_text())
    completion = json.loads((out/'completion.json').read_text())
    require = lambda condition, message: None if condition else fail(message)
    require(not (out/'failure.json').exists(), 'failure receipt present')
    require(completion == {'valid':True,'rows':270,'measured':144,'preparations':126}, 'wrong complete counts')
    fixed = {'blocks':6,'requests':LABELS,'modules':20,'functions':20,'steps':16,
             'codegen_units_requested':16,'codegen_jobs':1,'native_opt':'O2','cache':'all','capacity':256}
    require(manifest['fixed']==fixed, 'changed workload, cache or backend configuration')
    require(manifest.get('completed_utc') is not None, 'completion timestamp missing')
    mode = manifest['arguments']['mode']
    for name, digest in manifest['implementation'].items():
        require(file_hash(out/'implementation'/name)==digest, 'implementation hash mismatch: '+name)
    require(manifest['implementation_after_sha256']==manifest['implementation'], 'implementation changed during run')
    # Frozen generator and independent integer interpreter, not compiled output.
    workloads = rc.load_workloads(Path(manifest['arguments']['repo']),out/'implementation'/'native_workloads.py')
    sys.modules['native_workloads'] = workloads
    spec = importlib.util.spec_from_file_location('validated_native_runtime', out/'implementation'/'native_measure.py')
    native = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(native)
    environments = pc.inspect_runtime_environments(manifest)
    fixture_data = {}
    for label in LABELS:
        zfiles = {str(p.relative_to(out/'fixtures'/label/'zil')):p.read_text() for p in (out/'fixtures'/label/'zil').rglob('*.zil')}
        rfiles = {str(p.relative_to(out/'fixtures'/label/'rust')):p.read_text() for p in (out/'fixtures'/label/'rust').rglob('*.rs')}
        rc.validate_equivalence(zfiles,rfiles,20,20,16,label)
        for lang,files in [('zil',zfiles),('rust',rfiles)]:
            hashes = {name:rc.sha(text.encode()) for name,text in sorted(files.items())}
            identity = rc.sha(json.dumps(hashes,sort_keys=True).encode())
            require(identity==manifest['fixture_identity'][label][lang], 'fixture content mismatch')
        fixture_data[label] = (zfiles,rfiles)
    checked = {}
    def check_blob(value):
        relative = value['archive']
        if relative not in checked:
            path = (out/relative).resolve()
            require(path.is_relative_to(out) and path.is_file(), 'archive path missing/unsafe')
            checked[relative] = (file_hash(path), path.stat().st_size)
        require(checked[relative] == (value['sha256'], value['bytes']), 'blob hash/length mismatch')
    for arm in ['baseline','candidate']:
        check_blob(manifest['binaries'][arm])
        check_blob(manifest['binaries'][arm]['build_receipt'])
        binding=json.loads((out/manifest['binaries'][arm]['build_receipt']['archive']).read_text())
        require(binding['source']==manifest['sources'][arm]['commit'] and binding['tree']==manifest['sources'][arm]['tree'] and binding['sha256']==manifest['binaries'][arm]['sha256'] and binding['exit_code']==0 and bool(binding['command']), 'build receipt source/binary binding failed')
        check_blob(manifest['sources'][arm]['source_tree_inventory'])
        check_blob(manifest['sources'][arm]['worktree_diff'])
        require(manifest['binary_after_sha256'][arm]==manifest['binaries'][arm]['sha256'], 'binary changed during run')
    pc.baseline_pin(manifest['sources']['baseline']['commit'], manifest['arguments']['baseline_source'])
    extras = pc.candidate_flags(manifest['arguments'].get('candidate_extra_arg', []))
    push = '--push-reuse' in extras
    require(manifest.get('llvm_comparison_policy', pc.LLVM_RAW) == (pc.LLVM_PUSH if push else pc.LLVM_RAW), 'LLVM comparison policy differs from research flags')
    require(manifest['sources']['candidate']['commit'] == manifest['arguments']['candidate_source'], 'candidate differs from explicit source pin')
    pc.inspect_commands(out, manifest)
    resolved_cargo = pc.resolved_tool(out, manifest, 'cargo')
    pc.resolved_tool(out, manifest, 'rustc')
    for name,tool in manifest['tools'].items():
        require(manifest['tools_after_sha256'][name]==tool['actual_sha256'], 'tool changed during run')
    require('58cf8f9c35b355199727627d07ae34a4e2846d67' in (out/'rustc-version'/'stdout.bin').read_text(), 'wrong rustc commit')
    require('release: 1.99.1' in (out/'rustc-version'/'stdout.bin').read_text(), 'wrong Rust release')
    require('5f94df4789f005f9a352888e8355ffc645b7ed0e' in (out/'cargo-version'/'stdout.bin').read_text(), 'wrong Cargo commit')
    rows = [json.loads(line) for line in (out/'rows.jsonl').read_text().splitlines()]
    require(len(rows)==270 and sum(r['measured'] for r in rows)==144, 'row counts differ')
    native_count, inventory_counts = 0, Counter()
    namespace_bindings = {}
    runtime_bindings = {}
    for block in range(6):
        for arm in ['baseline', 'candidate']:
            directory = out/f'block-{block:02d}'/arm
            command = json.loads((directory/'command.json').read_text())
            require(command['environment'] == environments[arm], 'resident launch environment differs from arm declaration')
            pid = json.loads((directory/'startup.json').read_text())['pid']
            first = json.loads((directory/'runtime-startup.json').read_text())
            last = json.loads((directory/'runtime-final.json').read_text())
            first_dsos = pc.inspect_runtime_receipt(first, environments[arm], pid, out, check_blob, native.llvm_mappings)
            last_dsos = pc.inspect_runtime_receipt(last, environments[arm], pid, out, check_blob, native.llvm_mappings)
            require(first_dsos == last_dsos == manifest['runtime_libraries'][arm], 'resident LLVM DSO changed across block or differs from pinned run libraries')
            if block:
                require(first_dsos == runtime_bindings[(0, arm)][1], 'arm LLVM DSO changed between blocks')
            runtime_bindings[(block, arm)] = (pid, first_dsos)
    for block in range(6):
        current = [r for r in rows if r['block']==block]
        expected=[]
        for index,label in enumerate(LABELS):
            parent = rc.parent(label)
            for name,measured in ([(parent,False)] if parent else [])+[(label,True)]:
                expected += [(index,name,measured)]*3
        require([(r['request_index'],r['request'],r['measured']) for r in current]==expected, 'named parent request order mismatch')
        require(len(current)==45, 'block row count differs')
        project=out/f'block-{block:02d}'/'rust-project'
        require((project/'Cargo.toml').read_text()==rc.manifest_text(16) and (project/'Cargo.lock').read_text()==rc.lock_text(), 'actual Cargo profile or lock changed')
        for offset in range(0,len(current),3):
            triplet = current[offset:offset+3]
            require(sorted(r['arm'] for r in triplet)==sorted(ARMS), 'missing or duplicated arm')
            require([r['position'] for r in triplet]==[0,1,2], 'sequential positions differ')
            require([r['arm'] for r in triplet]==triplet[0]['order'], 'recorded arm order differs')
            pair_receipt = json.loads((out/triplet[0]['directory']).parent.joinpath('pair-validation.json').read_text())
            require(pair_receipt['valid'] is True, 'Zil pair semantic mismatch')
            require(len(set(r['source_identity_sha256'] for r in triplet))==1, 'three-arm source identity mismatch')
            left,right = [r['resident']['report'] for r in triplet if r['arm']!='rust']
            require(all(left[k]==right[k] for k in ['status','phase','diagnostics','input_fingerprint','source_files','scalar_interfaces']), 'raw Zil reports disagree')
            if mode=='o2' and triplet[0]['request']!='error':
                zil_rows=[r for r in triplet if r['arm']!='rust']
                require(zil_rows[0]['executable']['sha256']==zil_rows[1]['executable']['sha256'] and pair_receipt['executable_equal'] is True, 'main/candidate executable bytes differ')
                raw_equal = zil_rows[0]['llvm']['sha256'] == zil_rows[1]['llvm']['sha256']
                require(pair_receipt['llvm_text_equal'] is raw_equal, 'raw LLVM equality receipt differs')
                if push:
                    comparisons = []
                    for row in zil_rows:
                        check_blob(row['llvm'])
                        comparison = pc.llvm_comparison_sha256((out / row['llvm']['archive']).read_bytes())
                        require(comparison == row['llvm']['comparison_sha256_excluding_identity_comment'], 'LLVM comparison hash differs from archived bytes')
                        comparisons.append(comparison)
                    require(comparisons[0] == comparisons[1] and pair_receipt['raw_llvm_equal'] is raw_equal and pair_receipt['llvm_equal_excluding_identity_comment'] is True, 'LLVM differs outside the identity comment')
                else:
                    require(raw_equal, 'legacy LLVM full bytes differ')
    for row in rows:
        require(row['valid'] is True and row['mode']==mode, 'invalid row/mode')
        label,arm = row['request'],row['arm']
        if label=='error':
            require(row['publication_before']==row['publication_after'], 'error request changed final publication history')
        require(row['transition_from']==(rc.parent(row['target_request']) if row['measured'] else None), 'named parent differs')
        require(row['source_identity_sha256']==manifest['fixture_identity'][label]['zil'], 'row source differs from fixture')
        directory = out/row['directory']
        require(json.loads((directory/'request.json').read_text())==row, 'raw request differs from row')
        require(json.loads((directory/'result.json').read_text())==row['measurement'], 'raw timing differs from row')
        require(math.isfinite(row['measurement']['wall_ms']) and row['measurement']['wall_ms']>0, 'invalid wall time')
        if arm=='rust':
            command=json.loads((directory/'command.json').read_text())['argv']
            pc.rust_command_flags(command, cargo=resolved_cargo, mode=mode,
                                  target=Path(manifest['arguments']['out']) / f'block-{row["block"]:02d}' / 'rust-target')
            result=row['measurement']
            require(result['returncode']==(101 if label=='error' else 0) and not result['timed_out'], 'Rust process failure')
            require(file_hash(directory/'stdout.bin')==result['stdout_sha256'] and file_hash(directory/'stderr.bin')==result['stderr_sha256'], 'Rust raw process stream differs')
            messages=rc.cargo_messages(directory/'stdout.bin')
            diagnostics=[m.get('message',{}) for m in messages if m.get('reason')=='compiler-message']
            if label=='error':
                require(rc.expected_error(diagnostics,fixture_data[label][1]), 'Rust precise diagnostic oracle failed')
        else:
            runtime = json.loads((directory/'runtime.json').read_text())
            require(runtime == row['runtime'], 'raw runtime receipt differs from row')
            pid, expected_dsos = runtime_bindings[(row['block'], arm)]
            dsos = pc.inspect_runtime_receipt(runtime, environments[arm], pid, out, check_blob, native.llvm_mappings)
            require(dsos == expected_dsos, 'request used different LLVM DSO bytes/identity from its resident startup')
            response=json.loads((directory/'response.json').read_text())
            require(response==row['resident'], 'resident raw response differs')
            report=response['report']
            if arm == 'candidate' and '--push-reuse' in extras:
                pc.inspect_report(report, mode=mode, cold=label == 'cold' and row['measured'])
            if label=='error':
                errors=report['diagnostics']
                require(report['status']=='SafeReject' and report['phase']=='types' and report['verification_runs']==0 and len(errors)==1, 'Zil error request accepted or wrong rejection')
                error=errors[0]
                span=error['span']
                offset=fixture_data[label][0]['m0.zil'].encode().index(b'absent_name')
                require(error['code']=='E-UNKNOWN-NAME' and span['start']==offset and span['end']==offset+len(b'absent_name') and Path(report['source_files'][str(span['file'])]).name=='m0.zil', 'Zil precise diagnostic oracle failed')
            else:
                require(report['status']=='Success' and report['verification_runs']==1 and report['diagnostics']==[], 'current independent Verify missing')
                require(sum(e=={'phase':'verify','disposition':'executed'} for e in report['query_events'])==1, 'current Verify event missing')
        if mode=='o2' and label!='error':
            check_blob(row['executable'])
            if arm!='rust':
                check_blob(row['llvm'])
            outcomes=row['native']
            require(len(outcomes)==6*manifest['arguments']['native_repetitions'], 'native execution count differs')
            for outcome in outcomes:
                expected=workloads.oracle('scalar',bytes.fromhex(outcome['stdin_hex']),label,20,20,16)
                require(outcome['valid'] is True and outcome['expected']==expected==outcome['actual'], 'native oracle mismatch')
                native_dir=out/outcome['directory']
                native_result=json.loads((native_dir/'result.json').read_text())
                actual={'exit':native_result['returncode'],'stdout_hex':(native_dir/'stdout.bin').read_bytes().hex(),'stderr_hex':(native_dir/'stderr.bin').read_bytes().hex()}
                require(actual==expected and not native_result['timed_out'], 'raw native output differs')
                native_count+=1
            inventory=row['inventory']
            require(inventory['object_count']==len(inventory['objects'])>0, 'actual object inventory empty/wrong count')
            if arm!='rust':
                require(inventory['object_count']==row['resident']['report']['native_codegen']['actual_units'], 'Zil actual object count differs from current receipts')
                namespace=inventory['namespace']
                require(inventory['cache_schema']=='objects-v1' and len(namespace)==64 and all(c in '0123456789abcdef' for c in namespace), 'invalid Zil namespace binding')
                require(all(Path(item['path']).parts==('objects-v1',namespace,item['action_key']+'.zobj') for item in inventory['objects']), 'Zil object is outside bound namespace')
                binding_key=(row['block'],arm)
                require(namespace_bindings.setdefault(binding_key,namespace)==namespace, 'Zil namespace changed within block/arm')
                receipts=row['resident']['report']['native_codegen']['object_reports']
                require([item['unit_index'] for item in inventory['objects']]==list(range(len(receipts))), 'Zil unit index mapping is missing, reordered or duplicated')
                for item,receipt in zip(inventory['objects'],receipts):
                    receipt=json.loads(receipt) if isinstance(receipt,str) else receipt
                    require(item['action_key']==receipt['cache']['action_key'] and item['cache_observation']==receipt['cache'], 'Zil unit object does not match current receipt action key/state')
            inventory_counts[(arm,row['request'])]+=inventory['object_count']
            if arm=='rust':
                current=inventory['current_session']
                require(current is not None and all(str(Path(item['path']).parent)==current for item in inventory['objects']), 'Rust current-session inventory differs')
                require(all(str(Path(item['path']).parent)!=current for item in inventory.get('historical_objects',[])), 'historical Rust objects included in current count')
                new=[session for session in inventory['sessions'] if session not in inventory['previous_sessions'] and not Path(session).name.endswith('-working')]
                require((len(new)==1 and current==new[0]) or (not new and current==inventory['previous_current_session']), 'Rust session selection lacks transition evidence')
            for item in inventory['objects']+inventory.get('historical_objects',[]):
                check_blob(item['object'])
                require(file_hash(out/item['symbols']['path'])==item['symbols']['sha256'], 'object symbol content differs')
                if arm!='rust':
                    check_blob(item['cache_container'])
                    container=(out/item['cache_container']['archive']).read_bytes()
                    require(len(container)>80 and container[:8]==b'ZILOBJ01' and container[8:40].hex()==item['action_key'] and container[40:72].hex()==item['object']['sha256'] and int.from_bytes(container[72:80],'little')==len(container)-80 and rc.sha(container[80:])==item['object']['sha256'], 'Zil container magic/key/payload checksum/length mismatch')
            for item in inventory.get('metadata',[]):
                check_blob(item)
    summary={'valid':True,'rows':len(rows),'measured_requests':144,'preparation_requests':126,
             'native_executions':native_count,'verified_blobs':len(checked),
             'runtime_binding': {'schema': manifest['runtime_binding_schema'],
                                 'arms': {arm: {'environment': environments[arm],
                                               'llvm_dsos': [{'path': item['resolved_path'], 'sha256': item['sha256']}
                                                             for item in runtime_bindings[(0, arm)][1]]}
                                          for arm in ['baseline', 'candidate']}}, 'cells':{}}
    for label in LABELS:
        summary['cells'][label]={}
        for arm in ARMS:
            selected=[r for r in rows if r['measured'] and r['request']==label and r['arm']==arm]
            require(len(selected)==6, 'missing measured cell')
            values=[r['measurement']['wall_ms'] for r in selected]
            summary['cells'][label][arm]={'wall_ms':values,'median_ms':statistics.median(values),
                                        'actual_object_counts':[r.get('inventory',{}).get('object_count') for r in selected]}
    return summary


def fail(message):
    raise ValueError(message)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('out',type=Path)
    parser.add_argument('--write-summary',type=Path)
    args=parser.parse_args()
    try:
        result=audit(args.out.resolve())
    except (OSError,ValueError,KeyError,TypeError,IndexError) as error:
        print(json.dumps({'valid':False,'error':str(error)}))
        return 1
    if args.write_summary:
        rc.save(args.write_summary,result)
    print(json.dumps(result,ensure_ascii=False))
    return 0


if __name__=='__main__':
    raise SystemExit(main())
