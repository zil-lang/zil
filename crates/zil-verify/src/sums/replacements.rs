//! Replacement freshness is solved by rerunning this verifier's actual Content
//! transfer, independently of Analyze and independently of ordinary write rows.
use super::*;
use crate::place_replacements::{Inventory, actual};
use std::cell::RefCell;
#[cfg(test)]
mod helper_result_tests;
type Rows = BTreeMap<actual::Target, Content>;
struct Invocation<'a> {
    entry: usize,
    arguments: &'a [ValueId],
    captures: Option<&'a actual::Captures>,
    may_only: bool,
}
pub(super) struct Context<'a> {
    actual: &'a actual::Program,
    places: &'a [Inventory],
    summaries: &'a [Rows],
    whole_must: &'a BTreeSet<usize>,
    written: RefCell<Rows>,
    store_calls: &'a RefCell<BTreeSet<usize>>,
}
fn invalid(text: &str) -> Diagnostic {
    bad(None, text)
}
fn lookup(b: &mut Budget, count: usize) -> Check<()> {
    b.spend(
        count
            .checked_ilog2()
            .map_or(1, |n| n as usize + 2)
            .saturating_mul(12),
    )
}
fn merge_leaf(to: &mut Content, from: &Content, b: &mut Budget) -> Check<bool> {
    to.cost(b)?;
    from.cost(b)?;
    let (Content::Leaf(a), Content::Leaf(c)) = (&*to, from) else {
        return Err(invalid(
            "replacement needs actual complete managed leaf content",
        ));
    };
    let widths = |sources: &BTreeSet<Source>| -> Check<usize> {
        let mut total = 0usize;
        for source in sources {
            let width = match source {
                Source::Input(_, p) | Source::ReturnedInput(_, p) => p.len().saturating_add(1),
                _ => 1,
            };
            total = total
                .checked_add(width)
                .ok_or_else(|| limit("replacement content comparison width overflow"))?;
        }
        Ok(total)
    };
    let work = widths(a)?
        .checked_mul(c.len().saturating_add(1))
        .and_then(|n| {
            widths(c)
                .ok()
                .and_then(|m| m.checked_mul(a.len().saturating_add(1)))
                .and_then(|m| n.checked_add(m))
        })
        .ok_or_else(|| limit("replacement content comparison work overflow"))?;
    b.spend(work)?;
    to.merge(from, b)
}
// Whole endpoints replace one actual instance, not its historic alternatives.
// Keep the old Managed leaf transfer and its metering unchanged.
fn merge_endpoint(to: &mut Content, from: &Content, b: &mut Budget) -> Check<bool> {
    if matches!((&*to, from), (Content::Sum(..), Content::Sum(..))) {
        to.cost(b)?;
        from.cost(b)?;
        let changed = to != from;
        *to = from.clone();
        return Ok(changed);
    }
    if let (Content::Product(old), Content::Product(incoming)) = (&mut *to, from) {
        b.spend(old.len().saturating_add(incoming.len()))?;
        if old.len() != incoming.len() {
            return Err(invalid("replacement Product widths differ"));
        }
        let mut owned = 0usize;
        let mut changed = false;
        for (old, incoming) in old.iter_mut().zip(incoming) {
            match (&*old, incoming) {
                (Content::Copy, Content::Copy) => {}
                (Content::Leaf(_), Content::Leaf(_)) => {
                    owned += 1;
                    old.cost(b)?;
                    incoming.cost(b)?;
                    changed |= old != incoming;
                    // Commit replaces the old instance. Only the real staged
                    // RHS leaf survives; old Incoming must not taint a fresh
                    // replacement, nor old Fresh upgrade an incoming RHS.
                    *old = incoming.clone();
                }
                (Content::Product(_), Content::Product(_)) => {
                    owned += 1;
                    changed |= merge_endpoint(old, incoming, b)?;
                }
                (Content::Sum(_, _), Content::Sum(_, _)) => {
                    owned += 1;
                    changed |= merge_endpoint(old, incoming, b)?;
                }
                _ => return Err(invalid("replacement needs a complete ordinary Product")),
            }
        }
        if owned == 0 {
            return Err(invalid("replacement Product needs an actual owned leaf"));
        }
        Ok(changed)
    } else {
        merge_leaf(to, from, b)
    }
}
/// Structural admission only; actual roots, debts, loans and epochs are checked
/// independently. No full Product or nested owning profile is implied.
pub(crate) fn package_owners(ty: &Type, b: &mut Budget) -> Check<Option<Vec<usize>>> {
    let Type::Package { fields, .. } = ty else {
        return Ok(None);
    };
    b.spend(fields.len())?;
    let mut owners = Vec::new();
    for (index, (_, ty)) in fields.iter().enumerate() {
        match ty {
            Type::Managed(payload)
                if matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value() =>
            {
                owners.push(index);
            }
            Type::Managed(_) if dynamic_store::inline_owner(ty) => owners.push(index),
            Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::F32
            | Type::F64
            | Type::U8
            | Type::U16 => {}
            Type::Package { .. } if package_owners(ty, b)?.is_some() => owners.push(index),
            _ => return Ok(None),
        }
    }
    Ok((!owners.is_empty()).then_some(owners))
}
/// Result shape only; the independent module/type checks validate nominal
/// identity and actual instructions. No borrow or owner can leave this lane.
fn helper_copy_result(ty: &Type, b: &mut Budget) -> Check<bool> {
    // Preserve the accepted Unit helper's previous constant-time path.
    if matches!(ty, Type::Unit) {
        return Ok(true);
    }
    let mut first = Some(ty);
    let mut pending = Vec::new();
    while let Some(ty) = first.take().or_else(|| pending.pop()) {
        b.spend(1)?;
        match ty {
            Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::F32
            | Type::F64
            | Type::U8
            | Type::U16 => {}
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                pending.extend(fields);
            }
            Type::Array { element, .. } => {
                b.spend(1)?;
                pending.push(element);
            }
            Type::Record { fields, .. } => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, ty)| ty));
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
            _ => return Ok(false),
        }
    }
    Ok(true)
}

