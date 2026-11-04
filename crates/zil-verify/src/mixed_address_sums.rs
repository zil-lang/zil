//! Independent addressed-Sum tag checker. Facts refer to the actual address SSA
//! identity and are must facts of the current CFG epoch, never copied values.
use super::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
type Prefix = Vec<(u8, u32, u32)>;
type Selection = (ValueId, Prefix, u32);
#[derive(Clone, Default, PartialEq, Eq)]
struct Epoch {
    selected: BTreeSet<Selection>,
    reads: BTreeSet<ValueId>,
}
fn invalid(span: Option<Span>) -> Diagnostic {
    reject(
        "E-VERIFY-SUM-ADDRESS",
        span,
        "addressed payload requires a current real-address tag proof",
    )
}
fn endpoint<'a>(
    f: &'a TypedFunction,
    address: ValueId,
    path: &[zil_ir::PlaceProjection],
    budget: &mut Budget,
) -> Check<(bool, &'a Type)> {
    let Type::Address { mutable, inner } = value_type(f, address, None)? else {
        return Err(invalid(None));
    };
    let mut cursor = inner.as_ref();
    budget.spend(path.len().saturating_add(1))?;
    for projection in path {
        cursor = match (projection, cursor) {
            (zil_ir::PlaceProjection::Field(i), Type::Tuple(fields)) => fields.get(*i as usize),
            (
                zil_ir::PlaceProjection::Field(i),
                Type::Record { fields, .. } | Type::Package { fields, .. },
            ) => fields.get(*i as usize).map(|(_, t)| t),
            (zil_ir::PlaceProjection::Variant(p), Type::Enum { variants, .. }) => variants
                .get(p.variant as usize)
                .and_then(|(_, fs)| fs.get(p.index as usize)),
            (zil_ir::PlaceProjection::ErrorArm(e), Type::ErrorUnion { error, result }) => {
                Some(if *e { error.as_ref() } else { result.as_ref() })
            }
            _ => None,
        }
        .ok_or_else(|| invalid(None))?;
    }
    Ok((*mutable, cursor))
}
pub(super) fn instruction(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    budget: &mut Budget,
) -> Check<Option<Type>> {
    let span = Some(i.span);
    let depth = match &i.kind {
        InstructionKind::PlaceTagAddress { path, .. }
        | InstructionKind::BorrowPlaceAddress { path, .. } => path.len(),
        _ => 0,
    };
    if depth > budget.limits.max_call_depth {
        return Err(limit("address place projection depth exceeded"));
    }
    let (address, path) = match &i.kind {
        InstructionKind::PlaceTagAddress { address, path }
        | InstructionKind::BorrowPlaceAddress { address, path, .. } => (*address, path),
        _ => unreachable!(),
    };
    let (available, ty) = endpoint(f, address, path, budget)?;
    resources::charge_payload(ty, budget)?;
    if let InstructionKind::BorrowPlaceAddress { mutable, .. } = &i.kind {
        if path.is_empty()
            || (*mutable && !available)
            || (!ty.is_copy_value()
                && !matches!(
                    ty,
                    Type::Tuple(_)
                        | Type::Package { .. }
                        | Type::Enum { .. }
                        | Type::ErrorUnion { .. }
                ))
        {
            return Err(invalid(span));
        }
        match ty {
            Type::Package { .. } => packages::check_ordinary_type(module, ty, span, budget)?,
            Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. } => {
                sums::check_type_for(module, ty, span, budget)?
            }
            _ => check_type(ty, span)?,
        }
        Ok(Some(Type::Address {
            mutable: *mutable,
            inner: Box::new(ty.clone()),
        }))
    } else {
        if !matches!(ty, Type::Enum { .. } | Type::ErrorUnion { .. }) {
            return Err(invalid(span));
        }
        sums::check_type_for(module, ty, span, budget)?;
        Ok(Some(Type::I32))
    }
}
fn cost(e: &Epoch, b: &mut Budget) -> Check<()> {
    b.spend(
        e.selected
            .len()
            .checked_add(e.reads.len())
            .and_then(|n| n.checked_add(1))
            .ok_or_else(exhausted)?,
    )?;
    let paths = e.selected.iter().try_fold(0usize, |n, (_, p, _)| {
        n.checked_add(p.len()).ok_or_else(exhausted)
    })?;
    b.spend(paths)
}
fn copy(e: &Epoch, b: &mut Budget) -> Check<Epoch> {
    cost(e, b)?;
    Ok(e.clone())
}
fn validate_path(
    e: &Epoch,
    address: ValueId,
    path: &[zil_ir::PlaceProjection],
    span: Span,
    b: &mut Budget,
) -> Check<()> {
    let mut route = Vec::new();
    for step in path {
        search_fee(b, route.len(), e.selected.len())?;
        let tag = match step {
            zil_ir::PlaceProjection::Variant(v) => Some(v.variant),
            zil_ir::PlaceProjection::ErrorArm(e) => Some(u32::from(*e)),
            _ => None,
        };
        if let Some(tag) = tag
            && !e.selected.contains(&(address, route.clone(), tag))
        {
            return Err(invalid(Some(span)));
        }
        route.push(step_key(step));
    }
    Ok(())
}
fn mutates(i: &InstructionKind) -> bool {
    !matches!(
        i,
        InstructionKind::Constant(_)
            | InstructionKind::Unary { .. }
            | InstructionKind::Binary { .. }
            | InstructionKind::ConvertInteger { .. }
            | InstructionKind::Load(_)
            | InstructionKind::ReadCapture { .. }
            | InstructionKind::ReadAddress { .. }
            | InstructionKind::AddressField { .. }
            | InstructionKind::AddressIndex { .. }
            | InstructionKind::AddressSlice { .. }
            | InstructionKind::ReborrowAddress { .. }
            | InstructionKind::BorrowLocal { .. }
            | InstructionKind::BorrowVariantLocal { mutable: false, .. }
            | InstructionKind::BorrowFieldLocal { mutable: false, .. }
            | InstructionKind::VariantTagLocal { .. }
            | InstructionKind::ReadVariantLocal { .. }
    )
}

