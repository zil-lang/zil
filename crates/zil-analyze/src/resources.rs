//! Producer-side resource diagnostics. This module does not import verifier
//! code or produce trusted permission certificates. It uses sparse named cells
//! and explicit sets of possible lifecycle states, independently of Verify.
use super::*;
use std::cell::RefCell;
use std::collections::BTreeMap;
pub(super) mod product;
mod summary_graph;
pub(crate) mod sums;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Place {
    Local(LocalId),
    PackageField { local: LocalId, index: u32 },
    ArrayElement { local: LocalId, index: u32 },
    Value(ValueId),
    Capture(u32),
    CaptureField { capture: u32, index: u32 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Source {
    Head(LocalId),
    PackageField { local: LocalId, index: u32 },
    OwnedPackageField { local: LocalId, index: u32 },
    ArrayElement { local: LocalId, index: u32 },
    OwnedField(u32),
    Argument(u32),
    BorrowedField(u32),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Life {
    Vacant,
    Available,
    Transferred,
    Cleaned,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Birth {
    Created,
    Incoming,
}
type PackageValues = BTreeMap<(ValueId, u32), BTreeSet<Birth>>;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Fact {
    life: BTreeSet<Life>,
    sources: BTreeSet<Source>,
    birth: BTreeSet<Birth>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct State {
    cells: BTreeMap<Place, Fact>,
    later: BTreeSet<(LocalId, LocalId)>,
    // Complete SSA Packages carry the actual birth of each direct Managed
    // field. Local Packages already have separate lifecycle cells. Unknown
    // producers keep Incoming; packaging itself never creates a resource.
    package_values: PackageValues,
    product_births: product::Origins,
    sums: sums::Facts,
}
#[derive(Clone, Copy)]
struct Shape {
    owned: bool,
    description: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Effect {
    arguments: BTreeSet<u32>,
    borrowed_fields: BTreeSet<u32>,
}
type Shapes = BTreeMap<Place, Shape>;
pub(super) type Live = BTreeSet<Place>;

pub(super) fn owned_error(ty: &Type) -> bool {
    matches!(ty, Type::ErrorUnion { error, result }
        if matches!(error.as_ref(), Type::Package { .. }) || sums::owned(error)
            || matches!(result.as_ref(), Type::Package { .. } | Type::Managed(_)) || sums::owned(result))
}

fn managed_success(ty: &Type) -> bool {
    matches!(ty, Type::ErrorUnion { result, .. } if matches!(result.as_ref(), Type::Managed(_)))
}

fn error(message: &str) -> Vec<Diagnostic> {
    bad(format!("resource closure: {message}"))
}
fn exact(fact: &Fact, life: Life) -> bool {
    fact.life.len() == 1 && fact.life.contains(&life)
}
fn fact_cost(fact: &Fact, budget: &mut Budget) -> CompileResult<()> {
    budget.spend(
        fact.life
            .len()
            .saturating_add(fact.sources.len())
            .saturating_add(fact.birth.len())
            .saturating_add(1),
        "resource fact",
    )
}
fn copied(state: &State, place: Place, budget: &mut Budget) -> CompileResult<Fact> {
    budget.lookup(state.cells.len(), "resource lookup")?;
    let fact = state
        .cells
        .get(&place)
        .ok_or_else(|| error("missing typed resource cell"))?;
    fact_cost(fact, budget)?;
    Ok(fact.clone())
}
fn replace(state: &mut State, place: Place, fact: Fact, budget: &mut Budget) -> CompileResult<()> {
    budget.lookup(state.cells.len(), "resource update")?;
    if !state.cells.contains_key(&place) {
        return Err(error("resource update has no typed cell"));
    }
    state.cells.insert(place, fact);
    Ok(())
}
fn clone_state(state: &State, budget: &mut Budget) -> CompileResult<State> {
    budget.spend(
        state
            .cells
            .len()
            .saturating_add(state.later.len())
            .saturating_add(state.package_values.len()),
        "resource state clone",
    )?;
    for fact in state.cells.values() {
        fact_cost(fact, budget)?;
    }
    for birth in state.package_values.values() {
        budget.spend(birth.len(), "Package SSA birth clone")?;
    }
    product::copy_cost(&state.product_births, budget)?;
    state.sums.cost(budget)?;
    Ok(state.clone())
}
fn join(to: &mut State, from: &State, budget: &mut Budget) -> CompileResult<bool> {
    let mut changed = false;
    for (place, source) in &from.cells {
        budget.lookup(to.cells.len(), "resource join lookup")?;
        let target = to
            .cells
            .get_mut(place)
            .ok_or_else(|| error("incompatible resource states"))?;
        fact_cost(target, budget)?;
        fact_cost(source, budget)?;
        let before = (target.life.len(), target.sources.len(), target.birth.len());
        target.life.extend(&source.life);
        target.sources.extend(&source.sources);
        target.birth.extend(&source.birth);
        changed |= before != (target.life.len(), target.sources.len(), target.birth.len());
    }
    budget.spend(
        to.later.len().saturating_add(from.later.len()),
        "resource initialization order join",
    )?;
    let before = to.later.len();
    to.later.extend(&from.later);
    changed |= before != to.later.len();
    for (key, source) in &from.package_values {
        budget.lookup(to.package_values.len(), "Package SSA birth join lookup")?;
        let target = to
            .package_values
            .get_mut(key)
            .ok_or_else(|| error("incompatible Package SSA birth shapes"))?;
        budget.spend(
            target.len().saturating_add(source.len()),
            "Package SSA birth join",
        )?;
        let before = target.len();
        target.extend(source);
        changed |= before != target.len();
    }
    changed |= product::join(&mut to.product_births, &from.product_births, budget)?;
    changed |= to.sums.join(&from.sums, budget)?;
    Ok(changed)
}

fn package_value_shapes(
    f: &TypedFunction,
    budget: &mut Budget,
) -> CompileResult<(PackageValues, product::Origins)> {
    budget.spend(f.values.len(), "Package SSA birth shape scan")?;
    let mut fields = BTreeMap::new();
    let mut products = product::Origins::new();
    for (value, ty) in f.values.iter().enumerate() {
        if let Type::Package { fields: types, .. } = ty {
            budget.spend(types.len(), "Package SSA birth field scan")?;
            for (index, (_, ty)) in types.iter().enumerate() {
                if matches!(ty, Type::Managed(_)) {
                    budget.lookup(fields.len(), "Package SSA birth shape insertion")?;
                    budget.spend(2, "Package SSA birth allocation")?;
                    fields.insert(
                        (ValueId(value as u32), index as u32),
                        BTreeSet::from([Birth::Incoming]),
                    );
                } else if matches!(ty, Type::Package { .. }) {
                    product::add(
                        &mut products,
                        product::Carrier::Value(ValueId(value as u32)),
                        index as u32,
                        ty,
                        budget,
                    )?;
                }
            }
        }
    }
    Ok((fields, products))
}

fn package_birth(
    state: &State,
    value: ValueId,
    index: u32,
    budget: &mut Budget,
) -> CompileResult<BTreeSet<Birth>> {
    budget.lookup(state.package_values.len(), "Package SSA birth lookup")?;
    let birth = state
        .package_values
        .get(&(value, index))
        .ok_or_else(|| error("missing Package SSA birth field"))?;
    budget.spend(birth.len(), "Package SSA birth copy")?;
    Ok(birth.clone())
}

fn set_package_birth(
    state: &mut State,
    value: ValueId,
    index: u32,
    birth: BTreeSet<Birth>,
    budget: &mut Budget,
) -> CompileResult<()> {
    budget.lookup(state.package_values.len(), "Package SSA birth update")?;
    let target = state
        .package_values
        .get_mut(&(value, index))
        .ok_or_else(|| error("missing Package SSA birth field"))?;
    budget.spend(target.len(), "Package SSA birth replacement")?;
    *target = birth;
    Ok(())
}
fn shape(ty: &Type, module: &TypedModule, budget: &mut Budget) -> CompileResult<Shape> {
    Ok(match ty {
        ty if owned_error(ty) => Shape {
            owned: true,
            description: false,
        },
        Type::Managed(_) | Type::Package { .. } => Shape {
            owned: true,
            description: false,
        },
        Type::Array { element, length }
            if *length > 0
                && matches!(element.as_ref(), Type::Package { .. } | Type::Closure(_)) =>
        {
            Shape {
                owned: true,
                description: false,
            }
        }
        Type::Enum { .. } if budget.sum_type(ty)? => Shape {
            owned: true,
            description: false,
        },
        Type::Desc(_) => Shape {
            owned: false,
            description: true,
        },
        Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Closure(_)) => {
            let Type::Closure(id) = inner.as_ref() else {
                unreachable!()
            };
            let c = module
                .closures
                .get(id.0 as usize)
                .ok_or_else(|| error("absent borrowed closure shape"))?;
            budget.spend(c.captures.len(), "borrowed environment classification")?;
            Shape {
                owned: false,
                description: c.captures.iter().any(|t| matches!(t, Type::Desc(_))),
            }
        }
        Type::Closure(id) => {
            let c = module
                .closures
                .get(id.0 as usize)
                .ok_or_else(|| error("absent closure shape"))?;
            budget.spend(c.captures.len(), "resource environment classification")?;
            Shape {
                owned: c.kind == CallableKind::FnOnce
                    || c.captures.iter().any(|t| {
                        matches!(
                            t,
                            Type::Managed(_) | Type::Package { .. } | Type::Closure(_)
                        )
                    }),
                description: c.captures.iter().any(|t| matches!(t, Type::Desc(_))),
            }
        }
        _ => Shape {
            owned: false,
            description: false,
        },
    })
}
fn package_owned_field(ty: &Type, budget: &mut Budget) -> CompileResult<bool> {
    if matches!(ty, Type::Enum { .. } | Type::ErrorUnion { .. }) {
        if budget.sum_functions.is_empty() {
            return Ok(false);
        }
        budget.type_work(ty, false, "ordinary Sum field ownership classification")?;
        return Ok(sums::owned(ty));
    }
    Ok(matches!(
        ty,
        Type::Managed(_) | Type::Closure(_) | Type::Package { .. }
    ) || matches!(ty,Type::Array { element,length } if *length>0 && matches!(element.as_ref(),Type::Package { .. })))
}

fn shapes(module: &TypedModule, f: &TypedFunction, budget: &mut Budget) -> CompileResult<Shapes> {
    let mut result = BTreeMap::new();
    for (place, ty) in f.locals.iter().map(|l| (Place::Local(l.id), &l.ty)).chain(
        f.values
            .iter()
            .enumerate()
            .map(|(i, t)| (Place::Value(ValueId(i as u32)), t)),
    ) {
        budget.spend(1, "resource shape")?;
        let s = shape(ty, module, budget)?;
        if s.owned || s.description {
            budget.lookup(result.len(), "resource shape insertion")?;
            result.insert(place, s);
        }
        if let (Place::Local(local), Type::Package { fields, .. }) = (place, ty) {
            budget.spend(fields.len(), "package field shapes")?;
            for (index, (_, field)) in fields.iter().enumerate() {
                if package_owned_field(field, budget)? {
                    budget.lookup(result.len(), "package field shape insertion")?;
                    result.insert(
                        Place::PackageField {
                            local,
                            index: index as u32,
                        },
                        Shape {
                            owned: true,
                            description: false,
                        },
                    );
                }
            }
        }
        if let (Place::Local(local), Type::Array { element, length }) = (place, ty)
            && matches!(element.as_ref(), Type::Package { .. } | Type::Closure(_))
        {
            let width = match element.as_ref() {
                Type::Package { fields, .. } => fields.len().saturating_add(1),
                _ => 1,
            };
            budget.product(*length as usize, width, 1, "owning array element shapes")?;
            for index in 0..*length {
                budget.lookup(result.len(), "owning array element shape insertion")?;
                result.insert(
                    Place::ArrayElement { local, index },
                    Shape {
                        owned: true,
                        description: false,
                    },
                );
            }
        }
    }
    if let Some(id) = f.environment {
        let c = module
            .closures
            .get(id.0 as usize)
            .ok_or_else(|| error("absent entry environment"))?;
        for (index, ty) in c.captures.iter().enumerate() {
            budget.spend(1, "resource capture shape")?;
            if matches!(
                ty,
                Type::Managed(_) | Type::Package { .. } | Type::Closure(_)
            ) {
                budget.lookup(result.len(), "resource capture insertion")?;
                result.insert(
                    Place::Capture(index as u32),
                    Shape {
                        owned: true,
                        description: false,
                    },
                );
            }
            if let Type::Package { fields, .. } = ty {
                budget.spend(fields.len(), "capture Package field shapes")?;
                for (field, (_, ty)) in fields.iter().enumerate() {
                    if package_owned_field(ty, budget)? {
                        budget.lookup(result.len(), "capture Package field insertion")?;
                        result.insert(
                            Place::CaptureField {
                                capture: index as u32,
                                index: field as u32,
                            },
                            Shape {
                                owned: true,
                                description: false,
                            },
                        );
                    }
                }
            }
        }
    }
    Ok(result)
}
fn uses(kind: &InstructionKind) -> impl Iterator<Item = ValueId> + '_ {
    let (a, b, rest): (Option<ValueId>, Option<ValueId>, &[ValueId]) = match kind {
        InstructionKind::OwnedStoreReserve { store, capacity } => {
            (Some(*store), Some(*capacity), &[])
        }
        InstructionKind::OwnedStorePush { store, value } => (Some(*store), Some(*value), &[]),
        InstructionKind::OwnedStoreBorrow { store, index }
        | InstructionKind::OwnedStoreBorrowMut { store, index } => {
            (Some(*store), Some(*index), &[])
        }
        InstructionKind::OwnedStorePop { store }
        | InstructionKind::OwnedStoreLen { store }
        | InstructionKind::OwnedStoreCapacity { store } => (Some(*store), None, &[]),
        InstructionKind::Store { value, .. }
        | InstructionKind::ConvertInteger { value, .. }
        | InstructionKind::StorePackageField { value, .. }
        | InstructionKind::StoreOwnedArrayElement { value, .. }
        | InstructionKind::WriteCapture { value, .. } => (Some(*value), None, &[]),
        InstructionKind::Call { arguments, .. }
        | InstructionKind::ForeignCall { arguments, .. }
        | InstructionKind::CallClosureMut { arguments, .. }
        | InstructionKind::CallClosureShared { arguments, .. } => (None, None, arguments),
        InstructionKind::CallClosure { callee, arguments }
        | InstructionKind::CallClosureOnce { callee, arguments }
        | InstructionKind::CallClosureBorrowed {
            callee, arguments, ..
        }
        | InstructionKind::CallIndirect { callee, arguments } => (Some(*callee), None, arguments),
        InstructionKind::MakeClosure { captures, .. } => (None, None, captures),
        InstructionKind::MakeManaged { fields }
        | InstructionKind::MakePackage { fields }
        | InstructionKind::MakeAggregate { fields }
        | InstructionKind::MakeVariant { fields, .. } => (None, None, fields),
        InstructionKind::MakeOwnedArray { elements } => (None, None, elements),
        InstructionKind::ReadDesc { desc, .. } => (Some(*desc), None, &[]),
        InstructionKind::ByteBufferLen { buffer }
        | InstructionKind::ByteBufferCapacity { buffer }
        | InstructionKind::ByteBufferBorrow { buffer, .. } => (Some(*buffer), None, &[]),
        InstructionKind::ByteBufferReserve { buffer, capacity } => {
            (Some(*buffer), Some(*capacity), &[])
        }
        InstructionKind::ByteBufferTruncate { buffer, length } => {
            (Some(*buffer), Some(*length), &[])
        }
        InstructionKind::ByteBufferPush { buffer, value } => (Some(*buffer), Some(*value), &[]),
        InstructionKind::WriteDesc { desc, value, .. } => (Some(*desc), Some(*value), &[]),
        InstructionKind::Unary { operand, .. } => (Some(*operand), None, &[]),
        InstructionKind::Binary { left, right, .. } => (Some(*left), Some(*right), &[]),
        InstructionKind::ReadAddress { address }
        | InstructionKind::SliceLength { address }
        | InstructionKind::ReborrowAddress { address, .. }
        | InstructionKind::AddressField { address, .. } => (Some(*address), None, &[]),
        InstructionKind::WriteAddress { address, value } => (Some(*address), Some(*value), &[]),
        InstructionKind::AddressIndex { address, index } => (Some(*address), Some(*index), &[]),
        InstructionKind::AddressSlice {
            address,
            start,
            end,
        } => (Some(*address), *start, end.as_slice()),
        InstructionKind::ExtractField { aggregate, .. } => (Some(*aggregate), None, &[]),
        InstructionKind::ExtractIndex { aggregate, index } => (Some(*aggregate), Some(*index), &[]),
        InstructionKind::UpdateField {
            aggregate, value, ..
        } => (Some(*aggregate), Some(*value), &[]),
        InstructionKind::UpdateIndex {
            aggregate,
            index,
            value,
        } => (Some(*aggregate), Some(*index), std::slice::from_ref(value)),
        InstructionKind::MakeErrorUnion { value, .. }
        | InstructionKind::ErrorTag { value }
        | InstructionKind::ReadErrorUnion { value, .. }
        | InstructionKind::TakeErrorUnion { value, .. }
        | InstructionKind::VariantTag { value }
        | InstructionKind::ReadVariant { value, .. } => (Some(*value), None, &[]),
        InstructionKind::ByteBufferNew
        | InstructionKind::RegisterDefer { .. }
        | InstructionKind::VariantTagLocal { .. }
        | InstructionKind::ReadVariantLocal { .. }
        | InstructionKind::UnpackVariant { .. }
        | InstructionKind::DropEnum { .. }
        | InstructionKind::OwnedStoreNew { .. }
        | InstructionKind::Constant(_)
        | InstructionKind::Load(_)
        | InstructionKind::TakeLocal { .. }
        | InstructionKind::BorrowLocal { .. }
        | InstructionKind::BorrowLocalMut { .. }
        | InstructionKind::DescribeLocal { .. }
        | InstructionKind::ReadPackageField { .. }
        | InstructionKind::DescribePackageField { .. }
        | InstructionKind::TakePackageField { .. }
        | InstructionKind::DropPackageField { .. }
        | InstructionKind::ProjectOwnedArrayField { .. }
        | InstructionKind::TakeOwnedArrayElement { .. }
        | InstructionKind::DropOwnedArrayElement { .. }
        | InstructionKind::DropOwnedArray { .. }
        | InstructionKind::BorrowOwnedArrayElement { .. }
        | InstructionKind::BorrowPackageField { .. }
        | InstructionKind::BorrowCapture { .. }
        | InstructionKind::DescribeCapture { .. }
        | InstructionKind::TakeCapture { .. }
        | InstructionKind::ReadCapture { .. }
        | InstructionKind::BindPackageCapture { .. }
        | InstructionKind::EndPackageCapture { .. }
        | InstructionKind::DropManaged { .. }
        | InstructionKind::DropPackage { .. }
        | InstructionKind::DropErrorUnion { .. }
        | InstructionKind::DropClosure { .. }
        | InstructionKind::StorageLive(_)
        | InstructionKind::StorageDead(_)
        | InstructionKind::UnsafeEnter { .. }
        | InstructionKind::UnsafeExit { .. }
        | InstructionKind::ScopeEnter { .. }
        | InstructionKind::ScopeExit { .. }
        | InstructionKind::FunctionRef { .. }
        | InstructionKind::ForeignFunctionRef { .. }
        | InstructionKind::Unsupported(_) => (None, None, &[]),
    };
    a.into_iter().chain(b).chain(rest.iter().copied())
}
fn reverse(
    live: &mut Live,
    i: &Instruction,
    shapes: &Shapes,
    budget: &mut Budget,
) -> CompileResult<()> {
    budget.lookup(
        live.len().saturating_add(shapes.len()),
        "description liveness instruction",
    )?;
    if let Some(v) = i.result {
        live.remove(&Place::Value(v));
    }
    match i.kind {
        InstructionKind::Store { local, .. }
        | InstructionKind::StorageLive(local)
        | InstructionKind::StorageDead(local)
        | InstructionKind::BindPackageCapture { local, .. }
        | InstructionKind::EndPackageCapture { local, .. }
        | InstructionKind::DropClosure { local } => {
            live.remove(&Place::Local(local));
        }
        InstructionKind::Load(local)
        | InstructionKind::TakeLocal { local }
        | InstructionKind::CallClosureShared { callee: local, .. }
        | InstructionKind::CallClosureMut { callee: local, .. }
            if shapes
                .get(&Place::Local(local))
                .is_some_and(|s| s.description) =>
        {
            live.insert(Place::Local(local));
        }
        _ => {}
    }
    for v in uses(&i.kind) {
        budget.lookup(
            live.len().saturating_add(shapes.len()),
            "description operand liveness",
        )?;
        if shapes.get(&Place::Value(v)).is_some_and(|s| s.description) {
            live.insert(Place::Value(v));
        }
    }
    Ok(())
}
fn next(block: &BasicBlock) -> Vec<usize> {
    match block.terminator {
        Terminator::Jump(b) => vec![b.0 as usize],
        Terminator::Branch {
            then_block,
            else_block,
            ..
        } => vec![then_block.0 as usize, else_block.0 as usize],
        _ => vec![],
    }
}
fn afters(
    f: &TypedFunction,
    reachable: &[bool],
    shapes: &Shapes,
    budget: &mut Budget,
) -> CompileResult<Vec<Vec<Live>>> {
    budget.product(f.blocks.len(), 2, 1, "resource liveness blocks")?;
    let mut entry = vec![Live::new(); f.blocks.len()];
    let mut exit = entry.clone();
    loop {
        let mut changed = false;
        for (b, block) in f.blocks.iter().enumerate().rev() {
            if !reachable[b] {
                continue;
            }
            let mut live = Live::new();
            for successor in next(block) {
                let incoming = entry
                    .get(successor)
                    .ok_or_else(|| error("invalid resource CFG edge"))?;
                budget.spend(
                    live.len().saturating_add(incoming.len()),
                    "description liveness union",
                )?;
                live.extend(incoming);
            }
            budget.spend(live.len(), "description liveness exit clone")?;
            exit[b] = live.clone();
            for instruction in block.instructions.iter().rev() {
                reverse(&mut live, instruction, shapes, budget)?;
            }
            budget.spend(
                live.len().saturating_add(entry[b].len()).saturating_add(1),
                "description liveness convergence",
            )?;
            if live != entry[b] {
                changed = true;
                entry[b] = live;
            }
        }
        if !changed {
            break;
        }
    }
    let mut result = vec![];
    for (b, block) in f.blocks.iter().enumerate() {
        budget.spend(block.instructions.len(), "description liveness history")?;
        let mut rows = vec![Live::new(); block.instructions.len()];
        let mut live = std::mem::take(&mut exit[b]);
        for (j, i) in block.instructions.iter().enumerate().rev() {
            budget.spend(live.len(), "description liveness history clone")?;
            rows[j] = live.clone();
            reverse(&mut live, i, shapes, budget)?;
        }
        result.push(rows);
    }
    Ok(result)
}

/// Strong address windows for the newly qualified owned-Store profile. The
/// caller supplies actual private Hold roots, never the exported candidate.
/// Reuse the independently reconstructed last-use walk; an ordinary Desc is
/// deliberately absent because a weak resource description is not a loan.
pub(super) fn address_afters(
    f: &TypedFunction,
    reachable: &[bool],
    budget: &mut Budget,
) -> CompileResult<Vec<Vec<Live>>> {
    budget.spend(
        f.locals.len().saturating_add(f.values.len()),
        "captured Store address shape scan",
    )?;
    let mut shapes = BTreeMap::new();
    for (place, ty) in f.locals.iter().map(|l| (Place::Local(l.id), &l.ty)).chain(
        f.values
            .iter()
            .enumerate()
            .map(|(index, ty)| (Place::Value(ValueId(index as u32)), ty)),
    ) {
        if matches!(ty, Type::Address { .. }) {
            budget.lookup(shapes.len(), "captured Store address shape insertion")?;
            shapes.insert(
                place,
                Shape {
                    owned: false,
                    description: true,
                },
            );
        }
    }
    if shapes.is_empty() {
        return Ok(Vec::new());
    }
    afters(f, reachable, &shapes, budget)
}
fn live_sources(state: &State, fact: &Fact, budget: &mut Budget) -> CompileResult<()> {
    if fact.sources.is_empty() {
        return Err(error("description has no actual source"));
    }
    for source in &fact.sources {
        budget.spend(1, "description source")?;
        let place = match source {
            Source::Head(l) => Some(Place::Local(*l)),
            Source::PackageField { local, index } | Source::OwnedPackageField { local, index } => {
                if !exact(
                    &copied(state, Place::Local(*local), budget)?,
                    Life::Available,
                ) {
                    return Err(error(
                        "description may refer to a moved or cleaned package shell",
                    ));
                }
                Some(Place::PackageField {
                    local: *local,
                    index: *index,
                })
            }
            Source::OwnedField(i) => Some(Place::Capture(*i)),
            Source::ArrayElement { local, index } => {
                if !exact(
                    &copied(state, Place::Local(*local), budget)?,
                    Life::Available,
                ) {
                    return Err(error(
                        "description may refer to a moved or cleaned array shell",
                    ));
                }
                Some(Place::ArrayElement {
                    local: *local,
                    index: *index,
                })
            }
            _ => None,
        };
        if let Some(place) = place
            && !exact(&copied(state, place, budget)?, Life::Available)
        {
            return Err(error("description may refer to a moved or cleaned head"));
        }
    }
    Ok(())
}
fn no_later_use(
    state: &State,
    live: &Live,
    source: Source,
    budget: &mut Budget,
) -> CompileResult<()> {
    for place in live {
        let fact = copied(state, *place, budget)?;
        budget.spend(fact.sources.len(), "description owner matching")?;
        if fact.sources.iter().any(|actual| {
            *actual == source
                || matches!((source, actual),
                (Source::Head(owner), Source::PackageField { local, .. } | Source::OwnedPackageField { local, .. } | Source::ArrayElement { local, .. }) if owner == *local)
        }) {
            return Err(error(
                "head transition invalidates a description required later",
            ));
        }
    }
    Ok(())
}
fn forget_order(state: &mut State, local: LocalId, budget: &mut Budget) -> CompileResult<()> {
    budget.spend(state.later.len(), "resource order removal")?;
    state.later.retain(|(a, b)| *a != local && *b != local);
    Ok(())
}

// Only local aggregate slots can be partial. Every SSA carrier and function ABI
// value remains complete, so whole movement checks every responsibility slot.
fn package_transition(
    f: &TypedFunction,
    state: &mut State,
    local: LocalId,
    life: Life,
    check: bool,
    budget: &mut Budget,
) -> CompileResult<()> {
    let (fields, count) = match f.locals.get(local.0 as usize).map(|l| &l.ty) {
        Some(Type::Package { fields, .. }) => (Some(fields), fields.len()),
        Some(Type::Array { element, length })
            if matches!(element.as_ref(), Type::Package { .. } | Type::Closure(_)) =>
        {
            (None, *length as usize)
        }
        _ => return Ok(()),
    };
    budget.spend(count, "owned aggregate slot lifecycle")?;
    for index in 0..count {
        let place = if let Some(fields) = fields {
            if !package_owned_field(&fields[index].1, budget)? {
                continue;
            }
            Place::PackageField {
                local,
                index: index as u32,
            }
        } else {
            Place::ArrayElement {
                local,
                index: index as u32,
            }
        };
        let mut fact = copied(state, place, budget)?;
        match life {
            Life::Transferred => {
                if check && !exact(&fact, Life::Available) {
                    return Err(error(
                        "whole aggregate move requires every owned element or field live",
                    ));
                }
                fact.life = BTreeSet::from([Life::Transferred]);
            }
            Life::Available => {
                if check && fact.life.contains(&Life::Available) {
                    return Err(error("whole package store overwrites a live field"));
                }
                fact.life = BTreeSet::from([Life::Available]);
                fact.birth = BTreeSet::from([Birth::Incoming]);
            }
            Life::Cleaned => {
                if fact.life.remove(&Life::Available) {
                    fact.life.insert(Life::Cleaned);
                }
            }
            Life::Vacant => {
                if check && fact.life.contains(&Life::Available) {
                    return Err(error("storage boundary discards an owned package field"));
                }
                fact = Fact {
                    life: BTreeSet::from([Life::Vacant]),
                    ..Fact::default()
                };
            }
        }
        fact.sources.clear();
        replace(state, place, fact, budget)?;
    }
    Ok(())
}
fn record_write(
    sources: &BTreeSet<Source>,
    effect: &mut Effect,
    shared: bool,
    check: bool,
    budget: &mut Budget,
) -> CompileResult<()> {
    for source in sources {
        budget.lookup(
            effect
                .arguments
                .len()
                .saturating_add(effect.borrowed_fields.len()),
            "description write effect",
        )?;
        match source {
            Source::OwnedField(_) | Source::OwnedPackageField { .. } if shared && check => {
                return Err(error(
                    "shared owned capture is written through its description",
                ));
            }
            Source::Argument(p) => {
                effect.arguments.insert(*p);
            }
            Source::BorrowedField(p) => {
                effect.borrowed_fields.insert(*p);
            }
            _ => {}
        }
    }
    Ok(())
}
struct Flow<'a> {
    module: &'a TypedModule,
    function: &'a TypedFunction,
    shapes: &'a Shapes,
    after: &'a [Vec<Live>],
    effects: &'a [Effect],
    views: &'a BTreeMap<LocalId, u32>,
    dependencies: Option<&'a summary_graph::Dependencies>,
    sums: Option<&'a sums::Plan>,
}
impl Flow<'_> {
    fn package_source(
        &self,
        local: LocalId,
        index: u32,
        budget: &mut Budget,
    ) -> CompileResult<Source> {
        if !self.views.is_empty() {
            budget.lookup(self.views.len(), "Package description capture root")?;
            if self.views.contains_key(&local) {
                return Ok(Source::OwnedPackageField { local, index });
            }
        }
        Ok(Source::PackageField { local, index })
    }
    fn consume(
        &self,
        state: &mut State,
        value: ValueId,
        check: bool,
        budget: &mut Budget,
    ) -> CompileResult<()> {
        let place = Place::Value(value);
        if self.shapes.get(&place).is_some_and(|s| s.owned) {
            let mut fact = copied(state, place, budget)?;
            if check && !exact(&fact, Life::Available) {
                return Err(error("SSA responsibility was already consumed or absent"));
            }
            fact.life = BTreeSet::from([Life::Transferred]);
            replace(state, place, fact, budget)?;
        }
        Ok(())
    }
    fn run(
        &self,
        b: usize,
        state: &mut State,
        effect: &mut Effect,
        check: bool,
        budget: &mut Budget,
    ) -> CompileResult<()> {
        let f = self.function;
        let callable = f
            .environment
            .map(|id| self.module.closures[id.0 as usize].kind);
        let shared = callable == Some(CallableKind::Fn);
        for (position, i) in f.blocks[b].instructions.iter().enumerate() {
            budget.spend(1, "resource instruction")?;
            if check && shared && !self.views.is_empty() {
                let changed = match i.kind {
                    InstructionKind::TakeLocal { local }
                    | InstructionKind::Store { local, .. }
                    | InstructionKind::DropPackage { local }
                    | InstructionKind::TakePackageField { local, .. }
                    | InstructionKind::DropPackageField { local, .. }
                    | InstructionKind::StorePackageField { local, .. }
                    | InstructionKind::BorrowPackageField {
                        local,
                        mutable: true,
                        ..
                    } => Some(local),
                    _ => None,
                };
                budget.lookup(self.views.len(), "Shared Package view mutation")?;
                if changed.is_some_and(|local| self.views.contains_key(&local)) {
                    return Err(error(
                        "Shared entry cannot change a Package capture payload or responsibility",
                    ));
                }
            }
            for value in uses(&i.kind) {
                if f.values.get(value.0 as usize).is_none() {
                    return Err(error("operand is outside the value table"));
                }
                budget.lookup(self.shapes.len(), "resource operand shape")?;
                if check
                    && self
                        .shapes
                        .get(&Place::Value(value))
                        .is_some_and(|s| s.description)
                {
                    live_sources(state, &copied(state, Place::Value(value), budget)?, budget)?;
                }
            }
            let output = i.result.map(Place::Value);
            if i.result
                .is_some_and(|v| f.values.get(v.0 as usize).is_none())
            {
                return Err(error("result is outside the value table"));
            }
            let out_shape = output.and_then(|p| self.shapes.get(&p));
            let mut produced: Option<Fact> = None;
            if check
                && out_shape.is_some_and(|s| s.owned)
                && copied(state, output.unwrap(), budget)?
                    .life
                    .contains(&Life::Available)
            {
                return Err(error(
                    "loop recreates an SSA carrier with an unpaid responsibility",
                ));
            }
            if let Some((store, true)) = dynamic_store::access(&i.kind) {
                record_write(
                    &copied(state, Place::Value(store), budget)?.sources,
                    effect,
                    shared,
                    check,
                    budget,
                )?;
            }
            match &i.kind {
                InstructionKind::VariantTagLocal { local, .. }
                | InstructionKind::ReadVariantLocal { local, .. } => {
                    if check
                        && self.shapes.contains_key(&Place::Local(*local))
                        && !exact(
                            &copied(state, Place::Local(*local), budget)?,
                            Life::Available,
                        )
                    {
                        return Err(error("Sum observation needs a live local"));
                    }
                }
                InstructionKind::UnpackVariant {
                    local,
                    variant,
                    fields,
                } => {
                    let plan = self
                        .sums
                        .ok_or_else(|| error("Unpack has no finite Sum plan"))?;
                    plan.validate_unpack(f, i, &state.sums, check, budget)?;
                    let mut source = copied(state, Place::Local(*local), budget)?;
                    if check {
                        if !exact(&source, Life::Available) {
                            return Err(error("Unpack source is not a complete live owner"));
                        }
                        no_later_use(
                            state,
                            &self.after[b][position],
                            Source::Head(*local),
                            budget,
                        )?;
                        for target in fields {
                            if self.shapes.contains_key(&Place::Local(*target))
                                && copied(state, Place::Local(*target), budget)?
                                    .life
                                    .contains(&Life::Available)
                            {
                                return Err(error("Unpack overwrites a live target"));
                            }
                            no_later_use(
                                state,
                                &self.after[b][position],
                                Source::Head(*target),
                                budget,
                            )?;
                        }
                    }
                    source.life = BTreeSet::from([Life::Transferred]);
                    replace(state, Place::Local(*local), source, budget)?;
                    forget_order(state, *local, budget)?;
                    for (index, target) in fields.iter().enumerate().rev() {
                        if self
                            .shapes
                            .get(&Place::Local(*target))
                            .is_some_and(|s| s.owned)
                        {
                            let birth = state.sums.unpack_birth(*local, *variant, index, budget)?;
                            forget_order(state, *target, budget)?;
                            for (place, fact) in &state.cells {
                                budget.spend(1, "Unpack actual initialization order")?;
                                if let Place::Local(older) = place
                                    && older != target
                                    && fact.life.contains(&Life::Available)
                                {
                                    budget.lookup(state.later.len(), "Unpack order insertion")?;
                                    state.later.insert((*older, *target));
                                }
                            }
                            replace(
                                state,
                                Place::Local(*target),
                                Fact {
                                    life: BTreeSet::from([Life::Available]),
                                    birth,
                                    sources: BTreeSet::new(),
                                },
                                budget,
                            )?;
                            package_transition(f, state, *target, Life::Available, false, budget)?;
                        }
                    }
                }
                InstructionKind::MakeVariant { fields, .. }
                    if out_shape.is_some_and(|s| s.owned) =>
                {
                    for value in fields {
                        self.consume(state, *value, check, budget)?;
                    }
                    produced = Some(Fact {
                        life: BTreeSet::from([Life::Available]),
                        birth: BTreeSet::from([Birth::Incoming]),
                        sources: BTreeSet::new(),
                    });
                }
                InstructionKind::OwnedStorePush { value, .. } => {
                    self.consume(state, *value, check, budget)?;
                    produced = Some(Fact {
                        life: BTreeSet::from([Life::Available]),
                        birth: BTreeSet::from([Birth::Incoming]),
                        sources: BTreeSet::new(),
                    });
                }
                InstructionKind::OwnedStorePop { .. } => {
                    produced = Some(Fact {
                        life: BTreeSet::from([Life::Available]),
                        birth: BTreeSet::from([Birth::Incoming]),
                        sources: BTreeSet::new(),
                    });
                }
                InstructionKind::MakeErrorUnion { value, is_error }
                    if out_shape.is_some_and(|s| s.owned) =>
                {
                    // Birth belongs to the selected success owner, not to the
                    // wrapper. A Copy error has no success responsibility.
                    let birth = if i
                        .result
                        .is_some_and(|v| managed_success(&f.values[v.0 as usize]))
                    {
                        if *is_error {
                            BTreeSet::new()
                        } else {
                            copied(state, Place::Value(*value), budget)?.birth
                        }
                    } else {
                        BTreeSet::from([Birth::Incoming])
                    };
                    self.consume(state, *value, check, budget)?;
                    produced = Some(Fact {
                        life: BTreeSet::from([Life::Available]),
                        birth,
                        sources: BTreeSet::new(),
                    });
                }
                InstructionKind::TakeErrorUnion { value, .. } => {
                    if !f.values.get(value.0 as usize).is_some_and(owned_error) {
                        return Err(error("TakeErrorUnion requires an owned error-union value"));
                    }
                    let birth = if managed_success(&f.values[value.0 as usize]) {
                        copied(state, Place::Value(*value), budget)?.birth
                    } else {
                        BTreeSet::from([Birth::Incoming])
                    };
                    self.consume(state, *value, check, budget)?;
                    if out_shape.is_some_and(|s| s.owned) {
                        produced = Some(Fact {
                            life: BTreeSet::from([Life::Available]),
                            birth,
                            sources: BTreeSet::new(),
                        });
                    }
                }
                InstructionKind::ErrorTag { value }
                    if f.values.get(value.0 as usize).is_some_and(owned_error) =>
                {
                    if check
                        && !exact(
                            &copied(state, Place::Value(*value), budget)?,
                            Life::Available,
                        )
                    {
                        return Err(error("error tag requires a live owned union"));
                    }
                }
                InstructionKind::BindPackageCapture { local, index } => {
                    let whole = copied(state, Place::Capture(*index), budget)?;
                    if check && !exact(&whole, Life::Available) {
                        return Err(error(
                            "Package view binding requires a complete live capture",
                        ));
                    }
                    replace(state, Place::Local(*local), whole, budget)?;
                    replace(
                        state,
                        Place::Capture(*index),
                        Fact {
                            life: BTreeSet::from([Life::Vacant]),
                            ..Fact::default()
                        },
                        budget,
                    )?;
                    let Type::Package { fields, .. } = &f.locals[local.0 as usize].ty else {
                        return Err(error("Package view binding type is not a Package"));
                    };
                    budget.spend(fields.len(), "Package capture binding fields")?;
                    for (field, (_, ty)) in fields.iter().enumerate() {
                        if !package_owned_field(ty, budget)? {
                            continue;
                        }
                        let source = Place::CaptureField {
                            capture: *index,
                            index: field as u32,
                        };
                        let fact = copied(state, source, budget)?;
                        if check && !exact(&fact, Life::Available) {
                            return Err(error("Package capture entry has an absent field"));
                        }
                        replace(
                            state,
                            Place::PackageField {
                                local: *local,
                                index: field as u32,
                            },
                            fact,
                            budget,
                        )?;
                        replace(
                            state,
                            source,
                            Fact {
                                life: BTreeSet::from([Life::Vacant]),
                                ..Fact::default()
                            },
                            budget,
                        )?;
                    }
                }
                InstructionKind::EndPackageCapture { local, index } => {
                    let whole = copied(state, Place::Local(*local), budget)?;
                    if check {
                        if !exact(
                            &copied(state, Place::Capture(*index), budget)?,
                            Life::Vacant,
                        ) {
                            return Err(error("Package view ends without an active binding"));
                        }
                        no_later_use(
                            state,
                            &self.after[b][position],
                            Source::Head(*local),
                            budget,
                        )?;
                    }
                    replace(state, Place::Capture(*index), whole, budget)?;
                    replace(
                        state,
                        Place::Local(*local),
                        Fact {
                            life: BTreeSet::from([Life::Vacant]),
                            ..Fact::default()
                        },
                        budget,
                    )?;
                    let Type::Package { fields, .. } = &f.locals[local.0 as usize].ty else {
                        return Err(error("Package view end type is not a Package"));
                    };
                    budget.spend(fields.len(), "Package capture end fields")?;
                    for (field, (_, ty)) in fields.iter().enumerate() {
                        if !package_owned_field(ty, budget)? {
                            continue;
                        }
                        let source = Place::PackageField {
                            local: *local,
                            index: field as u32,
                        };
                        let fact = copied(state, source, budget)?;
                        replace(
                            state,
                            Place::CaptureField {
                                capture: *index,
                                index: field as u32,
                            },
                            fact,
                            budget,
                        )?;
                        replace(
                            state,
                            source,
                            Fact {
                                life: BTreeSet::from([Life::Vacant]),
                                ..Fact::default()
                            },
                            budget,
                        )?;
                    }
                    forget_order(state, *local, budget)?;
                }
                InstructionKind::OwnedStoreNew { .. }
                | InstructionKind::MakeManaged { .. }
                | InstructionKind::ByteBufferNew => {
                    produced = Some(Fact {
                        life: BTreeSet::from([Life::Available]),
                        birth: BTreeSet::from([Birth::Created]),
                        sources: BTreeSet::new(),
                    });
                }
                InstructionKind::MakePackage { fields } => {
                    let Some(Type::Package {
                        fields: expected, ..
                    }) = i.result.and_then(|v| f.values.get(v.0 as usize))
                    else {
                        return Err(error("MakePackage requires a package result"));
                    };
                    budget.spend(expected.len(), "package assembly fields")?;
                    if fields.len() != expected.len() {
                        return Err(error("package field arity mismatch"));
                    }
                    for (index, (value, (_, ty))) in fields.iter().zip(expected).enumerate() {
                        budget.type_work(ty, false, "package assembly type comparison")?;
                        if f.values.get(value.0 as usize) != Some(ty) {
                            return Err(error("package field type mismatch"));
                        }
                        if matches!(ty, Type::Managed(_)) {
                            let birth = copied(state, Place::Value(*value), budget)?.birth;
                            set_package_birth(
                                state,
                                i.result.expect("checked Package result"),
                                index as u32,
                                birth,
                                budget,
                            )?;
                        } else if matches!(ty, Type::Package { .. }) {
                            product::transfer(
                                state,
                                (product::Carrier::Value(*value), &[]),
                                (
                                    product::Carrier::Value(
                                        i.result.expect("checked Package result"),
                                    ),
                                    &[index as u32],
                                ),
                                ty,
                                budget,
                            )?;
                        }
                        self.consume(state, *value, check, budget)?;
                    }
                    produced = Some(Fact {
                        life: BTreeSet::from([Life::Available]),
                        birth: BTreeSet::from([Birth::Incoming]),
                        sources: BTreeSet::new(),
                    });
                }
                InstructionKind::MakeOwnedArray { elements } => {
                    let Some(Type::Array { element, length }) =
                        i.result.and_then(|v| f.values.get(v.0 as usize))
                    else {
                        return Err(error("owning array construction requires an array result"));
                    };
                    if *length == 0
                        || !matches!(element.as_ref(), Type::Package { .. } | Type::Closure(_))
                        || elements.len() != *length as usize
                    {
                        return Err(error(
                            "owning array construction requires every Package element",
                        ));
                    }
                    budget.spend(elements.len(), "owning array assembly elements")?;
                    for value in elements {
                        budget.type_work(element, false, "owning array element type comparison")?;
                        if f.values.get(value.0 as usize) != Some(element.as_ref()) {
                            return Err(error("owning array element type mismatch"));
                        }
                        self.consume(state, *value, check, budget)?;
                    }
                    produced = Some(Fact {
                        life: BTreeSet::from([Life::Available]),
                        birth: BTreeSet::from([Birth::Incoming]),
                        sources: BTreeSet::new(),
                    });
                }
                InstructionKind::ProjectOwnedArrayField { local, index, .. }
                | InstructionKind::TakeOwnedArrayElement { local, index }
                | InstructionKind::DropOwnedArrayElement { local, index }
                | InstructionKind::StoreOwnedArrayElement { local, index, .. }
                | InstructionKind::BorrowOwnedArrayElement { local, index, .. } => {
                    let Some(decl) = f.locals.get(local.0 as usize) else {
                        return Err(error("owning array operation requires a declared local"));
                    };
                    let Type::Array { element, length } = &decl.ty else {
                        return Err(error(
                            "owning array element operation requires an array local",
                        ));
                    };
                    let fields = match element.as_ref() {
                        Type::Package { fields, .. } => Some(fields),
                        Type::Closure(_) => None,
                        _ => {
                            return Err(error(
                                "owning array element requires a Package or concrete Closure",
                            ));
                        }
                    };
                    if *index >= *length {
                        return Err(error(
                            "owning array element index is outside the fixed length",
                        ));
                    }
                    if check
                        && !exact(
                            &copied(state, Place::Local(*local), budget)?,
                            Life::Available,
                        )
                    {
                        return Err(error(
                            "owning array element operation requires a live shell",
                        ));
                    }
                    let place = Place::ArrayElement {
                        local: *local,
                        index: *index,
                    };
                    let mut fact = copied(state, place, budget)?;
                    let result = i.result.and_then(|v| f.values.get(v.0 as usize));
                    match i.kind {
                        InstructionKind::ProjectOwnedArrayField { field, .. } => {
                            let Some((_, ty)) =
                                fields.and_then(|fields| fields.get(field as usize))
                            else {
                                return Err(error(
                                    "owning array Package field is outside its declaration",
                                ));
                            };
                            budget.type_work(ty, true, "owning array field projection type")?;
                            let expected = match ty {
                                Type::Managed(payload) => Type::Desc(payload.clone()),
                                ty if ty.is_copy_value() => ty.clone(),
                                _ => {
                                    return Err(error(
                                        "owning array projection requires Copy or Managed field",
                                    ));
                                }
                            };
                            if result != Some(&expected) {
                                return Err(error("owning array projection result type mismatch"));
                            }
                            if check && !exact(&fact, Life::Available) {
                                return Err(error(
                                    "owning array field projection requires a live element",
                                ));
                            }
                            if matches!(ty, Type::Managed(_)) {
                                produced = Some(Fact {
                                    sources: BTreeSet::from([Source::ArrayElement {
                                        local: *local,
                                        index: *index,
                                    }]),
                                    ..Fact::default()
                                });
                            }
                        }
                        InstructionKind::BorrowOwnedArrayElement { mutable, .. } => {
                            if !matches!(element.as_ref(), Type::Closure(_))
                                || (mutable && !decl.mutable)
                                || !matches!(result, Some(Type::Address { mutable: actual, inner }) if *actual == mutable && inner == element)
                            {
                                return Err(error(
                                    "callback array borrow requires the exact element address and permission",
                                ));
                            }
                            if check && !exact(&fact, Life::Available) {
                                return Err(error(
                                    "callback array borrow requires a live element on every path",
                                ));
                            }
                        }
                        InstructionKind::TakeOwnedArrayElement { .. } => {
                            budget.type_work(element, false, "owning array take type")?;
                            if result != Some(element.as_ref()) {
                                return Err(error(
                                    "owning array take requires its matching Package result",
                                ));
                            }
                            if check {
                                if !exact(&fact, Life::Available) {
                                    return Err(error(
                                        "owning array take requires a live element on every path",
                                    ));
                                }
                                no_later_use(
                                    state,
                                    &self.after[b][position],
                                    Source::ArrayElement {
                                        local: *local,
                                        index: *index,
                                    },
                                    budget,
                                )?;
                            }
                            produced = Some(fact.clone());
                            fact.life = BTreeSet::from([Life::Transferred]);
                        }
                        InstructionKind::DropOwnedArrayElement { .. } => {
                            if i.result.is_some() {
                                return Err(error("owning array element cleanup has no result"));
                            }
                            if check {
                                no_later_use(
                                    state,
                                    &self.after[b][position],
                                    Source::ArrayElement {
                                        local: *local,
                                        index: *index,
                                    },
                                    budget,
                                )?;
                            }
                            if fact.life.remove(&Life::Available) {
                                fact.life.insert(Life::Cleaned);
                            }
                        }
                        InstructionKind::StoreOwnedArrayElement { value, .. } => {
                            budget.type_work(element, false, "owning array store type")?;
                            if !decl.mutable
                                || i.result.is_some()
                                || f.values.get(value.0 as usize) != Some(element.as_ref())
                            {
                                return Err(error(
                                    "owning array store requires a mutable shell and matching complete element",
                                ));
                            }
                            if check {
                                if fact.life.contains(&Life::Available) {
                                    return Err(error(
                                        "owning array store overwrites an unpaid element",
                                    ));
                                }
                                no_later_use(
                                    state,
                                    &self.after[b][position],
                                    Source::ArrayElement {
                                        local: *local,
                                        index: *index,
                                    },
                                    budget,
                                )?;
                            }
                            self.consume(state, value, check, budget)?;
                            fact.life = BTreeSet::from([Life::Available]);
                            fact.birth = BTreeSet::from([Birth::Incoming]);
                        }
                        _ => unreachable!("owning array element operation"),
                    }
                    replace(state, place, fact, budget)?;
                }
                InstructionKind::ReadPackageField { local, index }
                | InstructionKind::DescribePackageField { local, index }
                | InstructionKind::TakePackageField { local, index }
                | InstructionKind::DropPackageField { local, index }
                | InstructionKind::StorePackageField { local, index, .. }
                | InstructionKind::BorrowPackageField { local, index, .. } => {
                    let Some(decl) = f.locals.get(local.0 as usize) else {
                        return Err(error("package operation requires a declared local"));
                    };
                    let Type::Package { fields, .. } = &decl.ty else {
                        return Err(error("field projection requires a package local"));
                    };
                    let Some((_, ty)) = fields.get(*index as usize) else {
                        return Err(error("package field index outside declaration"));
                    };
                    if check
                        && !exact(
                            &copied(state, Place::Local(*local), budget)?,
                            Life::Available,
                        )
                    {
                        return Err(error(
                            "package field operation requires an active local shell",
                        ));
                    }
                    let result = i.result.and_then(|v| f.values.get(v.0 as usize));
                    if matches!(i.kind, InstructionKind::ReadPackageField { .. }) {
                        budget.type_work(ty, true, "package field read type checks")?;
                        if !ty.is_copy_value() || result != Some(ty) {
                            return Err(error("package read requires a matching Copy field"));
                        }
                    } else if let InstructionKind::BorrowPackageField { mutable, .. } = i.kind {
                        budget.type_work(ty, true, "package field address type checks")?;
                        let shared_owned = !mutable
                            && (matches!(ty, Type::Package { .. })
                                || matches!(ty, Type::Managed(payload) if matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value()));
                        if shared_owned && matches!(ty, Type::Package { .. }) {
                            product::admit(ty, budget)?;
                        }
                        if (!ty.is_copy_value() && !matches!(ty, Type::Closure(_)) && !shared_owned)
                            || (mutable && !decl.mutable)
                            || !matches!(result, Some(Type::Address { mutable: actual, inner }) if *actual == mutable && inner.as_ref() == ty)
                        {
                            return Err(error(
                                "field borrow requires exact permission and a Copy or Closure field",
                            ));
                        }
                        // Copy fields are initialized with the shell and have
                        // no independent owning debt. Callback fields do.
                        if matches!(ty, Type::Closure(_)) || shared_owned {
                            let fact = copied(
                                state,
                                Place::PackageField {
                                    local: *local,
                                    index: *index,
                                },
                                budget,
                            )?;
                            if check && !exact(&fact, Life::Available) {
                                return Err(error(
                                    "callback field borrow requires its live environment on every path",
                                ));
                            }
                        }
                    } else if let InstructionKind::StorePackageField { value, .. } = i.kind {
                        let owns_field = package_owned_field(ty, budget)?;
                        budget.type_work(ty, !owns_field, "package field store type checks")?;
                        if !decl.mutable
                            || i.result.is_some()
                            || f.values.get(value.0 as usize) != Some(ty)
                        {
                            return Err(error(
                                "package field store requires a mutable shell and matching value",
                            ));
                        }
                        if owns_field {
                            let place = Place::PackageField {
                                local: *local,
                                index: *index,
                            };
                            if check {
                                if copied(state, place, budget)?
                                    .life
                                    .contains(&Life::Available)
                                {
                                    return Err(error(
                                        "package field store overwrites an unpaid responsibility",
                                    ));
                                }
                                no_later_use(
                                    state,
                                    &self.after[b][position],
                                    self.package_source(*local, *index, budget)?,
                                    budget,
                                )?;
                            }
                            let birth = if matches!(ty, Type::Managed(_)) {
                                copied(state, Place::Value(value), budget)?.birth
                            } else {
                                BTreeSet::from([Birth::Incoming])
                            };
                            if matches!(ty, Type::Package { .. }) {
                                product::transfer(
                                    state,
                                    (product::Carrier::Value(value), &[]),
                                    (product::Carrier::Local(*local), &[*index]),
                                    ty,
                                    budget,
                                )?;
                            }
                            self.consume(state, value, check, budget)?;
                            replace(
                                state,
                                place,
                                Fact {
                                    life: BTreeSet::from([Life::Available]),
                                    birth,
                                    sources: BTreeSet::new(),
                                },
                                budget,
                            )?;
                        } else if !ty.is_copy_value() {
                            return Err(error(
                                "package field store requires Managed or Copy field",
                            ));
                        }
                    } else {
                        if !package_owned_field(ty, budget)? {
                            return Err(error(
                                "package ownership operation requires an owning field",
                            ));
                        }
                        let place = Place::PackageField {
                            local: *local,
                            index: *index,
                        };
                        let mut fact = copied(state, place, budget)?;
                        if matches!(i.kind, InstructionKind::DescribePackageField { .. }) {
                            let Type::Managed(payload) = ty else {
                                return Err(error("package description requires a Managed field"));
                            };
                            budget.type_work(
                                payload,
                                false,
                                "package field description type check",
                            )?;
                            if !matches!(result, Some(Type::Desc(actual)) if actual == payload) {
                                return Err(error("package description payload mismatch"));
                            }
                            if check && !exact(&fact, Life::Available) {
                                return Err(error(
                                    "package description requires that field live on every path",
                                ));
                            }
                            produced = Some(Fact {
                                sources: BTreeSet::from([
                                    self.package_source(*local, *index, budget)?
                                ]),
                                ..Fact::default()
                            });
                        } else {
                            if check {
                                no_later_use(
                                    state,
                                    &self.after[b][position],
                                    self.package_source(*local, *index, budget)?,
                                    budget,
                                )?;
                            }
                            if matches!(i.kind, InstructionKind::TakePackageField { .. }) {
                                budget.type_work(ty, false, "package field take type check")?;
                                if result != Some(ty) {
                                    return Err(error(
                                        "package field take requires its exact owning result",
                                    ));
                                }
                                if check && !exact(&fact, Life::Available) {
                                    return Err(error(
                                        "package field take requires that field live on every path",
                                    ));
                                }
                                if matches!(ty, Type::Package { .. }) {
                                    product::transfer(
                                        state,
                                        (product::Carrier::Local(*local), &[*index]),
                                        (
                                            product::Carrier::Value(
                                                i.result.expect("checked child result"),
                                            ),
                                            &[],
                                        ),
                                        ty,
                                        budget,
                                    )?;
                                }
                                budget.spend(fact.birth.len(), "package field birth transfer")?;
                                produced = Some(Fact {
                                    life: BTreeSet::from([Life::Available]),
                                    birth: fact.birth.clone(),
                                    sources: BTreeSet::new(),
                                });
                                fact.life = BTreeSet::from([Life::Transferred]);
                            } else {
                                if i.result.is_some() {
                                    return Err(error("package field drop has no result"));
                                }
                                if fact.life.remove(&Life::Available) {
                                    fact.life.insert(Life::Cleaned);
                                }
                            }
                            fact.sources.clear();
                            replace(state, place, fact, budget)?;
                        }
                    }
                }
                InstructionKind::BorrowLocal { local }
                    if matches!(f.locals[local.0 as usize].ty, Type::Package { .. }) =>
                {
                    let ty = &f.locals[local.0 as usize].ty;
                    product::admit(ty, budget)?;
                    let Type::Package { fields, .. } = ty else {
                        unreachable!("Package borrow")
                    };
                    budget.spend(fields.len(), "shared Product completeness")?;
                    if check {
                        if !exact(
                            &copied(state, Place::Local(*local), budget)?,
                            Life::Available,
                        ) {
                            return Err(error(
                                "shared Product borrow requires a live complete shell",
                            ));
                        }
                        for (index, (_, field)) in fields.iter().enumerate() {
                            if package_owned_field(field, budget)?
                                && !exact(
                                    &copied(
                                        state,
                                        Place::PackageField {
                                            local: *local,
                                            index: index as u32,
                                        },
                                        budget,
                                    )?,
                                    Life::Available,
                                )
                            {
                                return Err(error(
                                    "shared Product borrow requires every owned child live",
                                ));
                            }
                        }
                    }
                }
                InstructionKind::BorrowLocal { local }
                    if matches!(f.locals[local.0 as usize].ty, Type::Managed(_)) =>
                {
                    if check
                        && !exact(
                            &copied(state, Place::Local(*local), budget)?,
                            Life::Available,
                        )
                    {
                        return Err(error("head address requires a live managed position"));
                    }
                }
                InstructionKind::DescribeLocal { local } => {
                    if !self.views.is_empty() {
                        budget.lookup(self.views.len(), "Package view description boundary")?;
                        if self.views.contains_key(local) {
                            return Err(error("Package view requires a precise field description"));
                        }
                    }
                    if check
                        && !exact(
                            &copied(state, Place::Local(*local), budget)?,
                            Life::Available,
                        )
                    {
                        return Err(error("description requires a live managed position"));
                    }
                    produced = Some(Fact {
                        sources: BTreeSet::from([Source::Head(*local)]),
                        ..Fact::default()
                    });
                }
                InstructionKind::BorrowCapture { index, .. } => {
                    if f.environment
                        .and_then(|id| self.module.closures.get(id.0 as usize))
                        .and_then(|capture| capture.captures.get(*index as usize))
                        .is_some_and(|ty| matches!(ty, Type::Closure(_)))
                    {
                        let fact = copied(state, Place::Capture(*index), budget)?;
                        if check && !exact(&fact, Life::Available) {
                            return Err(error(
                                "cannot borrow a moved captured closure environment",
                            ));
                        }
                    }
                }
                InstructionKind::DescribeCapture { index } => {
                    if f.environment
                        .and_then(|id| self.module.closures.get(id.0 as usize))
                        .and_then(|capture| capture.captures.get(*index as usize))
                        .is_some_and(|ty| matches!(ty, Type::Package { .. }))
                    {
                        return Err(error(
                            "Package capture description requires its precise view field",
                        ));
                    }
                    if check
                        && !exact(
                            &copied(state, Place::Capture(*index), budget)?,
                            Life::Available,
                        )
                    {
                        return Err(error("description requires a live owned field"));
                    }
                    produced = Some(Fact {
                        sources: BTreeSet::from([Source::OwnedField(*index)]),
                        ..Fact::default()
                    });
                }
                InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local }
                    if out_shape.is_some_and(|s| s.description) =>
                {
                    let source = copied(state, Place::Local(*local), budget)?;
                    produced = Some(Fact {
                        sources: source.sources,
                        ..Fact::default()
                    });
                }
                InstructionKind::ReborrowAddress { address, .. }
                    if out_shape.is_some_and(|s| s.description) =>
                {
                    let source = copied(state, Place::Value(*address), budget)?;
                    produced = Some(Fact {
                        sources: source.sources,
                        ..Fact::default()
                    });
                }
                InstructionKind::ReadCapture { index } => {
                    let c = f
                        .environment
                        .and_then(|id| self.module.closures.get(id.0 as usize))
                        .ok_or_else(|| error("capture has no environment"))?;
                    let ty = c
                        .captures
                        .get(*index as usize)
                        .ok_or_else(|| error("capture index is absent"))?;
                    if matches!(
                        ty,
                        Type::Managed(_) | Type::Package { .. } | Type::Closure(_)
                    ) {
                        return Err(error(
                            "ReadCapture cannot copy an owned Managed or Package field",
                        ));
                    }
                    if matches!(ty, Type::Desc(_)) {
                        produced = Some(Fact {
                            sources: BTreeSet::from([Source::BorrowedField(*index)]),
                            ..Fact::default()
                        });
                    }
                }
                InstructionKind::TakeCapture { index } => {
                    if f.environment
                        .and_then(|id| self.module.closures.get(id.0 as usize))
                        .and_then(|capture| capture.captures.get(*index as usize))
                        .is_some_and(|ty| matches!(ty, Type::Package { .. }))
                    {
                        return Err(error("Package capture movement requires its bound view"));
                    }
                    if callable != Some(CallableKind::FnOnce) {
                        return Err(error("TakeCapture requires a consuming entry"));
                    }
                    let place = Place::Capture(*index);
                    let mut fact = copied(state, place, budget)?;
                    if check {
                        if !exact(&fact, Life::Available) {
                            return Err(error("owned field was already given away"));
                        }
                        no_later_use(
                            state,
                            &self.after[b][position],
                            Source::OwnedField(*index),
                            budget,
                        )?;
                    }
                    produced = Some(fact.clone());
                    fact.life = BTreeSet::from([Life::Transferred]);
                    replace(state, place, fact, budget)?;
                }
                InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                    if let Some(s) = self.shapes.get(&Place::Local(*local)) {
                        let mut fact = copied(state, Place::Local(*local), budget)?;
                        if s.owned {
                            if matches!(i.kind, InstructionKind::Load(_)) {
                                return Err(error("Load cannot copy an owned carrier"));
                            }
                            if check {
                                if !exact(&fact, Life::Available) {
                                    return Err(error("move requires a live owned local"));
                                }
                                no_later_use(
                                    state,
                                    &self.after[b][position],
                                    Source::Head(*local),
                                    budget,
                                )?;
                            }
                            if let Type::Package { fields, .. } = &f.locals[local.0 as usize].ty {
                                budget.spend(fields.len(), "whole Package birth transfer")?;
                                for (index, (_, ty)) in fields.iter().enumerate() {
                                    if matches!(ty, Type::Managed(_)) {
                                        let birth = copied(
                                            state,
                                            Place::PackageField {
                                                local: *local,
                                                index: index as u32,
                                            },
                                            budget,
                                        )?
                                        .birth;
                                        set_package_birth(
                                            state,
                                            i.result.ok_or_else(|| error("move has no result"))?,
                                            index as u32,
                                            birth,
                                            budget,
                                        )?;
                                    } else if matches!(ty, Type::Package { .. }) {
                                        product::transfer(
                                            state,
                                            (product::Carrier::Local(*local), &[index as u32]),
                                            (
                                                product::Carrier::Value(
                                                    i.result.ok_or_else(|| {
                                                        error("move has no result")
                                                    })?,
                                                ),
                                                &[index as u32],
                                            ),
                                            ty,
                                            budget,
                                        )?;
                                    }
                                }
                            }
                            package_transition(f, state, *local, Life::Transferred, check, budget)?;
                            produced = Some(fact.clone());
                            fact.life = BTreeSet::from([Life::Transferred]);
                            fact.sources.clear();
                            replace(state, Place::Local(*local), fact, budget)?;
                            forget_order(state, *local, budget)?;
                        } else {
                            produced = Some(fact);
                        }
                    }
                }
                InstructionKind::Store { local, value } => {
                    if let Some(s) = self.shapes.get(&Place::Local(*local)) {
                        let mut source = copied(state, Place::Value(*value), budget)?;
                        if s.owned {
                            if check {
                                if copied(state, Place::Local(*local), budget)?
                                    .life
                                    .contains(&Life::Available)
                                {
                                    return Err(error("store overwrites an unpaid resource"));
                                }
                                no_later_use(
                                    state,
                                    &self.after[b][position],
                                    Source::Head(*local),
                                    budget,
                                )?;
                            }
                            self.consume(state, *value, check, budget)?;
                            forget_order(state, *local, budget)?;
                            for (place, fact) in &state.cells {
                                budget.spend(1, "resource successful initialization order")?;
                                if let Place::Local(other) = place
                                    && *other != *local
                                    && fact.life.contains(&Life::Available)
                                {
                                    if !self.views.is_empty() {
                                        budget.lookup(
                                            self.views.len(),
                                            "Package view order exclusion",
                                        )?;
                                        if self.views.contains_key(local) {
                                            continue;
                                        }
                                        budget.lookup(
                                            self.views.len(),
                                            "Package view order exclusion",
                                        )?;
                                        if self.views.contains_key(other) {
                                            continue;
                                        }
                                    }
                                    budget.lookup(state.later.len(), "resource order insertion")?;
                                    state.later.insert((*other, *local));
                                }
                            }
                            source.life = BTreeSet::from([Life::Available]);
                            package_transition(f, state, *local, Life::Available, check, budget)?;
                            if let Type::Package { fields, .. } = &f.locals[local.0 as usize].ty {
                                budget.spend(fields.len(), "whole Package birth store")?;
                                for (index, (_, ty)) in fields.iter().enumerate() {
                                    if matches!(ty, Type::Managed(_)) {
                                        let place = Place::PackageField {
                                            local: *local,
                                            index: index as u32,
                                        };
                                        let mut field = copied(state, place, budget)?;
                                        field.birth =
                                            package_birth(state, *value, index as u32, budget)?;
                                        replace(state, place, field, budget)?;
                                    } else if matches!(ty, Type::Package { .. }) {
                                        product::transfer(
                                            state,
                                            (product::Carrier::Value(*value), &[index as u32]),
                                            (product::Carrier::Local(*local), &[index as u32]),
                                            ty,
                                            budget,
                                        )?;
                                    }
                                }
                            }
                        }
                        replace(state, Place::Local(*local), source, budget)?;
                    }
                }
                InstructionKind::DropEnum { local }
                | InstructionKind::DropManaged { local }
                | InstructionKind::DropPackage { local }
                | InstructionKind::DropErrorUnion { local }
                | InstructionKind::DropOwnedArray { local }
                | InstructionKind::DropClosure { local } => {
                    if matches!(i.kind, InstructionKind::DropErrorUnion { .. })
                        && !f
                            .locals
                            .get(local.0 as usize)
                            .is_some_and(|l| owned_error(&l.ty))
                    {
                        return Err(error("DropErrorUnion requires an owned error-union local"));
                    }
                    if matches!(i.kind, InstructionKind::DropPackage { .. })
                        && !matches!(
                            f.locals.get(local.0 as usize).map(|l| &l.ty),
                            Some(Type::Package { .. })
                        )
                    {
                        return Err(error("DropPackage requires a package local"));
                    }
                    if matches!(i.kind, InstructionKind::DropOwnedArray { .. })
                        && !matches!(
                            f.locals.get(local.0 as usize).map(|l| &l.ty),
                            Some(Type::Array { element, length })
                                if *length > 0 && matches!(element.as_ref(), Type::Package { .. } | Type::Closure(_))
                        )
                    {
                        return Err(error("DropOwnedArray requires a finite owning array local"));
                    }
                    let mut fact = copied(state, Place::Local(*local), budget)?;
                    if check {
                        no_later_use(
                            state,
                            &self.after[b][position],
                            Source::Head(*local),
                            budget,
                        )?;
                        if exact(&fact, Life::Cleaned) {
                            return Err(error("duplicate automatic cleanup"));
                        }
                        for (before, later) in &state.later {
                            budget.spend(1, "resource cleanup order")?;
                            if *before == *local
                                && copied(state, Place::Local(*later), budget)?
                                    .life
                                    .contains(&Life::Available)
                            {
                                return Err(error(&format!(
                                    "cleanup is not reverse successful initialization order: local {} precedes live {} in {} block {}",
                                    local.0, later.0, f.name, b
                                )));
                            }
                        }
                    }
                    if fact.life.remove(&Life::Available) {
                        fact.life.insert(Life::Cleaned);
                    }
                    package_transition(f, state, *local, Life::Cleaned, check, budget)?;
                    fact.sources.clear();
                    replace(state, Place::Local(*local), fact, budget)?;
                    forget_order(state, *local, budget)?;
                }
                InstructionKind::StorageLive(local) | InstructionKind::StorageDead(local) => {
                    if let Some(s) = self.shapes.get(&Place::Local(*local)) {
                        if check && s.owned {
                            if copied(state, Place::Local(*local), budget)?
                                .life
                                .contains(&Life::Available)
                            {
                                return Err(error("storage boundary discards an owned carrier"));
                            }
                            no_later_use(
                                state,
                                &self.after[b][position],
                                Source::Head(*local),
                                budget,
                            )?;
                        }
                        replace(
                            state,
                            Place::Local(*local),
                            Fact {
                                life: if s.owned {
                                    BTreeSet::from([Life::Vacant])
                                } else {
                                    BTreeSet::new()
                                },
                                ..Fact::default()
                            },
                            budget,
                        )?;
                        package_transition(f, state, *local, Life::Vacant, check, budget)?;
                        forget_order(state, *local, budget)?;
                    }
                }
                InstructionKind::WriteDesc { desc, .. }
                | InstructionKind::ByteBufferReserve { buffer: desc, .. }
                | InstructionKind::ByteBufferTruncate { buffer: desc, .. }
                | InstructionKind::ByteBufferPush { buffer: desc, .. }
                | InstructionKind::ByteBufferBorrow {
                    buffer: desc,
                    mutable: true,
                } => {
                    record_write(
                        &copied(state, Place::Value(*desc), budget)?.sources,
                        effect,
                        shared,
                        check,
                        budget,
                    )?;
                }
                InstructionKind::MakeClosure { closure, captures } => {
                    let definition = self
                        .module
                        .closures
                        .get(closure.0 as usize)
                        .ok_or_else(|| error("missing closure definition"))?;
                    if definition.captures.len() != captures.len() {
                        return Err(error("capture arity mismatch"));
                    }
                    let mut fact = Fact::default();
                    for value in captures {
                        budget.lookup(self.shapes.len(), "captured resource lookup")?;
                        if self.shapes.contains_key(&Place::Value(*value)) {
                            let captured = copied(state, Place::Value(*value), budget)?;
                            budget.spend(
                                fact.sources.len().saturating_add(captured.sources.len()),
                                "captured resource sources",
                            )?;
                            fact.sources.extend(captured.sources);
                            self.consume(state, *value, check, budget)?;
                        }
                    }
                    if out_shape.is_some_and(|s| s.owned) {
                        fact.life.insert(Life::Available);
                        fact.birth.insert(Birth::Incoming);
                    }
                    produced = Some(fact);
                }
                InstructionKind::Call {
                    function,
                    arguments,
                } => {
                    let callee = self
                        .module
                        .functions
                        .get(function.0 as usize)
                        .ok_or_else(|| error("absent callee"))?;
                    if let Some(contract) = &callee.callable_contract {
                        budget.spend(contract.roles.len(), "helper role write projection")?;
                        for role in &contract.roles {
                            let environment = &self.module.closures[role.closure.0 as usize];
                            let mut invokes = false;
                            match &contract.traces {
                                CallableTraces::Finite(paths) => {
                                    budget.spend(
                                        paths.len(),
                                        "helper finite role invocation presence",
                                    )?;
                                    for path in paths {
                                        budget
                                            .spend(path.calls.len(), "helper finite role events")?;
                                        invokes |= path
                                            .calls
                                            .iter()
                                            .any(|event| event.parameter == role.parameter);
                                    }
                                }
                                CallableTraces::Flow(flow) => {
                                    // The complete graph includes reachable prefixes
                                    // with no Return. Inspect the current role, not
                                    // whether any unrelated environment was called.
                                    budget.spend(
                                        flow.nodes.len(),
                                        "helper flow role invocation presence",
                                    )?;
                                    for node in &flow.nodes {
                                        budget
                                            .spend(node.calls.len(), "helper flow role events")?;
                                        invokes |= node
                                            .calls
                                            .iter()
                                            .any(|event| event.parameter == role.parameter);
                                    }
                                }
                            }
                            budget.spend(role.writes.len(), "helper role capture writes")?;
                            if invokes
                                && role.writes.iter().any(|i| {
                                    matches!(environment.captures[*i as usize], Type::Desc(_))
                                })
                            {
                                let value = arguments
                                    .get(role.parameter as usize)
                                    .ok_or_else(|| error("missing helper environment argument"))?;
                                record_write(
                                    &copied(state, Place::Value(*value), budget)?.sources,
                                    effect,
                                    shared,
                                    check,
                                    budget,
                                )?;
                            }
                        }
                    } else if callee.callable_return_contract.is_none() {
                        // Callable helpers and self-contained factory callers
                        // consume only their public envelopes. Factory inputs
                        // cannot carry an external description or address.
                        // Concrete helper callers consume only their public
                        // envelope. Legacy private summaries belong to ordinary
                        // functions; even looking one up here is unnecessary.
                        if let Some(dependencies) = self.dependencies {
                            // Effects influence only writes, never origin/lifetime
                            // state when check=false. Extract each substitution
                            // once rather than replaying the body on every fact.
                            for (parameter, value) in arguments.iter().enumerate() {
                                budget.spend(1, "resource summary argument scan")?;
                                if self.shapes.contains_key(&Place::Value(*value)) {
                                    let sources =
                                        copied(state, Place::Value(*value), budget)?.sources;
                                    dependencies.connect(
                                        function.0 as usize,
                                        summary_graph::Kind::Argument(parameter as u32),
                                        f.id.0 as usize,
                                        &sources,
                                        budget,
                                    )?;
                                }
                            }
                        } else {
                            let summary = self
                                .effects
                                .get(function.0 as usize)
                                .ok_or_else(|| error("missing callee effect"))?;
                            for parameter in &summary.arguments {
                                budget.spend(1, "callee resource effect substitution")?;
                                let value = arguments
                                    .get(*parameter as usize)
                                    .ok_or_else(|| error("missing callee argument"))?;
                                record_write(
                                    &copied(state, Place::Value(*value), budget)?.sources,
                                    effect,
                                    shared,
                                    check,
                                    budget,
                                )?;
                            }
                        }
                    }
                    for value in arguments {
                        self.consume(state, *value, check, budget)?;
                    }
                    if out_shape.is_some_and(|s| s.owned) {
                        let callee = self
                            .module
                            .functions
                            .get(function.0 as usize)
                            .ok_or_else(|| error("absent callee"))?;
                        produced = Some(Fact {
                            life: BTreeSet::from([Life::Available]),
                            sources: BTreeSet::new(),
                            birth: BTreeSet::from([if callee.result_mode == PassingMode::Create {
                                Birth::Created
                            } else {
                                Birth::Incoming
                            }]),
                        });
                    }
                }
                InstructionKind::CallClosureShared { .. }
                | InstructionKind::CallClosureMut { .. }
                | InstructionKind::CallClosureOnce { .. }
                | InstructionKind::CallClosureBorrowed { .. }
                | InstructionKind::CallClosure { .. } => {
                    let (place, ty, expected) = match i.kind {
                        InstructionKind::CallClosureShared { callee, .. } => (
                            Place::Local(callee),
                            &f.locals[callee.0 as usize].ty,
                            CallableKind::Fn,
                        ),
                        InstructionKind::CallClosureMut { callee, .. } => (
                            Place::Local(callee),
                            &f.locals[callee.0 as usize].ty,
                            CallableKind::FnMut,
                        ),
                        InstructionKind::CallClosureOnce { callee, .. } => (
                            Place::Value(callee),
                            &f.values[callee.0 as usize],
                            CallableKind::FnOnce,
                        ),
                        InstructionKind::CallClosure { callee, .. } => (
                            Place::Value(callee),
                            &f.values[callee.0 as usize],
                            CallableKind::Fn,
                        ),
                        InstructionKind::CallClosureBorrowed { callee, kind, .. } => {
                            let Type::Address { inner, mutable } = &f.values[callee.0 as usize]
                            else {
                                return Err(error("borrowed environment call requires an address"));
                            };
                            if kind == CallableKind::FnOnce
                                || (kind == CallableKind::FnMut && !mutable)
                            {
                                return Err(error(
                                    "borrowed environment call requires Fn or an exclusive FnMut address",
                                ));
                            }
                            (Place::Value(callee), inner.as_ref(), kind)
                        }
                        _ => unreachable!(),
                    };
                    let Type::Closure(id) = ty else {
                        return Err(error("callee is not an environment"));
                    };
                    let definition = &self.module.closures[id.0 as usize];
                    if !matches!(
                        (definition.kind, expected),
                        (
                            CallableKind::Fn,
                            CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce
                        ) | (
                            CallableKind::FnMut,
                            CallableKind::FnMut | CallableKind::FnOnce
                        ) | (CallableKind::FnOnce, CallableKind::FnOnce)
                    ) {
                        return Err(error("call mode differs from environment entry"));
                    }
                    if let Some(s) = self.shapes.get(&place) {
                        let fact = copied(state, place, budget)?;
                        if s.owned && matches!(i.kind, InstructionKind::CallClosure { .. }) {
                            return Err(error("by-value Fn call copies owned environment"));
                        }
                        if check {
                            if s.owned && !exact(&fact, Life::Available) {
                                return Err(error("call uses consumed environment"));
                            }
                            if s.description {
                                live_sources(state, &fact, budget)?;
                            }
                        }
                        if let Some(dependencies) = self.dependencies {
                            dependencies.connect(
                                definition.function.0 as usize,
                                summary_graph::Kind::AnyBorrowed,
                                f.id.0 as usize,
                                &fact.sources,
                                budget,
                            )?;
                        } else {
                            let summary = self
                                .effects
                                .get(definition.function.0 as usize)
                                .ok_or_else(|| error("absent environment entry"))?;
                            if !summary.borrowed_fields.is_empty() {
                                record_write(&fact.sources, effect, shared, check, budget)?;
                            }
                        }
                    }
                    if let InstructionKind::CallClosureOnce { callee, .. } = i.kind {
                        self.consume(state, callee, check, budget)?;
                    }
                }
                _ => {}
            }
            if let Some(plan) = self.sums
                && let Some(birth) =
                    plan.apply(self.module, f, i, &mut state.sums, check, budget)?
                && let Some(produced) = &mut produced
            {
                produced.birth = birth;
            }
            if let (Some(place), Some(s)) = (output, out_shape) {
                let fact = produced
                    .ok_or_else(|| error("instruction cannot produce the claimed resource kind"))?;
                if check && s.owned && !exact(&fact, Life::Available) {
                    return Err(error("result does not carry a live responsibility"));
                }
                replace(state, place, fact, budget)?;
            }
        }
        if let Terminator::Return(value) = f.blocks[b].terminator {
            if let Some(value) = value {
                if check && f.result_mode == PassingMode::Create {
                    let birth = copied(state, Place::Value(value), budget)?.birth;
                    if birth != BTreeSet::from([Birth::Created])
                        && !(managed_success(&f.return_type) && birth.is_empty())
                    {
                        return Err(error("create result forwards an incoming responsibility"));
                    }
                }
                self.consume(state, value, check, budget)?;
            }
            if check {
                for (place, fact) in &state.cells {
                    budget.spend(1, "normal return resource conservation")?;
                    if matches!(place, Place::Capture(_) | Place::CaptureField { .. }) {
                        if callable != Some(CallableKind::FnOnce) && !exact(fact, Life::Available) {
                            return Err(error("repeatable entry fails to restore owned fields"));
                        }
                    } else if fact.life.contains(&Life::Available) {
                        return Err(error("normal return leaves a responsibility unpaid"));
                    }
                }
            }
        }
        Ok(())
    }
}
fn analyze_function(
    module: &TypedModule,
    f: &TypedFunction,
    reachable: &[bool],
    effects: &[Effect],
    check: bool,
    budget: &mut Budget,
) -> CompileResult<Effect> {
    analyze_function_with_dependencies(module, f, reachable, effects, check, budget, None)
}
fn analyze_function_with_dependencies(
    module: &TypedModule,
    f: &TypedFunction,
    reachable: &[bool],
    effects: &[Effect],
    check: bool,
    budget: &mut Budget,
    dependencies: Option<&summary_graph::Dependencies>,
) -> CompileResult<Effect> {
    if f.foreign.is_some() {
        return Ok(Effect::default());
    }
    let sums = sums::Plan::new(f, budget)?;
    let shapes = shapes(module, f, budget)?;
    if shapes.is_empty() && sums.is_none() {
        return Ok(Effect::default());
    }
    // Summary inference never consumes description liveness: all uses of
    // Flow::after are inside `check` branches. Build it only for the final
    // ordered ownership validation, not once per effect-frontier revisit.
    let after = if check {
        afters(f, reachable, &shapes, budget)?
    } else {
        Vec::new()
    };
    let (package_values, product_births) = package_value_shapes(f, budget)?;
    let mut initial = State {
        package_values,
        product_births,
        sums: sums
            .as_ref()
            .map(|p| p.initial(f, budget))
            .transpose()?
            .unwrap_or_default(),
        ..State::default()
    };
    for (place, s) in &shapes {
        budget.lookup(initial.cells.len(), "initial resource state")?;
        let mut fact = Fact::default();
        if s.owned {
            fact.life.insert(
                if matches!(place, Place::Capture(_) | Place::CaptureField { .. }) {
                    Life::Available
                } else {
                    Life::Vacant
                },
            );
        }
        if matches!(place, Place::Capture(_) | Place::CaptureField { .. }) {
            fact.birth.insert(Birth::Incoming);
        }
        if let Place::PackageField { local, index } = place
            && let Type::Package { fields, .. } = &f.locals[local.0 as usize].ty
            && let child @ Type::Package { .. } = &fields[*index as usize].1
        {
            product::add(
                &mut initial.product_births,
                product::Carrier::Local(*local),
                *index,
                child,
                budget,
            )?;
        }
        initial.cells.insert(*place, fact);
    }
    for (index, local) in f.parameters.iter().enumerate() {
        if let Some(s) = shapes.get(&Place::Local(*local)) {
            let mut fact = Fact::default();
            if s.owned {
                fact.life.insert(Life::Available);
                fact.birth.insert(Birth::Incoming);
                for (place, old) in &initial.cells {
                    budget.spend(1, "incoming resource order")?;
                    if let Place::Local(older) = place
                        && old.life.contains(&Life::Available)
                    {
                        initial.later.insert((*older, *local));
                    }
                }
            }
            if s.description {
                fact.sources.insert(Source::Argument(index as u32));
            }
            replace(&mut initial, Place::Local(*local), fact, budget)?;
            package_transition(f, &mut initial, *local, Life::Available, false, budget)?;
        }
    }
    budget.product(f.blocks.len(), 2, 1, "resource diagnostic CFG")?;
    let mut input: Vec<Option<State>> = vec![None; f.blocks.len()];
    input[f.entry.0 as usize] = Some(initial);
    let mut stack = vec![f.entry.0 as usize];
    let mut queued = vec![false; f.blocks.len()];
    queued[f.entry.0 as usize] = true;
    let mut views = BTreeMap::new();
    if f.environment.is_some() {
        for instruction in f.blocks[f.entry.0 as usize].instructions.iter().skip(1) {
            let InstructionKind::BindPackageCapture { local, index } = instruction.kind else {
                break;
            };
            budget.lookup(views.len(), "Package resource view mapping")?;
            views.insert(local, index);
        }
    }
    let flow = Flow {
        module,
        function: f,
        shapes: &shapes,
        after: &after,
        effects,
        views: &views,
        dependencies,
        sums: sums.as_ref(),
    };
    let mut effect = Effect::default();
    while let Some(b) = stack.pop() {
        queued[b] = false;
        let mut state = clone_state(input[b].as_ref().unwrap(), budget)?;
        flow.run(b, &mut state, &mut effect, false, budget)?;
        for successor in next(&f.blocks[b]) {
            budget.spend(1, "resource diagnostic edge")?;
            let edge_state = if let Some(plan) = &sums {
                let mut edge = clone_state(&state, budget)?;
                if !plan.edge(f, b, successor, &mut edge.sums, budget)? {
                    continue;
                }
                Some(edge)
            } else {
                None
            };
            let state = edge_state.as_ref().unwrap_or(&state);
            let changed = if let Some(previous) = &mut input[successor] {
                join(previous, state, budget)?
            } else {
                input[successor] = Some(clone_state(state, budget)?);
                true
            };
            if changed && !queued[successor] {
                stack.push(successor);
                queued[successor] = true;
            }
        }
    }
    // Every changed incoming state is queued and processed before the worklist
    // becomes empty. At fixed effects, rerunning the exact transfer on those
    // same final states can add no new summary fact. The validation pass is
    // still mandatory and unchanged when `check` is true.
    if !check {
        return Ok(effect);
    }
    for (b, incoming) in input.into_iter().enumerate() {
        if let Some(mut state) = incoming {
            flow.run(b, &mut state, &mut effect, check, budget)?;
        }
    }
    Ok(effect)
}
// Summary consumers are an input dependency, not an authority claim. Build
// them from this phase's actual Call/Closure read sites. The verifier has its
// own graph and fixed point, and never receives this table.
fn effect_consumers(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    budget: &mut Budget,
) -> CompileResult<Vec<Vec<usize>>> {
    let count = module.functions.len();
    // Adjacency headers, initial queue/queued cells and eventual table release.
    budget.product(count, 5, 1, "resource effect frontier tables")?;
    let mut consumers = vec![Vec::new(); count];
    for (caller, (function, (_, reachable))) in module.functions.iter().zip(graphs).enumerate() {
        for (block, live) in function.blocks.iter().zip(reachable) {
            budget.spend(1, "resource effect frontier block")?;
            if !live {
                continue;
            }
            for instruction in &block.instructions {
                budget.spend(1, "resource effect frontier instruction")?;
                let callee = match &instruction.kind {
                    InstructionKind::Call { function, .. } => {
                        let target = module
                            .functions
                            .get(function.0 as usize)
                            .ok_or_else(|| error("absent callee"))?;
                        // These calls consume published envelopes, not a
                        // changing private resource-write summary.
                        (target.callable_contract.is_none()
                            && target.callable_return_contract.is_none())
                        .then_some(function.0 as usize)
                    }
                    InstructionKind::CallClosureShared { callee, .. }
                    | InstructionKind::CallClosureMut { callee, .. } => {
                        let ty = function
                            .locals
                            .get(callee.0 as usize)
                            .ok_or_else(|| error("absent environment local"))?;
                        Some(effect_entry(module, &ty.ty)?)
                    }
                    InstructionKind::CallClosure { callee, .. }
                    | InstructionKind::CallClosureOnce { callee, .. }
                    | InstructionKind::CallClosureBorrowed { callee, .. } => {
                        let ty = function
                            .values
                            .get(callee.0 as usize)
                            .ok_or_else(|| error("absent environment value"))?;
                        Some(effect_entry(module, ty)?)
                    }
                    _ => None,
                };
                if let Some(callee) = callee {
                    let users = consumers
                        .get_mut(callee)
                        .ok_or_else(|| error("absent environment entry"))?;
                    // Caller IDs increase monotonically across this scan;
                    // repeated calls in one body need neither sorting nor a set.
                    if users.last() != Some(&caller) {
                        budget.spend(2, "resource effect frontier edge")?;
                        users.push(caller);
                    }
                }
            }
        }
    }
    Ok(consumers)
}