/// Shape admission for a real nonlocal complete endpoint. Authority and actual
/// target substitution stay in this kernel's independent origin/loan passes.
fn helper_owned_result(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
    if helper_copy_result(ty, b)? {
        return Ok(true);
    }
    resources::charge_payload(ty, b)?;
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        b.spend(1)?;
        match ty {
            Type::Managed(payload)
                if matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value() => {}
            Type::Managed(payload) if matches!(payload.as_ref(), Type::OwnedStore { .. }) => {
                resources::check_type_for(module, ty, None, b)?;
            }
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                pending.extend(fields);
            }
            Type::Package { fields, .. } | Type::Record { fields, .. } => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, field)| field));
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
            Type::Array { element, .. } => {
                b.spend(1)?;
                pending.push(element);
            }
            Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::F32
            | Type::F64
            | Type::U8
            | Type::U16 => {}
            _ => return Ok(false),
        }
    }
    Ok(true)
}
fn package_helper(
    module: &TypedModule,
    f: &TypedFunction,
    endpoint: &Type,
    parameter: Option<u32>,
    b: &mut Budget,
) -> Check<bool> {
    if f.foreign.is_some()
        || f.callable_contract.is_some()
        || f.callable_return_contract.is_some()
        || f.reference_return_bound.is_some()
        || f.storage_life.is_some()
    {
        return Ok(false);
    }
    if !(if f.environment.is_some() {
        helper_copy_result(&f.return_type, b)?
    } else {
        helper_owned_result(module, &f.return_type, b)?
    }) || (package_owners(endpoint, b)?.is_none()
        && !enum_endpoint(module, endpoint, b)?
        && !dynamic_store::inline_owner(endpoint))
    {
        return Ok(false);
    }
    b.spend(f.parameters.len())?;
    for (index, local) in f.parameters.iter().enumerate() {
        if parameter == Some(index as u32) {
            continue;
        }
        let ty = &f.locals[local.0 as usize].ty;
        match ty {
            Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::F32
            | Type::F64
            | Type::U8
            | Type::U16 => {}
            Type::Managed(payload)
                if matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value() => {}
            Type::Package { .. } | Type::Enum { .. } | Type::Tuple(_) | Type::ErrorUnion { .. }
                if ty == endpoint => {}
            Type::Address { inner, .. } if inner.as_ref() == endpoint => {}
            Type::Desc(payload) | Type::Managed(payload)
                if matches!(payload.as_ref(), Type::OwnedStore { element } if element.as_ref() == endpoint)
                    && matches!(endpoint, Type::Package { .. }) => {}
            _ => return Ok(false),
        }
    }
    Ok(true)
}

