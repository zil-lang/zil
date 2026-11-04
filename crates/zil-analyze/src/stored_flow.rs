//! Actual stored targets are distinct from the bytes containing their pointers.
//! This pass does not use a candidate, source label, or nominal name as a root.
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Target {
    pub base: HoldRoot,
    /// Empty is an ordinary input pointee. A nonempty path is an input's
    /// *stored target*, never a byte offset into that input's carrier.
    pub stored: Vec<PlaceProjection>,
    pub place: Vec<PlaceProjection>,
}
type Targets = BTreeSet<Target>;
type Loans = BTreeSet<usize>;
type Path = Vec<PlaceProjection>;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Permission {
    targets: Targets,
    loans: Loans,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Value {
    pointer: Permission,
    slots: BTreeMap<Path, Permission>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct State {
    locals: BTreeMap<LocalId, Value>,
    /// External addressed carriers have real cells too. Parameter position is
    /// not their target identity; each leaf starts with a symbolic target.
    external: BTreeMap<u32, BTreeMap<Path, Permission>>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Loan {
    targets: Targets,
    parents: Loans,
    mutable: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Facts {
    pub values: Vec<Vec<Target>>,
    pub returned: BTreeMap<Path, Vec<Target>>,
    pub apart: BTreeSet<(Target, Target)>,
}

fn failure(text: &str) -> Vec<Diagnostic> {
    bad(format!("stored target proof: {text}"))
}
fn fee(b: &mut Budget, n: usize) -> CompileResult<()> {
    b.spend(n, "stored target CFG proof")
}
fn ordered(b: &mut Budget, n: usize, width: usize) -> CompileResult<()> {
    b.product(
        n.checked_ilog2().map_or(1, |n| n as usize + 2),
        width.saturating_add(1),
        1,
        "stored path comparison/search",
    )
}
fn target_work(t: &Targets, b: &mut Budget) -> CompileResult<()> {
    fee(b, t.len())?;
    for target in t {
        fee(b, target.stored.len().saturating_add(target.place.len()))?;
    }
    Ok(())
}
fn union(to: &mut Permission, from: &Permission, b: &mut Budget) -> CompileResult<bool> {
    let mut changed = false;
    for target in &from.targets {
        ordered(
            b,
            to.targets.len(),
            target.stored.len().saturating_add(target.place.len()),
        )?;
        fee(
            b,
            target
                .stored
                .len()
                .saturating_add(target.place.len())
                .saturating_add(1),
        )?;
        changed |= to.targets.insert(target.clone());
    }
    for loan in &from.loans {
        ordered(b, to.loans.len(), 0)?;
        changed |= to.loans.insert(*loan);
    }
    Ok(changed)
}
fn merge(to: &mut Value, from: &Value, b: &mut Budget) -> CompileResult<bool> {
    let mut changed = union(&mut to.pointer, &from.pointer, b)?;
    for (path, value) in &from.slots {
        ordered(b, to.slots.len(), path.len())?;
        fee(b, path.len().saturating_add(1))?;
        changed |= union(to.slots.entry(path.clone()).or_default(), value, b)?;
    }
    Ok(changed)
}
fn snapshot(value: &Value, b: &mut Budget) -> CompileResult<Value> {
    let mut result = Value::default();
    merge(&mut result, value, b)?;
    Ok(result)
}
fn state_copy(s: &State, b: &mut Budget) -> CompileResult<State> {
    fee(b, s.locals.len().saturating_add(s.external.len()))?;
    let mut out = State::default();
    for (local, value) in &s.locals {
        out.locals.insert(*local, snapshot(value, b)?);
    }
    for (index, slots) in &s.external {
        let mut charged = Value::default();
        for (path, value) in slots {
            ordered(b, charged.slots.len(), path.len())?;
            fee(b, path.len())?;
            union(charged.slots.entry(path.clone()).or_default(), value, b)?;
        }
        out.external.insert(*index, charged.slots);
    }
    Ok(out)
}
fn state_merge(to: &mut State, from: &State, b: &mut Budget) -> CompileResult<()> {
    for (local, value) in &from.locals {
        ordered(b, to.locals.len(), 0)?;
        merge(to.locals.entry(*local).or_default(), value, b)?;
    }
    for (index, slots) in &from.external {
        ordered(b, to.external.len(), 0)?;
        let target = to.external.entry(*index).or_default();
        for (path, permission) in slots {
            ordered(b, target.len(), path.len())?;
            fee(b, path.len().saturating_add(1))?;
            union(target.entry(path.clone()).or_default(), permission, b)?;
        }
    }
    Ok(())
}

fn carrier(ty: &Type) -> Option<(&Type, &StorageLifeShape)> {
    if let Type::BorrowedStorage { storage, life } = ty {
        Some((storage, life))
    } else {
        None
    }
}
fn pointed_carrier(ty: &Type) -> Option<(&Type, &StorageLifeShape)> {
    if let Type::Address { inner, .. } = ty {
        carrier(inner)
    } else {
        None
    }
}
fn value_type(f: &TypedFunction, id: ValueId) -> CompileResult<&Type> {
    f.values
        .get(id.0 as usize)
        .ok_or_else(|| failure("value outside actual table"))
}
fn slot<'a>(ty: &'a Type, path: &[PlaceProjection], b: &mut Budget) -> CompileResult<&'a Type> {
    let mut ty = ty;
    for part in path {
        fee(b, 1)?;
        ty = match (ty, part) {
            (
                Type::Record { fields, .. } | Type::Package { fields, .. },
                PlaceProjection::Field(i),
            ) => fields.get(*i as usize).map(|(_, t)| t),
            (Type::Tuple(fields), PlaceProjection::Field(i)) => fields.get(*i as usize),
            (Type::Enum { variants, .. }, PlaceProjection::Variant(p)) => variants
                .get(p.variant as usize)
                .and_then(|(_, fs)| fs.get(p.index as usize)),
            (Type::ErrorUnion { error, result }, PlaceProjection::ErrorArm(e)) => {
                Some(if *e { error.as_ref() } else { result.as_ref() })
            }
            _ => None,
        }
        .ok_or_else(|| failure("physical path crosses an absent or non-inline ancestor"))?;
    }
    Ok(ty)
}

fn shape_work(ty: &Type, limits: &Limits, b: &mut Budget) -> CompileResult<()> {
    fee(b, 1)?;
    let mut pending = vec![(ty, 0usize)];
    while let Some((ty, depth)) = pending.pop() {
        fee(b, 1)?;
        if depth > limits.max_call_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Analyze,
                "stored type depth exceeds analysis limit",
                None,
            )]);
        }
        match ty {
            Type::BorrowedStorage { storage, life } => {
                fee(
                    b,
                    life.slots
                        .len()
                        .saturating_add(life.retained.len())
                        .saturating_add(life.nominal_arguments.len())
                        .saturating_add(1),
                )?;
                for row in &life.slots {
                    fee(b, row.path.len())?;
                }
                for row in &life.retained {
                    fee(b, row.path.len())?;
                }
                for row in &life.nominal_arguments {
                    fee(b, row.path.len())?;
                }
                pending.push((storage, depth + 1));
            }
            Type::Record { name, fields } | Type::Package { name, fields } => {
                fee(b, name.len().saturating_add(fields.len()))?;
                for (name, t) in fields {
                    fee(b, name.len())?;
                    pending.push((t, depth + 1));
                }
            }
            Type::Tuple(fields) => {
                fee(b, fields.len())?;
                for t in fields {
                    pending.push((t, depth + 1));
                }
            }
            Type::Enum { name, variants } => {
                fee(b, name.len().saturating_add(variants.len()))?;
                for (name, fields) in variants {
                    fee(b, name.len().saturating_add(fields.len()))?;
                    for t in fields {
                        pending.push((t, depth + 1));
                    }
                }
            }
            Type::Address { inner, .. }
            | Type::Array { element: inner, .. }
            | Type::Slice { element: inner }
            | Type::Managed(inner)
            | Type::Desc(inner) => pending.push((inner, depth + 1)),
            Type::ErrorUnion { error, result } => {
                fee(b, 2)?;
                pending.push((error, depth + 1));
                pending.push((result, depth + 1));
            }
            Type::FunctionPointer { parameters, result }
            | Type::BorrowedFunctionPointer {
                parameters, result, ..
            } => {
                fee(b, parameters.len().saturating_add(1))?;
                for t in parameters {
                    pending.push((t, depth + 1));
                }
                pending.push((result, depth + 1));
            }
            _ => {}
        }
    }
    Ok(())
}

