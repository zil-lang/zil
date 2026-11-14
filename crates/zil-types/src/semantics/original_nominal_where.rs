//! Nominal obligations in a real caller Gamma, borrowing the same immutable
//! whole-program proof. Runtime type equality only locates an application.
use super::*;

pub(in crate::semantics) struct NominalRequest {
    pub caller: SemanticDefinitionId,
    pub recipe: SemanticTypeRecipe,
}
pub(in crate::semantics) struct NominalWhere<'a> {
    program: &'a SemanticModule,
    proof: &'a Proof,
}
pub(super) fn borrowed_check<'a, 'b>(
    program: &'a SemanticModule,
    proof: &'a Proof,
    owner: SemanticDefinitionId,
    limits: &'b Limits,
    left: &'b mut usize,
) -> R<Check<'a, 'b>> {
    if !proof.complete.contains(&owner) {
        return Err(bad("original where caller has no complete original body"));
    }
    *left = left.checked_sub(1).ok_or_else(|| {
        Diagnostic::limit(Phase::Types, "original where context work exceeded", None)
    })?;
    Ok(Check {
        program,
        limits,
        left,
        proof: ProofAccess::Borrowed(proof),
        active: BTreeSet::new(),
        captures: BTreeMap::new(),
        loops: Vec::new(),
        trait_active: Vec::new(),
        contexts: vec![owner],
        predicate_active: Vec::new(),
        stage_depth: 0,
        cleanup_depth: 0,
        requested_snapshot: None,
        escape_active: Vec::new(),
    })
}
impl<'a> NominalWhere<'a> {
    pub(in crate::semantics) fn new(program: &'a SemanticModule, proof: &'a Proof) -> Self {
        Self { program, proof }
    }
    pub(in crate::semantics) fn requests(
        &self,
        definition: SemanticDefinitionId,
        limits: &Limits,
        left: &mut usize,
    ) -> R<Vec<NominalRequest>> {
        *left = left
            .checked_sub(self.proof.types.len().saturating_add(1))
            .ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "original nominal request lookup exceeded",
                    None,
                )
            })?;
        let mut result: Vec<NominalRequest> = Vec::new();
        for ((caller, _), value) in &self.proof.types {
            if !self.proof.complete.contains(caller) {
                continue;
            }
            for recipe in substitute::nominal_types(value, definition, limits, left)? {
                let mut duplicate = false;
                for previous in &result {
                    if previous.caller != *caller {
                        continue;
                    }
                    crate::dyn_metadata::semantic_operand_snapshot(
                        &previous.recipe,
                        limits,
                        left,
                        1,
                    )?;
                    crate::dyn_metadata::semantic_operand_snapshot(&recipe, limits, left, 1)?;
                    duplicate |= previous.recipe == recipe;
                }
                if !duplicate {
                    *left = left.checked_sub(1).ok_or_else(|| {
                        Diagnostic::limit(
                            Phase::Types,
                            "original nominal request allocation exceeded",
                            None,
                        )
                    })?;
                    result.push(NominalRequest {
                        caller: *caller,
                        recipe,
                    });
                }
            }
        }
        Ok(result)
    }
    pub(in crate::semantics) fn prove(
        &self,
        caller: SemanticDefinitionId,
        recipe: &SemanticTypeRecipe,
        limits: &Limits,
        left: &mut usize,
    ) -> R<()> {
        let SemanticTypeRecipe::Nominal {
            definition,
            arguments,
        } = recipe
        else {
            return Err(bad(
                "original nominal obligation changes its source recipe kind",
            ));
        };
        let mut check = borrowed_check(self.program, self.proof, caller, limits, left)?;
        let declaration = check.definition(*definition)?;
        if !matches!(declaration.body, SemanticDefinitionBody::Nominal { .. }) {
            return Err(bad(
                "original nominal obligation has no true nominal declaration",
            ));
        }
        let result = (|| {
            for requirement in
                declaration
                    .requirements
                    .iter()
                    .chain(
                        declaration
                            .parameters
                            .iter()
                            .flat_map(|formal| match formal {
                                SemanticFormal::Type { requirements } => requirements.as_slice(),
                                _ => &[],
                            }),
                    )
            {
                let mapped = super::super::instantiate::requirement(
                    self.program,
                    self.proof,
                    requirement,
                    super::super::instantiate::DomainMapping {
                        source: *definition,
                        use_definition: caller,
                        arguments,
                        parent_call: None,
                    },
                    limits,
                    check.left,
                )?;
                check.satisfy_requirement(caller, &mapped, 0)?;
            }
            Ok(())
        })();
        if !matches!(check.proof, ProofAccess::Borrowed(_)) {
            return Err(bad(
                "read-only original where unexpectedly changed its proof context",
            ));
        }
        result
    }
}

