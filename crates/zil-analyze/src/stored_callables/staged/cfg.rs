//! Paid forward provenance, independent of resource proofs.
//! DAGs retain their original schedule; cycles use monotone input-state worklists.
use super::*;
use zil_ir::{BasicBlock, ClosureId, ValueId};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Origin {
    Empty,
    Input(usize),
    Mixed,
}
impl Origin {
    fn join(self, other: Self) -> Self {
        if self == other { self } else { Self::Mixed }
    }
}
#[derive(Clone, PartialEq, Eq)]
struct Frame {
    locals: Vec<Origin>,
    values: Vec<Origin>,
    activated: Vec<u8>,
    retired: Vec<bool>,
    done: (u8, Vec<u8>),
}
impl Frame {
    fn merge(&mut self, other: &Self, b: &mut Budget) -> CompileResult<bool> {
        b.spend(
            self.locals
                .len()
                .saturating_mul(3)
                .saturating_add(self.values.len())
                .saturating_add(self.done.1.len())
                .saturating_add(1),
            "owning CFG frame merge",
        )?;
        let mut changed = false;
        for i in 0..self.locals.len() {
            changed |= self.locals[i] != self.locals[i].join(other.locals[i])
                || self.activated[i] != (self.activated[i] | other.activated[i])
                || self.retired[i] != (self.retired[i] | other.retired[i]);
            self.locals[i] = self.locals[i].join(other.locals[i]);
            self.activated[i] |= other.activated[i];
            self.retired[i] |= other.retired[i];
        }
        for (left, right) in self.values.iter_mut().zip(&other.values) {
            changed |= *left != left.join(*right);
            *left = left.join(*right);
        }
        changed |= self.done.0 != (self.done.0 | other.done.0);
        self.done.0 |= other.done.0;
        for (left, right) in self.done.1.iter_mut().zip(&other.done.1) {
            changed |= *left != (*left | *right);
            *left |= *right;
        }
        Ok(changed)
    }
    fn copy(&self, b: &mut Budget) -> CompileResult<Self> {
        b.spend(
            self.locals
                .len()
                .saturating_mul(3)
                .saturating_add(self.values.len())
                .saturating_add(self.done.1.len())
                .saturating_add(1),
            "owning CFG frame clone",
        )?;
        Ok(self.clone())
    }
}
fn edges(block: &BasicBlock) -> CompileResult<[Option<usize>; 2]> {
    Ok(match block.terminator {
        Terminator::Jump(id) => [Some(id.0 as usize), None],
        Terminator::Branch {
            then_block,
            else_block,
            ..
        } => [Some(then_block.0 as usize), Some(else_block.0 as usize)],
        Terminator::Return(_) | Terminator::Trap => [None, None],
        _ => return Err(closed()),
    })
}
fn closure_value(f: &TypedFunction, v: ValueId) -> bool {
    matches!(f.values.get(v.0 as usize), Some(Type::Closure(_)))
}
fn closure_local(f: &TypedFunction, l: zil_ir::LocalId) -> bool {
    matches!(
        f.locals.get(l.0 as usize).map(|l| &l.ty),
        Some(Type::Closure(_))
    )
}
fn scalar_operands(kind: &InstructionKind) -> [Option<ValueId>; 2] {
    match kind {
        InstructionKind::Unary { operand, .. } => [Some(*operand), None],
        InstructionKind::Binary { left, right, .. } => [Some(*left), Some(*right)],
        InstructionKind::ConvertInteger { value, .. }
        | InstructionKind::ConvertNumber { value, .. } => [Some(*value), None],
        InstructionKind::ReborrowAddress { address, .. } => [Some(*address), None],
        _ => [None, None],
    }
}

fn debt(roles: &[zil_ir::CallableRole], parameter: usize, b: &mut Budget) -> CompileResult<usize> {
    if roles.len() > 1 {
        b.spend(roles.len(), "actual owning role debt selector")?;
    }
    let index = roles
        .iter()
        .position(|role| role.parameter as usize == parameter)
        .ok_or_else(closed)?;
    Ok(index)
}

