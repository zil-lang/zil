from pathlib import Path
import json,hashlib,stat,collections,shutil
W=Path('/workspace');R=W/'zil-push-main';S=R/'out/evidence/full-push-checkpoint-32';P=R/'out/evidence/full-push-checkpoint-31'
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def key(e):return (e['sha256'],e['bytes'],e['mode'])
m=json.loads((S/'MASTER.json').read_text());old=json.loads((P/'MASTER.json').read_text());where={}
for shard in json.loads((P/'SHARDS.json').read_text())['shards']:
 proof=Path(shard['fresh_receipt']);assert sha(proof)==shard['fresh_receipt_sha256'] and json.loads(proof.read_text())['status']=='verified'
 for name in shard['regular_paths']:where[name]={k:shard[k] for k in ['fixed_archive_commit','archive_path','manifest_sha256','fresh_receipt_sha256']}
for a in json.loads((P/'REUSED-REGULAR.json').read_text())['files']:
 where[a['current']['path']]=a['stored']
catalog={}
for e in old['files']:
 loc=where[e['path']];stored={k:e[k] for k in ['path','sha256','bytes','mode']}
 if 'stored' in loc:loc=loc['stored']
 if 'path' in loc:stored={k:loc[k] for k in ['path','sha256','bytes','mode']}
 stored.update({k:loc[k] for k in ['fixed_archive_commit','archive_path','manifest_sha256']});stored['prior_actual_full_fresh_receipt_sha256']=loc.get('fresh_receipt_sha256',loc.get('prior_actual_full_fresh_receipt_sha256'))
 catalog.setdefault(key(e),{'stored':stored,'prior_raw_canonical':e['path']})
objects={}
for e in m['files']:
 k=key(e);objects.setdefault(k,{'sha256':e['sha256'],'bytes':e['bytes'],'mode':e['mode'],'logical_paths':[],'representative':e['path']})['logical_paths'].append(e['path'])
for k,o in objects.items():
 if k in catalog:o.update(storage='legacy-fixed-member',**catalog[k])
 else:o['storage']='new-exact-object'
summary={'logical_files':len(m['files']),'logical_bytes':m['logical_bytes'],'unique_objects':len(objects),'unique_object_bytes':sum(x['bytes'] for x in objects.values()),'prior_objects':sum(x['storage']=='legacy-fixed-member' for x in objects.values()),'prior_logical_paths':sum(len(x['logical_paths']) for x in objects.values() if x['storage']=='legacy-fixed-member'),'prior_object_bytes':sum(x['bytes'] for x in objects.values() if x['storage']=='legacy-fixed-member'),'new_objects':sum(x['storage']=='new-exact-object' for x in objects.values()),'new_unique_bytes':sum(x['bytes'] for x in objects.values() if x['storage']=='new-exact-object'),'free_bytes':shutil.disk_usage(W).free}
plan={'schema':'v32-exact-content-object-catalog-v1','master_sha256':sha(S/'MASTER.json'),'objects':list(objects.values()),'summary':summary,'physical_coalescence':'Only after actual fresh HTTPS complete archive/member proof, exact current SHA/size/mode/owner/dev and live-use checks; logical bytes/path/mode preserved, inode/ctime changes explicitly recorded.'}
with (S/'CAS-PLAN.json').open('x') as f:json.dump(plan,f,sort_keys=True,separators=(',',':'));f.write('\n')
print(json.dumps(summary),flush=True)
