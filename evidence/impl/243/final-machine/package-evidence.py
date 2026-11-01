import pathlib,json,hashlib,shutil,subprocess,tarfile,os
base=pathlib.Path('/workspace/.zil-env'); root=pathlib.Path('/workspace/zil-243');out=root/'evidence/impl/243/final-machine';out.mkdir(exist_ok=True)
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
assert commit=='9411e92db84fa9a71c40dfbe5cc0e67f1cfaa1f4'
# Capture clean tracked code before writing any new receipt.
tracked_status=subprocess.check_output(['git','status','--porcelain','--untracked-files=no'],cwd=root,text=True)
assert not tracked_status
logs=out/'logs';logs.mkdir(exist_ok=True)
for p in base.glob('243-*.log'):shutil.copy2(p,logs/p.name)
shutil.copy2(base/'243-final-cli.py',out/'run-final-cli.py')
shutil.copy2(base/'243-cli-final-checks/RESULT.json',out/'CLI-RESULT.json')
final=base/'243-machine-final01'
reports=[]
for p in sorted(final.glob('*/report.json')):
 r=json.loads(p.read_text());assert r['status']=='Success' and r['verification_runs']==1
 n=r['native_codegen'];assert n['fallback'] is None
 if n['backend']=='llvm-api':
  assert len(n['object_reports'])==1
  obj=n['object_reports'][0];assert obj['input_kind']=='builder' and obj['parse_ns']==0 and obj['verify_before_after']
  if (p.parent/'api.o').exists():assert obj['object_bytes']==(p.parent/'api.o').stat().st_size
 reports.append({'directory':p.parent.name,'backend':n['backend'],'report_sha256':sha(p),'executable_sha256':sha(p.parent/'program'),'object_reports':n['object_reports']})
executions=[];failures=0
for p in sorted(final.glob('*/*.exit')):
 prefix=p.with_suffix('');row={'directory':p.parent.name,'name':p.stem,'exit':p.read_text().strip(),'stdin_hex':prefix.with_suffix('.stdin').read_bytes().hex(),'stdout':prefix.with_suffix('.stdout').read_text(),'stderr':prefix.with_suffix('.stderr').read_text()}
 if 'fail-' in p.stem:
  stats=json.loads(row['stderr']);assert stats['live']==0 and stats['allocated']==stats['frees'];failures+=stats['failures'];row['allocation']=stats
 executions.append(row)
assert len(reports)==28 and len(executions)==74 and failures==6
(out/'BUILD-REPORTS.json').write_text(json.dumps(reports,indent=2)+'\n');(out/'EXECUTIONS.json').write_text(json.dumps(executions,indent=2)+'\n')
production=subprocess.check_output(['git','ls-files','crates/zil-lower','crates/zil-backend-llvm'],cwd=root,text=True).splitlines()
source={p:sha(root/p) for p in production}
(out/'PRODUCTION-MANIFEST.json').write_text(json.dumps(source,indent=2)+'\n')
# All intermediate artifacts are retained; these are observations with their
# original identities, never additional final-candidate coverage.
dirs=['243-source01','243-machine-source01','243-machine-source02','243-machine-source03','243-machine-source04','243-machine-source05','243-machine-final01','243-consumer244-memo01','243-consumer244-memo02','243-cli-final-checks']
manifest={}
for name in dirs:
 for p in sorted((base/name).rglob('*')):
  if p.is_file():manifest[str(p.relative_to(base))]={'bytes':p.stat().st_size,'sha256':sha(p)}
(out/'ARTIFACT-MANIFEST.json').write_text(json.dumps(manifest,indent=2)+'\n')
archive=out/'raw-artifacts.tar.gz'
with tarfile.open(archive,'w:gz') as tar:
 for name in dirs:tar.add(base/name,arcname=name)
receipt={'status':'AuthorLimitedMachinePass','implementation_commit':commit,'spec_hash':'c155bd918e73f365de6a7dc210dd9f08bcd807e402921336498631d54eb8cc72','normative_hash':'0e3808d47c1a9e7fcd2aeb06733a3d1ab3ed8cd00def193a193a0e0d55526e66','compiler_cli_sha256':sha(base/'target-243/debug/zil'),'compiler_build_log':'logs/243-cli-final01.log','compiler_build_seconds_reported':16.68,'production_source_clean_at_build_and_cli_checks':True,'target':'/workspace/.zil-env/target-243','jobs':1,'incremental':False,'llvm':'23.1.4','private_unit_tests':{'backend':5,'lower':1,'fail':0},'source_tests':{'impl243_machine':5,'impl243_default_consumer':1,'fail':0},'this_final_source_test_batch':{'builds':28,'ordinary_machine_builds':24,'allocator_api_object_builds':2,'fixed_project_default_builds_no_execution':2,'native_executions':74,'ordinary_executions':60,'allocator_executions':14,'actual_allocation_failures':6,'all_final_live_zero':True,'all_allocated_equals_frees':True},'extra_final_cli_checks':{'source_checks':6,'success_verify1':5,'nearest_double_consume_analyze_reject':1,'standard_cli_five_module_api_builds':1,'native_executions':0},'strict_clippy':{'own_lower_backend_all_targets_no_deps':'logs/243-machine-clippy04.log','own_two_driver_tests_no_deps':'logs/243-machine-test-clippy01.log'},'raw_archive':{'path':archive.name,'bytes':archive.stat().st_size,'sha256':sha(archive),'member_file_count':len(manifest)},'independent_consumer_not_counted_here':{'author':'/root/review_language_contracts','source_checkpoint':'627b222ae52e5c4db1f0cd317c8502f1a8fc9082','log':'/tmp/consumer244-memo-dev01.log','api_builds':14,'native_executions':476,'five_module_normal_executions':74,'five_module_allocator_executions':380,'actual_allocator_failures':252,'frozen_six_probe_executions':22,'status':'ReportedPass; independent reviewer owns original reports'},'intermediate_failures_preserved':['machine-source01 new probe invalid unsafe expression parse rejection; corrected only new source syntax','machine-unit03 private Empty variant recipe assertion; fixed Empty plan, no source oracle changed','cli-memo02 compile error missing LirView.entry; passed actual selected EntryPoint','machine-clippy01/02 strict lint failures; equivalent code/test initialization fixes','consumer244-memo01 Analyze ResourceLimit on old pre-241-performance combination; no native','machine-source04 new Clang report assertion incorrectly required API object reports; test-only correction'], 'not_claimed':['Formal whole-workspace pass','Complete ZUI/framework or full language','Runtime/RSS/compile performance victory over Rust','Native Windows run','Intermediate replay counts as new final coverage']}
(out/'RESULT.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps({'archive_bytes':archive.stat().st_size,'raw_files':len(manifest),'reports':len(reports),'executions':len(executions),'failures':failures,'sha':sha(archive)},indent=2))
