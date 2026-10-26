#!/usr/bin/env python3
"""Bind finite fact graphs to actual ZIL closure modes, Verify and native runs.
This generates a restricted source family; it is NOT a general Core extractor.
"""
import argparse, hashlib, json, os, pathlib, random, re, subprocess

def sha(p):return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
def main():
    p=argparse.ArgumentParser();p.add_argument('--driver',required=True);p.add_argument('--out',required=True);p.add_argument('--clang',default='clang');a=p.parse_args()
    out=pathlib.Path(a.out).resolve();out.mkdir(parents=True,exist_ok=False);(out/'scenarios.txt').write_text('cold\n')
    rng=random.Random(131);families=[]
    # (caller, callee, actual-index-for-formal0, actual-index-for-formal1)
    cycle=[(0,1,1,0),(1,0,1,0),(2,1,0,1),(3,2,0,1)]
    for seeds in [[],[2],[],[0,2],[0],[]]:families.append((cycle,seeds))
    aliases=[(0,1,0,0),(1,2,1,0),(2,0,0,1),(3,1,1,1)]
    for seeds in [[5],[],[0,7],[2],[]]:families.append((aliases,seeds))
    for _ in range(9):
        calls=[(i,j,rng.randrange(2),rng.randrange(2)) for i in range(4) for j in range(4) if rng.random()<.23]
        seeds=[i for i in range(8) if rng.random()<.16];families.append((calls,seeds))
    records=[];rows=[];native_count=0
    for family,(calls,seeds) in enumerate(families):
        edges=set()
        for caller,callee,x,y in calls:
            edges.add((callee*2,caller*2+x));edges.add((callee*2+1,caller*2+y))
        active=set(seeds)
        while True:
            next_=active|{v for u,v in edges if u in active}
            if next_==active:break
            active=next_
        prefix='resource Token{n:i32}\n'
        for f in range(4):
            body=''.join(f'f{g}({["a","b"][x]},{["a","b"][y]},depth-1);' for caller,g,x,y in calls if caller==f)
            writes=''.join(f'{["a","b"][j]}.n+=1;' for j in range(2) if f*2+j in seeds)
            prefix+=f'fn f{f}(a:Token,b:Token,depth:i32){{if depth>0{{{body}}}{writes}}}\n'
        def evaluate(f,slots,depth,values):
            if depth>0:
                for caller,g,x,y in calls:
                    if caller==f:evaluate(g,[slots[x],slots[y]],depth-1,values)
            for j in range(2):
                if f*2+j in seeds:values[slots[j]]+=1
        observed=[]
        for v in range(8):
            f,j=divmod(v,2);args=['owned','local'] if j==0 else ['local','owned']
            values=[0,0];evaluate(f,[0,1] if j==0 else [1,0],2,values)
            body=f'let owned=Token{{n:0}};var c=move ()=>{{let local=Token{{n:0}};f{f}({args[0]},{args[1]},2);return owned.n}};'
            source=prefix+f'fn main()->i32{{{body}return c()-{values[0]};}}\n'
            directory=out/f'case-{family:02}-{v}';directory.mkdir();(directory/'main.zil').write_text(source)
            command=[a.driver,str(directory/'build'),'build-emit',str(directory/'main.zil'),str(out),'64']
            r=subprocess.run(command,env={**os.environ,'ZIL_CLANG':a.clang},capture_output=True,timeout=30)
            (directory/'compiler.stdout').write_bytes(r.stdout);(directory/'compiler.stderr').write_bytes(r.stderr)
            wrapper=json.loads((directory/'build/report.json').read_text());report=wrapper['records'][0]['report']
            assert r.returncode==0 and wrapper['complete'] and report['verification_runs']==1,(family,v,report)
            text=(directory/'build/cold-dumps/typed.txt').read_text();kinds=set(re.findall(r'\bkind: (FnMut|FnOnce|Fn),',text));want='FnMut' if v in active else 'Fn'
            assert kinds=={want},(family,v,kinds,want)
            native=subprocess.run([str(directory/'build/cold')],capture_output=True,timeout=5);native_count+=1
            assert native.returncode==0 and not native.stdout and not native.stderr,(family,v,values,native)
            observed.append(int(want=='FnMut'))
            records.append({'family':family,'port':v,'mode':want,'source_sha256':sha(directory/'main.zil'),'status':report['status'],'verification_runs':report['verification_runs'],'native_exit':native.returncode,'compiler_source':wrapper['production_identity']})
        # Public Fn bound is a separate nearest negative/positive source observation.
        v=next(iter(sorted(active)),0);f,j=divmod(v,2);args=['owned','local'] if j==0 else ['local','owned']
        required='fn demand<F:Fn()->i32>(f:&F)->i32{return f()}\n'
        source=prefix+required+f'fn main()->i32{{let owned=Token{{n:0}};let c=move ()=>{{let local=Token{{n:0}};f{f}({args[0]},{args[1]},0);return owned.n}};return demand(&c)}}\n'
        path=out/f'bound-{family:02}.zil';path.write_text(source)
        r=subprocess.run([a.driver,str(out/f'bound-{family:02}'),'verify-emit',str(path),str(out),'64'],env={**os.environ,'ZIL_CLANG':a.clang},capture_output=True,timeout=30)
        report=json.loads((out/f'bound-{family:02}/report.json').read_text())['records'][0]['report']
        if active: assert report['status']=='SafeReject' and any(d['code']=='E-CLOSURE-MODE' for d in report['diagnostics']),(family,report)
        else: assert report['status']=='Success' and report['verification_runs']==1,(family,report)
        records.append({'family':family,'kind':'Fn-bound','port':v,'source_sha256':sha(path),'status':report['status'],'diagnostics':report['diagnostics']})
        rows.append(f'{family}\t8\t'+ ';'.join(f'{u},{v}' for u,v in sorted(edges))+'\t'+','.join(map(str,sorted(seeds)))+'\t'+''.join(map(str,observed)))
    (out/'source-cases.tsv').write_text('\n'.join(rows)+'\n')
    receipt={'status':'Pass','scope':'restricted real-source port graph anchors on archived compiler, not a production incremental implementation','families':len(families),'native_executions':native_count,'compiler_requests':len(records),'compiler_sha256':sha(a.driver),'compiler_source':'ff0ddb7e59b2b0577c35b92e74b895c86f20b218','tsv_sha256':sha(out/'source-cases.tsv'),'records':records,'production_eligible':False}
    (out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps({k:v for k,v in receipt.items() if k!='records'},indent=2))
if __name__=='__main__':main()
