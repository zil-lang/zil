//! Forward must facts for actual owning callable occupants. These facts are
//! private cleanup obligations, separate from affine debt and retained loans.
use super::*;

type Point = (usize, usize);
const CLOSED_TASK: u32 = u32::MAX;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Token {
    Parameter(u32),
    Value(ValueId),
    Capture(u32),
}

#[derive(Clone, PartialEq, Eq)]
struct Query {
    occupant: Token,
    leaves: BTreeSet<u32>,
}

#[derive(Clone, PartialEq, Eq)]
struct Condition {
    query: Query,
    ready_if: bool,
}

#[derive(Clone, PartialEq, Eq)]
struct Flow {
    aliases: Vec<Option<Token>>,
    local_aliases: Vec<Option<Token>>,
    queries: Vec<Option<Query>>,
    local_queries: Vec<Option<Query>>,
    tags: Vec<Option<Query>>,
    local_tags: Vec<Option<Query>>,
    integers: Vec<Option<i32>>,
    local_integers: Vec<Option<i32>>,
    conditions: Vec<Option<Condition>>,
    local_conditions: Vec<Option<Condition>>,
    closed: BTreeMap<Token, BTreeSet<u32>>,
}

impl Flow {
    fn work(&self) -> usize {
        self.aliases
            .len()
            .saturating_add(self.local_aliases.len())
            .saturating_mul(12)
            .saturating_add(
                self.queries
                    .iter()
                    .chain(&self.local_queries)
                    .chain(&self.tags)
                    .chain(&self.local_tags)
                    .flatten()
                    .map(|q| q.leaves.len().saturating_add(1))
                    .sum::<usize>(),
            )
            .saturating_add(
                self.conditions
                    .iter()
                    .chain(&self.local_conditions)
                    .flatten()
                    .map(|c| c.query.leaves.len().saturating_add(2))
                    .sum::<usize>(),
            )
            .saturating_add(
                self.closed
                    .values()
                    .map(|s| s.len().saturating_add(1))
                    .sum::<usize>(),
            )
    }
    fn snapshot(&self, b: &mut Budget) -> CompileResult<Self> {
        b.spend(self.work(), "closed occupant snapshot")?;
        Ok(self.clone())
    }
    fn new(f: &TypedFunction, b: &mut Budget) -> CompileResult<Self> {
        b.spend(
            f.values
                .len()
                .saturating_add(f.locals.len())
                .saturating_mul(12),
            "closed occupant tables",
        )?;
        let mut result = Self {
            aliases: vec![None; f.values.len()],
            local_aliases: vec![None; f.locals.len()],
            queries: vec![None; f.values.len()],
            local_queries: vec![None; f.locals.len()],
            tags: vec![None; f.values.len()],
            local_tags: vec![None; f.locals.len()],
            integers: vec![None; f.values.len()],
            local_integers: vec![None; f.locals.len()],
            conditions: vec![None; f.values.len()],
            local_conditions: vec![None; f.locals.len()],
            closed: BTreeMap::new(),
        };
        b.spend(f.parameters.len(), "closed actual parameter occupants")?;
        for (position, local) in f.parameters.iter().enumerate() {
            let ty = f
                .locals
                .get(local.0 as usize)
                .ok_or_else(|| fail(f.span, "closed parameter local absent"))?;
            if matches!(ty.ty, Type::Closure(_)) || task_owner(&ty.ty) {
                result.local_aliases[local.0 as usize] = Some(Token::Parameter(position as u32));
            }
        }
        Ok(result)
    }
    fn add_closed(
        &mut self,
        token: Token,
        leaves: &BTreeSet<u32>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        b.spend(
            self.closed
                .len()
                .saturating_add(leaves.len())
                .saturating_add(1),
            "closed actual leaf update",
        )?;
        self.closed
            .entry(token)
            .or_default()
            .extend(leaves.iter().copied());
        Ok(())
    }
    // A static constructor instruction may execute again in a loop. Forget all
    // old aliases and queries before reusing its finite site token. An older
    // live occupant then becomes unknown rather than aliasing the new frame.
    fn retire(&mut self, token: Token, b: &mut Budget) -> CompileResult<()> {
        b.spend(
            self.work(),
            "closed replacement invalidates old occupant queries",
        )?;
        for alias in self.aliases.iter_mut().chain(&mut self.local_aliases) {
            if *alias == Some(token) {
                *alias = None;
            }
        }
        for query in self
            .queries
            .iter_mut()
            .chain(&mut self.local_queries)
            .chain(&mut self.tags)
            .chain(&mut self.local_tags)
        {
            if query.as_ref().is_some_and(|q| q.occupant == token) {
                *query = None;
            }
        }
        for condition in self.conditions.iter_mut().chain(&mut self.local_conditions) {
            if condition
                .as_ref()
                .is_some_and(|c| c.query.occupant == token)
            {
                *condition = None;
            }
        }
        self.closed.remove(&token);
        Ok(())
    }
    fn forget_effects(&mut self, b: &mut Budget) -> CompileResult<()> {
        b.spend(
            self.work(),
            "unproved mutable execution invalidates closed facts",
        )?;
        self.closed.clear();
        self.queries.fill(None);
        self.local_queries.fill(None);
        self.tags.fill(None);
        self.local_tags.fill(None);
        self.conditions.fill(None);
        self.local_conditions.fill(None);
        Ok(())
    }
    fn expose_mutable_head(&mut self, token: Token, b: &mut Budget) -> CompileResult<()> {
        // A retained mutable head may later be driven by another Task or
        // callable. Do not carry an older close observation across that lease.
        // The owning handle itself still exists and may be moved normally.
        b.spend(
            self.work().saturating_mul(2),
            "closed mutable head exposure",
        )?;
        let owners = self.aliases.clone();
        let locals = self.local_aliases.clone();
        self.retire(token, b)?;
        self.aliases = owners;
        self.local_aliases = locals;
        Ok(())
    }
    fn merge(&mut self, incoming: &Self, b: &mut Budget) -> CompileResult<bool> {
        b.spend(
            self.work()
                .saturating_add(incoming.work())
                .saturating_mul(2),
            "closed CFG must intersection",
        )?;
        let old = self.clone();
        fn intersect<T: PartialEq>(left: &mut [Option<T>], right: &[Option<T>]) {
            for (left, right) in left.iter_mut().zip(right) {
                if left != right {
                    *left = None;
                }
            }
        }
        intersect(&mut self.aliases, &incoming.aliases);
        intersect(&mut self.local_aliases, &incoming.local_aliases);
        intersect(&mut self.queries, &incoming.queries);
        intersect(&mut self.local_queries, &incoming.local_queries);
        intersect(&mut self.tags, &incoming.tags);
        intersect(&mut self.local_tags, &incoming.local_tags);
        intersect(&mut self.integers, &incoming.integers);
        intersect(&mut self.local_integers, &incoming.local_integers);
        intersect(&mut self.conditions, &incoming.conditions);
        intersect(&mut self.local_conditions, &incoming.local_conditions);
        self.closed.retain(|token, leaves| {
            if let Some(other) = incoming.closed.get(token) {
                leaves.retain(|leaf| other.contains(leaf));
            } else {
                leaves.clear();
            }
            !leaves.is_empty()
        });
        Ok(*self != old)
    }
}

