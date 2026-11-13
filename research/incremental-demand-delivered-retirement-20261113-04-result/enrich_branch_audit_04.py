from pathlib import Path
import subprocess,json,hashlib
R=Path('/workspace/zil-push-main');S=R/'out/evidence/branch-retirement-04';j=json.loads((S/'INVENTORY.json').read_text());root=j['root_read']
def git(*a):return subprocess.check_output(['git','-C',str(R),*a],text=True).strip()
logs=git('log',root,'--format=%H%n%B%x00').split('\x00');subjects={}
for row in logs:
 lines=row.strip().splitlines()
 if len(lines)>1:subjects.setdefault(lines[1],[]).append(lines[0])
docs={str(p):p.read_text(errors='replace') for p in (R/'tools/research/incremental-demand').glob('*') if p.is_file() and p.suffix in ('.json','.md') and p.stat().st_size<2_000_000}
for d in j['worktrees']:
 d['ignored_all_directory_entries']=subprocess.check_output(['git','-C',d['worktree'],'ls-files','--others','--ignored','--exclude-standard','--directory'],text=True).splitlines()
 d['root_commit_subject_matches']=[];d['root_delivery_document_hits']=[]
 for row in d['unique_commits_vs_root']:
  sha,_,sub=row.partition(' ')
  if sub in subjects:d['root_commit_subject_matches'].append({'author_commit':sha,'subject':sub,'root_commits':subjects[sub]})
  for path,text in docs.items():
   for needle in [sha,sha[:9]]:
    i=text.find(needle)
    if i>=0:d['root_delivery_document_hits'].append({'path':path,'needle':needle,'excerpt':text[max(0,i-180):i+360]});break
 d['patch_integration_cherry']=git('cherry',root,d['HEAD']).splitlines() if d.get('branch') and d['worktree'] not in ['/workspace/zil-demand-archive','/workspace/zil-demand-goal'] else []
(S/'ENRICHED-INVENTORY.json').write_text(json.dumps(j,indent=2)+'\n')
for d in j['worktrees']:
 if d.get('branch') and d['worktree'] not in ['/workspace/zil-demand-archive','/workspace/zil-demand-goal','/workspace/zil-push-main','/workspace/zil','/workspace/zil-main-cache-serialization']:
  print(json.dumps({'tree':d['worktree'].removeprefix('/workspace/'),'tip':d['HEAD'][:9],'remote':d['remote_tip'],'ignored':d['ignored_all_directory_entries'],'commits':len(d['unique_commits_vs_root']),'matched_subjects':len(d['root_commit_subject_matches']),'delivery_hits':len(d['root_delivery_document_hits']),'cherry_equal':sum(x.startswith('-') for x in d['patch_integration_cherry']),'cherry_unique':sum(x.startswith('+') for x in d['patch_integration_cherry'])}))
