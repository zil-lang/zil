//! Immutable-module structural scheduling. No runtime source, class equation,
//! caller premise or permission is a member of this directory.
use super::*;
use std::any::TypeId;

type Profile=(usize,usize);
type RecipeKey=(TypeId,usize);
type OwnerKey=(u8,zil_base::ModuleId,zil_base::NodeId,bool,u32);
fn profile(b:&Budget)->Profile{(b.limits.max_nodes,b.limits.max_call_depth.min(128))}
fn lookup(n:usize)->usize{n.checked_ilog2().map_or(1,|n|n as usize+1)}
fn owner_key(owner:&SemanticDynOwner)->OwnerKey{match owner{
    SemanticDynOwner::Declaration(id)=>(0,id.module,id.declaration,id.compiler,0),
    SemanticDynOwner::MethodSignature{declaration:id,member}=>(1,id.module,id.declaration,id.compiler,*member),
    SemanticDynOwner::MethodDefinition(id)=>(2,id.module,id.source,false,0),
}}
fn cacheable(ty:&Type)->bool{!matches!(ty,
    Type::ByteBuffer|Type::ForeignHandle|Type::SyncCell(_)|Type::TaskI32|Type::CWideString|Type::Closure(_)|Type::Dyn{..}|Type::OwnedDyn{..}
)&&!ty.is_b0_scalar()}

