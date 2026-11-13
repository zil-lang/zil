from pathlib import Path
import os,stat,json,hashlib,subprocess,shutil,urllib.request,time
R=Path('/workspace/zil-push-main');A=Path('/workspace/zil-demand-archive');S=R/'out/evidence/legacy-target-cache-retirement-01';S.mkdir();REF='refs/heads/archive/source-history';REL='research/incremental-demand-legacy-target-cache-retirement-20261113-01';D=A/REL
TREES=[(Path('/workspace/zil-demand'),'b7703b95a59cc87dd1ac13ce6f652386ea4dfd0d'),(Path('/workspace/zil-baseline-3ec7e8a9'),'3ec7e8a960ca69368cd9a90f1a15ac063fec5a02')]
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def fullstat(p):
 s=p.lstat();return {k:getattr(s,k) for k in ['st_mode','st_ino','st_dev','st_nlink','st_uid','st_gid','st_size','st_atime_ns','st_mtime_ns','st_ctime_ns','st_blocks','st_blksize']}
def save(p,v):
 with p.open('x') as f:json.dump(v,f,sort_keys=True,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
def git(*a):return subprocess.check_output(['git','-C',str(A),*a]).decode().strip()
def act(a,label):
 p=subprocess.run(a,capture_output=True,text=True)
 with (S/'operations.jsonl').open('a') as f:f.write(json.dumps({'label':label,'argv':a,'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr})+'\n');f.flush();os.fsync(f.fileno())
 assert not p.returncode,(label,p.stderr)
records=[];allowed=[];protected=[]
for tree,tip in TREES:
 assert tree.is_dir() and not tree.is_symlink();assert subprocess.check_output(['git','-C',str(tree),'rev-parse','HEAD']).decode().strip()==tip;assert subprocess.run(['git','-C',str(tree),'symbolic-ref','-q','HEAD'],capture_output=True).returncode==1;assert not subprocess.check_output(['git','-C',str(tree),'status','--porcelain=v1','--untracked-files=all']).strip()
 root=tree/'target';keep=root/'release/zil';assert root.is_dir() and not root.is_symlink() and keep.is_file() and not keep.is_symlink();parents=[p for p in keep.parents if p==root or p.is_relative_to(root)];before=fullstat(keep);digest=sha(keep);after=fullstat(keep);assert all(before[k]==after[k] for k in ['st_dev','st_ino','st_mode','st_size','st_uid','st_gid','st_mtime_ns']);files=[];dirs=[];special=[]
 for a,ds,ns in os.walk(root,followlinks=False):
  p=Path(a);dirs.append({'path':str(p),'fullstat':fullstat(p),'keep_parent':p in parents})
  for name in ds+ns:
   p=Path(a)/name;s=p.lstat()
   if stat.S_ISREG(s.st_mode):
    if p!=keep:files.append({'path':str(p),'fullstat':fullstat(p),'kind':'ordinary regenerable target compilation cache'})
   elif stat.S_ISLNK(s.st_mode) or not stat.S_ISDIR(s.st_mode):special.append({'path':str(p),'fullstat':fullstat(p),'target':os.readlink(p) if p.is_symlink() else None})
 rec={'source_tree':str(tree),'detached_HEAD':tip,'target_root':str(root),'target_root_fullstat':fullstat(root),'preserved_binary':{'path':str(keep),'sha256':digest,'fullstat_before_hash':before,'fullstat_after_hash':after,'source_binding':'retained as found; no new source-to-binary binding asserted by cache cleanup'},'preserved_parent_directories':[{'path':str(p),'fullstat':fullstat(p)} for p in parents],'delete_only_ordinary_files':files,'directories':dirs,'special_entries_retained':special};records.append(rec);allowed.append(root);protected.append(keep)
assert records[0]['preserved_binary']['sha256']=='936d7c107713c11dca86bbf196fc9a0c6ceee336ef4f57e60751543ba1817d82';assert records[1]['preserved_binary']['sha256']=='1b9abd96de7989b4b9e01bc37ccbee54e5984617028988dffd5d00d516c73405'
uses=[]
for proc in Path('/proc').iterdir():
 if not proc.name.isdigit() or int(proc.name)==os.getpid():continue
 try:
  values=[]
  try:values.append(os.readlink(proc/'cwd'))
  except OSError:pass
  try:values.append((proc/'maps').read_text(errors='replace'))
  except OSError:pass
  for fd in (proc/'fd').iterdir():
   try:values.append(os.readlink(fd))
   except OSError:pass
  for tree,tip in TREES:
   if any(v==str(tree) or str(tree)+'/' in v for v in values):uses.append({'pid':proc.name,'tree':str(tree)})
 except OSError:pass
assert not uses,uses
save(S/'PRECONDITIONS.json',{'status':'proved before deletion','explicit_scope':'Only ordinary generated cache files beneath exactly two detached old target roots; keep exact target/release/zil and every ancestor directory. No raw/out/source/.git/current root target/other-owner path deletion.','trees':records,'observed_active_tree_uses':uses,'no_new_compile_test_native_measurement_or_archive_pack':True})
free_before=shutil.disk_usage('/workspace').free;unlinked=[];removed_dirs=[]
for rec in records:
 root=Path(rec['target_root']);keep=Path(rec['preserved_binary']['path'])
 for item in rec['delete_only_ordinary_files']:
  p=Path(item['path']);assert p.is_relative_to(root) and p!=keep and not p.is_symlink();current=fullstat(p);expected=item['fullstat'];assert all(current[k]==expected[k] for k in ['st_dev','st_ino','st_mode','st_size','st_uid','st_gid','st_mtime_ns']);p.unlink();unlinked.append(item)
  with (S/'unlink-journal.jsonl').open('a') as f:f.write(json.dumps(item,separators=(',',':'))+'\n');f.flush();os.fsync(f.fileno())
 for item in sorted(rec['directories'],key=lambda x:-len(Path(x['path']).parts)):
  p=Path(item['path'])
  if item['keep_parent'] or not p.exists():continue
  assert p.is_relative_to(root) and not p.is_symlink() and stat.S_ISDIR(p.lstat().st_mode)
  if not any(p.iterdir()):p.rmdir();removed_dirs.append(item['path'])
 after=fullstat(keep);h=sha(keep);assert h==rec['preserved_binary']['sha256'];before=rec['preserved_binary']['fullstat_after_hash'];assert all(after[k]==before[k] for k in ['st_dev','st_ino','st_mode','st_size','st_uid','st_gid','st_mtime_ns'])
 rec['binary_after']={'sha256':h,'fullstat_before_hash':after,'fullstat_after_hash':fullstat(keep),'identity_bytes_mode_uid_gid_mtime_preserved':True,'link_count_and_ctime_can_change_because_other_generated_hardlink_alias_removed':True}
 for item in rec['preserved_parent_directories']:
  p=Path(item['path']);after_dir=fullstat(p);assert p.is_dir() and all(after_dir[k]==item['fullstat'][k] for k in ['st_dev','st_ino','st_mode','st_uid','st_gid']);item['after']=after_dir
 tree=Path(rec['source_tree']);assert subprocess.check_output(['git','-C',str(tree),'rev-parse','HEAD']).decode().strip()==rec['detached_HEAD'];assert not subprocess.check_output(['git','-C',str(tree),'status','--porcelain=v1','--untracked-files=all']).strip()
inode_deletes={}
for x in unlinked:
 st=x['fullstat'];k=(st['st_dev'],st['st_ino']);entry=inode_deletes.setdefault(k,{'links_removed':0,'original_nlink':st['st_nlink'],'allocated_bytes':st['st_blocks']*512});entry['links_removed']+=1
fully_unlinked_bytes=sum(e['allocated_bytes'] for e in inode_deletes.values() if e['links_removed']==e['original_nlink']);result={'status':'complete, scope not expanded','cache_roots':[str(p) for p in allowed],'regular_generated_cache_files_removed':len(unlinked),'logical_file_bytes_unlinked':sum(e['fullstat']['st_size'] for e in unlinked),'estimated_file_allocated_bytes_freed_where_all_links_removed':fully_unlinked_bytes,'empty_generated_directories_removed':len(removed_dirs),'binary_and_all_parent_directories_preserved':True,'trees':records,'special_entries_removed':0,'raw_out_source_git_current_target_other_worker_files_deleted':0,'build_test_native_measurement_executions':0,'free_bytes_before':free_before,'free_bytes_after':shutil.disk_usage('/workspace').free,'scope_cannot_attribute_other_parallel_build_disk_changes_to_this_cleanup':True};save(S/'result.json',result)
assert not git('status','--porcelain');assert git('ls-remote','origin',REF).split()[0]==git('rev-parse','HEAD');D.mkdir()
for name in ['PRECONDITIONS.json','result.json','unlink-journal.jsonl']:shutil.copy2(S/name,D/name)
shutil.copy2(__file__,D/Path(__file__).name);(D/'README.md').write_text('# Two detached old target cache retirements\n\nOnly ordinary generated files under zil-demand/target and zil-baseline-3ec7e8a9/target were removed. Each exact target/release/zil and all its ancestor directories remain at the original path/inode/mode, with before/after identical SHA256 and byte size. Their other generated hardlink aliases may be removed, reducing nlink and changing ctime; original binary content/inode are preserved. Cache inventory and unlink journal do not claim these reproducible outputs are raw evidence or require archiving. Source HEAD/clean detached state, no active tree use, strict lexical scope and full stat are recorded. No out/raw/source/.git/current root target/current SDK/other worktree is touched.\n')
act(['git','-C',str(A),'add','--',REL],'add-two-cache-fixed-result');act(['git','-C',str(A),'commit','-m','Retire two obsolete target caches while preserving original compiler inodes and bytes'],'commit-two-cache-result');act(['git','-C',str(A),'-c','pack.threads=1','-c','pack.window=0','push','origin','HEAD:'+REF],'ordinary-small-result-push');fixed=git('rev-parse','HEAD');assert git('ls-remote','origin',REF).split()[0]==fixed;fresh=[]
for p in sorted(D.iterdir()):
 u='https://raw.githubusercontent.com/zil-lang/zil/'+fixed+'/'+REL+'/'+p.name;b=urllib.request.urlopen(u,timeout=60).read();assert b==p.read_bytes();fresh.append({'url':u,'bytes':len(b),'sha256':sha(p)})
assert not git('status','--porcelain');save(S/'PUBLISHED.json',{'status':'complete','fixed_result_commit':fixed,'result_path':REL,'regular_generated_cache_files_removed':len(unlinked),'logical_file_bytes_unlinked':result['logical_file_bytes_unlinked'],'estimated_file_allocated_bytes_freed':fully_unlinked_bytes,'preserved_binaries':[{'path':r['preserved_binary']['path'],'sha256':r['binary_after']['sha256'],'inode':r['binary_after']['fullstat_after_hash']['st_ino'],'bytes':r['binary_after']['fullstat_after_hash']['st_size'],'nlink_after':r['binary_after']['fullstat_after_hash']['st_nlink']} for r in records],'fixed_result_actual_fresh_HTTPS':fresh,'free_bytes':shutil.disk_usage('/workspace').free,'QUIET':True});Path('/tmp/full-push-legacy-target-cache-retirement.json').write_text(json.dumps({k:v for k,v in json.loads((S/'PUBLISHED.json').read_text()).items() if k!='fixed_result_actual_fresh_HTTPS'},sort_keys=True,indent=2)+'\n');print((S/'PUBLISHED.json').read_text(),flush=True)
