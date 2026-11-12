#!/usr/bin/env python3
"""Read-only ELF64/x86-64 function-code comparison; not a code-reuse proof."""
import hashlib,json,struct,sys
from pathlib import Path

def sha(b): return hashlib.sha256(b).hexdigest()
def inspect(path):
 b=Path(path).read_bytes()
 assert b[:7]==b'\x7fELF\x02\x01\x01'
 h=struct.unpack_from('<16sHHIQQQIHHHHHH',b)
 assert h[1:3]==(1,62) and h[11]==64 and 0<h[12]<100000
 def take(off,size):
  assert 0<=off<=len(b) and 0<=size<=len(b)-off
  return b[off:off+size]
 sec=[struct.unpack('<IIQQQQIIQQ',take(h[6]+64*i,64)) for i in range(h[12])]
 def data(s): return take(s[4],s[5])
 def string(table,offset):
  assert 0<=offset<len(table)
  end=table.find(b'\0',offset);assert end>=offset
  return table[offset:end].decode('utf8')
 names=data(sec[h[13]]);sn=[string(names,s[0]) for s in sec]
 tables={}
 for i,s in enumerate(sec):
  if s[1]==2:
   assert s[9]==24 and s[5]%24==0
   st=data(sec[s[6]]);syms=[]
   for off in range(0,s[5],24):
    name,info,other,section,value,size=struct.unpack_from('<IBBHQQ',data(s),off)
    syms.append(dict(name=string(st,name),info=info,other=other,section=section,value=value,size=size))
   tables[i]=syms
 relocs={}
 for s in sec:
  if s[1]==9: raise ValueError('SHT_REL not supported')
  if s[1]!=4: continue
  assert s[9]==24 and s[5]%24==0
  for off in range(0,s[5],24):
   at,info,addend=struct.unpack_from('<QQq',data(s),off);sym=tables[s[6]][info>>32]
   target=dict(name=sym['name'],info=sym['info'],other=sym['other'],section=sn[sym['section']] if sym['section']<len(sn) else sym['section'])
   relocs.setdefault(s[7],[]).append((at,info&0xffffffff,target,addend))
 functions={}
 for symbols in tables.values():
  for sym in symbols:
   if sym['info']&15!=2 or not sym['size'] or not 0<sym['section']<len(sec): continue
   start,size=sym['value'],sym['size'];s=sec[sym['section']];assert start+size<=s[5]
   rows=[dict(offset=at-start,type=typ,target=target,addend=addend) for at,typ,target,addend in relocs.get(sym['section'],[]) if start<=at<start+size]
   assert sym['name'] not in functions
   functions[sym['name']]=dict(bytes=size,code_sha256=sha(data(s)[start:start+size]),section=sn[sym['section']],section_flags=s[2],info=sym['info'],visibility=sym['other'],relocations=rows)
 return dict(path=str(path),sha256=sha(b),bytes=len(b),functions=functions)

a,z=map(inspect,sys.argv[1:3]);assert a['functions'].keys()==z['functions'].keys()
same=[n for n in a['functions'] if a['functions'][n]==z['functions'][n]]
different=[n for n in a['functions'] if n not in same]
r=dict(schema='native-function-code-opportunity-v1',compiler_source='900293cfd8afcd7118775e8165e1ec4f5266e3d7',scope='Existing formal block0 candidate cold/private CGU2 object bytes, no new compilation or timing.',cold=a,edited=z,unchanged_function_code_and_symbolic_relocations=same,changed_functions=different,unchanged_function_count=len(same),total_function_count=len(a['functions']),unchanged_code_bytes=sum(a['functions'][n]['bytes'] for n in same),cold_code_bytes=sum(v['bytes'] for v in a['functions'].values()),limits='Only function body bytes, selected ELF metadata and function-relative symbolic relocation rows are compared. Not a complete reusable native product: global/constant contents, unwind/debug/COMDAT/module state and LLVM context independence remain unproven. Same output bytes do not prove execution was skipped or an API can safely skip emission. No speedup is measured.')
print(json.dumps(r,indent=2))
