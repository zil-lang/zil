//! Independent stored-target reconstruction. Sorted vectors and batch CFG
//! solving deliberately do not consume analyzer roots, summaries, or claims.
use super::*;
#[path = "stored_flow_pure_copy.rs"]
mod pure_copy;

pub(super) fn qualified_function(f: &TypedFunction, b: &mut Budget) -> Check<bool> {
    // A plain address-returning helper can project the caller's actual stored
    // pointee even when its own signature contains no qualified carrier.
    if matches!(f.return_type, Type::Address { .. }) {
        return Ok(true);
    }

    b.spend(
        f.locals
            .len()
            .saturating_add(f.values.len())
            .saturating_add(1),
    )?;
    let mut pending: Vec<&Type> = f
        .locals
        .iter()
        .map(|local| &local.ty)
        .chain(&f.values)
        .chain(std::iter::once(&f.return_type))
        .collect();
    while let Some(ty) = pending.pop() {
        b.spend(1)?;
        match ty {
            // Their actual capture/retained declarations can carry targets
            // outside this type spelling. Keep their functions in the proof.
            Type::BorrowedStorage { .. }
            | Type::Closure(_)
            | Type::Task { .. }
            | Type::BorrowedFunctionPointer { .. } => return Ok(true),
            Type::Address { inner, .. }
            | Type::Managed(inner)
            | Type::Desc(inner)
            | Type::OwnedStore { element: inner }
            | Type::Array { element: inner, .. }
            | Type::Slice { element: inner }
            | Type::Opaque { hidden: inner, .. } => {
                b.spend(1)?;
                pending.push(inner);
            }
            Type::Package { fields, .. } | Type::Record { fields, .. } => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, child)| child));
            }
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                pending.extend(fields);
            }
            Type::Enum { variants, .. } => {
                b.spend(variants.len())?;
                for (_, fields) in variants {
                    b.spend(fields.len())?;
                    pending.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                b.spend(2)?;
                pending.extend([error.as_ref(), result.as_ref()]);
            }
            _ => {}
        }
    }
    Ok(false)
}

pub(super) mod enum_address;
mod nominal;
pub(super) mod option_address;
mod retained;
mod task_address;

