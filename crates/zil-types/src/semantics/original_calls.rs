//! Original method and callable schemes. This module receives only immutable
//! declarations and lexical Gamma; no actual Typed instance may supply a premise.
use super::*;

#[derive(Clone)]
pub(crate) struct MethodCall {
    pub declaration: SemanticDefinitionId,
    pub parent: SemanticDefinitionId,
    pub self_type: SemanticTypeRecipe,
    pub application: Option<SemanticTraitRecipe>,
    pub signature: SemanticSignatureRecipe,
    pub receiver_node: Option<NodeId>,
    pub receiver_parameter: Option<u32>,
    pub receiver_borrow: Option<SemanticTypeRecipe>,
    pub receiver_place: Option<MethodReceiverPlace>,
}
#[derive(Clone)]
pub(crate) struct MethodReceiverPlace {
    pub owner: SemanticDefinitionId,
    pub root: SemanticOriginalTypeSource,
    pub fields: Vec<(SemanticDefinitionId, u32)>,
}
#[derive(Clone)]
pub(crate) struct CallableCall {
    pub callee: SemanticTypeRecipe,
    pub callee_node: NodeId,
    pub kind: CallableKind,
    pub signature: SemanticSignatureRecipe,
}
fn compatible(actual: CallableKind, required: CallableKind) -> bool {
    matches!(
        (actual, required),
        (CallableKind::FunctionPointer, CallableKind::FunctionPointer)
            | (
                CallableKind::Fn,
                CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce
            )
            | (
                CallableKind::FnMut,
                CallableKind::FnMut | CallableKind::FnOnce
            )
            | (CallableKind::FnOnce, CallableKind::FnOnce)
    )
}
impl Check<'_, '_> {
    // A declared opaque result exports its original operation contract. Its
    // hidden closure's entry ABI is checked separately and cannot replace it.
    fn opaque_callable_operation(
        &mut self,
        value: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<
        Option<(
            CallableKind,
            SemanticSignatureRecipe,
            BTreeMap<u32, call_regions::Role>,
        )>,
    > {
        self.pay(1, depth)?;
        let SemanticTypeRecipe::Opaque {
            definition,
            arguments,
            hidden,
        } = value
        else {
            return Ok(None);
        };
        let owner = *self
            .contexts
            .last()
            .ok_or_else(|| bad("opaque callable has no original Gamma"))?;
        let declaration = self.definition(*definition)?;
        if declaration.parameters.len() != arguments.len() {
            return Err(bad(
                "original opaque callable omits full explicit arguments",
            ));
        }
        if !self.proof.complete.contains(definition) {
            return Err(bad(
                "opaque callable operation lacks its completed original creator proof",
            ));
        }
        let result = SemanticTypeRecipe::OriginalType {
            scope: SemanticScopeRef::Ancestor(0),
            source: SemanticOriginalTypeSource::Result,
        };
        let mut selected = None;
        for requirement in &declaration.requirements {
            for (_, kind, signature, origins) in
                self.callable_premises(requirement, &result, &mut Vec::new(), depth + 1)?
            {
                if selected.is_some() {
                    return Err(pending(
                        "original opaque callable has several complete result roles",
                    ));
                }
                let signature = super::super::instantiate::callable_signature(
                    self.program,
                    &self.proof,
                    &signature,
                    super::super::instantiate::DomainMapping {
                        source: *definition,
                        use_definition: owner,
                        arguments,
                        parent_call: None,
                    },
                    None,
                    self.limits,
                    self.left,
                )?;
                selected = Some((kind, signature, origins));
            }
        }
        if selected.is_some() {
            if !matches!(&declaration.body, SemanticDefinitionBody::FunctionWithBody {
                signature: SemanticSignatureRecipe { result: SemanticTypeRecipe::Opaque { definition: declared, .. }, .. },
                body: SemanticOriginalFunctionBody::Defined(_), ..
            } if declared == definition)
            {
                return Err(bad(
                    "opaque callable operation does not name its original result creator",
                ));
            }
            self.call_requirements(owner, declaration, arguments, depth + 1)?;
            if !matches!(
                hidden.as_ref(),
                SemanticTypeRecipe::OriginalType {
                    scope: SemanticScopeRef::Ancestor(0),
                    source: SemanticOriginalTypeSource::Result,
                }
            ) {
                let equation = self
                    .proof
                    .types
                    .get(&(*definition, Query::Result))
                    .ok_or_else(|| {
                        bad("opaque callable operation loses its complete all-return equation")
                    })?;
                let equation = copy_recipe(equation, self.limits, self.left)?;
                self.contexts.push(*definition);
                let result = (|| {
                    let hidden = self.resolve(*definition, hidden, depth + 1)?;
                    self.same(&hidden, &equation)
                })();
                self.contexts.pop();
                result?;
            }
        }
        Ok(selected)
    }

    pub(super) fn inference_template(
        &mut self,
        signature: &SemanticSignatureRecipe,
        formals: &[SemanticFormal],
        mark_implicit: bool,
        depth: usize,
    ) -> R<SemanticSignatureRecipe> {
        self.pay(formals.len(), depth)?;
        let mut placeholders = Vec::new();
        for (ordinal, formal) in formals.iter().enumerate() {
            let reference = SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(u32::MAX),
                ordinal: ordinal as u32,
            };
            placeholders.push(match formal {
                SemanticFormal::Type { .. } => {
                    SemanticActualRecipe::Type(SemanticTypeRecipe::Formal(reference))
                }
                SemanticFormal::Lifetime => {
                    SemanticActualRecipe::Life(SemanticLifeRecipe::Formal(reference))
                }
                SemanticFormal::Const { carrier } => SemanticActualRecipe::Const {
                    carrier: copy_recipe(carrier, self.limits, self.left)?,
                    value: SemanticConstRecipe::Formal(reference),
                },
            });
        }
        let frames = [(0, placeholders.as_slice())];
        let selected_frames: &[(u32, &[SemanticActualRecipe])] =
            if formals.is_empty() { &[] } else { &frames };
        let empty = BTreeMap::new();
        let substitution = substitute::Substitution {
            frames: selected_frames,
            selves: &[],
            environments: &[],
            implicit: &empty,
            invocation: &empty,
            shift: 0,
        };
        substitute::signature_template(
            signature,
            &substitution,
            mark_implicit,
            self.limits,
            self.left,
        )
    }
    /// A solved whole-Type actual can determine free mixed variables through
    /// its original callable bounds. These are constraints on this invocation,
    /// never premises imported from the target. Satisfaction still runs after
    /// the complete vector is sealed.
    pub(super) fn infer_callable_bounds(
        &mut self, owner: SemanticDefinitionId, formals: &[SemanticFormal],
        solutions: &mut [Option<SemanticActualRecipe>], depth: usize,
    ) -> R<()> {
        loop {
            self.pay(solutions.len(), depth)?;
            let before = solutions.iter().filter(|slot| slot.is_some()).count();
            if before == solutions.len() { return Ok(()); }
            for formal in formals {
                if let SemanticFormal::Type { requirements } = formal {
                    for requirement in requirements {
                        self.infer_callable_bound(owner, requirement, formals, solutions, depth + 1)?;
                    }
                }
            }
            self.pay(solutions.len(), depth)?;
            if solutions.iter().filter(|slot| slot.is_some()).count() == before { return Ok(()); }
        }
    }
    fn infer_callable_bound(
        &mut self, owner: SemanticDefinitionId, requirement: &SemanticRequirementRecipe,
        formals: &[SemanticFormal], solutions: &mut [Option<SemanticActualRecipe>], depth: usize,
    ) -> R<()> {
        self.pay(solutions.len().saturating_add(1), depth)?;
        if solutions.iter().all(Option::is_some) { return Ok(()); }
        if let SemanticRequirementRecipe::ForAll { requirements, .. } = requirement {
            for requirement in requirements {
                self.infer_callable_bound(owner, requirement, formals, solutions, depth + 1)?;
            }
            return Ok(());
        }
        let SemanticRequirementRecipe::Callable {
            ty: SemanticTypeRecipe::Formal(SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(0), ordinal,
            }), kind, signature,
        } = requirement else { return Ok(()); };
        let Some(Some(SemanticActualRecipe::Type(value))) = solutions.get(*ordinal as usize) else {
            return Ok(());
        };
        let value = copy_recipe(value, self.limits, self.left)?;
        let value = self.observation_type(owner, &value, depth + 1)?;
        let mut selected = self.concrete_callable(&value, depth + 1)?;
        if selected.is_none() {
            for premise in self.original_requirements(owner, depth + 1)?.iter() {
                for (_, candidate_kind, candidate, _) in
                    self.callable_premises(premise, &value, &mut Vec::new(), depth + 1)? {
                    if !compatible(candidate_kind, *kind) { continue; }
                    if let Some((prior_kind, prior)) = &selected {
                        crate::dyn_metadata::semantic_operand_snapshot(prior, self.limits, self.left, 1)?;
                        crate::dyn_metadata::semantic_operand_snapshot(&candidate, self.limits, self.left, 1)?;
                        if prior_kind != &candidate_kind || prior != &candidate {
                            return Err(pending("original mixed inference has several distinct callable operation anchors"));
                        }
                    } else { selected = Some((candidate_kind, candidate)); }
                }
            }
        }
        let Some((actual_kind, actual)) = selected else { return Ok(()); };
        // A candidate's own universally quantified call roles cannot uniquely
        // determine a free target actual. Their instantiation remains with the
        // existing complete callable satisfaction operation.
        if actual.bound_lifetimes != 0 || actual.invocation_lifetimes != 0 { return Ok(()); }
        if !compatible(actual_kind, *kind) { return Err(bad("original mixed callable actual lacks its required invocation kind")); }
        let wanted = self.inference_template(signature, formals, false, depth + 1)?;
        if actual.parameters.len() != wanted.parameters.len()
            || actual.bound_lifetimes != wanted.bound_lifetimes
            || actual.invocation_lifetimes != wanted.invocation_lifetimes
            || actual.call_kind != wanted.call_kind
                && !(*kind == CallableKind::FnOnce && actual.call_kind == CallKind::Direct && wanted.call_kind == CallKind::Suspend)
            || actual.result_mode != wanted.result_mode {
            return Err(bad("original mixed callable actual changes its complete operation scheme"));
        }
        let mut implicit = BTreeMap::new();
        let mut invocation = BTreeMap::new();
        for (parameter, source) in wanted.parameters.iter().zip(&actual.parameters) {
            self.pay(1, depth)?;
            if parameter.mode != source.mode { return Err(bad("original mixed callable actual changes its parameter mode")); }
            self.unify(&parameter.ty, &source.ty, formals, solutions, &mut implicit, &mut invocation, depth + 1)?;
        }
        if let (SemanticTypeRecipe::Address { mutable: requested, inner, .. },
            SemanticTypeRecipe::Address { mutable: available, lifetime: SemanticOuterLifeRecipe::Static, inner: source })
            = (&wanted.result, &actual.result) {
            if *requested && !available { return Err(bad("original mixed callable result strengthens shared access")); }
            self.unify(inner, source, formals, solutions, &mut implicit, &mut invocation, depth + 1)?;
        } else {
            self.unify(&wanted.result, &actual.result, formals, solutions, &mut implicit, &mut invocation, depth + 1)?;
        }
        Ok(())
    }
    fn method_name<'d>(
        &mut self,
        definition: &'d SemanticDefinition,
        parent: &SemanticDefinition,
        depth: usize,
    ) -> R<&'d str> {
        self.pay(
            definition
                .identity
                .declaration_path
                .len()
                .saturating_add(parent.identity.declaration_path.len()),
            depth,
        )?;
        if definition.identity.package != parent.identity.package
            || definition.identity.module_path != parent.identity.module_path
            || definition.identity.generation_path != parent.identity.generation_path
        {
            return Err(bad(
                "original method changes its lexical declaration identity",
            ));
        }
        let suffix = definition
            .identity
            .declaration_path
            .strip_prefix(parent.identity.declaration_path.as_slice())
            .ok_or_else(|| {
                bad("original method identity does not extend its actual lexical parent")
            })?;
        let [member, ordinal, name] = suffix else {
            return Err(pending(
                "original method identity omits its real declared spelling",
            ));
        };
        if member != "member" || ordinal.parse::<u32>().is_err() || name.is_empty() {
            return Err(bad(
                "original method has a malformed declared member identity",
            ));
        }
        self.pay(name.len(), depth)?;
        Ok(name)
    }
    pub(super) fn provider_conformance(
        &mut self,
        provider: &SemanticDefinition,
        depth: usize,
    ) -> R<Option<SemanticDefinitionId>> {
        self.pay(1, depth)?;
        if provider.kind != SemanticDefinitionKind::Method {
            return Ok(None);
        }
        let parent = self.definition(
            provider
                .parent
                .ok_or_else(|| bad("original method omits its lexical parent"))?,
        )?;
        let SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl {
            self_type,
            trait_application,
            ..
        }) = &parent.body
        else {
            return Ok(None);
        };
        let name = self.method_name(provider, parent, depth + 1)?;
        self.pay(self.program.definitions.len(), depth)?;
        let mut trait_scope = None;
        for candidate in &self.program.definitions {
            if matches!(&candidate.body, SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { declaration }) if declaration == &trait_application.declaration)
            {
                if trait_scope.replace(candidate).is_some() {
                    return Err(bad("original trait has duplicate declaration scopes"));
                }
            }
        }
        let trait_scope =
            trait_scope.ok_or_else(|| bad("original provider trait declaration is absent"))?;
        let mut declared = None;
        for candidate in &self.program.definitions {
            if candidate.parent == Some(trait_scope.id)
                && candidate.kind == SemanticDefinitionKind::Method
                && self.method_name(candidate, trait_scope, depth + 1)? == name
            {
                if declared.replace(candidate).is_some() {
                    return Err(bad("original trait repeats its declared method spelling"));
                }
            }
        }
        let declared = declared.ok_or_else(|| {
            bad("original implementation method has no matching trait declaration")
        })?;
        if provider.parameters.len() != declared.parameters.len() {
            return Err(bad(
                "original provider changes complete method mixed formals",
            ));
        }
        let empty = BTreeMap::new();
        let shift = substitute::Substitution {
            frames: &[],
            selves: &[],
            environments: &[],
            implicit: &empty,
            invocation: &empty,
            shift: 1,
        };
        self.pay(trait_application.arguments.len(), depth)?;
        let mut parent_arguments = Vec::new();
        for argument in &trait_application.arguments {
            let shifted = match argument {
                SemanticActualRecipe::Type(ty) => {
                    SemanticActualRecipe::Type(substitute::ty(ty, &shift, self.limits, self.left)?)
                }
                _ => substitute::actual(argument, &shift, self.limits, self.left)?,
            };
            parent_arguments.push(shifted);
        }
        let self_type = substitute::ty(self_type, &shift, self.limits, self.left)?;
        let frames = [(1, parent_arguments.as_slice())];
        let selves = [(1, &self_type)];
        let substitution = substitute::Substitution {
            frames: &frames,
            selves: &selves,
            environments: &[],
            implicit: &empty,
            invocation: &empty,
            shift: 0,
        };
        let SemanticDefinitionBody::FunctionWithBody {
            signature: actual,
            receiver_parameter: actual_receiver,
            ..
        } = &provider.body
        else {
            return Err(bad("original provider omits its complete signature"));
        };
        let SemanticDefinitionBody::FunctionWithBody {
            signature: wanted,
            receiver_parameter: wanted_receiver,
            ..
        } = &declared.body
        else {
            return Err(bad("original trait member omits its complete signature"));
        };
        if actual_receiver != wanted_receiver {
            return Err(bad(
                "original provider changes the declared receiver parameter identity",
            ));
        }
        let wanted = substitute::signature(wanted, &substitution, self.limits, self.left)?;
        crate::dyn_metadata::semantic_operand_snapshot(actual, self.limits, self.left, 1)?;
        let mut actual = actual.clone();
        for parameter in &mut actual.parameters {
            parameter.ty = self.resolve(provider.id, &parameter.ty, depth + 1)?;
        }
        actual.result = self.resolve(provider.id, &actual.result, depth + 1)?;
        self.compare_callable(&actual, &wanted, depth + 1)?;
        // A provider cannot strengthen an abstract method's generic premises.
        // Compare in the provider's quantified lexical domain, before any call.
        for (actual, wanted) in provider.parameters.iter().zip(&declared.parameters) {
            match (actual, wanted) {
                (
                    SemanticFormal::Type { requirements: a },
                    SemanticFormal::Type { requirements: b },
                ) => {
                    if a.len() != b.len() {
                        return Err(bad(
                            "original provider strengthens declared Type requirements",
                        ));
                    }
                    for (a, b) in a.iter().zip(b) {
                        let b = substitute::requirement(b, &substitution, self.limits, self.left)?;
                        if a != &b {
                            return Err(bad(
                                "original provider changes declared Type requirements",
                            ));
                        }
                    }
                }
                (SemanticFormal::Lifetime, SemanticFormal::Lifetime) => {}
                (SemanticFormal::Const { carrier: a }, SemanticFormal::Const { carrier: b }) => {
                    let b = substitute::ty(b, &substitution, self.limits, self.left)?;
                    self.same(a, &b)?;
                }
                _ => {
                    return Err(bad(
                        "original provider changes a complete mixed formal kind",
                    ));
                }
            }
        }
        if provider.requirements.len() != declared.requirements.len() {
            return Err(bad(
                "original provider strengthens declared method where requirements",
            ));
        }
        for (actual, wanted) in provider.requirements.iter().zip(&declared.requirements) {
            let wanted = substitute::requirement(wanted, &substitution, self.limits, self.left)?;
            if actual != &wanted {
                return Err(bad(
                    "original provider changes a declared method where requirement",
                ));
            }
        }
        Ok(Some(declared.id))
    }
    fn type_path(
        &mut self,
        owner: SemanticDefinitionId,
        path: &[String],
        depth: usize,
    ) -> R<Option<SemanticTypeRecipe>> {
        let definition = self.definition(owner)?;
        let SemanticDefinitionBody::FunctionWithBody { names, .. } = &definition.body else {
            return Err(bad(
                "original Type lookup has no lexical declaration environment",
            ));
        };
        self.pay(names.len(), depth)?;
        let mut selected = None;
        for binding in names {
            if binding.namespace != SemanticOriginalNamespace::Type || binding.path != path {
                continue;
            }
            if selected.is_some() {
                return Err(bad("original Type path is ambiguously bound"));
            }
            if let SemanticOriginalNameValue::Recipe(SemanticActualRecipe::Type(ty)) =
                &binding.value
            {
                selected = Some(self.resolve(owner, ty, depth + 1)?);
            } else if let SemanticOriginalNameValue::Definition(target) = &binding.value {
                let target = self.definition(*target)?;
                if !target.parameters.is_empty() {
                    return Err(pending(
                        "original generic Type name requires its full written arguments",
                    ));
                }
                if !matches!(target.body, SemanticDefinitionBody::Nominal { .. }) {
                    continue;
                }
                selected = Some(SemanticTypeRecipe::Nominal {
                    definition: target.id,
                    arguments: Vec::new(),
                });
            } else {
                return Err(bad("original Type path binds another semantic kind"));
            }
        }
        Ok(selected)
    }
    fn source_self_type(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        let mut ty = if let Some(path) = self.declaration_path(expression, variables, depth + 1)? {
            self.type_path(owner, &path, depth + 1)?
                .ok_or_else(|| bad("original static method receiver has no real Type binding"))?
        } else {
            self.place(owner, expression, variables, depth + 1)?
        };
        loop {
            self.pay(1, depth)?;
            match ty {
                SemanticTypeRecipe::Address { inner, .. }
                | SemanticTypeRecipe::Desc { inner, .. }
                | SemanticTypeRecipe::Managed(inner) => ty = *inner,
                _ => return self.observation_type(owner, &ty, depth + 1),
            }
        }
    }
    pub(super) fn original_requirements(
        &mut self,
        owner: SemanticDefinitionId,
        depth: usize,
    ) -> R<std::rc::Rc<Vec<SemanticRequirementRecipe>>> {
        self.pay(1, depth)?;
        if let Some(premises) = self.proof.premises.lookup(owner, self.left)? {
            return Ok(premises);
        }
        let mut target = Some(owner);
        let mut hops = 0u32;
        let mut requirements = Vec::new();
        while let Some(id) = target {
            self.pay(1, depth + hops as usize)?;
            let definition = self.definition(id)?;
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
                self.pay(1, depth)?;
                let empty = BTreeMap::new();
                let substitution = substitute::Substitution {
                    frames: &[],
                    selves: &[],
                    environments: &[],
                    implicit: &empty,
                    invocation: &empty,
                    shift: hops,
                };
                requirements.push(substitute::requirement(
                    requirement,
                    &substitution,
                    self.limits,
                    self.left,
                )?);
            }
            if let SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { declaration }) =
                &definition.body
            {
                let arguments = self.original_arguments(id, hops, depth + 1)?;
                self.pay(1, depth)?;
                requirements.push(SemanticRequirementRecipe::Trait {
                    ty: SemanticTypeRecipe::SelfType {
                        scope: SemanticScopeRef::Ancestor(hops),
                    },
                    application: SemanticTraitRecipe {
                        declaration: *declaration,
                        arguments,
                    },
                });
            }
            // Closure free semantic operands cross their explicit environment
            // boundary, never a guessed implicit ancestor substitution.
            if definition.kind == SemanticDefinitionKind::Closure {
                if let Some(parent) = definition.parent {
                    let empty = BTreeMap::new();
                    let shift = substitute::Substitution {
                        frames: &[],
                        selves: &[],
                        environments: &[],
                        implicit: &empty,
                        invocation: &empty,
                        shift: 1,
                    };
                    let mut equations = Vec::new();
                    for slot in 0..definition.environment.len() {
                        self.pay(1, depth)?;
                        let Some((source_owner, source)) =
                            self.proof.environments.get(&(id, slot as u32))
                        else {
                            continue;
                        };
                        if *source_owner != parent {
                            return Err(bad("original capture premise changes its source owner"));
                        }
                        let source = copy_recipe(source, self.limits, self.left)?;
                        let source = substitute::ty(&source, &shift, self.limits, self.left)?;
                        equations.push((
                            source,
                            SemanticTypeRecipe::Environment {
                                scope: SemanticScopeRef::Ancestor(0),
                                ordinal: slot as u32,
                            },
                        ));
                    }
                    for premise in self.original_requirements(parent, depth + 1)?.iter() {
                        let premise =
                            substitute::requirement(premise, &shift, self.limits, self.left)?;
                        if let Some(premise) =
                            self.capture_premise(&premise, &equations, depth + 1)?
                        {
                            requirements.push(substitute::requirement(
                                &premise,
                                &substitute::Substitution {
                                    shift: hops,
                                    ..shift
                                },
                                self.limits,
                                self.left,
                            )?);
                        }
                    }
                }
                break;
            }
            target = definition.parent;
            hops = hops
                .checked_add(1)
                .ok_or_else(|| bad("original premise ancestry overflows"))?;
        }
        self.extend_associated_requirements(owner, &mut requirements, depth + 1)?;
        self.pay(1, depth)?;
        let requirements = std::rc::Rc::new(requirements);
        self.proof
            .premises
            .remember(owner, &requirements, self.limits, self.left)?;
        Ok(requirements)
    }
    fn capture_premise(
        &mut self,
        premise: &SemanticRequirementRecipe,
        equations: &[(SemanticTypeRecipe, SemanticTypeRecipe)],
        depth: usize,
    ) -> R<Option<SemanticRequirementRecipe>> {
        self.pay(equations.len().saturating_add(1), depth)?;
        if let SemanticRequirementRecipe::ForAll {
            lifetimes,
            requirements,
        } = premise
        {
            let mut selected = Vec::new();
            for requirement in requirements {
                if let Some(requirement) =
                    self.capture_premise(requirement, equations, depth + 1)?
                {
                    selected.push(requirement);
                }
            }
            return Ok(
                (!selected.is_empty()).then_some(SemanticRequirementRecipe::ForAll {
                    lifetimes: *lifetimes,
                    requirements: selected,
                }),
            );
        }
        let subject = match premise {
            SemanticRequirementRecipe::Callable { ty, .. }
            | SemanticRequirementRecipe::Structural { ty, .. }
            | SemanticRequirementRecipe::Builtin { ty, .. }
            | SemanticRequirementRecipe::Trait { ty, .. } => ty,
            _ => return Ok(None),
        };
        if !equations.iter().any(|(source, _)| source == subject) {
            return Ok(None);
        }
        // These equations come from the original symbolic creator binding,
        // never the concrete captured Type or its inferred capabilities.
        Ok(Some(substitute::requirement_replacing(
            premise,
            equations,
            self.limits,
            self.left,
        )?))
    }
    fn trait_application(
        &mut self,
        owner: SemanticDefinitionId,
        self_type: &SemanticTypeRecipe,
        declaration: LifeDeclarationId,
        depth: usize,
    ) -> R<SemanticTraitRecipe> {
        let requirements = self.original_requirements(owner, depth + 1)?;
        let mut selected: Option<SemanticTraitRecipe> = None;
        for requirement in requirements.iter() {
            if let SemanticRequirementRecipe::Trait { ty, application } = requirement {
                if ty == self_type && application.declaration == declaration {
                    if selected.as_ref().is_some_and(|old| old != application) {
                        return Err(pending(
                            "original method has several distinct trait applications and needs an explicit qualification",
                        ));
                    }
                    crate::dyn_metadata::semantic_operand_snapshot(
                        application,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    selected = Some(application.clone());
                }
            }
        }
        selected.ok_or_else(|| {
            bad("original caller lacks the declared trait premise for its static member")
        })
    }
    pub(super) fn solve_parent(
        &mut self,
        owner: SemanticDefinitionId,
        parent: &SemanticDefinition,
        self_type: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<Vec<SemanticActualRecipe>> {
        if matches!(&parent.body, SemanticDefinitionBody::Nominal { .. }) {
            let SemanticTypeRecipe::Nominal { definition, arguments } = self_type else {
                return Err(bad("original member receiver has no nominal declaration"));
            };
            if *definition != parent.id {
                return Err(bad("original member receiver selects another nominal declaration"));
            }
            self.call_requirements(owner, parent, arguments, depth + 1)?;
            self.pay(arguments.len(), depth)?;
            for argument in arguments {
                crate::dyn_metadata::semantic_operand_snapshot(argument, self.limits, self.left, 1)?;
            }
            return Ok(arguments.clone());
        }
        let SemanticDefinitionBody::Scope(
            SemanticScopeRecipe::InherentImpl {
                self_type: template,
                ..
            }
            | SemanticScopeRecipe::TraitImpl {
                self_type: template,
                ..
            }
            | SemanticScopeRecipe::BuiltinImpl {
                self_type: template,
                ..
            },
        ) = &parent.body
        else {
            return Err(bad("original method has no implementation parent scheme"));
        };
        self.pay(parent.parameters.len(), depth)?;
        let mut solutions = vec![None; parent.parameters.len()];
        let template_signature = SemanticSignatureRecipe {
            bound_lifetimes: 0,
            invocation_lifetimes: 0,
            parameters: Vec::new(),
            result: copy_recipe(template, self.limits, self.left)?,
            result_mode: PassingMode::Infer,
            call_kind: CallKind::Direct,
        };
        let template = self
            .inference_template(&template_signature, &parent.parameters, false, depth + 1)?
            .result;
        self.unify(
            &template,
            self_type,
            &parent.parameters,
            &mut solutions,
            &mut BTreeMap::new(),
            &mut BTreeMap::new(),
            depth + 1,
        )?;
        self.pay(solutions.len(), depth)?;
        let actuals = solutions
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| {
                pending("original receiver does not determine every mixed implementation formal")
            })?;
        self.call_requirements(owner, parent, &actuals, depth + 1)?;
        Ok(actuals)
    }
    pub(super) fn method_call(
        &mut self,
        owner: SemanticDefinitionId,
        site: SemanticSiteId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<Option<SemanticTypeRecipe>> {
        self.pay(1, depth)?;
        let CoreExprKind::Call { callee, arguments } = &expression.kind else {
            unreachable!()
        };
        let mut base = callee.as_ref();
        while let CoreExprKind::GenericApply { callee, .. }
        | CoreExprKind::TypeApply { callee, .. } = &base.kind
        {
            self.pay(1, depth)?;
            base = callee;
        }
        let CoreExprKind::Member { object, member, .. } = &base.kind else {
            return Ok(None);
        };
        let mut target = None;
        if let Some(path) = self.declaration_path(base, variables, depth + 1)? {
            if let Some(candidate) = self.declaration_target(owner, &path, depth + 1)? {
                if self.definition(candidate)?.kind != SemanticDefinitionKind::Method {
                    return Ok(None);
                }
                target = Some(candidate);
            }
        }
        let self_type = match target {
            Some(_) => self.source_self_type(owner, object, variables, depth + 1)?,
            None => {
                // A runtime receiver remains lexical. Candidate spellings come
                // exclusively from actual original declaration bindings.
                let definition = self.definition(owner)?;
                let SemanticDefinitionBody::FunctionWithBody { names, .. } = &definition.body
                else {
                    unreachable!()
                };
                let self_type = if self
                    .declaration_path(object, variables, depth + 1)?
                    .is_some()
                {
                    return Ok(None);
                } else {
                    self.source_self_type(owner, object, variables, depth + 1)?
                };
                for binding in names {
                    self.pay(1, depth)?;
                    if binding.namespace != SemanticOriginalNamespace::Value
                        || binding.path.last() != Some(member)
                    {
                        continue;
                    }
                    let SemanticOriginalNameValue::Definition(candidate) = binding.value else {
                        continue;
                    };
                    let method = self.definition(candidate)?;
                    if method.kind != SemanticDefinitionKind::Method {
                        continue;
                    }
                    let parent = self.definition(
                        method
                            .parent
                            .ok_or_else(|| bad("original method has no lexical parent"))?,
                    )?;
                    let applicable = match &parent.body {
                        SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait {
                            declaration,
                        }) => self
                            .trait_application(owner, &self_type, *declaration, depth + 1)
                            .map(|_| ()),
                        _ => self
                            .solve_parent(owner, parent, &self_type, depth + 1)
                            .map(|_| ()),
                    };
                    match applicable {
                        Ok(()) => {
                            if target.is_some_and(|old| old != candidate) {
                                return Err(bad(
                                    "original receiver selects ambiguous method declarations",
                                ));
                            }
                            target = Some(candidate);
                        }
                        Err(diagnostic)
                            if diagnostic.category == zil_base::ResultCategory::ResourceLimit
                                || diagnostic.category == zil_base::ResultCategory::Unsupported =>
                        {
                            return Err(diagnostic);
                        }
                        Err(_) => {}
                    }
                }
                self_type
            }
        };
        let Some(target) = target else {
            return Ok(None);
        };
        let definition = self.definition(target)?;
        let parent = self.definition(
            definition
                .parent
                .ok_or_else(|| bad("original method has no real lexical parent"))?,
        )?;
        let (parent_actuals, application) = match &parent.body {
            SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { declaration }) => {
                let application =
                    self.trait_application(owner, &self_type, *declaration, depth + 1)?;
                self.call_requirements(owner, parent, &application.arguments, depth + 1)?;
                (application.arguments.clone(), Some(application))
            }
            SemanticDefinitionBody::Nominal { .. } | SemanticDefinitionBody::Scope(
                SemanticScopeRecipe::InherentImpl { .. }
                | SemanticScopeRecipe::TraitImpl { .. }
                | SemanticScopeRecipe::BuiltinImpl { .. },
            ) => {
                let actuals = self.solve_parent(owner, parent, &self_type, depth + 1)?;
                (actuals, None)
            }
            _ => return Err(bad("original member target has another lexical scope kind")),
        };
        if !self.proof.complete.contains(&target) && !self.active.contains(&target) {
            self.function(definition, None)?;
        }
        let SemanticDefinitionBody::FunctionWithBody {
            signature,
            receiver_parameter,
            ..
        } = &definition.body
        else {
            return Err(bad("original method omits its full value parameter scheme"));
        };
        let parent_frames = [(1, parent_actuals.as_slice())];
        let selves = [(1, &self_type)];
        let empty = BTreeMap::new();
        let parent_substitution = substitute::Substitution {
            frames: &parent_frames,
            selves: &selves,
            environments: &[],
            implicit: &empty,
            invocation: &empty,
            shift: 0,
        };
        crate::dyn_metadata::semantic_operand_snapshot(signature, self.limits, self.left, 1)?;
        let mut effective = signature.clone();
        effective.result = self.effective_result(target, &signature.result, depth + 1)?;
        let signature =
            self.inference_template(&effective, &definition.parameters, true, depth + 1)?;
        let signature =
            substitute::signature(&signature, &parent_substitution, self.limits, self.left)?;
        self.pay(
            signature
                .parameters
                .len()
                .saturating_add(definition.parameters.len()),
            depth,
        )?;
        let mut operands = Vec::new();
        let mut written = arguments.iter();
        for ordinal in 0..signature.parameters.len() {
            if *receiver_parameter == Some(ordinal as u32) {
                operands.push(object.as_ref());
            } else {
                operands.push(
                    written
                        .next()
                        .ok_or_else(|| bad("original method omits a declared argument"))?,
                );
            }
        }
        if written.next().is_some() {
            return Err(bad("original method adds an undeclared argument"));
        }
        let receiver_place = if receiver_parameter.is_some() {
            self.method_receiver_place(owner, object, variables, depth + 1)?
        } else {
            None
        };
        let mut receiver_borrow = None;
        let mut roles = Vec::new();
        if let Some(ordinal) = receiver_parameter {
            let parameter = signature
                .parameters
                .get(*ordinal as usize)
                .ok_or_else(|| bad("method receiver ordinal exceeds its full signature"))?;
            if let SemanticTypeRecipe::Address { mutable, .. } = &parameter.ty {
                let value = self.place(owner, object, variables, depth + 1)?;
                let value = self.observation_type(owner, &value, depth + 1)?;
                if !matches!(value, SemanticTypeRecipe::Address { .. }) {
                    if *mutable {
                        self.writable(owner, object, variables, depth + 1)?;
                    }
                    let lifetime = match self.borrow_life(owner, object.id) {
                        Ok(lifetime) => lifetime,
                        Err(error) if error.category == zil_base::ResultCategory::Unsupported => {
                            let Some(place) = receiver_place.as_ref() else {
                                return Err(error);
                            };
                            let SemanticOriginalTypeSource::Parameter(ordinal) = &place.root else {
                                return Err(error);
                            };
                            let root = self
                                .proof
                                .types
                                .get(&(owner, Query::Parameter(*ordinal)))
                                .ok_or_else(|| {
                                bad("method field receiver has no original root parameter")
                            })?;
                            let SemanticTypeRecipe::Address {
                                mutable: available,
                                lifetime,
                                ..
                            } = root
                            else {
                                return Err(error);
                            };
                            if *mutable && !available {
                                return Err(bad(
                                    "method receiver borrows through a shared original root",
                                ));
                            }
                            crate::dyn_metadata::semantic_operand_snapshot(
                                lifetime,
                                self.limits,
                                self.left,
                                1,
                            )?;
                            lifetime.clone()
                        }
                        Err(error) => return Err(error),
                    };
                    self.pay(2, depth)?;
                    let borrowed = SemanticTypeRecipe::Address {
                        mutable: *mutable,
                        lifetime,
                        inner: Box::new(value),
                    };
                    roles.push((object.id, copy_recipe(&borrowed, self.limits, self.left)?));
                    receiver_borrow = Some(borrowed);
                }
            }
        }
        let holes = vec![SemanticActualRecipe::Infer; definition.parameters.len()];
        let (actuals, solved) = self.solve_signature_roles(
            owner,
            expression,
            callee,
            &operands,
            &signature,
            &definition.parameters,
            &holes,
            variables,
            wanted,
            true,
            &BTreeMap::new(),
            &roles,
            0,
            None,
            depth + 1,
        )?;
        if receiver_borrow.is_some() {
            let ordinal = receiver_parameter
                .ok_or_else(|| bad("implicit method borrow has no original receiver ordinal"))?;
            receiver_borrow = Some(copy_recipe(
                &solved
                    .parameters
                    .get(ordinal as usize)
                    .ok_or_else(|| bad("solved method receiver is absent"))?
                    .ty,
                self.limits,
                self.left,
            )?);
        }
        // Method requirements may reference both the method and parent frame.
        let frames = [(0, actuals.as_slice()), (1, parent_actuals.as_slice())];
        let substitution = substitute::Substitution {
            frames: &frames,
            selves: &selves,
            environments: &[],
            implicit: &empty,
            invocation: &empty,
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
        self.pay(parent_actuals.len().saturating_add(3), depth)?;
        for actual in &parent_actuals {
            crate::dyn_metadata::semantic_operand_snapshot(actual, self.limits, self.left, 1)?;
        }
        self.proof
            .proof_mut()?
            .scope_calls
            .insert((owner, site, parent.id), parent_actuals);
        self.proof
            .proof_mut()?
            .call_targets
            .insert((owner, site), target);
        self.proof.proof_mut()?.calls.insert((owner, site), actuals);
        crate::dyn_metadata::semantic_operand_snapshot(&self_type, self.limits, self.left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(&solved, self.limits, self.left, 1)?;
        self.proof.proof_mut()?.method_calls.insert(
            (owner, site),
            MethodCall {
                declaration: target,
                parent: parent.id,
                self_type,
                application,
                signature: solved.clone(),
                receiver_node: receiver_parameter.map(|_| object.id),
                receiver_parameter: *receiver_parameter,
                receiver_borrow,
                receiver_place,
            },
        );
        Ok(Some(solved.result))
    }
    pub(super) fn concrete_callable(
        &mut self,
        value: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<Option<(CallableKind, SemanticSignatureRecipe)>> {
        self.pay(1, depth)?;
        let owner = self
            .contexts
            .last()
            .copied()
            .ok_or_else(|| bad("callable extraction has no actual lexical Gamma"))?;
        match value {
            SemanticTypeRecipe::Opaque {
                definition,
                arguments,
                hidden,
            } => {
                if let Some((kind, signature, _)) = self.opaque_callable_operation(value, depth + 1)? {
                    return Ok(Some((kind, signature)));
                }
                let declaration = self.definition(*definition)?;
                if declaration.parameters.len() != arguments.len() {
                    return Err(bad(
                        "original opaque callable omits full explicit arguments",
                    ));
                }
                // The hidden value belongs to this original definition, not
                // the use's same-numbered lexical ancestors. First extract
                // inside that sealed domain, then substitute all actuals and
                // transport the signature through the genuine boundary.
                self.contexts.push(*definition);
                let extracted = self.concrete_callable(hidden, depth + 1);
                self.contexts.pop();
                let Some((kind, signature)) = extracted? else {
                    return Ok(None);
                };
                let signature = super::super::instantiate::callable_signature(
                    self.program,
                    &self.proof,
                    &signature,
                    super::super::instantiate::DomainMapping {
                        source: *definition,
                        use_definition: owner,
                        arguments,
                        parent_call: None,
                    },
                    None,
                    self.limits,
                    self.left,
                )?;
                Ok(Some((kind, signature)))
            }
            SemanticTypeRecipe::Closure {
                definition,
                arguments,
                environment,
            } => {
                let declaration = self.definition(*definition)?;
                if !self.proof.complete.contains(definition) {
                    return Err(bad(
                        "original closure has no completed creator and code proof",
                    ));
                }
                let kind =
                    *self.proof.kinds.get(definition).ok_or_else(|| {
                        bad("original closure has no checked capture-effect kind")
                    })?;
                let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &declaration.body
                else {
                    return Err(bad("original closure omits its complete signature"));
                };
                crate::dyn_metadata::semantic_operand_snapshot(
                    signature,
                    self.limits,
                    self.left,
                    1,
                )?;
                let mut signature = signature.clone();
                for parameter in &mut signature.parameters {
                    parameter.ty = self.resolve(*definition, &parameter.ty, depth + 1)?;
                }
                signature.result =
                    self.effective_result(*definition, &signature.result, depth + 1)?;
                Ok(Some((
                    kind,
                    super::super::instantiate::callable_signature(
                        self.program,
                        &self.proof,
                        &signature,
                        super::super::instantiate::DomainMapping {
                            source: *definition,
                            use_definition: owner,
                            arguments,
                            parent_call: None,
                        },
                        Some(environment),
                        self.limits,
                        self.left,
                    )?,
                )))
            }
            SemanticTypeRecipe::Code { signature, .. } => {
                crate::dyn_metadata::semantic_operand_snapshot(
                    signature.as_ref(),
                    self.limits,
                    self.left,
                    1,
                )?;
                Ok(Some((
                    CallableKind::FunctionPointer,
                    signature.as_ref().clone(),
                )))
            }
            _ => Ok(None),
        }
    }
    fn compare_callable(
        &mut self,
        actual: &SemanticSignatureRecipe,
        wanted: &SemanticSignatureRecipe,
        depth: usize,
    ) -> R<()> {
        self.compare_callable_under(actual, wanted, &[], depth)
    }
    fn compare_callable_under(
        &mut self,
        actual: &SemanticSignatureRecipe,
        wanted: &SemanticSignatureRecipe,
        universal: &[u32],
        depth: usize,
    ) -> R<()> {
        self.compare_callable_operation(actual, wanted, false, universal, depth)
    }
    fn compare_callable_operation(
        &mut self,
        actual: &SemanticSignatureRecipe,
        wanted: &SemanticSignatureRecipe,
        consuming: bool,
        universal: &[u32],
        depth: usize,
    ) -> R<()> {
        self.pay(actual.parameters.len(), depth)?;
        if actual.parameters.len() != wanted.parameters.len()
            || actual.bound_lifetimes != wanted.bound_lifetimes
            || universal.is_empty() && actual.invocation_lifetimes != wanted.invocation_lifetimes
            || actual.call_kind != wanted.call_kind
                && !(consuming && actual.call_kind == CallKind::Direct
                    && wanted.call_kind == CallKind::Suspend)
            || actual.result_mode != wanted.result_mode
        {
            self.pay(384, depth)?;
            let mut diagnostic = bad("original callable changes its complete scoped call scheme");
            diagnostic.notes.push(format!(
                "original callable scheme: actual P={} Bound={} Invocation={} ABI={:?} resultMode={:?}; wanted P={} Bound={} Invocation={} ABI={:?} resultMode={:?}",
                actual.parameters.len(), actual.bound_lifetimes, actual.invocation_lifetimes, actual.call_kind, actual.result_mode,
                wanted.parameters.len(), wanted.bound_lifetimes, wanted.invocation_lifetimes, wanted.call_kind, wanted.result_mode
            ));
            diagnostic.notes.push(format!(
                "original callable owner: {:?}",
                self.contexts.last()
            ));
            return Err(diagnostic);
        }
        let actual = self.inference_template(actual, &[], true, depth + 1)?;
        let mut implicit = BTreeMap::new();
        let mut invocation = BTreeMap::new();
        for (a, b) in actual.parameters.iter().zip(&wanted.parameters) {
            if a.mode != b.mode {
                return Err(bad(
                    "original callable changes a declared parameter passing mode",
                ));
            }
            self.unify(
                &a.ty,
                &b.ty,
                &[],
                &mut [],
                &mut implicit,
                &mut invocation,
                depth + 1,
            )?;
        }
        if !universal.is_empty() {
            // A goal's external Bound frames are rigid. Instantiate only the
            // independently quantified candidate Invocation directory against
            // their complete P/R occurrences, including retained children.
            match (&actual.result, &wanted.result) {
                (
                    SemanticTypeRecipe::Address {
                        mutable: available,
                        lifetime: a,
                        inner: ai,
                    },
                    SemanticTypeRecipe::Address {
                        mutable: requested,
                        lifetime: b,
                        inner: bi,
                    },
                ) => {
                    if *requested && !available {
                        return Err(bad(
                            "original universal callable result strengthens shared access",
                        ));
                    }
                    self.unify(
                        ai,
                        bi,
                        &[],
                        &mut [],
                        &mut implicit,
                        &mut invocation,
                        depth + 1,
                    )?;
                    if !matches!(a, SemanticOuterLifeRecipe::Static) {
                        self.unify_outer(
                            a,
                            b,
                            &[],
                            &mut [],
                            &mut implicit,
                            &mut invocation,
                            depth + 1,
                        )?;
                    }
                }
                _ => self.unify(
                    &actual.result,
                    &wanted.result,
                    &[],
                    &mut [],
                    &mut implicit,
                    &mut invocation,
                    depth + 1,
                )?,
            }
            self.pay(actual.invocation_lifetimes as usize, depth)?;
            if actual.invocation_lifetimes as usize > self.limits.max_nodes {
                return Err(Diagnostic::limit(
                    Phase::Types,
                    "universal callable invocation domain exceeded",
                    None,
                ));
            }
            let mut arbitrary = None;
            for (frame, count) in universal.iter().enumerate().rev() {
                if *count != 0 {
                    arbitrary = Some(SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Bound {
                        depth: u32::try_from(universal.len() - frame - 1)
                            .map_err(|_| bad("universal callable Bound depth overflows"))?
                            .checked_add(u32::from(wanted.bound_lifetimes != 0))
                            .ok_or_else(|| bad("universal callable Bound frame overflows"))?,
                        index: 0,
                    }));
                    break;
                }
            }
            if arbitrary.is_none() && wanted.invocation_lifetimes != 0 {
                arbitrary = Some(SemanticOuterLifeRecipe::Class(
                    SemanticLifeRecipe::Invocation(0),
                ));
            }
            // Keep every original ordinal, even an unused phantom. Assigning
            // its arbitrary instance does not collapse the candidate domain.
            for ordinal in 0..actual.invocation_lifetimes {
                if !invocation.contains_key(&ordinal) {
                    let image = arbitrary.as_ref().ok_or_else(|| {
                        pending(
                            "universal callable phantom role has no complete instantiation domain",
                        )
                    })?;
                    crate::dyn_metadata::semantic_operand_snapshot(
                        image,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    invocation.insert(ordinal, image.clone());
                }
            }
            if actual.bound_lifetimes != 0 {
                // Solver images were observed inside this signature's own
                // Bound frame. Store them outside that frame before the
                // substitution traversal lifts them back into each occurrence.
                for image in implicit.values_mut().chain(invocation.values_mut()) {
                    if let SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Bound {
                        depth, ..
                    }) = image
                    {
                        *depth = depth.checked_sub(1).ok_or_else(|| bad("invocation instantiation depends on a signature-private Bound role"))?;
                    }
                }
            }
        }
        let substitution = substitute::Substitution {
            frames: &[],
            selves: &[],
            environments: &[],
            implicit: &implicit,
            invocation: &invocation,
            shift: 0,
        };
        let result =
            substitute::signature_fresh(&actual, &substitution, self.limits, self.left)?.result;
        self.callable_result(&result, &wanted.result, depth + 1)
    }
    fn callable_result(
        &mut self,
        actual: &SemanticTypeRecipe,
        wanted: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        if let (
            SemanticTypeRecipe::Address {
                mutable: available,
                lifetime: a,
                inner: ai,
            },
            SemanticTypeRecipe::Address {
                mutable: requested,
                lifetime: b,
                inner: bi,
            },
        ) = (actual, wanted)
        {
            if *requested && !available {
                return Err(bad("original callable result strengthens shared access"));
            }
            self.same(ai, bi)?;
            if a == b || matches!(a, SemanticOuterLifeRecipe::Static) {
                return Ok(());
            }
            return Err(bad(
                "original callable result lacks its scoped outer-role relation",
            ));
        }
        self.same(actual, wanted)
    }
    pub(super) fn callable_requirement(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        required: CallableKind,
        wanted: &SemanticSignatureRecipe,
        depth: usize,
    ) -> R<()> {
        self.callable_requirement_under(owner, ty, required, wanted, &[], depth)
    }
    pub(super) fn callable_requirement_under(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        required: CallableKind,
        wanted: &SemanticSignatureRecipe,
        universal: &[u32],
        depth: usize,
    ) -> R<()> {
        let value = self.observation_type(owner, ty, depth + 1)?;
        if let Some((kind, mut actual)) = self.concrete_callable(&value, depth + 1)? {
            // A stronger Fixed capture result may meet a receiver-bounded
            // operation through its complete original return derivations.
            // This local signature view is not stored as the entry's result.
            if matches!(&wanted.result, SemanticTypeRecipe::Address { mutable: false, lifetime, .. }
                if views::is_receiver_upper(lifetime))
                && matches!(&actual.result, SemanticTypeRecipe::Address { mutable: false, lifetime, .. }
                    if !views::is_receiver_upper(lifetime) && !matches!(lifetime, SemanticOuterLifeRecipe::Static))
            {
                let SemanticTypeRecipe::Closure { definition, .. } = &value else {
                    return Err(bad("receiver result adapter lacks its exact concrete closure"));
                };
                if !matches!(required, CallableKind::Fn | CallableKind::FnMut)
                    || wanted.call_kind != CallKind::Direct
                    || !self.proof.receiver_result_upper(self.program, *definition, self.limits, self.left)?
                { return Err(bad("receiver result lacks complete original capture return derivations")); }
                let SemanticTypeRecipe::Address { lifetime, .. } = &mut actual.result else { unreachable!() };
                *lifetime = views::receiver_upper();
            }
            if !compatible(kind, required) {
                return Err(bad(
                    "original concrete callable lacks the required invocation kind",
                ));
            }
            self.compare_callable_operation(&actual, wanted, required == CallableKind::FnOnce, universal, depth + 1)?;
            // Type compatibility does not settle a consuming invocation's
            // environment. The original protocol pass checks each real call
            // against its actual inputs and execution root.
            return Ok(());
        }
        for requirement in self.original_requirements(owner, depth + 1)?.iter() {
            for (premise, kind, signature, _) in
                self.callable_premises(requirement, &value, &mut Vec::new(), depth + 1)?
            {
                if premise == &value && compatible(kind, required) {
                    if required == CallableKind::FnOnce && wanted.call_kind == CallKind::Direct
                        && kind != CallableKind::FnOnce
                        && !self.cleanup_capability(owner, &value, depth + 1)? {
                        // A separate, stronger original FnOnce premise may
                        // still supply the complete consuming operation.
                        continue;
                    }
                    self.compare_callable_operation(&signature, wanted, required == CallableKind::FnOnce, universal, depth + 1)?;
                    return Ok(());
                }
            }
        }
        Err(bad(
            "original caller lacks a complete callable role premise",
        ))
    }
    pub(super) fn direct_callable_premise(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        consuming: bool,
        depth: usize,
    ) -> R<bool> {
        let value = self.observation_type(owner, ty, depth + 1)?;
        for requirement in self.original_requirements(owner, depth + 1)?.iter() {
            for (_, kind, signature, _) in
                self.callable_premises(requirement, &value, &mut Vec::new(), depth + 1)?
            {
                if signature.call_kind == CallKind::Direct
                    && (!consuming
                        || kind == CallableKind::FnOnce
                        || self.cleanup_capability(owner, &value, depth + 1)?)
                {
                    return Ok(true);
                }
            }
        }
        if let Some((kind, signature)) = self.concrete_callable(&value, depth + 1)? {
            // An opaque operation is proved at every real defining return.
            // A bare closure entry alone never proves its remaining cleanup.
            return Ok(signature.call_kind == CallKind::Direct
                && (!consuming
                    || matches!(value, SemanticTypeRecipe::Opaque { .. })
                        && kind == CallableKind::FnOnce
                    || self.cleanup_capability(owner, &value, depth + 1)?));
        }
        Ok(false)
    }
    pub(super) fn direct_consuming_bound(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<bool> {
        for requirement in self.original_requirements(owner, depth + 1)?.iter() {
            for (_, kind, signature, _) in
                self.callable_premises(requirement, ty, &mut Vec::new(), depth + 1)?
            {
                if kind == CallableKind::FnOnce && signature.call_kind == CallKind::Direct {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    pub(super) fn direct_consuming_inputs(
        &mut self,
        requirements: &[SemanticRequirementRecipe],
        inputs: &mut Vec<SemanticTypeRecipe>,
        depth: usize,
    ) -> R<()> {
        for requirement in requirements {
            self.pay(1, depth)?;
            match requirement {
                SemanticRequirementRecipe::Callable {
                    ty,
                    kind: CallableKind::FnOnce,
                    signature,
                } if signature.call_kind == CallKind::Direct
                    && !matches!(
                        ty,
                        SemanticTypeRecipe::OriginalType {
                            scope: SemanticScopeRef::Ancestor(0),
                            source: SemanticOriginalTypeSource::Result,
                        }
                    ) =>
                {
                    inputs.push(copy_recipe(ty, self.limits, self.left)?);
                }
                SemanticRequirementRecipe::ForAll { requirements, .. } => {
                    self.direct_consuming_inputs(requirements, inputs, depth + 1)?;
                }
                _ => {}
            }
        }
        Ok(())
    }
    pub(super) fn result_callable_requirements(
        &mut self,
        owner: SemanticDefinitionId,
        requirements: &[SemanticRequirementRecipe],
        universal: &mut Vec<u32>,
        depth: usize,
    ) -> R<()> {
        for requirement in requirements {
            self.pay(1, depth)?;
            match requirement {
                SemanticRequirementRecipe::Callable {
                    ty:
                        ty @ SemanticTypeRecipe::OriginalType {
                            scope: SemanticScopeRef::Ancestor(0),
                            source: SemanticOriginalTypeSource::Result,
                        },
                    kind,
                    signature,
                } => {
                    let actual = self.resolve(owner, ty, depth + 1)?;
                    self.callable_requirement_under(
                        owner,
                        &actual,
                        *kind,
                        signature,
                        universal,
                        depth + 1,
                    )?;
                }
                SemanticRequirementRecipe::ForAll {
                    lifetimes,
                    requirements,
                } => {
                    self.pay(universal.len().saturating_add(1), depth)?;
                    if *lifetimes != 0 {
                        universal.push(*lifetimes);
                    }
                    let result = self.result_callable_requirements(
                        owner,
                        requirements,
                        universal,
                        depth + 1,
                    );
                    if *lifetimes != 0 {
                        universal.pop();
                    }
                    result?;
                }
                _ => {}
            }
        }
        Ok(())
    }
    pub(super) fn input_requirement(
        &mut self,
        requirement: &SemanticRequirementRecipe,
        depth: usize,
    ) -> R<Option<SemanticRequirementRecipe>> {
        self.pay(1, depth)?;
        match requirement {
            SemanticRequirementRecipe::Callable {
                ty:
                    SemanticTypeRecipe::OriginalType {
                        scope: SemanticScopeRef::Ancestor(0),
                        source: SemanticOriginalTypeSource::Result,
                    },
                ..
            } => Ok(None),
            SemanticRequirementRecipe::ForAll {
                lifetimes,
                requirements,
            } => {
                let mut inputs = Vec::new();
                for requirement in requirements {
                    if let Some(input) = self.input_requirement(requirement, depth + 1)? {
                        self.pay(1, depth)?;
                        inputs.push(input);
                    }
                }
                Ok(
                    (!inputs.is_empty()).then_some(SemanticRequirementRecipe::ForAll {
                        lifetimes: *lifetimes,
                        requirements: inputs,
                    }),
                )
            }
            _ => {
                crate::dyn_metadata::semantic_operand_snapshot(
                    requirement,
                    self.limits,
                    self.left,
                    1,
                )?;
                Ok(Some(requirement.clone()))
            }
        }
    }
    pub(super) fn callable_premises<'p>(
        &mut self,
        requirement: &'p SemanticRequirementRecipe,
        value: &SemanticTypeRecipe,
        frames: &mut Vec<u32>,
        depth: usize,
    ) -> R<
        Vec<(
            &'p SemanticTypeRecipe,
            CallableKind,
            SemanticSignatureRecipe,
            BTreeMap<u32, call_regions::Role>,
        )>,
    > {
        self.pay(1, depth)?;
        let mut candidates = Vec::new();
        match requirement {
            SemanticRequirementRecipe::ForAll {
                lifetimes,
                requirements,
            } => {
                self.pay(
                    requirements
                        .len()
                        .saturating_add(frames.len())
                        .saturating_add(1),
                    depth,
                )?;
                if *lifetimes != 0 {
                    frames.push(*lifetimes);
                }
                let result: R<()> = (|| {
                    for requirement in requirements {
                        candidates.extend(self.callable_premises(
                            requirement,
                            value,
                            frames,
                            depth + 1,
                        )?);
                    }
                    Ok(())
                })();
                if *lifetimes != 0 {
                    frames.pop();
                }
                result?;
            }
            SemanticRequirementRecipe::Callable {
                ty,
                kind,
                signature,
            } if ty == value => {
                let mut origins = BTreeMap::new();
                let mut offset = signature.invocation_lifetimes;
                for (position, count) in frames.iter().enumerate() {
                    let frame_depth = u32::try_from(frames.len() - position - 1)
                        .map_err(|_| bad("original callable frame depth overflows"))?
                        .checked_add(u32::from(signature.bound_lifetimes != 0))
                        .ok_or_else(|| bad("original callable bound depth overflows"))?;
                    for index in 0..*count {
                        self.pay(1, depth)?;
                        origins.insert(
                            offset
                                .checked_add(index)
                                .ok_or_else(|| bad("original callable role ordinal overflows"))?,
                            call_regions::Role::Bound {
                                depth: frame_depth,
                                index,
                            },
                        );
                    }
                    offset = offset
                        .checked_add(*count)
                        .ok_or_else(|| bad("original callable role directory overflows"))?;
                }
                let signature = substitute::universal_callable_frames(
                    signature,
                    frames,
                    self.limits,
                    self.left,
                )?;
                candidates.push((ty, *kind, signature, origins));
            }
            _ => {}
        }
        Ok(candidates)
    }
    /// Contextual parameters and results are typing goals from this exact factory declaration,
    /// not a conclusion about the returned closure or its invocation effect.
    pub(super) fn contextual_closure_signature(
        &mut self, owner: SemanticDefinitionId, child: &SemanticDefinition,
        wanted: Option<&SemanticTypeRecipe>, depth: usize,
    ) -> R<Option<SemanticTypeRecipe>> {
        let Some(SemanticTypeRecipe::Opaque { definition, .. }) = wanted else { return Ok(None); };
        if *definition != owner || child.parent != Some(owner) { return Ok(None); }
        let SemanticDefinitionBody::FunctionWithBody { signature: child_signature, .. } = &child.body
        else { return Err(bad("contextual closure inputs have no immutable child signature")); };
        let declaration = self.definition(owner)?;
        self.pay(1, depth)?;
        let mut result_goal = None;
        let mut pending = vec![declaration.requirements.as_slice()];
        while let Some(requirements) = pending.pop() {
            for requirement in requirements {
                self.pay(1, depth)?;
                match requirement {
                    SemanticRequirementRecipe::ForAll { lifetimes: 0, requirements } => {
                        self.pay(1, depth)?;
                        pending.push(requirements);
                    }
                    SemanticRequirementRecipe::Callable { ty: SemanticTypeRecipe::OriginalType {
                        scope: SemanticScopeRef::Ancestor(0), source: SemanticOriginalTypeSource::Result,
                    }, signature, .. } if signature.bound_lifetimes == 0 && signature.invocation_lifetimes == 0 => {
                        if signature.parameters.len() != child_signature.parameters.len() {
                            return Err(bad("contextual closure changes its declared input arity"));
                        }
                        if matches!(child_signature.result, SemanticTypeRecipe::OriginalType {
                            scope: SemanticScopeRef::Ancestor(0), source: SemanticOriginalTypeSource::Result,
                        }) {
                            let goal = self.rehome(owner, child, &signature.result, depth + 1)?;
                            if let Some(previous) = &result_goal {
                                self.contexts.push(child.id);
                                let equal = self.same(previous, &goal);
                                self.contexts.pop();
                                equal?;
                            } else {
                                result_goal = Some(goal);
                            }
                        }
                        for (ordinal, (input, parameter)) in signature.parameters.iter()
                            .zip(&child_signature.parameters).enumerate()
                        {
                            self.pay(1, depth)?;
                            if !matches!(&parameter.ty, SemanticTypeRecipe::OriginalType {
                                scope: SemanticScopeRef::Ancestor(0),
                                source: SemanticOriginalTypeSource::Parameter(index),
                            } if *index as usize == ordinal) { continue; }
                            let expected = self.rehome(owner, child, &input.ty, depth + 1)?;
                            self.pay(self.proof.types.len().saturating_add(1).ilog2() as usize + 1, depth)?;
                            if let Some(old) = self.proof.types.get(&(child.id, Query::Parameter(ordinal as u32))) {
                                let old = copy_recipe(old, self.limits, self.left)?;
                                self.contexts.push(child.id);
                                let equal = self.same(&old, &expected);
                                self.contexts.pop();
                                equal?;
                            } else {
                                self.proof.proof_mut()?.types.insert((child.id, Query::Parameter(ordinal as u32)), expected);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(result_goal)
    }

    /// A checked scalar context in the original body can determine an
    /// unannotated parameter. Record the exact child parameter equation so
    /// every later use, including calls of the closure, checks the same type.
    pub(super) fn closure_scalar_input(
        &mut self,
        owner: SemanticDefinitionId,
        ordinal: u32,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(self.proof.types.len().saturating_add(1).ilog2() as usize + 1, depth)?;
        let key = (owner, Query::Parameter(ordinal));
        if let Some(ty) = self.proof.types.get(&key) {
            return copy_recipe(ty, self.limits, self.left);
        }
        let ty = wanted
            .filter(|ty| matches!(ty, SemanticTypeRecipe::Atom(scalar)
                if scalar.is_integer() || scalar.is_float() || *scalar == Type::Bool))
            .ok_or_else(|| pending(
                "original closure parameter requires a complete scalar source context",
            ))?;
        let equation = copy_recipe(ty, self.limits, self.left)?;
        self.pay(1, depth)?;
        self.proof.proof_mut()?.types.insert(key, equation);
        copy_recipe(ty, self.limits, self.left)
    }

    pub(super) fn callable_call(
        &mut self,
        owner: SemanticDefinitionId,
        site: SemanticSiteId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<Option<SemanticTypeRecipe>> {
        self.pay(1, depth)?;
        let CoreExprKind::Call { callee, arguments } = &expression.kind else {
            unreachable!()
        };
        let mut base = callee.as_ref();
        while let CoreExprKind::TypeApply { callee, .. }
        | CoreExprKind::GenericApply { callee, .. } = &base.kind
        {
            self.pay(1, depth)?;
            base = callee;
        }
        if self.declaration_path(base, variables, depth + 1)?.is_some() {
            return Ok(None);
        }
        // Keep this invocation's preparation fact in its control flow. The
        // fallback arm of place() fully evaluates an rvalue; Name/Member/Deref
        // only select a place and still require their own transfer checks.
        let mut selected_place = base;
        while let CoreExprKind::Group(inner) = &selected_place.kind {
            self.pay(1, depth)?;
            selected_place = inner;
        }
        if let CoreExprKind::Name(name) = &selected_place.kind {
            self.pay(name.len().saturating_add(variables.len().saturating_add(1).ilog2() as usize), depth)?;
            if let Some(binding) = variables.get(name)
                && let SemanticTypeRecipe::OriginalType {
                    scope: SemanticScopeRef::Ancestor(0),
                    source: SemanticOriginalTypeSource::Parameter(ordinal),
                } = &binding.ty
                && binding.origin_owner == owner
                && binding.origin == Query::Parameter(*ordinal)
                && !self.proof.types.contains_key(&(owner, Query::Parameter(*ordinal)))
            {
                return Err(zil_base::Diagnostic::new(
                    Phase::Types,
                    zil_base::ResultCategory::Unsupported,
                    code_context::MISSING,
                    "original callable parameter requires a complete source call context",
                    Some(base.span),
                ));
            }
        }
        let evaluated_rvalue = !matches!(selected_place.kind,
            CoreExprKind::Name(_) | CoreExprKind::Member { .. }
                | CoreExprKind::Unary { op: UnaryOp::Dereference, .. });
        let callee_type = match self.place(owner, base, variables, depth + 1) {
            Ok(ty) => ty,
            Err(diagnostic)
                if diagnostic.category == zil_base::ResultCategory::ResourceLimit
                    || diagnostic.category == zil_base::ResultCategory::Unsupported =>
            {
                return Err(diagnostic);
            }
            Err(_) => return Ok(None),
        };
        // A captured place can name an address through its creator. Determine
        // receiver permissions from that sealed lexical type and preserve the
        // original place recipe for emission and ownership transfer.
        let receiver_type = self.observation_type(owner, &callee_type, depth + 1)?;
        let (receiver, referent);
        let value = match &receiver_type {
            SemanticTypeRecipe::Address { mutable, inner, .. } => {
                receiver = Some(*mutable);
                referent = self.observation_type(owner, inner, depth + 1)?;
                &referent
            }
            value => {
                receiver = None;
                value
            }
        };
        let contextual_callee = match value {
            SemanticTypeRecipe::Closure { definition, .. } => Some(*definition),
            _ => None,
        };
        let operation = self.opaque_callable_operation(value, depth + 1)?;
        let (mut selected, mut invocation_origins) = if let Some((kind, signature, origins)) = operation {
            (Some((kind, signature)), origins)
        } else {
            (self.concrete_callable(value, depth + 1)?, BTreeMap::new())
        };
        if selected.is_none() {
            for requirement in self.original_requirements(owner, depth + 1)?.iter() {
                let candidates =
                    self.callable_premises(requirement, value, &mut Vec::new(), depth + 1)?;
                for (ty, kind, signature, origins) in candidates {
                    if ty == value {
                        if selected.is_some() {
                            return Err(pending(
                                "original callable has several complete roles and needs an explicit selection",
                            ));
                        }
                        selected = Some((kind, signature));
                        invocation_origins = origins;
                    }
                }
            }
        }
        let Some((kind, signature)) = selected else {
            return Ok(None);
        };
        if matches!(
            (receiver, kind),
            (Some(false), CallableKind::FnMut | CallableKind::FnOnce)
                | (Some(true), CallableKind::FnOnce)
        ) {
            return Err(bad(
                "original callable receiver does not grant its invocation mode",
            ));
        }
        if receiver.is_none() {
            match kind {
                CallableKind::FnOnce => {
                    self.consume_callable_place(owner, base, variables, &callee_type,
                        evaluated_rvalue, depth + 1)?;
                }
                CallableKind::FnMut => {
                    self.writable(owner, base, variables, depth + 1)?;
                }
                CallableKind::Fn | CallableKind::FunctionPointer => {}
            }
        } else if kind == CallableKind::FnMut {
            self.observe_exclusive_capture(owner, base, variables, depth + 1)?;
        }
        self.pay(arguments.len(), depth)?;
        let arguments: Vec<_> = arguments.iter().collect();
        let signature = if signature.bound_lifetimes != 0 {
            let offset = signature.invocation_lifetimes;
            for index in 0..signature.bound_lifetimes {
                self.pay(1, depth)?;
                invocation_origins.insert(
                    offset
                        .checked_add(index)
                        .ok_or_else(|| bad("original callable bound role directory overflows"))?,
                    call_regions::Role::Bound { depth: 0, index },
                );
            }
            substitute::universal_callable_own(&signature, self.limits, self.left)?
        } else {
            signature
        };
        let mut signature = self.inference_template(&signature, &[], false, depth + 1)?;
        let receiver_result = self.bind_receiver_call_result(
            owner, expression, base, &receiver_type, &mut signature, variables, depth + 1,
        )?;
        if receiver_result && (!matches!(kind, CallableKind::Fn | CallableKind::FnMut)
            || signature.call_kind != CallKind::Direct) {
            return Err(bad("consuming or thin code cannot borrow a callable receiver"));
        }
        // Solve complete inner types against the requested result, but leave
        // its outer receiver view to the directed return/call boundary. The
        // original call occurrence must not be equated with an input anchor.
        let receiver_wanted = if receiver_result {
            match wanted {
                Some(SemanticTypeRecipe::Address { mutable: false, inner, .. }) => {
                    let SemanticTypeRecipe::Address { lifetime, .. } = &signature.result else { unreachable!() };
                    crate::dyn_metadata::semantic_operand_snapshot(lifetime, self.limits, self.left, 1)?;
                    Some(SemanticTypeRecipe::Address { mutable: false, lifetime: lifetime.clone(),
                        inner: Box::new(copy_recipe(inner, self.limits, self.left)?) })
                }
                _ => None,
            }
        } else { None };
        let solver_wanted = if receiver_result { receiver_wanted.as_ref() } else { wanted };
        let (actuals, signature) = self.solve_signature_roles(
            owner,
            expression,
            callee,
            &arguments,
            &signature,
            &[],
            &[],
            variables,
            solver_wanted,
            true,
            &invocation_origins,
            &[],
            0,
            contextual_callee,
            depth + 1,
        )?;
        if !actuals.is_empty() {
            return Err(bad("abstract callable fabricates named generic parameters"));
        }
        let result = copy_recipe(&signature.result, self.limits, self.left)?;
        let mut operation = signature;
        if receiver_result {
            let SemanticTypeRecipe::Address { lifetime, .. } = &mut operation.result else {
                return Err(bad("callable receiver result changes its complete constructor"));
            };
            *lifetime = views::receiver_upper();
        }
        crate::dyn_metadata::semantic_operand_snapshot(&operation, self.limits, self.left, 1)?;
        self.pay(2, depth)?;
        self.proof.proof_mut()?.callable_calls.insert(
            (owner, site),
            CallableCall {
                callee: callee_type,
                callee_node: base.id,
                kind,
                signature: operation,
            },
        );
        self.proof
            .proof_mut()?
            .calls
            .insert((owner, site), Vec::new());
        Ok(Some(result))
    }
    pub(super) fn prove_trait(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        application: &SemanticTraitRecipe,
        depth: usize,
    ) -> R<()> {
        let ty = self.resolve(owner, ty, depth + 1)?;
        let application = self.resolve_application(owner, application, depth + 1)?;
        for requirement in self.original_requirements(owner, depth + 1)?.iter() {
            if matches!(&requirement, SemanticRequirementRecipe::Trait { ty: premise, application: assumed } if premise == &ty && assumed == &application)
            {
                return Ok(());
            }
        }
        if self
            .select_trait_provider(owner, &ty, &application, depth + 1)?
            .is_none()
        {
            return Err(bad(
                "original caller lacks a complete trait premise or provider",
            ));
        }
        Ok(())
    }
    pub(super) fn select_trait_provider(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        application: &SemanticTraitRecipe,
        depth: usize,
    ) -> R<Option<(SemanticDefinitionId, Vec<SemanticActualRecipe>)>> {
        self.pay(self.trait_active.len(), depth)?;
        if self
            .trait_active
            .iter()
            .any(|(active_type, active_application)| {
                active_type == ty && active_application == application
            })
        {
            return Err(bad(
                "original trait proof is cyclic without a caller premise",
            ));
        }
        crate::dyn_metadata::semantic_operand_snapshot(ty, self.limits, self.left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(application, self.limits, self.left, 1)?;
        self.pay(1, depth)?;
        self.trait_active.push((ty.clone(), application.clone()));
        self.pay(self.program.definitions.len(), depth)?;
        let mut selected = None;
        for provider in &self.program.definitions {
            let SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl {
                self_type,
                trait_application,
                ..
            }) = &provider.body
            else {
                continue;
            };
            if trait_application.declaration != application.declaration
                || trait_application.arguments.len() != application.arguments.len()
            {
                continue;
            }
            self.pay(provider.parameters.len(), depth)?;
            let mut solutions = vec![None; provider.parameters.len()];
            let mut implicit = BTreeMap::new();
            let mut invocation = BTreeMap::new();
            let candidate = (|| {
                self.pay(
                    trait_application
                        .arguments
                        .len()
                        .saturating_add(provider.parameters.len()),
                    depth,
                )?;
                let mut placeholders = Vec::new();
                for (ordinal, formal) in provider.parameters.iter().enumerate() {
                    let reference = SemanticFormalRef {
                        scope: SemanticScopeRef::Ancestor(u32::MAX),
                        ordinal: ordinal as u32,
                    };
                    placeholders.push(match formal {
                        SemanticFormal::Type { .. } => {
                            SemanticActualRecipe::Type(SemanticTypeRecipe::Formal(reference))
                        }
                        SemanticFormal::Lifetime => {
                            SemanticActualRecipe::Life(SemanticLifeRecipe::Formal(reference))
                        }
                        SemanticFormal::Const { carrier } => SemanticActualRecipe::Const {
                            carrier: copy_recipe(carrier, self.limits, self.left)?,
                            value: SemanticConstRecipe::Formal(reference),
                        },
                    });
                }
                let frames = [(0, placeholders.as_slice())];
                let empty = BTreeMap::new();
                let substitution = substitute::Substitution {
                    frames: if placeholders.is_empty() {
                        &[]
                    } else {
                        &frames
                    },
                    selves: &[],
                    environments: &[],
                    implicit: &empty,
                    invocation: &empty,
                    shift: 0,
                };
                let template_type =
                    substitute::ty(self_type, &substitution, self.limits, self.left)?;
                let mut template_arguments = Vec::new();
                for actual in &trait_application.arguments {
                    template_arguments.push(substitute::actual(
                        actual,
                        &substitution,
                        self.limits,
                        self.left,
                    )?);
                }
                self.unify(
                    &template_type,
                    ty,
                    &provider.parameters,
                    &mut solutions,
                    &mut implicit,
                    &mut invocation,
                    depth + 1,
                )?;
                for (template, actual) in template_arguments.iter().zip(&application.arguments) {
                    self.unify_actual(
                        template,
                        actual,
                        &provider.parameters,
                        &mut solutions,
                        &mut implicit,
                        &mut invocation,
                        depth + 1,
                    )?;
                }
                self.pay(solutions.len(), depth)?;
                let actuals = solutions
                    .into_iter()
                    .collect::<Option<Vec<_>>>()
                    .ok_or_else(|| {
                        pending("original trait provider has undetermined mixed formals")
                    })?;
                self.call_requirements(owner, provider, &actuals, depth + 1)?;
                Ok::<_, Diagnostic>(actuals)
            })();
            match candidate {
                Ok(actuals) => {
                    if selected.replace((provider.id, actuals)).is_some() {
                        return Err(bad(
                            "original trait application has ambiguous valid providers",
                        ));
                    }
                }
                Err(diagnostic)
                    if diagnostic.category == zil_base::ResultCategory::ResourceLimit
                        || diagnostic.category == zil_base::ResultCategory::Unsupported =>
                {
                    return Err(diagnostic);
                }
                Err(_) => {}
            }
        }
        self.trait_active.pop();
        Ok(selected)
    }
    pub(super) fn store_element(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<bool> {
        self.pay(1, depth)?;
        let ty = self.observation_type(owner, ty, depth + 1)?;
        for requirement in self.original_requirements(owner, depth + 1)?.iter() {
            let premise = match requirement {
                SemanticRequirementRecipe::Structural {
                    ty,
                    bound: DynStructuralBound::StoreElement,
                }
                | SemanticRequirementRecipe::Builtin {
                    ty,
                    trait_kind: SemanticBuiltinTrait::StoreElement,
                } => Some(ty),
                _ => None,
            };
            if let Some(premise) = premise {
                if self.premise_type_equal(owner, &ty, premise, depth + 1)? {
                    return Ok(true);
                }
            }
        }
        if let Some((callee, result)) = self.closed_opaque_result(&ty, depth + 1)? {
            return self.store_element(callee, &result, depth + 1);
        }
        if matches!(ty, SemanticTypeRecipe::Closure { .. }) {
            return Ok(self.original_zero_domain(owner, &ty, depth + 1)?
                && self.value_recipe(owner, &ty, depth + 1)?
                && self.sealed_capability(
                    owner,
                    &ty,
                    SemanticBuiltinTrait::SyncDrop,
                    depth + 1,
                )? == Some(true));
        }
        match &ty {
            SemanticTypeRecipe::Formal(reference) => {
                let SemanticScopeRef::Ancestor(hops) = reference.scope;
                let mut target = owner;
                for step in 0..hops {
                    self.pay(1, depth + step as usize)?;
                    target = self.definition(target)?.parent.ok_or_else(|| {
                        bad("original StoreElement formal escapes its lexical scope")
                    })?;
                }
                let original = SemanticTypeRecipe::Formal(SemanticFormalRef {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal: reference.ordinal,
                });
                for requirement in self.original_requirements(target, depth + 1)?.iter() {
                    if matches!(&requirement, SemanticRequirementRecipe::Structural { ty, bound: DynStructuralBound::StoreElement } | SemanticRequirementRecipe::Builtin { ty, trait_kind: SemanticBuiltinTrait::StoreElement } if ty == &original)
                    {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            SemanticTypeRecipe::Atom(ty) => Ok(ty.is_b0_scalar() && *ty != Type::Unit),
            SemanticTypeRecipe::Exact(exact) => {
                crate::inference::charge_type_snapshot(
                    &exact.ty,
                    Span::default(),
                    self.limits,
                    self.left,
                    1,
                )?;
                Ok(exact.life.binder_count == 0
                    && exact.classes.is_empty()
                    && crate::dynamic_store::supported_element(&exact.ty))
            }
            SemanticTypeRecipe::Managed(payload) => {
                if let Some(accepted) = self.resource_store_element(owner, payload, depth + 1)? {
                    return Ok(accepted);
                }
                Ok(matches!(
                    payload.as_ref(),
                    SemanticTypeRecipe::Atom(Type::ByteBuffer)
                ))
            }
            SemanticTypeRecipe::Array { element, length } => {
                let nonzero = !matches!(
                    length.as_ref(),
                    SemanticConstRecipe::Value(
                        Constant::Usize(0) | Constant::U64(0) | Constant::I32(0) | Constant::I64(0)
                    )
                );
                Ok(nonzero && self.store_element(owner, element, depth + 1)?)
            }
            SemanticTypeRecipe::Nominal {
                definition,
                arguments,
            } => {
                let definition = self.definition(*definition)?;
                let SemanticDefinitionBody::Nominal {
                    kind,
                    body: SemanticNominalBody::Product { fields },
                    ..
                } = &definition.body
                else {
                    return Ok(false);
                };
                if fields.is_empty()
                    || !matches!(kind, LifeNominalKind::Record | LifeNominalKind::Package)
                {
                    return Ok(false);
                }
                for field in fields {
                    let field = self.instantiate_field(
                        owner,
                        definition.id,
                        &field.ty,
                        arguments,
                        depth + 1,
                    )?;
                    if !self.store_element(owner, &field, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    pub(super) fn equal_types(
        &mut self,
        owner: SemanticDefinitionId,
        a: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<Vec<SemanticTypeRecipe>> {
        let requirements = self.original_requirements(owner, depth + 1)?;
        self.pay(
            requirements.len().saturating_mul(4).saturating_add(4),
            depth,
        )?;
        let mut frontier = vec![a];
        let mut seen: Vec<&SemanticTypeRecipe> = Vec::new();
        while let Some(current) = frontier.pop() {
            let mut duplicate = false;
            for old in &seen {
                crate::dyn_metadata::semantic_operand_snapshot(current, self.limits, self.left, 1)?;
                crate::dyn_metadata::semantic_operand_snapshot(*old, self.limits, self.left, 1)?;
                duplicate |= *old == current;
            }
            if duplicate {
                continue;
            }
            seen.push(current);
            for requirement in requirements.iter() {
                if let SemanticRequirementRecipe::TypeEqual { left, right } = requirement {
                    crate::dyn_metadata::semantic_operand_snapshot(
                        current,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    crate::dyn_metadata::semantic_operand_snapshot(
                        left,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    crate::dyn_metadata::semantic_operand_snapshot(
                        right,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    if current == left {
                        frontier.push(right);
                    }
                    if current == right {
                        frontier.push(left);
                    }
                }
            }
        }
        let mut result = Vec::new();
        self.pay(seen.len(), depth)?;
        for current in seen {
            if current != a {
                result.push(copy_recipe(current, self.limits, self.left)?);
            }
        }
        Ok(result)
    }
    pub(super) fn premise_type_equal(
        &mut self,
        owner: SemanticDefinitionId,
        a: &SemanticTypeRecipe,
        b: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<bool> {
        if a == b {
            return Ok(true);
        }
        for equivalent in self.equal_types(owner, a, depth + 1)? {
            crate::dyn_metadata::semantic_operand_snapshot(b, self.limits, self.left, 1)?;
            if &equivalent == b {
                return Ok(true);
            }
        }
        Ok(false)
    }
    /// Interpret a root capture-class query through its checked original
    /// whole-Type substitution. The result is the creator's real class in the
    /// current lexical domain, not a class chosen from a same-Type occurrence.
    fn capture_life_source(
        &mut self, owner: SemanticDefinitionId, life: &SemanticLifeRecipe, depth: usize,
    ) -> R<SemanticLifeRecipe> {
        self.pay(1, depth)?;
        crate::dyn_metadata::semantic_operand_snapshot(life, self.limits, self.left, 1)?;
        let SemanticLifeRecipe::OuterOccurrence {
            operand: SemanticRecipeOperand::Environment { scope: SemanticScopeRef::Ancestor(hops), ordinal },
            path, mode,
        } = life else { return Ok(life.clone()); };
        if !path.is_empty() { return Ok(life.clone()); }
        let mut domain = owner;
        for index in 0..*hops {
            self.pay(1, depth + index as usize)?;
            domain = self.definition(domain)?.parent.ok_or_else(|| bad("capture class query leaves its real lexical ancestry"))?;
        }
        self.pay(self.proof.environments.len().saturating_add(1).ilog2() as usize + 1, depth)?;
        let (parent, source) = self.proof.environments.get(&(domain, *ordinal))
            .ok_or_else(|| bad("capture class query lacks its checked whole-Type substitution"))?;
        let parent = *parent;
        let source = copy_recipe(source, self.limits, self.left)?;
        if self.definition(domain)?.parent != Some(parent) { return Err(bad("capture class query changes its true original creator")); }
        let source = self.observation_type(parent, &source, depth + 1)?;
        let source = self.rehome_role_type(parent, owner, &source, depth + 1)?;
        let original = match (&source, mode) {
            (SemanticTypeRecipe::Address { mutable: false, lifetime, .. }, StoredBorrowMode::SharedAddress)
            | (SemanticTypeRecipe::Address { mutable: true, lifetime, .. }, StoredBorrowMode::ExclusiveAddress)
            | (SemanticTypeRecipe::Desc { lifetime, .. }, StoredBorrowMode::WritableDescription) => lifetime,
            _ => return Err(bad("capture class query changes its complete original outer mode")),
        };
        let SemanticOuterLifeRecipe::Class(source) = original else {
            return Err(bad("capture class query replaces a real Static endpoint"));
        };
        self.capture_life_source(owner, source, depth + 1)
    }
    pub(super) fn same_life(&mut self, a: &SemanticLifeRecipe, b: &SemanticLifeRecipe) -> R<bool> {
        crate::dyn_metadata::semantic_operand_snapshot(a, self.limits, self.left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(b, self.limits, self.left, 1)?;
        if a == b {
            return Ok(true);
        }
        let Some(owner) = self.contexts.last().copied() else {
            return Ok(false);
        };
        let a = self.capture_life_source(owner, a, 0)?;
        let b = self.capture_life_source(owner, b, 0)?;
        if a == b { return Ok(true); }
        let requirements = self.original_requirements(owner, 0)?;
        self.pay(requirements.len().saturating_mul(4).saturating_add(4), 0)?;
        let mut frontier = vec![&a];
        let mut seen: Vec<&SemanticLifeRecipe> = Vec::new();
        while let Some(current) = frontier.pop() {
            crate::dyn_metadata::semantic_operand_snapshot(current, self.limits, self.left, 1)?;
            if current == &b {
                return Ok(true);
            }
            let mut duplicate = false;
            for old in &seen {
                crate::dyn_metadata::semantic_operand_snapshot(*old, self.limits, self.left, 1)?;
                duplicate |= *old == current;
            }
            if duplicate {
                continue;
            }
            seen.push(current);
            for requirement in requirements.iter() {
                if let SemanticRequirementRecipe::LifeEqual { left, right } = requirement {
                    crate::dyn_metadata::semantic_operand_snapshot(
                        left,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    crate::dyn_metadata::semantic_operand_snapshot(
                        right,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    if current == left {
                        frontier.push(right);
                    }
                    if current == right {
                        frontier.push(left);
                    }
                }
            }
        }
        Ok(false)
    }
    pub(super) fn same_const(
        &mut self,
        carrier: &SemanticTypeRecipe,
        a: &SemanticConstRecipe,
        b: &SemanticConstRecipe,
    ) -> R<bool> {
        crate::dyn_metadata::semantic_operand_snapshot(a, self.limits, self.left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(b, self.limits, self.left, 1)?;
        if a == b {
            return Ok(true);
        }
        let Some(owner) = self.contexts.last().copied() else {
            return Ok(false);
        };
        let requirements = self.original_requirements(owner, 0)?;
        self.pay(requirements.len().saturating_mul(4).saturating_add(4), 0)?;
        let mut frontier = vec![a];
        let mut seen: Vec<&SemanticConstRecipe> = Vec::new();
        while let Some(current) = frontier.pop() {
            crate::dyn_metadata::semantic_operand_snapshot(current, self.limits, self.left, 1)?;
            if current == b {
                return Ok(true);
            }
            let mut duplicate = false;
            for old in &seen {
                crate::dyn_metadata::semantic_operand_snapshot(*old, self.limits, self.left, 1)?;
                duplicate |= *old == current;
            }
            if duplicate {
                continue;
            }
            seen.push(current);
            for requirement in requirements.iter() {
                if let SemanticRequirementRecipe::ConstEqual {
                    carrier: required,
                    left,
                    right,
                } = requirement
                {
                    crate::dyn_metadata::semantic_operand_snapshot(
                        required,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    crate::dyn_metadata::semantic_operand_snapshot(
                        left,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    crate::dyn_metadata::semantic_operand_snapshot(
                        right,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    if required != carrier {
                        continue;
                    }
                    if current == left {
                        frontier.push(right);
                    }
                    if current == right {
                        frontier.push(left);
                    }
                }
            }
        }
        Ok(false)
    }
}
