from pathlib import Path
import os,stat,json,hashlib,subprocess,shutil,urllib.request,io,tarfile,zstandard,time
W=Path('/workspace');R=W/'zil-push-main';A=W/'zil-demand-archive';OLD=R/'out/evidence/env-cache-retirement-01';S=R/'out/evidence/env-cache-retirement-03';S.mkdir();REF='refs/heads/archive/source-history';TOOL=Path('/tmp/full-push-v15-archive_evidence.py');BASE='research/incremental-demand-env-cache-delta-20261113-03'
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def save(p,v):
 with p.open('x') as f:json.dump(v,f,sort_keys=True,separators=(',',':'));f.write('\n');f.flush();os.fsync(f.fileno())
def git(*a):return subprocess.check_output(['git','-C',str(A),*a]).decode().strip()
def act(a,label,check=True,inp=None):
 p=subprocess.run(a,input=inp,capture_output=True,text=True)
 with (S/'operations.jsonl').open('a') as f:f.write(json.dumps({'label':label,'argv':a,'stdin':inp,'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr})+'\n');f.flush();os.fsync(f.fileno())
 if check:assert not p.returncode,(label,p.stderr)
 return p
def push(rel,msg):
 act(['git','-C',str(A),'add','--',rel],'add-'+rel);act(['git','-C',str(A),'commit','-m',msg],'commit-'+rel);act(['git','-C',str(A),'-c','pack.threads=1','-c','pack.window=0','push','origin','HEAD:'+REF],'ordinary-push-'+rel);fixed=git('rev-parse','HEAD');assert git('ls-remote','origin',REF).split()[0]==fixed;return fixed
def readback(fixed,dest,rel):
 records=[]
 for p in sorted(dest.iterdir()):
  if not p.is_file() or p.name.startswith('evidence.tar.zst.part-'):continue
  u='https://raw.githubusercontent.com/zil-lang/zil/'+fixed+'/'+rel+'/'+p.name;b=urllib.request.urlopen(u,timeout=60).read();assert b==p.read_bytes();records.append({'url':u,'bytes':len(b),'sha256':sha(p)})
 return records
inv=json.loads((OLD/'CURRENT-INVENTORY.json').read_text());matches=json.loads((OLD/'GLOBAL-ARCHIVE-HASH-MATCHES.json').read_text());backed={e['current']['path']:e['archived'] for e in matches};selected=[e for e in inv if e['version'] in [1,2,3,4,5]];regular=[e for e in selected if e['kind']=='regular'];assert not git('status','--porcelain')
PREV=R/'out/evidence/env-cache-retirement-02';prior=json.loads((PREV/'DEPENDENCIES.json').read_text());objects={}
for batch in ['a','b']:
 result=json.loads((PREV/('batch-'+batch+'-result.json')).read_text());name=result['archive_path'];mp=A/name/'evidence-manifest.json';m=json.loads(mp.read_text());proof=PREV/('delta-'+batch+'-fresh.json');j=json.loads(proof.read_text());assert j['status']=='verified' and j['verification']['files']==len(m['inventory']['files']);prior[name]={'fixed_commit':result['fixed_delta_commit'],'archive_path':name,'manifest_sha256':sha(mp),'prior_full_fresh_proof':{'original':str(proof),'sha256':sha(proof)}}
 for e in m['inventory']['files']:objects[(e['bytes'],e['mode'],e['sha256'])]={'archive_path':name,'manifest_sha256':sha(mp),'entry':e}
for e in regular:
 if e['path'] not in backed and (e['bytes'],e['mode'],e['sha256']) in objects:
  item={'current':e,'archived':objects[(e['bytes'],e['mode'],e['sha256'])]};matches.append(item);backed[e['path']]=item['archived']
dependencies={}
for name in sorted({backed[e['path']]['archive_path'] for e in regular if e['path'] in backed}):
 if name in prior:
  d=prior[name].copy();mp=A/name/'evidence-manifest.json';assert sha(mp)==d['manifest_sha256'];d['manifest']=json.loads(mp.read_text());proof=Path(d['prior_full_fresh_proof']['original']);assert sha(proof)==d['prior_full_fresh_proof']['sha256'];dependencies[name]=d;continue
 assert name.endswith('-v5');mp=A/name/'evidence-manifest.json';commit=git('log','-1','--format=%H','--',name+'/evidence-manifest.json');act(['git','-C',str(A),'merge-base','--is-ancestor',commit,'HEAD'],'prior-v5-actual-archive-ancestor');body=urllib.request.urlopen('https://raw.githubusercontent.com/zil-lang/zil/'+commit+'/'+name+'/evidence-manifest.json',timeout=60).read();assert hashlib.sha256(body).hexdigest()==sha(mp);dependencies[name]={'fixed_commit':commit,'archive_path':name,'manifest_sha256':sha(mp),'manifest':json.loads(body)}
for name,d in dependencies.items():
 if not name.endswith('-v5'):continue
 version='5'
 wanted={e['archived']['entry']['path']:e['archived']['entry'] for e in matches if e['current']['version'] in [1,2,3,4,5] and e['archived']['archive_path']==name};parts=d['manifest']['archive']['parts']
 class Parts(io.RawIOBase):
  def __init__(self):self.i=0;self.r=None;self.h=None;self.n=0;self.total=0;self.all=hashlib.sha256();self.checked=[]
  def readable(self):return True
  def read(self,size=-1):
   size=1024*1024 if size<0 else size
   if size==0:return b''
   while True:
    if self.r is None:
     if self.i>=len(parts):return b''
     part=parts[self.i];self.r=urllib.request.urlopen('https://raw.githubusercontent.com/zil-lang/zil/'+d['fixed_commit']+'/'+name+'/'+part['name'],timeout=60);self.h=hashlib.sha256();self.n=0
    b=self.r.read(size)
    if b:self.h.update(b);self.all.update(b);self.n+=len(b);self.total+=len(b);return b
    self.r.close();self.r=None;part=parts[self.i];assert self.n==part['bytes'] and self.h.hexdigest()==part['sha256'];self.checked.append(part);self.i+=1
  def readinto(self,b):q=self.read(len(b));b[:len(q)]=q;return len(q)
 raw=Parts();verified=[];minimum=shutil.disk_usage(W).free
 with zstandard.ZstdDecompressor().stream_reader(raw) as dec:
  with tarfile.open(fileobj=dec,mode='r|') as tf:
   for member in tf:
    path=member.name.removeprefix('payload/')
    if path not in wanted:continue
    e=wanted[path];assert member.isreg() and member.size==e['bytes'] and member.mode==e['mode'];tmp=S/('stream-restore-v'+version);assert not tmp.exists();h=hashlib.sha256();n=0
    with tf.extractfile(member) as src,tmp.open('xb') as out:
     while b:=src.read(1024*1024):out.write(b);h.update(b);n+=len(b)
    tmp.chmod(e['mode']);assert n==e['bytes'] and h.hexdigest()==e['sha256'] and tmp.stat().st_size==n and stat.S_IMODE(tmp.stat().st_mode)==e['mode'];minimum=min(minimum,shutil.disk_usage(W).free);assert minimum>500*1024*1024;tmp.unlink();verified.append(e)
  while dec.read(1024*1024):pass
 assert raw.i==len(parts) and raw.total==d['manifest']['archive']['bytes'] and raw.all.hexdigest()==d['manifest']['archive']['sha256'];assert {e['path'] for e in verified}==set(wanted)
 record={'status':'verified','fixed_archive_commit':d['fixed_commit'],'archive_path':name,'all_compressed_parts_and_joined_archive_SHA256_verified':True,'selected_full_regular_files_actually_stream_reconstructed_hash_size_mode_verified':verified,'selected_regular_bytes':sum(e['bytes'] for e in verified),'min_observed_free_bytes':minimum,'no_unrelated_original_raw_deleted':True};proof=S/('v'+version+'-targeted-fresh.json');save(proof,record);d['prior_full_fresh_proof']={'original':str(proof),'sha256':sha(proof),'boundary':'targeted complete selected files, entire compressed archive parts/join SHA verified; not claim of unrelated historical entire tar restoration'};print(json.dumps({'stage':'old-dependency-fresh-closed','version':version,'files':len(verified)}),flush=True)
save(S/'DEPENDENCIES.json',{k:{a:b for a,b in d.items() if a!='manifest'} for k,d in dependencies.items()})
closed_objects={};batch_results=[];all_removed=[]
template=subprocess.check_output(['git','-C',str(A),'show','b8b13bfd95ed03c196cb910b3625a13fddfa48a7:research/incremental-demand-full-push-20261112-v11/restore_https.py']).decode()
for batch,versions in [('a',[1,2]),('b',[3,4,5])]:
 es=[e for e in selected if e['version'] in versions];regs=[e for e in es if e['kind']=='regular'];new={}
 for e in regs:
  p=Path(e['path']);s=p.lstat();assert (s.st_dev,s.st_ino,s.st_size,stat.S_IMODE(s.st_mode),s.st_mtime_ns)==(e['dev'],e['inode'],e['bytes'],e['mode'],e['mtime_ns']);assert sha(p)==e['sha256']
  key=str(e['mode'])+'-'+e['sha256']
  if e['path'] not in backed and key not in closed_objects:new.setdefault(key,e)
 rel=BASE+'-'+batch;dest=A/rel;stage=S/('stage-'+batch);stage.mkdir();prefix='zil-push-main/out/evidence/env-cache-retirement-03/objects';delta_paths={}
 for key,e in new.items():
  p=Path(e['path']);q=stage/prefix/key;q.parent.mkdir(parents=True,exist_ok=True);os.link(p,q,follow_symlinks=False);delta_paths[key]=prefix+'/'+key
 rawbytes=sum(e['bytes'] for e in new.values());assert shutil.disk_usage(W).free>rawbytes+550*1024*1024;scope='Only previously unarchived exact SDK installation delta bytes, deduplicated by size/mode/SHA; all existing fixed DSO/source/receipt bytes referenced without repacking. Cache removal only after complete old+new exact recovery closure. No current SDK/raw/source/compiler deletion.'
 act(['python',str(TOOL),'pack','--source-root',str(stage),'--archive-dir',str(dest),'--include','zil-push-main/out/evidence/env-cache-retirement-02/**','--main-commit','9170bb4210554392e36eb7528596612596ebef80','--source-commit','recovery_coordinator=124567d4d00722113818e46f1e4c662de6eb1d87','--scope',scope,'--level','3'],'pack-only-missing-unique-delta-'+batch);manifest=json.loads((dest/'evidence-manifest.json').read_text());assert {e['path'] for e in manifest['inventory']['files']}==set(delta_paths.values());shutil.rmtree(stage);shutil.copy2(TOOL,dest/'archive_evidence.py');(dest/'restore_https.py').write_text(template.replace('research/incremental-demand-full-push-20261112-v11',rel).replace('v11','sdk-delta-'+batch));save(dest/'RESTORE.json',{'schema':1,'manifest':{'bytes':(dest/'evidence-manifest.json').stat().st_size,'sha256':sha(dest/'evidence-manifest.json')},'verifier':{'bytes':TOOL.stat().st_size,'sha256':sha(TOOL)},'archive_sha256':manifest['archive']['sha256'],'scope':scope})
 mappings=[]
 for e in regs:
  key=str(e['mode'])+'-'+e['sha256']
  if e['path'] in backed:
   old=backed[e['path']];dep=dependencies[old['archive_path']];origin={'fixed_commit':dep['fixed_commit'],'archive_path':dep['archive_path'],'regular_path':old['entry']['path'],'dependency_fresh_proof':dep['prior_full_fresh_proof']}
  elif key in closed_objects:origin=closed_objects[key]
  else:origin={'fixed_commit':'THIS_DELTA_FIXED_COMMIT','archive_path':rel,'regular_path':delta_paths[key]}
  mappings.append({**e,'recover_from':origin})
 save(dest/'PREFIXES.json',{'status':'complete full regular-byte/mode/source/receipt/env closure by prior fixed files plus missing delta','versions':versions,'regular_files':mappings,'directories':[e for e in es if e['kind']=='directory'],'safe_relative_aliases':[e for e in es if e['kind']=='symlink'],'dependencies':{k:{a:b for a,b in d.items() if a!='manifest'} for k,d in dependencies.items()},'new_unique_delta_files':len(new),'new_unique_delta_bytes':rawbytes,'no_existing_DSO_repacked':True,'no_rebuild_needed_to_restore_exact_prefix':True});fixed=push(rel,'Preserve only missing deduplicated old SDK cache bytes '+batch);meta_fresh=readback(fixed,dest,rel);download=Path('/tmp')/('sdk-delta-'+batch+'-download');restored=Path('/tmp')/('sdk-delta-'+batch+'-restored');bootstrap=S/('bootstrap-'+batch+'.py');bootstrap.write_bytes(urllib.request.urlopen('https://raw.githubusercontent.com/zil-lang/zil/'+fixed+'/'+rel+'/restore_https.py',timeout=60).read());assert sha(bootstrap)==sha(dest/'restore_https.py');assert shutil.disk_usage(W).free>rawbytes+manifest['archive']['bytes']+500*1024*1024
 receipt=S/('delta-'+batch+'-fresh.json')
 with (S/('delta-'+batch+'-fresh.log')).open('xb') as log:p=subprocess.run(['python',str(bootstrap),fixed,'--download-dir',str(download),'--restore-dir',str(restored),'--receipt',str(receipt)],stdout=log,stderr=subprocess.STDOUT)
 assert not p.returncode;fresh=json.loads(receipt.read_text());assert fresh['status']=='verified' and fresh['verification']['files']==len(new);assert shutil.disk_usage(W).free>500*1024*1024
 for key,e in new.items():
  q=restored/delta_paths[key];assert q.stat().st_size==e['bytes'] and stat.S_IMODE(q.stat().st_mode)==e['mode'] and sha(q)==e['sha256'];closed_objects[key]={'fixed_commit':fixed,'archive_path':rel,'regular_path':delta_paths[key],'new_delta_fresh_receipt_sha256':sha(receipt)}
 shutil.rmtree(download);shutil.rmtree(restored)
 # Only this task's freshly proven archive worktree copies are dematerialized.
 index=Path(git('rev-parse','--path-format=absolute','--git-path','index'));backup=S/('archive-index-before-skip-'+batch);shutil.copy2(index,backup)
 for part in manifest['archive']['parts']:
  q=dest/part['name'];assert sha(q)==part['sha256'];act(['git','-C',str(A),'update-index','--skip-worktree','--',rel+'/'+part['name']],'skip-own-fresh-archive-copy-'+batch);q.unlink()
 # No active process may use an old cache. The current/root SDK18 is excluded.
 roots=[str(W/'.zil-environment'/('llvm-function-products-v'+str(v))) for v in versions];uses=[]
 for proc in Path('/proc').iterdir():
  if not proc.name.isdigit() or int(proc.name)==os.getpid():continue
  try:
   targets=[]
   for field in ['maps','environ']:
    try:targets.append((proc/field).read_bytes().decode(errors='replace'))
    except OSError:pass
   try:targets.append(os.readlink(proc/'cwd'))
   except OSError:pass
   for fd in (proc/'fd').iterdir():
    try:targets.append(os.readlink(fd))
    except OSError:pass
   for root in roots:
    if any(root+'/' in t or root+'\0' in t or t==root for t in targets):uses.append({'pid':proc.name,'prefix':root})
  except OSError:pass
 assert not uses,uses
 for v in versions:
  vr=[e for e in regs if e['version']==v];pr=W/'.zil-environment'/('llvm-function-products-v'+str(v));j=json.loads((pr/'build-receipt.json').read_text());assert j['status']=='Pass';lib=j['library'];assert next(e for e in vr if e['path']==lib['path'])['sha256']==lib['sha256'];env=(pr/'env.sh').read_text();assert str(pr/'lib') in env
 for e in regs:
  p=Path(e['path']);s=p.lstat();assert (s.st_dev,s.st_ino,s.st_size,stat.S_IMODE(s.st_mode),s.st_mtime_ns)==(e['dev'],e['inode'],e['bytes'],e['mode'],e['mtime_ns']);p.unlink()
  with (S/'unlink-journal.jsonl').open('a') as f:f.write(json.dumps({'path':e['path'],'bytes':e['bytes'],'mode':e['mode'],'sha256':e['sha256'],'after_full_delta_and_prior_fixed_fresh_custody':fixed})+'\n');f.flush();os.fsync(f.fileno())
 for e in [e for e in es if e['kind']=='symlink']:assert e['target']=='libLLVM.so.23.1' and os.readlink(e['path'])==e['target'];Path(e['path']).unlink()
 for e in sorted((e for e in es if e['kind']=='directory'),key=lambda e:-len(Path(e['path']).parts)):
  p=Path(e['path']);assert p.is_dir() and not p.is_symlink() and stat.S_IMODE(p.stat().st_mode)==e['mode'];p.rmdir()
 result={'status':'complete','batch':batch,'versions_removed':versions,'new_unique_delta_files':len(new),'new_unique_delta_bytes':rawbytes,'fixed_delta_commit':fixed,'archive_path':rel,'delta_fresh_receipt_sha256':sha(receipt),'metadata_actual_fresh':meta_fresh,'prefix_regular_files_removed':len(regs),'prefix_logical_bytes_removed':sum(e['bytes'] for e in regs),'active_users':uses,'all_original_out_runs_current_SDK18_stock_native_link_target_source_immutable_compilers_untouched':True,'free_bytes':shutil.disk_usage(W).free};save(S/('batch-'+batch+'-result.json'),result);batch_results.append(result);all_removed+=regs;print(json.dumps({'stage':'prefix-batch-closed-and-cache-retired',**result}),flush=True)
rel=BASE+'-closure';dest=A/rel;dest.mkdir();save(dest/'BATCHES.json',{'status':'both exact old-prefix missing-delta and full prior-file recovery closures complete','batches':batch_results});shutil.copy2(__file__,dest/Path(__file__).name)
for p in S.iterdir():
 if p.is_file():shutil.copy2(p,dest/p.name)
(dest/'README.md').write_text('# Original SDK1..5 exact cache recovery\n\nThe two batch PREFIXES.json mappings preserve every exact regular byte/size/mode and directory/lexical alias. Old fixed file objects and the newly fixed prior SDKdelta02 objects are reused, with SHA/mode equality and fresh receipts; only still-missing unique bytes are packed here. THIS_DELTA_FIXED_COMMIT resolves to that batch fixed commit in BATCHES.json. No rebuild is needed to restore an exact original prefix. Both new deltas were actually completely restored/hash/mode checked before cache retirement, after checking build receipt/env binding and no active prefix user. SDK18, stock23, native-link, v10 first-failure receipt, rustup/cargo, current target, source, raw, and immutable compilers were untouched.\n')
fixed=push(rel,'Close exact SDK1..5 missing-delta and full-prefix cache recovery');meta=readback(fixed,dest,rel);result={'status':'complete within exact five-prefix scope; 0.7GB incremental target not mathematically available from these550MB prefixes','batches':batch_results,'old_SDK_prefixes_removed':5,'SDK_regular_files_removed':len(all_removed),'SDK_regular_logical_bytes_removed':sum(e['bytes'] for e in all_removed),'new_unique_delta_objects':len(closed_objects),'new_unique_delta_bytes':sum(e['new_unique_delta_bytes'] for e in batch_results),'fixed_closure_commit':fixed,'closure_path':rel,'v10_first_failure_receipt_preserved':True,'all_raw_current_SDK18_stock_native_link_rustup_cargo_target_source_compilers_preserved':True,'build_test_native_measurement_executions':0,'free_bytes':shutil.disk_usage(W).free};save(S/'result.json',result);out=BASE+'-result';d=A/out;d.mkdir();shutil.copy2(S/'result.json',d/'result.json');shutil.copy2(S/'operations.jsonl',d/'operations.jsonl');save(d/'CLOSURE-FRESH.json',{'fixed_commit':fixed,'path':rel,'actual_fresh_HTTPS':meta});final=push(out,'Record actual five obsolete SDK cache retirements without touching raw or current target');fresh=readback(final,d,out);save(S/'PUBLISHED.json',{'status':'complete','fixed_result_commit':final,'result_path':out,'fixed_closure_commit':fixed,'closure_path':rel,'old_SDK_prefixes_removed':5,'SDK_regular_logical_bytes_removed':sum(e['bytes'] for e in all_removed),'new_unique_delta_objects':len(closed_objects),'new_unique_delta_bytes':sum(e['new_unique_delta_bytes'] for e in batch_results),'actual_fresh_result_files':fresh,'free_bytes':shutil.disk_usage(W).free,'QUIET':True});Path('/tmp/full-push-env-cache-final-retirement.json').write_text(json.dumps(result,sort_keys=True,indent=2)+'\n');print((S/'PUBLISHED.json').read_text(),flush=True)
