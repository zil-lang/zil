//! Universal source protocol proof. Claims are modular upper relations, not
//! facts about a concrete specialization. All original bodies are replayed
//! before any checked original result can leave the verifier.
use super::*;
mod expressions;
mod state;
mod tasks;
mod walk;
use intrinsics::Intrinsic;
use state::*;

struct Engine<'a> {
    original: &'a CheckedOriginal,
    module: &'a TypedModule,
    index: &'a index::Index<'a>,
    current: SemanticDefinitionId,
    contracts: Vec<FunctionProtocolContract>,
    inputs: BTreeMap<u32, ProtocolState>,
    poll: bool,
    next: u64,
    closed_tasks: BTreeSet<Place>,
    async_tasks: BTreeSet<Place>,
    error_unwind: Vec<&'a SemanticOriginalBlock>,
    task_loop_keys: Vec<BTreeSet<Binder>>,
}

impl CheckedOriginal {
    pub(super) fn check_protocols<'a>(
        &mut self,
        module: &'a TypedModule,
        context: &CheckedContext,
        index: &'a index::Index<'a>,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<()> {
        for (&id, function) in &index.definitions {
            b.spend(1)?;
            let SemanticDefinitionBody::FunctionWithBody {
                protocol_contracts,
                body,
                ..
            } = &function.definition.body
            else {
                return Err(bad("original protocol body inventory is incomplete"));
            };
            let prototype = self.prototypes.get(&id).cloned().ok_or_else(|| {
                bad("original protocol replay has no universally checked signature")
            })?;
            b.spend(prototype.parameters.len())?;
            let mut parameter_shapes = Vec::new();
            for ty in &prototype.parameters {
                parameter_shapes.push(self.task_shape(module, context, ty, false, active, b, 0)?);
            }
            let free = self.free_values.get(&id).cloned().unwrap_or_default();
            b.spend(free.len())?;
            let mut free_shapes = Vec::new();
            for binding in free.values() {
                let root = Binder::Capture {
                    definition: id,
                    source: Box::new(binding.identity.clone()),
                };
                if !self.used.get(&id).is_some_and(|used| used.contains(&root)) {
                    continue;
                }
                let shape = self.task_shape(
                    module,
                    context,
                    &binding.ty,
                    !self.creator_moves.get(&id).copied().unwrap_or(false),
                    active,
                    b,
                    0,
                )?;
                free_shapes.push((root, shape, binding.ty.clone()));
            }
            let mut relevant = !protocol_contracts.is_empty()
                || contains_owner(&self.resolved(&prototype.result, b)?);
            relevant |= parameter_shapes.iter().any(tasks::Shape::has_task)
                || free_shapes.iter().any(|(_, shape, _)| shape.has_task());
            for ty in &prototype.parameters {
                if contains_owner(&self.resolved(ty, b)?) {
                    relevant = true;
                }
            }
            for ((owner, _), ty) in &self.expressions {
                b.spend(1)?;
                if *owner == id && contains_owner(&self.resolved(ty, b)?) {
                    relevant = true;
                }
            }
            for ((owner, _), use_) in &self.intrinsic_calls {
                b.spend(1)?;
                if *owner == id
                    && matches!(
                        use_.kind,
                        Intrinsic::AsyncStart
                            | Intrinsic::AsyncValid
                            | Intrinsic::AsyncRequest
                            | Intrinsic::AsyncPoll
                            | Intrinsic::AsyncFinish
                            | Intrinsic::AsyncFinishChecked
                    )
                {
                    relevant = true;
                }
            }
            if !relevant {
                continue;
            }
            b.spend(protocol_contracts.iter().fold(1usize, |n, row| {
                n.saturating_add(row.entry.len()).saturating_add(
                    row.exits.iter().fold(1usize, |n, row| {
                        n.saturating_add(row.output.len()).saturating_add(2)
                    }),
                )
            }))?;
            let mut engine = Engine {
                original: self,
                module,
                index,
                current: id,
                contracts: protocol_contracts.clone(),
                inputs: BTreeMap::new(),
                poll: false,
                next: 0,
                closed_tasks: BTreeSet::new(),
                async_tasks: BTreeSet::new(),
                error_unwind: Vec::new(),
                task_loop_keys: Vec::new(),
            };
            engine.poll = engine.poll_type(&prototype.result, b)?;
            engine.shape(b)?;
            let SemanticOriginalFunctionBody::Defined(body) = body else {
                if protocol_contracts.is_empty() {
                    continue;
                }
                return Err(bad(
                    "original external or abstract body cannot advertise a proved protocol relation",
                ));
            };
            let mut state = State::default();
            for (ordinal, ty) in prototype.parameters.iter().enumerate() {
                b.spend(1)?;
                let ty = self.resolved(ty, b)?;
                if parameter_shapes[ordinal].has_task() {
                    let binder = Binder::Parameter {
                        definition: id,
                        ordinal: ordinal as u32,
                    };
                    let value = engine.task_entry(
                        &mut state,
                        &state::root_place(&binder),
                        &parameter_shapes[ordinal],
                        b,
                    )?;
                    state.values.insert(binder, value);
                } else if let Some(borrowed) = owner_type(&ty) {
                    let root = Binder::Parameter {
                        definition: id,
                        ordinal: ordinal as u32,
                    };
                    let generation = engine.generation(b)?;
                    state.owners.insert(
                        state::root_place(&root),
                        Owner {
                            states: LIVE,
                            generation,
                            task: tasks::is_task(&ty),
                            owned: !borrowed,
                        },
                    );
                    state.values.insert(
                        root.clone(),
                        Value::Owner {
                            root: state::root_place(&root),
                            borrowed,
                        },
                    );
                } else if contains_owner(&ty) {
                    return Err(unsupported(
                        "original nested registration needs its complete projected owner identity",
                        None,
                    ));
                }
            }
            for (root, shape, ty) in &free_shapes {
                if shape.has_task() {
                    let value =
                        engine.task_entry(&mut state, &state::root_place(root), shape, b)?;
                    state.values.insert(root.clone(), value);
                } else if contains_owner(&self.resolved(ty, b)?) {
                    return Err(unsupported(
                        "original captured registration needs its complete capture protocol relation",
                        None,
                    ));
                }
            }
            let mut combinations = vec![(state, BTreeMap::new())];
            for role in &engine.contracts {
                let mut next = Vec::new();
                for (state, inputs) in combinations {
                    for input in &role.entry {
                        let mut state = state.copy(b)?;
                        b.spend(inputs.len().saturating_add(1))?;
                        let mut inputs = inputs.clone();
                        inputs.insert(role.parameter, *input);
                        let root = Binder::Parameter {
                            definition: id,
                            ordinal: role.parameter,
                        };
                        state
                            .owners
                            .get_mut(&state::root_place(&root))
                            .ok_or_else(|| bad("original protocol entry selects no owner"))?
                            .states = bit(*input);
                        next.push((state, inputs));
                    }
                }
                combinations = next;
            }
            for (state, inputs) in combinations {
                engine.inputs = inputs;
                let outcome = engine.block(body, state, &[], None, 0, b)?;
                if outcome.breaks.is_some() || outcome.continues.is_some() {
                    return Err(bad("original protocol control exits outside its loop"));
                }
                if let Some(state) = outcome.fall {
                    engine.check_exit(state, Value::Plain, b)?;
                }
            }
            let closed = std::mem::take(&mut engine.closed_tasks);
            let asynchronous = std::mem::take(&mut engine.async_tasks);
            drop(engine);
            for root in closed {
                b.spend(1)?;
                self.task_closed_cleanups.insert((id, root));
            }
            for root in asynchronous {
                b.spend(1)?;
                self.task_async_cleanups.insert((id, root));
            }
        }
        Ok(())
    }
}

