import base64,concurrent.futures,datetime,hashlib,json,pathlib,subprocess
repo=pathlib.Path('/workspace/zil-public-integration')
def git(*a):return subprocess.check_output(['git','-C',str(repo),*a])
def api(path,data=None,method=None):
 cmd=['gh','api',path]
 if method:cmd+=['--method',method]
 if data is not None:cmd+=['--input','-']
 p=subprocess.run(cmd,input=None if data is None else json.dumps(data).encode(),capture_output=True)
 if p.returncode:raise RuntimeError(p.stderr.decode()[:1200])
 return json.loads(p.stdout)
def files():return sorted(set(git('ls-files','-m','--others','--exclude-standard','--','crates').decode().splitlines()))
for attempt in range(5):
 head=git('rev-parse','HEAD').decode().strip(); names=files(); contents={n:(repo/n).read_bytes() for n in names}; patch=git('diff','--binary','HEAD','--','crates'); untracked=git('ls-files','--others','--exclude-standard','--','crates').decode().splitlines()
 if names==files() and head==git('rev-parse','HEAD').decode().strip() and all((repo/n).read_bytes()==v for n,v in contents.items()):break
else:raise RuntimeError('Writers did not yield a stable WIP snapshot; no archive published')
stamp=datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ'); prefix='language/'+stamp+'-active-wip'; dest=pathlib.Path('/workspace/.zil-env/language-wip-snapshot-'+stamp);dest.mkdir()
artifacts={'source.patch':patch}
for name in untracked:artifacts['untracked/'+name]=contents[name]
for name in ('run-alpha-candidate.sh', 'check-alpha-core-chain.py', 'run-native-alpha.py', 'run-native-alpha-plan.json', 'run-native-alpha-manual.txt', 'backup-language-wip.py'):
 p=pathlib.Path('/workspace/.zil-env')/name
 if p.exists():artifacts['supporting-tools/'+name]=p.read_bytes()
manifest={'source_base':head,'state':'UNVERIFIED ACTIVE WIP; not an accepted candidate','snapshot_utc':stamp,'stable_read_check':True,'changed_files':{n:hashlib.sha256(v).hexdigest() for n,v in contents.items()},'artifacts':{n:hashlib.sha256(v).hexdigest() for n,v in artifacts.items()},'restore':'Checkout source_base, apply source.patch, copy untracked/ contents into the same checkout.'}
artifacts['manifest.json']=(json.dumps(manifest,indent=2)+'\n').encode()
for name,data in artifacts.items():p=dest/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(data)
base='repos/zil-lang/zil/git/'
def blob(item):
 name,data=item;r=api(base+'blobs',{'encoding':'base64','content':base64.b64encode(data).decode()},'POST');return {'path':prefix+'/'+name,'mode':'100644','type':'blob','sha':r['sha']}
with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:entries=list(pool.map(blob,artifacts.items()))
for attempt in range(4):
 parent=api(base+'ref/heads/archive/source-history')['object']['sha'];tree=api(base+'commits/'+parent)['tree']['sha'];newtree=api(base+'trees',{'base_tree':tree,'tree':entries},'POST')['sha'];commit=api(base+'commits',{'message':'Preserve active language Alpha WIP with stable snapshot and restoration manifest','tree':newtree,'parents':[parent,head]},'POST')['sha']
 try:api(base+'refs/heads/archive/source-history',{'sha':commit,'force':False},'PATCH');break
 except RuntimeError:
  if attempt==3:raise
for name in ('manifest.json','source.patch'):
 entry=next(e for e in entries if e['path']==prefix+'/'+name);actual=api(base+'blobs/'+entry['sha']);assert base64.b64decode(actual['content'])==artifacts[name]
receipt={'archive_commit':commit,'previous_archive':parent,'source_base':head,'prefix':prefix,'changed_files':len(names),'artifacts':len(artifacts),'remote_manifest_and_patch_readback_equal':True,'state':'WIP, not functional acceptance'}
(dest/'remote-receipt.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt),flush=True)
