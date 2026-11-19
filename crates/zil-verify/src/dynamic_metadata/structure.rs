//! Immutable-module structural scheduling. No runtime source, class equation,
//! caller premise or permission is a member of this directory.
use super::*;
use std::any::TypeId;

mod physical_pairs;
pub(crate) use physical_pairs::compare_type_cells;

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
fn add_index<K:Ord>(directory:&mut BTreeMap<K,Vec<usize>>,key:K,index:usize,b:&mut Budget)->Check<()>{
    b.spend(lookup(directory.len()).saturating_add(2))?;
    let rows=directory.entry(key).or_default();
    b.spend(1usize.saturating_add(if rows.len()==rows.capacity(){rows.len()}else{0}))?;
    rows.push(index);Ok(())
}
fn index_rows<K:Ord>(directory:&BTreeMap<K,Vec<usize>>,key:&K,b:&mut Budget)->Check<Vec<usize>>{
    b.spend(lookup(directory.len()))?;
    let rows=directory.get(key);b.spend(rows.map_or(0,Vec::len))?;
    Ok(rows.cloned().unwrap_or_default())
}
fn cacheable(ty:&Type)->bool{!matches!(ty,
    Type::ByteBuffer|Type::ForeignHandle|Type::SyncCell(_)|Type::TaskI32|Type::CWideString|Type::Closure(_)|Type::Dyn{..}|Type::OwnedDyn{..}
)&&!ty.is_b0_scalar()}

type TypeFact = (usize, usize, Profile);
const TYPE_FACT_BUCKETS: usize = 64;

// Partition by the immutable outer shape. Each bucket keeps exact address,
// entry depth and limits for every completed physical bounds check.
fn type_fact_bucket(ty: &Type) -> usize {
    fn child_kind(ty: &Type) -> usize {
        match ty {
            Type::Record { .. } | Type::Package { .. } => 0,
            Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. } => 1,
            Type::Address { .. } | Type::Desc(_) | Type::Managed(_) => 2,
            _ => 3,
        }
    }
    let (family, shape) = match ty {
        Type::Address { mutable, inner } => (usize::from(*mutable), child_kind(inner)),
        Type::Desc(inner) => (2, child_kind(inner)),
        Type::Managed(inner) => (3, child_kind(inner)),
        Type::Record { name, fields } => (4, name.len() ^ fields.len()),
        Type::Package { name, fields } => (5, name.len() ^ fields.len()),
        Type::Enum { name, variants } => (6, name.len() ^ variants.len()),
        Type::Tuple(fields) => (7, fields.len()),
        Type::BorrowedStorage { storage, .. } => (8, child_kind(storage)),
        Type::Array { element, .. } => (9, child_kind(element)),
        Type::ErrorUnion { result, .. } => (10, child_kind(result)),
        Type::Task { result, .. } => (11, child_kind(result)),
        Type::FunctionPointer { parameters, .. } | Type::BorrowedFunctionPointer { parameters, .. }
        | Type::ForeignCallback { parameters, .. } => (12, parameters.len()),
        Type::OwnedStore { element } | Type::Slice { element } => (13, child_kind(element)),
        Type::Named(name) | Type::Unsupported(name) => (14, name.len()),
        _ => (15, 0),
    };
    4 * family + (shape % 4)
}

// A scope with no completed Type facts needs no partition directory.
#[derive(Default)]
struct TypeFacts(Option<Box<[BTreeSet<TypeFact>; TYPE_FACT_BUCKETS]>>);
impl TypeFacts {
    #[cfg(test)]
    fn len(&self) -> usize {
        self.0.as_ref().map_or(0, |buckets| buckets.iter().map(BTreeSet::len).sum())
    }
}

type TypeCell = std::rc::Rc<crate::semantic::CheckedTypeData>;
#[derive(Clone)]
struct TypeSnapshot {
    cell: TypeCell,
    bounds: Option<Profile>,
}

pub(crate) struct Structure{
    scope_identity: std::rc::Rc<()>,
    physical_pairs: physical_pairs::Pairs,
    module:usize,
    program:Option<usize>,
    declarations:(usize,usize),
    types:BTreeSet<usize>,
    recipes:BTreeSet<RecipeKey>,
    program_bounds:BTreeSet<Profile>,
    declaration_nodes:BTreeSet<usize>,
    type_success:TypeFacts,
    recipe_success:BTreeSet<(RecipeKey,Profile)>,
    declaration_success:BTreeSet<(usize,Option<SemanticDefinitionId>,Profile)>,
    resource_success:BTreeMap<(usize,Profile),usize>,
    store_success:BTreeMap<(usize,bool,bool),Vec<Limits>>,
    sum_shapes:BTreeMap<(usize,Profile),bool>,
    store_definitions:BTreeSet<usize>,
    // Built-in scalar identity is complete without retaining a temporary Type pointer.
    scalar_store_definitions:BTreeSet<&'static str>,
    store_resource_payloads:BTreeSet<Profile>,
    stored_bounds:BTreeMap<(usize,usize),Vec<Limits>>,
    owners:BTreeMap<(OwnerKey,SemanticDefinitionId),usize>,
    lexical_scopes:BTreeMap<OwnerKey,Vec<SemanticDefinitionId>>,
    dyn_declarations:BTreeMap<LifeDeclarationId,Vec<usize>>,
    trait_providers:Option<BTreeMap<LifeDeclarationId,Vec<(usize,SemanticDefinitionId)>>>,
    nominal_declarations:BTreeMap<LifeDeclarationId,Vec<usize>>,
    borrowed_callback_domains:BTreeMap<ClosureId,u32>,
    nominal_instances:BTreeMap<LifeDeclarationId,Vec<usize>>,
    scope_instances:BTreeMap<SemanticDefinitionId,Vec<usize>>,
    // The original pointer and the copied root can both refer to the same
    // cell. Each copied root holds its own strong reference even if an alias
    // is subsequently replaced by a check of the original owner.
    type_cells:BTreeMap<usize,TypeSnapshot>,
}
impl Structure{
    pub(crate) fn identity_for(&self, module: &TypedModule) -> Option<std::rc::Rc<()>> {
        (self.module == module as *const TypedModule as usize).then(|| self.scope_identity.clone())
    }
    pub(crate) fn borrowed_callback_domain(&self,id:ClosureId)->Option<u32>{
        self.borrowed_callback_domains.get(&id).copied()
    }

