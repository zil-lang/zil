#!/usr/bin/env python3
"""Compare real example entry points against an immutable previous CLI.
Finite source/product differential, not a proof and not a performance experiment.
"""
import argparse,hashlib,json,os,subprocess,time
from pathlib import Path
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def main():
    ap=argparse.ArgumentParser(description=__doc__)
    for n in ('repo','baseline','candidate','clang','out'):ap.add_argument('--'+n,type=Path,required=True)
    a=ap.parse_args();a.out.mkdir(parents=True,exist_ok=False)
    sources=sorted((a.repo/'examples').rglob('main.zil'))
    state={'status':'Running','baseline_sha256':sha(a.baseline),'candidate_sha256':sha(a.candidate),
           'kind':'finite real-source differential; inputs frozen before run','sources':[str(s.relative_to(a.repo)) for s in sources],
           'configurations':[['O0',1],['O0',8],['O2',1],['O2',8]],'inputs_hex':['','33','00','4142430a'],
           'builds':[],'runs':[],'mismatches':[],'timeouts':[]}
    (a.out/'PLAN.json').write_text(json.dumps(state,indent=2))
    def save():(a.out/'summary.json').write_text(json.dumps(state,indent=2))
    for case,source in enumerate(sources):
        for opt,units in state['configurations']:
            results=[]
            for tag,cli in [('baseline',a.baseline),('candidate',a.candidate)]:
                d=a.out/f'{case}-{opt}-{units}-{tag}';d.mkdir()
                exe=d/'program'
                argv=[str(cli),'build',str(source),'-o',str(exe),'--opt',opt,'--codegen-units',str(units),'--codegen-jobs','4','--clang',str(a.clang),'--json']
                t=time.perf_counter()
                try:r=subprocess.run(argv,capture_output=True,timeout=90)
                except subprocess.TimeoutExpired:
                    state['timeouts'].append({'argv':argv});save();raise
                (d/'stdout').write_bytes(r.stdout);(d/'stderr').write_bytes(r.stderr)
                try:report=json.loads(r.stdout)
                except Exception:report={'status':'InvalidReport','diagnostics':[]}
                rec={'case':str(source.relative_to(a.repo)),'config':[opt,units],'arm':tag,'argv':argv,'exit':r.returncode,
                     'status':report['status'],'codes':[x['code'] for x in report.get('diagnostics',[])],
                     'seconds_diagnostic_only':time.perf_counter()-t}
                if report.get('native_codegen'):rec['codegen']=report['native_codegen']
                state['builds'].append(rec)
                if report['status']=='Success':
                    if tag=='candidate':
                        assert all(o['input_kind']=='builder' and o['parse_ns']==0 for o in report['native_codegen']['object_reports']),report
                        assert report['verification_runs']==1,report
                    rec['executable_sha256']=sha(exe)
                results.append((rec,exe))
            before,after=results
            if (before[0]['status'],before[0]['codes'])!=(after[0]['status'],after[0]['codes']):
                state['mismatches'].append({'build_before':before[0],'build_after':after[0]});save();continue
            if before[0]['status']=='Success':
                for hx in state['inputs_hex']:
                    observations=[]
                    for rec,exe in results:
                        try:r=subprocess.run([str(exe)],input=bytes.fromhex(hx),capture_output=True,timeout=3)
                        except subprocess.TimeoutExpired:
                            state['timeouts'].append({'case':rec['case'],'config':rec['config'],'arm':rec['arm'],'input_hex':hx})
                            observations.append(None);continue
                        observations.append((r.returncode,r.stdout.hex(),r.stderr.hex()))
                    state['runs'].append({'case':before[0]['case'],'config':[opt,units],'input_hex':hx,'observations':observations})
                    if observations[0]!=observations[1]:state['mismatches'].append(state['runs'][-1])
            save()
    state['status']='Pass' if not state['mismatches'] and not state['timeouts'] else 'Fail'
    save();print(json.dumps({'status':state['status'],'sources':len(sources),'builds':len(state['builds']),'run_pairs':len(state['runs']),
                           'mismatches':len(state['mismatches']),'timeouts':len(state['timeouts'])}))
    raise SystemExit(0 if state['status']=='Pass' else 1)
if __name__=='__main__':main()
