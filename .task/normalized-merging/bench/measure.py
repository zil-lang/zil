#!/usr/bin/env python3
from __future__ import annotations
import argparse,collections,hashlib,importlib.util,json,os,random,shutil,signal,statistics,subprocess,time
from pathlib import Path
HERE=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('corpus',HERE/'corpus.py'); corpus=importlib.util.module_from_spec(spec);spec.loader.exec_module(corpus)
MODES=('check','O0','O2')
REQUESTS=[('cold','base'),('warm','base'),('private','private'),('edit-warm','private'),('public','public'),('restore','base')]
PROFILES={'edit':{'cold':.1,'warm':.15,'private':.45,'edit-warm':.1,'public':.1,'restore':.1},'clean':{'cold':.7,'warm':.05,'private':.1,'edit-warm':.05,'public':.1,'restore':0}}
MODE_WEIGHTS={'interactive':{'check':.3,'O0':.5,'O2':.2},'release':{'check':.1,'O0':.2,'O2':.7}}
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def dump(p,v):Path(p).write_text(json.dumps(v,ensure_ascii=False,indent=2)+'\n')
def timed(argv,log,data=None,timeout=240):
    log.mkdir(parents=True)
    inp=log/'stdin';inp.write_bytes(data or b'')
    def alarm(*_):raise TimeoutError('deadline')
    previous=signal.signal(signal.SIGALRM,alarm);expired=False
    with inp.open('rb') as i,(log/'stdout').open('wb') as o,(log/'stderr').open('wb') as e:
        start=time.perf_counter_ns();p=subprocess.Popen(list(map(str,argv)),stdin=i,stdout=o,stderr=e,start_new_session=True)
        signal.setitimer(signal.ITIMER_REAL,timeout)
        try:_,status,use=os.wait4(p.pid,0)
        except TimeoutError:
            expired=True;os.killpg(p.pid,signal.SIGKILL);_,status,use=os.wait4(p.pid,0)
        finally:stop=time.perf_counter_ns();signal.setitimer(signal.ITIMER_REAL,0);signal.signal(signal.SIGALRM,previous)
        p.returncode=os.waitstatus_to_exitcode(status)
    row={'argv':list(map(str,argv)),'wall_ms':(stop-start)/1e6,'cpu_ms':1000*(use.ru_utime+use.ru_stime),'rss_kib':use.ru_maxrss,'exit':p.returncode,'timeout':expired,'log':str(log)}
    return row,(log/'stdout').read_bytes(),(log/'stderr').read_bytes()
def text_hash(exe):
    p=Path(str(exe)+'.text');subprocess.run(['objcopy','--dump-section','.text='+str(p),str(exe)],check=True,capture_output=True)
    result=sha(p);p.unlink();return result

