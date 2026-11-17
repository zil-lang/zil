//! Instantiate an already independently checked original term. This never
//! feeds executable class coincidences back into the rigid original solver.
use super::*;
use crate::semantic::value::{self, Argument, Class, Value};

impl Checked {
    pub(in crate::semantic) fn call_receiver_view(
        &self, context: &SemanticInstance, site: SemanticSiteId,
        inventory: &Inventory<'_>, b: &mut Budget,
    ) -> CompileResult<bool> {
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } =
            &inventory.definition(context.definition, b)?.body else {
                return Err(fail("Receiver call has no original body directory"));
            };
        let mut selected = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.site != site { continue; }
            let SemanticOriginalNode::Expression(node) = link.node else {
                return Err(fail("Receiver call changes its original expression kind"));
            };
            if selected.replace(node).is_some() { return Err(fail("Receiver call repeats its original expression")); }
        }
        let Some(node) = selected else { return Ok(false); };
        b.lookup(self.frames.len(), WORK)?;
        let frame = self.frames.get(&context.definition)
            .ok_or_else(|| fail("Receiver call lacks its checked original frame"))?;
        b.lookup(frame.expressions.len(), WORK)?;
        let Some(term) = frame.expressions.get(&node) else { return Ok(false); };
        let root = self.root(*term, b)?;
        let Kind::Address { mutable: false, life, .. } = self.arena[root] else { return Ok(false); };
        Ok(matches!(self.arena[self.root(life, b)?],
            Kind::Life(Life::Receiver(owner, call)) if owner == context.definition && call == node))
    }

    pub(in crate::semantic) fn call_output(&self, context: &SemanticInstance, site: SemanticSiteId,
        program: &SemanticModule, inventory: &Inventory<'_>, b: &mut Budget) -> CompileResult<Value> {
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } =
            &inventory.definition(context.definition, b)?.body else {
                return Err(fail("call output omits its complete original body"));
            };
        let mut node = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.site != site { continue; }
            let SemanticOriginalNode::Expression(actual) = link.node else {
                return Err(fail("call output changes its original expression role"));
            };
            if node.replace(actual).is_some() { return Err(fail("call output repeats its original anchor")); }
        }
        let node = node.ok_or_else(|| fail("call output has no original expression"))?;
        b.lookup(self.calls.len(), WORK)?;
        let ordinary = self.calls.contains_key(&(context.definition, node));
        b.lookup(self.trait_calls.len(), WORK)?;
        if !ordinary && !self.trait_calls.contains_key(&(context.definition, node)) {
            return Err(fail("call output has no independently checked original call"));
        }
        self.source_value(context, SemanticScopeRef::Ancestor(0),
            &SemanticOriginalTypeSource::Expression(node), program, inventory, b)
    }

    pub(in crate::semantic) fn description_source(
        &self,
        context: &SemanticInstance,
        site: SemanticSiteId,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<Option<descriptions::Source>> {
        let definition = inventory.definition(context.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &definition.body else {
            return Err(fail("original description has no complete body"));
        };
        let mut selected = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.site != site {
                continue;
            }
            let SemanticOriginalNode::Expression(node) = link.node else {
                continue;
            };
            b.lookup(self.descriptions.len(), WORK)?;
            let Some(source) = self.descriptions.get(&(context.definition, node)) else {
                continue;
            };
            b.spend(source.path.len(), WORK)?;
            let source = descriptions::Source {
                role: (
                    self.life_value(source.role.occurrence, context, inventory, b)?,
                    source.role.upper.map(|upper| self.life_value(upper, context, inventory, b)).transpose()?,
                    self.life_value(source.role.selected, context, inventory, b)?,
                    source.role.local,
                ),
                anchor: source.anchor,
                root: self.value(source.root, context, program, inventory, b, 0)?,
                source: self.value(source.source, context, program, inventory, b, 0)?,
                output: self.value(source.output, context, program, inventory, b, 0)?,
                path: source.path.clone(),
            };
            if selected.replace(source).is_some() {
                return Err(fail("description has competing original source operations"));
            }
        }
        Ok(selected)
    }
    fn context<'a>(
        &self,
        owner: SemanticDefinitionId,
        mut context: &'a SemanticInstance,
        inventory: &Inventory<'a>,
        b: &mut Budget,
    ) -> CompileResult<&'a SemanticInstance> {
        for _ in 0..=b.partial_depth {
            b.spend(1, WORK)?;
            if context.definition == owner {
                return Ok(context);
            }
            context = inventory.instance(
                context.parent.ok_or_else(|| {
                    fail("original semantic term leaves its actual lexical context")
                })?,
                b,
            )?;
        }
        Err(resource(
            "original actual lexical context exceeds its finite depth",
        ))
    }
    pub(super) fn life_value(
        &self,
        term: Term,
        context: &SemanticInstance,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<Class> {
        match &self.arena[self.root(term, b)?] {
            Kind::Life(Life::Formal(owner, ordinal)) => {
                let scope = self.context(*owner, context, inventory, b)?;
                value::class(
                    scope,
                    &SemanticClassExpr::Formal {
                        scope: SemanticScopeRef::Ancestor(0),
                        ordinal: *ordinal,
                    },
                    inventory,
                    b,
                    0,
                )
            }
            Kind::Life(Life::Implicit(owner, ordinal)) => {
                let scope = self.context(*owner, context, inventory, b)?;
                value::class(
                    scope,
                    &SemanticClassExpr::Implicit {
                        scope: SemanticScopeRef::Ancestor(0),
                        ordinal: *ordinal,
                    },
                    inventory,
                    b,
                    0,
                )
            }
            Kind::Life(Life::ClosureReceiver(owner)) => {
                self.receiver_sources(*owner, inventory, b)?
                    .ok_or_else(|| fail("closure Receiver has no complete original return upper proof"))?;
                let scope = self.context(*owner, context, inventory, b)?;
                let SemanticInstanceTarget::Function(target) = &scope.target else {
                    return Err(fail("closure Receiver has no actual entry world"));
                };
                let function = inventory.function(target, b)?;
                let closure = function.environment.and_then(|id| inventory.module.closures.get(id.0 as usize))
                    .filter(|closure| closure.function == function.id)
                    .ok_or_else(|| fail("closure Receiver is outside its actual environment"))?;
                if function.call_kind != CallKind::Direct || !crate::stored_callables::receiver_access_head(closure) {
                    return Err(fail("closure Receiver changes its actual reusable Direct access mode"));
                }
                let header = function.storage_life.as_ref()
                    .ok_or_else(|| fail("closure Receiver omits its actual entry header"))?;
                let Some(StorageBinder::Invocation(index)) = crate::signature_life::outer(header, StorageBase::Receiver) else {
                    return Err(fail("closure Receiver has no actual invocation access head"));
                };
                if index >= header.invocation_binder_count { return Err(fail("closure Receiver leaves its actual binder domain")); }
                Ok(Class::Invocation(scope.id, index))
            }
            Kind::Life(Life::CallResult(owner, node, target, ordinal)) => {
                let scope = self.context(*owner, context, inventory, b)?;
                b.lookup(self.calls.len(), WORK)?;
                let call = self
                    .calls
                    .get(&(*owner, *node))
                    .ok_or_else(|| fail("returned region has no checked original call"))?;
                if call.target != *target {
                    return Err(fail("returned region changes its exact original callee"));
                }
                let mut declared = false;
                for (_, role) in self
                    .reference_roles
                    .range((*target, NodeId(0))..=(*target, NodeId(u32::MAX)))
                {
                    b.spend(1, WORK)?;
                    if role.ordinal == *ordinal {
                        if declared {
                            return Err(fail("returned region repeats its original declaration"));
                        }
                        declared = true;
                    }
                }
                if !declared {
                    return Err(fail(
                        "returned region has no independently checked original local role",
                    ));
                }
                if inventory.source.is_none() {
                    let program = inventory.module.semantic.as_ref().ok_or_else(|| {
                        fail("returned region has no executable semantic program")
                    })?;
                    let recipe = inventory.call_anchor(*owner, *node, b)?;
                    let site = inventory.emitted_site(program, scope.id, recipe, b)?;
                    let SemanticSiteAddress::Typed {
                        block,
                        instruction,
                        edge: SemanticSiteEdge::After,
                    } = site.at
                    else {
                        return Err(fail("returned region has another actual call boundary"));
                    };
                    let SemanticInstanceTarget::Function(function) = &scope.target else {
                        return Err(fail("returned region is not in its actual function world"));
                    };
                    let function = inventory.function(function, b)?;
                    let actual = function
                        .blocks
                        .get(block.0 as usize)
                        .filter(|row| row.id == block)
                        .and_then(|row| row.instructions.get(instruction as usize))
                        .ok_or_else(|| fail("returned region has no actual call instruction"))?;
                    let declaration = inventory.definition(*target, b)?;
                    let SemanticDefinitionBody::FunctionWithBody { signature, .. } =
                        &declaration.body
                    else {
                        return Err(fail("returned region lacks the original callee signature"));
                    };
                    let callee = match &actual.kind {
                        InstructionKind::Call { function, .. }
                            if signature.call_kind == CallKind::Direct =>
                        {
                            *function
                        }
                        InstructionKind::SuspendCall {
                            function: callee, ..
                        } if signature.call_kind == CallKind::Suspend
                            && function.call_kind == CallKind::Suspend =>
                        {
                            *callee
                        }
                        _ => {
                            return Err(fail("returned region changes its exact original call ABI"));
                        }
                    };
                    if actual.result.is_none() {
                        return Err(fail("returned region has no actual result SSA"));
                    }
                    let worlds = inventory.worlds(program, callee, b)?;
                    if worlds.is_empty() || worlds.iter().any(|world| world.definition != *target) {
                        return Err(fail(
                            "returned region selects another actual callee definition",
                        ));
                    }
                }
                // This names only the region. The closed result consumer still
                // proves every world's complete Store and escaping-source graph.
                Ok(Class::CallResult(scope.id, *node, *target, *ordinal))
            }
            Kind::Life(Life::Receiver(owner, node)) => {
                let scope = self.context(*owner, context, inventory, b)?;
                b.lookup(self.calls.len().saturating_add(self.abstract_calls.len()), WORK)?;
                let callable = self.calls.get(&(*owner, *node)).is_some_and(|call| call.receiver.is_some())
                    || self.abstract_calls.get(&(*owner, *node)).is_some_and(|call|
                        roles::receiver_result(&call.signature, call.kind));
                if callable {
                    // Source reserves this exact genuine Call occurrence.
                    // Naming its class grants no source or receiver-head loan;
                    // the actual call consumer separately checks that identity.
                    let ordinal = self.callable_receiver_ordinal(*owner, *node, inventory, b)?;
                    if inventory.source.is_none() {
                        self.callable_receiver_operation(scope, *node, inventory, b)?;
                    }
                    return Ok(Class::Implicit(scope.id, ordinal));
                }
                // Trait receivers keep their existing independently checked
                // call-local role and actual borrowed-head mapping.
                if inventory.source.is_some() {
                    return Ok(Class::Role(scope.id, node.0));
                }
                let definition = inventory.definition(scope.definition, b)?;
                let SemanticDefinitionBody::FunctionWithBody {
                    site_nodes, sites, ..
                } = &definition.body
                else {
                    return Err(fail("receiver role has no complete original body"));
                };
                let program = inventory
                    .module
                    .semantic
                    .as_ref()
                    .ok_or_else(|| fail("receiver role has no actual semantic module"))?;
                let SemanticInstanceTarget::Function(target) = &scope.target else {
                    return Err(fail("receiver role has no actual function world"));
                };
                let function = inventory.function(target, b)?;
                b.lookup(self.trait_calls.len(), WORK)?;
                let call = self
                    .trait_calls
                    .get(&(*owner, *node))
                    .ok_or_else(|| fail("receiver role has no independently checked trait call"))?;
                if call.receiver_anchor.is_none() {
                    return Err(fail(
                        "receiver role changes its original borrowed method kind",
                    ));
                }
                let mut recipe = None;
                for link in site_nodes {
                    b.spend(1, WORK)?;
                    if link.node != SemanticOriginalNode::Expression(*node) {
                        continue;
                    }
                    b.spend(sites.len(), WORK)?;
                    let source = sites
                        .iter()
                        .find(|site| site.id == link.site)
                        .ok_or_else(|| fail("receiver role points to an absent original site"))?;
                    // Trait member calls retain their original syntax until
                    // the actual provider is selected. Both recipe forms bind
                    // this same independently checked call-local receiver.
                    if matches!(
                        source.kind,
                        SemanticSiteRecipeKind::OriginalCall | SemanticSiteRecipeKind::Call { .. }
                    ) && recipe.replace(link.site).is_some()
                    {
                        return Err(fail("receiver role has competing original call recipes"));
                    }
                }
                let recipe = recipe
                    .ok_or_else(|| fail("receiver role has no exact original call recipe"))?;
                let mut selected = None;
                for site in &program.sites {
                    b.spend(1, WORK)?;
                    if site.instance != scope.id || site.recipe != recipe {
                        continue;
                    }
                    if !matches!(site.action, SemanticSiteAction::BodyCall { .. }) {
                        return Err(fail("receiver role changes its actual call action"));
                    }
                    let SemanticSiteAddress::Typed {
                        block,
                        instruction,
                        edge: SemanticSiteEdge::After,
                    } = site.at
                    else {
                        return Err(fail("receiver role changes its original call occurrence"));
                    };
                    let operation = function
                        .blocks
                        .get(block.0 as usize)
                        .filter(|row| row.id == block)
                        .and_then(|row| row.instructions.get(instruction as usize))
                        .ok_or_else(|| fail("receiver role has no actual call operation"))?;
                    let head = match &operation.kind {
                        InstructionKind::Call { arguments, .. }
                            if call.dynamic.is_none() && call.call_kind == CallKind::Direct =>
                        {
                            *arguments.first().ok_or_else(|| fail("receiver role omits its actual receiver"))?
                        }
                        InstructionKind::SuspendCall { arguments, .. }
                            if call.dynamic.is_none() && call.call_kind == CallKind::Suspend
                                && function.call_kind == CallKind::Suspend =>
                        {
                            *arguments.first().ok_or_else(|| fail("receiver role omits its actual receiver"))?
                        }
                        InstructionKind::CallDynSlot { callee, interface, slot, .. }
                            if call.dynamic.is_some() && call.call_kind == CallKind::Direct =>
                        {
                            let receiver = self.value(call.receiver, scope, program, inventory, b, 0)?;
                            if !matches!(receiver.ty, Type::Dyn { interface: original } if original == *interface) {
                                return Err(fail("dynamic receiver role changes its complete original interface"));
                            }
                            let mut found = None;
                            for candidate in &inventory.module.dyn_interfaces {
                                b.spend(1, WORK)?;
                                if candidate.id == *interface && found.replace(candidate).is_some() {
                                    return Err(fail("dynamic receiver role repeats its actual interface"));
                                }
                            }
                            let directory = found.ok_or_else(|| fail("dynamic receiver role has no complete actual interface"))?;
                            let member = directory.slots.get(*slot as usize)
                                .ok_or_else(|| fail("dynamic receiver role selects an absent method slot"))?;
                            if directory.application.declaration != call.declaration
                                || member.member.application != directory.application
                                || member.member.ordinal != call.member
                                || member.call_kind != call.call_kind
                                || !matches!(function.values.get(callee.0 as usize),
                                    Some(Type::Address { mutable: false, inner })
                                    if matches!(inner.as_ref(), Type::Dyn { interface: actual } if actual == interface))
                            {
                                return Err(fail("dynamic receiver role changes its original member, application or actual head"));
                            }
                            *callee
                        }
                        _ => return Err(fail("receiver role changes its original call ABI or operation")),
                    };
                    if !matches!(
                        function.values.get(head.0 as usize),
                        Some(Type::Address { .. })
                    ) {
                        return Err(fail("receiver role has no actual borrowed head"));
                    }
                    if selected
                        .replace(Class::Reborrow(function.id, head))
                        .is_some()
                    {
                        return Err(fail("receiver role has competing actual call bindings"));
                    }
                }
                // This only names the call's outer class. The trait operation
                // still checks the complete provider/member, actual vector,
                // receiver place and type; memory flow proves the real loan.
                selected.ok_or_else(|| fail("receiver role has no exact actual call binding"))
            }
            Kind::Life(Life::Invocation(owner, ordinal)) => {
                let scope = self.context(*owner, context, inventory, b)?;
                Ok(Class::Invocation(scope.id, *ordinal))
            }
            _ => Err(unknown(
                "original lifetime instantiation is unresolved independently of actual caller images",
            )),
        }
    }
    pub(super) fn argument(
        &self,
        term: Term,
        context: &SemanticInstance,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Argument> {
        b.spend(1, WORK)?;
        match &self.arena[self.root(term, b)?] {
            Kind::Life(_) | Kind::LifeVariable(_) => Ok(Argument::Life(
                self.life_value(term, context, inventory, b)?,
            )),
            Kind::ConstValue(constant) => Ok(Argument::Const {
                carrier: constant.ty(),
                value: constant.clone(),
            }),
            Kind::ConstRigid {
                owner,
                ordinal,
                carrier,
            } => {
                let scope = self.context(*owner, context, inventory, b)?;
                let Some(SemanticActual::Const {
                    ty: SemanticTypeValue::Resolved { ty, .. },
                    value,
                }) = scope.actuals.get(*ordinal as usize)
                else {
                    return Err(fail(
                        "original constant lacks its complete actual carrier and value",
                    ));
                };
                crate::dyn_metadata::ty(ty, b)?;
                if self.materialize(*carrier, context, program, inventory, b, depth + 1)? != *ty
                    || value.ty() != *ty
                {
                    return Err(fail(
                        "original constant carrier differs from its actual full ordinal",
                    ));
                }
                Ok(Argument::Const {
                    carrier: ty.clone(),
                    value: value.clone(),
                })
            }
            _ => Ok(Argument::Type(self.value(
                term,
                context,
                program,
                inventory,
                b,
                depth + 1,
            )?)),
        }
    }
    pub(super) fn value(
        &self,
        term: Term,
        context: &SemanticInstance,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(resource(
                "original complete value instantiation exceeds its depth",
            ));
        }
        match &self.arena[self.root(term, b)?] {
            Kind::Atom(ty) => Ok(value::scalar(ty.clone())),
            Kind::Task(task) => self.task_value(task, context, program, inventory, b, depth + 1),
            Kind::Dyn(ty) => self.dyn_value(ty, context, program, inventory, b, depth + 1),
            Kind::MutexGuard { payload, life } => {
                b.spend(6, WORK)?;
                let payload = self.value(*payload, context, program, inventory, b, depth + 1)?;
                if !payload.classes.is_empty() || payload.ty != Type::I32 {
                    return Err(unknown(
                        "Mutex guard needs the complete protected payload and retained inventory",
                    ));
                }
                let class = self.life_value(*life, context, inventory, b)?;
                let payload = payload.into_type(b)?;
                let ty = Type::MutexGuard {
                    payload: Box::new(payload.clone()),
                    retained_life: RetainedLifeShape {
                        binder_count: 1,
                        ports: vec![RetainedBorrowPort {
                            leaf: Type::Mutex {
                                payload: Box::new(payload),
                            },
                            mode: StoredBorrowMode::SharedAddress,
                            binder: 0,
                        }],
                        retained: Vec::new(),
                        nominal_arguments: Vec::new(),
                    },
                };
                Ok(Value::new(ty, StorageLifeShape {
                        binder_count: 1,
                        slots: Vec::new(),
                        retained: Vec::new(),
                        nominal_arguments: Vec::new(),
                    }, vec![class]))
            }
            Kind::CallResult {
                caller,
                node,
                hidden,
            } => self.call_result(
                *caller,
                *node,
                *hidden,
                context,
                program,
                inventory,
                b,
                depth + 1,
            ),
            Kind::Rigid(owner, ordinal) => {
                let scope = self.context(*owner, context, inventory, b)?;
                let Some(SemanticActual::Type(actual)) = scope.actuals.get(*ordinal as usize)
                else {
                    return Err(fail("original Type formal has no complete actual"));
                };
                value::resolved(scope, actual, inventory, b, depth + 1)
            }
            Kind::SelfType(owner) => super::super::closed::original_self(
                *owner,
                context,
                program,
                inventory,
                self,
                b,
                depth + 1,
            ),
            Kind::Associated {
                receiver,
                declaration,
                arguments,
                member,
            } => {
                let receiver = self.value(*receiver, context, program, inventory, b, depth + 1)?;
                let mut actuals = Vec::new();
                for argument in arguments {
                    let argument =
                        self.argument(*argument, context, program, inventory, b, depth + 1)?;
                    Solver::push(&mut actuals, argument, b)?;
                }
                super::super::closed::original_associated(
                    &receiver,
                    *declaration,
                    &actuals,
                    *member,
                    program,
                    inventory,
                    self,
                    b,
                    depth + 1,
                )
            }
            Kind::Address {
                mutable,
                life,
                inner,
            } => {
                let inner = self.value(*inner, context, program, inventory, b, depth + 1)?;
                let class = self.life_value(*life, context, inventory, b)?;
                value::address(*mutable, class, inner, b)
            }
            Kind::Nominal {
                definition,
                arguments,
                fields,
            } => {
                let mut actuals = Vec::new();
                let mut physical = Vec::new();
                for term in arguments {
                    let actual = self.argument(*term, context, program, inventory, b, depth + 1)?;
                    Solver::push(&mut actuals, actual, b)?;
                }
                for term in fields {
                    let field = self.value(*term, context, program, inventory, b, depth + 1)?;
                    Solver::push(&mut physical, field, b)?;
                }
                value::nominal(*definition, &actuals, &physical, inventory, b, depth + 1)
            }
            Kind::Wrapped(wrapper, inner) => {
                let inner = self.value(*inner, context, program, inventory, b, depth + 1)?;
                if *wrapper == Wrapper::Managed && matches!(inner.ty, Type::Task { .. }) {
                    return value::resource(geometry::Unary::Managed, inner, b);
                }
                if !inner.classes.is_empty() {
                    return Err(unknown(
                        "original opaque resource constructor needs its complete retained and phantom child occurrence transport",
                    ));
                }
                let kind = match wrapper {
                    Wrapper::Managed => geometry::Unary::Managed,
                    Wrapper::Store => geometry::Unary::OwnedStore,
                    Wrapper::Mutex => geometry::Unary::Mutex,
                    Wrapper::Registration => geometry::Unary::Registration,
                    Wrapper::AsyncRegistration => geometry::Unary::AsyncRegistration,
                };
                value::unary(kind, inner, b)
            }
            Kind::Desc { life, inner } => {
                let inner = self.value(*inner, context, program, inventory, b, depth + 1)?;
                let class = self.life_value(*life, context, inventory, b)?;
                value::description(class, inner, b)
            }
            Kind::ErrorUnion { error, result } => {
                let error = self.value(*error, context, program, inventory, b, depth + 1)?;
                let result = self.value(*result, context, program, inventory, b, depth + 1)?;
                value::error_union(error, result, b)
            }
            Kind::Code {
                kind,
                parameters,
                result,
                ..
            } => {
                let mut physical = Vec::new();
                for parameter in parameters {
                    let value =
                        self.value(*parameter, context, program, inventory, b, depth + 1)?;
                    if !value.classes.is_empty() {
                        return Err(unknown(
                            "original code pointer needs its complete input scope relation",
                        ));
                    }
                    Solver::push(&mut physical, value.into_type(b)?, b)?;
                }
                let result = self.value(*result, context, program, inventory, b, depth + 1)?;
                if !result.classes.is_empty() {
                    return Err(unknown(
                        "original code pointer needs its complete output scope relation",
                    ));
                }
                let ty = match kind {
                    SemanticCodeKind::FunctionPointer => Type::FunctionPointer {
                        parameters: physical,
                        result: Box::new(result.into_type(b)?),
                    },
                    SemanticCodeKind::ForeignCallback => Type::ForeignCallback {
                        parameters: physical,
                        result: Box::new(result.into_type(b)?),
                    },
                    SemanticCodeKind::BorrowedFunctionPointer => {
                        return Err(unknown(
                            "original borrowed code needs its complete invocation contract",
                        ));
                    }
                };
                Ok(value::scalar(ty))
            }
            Kind::Array { element, length } => {
                let element = self.value(*element, context, program, inventory, b, depth + 1)?;
                if !element.classes.is_empty() {
                    return Err(unknown(
                        "original Array needs its complete Element class occurrence inventory",
                    ));
                }
                let Argument::Const {
                    carrier: Type::Usize,
                    value: Constant::Usize(length),
                } = self.argument(*length, context, program, inventory, b, depth + 1)?
                else {
                    return Err(fail(
                        "original Array length changes its complete usize constant carrier",
                    ));
                };
                let length = u32::try_from(length).map_err(|_| {
                    fail("original Array length exceeds its represented finite domain")
                })?;
                value::unary(geometry::Unary::Array(length), element, b)
            }
            Kind::Slice(element) => {
                let element = self.value(*element, context, program, inventory, b, depth + 1)?;
                if !element.classes.is_empty() {
                    return Err(unknown(
                        "original Slice requires its complete element occurrence embedding",
                    ));
                }
                value::unary(geometry::Unary::Slice, element, b)
            }
            Kind::Tuple(fields) => {
                let mut types = Vec::new();
                for field in fields {
                    let field = self.value(*field, context, program, inventory, b, depth + 1)?;
                    if !field.classes.is_empty() {
                        return Err(unknown(
                            "original tuple needs its complete child class embedding",
                        ));
                    }
                    Solver::push(&mut types, field, b)?;
                }
                value::tuple(types, b)
            }
            Kind::Closure {
                definition,
                environment,
                ..
            } => {
                let ty = self.materialize(term, context, program, inventory, b, depth + 1)?;
                let Type::Closure(id) = ty else {
                    return Err(fail("original closure materialization has another type"));
                };
                let entry = inventory
                    .module
                    .closures
                    .get(id.0 as usize)
                    .filter(|entry| entry.id == id)
                    .ok_or_else(|| fail("original closure has no actual inventory"))?;
                let count = entry
                    .storage_life
                    .as_ref()
                    .ok_or_else(|| unknown("original closure has no complete intrinsic scope"))?
                    .fixed_binder_count;
                let mut classes = Vec::new();
                for environment in environment {
                    let value =
                        self.value(*environment, context, program, inventory, b, depth + 1)?;
                    for class in value.classes {
                        value::intern(&mut classes, class, b)?;
                    }
                }
                if classes.len() < count as usize {
                    let child = program
                        .instances
                        .iter()
                        .find(|instance| {
                            instance.definition == *definition
                                && instance.environment.as_ref().is_some_and(|environment| {
                                    environment.closure == SemanticClosureTarget::Typed(id)
                                })
                        })
                        .ok_or_else(|| fail("original closure scope has no exact actual child"))?;
                    let source = inventory.definition(child.definition, b)?;
                    for binding in &child.header_embedding {
                        if matches!(binding.header, StorageBinder::Fixed(_)) {
                            if let SemanticClassExpr::Implicit {
                                scope: SemanticScopeRef::Ancestor(0),
                                ordinal,
                            } = binding.actual
                            {
                                if !source
                                    .implicit_lifetimes
                                    .iter()
                                    .any(|row| row.ordinal == ordinal)
                                {
                                    return Err(fail(
                                        "closure head class has no original occurrence",
                                    ));
                                }
                                value::intern(&mut classes, Class::Implicit(child.id, ordinal), b)?;
                            }
                        }
                    }
                }
                if count as usize != classes.len() {
                    return Err(fail(
                        "actual closure scope omits original environment or phantom classes",
                    ));
                }
                Ok(Value::new(Type::Closure(id), StorageLifeShape {
                        binder_count: count,
                        slots: Vec::new(),
                        retained: Vec::new(),
                        nominal_arguments: Vec::new(),
                    }, classes))
            }
            _ => Err(unknown(
                "original complete type remains unresolved independently of executable images",
            )),
        }
    }
}

