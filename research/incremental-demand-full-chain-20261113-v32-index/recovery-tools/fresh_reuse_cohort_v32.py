from pathlib import Path
import os,stat,json,hashlib,subprocess,shutil,urllib.request,time
from v32_cas_restore import verify_legacy,sha
W=Path('/workspace');R=W/'zil-push-main';S=R/'out/evidence/full-push-checkpoint-32';A=W/'zil-demand-archive';REF='refs/heads/archive/source-history';REL='research/incremental-demand-full-chain-20261113-v32-plan'
plan=json.loads((S/'CAS-PLAN-bytes.json').read_text());master=json.loads((S/'MASTER.json').read_text());entries={e['path']:e for e in master['files']};source=json.loads((S/'SOURCE.json').read_text())
def save(p,x):
 with p.open('x') as f:json.dump(x,f,sort_keys=True,separators=(',',':'));f.write('\n');f.flush();os.fsync(f.fileno())
def run(*args):
 p=subprocess.run(args,capture_output=True,text=True)
 with (S/'CAS-OPERATIONS.jsonl').open('a') as f:f.write(json.dumps({'argv':args,'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr})+'\n');f.flush();os.fsync(f.fileno())
 assert not p.returncode,p.stderr;return p.stdout.strip()
def git(*args):return run('git','-C',str(A),*args)
def full(p):
 st=p.lstat();return {n:getattr(st,n) for n in ['st_dev','st_ino','st_mode','st_nlink','st_uid','st_gid','st_size','st_atime_ns','st_mtime_ns','st_ctime_ns','st_blocks']}
def stable(s):return tuple(s[n] for n in ['st_dev','st_ino','st_mode','st_uid','st_gid','st_size','st_mtime_ns'])
def raw(p):return str(p).startswith('/workspace/zil-push-main/out/runs/') or str(p).startswith('/workspace/zil-demand/out/runs/')
def active(paths):
 out=[]
 for pd in Path('/proc').iterdir():
  if not pd.name.isdigit() or int(pd.name)==os.getpid():continue
  try:
   for f in (pd/'fd').iterdir():
    try:
     q=os.readlink(f)
     if q in paths:out.append({'pid':pd.name,'fd':str(f),'path':q})
    except OSError:pass
   text=(pd/'maps').read_text()
   for q in paths:
    if q in text:out.append({'pid':pd.name,'maps_path':q})
  except (OSError,PermissionError):pass
 return out
assert not git('status','--porcelain');remote=git('ls-remote','origin',REF).split()[0];assert remote==git('rev-parse','HEAD')=='76f24f54faf92749cf5afd5ebc13e00e58343c84'
dest=A/REL;dest.mkdir()
for name in ['MASTER.json','SOURCE.json','STRUCTURE.json','CAS-PLAN-bytes.json','COMPRESSION-ESTIMATE.json']:shutil.copy2(S/name,dest/name)
for name in ['v32_cas_restore.py','fresh_reuse_cohort_v32.py','freeze_cohort_v32.py','plan_cohort_v32_cas_bytes.py']:shutil.copy2(Path('/tmp')/name,dest/name)
save(dest/'STATUS.json',{'status':'V32 frozen explicit scope; publication and fresh whole closure PENDING','source_parents':source['git_sources'],'original_raw_paths_retained':True,'prior_source_only_a6_missing_historical_raw_boundary_explicit':True,'new_ClosedBody_WIP_excluded':True})
git('add','--',REL);git('commit','-m','Freeze V32 exact logical CAS scope and preserve original cohort and obsolete-route source parents')
git('merge','-s','ours','--no-ff',*sorted(set(source['git_sources'].values())),'-m','Preserve actual V32 original d662 98fa tool0db and obsolete a6 Git parents; custody only')
git('-c','pack.threads=1','-c','pack.window=0','push','origin','HEAD:'+REF);commit=git('rev-parse','HEAD');assert git('ls-remote','origin',REF).split()[0]==commit
fresh=[]
for p in sorted(dest.iterdir()):
 b=urllib.request.urlopen(f'https://raw.githubusercontent.com/zil-lang/zil/{commit}/{REL}/{p.name}',timeout=60).read();assert len(b)==p.stat().st_size and hashlib.sha256(b).hexdigest()==sha(p);fresh.append({'path':REL+'/'+p.name,'bytes':len(b),'sha256':sha(p)})
save(S/'PLAN-PUBLISHED.json',{'fixed_commit':commit,'path':REL,'fresh_files':fresh,'whole_closure_status':'pending'})
journal=S/'CAS-COALESCENCE.jsonl';holds=S/'coalescence-holds';holds.mkdir();changes=0;freed=0
def log(d):
 with journal.open('a') as f:f.write(json.dumps(d,separators=(',',':'))+'\n');f.flush();os.fsync(f.fileno())
def callback(obs,proof):
 global changes,freed
 # This callback is invoked only AFTER all parts and full compressed archive hash pass.
 for o in obs:
  canon=W/o['prior_raw_canonical']
  if not raw(canon) or not canon.is_file() or canon.is_symlink():continue
  cs=full(canon)
  if cs['st_size']!=o['bytes'] or sha(canon)!=o['sha256']:continue
  targets=[]
  for name in o['logical_paths']:
   p=W/name;e=entries[name]
   if not raw(p) or e['mode']!=stat.S_IMODE(cs['st_mode']):continue
   st=full(p);assert stable(st)==(e['dev'],e['inode'],stat.S_IFREG|e['mode'],st['st_uid'],st['st_gid'],e['bytes'],e['mtime_ns']),name
   assert sha(p)==e['sha256'] and stat.S_ISREG(st['st_mode']) and not p.is_symlink()
   assert (st['st_dev'],st['st_uid'],st['st_gid'])==(cs['st_dev'],cs['st_uid'],cs['st_gid'])
   if st['st_ino']!=cs['st_ino']:targets.append((p,st))
  if not targets:continue
  live=active({str(canon),*[str(p) for p,s in targets]});assert not live,live
  gid=o['sha256'];hdir=holds/gid;hdir.mkdir();log({'stage':'preconditions','sha256':gid,'canonical':str(canon),'canonical_before':cs,'targets':[{'path':str(p),'before':st} for p,st in targets],'fresh_dependency':proof})
  for i,(p,st) in enumerate(targets):os.link(p,hdir/str(i))
  replaced=[]
  try:
   for i,(p,st) in enumerate(targets):
    assert stable(full(p))==stable(st) and sha(p)==gid;assert stable(full(canon))==stable(cs) and sha(canon)==gid
    temp=p.parent/('.v32-cas-link-'+gid[:16]);assert not temp.exists();os.link(canon,temp);assert stable(full(p))==stable(st);os.replace(temp,p);replaced.append((i,p,st));after=full(p);assert after['st_ino']==cs['st_ino'] and after['st_mode']==st['st_mode'] and after['st_size']==st['st_size'] and sha(p)==gid
   # All old inodes remain held until every replacement and canonical is verified.
   assert sha(canon)==gid
   for i,p,st in replaced:
    after=full(p);log({'stage':'replacement-verified-before-hold-release','path':str(p),'sha256':gid,'before':st,'after':after,'canonical':str(canon)})
   seen={}
   for i,p,st in replaced:seen.setdefault((st['st_dev'],st['st_ino']),{'stat':st,'holds':[]})['holds'].append(hdir/str(i))
   for item in seen.values():
    h=item['holds'][0];s=h.stat()
    if s.st_nlink==len(item['holds']):freed+=s.st_blocks*512
   for i,p,st in replaced:(hdir/str(i)).unlink()
   hdir.rmdir();changes+=len(replaced)
   log({'stage':'group-complete','sha256':gid,'canonical_after':full(canon),'replacements':len(replaced),'all_logical_paths_preserved':True,'inode_identity_changed_as_recorded':True,'free_bytes':shutil.disk_usage(W).free})
  except BaseException:
   for i,p,st in reversed(replaced):
    h=hdir/str(i)
    if h.exists():os.replace(h,p)
   log({'stage':'group-aborted-rollback-old-inode-holds-retained','sha256':gid});raise
 result_progress={'changes':changes,'estimated_allocated_bytes_freed':freed,'free_bytes':shutil.disk_usage(W).free}
 (S/'CAS-COALESCENCE-PROGRESS.json').write_text(json.dumps(result_progress)+'\n')
result=verify_legacy(plan['objects'],entries,S/'legacy-HTTPS-fresh',callback=callback)
save(S/'LEGACY-FRESH.json',result);save(S/'CAS-COALESCENCE-RESULT.json',{'status':'complete','duplicate_inode_path_replacements':changes,'estimated_allocated_bytes_freed':freed,'free_bytes':shutil.disk_usage(W).free,'all_original_logical_paths_bytes_modes_preserved':True,'new_immutable_binaries_SDK_target_sources_changed':False,'prior_physical_inode_metadata_in_journal':True,'fresh_network_proof_before_each_coalescence':True})
holds.rmdir();print((S/'CAS-COALESCENCE-RESULT.json').read_text(),flush=True)
