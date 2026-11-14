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
            return Err(pending(
                "captured local role mutation requires its explicit creator/child substitution receipt",
            ));
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
    fn role_source(
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
    fn input_lifetime(
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
            if matches!(&parameter.ty, SemanticTypeRecipe::Address { lifetime: source, .. } | SemanticTypeRecipe::Desc { lifetime: source, .. } if source == lifetime)
            {
                return Ok(Some(ordinal as u32));
            }
        }
        Ok(None)
    }
    fn escape_source(
        &mut self,
        escape: &OriginalRoleEscape,
        active: &mut BTreeSet<(SemanticDefinitionId, NodeId)>,
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
            OriginalRoleEscape::LocalPlace { .. } => Err(bad(
                "original escaping value retains a borrow of local storage",
            )),
        }
    }
    fn escape_role(
        &mut self,
        owner: SemanticDefinitionId,
        binding: NodeId,
        active: &mut BTreeSet<(SemanticDefinitionId, NodeId)>,
        depth: usize,
    ) -> R<()> {
        self.pay(active.len().saturating_add(1), depth)?;
        if !active.insert((owner, binding)) {
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
        active.remove(&(owner, binding));
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
                        return Err(bad(
                            "original result exposes an unbounded local borrow occurrence",
                        ));
                    }
                }
                self.ensure_escape(owner, inner, depth + 1)?;
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
        Ok(())
    }
}
