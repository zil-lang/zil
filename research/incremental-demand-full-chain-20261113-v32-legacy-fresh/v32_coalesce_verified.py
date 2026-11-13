from pathlib import Path
import os,stat,json,hashlib,shutil
def coalesce_verified(objects,entries,workspace,receipt_path,holds_root):
 from v32_cas_restore import sha
 W=Path(workspace);journal=Path(receipt_path);holds=Path(holds_root);holds.mkdir();replacements=0;freed=0
 def full(p):
  s=p.lstat();return {k:getattr(s,k) for k in ['st_dev','st_ino','st_mode','st_nlink','st_uid','st_gid','st_size','st_atime_ns','st_mtime_ns','st_ctime_ns','st_blocks']}
 def stable(s):return tuple(s[k] for k in ['st_dev','st_ino','st_mode','st_uid','st_gid','st_size','st_mtime_ns'])
 def emit(v):
  with journal.open('a') as f:f.write(json.dumps(v,separators=(',',':'))+'\n');f.flush();os.fsync(f.fileno())
 for o in objects:
  modes={}
  for name in o['logical_paths']:
   if not (name.startswith('zil-push-main/out/runs/') or name.startswith('zil-demand/out/runs/')):continue
   modes.setdefault(entries[name]['mode'],[]).append(W/name)
  for mode,paths in modes.items():
   if len(paths)<2:continue
   canonical=paths[0];cs=full(canonical);assert stat.S_ISREG(cs['st_mode']) and stat.S_IMODE(cs['st_mode'])==mode and cs['st_size']==o['bytes'] and sha(canonical)==o['sha256']
   targets=[]
   for p in paths[1:]:
    s=full(p);assert stat.S_ISREG(s['st_mode']) and s['st_size']==o['bytes'] and stat.S_IMODE(s['st_mode'])==mode and sha(p)==o['sha256'];assert (s['st_dev'],s['st_uid'],s['st_gid'])==(cs['st_dev'],cs['st_uid'],cs['st_gid'])
    if s['st_ino']!=cs['st_ino']:targets.append((p,s))
   if not targets:continue
   scope={str(canonical),*[str(p) for p,s in targets]}
   for proc in Path('/proc').iterdir():
    if not proc.name.isdigit() or int(proc.name)==os.getpid():continue
    try:
     for fd in (proc/'fd').iterdir():
      try:assert os.readlink(fd) not in scope,('active_fd',str(fd))
      except OSError:pass
     maps=(proc/'maps').read_text();assert not any(p in maps for p in scope),('active_maps',proc.name)
    except (OSError,PermissionError):pass
   emit({'stage':'preconditions-after-whole-chunk-fresh-proof','sha256':o['sha256'],'canonical':str(canonical),'canonical_before':cs,'targets':[{'path':str(p),'before':s} for p,s in targets]})
   hd=holds/(o['sha256']+'-'+str(mode));hd.mkdir();done=[]
   for i,(p,s) in enumerate(targets):os.link(p,hd/str(i))
   try:
    for i,(p,s) in enumerate(targets):
     assert stable(full(p))==stable(s) and stable(full(canonical))==stable(cs) and sha(p)==o['sha256'];temp=p.parent/('.v32-new-cas-'+o['sha256'][:16]);assert not temp.exists();os.link(canonical,temp);assert stable(full(p))==stable(s);os.replace(temp,p);done.append((i,p,s));after=full(p);assert after['st_ino']==cs['st_ino'] and after['st_mode']==s['st_mode'] and sha(p)==o['sha256']
    for i,p,s in done:emit({'stage':'verified-before-hold-release','path':str(p),'sha256':o['sha256'],'before':s,'after':full(p),'canonical':str(canonical)})
    byinode={}
    for i,p,s in done:byinode.setdefault((s['st_dev'],s['st_ino']),[]).append(hd/str(i))
    for hs in byinode.values():
     st=hs[0].stat()
     if st.st_nlink==len(hs):freed+=st.st_blocks*512
    for i,p,s in done:(hd/str(i)).unlink()
    hd.rmdir();replacements+=len(done);emit({'stage':'group-complete','sha256':o['sha256'],'canonical_after':full(canonical),'replacements':len(done),'all_paths_bytes_modes_preserved':True})
   except BaseException:
    for i,p,s in reversed(done):
     h=hd/str(i)
     if h.exists():os.replace(h,p)
    emit({'stage':'aborted-old-inode-rollback-holds-retained','sha256':o['sha256']});raise
 holds.rmdir();return {'replacement_paths':replacements,'estimated_allocated_bytes_reclaimed':freed,'free_bytes':shutil.disk_usage(W).free,'all_logical_paths_bytes_modes_preserved':True,'original_physical_stat_saved_in_journal':True}
