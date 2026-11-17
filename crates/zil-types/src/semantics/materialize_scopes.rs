//! A specialized trait method's lexical parent is solved in the original
//! argument's source Gamma. Runtime Self images never become original facts.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

// Optional type-world reuse may be cold only at an explicit provenance or
// lexical-transport boundary. Original query failures are never mismatches.
enum SourceFailure {
    Unavailable(Diagnostic),
    Invalid(Diagnostic),
}
type SourceResult<T> = Result<T, SourceFailure>;
impl SourceFailure {
    fn into_diagnostic(self) -> Diagnostic {
        match self {
            Self::Unavailable(error) | Self::Invalid(error) => error,
        }
    }
    // actual_in_domains has no original query resolver. Its Unsupported
    // exits mean missing lexical/value/Invocation correspondence; malformed
    // substitutions and resource failures retain their original diagnostics.
    fn transport(error: Diagnostic) -> Self {
        if error.category == zil_base::ResultCategory::Unsupported {
            Self::Unavailable(error)
        } else {
            Self::Invalid(error)
        }
    }
}

struct SourceActual {
    scope: SemanticInstanceId,
    recipe: SemanticActualRecipe,
}
impl<'a> Eval<'a, '_, '_> {
    /// A closed Type world still needs its complete original application
    /// identity. Reuse original argument provenance, never resolved layouts or
    /// an invented callsite, to compare every mixed position in one real Gamma.
    pub(super) fn closed_application_source_matches(
        &mut self, caller: SemanticInstanceId, holder: SemanticInstanceId,
        arguments: &[SemanticActualRecipe], depth: usize,
    ) -> R<bool> {
        self.node(depth)?;
        let row = self.scope_source_instance(holder)?;
        self.fee(row.actuals.len().saturating_add(arguments.len()))?;
        if row.actuals.len() != arguments.len() { return Ok(false); }
        for (ordinal, expected) in arguments.iter().enumerate() {
            let original: SourceResult<bool> = (|| {
                let expected = self.source_specialize(caller, expected, &mut BTreeSet::new(), depth + 1)?;
                let actual = self.source_actual(holder, ordinal as u32, &mut BTreeSet::new(), depth + 1)?;
                let actual = self.source_relocate(&actual, expected.scope, depth + 1)?;
                let owner = self.scope_source_instance(expected.scope).map_err(SourceFailure::Invalid)?.definition;
                let proof = self.ledger.original.borrow();
                let proof = proof.as_ref()
                    .ok_or_else(|| bad("closed application identity lacks its original proof")).map_err(SourceFailure::Invalid)?;
                proof.equal_closed_actual_identity(self.input, owner,
                    &expected.recipe, &actual, self.limits, self.left).map_err(SourceFailure::Invalid)
            })();
            match original {
                Ok(true) => {}
                Ok(false) => return Ok(false),
                Err(SourceFailure::Unavailable(_)) => return Ok(false),
                Err(SourceFailure::Invalid(error)) => return Err(error),
            }
        }
        Ok(true)
    }

