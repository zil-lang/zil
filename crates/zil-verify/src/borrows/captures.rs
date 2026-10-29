//! Independently reconstructed capture slots and actual description accesses.
//! Public helper `writes` remains a capability upper bound. A concrete entry's
//! checked CFG supplies its actual accesses; no producer proof is consumed.
use super::*;
use std::collections::BTreeSet;

type RoleAccesses = Vec<(Option<ValueId>, Effects)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Source {
    Root(Root),
    InputCapture { parameter: u32, capture: u32 },
}

impl Source {
    pub(super) fn position(self) -> Root {
        match self {
            Self::Root(root) => root,
            // Within a helper the concrete capture is still symbolic. The
            // caller substitutes its own slot, rather than trusting this
            // symbolic capture position as evidence of separation.
            Self::InputCapture { parameter, capture } => Root::ParameterCapture(parameter, capture),
        }
    }

    fn external(self) -> bool {
        matches!(
            self,
            Self::Root(Root::Parameter(_) | Root::Capture(_)) | Self::InputCapture { .. }
        )
    }
}

type Sources = BTreeSet<Source>;
type Slots = BTreeMap<u32, Sources>;
pub(super) type State = BTreeMap<LocalId, Slots>;
pub(super) type Effects = BTreeMap<Source, bool>;

pub(super) struct CaptureFacts {
    pub(super) values: Vec<Slots>,
    pub(super) inputs: Vec<State>,
}

#[derive(Clone, Copy)]
pub(super) struct Context<'a> {
    pub(super) module: &'a TypedModule,
    pub(super) function: &'a TypedFunction,
    pub(super) ordinary: &'a Facts,
    pub(super) facts: &'a CaptureFacts,
    pub(super) effects: &'a [Effects],
}

/// One actual call edge. Its captured slots remain distinct from the incoming
/// environment position and from ordinary argument positions.
pub(super) struct Invocation<'a> {
    pub(super) function: FunctionId,
    arguments: &'a [ValueId],
    environment: Option<&'a Slots>,
    captures: &'a CaptureFacts,
    ordinary: &'a Facts,
    span: Span,
}

impl Invocation<'_> {
    pub(super) fn substitute(&self, root: Root, budget: &mut Budget) -> Check<Roots> {
        let ungrounded = || {
            reject(
                "E-BORROW-ORIGIN",
                Some(self.span),
                "window separation has no independently grounded call source",
            )
        };
        let slot = match root {
            Root::Parameter(parameter) => {
                let value = self
                    .arguments
                    .get(parameter as usize)
                    .ok_or_else(ungrounded)?;
                let roots = &self.ordinary.values[value.0 as usize];
                budget.spend(roots.len())?;
                return Ok(roots.clone());
            }
            Root::Capture(capture) => {
                let slots = self.environment.ok_or_else(ungrounded)?;
                lookup(budget, slots.len())?;
                slots.get(&capture)
            }
            Root::ParameterCapture(parameter, capture) => {
                let value = self
                    .arguments
                    .get(parameter as usize)
                    .ok_or_else(ungrounded)?;
                let slots = &self.captures.values[value.0 as usize];
                lookup(budget, slots.len())?;
                slots.get(&capture)
            }
            Root::Local(_) | Root::Field(..) => return Err(ungrounded()),
        }
        .filter(|roots| !roots.is_empty())
        .ok_or_else(ungrounded)?;
        let mut roots = Vec::new();
        // Source and Root have different ordering domains. Use the existing
        // charged sorted-root join instead of assuming their order coincides.
        for source in slot {
            roots_join(&mut roots, &[source.position()], budget)?;
        }
        Ok(roots)
    }
}

pub(super) fn input_state(facts: &CaptureFacts, block: usize, budget: &mut Budget) -> Check<State> {
    let input = &facts.inputs[block];
    state_cost(input, budget)?;
    Ok(input.clone())
}

fn slots_cost(slots: &Slots, budget: &mut Budget) -> Check<()> {
    budget.spend(slots.len())?;
    for row in slots.values() {
        budget.spend(row.len())?;
    }
    Ok(())
}

fn state_cost(state: &State, budget: &mut Budget) -> Check<()> {
    budget.spend(state.len())?;
    for row in state.values() {
        slots_cost(row, budget)?;
    }
    Ok(())
}

fn join(to: &mut Slots, from: &Slots, budget: &mut Budget) -> Check<bool> {
    let mut changed = false;
    for (capture, roots) in from {
        budget.spend(to.len().saturating_add(roots.len()).saturating_add(1))?;
        let row = to.entry(*capture).or_default();
        budget.spend(row.len())?;
        let before = row.len();
        row.extend(roots);
        changed |= row.len() != before;
    }
    Ok(changed)
}

