//! A call through an abstract F keeps its original subject and source parameter
//! identity. Its declaration premise is not an invented function definition.
use super::*;

#[derive(Clone)]
pub(super) struct AbstractCall {
    parameter: u32,
    pub(super) subject: Term,
    receiver: Term,
    signature_owner: SemanticDefinitionId,
    pub(super) signature: SemanticSignatureRecipe,
    kind: CallableKind,
    parameters: Vec<Term>,
    result: Term,
}
pub(in crate::semantic) struct AbstractBoundary {
    pub(in crate::semantic) parameter: u32,
    pub(in crate::semantic) subject: crate::semantic::value::Value,
    pub(in crate::semantic) receiver: crate::semantic::value::Value,
    pub(in crate::semantic) signature_owner: SemanticDefinitionId,
    pub(in crate::semantic) signature: SemanticSignatureRecipe,
    pub(in crate::semantic) kind: CallableKind,
    pub(in crate::semantic) parameters: Vec<crate::semantic::value::Value>,
    pub(in crate::semantic) result: crate::semantic::value::Value,
}
impl Solver<'_, '_> {
    pub(super) fn callable_premise(
        &self,
        owner: SemanticDefinitionId,
        ordinal: u32,
        b: &mut Budget,
    ) -> CompileResult<(CallableKind, SemanticSignatureRecipe)> {
        let declaration = self.inventory.definition(owner, b)?;
        let Some(SemanticFormal::Type { requirements }) =
            declaration.parameters.get(ordinal as usize)
        else {
            return Err(fail(
                "abstract callable changes its original Type formal kind",
            ));
        };
        let mut selected: Option<(CallableKind, &SemanticSignatureRecipe)> = None;
        for requirement in requirements.iter().chain(&declaration.requirements) {
            b.spend(1, WORK)?;
            let SemanticRequirementRecipe::Callable {
                ty: SemanticTypeRecipe::Formal(reference),
                kind,
                signature,
            } = requirement
            else {
                continue;
            };
            if self.original_scope(owner, &reference.scope, b)? != owner
                || reference.ordinal != ordinal
            {
                continue;
            }
            if let Some((old_kind, old)) = selected {
                crate::dyn_metadata::recipe(old, b)?;
                crate::dyn_metadata::recipe(signature.as_ref(), b)?;
                if old_kind != *kind || old != signature.as_ref() {
                    return Err(unknown(
                        "abstract callable has multiple roles requiring its complete result-role selection",
                    ));
                }
            } else {
                selected = Some((*kind, signature));
            }
        }
        let (kind, signature) = selected.ok_or_else(|| {
            fail("abstract call lacks a declared callable premise for its actual subject")
        })?;
        crate::dyn_metadata::recipe(signature, b)?;
        Ok((kind, signature.clone()))
    }