    /// Recover a Static view only from the immutable original actual recipe.
    /// A resolved physical address with an empty table is not such a source.
    pub(super) fn source_static_outer(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticActualRecipe,
        depth: usize,
    ) -> R<bool> {
        self.node(depth)?;
        let source = self.source_specialize(scope, recipe, &mut BTreeSet::new(), depth + 1)
            .map_err(SourceFailure::into_diagnostic)?;
        match source.recipe {
            SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType { scope, source: query }) => {
                let expanded = self.original_type(source.scope, scope, &query, depth + 1)?;
                self.source_static_outer(source.scope, &SemanticActualRecipe::Type(expanded), depth + 1)
            }
            SemanticActualRecipe::Type(SemanticTypeRecipe::Address {
                mutable: false,
                lifetime: SemanticOuterLifeRecipe::Static,
                ..
            }) => Ok(true),
            _ => Ok(false),
        }
    }

    fn scope_source_instance(&mut self, id: SemanticInstanceId) -> R<&'a SemanticInstance> {
        // The publication materializes actuals in place. Their defining
        // recipes remain in the one immutable original request context.
        if let Some(context) = self.ledger.original.context() {
            match context.instance(id, self.left) {
                Ok(row) => {
                    // Source provenance belongs to the immutable publication
                    // input, not a preparation view's retargeted actual row.
                    let actual = if let Some(actual) =
                        self.module.indexed_instance(self.input, id, self.left)?
                    {
                        actual
                    } else {
                        self.fee(self.input.instances.len())?;
                        self.input
                            .instances
                            .iter()
                            .find(|actual| actual.id == id)
                            .ok_or_else(|| bad("parent source request has no actual instance"))?
                    };
                    if row.definition != actual.definition || row.parent != actual.parent {
                        return Err(bad(
                            "parent source request changes its original instance identity",
                        ));
                    }
                    return Ok(row);
                }
                Err(error) if error.category == zil_base::ResultCategory::ResourceLimit => {
                    return Err(error);
                }
                Err(_) => {}
            }
        }
        if let Some(row) = self.module.indexed_instance(self.input, id, self.left)? {
            return Ok(row);
        }
        self.fee(self.input.instances.len())?;
        if let Some(row) = self.input.instances.iter().find(|row| row.id == id) {
            return Ok(row);
        }
        if let Some(source) = self.ledger.frozen.as_deref() {
            self.fee(source.semantic.instances.len())?;
            if let Some(row) = source.semantic.instances.iter().find(|row| row.id == id) {
                return Ok(row);
            }
        }
        Err(bad(
            "parent inference has no immutable original instance request",
        ))
    }
    fn scope_chain(&mut self, id: SemanticInstanceId, depth: usize) -> R<Vec<SemanticInstanceId>> {
        let mut rows = Vec::new();
        let mut next = Some(id);
        while let Some(id) = next {
            self.node(depth.saturating_add(rows.len()))?;
            self.fee(rows.len().saturating_add(1))?;
            if rows.contains(&id) {
                return Err(bad("parent inference has a cyclic lexical instance chain"));
            }
            let row = self.scope_source_instance(id)?;
            let definition = self.definition(row.definition)?;
            if row
                .parent
                .map(|id| self.scope_source_instance(id).map(|row| row.definition))
                .transpose()?
                != definition.parent
            {
                return Err(bad(
                    "parent inference changes the true original lexical chain",
                ));
            }
            next = row.parent;
            rows.push(id);
        }
        Ok(rows)
    }
    fn source_relocate(
        &mut self,
        value: &SourceActual,
        target: SemanticInstanceId,
        depth: usize,
    ) -> SourceResult<SemanticActualRecipe> {
        self.node(depth).map_err(SourceFailure::Invalid)?;
        let source = self.scope_chain(value.scope, depth + 1).map_err(SourceFailure::Invalid)?;
        let destination = self.scope_chain(target, depth + 1).map_err(SourceFailure::Invalid)?;
        self.fee(
            source
                .len()
                .saturating_mul(destination.len())
                .saturating_add(1),
        ).map_err(SourceFailure::Invalid)?;
        let mut scopes = BTreeMap::new();
        for (hop, id) in source.iter().enumerate() {
            if let Some(index) = destination.iter().position(|other| other == id) {
                scopes.insert(hop as u32, index as u32);
            }
        }
        let empty = BTreeMap::new();
        super::super::original::substitute::actual_in_domains(
            &value.recipe,
            &super::super::original::substitute::Substitution {
                frames: &[],
                selves: &[],
                environments: &[],
                implicit: &empty,
                invocation: &empty,
                shift: 0,
            },
            &scopes,
            value.scope == target,
            self.limits,
            self.left,
        ).map_err(SourceFailure::transport)
    }
    fn source_actual(
        &mut self,
        holder: SemanticInstanceId,
        ordinal: u32,
        active: &mut BTreeSet<(SemanticInstanceId, u32)>,
        depth: usize,
    ) -> SourceResult<SourceActual> {
        self.node(depth).map_err(SourceFailure::Invalid)?;
        self.fee(active.len().saturating_add(1)).map_err(SourceFailure::Invalid)?;
        if !active.insert((holder, ordinal)) {
            return Err(SourceFailure::Invalid(bad("original parent argument provenance is cyclic")));
        }
        let result = (|| {
            let row = self.scope_source_instance(holder).map_err(SourceFailure::Invalid)?;
            let original = self.definition(row.definition).map_err(SourceFailure::Invalid)?;
            if row.actuals.len() != original.parameters.len() {
                return Err(SourceFailure::Invalid(bad("parent source omits complete mixed original arguments")));
            }
            let value = row
                .actuals
                .get(ordinal as usize)
                .ok_or_else(|| bad("parent argument has no full original ordinal")).map_err(SourceFailure::Invalid)?;
            if let SemanticInstanceTarget::Function(SemanticFunctionTarget::Core { module, function }) = row.target {
                self.fee(self.ledger.templates.len().saturating_add(1).ilog2() as usize + 1)
                    .map_err(SourceFailure::Invalid)?;
                if self.ledger.templates.contains(&super::super::emission::Origin {
                    module, creator: function, closure: None,
                }) && matches!(value, SemanticActual::CoreRecipe { scope, recipe }
                    if *scope == holder && match recipe.as_ref() {
                        SemanticActualRecipe::Type(SemanticTypeRecipe::Formal(reference)) =>
                            reference.scope == SemanticScopeRef::Ancestor(0) && reference.ordinal == ordinal,
                        SemanticActualRecipe::Const { value: SemanticConstRecipe::Formal(reference), .. } =>
                            reference.scope == SemanticScopeRef::Ancestor(0) && reference.ordinal == ordinal,
                        _ => false,
                    })
                {
                    // A retained source template is not an application. Only
                    // its exact emitted request can supply this full actual.
                    return self.source_requested_actual(holder, original.id, ordinal, active, depth + 1);
                }
            }
            match value {
                SemanticActual::CoreRecipe { scope, recipe } => {
                    self.source_specialize(*scope, recipe, active, depth + 1)
                }
                SemanticActual::CoreInferred {
                    scope,
                    call,
                    formal,
                } => {
                    if *formal != ordinal {
                        return Err(SourceFailure::Invalid(bad("parent source changes a full mixed inference ordinal")));
                    }
                    if matches!(original.body, SemanticDefinitionBody::Scope(_)) {
                        let (source, actuals) =
                            self.source_parent_solution(*scope, *call, holder, active, depth + 1)?;
                        let recipe = actuals
                            .get(ordinal as usize)
                            .ok_or_else(|| bad("parent solution omits a full mixed ordinal")).map_err(SourceFailure::Invalid)?;
                        crate::dyn_metadata::semantic_operand_snapshot(
                            recipe,
                            self.limits,
                            self.left,
                            1,
                        ).map_err(SourceFailure::Invalid)?;
                        let recipe = recipe.clone();
                        Ok(SourceActual {
                            scope: source,
                            recipe,
                        })
                    } else {
                        let owner = self.scope_source_instance(*scope).map_err(SourceFailure::Invalid)?.definition;
                        let recipe = {
                            let proof = self.ledger.original.borrow();
                            let proof = proof
                                .as_ref()
                                .ok_or_else(|| bad("parent source has no original proof")).map_err(SourceFailure::Invalid)?;
                            let target = proof
                                .inferred_call_target(owner, *call, self.limits, self.left).map_err(SourceFailure::Invalid)?
                                .ok_or_else(|| bad("parent source call has no original target")).map_err(SourceFailure::Invalid)?;
                            if target != original.id
                                && proof.providers.get(&original.id) != Some(&target)
                            {
                                return Err(SourceFailure::Invalid(bad(
                                    "parent source call does not target the actual definition",
                                )));
                            }
                            proof.call_actual(owner, *call, *formal, self.limits, self.left).map_err(SourceFailure::Invalid)?
                        };
                        self.source_specialize(*scope, &recipe, active, depth + 1)
                    }
                }
                SemanticActual::Type(SemanticTypeValue::Core { scope, source }) => {
                    let recipe = match source {
                        SemanticCoreTypeSource::OriginalRecipe(recipe) => {
                            crate::dyn_metadata::semantic_operand_snapshot(
                                recipe.as_ref(),
                                self.limits,
                                self.left,
                                1,
                            ).map_err(SourceFailure::Invalid)?;
                            recipe.as_ref().clone()
                        }
                        SemanticCoreTypeSource::OriginalExpression { source, .. } => {
                            crate::dyn_metadata::semantic_operand_snapshot(
                                source,
                                self.limits,
                                self.left,
                                1,
                            ).map_err(SourceFailure::Invalid)?;
                            SemanticTypeRecipe::OriginalType {
                                scope: SemanticScopeRef::Ancestor(0),
                                source: source.clone(),
                            }
                        }
                        _ => {
                            return self.source_requested_actual(holder, original.id, ordinal, active, depth + 1);
                        }
                    };
                    self.source_specialize(
                        *scope,
                        &SemanticActualRecipe::Type(recipe),
                        active,
                        depth + 1,
                    )
                }
                _ => self.source_requested_actual(holder, original.id, ordinal, active, depth + 1),
            }
        })();
        active.remove(&(holder, ordinal));
        result
    }
    /// Recover a newly emitted world's source application only from the
    /// emitter's exact request. A resolved Type is never reverse engineered.
    fn source_requested_actual(
        &mut self, holder: SemanticInstanceId, definition: SemanticDefinitionId,
        ordinal: u32, active: &mut BTreeSet<(SemanticInstanceId, u32)>, depth: usize,
    ) -> SourceResult<SourceActual> {
        self.node(depth).map_err(SourceFailure::Invalid)?;
        let ledger = self.ledger;
        let request = {
            let requests = ledger.instance_requests.borrow();
            self.fee(requests.len().saturating_add(1).ilog2() as usize + 1)
                .map_err(SourceFailure::Invalid)?;
            requests.get(&holder).copied()
        };
        let Some((caller, call, function)) = request else {
            return Err(SourceFailure::Unavailable(unknown(
                "original argument has neither a source recipe nor an exact emitted call request",
            )));
        };
        let actual = self.instance(holder).map_err(SourceFailure::Invalid)?;
        if actual.definition != definition
            || actual.target != SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))
        {
            return Err(SourceFailure::Invalid(bad(
                "original argument request changes its actual callee identity",
            )));
        }
        let owner = self.scope_source_instance(caller).map_err(SourceFailure::Invalid)?.definition;
        let recipe = {
            let proof = ledger.original.borrow();
            let proof = proof.as_ref().ok_or_else(||
                SourceFailure::Invalid(bad("original argument request has no checked source proof")))?;
            let target = proof.inferred_call_target(owner, call, self.limits, self.left)
                .map_err(SourceFailure::Invalid)?
                .ok_or_else(|| SourceFailure::Invalid(bad("original argument request has no original call target")))?;
            if target != definition && proof.providers.get(&definition) != Some(&target) {
                return Err(SourceFailure::Invalid(bad(
                    "original argument request changes its original callee definition",
                )));
            }
            proof.call_actual(owner, call, ordinal, self.limits, self.left)
                .map_err(SourceFailure::Invalid)?
        };
        self.source_specialize(caller, &recipe, active, depth + 1)
    }
    fn source_specialize(
        &mut self,
        scope: SemanticInstanceId,
        value: &SemanticActualRecipe,
        active: &mut BTreeSet<(SemanticInstanceId, u32)>,
        depth: usize,
    ) -> SourceResult<SourceActual> {
        self.node(depth).map_err(SourceFailure::Invalid)?;
        let ancestry = self.scope_chain(scope, depth + 1).map_err(SourceFailure::Invalid)?;
        let mut arguments = Vec::new();
        let mut destination = None;
        for id in &ancestry {
            let row = self.scope_source_instance(*id).map_err(SourceFailure::Invalid)?;
            self.fee(row.actuals.len().saturating_add(1)).map_err(SourceFailure::Invalid)?;
            let mut frame = Vec::new();
            for ordinal in 0..row.actuals.len() {
                let actual = self.source_actual(*id, ordinal as u32, active, depth + 1)?;
                if destination.is_none() {
                    destination = Some(actual.scope);
                }
                frame.push(actual);
            }
            arguments.push(frame);
        }
        let destination = destination.unwrap_or(scope);
        let mut mapped = Vec::new();
        self.fee(arguments.len()).map_err(SourceFailure::Invalid)?;
        for frame in &arguments {
            let mut values = Vec::new();
            self.fee(frame.len()).map_err(SourceFailure::Invalid)?;
            for actual in frame {
                values.push(self.source_relocate(actual, destination, depth + 1)?);
            }
            mapped.push(values);
        }
        let destination_chain = self.scope_chain(destination, depth + 1).map_err(SourceFailure::Invalid)?;
        self.fee(
            ancestry
                .len()
                .saturating_mul(destination_chain.len())
                .saturating_add(mapped.len()),
        ).map_err(SourceFailure::Invalid)?;
        let mut scopes = BTreeMap::new();
        for (hop, id) in ancestry.iter().enumerate() {
            if let Some(index) = destination_chain.iter().position(|other| other == id) {
                scopes.insert(hop as u32, index as u32);
            }
        }
        let frames: Vec<_> = mapped
            .iter()
            .enumerate()
            .map(|(hop, actuals)| (hop as u32, actuals.as_slice()))
            .collect();
        let empty = BTreeMap::new();
        let recipe = super::super::original::substitute::actual_in_domains(
            value,
            &super::super::original::substitute::Substitution {
                frames: &frames,
                selves: &[],
                environments: &[],
                implicit: &empty,
                invocation: &empty,
                shift: 0,
            },
            &scopes,
            scope == destination,
            self.limits,
            self.left,
        ).map_err(SourceFailure::transport)?;
        Ok(SourceActual {
            scope: destination,
            recipe,
        })
    }
    fn source_parent_solution(
        &mut self,
        caller: SemanticInstanceId,
        call: SemanticSiteId,
        holder: SemanticInstanceId,
        active: &mut BTreeSet<(SemanticInstanceId, u32)>,
        depth: usize,
    ) -> SourceResult<(SemanticInstanceId, std::rc::Rc<Vec<SemanticActualRecipe>>)> {
        self.node(depth).map_err(SourceFailure::Invalid)?;
        let key = (caller, call, holder);
        if let Some(value) = self.module.parent_solution(
            self.input,
            self.ledger.original.context(),
            key,
            self.left,
        ).map_err(SourceFailure::Invalid)? {
            return Ok(value);
        }
        let value = self.source_parent_solution_uncached(caller, call, holder, active, depth)?;
        self.module.remember_parent(
            self.input,
            self.ledger.original.context(),
            key,
            &value,
            self.limits,
            self.left,
        ).map_err(SourceFailure::Invalid)?;
        Ok(value)
    }
    fn source_parent_solution_uncached(
        &mut self,
        caller: SemanticInstanceId,
        call: SemanticSiteId,
        holder: SemanticInstanceId,
        active: &mut BTreeSet<(SemanticInstanceId, u32)>,
        depth: usize,
    ) -> SourceResult<(SemanticInstanceId, std::rc::Rc<Vec<SemanticActualRecipe>>)> {
        self.node(depth).map_err(SourceFailure::Invalid)?;
        let caller_definition = self.scope_source_instance(caller).map_err(SourceFailure::Invalid)?.definition;
        let holder_definition = self.scope_source_instance(holder).map_err(SourceFailure::Invalid)?.definition;
        let function = self.function(caller).map_err(SourceFailure::Invalid)?;
        self.fee(self.input.sites.len()).map_err(SourceFailure::Invalid)?;
        let mut sites = self
            .input
            .sites
            .iter()
            .filter(|row| row.instance == caller && row.recipe == call);
        let site = sites
            .next()
            .ok_or_else(|| bad("parent inference has no actual call site")).map_err(SourceFailure::Invalid)?;
        if sites.next().is_some() {
            return Err(SourceFailure::Invalid(bad("parent inference has multiple actual call sites")));
        }
        let (block, instruction) = self.actual_call_point(caller, site, depth + 1).map_err(SourceFailure::Invalid)?;
        self.fee(function.blocks.len()).map_err(SourceFailure::Invalid)?;
        let op = function
            .blocks
            .iter()
            .find(|row| row.id == block)
            .and_then(|row| row.instructions.get(instruction as usize))
            .ok_or_else(|| bad("parent inference call leaves the actual CFG")).map_err(SourceFailure::Invalid)?;
        let (InstructionKind::Call {
            function: target, ..
        }
        | InstructionKind::SuspendCall {
            function: target, ..
        }) = &op.kind
        else {
            return Err(SourceFailure::Invalid(unknown(
                "parent inference requires its true direct method opcode",
            )));
        };
        let count = self.input.instances.len();
        self.fee(count.saturating_mul(count.saturating_add(1).ilog2() as usize + 1)).map_err(SourceFailure::Invalid)?;
        let mut ids = BTreeSet::new();
        for row in &self.input.instances {
            ids.insert(row.id);
        }
        let mut provider = None;
        for id in ids {
            let row = self.instance(id).map_err(SourceFailure::Invalid)?;
            if row.parent == Some(holder)
                && row.target
                    == SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(*target))
            {
                if provider.replace(row.definition).is_some() {
                    return Err(SourceFailure::Invalid(bad(
                        "parent inference has several actual provider instances for its exact holder",
                    )));
                }
            }
        }
        let provider = provider.ok_or_else(|| {
            bad("parent inference has no actual provider with the exact lexical holder")
        }).map_err(SourceFailure::Invalid)?;
        let (receiver, application) = {
            let proof = self.ledger.original.borrow();
            let method = proof
                .as_ref()
                .and_then(|proof| proof.method_calls.get(&(caller_definition, call)))
                .ok_or_else(|| {
                    bad("parent inference lacks its original method receiver theorem")
                }).map_err(SourceFailure::Invalid)?;
            crate::dyn_metadata::semantic_operand_snapshot(
                &method.self_type,
                self.limits,
                self.left,
                1,
            ).map_err(SourceFailure::Invalid)?;
            if let Some(application) = &method.application {
                crate::dyn_metadata::semantic_operand_snapshot(
                    application,
                    self.limits,
                    self.left,
                    1,
                ).map_err(SourceFailure::Invalid)?;
            }
            (method.self_type.clone(), method.application.clone())
        };
        let source = self.source_specialize(
            caller,
            &SemanticActualRecipe::Type(receiver),
            active,
            depth + 1,
        )?;
        let SemanticActualRecipe::Type(receiver) = source.recipe else {
            unreachable!()
        };
        let application = if let Some(application) = application {
            let mut actuals = Vec::new();
            self.fee(application.arguments.len()).map_err(SourceFailure::Invalid)?;
            for actual in &application.arguments {
                let actual = self.source_specialize(caller, actual, active, depth + 1)?;
                actuals.push(self.source_relocate(&actual, source.scope, depth + 1)?);
            }
            Some(SemanticTraitRecipe {
                declaration: application.declaration,
                arguments: actuals,
            })
        } else {
            None
        };
        let source_owner = self.scope_source_instance(source.scope).map_err(SourceFailure::Invalid)?.definition;
        let actuals = {
            let proof = self.ledger.original.borrow();
            proof
                .as_ref()
                .ok_or_else(|| bad("parent specialization lost its original proof")).map_err(SourceFailure::Invalid)?
                .specialized_parent(
                    self.input,
                    source_owner,
                    caller_definition,
                    call,
                    provider,
                    holder_definition,
                    &receiver,
                    application.as_ref(),
                    self.limits,
                    self.left,
                ).map_err(SourceFailure::Invalid)?
        };
        Ok((source.scope, actuals))
    }
    pub(super) fn associated_source_type(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticTypeRecipe,
        hint: Option<&Type>,
        depth: usize,
    ) -> R<SemanticTypeValue> {
        self.node(depth)?;
        if !matches!(recipe, SemanticTypeRecipe::AssociatedType { .. }) {
            return Err(bad(
                "associated source reduction changes its original constructor",
            ));
        }
        crate::dyn_metadata::semantic_operand_snapshot(recipe, self.limits, self.left, 1)?;
        let source = self.source_specialize(
            scope,
            &SemanticActualRecipe::Type(recipe.clone()),
            &mut BTreeSet::new(),
            depth + 1,
        ).map_err(SourceFailure::into_diagnostic)?;
        let SemanticActualRecipe::Type(recipe) = source.recipe else {
            unreachable!()
        };
        let owner = self.scope_source_instance(source.scope)?.definition;
        let reduced = {
            let proof = self.ledger.original.borrow();
            proof
                .as_ref()
                .ok_or_else(|| bad("associated source has no complete original proof"))?
                .resolve_source_type(self.input, owner, &recipe, self.limits, self.left)?
        };
        crate::dyn_metadata::semantic_operand_snapshot(&recipe, self.limits, self.left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(
            reduced.as_ref(),
            self.limits,
            self.left,
            1,
        )?;
        if recipe == *reduced {
            return Err(unknown(
                "associated source has no finite original defining equation in its full specialization domain",
            ));
        }
        self.ty(source.scope, &reduced, hint, depth + 1)
    }

    pub(super) fn inferred_parent_actual(
        &mut self,
        caller: SemanticInstanceId,
        call: SemanticSiteId,
        formal: u32,
        holder: Option<SemanticInstanceId>,
        depth: usize,
    ) -> R<Option<SemanticActual>> {
        self.node(depth)?;
        let Some(holder) = holder else {
            return Ok(None);
        };
        let row = self.scope_source_instance(holder)?;
        let definition = row.definition;
        if !matches!(row.actuals.get(formal as usize), Some(SemanticActual::CoreInferred {
            scope, call: actual_call, formal: actual_formal,
        }) if *scope == caller && *actual_call == call && *actual_formal == formal)
        {
            return Err(bad(
                "inferred parent changes its exact original holder request",
            ));
        }
        if !matches!(
            self.definition(definition)?.body,
            SemanticDefinitionBody::Scope(_)
        ) {
            return Ok(None);
        }
        let mut active = BTreeSet::new();
        let (source, actuals) =
            self.source_parent_solution(caller, call, holder, &mut active, depth + 1)
                .map_err(SourceFailure::into_diagnostic)?;
        let recipe = actuals
            .get(formal as usize)
            .ok_or_else(|| bad("inferred parent omits its complete mixed formal"))?;
        self.recipe_actual(source, recipe, depth + 1).map(Some)
    }
}

