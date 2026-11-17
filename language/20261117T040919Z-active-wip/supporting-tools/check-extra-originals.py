from lossless_phase_logs import compress_phases
import pathlib,json,hashlib,subprocess,concurrent.futures,argparse,shutil
p=argparse.ArgumentParser();p.add_argument('--repo',type=pathlib.Path,required=True);p.add_argument('--run',required=True);p.add_argument('--work-profile',choices=['standard','alpha'],default='standard');p.add_argument('--jobs',type=int,choices=range(1,5),default=4);a=p.parse_args();base=a.repo/'out/runs'/a.run
receipt=json.loads((base/'compiler-build-receipt.json').read_text());head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=a.repo,text=True).strip();assert receipt['source_commit']==head and receipt['status']=='Pass'
cli=pathlib.Path('/workspace/.zil-env/target-language-alpha/debug/zil');assert hashlib.sha256(cli.read_bytes()).hexdigest()==receipt['cli_sha256']
out=base/'complete-original-checks';out.mkdir(exist_ok=False);frozen=out/'zil';shutil.copyfile(cli,frozen);frozen.chmod(0o755)
paths=[('dyn-context/'+x,a.repo/'crates/zil-verify/tests/fixtures/dyn-context'/f'{x}.zil')for x in ['context-read','context-factory','context-unused','short-external-view','short-inline-view']]
paths += [('dyn-context/static-callback-getter-cycle',a.repo/'tests/fixtures/dyn-context/static-callback-getter-cycle-control.zil')]
paths += [('returned-environment-sources/'+x,a.repo/'tests/returned-environment-sources'/f'{x}.zil')for x in ['two-addresses','owned-projected-multiple','holder-pointer','holder-pointer-replaced','nested-borrowed','wrapped-borrowed','loop-sources']]
paths += [('loan_control',a.repo/'tests/borrowed-callable-arguments/loan_control.zil')]
def run(row):
 label,source=row;folder=out/label;folder.mkdir(parents=True);cmd=['bash','-c','source /workspace/.zil-env/language-env.sh; exec "$@"','alpha-original-check',str(frozen),'check',str(source),'--work-profile',a.work_profile,'--emit-dir',str(folder/'phases'),'--json']
 with (folder/'stdout.json').open('w')as stdout,(folder/'stderr.log').open('w')as stderr:r=subprocess.run(cmd,cwd=a.repo,stdout=stdout,stderr=stderr,timeout=180)
 try:v=json.loads((folder/'stdout.json').read_text())
 except ValueError:v={}
 result={'case':label,'source':str(source.relative_to(a.repo)),'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'exit':r.returncode,'status':v.get('status'),'verification_runs':v.get('verification_runs',0),'work_profile':v.get('work_profile'),'work_profile_revision':v.get('work_profile_revision'),'effective_limits':v.get('effective_limits'),'pass':v.get('work_profile')==a.work_profile and r.returncode==0 and v.get('status')=='Success' and isinstance(v.get('verification_runs'),int) and v['verification_runs']>0,'diagnostics':v.get('diagnostics',[])}
 compress_phases(folder)
 print(json.dumps({'case':label,'exit':r.returncode,'status':v.get('status'),'diagnostics':[(d['phase'],d['message'][:200])for d in v.get('diagnostics',[])]}),flush=True);return result
with concurrent.futures.ThreadPoolExecutor(max_workers=a.jobs)as pool:rows=list(pool.map(run,paths))
(out/'receipt.json').write_text(json.dumps({'candidate':head,'cli_sha256':receipt['cli_sha256'],'scope':'Original complete Source through independent verification; native acceptance separate','rows':rows},indent=2))
raise SystemExit(0 if rows and all(row['pass'] for row in rows) else 1)