fn identity_work(id: &LifeDeclarationIdentity, b: &mut Budget) -> CompileResult<()> {
    fee(
        b,
        id.package
            .len()
            .saturating_add(id.module_path.len())
            .saturating_add(id.declaration_path.len())
            .saturating_add(id.generation_path.len())
            .saturating_add(1),
    )?;
    for text in id.module_path.iter().chain(&id.declaration_path) {
        fee(b, text.len())?;
    }
    Ok(())
}

/// Inventory is untrusted identity data. It resolves full actual ordinals but
/// supplies no pointee, owner, initialization, or permission facts.
pub(super) fn inventory(
    module: &TypedModule,
    limits: &Limits,
    b: &mut Budget,
) -> CompileResult<()> {
    fee(
        b,
        module
            .life_declarations
            .len()
            .saturating_add(module.life_instances.len()),
    )?;
    for (index, decl) in module.life_declarations.iter().enumerate() {
        identity_work(&decl.identity, b)?;
        if decl.identity.package.is_empty()
            || decl.identity.module_path.is_empty()
            || decl.identity.declaration_path.is_empty()
            || decl
                .identity
                .module_path
                .iter()
                .chain(&decl.identity.declaration_path)
                .any(String::is_empty)
        {
            return Err(failure(
                "nominal declaration lacks its full stable identity",
            ));
        }
        fee(b, decl.parameters.len())?;
        for formal in &decl.parameters {
            if let LifeParameterKind::Const(ty) = formal {
                shape_work(ty, limits, b)?;
                b.type_work(ty, true, "nominal const parameter kind")?;
                if !ty.is_copy_value() {
                    return Err(failure("nominal const parameter has invalid data type"));
                }
            }
        }
        for earlier in &module.life_declarations[..index] {
            identity_work(&earlier.identity, b)?;
            identity_work(&decl.identity, b)?;
            fee(b, 1)?;
            if decl.id == earlier.id || decl.identity == earlier.identity {
                return Err(failure("duplicate declaration ID or stable identity"));
            }
        }
    }
    for (index, instance) in module.life_instances.iter().enumerate() {
        shape_work(&instance.ty, limits, b)?;
        let Some((storage, life)) = carrier(&instance.ty) else {
            return Err(failure(
                "nominal stored instance has no complete qualified type",
            ));
        };
        physical(&instance.ty, b)?;
        fee(b, module.life_declarations.len())?;
        let decl = module
            .life_declarations
            .iter()
            .find(|d| d.id == instance.declaration)
            .ok_or_else(|| failure("nominal instance references absent declaration"))?;
        if !matches!(
            (decl.kind, storage),
            (LifeNominalKind::Record, Type::Record { .. })
        ) {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "nominal stored kind requires its actual physical consumer",
                None,
            )]);
        }
        fee(
            b,
            instance
                .arguments
                .len()
                .saturating_add(decl.parameters.len()),
        )?;
        if instance.arguments.len() != decl.parameters.len() {
            return Err(failure(
                "nominal instance omits full generic actual ordinals",
            ));
        }
        let mut lives = Vec::new();
        for (ordinal, (formal, actual)) in
            decl.parameters.iter().zip(&instance.arguments).enumerate()
        {
            match (formal, actual) {
                (LifeParameterKind::Lifetime, LifeNominalActual::Lifetime(class))
                    if *class < life.binder_count =>
                {
                    fee(b, 1)?;
                    lives.push((ordinal as u32, *class));
                }
                (LifeParameterKind::Type, LifeNominalActual::Type(ty)) => {
                    shape_work(ty, limits, b)?;
                    b.type_work(ty, true, "nominal full type actual")?;
                    if !ty.is_copy_value() {
                        return Err(vec![Diagnostic::unsupported(
                            Phase::Analyze,
                            "borrowed metadata-only Type actual requires the qualified context consumer",
                            None,
                        )]);
                    }
                }
                (LifeParameterKind::Const(ty), LifeNominalActual::Const(value))
                    if value.ty() == *ty => {}
                _ => {
                    return Err(failure(
                        "nominal actual kind, ordinal or local fixed class is invalid",
                    ));
                }
            }
        }
        if lives.len() != life.nominal_arguments.len() {
            return Err(failure(
                "nominal lifetime metadata coverage differs from full actuals",
            ));
        }
        for ((ordinal, class), row) in lives.iter().zip(&life.nominal_arguments) {
            fee(b, row.path.len().saturating_add(1))?;
            if !row.path.is_empty()
                || row.declaration != decl.id
                || row.argument != *ordinal
                || row.binder != *class
            {
                return Err(failure(
                    "nominal lifetime row differs from actual declaration/ordinal/class",
                ));
            }
        }
        for earlier in &module.life_instances[..index] {
            shape_work(&earlier.ty, limits, b)?;
            shape_work(&instance.ty, limits, b)?;
            if earlier.ty == instance.ty {
                return Err(failure("duplicate complete nominal instance"));
            }
        }
    }
    Ok(())
}

