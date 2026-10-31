#!/usr/bin/env python3
"""Stage6 paired-source CLI screen with explicit per-arm options; preserve every outcome."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import signal
import shutil
import statistics
import subprocess
import time

ROOT = Path(__file__).resolve().parents[3]
REQUESTS = [('cold', 0), ('unchanged', 0), ('early_edit', 1), ('late_edit', 2), ('restore', 0)]

def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()

def shapes():
    out = []
    for size in [128, 512, 2048, 4096, 8192]:
        out.append(dict(name=f'large-{size}', sizes=[size], flavor='scalar'))
    for modules in [8, 64, 512, 1024]:
        out.append(dict(name=f'even-{modules}x{4096//modules}', sizes=[4096//modules]*modules, flavor='scalar'))
    out.append(dict(name='skew-8', sizes=[2048,1024,512,256,128,64,32,32], flavor='scalar'))
    for flavor in ['branch', 'generic', 'ct', 'body', 'types']:
        out.append(dict(name=f'{flavor}-64x16', sizes=[16]*64, flavor=flavor))
    out.append(dict(name='body-16x16', sizes=[16]*16, flavor='body'))
    out.append(dict(name='chain-64x16', sizes=[16]*64, flavor='scalar', graph='chain'))
    out.append(dict(name='fanout-64x16', sizes=[16]*64, flavor='scalar', graph='fanout'))
    out.append(dict(name='large-even-8x1024', sizes=[1024]*8, flavor='scalar'))
    out.append(dict(name='large-even-4x2048', sizes=[2048]*4, flavor='scalar'))
    out.append(dict(name='large-skew-8', sizes=[4096,2048,1024,512,256,128,64,64], flavor='scalar'))
    out.append(dict(name='types-16x16', sizes=[16]*16, flavor='types'))
    out.append(dict(name='types-32x16', sizes=[16]*32, flavor='types'))
    return out

def generate(case, out, edit):
    out.mkdir(parents=True, exist_ok=True)
    flavor=case['flavor']; sizes=case['sizes']; single=len(sizes)==1; graph=case.get('graph','star')
    for arm in ['zil','rust']:
        ext='zil' if arm=='zil' else 'rs'; root=[]
        if not single:
            root=[f'import m{i}\n' if arm=='zil' else f'mod m{i};\n' for i in range(len(sizes))]
        functions=0
        for i,size in enumerate(sizes):
            code=[]
            if flavor in ['generic','types']:
                code.append('fn ident<T>(x:T)->T where T:Copy {return x;}\n' if arm=='zil' else 'fn ident<T:Copy>(x:T)->T{return x;}\n')
            for j in range(size):
                if flavor=='types':code.append((f'struct S{j}{{v:i32}}\n' if arm=='zil' else f'#[derive(Clone,Copy)]struct S{j}{{v:i32}}\n'))
                delta=int(edit==1 and i==0 and j==0)+int(edit==2 and i==len(sizes)-1 and j==size-1)*2
                val=(j%7)+1+delta
                if flavor=='body':
                    body=f'var y:i32=x;\n' if arm=='zil' else 'let mut y:i32=x;\n'
                    body+=''.join('y=y+1;\n' for _ in range(64))+f'return y+{val};'
                elif flavor=='branch':body=f'if x<3 {{return x+{val};}} else {{return x*2+{val};}}'
                elif flavor=='types':body=f'return ident(S{j}{{v:x}}).v+{val};'
                elif flavor=='generic':body=f'return ident(x)+{val};'
                elif flavor=='ct':body=f'return x+'+(f'#({val}*2-1);' if arm=='zil' else f'const {{{val}*2-1}};')
                else:body=f'return x+{val};'
                code.append(f'fn h{j}(x:i32)->i32{{{body}}}\n');functions+=1
            var='var' if arm=='zil' else 'let mut'
            code.append(f'pub fn value()->i32{{{var} total:i32=0;\n')
            code.extend(f'total=total+h{j}({j%5});\n' for j in range(size))
            dependency=i+1 if graph=='chain' and i+1<len(sizes) else 0 if graph=='fanout' and i>0 else None
            if dependency is not None:
                if arm=='zil':code.insert(0,f'import m{dependency}\n')
                code.append(f'total=total+m{dependency}.value();\n' if arm=='zil' else f'total=total+crate::m{dependency}::value();\n')
            code.append('return total;}\n')
            if single:root+=code
            else:(out/f'm{i}.{ext}').write_text(''.join(code))
        if not single:
            var='var' if arm=='zil' else 'let mut';sep='.' if arm=='zil' else '::'
            root.append(f'fn value()->i32{{{var} total:i32=0;\n')
            root.extend(f'total=total+m{i}{sep}value();\n' for i in (range(1) if graph=='chain' else range(len(sizes))));root.append('return total;}\n')
        expected=0
        for i,size in enumerate(sizes):
            for j in range(size):
                val=j%7+1+int(edit==1 and i==0 and j==0)+int(edit==2 and i==len(sizes)-1 and j==size-1)*2;x=j%5
                weight=len(sizes) if graph=='fanout' and i==0 else 1
                expected+=weight*(x+64+val if flavor=='body' else (x if x<3 else x*2)+val if flavor=='branch' else x+val*2-1 if flavor=='ct' else x+val)
        body='let result=value();unsafe{'+''.join(f'putchar((result>>{shift})&255);' for shift in [0,8,16,24])+'}'
        root.append('extern \"C\" fn putchar(v:i32)->i32;\nfn main()->i32{'+body+'return 0;}\n' if arm=='zil' else 'unsafe extern \"C\"{fn putchar(v:i32)->i32;}\nfn main(){'+body+'}\n')
        (out/f'main.{ext}').write_text(''.join(root))
    return expected

def measured(cmd, run, label, timeout):
    def expired(_signum, _frame):raise TimeoutError('compiler timeout')
    signal.signal(signal.SIGALRM,expired)
    with (run/f'{label}.stdout').open('wb') as stdout,(run/f'{label}.stderr').open('wb') as stderr:
        start=time.perf_counter_ns()
        try:p=subprocess.Popen(cmd,stdout=stdout,stderr=stderr,start_new_session=True)
        except OSError as error:
            return {'elapsed_ns':time.perf_counter_ns()-start,'exit':None,'timeout':False,'launch_error':str(error),'cpu_seconds':None,'max_rss_kib':None}
        signal.alarm(timeout)
        timed_out=False
        try:_,status,usage=os.wait4(p.pid,0)
        except TimeoutError:
            timed_out=True;os.killpg(p.pid,signal.SIGKILL);_,status,usage=os.wait4(p.pid,0)
        finally:signal.alarm(0)
        wall=time.perf_counter_ns()-start;p.returncode=os.waitstatus_to_exitcode(status)
    return {'elapsed_ns':wall,'exit':p.returncode,'timeout':timed_out,'cpu_seconds':usage.ru_utime+usage.ru_stime,'max_rss_kib':usage.ru_maxrss}

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--zil-arg',action='append',default=[]);p.add_argument('--reference-arg',action='append',default=[]);p.add_argument('--reference-zil',type=Path);p.add_argument('--zil',type=Path,required=True);p.add_argument('--rustc',type=Path,required=True);p.add_argument('--out',type=Path,required=True)
    p.add_argument('--blocks',type=int,default=5);p.add_argument('--case',action='append');p.add_argument('--mode',action='append',choices=['check','O0','O2'])
    p.add_argument('--closed-executable',action='store_true');p.add_argument('--source-cache',action='store_true');p.add_argument('--smoke',action='store_true');p.add_argument('--timeout',type=int,default=180)
    a=p.parse_args();a.out.mkdir(parents=True,exist_ok=False);cpus=sorted(os.sched_getaffinity(0))[:2];os.sched_setaffinity(0,cpus)
    shutil.copy2(Path(__file__),a.out/'runner.py')
    cases=[c for c in shapes() if not a.case or c['name'] in a.case];assert cases
    reference='baseline' if a.reference_zil else 'rust'
    modes=a.mode or ['check','O0','O2'];requests=REQUESTS[:1] if a.smoke else REQUESTS
    manifest={'source':subprocess.check_output(['git','rev-parse','HEAD'],text=True,cwd=ROOT).strip(),'tool_sha256':sha(Path(__file__)),'binaries':{n:{'path':str(v.resolve()),'sha256':sha(v)} for n,v in [('zil',a.zil),(reference,a.reference_zil or a.rustc)]},'cases':cases,'blocks':a.blocks,'modes':modes,'requests':requests,'cpus':cpus,'source_cache':a.source_cache,'zil_args':a.zil_arg,'reference_args':a.reference_arg,'closed_executable':a.closed_executable,'scope':__doc__,'flags':'O0/O2, overflow checks, panic abort, 8 CGU, ZIL jobs2; rustc incremental and ZIL object cache persist per sequence; optional unverified ZIL source cache'}
    (a.out/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n');rows=[]
    for c in cases:
        src=a.out/('source-'+c['name']);generate(c,src,0)
        metrics={ext:{'files':len(list(src.glob('*.'+ext))),'bytes':sum(x.stat().st_size for x in src.glob('*.'+ext)),'max_file_bytes':max(x.stat().st_size for x in src.glob('*.'+ext)),'lines':sum(len(x.read_text().splitlines()) for x in src.glob('*.'+ext))} for ext in ['zil','rs']}
        (src/'metrics.json').write_text(json.dumps(metrics,indent=2)+'\n')
        for mode in modes:
            for block in range(a.blocks):
                for position,arm in enumerate(['zil',reference] if block%2==0 else [reference,'zil']):
                    run=a.out/f'{c["name"]}-{mode}-{block}-{position}-{arm}';run.mkdir()
                    for label,edit in requests:
                        expected=generate(c,src,edit)
                        source_hashes={x.name:sha(x) for x in sorted(src.glob('*.zil' if arm!='rust' else '*.rs'))}
                        identity=hashlib.sha256(json.dumps(source_hashes,sort_keys=True).encode()).hexdigest()
                        identities=a.out/'source-identities';identities.mkdir(exist_ok=True)
                        identity_file=identities/f'{c["name"]}-{label}-{arm}.json'
                        if identity_file.exists():assert json.loads(identity_file.read_text())==source_hashes
                        else:identity_file.write_text(json.dumps(source_hashes,indent=2)+'\n')
                        if arm!='rust':
                            cmd=[str((a.zil if arm=='zil' else a.reference_zil).resolve()),'check' if mode=='check' else 'build',str(src/'main.zil'),'--json']
                            if mode!='check':cmd+=['--opt',mode,'--codegen-units','8','--codegen-jobs','2','--object-cache',str(run/'objects'),'-o',str(run/'program')]
                            if mode!='check' and arm=='zil' and a.closed_executable:cmd+=['--closed-executable']
                            if a.source_cache:cmd+=['--cache-dir',str(run/'source-cache')]
                            cmd+=(a.zil_arg if arm=='zil' else a.reference_arg)
                        else:
                            cmd=[str(a.rustc.resolve()),str(src/'main.rs'),'--edition=2024','-A','dead_code','-C','overflow-checks=yes','-C','panic=abort','-C','opt-level='+('2' if mode=='O2' else '0'),'-C','codegen-units=8','-C','incremental='+str(run/'incremental'),'-o',str(run/'program')]
                            if mode=='check':cmd+=['--emit=metadata']
                            cmd+=a.reference_arg
                        (run/'program').unlink(missing_ok=True)
                        result=measured(cmd,run,label,a.timeout)
                        row={'case':c['name'],'mode':mode,'block':block,'position':position,'arm':arm,'request':label,'command':cmd,'edit':edit,'expected':expected,'source_identity_sha256':identity,'source_identity_file':str(identity_file.relative_to(a.out)),**result}
                        row['valid']=result['exit']==0 and not result['timeout']
                        if arm!='rust' and not result['timeout']:
                            try:
                                report=json.loads((run/f'{label}.stdout').read_text());row['status']=report['status'];row['verification_runs']=report['verification_runs'];row['native_codegen']=report['native_codegen'];row['queries']=report.get('query_events')
                                row['valid'] &= report['status']=='Success' and report['verification_runs']==1
                            except (ValueError, KeyError, TypeError) as error:
                                row['valid']=False;row['validation_error']=str(error)
                        if row['valid'] and mode!='check':
                            try:
                                n=subprocess.run([str(run/'program')],capture_output=True,timeout=20)
                                row['native']={'exit':n.returncode,'stdout_hex':n.stdout.hex(),'stderr_hex':n.stderr.hex()}
                                row['valid'] &= n.returncode==0 and n.stdout==(expected & 0xffffffff).to_bytes(4,'little') and not n.stderr
                            except OSError as error:
                                row['valid']=False;row['native']={'launch_error':str(error)}
                            except subprocess.TimeoutExpired as error:
                                row['valid']=False;row['native']={'timeout':True,'stdout_hex':(error.stdout or b'').hex(),'stderr_hex':(error.stderr or b'').hex()}
                        rows.append(row)
                        with (a.out/'rows.jsonl').open('a') as f:f.write(json.dumps(row)+'\n')
                        if not row['valid']:print('FAIL',c['name'],mode,arm,label,result['exit'],flush=True)
                print(c['name'],mode,'block',block+1,'/',a.blocks,flush=True)
    summary=[]
    for c in cases:
        for mode in modes:
            for label,_ in requests:
                s=[r for r in rows if (r['case'],r['mode'],r['request'])==(c['name'],mode,label)];arms={}
                for arm in ['zil',reference]:
                    rs=[r for r in s if r['arm']==arm];arms[arm]={'median_ms':statistics.median(r['elapsed_ns']/1e6 for r in rs),'max_rss_kib':statistics.median(r['max_rss_kib'] for r in rs if r['max_rss_kib'] is not None) if any(r['max_rss_kib'] is not None for r in rs) else None,'failures':sum(not r['valid'] for r in rs),'values_ms':[r['elapsed_ns']/1e6 for r in rs]}
                summary.append({'case':c['name'],'mode':mode,'request':label,'arms':arms,'zil_over_'+reference:arms['zil']['median_ms']/arms[reference]['median_ms'] if not any(x['failures'] for x in arms.values()) else None})
    (a.out/'summary.json').write_text(json.dumps({'rows':len(rows),'cells':summary},indent=2)+'\n');print('complete',len(rows),flush=True)
if __name__=='__main__':main()
