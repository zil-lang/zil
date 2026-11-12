//! Verify's backwards constructor proof. A type states obligations; executable
//! definitions discharge them. Calls bind actual inputs before following returns.
//! No Analyze origin, producer migration bit or result type supplies an origin.
use super::*;
use std::rc::Rc;
#[derive(Clone)]
struct Activation {
    function: FunctionId,
    arguments: Vec<Endpoint>,
}
#[derive(Clone)]
struct Endpoint {
    activation: Rc<Activation>,
    slot: Slot,
    path: Vec<zil_ir::PlaceProjection>,
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Slot {
    Value(ValueId),
    Local(LocalId),
}
struct Proof<'a> {
    module: &'a TypedModule,
    budget: &'a mut Budget,
    frames: BTreeSet<FunctionId>,
    definitions: BTreeSet<(FunctionId, Slot, Vec<zil_ir::PlaceProjection>)>,
}
impl Endpoint {
    fn value(activation: &Rc<Activation>, value: ValueId) -> Self {
        Self {
            activation: activation.clone(),
            slot: Slot::Value(value),
            path: vec![],
        }
    }
    fn projected(&self, slot: Slot, prefix: &[zil_ir::PlaceProjection]) -> Self {
        let mut path = prefix.to_vec();
        path.extend_from_slice(&self.path);
        Self {
            activation: self.activation.clone(),
            slot,
            path,
        }
    }
}
fn needs_origin(module: &TypedModule, t: &Type, b: &mut Budget) -> Check<bool> {
    resources::charge_payload(t, b)?;
    let mut work = vec![t];
    let mut closures = BTreeSet::new();
    while let Some(t) = work.pop() {
        b.spend(1)?;
        match t {
            Type::Task { .. } | Type::AsyncRegistration { .. } => return Ok(true),
            Type::Closure(id) => {
                if closures.insert(*id) {
                    let c = module
                        .closures
                        .get(id.0 as usize)
                        .ok_or_else(|| bad(Span::default(), "migration closure absent"))?;
                    b.spend(c.captures.len())?;
                    work.extend(&c.captures);
                }
            }
            Type::Managed(t)
            | Type::Desc(t)
            | Type::Address { inner: t, .. }
            | Type::Array { element: t, .. } => work.push(t),
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                b.spend(fields.len())?;
                work.extend(fields.iter().map(|(_, t)| t));
            }
            Type::Tuple(ts) => {
                b.spend(ts.len())?;
                work.extend(ts);
            }
            Type::Enum { variants, .. } => {
                b.spend(variants.len())?;
                for (_, ts) in variants {
                    b.spend(ts.len())?;
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
    fn obligation(&mut self, e: Endpoint) -> Check<()> {
        let f = &self.module.functions[e.activation.function.0 as usize];
        let mut t = match e.slot {
            Slot::Local(l) => &f.locals[l.0 as usize].ty,
            Slot::Value(v) => &f.values[v.0 as usize],
        };
        for projection in &e.path {
            if let Type::Managed(inner) = t {
                t = inner;
            }
            t = match (projection, t) {
                (zil_ir::PlaceProjection::Field(index), Type::Tuple(fields)) => {
                    fields.get(*index as usize)
                }
                (
                    zil_ir::PlaceProjection::Field(index),
                    Type::Record { fields, .. } | Type::Package { fields, .. },
                ) => fields.get(*index as usize).map(|(_, t)| t),
                (zil_ir::PlaceProjection::Variant(p), Type::Enum { variants, .. }) => variants
                    .get(p.variant as usize)
                    .and_then(|(_, ts)| ts.get(p.index as usize)),
                (
                    zil_ir::PlaceProjection::ErrorArm(error_arm),
                    Type::ErrorUnion { error, result },
                ) => Some(if *error_arm {
                    error.as_ref()
                } else {
                    result.as_ref()
                }),
                _ => None,
            }
            .ok_or_else(|| {
                bad(
                    f.span,
                    "migration selected slot has no matching actual type path",
                )
            })?;
        }
        // Data/cleanup qualification remains separate from origin resolution.
        let mut drops = vec![];
        data(self.module, t, self.budget, &mut drops, f.span)?;
        for drop in drops {
            graph(self.module, drop, false, self.budget)?;
        }
        if needs_origin(self.module, t, self.budget)? {
            self.origin(e)?;
        }
        Ok(())
    }
    fn origin(&mut self, e: Endpoint) -> Check<()> {
        self.budget.spend(
            self.definitions
                .len()
                .saturating_add(e.path.len())
                .saturating_add(1),
        )?;
        if self.definitions.len() >= self.budget.limits.max_call_depth {
            return Err(limit(
                "Task migration actual-origin dependency depth exceeded",
            ));
        }
        let key = (e.activation.function, e.slot, e.path.clone());
        if !self.definitions.insert(key.clone()) {
            return Err(bad(
                Span::default(),
                "migration dependency has a cyclic or unresolved actual origin",
            ));
        }
        let result = self.definition(e);
        self.definitions.remove(&key);
        result
    }
    fn definition(&mut self, e: Endpoint) -> Check<()> {
        let f = &self.module.functions[e.activation.function.0 as usize];
        let reachable = super::super::drop_sync::live_blocks(f, self.budget)?;
        match e.slot {
            Slot::Local(local) => {
                if let Some(index) = f.parameters.iter().position(|p| *p == local) {
                    let actual = e.activation.arguments.get(index).ok_or_else(|| {
                        bad(
                            f.span,
                            "migration parameter lacks actual caller substitution",
                        )
                    })?;
                    let mut actual = actual.clone();
                    actual.path.extend(e.path);
                    return self.origin(actual);
                }
                let mut sources = vec![];
                for (bi, block) in f.blocks.iter().enumerate() {
                    if !reachable[bi] {
                        continue;
                    }
                    for instruction in &block.instructions {
                        self.budget.spend(1)?;
                        match &instruction.kind {
                            InstructionKind::Store {
                                local: target,
                                value,
                            } if *target == local => {
                                sources.push(e.projected(Slot::Value(*value), &[]))
                            }
                            InstructionKind::UnpackVariant {
                                local: source,
                                variant,
                                fields,
                            } => {
                                if let Some(index) = fields.iter().position(|l| *l == local) {
                                    sources.push(e.projected(
                                        Slot::Local(*source),
                                        &[zil_ir::PlaceProjection::Variant(
                                            zil_ir::VariantProjection {
                                                variant: *variant,
                                                index: index as u32,
                                            },
                                        )],
                                    ));
                                }
                            }
                            InstructionKind::UnpackTuple {
                                local: source,
                                fields,
                            } => {
                                if let Some(index) = fields.iter().position(|l| *l == local) {
                                    sources.push(e.projected(
                                        Slot::Local(*source),
                                        &[zil_ir::PlaceProjection::Field(index as u32)],
                                    ));
                                }
                            }
                            _ => {}
                        }
                    }
                }
                if sources.is_empty() {
                    return Err(bad(
                        f.span,
                        "migration local has no complete executable origin",
                    ));
                }
                for source in sources {
                    self.origin(source)?;
                }
            }
            Slot::Value(value) => {
                let mut found = None;
                for (bi, block) in f.blocks.iter().enumerate() {
                    if !reachable[bi] {
                        continue;
                    }
                    for i in &block.instructions {
                        self.budget.spend(1)?;
                        if i.result == Some(value) && found.replace(i).is_some() {
                            return Err(bad(
                                i.span,
                                "migration value has duplicate executable definitions",
                            ));
                        }
                    }
                }
                let i = found
                    .ok_or_else(|| bad(f.span, "migration value has no actual constructor"))?;
                match &i.kind {
                    InstructionKind::TaskCreate {
                        worker, arguments, ..
                    }
                    | InstructionKind::TaskCreateOwned {
                        worker, arguments, ..
                    } => {
                        // Only the success arm contains a frame; failure is an empty debt.
                        if matches!(
                            e.path.first(),
                            Some(zil_ir::PlaceProjection::ErrorArm(true))
                        ) {
                            return Ok(());
                        }
                        let activation = Rc::new(Activation {
                            function: *worker,
                            arguments: arguments
                                .iter()
                                .map(|v| Endpoint::value(&e.activation, *v))
                                .collect(),
                        });
                        self.frame(activation)?;
                    }
                    InstructionKind::AsyncRegistrationStart {
                        register,
                        request_close,
                        poll_drain,
                        callback,
                        ..
                    } => {
                        // Each actual provider role independently promises cross-driver
                        // request/close/no-future/drain; callback safety is insufficient.
                        for (id, role) in [
                            (*register, ScopedCallbackRole::ThreadAsyncRegister),
                            (*request_close, ScopedCallbackRole::ThreadAsyncRequestClose),
                            (*poll_drain, ScopedCallbackRole::ThreadAsyncPollDrain),
                        ] {
                            let actual = self
                                .module
                                .functions
                                .get(id.0 as usize)
                                .and_then(|f| f.foreign.as_ref());
                            if !actual.is_some_and(|d| {
                                d.static_callbacks
                                    && d.thread_driver_transfer
                                    && d.scoped_callback == Some(role)
                            }) {
                                return Err(bad(
                                    i.span,
                                    "actual registration provider has no exclusive cross-driver migration contract",
                                ));
                            }
                        }
                        let Some(Type::Closure(id)) = f.values.get(callback.0 as usize) else {
                            return Err(bad(i.span, "actual registration callback missing"));
                        };
                        crate::registration::transfer_callback(
                            self.module,
                            *id,
                            Some(i.span),
                            self.budget,
                        )?;
                        self.obligation(Endpoint::value(&e.activation, *callback))?;
                    }
                    InstructionKind::Load(local)
                    | InstructionKind::TakeLocal { local }
                    | InstructionKind::DescribeLocal { local }
                    | InstructionKind::BorrowLocal { local }
                    | InstructionKind::BorrowLocalMut { local } => {
                        self.origin(e.projected(Slot::Local(*local), &[]))?
                    }
                    InstructionKind::TakeTupleField { local, path }
                    | InstructionKind::TakeVariantField { local, path } => {
                        self.origin(e.projected(Slot::Local(*local), path))?
                    }
                    InstructionKind::ExtractField { aggregate, index } => {
                        self.origin(e.projected(
                            Slot::Value(*aggregate),
                            &[zil_ir::PlaceProjection::Field(*index)],
                        ))?
                    }
                    InstructionKind::ReadVariant {
                        value,
                        variant,
                        index,
                    } => self.origin(e.projected(
                        Slot::Value(*value),
                        &[zil_ir::PlaceProjection::Variant(
                            zil_ir::VariantProjection {
                                variant: *variant,
                                index: *index,
                            },
                        )],
                    ))?,
                    InstructionKind::TakeErrorUnion { value, is_error }
                    | InstructionKind::ReadErrorUnion { value, is_error } => {
                        self.origin(e.projected(
                            Slot::Value(*value),
                            &[zil_ir::PlaceProjection::ErrorArm(*is_error)],
                        ))?
                    }
                    InstructionKind::MakeErrorUnion { value, is_error } => {
                        if let Some(zil_ir::PlaceProjection::ErrorArm(arm)) = e.path.first() {
                            if arm != is_error {
                                return Ok(());
                            }
                            let mut source = e.projected(Slot::Value(*value), &[]);
                            source.path.remove(0);
                            self.origin(source)?;
                        } else {
                            self.obligation(Endpoint::value(&e.activation, *value))?;
                        }
                    }
                    InstructionKind::MakeManaged { fields } => self.fields(&e, fields, None)?,
                    InstructionKind::MakeAggregate { fields }
                    | InstructionKind::MakeOwnedTuple { fields }
                    | InstructionKind::MakePackage { fields } => self.fields(&e, fields, None)?,
                    InstructionKind::MakeVariant { variant, fields } => {
                        self.fields(&e, fields, Some(*variant))?
                    }
                    InstructionKind::MakeClosure { captures, .. } => {
                        for capture in captures {
                            self.obligation(Endpoint::value(&e.activation, *capture))?;
                        }
                    }
                    InstructionKind::Call {
                        function,
                        arguments,
                    } => {
                        let target = &self.module.functions[function.0 as usize];
                        if target.foreign.is_some() {
                            return Err(bad(
                                i.span,
                                "foreign result supplies no actual migration origin",
                            ));
                        }
                        let activation = Rc::new(Activation {
                            function: *function,
                            arguments: arguments
                                .iter()
                                .map(|v| Endpoint::value(&e.activation, *v))
                                .collect(),
                        });
                        let live = super::super::drop_sync::live_blocks(target, self.budget)?;
                        let mut found = false;
                        for (bi, block) in target.blocks.iter().enumerate() {
                            if live[bi]
                                && let Terminator::Return(Some(value)) = block.terminator
                            {
                                found = true;
                                let mut source = Endpoint::value(&activation, value);
                                source.path = e.path.clone();
                                self.origin(source)?;
                            }
                        }
                        if !found {
                            return Err(bad(
                                i.span,
                                "migration helper has no actual returned origin",
                            ));
                        }
                    }
                    InstructionKind::WithScopedThread { argument, .. } => {
                        // Only Failed contains the exact unpolled original owner.
                        if let Some(zil_ir::PlaceProjection::Variant(p)) = e.path.first() {
                            if p.variant == 1 {
                                return Ok(());
                            }
                            if p.variant != 0 || p.index != 0 {
                                return Err(bad(i.span, "invalid Task failure owner projection"));
                            }
                            let mut source = e.projected(Slot::Value(*argument), &[]);
                            source.path.remove(0);
                            self.origin(source)?;
                        } else {
                            self.origin(e.projected(Slot::Value(*argument), &[]))?;
                        }
                    }
                    _ => {
                        return Err(bad(
                            i.span,
                            "migration authority lacks a supported actual constructor/projection",
                        ));
                    }
                }
            }
        }
        Ok(())
    }
    fn fields(&mut self, e: &Endpoint, fields: &[ValueId], variant: Option<u32>) -> Check<()> {
        self.budget
            .spend(fields.len().saturating_add(e.path.len()))?;
        if let Some(first) = e.path.first() {
            let index = match (first, variant) {
                (zil_ir::PlaceProjection::Field(index), None) => *index,
                (zil_ir::PlaceProjection::Variant(p), Some(v)) if p.variant == v => p.index,
                (zil_ir::PlaceProjection::Variant(_), Some(_)) => return Ok(()),
                _ => {
                    return Err(bad(
                        Span::default(),
                        "migration projection differs from actual aggregate constructor",
                    ));
                }
            };
            let value = *fields
                .get(index as usize)
                .ok_or_else(|| bad(Span::default(), "migration aggregate selected field absent"))?;
            let mut child = Endpoint::value(&e.activation, value);
            child.path = e.path[1..].to_vec();
            self.obligation(child)
        } else {
            for field in fields {
                self.obligation(Endpoint::value(&e.activation, *field))?;
            }
            Ok(())
        }
    }
    fn frame(&mut self, a: Rc<Activation>) -> Check<()> {
        self.budget.spend(self.frames.len().saturating_add(1))?;
        if !self.frames.insert(a.function) {
            return Err(bad(
                Span::default(),
                "recursive active Task frame has no finite migration dependency proof",
            ));
        }
        let f = &self.module.functions[a.function.0 as usize];
        graph(self.module, f.id, true, self.budget)?;
        // PC0 includes every actual input, including Copy-shaped hidden holders.
        for input in &a.arguments {
            let parent = &self.module.functions[input.activation.function.0 as usize];
            let t = match input.slot {
                Slot::Local(l) => &parent.locals[l.0 as usize].ty,
                Slot::Value(v) => &parent.values[v.0 as usize],
            };
            external(t, self.budget, parent.span)?;
            self.obligation(input.clone())?;
        }
        // Each may-live slot is supplied by Verify's own ownership transfer,
        // covering every reachable suspend/cancel PC, child and prepared result.
        let mut slots = BTreeSet::new();
        let cost = self
            .budget
            .task_ownership
            .get(&f.id)
            .map_or(0, |p| p.owners().len().saturating_add(p.boundaries().len()));
        self.budget.spend(cost)?;
        if let Some(ownership) = self.budget.task_ownership.get(&f.id) {
            for index in ownership.entry().iter().copied().chain(
                ownership
                    .boundaries()
                    .iter()
                    .flat_map(|pc| pc.may_live().iter().copied()),
            ) {
                let owner = ownership.owners().get(index).ok_or_else(|| {
                    bad(f.span, "Verify frame PC references absent responsibility")
                })?;
                let (slot, path) = match owner.slot() {
                    TaskOwnerSlot::Local(l) | TaskOwnerSlot::LocalShell(l) => {
                        (Slot::Local(l), vec![])
                    }
                    TaskOwnerSlot::LocalField { local, index } => (
                        Slot::Local(local),
                        vec![zil_ir::PlaceProjection::Field(index)],
                    ),
                    TaskOwnerSlot::Value(v) => (Slot::Value(v), vec![]),
                };
                slots.insert((slot, path));
            }
        }
        for (slot, path) in slots {
            self.obligation(Endpoint {
                activation: a.clone(),
                slot,
                path,
            })?;
        }
        // A direct child may have only Copy slots and hence no debt metadata.
        // Its activation still gets real argument substitutions at its call site.
        let live = super::super::drop_sync::live_blocks(f, self.budget)?;
        for (bi, block) in f.blocks.iter().enumerate() {
            if !live[bi] {
                continue;
            }
            if let Terminator::Return(Some(value)) = block.terminator {
                self.obligation(Endpoint::value(&a, value))?;
            }
            for i in &block.instructions {
                self.budget.spend(1)?;
                if let InstructionKind::SuspendCall {
                    function,
                    arguments,
                } = &i.kind
                {
                    self.frame(Rc::new(Activation {
                        function: *function,
                        arguments: arguments.iter().map(|v| Endpoint::value(&a, *v)).collect(),
                    }))?;
                }
            }
        }
        self.frames.remove(&a.function);
        Ok(())
    }
}
fn callers(
    module: &TypedModule,
    function: FunctionId,
    active: &mut BTreeSet<FunctionId>,
    budget: &mut Budget,
) -> Check<Vec<Rc<Activation>>> {
    budget.spend(active.len().saturating_add(1))?;
    if active.len() >= budget.limits.max_call_depth {
        return Err(limit("Task migration caller substitution depth exceeded"));
    }
    if !active.insert(function) {
        return Err(bad(
            Span::default(),
            "migration helper caller cycle has no complete actual substitutions",
        ));
    }
    let target = &module.functions[function.0 as usize];
    if target.parameters.is_empty() {
        active.remove(&function);
        return Ok(vec![Rc::new(Activation {
            function,
            arguments: vec![],
        })]);
    }
    let mut result = vec![];
    for f in &module.functions {
        if f.foreign.is_some() {
            continue;
        }
        let live = super::super::drop_sync::live_blocks(f, budget)?;
        for (bi, block) in f.blocks.iter().enumerate() {
            if !live[bi] {
                continue;
            }
            for i in &block.instructions {
                budget.spend(1)?;
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
                    for parent in callers(module, f.id, active, budget)? {
                        budget.spend(arguments.len().saturating_add(1))?;
                        result.push(Rc::new(Activation {
                            function,
                            arguments: arguments
                                .iter()
                                .map(|v| Endpoint::value(&parent, *v))
                                .collect(),
                        }));
                    }
                }
            }
        }
    }
    active.remove(&function);
    if result.is_empty() {
        return Err(bad(
            target.span,
            "migration helper has no actual caller/constructor for its input frame",
        ));
    }
    Ok(result)
}
pub(super) fn finish(module: &TypedModule, budget: &mut Budget) -> Check<()> {
    if !module.functions.iter().any(|f|f.blocks.iter().any(|block|block.instructions.iter().any(|i|matches!(i.kind,InstructionKind::WithScopedThread{argument,..} if f.values.get(argument.0 as usize).is_some_and(task_owner))))) {return Ok(());}
    for f in &module.functions {
        if f.foreign.is_some() {
            continue;
        }
        let live = super::super::drop_sync::live_blocks(f, budget)?;
        for (bi, block) in f.blocks.iter().enumerate() {
            if !live[bi] {
                continue;
            }
            for instruction in &block.instructions {
                if let InstructionKind::WithScopedThread {
                    argument, worker, ..
                } = instruction.kind
                    && f.values.get(argument.0 as usize).is_some_and(task_owner)
                {
                    for activation in callers(module, f.id, &mut BTreeSet::new(), budget)? {
                        let mut proof = Proof {
                            module,
                            budget,
                            frames: BTreeSet::new(),
                            definitions: BTreeSet::new(),
                        };
                        proof.origin(Endpoint::value(&activation, argument))?;
                        graph(module, worker, false, proof.budget)?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn external(t: &Type, b: &mut Budget, span: Span) -> Check<()> {
    resources::charge_payload(t, b)?;
    let mut pending = vec![t];
    while let Some(t) = pending.pop() {
        b.spend(1)?;
        match t {
            Type::Desc(_) => {
                return Err(bad(
                    span,
                    "external weak descriptor has no retained storage loan for migration",
                ));
            }
            Type::Managed(t) | Type::Address { inner: t, .. } | Type::Array { element: t, .. } => {
                pending.push(t)
            }
            Type::Tuple(ts) => pending.extend(ts),
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                pending.extend(fields.iter().map(|(_, t)| t))
            }
            Type::Enum { variants, .. } => {
                for (_, ts) in variants {
                    pending.extend(ts);
                }
            }
            Type::ErrorUnion { error, result } => pending.extend([error.as_ref(), result.as_ref()]),
            _ => {}
        }
    }
    Ok(())
}