pub(super) fn local_transfer(
    state: &mut State,
    instruction: &Instruction,
    values: &[Slots],
    budget: &mut Budget,
) -> Check<()> {
    match instruction.kind {
        InstructionKind::Store { local, value } => {
            lookup(budget, state.len())?;
            let row = &values[value.0 as usize];
            if row.is_empty() {
                state.remove(&local);
            } else {
                slots_cost(row, budget)?;
                state.insert(local, row.clone());
            }
        }
        InstructionKind::StorageLive(local)
        | InstructionKind::StorageDead(local)
        | InstructionKind::TakeLocal { local }
        | InstructionKind::DropClosure { local } => {
            lookup(budget, state.len())?;
            state.remove(&local);
        }
        _ => {}
    }
    Ok(())
}

fn tracks(ty: &Type) -> bool {
    matches!(ty, Type::Desc(_) | Type::Address { .. })
}

pub(super) fn reconstruct(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
    ordinary: &Facts,
    budget: &mut Budget,
) -> Check<CaptureFacts> {
    budget.spend(
        f.values
            .len()
            .saturating_add(f.blocks.len().saturating_mul(2)),
    )?;
    let mut values = vec![Slots::new(); f.values.len()];
    let mut inputs = vec![State::new(); f.blocks.len()];
    let mut outputs = vec![State::new(); f.blocks.len()];
    let mut initial = State::new();
    for (parameter, local) in f.parameters.iter().enumerate() {
        budget.spend(1)?;
        if let Some((id, _)) = helpers::parameter_type(&f.locals[local.0 as usize].ty) {
            let mut slots = Slots::new();
            for (capture, ty) in module.closures[id.0 as usize].captures.iter().enumerate() {
                budget.spend(2)?;
                if tracks(ty) {
                    slots.insert(
                        capture as u32,
                        Sources::from([Source::InputCapture {
                            parameter: parameter as u32,
                            capture: capture as u32,
                        }]),
                    );
                }
            }
            if !slots.is_empty() {
                initial.insert(*local, slots);
            }
        }
    }
    loop {
        let mut changed = false;
        for (b, block) in f.blocks.iter().enumerate() {
            budget.spend(1)?;
            if !graph.reachable[b] {
                continue;
            }
            let mut state = State::new();
            if b == f.entry.0 as usize {
                state_cost(&initial, budget)?;
                state.clone_from(&initial);
            }
            for &predecessor in &graph.predecessors[b] {
                budget.spend(1)?;
                if graph.reachable[predecessor] {
                    for (local, row) in &outputs[predecessor] {
                        join(state.entry(*local).or_default(), row, budget)?;
                    }
                }
            }
            state_cost(&state, budget)?;
            inputs[b] = state.clone();
            for instruction in &block.instructions {
                budget.spend(1)?;
                if let Some(result) = instruction.result
                    && helpers::parameter_type(&f.values[result.0 as usize]).is_some()
                {
                    let mut row = Slots::new();
                    match &instruction.kind {
                        InstructionKind::MakeClosure { closure, captures } => {
                            for (index, (value, ty)) in captures
                                .iter()
                                .zip(&module.closures[closure.0 as usize].captures)
                                .enumerate()
                            {
                                budget.spend(1)?;
                                if tracks(ty) {
                                    let roots = &ordinary.values[value.0 as usize];
                                    budget.spend(roots.len())?;
                                    row.insert(
                                        index as u32,
                                        roots.iter().copied().map(Source::Root).collect(),
                                    );
                                }
                            }
                        }
                        InstructionKind::Load(local)
                        | InstructionKind::TakeLocal { local }
                        | InstructionKind::BorrowLocal { local }
                        | InstructionKind::BorrowLocalMut { local } => {
                            lookup(budget, state.len())?;
                            if let Some(source) = state.get(local) {
                                slots_cost(source, budget)?;
                                row.clone_from(source);
                            }
                        }
                        InstructionKind::ReborrowAddress { address, .. } => {
                            let source = &values[address.0 as usize];
                            slots_cost(source, budget)?;
                            row.clone_from(source);
                        }
                        // Factories and retained fields contain independently
                        // checked self-contained environments, so they have no
                        // external Desc/Address slot to invent here.
                        _ => {}
                    }
                    changed |= join(&mut values[result.0 as usize], &row, budget)?;
                }
                local_transfer(&mut state, instruction, &values, budget)?;
            }
            state_cost(&state, budget)?;
            state_cost(&outputs[b], budget)?;
            if outputs[b] != state {
                outputs[b] = state;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    Ok(CaptureFacts { values, inputs })
}

fn add(row: &mut Effects, source: Source, write: bool, budget: &mut Budget) -> Check<bool> {
    budget.spend(row.len().saturating_add(1))?;
    match row.get_mut(&source) {
        Some(old) if write && !*old => {
            *old = true;
            Ok(true)
        }
        Some(_) => Ok(false),
        None => {
            row.insert(source, write);
            Ok(true)
        }
    }
}

fn add_roots(row: &mut Effects, roots: &[Root], write: bool, budget: &mut Budget) -> Check<()> {
    for root in roots {
        add(row, Source::Root(*root), write, budget)?;
    }
    Ok(())
}

fn add_slot(
    row: &mut Effects,
    slots: &Slots,
    capture: u32,
    write: bool,
    span: Span,
    budget: &mut Budget,
) -> Check<()> {
    lookup(budget, slots.len())?;
    let roots = slots
        .get(&capture)
        .filter(|row| !row.is_empty())
        .ok_or_else(|| {
            reject(
                "E-BORROW-ORIGIN",
                Some(span),
                "callback access has no independently grounded capture slot",
            )
        })?;
    for root in roots {
        add(row, *root, write, budget)?;
    }
    Ok(())
}

impl Context<'_> {
    pub(super) fn invocation<'a>(
        &'a self,
        instruction: &'a Instruction,
        locals: &'a State,
        budget: &mut Budget,
    ) -> Check<Option<Invocation<'a>>> {
        let (function, arguments, environment) = match &instruction.kind {
            InstructionKind::Call {
                function,
                arguments,
            } => (*function, arguments, None),
            InstructionKind::CallClosure { callee, arguments }
            | InstructionKind::CallClosureOnce { callee, arguments }
            | InstructionKind::CallClosureBorrowed {
                callee, arguments, ..
            } => {
                let (id, _) = helpers::parameter_type(&self.function.values[callee.0 as usize])
                    .expect("independently checked concrete call type");
                (
                    self.module.closures[id.0 as usize].function,
                    arguments,
                    Some(&self.facts.values[callee.0 as usize]),
                )
            }
            InstructionKind::CallClosureShared { callee, arguments }
            | InstructionKind::CallClosureMut { callee, arguments } => {
                let (id, _) = helpers::parameter_type(&self.function.locals[callee.0 as usize].ty)
                    .expect("independently checked concrete local call type");
                lookup(budget, locals.len())?;
                (
                    self.module.closures[id.0 as usize].function,
                    arguments,
                    locals.get(callee),
                )
            }
            _ => return Ok(None),
        };
        Ok(Some(Invocation {
            function,
            arguments,
            environment,
            captures: self.facts,
            ordinary: self.ordinary,
            span: instruction.span,
        }))
    }

    fn callback(
        &self,
        id: ClosureId,
        slots: &Slots,
        arguments: &[ValueId],
        span: Span,
        budget: &mut Budget,
    ) -> Check<Effects> {
        let (module, ordinary, effects) = (self.module, self.ordinary, self.effects);
        let definition = &module.closures[id.0 as usize];
        let mut row = Effects::new();
        for (source, write) in &effects[definition.function.0 as usize] {
            budget.spend(1)?;
            match *source {
                Source::Root(Root::Capture(capture))
                    if matches!(definition.captures[capture as usize], Type::Desc(_)) =>
                {
                    add_slot(&mut row, slots, capture, *write, span, budget)?;
                }
                Source::Root(Root::Parameter(parameter)) => {
                    let value = arguments.get(parameter as usize).ok_or_else(|| {
                        reject(
                            "E-BORROW-ORIGIN",
                            Some(span),
                            "callback access parameter is absent",
                        )
                    })?;
                    add_roots(&mut row, &ordinary.values[value.0 as usize], *write, budget)?;
                }
                // Owned captures are internal environment positions. Their writes
                // and cleanup retain the independent Fn/resource responsibility
                // checks; they are not external description permissions.
                Source::Root(Root::Capture(_)) => {}
                _ => {
                    return Err(unsupported(
                        "nested external callback effect requires a retained capture contract",
                        Some(span),
                    ));
                }
            }
        }
        Ok(row)
    }

    /// Keep each actual callee input-capture role until the loan checker has
    /// selected that role's own carrier windows. Ordinary argument effects
    /// never inherit any environment-role exemption.
    pub(super) fn call_role_accesses(
        &self,
        instruction: &Instruction,
        budget: &mut Budget,
    ) -> Check<Option<RoleAccesses>> {
        let InstructionKind::Call {
            function,
            arguments,
        } = &instruction.kind
        else {
            return Ok(None);
        };
        let effects = &self.effects[function.0 as usize];
        budget.spend(effects.len())?;
        let mut groups = Vec::with_capacity(effects.len());
        for (source, write) in effects {
            let mut row = Effects::new();
            let role = match *source {
                Source::Root(Root::Parameter(parameter)) => {
                    let value = arguments[parameter as usize];
                    add_roots(
                        &mut row,
                        &self.ordinary.values[value.0 as usize],
                        *write,
                        budget,
                    )?;
                    None
                }
                Source::InputCapture { parameter, capture } => {
                    let value = arguments[parameter as usize];
                    add_slot(
                        &mut row,
                        &self.facts.values[value.0 as usize],
                        capture,
                        *write,
                        instruction.span,
                        budget,
                    )?;
                    Some(value)
                }
                _ => {
                    return Err(reject(
                        "E-BORROW-ORIGIN",
                        Some(instruction.span),
                        "ordinary call has an unbound environment access",
                    ));
                }
            };
            groups.push((role, row));
        }
        Ok(Some(groups))
    }

    pub(super) fn accesses(
        &self,
        instruction: &Instruction,
        locals: &State,
        budget: &mut Budget,
    ) -> Check<Effects> {
        let (f, ordinary, captures, effects) =
            (self.function, self.ordinary, self.facts, self.effects);
        let mut row = Effects::new();
        let empty = Slots::new();
        match &instruction.kind {
            InstructionKind::Call {
                function,
                arguments,
            } => {
                // The independently reconstructed callee CFG includes both its
                // ordinary parameter accesses and actual callback invocations.
                // A public callable contract is an upper bound, not a reason
                // to discard either source class or invent a zero-call effect.
                for (source, write) in &effects[function.0 as usize] {
                    budget.spend(1)?;
                    match *source {
                        Source::Root(Root::Parameter(parameter)) => {
                            let value = arguments[parameter as usize];
                            add_roots(
                                &mut row,
                                &ordinary.values[value.0 as usize],
                                *write,
                                budget,
                            )?;
                        }
                        Source::InputCapture { parameter, capture } => {
                            let value = arguments[parameter as usize];
                            add_slot(
                                &mut row,
                                &captures.values[value.0 as usize],
                                capture,
                                *write,
                                instruction.span,
                                budget,
                            )?;
                        }
                        _ => {
                            return Err(reject(
                                "E-BORROW-ORIGIN",
                                Some(instruction.span),
                                "ordinary call has an unbound environment access",
                            ));
                        }
                    }
                }
            }
            InstructionKind::CallClosure { callee, arguments }
            | InstructionKind::CallClosureOnce { callee, arguments }
            | InstructionKind::CallClosureBorrowed {
                callee, arguments, ..
            } => {
                let (id, _) = helpers::parameter_type(&f.values[callee.0 as usize]).unwrap();
                row = self.callback(
                    id,
                    &captures.values[callee.0 as usize],
                    arguments,
                    instruction.span,
                    budget,
                )?;
            }
            InstructionKind::CallClosureShared { callee, arguments }
            | InstructionKind::CallClosureMut { callee, arguments } => {
                let (id, _) = helpers::parameter_type(&f.locals[callee.0 as usize].ty).unwrap();
                lookup(budget, locals.len())?;
                row = self.callback(
                    id,
                    locals.get(callee).unwrap_or(&empty),
                    arguments,
                    instruction.span,
                    budget,
                )?;
            }
            _ => {}
        }
        Ok(row)
    }
}

