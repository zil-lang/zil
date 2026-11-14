//! Explicit original-parameter result boundaries. The original caller supplies
//! the actual recipes; the actual emitted call selects its real callee entry.
//! Callee-local regions are deliberately not transported by this pass.
use super::*;

#[derive(PartialEq, Eq)]
struct ClosedContext {
    definition: SemanticDefinitionId,
    arguments: Vec<SemanticActual>,
    environment: Vec<SemanticActual>,
}

impl Eval<'_, '_> {
    /// A monomorphic opaque with an independently proved closed result has a
    /// fixed defining type, independent of runtime value arguments. Interpret
    /// every emitted defining world; never select a convenient caller image.
    pub(super) fn closed_opaque_definition(
        &mut self,
        _caller: SemanticInstanceId,
        recipe: &SemanticTypeRecipe,
        hint: Option<&Type>,
        depth: usize,
    ) -> R<Option<SemanticTypeValue>> {
        self.node(depth)?;
        let Some((definition, result_recipe)) = ({
            let proof = self.ledger.original.borrow();
            let proof = proof
                .as_ref()
                .ok_or_else(|| bad("closed opaque lacks its original all-return proof"))?;
            proof.closed_opaque_result(self.input, recipe, self.limits, self.left)?
        }) else {
            return Ok(None);
        };
        self.fee(self.input.instances.len())?;
        let mut result: Option<(SemanticTypeValue, ClosedContext)> = None;
        for instance in &self.input.instances {
            if instance.definition != definition {
                continue;
            }
            if !matches!(
                instance.target,
                SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(_))
            ) {
                continue;
            }
            if !instance.actuals.is_empty()
                || instance.parent.is_some()
                || instance.environment.is_some()
                || instance.self_type.is_some()
                || instance.lifetime_instantiation != SemanticLifetimeInstantiation::Rigid
            {
                return Err(bad(
                    "closed opaque defining world changes its original monomorphic context",
                ));
            }
            let value = self.ty(instance.id, &result_recipe, None, depth + 1)?;
            let context =
                self.closed_result_context(instance.id, &result_recipe, &value, depth + 1)?;
            if let Some((previous, previous_context)) = &result {
                crate::dyn_metadata::semantic_operand_snapshot(
                    previous,
                    self.limits,
                    self.left,
                    1,
                )?;
                crate::dyn_metadata::semantic_operand_snapshot(&value, self.limits, self.left, 1)?;
                self.fee(
                    previous_context
                        .arguments
                        .len()
                        .saturating_add(context.arguments.len())
                        .saturating_add(previous_context.environment.len())
                        .saturating_add(context.environment.len())
                        .saturating_add(1),
                )?;
                if previous != &value || previous_context != &context {
                    return Err(bad(
                        "closed opaque defining worlds disagree in complete result type or original environment",
                    ));
                }
            } else {
                result = Some((value, context));
            }
        }
        let Some((value, _)) = result else {
            return Ok(None);
        };
        if let Some(hint) = hint {
            let SemanticTypeValue::Resolved { ty, .. } = &value else {
                return Err(bad("closed opaque result remains unresolved"));
            };
            inference::charge_type_snapshot(ty, Span::default(), self.limits, self.left, 1)?;
            inference::charge_type_snapshot(hint, Span::default(), self.limits, self.left, 1)?;
            if ty != hint {
                return Err(bad(
                    "closed opaque original defining type differs from actual emitted type",
                ));
            }
        }
        Ok(Some(value))
    }
    fn opaque_actuals_match(
        &mut self,
        caller: SemanticInstanceId,
        call: SemanticSiteId,
        candidate: &SemanticInstance,
        expected: &[SemanticActual],
        depth: usize,
    ) -> R<bool> {
        self.fee(candidate.actuals.len().saturating_add(expected.len()))?;
        if candidate.actuals.len() != expected.len() {
            return Ok(false);
        }
        for (ordinal, (claim, expected)) in candidate.actuals.iter().zip(expected).enumerate() {
            if let SemanticActual::CoreInferred {
                scope,
                call: actual,
                formal,
            } = claim
                && (*scope != caller || *actual != call || *formal != ordinal as u32)
            {
                return Ok(false);
            }
            let actual = self.holder_actual(candidate.id, ordinal as u32, claim, depth + 1)?;
            match (expected, actual) {
                (SemanticActual::Life(expected), SemanticActual::Life(actual)) => {
                    if matches!(claim, SemanticActual::CoreInferred { .. }) {
                        crate::dyn_metadata::semantic_operand_snapshot(
                            expected,
                            self.limits,
                            self.left,
                            1,
                        )?;
                        crate::dyn_metadata::semantic_operand_snapshot(
                            &actual,
                            self.limits,
                            self.left,
                            1,
                        )?;
                        if expected != &actual {
                            return Ok(false);
                        }
                    } else {
                        if candidate.lifetime_instantiation != SemanticLifetimeInstantiation::Rigid
                        {
                            return Err(unknown(
                                "opaque result requires its actual rigid callee Life domain",
                            ));
                        }
                        let canonical = self.canonical_formal(
                            candidate.id,
                            &SemanticFormalRef {
                                scope: SemanticScopeRef::Ancestor(0),
                                ordinal: ordinal as u32,
                            },
                        )?;
                        if actual != canonical {
                            return Ok(false);
                        }
                    }
                }
                (
                    SemanticActual::Type(SemanticTypeValue::Resolved {
                        ty: expected,
                        life: expected_life,
                        classes: expected_classes,
                    }),
                    SemanticActual::Type(SemanticTypeValue::Resolved {
                        ty: actual,
                        life: actual_life,
                        classes: actual_classes,
                    }),
                ) => {
                    inference::charge_type_snapshot(
                        expected,
                        Span::default(),
                        self.limits,
                        self.left,
                        1,
                    )?;
                    inference::charge_type_snapshot(
                        &actual,
                        Span::default(),
                        self.limits,
                        self.left,
                        1,
                    )?;
                    let _ = self.copy_shape(expected_life)?;
                    let _ = self.copy_shape(&actual_life)?;
                    if expected != &actual
                        || expected_life != &actual_life
                        || expected_classes.len() != expected_life.binder_count as usize
                        || actual_classes.len() != actual_life.binder_count as usize
                    {
                        return Ok(false);
                    }
                    self.fee(actual_classes.len())?;
                    for (class, image) in actual_classes.iter().enumerate() {
                        crate::dyn_metadata::semantic_operand_snapshot(
                            image,
                            self.limits,
                            self.left,
                            1,
                        )?;
                        let canonical = SemanticClassExpr::IntrinsicClass {
                            operand: SemanticOperand::TypeArgument {
                                scope: SemanticScopeRef::Ancestor(0),
                                ordinal: ordinal as u32,
                            },
                            path: Vec::new(),
                            class: class as u32,
                        };
                        let expected = &expected_classes[class];
                        crate::dyn_metadata::semantic_operand_snapshot(
                            expected,
                            self.limits,
                            self.left,
                            1,
                        )?;
                        if image != &canonical && image != expected {
                            return Ok(false);
                        }
                    }
                }
                (SemanticActual::Const { .. }, actual @ SemanticActual::Const { .. }) => {
                    crate::dyn_metadata::semantic_operand_snapshot(
                        expected,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    crate::dyn_metadata::semantic_operand_snapshot(
                        &actual,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    if expected != &actual {
                        return Ok(false);
                    }
                }
                _ => return Ok(false),
            }
        }
        Ok(true)
    }
    fn explicit_result_images(
        &mut self,
        value: &mut SemanticTypeValue,
        actuals: &[SemanticActual],
    ) -> R<()> {
        let SemanticTypeValue::Resolved { life, classes, .. } = value else {
            return Err(bad("callee result retains an unresolved whole type"));
        };
        self.fee(classes.len())?;
        if classes.len() != life.binder_count as usize {
            return Err(bad("callee result omits an intrinsic class"));
        }
        for image in classes {
            let selected = match image {
                SemanticClassExpr::Formal {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal,
                } => {
                    let Some(SemanticActual::Life(value)) = actuals.get(*ordinal as usize) else {
                        return Err(bad("callee result Life selects another mixed actual kind"));
                    };
                    value
                }
                SemanticClassExpr::IntrinsicClass {
                    operand:
                        SemanticOperand::TypeArgument {
                            scope: SemanticScopeRef::Ancestor(0),
                            ordinal,
                        },
                    path,
                    class,
                } if path.is_empty() => {
                    let Some(SemanticActual::Type(SemanticTypeValue::Resolved {
                        life,
                        classes,
                        ..
                    })) = actuals.get(*ordinal as usize)
                    else {
                        return Err(bad("callee result class selects another mixed Type actual"));
                    };
                    if classes.len() != life.binder_count as usize {
                        return Err(bad("callee whole Type actual omits an intrinsic class"));
                    }
                    classes.get(*class as usize).ok_or_else(|| {
                        bad("callee result exceeds the full intrinsic Type domain")
                    })?
                }
                _ => {
                    return Err(unknown(
                        "opaque result requires a separately proved callee-local or ancestor class boundary",
                    ));
                }
            };
            crate::dyn_metadata::semantic_operand_snapshot(selected, self.limits, self.left, 1)?;
            *image = selected.clone();
        }
        Ok(())
    }
    fn charge_closed_context(&mut self, context: Option<&ClosedContext>) -> R<()> {
        if let Some(context) = context {
            self.fee(
                context
                    .arguments
                    .len()
                    .saturating_add(context.environment.len()),
            )?;
            for actual in context.arguments.iter().chain(&context.environment) {
                crate::dyn_metadata::semantic_operand_snapshot(actual, self.limits, self.left, 1)?;
            }
        }
        Ok(())
    }

    fn closed_context_actual(&mut self, value: &SemanticActual) -> R<()> {
        let shape = match value {
            SemanticActual::Type(SemanticTypeValue::Resolved { ty, life, classes })
            | SemanticActual::Const {
                ty: SemanticTypeValue::Resolved { ty, life, classes },
                ..
            } => {
                if !classes.is_empty() {
                    return Err(unknown(
                        "multiple opaque origins retain a free actual class",
                    ));
                }
                self.module.ordinary_zero(ty, self.limits, self.left)?;
                life
            }
            _ => {
                return Err(unknown(
                    "multiple opaque origins require a completely closed actual context",
                ));
            }
        };
        let _ = self.copy_shape(shape)?;
        if shape.binder_count != 0
            || !shape.slots.is_empty()
            || !shape.retained.is_empty()
            || !shape.nominal_arguments.is_empty()
        {
            return Err(unknown(
                "multiple opaque origins retain a nonempty actual domain",
            ));
        }
        Ok(())
    }

    fn closed_result_context(
        &mut self,
        creator: SemanticInstanceId,
        recipe: &SemanticTypeRecipe,
        value: &SemanticTypeValue,
        depth: usize,
    ) -> R<ClosedContext> {
        self.node(depth)?;
        let SemanticTypeRecipe::Closure {
            definition,
            arguments,
            environment,
        } = recipe
        else {
            return Err(unknown(
                "multiple opaque origins need their complete original closure result",
            ));
        };
        let SemanticTypeValue::Resolved {
            ty: Type::Closure(id),
            life,
            classes,
        } = value
        else {
            return Err(bad(
                "closed original closure result has another actual carrier",
            ));
        };
        let _ = self.copy_shape(life)?;
        if life.binder_count != 0
            || !classes.is_empty()
            || !life.slots.is_empty()
            || !life.retained.is_empty()
            || !life.nominal_arguments.is_empty()
        {
            return Err(unknown(
                "multiple opaque origins have a free actual result domain",
            ));
        }
        self.module
            .ordinary_zero(&Type::Closure(*id), self.limits, self.left)?;
        self.fee(
            self.module
                .closures
                .len()
                .saturating_add(self.input.instances.len()),
        )?;
        let closure = self
            .module
            .closures
            .iter()
            .find(|row| row.id == *id)
            .ok_or_else(|| bad("closed result has no actual closure definition"))?;
        let mut children = self.input.instances.iter().filter(|row| {
            row.parent == Some(creator)
                && row.definition == *definition
                && row.target
                    == SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(
                        closure.function,
                    ))
                && matches!(&row.environment, Some(SemanticEnvironmentInstance {
                    closure: SemanticClosureTarget::Typed(actual), ..
                }) if actual == id)
        });
        let child = children
            .next()
            .ok_or_else(|| bad("closed result lacks its actual creator environment"))?;
        if children.next().is_some() {
            return Err(bad("closed result has ambiguous creator environments"));
        }
        let actual_environment = &child
            .environment
            .as_ref()
            .expect("selected environment")
            .actuals;
        self.fee(arguments.len().saturating_add(environment.len()))?;
        if arguments.len() != child.actuals.len() || environment.len() != actual_environment.len() {
            return Err(bad("closed result omits a complete mixed context"));
        }
        let mut checked_arguments = Vec::new();
        for (ordinal, (recipe, claim)) in arguments.iter().zip(&child.actuals).enumerate() {
            let expected = self.recipe_actual(creator, recipe, depth + 1)?;
            let actual = self.holder_actual(child.id, ordinal as u32, claim, depth + 1)?;
            self.closed_context_actual(&expected)?;
            self.closed_context_actual(&actual)?;
            crate::dyn_metadata::semantic_operand_snapshot(&expected, self.limits, self.left, 1)?;
            crate::dyn_metadata::semantic_operand_snapshot(&actual, self.limits, self.left, 1)?;
            if expected != actual {
                return Err(bad("closed result changes its original mixed actual"));
            }
            checked_arguments.push(expected);
        }
        let mut checked_environment = Vec::new();
        for (recipe, claim) in environment.iter().zip(actual_environment) {
            let expected = self.recipe_actual(creator, recipe, depth + 1)?;
            let actual = self.actual(creator, claim, depth + 1)?;
            self.closed_context_actual(&expected)?;
            self.closed_context_actual(&actual)?;
            crate::dyn_metadata::semantic_operand_snapshot(&expected, self.limits, self.left, 1)?;
            crate::dyn_metadata::semantic_operand_snapshot(&actual, self.limits, self.left, 1)?;
            if expected != actual {
                return Err(bad("closed result changes its original sealed environment"));
            }
            checked_environment.push(expected);
        }
        Ok(ClosedContext {
            definition: *definition,
            arguments: checked_arguments,
            environment: checked_environment,
        })
    }

    pub(super) fn multiple_inferred_actual(
        &mut self,
        caller: SemanticInstanceId,
        call: SemanticSiteId,
        formal: u32,
        holder: Option<SemanticInstanceId>,
        depth: usize,
    ) -> R<Option<SemanticActual>> {
        self.node(depth)?;
        let owner = self.instance(caller)?.definition;
        let holder = holder
            .map(|id| self.instance(id).map(|row| row.definition))
            .transpose()?;
        self.fee(self.input.definitions.len())?;
        let boundaries = {
            let proof = self.ledger.original.borrow();
            let proof = proof
                .as_ref()
                .filter(|proof| proof.complete.contains(&owner))
                .or_else(|| self.ledger.frozen.as_deref().map(|source| &source.original))
                .ok_or_else(|| bad("inferred opaque origins lack their complete original proof"))?;
            if let Some(holder) = holder {
                if proof.call_targets.get(&(owner, call)) != Some(&holder) {
                    // Parent Scope actuals have a separate, checked receiver solution.
                    return Ok(None);
                }
            }
            let program = if self.input.definitions.iter().any(|row| row.id == owner) {
                self.input
            } else {
                &self
                    .ledger
                    .frozen
                    .as_deref()
                    .ok_or_else(|| {
                        bad("inferred opaque origins lost their original definition forest")
                    })?
                    .semantic
            };
            proof.call_actual_boundaries(program, owner, call, formal, self.limits, self.left)?
        };
        let Some(boundaries) = boundaries else {
            return Ok(None);
        };
        self.fee(boundaries.len())?;
        if boundaries.len() < 2 {
            return Err(bad(
                "multiple opaque origins omit their full original source set",
            ));
        }
        let mut result: Option<(SemanticTypeValue, Option<ClosedContext>)> = None;
        for boundary in &boundaries {
            let (value, context) =
                self.actual_result_boundary(caller, boundary, None, true, depth + 1)?;
            if let Some((prior, prior_context)) = &result {
                crate::dyn_metadata::semantic_operand_snapshot(prior, self.limits, self.left, 1)?;
                crate::dyn_metadata::semantic_operand_snapshot(&value, self.limits, self.left, 1)?;
                self.charge_closed_context(prior_context.as_ref())?;
                self.charge_closed_context(context.as_ref())?;
                if prior != &value || prior_context != &context {
                    return Err(bad(
                        "original opaque origins disagree on their complete actual result and context",
                    ));
                }
            } else {
                result = Some((value, context));
            }
        }
        let (value, _) = result.ok_or_else(|| bad("inferred opaque source set is empty"))?;
        Ok(Some(SemanticActual::Type(value)))
    }

    pub(super) fn original_result_boundary(
        &mut self,
        scope: SemanticInstanceId,
        selector: SemanticScopeRef,
        source: &SemanticOriginalTypeSource,
        hint: Option<&Type>,
        depth: usize,
    ) -> R<Option<SemanticTypeValue>> {
        let caller = self.ancestor(scope, selector, depth + 1)?;
        let caller_definition = self.instance(caller)?.definition;
        let boundary = {
            let proof = self.ledger.original.borrow();
            let proof = proof
                .as_ref()
                .ok_or_else(|| bad("opaque result lacks its checked original caller"))?;
            proof.call_result_boundary(
                caller_definition,
                caller_definition,
                source,
                self.limits,
                self.left,
            )?
        };
        let Some(boundary) = boundary else {
            return Ok(None);
        };
        if scope != caller {
            return Err(unknown(
                "ancestor opaque result requires its actual sealed environment boundary",
            ));
        }
        self.actual_result_boundary(caller, &boundary, hint, false, depth + 1)
            .map(|(value, _)| Some(value))
    }

    fn actual_result_boundary(
        &mut self,
        caller: SemanticInstanceId,
        boundary: &super::super::original::CalleeResultBoundary,
        hint: Option<&Type>,
        closed: bool,
        depth: usize,
    ) -> R<(SemanticTypeValue, Option<ClosedContext>)> {
        self.node(depth)?;
        if boundary.caller != self.instance(caller)?.definition {
            return Err(bad("opaque result changes its original caller definition"));
        }
        let function = self.function(caller)?;
        self.fee(self.input.sites.len())?;
        let mut sites = self
            .input
            .sites
            .iter()
            .filter(|site| site.instance == caller && site.recipe == boundary.call);
        let site = sites
            .next()
            .ok_or_else(|| bad("original result call has no actual emitted site"))?;
        if sites.next().is_some() {
            return Err(bad("original result call has multiple actual sites"));
        }
        let SemanticSiteAddress::Typed {
            block,
            instruction,
            edge: SemanticSiteEdge::After,
        } = site.at
        else {
            return Err(bad(
                "opaque result call is not its actual post-call boundary",
            ));
        };
        self.fee(function.blocks.len())?;
        let op = function
            .blocks
            .iter()
            .find(|row| row.id == block)
            .and_then(|row| row.instructions.get(instruction as usize))
            .ok_or_else(|| bad("opaque result call leaves its actual emitted CFG"))?;
        let (InstructionKind::Call {
            function: target, ..
        }
        | InstructionKind::SuspendCall {
            function: target, ..
        }
        | InstructionKind::ForeignCall {
            function: target, ..
        }) = &op.kind
        else {
            return Err(unknown(
                "opaque result needs its exact original direct-call target",
            ));
        };
        let actual_result = op
            .result
            .and_then(|id| function.values.get(id.0 as usize))
            .ok_or_else(|| bad("opaque result call has no actual complete result SSA"))?;
        if let Some(hint) = hint {
            inference::charge_type_snapshot(
                actual_result,
                Span::default(),
                self.limits,
                self.left,
                1,
            )?;
            inference::charge_type_snapshot(hint, Span::default(), self.limits, self.left, 1)?;
            if hint != actual_result {
                return Err(bad(
                    "original result alias changes the actual call's whole result type",
                ));
            }
        }
        let hint = Some(actual_result);
        let mut actuals = Vec::new();
        self.fee(boundary.actuals.len())?;
        for recipe in &boundary.actuals {
            actuals.push(self.recipe_actual(caller, recipe, depth + 1)?);
        }
        self.fee(self.input.instances.len())?;
        let mut result: Option<(SemanticTypeValue, Option<ClosedContext>)> = None;
        for candidate in &self.input.instances {
            if candidate.definition != boundary.callee
                || candidate.target
                    != SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(*target))
            {
                continue;
            }
            if !self.opaque_actuals_match(caller, boundary.call, candidate, &actuals, depth + 1)? {
                continue;
            }
            let mut value = self.ty(candidate.id, &boundary.result, hint, depth + 1)?;
            self.explicit_result_images(&mut value, &actuals)?;
            let context = if closed {
                Some(self.closed_result_context(
                    candidate.id,
                    &boundary.result,
                    &value,
                    depth + 1,
                )?)
            } else {
                None
            };
            if let Some((prior, prior_context)) = &result {
                crate::dyn_metadata::semantic_operand_snapshot(prior, self.limits, self.left, 1)?;
                crate::dyn_metadata::semantic_operand_snapshot(&value, self.limits, self.left, 1)?;
                self.charge_closed_context(prior_context.as_ref())?;
                self.charge_closed_context(context.as_ref())?;
                if prior != &value || prior_context != &context {
                    return Err(bad(
                        "actual callee contexts disagree on the complete original result",
                    ));
                }
            } else {
                result = Some((value, context));
            }
        }
        result
            .ok_or_else(|| bad("opaque result call has no complete matching actual callee context"))
    }
}

