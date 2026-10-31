#!/usr/bin/env python3
"""Independent read-only audit of the three final59d windows. No compilers run."""
from pathlib import Path
from collections import Counter,defaultdict
from fractions import Fraction
import hashlib,json,subprocess,importlib.util,tempfile,random,math,itertools

ROOT=Path('/workspace/.zil-env/research/stage5-followup')
REPO=Path('/workspace/zil')
SHA='59d8753dc23f0ddfcddbaf5780e3057f3c60b9e9'
PROD='59d8753dc23f0ddfcddbaf5780e3057f3c60b9e9'
WINDOWS={'final-sanity-generated':(92,46),'final-sanity-real':(112,56),'sanity-generated':(92,46),'sanity-real':(112,56)}
EDITS={'cold':0,'unchanged':0,'early_edit':1,'late_edit':2,'restore':0}
INPUTS=[b'',b'!',b'D',b'S',b'U',b'O',b'M',b'E',b'T',b'L',b'0',b'1',b'2',b'3',b'12as+!-q\0\x80']
inventory={};checks=Counter();reconstructed={};binaries={}
def digest(data):return hashlib.sha256(data).hexdigest()
def readfile(path):
    path=Path(path);data=path.read_bytes()
    inventory[str(path)]={'bytes':len(data),'sha256':digest(data)}
    return data
