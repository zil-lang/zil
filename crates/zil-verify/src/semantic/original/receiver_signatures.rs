//! A lending result is scoped to the current Callable subject. It is neither
//! the creator's receiver nor a lifetime inferred from a closure's first κ.
use super::*;
impl CheckedOriginal {
    pub(in crate::semantic) fn receiver_signature_recipe(
        signature: &SemanticSignatureRecipe,
    ) -> bool {
        matches!(&signature.result,SemanticTypeRecipe::Address{mutable:false,lifetime:SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::OuterOccurrence{operand:SemanticRecipeOperand::Receiver,path,mode:StoredBorrowMode::SharedAddress}),..}if path.is_empty())
    }
    pub(super) fn callable_signature(
        &mut self,
        module: &TypedModule,
        context: &CheckedContext,
        signature: &SemanticSignatureRecipe,
        kind: CallableKind,
        env: &Env,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<Prototype> {
        if !Self::receiver_signature_recipe(signature) {
            return self.signature(module, context, signature, env, active, b);
        }
        if signature.call_kind != CallKind::Direct
            || !matches!(kind, CallableKind::Fn | CallableKind::FnMut)
        {
            return Err(bad(
                "signature-local Receiver is available only to a nonconsuming Callable subject",
            ));
        }
        let life = Life::ReceiverSignature(active.fresh(b)?);
        self.signature_receiver(module, context, signature, env, life, active, b)
    }
    pub(super) fn receiver_closure_signature(
        &mut self,
        module: &TypedModule,
        context: &CheckedContext,
        signature: &SemanticSignatureRecipe,
        owner: SemanticDefinitionId,
        env: &Env,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<Prototype> {
        if signature.call_kind != CallKind::Direct
            || context.definition(module, owner, b)?.kind != SemanticDefinitionKind::Closure
            || env.current()? != owner
        {
            return Err(bad(
                "Receiver result has no exact original closure definition",
            ));
        }
        self.signature_receiver(
            module,
            context,
            signature,
            env,
            Life::ReceiverHead(owner),
            active,
            b,
        )
    }
    fn signature_receiver(
        &mut self,
        module: &TypedModule,
        context: &CheckedContext,
        signature: &SemanticSignatureRecipe,
        env: &Env,
        life: Life,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<Prototype> {
        let owner = env.current()?;
        let previous = self.current.replace(owner);
        let previous_premises = self.premise_environment.take();
        let result = self.signature_inner(module, context, signature, env, Some(life), active, b);
        self.premise_environment = previous_premises;
        self.current = previous;
        result
    }
    /// Only a real inline captured place is justified merely by borrowing the
    /// environment. Indirect projections retain their actual parent lifetime
    /// and the ordinary directed relation checks.
    pub(super) fn inline_capture_place(
        &self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        b: &mut Budget,
    ) -> Check<bool> {
        let mut expression = expression;
        loop {
            b.spend(1)?;
            let (object, array_projection) = match &expression.kind {
                CoreExprKind::Name(_) => return Ok(true),
                CoreExprKind::Group(inner) | CoreExprKind::Ascribed { value: inner, .. } => {
                    expression = inner;
                    continue;
                }
                CoreExprKind::Member { object, .. } => (object, false),
                CoreExprKind::Index { object, .. } | CoreExprKind::Slice { object, .. } => {
                    (object, true)
                }
                _ => return Ok(false),
            };
            let key = (owner, object.id);
            let Some(ty) = self
                .place_types
                .get(&key)
                .or_else(|| self.expressions.get(&key))
            else {
                return Ok(false);
            };
            let ty = self.resolved(ty, b)?;
            let inline = if array_projection {
                matches!(ty, Ty::Array(..))
            } else {
                !matches!(ty, Ty::Address { .. } | Ty::Desc { .. })
            };
            if !inline {
                return Ok(false);
            }
            expression = object;
        }
    }
    /// A view of a by-value captured sealed owner stays anchored in that
    /// exact environment occupant. A borrowed descriptor or a local owner is
    /// not such an anchor, even when its payload type is identical.
    pub(super) fn record_owned_receiver_view(
        &mut self,
        module: &TypedModule,
        owner: SemanticDefinitionId,
        life: &Life,
        b: &mut Budget,
    ) -> Check<()> {
        let Life::Local(definition, node) = self.resolved_life(life, b)? else {
            return Ok(());
        };
        if definition != owner || self.creator_moves.get(&owner) != Some(&true) {
            return Ok(());
        }
        b.spend(
            self.value_places
                .len()
                .checked_ilog2()
                .map_or(1, |n| n as usize + 1),
        )?;
        let Some(place) = self.value_places.get(&(owner, node)) else {
            return Ok(());
        };
        place.binder.charge_clone(b)?;
        if !matches!(place.binder, Binder::Capture { definition, .. } if definition == owner)
            || !self
                .used
                .get(&owner)
                .is_some_and(|used| used.contains(&place.binder))
        {
            return Ok(());
        }
        let Some(ty) = self.place_types.get(&(owner, node)) else {
            return Ok(());
        };
        if !matches!(self.resolved(ty, b)?, Ty::Managed(inner) if inner.as_ref() == &Ty::Atom(Type::ByteBuffer))
        {
            return Ok(());
        }
        let index = self.source_index(module, &[owner], b)?;
        let expression = index
            .definitions
            .get(&owner)
            .ok_or_else(|| bad("owned Receiver view has no exact original owner"))?
            .expression(node,b)?
            .ok_or_else(|| bad("owned Receiver view has no exact original source expression"))?;
        if self.inline_capture_place(owner, expression, b)? {
            b.spend(1)?;
            self.receiver_borrows.insert((owner, node));
        }
        Ok(())
    }
    pub(super) fn receiver_source_life(
        &self,
        owner: SemanticDefinitionId,
        life: &Life,
        seen: &mut BTreeSet<Life>,
        b: &mut Budget,
    ) -> Check<bool> {
        // A contextual result/local may be an upper for several distinct σ.
        // Even a real capture anchor must retain every incoming source; the
        // first capture is not evidence for a different branch's stack view.
        let root = self.resolved_life(life, b)?;
        let mut pending = vec![root.clone()];
        let mut graph: BTreeMap<Life, (bool, BTreeSet<Life>)> = BTreeMap::new();
        let flows = self
            .outer_flows
            .get(&owner)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        while let Some(life) = pending.pop() {
            b.spend(graph.len().checked_ilog2().map_or(1, |n| n as usize + 1))?;
            if graph.contains_key(&life) {
                continue;
            }
            if graph.len() >= b.limits.max_nodes {
                return Err(limit("Receiver result source inventory exceeded"));
            }
            let anchor = life == Life::Static
                || self.receiver_physical_source(owner, &life, b)?
                || matches!(life,Life::Local(definition,node)if definition==owner&&self.receiver_borrows.contains(&(owner,node)));
            let mut sources = BTreeSet::new();
            if let Life::Local(definition, node) = &life {
                if *definition == owner {
                    if let Some(parent) = self.borrow_parents.get(&(owner, *node)) {
                        sources.insert(self.resolved_life(parent, b)?);
                    }
                }
            }
            if let Life::CallRegion(id) = &life {
                if let Some((definition, _, _, rows)) = self.call_regions.get(id) {
                    if *definition == owner {
                        b.spend(rows.len())?;
                        for row in rows {
                            sources.insert(self.resolved_life(row, b)?);
                        }
                    }
                }
            }
            if let Some(rows) = self.local_outer_sources(&life, b)? {
                b.spend(rows.len())?;
                for row in rows {
                    sources.insert(self.resolved_life(&row, b)?);
                }
            }
            b.spend(flows.len())?;
            for (source, target) in flows {
                if self.resolved_life(target, b)? == life {
                    let source = self.resolved_life(source, b)?;
                    if source != life {
                        sources.insert(source);
                    }
                }
            }
            sources.remove(&life);
            if !anchor && sources.is_empty() {
                return Ok(false);
            }
            b.spend(sources.len().saturating_mul(2).saturating_add(1))?;
            pending.extend(sources.iter().cloned());
            graph.insert(life, (anchor, sources));
        }
        // Every reachable dependency must reach an actual capture/static root.
        // This admits a rooted directed cycle, but not a rootless SCC attached
        // to one safe alternative. No equality is introduced by this closure.
        b.spend(graph.len())?;
        let mut grounded: BTreeSet<Life> = graph
            .iter()
            .filter_map(|(life, (anchor, _))| anchor.then_some(life.clone()))
            .collect();
        loop {
            let mut changed = false;
            for (life, (_, sources)) in &graph {
                b.spend(sources.len().saturating_add(1))?;
                if !grounded.contains(life)
                    && sources.iter().any(|source| grounded.contains(source))
                {
                    b.spend(1)?;
                    grounded.insert(life.clone());
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        b.spend(graph.len().saturating_add(grounded.len()))?;
        seen.extend(graph.keys().cloned());
        Ok(grounded.len() == graph.len())
    }
    pub(super) fn receiver_physical_source(
        &self,
        owner: SemanticDefinitionId,
        life: &Life,
        b: &mut Budget,
    ) -> Check<bool> {
        // A physical captured Address keeps its stronger original σ. Desc,
        // phantom arguments and equal numeric images provide no such source.
        if let Some(free) = self.free_values.get(&owner) {
            for binding in free.values() {
                binding.identity.charge_clone(b)?;
                let capture = Binder::Capture {
                    definition: owner,
                    source: Box::new(binding.identity.clone()),
                };
                if !self
                    .used
                    .get(&owner)
                    .is_some_and(|used| used.contains(&capture))
                {
                    continue;
                }
                if let Ty::Address { life: source, .. } = self.resolved(&binding.ty, b)? {
                    if self.receiver_declared_life_equal(owner, life, &source, b)? {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }
    fn receiver_declared_life_equal(
        &self,
        owner: SemanticDefinitionId,
        left: &Life,
        right: &Life,
        b: &mut Budget,
    ) -> Check<bool> {
        let left = self.resolved_life(left, b)?;
        let right = self.resolved_life(right, b)?;
        if left == right {
            return Ok(true);
        }
        let rows = self
            .life_equalities
            .get(&owner)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let mut pending = vec![left];
        let mut seen = BTreeSet::new();
        while let Some(value) = pending.pop() {
            b.spend(rows.len().saturating_add(1))?;
            if !seen.insert(value.clone()) {
                continue;
            }
            for (a, z) in rows {
                let a = self.resolved_life(a, b)?;
                let z = self.resolved_life(z, b)?;
                if a == value {
                    if z == right {
                        return Ok(true);
                    }
                    pending.push(z);
                } else if z == value {
                    if a == right {
                        return Ok(true);
                    }
                    pending.push(a);
                }
            }
        }
        Ok(false)
    }
    fn prove_receiver_result(
        &mut self,
        module: &TypedModule,
        owner: SemanticDefinitionId,
        b: &mut Budget,
    ) -> Check<()> {
        let previous = self.current.replace(owner);
        let result = self.receiver_result_sources(module, owner, b);
        self.current = previous;
        for source in result? {
            b.spend(1)?;
            self.outer_flows
                .entry(owner)
                .or_default()
                .push((source, Life::ReceiverHead(owner)));
        }
        Ok(())
    }
    fn receiver_result_sources(
        &self,
        module: &TypedModule,
        owner: SemanticDefinitionId,
        b: &mut Budget,
    ) -> Check<Vec<Life>> {
        if !self.checked.contains(&owner)
            || !self.prototypes.get(&owner).is_some_and(|p| {
                p.call_kind == CallKind::Direct
                    && matches!(
                        p.callable_kind,
                        Some(CallableKind::Fn | CallableKind::FnMut)
                    )
            })
        {
            return Err(bad(
                "Receiver lending needs a completely checked nonconsuming original closure",
            ));
        }
        let index = self.source_index(module, &[owner], b)?;
        let function = index
            .definitions
            .get(&owner)
            .ok_or_else(|| bad("Receiver lending has no complete original body"))?;
        let mut returns = Vec::new();
        for &node in function.returns() {
            let statement = function.statement(node,b)?.ok_or_else(||bad("original return statement is absent"))?;
            b.spend(1)?;
            let CoreStatementKind::Return(Some(value)) = &statement.kind else {
                continue;
            };
            let ty =
                self.original_type(owner, &SemanticOriginalTypeSource::Expression(value.id), b)?;
            let Ty::Address {
                mutable: false,
                life,
                ..
            } = self.resolved(&ty, b)?
            else {
                return Err(bad(
                    "Receiver lending return changes its root shared Address",
                ));
            };
            if !self.receiver_source_life(owner, &life, &mut BTreeSet::new(), b)? {
                return Err(bad(
                    "Receiver lending return is not grounded in its exact original capture/view",
                ));
            }
            b.spend(1)?;
            returns.push(life);
        }
        if returns.is_empty() {
            return Err(bad("Receiver lending has no proved original return"));
        }
        Ok(returns)
    }
    pub(super) fn adapt_receiver_result_goal(
        &mut self,
        module: &TypedModule,
        context: &CheckedContext,
        receiver: &Ty,
        kind: CallableKind,
        wanted: &Prototype,
        actual: &mut Ty,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<()> {
        let Ty::Address {
            mutable: false,
            life: Life::ReceiverSignature(token),
            ..
        } = &wanted.result
        else {
            return Ok(());
        };
        if !matches!(kind, CallableKind::Fn | CallableKind::FnMut) {
            return Err(bad(
                "consuming Callable cannot use a Receiver lending result",
            ));
        }
        let provider = match self.resolved(receiver, b)? {
            Ty::Closure(id, ..) => Some(id),
            Ty::Opaque(owner, arguments) => {
                match self.opaque_hidden(module, context, owner, &arguments, active, b)? {
                    Ty::Closure(id, ..) => Some(id),
                    _ => None,
                }
            }
            _ => None,
        };
        if let Some(owner) = provider {
            self.prove_receiver_result(module, owner, b)?;
            b.spend(1)?;
            self.receiver_results.insert(owner);
        } else if !matches!(
            actual,
            Ty::Address {
                mutable: false,
                life: Life::ReceiverSignature(_),
                ..
            }
        ) {
            return Err(bad(
                "Receiver lending source lacks its complete original Callable premise",
            ));
        }
        let Ty::Address {
            mutable: false,
            life,
            ..
        } = actual
        else {
            return Err(bad(
                "Receiver lending provider changes its shared result mode",
            ));
        };
        // Only this result's root upper is adapted; every invariant inner κ,
        // nominal argument and phantom occurrence is compared by the caller.
        *life = Life::ReceiverSignature(*token);
        Ok(())
    }
    pub(in crate::semantic) fn receiver_result_proven(
        &self,
        owner: SemanticDefinitionId,
        b: &mut Budget,
    ) -> Check<bool> {
        b.spend(1)?;
        Ok(self.checked.contains(&owner)
            && self.receiver_results.contains(&owner)
            && self.prototypes.get(&owner).is_some_and(|p| {
                p.call_kind == CallKind::Direct
                    && matches!(
                        p.callable_kind,
                        Some(CallableKind::Fn | CallableKind::FnMut)
                    )
            }))
    }
    pub(in crate::semantic) fn original_call_node(
        &self,
        _module: &TypedModule,
        owner: SemanticDefinitionId,
        site: SemanticSiteId,
        b: &mut Budget,
    ) -> Check<NodeId> {
        let grammar = self.source_grammar.as_ref()
            .ok_or_else(|| bad("Receiver call has no original function"))?;
        let Some(SemanticOriginalNode::Expression(node)) = grammar.site_node(owner, site, b)? else {
            return Err(bad("Receiver call site has no exact original expression"));
        };
        let expression = grammar.place_syntax(owner, b)?.expression(node, b)?;
        if !matches!(expression.operation, index::PlaceOperation::Call { .. })
            || !self.callable_nodes.contains_key(&(owner, node))
        {
            return Err(bad("Receiver call selects another original operation"));
        }
        Ok(node)
    }
    pub(super) fn receiver_call_life(
        &self,
        module: &TypedModule,
        owner: SemanticDefinitionId,
        node: NodeId,
        b: &mut Budget,
    ) -> Check<Life> {
        let index = self.source_index(module, &[owner], b)?;
        let function = index
            .definitions
            .get(&owner)
            .ok_or_else(|| bad("Receiver call has no original caller"))?;
        let (position, site) = self.original_call_site(owner, node, b)?;
        let site = function
            .sites
            .get(position)
            .filter(|row| row.id == site)
            .ok_or_else(|| bad("Receiver call selects another original occurrence"))?;
        b.spend(function.definition.implicit_lifetimes.len())?;
        let mut selected = None;
        for row in &function.definition.implicit_lifetimes {
            b.spend(
                row.occurrence_path
                    .len()
                    .saturating_add(site.occurrence_path.len()),
            )?;
            if row.occurrence_path == site.occurrence_path {
                if selected.replace(row.ordinal).is_some() {
                    return Err(bad("Receiver call has multiple original access windows"));
                }
            }
        }
        Ok(Life::Implicit(
            owner,
            selected.ok_or_else(|| {
                bad("Receiver call lacks its original full-occurrence access window")
            })?,
        ))
    }
    pub(super) fn bind_receiver_call(
        &mut self,
        module: &TypedModule,
        _context: &CheckedContext,
        mut prototype: Prototype,
        node: NodeId,
        borrowed_head: Option<&Life>,
        env: &Env,
        b: &mut Budget,
    ) -> Check<Prototype> {
        let Ty::Address {
            mutable: false,
            life,
            ..
        } = &mut prototype.result
        else {
            return Ok(prototype);
        };
        if !matches!(life, Life::ReceiverSignature(_) | Life::ReceiverHead(_)) {
            return Ok(prototype);
        }
        if prototype.call_kind != CallKind::Direct
            || !matches!(
                prototype.callable_kind,
                Some(CallableKind::Fn | CallableKind::FnMut)
            )
        {
            return Err(bad("Receiver result is not a real nonconsuming call"));
        }
        let owner = env.current()?;
        let access = self.receiver_call_life(module, owner, node, b)?;
        // Calling through a real Address<F> enters the existing referenced
        // environment, not the local pointer cell. Its checked outer life is
        // a valid result upper for the Receiver contract. An owned/local F
        // still publishes only this call's fresh access window.
        let target = match borrowed_head {
            Some(head) => self.resolved_life(head, b)?,
            None => access.clone(),
        };
        b.spend(3)?;
        let bounds = (access.clone(), target.clone());
        if let Some(previous) = self
            .receiver_call_roles
            .insert((owner, node), bounds.clone())
        {
            if previous != bounds {
                return Err(bad(
                    "one original Receiver call changes its access-window identity",
                ));
            }
        }
        self.outer_flows.entry(owner).or_default().push((
            Life::ReceiverRole(owner, node, StoredBorrowMode::SharedAddress),
            access.clone(),
        ));
        if target != access {
            b.spend(1)?;
            self.outer_flows.entry(owner).or_default().push((
                Life::ReceiverRole(owner, node, StoredBorrowMode::SharedAddress),
                target.clone(),
            ));
        }
        *life = target;
        Ok(prototype)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn recipe(result: SemanticTypeRecipe) -> SemanticSignatureRecipe {
        SemanticSignatureRecipe {
            parameters: vec![],
            result,
            result_mode: PassingMode::Infer,
            call_kind: CallKind::Direct,
            bound_lifetimes: 0,
            invocation_lifetimes: 0,
        }
    }
    fn receiver(inner: SemanticTypeRecipe) -> SemanticTypeRecipe {
        SemanticTypeRecipe::Address {
            mutable: false,
            lifetime: SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::OuterOccurrence {
                operand: SemanticRecipeOperand::Receiver,
                path: vec![],
                mode: StoredBorrowMode::SharedAddress,
            }),
            inner: Box::new(inner),
        }
    }
    #[test]
    fn receiver_signature_has_a_private_subject_and_no_code_or_inner_escape() {
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        let (module, _) = zil_ir::wire::decode_module_candidate(
            include_bytes!("../fixtures/ordinary-identity-complete-head.wire"),
            &limits,
        )
        .unwrap();
        let context = CheckedContext::catalogue(&module, &mut b).unwrap();
        let mut active = ActiveQueries::new();
        let mut checked = CheckedOriginal::default();
        let id = module
            .semantic
            .as_ref()
            .unwrap()
            .definitions
            .iter()
            .find(|row| matches!(row.kind, SemanticDefinitionKind::Function))
            .unwrap()
            .id;
        let env = Env {
            lexical: vec![Frame {
                definition: id,
                actuals: vec![],
                environment: vec![],
            }],
            bounds: vec![],
            invocation: vec![],
            self_type: None,
        };
        let nested = SemanticTypeRecipe::Address {
            mutable: false,
            lifetime: SemanticOuterLifeRecipe::Static,
            inner: Box::new(SemanticTypeRecipe::Atom(Type::I32)),
        };
        let signature = recipe(receiver(nested));
        let a = checked
            .callable_signature(
                &module,
                &context,
                &signature,
                CallableKind::Fn,
                &env,
                &mut active,
                &mut b,
            )
            .unwrap();
        let z = checked
            .callable_signature(
                &module,
                &context,
                &signature,
                CallableKind::FnMut,
                &env,
                &mut active,
                &mut b,
            )
            .unwrap();
        let (Ty::Address { life: a, inner, .. }, Ty::Address { life: z, .. }) =
            (&a.result, &z.result)
        else {
            panic!("shared root role")
        };
        assert!(matches!(a, Life::ReceiverSignature(_)));
        assert!(!checked.same_life(a, z, &mut b).unwrap());
        assert!(
            matches!(
                inner.as_ref(),
                Ty::Address {
                    life: Life::Static,
                    ..
                }
            ),
            "the complete inner lifetime stays invariant"
        );
        assert!(
            checked
                .signature(&module, &context, &signature, &env, &mut active, &mut b)
                .is_err(),
            "ordinary code has no receiver subject"
        );
        for kind in [CallableKind::FnOnce, CallableKind::FunctionPointer] {
            assert!(
                checked
                    .callable_signature(
                        &module,
                        &context,
                        &signature,
                        kind,
                        &env,
                        &mut active,
                        &mut b
                    )
                    .is_err()
            );
        }
        let mut suspended = signature.clone();
        suspended.call_kind = CallKind::Suspend;
        assert!(
            checked
                .callable_signature(
                    &module,
                    &context,
                    &suspended,
                    CallableKind::Fn,
                    &env,
                    &mut active,
                    &mut b
                )
                .is_err()
        );
        let inside = recipe(receiver(receiver(SemanticTypeRecipe::Atom(Type::I32))));
        assert!(
            checked
                .callable_signature(
                    &module,
                    &context,
                    &inside,
                    CallableKind::Fn,
                    &env,
                    &mut active,
                    &mut b
                )
                .is_err(),
            "a subject role does not enter a referent"
        );
        let code = SemanticTypeRecipe::Code {
            kind: SemanticCodeKind::FunctionPointer,
            signature: Box::new(signature),
        };
        assert!(
            checked
                .ty(&module, &context, &code, &env, &mut active, &mut b)
                .is_err()
        );
    }
    #[test]
    fn receiver_join_proves_all_directed_sources_without_equating_branch_windows() {
        let mut checked = CheckedOriginal::default();
        let mut b = Budget::new(&Limits::default());
        let owner = SemanticDefinitionId {
            module: zil_base::ModuleId(0),
            index: 0,
        };
        let first = Life::Local(owner, NodeId(1));
        let second = Life::Local(owner, NodeId(2));
        let alias = Life::Local(owner, NodeId(3));
        let stack = Life::Local(owner, NodeId(4));
        checked
            .receiver_borrows
            .extend([(owner, NodeId(1)), (owner, NodeId(2))]);
        checked
            .borrow_parents
            .insert((owner, NodeId(3)), first.clone());
        checked
            .outer_flows
            .insert(owner, vec![(second.clone(), alias.clone())]);
        assert!(
            checked
                .receiver_source_life(owner, &alias, &mut BTreeSet::new(), &mut b)
                .unwrap(),
            "both original return/call sources belong to the same real environment"
        );
        assert!(
            !checked.same_life(&first, &second, &mut b).unwrap(),
            "the two actual borrow σ stay distinct"
        );
        checked
            .outer_flows
            .get_mut(&owner)
            .unwrap()
            .push((stack.clone(), first.clone()));
        assert!(
            !checked
                .receiver_source_life(owner, &alias, &mut BTreeSet::new(), &mut b)
                .unwrap(),
            "a capture anchor cannot hide another branch's local stack source"
        );
        checked.outer_flows.insert(
            owner,
            vec![
                (second.clone(), alias.clone()),
                (alias.clone(), first.clone()),
            ],
        );
        assert!(
            checked
                .receiver_source_life(owner, &alias, &mut BTreeSet::new(), &mut b)
                .unwrap(),
            "a directed cycle still has real capture roots"
        );
        let cycle = Life::Local(owner, NodeId(5));
        checked.outer_flows.get_mut(&owner).unwrap().extend([
            (cycle.clone(), stack.clone()),
            (stack.clone(), cycle),
            (stack, alias.clone()),
        ]);
        assert!(
            !checked
                .receiver_source_life(owner, &alias, &mut BTreeSet::new(), &mut b)
                .unwrap(),
            "a separate rootless component cannot borrow the safe branch's anchor"
        );
    }
    #[test]
    fn receiver_source_requires_every_real_capture_anchor_without_lifetime_equality() {
        let mut checked = CheckedOriginal::default();
        let mut b = Budget::new(&Limits::default());
        let owner = SemanticDefinitionId {
            module: zil_base::ModuleId(0),
            index: 0,
        };
        let other = SemanticDefinitionId {
            module: zil_base::ModuleId(0),
            index: 1,
        };
        let root = Life::Local(owner, NodeId(3));
        let ungrounded = Life::Local(owner, NodeId(4));
        checked.receiver_borrows.insert((owner, NodeId(3)));
        assert!(
            checked
                .receiver_source_life(owner, &root, &mut BTreeSet::new(), &mut b)
                .unwrap()
        );
        assert!(
            !checked
                .receiver_source_life(other, &root, &mut BTreeSet::new(), &mut b)
                .unwrap()
        );
        assert!(
            !checked
                .receiver_source_life(owner, &ungrounded, &mut BTreeSet::new(), &mut b)
                .unwrap()
        );
        checked.call_regions.insert(
            1,
            (owner, NodeId(5), vec![], vec![root.clone(), ungrounded]),
        );
        assert!(
            !checked
                .receiver_source_life(owner, &Life::CallRegion(1), &mut BTreeSet::new(), &mut b)
                .unwrap(),
            "one safe return source cannot erase an unsafe branch"
        );
        assert!(
            !checked
                .same_life(&root, &Life::ReceiverHead(owner), &mut b)
                .unwrap()
        );
        let formal = Life::Formal(owner, 0);
        let binding = Binder::Parameter {
            definition: other,
            ordinal: 0,
        };
        let capture = Binder::Capture {
            definition: owner,
            source: Box::new(binding.clone()),
        };
        checked.used.insert(owner, BTreeSet::from([capture]));
        checked.free_values.insert(
            owner,
            BTreeMap::from([(
                "ghost".into(),
                Binding {
                    ty: Ty::Nominal(other, vec![Arg::Life(formal.clone())]),
                    mutable: false,
                    identity: binding.clone(),
                },
            )]),
        );
        assert!(
            !checked
                .receiver_source_life(owner, &formal, &mut BTreeSet::new(), &mut b)
                .unwrap(),
            "a phantom lifetime is no physical capture head"
        );
        checked
            .free_values
            .get_mut(&owner)
            .unwrap()
            .get_mut("ghost")
            .unwrap()
            .ty = Ty::Address {
            mutable: false,
            life: formal.clone(),
            inner: Box::new(Ty::Atom(Type::I32)),
        };
        assert!(
            checked
                .receiver_source_life(owner, &formal, &mut BTreeSet::new(), &mut b)
                .unwrap()
        );
        assert!(
            !checked
                .same_life(&formal, &Life::ReceiverHead(owner), &mut b)
                .unwrap()
        );
    }
}