impl Checked {
    #[allow(clippy::too_many_arguments)]
    fn call_result(
        &self,
        caller: SemanticDefinitionId,
        node: NodeId,
        hidden: Term,
        context: &SemanticInstance,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        // A genuinely scalar result has no hidden class to anchor. This is
        // also the domain of an original CT call already evaluated by the
        // separately checked frozen program. It grants no execution or value
        // equation and cannot ground an opaque/phantom result.
        if let Kind::Atom(ty) = &self.arena[self.root(hidden, b)?]
            && ty.is_b0_scalar()
        {
            b.spend(1, WORK)?;
            return Ok(value::scalar(ty.clone()));
        }
        let context = self.context(caller, context, inventory, b)?;
        b.lookup(self.calls.len(), WORK)?;
        let call = self.calls.get(&(caller, node)).ok_or_else(|| fail("original result role has no checked call and actual vector"))?;
        let recipe = inventory.call_anchor(caller, node, b)?;
        let function = match &context.target {
            SemanticInstanceTarget::Function(target) => inventory.function(target, b)?,
            _ => return Err(fail("original call result has no real caller function")),
        };
        let site = inventory.emitted_site(program, context.id, recipe, b)?;
        let SemanticSiteAddress::Typed { block, instruction, edge: SemanticSiteEdge::After } = site.at else {
            return Err(fail("original call result does not name its actual after boundary"));
        };
        let instruction = function.blocks.get(block.0 as usize)
            .filter(|actual| actual.id == block)
            .and_then(|block| block.instructions.get(instruction as usize))
            .ok_or_else(|| fail("original call role has no actual instruction"))?;
        let action = &site.action;
        if let InstructionKind::Call {
            function: target, ..
        }
        | InstructionKind::SuspendCall {
            function: target, ..
        }
        | InstructionKind::ForeignCall {
            function: target, ..
        } = instruction.kind
        {
            let mut worlds = 0usize;
            for instance in inventory.worlds(program, target, b)? {
                if instance.definition != call.target {
                    return Err(fail("original result role selects another actual callee"));
                }
                worlds = worlds.saturating_add(1);
            }
            if worlds == 0 {
                return Err(fail("original result role has no actual callee world"));
            }
            if worlds > 1 {
                let actuals = match action {
                    SemanticSiteAction::BodyCall { actuals } => actuals,
                    SemanticSiteAction::Call {
                        target: SemanticFunctionTarget::Typed(id),
                        actuals,
                    } if *id == target => actuals,
                    _ => return Err(fail("shared result changes its exact actual call receipt")),
                };
                if actuals.len() != call.actuals.len() {
                    return Err(fail("shared result omits its original full mixed vector"));
                }
                // The result is instantiated in the caller's checked original
                // domain below. Do not select a representative callee world:
                // every world of this code must independently fit this receipt.
                for world in &program.instances {
                    b.spend(1, WORK)?;
                    if !matches!(world.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) if id==target)
                    {
                        continue;
                    }
                    // Each world retains its independently checked lexical
                    // parent, Self, environment and header. None is selected
                    // as a representative of the caller's result classes.
                    // The original result below is instantiated in this exact
                    // caller's checked scheme, while every callee world must
                    // satisfy the same complete mixed boundary structurally.
                    if world.actuals.len() != call.actuals.len() {
                        return Err(fail("shared result world changes its full mixed arity"));
                    }
                    for (ordinal, ((original, proposed), actual)) in
                        call.actuals.iter().zip(actuals).zip(&world.actuals).enumerate()
                    {
                        let expected =
                            self.argument(*original, context, program, inventory, b, depth + 1)?;
                        let proposed =
                            self.result_world_argument(context, proposed, inventory, b, depth + 1)?;
                        let actual =
                            self.result_world_argument(world, actual, inventory, b, depth + 1)?;
                        if proposed != expected {
                            return Err(fail("shared result receipt differs from its complete caller-domain original actual"));
                        }
                        let valid = match (&expected, &actual) {
                            (Argument::Life(_), Argument::Life(Class::Formal(owner, index))) =>
                                *owner == world.id && *index as usize == ordinal,
                            (Argument::Type(expected), Argument::Type(actual)) => {
                                // Classes belong to different original worlds.
                                // Compare full N/shape and physical Type here;
                                // actual header/input/result class equations
                                // remain separate boundary obligations.
                                geometry::same_type(&expected.ty, &actual.ty, b)?
                                    && expected.life == actual.life
                                    && expected.classes.len() == actual.classes.len()
                            }
                            (Argument::Const { carrier: expected, value }, Argument::Const { carrier: actual, value: actual_value }) =>
                                geometry::same_type(expected, actual, b)? && value == actual_value,
                            _ => false,
                        };
                        if !valid {
                            return Err(fail("shared result world changes its independently checked rigid mixed boundary"));
                        }
                    }
                }
            }
        } else if !call.receiver.is_some()
            || !matches!(
                instruction.kind,
                InstructionKind::CallClosure { .. }
                    | InstructionKind::CallClosureOnce { .. }
                    | InstructionKind::CallClosureShared { .. }
                    | InstructionKind::CallClosureMut { .. }
                    | InstructionKind::CallClosureBorrowed { .. }
            )
        {
            return Err(fail("original result role changed its actual call family"));
        }
        let result = self.value(hidden, context, program, inventory, b, depth + 1)?;
        let Some(value) = instruction.result else {
            if result.ty == Type::Unit && result.classes.is_empty() {
                return Ok(result);
            }
            return Err(fail("original class-bearing call has no actual result SSA"));
        };
        let ty = function
            .values
            .get(value.0 as usize)
            .ok_or_else(|| fail("original result role has no complete actual SSA type"))?;
        crate::dyn_metadata::ty(ty, b)?;
        if *ty != result.ty {
            return Err(fail(
                "actual call result differs from the complete original result role",
            ));
        }
        Ok(result)
    }

    /// Resolve a proposed boundary in its own exact original world. Full class
    /// images are retained; comparing worlds does not identify their classes.
    fn result_world_argument(
        &self,
        context: &SemanticInstance,
        actual: &SemanticActual,
        inventory: &Inventory<'_>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Argument> {
        crate::dyn_metadata::recipe(actual, b)?;
        let (input, constant) = match actual {
            SemanticActual::Type(input) => (input, None),
            SemanticActual::Const { ty, value } => (ty, Some(value)),
            SemanticActual::Life(life) => {
                return Ok(Argument::Life(value::class(context, life, inventory, b, depth + 1)?));
            }
            _ => return Err(fail("shared call result contains an unmaterialized actual")),
        };
        let value = value::resolved(context, input, inventory, b, depth)?;
        if let Some(constant) = constant {
            if constant.ty() != value.ty {
                return Err(fail(
                    "shared call result changes its complete constant carrier",
                ));
            }
            Ok(Argument::Const {
                carrier: value.into_type(b)?,
                value: constant.clone(),
            })
        } else {
            Ok(Argument::Type(value))
        }
    }
}

