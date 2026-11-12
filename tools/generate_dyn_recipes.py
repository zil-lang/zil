import re
from pathlib import Path
root=str(Path(__file__).resolve().parents[1] / 'crates/zil-ir/src')+'/'
src=open(root+'dynamic_recipes.rs').read()
src=re.sub(r'//[^\n]*','',src)
def split(s):
 out=[];start=0;depth=0
 for i,c in enumerate(s):
  if c in '<({[':depth+=1
  elif c in '>)}]':depth-=1
  elif c==',' and depth==0:out.append(s[start:i].strip());start=i+1
 if s[start:].strip():out.append(s[start:].strip())
 return out
def block(s,start):
 d=1;i=start
 while d:
  if s[i]=='{':d+=1
  elif s[i]=='}':d-=1
  i+=1
 return s[start:i-1]
def fields(s):return [(x.split(':',1)[0].replace('pub ','').strip(),x.split(':',1)[1].strip()) for x in split(s)]
items={}
for m in re.finditer(r'pub (struct|enum) (\w+)\s*\{',src):
 kind,name=m.groups();body=block(src,m.end())
 if kind=='struct':items[name]=(kind,fields(body))
 else:
  variants=[]
  for value in split(body):
   if '{' in value:n,fs=value.split('{',1);variants.append((n.strip(),'struct',fields(fs.rsplit('}',1)[0])))
   elif '(' in value:n,fs=value.split('(',1);variants.append((n.strip(),'tuple',[(f'value_{i}',t) for i,t in enumerate(split(fs.rsplit(')',1)[0]))]))
   else:variants.append((value.strip(),'unit',[]))
  items[name]=(kind,variants)
# The declaration-scoped pattern tags retain existing variant order; all new
# constructors extend that order. Original-life arguments now carry scope.
patterns=open(root+'dynamic.rs').read()
patterns=re.sub(r'//[^\n]*','',patterns)
for name,order in {
 'DynSelfPattern':None,
 'DynPatternActual':['Type','Const','Life'],
 'DynConstPattern':['Value','Parameter','MethodParameter','Unary','Binary','Convert','AssociatedProjection','IntegerLiteral','FloatLiteral'],
 'DynTypePattern':['Concrete','Parameter','Nominal','Address','Tuple','Array','Slice','Managed','Desc','OwnedStore','ErrorUnion','SelfType','MethodParameter','AssociatedProjection'],
}.items():
 m=re.search(r'pub (struct|enum) '+name+r'\s*\{',patterns)
 kind=m.group(1);body=block(patterns,m.end())
 if kind=='struct':items[name]=(kind,fields(body));continue
 vs=[]
 for value in split(body):
  if '{' in value:n,fs=value.split('{',1);vs.append((n.strip(),'struct',fields(fs.rsplit('}',1)[0])))
  elif '(' in value:n,fs=value.split('(',1);vs.append((n.strip(),'tuple',[(f'value_{i}',t) for i,t in enumerate(split(fs.rsplit(')',1)[0]))]))
  else:vs.append((value.strip(),'unit',[]))
 items[name]=(kind,sorted(vs,key=lambda v:order.index(v[0])))
def snake(n):return re.sub(r'(?<!^)(?=[A-Z])','_',n).lower()
existing={
 'String':('text','encode_text'),
 'Type':('ty','encode_type'), 'StorageLifeShape':('storage_shape','encode_storage_shape'),
 'StorageBinder':('storage_binder','encode_storage_binder'), 'DynTypePattern':('dyn_type_pattern','encode_dyn_type_pattern'),
 'DynConstPattern':('dyn_const_pattern','encode_dyn_const_pattern'), 'DynSelfPattern':('dyn_self_pattern','encode_dyn_self_pattern'),
 'DynGenericActual':('dyn_generic_actual','encode_dyn_generic_actual'), 'StoragePosition':('storage_position','encode_storage_position'),
 'DynCallAccess':('dyn_call_access','encode_dyn_call_access'), 'DynResultSource':('dyn_result_source','encode_dyn_result_source'),
 'DynResumeBound':('dyn_resume_bound','encode_dyn_resume_bound'), 'DynThreadRequirements':('dyn_thread_requirements','encode_dyn_thread_requirements'),
 'PassingMode':('dyn_passing_mode','encode_dyn_passing_mode'), 'CallKind':('dyn_call_kind','encode_dyn_call_kind'),
 'LifeDeclarationIdentity':('declaration_identity','encode_declaration_identity'), 'LifeDeclarationId':('declaration_id','encode_declaration_id'),
 'DynTraitApplication':('dyn_trait_application','encode_dyn_trait_application'), 'DynMember':('dyn_member','encode_dyn_member'),
 'Constant':('constant','encode_constant')}
