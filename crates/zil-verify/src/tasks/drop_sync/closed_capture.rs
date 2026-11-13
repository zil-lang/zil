//! Private, per-drop backwards obligations. A Ready edge is useful only for
//! its actual canonical close result and an unchanged receiver occupant.
//! No result of this pass is a type-level NoExternal or Transfer certificate.
use super::*;

type Point = (usize, usize);

struct Code<'a> {
    f: &'a TypedFunction,
    definitions: Vec<Option<Point>>,
    predecessors: Vec<Vec<usize>>,
    reachable: Vec<bool>,
}

impl<'a> Code<'a> {
    fn new(f: &'a TypedFunction, b: &mut Budget) -> Check<Self> {
        b.spend(
            f.values
                .len()
                .saturating_add(f.blocks.len().saturating_mul(3)),
        )?;
        let mut definitions = vec![None; f.values.len()];
        let mut predecessors = vec![Vec::new(); f.blocks.len()];
        for (block, row) in f.blocks.iter().enumerate() {
            for (position, instruction) in row.instructions.iter().enumerate() {
                b.spend(1)?;
                if let Some(value) = instruction.result {
                    let slot = definitions.get_mut(value.0 as usize).ok_or_else(|| {
                        reject(
                            "E-VERIFY-TASK",
                            Some(instruction.span),
                            "close proof result is outside the actual SSA arena",
                        )
                    })?;
                    if slot.replace((block, position)).is_some() {
                        return Err(reject(
                            "E-VERIFY-TASK",
                            Some(instruction.span),
                            "close proof SSA result repeats",
                        ));
                    }
                }
            }
            let targets = match row.terminator {
                Terminator::Jump(target) => [Some(target), None],
                Terminator::Branch {
                    then_block,
                    else_block,
                    ..
                } => [Some(then_block), Some(else_block)],
                _ => [None, None],
            };
            for target in targets.into_iter().flatten() {
                b.spend(1)?;
                predecessors
                    .get_mut(target.0 as usize)
                    .ok_or_else(|| {
                        reject(
                            "E-VERIFY-TASK",
                            Some(f.span),
                            "close proof CFG target is absent",
                        )
                    })?
                    .push(block);
            }
        }
        Ok(Self {
            f,
            definitions,
            predecessors,
            reachable: live_blocks(f, b)?,
        })
    }

    fn definition(&self, value: ValueId) -> Option<(Point, &'a Instruction)> {
        let point = *self.definitions.get(value.0 as usize)?.as_ref()?;
        Some((
            point,
            self.f.blocks.get(point.0)?.instructions.get(point.1)?,
        ))
    }
}

fn task_owner(ty: &Type) -> bool {
    matches!(ty, Type::Managed(payload) if matches!(payload.as_ref(), Type::Task { .. } | Type::TaskI32))
}

fn synchronous_other(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
    resources::charge_payload(ty, b)?;
    if stored_callables::scalar(ty) || matches!(ty, Type::Address { .. } | Type::Desc(_)) {
        return Ok(true);
    }
    let Type::Managed(payload) = ty else {
        return Ok(false);
    };
    if !stored_owned::payload(payload) {
        return Ok(false);
    }
    b.spend(module.resources.len())?;
    let mut actual = None;
    for resource in &module.resources {
        resources::charge_payload(&resource.payload, b)?;
        if resource.payload == **payload {
            if actual.replace(resource).is_some() {
                return Ok(false);
            }
        }
    }
    let Some(resource) = actual else {
        return Ok(false);
    };
    let Some(drop) = resource.teardown else {
        return Ok(true);
    };
    let Some(function) = module.functions.get(drop.0 as usize) else {
        return Ok(false);
    };
    b.spend(function.parameters.len().saturating_add(1))?;
    Ok(function.foreign.is_none()
        && function.environment.is_none()
        && function.call_kind == CallKind::Direct
        && function.return_type == Type::Unit
        && matches!(function.parameters.as_slice(), [parameter]
            if function.locals.get(parameter.0 as usize).is_some_and(|local| local.ty == Type::Desc(payload.clone()))))
}

