//! First-class code uses the complete original signature. Referencing a
//! function is not an execution edge and cannot add premises to its body.
use super::*;

impl Solver<'_, '_> {
    pub(super) fn callback_value(
        &mut self,
        frame: &mut Frame,
        node: NodeId,
        written: &[CoreGenericArgument<SemanticTypeRecipe>],
        arguments: &[SemanticOriginalExpr],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        let [argument] = arguments else {
            return Err(fail(
                "c_callback requires exactly one original static function",
            ));
        };
        if !written.is_empty() {
            return Err(fail("c_callback has no generic actuals"));
        }
        let Some(pointer) = self.function_value(frame, argument, b, depth + 1)? else {
            return Err(unknown(
                "c_callback needs its exact original closed static declaration",
            ));
        };
        let Kind::Code {
            kind: SemanticCodeKind::FunctionPointer,
            parameters,
            result,
            modes,
            result_mode,
            call_kind: CallKind::Direct,
        } = &self.state.arena[self.type_root(pointer, b)?]
        else {
            return Err(fail(
                "c_callback changes its complete original static code family or ABI",
            ));
        };
        b.spend(parameters.len().saturating_add(modes.len() + 1), WORK)?;
        let parameters = parameters.clone();
        let modes = modes.clone();
        let (result, result_mode) = (*result, *result_mode);
        // C callback transport is restricted to the registered fixed numeric
        // ABI. A generic Copy premise or same machine size is not this proof.
        for term in parameters.iter().chain(std::iter::once(&result)) {
            if !matches!(&self.state.arena[self.type_root(*term,b)?],Kind::Atom(ty)
                if ty.is_b0_scalar() && !matches!(ty,Type::Unit|Type::Bool))
            {
                return Err(unknown(
                    "c_callback needs the complete fixed supported C scalar ABI",
                ));
            }
        }
        b.lookup(self.state.function_values.len(), WORK)?;
        let target = *self
            .state
            .function_values
            .get(&(frame.definition, argument.id))
            .ok_or_else(|| fail("original c_callback loses its exact static function identity"))?;
        let output = self.term(
            Kind::Code {
                kind: SemanticCodeKind::ForeignCallback,
                parameters,
                result,
                modes,
                result_mode,
                call_kind: CallKind::Direct,
            },
            b,
        )?;
        b.lookup(frame.expressions.len(), WORK)?;
        if frame.expressions.insert(argument.id, output).is_some() {
            return Err(fail("original callback operand node is reused"));
        }
        b.lookup(self.state.function_values.len(), WORK)?;
        if self
            .state
            .function_values
            .insert((frame.definition, node), target)
            .is_some()
        {
            return Err(fail(
                "original callback conversion node has competing targets",
            ));
        }
        b.lookup(self.state.intrinsics.len(), WORK)?;
        if self
            .state
            .intrinsics
            .insert((frame.definition, node), sealed::Intrinsic::CCallback)
            .is_some()
        {
            return Err(fail("original callback conversion has competing schemes"));
        }
        b.lookup(self.state.intrinsic_inputs.len(), WORK)?;
        self.state
            .intrinsic_inputs
            .insert((frame.definition, node), vec![output]);
        Ok(output)
    }