fn task_owner(ty: &Type) -> bool {
    matches!(ty, Type::Managed(payload) if matches!(payload.as_ref(), Type::Task { .. } | Type::TaskI32))
}

fn ordinary_teardown(module: &TypedModule, ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    b.type_work(ty, false, "closed capture remaining teardown type")?;
    if super::super::super::stored_callables::scalar(ty)
        || matches!(ty, Type::Address { .. } | Type::Desc(_))
    {
        return Ok(true);
    }
    let Type::Managed(payload) = ty else {
        return Ok(false);
    };
    if !super::super::super::stored_callables::resource_payload(payload, b)? {
        return Ok(false);
    }
    b.spend(
        module.resources.len(),
        "closed capture actual resource teardown inventory",
    )?;
    let mut matched = None;
    for resource in &module.resources {
        b.type_work(
            &resource.payload,
            false,
            "closed capture nominal resource identity",
        )?;
        if resource.payload == **payload {
            if matched.replace(resource).is_some() {
                return Ok(false);
            }
        }
    }
    let Some(resource) = matched else {
        return Ok(false);
    };
    let Some(id) = resource.teardown else {
        return Ok(true);
    };
    let Some(drop) = module.functions.get(id.0 as usize) else {
        return Ok(false);
    };
    b.spend(
        drop.parameters.len().saturating_add(1),
        "closed capture exact direct teardown ABI",
    )?;
    Ok(drop.call_kind == CallKind::Direct
        && drop.foreign.is_none()
        && drop.environment.is_none()
        && drop.return_type == Type::Unit
        && matches!(drop.parameters.as_slice(), [parameter]
            if drop.locals.get(parameter.0 as usize).is_some_and(|local| local.ty == Type::Desc(payload.clone()))))
}