fn inventory(
    module: &TypedModule,
    d: &ClosureDefinition,
    b: &mut Budget,
) -> Check<Option<Vec<u32>>> {
    let Some(entry) = module.functions.get(d.function.0 as usize) else {
        return Ok(None);
    };
    if entry.environment != Some(d.id)
        || entry.foreign.is_some()
        || entry.call_kind != CallKind::Direct
        || entry.return_type != d.result
    {
        return Ok(None);
    }
    b.spend(d.captures.len())?;
    let mut tasks = Vec::new();
    for (index, ty) in d.captures.iter().enumerate() {
        if task_owner(ty) {
            tasks.push(index as u32);
        } else if !synchronous_other(module, ty, b)? {
            return Ok(None);
        }
    }
    Ok(Some(tasks))
}

// The first closed-capture consumer deliberately requires stable task fields.
// Replacement, escaping mutable heads, and nested invocations need their own
// occupant summaries; none is inferred from matching closure/class numbers.
fn stable_entry(code: &Code<'_>, d: &ClosureDefinition, b: &mut Budget) -> Check<bool> {
    for (block, row) in code.f.blocks.iter().enumerate() {
        if !code.reachable[block] {
            continue;
        }
        for instruction in &row.instructions {
            b.spend(1)?;
            match &instruction.kind {
                InstructionKind::TakeCapture { index }
                | InstructionKind::WriteCapture { index, .. }
                    if d.captures.get(*index as usize).is_some_and(task_owner) =>
                {
                    return Ok(false);
                }
                InstructionKind::CallClosure { .. }
                | InstructionKind::CallClosureShared { .. }
                | InstructionKind::CallClosureMut { .. }
                | InstructionKind::CallClosureOnce { .. }
                | InstructionKind::CallClosureBorrowed { .. }
                | InstructionKind::WithForeignCallback { .. }
                | InstructionKind::WithScopedThread { .. }
                | InstructionKind::WithSyncCell { .. } => return Ok(false),
                InstructionKind::Call { .. }
                | InstructionKind::CallIndirect { .. }
                | InstructionKind::ForeignCall { .. }
                | InstructionKind::WriteAddress { .. } => return Ok(false),
                _ => {}
            }
        }
    }
    Ok(true)
}

fn capture_head(
    code: &Code<'_>,
    value: ValueId,
    b: &mut Budget,
    seen: &mut BTreeSet<ValueId>,
) -> Check<Option<u32>> {
    b.spend(seen.len().saturating_add(1))?;
    if !seen.insert(value) {
        return Ok(None);
    }
    let result = match code.definition(value).map(|(_, i)| &i.kind) {
        Some(InstructionKind::BorrowCapture {
            index,
            mutable: true,
        }) => Some(*index),
        Some(InstructionKind::ReborrowAddress { address, .. }) => {
            capture_head(code, *address, b, seen)?
        }
        Some(InstructionKind::Load(local)) => {
            let mut answer = None;
            let mut found = false;
            for (block, row) in code.f.blocks.iter().enumerate() {
                if !code.reachable[block] {
                    continue;
                }
                for instruction in &row.instructions {
                    b.spend(1)?;
                    if let InstructionKind::Store {
                        local: target,
                        value,
                    } = instruction.kind
                        && target == *local
                    {
                        let source = capture_head(code, value, b, seen)?;
                        if source.is_none() || found && source != answer {
                            seen.remove(&value);
                            return Ok(None);
                        }
                        answer = source;
                        found = true;
                    }
                }
            }
            answer
        }
        _ => None,
    };
    seen.remove(&value);
    Ok(result)
}

