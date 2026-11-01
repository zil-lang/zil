import re, pathlib,collections,copy,json
p=pathlib.Path('/workspace/.zil-env/236-joint01/test-reports/consumer236-verify-61634/phases/typed.txt')
s=p.read_text().split('        TypedFunction {')[1]
local={}
for item in re.findall(r'^                LocalDecl \{\n(.*?)^                \},',s,re.M|re.S):
 i=int(re.search(r'id: LocalId\(\s*(\d+)',item)[1]); name=re.search(r'name: "(.*?)"',item)[1]; ty=re.search(r'ty: ([^\n]+)',item)[1]
 local[i]=(name,ty)
blocks=[]
for b in re.findall(r'^                BasicBlock \{\n(.*?)^                \},',s,re.M|re.S):
 bid=int(re.search(r'id: BlockId\(\s*(\d+)',b)[1]); instructions=[]
 for item in re.findall(r'^                        Instruction \{\n(.*?)^                        \},',b,re.M|re.S):
  k=re.search(r'kind: (\w+)',item)[1]
  def get(name):
   m=re.search(name+r': \w+\(\s*(\d+)',item);return int(m[1]) if m else None
  l=get('local')
  if k in ['StorageLive','StorageDead','Load']:l=int(re.search(k+r'\(\s*LocalId\(\s*(\d+)',item)[1])
  res=re.search(r'result: Some\(\s*ValueId\(\s*(\d+)',item)
  span=re.search('start: (\d+),\s*end: (\d+)',item)
  instructions.append(dict(kind=k,local=l,scope=get('scope'),value=get('value'),callee=get('callee'),result=int(res[1]) if res else None,span=span.groups() if span else None))
 term=b.split('                    terminator: ')[-1]
 targets=[int(v) for v in re.findall('BlockId\(\s*(\d+)',term)]
 blocks.append((bid,instructions,targets))
print('LOCALS',json.dumps(local))
print('BLOCKS',len(blocks))
for bi,ops,targets in blocks:
 print('BLOCK',bi,'->',targets)
 for o in ops:
  if o['kind'] in ['Store','TakeLocal','RegisterDefer','DropManaged','DropPackage','DropClosure','ScopeEnter','ScopeExit']:
   print(' ',o,'localname=',local.get(o['local']))
# Approximate exactly new defer mixed-order transfer, joining active owners and
# later edges by union; structural state is already proven in actual checker.
blocks={i:(ops,targets) for i,ops,targets in blocks}
actions={o['local'] for ops,_ in blocks.values() for o in ops if o['kind']=='RegisterDefer'}
own={i for i,(_,ty) in local.items() if ty.startswith(('Package','Managed','Closure'))}
inputs={0:(set(),set(),{},[])}; q=collections.deque([0])
while q:
 bi=q.popleft(); active,edges,pending,path=copy.deepcopy(inputs[bi]);path=path+[bi]
 ops,targets=blocks[bi]
 def forget(l):
  active.discard(l);edges.difference_update({(a,b) for a,b in edges if l in (a,b)})
 for pos,o in enumerate(ops):
  l=o['local'];k=o['kind']
  if k=='Store' and l in own and l not in actions or k=='RegisterDefer':
   forget(l);edges.update((a,l) for a in active);active.add(l)
   if k=='RegisterDefer':pending[l]=o['scope']
  elif k=='TakeLocal':
   if l not in actions:forget(l)
   else:
    newer=[b for a,b in edges if a==l and b in active]
    if newer: print('FAIL ACTION',path,'pos',pos,'op',o,'newer',[(v,local[v]) for v in newer]);raise SystemExit()
  elif k=='CallClosureOnce':
   if pos and ops[pos-1]['kind']=='TakeLocal':forget(ops[pos-1]['local']);pending.pop(ops[pos-1]['local'],None)
  elif k in ['DropManaged','DropPackage','DropClosure','DropOwnedArray','DropErrorUnion']:
   newer=[b for a,b in edges if a==l and b in active]
   if newer:print('FAIL DROP',path,'pos',pos,'op',o,'local',local[l],'newer',[(v,local[v]) for v in newer]);raise SystemExit()
   forget(l)
  elif k=='StorageDead':forget(l)
 for target in targets:
  if target not in inputs:inputs[target]=(copy.deepcopy(active),copy.deepcopy(edges),copy.deepcopy(pending),path);q.append(target)
  else:
   a,e,p,oldpath=inputs[target]
   if not active<=a or not edges<=e:
    a.update(active);e.update(edges);q.append(target)