fn binding(
    f: &TypedFunction,
    base: StorageBase,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> CompileResult<StorageBinder> {
    let c = f
        .storage_life
        .as_ref()
        .ok_or_else(|| failure("stored signature has no relation"))?;
    fee(b, c.bindings.len())?;
    let mut found = None;
    for row in &c.bindings {
        fee(
            b,
            row.position
                .path
                .len()
                .saturating_add(path.len())
                .saturating_add(1),
        )?;
        if row.position.base == base
            && row.position.path.len() == path.len()
            && row
                .position
                .path
                .iter()
                .zip(path)
                .all(|(a, b)| *a == ContractProjection::Place(*b))
        {
            if found.replace(row.binder).is_some() {
                return Err(failure("duplicate signature position"));
            }
        }
    }
    found.ok_or_else(|| failure("signature omits actual borrowed position"))
}

pub(super) fn header(f: &TypedFunction, b: &mut Budget) -> CompileResult<()> {
    let Some(c) = &f.storage_life else {
        for ty in f
            .parameters
            .iter()
            .map(|l| &f.locals[l.0 as usize].ty)
            .chain(std::iter::once(&f.return_type))
        {
            fee(b, 1)?;
            if carrier(ty).is_some() || pointed_carrier(ty).is_some() {
                return Err(failure("stored signature requires its complete relation"));
            }
        }
        return Ok(());
    };
    if f.reference_return_bound.is_some() || f.foreign.is_some() || f.environment.is_some() {
        return Err(failure(
            "stored relation contradicts another signature profile",
        ));
    }
    fee(b, c.bindings.len())?;
    let mut expected = 0usize;
    let mut fixed = BTreeSet::new();
    let mut invocation = BTreeSet::new();
    let mut grounded = BTreeSet::new();
    for (ordinal, ty) in f
        .parameters
        .iter()
        .map(|l| &f.locals[l.0 as usize].ty)
        .chain(std::iter::once(&f.return_type))
        .enumerate()
    {
        let input = ordinal < f.parameters.len();
        let base = if input {
            StorageBase::Parameter(ordinal as u32)
        } else {
            StorageBase::Result
        };
        let mut positions = Vec::new();
        if matches!(ty, Type::Address { .. }) {
            fee(b, 1)?;
            positions.push(Vec::new());
        }
        if let Some((_, life)) = carrier(ty).or_else(|| pointed_carrier(ty)) {
            let mut classes = BTreeMap::new();
            for slot in &life.slots {
                fee(b, slot.path.len().saturating_add(1))?;
                let upper = binding(f, base, &slot.path, b)?;
                ordered(b, classes.len(), 0)?;
                if let Some(previous) = classes.insert(slot.binder, upper)
                    && previous != upper
                {
                    return Err(failure("one stored fixed class is rebound inconsistently"));
                }
                if input {
                    let StorageBinder::Fixed(class) = upper else {
                        return Err(failure(
                            "input retained class was freshened to invocation scope",
                        ));
                    };
                    ordered(b, grounded.len(), 0)?;
                    grounded.insert(class);
                }
                positions.push(slot.path.clone());
            }
        }
        for path in positions {
            expected = expected
                .checked_add(1)
                .ok_or_else(|| failure("signature position count overflow"))?;
            let upper = binding(f, base, &path, b)?;
            let (set, index, limit) = match upper {
                StorageBinder::Fixed(i) => (&mut fixed, i, c.fixed_binder_count),
                StorageBinder::Invocation(i) => (&mut invocation, i, c.invocation_binder_count),
            };
            if index >= limit {
                return Err(failure("signature binder outside declared domain"));
            }
            ordered(b, set.len(), 0)?;
            if set.insert(index) && index as usize != set.len() - 1 {
                return Err(failure(
                    "signature binders are not dense by first occurrence",
                ));
            }
        }
    }
    if expected != c.bindings.len()
        || fixed.len() != c.fixed_binder_count as usize
        || invocation.len() != c.invocation_binder_count as usize
        || fixed != grounded
    {
        return Err(failure(
            "signature has missing/extra positions, unused classes or ungrounded Fixed classes",
        ));
    }
    Ok(())
}

fn returns(f: &TypedFunction, facts: &Facts, b: &mut Budget) -> CompileResult<()> {
    for (path, sources) in &facts.returned {
        if sources.is_empty() {
            return Err(failure(
                "stored/reference result has no actual input target; recursive cycles grant no roots",
            ));
        }
        if f.storage_life.is_none() {
            continue;
        }
        let upper = binding(f, StorageBase::Result, path, b)?;
        for source in sources {
            let HoldRoot::Parameter(index) = source.base else {
                return Err(failure("result retains local/captured target"));
            };
            if binding(f, StorageBase::Parameter(index), &source.stored, b)? != upper {
                return Err(failure(
                    "actual returned target exceeds its declared stored/fresh relation",
                ));
            }
        }
    }
    Ok(())
}

/// This first executable slice admits finite physical shared leaves. Other
/// common retained carriers remain closed instead of acquiring guessed roots.
pub(super) fn physical(ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    let Some((storage, life)) = carrier(ty) else {
        return Ok(false);
    };
    fee(b, life.slots.len().saturating_add(1))?;
    if !matches!(storage, Type::Record { .. } | Type::Tuple(_)) || !life.retained.is_empty() {
        return Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "nested retained/nominal stored carriers require their independent consumer",
            None,
        )]);
    }
    let mut actual = Vec::new();
    fn walk(ty: &Type, path: &mut Path, out: &mut Vec<Path>, b: &mut Budget) -> CompileResult<()> {
        fee(b, 1)?;
        match ty {
            Type::Address {
                mutable: false,
                inner,
            } => {
                b.type_work(inner, true, "stored shared actual pointee")?;
                if !inner.is_address_pointee() {
                    return Err(failure(
                        "stored shared leaf has an ineligible actual pointee",
                    ));
                }
                fee(b, path.len().saturating_add(1))?;
                out.push(path.clone());
            }
            Type::Record { fields, .. } => {
                for (i, (_, t)) in fields.iter().enumerate() {
                    fee(b, 1)?;
                    path.push(PlaceProjection::Field(i as u32));
                    walk(t, path, out, b)?;
                    path.pop();
                }
            }
            Type::Tuple(fields) => {
                for (i, t) in fields.iter().enumerate() {
                    fee(b, 1)?;
                    path.push(PlaceProjection::Field(i as u32));
                    walk(t, path, out, b)?;
                    path.pop();
                }
            }
            t => {
                b.type_work(t, true, "stored ordinary physical field")?;
                if !t.is_copy_value() {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Analyze,
                        "nonshared physical stored leaf is not yet implemented",
                        None,
                    )]);
                }
            }
        }
        Ok(())
    }
    walk(storage, &mut Vec::new(), &mut actual, b)?;
    if actual.len() != life.slots.len() || actual.is_empty() {
        return Err(failure("physical stored slot coverage is incomplete"));
    }
    let mut seen = BTreeSet::new();
    let mut next = 0u32;
    for row in &life.nominal_arguments {
        fee(b, row.path.len().saturating_add(1))?;
        if !row.path.is_empty() || row.binder >= life.binder_count {
            return Err(failure(
                "nested nominal stored relation is not supported by the physical slice",
            ));
        }
        ordered(b, seen.len(), 0)?;
        if seen.insert(row.binder) {
            if row.binder != next {
                return Err(failure(
                    "nominal lifetime binders are not dense by first occurrence",
                ));
            }
            next = next
                .checked_add(1)
                .ok_or_else(|| failure("binder overflow"))?;
        }
    }
    for (path, decl) in actual.iter().zip(&life.slots) {
        fee(b, path.len().saturating_add(decl.path.len()))?;
        if path != &decl.path
            || decl.mode != StoredBorrowMode::SharedAddress
            || decl.binder >= life.binder_count
        {
            return Err(failure(
                "stored slot order, mode or binder differs from actual fields",
            ));
        }
        ordered(b, seen.len(), 0)?;
        if seen.insert(decl.binder) {
            if decl.binder != next {
                return Err(failure("stored binders are not dense by first occurrence"));
            }
            next = next
                .checked_add(1)
                .ok_or_else(|| failure("binder overflow"))?;
        }
    }
    if next != life.binder_count {
        return Err(failure("stored binder domain has unused classes"));
    }
    Ok(true)
}

