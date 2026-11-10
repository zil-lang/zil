#!/usr/bin/env python3
import argparse,json,os,random,statistics,subprocess,time
from pathlib import Path

def main():
 p=argparse.ArgumentParser();p.add_argument('--base',required=True,type=Path);p.add_argument('--candidate',required=True,type=Path);p.add_argument('--out',required=True,type=Path);a=p.parse_args();out=a.out.resolve();out.mkdir(parents=True,exist_ok=False)
 os.sched_setaffinity(0,sorted(os.sched_getaffinity(0))[:2])
 clang=os.environ['ZIL_CLANG'];rows=[];builds=[]
 def run(argv,name,data=b'',env=None):
  d=out/name;d.mkdir(parents=True,exist_ok=False);t=time.perf_counter_ns();r=subprocess.run(list(map(str,argv)),input=data,capture_output=True,env=dict(os.environ,**(env or {})),timeout=120);elapsed=(time.perf_counter_ns()-t)/1e6
  (d/'stdout').write_bytes(r.stdout);(d/'stderr').write_bytes(r.stderr);rec={'argv':list(map(str,argv)),'input':data.hex(),'exit':r.returncode,'wall_ms':elapsed,'env':env or {}};(d/'record.json').write_text(json.dumps(rec));return r,rec
 edge=out/'edge.zil';edge.write_text('''fn main()->i32 {
 let b=bytes_new();
 if bytes_reserve(b,-1) {return 1;}
 if !bytes_reserve(b,0) || bytes_capacity(b)!=0 {return 2;}
 if !bytes_reserve(b,8) {if bytes_capacity(b)!=0 || bytes_len(b)!=0 {return 3;} return 0;}
 if !bytes_push(b,7u8) {return 4;}
 if !bytes_reserve(b,4) || !bytes_reserve(b,8) {return 5;}
 let ok=bytes_reserve(b,32);
 if ok {if bytes_capacity(b)!=32 {return 6;}} else {if bytes_capacity(b)!=8 {return 7;}}
 if bytes_len(b)!=1 {return 8;}
 let s=bytes_view(b);if s[0]!=7u8 {return 9;}
 return 0;
}
''')
 hot=out/'hot.zil';hot.write_text('''extern "C" fn getchar()->i32;
extern "C" fn putchar(x:i32)->i32;
fn main()->i32 {
 var n:i64=0;var k:i64=0;
 while k<4 {unsafe {let b=getchar();if b<0 {return 1;} n=n*256+i64(b);} k+=1;}
 let b=bytes_new();if !bytes_reserve(b,64) {return 2;}
 var acc:i64=0;var i:i64=0;
 while i<n {let need=i%65;if !bytes_reserve(b,need){return 3;} acc=(acc+bytes_capacity(b)+need)%65521;i+=1;}
 unsafe {putchar(i32(acc%256));putchar(i32(acc/256));}
 return 0;
}
''')
 obj=out/'probe.o';r,_=run([clang,'-c','tools/language/composed-store-closures/allocator_probe.c','-o',obj],'probe-build');assert r.returncode==0,r.stderr
 wrap=out/'link.py';wrap.write_text('#!/usr/bin/env python3\nimport os,sys\ncmd='+repr([clang])+'+sys.argv[1:]+'+'( [] if "-c" in sys.argv else '+repr(['-x','none',str(obj),'-Wl,--wrap=realloc','-Wl,--wrap=free'])+')\nos.execv(cmd[0],cmd)\n');wrap.chmod(0o755)
 try:
  for arm,cli in [('base',a.base.resolve()),('cand',a.candidate.resolve())]:
   for backend in ['llvm-api','clang']:
    for opt in ['O0','O2']:
     for units in [1,8]:
      name=f'{arm}/{backend}/{opt}/u{units}';exe=out/(name.replace('/','-')+'-edge')
      argv=[cli,'build',edge,'--json','--backend',backend,'--opt',opt,'--codegen-units',units,'--codegen-jobs',2,'--clang',wrap,'-o',exe]
      if opt=='O2':argv+=['--incremental-o2']
      r,rec=run(argv,name+'/build');rep=json.loads(r.stdout);assert r.returncode==0 and rep['status']=='Success' and rep['verification_runs']==1,rep;builds.append({'name':name,'report':rep})
      for fail in [0,1,2,3]:
       r,rec=run([exe],name+f'/fail-{fail}',env={'ZIL_TEST_FAIL_AT':str(fail)});assert r.returncode==0 and not r.stdout,(r.returncode,r.stdout,r.stderr)
       alloc=json.loads(r.stderr);assert alloc['live']==0 and alloc['frees']==alloc['allocated'],alloc
       assert alloc['alloc_calls']==(1 if fail==1 else 2),alloc
       assert alloc['failures']==int(fail in [1,2]),alloc
       assert alloc['allocated']==int(fail!=1),alloc
       rows.append({'name':name,'fail_at':fail,'allocation':alloc})
   exe=out/f'{arm}-hot';r,rec=run([cli,'build',hot,'--json','--backend','llvm-api','--opt','O2','--codegen-units','8','--codegen-jobs','2','--incremental-o2','--clang',clang,'-o',exe],arm+'/hot-build');rep=json.loads(r.stdout);assert r.returncode==0 and rep['verification_runs']==1,rep;builds.append({'name':arm+'/hot','report':rep})
  iterations=1<<20
  while iterations<1<<26:
   t=time.perf_counter();r=subprocess.run([out/'base-hot'],input=iterations.to_bytes(4,'big'),capture_output=True,timeout=30);assert r.returncode==0
   if time.perf_counter()-t>=.15:break
   iterations*=2
  q,rem=divmod(iterations,65);v=(64*iterations+q*2080+rem*(rem-1)//2)%65521;want=bytes([v%256,v//256]);runtime=[]
  for rep in range(-1,9):
   arms=['base','cand'];random.Random(rep).shuffle(arms)
   for arm in arms:
    r,rec=run([out/f'{arm}-hot'],f'hot-runtime/{rep}/{arm}',iterations.to_bytes(4,'big'));assert r.returncode==0 and r.stdout==want and not r.stderr,(r.stdout,want,r.stderr);rec.update(arm=arm,rep=rep);runtime.append(rec)
  med={arm:statistics.median(r['wall_ms'] for r in runtime if r['arm']==arm and r['rep']>=0) for arm in ['base','cand']}
  result={'status':'Pass','builds':len(builds),'failure_checks':len(rows),'hot_iterations':iterations,'hot_median_ms':med,'hot_ratio':med['cand']/med['base'],'hot_runtime_screen_pass':med['cand']/med['base']<=1.10,'observations':rows,'runtime':runtime,'build_reports':builds}
  (out/'summary.json').write_text(json.dumps(result,indent=2));print(json.dumps({k:v for k,v in result.items() if k not in ['observations','runtime','build_reports']}))
 except Exception as e:
  (out/'failure.json').write_text(json.dumps({'error':repr(e),'builds':builds,'observations':rows},indent=2));raise
if __name__=='__main__':main()
