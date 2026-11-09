#!/usr/bin/env python3
"""Implementation-authored mechanical audit, never a non-author source review."""
from pathlib import Path
import argparse, hashlib, json, math, statistics, struct, tarfile


def close(left,right):
    assert math.isclose(left,right,rel_tol=1e-10,abs_tol=1e-7),(left,right)


def digest(data): return hashlib.sha256(data).hexdigest()


def main():
    parser=argparse.ArgumentParser(); parser.add_argument('--run',type=Path,required=True); parser.add_argument('--out',type=Path,required=True); args=parser.parse_args()
    root=args.run; receipt=json.loads((root/'receipt.json').read_text())
    for name,meta in receipt['files'].items():
        path=root/name; assert path.stat().st_size==meta['bytes']; assert digest(path.read_bytes())==meta['sha256'],name
    manifest=json.loads((root/'manifest.json').read_text()); protocol=manifest['protocol']; summary=json.loads((root/'summary.json').read_text())
    rows=[json.loads(line) for line in (root/'rows.jsonl').read_text().splitlines()]
    cases={case['id']:case for case in manifest['cases']}; assert len(cases)==10
    close(sum(c['weight'] for c in cases.values()),1)
    for group,weight in protocol['case_group_weights'].items(): close(sum(c['weight'] for c in cases.values() if c['group']==group),weight)
    for family in ('mode_weights','request_weights'):
        for weights in protocol[family].values(): close(sum(weights.values()),1)
    blobs={}; native_images={}; total=0; names=set()
    with tarfile.open(root/'raw.tar.gz','r|gz') as archive:
        for member in archive:
            assert not member.name.startswith('/') and '..' not in Path(member.name).parts
            if not member.isfile(): continue
            assert member.name not in names; names.add(member.name); total+=member.size; assert total<1024**3
            wanted=member.name.startswith('logs/') or member.name.startswith('corpus/')
            image=member.name.startswith('quality/') and member.name.endswith('/app')
            if not (wanted or image): continue
            data=archive.extractfile(member).read()
            if image:
                assert data[:6]==b'\x7fELF\x02\x01'
                off=struct.unpack_from('<Q',data,40)[0]; step,count,string_index=struct.unpack_from('<HHH',data,58)
                sections=[struct.unpack_from('<IIQQQQIIQQ',data,off+i*step) for i in range(count)]
                strings=sections[string_index]; strings=data[strings[4]:strings[4]+strings[5]]
                for section in sections:
                    if strings[section[0]:strings.index(0,section[0])]==b'.text':
                        native_images[member.name]=(len(data),digest(data[section[4]:section[4]+section[5]])); break
                else: raise AssertionError('missing machine text')
            else: blobs[member.name]=data
    for name,expected in manifest['source_hashes'].items(): assert digest(blobs[name])==expected,name
    def process(row):
        prefix=row['log']; original=json.loads(blobs[prefix+'/process.json'])
        for key in ('argv','exit','timeout','wall_ms','cpu_ms','rss_kib','stdout_sha256','stderr_sha256'):
            assert original[key]==row[key],(prefix,key)
        assert digest(blobs[prefix+'/stdout'])==row['stdout_sha256']
        assert digest(blobs[prefix+'/stderr'])==row['stderr_sha256']
        return blobs[prefix+'/stdout'],blobs[prefix+'/stderr']
    expected_snapshots={'cold':'base','warm':'base','private':'private','public':'public','restore':'base'}
    identities=set(); formal=[]; native=0
    for row in rows:
        key=tuple(row[k] for k in ('case','mode','request','arm','block')); assert key not in identities; identities.add(key)
        assert row['snapshot']==expected_snapshots[row['request']] and row['warmup']==(row['block']<0)
        stdout,stderr=process(row); report=json.loads(stdout)
        assert row['exit']==0 and not row['timeout'] and row['valid']
        assert report['status']=='Success' and report['verification_runs']==row['verification_runs']==1
        if row['mode']!='check':
            cg=report['native_codegen']; assert cg==row['native_codegen'] and cg['backend']=='llvm-api' and cg['actual_units']>=1
            assert len(cg['object_reports'])==cg['actual_units']
            assert all(obj.get('llvm_version')=='23.1.4' for obj in cg['object_reports'])
            controls=cases[row['case']]['inputs']; assert len(controls)==len(row['native_checks'])
            for observed,control in zip(row['native_checks'],controls):
                out,err=process(observed); wanted=blobs[control['expected'][row['snapshot']]]
                assert observed['exit']==0 and out==wanted and not err and observed['valid']
                native+=1
        if not row['warmup']: formal.append(row)
    expected={(c,m,r,a,b) for c in cases for m in ('check','O0','O2') for r in expected_snapshots for a in ('base','cand') for b in [-1,*range(protocol['rounds'])]}
    assert identities==expected and len(rows)==summary['compile_requests']==2400 and len(formal)==summary['formal_requests']==2100
    assert summary['current_matrix_verify_runs']==2400
    calculated=[]
    for mode_name,modes in protocol['mode_weights'].items():
        for request_name,requests in protocol['request_weights'].items():
            totals={a:0.0 for a in ('base','cand')}; impact={}
            for row in formal:
                weight=cases[row['case']]['weight']*modes[row['mode']]*requests[row['request']]/protocol['rounds']
                totals[row['arm']]+=weight*row['wall_ms']
                key=(row['case'],row['mode'],row['request']); impact[key]=impact.get(key,0)+(1 if row['arm']=='base' else -1)*weight*row['wall_ms']
            old=next(p for p in summary['profiles'] if p['profile']==mode_name+'/'+request_name)
            for arm in totals: close(totals[arm],old['weighted_mean_ms'][arm])
            reduction=1-totals['cand']/totals['base']; close(reduction,old['reduction'])
            close(totals['base']-totals['cand'],old['saved_seconds_per_1000_requests'])
            ranked=sorted(impact.items(),key=lambda item:item[1],reverse=True)
            calculated.append({'profile':old['profile'],'weighted_mean_ms':totals,'reduction':reduction,
               'saved_seconds_per_1000_requests':totals['base']-totals['cand'],
               'largest_savings':[{'cell':list(k),'contribution_ms':v} for k,v in ranked[:5]],
               'largest_losses':[{'cell':list(k),'contribution_ms':v} for k,v in ranked[-3:]]})
    runtime=[json.loads(line) for line in (root/'runtime.jsonl').read_text().splitlines()]; assert len(runtime)==summary['runtime_executions']==192
    for row in runtime:
        out,err=process(row); assert row['valid'] and row['exit']==0 and not err
        path=f"quality/{row['case']}/{row['snapshot']}/{row['arm']}/app"
        assert native_images[path]==(row['elf_bytes'],row['text_sha256'])
    result={'status':'Pass: raw evidence and weighted arithmetic consistent','scope':'implementation-authored mechanical audit; not non-author source review',
      'compile_requests':len(rows),'formal_requests':len(formal),'matrix_native_checks':native,'runtime_executions':len(runtime),
      'profiles':calculated,'runtime_screen_pass':summary['runtime_screen_pass'],'severe_tails':summary['severe_tails'],
      'independent_review':'NotRun','merge_allowed':False,'receipt_sha256':digest((root/'receipt.json').read_bytes())}
    args.out.parent.mkdir(parents=True,exist_ok=True); args.out.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n'); print(json.dumps(result,ensure_ascii=False,indent=2))
if __name__=='__main__': main()
