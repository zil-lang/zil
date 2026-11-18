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
    requests: std::cell::RefCell<
        Option<BTreeMap<SemanticDefinitionId, std::rc::Rc<Vec<NominalRequest>>>>,
    >,
    // Bound to this immutable original proof and its declaration forest.
    proved: std::cell::RefCell<
        BTreeMap<(SemanticDefinitionId, zil_base::Fingerprint), Vec<SemanticTypeRecipe>>,
    >,
    proved_count: std::cell::Cell<usize>,
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
        closure_result_goal: None,
        integer_initializers: BTreeMap::new(),
        contextual_code_goals: BTreeMap::new(),
        contextual_code_need: None,
        contextual_code_inputs: BTreeMap::new(),
        contextual_code_calls: BTreeSet::new(),
        predicate_active: Vec::new(),
        stage_depth: 0,
        cleanup_depth: 0,
        requested_snapshot: None,
        snapshot_sources: None,
        escape_active: Vec::new(),
    })
}
impl<'a> NominalWhere<'a> {
    pub(in crate::semantics) fn new(program: &'a SemanticModule, proof: &'a Proof) -> Self {
        Self {
            program,
            proof,
            requests: Default::default(),
            proved: Default::default(),
            proved_count: Default::default(),
        }
    }
    pub(in crate::semantics) fn requests(
        &self,
        definition: SemanticDefinitionId,
        limits: &Limits,
        left: &mut usize,
    ) -> R<std::rc::Rc<Vec<NominalRequest>>> {
        if self.requests.borrow().is_none() {
            let indexed = self.index_requests(limits, left)?;
            *self.requests.borrow_mut() = Some(indexed);
        }
        let requests = self.requests.borrow();
        let requests = requests
            .as_ref()
            .expect("completed original request directory");
        Self::fee(left, requests.len().saturating_add(1).ilog2() as usize + 2)?;
        let original = requests.get(&definition).cloned().unwrap_or_default();
        Self::fee(left, self.proof.definition_positions.len().saturating_add(1).ilog2() as usize + 1)?;
        let is_poll = self.proof.definition_positions.get(&definition)
            .and_then(|index| self.program.definitions.get(*index))
            .is_some_and(|row| matches!(row.body, SemanticDefinitionBody::Nominal { declaration, .. }
                if declaration == zil_ir::builtin_life::POLL));
        if !is_poll {
            return Ok(original);
        }
        self.task_poll_requests(original, limits, left)
    }
    fn task_poll_requests(
        &self,
        original: std::rc::Rc<Vec<NominalRequest>>,
        limits: &Limits,
        left: &mut usize,
    ) -> R<std::rc::Rc<Vec<NominalRequest>>> {
        let mut derived = Vec::new();
        for ((caller, _), call) in &self.proof.intrinsics {
            Self::fee(left, 1)?;
            if !matches!(call.kind, IntrinsicKind::TaskCreate { owned: false, await_result: false, .. }) {
                continue;
            }
            let SemanticTypeRecipe::ErrorUnion { result, .. } = &call.result else { continue; };
            let SemanticTypeRecipe::Managed(task) = result.as_ref() else { continue; };
            let SemanticTypeRecipe::Task { result: payload, .. } = task.as_ref() else { continue; };
            if !matches!(payload.as_ref(), SemanticTypeRecipe::Address { mutable: false, .. }) {
                continue;
            }
            Self::fee(left, call.parameters.len())?;
            let mut ports = call.parameters.iter().filter(|ty| matches!(ty, SemanticTypeRecipe::Address { .. }));
            if ports.next().is_none_or(|port| port != payload.as_ref()) || ports.next().is_some() {
                continue;
            }
            // The current single-port constructor registers a Poll template
            // even if the body only closes the owner. The original creation
            // has already checked its worker, input and result inclusion. Its
            // complete caller-local result is the sole derived Poll argument.
            let recipe = borrowed_check(self.program, self.proof, *caller, limits, left)?
                .poll_carrier(payload, 0)?;
            let mut duplicate = false;
            for request in original.iter().chain(&derived) {
                Self::fee(left, 1)?;
                if request.caller != *caller {
                    continue;
                }
                crate::dyn_metadata::semantic_operand_snapshot(&request.recipe, limits, left, 1)?;
                crate::dyn_metadata::semantic_operand_snapshot(&recipe, limits, left, 1)?;
                if request.recipe == recipe {
                    duplicate = true;
                    break;
                }
            }
            if !duplicate {
                if original.len().saturating_add(derived.len()) >= limits.max_nodes {
                    return Err(Diagnostic::limit(Phase::Types, "original Task Poll request directory node limit exceeded", None));
                }
                Self::fee(left, std::mem::size_of::<NominalRequest>())?;
                derived.push(NominalRequest { caller: *caller, recipe });
            }
        }
        if derived.is_empty() {
            return Ok(original);
        }
        for request in original.iter() {
            Self::fee(left, std::mem::size_of::<NominalRequest>())?;
            let recipe = copy_recipe(&request.recipe, limits, left)?;
            derived.push(NominalRequest { caller: request.caller, recipe });
        }
        Ok(std::rc::Rc::new(derived))
    }
    fn fee(left: &mut usize, amount: usize) -> R<()> {
        *left = left.checked_sub(amount).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "original nominal request directory work exceeded",
                None,
            )
        })?;
        Ok(())
    }
    fn index_requests(
        &self,
        limits: &Limits,
        left: &mut usize,
    ) -> R<BTreeMap<SemanticDefinitionId, std::rc::Rc<Vec<NominalRequest>>>> {
        // One complete original scan for the immutable proof, rather than a
        // deep-copy traversal of every type again for each nominal target.
        // The existing substitution walker retains all Bound/Invocation and
        // opaque-owner boundaries, including complete mixed Const/Life parts.
        let mut result: BTreeMap<SemanticDefinitionId, Vec<NominalRequest>> = BTreeMap::new();
        let mut seen: BTreeMap<
            (SemanticDefinitionId, zil_base::Fingerprint),
            Vec<(SemanticDefinitionId, usize)>,
        > = BTreeMap::new();
        let mut count = 0usize;
        for ((caller, _), value) in &self.proof.types {
            Self::fee(
                left,
                self.proof.complete.len().saturating_add(1).ilog2() as usize + 2,
            )?;
            if !self.proof.complete.contains(caller) {
                continue;
            }
            for recipe in substitute::nominal_types(value, None, limits, left)? {
                let SemanticTypeRecipe::Nominal { definition, .. } = &recipe else {
                    return Err(bad(
                        "original nominal request collector changed its recipe kind",
                    ));
                };
                let definition = *definition;
                let digest = protocol::recipe_key(&recipe, limits, left)?;
                let key = (*caller, digest);
                Self::fee(left, seen.len().saturating_add(1).ilog2() as usize + 2)?;
                let mut duplicate = false;
                if let Some(bucket) = seen.get(&key) {
                    for (target, index) in bucket {
                        let previous = &result[target][*index];
                        crate::dyn_metadata::semantic_operand_snapshot(
                            &previous.recipe,
                            limits,
                            left,
                            1,
                        )?;
                        crate::dyn_metadata::semantic_operand_snapshot(&recipe, limits, left, 1)?;
                        if previous.caller == *caller && previous.recipe == recipe {
                            duplicate = true;
                            break;
                        }
                    }
                }
                if duplicate {
                    continue;
                }
                if count >= limits.max_nodes {
                    return Err(Diagnostic::limit(
                        Phase::Types,
                        "original nominal request directory node limit exceeded",
                        None,
                    ));
                }
                Self::fee(
                    left,
                    std::mem::size_of::<NominalRequest>()
                        + std::mem::size_of::<(SemanticDefinitionId, usize)>()
                        + result.len().saturating_add(1).ilog2() as usize
                        + seen.len().saturating_add(1).ilog2() as usize
                        + 2,
                )?;
                let rows = result.entry(definition).or_default();
                let index = rows.len();
                rows.push(NominalRequest {
                    caller: *caller,
                    recipe,
                });
                seen.entry(key).or_default().push((definition, index));
                count += 1;
            }
        }
        Self::fee(
            left,
            result.len().saturating_mul(
                2 * std::mem::size_of::<usize>() + std::mem::size_of::<Vec<NominalRequest>>(),
            ),
        )?;
        Ok(result
            .into_iter()
            .map(|(definition, rows)| (definition, std::rc::Rc::new(rows)))
            .collect())
    }
    pub(in crate::semantics) fn prove(
        &self,
        caller: SemanticDefinitionId,
        recipe: &SemanticTypeRecipe,
        limits: &Limits,
        left: &mut usize,
    ) -> R<()> {
        // Digests only choose a bucket. A theorem belongs to the exact caller
        // Gamma and complete mixed nominal application, including phantom args.
        let digest = protocol::recipe_key(recipe, limits, left)?;
        let key = (caller, digest);
        {
            let proved = self.proved.borrow();
            *left = left
                .checked_sub(proved.len().saturating_add(1).ilog2() as usize + 1)
                .ok_or_else(|| {
                    Diagnostic::limit(Phase::Types, "nominal where cache lookup exceeded", None)
                })?;
            if let Some(bucket) = proved.get(&key) {
                for previous in bucket {
                    crate::dyn_metadata::semantic_operand_snapshot(previous, limits, left, 1)?;
                    crate::dyn_metadata::semantic_operand_snapshot(recipe, limits, left, 1)?;
                    if previous == recipe {
                        return Ok(());
                    }
                }
            }
        }
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
        let before_mapping = *check.left;
        let mapped = super::super::instantiate::requirements(
            self.program,
            self.proof,
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
                ),
            super::super::instantiate::DomainMapping {
                source: *definition,
                use_definition: caller,
                arguments,
                parent_call: None,
            },
            limits,
            check.left,
        )
        .map_err(|mut error| {
            error.notes.push(format!(
                "original nominal obligation mapping work {}",
                before_mapping.saturating_sub(*check.left)
            ));
            error
        })?;
        let mapping_work = before_mapping.saturating_sub(*check.left);
        let before_proof = *check.left;
        let result = (|| {
            for (ordinal, requirement) in mapped.iter().enumerate() {
                check.satisfy_requirement(caller, requirement, 0).map_err(|mut error| {
                    error.notes.push(format!("original nominal obligation {ordinal}/{}; mapping work {mapping_work}; requirement proof work {}",
                        mapped.len(), before_proof.saturating_sub(*check.left)));
                    error
                })?;
            }
            Ok(())
        })();
        if !matches!(check.proof, ProofAccess::Borrowed(_)) {
            return Err(bad(
                "read-only original where unexpectedly changed its proof context",
            ));
        }
        result?;
        crate::dyn_metadata::semantic_operand_snapshot(recipe, limits, left, 1)?;
        let mut proved = self.proved.borrow_mut();
        let entries = self.proved_count.get();
        if entries >= limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "nominal where cache entry limit exceeded",
                None,
            ));
        }
        *left = left
            .checked_sub(proved.len().saturating_add(1).ilog2() as usize + 1)
            .ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "nominal where cache insertion exceeded", None)
            })?;
        proved.entry(key).or_default().push(recipe.clone());
        self.proved_count.set(entries + 1);
        Ok(())
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
        let query = source_cache::Query::WhereEntry;
        let key = if self.pure_queries_ready {
            let (key, cached) = self
                .pure_queries
                .lookup(owner, &query, limits, check.left)?;
            if let Some(value) = cached {
                return match value {
                    source_cache::Value::WhereEntry => Ok(()),
                    _ => Err(bad(
                        "original where-entry cache changes its exact query kind",
                    )),
                };
            }
            Some(key)
        } else {
            None
        };
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
        if let Some(key) = key {
            self.pure_queries.insert(
                key,
                query,
                source_cache::Value::WhereEntry,
                limits,
                check.left,
            )?;
        }
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
    ) -> R<std::rc::Rc<Vec<SemanticActualRecipe>>> {
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
        for argument in arguments {
            crate::dyn_metadata::semantic_operand_snapshot(argument, limits, check.left, 1)?;
        }
        let query = source_cache::Query::WhereCall {
            site,
            target,
            arguments: arguments.clone(),
        };
        let cache_key = if self.pure_queries_ready {
            let (key, cached) = self
                .pure_queries
                .lookup(caller, &query, limits, check.left)?;
            if let Some(value) = cached {
                return match value {
                    source_cache::Value::WhereCall(value) => Ok(value),
                    _ => Err(bad(
                        "original where-call cache changes its exact query kind",
                    )),
                };
            }
            Some(key)
        } else {
            None
        };
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
        let mapped = super::super::instantiate::requirements(
            program,
            self,
            original
                .requirements
                .iter()
                .chain(original.parameters.iter().flat_map(|formal| match formal {
                    SemanticFormal::Type { requirements } => requirements.as_slice(),
                    _ => &[],
                }))
                .filter(|requirement| {
                    !matches!(
                        requirement,
                        // Exit obligations belong to the independently checked
                        // callee body; they are not premises in the caller Gamma.
                        SemanticRequirementRecipe::Callable {
                            ty: SemanticTypeRecipe::OriginalType {
                                scope: SemanticScopeRef::Ancestor(0),
                                source: SemanticOriginalTypeSource::Result,
                            },
                            ..
                        }
                    )
                }),
            super::super::instantiate::DomainMapping {
                source: target,
                use_definition: caller,
                arguments,
                parent_call: self.method_calls.contains_key(&key).then_some(site),
            },
            limits,
            check.left,
        )?;
        for requirement in &mapped {
            check.satisfy_requirement(caller, requirement, 0)?;
        }
        for argument in arguments {
            crate::dyn_metadata::semantic_operand_snapshot(argument, limits, check.left, 1)?;
        }
        check.pay(1, 0)?;
        let arguments = std::rc::Rc::new(arguments.clone());
        if let Some(key) = cache_key {
            self.pure_queries.insert(
                key,
                query,
                source_cache::Value::WhereCall(arguments.clone()),
                limits,
                check.left,
            )?;
        }
        Ok(arguments)
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
        {
            return Err(bad(
                "parent specialization changes its complete original provider boundary",
            ));
        }
        let parent = check.definition(holder)?;
        let actual = check.definition(provider)?;
        if actual.parent != Some(holder) {
            let Some(lexical) = actual.parent else {
                return Err(bad("default provider lacks its true lexical parent"));
            };
            let SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl {
                trait_application,
                ..
            }) = &parent.body
            else {
                return Err(bad(
                    "default provider did not select an actual implementation",
                ));
            };
            if provider != method.declaration
                || !matches!(
                    actual.body,
                    SemanticDefinitionBody::FunctionWithBody {
                        body: SemanticOriginalFunctionBody::Defined(_),
                        ..
                    }
                )
                || !matches!(&check.definition(lexical)?.body, SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait{declaration}) if *declaration == trait_application.declaration)
                || application.is_none_or(|app| app.declaration != trait_application.declaration)
            {
                return Err(bad(
                    "default provider changes its original method or trait scope",
                ));
            }
            check.pay(self.providers.len(), 0)?;
            for (candidate, required) in &self.providers {
                if *required == provider && check.definition(*candidate)?.parent == Some(holder) {
                    return Err(bad(
                        "default provider replaces the actual implementation override",
                    ));
                }
            }
        }
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

