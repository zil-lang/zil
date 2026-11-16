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

type TypeFact=(usize,usize,Profile);
// This is the existing exact success set, partitioned by pointer identity.
// A collision still searches a complete ordered key (depth and profile too),
// so no approximate key can prove a fact. Growth and every real key copy are
// paid before insertion; the old set remains intact if growth runs out of work.
struct TypeFacts{buckets:Vec<BTreeSet<TypeFact>>,len:usize}
impl TypeFacts{
    fn new(b:&mut Budget)->Check<Self>{
        let mut result=Self{buckets:Vec::new(),len:0};
        result.grow(8,b)?;Ok(result)
    }
    #[cfg(test)]
    fn len(&self)->usize{self.len}
    fn bucket(ptr:usize,count:usize)->usize{
        let hash=ptr.wrapping_mul(0x9e3779b97f4a7c15u64 as usize);
        hash>>(usize::BITS-count.ilog2())
    }
    fn contains(&self,key:&TypeFact,b:&mut Budget)->Check<bool>{
        // Registration publishes only a nonempty bucket directory. Its
        // one-time allocation removes the per-query empty-directory branch;
        // an empty fact set still performs the exact ordinary bucket lookup.
        b.spend(3)?;let bucket=&self.buckets[Self::bucket(key.0,self.buckets.len())];
        b.spend(lookup(bucket.len()))?;Ok(bucket.contains(key))
    }
    fn grow(&mut self,count:usize,b:&mut Budget)->Check<()>{
        let words=std::mem::size_of::<BTreeSet<TypeFact>>().div_ceil(std::mem::size_of::<usize>());
        b.spend(count.saturating_mul(words).saturating_add(self.buckets.len()).saturating_add(2))?;
        let mut buckets:Vec<BTreeSet<TypeFact>>=(0..count).map(|_|BTreeSet::new()).collect();
        for old in &self.buckets{for key in old{
            b.spend(3)?;let to=Self::bucket(key.0,count);
            b.spend(lookup(buckets[to].len()).saturating_add(6))?;
            buckets[to].insert(*key);
        }}
        self.buckets=buckets;Ok(())
    }
    fn insert(&mut self,key:TypeFact,b:&mut Budget)->Check<()>{
        if self.len.saturating_add(1)>self.buckets.len()/2&&self.buckets.len()<4096{self.grow(self.buckets.len()*2,b)?;}
        b.spend(3)?;let to=Self::bucket(key.0,self.buckets.len());
        b.spend(lookup(self.buckets[to].len()).saturating_add(6))?;
        if self.buckets[to].insert(key){self.len+=1;}Ok(())
    }
}

