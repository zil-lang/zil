import subprocess,pathlib,json,hashlib,time
root=pathlib.Path('/workspace/zil-243');out=pathlib.Path('/workspace/.zil-env/243-cli-final-checks');out.mkdir(exist_ok=True)
cli=pathlib.Path('/workspace/.zil-env/target-243/debug/zil')
receipt={'candidate':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'status_before':subprocess.check_output(['git','status','--porcelain'],cwd=root,text=True),'cli_sha256':hashlib.sha256(cli.read_bytes()).hexdigest(),'commands':[]}
cases=sorted((root/'tests/impl244/frozen-Q').glob('*/main.zil'))
for src in cases:
 d=out/src.parent.name;d.mkdir(exist_ok=True)
 argv=[str(cli),'check',str(src),'--stop-after','verify','--emit-dir',str(d/'artifacts'),'--json']
 t=time.time();r=subprocess.run(argv,cwd=root,capture_output=True)
 (d/'stdout.json').write_bytes(r.stdout);(d/'stderr').write_bytes(r.stderr)
 report=json.loads(r.stdout)
 receipt['commands'].append({'argv':argv,'cwd':str(root),'exit':r.returncode,'seconds':time.time()-t,'status':report['status'],'phase':report['phase'],'verification_runs':report['verification_runs']})
 if src.parent.name.startswith('06-'):assert report['status']=='SafeReject' and report['phase']=='analyze',report
 else:assert report['status']=='Success' and report['verification_runs']==1,report
src=root/'tests/impl244/project/main.zil';d=out/'standard-cli-five-module';d.mkdir(exist_ok=True)
argv=[str(cli),'build',str(src),'--backend','llvm-api','--opt','O0','--emit-dir',str(d/'artifacts'),'-o',str(d/'program'),'--json']
t=time.time();r=subprocess.run(argv,cwd=root,capture_output=True);(d/'stdout.json').write_bytes(r.stdout);(d/'stderr').write_bytes(r.stderr);report=json.loads(r.stdout)
receipt['commands'].append({'argv':argv,'cwd':str(root),'exit':r.returncode,'seconds':time.time()-t,'status':report['status'],'phase':report['phase'],'verification_runs':report['verification_runs']})
assert report['status']=='Success' and report['verification_runs']==1,report
receipt['status_after']=subprocess.check_output(['git','status','--porcelain'],cwd=root,text=True)
assert receipt['status_after']==receipt['status_before']==''
(out/'RESULT.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt,indent=2))