fn owner_type(ty: &Ty) -> Option<bool> {
    match ty {
        Ty::Managed(inner)
            if matches!(
                inner.as_ref(),
                Ty::Carrier {
                    kind: CarrierKind::AsyncRegistration | CarrierKind::Task,
                    ..
                }
            ) =>
        {
            Some(false)
        }
        Ty::Address { inner, .. } if matches!(inner.as_ref(),Ty::Managed(payload)if matches!(payload.as_ref(),Ty::Carrier{kind:CarrierKind::AsyncRegistration|CarrierKind::Task,..})) => {
            Some(true)
        }
        Ty::Desc { inner, .. }
            if matches!(
                inner.as_ref(),
                Ty::Carrier {
                    kind: CarrierKind::AsyncRegistration | CarrierKind::Task,
                    ..
                }
            ) =>
        {
            Some(true)
        }
        _ => None,
    }
}
fn contains_owner(ty: &Ty) -> bool {
    match ty {
        Ty::Carrier {
            kind: CarrierKind::AsyncRegistration | CarrierKind::Task,
            ..
        } => true,
        Ty::Address { inner, .. }
        | Ty::Desc { inner, .. }
        | Ty::Managed(inner)
        | Ty::Store(inner)
        | Ty::Slice(inner)
        | Ty::Array(inner, _) => contains_owner(inner),
        Ty::Tuple(fields) => fields.iter().any(contains_owner),
        Ty::ErrorUnion(a, c) => contains_owner(a) || contains_owner(c),
        Ty::Carrier { child, .. } => contains_owner(child),
        Ty::Nominal(_, args) | Ty::Closure(_, args, _) => args
            .iter()
            .any(|arg| matches!(arg,Arg::Type(ty)if contains_owner(ty))),
        _ => false,
    }
}