#[cfg(test)]
mod request_tests {
    use super::*;
    fn id(index: u32) -> SemanticDefinitionId {
        SemanticDefinitionId {
            module: zil_base::ModuleId(0),
            index,
        }
    }
    fn nominal(definition: u32, lifetime: u32, constant: i32) -> SemanticTypeRecipe {
        SemanticTypeRecipe::Nominal {
            definition: id(definition),
            arguments: vec![
                SemanticActualRecipe::Type(SemanticTypeRecipe::Atom(Type::I32)),
                SemanticActualRecipe::Life(SemanticLifeRecipe::Formal(SemanticFormalRef {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal: lifetime,
                })),
                SemanticActualRecipe::Const {
                    carrier: SemanticTypeRecipe::Atom(Type::I32),
                    value: SemanticConstRecipe::Value(Constant::I32(constant)),
                },
            ],
        }
    }
    #[test]
    fn request_directory_keeps_callers_and_full_mixed_arguments() {
        let program = SemanticModule {
            definitions: vec![],
            instances: vec![],
            sites: vec![],
            dyn_scopes: vec![],
        };
        let mut proof = Proof::default();
        proof.complete.extend([id(0), id(1)]);
        for (owner, node, recipe) in [
            (0, 0, nominal(7, 0, 1)),
            (0, 1, nominal(7, 0, 1)),
            (0, 2, nominal(7, 1, 1)),
            (0, 3, nominal(7, 0, 2)),
            (1, 0, nominal(7, 0, 1)),
            (0, 4, nominal(8, 0, 1)),
        ] {
            proof
                .types
                .insert((id(owner), Query::Expression(NodeId(node))), recipe);
        }
        let limits = Limits::default();
        let mut left = limits.max_analysis_steps;
        let session = NominalWhere::new(&program, &proof);
        let first = session.requests(id(7), &limits, &mut left).unwrap();
        assert_eq!(first.len(), 4);
        assert_eq!(first.iter().filter(|row| row.caller == id(0)).count(), 3);
        let before = left;
        let repeated = session.requests(id(7), &limits, &mut left).unwrap();
        assert!(std::rc::Rc::ptr_eq(&first, &repeated));
        assert!(before - left < 16);
        assert_eq!(
            session.requests(id(8), &limits, &mut left).unwrap().len(),
            1
        );
        // Indexing occurrences does not establish their where requirements.
        assert!(
            session
                .prove(id(0), &first[0].recipe, &limits, &mut left)
                .is_err()
        );
    }
    #[test]
    fn all_nominal_collection_preserves_code_and_opaque_owner_boundaries() {
        let outer = nominal(7, 0, 1);
        let hidden = nominal(8, 0, 1);
        let code = SemanticTypeRecipe::Code {
            kind: SemanticCodeKind::BorrowedFunctionPointer,
            signature: Box::new(SemanticSignatureRecipe {
                bound_lifetimes: 1,
                invocation_lifetimes: 1,
                parameters: vec![],
                result: hidden.clone(),
                result_mode: PassingMode::Copy,
                call_kind: CallKind::Direct,
            }),
        };
        let opaque = SemanticTypeRecipe::Opaque {
            definition: id(9),
            arguments: vec![SemanticActualRecipe::Type(outer.clone())],
            hidden: Box::new(hidden),
        };
        let value = SemanticTypeRecipe::Tuple(vec![code, opaque]);
        let limits = Limits::default();
        let mut left = limits.max_analysis_steps;
        let all = substitute::nominal_types(&value, None, &limits, &mut left).unwrap();
        assert_eq!(all, vec![outer.clone()]);
        assert_eq!(
            substitute::nominal_types(&value, Some(id(7)), &limits, &mut left).unwrap(),
            vec![outer]
        );
        assert!(
            substitute::nominal_types(&value, Some(id(8)), &limits, &mut left)
                .unwrap()
                .is_empty()
        );
    }
}
