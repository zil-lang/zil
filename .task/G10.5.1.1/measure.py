#!/usr/bin/env python3
"""Finite paired experiment. Preserve every observation; no timings reused from old runs."""
import argparse, hashlib, importlib.util, json, os, random, shutil, signal, statistics, struct, subprocess, time
from pathlib import Path
HERE=Path(__file__).resolve().parent
PROTOCOL=json.loads((HERE/'protocol.json').read_text())
REQUESTS={'cold':'base','warm':'base','private':'private','public':'public','restore':'base'}
MODES=('check','O0','O2')
ARMS=('base','cand')


def sha(path):
    with Path(path).open('rb') as f: return hashlib.file_digest(f,'sha256').hexdigest()


def dump(path,value):
    Path(path).write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n')


def sync(source,destination):
    destination.mkdir(parents=True,exist_ok=True)
    source=Path(source); destination=Path(destination)
    files={p.relative_to(source) for p in source.rglob('*') if p.is_file()}
    for p in list(destination.rglob('*')):
        if p.is_file() and p.relative_to(destination) not in files: p.unlink()
    for relative in files:
        src,dst=source/relative,destination/relative
        dst.parent.mkdir(parents=True,exist_ok=True)
        if not dst.exists() or src.read_bytes()!=dst.read_bytes(): shutil.copyfile(src,dst)


def text_section(path):
    data=Path(path).read_bytes()
    if data[:6]!=b'\x7fELF\x02\x01': raise ValueError('expected ELF64 little endian')
    offset=struct.unpack_from('<Q',data,40)[0]
    size,count,strings=struct.unpack_from('<HHH',data,58)
    headers=[struct.unpack_from('<IIQQQQIIQQ',data,offset+i*size) for i in range(count)]
    sh=headers[strings]; names=data[sh[4]:sh[4]+sh[5]]
    for sh in headers:
        end=names.index(0,sh[0])
        if names[sh[0]:end]==b'.text': return hashlib.sha256(data[sh[4]:sh[4]+sh[5]]).hexdigest()
    raise ValueError('missing text section')


