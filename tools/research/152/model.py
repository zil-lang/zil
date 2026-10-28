#!/usr/bin/env python3
"""Differential FIFO/close/cancel model with actual allocator failure controls.

Does not reuse the ring rotation implementation. The oracle stores a Python
deque in logical FIFO order. Every command compares its result, pending count,
actual capacity and terminal state; final drains expose every pending byte.
"""
import argparse, collections, hashlib, json, os, pathlib, random, shutil, struct, subprocess
p=argparse.ArgumentParser();p.add_argument('--repo',required=True);p.add_argument('--driver',required=True);p.add_argument('--out',required=True);a=p.parse_args()
repo=pathlib.Path(a.repo).resolve();out=pathlib.Path(a.out).resolve();out.mkdir(parents=True,exist_ok=False)
shutil.copyfile(repo/'examples/mainline/growing_inbox/queue.zil',out/'queue.zil')
shutil.copyfile(repo/'examples/mainline/record_vector/io.zil',out/'io.zil')
source='''import queue
import io
fn read16()->i32{let lo=io.read_byte();if lo<0{return -1};let hi=io.read_byte();if hi<0{return -1};return lo+hi*256}
fn emit(value:i64){var x=value;if x<0{x+=4294967296};var i:i32=0;while i<4{io.write_byte(u8(x%256));x=x/256;i+=1}}
fn main()->i32{
 let initial=read16();let maximum=read16();if initial<0 || maximum<0{return 1}
 var q=queue.make(i64(initial),i64(maximum))
 while true{var command=read16();if command<0{break};if command>=32768{command-=65536}
  emit(q(command));emit(q(-3));emit(q(-4));emit(q(-6))
 }
 q(-5)
 return 0
}
'''
(out/'main.zil').write_text(source)
class Model:
 def __init__(self,initial,maximum,fail_at):
  self.values=collections.deque();self.maximum=maximum;self.capacity=0;self.closed=False;self.calls=0;self.failures=0;self.fail_at=fail_at;self.allocated=0;self.frees=0;self.head=0;self.wrapped_growth_failures=0
  self.ready=0<=initial<=maximum and maximum>0
  if self.ready and initial:self.ready=self.reserve(initial)
 def reserve(self,n):
  if n<=self.capacity:return True
  self.calls+=1
  if self.calls==self.fail_at:self.failures+=1;return False
  if not self.capacity:self.allocated+=1
  self.capacity=n;return True
 def release(self):
  if self.capacity:self.frees+=1
  self.capacity=0;self.head=0
 def call(self,c):
  if not self.ready:return -3
  if c==-3:return len(self.values)
  if c==-4:return self.capacity
  if c==-6:return int(self.closed)
  if c==-5:
   n=len(self.values);self.values.clear();self.closed=True;self.release();return n
  if c==-2:
   self.closed=True
   if not self.values:self.release()
   return len(self.values)
  if c==-1:
   if not self.values:return -4
   v=self.values.popleft();self.head=(self.head+1)%self.capacity
   if not self.values:
    self.head=0
    if self.closed:self.release()
   return v
  if c<0 or c>255:return -5
  if self.closed:return -1
  if len(self.values)>=self.maximum:return -2
  if len(self.values)==self.capacity:
   old=tuple(self.values),self.capacity,self.head
   n=min(self.maximum,self.capacity*2)if self.capacity else 1
   if not self.reserve(n):
    assert old==(tuple(self.values),self.capacity,self.head)
    if self.head:self.wrapped_growth_failures+=1
    return -6
   self.head=0
  self.values.append(c);return len(self.values)
def expected(initial,maximum,commands,fail):
 m=Model(initial,maximum,fail);b=bytearray()
 for c in commands:
  for v in [m.call(c),m.call(-3),m.call(-4),m.call(-6)]:b+=struct.pack('<i',v)
 m.call(-5)
 # Destruction still runs after failed initialization or the explicit cancel.
 if m.capacity:m.release()
 return bytes(b),m
