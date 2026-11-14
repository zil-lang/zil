import pathlib,subprocess,os,json,hashlib,datetime,shutil,sys
root=pathlib.Path('/workspace/zil-baseline9170');source=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip();assert source=='9170bb4210554392e36eb7528596612596ebef80'
assert not subprocess.check_output(['git','status','--porcelain'],cwd=root,text=True).strip()
out=pathlib.Path('/workspace/zil-push-main/out/runs/main9170-host-release-control-01');out.mkdir(exist_ok=False)
cmd=['cargo','build','--locked','--offline','--release','-p','zil-cli'];start=datetime.datetime.now(datetime.timezone.utc).isoformat()
with (out/'release.log').open('x') as f:r=subprocess.run(cmd,cwd=root,stdout=f,stderr=subprocess.STDOUT)
sha=lambda p:hashlib.file_digest(open(p,'rb'),'sha256').hexdigest()
receipt={'source':source,'tree':subprocess.check_output(['git','rev-parse','HEAD^{tree}'],cwd=root,text=True).strip(),'command':cmd,'exit_code':r.returncode,'start':start,'end':datetime.datetime.now(datetime.timezone.utc).isoformat(),'environment':{k:os.environ.get(k) for k in ['CARGO_TARGET_DIR','CARGO_INCREMENTAL','CARGO_BUILD_JOBS','ZIL_LLVM_LIB_DIR','LD_LIBRARY_PATH','ZIL_LLVM_CONFIG']},'rustc':subprocess.check_output(['rustc','-Vv'],text=True),'stock_DSO_sha256':sha(os.environ['ZIL_LLVM_LIB_DIR']+'/libLLVM.so.23.1'),'log_sha256':sha(out/'release.log'),'scope':'Matched host release control; compiler remains actual main9170. No LLVM build. ZIL benchmark settings independently CGU16/O2/jobs1.'}
if not r.returncode:
 binary=pathlib.Path('/workspace/zil-push-main/out/binaries/main9170-host-control-01/zil');binary.parent.mkdir(parents=True,exist_ok=False);shutil.copy2(pathlib.Path(os.environ['CARGO_TARGET_DIR'])/'release/zil',binary);receipt.update(binary=str(binary),sha256=sha(binary))
(out/'release.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt),flush=True);sys.exit(r.returncode)
