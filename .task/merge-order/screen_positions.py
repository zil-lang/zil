#!/usr/bin/env python3
"""Focused algorithm screen, not the original weighted acceptance matrix."""
from pathlib import Path
import argparse,importlib.util,json,os,random,shutil,statistics,subprocess,time
HERE=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('multi',HERE/'multimodule.py');multi=importlib.util.module_from_spec(spec);spec.loader.exec_module(multi);m=multi.m;c=m.corpus
CASES=['dispatch16','pipeline16','owned32','unequal16']
ARMS=['base','raw','normalize','before-inline','after-inline','after-cleanup','default-expanded']
def files(case,snapshot):
    if case=='owned32':return {'main.zil':c.resources(32,'zil',snapshot=='private')+c.io_source('zil')}
    result=multi.files('pipeline16' if case=='pipeline16' else 'dispatch16',16,'zil',snapshot)
    if case=='unequal16':
        for i in range(16):
            s=result[f'worker{i}.zil'];assert s.count('return y; }')==1
            result[f'worker{i}.zil']=s.replace('return y; }',f'return (y+{i})%65521; }}')
    return result
def expected(case,snapshot,data):
    if case=='owned32':return c.expected(case,32,snapshot,data)
    if case!='unequal16':return multi.expected(case,16,snapshot,data)
    x,depth,hi,lo=data;d=depth%12+1
    for k in range(hi*256+lo):
        y=(x+k)%65521;i=y%16
        y=(c.run_model('owned4',4,y,d,snapshot=='private' and i==0)+i)%65521
        x=(y+k)%65521
    return bytes([x%256,x//256])
def main():
    p=argparse.ArgumentParser();p.add_argument('--cli',type=Path,required=True);p.add_argument('--out',type=Path,required=True);a=p.parse_args();out=a.out.resolve();out.mkdir(parents=True)
    os.sched_setaffinity(0,sorted(os.sched_getaffinity(0))[:2]);cli=a.cli.resolve();rows=[];runtime=[];failures=[]
    for case in CASES:
        for snapshot in ['base','private']:
            directory=out/'corpus'/case/snapshot;directory.mkdir(parents=True)
            for name,s in files(case,snapshot).items():(directory/name).write_text(s)
    m.dump(out/'manifest.json',{'cases':CASES,'arms':ARMS,'formal_rounds':3,'warmup_rounds':1,'snapshot_history':['base','private'],'tools':{'cli_sha256':m.sha(cli)},'cpus':sorted(os.sched_getaffinity(0)),'source_hashes':{str(p.relative_to(out)):m.sha(p) for p in (out/'corpus').rglob('*') if p.is_file()},'scope':'focused current-main O2 pipeline placement; equal handlers plus explicitly unequal holdout; excluded from old weighted aggregate'})
    try:
        for case in CASES:
            for block in range(-1,3):
                for snapshot in ['base','private']:
                    order=ARMS[:];random.Random(f'{case}/{block}/{snapshot}').shuffle(order)
                    for arm in order:
                        work=out/'work'/case/str(block)/arm;work.mkdir(parents=True,exist_ok=True)
                        for name,text in files(case,snapshot).items():
                            path=work/name
                            if not path.exists() or path.read_text()!=text:path.write_text(text)
                        os.environ['ZIL_RESEARCH_PASS_POSITION']=arm if arm!='base' else 'raw'
                        exe=work/'app';argv=[cli,'build',work/'main.zil','--json','--opt','O2','--backend','llvm-api','--clang',os.environ['ZIL_CLANG'],'--codegen-units','8','--codegen-jobs','2','--incremental-o2','--function-merging','after' if arm=='base' else 'before-and-after','--semantic-cache',work/'semantic','--object-cache',work/'objects','-o',exe]
                        r,stdout,stderr=m.timed(argv,out/'logs'/str(len(rows)),timeout=180)
                        r.update(case=case,snapshot=snapshot,block=block,arm=arm,warmup=block<0,valid=False,env={'ZIL_RESEARCH_PASS_POSITION':os.environ['ZIL_RESEARCH_PASS_POSITION']},checks=[])
                        assert r['exit']==0 and not r['timeout'],(r,stdout,stderr)
                        report=json.loads(stdout);assert report['status']=='Success' and report['verification_runs']==1,report
                        r['report']=report
                        for data in [bytes([i,3,0,1]) for i in range(16)]+[bytes([73,11,0,2])]:
                            v=subprocess.run([str(exe)],input=data,capture_output=True,timeout=30);want=expected(case,snapshot,data)
                            assert v.returncode==0 and v.stdout==want and not v.stderr,(case,arm,snapshot,data.hex(),v.stdout.hex(),want.hex(),v.stderr)
                            r['checks'].append({'input':data.hex(),'output':v.stdout.hex(),'expected':want.hex()})
                        r['text_sha256']=m.text_hash(exe);r['valid']=True;rows.append(r);(out/'rows.jsonl').open('a').write(json.dumps(r)+'\n')
                        if block==0:
                            q=out/'quality'/case/snapshot/arm;q.mkdir(parents=True);shutil.copy2(exe,q/'app')
                    print(json.dumps({'case':case,'block':block,'snapshot':snapshot,'requests':len(rows)}),flush=True)
                shutil.rmtree(out/'work'/case/str(block))
        for case in CASES:
            for snapshot in ['base','private']:
                root=out/'quality'/case/snapshot;iterations=16384
                for depth in [0,11]:
                    data=bytes([73,depth,iterations//256,iterations%256]);want=expected(case,snapshot,data)
                    for rep in range(-1,7):
                        order=ARMS[:];random.Random(f'{case}/{snapshot}/{depth}/{rep}').shuffle(order)
                        for arm in order:
                            r,stdout,stderr=m.timed([root/arm/'app'],out/'runtime-logs'/str(len(runtime)),data,timeout=60)
                            r.update(case=case,snapshot=snapshot,depth=depth,repetition=rep,arm=arm,valid=r['exit']==0 and stdout==want and not stderr,iterations=iterations)
                            runtime.append(r);(out/'runtime.jsonl').open('a').write(json.dumps(r)+'\n');assert r['valid'],r
    except Exception as e:
        failures.append(repr(e));raise
    finally:
        cells=[];quality=[]
        for case in CASES:
            for snapshot in ['base','private']:
                cell={'case':case,'snapshot':snapshot}
                for arm in ARMS:
                    v=[r['wall_ms'] for r in rows if r['case']==case and r['snapshot']==snapshot and r['arm']==arm and not r['warmup'] and r['valid']]
                    cell[arm]={'n':len(v),'mean_ms':statistics.mean(v) if v else None,'values_ms':v}
                cells.append(cell)
                for depth in [0,11]:
                    q={'case':case,'snapshot':snapshot,'depth':depth,'arms':{}}
                    for arm in ARMS:
                        v=[r['wall_ms'] for r in runtime if (r['case'],r['snapshot'],r['depth'],r['arm'])==(case,snapshot,depth,arm) and r['repetition']>=0]
                        q['arms'][arm]={'n':len(v),'median_ms':statistics.median(v) if v else None,'values_ms':v}
                    if all(v['n']==7 for v in q['arms'].values()):
                        for arm in ARMS:q['arms'][arm]['ratio_to_base']=q['arms'][arm]['median_ms']/q['arms']['base']['median_ms']
                    quality.append(q)
        m.dump(out/'summary.json',{'complete':not failures and len(rows)==224 and len(runtime)==896,'compile_requests':len(rows),'formal_requests':sum(not r['warmup'] for r in rows),'output_checks':sum(len(r['checks']) for r in rows),'runtime_requests':len(runtime),'cells':cells,'quality':quality,'failures':failures,'acceptance':'Screen only; no automatic main integration or regression waiver'})
if __name__=='__main__':main()