def j(path):return json.loads(readfile(path))
def git(path):return subprocess.check_output(['git','show',SHA+':'+path],cwd=REPO)
def med(values):
    s=sorted(values);n=len(s);assert n
    return s[n//2] if n%2 else (s[n//2-1]+s[n//2])/2
def near(a,b):
    assert (a is None)==(b is None),(a,b)
    if a is not None:assert math.isclose(float(a),float(b),rel_tol=2e-14,abs_tol=2e-12),(a,b)
    checks['numeric_fields']+=1
def expected(case,edit):
    total=0
    for i,n in enumerate(case['sizes']):
      for k in range(n):
        x=k%5;v=k%7+1
        if edit==1 and i==0 and k==0:v+=1
        if edit==2 and i==len(case['sizes'])-1 and k==n-1:v+=2
        f=case['flavor'];result=(x if x<3 else 2*x)+v if f=='branch' else x+(2*v-1) if f=='ct' else x+64+v if f=='body' else x+v
        total+=result*(len(case['sizes']) if case.get('graph')=='fanout' and i==0 else 1)
    return total
def native_outcomes(outcomes):
    assert len(outcomes)==15
    for inp,n in zip(INPUTS,outcomes):
        assert set(n)=={'stdin_hex','exit','stdout_hex','stderr_hex'}
        assert n['stdin_hex']==inp.hex() and type(n['exit']) is int and n['exit']>=0
        bytes.fromhex(n['stdout_hex']);bytes.fromhex(n['stderr_hex'])
def get_generated(case,edit):
    key=(case['name'],edit)
    if key not in reconstructed:
        with tempfile.TemporaryDirectory(prefix='review-stage5-inputs-',dir='/tmp') as t:
            p=Path(t);actual=generator.generate(case,p,edit);assert actual==expected(case,edit)
            reconstructed[key]={ext:{x.name:digest(x.read_bytes()) for x in sorted(p.glob('*.'+ext))} for ext in ['zil','rs']}
    return reconstructed[key]

# The frozen generator is used only to reconstruct bytes, never its runner/validation/statistics.
source=git('tools/research/incremental-compilation/distribution_bench.py')
snapshot=ROOT/'sanity-frozen-distribution-generator.py';snapshot.write_bytes(source)
spec=importlib.util.spec_from_file_location('review_frozen_generator',snapshot);generator=importlib.util.module_from_spec(spec);spec.loader.exec_module(generator)
assert len(generator.shapes())==23
all_windows={};table_expected=[]
for name,(want_rows,want_cells) in WINDOWS.items():
    SHA='59d8753dc23f0ddfcddbaf5780e3057f3c60b9e9' if name.startswith('final-') else '05e0b9de2727dce3432d82a587970d4c3123300d'
    folder=ROOT/name;manifest=j(folder/'manifest.json');summary=j(folder/'summary.json')
    rows=[json.loads(s) for s in readfile(folder/'rows.jsonl').splitlines()]
    assert len(rows)==want_rows and summary['rows']==want_rows and len(summary['cells'])==want_cells
    assert manifest['source']==SHA and manifest['blocks']==1 and manifest['cpus']==[0,1]
    generated=isinstance(manifest['cases'][0],dict);reference='baseline' if name=='followup-isolation' else 'rust'
    arms=['zil',reference];closed=name.endswith('-closed')
    assert manifest['closed_executable']==closed
    runner_path='tools/research/incremental-compilation/'+('distribution_bench.py' if generated else 'real_pair_bench.py')
    assert readfile(folder/'runner.py')==git(runner_path)
    assert manifest['tool_sha256']==digest(git(runner_path))
    if generated:assert manifest['source_cache'] is False and manifest['requests']==[['cold',0]]
    else:
        assert readfile(folder/'distribution_bench.py')==source and manifest['runner_sha256']==digest(source)
        assert manifest['stdin_hex']==[x.hex() for x in INPUTS]
        assert manifest['native_arguments']=={'app':{'rust':['--check'],'zil':[]}}
    for arm,binary in manifest['binaries'].items():
        assert digest(readfile(binary['path']))==binary['sha256']
        binaries[binary['path']]=binary['sha256']
    cases={c['name']:c for c in manifest['cases']} if generated else {c:None for c in manifest['cases']}
    requests=['cold'] if generated else ['cold','unchanged']
    wanted_order=[(c,m,b,p,a,q) for c in cases for m in manifest['modes'] for b in range(1) for p,a in enumerate(arms if b%2==0 else arms[::-1]) for q in requests]
    actual_order=[tuple(r[k] for k in ('case','mode','block','position','arm','request')) for r in rows]
    assert actual_order==wanted_order
    oracles={};sources={}
    for case,shape in cases.items():
        p=folder/('source-'+case)
        if generated:
            baseline=get_generated(shape,0)
            for ext in ['zil','rs']:
                got={x.name:digest(readfile(x)) for x in sorted(p.glob('*.'+ext))};assert got==baseline[ext]
            metrics=j(p/'metrics.json')
            for ext in ['zil','rs']:
                fs=list(p.glob('*.'+ext));expected_metrics={'files':len(fs),'bytes':sum(f.stat().st_size for f in fs),'max_file_bytes':max(f.stat().st_size for f in fs),'lines':sum(len(f.read_text().splitlines()) for f in fs)}
                assert metrics[ext]==expected_metrics
            for q,edit in manifest['requests']:
                rebuilt=get_generated(shape,edit)
                for arm in arms:
                    ident=j(folder/'source-identities'/f'{case}-{q}-{arm}.json')
                    assert ident==rebuilt['rs' if arm=='rust' else 'zil']
            sources[case]=baseline['zil']
        else:
            ident=j(folder/(case+'-sources.json'))
            got={str(x.relative_to(p)):digest(readfile(x)) for x in sorted(p.rglob('*')) if x.is_file()}
            assert got==ident
            for path,h in ident.items():assert digest(git(f'examples/mainline/{case}/{path}'))==h
            sources[case]={path:h for path,h in ident.items() if path.endswith('.zil')}
            o=j(folder/('oracle-'+case)/'record.json');oracles[case]=o
            assert o['exit']==0 and o['timeout'] is False and o['oracle_valid'] is True
            native_outcomes(o['native'])
            assert o['native_arguments']==(['--check'] if case=='app' else [])
            oracle_dir=folder/('oracle-'+case)
            cmd=[manifest['binaries']['rust']['path'],str(p/'main.rs'),'--edition=2024','-C','overflow-checks=yes','-C','panic=abort','-C','opt-level=0','-o',str(oracle_dir/'program')]
            assert o['command']==cmd
            assert readfile(oracle_dir/'compile.stdout')==b''
            readfile(oracle_dir/'compile.stderr');assert readfile(oracle_dir/'program')[:4]==b'\x7fELF'
    groups=defaultdict(dict);fails=Counter();seen_objects=defaultdict(set);fps=defaultdict(set);source_lists=defaultdict(set);rowstats=Counter();cachepatterns=defaultdict(Counter)
    for row in rows:
        c,m,b,p,a,q=(row[k] for k in ('case','mode','block','position','arm','request'))
        run=folder/f'{c}-{m}-{b}-{p}-{a}';src=folder/('source-'+c)
        assert type(row['elapsed_ns']) is int and row['elapsed_ns']>0 and row['cpu_seconds']>=0 and row['max_rss_kib']>0
        assert row['timeout'] is False and 'launch_error' not in row
        stderr=readfile(run/(q+'.stderr'));stdout=readfile(run/(q+'.stdout'))
        if generated:
            e=EDITS[q];assert row['edit']==e and row['expected']==expected(cases[c],e)
            ident=get_generated(cases[c],e)['rs' if a=='rust' else 'zil']
            assert row['source_identity_file']==f'source-identities/{c}-{q}-{a}.json'
            assert row['source_identity_sha256']==digest(json.dumps(ident,sort_keys=True).encode())
        if a!='rust':
            cmd=[manifest['binaries'][a]['path'],'check' if m=='check' else 'build',str(src/'main.zil'),'--json']
            if m!='check':cmd+=['--opt',m,'--codegen-units','8','--codegen-jobs','2','--object-cache',str(run/'objects'),'-o',str(run/'program')]
            if closed and m!='check' and a=='zil':cmd+=['--closed-executable']
        else:
            cmd=[manifest['binaries']['rust']['path'],str(src/'main.rs'),'--edition=2024','-A','dead_code' if generated else 'warnings','-C','overflow-checks=yes','-C','panic=abort','-C','opt-level='+('2' if m=='O2' else '0'),'-C','codegen-units=8','-C','incremental='+str(run/'incremental'),'-o',str(run/'program')]
            if m=='check':cmd+=['--emit=metadata']
        assert row['command']==cmd
        valid=row['exit']==0
        if a!='rust':
            report=json.loads(stdout);checks['raw_zil_reports']+=1
            assert stderr==b'' and report['disk_cache']=='disabled'
            assert report['verification_runs']==row['verification_runs'] and report['native_codegen']==row['native_codegen']
            if generated:assert row['status']==report['status'] and row['queries']==report['query_events']
            actual_sources={str(Path(x).relative_to(src)) for x in report['source_files'].values()}
            assert actual_sources<=set(sources[c])
            if generated:assert actual_sources==set(sources[c])
            else:source_lists[c].add(tuple(report['source_files'].values()))
            fps[(c,EDITS[q] if generated else 0)].add(report['input_fingerprint'])
            if valid:
                assert report['status']=='Success' and report['verification_runs']==1 and report['diagnostics']==[]
                assert report['phase']==('verify' if m=='check' else 'link')
                assert report['query_events'].count({'phase':'verify','disposition':'executed'})==1
                rowstats['valid_current_verify']+=1
                if m=='check':assert report['native_codegen'] is None and report['commands']==[] and report['artifacts']=={}
                else:
                    cg=report['native_codegen'];oc=cg['object_cache'];objs=cg['object_reports'];units=cg['actual_units']
                    assert cg['backend']=='llvm-api' and cg['requested_units']==8 and 1<=units<=8 and len(objs)==units and cg['jobs']==min(2,units)
                    assert oc['enabled'] and oc['requested'] and oc['disabled_reason'] is None and oc['hits']+oc['misses']==units and oc['corrupt']==oc['write_failures']==0 and oc['stored']==oc['misses']
                    counts=Counter();seq=seen_objects[str(run)];keyset=set()
                    for obj in objs:
                        cache=obj['cache'];key=cache['action_key'];counts[cache['state']]+=1;keyset.add(key)
                        assert obj['backend']=='llvm-api' and obj['llvm_version']=='23.1.4' and obj['triple']=='x86_64-unknown-linux-gnu'
                        assert obj['object_bytes']>0 and cache['corrupt'] is False and obj['preflight_ns']>0
                        if cache['state']=='hit':
                            assert key in seq and obj['input_kind']=='object-cache' and obj['producer_verified_before_after'] is True and obj['llvm_verification_this_request'] is False
                            assert obj['preflight_ns']==cache['current_preflight_ns']>0 and all(obj[f]==0 for f in ['builder_ns','parse_ns','optimize_ns','emit_ns'])
                        else:
                            assert cache['state']=='miss' and obj['input_kind']=='builder' and obj['verify_before_after'] is True and obj['pipeline']==f'default<{m}>' and obj['parse_ns']==0
                        checks['object_reports']+=1
                    seq.update(keyset)
                    assert counts['hit']==oc['hits'] and counts['miss']==oc['misses']
                    assert oc['current_preflight_ns']==sum(o['cache']['current_preflight_ns'] for o in objs)
                    if q=='cold':assert oc['hits']==0
                    if q in ['unchanged','restore']:assert oc['hits']==units and oc['misses']==0
                    assert report['artifacts']=={'executable':str(run/'program'),'llvm':str(run/'program.ll')}
                    assert len(report['commands'])==1 and report['commands'][0][0]=='/workspace/.zil-env/llvm-23.1.4/bin/clang'
                    cachepatterns[(c,m,q)][(units,oc['hits'],oc['misses'])]+=1
                    rowstats['object_hits']+=oc['hits'];rowstats['object_misses']+=oc['misses']
            else:
                assert generated and a=='zil' and c in ['body-64x16','types-64x16']
                assert row['exit']==4 and report['status']=='ResourceLimit' and report['phase']=='expand' and report['verification_runs']==0 and report['native_codegen'] is None
                assert report['artifacts']=={} and report['commands']==[] and all(d['code']=='E-RESOURCE-LIMIT' for d in report['diagnostics']) and report['diagnostics']
                fails[(c,m,q,a,report['diagnostics'][0]['message'])]+=1
        else:
            assert row['exit']==0 and stdout==b''
        if valid and m!='check':
            n=row['native']
            if generated:
                assert n=={'exit':0,'stdout_hex':(expected(cases[c],EDITS[q])&0xffffffff).to_bytes(4,'little').hex(),'stderr_hex':''}
                rowstats['native_outcomes']+=1
            else:
                native_outcomes(n);assert n==oracles[c]['native']
                assert row['native_arguments']==(['--check'] if c=='app' and a=='rust' else [])
                rowstats['native_outcomes']+=15
            rowstats['native_builds']+=1
        else:assert 'native' not in row
        assert row['valid'] is valid
        rowstats['valid_rows' if valid else 'invalid_rows']+=1
        groups[(c,m,q)][(a,b)]=row
    assert len(groups)==want_cells and all(len(g)==2 for g in groups.values())
    assert all(len(v)==1 for v in fps.values()) and all(len(v)==1 for v in source_lists.values())
    # Hash retained final artifacts only; prior request native observations are retained in rows.
    for run in {folder/f"{r['case']}-{r['mode']}-{r['block']}-{r['position']}-{r['arm']}" for r in rows}:
        for p in run.iterdir():
            if p.is_file() and p.name in ['program','program.ll']:readfile(p)
    cells=[]
    for old in summary['cells']:
        key=tuple(old[k] for k in ('case','mode','request'));slots=groups[key];fails_count=0
        for arm in arms:
            row=slots[(arm,0)];assert old['arms'][arm]['median_ms']==row['elapsed_ns']/1e6
            assert old['arms'][arm]['values_ms']==[row['elapsed_ns']/1e6]
            failures=int(row['exit']!=0);assert old['arms'][arm]['failures']==failures;fails_count+=failures
        expected_ratio=None if fails_count else slots[('zil',0)]['elapsed_ns']/slots[('rust',0)]['elapsed_ns']
        near(expected_ratio,old['zil_over_rust'])
    all_windows[name]={'source':SHA,'rows':want_rows,'cell_count':want_cells,'counts':dict(rowstats),'oracle_builds':len(oracles),'oracle_native_outcomes':15*len(oracles),'failures':[{'case':k[0],'mode':k[1],'request':k[2],'arm':k[3],'reason':k[4],'count':v} for k,v in fails.items()]}
    print(name,dict(rowstats),'oracle',len(oracles),flush=True)
result={'pass':True,'windows':all_windows,'checks':dict(checks),'scope':'Final59d and intermediate05e sanity/source controls audited separately. Single-block timings are never pooled into formal windows.'}
(ROOT/'sanity-review-recomputed.json').write_text(json.dumps(result,indent=2)+'\n')
(ROOT/'sanity-review-inventory.json').write_text(json.dumps({'files':inventory,'count':len(inventory),'bytes':sum(v['bytes'] for v in inventory.values())},indent=2)+'\n')
print('PASS sanity windows',len(all_windows),'inventory',len(inventory),flush=True)
