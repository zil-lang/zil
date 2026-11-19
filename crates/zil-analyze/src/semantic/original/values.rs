//! Instantiate an already independently checked original term. This never
//! feeds executable class coincidences back into the rigid original solver.
use super::*;
use crate::semantic::value::{self, Argument, Class, Value};

impl Checked {
    pub(in crate::semantic) fn call_receiver_view(
        &self, context: &SemanticInstance, site: SemanticSiteId,
        inventory: &Inventory<'_>, b: &mut Budget,
    ) -> CompileResult<bool> {
        let Some(link) = inventory.original_index(context.definition, b)?.site_node(site, b)? else { return Ok(false); };
        let SemanticOriginalNode::Expression(node) = link else {
            return Err(fail("Receiver call changes its original expression kind"));
        };
        b.lookup(self.frames.len(), WORK)?;
        let frame = self.frames.get(&context.definition)
            .ok_or_else(|| fail("Receiver call lacks its checked original frame"))?;
        b.lookup(frame.expressions.len(), WORK)?;
        let Some(term) = frame.expressions.get(&node) else { return Ok(false); };
        let root = self.receiver_value_root(*term, b)?;
        let Kind::Address { mutable: false, life, .. } = self.arena[root] else { return Ok(false); };
        Ok(matches!(self.arena[self.root(life, b)?],
            Kind::Life(Life::Receiver(owner, call)) if owner == context.definition && call == node))
    }

    pub(in crate::semantic) fn call_output(&self, context: &SemanticInstance, site: SemanticSiteId,
        program: &SemanticModule, inventory: &Inventory<'_>, b: &mut Budget) -> CompileResult<Value> {
        let node = site_expression(context.definition, site, inventory, b,
            "call output has no original expression", "call output changes its original expression role")?;
        b.lookup(self.calls.len(), WORK)?;
        let ordinary = self.calls.contains_key(&(context.definition, node));
        b.lookup(self.trait_calls.len(), WORK)?;
        if !ordinary && !self.trait_calls.contains_key(&(context.definition, node)) {
            return Err(fail("call output has no independently checked original call"));
        }
        self.source_value(context, SemanticScopeRef::Ancestor(0),
            &SemanticOriginalTypeSource::Expression(node), program, inventory, b)
    }

    /// The caller-domain mixed vector is independently inferred from this
    /// exact original call. It is not reconstructed from an actual callee's
    /// coincident class images or from a representative specialization.
    pub(in crate::semantic) fn call_arguments(&self, context: &SemanticInstance,
        site: SemanticSiteId, program: &SemanticModule, inventory: &Inventory<'_>,
        b: &mut Budget) -> CompileResult<(SemanticDefinitionId, Vec<Argument>)> {
        let node = site_expression(context.definition, site, inventory, b,
            "call mixed vector has no original occurrence", "call mixed vector selects an original statement")?;
        b.lookup(self.calls.len(), WORK)?;
        let call = self.calls.get(&(context.definition, node))
            .ok_or_else(|| fail("call mixed vector has no independently checked original scheme"))?;
        let mut arguments = Vec::new();
        for argument in &call.actuals {
            Solver::push(&mut arguments, self.argument(*argument, context, program, inventory, b, 0)?, b)?;
        }
        Ok((call.target, arguments))
    }

