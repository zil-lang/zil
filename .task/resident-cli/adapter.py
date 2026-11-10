#!/usr/bin/env python3
"""Only translate the old research worker argv; no caching or timing logic here."""
import os,sys
from pathlib import Path
mode,source,output,cache,capacity=sys.argv[1:]
exe=os.environ['ZIL_RESIDENT_CLI'];cache=Path(cache)
args=[exe,'check' if mode=='check' else 'build',source,'--json','--resident','--resident-capacity',capacity,'--resident-reset-after','64','--semantic-cache',str(cache/'semantic'),'--object-cache',str(cache/'objects'),'--codegen-units','8','--codegen-jobs','2','--clang',os.environ['ZIL_CLANG']]
if mode!='check':args+=['-o',output,'--opt',mode]
if mode=='O2':args+=['--incremental-o2','--function-merging','after']
os.execv(exe,args)
