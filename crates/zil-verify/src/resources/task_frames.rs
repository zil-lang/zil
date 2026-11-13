//! Immutable continuation responsibility coverage, produced inside the same
//! transfer that checks ordinary ownership. No transported plan can supply it.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskOwnerSlot {
    Local(LocalId),
    LocalShell(LocalId),
    LocalField { local: LocalId, index: u32 },
    Value(ValueId),
}
#[derive(Clone, Debug)]
pub struct TaskOwner {
    slot: TaskOwnerSlot,
    ty: Type,
    debt: usize,
    cleanup_anchor: Option<usize>,
}
impl TaskOwner {
    pub fn slot(&self) -> TaskOwnerSlot {
        self.slot
    }
    pub fn ty(&self) -> &Type {
        &self.ty
    }
    pub fn cleanup_anchor(&self) -> Option<usize> {
        self.cleanup_anchor
    }
}
#[derive(Clone, Debug)]
pub struct TaskDebtBoundary {
    block: BlockId,
    instruction: usize,
    may_live: Vec<usize>,
}
impl TaskDebtBoundary {
    pub fn block(&self) -> BlockId {
        self.block
    }
    pub fn instruction(&self) -> usize {
        self.instruction
    }
    pub fn may_live(&self) -> &[usize] {
        &self.may_live
    }
}
#[derive(Clone, Debug)]
pub struct TaskOwnership {
    owners: Vec<TaskOwner>,
    entry: Vec<usize>,
    boundaries: Vec<TaskDebtBoundary>,
    actions: std::collections::BTreeMap<(BlockId, usize), TaskOwnerActions>,
    result_type: Option<Type>,
}
#[derive(Clone, Copy, Debug)]
pub enum TaskOwnerAction {
    Link(usize),
    LinkField { slot: usize, after: usize },
    Unlink(usize),
}
#[derive(Clone, Debug)]
pub struct TaskOwnerActions {
    block: BlockId,
    instruction: usize,
    before: Vec<TaskOwnerAction>,
    after: Vec<TaskOwnerAction>,
    // One affine environment root survives body return until its actual
    // remaining capture flags have completed asynchronous cleanup.
    invocation_owner: Option<usize>,
    after_invoke: Vec<TaskOwnerAction>,
}
impl TaskOwnerActions {
    pub fn block(&self) -> BlockId {
        self.block
    }
    pub fn instruction(&self) -> usize {
        self.instruction
    }
    pub fn before(&self) -> &[TaskOwnerAction] {
        &self.before
    }
    pub fn after(&self) -> &[TaskOwnerAction] {
        &self.after
    }
    pub fn invocation_owner(&self) -> Option<usize> {
        self.invocation_owner
    }
    pub fn after_invoke(&self) -> &[TaskOwnerAction] {
        &self.after_invoke
    }
}
impl TaskOwnership {
    pub fn result_type(&self) -> Option<&Type> {
        self.result_type.as_ref()
    }
    pub fn owner_for_value(&self, value: ValueId) -> Option<usize> {
        self.owners
            .iter()
            .position(|owner| owner.slot == TaskOwnerSlot::Value(value))
    }
    pub fn owners(&self) -> &[TaskOwner] {
        &self.owners
    }
    pub fn entry(&self) -> &[usize] {
        &self.entry
    }
    pub fn boundaries(&self) -> &[TaskDebtBoundary] {
        &self.boundaries
    }
    pub fn actions_at(&self, block: BlockId, instruction: usize) -> Option<&TaskOwnerActions> {
        self.actions.get(&(block, instruction))
    }
    pub fn action_count(&self) -> usize {
        self.actions.len()
    }
    pub fn actions(&self) -> impl Iterator<Item = &TaskOwnerActions> {
        self.actions.values()
    }
}

