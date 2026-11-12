//! Forward construction provenance for migration. Facts retain actual allocation
//! sites and caller substitutions, including already-active input registrations.
use super::*;
use std::collections::BTreeMap;
type Path = Vec<zil_ir::PlaceProjection>;
#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Fact {
    unknown: bool,
    sites: BTreeMap<Path, BTreeSet<(usize, ValueId)>>,
}
impl Fact {
    fn work(&self) -> usize {
        self.sites.iter().fold(1usize, |work, (path, sites)| {
            work.saturating_add(path.len())
                .saturating_add(sites.len())
                .saturating_add(1)
        })
    }
    fn snapshot(&self, b: &mut Budget) -> CompileResult<Self> {
        b.spend(self.work(), "Task actual-origin fact snapshot")?;
        Ok(self.clone())
    }
    fn join(&mut self, other: &Self, b: &mut Budget) -> CompileResult<bool> {
        b.spend(
            self.work().saturating_mul(2).saturating_add(other.work()),
            "Task migration construction facts",
        )?;
        let old = self.clone();
        self.unknown |= other.unknown;
        for (path, sites) in &other.sites {
            b.spend(
                path.len().saturating_add(sites.len()),
                "Task origin path merge",
            )?;
            self.sites.entry(path.clone()).or_default().extend(sites);
        }
        Ok(*self != old)
    }
    fn prefix(&self, prefix: &[zil_ir::PlaceProjection], b: &mut Budget) -> CompileResult<Self> {
        let mut result = Self {
            unknown: self.unknown,
            ..Self::default()
        };
        for (path, sites) in &self.sites {
            b.spend(
                path.len()
                    .saturating_add(prefix.len())
                    .saturating_add(sites.len()),
                "Task origin path construction",
            )?;
            let mut key = prefix.to_vec();
            key.extend_from_slice(path);
            result.sites.insert(key, sites.clone());
        }
        Ok(result)
    }
    fn select(&self, prefix: &[zil_ir::PlaceProjection], b: &mut Budget) -> CompileResult<Self> {
        let mut result = Self {
            unknown: self.unknown,
            ..Self::default()
        };
        for (path, sites) in &self.sites {
            b.spend(
                path.len()
                    .saturating_add(prefix.len())
                    .saturating_add(sites.len()),
                "Task origin selected path",
            )?;
            if let Some(tail) = path.strip_prefix(prefix) {
                result.sites.insert(tail.to_vec(), sites.clone());
            }
        }
        Ok(result)
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
        for (index, a) in self.activations.iter().enumerate() {
            self.budget.spend(1, "Task activation function match")?;
            if a.function == function {
                self.budget.spend(
                    a.inputs
                        .iter()
                        .chain(&inputs)
                        .fold(0usize, |n, fact| n.saturating_add(fact.work())),
                    "Task actual activation input comparison",
                )?;
                if a.inputs == inputs {
                    return Ok(index);
                }
            }
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
                    .map(|fact| fact.snapshot(self.budget))
                    .transpose()?
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
                        InstructionKind::Store { local, value } => {
                            let source = self.activations[index].values[value.0 as usize]
                                .snapshot(self.budget)?;
                            changed |= self.activations[index].locals[local.0 as usize]
                                .join(&source, self.budget)?;
                        }
                        InstructionKind::StorePackageField {
                            local,
                            index: field,
                            value,
                        } => {
                            let source = self.activations[index].values[value.0 as usize]
                                .prefix(&[zil_ir::PlaceProjection::Field(*field)], self.budget)?;
                            changed |= self.activations[index].locals[local.0 as usize]
                                .join(&source, self.budget)?;
                        }
                        InstructionKind::UnpackTuple { local, fields }
                        | InstructionKind::UnpackVariant { local, fields, .. } => {
                            let source = self.activations[index].locals[local.0 as usize]
                                .snapshot(self.budget)?;
                            for (position, field) in fields.iter().enumerate() {
                                if relevant(
                                    self.module,
                                    &f.locals[field.0 as usize].ty,
                                    self.budget,
                                )? {
                                    changed |= self.activations[index].locals[field.0 as usize]
                                        .join(
                                            &source.select(
                                                &[
                                                    if let InstructionKind::UnpackVariant {
                                                        variant,
                                                        ..
                                                    } = i.kind
                                                    {
                                                        zil_ir::PlaceProjection::Variant(
                                                            zil_ir::VariantProjection {
                                                                variant,
                                                                index: position as u32,
                                                            },
                                                        )
                                                    } else {
                                                        zil_ir::PlaceProjection::Field(
                                                            position as u32,
                                                        )
                                                    },
                                                ],
                                                self.budget,
                                            )?,
                                            self.budget,
                                        )?;
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
                        | InstructionKind::TaskCreateOwned { .. } => {
                            fact.sites.insert(
                                vec![zil_ir::PlaceProjection::ErrorArm(false)],
                                BTreeSet::from([(index, out)]),
                            );
                            if let InstructionKind::TaskCreateOwned { arguments, .. } = &i.kind {
                                for (position, v) in arguments.iter().enumerate() {
                                    let original = self.activations[index].values[v.0 as usize]
                                        .prefix(
                                            &[
                                                zil_ir::PlaceProjection::ErrorArm(true),
                                                zil_ir::PlaceProjection::Field(1),
                                                zil_ir::PlaceProjection::Field(position as u32),
                                            ],
                                            self.budget,
                                        )?;
                                    fact.join(&original, self.budget)?;
                                }
                            }
                        }
                        InstructionKind::AsyncRegistrationStart { .. } => {
                            fact.sites.insert(vec![], BTreeSet::from([(index, out)]));
                        }
                        InstructionKind::Load(local)
                        | InstructionKind::TakeLocal { local }
                        | InstructionKind::BorrowLocal { local }
                        | InstructionKind::BorrowLocalMut { local }
                        | InstructionKind::DescribeLocal { local } => {
                            fact = self.activations[index].locals[local.0 as usize]
                                .snapshot(self.budget)?
                        }
                        InstructionKind::TakeTupleField { local, path }
                        | InstructionKind::TakeVariantField { local, path } => {
                            fact = self.activations[index].locals[local.0 as usize]
                                .select(path, self.budget)?;
                        }
                        InstructionKind::TakePackageField {
                            local,
                            index: field,
                        } => {
                            fact = self.activations[index].locals[local.0 as usize]
                                .select(&[zil_ir::PlaceProjection::Field(*field)], self.budget)?;
                        }
                        InstructionKind::TakeErrorUnion { value, is_error }
                        | InstructionKind::ReadErrorUnion { value, is_error } => {
                            fact = self.activations[index].values[value.0 as usize].select(
                                &[zil_ir::PlaceProjection::ErrorArm(*is_error)],
                                self.budget,
                            )?;
                        }
                        InstructionKind::MakeErrorUnion { value, is_error } => {
                            fact = self.activations[index].values[value.0 as usize].prefix(
                                &[zil_ir::PlaceProjection::ErrorArm(*is_error)],
                                self.budget,
                            )?;
                        }
                        InstructionKind::ReadVariant {
                            value,
                            variant,
                            index: field,
                        } => {
                            fact = self.activations[index].values[value.0 as usize].select(
                                &[zil_ir::PlaceProjection::Variant(
                                    zil_ir::VariantProjection {
                                        variant: *variant,
                                        index: *field,
                                    },
                                )],
                                self.budget,
                            )?;
                        }
                        InstructionKind::ReborrowAddress { address: value, .. }
                        | InstructionKind::DescribeAddress { address: value } => {
                            fact = self.activations[index].values[value.0 as usize]
                                .snapshot(self.budget)?
                        }
                        InstructionKind::WithScopedThread { argument, .. } => {
                            fact = self.activations[index].values[argument.0 as usize].prefix(
                                &[zil_ir::PlaceProjection::Variant(
                                    zil_ir::VariantProjection {
                                        variant: 0,
                                        index: 0,
                                    },
                                )],
                                self.budget,
                            )?;
                        }
                        InstructionKind::ExtractField {
                            aggregate,
                            index: field,
                        } => {
                            fact = self.activations[index].values[aggregate.0 as usize]
                                .select(&[zil_ir::PlaceProjection::Field(*field)], self.budget)?;
                        }
                        InstructionKind::MakeAggregate { fields }
                        | InstructionKind::MakeOwnedTuple { fields }
                        | InstructionKind::MakePackage { fields }
                        | InstructionKind::MakeVariant { fields, .. }
                        | InstructionKind::MakeManaged { fields }
                        | InstructionKind::MakeClosure {
                            captures: fields, ..
                        } => {
                            for (position, field) in fields.iter().enumerate() {
                                let part = if let InstructionKind::MakeVariant { variant, .. } =
                                    i.kind
                                {
                                    zil_ir::PlaceProjection::Variant(zil_ir::VariantProjection {
                                        variant,
                                        index: position as u32,
                                    })
                                } else {
                                    zil_ir::PlaceProjection::Field(position as u32)
                                };
                                let child = self.activations[index].values[field.0 as usize]
                                    .prefix(&[part], self.budget)?;
                                fact.join(&child, self.budget)?;
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
                                    .map(|v| {
                                        self.activations[index].values[v.0 as usize]
                                            .snapshot(self.budget)
                                    })
                                    .collect::<CompileResult<_>>()?;
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
    fn caller_inputs(
        &mut self,
        function: FunctionId,
        visiting: &mut BTreeSet<FunctionId>,
    ) -> CompileResult<Vec<Vec<Fact>>> {
        let target = &self.module.functions[function.0 as usize];
        self.budget.spend(
            visiting.len().saturating_add(1),
            "migration forward caller inventory",
        )?;
        if visiting.len() >= self.budget.helper_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Analyze,
                "migration caller inventory depth exceeded",
                Some(target.span),
            )]);
        }
        if !visiting.insert(function) {
            return Err(bad(
                target.span,
                "migration caller cycle has unresolved actual inputs",
            ));
        }
        if target.parameters.is_empty() {
            visiting.remove(&function);
            return Ok(vec![vec![]]);
        }
        let mut sources = vec![];
        // Inventory executable call sites first. Each is solved forwards with
        // its real incoming facts; a signature never supplies a frame origin.
        for caller in &self.module.functions {
            if caller.foreign.is_some() {
                continue;
            }
            let live = super::super::drop_sync::reachable_blocks(caller, self.budget)?;
            for (bi, block) in caller.blocks.iter().enumerate() {
                if !live[bi] {
                    continue;
                }
                for i in &block.instructions {
                    self.budget.spend(1, "migration forward caller site")?;
                    if let InstructionKind::Call {
                        function: callee,
                        arguments,
                    }
                    | InstructionKind::SuspendCall {
                        function: callee,
                        arguments,
                    } = &i.kind
                        && *callee == function
                    {
                        self.budget.spend(
                            arguments.len().saturating_add(1),
                            "migration caller input snapshot",
                        )?;
                        sources.push((caller.id, arguments.clone()));
                    }
                }
            }
        }
        let mut inputs = vec![];
        for (caller, arguments) in sources {
            for parent_inputs in self.caller_inputs(caller, visiting)? {
                let parent = self.solve(caller, parent_inputs)?;
                self.budget
                    .spend(arguments.len(), "migration actual caller input facts")?;
                inputs.push(
                    arguments
                        .iter()
                        .map(|v| {
                            self.activations[parent].values[v.0 as usize].snapshot(self.budget)
                        })
                        .collect::<CompileResult<_>>()?,
                );
            }
        }
        visiting.remove(&function);
        if inputs.is_empty() {
            return Err(bad(
                target.span,
                "migration helper has no executable caller supplying its frame",
            ));
        }
        Ok(inputs)
    }
    fn facts(&mut self, fact: Fact) -> CompileResult<()> {
        self.budget
            .spend(fact.work(), "Task actual retained site deduplication")?;
        if fact.unknown {
            return Err(bad(
                Default::default(),
                "Task migration retains an unresolved actual constructor",
            ));
        }
        for (activation, value) in fact.sites.into_values().flatten().collect::<BTreeSet<_>>() {
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
                    let fact = self.activations[activation].values[arg.0 as usize]
                        .snapshot(self.budget)?;
                    let checked = fact.snapshot(self.budget)?;
                    self.facts(checked)?;
                    inputs.push(fact);
                }
                let child = self.solve(*worker, inputs)?;
                // Analyze forward provenance intentionally covers all reachable
                // constructed holders. Verify separately selects may-live PC slots.
                let retained = self.activations[child]
                    .values
                    .iter()
                    .chain(&self.activations[child].locals)
                    .map(|fact| fact.snapshot(self.budget))
                    .collect::<CompileResult<Vec<_>>>()?;
                for fact in retained {
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
                                .map(|v| {
                                    self.activations[child].values[v.0 as usize]
                                        .snapshot(self.budget)
                                })
                                .collect::<CompileResult<_>>()?;
                            let nested = self.solve(*function, inputs)?;
                            let retained = self.activations[nested]
                                .values
                                .iter()
                                .map(|fact| fact.snapshot(self.budget))
                                .collect::<CompileResult<Vec<_>>>()?;
                            for fact in retained {
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
                let callback = self.activations[activation].values[callback.0 as usize]
                    .snapshot(self.budget)?;
                self.facts(callback)?;
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
    for inputs in proof.caller_inputs(f.id, &mut BTreeSet::new())? {
        let root = proof.solve(f.id, inputs)?;
        let fact = proof.activations[root].values[value.0 as usize].snapshot(proof.budget)?;
        if fact.sites.is_empty() {
            return Err(bad(
                f.span,
                "migrating Task has no actual construction source",
            ));
        }
        proof.facts(fact)?;
    }
    Ok(())
}