pub(crate) struct Structure{
    module:usize,
    program:Option<usize>,
    declarations:(usize,usize),
    types:BTreeSet<usize>,
    recipes:BTreeSet<RecipeKey>,
    declaration_nodes:BTreeSet<usize>,
    type_success:BTreeSet<(usize,usize,Profile)>,
    recipe_success:BTreeSet<(RecipeKey,Profile)>,
    declaration_success:BTreeSet<(usize,Option<SemanticDefinitionId>,Profile)>,
    resource_success:BTreeSet<(usize,Profile)>,
    owners:BTreeMap<(OwnerKey,SemanticDefinitionId),usize>,
}
impl Structure{
    /// The root checker keeps this exact module immutably borrowed until this
    /// directory is removed. Nested module checks install a separate scope.
    pub(crate) fn register(module:&TypedModule,b:&mut Budget)->Check<Self>{
        b.spend(12)?;
        let mut result=Self{
            module:module as *const TypedModule as usize,
            program:module.semantic.as_ref().map(|program|program as *const SemanticModule as usize),
            declarations:(module.dyn_declarations.as_ptr() as usize,module.dyn_declarations.len()),
            types:BTreeSet::new(),recipes:BTreeSet::new(),declaration_nodes:BTreeSet::new(),
            type_success:BTreeSet::new(),recipe_success:BTreeSet::new(),declaration_success:BTreeSet::new(),resource_success:BTreeSet::new(),owners:BTreeMap::new(),
        };
        for function in &module.functions{
            result.ty(&function.return_type,b)?;
            for local in &function.locals{result.ty(&local.ty,b)?;}
            for ty in &function.values{result.ty(ty,b)?;}
            if let Some(instance)=&function.dyn_instance{result.visit(instance,b)?;}
        }
        for closure in &module.closures{for ty in closure.captures.iter().chain(&closure.parameters).chain(std::iter::once(&closure.result)){result.ty(ty,b)?;}}
        for resource in &module.resources{result.ty(&resource.payload,b)?;}
        for instance in &module.life_instances{result.ty(&instance.ty,b)?;}
        for declaration in &module.dyn_declarations{
            b.spend(lookup(result.declaration_nodes.len()).saturating_add(2))?;
            result.declaration_nodes.insert(declaration as *const DynDeclaration as usize);
            for kind in &declaration.parameters{if let LifeParameterKind::Const(ty)=kind{result.ty(ty,b)?;}}
            for member in &declaration.members{if let DynMemberKind::Method(formals)=&member.kind{for formal in formals{if let LifeParameterKind::Const(ty)=formal{result.ty(ty,b)?;}}}}
            macro_rules! rows{($field:ident)=>{if let Some(rows)=&declaration.$field{for row in rows{result.visit(row,b)?;}}};}
            rows!(implicit_lifetimes);rows!(direct_supertraits);rows!(requirements);rows!(associated_definitions);rows!(method_signatures);rows!(method_definitions);
            if let Some(pattern)=&declaration.self_pattern{result.visit(pattern,b)?;}
            if let Some(application)=&declaration.implemented_trait{result.visit(application,b)?;}
        }
        for interface in &module.dyn_interfaces{
            if let Some(context)=&interface.context{result.ty(context,b)?;}
            for row in &interface.associated_types{result.ty(&row.ty,b)?;}
            for row in &interface.associated_constants{result.ty(&row.ty,b)?;}
            for port in &interface.retained_life.ports{result.ty(&port.leaf,b)?;}
            for slot in &interface.slots{for ty in slot.parameters.iter().chain(std::iter::once(&slot.result)){result.ty(ty,b)?;}}
        }
        for table in &module.dyn_vtables{result.ty(&table.self_type,b)?;if let Some(rows)=&table.implementation_witnesses{for row in rows{result.visit(row,b)?;}}}
        if let Some(program)=&module.semantic{
            result.visit(program,b)?;
            for definition in &program.definitions{
                result.recipe(definition,b)?;
                for requirement in &definition.requirements{result.requirement(requirement,b)?;}
                for formal in &definition.parameters{result.formal(formal,b)?;}
                for formal in &definition.environment{result.formal(&formal.kind,b)?;result.recipe(&formal.source,b)?;}
                match &definition.body{
                    SemanticDefinitionBody::FunctionWithBody{signature,sites,..}|SemanticDefinitionBody::Function{signature,sites,..}=>{
                        result.recipe(signature,b)?;
                        for parameter in &signature.parameters{result.recipe(&parameter.ty,b)?;}
                        result.recipe(&signature.result,b)?;
                        for site in sites{result.recipe(&site.kind,b)?;}
                    },
                    SemanticDefinitionBody::Nominal{body,..}=>match body{
                        SemanticNominalBody::Product{fields}=>for field in fields{result.recipe(&field.ty,b)?;},
                        SemanticNominalBody::Enum{variants}=>for variant in variants{for field in &variant.fields{result.recipe(field,b)?;}},
                    },
                    _=>{},
                }
            }
        }
        Ok(result)
    }
    fn recipe<T:'static>(&mut self,value:&T,b:&mut Budget)->Check<()>{
        b.spend(lookup(self.recipes.len()).saturating_add(2))?;
        self.recipes.insert((TypeId::of::<T>(),value as *const T as usize));Ok(())
    }
    fn visit<T:DynRecipeMetadata+'static>(&mut self,value:&T,b:&mut Budget)->Check<()>{
        self.recipe(value,b)?;value.visit_dyn_metadata(&mut Register{structure:self,b},0)
    }
    fn requirement(&mut self,value:&SemanticRequirementRecipe,b:&mut Budget)->Check<()>{
        let mut pending=vec![value];
        while let Some(row)=pending.pop(){self.recipe(row,b)?;if let SemanticRequirementRecipe::ForAll{requirements,..}=row{b.spend(requirements.len())?;pending.extend(requirements);}}
        Ok(())
    }
    fn formal(&mut self,value:&SemanticFormal,b:&mut Budget)->Check<()>{
        self.recipe(value,b)?;
        match value{SemanticFormal::Type{requirements}=>for row in requirements{self.requirement(row,b)?;},SemanticFormal::Const{carrier}=>self.recipe(carrier,b)?,_=>{}}
        Ok(())
    }
    fn ty(&mut self,root:&Type,b:&mut Budget)->Check<()>{
        b.spend(2)?;let mut pending=vec![root];
        while let Some(ty)=pending.pop(){
            b.spend(2)?;
            if cacheable(ty){
                b.spend(lookup(self.types.len()).saturating_add(2))?;
                if !self.types.insert(ty as *const Type as usize){continue;}
            }
            match ty{
                Type::BorrowedStorage{storage:inner,..}|Type::Address{inner,..}|Type::Managed(inner)|Type::Desc(inner)|Type::Slice{element:inner}|Type::Array{element:inner,..}|Type::OwnedStore{element:inner}|Type::Mutex{payload:inner}|Type::Registration{environment:inner}|Type::AsyncRegistration{environment:inner}|Type::Opaque{hidden:inner,..}=>{b.spend(1)?;pending.push(inner);},
                Type::Task{result,retained_life}|Type::MutexGuard{payload:result,retained_life}=>{b.spend(retained_life.ports.len().saturating_add(1))?;pending.extend(retained_life.ports.iter().map(|row|&row.leaf));pending.push(result);},
                Type::Tuple(fields)=>{b.spend(fields.len())?;pending.extend(fields);},
                Type::Record{fields,..}|Type::Package{fields,..}=>{b.spend(fields.len())?;pending.extend(fields.iter().map(|(_,ty)|ty));},
                Type::Enum{variants,..}=>for(_,fields)in variants{b.spend(fields.len().saturating_add(1))?;pending.extend(fields);},
                Type::ErrorUnion{error,result}=>{b.spend(2)?;pending.push(error);pending.push(result);},
                Type::FunctionPointer{parameters,result}|Type::ForeignCallback{parameters,result}|Type::BorrowedFunctionPointer{parameters,result,..}=>{b.spend(parameters.len().saturating_add(1))?;pending.extend(parameters);pending.push(result);},
                _=>{},
            }
        }Ok(())
    }
}
struct Register<'a>{structure:&'a mut Structure,b:&'a mut Budget}
impl DynRecipeVisitor for Register<'_>{
    type Error=Diagnostic;
    fn node(&mut self,_:usize)->Check<()>{self.b.spend(1)}
    fn rows(&mut self,n:usize,_:usize)->Check<()>{self.b.spend(n.saturating_add(1))}
    fn scratch(&mut self,n:usize,_:usize)->Check<()>{self.b.spend(n)}
    fn ty(&mut self,ty:&Type,_:usize)->Check<()>{self.structure.ty(ty,self.b)}
    fn life(&mut self,_:&StorageLifeShape,_:usize)->Check<()>{self.b.spend(1)}
    fn identity(&mut self,_:&LifeDeclarationIdentity,_:usize)->Check<()>{self.b.spend(1)}
    fn position(&mut self,_:&StoragePosition,_:usize)->Check<()>{self.b.spend(1)}
    fn constant(&mut self,_:&Constant,_:usize)->Check<()>{self.b.spend(1)}
    fn text(&mut self,_:&str,_:usize)->Check<()>{self.b.spend(1)}
}