    /// The root checker keeps this exact module immutably borrowed until this
    /// directory is removed. Nested module checks install a separate scope.
    pub(crate) fn register(module:&TypedModule,b:&mut Budget)->Check<Self>{
        b.spend(12)?;
        let mut result=Self{
            module:module as *const TypedModule as usize,
            scope_identity: std::rc::Rc::new(()),
            physical_pairs: physical_pairs::Pairs::new(&b.limits),
            program:module.semantic.as_ref().map(|program|program as *const SemanticModule as usize),
            declarations:(module.dyn_declarations.as_ptr() as usize,module.dyn_declarations.len()),
            types:BTreeSet::new(),recipes:BTreeSet::new(),program_bounds:BTreeSet::new(),declaration_nodes:BTreeSet::new(),
            type_success:TypeFacts::default(),recipe_success:BTreeSet::new(),declaration_success:BTreeSet::new(),resource_success:BTreeMap::new(),store_success:BTreeMap::new(),sum_shapes:BTreeMap::new(),store_definitions:BTreeSet::new(),scalar_store_definitions:BTreeSet::new(),store_resource_payloads:BTreeSet::new(),stored_bounds:BTreeMap::new(),owners:BTreeMap::new(),lexical_scopes:BTreeMap::new(),dyn_declarations:BTreeMap::new(),trait_providers:None,nominal_declarations:BTreeMap::new(),nominal_instances:BTreeMap::new(),scope_instances:BTreeMap::new(),type_cells:BTreeMap::new(),borrowed_callback_domains:BTreeMap::new(),
        };
        for function in &module.functions{
            result.ty(&function.return_type,b)?;
            for local in &function.locals{result.ty(&local.ty,b)?;}
            for ty in &function.values{result.ty(ty,b)?;}
            if let Some(instance)=&function.dyn_instance{result.visit(instance,b)?;}
        }
        for (index,closure) in module.closures.iter().enumerate(){
            for ty in closure.captures.iter().chain(&closure.parameters).chain(std::iter::once(&closure.result)){result.ty(ty,b)?;}
            if closure.id.0 as usize==index && closure.kind==CallableKind::Fn
                && let Some(header)=&closure.storage_life
                && header.fixed_binder_count>0
                && module.functions.get(closure.function.0 as usize).is_some_and(|entry|
                    entry.id==closure.function && entry.environment==Some(closure.id)
                        && entry.call_kind==CallKind::Direct && entry.storage_life.as_ref().is_some_and(|row|
                            row.fixed_binder_count==header.fixed_binder_count))
            {
                b.spend(lookup(result.borrowed_callback_domains.len()).saturating_add(1))?;
                result.borrowed_callback_domains.insert(closure.id,header.fixed_binder_count);
            }
        }
        for resource in &module.resources{result.ty(&resource.payload,b)?;}
        for(index,declaration)in module.life_declarations.iter().enumerate(){add_index(&mut result.nominal_declarations,declaration.id,index,b)?;}
        for(index,instance)in module.life_instances.iter().enumerate(){
            add_index(&mut result.nominal_instances,instance.declaration,index,b)?;
            result.ty(&instance.ty,b)?;
            for actual in &instance.arguments{b.spend(1)?;match actual{LifeNominalActual::Type(ty)|LifeNominalActual::QualifiedType{ty,..}=>result.ty(ty,b)?,_=>{}}}
        }
        for (index,declaration) in module.dyn_declarations.iter().enumerate(){
            add_index(&mut result.dyn_declarations,declaration.id,index,b)?;
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
            for(index,instance)in program.instances.iter().enumerate(){
                b.spend(1)?;
                if instance.target==SemanticInstanceTarget::Scope{add_index(&mut result.scope_instances,instance.definition,index,b)?;}
            }
            for binding in &program.dyn_scopes{
                b.spend(lookup(result.lexical_scopes.len()).saturating_add(2))?;
                let rows=result.lexical_scopes.entry(owner_key(&binding.owner)).or_default();
                b.spend(1usize.saturating_add(if rows.len()==rows.capacity(){rows.len()}else{0}))?;
                // Duplicate claims remain duplicate candidates. Registration
                // proves no declaration/method/namespace correspondence.
                rows.push(binding.scope);
            }
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

pub(super) fn type_known(ty: &Type, depth: usize, b: &mut Budget) -> Check<bool> {
    if !cacheable(ty) || b.structural.is_none() { return Ok(false); }
    if b.structural.as_ref().unwrap().type_success.0.is_none() {
        b.spend(1)?;
        return Ok(false);
    }
    b.spend(2)?;
    let bucket = type_fact_bucket(ty);
    let fee = lookup(b.structural.as_ref().unwrap().type_success.0.as_ref().unwrap()[bucket].len());
    b.spend(fee)?;
    let key = (ty as *const Type as usize, depth, profile(b));
    let buckets = b.structural.as_ref().unwrap().type_success.0.as_ref().unwrap();
    Ok(buckets[bucket].contains(&key))
}

pub(super) fn type_success(ty: &Type, depth: usize, b: &mut Budget) -> Check<()> {
    if !cacheable(ty) { return Ok(()); }
    let Some(state) = &b.structural else { return Ok(()); };
    let ptr = ty as *const Type as usize;
    b.spend(lookup(state.types.len()))?;
    if !b.structural.as_ref().unwrap().types.contains(&ptr) { return Ok(()); }
    b.spend(2)?;
    let bucket = type_fact_bucket(ty);
    let directory = &b.structural.as_ref().unwrap().type_success.0;
    let fee = directory.as_ref().map_or(TYPE_FACT_BUCKETS + 2, |buckets| lookup(buckets[bucket].len()));
    b.spend(fee.saturating_add(2))?;
    let key = (ptr, depth, profile(b));
    let directory = &mut b.structural.as_mut().unwrap().type_success.0;
    let buckets = directory.get_or_insert_with(|| Box::new(std::array::from_fn(|_| BTreeSet::new())));
    buckets[bucket].insert(key);
    Ok(())
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

/// Bound the complete original program once for purely structural readers.
/// A caller that clones the program or compares a body still pays that work.
pub(crate) fn source_program_bounds(program: &SemanticModule, b: &mut Budget) -> Check<()> {
    let registered = b.structural.as_ref().is_some_and(|state| state.program == Some(program as *const _ as usize));
    let key = profile(b);
    if registered {
        b.spend(lookup(b.structural.as_ref().unwrap().program_bounds.len()))?;
        if b.structural.as_ref().unwrap().program_bounds.contains(&key) {
            return Ok(());
        }
    }
    super::recipe(program, b)?;
    if registered {
        b.spend(lookup(b.structural.as_ref().unwrap().program_bounds.len()).saturating_add(2))?;
        b.structural.as_mut().unwrap().program_bounds.insert(key);
    }
    Ok(())
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
/// All original scope claims for this exact owner and immutable module.
/// The consumer still proves the complete owner, mixed formals and actual Γ.
pub(crate) fn lexical_scopes(module:&TypedModule,owner:&SemanticDynOwner,b:&mut Budget)->Check<Option<Vec<SemanticDefinitionId>>>{
    let Some(program)=&module.semantic else{return Ok(None);};
    program_lexical_scopes(program,&module.dyn_declarations,owner,b)
}
pub(crate) fn program_lexical_scopes(program:&SemanticModule,declarations:&[DynDeclaration],owner:&SemanticDynOwner,b:&mut Budget)->Check<Option<Vec<SemanticDefinitionId>>>{
    let Some(state)=&b.structural else{return Ok(None);};
    if state.program!=Some(program as *const SemanticModule as usize)||state.declarations!=(declarations.as_ptr() as usize,declarations.len()){return Ok(None);}
    let fee=lookup(state.lexical_scopes.len());b.spend(fee)?;
    let state=b.structural.take().unwrap();
    let result=(||{let rows=state.lexical_scopes.get(&owner_key(owner));b.spend(rows.map_or(0,Vec::len))?;Ok(Some(rows.cloned().unwrap_or_default()))})();
    b.structural=Some(state);result
}
pub(crate) fn dyn_declarations<'a>(module:&'a TypedModule,id:LifeDeclarationId,b:&mut Budget)->Check<Vec<&'a DynDeclaration>>{
    let rows:Vec<usize>=if let Some(rows)=registered_rows(module,b,|state,b|index_rows(&state.dyn_declarations,&id,b))?{rows}else{
        b.spend(module.dyn_declarations.len())?;
        module.dyn_declarations.iter().enumerate().filter_map(|(index,row)|(row.id==id).then_some(index)).collect()
    };
    b.spend(rows.len())?;
    Ok(rows.into_iter().map(|index|&module.dyn_declarations[index]).collect())
}

/// Reuse the exact checked owner role of a declaration. Selection and
/// instantiation still run against every matching original implementation.
pub(crate) fn trait_providers(
    module:&TypedModule,
    declaration:LifeDeclarationId,
    b:&mut Budget,
)->Check<Vec<(usize,SemanticDefinitionId)>>{
    let registered=b.structural.as_ref().is_some_and(|state|state.module==module as *const TypedModule as usize);
    if registered&&b.structural.as_ref().unwrap().trait_providers.is_none(){
        let mut directory=BTreeMap::<_,Vec<_>>::new();
        for (index,id,application) in provider_scopes(module,b)?{
            b.spend(lookup(directory.len()).saturating_add(2))?;
            let rows=directory.entry(application).or_default();
            b.spend(1usize.saturating_add(if rows.len()==rows.capacity(){rows.len()}else{0}))?;
            rows.push((index,id));
        }
        b.structural.as_mut().unwrap().trait_providers=Some(directory);
    }
    if registered{
        let state=b.structural.take().unwrap();
        let result=(||{
            let directory=state.trait_providers.as_ref().unwrap();
            b.spend(lookup(directory.len()))?;
            let rows=directory.get(&declaration);
            b.spend(rows.map_or(0,Vec::len))?;
            Ok(rows.cloned().unwrap_or_default())
        })();
        b.structural=Some(state);
        result
    }else{
        let providers=provider_scopes(module,b)?;
        b.spend(providers.len())?;
        Ok(providers.into_iter().filter_map(|(index,id,application)|(application==declaration).then_some((index,id))).collect())
    }
}

fn provider_scopes(
    module:&TypedModule,
    b:&mut Budget,
)->Check<Vec<(usize,SemanticDefinitionId,LifeDeclarationId)>>{
    b.spend(module.dyn_declarations.len())?;
    let mut providers=Vec::new();
    for (index,implementation) in module.dyn_declarations.iter().enumerate(){
        if implementation.kind!=DynDeclarationKind::Impl{continue;}
        let definition=crate::dynamic_witness::semantic_external::original_scope(module,implementation.id,b)?;
        if let SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl{trait_application,..})=&definition.body{
            if providers.len()>=b.limits.max_nodes{return Err(limit("original provider directory exceeds selected node bound"));}
            b.spend(1usize.saturating_add(if providers.len()==providers.capacity(){providers.len()}else{0}))?;
            providers.push((index,definition.id,trait_application.declaration));
        }
    }
    Ok(providers)
}
fn registered_rows(module:&TypedModule,b:&mut Budget,select:impl FnOnce(&Structure,&mut Budget)->Check<Vec<usize>>)->Check<Option<Vec<usize>>>{
    let Some(state)=&b.structural else{return Ok(None);};
    if state.module!=module as *const TypedModule as usize{return Ok(None);}
    let state=b.structural.take().unwrap();
    let result=select(&state,b).map(Some);b.structural=Some(state);result
}
/// Immutable candidate selection only: every matching row, in original order.
/// The consumer still proves each full mixed actual, domain and physical value.
pub(crate) fn nominal_declarations<'m>(module:&'m TypedModule,id:LifeDeclarationId,b:&mut Budget)->Check<Vec<&'m LifeNominalDeclaration>>{
    let rows=if let Some(rows)=registered_rows(module,b,|state,b|index_rows(&state.nominal_declarations,&id,b))?{rows}else{
        b.spend(module.life_declarations.len())?;module.life_declarations.iter().enumerate().filter_map(|(index,row)|(row.id==id).then_some(index)).collect()
    };
    b.spend(rows.len())?;Ok(rows.into_iter().map(|index|&module.life_declarations[index]).collect())
}
pub(crate) fn nominal_instances<'m>(module:&'m TypedModule,id:LifeDeclarationId,b:&mut Budget)->Check<Vec<&'m LifeNominalInstance>>{
    let rows=if let Some(rows)=registered_rows(module,b,|state,b|index_rows(&state.nominal_instances,&id,b))?{rows}else{
        b.spend(module.life_instances.len())?;module.life_instances.iter().enumerate().filter_map(|(index,row)|(row.declaration==id).then_some(index)).collect()
    };
    b.spend(rows.len())?;Ok(rows.into_iter().map(|index|&module.life_instances[index]).collect())
}
pub(crate) fn nominal_scope_instances<'m>(module:&'m TypedModule,id:SemanticDefinitionId,b:&mut Budget)->Check<Vec<&'m SemanticInstance>>{
    let Some(program)=&module.semantic else{return Ok(vec![]);};
    let rows=if let Some(rows)=registered_rows(module,b,|state,b|index_rows(&state.scope_instances,&id,b))?{rows}else{
        b.spend(program.instances.len())?;program.instances.iter().enumerate().filter_map(|(index,row)|(row.definition==id&&row.target==SemanticInstanceTarget::Scope).then_some(index)).collect()
    };
    b.spend(rows.len())?;Ok(rows.into_iter().map(|index|&program.instances[index]).collect())
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

