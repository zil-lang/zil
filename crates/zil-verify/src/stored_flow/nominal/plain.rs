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
            (LifeNominalKind::Record, Type::Record { .. }) |
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
            (LifeParameterKind::Const(ty), LifeNominalActual::Const(value)) if value.ty() == *ty && (ty.is_integer() || ty.is_float() || matches!(ty, Type::Unit | Type::Bool)) => { type_work(ty, limits, b)?; },
            _ => return Err(err("plain nominal changes its full-ordinal actual kind or constant carrier")),
        }
    }
    if declaration.id.compiler {
        let [LifeNominalActual::Type(actual)] = instance.arguments.as_slice() else { return Err(err("plain builtin nominal omits its complete Type argument")); };
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
            let (count, pending) = crate::dynamic_witness::intrinsic_domain(module, ty, b)?;
            if count != 0 || pending { return Err(err("plain nominal has a nonzero or unproved opaque persistent domain")); }
        }
    }
    Ok(())
}
