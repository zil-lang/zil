//! Dynamic member selection in the immutable original interface domain.
//! Runtime slots and concrete provider specializations are never premises here.
use super::*;

impl Check<'_, '_> {
    pub(super) fn dyn_call(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<Option<SemanticTypeRecipe>> {
        self.pay(1, depth)?;
        let CoreExprKind::Call { callee, arguments } = &expression.kind else {
            return Ok(None);
        };
        let mut member_expression = callee.as_ref();
        while let CoreExprKind::TypeApply { callee, .. }
        | CoreExprKind::GenericApply { callee, .. } = &member_expression.kind
        {
            self.pay(1, depth)?;
            member_expression = callee;
        }
        let CoreExprKind::Member { object, member, .. } = &member_expression.kind else {
            return Ok(None);
        };
        if self
            .declaration_path(object, variables, depth + 1)?
            .is_some()
        {
            return Ok(None);
        }
        let source = self.place(owner, object, variables, depth + 1)?;
        let source = self.observation_type(owner, &source, depth + 1)?;
        let (interface, source_mutability, source_lifetime) = match &source {
            SemanticTypeRecipe::Dyn {
                owned: true,
                interface,
            } => (interface.as_ref(), None, None),
            SemanticTypeRecipe::Address {
                mutable,
                lifetime,
                inner,
            } => match inner.as_ref() {
                SemanticTypeRecipe::Dyn {
                    owned: false,
                    interface,
                } => (interface.as_ref(), Some(*mutable), Some(lifetime)),
                _ => return Ok(None),
            },
            _ => return Ok(None),
        };
        self.pay(self.program.definitions.len(), depth)?;
        let mut parent = None;
        for definition in &self.program.definitions {
            if matches!(definition.body, SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { declaration }) if declaration == interface.application.declaration)
            {
                if parent.replace(definition).is_some() {
                    return Err(bad("dynamic interface has ambiguous original trait scopes"));
                }
            }
        }
        let parent =
            parent.ok_or_else(|| bad("dynamic interface has no complete original trait scope"))?;
        if interface.application.arguments.len() != parent.parameters.len() {
            return Err(bad(
                "dynamic interface omits original mixed trait arguments",
            ));
        }
        self.call_requirements(owner, parent, &interface.application.arguments, depth + 1)?;
        self.pay(self.program.definitions.len(), depth)?;
        let mut selected = None;
        for definition in &self.program.definitions {
            if definition.parent != Some(parent.id)
                || definition.kind != SemanticDefinitionKind::Method
            {
                continue;
            }
            self.pay(
                definition
                    .identity
                    .declaration_path
                    .len()
                    .saturating_add(parent.identity.declaration_path.len()),
                depth,
            )?;
            let prefix = &parent.identity.declaration_path;
            let path = &definition.identity.declaration_path;
            if definition.identity.package != parent.identity.package
                || definition.identity.module_path != parent.identity.module_path
                || definition.identity.generation_path != parent.identity.generation_path
                || !path.starts_with(prefix)
                || path.len() != prefix.len() + 3
                || path[prefix.len()] != "member"
            {
                return Err(bad(
                    "dynamic original method loses its complete trait identity",
                ));
            }
            let ordinal = path[prefix.len() + 1]
                .parse::<u32>()
                .map_err(|_| bad("dynamic original member ordinal is malformed"))?;
            if path.last() == Some(member) {
                if selected.replace((definition, ordinal)).is_some() {
                    return Err(bad(
                        "dynamic member spelling is ambiguous in its original trait",
                    ));
                }
            }
        }
        let (definition, member_ordinal) =
            selected.ok_or_else(|| bad("dynamic interface has no declared original method"))?;
        if !self.proof.complete.contains(&definition.id) && !self.active.contains(&definition.id) {
            self.function(definition, None)?;
        }
        let SemanticDefinitionBody::FunctionWithBody {
            signature,
            receiver_parameter: Some(0),
            ..
        } = &definition.body
        else {
            return Err(bad(
                "dynamic method requires its real first receiver parameter",
            ));
        };
        crate::dyn_metadata::semantic_operand_snapshot(interface, self.limits, self.left, 1)?;
        let borrowed = SemanticTypeRecipe::Dyn {
            owned: false,
            interface: Box::new(interface.clone()),
        };
        let parent_frames = [(1, interface.application.arguments.as_slice())];
        let selves = [(1, &borrowed)];
        let empty = BTreeMap::new();
        let substitution = substitute::Substitution {
            frames: &parent_frames,
            selves: &selves,
            environments: &[],
            implicit: &empty,
            invocation: &empty,
            shift: 0,
        };
        let template =
            self.inference_template(signature, &definition.parameters, true, depth + 1)?;
        let mut template = substitute::signature(&template, &substitution, self.limits, self.left)?;
        self.pay(template.parameters.len(), depth)?;
        let receiver = template
            .parameters
            .first()
            .ok_or_else(|| bad("dynamic method omits its receiver"))?;
        let SemanticTypeRecipe::Address { mutable, inner, .. } = &receiver.ty else {
            return Err(bad("dynamic method receiver must be an address"));
        };
        if inner.as_ref() != &borrowed
            || !matches!(
                (receiver.mode, *mutable),
                (PassingMode::Borrow, false) | (PassingMode::BorrowMut, true)
            )
        {
            return Err(bad(
                "dynamic method changes its declared erased receiver mode",
            ));
        }
        let mutable = *mutable;
        if mutable {
            if source_mutability == Some(false) {
                return Err(bad("exclusive dynamic call has only a shared receiver"));
            }
            self.writable(owner, object, variables, depth + 1)?;
        }
        let lifetime = match source_lifetime {
            Some(lifetime) => {
                crate::dyn_metadata::semantic_operand_snapshot(
                    lifetime,
                    self.limits,
                    self.left,
                    1,
                )?;
                lifetime.clone()
            }
            None => {
                let source = self.borrow_life(owner, object.id)?;
                let selector = match &receiver.ty {
                    SemanticTypeRecipe::Address { lifetime, .. } => {
                        match call_regions::variable(lifetime) {
                            Some(call_regions::Variable::Life(ordinal)) => {
                                Some(call_regions::Role::Life(ordinal))
                            }
                            Some(call_regions::Variable::Implicit(ordinal)) => {
                                Some(call_regions::Role::Implicit(ordinal))
                            }
                            Some(call_regions::Variable::Invocation(ordinal)) => {
                                Some(call_regions::Role::Invocation(ordinal))
                            }
                            None => match lifetime {
                                SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Bound {
                                    depth: 0,
                                    index,
                                }) if *index < template.bound_lifetimes => {
                                    Some(call_regions::Role::Bound {
                                        depth: 0,
                                        index: *index,
                                    })
                                }
                                _ => None,
                            },
                        }
                    }
                    _ => None,
                };
                if let Some(selector) = selector {
                    // Owned receivers acquire their actual head loan at this
                    // call. The owner expression itself remains an Owned Dyn
                    // value; its observation occurrence is not the call role.
                    let region =
                        self.call_region_life(owner, expression.id, selector, depth + 1)?;
                    self.record_region_source(
                        owner,
                        &region,
                        object,
                        &source,
                        variables,
                        depth + 1,
                    )?;
                    region
                } else {
                    source
                }
            }
        };
        let receiver_type = SemanticTypeRecipe::Address {
            mutable,
            lifetime,
            inner: Box::new(copy_recipe(&borrowed, self.limits, self.left)?),
        };
        self.pay(
            arguments
                .len()
                .saturating_add(definition.parameters.len())
                .saturating_add(1),
            depth,
        )?;
        let operands = std::iter::once(object.as_ref())
            .chain(arguments.iter())
            .collect::<Vec<_>>();
        let holes = vec![SemanticActualRecipe::Infer; definition.parameters.len()];
        let receiver_roles = [(object.id, receiver_type)];
        // Retain the real receiver in the full joint solve. Removing it first
        // would either lose its call region or bind it to a first source before
        // inner/written/wanted constraints and the remaining inputs are known.
        let (actuals, solved) = self.solve_signature_roles(
            owner,
            expression,
            callee,
            &operands,
            &template,
            &definition.parameters,
            &holes,
            variables,
            wanted,
            true,
            &BTreeMap::new(),
            &receiver_roles,
            0,
            None,
            Some(definition),
            depth + 1,
        )?;
        let mut implicit = BTreeMap::new();
        let mut invocation = BTreeMap::new();
        let mut solutions = Vec::new();
        self.pay(actuals.len(), depth)?;
        for actual in &actuals {
            crate::dyn_metadata::semantic_operand_snapshot(actual, self.limits, self.left, 1)?;
            solutions.push(Some(actual.clone()));
        }
        let template = if template.bound_lifetimes != 0 {
            substitute::universal_callable_own(&template, self.limits, self.left)?
        } else {
            template
        };
        for (parameter, actual) in template.parameters.iter().zip(&solved.parameters) {
            self.unify(
                &parameter.ty,
                &actual.ty,
                &definition.parameters,
                &mut solutions,
                &mut implicit,
                &mut invocation,
                depth + 1,
            )?;
        }
        self.unify(
            &template.result,
            &solved.result,
            &definition.parameters,
            &mut solutions,
            &mut implicit,
            &mut invocation,
            depth + 1,
        )?;
        let receiver = solved
            .parameters
            .first()
            .ok_or_else(|| bad("dynamic joint solve loses its original receiver"))?;
        let frames = [
            (0, actuals.as_slice()),
            (1, interface.application.arguments.as_slice()),
        ];
        let substitution = substitute::Substitution {
            frames: &frames,
            selves: &selves,
            environments: &[],
            implicit: &implicit,
            invocation: &invocation,
            shift: 0,
        };
        for requirement in
            definition
                .requirements
                .iter()
                .chain(
                    definition
                        .parameters
                        .iter()
                        .flat_map(|formal| match formal {
                            SemanticFormal::Type { requirements } => requirements.as_slice(),
                            _ => &[],
                        }),
                )
        {
            let requirement =
                substitute::requirement(requirement, &substitution, self.limits, self.left)?;
            self.satisfy_requirement(owner, &requirement, depth + 1)?;
        }
        let site = self
            .site(owner, SemanticOriginalNode::Expression(expression.id))?
            .ok_or_else(|| bad("dynamic original call has no exact operation occurrence"))?;
        self.pay(solved.parameters.len().saturating_add(2), depth)?;
        let mut parameters = Vec::new();
        for parameter in &solved.parameters {
            parameters.push(copy_recipe(&parameter.ty, self.limits, self.left)?);
        }
        let kind = IntrinsicKind::DynCall {
            receiver_node: object.id,
            declaration: definition.id,
            parent: parent.id,
            member: member_ordinal,
            receiver_mode: receiver.mode,
            parameter_modes: solved
                .parameters
                .iter()
                .skip(1)
                .map(|parameter| parameter.mode)
                .collect(),
            result_mode: solved.result_mode,
            call_kind: solved.call_kind,
        };
        self.record_intrinsic(
            owner,
            site.id,
            kind,
            parameters,
            &solved.result,
            wanted,
            depth + 1,
        )?;
        self.proof
            .proof_mut()?
            .calls
            .insert((owner, site.id), actuals);
        // The source observation still denotes the owner. The independently
        // emitted receiver borrow has its own outer occurrence and is checked
        // against parameters[0] at the actual slot boundary.
        Ok(Some(solved.result))
    }
}

