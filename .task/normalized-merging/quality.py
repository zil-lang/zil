#!/usr/bin/env python3
"""Finite extended runtime guard; not a replacement for weighted compilation."""
import argparse,functools,hashlib,json,os,random,statistics,subprocess,sys,time
from pathlib import Path
sys.path.insert(0,str(Path(__file__).parent/'bench'))
import corpus

def main():
    p=argparse.ArgumentParser();p.add_argument('--base',required=True);p.add_argument('--candidate',required=True);p.add_argument('--out',required=True);a=p.parse_args()
    root=Path(a.out).resolve();root.mkdir(parents=True,exist_ok=False);log=root/'logs';log.mkdir()
    os.sched_setaffinity(0,sorted(os.sched_getaffinity(0))[:2]);records=[];cells=[];builds=[]
    def run(cmd,label,data=None):
        started=time.perf_counter();r=subprocess.run(cmd,input=data,capture_output=True,timeout=180)
        row={'label':label,'argv':cmd,'ms':(time.perf_counter()-started)*1000,'exit':r.returncode,'input':None if data is None else data.hex(),'stdout':r.stdout.hex(),'stderr':r.stderr.decode(errors='replace')}
        records.append(row);(log/(str(len(records))+'.json')).write_text(json.dumps(row,indent=2));return r,row
    @functools.lru_cache(None)
    def inner(n,x,d,edit):return corpus.run_model('owned'+str(n),n,x,d,edit)
    def expected(n,edit,data):
        x,depth,hi,lo=data;d=depth%12+1
        for i in range(hi*256+lo):x=(inner(n,((x+i)%65521)%251,d,edit)+i)%65521
        return bytes((x%256,x//256))
    for n in (8,16,24,32,64):
        for snapshot in ('base','private'):
            edit=snapshot=='private';programs={}
            for arm,compiler in [('base',a.base),('cand',a.candidate),('rust','rustc')]:
                d=root/f'owned{n}'/snapshot/arm;d.mkdir(parents=True);exe=d/'app';lang='rust' if arm=='rust' else 'zil';source=d/('main.rs' if lang=='rust' else 'main.zil');source.write_text(corpus.resources(n,lang,edit)+corpus.io_source(lang))
                cmd=([compiler,str(source),'--edition=2024','-C','opt-level=2','-C','overflow-checks=yes','-C','debuginfo=0','-C','incremental='+str(d/'incremental'),'-o',str(exe)] if arm=='rust' else [compiler,'build',str(source),'-o',str(exe),'--json','--opt','O2','--codegen-units','8','--codegen-jobs','2','--incremental-o2','--function-merging','before-and-after' if arm=='cand' else 'after'])
                r,row=run(cmd,f'{n}/{snapshot}/{arm}/build');report=None if arm=='rust' else json.loads(r.stdout)
                valid=r.returncode==0 and (arm=='rust' or (report['status']=='Success' and report['verification_runs']==1))
                builds.append({'n':n,'snapshot':snapshot,'arm':arm,'valid':valid,'report':report,'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest()})
                if not valid:
                    (root/'summary.json').write_text(json.dumps({'status':'Incomplete','builds':builds,'records':records},indent=2));raise RuntimeError('runtime input did not compile; retained failure')
                programs[arm]=str(exe)
            for depth in (0,5,11):
                data=bytes((29,depth,255,255));want=expected(n,edit,data);vals={k:[] for k in programs}
                for repeat in range(-1,9):
                    arms=list(programs);random.Random(n*1000+depth*10+repeat).shuffle(arms)
                    for arm in arms:
                        r,row=run([programs[arm]],f'{n}/{snapshot}/depth{depth}/{repeat}/{arm}',data)
                        if r.returncode or r.stdout!=want:raise RuntimeError(('output mismatch',row,want.hex()))
                        if repeat>=0:vals[arm].append(row['ms'])
                med={k:statistics.median(v) for k,v in vals.items()};ratio=med['cand']/med['base'];rust_throughput=med['rust']/med['cand']
                cells.append({'n':n,'snapshot':snapshot,'depth':depth,'median_ms':med,'values_ms':vals,'candidate_latency_ratio':ratio,'rust_throughput_ratio':rust_throughput,'relative_gate_pass':ratio<=1.10,'rust90_pass':rust_throughput>=.90})
                print(json.dumps(cells[-1]),flush=True)
    summary={'status':'Complete finite runtime guard','compile_requests':len(builds),'runtime_requests':len(records)-len(builds),'runtime_gate_pass':all(c['relative_gate_pass'] and c['rust90_pass'] for c in cells),'cells':cells,'builds':builds,'scope':'mechanism samples only; no general runtime or aggregate Rust conclusion'}
    (root/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');(root/'rows.json').write_text(json.dumps(records,indent=2)+'\n');print(json.dumps({k:v for k,v in summary.items() if k not in ('cells','builds')}))
if __name__=='__main__':main()
