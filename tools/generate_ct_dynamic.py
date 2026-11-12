"""Generate the finite CT dynamic metadata schema, never verification logic.

Runtime Type payloads become references to a full finite metadata Type DAG.
The public input grammar is parsed, not reduced to the subset used by an ABI.
"""
import re
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
IR=ROOT/'crates/zil-ir/src'
# This parser has no generation side effects on the runtime codec.
def split(s):
 out=[];start=0;depth=0
 for i,c in enumerate(s):
  if c in '<({[':depth+=1
  elif c in '>)}]':depth-=1
  elif c==',' and depth==0:out.append(s[start:i].strip());start=i+1
 if s[start:].strip():out.append(s[start:].strip())
 return out
def fields(s):return [(x.split(':',1)[0].replace('pub ','').strip(),x.split(':',1)[1].strip()) for x in split(s)]
def declarations(path, only=None):
 src=re.sub(r'//[^\n]*','',path.read_text());out={}
 for m in re.finditer(r'pub (struct|enum) (\w+)\s*\{',src):
  kind,name=m.groups()
  if only is not None and name not in only:continue
  i=m.end();start=i;depth=1
  while depth:
   if src[i]=='{':depth+=1
   elif src[i]=='}':depth-=1
   i+=1
  body=src[start:i-1]
  if kind=='struct':out[name]=(kind,fields(body));continue
  rows=[]
  for v in split(body):
   if '{' in v:n,fs=v.split('{',1);rows.append((n.strip(),'struct',fields(fs.rsplit('}',1)[0])))
   elif '(' in v:n,fs=v.split('(',1);rows.append((n.strip(),'tuple',[(f'value_{i}',t) for i,t in enumerate(split(fs.rsplit(')',1)[0]))]))
   else:rows.append((v.split('=',1)[0].strip(),'unit',[]))
  out[name]=(kind,rows)
 return out
items=declarations(IR/'dynamic.rs')
items.update(declarations(IR/'dynamic_recipes.rs'))
items.update(declarations(IR/'storage_sources.rs'))
items.update(declarations(IR/'lib.rs',{'StorageSourceBinding','RetainedBorrowPort','RetainedLifeShape','LifeParameterKind','LifeNominalKind','LifeNominalDeclaration','LifeNominalActual','LifeNominalInstance','Type'}))
names={n:'Ct'+n for n in items}
names['Type']='CtDynamicTypeNode'
def mapped(t):
 return re.sub(r'\b\w+\b',lambda m:'CtDynamicTypeId' if m[0]=='Type' else names.get(m[0],m[0]),t)
def shape(n,kind,rows):
 if kind=='struct':return 'pub struct '+n+' {\n'+''.join('    pub '+f+': '+mapped(t)+',\n' for f,t in rows)+'}\n'
 return 'pub enum '+n+' {\n'+''.join('    '+v+(' { '+', '.join(f+': '+mapped(t) for f,t in fs)+' }' if k=='struct' else '('+', '.join(mapped(t) for f,t in fs)+')' if k=='tuple' else '')+',\n' for v,k,fs in rows)+'}\n'