pub(crate) struct Structure{
    module:usize,
    program:Option<usize>,
    declarations:(usize,usize),
    types:BTreeSet<usize>,
    recipes:BTreeSet<RecipeKey>,
    declaration_nodes:BTreeSet<usize>,
    type_success:TypeFacts,
    recipe_success:BTreeSet<(RecipeKey,Profile)>,
    declaration_success:BTreeSet<(usize,Option<SemanticDefinitionId>,Profile)>,
    resource_success:BTreeMap<(usize,Profile),usize>,
    store_success:BTreeMap<(usize,bool,bool),Vec<Limits>>,
    stored_bounds:BTreeMap<(usize,usize),Vec<Limits>>,
    owners:BTreeMap<(OwnerKey,SemanticDefinitionId),usize>,
    shared_types:BTreeMap<usize,std::rc::Rc<crate::semantic::CheckedTypeData>>,
    type_owners:BTreeMap<usize,std::rc::Rc<crate::semantic::CheckedTypeData>>,
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
            type_success:TypeFacts::new(b)?,recipe_success:BTreeSet::new(),declaration_success:BTreeSet::new(),resource_success:BTreeMap::new(),store_success:BTreeMap::new(),stored_bounds:BTreeMap::new(),owners:BTreeMap::new(),shared_types:BTreeMap::new(),type_owners:BTreeMap::new(),
        };
        for function in &module.functions{
            result.ty(&function.return_type,b)?;
            for local in &function.locals{result.ty(&local.ty,b)?;}
            for ty in &function.values{result.ty(ty,b)?;}
            if let Some(instance)=&function.dyn_instance{result.visit(instance,b)?;}
        }
        for closure in &module.closures{for ty in closure.captures.iter().chain(&closure.parameters).chain(std::iter::once(&closure.result)){result.ty(ty,b)?;}}
        for resource in &module.resources{result.ty(&resource.payload,b)?;}
        for instance in &module.life_instances{
            result.ty(&instance.ty,b)?;
            for actual in &instance.arguments{b.spend(1)?;match actual{LifeNominalActual::Type(ty)|LifeNominalActual::QualifiedType{ty,..}=>result.ty(ty,b)?,_=>{}}}
        }
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
    let Some(_)=&b.structural else{return Ok(false);};
    // Only a registered, still-live immutable node can enter type_success.
    // A second tree search for its membership repeats that established fact.
    let key=(ty as *const Type as usize,depth,profile(b));
    let state=b.structural.take().unwrap();
    let result=state.type_success.contains(&key,b);b.structural=Some(state);result
}
pub(super) fn type_success(ty:&Type,depth:usize,b:&mut Budget)->Check<()>{
    if !cacheable(ty){return Ok(());}
    let Some(state)=&b.structural else{return Ok(());};let ptr=ty as *const Type as usize;
    let membership=lookup(state.types.len());b.spend(membership)?;
    if !b.structural.as_ref().unwrap().types.contains(&ptr){return Ok(());}
    let key=(ptr,depth,profile(b));let mut state=b.structural.take().unwrap();
    let result=state.type_success.insert(key,b);b.structural=Some(state);result
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
pub(crate) fn resource_known(module:&TypedModule,record:&Type,b:&mut Budget)->Check<Option<usize>>{
    let Some(state)=&b.structural else{return Ok(None);};
    if state.module!=module as *const TypedModule as usize{return Ok(None);}
    let ptr=record as *const Type as usize;let fee=lookup(state.resource_success.len());
    b.spend(fee)?;let state=b.structural.as_ref().unwrap();
    let found=state.resource_success.get(&(ptr,profile(b))).copied();
    if b.diagnostic_progress.is_some(){
        let registered=state.types.contains(&ptr);
        let progress=b.diagnostic_progress.as_mut().unwrap();
        if found.is_some(){progress.resource_hits=progress.resource_hits.saturating_add(1);}
        else if registered{progress.resource_registered_cold=progress.resource_registered_cold.saturating_add(1);}
        else{progress.resource_unregistered_cold=progress.resource_unregistered_cold.saturating_add(1);}
    }
    Ok(found)
}
pub(crate) fn resource_success(module:&TypedModule,record:&Type,index:usize,b:&mut Budget)->Check<()>{
    let Some(state)=&b.structural else{return Ok(());};let ptr=record as *const Type as usize;
    if state.module!=module as *const TypedModule as usize{return Ok(());}
    let membership=lookup(state.types.len());b.spend(membership)?;
    if !b.structural.as_ref().unwrap().types.contains(&ptr){return Ok(());}
    let state=b.structural.as_ref().unwrap();
    let key=(ptr,profile(b));let fee=lookup(state.resource_success.len()).saturating_add(2);b.spend(fee)?;
    b.structural.as_mut().unwrap().resource_success.insert(key,index);Ok(())
}

// The stored bounds visitor has its own depth convention and diagnostics.
// It is not a copy/Eq prepayment and proves no shape/nominal/κ permission.
pub(crate) fn stored_type_known(ty:&Type,limits:&Limits,b:&mut Budget)->Check<bool>{
    if !cacheable(ty){return Ok(false);}
    let Some(state)=&b.structural else{return Ok(false);};
    let fee=lookup(state.stored_bounds.len());b.spend(fee)?;
    let state=b.structural.take().unwrap();
    let result=(||{
        if let Some(profiles)=state.stored_bounds.get(&(ty as *const Type as usize,limits.max_call_depth)){
            for profile in profiles{b.spend(12)?;if *profile==b.limits{return Ok(true);}}
        }Ok(false)
    })();b.structural=Some(state);result
}
pub(crate) fn stored_type_success(ty:&Type,limits:&Limits,b:&mut Budget)->Check<()>{
    if !cacheable(ty){return Ok(());}
    let Some(state)=&b.structural else{return Ok(());};let ptr=ty as *const Type as usize;
    let fee=lookup(state.types.len());b.spend(fee)?;
    if !b.structural.as_ref().unwrap().types.contains(&ptr){return Ok(());}
    let fee=lookup(b.structural.as_ref().unwrap().stored_bounds.len()).saturating_add(14);b.spend(fee)?;
    let profile=b.limits.clone();b.structural.as_mut().unwrap().stored_bounds.entry((ptr,limits.max_call_depth)).or_default().push(profile);Ok(())
}

// Only complete static Store-element admission is retained. This table has
// no value, owner epoch, address window, cleanup state or Store permission.
pub(crate) fn store_element_known(module:&TypedModule,ty:&Type,product:bool,b:&mut Budget)->Check<bool>{
    let Some(state)=&b.structural else{return Ok(false);};
    if state.module!=module as *const TypedModule as usize{return Ok(false);}
    let fee=lookup(state.store_success.len());b.spend(fee)?;
    let state=b.structural.take().unwrap();
    let result=(||{
        if let Some(profiles)=state.store_success.get(&(ty as *const Type as usize,product,b.nested_store_windows)){
            for limits in profiles {b.spend(12)?;if *limits==b.limits{return Ok(true);}}
        }Ok(false)
    })();
    b.structural=Some(state);result
}
pub(crate) fn store_element_success(module:&TypedModule,ty:&Type,product:bool,b:&mut Budget)->Check<()>{
    let Some(state)=&b.structural else{return Ok(());};
    if state.module!=module as *const TypedModule as usize{return Ok(());}
    let ptr=ty as *const Type as usize;b.spend(lookup(state.types.len()))?;
    if !b.structural.as_ref().unwrap().types.contains(&ptr){return Ok(());}
    // Exclude all late-bound environments and address/κ carriers. Their
    // qualification still runs in its actual request on every occurrence.
    let mut pending=vec![ty];
    while let Some(ty)=pending.pop(){b.spend(1)?;match ty{
        Type::Managed(inner)|Type::Array{element:inner,..}=>{b.spend(1)?;pending.push(inner);},
        Type::Tuple(fields)=>{b.spend(fields.len())?;pending.extend(fields);},
        Type::Record{fields,..}|Type::Package{fields,..}=>{b.spend(fields.len())?;pending.extend(fields.iter().map(|(_,ty)|ty));},
        Type::Enum{variants,..}=>for(_,fields)in variants{b.spend(fields.len().saturating_add(1))?;pending.extend(fields);},
        Type::ErrorUnion{error,result}=>{b.spend(2)?;pending.push(error);pending.push(result);},
        Type::ByteBuffer|Type::ForeignHandle|Type::CWideString|Type::SyncCell(_)=>{},
        ty if ty.is_b0_scalar()=>{},
        _=>return Ok(()),
    }}
    b.spend(lookup(b.structural.as_ref().unwrap().store_success.len()).saturating_add(14))?;
    let key=(ptr,product,b.nested_store_windows);let limits=b.limits.clone();
    b.structural.as_mut().unwrap().store_success.entry(key).or_default().push(limits);Ok(())
}

/// Adopt an already formed immutable physical type. Strong ownership covers
/// every registered child address until this exact module scope is restored.
pub(crate) fn adopt_type_cell(cell:&std::rc::Rc<crate::semantic::CheckedTypeData>,b:&mut Budget)->Check<()>{
    let Some(state)=&b.structural else{return Ok(());};
    let ptr=&cell.ty as *const Type as usize;
    b.spend(lookup(state.type_owners.len()).saturating_add(1))?;
    if b.structural.as_ref().unwrap().type_owners.get(&ptr).is_some_and(|known|std::rc::Rc::ptr_eq(known,cell)){return Ok(());}
    b.spend(lookup(b.structural.as_ref().unwrap().type_owners.len()).saturating_add(2))?;
    let mut state=b.structural.take().unwrap();
    state.type_owners.insert(ptr,cell.clone());
    let result=state.ty(&cell.ty,b);
    b.structural=Some(state);result
}

/// Owned storage for an exact registered immutable input node. This memo owns
/// no class/origin/source result and never recognizes a temporary by address.
pub(crate) fn shared_type(ty:&Type,b:&mut Budget)->Check<Option<std::rc::Rc<crate::semantic::CheckedTypeData>>>{
    let Some(state)=&b.structural else{return Ok(None);};
    let ptr=ty as *const Type as usize;
    let fee=lookup(state.types.len()).saturating_add(lookup(state.shared_types.len())).saturating_add(lookup(state.type_owners.len())).saturating_add(1);
    b.spend(fee)?;
    let state=b.structural.as_ref().unwrap();
    if !state.types.contains(&ptr){return Ok(None);}
    if let Some(shared)=state.type_owners.get(&ptr){return Ok(Some(shared.clone()));}
    if let Some(shared)=state.shared_types.get(&ptr){return Ok(Some(shared.clone()));}
    super::ty(ty,b)?;
    let shared=crate::semantic::CheckedTypeData::new(ty.clone());
    b.spend(lookup(b.structural.as_ref().unwrap().shared_types.len()).saturating_add(lookup(b.structural.as_ref().unwrap().type_owners.len())).saturating_add(4))?;
    let mut state=b.structural.take().unwrap();
    // The scope itself keeps every registered copied node alive. Registering
    // its child pointers permits the same immutable payload directory to be
    // borrowed by resource/metadata consumers, without re-cloning the tree.
    state.shared_types.insert(ptr,shared.clone());
    state.type_owners.insert(&shared.ty as *const Type as usize,shared.clone());
    let result=state.ty(&shared.ty,b);
    b.structural=Some(state);
    result?;
    Ok(Some(shared))
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn bucketed_type_facts_preserve_exact_depth_profiles_and_failed_growth(){
        let mut b=Budget::new(&Limits::default());let mut facts=TypeFacts::new(&mut b).unwrap();let mut expected=BTreeSet::new();
        assert!(!facts.contains(&(0,0,(1_000_000,128)),&mut b).unwrap(),"an allocated empty directory supplies no fact");
        for index in 0..1024usize{let key=(0x1000+index*256,index%7,(1_000_000,128));facts.insert(key,&mut b).unwrap();expected.insert(key);}
        assert_eq!(facts.len(),expected.len());
        for key in &expected{assert!(facts.contains(key,&mut b).unwrap());assert!(!facts.contains(&(key.0,key.1+13,key.2),&mut b).unwrap());assert!(!facts.contains(&(key.0,key.1,(key.2.0-1,key.2.1)),&mut b).unwrap());}
        let count=facts.buckets.len()*2;let words=std::mem::size_of::<BTreeSet<TypeFact>>().div_ceil(std::mem::size_of::<usize>());
        let mut short=Budget::new(&Limits{max_analysis_steps:count*words+facts.buckets.len()+5,..Limits::default()});
        assert!(facts.grow(count,&mut short).is_err());assert_eq!(facts.len(),expected.len());
        for key in &expected{assert!(facts.contains(key,&mut b).unwrap(),"failed reindex cannot lose or invent prior exact facts");}
        // Multiple complete keys at the same pointer are never first-match
        // aliases: all exact depth/profile coordinates remain independent.
        let key=*expected.iter().next().unwrap();facts.insert((key.0,key.1+13,key.2),&mut b).unwrap();assert!(facts.contains(&(key.0,key.1+13,key.2),&mut b).unwrap());assert_eq!(facts.len(),expected.len()+1);
    }
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
    fn store_element_static_success_keeps_mode_limits_module_and_temporary_separate(){
        let mut module=fixture();let owner=&mut module.functions[0].locals.last_mut().unwrap().ty;
        *owner=Type::Package{name:"checked_store_item".into(),fields:vec![("owner".into(),owner.clone()),("data".into(),Type::I32)]};
        let ty=&module.functions[0].locals.last().unwrap().ty;let mut b=budget(&module);
        crate::dynamic_store::check_store_element(&module,ty,None,&mut b).unwrap();
        assert!(store_element_known(&module,ty,true,&mut b).unwrap());
        assert!(!store_element_known(&module,ty,false,&mut b).unwrap());
        let temporary=ty.clone();assert!(!store_element_known(&module,&temporary,true,&mut b).unwrap());
        let other=module.clone();assert!(!store_element_known(&other,ty,true,&mut b).unwrap());
        b.nested_store_windows=!b.nested_store_windows;assert!(!store_element_known(&module,ty,true,&mut b).unwrap());b.nested_store_windows=!b.nested_store_windows;
        let limits=b.limits.clone();b.limits.max_nodes=1;assert!(!store_element_known(&module,ty,true,&mut b).unwrap());
        let actual=crate::dynamic_store::check_store_element(&module,ty,None,&mut b);
        let expected=crate::dynamic_store::check_store_element(&module,ty,None,&mut Budget::new(&b.limits));assert_eq!(actual,expected);
        b.limits=limits;assert!(store_element_known(&module,ty,true,&mut b).unwrap());
    }
    #[test]
    fn stored_bounds_reuse_preserves_depth_errors_temporary_identity_and_full_copy_fee(){
        let module=fixture();let ty=&module.functions[0].locals.last().unwrap().ty;let limits=Limits::default();let mut b=budget(&module);
        let before=b.remaining;crate::stored_flow::type_bounds(ty,&limits,&mut b).unwrap();let cold=before-b.remaining;
        let before=b.remaining;crate::stored_flow::type_bounds(ty,&limits,&mut b).unwrap();let hot=before-b.remaining;assert!(hot<cold);
        let before=b.remaining;crate::stored_flow::type_work(ty,&limits,&mut b).unwrap();assert!(before-b.remaining>hot,"actual Type copy/Eq keeps its complete name-byte prepayment");
        let temporary=ty.clone();assert!(!stored_type_known(&temporary,&limits,&mut b).unwrap());
        let mut shallow=limits.clone();shallow.max_call_depth=0;
        let actual=crate::stored_flow::type_bounds(ty,&shallow,&mut b).unwrap_err();
        let expected=crate::stored_flow::type_work(ty,&shallow,&mut Budget::new(&limits)).unwrap_err();assert_eq!(actual,expected);
        assert!(stored_type_known(ty,&limits,&mut b).unwrap());
        b.limits.max_values-=1;assert!(!stored_type_known(ty,&limits,&mut b).unwrap(),"budget metadata profiles remain separate");
    }
    #[test]
    fn adopting_source_cells_keeps_every_copied_tree_alive(){
        let module=fixture();let root=&module.functions[0].locals.last().unwrap().ty;
        let mut b=budget(&module);
        let first=shared_type(root,&mut b).unwrap().unwrap();
        let next=shared_type(&first.ty,&mut b).unwrap().unwrap();
        let weak=std::rc::Rc::downgrade(&next);
        drop(next);
        adopt_type_cell(&first,&mut b).unwrap();
        assert!(weak.upgrade().is_some(),"adoption cannot drop the last owner of a registered snapshot tree");
        let again=shared_type(&first.ty,&mut b).unwrap().unwrap();
        assert!(std::rc::Rc::ptr_eq(&first,&again),"an owned immutable root is already its own snapshot");
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
