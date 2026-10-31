#!/usr/bin/env python3
"""Scoped SLP-off native correctness; timings are functional diagnostics only."""
import argparse,hashlib,json,subprocess,sys
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parent.parent))
from stage6_runtime import generate

def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
 p=argparse.ArgumentParser();p.add_argument('--zil',type=Path,required=True);p.add_argument('--out',type=Path,required=True);p.add_argument('--snapshot-receipt',type=Path,required=True);a=p.parse_args();a.out=a.out.resolve();a.out.mkdir(parents=True,exist_ok=False)
 (a.out/'manifest.json').write_text(json.dumps({'scope':__doc__,'compiler':{'path':str(a.zil.resolve()),'sha256':sha(a.zil)},'snapshot_sha256':sha(a.snapshot_receipt),'runner_sha256':sha(Path(__file__)),'generator_sha256':sha(Path(__file__).resolve().parent.parent/'stage6_runtime.py'),'rounds':20,'helpers':512,'timeout_per_build_seconds':90},indent=2)+'\n')
 rows=[]
 for flavor in ['arithmetic','branch','body']:
  source=a.out/flavor;source.mkdir();generate(source,512,flavor,20)
  expected={}
  for seed in [0,1,7,42,255]:
   total=0;x=seed
   for _ in range(20):
    value=sum((x+64+i%97)%1009 if flavor=='body' else (x*3+i%97)%1009 if flavor!='branch' or x%3==0 else (x+i%31)%1009 for i in range(512));total=(total+value)%1000003;x=(x+17)%1009
   expected[seed]=total.to_bytes(4,'little').hex()
  for closed in [False,True]:
   for policy in ['off','after','before-and-after']:
    dest=source/(f'closed{int(closed)}-'+policy);dest.mkdir();cmd=[str(a.zil.resolve()),'build',str(source/'main.zil'),'--backend','llvm-api','--opt','O2','--function-merging',policy,'--codegen-units','8','--codegen-jobs','2','--json','-o',str(dest/'program')]+(['--closed-executable'] if closed else [])
    row={'flavor':flavor,'closed':closed,'policy':policy,'command':cmd,'source_sha256':sha(source/'main.zil'),'valid':False,'native':[]}
    try:
     run=subprocess.run(cmd,capture_output=True,timeout=90);(dest/'driver.stdout').write_bytes(run.stdout);(dest/'driver.stderr').write_bytes(run.stderr);report=json.loads(run.stdout)
     row.update(exit=run.returncode,status=report.get('status'),verification_runs=report.get('verification_runs'),native_codegen=report.get('native_codegen'))
     row['valid']=run.returncode==0 and report.get('status')=='Success' and report.get('verification_runs')==1
     if row['valid']:
      receipts=report['native_codegen']['object_reports'];row['valid'] &= all('slp-vectorization=false' in item['pipeline'] and item['verify_before_after'] for item in receipts)
      row['executable_sha256']=sha(dest/'program')
      for seed,want in expected.items():
       native=subprocess.run([str(dest/'program')],input=bytes([seed]),capture_output=True,timeout=30);item={'seed':seed,'exit':native.returncode,'stdout_hex':native.stdout.hex(),'stderr_hex':native.stderr.hex(),'expected_hex':want};item['valid']=native.returncode==0 and native.stdout.hex()==want and not native.stderr;row['native'].append(item);row['valid'] &= item['valid']
    except (subprocess.TimeoutExpired,OSError,ValueError,KeyError) as error:row['error']=str(error)
    rows.append(row);(dest/'result.json').write_text(json.dumps(row,indent=2)+'\n');(a.out/'summary.json').write_text(json.dumps({'functional_only':True,'rows':rows,'valid':all(x['valid'] for x in rows)},indent=2)+'\n');print(flavor,closed,policy,row['valid'],flush=True)
 if not all(row['valid'] for row in rows):raise SystemExit(1)
if __name__=='__main__':main()
