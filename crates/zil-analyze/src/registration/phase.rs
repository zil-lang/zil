//! Producer proof for the finite pollable registration phase relation.
//! Runtime drain remains mandatory; these facts never establish ownership.
use super::*;
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
    if c.protocol != ProtocolKind::AsyncRegistrationV1
        || c.entry.is_empty()
        || c.entry.windows(2).any(|p| p[0] >= p[1])
        || c.parameter as usize >= f.parameters.len()
    {
        return Err(invalid(f, "invalid finite protocol role or entry"));
    }
    let local = f.parameters[c.parameter as usize];
    let Some(role) = f.locals.get(local.0 as usize) else {
        return Err(invalid(f, "protocol parameter local is absent"));
    };
    if f.foreign.is_some() || f.environment.is_some() || f.call_kind != CallKind::Direct {
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
    if !is_borrow(&role.ty)
        || f.parameters.iter().enumerate().any(|(i, l)| {
            i != c.parameter as usize
                && f.locals
                    .get(l.0 as usize)
                    .is_some_and(|other| is_borrow(&other.ty) || is_owner(&other.ty))
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
            || row.output.is_empty()
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
    phases: Vec<u8>,
    generations: Vec<u32>,
    aliases: Vec<Option<usize>>,
    values: Vec<u8>,
    value_aliases: Vec<Option<usize>>,
    local_results: Vec<Option<ResultOrigin>>,
    results: Vec<Option<ResultOrigin>>,
    local_tags: Vec<Option<usize>>,
    tags: Vec<Option<usize>>,
    local_integers: Vec<Option<i32>>,
    integers: Vec<Option<i32>>,
    local_conditions: Vec<Option<(usize, u32, u8, u8)>>,
    conditions: Vec<Option<(usize, u32, u8, u8)>>,
    local_return_tags: Vec<Option<ProtocolExit>>,
    return_tags: Vec<Option<ProtocolExit>>,
}
impl Flow {
    fn invalidate(&mut self, root: usize, budget: &mut Budget) -> CompileResult<()> {
        let width = self
            .results
            .len()
            .saturating_add(self.local_results.len())
            .saturating_mul(4)
            .saturating_add(1);
        budget.spend(width, "protocol invalidation")?;
        for result in self.results.iter_mut().chain(&mut self.local_results) {
            if result.is_some_and(|origin| origin.root == root) {
                *result = None;
            }
        }
        for condition in self.conditions.iter_mut().chain(&mut self.local_conditions) {
            if condition.is_some_and(|(owner, _, _, _)| owner == root) {
                *condition = None;
            }
        }
        Ok(())
    }
    fn new(f: &TypedFunction, entry: Option<(usize, u8)>) -> Self {
        let n = f.locals.len();
        let v = f.values.len();
        let mut s = Self {
            phases: vec![0; n],
            generations: vec![0; n],
            aliases: vec![None; n],
            values: vec![0; v],
            value_aliases: vec![None; v],
            local_results: vec![None; n],
            results: vec![None; v],
            local_tags: vec![None; n],
            tags: vec![None; v],
            local_integers: vec![None; n],
            integers: vec![None; v],
            local_conditions: vec![None; n],
            conditions: vec![None; v],
            local_return_tags: vec![None; n],
            return_tags: vec![None; v],
        };
        for (i, l) in f.parameters.iter().enumerate() {
            if is_borrow(&f.locals[l.0 as usize].ty) {
                s.aliases[l.0 as usize] = Some(l.0 as usize);
                if entry.map(|(j, _)| j) == Some(i) {
                    s.phases[l.0 as usize] = entry.unwrap().1;
                } else {
                    s.phases[l.0 as usize] = ANY;
                }
            }
        }
        s
    }
    fn merge(&mut self, other: &Self) -> bool {
        let old = self.clone();
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
            if *a != *b {
                *a = None;
            }
        }
        for (a, b) in self.results.iter_mut().zip(&other.results) {
            if *a != *b {
                *a = None;
            }
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
            if *a != *b {
                *a = None;
            }
        }
        for (a, b) in self.conditions.iter_mut().zip(&other.conditions) {
            if *a != *b {
                *a = None;
            }
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
    b.spend(16, "protocol instruction")?;
    let out = i.result.map(|v| v.0 as usize);
    match &i.kind {
        InstructionKind::Load(l) => {
            if let Some(o) = out {
                s.value_aliases[o] = s.aliases[l.0 as usize];
                s.values[o] = s.phases[l.0 as usize];
                s.results[o] = s.local_results[l.0 as usize];
                s.tags[o] = s.local_tags[l.0 as usize];
                s.integers[o] = s.local_integers[l.0 as usize];
                s.conditions[o] = s.local_conditions[l.0 as usize];
                s.return_tags[o] = s.local_return_tags[l.0 as usize];
            }
        }
        InstructionKind::BorrowLocalMut { local } => {
            if let Some(o) = out {
                s.value_aliases[o] = Some(local.0 as usize);
                s.results[o] = s.local_results[local.0 as usize];
                s.tags[o] = s.local_tags[local.0 as usize];
                s.integers[o] = s.local_integers[local.0 as usize];
                s.conditions[o] = s.local_conditions[local.0 as usize];
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
            s.local_results[l] = s.results[value.0 as usize];
            s.local_tags[l] = s.tags[value.0 as usize];
            s.local_integers[l] = s.integers[value.0 as usize];
            s.local_conditions[l] = s.conditions[value.0 as usize];
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
                s.results[o] = Some(ResultOrigin {
                    root: r,
                    generation: s.generations[r],
                    low: pending,
                    high: CLOSED,
                });
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
                if let Some(p) = s.results[value.0 as usize] {
                    s.conditions[o] = Some((p.root, p.generation, p.low, p.high));
                }
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
                    if let Some(p) = s.results[v] {
                        s.conditions[o] = Some((
                            p.root,
                            p.generation,
                            if n == 0 { p.low } else { p.high },
                            if n == 0 { p.high } else { p.low },
                        ));
                    }
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
            if let Some(c) = &callee.protocol_contract {
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
                let before = s.phases[r];
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
                if all == 0 {
                    return Err(invalid(f, "protocol call has no published result relation"));
                }
                s.invalidate(r, b)?;
                s.phases[r] = all;
                s.generations[r] = s.generations[r].saturating_add(1);
                if let Some(o) = out {
                    s.results[o] = Some(ResultOrigin {
                        root: r,
                        generation: s.generations[r],
                        low,
                        high,
                    });
                }
            } else {
                for arg in arguments {
                    if let Some(r) = s.value_aliases[arg.0 as usize] {
                        s.invalidate(r, b)?;
                        s.phases[r] = ANY;
                        s.generations[r] = s.generations[r].saturating_add(1);
                    }
                }
                if let Some(o) = out {
                    if is_owner(&f.values[o]) {
                        s.values[o] = OPEN | CLOSED;
                    }
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
            let mut state = states[index]
                .clone()
                .ok_or_else(|| invalid(f, "missing queued protocol state"))?;
            let block = &f.blocks[index];
            for i in &block.instructions {
                effect(&mut state, i, f, m, b)?;
            }
            if matches!(block.terminator, Terminator::Return(_)) {
                if let Some(c) = c {
                    b.spend(
                        c.exits.len().saturating_mul(2).saturating_add(1),
                        "protocol return relation",
                    )?;
                }
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
                let next = next.clone();
                let slot = &mut states[target];
                if let Some(old) = slot {
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
                    if let Some((r, g, t, e)) = state.conditions[condition.0 as usize] {
                        if state.generations[r] == g {
                            let original = state.phases[r];
                            state.phases[r] &= t;
                            if state.phases[r] != 0 {
                                edge(then_block.0 as usize, &state)?;
                            }
                            state.phases[r] = original & e;
                            if state.phases[r] != 0 {
                                edge(else_block.0 as usize, &state)?;
                            }
                            continue;
                        }
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
                            if actual == 0 {
                                return Ok(());
                            }
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
                                } else if let Some(p) =
                                    value.and_then(|v| state.results[v.0 as usize])
                                {
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
                                let p = value
                                    .and_then(|v| state.results[v.0 as usize])
                                    .ok_or_else(|| {
                                        invalid(
                                            f,
                                            "Poll return lacks actual sealed result provenance",
                                        )
                                    })?;
                                if p.root != root || p.generation != state.generations[root] {
                                    return Err(invalid(
                                        f,
                                        "returned Poll relation is stale or belongs to another root",
                                    ));
                                }
                                check(ProtocolExit::PollPending, p.low)?;
                                check(ProtocolExit::PollReady, p.high)?;
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
        if f.protocol_contract.is_none() {
            continue;
        }
        for instruction in f.blocks.iter().flat_map(|block| &block.instructions) {
            b.spend(1, "protocol dependency")?;
            if let InstructionKind::Call { function, .. } = &instruction.kind {
                let target = function.0 as usize;
                if m.functions
                    .get(target)
                    .is_some_and(|callee| callee.protocol_contract.is_some())
                {
                    outgoing[index].push(target);
                    incoming[target] = incoming[target].saturating_add(1);
                }
            }
        }
    }
    let mut queue = VecDeque::new();
    for (index, f) in m.functions.iter().enumerate() {
        if f.protocol_contract.is_some() && incoming[index] == 0 {
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
        .find(|(index, f)| f.protocol_contract.is_some() && incoming[*index] != 0)
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
        let active = f.protocol_contract.is_some()
            || f.blocks.iter().flat_map(|x| &x.instructions).any(|i| {
                matches!(i.kind, InstructionKind::AsyncRegistrationFinishDestroyChecked { .. })
                    || matches!(i.kind, InstructionKind::Call { function, .. }
                        if m.functions.get(function.0 as usize).is_some_and(|callee| callee.protocol_contract.is_some()))
            });
        if active {
            run(f, m, f.protocol_contract.as_ref(), b)?;
        }
    }
    Ok(())
}