# Reusable value binding codec belongs to the new recipe transport too.
items['StorageSourceBinding']=('struct',[('position','StoragePosition'),('sources','Vec<StoragePosition>')])
def gen(t,e,mode,level=0):
 t=t.strip();i='    '*level
 if t.startswith('Box<'):
  inner=t[4:-1];return gen(inner,e,mode,level) if mode!='r' else f'Box::new({gen(inner,e,mode,level)})'
 if t.startswith('Vec<'):
  inner=t[4:-1]
  if mode=='w':return f'self.count(({e}).len(), self.limits.max_nodes)?;\nfor item in {e} {{\n{gen(inner,"item",mode,level)}\n}}'
  if mode=='f':return f'encode_len(b, ({e}).len());\nfor item in {e} {{\n{gen(inner,"item",mode,level)}\n}}'
  return f'{{ let count = self.count(self.limits.max_nodes, 1)?; let mut rows = Vec::with_capacity(count); for _ in 0..count {{ rows.push({gen(inner,"",mode,level)}); }} rows }}'
 if t.startswith('Option<'):
  inner=t[7:-1]
  if mode=='w':return f'self.boolean(({e}).is_some())?;\nif let Some(item) = {e} {{\n{gen(inner,"item",mode,level)}\n}}'
  if mode=='f':return f'b.push(u8::from(({e}).is_some()));\nif let Some(item) = {e} {{\n{gen(inner,"item",mode,level)}\n}}'
  return f'if self.boolean()? {{ Some({gen(inner,"",mode,level)}) }} else {{ None }}'
 if t=='u32':return f'self.u32(*{e})?;' if mode=='w' else f'encode_u32(b, *{e});' if mode=='f' else 'self.u32()?'
 if t=='bool':return f'self.boolean(*{e})?;' if mode=='w' else f'b.push(u8::from(*{e}));' if mode=='f' else 'self.boolean()?'
 if t in ('ModuleId','NodeId','FunctionId'):
  return f'self.u32(({e}).0)?;' if mode=='w' else f'encode_u32(b, ({e}).0);' if mode=='f' else f'{t}(self.u32()?)'
 if t in ('UnaryOp','BinaryOp'):
  method='unary' if t=='UnaryOp' else 'binary'
  return f'self.{method}(*{e})?;' if mode=='w' else f'b.push(*{e} as u8);' if mode=='f' else f'self.{method}()?'
 if t=='CallableKind':
  if mode=='r':return 'self.callable_kind()?'
  v=f'match {e} {{ CallableKind::FunctionPointer => 0, CallableKind::Fn => 1, CallableKind::FnMut => 2, CallableKind::FnOnce => 3 }}'
  return f'self.u8({v})?;' if mode=='w' else f'b.push({v});'
 if t in items:
  method=snake(t)
  return f'self.{method}({e}, depth + 1)?;' if mode=='w' else f'encode_{method}(b, {e});' if mode=='f' else f'self.{method}(depth + 1)?'
 method,fp=existing[t];depth=t in ('Type','DynTypePattern','DynConstPattern','DynSelfPattern')
 return f'self.{method}({e}{", depth + 1" if depth else ""})?;' if mode=='w' else f'{fp}(b, {e});' if mode=='f' else f'self.{method}({"depth + 1" if depth else ""})?'
def body(name,kind,values,mode):
 if kind=='struct':
  if mode=='r':return 'Ok('+name+' {\n'+''.join(f'{n}: {gen(t,"",mode)},\n' for n,t in values)+'})'
  return '\n'.join(gen(t,'&value.'+n,mode) for n,t in values)
 arms=[]
 for tag,(n,k,fs) in enumerate(values):
  prefix=f'{name}::{n}'
  pattern=prefix+(' { '+', '.join(n for n,t in fs)+' }' if k=='struct' else '('+', '.join(n for n,t in fs)+')' if k=='tuple' else '')
  if mode=='r':
   expr=prefix+(' { '+', '.join(n+': '+gen(t,'',mode) for n,t in fs)+' }' if k=='struct' else '('+', '.join(gen(t,'',mode) for n,t in fs)+')' if k=='tuple' else '')
   arms.append(f'{tag} => {expr},')
  else:arms.append(pattern+' => {\n'+(f'self.u8({tag})?;' if mode=='w' else f'b.push({tag});')+'\n'+'\n'.join(gen(t,n,mode) for n,t in fs)+'\n},')
 return ('Ok(match self.u8()? {\n'+'\n'.join(arms)+'\n_ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),\n})') if mode=='r' else 'match value {\n'+'\n'.join(arms)+'\n}'
w='//! Bounded transport for declaration-scoped dynamic recipes.\nuse super::*;\n'
for mode in ['w','r']:
 w+='impl '+('Writer' if mode=='w' else 'Reader')+"<'_> {\n"
 w+='pub(super) fn dyn_recipe_node(&mut self, depth: usize) -> WireResult<()> { if depth >= self.limits.max_call_depth.min(128) {return Err(quota("dynamic recipe depth exceeded"));} self.charge(1) }\n' if mode=='w' else ''
 # Reader uses a distinct name from Writer's inherent impl; same method names are fine.
 if mode=='r':w+='pub(super) fn dyn_recipe_node(&mut self, depth: usize) -> WireResult<()> { if depth >= self.limits.max_call_depth.min(128) {return Err(quota("dynamic recipe depth exceeded"));} self.charge(1) }\n'
 for name,(kind,values) in items.items():
  params=f'&mut self, value: &{name}, depth: usize' if mode=='w' else '&mut self, depth: usize'
  w+=f'pub(super) fn {snake(name)}({params}) -> WireResult<{"()" if mode=="w" else name}> {{\nself.dyn_recipe_node(depth)?;\n'+body(name,kind,values,mode)+ ('\nOk(())' if mode=='w' else '')+'\n}\n'
 w+='}\n'
open(root+'dynamic_recipes_wire.rs','w').write(w)
f='//! Complete raw dynamic recipe identity. This does not certify recipes.\nuse super::*;\n'
for name,(kind,values) in items.items():f+=f'pub(super) fn encode_{snake(name)}(b: &mut impl FingerprintSink, value: &{name}) {{\n'+body(name,kind,values,'f')+'\n}\n'
open(root+'dynamic_recipes_fingerprint.rs','w').write(f)
print('generated',len(items),'types')
