from pathlib import Path
import shutil,subprocess,difflib,hashlib,json
src=Path('/workspace/zil-342');out=Path('/tmp/342-public39'); prior=Path('/tmp/342-retained-codec')
for crate in ['zil-ir','zil-base']:
 shutil.copytree(prior/'crates'/crate/'src',out/'crates'/crate/'src',dirs_exist_ok=True)
subprocess.run(['patch','-p1','-i','/workspace/.zil-env/terminal-20261104/shared-retained-life/retained-life-inventory.patch'],cwd=out,check=True)
lib=out/'crates/zil-ir/src/lib.rs';s=lib.read_text()
s=s.replace('ZIL-TYPED-MODULE-V38','ZIL-TYPED-MODULE-V39').replace('ZIL-TYPE-IDENTITY-V35','ZIL-TYPE-IDENTITY-V39')
a=s.index('fn encode_storage_contract(');b=s.index('fn encode_type(',a)
s=s[:a]+(out/'fingerprint.rs').read_text()+'\n'+s[b:]
s=s.replace('    encode_len(bytes, module.functions.len());','    encode_life_inventory(bytes, &module.life_declarations, &module.life_instances);\n    encode_len(bytes, module.functions.len());',1)
# New callers share the complete relation encoder, not a cloned Option carrier.
a=s.index('            encode_u32(b, life.binder_count);',s.index('        Type::BorrowedFunctionPointer {',s.index('fn encode_type(')));b=s.index('            encode_len(b, parameters.len());',a)
s=s[:a]+'            encode_function_life(b, life);\n'+s[b:]
s=s.replace('fn encode_retained_shape(', 'fn encode_retained_shape(',1)
s=s.replace('fn encode_life_inventory(','''fn encode_retained_port_binding(b:&mut impl FingerprintSink,binding:&RetainedPortBinding){encode_u32(b,binding.port);encode_storage_position(b,&binding.source);}
fn encode_life_inventory(''',1)
lib.write_text(s)
p=out/'crates/zil-ir/src/wire.rs';s=p.read_text().replace('WIRE_VERSION: u32 = 38','WIRE_VERSION: u32 = 39')
a=s.index('    fn storage_shape(&mut self, shape:');b=s.index('    fn ty(',a)
s=s[:a]+(out/'runtime_encode.rs').read_text()+s[b:]
a=s.index('    fn storage_shape(&mut self)');b=s.index('    fn ty(',a)
s=s[:a]+(out/'runtime_decode.rs').read_text()+s[b:]
s=s.replace('        self.target(&module.target)?;','        self.target(&module.target)?;\n        self.life_inventory(&module.life_declarations, &module.life_instances)?;',1)
a=s.index('    fn module(&mut self) -> WireResult<TypedModule>');b=s.index('    fn ',a+8)
frag=s[a:b].replace('        let target = self.target()?;','        let target = self.target()?;\n        let (life_declarations, life_instances) = self.life_inventory()?;',1).replace('        Ok(TypedModule {','        Ok(TypedModule {\n            life_declarations, life_instances,',1)
s=s[:a]+frag+s[b:]
# Share helper for BFP encode; decode is byte-identical and stays iterative.
a=s.index('                    self.count(life.binder_count',s.index('                Type::BorrowedFunctionPointer {'));b=s.index('                    self.count(parameters.len()',a)
s=s[:a]+'                    self.function_life(life)?;\n'+s[b:]
s=s.replace('    fn life_inventory(&mut self, declarations:', '    pub(super) fn retained_port_binding(&mut self,binding:&RetainedPortBinding)->WireResult<()>{self.u32(binding.port)?;self.storage_position(&binding.source)}\n    fn life_inventory(&mut self, declarations:',1)
s=s.replace('    fn life_inventory(&mut self)->WireResult<', '    pub(super) fn retained_port_binding(&mut self)->WireResult<RetainedPortBinding>{Ok(RetainedPortBinding{port:self.u32()?,source:self.storage_position()?})}\n    fn life_inventory(&mut self)->WireResult<',1)
p.write_text(s)
p=out/'crates/zil-base/src/lib.rs';p.write_text(p.read_text().replace('SCHEMA_VERSION: u32 = 38','SCHEMA_VERSION: u32 = 39'))
# CT4 starts from the EXACT distinct CT3 public base, not the runtime tree CT2.
for f in ['ct_artifact.rs','ct_wire.rs']:
 (out/'crates/zil-ir/src'/f).write_bytes((out/'base-ct3'/f).read_bytes())
