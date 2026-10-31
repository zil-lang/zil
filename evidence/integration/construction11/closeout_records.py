from pathlib import Path
from collections import Counter
import argparse, datetime, gzip, hashlib, json, subprocess

p=argparse.ArgumentParser()
p.add_argument('--repo',type=Path,required=True)
p.add_argument('--baseline',type=Path,required=True)
p.add_argument('--out',type=Path,required=True)
a=p.parse_args(); repo=a.repo.resolve(); out=a.out.resolve()
assert not out.exists(), 'Preserve previous check outputs; choose a new directory.'
assert not subprocess.check_output(['git','-C',str(repo),'status','--porcelain'],text=True).strip()
out.mkdir(parents=True)
head=subprocess.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip()
baseline_bytes=a.baseline.read_bytes(); baseline=json.loads(baseline_bytes)
(out/'baseline-handoff.json.gz').write_bytes(gzip.compress(baseline_bytes,mtime=0))
commands=[]; outputs={}
for name,cmd in [('check-plan',['python3','tools/check-plan.py','--self-test']),('handoff',['node','tools/handoff.mjs'])]:
    started=datetime.datetime.now(datetime.timezone.utc).isoformat()
    proc=subprocess.run(cmd,cwd=repo,capture_output=True)
    outputs[name]=proc
    (out/(name+'.stdout.gz')).write_bytes(gzip.compress(proc.stdout,mtime=0))
    (out/(name+'.stderr')).write_bytes(proc.stderr)
    commands.append({'name':name,'argv':cmd,'checked_commit':head,'started_at':started,'completed_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'exit_code':proc.returncode})
live=json.loads(outputs['handoff'].stdout)
def bag(errors): return Counter(json.dumps(e,sort_keys=True) for e in errors)
before=bag(baseline['errors']); after=bag(live['errors'])
added=[json.loads(x) for x in (after-before).elements()]
removed=[json.loads(x) for x in (before-after).elements()]
task_errors={i:[e for e in live['errors'] if e['id'] in [i,'.task/impl/'+i+'.md']] for i in ['232','233','234']}
result={'schema_version':1,'status':live['status'],'scope':'Live handoff record integrity; separate from compiler CI and bounded language acceptance.','checked_commit':head,'baseline_commit':baseline['main_head'],'commands':commands,'errors':len(live['errors']),'warnings':len(live['warnings']),'baseline_errors':len(baseline['errors']),'added_errors':added,'removed_errors':removed,'new_task_errors':task_errors,'three_task_records_pass':not any(task_errors.values()),'global_record_integrity_pass':live['status']=='Pass','check_plan_pass':outputs['check-plan'].returncode==0,'spec_hash':live['spec_hash'],'normative_hash':live['normative_hash'],'baseline_sha256':hashlib.sha256(baseline_bytes).hexdigest(),'file_hashes':{f.name:hashlib.sha256(f.read_bytes()).hexdigest() for f in sorted(out.iterdir()) if f.is_file()}}
(out/'RESULT.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({k:v for k,v in result.items() if k not in ['commands','file_hashes']},indent=2))
assert result['check_plan_pass'] and not added and not removed and result['three_task_records_pass'] and result['warnings']==0