    pub(in crate::semantic) fn description_source(
        &self,
        context: &SemanticInstance,
        site: SemanticSiteId,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<Option<descriptions::Source>> {
        let Some(SemanticOriginalNode::Expression(node)) = inventory.original_index(context.definition, b)?.site_node(site, b)? else { return Ok(None); };
        b.lookup(self.descriptions.len(), WORK)?;
        let Some(source) = self.descriptions.get(&(context.definition, node)) else { return Ok(None); };
        b.spend(source.path.len(), WORK)?;
        Ok(Some(descriptions::Source {
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
        }))
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
    /// A Fixed ordinal names a slot in the closure ABI. Distinct slots may
    /// carry the same creator lifetime (including slots of different captures).
    fn intrinsic_capture_images(
        child: &SemanticInstance,
        captures: &[Value],
        count: u32,
        b: &mut Budget,
    ) -> CompileResult<Option<Vec<Class>>> {
        b.spend(child.header_embedding.len(), WORK)?;
        let fixed = child.header_embedding.iter().filter(|row| matches!(row.header, StorageBinder::Fixed(_)));
        if !fixed.clone().all(|row| matches!(&row.actual,
            SemanticClassExpr::IntrinsicClass { operand: SemanticOperand::Environment(_), path, .. }
            if path.is_empty())) {
            return Ok(None);
        }
        if fixed.clone().count() != count as usize {
            return Err(fail("closure Fixed directory changes its complete header count"));
        }
        let mut images = vec![None; count as usize];
        let mut covered = BTreeSet::new();
        for row in fixed {
            let StorageBinder::Fixed(index) = row.header else { unreachable!() };
            let SemanticClassExpr::IntrinsicClass { operand: SemanticOperand::Environment(ordinal), class, .. }
                = &row.actual else { unreachable!() };
            let original = captures.get(*ordinal as usize).and_then(|v| v.classes.get(*class as usize))
                .ok_or_else(|| fail("closure Fixed header leaves its original capture domain"))?;
            // The header selects an occurrence. Its image belongs to the
            // current original instantiation, which can be a caller world.
            let slot = images.get_mut(index as usize)
                .ok_or_else(|| fail("closure Fixed header ordinal is out of range"))?;
            if slot.replace(original.clone()).is_some() {
                return Err(fail("closure Fixed header ordinal is repeated"));
            }
            b.lookup(covered.len(), WORK)?;
            covered.insert((*ordinal as usize, *class as usize));
        }
        for (ordinal, capture) in captures.iter().enumerate() {
            for class in 0..capture.classes.len() {
                b.lookup(covered.len(), WORK)?;
                if !covered.contains(&(ordinal, class)) {
                    return Err(fail("closure Fixed directory omits an original capture class"));
                }
            }
        }
        images.into_iter().map(|image| image.ok_or_else(|| fail("closure Fixed header has no source")))
            .collect::<CompileResult<Vec<_>>>().map(Some)
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
        let root = self.root(term, b)?;
        let Some(cache) = &self.closed_values else {
            return self.value_at(root, context, program, inventory, b, depth);
        };
        if matches!(self.arena[root], Kind::Atom(_)) || !std::ptr::eq(program, inventory.program) {
            return self.value_at(root, context, program, inventory, b, depth);
        }
        let key = (context as *const SemanticInstance as usize, depth, b.partial_depth, b.helper_nodes);
        b.spend(1, WORK)?;
        if let Some(rows) = cache.borrow()[root].as_ref() {
            b.lookup(rows.len(), WORK)?;
            if let Some(value) = rows.get(&key) { return value.copy(b); }
        }
        let value = self.value_at(root, context, program, inventory, b, depth)?;
        // Synthetic contexts can carry different substituted actuals even
        // when their numerical instance IDs coincide. Publish only a value
        // of the immutable instance held by this original inventory.
        // These are declared class identities of this exact original world.
        // Fresh physical reborrows, returned regions and temporary shape
        // atoms still require their complete source-occurrence selection.
        b.spend(value.classes.len(), WORK)?;
        if value.classes.iter().all(|class| matches!(class, Class::Formal(..)
            | Class::Implicit(..) | Class::Invocation(..) | Class::TypeArgument(..) | Class::Nominal(..))) {
            b.lookup(inventory.instances.len(), WORK)?;
            if inventory.instances.get(&context.id).is_some_and(|held| std::ptr::eq(context, *held)) {
                b.spend(1, WORK)?;
                let mut terms = cache.borrow_mut();
                let rows = terms[root].get_or_insert_with(Default::default);
                b.lookup(rows.len(), WORK)?;
                rows.insert(key, value.copy(b)?);
            }
        }
        Ok(value)
    }

    fn closed_formation(
        &self, definition: SemanticDefinitionId, args: &[Argument], fields: &[Value], depth: usize, b: &mut Budget,
    ) -> CompileResult<Option<ClosedNominalFormation>> {
        b.spend(args.len().saturating_add(fields.len()), WORK)?;
        let mut actuals = Vec::with_capacity(args.len());
        for argument in args {
            let Argument::Type(value) = argument else { return Ok(None); };
            let Some(id) = value.closed_geometry_identity(b)? else { return Ok(None); };
            actuals.push(id);
        }
        let mut parts = Vec::with_capacity(fields.len());
        for field in fields {
            let Some(id) = field.closed_geometry_identity(b)? else { return Ok(None); };
            parts.push(id);
        }
        Ok(Some(ClosedNominalFormation { definition, depth, depth_limit: b.partial_depth,
            node_limit: b.helper_nodes, actuals, fields: parts }))
    }

    fn value_at(
        &self,
        root: Term,
        context: &SemanticInstance,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        match &self.arena[root] {
            Kind::Atom(ty) => value::borrowed_scalar(ty, inventory, b),
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
                let key = if self.closed_values.is_some() && std::ptr::eq(program, inventory.program) {
                    self.closed_formation(*definition, &actuals, &physical, depth + 1, b)?
                } else { None };
                let words = physical.len().saturating_add(actuals.len()).saturating_add(4);
                if let Some(key) = &key {
                    let cache = self.closed_formations.borrow();
                    b.product(cache.len().checked_ilog2().map_or(1, |n| n as usize + 2), words, 1, WORK)?;
                    if let Some(formed) = cache.get(key) { return formed.copy(b); }
                }
                let formed = value::nominal(*definition, &actuals, &physical, program, self, inventory, b, depth + 1)?;
                if let Some(key) = key && formed.closed_geometry_identity(b)?.is_some() {
                    let mut cache = self.closed_formations.borrow_mut();
                    b.product(cache.len().checked_ilog2().map_or(1, |n| n as usize + 2), words, 1, WORK)?;
                    cache.insert(key, formed.copy(b)?);
                }
                Ok(formed)
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
                if *kind == SemanticCodeKind::BorrowedFunctionPointer {
                    let mut physical = Vec::new();
                    for parameter in parameters {
                        let ty = self.invocation_code_part(*parameter, context, program, inventory, b, depth + 1)?;
                        Solver::push(&mut physical, ty, b)?;
                    }
                    let result = self.invocation_code_part(*result, context, program, inventory, b, depth + 1)?;
                    return value::invocation_code(physical, result, b);
                }
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
                    Solver::push(&mut types, field, b)?;
                }
                value::tuple(types, b)
            }
            Kind::Closure { .. } => self.closure_value(
                root, context, context, None, program, inventory, b, depth + 1,
            ),
            _ => Err(unknown(
                "original complete type remains unresolved independently of executable images",
            )),
        }
    }
}

