#!/usr/bin/env python3
"""Real owned FIFO callback: wraparound, full/closed/empty and fail-on-init."""
import argparse,json,os,pathlib,shutil,subprocess
p=argparse.ArgumentParser();p.add_argument('--repo',required=True);p.add_argument('--driver',required=True);p.add_argument('--out',required=True);a=p.parse_args();repo=pathlib.Path(a.repo).resolve();out=pathlib.Path(a.out).resolve();out.mkdir(parents=True,exist_ok=False)
for name in ['inbox.zil','records.zil','io.zil']:shutil.copyfile(repo/'examples/mainline/record_vector'/name,out/name)
source='''import inbox
import io
fn main()->i32 {
 let mode=io.read_byte()
 var q=inbox.make(8)
 if mode==1 {if q(7)!=-3 {return 1};return 0}
 if q(-4)!=8 {return 2}
 var round:i32=0
 while round<4096 {
  var j:i32=0
  while j<5 {if q((round+j)%256)!=i64(j)+1 {return 3};j+=1}
  j=0
  while j<5 {if q(-1)!=i64((round+j)%256) {return 4};j+=1}
  round+=1
 }
 var i:i32=0
 while i<8 {if q(31+i)!=i64(i)+1 {return 5};i+=1}
 if q(99)!=-2 || q(-2)!=8 || q(99)!=-1 {return 6}
 i=0
 while i<8 {if q(-1)!=i64(31+i) {return 7};i+=1}
 if q(-4)!=0 || q(-3)!=0 || q(-1)!=-4 || q(99)!=-1 {return 8}
 return 0
}
''';(out/'main.zil').write_text(source);rows=[]
for opt in ['O0','O2']:
 dst=out/opt;r=subprocess.run([a.driver,str(out/'main.zil'),str(dst),opt],capture_output=True);(out/(opt+'.build.stdout')).write_bytes(r.stdout);(out/(opt+'.build.stderr')).write_bytes(r.stderr);assert r.returncode==0,(r.stdout,r.stderr)
 exe=dst/'program-probe';cmd=['clang','-'+opt,str(dst/'phases/llvm.ll'),str(repo/'tools/research/142/allocator_probe.c'),'-Wl,--wrap=realloc','-Wl,--wrap=free','-o',str(exe)];r=subprocess.run(cmd,capture_output=True);(dst/'probe-build.stderr').write_bytes(r.stderr);assert r.returncode==0,r.stderr
 for fail_at in [0,1,2]:
  data=bytes([1 if fail_at==1 else 0]);env={**os.environ,'ZIL_TEST_FAIL_AT':str(fail_at)};r=subprocess.run([str(exe)],input=data,capture_output=True,env=env,timeout=30);name=f'{opt}-fail{fail_at}';(out/(name+'.stdin')).write_bytes(data);(out/(name+'.stdout')).write_bytes(r.stdout);(out/(name+'.stderr')).write_bytes(r.stderr);probe=json.loads(r.stderr);row={'variant':opt,'fail_at':fail_at,'exit':r.returncode,'probe':probe,'fifo_records_drained':0 if fail_at==1 else 4096*5+8};rows.append(row);(out/'runs.json').write_text(json.dumps(rows,indent=2));assert r.returncode==0 and probe['live']==0,row
  assert probe['alloc_calls']==1 and probe['allocated']==probe['frees']==(0 if fail_at==1 else 1),row
  assert probe['failures']==(1 if fail_at==1 else 0),row
receipt={'native_runs':len(rows),'passed':True,'rows':rows,'scope':'single-thread finite byte events; post/full/FIFO/close/drain, eager buffer release on empty closed inbox; not async scheduler or generic owning-element queue'};(out/'summary.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt,indent=2))
