//! Bounded raw dyn metadata traversal before hashing or recursive comparisons.
//! This grants no Context, port, cleanup or return-source authority.
use super::*;
mod structure;
pub(crate) use structure::{Structure,owner_known,owner_success,resource_known,resource_success,store_element_known,store_element_success,shared_type,adopt_type_cell};
fn pay(b: &mut Budget, nodes: &mut usize, cap: usize, n: usize) -> Check<()> {
    *nodes = nodes
        .checked_add(n)
        .ok_or_else(|| limit("dyn metadata node count overflow"))?;
    if *nodes > cap {
        return Err(limit("dyn metadata node count exceeded"));
    }
    b.spend(n)
}
fn path(b: &mut Budget, nodes: &mut usize, cap: usize, depth_cap: usize, n: usize) -> Check<()> {
    if n > depth_cap || n > cap {
        return Err(limit("dyn metadata path limit exceeded"));
    }
    pay(b, nodes, cap, n.saturating_add(1))
}
fn storage(
    b: &mut Budget,
    nodes: &mut usize,
    cap: usize,
    depth_cap: usize,
    c: &StorageLifeContract,
) -> Check<()> {
    pay(b, nodes, cap, 2)?;
    for row in &c.bindings {
        path(b, nodes, cap, depth_cap, row.position.path.len())?;
    }
    for position in &c.static_bindings {
        path(b, nodes, cap, depth_cap, position.path.len())?;
    }
    for row in &c.classes {
        path(b, nodes, cap, depth_cap, row.position.path.len())?;
    }
    Ok(())
}
fn retained(
    b: &mut Budget,
    nodes: &mut usize,
    cap: usize,
    depth_cap: usize,
    c: &RetainedLifeShape,
) -> Check<()> {
    pay(b, nodes, cap, c.ports.len().saturating_add(2))?;
    for row in &c.retained {
        path(b, nodes, cap, depth_cap, row.path.len())?;
    }
    for row in &c.nominal_arguments {
        path(b, nodes, cap, depth_cap, row.path.len())?;
    }
    Ok(())
}
#[track_caller]
pub(super) fn ty(root: &Type, b: &mut Budget) -> Check<()> {
    let previous=b.diagnostic_metadata_caller;
    if b.diagnostic_costs.is_some(){b.diagnostic_metadata_caller=Some(std::panic::Location::caller());}
    let result=ty_at(root, 0, b);
    b.diagnostic_metadata_caller=previous;
    result
}
fn ty_at(root: &Type, initial_depth: usize, b: &mut Budget) -> Check<()> {
    type_walk(root,initial_depth,true,b)
}
fn type_walk(root:&Type,initial_depth:usize,copy_or_compare:bool,b:&mut Budget)->Check<()>{
    let cap = b.limits.max_nodes;
    let depth_cap = b.limits.max_call_depth.min(128);
    pay(b, &mut 0usize, cap, 1)?;
    let mut nodes = 0usize;
    let mut pending = std::collections::VecDeque::new();
    pending.push_back((root, initial_depth));
    while let Some((ty, depth)) = pending.pop_front() {
        pay(b, &mut nodes, cap, 1)?;
        if depth > depth_cap {
            return Err(limit("dyn metadata type depth exceeded"));
        }
        let mut children = Vec::new();
        match ty {
            Type::BorrowedStorage { storage, life } => {
                pay(b, &mut nodes, cap, life.slots.len().saturating_add(1))?;
                for row in &life.slots {
                    path(b, &mut nodes, cap, depth_cap, row.path.len())?;
                }
                for row in &life.retained {
                    path(b, &mut nodes, cap, depth_cap, row.path.len())?;
                }
                for row in &life.nominal_arguments {
                    path(b, &mut nodes, cap, depth_cap, row.path.len())?;
                }
                children.push(storage.as_ref());
            }
            Type::Address { inner, .. }
            | Type::Managed(inner)
            | Type::Desc(inner)
            | Type::Slice { element: inner }
            | Type::Array { element: inner, .. }
            | Type::OwnedStore { element: inner }
            | Type::Mutex { payload: inner }
            | Type::Registration { environment: inner }
            | Type::AsyncRegistration { environment: inner }
            | Type::Opaque { hidden: inner, .. } => children.push(inner.as_ref()),
            Type::Task {
                result,
                retained_life,
            }
            | Type::MutexGuard {
                payload: result,
                retained_life,
            } => {
                retained(b, &mut nodes, cap, depth_cap, retained_life)?;
                if retained_life.ports.len() > cap {
                    return Err(limit("dyn metadata port count exceeded"));
                }
                b.spend(retained_life.ports.len())?;
                children.extend(retained_life.ports.iter().map(|p| &p.leaf));
                children.push(result.as_ref());
            }
            Type::Record { name, fields } | Type::Package { name, fields } => {
                b.spend(if copy_or_compare{name.len()}else{1})?;
                pay(b, &mut nodes, cap, fields.len())?;
                for (name, ty) in fields {
                    b.spend(if copy_or_compare{name.len()}else{1})?;
                    children.push(ty);
                }
            }
            Type::Tuple(fields) => {
                pay(b, &mut nodes, cap, fields.len())?;
                children.extend(fields);
            }
            Type::Enum { name, variants } => {
                b.spend(if copy_or_compare{name.len()}else{1})?;
                pay(b, &mut nodes, cap, variants.len())?;
                for (name, fields) in variants {
                    b.spend(if copy_or_compare{name.len()}else{1})?;
                    pay(b, &mut nodes, cap, fields.len())?;
                    children.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                children.push(error.as_ref());
                children.push(result.as_ref());
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                pay(b, &mut nodes, cap, parameters.len())?;
                children.extend(parameters);
                children.push(result.as_ref());
            }
            Type::BorrowedFunctionPointer {
                parameters,
                result,
                life,
            } => {
                storage(b, &mut nodes, cap, depth_cap, life)?;
                pay(b, &mut nodes, cap, parameters.len())?;
                children.extend(parameters);
                children.push(result.as_ref());
            }
            Type::Named(name) | Type::Unsupported(name) => {
                b.spend(if copy_or_compare{name.len()}else{1})?;
            }
            Type::ByteBuffer
            | Type::ForeignHandle
            | Type::SyncCell(_)
            | Type::TaskI32
            | Type::CWideString
            | Type::Closure(_)
            | Type::Dyn { .. }
            | Type::OwnedDyn { .. } => {}
            ty if ty.is_b0_scalar() => {}
            _ => return Err(limit("unrecognized dyn metadata type")),
        }
        if children.len() > cap.saturating_sub(nodes).saturating_sub(pending.len()) {
            return Err(limit("dyn metadata pending type count exceeded"));
        }
        b.spend(children.len())?;
        for child in children {
            pending.push_back((child, depth + 1));
        }
    }
    Ok(())
}
/// Pure bounds only. Callers that clone, hash or compare the complete Type
/// continue to use ty(), whose full traversal prepays that actual work.
fn type_bounds_at(root:&Type,depth:usize,b:&mut Budget)->Check<()>{
    if structure::type_known(root,depth,b)?{return Ok(());}
    type_walk(root,depth,false,b)?;
    structure::type_success(root,depth,b)
}
pub(crate) fn bounds_type(root:&Type,b:&mut Budget)->Check<()>{type_bounds_at(root,0,b)}
/// The exact public DTV callbacks, without allocating/copying a DTV payload.
/// All class/domain interpretation remains with the independent inventory.
pub(crate) fn bounds_type_parts(ty:&Type,life:&StorageLifeShape,classes:&[StorageBinder],b:&mut Budget)->Check<()>{
    let work=&mut BoundsWork(RecipeWork{b,nodes:0});
    work.node(0)?;work.ty(ty,1)?;work.life(life,1)?;work.rows(classes.len(),1)?;
    for _ in classes{work.node(1)?;}Ok(())
}

/// Prepayment for the one real owned child copy made by a fresh constructor.
/// This is the same complete DTV visitor without allocating a temporary DTV.
#[track_caller]
pub(crate) fn copy_type_parts(ty:&Type,life:&StorageLifeShape,classes:&[StorageBinder],b:&mut Budget)->Check<()>{
    let previous=b.diagnostic_metadata_caller;
    if b.diagnostic_costs.is_some(){b.diagnostic_metadata_caller=Some(std::panic::Location::caller());}
    let result=(||{let work=&mut RecipeWork{b,nodes:0};
    work.node(0)?;work.ty(ty,1)?;work.life(life,1)?;work.rows(classes.len(),1)?;
    for _ in classes{work.node(1)?;}Ok(())})();
    b.diagnostic_metadata_caller=previous;
    result
}

#[cfg(test)]
mod borrowed_parts_tests {
    use super::*;
    #[test]
    fn metadata_caller_attribution_restores_outer_on_success_and_error_without_changing_work(){
        let value=DynTypeValue{ty:Type::Tuple(vec![Type::I32]),life:StorageLifeShape{binder_count:0,slots:vec![],retained:vec![],nominal_arguments:vec![]},classes:vec![]};
        let outer=std::panic::Location::caller();
        for kind in 0..3{
            let limits=Limits{max_call_depth:0,..Limits::default()};
            let mut profiled=Budget::new(&limits);let mut control=Budget::new(&limits);
            profiled.diagnostic_costs=Some(BTreeMap::new());control.diagnostic_costs=None;
            profiled.diagnostic_metadata_caller=Some(outer);
            let run=|b:&mut Budget|match kind{0=>ty(&value.ty,b),1=>recipe(&value,b),_=>copy_type_parts(&value.ty,&value.life,&value.classes,b)};
            assert!(run(&mut profiled).is_err());assert!(run(&mut control).is_err());
            assert_eq!(profiled.remaining,control.remaining);
            assert!(std::ptr::eq(profiled.diagnostic_metadata_caller.unwrap(),outer));
            profiled.limits.max_call_depth=128;control.limits.max_call_depth=128;
            run(&mut profiled).unwrap();run(&mut control).unwrap();
            assert_eq!(profiled.remaining,control.remaining);
            assert!(std::ptr::eq(profiled.diagnostic_metadata_caller.unwrap(),outer));
        }
    }
    #[test]
    fn borrowed_dtv_preserves_public_visitor_quotas_and_depths() {
        let value=DynTypeValue{
            ty:Type::Tuple(vec![Type::Record{name:"long_name".repeat(64),fields:vec![("field".repeat(64),Type::I32)]}]),
            life:StorageLifeShape{binder_count:1,slots:vec![],retained:vec![StorageRetainedBinding{path:vec![LifeTypeProjection::Place(PlaceProjection::Field(0))],class:0,binder:0}],nominal_arguments:vec![]},
            classes:vec![StorageBinder::Invocation(0)],
        };
        for nodes in [0,1,2,3,4,8,16,64] {
            for depth in [0,1,2,3,128] {
                let limits=Limits{max_nodes:nodes,max_call_depth:depth,..Limits::default()};
                let mut borrowed=Budget::new(&limits);let mut owned=Budget::new(&limits);
                let a=bounds_type_parts(&value.ty,&value.life,&value.classes,&mut borrowed);
                let z=value.visit_dyn_metadata(&mut BoundsWork(RecipeWork{b:&mut owned,nodes:0}),0);
                assert_eq!(a,z,"nodes={nodes}, depth={depth}");
                assert_eq!(borrowed.remaining,owned.remaining);
                let mut copied=Budget::new(&limits);let mut public=Budget::new(&limits);
                let a=copy_type_parts(&value.ty,&value.life,&value.classes,&mut copied);
                let z=recipe(&value,&mut public);
                assert_eq!(a,z,"owned-copy nodes={nodes}, depth={depth}");
                assert_eq!(copied.remaining,public.remaining);
            }
        }
        let mut bounded=Budget::new(&Limits::default());let mut copied=Budget::new(&Limits::default());
        bounds_type_parts(&value.ty,&value.life,&value.classes,&mut bounded).unwrap();
        recipe(&value,&mut copied).unwrap();
        assert!(bounded.remaining>copied.remaining,"only the removed copy stops reading nominal name bytes");
    }
}

/// Pay the entire public retained directory independently of its Context type.
/// A forged directory need not have the Context's size or paths.
pub(super) fn retained_shape(shape: &RetainedLifeShape, b: &mut Budget) -> Check<()> {
    let cap = b.limits.max_nodes;
    let depth_cap = b.limits.max_call_depth.min(128);
    let mut nodes = 0usize;
    retained(b, &mut nodes, cap, depth_cap, shape)?;
    b.spend(shape.ports.len())?;
    for port in &shape.ports {
        ty(&port.leaf, b)?;
    }
    Ok(())
}

pub(super) fn position(position: &StoragePosition, b: &mut Budget) -> Check<()> {
    path(
        b,
        &mut 0usize,
        b.limits.max_nodes,
        b.limits.max_call_depth.min(128),
        position.path.len(),
    )
}

/// Bound the complete untrusted expression before identity or interpretation.
/// Paying for a view or Static expression establishes no such provenance.
pub(super) fn source(source: &StorageSource, b: &mut Budget) -> Check<()> {
    b.spend(1)?;
    match source {
        StorageSource::Exact(p) => position(p, b),
        StorageSource::PlaceAddress { place, .. } => {
            b.spend(1)?;
            position(place, b)
        }
        StorageSource::OwnedView { head, leaf, .. } => {
            b.spend(1)?;
            position(head, b)?;
            ty(leaf, b)
        }
        StorageSource::ReborrowedLeaf { source, head } => {
            b.dyn_reborrow_sources = true;
            position(source, b)?;
            position(head, b)
        }
        StorageSource::Static { leaf, .. } => {
            b.spend(1)?;
            ty(leaf, b)
        }
    }
}
fn actuals(values: &[DynGenericActual], b: &mut Budget) -> Check<()> {
    pay(b, &mut 0usize, b.limits.max_nodes, values.len())?;
    for actual in values {
        match actual {
            DynGenericActual::Type(node) => ty(node, b)?,
            DynGenericActual::Const(Constant::WideString(units)) => b.spend(units.len())?,
            _ => b.spend(1)?,
        }
    }
    Ok(())
}
fn kinds(kinds: &[LifeParameterKind], b: &mut Budget) -> Check<()> {
    pay(b, &mut 0usize, b.limits.max_nodes, kinds.len())?;
    for kind in kinds {
        if let LifeParameterKind::Const(node) = kind {
            ty(node, b)?;
        }
    }
    Ok(())
}
pub(super) fn declaration(declaration: &DynDeclaration, b: &mut Budget) -> Check<()> {
    declaration_at(declaration,None,b)
}
pub(super) fn scoped_declaration(module:&TypedModule,declaration:&DynDeclaration,b:&mut Budget)->Check<()> {
    let owner=if module.semantic.is_some(){Some(crate::dynamic_witness::semantic_external::original_scope(module,declaration.id,b)?.id)}else{None};
    declaration_at(declaration,owner,b)
}
pub(super) fn program_declaration(program:&SemanticModule,declarations:&[DynDeclaration],declaration:&DynDeclaration,b:&mut Budget)->Check<()> {
    let owner=crate::dynamic_witness::semantic_external::original_program_scope(program,declarations,declaration.id,b)?;
    declaration_in_scope(declaration,owner,b)
}
pub(super) fn declaration_in_scope(declaration:&DynDeclaration,owner:&SemanticDefinition,b:&mut Budget)->Check<()> {
    if matches!(owner.body,SemanticDefinitionBody::Scope(SemanticScopeRecipe::BuiltinImpl{trait_kind:SemanticBuiltinTrait::Drop,..})){
        // This comparison still occurs even when the declaration's bounds
        // are known. Keep the full identity-read payment before equality.
        for identity in [&owner.identity,&declaration.identity]{
            b.spend(identity.package.len().saturating_add(identity.module_path.len()).saturating_add(identity.declaration_path.len()).saturating_add(identity.generation_path.len()).saturating_add(1))?;
            for text in identity.module_path.iter().chain(&identity.declaration_path){b.spend(text.len())?;}
        }
    }
    let valid=match (&owner.body,declaration.kind) {
        (SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait{declaration:id}),DynDeclarationKind::Trait)=>*id==declaration.id,
        (SemanticDefinitionBody::Scope(SemanticScopeRecipe::InherentImpl{declaration:id,..}|SemanticScopeRecipe::TraitImpl{declaration:id,..}),DynDeclarationKind::Impl)=>*id==declaration.id,
        (SemanticDefinitionBody::Scope(SemanticScopeRecipe::BuiltinImpl{trait_kind:SemanticBuiltinTrait::Drop,..}),DynDeclarationKind::Impl)=>owner.identity==declaration.identity,
        _=>false,
    };
    if !valid||owner.kind!=SemanticDefinitionKind::Scope{return Err(reject("E-VERIFY-DYN-PATTERN",None,"declaration does not select its exact original owner scope"));}
    declaration_at(declaration,Some(owner.id),b)
}
fn declaration_at(declaration:&DynDeclaration,owner:Option<SemanticDefinitionId>,b:&mut Budget)->Check<()> {
    if structure::declaration_known(declaration,owner,b)?{return Ok(());}
    recipe_declaration(declaration, b)?;
    pattern_at(declaration, owner, b)?;
    kinds(&declaration.parameters, b)?;
    let identity = &declaration.identity;
    b.spend(
        identity
            .package
            .len()
            .saturating_add(identity.module_path.len())
            .saturating_add(identity.declaration_path.len())
            .saturating_add(identity.generation_path.len())
            .saturating_add(1),
    )?;
    for part in identity
        .module_path
        .iter()
        .chain(&identity.declaration_path)
    {
        b.spend(part.len())?;
    }
    pay(
        b,
        &mut 0usize,
        b.limits.max_nodes,
        declaration.members.len(),
    )?;
    for member in &declaration.members {
        b.spend(member.name.len().saturating_add(1))?;
        match &member.kind {
            DynMemberKind::Constant => {}
            DynMemberKind::Method(formals) => kinds(formals, b)?,
            DynMemberKind::Type => {}
        }
    }
    structure::declaration_success(declaration,owner,b)?;
    Ok(())
}
pub(super) fn interface(module: &TypedModule, item: &DynInterface, b: &mut Budget) -> Check<()> {
    actuals(&item.application.actuals, b)?;
    pay(
        b,
        &mut 0usize,
        b.limits.max_nodes,
        item.inherited
            .len()
            .saturating_add(item.associated_types.len())
            .saturating_add(item.associated_constants.len())
            .saturating_add(item.slots.len()),
    )?;
    for application in &item.inherited {
        actuals(&application.actuals, b)?;
    }
    for associated in &item.associated_types {
        actuals(&associated.member.application.actuals, b)?;
        ty(&associated.ty, b)?;
        let mut work = RecipeWork { b, nodes: 0 };
        DynRecipeVisitor::life(&mut work, &associated.life, 0)?;
        work.count(associated.classes.len(), 0)?;
    }
    for associated in &item.associated_constants {
        actuals(&associated.member.application.actuals, b)?;
        ty(&associated.ty, b)?;
        if let Constant::WideString(units) = &associated.value {
            b.spend(units.len())?;
        }
    }
    for slot in &item.slots {
        actuals(&slot.member.application.actuals, b)?;
        actuals(&slot.actuals, b)?;
        pay(
            b,
            &mut 0usize,
            b.limits.max_nodes,
            slot.parameters
                .len()
                .saturating_add(slot.parameter_modes.len()),
        )?;
        for node in slot.parameters.iter().chain(std::iter::once(&slot.result)) {
            ty(node, b)?;
        }
        if let Some(life) = &slot.storage_life {
            storage(
                b,
                &mut 0usize,
                b.limits.max_nodes,
                b.limits.max_call_depth.min(128),
                life,
            )?;
        }
        let c = &slot.contract;
        b.spend(9)?;
        for requirement in c.requires.iter().flatten() {
            match requirement {
                DynRequirement::Read(p) | DynRequirement::Write(p) | DynRequirement::Manage(p) => {
                    position(p, b)?
                }
                DynRequirement::Apart(a, z) => {
                    position(a, b)?;
                    position(z, b)?;
                }
                DynRequirement::State {
                    position: p,
                    ty: node,
                    ..
                } => {
                    position(p, b)?;
                    ty(node, b)?;
                }
            }
        }
        for guarantee in c.guarantees.iter().flatten() {
            match guarantee {
                DynGuarantee::StorageSources(binding) => {
                    position(&binding.position, b)?;
                    pay(b, &mut 0usize, b.limits.max_nodes, binding.sources.len())?;
                    for source in &binding.sources {
                        self::source(source, b)?;
                    }
                }
                DynGuarantee::State {
                    position: p,
                    ty: node,
                    ..
                } => {
                    position(p, b)?;
                    ty(node, b)?;
                }
                DynGuarantee::ManageResult => b.spend(1)?,
            }
        }
        for p in c.hold.iter().flatten() {
            position(p, b)?;
        }
        if let Some(call) = &c.call {
            for p in call.reads.iter().chain(&call.writes) {
                position(p, b)?;
            }
        }
        if let Some(DynResultSource::Borrow { sources, .. }) = &c.result {
            pay(b, &mut 0usize, b.limits.max_nodes, sources.len())?;
            for p in sources {
                source(p, b)?;
            }
        }
        for effect in c.effects.iter().flatten() {
            match effect {
                DynEffect::Cleanup(node) => ty(node, b)?,
                DynEffect::Call(p) => position(p, b)?,
                _ => b.spend(1)?,
            }
        }
        crate::dynamic_cleanup::metadata(module, c.cleanup.as_ref(), b)?;
    }
    Ok(())
}

