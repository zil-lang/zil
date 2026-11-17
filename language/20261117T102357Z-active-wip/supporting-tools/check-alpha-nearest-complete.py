from lossless_phase_logs import compress_phases
import argparse, concurrent.futures, hashlib, json, pathlib, subprocess
p=argparse.ArgumentParser();p.add_argument('--repo',type=pathlib.Path,required=True);p.add_argument('--run',required=True);p.add_argument('--work-profile',choices=['standard','alpha'],default='alpha');a=p.parse_args()
b=a.repo/'out/runs'/a.run;identity=json.loads((b/'compiler-build-receipt.json').read_text());cli=b/'complete-original-checks/zil'
assert identity['status']=='Pass' and hashlib.sha256(cli.read_bytes()).hexdigest()==identity['cli_sha256']
out=b/'nearest-complete-originals';out.mkdir(exist_ok=False)
manifests=[pathlib.Path('/tmp')/name/'cases.json' for name in ['zil-owned-rvalue-and-call-domain-nearest','zil-shared-reborrow-nearest','zil-call-region-source-view-nearest','zil-callable-quantifier-nearest','zil-borrowed-closure-child-nearest','zil-inferred-record-mixed-nearest','zil-nominal-result-outside-nearest']]
cases=[]
for manifest in manifests:
 for case in json.loads(manifest.read_text()):
  case=dict(case)
  if 'expected' not in case: case['expected']='pass' if case['positive'] else 'reject'
  cases.append(dict(case,manifest=str(manifest)))
cases.extend([{'source':'/workspace/.zil-env/nested-address-source-controls/inspect.zil','expected':'pass'},{'source':'/workspace/.zil-env/nested-address-source-controls/bad-slot-escape.zil','expected':'reject'}])
cases.extend([{'source':'/tmp/zil-nominal-result-outside-nearest/existing-exact-result.option-input-unsupported.zil','expected':'pass'}])
cases.extend(json.loads(pathlib.Path('/tmp/zil-raw-reference-source-controls/cases.json').read_text())['rows'])
cases.extend([{'source':'/tmp/zil-raw-reference-write-controls/replace-pointer-cell.zil','expected':'pass'}])
def one(pair):
 index,c=pair;source=pathlib.Path(c['source']);d=out/f'{index:02d}-{source.stem}';d.mkdir();cmd=[str(cli),'check',str(source),'--work-profile',a.work_profile,'--emit-dir',str(d/'phases'),'--json']
 with (d/'stdout.json').open('w') as stdout,(d/'stderr.log').open('w') as stderr:r=subprocess.run(cmd,cwd=a.repo,stdout=stdout,stderr=stderr,timeout=180)
 try:v=json.loads((d/'stdout.json').read_text())
 except ValueError:v={}
 passed=(r.returncode==0 and v.get('status')=='Success' and v.get('verification_runs')==1) if c['expected']=='pass' else (r.returncode==1 and v.get('status')=='SafeReject' and any(d.get('phase') in ('expand','types','analyze','verify') for d in v.get('diagnostics',[])))
 row=dict(c,source_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),command=cmd,exit=r.returncode,status=v.get('status'),verification_runs=v.get('verification_runs'),diagnostics=v.get('diagnostics'),work_profile=v.get('work_profile'),effective_limits=v.get('effective_limits'),passed=passed and v.get('work_profile')==a.work_profile)
 compress_phases(d)
 print(json.dumps({k:row.get(k)for k in ['source','expected','status','passed','diagnostics']}),flush=True);return row
with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:rows=list(pool.map(one,enumerate(cases)))
(out/'receipt.json').write_text(json.dumps(dict(candidate=identity['source_commit'],cli_sha256=identity['cli_sha256'],rows=rows),indent=2))
raise SystemExit(0 if rows and all(row['passed'] for row in rows) else 1)