fn seed(f: &TypedFunction, loans: &mut Vec<Loan>, b: &mut Budget) -> CompileResult<State> {
    let mut state = State::default();
    for (index, local) in f.parameters.iter().enumerate() {
        fee(b, 1)?;
        let ty = &f.locals[local.0 as usize].ty;
        let mut value = Value::default();
        if let Type::Address { mutable, .. } = ty {
            let target = Target {
                base: HoldRoot::Parameter(index as u32),
                stored: Vec::new(),
                place: Vec::new(),
            };
            let id = loans.len();
            fee(b, 2)?;
            loans.push(Loan {
                targets: BTreeSet::from([target.clone()]),
                parents: Loans::new(),
                mutable: *mutable,
            });
            value.pointer = Permission {
                targets: BTreeSet::from([target]),
                loans: Loans::from([id]),
            };
        }
        let qualified = carrier(ty).or_else(|| pointed_carrier(ty));
        if let Some((_, life)) = qualified {
            let mut slots = BTreeMap::new();
            for decl in &life.slots {
                fee(b, decl.path.len().saturating_mul(3).saturating_add(3))?;
                let target = Target {
                    base: HoldRoot::Parameter(index as u32),
                    stored: decl.path.clone(),
                    place: Vec::new(),
                };
                let id = loans.len();
                loans.push(Loan {
                    targets: BTreeSet::from([target.clone()]),
                    parents: Loans::new(),
                    mutable: false,
                });
                slots.insert(
                    decl.path.clone(),
                    Permission {
                        targets: BTreeSet::from([target]),
                        loans: Loans::from([id]),
                    },
                );
            }
            if pointed_carrier(ty).is_some() {
                let mut copy = Value::default();
                for (path, permission) in &slots {
                    ordered(b, copy.slots.len(), path.len())?;
                    fee(b, path.len())?;
                    union(copy.slots.entry(path.clone()).or_default(), permission, b)?;
                }
                state.external.insert(index as u32, copy.slots);
            }
            value.slots = slots;
        }
        state.locals.insert(*local, value);
    }
    Ok(state)
}

fn select(
    state: &State,
    address: &Permission,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> CompileResult<Permission> {
    if address.targets.is_empty() {
        return Err(failure("stored selection has no actual carrier location"));
    }
    let mut out = Permission::default();
    for root in &address.targets {
        fee(b, root.stored.len().saturating_add(1))?;
        if !root.stored.is_empty() || !root.place.is_empty() {
            return Err(failure("stored target is not an inline carrier address"));
        }
        let slots = match root.base {
            HoldRoot::Local(local) => {
                ordered(b, state.locals.len(), 0)?;
                &state
                    .locals
                    .get(&local)
                    .ok_or_else(|| failure("carrier local is unavailable"))?
                    .slots
            }
            HoldRoot::Parameter(index) => {
                ordered(b, state.external.len(), 0)?;
                state
                    .external
                    .get(&index)
                    .ok_or_else(|| failure("external carrier has no actual slot facts"))?
            }
            HoldRoot::Capture(_) => {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Analyze,
                    "captured stored carrier facts are not yet implemented",
                    None,
                )]);
            }
        };
        ordered(b, slots.len(), path.len())?;
        let leaf = slots
            .get(path)
            .ok_or_else(|| failure("stored leaf is unavailable or uninitialized"))?;
        union(&mut out, leaf, b)?;
    }
    if out.targets.is_empty() {
        return Err(failure("stored leaf has no actual target"));
    }
    Ok(out)
}

