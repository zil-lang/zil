import argparse,json,hashlib,subprocess,os,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--cli',type=Path,required=True);p.add_argument('--build-receipt',type=Path,required=True);p.add_argument('--out',type=Path,required=True);args=p.parse_args()
source=Path(__file__).with_name('main.zil');args.out.mkdir(parents=True,exist_ok=False)
build=json.loads(args.build_receipt.read_text());digest=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
assert build['status']=='Pass' and digest(args.cli)==build['cli_sha256']
# Independent source oracle fixed before compilation or execution.
oracles=[{'input':'0','exit':0,'stdout':'BAZ'},{'input':'1','exit':0,'stdout':'CDCBAZ'},{'input':'2','exit':0,'stdout':'DCBAZ'},{'input':'3','exit':1,'stdout':'CBAZ'}]
report={'status':'Running','compiler_source_commit':build['source_commit'],'cli_sha256':build['cli_sha256'],'source_sha256':digest(source),'oracles':oracles,'builds':[],'runs':[],'scope':'Independent generator receiving-scope cleanup; allocation count not measured'}
def save(): (args.out/'RESULT.json').write_text(json.dumps(report,indent=2)+'\n')
save()
env=dict(os.environ,**{k:v for k,v in build['environment'].items() if k.startswith('ZIL_') or k=='LD_LIBRARY_PATH'})
try:
 for opt in ['O0','O2']:
  d=args.out/opt;d.mkdir();exe=d/'program';cmd=[str(args.cli),'build',str(source),'--backend','llvm-api','--opt',opt,'-o',str(exe),'--emit-dir',str(d/'phases'),'--json']
  r=subprocess.run(cmd,capture_output=True,env=env,timeout=60);(d/'build.json').write_bytes(r.stdout);(d/'build.stderr').write_bytes(r.stderr);compiled=json.loads(r.stdout)
  assert r.returncode==0 and compiled['status']=='Success' and compiled['verification_runs']==1,compiled
  assert compiled['native_codegen']['backend']=='llvm-api'
  assert all(x['input_kind']=='builder' for x in compiled['native_codegen']['object_reports'])
  report['builds'].append({'optimization':opt,'command':cmd,'executable_sha256':digest(exe),'compiler':compiled['native_codegen']});save()
  for oracle in oracles:
   r=subprocess.run([str(exe)],input=oracle['input'].encode(),capture_output=True,timeout=10)
   (d/(oracle['input']+'.stdout')).write_bytes(r.stdout);(d/(oracle['input']+'.stderr')).write_bytes(r.stderr)
   row={'optimization':opt,**oracle,'actual_exit':r.returncode,'actual_stdout':r.stdout.decode(),'stderr_hex':r.stderr.hex()};report['runs'].append(row);save()
   assert r.returncode==oracle['exit'] and r.stdout.decode()==oracle['stdout'] and not r.stderr,row
 report['status']='Pass';save();print('Pass: 8 independent actual LLVM API native runs')
except BaseException as error:
 report.update(status='Fail',error=str(error));save();raise
