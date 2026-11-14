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
                    ))
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
    ) -> R<Vec<SemanticRequirementRecipe>> {
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
            // Closure free semantic operands cross their explicit environment
            // boundary, never a guessed implicit ancestor substitution.
            if definition.kind == SemanticDefinitionKind::Closure {
                break;
            }
            target = definition.parent;
            hops = hops
                .checked_add(1)
                .ok_or_else(|| bad("original premise ancestry overflows"))?;
        }
        Ok(requirements)
    }
    fn trait_application(
        &mut self,
        owner: SemanticDefinitionId,
        self_type: &SemanticTypeRecipe,
        declaration: LifeDeclarationId,
        depth: usize,
    ) -> R<SemanticTraitRecipe> {
        let requirements = self.original_requirements(owner, depth + 1)?;
        let mut selected = None;
        for requirement in requirements {
            if let SemanticRequirementRecipe::Trait { ty, application } = requirement {
                if &ty == self_type && application.declaration == declaration {
                    if selected.as_ref().is_some_and(|old| old != &application) {
                        return Err(pending("original method has several distinct trait applications and needs an explicit qualification"));
                    }
                    selected = Some(application);
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
                    match self.solve_parent(owner, parent, &self_type, depth + 1) {
                        Ok(_) => {
                            if target.replace(candidate).is_some() {
                                return Err(bad(
                                    "original receiver selects ambiguous method declarations",
                                ));
                            }
                        }
                        Err(diagnostic)
                            if diagnostic.category == zil_base::ResultCategory::ResourceLimit
                                || diagnostic.category == zil_base::ResultCategory::Unsupported =>
                        {
                            return Err(diagnostic)
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
            SemanticDefinitionBody::Scope(
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
        let signature =
            self.inference_template(signature, &definition.parameters, true, depth + 1)?;
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
        let holes = vec![SemanticActualRecipe::Infer; definition.parameters.len()];
        let (actuals, solved) = self.solve_signature(
            owner,
            callee,
            &operands,
            &signature,
            &definition.parameters,
            &holes,
            variables,
            wanted,
            true,
            depth + 1,
        )?;
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
            .scope_calls
            .insert((owner, site, parent.id), parent_actuals);
        self.proof.call_targets.insert((owner, site), target);
        self.proof.calls.insert((owner, site), actuals);
        crate::dyn_metadata::semantic_operand_snapshot(&self_type, self.limits, self.left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(&solved, self.limits, self.left, 1)?;
        self.proof.method_calls.insert(
            (owner, site),
            MethodCall {
                declaration: target,
                parent: parent.id,
                self_type,
                application,
                signature: solved.clone(),
            },
        );
        Ok(Some(solved.result))
    }
    fn concrete_callable(
        &mut self,
        value: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<Option<(CallableKind, SemanticSignatureRecipe)>> {
        self.pay(1, depth)?;
        match value {
            SemanticTypeRecipe::Opaque {
                definition,
                arguments,
                hidden,
            } => {
                let declaration = self.definition(*definition)?;
                if declaration.parameters.len() != arguments.len() {
                    return Err(bad(
                        "original opaque callable omits full explicit arguments",
                    ));
                }
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
                // Enter the opaque definition's sealed body domain deliberately;
                // ordinary substitution never traverses this boundary itself.
                let hidden = substitute::ty(hidden, &substitution, self.limits, self.left)?;
                self.concrete_callable(&hidden, depth + 1)
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
                signature.result = self.resolve(*definition, &signature.result, depth + 1)?;
                let frames = [(0, arguments.as_slice())];
                let environments = [(0, environment.as_slice())];
                let empty = BTreeMap::new();
                let substitution = substitute::Substitution {
                    frames: &frames,
                    selves: &[],
                    environments: &environments,
                    implicit: &empty,
                    invocation: &empty,
                    shift: 0,
                };
                Ok(Some((
                    kind,
                    substitute::signature(&signature, &substitution, self.limits, self.left)?,
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
        self.pay(actual.parameters.len(), depth)?;
        if actual.parameters.len() != wanted.parameters.len()
            || actual.bound_lifetimes != wanted.bound_lifetimes
            || actual.invocation_lifetimes != wanted.invocation_lifetimes
            || actual.call_kind != wanted.call_kind
            || actual.result_mode != wanted.result_mode
        {
            self.pay(384, depth)?;
            let mut diagnostic = bad("original callable changes its complete scoped call scheme");
            diagnostic.notes.push(format!(
                "original callable scheme: actual P={} Bound={} Invocation={} ABI={:?} resultMode={:?}; wanted P={} Bound={} Invocation={} ABI={:?} resultMode={:?}",
                actual.parameters.len(), actual.bound_lifetimes, actual.invocation_lifetimes, actual.call_kind, actual.result_mode,
                wanted.parameters.len(), wanted.bound_lifetimes, wanted.invocation_lifetimes, wanted.call_kind, wanted.result_mode
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
        let value = self.observation_type(owner, ty, depth + 1)?;
        if let Some((kind, actual)) = self.concrete_callable(&value, depth + 1)? {
            if !compatible(kind, required) {
                return Err(bad(
                    "original concrete callable lacks the required invocation kind",
                ));
            }
            return self.compare_callable(&actual, wanted, depth + 1);
        }
        for requirement in self.original_requirements(owner, depth + 1)? {
            if let SemanticRequirementRecipe::Callable {
                ty: premise,
                kind,
                signature,
            } = requirement
            {
                if premise == value && compatible(kind, required) {
                    return self.compare_callable(&signature, wanted, depth + 1);
                }
            }
        }
        Err(bad(
            "original caller lacks a complete callable role premise",
        ))
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
        let callee_type = match self.place(owner, base, variables, depth + 1) {
            Ok(ty) => ty,
            Err(diagnostic)
                if diagnostic.category == zil_base::ResultCategory::ResourceLimit
                    || diagnostic.category == zil_base::ResultCategory::Unsupported =>
            {
                return Err(diagnostic)
            }
            Err(_) => return Ok(None),
        };
        let (receiver, value) = match &callee_type {
            SemanticTypeRecipe::Address { mutable, inner, .. } => (Some(*mutable), inner.as_ref()),
            _ => (None, &callee_type),
        };
        let value = self.observation_type(owner, value, depth + 1)?;
        let mut selected = self.concrete_callable(&value, depth + 1)?;
        if selected.is_none() {
            for requirement in self.original_requirements(owner, depth + 1)? {
                if let SemanticRequirementRecipe::Callable {
                    ty,
                    kind,
                    signature,
                } = requirement
                {
                    if ty == value {
                        if selected.is_some() {
                            return Err(pending("original callable has several complete roles and needs an explicit selection"));
                        }
                        selected = Some((kind, *signature));
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
                    self.expr(owner, base, variables, None, depth + 1)?;
                }
                CallableKind::FnMut => {
                    self.writable(owner, base, variables, depth + 1)?;
                }
                CallableKind::Fn | CallableKind::FunctionPointer => {}
            }
        }
        self.pay(arguments.len(), depth)?;
        let arguments: Vec<_> = arguments.iter().collect();
        let signature = self.inference_template(&signature, &[], false, depth + 1)?;
        let (actuals, signature) = self.solve_signature(
            owner,
            callee,
            &arguments,
            &signature,
            &[],
            &[],
            variables,
            wanted,
            true,
            depth + 1,
        )?;
        if !actuals.is_empty() {
            return Err(bad("abstract callable fabricates named generic parameters"));
        }
        crate::dyn_metadata::semantic_operand_snapshot(&signature, self.limits, self.left, 1)?;
        self.pay(2, depth)?;
        self.proof.callable_calls.insert(
            (owner, site),
            CallableCall {
                callee: callee_type,
                callee_node: base.id,
                kind,
                signature: signature.clone(),
            },
        );
        self.proof.calls.insert((owner, site), Vec::new());
        Ok(Some(signature.result))
    }
    pub(super) fn prove_trait(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        application: &SemanticTraitRecipe,
        depth: usize,
    ) -> R<()> {
        self.pay(self.trait_active.len(), depth)?;
        for requirement in self.original_requirements(owner, depth + 1)? {
            if matches!(&requirement, SemanticRequirementRecipe::Trait { ty: premise, application: assumed } if premise == ty && assumed == application)
            {
                return Ok(());
            }
        }
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
                    return Err(diagnostic)
                }
                Err(_) => {}
            }
        }
        self.trait_active.pop();
        if selected.is_none() {
            return Err(bad(
                "original caller lacks a complete trait premise or provider",
            ));
        }
        Ok(())
    }
    pub(super) fn store_element(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<bool> {
        self.pay(1, depth)?;
        let ty = self.observation_type(owner, ty, depth + 1)?;
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
                for requirement in self.original_requirements(target, depth + 1)? {
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
                    let frames = [(0, arguments.as_slice())];
                    let empty = BTreeMap::new();
                    let substitution = substitute::Substitution {
                        frames: &frames,
                        selves: &[],
                        environments: &[],
                        implicit: &empty,
                        invocation: &empty,
                        shift: 0,
                    };
                    let field = substitute::ty(&field.ty, &substitution, self.limits, self.left)?;
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
            for requirement in &requirements {
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
    pub(super) fn same_life(&mut self, a: &SemanticLifeRecipe, b: &SemanticLifeRecipe) -> R<bool> {
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
        let mut seen: Vec<&SemanticLifeRecipe> = Vec::new();
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
            for requirement in &requirements {
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
            for requirement in &requirements {
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