impl Check<'_, '_> {
    pub(super) fn same_dyn_interface(
        &mut self,
        left: &SemanticDynInterfaceRecipe,
        right: &SemanticDynInterfaceRecipe,
        depth: usize,
    ) -> R<()> {
        crate::dyn_metadata::semantic_operand_snapshot(left, self.limits, self.left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(right, self.limits, self.left, 1)?;
        self.same_dyn_application(&left.application, &right.application, depth + 1)?;
        match (&left.context, &right.context) {
            (SemanticDynContextRecipe::Closed, SemanticDynContextRecipe::Closed) => {}
            (SemanticDynContextRecipe::Type(a), SemanticDynContextRecipe::Type(b)) => {
                self.same(a, b)?
            }
            (SemanticDynContextRecipe::Shape(a), SemanticDynContextRecipe::Shape(b)) => {
                let owner = self
                    .contexts
                    .last()
                    .copied()
                    .ok_or_else(|| bad("original Dyn comparison has no lexical Gamma"))?;
                self.same_task_shape(owner, a, b, depth + 1)?;
            }
            _ => return Err(bad("original Dyn changes its complete Context kind")),
        }
        self.pay(
            left.associated.len().saturating_add(right.associated.len()),
            depth,
        )?;
        if left.associated.len() != right.associated.len() {
            return Err(bad(
                "original Dyn changes its complete associated binding inventory",
            ));
        }
        for (a, b) in left.associated.iter().zip(&right.associated) {
            if a.member != b.member {
                return Err(bad("original Dyn changes an associated member identity"));
            }
            self.same_dyn_application(&a.application, &b.application, depth + 1)?;
            match (&a.value, &b.value) {
                (SemanticAssociatedRecipe::Type(a), SemanticAssociatedRecipe::Type(b)) => {
                    self.same(a, b)?
                }
                (
                    SemanticAssociatedRecipe::Const {
                        carrier: a,
                        value: av,
                    },
                    SemanticAssociatedRecipe::Const {
                        carrier: b,
                        value: bv,
                    },
                ) => {
                    self.same(a, b)?;
                    if !self.same_const(a, av, bv)? {
                        return Err(bad("original Dyn changes an associated Const payload"));
                    }
                }
                _ => return Err(bad("original Dyn changes an associated binding kind")),
            }
        }
        Ok(())
    }
    fn same_dyn_application(
        &mut self,
        left: &SemanticTraitRecipe,
        right: &SemanticTraitRecipe,
        depth: usize,
    ) -> R<()> {
        self.pay(
            left.arguments.len().saturating_add(right.arguments.len()),
            depth,
        )?;
        if left.declaration != right.declaration || left.arguments.len() != right.arguments.len() {
            return Err(bad(
                "original Dyn changes its declaration or full mixed actual vector",
            ));
        }
        for (a, b) in left.arguments.iter().zip(&right.arguments) {
            self.same_actual(a, b)?;
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn unify_dyn_interface(
        &mut self,
        left: &SemanticDynInterfaceRecipe,
        right: &SemanticDynInterfaceRecipe,
        formals: &[SemanticFormal],
        solutions: &mut [Option<SemanticActualRecipe>],
        implicit: &mut BTreeMap<u32, SemanticOuterLifeRecipe>,
        invocation: &mut BTreeMap<u32, SemanticOuterLifeRecipe>,
        depth: usize,
    ) -> R<()> {
        crate::dyn_metadata::semantic_operand_snapshot(left, self.limits, self.left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(right, self.limits, self.left, 1)?;
        self.unify_dyn_application(
            &left.application,
            &right.application,
            formals,
            solutions,
            implicit,
            invocation,
            depth + 1,
        )?;
        match (&left.context, &right.context) {
            (SemanticDynContextRecipe::Closed, SemanticDynContextRecipe::Closed) => {}
            (SemanticDynContextRecipe::Type(a), SemanticDynContextRecipe::Type(b)) => {
                self.unify(a, b, formals, solutions, implicit, invocation, depth + 1)?
            }
            (SemanticDynContextRecipe::Shape(a), SemanticDynContextRecipe::Shape(b)) => {
                self.unify_task_shape(a, b, formals, solutions, implicit, invocation, depth + 1)?
            }
            _ => return Err(bad("original Dyn call changes its complete Context kind")),
        }
        self.pay(
            left.associated.len().saturating_add(right.associated.len()),
            depth,
        )?;
        if left.associated.len() != right.associated.len() {
            return Err(bad(
                "original Dyn call changes its complete associated binding inventory",
            ));
        }
        for (a, b) in left.associated.iter().zip(&right.associated) {
            if a.member != b.member {
                return Err(bad(
                    "original Dyn call changes an associated member identity",
                ));
            }
            self.unify_dyn_application(
                &a.application,
                &b.application,
                formals,
                solutions,
                implicit,
                invocation,
                depth + 1,
            )?;
            match (&a.value, &b.value) {
                (SemanticAssociatedRecipe::Type(a), SemanticAssociatedRecipe::Type(b)) => {
                    self.unify(a, b, formals, solutions, implicit, invocation, depth + 1)?
                }
                (
                    SemanticAssociatedRecipe::Const {
                        carrier: a,
                        value: av,
                    },
                    SemanticAssociatedRecipe::Const {
                        carrier: b,
                        value: bv,
                    },
                ) => {
                    crate::dyn_metadata::semantic_operand_snapshot(av, self.limits, self.left, 1)?;
                    crate::dyn_metadata::semantic_operand_snapshot(bv, self.limits, self.left, 1)?;
                    let a = SemanticActualRecipe::Const {
                        carrier: copy_recipe(a, self.limits, self.left)?,
                        value: av.clone(),
                    };
                    let b = SemanticActualRecipe::Const {
                        carrier: copy_recipe(b, self.limits, self.left)?,
                        value: bv.clone(),
                    };
                    self.unify_actual(&a, &b, formals, solutions, implicit, invocation, depth + 1)?;
                }
                _ => return Err(bad("original Dyn call changes an associated binding kind")),
            }
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn unify_dyn_application(
        &mut self,
        left: &SemanticTraitRecipe,
        right: &SemanticTraitRecipe,
        formals: &[SemanticFormal],
        solutions: &mut [Option<SemanticActualRecipe>],
        implicit: &mut BTreeMap<u32, SemanticOuterLifeRecipe>,
        invocation: &mut BTreeMap<u32, SemanticOuterLifeRecipe>,
        depth: usize,
    ) -> R<()> {
        self.pay(
            left.arguments.len().saturating_add(right.arguments.len()),
            depth,
        )?;
        if left.declaration != right.declaration || left.arguments.len() != right.arguments.len() {
            return Err(bad(
                "original Dyn call changes its declaration or full mixed actual vector",
            ));
        }
        for (a, b) in left.arguments.iter().zip(&right.arguments) {
            self.unify_actual(a, b, formals, solutions, implicit, invocation, depth + 1)?;
        }
        Ok(())
    }
}

impl Proof {
    /// This relation was proved in the provider's rigid original Gamma by
    /// provider_conformance, before any concrete contract is interpreted.
    pub(crate) fn dyn_provider_conformance(
        &self,
        provider: SemanticDefinitionId,
        declaration: SemanticDefinitionId,
    ) -> bool {
        self.complete.contains(&provider)
            && self.complete.contains(&declaration)
            && self.providers.get(&provider) == Some(&declaration)
    }
}
