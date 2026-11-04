from pathlib import Path
import re

root = Path(__file__).parent / 'crates/zil-ir/src'
structs = {
 'DynInterfaceId':[('module','ModuleId'),('index','u32')],
 'DynVtableId':[('module','ModuleId'),('index','u32')],
 'DynDeclaration':[('id','LifeDeclarationId'),('identity','LifeDeclarationIdentity'),('kind','DynDeclarationKind'),('parameters','Vec<LifeParameterKind>'),('members','Vec<DynMemberDeclaration>')],
 'DynMemberDeclaration':[('name','String'),('kind','DynMemberKind')],
 'DynTraitApplication':[('declaration','LifeDeclarationId'),('actuals','Vec<DynGenericActual>')],
 'DynMember':[('application','DynTraitApplication'),('ordinal','u32')],
 'DynAssociatedType':[('member','DynMember'),('ty','Type')],
 'DynAssociatedConstant':[('member','DynMember'),('ty','Type'),('value','Constant')],
 'DynReceiver':[('mutable','bool'),('mode','PassingMode')],
 'DynSlot':[('member','DynMember'),('actuals','Vec<DynGenericActual>'),('receiver','DynReceiver'),('parameters','Vec<Type>'),('parameter_modes','Vec<PassingMode>'),('result','Type'),('result_mode','PassingMode'),('call_kind','CallKind'),('invocation_life','Option<FunctionLifeContract>'),('storage_life','Option<StorageLifeContract>'),('contract','DynContract')],
 'DynInterface':[('id','DynInterfaceId'),('application','DynTraitApplication'),('inherited','Vec<DynTraitApplication>'),('associated_types','Vec<DynAssociatedType>'),('associated_constants','Vec<DynAssociatedConstant>'),('retained_life','RetainedLifeShape'),('slots','Vec<DynSlot>')],
 'DynEntry':[('slot','u32'),('function','FunctionId')],
 'DynVtable':[('id','DynVtableId'),('interface','DynInterfaceId'),('self_type','Type'),('implementation','LifeDeclarationId'),('actuals','Vec<DynGenericActual>'),('ports','Vec<RetainedPortBinding>'),('entries','Vec<DynEntry>'),('cleanup','Option<DynEntry>')],
 'DynCallAccess':[('reads','Vec<StoragePosition>'),('writes','Vec<StoragePosition>')],
 'DynCleanupContract':[('environment','bool'),('positions','Vec<StoragePosition>'),('access','DynCallAccess'),('effects','Vec<DynEffect>')],
 'DynThreadRequirements':[('transfer','bool'),('share','bool')],
 'DynContract':[('requires','Option<Vec<DynRequirement>>'),('guarantees','Option<Vec<DynGuarantee>>'),('hold','Option<Vec<StoragePosition>>'),('call','Option<DynCallAccess>'),('result','Option<DynResultSource>'),('cleanup','Option<DynCleanupContract>'),('effects','Option<Vec<DynEffect>>'),('resume','Option<DynResumeBound>'),('threads','Option<DynThreadRequirements>')],
 # These shared structures are encoded by the final342 companion.
}
enums={
 'DynDeclarationKind':[('Trait',[]),('Impl',[])],
 'DynMemberKind':[('Type',[]),('Constant',[('0','Type')]),('Method',[('0','Vec<LifeParameterKind>')])],
 'DynGenericActual':[('Type',[('0','Type')]),('Const',[('0','Constant')]),('Life',[('0','StorageBinder')])],
 'DynRequirement':[('Read',[('0','StoragePosition')]),('Write',[('0','StoragePosition')]),('Manage',[('0','StoragePosition')]),('Apart',[('0','StoragePosition'),('1','StoragePosition')]),('State',[('position','StoragePosition'),('ty','Type'),('variant','u32')])],
 'DynGuarantee':[('ManageResult',[]),('State',[('position','StoragePosition'),('ty','Type'),('variant','u32')])],
 'DynEffect':[(n,[]) for n in ['Alloc','Io','Block','Suspend','Spawn','Ffi','Reenter','Panic']]+[('Call',[('0','StoragePosition')]),('Cleanup',[('0','Type')])],
 'DynResultSource':[('Value',[]),('Create',[]),('Give',[]),('Borrow',[('sources','Vec<StoragePosition>'),('mutable','bool')])],
 'DynResumeBound':[('Local',[]),('Transfer',[])],
 'PassingMode':[(n,[]) for n in ['Copy','Move','Describe','Create','Give','Borrow','BorrowMut','Infer']],
 'CallKind':[(n,[]) for n in ['Direct','Suspend']],
}
common={ 'LifeDeclarationId':'declaration_id', 'LifeDeclarationIdentity':'declaration_identity', 'LifeParameterKind':'parameter_kind', 'StoragePosition':'storage_position', 'RetainedLifeShape':'retained_shape', 'StorageBinder':'storage_binder', 'RetainedPortBinding':'retained_port_binding', 'FunctionLifeContract':'function_life' }

