//! Forward construction provenance for migration. Facts retain actual allocation
//! sites and caller substitutions, including already-active input registrations.
use super::*;
#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Fact {
    unknown: bool,
    sites: BTreeSet<(usize, ValueId)>,
}
impl Fact {
    fn join(&mut self, other: &Self, b: &mut Budget) -> CompileResult<bool> {
        b.spend(
            self.sites
                .len()
                .saturating_add(other.sites.len())
                .saturating_add(1),
            "Task migration construction facts",
        )?;
        let old = self.clone();
        self.unknown |= other.unknown;
        self.sites.extend(other.sites.iter().copied());
        Ok(*self != old)
    }
}
struct Activation {
    function: FunctionId,
    inputs: Vec<Fact>,
    values: Vec<Fact>,
    locals: Vec<Fact>,
}
struct Proof<'a> {
    module: &'a TypedModule,
    budget: &'a mut Budget,
    activations: Vec<Activation>,
    solving: BTreeSet<FunctionId>,
    checking: BTreeSet<(usize, ValueId)>,
}
fn relevant(module: &TypedModule, t: &Type, b: &mut Budget) -> CompileResult<bool> {
    b.type_work(t, false, "Task migration actual retained dependency")?;
    let mut work = vec![t];
    let mut seen = BTreeSet::new();
    while let Some(t) = work.pop() {
        b.spend(1, "Task migration actual retained dependency")?;
        match t {
            Type::Task { .. } | Type::AsyncRegistration { .. } => return Ok(true),
            Type::Managed(t)
            | Type::Desc(t)
            | Type::Address { inner: t, .. }
            | Type::Array { element: t, .. } => work.push(t),
            Type::Closure(id) => {
                if seen.insert(*id) {
                    let c = module
                        .closures
                        .get(id.0 as usize)
                        .ok_or_else(|| bad(Default::default(), "Task input closure absent"))?;
                    work.extend(&c.captures);
                }
            }
            Type::Tuple(ts) => work.extend(ts),
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                work.extend(fields.iter().map(|(_, t)| t))
            }
            Type::Enum { variants, .. } => {
                for (_, ts) in variants {
                    work.extend(ts);
                }
            }
            Type::ErrorUnion { error, result } => work.extend([error.as_ref(), result.as_ref()]),
            _ => {}
        }
    }
    Ok(false)
}
impl Proof<'_> {
    fn solve(&mut self, function: FunctionId, inputs: Vec<Fact>) -> CompileResult<usize> {
        self.budget.spend(
            self.activations.len().saturating_add(inputs.len()),
            "Task migration activation",
        )?;
        if let Some(index) = self
            .activations
            .iter()
            .position(|a| a.function == function && a.inputs == inputs)
        {
            return Ok(index);
        }
        let f = &self.module.functions[function.0 as usize];
        if self.solving.len() >= self.budget.helper_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Analyze,
                "Task migration actual-helper depth exceeded",
                Some(f.span),
            )]);
        }
        if !self.solving.insert(function) {
            return Err(bad(
                f.span,
                "recursive Task helper has unresolved actual inputs",
            ));
        }
        let index = self.activations.len();
        self.budget.spend(
            f.values.len().saturating_add(f.locals.len()),
            "Task migration construction tables",
        )?;
        self.activations.push(Activation {
            function,
            inputs,
            values: vec![Fact::default(); f.values.len()],
            locals: vec![Fact::default(); f.locals.len()],
        });
        let mut relevant_values = vec![];
        for t in &f.values {
            relevant_values.push(relevant(self.module, t, self.budget)?);
        }
        for (position, local) in f.parameters.iter().enumerate() {
            if relevant(self.module, &f.locals[local.0 as usize].ty, self.budget)? {
                self.activations[index].locals[local.0 as usize] = self.activations[index]
                    .inputs
                    .get(position)
                    .cloned()
                    .unwrap_or(Fact {
                        unknown: true,
                        ..Fact::default()
                    });
            }
        }
        let live = super::super::drop_sync::reachable_blocks(f, self.budget)?;
        loop {
            let mut changed = false;
            for (bi, block) in f.blocks.iter().enumerate() {
                if !live[bi] {
                    continue;
                }
                for i in &block.instructions {
                    self.budget
                        .spend(1, "Task migration forward construction")?;
                    match &i.kind {
                        InstructionKind::Store { local, value }
                        | InstructionKind::StorePackageField { local, value, .. } => {
                            let source = self.activations[index].values[value.0 as usize].clone();
                            changed |= self.activations[index].locals[local.0 as usize]
                                .join(&source, self.budget)?;
                        }
                        InstructionKind::UnpackTuple { local, fields }
                        | InstructionKind::UnpackVariant { local, fields, .. } => {
                            let source = self.activations[index].locals[local.0 as usize].clone();
                            for field in fields {
                                if relevant(
                                    self.module,
                                    &f.locals[field.0 as usize].ty,
                                    self.budget,
                                )? {
                                    changed |= self.activations[index].locals[field.0 as usize]
                                        .join(&source, self.budget)?;
                                }
                            }
                        }
                        _ => {}
                    }
                    let Some(out) = i.result else {
                        continue;
                    };
                    if !relevant_values[out.0 as usize] {
                        continue;
                    }
                    let mut fact = Fact::default();
                    match &i.kind {
                        InstructionKind::TaskCreate { .. }
                        | InstructionKind::TaskCreateOwned { .. }
                        | InstructionKind::AsyncRegistrationStart { .. } => {
                            fact.sites.insert((index, out));
                        }
                        InstructionKind::Load(local)
                        | InstructionKind::TakeLocal { local }
                        | InstructionKind::BorrowLocal { local }
                        | InstructionKind::BorrowLocalMut { local }
                        | InstructionKind::DescribeLocal { local }
                        | InstructionKind::TakeTupleField { local, .. }
                        | InstructionKind::TakeVariantField { local, .. }
                        | InstructionKind::TakePackageField { local, .. } => {
                            fact = self.activations[index].locals[local.0 as usize].clone()
                        }
                        InstructionKind::TakeErrorUnion { value, .. }
                        | InstructionKind::ReadErrorUnion { value, .. }
                        | InstructionKind::MakeErrorUnion { value, .. }
                        | InstructionKind::ReadVariant { value, .. }
                        | InstructionKind::ReborrowAddress { address: value, .. }
                        | InstructionKind::DescribeAddress { address: value } => {
                            fact = self.activations[index].values[value.0 as usize].clone()
                        }
                        InstructionKind::WithScopedThread { argument, .. } => {
                            fact = self.activations[index].values[argument.0 as usize].clone()
                        }
                        InstructionKind::ExtractField { aggregate, .. } => {
                            fact = self.activations[index].values[aggregate.0 as usize].clone()
                        }
                        InstructionKind::MakeAggregate { fields }
                        | InstructionKind::MakeOwnedTuple { fields }
                        | InstructionKind::MakePackage { fields }
                        | InstructionKind::MakeVariant { fields, .. }
                        | InstructionKind::MakeManaged { fields }
                        | InstructionKind::MakeClosure {
                            captures: fields, ..
                        } => {
                            for field in fields {
                                fact.join(
                                    &self.activations[index].values[field.0 as usize],
                                    self.budget,
                                )?;
                            }
                        }
                        InstructionKind::Call {
                            function,
                            arguments,
                        } => {
                            let target = &self.module.functions[function.0 as usize];
                            if target.foreign.is_some() {
                                fact.unknown = true;
                            } else {
                                let inputs = arguments
                                    .iter()
                                    .map(|v| self.activations[index].values[v.0 as usize].clone())
                                    .collect();
                                let callee = self.solve(*function, inputs)?;
                                let live =
                                    super::super::drop_sync::reachable_blocks(target, self.budget)?;
                                for (bi, block) in target.blocks.iter().enumerate() {
                                    if live[bi]
                                        && let Terminator::Return(Some(v)) = block.terminator
                                    {
                                        fact.join(
                                            &self.activations[callee].values[v.0 as usize],
                                            self.budget,
                                        )?;
                                    }
                                }
                            }
                        }
                        _ => fact.unknown = true,
                    }
                    changed |=
                        self.activations[index].values[out.0 as usize].join(&fact, self.budget)?;
                }
            }
            if !changed {
                break;
            }
        }
        self.solving.remove(&function);
        Ok(index)
    }
    fn facts(&mut self, fact: Fact) -> CompileResult<()> {
        if fact.unknown {
            return Err(bad(
                Default::default(),
                "Task migration retains an unresolved actual constructor",
            ));
        }
        for (activation, value) in fact.sites {
            self.budget.spend(
                self.checking.len().saturating_add(1),
                "Task migration retained frame graph",
            )?;
            if self.checking.len() >= self.budget.helper_depth {
                return Err(vec![Diagnostic::limit(
                    Phase::Analyze,
                    "Task migration actual-frame depth exceeded",
                    None,
                )]);
            }
            if !self.checking.insert((activation, value)) {
                return Err(bad(
                    Default::default(),
                    "cyclic active frame cannot establish finite migration ownership",
                ));
            }
            self.site(activation, value)?;
            self.checking.remove(&(activation, value));
        }
        Ok(())
    }
    fn site(&mut self, activation: usize, value: ValueId) -> CompileResult<()> {
        let f = &self.module.functions[self.activations[activation].function.0 as usize];
        let i = f
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .find(|i| i.result == Some(value))
            .ok_or_else(|| bad(f.span, "Task migration construction site absent"))?;
        match &i.kind {
            InstructionKind::TaskCreate {
                worker, arguments, ..
            }
            | InstructionKind::TaskCreateOwned {
                worker, arguments, ..
            } => {
                graph(self.module, *worker, true, self.budget)?;
                let target = &self.module.functions[worker.0 as usize];
                let mut inputs = vec![];
                for arg in arguments {
                    external_input(&f.values[arg.0 as usize], self.budget, i.span)?;
                    let fact = self.activations[activation].values[arg.0 as usize].clone();
                    self.facts(fact.clone())?;
                    inputs.push(fact);
                }
                let child = self.solve(*worker, inputs)?;
                // Analyze forward provenance intentionally covers all reachable
                // constructed holders. Verify separately selects may-live PC slots.
                for fact in self.activations[child]
                    .values
                    .clone()
                    .into_iter()
                    .chain(self.activations[child].locals.clone())
                {
                    self.facts(fact)?;
                }
                for block in &target.blocks {
                    for i in &block.instructions {
                        if let InstructionKind::SuspendCall {
                            function,
                            arguments,
                        } = &i.kind
                        {
                            graph(self.module, *function, true, self.budget)?;
                            let inputs = arguments
                                .iter()
                                .map(|v| self.activations[child].values[v.0 as usize].clone())
                                .collect();
                            let nested = self.solve(*function, inputs)?;
                            for fact in self.activations[nested].values.clone() {
                                self.facts(fact)?;
                            }
                        }
                    }
                }
            }
            InstructionKind::AsyncRegistrationStart {
                register,
                request_close,
                poll_drain,
                callback,
                ..
            } => {
                for (id, role) in [
                    (*register, ScopedCallbackRole::ThreadAsyncRegister),
                    (*request_close, ScopedCallbackRole::ThreadAsyncRequestClose),
                    (*poll_drain, ScopedCallbackRole::ThreadAsyncPollDrain),
                ] {
                    let d = self
                        .module
                        .functions
                        .get(id.0 as usize)
                        .and_then(|f| f.foreign.as_ref());
                    if !d.is_some_and(|d| {
                        d.static_callbacks
                            && d.thread_driver_transfer
                            && d.scoped_callback == Some(role)
                    }) {
                        return Err(bad(
                            i.span,
                            "retained registration provider lacks actual cross-driver contract",
                        ));
                    }
                }
                let Some(Type::Closure(id)) = f.values.get(callback.0 as usize) else {
                    return Err(bad(i.span, "retained registration callback absent"));
                };
                crate::registration::transfer_callback(self.module, *id, self.budget, i.span)?;
                self.facts(self.activations[activation].values[callback.0 as usize].clone())?;
            }
            _ => {
                return Err(bad(
                    i.span,
                    "Task migration origin is not an actual owning construction",
                ));
            }
        }
        Ok(())
    }
}
pub(super) fn argument(
    module: &TypedModule,
    f: &TypedFunction,
    value: ValueId,
    b: &mut Budget,
) -> CompileResult<()> {
    let mut proof = Proof {
        module,
        budget: b,
        activations: vec![],
        solving: BTreeSet::new(),
        checking: BTreeSet::new(),
    };
    let root = proof.solve(f.id, vec![])?;
    let fact = proof.activations[root].values[value.0 as usize].clone();
    if fact.sites.is_empty() {
        return Err(bad(
            f.span,
            "migrating Task has no actual construction source",
        ));
    }
    proof.facts(fact)
}