impl Proof {
    /// A definition's own quantified premises are assumptions only at its
    /// entry. The complete original check has already checked their scope,
    /// kinds and uses; this does not install them in any caller's Gamma.
    pub(in crate::semantics) fn where_entry(
        &self,
        program: &SemanticModule,
        owner: SemanticDefinitionId,
        limits: &Limits,
        left: &mut usize,
    ) -> R<()> {
        let mut check = borrowed_check(program, self, owner, limits, left)?;
        let original = check.definition(owner)?;
        if !matches!(
            original.body,
            SemanticDefinitionBody::FunctionWithBody { .. }
        ) {
            return Err(bad(
                "where entry does not select a checked original function",
            ));
        }
        // Include inherited lexical and associated premises using the same
        // declaration-domain operation used during original body checking.
        let _ = check.original_requirements(owner, 0)?;
        Ok(())
    }

    /// Recheck all callee premises in the caller's original Gamma at the exact
    /// original operation. The returned mixed vector must still be compared
    /// against every actual world argument by the executable consumer.
    pub(in crate::semantics) fn where_call(
        &self,
        program: &SemanticModule,
        caller: SemanticDefinitionId,
        site: SemanticSiteId,
        target: SemanticDefinitionId,
        limits: &Limits,
        left: &mut usize,
    ) -> R<Vec<SemanticActualRecipe>> {
        let mut check = borrowed_check(program, self, caller, limits, left)?;
        if !self.complete.contains(&target) {
            return Err(bad("where call target lacks complete original typing"));
        }
        check.pay(
            self.calls
                .len()
                .saturating_add(self.call_targets.len())
                .saturating_add(2)
                .ilog2() as usize
                + 1,
            0,
        )?;
        let key = (caller, site);
        if self.call_targets.get(&key) != Some(&target) {
            return Err(bad("where call changes its exact original target"));
        }
        let arguments = self
            .calls
            .get(&key)
            .ok_or_else(|| bad("where call lacks its complete original mixed actuals"))?;
        let original = check.definition(target)?;
        if original.parameters.len() != arguments.len() {
            return Err(bad("where call omits a complete mixed argument position"));
        }
        for (formal, actual) in original.parameters.iter().zip(arguments) {
            check.pay(1, 0)?;
            if !matches!(
                (formal, actual),
                (SemanticFormal::Type { .. }, SemanticActualRecipe::Type(_))
                    | (SemanticFormal::Lifetime, SemanticActualRecipe::Life(_))
                    | (
                        SemanticFormal::Const { .. },
                        SemanticActualRecipe::Const { .. }
                    )
            ) {
                return Err(bad("where call changes an original mixed argument kind"));
            }
        }
        for requirement in original
            .requirements
            .iter()
            .chain(original.parameters.iter().flat_map(|formal| match formal {
                SemanticFormal::Type { requirements } => requirements.as_slice(),
                _ => &[],
            }))
        {
            if matches!(
                requirement,
                SemanticRequirementRecipe::Callable {
                    ty: SemanticTypeRecipe::OriginalType {
                        scope: SemanticScopeRef::Ancestor(0),
                        source: SemanticOriginalTypeSource::Result,
                    },
                    ..
                }
            ) {
                // This is an exit obligation of the independently checked
                // callee body, never a caller assumption about an arbitrary F.
                continue;
            }
            let mapped = super::super::instantiate::requirement(
                program,
                self,
                requirement,
                super::super::instantiate::DomainMapping {
                    source: target,
                    use_definition: caller,
                    arguments,
                    parent_call: Some(site),
                },
                limits,
                check.left,
            )?;
            check.satisfy_requirement(caller, &mapped, 0)?;
        }
        for argument in arguments {
            crate::dyn_metadata::semantic_operand_snapshot(argument, limits, check.left, 1)?;
        }
        Ok(arguments.clone())
    }

