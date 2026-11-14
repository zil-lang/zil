//! Explicit original-parameter result boundaries. The original caller supplies
//! the actual recipes; the actual emitted call selects its real callee entry.
//! Callee-local regions are deliberately not transported by this pass.
use super::*;
impl Eval<'_, '_> {
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
        if boundary.caller != caller_definition {
            return Err(bad("opaque result changes its original caller definition"));
        }
        if scope != caller {
            return Err(unknown(
                "ancestor opaque result requires its actual sealed environment boundary",
            ));
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
        let mut result: Option<SemanticTypeValue> = None;
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
            if let Some(prior) = &result {
                crate::dyn_metadata::semantic_operand_snapshot(prior, self.limits, self.left, 1)?;
                crate::dyn_metadata::semantic_operand_snapshot(&value, self.limits, self.left, 1)?;
                if prior != &value {
                    return Err(bad(
                        "actual callee contexts disagree on the complete original result",
                    ));
                }
            } else {
                result = Some(value);
            }
        }
        result
            .map(Some)
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
        module,
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
