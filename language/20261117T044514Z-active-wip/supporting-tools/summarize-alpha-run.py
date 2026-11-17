import json,pathlib,sys,re
from collections import Counter
p=pathlib.Path('/workspace/zil-alpha-candidate/out/runs')/sys.argv[1]
identity_path=p/'compiler-build-receipt.json'
identity=json.loads(identity_path.read_text()) if identity_path.exists() else None
evidence_failed=False
for directory,pass_key in [('alpha-coverage','pass'),('complete-original-checks','pass'),('nearest-complete-originals','passed'),('focused-raw-complete','pass_source_check'),('all-task-helper-native','passed'),('task-frame-observer','passed')]:
 f=p/directory/'receipt.json'
 if not f.exists():continue
 receipt=json.loads(f.read_text());rows=receipt['rows']
 evidence_failed |= not rows or any(row[pass_key] is not True for row in rows)
 if identity:
  assert receipt['candidate']==identity['source_commit'],str(f)
  assert receipt['cli_sha256']==identity['cli_sha256'],str(f)
 print(json.dumps({'row_verified_suite':directory,'candidate':receipt['candidate'],'passed':sum(row[pass_key] is True for row in rows),'total':len(rows),'failed_cases':[row.get('case',row.get('family',row.get('source',row.get('path'))))for row in rows if row[pass_key] is not True]},ensure_ascii=False))
for directory in ['all-task-helper-sources','original-task-positive-fixtures']:
 f=p/directory/'receipt.json'
 if not f.exists():continue
 receipt=json.loads(f.read_text());rows=receipt['rows']
 if identity:
  assert receipt['candidate']==identity['source_commit'],str(f)
  assert receipt['cli_sha256']==identity['cli_sha256'],str(f)
 verified=lambda row:row.get('status')=='Success' and row.get('verification_runs')==1
 evidence_failed |= not rows or any(not verified(row)for row in rows)
 print(json.dumps({'diagnostic_source_census':directory,'candidate':receipt['candidate'],'verified':sum(verified(row)for row in rows),'total':len(rows),'remaining_by_phase':dict(Counter(row.get('phase','unknown')for row in rows if not verified(row)))},ensure_ascii=False))
f=p/'task-owner-complete/failure-inventory.json'
if f.exists():
 inventory=json.loads(f.read_text())
 evidence_failed |= bool(inventory['rows'])
 if identity:assert inventory['candidate']==identity['source_commit'],str(f)
 print(json.dumps({'sdk_failure_inventory':str(f),'candidate':inventory['candidate'],'total_failed':len(inventory['rows']),'actual_failed_by_phase':dict(Counter(row['phase']for row in inventory['rows']))},ensure_ascii=False))
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
print(json.dumps({'observed_failures':evidence_failed,'alpha_acceptance_claim':False}))
raise SystemExit(1 if evidence_failed else 0)
