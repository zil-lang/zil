import json,pathlib,sys,re
from collections import Counter
p=pathlib.Path('/workspace/zil-alpha-candidate/out/runs')/sys.argv[1]
identity_path=p/'compiler-build-receipt.json'
identity=json.loads(identity_path.read_text()) if identity_path.exists() else None
evidence_failed=False
for directory,pass_key in [('alpha-coverage','pass'),('complete-original-checks','pass'),('nearest-complete-originals','passed'),('focused-raw-complete','pass_source_check'),('all-task-helper-native','passed'),('task-frame-observer','passed'),('existing-direct-cleanup-native','passed'),('original-simple-task-native','passed')]:
 f=p/directory/'receipt.json'
 if not f.exists():continue
 receipt=json.loads(f.read_text());rows=receipt['rows']
 evidence_failed |= not rows or any(row[pass_key] is not True for row in rows)
 if identity:
  assert receipt['candidate']==identity['source_commit'],str(f)
  assert receipt['cli_sha256']==identity['cli_sha256'],str(f)
 print(json.dumps({'row_verified_suite':directory,'candidate':receipt['candidate'],'passed':sum(row[pass_key] is True for row in rows),'total':len(rows),'failed_cases':[row.get('case',row.get('family',row.get('source',row.get('path'))))for row in rows if row[pass_key] is not True]},ensure_ascii=False))
for directory in ['all-task-helper-sources','root-all-task-helper-sources','original-task-positive-fixtures','original-task-positive-fixtures-alpha']:
 f=p/directory/'receipt.json'
 if not f.exists():continue
 receipt=json.loads(f.read_text());rows=receipt['rows']
 if identity:
  assert receipt['candidate']==identity['source_commit'],str(f)
  assert receipt['cli_sha256']==identity['cli_sha256'],str(f)
 verified=lambda row:row.get('status')=='Success' and row.get('verification_runs')==1 and row.get('verified',True) is True and row.get('exit_code',row.get('exit',0))==0
 evidence_failed |= not rows or any(not verified(row)for row in rows)
 print(json.dumps({'diagnostic_only':True,'diagnostic_source_census':directory,'candidate':receipt['candidate'],'verified':sum(verified(row)for row in rows),'total':len(rows),'remaining_by_phase':dict(Counter(row.get('phase','unknown')for row in rows if not verified(row)))},ensure_ascii=False))
f=p/'task-owner-complete/failure-inventory.json'
if f.exists():
 inventory=json.loads(f.read_text())
 evidence_failed |= bool(inventory['rows'])
 if identity:assert inventory['candidate']==identity['source_commit'],str(f)
 print(json.dumps({'sdk_failure_inventory':str(f),'candidate':inventory['candidate'],'total_failed':len(inventory['rows']),'actual_failed_by_phase':dict(Counter(row['phase']for row in inventory['rows']))},ensure_ascii=False))
for directory in ('root-foundation','root-foundation-alpha'):
 f=p/directory/'receipt.json'
 if not f.exists():continue
 report=json.loads(f.read_text());rows=report['rows']
 if identity:assert report['candidate']==identity['source_commit'] and report['cli_sha256']==identity['cli_sha256'],str(f)
 evidence_failed |= not rows or any(row.get('pass')is not True for row in rows)
 print(json.dumps({'foundation_source_suite':directory,'work_profile':report['work_profile'],'positive_passed':sum(row['pass']for row in rows if row['group']=='positive_cases'),'positive_total':sum(row['group']=='positive_cases'for row in rows),'negative_passed':sum(row['pass']for row in rows if row['group']=='negative_cases'),'negative_total':sum(row['group']=='negative_cases'for row in rows)}))
