#!/usr/bin/env python3
"""Predeclare sizes, semantic edits and workload weights; never derives weights from times."""
import argparse,hashlib,json,pathlib,shutil
p=argparse.ArgumentParser();p.add_argument('--repo',required=True);p.add_argument('--out',required=True);a=p.parse_args();repo=pathlib.Path(a.repo).resolve();out=pathlib.Path(a.out).resolve();out.mkdir(parents=True,exist_ok=False)
cases=[]
def graph(n,width=2,cyclic=False,captured=False,forward=False):
 s='resource R{n:i32}\n';params=','.join(f'a{i}:R' for i in range(width));args=','.join(f'a{(i+1)%width}' for i in range(width))
 for i in (reversed(range(n)) if forward else range(n)):
  if cyclic:body=f'if steps>0{{f{(i+1)%n}({args},steps-1)}}else{{a0.n+=1}}'
  else:body=f'f{i+1}({args},steps)' if i+1<n else 'a0.n+=1'
  s+=f'fn f{i}({params},steps:i32){{{body}}}\n'
 names=','.join(f'a{i}' for i in range(width));total='+'.join(f'a{i}.n' for i in range(width));setup=''.join(f'let a{i}=R{{n:0}};' for i in range(width))
 if captured:s+='fn make()->impl FnMut()->i32{'+setup+f'return move ()=>{{f0({names},17);return {total}}}}}\nfn main()->i32{{var f=make();return f()-1}}\n'
 else:s+='fn main()->i32{'+setup+f'f0({names},17);return {total}-1}}\n'
 return s

def add(name,text,category,kind='synthetic'):
 d=out/'inputs'/name;d.mkdir(parents=True);(d/'main.zil').write_text(text);cases.append({'name':name,'category':category,'kind':kind,'root':str(d/'main.zil'),'edit_target':str(d/'main.zil'),'capacity':1024})
add('tiny','fn main()->i32=>0\n','tiny')
for n,category in [(128,'medium'),(512,'large'),(2048,'large'),(4096,'stress')]:
 add(f'reverse{n}',graph(n),category)
 add(f'captured-reverse{n}',graph(n,captured=True),category)
for n in [128,512,1024]:add(f'cycle{n}x8',graph(n,8,True,captured=True),'stress' if n==1024 else 'large')
add('forward2048',graph(2048,forward=True),'large')
# A generated layered module DAG, eight called functions per module, up to 8192 functions.
for n,category in [(64,'medium'),(256,'large'),(1024,'stress')]:
 name=f'modules{n}x8';d=out/'inputs'/name;d.mkdir(parents=True)
 for i in range(n):
  # All modules are reached from main; local bodies and a fan-in cross-module call are exercised.
  imports=f'import m{i-1}\n' if i else ''
  text=imports+'\n'.join(f'fn f{k}(x:i32)->i32=>x+1' for k in range(8))+'\n'
  expr='x'
  for k in range(8):expr=f'f{k}({expr})'
  text+=f'pub fn run(x:i32)->i32=>'+(f'm{i-1}.run({expr})' if i else expr)+'\n'
  (d/f'm{i}.zil').write_text(text)
 (d/'main.zil').write_text(f'import m{n-1}\nfn main()->i32=>m{n-1}.run(0)-{n*8}\n')
 cases.append({'name':name,'category':category,'kind':'synthetic-module-DAG','root':str(d/'main.zil'),'edit_target':str(d/'m0.zil'),'capacity':1024})
# Same broad size as deep-import control, but logarithmic import depth.
for n in [256,1024]:
 name=f'tree{n}x8';d=out/'inputs'/name;d.mkdir(parents=True)
 for i in range(n):
  children=[j for j in [2*i+1,2*i+2] if j<n]
  text=''.join(f'import m{j}\n' for j in children)
  text+='\n'.join(f'fn f{k}(x:i32)->i32=>x+1' for k in range(8))+'\n'
  text+='pub fn run(x:i32)->i32{var y=x;'+''.join(f'y=f{k}(y);' for k in range(8))+''.join(f'y=m{j}.run(y);' for j in children)+'return y}\n'
  (d/f'm{i}.zil').write_text(text)
 (d/'main.zil').write_text(f'import m0\nfn main()->i32=>m0.run(0)-{n*8}\n')
 cases.append({'name':name,'category':'stress','kind':'synthetic-balanced-module-DAG','root':str(d/'main.zil'),'edit_target':str(d/f'm{n-1}.zil'),'parent':str(d/f'm{(n-2)//2}.zil'),'capacity':1024})
for name in ['owned_component','byte_buffer','integrated_zui','text_decode']:
 d=out/'inputs'/name;shutil.copytree(repo/'examples/mainline'/name,d)
 cases.append({'name':name,'category':'consumer','kind':'existing-source-consumer','root':str(d/'main.zil'),'edit_target':str(d/'main.zil'),'capacity':1024})