pub(super) fn start(
    f: &TypedFunction,
    slots: &Slots,
    initial: &State,
    budget: &mut Budget,
) -> Check<()> {
    // Existing scalar tasks have no resource slots and retain their exact old
    // cost. Ordinary functions never create continuation metadata.
    if slots.debt_count == 0 || budget.task_workers.is_empty() {
        return Ok(());
    }
    budget.spend(budget.task_workers.len().saturating_add(1))?;
    if !budget.task_workers.contains(&f.id) {
        return Ok(());
    }
    budget.spend(
        slots
            .debt_count
            .saturating_mul(4)
            .saturating_add(f.locals.len())
            .saturating_add(f.values.len()),
    )?;
    let mut owners = vec![];
    for (index, local) in f.locals.iter().enumerate() {
        if let Some(debt) = slots.managed_local[index] {
            charge_payload(&local.ty, budget)?;
            if let Type::Package { fields, .. } = &local.ty {
                let shell = owners.len();
                owners.push(TaskOwner {
                    slot: TaskOwnerSlot::LocalShell(local.id),
                    ty: Type::Unit,
                    debt,
                    cleanup_anchor: None,
                });
                for (index, ((_, ty), field)) in
                    fields.iter().zip(slots.fields(local.id)).enumerate()
                {
                    if let Some(debt) = field {
                        charge_payload(ty, budget)?;
                        owners.push(TaskOwner {
                            slot: TaskOwnerSlot::LocalField {
                                local: local.id,
                                index: index as u32,
                            },
                            ty: ty.clone(),
                            debt: *debt,
                            cleanup_anchor: Some(shell),
                        });
                    }
                }
            } else if matches!(
                local.ty,
                Type::Managed(_)
                    | Type::Tuple(_)
                    | Type::Enum { .. }
                    | Type::ErrorUnion { .. }
                    | Type::Closure(_)
            ) {
                owners.push(TaskOwner {
                    slot: TaskOwnerSlot::Local(local.id),
                    ty: local.ty.clone(),
                    debt,
                    cleanup_anchor: None,
                });
            } else {
                return Err(unsupported(
                    "continuation aggregate debt requires canonical partial-owner frame mapping",
                    Some(local.span),
                ));
            }
        }
    }
    for (index, ty) in f.values.iter().enumerate() {
        if let Some(debt) = slots.managed_value[index] {
            charge_payload(ty, budget)?;
            if !matches!(
                ty,
                Type::Managed(_)
                    | Type::Package { .. }
                    | Type::Tuple(_)
                    | Type::Enum { .. }
                    | Type::ErrorUnion { .. }
                    | Type::Closure(_)
            ) {
                return Err(unsupported(
                    "continuation aggregate SSA debt requires canonical partial-owner frame mapping",
                    Some(f.span),
                ));
            }
            owners.push(TaskOwner {
                slot: TaskOwnerSlot::Value(ValueId(index as u32)),
                ty: ty.clone(),
                debt,
                cleanup_anchor: None,
            });
        }
    }
    let mut covered = vec![false; slots.debt_count];
    for owner in &owners {
        if std::mem::replace(&mut covered[owner.debt], true) {
            return Err(reject(
                "E-VERIFY-TASK",
                Some(f.span),
                "continuation responsibility has more than one canonical owner slot",
            ));
        }
    }
    // This is a conservation test, not an inference that all typed locals are
    // live. Captures/partial fields need their own canonical mapping first.
    if covered.iter().any(|covered| !covered) {
        return Err(unsupported(
            "continuation has an unmapped capture or partial resource obligation",
            Some(f.span),
        ));
    }
    let mut entry = vec![];
    budget.spend(
        f.parameters
            .len()
            .saturating_mul(owners.len().saturating_add(1)),
    )?;
    // Actual input initialization order, rather than LocalId sort order.
    for parameter in &f.parameters {
        for (index, owner) in owners.iter().enumerate() {
            let local = match owner.slot {
                TaskOwnerSlot::Local(local)
                | TaskOwnerSlot::LocalShell(local)
                | TaskOwnerSlot::LocalField { local, .. } => Some(local),
                _ => None,
            };
            if local == Some(*parameter) && initial.debt[owner.debt] & LIVE != 0 {
                entry.push(index);
            }
        }
    }
    budget.spend(
        budget
            .task_ownership
            .len()
            .saturating_mul(4)
            .saturating_add(1),
    )?;
    let result_type = if !f.return_type.is_copy_value() {
        charge_payload(&f.return_type, budget)?;
        Some(f.return_type.clone())
    } else {
        None
    };
    budget.task_ownership.insert(
        f.id,
        TaskOwnership {
            owners,
            entry,
            boundaries: vec![],
            actions: std::collections::BTreeMap::new(),
            result_type,
        },
    );
    Ok(())
}

