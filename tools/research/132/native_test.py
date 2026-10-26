#!/usr/bin/env python3
"""Actual .zil -> archived production compiler -> LLVM -> native, never model-only."""
import argparse, concurrent.futures, hashlib, json, os, pathlib, random, shutil, struct, subprocess

def digest(path): return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()
def run(argv, **kw): return subprocess.run(list(map(str,argv)),capture_output=True,timeout=30,**kw)
def main():
    a=argparse.ArgumentParser();a.add_argument('--driver',required=True);a.add_argument('--clang',default='clang');a.add_argument('--out',required=True);args=a.parse_args()
    root=pathlib.Path(__file__).resolve().parents[3];out=pathlib.Path(args.out).resolve();out.mkdir(parents=True,exist_ok=False)
    src=out/'source';shutil.copytree(root/'examples/mainline/owned_text',src)
    (out/'scenarios.txt').write_text('cold\n');env={**os.environ,'ZIL_CLANG':str(args.clang)}
    commands=[]
    def command(argv,**kw):
        r=run(argv,**kw);commands.append({'argv':list(map(str,argv)),'exit_code':r.returncode,'stdout':r.stdout.decode(errors='replace'),'stderr':r.stderr.decode(errors='replace')});return r
    r=command([args.driver,out/'build','build-emit',src/'main.zil',out,'64'],env=env)
    report=json.loads((out/'build/report.json').read_text());assert r.returncode==0 and report['complete'],commands[-1]
    assert report['records'][0]['report']['verification_runs']==1
    versions={'clang':command([args.clang,'--version']).stdout.decode()}
    ir=out/'build/cold.ll';executables={'O0':out/'build/cold'}
    for opt in ['O0','O2']:
        if opt=='O2':
            p=out/'native-O2';assert command([args.clang,'-Wno-override-module','-x','ir','-O2',ir,'-o',p]).returncode==0;executables[opt]=p
        p=out/f'probe-{opt}'
        assert command([args.clang,'-Wno-override-module',f'-{opt}',ir,root/'tools/research/132/allocator_probe.c','-Wl,--wrap=realloc','-Wl,--wrap=free','-o',p]).returncode==0
        executables[f'probe-{opt}']=p
    cases=[]
    def add(old,insert,begin,end,limit=65000,label=''):
        if isinstance(old,str): old=old.encode()
        if isinstance(insert,str): insert=insert.encode()
        cases.append(dict(old=old.hex(),insert=insert.hex(),begin=begin,end=end,limit=limit,label=label))
    for text in ['', 'abc', 'a中🙂z', '\x00a\x00', 'é\u0301🧑\u200d💻', '\u007f\u0080\u07ff\u0800\ud7ff\ue000\uffff\U00010000\U0010ffff']:
        for i in range(len(text)+1):
            for j in range(i,len(text)+1):
                for inserted in ['', '中🙂', '\x00']: add(text,inserted,i,j,label='scalar-boundaries')
    bad=[bytes([b]) for b in range(128,256)]+[bytes.fromhex(s) for s in ['c080','c1bf','e08080','eda080','edbfbf','f0808080','f4908080','f5808080','e282','e228a1','f09f92','f0288cbc','c2','c220','fe','ff']]
    for b in range(128): add(bytes([b]),b'',0,1,label='ASCII-byte')
    for raw in bad:
        add(raw,'',0,0,label='invalid-source');add('a',raw,0,1,label='invalid-insert');add(b'a'+raw,'x',0,1,label='invalid-tail')
    for x in [(-1,0),(0,-1),(2,1),(4,4),(0,4)]: add('a中b','x',*x,label='invalid-range')
    for limit in [-1,0,1,3,4,5,10]: add('a中🙂z','é',1,3,limit,label='limit')
    rng=random.Random(132)
    alphabet=['a','中','🙂','é','\x00','\u0301','\U0010ffff','\ue000','\n']
    for i in range(500):
        text=''.join(rng.choices(alphabet,k=rng.randrange(25)));insert=''.join(rng.choices(alphabet,k=rng.randrange(8)))
        begin=rng.randrange(len(text)+1);end=rng.randrange(begin,len(text)+1)
        add(text,insert,begin,end,rng.choice([0,8,64,65000]),label='random-valid')
    for i in range(300):
        raw=bytes(rng.randrange(256) for _ in range(rng.randrange(20))); ins=bytes(rng.randrange(256) for _ in range(rng.randrange(7)))
        add(raw,ins,rng.randrange(-1,5),rng.randrange(-1,5),rng.randrange(-1,20),label='random-bytes')
    def expected(c):
        old=bytes.fromhex(c['old']);ins=bytes.fromhex(c['insert']);a=c['begin'];b=c['end'];lim=c['limit']
        try: text=old.decode('utf8')
        except UnicodeDecodeError as e: return 1,e.start,-1,old
        try: new=ins.decode('utf8')
        except UnicodeDecodeError as e: return 2,-1,e.start,old
        if a<0 or b<a or b>len(text): return 3,-1,-1,old
        result=(text[:a]+new+text[b:]).encode()
        if lim<0 or len(result)>lim:return 4,-1,-1,old
        return 0,-1,-1,result
    def check(job):
        name,index,c=job;old=bytes.fromhex(c['old']);ins=bytes.fromhex(c['insert']);frame=struct.pack('<5H',len(old),len(ins),c['begin']&65535,c['end']&65535,c['limit']&65535)+old+ins
        status,e1,e2,value=expected(c);want=bytes([status])+struct.pack('<3H',e1&65535,e2&65535,len(value))+value
        r=run([executables[name]],input=frame)
        assert r.returncode==0 and r.stdout==want,dict(executable=name,case=c,exit=r.returncode,expected=want.hex(),actual=r.stdout.hex(),stderr=r.stderr.decode(errors='replace'))
        memory=None
        if name.startswith('probe-'):
            if r.stderr: memory=json.loads(r.stderr)
            if memory: assert memory['live']==0 and memory['allocated']==memory['released'],memory
            else: assert not old and not ins and not value
        return {'mode':name,'case':index,'exit_code':r.returncode,'output_sha256':hashlib.sha256(r.stdout).hexdigest(),'allocator':memory}
    jobs=[(name,index,c) for name in executables for index,c in enumerate(cases)]
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool: records=list(pool.map(check,jobs))
    faults=[]
    old=b'abc';ins=b'XY';frame=struct.pack('<5H',3,2,1,2,100)+old+ins
    for name in ['probe-O0','probe-O2']:
        for stop in [1,2,3,4]:
            r=run([executables[name]],input=frame,env={**env,'ZIL_TEST_FAIL_AT':str(stop)})
            m=json.loads(r.stderr);assert m['live']==0 and m['allocated']==m['released'],m
            if stop<=2: assert r.returncode==91,(name,stop,r.returncode,m)
            else:
                value=old if stop==3 else b'aXYc';code=5 if stop==3 else 0
                want=bytes([code])+struct.pack('<3H',65535,65535,len(value))+value
                assert r.returncode==0 and r.stdout==want,(name,stop,r.returncode,r.stdout.hex(),want.hex(),m)
            faults.append({'mode':name,'stop':stop,'exit':r.returncode,'output':r.stdout.hex(),'allocator':m})
    # Actual nearest negative: use old window AFTER publishing a replacement.
    negative=out/'negative';shutil.copytree(src,negative)
    (negative/'main.zil').write_text('import edit\npackage State{manages data:ByteBuffer}\nfn main()->i32{var s=State{data:bytes_new()};let old=bytes_view(s.data);let result=edit.replace(old,0,0,old,10);let prepared=edit.take(result);s.data=give prepared;return i32(old.len())}\n')
    r=command([args.driver,out/'rejected','build-emit',negative/'main.zil',out,'64'],env=env)
    rejection=json.loads((out/'rejected/report.json').read_text())['records'][0]['report']
    assert r.returncode!=0 and rejection['status']=='SafeReject',rejection
    assert any(d['code']=='E-BORROW-CONFLICT' for d in rejection['diagnostics']),rejection
    assert not (out/'rejected/cold').exists()
    result={'format':'zil-132-native/1','status':'Pass','source_snapshot':{p.name:digest(p) for p in src.glob('*.zil')},'compiler_sha256':digest(args.driver),'compiler_source':report['production_identity'],'recovered_main_source':'ea782c91007cf356bf99f3c51d55482c38e134ee','scope':'ordinary-library native tests on frozen 123 baseline, NOT a rebuild of latest draft24','case_count':len(cases),'native_executions':len(records),'allocator_fault_executions':len(faults),'negative':rejection,'tools':versions,'latest_workspace':'NotRun','formal_gates':'NotRun','independent_review':'NotRun'}
    for name,data in [('cases.json',cases),('native-records.json',records),('faults.json',faults),('commands.json',commands),('receipt.json',result)]: (out/name).write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n')
    print(json.dumps(result,ensure_ascii=False,indent=2))
if __name__=='__main__':main()
