#!/usr/bin/env python3
"""One fixed rejection screen; not a replacement for the weighted acceptance suite."""
from pathlib import Path
import hashlib, importlib.util, itertools, json, os, random, shutil, statistics, subprocess

ROOT = Path('/workspace/scratch/beabdcf5ebf4')
OUT = Path('/workspace/scratch/abf690ce880c/out/resumption/no-inline-screen')
PROTO = ROOT/'research-composition/.task'
def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec); spec.loader.exec_module(value); return value
m = module('primary', PROTO/'normalized-merging/bench/measure.py')
h = module('holdout', PROTO/'normalized-consumers/holdout.py')
def sha(path): return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def save(path, value): path.write_text(json.dumps(value, indent=2, ensure_ascii=False)+'\n')
def append(path, value):
    with path.open('a') as f: f.write(json.dumps(value)+'\n')

TOOLS = {
    'base': ROOT/'target-resource/release/zil',
    'old': OUT.parent/'zil-normalized-old',
    'cand': ROOT/'target-normalized/release/zil',
}
ARMS = list(TOOLS)
prefix = 'function(sroa,early-cse),ipsccp,globalopt,function(mem2reg,instcombine<max-iterations=1;no-verify-fixpoint>,simplifycfg),'
PIPE = {'base':'default<O2>|merge-functions=after|aliases=false',
        'old':prefix+'cgscc(inline),mergefunc,default<O2>|merge-functions=after|aliases=false',
        'cand':prefix+'mergefunc,default<O2>|merge-functions=after|aliases=false'}

