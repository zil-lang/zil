//! A specialized trait method's lexical parent is solved in the original
//! argument's source Gamma. Runtime Self images never become original facts.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

struct SourceActual {
    scope: SemanticInstanceId,
    recipe: SemanticActualRecipe,
}
impl<'a> Eval<'a, '_> {
    fn scope_source_instance(&mut self, id: SemanticInstanceId) -> R<&'a SemanticInstance> {
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
    ) -> R<SemanticActualRecipe> {
        self.node(depth)?;
        let source = self.scope_chain(value.scope, depth + 1)?;
        let destination = self.scope_chain(target, depth + 1)?;
        self.fee(
            source
                .len()
                .saturating_mul(destination.len())
                .saturating_add(1),
        )?;
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
        )
    }
    fn source_actual(
        &mut self,
        holder: SemanticInstanceId,
        ordinal: u32,
        active: &mut BTreeSet<(SemanticInstanceId, u32)>,
        depth: usize,
    ) -> R<SourceActual> {
        self.node(depth)?;
        self.fee(active.len().saturating_add(1))?;
        if !active.insert((holder, ordinal)) {
            return Err(bad("original parent argument provenance is cyclic"));
        }
        let result = (|| {
            let row = self.scope_source_instance(holder)?;
            let original = self.definition(row.definition)?;
            if row.actuals.len() != original.parameters.len() {
                return Err(bad("parent source omits complete mixed original arguments"));
            }
            let value = row
                .actuals
                .get(ordinal as usize)
                .ok_or_else(|| bad("parent argument has no full original ordinal"))?;
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
                        return Err(bad("parent source changes a full mixed inference ordinal"));
                    }
                    if matches!(original.body, SemanticDefinitionBody::Scope(_)) {
                        let (source, actuals) =
                            self.source_parent_solution(*scope, *call, holder, active, depth + 1)?;
                        let recipe = actuals
                            .into_iter()
                            .nth(ordinal as usize)
                            .ok_or_else(|| bad("parent solution omits a full mixed ordinal"))?;
                        Ok(SourceActual {
                            scope: source,
                            recipe,
                        })
                    } else {
                        let owner = self.scope_source_instance(*scope)?.definition;
                        let recipe = {
                            let proof = self.ledger.original.borrow();
                            let proof = proof
                                .as_ref()
                                .ok_or_else(|| bad("parent source has no original proof"))?;
                            let target = proof
                                .call_targets
                                .get(&(owner, *call))
                                .ok_or_else(|| bad("parent source call has no original target"))?;
                            if *target != original.id
                                && proof.providers.get(&original.id) != Some(target)
                            {
                                return Err(bad(
                                    "parent source call does not target the actual definition",
                                ));
                            }
                            proof.call_actual(owner, *call, *formal, self.limits, self.left)?
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
                            )?;
                            recipe.as_ref().clone()
                        }
                        SemanticCoreTypeSource::OriginalExpression { source, .. } => {
                            crate::dyn_metadata::semantic_operand_snapshot(
                                source,
                                self.limits,
                                self.left,
                                1,
                            )?;
                            SemanticTypeRecipe::OriginalType {
                                scope: SemanticScopeRef::Ancestor(0),
                                source: source.clone(),
                            }
                        }
                        _ => {
                            return Err(unknown(
                                "parent source needs a complete original type recipe",
                            ));
                        }
                    };
                    self.source_specialize(
                        *scope,
                        &SemanticActualRecipe::Type(recipe),
                        active,
                        depth + 1,
                    )
                }
                _ => Err(unknown(
                    "parent inference cannot reconstruct original arguments from resolved runtime images",
                )),
            }
        })();
        active.remove(&(holder, ordinal));
        result
    }
    fn source_specialize(
        &mut self,
        scope: SemanticInstanceId,
        value: &SemanticActualRecipe,
        active: &mut BTreeSet<(SemanticInstanceId, u32)>,
        depth: usize,
    ) -> R<SourceActual> {
        self.node(depth)?;
        let ancestry = self.scope_chain(scope, depth + 1)?;
        let mut arguments = Vec::new();
        let mut destination = None;
        for id in &ancestry {
            let row = self.scope_source_instance(*id)?;
            self.fee(row.actuals.len().saturating_add(1))?;
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
        self.fee(arguments.len())?;
        for frame in &arguments {
            let mut values = Vec::new();
            self.fee(frame.len())?;
            for actual in frame {
                values.push(self.source_relocate(actual, destination, depth + 1)?);
            }
            mapped.push(values);
        }
        let destination_chain = self.scope_chain(destination, depth + 1)?;
        self.fee(
            ancestry
                .len()
                .saturating_mul(destination_chain.len())
                .saturating_add(mapped.len()),
        )?;
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
        )?;
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
    ) -> R<(SemanticInstanceId, Vec<SemanticActualRecipe>)> {
        self.node(depth)?;
        let caller_definition = self.scope_source_instance(caller)?.definition;
        let holder_definition = self.scope_source_instance(holder)?.definition;
        let function = self.function(caller)?;
        self.fee(self.input.sites.len())?;
        let mut sites = self
            .input
            .sites
            .iter()
            .filter(|row| row.instance == caller && row.recipe == call);
        let site = sites
            .next()
            .ok_or_else(|| bad("parent inference has no actual call site"))?;
        if sites.next().is_some() {
            return Err(bad("parent inference has multiple actual call sites"));
        }
        let SemanticSiteAddress::Typed {
            block,
            instruction,
            edge: SemanticSiteEdge::After,
        } = site.at
        else {
            return Err(unknown(
                "parent inference awaits its exact emitted call boundary",
            ));
        };
        self.fee(function.blocks.len())?;
        let op = function
            .blocks
            .iter()
            .find(|row| row.id == block)
            .and_then(|row| row.instructions.get(instruction as usize))
            .ok_or_else(|| bad("parent inference call leaves the actual CFG"))?;
        let (InstructionKind::Call {
            function: target, ..
        }
        | InstructionKind::SuspendCall {
            function: target, ..
        }) = &op.kind
        else {
            return Err(unknown(
                "parent inference requires its true direct method opcode",
            ));
        };
        let count = self.input.instances.len();
        self.fee(count.saturating_mul(count.saturating_add(1).ilog2() as usize + 1))?;
        let mut ids = BTreeSet::new();
        for row in &self.input.instances {
            ids.insert(row.id);
        }
        let mut provider = None;
        for id in ids {
            let row = self.instance(id)?;
            if row.parent == Some(holder)
                && row.target
                    == SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(*target))
            {
                if provider.replace(row.definition).is_some() {
                    return Err(bad(
                        "parent inference has several actual provider instances for its exact holder",
                    ));
                }
            }
        }
        let provider = provider.ok_or_else(|| {
            bad("parent inference has no actual provider with the exact lexical holder")
        })?;
        let (receiver, application) = {
            let proof = self.ledger.original.borrow();
            let method = proof
                .as_ref()
                .and_then(|proof| proof.method_calls.get(&(caller_definition, call)))
                .ok_or_else(|| {
                    bad("parent inference lacks its original method receiver theorem")
                })?;
            crate::dyn_metadata::semantic_operand_snapshot(
                &method.self_type,
                self.limits,
                self.left,
                1,
            )?;
            if let Some(application) = &method.application {
                crate::dyn_metadata::semantic_operand_snapshot(
                    application,
                    self.limits,
                    self.left,
                    1,
                )?;
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
            self.fee(application.arguments.len())?;
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
        let source_owner = self.scope_source_instance(source.scope)?.definition;
        let actuals = {
            let proof = self.ledger.original.borrow();
            proof
                .as_ref()
                .ok_or_else(|| bad("parent specialization lost its original proof"))?
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
                )?
        };
        Ok((source.scope, actuals))
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
            self.source_parent_solution(caller, call, holder, &mut active, depth + 1)?;
        let recipe = actuals
            .get(formal as usize)
            .ok_or_else(|| bad("inferred parent omits its complete mixed formal"))?;
        self.recipe_actual(source, recipe, depth + 1).map(Some)
    }
}
