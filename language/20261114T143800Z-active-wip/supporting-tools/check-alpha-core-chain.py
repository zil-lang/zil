#!/usr/bin/env python3
"""Run the first real Alpha chains against one immutable source candidate."""
import argparse,hashlib,json,re,subprocess,time
from pathlib import Path
ap=argparse.ArgumentParser();ap.add_argument('--repo',type=Path,required=True);ap.add_argument('--cli',type=Path,required=True);ap.add_argument('--out',type=Path,required=True);args=ap.parse_args()
args.out.mkdir(parents=True,exist_ok=True)
sha=subprocess.check_output(['git','-C',str(args.repo),'rev-parse','HEAD'],text=True).strip()
sources=['tests/fixtures/semantic-scopes/ordinary-generic.zil','tests/fixtures/semantic-scopes/local-closure.zil','tests/fixtures/returned-owning-holders/semantic-only-phantom-capture.zil','tests/fixtures/semantic-scopes/unused-generic-identity.zil']
rows=[]
for source in sources:
 for backend in ['clang','llvm-api']:
  for opt in ['O0','O2']:
   dst=args.out/(Path(source).stem+'-'+backend+'-'+opt);dst.mkdir(parents=True,exist_ok=True)
   cmd=[str(args.cli),'build',str(args.repo/source),'--backend',backend,'--opt',opt,'--emit-dir',str(dst/'phases'),'-o',str(dst/'program'),'--json']
   run=subprocess.run(cmd,cwd=args.repo,text=True,capture_output=True,timeout=120)
   (dst/'compiler.stdout').write_text(run.stdout);(dst/'compiler.stderr').write_text(run.stderr)
   row={'source':source,'source_sha256':hashlib.sha256((args.repo/source).read_bytes()).hexdigest(),'backend':backend,'optimization':opt,'command':cmd,'compile_exit':run.returncode}
   try: report=json.loads(run.stdout)
   except json.JSONDecodeError: report={}
   row['status']=report.get('status','InvalidCompilerReport');row['diagnostics']=report.get('diagnostics',[])
   row['verification_runs']=report.get('verification_runs')
   row['source_semantic_some']=False;row['typed_semantic_some']=False;row['source_original_body']=False;row['typed_original_body']=False
   for phase,key in [('core','source_semantic_some'),('typed','typed_semantic_some')]:
    file=dst/'phases'/(phase+'.txt')
    if file.exists():
     dump=file.read_text();row[key]=bool(re.search(r'semantic:\s*Some\(',dump));row['source_original_body' if phase=='core' else 'typed_original_body']='FunctionWithBody {' in dump and not bool(re.search(r'body:\s*Function\s*\{',dump))
   codegen=report.get('native_codegen') or {};row['actual_backend']=codegen.get('backend');row['fallback']=codegen.get('fallback')
   row['pass']=False
   if run.returncode==0 and row['status']=='Success' and row['verification_runs']==1 and row['source_semantic_some'] and row['typed_semantic_some'] and row['source_original_body'] and row['typed_original_body'] and row['actual_backend']==backend and row['fallback'] is None:
    native=subprocess.run([str(dst/'program')],text=True,capture_output=True,timeout=10)
    row.update(native_exit=native.returncode,stdout=native.stdout,stderr=native.stderr)
    row['pass']=native.returncode==0 and not native.stdout and not native.stderr
   rows.append(row)
   print(json.dumps({k:row[k] for k in ['source','backend','optimization','status','source_semantic_some','typed_semantic_some','pass']}),flush=True)
receipt={'candidate':sha,'cli_sha256':hashlib.sha256(args.cli.read_bytes()).hexdigest(),'scope':'Original generic-body/closure feature chains, not all Alpha coverage','rows':rows,'passed':sum(r['pass'] for r in rows),'failed':sum(not r['pass'] for r in rows)}
(args.out/'receipt.json').write_text(json.dumps(receipt,ensure_ascii=False,indent=2)+'\n')
raise SystemExit(0 if receipt['failed']==0 else 1)