    pub(super) fn function_value(
        &mut self,
        frame: &Frame,
        value: &SemanticOriginalExpr,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<Term>> {
        self.step(depth, b)?;
        if !matches!(
            value.kind,
            CoreExprKind::Name(_) | CoreExprKind::Member { .. }
        ) {
            return Ok(None);
        }
        let Some(path) = sealed::path(value, b, depth + 1)? else {
            return Ok(None);
        };
        let mut locals = BTreeSet::new();
        for local in &frame.locals {
            b.spend(local.name.len().saturating_add(locals.len() + 1), WORK)?;
            locals.insert(local.name.clone());
        }
        let sealed::Resolution::Definition(target) =
            sealed::resolve(self.inventory, frame.definition, &path, &locals, b)?
        else {
            return Ok(None);
        };
        let declaration = self.inventory.definition(target, b)?;
        let SemanticDefinitionBody::FunctionWithBody {
            signature, foreign, ..
        } = &declaration.body
        else {
            return Ok(None);
        };
        if declaration.kind != SemanticDefinitionKind::Function
            || declaration.parent.is_some()
            || !declaration.environment.is_empty()
            || !declaration.parameters.is_empty()
            || foreign.is_some()
            || signature.call_kind != CallKind::Direct
            || signature.bound_lifetimes != 0
            || signature.invocation_lifetimes != 0
        {
            return Err(unknown(
                "original function value requires its complete specialization, environment and fixed code ABI",
            ));
        }
        let (_, parameters, result) = self.scheme(frame, target, &[], b, depth + 1)?;
        let mut modes = Vec::new();
        for (term, parameter) in parameters.iter().zip(&signature.parameters) {
            if !matches!(parameter.mode, PassingMode::Infer | PassingMode::Copy) {
                return Err(unknown(
                    "original thin function value requires its complete copy passing ABI",
                ));
            }
            Self::push(
                &mut self.needs,
                Need {
                    term: *term,
                    predicate: Predicate::Copy,
                },
                b,
            )?;
            Self::push(&mut modes, parameter.mode, b)?;
        }
        if !matches!(
            signature.result_mode,
            PassingMode::Infer | PassingMode::Copy
        ) {
            return Err(unknown(
                "original thin function result requires its complete passing ABI",
            ));
        }
        Self::push(
            &mut self.needs,
            Need {
                term: result,
                predicate: Predicate::Copy,
            },
            b,
        )?;
        b.lookup(self.state.function_values.len(), WORK)?;
        if self
            .state
            .function_values
            .insert((frame.definition, value.id), target)
            .is_some()
        {
            return Err(fail("original function reference node is repeated"));
        }
        self.term(
            Kind::Code {
                kind: SemanticCodeKind::FunctionPointer,
                parameters,
                result,
                modes,
                result_mode: signature.result_mode,
                call_kind: signature.call_kind,
            },
            b,
        )
        .map(Some)
    }

    pub(super) fn code_call(
        &mut self,
        frame: &mut Frame,
        head: &SemanticOriginalExpr,
        node: NodeId,
        term: Term,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<(Vec<Term>, Term)>> {
        self.step(depth, b)?;
        let root = self.type_root(term, b)?;
        let Kind::Code {
            kind,
            parameters,
            result,
            modes,
            result_mode,
            call_kind,
        } = &self.state.arena[root]
        else {
            return Ok(None);
        };
        if *kind != SemanticCodeKind::FunctionPointer
            || *call_kind != CallKind::Direct
            || !matches!(result_mode, PassingMode::Infer | PassingMode::Copy)
            || modes.len() != parameters.len()
            || modes
                .iter()
                .any(|mode| !matches!(mode, PassingMode::Infer | PassingMode::Copy))
        {
            return Err(unknown(
                "original indirect code call needs its complete fixed copy ABI",
            ));
        }
        b.spend(parameters.len().saturating_add(modes.len() + 1), WORK)?;
        let parameters = parameters.clone();
        let result = *result;
        for term in parameters.iter().chain(std::iter::once(&result)) {
            Self::push(
                &mut self.needs,
                Need {
                    term: *term,
                    predicate: Predicate::Copy,
                },
                b,
            )?;
        }
        let CoreExprKind::Name(name) = &head.kind else {
            return Err(unknown(
                "original indirect code call needs its exact lexical value anchor",
            ));
        };
        let mut anchor = None;
        for local in frame.locals.iter().rev() {
            b.spend(local.name.len().saturating_add(name.len() + 1), WORK)?;
            if local.name == *name {
                anchor = local.anchor;
                break;
            }
        }
        let anchor = anchor.ok_or_else(|| {
            unknown("original indirect code call has no exact parameter or local binding")
        })?;
        let callee = self.expression(frame, head, b, depth + 1)?;
        self.unify(callee, root, depth + 1, b)?;
        b.lookup(self.state.code_calls.len(), WORK)?;
        if self
            .state
            .code_calls
            .insert((frame.definition, node), CodeCall { code: root, anchor })
            .is_some()
        {
            return Err(fail("original indirect call node is repeated"));
        }
        Ok(Some((parameters, result)))
    }
}

impl Checked {
    pub(in crate::semantic) fn code_boundary(
        &self,
        context: &SemanticInstance,
        site: SemanticSiteId,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<Option<(LocalAnchor, crate::semantic::value::Value)>> {
        let definition = inventory.definition(context.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &definition.body else {
            return Err(fail("original code operation has no body inventory"));
        };
        let mut found = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.site == site {
                if let SemanticOriginalNode::Expression(node) = link.node {
                    b.lookup(self.code_calls.len(), WORK)?;
                    if let Some(call) = self.code_calls.get(&(context.definition, node)) {
                        let value = self.value(call.code, context, program, inventory, b, 0)?;
                        if found.replace((call.anchor, value)).is_some() {
                            return Err(fail("original code call has multiple node links"));
                        }
                    }
                }
            }
        }
        Ok(found)
    }
    pub(in crate::semantic) fn function_reference(
        &self,
        context: &SemanticInstance,
        site: SemanticSiteId,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<Option<SemanticDefinitionId>> {
        let definition = inventory.definition(context.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &definition.body else {
            return Err(fail("original code value has no body inventory"));
        };
        let mut found = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.site == site {
                if let SemanticOriginalNode::Expression(node) = link.node {
                    b.lookup(self.function_values.len(), WORK)?;
                    if let Some(target) = self.function_values.get(&(context.definition, node)) {
                        if found.replace(*target).is_some() {
                            return Err(fail("original code value has multiple node links"));
                        }
                    }
                }
            }
        }
        Ok(found)
    }
}