pub(super) fn infer(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    ordinary: &[Facts],
    captures: &[CaptureFacts],
    budget: &mut Budget,
) -> Check<Vec<Effects>> {
    budget.spend(module.functions.len())?;
    let mut effects = vec![Effects::new(); module.functions.len()];
    loop {
        let mut changed = false;
        for (index, f) in module.functions.iter().enumerate() {
            budget.spend(1)?;
            for (b, block) in f.blocks.iter().enumerate() {
                budget.spend(1)?;
                if !graphs[index].reachable[b] {
                    continue;
                }
                let input = &captures[index].inputs[b];
                state_cost(input, budget)?;
                let mut locals = input.clone();
                for instruction in &block.instructions {
                    budget.spend(1)?;
                    let mut found = Context {
                        module,
                        function: f,
                        ordinary: &ordinary[index],
                        facts: &captures[index],
                        effects: &effects,
                    }
                    .accesses(instruction, &locals, budget)?;
                    if let Some((value, write)) = buffer_effects::access(&instruction.kind, true) {
                        add_roots(
                            &mut found,
                            &ordinary[index].values[value.0 as usize],
                            write,
                            budget,
                        )?;
                    }
                    for (source, write) in found {
                        budget.spend(1)?;
                        if source.external() {
                            changed |= add(&mut effects[index], source, write, budget)?;
                        }
                    }
                    local_transfer(&mut locals, instruction, &captures[index].values, budget)?;
                }
            }
        }
        if !changed {
            return Ok(effects);
        }
    }
}
