import json,pathlib,hashlib,subprocess,collections
B=pathlib.Path('/workspace/.zil-env/236-finalQ'); R=pathlib.Path('/workspace/zil-core'); OLD=pathlib.Path('/workspace/.zil-env/236-joint02/attempt02'); P='e92508be996a190fa188d08319d37cc8b522bae6'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def read(p):return json.loads(p.read_text())
def git(*args):return subprocess.check_output(['git','-C',str(R),*args],text=True).strip()
def manifest(base,p,key):
 x=read(p)[key];return {'count':len(x),'sha256':sha(p),'mismatches':[n for n,h in x.items() if not (base/n).is_file() or sha(base/n)!=h]}
a={'candidate_commit':P,'head':git('rev-parse','HEAD'),'git_status':git('status','--porcelain'),'P_to_Q_files':git('diff','--name-only','23c636a5ab1a50d71e5b65dd8b281f16e4216fba',P).splitlines(),'source_manifest':manifest(R,R/'tools/language/236/SOURCE-MANIFEST.json','files'),'compiler_build':read(B/'compiler-build.json'),'cli_actual_sha256':sha(pathlib.Path('/workspace/.zil-env/target-236/debug/zil')),'scheduler_sha256':sha(B/'candidate-controls.py'),'matrices':{}}
for kind in ['matrix','controls']:
 base=B/kind;x=read(base/'RESULT.json');old=read(OLD/kind/'RESULT.json');obs=x['observations'];alloc=[o['allocation'] for o in obs if o['instrumented']];bad=[];keys=[]
 for o in obs:
  name=o.get('scenario',o.get('name')); leaf='fail-'+str(o['fail_at']) if o['instrumented'] else 'ordinary'; d=base/o['opt']/name/leaf;c=read(d/'command.json');keys.append((o['opt'],name,o['instrumented'],o.get('fail_at',0)))
  expected_stdin=(str(int(name.split('-')[1]))+str(int(name.split('-')[3]))).encode().hex() if kind=='matrix' and name.startswith('count-') else {'eof':'','short-input':'32','bad-count':'3930','bad-mode':'3239'}.get(name,'')
  expected_env={'ZIL_TEST_FAIL_AT':str(o['fail_at'])} if o['instrumented'] else {}
  expected_command=[str((base/o['opt'] if kind=='matrix' else base/o['opt']/name)/('probe' if o['instrumented'] else 'program'))]
  if c['status']!='Completed' or c['exit']!=o['exit'] or c['command']!=expected_command or c['stdin_hex']!=expected_stdin or c['environment']!=expected_env or c['end']<c['start']:bad.append(str(d)+' command')
  if (d/'stdout').read_bytes()!=o['stdout'].encode():bad.append(str(d)+' stdout')
  if o['instrumented']:
   if read(d/'stderr')!=o['allocation']:bad.append(str(d)+' allocation')
  elif (d/'stderr').read_bytes():bad.append(str(d)+' ordinary stderr')
 a['matrices'][kind]={'status':x['status'],'identity':{k:v for k,v in x.items() if k in ['consumer_commit','compiler_commit','candidate_commit','cli_sha256','compiler_build_receipt_sha256','source_manifest_sha256','oracle_sha256']},'manifest':manifest(base,base/'ARTIFACT-MANIFEST.json','sha256'),'observations_identical_to_independently_prechecked_joint02':obs==old['observations'],'observations':len(obs),'ordinary':sum(not o['instrumented'] for o in obs),'allocator':len(alloc),'actual_allocation_failures':sum(q['failures'] for q in alloc),'all_live_zero':all(q['live']==0 for q in alloc),'all_allocated_equal_frees':all(q['allocated']==q['frees'] for q in alloc),'max_peak_live':max(q['peak_live'] for q in alloc),'max_peak_requested_bytes':max(q['peak_bytes'] for q in alloc),'per_optimization':dict(collections.Counter(o['opt'] for o in obs)),'duplicate_keys':len(keys)!=len(set(keys)),'raw_mismatches':bad}
a['objects']=[]
for kind in ['matrix','controls']:
 for opt in ['O0','O2']:
  for name in (['main'] if kind=='matrix' else ['leaf_control','defer_control']):
   d=B/kind/opt if kind=='matrix' else B/kind/opt/name
   report=read(d/'build/stdout'); native=report['native_codegen'];link=read(d/'probe-link/command.json');obj=d/'api.o';cmd=read(d/'build/command.json'); symbols=subprocess.check_output(['nm','-u',str(obj)],text=True).splitlines()
   a['objects'].append({'path':str(d.relative_to(B)),'source_modules':len(report['source_files']),'verification_runs':report['verification_runs'],'status':report['status'],'phase':report['phase'],'backend':native['backend'],'fallback':native['fallback'],'units':native['actual_units'],'input_kinds':[q['input_kind'] for q in native['object_reports']],'parse_ns':[q['parse_ns'] for q in native['object_reports']],'verify_before_after':[q['verify_before_after'] for q in native['object_reports']],'reported_bytes':[q['object_bytes'] for q in native['object_reports']],'actual_bytes':obj.stat().st_size,'object_sha256':sha(obj),'program_sha256':sha(d/'program'),'probe_sha256':sha(d/'probe'),'link_command':link['command'],'build_command':cmd['command'],'capture_wrapper':(d/'capture-object.sh').read_text(),'reported_native_link':report['commands'],'undefined_symbols':[s.strip() for s in symbols]})
a['compiler_build_receipt_sha256']=sha(B/'compiler-build.json');a['preflight_audit_sha256']=sha(pathlib.Path('/tmp/review236-preflight-audit.json'))
pathlib.Path('/tmp/review236-finalQ/audit.json').write_text(json.dumps(a,indent=2)+'\n')
print(json.dumps({k:v for k,v in a.items() if k not in ['objects']},indent=2))