fn effect_entry(module: &TypedModule, ty: &Type) -> CompileResult<usize> {
    let ty = if let Type::Address { inner, .. } = ty {
        inner.as_ref()
    } else {
        ty
    };
    let Type::Closure(id) = ty else {
        return Err(error("callee is not an environment"));
    };
    module
        .closures
        .get(id.0 as usize)
        .map(|definition| definition.function.0 as usize)
        .ok_or_else(|| error("absent environment entry"))
}

fn grow_effect(target: &mut Effect, found: Effect, budget: &mut Budget) -> CompileResult<bool> {
    budget.spend(
        found
            .arguments
            .len()
            .saturating_add(found.borrowed_fields.len())
            .saturating_add(target.arguments.len())
            .saturating_add(target.borrowed_fields.len())
            .saturating_add(1),
        "resource effect convergence",
    )?;
    let before = (target.arguments.len(), target.borrowed_fields.len());
    target.arguments.extend(found.arguments);
    target.borrowed_fields.extend(found.borrowed_fields);
    Ok(before != (target.arguments.len(), target.borrowed_fields.len()))
}

fn infer_effects(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    budget: &mut Budget,
) -> CompileResult<Vec<Effect>> {
    if module.functions.len() >= 32 {
        summary_graph::solve(module, graphs, budget)
    } else {
        infer_effects_frontier(module, graphs, budget)
    }
}
fn infer_effects_frontier(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    budget: &mut Budget,
) -> CompileResult<Vec<Effect>> {
    budget.spend(module.functions.len(), "resource effect table")?;
    let mut effects = vec![Effect::default(); module.functions.len()];
    let consumers = effect_consumers(module, graphs, budget)?;
    let mut pending: VecDeque<_> = (0..module.functions.len()).collect();
    let mut queued = vec![true; module.functions.len()];
    while let Some(index) = pending.pop_front() {
        budget.spend(1, "resource effect frontier pop")?;
        queued[index] = false;
        #[cfg(test)]
        TEST_EFFECT_SOLVES.with(|value| value.set(value.get() + 1));
        let (_, reachable) = &graphs[index];
        let found = analyze_function(
            module,
            &module.functions[index],
            reachable,
            &effects,
            false,
            budget,
        )?;
        if grow_effect(&mut effects[index], found, budget)? {
            budget.spend(consumers[index].len(), "resource effect notification")?;
            for &caller in &consumers[index] {
                if !queued[caller] {
                    budget.spend(1, "resource effect frontier push")?;
                    queued[caller] = true;
                    pending.push_back(caller);
                }
            }
        }
    }
    Ok(effects)
}

