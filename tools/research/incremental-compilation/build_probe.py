#!/usr/bin/env python3
"""Link session_probe.rs against an existing release zil-cli build."""
import argparse
from pathlib import Path
import subprocess
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--target',required=True,type=Path)
p.add_argument('--out',required=True,type=Path)
a=p.parse_args()
d=a.target.resolve()/'release/deps'
cmd=['rustc','--edition=2024','-O','-C','panic=abort',str(Path(__file__).with_name('session_probe.rs')),'-L',f'dependency={d}']
llvm=subprocess.check_output(['llvm-config','--libdir'],text=True).strip()
cmd+=['-L',f'native={llvm}']
for name in ['zil_driver','zil_base']:
 libs=list(d.glob('lib'+name+'-*.rlib'))
 if len(libs)!=1: raise SystemExit(f'Expected one {name} build in {d}: {libs}')
 cmd+=['--extern',f'{name}={libs[0]}']
cmd+=['-o',str(a.out)]
subprocess.run(cmd,check=True)