fn roots(value: &Value, b: &mut Budget) -> CompileResult<Targets> {
    let mut permission = Permission::default();
    union(&mut permission, &value.pointer, b)?;
    for leaf in value.slots.values() {
        union(&mut permission, leaf, b)?;
    }
    Ok(permission.targets)
}

fn substitute(
    source: &Target,
    arguments: &[ValueId],
    values: &[Value],
    state: &State,
    b: &mut Budget,
) -> CompileResult<Permission> {
    let HoldRoot::Parameter(index) = source.base else {
        return Err(failure(
            "callee result retains activation-local or captured storage",
        ));
    };
    let actual = *arguments
        .get(index as usize)
        .ok_or_else(|| failure("callee source parameter has no actual argument"))?;
    let value = values
        .get(actual.0 as usize)
        .ok_or_else(|| failure("call actual is outside values"))?;
    let mut result = if source.stored.is_empty() {
        let mut out = Permission::default();
        union(&mut out, &value.pointer, b)?;
        out
    } else if !value.slots.is_empty() {
        ordered(b, value.slots.len(), source.stored.len())?;
        let mut out = Permission::default();
        union(
            &mut out,
            value
                .slots
                .get(&source.stored)
                .ok_or_else(|| failure("actual carrier lacks the callee's returned leaf"))?,
            b,
        )?;
        out
    } else {
        select(state, &value.pointer, &source.stored, b)?
    };
    if !source.place.is_empty() {
        let mut targets = Targets::new();
        for target in &result.targets {
            fee(
                b,
                target
                    .stored
                    .len()
                    .saturating_add(target.place.len())
                    .saturating_add(source.place.len())
                    .saturating_add(1),
            )?;
            let mut target = target.clone();
            target.place.extend(&source.place);
            ordered(
                b,
                targets.len(),
                target.stored.len().saturating_add(target.place.len()),
            )?;
            targets.insert(target);
        }
        result.targets = targets;
    }
    Ok(result)
}

fn transfer(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &mut State,
    values: &[Value],
    summaries: &[Facts],
    loans: &mut [Loan],
    b: &mut Budget,
) -> CompileResult<Value> {
    fee(b, 1)?;
    let mut out = Value::default();
    let get = |id: ValueId| {
        values
            .get(id.0 as usize)
            .ok_or_else(|| failure("operand is outside actual values"))
    };
    match &i.kind {
        InstructionKind::Load(local) => {
            ordered(b, state.locals.len(), 0)?;
            out = snapshot(
                state
                    .locals
                    .get(local)
                    .ok_or_else(|| failure("read of unavailable carrier/local"))?,
                b,
            )?;
        }
        InstructionKind::Store { local, value } => {
            ordered(b, state.locals.len(), 0)?;
            state.locals.insert(*local, snapshot(get(*value)?, b)?);
        }
        InstructionKind::StorageLive(local) | InstructionKind::StorageDead(local) => {
            ordered(b, state.locals.len(), 0)?;
            state.locals.remove(local);
        }
        InstructionKind::BorrowLocal { local } | InstructionKind::BorrowLocalMut { local } => {
            fee(b, 1)?;
            out.pointer.targets.insert(Target {
                base: HoldRoot::Local(*local),
                stored: Vec::new(),
                place: Vec::new(),
            });
            if carrier(&f.locals[local.0 as usize].ty).is_some() {
                ordered(b, state.locals.len(), 0)?;
                let held = state
                    .locals
                    .get(local)
                    .ok_or_else(|| failure("borrowed carrier is unavailable"))?;
                for (path, permission) in &held.slots {
                    ordered(b, out.slots.len(), path.len())?;
                    fee(b, path.len())?;
                    union(out.slots.entry(path.clone()).or_default(), permission, b)?;
                }
            }
        }
        InstructionKind::AddressField { address, index } => {
            union(&mut out.pointer, &get(*address)?.pointer, b)?;
            let mut targets = Targets::new();
            for target in &out.pointer.targets {
                fee(
                    b,
                    target
                        .stored
                        .len()
                        .saturating_add(target.place.len())
                        .saturating_add(2),
                )?;
                let mut target = target.clone();
                target.place.push(PlaceProjection::Field(*index));
                ordered(
                    b,
                    targets.len(),
                    target.stored.len().saturating_add(target.place.len()),
                )?;
                targets.insert(target);
            }
            out.pointer.targets = targets;
        }
        InstructionKind::ReborrowAddress { address, .. }
        | InstructionKind::AddressIndex { address, .. }
        | InstructionKind::AddressSlice { address, .. } => {
            union(&mut out.pointer, &get(*address)?.pointer, b)?;
        }
        InstructionKind::MakeAggregate { fields } => {
            let ty = value_type(
                f,
                i.result.ok_or_else(|| failure("aggregate lacks result"))?,
            )?;
            let Some((storage, life)) = carrier(ty) else {
                return Ok(out);
            };
            for decl in &life.slots {
                let Some((PlaceProjection::Field(index), tail)) = decl.path.split_first() else {
                    return Err(failure("aggregate slot has no physical field"));
                };
                let source = get(*fields
                    .get(*index as usize)
                    .ok_or_else(|| failure("aggregate omits borrowed field"))?)?;
                let mut permission = Permission::default();
                if tail.is_empty() {
                    union(&mut permission, &source.pointer, b)?;
                } else {
                    ordered(b, source.slots.len(), tail.len())?;
                    union(
                        &mut permission,
                        source
                            .slots
                            .get(tail)
                            .ok_or_else(|| failure("nested aggregate omits stored field"))?,
                        b,
                    )?;
                }
                if permission.targets.is_empty() {
                    return Err(failure("constructed stored field has no actual target"));
                }
                if !matches!(
                    slot(storage, &decl.path, b)?,
                    Type::Address { mutable: false, .. }
                ) {
                    return Err(failure("constructed stored field is not shared"));
                }
                ordered(b, out.slots.len(), decl.path.len())?;
                fee(b, decl.path.len())?;
                out.slots.insert(decl.path.clone(), permission);
            }
        }
        InstructionKind::LoadStoredBorrow {
            storage,
            path,
            mode: StoredBorrowLoad::CopyShared,
        } => {
            out.pointer = select(state, &get(*storage)?.pointer, path, b)?;
        }
        InstructionKind::LoadStoredBorrow { .. } | InstructionKind::StoreStoredBorrow { .. } => {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "stored reborrow/take/replacement CFG consumer is not yet implemented",
                Some(i.span),
            )]);
        }
        InstructionKind::Call {
            function,
            arguments,
        } => {
            let callee = module
                .functions
                .get(function.0 as usize)
                .ok_or_else(|| failure("call target absent"))?;
            let summary = summaries
                .get(function.0 as usize)
                .ok_or_else(|| failure("callee stored summary absent"))?;
            for (path, sources) in &summary.returned {
                let mut permission = Permission::default();
                for source in sources {
                    union(
                        &mut permission,
                        &substitute(source, arguments, values, state, b)?,
                        b,
                    )?;
                }
                if path.is_empty() {
                    out.pointer = permission;
                } else {
                    fee(b, path.len())?;
                    out.slots.insert(path.clone(), permission);
                }
            }
            if callee.foreign.is_some()
                || callee.environment.is_some()
                || callee.call_kind != CallKind::Direct
            {
                return Err(failure(
                    "stored call requires an independently checked ordinary direct body",
                ));
            }
        }
        InstructionKind::Constant(_)
        | InstructionKind::Unary { .. }
        | InstructionKind::Binary { .. }
        | InstructionKind::ReadAddress { .. }
        | InstructionKind::WriteAddress { .. }
        | InstructionKind::SliceLength { .. }
        | InstructionKind::ScopeEnter { .. }
        | InstructionKind::ScopeExit { .. } => {}
        _ => {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "instruction has no common stored-target transfer yet",
                Some(i.span),
            )]);
        }
    }
    if let Some(result) = i.result
        && matches!(
            i.kind,
            InstructionKind::BorrowLocal { .. }
                | InstructionKind::BorrowLocalMut { .. }
                | InstructionKind::ReborrowAddress { .. }
                | InstructionKind::AddressField { .. }
                | InstructionKind::AddressIndex { .. }
                | InstructionKind::AddressSlice { .. }
        )
    {
        let id = result.0 as usize;
        let loan = loans
            .get_mut(id)
            .ok_or_else(|| failure("fresh loan is outside actual values"))?;
        let mut targets = Permission {
            targets: std::mem::take(&mut loan.targets),
            loans: std::mem::take(&mut loan.parents),
        };
        union(&mut targets, &out.pointer, b)?;
        loan.targets = targets.targets;
        loan.parents = targets.loans;
        loan.mutable = matches!(value_type(f, result)?, Type::Address { mutable: true, .. });
        out.pointer.loans.clear();
        fee(b, 1)?;
        out.pointer.loans.insert(id);
    }
    Ok(out)
}

