//! Plain nominal inventory is canonical only after its full persistent domain
//! is independently zero. This does not turn a metadata selector into a loan.
use super::*;

pub(in crate::stored_flow) fn check(module: &TypedModule, instance: &LifeNominalInstance, limits: &Limits, b: &mut Budget) -> Check<()> {
    walk(module, instance, limits, b, &mut Vec::new()).map(|_| ())
}

pub(super) fn with_active(module: &TypedModule, instance: &LifeNominalInstance, limits: &Limits, b: &mut Budget, active: &mut Vec<usize>) -> Check<()> {
    b.spend(module.life_declarations.len().saturating_add(instance.arguments.len()))?;
    let mut declarations = module.life_declarations.iter().filter(|d| d.id == instance.declaration);
    let declaration = declarations.next().ok_or_else(|| err("plain nominal has no original declaration"))?;
    if declarations.next().is_some() || declaration.parameters.len() != instance.arguments.len()
        || !matches!((declaration.kind, &instance.ty),
            (LifeNominalKind::Record | LifeNominalKind::Resource, Type::Record { .. }) |
            (LifeNominalKind::Package, Type::Package { .. }) |
            (LifeNominalKind::Enum, Type::Enum { .. })) {
        return Err(err("plain nominal declaration kind or full mixed arity differs"));
    }
    for (formal, actual) in declaration.parameters.iter().zip(&instance.arguments) {
        match (formal, actual) {
            // A phantom lifetime is still a persistent class. Plain storage
            // cannot provide its original occurrence or map it to no class.
            (LifeParameterKind::Lifetime, _) => return Err(err("plain nominal omits a persistent lifetime actual")),
            (LifeParameterKind::Type, LifeNominalActual::Type(ty)) => zero(module, ty, limits, b, active)?,
            (LifeParameterKind::Type, LifeNominalActual::QualifiedType{ty,life}) => {
                if life.binder_count!=0||!life.slots.is_empty()||!life.retained.is_empty()||!life.nominal_arguments.is_empty(){return Err(err("plain nominal cannot erase a nonzero qualified Type actual"));}
                // Empty metadata is not a zero-domain proof. The complete
                // actual type independently establishes Known0 below.
                zero(module,ty,limits,b,active)?;
            },
            (LifeParameterKind::Const(ty), LifeNominalActual::Const(value)) if value.ty() == *ty && (ty.is_integer() || ty.is_float() || matches!(ty, Type::Unit | Type::Bool)) => { type_work(ty, limits, b)?; },
            _ => return Err(err("plain nominal changes its full-ordinal actual kind or constant carrier")),
        }
    }
    if declaration.id.compiler && !sealed_task_enum(declaration,instance,b)? {
        let [argument] = instance.arguments.as_slice() else { return Err(err("plain builtin nominal omits its complete Type argument")); };
        let actual=match argument{LifeNominalActual::Type(ty)|LifeNominalActual::QualifiedType{ty,..}=>ty,_=>return Err(err("plain builtin nominal changes its actual full-ordinal Type kind"))};
        let Type::Enum { name, variants } = &instance.ty else { return Err(err("plain builtin nominal is not its actual enum")); };
        b.spend(name.len().saturating_add(variants.len()).saturating_add(96))?;
        let option = option_address::declaration(declaration);
        let poll = enum_address::poll_declaration(declaration);
        if !(option || poll) || variants.len() != 2 || !variants[0].1.is_empty()
            || variants[1].1.as_slice() != std::slice::from_ref(actual)
            || variants[0].0 != if option { "None" } else { "Pending" }
            || variants[1].0 != if option { "Some" } else { "Ready" }
            || poll && name != &format!("@std.task:v1:Poll:{}", actual.fingerprint()) {
            return Err(err("plain compiler nominal lacks its sealed declaration/actual/variant correspondence"));
        }
    }
    match &instance.ty {
        Type::Record { name, fields } | Type::Package { name, fields } => {
            if name.is_empty() { return Err(err("plain nominal has no physical name")); }
            b.spend(fields.len())?;
            let mut names = Vec::new();
            for (name, _) in fields {
                b.spend(name.len().saturating_add(names.len()))?;
                if name.is_empty() || names.contains(&name) { return Err(err("plain nominal has an empty or duplicate field")); }
                names.push(name);
            }
            for (_, child) in fields { zero(module, child, limits, b, active)?; }
        }
        Type::Enum { name, variants } => {
            if name.is_empty() { return Err(err("plain enum has no physical name")); }
            let mut names = Vec::new();
            for (name, fields) in variants {
                b.spend(name.len().saturating_add(names.len()).saturating_add(fields.len()).saturating_add(1))?;
                if name.is_empty() || names.contains(&name) { return Err(err("plain enum has an empty or duplicate variant")); }
                names.push(name);
                for child in fields { zero(module, child, limits, b, active)?; }
            }
        },
        _ => unreachable!(),
    }
    Ok(())
}