header='''//! Complete untrusted CT inventory for dynamic metadata and storage sources.
//! Type payloads are edges in the finite metadata Type DAG in this catalogue.
//! Catalogue membership grants no visibility, trait, provider or borrow right.
use crate::*;
use super::{CtStageIdentity, CtStageFacetRequest};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CtDynamicTypeId(pub u32);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CtDynamicCatalogue {
    pub stage: Option<CtStageIdentity>,
    pub facet_requests: Vec<CtStageFacetRequest>,
    pub types: Vec<CtDynamicTypeNode>,
    pub life_declarations: Vec<CtLifeNominalDeclaration>,
    pub life_instances: Vec<CtLifeNominalInstance>,
    pub declarations: Vec<CtDynDeclaration>,
    pub interfaces: Vec<CtDynInterface>,
    pub vtables: Vec<CtCoreDynVtable>,
    pub function_instances: Vec<CtDynFunctionInstanceRecord>,
    pub function_sources: Vec<CtFunctionSourceBindings>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynFunctionInstanceRecord {
    pub module: ModuleId,
    pub source: NodeId,
    pub instance: CtDynFunctionInstance,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtFunctionSourceBindings {
    pub module: ModuleId,
    pub source: NodeId,
    pub retained_return_sources: Option<Vec<CtStorageSourceBinding>>,
}
'''
out=header
for name,(kind,rows) in items.items():out+='\n#[derive(Clone, Debug, PartialEq, Eq)]\n'+shape(names[name],kind,rows)
(IR/'ct_dynamic.rs').write_text(out)
# The recursive CT grammar is bounded by Encoder/Decoder::descend BEFORE any
# child visit. Every vector allocation is governed by their shared budget.
wire='//! Generated bounded transport for the complete CT dynamic catalogue.\nuse super::*;\nuse crate::*;\n'
def codec(name,kind,rows):
 out='impl Wire for '+name+' {\nfn encode(&self,e:&mut Encoder)->Result<()> { if e.depth>=128 {return Err(WireError::Limit);} e.descend(|e| {\n'
 if kind=='struct':out+=''.join('self.'+n+'.encode(e)?;\n' for n,t in rows)
 else:
  out+='match self {\n'
  for tag,(v,k,fs) in enumerate(rows):
   pat='Self::'+v+(' { '+', '.join(n for n,t in fs)+' }' if k=='struct' else '('+', '.join(n for n,t in fs)+')' if k=='tuple' else '')
   out+=pat+' => { '+str(tag)+'u8.encode(e)?; '+''.join(n+'.encode(e)?; ' for n,t in fs)+'},\n'
  out+='}\n'
 out+='Ok(()) }) }\nfn decode(d:&mut Decoder<\'_>)->Result<Self>{if d.depth>=128 {return Err(WireError::Limit);} d.descend(|d| {\n'
 if kind=='struct':out+='Ok(Self { '+', '.join(n+': <'+mapped(t)+' as Wire>::decode(d)?' for n,t in rows)+' })'
 else:
  out+='Ok(match u8::decode(d)? {\n'
  for tag,(v,k,fs) in enumerate(rows):
   out+=str(tag)+' => Self::'+v+(' { '+', '.join(n+': <'+mapped(t)+' as Wire>::decode(d)?' for n,t in fs)+' }' if k=='struct' else '('+', '.join('<'+mapped(t)+' as Wire>::decode(d)?' for n,t in fs)+')' if k=='tuple' else '')+',\n'
  out+='_ => return Err(WireError::UnknownTag),\n})'
 return out+'\n})}\n}\n'