f=p/'core-coverage/receipt.json'
if f.exists():
 r=json.loads(f.read_text());print(json.dumps({'candidate':r['candidate'],'positive_passed':sum(x['pass']for x in r['rows']if x['group']=='positive_cases'),'positive_total':sum(x['group']=='positive_cases'for x in r['rows']),'negative_passed':sum(x['pass']for x in r['rows']if x['group']=='negative_cases')},ensure_ascii=False))
 for x in r['rows']:
  if not x['pass']:
   print(json.dumps({'case':x['family'],'diagnostics':[dict(phase=d['phase'],category=d['category'],code=d['code'],message=d['message'][:700],span=d.get('span'),notes=d.get('notes',[])[:3])for d in x['diagnostics']]},ensure_ascii=False))
for name in ['original-app.json','extra-source.json']:
 f=p/name
 if f.exists():
  r=json.loads(f.read_text());print(json.dumps({'case':name,'status':r['status'],'diagnostics':[dict(phase=d['phase'],category=d['category'],code=d['code'],message=d['message'][:700],span=d.get('span'),notes=[n[:1000]for n in d.get('notes',[])[:3]])for d in r['diagnostics']]},ensure_ascii=False))
for f in p.glob('*/test-receipt.json'):
 report=json.loads(f.read_text())
 rows=report if isinstance(report,list) else report.get('rows',[report])
 assert isinstance(rows,list) and all(isinstance(row,dict)for row in rows),str(f)
 print(json.dumps({'test_receipt':str(f),'rows':[{k:r.get(k)for k in ['family','target','status','selected_executed','passed_tests','failed_tests']}for r in rows]},ensure_ascii=False))
for f in p.glob('**/manifest.json'):
 try:r=json.loads(f.read_text())
 except (ValueError,OSError):continue
 if r.get('authority')!='UntrustedSourceAndProducerCandidates; independent raw/Wire verification required':continue
 print(json.dumps({'source_manifest':str(f),'generated':len(r.get('records',[])),'sources':[x['source']for x in r.get('records',[])]},ensure_ascii=False))
 for e in sorted(f.parent.rglob('*.errors.txt')):
  s=e.read_text();print(json.dumps({'source_case':str(e.relative_to(f.parent)),'stage':s.splitlines()[0],'messages':re.findall(r'message: "([^"]*)"',s)[:2]},ensure_ascii=False))
# Separate command completion from actual selected assertions and Source verification.
required=['compiler-build-receipt.json','alpha-coverage/receipt.json','original-task-positive-fixtures/receipt.json','original-task-positive-fixtures-alpha/receipt.json','all-task-helper-sources/receipt.json','task-owner-complete/test-receipt.json','existing-direct-cleanup-native/receipt.json','original-simple-task-native/receipt.json']
alternatives={'alpha-coverage/receipt.json':'root-foundation-alpha/receipt.json','all-task-helper-sources/receipt.json':'root-all-task-helper-sources/receipt.json'}
missing=[name for name in required if not (p/name).exists() and not (name in alternatives and (p/alternatives[name]).exists())]
if identity:evidence_failed |= identity.get('status')!='Pass'
actual_tests=[]
for label in ('task-owner-complete','source-integrations','source-static-controls','address-product-controls','frame-consumer-controls'):
 log=p/label/'test.log';receipt_file=p/label/'test-receipt.json'
 if not log.exists() or not receipt_file.exists():missing.append(label+'/actual-test-evidence');continue
 report=json.loads(receipt_file.read_text());found=re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',log.read_text())
 counts=[sum(int(x[i])for x in found)for i in range(3)];code=report.get('exit_code',report.get('exit'))
 passed=bool(found) and counts[0]>0 and counts[1]==0 and counts[2]==0 and code==0
 evidence_failed |= not passed
 row={'suite':label,'executed_passed':counts[0],'executed_failed':counts[1],'ignored':counts[2],'command_exit':code,'passed':passed};actual_tests.append(row);print(json.dumps({'actual_tests':row}))
