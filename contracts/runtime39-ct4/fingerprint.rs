fn encode_declaration_id(b:&mut impl FingerprintSink,id:&LifeDeclarationId){encode_u32(b,id.module.0);encode_u32(b,id.declaration.0);b.push(u8::from(id.compiler));}
fn encode_declaration_identity(b:&mut impl FingerprintSink,id:&LifeDeclarationIdentity){
    encode_text(b,&id.package);
    for path in [&id.module_path,&id.declaration_path]{encode_len(b,path.len());for part in path{encode_text(b,part);}}
    encode_len(b,id.generation_path.len());for part in &id.generation_path{encode_u32(b,*part);}
}
fn encode_parameter_kind(b:&mut impl FingerprintSink,kind:&LifeParameterKind){match kind{LifeParameterKind::Type=>b.push(0),LifeParameterKind::Const(ty)=>{b.push(1);encode_type(b,ty);},LifeParameterKind::Lifetime=>b.push(2)}}
fn encode_storage_binder(b:&mut impl FingerprintSink,binder:&StorageBinder){match binder{StorageBinder::Fixed(i)=>{b.push(0);encode_u32(b,*i)},StorageBinder::Invocation(i)=>{b.push(1);encode_u32(b,*i)}}}
fn encode_storage_position(b:&mut impl FingerprintSink,position:&StoragePosition){
    match position.base{StorageBase::Parameter(i)=>{b.push(0);encode_u32(b,i)},StorageBase::Capture(i)=>{b.push(1);encode_u32(b,i)},StorageBase::Result=>b.push(2)}
    encode_len(b,position.path.len());for step in &position.path{match step{ContractProjection::Place(p)=>{b.push(0);encode_storage_path(b,std::slice::from_ref(p))},ContractProjection::RetainedPort(i)=>{b.push(1);encode_u32(b,*i)},ContractProjection::RetainedClass(i)=>{b.push(2);encode_u32(b,*i)}}}
}
fn encode_life_maps(b:&mut impl FingerprintSink,retained:&[StorageRetainedBinding],nominal_arguments:&[NominalLifeArgument]){
    encode_len(b,retained.len());for binding in retained{encode_life_type_path(b,&binding.path);encode_u32(b,binding.class);encode_u32(b,binding.binder);}
    encode_len(b,nominal_arguments.len());for argument in nominal_arguments{encode_life_type_path(b,&argument.path);encode_declaration_id(b,&argument.declaration);encode_u32(b,argument.argument);encode_u32(b,argument.binder);}
}
fn encode_retained_shape(b:&mut impl FingerprintSink,shape:&RetainedLifeShape){encode_u32(b,shape.binder_count);encode_len(b,shape.ports.len());for port in &shape.ports{encode_type(b,&port.leaf);b.push(port.mode as u8);encode_u32(b,port.binder);}encode_life_maps(b,&shape.retained,&shape.nominal_arguments);}
fn encode_required_storage_contract(b:&mut impl FingerprintSink,contract:&StorageLifeContract){encode_u32(b,contract.fixed_binder_count);encode_u32(b,contract.invocation_binder_count);encode_len(b,contract.bindings.len());for binding in &contract.bindings{encode_storage_position(b,&binding.position);encode_storage_binder(b,&binding.binder);}}
fn encode_storage_contract(b:&mut impl FingerprintSink,contract:&Option<StorageLifeContract>){b.push(u8::from(contract.is_some()));if let Some(contract)=contract{encode_required_storage_contract(b,contract);}}
fn encode_function_life(b:&mut impl FingerprintSink,life:&FunctionLifeContract){encode_u32(b,life.binder_count);encode_len(b,life.parameter_lives.len());for tag in &life.parameter_lives{b.push(u8::from(tag.is_some()));if let Some(tag)=tag{encode_u32(b,*tag);}}b.push(u8::from(life.result_life.is_some()));if let Some(tag)=life.result_life{encode_u32(b,tag);}}
fn encode_life_inventory(b:&mut impl FingerprintSink,declarations:&[LifeNominalDeclaration],instances:&[LifeNominalInstance]){
    encode_len(b,declarations.len());for declaration in declarations{encode_declaration_id(b,&declaration.id);encode_declaration_identity(b,&declaration.identity);b.push(match declaration.kind{LifeNominalKind::Record=>0,LifeNominalKind::Package=>1,LifeNominalKind::Resource=>2,LifeNominalKind::Enum=>3});encode_len(b,declaration.parameters.len());for parameter in &declaration.parameters{encode_parameter_kind(b,parameter);}}
    encode_len(b,instances.len());for instance in instances{encode_declaration_id(b,&instance.declaration);encode_type(b,&instance.ty);encode_len(b,instance.arguments.len());for argument in &instance.arguments{match argument{LifeNominalActual::Type(ty)=>{b.push(0);encode_type(b,ty)},LifeNominalActual::Const(value)=>{b.push(1);encode_constant(b,value)},LifeNominalActual::Lifetime(i)=>{b.push(2);encode_u32(b,*i)}}}}
}
