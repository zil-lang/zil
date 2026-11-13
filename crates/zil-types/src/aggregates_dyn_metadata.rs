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
        // Closed raw tables keep their original nominal source name. Qualified
        // tables must retain the complete resolved class-bearing Self type.
        if matches!(t.self_type, Type::BorrowedStorage { .. }) {
            t.self_type = r.ty(&t.self_type, 0, Span::default())?;
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
