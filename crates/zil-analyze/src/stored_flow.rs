//! Actual stored targets are distinct from the bytes containing their pointers.
//! This pass does not use a candidate, source label, or nominal name as a root.
use super::*;

mod nominal;
pub(super) mod option_address;
mod retained;
use std::collections::BTreeMap;
pub(super) mod owned;
#[path = "stored_flow_pure_copy.rs"]
mod pure_copy;

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
    option_variants: option_address::Variants,
    pointer: Permission,
    slots: BTreeMap<Path, Permission>,
    held: Permission,
    /// Capture ordinal followed by its pointee's stored-leaf path. These are
    /// symbolic environment payload selectors, never physical slot authority.
    environment: BTreeMap<Path, Permission>,
    creation_complete: Option<bool>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct State {
    locals: BTreeMap<LocalId, Value>,
    /// External addressed carriers have real cells too. Parameter position is
    /// not their target identity; each leaf starts with a symbolic target.
    external: BTreeMap<HoldRoot, BTreeMap<Path, Permission>>,
    captures: BTreeMap<u32, Value>,
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
    complete: bool,
    returned_variants: option_address::Variants,
    none_values: BTreeSet<ValueId>,
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
fn environment_merge(to: &mut Value, from: &Value, b: &mut Budget) -> CompileResult<bool> {
    let mut changed = union(&mut to.held, &from.held, b)?;
    for (path, permission) in &from.environment {
        ordered(b, to.environment.len(), path.len())?;
        fee(b, path.len().saturating_add(1))?;
        changed |= union(
            to.environment.entry(path.clone()).or_default(),
            permission,
            b,
        )?;
    }
    Ok(changed)
}
fn merge(to: &mut Value, from: &Value, b: &mut Budget) -> CompileResult<bool> {
    let mut changed = union(&mut to.pointer, &from.pointer, b)?;
    changed |= to.option_variants.join(&from.option_variants, b)?;
    changed |= environment_merge(to, from, b)?;
    if let Some(complete) = from.creation_complete {
        let joined = Some(to.creation_complete.unwrap_or(false) || complete);
        changed |= to.creation_complete != joined;
        to.creation_complete = joined;
    }
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
    fee(
        b,
        s.locals
            .len()
            .saturating_add(s.external.len())
            .saturating_add(s.captures.len()),
    )?;
    let mut out = State::default();
    for (local, value) in &s.locals {
        ordered(b, out.locals.len(), 0)?;
        out.locals.insert(*local, snapshot(value, b)?);
    }
    for (index, value) in &s.captures {
        ordered(b, out.captures.len(), 0)?;
        out.captures.insert(*index, snapshot(value, b)?);
    }
    for (index, slots) in &s.external {
        let mut charged = Value::default();
        for (path, value) in slots {
            ordered(b, charged.slots.len(), path.len())?;
            fee(b, path.len())?;
            union(charged.slots.entry(path.clone()).or_default(), value, b)?;
        }
        ordered(b, out.external.len(), 0)?;
        out.external.insert(*index, charged.slots);
    }
    Ok(out)
}
fn state_work(s: &State, b: &mut Budget) -> CompileResult<()> {
    fee(
        b,
        s.locals
            .len()
            .saturating_add(s.external.len())
            .saturating_add(s.captures.len()),
    )?;
    for value in s.locals.values().chain(s.captures.values()) {
        value.option_variants.work(b)?;
        target_work(&value.held.targets, b)?;
        fee(b, value.held.loans.len())?;
        target_work(&value.pointer.targets, b)?;
        fee(b, value.pointer.loans.len())?;
        fee(b, value.environment.len())?;
        for (path, permission) in value.environment.iter().chain(&value.slots) {
            fee(
                b,
                path.len()
                    .saturating_add(permission.loans.len())
                    .saturating_add(1),
            )?;
            target_work(&permission.targets, b)?;
        }
    }
    for slots in s.external.values() {
        fee(b, slots.len())?;
        for (path, permission) in slots {
            fee(
                b,
                path.len()
                    .saturating_add(permission.loans.len())
                    .saturating_add(1),
            )?;
            target_work(&permission.targets, b)?;
        }
    }
    Ok(())
}

fn facts_work(f: &Facts, b: &mut Budget) -> CompileResult<()> {
    f.returned_variants.work(b)?;
    if !f.none_values.is_empty() {
        fee(b, f.none_values.len())?;
    }

    fee(
        b,
        f.values
            .len()
            .saturating_add(f.returned.len())
            .saturating_add(f.apart.len()),
    )?;
    for row in &f.values {
        for root in row {
            fee(
                b,
                root.stored
                    .len()
                    .saturating_add(root.place.len())
                    .saturating_add(1),
            )?;
        }
    }
    for (path, row) in &f.returned {
        fee(b, path.len().saturating_add(row.len()))?;
        for root in row {
            fee(
                b,
                root.stored
                    .len()
                    .saturating_add(root.place.len())
                    .saturating_add(1),
            )?;
        }
    }
    for (left, right) in &f.apart {
        fee(
            b,
            left.stored
                .len()
                .saturating_add(left.place.len())
                .saturating_add(right.stored.len())
                .saturating_add(right.place.len())
                .saturating_add(2),
        )?;
    }
    Ok(())
}
fn state_merge(to: &mut State, from: &State, b: &mut Budget) -> CompileResult<()> {
    for (local, value) in &from.locals {
        ordered(b, to.locals.len(), 0)?;
        merge(to.locals.entry(*local).or_default(), value, b)?;
    }
    for (index, value) in &from.captures {
        ordered(b, to.captures.len(), 0)?;
        merge(to.captures.entry(*index).or_default(), value, b)?;
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

pub(super) fn shape_work(ty: &Type, limits: &Limits, b: &mut Budget) -> CompileResult<()> {
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
            Type::AsyncRegistration { environment: inner }
            | Type::Address { inner, .. }
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

pub(super) fn qualification(
    module: &TypedModule,
    ty: &Type,
    limits: &Limits,
    b: &mut Budget,
) -> CompileResult<()> {
    shape_work(ty, limits, b)?;
    shape_work(ty, limits, b)?;
    physical(ty, b)?;
    let (storage, _) = carrier(ty).ok_or_else(|| failure("expected a qualified carrier"))?;
    if matches!(storage, Type::Record { .. } | Type::Enum { .. }) {
        fee(b, module.life_instances.len())?;
        let mut present = false;
        for instance in &module.life_instances {
            shape_work(&instance.ty, limits, b)?;
            shape_work(ty, limits, b)?;
            if instance.ty == *ty {
                present = true;
            }
        }
        if !present {
            return Err(failure(
                "nominal carrier has no exact complete declaration/instance correspondence",
            ));
        }
    }
    Ok(())
}

/// Copy only a fully qualified physical value without runtime borrowing or
/// ownership leaves. The address is still checked by the CFG access proof.
pub(super) fn pure_read(module: &TypedModule, ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    if !b.pure_nominal
        || !matches!(ty, Type::BorrowedStorage { storage, .. } if matches!(storage.as_ref(), Type::Record { .. } | Type::Enum { .. }))
    {
        return Ok(false);
    }
    b.type_work(ty, true, "pure stored ReadAddress complete payload")?;
    if !retained::pure(ty, b)? {
        return Ok(false);
    }
    physical(ty, b)?;
    let mut count = 0usize;
    for instance in &module.life_instances {
        b.type_work(
            &instance.ty,
            false,
            "pure stored ReadAddress instance identity",
        )?;
        b.type_work(ty, false, "pure stored ReadAddress actual identity")?;
        if instance.ty == *ty {
            count += 1;
        }
    }
    if count != 1 {
        return Err(failure(
            "pure stored read lacks one complete nominal instance",
        ));
    }
    Ok(true)
}

/// Pure inline products have no runtime targets. Every qualified descendant
/// still needs one complete, independently checked nominal instance.
pub(super) fn pure_inline(module: &TypedModule, ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    if !b.pure_nominal
        || ty.is_copy_value()
        || !matches!(
            ty,
            Type::Array { .. } | Type::Tuple(_) | Type::Record { .. } | Type::Enum { .. }
        )
        || !retained::pure(ty, b)?
    {
        return Ok(false);
    }
    b.type_work(ty, true, "pure inline complete payload")?;
    let mut pending = vec![ty];
    while let Some(current) = pending.pop() {
        fee(b, 1)?;
        match current {
            Type::BorrowedStorage { storage, .. } => {
                physical(current, b)?;
                let mut count = 0usize;
                for instance in &module.life_instances {
                    b.type_work(&instance.ty, false, "pure inline nominal identity")?;
                    b.type_work(current, false, "pure inline actual identity")?;
                    if instance.ty == *current {
                        count += 1;
                    }
                }
                if count != 1 {
                    return Err(failure(
                        "pure inline child has no unique complete nominal instance",
                    ));
                }
                pending.push(storage);
            }
            Type::Array { element, .. } => pending.push(element),
            Type::Tuple(fields) => pending.extend(fields),
            Type::Record { fields, .. } => pending.extend(fields.iter().map(|(_, t)| t)),
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    pending.extend(fields);
                }
            }
            Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::U8
            | Type::U16
            | Type::F32
            | Type::F64 => {}
            _ => return Ok(false),
        }
    }
    Ok(true)
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
    if module.life_declarations.len() > limits.max_nodes
        || module.life_instances.len() > limits.max_nodes
    {
        return Err(vec![Diagnostic::limit(
            Phase::Analyze,
            "stored inventory count exceeds analysis limit",
            None,
        )]);
    }
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
        if matches!(&instance.ty, Type::BorrowedStorage { life, .. } if life.slots.is_empty()) {
            b.pure_nominal = true;
        }
        let Some((storage, _)) = carrier(&instance.ty) else {
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
                | (LifeNominalKind::Enum, Type::Enum { .. })
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
        nominal::rows(module, instance, limits, b)?;
        for earlier in &module.life_instances[..index] {
            shape_work(&earlier.ty, limits, b)?;
            shape_work(&instance.ty, limits, b)?;
            if earlier.ty == instance.ty
                || (earlier.declaration == instance.declaration
                    && nominal::same_key(earlier, instance, limits, b)?)
            {
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
        .ok_or_else(|| failure("stored signature lacks complete relation"))?;
    if path.is_empty() {
        fee(b, c.bindings.len())?;
        return super::signature_life::outer(c, base)
            .ok_or_else(|| failure("signature omits actual outer borrow"));
    }
    let ty = match base {
        StorageBase::Parameter(i) => f
            .parameters
            .get(i as usize)
            .and_then(|l| f.locals.get(l.0 as usize))
            .map(|l| &l.ty),
        StorageBase::Result => Some(&f.return_type),
        StorageBase::Capture(_) => None,
    }
    .ok_or_else(|| failure("stored relation base is absent"))?;
    let ((_, shape), pointee) = if let Some(q) = carrier(ty) {
        (q, false)
    } else {
        (
            pointed_carrier(ty)
                .ok_or_else(|| failure("physical leaf has no actual qualified boundary"))?,
            true,
        )
    };
    fee(b, shape.slots.len())?;
    let mut local = None;
    for slot in &shape.slots {
        fee(
            b,
            slot.path.len().saturating_add(path.len()).saturating_add(1),
        )?;
        if slot.path == path && local.replace(slot.binder).is_some() {
            return Err(failure("duplicate actual stored leaf"));
        }
    }
    let local = local.ok_or_else(|| failure("relation selected an absent physical stored leaf"))?;
    fee(b, c.classes.len())?;
    for row in &c.classes {
        fee(b, row.position.path.len().saturating_add(1))?;
        if row.position.base == base
            && row.position.class == local
            && if pointee {
                row.position.path.as_slice() == [LifeTypeProjection::Pointee]
            } else {
                row.position.path.is_empty()
            }
        {
            return Ok(row.binder);
        }
    }
    Err(failure("signature omits actual qualified local class"))
}

pub(super) fn header(module: &TypedModule, f: &TypedFunction, b: &mut Budget) -> CompileResult<()> {
    fn nested_class(ty: &Type, depth: usize, b: &mut Budget) -> CompileResult<bool> {
        if depth > b.partial_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Analyze,
                "stored signature class depth exceeds limit",
                None,
            )]);
        }
        b.spend(1, "stored signature actual nested class inventory")?;
        match ty {
            Type::BorrowedStorage { .. } => Ok(true),
            Type::Address { inner, .. } | Type::Array { element: inner, .. } => {
                nested_class(inner, depth + 1, b)
            }
            Type::Tuple(fields) => {
                b.spend(fields.len(), "stored signature tuple class inventory")?;
                for field in fields {
                    if nested_class(field, depth + 1, b)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Type::Record { fields, .. } => {
                b.spend(fields.len(), "stored signature record class inventory")?;
                for (_, field) in fields {
                    if nested_class(field, depth + 1, b)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Type::Enum { variants, .. } => {
                b.spend(variants.len(), "stored signature variant class inventory")?;
                for (_, fields) in variants {
                    b.spend(fields.len(), "stored signature payload class inventory")?;
                    for field in fields {
                        if nested_class(field, depth + 1, b)? {
                            return Ok(true);
                        }
                    }
                }
                Ok(false)
            }
            _ => Ok(false),
        }
    }
    for p in &f.parameters {
        fee(b, 1)?;
        if f.locals.get(p.0 as usize).is_none() {
            return Err(failure("signature parameter outside actual local table"));
        }
    }
    if super::stored_callables::fixed_value(module, &f.return_type) {
        return Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "captured Fixed environment escape needs its retained result target transfer",
            Some(f.span),
        )]);
    }
    let Some(c) = &f.storage_life else {
        for ty in f
            .parameters
            .iter()
            .map(|p| &f.locals[p.0 as usize].ty)
            .chain(std::iter::once(&f.return_type))
        {
            fee(b, 1)?;
            if f.environment.is_none()
                && (carrier(ty).is_some()
                    || pointed_carrier(ty).is_some()
                    || super::stored_callables::fixed_value(module, ty))
            {
                return Err(failure("stored signature requires complete class relation"));
            }
            let composite = |ty: &Type| {
                matches!(
                    ty,
                    Type::Array { .. } | Type::Tuple(_) | Type::Record { .. } | Type::Enum { .. }
                )
            };
            if !module.life_instances.is_empty()
                && (composite(ty) || matches!(ty, Type::Address { inner, .. } if composite(inner)))
                && nested_class(ty, 0, b)?
            {
                return Err(failure(
                    "nested stored qualified signature requires complete class relation",
                ));
            }
        }
        return Ok(());
    };
    for ty in f
        .parameters
        .iter()
        .map(|p| &f.locals[p.0 as usize].ty)
        .chain(std::iter::once(&f.return_type))
    {
        if option_address::candidate(ty) {
            fee(b, 96)?;
        }
    }
    super::signature_life::charge(c, b)?;
    fee(b, f.parameters.len())?;
    if f.reference_return_bound.is_some() || f.foreign.is_some() {
        return Err(failure(
            "stored signature contradicts another active profile",
        ));
    }
    let mut parameters = Vec::with_capacity(f.parameters.len());
    for p in &f.parameters {
        let ty = &f.locals[p.0 as usize].ty;
        b.type_work(ty, true, "stored signature type clone and relation")?;
        let new_nominal = |ty: &Type| {
            matches!(ty, Type::BorrowedStorage { storage, life }
            if life.slots.is_empty()
                && (!matches!(storage.as_ref(), Type::Record { .. } | Type::Tuple(_))
                    || !life.retained.is_empty()))
        };
        if (matches!(ty, Type::Array { .. }) && !ty.is_copy_value())
            || new_nominal(ty)
            || matches!(ty, Type::Address { inner, .. }
                if (matches!(inner.as_ref(), Type::Array { .. }) && !inner.is_copy_value())
                    || new_nominal(inner))
        {
            b.type_work(ty, false, "pure Array signature relation walk")?;
        }
        if f.callable_contract.is_some()
            && let Type::Managed(payload) = ty
            && let Type::Record { fields, .. } = payload.as_ref()
        {
            fee(b, fields.len().saturating_mul(2))?;
        }
        parameters.push(ty.clone());
    }
    b.type_work(&f.return_type, true, "stored signature result relation")?;
    let new_nominal = |ty: &Type| {
        matches!(ty, Type::BorrowedStorage { storage, life }
        if life.slots.is_empty()
            && (!matches!(storage.as_ref(), Type::Record { .. } | Type::Tuple(_))
                || !life.retained.is_empty()))
    };
    if (matches!(f.return_type, Type::Array { .. }) && !f.return_type.is_copy_value())
        || new_nominal(&f.return_type)
        || matches!(&f.return_type, Type::Address { inner, .. }
            if (matches!(inner.as_ref(), Type::Array { .. }) && !inner.is_copy_value())
                || new_nominal(inner))
    {
        b.type_work(&f.return_type, false, "pure Array result relation walk")?;
    }
    if let Some(id) = f.environment {
        let d = module
            .closures
            .get(id.0 as usize)
            .ok_or_else(|| failure("captured entry definition is absent"))?;
        super::signature_life::charge(c, b)?;
        if d.storage_life.as_ref() != Some(c) {
            return Err(failure(
                "captured entry relation differs from actual definition",
            ));
        }
        super::borrowed_fp::qualified_array_relation_work(&d.captures, &Type::Unit, b)?;
        return super::signature_life::capture_relation(
            &parameters,
            &d.captures,
            super::stored_callables::life_result(d),
            c,
        );
    }
    super::signature_life::function_relation(
        &parameters,
        &f.return_type,
        c,
        f.callable_contract.is_some(),
        &module.closures,
    )?;
    Ok(())
}

fn returns(
    module: &TypedModule,
    f: &TypedFunction,
    facts: &Facts,
    b: &mut Budget,
) -> CompileResult<()> {
    if let Some(d) = f
        .environment
        .and_then(|id| module.closures.get(id.0 as usize))
        && super::stored_callables::lending(d)
    {
        fee(b, d.lending_captures.len().saturating_add(1))?;
        let mut actual = BTreeSet::new();
        for (path, sources) in &facts.returned {
            if !path.is_empty() || sources.is_empty() {
                return Err(failure(
                    "lending result lacks actual inline capture sources",
                ));
            }
            for source in sources {
                fee(
                    b,
                    source
                        .place
                        .len()
                        .saturating_add(source.stored.len())
                        .saturating_add(1),
                )?;
                let HoldRoot::Capture(index) = source.base else {
                    return Err(failure(
                        "environment result escapes local or explicit input storage",
                    ));
                };
                if !source.place.is_empty()
                    || !source.stored.is_empty()
                    || !d
                        .captures
                        .get(index as usize)
                        .is_some_and(super::stored_callables::scalar)
                {
                    return Err(failure(
                        "lending source is not an actual inline scalar capture",
                    ));
                }
                ordered(b, actual.len(), 0)?;
                actual.insert(index);
            }
        }
        if actual.len() != d.lending_captures.len()
            || actual
                .into_iter()
                .zip(&d.lending_captures)
                .any(|(actual, proposed)| actual != *proposed)
        {
            return Err(failure(
                "lending capture proposal differs from actual returned environment fields",
            ));
        }
        return Ok(());
    }
    for (path, sources) in &facts.returned {
        if sources.is_empty() {
            return Err(failure(
                "stored/reference result has no actual input target; recursive cycles grant no roots",
            ));
        }
        if let Some(bound) = &f.reference_return_bound {
            fee(
                b,
                sources.len().saturating_mul(bound.len().saturating_add(1)),
            )?;
            if sources.iter().any(|s| {
                !s.stored.is_empty()
                    || (s.base != HoldRoot::Static && !matches!(s.base,HoldRoot::Parameter(p)if bound.binary_search(&p).is_ok()))
            }) {
                return Err(failure(
                    "actual returned target exceeds the existing direct reference upper",
                ));
            }
        }
        if f.storage_life.is_none() {
            continue;
        }
        let upper = binding(f, StorageBase::Result, path, b)?;
        for source in sources {
            if source.base == HoldRoot::Static {
                if !source.stored.is_empty() || !source.place.is_empty() {
                    return Err(failure(
                        "static UTF8 result cannot retain nested projections",
                    ));
                }
                continue;
            }
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
    if option_address::shape(ty, b)? {
        return Ok(true);
    }
    if !matches!(
        storage,
        Type::Record { .. } | Type::Tuple(_) | Type::Enum { .. }
    ) || (!life.retained.is_empty() && !life.slots.is_empty())
        || (matches!(storage, Type::Enum { .. })
            && (!life.slots.is_empty() || !retained::pure(storage, b)?))
    {
        return Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "nested retained/nominal stored carriers require their independent consumer",
            None,
        )]);
    }
    let mut actual = Vec::new();
    fn walk(
        ty: &Type,
        path: &mut Path,
        out: &mut Vec<(Path, StoredBorrowMode)>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        fee(b, 1)?;
        match ty {
            Type::Address { mutable, inner } => {
                b.type_work(inner, true, "stored shared actual pointee")?;
                if !inner.is_address_pointee() {
                    return Err(failure(
                        "stored shared leaf has an ineligible actual pointee",
                    ));
                }
                fee(b, path.len().saturating_add(1))?;
                out.push((
                    path.clone(),
                    if *mutable {
                        StoredBorrowMode::ExclusiveAddress
                    } else {
                        StoredBorrowMode::SharedAddress
                    },
                ));
            }
            Type::Record { name, fields } => {
                fee(b, name.len())?;
                if name.is_empty() {
                    return Err(failure("stored physical record has empty nominal identity"));
                }
                let mut names = BTreeSet::new();
                for (i, (name, t)) in fields.iter().enumerate() {
                    ordered(b, names.len(), name.len())?;
                    if name.is_empty() || !names.insert(name) {
                        return Err(failure(
                            "stored record fields require unique nonempty actual names",
                        ));
                    }
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
            Type::Enum { name, variants } => {
                if !retained::pure(ty, b)? {
                    return Err(failure(
                        "stored Enum has a real address or ownership payload",
                    ));
                }
                fee(b, name.len())?;
                if name.is_empty() {
                    return Err(failure("stored Enum has empty nominal identity"));
                }
                let mut names = BTreeSet::new();
                for (variant, (variant_name, fields)) in variants.iter().enumerate() {
                    ordered(b, names.len(), variant_name.len())?;
                    if variant_name.is_empty() || !names.insert(variant_name) {
                        return Err(failure(
                            "stored Enum variants require unique nonempty names",
                        ));
                    }
                    for (index, field) in fields.iter().enumerate() {
                        path.push(PlaceProjection::Variant(zil_ir::VariantProjection {
                            variant: variant as u32,
                            index: index as u32,
                        }));
                        walk(field, path, out, b)?;
                        path.pop();
                    }
                }
            }
            Type::Array { element, .. } => {
                if !retained::pure(ty, b)? {
                    return Err(failure(
                        "stored Array contains an address or ownership leaf",
                    ));
                }
                walk(element, path, out, b)?;
            }
            Type::BorrowedStorage { .. } => {
                if !retained::pure(ty, b)? {
                    return Err(failure(
                        "retained physical leaf needs a real borrowing consumer",
                    ));
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
    if actual.len() != life.slots.len()
        || (actual.is_empty() && life.nominal_arguments.is_empty() && life.retained.is_empty())
    {
        return Err(failure("physical stored slot coverage is incomplete"));
    }
    let mut seen = BTreeSet::new();
    let mut next = 0u32;
    for row in &life.nominal_arguments {
        fee(b, row.path.len().saturating_add(1))?;
        if row.binder >= life.binder_count {
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
    for ((path, mode), decl) in actual.iter().zip(&life.slots) {
        fee(b, path.len().saturating_add(decl.path.len()))?;
        if path != &decl.path || mode != &decl.mode || decl.binder >= life.binder_count {
            return Err(failure(
                "stored slot order, mode or binder differs from actual fields",
            ));
        }
        if *mode == StoredBorrowMode::ExclusiveAddress
            && (!matches!(storage, Type::Record { .. })
                || life.slots.len() != 1
                || !life.retained.is_empty()
                || !matches!(path.as_slice(), [PlaceProjection::Field(_)]))
        {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "exclusive stored holder requires one direct Record slot",
                None,
            )]);
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
    let actual_retained = retained::expected(storage, b)?;
    fee(b, life.retained.len())?;
    if actual_retained.len() != life.retained.len() {
        return Err(failure("physical retained class coverage differs"));
    }
    for ((path, class), row) in actual_retained.iter().zip(&life.retained) {
        fee(
            b,
            path.len()
                .saturating_add(row.path.len())
                .saturating_add(seen.len())
                .saturating_add(1),
        )?;
        if path != &row.path || *class != row.class || row.binder >= life.binder_count {
            return Err(failure(
                "retained row differs from actual physical boundary/class",
            ));
        }
        if seen.insert(row.binder) {
            if row.binder != next {
                return Err(failure("retained classes are not canonical"));
            }
            next = next
                .checked_add(1)
                .ok_or_else(|| failure("retained class overflow"))?;
        }
    }
    if next != life.binder_count {
        return Err(failure("stored binder domain has unused classes"));
    }
    Ok(true)
}

fn environment_leaf(
    value: &mut Value,
    path: Path,
    permission: &Permission,
    b: &mut Budget,
) -> CompileResult<()> {
    union(&mut value.held, permission, b)?;
    ordered(b, value.environment.len(), path.len())?;
    union(value.environment.entry(path).or_default(), permission, b)?;
    Ok(())
}
fn environment_path(
    capture: usize,
    leaf: &[PlaceProjection],
    b: &mut Budget,
) -> CompileResult<Path> {
    fee(b, leaf.len().saturating_add(1))?;
    let index = u32::try_from(capture).map_err(|_| failure("capture payload ordinal overflow"))?;
    let mut path = Vec::with_capacity(leaf.len().saturating_add(1));
    path.push(PlaceProjection::Field(index));
    path.extend_from_slice(leaf);
    Ok(path)
}
fn seed_environment(
    module: &TypedModule,
    ty: &Type,
    base: HoldRoot,
    value: &mut Value,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> CompileResult<()> {
    let ty = if let Type::Address { inner, .. } = ty {
        inner.as_ref()
    } else {
        ty
    };
    let Type::Closure(id) = ty else { return Ok(()) };
    let d = module
        .closures
        .get(id.0 as usize)
        .ok_or_else(|| failure("environment parameter definition absent"))?;
    if d.storage_life.is_none() {
        return Ok(());
    }
    fee(b, d.captures.len())?;
    let mut add = |path: Path, value: &mut Value, b: &mut Budget| -> CompileResult<()> {
        fee(b, path.len().saturating_mul(2).saturating_add(3))?;
        let target = Target {
            base,
            stored: path.clone(),
            place: Vec::new(),
        };
        let id = loans.len();
        loans.push(Loan {
            targets: BTreeSet::from([target.clone()]),
            parents: Loans::new(),
            mutable: false,
        });
        let permission = Permission {
            targets: BTreeSet::from([target]),
            loans: Loans::from([id]),
        };
        environment_leaf(value, path, &permission, b)
    };
    for (index, capture) in d.captures.iter().enumerate() {
        if matches!(capture, Type::Address { mutable: false, .. }) {
            add(environment_path(index, &[], b)?, value, b)?;
            if let Some((_, shape)) = pointed_carrier(capture) {
                for leaf in &shape.slots {
                    add(environment_path(index, &leaf.path, b)?, value, b)?;
                }
            }
        }
    }
    Ok(())
}
fn seed_value(
    ty: &Type,
    base: HoldRoot,
    state: &mut State,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> CompileResult<Value> {
    let mut value = Value::default();
    if let Type::Address { mutable, .. } = ty {
        let target = Target {
            base,
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
                base,
                stored: decl.path.clone(),
                place: Vec::new(),
            };
            let id = loans.len();
            loans.push(Loan {
                targets: BTreeSet::from([target.clone()]),
                parents: Loans::new(),
                mutable: decl.mode == StoredBorrowMode::ExclusiveAddress,
            });
            ordered(b, slots.len(), decl.path.len())?;
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
            ordered(b, state.external.len(), 0)?;
            state.external.insert(base, copy.slots);
        }
        value.slots = slots;
    }
    Ok(value)
}
fn seed(
    module: &TypedModule,
    f: &TypedFunction,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> CompileResult<State> {
    let mut state = State::default();
    for (index, local) in f.parameters.iter().enumerate() {
        fee(b, 1)?;
        let mut value = seed_value(
            &f.locals[local.0 as usize].ty,
            HoldRoot::Parameter(index as u32),
            &mut state,
            loans,
            b,
        )?;
        if option_address::candidate(&f.locals[local.0 as usize].ty) {
            fee(b, 96)?;
            if option_address::signature(&f.locals[local.0 as usize].ty) {
                value.option_variants = option_address::Variants::parameter(index as u32, b)?;
            }
        }
        seed_environment(
            module,
            &f.locals[local.0 as usize].ty,
            HoldRoot::Parameter(index as u32),
            &mut value,
            loans,
            b,
        )?;
        if b.stored_owned_fixed && matches!(f.locals[local.0 as usize].ty, Type::Desc(_)) {
            owned::describe(&mut value, HoldRoot::Parameter(index as u32), b)?;
        }
        ordered(b, state.locals.len(), 0)?;
        state.locals.insert(*local, value);
    }
    if let Some(d) = f
        .environment
        .and_then(|id| module.closures.get(id.0 as usize))
        && d.storage_life.is_some()
    {
        for (index, ty) in d.captures.iter().enumerate() {
            fee(b, 1)?;
            let value = seed_value(ty, HoldRoot::Capture(index as u32), &mut state, loans, b)?;
            ordered(b, state.captures.len(), 0)?;
            state.captures.insert(index as u32, value);
        }
    }
    Ok(state)
}

fn select(
    state: &State,
    address: &Permission,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> CompileResult<Permission> {
    // Bottom while a predecessor/callee is unsolved is not permission. Final
    // completeness and window checking reject every remaining empty target.
    if address.targets.is_empty() {
        return Ok(Permission::default());
    }
    let mut out = Permission::default();
    for root in &address.targets {
        fee(b, root.stored.len().saturating_add(1))?;
        if !root.stored.is_empty() || !root.place.is_empty() {
            return Err(failure("stored target is not an inline carrier address"));
        }
        let slots = match root.base {
            HoldRoot::Static => {
                return Err(failure("static UTF8 cannot grant nested carrier slots"));
            }
            HoldRoot::Local(local) => {
                ordered(b, state.locals.len(), 0)?;
                let Some(value) = state.locals.get(&local) else {
                    continue;
                };
                &value.slots
            }
            HoldRoot::Parameter(_) | HoldRoot::Capture(_) => {
                ordered(b, state.external.len(), 0)?;
                state
                    .external
                    .get(&root.base)
                    .ok_or_else(|| failure("external carrier has no actual slot facts"))?
            }
        };
        ordered(b, slots.len(), path.len())?;
        if let Some(leaf) = slots.get(path) {
            union(&mut out, leaf, b)?;
        }
    }
    Ok(out)
}

fn roots(value: &Value, b: &mut Budget) -> CompileResult<Targets> {
    let mut permission = Permission::default();
    union(&mut permission, &value.pointer, b)?;
    union(&mut permission, &value.held, b)?;
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
    if source.base == HoldRoot::Static {
        if !source.stored.is_empty() || !source.place.is_empty() {
            return Err(failure(
                "static UTF8 summary cannot carry nested projections",
            ));
        }
        fee(b, 1)?;
        return Ok(Permission {
            targets: Targets::from([source.clone()]),
            loans: Loans::new(),
        });
    }
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
    } else if !value.environment.is_empty() {
        ordered(b, value.environment.len(), source.stored.len())?;
        let mut out = Permission::default();
        if let Some(actual) = value.environment.get(&source.stored) {
            union(&mut out, actual, b)?;
        }
        out
    } else if !value.slots.is_empty() {
        ordered(b, value.slots.len(), source.stored.len())?;
        let mut out = Permission::default();
        if let Some(actual) = value.slots.get(&source.stored) {
            union(&mut out, actual, b)?;
        }
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

#[allow(clippy::too_many_arguments)]
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
    let option_transfer = option_address::transfer(module, f, i, values, b)?;
    match &i.kind {
        InstructionKind::StaticUtf8 { .. } => {
            fee(b, 1)?;
            out.pointer.targets.insert(Target {
                base: HoldRoot::Static,
                stored: Vec::new(),
                place: Vec::new(),
            });
        }
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
        InstructionKind::TakeLocal { local } => {
            ordered(b, state.locals.len(), 0)?;
            if super::stored_callables::fixed_value(module, &f.locals[local.0 as usize].ty) {
                out = snapshot(
                    state
                        .locals
                        .get(local)
                        .ok_or_else(|| failure("move of unavailable environment"))?,
                    b,
                )?;
            }
            state.locals.remove(local);
        }
        InstructionKind::StorageLive(local)
        | InstructionKind::StorageDead(local)
        | InstructionKind::DropClosure { local } => {
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
            if super::stored_callables::fixed_value(module, &f.locals[local.0 as usize].ty) {
                ordered(b, state.locals.len(), 0)?;
                if let Some(held) = state.locals.get(local) {
                    environment_merge(&mut out, held, b)?;
                }
            }
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
            environment_merge(&mut out, get(*address)?, b)?;
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
                    if let Some(actual) = source.slots.get(tail) {
                        union(&mut permission, actual, b)?;
                    }
                }
                if !matches!(
                    (slot(storage, &decl.path, b)?, decl.mode),
                    (
                        Type::Address { mutable: false, .. },
                        StoredBorrowMode::SharedAddress
                    ) | (
                        Type::Address { mutable: true, .. },
                        StoredBorrowMode::ExclusiveAddress
                    )
                ) {
                    return Err(failure(
                        "constructed stored field mode differs from its actual address",
                    ));
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
        InstructionKind::LoadStoredBorrow {
            storage,
            path,
            mode: StoredBorrowLoad::Reborrow { mutable: true },
        } => {
            out.pointer = select(state, &get(*storage)?.pointer, path, b)?;
            for parent in &get(*storage)?.pointer.loans {
                fee(b, 1)?;
                out.pointer.loans.insert(*parent);
            }
        }
        InstructionKind::LoadStoredBorrow { .. } | InstructionKind::StoreStoredBorrow { .. } => {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "stored reborrow/take/replacement CFG consumer is not yet implemented",
                Some(i.span),
            )]);
        }
        InstructionKind::ForeignCall { function, .. } => {
            let declaration = module
                .functions
                .get(function.0 as usize)
                .ok_or_else(|| failure("scalar foreign target is absent"))?;
            if !super::stored_callables::scalar_foreign(declaration, b)? {
                return Err(failure(
                    "foreign call has pointer or owning payload permissions",
                ));
            }
        }
        InstructionKind::UnsafeEnter { .. } | InstructionKind::UnsafeExit { .. } => {}
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
            out.option_variants = summary.returned_variants.substitute(arguments, values, b)?;
            for (path, sources) in &summary.returned {
                let mut permission = Permission::default();
                for source in sources {
                    union(
                        &mut permission,
                        &substitute(source, arguments, values, state, b)?,
                        b,
                    )?;
                    if path.is_empty()
                        && matches!(callee.return_type, Type::Address { mutable: true, .. })
                        && !source.stored.is_empty()
                        && let HoldRoot::Parameter(index) = source.base
                    {
                        let actual = arguments
                            .get(index as usize)
                            .and_then(|v| values.get(v.0 as usize))
                            .ok_or_else(|| failure("stored result carrier parameter absent"))?;
                        for loan in &actual.pointer.loans {
                            fee(b, 1)?;
                            permission.loans.insert(*loan);
                        }
                    }
                }
                if path.is_empty() {
                    out.pointer = permission;
                } else if !option_address::candidate(&callee.return_type)
                    || out.option_variants.mask() & 2 != 0
                {
                    ordered(b, out.slots.len(), path.len())?;
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
        InstructionKind::MakeClosure { closure, captures }
            if module.closures[closure.0 as usize].storage_life.is_some() =>
        {
            let mut complete = true;
            for (index, capture) in captures.iter().enumerate() {
                let actual = get(*capture)?;
                if matches!(value_type(f, *capture)?, Type::Address { .. }) {
                    complete &= !actual.pointer.targets.is_empty();
                    let path = environment_path(index, &[], b)?;
                    environment_leaf(&mut out, path, &actual.pointer, b)?;
                }
                if let Some((_, shape)) = pointed_carrier(value_type(f, *capture)?) {
                    for slot in &shape.slots {
                        let leaf = select(state, &actual.pointer, &slot.path, b)?;
                        complete &= !leaf.targets.is_empty();
                        let path = environment_path(index, &slot.path, b)?;
                        environment_leaf(&mut out, path, &leaf, b)?;
                    }
                }
            }
            out.creation_complete = Some(complete);
        }
        InstructionKind::BorrowCapture {
            index,
            mutable: false,
        } => {
            let d = f
                .environment
                .and_then(|id| module.closures.get(id.0 as usize))
                .filter(|d| super::stored_callables::lending(d))
                .ok_or_else(|| failure("inline capture loan has no lending environment"))?;
            if !d
                .captures
                .get(*index as usize)
                .is_some_and(super::stored_callables::scalar)
            {
                return Err(failure("inline capture loan requires a real scalar field"));
            }
            fee(b, 1)?;
            out.pointer.targets.insert(Target {
                base: HoldRoot::Capture(*index),
                stored: vec![],
                place: vec![],
            });
        }
        kind if super::stored_callables::called(module, f, kind)
            .is_some_and(|(d, _)| super::stored_callables::lending(d)) =>
        {
            let (d, _) = super::stored_callables::called(module, f, kind)
                .ok_or_else(|| failure("lending callee missing"))?;
            let mut receiver = Permission::default();
            match kind {
                InstructionKind::CallClosureBorrowed { callee, .. } => {
                    union(&mut receiver, &get(*callee)?.pointer, b)?;
                }
                InstructionKind::CallClosureShared { callee, .. }
                | InstructionKind::CallClosureMut { callee, .. } => {
                    fee(b, 1)?;
                    receiver.targets.insert(Target {
                        base: HoldRoot::Local(*callee),
                        stored: vec![],
                        place: vec![],
                    });
                }
                _ => {
                    return Err(failure(
                        "lending invocation must borrow its actual runtime environment",
                    ));
                }
            }
            if let Some(sources) = summaries[d.function.0 as usize].returned.get(&Vec::new()) {
                for source in sources {
                    let HoldRoot::Capture(index) = source.base else {
                        return Err(failure(
                            "callee lending result lacks an actual capture source",
                        ));
                    };
                    if !source.stored.is_empty() {
                        return Err(failure("lender cannot return an external capture target"));
                    }
                    for target in &receiver.targets {
                        fee(
                            b,
                            target
                                .place
                                .len()
                                .saturating_add(target.stored.len())
                                .saturating_add(source.place.len())
                                .saturating_add(3),
                        )?;
                        let mut target = target.clone();
                        target.place.push(PlaceProjection::Field(index));
                        target.place.extend_from_slice(&source.place);
                        ordered(
                            b,
                            out.pointer.targets.len(),
                            target.place.len().saturating_add(target.stored.len()),
                        )?;
                        out.pointer.targets.insert(target);
                    }
                    for loan in &receiver.loans {
                        ordered(b, out.pointer.loans.len(), 0)?;
                        out.pointer.loans.insert(*loan);
                    }
                }
            }
        }
        InstructionKind::ReadCapture { index } if !state.captures.is_empty() => {
            ordered(b, state.captures.len(), 0)?;
            out = snapshot(
                state
                    .captures
                    .get(index)
                    .ok_or_else(|| failure("actual environment capture is absent"))?,
                b,
            )?;
        }
        InstructionKind::DescribeLocal { local } if b.stored_owned_fixed => {
            owned::describe(&mut out, HoldRoot::Local(*local), b)?;
        }
        InstructionKind::DescribeCapture { index } if b.stored_owned_fixed => {
            owned::describe(&mut out, HoldRoot::Capture(*index), b)?;
        }
        InstructionKind::DropManaged { local } if b.stored_owned_fixed => {
            state.locals.remove(local);
        }
        _ if b.stored_owned_fixed && owned::operation(&i.kind) => {}
        _ if option_transfer.is_some() => {
            out = option_transfer.unwrap();
        }
        _ if pure_copy::operation(&i.kind) => {
            pure_copy::types(f, i, b)?;
            for input in resources::uses(&i.kind) {
                let payload = get(input)?;
                if !payload.pointer.targets.is_empty()
                    || !payload.pointer.loans.is_empty()
                    || !payload.slots.is_empty()
                    || !payload.held.targets.is_empty()
                    || !payload.held.loans.is_empty()
                    || !payload.environment.is_empty()
                {
                    return Err(failure(
                        "ordinary pure Copy input carries actual stored target permissions",
                    ));
                }
            }
        }
        _ if super::stored_callables::instruction(module, f, i) => {}
        InstructionKind::FunctionRef { .. } | InstructionKind::CallIndirect { .. }
            if super::class_flow::pointer_instruction(f, i) => {}
        InstructionKind::ReadAddress { address } => {
            let Type::Address { inner, .. } = value_type(f, *address)? else {
                return Err(failure("read transfer lacks an actual Address"));
            };
            if carrier(inner).is_some() && !pure_read(module, inner, b)? {
                return Err(failure(
                    "read transfer cannot erase actual borrowed/owned leaves",
                ));
            }
            // Only the source Address permission is used by access checking.
            // A pure value has no borrowed targets to copy: the full result
            // type carries its nominal/retained classes, without rooting them
            // in the storage bytes or inventing a loan from a phantom class.
        }
        InstructionKind::WriteAddress { address, .. } => {
            let Type::Address { inner, .. } = value_type(f, *address)? else {
                return Err(failure("write base is not an actual Address"));
            };
            b.type_work(inner, true, "stored profile actual ordinary write payload")?;
            if !inner.is_copy_value() {
                return Err(failure(
                    "ordinary write cannot replace stored leaves or qualified carriers; use the checked slot protocol",
                ));
            }
        }
        InstructionKind::Constant(_)
        | InstructionKind::ExtractField { .. }
        | InstructionKind::ConvertInteger { .. }
        | InstructionKind::ConvertNumber { .. }
        | InstructionKind::Unary { .. }
        | InstructionKind::Binary { .. }
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
        && b.static_utf8
        && matches!(value_type(f, result)?, Type::Address { mutable: true, .. })
    {
        fee(b, out.pointer.targets.len())?;
        if out
            .pointer
            .targets
            .iter()
            .any(|t| t.base == HoldRoot::Static)
        {
            return Err(failure("static UTF8 cannot produce a mutable address"));
        }
    }
    if let Some(result) = i.result
        && matches!(
            i.kind,
            InstructionKind::BorrowLocal { .. }
                | InstructionKind::BorrowLocalMut { .. }
                | InstructionKind::ReborrowAddress { .. }
                | InstructionKind::LoadStoredBorrow {
                    mode: StoredBorrowLoad::Reborrow { mutable: true },
                    ..
                }
                | InstructionKind::AddressField { .. }
                | InstructionKind::AddressIndex { .. }
                | InstructionKind::AddressSlice { .. }
                | InstructionKind::Call { .. }
                | InstructionKind::BorrowCapture { .. }
                | InstructionKind::CallClosureBorrowed { .. }
                | InstructionKind::CallClosureShared { .. }
                | InstructionKind::CallClosureMut { .. }
        )
        && matches!(value_type(f, result)?, Type::Address { .. })
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
    if let Some(result) = i.result
        && matches!(i.kind, InstructionKind::Load(_))
        && matches!(value_type(f, result)?, Type::Address { mutable: true, .. })
    {
        let id = result.0 as usize;
        let loan = &mut loans[id];
        let mut collected = Permission {
            targets: std::mem::take(&mut loan.targets),
            loans: std::mem::take(&mut loan.parents),
        };
        union(&mut collected, &out.pointer, b)?;
        loan.targets = collected.targets;
        loan.parents = collected.loans;
        loan.mutable = true;
        out.pointer.loans.clear();
        fee(b, 1)?;
        out.pointer.loans.insert(id);
    }
    if let Some(result) = i.result
        && matches!(i.kind, InstructionKind::Call { .. })
        && carrier(value_type(f, result)?).is_some()
    {
        let mut next = f.values.len();
        fee(b, result.0 as usize)?;
        for ty in &f.values[..result.0 as usize] {
            if let Some((_, shape)) = carrier(ty) {
                next = next
                    .checked_add(shape.slots.len())
                    .ok_or_else(|| failure("stored result loan count overflow"))?;
            }
        }
        let (_, shape) = carrier(value_type(f, result)?)
            .ok_or_else(|| failure("stored call result lacks actual shape"))?;
        for declaration in &shape.slots {
            ordered(b, out.slots.len(), declaration.path.len())?;
            if let Some(permission) = out.slots.get_mut(&declaration.path) {
                let loan = loans
                    .get_mut(next)
                    .ok_or_else(|| failure("stored call result loan outside actual arena"))?;
                let mut collected = Permission {
                    targets: std::mem::take(&mut loan.targets),
                    loans: std::mem::take(&mut loan.parents),
                };
                union(&mut collected, permission, b)?;
                loan.targets = collected.targets;
                loan.parents = collected.loans;
                loan.mutable = declaration.mode == StoredBorrowMode::ExclusiveAddress;
                permission.loans.clear();
                fee(b, 1)?;
                permission.loans.insert(next);
            }
            next = next
                .checked_add(1)
                .ok_or_else(|| failure("stored result loan identity overflow"))?;
        }
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
    if b.static_utf8 {
        fee(b, permission.targets.len())?;
    }
    if b.static_utf8
        && write
        && permission
            .targets
            .iter()
            .any(|t| t.base == HoldRoot::Static)
    {
        return Err(failure("program-static UTF8 has no mutable authority"));
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
            union(&mut direct, &value.held, b)?;
            for leaf in value.slots.values() {
                union(&mut direct, leaf, b)?;
            }
        }
    }
    for value in state.captures.values() {
        union(&mut direct, &value.pointer, b)?;
        for leaf in value.slots.values() {
            union(&mut direct, leaf, b)?;
        }
    }
    ancestry(&direct.loans, loans, b)
}
fn reverse(
    module: &TypedModule,
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
        InstructionKind::TakeLocal { local }
        | InstructionKind::DropClosure { local }
        | InstructionKind::CallClosureShared { callee: local, .. }
        | InstructionKind::CallClosureMut { callee: local, .. }
            if super::stored_callables::fixed_value(module, &f.locals[local.0 as usize].ty) =>
        {
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
    module: &TypedModule,
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
                reverse(module, f, i, &mut live, b)?;
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
    let live_out = liveness(module, f, preds, reachable, b)?;
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
            reverse(module, f, i, &mut live, b)?;
        }
        for (position, i) in block.instructions.iter().enumerate() {
            fee(b, width)?;
            let mut live = after[position].clone();
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
                InstructionKind::Load(local)
                    if matches!(
                        f.locals[local.0 as usize].ty,
                        Type::Address { mutable: true, .. }
                    ) =>
                {
                    ordered(b, state.locals.len(), 0)?;
                    let source = state
                        .locals
                        .get(local)
                        .ok_or_else(|| failure("exclusive address local is unavailable"))?;
                    use_permission(&source.pointer, true, &current, loans, apart, b)?;
                }
                InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local }
                | InstructionKind::Store { local, .. }
                | InstructionKind::StorageDead(local)
                | InstructionKind::TakeLocal { local }
                | InstructionKind::DropClosure { local } => {
                    let mut actual = Permission::default();
                    actual.targets.insert(Target {
                        base: HoldRoot::Local(*local),
                        stored: Vec::new(),
                        place: Vec::new(),
                    });
                    let write = !matches!(i.kind, InstructionKind::BorrowLocal { .. });
                    use_permission(&actual, write, &current, loans, apart, b)?;
                }
                InstructionKind::BorrowCapture { index, mutable } => {
                    let actual = Permission {
                        targets: BTreeSet::from([Target {
                            base: HoldRoot::Capture(*index),
                            stored: vec![],
                            place: vec![],
                        }]),
                        loans: Loans::new(),
                    };
                    fee(b, 1)?;
                    use_permission(&actual, *mutable, &current, loans, apart, b)?;
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
                InstructionKind::LoadStoredBorrow {
                    storage,
                    path,
                    mode,
                } => {
                    let exclusive = matches!(
                        mode,
                        StoredBorrowLoad::Reborrow { mutable: true } | StoredBorrowLoad::Take
                    );
                    use_permission(pointer(*storage)?, exclusive, &current, loans, apart, b)?;
                    let actual = select(&state, pointer(*storage)?, path, b)?;
                    use_permission(&actual, exclusive, &current, loans, apart, b)?;
                }
                InstructionKind::WriteAddress { address, .. }
                | InstructionKind::ReborrowAddress {
                    address,
                    mutable: true,
                } => use_permission(pointer(*address)?, true, &current, loans, apart, b)?,
                InstructionKind::CallIndirect { arguments, .. }
                | InstructionKind::CallClosure { arguments, .. }
                | InstructionKind::CallClosureBorrowed { arguments, .. }
                | InstructionKind::CallClosureShared { arguments, .. }
                | InstructionKind::CallClosureMut { arguments, .. }
                | InstructionKind::CallClosureOnce { arguments, .. } => {
                    let mut all: Vec<(Permission, bool)> = Vec::new();
                    match &i.kind {
                        InstructionKind::CallClosureBorrowed { callee, .. } => {
                            let mut p = Permission::default();
                            union(&mut p, pointer(*callee)?, b)?;
                            fee(b, 1)?;
                            all.push((
                                p,
                                matches!(
                                    i.kind,
                                    InstructionKind::CallClosureBorrowed {
                                        kind: CallableKind::FnMut,
                                        ..
                                    }
                                ),
                            ));
                        }
                        InstructionKind::CallClosureShared { callee, .. }
                        | InstructionKind::CallClosureMut { callee, .. } => {
                            fee(b, 2)?;
                            all.push((
                                Permission {
                                    targets: BTreeSet::from([Target {
                                        base: HoldRoot::Local(*callee),
                                        stored: vec![],
                                        place: vec![],
                                    }]),
                                    loans: Loans::new(),
                                },
                                matches!(i.kind, InstructionKind::CallClosureMut { .. }),
                            ));
                        }
                        _ => {}
                    }

                    let receiver_held = match &i.kind {
                        InstructionKind::CallClosureShared { callee, .. }
                        | InstructionKind::CallClosureMut { callee, .. }
                            if super::stored_callables::fixed_value(
                                module,
                                &f.locals[callee.0 as usize].ty,
                            ) =>
                        {
                            ordered(b, state.locals.len(), 0)?;
                            state.locals.get(callee).map(|v| &v.held)
                        }
                        InstructionKind::CallClosure { callee, .. }
                        | InstructionKind::CallClosureBorrowed { callee, .. }
                        | InstructionKind::CallClosureOnce { callee, .. } => {
                            Some(&values[callee.0 as usize].held)
                        }
                        _ => None,
                    };
                    if let Some(held) = receiver_held
                        && !held.targets.is_empty()
                    {
                        let mut p = Permission::default();
                        union(&mut p, held, b)?;
                        fee(b, 1)?;
                        all.push((p, false));
                    }
                    for argument in arguments {
                        let actual = &values[argument.0 as usize];
                        if let Type::Address { mutable, .. } = value_type(f, *argument)? {
                            let mut p = Permission::default();
                            union(&mut p, &actual.pointer, b)?;
                            fee(b, 1)?;
                            all.push((p, *mutable));
                        }
                        for leaf in actual.slots.values() {
                            let mut p = Permission::default();
                            union(&mut p, leaf, b)?;
                            fee(b, 1)?;
                            all.push((p, false));
                        }
                        if let Some((_, shape)) = pointed_carrier(value_type(f, *argument)?) {
                            for slot in &shape.slots {
                                let p = select(&state, &actual.pointer, &slot.path, b)?;
                                fee(b, 1)?;
                                all.push((p, false));
                            }
                        }
                    }
                    for (index, (permission, write)) in all.iter().enumerate() {
                        use_permission(permission, *write, &current, loans, apart, b)?;
                        for (other, other_write) in &all[..index] {
                            if *write || *other_write {
                                for a in &permission.targets {
                                    for z in &other.targets {
                                        separate(a, z, apart, b)?;
                                    }
                                }
                            }
                        }
                    }
                }
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
                        if !actual.held.targets.is_empty() {
                            use_permission(&actual.held, false, &current, loans, apart, b)?;
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
                                for leaf in right
                                    .slots
                                    .values()
                                    .chain((!right.held.targets.is_empty()).then_some(&right.held))
                                {
                                    for x in &actual.pointer.targets {
                                        for y in &leaf.targets {
                                            separate(x, y, apart, b)?;
                                        }
                                    }
                                }
                            }
                            if matches!(value_type(f, *other)?, Type::Address { mutable: true, .. })
                            {
                                for leaf in actual.slots.values().chain(
                                    (!actual.held.targets.is_empty()).then_some(&actual.held),
                                ) {
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
    derive_cfg(module, graphs, None, b)
}
fn derive_cfg(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    inactive: Option<&[BTreeSet<ValueId>]>,
    b: &mut Budget,
) -> CompileResult<Vec<Facts>> {
    let mut observed_none = false;
    fee(b, module.functions.len())?;
    let mut summaries = vec![Facts::default(); module.functions.len()];
    loop {
        let mut changed = false;
        for (index, f) in module.functions.iter().enumerate() {
            let (preds, reachable) = &graphs[index];
            fee(
                b,
                f.values
                    .len()
                    .saturating_mul(6)
                    .saturating_add(f.blocks.len()),
            )?;
            let mut loan_count = f.values.len();
            fee(b, f.values.len())?;
            for ty in &f.values {
                if let Some((_, shape)) = carrier(ty) {
                    loan_count = loan_count
                        .checked_add(shape.slots.len())
                        .ok_or_else(|| failure("stored loan arena overflow"))?;
                }
            }
            fee(b, loan_count.saturating_mul(4))?;
            let mut loans = vec![Loan::default(); loan_count];
            let entry = seed(module, f, &mut loans, b)?;
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
                    state_work(&state, b)?;
                    state_work(&outputs[bi], b)?;
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
                complete: false,
                returned_variants: option_address::Variants::default(),
                none_values: BTreeSet::new(),
            };
            for (id, value) in values.iter().enumerate() {
                if value.option_variants.none() {
                    observed_none = true;
                    fee(b, actual.none_values.len().saturating_add(1))?;
                    actual.none_values.insert(ValueId(id as u32));
                }
                let root = roots(value, b)?;
                fee(b, root.len())?;
                actual.values.push(root.into_iter().collect());
            }
            for (block, live) in f.blocks.iter().zip(reachable) {
                fee(b, 1)?;
                if *live && let Terminator::Return(Some(id)) = block.terminator {
                    let value = &values[id.0 as usize];
                    actual.returned_variants.join(&value.option_variants, b)?;
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
                                    .saturating_add(target.place.len())
                                    .saturating_mul(row.len().saturating_add(1))
                                    .saturating_add(1),
                            )?;
                            if !matches!(target.base, HoldRoot::Parameter(_) | HoldRoot::Static)
                                && !(matches!(target.base, HoldRoot::Capture(_))
                                    && f.environment
                                        .and_then(|id| module.closures.get(id.0 as usize))
                                        .is_some_and(super::stored_callables::lending))
                            {
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
            let mut grounded = true;
            for (id, (ty, value)) in f.values.iter().zip(&values).enumerate() {
                fee(b, 1)?;
                if let Some(rows) = inactive {
                    ordered(b, rows[index].len(), 0)?;
                    if rows[index].contains(&ValueId(id as u32)) {
                        continue;
                    }
                }
                if value.creation_complete == Some(false) {
                    grounded = false;
                }
                if matches!(ty, Type::Address { .. }) && value.pointer.targets.is_empty() {
                    grounded = false;
                }
                if let Some((_, life)) = carrier(ty) {
                    if option_address::candidate(ty) {
                        fee(b, 96)?;
                        if !option_address::signature(ty) {
                            return Err(failure("Option activity has no exact shape"));
                        }
                        let mask = value.option_variants.mask();
                        if mask == 0 {
                            grounded = false;
                        }
                        if mask == 1 {
                            continue;
                        }
                    }
                    fee(b, life.slots.len())?;
                    for slot in &life.slots {
                        ordered(b, value.slots.len(), slot.path.len())?;
                        if value
                            .slots
                            .get(&slot.path)
                            .is_none_or(|p| p.targets.is_empty())
                        {
                            grounded = false;
                        }
                    }
                }
            }
            for (id, loan) in loans.iter().enumerate() {
                let parents = ancestry(&loan.parents, &loans, b)?;
                ordered(b, parents.len(), 0)?;
                if parents.contains(&id) {
                    return Err(failure(
                        "cyclic stored/target loan ancestry has no actual finite authority",
                    ));
                }
            }
            actual.complete = grounded;
            if grounded {
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
            }
            facts_work(&actual, b)?;
            facts_work(&summaries[index], b)?;
            if actual != summaries[index] {
                summaries[index] = actual;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    // Freeze only the original, fully converged graph facts. A second solve
    // receives no authority to trim another edge from its narrowed summaries.
    if inactive.is_none()
        && observed_none
        && let Some(proof) = option_address::refine(module, graphs, &summaries, b)?
    {
        return derive_cfg(module, &proof.graphs, Some(&proof.inactive), b);
    }
    for (index, (f, facts)) in module.functions.iter().zip(&summaries).enumerate() {
        if !facts.complete {
            return Err(failure(
                "stored/address producer has incomplete actual target facts",
            ));
        }
        returns(module, f, facts, b)?;
        for (id, (ty, roots)) in f.values.iter().zip(&facts.values).enumerate() {
            fee(b, 1)?;
            if let Some(rows) = inactive {
                ordered(b, rows[index].len(), 0)?;
                if rows[index].contains(&ValueId(id as u32)) {
                    continue;
                }
            }
            let needs_target = matches!(ty, Type::Address { .. })
                || matches!(ty,Type::BorrowedStorage{life,..}if !life.slots.is_empty());
            if needs_target && roots.is_empty() && !facts.none_values.contains(&ValueId(id as u32))
            {
                return Err(failure(
                    "stored/address producer has no independently grounded actual origin",
                ));
            }
        }
    }
    Ok(summaries)
}

pub(super) fn infer(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    b: &mut Budget,
) -> CompileResult<Vec<Vec<ValueHold>>> {
    b.stored_owned_fixed = owned::profile(module, b)?;
    super::stored_callables::definitions(module, b)?;
    let mut foreign_symbols = BTreeSet::new();
    for f in &module.functions {
        if (f.foreign.is_some() && !super::stored_callables::scalar_foreign(f, b)?)
            || f.callable_return_contract.is_some()
            || f.call_kind != CallKind::Direct
        {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "stored profile requires ordinary direct function bodies",
                Some(f.span),
            )]);
        }
        if let Some(declaration) = &f.foreign {
            b.spend(
                foreign_symbols
                    .len()
                    .saturating_add(1)
                    .saturating_mul(declaration.symbol.len().saturating_add(1)),
                "stored scalar foreign symbol uniqueness",
            )?;
            if !foreign_symbols.insert(declaration.symbol.as_str()) {
                return Err(failure(
                    "scalar foreign symbol has more than one actual declaration",
                ));
            }
        }
        if let Some(id) = f.environment {
            let d = module
                .closures
                .get(id.0 as usize)
                .ok_or_else(|| failure("callback function environment is absent"))?;
            if !super::stored_callables::finite(module, id) || d.function != f.id {
                return Err(failure("callback function is not the actual finite entry"));
            }
        }
        for ty in f
            .locals
            .iter()
            .map(|l| &l.ty)
            .chain(&f.values)
            .chain(std::iter::once(&f.return_type))
        {
            b.type_work(ty, true, "stored ordinary executable value profile")?;
            if !(super::stored_callables::value(module, ty)
                || carrier(ty).is_some()
                || pointed_carrier(ty).is_some()
                || matches!(ty,Type::Address{inner,..}if inner.is_address_pointee())
                || ty.is_copy_value()
                || b.stored_owned_fixed && owned::data(ty, b)?
                || pure_inline(module, ty, b)?
                || matches!(ty, Type::Address { inner, mutable: false } if pure_inline(module, inner, b)?))
            {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Analyze,
                    "owned or opaque value has no actual stored permission integration",
                    Some(f.span),
                )]);
            }
        }
        if super::stored_callables::fixed_value(module, &f.return_type) {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "captured Fixed environment escape needs its retained result target transfer",
                Some(f.span),
            )]);
        }
        if f.foreign.is_none() {
            actual_instructions(module, f, &graphs[f.id.0 as usize], b)?;
        }
        super::class_flow::check(module, f, &graphs[f.id.0 as usize].1, b)?;
    }
    super::stored_callables::roles(module, graphs, b)?;
    let facts = derive(module, graphs, b)?;
    // A scalar capture has no destructor, but a concrete FnOnce environment
    // still has one actual consumption obligation. The stored dispatcher must
    // not bypass the independent SSA/local responsibility ledger.
    fee(b, module.closures.len())?;
    if module
        .closures
        .iter()
        .any(|d| d.kind == CallableKind::FnOnce)
        || b.stored_owned_fixed
    {
        super::resources::check_module(module, graphs, b)?;
    }
    let mut result = Vec::new();
    fee(b, facts.len())?;
    for function in facts {
        let mut rows = Vec::new();
        fee(b, function.values.len())?;
        for (id, targets) in function.values.into_iter().enumerate() {
            let mut roots = BTreeSet::new();
            for target in targets {
                ordered(b, roots.len(), 0)?;
                roots.insert(target.base);
            }
            rows.push(ValueHold {
                value: ValueId(id as u32),
                roots: roots.into_iter().collect(),
            });
        }
        result.push(rows);
    }
    Ok(result)
}

fn address_result(
    f: &TypedFunction,
    i: &Instruction,
    mutable: bool,
    payload: &Type,
    b: &mut Budget,
) -> CompileResult<()> {
    b.type_work(payload, false, "stored actual producer result comparison")?;
    let result = i
        .result
        .ok_or_else(|| failure("actual address producer lacks result"))?;
    if !matches!(value_type(f,result)?,Type::Address{mutable:m,inner}if *m==mutable&&inner.as_ref()==payload)
    {
        return Err(failure(
            "address producer's actual mode/payload differs from claimed type",
        ));
    }
    Ok(())
}

fn actual_instructions(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &(Vec<Vec<usize>>, Vec<bool>),
    b: &mut Budget,
) -> CompileResult<()> {
    let mut scalar_ffi = false;
    let (preds, reachable) = graph;
    fee(b, f.values.len())?;
    let mut definitions = vec![false; f.values.len()];
    for block in &f.blocks {
        for i in &block.instructions {
            fee(b, 1)?;
            let owned_operation = b.stored_owned_fixed && owned::instruction(module, f, i, b)?;
            if !super::class_flow::pointer_instruction(f, i)
                && !super::stored_callables::instruction(module, f, i)
                && !owned_operation
                && !pure_copy::operation(&i.kind)
                && !matches!(
                    &i.kind,
                    InstructionKind::StaticUtf8 { .. }
                        | InstructionKind::Constant(_)
                        | InstructionKind::Load(_)
                        | InstructionKind::Store { .. }
                        | InstructionKind::StorageLive(_)
                        | InstructionKind::StorageDead(_)
                        | InstructionKind::BorrowLocal { .. }
                        | InstructionKind::BorrowLocalMut { .. }
                        | InstructionKind::ReborrowAddress { .. }
                        | InstructionKind::AddressField { .. }
                        | InstructionKind::AddressIndex { .. }
                        | InstructionKind::AddressSlice { .. }
                        | InstructionKind::ReadAddress { .. }
                        | InstructionKind::WriteAddress { .. }
                        | InstructionKind::SliceLength { .. }
                        | InstructionKind::MakeAggregate { .. }
                        | InstructionKind::ExtractField { .. }
                        | InstructionKind::Call { .. }
                        | InstructionKind::ForeignCall { .. }
                        | InstructionKind::UnsafeEnter { .. }
                        | InstructionKind::UnsafeExit { .. }
                        | InstructionKind::Unary { .. }
                        | InstructionKind::Binary { .. }
                        | InstructionKind::ConvertInteger { .. }
                        | InstructionKind::ConvertNumber { .. }
                        | InstructionKind::ScopeEnter { .. }
                        | InstructionKind::ScopeExit { .. }
                        | InstructionKind::LoadStoredBorrow {
                            mode: StoredBorrowLoad::CopyShared
                                | StoredBorrowLoad::Reborrow { mutable: true },
                            ..
                        }
                )
            {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Analyze,
                    "instruction is outside the actual stored profile, including in unreachable blocks",
                    Some(i.span),
                )]);
            }
            if let Some(v) = i.result {
                let slot = definitions
                    .get_mut(v.0 as usize)
                    .ok_or_else(|| failure("result outside actual values"))?;
                if *slot {
                    return Err(failure("one SSA value has multiple actual definitions"));
                }
                *slot = true;
            }
            for v in resources::uses(&i.kind) {
                fee(b, 1)?;
                value_type(f, v)?;
            }
            let result = |ty: &Type, b: &mut Budget| -> CompileResult<()> {
                b.type_work(ty, false, "stored actual scalar/carrier result")?;
                if i.result
                    .is_none_or(|v| f.values.get(v.0 as usize) != Some(ty))
                {
                    return Err(failure(
                        "instruction result differs from actual producer type",
                    ));
                }
                Ok(())
            };
            if super::class_flow::pointer_instruction(f, i) {
                super::class_flow::pointer_types(module, f, i, b)?;
            }
            if super::stored_callables::instruction(module, f, i) {
                super::stored_callables::types(module, f, i, b)?;
            }
            if pure_copy::operation(&i.kind) && option_address::types(module, f, i, b)?.is_none() {
                pure_copy::types(f, i, b)?;
            }
            match &i.kind {
                InstructionKind::UnsafeEnter { .. } | InstructionKind::UnsafeExit { .. } => {
                    scalar_ffi = true;
                    if i.result.is_some() {
                        return Err(failure("unsafe region marker cannot produce an SSA result"));
                    }
                }
                InstructionKind::ForeignCall {
                    function,
                    arguments,
                    ..
                } => {
                    scalar_ffi = true;
                    let callee = module
                        .functions
                        .get(function.0 as usize)
                        .ok_or_else(|| failure("scalar foreign target absent"))?;
                    if !super::stored_callables::scalar_foreign(callee, b)?
                        || arguments.len() != callee.parameters.len()
                    {
                        return Err(failure("foreign call is outside actual scalar ABI"));
                    }
                    fee(b, arguments.len())?;
                    for (argument, parameter) in arguments.iter().zip(&callee.parameters) {
                        let expected = &callee
                            .locals
                            .get(parameter.0 as usize)
                            .ok_or_else(|| failure("foreign actual parameter absent"))?
                            .ty;
                        b.type_work(
                            expected,
                            false,
                            "foreign scalar actual parameter comparison",
                        )?;
                        if value_type(f, *argument)? != expected {
                            return Err(failure(
                                "foreign scalar argument differs from its declaration",
                            ));
                        }
                    }
                    if callee.return_type == Type::Unit {
                        if i.result.is_some() {
                            return Err(failure("Unit foreign call fabricates a value"));
                        }
                    } else {
                        result(&callee.return_type, b)?;
                    }
                }
                InstructionKind::BorrowCapture {
                    index,
                    mutable: false,
                } => {
                    let d = f
                        .environment
                        .and_then(|id| module.closures.get(id.0 as usize))
                        .filter(|d| super::stored_callables::lending(d))
                        .ok_or_else(|| failure("capture borrow outside real lending entry"))?;
                    let ty = d
                        .captures
                        .get(*index as usize)
                        .filter(|ty| super::stored_callables::scalar(ty))
                        .ok_or_else(|| {
                            failure("capture borrow lacks actual inline scalar field")
                        })?;
                    address_result(f, i, false, ty, b)?;
                }
                InstructionKind::ExtractField { .. } => {}
                InstructionKind::ConvertInteger { value, target }
                | InstructionKind::ConvertNumber { value, target } => {
                    if !value_type(f, *value)?.is_numeric() || !target.is_numeric() {
                        return Err(failure(
                            "ordinary numeric conversion has nonnumeric actual data",
                        ));
                    }
                    result(target, b)?;
                }
                InstructionKind::Load(local) => {
                    let ty = &f.locals[local.0 as usize].ty;
                    result(ty, b)?;
                }
                InstructionKind::Store { local, value } => {
                    let ty = &f.locals[local.0 as usize].ty;
                    b.type_work(ty, false, "stored actual local assignment")?;
                    if value_type(f, *value)? != ty {
                        return Err(failure(
                            "actual local assignment changes complete value type",
                        ));
                    }
                }
                InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local } => {
                    let exclusive = matches!(i.kind, InstructionKind::BorrowLocalMut { .. });
                    let local = &f.locals[local.0 as usize];
                    if exclusive && !local.mutable {
                        return Err(failure("exclusive borrow lacks actual mutable local"));
                    }
                    address_result(f, i, exclusive, &local.ty, b)?;
                }
                InstructionKind::ReborrowAddress { address, mutable } => {
                    let Type::Address {
                        mutable: available,
                        inner,
                    } = value_type(f, *address)?
                    else {
                        return Err(failure("reborrow source is not an actual Address"));
                    };
                    if *mutable && !*available {
                        return Err(failure("reborrow upgrades shared actual authority"));
                    }
                    address_result(f, i, *mutable, inner, b)?;
                }
                InstructionKind::AddressField { address, index } => {
                    let Type::Address { mutable, inner } = value_type(f, *address)? else {
                        return Err(failure("field base is not an actual Address"));
                    };
                    let inner = carrier(inner).map_or(inner.as_ref(), |(storage, _)| storage);
                    let child = match inner {
                        Type::Record { fields, .. } => fields.get(*index as usize).map(|(_, t)| t),
                        Type::Tuple(fs) => fs.get(*index as usize),
                        _ => None,
                    }
                    .ok_or_else(|| {
                        failure("actual field is absent or crosses non-inline storage")
                    })?;
                    b.type_work(child, true, "stored actual ordinary inline projection")?;
                    if !child.is_copy_value()
                        && !pure_read(module, child, b)?
                        && !pure_inline(module, child, b)?
                    {
                        return Err(failure(
                            "stored borrowed leaf must use actual slot-load protocol",
                        ));
                    }
                    address_result(f, i, *mutable, child, b)?;
                }
                InstructionKind::AddressIndex { address, index } => {
                    let Type::Address { mutable, inner } = value_type(f, *address)? else {
                        return Err(failure("index base is not an Address"));
                    };
                    if !value_type(f, *index)?.is_integer() {
                        return Err(failure("index is not an integer"));
                    }
                    let element = match inner.as_ref() {
                        Type::Array { element, .. } | Type::Slice { element } => element.as_ref(),
                        _ => return Err(failure("actual index base is not Array/Slice")),
                    };
                    address_result(f, i, *mutable, element, b)?;
                }
                InstructionKind::AddressSlice {
                    address,
                    start,
                    end,
                } => {
                    let Type::Address { mutable, inner } = value_type(f, *address)? else {
                        return Err(failure("slice base is not an Address"));
                    };
                    for index in start.iter().chain(end) {
                        if !value_type(f, *index)?.is_integer() {
                            return Err(failure("slice bound is not an integer"));
                        }
                    }
                    let element = match inner.as_ref() {
                        Type::Array { element, .. } | Type::Slice { element } => element.as_ref(),
                        _ => return Err(failure("actual slice base is not Array/Slice")),
                    };
                    b.type_work(element, true, "stored actual slice payload")?;
                    let Some(Type::Address {
                        mutable: claimed,
                        inner,
                    }) = i.result.and_then(|v| f.values.get(v.0 as usize))
                    else {
                        return Err(failure("slice lacks Address result"));
                    };
                    if claimed != mutable
                        || !matches!(inner.as_ref(),Type::Slice{element:t}if t.as_ref()==element)
                    {
                        return Err(failure("slice result differs from actual source"));
                    }
                }
                InstructionKind::ReadAddress { address } => {
                    let Type::Address { inner, .. } = value_type(f, *address)? else {
                        return Err(failure("read base is not an Address"));
                    };
                    b.type_work(inner, true, "stored actual ordinary read payload")?;
                    if !inner.is_copy_value()
                        && !pure_read(module, inner, b)?
                        && !pure_inline(module, inner, b)?
                    {
                        return Err(failure(
                            "ordinary read cannot copy stored carrier/borrow leaf",
                        ));
                    }
                    result(inner, b)?;
                }
                InstructionKind::WriteAddress { address, value } => {
                    let Type::Address {
                        mutable: true,
                        inner,
                    } = value_type(f, *address)?
                    else {
                        return Err(failure("write lacks exclusive actual Address"));
                    };
                    b.type_work(inner, true, "stored actual ordinary write payload")?;
                    if !inner.is_copy_value() || value_type(f, *value)? != inner.as_ref() {
                        return Err(failure(
                            "ordinary write changes actual slot state/type outside protocol",
                        ));
                    }
                }
                InstructionKind::LoadStoredBorrow { .. }
                | InstructionKind::StoreStoredBorrow { .. } => {
                    super::stored_borrows::instruction(f, i, b)?
                }
                InstructionKind::MakeAggregate { fields } => {
                    let ty = value_type(
                        f,
                        i.result.ok_or_else(|| failure("aggregate lacks result"))?,
                    )?;
                    let ty = carrier(ty).map_or(ty, |(storage, _)| storage);
                    let count = match ty {
                        Type::Record { fields, .. } => fields.len(),
                        Type::Tuple(fs) => fs.len(),
                        Type::Array { length, .. } => *length as usize,
                        _ => return Err(failure("aggregate has no actual inline layout")),
                    };
                    if fields.len() != count {
                        return Err(failure("aggregate omits actual fields"));
                    }
                    for (position, v) in fields.iter().enumerate() {
                        let expected = match ty {
                            Type::Record { fields, .. } => &fields[position].1,
                            Type::Tuple(fs) => &fs[position],
                            Type::Array { element, .. } => element.as_ref(),
                            _ => unreachable!(),
                        };
                        b.type_work(expected, false, "stored actual constructed field")?;
                        if value_type(f, *v)? != expected {
                            return Err(failure("constructed field differs from actual full type"));
                        }
                    }
                }
                InstructionKind::Call {
                    function,
                    arguments,
                } => {
                    let target = module
                        .functions
                        .get(function.0 as usize)
                        .ok_or_else(|| failure("actual call target absent"))?;
                    if arguments.len() != target.parameters.len() {
                        return Err(failure("call arity differs from actual body signature"));
                    }
                    for (v, l) in arguments.iter().zip(&target.parameters) {
                        let expected = target
                            .locals
                            .get(l.0 as usize)
                            .ok_or_else(|| failure("callee parameter absent"))?;
                        b.type_work(&expected.ty, false, "stored complete actual call argument")?;
                        if value_type(f, *v)? != &expected.ty {
                            return Err(failure(
                                "call argument differs from complete actual signature",
                            ));
                        }
                    }
                    if target.return_type == Type::Unit {
                        if i.result.is_some() {
                            return Err(failure("Unit call fabricates a result"));
                        }
                    } else {
                        result(&target.return_type, b)?;
                    }
                }
                _ => {}
            }
        }
    }
    if b.capture_relations {
        fee(b, definitions.len())?;
        if definitions.iter().any(|defined| !defined) {
            return Err(failure(
                "captured context SSA slot has no actual definition",
            ));
        }
    }
    for block in &f.blocks {
        fee(b, 1)?;
        match block.terminator {
            Terminator::Return(Some(v)) => {
                b.type_work(&f.return_type, false, "stored actual return signature")?;
                if value_type(f, v)? != &f.return_type {
                    return Err(failure(
                        "actual returned value differs from complete return signature",
                    ));
                }
            }
            Terminator::Return(None) if f.return_type != Type::Unit => {
                return Err(failure("non-Unit return omits actual value"));
            }
            Terminator::Branch { condition, .. } if value_type(f, condition)? != &Type::Bool => {
                return Err(failure("actual branch condition is not Bool"));
            }
            _ => {}
        }
    }
    b.product(
        f.blocks.len(),
        f.values.len(),
        2,
        "stored actual SSA availability",
    )?;
    if scalar_ffi {
        super::stored_callables::foreign_regions(f, preds, reachable, b)?;
    }
    let mut inputs = vec![vec![true; f.values.len()]; f.blocks.len()];
    let mut outputs = inputs.clone();
    fee(b, f.values.len())?;
    inputs[f.entry.0 as usize].fill(false);
    loop {
        let mut changed = false;
        for (bi, block) in f.blocks.iter().enumerate() {
            if !reachable[bi] {
                continue;
            }
            fee(b, f.values.len())?;
            let mut live = if bi == f.entry.0 as usize {
                vec![false; f.values.len()]
            } else {
                vec![true; f.values.len()]
            };
            if bi != f.entry.0 as usize {
                for p in &preds[bi] {
                    if reachable[*p] {
                        fee(b, live.len())?;
                        for (a, z) in live.iter_mut().zip(&outputs[*p]) {
                            *a &= *z;
                        }
                    }
                }
            }
            fee(b, live.len())?;
            inputs[bi].clone_from(&live);
            for i in &block.instructions {
                if let Some(v) = i.result {
                    fee(b, 1)?;
                    live[v.0 as usize] = true;
                }
            }
            fee(b, live.len())?;
            if live != outputs[bi] {
                outputs[bi] = live;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    for (bi, block) in f.blocks.iter().enumerate() {
        if !reachable[bi] {
            continue;
        }
        fee(b, inputs[bi].len())?;
        let mut live = inputs[bi].clone();
        for i in &block.instructions {
            for v in resources::uses(&i.kind) {
                fee(b, 1)?;
                if !live[v.0 as usize] {
                    return Err(failure(
                        "actual operand definition does not dominate its stored/loan use",
                    ));
                }
            }
            if let Some(v) = i.result {
                live[v.0 as usize] = true;
            }
        }
        if let Terminator::Return(Some(v)) | Terminator::Branch { condition: v, .. } =
            block.terminator
        {
            fee(b, 1)?;
            if !live.get(v.0 as usize).copied().unwrap_or(false) {
                return Err(failure("terminator operand is not actually available"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    mod fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/language-readiness/terminal/higher-rank-borrows/stored_flow_fixture.rs"
        ));
    }
    #[test]
    fn owned_fixed_once_keeps_resource_and_class_proofs_independent() {
        let m = fixture::captured_owned_fixed(false);
        let limits = Limits::default();
        let plan = super::super::analyze(&m, &limits).unwrap();
        assert!(plan.functions[0].holds[7].roots.is_empty());
        assert_eq!(
            plan.functions[0].holds[8].roots,
            vec![HoldRoot::Local(LocalId(1))]
        );
        assert!(super::super::analyze(&fixture::captured_owned_fixed(true), &limits).is_err());
        for mode in 0..5 {
            let mut raw = m.clone();
            let entry = &mut raw.functions[0];
            match mode {
                0 => {
                    let mut take = entry.blocks[0].instructions[0].clone();
                    let id = ValueId(entry.values.len() as u32);
                    entry.values.push(entry.values[7].clone());
                    take.result = Some(id);
                    entry.blocks[0].instructions.insert(1, take);
                }
                1 => {
                    entry.blocks[0].instructions[0].kind = InstructionKind::ReadCapture { index: 0 }
                }
                2 => {
                    let code = &mut entry.blocks[0].instructions;
                    let at = code
                        .iter()
                        .position(|i| matches!(i.kind, InstructionKind::DropManaged { .. }))
                        .unwrap();
                    let drop = code.remove(at);
                    let read = code
                        .iter()
                        .position(|i| matches!(i.kind, InstructionKind::ReadDesc { .. }))
                        .unwrap();
                    code.insert(read, drop);
                }
                3 => {
                    let maker = raw.functions[1].blocks[0]
                        .instructions
                        .iter_mut()
                        .find(|i| matches!(i.kind, InstructionKind::MakeManaged { .. }))
                        .unwrap();
                    maker.kind = InstructionKind::MakeManaged { fields: vec![] };
                }
                _ => {
                    let read = entry.blocks[0]
                        .instructions
                        .iter_mut()
                        .find(|i| matches!(i.kind, InstructionKind::ReadDesc { .. }))
                        .unwrap();
                    if let InstructionKind::ReadDesc { index, .. } = &mut read.kind {
                        *index = 9;
                    }
                }
            }
            assert!(
                super::super::analyze(&raw, &limits).is_err(),
                "owned neighbor {mode}"
            );
        }
    }
    #[test]
    fn owned_fixed_once_has_one_exact_fresh_and_warm_request_budget() {
        let m = fixture::captured_owned_fixed(false);
        let limits = |n| Limits {
            max_analysis_steps: n,
            ..Limits::default()
        };
        let expected = super::super::analyze(&m, &Limits::default()).unwrap();
        let (mut low, mut high) = (1, Limits::default().max_analysis_steps);
        while low < high {
            let mid = low + (high - low) / 2;
            if super::super::analyze(&m, &limits(mid)).is_ok() {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        assert_eq!(
            // Two complete nominal rows add 74; fifteen actual retained
            // boundary scans add 45. The prior N remains a limit control.
            low,
            14_077 + 74 + 15 * 3,
            "bounded actual identity/ownership work changed"
        );
        assert!(
            super::super::analyze(&m, &limits(14_077))
                .unwrap_err()
                .iter()
                .any(|d| d.code == "E-RESOURCE-LIMIT")
        );
        let mut session = super::super::AnalysisSession::default();
        for warm in [false, true] {
            assert_eq!(session.analyze(&m, &limits(low)).unwrap(), expected);
            if warm {
                assert!(session.last_stats().reused_functions > 0);
            }
            let error = session.analyze(&m, &limits(low - 1)).unwrap_err();
            assert!(
                error.iter().any(|d| d.code == "E-RESOURCE-LIMIT"),
                "{error:?}"
            );
            assert_eq!(super::super::analyze(&m, &limits(low)).unwrap(), expected);
        }
        println!("owned Fixed Analyze N={low}; N-1/fresh/warm identical");
    }
    #[test]
    fn owned_fixed_heads_require_actual_resource_definitions() {
        let m = fixture::captured_owned_fixed(false);
        let limits = Limits::default();
        let mut accepted = Vec::new();
        for mode in 0..7 {
            let mut raw = m.clone();
            match mode {
                0 => {
                    raw.resources[0].name = "Other".into();
                    let Type::Record { name, .. } = &mut raw.resources[0].payload else {
                        unreachable!()
                    };
                    *name = "Other".into();
                }
                1 => raw.resources.clear(),
                2 => raw.resources.push(raw.resources[0].clone()),
                3 => {
                    let mut local = raw.functions[1].locals[0].clone();
                    local.id = LocalId(raw.functions[1].locals.len() as u32);
                    local.name = "unused_other".into();
                    local.ty = Type::Managed(Box::new(Type::Record {
                        name: "Other".into(),
                        fields: vec![("n".into(), Type::I32)],
                    }));
                    raw.functions[1].locals.push(local);
                }
                4 => {
                    let Type::Record { fields, .. } = &mut raw.resources[0].payload else {
                        unreachable!()
                    };
                    fields[0].1 = Type::I64;
                }
                5 => raw.resources[0].name = "Other".into(),
                _ => raw.resources[0].teardown = Some(raw.functions[0].id),
            }
            let result = super::super::analyze(&raw, &limits);
            eprintln!(
                "resource identity neighbor {mode}: {:?}",
                result.as_ref().err()
            );
            if result.is_ok() {
                accepted.push(mode);
            }
        }
        assert!(
            accepted.is_empty(),
            "unbound resource heads accepted: {accepted:?}"
        );
        let mut renamed = m.clone();
        renamed.resources[0].name = "Renamed".into();
        if let Type::Record { name, .. } = &mut renamed.resources[0].payload {
            *name = "Renamed".into();
        }
        for ty in renamed
            .functions
            .iter_mut()
            .flat_map(|f| f.locals.iter_mut().map(|l| &mut l.ty).chain(&mut f.values))
            .chain(renamed.closures.iter_mut().flat_map(|d| &mut d.captures))
        {
            if let Type::Managed(payload) | Type::Desc(payload) = ty {
                let Type::Record { name, .. } = payload.as_mut() else {
                    unreachable!()
                };
                *name = "Renamed".into();
            }
        }
        super::super::analyze(&renamed, &limits).unwrap();
    }
    use fixture::{address, instruction, module};
    fn prove(module: &TypedModule) -> CompileResult<Vec<Facts>> {
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        inventory(module, &limits, &mut b)?;
        for f in &module.functions {
            header(module, f, &mut b)?;
        }
        derive(
            module,
            &vec![(vec![vec![]], vec![true]); module.functions.len()],
            &mut b,
        )
    }
    #[test]
    fn captured_fixed_helper_payload_naturally_projects_parameter_roots() {
        for mode in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
            let m = fixture::captured_fixed_helper(mode, false);
            let limits = Limits::default();
            let plan = super::super::analyze(&m, &limits).unwrap();
            assert_eq!(
                plan.functions[2].holds[0].roots,
                vec![HoldRoot::Parameter(0)],
                "{mode:?}"
            );
            let mut b = Budget::new(&limits);
            let helper = &m.functions[2];
            let mut loans = vec![Loan::default(); helper.values.len()];
            let entry = seed(&m, helper, &mut loans, &mut b).unwrap();
            let actual = &entry.locals[&LocalId(0)];
            assert_eq!(
                actual.environment.len(),
                2,
                "outer Address and its distinct physical field target"
            );
            assert_eq!(actual.held.loans.len(), 2);
            for (path, permission) in &actual.environment {
                assert!(matches!(
                    path.as_slice(),
                    [PlaceProjection::Field(0)]
                        | [PlaceProjection::Field(0), PlaceProjection::Field(0)]
                ));
                for target in &permission.targets {
                    assert_eq!(target.base, HoldRoot::Parameter(0));
                    assert_eq!(&target.stored, path);
                }
                for loan in &permission.loans {
                    assert_eq!(loans[*loan].targets, permission.targets);
                    assert!(!loans[*loan].mutable);
                }
            }
        }
    }
    #[test]
    fn captured_fixed_helper_substitutes_actual_local_capture_targets() {
        // The independent review's neutral actual-IR constructor: a real
        // local scalar is retained through a Tag and &mut Closure helper.
        let mut m = fixture::captured_fixed_helper(CallableKind::FnMut, false);
        m.functions[1] = fixture::captured_fixed_parent_end(false).functions[1].clone();
        let f = &mut m.functions[1];
        f.locals[2].mutable = true;
        f.values.push(address(Type::Closure(ClosureId(0)), true));
        let code = &mut f.blocks[0].instructions;
        code.pop();
        code.push(instruction(
            Some(6),
            InstructionKind::BorrowLocalMut { local: LocalId(2) },
        ));
        code.push(instruction(
            Some(5),
            InstructionKind::Call {
                function: FunctionId(2),
                arguments: vec![ValueId(6), ValueId(3)],
            },
        ));
        super::super::analyze(&m, &Limits::default()).unwrap();
        let at = m.functions[1].blocks[0].instructions.len() - 1;
        m.functions[1].blocks[0].instructions.insert(
            at,
            instruction(None, InstructionKind::StorageDead(LocalId(0))),
        );
        let error = super::super::analyze(&m, &Limits::default()).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.contains("actual stored target/parent loan")),
            "{error:?}"
        );
    }
    #[test]
    fn captured_fixed_phantom_needs_actual_header_origin() {
        for header in [true, false] {
            let mut m = if header {
                fixture::captured_fixed(CallableKind::Fn, false)
            } else {
                fixture::captured_fixed_parent_end(false)
            };
            let mut phantom = m.life_instances[0].ty.clone();
            let Type::BorrowedStorage { storage, life } = &mut phantom else {
                unreachable!()
            };
            **storage = Type::Record {
                name: "Phantom".into(),
                fields: vec![("n".into(), Type::I64)],
            };
            life.slots.clear();
            m.life_instances[0].ty = phantom.clone();
            let ty = address(phantom.clone(), false);
            m.closures[0].captures = vec![ty.clone()];
            m.closures[0].parameters = vec![ty.clone()];
            let entry = &mut m.functions[0];
            entry.locals[0].ty = ty.clone();
            entry.values = vec![ty.clone(), Type::I64];
            entry.blocks[0].instructions = vec![
                instruction(Some(0), InstructionKind::ReadCapture { index: 0 }),
                instruction(Some(1), InstructionKind::Constant(Constant::I64(42))),
            ];
            entry.blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
            let caller = &mut m.functions[1];
            if header {
                caller.locals[..2]
                    .iter_mut()
                    .for_each(|l| l.ty = ty.clone());
                caller.values[..2].fill(ty);
            } else {
                caller.locals[1].ty = phantom.clone();
                caller.values[2] = phantom;
                caller.values[3] = ty;
                let InstructionKind::MakeAggregate { fields } =
                    &mut caller.blocks[0].instructions[3].kind
                else {
                    unreachable!()
                };
                fields[0] = ValueId(0); // scalar field does not ground its nominal class
            }
            let result = super::super::analyze(&m, &Limits::default());
            if header {
                result.unwrap();
            } else {
                let error = result.unwrap_err();
                assert!(
                    error
                        .iter()
                        .any(|d| d.message.contains("no actual header or Address origin")),
                    "{error:?}"
                );
            }
        }
    }
    #[test]
    fn captured_fixed_ordinary_address_needs_no_nominal_trigger() {
        for wrong in [false, true] {
            let mut m = fixture::captured_fixed(CallableKind::Fn, wrong);
            let ty = address(Type::I64, false);
            let contract = StorageLifeContract {
                fixed_binder_count: 1,
                invocation_binder_count: 0,
                bindings: vec![
                    StorageLifeBinding {
                        position: StoragePosition {
                            base: StorageBase::Parameter(0),
                            path: vec![],
                        },
                        binder: StorageBinder::Fixed(0),
                    },
                    StorageLifeBinding {
                        position: StoragePosition {
                            base: StorageBase::Capture(0),
                            path: vec![],
                        },
                        binder: StorageBinder::Fixed(0),
                    },
                ],
                classes: vec![],
            };
            m.life_declarations.clear();
            m.life_instances.clear();
            m.closures[0].captures = vec![ty.clone()];
            m.closures[0].parameters = vec![ty.clone()];
            m.closures[0].storage_life = Some(contract.clone());
            let entry = &mut m.functions[0];
            entry.locals[0].ty = ty.clone();
            entry.storage_life = Some(contract);
            entry.values = vec![ty.clone(), Type::I64, ty.clone(), Type::I64, Type::I64];
            entry.blocks[0].instructions = vec![
                instruction(Some(0), InstructionKind::ReadCapture { index: 0 }),
                instruction(
                    Some(1),
                    InstructionKind::ReadAddress {
                        address: ValueId(0),
                    },
                ),
                instruction(Some(2), InstructionKind::Load(LocalId(0))),
                instruction(
                    Some(3),
                    InstructionKind::ReadAddress {
                        address: ValueId(2),
                    },
                ),
                instruction(
                    Some(4),
                    InstructionKind::Binary {
                        op: BinaryOp::Add,
                        left: ValueId(1),
                        right: ValueId(3),
                    },
                ),
            ];
            entry.blocks[0].terminator = Terminator::Return(Some(ValueId(4)));
            let caller = &mut m.functions[1];
            for local in &mut caller.locals[..2] {
                local.ty = ty.clone();
            }
            caller.values[..2].fill(ty);
            let header = caller.storage_life.as_mut().unwrap();
            header.fixed_binder_count = 0;
            header.classes.clear();
            let result = super::super::analyze(&m, &Limits::default());
            if wrong {
                let error = result.unwrap_err();
                assert!(
                    error
                        .iter()
                        .any(|d| d.message.contains("actual retained classes")),
                    "{error:?}"
                );
            } else {
                result.unwrap();
            }
        }
    }
    #[test]
    fn captured_fixed_once_move_keeps_body_parameter_classes() {
        let mut m = fixture::captured_fixed_helper(CallableKind::FnOnce, false);
        let helper = &mut m.functions[2];
        let ty = helper.locals[1].ty.clone();
        let mut local = helper.locals[1].clone();
        local.id = LocalId(2);
        helper.locals.push(local);
        helper.parameters.push(LocalId(2));
        helper.values.push(ty);
        helper.blocks[0]
            .instructions
            .insert(2, instruction(Some(3), InstructionKind::Load(LocalId(2))));
        let InstructionKind::CallClosureOnce { arguments, .. } =
            &mut helper.blocks[0].instructions[3].kind
        else {
            unreachable!()
        };
        arguments[0] = ValueId(3);
        let header = helper.storage_life.as_mut().unwrap();
        header.fixed_binder_count = 3;
        header.invocation_binder_count = 2;
        header.bindings.push(StorageLifeBinding {
            position: StoragePosition {
                base: StorageBase::Parameter(2),
                path: vec![],
            },
            binder: StorageBinder::Invocation(1),
        });
        header.classes.push(SignatureLifeClassBinding {
            position: SignatureLifeClass {
                base: StorageBase::Parameter(2),
                path: vec![LifeTypeProjection::Pointee],
                class: 0,
            },
            binder: StorageBinder::Fixed(2),
        });
        let InstructionKind::Call { arguments, .. } = &mut m.functions[1].blocks[0]
            .instructions
            .last_mut()
            .unwrap()
            .kind
        else {
            unreachable!()
        };
        arguments.push(ValueId(1));
        let error = super::super::analyze(&m, &Limits::default()).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.contains("actual retained classes")),
            "{error:?}"
        );
    }
    #[test]
    fn captured_fixed_persists_while_invocation_is_fresh() {
        for mode in [CallableKind::Fn, CallableKind::FnMut] {
            let mut m = fixture::captured_fixed(mode, false);
            let caller = &mut m.functions[1];
            let header = caller.storage_life.as_mut().unwrap();
            header.fixed_binder_count = 1;
            header.classes[1].binder = StorageBinder::Fixed(0);
            caller.values.push(Type::I64);
            caller.blocks[0].instructions.push(instruction(
                Some(4),
                if mode == CallableKind::Fn {
                    InstructionKind::CallClosureShared {
                        callee: LocalId(2),
                        arguments: vec![ValueId(1)],
                    }
                } else {
                    InstructionKind::CallClosureMut {
                        callee: LocalId(2),
                        arguments: vec![ValueId(1)],
                    }
                },
            ));
            caller.blocks[0].terminator = Terminator::Return(Some(ValueId(4)));
            super::super::analyze(&m, &Limits::default()).unwrap();
            // Both independent outer input borrows are used with the same
            // retained Tag class. Reusing Invocation would incorrectly reject.
        }
    }
    #[test]
    fn captured_fixed_helper_cannot_choose_context_from_its_claim() {
        for missing in [true, false] {
            let mut m = fixture::captured_fixed_helper(CallableKind::Fn, false);
            if missing {
                m.functions[2].storage_life = None;
            } else {
                let life = m.functions[2].callable_contract.as_mut().unwrap().roles[0]
                    .life
                    .as_mut()
                    .unwrap();
                life.invocation_binder_count = 2;
                life.classes[0].binder = StorageBinder::Invocation(1);
                life.bindings[1].binder = StorageBinder::Fixed(0);
                life.classes[1].binder = StorageBinder::Fixed(1);
            }
            let error = super::super::analyze(&m, &Limits::default()).unwrap_err();
            assert!(
                error.iter().all(|d| d.code != "E-RESOURCE-LIMIT"),
                "{error:?}"
            );
            println!(
                "missing/mismatched helper context {missing}: {}",
                error[0].message
            );
        }
    }
    #[test]
    fn captured_fixed_relations_and_consumption_are_not_permissions() {
        let base = fixture::captured_fixed(CallableKind::FnOnce, false);
        for mutation in 0..8 {
            let mut m = base.clone();
            match mutation {
                0 => {
                    m.closures[0].storage_life.as_mut().unwrap().bindings[1].binder =
                        StorageBinder::Invocation(0);
                    m.functions[0].storage_life = m.closures[0].storage_life.clone();
                }
                1 => {
                    m.closures[0].storage_life.as_mut().unwrap().classes[1]
                        .position
                        .path
                        .clear();
                    m.functions[0].storage_life = m.closures[0].storage_life.clone();
                }
                2 => {
                    m.functions[0].storage_life.as_mut().unwrap().classes[1].binder =
                        StorageBinder::Fixed(1);
                }
                3 => {
                    m.functions[1].values.push(Type::I64);
                    m.functions[1].blocks[0].instructions.push(instruction(
                        Some(4),
                        InstructionKind::CallClosureOnce {
                            callee: ValueId(2),
                            arguments: vec![ValueId(0)],
                        },
                    ));
                }
                4 => {
                    m.functions[1].values.push(Type::Closure(ClosureId(0)));
                    let code = &mut m.functions[1].blocks[0].instructions;
                    code.insert(
                        3,
                        instruction(
                            None,
                            InstructionKind::Store {
                                local: LocalId(2),
                                value: ValueId(2),
                            },
                        ),
                    );
                    code.insert(4, instruction(Some(4), InstructionKind::Load(LocalId(2))));
                }
                5 => {
                    m.closures[0].kind = CallableKind::FnMut;
                }
                6 => {
                    m.functions[1].return_type = Type::Closure(ClosureId(0));
                    m.functions[1].blocks[0].instructions.pop();
                    m.functions[1].blocks[0].terminator = Terminator::Return(Some(ValueId(2)));
                }
                _ => m.functions[1].values.push(Type::I64),
            }
            if mutation == 5 {
                // Finite nonlending FnMut-to-Once adaptation consumes this
                // actual environment under the independent owner ledger.
                super::super::analyze(&m, &Limits::default()).unwrap();
                continue;
            }
            let error = super::super::analyze(&m, &Limits::default()).unwrap_err();
            assert!(
                error.iter().all(|d| d.code != "E-RESOURCE-LIMIT"),
                "mutation {mutation}: {error:?}"
            );
            if mutation == 6 {
                assert!(
                    error
                        .iter()
                        .any(|d| d.message.contains("retained result target transfer")),
                    "{error:?}"
                );
            }
            println!(
                "malformed/consume neighbor {mutation}: {}",
                error[0].message
            );
        }
    }
    #[test]
    fn captured_fixed_fnmut_writes_its_actual_inline_scalar_capture() {
        let mut m = fixture::captured_fixed(CallableKind::FnMut, false);
        m.closures[0].captures.push(Type::I64);
        m.functions[0].blocks[0].instructions.push(instruction(
            None,
            InstructionKind::WriteCapture {
                index: 1,
                value: ValueId(6),
            },
        ));
        let caller = &mut m.functions[1];
        caller.values.push(Type::I64);
        caller.blocks[0].instructions.insert(
            2,
            instruction(Some(4), InstructionKind::Constant(Constant::I64(0))),
        );
        let InstructionKind::MakeClosure { captures, .. } =
            &mut caller.blocks[0].instructions[3].kind
        else {
            unreachable!()
        };
        captures.push(ValueId(4));
        super::super::analyze(&m, &Limits::default()).unwrap();
        m.closures[0].kind = CallableKind::Fn;
        let error = super::super::analyze(&m, &Limits::default()).unwrap_err();
        assert!(
            error.iter().any(|d| d.message.contains("capture write")),
            "{error:?}"
        );
    }
    #[test]
    fn captured_fixed_context_uses_one_request_budget() {
        for mode in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
            let m = fixture::captured_fixed_helper(mode, false);
            let limit = |n| Limits {
                max_analysis_steps: n,
                ..Limits::default()
            };
            let expected = super::super::analyze(&m, &Limits::default()).unwrap();
            let mut low = 1;
            let mut high = Limits::default().max_analysis_steps;
            while low < high {
                let mid = low + (high - low) / 2;
                if super::super::analyze(&m, &limit(mid)).is_ok() {
                    high = mid;
                } else {
                    low = mid + 1;
                }
            }
            let error = super::super::analyze(&m, &limit(low - 1)).unwrap_err();
            assert!(
                error.iter().any(|d| d.code == "E-RESOURCE-LIMIT"),
                "{error:?}"
            );
            assert_eq!(super::super::analyze(&m, &limit(low)).unwrap(), expected);
            assert_eq!(super::super::analyze(&m, &limit(low)).unwrap(), expected);
            println!(
                "{mode:?} full Typed helper Analyze N={low}; N-1 ResourceLimit/fresh N identical"
            );
        }
    }
    #[test]
    fn captured_fixed_instances_and_helpers_keep_receiver_context() {
        for mode in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
            for make in [
                fixture::captured_fixed_instances,
                fixture::captured_fixed_helper,
            ] {
                super::super::analyze(&make(mode, false), &Limits::default()).unwrap();
                let error =
                    super::super::analyze(&make(mode, true), &Limits::default()).unwrap_err();
                assert!(
                    error
                        .iter()
                        .any(|d| d.message.contains("actual retained classes")),
                    "{mode:?}: {error:?}"
                );
            }
        }
    }
    #[test]
    fn captured_fixed_keeps_actual_parent_alive() {
        super::super::analyze(
            &fixture::captured_fixed_parent_end(false),
            &Limits::default(),
        )
        .unwrap();
        let error = super::super::analyze(
            &fixture::captured_fixed_parent_end(true),
            &Limits::default(),
        )
        .unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.contains("live access overlaps")),
            "{error:?}"
        );
    }
    #[test]
    fn captured_fixed_uses_actual_environment_in_all_three_modes() {
        for mode in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
            super::super::analyze(&fixture::captured_fixed(mode, false), &Limits::default())
                .unwrap();
            let error =
                super::super::analyze(&fixture::captured_fixed(mode, true), &Limits::default())
                    .unwrap_err();
            assert!(
                error
                    .iter()
                    .any(|d| d.message.contains("actual retained classes")),
                "{mode:?}: {error:?}"
            );
        }
    }
    #[test]
    fn qualified_role_proofs_share_the_actual_request_allowance() {
        let m = fixture::qualified_two_fn_roles(false);
        let limit = |n| Limits {
            max_analysis_steps: n,
            ..Limits::default()
        };
        let expected = super::super::analyze(&m, &Limits::default()).unwrap();
        let mut low = 0;
        let mut high = Limits::default().max_analysis_steps;
        while low < high {
            let mid = low + (high - low) / 2;
            if super::super::analyze(&m, &limit(mid)).is_ok() {
                high = mid
            } else {
                low = mid + 1
            }
        }
        let error = super::super::analyze(&m, &limit(low - 1)).unwrap_err();
        assert!(
            error.iter().any(|d| d.code == "E-RESOURCE-LIMIT"),
            "{error:?}"
        );
        assert_eq!(super::super::analyze(&m, &limit(low)).unwrap(), expected);
        assert_eq!(super::super::analyze(&m, &limit(low)).unwrap(), expected);
        println!("342 qualified Fn role Analyze N={low}; N-1 ResourceLimit/fresh N identical");
    }
    #[test]
    fn qualified_callback_arguments_keep_simultaneous_exclusive_windows() {
        super::super::analyze(
            &fixture::qualified_fn_exclusive_call(false),
            &Limits::default(),
        )
        .unwrap();
        let error = super::super::analyze(
            &fixture::qualified_fn_exclusive_call(true),
            &Limits::default(),
        )
        .unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.contains("live access overlaps")),
            "{error:?}"
        );
    }
    #[test]
    fn the_same_closure_is_checked_under_each_actual_parameter_role() {
        super::super::analyze(&fixture::qualified_two_fn_roles(false), &Limits::default()).unwrap();
        let error =
            super::super::analyze(&fixture::qualified_two_fn_roles(true), &Limits::default())
                .unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.contains("actual retained classes")),
            "{error:?}"
        );
    }
    #[test]
    fn qualified_fn_roles_use_actual_entry_and_fresh_argument_class_equations() {
        super::super::analyze(
            &fixture::qualified_fn_role(false, false),
            &Limits::default(),
        )
        .unwrap();
        for m in [
            fixture::qualified_fn_role(true, false),
            fixture::qualified_fn_role(false, true),
        ] {
            let error = super::super::analyze(&m, &Limits::default()).unwrap_err();
            assert!(
                error
                    .iter()
                    .any(|d| d.message.contains("actual retained classes")),
                "{error:?}"
            );
        }
    }
    #[test]
    fn a_pointer_fixed_class_without_enclosing_context_is_not_fresh_per_call() {
        super::super::analyze(
            &fixture::phantom_class_calls(true, false),
            &Limits::default(),
        )
        .unwrap();
        let error = super::super::analyze(&fixture::unbound_fixed_pointer(), &Limits::default())
            .unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.contains("genuine enclosing semantic context")),
            "{error:?}"
        );
    }
    #[test]
    fn rich_indirect_actual_exclusive_windows_are_simultaneous() {
        super::super::analyze(
            &fixture::rich_marker_exclusive_call(false),
            &Limits::default(),
        )
        .unwrap();
        let error = super::super::analyze(
            &fixture::rich_marker_exclusive_call(true),
            &Limits::default(),
        )
        .unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.contains("live access overlaps")),
            "{error:?}"
        );
    }
    #[test]
    fn rich_class_equations_share_the_request_budget_and_fresh_boundary() {
        let m = fixture::rich_marker_pointer(false, false);
        let limit = |n| Limits {
            max_analysis_steps: n,
            ..Limits::default()
        };
        let expected = super::super::analyze(&m, &Limits::default()).unwrap();
        let mut low = 0;
        let mut high = Limits::default().max_analysis_steps;
        while low < high {
            let mid = low + (high - low) / 2;
            if super::super::analyze(&m, &limit(mid)).is_ok() {
                high = mid
            } else {
                low = mid + 1
            }
        }
        let error = super::super::analyze(&m, &limit(low - 1)).unwrap_err();
        assert!(
            error.iter().any(|d| d.code == "E-RESOURCE-LIMIT"),
            "{error:?}"
        );
        assert_eq!(super::super::analyze(&m, &limit(low)).unwrap(), expected);
        assert_eq!(super::super::analyze(&m, &limit(low)).unwrap(), expected);
        println!("342 rich class Analyze N={low}; N-1 ResourceLimit/fresh N identical");
    }
    #[test]
    fn rich_pointer_proves_target_implication_and_actual_outer_to_class_equations() {
        super::super::analyze(
            &fixture::rich_marker_pointer(false, false),
            &Limits::default(),
        )
        .unwrap();
        let bad = super::super::analyze(
            &fixture::rich_marker_pointer(true, false),
            &Limits::default(),
        )
        .unwrap_err();
        assert!(
            bad.iter()
                .any(|d| d.message.contains("actual retained classes")),
            "{bad:?}"
        );
        let bad = super::super::analyze(
            &fixture::rich_marker_pointer(false, true),
            &Limits::default(),
        )
        .unwrap_err();
        assert!(
            bad.iter()
                .any(|d| d.message.contains("entail actual target")),
            "{bad:?}"
        );
    }
    #[test]
    fn a_fresh_phantom_class_is_instantiated_once_and_retained_between_actual_calls() {
        for indirect in [false, true] {
            super::super::analyze(
                &fixture::phantom_class_calls(indirect, false),
                &Limits::default(),
            )
            .unwrap();
            let error = super::super::analyze(
                &fixture::phantom_class_calls(indirect, true),
                &Limits::default(),
            )
            .unwrap_err();
            assert!(
                error
                    .iter()
                    .any(|d| d.message.contains("actual retained classes")),
                "{error:?}"
            );
        }
    }
    #[test]
    fn actual_shared_field_survives_carrier_end_and_two_forwarding_helpers() {
        let m = module();
        let facts = prove(&m).unwrap();
        assert_eq!(
            facts[3].values[4],
            vec![Target {
                base: HoldRoot::Local(LocalId(0)),
                stored: vec![],
                place: vec![]
            }]
        );
        super::super::analyze(&m, &Limits::default()).unwrap();
    }
    #[test]
    fn copied_field_keeps_target_parent_loan_after_carrier_end() {
        let mut m = module();
        let f = &mut m.functions[3];
        f.values.push(address(Type::I64, true));
        f.blocks[0].instructions.insert(
            8,
            instruction(
                Some(6),
                InstructionKind::BorrowLocalMut { local: LocalId(0) },
            ),
        );
        assert!(prove(&m).is_err());
    }
    #[test]
    fn unresolved_recursive_result_is_not_an_empty_authority() {
        let mut m = module();
        let f = &mut m.functions[4];
        f.values.push(address(Type::I64, false));
        f.blocks[0].instructions.push(instruction(
            Some(1),
            InstructionKind::Call {
                function: FunctionId(4),
                arguments: vec![ValueId(0)],
            },
        ));
        f.blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
        assert!(prove(&m).is_err());
    }
    #[test]
    fn actual_calls_create_children_and_do_not_duplicate_exclusive_input_identity() {
        let positive = fixture::exclusive_returns(false);
        super::super::analyze(&positive, &Limits::default()).unwrap();
        let bad = fixture::exclusive_returns(true);
        let error = super::super::analyze(&bad, &Limits::default()).unwrap_err();
        assert!(
            error[0].message.contains("live access overlaps"),
            "{error:?}"
        );
    }
}