pub(super) fn parameter(
    f: &TypedFunction,
    call: &InstructionKind,
    id: ClosureId,
    b: &mut Budget,
) -> CompileResult<usize> {
    match solve(f, call, id, b, false) {
        Err(errors)
            if errors
                .iter()
                .all(|d| d.category != zil_base::ResultCategory::ResourceLimit) =>
        {
            solve(f, call, id, b, true)
        }
        result => result,
    }
}
fn solve(
    f: &TypedFunction,
    call: &InstructionKind,
    id: ClosureId,
    b: &mut Budget,
    cyclic: bool,
) -> CompileResult<usize> {
    let n = f.blocks.len();
    b.spend(
        n.saturating_mul(8)
            .saturating_add(f.locals.len())
            .saturating_add(f.values.len())
            .saturating_add(f.parameters.len()),
        "owning CFG graph and origin inventory",
    )?;
    let roles = &f.callable_contract.as_ref().ok_or_else(closed)?.roles;
    if roles.is_empty() {
        return Err(closed());
    }
    if roles.len() > 1 {
        b.spend(
            roles
                .len()
                .saturating_add(f.parameters.len().saturating_mul(roles.len())),
            "owning role inventory",
        )?;
    }
    for role in roles {
        let local = f
            .parameters
            .get(role.parameter as usize)
            .ok_or_else(closed)?;
        let ty = &f.locals.get(local.0 as usize).ok_or_else(closed)?.ty;
        match (role.kind, role.cleanup, ty) {
            (zil_ir::CallableKind::FnOnce, zil_ir::CallableCleanup::Owned, Type::Closure(id))
                if *id == role.closure => {}
            (
                zil_ir::CallableKind::Fn | zil_ir::CallableKind::FnMut,
                zil_ir::CallableCleanup::Borrowed,
                Type::Address { mutable, inner },
            ) if *mutable == (role.kind == zil_ir::CallableKind::FnMut)
                && inner.as_ref() == &Type::Closure(role.closure) => {}
            _ => return Err(closed()),
        }
    }
    if roles.len() == 1 && roles[0].closure != id {
        return Err(closed());
    }
    let mut inputs = 0usize;
    for (index, local) in f.parameters.iter().enumerate() {
        if closure_local(f, *local) {
            inputs += 1;
            if !roles.iter().any(|role| role.parameter as usize == index) {
                return Err(closed());
            }
        }
    }
    if inputs
        != roles
            .iter()
            .filter(|role| role.cleanup == zil_ir::CallableCleanup::Owned)
            .count()
        || n == 0
        || f.entry.0 as usize >= n
    {
        return Err(closed());
    }
    let mut incoming = vec![0usize; n];
    let mut ready = Vec::with_capacity(n);
    let mut states: Vec<Option<Frame>> = vec![None; n];
    let mut defined = vec![false; f.values.len()];
    for (index, block) in f.blocks.iter().enumerate() {
        if block.id.0 as usize != index {
            return Err(closed());
        }
        for next in edges(block)?.into_iter().flatten() {
            if next >= n {
                return Err(closed());
            }
            incoming[next] = incoming[next].saturating_add(1);
        }
        b.spend(
            block.instructions.len(),
            "owning CFG SSA definition inventory",
        )?;
        for i in &block.instructions {
            if let Some(value) = i.result {
                let def = defined.get_mut(value.0 as usize).ok_or_else(closed)?;
                if *def {
                    return Err(closed());
                }
                *def = true;
            }
        }
    }
    if cyclic {
        // This fallback extends actual cycles only. A rejected DAG does not
        // gain a different local-activation discipline by retrying it.
        b.spend(n.saturating_mul(3), "cyclic graph classification tables")?;
        let mut degrees = incoming.clone();
        let mut frontier = Vec::with_capacity(n);
        for (index, count) in degrees.iter().enumerate() {
            if *count == 0 {
                frontier.push(index);
            }
        }
        let mut removed = 0usize;
        while let Some(index) = frontier.pop() {
            b.spend(3, "cyclic graph classification edges")?;
            removed += 1;
            for next in edges(&f.blocks[index])?.into_iter().flatten() {
                degrees[next] -= 1;
                if degrees[next] == 0 {
                    frontier.push(next);
                }
            }
        }
        if removed == n {
            return Err(closed());
        }
    }
    let entry = f.entry.0 as usize;
    if !cyclic && incoming[entry] != 0 {
        return Err(closed());
    }
    b.spend(
        f.locals
            .len()
            .saturating_mul(3)
            .saturating_add(f.values.len()),
        "owning CFG initial frame",
    )?;
    if roles.len() > 1 {
        b.spend(roles.len(), "dynamic owning debt allocation")?;
    }
    let mut initial = Frame {
        locals: vec![Origin::Empty; f.locals.len()],
        values: vec![Origin::Empty; f.values.len()],
        activated: vec![1; f.locals.len()],
        retired: vec![false; f.locals.len()],
        done: (1, vec![1; roles.len() - 1]),
    };
    if roles.len() > 1 {
        b.spend(roles.len(), "actual owning role seeds")?;
    }
    for (index, role) in roles.iter().enumerate() {
        if role.cleanup == zil_ir::CallableCleanup::Borrowed {
            if index == 0 {
                initial.done.0 = 2;
            } else {
                initial.done.1[index - 1] = 2;
            }
        } else {
            let p = role.parameter as usize;
            let local = f.parameters[p].0 as usize;
            initial.locals[local] = Origin::Input(p);
            initial.activated[local] = 2;
        }
    }
    states[entry] = Some(initial);
    let mut seen = if cyclic {
        b.spend(
            n.saturating_mul(2),
            "cyclic owning CFG block activation/queue inventory",
        )?;
        vec![false; n]
    } else {
        Vec::new()
    };
    let mut queued = if cyclic { vec![false; n] } else { Vec::new() };
    if cyclic {
        ready.push(entry);
        queued[entry] = true;
    } else {
        for (index, count) in incoming.iter().enumerate() {
            if *count == 0 {
                ready.push(index);
            }
        }
    }
    let mut visited = 0usize;
    let mut answer = None;
    while let Some(index) = ready.pop() {
        b.spend(3, "owning CFG actual block visit and edges")?;
        if !cyclic || !seen[index] {
            visited += 1;
        }
        if cyclic {
            seen[index] = true;
            queued[index] = false;
        }
        let block = &f.blocks[index];
        let mut frame = if cyclic {
            states[index].as_ref().ok_or_else(closed)?.copy(b)?
        } else {
            states[index].take().ok_or_else(closed)?
        };
        b.spend(block.instructions.len(), "owning CFG linear transfers")?;
        for i in &block.instructions {
            let result = i.result;
            if result.is_some_and(|v| closure_value(f, v))
                && !matches!(i.kind, InstructionKind::TakeLocal { .. })
            {
                return Err(closed());
            }
            match &i.kind {
                InstructionKind::TakeLocal { local } if closure_local(f, *local) => {
                    let l = local.0 as usize;
                    let origin = frame.locals[l];
                    let Origin::Input(actual) = origin else {
                        return Err(closed());
                    };
                    let mask = debt(roles, actual, b)?;
                    if frame.retired[l]
                        || frame.activated[l] != 2
                        || (if mask == 0 {
                            frame.done.0
                        } else {
                            frame.done.1[mask - 1]
                        }) != 1
                    {
                        return Err(closed());
                    }
                    let value = result
                        .filter(|v| f.values.get(v.0 as usize) == Some(&f.locals[l].ty))
                        .ok_or_else(closed)?;
                    frame.values[value.0 as usize] = origin;
                    frame.locals[l] = Origin::Empty;
                }
                InstructionKind::Store { local, value } if closure_local(f, *local) => {
                    let l = local.0 as usize;
                    let v = value.0 as usize;
                    let Some(Origin::Input(actual)) = frame.values.get(v) else {
                        return Err(closed());
                    };
                    let mask = debt(roles, *actual, b)?;
                    if frame.retired[l]
                        || frame.activated[l] != 2
                        || frame.locals[l] != Origin::Empty
                        || (if mask == 0 {
                            frame.done.0
                        } else {
                            frame.done.1[mask - 1]
                        }) != 1
                        || f.values.get(v) != Some(&f.locals[l].ty)
                        || roles
                            .iter()
                            .any(|role| f.parameters[role.parameter as usize] == *local)
                    {
                        return Err(closed());
                    }
                    frame.locals[l] = frame.values[v];
                    frame.values[v] = Origin::Empty;
                }
                InstructionKind::CallClosureOnce { callee, arguments } => {
                    b.spend(arguments.len(), "owning CFG callback operands")?;
                    if arguments.iter().any(|v| closure_value(f, *v)) {
                        return Err(closed());
                    }
                    let value = frame.values.get_mut(callee.0 as usize).ok_or_else(closed)?;
                    let Origin::Input(actual) = *value else {
                        return Err(closed());
                    };
                    let mask = debt(roles, actual, b)?;
                    if (if mask == 0 {
                        frame.done.0
                    } else {
                        frame.done.1[mask - 1]
                    }) != 1
                    {
                        return Err(closed());
                    }
                    if std::ptr::eq(&i.kind, call) || i.kind == *call {
                        if f.values.get(callee.0 as usize) != Some(&Type::Closure(id))
                            || answer.replace(actual).is_some_and(|old| old != actual)
                        {
                            return Err(closed());
                        }
                    }
                    *value = Origin::Empty;
                    if mask == 0 {
                        frame.done.0 = 2;
                    } else {
                        frame.done.1[mask - 1] = 2;
                    }
                }
                InstructionKind::DropClosure { local } if closure_local(f, *local) => {
                    let l = local.0 as usize;
                    match frame.locals[l] {
                        Origin::Input(actual) => {
                            let mask = debt(roles, actual, b)?;
                            if (if mask == 0 {
                                frame.done.0
                            } else {
                                frame.done.1[mask - 1]
                            }) != 1
                            {
                                return Err(closed());
                            }
                            if mask == 0 {
                                frame.done.0 = 2;
                            } else {
                                frame.done.1[mask - 1] = 2;
                            }
                            frame.locals[l] = Origin::Empty;
                        }
                        Origin::Empty => {}
                        _ => return Err(closed()),
                    }
                    frame.retired[l] = true;
                    if cyclic {
                        frame.activated[l] = 1;
                    }
                }
                InstructionKind::StorageDead(local) if closure_local(f, *local) => {
                    let l = local.0 as usize;
                    if frame.locals[l] != Origin::Empty {
                        return Err(closed());
                    }
                    frame.retired[l] = true;
                    if cyclic {
                        frame.activated[l] = 1;
                    }
                }
                InstructionKind::StorageLive(local) if closure_local(f, *local) => {
                    let l = local.0 as usize;
                    if frame.activated[l] != 1
                        || (frame.retired[l] && !cyclic)
                        || (cyclic
                            && (frame.locals[l] != Origin::Empty
                                || roles
                                    .iter()
                                    .any(|role| f.parameters[role.parameter as usize] == *local)))
                    {
                        return Err(closed());
                    }
                    frame.activated[l] = 2;
                    if cyclic {
                        frame.retired[l] = false;
                    }
                }
                InstructionKind::Load(local) | InstructionKind::TakeLocal { local }
                    if closure_local(f, *local) =>
                {
                    return Err(closed());
                }
                InstructionKind::Store { value, .. } if closure_value(f, *value) => {
                    return Err(closed());
                }
                InstructionKind::ScopeEnter { .. }
                | InstructionKind::ScopeExit { .. }
                | InstructionKind::StorageLive(_)
                | InstructionKind::StorageDead(_)
                | InstructionKind::Constant(_)
                | InstructionKind::Load(_)
                | InstructionKind::TakeLocal { .. }
                | InstructionKind::Store { .. }
                | InstructionKind::ReborrowAddress { .. }
                | InstructionKind::Unary { .. }
                | InstructionKind::Binary { .. }
                | InstructionKind::ConvertInteger { .. }
                | InstructionKind::ConvertNumber { .. }
                | InstructionKind::CallClosureBorrowed { .. } => {
                    if let InstructionKind::CallClosureBorrowed {
                        callee, arguments, ..
                    } = &i.kind
                    {
                        b.spend(
                            arguments.len().saturating_add(1),
                            "borrowed callback operands in owning CFG",
                        )?;
                        if closure_value(f, *callee)
                            || arguments.iter().any(|v| closure_value(f, *v))
                        {
                            return Err(closed());
                        }
                    }
                    if scalar_operands(&i.kind)
                        .into_iter()
                        .flatten()
                        .any(|v| closure_value(f, v))
                    {
                        return Err(closed());
                    }
                }
                _ => return Err(closed()),
            }
        }
        let terminal = match block.terminator {
            Terminator::Return(v) => {
                if v.is_some_and(|v| closure_value(f, v)) {
                    return Err(closed());
                }
                true
            }
            Terminator::Branch { condition, .. } => {
                if closure_value(f, condition) {
                    return Err(closed());
                }
                false
            }
            _ => false,
        };
        if terminal {
            b.spend(
                frame
                    .locals
                    .len()
                    .saturating_add(frame.values.len())
                    .saturating_add(roles.len().saturating_sub(1)),
                "owning CFG return debt inventory",
            )?;
            if frame.done.0 != 2
                || frame.done.1.iter().any(|state| *state != 2)
                || frame
                    .locals
                    .iter()
                    .chain(&frame.values)
                    .any(|o| *o != Origin::Empty)
            {
                return Err(closed());
            }
        }
        for next in edges(block)?.into_iter().flatten() {
            let changed = if let Some(previous) = &mut states[next] {
                previous.merge(&frame, b)?
            } else {
                states[next] = Some(frame.copy(b)?);
                true
            };
            if cyclic {
                if changed && !queued[next] {
                    ready.push(next);
                    queued[next] = true;
                }
            } else {
                incoming[next] -= 1;
                if incoming[next] == 0 {
                    ready.push(next);
                }
            }
        }
    }
    if visited != n {
        return Err(closed());
    }
    answer.ok_or_else(closed)
}