fn close_query(
    code: &Code<'_>,
    value: ValueId,
    b: &mut Budget,
    seen: &mut BTreeSet<ValueId>,
) -> Check<Option<u32>> {
    b.spend(seen.len().saturating_add(1))?;
    if !seen.insert(value) {
        return Ok(None);
    }
    let result = match code.definition(value).map(|(_, i)| &i.kind) {
        Some(InstructionKind::TaskPollClose { task }) => {
            if code.f.values.get(value.0 as usize)
                != Some(&super::super::task_poll_type(&Type::Unit))
            {
                None
            } else {
                capture_head(code, *task, b, &mut BTreeSet::new())?
            }
        }
        Some(InstructionKind::Load(local)) => {
            let mut answer = None;
            let mut found = false;
            for (block, row) in code.f.blocks.iter().enumerate() {
                if !code.reachable[block] {
                    continue;
                }
                for instruction in &row.instructions {
                    b.spend(1)?;
                    if let InstructionKind::Store {
                        local: target,
                        value: source,
                    } = instruction.kind
                        && target == *local
                    {
                        let source = close_query(code, source, b, seen)?;
                        if source.is_none() || found && source != answer {
                            seen.remove(&value);
                            return Ok(None);
                        }
                        answer = source;
                        found = true;
                    }
                }
            }
            answer
        }
        _ => None,
    };
    seen.remove(&value);
    Ok(result)
}

fn ready_test(
    code: &Code<'_>,
    condition: ValueId,
    b: &mut Budget,
) -> Check<Option<(ValueId, bool)>> {
    b.spend(5)?;
    let Some((_, instruction)) = code.definition(condition) else {
        return Ok(None);
    };
    let InstructionKind::Binary {
        op: BinaryOp::Equal,
        left,
        right,
    } = instruction.kind
    else {
        return Ok(None);
    };
    for (tag, number) in [(left, right), (right, left)] {
        if let Some((_, constant)) = code.definition(number)
            && let InstructionKind::Constant(Constant::I32(number @ (0 | 1))) = constant.kind
            && let Some((_, tag)) = code.definition(tag)
            && let InstructionKind::VariantTag { value } = tag.kind
        {
            return Ok(Some((value, number == 1)));
        }
    }
    Ok(None)
}

// Reachability in the graph with proved Ready edges cut. A reachable entry is
// a counterexample. Cycles supply no anchor: their incoming entry path is still
// visited, while a loop entered after a real close retains that induction base.
fn capture_closed(code: &Code<'_>, at: Point, capture: u32, b: &mut Budget) -> Check<bool> {
    let mut pending = vec![at.0];
    let mut seen = BTreeSet::new();
    while let Some(block) = pending.pop() {
        b.spend(seen.len().saturating_add(1))?;
        if !seen.insert(block) {
            continue;
        }
        if block == code.f.entry.0 as usize {
            return Ok(false);
        }
        for &predecessor in &code.predecessors[block] {
            b.spend(1)?;
            if !code.reachable[predecessor] {
                continue;
            }
            let mut anchor = false;
            if let Terminator::Branch {
                condition,
                then_block,
                else_block,
            } = code.f.blocks[predecessor].terminator
                && then_block != else_block
                && let Some((value, yes)) = ready_test(code, condition, b)?
                && (if yes { then_block } else { else_block }).0 as usize == block
            {
                anchor = close_query(code, value, b, &mut BTreeSet::new())? == Some(capture);
            }
            if !anchor {
                pending.push(predecessor);
            }
        }
    }
    Ok(true)
}

#[derive(Default)]
struct ExitProof {
    always: BTreeSet<u32>,
    ready: BTreeSet<u32>,
}

