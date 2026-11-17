//! Original invocation quantifiers are anchored to complete actual input
//! occurrences, independently of the final header's binder numbering.
use super::*;
use std::collections::BTreeMap;
use std::rc::Rc;

pub(super) type InputKey = (SemanticInstanceId, SemanticDefinitionId, FunctionId, u32);
pub(super) type Inputs = Rc<BTreeMap<u32, SemanticClassExpr>>;

impl Eval<'_, '_, '_> {
    pub(super) fn invocation_input(
        &mut self,
        scope: SemanticInstanceId,
        index: u32,
    ) -> R<SemanticClassExpr> {
        let instance = self.instance(scope)?;
        let definition = self.definition(instance.definition)?;
        let (SemanticDefinitionBody::Function { signature, .. }
        | SemanticDefinitionBody::FunctionWithBody { signature, .. }) = &definition.body
        else {
            return Err(bad("invocation input has no original callable signature"));
        };
        let count = signature.invocation_lifetimes;
        if index >= count {
            return Err(bad(
                "invocation input exceeds its original complete quantifier frame",
            ));
        }
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) =
            instance.target
        else {
            return Err(unknown("invocation input needs its exact actual entry"));
        };
        let key = (scope, definition.id, function, count);
        let stable = self.module.stable_invocation_inputs();
        let cached = if stable {
            let size = self.module.invocation_inputs.borrow().len();
            self.fee(size.saturating_add(1).ilog2() as usize + 1)?;
            self.module.invocation_inputs.borrow().get(&key).cloned()
        } else {
            None
        };
        let inputs = if let Some(inputs) = cached {
            inputs
        } else {
            let size = self.module.invocation_active.borrow().len();
            self.fee(size.saturating_add(1).ilog2() as usize + 1)?;
            if !self.module.invocation_active.borrow_mut().insert(scope) {
                return Err(bad(
                    "invocation inputs have a cyclic original scope dependency",
                ));
            }
            let result = self.form_invocation_inputs(scope, function, count);
            let size = self.module.invocation_active.borrow().len();
            let removal = self.fee(size.saturating_add(1).ilog2() as usize + 1);
            self.module.invocation_active.borrow_mut().remove(&scope);
            if let Err(diagnostic) = removal {
                // The active marker is removed even when its paid cleanup
                // exhausts the request. Resource/error priority is independent
                // of an earlier exact missing-header dependency.
                self.pending_header = None;
                return Err(diagnostic);
            }
            let inputs = Rc::new(result.map_err(|mut diagnostic| {
                if let Some(left) = self.left.checked_sub(128) {
                    *self.left = left;
                    diagnostic.notes.push(format!(
                        "original invocation input origin: instance {scope:?}, definition {:?}, actual entry {function:?}, complete invocation frame {count}",
                        definition.id,
                    ));
                }
                diagnostic
            })?);
            if stable {
                let size = self.module.invocation_inputs.borrow().len();
                self.fee(size.saturating_add(1).ilog2() as usize + 1)?;
                self.module
                    .invocation_inputs
                    .borrow_mut()
                    .insert(key, inputs.clone());
            }
            inputs
        };
        self.fee(inputs.len().saturating_add(1).ilog2() as usize + 1)?;
        let image = inputs.get(&index).ok_or_else(|| {
            unknown("original invocation has no complete actual input coordinate")
        })?;
        crate::dyn_metadata::semantic_operand_snapshot(image, self.limits, self.left, 1)?;
        Ok(image.clone())
    }

    fn form_invocation_inputs(
        &mut self,
        scope: SemanticInstanceId,
        function: FunctionId,
        count: u32,
    ) -> R<BTreeMap<u32, SemanticClassExpr>> {
        if count as usize > self.limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "invocation input frame node limit exceeded",
                None,
            ));
        }
        let entry = self
            .module
            .function(function)
            .ok_or_else(|| bad("invocation input lost its actual typed entry"))?;
        let instance = self.instance(scope)?;
        let original = self.definition(instance.definition)?;
        let (SemanticDefinitionBody::Function { signature, .. }
        | SemanticDefinitionBody::FunctionWithBody { signature, .. }) = &original.body
        else {
            return Err(bad("invocation input lost its original complete signature"));
        };
        if entry.parameters.len() != signature.parameters.len() {
            return Err(bad("invocation input changes its original parameter arity"));
        }
        self.fee(entry.parameters.len())?;
        let mut inputs = BTreeMap::new();
        for (ordinal, local) in entry.parameters.iter().enumerate() {
            self.fee(1)?;
            let actual = &entry
                .locals
                .get(local.0 as usize)
                .ok_or_else(|| bad("invocation input has no actual parameter local"))?
                .ty;
            let mut probe = Eval {
                input: self.input,
                module: self.module,
                ledger: self.ledger,
                limits: self.limits,
                left: self.left,
                active: BTreeSet::new(),
                invocation_probe: Some((scope, count)),
                pending_header: None,
            };
            let draft = probe.ty(
                scope,
                &SemanticTypeRecipe::OriginalType {
                    scope: SemanticScopeRef::Ancestor(0),
                    source: SemanticOriginalTypeSource::Parameter(ordinal as u32),
                },
                Some(actual),
                0,
            );
            let pending = probe.pending_header;
            drop(probe);
            let draft = match draft {
                Ok(value) => value,
                Err(diagnostic) => {
                    // Transport the exact originating child identity through
                    // this nested query only while its tagged failure remains
                    // the actual error. Failed/pending inputs are never cached.
                    self.pending_header = if is_pending_header(&diagnostic) { pending } else { None };
                    return Err(diagnostic);
                }
            };
            let SemanticTypeValue::Resolved { ty, life, classes } = draft else {
                return Err(bad(
                    "invocation input has no complete original whole-type domain",
                ));
            };
            inference::charge_type_snapshot(&ty, Span::default(), self.limits, self.left, 1)?;
            inference::charge_type_snapshot(actual, Span::default(), self.limits, self.left, 1)?;
            if &ty != actual || life.binder_count as usize != classes.len() {
                return Err(bad(
                    "invocation input changes its complete actual type or class domain",
                ));
            }
            self.fee(classes.len())?;
            for (class, image) in classes.iter().enumerate() {
                let SemanticClassExpr::Implicit {
                    scope: SemanticScopeRef::Ancestor(u32::MAX),
                    ordinal: role,
                } = image
                else {
                    continue;
                };
                if *role >= count {
                    return Err(bad("invocation input has an unbound discovery variable"));
                }
                self.fee(life.slots.len().saturating_add(life.retained.len()))?;
                let parameter = u32::try_from(ordinal)
                    .map_err(|_| bad("invocation parameter index overflows"))?;
                let image = if let Some(slot) =
                    life.slots.iter().find(|slot| slot.binder as usize == class)
                {
                    self.fee(slot.path.len().saturating_add(1))?;
                    SemanticClassExpr::OuterOccurrence {
                        operand: SemanticOperand::Parameter(parameter),
                        path: slot
                            .path
                            .iter()
                            .copied()
                            .map(LifeTypeProjection::Place)
                            .collect(),
                        mode: slot.mode,
                    }
                } else if let Some(child) = life
                    .retained
                    .iter()
                    .find(|row| row.binder as usize == class)
                {
                    self.fee(child.path.len().saturating_add(1))?;
                    SemanticClassExpr::IntrinsicClass {
                        operand: SemanticOperand::Parameter(parameter),
                        path: child.path.clone(),
                        class: child.class,
                    }
                } else {
                    // A phantom whole-input class is still a real coordinate.
                    // It contributes no physical slot and is never dropped.
                    SemanticClassExpr::IntrinsicClass {
                        operand: SemanticOperand::Parameter(parameter),
                        path: Vec::new(),
                        class: class as u32,
                    }
                };
                self.fee(inputs.len().saturating_add(1).ilog2() as usize + 1)?;
                inputs.entry(*role).or_insert(image);
            }
        }
        self.fee(count as usize)?;
        for role in 0..count {
            if !inputs.contains_key(&role) {
                return Err(unknown(
                    "original invocation quantifier has no real input occurrence",
                ));
            }
        }
        Ok(inputs)
    }
}
