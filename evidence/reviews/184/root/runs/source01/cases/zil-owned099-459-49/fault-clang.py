#!/usr/bin/env python3
import os,sys
compiler="/workspace/scratch/39be3faa4442/toolchains178/llvm-17.0.6/usr/lib/llvm-17/bin/clang"
extra=['-x','c',"/workspace/scratch/39be3faa4442/run-logs/184-review/source01/cases/zil-owned099-459-49/fault-allocator.c",'-Wl,--wrap=malloc','-Wl,--wrap=realloc','-Wl,--wrap=free']
os.execvp(compiler,[compiler]+sys.argv[1:]+extra)
