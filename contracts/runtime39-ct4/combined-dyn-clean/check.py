from pathlib import Path
import subprocess, os, hashlib, json
out=Path('/tmp/342-public39/combined-dyn-clean'); deps=Path('/workspace/.zil-env/target342/debug/deps'); check=out/'compile-check';check.mkdir(exist_ok=True)
log=[]
def run(cmd):
 p=subprocess.run(cmd,capture_output=True,text=True);log.append(' '.join(map(str,cmd))+'\n'+p.stdout+p.stderr+f'\nexit={p.returncode}\n');(out/'compile-check.log').write_text('\n'.join(log));
 if p.returncode: raise SystemExit(p.returncode)
run(['rustc','--edition=2024','--crate-name','zil_base','--crate-type=rlib',str(out/'crates/zil-base/src/lib.rs'),'-L',f'dependency={deps}','--extern',f'sha2={next(deps.glob("libsha2-*.rlib"))}','--extern',f'rustc_apfloat={next(deps.glob("librustc_apfloat-*.rlib"))}','--out-dir',str(check)])
run(['rustc','--edition=2024','--crate-name','zil_ir','--crate-type=rlib',str(out/'crates/zil-ir/src/lib.rs'),'-L',f'dependency={deps}','-L',f'dependency={check}','--extern',f'zil_base={check}/libzil_base.rlib','--extern',f'rustc_apfloat={next(deps.glob("librustc_apfloat-*.rlib"))}','--out-dir',str(check)])