    /// Reduce a complete recipe in its explicitly selected original Gamma.
    /// This never installs specialization equalities or new body facts.
    pub(in crate::semantics) fn resolve_source_type(
        &self,
        program: &SemanticModule,
        owner: SemanticDefinitionId,
        recipe: &SemanticTypeRecipe,
        limits: &Limits,
        left: &mut usize,
    ) -> R<std::rc::Rc<SemanticTypeRecipe>> {
        let mut check = borrowed_check(program, self, owner, limits, left)?;
        crate::dyn_metadata::semantic_operand_snapshot(recipe, limits, check.left, 1)?;
        let query = source_cache::Query::Type(recipe.clone());
        let key = if self.pure_queries_ready {
            let (key, cached) = self
                .pure_queries
                .lookup(owner, &query, limits, check.left)?;
            if let Some(value) = cached {
                return match value {
                    source_cache::Value::Type(value) => Ok(value),
                    _ => Err(bad(
                        "original pure source cache changes its exact query kind",
                    )),
                };
            }
            Some(key)
        } else {
            None
        };
        let result = check.resolve(owner, recipe, 0)?;
        check.pay(2, 0)?;
        let result = std::rc::Rc::new(result);
        if let Some(key) = key {
            self.pure_queries.insert(
                key,
                query,
                source_cache::Value::Type(result.clone()),
                limits,
                check.left,
            )?;
        }
        Ok(result)
    }
    /// Specialize a universally checked method provider in a real original
    /// source Gamma. The runtime call selects data; it supplies no premise.
    pub(in crate::semantics) fn specialized_parent(
        &self,
        program: &SemanticModule,
        source_owner: SemanticDefinitionId,
        caller: SemanticDefinitionId,
        call: SemanticSiteId,
        provider: SemanticDefinitionId,
        holder: SemanticDefinitionId,
        receiver: &SemanticTypeRecipe,
        application: Option<&SemanticTraitRecipe>,
        limits: &Limits,
        left: &mut usize,
    ) -> R<std::rc::Rc<Vec<SemanticActualRecipe>>> {
        let mut check = borrowed_check(program, self, source_owner, limits, left)?;
        check.pay(3, 0)?;
        let method = self
            .method_calls
            .get(&(caller, call))
            .ok_or_else(|| bad("parent specialization lacks its original method call"))?;
        if !self.complete.contains(&caller)
            || !self.complete.contains(&provider)
            || self.call_targets.get(&(caller, call)) != Some(&method.declaration)
            || (provider != method.declaration
                && self.providers.get(&provider) != Some(&method.declaration))
            || check.definition(provider)?.parent != Some(holder)
        {
            return Err(bad(
                "parent specialization changes its complete original provider boundary",
            ));
        }
        let parent = check.definition(holder)?;
        crate::dyn_metadata::semantic_operand_snapshot(receiver, limits, check.left, 1)?;
        if let Some(application) = application {
            crate::dyn_metadata::semantic_operand_snapshot(application, limits, check.left, 1)?;
        }
        check.pay(8, 0)?;
        let query = source_cache::Query::Parent {
            caller,
            call,
            provider,
            holder,
            receiver: receiver.clone(),
            application: application.cloned(),
        };
        let key = if self.pure_queries_ready {
            let (key, cached) =
                self.pure_queries
                    .lookup(source_owner, &query, limits, check.left)?;
            if let Some(value) = cached {
                return match value {
                    source_cache::Value::Parent(value) => Ok(value),
                    _ => Err(bad("original parent cache changes its exact query kind")),
                };
            }
            Some(key)
        } else {
            None
        };
        let actuals = if let Some(application) = application {
            if method.application.as_ref().map(|row| row.declaration)
                != Some(application.declaration)
            {
                return Err(bad(
                    "parent specialization changes the original trait declaration",
                ));
            }
            let (selected, actuals) = check
                .select_trait_provider(source_owner, receiver, application, 0)?
                .ok_or_else(|| {
                    bad("parent specialization has no complete original trait provider")
                })?;
            if selected != holder {
                return Err(bad(
                    "actual method parent differs from the unique original trait provider",
                ));
            }
            actuals
        } else {
            if method.application.is_some() || method.declaration != provider {
                return Err(bad(
                    "parent specialization loses its original trait application",
                ));
            }
            check.solve_parent(source_owner, parent, receiver, 0)?
        };
        check.pay(2, 0)?;
        let actuals = std::rc::Rc::new(actuals);
        if let Some(key) = key {
            self.pure_queries.insert(
                key,
                query,
                source_cache::Value::Parent(actuals.clone()),
                limits,
                check.left,
            )?;
        }
        Ok(actuals)
    }
}
