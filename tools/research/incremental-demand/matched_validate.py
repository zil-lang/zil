#!/usr/bin/env python3
"""Read-only audit of a complete matched_measure.py archive, including raw data.

This audit recomputes counts, identities, timing/native/diagnostic oracles and
object hashes. It does not claim an independent review of the runner/compiler.
"""
import argparse
from collections import Counter
import hashlib
import json
import math
from pathlib import Path
import statistics
import sys

import rust_compare as rc

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
    checked = set()
    def check_blob(value):
        relative = value['archive']
        if relative not in checked:
            path = (out/relative).resolve()
            require(path.is_relative_to(out) and path.is_file(), 'archive path missing/unsafe')
            require(file_hash(path)==value['sha256'] and path.stat().st_size==value['bytes'], 'blob hash/length mismatch')
            checked.add(relative)
    for arm in ['baseline','candidate']:
        check_blob(manifest['binaries'][arm])
        check_blob(manifest['binaries'][arm]['build_receipt'])
        binding=json.loads((out/manifest['binaries'][arm]['build_receipt']['archive']).read_text())
        require(binding['source']==manifest['sources'][arm]['commit'] and binding['tree']==manifest['sources'][arm]['tree'] and binding['sha256']==manifest['binaries'][arm]['sha256'] and binding['exit_code']==0 and bool(binding['command']), 'build receipt source/binary binding failed')
        check_blob(manifest['sources'][arm]['source_tree_inventory'])
        check_blob(manifest['sources'][arm]['worktree_diff'])
        require(manifest['binary_after_sha256'][arm]==manifest['binaries'][arm]['sha256'], 'binary changed during run')
    require(manifest['sources']['baseline']['commit']=='99035931d358add0eaed6d4535035c38413701dd', 'wrong main pin')
    for name,tool in manifest['tools'].items():
        require(manifest['tools_after_sha256'][name]==tool['actual_sha256'], 'tool changed during run')
    require('58cf8f9c35b355199727627d07ae34a4e2846d67' in (out/'rustc-version'/'stdout.bin').read_text(), 'wrong rustc commit')
    require('release: 1.99.1' in (out/'rustc-version'/'stdout.bin').read_text(), 'wrong Rust release')
    require('5f94df4789f005f9a352888e8355ffc645b7ed0e' in (out/'cargo-version'/'stdout.bin').read_text(), 'wrong Cargo commit')
    rows = [json.loads(line) for line in (out/'rows.jsonl').read_text().splitlines()]
    require(len(rows)==270 and sum(r['measured'] for r in rows)==144, 'row counts differ')
    native_count, inventory_counts = 0, Counter()
    for block in range(6):
        current = [r for r in rows if r['block']==block]
        expected=[]
        for index,label in enumerate(LABELS):
            parent = rc.parent(label)
            for name,measured in ([(parent,False)] if parent else [])+[(label,True)]:
                expected += [(index,name,measured)]*3
        require([(r['request_index'],r['request'],r['measured']) for r in current]==expected, 'named parent request order mismatch')
        require(len(current)==45, 'block row count differs')
        for arm in ['baseline','candidate']:
            command=json.loads((out/f'block-{block:02d}'/arm/'command.json').read_text())['argv']
            require(command[0]==manifest['binaries'][arm]['path'] and command[1]==('check' if mode=='check' else 'build'), 'Zil binary or mode differs')
            require(command[command.index('--resident-capacity')+1]=='256' and command[command.index('--resident-reset-after')+1]=='4096' and '--semantic-cache' in command, 'resident cache/history changed')
            require('--profile-compilation' not in command and '--incremental-o2' not in command, 'profile or alternate backend path in wall run')
            if mode=='o2':
                require(all(command[command.index(key)+1]==value for key,value in [('--backend','llvm-api'),('--opt','O2'),('--codegen-units','16'),('--codegen-jobs','1')]) and '--object-cache' in command, 'native settings differ')
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
    for row in rows:
        require(row['valid'] is True and row['mode']==mode, 'invalid row/mode')
        label,arm = row['request'],row['arm']
        require(row['transition_from']==(rc.parent(row['target_request']) if row['measured'] else None), 'named parent differs')
        require(row['source_identity_sha256']==manifest['fixture_identity'][label]['zil'], 'row source differs from fixture')
        directory = out/row['directory']
        require(json.loads((directory/'request.json').read_text())==row, 'raw request differs from row')
        require(json.loads((directory/'result.json').read_text())==row['measurement'], 'raw timing differs from row')
        require(math.isfinite(row['measurement']['wall_ms']) and row['measurement']['wall_ms']>0, 'invalid wall time')
        if arm=='rust':
            command=json.loads((directory/'command.json').read_text())['argv']
            require(all(flag in command for flag in ['--offline','--locked','--verbose','--jobs','--message-format=json']), 'Cargo cache/history/command flags missing')
            require(command[command.index('--jobs')+1]=='1' and ('--release' in command)==(mode=='o2'), 'Cargo jobs/optimization changed')
            result=row['measurement']
            require(result['returncode']==(101 if label=='error' else 0) and not result['timed_out'], 'Rust process failure')
            require(file_hash(directory/'stdout.bin')==result['stdout_sha256'] and file_hash(directory/'stderr.bin')==result['stderr_sha256'], 'Rust raw process stream differs')
            messages=rc.cargo_messages(directory/'stdout.bin')
            diagnostics=[m.get('message',{}) for m in messages if m.get('reason')=='compiler-message']
            if label=='error':
                require(rc.expected_error(diagnostics,fixture_data[label][1]), 'Rust precise diagnostic oracle failed')
        else:
            response=json.loads((directory/'response.json').read_text())
            require(response==row['resident'], 'resident raw response differs')
            report=response['report']
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
            for item in inventory.get('metadata',[]):
                check_blob(item)
    summary={'valid':True,'rows':len(rows),'measured_requests':144,'preparation_requests':126,
             'native_executions':native_count,'verified_blobs':len(checked),'cells':{}}
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
