//! Independent stored-target reconstruction. Sorted vectors and batch CFG
//! solving deliberately do not consume analyzer roots, summaries, or claims.
use super::*;

type Path = Vec<PlaceProjection>;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Source {
    base: HoldRoot,
    leaf: Path,
    inline: Path,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Permission {
    sources: Vec<Source>,
    loans: Vec<usize>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Payload {
    address: Permission,
    leaves: Vec<(Path, Permission)>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Frame {
    locals: Vec<Payload>,
    external: Vec<Payload>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Loan {
    sources: Vec<Source>,
    parents: Vec<usize>,
    exclusive: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Summary {
    result: Vec<(Path, Vec<Source>)>,
    apart: Vec<(Source, Source)>,
    complete: bool,
}
struct Solution {
    values: Vec<Payload>,
    outputs: Vec<Frame>,
    entry: Frame,
    loans: Vec<Loan>,
}

fn err(text: &str) -> Diagnostic {
    reject(
        "E-VERIFY-STORED",
        None,
        format!("stored target proof: {text}"),
    )
}
fn amount(a: usize, b: usize) -> Check<usize> {
    a.checked_mul(b)
        .ok_or_else(|| limit("stored comparison work overflow"))
}
fn width(s: &Source) -> usize {
    s.leaf
        .len()
        .saturating_add(s.inline.len())
        .saturating_add(1)
}
fn source_work(s: &[Source], b: &mut Budget) -> Check<()> {
    b.spend(s.len())?;
    for x in s {
        b.spend(width(x))?;
    }
    Ok(())
}
fn permission_work(p: &Permission, b: &mut Budget) -> Check<()> {
    source_work(&p.sources, b)?;
    b.spend(p.loans.len())
}
fn payload_work(p: &Payload, b: &mut Budget) -> Check<()> {
    permission_work(&p.address, b)?;
    b.spend(p.leaves.len())?;
    for (path, p) in &p.leaves {
        b.spend(path.len())?;
        permission_work(p, b)?;
    }
    Ok(())
}
fn frame_work(f: &Frame, b: &mut Budget) -> Check<()> {
    b.spend(f.locals.len().saturating_add(f.external.len()))?;
    for p in f.locals.iter().chain(&f.external) {
        payload_work(p, b)?;
    }
    Ok(())
}
fn copy_payload(p: &Payload, b: &mut Budget) -> Check<Payload> {
    payload_work(p, b)?;
    Ok(p.clone())
}
fn copy_frame(f: &Frame, b: &mut Budget) -> Check<Frame> {
    frame_work(f, b)?;
    Ok(f.clone())
}

fn add_sources(to: &mut Vec<Source>, from: &[Source], b: &mut Budget) -> Check<bool> {
    let mut changed = false;
    for source in from {
        b.spend(amount(to.len().saturating_add(1), width(source))?)?;
        if !to.contains(source) {
            b.spend(width(source))?;
            to.push(source.clone());
            changed = true;
        }
    }
    // Canonical order is not an authority; pay all possible full comparisons.
    b.spend(to.len())?;
    let max = to.iter().map(width).max().unwrap_or(1);
    b.spend(amount(amount(to.len(), to.len())?, max)?)?;
    to.sort();
    Ok(changed)
}
fn add_loans(to: &mut Vec<usize>, from: &[usize], b: &mut Budget) -> Check<bool> {
    let mut changed = false;
    b.spend(amount(
        to.len().saturating_add(from.len()).saturating_add(1),
        from.len().saturating_add(1),
    )?)?;
    for id in from {
        if !to.contains(id) {
            to.push(*id);
            changed = true;
        }
    }
    to.sort_unstable();
    Ok(changed)
}
fn join_permission(to: &mut Permission, from: &Permission, b: &mut Budget) -> Check<bool> {
    let a = add_sources(&mut to.sources, &from.sources, b)?;
    Ok(add_loans(&mut to.loans, &from.loans, b)? || a)
}
fn leaf<'a>(
    p: &'a Payload,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<Option<&'a Permission>> {
    b.spend(amount(
        p.leaves.len().saturating_add(1),
        path.len().saturating_add(1),
    )?)?;
    Ok(p.leaves.iter().find(|(p, _)| p == path).map(|(_, p)| p))
}
fn add_leaf(
    to: &mut Payload,
    path: &[PlaceProjection],
    from: &Permission,
    b: &mut Budget,
) -> Check<bool> {
    b.spend(amount(
        to.leaves.len().saturating_add(1),
        path.len().saturating_add(1),
    )?)?;
    if let Some((_, p)) = to.leaves.iter_mut().find(|(p, _)| p == path) {
        join_permission(p, from, b)
    } else {
        b.spend(path.len())?;
        permission_work(from, b)?;
        to.leaves.push((path.to_vec(), from.clone()));
        Ok(true)
    }
}
fn join_payload(to: &mut Payload, from: &Payload, b: &mut Budget) -> Check<bool> {
    let mut changed = join_permission(&mut to.address, &from.address, b)?;
    for (path, p) in &from.leaves {
        changed |= add_leaf(to, path, p, b)?;
    }
    Ok(changed)
}
fn join_frame(to: &mut Frame, from: &Frame, b: &mut Budget) -> Check<()> {
    b.spend(to.locals.len().saturating_add(to.external.len()))?;
    for (a, z) in to
        .locals
        .iter_mut()
        .zip(&from.locals)
        .chain(to.external.iter_mut().zip(&from.external))
    {
        join_payload(a, z, b)?;
    }
    Ok(())
}
fn qualified(t: &Type) -> Option<(&Type, &StorageLifeShape)> {
    if let Type::BorrowedStorage { storage, life } = t {
        Some((storage, life))
    } else {
        None
    }
}
fn addressed(t: &Type) -> Option<(&Type, &StorageLifeShape)> {
    if let Type::Address { inner, .. } = t {
        qualified(inner)
    } else {
        None
    }
}
fn vt(f: &TypedFunction, v: ValueId) -> Check<&Type> {
    f.values
        .get(v.0 as usize)
        .ok_or_else(|| err("operand outside actual value table"))
}

pub(super) fn type_work(ty: &Type, limits: &Limits, b: &mut Budget) -> Check<()> {
    b.spend(1)?;
    let mut stack = vec![(ty, 0usize)];
    while let Some((t, depth)) = stack.pop() {
        b.spend(3)?;
        if depth > limits.max_call_depth {
            return Err(limit("stored type depth exceeds verification limit"));
        }
        match t {
            Type::BorrowedStorage { storage, life } => {
                b.spend(
                    life.slots
                        .len()
                        .saturating_add(life.retained.len())
                        .saturating_add(life.nominal_arguments.len()),
                )?;
                for x in &life.slots {
                    b.spend(x.path.len())?;
                }
                for x in &life.retained {
                    b.spend(x.path.len())?;
                }
                for x in &life.nominal_arguments {
                    b.spend(x.path.len())?;
                }
                stack.push((storage, depth + 1));
            }
            Type::Record { name, fields } | Type::Package { name, fields } => {
                b.spend(name.len().saturating_add(fields.len()))?;
                for (n, t) in fields {
                    b.spend(n.len())?;
                    stack.push((t, depth + 1));
                }
            }
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                for t in fields {
                    stack.push((t, depth + 1));
                }
            }
            Type::Enum { name, variants } => {
                b.spend(name.len().saturating_add(variants.len()))?;
                for (n, fs) in variants {
                    b.spend(n.len().saturating_add(fs.len()))?;
                    for t in fs {
                        stack.push((t, depth + 1));
                    }
                }
            }
            Type::Address { inner, .. }
            | Type::Managed(inner)
            | Type::Desc(inner)
            | Type::Array { element: inner, .. }
            | Type::Slice { element: inner } => stack.push((inner, depth + 1)),
            Type::ErrorUnion { error, result } => {
                b.spend(2)?;
                stack.push((error, depth + 1));
                stack.push((result, depth + 1));
            }
            Type::BorrowedFunctionPointer {
                parameters,
                result,
                life,
            } => {
                crate::signature_life::charge(life, b)?;
                b.spend(parameters.len().saturating_add(1))?;
                stack.extend(parameters.iter().map(|ty| (ty, depth + 1)));
                stack.push((result, depth + 1));
            }
            Type::FunctionPointer { parameters, result } => {
                b.spend(parameters.len().saturating_add(1))?;
                for t in parameters {
                    stack.push((t, depth + 1));
                }
                stack.push((result, depth + 1));
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn physical(ty: &Type, limits: &Limits, b: &mut Budget) -> Check<bool> {
    let Some((storage, shape)) = qualified(ty) else {
        return Ok(false);
    };
    type_work(ty, limits, b)?;
    if !matches!(storage, Type::Record { .. } | Type::Tuple(_)) || !shape.retained.is_empty() {
        return Err(unsupported(
            "nested retained/nominal stored carrier needs its independent consumer",
            None,
        ));
    }
    b.spend(1)?;
    let mut todo = vec![(storage, Vec::new())];
    let mut actual = Vec::new();
    while let Some((t, path)) = todo.pop() {
        b.spend(1)?;
        match t {
            Type::Address {
                mutable: false,
                inner,
            } => {
                type_work(inner, limits, b)?;
                if !inner.is_address_pointee() {
                    return Err(err("stored shared leaf has invalid actual pointee"));
                }
                b.spend(path.len().saturating_add(1))?;
                actual.push(path);
            }
            Type::Record { name, fields } => {
                b.spend(name.len())?;
                if name.is_empty() {
                    return Err(err("stored record lacks actual nominal identity"));
                }
                let mut names: Vec<&str> = Vec::new();
                for (name, _) in fields {
                    b.spend(amount(
                        names.len().saturating_add(1),
                        name.len().saturating_add(1),
                    )?)?;
                    if name.is_empty() || names.contains(&name.as_str()) {
                        return Err(err("stored actual fields have duplicate/empty names"));
                    }
                    names.push(name.as_str());
                }
                for (i, (_, child)) in fields.iter().enumerate().rev() {
                    b.spend(path.len().saturating_add(1))?;
                    let mut p = path.clone();
                    p.push(PlaceProjection::Field(i as u32));
                    todo.push((child, p));
                }
            }
            Type::Tuple(fields) => {
                for (i, child) in fields.iter().enumerate().rev() {
                    b.spend(path.len().saturating_add(1))?;
                    let mut p = path.clone();
                    p.push(PlaceProjection::Field(i as u32));
                    todo.push((child, p));
                }
            }
            child => {
                type_work(child, limits, b)?;
                if !child.is_copy_value() {
                    return Err(unsupported(
                        "nonshared actual stored leaf needs its physical consumer",
                        None,
                    ));
                }
            }
        }
    }
    if actual.len() != shape.slots.len()
        || (actual.is_empty() && shape.nominal_arguments.is_empty())
    {
        return Err(err("stored physical leaf coverage is incomplete"));
    }
    let mut classes = Vec::new();
    let mut class = |id: u32, b: &mut Budget| -> Check<()> {
        b.spend(classes.len().saturating_add(1))?;
        if id >= shape.binder_count {
            return Err(err("stored class is outside declared domain"));
        }
        if !classes.contains(&id) {
            if id as usize != classes.len() {
                return Err(err(
                    "stored classes are not dense by first actual occurrence",
                ));
            }
            classes.push(id);
        }
        Ok(())
    };
    for row in &shape.nominal_arguments {
        b.spend(row.path.len().saturating_add(1))?;
        if !row.path.is_empty() {
            return Err(unsupported(
                "nested nominal class mapping is not yet implemented",
                None,
            ));
        }
        class(row.binder, b)?;
    }
    for (path, row) in actual.iter().zip(&shape.slots) {
        b.spend(path.len().saturating_add(row.path.len()).saturating_add(1))?;
        if path != &row.path || row.mode != StoredBorrowMode::SharedAddress {
            return Err(err(
                "actual physical stored path or mode differs from declared leaf",
            ));
        }
        class(row.binder, b)?;
    }
    if classes.len() != shape.binder_count as usize {
        return Err(err("stored domain has unused class labels"));
    }
    Ok(true)
}

pub(super) fn qualification(
    module: &TypedModule,
    ty: &Type,
    limits: &Limits,
    b: &mut Budget,
) -> Check<()> {
    type_work(ty, limits, b)?;
    type_work(ty, limits, b)?;
    physical(ty, limits, b)?;
    let (storage, _) =
        qualified(ty).ok_or_else(|| err("expected an actual qualified stored type"))?;
    if matches!(storage, Type::Record { .. }) {
        let mut matches = 0usize;
        b.spend(module.life_instances.len())?;
        for instance in &module.life_instances {
            type_work(&instance.ty, limits, b)?;
            type_work(ty, limits, b)?;
            if instance.ty == *ty {
                matches += 1;
            }
        }
        if matches != 1 {
            return Err(err(
                "nominal carrier lacks its exact complete declaration/instance correspondence",
            ));
        }
    }
    Ok(())
}

pub(super) fn qualifications(module: &TypedModule, limits: &Limits, b: &mut Budget) -> Check<()> {
    for f in &module.functions {
        for block in &f.blocks {
            for i in &block.instructions {
                b.spend(1)?;
                if !super::class_flow::pointer_instruction(f, i)
                    && !matches!(
                        &i.kind,
                        InstructionKind::Constant(_)
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
                            | InstructionKind::Call { .. }
                            | InstructionKind::Unary { .. }
                            | InstructionKind::Binary { .. }
                            | InstructionKind::ConvertInteger { .. }
                            | InstructionKind::ConvertNumber { .. }
                            | InstructionKind::ScopeEnter { .. }
                            | InstructionKind::ScopeExit { .. }
                            | InstructionKind::LoadStoredBorrow {
                                mode: StoredBorrowLoad::CopyShared,
                                ..
                            }
                    )
                {
                    return Err(unsupported(
                        "instruction is outside the checked stored profile, including unreachable blocks",
                        Some(i.span),
                    ));
                }
                if super::class_flow::pointer_instruction(f, i) {
                    super::class_flow::pointer_types(module, f, i, b)?;
                }
            }
        }
        for root in f
            .locals
            .iter()
            .map(|l| &l.ty)
            .chain(&f.values)
            .chain(std::iter::once(&f.return_type))
        {
            b.spend(1)?;
            let mut todo = vec![root];
            while let Some(t) = todo.pop() {
                b.spend(1)?;
                match t {
                    Type::BorrowedStorage { storage, .. } => {
                        qualification(module, t, limits, b)?;
                        todo.push(storage);
                    }
                    Type::Address { inner, .. }
                    | Type::Array { element: inner, .. }
                    | Type::Slice { element: inner } => todo.push(inner),
                    Type::Tuple(fs) => {
                        b.spend(fs.len())?;
                        todo.extend(fs);
                    }
                    Type::Record { fields, .. } => {
                        b.spend(fields.len())?;
                        todo.extend(fields.iter().map(|(_, t)| t));
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn identity_work(id: &LifeDeclarationIdentity, b: &mut Budget) -> Check<()> {
    b.spend(
        id.package
            .len()
            .saturating_add(id.module_path.len())
            .saturating_add(id.declaration_path.len())
            .saturating_add(id.generation_path.len())
            .saturating_add(1),
    )?;
    for n in id.module_path.iter().chain(&id.declaration_path) {
        b.spend(n.len())?;
    }
    Ok(())
}
pub(super) fn instruction_work(f: &TypedFunction, i: &Instruction, b: &mut Budget) -> Check<()> {
    let limits = b.limits.clone();
    for value in instruction_operands(&i.kind).chain(i.result) {
        type_work(vt(f, value)?, &limits, b)?;
    }
    Ok(())
}
pub(super) fn inventory(module: &TypedModule, limits: &Limits, b: &mut Budget) -> Check<()> {
    if module.life_declarations.len() > limits.max_nodes
        || module.life_instances.len() > limits.max_nodes
    {
        return Err(limit(
            "stored nominal inventory count exceeds verification limit",
        ));
    }
    b.spend(
        module
            .life_declarations
            .len()
            .saturating_add(module.life_instances.len()),
    )?;
    let mut declarations = Vec::<&LifeNominalDeclaration>::new();
    for d in &module.life_declarations {
        identity_work(&d.identity, b)?;
        if d.identity.package.is_empty()
            || d.identity.declaration_path.is_empty()
            || d.identity
                .module_path
                .iter()
                .chain(&d.identity.declaration_path)
                .any(String::is_empty)
        {
            return Err(err("declaration lacks full stable nominal identity"));
        }
        for old in &declarations {
            identity_work(&d.identity, b)?;
            identity_work(&old.identity, b)?;
            if old.id == d.id || old.identity == d.identity {
                return Err(err("duplicate nominal declaration reference or identity"));
            }
        }
        b.spend(d.parameters.len())?;
        for p in &d.parameters {
            if let LifeParameterKind::Const(t) = p {
                type_work(t, limits, b)?;
                if !t.is_copy_value() {
                    return Err(err("nominal const kind has invalid actual data type"));
                }
            }
        }
        b.spend(1)?;
        declarations.push(d);
    }
    for (i, instance) in module.life_instances.iter().enumerate() {
        type_work(&instance.ty, limits, b)?;
        physical(&instance.ty, limits, b)?;
        let (storage, shape) = qualified(&instance.ty)
            .ok_or_else(|| err("nominal instance has no complete qualified type"))?;
        b.spend(declarations.len())?;
        let d = declarations
            .iter()
            .find(|d| d.id == instance.declaration)
            .ok_or_else(|| err("nominal instance references absent declaration"))?;
        if !matches!(
            (d.kind, storage),
            (LifeNominalKind::Record, Type::Record { .. })
        ) {
            return Err(unsupported(
                "nominal kind has no actual physical stored consumer",
                None,
            ));
        }
        if d.parameters.len() != instance.arguments.len() {
            return Err(err("nominal actual list omits complete generic ordinal"));
        }
        let mut lifetime_rows = Vec::new();
        b.spend(instance.arguments.len())?;
        for (ordinal, (formal, actual)) in d.parameters.iter().zip(&instance.arguments).enumerate()
        {
            match (formal, actual) {
                (LifeParameterKind::Lifetime, LifeNominalActual::Lifetime(class))
                    if *class < shape.binder_count =>
                {
                    b.spend(1)?;
                    lifetime_rows.push((ordinal as u32, *class));
                }
                (LifeParameterKind::Type, LifeNominalActual::Type(ty)) => {
                    type_work(ty, limits, b)?;
                    if !ty.is_copy_value() {
                        return Err(unsupported(
                            "metadata-only borrowed Type actual needs qualified context checking",
                            None,
                        ));
                    }
                }
                (LifeParameterKind::Const(ty), LifeNominalActual::Const(c)) if c.ty() == *ty => {
                    b.spend(match c {
                        Constant::WideString(data) => data.len().saturating_add(1),
                        _ => 1,
                    })?;
                }
                _ => {
                    return Err(err(
                        "full nominal actual kind or local fixed class differs from declaration",
                    ));
                }
            }
        }
        if lifetime_rows.len() != shape.nominal_arguments.len() {
            return Err(err("nominal lifetime relation coverage is incomplete"));
        }
        for ((ordinal, class), row) in lifetime_rows.iter().zip(&shape.nominal_arguments) {
            b.spend(row.path.len().saturating_add(1))?;
            if !row.path.is_empty()
                || row.declaration != d.id
                || row.argument != *ordinal
                || row.binder != *class
            {
                return Err(err(
                    "nominal relation differs from actual declaration, ordinal or local class",
                ));
            }
        }
        for previous in &module.life_instances[..i] {
            type_work(&previous.ty, limits, b)?;
            type_work(&instance.ty, limits, b)?;
            if previous.ty == instance.ty {
                return Err(err("duplicate complete nominal instance"));
            }
        }
    }
    Ok(())
}

fn relation(
    f: &TypedFunction,
    base: StorageBase,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<StorageBinder> {
    let c = f
        .storage_life
        .as_ref()
        .ok_or_else(|| err("stored signature lacks complete relation"))?;
    if path.is_empty() {
        b.spend(c.bindings.len())?;
        return super::signature_life::outer(c, base)
            .ok_or_else(|| err("signature omits actual outer borrow"));
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
    .ok_or_else(|| err("stored relation base is absent"))?;
    let ((_, shape), pointee) = if let Some(q) = qualified(ty) {
        (q, false)
    } else {
        (
            addressed(ty).ok_or_else(|| err("physical leaf has no actual qualified boundary"))?,
            true,
        )
    };
    b.spend(shape.slots.len())?;
    let mut local = None;
    for slot in &shape.slots {
        b.spend(slot.path.len().saturating_add(path.len()).saturating_add(1))?;
        if slot.path == path && local.replace(slot.binder).is_some() {
            return Err(err("duplicate actual stored leaf"));
        }
    }
    let local = local.ok_or_else(|| err("relation selected an absent physical stored leaf"))?;
    b.spend(c.classes.len())?;
    for row in &c.classes {
        b.spend(row.position.path.len().saturating_add(1))?;
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
    Err(err("signature omits actual qualified local class"))
}

pub(super) fn header(f: &TypedFunction, b: &mut Budget) -> Check<()> {
    for p in &f.parameters {
        b.spend(1)?;
        if f.locals.get(p.0 as usize).is_none() {
            return Err(err("signature parameter outside actual local table"));
        }
    }
    let Some(c) = &f.storage_life else {
        for ty in f
            .parameters
            .iter()
            .map(|p| &f.locals[p.0 as usize].ty)
            .chain(std::iter::once(&f.return_type))
        {
            b.spend(1)?;
            if qualified(ty).is_some() || addressed(ty).is_some() {
                return Err(err("stored signature requires complete class relation"));
            }
        }
        return Ok(());
    };
    crate::signature_life::charge(c, b)?;
    b.spend(f.parameters.len())?;
    if f.reference_return_bound.is_some() || f.foreign.is_some() || f.environment.is_some() {
        return Err(err("stored signature contradicts another active profile"));
    }
    let mut parameters = Vec::with_capacity(f.parameters.len());
    for p in &f.parameters {
        let ty = &f.locals[p.0 as usize].ty;
        type_work(ty, &b.limits.clone(), b)?;
        type_work(ty, &b.limits.clone(), b)?;
        parameters.push(ty.clone());
    }
    type_work(&f.return_type, &b.limits.clone(), b)?;
    type_work(&f.return_type, &b.limits.clone(), b)?;
    super::signature_life::relation(&parameters, &f.return_type, c, Some(f.span))?;
    Ok(())
}

fn ancestry(ids: &[usize], loans: &[Loan], b: &mut Budget) -> Check<Vec<usize>> {
    b.spend(ids.len())?;
    let mut todo = ids.to_vec();
    let mut result = Vec::new();
    while let Some(id) = todo.pop() {
        b.spend(result.len().saturating_add(1))?;
        if result.contains(&id) {
            continue;
        }
        result.push(id);
        let row = loans
            .get(id)
            .ok_or_else(|| err("loan parent outside actual arena"))?;
        b.spend(row.parents.len())?;
        todo.extend(&row.parents);
    }
    b.spend(amount(result.len(), result.len())?)?;
    result.sort_unstable();
    Ok(result)
}
fn overlap(a: &Source, z: &Source, b: &mut Budget) -> Check<bool> {
    b.spend(width(a).saturating_add(width(z)))?;
    Ok(a.base == z.base
        && a.leaf == z.leaf
        && (a.inline.starts_with(&z.inline) || z.inline.starts_with(&a.inline)))
}
fn apart(
    a: &Source,
    z: &Source,
    requirements: &mut Vec<(Source, Source)>,
    b: &mut Budget,
) -> Check<()> {
    if overlap(a, z, b)? {
        return Err(err("actual target intersects a live incompatible loan"));
    }
    if matches!(
        (a.base, z.base),
        (HoldRoot::Parameter(_), HoldRoot::Parameter(_))
    ) && (a.base != z.base || a.leaf != z.leaf)
    {
        let size = width(a).saturating_add(width(z));
        b.spend(amount(requirements.len().saturating_add(1), size)?)?;
        let pair = if a < z { (a, z) } else { (z, a) };
        if !requirements.iter().any(|(a, z)| a == pair.0 && z == pair.1) {
            b.spend(size)?;
            requirements.push((pair.0.clone(), pair.1.clone()));
        }
    }
    Ok(())
}
fn access(
    p: &Permission,
    write: bool,
    active: &[usize],
    loans: &[Loan],
    requirements: &mut Vec<(Source, Source)>,
    b: &mut Budget,
) -> Check<()> {
    if p.sources.is_empty() {
        return Err(err(
            "access has no independently grounded target permission",
        ));
    }
    let delegated = ancestry(&p.loans, loans, b)?;
    for id in active {
        b.spend(delegated.len().saturating_add(1))?;
        if delegated.contains(id) {
            continue;
        }
        let loan = loans.get(*id).ok_or_else(|| err("live loan absent"))?;
        if write || loan.exclusive {
            for a in &p.sources {
                for z in &loan.sources {
                    apart(a, z, requirements, b)?;
                }
            }
        }
    }
    Ok(())
}
fn empty_frame(f: &TypedFunction, b: &mut Budget) -> Check<Frame> {
    b.spend(f.locals.len().saturating_add(f.parameters.len()))?;
    Ok(Frame {
        locals: vec![Payload::default(); f.locals.len()],
        external: vec![Payload::default(); f.parameters.len()],
    })
}
fn seed(f: &TypedFunction, loans: &mut Vec<Loan>, b: &mut Budget) -> Check<Frame> {
    let mut state = empty_frame(f, b)?;
    for (position, local) in f.parameters.iter().enumerate() {
        b.spend(1)?;
        let ty = &f.locals[local.0 as usize].ty;
        let mut p = Payload::default();
        if let Type::Address { mutable, .. } = ty {
            let source = Source {
                base: HoldRoot::Parameter(position as u32),
                leaf: Vec::new(),
                inline: Vec::new(),
            };
            b.spend(3)?;
            let id = loans.len();
            loans.push(Loan {
                sources: vec![source.clone()],
                parents: Vec::new(),
                exclusive: *mutable,
            });
            p.address = Permission {
                sources: vec![source],
                loans: vec![id],
            };
        }
        if let Some((_, shape)) = qualified(ty).or_else(|| addressed(ty)) {
            for slot in &shape.slots {
                b.spend(slot.path.len().saturating_mul(3).saturating_add(4))?;
                let source = Source {
                    base: HoldRoot::Parameter(position as u32),
                    leaf: slot.path.clone(),
                    inline: Vec::new(),
                };
                let id = loans.len();
                loans.push(Loan {
                    sources: vec![source.clone()],
                    parents: Vec::new(),
                    exclusive: false,
                });
                p.leaves.push((
                    slot.path.clone(),
                    Permission {
                        sources: vec![source],
                        loans: vec![id],
                    },
                ));
            }
            if addressed(ty).is_some() {
                state.external[position] = copy_payload(&p, b)?;
            }
        }
        state.locals[local.0 as usize] = p;
    }
    Ok(state)
}
fn select(
    state: &Frame,
    address: &Permission,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<Permission> {
    if address.sources.is_empty() {
        return Ok(Permission::default());
    }
    let mut result = Permission::default();
    for source in &address.sources {
        b.spend(width(source))?;
        if !source.leaf.is_empty() || !source.inline.is_empty() {
            return Err(err("stored leaf base crosses a target or inline boundary"));
        }
        let p = match source.base {
            HoldRoot::Local(l) => state.locals.get(l.0 as usize),
            HoldRoot::Parameter(p) => state.external.get(p as usize),
            _ => {
                return Err(unsupported(
                    "captured stored target CFG is not yet implemented",
                    None,
                ));
            }
        }
        .ok_or_else(|| err("carrier location is absent"))?;
        if let Some(p) = leaf(p, path, b)? {
            join_permission(&mut result, p, b)?;
        }
    }
    Ok(result)
}
fn instantiate(
    source: &Source,
    args: &[ValueId],
    values: &[Payload],
    state: &Frame,
    b: &mut Budget,
) -> Check<Permission> {
    let HoldRoot::Parameter(position) = source.base else {
        return Err(err("callee returned activation or capture storage"));
    };
    let v = args
        .get(position as usize)
        .and_then(|v| values.get(v.0 as usize))
        .ok_or_else(|| err("callee source has no actual argument"))?;
    let mut p = if source.leaf.is_empty() {
        permission_work(&v.address, b)?;
        v.address.clone()
    } else if let Some(p) = leaf(v, &source.leaf, b)? {
        permission_work(p, b)?;
        p.clone()
    } else {
        select(state, &v.address, &source.leaf, b)?
    };
    for target in &mut p.sources {
        b.spend(source.inline.len())?;
        target.inline.extend(&source.inline);
    }
    Ok(p)
}

#[allow(clippy::too_many_arguments)]
fn step(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &mut Frame,
    values: &[Payload],
    summaries: &[Summary],
    loans: &mut [Loan],
    b: &mut Budget,
) -> Check<Payload> {
    b.spend(1)?;
    let mut out = Payload::default();
    let val = |v: ValueId| {
        values
            .get(v.0 as usize)
            .ok_or_else(|| err("operand outside actual value table"))
    };
    match &i.kind {
        InstructionKind::Load(l) => {
            out = copy_payload(
                state
                    .locals
                    .get(l.0 as usize)
                    .ok_or_else(|| err("local outside table"))?,
                b,
            )?
        }
        InstructionKind::Store { local, value } => {
            state.locals[local.0 as usize] = copy_payload(val(*value)?, b)?
        }
        InstructionKind::StorageLive(l) | InstructionKind::StorageDead(l) => {
            state.locals[l.0 as usize] = Payload::default()
        }
        InstructionKind::BorrowLocal { local } | InstructionKind::BorrowLocalMut { local } => {
            b.spend(1)?;
            out.address.sources.push(Source {
                base: HoldRoot::Local(*local),
                leaf: Vec::new(),
                inline: Vec::new(),
            });
            if qualified(&f.locals[local.0 as usize].ty).is_some() {
                for (path, p) in &state.locals[local.0 as usize].leaves {
                    add_leaf(&mut out, path, p, b)?;
                }
            }
        }
        InstructionKind::AddressField { address, index } => {
            join_permission(&mut out.address, &val(*address)?.address, b)?;
            for target in &mut out.address.sources {
                b.spend(1)?;
                target.inline.push(PlaceProjection::Field(*index));
            }
        }
        InstructionKind::ReborrowAddress { address, .. }
        | InstructionKind::AddressIndex { address, .. }
        | InstructionKind::AddressSlice { address, .. } => {
            join_permission(&mut out.address, &val(*address)?.address, b)?;
        }
        InstructionKind::MakeAggregate { fields } => {
            if let Some((_, shape)) = qualified(vt(
                f,
                i.result.ok_or_else(|| err("aggregate lacks result"))?,
            )?) {
                for row in &shape.slots {
                    let Some((PlaceProjection::Field(index), tail)) = row.path.split_first() else {
                        return Err(err("aggregate stored leaf has no inline field"));
                    };
                    let value = val(*fields
                        .get(*index as usize)
                        .ok_or_else(|| err("aggregate omits actual borrowed field"))?)?;
                    if tail.is_empty() {
                        add_leaf(&mut out, &row.path, &value.address, b)?;
                    } else if let Some(p) = leaf(value, tail, b)? {
                        add_leaf(&mut out, &row.path, p, b)?;
                    }
                }
            }
        }
        InstructionKind::LoadStoredBorrow {
            storage,
            path,
            mode: StoredBorrowLoad::CopyShared,
        } => out.address = select(state, &val(*storage)?.address, path, b)?,
        InstructionKind::LoadStoredBorrow { .. } | InstructionKind::StoreStoredBorrow { .. } => {
            return Err(unsupported(
                "stored reborrow/take/replacement CFG is not yet implemented",
                Some(i.span),
            ));
        }
        InstructionKind::Call {
            function,
            arguments,
        } => {
            let callee = module
                .functions
                .get(function.0 as usize)
                .ok_or_else(|| err("call target absent"))?;
            if callee.foreign.is_some()
                || callee.environment.is_some()
                || callee.call_kind != CallKind::Direct
            {
                return Err(err(
                    "stored call requires an independently checked ordinary direct body",
                ));
            }
            for (path, sources) in &summaries[function.0 as usize].result {
                let mut p = Permission::default();
                for source in sources {
                    join_permission(
                        &mut p,
                        &instantiate(source, arguments, values, state, b)?,
                        b,
                    )?;
                }
                if path.is_empty() {
                    out.address = p;
                } else {
                    add_leaf(&mut out, path, &p, b)?;
                }
            }
        }
        InstructionKind::FunctionRef { .. } | InstructionKind::CallIndirect { .. }
            if super::class_flow::pointer_instruction(f, i) => {}
        InstructionKind::WriteAddress { address, .. } => {
            let Type::Address { inner, .. } = vt(f, *address)? else {
                return Err(err("write base lacks an actual Address type"));
            };
            resources::charge_payload(inner, b)?;
            if !inner.is_copy_value() {
                return Err(err(
                    "ordinary write cannot replace qualified carriers or borrowed leaves outside the actual slot protocol",
                ));
            }
        }
        InstructionKind::Constant(_)
        | InstructionKind::ConvertInteger { .. }
        | InstructionKind::ConvertNumber { .. }
        | InstructionKind::Unary { .. }
        | InstructionKind::Binary { .. }
        | InstructionKind::ReadAddress { .. }
        | InstructionKind::SliceLength { .. }
        | InstructionKind::ScopeEnter { .. }
        | InstructionKind::ScopeExit { .. } => {}
        _ => {
            return Err(unsupported(
                "instruction lacks an independent stored target transfer",
                Some(i.span),
            ));
        }
    }
    if let Some(id) = i.result
        && matches!(
            i.kind,
            InstructionKind::BorrowLocal { .. }
                | InstructionKind::BorrowLocalMut { .. }
                | InstructionKind::ReborrowAddress { .. }
                | InstructionKind::AddressField { .. }
                | InstructionKind::AddressIndex { .. }
                | InstructionKind::AddressSlice { .. }
                | InstructionKind::Call { .. }
        )
        && matches!(vt(f, id)?, Type::Address { .. })
    {
        let row = &mut loans[id.0 as usize];
        add_sources(&mut row.sources, &out.address.sources, b)?;
        add_loans(&mut row.parents, &out.address.loans, b)?;
        row.exclusive = matches!(vt(f, id)?, Type::Address { mutable: true, .. });
        b.spend(1)?;
        out.address.loans = vec![id.0 as usize];
    }
    if let Some(id) = i.result
        && matches!(i.kind, InstructionKind::Load(_))
        && matches!(vt(f, id)?, Type::Address { mutable: true, .. })
    {
        let row = &mut loans[id.0 as usize];
        add_sources(&mut row.sources, &out.address.sources, b)?;
        add_loans(&mut row.parents, &out.address.loans, b)?;
        row.exclusive = true;
        b.spend(1)?;
        out.address.loans = vec![id.0 as usize];
    }
    if let Some(id) = i.result
        && matches!(i.kind, InstructionKind::Call { .. })
        && qualified(vt(f, id)?).is_some()
    {
        let mut loan_id = f.values.len();
        b.spend(id.0 as usize)?;
        for ty in &f.values[..id.0 as usize] {
            if let Some((_, shape)) = qualified(ty) {
                loan_id = loan_id
                    .checked_add(shape.slots.len())
                    .ok_or_else(|| limit("stored result loan count overflow"))?;
            }
        }
        let (_, shape) = qualified(vt(f, id)?).ok_or_else(|| err("stored result has no shape"))?;
        for declaration in &shape.slots {
            b.spend(amount(
                out.leaves.len().saturating_add(1),
                declaration.path.len().saturating_add(1),
            )?)?;
            if let Some((_, p)) = out
                .leaves
                .iter_mut()
                .find(|(path, _)| path == &declaration.path)
            {
                let row = loans
                    .get_mut(loan_id)
                    .ok_or_else(|| err("stored result loan outside actual arena"))?;
                add_sources(&mut row.sources, &p.sources, b)?;
                add_loans(&mut row.parents, &p.loans, b)?;
                row.exclusive = false;
                b.spend(1)?;
                p.loans = vec![loan_id];
            }
            loan_id = loan_id
                .checked_add(1)
                .ok_or_else(|| limit("stored result loan identity overflow"))?;
        }
    }
    Ok(out)
}

fn solve(
    module: &TypedModule,
    f: &TypedFunction,
    g: &FunctionGraph,
    summaries: &[Summary],
    b: &mut Budget,
) -> Check<Solution> {
    b.spend(
        f.values
            .len()
            .saturating_mul(5)
            .saturating_add(f.blocks.len()),
    )?;
    let mut count = f.values.len();
    b.spend(f.values.len())?;
    for ty in &f.values {
        if let Some((_, shape)) = qualified(ty) {
            count = count
                .checked_add(shape.slots.len())
                .ok_or_else(|| limit("stored loan arena overflow"))?;
        }
    }
    b.spend(amount(count, 4)?)?;
    let mut loans = vec![Loan::default(); count];
    let entry = seed(f, &mut loans, b)?;
    let mut values = vec![Payload::default(); f.values.len()];
    let mut outputs = Vec::new();
    for _ in &f.blocks {
        outputs.push(empty_frame(f, b)?);
    }
    loop {
        let mut changed = false;
        // Reverse batches differ from the candidate's forward in-place walk.
        let mut replacement = Vec::new();
        for block in &outputs {
            replacement.push(copy_frame(block, b)?);
        }
        for bi in (0..f.blocks.len()).rev() {
            b.spend(1)?;
            if !g.reachable[bi] {
                continue;
            }
            let mut state = if bi == f.entry.0 as usize {
                copy_frame(&entry, b)?
            } else {
                empty_frame(f, b)?
            };
            for pred in &g.predecessors[bi] {
                if g.reachable[*pred] {
                    join_frame(&mut state, &outputs[*pred], b)?;
                }
            }
            for i in &f.blocks[bi].instructions {
                let out = step(module, f, i, &mut state, &values, summaries, &mut loans, b)?;
                if let Some(v) = i.result {
                    changed |= join_payload(&mut values[v.0 as usize], &out, b)?;
                }
            }
            frame_work(&state, b)?;
            frame_work(&outputs[bi], b)?;
            if state != outputs[bi] {
                replacement[bi] = state;
                changed = true;
            }
        }
        outputs = replacement;
        if !changed {
            break;
        }
    }
    Ok(Solution {
        values,
        outputs,
        entry,
        loans,
    })
}

fn reverse(f: &TypedFunction, i: &Instruction, live: &mut [bool], b: &mut Budget) -> Check<()> {
    b.spend(1)?;
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
    for v in instruction_operands(&i.kind) {
        b.spend(1)?;
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
fn live_out(f: &TypedFunction, g: &FunctionGraph, b: &mut Budget) -> Check<Vec<Vec<bool>>> {
    let width = f
        .values
        .len()
        .checked_add(f.locals.len())
        .ok_or_else(|| limit("stored liveness width overflow"))?;
    b.spend(amount(amount(f.blocks.len(), width)?, 2)?)?;
    let mut inputs = vec![vec![false; width]; f.blocks.len()];
    let mut outputs = inputs.clone();
    let mut dirty: Vec<_> = (0..f.blocks.len()).filter(|i| g.reachable[*i]).collect();
    b.spend(f.blocks.len())?;
    while let Some(bi) = dirty.pop() {
        b.spend(width)?;
        let mut out = vec![false; width];
        for child in &g.successors[bi] {
            b.spend(width)?;
            for (a, z) in out.iter_mut().zip(&inputs[*child]) {
                *a |= *z;
            }
        }
        b.spend(width)?;
        outputs[bi].clone_from(&out);
        term_live(&f.blocks[bi].terminator, &mut out);
        for i in f.blocks[bi].instructions.iter().rev() {
            reverse(f, i, &mut out, b)?;
        }
        b.spend(width)?;
        if inputs[bi] != out {
            inputs[bi] = out;
            for pred in &g.predecessors[bi] {
                b.spend(dirty.len().saturating_add(1))?;
                if !dirty.contains(pred) {
                    dirty.push(*pred);
                }
            }
        }
    }
    Ok(outputs)
}
fn live_loans(
    solution: &Solution,
    state: &Frame,
    live: &[bool],
    b: &mut Budget,
) -> Check<Vec<usize>> {
    b.spend(live.len())?;
    let mut loans = Vec::new();
    for (id, yes) in live.iter().enumerate() {
        if !*yes {
            continue;
        }
        let payload = if id < solution.values.len() {
            &solution.values[id]
        } else {
            &state.locals[id - solution.values.len()]
        };
        add_loans(&mut loans, &payload.address.loans, b)?;
        for (_, p) in &payload.leaves {
            add_loans(&mut loans, &p.loans, b)?;
        }
    }
    ancestry(&loans, &solution.loans, b)
}
#[allow(clippy::too_many_arguments)]
fn check_call(
    module: &TypedModule,
    f: &TypedFunction,
    function: FunctionId,
    arguments: &[ValueId],
    solution: &Solution,
    state: &Frame,
    active: &[usize],
    summaries: &[Summary],
    requirements: &mut Vec<(Source, Source)>,
    b: &mut Budget,
) -> Check<()> {
    let callee = module
        .functions
        .get(function.0 as usize)
        .ok_or_else(|| err("call target absent"))?;
    if arguments.len() != callee.parameters.len() {
        return Err(err("call arity differs from actual body signature"));
    }
    for (position, argument) in arguments.iter().enumerate() {
        let value = &solution.values[argument.0 as usize];
        let ty = &callee.locals[callee.parameters[position].0 as usize].ty;
        if let Type::Address { mutable, .. } = ty {
            access(
                &value.address,
                *mutable,
                active,
                &solution.loans,
                requirements,
                b,
            )?;
        }
        for (_, p) in &value.leaves {
            access(p, false, active, &solution.loans, requirements, b)?;
        }
        for earlier in &arguments[..position] {
            let right = &solution.values[earlier.0 as usize];
            let left_mut = matches!(vt(f, *argument)?, Type::Address { mutable: true, .. });
            let right_mut = matches!(vt(f, *earlier)?, Type::Address { mutable: true, .. });
            if left_mut || right_mut {
                for a in &value.address.sources {
                    for z in &right.address.sources {
                        apart(a, z, requirements, b)?;
                    }
                }
            }
            if left_mut {
                for (_, p) in &right.leaves {
                    for a in &value.address.sources {
                        for z in &p.sources {
                            apart(a, z, requirements, b)?;
                        }
                    }
                }
            }
            if right_mut {
                for (_, p) in &value.leaves {
                    for a in &p.sources {
                        for z in &right.address.sources {
                            apart(a, z, requirements, b)?;
                        }
                    }
                }
            }
        }
    }
    for (a, z) in &summaries[function.0 as usize].apart {
        let a = instantiate(a, arguments, &solution.values, state, b)?;
        let z = instantiate(z, arguments, &solution.values, state, b)?;
        if a.sources.is_empty() || z.sources.is_empty() {
            return Err(err("call separation has incomplete actual target facts"));
        }
        for a in &a.sources {
            for z in &z.sources {
                apart(a, z, requirements, b)?;
            }
        }
    }
    Ok(())
}

fn windows(
    module: &TypedModule,
    f: &TypedFunction,
    g: &FunctionGraph,
    solution: &mut Solution,
    summaries: &[Summary],
    requirements: &mut Vec<(Source, Source)>,
    b: &mut Budget,
) -> Check<()> {
    let output_live = live_out(f, g, b)?;
    for (bi, block) in f.blocks.iter().enumerate() {
        b.spend(1)?;
        if !g.reachable[bi] {
            continue;
        }
        let mut state = if bi == f.entry.0 as usize {
            copy_frame(&solution.entry, b)?
        } else {
            empty_frame(f, b)?
        };
        for pred in &g.predecessors[bi] {
            if g.reachable[*pred] {
                join_frame(&mut state, &solution.outputs[*pred], b)?;
            }
        }
        let width = f.values.len().saturating_add(f.locals.len());
        b.spend(amount(block.instructions.len().saturating_add(1), width)?)?;
        let mut live = output_live[bi].clone();
        term_live(&block.terminator, &mut live);
        let mut after = vec![vec![false; width]; block.instructions.len()];
        for (at, i) in block.instructions.iter().enumerate().rev() {
            after[at].clone_from(&live);
            reverse(f, i, &mut live, b)?;
        }
        for (at, i) in block.instructions.iter().enumerate() {
            b.spend(width)?;
            let mut live = after[at].clone();
            if let Some(id) = i.result {
                live[id.0 as usize] = false;
            }
            let mut active = live_loans(solution, &state, &live, b)?;
            if let Some(id) = i.result {
                b.spend(active.len())?;
                active.retain(|x| *x != id.0 as usize);
            }
            let pointer = |v: ValueId| -> Check<&Permission> {
                solution
                    .values
                    .get(v.0 as usize)
                    .map(|v| &v.address)
                    .ok_or_else(|| err("address operand absent"))
            };
            match &i.kind {
                InstructionKind::Load(local)
                    if matches!(
                        f.locals[local.0 as usize].ty,
                        Type::Address { mutable: true, .. }
                    ) =>
                {
                    access(
                        &state.locals[local.0 as usize].address,
                        true,
                        &active,
                        &solution.loans,
                        requirements,
                        b,
                    )?;
                }
                InstructionKind::Store { local, .. }
                | InstructionKind::StorageDead(local)
                | InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local } => {
                    b.spend(1)?;
                    let source = Source {
                        base: HoldRoot::Local(*local),
                        leaf: Vec::new(),
                        inline: Vec::new(),
                    };
                    let actual = Permission {
                        sources: vec![source],
                        loans: Vec::new(),
                    };
                    access(
                        &actual,
                        !matches!(i.kind, InstructionKind::BorrowLocal { .. }),
                        &active,
                        &solution.loans,
                        requirements,
                        b,
                    )?;
                }
                InstructionKind::ReadAddress { address }
                | InstructionKind::SliceLength { address }
                | InstructionKind::ReborrowAddress {
                    address,
                    mutable: false,
                } => access(
                    pointer(*address)?,
                    false,
                    &active,
                    &solution.loans,
                    requirements,
                    b,
                )?,
                InstructionKind::AddressField { address, .. }
                | InstructionKind::AddressIndex { address, .. }
                | InstructionKind::AddressSlice { address, .. } => {
                    let write = i.result.is_some_and(|v| {
                        matches!(f.values[v.0 as usize], Type::Address { mutable: true, .. })
                    });
                    access(
                        pointer(*address)?,
                        write,
                        &active,
                        &solution.loans,
                        requirements,
                        b,
                    )?;
                }
                InstructionKind::WriteAddress { address, .. }
                | InstructionKind::ReborrowAddress {
                    address,
                    mutable: true,
                } => access(
                    pointer(*address)?,
                    true,
                    &active,
                    &solution.loans,
                    requirements,
                    b,
                )?,
                InstructionKind::LoadStoredBorrow { storage, path, .. } => {
                    access(
                        pointer(*storage)?,
                        false,
                        &active,
                        &solution.loans,
                        requirements,
                        b,
                    )?;
                    let p = select(&state, pointer(*storage)?, path, b)?;
                    access(&p, false, &active, &solution.loans, requirements, b)?;
                }
                InstructionKind::CallIndirect { arguments, .. } => {
                    let mut all: Vec<(Permission, bool)> = Vec::new();
                    for argument in arguments {
                        let actual = &solution.values[argument.0 as usize];
                        if let Type::Address { mutable, .. } = vt(f, *argument)? {
                            permission_work(&actual.address, b)?;
                            b.spend(1)?;
                            all.push((actual.address.clone(), *mutable));
                        }
                        for (_, p) in &actual.leaves {
                            permission_work(p, b)?;
                            b.spend(1)?;
                            all.push((p.clone(), false));
                        }
                        if let Some((_, shape)) = addressed(vt(f, *argument)?) {
                            for slot in &shape.slots {
                                let p = select(&state, &actual.address, &slot.path, b)?;
                                b.spend(1)?;
                                all.push((p, false));
                            }
                        }
                    }
                    for (index, (p, write)) in all.iter().enumerate() {
                        access(p, *write, &active, &solution.loans, requirements, b)?;
                        for (other, other_write) in &all[..index] {
                            if *write || *other_write {
                                for a in &p.sources {
                                    for z in &other.sources {
                                        apart(a, z, requirements, b)?;
                                    }
                                }
                            }
                        }
                    }
                }
                InstructionKind::Call {
                    function,
                    arguments,
                } => check_call(
                    module,
                    f,
                    *function,
                    arguments,
                    solution,
                    &state,
                    &active,
                    summaries,
                    requirements,
                    b,
                )?,
                _ => {}
            }
            step(
                module,
                f,
                i,
                &mut state,
                &solution.values,
                summaries,
                &mut solution.loans,
                b,
            )?;
        }
    }
    Ok(())
}
fn summary_work(s: &Summary, b: &mut Budget) -> Check<()> {
    b.spend(s.result.len().saturating_add(s.apart.len()))?;
    for (path, row) in &s.result {
        b.spend(path.len())?;
        source_work(row, b)?;
    }
    for (a, z) in &s.apart {
        b.spend(width(a).saturating_add(width(z)))?;
    }
    Ok(())
}
fn result_row(
    summary: &mut Summary,
    path: &[PlaceProjection],
    sources: &[Source],
    b: &mut Budget,
) -> Check<()> {
    b.spend(amount(
        summary.result.len().saturating_add(1),
        path.len().saturating_add(1),
    )?)?;
    if let Some((_, row)) = summary.result.iter_mut().find(|(p, _)| p == path) {
        add_sources(row, sources, b)?;
    } else {
        b.spend(path.len())?;
        source_work(sources, b)?;
        summary.result.push((path.to_vec(), sources.to_vec()));
    }
    Ok(())
}
fn summarize(
    f: &TypedFunction,
    g: &FunctionGraph,
    solution: &Solution,
    b: &mut Budget,
) -> Check<Summary> {
    let mut summary = Summary {
        complete: true,
        ..Summary::default()
    };
    for (ty, p) in f.values.iter().zip(&solution.values) {
        b.spend(1)?;
        if matches!(ty, Type::Address { .. }) && p.address.sources.is_empty() {
            summary.complete = false;
        }
        if let Some((_, shape)) = qualified(ty) {
            for row in &shape.slots {
                if leaf(p, &row.path, b)?.is_none_or(|p| p.sources.is_empty()) {
                    summary.complete = false;
                }
            }
        }
    }
    for (block, reachable) in f.blocks.iter().zip(&g.reachable) {
        b.spend(1)?;
        if *reachable && let Terminator::Return(Some(id)) = block.terminator {
            let payload = &solution.values[id.0 as usize];
            if matches!(f.return_type, Type::Address { .. }) {
                result_row(&mut summary, &[], &payload.address.sources, b)?;
            }
            for (path, p) in &payload.leaves {
                result_row(&mut summary, path, &p.sources, b)?;
            }
        }
    }
    for loan in &solution.loans {
        let _ = ancestry(&loan.parents, &solution.loans, b)?;
    }
    for (id, loan) in solution.loans.iter().enumerate() {
        let parents = ancestry(&loan.parents, &solution.loans, b)?;
        b.spend(parents.len())?;
        if parents.contains(&id) {
            return Err(err("cyclic actual loan ancestry"));
        }
    }
    Ok(summary)
}

/// Every output and every call requirement is reconstructed from current raw
/// executable data. The candidate is compared only after the fixed point.
pub(super) fn check(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    plan: &CandidatePlan,
    b: &mut Budget,
) -> Check<()> {
    if !module.resources.is_empty() || !module.closures.is_empty() {
        return Err(unsupported(
            "owned/closure storage needs its common retained integration",
            None,
        ));
    }
    for f in &module.functions {
        super::class_flow::check(module, f, &graphs[f.id.0 as usize].reachable, b)?;
        if f.foreign.is_some()
            || f.environment.is_some()
            || f.callable_contract.is_some()
            || f.callable_return_contract.is_some()
            || f.call_kind != CallKind::Direct
        {
            return Err(unsupported(
                "stored profile requires ordinary direct function bodies",
                Some(f.span),
            ));
        }
        for ty in f
            .locals
            .iter()
            .map(|l| &l.ty)
            .chain(&f.values)
            .chain(std::iter::once(&f.return_type))
        {
            type_work(ty, &b.limits.clone(), b)?;
            if !(qualified(ty).is_some()
                || addressed(ty).is_some()
                || matches!(ty,Type::Address{inner,..}if inner.is_address_pointee())
                || ty.is_copy_value())
            {
                return Err(unsupported(
                    "owned or opaque executable value lacks actual stored permission integration",
                    Some(f.span),
                ));
            }
        }
    }
    b.spend(module.functions.len().saturating_mul(2))?;
    let mut summaries = vec![Summary::default(); module.functions.len()];
    let mut solved: Vec<Option<Solution>> = (0..module.functions.len()).map(|_| None).collect();
    loop {
        let mut changed = false;
        let mut pending = Vec::new();
        b.spend(module.functions.len())?;
        for index in (0..module.functions.len()).rev() {
            let f = &module.functions[index];
            let mut solution = solve(module, f, &graphs[index], &summaries, b)?;
            let mut summary = summarize(f, &graphs[index], &solution, b)?;
            if summary.complete {
                windows(
                    module,
                    f,
                    &graphs[index],
                    &mut solution,
                    &summaries,
                    &mut summary.apart,
                    b,
                )?;
            }
            summary_work(&summary, b)?;
            summary_work(&summaries[index], b)?;
            if summary != summaries[index] {
                changed = true;
            }
            pending.push((index, summary));
            solved[index] = Some(solution);
        }
        for (index, s) in pending {
            summaries[index] = s;
        }
        if !changed {
            break;
        }
    }
    for (index, (f, s)) in module.functions.iter().zip(&summaries).enumerate() {
        if !s.complete {
            return Err(err(
                "stored/reference operation has no complete actual target permission",
            ));
        }
        for (path, sources) in &s.result {
            if sources.is_empty() {
                return Err(err("stored/reference result has no actual input source"));
            }
            for source in sources {
                let HoldRoot::Parameter(position) = source.base else {
                    return Err(err(
                        "stored/reference return retains activation or capture storage",
                    ));
                };
                if let Some(bound) = &f.reference_return_bound {
                    b.spend(
                        bound
                            .len()
                            .saturating_add(source.leaf.len())
                            .saturating_add(1),
                    )?;
                    if !source.leaf.is_empty() || bound.binary_search(&position).is_err() {
                        return Err(err(
                            "actual returned target exceeds the existing direct reference upper",
                        ));
                    }
                }
                if f.storage_life.is_some()
                    && relation(f, StorageBase::Result, path, b)?
                        != relation(f, StorageBase::Parameter(position), &source.leaf, b)?
                {
                    return Err(err(
                        "actual returned target exceeds the complete stored relation",
                    ));
                }
            }
        }
        let solution = solved[index]
            .as_ref()
            .ok_or_else(|| err("independent solver omitted a function"))?;
        let claimed = &plan.functions[index].holds;
        if claimed.len() != solution.values.len() {
            return Err(err("candidate must cover each actual value once"));
        }
        for (v, (p, claim)) in solution.values.iter().zip(claimed).enumerate() {
            let mut actual = BTreeSet::new();
            for root in p
                .address
                .sources
                .iter()
                .chain(p.leaves.iter().flat_map(|(_, p)| &p.sources))
            {
                b.spend(actual.len().saturating_add(1))?;
                actual.insert(root.base);
            }
            b.spend(actual.len().saturating_add(claim.roots.len()))?;
            if claim.value != ValueId(v as u32) || !actual.iter().eq(claim.roots.iter()) {
                return Err(err(
                    "candidate differs from independently reconstructed stored target roots",
                ));
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
            "/../../tests/language-readiness/terminal/342/stored_flow_fixture.rs"
        ));
    }
    #[test]
    fn unbound_pointer_fixed_is_rejected_in_raw_and_decoded_wire() {
        let m = fixture::phantom_class_calls(true, false);
        let c = fixture::class_candidate(&m);
        let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
        super::super::verify_module(m.clone(), c, &m.target, &Limits::default()).unwrap();
        super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()).unwrap();
        let m = fixture::unbound_fixed_pointer();
        let c = fixture::class_candidate(&m);
        let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
        let error =
            super::super::verify_module(m.clone(), c, &m.target, &Limits::default()).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.contains("genuine enclosing semantic context")),
            "{error:?}"
        );
        let error =
            super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.contains("genuine enclosing semantic context")),
            "{error:?}"
        );
    }
    #[test]
    fn rich_indirect_alias_is_rejected_in_raw_and_wire_even_with_matching_classes() {
        let m = fixture::rich_marker_exclusive_call(true);
        let c = fixture::class_candidate(&m);
        let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
        let error =
            super::super::verify_module(m.clone(), c, &m.target, &Limits::default()).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.contains("live incompatible loan")),
            "{error:?}"
        );
        let error =
            super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.contains("live incompatible loan")),
            "{error:?}"
        );
    }
    #[test]
    fn rich_class_equations_use_the_same_verification_allowance() {
        let m = fixture::rich_marker_pointer(false, false);
        let c = fixture::class_candidate(&m);
        let limit = |n| Limits {
            max_analysis_steps: n,
            ..Limits::default()
        };
        let check = |n| super::super::verify_module(m.clone(), c.clone(), &m.target, &limit(n));
        let expected = check(Limits::default().max_analysis_steps)
            .unwrap()
            .fingerprint();
        let mut low = 0;
        let mut high = Limits::default().max_analysis_steps;
        while low < high {
            let mid = low + (high - low) / 2;
            if check(mid).is_ok() {
                high = mid
            } else {
                low = mid + 1
            }
        }
        let error = check(low - 1).unwrap_err();
        assert!(
            error.iter().any(|d| d.code == "E-RESOURCE-LIMIT"),
            "{error:?}"
        );
        assert_eq!(check(low).unwrap().fingerprint(), expected);
        assert_eq!(check(low).unwrap().fingerprint(), expected);
        println!("342 rich class Verify N={low}; N-1 ResourceLimit/fresh N identical");
    }
    #[test]
    fn raw_and_wire_rich_pointer_and_persistent_phantom_class_equations() {
        let good = [
            fixture::rich_marker_pointer(false, false),
            fixture::rich_marker_exclusive_call(false),
            fixture::phantom_class_calls(false, false),
            fixture::phantom_class_calls(true, false),
        ];
        for m in good {
            let c = fixture::class_candidate(&m);
            let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
            super::super::verify_module(m.clone(), c, &m.target, &Limits::default()).unwrap();
            super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()).unwrap();
        }
        let bad = [
            fixture::rich_marker_pointer(true, false),
            fixture::rich_marker_pointer(false, true),
            fixture::phantom_class_calls(false, true),
            fixture::phantom_class_calls(true, true),
        ];
        for m in bad {
            let c = fixture::class_candidate(&m);
            let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
            let error = super::super::verify_module(m.clone(), c, &m.target, &Limits::default())
                .unwrap_err();
            assert!(
                error.iter().any(|d| d.message.contains("class equality")
                    || d.message.contains("entail actual target")),
                "{error:?}"
            );
            let error = super::super::verify_module_bytes(&bytes, &m.target, &Limits::default())
                .unwrap_err();
            assert!(
                error.iter().any(|d| d.message.contains("class equality")
                    || d.message.contains("entail actual target")),
                "{error:?}"
            );
        }
    }
    fn prove(module: &TypedModule) -> Check<()> {
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        inventory(module, &limits, &mut budget)?;
        for f in &module.functions {
            header(f, &mut budget)?;
        }
        let graphs: Vec<_> = module
            .functions
            .iter()
            .map(|_| FunctionGraph {
                successors: vec![vec![]],
                predecessors: vec![vec![]],
                reachable: vec![true],
                package_views: packages::CaptureViews::default(),
                role_data_uses: vec![],
            })
            .collect();
        // Deliberately supplied raw claims, checked against independent roots.
        let mut origins: Vec<Vec<Vec<HoldRoot>>> = vec![
            vec![vec![HoldRoot::Parameter(0)]; 3],
            vec![vec![HoldRoot::Parameter(0)]; 2],
            vec![vec![HoldRoot::Parameter(0)]; 2],
            vec![
                vec![],
                vec![HoldRoot::Local(LocalId(0))],
                vec![HoldRoot::Local(LocalId(0))],
                vec![HoldRoot::Local(LocalId(0)), HoldRoot::Local(LocalId(1))],
                vec![HoldRoot::Local(LocalId(0))],
                vec![],
            ],
            vec![vec![HoldRoot::Parameter(0)]],
        ];
        if module.functions.len() == 6 {
            origins.push(vec![vec![HoldRoot::Parameter(0)]]);
            origins[3].extend([
                vec![HoldRoot::Local(LocalId(0))],
                vec![HoldRoot::Local(LocalId(0))],
                vec![HoldRoot::Local(LocalId(0))],
                vec![],
                vec![],
            ]);
        }
        let mut candidate = CandidatePlan::empty_for(module);
        candidate.functions = module
            .functions
            .iter()
            .zip(origins)
            .map(|(f, origins)| FunctionPlan {
                function: f.id,
                blocks: vec![],
                holds: origins
                    .into_iter()
                    .enumerate()
                    .map(|(i, roots)| ValueHold {
                        value: ValueId(i as u32),
                        roots,
                    })
                    .collect(),
            })
            .collect();
        check(module, &graphs, &candidate, &mut budget)?;
        for (index, function) in candidate.functions.iter_mut().enumerate() {
            function.blocks = vec![BlockPlan {
                block: BlockId(0),
                initialized_in: if index == 3 { vec![] } else { vec![LocalId(0)] },
                initialized_out: vec![LocalId(0)],
            }];
        }
        verify_module(module.clone(), candidate.clone(), &module.target, &limits)
            .map_err(|d| d.into_iter().next().unwrap())?;
        let bytes = zil_ir::wire::encode_module_candidate(module, &candidate, &limits)
            .map_err(|d| d.into_iter().next().unwrap())?;
        verify_module_bytes(&bytes, &module.target, &limits)
            .map_err(|d| d.into_iter().next().unwrap())?;
        Ok(())
    }
    #[test]
    fn actual_shared_field_survives_carrier_end_and_two_forwarding_helpers() {
        prove(&fixture::module()).unwrap();
    }
    #[test]
    fn copied_field_keeps_target_parent_loan_after_carrier_end() {
        let mut m = fixture::module();
        let f = &mut m.functions[3];
        f.values.push(fixture::address(Type::I64, true));
        f.blocks[0].instructions.insert(
            8,
            fixture::instruction(
                Some(6),
                InstructionKind::BorrowLocalMut { local: LocalId(0) },
            ),
        );
        assert!(prove(&m).is_err());
    }
    #[test]
    fn unresolved_recursive_result_is_not_an_empty_authority() {
        let mut m = fixture::module();
        let f = &mut m.functions[4];
        f.values.push(fixture::address(Type::I64, false));
        f.blocks[0].instructions.push(fixture::instruction(
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
        prove(&fixture::exclusive_returns(false)).unwrap();
        let error = prove(&fixture::exclusive_returns(true)).unwrap_err();
        assert!(
            error.message.contains("actual target intersects"),
            "{error:?}"
        );
    }
}
