//! Actual provider relations, reconstructed from complete nominal occurrences.
use crate::{bad, budget::Budget};
use std::collections::BTreeSet;
use zil_base::CompileResult;
use zil_ir::*;
const WORK: &str = "dyn actual provider relation";
fn closed(ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    let mut work = vec![(ty, 0usize)];
    while let Some((t, depth)) = work.pop() {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(bad("provider closed type actual exceeds depth"));
        }
        match t {
            Type::Tuple(fields) => {
                b.spend(fields.len(), WORK)?;
                work.extend(fields.iter().map(|t| (t, depth + 1)));
            }
            Type::Record { fields, .. } => {
                b.spend(fields.len(), WORK)?;
                work.extend(fields.iter().map(|(_, t)| (t, depth + 1)));
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    b.spend(fields.len(), WORK)?;
                    work.extend(fields.iter().map(|t| (t, depth + 1)));
                }
            }
            Type::Array { element, .. } => work.push((element, depth + 1)),
            t if t.is_b0_scalar() || *t == Type::Unit => {}
            _ => return Ok(false),
        }
    }
    Ok(true)
}

fn occurrence(
    life: &StorageLifeShape,
    declaration: LifeDeclarationId,
    argument: usize,
    local: u32,
    table: &DynVtable,
    b: &mut Budget,
) -> CompileResult<DynGenericActual> {
    b.spend(life.nominal_arguments.len().saturating_add(1), WORK)?;
    let mut rows = life.nominal_arguments.iter().filter(|row| {
        row.path.is_empty() && row.declaration == declaration && row.argument as usize == argument
    });
    let row = rows
        .next()
        .ok_or_else(|| bad("provider omits an actual original nominal life occurrence"))?;
    if rows.next().is_some() || row.binder != local {
        return Err(bad(
            "provider nominal occurrence changes its actual Self class",
        ));
    }
    let mapped = table
        .self_classes
        .get(local as usize)
        .ok_or_else(|| bad("provider Self class mapping is absent"))?;
    Ok(DynGenericActual::Life(*mapped))
}
fn pattern_actual(
    pattern: &DynPatternActual,
    actual: &DynGenericActual,
    implementation: &DynDeclaration,
    table: &DynVtable,
    b: &mut Budget,
) -> CompileResult<()> {
    b.spend(1, WORK)?;
    let valid = match (pattern, actual) {
        (DynPatternActual::LifeParameter(i), DynGenericActual::Life(_)) => {
            implementation.parameters.get(*i as usize) == Some(&LifeParameterKind::Lifetime)
                && table.actuals.get(*i as usize) == Some(actual)
        }
        (DynPatternActual::Type(DynTypePattern::Parameter(i)), DynGenericActual::Type(t)) => {
            crate::dyn_metadata::ty(t, b)?;
            implementation.parameters.get(*i as usize) == Some(&LifeParameterKind::Type)
                && closed(t, b)?
                && table.actuals.get(*i as usize) == Some(actual)
        }
        (DynPatternActual::Type(DynTypePattern::Concrete(expected)), DynGenericActual::Type(t)) => {
            crate::dyn_metadata::ty(expected, b)?;
            crate::dyn_metadata::ty(t, b)?;
            expected == t && closed(t, b)?
        }
        (
            DynPatternActual::Const(DynConstPattern::Value(value)),
            DynGenericActual::Const(actual),
        ) => value == actual,
        (
            DynPatternActual::Const(DynConstPattern::Parameter(i)),
            DynGenericActual::Const(value),
        ) => {
            matches!(implementation.parameters.get(*i as usize), Some(LifeParameterKind::Const(t)) if *t == value.ty())
                && table.actuals.get(*i as usize) == Some(actual)
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(bad(
            "provider original formal recipe does not instantiate to the actual application",
        ))
    }
}
fn selected<'a>(
    storage: &'a Type,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> CompileResult<&'a Type> {
    let mut ty = storage;
    b.spend(path.len(), WORK)?;
    for p in path {
        if let Type::BorrowedStorage { storage, .. } = ty {
            ty = storage;
        }
        ty = match (ty, p) {
            (
                Type::Record { fields, .. } | Type::Package { fields, .. },
                PlaceProjection::Field(i),
            ) => fields.get(*i as usize).map(|(_, t)| t),
            (Type::Tuple(fields), PlaceProjection::Field(i)) => fields.get(*i as usize),
            (Type::Array { element, length }, PlaceProjection::Field(i)) if *i < *length => {
                Some(element.as_ref())
            }
            (Type::Enum { variants, .. }, PlaceProjection::Variant(v)) => variants
                .get(v.variant as usize)
                .and_then(|(_, f)| f.get(v.index as usize)),
            (Type::ErrorUnion { error, result }, PlaceProjection::ErrorArm(e)) => {
                Some(if *e { error.as_ref() } else { result.as_ref() })
            }
            _ => None,
        }
        .ok_or_else(|| bad("provider port does not select an actual complete Self leaf"))?;
    }
    Ok(ty)
}
pub(super) fn prove(
    module: &TypedModule,
    table: &DynVtable,
    interface: &DynInterface,
    b: &mut Budget,
) -> CompileResult<()> {
    b.spend(
        module
            .dyn_declarations
            .len()
            .saturating_add(table.actuals.len()),
        WORK,
    )?;
    let implementation = module
        .dyn_declarations
        .iter()
        .find(|d| d.id == table.implementation)
        .ok_or_else(|| bad("provider original implementation is absent"))?;
    if implementation.kind != DynDeclarationKind::Impl
        || implementation.parameters.len() != table.actuals.len()
    {
        return Err(bad(
            "provider implementation has no complete ordered actual vector",
        ));
    }
    for (kind, actual) in implementation.parameters.iter().zip(&table.actuals) {
        match (kind, actual) {
            (LifeParameterKind::Lifetime, DynGenericActual::Life(StorageBinder::Fixed(i)))
                if *i < interface.retained_life.binder_count => {}
            (LifeParameterKind::Type, DynGenericActual::Type(t)) if closed(t, b)? => {
                crate::dyn_metadata::ty(t, b)?
            }
            (LifeParameterKind::Const(t), DynGenericActual::Const(c)) if *t == c.ty() => {}
            _ => return Err(bad("provider formal kind or full actual domain differs")),
        }
    }
    let (storage, life) = match &table.self_type {
        Type::BorrowedStorage { storage, life } => (storage.as_ref(), Some(life)),
        other => (other, None),
    };
    if let Some(life) = life {
        let limits = zil_base::Limits {
            max_nodes: b.helper_nodes,
            max_call_depth: b.partial_depth,
            ..zil_base::Limits::default()
        };
        crate::stored_flow::qualification(module, &table.self_type, &limits, b)?;
        if table.self_classes.len() != life.binder_count as usize {
            return Err(bad(
                "provider omits a complete Self class including phantom occurrences",
            ));
        }
        let recipe = implementation
            .self_pattern
            .as_ref()
            .ok_or_else(|| bad("borrowing provider has no original Self recipe"))?;
        b.spend(
            module
                .life_instances
                .len()
                .saturating_add(module.life_declarations.len()),
            WORK,
        )?;
        let mut instances = module
            .life_instances
            .iter()
            .filter(|i| i.ty == table.self_type);
        let instance = instances
            .next()
            .ok_or_else(|| bad("provider Self has no actual nominal instance"))?;
        let declaration = module
            .life_declarations
            .iter()
            .find(|d| d.id == instance.declaration)
            .ok_or_else(|| bad("provider nominal declaration is absent"))?;
        if instances.next().is_some()
            || instance.declaration != recipe.declaration
            || recipe.arguments.len() != instance.arguments.len()
            || declaration.parameters.len() != instance.arguments.len()
        {
            return Err(bad(
                "provider Self recipe has a different full nominal identity or arity",
            ));
        }
        b.spend(instance.arguments.len(), WORK)?;
        for (index, ((p, a), kind)) in recipe
            .arguments
            .iter()
            .zip(&instance.arguments)
            .zip(&declaration.parameters)
            .enumerate()
        {
            let actual = match (a, kind) {
                (LifeNominalActual::Lifetime(local), LifeParameterKind::Lifetime) => {
                    occurrence(life, instance.declaration, index, *local, table, b)?
                }
                (LifeNominalActual::Type(t), LifeParameterKind::Type) => {
                    crate::dyn_metadata::ty(t, b)?;
                    DynGenericActual::Type(t.clone())
                }
                (LifeNominalActual::Const(c), LifeParameterKind::Const(t)) if *t == c.ty() => {
                    DynGenericActual::Const(c.clone())
                }
                _ => return Err(bad("provider nominal argument changes declaration kind")),
            };
            pattern_actual(p, &actual, implementation, table, b)?;
        }
    } else if implementation.self_pattern.is_some()
        || !implementation.parameters.is_empty()
        || !table.self_classes.is_empty()
    {
        return Err(bad("provider Self recipe has no actual qualified instance"));
    }
    let recipe = implementation
        .implemented_trait
        .as_ref()
        .ok_or_else(|| bad("provider implemented trait application is Unknown"))?;
    if recipe.declaration != interface.application.declaration
        || recipe.arguments.len() != interface.application.actuals.len()
    {
        return Err(bad(
            "provider implements a different complete trait application",
        ));
    }
    for (p, a) in recipe.arguments.iter().zip(&interface.application.actuals) {
        pattern_actual(p, a, implementation, table, b)?;
    }
    if table.ports.len() != interface.retained_life.ports.len() {
        return Err(bad("provider opaque port table is incomplete"));
    }
    let mut covered = BTreeSet::new();
    b.spend(
        table.ports.len().saturating_add(table.self_classes.len()),
        WORK,
    )?;
    for class in &table.self_classes {
        if !matches!(class, StorageBinder::Fixed(i) if *i < interface.retained_life.binder_count) {
            return Err(bad(
                "provider Self class is outside the actual Context domain",
            ));
        }
    }
    for (ordinal, (row, upper)) in table
        .ports
        .iter()
        .zip(&interface.retained_life.ports)
        .enumerate()
    {
        if row.port as usize != ordinal {
            return Err(bad("provider port rows are not complete and canonical"));
        }
        b.spend(row.sources.len(), WORK)?;
        for source in &row.sources {
            if source.base != StorageBase::Result {
                return Err(bad("provider leaf source is not its actual Self"));
            }
            b.spend(
                source
                    .path
                    .len()
                    .saturating_mul(3)
                    .saturating_add(covered.len()),
                WORK,
            )?;
            let path = source
                .path
                .iter()
                .map(|p| match p {
                    ContractProjection::Place(p) => Ok(*p),
                    _ => Err(bad("provider source needs an actual physical Self slot")),
                })
                .collect::<CompileResult<Vec<_>>>()?;
            if !covered.insert(path.clone()) {
                return Err(bad("provider actual Self leaf is repeated"));
            }
            let life =
                life.ok_or_else(|| bad("closed provider claims an external borrowed source"))?;
            b.spend(life.slots.len(), WORK)?;
            let slot = life.slots.iter().find(|s| s.path == path).ok_or_else(|| {
                bad("provider claims a nonborrowed Self slot as an external source")
            })?;
            let leaf = selected(storage, &path, b)?;
            crate::dyn_metadata::ty(leaf, b)?;
            crate::dyn_metadata::ty(&upper.leaf, b)?;
            if leaf != &upper.leaf
                || slot.mode != upper.mode
                || table.self_classes.get(slot.binder as usize)
                    != Some(&StorageBinder::Fixed(upper.binder))
            {
                return Err(bad(
                    "provider source changes full leaf type, mode or original class",
                ));
            }
        }
    }
    if covered.len() != life.map_or(0, |life| life.slots.len()) {
        return Err(bad("provider omits an actual external Self leaf"));
    }
    Ok(())
}

/// An unused recipe supplies no authority, but its complete formal scope and
/// referenced declaration kinds still have to be well formed.
pub(super) fn declaration(
    module: &TypedModule,
    d: &DynDeclaration,
    b: &mut Budget,
) -> CompileResult<()> {
    fn formal(d: &DynDeclaration, i: u32, kind: &LifeParameterKind) -> CompileResult<()> {
        if d.parameters.get(i as usize) == Some(kind) {
            Ok(())
        } else {
            Err(bad(
                "dyn recipe references a missing or wrong-kind original formal",
            ))
        }
    }
    fn cp(c: &DynConstPattern, d: &DynDeclaration, kind: Option<&Type>) -> CompileResult<()> {
        match c {
            DynConstPattern::Value(c) if kind.is_none_or(|t| *t == c.ty()) => Ok(()),
            DynConstPattern::Parameter(i) if matches!(d.parameters.get(*i as usize),Some(LifeParameterKind::Const(t)) if kind.is_none_or(|k|k==t)) => {
                Ok(())
            }
            _ => Err(bad("dyn constant recipe changes its original carrier kind")),
        }
    }
    fn tp(
        p: &DynTypePattern,
        m: &TypedModule,
        d: &DynDeclaration,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth.min(128) {
            return Err(bad("dyn recipe exceeds independent depth"));
        }
        match p {
            DynTypePattern::Parameter(i) => formal(d, *i, &LifeParameterKind::Type),
            DynTypePattern::Concrete(t) => {
                crate::dyn_metadata::ty(t, b)?;
                if closed(t, b)? {
                    Ok(())
                } else {
                    Err(bad(
                        "dyn concrete recipe contains an unbound or retained type",
                    ))
                }
            }
            DynTypePattern::Nominal(p) => np(p, m, d, b, depth + 1, false),
            DynTypePattern::Address {
                lifetime, inner, ..
            } => {
                if let Some(i) = lifetime {
                    formal(d, *i, &LifeParameterKind::Lifetime)?;
                }
                tp(inner, m, d, b, depth + 1)
            }
            DynTypePattern::Tuple(fields) => {
                b.spend(fields.len(), WORK)?;
                for f in fields {
                    tp(f, m, d, b, depth + 1)?;
                }
                Ok(())
            }
            DynTypePattern::Array { element, length } => {
                cp(length, d, None)?;
                tp(element, m, d, b, depth + 1)
            }
            DynTypePattern::Slice(t)
            | DynTypePattern::Managed(t)
            | DynTypePattern::Desc(t)
            | DynTypePattern::OwnedStore(t) => tp(t, m, d, b, depth + 1),
            DynTypePattern::ErrorUnion { error, result } => {
                tp(error, m, d, b, depth + 1)?;
                tp(result, m, d, b, depth + 1)
            }
        }
    }
    fn np(
        p: &DynSelfPattern,
        m: &TypedModule,
        d: &DynDeclaration,
        b: &mut Budget,
        depth: usize,
        is_trait: bool,
    ) -> CompileResult<()> {
        b.spend(
            m.life_declarations
                .len()
                .saturating_add(m.dyn_declarations.len())
                .saturating_add(p.arguments.len()),
            WORK,
        )?;
        if depth > b.partial_depth.min(128) {
            return Err(bad("dyn nominal recipe exceeds independent depth"));
        }
        let kinds = if is_trait {
            let target = m
                .dyn_declarations
                .iter()
                .find(|t| t.id == p.declaration && t.kind == DynDeclarationKind::Trait)
                .ok_or_else(|| {
                    bad("implemented application recipe does not reference an actual trait")
                })?;
            &target.parameters
        } else {
            &m.life_declarations
                .iter()
                .find(|t| t.id == p.declaration)
                .ok_or_else(|| bad("Self recipe does not reference an actual nominal declaration"))?
                .parameters
        };
        if kinds.len() != p.arguments.len() {
            return Err(bad("dyn recipe omits a complete ordered actual occurrence"));
        }
        for (kind, actual) in kinds.iter().zip(&p.arguments) {
            match (kind, actual) {
                (LifeParameterKind::Lifetime, DynPatternActual::LifeParameter(i)) => {
                    formal(d, *i, kind)?
                }
                (LifeParameterKind::Type, DynPatternActual::Type(t)) => tp(t, m, d, b, depth + 1)?,
                (LifeParameterKind::Const(t), DynPatternActual::Const(c)) => cp(c, d, Some(t))?,
                _ => return Err(bad("dyn recipe actual changes original declaration kind")),
            }
        }
        Ok(())
    }
    if d.kind == DynDeclarationKind::Trait {
        if d.self_pattern.is_some() || d.implemented_trait.is_some() {
            return Err(bad("trait declaration cannot claim implementation recipes"));
        }
    } else {
        if let Some(p) = &d.self_pattern {
            np(p, module, d, b, 0, false)?;
        }
        if let Some(p) = &d.implemented_trait {
            np(p, module, d, b, 0, true)?;
        }
    }
    Ok(())
}

pub(super) fn application(
    module: &TypedModule,
    interface: &DynInterface,
    b: &mut Budget,
) -> CompileResult<()> {
    b.spend(
        module
            .dyn_declarations
            .len()
            .saturating_add(interface.application.actuals.len()),
        WORK,
    )?;
    let d = module
        .dyn_declarations
        .iter()
        .find(|d| d.id == interface.application.declaration && d.kind == DynDeclarationKind::Trait)
        .ok_or_else(|| bad("dyn application has no actual trait declaration"))?;
    if d.parameters.len() != interface.application.actuals.len() {
        return Err(bad(
            "dyn application omits full ordered declaration actuals",
        ));
    }
    for (kind, actual) in d.parameters.iter().zip(&interface.application.actuals) {
        match (kind, actual) {
            (LifeParameterKind::Lifetime, DynGenericActual::Life(StorageBinder::Fixed(i)))
                if *i < interface.retained_life.binder_count => {}
            (LifeParameterKind::Type, DynGenericActual::Type(t)) if closed(t, b)? => {}
            (LifeParameterKind::Const(t), DynGenericActual::Const(c)) if *t == c.ty() => {}
            _ => {
                return Err(bad(
                    "dyn application actual changes declared kind or fixed Context domain",
                ));
            }
        }
    }
    Ok(())
}
