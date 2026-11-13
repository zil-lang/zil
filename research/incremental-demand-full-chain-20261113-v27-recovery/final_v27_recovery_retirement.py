from pathlib import Path
import subprocess,json,hashlib,os,shutil,urllib.request,time
A=Path('/workspace/zil-demand-archive');R=Path('/workspace/zil-push-main');V=R/'out/evidence/full-push-checkpoint-27';S=R/'out/evidence/branch-retirement-08';assert S.is_dir()
TIP='4ca7d7ff7dba2e5e4526dd35d6dcc896fb598599';ROOT='de9a8f892abfe9b7f80912b266f228bee938081f';BR='research/compact-scalar-machine';T=Path('/workspace/zil-compact-scalar-machine');REF='refs/heads/archive/source-history'
P='research/incremental-demand-compact-source-convergence-20261113';D=A/P;Q='research/incremental-demand-full-chain-20261113-v27-recovery';E=A/Q
def save(p,v):
 with p.open('x') as f:json.dump(v,f,sort_keys=True,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
def run(argv,label,check=True):
 p=subprocess.run(argv,capture_output=True)
 with (S/'operations.jsonl').open('a') as f:f.write(json.dumps({'label':label,'argv':argv,'exit':p.returncode,'stdout':p.stdout.decode(errors='replace'),'stderr':p.stderr.decode(errors='replace')})+'\n');f.flush();os.fsync(f.fileno())
 if check:assert p.returncode==0,(label,p.stderr)
 return p
def git(*args):return subprocess.check_output(['git','-C',str(A),*args]).decode().strip()
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def fresh_files(commit,folder,rel):
 records=[]
 for p in sorted(folder.rglob('*')):
  if not p.is_file():continue
  name=rel+'/'+p.relative_to(folder).as_posix();u='https://raw.githubusercontent.com/zil-lang/zil/'+commit+'/'+name;b=urllib.request.urlopen(u,timeout=60).read();assert b==p.read_bytes();records.append({'path':name,'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest(),'url':u})
 return records
def push():
 for n in range(4):
  p=run(['git','-C',str(A),'-c','pack.threads=1','-c','pack.window=0','push','origin','HEAD:'+REF],'ordinary-archive-push-'+str(n),False)
  if not p.returncode:break
  run(['git','-C',str(A),'fetch','origin',REF+':refs/remotes/origin/archive/source-history'],'fetch-concurrent')
  run(['git','-C',str(A),'merge','--no-edit','refs/remotes/origin/archive/source-history'],'preserve-concurrent-archive')
 else:raise RuntimeError('archive push failed')
 tip=git('rev-parse','HEAD');assert git('ls-remote','origin',REF).split()[0]==tip;return tip
fixed='e3c0bb1d4dae140ad26d29d86556d0e6e0f50d54'
assert git('rev-parse','HEAD')==fixed and not git('status','--porcelain')
assert git('ls-remote','origin',REF).split()[0]==fixed
sourcefiles=fresh_files(fixed,D,P)
# A new remote-only shallow repository obtains ALL original source trees/blobs.
F=Path('/tmp/zil-v27-compact-fresh-source-only-03.git');assert not F.exists();run(['git','init','--bare',str(F)],'init-isolated-full-source-proof')
run(['git','-C',str(F),'-c','gc.auto=0','fetch','--no-tags','--depth=1','https://github.com/zil-lang/zil.git',TIP,ROOT],'fresh-fetch-complete-source-trees-and-blobs')
run(['git','-C',str(F),'fsck','--full','--no-reflogs'],'fresh-full-Git-object-check')
fresh_tree=subprocess.check_output(['git','-C',str(F),'ls-tree','-r','-l','-z',TIP]);assert fresh_tree==(D/'original-source-tree.git-ls-tree').read_bytes()
C=Path('/tmp/zil-v27-compact-fresh-archive-commit-only-03.git');assert not C.exists();run(['git','init','--bare',str(C)],'init-fresh-archive-commit-proof');run(['git','-C',str(C),'-c','gc.auto=0','fetch','--filter=tree:0','--no-tags','--depth=1','https://github.com/zil-lang/zil.git',fixed],'fetch-archive-commit-only-no-archive-payloads');raw=subprocess.check_output(['git','-C',str(C),'cat-file','commit',fixed]);parents=[x[7:] for x in raw.decode().splitlines() if x.startswith('parent ')];assert TIP in parents and ROOT in parents
for tip in [TIP,ROOT]:assert subprocess.check_output(['git','-C',str(F),'cat-file','commit',tip])==(D/(tip+'.commit')).read_bytes()
save(S/'SOURCE-FRESH.json',{'status':'verified','fixed_archive_commit':fixed,'archive_path':P,'actual_direct_parents':parents,'remote_only_fresh_git_dir':str(F),'all_original_source_trees_and_blobs_fetched':True,'Git_full_object_fsck_passed':True,'source_entries':len([x for x in fresh_tree.split(b'\0') if x]),'source_tree_listing_sha256':hashlib.sha256(fresh_tree).hexdigest(),'actual_fresh_HTTPS_files':sourcefiles})
# Recheck exact tip/empty status before ordinary atomic delete and removal.
assert git('ls-remote','origin','refs/heads/'+BR).split()[0]==TIP
assert subprocess.check_output(['git','-C',str(T),'rev-parse','HEAD']).decode().strip()==TIP
for args in [('status','--porcelain=v1','--untracked-files=all'),('ls-files','--others','--ignored','--exclude-standard','--directory')]:assert not subprocess.check_output(['git','-C',str(T),*args]).strip()
run(['git','-C',str(A),'push','--atomic','origin','--delete',BR],'ordinary-atomic-remote-delete-no-force')
assert not git('ls-remote','origin','refs/heads/'+BR)
run(['git','-C',str(A),'worktree','remove',str(T)],'ordinary-worktree-remove')
run(['git','-C',str(A),'branch','-d',BR],'ordinary-local-branch-d')
save(S/'result.json',{'status':'complete','remote_refs_deleted':1,'local_branches_deleted':1,'worktrees_removed':1,'ref':'refs/heads/'+BR,'tip':TIP,'source_fixed_commit':fixed,'source_path':P,'source_fresh_receipt_sha256':sha(S/'SOURCE-FRESH.json'),'ignored_files':0,'all_Git_objects_preserved':True,'root_worktree_branch_target_untouched':True,'build_test_native_measurement_executions':0,'next_chain_accepted':False,'V27_formal_candidate_remains_6face':True})
E.mkdir()
for label,origin in [('five-authors',R/'out/evidence/branch-retirement-07'),('compact-source-only',S)]:
 dest=E/'retirement'/label;dest.mkdir(parents=True)
 for p in origin.iterdir():
  if p.is_file():shutil.copy2(p,dest/p.name)

for failure in [R/'out/evidence/final-v27-recovery-retirement-01.log',R/'out/evidence/final-v27-recovery-retirement-01.exit',R/'out/evidence/final-v27-recovery-retirement-02.log',R/'out/evidence/final-v27-recovery-retirement-02.exit',Path('/tmp/final_v27_recovery_retirement.py'),Path('/tmp/final_v27_recovery_retirement_02.py')]:shutil.copy2(failure,E/failure.name)
shutil.copy2('/tmp/retire_v27_authors.py',E/'retire_v27_authors.py');shutil.copy2(__file__,E/'final_v27_recovery_retirement.py')
index='d2a514c899aaa8ba9150617541c3caa52a9c96d7';idx=A/'research/incremental-demand-full-chain-20261113-v27-index';text=(idx/'restore_all_https.py').read_text();assert text.count("CHECKPOINT = 'zil-push-main/out/evidence/full-push-checkpoint-24'")==1;text=text.replace("CHECKPOINT = 'zil-push-main/out/evidence/full-push-checkpoint-24'","CHECKPOINT = 'zil-push-main/out/evidence/full-push-checkpoint-27'")
(E/'restore_all_https.py').write_text(text);compile(text,'restore_all_https.py','exec')
# Exercise the revised union/master/structure selection against the SAME frozen,
# individually fresh-proven six manifests without redownloading/retesting 2.2GB.
i=json.loads((idx/'INDEX.json').read_text());m=json.loads((V/'MASTER.json').read_text());st=json.loads((V/'STRUCTURE.json').read_text());actual={}
for shard in i['shards']:
 manifest=json.loads(subprocess.check_output(['git','-C',str(A),'show',shard['fixed_archive_commit']+':'+shard['archive_path']+'/evidence-manifest.json']))
 for e in manifest['inventory']['files']:
  assert e['path'] not in actual;actual[e['path']]=(e['bytes'],e['mode'],e['sha256'])
checkpoint='zil-push-main/out/evidence/full-push-checkpoint-27';assert checkpoint+'/MASTER.json' in actual and checkpoint+'/STRUCTURE.json' in actual
assert actual[checkpoint+'/MASTER.json'][2]==i['master_sha256'];assert actual[checkpoint+'/STRUCTURE.json'][2]==i['structure_sha256'];assert len(m['files'])==m['regular_files']==i['master_files'];assert sum(e['bytes'] for e in m['files'])==i['master_bytes'];assert set(actual)=={e['path'] for e in m['files']}|set(i['envelope_paths']);assert len(actual)==i['union_unique_files']
for e in m['files']+i['envelope_entries']:assert actual[e['path']]==(e['bytes'],e['mode'],e['sha256'])
assert not st['symlinks'] and not st['special_files']
save(E/'RESTORER-VALIDATION.json',{'status':'passed','fixed_index':index,'corrected_checkpoint_path':checkpoint,'fixed_index_historical_wrapper_path_bug':'CHECKPOINT24 prevented final master/structure selection after all six pieces; only new recovery wrapper corrected, originals retained','syntax_compiled':True,'six_actual_fixed_piece_manifests_replayed':True,'full_exact_union_files':len(actual),'master_files':len(m['files']),'master_bytes':i['master_bytes'],'every_master_and_envelope_size_mode_sha256_tuple_matched':True,'new_large_download_or_compiler_execution':False,'individual_full_fresh_restore_proofs_remain_unchanged':True,'restorer_sha256':sha(E/'restore_all_https.py')})
save(E/'RECOVERY.json',{'status':'V27 complete durable artifact custody, explicit negative outcomes; later six author retirements completed','fixed_index_commit':index,'fixed_index_path':str(idx.relative_to(A)),'fixed_index_sha256':sha(idx/'INDEX.json'),'fixed_closure_commit':'8c876b6344dfbecaed2c425690442bc26e940257','fixed_closure_path':'research/incremental-demand-full-chain-20261113-v27-closure','SDK_plus_raw_pieces':6,'master_files':i['master_files'],'master_bytes':i['master_bytes'],'unique_union_files':i['union_unique_files'],'compiled_candidate':i['compiled_candidate'],'formal_main':i['formal_main'],'measurement_tools':i['measurement_tools'],'docs_source':i['docs_source'],'new_native_link_SDK_in_piece001':{'commit':'70f92d8677ee501fc5000c77fd1d48f5dc2407e8','manifest_sha256':'192bd5739d2420fb3f15afe5c1672ec4934c94046259a79d642e9ffc828f65e6'},'external_LLVM_SDK18_dependency':{'fixed_index_commit':'f0a9bf9ec100653e1736ca2db1d2bd19f3266d94','fixed_index_path':'research/incremental-demand-full-push-20261112-v23-04/V23-INDEX.json','index_sha256':'a5f619c32b16879849d7d72f3c63aeaa4793651faee1fb00bee707f61a65ef32','candidate_DSO_sha256':'785a2413d05149ba84789a588f5ca2d4d6b1ef34248ca282789acd4f3133c1d2','historical_baseline_DSO_sha256':'2bf890dac0d0a9e2bf2a81abf0a611e6893a06cbd827c525abaa1114f1ec8250'},'prior_V26_recovery_commit':'0f02d7820952cb008b34b936764148ddf8887acd','six_author_retirements':{'remote_refs_deleted':6,'local_branches_deleted':6,'worktrees_removed':6,'five_source_and_raw_fixed_in_V27':True,'compact_source_only_fixed_commit':fixed,'compact_source_only_path':P,'compact_next_chain_acceptance':False,'known_exact_fresh_ignored_removed_files':44,'known_exact_fresh_ignored_removed_bytes':3243471,'unknown_ignored_lost_files':0},'formal_matrix':{'check_rows':102,'O2_rows':102,'measured_each':54,'parent_each':48,'valid':True,'O2_hard_ELF_pairs':30,'native_oracles':1080},'publication_correctness_valid':False,'Rust_overwrite_failure_and_controller_exit2_preserved':True,'cold_broad_performance_goal_met':False,'noop_stable_lead_claimed':False,'historical_null_control_is_3ec_not_formal9170':True,'source_only_retirement_not_next_chain_acceptance':True,'formal_raw_and_compiler_SDK_paths_retained':True,'retired_author_ignored_paths_removed_only_after_exact_fresh_custody':True,'protected_root_branch_worktree_target_untouched':True,'build_test_native_measurement_executions':0,'restore_script':'restore_all_https.py','restore_script_sha256':sha(E/'restore_all_https.py')})
(E/'README.md').write_text('# V27 fixed recovery and completed author retirement\n\nThe original six pieces and final metadata were each actually restored over fresh HTTPS and checked by size, SHA256 and mode. INDEX and CLOSURE are pinned in RECOVERY.json. The corrected restore_all_https.py selects checkpoint27; the historical index wrapper selected checkpoint24. Its exact union/master/structure replay passed; full per-piece fresh proofs remain fixed.\n\nRun `python restore_all_https.py '+index+' --work-dir /fresh/download --restore-root /fresh/restored --receipt /fresh/result.json`. Both directories must be new. Restore LLVM SDK18 separately from the exact V23 index and DSO hashes in RECOVERY.json; do not substitute a different DSO. Native-link SDK192bd is in V27 piece001.\n\nCompiled6face, main9170, toolsffc1 and docs2d632 have distinct roles. Both102row matrices and1080native/30ELF checks passed; the independent Rust overwrite publication failure caused exit2. Cold/broad performance missed the target and noop did not lead stably. Historical null control is same3ec only. All first failures and negatives are retained.\n\nLater explicit user authorization retired exactly six delivered author refs/local branches/worktrees. Five were already source/raw-proven in V27; compact4ca source was newly preserved as an actual archive parent with full fresh Git tree/blob fsck, source listing and complete rootde9 diff. Compact next-chain runtime/performance acceptance is pending and separate. No root tree/target, production source, main, goal, Aff HOLD, NativeRegion, or other worker was touched. Detailed preconditions, ordinary non-forced actions and all results are under retirement/.\n')
run(['git','-C',str(A),'add','--',Q],'add-final-small-recovery-and-actual-retirements');run(['git','-C',str(A),'commit','-m','Publish fixed V27 recovery and six completed source-preserving author retirements'],'commit-final-recovery');final=push();files=fresh_files(final,E,Q);assert not git('status','--porcelain')
save(V/'RECOVERY-PUBLISHED.json',{'status':'complete','fixed_recovery_commit':final,'recovery_path':Q,'fixed_index_commit':index,'fixed_closure_commit':'8c876b6344dfbecaed2c425690442bc26e940257','compact_source_commit':fixed,'compact_source_path':P,'remote_refs_deleted':6,'local_branches_deleted':6,'worktrees_removed':6,'actual_fresh_HTTPS_files':files,'archive_status_clean':True,'no_build_test_native_measurement_executions':True,'protected_root_untouched':True,'heavy_archive_IO_finished':True,'performance_goal_met':False,'publication_correctness_valid':False,'compact_next_chain_accepted':False,'free_bytes':shutil.disk_usage('/workspace').free})
print((V/'RECOVERY-PUBLISHED.json').read_text(),flush=True)