impl Checked {
    #[allow(clippy::too_many_arguments)]
    fn closure_value(
        &self,
        term: Term,
        context: &SemanticInstance,
        identity_context: &SemanticInstance,
        returned_entry: Option<ClosureId>,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(resource("original closure value exceeds its finite depth"));
        }
        let Kind::Closure { definition, environment, .. } = &self.arena[self.root(term, b)?] else {
            return Err(fail("original closure value has another symbolic type"));
        };
        let source = inventory.definition(*definition, b)?;
        let lexical_parent = source.parent.ok_or_else(|| fail("original closure has no lexical creator"))?;
        let mut owner = None;
        let mut current = Some(identity_context);
        for _ in 0..=b.partial_depth {
            let Some(world) = current else { break; };
            b.spend(1, WORK)?;
            if world.definition == *definition {
                owner = Some((true, world.id));
                break;
            }
            if world.definition == lexical_parent {
                owner = Some((false, world.id));
                break;
            }
            current = world.parent.map(|id| inventory.instance(id, b)).transpose()?;
        }
        if current.is_some() && owner.is_none() {
            return Err(resource("original closure creator exceeds its finite lexical depth"));
        }
        let mut selected = None;
        let mut unique_child = None;
        let mut ambiguous_child = false;
        for child in &program.instances {
            b.spend(1, WORK)?;
            if child.definition != *definition || owner.is_some_and(|(exact, id)| {
                if exact { child.id != id } else { child.parent != Some(id) }
            }) {
                continue;
            }
            let Some(environment) = &child.environment else { continue; };
            let SemanticClosureTarget::Typed(id) = environment.closure else {
                return Err(fail("original closure type retains a Core target"));
            };
            if returned_entry.is_some_and(|returned| returned != id) {
                if owner.is_some() { return Err(fail("original closure return changes its exact lexical creator entry")); }
                continue;
            }
            if selected.replace(id).is_some_and(|previous| previous != id) {
                return Err(fail("original closure Type has multiple actual entry identities in its creator domain"));
            }
            ambiguous_child |= unique_child.replace(child).is_some();
        }
        let id = selected.ok_or_else(|| fail("original closure Type has no exact actual creator instance"))?;
        let entry = inventory.module.closures.get(id.0 as usize).filter(|entry| entry.id == id)
            .ok_or_else(|| fail("original closure has no actual inventory"))?;
        let count = entry.storage_life.as_ref()
            .ok_or_else(|| unknown("original closure has no complete intrinsic scope"))?.fixed_binder_count;
        let mut captures = Vec::new();
        let mut classes = Vec::new();
        for capture in environment {
            let value = self.value(*capture, context, program, inventory, b, depth + 1)?;
            for class in &value.classes {
                value::intern(&mut classes, class.clone(), b)?;
            }
            Solver::push(&mut captures, value, b)?;
        }
        // Extra ABI classes must be read from the exact selected creator.
        // Capture values themselves are transported in the caller domain.
        if classes.len() < count as usize {
            let child = unique_child.filter(|_| !ambiguous_child)
                .ok_or_else(|| fail("closure scope has no unique actual child at its lexical creator"))?;
            if let Some(images) = Self::intrinsic_capture_images(child, &captures, count, b)? {
                classes = images;
            } else {
                for binding in &child.header_embedding {
                    b.spend(1, WORK)?;
                    if matches!(binding.header, StorageBinder::Fixed(_))
                        && let SemanticClassExpr::Implicit {
                            scope: SemanticScopeRef::Ancestor(0), ordinal,
                        } = binding.actual
                    {
                        b.spend(source.implicit_lifetimes.len(), WORK)?;
                        if !source.implicit_lifetimes.iter().any(|row| row.ordinal == ordinal) {
                            return Err(fail("closure head class has no original occurrence"));
                        }
                        value::intern(&mut classes, Class::Implicit(child.id, ordinal), b)?;
                    }
                }
            }
        }
        if count as usize != classes.len() {
            return Err(fail("actual closure scope omits original environment or phantom classes"));
        }
        Ok(Value::new(Type::Closure(id), StorageLifeShape {
            binder_count: count,
            slots: Vec::new(),
            retained: Vec::new(),
            nominal_arguments: Vec::new(),
        }, classes))
    }
}