/// This fact is confined to a registered immutable root in this request. It
/// determines which structural checker to run while all value/tag checks
/// still run on their actual inputs.
pub(crate) fn sum_shape_known(ty: &Type, b: &mut Budget) -> Check<Option<bool>> {
    if !cacheable(ty) { return Ok(None); }
    let Some(state) = &b.structural else { return Ok(None); };
    b.spend(lookup(state.sum_shapes.len()))?;
    let ptr = ty as *const Type as usize;
    Ok(b.structural.as_ref().unwrap().sum_shapes.get(&(ptr, profile(b))).copied())
}

pub(crate) fn sum_shape_success(ty: &Type, shape: bool, b: &mut Budget) -> Check<()> {
    if !cacheable(ty) { return Ok(()); }
    let Some(state) = &b.structural else { return Ok(()); };
    b.spend(lookup(state.types.len()))?;
    let state = b.structural.as_ref().unwrap();
    let ptr = ty as *const Type as usize;
    if state.types.contains(&ptr) {
        b.spend(lookup(state.sum_shapes.len()).saturating_add(2))?;
        let key = (ptr, profile(b));
        b.structural.as_mut().unwrap().sum_shapes.insert(key, shape);
    }
    Ok(())
}

/// The only shared fact here is the canonical Store declaration's
/// exact name, payload equality, and absence of a custom teardown. Element
/// qualification still executes at its call site, under its current profile.
pub(crate) fn store_definition_known(module: &TypedModule, element: &Type, b: &mut Budget) -> Check<bool> {
    let Some(state) = &b.structural else { return Ok(false); };
    if state.module != module as *const TypedModule as usize { return Ok(false); }
    if let Some(scalar) = crate::dynamic_store::scalar_name(element) {
        b.spend(lookup(state.scalar_store_definitions.len()).saturating_mul(scalar.len() + 1))?;
        return Ok(b.structural.as_ref().unwrap().scalar_store_definitions.contains(scalar));
    }
    b.spend(lookup(state.store_definitions.len()))?;
    Ok(b.structural.as_ref().unwrap().store_definitions.contains(&(element as *const Type as usize)))
}