type Path = Vec<PlaceProjection>;
type WriteSites = Vec<(usize, u32, Path, usize, bool)>;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Source {
    pub(super) retained_port: Option<u32>,
    pub(super) base: HoldRoot,
    pub(super) leaf: Path,
    pub(super) inline: Path,
    pub(super) heap_element: Option<Path>,
}
#[derive(Clone, Debug, Default)]
pub(super) struct StoreOptionFunctionSources {
    pub(super) values: Vec<Vec<Source>>,
}
pub(super) fn option_sources_snapshot(
    function: FunctionId,
    value: ValueId,
    budget: &mut Budget,
) -> Check<Vec<Source>> {
    fn lookup(b: &Budget, function: FunctionId, value: ValueId) -> Option<&Vec<Source>> {
        b.store_option_sources
            .as_ref()
            .and_then(|rows| rows.get(function.0 as usize))
            .and_then(|function| function.values.get(value.0 as usize))
    }
    let count = lookup(budget, function, value)
        .ok_or_else(|| err("Option value lacks actual stored source proof"))?
        .len();
    budget.spend(count)?;
    let cost = lookup(budget, function, value)
        .unwrap()
        .iter()
        .fold(0usize, |cost, source| cost.saturating_add(width(source)));
    budget.spend(cost)?;
    Ok(lookup(budget, function, value).unwrap().clone())
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Permission {
    sources: Vec<Source>,
    loans: Vec<usize>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Payload {
    enum_variants: enum_address::Variants,
    option_variants: option_address::Variants,
    address: Permission,
    store_description: Permission,
    task_ports: Vec<(Vec<ContractProjection>, Permission)>,
    creation_complete: Option<bool>,
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
    /// Union of paths left vacant by any incoming branch.
    vacant: Vec<Source>,
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
    task_ports: Vec<(Vec<ContractProjection>, Vec<Source>)>,
    external_after: Vec<(u32, Path, Vec<Source>)>,
    apart: Vec<(Source, Source)>,
    complete: bool,
    returned_enum_variants: enum_address::Variants,
    enum_values: Vec<(ValueId, enum_address::Variants)>,
    returned_variants: option_address::Variants,
    none_values: Vec<ValueId>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum TaskLoanAnchor {
    Value(ValueId),
    Input(zil_ir::StoragePosition),
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct TaskPointerRoot {
    pub(super) base: HoldRoot,
    pub(super) port: Option<u32>,
}
#[derive(Clone, Debug, Default)]
pub(super) struct TaskValueSources {
    pub(super) pointer: Vec<TaskPointerRoot>,
    pub(super) public: Vec<HoldRoot>,
    pub(super) retained: Vec<(Vec<ContractProjection>, Vec<TaskPointerRoot>)>,
    pub(super) pointer_parents: Vec<TaskLoanAnchor>,
    pub(super) payload_parents: Vec<TaskLoanAnchor>,
}
#[derive(Clone, Debug, Default)]
pub(super) struct TaskFunctionSources {
    /// False denotes an ordinary function omitted from stored solving.
    pub(super) covered: bool,
    pub(super) values: Vec<TaskValueSources>,
}
struct Solution {
    values: Vec<Payload>,
    outputs: Vec<Frame>,
    entry: Frame,
    loans: Vec<Loan>,
    sites: WriteSites,
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
        .saturating_add(s.heap_element.as_ref().map_or(0, Vec::len))
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
    p.enum_variants.work(b)?;
    p.option_variants.work(b)?;
    permission_work(&p.address, b)?;
    permission_work(&p.store_description, b)?;
    task_address::work(p, b)?;
    b.spend(p.leaves.len())?;
    for (path, p) in &p.leaves {
        b.spend(path.len())?;
        permission_work(p, b)?;
    }
    Ok(())
}
fn frame_work(f: &Frame, b: &mut Budget) -> Check<()> {
    source_work(&f.vacant, b)?;
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
fn take_package_field_leaves(holder: &mut Payload, index: u32, b: &mut Budget) -> Check<Payload> {
    b.spend(holder.leaves.len())?;
    let mut result = Payload::default();
    let mut retained = Vec::new();
    for (path, permission) in std::mem::take(&mut holder.leaves) {
        b.spend(path.len().saturating_add(1))?;
        if path.first() == Some(&PlaceProjection::Field(index)) {
            result.leaves.push((path[1..].to_vec(), permission));
        } else {
            retained.push((path, permission));
        }
    }
    holder.leaves = retained;
    Ok(result)
}
fn join_payload(to: &mut Payload, from: &Payload, b: &mut Budget) -> Check<bool> {
    let mut changed = join_permission(&mut to.address, &from.address, b)?;
    // Only the Option profile creates this private description ledger.
    if b.store_option_results {
        changed |= join_permission(&mut to.store_description, &from.store_description, b)?;
    }
    changed |= task_address::join(to, from, b)?;
    if let Some(complete) = from.creation_complete {
        let joined = Some(to.creation_complete.unwrap_or(false) || complete);
        changed |= to.creation_complete != joined;
        to.creation_complete = joined;
    }
    changed |= to.enum_variants.join(&from.enum_variants, b)?;
    changed |= to.option_variants.join(&from.option_variants, b)?;
    for (path, p) in &from.leaves {
        changed |= add_leaf(to, path, p, b)?;
    }
    Ok(changed)
}
fn join_frame(to: &mut Frame, from: &Frame, b: &mut Budget) -> Check<()> {
    add_sources(&mut to.vacant, &from.vacant, b)?;
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
fn exclusive_carrier_value(ty: &Type) -> bool {
    match ty {
        Type::BorrowedStorage { storage, life } => {
            life.slots
                .iter()
                .any(|slot| slot.mode == StoredBorrowMode::ExclusiveAddress)
                || exclusive_carrier_value(storage)
        }
        Type::Tuple(fields) => fields.iter().any(exclusive_carrier_value),
        Type::Record { fields, .. } | Type::Package { fields, .. } => fields
            .iter()
            .any(|(_, field)| exclusive_carrier_value(field)),
        Type::Array { element, .. } => exclusive_carrier_value(element),
        Type::Enum { variants, .. } => variants
            .iter()
            .any(|(_, fields)| fields.iter().any(exclusive_carrier_value)),
        Type::ErrorUnion { error, result } => {
            exclusive_carrier_value(error) || exclusive_carrier_value(result)
        }
        _ => false,
    }
}
fn affine_stored_input(ty: &Type) -> bool {
    match ty {
        Type::Address { mutable: true, .. } => true,
        Type::Tuple(fields) => fields.iter().any(affine_stored_input),
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            fields.iter().any(|(_, field)| affine_stored_input(field))
        }
        Type::Array { element, .. } => affine_stored_input(element),
        Type::Enum { variants, .. } => variants
            .iter()
            .any(|(_, fields)| fields.iter().any(affine_stored_input)),
        Type::ErrorUnion { error, result } => {
            affine_stored_input(error) || affine_stored_input(result)
        }
        _ => exclusive_carrier_value(ty),
    }
}
// Address arguments to ordinary calls are reborrows. Only actual storage
// transfers consume that SSA address; carrier values retain their move rules.
fn transfers_address(kind: &InstructionKind) -> bool {
    matches!(
        kind,
        InstructionKind::Store { .. }
            | InstructionKind::MakeAggregate { .. }
            | InstructionKind::MakePackage { .. }
            | InstructionKind::MakeManaged { .. }
            | InstructionKind::MakeOwnedTuple { .. }
            | InstructionKind::MakeVariant { .. }
            | InstructionKind::MakeOwnedArray { .. }
            | InstructionKind::MakeClosure { .. }
            | InstructionKind::MakeErrorUnion { .. }
            | InstructionKind::ExtractField { .. }
            | InstructionKind::ExtractIndex { .. }
            | InstructionKind::UpdateField { .. }
            | InstructionKind::UpdateIndex { .. }
            | InstructionKind::StorePackageField { .. }
            | InstructionKind::StoreOwnedArrayElement { .. }
            | InstructionKind::StoreStoredBorrow { .. }
    )
}
fn exclusive_consumers(kind: &InstructionKind) -> (Option<ValueId>, &[ValueId]) {
    match kind {
        InstructionKind::Store { value, .. }
        | InstructionKind::StorePackageField { value, .. }
        | InstructionKind::StoreOwnedArrayElement { value, .. }
        | InstructionKind::StoreStoredBorrow { value, .. }
        | InstructionKind::MakeErrorUnion { value, .. }
        | InstructionKind::ExtractField {
            aggregate: value, ..
        }
        | InstructionKind::ExtractIndex {
            aggregate: value, ..
        } => (Some(*value), &[]),
        InstructionKind::UpdateField {
            aggregate, value, ..
        }
        | InstructionKind::UpdateIndex {
            aggregate, value, ..
        } => (Some(*aggregate), std::slice::from_ref(value)),
        InstructionKind::MakeAggregate { fields }
        | InstructionKind::MakePackage { fields }
        | InstructionKind::MakeManaged { fields }
        | InstructionKind::MakeOwnedTuple { fields }
        | InstructionKind::MakeVariant { fields, .. }
        | InstructionKind::MakeOwnedArray { elements: fields }
        | InstructionKind::MakeClosure {
            captures: fields, ..
        }
        | InstructionKind::Call {
            arguments: fields, ..
        }
        | InstructionKind::CallIndirect {
            arguments: fields, ..
        }
        | InstructionKind::CallDynSlot {
            arguments: fields, ..
        }
        | InstructionKind::CallClosure {
            arguments: fields, ..
        }
        | InstructionKind::CallClosureOnce {
            arguments: fields, ..
        }
        | InstructionKind::CallClosureBorrowed {
            arguments: fields, ..
        }
        | InstructionKind::CallClosureShared {
            arguments: fields, ..
        }
        | InstructionKind::CallClosureMut {
            arguments: fields, ..
        }
        | InstructionKind::ForeignCall {
            arguments: fields, ..
        }
        | InstructionKind::SuspendCall {
            arguments: fields, ..
        }
        | InstructionKind::TaskCreate {
            arguments: fields, ..
        } => (None, fields),
        _ => (None, &[]),
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
            Type::Task { .. } => super::tasks::charge_result_context(t, b)?,
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
    if enum_address::shape(ty, limits, b)? {
        return Ok(true);
    }
    if !matches!(
        storage,
        Type::Record { .. } | Type::Package { .. } | Type::Tuple(_) | Type::Enum { .. }
    ) || (matches!(storage, Type::Enum { .. })
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
                    None,
                ));
            }
            Type::Record { name, fields } | Type::Package { name, fields } => {
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
            Type::BorrowedStorage {
                storage: child,
                life,
            } => {
                if !matches!(storage, Type::Package { .. })
                    || !matches!(child.as_ref(), Type::Package { .. })
                {
                    if !retained::pure(t, b)? {
                        return Err(err("nested physical child requires owning Package"));
                    }
                    continue;
                }
                physical(t, limits, b)?;
                let boundary: Vec<_> = path
                    .iter()
                    .copied()
                    .map(LifeTypeProjection::Place)
                    .collect();
                for slot in &life.slots {
                    b.spend(path.len().saturating_add(slot.path.len()).saturating_add(1))?;
                    let mut complete = path.clone();
                    complete.extend(&slot.path);
                    actual.push((complete, slot.mode, Some((boundary.clone(), slot.binder))));
                }
            }
            Type::Managed(payload) if matches!(storage, Type::Package { .. }) => {
                type_work(payload, limits, b)?;
                if !matches!(
                    payload.as_ref(),
                    Type::ByteBuffer | Type::Record { .. } | Type::OwnedStore { .. }
                ) {
                    return Err(err(
                        "stored Package actual owner is outside finite resource profile",
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
    for ((path, mode, child_class), row) in actual.iter().zip(&shape.slots) {
        b.spend(path.len().saturating_add(row.path.len()).saturating_add(1))?;
        if path != &row.path || mode != &row.mode {
            return Err(err(
                "actual physical stored path or mode differs from declared leaf",
            ));
        }
        b.stored_exclusive_shapes |= *mode == StoredBorrowMode::ExclusiveAddress;
        if let Some((boundary, class)) = child_class
            && !shape.retained.iter().any(|entry| {
                &entry.path == boundary && entry.class == *class && entry.binder == row.binder
            })
        {
            return Err(err("nested slot binder differs from retained child class"));
        }
        if *mode == StoredBorrowMode::ExclusiveAddress
            && (!matches!(storage, Type::Record { .. } | Type::Package { .. })
                || !path
                    .iter()
                    .all(|part| matches!(part, PlaceProjection::Field(_)))
                || (child_class.is_none() && path.len() != 1))
        {
            return Err(unsupported(
                "exclusive stored holder requires direct nominal address slots",
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
    if matches!(
        storage,
        Type::Record { .. } | Type::Package { .. } | Type::Enum { .. }
    ) {
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

pub(super) fn qualifications(
    module: &TypedModule,
    limits: &Limits,
    mask: Option<&[bool]>,
    b: &mut Budget,
) -> Check<()> {
    let mixed_package = b.stored_package_instance;
    for (index, f) in module.functions.iter().enumerate() {
        if mask.is_some_and(|mask| !mask[index]) {
            continue;
        }
        for block in &f.blocks {
            for i in &block.instructions {
                b.spend(1)?;
                if let InstructionKind::Load(local) = i.kind
                    && f.locals
                        .get(local.0 as usize)
                        .is_some_and(|local| exclusive_carrier_value(&local.ty))
                {
                    return Err(err(
                        "actual exclusive stored carrier was copied without a transfer",
                    ));
                }
                let scalar_owner_operation = match &i.kind {
                    InstructionKind::ReadDesc { desc, .. } => f
                        .values
                        .get(desc.0 as usize)
                        .is_some_and(super::stored_owned::description),
                    InstructionKind::MakeManaged { .. } => i.result.is_some_and(|id| {
                        f.values
                            .get(id.0 as usize)
                            .is_some_and(super::stored_owned::owner)
                    }),
                    _ => false,
                };
                let enum_operation = enum_address::instruction(module, f, i, b)?;
                let owned_operation = ((!mixed_package
                    && !b.store_option_results
                    && super::stored_owned::instruction(f, &i.kind))
                    || scalar_owner_operation)
                    && !(b.task_borrowed_results && task_address::owner_instruction(f, &i.kind));
                if !super::class_flow::pointer_instruction(f, i)
                    && !super::stored_callables::instruction(module, f, i)
                    && !owned_operation
                    && !enum_operation
                    && !(b.task_borrowed_results && task_address::operation(&i.kind))
                    && !(b.store_option_results
                        && matches!(
                            i.kind,
                            InstructionKind::OwnedStoreNew { .. }
                                | InstructionKind::OwnedStoreReserve { .. }
                                | InstructionKind::OwnedStorePush { .. }
                                | InstructionKind::OwnedStorePop { .. }
                                | InstructionKind::OwnedStoreCapacity { .. }
                                | InstructionKind::OwnedStoreLen { .. }
                                | InstructionKind::OwnedStoreBorrow { .. }
                                | InstructionKind::DropErrorUnion { .. }
                                | InstructionKind::DescribePackageField { .. }
                        ))
                    && !pure_copy::operation(&i.kind)
                    && !pure_copy::ordinary_owned_union(f, i, b)?
                    && !pure_copy::ordinary_package_borrow(f, i, b)?
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
                            | InstructionKind::MakePackage { .. }
                            | InstructionKind::ByteBufferNew
                            | InstructionKind::ByteBufferPush { .. }
                            | InstructionKind::ByteBufferReserve { .. }
                            | InstructionKind::ByteBufferTruncate { .. }
                            | InstructionKind::DescribeAddress { .. }
                            | InstructionKind::DescribeLocal { .. }
                            | InstructionKind::DescribeCapture { .. }
                            | InstructionKind::DropPackage { .. }
                            | InstructionKind::TakePackageField { .. }
                            | InstructionKind::DropPackageField { .. }
                            | InstructionKind::StorePackageField { .. }
                            | InstructionKind::DropManaged { .. }
                            | InstructionKind::TakeLocal { .. }
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
                                    | StoredBorrowLoad::Reborrow { mutable: true }
                                    | StoredBorrowLoad::Take,
                                ..
                            }
                            | InstructionKind::StoreStoredBorrow { .. }
                    )
                {
                    return Err(unsupported(
                        "instruction is outside the checked stored profile, including unreachable blocks",
                        Some(i.span),
                    ));
                }
                if owned_operation {
                    super::stored_owned::types(module, f, i, b)?;
                }
                if super::class_flow::pointer_instruction(f, i) {
                    super::class_flow::pointer_types(module, f, i, b)?;
                }
                if super::stored_callables::instruction(module, f, i) {
                    super::stored_callables::types(module, f, i, b)?;
                }
                if pure_copy::operation(&i.kind)
                    && enum_address::types(module, f, i, b)?.is_none()
                    && option_address::types(module, f, i, b)?.is_none()
                    && !(b.task_borrowed_results && task_address::wrapper_instruction(f, i, b)?)
                {
                    // A qualified Enum claim cannot become an unsupported Copy
                    // operation merely by deleting its nominal instance inventory.
                    let claimed = match i.kind {
                        InstructionKind::MakeVariant { .. } => {
                            i.result.map(|v| vt(f, v)).transpose()?
                        }
                        InstructionKind::ReadVariant { value, .. }
                        | InstructionKind::VariantTag { value } => Some(vt(f, value)?),
                        _ => None,
                    };
                    if claimed.is_some_and(enum_address::candidate) {
                        return Err(err(
                            "borrowed Enum operation has no authenticated complete nominal instance",
                        ));
                    }
                    if !pure_copy::ordinary_owned_union(f, i, b)? {
                        pure_copy::types(f, i, b)?;
                    }
                }
                if let InstructionKind::TakePackageField { local, .. }
                | InstructionKind::DropPackageField { local, .. }
                | InstructionKind::StorePackageField { local, .. } = &i.kind
                    && f.locals
                        .get(local.0 as usize)
                        .is_some_and(|local| qualified(&local.ty).is_some())
                {
                    super::packages::check_instruction(module, f, i, b)?;
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
                    Type::ErrorUnion { error, result }
                        if b.stored_enum_addresses && enum_address::signature(t) =>
                    {
                        b.spend(2)?;
                        todo.push(error);
                        todo.push(result);
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
        if enum_address::poll_reserved(d) && !enum_address::poll_declaration(d) {
            return Err(err("reserved Poll nominal declaration is not authentic"));
        }
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
        if matches!(storage, Type::Enum { name, .. } if name.starts_with("@std.task:v1:Poll:"))
            && !enum_address::poll_declaration(d)
        {
            return Err(err(
                "reserved Poll physical nominal has no sealed declaration",
            ));
        }
        if !matches!(
            (d.kind, storage),
            (LifeNominalKind::Record, Type::Record { .. })
                | (LifeNominalKind::Package, Type::Package { .. })
                | (LifeNominalKind::Enum, Type::Enum { .. })
        ) {
            if enum_address::candidate(&instance.ty) {
                return Err(err(
                    "borrowed Enum instance disagrees with its declaration kind",
                ));
            }
            return Err(unsupported(
                "nominal kind has no actual physical stored consumer",
                None,
            ));
        }
        b.stored_package_instance |= matches!(storage, Type::Package { .. });
        if d.parameters.len() != instance.arguments.len() {
            return Err(err("nominal actual list omits complete generic ordinal"));
        }
        nominal::rows(module, instance, limits, b)?;
        b.stored_enum_addresses |= (!d.id.compiler || enum_address::poll_declaration(d))
            && enum_address::candidate(&instance.ty);
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
    let mut boundary = ty;
    let mut leaf_path = path;
    let mut class_path = Vec::new();
    if b.stored_enum_addresses {
        while let Type::ErrorUnion { error, result } = boundary {
            b.spend(1)?;
            let Some(PlaceProjection::ErrorArm(arm)) = leaf_path.first() else {
                return Err(err(
                    "stored sum relation omits its actual ErrorArm boundary",
                ));
            };
            class_path.push(LifeTypeProjection::Place(PlaceProjection::ErrorArm(*arm)));
            boundary = if *arm {
                error.as_ref()
            } else {
                result.as_ref()
            };
            leaf_path = &leaf_path[1..];
        }
    }
    let ((_, shape), pointee) = if let Some(q) = qualified(boundary) {
        (q, false)
    } else {
        (
            addressed(boundary)
                .ok_or_else(|| err("physical leaf has no actual qualified boundary"))?,
            true,
        )
    };
    b.spend(shape.slots.len())?;
    let mut local = None;
    for slot in &shape.slots {
        b.spend(slot.path.len().saturating_add(path.len()).saturating_add(1))?;
        if slot.path == leaf_path && local.replace(slot.binder).is_some() {
            return Err(err("duplicate actual stored leaf"));
        }
    }
    let local = local.ok_or_else(|| err("relation selected an absent physical stored leaf"))?;
    if pointee && !class_path.is_empty() {
        class_path.insert(0, LifeTypeProjection::Pointee);
    }
    b.spend(c.classes.len())?;
    for row in &c.classes {
        b.spend(row.position.path.len().saturating_add(1))?;
        if row.position.base == base
            && row.position.class == local
            && if class_path.is_empty() {
                if pointee {
                    row.position.path.as_slice() == [LifeTypeProjection::Pointee]
                } else {
                    row.position.path.is_empty()
                }
            } else {
                row.position.path == class_path
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
        if b.stored_enum_addresses
            && f.parameters
                .iter()
                .map(|l| &f.locals[l.0 as usize].ty)
                .chain(std::iter::once(&f.return_type))
                .any(|ty| matches!(ty, Type::ErrorUnion { .. }) && enum_address::signature(ty))
        {
            return Err(err(
                "borrowed sum wrapper signature omits its complete relation",
            ));
        }
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
        && a.retained_port == z.retained_port
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
    ) && (a.base != z.base || a.leaf != z.leaf || a.retained_port != z.retained_port)
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
            retained_port: None,
            base,
            leaf: Vec::new(),
            inline: Vec::new(),
            heap_element: None,
        };
        b.spend(3)?;
        let id = loans.len();
        loans.push(Loan {
            sources: vec![source.clone()],
            parents: Vec::new(),
            exclusive: *mutable,
        });
        b.stored_exclusive_loans |= *mutable;
        p.address = Permission {
            sources: vec![source],
            loans: vec![id],
        };
    }
    if b.stored_enum_addresses
        && matches!(ty, Type::ErrorUnion { .. })
        && enum_address::signature(ty)
    {
        for (path, mode) in enum_address::slots(ty, b)? {
            b.spend(path.len().saturating_mul(2).saturating_add(3))?;
            let source = Source {
                retained_port: None,
                base,
                leaf: path.clone(),
                inline: Vec::new(),
                heap_element: None,
            };
            let id = loans.len();
            loans.push(Loan {
                sources: vec![source.clone()],
                parents: Vec::new(),
                exclusive: mode == StoredBorrowMode::ExclusiveAddress,
            });
            b.stored_exclusive_loans |= mode == StoredBorrowMode::ExclusiveAddress;
            add_leaf(
                &mut p,
                &path,
                &Permission {
                    sources: vec![source],
                    loans: vec![id],
                },
                b,
            )?;
        }
    }
    if let Some((_, shape)) = qualified(ty).or_else(|| addressed(ty)) {
        for slot in &shape.slots {
            b.spend(slot.path.len().saturating_mul(3).saturating_add(4))?;
            let source = Source {
                retained_port: None,
                base,
                leaf: slot.path.clone(),
                inline: Vec::new(),
                heap_element: None,
            };
            let id = loans.len();
            loans.push(Loan {
                sources: vec![source.clone()],
                parents: Vec::new(),
                exclusive: slot.mode == StoredBorrowMode::ExclusiveAddress,
            });
            b.stored_exclusive_loans |= slot.mode == StoredBorrowMode::ExclusiveAddress;
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
                retained_port: None,
                base,
                leaf: Vec::new(),
                inline: Vec::new(),
                heap_element: None,
            });
        }
    }
    task_address::seed(ty, base, &mut p, loans, b)?;
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
        vacant: Vec::new(),
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
        if enum_address::ordinary(module, ty, b)? {
            p.enum_variants = enum_address::Variants::parameter(position as u32, ty, b)?;
        } else if option_address::candidate(ty) {
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
        if !source.leaf.is_empty() {
            return Err(err("stored leaf base crosses a target or inline boundary"));
        }
        let mut actual_path = source.inline.clone();
        actual_path.extend_from_slice(path);
        let p = match source.base {
            HoldRoot::Static => {
                return Err(err("static text cannot supply nested carrier permissions"));
            }
            HoldRoot::Local(l) => state.locals.get(l.0 as usize),
            HoldRoot::Parameter(p) => state.external.get(p as usize),
            HoldRoot::Capture(p) => state.captures.get(p as usize),
        }
        .ok_or_else(|| err("carrier location is absent"))?;
        if let Some(p) = leaf(p, &actual_path, b)? {
            join_permission(&mut result, p, b)?;
        }
    }
    Ok(result)
}

fn slot_position(address: &Permission, path: &[PlaceProjection], b: &mut Budget) -> Check<Source> {
    let [root] = address.sources.as_slice() else {
        return Err(err("stored slot needs one actual inline carrier"));
    };
    if !root.leaf.is_empty() {
        return Err(err("stored slot crosses a target boundary"));
    }
    b.spend(width(root).saturating_add(path.len()))?;
    let mut slot = root.clone();
    slot.inline.extend_from_slice(path);
    Ok(slot)
}

fn require_filled(
    state: &Frame,
    address: &Permission,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<()> {
    if state.vacant.is_empty() {
        return Ok(());
    }
    for root in &address.sources {
        b.spend(width(root).saturating_add(path.len()))?;
        let mut slot = root.clone();
        slot.inline.extend_from_slice(path);
        b.spend(amount(state.vacant.len().saturating_add(1), width(&slot))?)?;
        if state.vacant.contains(&slot) {
            return Err(err("stored slot is vacant on an incoming path"));
        }
    }
    Ok(())
}

fn clear_local_holes(state: &mut Frame, local: LocalId, b: &mut Budget) -> Check<()> {
    b.spend(state.vacant.len())?;
    state
        .vacant
        .retain(|slot| slot.base != HoldRoot::Local(local));
    Ok(())
}
fn projection_only_load(
    f: &TypedFunction,
    instruction: &Instruction,
    storage: ValueId,
    b: &mut Budget,
) -> Check<Option<LocalId>> {
    let mut adjacent = None;
    let mut uses = 0usize;
    for block in &f.blocks {
        for (index, actual) in block.instructions.iter().enumerate() {
            b.spend(1)?;
            if std::ptr::eq(actual, instruction)
                && index > 0
                && block.instructions[index - 1].result == Some(storage)
                && let InstructionKind::Load(local) = block.instructions[index - 1].kind
                && addressed(&f.locals[local.0 as usize].ty).is_some()
            {
                adjacent = Some(local);
            }
            for operand in instruction_operands(&actual.kind) {
                b.spend(1)?;
                uses += usize::from(operand == storage);
            }
        }
        b.spend(1)?;
        uses += usize::from(matches!(block.terminator,
            Terminator::Return(Some(value)) if value == storage));
        uses += usize::from(matches!(block.terminator,
            Terminator::Branch { condition, .. } if condition == storage));
    }
    Ok((uses == 1).then_some(adjacent).flatten())
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
    let mut p = if let Some(port) = source.retained_port {
        task_address::select(v, &source.leaf, port, b)?
    } else if source.leaf.is_empty() {
        permission_work(&v.address, b)?;
        v.address.clone()
    } else if !v.leaves.is_empty()
        && !v.address.sources.is_empty()
        && v.address.sources.iter().all(|root| {
            root.leaf.is_empty()
                && root.inline.is_empty()
                && match root.base {
                    HoldRoot::Local(local) => state.locals.get(local.0 as usize).is_some(),
                    HoldRoot::Parameter(index) => state.external.get(index as usize).is_some(),
                    HoldRoot::Static | HoldRoot::Capture(_) => false,
                }
        })
    {
        // A mutable carrier's current slot comes from the frame. The loaded
        // SSA payload can predate an earlier call or field replacement.
        select(state, &v.address, &source.leaf, b)?
    } else if let Some(p) = leaf(v, &source.leaf, b)? {
        permission_work(p, b)?;
        p.clone()
    } else {
        select(state, &v.address, &source.leaf, b)?
    };
    for target in &mut p.sources {
        b.spend(source.inline.len())?;
        if let Some(heap) = &mut target.heap_element {
            if source.heap_element.is_some() {
                return Err(err("nested Store element summary is outside this profile"));
            }
            heap.extend(&source.inline);
        } else {
            target.inline.extend(&source.inline);
            if let Some(heap) = &source.heap_element {
                b.spend(heap.len())?;
                target.heap_element = Some(heap.clone());
            }
        }
    }
    Ok(p)
}

fn write_sites(
    module: &TypedModule,
    f: &TypedFunction,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<WriteSites> {
    let mut sites = Vec::new();
    if !b.stored_slot_updates && !b.stored_constructor_sites {
        return Ok(sites);
    }
    b.spend(f.blocks.len())?;
    for block in &f.blocks {
        b.spend(block.instructions.len())?;
        for i in &block.instructions {
            let site = i as *const Instruction as usize;
            let mut add = |parameter: u32, path: &Path, exclusive: bool| -> Check<()> {
                b.spend(path.len().saturating_add(sites.len()).saturating_add(2))?;
                let id = loans.len();
                loans.push(Loan::default());
                b.stored_exclusive_loans |= exclusive;
                sites.push((site, parameter, path.clone(), id, exclusive));
                Ok(())
            };
            match &i.kind {
                InstructionKind::MakeAggregate { .. } | InstructionKind::MakePackage { .. } => {
                    if let Some((_, shape)) = qualified(vt(
                        f,
                        i.result.ok_or_else(|| err("aggregate lacks result"))?,
                    )?) {
                        for slot in &shape.slots {
                            if slot.mode == StoredBorrowMode::ExclusiveAddress {
                                add(u32::MAX, &slot.path, true)?;
                            }
                        }
                    }
                }
                InstructionKind::StoreStoredBorrow { path, value, .. } => add(
                    u32::MAX,
                    path,
                    matches!(vt(f, *value)?, Type::Address { mutable: true, .. }),
                )?,
                InstructionKind::Call { function, .. } => {
                    let callee = module
                        .functions
                        .get(function.0 as usize)
                        .ok_or_else(|| err("stored call target is absent"))?;
                    for (position, local) in callee.parameters.iter().enumerate() {
                        if let Type::Address {
                            mutable: true,
                            inner,
                        } = &callee.locals[local.0 as usize].ty
                            && let Some((_, shape)) = qualified(inner)
                        {
                            for slot in &shape.slots {
                                add(
                                    position as u32,
                                    &slot.path,
                                    slot.mode == StoredBorrowMode::ExclusiveAddress,
                                )?;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Ok(sites)
}

fn retain_written_slot(
    source: &Permission,
    i: &Instruction,
    parameter: u32,
    path: &Path,
    sites: &WriteSites,
    loans: &mut [Loan],
    b: &mut Budget,
) -> Check<Permission> {
    let site = i as *const Instruction as usize;
    b.spend(sites.len().saturating_mul(path.len().saturating_add(1)))?;
    let (_, _, _, id, exclusive) = sites
        .iter()
        .find(|(at, p, selected, _, _)| *at == site && *p == parameter && selected == path)
        .ok_or_else(|| err("stored write has no stable loan site"))?;
    let row = loans
        .get_mut(*id)
        .ok_or_else(|| err("stored write loan exceeds actual arena"))?;
    add_sources(&mut row.sources, &source.sources, b)?;
    add_loans(&mut row.parents, &source.loans, b)?;
    row.parents.retain(|parent| parent != id);
    row.exclusive = *exclusive;
    b.stored_exclusive_loans |= row.exclusive;
    permission_work(source, b)?;
    Ok(Permission {
        sources: source.sources.clone(),
        loans: vec![*id],
    })
}

#[allow(clippy::too_many_arguments)]
fn step(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &mut Frame,
    values: &[Payload],
    summaries: &[Summary],
    sites: &WriteSites,
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
    let task_transfer = task_address::transfer(module, f, i, state, values, summaries, b)?;
    let enum_transfer = enum_address::transfer(module, f, i, values, b)?;
    let option_transfer = if enum_transfer.is_none() && task_transfer.is_none() {
        option_address::transfer(module, f, i, values, b)?
    } else {
        None
    };
    match &i.kind {
        InstructionKind::StaticUtf8 { .. } => {
            b.spend(1)?;
            out.address.sources.push(Source {
                retained_port: None,
                base: HoldRoot::Static,
                leaf: Vec::new(),
                inline: Vec::new(),
                heap_element: None,
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
            clear_local_holes(state, *local, b)?;
            state.locals[local.0 as usize] = copy_payload(val(*value)?, b)?
        }
        InstructionKind::TakePackageField { local, index } => {
            let holder = state
                .locals
                .get_mut(local.0 as usize)
                .ok_or_else(|| err("taken Package field local is absent"))?;
            out = take_package_field_leaves(holder, *index, b)?;
            b.spend(state.vacant.len())?;
            state.vacant.retain(|slot| {
                !(slot.base == HoldRoot::Local(*local)
                    && slot.inline.first() == Some(&PlaceProjection::Field(*index)))
            });
        }
        InstructionKind::DropPackageField { local, index } => {
            let holder = state
                .locals
                .get_mut(local.0 as usize)
                .ok_or_else(|| err("dropped Package field local is absent"))?;
            take_package_field_leaves(holder, *index, b)?;
            b.spend(state.vacant.len())?;
            state.vacant.retain(|slot| {
                !(slot.base == HoldRoot::Local(*local)
                    && slot.inline.first() == Some(&PlaceProjection::Field(*index)))
            });
        }
        InstructionKind::StorePackageField {
            local,
            index,
            value,
        } => {
            let source = copy_payload(val(*value)?, b)?;
            let holder = state
                .locals
                .get_mut(local.0 as usize)
                .ok_or_else(|| err("stored Package field local is absent"))?;
            take_package_field_leaves(holder, *index, b)?;
            for (path, permission) in &source.leaves {
                b.spend(path.len().saturating_add(1))?;
                let mut complete = vec![PlaceProjection::Field(*index)];
                complete.extend(path);
                add_leaf(holder, &complete, permission, b)?;
            }
            b.spend(state.vacant.len())?;
            state.vacant.retain(|slot| {
                !(slot.base == HoldRoot::Local(*local)
                    && slot.inline.first() == Some(&PlaceProjection::Field(*index)))
            });
        }
        InstructionKind::StorageLive(l)
        | InstructionKind::StorageDead(l)
        | InstructionKind::DropClosure { local: l } => {
            state.locals[l.0 as usize] = Payload::default();
            clear_local_holes(state, *l, b)?;
        }
        InstructionKind::DescribeLocal { local } => {
            if f.locals
                .get(local.0 as usize)
                .is_some_and(|local| super::stored_owned::owner(&local.ty))
            {
                b.spend(1)?;
                out.address.sources.push(Source {
                    retained_port: None,
                    base: HoldRoot::Local(*local),
                    leaf: Vec::new(),
                    inline: Vec::new(),
                    heap_element: None,
                });
            }
        }
        InstructionKind::DescribeCapture { index } => {
            b.spend(1)?;
            out.address.sources.push(Source {
                retained_port: None,
                base: HoldRoot::Capture(*index),
                leaf: Vec::new(),
                inline: Vec::new(),
                heap_element: None,
            });
        }
        InstructionKind::TakeCapture { .. }
        | InstructionKind::MakeManaged { .. }
        | InstructionKind::ReadDesc { .. } => {}
        InstructionKind::DropManaged { local } | InstructionKind::DropPackage { local } => {
            state.locals[local.0 as usize] = Payload::default();
            clear_local_holes(state, *local, b)?;
        }
        InstructionKind::DescribeAddress { address } if b.store_option_results => {
            join_permission(&mut out.store_description, &val(*address)?.address, b)?;
        }
        InstructionKind::OwnedStoreBorrow { store, .. } if b.store_option_results => {
            let description = &val(*store)?.store_description;
            // The reverse-batch solver may see bottom before a predecessor's
            // description arrives. Its final grounded-value check rejects a
            // window that remains empty at the fixed point.
            for source in &description.sources {
                if source.heap_element.is_some() {
                    return Err(err("nested Store element window is outside this profile"));
                }
                b.spend(width(source).saturating_add(1))?;
                let mut target = source.clone();
                target.heap_element = Some(Vec::new());
                add_sources(&mut out.address.sources, &[target], b)?;
            }
            add_loans(&mut out.address.loans, &description.loans, b)?;
        }
        InstructionKind::ByteBufferNew | InstructionKind::DescribeAddress { .. } => {}
        InstructionKind::ByteBufferPush { .. }
        | InstructionKind::ByteBufferReserve { .. }
        | InstructionKind::ByteBufferTruncate { .. } => {
            // ByteBuffer growth may relocate bytes. This component must not
            // silently preserve a stored pointer into that payload.
            for ty in f.locals.iter().map(|local| &local.ty).chain(&f.values) {
                let Some((storage, shape)) = qualified(ty) else {
                    continue;
                };
                b.spend(shape.slots.len())?;
                for row in &shape.slots {
                    let mut field = storage;
                    for step in &row.path {
                        b.spend(1)?;
                        if let Type::BorrowedStorage { storage, .. } = field {
                            field = storage;
                        }
                        field = match (field, step) {
                            (
                                Type::Record { fields, .. } | Type::Package { fields, .. },
                                PlaceProjection::Field(index),
                            ) => fields
                                .get(*index as usize)
                                .map(|(_, ty)| ty)
                                .ok_or_else(|| err("stored field is absent"))?,
                            (Type::Tuple(fields), PlaceProjection::Field(index)) => fields
                                .get(*index as usize)
                                .ok_or_else(|| err("stored tuple field is absent"))?,
                            _ => {
                                return Err(unsupported(
                                    "ByteBuffer relocation needs projected stored byte target invalidation",
                                    Some(i.span),
                                ));
                            }
                        };
                    }
                    if matches!(field, Type::Address { inner, .. }
                        if inner.as_ref() == &Type::U8)
                    {
                        return Err(unsupported(
                            "ByteBuffer relocation needs projected stored byte target invalidation",
                            Some(i.span),
                        ));
                    }
                }
            }
        }
        InstructionKind::TakeLocal { local } => {
            out = copy_payload(&state.locals[local.0 as usize], b)?;
            state.locals[local.0 as usize] = Payload::default();
            clear_local_holes(state, *local, b)?;
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
                retained_port: None,
                base: HoldRoot::Capture(*index),
                leaf: Vec::new(),
                inline: Vec::new(),
                heap_element: None,
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
                            retained_port: None,
                            base: HoldRoot::Local(*callee),
                            leaf: vec![],
                            inline: vec![],
                            heap_element: None,
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
                retained_port: None,
                base: HoldRoot::Local(*local),
                leaf: Vec::new(),
                inline: Vec::new(),
                heap_element: None,
            });
            if b.task_borrowed_results {
                task_address::join(&mut out, &state.locals[local.0 as usize], b)?;
            }
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
                if let Some(element) = &mut target.heap_element {
                    element.push(PlaceProjection::Field(*index));
                } else {
                    target.inline.push(PlaceProjection::Field(*index));
                }
            }
        }
        InstructionKind::ReborrowAddress { address, .. }
        | InstructionKind::AddressIndex { address, .. }
        | InstructionKind::AddressSlice { address, .. } => {
            join_permission(&mut out.address, &val(*address)?.address, b)?;
            task_address::join(&mut out, val(*address)?, b)?;
            if fixed_environment(module, vt(f, *address)?) {
                for (path, p) in &val(*address)?.leaves {
                    add_leaf(&mut out, path, p, b)?;
                }
            }
        }
        InstructionKind::MakeAggregate { fields } | InstructionKind::MakePackage { fields } => {
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
                    let permission = if tail.is_empty() {
                        Some(&value.address)
                    } else {
                        leaf(value, tail, b)?
                    };
                    if let Some(permission) = permission {
                        if row.mode == StoredBorrowMode::ExclusiveAddress {
                            let held = retain_written_slot(
                                permission,
                                i,
                                u32::MAX,
                                &row.path,
                                sites,
                                loans,
                                b,
                            )?;
                            add_leaf(&mut out, &row.path, &held, b)?;
                        } else {
                            add_leaf(&mut out, &row.path, permission, b)?;
                        }
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
            let parents = if let Some(local) = projection_only_load(f, i, *storage, b)? {
                state.locals[local.0 as usize].address.loans.clone()
            } else {
                val(*storage)?.address.loans.clone()
            };
            add_loans(&mut out.address.loans, &parents, b)?;
        }
        InstructionKind::LoadStoredBorrow {
            storage,
            path,
            mode: StoredBorrowLoad::Take,
        } => {
            let address = &val(*storage)?.address;
            out.address = select(state, address, path, b)?;
            if !address.sources.is_empty() {
                let position = slot_position(address, path, b)?;
                let holder = match position.base {
                    HoldRoot::Local(local) => state.locals.get_mut(local.0 as usize),
                    HoldRoot::Parameter(index) => state.external.get_mut(index as usize),
                    HoldRoot::Capture(index) => state.captures.get_mut(index as usize),
                    HoldRoot::Static => None,
                };
                if let Some(holder) = holder {
                    b.spend(holder.leaves.len())?;
                    holder.leaves.retain(|(path, _)| path != &position.inline);
                }
                add_sources(&mut state.vacant, &[position], b)?;
            }
        }
        InstructionKind::StoreStoredBorrow {
            storage,
            path,
            value,
        } => {
            let [root] = val(*storage)?.address.sources.as_slice() else {
                return Err(err("replacement carrier requires one exact actual source"));
            };
            if !root.leaf.is_empty() {
                return Err(err("replacement carrier crosses a target boundary"));
            }
            let actual_path = if root.inline.is_empty() {
                path.clone()
            } else {
                slot_position(&val(*storage)?.address, path, b)?.inline
            };
            let new =
                retain_written_slot(&val(*value)?.address, i, u32::MAX, path, sites, loans, b)?;
            // Reverse CFG batches can first see an unresolved predecessor.
            // Final access validation rejects a truly empty replacement.
            let holder = match root.base {
                HoldRoot::Local(local) => state.locals.get_mut(local.0 as usize),
                HoldRoot::Parameter(index) => state.external.get_mut(index as usize),
                HoldRoot::Static | HoldRoot::Capture(_) => None,
            }
            .ok_or_else(|| err("replacement carrier location is absent"))?;
            b.spend(amount(
                holder.leaves.len().saturating_add(1),
                actual_path.len().saturating_add(1),
            )?)?;
            if let Some((_, leaf)) = holder
                .leaves
                .iter_mut()
                .find(|(existing, _)| existing == &actual_path)
            {
                *leaf = new;
            } else {
                b.spend(actual_path.len().saturating_add(1))?;
                holder.leaves.push((actual_path, new));
            }
            if !state.vacant.is_empty() {
                let position = slot_position(&val(*storage)?.address, path, b)?;
                b.spend(state.vacant.len())?;
                state.vacant.retain(|slot| slot != &position);
            }
        }
        InstructionKind::LoadStoredBorrow { .. } => {
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
            task_address::call(
                &summaries[function.0 as usize],
                arguments,
                values,
                state,
                &mut out,
                b,
            )?;
            out.enum_variants = summaries[function.0 as usize]
                .returned_enum_variants
                .substitute(arguments, values, b)?;
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
                } else if (out.enum_variants.present() && out.enum_variants.slot_active(path))
                    || (!out.enum_variants.present()
                        && (!option_address::candidate(&callee.return_type)
                            || out.option_variants.mask() & 2 != 0))
                {
                    add_leaf(&mut out, path, &p, b)?;
                }
            }
            let mut updates = Vec::new();
            for (parameter, path, sources) in &summaries[function.0 as usize].external_after {
                let index = *parameter as usize;
                let local = *callee
                    .parameters
                    .get(index)
                    .ok_or_else(|| err("external summary parameter is absent"))?;
                if !matches!(&callee.locals[local.0 as usize].ty,
                    Type::Address { mutable: true, inner } if qualified(inner).is_some())
                {
                    continue;
                }
                let actual = val(*arguments
                    .get(index)
                    .ok_or_else(|| err("external summary actual argument is absent"))?)?;
                let [root] = actual.address.sources.as_slice() else {
                    return Err(err("external update needs one exact actual carrier source"));
                };
                if !root.leaf.is_empty() {
                    return Err(err("external update crosses a target boundary"));
                }
                let actual_path = if root.inline.is_empty() {
                    path.clone()
                } else {
                    slot_position(&actual.address, path, b)?.inline
                };
                let mut permission = Permission::default();
                for source in sources {
                    join_permission(
                        &mut permission,
                        &instantiate(source, arguments, values, state, b)?,
                        b,
                    )?;
                }
                if permission.sources.is_empty() {
                    return Err(err("external update has no grounded replacement source"));
                }
                b.spend(path.len().saturating_add(permission.sources.len()))?;
                let permission =
                    retain_written_slot(&permission, i, *parameter, path, sites, loans, b)?;
                updates.push((root.clone(), actual_path, permission));
            }
            for (root, path, permission) in updates {
                let holder = match root.base {
                    HoldRoot::Local(local) => state.locals.get_mut(local.0 as usize),
                    HoldRoot::Parameter(index) => state.external.get_mut(index as usize),
                    HoldRoot::Static | HoldRoot::Capture(_) => None,
                }
                .ok_or_else(|| err("updated caller carrier location is absent"))?;
                b.spend(amount(
                    holder.leaves.len().saturating_add(1),
                    path.len().saturating_add(1),
                )?)?;
                if let Some((_, leaf)) = holder
                    .leaves
                    .iter_mut()
                    .find(|(existing, _)| existing == &path)
                {
                    *leaf = permission;
                } else {
                    b.spend(path.len().saturating_add(1))?;
                    holder.leaves.push((path, permission));
                }
            }
        }
        _ if enum_transfer.is_some() => {
            out = enum_transfer.unwrap();
        }
        _ if option_transfer.is_some() => {
            out = option_transfer.unwrap();
        }
        _ if pure_copy::ordinary_owned_union(f, i, b)? => {}
        _ if pure_copy::ordinary_package_borrow(f, i, b)? => {}
        _ if task_transfer.is_some() => {
            out = task_transfer.clone().unwrap();
        }
        _ if pure_copy::operation(&i.kind) => {
            if !pure_copy::ordinary_owned_union(f, i, b)? {
                pure_copy::types(f, i, b)?;
            }
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
        | InstructionKind::OwnedStoreNew { .. }
        | InstructionKind::OwnedStoreReserve { .. }
        | InstructionKind::OwnedStorePush { .. }
        | InstructionKind::OwnedStorePop { .. }
        | InstructionKind::OwnedStoreCapacity { .. }
        | InstructionKind::OwnedStoreLen { .. }
        | InstructionKind::DropErrorUnion { .. }
        | InstructionKind::DescribePackageField { .. }
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
        b.stored_exclusive_loans |= row.exclusive;
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
        b.stored_exclusive_loans = true;
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
                b.stored_exclusive_loans |= row.exclusive;
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

/// A reachable DAG needs no iterative frame propagation. The graph comes from
/// this verifier's structural checks, including checks of unreachable blocks.
fn acyclic_order(g: &FunctionGraph, b: &mut Budget) -> Check<Option<Vec<usize>>> {
    let count = g.reachable.len();
    b.spend(amount(count, 3)?)?;
    let mut incoming = vec![0usize; count];
    let mut ready = Vec::with_capacity(count);
    let mut order = Vec::with_capacity(count);
    let mut reachable = 0usize;
    b.spend(count)?;
    for (block, live) in g.reachable.iter().enumerate() {
        if !live {
            continue;
        }
        reachable += 1;
        b.spend(g.successors[block].len())?;
        for child in &g.successors[block] {
            if g.reachable[*child] {
                incoming[*child] = incoming[*child]
                    .checked_add(1)
                    .ok_or_else(|| limit("stored DAG predecessor count overflow"))?;
            }
        }
    }
    b.spend(count)?;
    for (block, degree) in incoming.iter().enumerate() {
        if g.reachable[block] && *degree == 0 {
            ready.push(block);
        }
    }
    while let Some(block) = ready.pop() {
        b.spend(1)?;
        order.push(block);
        b.spend(g.successors[block].len())?;
        for child in &g.successors[block] {
            if g.reachable[*child] {
                incoming[*child] -= 1;
                if incoming[*child] == 0 {
                    ready.push(*child);
                }
            }
        }
    }
    Ok((order.len() == reachable).then_some(order))
}

fn solve(
    module: &TypedModule,
    f: &TypedFunction,
    g: &FunctionGraph,
    summaries: &[Summary],
    b: &mut Budget,
) -> Check<Solution> {
    // Each reverse-batch solve creates a new function-local loan arena.
    // Its paid seed/site/step writes record every possible exclusive loan,
    // including constructor sites that are not currently active.
    b.stored_exclusive_loans = false;
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
    let sites = write_sites(module, f, &mut loans, b)?;
    let mut values = vec![Payload::default(); f.values.len()];
    let mut outputs = Vec::new();
    for _ in &f.blocks {
        outputs.push(empty_frame(module, f, b)?);
    }
    // The combined Task/Option domain is newly admitted. Keep historical
    // requests on their existing solver and request-cost path. In a DAG every
    // predecessor frame and dominating SSA definition is final before its user;
    // join_payload still records all sources and loan growth, including values
    // whose creation does not change a local frame.
    if b.task_borrowed_results
        && b.store_option_results
        && let Some(order) = acyclic_order(g, b)?
    {
        for bi in order {
            b.spend(1)?;
            let mut state = if bi == f.entry.0 as usize {
                copy_frame(&entry, b)?
            } else {
                empty_frame(module, f, b)?
            };
            b.spend(g.predecessors[bi].len())?;
            for pred in &g.predecessors[bi] {
                if g.reachable[*pred] {
                    join_frame(&mut state, &outputs[*pred], b)?;
                }
            }
            for instruction in &f.blocks[bi].instructions {
                let out = step(
                    module,
                    f,
                    instruction,
                    &mut state,
                    &values,
                    summaries,
                    &sites,
                    &mut loans,
                    b,
                )?;
                if let Some(value) = instruction.result {
                    join_payload(&mut values[value.0 as usize], &out, b)?;
                }
            }
            outputs[bi] = state;
        }
        return Ok(Solution {
            values,
            outputs,
            entry,
            loans,
            sites,
        });
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
                let out = step(
                    module, f, i, &mut state, &values, summaries, &sites, &mut loans, b,
                )?;
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
        sites,
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
        InstructionKind::TakeLocal { local }
        | InstructionKind::TakePackageField { local, .. }
        | InstructionKind::DropPackageField { local, .. }
        | InstructionKind::StorePackageField { local, .. }
            if qualified(&f.locals[local.0 as usize].ty).is_some()
                || matches!(i.kind, InstructionKind::TakeLocal { .. })
                    && matches!(f.locals[local.0 as usize].ty, Type::Address { .. }) =>
        {
            live[f.values.len() + local.0 as usize] = true;
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
        if b.task_borrowed_results {
            for (_, p) in &payload.task_ports {
                add_loans(&mut loans, &p.loans, b)?;
            }
        }
        for (_, p) in &payload.leaves {
            add_loans(&mut loans, &p.loans, b)?;
        }
    }
    if b.task_borrowed_results {
        // The actual frame retains its inputs until its owning carrier moves
        // or is discharged; dead SSA uses of a head do not discharge a frame.
        for payload in &state.locals {
            for (_, permission) in &payload.task_ports {
                add_loans(&mut loans, &permission.loans, b)?;
            }
        }
    }
    // Only an actual slot write can create a new external loan that is not
    // already represented by the original parameter relation.
    if b.stored_slot_updates {
        for holder in &state.external {
            for (_, p) in &holder.leaves {
                add_loans(&mut loans, &p.loans, b)?;
            }
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
        if let Some((_, shape)) = addressed(ty) {
            for slot in &shape.slots {
                require_filled(state, &value.address, &slot.path, b)?;
            }
        }
        if let Type::Address { mutable, .. } = ty {
            access(
                &value.address,
                *mutable,
                active,
                &solution.loans,
                requirements,
                b,
            )?;
            if b.stored_slot_updates
                && *mutable
                && let Some((_, shape)) = addressed(ty)
            {
                for slot in &shape.slots {
                    let held = select(state, &value.address, &slot.path, b)?;
                    access(
                        &held,
                        slot.mode == StoredBorrowMode::ExclusiveAddress,
                        active,
                        &solution.loans,
                        requirements,
                        b,
                    )?;
                }
            }
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
                InstructionKind::MakeAggregate { fields }
                | InstructionKind::MakePackage { fields }
                    if b.stored_exclusive_loans =>
                {
                    let result = i.result.ok_or_else(|| err("aggregate lacks result"))?;
                    if let Some((_, shape)) = qualified(vt(f, result)?) {
                        for (index, slot) in shape.slots.iter().enumerate() {
                            b.spend(slot.path.len().saturating_add(1))?;
                            let Some((PlaceProjection::Field(field), tail)) =
                                slot.path.split_first()
                            else {
                                return Err(err("constructor slot has no input field"));
                            };
                            let source = &solution.values[fields[*field as usize].0 as usize];
                            let permission = if tail.is_empty() {
                                &source.address
                            } else {
                                leaf(source, tail, b)?
                                    .ok_or_else(|| err("constructor has no actual input leaf"))?
                            };
                            let exclusive = slot.mode == StoredBorrowMode::ExclusiveAddress;
                            access(
                                permission,
                                exclusive,
                                &active,
                                &solution.loans,
                                requirements,
                                b,
                            )?;
                            for previous in &shape.slots[..index] {
                                b.spend(1)?;
                                if exclusive || previous.mode == StoredBorrowMode::ExclusiveAddress
                                {
                                    let other = leaf(
                                        &solution.values[result.0 as usize],
                                        &previous.path,
                                        b,
                                    )?
                                    .ok_or_else(|| err("constructor has no prior stored leaf"))?;
                                    for left in &permission.sources {
                                        for right in &other.sources {
                                            apart(left, right, requirements, b)?;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                InstructionKind::TakePackageField { local, index }
                | InstructionKind::DropPackageField { local, index }
                | InstructionKind::StorePackageField { local, index, .. } => {
                    if matches!(i.kind, InstructionKind::TakePackageField { .. }) {
                        b.spend(state.vacant.len())?;
                        if state.vacant.iter().any(|slot| {
                            slot.base == HoldRoot::Local(*local)
                                && slot.inline.first() == Some(&PlaceProjection::Field(*index))
                        }) {
                            return Err(err("child Package move has a vacant stored slot"));
                        }
                    }
                    let actual = Permission {
                        sources: vec![Source {
                            retained_port: None,
                            base: HoldRoot::Local(*local),
                            leaf: Vec::new(),
                            inline: vec![PlaceProjection::Field(*index)],
                            heap_element: None,
                        }],
                        loans: Vec::new(),
                    };
                    b.spend(1)?;
                    access(&actual, true, &active, &solution.loans, requirements, b)?;
                }
                InstructionKind::Load(local) => {
                    b.spend(state.vacant.len())?;
                    if state
                        .vacant
                        .iter()
                        .any(|slot| slot.base == HoldRoot::Local(*local))
                    {
                        return Err(err("whole carrier copy has a vacant stored slot"));
                    }
                    if matches!(
                        f.locals[local.0 as usize].ty,
                        Type::Address { mutable: true, .. }
                    ) {
                        access(
                            &state.locals[local.0 as usize].address,
                            true,
                            &active,
                            &solution.loans,
                            requirements,
                            b,
                        )?;
                    }
                }
                InstructionKind::BorrowCapture { index, mutable } => {
                    b.spend(1)?;
                    let actual = Permission {
                        sources: vec![Source {
                            retained_port: None,
                            base: HoldRoot::Capture(*index),
                            leaf: vec![],
                            inline: vec![],
                            heap_element: None,
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
                    if matches!(i.kind, InstructionKind::TakeLocal { .. }) {
                        b.spend(state.vacant.len())?;
                        if state
                            .vacant
                            .iter()
                            .any(|slot| slot.base == HoldRoot::Local(*local))
                        {
                            return Err(err("whole carrier move has a vacant stored slot"));
                        }
                    }
                    b.spend(1)?;
                    let source = Source {
                        retained_port: None,
                        base: HoldRoot::Local(*local),
                        leaf: Vec::new(),
                        inline: Vec::new(),
                        heap_element: None,
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
                    require_filled(&state, pointer(*storage)?, path, b)?;
                    let exclusive = matches!(
                        mode,
                        StoredBorrowLoad::Reborrow { mutable: true } | StoredBorrowLoad::Take
                    );
                    // The established single shared slot has the same whole
                    // carrier access as before mutable slot replacement was
                    // introduced. Only a writer or multiple/exclusive slots
                    // need the finer inline-window proof below.
                    if !b.stored_slot_updates
                        && matches!(mode, StoredBorrowLoad::CopyShared)
                        && addressed(vt(f, *storage)?).is_some_and(|(_, shape)| {
                            shape.slots.len() == 1
                                && shape.slots[0].mode == StoredBorrowMode::SharedAddress
                        })
                    {
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
                    } else {
                        // Compare the actual inline slot, retaining the carrier
                        // loans. Whole-carrier access would conflate sibling slots.
                        let base = pointer(*storage)?;
                        b.spend(
                            base.sources
                                .len()
                                .saturating_mul(path.len().saturating_add(1)),
                        )?;
                        let slot = Permission {
                            sources: base
                                .sources
                                .iter()
                                .map(|source| {
                                    let mut source = source.clone();
                                    source.inline.extend(path);
                                    source
                                })
                                .collect(),
                            loans: base.loans.clone(),
                        };
                        access(&slot, exclusive, &active, &solution.loans, requirements, b)?;
                        let p = select(&state, pointer(*storage)?, path, b)?;
                        let target_write = if matches!(mode, StoredBorrowLoad::Take) {
                            i.result.is_some_and(|id| {
                                matches!(
                                    f.values[id.0 as usize],
                                    Type::Address { mutable: true, .. }
                                )
                            })
                        } else {
                            exclusive
                        };
                        access(&p, target_write, &active, &solution.loans, requirements, b)?;
                    }
                }
                InstructionKind::StoreStoredBorrow {
                    storage,
                    path,
                    value,
                } => {
                    let exclusive = matches!(vt(f, *value)?, Type::Address { mutable: true, .. });
                    let base = pointer(*storage)?;
                    b.spend(
                        base.sources
                            .len()
                            .saturating_mul(path.len().saturating_add(1)),
                    )?;
                    let slot = Permission {
                        sources: base
                            .sources
                            .iter()
                            .map(|source| {
                                let mut source = source.clone();
                                source.inline.extend(path);
                                source
                            })
                            .collect(),
                        loans: base.loans.clone(),
                    };
                    access(&slot, true, &active, &solution.loans, requirements, b)?;
                    let previous = select(&state, base, path, b)?;
                    if previous.sources.is_empty() {
                        let position = slot_position(base, path, b)?;
                        if !state.vacant.contains(&position) {
                            return Err(err("replacement has no live slot and no extracted hole"));
                        }
                    } else {
                        access(
                            &previous,
                            exclusive,
                            &active,
                            &solution.loans,
                            requirements,
                            b,
                        )?;
                    }
                    access(
                        pointer(*value)?,
                        exclusive,
                        &active,
                        &solution.loans,
                        requirements,
                        b,
                    )?;
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
                                        retained_port: None,
                                        base: HoldRoot::Local(*callee),
                                        leaf: vec![],
                                        inline: vec![],
                                        heap_element: None,
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
                &solution.sites,
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
    s.returned_enum_variants.work(b)?;
    for (_, tags) in &s.enum_values {
        tags.work(b)?;
    }
    s.returned_variants.work(b)?;
    b.spend(
        s.result
            .len()
            .saturating_add(s.external_after.len())
            .saturating_add(s.apart.len()),
    )?;
    if b.task_borrowed_results {
        for (path, row) in &s.task_ports {
            b.spend(path.len())?;
            source_work(row, b)?;
        }
    }
    for (path, row) in &s.result {
        b.spend(path.len())?;
        source_work(row, b)?;
    }
    for (_, path, row) in &s.external_after {
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
    for (bi, block) in f.blocks.iter().enumerate() {
        if g.reachable[bi] && matches!(block.terminator, Terminator::Return(_)) {
            b.spend(solution.outputs[bi].vacant.len())?;
            if solution.outputs[bi]
                .vacant
                .iter()
                .any(|slot| matches!(slot.base, HoldRoot::Parameter(_) | HoldRoot::Capture(_)))
            {
                summary.complete = false;
            }
        }
    }
    for (id, (ty, p)) in f.values.iter().zip(&solution.values).enumerate() {
        b.spend(1)?;
        if p.enum_variants.present() {
            p.enum_variants.work(b)?;
            summary
                .enum_values
                .push((ValueId(id as u32), p.enum_variants.clone()));
        }
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
        if p.creation_complete == Some(false) {
            summary.complete = false;
        }
        if matches!(ty, Type::Address { .. }) && p.address.sources.is_empty() {
            summary.complete = false;
        }
        if b.stored_enum_addresses
            && matches!(ty, Type::ErrorUnion { .. })
            && enum_address::ordinary(module, ty, b)?
        {
            if !p.enum_variants.grounded(ty, b)? {
                summary.complete = false;
            }
            for (path, _) in enum_address::slots(ty, b)? {
                if p.enum_variants.slot_active(&path)
                    && leaf(p, &path, b)?.is_none_or(|p| p.sources.is_empty())
                {
                    summary.complete = false;
                }
            }
        }
        if let Some((_, shape)) = qualified(ty) {
            if enum_address::ordinary(module, ty, b)? {
                if !p.enum_variants.grounded(ty, b)? {
                    summary.complete = false;
                }
            } else if option_address::candidate(ty) {
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
                if p.enum_variants.present() && !p.enum_variants.slot_active(&row.path) {
                    continue;
                }
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
    for (bi, (block, reachable)) in f.blocks.iter().zip(&g.reachable).enumerate() {
        b.spend(1)?;
        if b.stored_slot_updates && *reachable && matches!(block.terminator, Terminator::Return(_))
        {
            for (position, payload) in solution.outputs[bi].external.iter().enumerate() {
                let local = *f
                    .parameters
                    .get(position)
                    .ok_or_else(|| err("external output exceeds actual parameter table"))?;
                if !matches!(&f.locals[local.0 as usize].ty,
                    Type::Address { mutable: true, inner } if qualified(inner).is_some())
                {
                    continue;
                }
                for (path, permission) in &payload.leaves {
                    b.spend(
                        path.len()
                            .saturating_add(permission.sources.len())
                            .saturating_add(1),
                    )?;
                    for source in &permission.sources {
                        if !matches!(source.base, HoldRoot::Parameter(_) | HoldRoot::Static) {
                            return Err(err("external slot update retains a callee-local target"));
                        }
                        if let HoldRoot::Parameter(origin) = source.base
                            && relation(f, StorageBase::Parameter(position as u32), path, b)?
                                != relation(f, StorageBase::Parameter(origin), &source.leaf, b)?
                        {
                            return Err(err(
                                "external slot update exceeds its declared lifetime relation",
                            ));
                        }
                    }
                    let key = (position as u32, path.clone());
                    if let Some((_, _, row)) = summary
                        .external_after
                        .iter_mut()
                        .find(|(p, existing, _)| *p == key.0 && *existing == key.1)
                    {
                        add_sources(row, &permission.sources, b)?;
                    } else {
                        b.spend(
                            summary
                                .external_after
                                .len()
                                .saturating_mul(path.len().saturating_add(1)),
                        )?;
                        summary
                            .external_after
                            .push((key.0, key.1, permission.sources.clone()));
                    }
                }
            }
        }
        if *reachable && let Terminator::Return(Some(id)) = block.terminator {
            let payload = &solution.values[id.0 as usize];
            summary
                .returned_enum_variants
                .join(&payload.enum_variants, b)?;
            summary
                .returned_variants
                .join(&payload.option_variants, b)?;
            if matches!(f.return_type, Type::Address { .. }) {
                result_row(&mut summary, &[], &payload.address.sources, b)?;
            }
            for (path, p) in &payload.leaves {
                result_row(&mut summary, path, &p.sources, b)?;
            }
            task_address::summarize(f, payload, &mut summary, b)?;
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
pub(super) fn check_mixed(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    plan: &CandidatePlan,
    mask: &[bool],
    b: &mut Budget,
) -> Check<()> {
    check_with_mask(module, graphs, plan, Some(mask), b)
}
fn check_exclusive_transfers(
    f: &TypedFunction,
    graph: &FunctionGraph,
    b: &mut Budget,
) -> Check<()> {
    // Reconstruct transfers from the raw CFG, without candidate roots or the
    // ordinary borrow verifier's separate function partition.
    let mut affine: Vec<bool> = f.values.iter().map(exclusive_carrier_value).collect();
    for instruction in f.blocks.iter().flat_map(|block| &block.instructions) {
        if transfers_address(&instruction.kind) {
            let (first, rest) = exclusive_consumers(&instruction.kind);
            for value in first.into_iter().chain(rest.iter().copied()) {
                affine[value.0 as usize] |= affine_stored_input(&f.values[value.0 as usize]);
            }
        }
    }
    if !affine.iter().any(|affine| *affine) {
        return Ok(());
    }
    let width = affine.len();
    b.spend(width.saturating_mul(f.blocks.len().saturating_add(2)))?;
    let mut outputs = vec![vec![false; width]; f.blocks.len()];
    loop {
        let mut changed = false;
        for (block_index, block) in f.blocks.iter().enumerate() {
            if !graph.reachable[block_index] {
                continue;
            }
            let mut used = vec![false; width];
            if block_index != f.entry.0 as usize {
                for &predecessor in &graph.predecessors[block_index] {
                    if graph.reachable[predecessor] {
                        b.spend(width)?;
                        for (state, old) in used.iter_mut().zip(&outputs[predecessor]) {
                            *state |= *old;
                        }
                    }
                }
            }
            for instruction in &block.instructions {
                if let Some(value) = instruction.result {
                    used[value.0 as usize] = false;
                }
                let (first, rest) = exclusive_consumers(&instruction.kind);
                for value in first.into_iter().chain(rest.iter().copied()) {
                    b.spend(1)?;
                    if affine[value.0 as usize]
                        && (exclusive_carrier_value(&f.values[value.0 as usize])
                            || transfers_address(&instruction.kind))
                    {
                        used[value.0 as usize] = true;
                    }
                }
            }
            if used != outputs[block_index] {
                outputs[block_index] = used;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    for (block_index, block) in f.blocks.iter().enumerate() {
        if !graph.reachable[block_index] {
            continue;
        }
        let mut used = vec![false; width];
        if block_index != f.entry.0 as usize {
            for &predecessor in &graph.predecessors[block_index] {
                if graph.reachable[predecessor] {
                    b.spend(width)?;
                    for (state, old) in used.iter_mut().zip(&outputs[predecessor]) {
                        *state |= *old;
                    }
                }
            }
        }
        for instruction in &block.instructions {
            if let Some(value) = instruction.result {
                used[value.0 as usize] = false;
            }
            let (first, rest) = exclusive_consumers(&instruction.kind);
            for value in instruction_operands(&instruction.kind) {
                b.spend(1)?;
                let index = value.0 as usize;
                if !affine[index] || !used[index] {
                    continue;
                }
                b.spend(rest.len().saturating_add(1))?;
                let consuming = (first == Some(value) || rest.contains(&value))
                    && (exclusive_carrier_value(&f.values[index])
                        || transfers_address(&instruction.kind));
                if !consuming {
                    return Err(err("exclusive stored SSA value was used after transfer"));
                }
            }
            for value in first.into_iter().chain(rest.iter().copied()) {
                let index = value.0 as usize;
                b.spend(1)?;
                if affine[index]
                    && (exclusive_carrier_value(&f.values[index])
                        || transfers_address(&instruction.kind))
                    && std::mem::replace(&mut used[index], true)
                {
                    return Err(err(
                        "exclusive stored carrier SSA value was transferred twice",
                    ));
                }
            }
        }
        if let Terminator::Return(Some(value)) = block.terminator
            && affine[value.0 as usize]
            && used[value.0 as usize]
        {
            return Err(err(
                "exclusive stored carrier result was already transferred",
            ));
        }
    }
    Ok(())
}
fn check_with_mask(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    plan: &CandidatePlan,
    mask: Option<&[bool]>,
    b: &mut Budget,
) -> Check<()> {
    let mixed_package = b.stored_package_instance;
    if mask.is_none() || b.store_option_results {
        if !mixed_package && !b.task_borrowed_results && !b.store_option_results {
            super::stored_owned::module(module, b)?;
        }
        super::stored_callables::definitions(module, b)?;
    }
    for (index, f) in module.functions.iter().enumerate() {
        if mask.is_some_and(|mask| !mask[index]) {
            continue;
        }
        check_exclusive_transfers(f, &graphs[f.id.0 as usize], b)?;
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
                && !(b.task_borrowed_results && f.call_kind == CallKind::Suspend)
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
            if !module.resources.is_empty()
                && !mixed_package
                && !(b.task_borrowed_results && task_address::shape(ty))
                && !b.store_option_results
            {
                super::stored_owned::value_type(ty, b)?;
            }
            let mixed_resource = if mixed_package {
                b.spend(module.resources.len())?;
                matches!(ty, Type::Managed(payload) | Type::Desc(payload)
                    if module.resources.iter().any(|definition| definition.payload == **payload))
                    || matches!(ty, Type::Address { inner, .. }
                        if matches!(inner.as_ref(), Type::Managed(payload)
                            if module.resources.iter().any(|definition| definition.payload == **payload)))
            } else {
                false
            };
            if !(super::stored_callables::value(module, ty)
                || b.task_borrowed_results && task_address::admitted_value(ty)
                || qualified(ty).is_some()
                || addressed(ty).is_some()
                || matches!(ty,Type::Address{inner,..}if inner.is_address_pointee())
                || ty.is_copy_value()
                || super::stored_owned::owner(ty)
                || super::stored_owned::description(ty)
                || b.store_option_results
                    && matches!(
                        ty,
                        Type::Package { .. }
                            | Type::OwnedStore { .. }
                            | Type::ByteBuffer
                            | Type::Managed(_)
                            | Type::Desc(_)
                            | Type::ErrorUnion { .. }
                    )
                || b.store_option_results
                    && matches!(ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Package { .. } | Type::OwnedStore { .. } | Type::Managed(_)))
                || mixed_resource
                || b.stored_enum_addresses
                    && enum_address::signature(ty)
                    && enum_address::ordinary(module, ty, b)?
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
    if mask.is_none() {
        super::stored_callables::roles(module, graphs, b)?;
    }
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
        for (index, yes) in mask.iter().enumerate() {
            if !yes {
                summaries[index].complete = true;
            }
        }
    }
    let mut solved: Vec<Option<Solution>> = (0..module.functions.len()).map(|_| None).collect();
    loop {
        let mut changed = false;
        let mut pending = Vec::new();
        b.spend(module.functions.len())?;
        for index in (0..module.functions.len()).rev() {
            if mask.is_some_and(|mask| !mask[index]) {
                continue;
            }
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
            if b.stored_slot_updates {
                b.spend(f.blocks.len())?;
                for block in &f.blocks {
                    b.spend(block.instructions.len())?;
                    for instruction in &block.instructions {
                        let InstructionKind::Call { function, .. } = &instruction.kind else {
                            continue;
                        };
                        let callee = module
                            .functions
                            .get(function.0 as usize)
                            .ok_or_else(|| err("stored call target is absent"))?;
                        let prior = summaries
                            .get(function.0 as usize)
                            .ok_or_else(|| err("stored call summary is absent"))?;
                        if !prior.complete {
                            summary.complete = false;
                        }
                        for (position, local) in callee.parameters.iter().enumerate() {
                            if let Type::Address {
                                mutable: true,
                                inner,
                            } = &callee.locals[local.0 as usize].ty
                                && let Some((_, shape)) = qualified(inner)
                            {
                                for slot in &shape.slots {
                                    b.spend(
                                        slot.path
                                            .len()
                                            .saturating_add(prior.external_after.len())
                                            .saturating_add(1),
                                    )?;
                                    if !prior.external_after.iter().any(|(p, path, _)| {
                                        *p == position as u32 && *path == slot.path
                                    }) {
                                        summary.complete = false;
                                    }
                                }
                            }
                        }
                    }
                }
            }
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
    if (mask.is_none() || b.store_option_results)
        && inactive.is_none()
        && let Some(proof) = enum_address::refine(module, graphs, &summaries, b)?
    {
        return solve_check(module, &proof.graphs, plan, Some(&proof.inactive), mask, b);
    }
    if (mask.is_none() || b.store_option_results)
        && inactive.is_none()
        && observed_none
        && let Some(proof) = option_address::refine(module, graphs, &summaries, b)?
    {
        return solve_check(module, &proof.graphs, plan, Some(&proof.inactive), mask, b);
    }
    let mut checked_task_sources = if b.task_borrowed_results {
        Some(Vec::with_capacity(module.functions.len()))
    } else {
        None
    };
    if b.store_option_results {
        b.spend(module.functions.len())?;
    }
    let mut checked_option_sources = b
        .store_option_results
        .then(|| Vec::with_capacity(module.functions.len()));
    for (index, (f, s)) in module.functions.iter().zip(&summaries).enumerate() {
        if mask.is_some_and(|mask| !mask[index]) {
            if let Some(checked) = &mut checked_option_sources {
                b.spend(f.values.len())?;
                checked.push(StoreOptionFunctionSources {
                    values: vec![Vec::new(); f.values.len()],
                });
            }
            if let Some(checked) = &mut checked_task_sources {
                b.spend(f.values.len())?;
                checked.push(TaskFunctionSources {
                    covered: false,
                    values: vec![TaskValueSources::default(); f.values.len()],
                });
            }
            continue;
        }
        if !s.complete {
            return Err(err(
                "stored/reference operation has no complete actual target permission",
            ));
        }
        enum_address::projections(f, &graphs[index], s, b)?;
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
                        != (if source.retained_port.is_some() {
                            task_address::source_relation(f, source, b)?
                        } else {
                            relation(f, StorageBase::Parameter(position), &source.leaf, b)?
                        })
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
        if b.store_option_results {
            b.spend(f.blocks.len())?;
            for (block, reachable) in f.blocks.iter().zip(&graphs[index].reachable) {
                if !reachable {
                    continue;
                }
                b.spend(block.instructions.len())?;
                for instruction in &block.instructions {
                    let InstructionKind::AddressField { .. } = instruction.kind else {
                        continue;
                    };
                    let result = instruction
                        .result
                        .ok_or_else(|| err("Store field result absent"))?;
                    if !matches!(vt(f, result)?, Type::Address { inner, .. }
                        if matches!(inner.as_ref(), Type::Package { .. }))
                    {
                        continue;
                    }
                    let sources = &solution
                        .values
                        .get(result.0 as usize)
                        .ok_or_else(|| err("Store field result source absent"))?
                        .address
                        .sources;
                    source_work(sources, b)?;
                    if sources.is_empty()
                        || sources.iter().any(|source| source.heap_element.is_none())
                    {
                        return Err(err(
                            "owning Store field address lacks an actual element window",
                        ));
                    }
                }
            }
        }
        if let Some(checked) = &mut checked_option_sources {
            b.spend(solution.values.len())?;
            let mut values = Vec::with_capacity(solution.values.len());
            for value in &solution.values {
                source_work(&value.address.sources, b)?;
                let mut sources = value.address.sources.clone();
                for (_, leaf) in &value.leaves {
                    add_sources(&mut sources, &leaf.sources, b)?;
                }
                values.push(sources);
            }
            checked.push(StoreOptionFunctionSources { values });
        }
        if let Some(checked) = checked_task_sources.as_mut() {
            checked.push(task_address::checked_sources(f, solution, b)?);
        }
        let claimed = &plan.functions[index].holds;
        if claimed.len() != solution.values.len() {
            return Err(err("candidate must cover each actual value once"));
        }
        for (v, (p, claim)) in solution.values.iter().zip(claimed).enumerate() {
            if b.store_option_results
                && !matches!(f.values[v], Type::BorrowedStorage { .. })
                && !p
                    .address
                    .sources
                    .iter()
                    .any(|source| source.heap_element.is_some())
                && !p
                    .leaves
                    .iter()
                    .flat_map(|(_, leaf)| &leaf.sources)
                    .any(|source| source.heap_element.is_some())
            {
                // Ordinary resource/borrow reconstruction checks this value's
                // public Hold; the stored lane owns only qualified carriers
                // and Store-element payload windows in this dual profile.
                continue;
            }
            let mut actual = BTreeSet::new();
            for root in p
                .address
                .sources
                .iter()
                .chain(p.store_description.sources.iter())
                .chain(p.leaves.iter().flat_map(|(_, p)| &p.sources))
                .chain(p.task_ports.iter().flat_map(|(_, p)| &p.sources))
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
    if let Some(checked) = checked_task_sources {
        b.task_stored_sources = Some(checked);
    }
    b.store_option_sources = checked_option_sources;
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