for c in cases:
 target=pathlib.Path(c['edit_target']);original=target.read_text();ed=out/'edits'/c['name'];ed.mkdir(parents=True)
 if 'a0.n+=1' in original:
  private=original.replace('a0.n+=1','a0.n+=2');public=original.replace('a0.n+=1','a1.n+=1')
  meaning='change called write 1->2; then change directly written argument a0->a1 while retaining FnMut'
 elif c['name'].startswith('modules'):
  private=original.replace('x+1','x+2',1);public=private.replace('pub fn run(x:i32)->i32','pub fn run(x:i32,y:i32)->i32')
  # Public signature change must migrate its immediate caller, not create an invalid benchmark.
  caller=target.with_name('m1.zil');caller_text=caller.read_text().replace('m0.run(f7','m0.run(f7')
  # Place new argument at the closing parenthesis of the exported caller expression.
  lines=caller_text.splitlines();lines[-1]=lines[-1][:-1]+',0)';caller_text='\n'.join(lines)+'\n'
  (ed/'public_caller.zil').write_text(caller_text)
  meaning='change a called leaf computation; then exported arity plus caller migration'
 elif c['name'].startswith('tree'):
  private=original.replace('x+1','x+2',1);public=private.replace('pub fn run(x:i32)->i32','pub fn run(x:i32,extra:i32)->i32')
  caller=pathlib.Path(c['parent']);caller_text=caller.read_text().replace(target.stem+'.run(y)',target.stem+'.run(y,0)')
  (ed/'public_caller.zil').write_text(caller_text);meaning='called leaf body then exported arity/caller migration, balanced module graph'
 elif c['name']=='tiny':
  private=original.replace('=>0','=>1');public=private+'pub fn declared()->i32=>2\n';meaning='called root body then added export (tiny control)'
 else:
  # Existing consumers remain unmodified for cold/warm measurements; edit mode is stated explicitly.
  private=original+'\nfn scale141_private()->i32=>17\n';public=private+'\npub fn scale141_export()->i32=>29\n';meaning='add unused private/exported declaration; not called-body editing'
 for scenario,text in [('private_edit',private),('public_edit',public)]:
  replacement=ed/(scenario+'.zil');replacement.write_text(text);manifest=str(target)+'\t'+str(replacement)+'\n'
  if scenario=='public_edit' and c['name'].startswith(('modules','tree')):manifest+=str(caller)+'\t'+str(ed/'public_caller.zil')+'\n'
  (ed/(scenario+'.tsv')).write_text(manifest)
 (ed/'scenarios.txt').write_text('cold\nunchanged\nprivate_edit\npublic_edit\n');c['edits']=str(ed);c['edit_semantics']=meaning
 sources=sorted(pathlib.Path(c['root']).parent.rglob('*.zil'));texts=[f.read_text() for f in sources]
 c['size']={'modules':len(sources),'source_bytes':sum(f.stat().st_size for f in sources),'lines':sum(x.count('\n') for x in texts),'declared_functions':sum(x.count('fn ') for x in texts)}
# Target workload assumptions, not measured user frequencies. Stress cells have no timing weight.
group_weights={'tiny':.05,'medium':.15,'large':.60,'consumer':.20,'stress':0.0}
counts={g:sum(c['category']==g for c in cases) for g in group_weights}
for c in cases:c['weight']=group_weights[c['category']]/counts[c['category']]
spec={'schema':'zil-141-scale/2','preflight_correction':'V1 revealed unsupported opaque Fn-to-FnMut adaptation after removing every write. V2 changes which resource argument is written instead; all V1 failures remain archived. Weighted groups are unchanged. Balanced DAG stress is added separately at zero weight.','variants':{'A':'main-existing-worklist','B':'140-no-consumer-elision','F':'136-compiled-parameter-facts','G':'144-summary-only-unused-liveness-elision'},'blocks':6,'group_weights':group_weights,'weights_basis':'predeclared target workload assumption; not empirical ZUI production telemetry','scenario_weights':{'cold':.10,'unchanged':.35,'private_edit':.40,'public_edit':.15},'primary':'weighted arithmetic expected request latency; ratio of weighted times, not mean of speedups','sensitivity':['equal per cell','large-only','consumer-only'],'order':'blockwise ABBA/BAAB for B and F versus A; reversed comparison order on odd blocks','limits':'unchanged production default; report ResourceLimit/timeout, never label as success or silently renormalize primary weights','cases':cases}
spec['input_hashes']={str(f.relative_to(out)):hashlib.sha256(f.read_bytes()).hexdigest() for f in out.rglob('*') if f.is_file()}
(out/'frozen.json').write_text(json.dumps(spec,indent=2)+'\n');print(json.dumps({'cases':len(cases),'sizes':[{k:c[k] for k in ['name','category','size','weight']} for c in cases]},indent=2))