// Kept only as an executable scheduling oracle. It calls the same unchanged
// per-function transfer with a different, exhaustive iteration strategy.
#[cfg(test)]
fn infer_effects_full_scan(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    budget: &mut Budget,
) -> CompileResult<Vec<Effect>> {
    budget.spend(module.functions.len(), "resource effect table")?;
    let mut effects = vec![Effect::default(); module.functions.len()];
    loop {
        let mut changed = false;
        for (index, (f, (_, reachable))) in module.functions.iter().zip(graphs).enumerate() {
            TEST_EFFECT_SOLVES.with(|value| value.set(value.get() + 1));
            let found = analyze_function(module, f, reachable, &effects, false, budget)?;
            changed |= grow_effect(&mut effects[index], found, budget)?;
        }
        if !changed {
            return Ok(effects);
        }
    }
}

#[cfg(test)]
thread_local! {
    static TEST_EFFECT_SOLVES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(super) fn check_module(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    budget: &mut Budget,
) -> CompileResult<()> {
    budget.spend(module.closures.len(), "resource closure presence")?;
    if module.resources.is_empty() {
        let mut owning_entry = false;
        for closure in &module.closures {
            budget.spend(closure.captures.len(), "resource capture presence")?;
            owning_entry |= closure.kind == CallableKind::FnOnce
                || closure.captures.iter().any(|ty| {
                    matches!(
                        ty,
                        Type::Managed(_) | Type::Package { .. } | Type::Closure(_)
                    )
                });
        }
        for function in &module.functions {
            budget.spend(
                function.locals.len().saturating_add(function.values.len()),
                "owning array presence",
            )?;
            owning_entry |= budget
                .sum_functions
                .get(function.id.0 as usize)
                .copied()
                .unwrap_or(false);
            owning_entry |= function.locals.iter().map(|l| &l.ty).chain(&function.values)
                .any(|ty| matches!(ty, Type::Array { element, .. } if matches!(element.as_ref(), Type::Package { .. } | Type::Closure(_))));
        }
        if !owning_entry {
            return Ok(());
        }
    }
    let effects = infer_effects(module, graphs, budget)?;
    for (f, (_, reachable)) in module.functions.iter().zip(graphs) {
        analyze_function(module, f, reachable, &effects, true, budget)?;
    }
    Ok(())
}

#[cfg(test)]
mod frontier_tests;

// Frozen pre-144 algorithm, used only as an executable transformation oracle.
#[cfg(test)]
fn analyze_function_eager_reference(
    module: &TypedModule,
    f: &TypedFunction,
    reachable: &[bool],
    effects: &[Effect],
    check: bool,
    budget: &mut Budget,
) -> CompileResult<Effect> {
    if f.foreign.is_some() {
        return Ok(Effect::default());
    }
    let sums = sums::Plan::new(f, budget)?;
    let shapes = shapes(module, f, budget)?;
    if shapes.is_empty() && sums.is_none() {
        return Ok(Effect::default());
    }
    let after = afters(f, reachable, &shapes, budget)?;
    let (package_values, product_births) = package_value_shapes(f, budget)?;
    let mut initial = State {
        package_values,
        product_births,
        sums: sums
            .as_ref()
            .map(|p| p.initial(f, budget))
            .transpose()?
            .unwrap_or_default(),
        ..State::default()
    };
    for (place, s) in &shapes {
        budget.lookup(initial.cells.len(), "initial resource state")?;
        let mut fact = Fact::default();
        if s.owned {
            fact.life.insert(
                if matches!(place, Place::Capture(_) | Place::CaptureField { .. }) {
                    Life::Available
                } else {
                    Life::Vacant
                },
            );
        }
        if matches!(place, Place::Capture(_) | Place::CaptureField { .. }) {
            fact.birth.insert(Birth::Incoming);
        }
        if let Place::PackageField { local, index } = place
            && let Type::Package { fields, .. } = &f.locals[local.0 as usize].ty
            && let child @ Type::Package { .. } = &fields[*index as usize].1
        {
            product::add(
                &mut initial.product_births,
                product::Carrier::Local(*local),
                *index,
                child,
                budget,
            )?;
        }
        initial.cells.insert(*place, fact);
    }
    for (index, local) in f.parameters.iter().enumerate() {
        if let Some(s) = shapes.get(&Place::Local(*local)) {
            let mut fact = Fact::default();
            if s.owned {
                fact.life.insert(Life::Available);
                fact.birth.insert(Birth::Incoming);
                for (place, old) in &initial.cells {
                    budget.spend(1, "incoming resource order")?;
                    if let Place::Local(older) = place
                        && old.life.contains(&Life::Available)
                    {
                        initial.later.insert((*older, *local));
                    }
                }
            }
            if s.description {
                fact.sources.insert(Source::Argument(index as u32));
            }
            replace(&mut initial, Place::Local(*local), fact, budget)?;
            package_transition(f, &mut initial, *local, Life::Available, false, budget)?;
        }
    }
    budget.product(f.blocks.len(), 2, 1, "resource diagnostic CFG")?;
    let mut input: Vec<Option<State>> = vec![None; f.blocks.len()];
    input[f.entry.0 as usize] = Some(initial);
    let mut stack = vec![f.entry.0 as usize];
    let mut queued = vec![false; f.blocks.len()];
    queued[f.entry.0 as usize] = true;
    let mut views = BTreeMap::new();
    if f.environment.is_some() {
        for instruction in f.blocks[f.entry.0 as usize].instructions.iter().skip(1) {
            let InstructionKind::BindPackageCapture { local, index } = instruction.kind else {
                break;
            };
            budget.lookup(views.len(), "Package resource view mapping")?;
            views.insert(local, index);
        }
    }
    let flow = Flow {
        module,
        function: f,
        shapes: &shapes,
        after: &after,
        effects,
        views: &views,
        dependencies: None,
        sums: sums.as_ref(),
    };
    let mut effect = Effect::default();
    while let Some(b) = stack.pop() {
        queued[b] = false;
        let mut state = clone_state(input[b].as_ref().unwrap(), budget)?;
        flow.run(b, &mut state, &mut effect, false, budget)?;
        for successor in next(&f.blocks[b]) {
            budget.spend(1, "resource diagnostic edge")?;
            let edge_state = if let Some(plan) = &sums {
                let mut edge = clone_state(&state, budget)?;
                if !plan.edge(f, b, successor, &mut edge.sums, budget)? {
                    continue;
                }
                Some(edge)
            } else {
                None
            };
            let state = edge_state.as_ref().unwrap_or(&state);
            let changed = if let Some(previous) = &mut input[successor] {
                join(previous, state, budget)?
            } else {
                input[successor] = Some(clone_state(state, budget)?);
                true
            };
            if changed && !queued[successor] {
                stack.push(successor);
                queued[successor] = true;
            }
        }
    }
    for (b, incoming) in input.into_iter().enumerate() {
        if let Some(mut state) = incoming {
            flow.run(b, &mut state, &mut effect, check, budget)?;
        }
    }
    Ok(effect)
}