pub(super) fn capture(
    f: &TypedFunction,
    block: usize,
    instruction: usize,
    state: &State,
    budget: &mut Budget,
) -> Check<()> {
    if budget.task_ownership.is_empty() {
        return Ok(());
    }
    budget.spend(
        budget
            .task_ownership
            .len()
            .saturating_mul(4)
            .saturating_add(1),
    )?;
    let Some(plan) = budget.task_ownership.get(&f.id) else {
        return Ok(());
    };
    let count = plan.owners.len();
    budget.spend(count.saturating_add(1))?;
    let plan = budget
        .task_ownership
        .get_mut(&f.id)
        .expect("same checked plan");
    let mut may_live = vec![];
    for (index, owner) in plan.owners.iter().enumerate() {
        if state.debt[owner.debt] & LIVE != 0 {
            may_live.push(index);
        }
    }
    plan.boundaries.push(TaskDebtBoundary {
        block: BlockId(block as u32),
        instruction,
        may_live,
    });
    Ok(())
}

pub(super) fn begin(
    f: &TypedFunction,
    state: &State,
    budget: &mut Budget,
) -> Check<Option<Vec<u8>>> {
    if budget.task_ownership.is_empty() {
        return Ok(None);
    }
    budget.spend(
        budget
            .task_ownership
            .len()
            .saturating_mul(4)
            .saturating_add(1),
    )?;
    let Some(plan) = budget.task_ownership.get(&f.id) else {
        return Ok(None);
    };
    let count = plan.owners.len();
    budget.spend(count.saturating_add(1))?;
    Ok(Some(
        budget.task_ownership[&f.id]
            .owners
            .iter()
            .map(|owner| state.debt[owner.debt])
            .collect(),
    ))
}
pub(super) fn finish(
    f: &TypedFunction,
    block: usize,
    instruction: usize,
    old: Option<Vec<u8>>,
    state: &State,
    resumable_once: bool,
    budget: &mut Budget,
) -> Check<()> {
    let Some(old) = old else {
        return Ok(());
    };
    budget.spend(
        budget
            .task_ownership
            .len()
            .saturating_mul(4)
            .saturating_add(1),
    )?;
    let action_count = budget
        .task_ownership
        .get(&f.id)
        .map_or(0, |p| p.action_count());
    budget.spend(
        old.len()
            .saturating_mul(4)
            .saturating_add(budget.task_ownership.len().saturating_mul(4))
            .saturating_add(
                action_count
                    .checked_ilog2()
                    .map_or(1, |n| n as usize + 2)
                    .saturating_mul(12),
            ),
    )?;
    let enum_fields = match &f.blocks[block].instructions[instruction].kind {
        InstructionKind::UnpackVariant { fields, .. } => Some(fields),
        _ => None,
    };
    if let Some(fields) = enum_fields {
        // Count and prepay the actual new Link row before borrowing the
        // retained plan mutably or allocating the reordered row.
        budget.spend(old.len())?;
        let links = budget.task_ownership[&f.id]
            .owners
            .iter()
            .enumerate()
            .filter(|(i, owner)| old[*i] & LIVE == 0 && state.debt[owner.debt] & LIVE != 0)
            .count();
        budget.spend(
            fields
                .len()
                .saturating_mul(links.saturating_add(1))
                .saturating_mul(3)
                .saturating_add(links.saturating_mul(2)),
        )?;
    }
    if resumable_once {
        budget.spend(old.len().saturating_mul(8).saturating_add(4))?;
    }
    let plan = budget
        .task_ownership
        .get_mut(&f.id)
        .expect("same actual frame plan");
    let mut before = vec![];
    let mut after = vec![];
    for (index, owner) in plan.owners.iter().enumerate() {
        let was_live = old[index] & LIVE != 0;
        let live = state.debt[owner.debt] & LIVE != 0;
        if was_live && !live {
            before.push(TaskOwnerAction::Unlink(index));
        }
        if !was_live && live {
            after.push(if let Some(anchor) = owner.cleanup_anchor {
                TaskOwnerAction::LinkField {
                    slot: index,
                    after: anchor,
                }
            } else {
                TaskOwnerAction::Link(index)
            });
        }
    }
    if let Some(fields) = enum_fields {
        // Ordinary Unpack initializes the actual fields in reverse order.
        // LocalId/owner inventory order does not establish their cleanup age.
        let mut ordered = Vec::with_capacity(after.len());
        for &local in fields.iter().rev() {
            let Some(at) = after.iter().position(|action| {
                matches!(action, TaskOwnerAction::Link(slot)
                    if plan.owners[*slot].slot == TaskOwnerSlot::Local(local))
            }) else {
                continue;
            };
            ordered.push(after.remove(at));
        }
        if !after.is_empty() {
            return Err(reject(
                "E-VERIFY-TASK",
                Some(f.blocks[block].instructions[instruction].span),
                "whole Enum Unpack has a noncanonical destination frame action",
            ));
        }
        after = ordered;
    }
    let mut invocation_owner = None;
    let mut after_invoke = vec![];
    if resumable_once {
        let actual = &f.blocks[block].instructions[instruction];
        let InstructionKind::CallClosureOnce { callee, .. } = actual.kind else {
            return Err(reject("E-VERIFY-TASK", Some(actual.span),
                "remaining environment cleanup has no actual consuming invocation"));
        };
        let owner = plan.owner_for_value(callee).filter(|&index| old[index] & LIVE != 0)
            .ok_or_else(|| reject("E-VERIFY-TASK", Some(actual.span),
                "remaining environment cleanup has no unique live invocation owner"))?;
        let unlink = before.iter().position(|action| matches!(action,
            TaskOwnerAction::Unlink(index) if *index == owner)).ok_or_else(|| {
                reject("E-VERIFY-TASK", Some(actual.span),
                    "consuming invocation does not surrender its actual environment root")
            })?;
        before.remove(unlink);
        // Arguments keep their ordinary pre-call consumption. The returned
        // owners are newer than the remaining environment and must already
        // be protected if that environment's cleanup returns Pending.
        after_invoke = std::mem::take(&mut after);
        after.push(TaskOwnerAction::Unlink(owner));
        invocation_owner = Some(owner);
        let may_live = plan.owners.iter().enumerate().filter_map(|(index, slot)| {
            (index == owner || state.debt[slot.debt] & LIVE != 0).then_some(index)
        }).collect();
        plan.boundaries.push(TaskDebtBoundary {
            block: BlockId(block as u32), instruction, may_live,
        });
    }
    // TakeLocal forgets ordinary local age. Its output is a successfully
    // prepared temporary, registered after remaining outer obligations.
    // A later Store independently registers the destination local's new age.
    if !before.is_empty() || !after.is_empty() {
        plan.actions.insert(
            (BlockId(block as u32), instruction),
            TaskOwnerActions {
                block: BlockId(block as u32),
                instruction,
                before,
                after,
                invocation_owner,
                after_invoke,
            },
        );
    }
    Ok(())
}
