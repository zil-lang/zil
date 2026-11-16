from pathlib import Path
import subprocess,json,hashlib,os,re,datetime,sys
root=Path('/workspace/zil-push-main');source=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip();out=root/'out/runs'/('restart-function-gates-01-'+source[:12]);out.mkdir(exist_ok=False)
sha=lambda p:hashlib.file_digest(p.open('rb'),'sha256').hexdigest()
binding={'source':source,'tree':subprocess.check_output(['git','rev-parse','HEAD^{tree}'],cwd=root,text=True).strip(),'DSO_sha256':sha(Path(os.environ['ZIL_LLVM_LIB_DIR'])/'libLLVM.so.23.1'),'LLVM_rebuilt':False,'host_env':{k:os.environ.get(k)for k in ['CARGO_TARGET_DIR','CARGO_BUILD_JOBS','CARGO_INCREMENTAL','CARGO_PROFILE_TEST_DEBUG','ZIL_TEST_NATIVE_LINK_PROFILE']},'unchanged_gate_reuse':{'Expand124_CT76_backend299':'bcf409b37 and previousf19/f629 receipts: no API/model/Expand/CT/backend source change frombcf. Types/driver/Source/publication/strict/fmt/release rerun once for complete7filechain.'}}
(out/'binding.json').write_text(json.dumps(binding,indent=2)+'\n')
commands=[('mechanical',['cargo','check','--locked','--offline','-p','zil-cli','-p','zil-driver','-p','zil-types','--features','zil-cli/native-products,zil-driver/native-products','--all-targets']),('types',['cargo','test','--locked','--offline','-p','zil-types','--lib']),('driver',['cargo','test','--locked','--offline','-p','zil-driver','--features','native-products','--lib']),('source',['cargo','test','--locked','--offline','-p','zil-driver','--features','native-products','--test','scalar_local_promotion']),('publication',['cargo','test','--locked','--offline','-p','zil-driver','--features','native-products','--test','retained_native_publication','--','--ignored']),('strict',['cargo','clippy','--locked','--offline','-p','zil-cli','-p','zil-llvm-api','--features','zil-cli/native-products,zil-llvm-api/text-reference','--all-targets','--','-D','warnings']),('fmt',['cargo','fmt','--all','--','--check']),('release',['cargo','build','--locked','--offline','--release','-p','zil-cli','--features','native-products'])]
passed=True
for name,cmd in commands:
 assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()==source
 assert not subprocess.check_output(['git','diff','--name-only','HEAD','--','crates'],cwd=root,text=True).strip()
 log=out/(name+'.log');started=datetime.datetime.now(datetime.timezone.utc).isoformat();print(name,'START',started,flush=True)
 with log.open('x')as f:r=subprocess.run(cmd,cwd=root,stdout=f,stderr=subprocess.STDOUT)
 results=re.findall(r'^test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored;',log.read_text(),re.M)
 valid=r.returncode==0 and(name in ['mechanical','strict','fmt','release']or bool(results)and all(x[0]=='ok'and int(x[1])>0 and int(x[2])==0 and int(x[3])==0 for x in results))
 receipt={'source':source,'tree':binding['tree'],'command':cmd,'start':started,'end':datetime.datetime.now(datetime.timezone.utc).isoformat(),'exit_code':r.returncode,'valid':valid,'test_results':results,'log_sha256':sha(log)}
 if name=='release'and valid:
  p=Path(os.environ['CARGO_TARGET_DIR'])/'release/zil';receipt.update(binary=str(p),sha256=sha(p))
 (out/(name+'.json')).write_text(json.dumps(receipt,indent=2)+'\n');print(name,r.returncode,valid,results,flush=True)
 if not valid:passed=False;break
(out/'completion.json').write_text(json.dumps({'source':source,'all_gates_valid':passed},indent=2)+'\n');sys.exit(0 if passed else 1)