/// Bound declaration recipes before their fingerprints recurse. A pattern is
/// metadata only; this pass does not instantiate it or establish a class.
pub(super) fn pattern(declaration: &DynDeclaration, b: &mut Budget) -> Check<()> {
    pattern_at(declaration,None,b)
}
fn pattern_at(declaration:&DynDeclaration,owner:Option<SemanticDefinitionId>,b:&mut Budget)->Check<()> {
    enum Node<'a> {
        Nominal(&'a DynSelfPattern, usize),
        Actual(&'a DynPatternActual, usize),
        Ty(&'a DynTypePattern, usize),
        Constant(&'a DynConstPattern, usize),
    }

    let cap = b.limits.max_nodes;
    let depth_cap = b.limits.max_call_depth.min(128);
    let mut count = 0usize;
    pay(b, &mut count, cap, 2)?;
    let mut pending = Vec::new();
    if let Some(root) = &declaration.self_pattern {
        pending.push(Node::Ty(root, 0));
    }
    if let Some(root) = &declaration.implemented_trait {
        pending.push(Node::Nominal(root, 0));
    }
    while let Some(node) = pending.pop() {
        pay(b, &mut count, cap, 1)?;
        let mut children = Vec::new();
        let depth = match node {
            Node::Nominal(node, depth) => {
                pay(b, &mut count, cap, node.arguments.len())?;
                children.extend(node.arguments.iter().map(|a| Node::Actual(a, depth + 1)));
                depth
            }
            Node::Actual(actual, depth) => {
                match actual {
                    DynPatternActual::Life(
                        zil_ir::DynLifePattern::MethodParameter(_)
                        | zil_ir::DynLifePattern::Bound { .. }
                        | zil_ir::DynLifePattern::Invocation(_)
                        | zil_ir::DynLifePattern::ImplicitDeclaration(_),
                    ) => {
                        return Err(reject(
                            "E-VERIFY-DYN-PATTERN",
                            None,
                            "persistent declaration life recipe is outside its original formal scope",
                        ));
                    }
                    DynPatternActual::Type(ty) => children.push(Node::Ty(ty, depth + 1)),
                    DynPatternActual::Const(value) => {
                        children.push(Node::Constant(value, depth + 1))
                    }
                    DynPatternActual::Life(zil_ir::DynLifePattern::DeclarationParameter(index)) => {
                        if !matches!(
                            declaration.parameters.get(*index as usize),
                            Some(LifeParameterKind::Lifetime)
                        ) {
                            return Err(reject(
                                "E-VERIFY-DYN-PATTERN",
                                None,
                                "life recipe selects a non-life formal ordinal",
                            ));
                        }
                    }
                }
                depth
            }
            Node::Constant(pattern, depth) => {
                match pattern {
                    DynConstPattern::MethodParameter(_)
                    | DynConstPattern::Unary { .. }
                    | DynConstPattern::Binary { .. }
                    | DynConstPattern::Convert { .. }
                    | DynConstPattern::AssociatedProjection { .. } => {
                        return Err(reject(
                            "E-VERIFY-DYN-PATTERN",
                            None,
                            "constant recipe requires a checked declaration specialization",
                        ));
                    }
                    DynConstPattern::Parameter(index) => {
                        if !matches!(
                            declaration.parameters.get(*index as usize),
                            Some(LifeParameterKind::Const(_))
                        ) {
                            return Err(reject(
                                "E-VERIFY-DYN-PATTERN",
                                None,
                                "constant recipe selects a non-constant formal ordinal",
                            ));
                        }
                    }
                    DynConstPattern::Value(Constant::WideString(units)) => {
                        pay(b, &mut count, cap, units.len())?;
                    }
                    DynConstPattern::Value(_) => {}
                    DynConstPattern::IntegerLiteral(text) | DynConstPattern::FloatLiteral(text) => {
                        pay(b, &mut count, cap, text.len())?;
                    }
                }
                depth
            }
            Node::Ty(pattern, depth) => {
                match pattern {
                    DynTypePattern::Semantic { scope,ty } => {
                        if owner!=Some(*scope){return Err(unsupported("complete semantic recipe needs its independently checked owner context", None));}
                        recipe(ty.as_ref(),b)?;
                    }
                    DynTypePattern::SelfType
                    | DynTypePattern::MethodParameter(_)
                    | DynTypePattern::AssociatedProjection { .. } => {
                        return Err(reject(
                            "E-VERIFY-DYN-PATTERN",
                            None,
                            "Self recipe cannot be established by a receiver or projection claim",
                        ));
                    }
                    DynTypePattern::Concrete(node) => ty(node, b)?,
                    DynTypePattern::Parameter(index) => {
                        if !matches!(
                            declaration.parameters.get(*index as usize),
                            Some(LifeParameterKind::Type)
                        ) {
                            return Err(reject(
                                "E-VERIFY-DYN-PATTERN",
                                None,
                                "type recipe selects a non-type formal ordinal",
                            ));
                        }
                    }
                    DynTypePattern::Nominal(node) => children.push(Node::Nominal(node, depth + 1)),
                    DynTypePattern::Address {
                        lifetime, inner, ..
                    } => {
                        if lifetime.is_some_and(|life| match life {
                            zil_ir::DynLifePattern::DeclarationParameter(index) => !matches!(
                                declaration.parameters.get(index as usize),
                                Some(LifeParameterKind::Lifetime)
                            ),
                            _ => true,
                        }) {
                            return Err(reject(
                                "E-VERIFY-DYN-PATTERN",
                                None,
                                "address lifetime recipe selects a non-life formal ordinal",
                            ));
                        }
                        children.push(Node::Ty(inner, depth + 1));
                    }
                    DynTypePattern::Tuple(fields) => {
                        pay(b, &mut count, cap, fields.len())?;
                        children.extend(fields.iter().map(|ty| Node::Ty(ty, depth + 1)));
                    }
                    DynTypePattern::Array { element, length } => {
                        children.push(Node::Ty(element, depth + 1));
                        children.push(Node::Constant(length, depth + 1));
                    }
                    DynTypePattern::Slice(node)
                    | DynTypePattern::Managed(node)
                    | DynTypePattern::Desc { inner: node, .. }
                    | DynTypePattern::OwnedStore(node) => children.push(Node::Ty(node, depth + 1)),
                    DynTypePattern::ErrorUnion { error, result } => {
                        children.push(Node::Ty(error, depth + 1));
                        children.push(Node::Ty(result, depth + 1));
                    }
                }
                depth
            }
        };
        if depth > depth_cap {
            return Err(limit("dyn recipe depth exceeds selected bound"));
        }
        if children
            .len()
            .saturating_add(pending.len())
            .saturating_add(count)
            > cap
        {
            return Err(limit("dyn recipe pending node bound exceeded"));
        }
        b.spend(children.len())?;
        pending.extend(children);
    }
    Ok(())
}

/// This visitor only bounds raw structure before fingerprinting. Actual
/// declaration, substitution, origin and obligation proofs remain separate.
struct RecipeWork<'a> {
    b: &'a mut Budget,
    nodes: usize,
}
impl RecipeWork<'_> {
    fn count(&mut self, n: usize, depth: usize) -> Check<()> {
        if depth > self.b.limits.max_call_depth.min(128) {
            return Err(limit("dynamic recipe metadata quota exceeded"));
        }
        self.nodes = self
            .nodes
            .checked_add(n)
            .ok_or_else(|| limit("dynamic recipe metadata quota exceeded"))?;
        if self.nodes > self.b.limits.max_nodes {
            return Err(limit("dynamic recipe metadata quota exceeded"));
        }
        self.b.spend(n)
    }
}
impl DynRecipeVisitor for RecipeWork<'_> {
    type Error = zil_base::Diagnostic;
    fn node(&mut self, depth: usize) -> Check<()> {
        self.count(1, depth)
    }
    fn rows(&mut self, n: usize, depth: usize) -> Check<()> {
        self.count(n.saturating_add(1), depth)
    }
    fn ty(&mut self, value: &Type, depth: usize) -> Check<()> {
        self.node(depth)?;
        ty_at(value, depth, self.b)
    }
    fn life(&mut self, value: &StorageLifeShape, depth: usize) -> Check<()> {
        self.rows(value.binder_count as usize, depth)?;
        self.rows(
            value
                .slots
                .len()
                .saturating_add(value.retained.len())
                .saturating_add(value.nominal_arguments.len()),
            depth,
        )?;
        for row in &value.slots {
            self.rows(row.path.len(), depth.saturating_add(row.path.len()))?;
        }
        for row in &value.retained {
            self.rows(row.path.len(), depth.saturating_add(row.path.len()))?;
        }
        for row in &value.nominal_arguments {
            self.rows(row.path.len(), depth.saturating_add(row.path.len()))?;
        }
        Ok(())
    }
    fn identity(&mut self, value: &LifeDeclarationIdentity, depth: usize) -> Check<()> {
        self.node(depth)?;
        self.rows(value.package.len(), depth)?;
        self.rows(
            value
                .module_path
                .len()
                .saturating_add(value.declaration_path.len())
                .saturating_add(value.generation_path.len()),
            depth,
        )?;
        for part in value.module_path.iter().chain(&value.declaration_path) {
            self.rows(part.len(), depth)?;
        }
        Ok(())
    }
    fn position(&mut self, value: &StoragePosition, depth: usize) -> Check<()> {
        self.rows(value.path.len(), depth.saturating_add(value.path.len()))
    }
    fn constant(&mut self, value: &Constant, depth: usize) -> Check<()> {
        self.node(depth)?;
        if let Constant::WideString(units) = value {
            self.rows(units.len(), depth)?;
        }
        Ok(())
    }
    fn text(&mut self, value: &str, depth: usize) -> Check<()> {
        self.rows(value.len(), depth)
    }
}
#[track_caller]
pub(super) fn recipe<T: DynRecipeMetadata>(value: &T, b: &mut Budget) -> Check<()> {
    let previous=b.diagnostic_metadata_caller;
    if b.diagnostic_costs.is_some(){b.diagnostic_metadata_caller=Some(std::panic::Location::caller());}
    let result=value.visit_dyn_metadata(&mut RecipeWork { b, nodes: 0 }, 0);
    b.diagnostic_metadata_caller=previous;
    result
}
/// A registered immutable recipe may reuse only its structural bounds. This
/// is not a prepayment for cloning/comparing it, or a semantic obligation.
pub(super) fn bounds_recipe<T:DynRecipeMetadata+'static>(value:&T,b:&mut Budget)->Check<()>{
    if structure::recipe_known(value,b)?{return Ok(());}
    value.visit_dyn_metadata(&mut BoundsWork(RecipeWork{b,nodes:0}),0)?;
    structure::recipe_success(value,b)
}
struct BoundsWork<'a>(RecipeWork<'a>);
impl DynRecipeVisitor for BoundsWork<'_>{
    type Error=Diagnostic;
    fn node(&mut self,d:usize)->Check<()>{self.0.node(d)}
    fn rows(&mut self,n:usize,d:usize)->Check<()>{self.0.rows(n,d)}
    fn ty(&mut self,value:&Type,d:usize)->Check<()>{self.0.node(d)?;type_bounds_at(value,d,self.0.b)}
    fn life(&mut self,value:&StorageLifeShape,d:usize)->Check<()>{self.0.life(value,d)}
    fn identity(&mut self,value:&LifeDeclarationIdentity,d:usize)->Check<()>{self.0.identity(value,d)}
    fn position(&mut self,value:&StoragePosition,d:usize)->Check<()>{self.0.position(value,d)}
    fn constant(&mut self,value:&Constant,d:usize)->Check<()>{self.0.constant(value,d)}
    fn text(&mut self,value:&str,d:usize)->Check<()>{self.0.text(value,d)}
}
/// The same bounded primitive visitors used by complete recipe traversal.
pub(super) fn life_shape(value:&StorageLifeShape,b:&mut Budget)->Check<()>{
    DynRecipeVisitor::life(&mut RecipeWork{b,nodes:0},value,0)
}
pub(super) fn constant(value:&Constant,b:&mut Budget)->Check<()>{
    DynRecipeVisitor::constant(&mut RecipeWork{b,nodes:0},value,0)
}
fn recipe_rows<T: DynRecipeMetadata>(
    values: &Option<Vec<T>>,
    work: &mut RecipeWork<'_>,
) -> Check<()> {
    work.count(1, 0)?;
    if let Some(values) = values {
        work.count(values.len(), 0)?;
        for value in values {
            value.visit_dyn_metadata(work, 0)?;
        }
    }
    Ok(())
}
pub(super) fn recipe_declaration(value: &DynDeclaration, b: &mut Budget) -> Check<()> {
    let work = &mut RecipeWork { b, nodes: 0 };
    recipe_rows(&value.implicit_lifetimes, work)?;
    recipe_rows(&value.direct_supertraits, work)?;
    recipe_rows(&value.requirements, work)?;
    recipe_rows(&value.associated_definitions, work)?;
    recipe_rows(&value.method_signatures, work)?;
    recipe_rows(&value.method_definitions, work)
}
pub(super) fn recipe_table(value: &DynVtable, b: &mut Budget) -> Check<()> {
    let work = &mut RecipeWork { b, nodes: 0 };
    work.count(1, 0)?;
    if let Some(shape) = &value.self_life {
        DynRecipeVisitor::life(work, shape, 0)?;
    }
    recipe_rows(&value.implementation_witnesses, work)
}

