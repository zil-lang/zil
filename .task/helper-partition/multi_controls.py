#!/usr/bin/env python3
from pathlib import Path
import argparse,importlib.util,json,os,subprocess,sys
HERE=Path(__file__).resolve().parent;sys.path.insert(0,str(HERE));import multimodule as workload
spec=importlib.util.spec_from_file_location('old_controls',HERE/'controls.py');old=importlib.util.module_from_spec(spec);spec.loader.exec_module(old)
p=argparse.ArgumentParser();p.add_argument('--base',required=True);p.add_argument('--candidate',required=True);p.add_argument('--out',type=Path,required=True);a=p.parse_args();out=a.out.resolve();out.mkdir(parents=True)
clang=os.environ['ZIL_CLANG'];(out/'probe.c').write_text(old.PROBE)
r=subprocess.run([clang,'-c',str(out/'probe.c'),'-o',str(out/'probe.o')],capture_output=True);assert r.returncode==0,r.stderr
wrapper=out/'link.py';wrapper.write_text('#!/usr/bin/env python3\nimport os,sys\ncmd='+repr([clang])+'+sys.argv[1:]+([] if "-c" in sys.argv else '+repr(['-x','none',str(out/'probe.o'),'-Wl,--wrap=realloc','-Wl,--wrap=free'])+')\nos.execv(cmd[0],cmd)\n');wrapper.chmod(0o755)
records=[];builds=[];paired={}
try:
 for kind,n in workload.SPECS:
  for backend in ['llvm-api','clang']:
   for opt in ['O0','O2']:
    for arm,cli in [('base',a.base),('cand',a.candidate)]:
     work=out/kind/backend/opt/arm;work.mkdir(parents=True)
     for name,text in workload.files(kind,n,'zil','base').items():(work/name).write_text(text)
     exe=work/'app';cmd=[cli,'build',str(work/'main.zil'),'--json','--backend',backend,'--opt',opt,'--codegen-units','8','--codegen-jobs','2','--clang',str(wrapper),'-o',str(exe)]
     if opt=='O2':cmd+=['--incremental-o2','--function-merging','after']
     r=subprocess.run(cmd,capture_output=True,timeout=120);(work/'stdout').write_bytes(r.stdout);(work/'stderr').write_bytes(r.stderr)
     report=json.loads(r.stdout);assert r.returncode==0 and report['status']=='Success' and report['verification_runs']==1,report
     builds.append({'case':kind,'backend':backend,'opt':opt,'arm':arm,'report':report})
     for fail in [0,1,4,16,64]:
      for seed in [0,3,7,15]:
       data=bytes([seed,3,0,2]);r=subprocess.run([str(exe)],input=data,capture_output=True,env=dict(os.environ,ZIL_TEST_FAIL_AT=str(fail)),timeout=30)
       assert r.returncode==0,(kind,arm,fail,r.returncode,r.stderr);allocation=json.loads(r.stderr)
       assert allocation['live']==0 and allocation['allocated']==allocation['freed'],allocation
       if fail==0:assert r.stdout==workload.expected(kind,n,'base',data)
       key=(kind,backend,opt,fail,seed);value=(r.stdout.hex(),allocation)
       if arm=='base':paired[key]=value
       else:assert paired[key]==value,(key,paired[key],value)
       records.append({'case':kind,'backend':backend,'opt':opt,'arm':arm,'fail':fail,'input':data.hex(),'output':r.stdout.hex(),'allocation':allocation})
 result={'complete':True,'builds':len(builds),'observations':len(records),'build_reports':builds,'observations_detail':records}
 (out/'summary.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({k:v for k,v in result.items() if k not in ('build_reports','observations_detail')}))
except Exception as error:
 (out/'failure.json').write_text(json.dumps({'error':repr(error),'builds':builds,'observations':records},indent=2)+'\n');raise
