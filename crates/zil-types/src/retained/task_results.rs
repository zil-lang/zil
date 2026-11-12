//! Untrusted result-scope proposal from actual owning Task return transfers.
//! Class ordinals are local to each Task: only an actual capture's complete
//! class map supplies a closure Fixed binder. No targets or capabilities arise.
use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Site {
    Value(ValueId),
    Local {
        block: usize,
        before: usize,
        local: LocalId,
    },
}
fn bad(span: Span, text: &str) -> Diagnostic {
    unsupported(span, text)
}
fn limit(text: &str, span: Span) -> Diagnostic {
    Diagnostic::limit(Phase::Types, text, Some(span))
}
fn pay(left: &mut usize, work: usize, span: Span) -> R<()> {
    *left = left
        .checked_sub(work)
        .ok_or_else(|| limit("Task result scope proposal budget exceeded", span))?;
    Ok(())
}

pub(super) fn populate(
    module: &TypedModule,
    definition: &ClosureDefinition,
    proposal: &mut StorageLifeContract,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let Type::Managed(result) = &definition.result else {
        return Ok(());
    };
    let Type::Task { retained_life, .. } = result.as_ref() else {
        return Ok(());
    };
    let function = module
        .functions
        .get(definition.function.0 as usize)
        .filter(|function| function.environment == Some(definition.id))
        .ok_or_else(|| bad(Span::default(), "Task result has no actual closure entry"))?;
    let span = function.span;
    if !function.parameters.is_empty() || definition.kind != CallableKind::FnOnce {
        return Err(bad(
            span,
            "Task result scope requires the actual consuming entry and complete parameter relation",
        ));
    }
    pay(left, function.blocks.len(), span)?;
    let mut instruction_count = 0usize;
    for block in &function.blocks {
        instruction_count = instruction_count.saturating_add(block.instructions.len());
    }
    pay(
        left,
        instruction_count
            .saturating_add(function.values.len())
            .saturating_add(function.blocks.len().saturating_mul(3)),
        span,
    )?;
    let mut definitions = vec![None; function.values.len()];
    for (block, body) in function.blocks.iter().enumerate() {
        for (index, instruction) in body.instructions.iter().enumerate() {
            if let Some(value) = instruction.result {
                let entry = definitions.get_mut(value.0 as usize).ok_or_else(|| {
                    bad(span, "Task result SSA definition exceeds its actual table")
                })?;
                if entry.replace((block, index)).is_some() {
                    return Err(bad(span, "Task result SSA definition is repeated"));
                }
            }
        }
    }
    let mut predecessors = vec![Vec::new(); function.blocks.len()];
    let mut reached = vec![false; function.blocks.len()];
    let mut pending = vec![function.entry.0 as usize];
    while let Some(block) = pending.pop() {
        pay(left, 1, span)?;
        let visited = reached
            .get_mut(block)
            .ok_or_else(|| bad(span, "Task result entry lies outside its actual CFG"))?;
        if std::mem::replace(visited, true) {
            continue;
        }
        let targets = match function.blocks[block].terminator {
            Terminator::Jump(to) => [Some(to), None],
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => [Some(then_block), Some(else_block)],
            _ => [None, None],
        };
        for target in targets.into_iter().flatten() {
            pay(left, 2, span)?;
            let next = target.0 as usize;
            predecessors
                .get_mut(next)
                .ok_or_else(|| bad(span, "Task result edge lies outside its actual CFG"))?
                .push(block);
            pending.push(next);
        }
    }
    let count = retained_life.binder_count as usize;
    if count > limits.max_nodes {
        return Err(limit("Task result class domain exceeds node limit", span));
    }
    pay(left, count.saturating_add(retained_life.ports.len()), span)?;
    let mut result_classes = Vec::with_capacity(count);
    for class in 0..retained_life.binder_count {
        let mut work = Vec::new();
        for (block, body) in function.blocks.iter().enumerate() {
            pay(left, 1, span)?;
            if reached[block]
                && let Terminator::Return(Some(value)) = body.terminator
            {
                work.push(Site::Value(value));
            }
        }
        let mut visited = BTreeSet::new();
        let mut actual = BTreeSet::new();
        while let Some(site) = work.pop() {
            pay(left, visited.len().saturating_add(1), span)?;
            if !visited.insert(site) {
                continue;
            }
            match site {
                Site::Value(value) => {
                    let ty = function
                        .values
                        .get(value.0 as usize)
                        .ok_or_else(|| bad(span, "returned Task SSA type is absent"))?;
                    inference::charge_type_snapshot(ty, span, limits, left, 2)?;
                    inference::charge_type_snapshot(&definition.result, span, limits, left, 2)?;
                    if ty != &definition.result {
                        return Err(bad(
                            span,
                            "returned Task class proposal changes its complete actual type",
                        ));
                    }
                    let (block, index) = definitions[value.0 as usize]
                        .ok_or_else(|| bad(span, "returned Task has no actual producer"))?;
                    match function.blocks[block].instructions[index].kind {
                        InstructionKind::TakeCapture { index: capture } => {
                            let captured = definition
                                .captures
                                .get(capture as usize)
                                .ok_or_else(|| bad(span, "returned Task capture is absent"))?;
                            inference::charge_type_snapshot(captured, span, limits, left, 2)?;
                            if captured != ty {
                                return Err(bad(
                                    span,
                                    "returned Task differs from its actual capture type",
                                ));
                            }
                            pay(left, proposal.classes.len().saturating_mul(3), span)?;
                            let mut rows = proposal.classes.iter().filter(|row| {
                                row.position.base == StorageBase::Capture(capture)
                                    && row.position.path == [LifeTypeProjection::Pointee]
                                    && row.position.class == class
                            });
                            let row = rows.next().ok_or_else(|| {
                                bad(
                                    span,
                                    "returned Task capture lacks a complete actual class map",
                                )
                            })?;
                            if rows.next().is_some()
                                || !matches!(row.binder,StorageBinder::Fixed(fixed) if fixed<proposal.fixed_binder_count)
                            {
                                return Err(bad(
                                    span,
                                    "returned Task capture class is not one actual Fixed binder",
                                ));
                            }
                            pay(left, actual.len().saturating_add(1), span)?;
                            actual.insert(row.binder);
                        }
                        InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => work
                            .push(Site::Local {
                                block,
                                before: index,
                                local,
                            }),
                        _ => {
                            return Err(bad(
                                span,
                                "returned Task requires its actual capture or whole local transfer source",
                            ));
                        }
                    }
                }
                Site::Local {
                    block,
                    before,
                    local,
                } => {
                    let mut found = false;
                    for instruction in function.blocks[block].instructions[..before].iter().rev() {
                        pay(left, 1, span)?;
                        match instruction.kind {
                            InstructionKind::Store {
                                local: written,
                                value,
                            } if written == local => {
                                work.push(Site::Value(value));
                                found = true;
                                break;
                            }
                            InstructionKind::StorageLive(written)
                            | InstructionKind::StorageDead(written)
                            | InstructionKind::TakeLocal { local: written }
                            | InstructionKind::DropManaged { local: written }
                                if written == local =>
                            {
                                return Err(bad(
                                    span,
                                    "returned Task local has no live preceding actual owner",
                                ));
                            }
                            _ => {}
                        }
                    }
                    if !found {
                        pay(left, predecessors[block].len(), span)?;
                        if predecessors[block].is_empty() {
                            return Err(bad(
                                span,
                                "returned Task local has no actual initialized incoming source",
                            ));
                        }
                        for previous in &predecessors[block] {
                            work.push(Site::Local {
                                block: *previous,
                                before: function.blocks[*previous].instructions.len(),
                                local,
                            });
                        }
                    }
                }
            }
        }
        if actual.len() != 1 {
            return Err(bad(
                span,
                "Task result class has no single complete actual return relation",
            ));
        }
        result_classes.push(*actual.first().unwrap());
    }
    pay(left, count.saturating_add(retained_life.ports.len()), span)?;
    for (class, binder) in result_classes.iter().enumerate() {
        proposal.classes.push(SignatureLifeClassBinding {
            position: SignatureLifeClass {
                base: StorageBase::Result,
                path: vec![LifeTypeProjection::Pointee],
                class: class as u32,
            },
            binder: *binder,
        });
    }
    for (port, row) in retained_life.ports.iter().enumerate() {
        let binder = *result_classes.get(row.binder as usize).ok_or_else(|| {
            bad(
                span,
                "Task result port class exceeds its complete actual domain",
            )
        })?;
        proposal.bindings.push(StorageLifeBinding {
            position: StoragePosition {
                base: StorageBase::Result,
                path: vec![ContractProjection::RetainedPort(port as u32)],
            },
            binder,
        });
    }
    Ok(())
}
