import argparse,json,os,subprocess
from pathlib import Path
ap=argparse.ArgumentParser();ap.add_argument('--cli',required=True);ap.add_argument('--worker',required=True);ap.add_argument('--out',type=Path,required=True);a=ap.parse_args();a.out.mkdir(parents=True)
src=a.out/'main.zil';cache=a.out/'cache';exe=a.out/'app';log=[]
p=subprocess.Popen([a.worker,'check',str(src),str(exe),str(cache),'8'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,bufsize=1)
hello=json.loads(p.stdout.readline());assert hello['ready']
try:
 for text in ['fn main()->i32{return 41;}','fn main()->i32{return 41;}','fn main()->i32{return missing();}','fn main()->i32{return 42;}','fn main( ->','fn main()->i32{return 41;}']:
  src.write_text(text);p.stdin.write('compile\n');p.stdin.flush();resident=json.loads(p.stdout.readline());r=resident['report']
  argv=[a.cli,'check',str(src),'--json','--backend','llvm-api','--semantic-cache',str(cache/'semantic')]
  result=subprocess.run(argv,capture_output=True,text=True,timeout=60);fresh=json.loads(result.stdout)
  for k in ('status','phase','diagnostics','verification_runs'):assert r[k]==fresh[k],(k,r,fresh)
  if r['status']=='Success':assert r['verification_runs']==1
  assert resident['cached_queries']<=8
  log.append({'source':text,'resident':resident,'fresh':fresh,'argv':argv,'fresh_exit':result.returncode})
 p.stdin.write('clear\n');p.stdin.flush();cleared=json.loads(p.stdout.readline());assert cleared=={'cleared':True,'cached_queries':0}
 p.stdin.write('compile\n');p.stdin.flush();r=json.loads(p.stdout.readline());assert r['report']['status']=='Success' and r['report']['verification_runs']==1
 log.append({'after_clear':r});p.stdin.write('quit\n');p.stdin.flush();assert p.wait(timeout=10)==0
finally:
 if p.poll() is None:p.kill();p.wait()
 (a.out/'records.json').write_text(json.dumps(log,indent=2)+'\n')
p=subprocess.Popen([a.worker,'check',str(src),str(exe),str(cache),'8'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
assert json.loads(p.stdout.readline())['ready'];p.stdin.write('x'*40+'\n');p.stdin.flush();stdout,stderr=p.communicate(timeout=10);assert p.returncode==2 and 'oversized' in stderr
summary={'status':'Pass','same_process_compiles':7,'fresh_reference_compiles':6,'invalid_program_requests':2,'restore_after_failure':True,'clear_confirmed':True,'oversized_protocol_refused':True,'capacity_bound':'raw query entry count only, not a process memory byte bound'}
(a.out/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary))