    fn invocation_scheme(
        &mut self,
        context: &Frame,
        signature: &SemanticSignatureRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<(Vec<Term>, Term)> {
        self.step(depth, b)?;
        if signature.bound_lifetimes != 0 {
            return Err(unknown(
                "abstract call needs its complete quantified invocation proof",
            ));
        }
        let mut lifetimes = Vec::new();
        b.spend(signature.invocation_lifetimes as usize, WORK)?;
        for _ in 0..signature.invocation_lifetimes {
            // These variables exist only for this real original invocation.
            // Solving them never mutates a rigid original or a retained class.
            let life = self.term(Kind::LifeVariable(None), b)?;
            Self::push(&mut lifetimes, life, b)?;
        }
        Self::push(
            &mut self.role_frames,
            roles::RoleFrame {
                owner: context.definition,
                lifetimes,
                implicits: BTreeMap::new(),
            },
            b,
        )?;
        let result: CompileResult<(Vec<Term>, Term)> = (|| {
            let mut parameters = Vec::new();
            for parameter in &signature.parameters {
                if !matches!(parameter.mode, PassingMode::Infer | PassingMode::Copy) {
                    return Err(unknown(
                        "abstract call needs its complete resource argument mode",
                    ));
                }
                let parameter = self.recipe(context, &parameter.ty, b, depth + 1)?;
                Self::push(&mut parameters, parameter, b)?;
            }
            let result = self.recipe(context, &signature.result, b, depth + 1)?;
            Ok((parameters, result))
        })();
        self.role_frames
            .pop()
            .ok_or_else(|| fail("abstract invocation lost its own lifetime frame"))?;
        result
    }

    pub(super) fn finish_abstract_call(
        &mut self,
        owner: SemanticDefinitionId,
        node: NodeId,
        b: &mut Budget,
    ) -> CompileResult<()> {
        b.lookup(self.state.abstract_calls.len(), WORK)?;
        let Some(call) = self.state.abstract_calls.get(&(owner, node)) else {
            return Ok(());
        };
        let (result, receiver) = (call.result, call.receiver);
        if let Kind::Address { life, .. } = self.state.arena[self.root(result, b)?] {
            if matches!(
                self.state.arena[self.root(life, b)?],
                Kind::LifeVariable(None)
            ) {
                let Kind::Address { life: head, .. } = self.state.arena[self.root(receiver, b)?]
                else {
                    return Err(fail(
                        "short callable result has no actual original receiver head",
                    ));
                };
                // Only an otherwise-unbound outer result invocation receives
                // this virtual input. Inner/phantom classes are never grounded
                // by a head and retained Fixed identities are never retagged.
                self.unify(life, head, 0, b)?;
            }
        }
        Ok(())
    }

    pub(super) fn abstract_scheme(
        &mut self,
        frame: &mut Frame,
        callee: &SemanticOriginalExpr,
        call: NodeId,
        local: Term,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<(Vec<Term>, Term)>> {
        self.step(depth, b)?;
        let root = self.root(local, b)?;
        let Kind::Address { mutable, inner, .. } = self.state.arena[root] else {
            return Ok(None);
        };
        let subject = self.root(inner, b)?;
        let Kind::Rigid(owner, ordinal) = self.state.arena[subject] else {
            return Ok(None);
        };
        let CoreExprKind::Name(name) = &callee.kind else {
            return Err(unknown(
                "abstract receiver needs its complete original place anchor",
            ));
        };
        let mut position = None;
        for (index, local) in frame.locals.iter().enumerate().rev() {
            b.spend(local.name.len().saturating_add(name.len() + 1), WORK)?;
            if local.name == *name {
                if !local.initialized {
                    return Err(fail(
                        "abstract receiver is not initialized in the original context",
                    ));
                }
                if local.used {
                    Self::push(
                        &mut self.needs,
                        Need {
                            term: local.ty,
                            predicate: Predicate::Copy,
                        },
                        b,
                    )?;
                }
                position = Some(index);
                break;
            }
        }
        let position =
            position.ok_or_else(|| fail("abstract receiver has no original lexical value"))?;
        let parameter = position
            .checked_sub(frame.inherited)
            .filter(|position| *position < frame.parameters.len())
            .ok_or_else(|| {
                unknown("abstract receiver alias needs its exact original Binding anchor")
            })?;
        let parameter = u32::try_from(parameter)
            .map_err(|_| fail("abstract receiver parameter ordinal overflow"))?;
        let (kind, signature) = self.callable_premise(owner, ordinal, b)?;
        match kind {
            CallableKind::Fn => {}
            CallableKind::FnMut if mutable => {}
            CallableKind::FnMut => {
                return Err(fail(
                    "abstract FnMut invocation requires an exclusive original receiver",
                ));
            }
            CallableKind::FnOnce => {
                return Err(fail("abstract FnOnce cannot consume a borrowed receiver"));
            }
            CallableKind::FunctionPointer => {
                return Err(unknown(
                    "abstract code pointer needs its exact original pointer invocation",
                ));
            }
        }
        let (parameters, result) = if owner == frame.definition {
            self.invocation_scheme(frame, &signature, b, depth + 1)?
        } else {
            let parent =
                self.state.frames.remove(&owner).ok_or_else(|| {
                    fail("abstract callable premise has no checked lexical owner")
                })?;
            let answer = self.invocation_scheme(&parent, &signature, b, depth + 1);
            b.lookup(self.state.frames.len(), WORK)?;
            self.state.frames.insert(owner, parent);
            answer?
        };
        b.spend(parameters.len().saturating_add(1), WORK)?;
        let saved = AbstractCall {
            parameter,
            subject,
            receiver: local,
            signature_owner: owner,
            signature,
            kind,
            parameters: parameters.clone(),
            result,
        };
        b.lookup(self.state.abstract_calls.len(), WORK)?;
        if self
            .state
            .abstract_calls
            .insert((frame.definition, call), saved)
            .is_some()
        {
            return Err(fail("abstract call has competing original resolutions"));
        }
        b.lookup(frame.expressions.len(), WORK)?;
        if frame.expressions.insert(callee.id, local).is_some() {
            return Err(fail("abstract receiver expression identity is reused"));
        }
        Ok(Some((parameters, result)))
    }
}

impl Checked {
    pub(in crate::semantic) fn abstract_boundary(
        &self,
        context: &SemanticInstance,
        site: SemanticSiteId,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<Option<AbstractBoundary>> {
        let definition = inventory.definition(context.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &definition.body else {
            return Err(fail(
                "abstract actual call omits its complete original body",
            ));
        };
        let mut node = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.site == site {
                let SemanticOriginalNode::Expression(id) = link.node else {
                    return Err(fail("abstract actual call selects an original statement"));
                };
                if node.replace(id).is_some() {
                    return Err(fail("abstract actual call has competing original nodes"));
                }
            }
        }
        let node = node.ok_or_else(|| fail("abstract actual call has no original node"))?;
        b.lookup(self.abstract_calls.len(), WORK)?;
        let Some(call) = self.abstract_calls.get(&(context.definition, node)) else {
            return Ok(None);
        };
        let mut parameters = Vec::new();
        for parameter in &call.parameters {
            let value = self.value(*parameter, context, program, inventory, b, 0)?;
            Solver::push(&mut parameters, value, b)?;
        }
        crate::dyn_metadata::recipe(&call.signature, b)?;
        Ok(Some(AbstractBoundary {
            parameter: call.parameter,
            subject: self.value(call.subject, context, program, inventory, b, 0)?,
            receiver: self.value(call.receiver, context, program, inventory, b, 0)?,
            signature_owner: call.signature_owner,
            signature: call.signature.clone(),
            kind: call.kind,
            parameters,
            result: self.value(call.result, context, program, inventory, b, 0)?,
        }))
    }
}
