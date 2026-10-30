#!/usr/bin/env python3
"""Correctness controls, not a performance benchmark; every request is a new CLI process."""
import hashlib, json, pathlib, subprocess, sys, tempfile
binary=pathlib.Path(sys.argv[1]).resolve()
out=pathlib.Path(sys.argv[2]).resolve();out.mkdir(parents=True,exist_ok=True)
rows=[]
sha=lambda b:hashlib.sha256(b).hexdigest()
with tempfile.TemporaryDirectory(prefix='zil-cp22-cli-') as directory:
    project=pathlib.Path(directory);query=project/'queries';objects=project/'objects';exe=project/'program'
    root='import math\nimport other\nfn value()->i64=>i64(math.number())+i64(other.one())\n'
    math='fn hidden()->i32=>41\npub fn number()->i32=>hidden()\n'
    other='pub fn one()->i32=>1\n'
    def put(name,text):
        path=project/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text(text)
    put('main.zil',root);put('math.zil',math);put('other.zil',other)
    def run(label,expected=None,hits=None,parsed=None,cached=True,extra=()):
        command=[str(binary),'build',str(project/'main.zil'),'--entry','value','-o',str(exe),'--object-cache',str(objects),'--json']
        if cached:command+=['--query-cache',str(query)]
        command+=list(extra)
        result=subprocess.run(command,capture_output=True,check=False)
        (out/(label+'.stdout.json')).write_bytes(result.stdout);(out/(label+'.stderr')).write_bytes(result.stderr)
        report=json.loads(result.stdout)
        row={'label':label,'command':command,'exit':result.returncode,'status':report['status'],'phase':report['phase'],'parse_cache':report.get('parse_cache'),'query_cache':report.get('query_cache'),'verification_runs':report.get('verification_runs'),'stdout_sha256':sha(result.stdout),'stderr_sha256':sha(result.stderr),'inputs':{p.name:sha(p.read_bytes()) for p in project.glob('*.zil')}}
        if expected is not None:
            assert result.returncode==0 and report['status']=='Success',(label,report)
            assert report['verification_runs']==1,(label,report)
            if hits is not None:assert report['parse_cache']['hits']==hits,(label,report)
            if parsed is not None:assert report['parse_cache']['parsed']==parsed,(label,report)
            native=subprocess.run([str(exe)],capture_output=True,check=False)
            assert native.returncode==0 and native.stdout==f'{expected}\n'.encode() and not native.stderr,(label,native)
            (out/(label+'.native.stdout')).write_bytes(native.stdout)
            row['native_stdout_sha256']=sha(native.stdout);row['executable_sha256']=sha(exe.read_bytes())
        else:
            assert result.returncode!=0,(label,report)
            assert report.get('parse_cache',{}).get('stored',0)==0,(label,report)
        rows.append(row);return report
    run('01-cold',42,0,3)
    run('02-noop',42,3,0)
    put('main.zil','// added token before unchanged dependencies\n'+root)
    run('03-root-token-edit',42,2,1)
    math=math.replace('=>41','=>42');put('math.zil',math)
    run('04-private-body-edit',43,2,1)
    math=math.replace('number()->i32=>hidden()','number()->i64=>i64(hidden())');put('math.zil',math)
    run('05-public-signature-edit',43,2,1)
    root=root.replace('import math\nimport other','import other\nimport math');put('main.zil',root)
    run('06-import-id-reorder',43,0,3)
    run('07-reordered-noop',43,3,0)
    for path in (query/'parse-cp22-v1').glob('*.psyn'):
        b=bytearray(path.read_bytes());b[-1]^=1;path.write_bytes(b)
    repaired=run('08-corrupt-repair',43,0,3);assert repaired['parse_cache']['corrupt']==3
    old=sha(exe.read_bytes())
    (project/'other.zil').unlink()
    a=run('09-missing-cached');b=run('09-missing-uncached',cached=False)
    for field in ['status','phase','diagnostics','source_files']:assert a[field]==b[field],(field,a,b)
    assert sha(exe.read_bytes())==old
    put('other.zil',other);put('other/mod.zil',other)
    a=run('10-ambiguous-cached');b=run('10-ambiguous-uncached',cached=False)
    for field in ['status','phase','diagnostics','source_files']:assert a[field]==b[field],(field,a,b)
    assert sha(exe.read_bytes())==old
    (project/'other/mod.zil').unlink();(project/'other').rmdir()
    run('11-recovery',43,3,0)
    cancelled=run('12-cancelled',extra=['--compile-timeout-ms','0']);assert cancelled['status']=='Cancelled'
    assert sha(exe.read_bytes())==old
    manifest={'binary':str(binary),'binary_sha256':sha(binary.read_bytes()),'kind':'correctness-not-performance','requests':rows,'all_passed':True}
    (out/'MANIFEST.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps({'requests':len(rows),'native_successes':sum(r['status']=='Success' for r in rows),'all_passed':True}))
