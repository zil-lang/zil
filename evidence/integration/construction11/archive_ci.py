from pathlib import Path
import argparse, gzip, hashlib, json, shutil

p=argparse.ArgumentParser()
p.add_argument('source', type=Path)
p.add_argument('destination', type=Path)
a=p.parse_args(); src=a.source; dst=a.destination
assert (src/'AUDIT.json').is_file()
audit=json.loads((src/'AUDIT.json').read_text())
dst.mkdir(parents=True,exist_ok=True)
inventory=[]
for name,expected in audit['receipt_hashes'].items():
    f=src/'final/receipts'/name; data=f.read_bytes()
    assert len(data)==expected['bytes'] and hashlib.sha256(data).hexdigest()==expected['sha256'],name
    compressed=len(data)>100000
    out=dst/(name+'.gz' if compressed else name)
    archived=gzip.compress(data,mtime=0) if compressed else data
    assert not out.exists() or out.read_bytes()==archived, 'Refusing to overwrite different original receipt: '+name
    out.write_bytes(archived)
    inventory.append({'original':name,'stored':out.name,'bytes':len(data),'sha256':expected['sha256'],'stored_bytes':len(archived),'stored_sha256':hashlib.sha256(archived).hexdigest()})
for pattern in ['*-download-receipt.json','SELECTED-*.json','jobs.json','artifacts.json','run.json']:
    for f in src.glob(pattern):
        target=dst/f.name
        assert not target.exists() or target.read_bytes()==f.read_bytes(),f.name
        shutil.copyfile(f,target)
audit['archived_receipts']=inventory
audit['repository_bundle']='Retained in the original GitHub artifact and verified local artifact; not embedded recursively inside Git.'
audit['run_url']=f"https://github.com/zil-lang/zil/actions/runs/{audit['run_id']}"
out=dst/'RESULT.json'
assert not out.exists(), 'Use a new immutable result location'
out.write_text(json.dumps(audit,indent=2)+'\n')
print(json.dumps({'status':audit['status'],'run_id':audit['run_id'],'files':len(inventory),'destination':str(dst)}))