def snake(n): return re.sub(r'(?<!^)(?=[A-Z])','_',n).lower()
def method(n): return common.get(n,'dyn_'+snake(n.removeprefix('Dyn')))
def generic(t,n): return t.startswith(n+'<') and t.endswith('>')
def write(t,v,fp=False,indent='        '):
 s='b' if fp else 'self'
 v='('+v+')'
 out=[]
 def line(x): out.append(indent+x)
 if generic(t,'Vec'):
  inner=t[4:-1]
  line(f'encode_len(b, {v}.len());' if fp else f'self.count({v}.len(), self.limits.max_nodes)?;')
  line(f'for value in {v} {{')
  out.append(write(inner,'value',fp,indent+'    '));line('}')
 elif generic(t,'Option'):
  inner=t[7:-1]
  line(f'b.push(u8::from({v}.is_some()));' if fp else f'self.boolean({v}.is_some())?;')
  line(f'if let Some(value) = {v} {{');out.append(write(inner,'value',fp,indent+'    '));line('}')
 elif t in ('ModuleId','FunctionId'):
  line(f'encode_u32(b, {v}.0);' if fp else f'self.u32({v}.0)?;')
 elif t=='u32': line(f'encode_u32(b, *{v});' if fp else f'self.u32(*{v})?;')
 elif t=='bool': line(f'b.push(u8::from(*{v}));' if fp else f'self.boolean(*{v})?;')
 elif t=='String': line(f'encode_text(b, {v});' if fp else f'self.text({v})?;')
 elif t=='Type': line(f'encode_type(b, {v});' if fp else f'self.ty({v}, 0)?;')
 elif t=='Constant':line(f'encode_constant(b, {v});' if fp else f'self.constant({v})?;')
 elif t=='StorageLifeContract':
  # existing helper takes Option: caller handles Some itself, use one shared
  # required-contract helper in342 rather than clone/wrap the entire relation.
  line(f'encode_required_storage_contract(b, {v});' if fp else f'self.required_storage_contract({v})?;')
 else: line(f'encode_{method(t)}(b, {v});' if fp else f'self.{method(t)}({v})?;')
 return '\n'.join(out)

counter=0
def read(t,indent='        '):
 global counter
 if generic(t,'Vec'):
  counter+=1; k=counter; inner=t[4:-1]
  return '{\n'+indent+f'    let count = self.count(self.limits.max_nodes, 1)?;\n'+indent+f'    let mut values_{k} = Vec::with_capacity(count);\n'+indent+f'    for _ in 0..count {{ values_{k}.push({read(inner,indent+"    ")}); }}\n'+indent+f'    values_{k}\n'+indent+'}'
 if generic(t,'Option'):return f'if self.boolean()? {{ Some({read(t[7:-1],indent)}) }} else {{ None }}'
 if t in ('ModuleId','FunctionId'):return t+'(self.u32()?)'
 if t in ('u32','bool','String','Constant'):return 'self.'+{'u32':'u32','bool':'boolean','String':'text','Constant':'constant'}[t]+'()?'
 if t=='Type':return 'self.ty(0)?'
 if t=='StorageLifeContract':return 'self.required_storage_contract()?'
 return 'self.'+method(t)+'()?'

writer=['//! Bounded raw dynamic table transport; shared life codecs live in wire.rs.','use super::*;','\nimpl Writer<\'_> {']
reader=['\nimpl Reader<\'_> {']
fp=['//! Complete raw module metadata fingerprints, not cross-request Type identity.','use super::*;']
for name,fields in structs.items():
 m=method(name)
 writer.append(f'    pub(super) fn {m}(&mut self, value: &{name}) -> WireResult<()> {{')
 fp.append(f'pub(super) fn encode_{m}(b: &mut impl FingerprintSink, value: &{name}) {{')
 reader.append(f'    pub(super) fn {m}(&mut self) -> WireResult<{name}> {{\n        Ok({name} {{')
 for field,t in fields:
  writer.append(write(t,'&value.'+field))
  fp.append(write(t,'&value.'+field,True,'    '))
  reader.append(f'            {field}: {read(t,"            ")},')
 writer.append('        Ok(())\n    }');fp.append('}');reader.append('        })\n    }')
for name,variants in enums.items():
 m=method(name)
 writer.append(f'    pub(super) fn {m}(&mut self, value: &{name}) -> WireResult<()> {{\n        match value {{')
 fp.append(f'pub(super) fn encode_{m}(b: &mut impl FingerprintSink, value: &{name}) {{\n    match value {{')
 reader.append(f'    pub(super) fn {m}(&mut self) -> WireResult<{name}> {{\n        Ok(match self.u8()? {{')
 for tag,(variant,fields) in enumerate(variants):
  tuple_=bool(fields) and fields[0][0]=='0'
  names=[f'value_{i}' if f.isdigit() else f for i,(f,t) in enumerate(fields)]
  pattern=name+'::'+variant+('('+', '.join(names)+')' if tuple_ else (' { '+', '.join(names)+' }' if fields else ''))
  writer.append(f'            {pattern} => {{\n                self.u8({tag})?;')
  fp.append(f'        {pattern} => {{\n            b.push({tag});')
  exprs=[]
  for (field,t),local in zip(fields,names):
   writer.append(write(t,local,False,'                '));fp.append(write(t,local,True,'            '));exprs.append(('' if tuple_ else field+': ')+read(t,'            '))
  writer.append('            }');fp.append('        }')
  expr=name+'::'+variant+('('+', '.join(exprs)+')' if tuple_ else (' { '+', '.join(exprs)+' }' if fields else ''))
  reader.append(f'            {tag} => {expr},')
 writer.append('        }\n        Ok(())\n    }');fp.append('    }\n}');reader.append(f'            _ => return Err(malformed("E-WIRE-TAG", "unknown {name} tag")),\n        }})\n    }}')
writer.append('}');reader.append('}')
(root/'dynamic_wire.rs').write_text('\n'.join(writer+reader)+'\n')
(root/'dynamic_fingerprint.rs').write_text('\n'.join(fp)+'\n')
