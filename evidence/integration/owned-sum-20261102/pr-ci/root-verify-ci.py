from pathlib import Path
import gzip,hashlib,importlib.util,json,re,shutil,subprocess,sys,tarfile
run=Path(sys.argv[1]);out=Path(sys.argv[2]);head=sys.argv[3];candidate='15498f14af8f11d3a3bf4b198fe69208be5913ce';repo=Path('/workspace/zil-core');main=len(sys.argv)>4 and sys.argv[4]=='main'
def git(*args):return subprocess.check_output(['git',*args],cwd=repo)
def digest(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
meta=json.loads((run/'run.json').read_text());agent=json.loads((run/'RESULT.json').read_text());recovery=json.loads((run/'recovery.json').read_text())
assert meta['status']=='completed' and meta['conclusion']=='success' and meta['headSha']==head
assert agent['expected_pr_head']==head and agent['status']=='CompleteEvidenceRecovered'
assert all(j['conclusion']=='success' for j in meta['jobs'])
assert not [(j['name'],s['name']) for j in meta['jobs'] for s in j['steps'] if s['conclusion'] in ('failure','cancelled')]
tool=repo/'tools/ci/evidence_transport.py';assert tool.read_bytes()==git('show',f'{candidate}:tools/ci/evidence_transport.py')
spec=importlib.util.spec_from_file_location('transport',tool);t=importlib.util.module_from_spec(spec);spec.loader.exec_module(t)
archive=run/'evidence-recovery.tar.zst';assert digest(archive)==agent['archive']['sha256']
index=t.verify(archive);assert index['run_id']==str(meta['databaseId']) and index['producer_sha256']==digest(tool) and index['missing_roots']==[]
receipts=run/'selected/receipts';inputs=json.loads((receipts/'source-inputs.json').read_text());checkout=(receipts/'commit.txt').read_text().strip()
assert inputs['status']=='Pass' and inputs['commit']==index['checkout']==checkout==agent['actual_checkout']
assert inputs['checkout_before']==inputs['checkout_after']=='ActualBytesTypesAndGitModesMatchHEAD'
if main:assert checkout==head
listing=git('ls-tree','-r','-l',candidate);assert listing==(receipts/'source-tree.txt').read_bytes()
assert git('rev-parse',head+'^{tree}')==git('rev-parse',candidate+'^{tree}')
expected={}
for row in git('ls-tree','-r','-l','-z',candidate).split(b'\0'):
 if not row:continue
 hdr,path=row.split(b'\t',1);name=path.decode();mode,kind,oid,size=hdr.split()
 if name.startswith('evidence/'):continue
 assert kind==b'blob';expected[name]=(mode.decode(),oid.decode(),int(size))
assert set(expected)=={f['path'] for f in inputs['files']} and len(expected)==inputs['input_files']==4092
cat=subprocess.Popen(['git','cat-file','--batch'],cwd=repo,stdin=subprocess.PIPE,stdout=subprocess.PIPE)
for f in inputs['files']:
 mode,oid,size=expected[f['path']];assert (mode,oid,size)==(f['git_mode'],f['git_oid'],f['bytes'])
 cat.stdin.write((oid+'\n').encode());cat.stdin.flush();assert cat.stdout.readline().split()==[oid.encode(),b'blob',str(size).encode()]
 data=cat.stdout.read(size);assert cat.stdout.read(1)==b'\n';assert hashlib.sha256(data).hexdigest()==f['sha256']
cat.stdin.close();assert cat.wait()==0
assert digest(receipts/'source-inputs.tar.gz')==inputs['archive_sha256']
counts={}
for name,n in [('workspace',3575),('reference',3474),('text-reference',64)]:
 rows=re.findall(r'test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out',(receipts/(name+'.log')).read_text())
 totals=[sum(int(v[i]) for v in rows) for i in range(5)];assert totals==[n,0,0,0,0]
 counts[name]=dict(zip(['passed','failed','ignored','measured','filtered'],totals))
consumer=json.loads((receipts/'consumer209.json').read_text());assert consumer['status']=='Pass' and consumer['source_commit']==checkout and len(consumer['builds'])==16 and len(consumer['observations'])==128
artifacts=json.loads((run/'artifacts.json').read_text())
result={'status':'Pass','scope':'Root verified completed '+('main' if main else 'PR')+' CI source, all archived blobs and exact complete test counts','run_id':meta['databaseId'],'url':meta['url'],'workflow':meta['name'],'head':head,'implementation_commit':candidate,'actual_checkout':checkout,'tree':git('rev-parse',candidate+'^{tree}').decode().strip(),'tree_identical_to_candidate':True,'source_files_rehashed_against_Git':len(expected),'tests':counts,'consumer209_status':'Pass','archive':{'path':str(archive),'bytes':archive.stat().st_size,'sha256':digest(archive),'files':len(index['files']),'all_indexed_blobs_reverified_by_root':True},'artifact_api_total_count':artifacts['total_count'],'upload_outcomes':index.get('uploads'),'recovery_note':'Artifact uploads were unavailable; full log transport was downloaded and every indexed blob verified. Action success alone was not acceptance.','replays_not_added':True}
out.mkdir(parents=True,exist_ok=False)
(out/'RESULT.json').write_text(json.dumps(result,indent=2)+'\n')
for name in ['run.json','recovery.json','artifacts.json']:shutil.copy2(run/name,out/name)
(out/'index-identity.json').write_text(json.dumps({'sha256':digest(run/'INDEX.json'),'files':len(index['files']),'producer_sha256':index['producer_sha256'],'checkout':checkout,'run_id':index['run_id'],'missing_roots':index['missing_roots']},indent=2)+'\n')
for name in ['consumer209.json','llvm-api-toolchain.json','llvm-install.json','toolchain.txt','commit.txt']:
 shutil.copy2(receipts/name,out/name)
if (receipts/'llvm-research-identity.json').exists():shutil.copy2(receipts/'llvm-research-identity.json',out/'llvm-research-identity.json')
for name in ['workspace.log','reference.log','text-reference.log','clippy.log','fmt.log','migration-runner.log','source-inputs.json']:
 with gzip.GzipFile(filename=str(out/(name+'.gz')),mode='wb',mtime=0) as f:f.write((receipts/name).read_bytes())
print(json.dumps({'run':meta['databaseId'],'status':'Pass','source_files':len(expected),'archive_files':len(index['files']),'tests':counts},indent=2))
