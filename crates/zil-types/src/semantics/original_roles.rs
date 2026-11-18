//! Constructive outer-role inference for mutable raw address/descriptor locals.
//! The fresh role is a real Source declaration occurrence. Its source bounds
//! never become exact lifetime equalities or nominal normalization premises.
use super::*;
impl Check<'_, '_> {
    fn local_lifetime(
        &mut self,
        owner: SemanticDefinitionId,
        statement: NodeId,
        depth: usize,
    ) -> R<SemanticLifeRecipe> {
        let definition = self.definition(owner)?;
        let SemanticDefinitionBody::FunctionWithBody {
            sites, site_nodes, ..
        } = &definition.body
        else {
            return Err(bad("original local role has no declaration body"));
        };
        self.pay(
            sites
                .len()
                .saturating_add(site_nodes.len())
                .saturating_add(definition.implicit_lifetimes.len()),
            depth,
        )?;
        let mut selected = None;
        for row in site_nodes {
            if row.node != SemanticOriginalNode::Statement(statement) {
                continue;
            }
            let Some(site) = sites.iter().find(|site| {
                site.id == row.site
                    && matches!(site.kind, SemanticSiteRecipeKind::DeclareLocal { .. })
            }) else {
                continue;
            };
            for lifetime in &definition.implicit_lifetimes {
                if lifetime.occurrence_path == site.occurrence_path {
                    if selected.replace(lifetime.ordinal).is_some() {
                        return Err(bad("original local has several inferred role occurrences"));
                    }
                }
            }
        }
        Ok(SemanticLifeRecipe::Implicit {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal: selected.ok_or_else(|| {
                pending(
                    "original inferred local role requires its real Source declaration occurrence",
                )
            })?,
        })
    }
    fn outer<'t>(
        &mut self,
        ty: &'t SemanticTypeRecipe,
        depth: usize,
    ) -> R<
        Option<(
            LocalRoleKind,
            &'t SemanticOuterLifeRecipe,
            &'t SemanticTypeRecipe,
        )>,
    > {
        self.pay(1, depth)?;
        Ok(match ty {
            SemanticTypeRecipe::Address {
                mutable,
                lifetime,
                inner,
            } => Some((
                LocalRoleKind::Address { mutable: *mutable },
                lifetime,
                inner,
            )),
            SemanticTypeRecipe::Desc { lifetime, inner } => {
                Some((LocalRoleKind::Desc, lifetime, inner))
            }
            _ => None,
        })
    }
    pub(super) fn introduce_role(
        &mut self,
        owner: SemanticDefinitionId,
        statement: NodeId,
        initializer: &SemanticOriginalExpr,
        ty: SemanticTypeRecipe,
        variables: &BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        let Some((kind, source, inner)) = self.outer(&ty, depth + 1)? else {
            return Ok(ty);
        };
        crate::dyn_metadata::semantic_operand_snapshot(source, self.limits, self.left, 1)?;
        let source = source.clone();
        let inner = copy_recipe(inner, self.limits, self.left)?;
        let lifetime = self.local_lifetime(owner, statement, depth + 1)?;
        let role_type = match kind {
            LocalRoleKind::Address { mutable } => SemanticTypeRecipe::Address {
                mutable,
                lifetime: SemanticOuterLifeRecipe::Class(lifetime.clone()),
                inner: Box::new(inner),
            },
            LocalRoleKind::Desc => SemanticTypeRecipe::Desc {
                lifetime: SemanticOuterLifeRecipe::Class(lifetime.clone()),
                inner: Box::new(inner),
            },
        };
        let escape = self.role_source(owner, initializer, &source, variables, depth + 1)?;
        let source = LocalRoleSource {
            at: SemanticOriginalNode::Expression(initializer.id),
            lifetime: source,
            escape,
        };
        self.pay(2, depth)?;
        let copy = copy_recipe(&role_type, self.limits, self.left)?;
        self.proof.proof_mut()?.local_roles.insert(
            (owner, statement),
            LocalRole {
                kind,
                binding: statement,
                lifetime,
                ty: copy,
                sources: vec![source],
                escapes: false,
            },
        );
        Ok(role_type)
    }
    /// A moved capture may seal the real incoming class of its pointer.
    /// Keep the local declaration role and every other class in its type.
    pub(super) fn moved_capture_type(
        &mut self,
        owner: SemanticDefinitionId,
        value: &Binding,
        depth: usize,
    ) -> R<Option<SemanticTypeRecipe>> {
        let Query::Binding(binding) = value.origin else {
            return Ok(None);
        };
        if value.origin_owner != owner || value.borrowed {
            return Ok(None);
        }
        self.pay(self.proof.local_roles.len().saturating_add(1).ilog2() as usize + 1, depth)?;
        let Some(role) = self.proof.local_roles.get(&(owner, binding)) else {
            return Ok(None);
        };
        let Some((first, rest)) = role.sources.split_first() else {
            return Err(bad("moved local role has no complete original source"));
        };
        if !matches!(first.lifetime, SemanticOuterLifeRecipe::Class(_)) {
            return Ok(None);
        }
        for source in std::iter::once(first).chain(rest) {
            crate::dyn_metadata::semantic_operand_snapshot(&source.lifetime, self.limits, self.left, 1)?;
            if source.lifetime != first.lifetime
                || !matches!(source.escape, OriginalRoleEscape::Input { owner: scope, .. } if scope == owner)
            {
                return Ok(None);
            }
        }
        let mut captured = copy_recipe(&value.ty, self.limits, self.left)?;
        let lifetime = match &mut captured {
            SemanticTypeRecipe::Address { lifetime, .. }
            | SemanticTypeRecipe::Desc { lifetime, .. } => lifetime,
            _ => return Err(bad("moved local role changes its original outer constructor")),
        };
        if !matches!(lifetime, SemanticOuterLifeRecipe::Class(image) if *image == role.lifetime) {
            return Err(bad("moved local role changes its original declaration occurrence"));
        }
        *lifetime = first.lifetime.clone();
        Ok(Some(captured))
    }

    /// Existing sources establish only incoming directed bounds for a mutable
    /// local role. They never identify that role with an input, nor prove that
    /// the role outlives its sources (which later writes may extend).
    pub(super) fn local_role_source_lifetimes(
        &mut self,
        owner: SemanticDefinitionId,
        lifetime: &SemanticLifeRecipe,
        depth: usize,
    ) -> R<Option<Vec<SemanticOuterLifeRecipe>>> {
        if !matches!(
            lifetime,
            SemanticLifeRecipe::Implicit {
                scope: SemanticScopeRef::Ancestor(0),
                ..
            }
        ) {
            return Ok(None);
        }
        self.pay(self.proof.local_roles.len(), depth)?;
        let mut selected = None;
        for (key, role) in &self.proof.local_roles {
            if key.0 == owner && &role.lifetime == lifetime {
                if selected.replace((*key, role.sources.len())).is_some() {
                    return Err(bad(
                        "one original local lifetime has several declaration roles",
                    ));
                }
            }
        }
        let Some((key, count)) = selected else {
            return Ok(None);
        };
        self.pay(
            count.saturating_add(
                self.proof.local_roles.len().saturating_add(1).ilog2() as usize + 1,
            ),
            depth,
        )?;
        let role = self
            .proof
            .local_roles
            .get(&key)
            .ok_or_else(|| bad("original local source role disappeared"))?;
        let mut sources = Vec::new();
        for source in &role.sources {
            crate::dyn_metadata::semantic_operand_snapshot(
                &source.lifetime,
                self.limits,
                self.left,
                1,
            )?;
            sources.push(source.lifetime.clone());
        }
        if sources.is_empty() {
            return Err(bad(
                "original local directed bound has no complete incoming source",
            ));
        }
        Ok(Some(sources))
    }
    pub(super) fn assign_role(
        &mut self,
        owner: SemanticDefinitionId,
        role_owner: SemanticDefinitionId,
        statement: NodeId,
        value: &SemanticOriginalExpr,
        actual: &SemanticTypeRecipe,
        variables: &BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        if owner != role_owner {
            return self.assign_captured_role(owner, role_owner, statement, value, actual, variables, depth + 1);
        }
        let role = self
            .proof
            .local_roles
            .get(&(role_owner, statement))
            .ok_or_else(|| bad("original assignment loses its inferred local role"))?;
        let expected = copy_recipe(&role.ty, self.limits, self.left)?;
        let Some((kind, _, expected_inner)) = self.outer(&expected, depth + 1)? else {
            return Err(bad("original inferred role is not a raw outer occurrence"));
        };
        let Some((actual_kind, lifetime, actual_inner)) = self.outer(actual, depth + 1)? else {
            return Err(bad(
                "original local role assignment changes its complete outer kind",
            ));
        };
        if kind != actual_kind {
            return Err(bad(
                "original local role assignment changes its access permission mode",
            ));
        }
        self.same(expected_inner, actual_inner)?;
        crate::dyn_metadata::semantic_operand_snapshot(lifetime, self.limits, self.left, 1)?;
        let lifetime = lifetime.clone();
        let escape = self.role_source(owner, value, &lifetime, variables, depth + 1)?;
        if matches!(&escape, OriginalRoleEscape::Role { owner, binding } if *owner == role_owner && *binding == statement)
        {
            return Ok(());
        }
        let source = LocalRoleSource {
            at: SemanticOriginalNode::Expression(value.id),
            lifetime,
            escape,
        };
        self.pay(1, depth)?;
        let role = self
            .proof
            .proof_mut()?
            .local_roles
            .get_mut(&(role_owner, statement))
            .ok_or_else(|| bad("original local role disappeared during assignment proof"))?;
        if !role.sources.contains(&source) {
            role.sources.push(source);
        }
        Ok(())
    }
    pub(super) fn rehome_role_type(
        &mut self, source: SemanticDefinitionId, owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe, depth: usize,
    ) -> R<SemanticTypeRecipe> {
        let mut chain = Vec::new();
        let mut cursor = owner;
        while cursor != source {
            self.pay(1, depth + chain.len())?;
            chain.push(cursor);
            cursor = self.definition(cursor)?.parent
                .ok_or_else(|| bad("captured role source is outside its true lexical creator chain"))?;
        }
        let mut result = copy_recipe(ty, self.limits, self.left)?;
        let mut parent = source;
        for id in chain.into_iter().rev() {
            let child = self.definition(id)?;
            result = self.rehome(parent, child, &result, depth + 1)?;
            parent = id;
        }
        Ok(result)
    }

    /// A persistent captured slot keeps its creator's complete domain. Child
    /// writes are directed source loans into that slot, not an equality of the
    /// child parameter with an ancestor's same-numbered inferred local role.
    fn assign_captured_role(
        &mut self, owner: SemanticDefinitionId, role_owner: SemanticDefinitionId,
        statement: NodeId, value: &SemanticOriginalExpr, actual: &SemanticTypeRecipe,
        variables: &BTreeMap<String, Binding>, depth: usize,
    ) -> R<()> {
        self.pay(variables.len(), depth)?;
        let mut targets = variables.iter().filter(|(_, binding)| binding.capture
            && binding.origin_owner == role_owner && binding.origin == Query::Binding(statement));
        let (name, target) = targets.next().ok_or_else(|| bad("captured role write has no exact inherited lexical binder"))?;
        if targets.next().is_some() { return Err(bad("captured role write has ambiguous lexical binders")); }
        let SemanticTypeRecipe::Environment { scope: SemanticScopeRef::Ancestor(0), ordinal } = &target.ty else {
            return Err(bad("captured role write lost its sealed whole-Type environment selection"));
        };
        let ordinal = *ordinal;
        let child = self.definition(owner)?;
        let formal = child.environment.get(ordinal as usize)
            .ok_or_else(|| bad("captured role write leaves its complete original environment"))?;
        if let SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType {
            source: SemanticOriginalTypeSource::NameAt { name: original, .. }, ..
        }) = &formal.source {
            self.pay(original.len().saturating_add(name.len()), depth)?;
        }
        if !matches!(&formal.source, SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType {
            scope: SemanticScopeRef::Ancestor(0), source: SemanticOriginalTypeSource::NameAt { name: original, .. },
        }) if original == name) {
            return Err(bad("captured role write changes its original creator NameAt selection"));
        }
        let (creator, sealed) = self.proof.environments.get(&(owner, ordinal))
            .ok_or_else(|| bad("captured role write has no checked creator/child substitution"))?;
        if child.parent != Some(*creator) { return Err(bad("captured role write changes its actual lexical creator")); }
        let _sealed = copy_recipe(sealed, self.limits, self.left)?;
        let expected = self.observation_type(owner, &target.ty, depth + 1)?;
        let actual = self.observation_type(owner, actual, depth + 1)?;
        let Some((kind, target_life, inner)) = self.outer(&expected, depth + 1)? else {
            return Err(bad("captured role target is not an address or description"));
        };
        let Some((source_kind, source_life, source_inner)) = self.outer(&actual, depth + 1)? else {
            return Err(bad("captured role source changes its complete outer constructor"));
        };
        if kind != source_kind { return Err(bad("captured role write changes its permission mode")); }
        self.same(inner, source_inner)?;
        let count = self.proof.local_roles.get(&(role_owner, statement))
            .ok_or_else(|| bad("captured role write loses its original declaration role"))?.sources.len();
        self.pay(count.saturating_add(1), depth)?;
        let parent_role = self.proof.local_roles.get(&(role_owner, statement))
            .ok_or_else(|| bad("captured role declaration disappeared"))?;
        let parent_type = copy_recipe(&parent_role.ty, self.limits, self.left)?;
        let mut bounds = Vec::new();
        for source in &parent_role.sources {
            crate::dyn_metadata::semantic_operand_snapshot(&source.lifetime, self.limits, self.left, 1)?;
            bounds.push(source.lifetime.clone());
        }
        let Some((_, _, parent_inner)) = self.outer(&parent_type, depth + 1)? else {
            return Err(bad("captured creator role has no complete pointer payload"));
        };
        if bounds.is_empty() { return Err(bad("captured role has no initialized creator source bounds")); }
        for lifetime in bounds {
            let bound = match kind {
                LocalRoleKind::Address { mutable } => SemanticTypeRecipe::Address {
                    mutable, lifetime, inner: Box::new(copy_recipe(parent_inner, self.limits, self.left)?),
                },
                LocalRoleKind::Desc => SemanticTypeRecipe::Desc {
                    lifetime, inner: Box::new(copy_recipe(parent_inner, self.limits, self.left)?),
                },
            };
            let bound = self.rehome_role_type(role_owner, owner, &bound, depth + 1)?;
            let Some((_, required, _)) = self.outer(&bound, depth + 1)? else { unreachable!() };
            let valid = match (source_life, required) {
                (SemanticOuterLifeRecipe::Static, _) => true,
                (_, SemanticOuterLifeRecipe::Static) => false,
                (SemanticOuterLifeRecipe::Class(source), SemanticOuterLifeRecipe::Class(target)) =>
                    self.life_outlives(owner, source, target, depth + 1)?,
            };
            if !valid { return Err(bad("captured role write exceeds its complete persistent creator source bounds")); }
        }
        let Some((root, steps)) = self.proof.view_source_steps(owner, value.id, self.limits, self.left)? else {
            return Err(bad("captured role write has no complete original source path"));
        };
        for life in [source_life, target_life] {
            crate::dyn_metadata::semantic_operand_snapshot(life, self.limits, self.left, 2)?;
        }
        let write = CapturedRoleWrite { creator_role_owner: role_owner, creator_binding: statement,
            environment: ordinal, role: BorrowRole { root, steps,
                source_lifetime: source_life.clone(), occurrence_lifetime: source_life.clone(),
                target_lifetime: target_life.clone(), inner: copy_recipe(inner, self.limits, self.left)? } };
        self.pay(self.proof.capture_writes.len().saturating_add(1).ilog2() as usize + 1, depth)?;
        if let Some(prior) = self.proof.capture_writes.get(&(owner, value.id)) {
            let previous = views::snapshot_role(&prior.role, self.limits, self.left)?;
            if prior.creator_role_owner != write.creator_role_owner
                || prior.creator_binding != write.creator_binding || prior.environment != write.environment
                || previous != write.role
            { return Err(bad("captured role write changes its checked creator/source receipt")); }
        } else {
            self.proof.proof_mut()?.capture_writes.insert((owner, value.id), write);
        }
        Ok(())
    }
    pub(super) fn role_source(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        lifetime: &SemanticOuterLifeRecipe,
        variables: &BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<OriginalRoleEscape> {
        self.pay(1, depth)?;
        if matches!(lifetime, SemanticOuterLifeRecipe::Static) {
            return Ok(OriginalRoleEscape::Static);
        }
        if let Some((owner, ordinal)) = self.region_key(owner, lifetime, depth + 1)? {
            return Ok(OriginalRoleEscape::CallRegion { owner, ordinal });
        }
        match &expression.kind {
            CoreExprKind::Name(name) => {
                let binding = variables
                    .get(name)
                    .ok_or_else(|| bad("original role source has no lexical binder"))?;
                if let Query::Binding(statement) = binding.origin {
                    if self
                        .proof
                        .local_roles
                        .contains_key(&(binding.origin_owner, statement))
                    {
                        return Ok(OriginalRoleEscape::Role {
                            owner: binding.origin_owner,
                            binding: statement,
                        });
                    }
                }
                if let Query::Parameter(parameter) = binding.origin {
                    return Ok(OriginalRoleEscape::Input {
                        owner: binding.origin_owner,
                        parameter,
                    });
                }
            }
            CoreExprKind::Group(value) | CoreExprKind::Ascribed { value, .. } => {
                return self.role_source(owner, value, lifetime, variables, depth + 1);
            }
            CoreExprKind::Unary {
                op: UnaryOp::Borrow | UnaryOp::BorrowMut,
                operand,
            } => return self.borrow_escape(owner, operand, variables, depth + 1),
            _ => {}
        }
        // A copied pointer still names its original referent. A local alias
        // is not a borrow of the alias's storage. Only the complete transparent
        // ReadLocal/Alias path can transport the declaration role this way.
        if let Some((root, steps)) = self.proof.view_source_steps(owner, expression.id, self.limits, self.left)? {
            self.pay(steps.len(), depth)?;
            if views::local_source_copy(root, &steps) {
                let ViewBorrowRoot::Local(binding) = root else { unreachable!() };
                self.pay(self.proof.local_roles.len().saturating_add(1).ilog2() as usize + 1, depth)?;
                if self.proof.local_roles.contains_key(&(owner, binding)) {
                    return Ok(OriginalRoleEscape::Role { owner, binding });
                }
            }
        }
        if let Some(parameter) = self.input_lifetime(owner, lifetime, depth + 1)? {
            return Ok(OriginalRoleEscape::Input { owner, parameter });
        }
        Ok(OriginalRoleEscape::LocalPlace {
            owner,
            node: expression.id,
        })
    }
    pub(super) fn borrow_escape(
        &mut self,
        owner: SemanticDefinitionId,
        place: &SemanticOriginalExpr,
        variables: &BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<OriginalRoleEscape> {
        self.pay(1, depth)?;
        match &place.kind {
            CoreExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } => {
                if let CoreExprKind::Name(name) = &operand.kind {
                    let binding = variables
                        .get(name)
                        .ok_or_else(|| bad("original reborrow has no lexical source"))?;
                    if let Query::Parameter(parameter) = binding.origin {
                        return Ok(OriginalRoleEscape::Input {
                            owner: binding.origin_owner,
                            parameter,
                        });
                    }
                    if let Query::Binding(statement) = binding.origin {
                        if self
                            .proof
                            .local_roles
                            .contains_key(&(binding.origin_owner, statement))
                        {
                            return Ok(OriginalRoleEscape::Role {
                                owner: binding.origin_owner,
                                binding: statement,
                            });
                        }
                    }
                }
            }
            CoreExprKind::Member { object, .. }
            | CoreExprKind::Index { object, .. }
            | CoreExprKind::Slice { object, .. } => {
                if let CoreExprKind::Name(name) = &object.kind {
                    let binding = variables
                        .get(name)
                        .ok_or_else(|| bad("original projected borrow has no lexical source"))?;
                    if matches!(binding.ty, SemanticTypeRecipe::Address { .. }) {
                        if let Query::Parameter(parameter) = binding.origin {
                            return Ok(OriginalRoleEscape::Input {
                                owner: binding.origin_owner,
                                parameter,
                            });
                        }
                        if let Query::Binding(statement) = binding.origin {
                            if self
                                .proof
                                .local_roles
                                .contains_key(&(binding.origin_owner, statement))
                            {
                                return Ok(OriginalRoleEscape::Role {
                                    owner: binding.origin_owner,
                                    binding: statement,
                                });
                            }
                        }
                    }
                }
                return self.borrow_escape(owner, object, variables, depth + 1);
            }
            _ => {}
        }
        Ok(OriginalRoleEscape::LocalPlace {
            owner,
            node: place.id,
        })
    }
    pub(super) fn input_lifetime(
        &mut self,
        owner: SemanticDefinitionId,
        lifetime: &SemanticOuterLifeRecipe,
        depth: usize,
    ) -> R<Option<u32>> {
        let definition = self.definition(owner)?;
        let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &definition.body else {
            return Err(bad("original role input has no full signature"));
        };
        for (ordinal, parameter) in signature.parameters.iter().enumerate() {
            self.pay(1, depth)?;
            let parameter_type = self
                .proof
                .types
                .get(&(owner, Query::Parameter(ordinal as u32)))
                .unwrap_or(&parameter.ty);
            if matches!(parameter_type, SemanticTypeRecipe::Address { lifetime: source, .. } | SemanticTypeRecipe::Desc { lifetime: source, .. } if source == lifetime)
            {
                return Ok(Some(ordinal as u32));
            }
        }
        Ok(None)
    }
    pub(super) fn escape_source(
        &mut self,
        escape: &OriginalRoleEscape,
        active: &mut BTreeSet<(SemanticDefinitionId, Option<NodeId>, Option<u32>)>,
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        match escape {
            OriginalRoleEscape::Static => Ok(()),
            OriginalRoleEscape::Input { owner, parameter } => {
                let definition = self.definition(*owner)?;
                let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &definition.body
                else {
                    return Err(bad("original escaping source has no real input definition"));
                };
                if !matches!(
                    signature
                        .parameters
                        .get(*parameter as usize)
                        .map(|parameter| &parameter.ty),
                    Some(SemanticTypeRecipe::Address { .. } | SemanticTypeRecipe::Desc { .. })
                ) {
                    return Err(bad(
                        "original escaping role does not originate at a borrowed input occurrence",
                    ));
                }
                Ok(())
            }
            OriginalRoleEscape::Role { owner, binding } => {
                self.escape_role(*owner, *binding, active, depth + 1)
            }
            OriginalRoleEscape::CallRegion { owner, ordinal } => {
                self.escape_call_region(*owner, *ordinal, active, depth + 1)
            }
            OriginalRoleEscape::LocalPlace { .. } => Err(bad(
                "original escaping value retains a borrow of local storage",
            )),
        }
    }
    fn escape_role(
        &mut self,
        owner: SemanticDefinitionId,
        binding: NodeId,
        active: &mut BTreeSet<(SemanticDefinitionId, Option<NodeId>, Option<u32>)>,
        depth: usize,
    ) -> R<()> {
        self.pay(active.len().saturating_add(1), depth)?;
        if !active.insert((owner, Some(binding), None)) {
            return Err(bad(
                "original inferred escaping role has a cyclic source equation",
            ));
        }
        let count = self
            .proof
            .local_roles
            .get(&(owner, binding))
            .ok_or_else(|| bad("original escaping role lacks its declaration receipt"))?
            .sources
            .len();
        self.pay(count, depth)?;
        let role = self
            .proof
            .local_roles
            .get(&(owner, binding))
            .ok_or_else(|| bad("original escaping role lacks its declaration receipt"))?;
        let mut sources = Vec::new();
        for source in &role.sources {
            crate::dyn_metadata::semantic_operand_snapshot(
                &source.lifetime,
                self.limits,
                self.left,
                1,
            )?;
            sources.push(source.escape.clone());
        }
        if sources.is_empty() {
            return Err(bad(
                "original inferred escaping role has no checked incoming source",
            ));
        }
        for source in &sources {
            self.escape_source(source, active, depth + 1)?;
        }
        active.remove(&(owner, Some(binding), None));
        self.proof
            .proof_mut()?
            .local_roles
            .get_mut(&(owner, binding))
            .ok_or_else(|| bad("original role disappeared during escape proof"))?
            .escapes = true;
        Ok(())
    }
    pub(super) fn ensure_escape(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<()> {
        self.pay(self.escape_active.len().saturating_add(1), depth)?;
        if self.escape_active.iter().any(|active| active == ty) {
            return Ok(());
        }
        self.escape_active
            .push(copy_recipe(ty, self.limits, self.left)?);
        let result = self.ensure_escape_inner(owner, ty, depth + 1);
        self.escape_active.pop();
        result
    }
    fn ensure_outer_escape(
        &mut self,
        owner: SemanticDefinitionId,
        lifetime: &SemanticOuterLifeRecipe,
        depth: usize,
    ) -> R<()> {
        if let Some((owner, ordinal)) = self.region_key(owner, lifetime, depth + 1)? {
            return self.escape_call_region(owner, ordinal, &mut BTreeSet::new(), depth + 1);
        }
        let mut role = None;
        self.pay(self.proof.local_roles.len(), depth)?;
        for ((role_owner, binding), declaration) in &self.proof.local_roles {
            if *role_owner == owner
                && matches!(lifetime, SemanticOuterLifeRecipe::Class(actual) if actual == &declaration.lifetime)
            {
                role = Some((*role_owner, *binding));
                break;
            }
        }
        if let Some((owner, binding)) = role {
            self.escape_role(owner, binding, &mut BTreeSet::new(), depth + 1)?;
        } else if !matches!(lifetime, SemanticOuterLifeRecipe::Static)
            && self.input_lifetime(owner, lifetime, depth + 1)?.is_none()
        {
            // Explicit life formals and checked sealed outer anchors are
            // semantic inputs. A fresh body implicit borrow is not one.
            if matches!(
                lifetime,
                SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Implicit {
                    scope: SemanticScopeRef::Ancestor(0),
                    ..
                })
            ) {
                let SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Implicit { ordinal, .. }) = lifetime else { unreachable!() };
                let original = self.definition(owner)?;
                self.pay(original.implicit_lifetimes.len(), depth)?;
                let source = original.implicit_lifetimes.iter().find(|row| row.ordinal == *ordinal)
                    .ok_or_else(|| bad("escaping original borrow lacks its real lifetime occurrence"))?;
                let SemanticDefinitionBody::FunctionWithBody { sites, site_nodes, .. } = &original.body else {
                    return Err(bad("escaping original borrow has no complete definition body"));
                };
                self.pay(sites.len().saturating_mul(site_nodes.len().saturating_add(1)), depth)?;
                for site in sites {
                    self.pay(site.occurrence_path.len().saturating_add(source.occurrence_path.len()), depth)?;
                    if site.occurrence_path != source.occurrence_path { continue; }
                    for link in site_nodes {
                        if link.site != site.id { continue; }
                        let SemanticOriginalNode::Expression(node) = link.node else { continue; };
                        if let Some((root, steps)) = self.proof.view_source_steps(owner, node, self.limits, self.left)? {
                            self.pay(steps.len(), depth)?;
                            let local_storage = match root {
                                ViewBorrowRoot::Local(binding) => steps.iter().any(|step| matches!(step.kind,
                                    ViewBorrowStepKind::Local { binding: selected } if selected == binding)
                                    || matches!(step.kind, ViewBorrowStepKind::ProjectedLocal { statement, read: false, .. } if statement == binding)),
                                ViewBorrowRoot::Parameter(_) => steps.iter().any(|step| matches!(step.kind, ViewBorrowStepKind::ParameterStorage)),
                                _ => false,
                            };
                            if local_storage {
                                return self.escape_source(&OriginalRoleEscape::LocalPlace { owner, node }, &mut BTreeSet::new(), depth + 1);
                            }
                        }
                    }
                }
                return Err(bad(
                    "original result exposes an unbounded local borrow occurrence",
                ));
            }
        }
        Ok(())
    }
    fn ensure_sequence_escape(
        &mut self,
        owner: SemanticDefinitionId,
        sequence: &SemanticClassSequenceRecipe,
        depth: usize,
    ) -> R<()> {
        self.pay(sequence.parts.len(), depth)?;
        for part in &sequence.parts {
            match part {
                SemanticClassSequencePart::One(life) => {
                    crate::dyn_metadata::semantic_operand_snapshot(
                        life,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    self.ensure_outer_escape(
                        owner,
                        &SemanticOuterLifeRecipe::Class(life.clone()),
                        depth + 1,
                    )?;
                }
                SemanticClassSequencePart::AllClasses { ty, .. } => {
                    self.ensure_escape(owner, ty, depth + 1)?
                }
            }
        }
        Ok(())
    }
    fn ensure_retained_escape(
        &mut self,
        owner: SemanticDefinitionId,
        retained: &SemanticRetainedShapeRecipe,
        depth: usize,
    ) -> R<()> {
        self.ensure_sequence_escape(owner, &retained.domain, depth + 1)?;
        for part in &retained.ports {
            match part {
                SemanticPortPart::One { leaf, class, .. } => {
                    self.ensure_escape(owner, leaf, depth + 1)?;
                    crate::dyn_metadata::semantic_operand_snapshot(
                        class,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    self.ensure_outer_escape(
                        owner,
                        &SemanticOuterLifeRecipe::Class(class.clone()),
                        depth + 1,
                    )?;
                }
                SemanticPortPart::FromType { ty, classes } => {
                    self.ensure_escape(owner, ty, depth + 1)?;
                    self.ensure_sequence_escape(owner, classes, depth + 1)?;
                }
            }
        }
        for part in &retained.retained {
            match part {
                SemanticRetainedPart::One { parent, .. } => {
                    crate::dyn_metadata::semantic_operand_snapshot(
                        parent,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    self.ensure_outer_escape(
                        owner,
                        &SemanticOuterLifeRecipe::Class(parent.clone()),
                        depth + 1,
                    )?;
                }
                SemanticRetainedPart::All { parents, .. } => {
                    self.ensure_sequence_escape(owner, parents, depth + 1)?
                }
            }
        }
        for part in &retained.nominal {
            match part {
                SemanticNominalPart::One { class, .. } => {
                    crate::dyn_metadata::semantic_operand_snapshot(
                        class,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    self.ensure_outer_escape(
                        owner,
                        &SemanticOuterLifeRecipe::Class(class.clone()),
                        depth + 1,
                    )?;
                }
                SemanticNominalPart::FromType { ty, classes, .. } => {
                    self.ensure_escape(owner, ty, depth + 1)?;
                    self.ensure_sequence_escape(owner, classes, depth + 1)?;
                }
            }
        }
        Ok(())
    }
    fn ensure_escape_inner(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        match ty {
            SemanticTypeRecipe::Address {
                lifetime, inner, ..
            }
            | SemanticTypeRecipe::Desc { lifetime, inner } => {
                self.ensure_outer_escape(owner, lifetime, depth + 1)?;
                self.ensure_escape(owner, inner, depth + 1)?;
            }
            SemanticTypeRecipe::Task { result, retained }
            | SemanticTypeRecipe::MutexGuard {
                payload: result,
                retained,
            } => {
                self.ensure_escape(owner, result, depth + 1)?;
                self.ensure_retained_escape(owner, retained, depth + 1)?;
            }
            SemanticTypeRecipe::Tuple(fields) | SemanticTypeRecipe::AnonymousSum(fields) => {
                for field in fields {
                    self.ensure_escape(owner, field, depth + 1)?;
                }
            }
            SemanticTypeRecipe::Array { element, .. }
            | SemanticTypeRecipe::Slice(element)
            | SemanticTypeRecipe::Managed(element)
            | SemanticTypeRecipe::OwnedStore(element)
            | SemanticTypeRecipe::Mutex(element)
            | SemanticTypeRecipe::Registration(element)
            | SemanticTypeRecipe::AsyncRegistration(element) => {
                self.ensure_escape(owner, element, depth + 1)?
            }
            SemanticTypeRecipe::ErrorUnion { error, result } => {
                self.ensure_escape(owner, error, depth + 1)?;
                self.ensure_escape(owner, result, depth + 1)?;
            }
            SemanticTypeRecipe::Opaque { hidden, .. } => {
                self.ensure_escape(owner, hidden, depth + 1)?
            }
            SemanticTypeRecipe::Nominal {
                definition,
                arguments,
            } => {
                let definition = self.definition(*definition)?;
                let frames = [(0, arguments.as_slice())];
                let empty = BTreeMap::new();
                let substitution = substitute::Substitution {
                    frames: if arguments.is_empty() { &[] } else { &frames },
                    selves: &[],
                    environments: &[],
                    implicit: &empty,
                    invocation: &empty,
                    shift: 0,
                };
                let SemanticDefinitionBody::Nominal { body, .. } = &definition.body else {
                    return Err(bad(
                        "original escaping nominal has no complete declared body",
                    ));
                };
                match body {
                    SemanticNominalBody::Product { fields } => {
                        for field in fields {
                            let ty =
                                substitute::ty(&field.ty, &substitution, self.limits, self.left)?;
                            self.ensure_escape(owner, &ty, depth + 1)?;
                        }
                    }
                    SemanticNominalBody::Enum { variants } => {
                        for variant in variants {
                            for field in &variant.fields {
                                let ty =
                                    substitute::ty(field, &substitution, self.limits, self.left)?;
                                self.ensure_escape(owner, &ty, depth + 1)?;
                            }
                        }
                    }
                }
            }
            SemanticTypeRecipe::Closure {
                definition,
                environment,
                ..
            } => {
                if self.proof.borrowed_capture_heads.contains(definition) {
                    return Err(bad(
                        "original escaping closure borrows its creator's local storage",
                    ));
                }
                for actual in environment {
                    if let SemanticActualRecipe::Type(ty) = actual {
                        self.ensure_escape(owner, ty, depth + 1)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn recheck_escaping_roles(
        &mut self,
        owner: SemanticDefinitionId,
        depth: usize,
    ) -> R<()> {
        self.pay(self.proof.local_roles.len(), depth)?;
        let mut roles = Vec::new();
        for ((role_owner, binding), role) in &self.proof.local_roles {
            if *role_owner == owner && role.escapes {
                roles.push(*binding);
            }
        }
        for binding in roles {
            self.escape_role(owner, binding, &mut BTreeSet::new(), depth + 1)?;
        }
        self.pay(self.proof.call_regions.len(), depth)?;
        let regions = self
            .proof
            .call_regions
            .iter()
            .filter_map(|((region_owner, ordinal), region)| {
                (*region_owner == owner && region.escapes).then_some(*ordinal)
            })
            .collect::<Vec<_>>();
        for ordinal in regions {
            self.escape_call_region(owner, ordinal, &mut BTreeSet::new(), depth + 1)?;
        }
        self.recheck_call_region_bounds(owner, depth + 1)?;
        Ok(())
    }
}