core=p/'core-libraries/receipt.json'
if core.exists():
 report=json.loads(core.read_text());core_rows=report.get('rows',[])
 for row in core_rows:evidence_failed |= row.get('exit')!=0 or row.get('failed',0)!=0 or row.get('passed',0)<=0 or row.get('ignored',0)!=0
 print(json.dumps({'core_actual_rows':core_rows}))
actual_native=[]
for native_directory in ('existing-direct-cleanup-native','original-simple-task-native'):
 native=p/native_directory/'receipt.json'
 if not native.exists():continue
 report=json.loads(native.read_text());rows=report.get('rows',[]);executions=[x for row in rows for x in row.get('executions',[])]
 if identity:
  assert report['candidate']==identity['source_commit'],str(native)
  assert report['cli_sha256']==identity['cli_sha256'],str(native)
 ok=bool(rows) and bool(executions) and not report.get('skipped') and all(row.get('passed') is True and row.get('build_exit')==0 and bool(row.get('executions')) for row in rows) and all(x.get('passed') is True for x in executions)
 evidence_failed |= not ok
 row={'suite':native_directory,'builds_passed':sum(row.get('build_exit')==0 for row in rows),'builds_total':len(rows),'executions_passed':sum(x.get('passed') is True for x in executions),'executions_total':len(executions),'skipped_cases':[x.get('case')for x in report.get('skipped',[])],'passed':ok};actual_native.append(row);print(json.dumps({'actual_native':row}))
preservation=None;policy=pathlib.Path('/workspace/.zil-env/root-recovered-task-success-baseline.json')
if policy.exists():
 baseline=json.loads(policy.read_text());profiles={}
 for profile,contract in baseline['profiles'].items():
  current=p/('original-task-positive-fixtures' if profile=='standard' else 'original-task-positive-fixtures-alpha')/'receipt.json'
  if not current.exists():
   profiles[profile]={'passed':False,'missing_receipt':str(current)};evidence_failed=True;continue
  report=json.loads(current.read_text())
  if identity:assert report['candidate']==identity['source_commit'] and report['cli_sha256']==identity['cli_sha256'],str(current)
  actual={row['case']:row for row in report['rows']};expected={row['case']:row['source_sha256']for row in contract['required_cases']}
  same=all(name in actual and actual[name]['source_sha256']==sha for name,sha in expected.items())
  successes={name for name,row in actual.items()if row.get('status')=='Success'and row.get('verification_runs')==1 and row.get('verified',True)is True and row.get('exit_code',row.get('exit',0))==0}
  required_successes=set(expected);lost=sorted(required_successes-successes);evidence_failed |= not same or bool(lost)
  profiles[profile]={'source_sha_identities_preserved':same,'required_preserved_successes':sorted(required_successes),'regressions':lost,'new_successes':sorted(successes-required_successes),'current_successes':sorted(successes),'passed':same and not lost}
 preservation={'policy':str(policy),'profiles':profiles,'passed':all(row['passed']for row in profiles.values())};print(json.dumps({'success_set_preservation':preservation}))
aggregate={'candidate':identity.get('source_commit')if identity else None,'cli_sha256':identity.get('cli_sha256')if identity else None,'scope':'Selected Task Source/SDK/native batch only; product/CT/project/core/full native plan/recovery gates remain separate','command_exit_is_not_acceptance':True,'observed_failures':bool(evidence_failed),'missing_required_evidence':sorted(set(missing)),'selected_batch_passed':not evidence_failed and not missing,'success_set_preservation':preservation,'actual_tests':actual_tests,'actual_native':actual_native,'alpha_acceptance_claim':False}
(p/'row-verified-batch-acceptance.json').write_text(json.dumps(aggregate,indent=2)+'\n');evidence_failed |= bool(missing)
print(json.dumps(aggregate))
print(json.dumps({'observed_failures':evidence_failed,'alpha_acceptance_claim':False}))
raise SystemExit(1 if evidence_failed else 0)