impl<'a> Engine<'a> {
    fn function(&self) -> Check<&'a index::Function<'a>> {
        self.index
            .definitions
            .get(&self.current)
            .ok_or_else(|| bad("original protocol function disappeared"))
    }
    fn depth(&self, depth: usize, b: &mut Budget) -> Check<()> {
        b.spend(1)?;
        if depth > b.limits.max_call_depth.min(128) {
            Err(limit("original protocol replay depth exceeded"))
        } else {
            Ok(())
        }
    }
    fn poll_type(&self, ty: &Ty, b: &mut Budget) -> Check<bool> {
        b.spend(1)?;
        let ty = self.original.resolved(ty, b)?;
        let Ty::Nominal(id, arguments) = ty else {
            return Ok(false);
        };
        let program = self
            .module
            .semantic
            .as_ref()
            .ok_or_else(|| bad("original protocol nominal directory absent"))?;
        b.spend(program.definitions.len())?;
        let Some(definition) = program.definitions.iter().find(|row| row.id == id) else {
            return Err(bad("original Poll type has no declaration"));
        };
        if !matches!(definition.body,SemanticDefinitionBody::Nominal{declaration,..}if declaration==zil_ir::builtin_life::POLL)
        {
            return Ok(false);
        }
        // Reuse the independent canonical declaration proof, not its spelling.
        self.original
            .poll_type(self.module, Ty::Atom(Type::Unit), b)?;
        Ok(match arguments.as_slice() {
            [Arg::Type(Ty::Atom(Type::Unit))] => true,
            [Arg::Type(Ty::Tuple(fields))] => fields.is_empty(),
            _ => false,
        })
    }
    fn shape(&self, b: &mut Budget) -> Check<()> {
        let function = self.function()?;
        let prototype = self
            .original
            .prototypes
            .get(&self.current)
            .ok_or_else(|| bad("original protocol signature absent"))?;
        if self
            .contracts
            .windows(2)
            .any(|rows| rows[0].parameter >= rows[1].parameter)
        {
            return Err(bad("original protocol roles are duplicated or unordered"));
        }
        for role in &self.contracts {
            b.spend(
                role.entry
                    .len()
                    .saturating_add(role.exits.len())
                    .saturating_add(prototype.parameters.len())
                    .saturating_add(1),
            )?;
            let parameter = prototype
                .parameters
                .get(role.parameter as usize)
                .ok_or_else(|| bad("original protocol parameter ordinal is absent"))?;
            let borrowed = matches!(parameter,Ty::Address{mutable:true,inner,..}if matches!(inner.as_ref(),Ty::Managed(payload)if matches!(payload.as_ref(),Ty::Carrier{kind:CarrierKind::AsyncRegistration,..})));
            let owned = owner_type(parameter) == Some(false);
            if tasks::is_task(parameter)
                || role.protocol != ProtocolKind::AsyncRegistrationV1
                || (!borrowed && !owned)
                || role.entry.is_empty()
                || role.entry.windows(2).any(|rows| rows[0] >= rows[1])
                || role.exits.is_empty()
            {
                return Err(bad(
                    "original protocol has another kind, entry domain or exclusive owner role",
                ));
            }
            if !matches!(function.body, SemanticOriginalFunctionBody::Defined(_))
                || function.definition.kind == SemanticDefinitionKind::Closure
            {
                return Err(bad(
                    "original protocol needs an ordinary complete executable body",
                ));
            }
            if borrowed && prototype.call_kind != CallKind::Direct {
                return Err(unsupported(
                    "original borrowed protocol role cannot suspend across an unproved owner epoch",
                    None,
                ));
            }
            if owned
                && !self.poll
                && !matches!(&prototype.result,Ty::Atom(ty)if ty.is_b0_scalar()||*ty==Type::ForeignHandle)
            {
                return Err(unsupported(
                    "original consumed protocol role needs its complete finite Copy result/escape proof",
                    None,
                ));
            }
            if matches!(prototype.result, Ty::ErrorUnion(..)) {
                return Err(unsupported(
                    "original ErrorUnion protocol exits require their complete two-arm source relation",
                    None,
                ));
            }
            for row in &role.exits {
                b.spend(row.output.len().saturating_add(1))?;
                let allowed = if self.poll {
                    matches!(
                        row.exit,
                        ProtocolExit::PollPending | ProtocolExit::PollReady
                    )
                } else {
                    row.exit == ProtocolExit::Return
                };
                if !role.entry.contains(&row.input)
                    || !allowed
                    || row.output.windows(2).any(|rows| rows[0] >= rows[1])
                    || (borrowed && row.output.is_empty())
                    || (owned && !row.output.is_empty())
                {
                    return Err(bad(
                        "original protocol transition changes its input/exit/owner category",
                    ));
                }
            }
            if role
                .exits
                .windows(2)
                .any(|rows| (rows[0].input, rows[0].exit) >= (rows[1].input, rows[1].exit))
            {
                return Err(bad(
                    "original protocol transitions are duplicated or unordered",
                ));
            }
            for (ordinal, parameter) in prototype.parameters.iter().enumerate() {
                if owner_type(parameter).is_some()
                    && !tasks::is_task(parameter)
                    && !self
                        .contracts
                        .iter()
                        .any(|row| row.parameter as usize == ordinal)
                {
                    return Err(bad(
                        "original protocol omits another actual owner parameter",
                    ));
                }
            }
        }
        Ok(())
    }
    fn check_exit(&mut self, mut state: State, value: Value, b: &mut Budget) -> Check<()> {
        self.task_exit(&mut state, b)?;
        let exits = if self.poll {
            &[ProtocolExit::PollPending, ProtocolExit::PollReady][..]
        } else {
            &[ProtocolExit::Return][..]
        };
        for &exit in exits {
            let mut actual = state.copy(b)?;
            if self.poll && !actual.refine(&value, exit, b)? {
                continue;
            }
            for role in &self.contracts {
                b.spend(role.exits.len().saturating_add(2))?;
                let input = self.inputs.get(&role.parameter).ok_or_else(|| {
                    bad("original protocol exit lost its universal entry assignment")
                })?;
                let row = role
                    .exits
                    .iter()
                    .find(|row| row.input == *input && row.exit == exit)
                    .ok_or_else(|| {
                        bad("original actual Return is missing from its declared protocol upper")
                    })?;
                let root = Binder::Parameter {
                    definition: self.current,
                    ordinal: role.parameter,
                };
                let observed = actual
                    .owners
                    .get(&state::root_place(&root))
                    .map_or(DEAD, |owner| owner.states);
                let allowed = if row.output.is_empty() {
                    DEAD
                } else {
                    mask(&row.output)
                };
                if observed == 0 || observed & !allowed != 0 {
                    return Err(bad(
                        "original actual Return exceeds its declared owner phase relation",
                    ));
                }
            }
        }
        Ok(())
    }
}
