import os,sys
from pathlib import Path
binary,source,cache,namespace,key,clang,kind,enabled=sys.argv[1:]
name=f'.{key}.{os.getpid()}.0.tmp' if kind=='parse' else f'.pending-{os.getpid()}-0'
output=Path(namespace)/name
argv=[binary,'build',source,'--json','--clang',clang,'-o',str(output)]
if enabled=='yes':argv+=['--object-cache' if kind=='object' else '--query-cache',cache]
os.execv(binary,argv)