for n,(kind,rows) in items.items():wire+=codec(names[n],kind,rows)
# Existing non-Type metadata has no relocated Type values; preserve it whole.
external={}
for path,wanted in [(IR/'lib.rs',{'LifeDeclarationIdentity','PassingMode','CallKind','CallableKind'} )]:external.update(declarations(path,wanted))
for n,(kind,rows) in external.items():wire+=codec(n,kind,rows)
extra={
'CtDynamicCatalogue':[('stage','Option<CtStageIdentity>'),('facet_requests','Vec<CtStageFacetRequest>'),('types','Vec<CtDynamicTypeNode>'),('life_declarations','Vec<CtLifeNominalDeclaration>'),('life_instances','Vec<CtLifeNominalInstance>'),('declarations','Vec<CtDynDeclaration>'),('interfaces','Vec<CtDynInterface>'),('vtables','Vec<CtCoreDynVtable>'),('function_instances','Vec<CtDynFunctionInstanceRecord>'),('function_sources','Vec<CtFunctionSourceBindings>')],
'CtDynFunctionInstanceRecord':[('module','ModuleId'),('source','NodeId'),('instance','CtDynFunctionInstance')],
'CtFunctionSourceBindings':[('module','ModuleId'),('source','NodeId'),('retained_return_sources','Option<Vec<CtStorageSourceBinding>>')],
}
stage_items=declarations(IR/'ct_stage.rs')
for n,(kind,rows) in stage_items.items():wire+=codec(n,kind,rows)
for n,rows in extra.items():wire+=codec(n,'struct',rows)
wire+='impl Wire for CtDynamicTypeId { fn encode(&self,e:&mut Encoder)->Result<()>{self.0.encode(e)} fn decode(d:&mut Decoder<\'_>)->Result<Self>{Ok(Self(u32::decode(d)?))} }\n'
wire+='impl Wire for ClosureId { fn encode(&self,e:&mut Encoder)->Result<()>{self.0.encode(e)} fn decode(d:&mut Decoder<\'_>)->Result<Self>{Ok(Self(u32::decode(d)?))} }\n'
wire+='impl Wire for FunctionId { fn encode(&self,e:&mut Encoder)->Result<()>{self.0.encode(e)} fn decode(d:&mut Decoder<\'_>)->Result<Self>{Ok(Self(u32::decode(d)?))} }\n'
(IR/'ct_wire_dynamic.rs').write_text(wire)
print('generated',len(items),'CT dynamic records')
# A structural visitor is shared data traversal, never shared semantic proof.
# Each consumer supplies its own meters and type-reference authentication.
visit_items=dict(items)
visit_items.update(external)
visit_items.update(declarations(IR/'lib.rs',{'StorageLifeShape','StorageLifeSlot','StorageRetainedBinding','NominalLifeArgument','StorageLifeContract','StorageLifeBinding','SignatureLifeClass','SignatureLifeClassBinding','StoragePosition','LifeDeclarationId'}))
visit_items={names.get(n,n):(kind,[(f,mapped(t)) for f,t in rows] if kind=='struct' else [(v,k,[(f,mapped(t)) for f,t in fs]) for v,k,fs in rows]) for n,(kind,rows) in visit_items.items()}
visit_items.update({n:('struct',rows) for n,rows in extra.items()})
visit_items.update(stage_items)
# Traverse complete Type handles carried by facet requests; the visitor still
# supplies no semantic readiness or index authentication.
ct_items=declarations(IR/'ct_artifact.rs')
pending=['CtTypeRef']
while pending:
 n=pending.pop()
 if n in visit_items:continue
 kind,rows=ct_items[n]
 visit_items[n]=(kind,rows)
 fs=rows if kind=='struct' else [field for _,_,fields in rows for field in fields]
 for _,t in fs:
  for dep in re.findall(r'\b\w+\b',t):
   if dep in ct_items and dep not in visit_items:pending.append(dep)