class Experiment:
    def __init__(self,args):
        self.args=args; self.out=args.out.resolve(); self.out.mkdir(parents=True,exist_ok=False)
        self.rows=[]; self.runtime=[]; self.references=[]; self.rejections=[]; self.commands=0; self.native_checks=0
        self.cpus=sorted(os.sched_getaffinity(0))[:2]; os.sched_setaffinity(0,self.cpus)
        spec=importlib.util.spec_from_file_location('extended_corpus',HERE/'corpus.py')
        self.corpus=importlib.util.module_from_spec(spec); spec.loader.exec_module(self.corpus)
        self.cases=self.corpus.generate(self.out)
        self.clis={'base':args.base.resolve(),'cand':args.candidate.resolve()}
        self.rng=random.Random(20261109)
        self.manifest={'protocol':PROTOCOL,'cases':self.cases,'cpus':self.cpus,
          'cli':{a:{'path':str(p),'sha256':sha(p)} for a,p in self.clis.items()},
          'rustc':subprocess.check_output(['rustc','-vV'],text=True),
          'clang':{'path':os.environ['ZIL_CLANG'],'sha256':sha(os.environ['ZIL_CLANG'])},
          'source_hashes':{str(p.relative_to(self.out)):sha(p) for p in (self.out/'corpus').rglob('*') if p.is_file()},
          'timing':'spawn through wait4, opened input/output before clock; empty project caches not flushed OS page cache',
          'rss':'wait4 maximum individual process peak, not simultaneous process-tree sum',
          'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()}
        dump(self.out/'manifest.json',self.manifest)

    def execute(self,argv,kind,input_path=None,timeout=240):
        directory=self.out/'logs'/f'{self.commands:06d}'; self.commands+=1; directory.mkdir(parents=True)
        def alarm(_a,_b): raise TimeoutError('registered deadline')
        old=signal.signal(signal.SIGALRM,alarm)
        with open(input_path or os.devnull,'rb') as inp,(directory/'stdout').open('wb') as stdout,(directory/'stderr').open('wb') as stderr:
            start=time.perf_counter_ns(); timed_out=False
            p=subprocess.Popen(list(map(str,argv)),stdin=inp,stdout=stdout,stderr=stderr,start_new_session=True)
            try:
                signal.setitimer(signal.ITIMER_REAL,timeout)
                _,status,usage=os.wait4(p.pid,0)
            except TimeoutError:
                timed_out=True; os.killpg(p.pid,signal.SIGKILL); _,status,usage=os.wait4(p.pid,0)
            finally:
                signal.setitimer(signal.ITIMER_REAL,0); signal.signal(signal.SIGALRM,old)
            end=time.perf_counter_ns(); p.returncode=os.waitstatus_to_exitcode(status)
        row={'kind':kind,'argv':list(map(str,argv)),'exit':p.returncode,'timeout':timed_out,'wall_ms':(end-start)/1e6,
             'cpu_ms':(usage.ru_utime+usage.ru_stime)*1000,'rss_kib':usage.ru_maxrss,
             'log':str(directory.relative_to(self.out)),'stdout_sha256':sha(directory/'stdout'),'stderr_sha256':sha(directory/'stderr')}
        dump(directory/'process.json',row)
        return row,(directory/'stdout').read_bytes(),(directory/'stderr').read_bytes()

    def command(self,cli,case,mode,work,plain=False):
        argv=[cli,'check' if mode=='check' else 'build',work/'src/main.zil','--json','--semantic-cache',work/'semantic']
        if case['entry']: argv+=['--entry',case['entry']]
        if mode!='check':
            argv+=['--backend','llvm-api','--opt',mode,'--codegen-units','8','--codegen-jobs','2',
                   '--object-cache',work/'objects','--clang',os.environ['ZIL_CLANG'],'-o',work/'app']
            if mode=='O2':
                argv+=['--function-merging','after']
                if not plain: argv+=['--incremental-o2']
        return argv

    def validate_build(self,row,stdout,stderr):
        if row['exit'] or row['timeout']: raise RuntimeError(('compile failed',row,stderr[-2000:].decode(errors='replace')))
        report=json.loads(stdout)
        if report.get('status')!='Success' or report.get('verification_runs')!=1:
            raise RuntimeError(('missing actual successful Verify',row,report))
        return report

    def native(self,exe,case,snapshot):
        checks=[]
        for control in case['inputs']:
            row,stdout,stderr=self.execute([exe],'native-output',self.out/control['path'],60)
            expected=(self.out/control['expected'][snapshot]).read_bytes()
            valid=row['exit']==0 and stdout==expected and not stderr and not row['timeout']
            row.update(valid=valid,expected_sha256=hashlib.sha256(expected).hexdigest(),input_bytes=control['bytes'])
            checks.append(row); self.native_checks+=1
            if not valid: raise RuntimeError(('native mismatch',row,stdout[:200],stderr[:200]))
        return checks

    def histories(self):
        for block in [-1,*range(PROTOCOL['rounds'])]:
            cases=list(self.cases); self.rng.shuffle(cases)
            for case in cases:
                modes=list(MODES); self.rng.shuffle(modes)
                for mode in modes:
                    for request,snapshot in REQUESTS.items():
                        arms=list(ARMS); self.rng.shuffle(arms)
                        for arm in arms:
                            work=self.out/'work'/case['id']/mode/str(block)/arm
                            sync(self.out/case['root']/snapshot/'zil',work/'src')
                            row,stdout,stderr=self.execute(self.command(self.clis[arm],case,mode,work),'compile')
                            row.update(case=case['id'],mode=mode,request=request,snapshot=snapshot,arm=arm,block=block,warmup=block<0)
                            self.rows.append(row)
                            try:
                                report=self.validate_build(row,stdout,stderr)
                                row.update(valid=True,verification_runs=report['verification_runs'],native_codegen=report.get('native_codegen'))
                                if mode!='check':
                                    row['native_checks']=self.native(work/'app',case,snapshot)
                                    row['elf_bytes']=(work/'app').stat().st_size
                                    row['text_sha256']=text_section(work/'app')
                                    if block==0 and mode=='O2' and request in ('cold','private'):
                                        dest=self.out/'quality'/case['id']/snapshot/arm; dest.mkdir(parents=True)
                                        shutil.copyfile(work/'app',dest/'app'); (dest/'app').chmod(0o755)
                                        shutil.copyfile(work/'app.ll',dest/'app.ll')
                            finally:
                                with (self.out/'rows.jsonl').open('a') as f: f.write(json.dumps(row)+'\n')
                        if len(self.rows)%100==0: print(json.dumps({'compile_requests':len(self.rows),'native_checks':self.native_checks,'case':case['id'],'mode':mode,'block':block}),flush=True)
                    if not (block==0 and mode=='O0'):
                        shutil.rmtree(self.out/'work'/case['id']/mode/str(block))
        expected=len(self.cases)*len(MODES)*len(REQUESTS)*len(ARMS)*(PROTOCOL['rounds']+1)
        assert len(self.rows)==expected and all(row.get('valid') for row in self.rows)

    def rejection_controls(self):
        for case in self.cases:
            pair=[]
            for arm in ARMS:
                work=self.out/'work'/case['id']/'O0'/'0'/arm
                path=work/'src/main.zil'; original=path.read_text()
                path.write_text(original+'\nfn compiler_cost_bad()->i32 { return true; }\n')
                try:
                    row,stdout,stderr=self.execute(self.command(self.clis[arm],case,'check',work),'negative-control')
                    report=json.loads(stdout)
                    assert row['exit']!=0 and report.get('status')!='Success' and report.get('diagnostics')
                    row['diagnostics']=[(d.get('code'),d.get('phase')) for d in report['diagnostics']]
                    pair.append(row); self.rejections.append(row)
                finally: path.write_text(original)
                row,stdout,stderr=self.execute(self.command(self.clis[arm],case,'check',work),'rejection-restore')
                self.validate_build(row,stdout,stderr); self.rejections.append(row)
            assert pair[0]['diagnostics']==pair[1]['diagnostics'],pair
        dump(self.out/'rejections.json',self.rejections)

    def runtime_quality(self):
        existing=self.corpus.load_existing()
        for case in self.cases:
            if case['id'] not in ('tree32x8','tree256x8','tree1024x8'): continue
            n=case['modules_requested']; size={32:131072,256:16384,1024:4096}[n]
            data=bytes((i*73+19)%256 for i in range(size)); inp=self.out/'quality'/case['id']/'input.bin'; inp.write_bytes(data)
            for snapshot in ('base','private'):
                root=self.out/'quality'/case['id']/snapshot
                plain=root/'plain'; sync(self.out/case['root']/snapshot/'zil',plain/'src')
                row,stdout,stderr=self.execute(self.command(self.clis['base'],case,'O2',plain,plain=True),'ordinary-O2-reference')
                self.validate_build(row,stdout,stderr); row['native_checks']=self.native(plain/'app',case,snapshot); self.references.append(row)
                rust=root/'rust'; sync(self.out/case['root']/snapshot/'rust',rust/'src')
                row,stdout,stderr=self.execute(['rustc',rust/'src/main.rs','--edition=2024','--crate-name','compile_cost','-A','warnings',
                    '-C','opt-level=2','-C','debuginfo=0','-C','overflow-checks=yes','-C','panic=abort','-C','embed-bitcode=no','-o',rust/'app'],'rust-O2-reference')
                assert row['exit']==0 and not row['timeout'],(row,stderr)
                row['native_checks']=self.native(rust/'app',case,snapshot); self.references.append(row)
                expected=existing.tree_expected(n,8,snapshot,data)
                for repetition in range(-1,7):
                    arms=['base','cand','plain','rust']; self.rng.shuffle(arms)
                    for arm in arms:
                        exe=root/arm/'app'
                        row,stdout,stderr=self.execute([exe],'runtime',inp,120)
                        row.update(case=case['id'],snapshot=snapshot,arm=arm,repetition=repetition,warmup=repetition<0,
                                   valid=row['exit']==0 and stdout==expected and not stderr and not row['timeout'],
                                   elf_bytes=exe.stat().st_size,text_sha256=text_section(exe))
                        self.runtime.append(row)
                        with (self.out/'runtime.jsonl').open('a') as f: f.write(json.dumps(row)+'\n')
                        assert row['valid'],row
        dump(self.out/'reference-builds.json',self.references)

    def summarize(self):
        cells=[]; values={}; indexed={}; formal=[row for row in self.rows if not row['warmup']]
        for case in self.cases:
            for mode in MODES:
                for request in REQUESTS:
                    pair={}
                    for arm in ARMS:
                        rows=sorted((r for r in formal if (r['case'],r['mode'],r['request'],r['arm'])==(case['id'],mode,request,arm)),key=lambda r:r['block'])
                        assert len(rows)==PROTOCOL['rounds'] and [r['block'] for r in rows]==list(range(PROTOCOL['rounds']))
                        times=[r['wall_ms'] for r in rows]; pair[arm]=times
                        values[(case['id'],mode,request,arm)]=times
                        for row in rows: indexed[(case['id'],mode,request,arm,row['block'])]=row['wall_ms']
                    b,c=pair['base'],pair['cand']; bm,cm=statistics.mean(b),statistics.mean(c)
                    cells.append({'case':case['id'],'group':case['group'],'mode':mode,'request':request,'n':len(b),
                       'base_mean_ms':bm,'candidate_mean_ms':cm,'base_median_ms':statistics.median(b),'candidate_median_ms':statistics.median(c),
                       'saved_mean_ms':bm-cm,'mean_ratio':cm/bm,'paired_wins':sum(y<x for x,y in zip(b,c)),
                       'severe_tail':cm>2*bm and cm-bm>1000,'base_all_ms':b,'candidate_all_ms':c})
        dump(self.out/'cells.json',cells)
        profiles=[]; rng=random.Random(451109)
        for mode_name,mw in PROTOCOL['mode_weights'].items():
            for request_name,rw in PROTOCOL['request_weights'].items():
                totals={a:0.0 for a in ARMS}; medians={a:0.0 for a in ARMS}; pain={a:{'over_100ms':0.0,'over_1000ms':0.0} for a in ARMS}
                for case in self.cases:
                    for mode in MODES:
                        for request in REQUESTS:
                            weight=case['weight']*mw[mode]*rw[request]
                            for arm in ARMS:
                                v=values[(case['id'],mode,request,arm)]
                                totals[arm]+=weight*statistics.mean(v); medians[arm]+=weight*statistics.median(v)
                                for threshold in (100,1000): pain[arm][f'over_{threshold}ms']+=weight*statistics.mean(max(t-threshold,0) for t in v)
                boot=[]
                for _ in range(PROTOCOL['bootstrap_replicates']):
                    sums={a:0.0 for a in ARMS}
                    for case in self.cases:
                        for mode in MODES:
                            indices=[rng.randrange(PROTOCOL['rounds']) for _ in range(PROTOCOL['rounds'])]
                            for request in REQUESTS:
                                weight=case['weight']*mw[mode]*rw[request]
                                for arm in ARMS:
                                    v=values[(case['id'],mode,request,arm)]
                                    sums[arm]+=weight*sum(v[i] for i in indices)/len(indices)
                    boot.append(1-sums['cand']/sums['base'])
                boot.sort()
                profiles.append({'profile':mode_name+'/'+request_name,'weighted_mean_ms':totals,'weighted_median_ms':medians,
                    'reduction':1-totals['cand']/totals['base'],'saved_seconds_per_1000_requests':totals['base']-totals['cand'],
                    'excess_wait_ms':pain,'bootstrap_95_reduction':[boot[int(.025*len(boot))],boot[int(.975*len(boot))-1]],
                    'weights_are':'predeclared assumptions, not telemetry'})
        runtime=[]
        for name in ('tree32x8','tree256x8','tree1024x8'):
            for snapshot in ('base','private'):
                rows=[r for r in self.runtime if r['case']==name and r['snapshot']==snapshot and not r['warmup']]
                arm_values={a:[r['wall_ms'] for r in rows if r['arm']==a] for a in ('base','cand','plain','rust')}
                assert all(len(v)==7 for v in arm_values.values())
                med={a:statistics.median(v) for a,v in arm_values.items()}
                runtime.append({'case':name,'snapshot':snapshot,'median_ms':med,'candidate_over_base':med['cand']/med['base'],
                    'candidate_over_rust':med['cand']/med['rust'],'candidate_over_plain':med['cand']/med['plain'],
                    'screen_pass':med['cand']/med['base']<=PROTOCOL['runtime_screen_ratio'],
                    'all_values_ms':arm_values,
                    'candidate_plain_text_equal':next(r['text_sha256'] for r in rows if r['arm']=='cand')==next(r['text_sha256'] for r in rows if r['arm']=='plain')})
        result={'status':'Complete finite weighted matrix','compile_requests':len(self.rows),'formal_requests':len(formal),
            'native_output_checks':self.native_checks,'runtime_executions':len(self.runtime),'negative_and_restore_requests':len(self.rejections),
            'current_matrix_verify_runs':sum(r['verification_runs'] for r in self.rows),'profiles':profiles,'runtime_quality':runtime,
            'runtime_screen_pass':all(r['screen_pass'] for r in runtime),'severe_tails':[c for c in cells if c['severe_tail']],
            'independent_review':'NotRun','merge_allowed':False,'coverage':'finite declared cases; no industrial/full-language acceptance'}
        dump(self.out/'summary.json',result); print(json.dumps(result,ensure_ascii=False,indent=2),flush=True)


def main():
    parser=argparse.ArgumentParser(); parser.add_argument('--base',type=Path,required=True); parser.add_argument('--candidate',type=Path,required=True); parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args(); experiment=Experiment(args)
    try:
        experiment.histories(); experiment.rejection_controls(); experiment.runtime_quality(); experiment.summarize()
    except Exception as error:
        dump(experiment.out/'failure.json',{'status':'Fail','error':repr(error),'compile_requests':len(experiment.rows),'native_output_checks':experiment.native_checks,'commands':experiment.commands})
        raise
if __name__=='__main__': main()