cases=[]
for seed in range(32):
 rng=random.Random(152000+seed);initial=[0,1,3,7][seed%4];maximum=[7,17,31,257][seed//4%4]
 # A fixed wrapped-full growth prefix before randomized commands.
 commands=[10,11,12,-1,-1,13,14,15,16,-1,17,18,19,20]
 for _ in range(160):commands.append(rng.choice([-1,-1,-3,-4,256,-100]+[rng.randrange(256) for _ in range(12)]))
 commands+=[-2 if seed%2==0 else -5]+[-1]*(maximum+2)+[1,-2,-5,-5,-6]
 cases.append({'seed':152000+seed,'initial':initial,'maximum':maximum,'commands':commands})
(out/'cases.json').write_text(json.dumps(cases,indent=2));rows=[]
for opt in ['O0','O2']:
 dst=out/opt;cmd=[a.driver,str(out/'main.zil'),str(dst),opt];r=subprocess.run(cmd,capture_output=True,timeout=60)
 (out/(opt+'-build.stdout')).write_bytes(r.stdout);(out/(opt+'-build.stderr')).write_bytes(r.stderr)
 assert r.returncode==0,(r.stdout,r.stderr)
 exe=dst/'program-probe';cmd=['clang','-'+opt,str(dst/'phases/llvm.ll'),str(repo/'tools/research/142/allocator_probe.c'),'-Wl,--wrap=realloc','-Wl,--wrap=free','-o',str(exe)]
 r=subprocess.run(cmd,capture_output=True,timeout=30);(dst/'probe-command.json').write_text(json.dumps(cmd));(dst/'probe-build.stderr').write_bytes(r.stderr);assert r.returncode==0,r.stderr
 for case in cases:
  data=struct.pack('<HH',case['initial'],case['maximum'])+b''.join(struct.pack('<h',x)for x in case['commands'])
  for fail in [0,1,2,3,5,7]:
   wanted,m=expected(case['initial'],case['maximum'],case['commands'],fail);name=f'{opt}-seed{case["seed"]}-fail{fail}';d=out/'runs'/name;d.mkdir(parents=True)
   (d/'stdin').write_bytes(data);(d/'expected').write_bytes(wanted)
   r=subprocess.run([str(exe)],input=data,capture_output=True,timeout=15,env={**os.environ,'ZIL_TEST_FAIL_AT':str(fail)})
   (d/'stdout').write_bytes(r.stdout);(d/'stderr').write_bytes(r.stderr)
   probe=json.loads(r.stderr) if r.stderr else {'alloc_calls':0,'allocated':0,'frees':0,'failures':0,'live':0}
   ok=r.returncode==0 and r.stdout==wanted and probe['live']==0 and probe['alloc_calls']==m.calls and probe['failures']==m.failures and probe['allocated']==m.allocated and probe['frees']==m.frees
   row={'name':name,'pass':ok,'exit':r.returncode,'commands':len(case['commands']),'fail_at':fail,'probe':probe,'expected_calls':m.calls,'wrapped_growth_failures':m.wrapped_growth_failures};rows.append(row)
   with (out/'runs.jsonl').open('a')as f:f.write(json.dumps(row)+'\n')
   assert ok,row
summary={'status':'Pass','cases':len(cases),'native_executions':len(rows),'operation_executions':sum(r['commands']for r in rows),'actual_allocator_failures':sum(r['probe']['failures']for r in rows),'wrapped_growth_faults':sum(r['wrapped_growth_failures']for r in rows),'all_released':all(r['probe']['live']==0 for r in rows),'source_sha256':hashlib.sha256((out/'queue.zil').read_bytes()).hexdigest(),'oracle':'Python deque plus separate capacity/failure contract; never uses native rotation implementation','limits':'single-thread byte FIFO; not generic owning event container or external async cancellation'}
(out/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary,indent=2))
