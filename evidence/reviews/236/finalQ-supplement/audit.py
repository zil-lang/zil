import pathlib,json,hashlib,subprocess,datetime,tarfile,gzip
R=pathlib.Path('/workspace/zil-core');F=pathlib.Path('/workspace/.zil-env/formal-languageQ');B=F/'native';D=pathlib.Path('/tmp/review236-finalQ-supplement');Q='e92508be996a190fa188d08319d37cc8b522bae6';pid=184928
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def read(p):return json.loads(p.read_text())
entry='import portable\nimport model\nimport collections\nimport session\nfn main()->i32{return 0}\n'
base=(R/'examples/mainline/zui_core236/session.zil').read_text()
suffix='\nfn held_then_move(owned: Session) -> Session {\nvar current = owned\nlet child = current.nodes\nlet held = store_at(child.items, 0)\ncurrent.nodes = child\nportable.emit(held.id)\nreturn current\n}\n'
assert (F/'commit.txt').read_text().strip()==Q
assert (F/'status-before.txt').read_text()==''
assert subprocess.check_output(['git','-C',str(R),'rev-parse','HEAD'],text=True).strip()==Q
assert subprocess.check_output(['git','-C',str(R),'status','--porcelain'],text=True)==''
finalq=pathlib.Path('/tmp/review236-finalQ/finalQ.json');assert sha(finalq)=='3d80a2d905a927d9dda4464c30f6c86493cf9aefc0550821fd05e0cfaf7089e6'
rows=[];files={};dirs=[]
for label,session,expected in [
 ('counterexample-control',base,None),
 ('missing-refill',base.replace('    current.nodes = node_set\n','',1),'resource closure: whole aggregate move requires every owned element or field live'),
 ('live-window',base+suffix,'captured Store transition invalidates a live element window'),
 ('verify',base,None),
]:
 d=B/f'consumer236-{label}-{pid}';dirs.append(d);report=read(d/'report.json');mismatches=[]
 for module in ['main','portable','model','collections','session']:
  canonical=(R/f'examples/mainline/zui_core236/{module}.zil').read_text()
  expected_source=(entry if label!='verify' else canonical) if module=='main' else session if module=='session' else canonical
  if (d/f'{module}.zil').read_text()!=expected_source:mismatches.append(module)
 assert not mismatches,(label,mismatches)
 assert len(report['source_files'])==5
 assert 'executable' not in report['artifacts'] and not (d/'program').exists()
 executed=[e['phase'] for e in report['query_events'] if e['disposition']=='executed']
 if expected:
  assert report['status']=='SafeReject' and report['phase']=='analyze' and report['verification_runs']==0
  assert len(report['diagnostics'])==1
  diag=report['diagnostics'][0]
  assert (diag['phase'],diag['category'],diag['code'],diag['message'])==('analyze','SafeReject','E-ANALYZE-IR',expected)
  assert 'types' in executed and 'analyze' in executed and 'verify' not in executed
  assert 'verified' not in report['artifacts']
 else:
  assert report['status']=='Success' and report['phase']=='verify' and report['verification_runs']==1 and report['diagnostics']==[]
  assert 'verify' in executed and 'verified' in report['artifacts']
 for f in d.rglob('*'):
  if f.is_file():files[str(f.relative_to(F))]=sha(f)
 rows.append({'case':label,'original_directory':str(d),'source_modules':5,'source_matches_exact_expected_mutation':True,'source_sha256':{n+'.zil':sha(d/(n+'.zil')) for n in ['main','portable','model','collections','session']},'report_sha256':sha(d/'report.json'),'status':report['status'],'phase':report['phase'],'verification_runs':report['verification_runs'],'executable_artifact':None,'program_file_exists':False,'diagnostics':report['diagnostics'],'executed_phases':executed,'source_expectation':{'counterexample-control':'All four original library modules plus exact no-defer main returning0; independent valid five-module control.','missing-refill':'Same control project; remove only the first current.nodes=node_set refill. Whole current later moves with that owned child absent.','live-window':'Same control project; append exactly the unused held_then_move function: take nodes, retain store_at element, refill nodes, then read held.id after the transition.','verify':'All five original frozen consumer sources unchanged, including ordinary defer.'}[label]})
