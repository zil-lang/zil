#!/usr/bin/env python3
"""Independent list-of-byte-records oracle; preserves input/output and every failure."""
import argparse,hashlib,json,os,pathlib,random,shutil,subprocess,time
p=argparse.ArgumentParser();p.add_argument('--repo',required=True);p.add_argument('--driver',required=True);p.add_argument('--out',required=True);p.add_argument('--clang',default='clang');a=p.parse_args();repo=pathlib.Path(a.repo).resolve();out=pathlib.Path(a.out).resolve();out.mkdir(parents=True,exist_ok=False)
source=repo/'examples/mainline/record_vector/main.zil';started=time.time();builds=[]
# Keep this semantic oracle independent of ZIL implementation details: records
# are Python immutable byte strings, not a byte-buffer mutation implementation.
def case(seed,width,limit,steps):
 rng=random.Random(seed);records=[];cap=0;input=bytearray([width,limit%256,limit//256]);expected=bytearray();ops=[]
 def prepare(extra):
  nonlocal cap
  length=len(records)*width
  if width<=0 or extra<0 or length>limit or extra>(limit-length)//width:return False
  need=length+extra*width
  if need>cap:
   n=cap or width;n=n*2 if n<=limit//2 else limit;cap=max(n,need)
  return True
 for step in range(steps):
  code=rng.choices(range(9),weights=[6,4,3,3,3,2,1,3,1])[0];n=len(records);index=rng.choice([-1,0,max(0,n-1),n,n+1,rng.randrange(-3,128)]);index=min(127,max(-128,index))
  length=width if rng.random()<.90 else max(0,width-1);item=bytes(rng.randrange(256) for _ in range(length));ok=False;selected=0
  input+=bytes([code,index+128,len(item)])+item
  valid=width>0
  if code==0 and valid and len(item)==width and prepare(1):records.append(item);ok=True
  elif code==1 and valid and 0<=index<=n and len(item)==width and prepare(1):records.insert(index,item);ok=True
  elif code==2 and valid and 0<=index<n and len(item)==width:records[index]=item;ok=True
  elif code==3 and valid and 0<=index<n:records.pop(index);ok=True
  elif code==4 and valid and 0<=index<n:records[index]=records[-1];records.pop();ok=True
  elif code==5 and valid and n:records.pop();ok=True
  elif code==6 and valid:records.clear();ok=True
  elif code==7 and valid and 0<=index<n:ok=True;selected=records[index][0]
  elif code==8 and valid:ok=prepare(index)
  data=b''.join(records);expected+=bytes([int(ok),selected])+len(data).to_bytes(2,'little')+cap.to_bytes(2,'little')+data
  ops.append({'op':code,'index':index,'item':item.hex(),'success':ok,'records':len(records),'capacity':cap})
 return bytes(input),bytes(expected),ops
for opt in ['O0','O2']:
 dst=out/opt;result=subprocess.run([a.driver,str(source),str(dst),opt],capture_output=True);(out/(opt+'.build.stdout')).write_bytes(result.stdout);(out/(opt+'.build.stderr')).write_bytes(result.stderr);assert result.returncode==0,result.stdout.decode(errors='replace')
 ll=dst/'phases/llvm.ll';assert ll.exists(),list((dst/'phases').iterdir())
 probe=dst/'program-probe';cmd=[a.clang,'-O0' if opt=='O0' else '-O2',str(ll),str(repo/'tools/research/142/allocator_probe.c'),'-Wl,--wrap=realloc','-Wl,--wrap=free','-o',str(probe)];res=subprocess.run(cmd,capture_output=True);(dst/'probe-build.stderr').write_bytes(res.stderr);assert res.returncode==0,res.stderr.decode();builds.extend([(opt,dst/'program'),(opt+'-probe',probe)])
inputs=out/'cases';inputs.mkdir();rows=[];total_ops=0
for width in [0,1,3,12,64]:
 for seed in range(16):
  name=f'w{width}-s{seed}';limit=[0,width,max(width,3*width),min(65535,max(width,64*width))][seed%4]
  data,expected,operations=case(seed*991+width,width,limit,160);total_ops+=len(operations);(inputs/(name+'.stdin')).write_bytes(data);(inputs/(name+'.expected')).write_bytes(expected);(inputs/(name+'.ops.json')).write_text(json.dumps(operations,separators=(',',':')))
  for variant,binary in builds:
   result=subprocess.run([str(binary)],input=data,capture_output=True,timeout=15)
   (inputs/(name+'.'+variant+'.stdout')).write_bytes(result.stdout);(inputs/(name+'.'+variant+'.stderr')).write_bytes(result.stderr)
   row={'case':name,'variant':variant,'exit':result.returncode,'output_matches':result.stdout==expected,'operations':len(operations),'stdout_bytes':len(result.stdout)};rows.append(row)
   (out/'runs.json').write_text(json.dumps(rows,indent=2))
   assert result.returncode==0 and result.stdout==expected,row
   if variant.endswith('-probe'):
    probe=json.loads(result.stderr);assert probe['live']==0 and probe['allocated']==probe['frees'],probe;row['allocation']=probe
(out/'runs.json').write_text(json.dumps(rows,indent=2))
print('random cases passed',len(rows),flush=True)
summary={'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'library_sha256':hashlib.sha256((source.parent/'records.zil').read_bytes()).hexdigest(),'cases':80,'operations_per_case':160,'unique_operations':total_ops,'native_executions':len(rows),'executed_operations':total_ops*len(builds),'variants':[x[0] for x in builds],'all_full_byte_outputs_match':True,'all_probe_live_zero':True,'seconds':time.time()-started,'scope':'ordinary fixed-width byte-record library; not generic Vec<T> or resource-owning elements'}
(out/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary,indent=2))