pub(super) fn type_known(ty:&Type,depth:usize,b:&mut Budget)->Check<bool>{
    if !cacheable(ty){return Ok(false);}
    let Some(state)=&b.structural else{return Ok(false);};
    let fee=lookup(state.types.len()).saturating_add(lookup(state.type_success.len()));
    b.spend(fee)?;
    let state=b.structural.as_ref().unwrap();let ptr=ty as *const Type as usize;
    Ok(state.types.contains(&ptr)&&state.type_success.contains(&(ptr,depth,profile(b))))
}
pub(super) fn type_success(ty:&Type,depth:usize,b:&mut Budget)->Check<()>{
    if !cacheable(ty){return Ok(());}
    let Some(state)=&b.structural else{return Ok(());};let ptr=ty as *const Type as usize;
    let membership=lookup(state.types.len());b.spend(membership)?;
    if !b.structural.as_ref().unwrap().types.contains(&ptr){return Ok(());}
    let state=b.structural.as_ref().unwrap();
    let key=(ptr,depth,profile(b));let fee=lookup(state.type_success.len()).saturating_add(2);
    b.spend(fee)?;b.structural.as_mut().unwrap().type_success.insert(key);Ok(())
}
pub(super) fn recipe_known<T:'static>(value:&T,b:&mut Budget)->Check<bool>{
    let Some(state)=&b.structural else{return Ok(false);};let fee=lookup(state.recipes.len()).saturating_add(lookup(state.recipe_success.len()));
    b.spend(fee)?;let state=b.structural.as_ref().unwrap();let key=(TypeId::of::<T>(),value as *const T as usize);
    Ok(state.recipes.contains(&key)&&state.recipe_success.contains(&(key,profile(b))))
}
pub(super) fn recipe_success<T:'static>(value:&T,b:&mut Budget)->Check<()>{
    let Some(state)=&b.structural else{return Ok(());};let key=(TypeId::of::<T>(),value as *const T as usize);
    let membership=lookup(state.recipes.len());b.spend(membership)?;
    if !b.structural.as_ref().unwrap().recipes.contains(&key){return Ok(());}
    let state=b.structural.as_ref().unwrap();
    let key=(key,profile(b));let fee=lookup(state.recipe_success.len()).saturating_add(2);
    b.spend(fee)?;b.structural.as_mut().unwrap().recipe_success.insert(key);Ok(())
}
pub(super) fn declaration_known(value:&DynDeclaration,owner:Option<SemanticDefinitionId>,b:&mut Budget)->Check<bool>{
    let Some(state)=&b.structural else{return Ok(false);};let ptr=value as *const DynDeclaration as usize;
    let fee=lookup(state.declaration_nodes.len()).saturating_add(lookup(state.declaration_success.len()));
    b.spend(fee)?;let state=b.structural.as_ref().unwrap();
    Ok(state.declaration_nodes.contains(&ptr)&&state.declaration_success.contains(&(ptr,owner,profile(b))))
}
pub(super) fn declaration_success(value:&DynDeclaration,owner:Option<SemanticDefinitionId>,b:&mut Budget)->Check<()>{
    let Some(state)=&b.structural else{return Ok(());};let ptr=value as *const DynDeclaration as usize;
    let membership=lookup(state.declaration_nodes.len());b.spend(membership)?;
    if !b.structural.as_ref().unwrap().declaration_nodes.contains(&ptr){return Ok(());}
    let state=b.structural.as_ref().unwrap();
    let key=(ptr,owner,profile(b));let fee=lookup(state.declaration_success.len()).saturating_add(2);
    b.spend(fee)?;b.structural.as_mut().unwrap().declaration_success.insert(key);Ok(())
}
pub(crate) fn owner_known(program:&SemanticModule,declarations:&[DynDeclaration],owner:&SemanticDynOwner,requested:SemanticDefinitionId,b:&mut Budget)->Check<Option<usize>>{
    let Some(state)=&b.structural else{return Ok(None);};
    if state.program!=Some(program as *const SemanticModule as usize)||state.declarations!=(declarations.as_ptr() as usize,declarations.len()){return Ok(None);}
    let fee=lookup(state.owners.len());b.spend(fee)?;
    Ok(b.structural.as_ref().unwrap().owners.get(&(owner_key(owner),requested)).copied())
}
pub(crate) fn owner_success(program:&SemanticModule,declarations:&[DynDeclaration],owner:&SemanticDynOwner,requested:SemanticDefinitionId,index:usize,b:&mut Budget)->Check<()>{
    let Some(state)=&b.structural else{return Ok(());};
    if state.program!=Some(program as *const SemanticModule as usize)||state.declarations!=(declarations.as_ptr() as usize,declarations.len()){return Ok(());}
    let fee=lookup(state.owners.len()).saturating_add(2);b.spend(fee)?;
    b.structural.as_mut().unwrap().owners.insert((owner_key(owner),requested),index);Ok(())
}
pub(crate) fn resource_known(module:&TypedModule,record:&Type,b:&mut Budget)->Check<bool>{
    let Some(state)=&b.structural else{return Ok(false);};
    if state.module!=module as *const TypedModule as usize{return Ok(false);}
    let ptr=record as *const Type as usize;let fee=lookup(state.types.len()).saturating_add(lookup(state.resource_success.len()));
    b.spend(fee)?;let state=b.structural.as_ref().unwrap();
    Ok(state.types.contains(&ptr)&&state.resource_success.contains(&(ptr,profile(b))))
}
pub(crate) fn resource_success(module:&TypedModule,record:&Type,b:&mut Budget)->Check<()>{
    let Some(state)=&b.structural else{return Ok(());};let ptr=record as *const Type as usize;
    if state.module!=module as *const TypedModule as usize{return Ok(());}
    let membership=lookup(state.types.len());b.spend(membership)?;
    if !b.structural.as_ref().unwrap().types.contains(&ptr){return Ok(());}
    let state=b.structural.as_ref().unwrap();
    let key=(ptr,profile(b));let fee=lookup(state.resource_success.len()).saturating_add(2);b.spend(fee)?;
    b.structural.as_mut().unwrap().resource_success.insert(key);Ok(())
}