def main():
    OUT.mkdir(parents=True, exist_ok=False)
    cpus = sorted(os.sched_getaffinity(0))[:2]; os.sched_setaffinity(0, cpus)
    primary = ROOT/'evidence/normalized-acceptance/primary'
    holdout = ROOT/'evidence/normalized-acceptance/holdout'
    cases = {c['id']:c for c in json.loads((primary/'manifest.json').read_text())['cases']}
    cells = []
    for lane, source, names in [('primary',primary,['owned8','shared4096','ring768']),('holdout',holdout,['dispatch16'])]:
        prior = [json.loads(s) for s in (source/'runtime.jsonl').read_text().splitlines()]
        for case, snapshot in itertools.product(names,['base','private']):
            row = next(r for r in prior if r['case']==case and r['snapshot']==snapshot and r['arm']=='base' and r['repetition']==0 and (lane=='primary' or r['depth']==0))
            log = Path(row['log'])
            cells.append({'lane':lane,'case':case,'snapshot':snapshot,'input':(log/'stdin').read_bytes().hex(),
                          'expected':(log/'stdout').read_bytes().hex(),'prior_record':str(log),'iterations':row['iterations']})
    sources = {}
    for cell in cells:
        case,snapshot=cell['case'],cell['snapshot']
        texts = h.files(case,snapshot) if cell['lane']=='holdout' else {'main.zil':m.source_text(primary/'corpus',cases[case],snapshot,'base')}
        for name,text in texts.items():
            p=OUT/'corpus'/case/snapshot/name; p.parent.mkdir(parents=True,exist_ok=True);p.write_text(text)
            original=(holdout if cell['lane']=='holdout' else primary)/'corpus'/case/snapshot/('' if cell['lane']=='holdout' else 'zil')/name
            assert p.read_bytes()==original.read_bytes(), (p,original)
            sources[str(p.relative_to(OUT))]=sha(p)
    schedule=[]
    for rep in range(-2,15):
        order=list(range(len(cells)));random.Random(f'fixed-no-inline-cells/{rep}').shuffle(order)
        for index in order:
            arms=ARMS[:];random.Random(f'fixed-no-inline-arms/{rep}/{index}').shuffle(arms)
            for arm in arms: schedule.append({'repetition':rep,'cell':index,'arm':arm})
    dirty=subprocess.check_output(['git','-C',str(ROOT/'normalized'),'diff','--binary'])
    (OUT/'candidate.patch').write_bytes(dirty)
    plan={'purpose':'Fixed old-counterexample screen; any new runtime regression blocks full acceptance. No retry or threshold change.',
          'source_head':subprocess.check_output(['git','-C',str(ROOT/'normalized'),'rev-parse','HEAD'],text=True).strip(),
          'candidate_patch_sha256':sha(OUT/'candidate.patch'),'binary_hashes':{a:sha(p) for a,p in TOOLS.items()},
          'cpus':cpus,'runtime_cpus':[cpus[0]],'compile_history':['base','private'],'compile_rounds':1,
          'compile_scope':'Diagnostic single history, not statistically accepted compile improvement',
          'runtime_formal_rounds':15,'runtime_warmups':2,'runtime_regression_limit':1.1,'source_hashes':sources,
          'long_oracle':'Exact input and independent-oracle output inherited from retained old run; current short cases also checked independently',
          'cells':cells,'schedule':schedule,'pipeline':PIPE,'weighted_suite_replaced':False}
    save(OUT/'plan.json',plan)
    print(json.dumps({'plan_sha256':sha(OUT/'plan.json'),'cells':len(cells),'runtime_samples':len(schedule)}),flush=True)
    builds=[]; runtimes=[]
    for case in ['owned8','shared4096','ring768','dispatch16']:
        for snapshot in ['base','private']:
            cell=next(c for c in cells if c['case']==case and c['snapshot']==snapshot)
            order=ARMS[:];random.Random(f'fixed-build/{case}/{snapshot}').shuffle(order)
            for arm in order:
                work=OUT/'work'/case/arm;work.mkdir(parents=True,exist_ok=True)
                for src in (OUT/'corpus'/case/snapshot).iterdir():
                    dst=work/src.name
                    if not dst.exists() or dst.read_bytes()!=src.read_bytes():shutil.copy2(src,dst)
                exe=work/'app'
                command=[TOOLS[arm],'build',work/'main.zil','--json','--opt','O2','--backend','llvm-api','--clang',os.environ['ZIL_CLANG'],
                         '--codegen-units','8','--codegen-jobs','2','--incremental-o2','--function-merging','after' if arm=='base' else 'before-and-after',
                         '--semantic-cache',work/'semantic','--object-cache',work/'objects','-o',exe]
                r,stdout,stderr=m.timed(command,OUT/'build-logs'/str(len(builds)),timeout=180)
                assert r['exit']==0 and not r['timeout'],(r,stderr)
                report=json.loads(stdout);assert report['status']=='Success' and report['verification_runs']==1
                for obj in report['native_codegen']['object_reports']:
                    assert obj['pipeline']==PIPE[arm]
                    if obj['input_kind']=='builder':assert obj['verify_before_after'] and obj['parse_ns']==0
                    else:assert obj['input_kind']=='object-cache'
                checks=[]
                for data in [bytes([1,0,0,1]),bytes([29,2,0,2]),bytes([251,5,0,3])]:
                    p=subprocess.run([str(exe)],input=data,capture_output=True,timeout=30)
                    want=h.expected(case,snapshot,data) if cell['lane']=='holdout' else m.expected(cases[case],snapshot,data)
                    assert p.returncode==0 and p.stdout==want and not p.stderr,(case,snapshot,arm,data,p)
                    checks.append({'input':data.hex(),'expected':want.hex(),'actual':p.stdout.hex(),'exit':p.returncode})
                dest=OUT/'quality'/case/snapshot/arm/'app';dest.parent.mkdir(parents=True);shutil.copy2(exe,dest)
                r.update(case=case,snapshot=snapshot,arm=arm,report=report,checks=checks,sha256=sha(exe),text_sha256=m.text_hash(exe))
                builds.append(r);append(OUT/'builds.jsonl',r)
            print(json.dumps({'built':case,'snapshot':snapshot,'count':len(builds)}),flush=True)
    os.sched_setaffinity(0,[cpus[0]])
    for item in schedule:
        cell=cells[item['cell']];exe=OUT/'quality'/cell['case']/cell['snapshot']/item['arm']/'app'
        r,stdout,stderr=m.timed([exe],OUT/'runtime-logs'/str(len(runtimes)),bytes.fromhex(cell['input']),timeout=60)
        assert r['exit']==0 and not r['timeout'] and stdout.hex()==cell['expected'] and not stderr,(cell,item,r,stdout,stderr)
        r.update(item,case=cell['case'],snapshot=cell['snapshot'],input=cell['input'],output=stdout.hex(),valid=True)
        runtimes.append(r);append(OUT/'runtime.jsonl',r)
        if len(runtimes)%24==0:print(json.dumps({'runtime_samples':len(runtimes)}),flush=True)
    result=[]
    for i,cell in enumerate(cells):
        row={'case':cell['case'],'snapshot':cell['snapshot'],'arms':{}}
        for arm in ARMS:
            vals=[r['wall_ms'] for r in runtimes if r['cell']==i and r['arm']==arm and r['repetition']>=0]
            row['arms'][arm]={'n':len(vals),'median_ms':statistics.median(vals),'values_ms':vals}
        row['candidate_to_base']=row['arms']['cand']['median_ms']/row['arms']['base']['median_ms']
        row['candidate_to_old']=row['arms']['cand']['median_ms']/row['arms']['old']['median_ms']
        row['within_original_regression_gate']=row['candidate_to_base']<=1.1;result.append(row)
    summary={'complete':len(builds)==24 and len(runtimes)==408,'builds':len(builds),'runtime_samples':len(runtimes),'cells':result,
             'counterexample_screen_pass':all(r['within_original_regression_gate'] for r in result),'merge_allowed':False,
             'scope':'Single predeclared rejection screen; full weighted acceptance still required even on screen pass'}
    save(OUT/'summary.json',summary);print(json.dumps(summary,indent=2),flush=True)

if __name__=='__main__':main()
