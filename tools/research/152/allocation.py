#!/usr/bin/env python3
"""Large bounded FIFO growth, wrapping and last-growth failure, with real allocator probe."""
import argparse,json,os,pathlib,shutil,subprocess
p=argparse.ArgumentParser();p.add_argument('--repo',required=True);p.add_argument('--driver',required=True);p.add_argument('--out',required=True);a=p.parse_args();repo=pathlib.Path(a.repo);out=pathlib.Path(a.out);out.mkdir(parents=True,exist_ok=False)
shutil.copyfile(repo/'examples/mainline/growing_inbox/queue.zil',out/'queue.zil')
(out/'main.zil').write_text('''import queue
fn main()->i32 {
 var q=queue.make(3,65536)
 var n:i32=0
 while n<65536 {
  let posted=q(n%256)
  if posted == -6 {
   if n!=49152 || q(-3)!=49152 || q(-4)!=49152 {return 11}
   var k:i32=0
   while k<n {if q(-1)!=i64(k%256) {return 12};k+=1}
   if q(-5)!=0 || q(-4)!=0 {return 13}
   return 0
  }
  if posted!=i64(n)+1 {return 1};n+=1
 }
 n=0
 while n<32768 {if q(-1)!=i64(n%256) {return 2};n+=1}
 n=0
 while n<32768 {if q(n%256)!=32769+i64(n) {return 3};n+=1}
 if q(-2)!=65536 {return 4}
 n=0
 while n<65536 {if q(-1)!=i64(n%256) {return 5};n+=1}
 if q(-4)!=0 || q(-3)!=0 {return 6}
 return 0
}
''')
rows=[]
for opt in ['O0','O2']:
 d=out/opt;cmd=[a.driver,str(out/'main.zil'),str(d),opt];r=subprocess.run(cmd,capture_output=True,timeout=60);(out/(opt+'-build.stdout')).write_bytes(r.stdout);(out/(opt+'-build.stderr')).write_bytes(r.stderr);assert r.returncode==0,(r.stdout,r.stderr)
 exe=d/'program-probe';cmd=['clang','-'+opt,str(d/'phases/llvm.ll'),str(repo/'tools/research/142/allocator_probe.c'),'-Wl,--wrap=realloc','-Wl,--wrap=free','-o',str(exe)];(d/'probe-command.json').write_text(json.dumps(cmd));r=subprocess.run(cmd,capture_output=True,timeout=30);(d/'probe-build.stderr').write_bytes(r.stderr);assert r.returncode==0,r.stderr
 for fail in [0,16]:
  r=subprocess.run([str(exe)],capture_output=True,timeout=15,env={**os.environ,'ZIL_TEST_FAIL_AT':str(fail)});(d/f'fail{fail}.stdout').write_bytes(r.stdout);(d/f'fail{fail}.stderr').write_bytes(r.stderr);probe=json.loads(r.stderr)
  ok=r.returncode==0 and probe['alloc_calls']==16 and probe['allocated']==1 and probe['frees']==1 and probe['live']==0 and probe['failures']==int(fail>0) and probe['peak_bytes']==(49152 if fail else 65536)
  row={'opt':opt,'fail_at':fail,'exit':r.returncode,'probe':probe,'pass':ok,'scope':'65536 capacity and 98304 FIFO events on successful growth; last-growth failure preserves and drains 49152 pending bytes'};rows.append(row);assert ok,row
summary={'status':'Pass','native_executions':4,'rows':rows,'not_a_timing_or_Rust_comparison':True};(out/'summary.json').write_text(json.dumps(summary,indent=2));print(json.dumps(summary,indent=2))
