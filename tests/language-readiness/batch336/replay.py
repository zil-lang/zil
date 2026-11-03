"""Replay committed component/CT inputs using an externally attested compiler."""
import argparse,pathlib,subprocess,hashlib,json,sys,time
p=argparse.ArgumentParser();p.add_argument('--zil',type=pathlib.Path,required=True);p.add_argument('--compiler-source',required=True);p.add_argument('--cli-sha256',required=True);p.add_argument('--output',type=pathlib.Path,required=True);a=p.parse_args()
root=pathlib.Path(__file__).resolve().parents[3];cli=a.zil.resolve();source_commit=a.compiler_source;out=a.output.resolve();out.mkdir(parents=True,exist_ok=False)
def sha(p):return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
assert sha(cli)==a.cli_sha256, 'compiler binary differs from observed build receipt'
subprocess.run(['git','cat-file','-e',source_commit+'^{commit}'],cwd=root,check=True)
subprocess.run(['git','diff','--exit-code',source_commit,'HEAD','--','crates','Cargo.toml','Cargo.lock'],cwd=root,check=True,capture_output=True)
cases=['tests/language-readiness/batch336/component-explicit.zil','tests/language-readiness/review331/component-planned.zil','tests/language-readiness/pending/ct-type-generic-function.zil','tests/language-readiness/pending/ct-type-generic-record.zil']
r={'status':'Running','compiler_source_commit':source_commit,'cli_sha256':sha(cli),'cases':[],'source_identity_basis':'CLI SHA is supplied from observed build receipt, not authenticated by this runner; current crates/Cargo inputs equal compiler source commit.','scope':'Four real-source component/CT cases, each Clang/API O0/O2; no product/Rust cost claim'}
try:
 for index,source in enumerate(cases):
  for backend in ['clang','llvm-api']:
   for opt in ['O0','O2']:
    dst=out/f'{index}-{backend}-{opt}';dst.mkdir();exe=dst/'program'
    cmd=[str(cli),'build',source,'--entry','main','--backend',backend,'--opt',opt,'--codegen-units',('4'if index<2 else'1'),'--codegen-jobs','2','-o',str(exe),'--json']
    p=subprocess.run(cmd,cwd=root,capture_output=True,timeout=120);(dst/'build.json').write_bytes(p.stdout);(dst/'build.stderr').write_bytes(p.stderr)
    j=json.loads(p.stdout);assert p.returncode==0 and j['status']=='Success' and j['verification_runs']==1,(source,backend,opt,j)
    cg=j['native_codegen'];assert cg['backend']==backend and cg['fallback'] is None,(source,cg)
    if backend=='llvm-api':
     assert cg['actual_units']>=1,cg
     if index==0:assert cg['actual_units']>1,cg
     assert all(x['backend']=='llvm-api' and x['input_kind']=='builder' and x['pipeline']=='default<'+opt+'>'for x in cg['object_reports']),cg
    q=subprocess.run([str(exe)],cwd=root,capture_output=True,timeout=30);assert(q.returncode,q.stdout,q.stderr)==(0,b'0\n',b''),(source,backend,opt,q.returncode,q.stdout,q.stderr)
    r['cases'].append({'source':source,'source_sha256':sha(root/source),'command':cmd,'backend':backend,'opt':opt,'verification_runs':1,'codegen':cg,'llvm_sha256':sha(j['artifacts']['llvm']),'executable_sha256':sha(exe),'exit_code':q.returncode,'stdout':q.stdout.decode(),'stderr':''})
    (out/'summary.json').write_text(json.dumps(r,indent=2)+'\n')
 r['status']='Pass';r['builds']=len(r['cases']);r['executions']=len(r['cases'])
except Exception as error:r.update(status='Fail',error=repr(error));raise
finally:(out/'summary.json').write_text(json.dumps(r,indent=2)+'\n')
print(json.dumps({'status':r['status'],'builds':len(r['cases']),'executions':len(r['cases'])}))