visitor='''//! Structural CT metadata traversal; callbacks carry the caller's own budget.
use super::*;
use crate::*;
pub trait CtDynamicVisitor {
    type Error;
    fn node(&mut self, depth: usize) -> Result<(),Self::Error>;
    fn rows(&mut self, count: usize, element_bytes: usize, depth: usize) -> Result<(),Self::Error>;
    fn type_node(&mut self,index:usize,node:&CtDynamicTypeNode,depth:usize)->Result<(),Self::Error>;
    fn type_reference(&mut self, id: CtDynamicTypeId, depth: usize) -> Result<(),Self::Error>;
    fn type_handle(&mut self, value: &CtTypeRef, depth: usize) -> Result<(),Self::Error>;
    fn constant(&mut self, value: &Constant, depth: usize) -> Result<(),Self::Error>;
    fn text(&mut self, value: &str, depth: usize) -> Result<(),Self::Error>;
}
pub trait CtDynamicMetadata {
    fn visit_ct_dynamic<V:CtDynamicVisitor>(&self, visitor:&mut V, depth:usize)->Result<(),V::Error>;
}
'''
copy_leaves={'Fingerprint','CtTypeNodeId','CtAmbientScopeId','CtModuleId','CtDeclarationId','CtEntityId','u8','u16','u32','u64','i8','i16','i32','i64','bool','ModuleId','NodeId','FunctionId','ClosureId','BinaryOp','UnaryOp','StorageBinder','StoredBorrowMode','ContractProjection','PlaceProjection','LifeTypeProjection','StorageBase','PassingMode','CallKind','CallableKind','SyncCellKind'}
def receiver(e):return e[1:] if e.startswith('&') else e
def walk(t,e):
 t=t.removeprefix('crate::')
 if t=='CtTypeRef':return f'visitor.type_handle({e},depth+1)?;\n'
 if t.startswith('Box<'):return f'visitor.rows(1,std::mem::size_of::<{t[4:-1]}>(),depth+1)?; '+walk(t[4:-1],'('+receiver(e)+').as_ref()')
 if t.startswith('('):
  return ''.join(walk(v,f'&({e}).{i}') for i,v in enumerate(split(t[1:-1])))
 if t=='Vec<CtDynamicTypeNode>':return f'visitor.rows(({receiver(e)}).len(),std::mem::size_of::<CtDynamicTypeNode>(),depth+1)?; for (index,node) in ({receiver(e)}).iter().enumerate() {{ visitor.type_node(index,node,depth+1)?; }}\n'
 if t.startswith('Vec<'):
  inner=t[4:-1];return f'visitor.rows(({receiver(e)}).len(),std::mem::size_of::<{inner}>(),depth+1)?;\nfor item in {e} {{ '+walk(inner,'item')+' }\n'
 if t.startswith('Option<'):return f'visitor.node(depth+1)?; if let Some(item)={e} {{ '+walk(t[7:-1],'item')+' }\n'
 if t in visit_items:return f'({receiver(e)}).visit_ct_dynamic(visitor,depth+1)?;\n'
 if t=='CtDynamicTypeId':return f'visitor.type_reference({receiver(e) if e.startswith("&") else "*"+e},depth+1)?;\n'
 if t=='String':return f'visitor.text({e},depth+1)?;\n'
 if t=='Constant':return f'visitor.constant({e},depth+1)?;\n'
 if t not in copy_leaves:raise ValueError('unvisited CT metadata leaf: '+t)
 return f'let _={e}; visitor.node(depth+1)?;\n'
for n,(kind,rows) in visit_items.items():
 visitor+=f'impl CtDynamicMetadata for {n} {{ fn visit_ct_dynamic<V:CtDynamicVisitor>(&self,visitor:&mut V,depth:usize)->Result<(),V::Error>{{ visitor.node(depth)?;\n'
 if kind=='struct':visitor+=''.join(walk(t,'&self.'+f) for f,t in rows)
 else:
  visitor+='match self {\n'
  for v,k,fs in rows:
   pat='Self::'+v+(' { '+', '.join(f+': field_'+f for f,t in fs)+' }' if k=='struct' else '('+', '.join('field_'+f for f,t in fs)+')' if k=='tuple' else '')
   visitor+=pat+' => { '+''.join(walk(t,'field_'+f) for f,t in fs)+' },\n'
  visitor+='}\n'
 visitor+='Ok(()) } }\n'