impl Checked {
    #[allow(clippy::too_many_arguments)]
    fn invocation_code_part(
        &self,
        term: Term,
        context: &SemanticInstance,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<(Type, Option<u32>)> {
        let term = self.root(term, b)?;
        if let Kind::Address { mutable, life, inner } = self.arena[term] {
            let Kind::Life(Life::Invocation(_, ordinal)) = self.arena[self.root(life, b)?] else {
                return Err(fail("borrowed code part loses its original invocation role"));
            };
            let inner = self.value(inner, context, program, inventory, b, depth + 1)?;
            if inner.life != value::empty() || !inner.classes.is_empty() {
                return Err(unknown("borrowed code pointee needs its complete nested class relation"));
            }
            Ok((Type::Address { mutable, inner: Box::new(inner.into_type(b)?) }, Some(ordinal)))
        } else {
            let actual = self.value(term, context, program, inventory, b, depth + 1)?;
            if actual.life != value::empty() || !actual.classes.is_empty() {
                return Err(unknown("borrowed code data needs its complete nested class relation"));
            }
            Ok((actual.into_type(b)?, None))
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
            return value::borrowed_scalar(ty, inventory, b);
        }
        // A frozen original call retains its complete declaration type but
        // has no runtime Call result to seed. Observe its plain value only
        // within the original erasure boundary. The outer query's TypeUse
        // must separately match its actual transcribed producer tree.
        if inventory.original_index(caller, b)?.erased(node, b)? {
            let mut hidden = self.root(hidden, b)?;
            for _ in 0..=b.partial_depth {
                if let Kind::CallResult { hidden: next, .. } = self.arena[hidden] {
                    hidden = self.root(next, b)?;
                    continue;
                }
                let value = self.value(hidden, context, program, inventory, b, depth + 1)?;
                super::super::closed::ct_data::serializable(&value.ty, b, depth + 1)?;
                if value.life != value::empty() || !value.classes.is_empty() {
                    return Err(fail("original erased call retains non-serializable classes"));
                }
                return Ok(value);
            }
            return Err(resource("erased original call result exceeds its bounded depth"));
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
        let mut closure_result: Option<Value> = None;
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
            let returns_closure = matches!(self.arena[self.root(hidden, b)?], Kind::Closure { .. });
            let mut worlds = 0usize;
            for instance in inventory.worlds(program, target, b)? {
                if instance.definition != call.target {
                    return Err(fail("original result role selects another actual callee"));
                }
                worlds = worlds.saturating_add(1);
                if returns_closure {
                    // Derive the entry inside the exact actual callee's original
                    // result scope. A forwarding body can resolve another
                    // original call there. The outer capture types and class
                    // substitution still belong to this caller's scheme.
                    b.lookup(self.frames.len(), WORK)?;
                    let frame = self.frames.get(&instance.definition)
                        .ok_or_else(|| fail("opaque call result has no checked declaration frame"))?;
                    let origin = self.value(frame.result, instance, program, inventory, b, depth + 1)?;
                    let Type::Closure(entry) = origin.ty else {
                        return Err(fail("opaque call result has no original closure creator"));
                    };
                    let value = self.closure_value(hidden, context, instance, Some(entry), program, inventory, b, depth + 1)?;
                    if let Some(previous) = &closure_result {
                        if !previous.equal_paid(&value, b)? {
                            return Err(fail("actual callee worlds disagree on their original returned closure"));
                        }
                    } else {
                        closure_result = Some(value);
                    }
                }
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
                // The original scheme supplies this result in the caller's
                // domain. Each proposed mixed actual may name a real prepared
                // SSA operand in that domain, so the mandatory closed BodyCall
                // proof resolves it with its actual boundary and checks every
                // target world before publishing any result.
                for world in inventory.worlds(program, target, b)? {
                    if world.actuals.len() != call.actuals.len() {
                        return Err(fail("shared result world changes its full mixed arity"));
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
        let result = match closure_result {
            Some(value) => value,
            None => self.value(hidden, context, program, inventory, b, depth + 1)?,
        };
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
                        || crate::stored_callables::constant_result(closure)
                        || self.fixed_shared_capture_result(closure, inventory, b)?) || operation.result.is_none()
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

    pub(in crate::semantic) fn fixed_shared_capture_result(
        &self, closure: &ClosureDefinition, inventory: &Inventory<'_>, b: &mut Budget,
    ) -> CompileResult<bool> {
        let Some(life) = closure.storage_life.as_ref() else { return Ok(false); };
        if !matches!(closure.kind, CallableKind::Fn | CallableKind::FnMut)
            || !matches!(closure.result, Type::Address { mutable: false, .. })
        {
            return Ok(false);
        }
        crate::signature_life::charge(life, b)?;
        let Some(binder @ StorageBinder::Fixed(_)) =
            crate::signature_life::outer(life, StorageBase::Result)
        else { return Ok(false); };
        if !life.bindings.iter().any(|row| {
            matches!(row.position.base, StorageBase::Capture(_))
                && row.position.path.is_empty() && row.binder == binder
        }) && !life.classes.iter().any(|row| {
            matches!(row.position.base, StorageBase::Capture(_)) && row.binder == binder
        }) { return Ok(false); }
        let program = inventory.module.semantic.as_ref()
            .ok_or_else(|| fail("Fixed captured result has no original semantic module"))?;
        let worlds = inventory.worlds(program, closure.function, b)?;
        if worlds.is_empty() { return Ok(false); }
        for world in worlds {
            if !world.environment.as_ref().is_some_and(|env|
                env.closure == SemanticClosureTarget::Typed(closure.id))
                || !self.receiver_sources(world.definition, inventory, b)?
                    .is_some_and(|sources| !sources.is_empty())
            {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
