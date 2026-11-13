from pathlib import Path
helper=Path('/tmp/v32_cas_restore.py').read_text()
main=r'''
def main():
 import argparse
 ap=argparse.ArgumentParser(description='Fresh HTTPS exact V32 CAS recovery. No payload execution. Python zstandard required.')
 ap.add_argument('fixed_index_commit');ap.add_argument('--work-dir',required=True);ap.add_argument('--restore-root');ap.add_argument('--receipt',required=True);ap.add_argument('--reserve-bytes',type=int,default=500*1024*1024)
 args=ap.parse_args();commit=args.fixed_index_commit;assert len(commit)==40 and all(c in '0123456789abcdef' for c in commit);work=Path(args.work_dir);assert not work.exists();work.mkdir();restore=Path(args.restore_root) if args.restore_root else None
 if restore:assert not restore.exists();restore.mkdir()
 rel='research/incremental-demand-full-chain-20261113-v32-index';base=f'https://raw.githubusercontent.com/zil-lang/zil/{commit}/{rel}/'
 def fetch(name):return urllib.request.urlopen(base+safe(name),timeout=60).read(64*1024*1024+1)
 pointer=json.loads(fetch('RESTORE.json'));body=fetch(pointer['metadata_manifest']['path']);assert hashlib.sha256(body).hexdigest()==pointer['metadata_manifest']['sha256'];manifest=json.loads(body);metadata=work/'metadata';metadata.mkdir();checked=[]
 for e in manifest['files']:
  data=fetch(e['path']);assert len(data)==e['bytes'] and hashlib.sha256(data).hexdigest()==e['sha256'];p=metadata/safe(e['path']);p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(data);p.chmod(e['mode']);assert sha(p)==e['sha256'] and stat.S_IMODE(p.stat().st_mode)==e['mode'];assert shutil.disk_usage(work).free>args.reserve_bytes;checked.append(e['path'])
 index=json.loads((metadata/'INDEX.json').read_text());master=json.loads((metadata/index['master']['path']).read_text());assert sha(metadata/index['master']['path'])==index['master']['sha256'];entries={e['path']:e for e in master['files']};catalog=json.loads((metadata/'CAS.json').read_text());objects=catalog['objects'];assert {p for o in objects for p in o['logical_paths']}==set(entries) and sum(len(o['logical_paths']) for o in objects)==len(entries)
 old=verify_legacy(objects,entries,work/'legacy-fresh',reserve=args.reserve_bytes,restore_root=restore);new=[]
 for chunk in catalog['new_chunks']:
  selected=[o for o in objects if o['sha256'] in set(chunk['objects'])];assert len(selected)==len(chunk['objects']);new.append(verify_new_chunk(chunk['fixed_commit'],chunk['archive_path'],chunk['manifest_sha256'],selected,entries,work/(chunk['tag']+'-fresh'),reserve=args.reserve_bytes,restore_root=restore))
 restored_metadata=[]
 if restore:
  for e in manifest['files']:
   if not e.get('restore_path'):continue
   target=restore/safe(e['restore_path']);target.parent.mkdir(parents=True,exist_ok=True);origin=metadata/e['path']
   if target.exists():assert target.is_file() and sha(target)==e['sha256'] and target.stat().st_size==e['bytes'] and stat.S_IMODE(target.stat().st_mode)==e['mode']
   else:
    assert shutil.disk_usage(restore).free>e['bytes']+args.reserve_bytes;shutil.copyfile(origin,target);target.chmod(e['mode']);assert sha(target)==e['sha256']
   restored_metadata.append(e['restore_path'])
  structure=json.loads((metadata/index['structure']['path']).read_text())
  for d in sorted(structure['directories'],key=lambda x:len(x['path'].split('/')),reverse=True):
   target=restore/safe(d['path']);target.mkdir(parents=True,exist_ok=True);target.chmod(d['mode']);assert stat.S_IMODE(target.stat().st_mode)==d['mode']
  wanted=set(entries)|set(restored_metadata);actual={str(p.relative_to(restore)) for p in restore.rglob('*') if p.is_file()};assert actual==wanted
 result={'status':'verified','fixed_index_commit':commit,'metadata_files_fresh_verified':len(checked),'original_logical_files':len(entries),'original_logical_bytes':master['logical_bytes'],'legacy_actual_fresh':old,'new_actual_fresh':new,'restore_root':str(restore) if restore else None,'metadata_paths_restored':len(set(restored_metadata)),'every_original_path_fresh_bytes_size_mode_verified':True,'no_payloads_executed':True,'dependencies':index['dependencies'],'boundary':master['boundary']}
 Path(args.receipt).write_text(json.dumps(result,sort_keys=True,indent=2)+'\n');print(json.dumps({k:v for k,v in result.items() if k not in ['legacy_actual_fresh','new_actual_fresh','dependencies','boundary']}))
if __name__=='__main__':main()
'''
Path('/tmp/restore_cohort_v32_https.py').write_text(helper+'\n'+main)