pub(crate) fn store_definition_success(module: &TypedModule, element: &Type, b: &mut Budget) -> Check<()> {
    let Some(state) = &b.structural else { return Ok(()); };
    if state.module != module as *const TypedModule as usize { return Ok(()); }
    if let Some(scalar) = crate::dynamic_store::scalar_name(element) {
        b.spend(lookup(state.scalar_store_definitions.len()).saturating_mul(scalar.len() + 1).saturating_add(2))?;
        b.structural.as_mut().unwrap().scalar_store_definitions.insert(scalar);
        return Ok(());
    }
    b.spend(lookup(state.types.len()))?;
    let state = b.structural.as_ref().unwrap();
    let ptr = element as *const Type as usize;
    if state.types.contains(&ptr) {
        b.spend(lookup(state.store_definitions.len()).saturating_add(2))?;
        b.structural.as_mut().unwrap().store_definitions.insert(ptr);
    }
    Ok(())
}

/// POD carrier admission also checks the resource inventory's closed Task
/// context. The live module and shape caps bind this separate structural fact.
pub(crate) fn check_store_resource_payloads(module: &TypedModule, b: &mut Budget) -> Check<()> {
    let registered = b.structural.as_ref().is_some_and(|state| state.module == module as *const TypedModule as usize);
    let key = profile(b);
    if registered {
        let state = b.structural.as_ref().unwrap();
        b.spend(lookup(state.store_resource_payloads.len()))?;
        if b.structural.as_ref().unwrap().store_resource_payloads.contains(&key) {
            return Ok(());
        }
    }
    b.spend(module.resources.len())?;
    for resource in &module.resources {
        crate::resources::charge_payload(&resource.payload, b)?;
    }
    if registered {
        b.spend(lookup(b.structural.as_ref().unwrap().store_resource_payloads.len()).saturating_add(2))?;
        b.structural.as_mut().unwrap().store_resource_payloads.insert(key);
    }
    Ok(())
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

/// Adopt an already formed immutable physical type. Hold the root before
/// registration so any child address entered before a budget error stays live.
pub(crate) fn adopt_type_cell(cell: &TypeCell, b: &mut Budget) -> Check<()> {
    adopt_type_snapshot(cell, None, b)
}

pub(super) fn adopt_checked_type_cell(cell: &TypeCell, b: &mut Budget) -> Check<()> {
    adopt_type_snapshot(cell, Some(profile(b)), b)?;
    if !super::inexpensive_bounds(&cell.ty) {
        type_success(&cell.ty, 0, b)?;
    }
    Ok(())
}

fn adopt_type_snapshot(cell: &TypeCell, bounds: Option<Profile>, b: &mut Budget) -> Check<()> {
    let Some(state) = &b.structural else {
        return Ok(());
    };
    let fee = lookup(state.type_cells.len());
    let ptr = &cell.ty as *const Type as usize;
    b.spend(fee.saturating_add(1))?;
    if b.structural.as_ref().unwrap().type_cells.get(&ptr)
        .is_some_and(|known| std::rc::Rc::ptr_eq(&known.cell, cell))
    {
        return Ok(());
    }
    b.spend(fee.saturating_add(2))?;
    let mut state = b.structural.take().unwrap();
    state.type_cells.insert(ptr, TypeSnapshot { cell: cell.clone(), bounds });
    let result = state.ty(&cell.ty, b);
    b.structural = Some(state);
    result
}

/// Owned storage for an exact registered immutable input node. Both the
/// borrowed root and every copied root stay live in this same module scope.
pub(super) fn shared_type(ty: &Type, b: &mut Budget) -> Check<Option<TypeCell>> {
    let Some(state) = &b.structural else {
        return Ok(None);
    };
    if !cacheable(ty) {
        return Ok(None);
    }
    let ptr = ty as *const Type as usize;
    b.spend(lookup(state.type_cells.len()).saturating_add(1))?;
    if let Some(shared) = b.structural.as_ref().unwrap().type_cells.get(&ptr).cloned() {
        if shared.bounds != Some(profile(b)) {
            super::bounds_type(ty, b)?;
        }
        return Ok(Some(shared.cell));
    }
    b.spend(lookup(b.structural.as_ref().unwrap().types.len()))?;
    if !b.structural.as_ref().unwrap().types.contains(&ptr) {
        return Ok(None);
    }
    let hash = match physical_pairs::representative(ty, b)? {
        physical_pairs::Representative::Existing(cell) => {
            let bounds = Some(profile(b));
            let fee = lookup(b.structural.as_ref().unwrap().type_cells.len());
            b.spend(fee.saturating_add(2))?;
            b.structural.as_mut().unwrap().type_cells.insert(ptr, TypeSnapshot {cell: cell.clone(), bounds});
            return Ok(Some(cell));
        }
        physical_pairs::Representative::New(hash) => hash,
    };
    // The copy traversal includes the same complete depth and node bounds.
    super::ty(ty, b)?;
    let shared = crate::semantic::CheckedTypeData::new(ty.clone());
    // The original and its exact clone both passed the same physical walk.
    let snapshot = TypeSnapshot { cell: shared.clone(), bounds: Some(profile(b)) };
    let fee = lookup(b.structural.as_ref().unwrap().type_cells.len());
    b.spend(fee.saturating_mul(2).saturating_add(4))?;
    let mut state = b.structural.take().unwrap();
    state.type_cells.insert(&shared.ty as *const Type as usize, snapshot.clone());
    let result = state.ty(&shared.ty, b);
    if result.is_ok() {
        state.type_cells.insert(ptr, snapshot);
    }
    b.structural = Some(state);
    result?;
    physical_pairs::publish_representative(hash, &shared, b)?;
    Ok(Some(shared))
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn exact_type_facts_keep_depth_profiles_and_allocation_independent_work() {
        let mut module = fixture();
        let start = module.functions[0].locals.len();
        for index in 0..128 {
            let function = &mut module.functions[0];
            let ty = match index % 3 {
                0 => Type::Managed(Box::new(record())),
                1 => Type::Tuple(vec![record(), Type::I32]),
                _ => Type::Address { mutable: true, inner: Box::new(record()) },
            };
            function.locals.push(LocalDecl {
                id: LocalId(function.locals.len() as u32), name: format!("root_{index}"),
                ty, mutable: false, span: function.span,
            });
        }
        let copy = module.clone();
        let mut first = budget(&module);
        let mut other = budget(&copy);
        let originals = &module.functions[0].locals[start..];
        let clones = &copy.functions[0].locals[start..];
        for (index, (original, cloned)) in originals.iter().zip(clones).enumerate() {
            let depth = index % 7;
            for (b, root, foreign) in [(&mut first, &original.ty, &cloned.ty), (&mut other, &cloned.ty, &original.ty)] {
                assert!(!type_known(foreign, depth, b).unwrap());
                assert!(!type_known(root, depth, b).unwrap());
                super::super::type_bounds_at(root, depth, b).unwrap();
                assert!(type_known(root, depth, b).unwrap());
                assert!(!type_known(root, depth + 1, b).unwrap());
                b.limits.max_nodes -= 1;
                assert!(!type_known(root, depth, b).unwrap());
                b.limits.max_nodes += 1;
            }
            assert_eq!(first.remaining, other.remaining);
        }
        let root = &originals[0].ty;
        let entries = first.structural.as_ref().unwrap().type_success.len();
        let before = first.remaining;
        first.remaining = 0;
        assert!(type_success(root, 1, &mut first).is_err());
        assert_eq!(first.structural.as_ref().unwrap().type_success.len(), entries);
        first.remaining = before;
        super::super::type_bounds_at(root, 1, &mut first).unwrap();
        assert!(type_known(root, 1, &mut first).unwrap());
        assert_eq!(first.structural.as_ref().unwrap().type_success.len(), entries + 1);
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
    fn lexical_scope_directory_keeps_owner_kinds_namespaces_and_duplicate_claims(){
        let mut module=fixture();
        let declaration=LifeDeclarationId{module:zil_base::ModuleId(7),declaration:zil_base::NodeId(9),compiler:false};
        let scope=|index|SemanticDefinitionId{module:zil_base::ModuleId(7),index};
        let owners=[
            SemanticDynOwner::Declaration(declaration),
            SemanticDynOwner::Declaration(LifeDeclarationId{compiler:true,..declaration}),
            SemanticDynOwner::MethodSignature{declaration,member:0},
            SemanticDynOwner::MethodDefinition(DynDefinitionId{module:declaration.module,source:declaration.declaration}),
            SemanticDynOwner::Declaration(LifeDeclarationId{module:zil_base::ModuleId(8),..declaration}),
        ];
        let mut rows:Vec<_>=owners.iter().enumerate().map(|(index,owner)|SemanticDynScopeBinding{owner:owner.clone(),scope:scope(index as u32)}).collect();
        rows.push(SemanticDynScopeBinding{owner:owners[0].clone(),scope:scope(11)});
        module.semantic=Some(SemanticModule{definitions:vec![],instances:vec![],sites:vec![],dyn_scopes:rows});
        let mut b=budget(&module);
        for(index,owner)in owners.iter().enumerate(){
            let expected=if index==0{vec![scope(0),scope(11)]}else{vec![scope(index as u32)]};
            assert_eq!(lexical_scopes(&module,owner,&mut b).unwrap(),Some(expected));
        }
        let missing=SemanticDynOwner::MethodSignature{declaration,member:1};
        assert_eq!(lexical_scopes(&module,&missing,&mut b).unwrap(),Some(vec![]));
        let copy=module.clone();
        assert_eq!(lexical_scopes(&copy,&owners[0],&mut b).unwrap(),None);
        assert_eq!(lexical_scopes(&module,&owners[0],&mut Budget::new(&Limits::default())).unwrap(),None);
        b.remaining=lookup(b.structural.as_ref().unwrap().lexical_scopes.len());
        assert!(lexical_scopes(&module,&owners[0],&mut b).is_err());
        assert!(b.structural.is_some(),"failed row copying restores the enclosing immutable directory");
        b.remaining=Limits::default().max_analysis_steps;
        assert_eq!(lexical_scopes(&module,&owners[0],&mut b).unwrap(),Some(vec![scope(0),scope(11)]));
        // Rows are merely untrusted claims; registration does not synthesize
        // an original definition or choose the first duplicate as authority.
        assert!(module.semantic.as_ref().unwrap().definitions.is_empty());
    }
    #[test]
    fn nominal_candidate_directories_preserve_order_duplicates_and_exact_identity(){
        let mut module=fixture();
        let id=LifeDeclarationId{module:zil_base::ModuleId(7),declaration:zil_base::NodeId(9),compiler:false};
        let other=LifeDeclarationId{compiler:true,..id};
        let declaration=LifeNominalDeclaration{id,identity:LifeDeclarationIdentity{package:"candidate_directory".into(),module_path:vec![],declaration_path:vec![],generation_path:vec![]},kind:LifeNominalKind::Record,parameters:vec![LifeParameterKind::Type]};
        module.life_declarations=vec![declaration.clone(),LifeNominalDeclaration{id:other,..declaration.clone()},declaration];
        module.life_instances=vec![LifeNominalInstance{declaration:id,ty:Type::I32,arguments:vec![]},LifeNominalInstance{declaration:other,ty:Type::Bool,arguments:vec![]},LifeNominalInstance{declaration:id,ty:Type::I64,arguments:vec![]}];
        let definition=SemanticDefinitionId{module:id.module,index:4};
        let scope=SemanticInstance{id:SemanticInstanceId{module:id.module,index:0},definition,target:SemanticInstanceTarget::Scope,lifetime_instantiation:SemanticLifetimeInstantiation::Rigid,parent:None,actuals:vec![],header_embedding:vec![],environment:None,self_type:None};
        module.semantic=Some(SemanticModule{definitions:vec![],sites:vec![],dyn_scopes:vec![],instances:vec![scope.clone(),SemanticInstance{target:SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(FunctionId(0))),..scope.clone()},scope]});
        let mut b=budget(&module);
        for active in [&module,&module.clone()]{
            let rows=nominal_declarations(active,id,&mut b).unwrap();assert_eq!(rows.len(),2);assert!(std::ptr::eq(rows[0],&active.life_declarations[0]));assert!(std::ptr::eq(rows[1],&active.life_declarations[2]));
            let rows=nominal_instances(active,id,&mut b).unwrap();assert_eq!(rows.iter().map(|row|&row.ty).collect::<Vec<_>>(),vec![&Type::I32,&Type::I64]);
            let rows=nominal_scope_instances(active,definition,&mut b).unwrap();assert_eq!(rows.len(),2);assert!(std::ptr::eq(rows[0],&active.semantic.as_ref().unwrap().instances[0]));assert!(std::ptr::eq(rows[1],&active.semantic.as_ref().unwrap().instances[2]));
            assert!(nominal_scope_instances(active,SemanticDefinitionId{module:zil_base::ModuleId(8),..definition},&mut b).unwrap().is_empty());
            assert_eq!(nominal_declarations(active,other,&mut b).unwrap().len(),1);
        }
        b.remaining=0;assert!(nominal_instances(&module,id,&mut b).is_err());assert!(b.structural.is_some());
        // These deliberately duplicate and unresolved claims are just candidate
        // rows. No declaration, actual argument or nominal proof was cached.
        assert!(module.semantic.as_ref().unwrap().definitions.is_empty());
    }
    #[test]
    fn inexpensive_bounds_keep_full_cold_walk_quotas_work_and_no_published_fact(){
        let mut module=fixture();
        let shapes=vec![
            Type::Address{mutable:false,inner:Box::new(Type::I32)},
            Type::Desc(Box::new(Type::ByteBuffer)),
            Type::Array{element:Box::new(Type::Bool),length:7},
            Type::Record{name:"complete_name".repeat(64),fields:vec![("complete_field".repeat(32),Type::I32)]},
            Type::Tuple(vec![Type::I32,Type::U32]),
            Type::ErrorUnion{error:Box::new(Type::Unit),result:Box::new(Type::I64)},
            Type::FunctionPointer{parameters:vec![Type::I32],result:Box::new(Type::Bool)},
        ];
        let start=module.functions[0].locals.len();
        for ty in shapes{let f=&mut module.functions[0];f.locals.push(LocalDecl{id:LocalId(f.locals.len() as u32),name:"cheap_complete_tree".into(),ty,mutable:false,span:f.span});}
        let mut b=budget(&module);
        for local in &module.functions[0].locals[start..]{
            assert!(super::super::inexpensive_bounds(&local.ty));
            for (nodes,depth_cap) in [(Limits::default().max_nodes,128),(0,128),(1,128),(Limits::default().max_nodes,0),(Limits::default().max_nodes,1)]{
                for depth in [0,1]{
                    b.limits.max_nodes=nodes;b.limits.max_call_depth=depth_cap;
                    let before=b.remaining;let entries=b.structural.as_ref().unwrap().type_success.len();
                    let actual=super::super::type_bounds_at(&local.ty,depth,&mut b);
                    let mut cold=Budget::new(&b.limits);let cold_before=cold.remaining;
                    let expected=super::super::type_walk(&local.ty,depth,false,&mut cold);
                    assert_eq!(actual,expected);assert_eq!(before-b.remaining,cold_before-cold.remaining);
                    assert_eq!(entries,b.structural.as_ref().unwrap().type_success.len());
                }
            }
        }
        b.limits=Limits::default();
        let named=&module.functions[0].locals[start+3].ty;
        let before=b.remaining;super::super::type_bounds_at(named,0,&mut b).unwrap();let bounds=before-b.remaining;
        let before=b.remaining;super::super::ty(named,&mut b).unwrap();
        assert!(before-b.remaining>bounds,"real copies and equality retain the complete name-byte traversal");
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
    fn validated_type_cell_checks_current_limits_and_pays_full_temporary_copy() {
        let module = fixture();
        let root = &module.functions[0].locals.last().unwrap().ty;
        let mut b = budget(&module);
        let cached = super::super::validated_type_cell(root, &mut b).unwrap();
        let same = super::super::validated_type_cell(&cached.ty, &mut b).unwrap();
        assert!(std::rc::Rc::ptr_eq(&cached, &same));
        let temporary = root.clone();
        let copied = super::super::validated_type_cell(&temporary, &mut b).unwrap();
        assert_eq!(copied.ty, cached.ty);
        assert!(!std::rc::Rc::ptr_eq(&cached, &copied));

        for (max_nodes, max_call_depth) in [(1, 128), (Limits::default().max_nodes, 0)] {
            b.limits.max_nodes = max_nodes;
            b.limits.max_call_depth = max_call_depth;
            for actual in [root, &cached.ty, &temporary] {
                let expected = super::super::ty(actual, &mut Budget::new(&b.limits)).unwrap_err();
                let hot = super::super::validated_type_cell(actual, &mut b).map(|_| ()).unwrap_err();
                assert_eq!(hot, expected);
            }
        }
        let limits = Limits::default();
        let mut cold = Budget::new(&limits);
        super::super::ty(&temporary, &mut cold).unwrap();
        let used = limits.max_analysis_steps - cold.remaining;
        let mut exact = Budget::new(&Limits { max_analysis_steps: used, ..limits.clone() });
        super::super::validated_type_cell(&temporary, &mut exact).unwrap();
        assert_eq!(exact.remaining, 0);
        let mut short = Budget::new(&Limits { max_analysis_steps: used - 1, ..limits });
        assert!(super::super::validated_type_cell(&temporary, &mut short).is_err());
    }
    #[test]
    fn owned_type_adoption_checks_before_publication_and_keeps_exact_budget() {
        let module = fixture();
        let root = &module.functions[0].locals.last().unwrap().ty;
        let adopt = |steps| {
            let mut b = budget(&module);
            b.remaining = steps;
            let owned = super::super::adopt_checked_type(root.clone(), &mut b)?;
            let shared = super::super::validated_type_cell(&owned.ty, &mut b)?;
            assert!(std::rc::Rc::ptr_eq(&owned, &shared));
            Ok(b.remaining)
        };
        let steps = 1_000_000;
        let used = steps - adopt(steps).unwrap();
        assert_eq!(adopt(used).unwrap(), 0);
        let short: Check<_> = adopt(used - 1);
        assert_eq!(short.unwrap_err().category, zil_base::ResultCategory::ResourceLimit);

        let mut b = budget(&module);
        let raw = crate::semantic::CheckedTypeData::new(root.clone());
        adopt_type_cell(&raw, &mut b).unwrap();
        let entries = b.structural.as_ref().unwrap().type_cells.len();
        b.limits.max_call_depth = 0;
        let expected = super::super::bounds_type(root, &mut Budget::new(&b.limits)).unwrap_err();
        let invalid = super::super::adopt_checked_type(root.clone(), &mut b).map(|_| ());
        assert_eq!(invalid.unwrap_err(), expected);
        assert_eq!(b.structural.as_ref().unwrap().type_cells.len(), entries);
        let unchecked = super::super::validated_type_cell(&raw.ty, &mut b).map(|_| ());
        assert_eq!(unchecked.unwrap_err(), expected);
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
        super::super::type_bounds_at(&temporary,0,&mut b).unwrap();let actual=before-b.remaining;
        let mut cold_walk=Budget::new(&b.limits);let cold_before=cold_walk.remaining;
        super::super::type_walk(&temporary,0,false,&mut cold_walk).unwrap();
        // A temporary executes the complete cold walker. Its two real index
        // lookups cannot publish a success for this unregistered allocation.
        let directory=&b.structural.as_ref().unwrap();
        let bucket=type_fact_bucket(&temporary);
        let lookups=2usize.saturating_add(lookup(directory.type_success.0.as_ref().unwrap()[bucket].len())).saturating_add(lookup(directory.types.len()));
        assert_eq!(actual,cold_before-cold_walk.remaining+lookups);
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
