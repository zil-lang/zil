#!/usr/bin/env python3
"""Package already-executed core-only evidence; never alters core or its HEAD."""
from pathlib import Path
import datetime,hashlib,json,shutil,subprocess,tarfile
CORE=Path('/workspace/zil-core')
HERE=Path('/workspace/zil-244')
Q='b51dc2043ae51ec42c1bec964bec0623df152431'
R='15498f14af8f11d3a3bf4b198fe69208be5913ce'
SPEC='a7f77e9042b79f82368a13ae9712ed843ff2dead1edc31153d7c81c689b602e8'
NORM='74d508ea769bbc8e8a83649011b7553bb2dcd7fcbe78422231921c004e58de5e'
REVIEWER='/root/review_language_contracts'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def write(p,d):p.parent.mkdir(parents=True,exist_ok=True);p.write_text(json.dumps(d,indent=2,ensure_ascii=False)+'\n')
def git(*a):return subprocess.check_output(['git',*a],cwd=CORE)
assert git('rev-parse','HEAD').decode().strip()==R
assert not git('status','--porcelain').strip()
# This identity is checked in addition to the execution scripts' before/after snapshots.
for label,commit in [('Q',Q),('R',R)]:
 root=Path(f'/tmp/final-owned-sum-{label}-review')
 assert (root/'HEAD-before').read_text().strip()==commit
 assert (root/'HEAD-after').read_text().strip()==commit
 assert not (root/'status-before').read_text().strip() and not (root/'status-after').read_text().strip()
 assert (root/'dependency-blobs-before.txt').read_bytes()==(root/'dependency-blobs-after.txt').read_bytes()
 assert (root/'cargo-exit.txt').read_text().strip()=='0'
 counts=json.loads((root/'COUNTS.json').read_text());assert counts['passed']==138 and counts['failed']==0
 assert json.loads((root/'normative-hashes.json').read_text())=={'spec_hash':SPEC,'normative_hash':NORM}
 # Freeze reviewer runner sources alongside exact commands and artifact hashes.
 scripts=root/'reviewer-sources';scripts.mkdir(exist_ok=True)
 for name in ['final-review-Q.sh']+(['final-review-R.sh','review-owner-join.rs'] if label=='R' else []):
  shutil.copy2(HERE/'tools/research/244'/name,scripts/name)
 if label=='R':
  (root/'Q-to-R-production.patch').write_bytes(git('diff',Q,R,'--','crates'))
  assert (root/'clippy-exit.txt').read_text().strip()=='0'
  final=root/'owner-join/results04'
  reports=[json.loads(p.read_text()) for p in final.glob('*/report.json')]
  assert len(reports)==18
  assert sum(d['status']=='Success' and d['phase']=='analyze' for d in reports)==9
  assert sum(d['phase']=='expand' and d['status']=='SafeReject' for d in reports)==4
  assert sum(d['phase']=='expand' and d['status']=='Unsupported' for d in reports)==1
  native=[d for d in reports if d['phase']=='link'];assert len(native)==4
  for d in native:
   assert d['status']=='Success' and d['verification_runs']==1
   n=d['native_codegen'];assert n['backend']=='llvm-api' and n['fallback'] is None
   assert all(o['input_kind']=='builder' and o['parse_ns']==0 and o['verify_before_after'] for o in n['object_reports'])
  write(root/'owner-join/CLASSIFICATION.json',{
   'final_run':'results04','source_checks':14,'positive_Analyze':9,'negative_Expand':5,
   'negative_categories':{'SafeReject/E-GENERIC-ARGUMENT':4,'Unsupported/E-UNSUPPORTED/managed-give-desc':1},
   'native_builds':4,'native_executions':4,'stdout':'AB','exit':0,'optimizations':['O0','O2'],'flags':[False,True],
   'prior_exploratory_runs':['results','results02','results03'],
   'prior_fixture_errors':['alias-give diagnostic was initially required to be E-GENERIC-ARGUMENT, but the established E-UNSUPPORTED managed-give gate is correct','one additional if-expression fixture was syntactically invalid; its Parse failure is retained and excluded'],
   'result03_note':'All14 semantic checks plus4 native ran successfully; only the harness expected-text/category labels were corrected for final results04. Replays are not added to final coverage counts.',
   'origin':'Three original kernel-review diagnostic sources copied byte-for-byte;11 reviewer-authored nearest controls. Original Q blocker discovery remains kernel attribution.'})
 # A content manifest covers every raw report, source, phase, object, executable and log.
 manifest=[]
 for p in sorted(root.rglob('*')):
  if p.is_file() and p.name!='ARTIFACT-MANIFEST.json':
   manifest.append({'path':str(p.relative_to(root)),'bytes':p.stat().st_size,'sha256':sha(p)})
 write(root/'ARTIFACT-MANIFEST.json',manifest)
 dest=HERE/f'evidence/reviews/239/integration{label}'
 dest.mkdir(parents=True,exist_ok=True)
 for name in ['COUNTS.json','COMMAND.txt','targets.txt','normative-hashes.json','HEAD-before','HEAD-after','status-before','status-after','rustc.txt','cargo.txt','clang.txt','cargo-exit.txt','cargo.log','clippy.log','BUILDER-REPORTS.json']:
  if(root/name).exists():shutil.copy2(root/name,dest/name)
 archive=dest/'raw.tar.gz'
 # Original names/bytes and mtimes are preserved; no selective removal of failures.
 with tarfile.open(archive,'w:gz',compresslevel=1) as tf:tf.add(root,arcname=f'integration{label}-raw')
 write(dest/'ARCHIVE.json',{'path':str(archive.relative_to(HERE)),'sha256':sha(archive),'bytes':archive.stat().st_size,'members':len(manifest)+1,'artifact_manifest_sha256':sha(root/'ARTIFACT-MANIFEST.json'),'source_directory':str(root)})
 common={
  'reviewer':REVIEWER,'implementation_commit':commit,'schema':30,'wire':30,'spec_hash':SPEC,'normative_hash':NORM,
  'candidate_verdict':'Fail' if label=='Q' else 'PassBounded',
  'candidate_failure':{'slice':240,'discovery':'/root/kernel_defer227 supplementary source review','reason':'A live actual owner replaced on only one branch was incorrectly treated as an expired old description by definition-time generic checking. Ordinary nongeneric source succeeds, adding unused F/cb falsely rejects.','original_inputs':'kernel-owner-join-blocker/ in raw archive','repair_candidate':R} if label=='Q' else None,
  'independence':{'reviewer_authored_production':False,'reviewer_authored_consumer244_and_review_tests':True,'consumer244_independent_oracle_reviewer':'/root','core_tree_readonly':True,'full_workspace':'NotRun by this reviewer; root owns formal runs','author_results_not_added_to_reviewer_counts':True},
  'execution':{'cwd':str(CORE),'cargo_target':'/workspace/.zil-env/target-review-sum-Q','target_provenance':'Created cold for b51 Q, only core cwd; reused for R at same absolute cwd. Earlier cross-tree target244 observations classified invalid and retained separately.','features':['llvm-api'],'jobs':1,'locked':True,'selected_test_targets':16,'selected_Rust_tests':138,'executed_Rust_tests':138,'passed':138,'failed':0,'ignored':0,'filtered':0,'independent_and_consumer_tests':29,'legacy_migration_tests':109,'clippy':('same16targets -D warnings passed, exit0 captured' if label=='R' else 'same16targets -D warnings completed successfully in recorded log; separate process-exit file was not captured'),'commands':'COMMAND.txt, clippy.log, exact owner-join rustc-command.json for R','logs':'cargo.log plus full raw archive','source_dependency_blobs_before_after_equal':True,'HEAD_clean_before_after':True,'evidence_reuse':'All138 tests actually rerun at this exact candidate. Prior author/protocol/source-only evidence is identified separately.'},
  'levels':{
   'production_source':'Original six probes, five-module consumer, generated/handwritten enum, exits/defer, callback helper; full stage artifacts retained.',
   'source_derived_raw':'Freshness, Store windows, unused capture and ErrorUnion metadata attacks; public Typed/raw/Wire inputs, without native claims.',
   'handmade_raw':'Tag-edge/loop and actual-request controls use public candidates and empty Hold/initialization plumbing. Verify is invoked before Analyze; no trusted certificate fabricated.',
   'CT':'Pure Copy Enum positive evaluates I32(7); new local observers rejected even when payload Copy, raw and Wire. No owned runtime resource evaluation permitted.',
   'native':'x86_64 Linux actual LLVM API builder; no parse fallback; O0 and O2 where native specified.',
   'product_comparison':'NotRun; no full-language completion, fullZUI, cross-platform or Rust performance claim.'},
  'native_counts':{'consumer_builds':14,'consumer_executions':476,'original_frozen_probe_executions':22,'five_module_normal_invalid_executions':74,'actual_allocator_probe_executions':380,'actual_allocation_failure_executions':252,'all_live_allocations_after_failures':0,'independent_generation_match_helper_builds':24,'independent_generation_match_helper_executions':24,'main_matrix_total_builder_builds':38,'main_matrix_counted_native_executions':500,'R_owner_join_extra_builds':4 if label=='R' else 0,'R_owner_join_extra_native_executions':4 if label=='R' else 0,'total_counted_native_executions':504 if label=='R' else 500,'legacy_native':'Separate artifacts preserved; no aggregate legacy native total included. Repeated Q/R executions are not presented as new distinct scenarios.'},
  'frozen_consumer':{'source_checkpoint':'33590ffe8','first_source_checkpoint':'ec3580d86','oracle':'tests/impl244/ORACLE.json','oracle_unchanged':True,'original03_sha256':'711e96d70a4225254168ec1134d678baeb5f934a5fb75dbe0fe7d98adeaa434c','original05_sha256':'158d195a306f7e982de6e1ee860eb477c7dca7834ae4d96c07cc1e2e6c7a548f','original_unsupported_promoted_to_native':True,'sixth_probe_duplicate_consume_still_rejected':True,'ordinary_matrix':32,'invalid_inputs':5,'allocator_inputs':'190 per optimization,126 actual failed allocations per optimization; calls/frees/allocations/live compared to predeclared source-derived closed forms','first_failure_history':'ec358 qualified Unit pattern omitted (), fixture-only syntax correction335; subsequent actual Analyze/Verify/Codegen resource-limit failures fixed by implementations without changing sources/oracles/limits.'},
  'legacy_migration_review':{'author_commit':'2f73305a502c9450141f9598b901d01bb099866f','independently_reviewed':True,'source_and_raw_constructor_inputs_unchanged':True,'scope':'Only ordinary Sum and mutable whole-Sum combinations promoted; capture/shared/array/StoreElement/ownership-mode controls remain negative.','native_166_oracle':'Original source yields AZB/0 at O0/O2: commit cleans oldA, laterZ precedes original-shell newB cleanup.','direct_nested_union_negative':'review223 raw variant2 remains rejection; variants0,1,3 promoted.','raw183_note':'Separate raw183 historical shape diagnostic was root/242 responsibility; it is not one of these16 targets and is not counted as this reviewer execution.'},
  'budget':{'default_limits_unchanged':True,'consumer_passes_default_limits':True,'actual_request':'Sealed VerifiedModule limits/presence tested through owned/shared/program/Wire/current clone; Lower/emission inherit current small request, no default reset.','qualified_old_profile_exceptions':['Copy generic Enum actual instantiated field proof: Expand331/124 ->336/126 (+5work/+2nodes)','Actual Enum/ErrorUnion Copy/linear qualification in unused generic helpers: Expand266/116 ->296/130 (+30work/+14nodes); Types219 unchanged'],'exceptions_author_measurements':'Explicit current normative registration; not added to reviewer test counts. NonSum old profiles stay on prior path.','no_timing_claim':True},
  'R_owner_join_review':{'production_commit':'1246dd29cbdd68887fa7d8798a262528dd5eccaf','production_delta':'Only generic_resource_desc.rs27 lines plus4 author testcase lines; other Q->R changes cards/evidence.','static':'Current local is matched by actual id; every continuing branch must keep actual owned/declaration mode, unmoved local and roots. Only unequal actual root sets create a fresh joint owner activation. Existing borrowed descriptions retain old tokens, and moved-token sets still union. Root comparisons and new activation storage are prepaid. No new raw permission/schema/budget reset.','source_checks':14,'positive_Analyze':9,'negative_Expand':5,'native':'true/false xO0/O2 real AB/0','final_raw':'owner-join/results04','fixture_development':'Two reviewer harness errors retained and excluded; successful replay03 is not added to final counts.'} if label=='R' else None,
  'historical_failures_preserved':['P exact d393: independent helper ordinary Enum reaches Analyze Unsupported, original sources/oracles unchanged; evidence/reviews/241/integrationP','P new ordinary capture and direct-owned nested ErrorUnion false accepts retained in profile-supplement-P; root/242 discovery credited to authors','Q b51:138 tests pass but kernel actual current-owner branch join positive fails; Q verdict remains Fail','Invalid target244 cross-tree stale-dep observations are not used for acceptance/rejection conclusions','Reviewer owner-join harness expectation/Parse fixture errors retained separately'],
  'rust_goals':{'safety':'Independent kernels reject hostile source/raw/Wire cases, freshness laundering, stale tag/Desc/window/capture/profile attacks; actual allocation failures clean live resources to zero. This finite evidence does not prove full language safety.','runtime_memory':'Ordinary inline Sum cleanup reaches real native behavior without hidden RC/boxing. Allocation balance measured; no comparative runtime/RSS benchmark.','ergonomics_maintenance':'Original03/05 owning Store/error compositions and finite helper API now work; entire generic/lending/capture/framework maintenance scope remains incomplete.','compile_cost':'Default request budgets pass with explicitly paid sharing/memo, two declared old fees. No comparative compiler speed or memory result.','breakthrough':'Bounded language capability: frozen previously Unsupported owning Sum consumer sources03/05 now compile and run unchanged in O0/O2. No product net-win conclusion.'},
  'limitations':['Finite acyclic ordinary owned shapes only, explicit manages markers; no general owning generic Enum, capture/shared/StoreElement/array/FFI/async permission expansion.','No full workspace execution by this reviewer; root formal run is separate.','Not fullZUI, language1.4 completion, or product/Rust-goals net-win acceptance.','Our own244 authored consumer is independent test evidence for production239–243; root supplies independent244 audit.'],
  'raw_archive':json.loads((dest/'ARCHIVE.json').read_text()),'created_at':datetime.datetime.now(datetime.timezone.utc).isoformat()}
 write(dest/'COMMON-RESULT.json',common)
 slices={
 239:('PublicIR/Wire and CT/runtime separation', ['schema30 four new ops90–93 and precise typed paths','generated versus handwritten manages order, private iteration identities, bare payload/owning generic Enum gates','pure Copy CT positive and runtime observer raw/Wire denial'], 'review239_generation3/0; generated4 native; protocol supplement is reused only after exact production-file hash comparison'),
 240:('Ordinary finite owning Sum frontend and generic helper', ['recursive pattern tests then unpack and reverseDFS publication, source match exits/defer actual captures','seven finite helper positive variants/17 unused-definition negatives','actual resourceF versus callable binderF lexical scope, bare Desc not owner, stale roots'], 'review240_helper_sum2/0 and match_exits4/0;20 native, plus R14 semantic checks/4 native; Q branch-join blocker prevents240 Pass'),
 241:('Independent Analyze ownership/source/carrier/freshness/window facts', ['direct fresh construction versus ordinary Value return downgrading','same typed leaves and both Store window routes through relay/nonidentity Sum','tag edges and local observer invalidation, unused profile gates, callback data arguments'], 'freshness4/0, Store windows2/0, tag-edge crosscheck and capture/profile controls; no production authorship'),
 242:('Independent Verify public/raw/Wire trust boundary', ['untrusted tag/local/path/prefix/same-target/bypass/loop observations','all Unpack targets checked atomically and right-nested initialization/publication order','ordinary helper Sum versus capture/State/Managed-success profile boundaries','actual per-request limits/presence owned/shared/program/Wire/clone'], 'tag_edges4/0, capture_profiles2/0 plus source-derived raw/Wire controls; Verify fixtures do not use Analyze for permission'),
 243:('Lower/LLVM ABI, complete recursive cleanup and actual-request budgets', ['native inline active variant payloads and declared drop order','Product/Sum recursive cleanup helpers preserve exactType layout and shared current budget','frozen source normal/invalid/actual allocation failure matrix at O0/O2'], '38 actual LLVM API builds/500 counted native, plus R4 builder/native; verify1/fallbacknone/builder/parse0; legacy native separately retained')}
 for number,(scope,points,validation) in slices.items():
  verdict='Fail' if label=='Q' and number==240 else 'Pass'
  path=HERE/f'evidence/reviews/{number}/integration{label}'
  path.mkdir(parents=True,exist_ok=True)
  item={'reviewer':REVIEWER,'verdict':verdict,'implementation_commit':commit,'spec_hash':SPEC,'normative_hash':NORM,'schema':30,'scope':scope,'bounded':True,'scope_points':points,'validation':validation,'candidate_overall_verdict':common['candidate_verdict'],'common_evidence':f'evidence/reviews/239/integration{label}/COMMON-RESULT.json','raw_archive':common['raw_archive'],'production_source_authored_by_reviewer':False,'consumer244_independent_root_review':True,'full_language_or_product_completion':False,'blocking_issue':common['candidate_failure'] if number==240 else None,'evidence_reuse':'Exact candidate16targets rerun. Historical protocol/author evidence is separately attributed; no old logs represented as fresh execution.'}
  if number==239:
   old=json.loads((HERE/'evidence/reviews/239/protocol-supplement/REPORT.json').read_text());same={}
   for name,expected in old['reviewed_source_sha256'].items():
    if '/zil-ir/' in name or '/zil-ct/' in name or '/ct_selection/' in name or name.endswith('/ct_program.rs'):
     actual=hashlib.sha256(git('show',f'{commit}:{name}')).hexdigest();assert actual==expected,name;same[name]=actual
   item['protocol_supplement_reuse']={'reviewer':old['reviewer'],'source_report':'evidence/reviews/239/protocol-supplement/REPORT.json','exact_source_sha256_at_candidate':same,'actual_protocol_tests_this_batch':'NotRun','original_scope':'Public transport/CT supplement; no source authority fabricated'}
  write(path/'REVIEW.json',item)
 print(label,archive.stat().st_size,common['candidate_verdict'])
assert git('rev-parse','HEAD').decode().strip()==R and not git('status','--porcelain').strip()
