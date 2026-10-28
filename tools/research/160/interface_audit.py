"""Read-only opportunity audit. Equal observed interfaces are NOT a reuse proof."""
import argparse,pathlib,json,hashlib
p=argparse.ArgumentParser();p.add_argument('--campaign',required=True);p.add_argument('--out',required=True);a=p.parse_args();root=pathlib.Path(a.campaign);out=pathlib.Path(a.out);out.mkdir(parents=True,exist_ok=False)
spec=json.loads((root/'frozen158.json').read_text());rows=[]
for case in spec['cases']:
 if case['weight']<=0:continue
 file=root/'preflight'/(case['name']+'-A')/'report.json';report=json.loads(file.read_text());prev=None
 for request in report['records']:
  current={x['module']:x for x in request['report'].get('scalar_interfaces',[])}
  if prev is not None and request['scenario'].endswith('_edit'):
   same=[];changed=[];body_changed_interface_same=[];missing=[]
   for key in sorted(set(prev)|set(current)):
    if key not in prev or key not in current:missing.append(key);continue
    old=prev[key];new=current[key]
    equal=all(old.get(k)==new.get(k) for k in ['signature_fingerprint','contract_fingerprint','exports'])
    (same if equal else changed).append(key)
    if equal and old.get('implementation_fingerprint')!=new.get('implementation_fingerprint'):body_changed_interface_same.append(key)
   rows.append({'case':case['name'],'scenario':request['scenario'],'observed_modules':len(current),'same_public_signature_contract_and_exports':len(same),'changed_public_contract':changed,'missing_or_new':missing,'body_changed_interface_same':body_changed_interface_same,'report_sha256':hashlib.sha256(file.read_bytes()).hexdigest()})
  prev=current
summary={'status':'ObservedOnly_NotProductionEligible','edits':len(rows),'opportunity_edits':sum(bool(x['body_changed_interface_same'])for x in rows),'rows':rows,'not_proved':['stable logical symbol identity across function/module renumbering','diagnostic/source-map remapping','generic specialization and private lookup dependencies','resource origin and backing identity completeness','all quota/target/rule axes','cost of relinking and independently verifying reused fragments'],'cannot_claim':'No caller reuse implemented, no new speedup, no implication that equal observed scalar interfaces suffice for safety or semantic equivalence.'};(out/'summary.json').write_text(json.dumps(summary,indent=2));print(json.dumps(summary,indent=2))