/// Recheck a specialized parent boundary after publication has consumed its
/// Core requests. Full original provenance comes from the immutable context.
impl Eval<'_, '_, '_> {
    pub(super) fn parent_actuals(
        &mut self,
        caller: SemanticInstanceId,
        call: SemanticSiteId,
        holder: SemanticInstanceId,
    ) -> R<Vec<SemanticActual>> {
        let request = self.scope_source_instance(holder)?;
        if !matches!(
            self.definition(request.definition)?.body,
            SemanticDefinitionBody::Scope(_)
        ) {
            return Err(bad(
                "method parent boundary does not name a true original Scope",
            ));
        }
        self.fee(request.actuals.len())?;
        for (ordinal, actual) in request.actuals.iter().enumerate() {
            if !matches!(actual, SemanticActual::CoreInferred { scope, call: actual_call, formal }
                if *scope == caller && *actual_call == call && *formal == ordinal as u32)
            {
                return Err(bad(
                    "method parent boundary changes its full original request provenance",
                ));
            }
        }
        let (source, recipes) =
            self.source_parent_solution(caller, call, holder, &mut BTreeSet::new(), 0)
                .map_err(SourceFailure::into_diagnostic)?;
        if recipes.len() != request.actuals.len() {
            return Err(bad("method parent boundary changes its full mixed arity"));
        }
        self.fee(recipes.len())?;
        let mut result = Vec::new();
        for recipe in recipes.iter() {
            result.push(self.recipe_actual(source, recipe, 0)?);
        }
        Ok(result)
    }
}