def expected(case,snapshot,data):
    x,d,hi,lo=data;d=d%12+1
    for i in range(hi*256+lo):
        y=corpus.run_model(case['id'],case['n'],(x+i)%65521,d,snapshot!='base')
        if snapshot=='public':y=(y+3)%65521
        x=(y+i)%65521
    return bytes([x%256,x//256])
def source_text(root,case,snapshot,arm):
    lang='rust' if arm=='rust' else 'zil';ext='rs' if arm=='rust' else 'zil'
    s=(root/case['id']/('private' if snapshot=='public' else snapshot)/lang/('main.'+ext)).read_text()
    if snapshot=='public':
        before='fn run(x:i64,d:i32)->i64';assert s.count(before)==1
        s=s.replace(before,'fn run(x:i64,d:i32,extra:i64)->i64')
        before='run((acc+i)%65521,(depth%12+1) as i32)' if arm=='rust' else 'run((acc+i)%65521,i32(depth%12+1))'
        assert s.count(before)==1;s=s.replace(before,before[:-1]+',3)')
        if case['group']=='trait':
            before='return (y+i64::from(d))%65521;' if arm=='rust' else 'return (y+i64(d))%65521;'
            assert s.count(before)==1;s=s.replace(before,before.replace(')%65521', '+extra)%65521'))
        else:
            before='return y; }';assert s.count(before)==1;s=s.replace(before,'return (y+extra)%65521; }')
    return s

def main():
    ap=argparse.ArgumentParser();ap.add_argument('--base',type=Path,required=True);ap.add_argument('--candidate',type=Path,required=True);ap.add_argument('--out',type=Path,required=True);ap.add_argument('--rounds',type=int,default=5)
    a=ap.parse_args();out=a.out.resolve();out.mkdir(parents=True);cpus=sorted(os.sched_getaffinity(0))[:2];assert len(cpus)==2;os.sched_setaffinity(0,cpus)
    tools={'base':a.base.resolve(),'cand':a.candidate.resolve(),'rust':Path(subprocess.check_output(['rustup','which','rustc'],text=True).strip())}
    cases=corpus.generate(out/'corpus');rows=[];runtime=[];failures=[];equivalence=[]
    manifest={'baseline':'099d0af969a51260355437603b77d3f9030d83bc','candidate':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'cases':cases,'tools':{k:sha(v) for k,v in tools.items()},'cpus':cpus,'rounds':a.rounds,'request_weights':PROFILES,'mode_weights':MODE_WEIGHTS,'weights':'predeclared assumptions, not telemetry','upstream_original_crates':'separate unpaired lane, excluded from paired aggregate; no substitution claimed','boundary':'fresh process through link; project caches retained only within one history; OS caches retained; no Cargo Fresh in paired rows','source_hashes':{str(p.relative_to(out)):sha(p) for p in (out/'corpus').rglob('*') if p.is_file()}}
    dump(out/'manifest.json',manifest)
    count=0
    def command(arm,mode,work):
        ext='rs' if arm=='rust' else 'zil';exe=work/'app';source=work/('main.'+ext)
        if arm=='rust':return [tools[arm],source,'--edition=2024','--crate-name','expanded_cost','-A','warnings','-C','opt-level=2','-C','debuginfo=0','-C','overflow-checks=yes','-C','panic=abort','-C','embed-bitcode=no','-C','incremental='+str(work/'incremental'),'-o',exe]
        argv=[tools[arm],'check' if mode=='check' else 'build',source,'--json','--semantic-cache',work/'semantic']
        if mode!='check':
            argv+=['--opt',mode,'--backend','llvm-api','--clang',os.environ['ZIL_CLANG'],'--codegen-units','8','--codegen-jobs','2','--object-cache',work/'objects','-o',exe]
            if mode=='O2':argv+=['--incremental-o2','--function-merging','before-and-after' if arm=='cand' else 'after']
        return argv
    try:
        for case in cases:
            case_failed=False
            for mode in MODES:
                if case_failed:break
                arms=['base','cand','rust'] if mode=='O2' and case['group']=='resource' else ['base','cand']
                for block in range(-1,a.rounds):
                    if case_failed:break
                    for request,snapshot in REQUESTS:
                        if case_failed:break
                        order=arms[:];random.Random(f'{case["id"]}/{mode}/{block}/{request}').shuffle(order)
                        pair={}
                        for arm in order:
                            work=out/'work'/case['id']/mode/str(block)/arm;work.mkdir(parents=True,exist_ok=True)
                            p=work/('main.rs' if arm=='rust' else 'main.zil');s=source_text(out/'corpus',case,snapshot,arm)
                            if not p.exists() or p.read_text()!=s:p.write_text(s)
                            (work/'app').unlink(missing_ok=True);Path(str(work/'app')+'.ll').unlink(missing_ok=True)
                            row,stdout,stderr=timed(command(arm,mode,work),out/'logs'/str(count));count+=1
                            row.update(case=case['id'],group=case['group'],mode=mode,request=request,snapshot=snapshot,block=block,warmup=block<0,arm=arm,valid=False)
                            valid=row['exit']==0 and not row['timeout'];report={}
                            if arm!='rust':
                                try:
                                    report=json.loads(stdout);valid=valid and report['status']=='Success' and report['verification_runs']==1
                                    row['verification_runs']=report.get('verification_runs');row['native_codegen']=report.get('native_codegen')
                                except Exception as error:row['report_error']=repr(error);valid=False
                            if valid and mode!='check':
                                exe=work/'app';row['text_sha256']=text_hash(exe);row['exe_bytes']=exe.stat().st_size;row['native_checks']=[]
                                for data in (bytes([1,0,0,1]),bytes([29,2,0,2]),bytes([251,5,0,3])):
                                    run=subprocess.run([str(exe)],input=data,capture_output=True,timeout=30)
                                    want=expected(case,snapshot,data);ok=run.returncode==0 and run.stdout==want and not run.stderr
                                    row['native_checks'].append({'input':data.hex(),'expected':want.hex(),'actual':run.stdout.hex(),'exit':run.returncode,'pass':ok});valid=valid and ok
                                if block==0 and mode=='O2' and request in ('cold','private'):
                                    dst=out/'quality'/case['id']/snapshot/arm;dst.mkdir(parents=True,exist_ok=True);shutil.copy2(exe,dst/'app')
                            row['valid']=valid;rows.append(row);dump(Path(row['log'])/'record.json',row)
                            with (out/'rows.jsonl').open('a') as f:f.write(json.dumps(row)+'\n')
                            pair[arm]=row
                            if not valid:
                                failures.append({'case':case['id'],'mode':mode,'arm':arm,'request':request,'row':row,'stdout':stdout[:24000].decode(errors='replace'),'stderr':stderr[:4000].decode(errors='replace')});case_failed=True
                        if all(k in pair and pair[k]['valid'] for k in ('base','cand')) and mode!='check':
                            eq=pair['base']['text_sha256']==pair['cand']['text_sha256']
                            equivalence.append({'case':case['id'],'mode':mode,'request':request,'block':block,'text_equal':eq})
                            if not eq and mode!='O2':raise AssertionError('unaffected O0 machine code changed')
                    if not (block==0 and mode=='O2'):shutil.rmtree(out/'work'/case['id']/mode/str(block),ignore_errors=True)
                    print(json.dumps({'case':case['id'],'mode':mode,'block':block,'requests':len(rows),'failed':case_failed}),flush=True)
        # Code equality is checked for every native pair. Runtime measurement uses
        # one small fixed I/O envelope and a compute-heavy loop, not per-item I/O.
        if not failures:
            for case in cases:
                for snapshot in ('base','private'):
                    root=out/'quality'/case['id']/snapshot
                    iterations=1
                    while True:
                        data=bytes([73,3,iterations//256,iterations%256]);t=time.perf_counter();pr=subprocess.run([str(root/'base/app')],input=data,capture_output=True,timeout=30);elapsed=time.perf_counter()-t
                        assert pr.returncode==0
                        if (elapsed>=.12 and case['group']!='resource') or iterations>=65535:break
                        iterations=min(65535,iterations*4)
                    data=bytes([73,3,iterations//256,iterations%256])
                    # Separate reference computation is outside every native timer.
                    want=expected(case,snapshot,data)
                    for rep in range(-1,5):
                        arms=(['base','cand','rust'] if case['group']=='resource' else ['base','cand']);random.Random(f'{case["id"]}/{snapshot}/{rep}').shuffle(arms)
                        for arm in arms:
                            row,stdout,stderr=timed([root/arm/'app'],out/'runtime-logs'/str(len(runtime)),data,60)
                            row.update(case=case['id'],snapshot=snapshot,arm=arm,repetition=rep,warmup=rep<0,iterations=iterations,valid=row['exit']==0 and stdout==want and not stderr)
                            runtime.append(row)
                            with (out/'runtime.jsonl').open('a') as f:f.write(json.dumps(row)+'\n')
                            assert row['valid'],row
    finally:
        dump(out/'failures.json',failures);dump(out/'equivalence.json',equivalence)
        cells=[];v={}
        for case in cases:
            for mode in MODES:
                for request,_ in REQUESTS:
                    cell={'case':case['id'],'mode':mode,'request':request}
                    for arm in ('base','cand','rust'):
                        rr=[r for r in rows if (r['case'],r['mode'],r['request'],r['arm'])==(case['id'],mode,request,arm) and not r['warmup'] and r['valid']]
                        if rr:
                            x=[r['wall_ms'] for r in rr];v[(case['id'],mode,request,arm)]=x;cell[arm]={'n':len(x),'mean_ms':statistics.mean(x),'median_ms':statistics.median(x),'values_ms':x}
                    cells.append(cell)
        dump(out/'cells.json',cells)
        complete=not failures and all(len(v.get((c['id'],m,r,arm),[]))==a.rounds for c in cases for m in MODES for r,_ in REQUESTS for arm in ('base','cand'))
        profiles=[]
        if complete:
            for mn,mw in MODE_WEIGHTS.items():
                for rn,rw in PROFILES.items():
                    sums={arm:sum(c['weight']*mw[m]*rw[r]*statistics.mean(v[(c['id'],m,r,arm)]) for c in cases for m in MODES for r,_ in REQUESTS) for arm in ('base','cand')}
                    profiles.append({'profile':mn+'/'+rn,'mean_ms':sums,'reduction':1-sums['cand']/sums['base'],'seconds_saved_per_1000':sums['base']-sums['cand']})
        result={'status':'Complete finite paired matrix' if complete else 'Incomplete; missing or failed cells not renormalized','compile_requests':len(rows),'formal_requests':sum(not r['warmup'] for r in rows),'native_output_checks':sum(len(r.get('native_checks',[])) for r in rows),'text_pairs':len(equivalence),'text_pairs_equal':all(r['text_equal'] for r in equivalence),'runtime_requests':len(runtime),'profiles':profiles,'failures':len(failures),'independent_review':'NotRun','merge_allowed':False}
        dump(out/'summary.json',result);print(json.dumps(result,indent=2),flush=True)
    if failures:raise SystemExit(1)
if __name__=='__main__':main()