fn transfer(
    f: &TypedFunction,
    block: usize,
    epoch: &mut Epoch,
    validate: bool,
    b: &mut Budget,
) -> Check<()> {
    for i in &f.blocks[block].instructions {
        b.spend(1)?;
        match &i.kind {
            InstructionKind::PlaceTagAddress { address, path } => {
                if validate {
                    validate_path(epoch, *address, path, i.span, b)?
                }
                search_fee(b, 0, epoch.reads.len())?;
                epoch.reads.insert(i.result.unwrap());
            }
            InstructionKind::BorrowPlaceAddress {
                address,
                path,
                mutable: _,
            } => {
                if validate {
                    validate_path(epoch, *address, path, i.span, b)?
                }
            }
            other if mutates(other) => {
                b.spend(
                    epoch
                        .selected
                        .len()
                        .checked_add(epoch.reads.len())
                        .ok_or_else(exhausted)?,
                )?;
                epoch.selected.clear();
                epoch.reads.clear();
            }
            _ => {}
        }
    }
    Ok(())
}
pub(super) fn check(
    f: &TypedFunction,
    successors: &[Vec<usize>],
    reachable: &[bool],
    b: &mut Budget,
) -> Check<()> {
    let mut observations = BTreeMap::new();
    let mut definitions = BTreeMap::new();
    // Work is already preflight-selected for functions with an addressed Sum op.
    b.spend(
        f.values
            .len()
            .saturating_add(f.blocks.len().saturating_mul(2)),
    )?;
    for block in &f.blocks {
        for i in &block.instructions {
            b.spend(1)?;
            if let Some(v) = i.result {
                search_fee(b, 0, definitions.len())?;
                definitions.insert(v, &i.kind);
                if let InstructionKind::PlaceTagAddress { address, path } = &i.kind {
                    b.spend(path.len())?;
                    search_fee(b, 0, observations.len())?;
                    observations.insert(v, (*address, path));
                }
            }
        }
    }
    let mut incoming = vec![None; f.blocks.len()];
    incoming[f.entry.0 as usize] = Some(Epoch::default());
    let mut todo = VecDeque::from([f.entry.0 as usize]);
    while let Some(block) = todo.pop_front() {
        let mut output = copy(incoming[block].as_ref().unwrap(), b)?;
        transfer(f, block, &mut output, false, b)?;
        let (edges, count) = match f.blocks[block].terminator {
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => (
                [
                    (then_block.0 as usize, Some(true)),
                    (else_block.0 as usize, Some(false)),
                ],
                2,
            ),
            _ => {
                let next = successors[block].first().copied();
                (
                    [(next.unwrap_or(0), None), (0, None)],
                    usize::from(next.is_some()),
                )
            }
        };
        for (successor, truth) in edges.into_iter().take(count) {
            let mut edge = copy(&output, b)?;
            // Prepay all bounded ordered-map/set lookups used to recognize
            // a possible equality edge, including a path with no tag facts.
            search_fee(b, 0, definitions.len())?;
            for _ in 0..2 {
                search_fee(b, 0, definitions.len())?;
                search_fee(b, 0, observations.len())?;
                search_fee(b, 0, output.reads.len())?;
            }
            if let Terminator::Branch { condition, .. } = f.blocks[block].terminator
                && let Some(InstructionKind::Binary { op, left, right }) =
                    definitions.get(&condition).copied()
                && matches!(op, BinaryOp::Equal | BinaryOp::NotEqual)
            {
                for (tag, literal) in [(*left, *right), (*right, *left)] {
                    if let (
                        Some((address, path)),
                        Some(InstructionKind::Constant(Constant::I32(variant))),
                    ) = (observations.get(&tag), definitions.get(&literal).copied())
                        && *variant >= 0
                        && output.reads.contains(&tag)
                        && truth == Some(*op == BinaryOp::Equal)
                    {
                        search_fee(b, path.len(), edge.selected.len())?;
                        let route = path.iter().map(step_key).collect::<Vec<_>>();
                        edge.selected.insert((*address, route, *variant as u32));
                    }
                }
            }
            let changed = if let Some(prior) = &mut incoming[successor] {
                cost(prior, b)?;
                cost(&edge, b)?;
                for (_, p, _) in &prior.selected {
                    search_fee(b, p.len(), edge.selected.len())?;
                }
                for _ in &prior.reads {
                    search_fee(b, 0, edge.reads.len())?;
                }
                let lengths = (prior.selected.len(), prior.reads.len());
                prior.selected.retain(|f| edge.selected.contains(f));
                prior.reads.retain(|r| edge.reads.contains(r));
                lengths != (prior.selected.len(), prior.reads.len())
            } else {
                incoming[successor] = Some(edge);
                true
            };
            if changed {
                todo.push_back(successor)
            }
        }
    }
    for (index, live) in reachable.iter().enumerate() {
        if *live {
            let mut epoch = copy(incoming[index].as_ref().ok_or_else(|| invalid(None))?, b)?;
            transfer(f, index, &mut epoch, true, b)?
        }
    }
    Ok(())
}

fn step_key(p: &zil_ir::PlaceProjection) -> (u8, u32, u32) {
    match p {
        zil_ir::PlaceProjection::Field(i) => (0, *i, 0),
        zil_ir::PlaceProjection::Variant(v) => (1, v.variant, v.index),
        zil_ir::PlaceProjection::ErrorArm(e) => (2, u32::from(*e), 0),
    }
}

fn exhausted() -> Diagnostic {
    limit("mixed address accounting overflow")
}
fn search_fee(b: &mut Budget, path: usize, set: usize) -> Check<()> {
    let bound = path
        .checked_add(1)
        .and_then(|n| n.checked_mul(8))
        .and_then(|n| n.checked_mul(set.checked_ilog2().map_or(1, |n| n as usize + 2)))
        .ok_or_else(exhausted)?;
    b.spend(bound)
}
