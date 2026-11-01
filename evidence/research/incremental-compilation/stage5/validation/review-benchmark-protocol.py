#!/usr/bin/env python3
"""Independent synthetic runner controls. These are NOT compiler/performance evidence."""
from pathlib import Path
import importlib.util,sys,json,hashlib,subprocess,re,os
ROOT=Path('/workspace/zil');OUT=Path('/workspace/.zil-env/research/stage5/review-protocol');OUT.mkdir(exist_ok=False)
source=ROOT/'tools/research/incremental-compilation/distribution_bench.py';snapshot=OUT/'distribution_bench.py';snapshot.write_bytes(source.read_bytes())
spec=importlib.util.spec_from_file_location('distribution_review',snapshot);mod=importlib.util.module_from_spec(spec);spec.loader.exec_module(mod);mod.ROOT=ROOT
original_measured=mod.measured
launch=OUT/'launch';launch.mkdir();launch_result=original_measured(['/tmp/no-such-zil-review-compiler-02712'],launch,'absent',1);assert launch_result['exit'] is None and launch_result['launch_error'] and launch_result['max_rss_kib'] is None
hang=OUT/'hang';hang.mkdir();timeout_result=original_measured([sys.executable,'-c','import time;time.sleep(10)'],hang,'timeout',1);assert timeout_result['timeout'] and timeout_result['exit']==-9
mod.shapes=lambda:[dict(name='protocol',sizes=[1],flavor='scalar')]
mod.REQUESTS=[('cold',0),('unchanged',0),('malformed_json',0),('early_edit',1),('late_edit',2),('compiler_failure',0),('missing_native',0),('native_timeout',0),('launch_failure',0),('restore',0)]
active={};original_run=subprocess.run
def simulated(cmd,run,label,timeout):
 arm='zil' if run.name.endswith('-zil') else 'rust';active.update(arm=arm,label=label)
 assert not (run/'program').exists(),'runner must remove previous executable before the compiler'
 (run/f'{label}.stderr').write_bytes(b'')
 report={'status':'Success','verification_runs':1,'native_codegen':None,'query_events':[]}
 result={'elapsed_ns':1000000,'exit':0,'timeout':False,'cpu_seconds':0.001,'max_rss_kib':100}
 if arm=='zil' and label=='unchanged':body='null'
 elif arm=='zil' and label=='malformed_json':body='not JSON'
 else:body=json.dumps(report)
 if arm=='zil' and label=='compiler_failure':result['exit']=17;body=json.dumps({'status':'ResourceLimit','verification_runs':0,'native_codegen':None})
 if arm=='zil' and label=='late_edit':result.update(exit=-9,timeout=True)
 if arm=='zil' and label=='launch_failure':result.update(exit=None,launch_error='synthetic launch refusal',cpu_seconds=None,max_rss_kib=None)
 (run/f'{label}.stdout').write_text(body if arm=='zil' else '')
 if not (arm=='zil' and label=='missing_native'):
  source_path=Path(cmd[2] if arm=='zil' else cmd[1]);text=source_path.read_text();value=int(re.search(r'fn h0\(x:i32\)->i32\{return x\+(\d+);\}',text).group(1))
  if arm=='zil' and label=='early_edit':value+=127
  program=run/'program';program.write_text('#!'+sys.executable+'\nimport os\nos.write(1,'+repr(value.to_bytes(4,'little'))+')\n');program.chmod(0o700)
 return result
def run_native(args,*a,**kw):
 if isinstance(args,list) and args and Path(args[0]).name=='program' and active=={'arm':'zil','label':'native_timeout'}:
  raise subprocess.TimeoutExpired(args,20,output=b'partial',stderr=b'')
 return original_run(args,*a,**kw)
mod.measured=simulated;subprocess.run=run_native
sys.argv=['distribution_bench.py','--zil',sys.executable,'--rustc',sys.executable,'--out',str(OUT/'screen'),'--blocks','1','--mode','O0']
try:mod.main()
finally:subprocess.run=original_run
rows=[json.loads(x) for x in (OUT/'screen/rows.jsonl').read_text().splitlines()];summary=json.loads((OUT/'screen/summary.json').read_text());assert len(rows)==summary['rows']==20
valid_labels={'cold','restore'}
for row in rows:
 assert row['valid']==(row['arm']=='rust' or row['request'] in valid_labels),row
 ident=json.loads((OUT/'screen'/row['source_identity_file']).read_text());assert hashlib.sha256(json.dumps(ident,sort_keys=True).encode()).hexdigest()==row['source_identity_sha256']
for cell in summary['cells']:
 assert (cell['zil_over_rust'] is None)==(cell['request'] not in valid_labels)
 if cell['request']=='launch_failure':assert cell['arms']['zil']['max_rss_kib'] is None
expected={0:1,1:2,2:3}
assert all(row['expected']==expected[row['edit']] for row in rows)
result={'level':'synthetic benchmark-runner fault controls only, not real compilers or performance','tool_sha256':hashlib.sha256(snapshot.read_bytes()).hexdigest(),'rows':20,'cells':10,'zil_invalid_rows':8,'all_failed_cell_ratios_null':True,'full_i32_rejects_expected_plus_127':True,'all_source_identities_match':True,'actual_missing_launch':launch_result,'actual_compiler_timeout':timeout_result,'simulated_controls':['JSON null','non-JSON output','incorrect full native value differing by 127','compiler timeout','compiler failure','missing native executable after exit0','native timeout','compiler launch failure'],'verdict':'Pass'}
(OUT/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
