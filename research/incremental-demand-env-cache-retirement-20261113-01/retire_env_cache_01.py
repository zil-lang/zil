from pathlib import Path
import subprocess,json,hashlib,shutil,os,stat,urllib.request,time
A=Path('/workspace/zil-demand-archive');R=Path('/workspace/zil-push-main');S=R/'out/evidence/env-cache-retirement-01';SDK=Path('/workspace/.zil-environment/llvm-function-products-v16');CACHE=Path('/workspace/zil-baseline-9170/target');BIN=Path('/workspace/zil-baseline-9170/out/binaries/9170bb4210554392e36eb7528596612596ebef80/zil');REF='refs/heads/archive/source-history';REL='research/incremental-demand-env-cache-retirement-20261113-01';D=A/REL;OUT=REL+'-result';E=A/OUT
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def save(p,v):
 with p.open('x') as f:json.dump(v,f,sort_keys=True,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
def git(*a):return subprocess.check_output(['git','-C',str(A),*a]).decode().strip()
def act(a,label):
 p=subprocess.run(a,capture_output=True,text=True)
 with (S/'operations.jsonl').open('a') as f:f.write(json.dumps({'label':label,'argv':a,'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr})+'\n');f.flush();os.fsync(f.fileno())
 assert not p.returncode,(label,p.stderr)
def publish(folder,rel,msg):
 act(['git','-C',str(A),'add','--',rel],'stage-'+rel);act(['git','-C',str(A),'commit','-m',msg],'commit-'+rel);act(['git','-C',str(A),'-c','pack.threads=1','-c','pack.window=0','push','origin','HEAD:'+REF],'ordinary-push-'+rel);tip=git('rev-parse','HEAD');assert git('ls-remote','origin',REF).split()[0]==tip;verified=[]
 for p in sorted(folder.iterdir()):
  if not p.is_file():continue
  url='https://raw.githubusercontent.com/zil-lang/zil/'+tip+'/'+rel+'/'+p.name;b=urllib.request.urlopen(url,timeout=60).read();assert len(b)==p.stat().st_size and hashlib.sha256(b).hexdigest()==sha(p);verified.append({'url':url,'bytes':len(b),'sha256':sha(p)})
 return tip,verified
assert not git('status','--porcelain');assert git('ls-remote','origin',REF).split()[0]==git('rev-parse','HEAD');inv=json.loads((S/'CURRENT-INVENTORY.json').read_text());es=[e for e in inv if e['version']==16];regular=[e for e in es if e['kind']=='regular'];assert len(regular)==156 and sum(e['bytes'] for e in regular)==124312325
coverage={};deps=[]
for n in ['06','07']:
 summary=R/f'out/runs/full-push-archive-v21-{n}-recovery-01.json';j=json.loads(summary.read_text());assert j['status']=='verified' and j['fresh_https_only'] and j['file_hash_stream_and_post_restore_verified'] and j['file_modes_verified'];m=A/j['archive_path']/'evidence-manifest.json';assert sha(m)==j['manifest_sha256'];remote=urllib.request.urlopen('https://raw.githubusercontent.com/zil-lang/zil/'+j['fixed_archive_commit']+'/'+j['archive_path']+'/evidence-manifest.json',timeout=60).read();assert hashlib.sha256(remote).hexdigest()==j['manifest_sha256'];mj=json.loads(remote)
 for e in mj['inventory']['files']:coverage['/workspace/'+e['path']]={'entry':e,'fixed_commit':j['fixed_archive_commit'],'archive_path':j['archive_path'],'bootstrap_sha256':j['bootstrap_sha256']}
 deps.append({'summary':str(summary),'summary_sha256':sha(summary),'fixed_commit':j['fixed_archive_commit'],'archive_path':j['archive_path'],'manifest_sha256':j['manifest_sha256'],'actual_full_fresh_receipt_sha256':j['full_receipt_sha256'],'actual_full_fresh_verified':True})
for e in regular:
 c=coverage[e['path']];assert all(e[k]==c['entry'][k] for k in ['bytes','mode','sha256']);s=Path(e['path']).lstat();assert (s.st_dev,s.st_ino,s.st_size,stat.S_IMODE(s.st_mode),s.st_mtime_ns)==(e['dev'],e['inode'],e['bytes'],e['mode'],e['mtime_ns'])
aliases=[e for e in es if e['kind']=='symlink'];assert len(aliases)==1 and aliases[0]['target']=='libLLVM.so.23.1';assert os.readlink(aliases[0]['path'])==aliases[0]['target']
receipt=json.loads((SDK/'build-receipt.json').read_text());dso=next(e for e in regular if e['path']==receipt['library']['path']);assert receipt['status']=='Pass' and receipt['library']['sha256']==dso['sha256'];env=(SDK/'env.sh').read_text();assert str(SDK/'lib') in env and '/workspace/.zil-environment/llvm-23.1.4/' in env
for u in receipt['compile_units']:
 source=SDK/'source'/u['source'];assert next(e for e in regular if e['path']==str(source))['sha256']==u['patched_source_sha256']
active=[]
for proc in Path('/proc').iterdir():
 if not proc.name.isdigit() or int(proc.name)==os.getpid():continue
 try:
  checks=[]
  for field in ['maps','environ']:
   try:checks.extend(proc.joinpath(field).read_bytes().decode(errors='replace').splitlines())
   except (OSError,PermissionError):pass
  try:checks.append(os.readlink(proc/'cwd'))
  except (OSError,PermissionError):pass
  for fd in (proc/'fd').iterdir():
   try:checks.append(os.readlink(fd))
   except (OSError,PermissionError):pass
  for text in checks:
   for protected in [str(SDK),str(CACHE)]:
    if protected in text:active.append({'pid':proc.name,'path':protected})
 except (OSError,PermissionError):pass
assert not active,active
assert CACHE.is_dir() and not CACHE.is_symlink() and CACHE.resolve()==CACHE;cache=[]
for a,ds,ns in os.walk(CACHE,followlinks=False):
 for name in ns:
  p=Path(a)/name;s=p.lstat();cache.append({'path':str(p),'bytes':s.st_size,'mode':stat.S_IMODE(s.st_mode),'kind':'symlink' if p.is_symlink() else 'regenerable-build-cache','target':os.readlink(p) if p.is_symlink() else None})
binary_before=BIN.stat();assert sha(BIN)=='8d8f7ddbd9bcf9567e042a7a8256a58698897589cafc0961655ba0a24113514e'
pre={'status':'all deletion preconditions proved','authority':'Root explicitly authorizes only fully backed SDK16 and own baseline9170 target fallback; preserve v1..v15, v17/v18, stock23, native-link, rustup/cargo, raw and all immutable compilers.','SDK16':{'regular_files':regular,'directories':[e for e in es if e['kind']=='directory'],'safe_relative_symlinks':aliases,'dependencies':deps,'current_hash_recheck_already_completed_in_this_run':True,'all_current_regular_bytes_mode_sha256_match_prior_actual_full_fresh':True,'build_receipt_library_and_patched_sources_bound':True,'env_bound_to_original_v16_and_stock23':True},'baseline_build_cache':{'path':str(CACHE),'files':cache,'logical_bytes':sum(e['bytes'] for e in cache),'authority':'root-created reproducible compilation cache, not raw evidence; no remote custody asserted for target cache','rebuild_binding_fixed_V27_recovery':'3e06887f44907a75df871025b78bd5e2756ce151'},'baseline_immutable_preserved':{'path':str(BIN),'sha256':'8d8f7ddbd9bcf9567e042a7a8256a58698897589cafc0961655ba0a24113514e','bytes':binary_before.st_size,'mode':stat.S_IMODE(binary_before.st_mode),'dev':binary_before.st_dev,'inode':binary_before.st_ino},'observed_active_uses':active,'blocked_v1_to15':'Missing complete exact archived static libraries/objects or original DSO; all original paths retained. Full per-version missing hashes in COVERAGE-SUMMARY.json. v10 failed201B receipt retained.','no_compile_test_native_measurement':True};save(S/'PRECONDITIONS.json',pre);D.mkdir();shutil.copy2(S/'PRECONDITIONS.json',D/'PRECONDITIONS.json');shutil.copy2(S/'COVERAGE-SUMMARY.json',D/'COVERAGE-SUMMARY.json');shutil.copy2(S/'coverage-scan-01-stopped.json',D/'coverage-scan-01-stopped.json');shutil.copy2(__file__,D/Path(__file__).name)
for dep in deps:shutil.copy2(dep['summary'],D/Path(dep['summary']).name)
(D/'RESTORE.md').write_text('# Exact retired SDK16 recovery\n\nUse the two immutable V21 commits/paths in PRECONDITIONS.json. Download each fixed restore_https.py, verify bootstrap hash from its fixed RESTORE.json, and run it with fresh --download-dir/--restore-dir/--receipt. The original full byte/mode fresh checks are retained here. Copy the156 listed regular files from the restored workspace-relative paths to a NEW SDK16 prefix, verifying every listed size/SHA256/mode. Recreate all26 recorded directory modes and the one lexical alias lib/libLLVM-23.so -> libLLVM.so.23.1 only inside that new prefix. No source compile is necessary to restore any SDK16 byte. The archived env.sh and build-receipt bind the original v16 library3930f308 and patched sources; do not select it for current SDK18 builds.\n\nBaseline9170 target is ordinary disposable build cache, not evidence. Its official immutable out/binaries/9170.../zil remains byte-identical SHA8d8f7d and complete raw/build binding/source remain fixed V27. Cache regeneration uses that exact source/build environment, and is not required for executing the immutable baseline. All v1..v15 incomplete prefixes remain in place.\n')
fixed,verified=publish(D,REL,'Preserve exact SDK16 cache recovery and baseline9170 cache deletion preconditions');save(S/'PRESERVATION-FRESH.json',{'status':'verified','fixed_commit':fixed,'path':REL,'actual_fresh_HTTPS':verified})
free_before=shutil.disk_usage('/workspace').free
for e in regular:
 p=Path(e['path']);s=p.lstat();assert (s.st_dev,s.st_ino,s.st_size,stat.S_IMODE(s.st_mode),s.st_mtime_ns)==(e['dev'],e['inode'],e['bytes'],e['mode'],e['mtime_ns']);p.unlink()
 with (S/'unlink-journal.jsonl').open('a') as f:f.write(json.dumps({'path':str(p),'bytes':e['bytes'],'mode':e['mode'],'sha256':e['sha256'],'after_fixed_fresh_preservation':fixed})+'\n');f.flush();os.fsync(f.fileno())
for e in aliases:assert os.readlink(e['path'])==e['target'];Path(e['path']).unlink()
for e in sorted((e for e in es if e['kind']=='directory'),key=lambda e:-len(Path(e['path']).parts)):
 p=Path(e['path']);assert p.is_dir() and not p.is_symlink() and stat.S_IMODE(p.stat().st_mode)==e['mode'];p.rmdir()
assert not SDK.exists();shutil.rmtree(CACHE);assert not CACHE.exists();after=BIN.stat();assert (after.st_dev,after.st_ino,after.st_size,after.st_mode,after.st_mtime_ns)==(binary_before.st_dev,binary_before.st_ino,binary_before.st_size,binary_before.st_mode,binary_before.st_mtime_ns)
result={'status':'complete within proven scope; 0.7GB target not reached and no scope expansion','fixed_preservation_commit':fixed,'preservation_path':REL,'SDK_prefixes_removed':[str(SDK)],'SDK_regular_files_removed':156,'SDK_regular_logical_bytes_removed':124312325,'baseline_cache_removed':str(CACHE),'baseline_cache_logical_bytes_removed':sum(e['bytes'] for e in cache),'all_other_old_SDK_prefixes_retained':True,'current_SDK18_stock23_native_link_and_profiles_untouched':True,'original_out_runs_raw_deleted':0,'immutable_compiler_deleted':0,'all_Git_objects_retained':True,'baseline_immutable_inode_size_mode_mtime_and_original_SHA_preserved':True,'baseline_immutable_sha256':'8d8f7ddbd9bcf9567e042a7a8256a58698897589cafc0961655ba0a24113514e','free_before':free_before,'free_after':shutil.disk_usage('/workspace').free,'build_test_native_measurement_executions':0};save(S/'result.json',result);E.mkdir()
for name in ['result.json','operations.jsonl','PRESERVATION-FRESH.json','unlink-journal.jsonl']:shutil.copy2(S/name,E/name)
final,fresh=publish(E,OUT,'Record SDK16 and own baseline build cache retirement; preserve incomplete prefixes');save(S/'PUBLISHED.json',{'status':'complete','fixed_preservation_commit':fixed,'preservation_path':REL,'fixed_result_commit':final,'result_path':OUT,'SDK_regular_bytes_removed':124312325,'baseline_cache_logical_bytes_removed':sum(e['bytes'] for e in cache),'fresh_result_files':fresh,'free_bytes':shutil.disk_usage('/workspace').free,'QUIET':True,'no_build_test_native_measurement':True});Path('/tmp/full-push-env-cache-retirement.json').write_text(json.dumps({**result,'fixed_result_commit':final,'fixed_result_path':OUT},sort_keys=True,indent=2)+'\n');print((S/'PUBLISHED.json').read_text(),flush=True)