p=out/'crates/zil-ir/src/ct_artifact.rs';s=p.read_text()
s=s.replace('pub enum CtTypeNode {','pub enum CtTypeNode {\n    /// One actual storage DAG edge; relation metadata creates no physical edge.\n    BorrowedStorage { storage: CtTypeNodeId, life: crate::StorageLifeShape },',1)
s=s.replace('pub enum CtTypeConstructor {','pub enum CtTypeConstructor {\n    BorrowedStorage { life: crate::StorageLifeShape },',1)
p.write_text(s)
subprocess.run(['patch','-p1','-i','/workspace/.zil-env/terminal-20261104/shared-retained-life/ct-life-model.patch'],cwd=out,check=True)
p=out/'crates/zil-ir/src/ct_artifact.rs';s=p.read_text().replace('CT_ARTIFACT_SCHEMA_VERSION: u32 = 3','CT_ARTIFACT_SCHEMA_VERSION: u32 = 4').replace('/v3"','/v4"');p.write_text(s)
p=out/'crates/zil-ir/src/ct_wire.rs';s=p.read_text().replace('/v3"','/v4"')
# Copy the bounded storage path/shape codecs, including all semantic selectors.
old=(prior/'crates/zil-ir/src/ct_wire.rs').read_text();a=old.index('impl Wire for crate::VariantProjection');b=old.index('impl Wire for FunctionLifeContract',a)
s=s.replace('impl Wire for FunctionLifeContract',old[a:b]+'\nimpl Wire for FunctionLifeContract',1)
# Required semantic backreference: never traverse owner as a DAG child.
extra='''impl Wire for crate::StorageBinder {
 fn encode(&self,e:&mut Encoder)->Result<()>{match self{Self::Fixed(i)=>{0u8.encode(e)?;i.encode(e)},Self::Invocation(i)=>{1u8.encode(e)?;i.encode(e)}}}
 fn decode(d:&mut Decoder<'_>)->Result<Self>{match u8::decode(d)?{0=>Ok(Self::Fixed(u32::decode(d)?)),1=>Ok(Self::Invocation(u32::decode(d)?)),_=>Err(WireError::UnknownTag)}}
}
impl Wire for CtLifetimeArgument {
 fn encode(&self,e:&mut Encoder)->Result<()>{e.descend(|e|{self.owner.encode(e)?;self.binder.encode(e)})}
 fn decode(d:&mut Decoder<'_>)->Result<Self>{d.descend(|d|Ok(Self{owner:CtTypeNodeId::decode(d)?,binder:crate::StorageBinder::decode(d)?}))}
}
'''
s=s.replace('impl Wire for CtSchemaArgument {',extra+'\nimpl Wire for CtSchemaArgument {',1)
a=s.index('impl Wire for CtSchemaArgument {');b=s.index('impl Wire for CtRecordSchema {',a);frag=s[a:b]
frag=frag.replace('            Self::Scalar(value) => {','            Self::Lifetime(value) => { 2u8.encode(e)?; value.encode(e) },\n            Self::TypeInContext(value) => { 3u8.encode(e)?; value.encode(e) },\n            Self::Scalar(value) => {',1).replace('            1 => Ok(Self::Scalar(Constant::decode(d)?)),','            1 => Ok(Self::Scalar(Constant::decode(d)?)),\n            2 => Ok(Self::Lifetime(CtLifetimeArgument::decode(d)?)),\n            3 => Ok(Self::TypeInContext(CtTypeNodeId::decode(d)?)),',1)
s=s[:a]+frag+s[b:]
for name,nxt in [('CtRecordSchema','CtEnumSchema'),('CtEnumSchema','CtSchemaOrigin')]:
 a=s.index('impl Wire for '+name+' {');b=s.index('impl Wire for '+nxt+' {',a);frag=s[a:b]
 frag=frag.replace('            self.name.encode(e)?;','            self.name.encode(e)?;\n            self.life_context.encode(e)?;',1).replace('                name: <CtSymbolId as Wire>::decode(d)?,','                name: <CtSymbolId as Wire>::decode(d)?,\n                life_context: Option::<CtTypeRef>::decode(d)?,',1)
 s=s[:a]+frag+s[b:]
