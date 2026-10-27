#!/usr/bin/env python3
"""Fault-preservation and growth policy checks using actual realloc/free calls.
No wall-clock speed claims: every run checks contents and released ownership.
"""
import argparse,json,os,pathlib,shutil,subprocess
p=argparse.ArgumentParser();p.add_argument('--repo',required=True);p.add_argument('--driver',required=True);p.add_argument('--model-results',required=True);p.add_argument('--out',required=True);a=p.parse_args();repo=pathlib.Path(a.repo).resolve();model=pathlib.Path(a.model_results).resolve();out=pathlib.Path(a.out).resolve();out.mkdir(parents=True,exist_ok=False);rows=[]
def payload(ops,width,limit,fail_at):
 records=[];cap=0;calls=1;data=bytearray([width,limit%256,limit//256]);expected=bytearray();failures=[]
 def prepare(extra):
  nonlocal cap,calls
  length=len(records)*width
  if extra<0 or length>limit or extra>(limit-length)//width:return False
  need=length+extra*width
  if need>cap:
   calls+=1
   if calls==fail_at:return False
   next=cap or width;next=next*2 if next<=limit//2 else limit;cap=max(next,need)
  return True
 for code,index,item in ops:
  data+=bytes([code,index+128,len(item)])+item
  before=(tuple(records),cap);n=len(records);ok=False;selected=0
  if code==0 and prepare(1):records.append(item);ok=True
  elif code==1 and 0<=index<=n and prepare(1):records.insert(index,item);ok=True
  elif code==2 and 0<=index<n:records[index]=item;ok=True
  elif code==3 and 0<=index<n:records.pop(index);ok=True
  elif code==4 and 0<=index<n:records[index]=records[-1];records.pop();ok=True
  elif code==5 and n:records.pop();ok=True
  elif code==6:records.clear();ok=True
  elif code==7 and 0<=index<n:ok=True;selected=records[index][0]
  elif code==8:ok=prepare(index)
  if not ok:
   assert (tuple(records),cap)==before
   failures.append({'operation':code,'index':index,'before_length':n*width,'before_capacity':before[1]})
  blob=b''.join(records);expected+=bytes([int(ok),selected])+len(blob).to_bytes(2,'little')+cap.to_bytes(2,'little')+blob
 return bytes(data),b'' if fail_at==1 else bytes(expected),3 if fail_at==1 else 0,failures
for width in [1,3,12,64]:
 x=bytes([7])*width;y=bytes([11])*width;z=bytes([23])*width
 ops=[(0,0,x),(0,0,y),(1,1,z),(8,17,b''),(8,127,b''),(2,0,z),(3,1,b''),(4,0,b''),(5,0,b''),(6,0,b''),(0,0,x),(8,127,b'')]
 for fail_at in range(1,7):
  data,expected,code,failures=payload(ops,width,65535,fail_at)
  for opt in ['O0','O2']:
   name=f'fault-w{width}-at{fail_at}-{opt}';dst=out/name;dst.mkdir();env={**os.environ,'ZIL_TEST_FAIL_AT':str(fail_at)};r=subprocess.run([str(model/opt/'program-probe')],input=data,capture_output=True,env=env,timeout=15)
   (dst/'stdin').write_bytes(data);(dst/'expected').write_bytes(expected);(dst/'stdout').write_bytes(r.stdout);(dst/'stderr').write_bytes(r.stderr);probe=json.loads(r.stderr)
   row={'name':name,'returncode':r.returncode,'expected_returncode':code,'bytes_match':r.stdout==expected,'fault_at':fail_at,'operation_failures':failures,'probe':probe};rows.append(row);(out/'runs.json').write_text(json.dumps(rows,indent=2))
   assert r.returncode==code and r.stdout==expected and probe['live']==0 and probe['allocated']==probe['frees'],row
   assert probe['failures']==(0 if fail_at==6 else 1),row
for n in [4096,65536]:
 for strategy in ['exact','geometric']:
  src=out/f'{strategy}-{n}';src.mkdir();shutil.copyfile(repo/'examples/mainline/record_vector/records.zil',src/'records.zil')
  push='if !records.push(v,12,&item[..],2000000) {return 2}' if strategy=='geometric' else 'if !bytes_reserve(v,bytes_len(v)+12) {return 2}; var k:i64=0;while k<12 {bytes_push(v,item[k]);k+=1}'
  program='''import records
fn main()->i32 {let v=bytes_new();var i:i64=0
 while i<N {let item=[u8(i%256),u8(i%256),u8(i%256),u8(i%256),u8(i%256),u8(i%256),u8(i%256),u8(i%256),u8(i%256),u8(i%256),u8(i%256),u8(i%256)];PUSH;i+=1}
 if records.count(v,12)!=N {return 3}
 let data=bytes_view(v);var at:i64=0
 while at<data.len() {if data[at]!=u8((at/12)%256) {return 4};at+=1}
 if !records.clear(v,12) || records.count(v,12)!=0 {return 5}
 return 0}
'''.replace('N',str(n)).replace('PUSH',push)
  (src/'main.zil').write_text(program)
  for opt in ['O0','O2']:
   build=src/opt;r=subprocess.run([a.driver,str(src/'main.zil'),str(build),opt],capture_output=True);(src/(opt+'.build.stdout')).write_bytes(r.stdout);(src/(opt+'.build.stderr')).write_bytes(r.stderr);assert r.returncode==0,(r.stdout,r.stderr)
   exe=build/'program-probe';cmd=['clang','-'+opt,str(build/'phases/llvm.ll'),str(repo/'tools/research/142/allocator_probe.c'),'-Wl,--wrap=realloc','-Wl,--wrap=free','-o',str(exe)];r=subprocess.run(cmd,capture_output=True);(build/'probe-build.stderr').write_bytes(r.stderr);assert r.returncode==0,r.stderr
   r=subprocess.run([str(exe)],capture_output=True,timeout=30);(build/'stdout').write_bytes(r.stdout);(build/'stderr').write_bytes(r.stderr);probe=json.loads(r.stderr);row={'name':f'growth-{strategy}-{n}-{opt}','returncode':r.returncode,'strategy':strategy,'records':n,'bytes_checked':n*12,'probe':probe};rows.append(row);(out/'runs.json').write_text(json.dumps(rows,indent=2))
   assert r.returncode==0 and probe['live']==0 and probe['allocated']==probe['frees']==1,row
   assert probe['alloc_calls']==(n if strategy=='exact' else (12*n-1).bit_length()-4),row
summary={'fault_native_runs':48,'faults_actually_injected':40,'armed_but_unneeded_allocation_runs':8,'growth_native_runs':8,'all_passed':True,'all_ownership_released':True,'claim':'allocation call counts and exact contents only; no timing or general Vec<T> claim','growth':[x for x in rows if x['name'].startswith('growth')]}
(out/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary,indent=2))