/// Deliberately bounded must-Commit proof. A single normal-return block with
/// exactly one real Commit and no call cannot skip, repeat, abort after, or
/// overwrite that Commit through a nested helper. Wider CFGs retain may rows.
fn whole_must(f: &TypedFunction, places: &Inventory, b: &mut Budget) -> Check<bool> {
    // Only a Unit helper with an actual input can export this postcondition.
    // Body-local Commits keep their independent Content/debt checks and never
    // need this call-boundary proof, including a parameterless/non-Unit main.
    if !matches!(f.return_type, Type::Unit)
        || f.parameters.is_empty()
        || !places.whole_package
        || f.blocks.len() != 1
        || f.entry.0 != 0
        || !matches!(f.blocks[0].terminator, Terminator::Return(_))
    {
        return Ok(false);
    }
    // New capture/multiple-address/Store admission remains a may relation.
    if f.environment.is_some() || f.call_kind != zil_ir::CallKind::Direct {
        return Ok(false);
    }
    b.spend(f.parameters.len())?;
    let mut addresses = 0usize;
    for local in &f.parameters {
        match &f.locals[local.0 as usize].ty {
            Type::Address { .. } => addresses += 1,
            Type::Desc(_) => return Ok(false),
            Type::Managed(payload) if matches!(payload.as_ref(), Type::OwnedStore { .. }) => {
                return Ok(false);
            }
            _ => {}
        }
    }
    if addresses != 1 {
        return Ok(false);
    }
    let mut commits = 0usize;
    let mut whole = false;
    for instruction in &f.blocks[0].instructions {
        b.spend(1)?;
        match instruction.kind {
            InstructionKind::CommitPlaceReplace { reservation, .. } => {
                commits += 1;
                whole = package_owners(&places.get(reservation, b)?.ty, b)?
                    .is_some_and(|owners| owners.len() == 1)
                    && !packages::has_children(&places.get(reservation, b)?.ty);
            }
            InstructionKind::Call { .. }
            | InstructionKind::CallClosure { .. }
            | InstructionKind::CallClosureShared { .. }
            | InstructionKind::CallClosureMut { .. }
            | InstructionKind::CallClosureOnce { .. }
            | InstructionKind::CallClosureBorrowed { .. }
            | InstructionKind::WithForeignCallback { .. }
            | InstructionKind::WithScopedThread { .. }
            | InstructionKind::WithSyncCell { .. }
            | InstructionKind::SuspendCall { .. }
            | InstructionKind::TaskYield => return Ok(false),
            _ => {}
        }
    }
    Ok(commits == 1 && whole)
}
/// Finite whole-Enum endpoint, not ordinary Sum qualification. Each owning
/// arm has one direct canonical Copy-record leaf and scalar/Unit companions.
fn enum_endpoint(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
    fn ordinary(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
        resources::charge_payload(ty, b)?;
        if !owned(ty) || !helper_owned_result(module, ty, b)? {
            return Ok(false);
        }
        check_type_for(module, ty, None, b)?;
        Ok(true)
    }
    if matches!(ty, Type::Tuple(_) | Type::ErrorUnion { .. }) {
        return ordinary(module, ty, b);
    }
    let Type::Enum { variants, .. } = ty else {
        return Ok(false);
    };
    b.spend(variants.len())?;
    let mut owns = false;
    for (_, fields) in variants {
        b.spend(fields.len())?;
        let mut arm_owns = false;
        for field in fields {
            match field {
                Type::Managed(payload)
                    if matches!(payload.as_ref(), Type::Record { .. })
                        && payload.is_copy_value() =>
                {
                    arm_owns = true;
                }
                Type::Unit
                | Type::Bool
                | Type::I32
                | Type::I64
                | Type::U64
                | Type::Usize
                | Type::F32
                | Type::F64
                | Type::U8
                | Type::U16 => {}
                _ => return ordinary(module, ty, b),
            }
        }
        owns |= arm_owns;
    }
    Ok(owns)
}
fn merge_row(
    rows: &mut Rows,
    target: &actual::Target,
    value: &Content,
    b: &mut Budget,
) -> Check<bool> {
    b.spend(rows.len())?;
    let mut work = target
        .steps
        .len()
        .checked_add(1)
        .and_then(|n| n.checked_mul(rows.len().saturating_add(1)))
        .ok_or_else(|| limit("replacement effect key work overflow"))?;
    for old in rows.keys() {
        work = work
            .checked_add(old.steps.len().saturating_add(1))
            .ok_or_else(|| limit("replacement effect route width overflow"))?;
    }
    b.spend(work)?;
    if let Some(old) = rows.get_mut(target) {
        if matches!(
            (&*old, value),
            (Content::Product(_), Content::Product(_)) | (Content::Sum(_, _), Content::Sum(_, _))
        ) {
            old.merge(value, b)
        } else {
            merge_leaf(old, value, b)
        }
    } else {
        b.spend(target.steps.len().saturating_add(1))?;
        value.cost(b)?;
        rows.insert(target.clone(), value.clone());
        Ok(true)
    }
}
pub(crate) fn at_type<'a>(
    module: &'a TypedModule,
    f: &'a TypedFunction,
    target: &actual::Target,
    b: &mut Budget,
) -> Check<&'a Type> {
    let ty = match target.base {
        actual::Base::Local(local) => &f.locals[local.0 as usize].ty,
        actual::Base::Input(p) => &f.locals[f.parameters[p as usize].0 as usize].ty,
        actual::Base::Environment(c) => {
            &module.closures[f
                .environment
                .ok_or_else(|| invalid("replacement capture has no actual environment"))?
                .0 as usize]
                .captures[c as usize]
        }
        actual::Base::InputCapture(p, c) => {
            let ty = &f.locals[f.parameters[p as usize].0 as usize].ty;
            let ty = if let Type::Address { inner, .. } = ty {
                inner.as_ref()
            } else {
                ty
            };
            let Type::Closure(id) = ty else {
                return Err(invalid("replacement input capture has no actual closure"));
            };
            &module.closures[id.0 as usize].captures[c as usize]
        }
        actual::Base::Element(value) => {
            if !matches!(target.steps.as_slice(), [] | [PlaceProjection::Field(_)]) {
                return Err(unsupported(
                    "Store replacement requires a direct complete owning field",
                    None,
                ));
            }
            &f.values[value.0 as usize]
        }
    };
    let mut ty = if matches!(target.base, actual::Base::Local(_)) {
        ty
    } else {
        let Type::Address {
            mutable: true,
            inner,
        } = ty
        else {
            return Err(invalid(
                "replacement needs an actual mutable input/capture address",
            ));
        };
        inner.as_ref()
    };
    if !target.mutable {
        return Err(invalid("replacement cannot promote a shared actual source"));
    }
    let owner_root = ty;
    let mut direct_package_field = !target.steps.is_empty();
    b.spend(target.steps.len().saturating_add(1))?;
    for step in &target.steps {
        direct_package_field &=
            matches!(step, PlaceProjection::Field(_)) && matches!(ty, Type::Package { .. });
        ty = match (step, ty) {
            (PlaceProjection::Field(i), Type::Tuple(fs)) => fs.get(*i as usize),
            (
                PlaceProjection::Field(i),
                Type::Package { fields, .. } | Type::Record { fields, .. },
            ) => fields.get(*i as usize).map(|(_, t)| t),
            (PlaceProjection::Variant(p), Type::Enum { variants, .. }) => variants
                .get(p.variant as usize)
                .and_then(|(_, fs)| fs.get(p.index as usize)),
            (PlaceProjection::ErrorArm(e), Type::ErrorUnion { error, result }) => {
                Some(if *e { error.as_ref() } else { result.as_ref() })
            }
            _ => None,
        }
        .ok_or_else(|| invalid("replacement actual route crosses a view or different Type"))?;
    }
    resources::charge_payload(ty, b)?;
    let inline_store = dynamic_store::inline_owner(ty);
    if inline_store {
        resources::check_type_for(module, ty, None, b)?;
    }
    if !matches!(ty,Type::Managed(payload) if matches!(payload.as_ref(),Type::Record{..})&&payload.is_copy_value())
    {
        let whole_local = matches!(target.base, actual::Base::Local(_)) && target.steps.is_empty();
        let whole_input = target.steps.is_empty()
            && match target.base {
                actual::Base::Input(p) => package_helper(module, f, ty, Some(p), b)?,
                actual::Base::Environment(_) => package_helper(module, f, ty, None, b)?,
                actual::Base::InputCapture(p, c) => {
                    f.foreign.is_none()
                        && f.callable_return_contract.is_none()
                        && f.reference_return_bound.is_none()
                        && f.storage_life.is_none()
                        && helper_copy_result(&f.return_type, b)?
                        && resources::public_roles::mutable_capture(module, f, p, c, b)?
                }
                actual::Base::Element(_) => {
                    matches!(ty, Type::Package { .. }) && package_helper(module, f, ty, None, b)?
                }
                _ => false,
            };
        let store_field = inline_store
            && direct_package_field
            && package_owners(owner_root, b)?.is_some()
            && match target.base {
                actual::Base::Local(local) => !f.parameters.contains(&local),
                actual::Base::Input(p) => package_helper(module, f, owner_root, Some(p), b)?,
                actual::Base::Environment(_) => package_helper(module, f, owner_root, None, b)?,
                actual::Base::InputCapture(p, c) => {
                    f.foreign.is_none()
                        && f.callable_return_contract.is_none()
                        && f.reference_return_bound.is_none()
                        && f.storage_life.is_none()
                        && helper_copy_result(&f.return_type, b)?
                        && resources::public_roles::mutable_capture(module, f, p, c, b)?
                }
                actual::Base::Element(_) => false,
            };
        if (!whole_local && !whole_input && !store_field)
            || (package_owners(ty, b)?.is_none() && !enum_endpoint(module, ty, b)? && !inline_store)
        {
            return Err(unsupported(
                "owning replacement endpoint cleanup/hold bridge is under construction",
                None,
            ));
        }
        b.spend(f.parameters.len())?;
        if let actual::Base::Local(local) = target.base
            && f.parameters.contains(&local)
        {
            return Err(unsupported(
                "whole Package/Enum replacement requires a body-owned nonparameter local",
                None,
            ));
        }
    }
    if matches!(ty, Type::Tuple(_) | Type::ErrorUnion { .. }) {
        // Successful independent whole-endpoint reconstruction also enables
        // the existing complete Sum drop recipes on this verified module.
        b.mark_sum(Some(f.id.0 as usize))?;
    }
    Ok(ty)
}
/// Consume only an actual current edge observation reconstructed by this
/// kernel. The declaration/path alone never narrows a candidate arm set.
pub(super) fn refine_actual_error_arm(
    state: &mut State,
    local: LocalId,
    tag: u32,
    equal: bool,
    b: &mut Budget,
) -> Check<()> {
    if tag > 1 || state.initialized[local.0 as usize] != 2 {
        return Ok(());
    }
    let value = &mut state.locals[local.0 as usize];
    value.cost(b)?; // includes a possible Arc copy before mutation
    let Content::Sum(arms, _) = value.as_ref() else {
        return Err(invalid(
            "actual error tag observation has no ErrorUnion content",
        ));
    };
    b.spend(arms.len().saturating_mul(2))?;
    // Runtime tag is is_error(0/1); this kernel's Content convention is
    // error=0/success=1. Keep its independent convention explicit.
    let selected = if tag == 0 { 1 } else { 0 };
    let keep = |key: u32| (key == selected) == equal;
    if !arms.keys().any(|key| keep(*key)) {
        // Preserve the existing conservative treatment of unreachable CFG
        // edges. Never turn a missing/empty alternative into initialized data.
        return Ok(());
    }
    let Content::Sum(arms, _) = Arc::make_mut(value) else {
        unreachable!()
    };
    arms.retain(|key, _| keep(*key));
    Ok(())
}
fn selected<'a>(
    value: &'a Content,
    path: &[PlaceProjection],
    validate: bool,
    b: &mut Budget,
) -> Check<&'a Content> {
    b.spend(1)?;
    if matches!(value, Content::Missing(_)) {
        return Err(invalid("replacement traverses a moved owner subtree"));
    }
    let Some((step, rest)) = path.split_first() else {
        if validate && !value.complete(b)? {
            return Err(invalid(
                "replacement endpoint is not completely initialized",
            ));
        }
        return Ok(value);
    };
    let next = match (step, value) {
        (PlaceProjection::Field(i), Content::Product(fs)) => fs.get(*i as usize),
        (PlaceProjection::Variant(p), Content::Sum(vs, _)) => {
            if validate && vs.len() != 1 {
                return Err(invalid("replacement has no exact active variant"));
            }
            vs.get(&p.variant).and_then(|fs| fs.get(p.index as usize))
        }
        (PlaceProjection::ErrorArm(e), Content::Sum(vs, _)) => {
            if validate && vs.len() != 1 {
                return Err(invalid("replacement has no exact active error arm"));
            }
            vs.get(&if *e { 0 } else { 1 }).and_then(|fs| fs.first())
        }
        _ => None,
    }
    .ok_or_else(|| invalid("replacement initialization path differs from actual content"))?;
    selected(next, rest, validate, b)
}
fn update(
    value: &mut Content,
    path: &[PlaceProjection],
    incoming: &Content,
    validate: bool,
    b: &mut Budget,
) -> Check<()> {
    b.spend(1)?;
    if matches!(value, Content::Missing(_)) {
        return Err(invalid("replacement writes a moved owner subtree"));
    }
    let Some((step, rest)) = path.split_first() else {
        merge_endpoint(value, incoming, b)?;
        return Ok(());
    };
    let next = match (step, value) {
        (PlaceProjection::Field(i), Content::Product(fs)) => fs.get_mut(*i as usize),
        (PlaceProjection::Variant(p), Content::Sum(vs, _)) => {
            if validate && vs.len() != 1 {
                return Err(invalid("replacement lost its active variant"));
            }
            vs.get_mut(&p.variant)
                .and_then(|fs| fs.get_mut(p.index as usize))
        }
        (PlaceProjection::ErrorArm(e), Content::Sum(vs, _)) => {
            if validate && vs.len() != 1 {
                return Err(invalid("replacement lost its active error arm"));
            }
            vs.get_mut(&if *e { 0 } else { 1 })
                .and_then(|fs| fs.first_mut())
        }
        _ => None,
    }
    .ok_or_else(|| invalid("replacement content update route is absent"))?;
    update(next, rest, incoming, validate, b)
}
impl Context<'_> {
    // Keep authority, update strength, validation and budget explicit, as in
    // the adjacent resource replacement kernel.
    #[allow(clippy::too_many_arguments)]
    fn apply(
        &self,
        flow: &Flow<'_>,
        state: &mut State,
        target: &actual::Target,
        value: &Content,
        strong: bool,
        validate: bool,
        b: &mut Budget,
    ) -> Check<()> {
        at_type(flow.module, flow.f, target, b)?;
        match target.base {
            actual::Base::Local(local) => {
                if validate && state.initialized[local.0 as usize] != 2 {
                    return Err(invalid("replacement owner shell is not initialized"));
                }
                selected(&state.locals[local.0 as usize], &target.steps, validate, b)?;
                state.locals[local.0 as usize].cost(b)?;
                if !strong
                    && target.steps.is_empty()
                    && matches!(value, Content::Product(_) | Content::Sum(_, _))
                {
                    // A may-Commit preserves ordinary initialized access, but
                    // a no-write return must retain the old leaf's origin.
                    Arc::make_mut(&mut state.locals[local.0 as usize]).merge(value, b)?;
                } else {
                    update(
                        Arc::make_mut(&mut state.locals[local.0 as usize]),
                        &target.steps,
                        value,
                        validate,
                        b,
                    )?;
                }
                if target.steps.is_empty()
                    && matches!(
                        flow.f.locals[local.0 as usize].ty,
                        Type::Enum { .. } | Type::Tuple(_) | Type::ErrorUnion { .. }
                    )
                {
                    // The old observation belongs to the retired instance.
                    // The incoming Content alone carries the new tag set.
                    flow.kill(state, local, b)?;
                }
            }
            // An actual borrowed Store element is complete and retained by the
            // parent Store prefix. Replacement neither constructs an element
            // nor exports a fresh Content claim for later Store pop.
            actual::Base::Element(_) => {}
            _ => {
                if matches!(target.base, actual::Base::Environment(_))
                    && flow.f.environment.is_some_and(|id| {
                        flow.module.closures[id.0 as usize].kind == CallableKind::Fn
                    })
                {
                    return Err(invalid(
                        "shared Fn entry cannot replace through a captured exclusive address",
                    ));
                }
                merge_row(&mut self.written.borrow_mut(), target, value, b)?;
            }
        }
        Ok(())
    }
    fn call(
        &self,
        flow: &Flow<'_>,
        state: &mut State,
        invocation: Invocation<'_>,
        validate: bool,
        b: &mut Budget,
    ) -> Check<()> {
        let Invocation {
            entry,
            arguments,
            captures,
            may_only,
        } = invocation;
        let rows = &self.summaries[entry];
        b.spend(rows.len())?;
        for (target, payload) in rows {
            b.spend(target.steps.len().saturating_add(1))?;
            let targets = actual::substitute(
                &BTreeSet::from([target.clone()]),
                arguments,
                captures,
                &self.actual.functions[flow.f.id.0 as usize],
                b,
            )?;
            if targets.is_empty() {
                return Err(invalid(
                    "replacement call has no actual selected destination",
                ));
            }
            let whole = matches!(payload, Content::Product(_) | Content::Sum(_, _));
            if whole && targets.len() != 1 {
                return Err(unsupported(
                    "whole Package helper needs one independently reconstructed actual destination",
                    None,
                ));
            }
            let strong = if whole {
                lookup(b, self.whole_must.len())?;
                !may_only
                    && matches!(payload, Content::Product(_))
                    && self.whole_must.contains(&entry)
            } else {
                true
            };
            let incoming = flow.instantiate(payload, arguments, state, b)?;
            for target in targets {
                if matches!(target.base, actual::Base::Element(_)) {
                    let mut needed = self.store_calls.borrow_mut();
                    b.spend(needed.len().saturating_add(1))?;
                    needed.insert(entry);
                }
                self.apply(flow, state, &target, &incoming, strong, validate, b)?;
            }
        }
        Ok(())
    }
    pub(super) fn step(
        &self,
        flow: &Flow<'_>,
        state: &mut State,
        i: &Instruction,
        validate: bool,
        b: &mut Budget,
    ) -> Check<Option<Content>> {
        let f = flow.f;
        let facts = &self.actual.functions[f.id.0 as usize];
        match &i.kind {
            InstructionKind::ReservePlaceLocal { reservation, .. }
            | InstructionKind::ReservePlaceAddress { reservation, .. } => {
                let place = self.places[f.id.0 as usize].get(*reservation, b)?;
                for target in self.actual.selected(f, place, b)? {
                    let ty = at_type(flow.module, f, &target, b)?;
                    resources::charge_payload(ty, b)?;
                    if ty != &place.ty {
                        return Err(invalid("reservation Type differs from actual endpoint"));
                    }
                    if validate && let actual::Base::Local(local) = target.base {
                        if state.initialized[local.0 as usize] != 2 {
                            return Err(invalid("reservation pins an uninitialized owner"));
                        }
                        selected(&state.locals[local.0 as usize], &target.steps, true, b)?;
                    }
                }
            }
            InstructionKind::CommitPlaceReplace { reservation, value } => {
                let place = self.places[f.id.0 as usize].get(*reservation, b)?;
                let incoming = state.values[value.0 as usize].duplicate(b)?;
                if validate
                    && matches!(
                        place.ty,
                        Type::Package { .. }
                            | Type::Enum { .. }
                            | Type::Tuple(_)
                            | Type::ErrorUnion { .. }
                    )
                    && !incoming.complete(b)?
                {
                    return Err(invalid("whole replacement RHS is not complete"));
                }
                for target in self.actual.selected(f, place, b)? {
                    if f.callable_contract.is_some()
                        && !matches!(target.base, actual::Base::Local(_))
                    {
                        return Err(unsupported(
                            "public callable helper cannot publish its own nonlocal Commit",
                            Some(i.span),
                        ));
                    }
                    self.apply(flow, state, &target, &incoming, true, validate, b)?;
                }
            }
            InstructionKind::Call {
                function,
                arguments,
            } => {
                let callee = &flow.module.functions[function.0 as usize];
                if let Some(contract) = &callee.callable_contract {
                    b.spend(self.summaries[function.0 as usize].len())?;
                    if self.summaries[function.0 as usize]
                        .keys()
                        .any(|target| !matches!(target.base, actual::Base::InputCapture(..)))
                    {
                        return Err(unsupported(
                            "public callable helper has an unpublished replacement effect",
                            Some(i.span),
                        ));
                    }
                    for (entry, value) in
                        resources::public_roles::invoked(flow.module, f, contract, arguments, b)?
                    {
                        b.spend(self.summaries[entry].len())?;
                        for target in self.summaries[entry].keys() {
                            if !matches!(target.base, actual::Base::Environment(_)) {
                                return Err(unsupported(
                                    "callback replacement needs an actual captured destination; callback argument mapping is absent",
                                    Some(i.span),
                                ));
                            }
                        }
                        self.call(
                            flow,
                            state,
                            Invocation {
                                entry,
                                arguments: &[],
                                captures: Some(&facts.captures[value.0 as usize]),
                                may_only: true,
                            },
                            validate,
                            b,
                        )?;
                    }
                    return Ok(None);
                }
                if f.callable_contract.is_some() && !self.summaries[function.0 as usize].is_empty()
                {
                    return Err(unsupported(
                        "public callable helper cannot publish an ordinary callee's private replacement effects",
                        Some(i.span),
                    ));
                }
                if (callee.callable_contract.is_some() || callee.callable_return_contract.is_some())
                    && !self.summaries[function.0 as usize].is_empty()
                {
                    return Err(unsupported(
                        "replacement through a callable helper needs a checked public replacement envelope",
                        Some(i.span),
                    ));
                }
                self.call(
                    flow,
                    state,
                    Invocation {
                        entry: function.0 as usize,
                        arguments,
                        captures: None,
                        may_only: false,
                    },
                    validate,
                    b,
                )?;
            }
            InstructionKind::CallClosureShared { callee, arguments }
            | InstructionKind::CallClosureMut { callee, arguments } => {
                let entry = resources::effect_closure_target(
                    flow.module,
                    &f.locals[callee.0 as usize].ty,
                    i.span,
                )?;
                lookup(b, facts.local_captures.len())?;
                self.call(
                    flow,
                    state,
                    Invocation {
                        entry,
                        arguments,
                        captures: facts.local_captures.get(callee),
                        may_only: false,
                    },
                    validate,
                    b,
                )?;
            }
            InstructionKind::CallClosure { callee, arguments }
            | InstructionKind::CallClosureOnce { callee, arguments }
            | InstructionKind::CallClosureBorrowed {
                callee, arguments, ..
            } => {
                let entry = resources::effect_closure_target(
                    flow.module,
                    &f.values[callee.0 as usize],
                    i.span,
                )?;
                self.call(
                    flow,
                    state,
                    Invocation {
                        entry,
                        arguments,
                        captures: Some(&facts.captures[callee.0 as usize]),
                        may_only: false,
                    },
                    validate,
                    b,
                )?;
            }
            InstructionKind::WithForeignCallback { callback, body, .. } => {
                for value in [*callback, *body] {
                    let entry = resources::effect_closure_target(
                        flow.module,
                        &f.values[value.0 as usize],
                        i.span,
                    )?;
                    self.call(
                        flow,
                        state,
                        Invocation {
                            entry,
                            arguments: &[],
                            captures: Some(&facts.captures[value.0 as usize]),
                            may_only: false,
                        },
                        validate,
                        b,
                    )?;
                }
            }
            InstructionKind::WithScopedThread { body, .. }
            | InstructionKind::WithSyncCell { body, .. } => {
                let entry = resources::effect_closure_target(
                    flow.module,
                    &f.values[body.0 as usize],
                    i.span,
                )?;
                self.call(
                    flow,
                    state,
                    Invocation {
                        entry,
                        arguments: &[],
                        captures: Some(&facts.captures[body.0 as usize]),
                        may_only: false,
                    },
                    validate,
                    b,
                )?;
            }
            _ => {}
        }
        Ok(None)
    }
}
pub(super) fn check(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    b: &mut Budget,
) -> Check<Proofs> {
    let actual = actual::Program::reconstruct(module, graphs, b)?;
    b.spend(module.functions.len().saturating_mul(3))?;
    let mut places = Vec::with_capacity(module.functions.len());
    let mut result = Vec::with_capacity(module.functions.len());
    let mut effects = vec![Rows::new(); module.functions.len()];
    let mut whole_must_rows = BTreeSet::new();
    let store_calls = RefCell::new(BTreeSet::new());
    for (f, graph) in module.functions.iter().zip(graphs) {
        let inventory = Inventory::reconstruct(module, f, b)?;
        if whole_must(f, &inventory, b)? {
            b.spend(whole_must_rows.len().saturating_add(1))?;
            whole_must_rows.insert(f.id.0 as usize);
        }
        if f.foreign.is_none() {
            inventory.activation_entries(f, graph, b)?;
        }
        places.push(inventory);
        result.push(content(&f.return_type, None, &mut Vec::new(), true, b)?);
    }
    loop {
        let mut changed = false;
        for (f, graph) in module.functions.iter().zip(graphs) {
            b.spend(1)?;
            if f.foreign.is_some() {
                continue;
            }
            let context = Context {
                actual: &actual,
                places: &places,
                summaries: &effects,
                whole_must: &whole_must_rows,
                written: RefCell::new(Rows::new()),
                store_calls: &store_calls,
            };
            let (found, _) =
                run_with_replacements(module, f, graph, &result, false, b, Some(&context))?;
            let rows = context.written.into_inner();
            changed |= result[f.id.0 as usize].merge(&found, b)?;
            b.spend(rows.len())?;
            for (target, payload) in rows {
                changed |= merge_row(&mut effects[f.id.0 as usize], &target, &payload, b)?;
            }
        }
        if !changed {
            break;
        }
    }
    let mut rows = Vec::with_capacity(module.functions.len());
    for (f, graph) in module.functions.iter().zip(graphs) {
        if f.foreign.is_some() {
            rows.push(Vec::new());
            continue;
        }
        let context = Context {
            actual: &actual,
            places: &places,
            summaries: &effects,
            whole_must: &whole_must_rows,
            written: RefCell::new(Rows::new()),
            store_calls: &store_calls,
        };
        rows.push(run_with_replacements(module, f, graph, &result, true, b, Some(&context))?.1);
    }
    b.spend(effects.len())?;
    let mut replacement_writes = Vec::with_capacity(effects.len());
    let needed = store_calls.into_inner();
    let mut replacement_targets = BTreeMap::new();
    let mut retirement_targets = BTreeMap::new();
    for (entry, effect) in effects.into_iter().enumerate() {
        let mut writes = BTreeSet::new();
        let mut targets = Vec::new();
        let mut retired = Vec::new();
        b.spend(needed.len())?;
        let transfer = needed.contains(&entry);
        b.spend(effect.len())?;
        for (target, _) in effect {
            let root = match target.base {
                actual::Base::Input(p) | actual::Base::InputCapture(p, _) => {
                    Some(resources::Root::Parameter(p))
                }
                actual::Base::Environment(c) => Some(resources::Root::BorrowedCapture(c)),
                _ => None,
            };
            if let Some(root) = root {
                lookup(b, writes.len())?;
                writes.insert(root);
                if transfer {
                    b.spend(1)?;
                    targets.push(target);
                } else {
                    // The exact input/capture ordinal and full route survive
                    // the boundary. Ordinary write roots lose this identity.
                    b.spend(target.steps.len().saturating_add(1))?;
                    retired.push(target);
                }
            }
        }
        replacement_writes.push(writes);
        if !targets.is_empty() {
            b.spend(replacement_targets.len().saturating_add(1))?;
            replacement_targets.insert(entry, targets);
        }
        if !retired.is_empty() {
            b.spend(retirement_targets.len().saturating_add(1))?;
            retirement_targets.insert(entry, retired);
        }
    }
    Ok(Proofs {
        rows,
        replacement_writes,
        replacement_targets,
        retirement_targets,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_content_selects_and_updates_only_the_actual_error_arm() {
        for is_error in [false, true] {
            let limits = Limits::default();
            let mut budget = Budget::new(&limits);
            let original = Content::Leaf(BTreeSet::from([Source::Fresh]));
            let incoming = Content::Leaf(BTreeSet::from([Source::Input(0, vec![])]));
            // Use this kernel's actual MakeErrorUnion convention. Analyze's
            // private Node convention is intentionally not consulted here.
            let mut value = Content::Sum(
                BTreeMap::from([(if is_error { 0 } else { 1 }, vec![original.clone()])]),
                None,
            );
            let path = [PlaceProjection::ErrorArm(is_error)];
            let wrong = [PlaceProjection::ErrorArm(!is_error)];
            assert_eq!(
                selected(&value, &path, true, &mut budget).unwrap(),
                &original
            );
            assert!(selected(&value, &wrong, true, &mut budget).is_err());
            let before = value.clone();
            assert!(update(&mut value, &wrong, &incoming, true, &mut budget).is_err());
            assert_eq!(value, before);
            update(&mut value, &path, &incoming, true, &mut budget).unwrap();
            assert_eq!(
                selected(&value, &path, true, &mut budget).unwrap(),
                &Content::Leaf(BTreeSet::from([Source::Fresh, Source::Input(0, vec![]),]))
            );
        }
    }
}

#[cfg(test)]
mod package_tests {
    use super::*;
    #[test]
    fn complete_package_content_keeps_leaf_provenance_and_rejects_partial_subtrees() {
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        let fresh = Content::Leaf(BTreeSet::from([Source::Fresh]));
        let input = Content::Leaf(BTreeSet::from([Source::Input(0, vec![])]));
        let old = Content::Product(vec![fresh.clone(), Content::Copy]);
        let incoming = Content::Product(vec![input.clone(), Content::Copy]);
        let mut combined = old.clone();
        merge_endpoint(&mut combined, &incoming, &mut budget).unwrap();
        assert_eq!(
            combined,
            Content::Product(vec![
                Content::Leaf(BTreeSet::from([Source::Input(0, vec![])])),
                Content::Copy
            ])
        );
        for bad in [
            Content::Product(vec![
                Content::Missing(Box::new(input.clone())),
                Content::Copy,
            ]),
            Content::Product(vec![input.clone()]),
            Content::Product(vec![input, fresh]),
        ] {
            assert!(merge_endpoint(&mut old.clone(), &bad, &mut budget).is_err());
        }
    }
}

#[cfg(test)]
mod enum_tests {
    use super::*;

    #[test]
    fn complete_enum_replacement_retires_old_arms_and_preserves_only_rhs_origins() {
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        let fresh = Content::Leaf(BTreeSet::from([Source::Fresh]));
        let input = Content::Leaf(BTreeSet::from([Source::Input(0, vec![1, 1, 1])]));
        let old = Content::Sum(
            BTreeMap::from([(0, vec![fresh.clone(), Content::Copy])]),
            None,
        );
        let incoming = Content::Sum(
            BTreeMap::from([(1, vec![Content::Copy, input])]),
            Some((0, vec![])),
        );
        let mut value = old.clone();
        update(&mut value, &[], &incoming, true, &mut b).unwrap();
        assert_eq!(value, incoming);
        update(&mut value, &[], &old, true, &mut b).unwrap();
        assert_eq!(value, old);
        let empty = Content::Sum(BTreeMap::from([(2, vec![])]), None);
        update(&mut value, &[], &empty, true, &mut b).unwrap();
        assert_eq!(value, empty);
    }
}
