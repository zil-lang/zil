#!/usr/bin/env python3
import argparse,json,os,subprocess,time,sys,hashlib
from pathlib import Path
sys.path.insert(0,str(Path(__file__).parent/'bench'))
import corpus

PROBE=r'''#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
extern void *__real_realloc(void *,size_t);
extern void __real_free(void *);
static struct {void *p; unsigned id;} slots[4096];
static unsigned calls,failed,allocated,freed,live,next_id,fail_at;
static uint64_t trace=1469598103934665603ULL;
static void event(unsigned tag,unsigned id,size_t size){trace=(trace^tag)*1099511628211ULL;trace=(trace^id)*1099511628211ULL;trace=(trace^size)*1099511628211ULL;}
static void report(void){fprintf(stderr,"{\"calls\":%u,\"failed\":%u,\"allocated\":%u,\"freed\":%u,\"live\":%u,\"trace\":\"%016llx\"}\n",calls,failed,allocated,freed,live,(unsigned long long)trace);}
__attribute__((constructor)) static void init(void){const char*s=getenv("ZIL_TEST_FAIL_AT");if(s)fail_at=(unsigned)strtoul(s,0,10);if(atexit(report))_Exit(112);}
void *__wrap_realloc(void*p,size_t n){unsigned k=4096;for(unsigned i=0;i<4096;i++)if(slots[i].p==p&&(p||k==4096)){k=i;if(p)break;}
 if(k==4096)_Exit(113);calls++;event(1,p?slots[k].id:0,n);if(fail_at&&calls==fail_at){failed++;event(2,p?slots[k].id:0,n);return 0;}
 void*q=__real_realloc(p,n);if(!q){failed++;event(2,p?slots[k].id:0,n);return 0;}if(!p){slots[k].id=++next_id;allocated++;live++;}slots[k].p=q;event(3,slots[k].id,n);return q;}
void __wrap_free(void*p){if(!p)return;for(unsigned i=0;i<4096;i++)if(slots[i].p==p){event(4,slots[i].id,0);slots[i].p=0;freed++;live--;__real_free(p);return;}_Exit(114);}
'''

def main():
 p=argparse.ArgumentParser();p.add_argument('--base',type=Path,required=True);p.add_argument('--candidate',type=Path,required=True);p.add_argument('--out',type=Path,required=True);a=p.parse_args();out=a.out.resolve();out.mkdir(parents=True,exist_ok=False)
 clang=os.environ['ZIL_CLANG'];builds=[];rows=[];paired={};activations=[]
 def invoke(argv,name,data=b'',env=None):
  d=out/name;d.mkdir(parents=True);argv=list(map(str,argv));t=time.perf_counter_ns();r=subprocess.run(argv,input=data,capture_output=True,env=dict(os.environ,**(env or {})),timeout=180)
  (d/'stdout').write_bytes(r.stdout);(d/'stderr').write_bytes(r.stderr);rec={'argv':argv,'input':data.hex(),'exit':r.returncode,'wall_ms':(time.perf_counter_ns()-t)/1e6,'env':env or {}};(d/'record.json').write_text(json.dumps(rec));return r,rec
 c=out/'probe.c';c.write_text(PROBE);obj=out/'probe.o';r,_=invoke([clang,'-c',c,'-o',obj],'probe');assert r.returncode==0,r.stderr
 wrapper=out/'link.py';wrapper.write_text('#!/usr/bin/env python3\nimport os,sys\ncmd='+repr([clang])+'+sys.argv[1:]+([] if "-c" in sys.argv else '+repr(['-x','none',str(obj),'-Wl,--wrap=realloc','-Wl,--wrap=free'])+')\nos.execv(cmd[0],cmd)\n');wrapper.chmod(0o755)
 try:
  for n in [8,32,64]:
   source=out/f'owned{n}.zil';source.write_text(corpus.resources(n,'zil',False)+corpus.io_source('zil'))
   for backend in ['llvm-api','clang']:
    for opt in ['O0','O2']:
     for units in [1,8]:
      for arm,cli in [('base',a.base.resolve()),('cand',a.candidate.resolve())]:
       name=f'owned{n}/{backend}/{opt}/{units}/{arm}';exe=out/name/'app';exe.parent.mkdir(parents=True)
       argv=[cli,'build',source,'--json','--backend',backend,'--opt',opt,'--codegen-units',units,'--codegen-jobs',2,'--clang',wrapper,'-o',exe]
       if opt=='O2':argv+=['--incremental-o2','--function-merging','after']
       r,rec=invoke(argv,name+'/build');report=json.loads(r.stdout);assert r.returncode==0 and report['status']=='Success' and report['verification_runs']==1,report
       ir=Path(str(exe)+'.ll').read_text();active='cleanup.tail.' in ir
       assert active,(name,'both compilers must retain accepted PR76 cleanup sharing')
       drops=[line for line in ir.splitlines() if line.startswith('define internal void @zil.closure.drop.')]
       assert len(drops)==n,(name,len(drops),n)
       outlined=sum('noinline' in line for line in drops)
       assert outlined==(n if arm=='cand' and n>=16 else 0),(name,'destructor policy did not match frozen threshold',outlined)
       activations.append({'name':name,'active':active,'ir_bytes':len(ir.encode()),'cleanup_refs':ir.count('cleanup.tail.'),'closure_drop_calls':sum('call void @zil.closure.drop.' in line for line in ir.splitlines())})
       builds.append({'name':name,'report':report,'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest()})
       for fail in [0,1,2,4,8,16,32,64,128]:
        for data in [bytes([73,3,0,1]),bytes([29,1,0,2])]:
         r,rec=invoke([exe],name+f'/fail{fail}-{data.hex()}',data,{'ZIL_TEST_FAIL_AT':str(fail)});assert r.returncode==0,(name,fail,r.returncode,r.stderr)
         allocation=json.loads(r.stderr);assert allocation['live']==0 and allocation['allocated']==allocation['freed'],allocation
         if fail==0:assert r.stdout==corpus.expected(f'owned{n}',n,'base',data)
         key=(n,backend,opt,units,fail,data.hex());value=(r.stdout.hex(),allocation)
         if arm=='base':paired[key]=value
         else:assert paired[key]==value,(key,paired[key],value)
         rows.append({'name':name,'fail_at':fail,'input':data.hex(),'output':r.stdout.hex(),'allocation':allocation})
  result={'status':'Pass','builds':len(builds),'observations':len(rows),'paired_observations':len(rows)//2,'activated_candidate_builds':sum(v['active'] and v['name'].endswith('/cand') for v in activations),'activation':activations,'build_reports':builds,'observations_detail':rows}
  (out/'summary.json').write_text(json.dumps(result,indent=2));print(json.dumps({k:v for k,v in result.items() if k not in ['activation','build_reports','observations_detail']}))
 except Exception as e:
  (out/'failure.json').write_text(json.dumps({'error':repr(e),'builds':builds,'observations':rows,'activation':activations},indent=2));raise
if __name__=='__main__':main()