(IR/'ct_dynamic_visit.rs').write_text(visitor)
# Structural reification uses caller-owned budgets, with no proof callbacks.
# All allocation-bearing constructors are preceded by rows/text/constant hooks.
copy_items=declarations(IR/'lib.rs',{'LifeDeclarationIdentity','StorageLifeShape','StorageLifeSlot','StorageRetainedBinding','NominalLifeArgument','StorageLifeContract','StorageLifeBinding','SignatureLifeClass','SignatureLifeClassBinding','StoragePosition','LifeDeclarationId'})
converters=dict(items);converters.update(copy_items)
def method(n):return 'reify_'+re.sub(r'(?<!^)(?=[A-Z])','_',n).lower()
convert='''//! Structural reification of frozen runtime metadata into raw CT data.
//! Budget callbacks supply no semantic authority. Every type is a real DAG node.
use super::*;
use crate::*;
pub trait CtDynamicReifier {
    type Error;
    fn node(&mut self,depth:usize)->Result<(),Self::Error>;
    fn rows(&mut self,count:usize,element_bytes:usize,depth:usize)->Result<(),Self::Error>;
    fn text(&mut self,text:&str,depth:usize)->Result<String,Self::Error>;
    fn constant(&mut self,value:&Constant,depth:usize)->Result<Constant,Self::Error>;
    fn store_type(&mut self,node:CtDynamicTypeNode,depth:usize)->Result<CtDynamicTypeId,Self::Error>;
}
'''
def expr(t,e):
 if t.startswith('Box<'):
  inner=t[4:-1];return '{ reifier.rows(1,std::mem::size_of::<'+mapped(inner)+'>(),depth+1)?; Box::new('+expr(inner,e)+') }'
 if t.startswith('Vec<'):
  inner=t[4:-1];return '{ reifier.rows(('+receiver(e)+').len(),std::mem::size_of::<'+mapped(inner)+'>(),depth+1)?; let mut output=Vec::with_capacity(('+receiver(e)+').len()); for item in '+e+' { output.push('+expr(inner,'item')+'); } output }'
 if t.startswith('Option<'):return 'match '+e+' { Some(item)=>Some('+expr(t[7:-1],'item')+'), None=>None }'
 if t.startswith('('):return '('+', '.join(expr(v,f'&({e}).{i}') for i,v in enumerate(split(t[1:-1])))+')'
 if t=='Type':return 'reify_type_value('+e+',reifier,depth+1)?'
 if t=='String':return 'reifier.text('+e+',depth+1)?'
 if t=='Constant':return 'reifier.constant('+e+',depth+1)?'
 if t in converters:return method(t)+'('+e+',reifier,depth+1)?'
 if t not in copy_leaves:raise ValueError('unreified CT metadata leaf: '+t)
 return '{ reifier.node(depth+1)?; '+(e[1:] if e.startswith('&') else '*('+e+')')+' }'
for name,(kind,rows) in converters.items():
 result=names.get(name,name)
 convert+=f'pub fn {method(name)}<R:CtDynamicReifier>(value:&{name},reifier:&mut R,depth:usize)->Result<{result},R::Error>{{ reifier.node(depth)?;\n'
 if kind=='struct':convert+='Ok('+result+' { '+', '.join(f+': '+expr(t,'&value.'+f) for f,t in rows)+' })'
 else:
  convert+='Ok(match value {\n'
  for v,k,fs in rows:
   pat=name+'::'+v+(' { '+', '.join(f+': field_'+f for f,t in fs)+' }' if k=='struct' else '('+', '.join('field_'+f for f,t in fs)+')' if k=='tuple' else '')
   val=result+'::'+v+(' { '+', '.join(f+': '+expr(t,'field_'+f) for f,t in fs)+' }' if k=='struct' else '('+', '.join(expr(t,'field_'+f) for f,t in fs)+')' if k=='tuple' else '')
   convert+=pat+' => '+val+',\n'
  convert+='})'
 convert+='\n}\n'
convert+='''pub fn reify_type_value<R:CtDynamicReifier>(value:&Type,reifier:&mut R,depth:usize)->Result<CtDynamicTypeId,R::Error>{
 let node=reify_type(value,reifier,depth)?;
 reifier.store_type(node,depth)
}
'''
(IR/'ct_dynamic_reify.rs').write_text(convert)

with (IR/'ct_wire_dynamic.rs').open('a') as file:
 file.write("""
impl<A:Wire,B:Wire> Wire for (A,B) {
fn encode(&self,e:&mut Encoder)->Result<()>{e.descend(|e|{self.0.encode(e)?;self.1.encode(e)})}
fn decode(d:&mut Decoder<'_>)->Result<Self>{d.descend(|d|Ok((A::decode(d)?,B::decode(d)?)))}
}
impl Wire for SyncCellKind {
fn encode(&self,e:&mut Encoder)->Result<()>{(*self as u8).encode(e)}
fn decode(d:&mut Decoder<'_>)->Result<Self>{match u8::decode(d)?{0=>Ok(Self::MutexI32),1=>Ok(Self::AtomicI32),2=>Ok(Self::AtomicI64),3=>Ok(Self::AtomicU64),4=>Ok(Self::AtomicUsize),_=>Err(WireError::UnknownTag)}}
}
""")