log=(F/'workspace.log').read_text();start=log.index('     Running tests/impl236_zui_core.rs');end=log.index('     Running tests/',start+20);excerpt=log[start:end]
assert 'running 3 tests' in excerpt and '3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out' in excerpt
for name in ['complete_session_reaches_independent_verify','nearest_missing_refill_and_live_window_counterexamples','runtime_counts_exit_modes_and_declared_shutdown_order']:assert f'test {name} ... ok' in excerpt
(D/'impl236-Q-excerpt.log').write_text(excerpt)
with gzip.open(D/'workspace.log.gz','wb') as out:out.write((F/'workspace.log').read_bytes())
(D/'RAW-MANIFEST.json').write_text(json.dumps({'scope':'Q source/phase artifacts for control, two nearest negatives and original Verify; reviewer performs no new execution','original_base':str(F),'sha256':files},indent=2)+'\n')
with tarfile.open(D/'source-phase-artifacts.tar.gz','w:gz') as tar:
 for d in dirs:tar.add(d,arcname=str(d.relative_to(F)))
with tarfile.open(D/'source-phase-artifacts.tar.gz','r:gz') as tar:
 observed={m.name:hashlib.sha256(tar.extractfile(m).read()).hexdigest() for m in tar.getmembers() if m.isfile()}
 assert observed==files
result={'reviewer':'/root/ir_defer227','verdict':'Pass','status':'Pass','implementation_commit':Q,'reviewed_at_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'scope':'Supplement to unchanged finalQ236 review: actual Q formal default workspace source controls and ownership counterexamples, read-only verification of root execution receipts.','execution_owner':'/root','reviewer_new_native_executions':0,'production_or_core_HEAD_modified_by_reviewer':False,'identity':{'formal_commit_file_sha256':sha(F/'commit.txt'),'formal_commit':(F/'commit.txt').read_text().strip(),'formal_status_before':'','core_current_status':'','formal_workspace_log_sha256':sha(F/'workspace.log'),'formal_workspace_exit':int((F/'workspace.exit').read_text()),'unchanged_finalQ_review_sha256':sha(finalq),'unchanged_finalQ_review_path':'evidence/reviews/236/finalQ.json'},'impl236_subbinary':{'tests_passed':3,'tests_failed':0,'tests_ignored':0,'tests_filtered':0,'excerpt_sha256':sha(D/'impl236-Q-excerpt.log'),'root_native_executions_in_runtime_test':40,'these_executions_added_to_consumer364':False},'source_cases':rows,'raw_artifacts':{'files':len(files),'manifest_sha256':sha(D/'RAW-MANIFEST.json'),'archive_sha256':sha(D/'source-phase-artifacts.tar.gz'),'archive_members_match_source_manifest':True},'counting':{'original_Q_consumer_matrix_and_controls':364,'original_Q_injected_failures':208,'original_Q_report_overwritten':False,'new_review_or_supplement_native_executions':0,'root_workspace_runtime_test_native_executions':40,'overlapping_runs_not_summed_as_independent_coverage':True},'conclusion':'The actual Q no-defer control and original consumer reach Verify once. Both exact nearest ownership mutations pass Types and are rejected at Analyze with the original exact E-ANALYZE-IR messages, Verify0 and no executable. This adds current-Q negative evidence without relabeling P/42 or changing the364 matrix count.','findings':[],'limits':'Finite236 ownership/control-flow slice only; does not promote full language/ZUI, product, native Windows or Rust comparative goals.'}
(D/'finalQ-negative-supplement.json').write_text(json.dumps(result,indent=2)+'\n')
(D/'FILES.json').write_text(json.dumps({'sha256':{p.name:sha(p) for p in sorted(D.iterdir()) if p.is_file() and p.name!='FILES.json'}},indent=2)+'\n')
print(json.dumps({'reviewer':result['reviewer'],'verdict':result['verdict'],'implementation_commit':Q,'cases':len(rows),'raw_files':len(files),'supplement':str(D/'finalQ-negative-supplement.json'),'sha256':sha(D/'finalQ-negative-supplement.json')}))
