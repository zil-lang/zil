#!/usr/bin/env python3
import hashlib,json,tarfile
from pathlib import Path,PurePosixPath
ROOT=Path('/workspace/.zil-env/research/stage5')
DELIVERY=Path('/workspace/zil/evidence/research/incremental-compilation/stage5')
W=['generated-default','generated-closed','implementation-ablation','real-default','real-closed']
inventory=json.loads((ROOT/'data-review-inventory.json').read_text())['files']
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
results=[]
for name in W:
 d=DELIVERY/name;a=d/'raw.tar.gz';receipt=json.loads((d/'raw.tar.gz.json').read_text())
 assert sha(a)==receipt['sha256'] and a.stat().st_size==receipt['bytes']
 observed={};embedded=None
 with tarfile.open(a,'r|gz') as t:
  for m in t:
   assert m.isfile() and not PurePosixPath(m.name).is_absolute() and '..' not in PurePosixPath(m.name).parts
   assert m.name not in observed
   data=t.extractfile(m).read()
   if m.name=='FILES.json':embedded=json.loads(data);continue
   observed[m.name]={'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()}
   local=ROOT/m.name
   known=inventory.get(str(local))
   if known is None:known={'bytes':local.stat().st_size,'sha256':sha(local)}
   assert known==observed[m.name],m.name
 assert embedded is not None
 listed={r['path']:{'bytes':r['bytes'],'sha256':r['sha256']} for r in embedded['files']}
 assert len(listed)==len(embedded['files']) and observed==listed
 assert len(observed)==receipt['file_count'] and sum(v['bytes'] for v in observed.values())==receipt['uncompressed_bytes']
 for file in ['manifest.json','summary.json']:
  assert sha(d/file)==sha(ROOT/name/file)
 result={'window':name,'files':len(observed),'uncompressed_bytes':receipt['uncompressed_bytes'],'archive_sha256':receipt['sha256'],'receipt_sha256':sha(d/'raw.tar.gz.json'),'embedded_files_sha256':hashlib.sha256(json.dumps(embedded,sort_keys=True).encode()).hexdigest()}
 results.append(result);print(name,len(observed),'Pass',flush=True)
assert sha(DELIVERY/'analysis.json')==sha(ROOT/'analysis/analysis.json')
(ROOT/'data-review-archives.json').write_text(json.dumps({'pass':True,'files':sum(r['files'] for r in results),'windows':results},indent=2)+'\n')
print('PASS',sum(r['files'] for r in results),flush=True)