fn inventory(
    module: &TypedModule,
    id: ClosureId,
    b: &mut Budget,
) -> CompileResult<Option<BTreeSet<u32>>> {
    let Some(d) = module.closures.get(id.0 as usize) else {
        return Ok(None);
    };
    let Some(entry) = module.functions.get(d.function.0 as usize) else {
        return Ok(None);
    };
    if entry.environment != Some(id)
        || entry.foreign.is_some()
        || entry.call_kind != CallKind::Direct
        || entry.return_type != d.result
    {
        return Ok(None);
    }
    b.spend(
        d.captures.len(),
        "closed actual owned capture leaf inventory",
    )?;
    let mut tasks = BTreeSet::new();
    for (index, ty) in d.captures.iter().enumerate() {
        if task_owner(ty) {
            tasks.insert(index as u32);
        } else if !ordinary_teardown(module, ty, b)? {
            return Ok(None);
        }
    }
    Ok(Some(tasks))
}

#[derive(Default)]
struct ExitProof {
    always: BTreeSet<u32>,
    ready: BTreeSet<u32>,
}

fn entry_is_stable(
    f: &TypedFunction,
    captures: &BTreeSet<u32>,
    b: &mut Budget,
) -> CompileResult<bool> {
    for row in &f.blocks {
        b.spend(
            row.instructions.len(),
            "closed capture replacement/exposure inventory",
        )?;
        for instruction in &row.instructions {
            match &instruction.kind {
                InstructionKind::TakeCapture { index }
                | InstructionKind::WriteCapture { index, .. }
                    if captures.contains(index) =>
                {
                    return Ok(false);
                }
                InstructionKind::Call { .. }
                | InstructionKind::CallIndirect { .. }
                | InstructionKind::ForeignCall { .. }
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
    }
    Ok(true)
}

fn clone_query(query: &Option<Query>, b: &mut Budget) -> CompileResult<Option<Query>> {
    b.spend(
        query
            .as_ref()
            .map_or(1, |q| q.leaves.len().saturating_add(1)),
        "closed query transport",
    )?;
    Ok(query.clone())
}

fn step(
    module: &TypedModule,
    f: &TypedFunction,
    state: &mut Flow,
    i: &Instruction,
    body: bool,
    b: &mut Budget,
) -> CompileResult<()> {
    b.spend(1, "closed actual instruction")?;
    let result = i.result.map(|v| v.0 as usize);
    if let Some(out) = result {
        if out >= state.aliases.len() {
            return Err(fail(i.span, "closed result outside actual SSA arena"));
        }
        state.aliases[out] = None;
        state.queries[out] = None;
        state.tags[out] = None;
        state.conditions[out] = None;
        state.integers[out] = None;
    }
    match &i.kind {
        InstructionKind::BorrowCapture {
            index,
            mutable: true,
        } if body => {
            let ty = f
                .environment
                .and_then(|id| module.closures.get(id.0 as usize))
                .and_then(|d| d.captures.get(*index as usize));
            if ty.is_some_and(task_owner)
                && let Some(out) = result
            {
                state.aliases[out] = Some(Token::Capture(*index));
            }
        }
        InstructionKind::BorrowLocalMut { local } | InstructionKind::BorrowLocal { local } => {
            if matches!(i.kind, InstructionKind::BorrowLocalMut { .. })
                && f.locals
                    .get(local.0 as usize)
                    .is_some_and(|l| matches!(l.ty, Type::Closure(_)))
                && let Some(token) = state.local_aliases.get(local.0 as usize).copied().flatten()
            {
                state.expose_mutable_head(token, b)?;
            }
            if let Some(out) = result {
                state.aliases[out] = state.local_aliases.get(local.0 as usize).copied().flatten();
            }
        }
        InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
            let local = local.0 as usize;
            if local >= state.local_aliases.len() {
                return Err(fail(i.span, "closed source local absent"));
            }
            if let Some(out) = result {
                state.aliases[out] = state.local_aliases[local];
                state.queries[out] = clone_query(&state.local_queries[local], b)?;
                state.tags[out] = clone_query(&state.local_tags[local], b)?;
                state.integers[out] = state.local_integers[local];
                b.spend(
                    state.local_conditions[local]
                        .as_ref()
                        .map_or(1, |c| c.query.leaves.len().saturating_add(2)),
                    "closed condition move",
                )?;
                state.conditions[out] = state.local_conditions[local].clone();
            }
            if matches!(i.kind, InstructionKind::TakeLocal { .. }) {
                state.local_aliases[local] = None;
                state.local_queries[local] = None;
                state.local_tags[local] = None;
                state.local_conditions[local] = None;
                state.local_integers[local] = None;
            }
        }
        InstructionKind::Store { local, value } => {
            let target = local.0 as usize;
            let source = value.0 as usize;
            if target >= state.local_aliases.len() || source >= state.aliases.len() {
                return Err(fail(i.span, "closed store arena mismatch"));
            }
            let alias = state.aliases[source];
            if let Some(previous) = state.local_aliases[target]
                && alias != Some(previous)
            {
                state.retire(previous, b)?;
            }
            state.local_aliases[target] = alias;
            state.local_queries[target] = clone_query(&state.queries[source], b)?;
            state.local_tags[target] = clone_query(&state.tags[source], b)?;
            state.local_integers[target] = state.integers[source];
            b.spend(
                state.conditions[source]
                    .as_ref()
                    .map_or(1, |c| c.query.leaves.len().saturating_add(2)),
                "closed stored condition",
            )?;
            state.local_conditions[target] = state.conditions[source].clone();
        }
        InstructionKind::ReborrowAddress { address, .. } => {
            if let Some(out) = result {
                state.aliases[out] = state.aliases.get(address.0 as usize).copied().flatten();
            }
        }
        InstructionKind::TaskPollClose { task } => {
            if let Some(out) = result
                && f.values.get(out) == Some(&super::super::task_poll_type(&Type::Unit))
                && let Some(token) = state.aliases.get(task.0 as usize).copied().flatten()
            {
                b.spend(1, "canonical close result actual occupant")?;
                state.queries[out] = Some(Query {
                    occupant: token,
                    leaves: BTreeSet::from([CLOSED_TASK]),
                });
            }
        }
        InstructionKind::MakeVariant { variant: 0, fields }
            if fields.is_empty()
                && result.is_some_and(|out| {
                    f.values.get(out) == Some(&super::super::task_poll_type(&Type::Unit))
                }) =>
        {
            // This exit cannot return Ready. It supplies no Closed fact; it
            // only leaves a Ready-conditional postcondition vacuous here.
            state.integers[result.expect("checked canonical Pending")] = Some(0);
        }
        InstructionKind::VariantTag { value } => {
            if let Some(out) = result {
                state.tags[out] =
                    clone_query(state.queries.get(value.0 as usize).unwrap_or(&None), b)?;
            }
        }
        InstructionKind::Constant(Constant::I32(n)) => {
            if let Some(out) = result {
                state.integers[out] = Some(*n);
            }
        }
        InstructionKind::Binary {
            op: BinaryOp::Equal,
            left,
            right,
        } => {
            if let Some(out) = result {
                for (tag, number) in [(*left, *right), (*right, *left)] {
                    if let Some(query) = state.tags.get(tag.0 as usize).and_then(Option::as_ref)
                        && let Some(number @ (0 | 1)) =
                            state.integers.get(number.0 as usize).copied().flatten()
                    {
                        b.spend(
                            query.leaves.len().saturating_add(2),
                            "canonical Ready branch relation",
                        )?;
                        state.conditions[out] = Some(Condition {
                            query: query.clone(),
                            ready_if: number == 1,
                        });
                        break;
                    }
                }
            }
        }
        InstructionKind::CallClosureMut { callee, .. } if !body => {
            let token = state
                .local_aliases
                .get(callee.0 as usize)
                .copied()
                .flatten();
            let id = f.locals.get(callee.0 as usize).and_then(|l| {
                if let Type::Closure(id) = l.ty {
                    Some(id)
                } else {
                    None
                }
            });
            if let (Some(token), Some(id)) = (token, id) {
                if let Some(proof) = exits(module, id, b)? {
                    state.add_closed(token, &proof.always, b)?;
                    if let Some(out) = result
                        && !proof.ready.is_empty()
                    {
                        state.queries[out] = Some(Query {
                            occupant: token,
                            leaves: proof.ready,
                        });
                    }
                } else {
                    state.forget_effects(b)?;
                }
            } else {
                state.forget_effects(b)?;
            }
        }
        InstructionKind::DropClosure { local }
        | InstructionKind::StorageDead(local)
        | InstructionKind::StorageLive(local) => {
            let owner = f
                .locals
                .get(local.0 as usize)
                .is_some_and(|local| matches!(local.ty, Type::Closure(_)) || task_owner(&local.ty));
            if owner
                && let Some(token) = state.local_aliases.get(local.0 as usize).copied().flatten()
            {
                state.retire(token, b)?;
            }
            if let Some(alias) = state.local_aliases.get_mut(local.0 as usize) {
                *alias = None;
            }
            if let Some(query) = state.local_queries.get_mut(local.0 as usize) {
                *query = None;
            }
            if let Some(tag) = state.local_tags.get_mut(local.0 as usize) {
                *tag = None;
            }
            if let Some(condition) = state.local_conditions.get_mut(local.0 as usize) {
                *condition = None;
            }
            if let Some(integer) = state.local_integers.get_mut(local.0 as usize) {
                *integer = None;
            }
        }
        InstructionKind::Call { .. }
        | InstructionKind::CallIndirect { .. }
        | InstructionKind::ForeignCall { .. }
        | InstructionKind::CallClosure { .. }
        | InstructionKind::CallClosureMut { .. }
        | InstructionKind::CallClosureShared { .. }
        | InstructionKind::CallClosureOnce { .. }
        | InstructionKind::CallClosureBorrowed { .. }
        | InstructionKind::WriteAddress { .. }
        | InstructionKind::WithForeignCallback { .. }
        | InstructionKind::WithScopedThread { .. }
        | InstructionKind::WithSyncCell { .. } => {
            state.forget_effects(b)?;
            if let Some(out) = result
                && matches!(f.values.get(out), Some(Type::Closure(_)))
            {
                let token = Token::Value(i.result.expect("checked result"));
                state.retire(token, b)?;
                state.aliases[out] = Some(token);
            }
        }
        InstructionKind::MakeClosure { .. } => {
            if let Some(out) = result {
                let token = Token::Value(i.result.expect("checked result"));
                state.retire(token, b)?;
                state.aliases[out] = Some(token);
            }
        }
        _ => {}
    }
    Ok(())
}

fn run(
    module: &TypedModule,
    f: &TypedFunction,
    body: bool,
    b: &mut Budget,
) -> CompileResult<Vec<Option<Flow>>> {
    b.spend(f.blocks.len().saturating_mul(3), "closed CFG work frontier")?;
    let mut incoming = vec![None; f.blocks.len()];
    let entry = f.entry.0 as usize;
    if entry >= incoming.len() {
        return Err(fail(f.span, "closed CFG entry absent"));
    }
    incoming[entry] = Some(Flow::new(f, b)?);
    let mut queue = std::collections::VecDeque::from([entry]);
    let mut queued = vec![false; f.blocks.len()];
    queued[entry] = true;
    while let Some(block) = queue.pop_front() {
        queued[block] = false;
        let mut state = incoming[block]
            .as_ref()
            .expect("queued actual state")
            .snapshot(b)?;
        for instruction in &f.blocks[block].instructions {
            step(module, f, &mut state, instruction, body, b)?;
        }
        let targets = match f.blocks[block].terminator {
            Terminator::Jump(next) => [(Some(next), None), (None, None)],
            Terminator::Branch {
                condition,
                then_block,
                else_block,
            } => [
                (Some(then_block), Some((condition, true))),
                (Some(else_block), Some((condition, false))),
            ],
            _ => [(None, None), (None, None)],
        };
        for (target, select) in targets {
            let Some(target) = target else {
                continue;
            };
            let target = target.0 as usize;
            if target >= incoming.len() {
                return Err(fail(f.span, "closed CFG successor absent"));
            }
            let mut next = state.snapshot(b)?;
            if let Some((condition, yes)) = select
                && let Some(relation) = state
                    .conditions
                    .get(condition.0 as usize)
                    .and_then(Option::as_ref)
                && yes == relation.ready_if
            {
                next.add_closed(relation.query.occupant, &relation.query.leaves, b)?;
            }
            let changed = if let Some(previous) = &mut incoming[target] {
                previous.merge(&next, b)?
            } else {
                incoming[target] = Some(next);
                true
            };
            if changed && !queued[target] {
                queued[target] = true;
                queue.push_back(target);
            }
        }
    }
    Ok(incoming)
}

fn exits(module: &TypedModule, id: ClosureId, b: &mut Budget) -> CompileResult<Option<ExitProof>> {
    let Some(tasks) = inventory(module, id, b)? else {
        return Ok(None);
    };
    let d = &module.closures[id.0 as usize];
    let f = &module.functions[d.function.0 as usize];
    if !entry_is_stable(f, &tasks, b)? {
        return Ok(None);
    }
    let incoming = run(module, f, true, b)?;
    let mut proof = ExitProof {
        always: tasks.clone(),
        ready: tasks.clone(),
    };
    b.spend(
        tasks.len().saturating_mul(2),
        "closed callable exit candidate masks",
    )?;
    let mut observed = false;
    for (index, row) in f.blocks.iter().enumerate() {
        let Some(before) = &incoming[index] else {
            continue;
        };
        let Terminator::Return(value) = row.terminator else {
            continue;
        };
        observed = true;
        let mut state = before.snapshot(b)?;
        for instruction in &row.instructions {
            step(module, f, &mut state, instruction, true, b)?;
        }
        for &capture in &tasks {
            b.spend(
                state.closed.len().saturating_add(1),
                "closed captured task exit obligation",
            )?;
            let closed = state
                .closed
                .get(&Token::Capture(capture))
                .is_some_and(|leaves| leaves.contains(&CLOSED_TASK));
            if !closed {
                proof.always.remove(&capture);
                let related = value
                    .and_then(|v| state.queries.get(v.0 as usize))
                    .and_then(Option::as_ref)
                    .is_some_and(|q| {
                        q.occupant == Token::Capture(capture) && q.leaves.contains(&CLOSED_TASK)
                    });
                let pending = value.is_some_and(|value| {
                    f.values.get(value.0 as usize)
                        == Some(&super::super::task_poll_type(&Type::Unit))
                        && state.integers.get(value.0 as usize) == Some(&Some(0))
                });
                if !related && !pending {
                    proof.ready.remove(&capture);
                }
            }
        }
    }
    if d.result != super::super::task_poll_type(&Type::Unit) {
        proof.ready.clear();
    }
    Ok(observed.then_some(proof))
}

pub(super) fn allow_at(
    module: &TypedModule,
    f: &TypedFunction,
    local: LocalId,
    at: Point,
    b: &mut Budget,
) -> CompileResult<bool> {
    let Some(Type::Closure(id)) = f.locals.get(local.0 as usize).map(|l| &l.ty) else {
        return Ok(false);
    };
    let Some(tasks) = inventory(module, *id, b)? else {
        return Ok(false);
    };
    let incoming = run(module, f, false, b)?;
    let Some(before) = incoming.get(at.0).and_then(Option::as_ref) else {
        return Ok(false);
    };
    let Some(row) = f.blocks.get(at.0) else {
        return Ok(false);
    };
    if at.1 > row.instructions.len() {
        return Ok(false);
    }
    let mut state = before.snapshot(b)?;
    for instruction in &row.instructions[..at.1] {
        step(module, f, &mut state, instruction, false, b)?;
    }
    let Some(token) = state.local_aliases.get(local.0 as usize).copied().flatten() else {
        return Ok(false);
    };
    b.spend(
        tasks.len().saturating_add(state.closed.len()),
        "closed drop retains every actual task leaf obligation",
    )?;
    Ok(tasks.is_empty()
        || state
            .closed
            .get(&token)
            .is_some_and(|leaves| tasks.iter().all(|capture| leaves.contains(capture))))
}