fn exits(module: &TypedModule, id: ClosureId, b: &mut Budget) -> Check<Option<ExitProof>> {
    let Some(d) = module.closures.get(id.0 as usize) else {
        return Ok(None);
    };
    let Some(tasks) = inventory(module, d, b)? else {
        return Ok(None);
    };
    let code = Code::new(&module.functions[d.function.0 as usize], b)?;
    if !stable_entry(&code, d, b)? {
        return Ok(None);
    }
    let mut proof = ExitProof::default();
    let mut returns = Vec::new();
    for (block, row) in code.f.blocks.iter().enumerate() {
        b.spend(1)?;
        if code.reachable[block]
            && let Terminator::Return(value) = row.terminator
        {
            returns.push(((block, row.instructions.len()), value));
        }
    }
    if returns.is_empty() {
        return Ok(None);
    }
    for capture in tasks {
        let mut all_closed = true;
        let mut ready_closed = d.result == super::super::task_poll_type(&Type::Unit);
        for &(at, value) in &returns {
            if capture_closed(&code, at, capture, b)? {
                continue;
            }
            all_closed = false;
            let conditional = if let Some(value) = value {
                close_query(&code, value, b, &mut BTreeSet::new())? == Some(capture)
                    || matches!(
                        code.definition(value).map(|(_, i)| &i.kind),
                        Some(InstructionKind::MakeVariant { variant: 0, .. })
                    )
            } else {
                false
            };
            ready_closed &= conditional;
        }
        if all_closed {
            proof.always.insert(capture);
        }
        if ready_closed {
            proof.ready.insert(capture);
        }
    }
    Ok(Some(proof))
}

// Until conditional result/occupant transport is installed, only an actual
// blocking invocation on this unchanged owning local discharges its leaf.
pub(super) fn allow_at(
    module: &TypedModule,
    f: &TypedFunction,
    local: LocalId,
    at: Point,
    b: &mut Budget,
) -> Check<bool> {
    let Some(Type::Closure(id)) = f.locals.get(local.0 as usize).map(|l| &l.ty) else {
        return Ok(false);
    };
    let Some(d) = module.closures.get(id.0 as usize) else {
        return Ok(false);
    };
    let Some(tasks) = inventory(module, d, b)? else {
        return Ok(false);
    };
    let Some(proof) = exits(module, *id, b)? else {
        return Ok(false);
    };
    if tasks.iter().any(|capture| !proof.always.contains(capture)) {
        return Ok(false);
    }
    let code = Code::new(f, b)?;
    let mut pending = vec![at];
    let mut seen = BTreeSet::new();
    while let Some((block, end)) = pending.pop() {
        b.spend(seen.len().saturating_add(1))?;
        if !seen.insert((block, end)) {
            continue;
        }
        let Some(row) = f.blocks.get(block) else {
            return Ok(false);
        };
        if end > row.instructions.len() {
            return Ok(false);
        }
        let mut anchored = false;
        for instruction in row.instructions[..end].iter().rev() {
            b.spend(1)?;
            match instruction.kind {
                InstructionKind::CallClosureMut { callee, .. } if callee == local => {
                    anchored = true;
                    break;
                }
                InstructionKind::Store { local: changed, .. }
                | InstructionKind::TakeLocal { local: changed }
                | InstructionKind::DropClosure { local: changed }
                | InstructionKind::StorageDead(changed)
                    if changed == local =>
                {
                    return Ok(false);
                }
                InstructionKind::Call { .. }
                | InstructionKind::CallIndirect { .. }
                | InstructionKind::CallClosure { .. }
                | InstructionKind::CallClosureShared { .. }
                | InstructionKind::CallClosureMut { .. }
                | InstructionKind::CallClosureOnce { .. }
                | InstructionKind::CallClosureBorrowed { .. }
                | InstructionKind::WriteAddress { .. }
                | InstructionKind::WithForeignCallback { .. }
                | InstructionKind::WithScopedThread { .. }
                | InstructionKind::WithSyncCell { .. } => return Ok(false),
                _ => {}
            }
        }
        if anchored {
            continue;
        }
        if block == f.entry.0 as usize {
            return Ok(false);
        }
        for &predecessor in &code.predecessors[block] {
            if code.reachable[predecessor] {
                pending.push((predecessor, f.blocks[predecessor].instructions.len()));
            }
        }
    }
    Ok(true)
}
