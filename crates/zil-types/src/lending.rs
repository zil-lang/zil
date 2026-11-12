//! Producer-side environment lending claims from the completed Typed CFG.
//! Claims stay untrusted. Verify reconstructs its own origins and loans.
use super::*;
use std::collections::BTreeSet;

pub(super) fn supported_result(ty: &Type) -> bool {
    matches!(ty, Type::Address { mutable: false, inner }
        if inner.is_copy_value()
            || matches!(inner.as_ref(), Type::Slice { element } if element.is_copy_value()))
}

pub(super) fn qualified_result(closure: &ClosureDefinition) -> bool {
    closure.kind != CallableKind::FnOnce
        && matches!(&closure.result, Type::Address { mutable:false, inner } if inner.is_b0_scalar())
        && closure.storage_life.as_ref().is_some_and(|life| {
            !life
                .bindings
                .iter()
                .any(|row| row.position.base == StorageBase::Result)
                && !life
                    .classes
                    .iter()
                    .any(|row| row.position.base == StorageBase::Result)
        })
}

fn owning_capture_address(ty: &Type) -> bool {
    matches!(ty, Type::Address { inner, .. }
        if matches!(inner.as_ref(), Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }))
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Source {
    Local(LocalId),
    Parameter(u32),
    Capture(u32),
}
type Sources = BTreeSet<Source>;
type State = BTreeMap<LocalId, Sources>;
struct Work(usize);
impl Work {
    fn spend(&mut self, count: usize, span: Span) -> R<()> {
        self.0 = self.0.checked_sub(count).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "environment lending source budget exceeded",
                Some(span),
            )
        })?;
        Ok(())
    }
    fn join(&mut self, target: &mut Sources, source: &Sources, span: Span) -> R<bool> {
        self.spend(
            source
                .len()
                .saturating_mul(target.len().checked_ilog2().map_or(1, |n| n as usize + 2))
                .saturating_add(1),
            span,
        )?;
        let before = target.len();
        target.extend(source);
        Ok(before != target.len())
    }
}
fn tracked(ty: &Type) -> bool {
    matches!(ty, Type::Address { .. } | Type::Desc(_))
}

