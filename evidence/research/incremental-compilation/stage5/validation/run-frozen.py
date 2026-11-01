from pathlib import Path
import subprocess,json,datetime,zoneinfo
root=Path('/workspace/zil');r=Path('/workspace/.zil-env/research/stage5')
base=['--zil','/workspace/.zil-env/research/target-stage5/release/zil','--rustc',subprocess.check_output(['rustup','which','rustc'],text=True).strip(),'--blocks','6']
shapes='tools/research/incremental-compilation/distribution_bench.py';real='tools/research/incremental-compilation/real_pair_bench.py'
jobs=[('generated-default',shapes,[]),('generated-closed',shapes,['--mode','O2','--closed-executable']),('implementation-ablation',shapes,['--reference-zil','/workspace/.zil-env/research/target-stage4/release/zil','--mode','O0','--mode','O2']+sum((['--case',c] for c in ['large-128','large-512','large-2048','large-4096','large-8192','even-64x64','skew-8','body-16x16']),[])),('real-default',real,[]),('real-closed',real,['--mode','O2','--closed-executable'])]
now=lambda:datetime.datetime.now(zoneinfo.ZoneInfo('Asia/Shanghai')).isoformat()
records=[]
for name,tool,extra in jobs:
 cmd=['python3',tool,*base,*extra,'--out',str(r/name)]
 row={'window':name,'command':cmd,'started':now()};print('START',name,row['started'],flush=True)
 with (r/(name+'.log')).open('xb') as f:run=subprocess.run(cmd,cwd=root,stdout=f,stderr=subprocess.STDOUT)
 row.update(exit=run.returncode,finished=now());records.append(row)
 (r/'frozen-execution.json').write_text(json.dumps(records,indent=2)+'\n');print('FINISH',name,row['exit'],row['finished'],flush=True)
 if run.returncode!=0:raise SystemExit(run.returncode)
