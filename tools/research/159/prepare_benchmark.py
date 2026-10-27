"""Freeze before observations. Reuse source generators, never their old measurements."""
import pathlib,subprocess,json,hashlib,shutil
r=pathlib.Path(__import__('os').environ.get('ZIL_WORK_ROOT','/mnt/data/zil-round154')); repo=r/'compose159'; root=r/'evidence/compose159-benchmark-final'
subprocess.run(['python3',str(repo/'tools/research/150/prepare.py'),'--repo',str(repo),'--out',str(root)],check=True,stdout=subprocess.DEVNULL)
d=json.loads((root/'frozen.json').read_text())
selected={'tiny':'tiny','reverse128':'medium','modules64x8':'medium','tree256x8':'large','tree1024x8':'large','reverse4096':'large','cycle1024x8':'large','captured-reverse2048':'large','owned_component':'consumer','byte_buffer':'consumer','integrated_zui':'consumer','text_decode':'consumer'}
cases=[c for c in d['cases'] if c['name'] in selected]
weights={'tiny':.05,'medium':.15,'large':.60,'consumer':.20}
for c in cases:
 c['category']=selected[c['name']];c['weight']=weights[c['category']]/list(selected.values()).count(c['category']);c['capacity']=32
for name in []:
 c=next(x for x in cases if x['name']==name).copy();c.update(name=name+'-default32',category='stress',weight=0.,capacity=32);cases.append(c)
d.update(schema='zil158-full-request/1',cases=cases,variants={'A':'compose153+task-docs','C':'compose159 includes 155+156+158v2; measured anew, not inherited158 performance'},blocks=4,group_weights=weights,preflight_correction='None: fixed plan before any run; no derived weighting',cache_profile='Default32 cache capacity throughout, unchanged compiler default; syntax borrowed before existing clear guard',order='ABBA/BAAB block alternation, deterministic shuffled scenario order',scope='Verify requests include source reading, parse/expand/type/analyze/independent Verify; no LLVM link. Existing consumers edits add unused declarations, synthetic inputs modify called bodies / exported arities.')
d['binary_bindings']=json.loads((r/'evidence/driver159-binaries.json').read_text())
for label,src in [('A',r/'base'),('C',repo)]:
 d['binary_bindings'][label]['git_base']=subprocess.check_output(['git','rev-parse','HEAD'],cwd=src,text=True).strip()
 d['binary_bindings'][label]['source_files']={str(p.relative_to(src)):hashlib.sha256(p.read_bytes()).hexdigest() for p in (src/'crates').rglob('*.rs')}
d['input_hashes']={str(f.relative_to(root)):hashlib.sha256(f.read_bytes()).hexdigest() for parent in ['inputs','edits'] for f in (root/parent).rglob('*') if f.is_file()}
(root/'frozen158.json').write_text(json.dumps(d,indent=2)+'\n')
print('frozen',len(cases),'cells; weights',sum(c['weight'] for c in cases))