fn intersects(left: &Target, right: &Target, b: &mut Budget) -> CompileResult<bool> {
    fee(
        b,
        left.stored
            .len()
            .saturating_add(right.stored.len())
            .saturating_add(left.place.len())
            .saturating_add(right.place.len())
            .saturating_add(1),
    )?;
    if left.base != right.base || left.stored != right.stored {
        return Ok(false);
    }
    Ok(left.place.starts_with(&right.place) || right.place.starts_with(&left.place))
}
fn may_alias(left: &Target, right: &Target) -> bool {
    matches!(
        (left.base, right.base),
        (HoldRoot::Parameter(_), HoldRoot::Parameter(_))
    ) && (left.base != right.base || left.stored != right.stored)
}
fn separate(
    left: &Target,
    right: &Target,
    apart: &mut BTreeSet<(Target, Target)>,
    b: &mut Budget,
) -> CompileResult<()> {
    if intersects(left, right, b)? {
        return Err(failure(
            "simultaneously live access overlaps an actual stored target/parent loan",
        ));
    }
    if may_alias(left, right) {
        let width = left
            .stored
            .len()
            .saturating_add(right.stored.len())
            .saturating_add(left.place.len())
            .saturating_add(right.place.len())
            .saturating_add(2);
        ordered(b, apart.len(), width)?;
        fee(b, width)?;
        let pair = if left < right {
            (left.clone(), right.clone())
        } else {
            (right.clone(), left.clone())
        };
        apart.insert(pair);
    }
    Ok(())
}
fn ancestry(ids: &Loans, loans: &[Loan], b: &mut Budget) -> CompileResult<Loans> {
    let mut out = Loans::new();
    let mut pending = Vec::new();
    fee(b, ids.len())?;
    pending.extend(ids);
    while let Some(id) = pending.pop() {
        ordered(b, out.len(), 0)?;
        if out.insert(id) {
            let loan = loans
                .get(id)
                .ok_or_else(|| failure("loan ancestry is outside its actual table"))?;
            fee(b, loan.parents.len())?;
            pending.extend(&loan.parents);
        }
    }
    Ok(out)
}
fn use_permission(
    permission: &Permission,
    write: bool,
    active: &Loans,
    loans: &[Loan],
    apart: &mut BTreeSet<(Target, Target)>,
    b: &mut Budget,
) -> CompileResult<()> {
    if permission.targets.is_empty() {
        return Err(failure("address access has no actual target permission"));
    }
    let ignore = ancestry(&permission.loans, loans, b)?;
    for id in active {
        ordered(b, ignore.len(), 0)?;
        if ignore.contains(id) {
            continue;
        }
        let loan = loans.get(*id).ok_or_else(|| failure("live loan absent"))?;
        if write || loan.mutable {
            for target in &permission.targets {
                for held in &loan.targets {
                    separate(target, held, apart, b)?;
                }
            }
        }
    }
    Ok(())
}
fn active(
    values: &[Value],
    state: &State,
    live: &[bool],
    loans: &[Loan],
    b: &mut Budget,
) -> CompileResult<Loans> {
    let mut direct = Permission::default();
    fee(b, live.len())?;
    for (i, yes) in live.iter().enumerate() {
        if !*yes {
            continue;
        }
        let value = if i < values.len() {
            Some(&values[i])
        } else {
            ordered(b, state.locals.len(), 0)?;
            state.locals.get(&LocalId((i - values.len()) as u32))
        };
        if let Some(value) = value {
            union(&mut direct, &value.pointer, b)?;
            for leaf in value.slots.values() {
                union(&mut direct, leaf, b)?;
            }
        }
    }
    ancestry(&direct.loans, loans, b)
}
fn reverse(
    f: &TypedFunction,
    i: &Instruction,
    live: &mut [bool],
    b: &mut Budget,
) -> CompileResult<()> {
    fee(b, 1)?;
    if let Some(v) = i.result {
        live[v.0 as usize] = false;
    }
    match i.kind {
        InstructionKind::Store { local, .. }
        | InstructionKind::StorageLive(local)
        | InstructionKind::StorageDead(local) => live[f.values.len() + local.0 as usize] = false,
        InstructionKind::Load(local)
        | InstructionKind::BorrowLocal { local }
        | InstructionKind::BorrowLocalMut { local } => {
            live[f.values.len() + local.0 as usize] = true
        }
        _ => {}
    }
    for v in resources::uses(&i.kind) {
        fee(b, 1)?;
        live[v.0 as usize] = true;
    }
    Ok(())
}
fn term_live(t: &Terminator, live: &mut [bool]) {
    match t {
        Terminator::Return(Some(v)) | Terminator::Branch { condition: v, .. } => {
            live[v.0 as usize] = true
        }
        _ => {}
    }
}
fn liveness(
    f: &TypedFunction,
    preds: &[Vec<usize>],
    reachable: &[bool],
    b: &mut Budget,
) -> CompileResult<Vec<Vec<bool>>> {
    let width = f
        .values
        .len()
        .checked_add(f.locals.len())
        .ok_or_else(|| failure("stored liveness width overflow"))?;
    b.product(f.blocks.len(), width, 2, "stored liveness tables")?;
    let mut inputs = vec![vec![false; width]; f.blocks.len()];
    let mut outputs = inputs.clone();
    loop {
        let mut changed = false;
        for bi in (0..f.blocks.len()).rev() {
            fee(b, 1)?;
            if !reachable[bi] {
                continue;
            }
            fee(b, width)?;
            let mut live = outputs[bi].clone();
            term_live(&f.blocks[bi].terminator, &mut live);
            for i in f.blocks[bi].instructions.iter().rev() {
                reverse(f, i, &mut live, b)?;
            }
            fee(b, width)?;
            if live != inputs[bi] {
                fee(b, width)?;
                inputs[bi] = live;
                for pred in &preds[bi] {
                    fee(b, width)?;
                    for (to, from) in outputs[*pred].iter_mut().zip(&inputs[bi]) {
                        if *from && !*to {
                            *to = true;
                            changed = true;
                        }
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    Ok(outputs)
}

#[allow(clippy::too_many_arguments)]
fn windows(
    module: &TypedModule,
    f: &TypedFunction,
    preds: &[Vec<usize>],
    reachable: &[bool],
    entry: &State,
    outputs: &[State],
    values: &[Value],
    loans: &mut [Loan],
    summaries: &[Facts],
    apart: &mut BTreeSet<(Target, Target)>,
    b: &mut Budget,
) -> CompileResult<()> {
    let live_out = liveness(f, preds, reachable, b)?;
    for (bi, block) in f.blocks.iter().enumerate() {
        fee(b, 1)?;
        if !reachable[bi] {
            continue;
        }
        let mut state = if bi == f.entry.0 as usize {
            state_copy(entry, b)?
        } else {
            State::default()
        };
        for pred in &preds[bi] {
            fee(b, 1)?;
            if reachable[*pred] {
                state_merge(&mut state, &outputs[*pred], b)?;
            }
        }
        let width = f.values.len().saturating_add(f.locals.len());
        b.product(
            block.instructions.len().saturating_add(1),
            width,
            1,
            "stored instruction liveness snapshots",
        )?;
        let mut live = live_out[bi].clone();
        term_live(&block.terminator, &mut live);
        let mut after = vec![vec![false; width]; block.instructions.len()];
        for (position, i) in block.instructions.iter().enumerate().rev() {
            after[position].clone_from(&live);
            reverse(f, i, &mut live, b)?;
        }
        for (position, i) in block.instructions.iter().enumerate() {
            let mut live = after[position].clone();
            fee(b, width)?;
            if let Some(v) = i.result {
                live[v.0 as usize] = false;
            }
            let mut current = active(values, &state, &live, loans, b)?;
            if let Some(v) = i.result {
                current.remove(&(v.0 as usize));
            }
            let pointer = |v: ValueId| -> CompileResult<&Permission> {
                values
                    .get(v.0 as usize)
                    .map(|v| &v.pointer)
                    .ok_or_else(|| failure("access operand outside values"))
            };
            match &i.kind {
                InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local }
                | InstructionKind::Store { local, .. }
                | InstructionKind::StorageDead(local) => {
                    let mut actual = Permission::default();
                    actual.targets.insert(Target {
                        base: HoldRoot::Local(*local),
                        stored: Vec::new(),
                        place: Vec::new(),
                    });
                    let write = !matches!(i.kind, InstructionKind::BorrowLocal { .. });
                    use_permission(&actual, write, &current, loans, apart, b)?;
                }
                InstructionKind::ReadAddress { address }
                | InstructionKind::SliceLength { address }
                | InstructionKind::ReborrowAddress {
                    address,
                    mutable: false,
                } => use_permission(pointer(*address)?, false, &current, loans, apart, b)?,
                InstructionKind::AddressIndex { address, .. }
                | InstructionKind::AddressSlice { address, .. }
                | InstructionKind::AddressField { address, .. } => {
                    let write = i.result.is_some_and(|v| {
                        matches!(f.values[v.0 as usize], Type::Address { mutable: true, .. })
                    });
                    use_permission(pointer(*address)?, write, &current, loans, apart, b)?;
                }
                InstructionKind::LoadStoredBorrow { storage, path, .. } => {
                    use_permission(pointer(*storage)?, false, &current, loans, apart, b)?;
                    let actual = select(&state, pointer(*storage)?, path, b)?;
                    use_permission(&actual, false, &current, loans, apart, b)?;
                }
                InstructionKind::WriteAddress { address, .. }
                | InstructionKind::ReborrowAddress {
                    address,
                    mutable: true,
                } => use_permission(pointer(*address)?, true, &current, loans, apart, b)?,
                InstructionKind::Call {
                    function,
                    arguments,
                } => {
                    let callee = &module.functions[function.0 as usize];
                    for (a, (argument, parameter)) in
                        arguments.iter().zip(&callee.parameters).enumerate()
                    {
                        let actual = &values[argument.0 as usize];
                        let ty = &callee.locals[parameter.0 as usize].ty;
                        if let Type::Address { mutable, .. } = ty {
                            use_permission(&actual.pointer, *mutable, &current, loans, apart, b)?;
                        }
                        for leaf in actual.slots.values() {
                            use_permission(leaf, false, &current, loans, apart, b)?;
                        }
                        for other in &arguments[..a] {
                            let right = &values[other.0 as usize];
                            let write = matches!(
                                value_type(f, *argument)?,
                                Type::Address { mutable: true, .. }
                            ) || matches!(
                                value_type(f, *other)?,
                                Type::Address { mutable: true, .. }
                            );
                            if write {
                                for x in &actual.pointer.targets {
                                    for y in &right.pointer.targets {
                                        separate(x, y, apart, b)?;
                                    }
                                }
                            }
                            if matches!(
                                value_type(f, *argument)?,
                                Type::Address { mutable: true, .. }
                            ) {
                                for leaf in right.slots.values() {
                                    for x in &actual.pointer.targets {
                                        for y in &leaf.targets {
                                            separate(x, y, apart, b)?;
                                        }
                                    }
                                }
                            }
                            if matches!(value_type(f, *other)?, Type::Address { mutable: true, .. })
                            {
                                for leaf in actual.slots.values() {
                                    for x in &leaf.targets {
                                        for y in &right.pointer.targets {
                                            separate(x, y, apart, b)?;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    for (left, right) in &summaries[function.0 as usize].apart {
                        let left = substitute(left, arguments, values, &state, b)?;
                        let right = substitute(right, arguments, values, &state, b)?;
                        if left.targets.is_empty() || right.targets.is_empty() {
                            return Err(failure(
                                "callee separation lacks complete actual slot facts",
                            ));
                        }
                        for x in &left.targets {
                            for y in &right.targets {
                                separate(x, y, apart, b)?;
                            }
                        }
                    }
                }
                _ => {}
            }
            transfer(module, f, i, &mut state, values, summaries, loans, b)?;
        }
    }
    Ok(())
}

/// Path-aware return solving is separate from the legacy parameter-only
/// reference summaries. No declared life bound fabricates a nonempty result.
pub(super) fn derive(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    b: &mut Budget,
) -> CompileResult<Vec<Facts>> {
    fee(b, module.functions.len())?;
    let mut summaries = vec![Facts::default(); module.functions.len()];
    loop {
        let mut changed = false;
        for (index, f) in module.functions.iter().enumerate() {
            let (preds, reachable) = &graphs[index];
            fee(b, f.values.len().saturating_add(f.blocks.len()))?;
            let mut loans = vec![Loan::default(); f.values.len()];
            let entry = seed(f, &mut loans, b)?;
            let mut values = vec![Value::default(); f.values.len()];
            let mut outputs = vec![State::default(); f.blocks.len()];
            loop {
                let mut local_changed = false;
                for (bi, block) in f.blocks.iter().enumerate() {
                    fee(b, 1)?;
                    if !reachable[bi] {
                        continue;
                    }
                    let mut state = if bi == f.entry.0 as usize {
                        state_copy(&entry, b)?
                    } else {
                        State::default()
                    };
                    for pred in &preds[bi] {
                        fee(b, 1)?;
                        if reachable[*pred] {
                            state_merge(&mut state, &outputs[*pred], b)?;
                        }
                    }
                    for instruction in &block.instructions {
                        let out = transfer(
                            module,
                            f,
                            instruction,
                            &mut state,
                            &values,
                            &summaries,
                            &mut loans,
                            b,
                        )?;
                        if let Some(id) = instruction.result {
                            local_changed |= merge(&mut values[id.0 as usize], &out, b)?;
                        }
                    }
                    fee(
                        b,
                        state.locals.len().saturating_add(outputs[bi].locals.len()),
                    )?;
                    if state != outputs[bi] {
                        outputs[bi] = state;
                        local_changed = true;
                    }
                }
                if !local_changed {
                    break;
                }
            }
            let mut actual = Facts {
                values: Vec::with_capacity(values.len()),
                returned: BTreeMap::new(),
                apart: BTreeSet::new(),
            };
            for value in &values {
                let root = roots(value, b)?;
                fee(b, root.len())?;
                actual.values.push(root.into_iter().collect());
            }
            for (block, live) in f.blocks.iter().zip(reachable) {
                fee(b, 1)?;
                if *live && let Terminator::Return(Some(id)) = block.terminator {
                    let value = &values[id.0 as usize];
                    let mut add = |path: &Path, permission: &Permission| -> CompileResult<()> {
                        ordered(b, actual.returned.len(), path.len())?;
                        fee(b, path.len())?;
                        let row = actual.returned.entry(path.clone()).or_default();
                        for target in &permission.targets {
                            fee(
                                b,
                                target
                                    .stored
                                    .len()
                                    .saturating_mul(row.len().saturating_add(1))
                                    .saturating_add(1),
                            )?;
                            if !matches!(target.base, HoldRoot::Parameter(_)) {
                                return Err(failure(
                                    "stored return would retain an activation-local target",
                                ));
                            }
                            if !row.contains(target) {
                                row.push(target.clone());
                            }
                        }
                        Ok(())
                    };
                    if matches!(f.return_type, Type::Address { .. }) {
                        add(&Vec::new(), &value.pointer)?;
                    }
                    for (path, permission) in &value.slots {
                        add(path, permission)?;
                    }
                }
            }
            windows(
                module,
                f,
                preds,
                reachable,
                &entry,
                &outputs,
                &values,
                &mut loans,
                &summaries,
                &mut actual.apart,
                b,
            )?;
            if actual != summaries[index] {
                summaries[index] = actual;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    for (f, facts) in module.functions.iter().zip(&summaries) {
        returns(f, facts, b)?;
    }
    Ok(summaries)
}