#[cfg(test)]
mod tests{
    use super::*;
    fn record()->Type{Type::Record{name:"registered_complete_payload_".repeat(16),fields:vec![("whole_field".repeat(8),Type::Tuple(vec![Type::I32,Type::Array{element:Box::new(Type::U8),length:4}]))]}}
    fn fixture()->TypedModule{
        let(mut module,_)=crate::tests::baseline();let payload=record();
        let Type::Record{name,..}=&payload else{unreachable!()};
        module.resources.push(ResourceDefinition{name:name.clone(),payload:payload.clone(),teardown:None});
        let f=&mut module.functions[0];let id=LocalId(f.locals.len() as u32);
        f.locals.push(LocalDecl{id,name:"registered_owner".into(),ty:Type::Managed(Box::new(payload)),mutable:true,span:f.span});
        module
    }
    fn budget(module:&TypedModule)->Budget{
        let mut b=Budget::new(&Limits::default());
        b.structural=Some(Structure::register(module,&mut b).unwrap());b
    }
    #[test]
    fn immutable_type_memo_keeps_each_depth_quota_and_temporary_cold_walk(){
        let module=fixture();let ty=&module.functions[0].locals.last().unwrap().ty;
        let mut b=budget(&module);let before=b.remaining;
        super::super::type_bounds_at(ty,0,&mut b).unwrap();let cold=before-b.remaining;let before=b.remaining;
        super::super::type_bounds_at(ty,0,&mut b).unwrap();let hot=before-b.remaining;assert!(hot<cold);
        let before=b.remaining;super::super::ty(ty,&mut b).unwrap();assert!(before-b.remaining>hot,"copy/equality prepayment is not a pure bounds lookup");
        let entries=b.structural.as_ref().unwrap().type_success.len();
        let temporary=ty.clone();let before=b.remaining;
        super::super::type_bounds_at(&temporary,0,&mut b).unwrap();assert!(before-b.remaining>=cold/2);
        assert_eq!(entries,b.structural.as_ref().unwrap().type_success.len());
        b.limits.max_nodes=1;
        let actual=super::super::type_bounds_at(ty,0,&mut b).unwrap_err();
        let expected=super::super::ty_at(ty,0,&mut Budget::new(&b.limits)).unwrap_err();assert_eq!(actual,expected);
        b.limits.max_nodes=Limits::default().max_nodes;
        let depth=b.limits.max_call_depth.min(128);
        let actual=super::super::type_bounds_at(ty,depth,&mut b).unwrap_err();
        let expected=super::super::ty_at(ty,depth,&mut Budget::new(&b.limits)).unwrap_err();assert_eq!(actual,expected);
        assert_eq!(entries,b.structural.as_ref().unwrap().type_success.len());
        super::super::type_bounds_at(ty,0,&mut b).unwrap();
    }
    #[test]
    fn original_recipe_memo_does_not_grant_smaller_domains_or_temporary_identity(){
        let(mut module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("../semantic/fixtures/ordinary-identity-complete-head.wire"),&Limits::default()).unwrap();
        module.semantic.as_mut().unwrap().definitions[0].requirements.push(SemanticRequirementRecipe::ForAll{lifetimes:1,requirements:vec![SemanticRequirementRecipe::Structural{ty:SemanticTypeRecipe::Atom(record()),bound:DynStructuralBound::Copy}]});
        let recipe=module.semantic.as_ref().unwrap().definitions[0].requirements.last().unwrap();
        let mut b=budget(&module);let before=b.remaining;
        super::super::bounds_recipe(recipe,&mut b).unwrap();let cold=before-b.remaining;let before=b.remaining;
        super::super::bounds_recipe(recipe,&mut b).unwrap();let hot=before-b.remaining;assert!(hot<cold);
        let before=b.remaining;super::super::recipe(recipe,&mut b).unwrap();assert!(before-b.remaining>hot,"recipe copying keeps its complete old payment");
        let entries=b.structural.as_ref().unwrap().recipe_success.len();let temporary=recipe.clone();
        super::super::bounds_recipe(&temporary,&mut b).unwrap();assert_eq!(entries,b.structural.as_ref().unwrap().recipe_success.len());
        for (nodes,depth) in [(1,128),(Limits::default().max_nodes,0)]{
            b.limits.max_nodes=nodes;b.limits.max_call_depth=depth;
            let actual=super::super::bounds_recipe(recipe,&mut b).unwrap_err();
            let expected=super::super::recipe(recipe,&mut Budget::new(&b.limits)).unwrap_err();assert_eq!(actual,expected);
        }
    }
    #[test]
    fn resource_static_memo_keeps_other_modules_and_raw_wire_counterexamples_cold(){
        let module=fixture();let carrier=&module.functions[0].locals.last().unwrap().ty;let span=Some(module.functions[0].span);
        let mut b=budget(&module);crate::resources::check_type_for(&module,carrier,span,&mut b).unwrap();
        assert_eq!(b.structural.as_ref().unwrap().resource_success.len(),1);
        let before=b.remaining;crate::resources::check_type_for(&module,carrier,span,&mut b).unwrap();let hot=before-b.remaining;
        let temporary=carrier.clone();let before=b.remaining;crate::resources::check_type_for(&module,&temporary,span,&mut b).unwrap();assert!(before-b.remaining>hot);
        let mut bad=module.clone();bad.resources.clear();
        let bytes=zil_ir::wire::encode_module_candidate(&bad,&CandidatePlan::empty_for(&bad),&Limits::default()).unwrap();
        let(decoded,_)=zil_ir::wire::decode_module_candidate(&bytes,&Limits::default()).unwrap();
        for other in [&bad,&decoded]{
            let actual=crate::resources::check_type_for(other,carrier,span,&mut b).unwrap_err();
            let expected=crate::resources::check_type_for(other,carrier,span,&mut Budget::new(&Limits::default())).unwrap_err();assert_eq!(actual,expected);
        }
        assert_eq!(b.structural.as_ref().unwrap().resource_success.len(),1);
    }
    #[test]
    fn nested_module_scope_and_registration_failure_restore_parent_directory(){
        let parent=fixture();let mut b=budget(&parent);
        let parent_key=b.structural.as_ref().unwrap().module;
        let(child,plan)=crate::tests::baseline();
        crate::check_module_budget(&child,&plan,&child.target,&Limits::default(),None,&mut b,None).unwrap();
        assert_eq!(b.structural.as_ref().unwrap().module,parent_key);
        b.remaining=0;
        assert!(crate::check_module_budget(&child,&plan,&child.target,&Limits::default(),None,&mut b,None).is_err());
        assert_eq!(b.structural.as_ref().unwrap().module,parent_key);
    }
    #[test]
    fn complete_registered_requests_keep_exact_n_minus_one_and_fresh_work(){
        let module=fixture();let carrier=&module.functions[0].locals.last().unwrap().ty;
        let run=|steps|->Check<(usize,usize,usize)>{
            let mut b=Budget::new(&Limits{max_analysis_steps:steps,..Limits::default()});
            b.structural=Some(Structure::register(&module,&mut b)?);
            for _ in 0..8{super::super::type_bounds_at(carrier,0,&mut b)?;crate::resources::check_type_for(&module,carrier,None,&mut b)?;}
            let state=b.structural.as_ref().unwrap();
            Ok((state.type_success.len(),state.resource_success.len(),b.remaining))
        };
        let room=1_000_000;let used=room-run(room).unwrap().2;
        assert_eq!(run(used).unwrap(),(1,1,0));
        assert_eq!(run(used-1).unwrap_err().category,zil_base::ResultCategory::ResourceLimit);
        assert_eq!(run(used).unwrap(),(1,1,0));
    }
}