impl Checked {
    fn callable_receiver_ordinal(
        &self, owner: SemanticDefinitionId, node: NodeId,
        inventory: &Inventory<'_>, b: &mut Budget,
    ) -> CompileResult<u32> {
        let definition = inventory.definition(owner, b)?;
        let SemanticDefinitionBody::FunctionWithBody { sites, site_nodes, .. } = &definition.body
            else { return Err(fail("callable Receiver has no original body")); };
        let mut selected = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.node != SemanticOriginalNode::Expression(node) { continue; }
            for site in sites {
                b.spend(site.occurrence_path.len().saturating_add(1), WORK)?;
                if site.id != link.site || !matches!(site.kind,
                    SemanticSiteRecipeKind::OriginalCall | SemanticSiteRecipeKind::Call { .. }) { continue; }
                for row in &definition.implicit_lifetimes {
                    b.spend(row.occurrence_path.len().saturating_add(site.occurrence_path.len() + 1), WORK)?;
                    if row.occurrence_path == site.occurrence_path
                        && selected.replace(row.ordinal).is_some() {
                        return Err(fail("callable Receiver repeats its exact original call occurrence"));
                    }
                }
            }
        }
        selected.ok_or_else(|| fail("callable Receiver lacks its genuine original Call occurrence"))
    }

    fn callable_receiver_operation(
        &self, scope: &SemanticInstance, node: NodeId, inventory: &Inventory<'_>, b: &mut Budget,
    ) -> CompileResult<()> {
        let program = inventory.module.semantic.as_ref()
            .ok_or_else(|| fail("callable Receiver has no actual semantic module"))?;
        let SemanticInstanceTarget::Function(target) = &scope.target else {
            return Err(fail("callable Receiver has no actual function world"));
        };
        let function = inventory.function(target, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } =
            &inventory.definition(scope.definition, b)?.body else {
                return Err(fail("callable Receiver has no complete original body"));
            };
        let mut selected = false;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.node != SemanticOriginalNode::Expression(node) { continue; }
            for site in &program.sites {
                b.spend(1, WORK)?;
                if site.instance != scope.id || site.recipe != link.site
                    || !matches!(site.action, SemanticSiteAction::BodyCall { .. }) { continue; }
                let SemanticSiteAddress::Typed { block, instruction, edge: SemanticSiteEdge::After } = site.at else {
                    return Err(fail("callable Receiver has another actual call boundary"));
                };
                let operation = function.blocks.get(block.0 as usize).filter(|row| row.id == block)
                    .and_then(|row| row.instructions.get(instruction as usize))
                    .ok_or_else(|| fail("callable Receiver has no actual operation"))?;
                let id = match operation.kind {
                    InstructionKind::CallClosureShared { callee, .. }
                    | InstructionKind::CallClosureMut { callee, .. } => match function.locals.get(callee.0 as usize).map(|row| &row.ty) {
                        Some(Type::Closure(id)) => *id,
                        _ => return Err(fail("callable Receiver local loses its complete closure type")),
                    },
                    InstructionKind::CallClosureBorrowed { callee, kind, .. }
                        if matches!(kind, CallableKind::Fn | CallableKind::FnMut) =>
                        match function.values.get(callee.0 as usize) {
                            Some(Type::Address { mutable, inner }) if kind == CallableKind::Fn || *mutable =>
                                match inner.as_ref() { Type::Closure(id) => *id,
                                    _ => return Err(fail("callable Receiver address changes its pointee")) },
                            _ => return Err(fail("callable Receiver has no actual borrow authority")),
                        },
                    _ => return Err(fail("callable Receiver requires a real reusable receiver operation")),
                };
                let closure = inventory.module.closures.get(id.0 as usize).filter(|row| row.id == id)
                    .ok_or_else(|| fail("callable Receiver has no exact actual closure"))?;
                let entry = inventory.module.functions.get(closure.function.0 as usize)
                    .filter(|row| row.id == closure.function)
                    .ok_or_else(|| fail("callable Receiver has no exact actual entry"))?;
                if entry.call_kind != CallKind::Direct
                    || !(crate::stored_callables::receiver_access_head(closure)
                        || crate::stored_callables::constant_result(closure)) || operation.result.is_none()
                    || !matches!(closure.kind, CallableKind::Fn | CallableKind::FnMut) {
                    return Err(fail("callable Receiver lacks its explicit actual head/result relation"));
                }
                if selected { return Err(fail("callable Receiver has competing actual call operations")); }
                selected = true;
            }
        }
        if !selected { return Err(fail("callable Receiver lacks an actual call binding")); }
        Ok(())
    }
}
