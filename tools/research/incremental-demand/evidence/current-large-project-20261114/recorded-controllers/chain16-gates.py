import subprocess,json,pathlib,hashlib,datetime,os,sys,re
root=pathlib.Path('/workspace/zil-push-main');source=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip();out=root/'out/runs'/('large-source-native-publication-16-'+source[:12]);out.mkdir(parents=True,exist_ok=False)
def sha(p):return hashlib.file_digest(open(p,'rb'),'sha256').hexdigest()
binding={'source':source,'tree':subprocess.check_output(['git','rev-parse','HEAD^{tree}'],cwd=root,text=True).strip(),'rustc':subprocess.check_output(['rustc','-Vv'],text=True),'dso_sha256':sha(os.environ['ZIL_LLVM_LIB_DIR']+'/libLLVM.so.23.1'),'host_build_jobs':os.environ['CARGO_BUILD_JOBS'],'host_incremental':os.environ['CARGO_INCREMENTAL'],'host_test_debug':os.environ.get('CARGO_PROFILE_TEST_DEBUG'),'explicit_native_link_profile':os.environ.get('ZIL_TEST_NATIVE_LINK_PROFILE'),'host_release_profile':'repository thin-LTO/CGU1/panic-abort; benchmark requests separately O2/CGU16/jobs1','SDK_rebuilt':False,'scope':'Unified frozen chain; command-level failures are retained, zero executed tests cannot pass.'}
(out/'binding.json').write_text(json.dumps(binding,indent=2)+'\n');print(str(out),flush=True)
commands=[('retained-source',['cargo','test','--locked','--offline','-p','zil-driver','--features','native-products','--test','retained_native_publication','--','--ignored']),('clippy',['cargo','clippy','--locked','--offline','-p','zil-expand','-p','zil-cli','--features','zil-cli/native-products','--all-targets','--','-D','warnings']),('fmt',['cargo','fmt','--all','--','--check']),('release',['cargo','build','--locked','--offline','--release','-p','zil-cli','--features','native-products'])]
passed=True
for name,cmd in commands:
 assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()==source,'frozen source HEAD changed'
 dirty=subprocess.check_output(['git','diff','--name-only','HEAD','--','crates/','Cargo.toml','Cargo.lock'],cwd=root,text=True);assert not dirty,dirty
 start=datetime.datetime.now(datetime.timezone.utc).isoformat();print(name,'START',start,flush=True);log=out/(name+'.log')
 with log.open('xb') as f:r=subprocess.run(cmd,cwd=root,stdout=f,stderr=subprocess.STDOUT)
 text=log.read_text();results=re.findall(r'^test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored;',text,re.M)
 valid=r.returncode==0 and (name in ('clippy','fmt','release') or bool(results) and all(k=='ok' and int(n)>0 and int(f)==0 and int(i)==0 for k,n,f,i in results))
 record={'source':source,'tree':binding['tree'],'command':cmd,'start':start,'end':datetime.datetime.now(datetime.timezone.utc).isoformat(),'exit':r.returncode,'valid':valid,'log_sha256':sha(log),'test_results':results}
 if name=='release' and not r.returncode:record.update({'sha256':sha(os.environ['CARGO_TARGET_DIR']+'/release/zil'),'binary':os.environ['CARGO_TARGET_DIR']+'/release/zil','exit_code':0})
 (out/(name+'.json')).write_text(json.dumps(record,indent=2)+'\n');print(name,r.returncode,valid,results,flush=True)
 if not valid:passed=False
 if r.returncode:break
(out/'completion.json').write_text(json.dumps({'source':source,'all_gates_valid':passed},indent=2)+'\n')
sys.exit(0 if passed else 1)