/// A source builtin Drop directory is an identity record, not an open user
/// trait scheme. Its absent dynamic recipes remain Unknown; actual resource
/// cleanup is independently checked against the original sealed Drop body.
pub(crate) fn sealed_drop_metadata(module:&TypedModule,declaration:&DynDeclaration,b:&mut Budget)->Check<bool>{
    let Some(program)=&module.semantic else{return Ok(false);};
    program_sealed_drop_metadata(program,&module.dyn_declarations,declaration,b)
}
pub(in crate) fn program_sealed_drop_metadata(program:&SemanticModule,declarations:&[DynDeclaration],declaration:&DynDeclaration,b:&mut Budget)->Check<bool>{
    b.spend(program.dyn_scopes.len().saturating_add(program.definitions.len()))?;
    let mut bindings=program.dyn_scopes.iter().filter(|row|row.owner==SemanticDynOwner::Declaration(declaration.id));
    let Some(binding)=bindings.next() else{return Ok(false);};if bindings.next().is_some(){return Err(reject("E-VERIFY-DYN-PATTERN",None,"sealed Drop original owner is duplicated"));}
    let Some(owner)=program.definitions.iter().find(|row|row.id==binding.scope) else{return Err(reject("E-VERIFY-DYN-PATTERN",None,"sealed Drop original owner is absent"));};
    if !matches!(owner.body,SemanticDefinitionBody::Scope(SemanticScopeRecipe::BuiltinImpl{trait_kind:SemanticBuiltinTrait::Drop,..})){return Ok(false);}
    let owner=crate::dynamic_witness::semantic_external::original_program_scope(program,declarations,declaration.id,b)?;
    declaration_in_scope(declaration,owner,b)?;
    if declaration.implemented_trait.is_some(){return Err(reject("E-VERIFY-DYN-PATTERN",None,"sealed Drop metadata cannot implement an open dynamic trait"));}
    let [member]=declaration.members.as_slice() else{return Err(reject("E-VERIFY-DYN-PATTERN",None,"sealed Drop original member inventory differs"));};
    let DynMemberKind::Method(kinds)=&member.kind else{return Err(reject("E-VERIFY-DYN-PATTERN",None,"sealed Drop member is not a method"));};
    let mut methods=program.definitions.iter().filter(|row|row.parent==Some(owner.id)&&row.kind==SemanticDefinitionKind::Method);
    let method=methods.next().ok_or_else(||reject("E-VERIFY-DYN-PATTERN",None,"sealed Drop has no original method body"))?;
    if methods.next().is_some()||member.name!="drop"||kinds.len()!=method.parameters.len(){return Err(reject("E-VERIFY-DYN-PATTERN",None,"sealed Drop changes its actual original method identity or mixed arity"));}
    for(kind,formal)in kinds.iter().zip(&method.parameters){if !matches!((kind,formal),(LifeParameterKind::Type,SemanticFormal::Type{..})|(LifeParameterKind::Lifetime,SemanticFormal::Lifetime))&&!matches!((kind,formal),(LifeParameterKind::Const(a),SemanticFormal::Const{carrier:SemanticTypeRecipe::Atom(z)})if a==z){return Err(reject("E-VERIFY-DYN-PATTERN",None,"sealed Drop method changes its ordered original formal kind"));}}
    if !matches!(method.body,SemanticDefinitionBody::FunctionWithBody{body:SemanticOriginalFunctionBody::Defined(_),..}){return Err(reject("E-VERIFY-DYN-PATTERN",None,"sealed Drop original method has no Defined body"));}
    Ok(true)
}
