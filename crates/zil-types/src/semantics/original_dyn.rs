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
            None => self.borrow_life(owner, object.id)?,
        };
        let receiver_type = SemanticTypeRecipe::Address {
            mutable,
            lifetime,
            inner: Box::new(copy_recipe(&borrowed, self.limits, self.left)?),
        };
        self.pay(definition.parameters.len(), depth)?;
        let mut solutions = vec![None; definition.parameters.len()];
        let mut implicit = BTreeMap::new();
        let mut invocation = BTreeMap::new();
        self.unify(
            &receiver.ty,
            &receiver_type,
            &definition.parameters,
            &mut solutions,
            &mut implicit,
            &mut invocation,
            depth + 1,
        )?;
        if solutions.iter().any(Option::is_some) {
            return Err(bad(
                "dynamic erased receiver unexpectedly determines a method generic",
            ));
        }
        let roles = substitute::Substitution {
            frames: &[],
            selves: &[],
            environments: &[],
            implicit: &implicit,
            invocation: &invocation,
            shift: 0,
        };
        template = substitute::signature_fresh(&template, &roles, self.limits, self.left)?;
        let receiver = template.parameters.remove(0);
        self.same(&receiver.ty, &receiver_type)?;
        self.pay(
            arguments.len().saturating_add(definition.parameters.len()),
            depth,
        )?;
        let operands = arguments.iter().collect::<Vec<_>>();
        let holes = vec![SemanticActualRecipe::Infer; definition.parameters.len()];
        let (actuals, solved) = self.solve_signature(
            owner,
            callee,
            &operands,
            &template,
            &definition.parameters,
            &holes,
            variables,
            wanted,
            true,
            depth + 1,
        )?;
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
        let mut parameters = vec![copy_recipe(&receiver.ty, self.limits, self.left)?];
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
        self.proof.calls.insert((owner, site.id), actuals);
        // The source observation still denotes the owner. The independently
        // emitted receiver borrow has its own outer occurrence and is checked
        // against parameters[0] at the actual slot boundary.
        Ok(Some(solved.result))
    }
}
