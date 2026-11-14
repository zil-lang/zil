//! Resolve every embedded dyn type in the same nominal environment as its
//! provider headers and life instances. The caller prepays the full snapshot.
use super::*;
fn actual(a: &mut DynGenericActual, r: &mut Resolver<'_>) -> R<()> {
    match a {
        DynGenericActual::Type(ty) => *ty = r.ty(ty, 0, Span::default())?,
        DynGenericActual::QualifiedType(value) => value.ty = r.ty(&value.ty, 0, Span::default())?,
        DynGenericActual::Const(_) | DynGenericActual::Life(_) => {}
    }
    Ok(())
}
// A specialization receiver denotes payload Self, rather than an ordinary
// default resource description. Only its original bare resource declaration
// selects this category; explicit wrappers and complete Type actuals survive.
fn receiver_type(value: &Type, r: &mut Resolver<'_>) -> R<Type> {
    r.charge(0, Span::default())?;
    let raw_resource = match value {
        Type::Named(name)
            if r.ordinary_parameter.as_ref() != Some(name)
                && !r.aggregate_parameters.contains_key(name) =>
        {
            r.declarations
                .get(name)
                .is_some_and(|d| d.kind == AggregateKind::Resource)
                || matches!(name.as_str(), "ByteBuffer" | "TaskI32")
        }
        _ => false,
    };
    let resolved = r.ty(value, 0, Span::default())?;
    Ok(if raw_resource {
        match resolved {
            Type::Desc(payload) => *payload,
            other => other,
        }
    } else {
        resolved
    })
}
fn receiver(value: &mut DynTypeValue, r: &mut Resolver<'_>) -> R<()> {
    value.ty = receiver_type(&value.ty, r)?;
    // life/classes are the original complete domain, not data reconstructed
    // from the resolved physical shape. Independent consumers check them.
    Ok(())
}
pub(super) fn function_instance(value: &mut DynFunctionInstance, r: &mut Resolver<'_>) -> R<()> {
    crate::dyn_metadata::semantic_operand_snapshot(value, &r.generic_limits, &mut r.remaining, 1)?;
    receiver(&mut value.receiver, r)?;
    for a in value
        .declaration_actuals
        .iter_mut()
        .chain(&mut value.method_actuals)
    {
        actual(a, r)?;
    }
    Ok(())
}
fn witness(value: &mut DynImplementationWitness, r: &mut Resolver<'_>) -> R<()> {
    receiver(&mut value.receiver, r)?;
    for a in value
        .actuals
        .iter_mut()
        .chain(&mut value.application.actuals)
    {
        actual(a, r)?;
    }
    for associated in &mut value.associated {
        r.charge(0, Span::default())?;
        for a in &mut associated.member.application.actuals {
            actual(a, r)?;
        }
        let ty = match &mut associated.value {
            DynAssociatedValue::Type(ty) | DynAssociatedValue::Constant { ty, .. } => ty,
        };
        ty.ty = r.ty(&ty.ty, 0, Span::default())?;
    }
    Ok(())
}
fn obligation(value: &mut DynObligation, r: &mut Resolver<'_>, depth: usize) -> R<()> {
    r.charge(depth, Span::default())?;
    match value {
        DynObligation::Structural { receiver, .. } => type_pattern(receiver, r, depth + 1),
        DynObligation::Trait {
            receiver,
            application,
        } => {
            type_pattern(receiver, r, depth + 1)?;
            pattern(application, r, depth + 1)
        }
        DynObligation::Callable {
            receiver,
            signature: value,
            ..
        } => {
            type_pattern(receiver, r, depth + 1)?;
            signature(value, r, depth + 1)
        }
        DynObligation::TypeEqual { left, right } => {
            type_pattern(left, r, depth + 1)?;
            type_pattern(right, r, depth + 1)
        }
        DynObligation::ConstEqual { ty, left, right } => {
            type_pattern(ty, r, depth + 1)?;
            const_pattern(left, r, depth + 1)?;
            const_pattern(right, r, depth + 1)
        }
    }
}
fn storage_source(value: &mut DynStorageSourceRecipe, r: &mut Resolver<'_>, depth: usize) -> R<()> {
    r.charge(depth, Span::default())?;
    match value {
        DynStorageSourceRecipe::OwnedView { leaf, .. }
        | DynStorageSourceRecipe::Static { leaf, .. } => type_pattern(leaf, r, depth + 1),
        _ => Ok(()),
    }
}
fn recipe_effects(values: &mut [DynEffectRecipe], r: &mut Resolver<'_>, depth: usize) -> R<()> {
    for value in values {
        r.charge(depth, Span::default())?;
        if let DynEffectRecipe::Cleanup(ty) = value {
            type_pattern(ty, r, depth + 1)?;
        }
    }
    Ok(())
}
fn signature(value: &mut DynSignatureRecipe, r: &mut Resolver<'_>, depth: usize) -> R<()> {
    r.charge(depth, Span::default())?;
    for parameter in &mut value.parameters {
        type_pattern(&mut parameter.ty, r, depth + 1)?;
    }
    type_pattern(&mut value.result, r, depth + 1)?;
    for requirement in value.contract.requires.iter_mut().flatten() {
        r.charge(depth + 1, Span::default())?;
        if let DynRequirementRecipe::State { ty, .. } = requirement {
            type_pattern(ty, r, depth + 2)?;
        }
    }
    for guarantee in value.contract.guarantees.iter_mut().flatten() {
        r.charge(depth + 1, Span::default())?;
        match guarantee {
            DynGuaranteeRecipe::State { ty, .. } => type_pattern(ty, r, depth + 2)?,
            DynGuaranteeRecipe::StorageSources(binding) => {
                for source in &mut binding.sources {
                    storage_source(source, r, depth + 2)?;
                }
            }
            DynGuaranteeRecipe::ManageResult => {}
        }
    }
    if let Some(DynResultSourceRecipe::Borrow { sources, .. }) = &mut value.contract.result {
        for source in sources {
            storage_source(source, r, depth + 1)?;
        }
    }
    if let Some(cleanup) = &mut value.contract.cleanup {
        recipe_effects(&mut cleanup.effects, r, depth + 1)?;
    }
    if let Some(effects) = &mut value.contract.effects {
        recipe_effects(effects, r, depth + 1)?;
    }
    Ok(())
}
fn pattern(p: &mut DynSelfPattern, r: &mut Resolver<'_>, depth: usize) -> R<()> {
    r.charge(depth, Span::default())?;
    for a in &mut p.arguments {
        match a {
            DynPatternActual::Type(t) => type_pattern(t, r, depth + 1)?,
            DynPatternActual::Const(c) => const_pattern(c, r, depth + 1)?,
            DynPatternActual::Life(_) => {}
        }
    }
    Ok(())
}
fn type_pattern(p: &mut DynTypePattern, r: &mut Resolver<'_>, depth: usize) -> R<()> {
    r.charge(depth, Span::default())?;
    match p {
        DynTypePattern::Concrete(t) => *t = r.ty(t, depth + 1, Span::default())?,
        DynTypePattern::Nominal(p) => pattern(p, r, depth + 1)?,
        DynTypePattern::Address { inner, .. }
        | DynTypePattern::Slice(inner)
        | DynTypePattern::Managed(inner)
        | DynTypePattern::Desc { inner, .. }
        | DynTypePattern::OwnedStore(inner) => type_pattern(inner, r, depth + 1)?,
        DynTypePattern::Array { element, length } => {
            type_pattern(element, r, depth + 1)?;
            const_pattern(length, r, depth + 1)?;
        }
        DynTypePattern::Tuple(fields) => {
            for f in fields {
                type_pattern(f, r, depth + 1)?;
            }
        }
        DynTypePattern::ErrorUnion { error, result } => {
            type_pattern(error, r, depth + 1)?;
            type_pattern(result, r, depth + 1)?;
        }
        DynTypePattern::AssociatedProjection {
            receiver,
            application,
            ..
        } => {
            type_pattern(receiver, r, depth + 1)?;
            pattern(application, r, depth + 1)?;
        }
        DynTypePattern::Semantic { ty, .. } => {
            crate::semantics::rewrite::ty(ty, &mut SemanticRewrite(r), depth + 1)?;
        }
        DynTypePattern::Parameter(_)
        | DynTypePattern::MethodParameter(_)
        | DynTypePattern::SelfType => {}
    }
    Ok(())
}
fn const_pattern(p: &mut DynConstPattern, r: &mut Resolver<'_>, depth: usize) -> R<()> {
    r.charge(depth, Span::default())?;
    match p {
        DynConstPattern::Unary { value, ty, .. } | DynConstPattern::Convert { value, ty } => {
            const_pattern(value, r, depth + 1)?;
            type_pattern(ty, r, depth + 1)?;
        }
        DynConstPattern::Binary {
            left, right, ty, ..
        } => {
            const_pattern(left, r, depth + 1)?;
            const_pattern(right, r, depth + 1)?;
            type_pattern(ty, r, depth + 1)?;
        }
        DynConstPattern::AssociatedProjection {
            receiver,
            application,
            ..
        } => {
            type_pattern(receiver, r, depth + 1)?;
            pattern(application, r, depth + 1)?;
        }
        DynConstPattern::Value(_)
        | DynConstPattern::Parameter(_)
        | DynConstPattern::MethodParameter(_)
        | DynConstPattern::IntegerLiteral(_)
        | DynConstPattern::FloatLiteral(_) => {}
    }
    Ok(())
}
fn effects(e: &mut [DynEffect], r: &mut Resolver<'_>) -> R<()> {
    for e in e {
        r.charge(0, Span::default())?;
        if let DynEffect::Cleanup(t) = e {
            *t = r.ty(t, 0, Span::default())?;
        }
    }
    Ok(())
}
fn cleanup(c: &mut DynCleanupContract, r: &mut Resolver<'_>) -> R<()> {
    effects(&mut c.effects, r)
}
fn contract(c: &mut DynContract, r: &mut Resolver<'_>) -> R<()> {
    for requirement in c.requires.iter_mut().flatten() {
        if let DynRequirement::State { ty, .. } = requirement {
            *ty = r.ty(ty, 0, Span::default())?;
        }
    }
    for guarantee in c.guarantees.iter_mut().flatten() {
        if let DynGuarantee::State { ty, .. } = guarantee {
            *ty = r.ty(ty, 0, Span::default())?;
        }
    }
    if let Some(c) = &mut c.cleanup {
        cleanup(c, r)?;
    }
    if let Some(e) = &mut c.effects {
        effects(e, r)?;
    }
    Ok(())
}
#[allow(clippy::type_complexity)]
pub(super) fn resolve(
    core: &CoreModule,
    r: &mut Resolver<'_>,
) -> R<(Vec<DynDeclaration>, Vec<DynInterface>, Vec<CoreDynVtable>)> {
    let mut declarations = core.dyn_declarations.clone();
    let mut interfaces = core.dyn_interfaces.clone();
    let mut tables = core.dyn_vtables.clone();
    for d in &mut declarations {
        for k in &mut d.parameters {
            if let LifeParameterKind::Const(t) = k {
                *t = r.ty(t, 0, Span::default())?;
            }
        }
        for m in &mut d.members {
            match &mut m.kind {
                DynMemberKind::Constant => {}
                DynMemberKind::Method(ks) => {
                    for k in ks {
                        if let LifeParameterKind::Const(t) = k {
                            *t = r.ty(t, 0, Span::default())?;
                        }
                    }
                }
                DynMemberKind::Type => {}
            }
        }
        if let Some(p) = &mut d.self_pattern {
            type_pattern(p, r, 0)?;
        }
        if let Some(p) = &mut d.implemented_trait {
            pattern(p, r, 0)?;
        }
        for p in d.direct_supertraits.iter_mut().flatten() {
            pattern(p, r, 0)?;
        }
        for value in d.requirements.iter_mut().flatten() {
            obligation(value, r, 0)?;
        }
        for value in d.associated_definitions.iter_mut().flatten() {
            if let Some(carrier) = &mut value.carrier {
                type_pattern(carrier, r, 0)?;
            }
            if let Some(value) = &mut value.value {
                match value {
                    DynAssociatedValueRecipe::Type(ty) => type_pattern(ty, r, 0)?,
                    DynAssociatedValueRecipe::Constant { ty, value } => {
                        type_pattern(ty, r, 0)?;
                        const_pattern(value, r, 0)?;
                    }
                }
            }
            for requirement in &mut value.requirements {
                obligation(requirement, r, 0)?;
            }
        }
        for method in d.method_signatures.iter_mut().flatten() {
            signature(&mut method.signature, r, 0)?;
            for value in &mut method.requirements {
                obligation(value, r, 0)?;
            }
        }
        for method in d.method_definitions.iter_mut().flatten() {
            signature(&mut method.signature, r, 0)?;
            for value in &mut method.requirements {
                obligation(value, r, 0)?;
            }
            for instance in &mut method.instances {
                receiver(&mut instance.receiver, r)?;
                for a in instance
                    .declaration_actuals
                    .iter_mut()
                    .chain(&mut instance.method_actuals)
                {
                    actual(a, r)?;
                }
            }
        }
    }
    for i in &mut interfaces {
        if let Some(t) = &mut i.context {
            *t = r.ty(t, 0, Span::default())?;
        }
        if let Some(c) = &mut i.owning_cleanup {
            cleanup(c, r)?;
        }
        for a in &mut i.application.actuals {
            actual(a, r)?;
        }
        for app in &mut i.inherited {
            for a in &mut app.actuals {
                actual(a, r)?;
            }
        }
        for a in &mut i.associated_types {
            a.ty = r.ty(&a.ty, 0, Span::default())?;
            for p in &mut a.member.application.actuals {
                actual(p, r)?;
            }
        }
        for a in &mut i.associated_constants {
            a.ty = r.ty(&a.ty, 0, Span::default())?;
            for p in &mut a.member.application.actuals {
                actual(p, r)?;
            }
        }
        for p in &mut i.retained_life.ports {
            p.leaf = r.ty(&p.leaf, 0, Span::default())?;
        }
        for s in &mut i.slots {
            for a in &mut s.member.application.actuals {
                actual(a, r)?;
            }
            for a in &mut s.actuals {
                actual(a, r)?;
            }
            for t in &mut s.parameters {
                *t = r.ty(t, 0, Span::default())?;
            }
            s.result = r.ty(&s.result, 0, Span::default())?;
            contract(&mut s.contract, r)?;
        }
    }
    for t in &mut tables {
        // Self is payload at this explicit specialization boundary. Preserve
        // the original full life/classes while resolving its complete type.
        t.self_type = receiver_type(&t.self_type, r)?;
        for value in t.implementation_witnesses.iter_mut().flatten() {
            witness(value, r)?;
        }
        for a in &mut t.actuals {
            actual(a, r)?;
        }
    }
    Ok((declarations, interfaces, tables))
}

struct SemanticRewrite<'r, 'a>(&'r mut Resolver<'a>);
impl crate::semantics::rewrite::Context for SemanticRewrite<'_, '_> {
    fn node(&mut self, depth: usize) -> R<()> {
        self.0.charge(depth, Span::default())
    }
    fn payload(&mut self, ty: &Type, depth: usize) -> R<Type> {
        self.0.ty(ty, depth, Span::default())
    }
}
pub(super) fn semantic(value: &mut SemanticModule, resolver: &mut Resolver<'_>) -> R<()> {
    crate::semantics::rewrite::module(value, &mut SemanticRewrite(resolver))
}