fn zero(module: &TypedModule, ty: &Type, limits: &Limits, b: &mut Budget, active: &mut Vec<usize>) -> Check<()> {
    type_work(ty, limits, b)?;
    match ty {
        Type::Record { .. } | Type::Package { .. } | Type::Enum { .. } => {
            b.spend(module.life_instances.len())?;
            let mut instances = module.life_instances.iter().filter(|i| i.ty == *ty);
            let instance = instances.next().ok_or_else(|| err("plain nominal child lacks its complete declaration/actual inventory"))?;
            if instances.next().is_some() { return Err(err("plain nominal child has ambiguous complete identities")); }
            walk(module, instance, limits, b, active)?;
        }
        Type::BorrowedStorage { life, .. } => {
            if life.binder_count != 0 || !life.slots.is_empty() || !life.retained.is_empty() || !life.nominal_arguments.is_empty() {
                return Err(err("plain nominal erases a qualified child persistent domain"));
            }
            qualification(module, ty, limits, b)?;
        }
        Type::Tuple(children) => { b.spend(children.len())?; for child in children { zero(module, child, limits, b, active)?; } }
        Type::Array { element, .. } | Type::Managed(element) | Type::OwnedStore { element } | Type::Mutex { payload: element } => zero(module, element, limits, b, active)?,
        Type::ErrorUnion { error, result } => { zero(module, error, limits, b, active)?; zero(module, result, limits, b, active)?; }
        Type::Address { .. } | Type::Desc(_) | Type::Slice { .. } => return Err(err("plain nominal lacks an actual outer borrowing occurrence")),
        _ => {
            let (count, _pending_sources) = crate::dynamic_witness::intrinsic_domain(module, ty, b)?;
            if count != 0 { return Err(err("plain nominal has a nonzero opaque persistent domain")); }
            // The complete inventory above independently proves cardinality,
            // including a real Some closure relation and exact entry/captures.
            // Pending construction/access/cleanup sources are separate actual
            // CFG obligations; this nominal metadata proof discharges none.
        }
    }
    Ok(())
}

// These compiler declarations have zero Type formals. Their sealed complete
// nominal identities and all variants are checked independently; they are not
// Option/Poll instantiations selected by a matching physical enum layout.
fn sealed_task_enum(declaration:&LifeNominalDeclaration,instance:&LifeNominalInstance,b:&mut Budget)->Check<bool>{
    let (word,variants)=if declaration.id==zil_ir::builtin_life::TASK_CREATE_ERROR{("TaskCreateError",&["Allocation"][..])}
        else if declaration.id==zil_ir::builtin_life::TASK_POLL_STATE{("TaskPollState",&["Complete","Cancelled","Invalid"][..])}
        else{return Ok(false);};
    b.spend(word.len().saturating_add(variants.len()).saturating_add(declaration.identity.package.len()).saturating_add(96))?;
    if declaration.kind!=LifeNominalKind::Enum||!declaration.parameters.is_empty()||declaration.identity.package!="@zil.builtin"||declaration.identity.module_path.as_slice()!=["task"]||declaration.identity.declaration_path.as_slice()!=["enum",word]||!declaration.identity.generation_path.is_empty()||!instance.arguments.is_empty(){return Err(err("plain sealed Task enum changes its complete original declaration"));}
    let Type::Enum{name,variants:actual}=&instance.ty else{return Err(err("plain sealed Task enum has another physical kind"));};
    if name!=&format!("@std.task:v1:{word}")||actual.len()!=variants.len()||actual.iter().zip(variants).any(|((name,fields),expected)|name.as_str()!=*expected||!fields.is_empty()){return Err(err("plain sealed Task enum changes its exact complete variants"));}
    Ok(true)
}

#[cfg(test)]
mod tests{
    use super::*;
    // Metadata-only proofs deliberately retain pending actual sources. These
    // cases are not executable Source/owner/loan certificates.
    #[test]
    fn opaque_zero_cardinality_is_separate_from_pending_actual_sources_raw_wire(){
        let limits=Limits::default();
        for count in [0,1]{
            let(mut module,plan)=crate::tests::baseline();
            let relation=StorageLifeContract{fixed_binder_count:count,invocation_binder_count:0,bindings:(0..count).map(|index|StorageLifeBinding{position:StoragePosition{base:StorageBase::Capture(index),path:vec![]},binder:StorageBinder::Fixed(index)}).collect(),classes:vec![],static_bindings:vec![]};
            module.functions[0].environment=Some(ClosureId(0));module.functions[0].storage_life=Some(relation.clone());
            module.closures.push(ClosureDefinition{id:ClosureId(0),function:FunctionId(0),captures:(0..count).map(|_|Type::Address{mutable:false,inner:Box::new(Type::I32)}).collect(),parameters:vec![],result:Type::I32,kind:CallableKind::Fn,storage_life:Some(relation),lending_captures:vec![]});
            for wire in [false,true]{
                let bytes=zil_ir::wire::encode_module_candidate(&module,&plan,&limits).unwrap();
                let(candidate,_)=if wire{zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap()}else{(module.clone(),plan.clone())};
                let mut b=Budget::new(&limits);let ty=Type::Closure(ClosureId(0));
                assert_eq!(crate::dynamic_witness::intrinsic_domain(&candidate,&ty,&mut b).unwrap(),(count,true));
                assert_eq!(zero(&candidate,&ty,&limits,&mut b,&mut vec![]).is_ok(),count==0);
                let mut unknown=candidate.clone();unknown.closures[0].storage_life=None;unknown.functions[0].storage_life=None;
                assert!(zero(&unknown,&ty,&limits,&mut Budget::new(&limits),&mut vec![]).is_err());
            }
        }
    }
}
