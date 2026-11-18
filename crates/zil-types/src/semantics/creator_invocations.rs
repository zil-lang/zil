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
        let mut count = signature.invocation_lifetimes;
        if count == 0 && definition.kind == SemanticDefinitionKind::Closure {
            let ledger = self.ledger;
            let proof = ledger.original.borrow();
            let proof = proof.as_ref().filter(|proof| proof.complete.contains(&definition.id))
                .or_else(|| ledger.frozen.as_deref().map(|source| &source.original));
            if let Some(proof) = proof {
                self.fee(proof.contextual_code_frames.len().saturating_add(1).ilog2() as usize + 1)?;
                if let Some(frame) = proof.contextual_code_frames.get(&definition.id) {
                    count = *frame;
                }
            }
        }
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

    /// A code address stores no input borrow. Its own complete flat
    /// parameter/result relation is carried by its invocation header.
    pub(super) fn borrowed_code_type(
        &mut self,
        scope: SemanticInstanceId,
        signature: &SemanticSignatureRecipe,
        depth: usize,
    ) -> R<SemanticTypeValue> {
        if signature.call_kind != CallKind::Direct || signature.bound_lifetimes != 0 {
            return Err(unknown("borrowed code needs its complete quantified Direct signature"));
        }
        self.fee(signature.parameters.len().saturating_add(1).saturating_mul(4))?;
        let mut parameters = Vec::with_capacity(signature.parameters.len());
        let mut parameter_lives = Vec::with_capacity(signature.parameters.len());
        for parameter in &signature.parameters {
            let (ty, role) = self.borrowed_code_endpoint(
                scope, &parameter.ty, parameter.mode, depth + 1,
            )?;
            parameters.push(ty);
            parameter_lives.push(role);
        }
        let (result, result_life) = self.borrowed_code_endpoint(
            scope, &signature.result, signature.result_mode, depth + 1,
        )?;
        let life = StorageLifeContract::from_flat(FunctionLifeContract {
            binder_count: signature.invocation_lifetimes,
            parameter_lives,
            result_life,
        }).ok_or_else(|| bad("borrowed code exceeds its complete parameter arity"))?;
        self.fee(crate::borrowed_fp::metadata_work(&life))?;
        crate::borrowed_fp::relation(&parameters, &result, &life, Span::default())?;
        Ok(resolved(Type::BorrowedFunctionPointer {
            parameters,
            result: Box::new(result),
            life,
        }, empty(), Vec::new()))
    }

    fn borrowed_code_endpoint(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticTypeRecipe,
        mode: PassingMode,
        depth: usize,
    ) -> R<(Type, Option<u32>)> {
        if !matches!(mode, PassingMode::Infer | PassingMode::Copy) {
            return Err(unknown("borrowed code needs its complete passing mode"));
        }
        let address = match recipe {
            SemanticTypeRecipe::Address {
                mutable,
                lifetime: SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Invocation(role)),
                inner,
            } => Some((*mutable, *role, inner.as_ref())),
            SemanticTypeRecipe::Address { .. } => {
                return Err(unknown("borrowed code requires its own invocation-local address"));
            }
            _ => None,
        };
        let value = self.ty(scope, address.map_or(recipe, |(_, _, inner)| inner), None, depth + 1)?;
        let SemanticTypeValue::Resolved { ty, life, classes } = value else {
            return Err(bad("borrowed code endpoint has no complete type"));
        };
        if life.binder_count != 0 || !classes.is_empty() {
            return Err(unknown("borrowed code endpoint requires its complete nested class domain"));
        }
        if mode == PassingMode::Copy && matches!(address, Some((true, _, _))) {
            return Err(bad("borrowed code cannot copy an exclusive input or result"));
        }
        Ok(match address {
            Some((mutable, role, _)) => (Type::Address { mutable, inner: Box::new(ty) }, Some(role)),
            None => (ty, None),
        })
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