a=s.index('impl Wire for CtTypeArgument {');b=s.index('impl Wire for CtNameNamespace {',a);frag=s[a:b]
frag=frag.replace('                Self::Scalar(v0) => {','                Self::Lifetime(v0) => {2u8.encode(e)?;v0.encode(e)?;},\n                Self::Scalar(v0) => {',1).replace('            1 => Ok(Self::Scalar(<Constant as Wire>::decode(d)?)),','            1 => Ok(Self::Scalar(<Constant as Wire>::decode(d)?)),\n            2 => Ok(Self::Lifetime(CtLifetimeArgument::decode(d)?)),',1)
s=s[:a]+frag+s[b:]
a=s.index('impl Wire for CtTypeNode {');b=s.index('impl Wire for CtTypeArgument {',a);frag=s[a:b]
frag=frag.replace('            match self {','            match self {\n                Self::BorrowedStorage{storage,life}=>{16u8.encode(e)?;storage.encode(e)?;life.encode(e)?;},',1).replace('        d.descend(|d| match u8::decode(d)? {','        d.descend(|d| match u8::decode(d)? {\n            16=>Ok(Self::BorrowedStorage{storage:CtTypeNodeId::decode(d)?,life:crate::StorageLifeShape::decode(d)?}),',1)
s=s[:a]+frag+s[b:]
a=s.index('impl Wire for CtTypeConstructor {');b=s.index('impl Wire for ',a+16);frag=s[a:b]
frag=frag.replace('            match self {','            match self {\n                Self::BorrowedStorage{life}=>{12u8.encode(e)?;life.encode(e)?;},',1).replace('        d.descend(|d| match u8::decode(d)? {','        d.descend(|d| match u8::decode(d)? {\n            12=>Ok(Self::BorrowedStorage{life:crate::StorageLifeShape::decode(d)?}),',1)
s=s[:a]+frag+s[b:];p.write_text(s)
for f in ['crates/zil-base/src/lib.rs','crates/zil-ir/src/lib.rs','crates/zil-ir/src/wire.rs','crates/zil-ir/src/ct_artifact.rs','crates/zil-ir/src/ct_wire.rs']:
 subprocess.run(['rustfmt','--edition','2024','--config','skip_children=true',str(out/f)],check=True)
# Build two exact-base atomic patches, with one manifest for their combined tree.
runtime=['crates/zil-base/src/lib.rs','crates/zil-ir/src/lib.rs','crates/zil-ir/src/wire.rs'];ct=['crates/zil-ir/src/ct_artifact.rs','crates/zil-ir/src/ct_wire.rs'];receipt={}
for name,files in [('runtime39-inventory.patch',runtime),('ct4-scope.patch',ct)]:
 patch=''
 for f in files:
  a=(src/f).read_bytes() if f in runtime else (out/'base-ct3'/Path(f).name).read_bytes();b=(out/f).read_bytes()
  patch+=''.join(difflib.unified_diff(a.decode().splitlines(True),b.decode().splitlines(True),fromfile='a/'+f,tofile='b/'+f))
  receipt[f]={'base':hashlib.sha256(a).hexdigest(),'candidate':hashlib.sha256(b).hexdigest(),'base_commit':'b377f8e9f' if f in runtime else 'b46027807'}
 (out/name).write_text(patch);receipt[name]=hashlib.sha256(patch.encode()).hexdigest()
(out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps({k:v for k,v in receipt.items() if k.endswith('.patch')},indent=2))