/// Evaluate only the result of this exact original and emitted call. This is
/// an untrusted whole-type proposal; World still proves the actual call/return
/// class flow independently of these result images.
pub(crate) fn call_result_at(
    program: &SemanticModule,
    module: &TypedModule,
    ledger: &Ledger,
    scope: SemanticInstanceId,
    site: SemanticSiteId,
    hint: &Type,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticTypeValue> {
    let mut eval = Eval {
        input: program,
        module: &ActualDirectory::from_module(module),
        ledger,
        limits,
        left,
        active: BTreeSet::new(),
    };
    let owner = eval.instance(scope)?.definition;
    let original = eval.definition(owner)?;
    let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &original.body else {
        return Err(bad(
            "opaque call result lacks its complete original operation body",
        ));
    };
    eval.fee(site_nodes.len())?;
    let mut found = site_nodes.iter().filter(|row| row.site == site);
    let row = found
        .next()
        .ok_or_else(|| bad("opaque call result has no original node relationship"))?;
    if found.next().is_some() {
        return Err(bad(
            "opaque result site has multiple original operation nodes",
        ));
    }
    let SemanticOriginalNode::Expression(node) = row.node else {
        return Err(bad(
            "opaque result site does not select an original expression",
        ));
    };
    eval.original_result_boundary(
        scope,
        SemanticScopeRef::Ancestor(0),
        &SemanticOriginalTypeSource::Expression(node),
        Some(hint),
        0,
    )?
    .ok_or_else(|| bad("original call has no complete opaque result boundary"))
}
