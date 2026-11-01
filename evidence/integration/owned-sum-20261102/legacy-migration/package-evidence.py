from pathlib import Path
import json,hashlib,subprocess,shutil,tarfile,gzip,re
root=Path('/workspace/zil-243');base=Path('/workspace/.zil-env');out=root/'evidence/integration/owned-sum-20261102/legacy-migration'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
for name in ['legacy-sum-migration01.log','legacy-sum-migration02.log','legacy-sum-migration-clippy01.log']:shutil.copy2(base/name,out/name)
with gzip.open(out/'original-P-formal-workspace.log.gz','wb') as f:f.write((base/'formal-owned-sum/workspace.log').read_bytes())
shutil.copy2('/tmp/legacy-source-preservation.py',out/'check-input-preservation.py')
patch=subprocess.check_output(['git','diff','--','crates/zil-driver/tests'],cwd=root);(out/'MIGRATION.patch').write_bytes(patch)
raw={}
for name in ['legacy-sum-migration01','legacy-sum-migration02']:
 for p in sorted((base/name).rglob('*')):
  if p.is_file():raw[str(p.relative_to(base))]={'bytes':p.stat().st_size,'sha256':sha(p)}
(out/'RAW-MANIFEST.json').write_text(json.dumps(raw,indent=2)+'\n')
with tarfile.open(out/'raw-P-migration.tar.gz','w:gz') as t:
 for name in ['legacy-sum-migration01','legacy-sum-migration02']:t.add(base/name,arcname=name)
reassign=[]
for p in (base/'legacy-sum-migration01').glob('owned-results166-*/main.zil'):
 if 'var v:E!P=P{r:R{code:65}};let later=R{code:90};v=P{r:R{code:66}}' in p.read_text():
  d=p.parent;r=json.loads((d/'compile.json').read_text());assert (d/'stdout').read_bytes()==b'AZB';assert (d/'exit.txt').read_text().strip()=='Some(0)';assert r['status']=='Success' and r['verification_runs']==1
  reassign.append({'dir':d.name,'source_sha256':sha(p),'stdout':'AZB','exit':0,'report_sha256':sha(d/'compile.json'),'native_codegen':r['native_codegen']})
assert len(reassign)==2
assert reassign[0]['source_sha256']==reassign[1]['source_sha256']
(out/'166-REASSIGNMENT.json').write_text(json.dumps(reassign,indent=2)+'\n')
changed=subprocess.check_output(['git','diff','--name-only'],cwd=root,text=True).splitlines()
allowed=json.loads((out/'ORIGINAL-MANIFEST.json').read_text())['files'];assert set(changed).issubset(allowed)
for p,expected in allowed.items():
 if p.endswith('.zil'):assert sha(root/p)==expected
receipt={'status':'MigrationReadyWithKnownPVerifierFailure','role':'legacy-test migration author; not 243 production acceptance','production_commit':'d393f47db2a1e3da23d1d52c875fb142c193ef14','tests_state':'P plus MIGRATION.patch; no production modifications','patch_sha256':sha(out/'MIGRATION.patch'),'changed_targets':changed,'selection':{'first_six_targets_pass':100,'review223_pass':1,'review223_fail':1,'review235_separate_pass':7,'combined_distinct_tests_pass':108,'combined_distinct_tests_fail':1,'ignored':0,'filtered':0},'known_fail':'review223 non-unit-sum-2 direct owning ErrorUnion in ErrorUnion error slot is outside owned_shapes::shape but P Verify accepts it. Expected rejection unchanged; 242 repair pending.','166_original_source_native':{'optimizations':['O0','O2'],'stdout':'AZB','exit':0,'actual_executions':2},'clippy':'8 exact test targets, llvm-api, --no-deps -D warnings: Pass','old_formal_P':'3548/12; zero ignored/filtered; gzip original log retained, not overwritten','scope':'only eight test assertion/harness expectations and this evidence directory; original program literals, fixtures and raw mutation constructions unchanged','raw_archive':{'path':'raw-P-migration.tar.gz','bytes':(out/'raw-P-migration.tar.gz').stat().st_size,'sha256':sha(out/'raw-P-migration.tar.gz'),'files':len(raw)},'not_claimed':['P full pass','fixed direct nested ErrorUnion profile','new whole-workspace acceptance','sum permission for capture/shared/StoreElement/arrays/FFI/resource heads']}
(out/'RESULT-P.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps({'rawfiles':len(raw),'archivebytes':receipt['raw_archive']['bytes'],'pass':108,'fail':1,'166':2}))
