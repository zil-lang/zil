//! Independent stored-target reconstruction. Sorted vectors and batch CFG
//! solving deliberately do not consume analyzer roots, summaries, or claims.
use super::*;
#[path = "stored_flow_pure_copy.rs"]
mod pure_copy;

mod nominal;
pub(super) mod option_address;
mod retained;

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
    option_variants: option_address::Variants,
    address: Permission,
    // For a concrete Fixed environment only, a private Field(capture) prefix
    // retains its actual outer address and pointee leaves. It grants no public
    // projection, is never a signature class selector, and is not serialized.
    leaves: Vec<(Path, Permission)>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Frame {
    locals: Vec<Payload>,
    external: Vec<Payload>,
    captures: Vec<Payload>,
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
    returned_variants: option_address::Variants,
    none_values: Vec<ValueId>,
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
    p.option_variants.work(b)?;
    permission_work(&p.address, b)?;
    b.spend(p.leaves.len())?;
    for (path, p) in &p.leaves {
        b.spend(path.len())?;
        permission_work(p, b)?;
    }
    Ok(())
}
fn frame_work(f: &Frame, b: &mut Budget) -> Check<()> {
    b.spend(
        f.locals
            .len()
            .saturating_add(f.external.len())
            .saturating_add(f.captures.len()),
    )?;
    for p in f.locals.iter().chain(&f.external).chain(&f.captures) {
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
    changed |= to.option_variants.join(&from.option_variants, b)?;
    for (path, p) in &from.leaves {
        changed |= add_leaf(to, path, p, b)?;
    }
    Ok(changed)
}
fn join_frame(to: &mut Frame, from: &Frame, b: &mut Budget) -> Check<()> {
    b.spend(
        to.locals
            .len()
            .saturating_add(to.external.len())
            .saturating_add(to.captures.len()),
    )?;
    for (a, z) in to
        .locals
        .iter_mut()
        .zip(&from.locals)
        .chain(to.external.iter_mut().zip(&from.external))
        .chain(to.captures.iter_mut().zip(&from.captures))
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
            Type::AsyncRegistration { environment: inner }
            | Type::Address { inner, .. }
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
    if option_address::shape(ty, limits, b)? {
        return Ok(true);
    }
    if !matches!(
        storage,
        Type::Record { .. } | Type::Tuple(_) | Type::Enum { .. }
    ) || (!shape.retained.is_empty() && !shape.slots.is_empty())
        || (matches!(storage, Type::Enum { .. })
            && (!shape.slots.is_empty() || !retained::pure(storage, b)?))
    {
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
            Type::Address { mutable, inner } => {
                type_work(inner, limits, b)?;
                if !inner.is_address_pointee() {
                    return Err(err("stored shared leaf has invalid actual pointee"));
                }
                b.spend(path.len().saturating_add(1))?;
                actual.push((
                    path,
                    if *mutable {
                        StoredBorrowMode::ExclusiveAddress
                    } else {
                        StoredBorrowMode::SharedAddress
                    },
                ));
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
            Type::Enum { name, variants } => {
                if !retained::pure(t, b)? {
                    return Err(err("stored Enum has a real address or ownership payload"));
                }
                b.spend(name.len())?;
                if name.is_empty() {
                    return Err(err("stored Enum lacks actual nominal identity"));
                }
                let mut names = Vec::<&str>::new();
                for (variant, (variant_name, fields)) in variants.iter().enumerate().rev() {
                    b.spend(amount(
                        names.len().saturating_add(1),
                        variant_name.len().saturating_add(1),
                    )?)?;
                    if variant_name.is_empty() || names.contains(&variant_name.as_str()) {
                        return Err(err(
                            "stored actual Enum variants have duplicate or empty names",
                        ));
                    }
                    names.push(variant_name);
                    for (index, child) in fields.iter().enumerate().rev() {
                        b.spend(path.len().saturating_add(1))?;
                        let mut p = path.clone();
                        p.push(PlaceProjection::Variant(zil_ir::VariantProjection {
                            variant: variant as u32,
                            index: index as u32,
                        }));
                        todo.push((child, p));
                    }
                }
            }
            Type::Array { element, .. } => {
                if !retained::pure(t, b)? {
                    return Err(err("stored Array contains an address or ownership leaf"));
                }
                todo.push((element, path));
            }
            Type::BorrowedStorage { .. } => {
                if !retained::pure(t, b)? {
                    return Err(err(
                        "retained physical leaf needs a real borrowing consumer",
                    ));
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
        || (actual.is_empty() && shape.nominal_arguments.is_empty() && shape.retained.is_empty())
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
        class(row.binder, b)?;
    }
    for ((path, mode), row) in actual.iter().zip(&shape.slots) {
        b.spend(path.len().saturating_add(row.path.len()).saturating_add(1))?;
        if path != &row.path || mode != &row.mode {
            return Err(err(
                "actual physical stored path or mode differs from declared leaf",
            ));
        }
        if *mode == StoredBorrowMode::ExclusiveAddress
            && (!matches!(storage, Type::Record { .. })
                || shape.slots.len() != 1
                || !shape.retained.is_empty()
                || !matches!(path.as_slice(), [PlaceProjection::Field(_)]))
        {
            return Err(unsupported(
                "exclusive stored holder requires one direct Record slot",
                None,
            ));
        }
        class(row.binder, b)?;
    }
    let actual_retained = retained::expected(storage, limits, b)?;
    b.spend(shape.retained.len())?;
    if actual_retained.len() != shape.retained.len() {
        return Err(err("physical retained class coverage differs"));
    }
    for ((path, local), row) in actual_retained.iter().zip(&shape.retained) {
        b.spend(path.len().saturating_add(row.path.len()).saturating_add(1))?;
        if path != &row.path || *local != row.class {
            return Err(err(
                "retained row differs from actual physical boundary/class",
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
    if matches!(storage, Type::Record { .. } | Type::Enum { .. }) {
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

/// This admits only a qualified pure value, retaining its complete semantic
/// type. The ordinary read's actual pointer/loan proof remains mandatory.
pub(super) fn pure_read(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
    if !b.pure_nominal
        || !matches!(ty, Type::BorrowedStorage { storage, .. } if matches!(storage.as_ref(), Type::Record { .. } | Type::Enum { .. }))
    {
        return Ok(false);
    }
    let limits = b.limits.clone();
    type_work(ty, &limits, b)?;
    if !retained::pure(ty, b)? {
        return Ok(false);
    }
    qualification(module, ty, &limits, b)?;
    Ok(true)
}

pub(super) fn pure_inline(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
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
    let limits = b.limits.clone();
    type_work(ty, &limits, b)?;
    let mut pending = vec![ty];
    while let Some(current) = pending.pop() {
        b.spend(1)?;
        match current {
            Type::BorrowedStorage { storage, .. } => {
                physical(current, &limits, b)?;
                let mut count = 0usize;
                for instance in &module.life_instances {
                    type_work(&instance.ty, &limits, b)?;
                    type_work(current, &limits, b)?;
                    if instance.ty == *current {
                        count += 1;
                    }
                }
                if count != 1 {
                    return Err(err(
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

pub(super) fn qualifications(module: &TypedModule, limits: &Limits, mask: Option<&[bool]>, b: &mut Budget) -> Check<()> {
    for (index, f) in module.functions.iter().enumerate() {
        if mask.is_some_and(|mask| !mask[index]) { continue; }
        for block in &f.blocks {
            for i in &block.instructions {
                b.spend(1)?;
                if !super::class_flow::pointer_instruction(f, i)
                    && !super::stored_callables::instruction(module, f, i)
                    && !super::stored_owned::instruction(f, &i.kind)
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
                    return Err(unsupported(
                        "instruction is outside the checked stored profile, including unreachable blocks",
                        Some(i.span),
                    ));
                }
                if super::stored_owned::instruction(f, &i.kind) {
                    super::stored_owned::types(module, f, i, b)?;
                }
                if super::class_flow::pointer_instruction(f, i) {
                    super::class_flow::pointer_types(module, f, i, b)?;
                }
                if super::stored_callables::instruction(module, f, i) {
                    super::stored_callables::types(module, f, i, b)?;
                }
                if pure_copy::operation(&i.kind)
                    && option_address::types(module, f, i, b)?.is_none()
                {
                    pure_copy::types(f, i, b)?;
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
        if matches!(&instance.ty, Type::BorrowedStorage { life, .. } if life.slots.is_empty()) {
            b.pure_nominal = true;
        }
        physical(&instance.ty, limits, b)?;
        let (storage, _) = qualified(&instance.ty)
            .ok_or_else(|| err("nominal instance has no complete qualified type"))?;
        b.spend(declarations.len())?;
        let d = declarations
            .iter()
            .find(|d| d.id == instance.declaration)
            .ok_or_else(|| err("nominal instance references absent declaration"))?;
        if !matches!(
            (d.kind, storage),
            (LifeNominalKind::Record, Type::Record { .. })
                | (LifeNominalKind::Enum, Type::Enum { .. })
        ) {
            return Err(unsupported(
                "nominal kind has no actual physical stored consumer",
                None,
            ));
        }
        if d.parameters.len() != instance.arguments.len() {
            return Err(err("nominal actual list omits complete generic ordinal"));
        }
        nominal::rows(module, instance, limits, b)?;
        for previous in &module.life_instances[..i] {
            type_work(&previous.ty, limits, b)?;
            type_work(&instance.ty, limits, b)?;
            if previous.ty == instance.ty
                || (previous.declaration == instance.declaration
                    && nominal::same_key(previous, instance, limits, b)?)
            {
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

pub(super) fn header(module: &TypedModule, f: &TypedFunction, b: &mut Budget) -> Check<()> {
    fn nested_class(ty: &Type, depth: usize, b: &mut Budget) -> Check<bool> {
        if depth > b.limits.max_call_depth {
            return Err(limit("stored signature class depth exceeds limit"));
        }
        b.spend(1)?;
        match ty {
            Type::BorrowedStorage { .. } => Ok(true),
            Type::Address { inner, .. } | Type::Array { element: inner, .. } => {
                nested_class(inner, depth + 1, b)
            }
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                for field in fields {
                    if nested_class(field, depth + 1, b)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Type::Record { fields, .. } => {
                b.spend(fields.len())?;
                for (_, field) in fields {
                    if nested_class(field, depth + 1, b)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Type::Enum { variants, .. } => {
                b.spend(variants.len())?;
                for (_, fields) in variants {
                    b.spend(fields.len())?;
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
            if f.environment.is_none() && (qualified(ty).is_some() || addressed(ty).is_some()) {
                return Err(err("stored signature requires complete class relation"));
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
                return Err(err(
                    "nested stored qualified signature requires complete class relation",
                ));
            }
        }
        return Ok(());
    };
    crate::signature_life::charge(c, b)?;
    b.spend(f.parameters.len())?;
    if f.reference_return_bound.is_some() || f.foreign.is_some() {
        return Err(err("stored signature contradicts another active profile"));
    }
    let mut parameters = Vec::with_capacity(f.parameters.len());
    let mut has_fixed_environment = false;
    for p in &f.parameters {
        let ty = &f.locals[p.0 as usize].ty;
        type_work(ty, &b.limits.clone(), b)?;
        type_work(ty, &b.limits.clone(), b)?;
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
            type_work(ty, &b.limits.clone(), b)?;
        }
        if f.callable_contract.is_some()
            && let Type::Managed(payload) = ty
            && let Type::Record { fields, .. } = payload.as_ref()
        {
            b.spend(fields.len().saturating_mul(2))?;
        }
        has_fixed_environment |= fixed_environment(module, ty);
        parameters.push(ty.clone());
    }
    type_work(&f.return_type, &b.limits.clone(), b)?;
    type_work(&f.return_type, &b.limits.clone(), b)?;
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
        type_work(&f.return_type, &b.limits.clone(), b)?;
    }
    if let Some(id) = f.environment {
        let d = module
            .closures
            .get(id.0 as usize)
            .ok_or_else(|| err("captured Fixed entry has no actual environment"))?;
        if d.id != id || d.function != f.id || d.storage_life.as_ref() != Some(c) {
            return Err(err("captured Fixed entry and definition relation differ"));
        }
        super::stored_callables::captured_profile(d, b)?;
        return super::signature_life::closure_relation(
            &parameters,
            &d.captures,
            super::stored_callables::life_result(d),
            c,
            Some(f.span),
        );
    }
    if has_fixed_environment {
        b.spend(parameters.len())?;
        for ty in &parameters {
            if !fixed_environment(module, ty)
                && !super::stored_callables::shared_input(ty)
                && !super::stored_owned::owner(ty)
            {
                return Err(unsupported(
                    "captured Fixed helper requires shared finite data parameters",
                    Some(f.span),
                ));
            }
        }
    }
    super::signature_life::function_relation(
        module,
        &parameters,
        &f.return_type,
        c,
        Some(f.span),
        f.callable_contract.is_some(),
    )?;
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
    if b.static_utf8 {
        b.spend(p.sources.len())?;
    }
    if b.static_utf8
        && write
        && p.sources
            .iter()
            .any(|source| source.base == HoldRoot::Static)
    {
        return Err(err("program-static text cannot authorize mutation"));
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
fn captured_definition<'a>(
    module: &'a TypedModule,
    f: &TypedFunction,
) -> Option<&'a ClosureDefinition> {
    f.environment
        .and_then(|id| module.closures.get(id.0 as usize))
        .filter(|d| d.storage_life.is_some())
}
fn fixed_environment(module: &TypedModule, ty: &Type) -> bool {
    let ty = if let Type::Address { inner, .. } = ty {
        inner.as_ref()
    } else {
        ty
    };
    matches!(ty, Type::Closure(id) if module.closures.get(id.0 as usize).is_some_and(|d| d.storage_life.is_some()))
}
fn seed_environment(
    module: &TypedModule,
    ty: &Type,
    base: HoldRoot,
    out: &mut Payload,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<()> {
    let ty = if let Type::Address { inner, .. } = ty {
        inner.as_ref()
    } else {
        ty
    };
    let Type::Closure(id) = ty else {
        return Err(err("environment carrier is not actual Closure"));
    };
    let d = module
        .closures
        .get(id.0 as usize)
        .ok_or_else(|| err("environment definition absent"))?;
    b.spend(d.captures.len())?;
    for (index, ty) in d.captures.iter().enumerate() {
        let capture = seed_payload(ty, base, loans, b)?;
        let prefix = vec![PlaceProjection::Field(index as u32)];
        for (tail, mut p) in std::iter::once((Vec::new(), capture.address)).chain(capture.leaves) {
            if p.sources.is_empty() {
                continue;
            }
            b.spend(tail.len().saturating_add(1))?;
            let mut path = prefix.clone();
            path.extend(tail);
            for source in &mut p.sources {
                b.spend(path.len())?;
                source.leaf = path.clone();
            }
            // The corresponding abstract loan must name the same physical target.
            for loan in &p.loans {
                source_work(&p.sources, b)?;
                loans[*loan].sources = p.sources.clone();
            }
            add_leaf(out, &path, &p, b)?;
        }
    }
    Ok(())
}
fn seed_payload(
    ty: &Type,
    base: HoldRoot,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<Payload> {
    let mut p = Payload::default();
    if let Type::Address { mutable, .. } = ty {
        let source = Source {
            base,
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
                base,
                leaf: slot.path.clone(),
                inline: Vec::new(),
            };
            let id = loans.len();
            loans.push(Loan {
                sources: vec![source.clone()],
                parents: Vec::new(),
                exclusive: slot.mode == StoredBorrowMode::ExclusiveAddress,
            });
            p.leaves.push((
                slot.path.clone(),
                Permission {
                    sources: vec![source],
                    loans: vec![id],
                },
            ));
        }
    }
    if matches!(ty, Type::Desc(_)) {
        super::stored_flow::type_work(ty, &b.limits.clone(), b)?;
        if super::stored_owned::description(ty) {
            b.spend(1)?;
            p.address.sources.push(Source {
                base,
                leaf: Vec::new(),
                inline: Vec::new(),
            });
        }
    }
    Ok(p)
}
fn empty_frame(module: &TypedModule, f: &TypedFunction, b: &mut Budget) -> Check<Frame> {
    b.spend(f.locals.len().saturating_add(f.parameters.len()))?;
    let count = captured_definition(module, f).map_or(0, |d| d.captures.len());
    b.spend(count)?;
    Ok(Frame {
        captures: vec![Payload::default(); count],
        locals: vec![Payload::default(); f.locals.len()],
        external: vec![Payload::default(); f.parameters.len()],
    })
}
fn seed(
    module: &TypedModule,
    f: &TypedFunction,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<Frame> {
    let mut state = empty_frame(module, f, b)?;
    for (position, local) in f.parameters.iter().enumerate() {
        b.spend(1)?;
        let ty = &f.locals[local.0 as usize].ty;
        let mut p = seed_payload(ty, HoldRoot::Parameter(position as u32), loans, b)?;
        if option_address::candidate(ty) {
            b.spend(96)?;
            if option_address::signature(ty) {
                p.option_variants = option_address::Variants::parameter(position as u32, b)?;
            }
        }
        if fixed_environment(module, ty) {
            seed_environment(
                module,
                ty,
                HoldRoot::Parameter(position as u32),
                &mut p,
                loans,
                b,
            )?;
        }
        if addressed(ty).is_some() || fixed_environment(module, ty) {
            state.external[position] = copy_payload(&p, b)?;
        }
        state.locals[local.0 as usize] = p;
    }
    if let Some(d) = captured_definition(module, f) {
        b.spend(d.captures.len())?;
        for (index, ty) in d.captures.iter().enumerate() {
            state.captures[index] = seed_payload(ty, HoldRoot::Capture(index as u32), loans, b)?;
        }
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
            HoldRoot::Static => {
                return Err(err("static text cannot supply nested carrier permissions"));
            }
            HoldRoot::Local(l) => state.locals.get(l.0 as usize),
            HoldRoot::Parameter(p) => state.external.get(p as usize),
            HoldRoot::Capture(p) => state.captures.get(p as usize),
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
    if source.base == HoldRoot::Static {
        if !source.leaf.is_empty() || !source.inline.is_empty() {
            return Err(err("static text summary contains a nested projection"));
        }
        b.spend(1)?;
        return Ok(Permission {
            sources: vec![source.clone()],
            loans: Vec::new(),
        });
    }
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
    let option_transfer = option_address::transfer(module, f, i, values, b)?;
    match &i.kind {
        InstructionKind::StaticUtf8 { .. } => {
            b.spend(1)?;
            out.address.sources.push(Source {
                base: HoldRoot::Static,
                leaf: Vec::new(),
                inline: Vec::new(),
            });
        }
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
        InstructionKind::StorageLive(l)
        | InstructionKind::StorageDead(l)
        | InstructionKind::DropClosure { local: l } => {
            state.locals[l.0 as usize] = Payload::default()
        }
        InstructionKind::DescribeLocal { local } => {
            b.spend(1)?;
            out.address.sources.push(Source {
                base: HoldRoot::Local(*local),
                leaf: Vec::new(),
                inline: Vec::new(),
            });
        }
        InstructionKind::DescribeCapture { index } => {
            b.spend(1)?;
            out.address.sources.push(Source {
                base: HoldRoot::Capture(*index),
                leaf: Vec::new(),
                inline: Vec::new(),
            });
        }
        InstructionKind::TakeCapture { .. }
        | InstructionKind::MakeManaged { .. }
        | InstructionKind::ReadDesc { .. } => {}
        InstructionKind::DropManaged { local } => {
            state.locals[local.0 as usize] = Payload::default();
        }
        InstructionKind::TakeLocal { local } => {
            out = copy_payload(&state.locals[local.0 as usize], b)?;
            state.locals[local.0 as usize] = Payload::default();
        }
        InstructionKind::ReadCapture { index } if captured_definition(module, f).is_some() => {
            out = copy_payload(
                state
                    .captures
                    .get(*index as usize)
                    .ok_or_else(|| err("capture payload absent"))?,
                b,
            )?;
        }
        InstructionKind::BorrowCapture {
            index,
            mutable: false,
        } => {
            let definition = captured_definition(module, f)
                .filter(|d| super::stored_callables::lending(d))
                .ok_or_else(|| err("capture loan has no qualified lender entry"))?;
            if !definition
                .captures
                .get(*index as usize)
                .is_some_and(super::stored_callables::scalar)
            {
                return Err(err("capture loan does not address an inline scalar field"));
            }
            b.spend(1)?;
            out.address.sources.push(Source {
                base: HoldRoot::Capture(*index),
                leaf: Vec::new(),
                inline: Vec::new(),
            });
        }
        kind if super::stored_callables::called(module, f, kind)
            .is_some_and(|(definition, _)| super::stored_callables::lending(definition)) =>
        {
            let (definition, _) = super::stored_callables::called(module, f, kind)
                .ok_or_else(|| err("lender entry absent"))?;
            let receiver = match kind {
                InstructionKind::CallClosureShared { callee, .. }
                | InstructionKind::CallClosureMut { callee, .. } => {
                    b.spend(1)?;
                    Permission {
                        sources: vec![Source {
                            base: HoldRoot::Local(*callee),
                            leaf: vec![],
                            inline: vec![],
                        }],
                        loans: vec![],
                    }
                }
                InstructionKind::CallClosureBorrowed { callee, .. } => {
                    let mut permission = Permission::default();
                    join_permission(&mut permission, &val(*callee)?.address, b)?;
                    permission
                }
                _ => {
                    return Err(err(
                        "environment result requires a borrowed actual receiver",
                    ));
                }
            };
            for (path, returned) in &summaries[definition.function.0 as usize].result {
                b.spend(path.len().saturating_add(1))?;
                if !path.is_empty() {
                    return Err(err("environment lending result has extra stored leaves"));
                }
                for actual in returned {
                    let HoldRoot::Capture(field) = actual.base else {
                        return Err(err("lender returned a nonenvironment origin"));
                    };
                    if !actual.leaf.is_empty() {
                        return Err(err("lender returned an external capture target"));
                    }
                    for original in &receiver.sources {
                        b.spend(
                            width(original)
                                .saturating_add(actual.inline.len())
                                .saturating_add(2),
                        )?;
                        let mut source = original.clone();
                        source.inline.push(PlaceProjection::Field(field));
                        source.inline.extend_from_slice(&actual.inline);
                        add_sources(&mut out.address.sources, &[source], b)?;
                    }
                    add_loans(&mut out.address.loans, &receiver.loans, b)?;
                }
            }
        }
        InstructionKind::MakeClosure { closure, captures }
            if module
                .closures
                .get(closure.0 as usize)
                .is_some_and(|d| d.storage_life.is_some()) =>
        {
            let d = &module.closures[closure.0 as usize];
            b.spend(captures.len())?;
            for (index, (value, ty)) in captures.iter().zip(&d.captures).enumerate() {
                let actual = val(*value)?;
                let prefix = vec![PlaceProjection::Field(index as u32)];
                if matches!(ty, Type::Address { .. }) {
                    add_leaf(&mut out, &prefix, &actual.address, b)?;
                }
                if let Some((_, shape)) = addressed(ty) {
                    for slot in &shape.slots {
                        let p = select(state, &actual.address, &slot.path, b)?;
                        b.spend(slot.path.len().saturating_add(1))?;
                        let mut path = prefix.clone();
                        path.extend(&slot.path);
                        add_leaf(&mut out, &path, &p, b)?;
                    }
                }
            }
        }
        InstructionKind::BorrowLocal { local } | InstructionKind::BorrowLocalMut { local } => {
            b.spend(1)?;
            out.address.sources.push(Source {
                base: HoldRoot::Local(*local),
                leaf: Vec::new(),
                inline: Vec::new(),
            });
            if qualified(&f.locals[local.0 as usize].ty).is_some()
                || fixed_environment(module, &f.locals[local.0 as usize].ty)
            {
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
            if fixed_environment(module, vt(f, *address)?) {
                for (path, p) in &val(*address)?.leaves {
                    add_leaf(&mut out, path, p, b)?;
                }
            }
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
        InstructionKind::LoadStoredBorrow {
            storage,
            path,
            mode: StoredBorrowLoad::Reborrow { mutable: true },
        } => {
            out.address = select(state, &val(*storage)?.address, path, b)?;
            add_loans(&mut out.address.loans, &val(*storage)?.address.loans, b)?;
        }
        InstructionKind::LoadStoredBorrow { .. } | InstructionKind::StoreStoredBorrow { .. } => {
            return Err(unsupported(
                "stored reborrow/take/replacement CFG is not yet implemented",
                Some(i.span),
            ));
        }
        InstructionKind::ForeignCall { function, .. } => {
            let declaration = module
                .functions
                .get(function.0 as usize)
                .ok_or_else(|| err("foreign scalar entry is absent"))?;
            if !super::stored_callables::scalar_foreign(declaration, b)? {
                return Err(err(
                    "stored foreign call requires an actual scalar-only declaration",
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
                .ok_or_else(|| err("call target absent"))?;
            if callee.foreign.is_some()
                || callee.environment.is_some()
                || callee.call_kind != CallKind::Direct
            {
                return Err(err(
                    "stored call requires an independently checked ordinary direct body",
                ));
            }
            out.option_variants = summaries[function.0 as usize]
                .returned_variants
                .substitute(arguments, values, b)?;
            for (path, sources) in &summaries[function.0 as usize].result {
                let mut p = Permission::default();
                for source in sources {
                    join_permission(
                        &mut p,
                        &instantiate(source, arguments, values, state, b)?,
                        b,
                    )?;
                    if path.is_empty()
                        && matches!(callee.return_type, Type::Address { mutable: true, .. })
                        && !source.leaf.is_empty()
                        && let HoldRoot::Parameter(index) = source.base
                    {
                        let actual = arguments
                            .get(index as usize)
                            .and_then(|v| values.get(v.0 as usize))
                            .ok_or_else(|| err("stored result carrier parameter absent"))?;
                        add_loans(&mut p.loans, &actual.address.loans, b)?;
                    }
                }
                if path.is_empty() {
                    out.address = p;
                } else if !option_address::candidate(&callee.return_type)
                    || out.option_variants.mask() & 2 != 0
                {
                    add_leaf(&mut out, path, &p, b)?;
                }
            }
        }
        _ if option_transfer.is_some() => {
            out = option_transfer.unwrap();
        }
        _ if pure_copy::operation(&i.kind) => {
            pure_copy::types(f, i, b)?;
            for input in instruction_operands(&i.kind) {
                let payload = val(input)?;
                if !payload.address.sources.is_empty()
                    || !payload.address.loans.is_empty()
                    || !payload.leaves.is_empty()
                {
                    return Err(err(
                        "ordinary pure Copy input retains actual stored source permissions",
                    ));
                }
            }
        }
        _ if super::stored_callables::instruction(module, f, i) => {}
        InstructionKind::FunctionRef { .. } | InstructionKind::CallIndirect { .. }
            if super::class_flow::pointer_instruction(f, i) => {}
        InstructionKind::ReadAddress { address } => {
            let Type::Address { inner, .. } = vt(f, *address)? else {
                return Err(err("read transfer has no actual Address"));
            };
            if qualified(inner).is_some() && !pure_read(module, inner, b)? {
                return Err(err(
                    "read transfer would erase actual borrowed/owned leaves",
                ));
            }
            // Access checks retain the actual pointer's permissions and loans.
            // This proven pure payload has no runtime leaves to transfer;
            // the complete result type preserves the semantic class mapping.
            // Do not turn the containing storage root into a phantom target.
        }
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
        | InstructionKind::ExtractField { .. }
        | InstructionKind::ConvertInteger { .. }
        | InstructionKind::ConvertNumber { .. }
        | InstructionKind::Unary { .. }
        | InstructionKind::Binary { .. }
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
        && b.static_utf8
        && matches!(vt(f, id)?, Type::Address { mutable: true, .. })
    {
        b.spend(out.address.sources.len())?;
        if out
            .address
            .sources
            .iter()
            .any(|source| source.base == HoldRoot::Static)
        {
            return Err(err("static text became a mutable address"));
        }
    }
    if let Some(id) = i.result
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
                row.exclusive = declaration.mode == StoredBorrowMode::ExclusiveAddress;
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
    let entry = seed(module, f, &mut loans, b)?;
    let mut values = vec![Payload::default(); f.values.len()];
    let mut outputs = Vec::new();
    for _ in &f.blocks {
        outputs.push(empty_frame(module, f, b)?);
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
                empty_frame(module, f, b)?
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

fn reverse(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    live: &mut [bool],
    b: &mut Budget,
) -> Check<()> {
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
        InstructionKind::CallClosureShared { callee, .. }
        | InstructionKind::CallClosureMut { callee, .. }
            if fixed_environment(module, &f.locals[callee.0 as usize].ty) =>
        {
            live[f.values.len() + callee.0 as usize] = true;
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
fn live_out(
    module: &TypedModule,
    f: &TypedFunction,
    g: &FunctionGraph,
    b: &mut Budget,
) -> Check<Vec<Vec<bool>>> {
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
            reverse(module, f, i, &mut out, b)?;
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
    let output_live = live_out(module, f, g, b)?;
    for (bi, block) in f.blocks.iter().enumerate() {
        b.spend(1)?;
        if !g.reachable[bi] {
            continue;
        }
        let mut state = if bi == f.entry.0 as usize {
            copy_frame(&solution.entry, b)?
        } else {
            empty_frame(module, f, b)?
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
            reverse(module, f, i, &mut live, b)?;
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
                InstructionKind::BorrowCapture { index, mutable } => {
                    b.spend(1)?;
                    let actual = Permission {
                        sources: vec![Source {
                            base: HoldRoot::Capture(*index),
                            leaf: vec![],
                            inline: vec![],
                        }],
                        loans: vec![],
                    };
                    access(&actual, *mutable, &active, &solution.loans, requirements, b)?;
                }
                InstructionKind::Store { local, .. }
                | InstructionKind::StorageDead(local)
                | InstructionKind::TakeLocal { local }
                | InstructionKind::DropClosure { local }
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
                InstructionKind::LoadStoredBorrow {
                    storage,
                    path,
                    mode,
                } => {
                    let exclusive = matches!(
                        mode,
                        StoredBorrowLoad::Reborrow { mutable: true } | StoredBorrowLoad::Take
                    );
                    access(
                        pointer(*storage)?,
                        exclusive,
                        &active,
                        &solution.loans,
                        requirements,
                        b,
                    )?;
                    let p = select(&state, pointer(*storage)?, path, b)?;
                    access(&p, exclusive, &active, &solution.loans, requirements, b)?;
                }
                InstructionKind::CallIndirect { arguments, .. }
                | InstructionKind::CallClosure { arguments, .. }
                | InstructionKind::CallClosureBorrowed { arguments, .. }
                | InstructionKind::CallClosureShared { arguments, .. }
                | InstructionKind::CallClosureMut { arguments, .. }
                | InstructionKind::CallClosureOnce { arguments, .. } => {
                    let mut all: Vec<(Permission, bool)> = Vec::new();
                    match &i.kind {
                        InstructionKind::CallClosureBorrowed { callee, .. } => {
                            let p = pointer(*callee)?;
                            permission_work(p, b)?;
                            b.spend(1)?;
                            all.push((
                                p.clone(),
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
                            b.spend(2)?;
                            all.push((
                                Permission {
                                    sources: vec![Source {
                                        base: HoldRoot::Local(*callee),
                                        leaf: vec![],
                                        inline: vec![],
                                    }],
                                    loans: vec![],
                                },
                                matches!(i.kind, InstructionKind::CallClosureMut { .. }),
                            ));
                        }
                        _ => {}
                    }

                    let receiver = match &i.kind {
                        InstructionKind::CallClosureShared { callee, .. }
                        | InstructionKind::CallClosureMut { callee, .. }
                            if fixed_environment(module, &f.locals[callee.0 as usize].ty) =>
                        {
                            Some(&state.locals[callee.0 as usize])
                        }
                        InstructionKind::CallClosure { callee, .. }
                        | InstructionKind::CallClosureBorrowed { callee, .. }
                        | InstructionKind::CallClosureOnce { callee, .. }
                            if fixed_environment(module, vt(f, *callee)?) =>
                        {
                            Some(&solution.values[callee.0 as usize])
                        }
                        _ => None,
                    };
                    if let Some(receiver) = receiver {
                        b.spend(receiver.leaves.len())?;
                        for (_, p) in &receiver.leaves {
                            permission_work(p, b)?;
                            all.push((p.clone(), false));
                        }
                    }
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
                InstructionKind::MakeClosure { closure, captures }
                    if module
                        .closures
                        .get(closure.0 as usize)
                        .is_some_and(|d| d.storage_life.is_some()) =>
                {
                    b.spend(captures.len())?;
                    for (value, ty) in captures
                        .iter()
                        .zip(&module.closures[closure.0 as usize].captures)
                    {
                        let actual = &solution.values[value.0 as usize];
                        if matches!(ty, Type::Address { .. }) {
                            access(
                                &actual.address,
                                false,
                                &active,
                                &solution.loans,
                                requirements,
                                b,
                            )?;
                        }
                        if let Some((_, shape)) = addressed(ty) {
                            for slot in &shape.slots {
                                let p = select(&state, &actual.address, &slot.path, b)?;
                                access(&p, false, &active, &solution.loans, requirements, b)?;
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
    if !s.none_values.is_empty() {
        b.spend(s.none_values.len())?;
    }
    s.returned_variants.work(b)?;
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
    module: &TypedModule,
    f: &TypedFunction,
    g: &FunctionGraph,
    solution: &Solution,
    inactive: Option<&[ValueId]>,
    b: &mut Budget,
) -> Check<Summary> {
    let mut summary = Summary {
        complete: true,
        ..Summary::default()
    };
    for (id, (ty, p)) in f.values.iter().zip(&solution.values).enumerate() {
        b.spend(1)?;
        if p.option_variants.mask() == 1 {
            b.spend(2)?;
            summary.none_values.push(ValueId(id as u32));
        }
        if let Some(dead) = inactive {
            b.spend(dead.len().saturating_add(1))?;
            if dead.contains(&ValueId(id as u32)) {
                continue;
            }
        }
        if matches!(ty, Type::Address { .. }) && p.address.sources.is_empty() {
            summary.complete = false;
        }
        if let Some((_, shape)) = qualified(ty) {
            if option_address::candidate(ty) {
                b.spend(96)?;
                if !option_address::signature(ty) {
                    return Err(err("Option activity has no exact shape"));
                }
                let mask = p.option_variants.mask();
                if mask == 0 {
                    summary.complete = false;
                }
                if mask == 1 {
                    continue;
                }
            }
            for row in &shape.slots {
                if leaf(p, &row.path, b)?.is_none_or(|p| p.sources.is_empty()) {
                    summary.complete = false;
                }
            }
        }
        if fixed_environment(module, ty) {
            let carrier = if let Type::Address { inner, .. } = ty {
                inner.as_ref()
            } else {
                ty
            };
            let Type::Closure(id) = carrier else {
                return Err(err("actual environment type is absent"));
            };
            let d = &module.closures[id.0 as usize];
            b.spend(d.captures.len())?;
            for (index, ty) in d.captures.iter().enumerate() {
                let prefix = vec![PlaceProjection::Field(index as u32)];
                if matches!(ty, Type::Address { .. })
                    && leaf(p, &prefix, b)?.is_none_or(|p| p.sources.is_empty())
                {
                    summary.complete = false;
                }
                if let Some((_, shape)) = addressed(ty) {
                    for row in &shape.slots {
                        b.spend(row.path.len().saturating_add(1))?;
                        let mut path = prefix.clone();
                        path.extend(&row.path);
                        if leaf(p, &path, b)?.is_none_or(|p| p.sources.is_empty()) {
                            summary.complete = false;
                        }
                    }
                }
            }
        }
    }
    for (block, reachable) in f.blocks.iter().zip(&g.reachable) {
        b.spend(1)?;
        if *reachable && let Terminator::Return(Some(id)) = block.terminator {
            let payload = &solution.values[id.0 as usize];
            summary
                .returned_variants
                .join(&payload.option_variants, b)?;
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
    check_with_mask(module, graphs, plan, None, b)
}
pub(super) fn check_mixed(module: &TypedModule, graphs: &[FunctionGraph], plan: &CandidatePlan, mask: &[bool], b: &mut Budget) -> Check<()> {
    check_with_mask(module, graphs, plan, Some(mask), b)
}
fn check_with_mask(module: &TypedModule, graphs: &[FunctionGraph], plan: &CandidatePlan, mask: Option<&[bool]>, b: &mut Budget) -> Check<()> {
    if mask.is_none() {
        super::stored_owned::module(module, b)?;
        super::stored_callables::definitions(module, b)?;
    }
    for (index, f) in module.functions.iter().enumerate() {
        if mask.is_some_and(|mask| !mask[index]) { continue; }
        super::class_flow::check(module, f, &graphs[f.id.0 as usize].reachable, b)?;
        if !module.resources.is_empty() && matches!(f.return_type, Type::Desc(_)) {
            return Err(unsupported(
                "owning Fixed profile does not export descriptions",
                Some(f.span),
            ));
        }
        if (f.foreign.is_some() && !super::stored_callables::scalar_foreign(f, b)?)
            || f.callable_return_contract.is_some()
            || f.call_kind != CallKind::Direct
        {
            return Err(unsupported(
                "stored profile requires ordinary direct function bodies",
                Some(f.span),
            ));
        }
        if let Some(id) = f.environment {
            let d = module
                .closures
                .get(id.0 as usize)
                .ok_or_else(|| err("callback function environment is absent"))?;
            if !super::stored_callables::finite(module, id) || d.function != f.id {
                return Err(err("callback function is not the actual finite entry"));
            }
        }
        for ty in f
            .locals
            .iter()
            .map(|l| &l.ty)
            .chain(&f.values)
            .chain(std::iter::once(&f.return_type))
        {
            type_work(ty, &b.limits.clone(), b)?;
            if !module.resources.is_empty() {
                super::stored_owned::value_type(ty, b)?;
            }
            if !(super::stored_callables::value(module, ty)
                || qualified(ty).is_some()
                || addressed(ty).is_some()
                || matches!(ty,Type::Address{inner,..}if inner.is_address_pointee())
                || ty.is_copy_value()
                || super::stored_owned::owner(ty)
                || super::stored_owned::description(ty)
                || pure_inline(module, ty, b)?
                || matches!(ty, Type::Address { inner, mutable: false } if pure_inline(module, inner, b)?))
            {
                return Err(unsupported(
                    "owned or opaque executable value lacks actual stored permission integration",
                    Some(f.span),
                ));
            }
        }
    }
    if mask.is_none() { super::stored_callables::roles(module, graphs, b)?; }
    solve_check(module, graphs, plan, None, mask, b)
}
fn solve_check(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    plan: &CandidatePlan,
    inactive: Option<&[Vec<ValueId>]>,
    mask: Option<&[bool]>,
    b: &mut Budget,
) -> Check<()> {
    b.spend(module.functions.len().saturating_mul(2))?;
    let mut observed_none = false;
    let mut summaries = vec![Summary::default(); module.functions.len()];
    if let Some(mask) = mask {
        for (index, yes) in mask.iter().enumerate() { if !yes { summaries[index].complete = true; } }
    }
    let mut solved: Vec<Option<Solution>> = (0..module.functions.len()).map(|_| None).collect();
    loop {
        let mut changed = false;
        let mut pending = Vec::new();
        b.spend(module.functions.len())?;
        for index in (0..module.functions.len()).rev() {
            if mask.is_some_and(|mask| !mask[index]) { continue; }
            let f = &module.functions[index];
            let mut solution = solve(module, f, &graphs[index], &summaries, b)?;
            let mut summary = summarize(
                module,
                f,
                &graphs[index],
                &solution,
                inactive.map(|rows| rows[index].as_slice()),
                b,
            )?;
            observed_none |= !summary.none_values.is_empty();
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
    if mask.is_none() && inactive.is_none()
        && observed_none
        && let Some(proof) = option_address::refine(module, graphs, &summaries, b)?
    {
        return solve_check(module, &proof.graphs, plan, Some(&proof.inactive), mask, b);
    }
    for (index, (f, s)) in module.functions.iter().zip(&summaries).enumerate() {
        if mask.is_some_and(|mask| !mask[index]) { continue; }
        if !s.complete {
            return Err(err(
                "stored/reference operation has no complete actual target permission",
            ));
        }
        let lender = captured_definition(module, f).filter(|d| super::stored_callables::lending(d));
        if let Some(definition) = lender {
            b.spend(definition.lending_captures.len().saturating_add(1))?;
            let mut fields = Vec::new();
            for (path, origins) in &s.result {
                if !path.is_empty() || origins.is_empty() {
                    return Err(err("lender result lacks real capture sources"));
                }
                for origin in origins {
                    b.spend(width(origin).saturating_add(fields.len()).saturating_add(1))?;
                    let HoldRoot::Capture(field) = origin.base else {
                        return Err(err(
                            "lender retained activation or explicit argument storage",
                        ));
                    };
                    if !origin.leaf.is_empty()
                        || !origin.inline.is_empty()
                        || !definition
                            .captures
                            .get(field as usize)
                            .is_some_and(super::stored_callables::scalar)
                    {
                        return Err(err(
                            "lender result is not grounded in a real inline scalar capture",
                        ));
                    }
                    if let Err(index) = fields.binary_search(&field) {
                        fields.insert(index, field);
                    }
                }
            }
            if fields != definition.lending_captures {
                return Err(err(
                    "lending capture claim differs from independently recovered fields",
                ));
            }
        }
        for (path, sources) in &s.result {
            if sources.is_empty() {
                return Err(err("stored/reference result has no actual input source"));
            }
            for source in sources {
                if lender.is_some() {
                    continue;
                }
                if source.base == HoldRoot::Static {
                    if !source.leaf.is_empty() || !source.inline.is_empty() {
                        return Err(err("static result has a nested permission projection"));
                    }
                    continue;
                }
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
            "/../../tests/language-readiness/terminal/higher-rank-borrows/stored_flow_fixture.rs"
        ));
    }
    // These fixtures have one straight-line block. The candidate supplies only
    // root identities; all paths, loans and environment class bindings remain
    // independently reconstructed by the verifier.
    fn captured_candidate(m: &TypedModule) -> CandidatePlan {
        let mut c = fixture::class_candidate(m);
        for (f, plan) in m.functions.iter().zip(&mut c.functions) {
            let mut locals: Vec<Vec<HoldRoot>> = f.locals.iter().map(|_| vec![]).collect();
            for (index, local) in f.parameters.iter().enumerate() {
                locals[local.0 as usize] = vec![HoldRoot::Parameter(index as u32)];
            }
            let mut initialized = f.parameters.clone();
            for i in &f.blocks[0].instructions {
                let roots = match &i.kind {
                    InstructionKind::Load(l) | InstructionKind::TakeLocal { local: l } => {
                        locals[l.0 as usize].clone()
                    }
                    InstructionKind::ReadCapture { index } => vec![HoldRoot::Capture(*index)],
                    InstructionKind::DescribeLocal { local } => vec![HoldRoot::Local(*local)],
                    InstructionKind::DescribeCapture { index } => vec![HoldRoot::Capture(*index)],
                    InstructionKind::BorrowLocal { local }
                    | InstructionKind::BorrowLocalMut { local } => {
                        let mut roots = vec![HoldRoot::Local(*local)];
                        if matches!(
                            f.locals[local.0 as usize].ty,
                            Type::BorrowedStorage { .. } | Type::Closure(_)
                        ) {
                            roots.extend(locals[local.0 as usize].clone());
                        }
                        roots
                    }
                    InstructionKind::MakeClosure { captures, .. }
                    | InstructionKind::MakeAggregate { fields: captures } => captures
                        .iter()
                        .flat_map(|v| plan.holds[v.0 as usize].roots.clone())
                        .collect(),
                    InstructionKind::LoadStoredBorrow { storage, .. } => {
                        plan.holds[storage.0 as usize].roots.clone()
                    }
                    InstructionKind::ReborrowAddress { address, .. } => {
                        plan.holds[address.0 as usize].roots.clone()
                    }
                    _ => vec![],
                };
                if let Some(v) = i.result {
                    let mut roots = roots;
                    roots.sort();
                    roots.dedup();
                    plan.holds[v.0 as usize].roots = roots;
                }
                match &i.kind {
                    InstructionKind::Store { local, value } => {
                        locals[local.0 as usize] = plan.holds[value.0 as usize].roots.clone();
                        if !initialized.contains(local) {
                            initialized.push(*local);
                        }
                    }
                    InstructionKind::StorageDead(l) | InstructionKind::TakeLocal { local: l } => {
                        initialized.retain(|x| x != l)
                    }
                    _ => {}
                }
            }
            initialized.sort();
            plan.blocks[0].initialized_out = initialized;
        }
        c
    }
    #[test]
    fn capture_grounded_fixed_actual_environments_raw_and_wire() {
        for kind in [
            zil_ir::CallableKind::Fn,
            zil_ir::CallableKind::FnMut,
            zil_ir::CallableKind::FnOnce,
        ] {
            for make in [
                fixture::captured_fixed,
                fixture::captured_fixed_instances,
                fixture::captured_fixed_helper,
            ] {
                for wrong in [false, true] {
                    let m = make(kind, wrong);
                    let c = captured_candidate(&m);
                    let bytes =
                        zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
                    for result in [
                        super::super::verify_module(m.clone(), c, &m.target, &Limits::default()),
                        super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()),
                    ] {
                        if wrong {
                            let error = result.unwrap_err();
                            assert!(
                                error.iter().any(|d| d.message.contains("class equality")),
                                "{kind:?}: {error:?}"
                            );
                        } else {
                            result.unwrap_or_else(|error| panic!("{kind:?}: {error:?}"));
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn capture_grounded_fixed_parent_target_keeps_real_loan() {
        for dead in [false, true] {
            let m = fixture::captured_fixed_parent_end(dead);
            let c = captured_candidate(&m);
            let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
            for result in [
                super::super::verify_module(m.clone(), c, &m.target, &Limits::default()),
                super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()),
            ] {
                if dead {
                    let error = result.unwrap_err();
                    assert!(
                        error
                            .iter()
                            .any(|d| d.message.contains("live incompatible loan")),
                        "{error:?}"
                    );
                } else {
                    result.unwrap();
                }
            }
        }
    }
    #[test]
    fn capture_grounded_fixed_malformed_contracts_and_actual_role_reject() {
        for variant in 0..9 {
            let mut m = fixture::captured_fixed_helper(zil_ir::CallableKind::Fn, false);
            match variant {
                0 => {
                    m.closures[0].storage_life.as_mut().unwrap().classes[1].binder =
                        StorageBinder::Fixed(1)
                }
                1 => {
                    let d = m.closures[0].storage_life.as_mut().unwrap();
                    d.bindings[1].binder = StorageBinder::Invocation(0);
                    m.functions[0].storage_life = Some(d.clone());
                }
                2 => {
                    let role = m.functions[2].callable_contract.as_mut().unwrap().roles[0]
                        .life
                        .as_mut()
                        .unwrap();
                    role.bindings[1].binder = StorageBinder::Fixed(0);
                    role.classes[1].binder = StorageBinder::Fixed(1);
                }
                3 => m.functions[2].storage_life = None,
                4 => {
                    // Same physical types, but actual helper argument is the
                    // other header class. A declared role is not permission.
                    if let InstructionKind::Call { arguments, .. } = &mut m.functions[1].blocks[0]
                        .instructions
                        .last_mut()
                        .unwrap()
                        .kind
                    {
                        arguments[1] = ValueId(1);
                    }
                }
                5 => {
                    // Moving a receiver preserves its fixed domain. A helper
                    // cannot choose another parameter's class after TakeLocal.
                    m = fixture::captured_fixed_helper(zil_ir::CallableKind::FnOnce, false);
                    let helper = &mut m.functions[2];
                    let tag = helper.locals[1].ty.clone();
                    let mut local = helper.locals[1].clone();
                    local.id = LocalId(2);
                    helper.locals.push(local);
                    helper.parameters.push(LocalId(2));
                    helper.values.push(tag);
                    let c = helper.storage_life.as_mut().unwrap();
                    c.fixed_binder_count += 1;
                    c.invocation_binder_count += 1;
                    c.bindings.push(StorageLifeBinding {
                        position: StoragePosition {
                            base: StorageBase::Parameter(2),
                            path: vec![],
                        },
                        binder: StorageBinder::Invocation(1),
                    });
                    let mut row = c.classes.last().unwrap().clone();
                    row.position.base = StorageBase::Parameter(2);
                    row.binder = StorageBinder::Fixed(2);
                    c.classes.push(row);
                    helper.blocks[0].instructions.insert(
                        2,
                        Instruction {
                            result: Some(ValueId(3)),
                            kind: InstructionKind::Load(LocalId(2)),
                            span: helper.span,
                        },
                    );
                    if let InstructionKind::CallClosureOnce { arguments, .. } =
                        &mut helper.blocks[0].instructions.last_mut().unwrap().kind
                    {
                        arguments[0] = ValueId(3);
                    }
                    if let InstructionKind::Call { arguments, .. } = &mut m.functions[1].blocks[0]
                        .instructions
                        .last_mut()
                        .unwrap()
                        .kind
                    {
                        arguments.push(ValueId(1));
                    }
                }
                6 => {
                    m = fixture::captured_fixed(zil_ir::CallableKind::Fn, false);
                    m.closures[0].kind = zil_ir::CallableKind::Fn;
                    m.functions[1].blocks[0]
                        .instructions
                        .last_mut()
                        .unwrap()
                        .kind = InstructionKind::CallClosureOnce {
                        callee: ValueId(2),
                        arguments: vec![ValueId(0)],
                    };
                }
                7 => {
                    // A return of the actual environment cannot erase its
                    // held targets through an ordinary function result.
                    m = fixture::captured_fixed(zil_ir::CallableKind::Fn, false);
                    m.functions[1].return_type = Type::Closure(zil_ir::ClosureId(0));
                    m.functions[1].blocks[0].terminator = Terminator::Return(Some(ValueId(2)));
                }
                _ => {
                    // Extra rows must not smuggle a second Capture occurrence.
                    let d = m.closures[0].storage_life.as_mut().unwrap();
                    d.classes.push(d.classes[1].clone());
                    m.functions[0].storage_life = Some(d.clone());
                }
            }
            let c = captured_candidate(&m);
            let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
            for result in [
                super::super::verify_module(m.clone(), c, &m.target, &Limits::default()),
                super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()),
            ] {
                if variant == 6 {
                    // The actual finite nonlending Fn entry may be consumed
                    // through Once. Its environment still settles once.
                    result.unwrap();
                    continue;
                }
                let error = result.unwrap_err();
                let expected = match variant {
                    0 => "entry and definition relation differ",
                    1 | 7 | 8 => "signature class relation",
                    2 => "actual capture mapping",
                    3 => "requires complete class relation",
                    4 | 5 => "class equality",
                    _ => unreachable!(),
                };
                assert!(
                    error.iter().any(|d| d.message.contains(expected)),
                    "variant{variant}: {error:?}"
                );
                if matches!(variant, 4 | 5) {
                    assert!(
                        error.iter().any(|d| d.message.contains("class equality")),
                        "variant{variant}: {error:?}"
                    );
                }
            }
        }
    }
    #[test]
    fn capture_grounded_fixed_reborrow_scalar_mutation_and_invocations() {
        // Reborrowed capture must recover physical stored field targets from
        // the actual address, not rely on a copied SSA leaf cache.
        let mut reborrow = fixture::captured_fixed(zil_ir::CallableKind::Fn, false);
        let caller = &mut reborrow.functions[1];
        caller.values.push(caller.values[0].clone());
        caller.blocks[0].instructions.insert(
            2,
            Instruction {
                result: Some(ValueId(4)),
                kind: InstructionKind::ReborrowAddress {
                    address: ValueId(0),
                    mutable: false,
                },
                span: caller.span,
            },
        );
        if let InstructionKind::MakeClosure { captures, .. } =
            &mut caller.blocks[0].instructions[3].kind
        {
            captures[0] = ValueId(4);
        }
        // Scalar FnMut environment writes remain ordinary exclusive access.
        let mut mutation = fixture::captured_fixed(zil_ir::CallableKind::FnMut, false);
        mutation.closures[0].captures.push(Type::I64);
        let caller = &mut mutation.functions[1];
        caller.values.push(Type::I64);
        caller.blocks[0].instructions.insert(
            2,
            Instruction {
                result: Some(ValueId(4)),
                kind: InstructionKind::Constant(Constant::I64(0)),
                span: caller.span,
            },
        );
        if let InstructionKind::MakeClosure { captures, .. } =
            &mut caller.blocks[0].instructions[3].kind
        {
            captures.push(ValueId(4));
        }
        let entry = &mut mutation.functions[0];
        entry.blocks[0].instructions.insert(
            3,
            Instruction {
                result: None,
                kind: InstructionKind::WriteCapture {
                    index: 1,
                    value: ValueId(2),
                },
                span: entry.span,
            },
        );
        // Different outer invocation borrows can share a genuine inner class.
        let mut repeated = fixture::captured_fixed(zil_ir::CallableKind::Fn, false);
        let caller = &mut repeated.functions[1];
        let header = caller.storage_life.as_mut().unwrap();
        header.fixed_binder_count = 1;
        header.classes[1].binder = StorageBinder::Fixed(0);
        caller.values.push(Type::I64);
        caller.blocks[0].instructions.push(Instruction {
            result: Some(ValueId(4)),
            kind: InstructionKind::CallClosureShared {
                callee: LocalId(2),
                arguments: vec![ValueId(1)],
            },
            span: caller.span,
        });
        caller.blocks[0].terminator = Terminator::Return(Some(ValueId(4)));
        // Outer capture Address alone grounds a Fixed, without a nominal
        // qualification. The same real target/loan reconstruction still runs.
        let mut ordinary = fixture::captured_fixed(zil_ir::CallableKind::Fn, false);
        let shared = Type::Address {
            mutable: false,
            inner: Box::new(Type::I64),
        };
        ordinary.closures[0].captures[0] = shared.clone();
        ordinary.closures[0].parameters[0] = shared.clone();
        let life = ordinary.closures[0].storage_life.as_mut().unwrap();
        life.fixed_binder_count = 1;
        life.classes.clear();
        life.bindings[1].binder = StorageBinder::Fixed(0);
        let entry = &mut ordinary.functions[0];
        entry.locals[0].ty = shared.clone();
        entry.values[0] = shared.clone();
        entry.values[3] = shared.clone();
        entry.storage_life = Some(life.clone());
        entry.blocks[0].instructions[1].kind = InstructionKind::ReborrowAddress {
            address: ValueId(0),
            mutable: false,
        };
        entry.blocks[0].instructions[4].kind = InstructionKind::ReborrowAddress {
            address: ValueId(3),
            mutable: false,
        };
        let caller = &mut ordinary.functions[1];
        for index in 0..2 {
            caller.locals[index].ty = shared.clone();
            caller.values[index] = shared.clone();
        }
        let header = caller.storage_life.as_mut().unwrap();
        header.fixed_binder_count = 0;
        header.classes.clear();
        for m in [reborrow, mutation, repeated, ordinary] {
            let c = captured_candidate(&m);
            let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
            super::super::verify_module(m.clone(), c, &m.target, &Limits::default()).unwrap();
            super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()).unwrap();
        }
    }
    #[test]
    fn capture_grounded_fixed_uses_one_request_budget_exactly() {
        let m = fixture::captured_fixed_helper(zil_ir::CallableKind::FnMut, false);
        let c = captured_candidate(&m);
        let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
        let check = |n, wire| {
            let limits = Limits {
                max_analysis_steps: n,
                ..Limits::default()
            };
            if wire {
                super::super::verify_module_bytes(&bytes, &m.target, &limits)
            } else {
                super::super::verify_module(m.clone(), c.clone(), &m.target, &limits)
            }
        };
        for wire in [false, true] {
            let expected = check(Limits::default().max_analysis_steps, wire)
                .unwrap()
                .fingerprint();
            let mut low = 0;
            let mut high = Limits::default().max_analysis_steps;
            while low < high {
                let mid = low + (high - low) / 2;
                if check(mid, wire).is_ok() {
                    high = mid;
                } else {
                    low = mid + 1;
                }
            }
            let error = check(low - 1, wire).unwrap_err();
            assert!(
                error.iter().any(|d| d.code == "E-RESOURCE-LIMIT"),
                "{error:?}"
            );
            assert_eq!(check(low, wire).unwrap().fingerprint(), expected);
            assert_eq!(check(low, wire).unwrap().fingerprint(), expected);
            assert!(check(low - 1, wire).is_err());
            assert_eq!(check(low, wire).unwrap().fingerprint(), expected);
            println!("capture helper Verify wire={wire} N={low}; N-1/fresh/warm/retry passed");
        }
    }
    #[test]
    fn capture_grounded_fixed_candidate_cannot_replace_actual_capture_roots() {
        let m = fixture::captured_fixed(zil_ir::CallableKind::Fn, false);
        for forged in [vec![], vec![HoldRoot::Parameter(1)]] {
            let mut c = captured_candidate(&m);
            c.functions[1].holds[2].roots = forged;
            let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
            for result in [
                super::super::verify_module(m.clone(), c, &m.target, &Limits::default()),
                super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()),
            ] {
                let error = result.unwrap_err();
                assert!(
                    error.iter().any(|d| d.code == "E-VERIFY-STORED"
                        && d.message
                            .contains("independently reconstructed stored target roots")),
                    "{error:?}"
                );
            }
        }
    }
    #[test]
    fn capture_grounded_fixed_once_still_consumes_actual_owner() {
        for duplicate_call in [true, false] {
            let mut m = fixture::captured_fixed(zil_ir::CallableKind::FnOnce, false);
            let f = &mut m.functions[1];
            if duplicate_call {
                f.values.push(Type::I64);
                f.blocks[0].instructions.push(Instruction {
                    result: Some(ValueId(4)),
                    kind: InstructionKind::CallClosureOnce {
                        callee: ValueId(2),
                        arguments: vec![ValueId(0)],
                    },
                    span: f.span,
                });
                f.blocks[0].terminator = Terminator::Return(Some(ValueId(4)));
            } else {
                f.values.push(Type::Closure(zil_ir::ClosureId(0)));
                f.blocks[0].instructions.insert(
                    3,
                    Instruction {
                        result: None,
                        kind: InstructionKind::Store {
                            local: LocalId(2),
                            value: ValueId(2),
                        },
                        span: f.span,
                    },
                );
                f.blocks[0].instructions.insert(
                    4,
                    Instruction {
                        result: Some(ValueId(4)),
                        kind: InstructionKind::Load(LocalId(2)),
                        span: f.span,
                    },
                );
                if let InstructionKind::CallClosureOnce { callee, .. } =
                    &mut f.blocks[0].instructions.last_mut().unwrap().kind
                {
                    *callee = ValueId(4);
                }
            }
            let c = captured_candidate(&m);
            let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
            for result in [
                super::super::verify_module(m.clone(), c, &m.target, &Limits::default()),
                super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()),
            ] {
                let error = result.unwrap_err();
                assert!(
                    error.iter().any(|d| d.code == "E-VERIFY-RESOURCE"),
                    "{error:?}"
                );
            }
        }
    }
    #[test]
    fn capture_grounded_fixed_does_not_guess_local_phantom_classes() {
        let mut m = fixture::captured_fixed_parent_end(false);
        let Type::BorrowedStorage { storage, life } = &mut m.life_instances[0].ty else {
            unreachable!()
        };
        **storage = Type::Record {
            name: "View".into(),
            fields: vec![("number".into(), Type::I64)],
        };
        life.slots.clear();
        let phantom = m.life_instances[0].ty.clone();
        let receiver = Type::Address {
            mutable: false,
            inner: Box::new(phantom.clone()),
        };
        m.closures[0].captures[0] = receiver.clone();
        m.closures[0].parameters[0] = receiver.clone();
        let entry = &mut m.functions[0];
        entry.locals[0].ty = receiver.clone();
        entry.values = vec![Type::I64];
        entry.blocks[0].instructions = vec![fixture::instruction(
            Some(0),
            InstructionKind::Constant(Constant::I64(0)),
        )];
        entry.blocks[0].terminator = Terminator::Return(Some(ValueId(0)));
        let caller = &mut m.functions[1];
        caller.locals[1].ty = phantom.clone();
        caller.values[2] = phantom;
        caller.values[3] = receiver;
        if let InstructionKind::MakeAggregate { fields } =
            &mut caller.blocks[0].instructions[3].kind
        {
            fields[0] = ValueId(0);
        }
        let c = captured_candidate(&m);
        let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
        for result in [
            super::super::verify_module(m.clone(), c, &m.target, &Limits::default()),
            super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()),
        ] {
            let error = result.unwrap_err();
            assert!(
                error.iter().any(|d| d
                    .message
                    .contains("no independently established source class")),
                "{error:?}"
            );
        }
    }
    #[test]
    fn owning_fixed_true_once_has_real_description_roots_and_resource_check() {
        for wrong in [false, true] {
            for relay in [false, true] {
                let mut m = fixture::captured_owned_fixed(wrong);
                if relay {
                    let f = &mut m.functions[1];
                    let owner = f.values[5].clone();
                    let id = f.values.len() as u32;
                    f.values.push(owner.clone());
                    let local = LocalId(f.locals.len() as u32);
                    f.locals.push(LocalDecl {
                        id: local,
                        name: "mark".into(),
                        ty: owner,
                        mutable: false,
                        span: f.span,
                    });
                    let at = f.blocks[0]
                        .instructions
                        .iter()
                        .position(|i| matches!(i.kind, InstructionKind::MakeClosure { .. }))
                        .unwrap();
                    f.blocks[0].instructions.splice(
                        at..at,
                        [
                            fixture::instruction(
                                None,
                                InstructionKind::Store {
                                    local,
                                    value: ValueId(5),
                                },
                            ),
                            fixture::instruction(Some(id), InstructionKind::TakeLocal { local }),
                        ],
                    );
                    if let InstructionKind::MakeClosure { captures, .. } =
                        &mut f.blocks[0].instructions[at + 2].kind
                    {
                        captures[0] = ValueId(id);
                    }
                }
                let c = captured_candidate(&m);
                let bytes =
                    zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
                for result in [
                    super::super::verify_module(m.clone(), c, &m.target, &Limits::default()),
                    super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()),
                ] {
                    if wrong {
                        let error = result.unwrap_err();
                        assert!(
                            error.iter().any(|d| d.message.contains("class equality")),
                            "{error:?}"
                        );
                    } else {
                        result.unwrap();
                    }
                }
            }
        }
    }
    #[test]
    fn owning_fixed_actual_owner_epochs_and_metadata_never_come_from_classes() {
        for case in 0..14 {
            let mut m = fixture::captured_owned_fixed(false);
            match case {
                0 => {
                    let f = &mut m.functions[0];
                    let id = f.values.len() as u32;
                    f.values.push(f.values[7].clone());
                    f.blocks[0].instructions.insert(
                        1,
                        fixture::instruction(Some(id), InstructionKind::TakeCapture { index: 0 }),
                    );
                }
                1 => {
                    let f = &mut m.functions[1];
                    let id = f.values.len() as u32;
                    f.values.push(Type::I64);
                    f.blocks[0].instructions.push(fixture::instruction(
                        Some(id),
                        InstructionKind::CallClosureOnce {
                            callee: ValueId(2),
                            arguments: vec![ValueId(0)],
                        },
                    ));
                    f.blocks[0].terminator = Terminator::Return(Some(ValueId(id)));
                }
                2 | 3 => {
                    let code = &mut m.functions[0].blocks[0].instructions;
                    let drop = code
                        .iter()
                        .position(|i| matches!(i.kind, InstructionKind::DropManaged { .. }))
                        .unwrap();
                    let i = code.remove(drop);
                    let at = code
                        .iter()
                        .position(|i| {
                            if case == 2 {
                                matches!(i.kind, InstructionKind::ReadDesc { .. })
                            } else {
                                matches!(i.kind, InstructionKind::DescribeLocal { .. })
                            }
                        })
                        .unwrap();
                    code.insert(at, i);
                }
                4 => {
                    let f = &mut m.functions[1];
                    if let InstructionKind::MakeManaged { fields } = &mut f.blocks[0]
                        .instructions
                        .iter_mut()
                        .find(|i| matches!(i.kind, InstructionKind::MakeManaged { .. }))
                        .unwrap()
                        .kind
                    {
                        fields[0] = ValueId(0);
                    }
                }
                5 => {
                    m.functions[0].values[8] = m.functions[0].values[7].clone();
                }
                6 => {
                    let f = &mut m.functions[0];
                    if let InstructionKind::ReadDesc { index, .. } = &mut f.blocks[0]
                        .instructions
                        .iter_mut()
                        .find(|i| matches!(i.kind, InstructionKind::ReadDesc { .. }))
                        .unwrap()
                        .kind
                    {
                        *index = 99;
                    }
                }
                7 => {
                    let f = &mut m.functions[0];
                    let id = f.values.len() as u32;
                    f.values.push(Type::I32);
                    f.blocks[0]
                        .instructions
                        .iter_mut()
                        .find(|i| matches!(i.kind, InstructionKind::DropManaged { .. }))
                        .unwrap()
                        .result = Some(ValueId(id));
                }
                8 => {
                    m.resources.push(ResourceDefinition {
                        name: "Other".into(),
                        payload: Type::Record {
                            name: "Other".into(),
                            fields: vec![("n".into(), Type::I32)],
                        },
                        teardown: None,
                    });
                }
                9 => {
                    let f = &mut m.functions[0];
                    let id = f.values.len() as u32;
                    f.values.push(Type::Address {
                        mutable: false,
                        inner: Box::new(f.values[7].clone()),
                    });
                    // A real well-typed but unused owner-head address must not
                    // bypass the finite profile through Copy-value fallback.
                    f.blocks[0].instructions.insert(
                        2,
                        fixture::instruction(
                            Some(id),
                            InstructionKind::BorrowLocal { local: LocalId(1) },
                        ),
                    );
                }
                10 => {
                    m.closures[0].kind = zil_ir::CallableKind::Fn;
                }
                11 => {
                    m.resources[0].name = "Other".into();
                    if let Type::Record { name, .. } = &mut m.resources[0].payload {
                        *name = "Other".into();
                    }
                }
                12 => {
                    m.resources.clear();
                }
                _ => {
                    m.resources[0].teardown = Some(FunctionId(0));
                }
            }
            let c = captured_candidate(&m);
            let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
            for result in [
                super::super::verify_module(m.clone(), c, &m.target, &Limits::default()),
                super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()),
            ] {
                let error = result.unwrap_err();
                if case <= 2 {
                    assert!(
                        error.iter().any(|d| d.code == "E-VERIFY-RESOURCE"),
                        "case{case}: {error:?}"
                    );
                } else if case == 3 {
                    assert!(
                        error.iter().any(|d| d.code == "E-VERIFY-INITIALIZATION"),
                        "case{case}: {error:?}"
                    );
                } else if case <= 7 {
                    assert!(
                        error.iter().any(|d| d.code == "E-VERIFY-STORED-OWNER"),
                        "case{case}: {error:?}"
                    );
                } else if case == 10 {
                    assert!(
                        error.iter().any(|d| d.code == "E-VERIFY-STORED-OWNER"
                            && d.message.contains("actual consuming FnOnce entry")),
                        "case{case}: {error:?}"
                    );
                } else if case >= 11 {
                    assert!(
                        error.iter().any(|d| d.code == "E-VERIFY-RESOURCE"),
                        "case{case}: {error:?}"
                    );
                } else {
                    assert!(
                        error
                            .iter()
                            .any(|d| d.category == zil_base::ResultCategory::Unsupported),
                        "case{case}: {error:?}"
                    );
                }
            }
        }
    }
    #[test]
    fn owning_fixed_owner_work_is_prepaid_and_actual_description_claims_are_checked() {
        let m = fixture::captured_owned_fixed(false);
        let c = captured_candidate(&m);
        let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
        let check = |work, wire| {
            let limits = Limits {
                max_analysis_steps: work,
                ..Limits::default()
            };
            if wire {
                super::super::verify_module_bytes(&bytes, &m.target, &limits)
            } else {
                super::super::verify_module(m.clone(), c.clone(), &m.target, &limits)
            }
        };
        for wire in [false, true] {
            let expected = check(Limits::default().max_analysis_steps, wire)
                .unwrap()
                .fingerprint();
            let (mut low, mut high) = (1, Limits::default().max_analysis_steps);
            while low < high {
                let mid = low + (high - low) / 2;
                if check(mid, wire).is_ok() {
                    high = mid
                } else {
                    low = mid + 1
                }
            }
            let error = check(low - 1, wire).unwrap_err();
            assert!(
                error.iter().any(|d| d.code == "E-RESOURCE-LIMIT"),
                "{error:?}"
            );
            assert_eq!(check(low, wire).unwrap().fingerprint(), expected);
            assert_eq!(check(low, wire).unwrap().fingerprint(), expected);
            assert!(check(low - 1, wire).is_err());
            assert_eq!(check(low, wire).unwrap().fingerprint(), expected);
            println!("owning Fixed Verify wire={wire} N={low}; N-1/fresh/repeated/retry");
        }
        let mut forged = c;
        forged.functions[0].holds[8].roots.clear();
        let bytes = zil_ir::wire::encode_module_candidate(&m, &forged, &Limits::default()).unwrap();
        for result in [
            super::super::verify_module(m.clone(), forged, &m.target, &Limits::default()),
            super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()),
        ] {
            let error = result.unwrap_err();
            assert!(
                error.iter().any(|d| d.code == "E-VERIFY-STORED"
                    && d.message
                        .contains("independently reconstructed stored target roots")),
                "{error:?}"
            );
        }
    }
    #[test]
    fn two_rich_roles_consume_one_verification_ledger() {
        let m = fixture::qualified_two_fn_roles(false);
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
        println!("342 qualified Fn role Verify N={low}; N-1 ResourceLimit/fresh N identical");
    }
    #[test]
    fn qualified_callback_exclusive_alias_fails_even_with_valid_class_equations() {
        let m = fixture::qualified_fn_exclusive_call(false);
        let c = fixture::class_candidate(&m);
        let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
        super::super::verify_module(m.clone(), c, &m.target, &Limits::default()).unwrap();
        super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()).unwrap();
        let m = fixture::qualified_fn_exclusive_call(true);
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
    fn two_parameter_roles_with_one_closure_keep_distinct_rich_relations() {
        let m = fixture::qualified_two_fn_roles(false);
        let c = fixture::class_candidate(&m);
        let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
        super::super::verify_module(m.clone(), c, &m.target, &Limits::default()).unwrap();
        super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()).unwrap();
        let m = fixture::qualified_two_fn_roles(true);
        let c = fixture::class_candidate(&m);
        let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
        let error =
            super::super::verify_module(m.clone(), c, &m.target, &Limits::default()).unwrap_err();
        assert!(
            error.iter().any(|d| d.message.contains("class equality")),
            "{error:?}"
        );
        let error =
            super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()).unwrap_err();
        assert!(
            error.iter().any(|d| d.message.contains("class equality")),
            "{error:?}"
        );
    }
    #[test]
    fn qualified_fn_role_class_equations_are_checked_in_raw_and_wire() {
        let m = fixture::qualified_fn_role(false, false);
        let c = fixture::class_candidate(&m);
        let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
        super::super::verify_module(m.clone(), c, &m.target, &Limits::default()).unwrap();
        super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()).unwrap();
        for m in [
            fixture::qualified_fn_role(true, false),
            fixture::qualified_fn_role(false, true),
        ] {
            let c = fixture::class_candidate(&m);
            let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
            let error = super::super::verify_module(m.clone(), c, &m.target, &Limits::default())
                .unwrap_err();
            assert!(
                error.iter().any(|d| d.message.contains("class equality")),
                "{error:?}"
            );
            let error = super::super::verify_module_bytes(&bytes, &m.target, &Limits::default())
                .unwrap_err();
            assert!(
                error.iter().any(|d| d.message.contains("class equality")),
                "{error:?}"
            );
        }
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
            header(module, f, &mut budget)?;
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
