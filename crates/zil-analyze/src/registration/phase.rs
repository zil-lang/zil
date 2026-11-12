//! Producer proof for the finite pollable registration phase relation.
//! Runtime drain remains mandatory; these facts never establish ownership.
use super::*;
mod products;
use std::collections::VecDeque;
const OPEN: u8 = 1;
const CLOSING: u8 = 2;
const CLOSED: u8 = 4;
const ANY: u8 = OPEN | CLOSING | CLOSED;
fn bit(s: ProtocolState) -> u8 {
    match s {
        ProtocolState::Open => OPEN,
        ProtocolState::Closing => CLOSING,
        ProtocolState::Closed => CLOSED,
    }
}
fn mask(v: &[ProtocolState]) -> u8 {
    v.iter().fold(0, |m, s| m | bit(*s))
}
fn is_owner(t: &Type) -> bool {
    matches!(t, Type::Managed(p) if matches!(p.as_ref(),Type::AsyncRegistration{..}))
}
fn is_borrow(t: &Type) -> bool {
    matches!(t, Type::Address{mutable:true,inner} if is_owner(inner))
}
fn invalid(f: &TypedFunction, text: &str) -> Vec<Diagnostic> {
    fail(f.span, text)
}
fn shape(
    f: &TypedFunction,
    c: &FunctionProtocolContract,
    budget: &mut Budget,
) -> CompileResult<()> {
    budget.spend(
        c.entry
            .len()
            .saturating_add(c.exits.len())
            .saturating_add(4),
        "protocol header",
    )?;
    budget.spend(
        f.parameters
            .len()
            .saturating_mul(f.protocol_contracts.len().saturating_add(1)),
        "protocol role coverage",
    )?;
    if c.protocol != ProtocolKind::AsyncRegistrationV1
        || c.entry.is_empty()
        || c.exits.is_empty()
        || c.entry.windows(2).any(|p| p[0] >= p[1])
        || c.parameter as usize >= f.parameters.len()
    {
        return Err(invalid(f, "invalid finite protocol role or entry"));
    }
    let local = f.parameters[c.parameter as usize];
    let Some(role) = f.locals.get(local.0 as usize) else {
        return Err(invalid(f, "protocol parameter local is absent"));
    };
    if f.foreign.is_some()
        || f.environment.is_some()
        || (f.call_kind != CallKind::Direct && !is_owner(&role.ty))
    {
        return Err(invalid(
            f,
            "protocol relation requires an actual ordinary body",
        ));
    }
    let allowed_exit = |exit: ProtocolExit| match &f.return_type {
        Type::ErrorUnion { .. } => matches!(exit, ProtocolExit::ErrorOk | ProtocolExit::ErrorErr),
        Type::Enum { name, .. }
            if name == &format!("@std.task:v1:Poll:{}", Type::Unit.fingerprint()) =>
        {
            matches!(exit, ProtocolExit::PollPending | ProtocolExit::PollReady)
        }
        _ => exit == ProtocolExit::Return,
    };
    if !(is_borrow(&role.ty) || (is_owner(&role.ty) && f.return_type.is_copy_value()))
        || f.parameters.iter().enumerate().any(|(i, l)| {
            i != c.parameter as usize
                && f.locals
                    .get(l.0 as usize)
                    .is_some_and(|other| is_borrow(&other.ty) || is_owner(&other.ty))
                && !f
                    .protocol_contracts
                    .iter()
                    .any(|role| role.parameter as usize == i)
        })
    {
        return Err(invalid(
            f,
            "finite protocol requires one exclusive borrowed registration role",
        ));
    }
    for row in &c.exits {
        budget.spend(row.output.len().saturating_add(1), "protocol transition")?;
        if !allowed_exit(row.exit)
            || !c.entry.contains(&row.input)
            || (row.output.is_empty() && !is_owner(&role.ty))
            || (is_owner(&role.ty) && !row.output.is_empty())
            || row.output.windows(2).any(|p| p[0] >= p[1])
        {
            return Err(invalid(f, "invalid protocol transition row"));
        }
    }
    if c.exits
        .windows(2)
        .any(|p| (p[0].input, p[0].exit) >= (p[1].input, p[1].exit))
    {
        return Err(invalid(
            f,
            "protocol transition rows must be unique and ordered",
        ));
    }
    Ok(())
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct ResultOrigin {
    root: usize,
    generation: u32,
    low: u8,
    high: u8,
}
#[derive(Clone, PartialEq, Eq)]
struct Flow {
    products: Option<products::Products>,
    phases: Vec<u8>,
    generations: Vec<u32>,
    aliases: Vec<Option<usize>>,
    values: Vec<u8>,
    value_aliases: Vec<Option<usize>>,
    local_results: Vec<Vec<ResultOrigin>>,
    results: Vec<Vec<ResultOrigin>>,
    local_tags: Vec<Option<usize>>,
    tags: Vec<Option<usize>>,
    local_integers: Vec<Option<i32>>,
    integers: Vec<Option<i32>>,
    local_conditions: Vec<Vec<(usize, u32, u8, u8)>>,
    conditions: Vec<Vec<(usize, u32, u8, u8)>>,
    local_return_tags: Vec<Option<ProtocolExit>>,
    return_tags: Vec<Option<ProtocolExit>>,
}
impl Flow {
    fn relation_work(&self) -> usize {
        self.results
            .iter()
            .chain(&self.local_results)
            .map(|r| r.len().saturating_mul(4))
            .sum::<usize>()
            .saturating_add(
                self.conditions
                    .iter()
                    .chain(&self.local_conditions)
                    .map(|r| r.len().saturating_mul(4))
                    .sum::<usize>(),
            )
    }
    fn invalidate(&mut self, root: usize, budget: &mut Budget) -> CompileResult<()> {
        let width = self
            .results
            .len()
            .saturating_add(self.local_results.len())
            .saturating_mul(4)
            .saturating_add(1);
        budget.spend(
            width.saturating_add(self.relation_work()),
            "protocol invalidation",
        )?;
        for result in self.results.iter_mut().chain(&mut self.local_results) {
            result.retain(|origin| origin.root != root);
        }
        for condition in self.conditions.iter_mut().chain(&mut self.local_conditions) {
            condition.retain(|(owner, _, _, _)| *owner != root);
        }
        Ok(())
    }
    fn new(f: &TypedFunction, entry: Option<(usize, u8)>) -> Self {
        let n = f.locals.len();
        let v = f.values.len();
        let mut s = Self {
            products: f
                .blocks
                .iter()
                .any(|block| {
                    block
                        .instructions
                        .iter()
                        .any(|i| matches!(i.kind, InstructionKind::TaskCreateOwned { .. }))
                })
                .then(|| products::Products::new(f)),
            phases: vec![0; n],
            generations: vec![0; n],
            aliases: vec![None; n],
            values: vec![0; v],
            value_aliases: vec![None; v],
            local_results: vec![Vec::new(); n],
            results: vec![Vec::new(); v],
            local_tags: vec![None; n],
            tags: vec![None; v],
            local_integers: vec![None; n],
            integers: vec![None; v],
            local_conditions: vec![Vec::new(); n],
            conditions: vec![Vec::new(); v],
            local_return_tags: vec![None; n],
            return_tags: vec![None; v],
        };
        for (i, l) in f.parameters.iter().enumerate() {
            if is_borrow(&f.locals[l.0 as usize].ty) || is_owner(&f.locals[l.0 as usize].ty) {
                s.aliases[l.0 as usize] = Some(l.0 as usize);
                if entry.map(|(j, _)| j) == Some(i) {
                    s.phases[l.0 as usize] = entry.unwrap().1;
                } else {
                    s.phases[l.0 as usize] = f
                        .protocol_contracts
                        .iter()
                        .find(|c| c.parameter as usize == i)
                        .map_or(ANY, |c| mask(&c.entry));
                }
            }
        }
        s
    }
    fn merge(&mut self, other: &Self) -> bool {
        let old = self.clone();
        if let (Some(to), Some(from)) = (&mut self.products, &other.products) {
            to.merge(from);
        }
        for (a, b) in self.phases.iter_mut().zip(&other.phases) {
            *a |= *b;
        }
        for (a, b) in self.generations.iter_mut().zip(&other.generations) {
            if *a != *b {
                *a = u32::MAX;
            }
        }
        for (a, b) in self.aliases.iter_mut().zip(&other.aliases) {
            if *a != *b {
                *a = None;
            }
        }
        for (a, b) in self.values.iter_mut().zip(&other.values) {
            *a |= *b;
        }
        for (a, b) in self.value_aliases.iter_mut().zip(&other.value_aliases) {
            if *a != *b {
                *a = None;
            }
        }
        for (a, b) in self.local_results.iter_mut().zip(&other.local_results) {
            a.retain(|row| b.contains(row));
        }
        for (a, b) in self.results.iter_mut().zip(&other.results) {
            a.retain(|row| b.contains(row));
        }
        for (a, b) in self.local_tags.iter_mut().zip(&other.local_tags) {
            if *a != *b {
                *a = None;
            }
        }
        for (a, b) in self.tags.iter_mut().zip(&other.tags) {
            if *a != *b {
                *a = None;
            }
        }
        for (a, b) in self.local_integers.iter_mut().zip(&other.local_integers) {
            if *a != *b {
                *a = None;
            }
        }
        for (a, b) in self.integers.iter_mut().zip(&other.integers) {
            if *a != *b {
                *a = None;
            }
        }
        for (a, b) in self
            .local_conditions
            .iter_mut()
            .zip(&other.local_conditions)
        {
            a.retain(|row| b.contains(row));
        }
        for (a, b) in self.conditions.iter_mut().zip(&other.conditions) {
            a.retain(|row| b.contains(row));
        }
        for (a, b) in self
            .local_return_tags
            .iter_mut()
            .zip(&other.local_return_tags)
        {
            if *a != *b {
                *a = None;
            }
        }
        for (a, b) in self.return_tags.iter_mut().zip(&other.return_tags) {
            if *a != *b {
                *a = None;
            }
        }
        *self != old
    }
}
fn root(s: &Flow, v: ValueId, f: &TypedFunction) -> CompileResult<usize> {
    s.value_aliases[v.0 as usize]
        .ok_or_else(|| invalid(f, "protocol operation lacks one actual owner root"))
}
fn effect(
    s: &mut Flow,
    i: &Instruction,
    f: &TypedFunction,
    m: &TypedModule,
    b: &mut Budget,
) -> CompileResult<()> {
    b.spend(
        16usize.saturating_add(s.relation_work()),
        "protocol instruction",
    )?;
    let out = i.result.map(|v| v.0 as usize);
    match &i.kind {
        InstructionKind::Load(l) => {
            if let Some(o) = out {
                s.value_aliases[o] = s.aliases[l.0 as usize];
                s.values[o] = s.phases[l.0 as usize];
                s.results[o] = s.local_results[l.0 as usize].clone();
                s.tags[o] = s.local_tags[l.0 as usize];
                s.integers[o] = s.local_integers[l.0 as usize];
                s.conditions[o] = s.local_conditions[l.0 as usize].clone();
                s.return_tags[o] = s.local_return_tags[l.0 as usize];
            }
        }
        InstructionKind::BorrowLocalMut { local } => {
            if let Some(o) = out {
                s.value_aliases[o] = Some(local.0 as usize);
                s.results[o] = s.local_results[local.0 as usize].clone();
                s.tags[o] = s.local_tags[local.0 as usize];
                s.integers[o] = s.local_integers[local.0 as usize];
                s.conditions[o] = s.local_conditions[local.0 as usize].clone();
                s.return_tags[o] = s.local_return_tags[local.0 as usize];
            }
        }
        InstructionKind::ReborrowAddress { address, .. } => {
            if let Some(o) = out {
                s.value_aliases[o] = s.value_aliases[address.0 as usize];
            }
        }
        InstructionKind::TakeLocal { local } => {
            if let Some(o) = out {
                s.values[o] = s.phases[local.0 as usize];
                s.value_aliases[o] = Some(local.0 as usize);
            }
            s.invalidate(local.0 as usize, b)?;
            s.phases[local.0 as usize] = 0;
            s.generations[local.0 as usize] = s.generations[local.0 as usize].saturating_add(1);
        }
        InstructionKind::Store { local, value } => {
            let l = local.0 as usize;
            s.aliases[l] = s.value_aliases[value.0 as usize];
            s.local_results[l] = s.results[value.0 as usize].clone();
            s.local_tags[l] = s.tags[value.0 as usize];
            s.local_integers[l] = s.integers[value.0 as usize];
            s.local_conditions[l] = s.conditions[value.0 as usize].clone();
            s.local_return_tags[l] = s.return_tags[value.0 as usize];
            if is_owner(&f.locals[l].ty) {
                s.invalidate(l, b)?;
                s.phases[l] = s.values[value.0 as usize];
                s.generations[l] = s.generations[l].saturating_add(1);
            }
        }
        InstructionKind::AsyncRegistrationStart { .. } => {
            if let Some(o) = out {
                s.values[o] = OPEN | CLOSED;
            }
        }
        InstructionKind::AsyncRegistrationRequestClose { registration } => {
            let r = root(s, *registration, f)?;
            if s.phases[r] == 0 {
                return Err(invalid(
                    f,
                    "request requires an available actual registration",
                ));
            }
            s.invalidate(r, b)?;
            s.phases[r] = (s.phases[r] & CLOSED)
                | if s.phases[r] & (OPEN | CLOSING) != 0 {
                    CLOSING
                } else {
                    0
                };
            s.generations[r] = s.generations[r].saturating_add(1);
        }
        InstructionKind::AsyncRegistrationPollClose { registration } => {
            let r = root(s, *registration, f)?;
            if s.phases[r] == 0 {
                return Err(invalid(f, "poll requires an available actual registration"));
            }
            let pending = if s.phases[r] & (OPEN | CLOSING) != 0 {
                CLOSING
            } else {
                0
            };
            s.invalidate(r, b)?;
            s.phases[r] = pending | CLOSED;
            s.generations[r] = s.generations[r].saturating_add(1);
            if let Some(o) = out {
                s.results[o] = vec![ResultOrigin {
                    root: r,
                    generation: s.generations[r],
                    low: pending,
                    high: CLOSED,
                }];
            }
        }
        InstructionKind::AsyncRegistrationFinishDestroyChecked { registration } => {
            let r = root(s, *registration, f)?;
            if s.values[registration.0 as usize] != CLOSED {
                return Err(invalid(
                    f,
                    "checked finish requires statically proven Closed on the same live owner",
                ));
            }
            s.invalidate(r, b)?;
            s.phases[r] = 0;
            s.generations[r] = s.generations[r].saturating_add(1);
        }
        InstructionKind::AsyncRegistrationFinishDestroy { registration } => {
            if let Some(r) = s.value_aliases[registration.0 as usize] {
                s.invalidate(r, b)?;
                s.phases[r] = 0;
                s.generations[r] = s.generations[r].saturating_add(1);
            }
        }
        InstructionKind::VariantTag { value } => {
            if let Some(o) = out {
                s.tags[o] = Some(value.0 as usize);
            }
        }
        InstructionKind::ErrorTag { value } => {
            if let Some(o) = out {
                s.conditions[o] = s.results[value.0 as usize]
                    .iter()
                    .map(|p| (p.root, p.generation, p.low, p.high))
                    .collect();
            }
        }
        InstructionKind::Constant(Constant::I32(n)) => {
            if let Some(o) = out {
                s.integers[o] = Some(*n);
            }
        }
        InstructionKind::Binary {
            op: BinaryOp::Equal,
            left,
            right,
        } => {
            if let Some(o) = out {
                let pair = [(*left, *right), (*right, *left)]
                    .into_iter()
                    .find_map(|(t, n)| Some((s.tags[t.0 as usize]?, s.integers[n.0 as usize]?)));
                if let Some((v, n)) = pair.filter(|(_, n)| *n == 0 || *n == 1) {
                    s.conditions[o] = s.results[v]
                        .iter()
                        .map(|p| {
                            (
                                p.root,
                                p.generation,
                                if n == 0 { p.low } else { p.high },
                                if n == 0 { p.high } else { p.low },
                            )
                        })
                        .collect();
                }
            }
        }
        InstructionKind::MakeErrorUnion { is_error, .. } => {
            if let Some(o) = out {
                s.return_tags[o] = Some(if *is_error {
                    ProtocolExit::ErrorErr
                } else {
                    ProtocolExit::ErrorOk
                });
            }
        }
        InstructionKind::Call {
            function,
            arguments,
        } => {
            let callee = m
                .functions
                .get(function.0 as usize)
                .ok_or_else(|| invalid(f, "protocol callee is absent"))?;
            if !callee.protocol_contracts.is_empty() {
                if let Some(o) = out {
                    s.results[o].clear();
                }
                for c in &callee.protocol_contracts {
                    b.spend(
                        c.exits
                            .len()
                            .saturating_mul(8)
                            .saturating_add(arguments.len()),
                        "protocol call relation",
                    )?;
                    let arg = *arguments
                        .get(c.parameter as usize)
                        .ok_or_else(|| invalid(f, "protocol call role absent"))?;
                    let r = root(s, arg, f)?;
                    let owned = is_owner(
                        &callee.locals[callee.parameters[c.parameter as usize].0 as usize].ty,
                    );
                    let before = if owned {
                        s.values[arg.0 as usize]
                    } else {
                        s.phases[r]
                    };
                    if before == 0
                        || arguments.iter().enumerate().any(|(i, v)| {
                            i != c.parameter as usize && s.value_aliases[v.0 as usize] == Some(r)
                        })
                    {
                        return Err(invalid(
                            f,
                            "protocol role is unavailable or aliased by another actual argument",
                        ));
                    }
                    let mut low = 0;
                    let mut high = 0;
                    let mut plain = 0;
                    for phase in [
                        ProtocolState::Open,
                        ProtocolState::Closing,
                        ProtocolState::Closed,
                    ] {
                        if before & bit(phase) == 0 {
                            continue;
                        }
                        if !c.entry.contains(&phase) {
                            return Err(invalid(
                                f,
                                "protocol call cannot satisfy every possible entry state",
                            ));
                        }
                        let mut found = false;
                        for row in c.exits.iter().filter(|row| row.input == phase) {
                            found = true;
                            match row.exit {
                                ProtocolExit::ErrorErr | ProtocolExit::PollPending => {
                                    low |= mask(&row.output)
                                }
                                ProtocolExit::ErrorOk | ProtocolExit::PollReady => {
                                    high |= mask(&row.output)
                                }
                                ProtocolExit::Return => plain |= mask(&row.output),
                            }
                        }
                        if !found {
                            return Err(invalid(f, "possible protocol entry has no result row"));
                        }
                    }
                    let all = low | high | plain;
                    if all == 0 && !owned {
                        return Err(invalid(f, "protocol call has no published result relation"));
                    }
                    s.invalidate(r, b)?;
                    s.phases[r] = if owned { 0 } else { all };
                    s.generations[r] = s.generations[r].saturating_add(1);
                    if let Some(o) = out {
                        s.results[o].push(ResultOrigin {
                            root: r,
                            generation: s.generations[r],
                            low,
                            high,
                        });
                    }
                }
            } else {
                for arg in arguments {
                    if let Some(r) = s.value_aliases[arg.0 as usize] {
                        s.invalidate(r, b)?;
                        s.phases[r] = ANY;
                        s.generations[r] = s.generations[r].saturating_add(1);
                    }
                }
                if let Some(o) = out
                    && is_owner(&f.values[o])
                {
                    // No declared/derived return relation: an owner may have
                    // been returned after request_close. Unknown includes
                    // Closing; type/factory shape is not a phase certificate.
                    s.values[o] = ANY;
                }
            }
        }
        InstructionKind::ForeignCall { arguments, .. }
        | InstructionKind::CallIndirect { arguments, .. }
        | InstructionKind::SuspendCall { arguments, .. }
        | InstructionKind::CallDynSlot { arguments, .. }
        | InstructionKind::CallClosure { arguments, .. }
        | InstructionKind::CallClosureMut { arguments, .. }
        | InstructionKind::CallClosureOnce { arguments, .. }
        | InstructionKind::CallClosureBorrowed { arguments, .. }
        | InstructionKind::CallClosureShared { arguments, .. } => {
            for arg in arguments {
                if let Some(r) = s.value_aliases[arg.0 as usize] {
                    s.invalidate(r, b)?;
                    s.phases[r] = ANY;
                    s.generations[r] = s.generations[r].saturating_add(1);
                }
            }
        }
        _ => {}
    }
    products::step(s, i, f, m, b)?;
    Ok(())
}
fn run(
    f: &TypedFunction,
    m: &TypedModule,
    c: Option<&FunctionProtocolContract>,
    b: &mut Budget,
) -> CompileResult<()> {
    if let Some(c) = c {
        shape(f, c, b)?;
    }
    let entries: Vec<_> = c.map_or(vec![None], |c| {
        c.entry
            .iter()
            .map(|s| Some((c.parameter as usize, bit(*s))))
            .collect()
    });
    for entry in entries {
        let mut states = vec![None; f.blocks.len()];
        let mut queue = VecDeque::new();
        let mut queued = vec![false; f.blocks.len()];
        b.spend(
            f.locals
                .len()
                .saturating_add(f.values.len())
                .saturating_mul(16)
                .saturating_add(1),
            "protocol initial flow",
        )?;
        states[f.entry.0 as usize] = Some(Flow::new(f, entry));
        queue.push_back(f.entry.0 as usize);
        queued[f.entry.0 as usize] = true;
        while let Some(index) = queue.pop_front() {
            queued[index] = false;
            b.spend(
                f.locals
                    .len()
                    .saturating_add(f.values.len())
                    .saturating_mul(16)
                    .saturating_add(1),
                "protocol CFG state",
            )?;
            b.spend(
                states[index]
                    .as_ref()
                    .and_then(|s| s.products.as_ref())
                    .map_or(0, |p| p.work()),
                "original input phase CFG snapshot",
            )?;
            b.spend(
                states[index].as_ref().map_or(0, |s| s.relation_work()),
                "phase relation block snapshot",
            )?;
            let mut state = states[index]
                .clone()
                .ok_or_else(|| invalid(f, "missing queued protocol state"))?;
            let block = &f.blocks[index];
            for i in &block.instructions {
                effect(&mut state, i, f, m, b)?;
            }
            b.spend(
                state
                    .relation_work()
                    .saturating_mul(f.protocol_contracts.len()),
                "returned multi-role selector inventory",
            )?;
            if matches!(block.terminator, Terminator::Return(_))
                && let Some(c) = c
            {
                b.spend(
                    c.exits.len().saturating_mul(2).saturating_add(1),
                    "protocol return relation",
                )?;
            }
            if matches!(block.terminator, Terminator::Branch { .. }) {
                b.spend(
                    state
                        .relation_work()
                        .saturating_add(
                            f.locals
                                .len()
                                .saturating_add(f.values.len())
                                .saturating_mul(32),
                        )
                        .saturating_mul(2),
                    "multi-role branch snapshots",
                )?;
            }
            let mut edge = |target: usize, next: &Flow| -> CompileResult<()> {
                b.spend(
                    f.locals
                        .len()
                        .saturating_add(f.values.len())
                        .saturating_mul(32)
                        .saturating_add(1),
                    "protocol edge state",
                )?;
                b.spend(
                    next.products.as_ref().map_or(0, |p| p.work()),
                    "original input phase edge snapshot",
                )?;
                b.spend(next.relation_work(), "phase relation edge snapshot")?;
                let next = next.clone();
                let slot = &mut states[target];
                if let Some(old) = slot {
                    b.spend(
                        old.products
                            .as_ref()
                            .map_or(0, |p| p.work())
                            .saturating_add(next.products.as_ref().map_or(0, |p| p.work())),
                        "original input phase merge snapshot",
                    )?;
                    b.spend(
                        old.relation_work()
                            .saturating_mul(next.relation_work().saturating_add(1)),
                        "phase relation merge",
                    )?;
                    if old.merge(&next) && !queued[target] {
                        queue.push_back(target);
                        queued[target] = true;
                    }
                } else {
                    *slot = Some(next);
                    if !queued[target] {
                        queue.push_back(target);
                        queued[target] = true;
                    }
                }
                Ok(())
            };
            match block.terminator {
                Terminator::Jump(id) => edge(id.0 as usize, &state)?,
                Terminator::Branch {
                    condition,
                    then_block,
                    else_block,
                } => {
                    let rows = &state.conditions[condition.0 as usize];
                    if rows.iter().any(|(r, g, _, _)| state.generations[*r] == *g) {
                        for (target, yes) in [
                            (then_block.0 as usize, true),
                            (else_block.0 as usize, false),
                        ] {
                            let mut selected = state.clone();
                            let mut possible = true;
                            for &(r, g, t, e) in rows {
                                if state.generations[r] == g {
                                    selected.phases[r] &= if yes { t } else { e };
                                    possible &= selected.phases[r] != 0;
                                }
                            }
                            if possible {
                                edge(target, &selected)?;
                            }
                        }
                        continue;
                    }
                    edge(then_block.0 as usize, &state)?;
                    edge(else_block.0 as usize, &state)?;
                }
                Terminator::Return(value) => {
                    if let Some(c) = c {
                        let (_, entry_mask) =
                            entry.ok_or_else(|| invalid(f, "missing protocol entry"))?;
                        let input = [
                            ProtocolState::Open,
                            ProtocolState::Closing,
                            ProtocolState::Closed,
                        ]
                        .into_iter()
                        .find(|s| bit(*s) == entry_mask)
                        .unwrap();
                        let root = f.parameters[c.parameter as usize].0 as usize;
                        let observed = state.phases[root];
                        let check = |exit: ProtocolExit, actual: u8| -> CompileResult<()> {
                            let row = c
                                .exits
                                .iter()
                                .find(|r| r.input == input && r.exit == exit)
                                .ok_or_else(|| {
                                    invalid(f, "reachable protocol result missing upper relation")
                                })?;
                            if actual & !mask(&row.output) != 0 {
                                return Err(invalid(
                                    f,
                                    "actual protocol exit exceeds declared state upper bound",
                                ));
                            }
                            Ok(())
                        };
                        match &f.return_type {
                            Type::ErrorUnion { .. } => {
                                if let Some(tag) =
                                    value.and_then(|v| state.return_tags[v.0 as usize])
                                {
                                    check(tag, observed)?;
                                } else if let Some(p) = value.and_then(|v| {
                                    state.results[v.0 as usize]
                                        .iter()
                                        .find(|p| p.root == root)
                                        .copied()
                                }) {
                                    if p.root != root || p.generation != state.generations[root] {
                                        return Err(invalid(
                                            f,
                                            "returned protocol relation is stale or belongs to another root",
                                        ));
                                    }
                                    check(ProtocolExit::ErrorErr, p.low)?;
                                    check(ProtocolExit::ErrorOk, p.high)?;
                                } else {
                                    return Err(invalid(
                                        f,
                                        "protocol error return lacks actual tag proof",
                                    ));
                                }
                            }
                            Type::Enum { name, .. }
                                if name
                                    == &format!(
                                        "@std.task:v1:Poll:{}",
                                        Type::Unit.fingerprint()
                                    ) =>
                            {
                                let related = value.map(|v| &state.results[v.0 as usize]);
                                if let Some(p) =
                                    related.and_then(|rows| rows.iter().find(|p| p.root == root))
                                {
                                    if p.generation != state.generations[root] {
                                        return Err(invalid(f, "returned Poll relation is stale"));
                                    }
                                    check(ProtocolExit::PollPending, p.low)?;
                                    check(ProtocolExit::PollReady, p.high)?;
                                } else if f.protocol_contracts.len() > 1
                                    && related.is_some_and(|rows| {
                                        rows.iter().any(|p| {
                                            p.generation == state.generations[p.root]
                                                && f.protocol_contracts.iter().any(|c| {
                                                    f.parameters[c.parameter as usize].0 as usize
                                                        == p.root
                                                })
                                        })
                                    })
                                {
                                    check(ProtocolExit::PollPending, observed)?;
                                    check(ProtocolExit::PollReady, observed)?;
                                } else {
                                    return Err(invalid(
                                        f,
                                        "Poll return lacks actual sealed result provenance",
                                    ));
                                }
                            }
                            _ => check(ProtocolExit::Return, observed)?,
                        }
                    }
                }
                Terminator::Trap | Terminator::Unreachable => {}
            }
        }
    }
    Ok(())
}
pub(super) fn check(m: &TypedModule, b: &mut Budget) -> CompileResult<()> {
    let mut outgoing = vec![Vec::new(); m.functions.len()];
    let mut incoming = vec![0usize; m.functions.len()];
    for (index, f) in m.functions.iter().enumerate() {
        if f.protocol_contracts.is_empty() {
            continue;
        }
        for instruction in f.blocks.iter().flat_map(|block| &block.instructions) {
            b.spend(1, "protocol dependency")?;
            if let InstructionKind::Call { function, .. } = &instruction.kind {
                let target = function.0 as usize;
                if m.functions
                    .get(target)
                    .is_some_and(|callee| !callee.protocol_contracts.is_empty())
                {
                    outgoing[index].push(target);
                    incoming[target] = incoming[target].saturating_add(1);
                }
            }
        }
    }
    let mut queue = VecDeque::new();
    for (index, f) in m.functions.iter().enumerate() {
        if !f.protocol_contracts.is_empty() && incoming[index] == 0 {
            queue.push_back(index);
        }
    }
    while let Some(index) = queue.pop_front() {
        b.spend(1, "protocol dependency")?;
        for &target in &outgoing[index] {
            incoming[target] -= 1;
            if incoming[target] == 0 {
                queue.push_back(target);
            }
        }
    }
    if let Some((index, _)) = m
        .functions
        .iter()
        .enumerate()
        .find(|(index, f)| !f.protocol_contracts.is_empty() && incoming[*index] != 0)
    {
        return Err(invalid(
            &m.functions[index],
            "cyclic protocol summaries cannot prove one another",
        ));
    }
    for f in &m.functions {
        b.spend(
            f.blocks
                .iter()
                .map(|block| block.instructions.len())
                .sum::<usize>()
                .saturating_add(f.blocks.len())
                .saturating_add(1),
            "protocol active instruction inventory",
        )?;
        let active = !f.protocol_contracts.is_empty()
            || f.blocks.iter().flat_map(|x| &x.instructions).any(|i| {
                matches!(i.kind, InstructionKind::AsyncRegistrationFinishDestroyChecked { .. } | InstructionKind::TaskCreateOwned{..})
                    || matches!(i.kind, InstructionKind::Call { function, .. }
                        if m.functions.get(function.0 as usize).is_some_and(|callee| !callee.protocol_contracts.is_empty()))
            });
        if active {
            if f.protocol_contracts.is_empty() {
                run(f, m, None, b)?;
            } else {
                if f.protocol_contracts
                    .windows(2)
                    .any(|w| w[0].parameter >= w[1].parameter)
                {
                    return Err(invalid(
                        f,
                        "protocol roles must be unique and ordered by actual parameter",
                    ));
                }
                for contract in &f.protocol_contracts {
                    run(f, m, Some(contract), b)?;
                }
            }
        }
    }
    Ok(())
}