impl Builder<'_, '_> {
    pub(super) fn lending_closure(&mut self, id: Option<ClosureId>) -> R<bool> {
        let Some(id) = id else {
            return Ok(false);
        };
        if !self
            .closures
            .thread_result(id)?
            .as_ref()
            .is_some_and(supported_result)
        {
            return Ok(false);
        }
        if self.closures.has_source_context(id) {
            return Ok(true);
        }
        let arity = self.closures.pending[id.0 as usize].parameters.len();
        for index in 0..arity {
            if matches!(
                self.closures.thread_parameter(id, index)?,
                Some(Type::Address { .. })
            ) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

fn sources(
    module: &TypedModule,
    f: &TypedFunction,
    summaries: &[Sources],
    work: &mut Work,
) -> R<Sources> {
    let span = f.span;
    work.spend(
        f.blocks
            .len()
            .saturating_mul(3)
            .saturating_add(f.values.len()),
        span,
    )?;
    let mut reachable = vec![false; f.blocks.len()];
    let mut predecessors = vec![Vec::new(); f.blocks.len()];
    let mut pending = vec![f.entry.0 as usize];
    while let Some(index) = pending.pop() {
        work.spend(1, span)?;
        if reachable[index] {
            continue;
        }
        reachable[index] = true;
        match f.blocks[index].terminator {
            Terminator::Jump(target) => {
                predecessors[target.0 as usize].push(index);
                pending.push(target.0 as usize);
            }
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => {
                predecessors[then_block.0 as usize].push(index);
                predecessors[else_block.0 as usize].push(index);
                pending.extend([then_block.0 as usize, else_block.0 as usize]);
            }
            _ => {}
        }
    }
    let mut values = vec![Sources::new(); f.values.len()];
    let mut outputs = vec![State::new(); f.blocks.len()];
    loop {
        let mut changed = false;
        for (b, block) in f.blocks.iter().enumerate() {
            work.spend(1, span)?;
            if !reachable[b] {
                continue;
            }
            let mut state = State::new();
            if b == f.entry.0 as usize {
                work.spend(f.parameters.len(), span)?;
                for (index, local) in f.parameters.iter().enumerate() {
                    if tracked(&f.locals[local.0 as usize].ty) {
                        state.insert(*local, Sources::from([Source::Parameter(index as u32)]));
                    }
                }
            } else {
                for predecessor in &predecessors[b] {
                    work.spend(1, span)?;
                    for (local, row) in &outputs[*predecessor] {
                        work.join(state.entry(*local).or_default(), row, span)?;
                    }
                }
            }
            for instruction in &block.instructions {
                work.spend(1, instruction.span)?;
                let mut found = Sources::new();
                if let Some(result) = instruction.result
                    && tracked(&f.values[result.0 as usize])
                {
                    if let Some((definition, arguments)) =
                        borrowed_fp::invocation_call(module, f, &instruction.kind, |n| {
                            work.spend(n, instruction.span)
                        })?
                    {
                        for source in &summaries[definition.function.0 as usize] {
                            work.spend(1, instruction.span)?;
                            if let Source::Parameter(index) = source {
                                let actual = arguments.get(*index as usize).ok_or_else(|| {
                                    error(
                                        "E-LENDING-ORIGIN",
                                        instruction.span,
                                        "callback actual argument absent",
                                    )
                                })?;
                                work.join(
                                    &mut found,
                                    &values[actual.0 as usize],
                                    instruction.span,
                                )?;
                            }
                        }
                    } else {
                        match &instruction.kind {
                            InstructionKind::BorrowLocal { local }
                            | InstructionKind::BorrowLocalMut { local }
                            | InstructionKind::DescribeLocal { local }
                            | InstructionKind::DescribePackageField { local, .. }
                            | InstructionKind::BorrowPackageField { local, .. }
                            | InstructionKind::BorrowOwnedArrayElement { local, .. } => {
                                found.insert(Source::Local(*local));
                            }
                            InstructionKind::BorrowCapture { index, .. }
                            | InstructionKind::DescribeCapture { index }
                            | InstructionKind::ReadCapture { index } => {
                                found.insert(Source::Capture(*index));
                            }
                            InstructionKind::BorrowPlaceAddress { address, .. }
                            | InstructionKind::ReborrowAddress { address, .. }
                            | InstructionKind::AddressField { address, .. }
                            | InstructionKind::AddressIndex { address, .. }
                            | InstructionKind::AddressSlice { address, .. }
                            | InstructionKind::ByteBufferBorrow {
                                buffer: address, ..
                            }
                            | InstructionKind::OwnedStoreBorrow { store: address, .. }
                            | InstructionKind::OwnedStoreBorrowMut { store: address, .. }
                            | InstructionKind::CallClosureBorrowed {
                                callee: address, ..
                            } => {
                                work.join(
                                    &mut found,
                                    &values[address.0 as usize],
                                    instruction.span,
                                )?;
                            }
                            InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                                if let Some(row) = state.get(local) {
                                    work.join(&mut found, row, instruction.span)?;
                                }
                            }
                            InstructionKind::CallClosureShared { callee, .. }
                            | InstructionKind::CallClosureMut { callee, .. } => {
                                found.insert(Source::Local(*callee));
                            }
                            InstructionKind::Call {
                                function,
                                arguments,
                            } => {
                                for source in &summaries[function.0 as usize] {
                                    work.spend(1, instruction.span)?;
                                    if let Source::Parameter(index) = source {
                                        work.join(
                                            &mut found,
                                            &values[arguments[*index as usize].0 as usize],
                                            instruction.span,
                                        )?;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    changed |=
                        work.join(&mut values[result.0 as usize], &found, instruction.span)?;
                }
                match instruction.kind {
                    InstructionKind::Store { local, value } => {
                        work.spend(
                            values[value.0 as usize].len().saturating_add(1),
                            instruction.span,
                        )?;
                        if values[value.0 as usize].is_empty() {
                            state.remove(&local);
                        } else {
                            state.insert(local, values[value.0 as usize].clone());
                        }
                    }
                    InstructionKind::StorageLive(local)
                    | InstructionKind::StorageDead(local)
                    | InstructionKind::TakeLocal { local }
                    | InstructionKind::DropClosure { local } => {
                        state.remove(&local);
                    }
                    _ => {}
                }
            }
            work.spend(state.len().saturating_add(outputs[b].len()), span)?;
            if state != outputs[b] {
                outputs[b] = state;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut returned = Sources::new();
    for (block, reachable) in f.blocks.iter().zip(reachable) {
        work.spend(1, span)?;
        if reachable && let Terminator::Return(Some(value)) = block.terminator {
            work.join(&mut returned, &values[value.0 as usize], span)?;
        }
    }
    Ok(returned)
}

/// Complete-module placement permits ordinary parameter-origin forwarding
/// without treating source names or source annotations as provenance shortcuts.
pub(super) fn populate(module: &mut TypedModule, remaining: &mut usize) -> R<()> {
    let mut work = Work(*remaining);
    let result = populate_inner(module, &mut work);
    *remaining = work.0;
    result
}
fn populate_inner(module: &mut TypedModule, work: &mut Work) -> R<()> {
    work.spend(module.closures.len(), Span::default())?;
    if !module
        .closures
        .iter()
        .any(|c| matches!(c.result, Type::Address { .. }))
    {
        return Ok(());
    }
    work.spend(module.functions.len(), Span::default())?;
    let mut summaries = vec![Sources::new(); module.functions.len()];
    loop {
        let mut changed = false;
        for f in &module.functions {
            work.spend(1, f.span)?;
            if matches!(f.return_type, Type::Address { .. }) {
                let found = sources(module, f, &summaries, work)?;
                changed |= work.join(&mut summaries[f.id.0 as usize], &found, f.span)?;
            }
        }
        if !changed {
            break;
        }
    }
    for closure in &mut module.closures {
        let f = &module.functions[closure.function.0 as usize];
        work.spend(closure.captures.len().saturating_add(1), f.span)?;
        if !matches!(closure.result, Type::Address { .. }) {
            continue;
        }
        work.spend(closure.parameters.len(), f.span)?;
        if borrowed_fp::invocation(&closure.parameters, &closure.result)
            && !qualified_result(closure)
        {
            let mutable = matches!(closure.result, Type::Address { mutable: true, .. });
            let roots = &summaries[closure.function.0 as usize];
            work.spend(roots.len(), f.span)?;
            if roots.is_empty()
                || roots.iter().any(|root| {
                    let Source::Parameter(index) = root else {
                        return true;
                    };
                    !closure
                        .parameters
                        .get(*index as usize)
                        .is_some_and(|p| matches!(p,Type::Address{mutable:m,..} if !mutable||*m))
                })
            {
                return Err(error(
                    "E-LENDING-ORIGIN",
                    f.span,
                    "invocation callback result must borrow current actual argument storage",
                ));
            }
            continue;
        }
        if !supported_result(&closure.result) || closure.kind == CallableKind::FnOnce {
            return Err(unsupported(
                f.span,
                "environment lending requires a shared result and a non-consuming Fn/FnMut entry",
            ));
        }
        let external_capture = closure.captures.iter().any(|ty|matches!(ty,Type::Address{..}))
            && closure.parameters.iter().all(Type::is_copy_value);
        if closure.captures.iter().any(|ty| {
            matches!(ty, Type::Desc(_))
                || matches!(ty, Type::Address { .. })
                    && !owning_capture_address(ty)
                    && !qualified_result(closure)
        }) && !external_capture {
            return Err(unsupported(
                f.span,
                "environment lending requires self-contained captures, not external Address/Desc targets",
            ));
        }
        let mut indices = Vec::new();
        for source in &summaries[closure.function.0 as usize] {
            work.spend(1, f.span)?;
            let Source::Capture(index) = source else {
                return Err(error(
                    "E-LENDING-ORIGIN",
                    f.span,
                    "closure result must borrow actual environment storage, not an activation local or explicit argument",
                ));
            };
            if !closure.captures.get(*index as usize).is_some_and(|ty| {
                ty.is_copy_value()
                    || owning_capture_address(ty)
                    || external_capture && matches!(ty,Type::Address{..})
                    || matches!(ty, Type::Managed(inner) if **inner == Type::ByteBuffer)
            }) {
                return Err(unsupported(
                    f.span,
                    "lending source requires an inline Copy field or an owned ByteBuffer field",
                ));
            }
            indices.push(*index);
        }
        if indices.is_empty() {
            return Err(error(
                "E-LENDING-ORIGIN",
                f.span,
                "closure result has no grounded environment source",
            ));
        }
        closure.lending_captures = indices;
    }
    for f in &mut module.functions {
        work.spend(1, f.span)?;
        if let Some(contract) = &mut f.callable_return_contract {
            let indices = &module.closures[contract.closure.0 as usize].lending_captures;
            work.spend(indices.len(), f.span)?;
            contract.lending_captures.clone_from(indices);
        }
    }
    Ok(())
}
