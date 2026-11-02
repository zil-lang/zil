#!/usr/bin/env python3
"""Build a diagnostic constructor in an authorized functional window only."""
import argparse, hashlib, json, subprocess
from pathlib import Path

def identity(path):
    path=path.resolve()
    return {'path':str(path),'bytes':path.stat().st_size,'sha256':hashlib.sha256(path.read_bytes()).hexdigest()}

def main():
    p=argparse.ArgumentParser();p.add_argument('--clang',type=Path,required=True);p.add_argument('--llvm-library',type=Path,required=True);p.add_argument('--out',type=Path,required=True);a=p.parse_args()
    a.out=a.out.resolve();a.out.mkdir(parents=True,exist_ok=False);source=Path(__file__).with_name('enable_timing.c');copy=a.out/source.name;copy.write_bytes(source.read_bytes())
    cmd=[str(a.clang.absolute()),'-O2','-std=c11','-shared','-fPIC','-Wall','-Wextra','-Werror',str(copy),'-o',str(a.out/'enable_timing.so'),'-ldl']
    result=subprocess.run(cmd,capture_output=True);(a.out/'build.stdout').write_bytes(result.stdout);(a.out/'build.stderr').write_bytes(result.stderr)
    receipt={'scope':'Research constructor only; no LLVM functions interposed and no pipeline mutation','command':cmd,'exit':result.returncode,'source':identity(copy),'clang':identity(a.clang),'llvm':identity(a.llvm_library),'probe':identity(a.out/'enable_timing.so') if result.returncode==0 else None}
    (a.out/'manifest.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt,indent=2));raise SystemExit(result.returncode)

if __name__=='__main__':main()
