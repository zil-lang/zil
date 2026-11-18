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
        let borrowed = match self.receiving_for(frame, value.id, b)? {
            Some(term) => {
                let root = self.type_root(term, b)?;
                matches!(self.state.arena[root], Kind::Code {
                    kind: SemanticCodeKind::BorrowedFunctionPointer, ..
                }).then_some(root)
            }
            None => None,
        };
        if declaration.kind != SemanticDefinitionKind::Function
            || declaration.parent.is_some()
            || !declaration.environment.is_empty()
            || !declaration.parameters.is_empty()
            || foreign.is_some()
            || signature.call_kind != CallKind::Direct
            || signature.bound_lifetimes != 0
            || borrowed.is_none() && signature.invocation_lifetimes != 0
        {
            return Err(unknown(
                "original function value requires its complete specialization, environment and fixed code ABI",
            ));
        }
        let (_, parameters, result) = self.scheme(frame, None, target, &[], None, b, depth + 1)?;
        if let Some(expected) = borrowed {
            for (parameter, term) in signature.parameters.iter().zip(&parameters) {
                b.spend(1, WORK)?;
                if !matches!(parameter.mode, PassingMode::Infer | PassingMode::Copy)
                    && !(parameter.mode == PassingMode::Borrow
                        && matches!(self.state.arena[self.type_root(*term, b)?], Kind::Address { .. }))
                {
                    return Err(fail("borrowed code target changes its complete original parameter mode"));
                }
            }
            self.check_borrowed_target(expected, &parameters, result, b, depth + 1)?;
            self.record_code_reference(frame.definition, value.id, target, b)?;
            return Ok(Some(expected));
        }
        self.fixed_code_value(frame.definition, value.id, target, parameters, result, signature, b)
            .map(Some)
    }

    /// The receiving occurrence may constrain a lexical child's body through
    /// its original code signature or through the owner's exact opaque result
    /// bound. Neither goal reads a specialized executable parameter/result.
    pub(super) fn closure_goal(
        &mut self,
        frame: &Frame,
        node: NodeId,
        child: SemanticDefinitionId,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<(Vec<Term>, Term)>> {
        let Some(wanted) = self.receiving_for(frame, node, b)? else {
            return Ok(None);
        };
        let root = self.type_root(wanted, b)?;
        if let Kind::Code { kind, parameters, result, call_kind: CallKind::Direct, .. } =
            &self.state.arena[root]
            && matches!(kind, SemanticCodeKind::FunctionPointer | SemanticCodeKind::BorrowedFunctionPointer)
        {
            b.spend(parameters.len().saturating_add(1), WORK)?;
            let borrowed = *kind == SemanticCodeKind::BorrowedFunctionPointer;
            let mut parameters = parameters.clone();
            let mut result = *result;
            if borrowed {
                self.instantiate_code_terms(&mut parameters, &mut result, Some(child), b)?;
            }
            return Ok(Some((parameters, result)));
        }
        if wanted != frame.result {
            return Ok(None);
        }
        let definition = self.inventory.definition(frame.definition, b)?;
        if !matches!(&definition.body, SemanticDefinitionBody::FunctionWithBody { signature, .. }
            if matches!(&signature.result, SemanticTypeRecipe::Opaque { definition: owner, hidden, .. }
                if *owner == definition.id && matches!(hidden.as_ref(), SemanticTypeRecipe::OriginalType {
                    scope: SemanticScopeRef::Ancestor(0), source: SemanticOriginalTypeSource::Result,
                })))
        {
            return Ok(None);
        }
        let mut selected = None;
        for requirement in &definition.requirements {
            b.spend(1, WORK)?;
            if let SemanticRequirementRecipe::Callable {
                ty: SemanticTypeRecipe::OriginalType {
                    scope: SemanticScopeRef::Ancestor(0), source: SemanticOriginalTypeSource::Result,
                },
                signature, ..
            } = requirement
                && selected.replace(signature).is_some_and(|prior| prior != signature)
            {
                return Ok(None);
            }
        }
        let Some(signature) = selected else { return Ok(None) };
        if signature.bound_lifetimes != 0 || signature.invocation_lifetimes != 0
            || signature.call_kind != CallKind::Direct
        {
            return Ok(None);
        }
        let mut inputs = Vec::new();
        for input in &signature.parameters {
            let term = self.recipe(frame, &input.ty, b, depth + 1)?;
            Self::push(&mut inputs, term, b)?;
        }
        let result = self.recipe(frame, &signature.result, b, depth + 1)?;
        Ok(Some((inputs, result)))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn thin_closure_value(
        &mut self,
        frame: &Frame,
        node: NodeId,
        target: SemanticDefinitionId,
        captures: bool,
        parameters: Vec<Term>,
        result: Term,
        signature: &SemanticSignatureRecipe,
        unchanged: Term,
        b: &mut Budget,
    ) -> CompileResult<Term> {
        let Some(receiving) = self.receiving_for(frame, node, b)? else {
            return Ok(unchanged);
        };
        let receiving = self.type_root(receiving, b)?;
        let Kind::Code { kind, .. } = self.state.arena[receiving] else {
            return Ok(unchanged);
        };
        if !matches!(kind, SemanticCodeKind::FunctionPointer | SemanticCodeKind::BorrowedFunctionPointer) {
            return Ok(unchanged);
        }
        let declaration = self.inventory.definition(target, b)?;
        if captures || !declaration.environment.is_empty() || !declaration.parameters.is_empty() {
            return Err(fail("original thin closure retains an environment or generic actuals"));
        }
        if kind == SemanticCodeKind::BorrowedFunctionPointer {
            self.check_borrowed_target(receiving, &parameters, result, b, 0)?;
            self.record_code_reference(frame.definition, node, target, b)?;
            return Ok(receiving);
        }
        self.fixed_code_value(frame.definition, node, target, parameters, result, signature, b)
    }

    #[allow(clippy::too_many_arguments)]
    fn fixed_code_value(
        &mut self,
        owner: SemanticDefinitionId,
        node: NodeId,
        target: SemanticDefinitionId,
        parameters: Vec<Term>,
        result: Term,
        signature: &SemanticSignatureRecipe,
        b: &mut Budget,
    ) -> CompileResult<Term> {
        if signature.call_kind != CallKind::Direct || signature.bound_lifetimes != 0
            || signature.invocation_lifetimes != 0 || parameters.len() != signature.parameters.len()
        {
            return Err(unknown("original thin code value requires its complete fixed signature"));
        }
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
        self.record_code_reference(owner, node, target, b)?;
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
    }

    fn code_projection_anchor(
        place: places::Place,
        index: Option<NodeId>,
        b: &mut Budget,
    ) -> CompileResult<CodeCallAnchor> {
        let local = place.anchor.ok_or_else(|| {
            unknown("original projected code has no exact lexical value anchor")
        })?;
        b.spend(place.path.len().saturating_add(1), WORK)?;
        let mut projections: Vec<_> = place.path.into_iter().map(CodeProjection::Field).collect();
        if let Some(index) = index {
            projections.push(CodeProjection::Index(index));
        }
        Ok(CodeCallAnchor::Projected { local, projections })
    }

    pub(super) fn borrowed_code_recipe(
        &mut self,
        frame: &Frame,
        signature: &SemanticSignatureRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        if signature.bound_lifetimes != 0 || signature.invocation_lifetimes == 0
            || signature.call_kind != CallKind::Direct
        {
            return Err(unknown("borrowed code needs a complete Direct invocation signature"));
        }
        let mut inputs = Vec::new();
        let mut modes = Vec::new();
        let mut used = BTreeSet::new();
        for argument in &signature.parameters {
            let term = self.borrowed_code_type(frame, &argument.ty, &mut used, b, depth + 1)?;
            let address = matches!(self.state.arena[self.type_root(term, b)?], Kind::Address { .. });
            if !matches!(argument.mode, PassingMode::Infer | PassingMode::Copy)
                && !(address && argument.mode == PassingMode::Borrow)
            {
                return Err(unknown("borrowed code parameter needs its complete ordinary mode"));
            }
            Self::push(&mut inputs, term, b)?;
            Self::push(&mut modes, if address { PassingMode::Infer } else { argument.mode }, b)?;
        }
        if used.len() != signature.invocation_lifetimes as usize
            || used.iter().copied().ne(0..signature.invocation_lifetimes)
        {
            return Err(fail("borrowed code parameters omit their exact invocation domain"));
        }
        let result = self.borrowed_code_type(frame, &signature.result, &mut used, b, depth + 1)?;
        if used.len() != signature.invocation_lifetimes as usize
            || !matches!(signature.result_mode, PassingMode::Infer | PassingMode::Copy)
        {
            return Err(unknown("borrowed code result requires its complete input relation"));
        }
        self.term(Kind::Code {
            kind: SemanticCodeKind::BorrowedFunctionPointer,
            parameters: inputs,
            result,
            modes,
            result_mode: signature.result_mode,
            call_kind: CallKind::Direct,
        }, b)
    }

    fn borrowed_code_type(
        &mut self,
        frame: &Frame,
        recipe: &SemanticTypeRecipe,
        used: &mut BTreeSet<u32>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        let SemanticTypeRecipe::Address { mutable, lifetime, inner } = recipe else {
            return self.recipe(frame, recipe, b, depth);
        };
        let SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Invocation(index)) = lifetime else {
            return Err(unknown("borrowed code requires a local invocation lifetime"));
        };
        b.lookup(used.len(), WORK)?;
        used.insert(*index);
        let life = self.term(Kind::Life(Life::Invocation(frame.definition, *index)), b)?;
        let inner = self.recipe(frame, inner, b, depth + 1)?;
        self.term(Kind::Address { mutable: *mutable, life, inner }, b)
    }

    pub(super) fn code_outer(
        &self,
        term: Term,
        b: &mut Budget,
    ) -> CompileResult<Option<(bool, u32, Term)>> {
        let Kind::Address { mutable, life, inner } = self.state.arena[self.type_root(term, b)?] else {
            return Ok(None);
        };
        let Kind::Life(Life::Invocation(_, index)) = self.state.arena[self.root(life, b)?] else {
            return Err(fail("borrowed code signature has no checked invocation class"));
        };
        Ok(Some((mutable, index, inner)))
    }

    pub(super) fn instantiate_code_terms(
        &mut self,
        parameters: &mut [Term],
        result: &mut Term,
        owner: Option<SemanticDefinitionId>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let mut classes = BTreeMap::new();
        for term in parameters.iter_mut().chain(std::iter::once(result)) {
            let Some((mutable, index, inner)) = self.code_outer(*term, b)? else {
                continue;
            };
            b.lookup(classes.len(), WORK)?;
            let life = match classes.get(&index) {
                Some(life) => *life,
                None => {
                    let kind = match owner {
                        Some(owner) => Kind::Life(Life::Invocation(owner, index)),
                        None => Kind::LifeVariable(None),
                    };
                    let life = self.term(kind, b)?;
                    classes.insert(index, life);
                    life
                }
            };
            *term = self.term(Kind::Address { mutable, life, inner }, b)?;
        }
        Ok(())
    }

    fn check_borrowed_target(
        &mut self,
        receiving: Term,
        parameters: &[Term],
        result: Term,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        let Kind::Code { kind: SemanticCodeKind::BorrowedFunctionPointer,
            parameters: expected, result: expected_result, .. } = &self.state.arena[receiving] else {
            return Err(fail("borrowed function value has no checked original receiving ABI"));
        };
        if expected.len() != parameters.len() {
            return Err(fail("borrowed code target changes its original arity"));
        }
        b.spend(expected.len().saturating_add(1), WORK)?;
        let expected = expected.clone();
        let expected_result = *expected_result;
        let mut classes = Vec::new();
        for (&actual, wanted) in parameters.iter().zip(expected).chain(std::iter::once((&result, expected_result))) {
            let Some((mode, ordinal, inner)) = self.code_outer(wanted, b)? else {
                self.unify(actual, wanted, depth + 1, b)?;
                continue;
            };
            let Kind::Address { mutable, life, inner: actual_inner } = self.state.arena[self.type_root(actual, b)?] else {
                return Err(fail("borrowed code target changes its original address parameter or result"));
            };
            let life = self.root(life, b)?;
            if mutable != mode
                || !matches!(self.state.arena[life], Kind::LifeVariable(None)
                    | Kind::Life(Life::Formal(..) | Life::Implicit(..) | Life::Invocation(..))) {
                return Err(fail("borrowed code target changes its original mode or invocation source"));
            }
            b.spend(classes.len(), WORK)?;
            for (prior, previous) in &classes {
                let same = *previous == life
                    || matches!((&self.state.arena[*previous], &self.state.arena[life]),
                        (Kind::Life(left), Kind::Life(right)) if left == right);
                if same != (*prior == ordinal) {
                    return Err(fail("borrowed code changes its complete original invocation relations"));
                }
            }
            classes.push((ordinal, life));
            self.unify(actual_inner, inner, depth + 1, b)?;
        }
        Ok(())
    }

    fn record_code_reference(
        &mut self,
        owner: SemanticDefinitionId,
        node: NodeId,
        target: SemanticDefinitionId,
        b: &mut Budget,
    ) -> CompileResult<()> {
        b.lookup(self.state.function_values.len(), WORK)?;
        if self.state.function_values.insert((owner, node), target).is_some() {
            return Err(fail("original function reference node is repeated"));
        }
        Ok(())
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
        let borrowed = *kind == SemanticCodeKind::BorrowedFunctionPointer;
        if !matches!(kind, SemanticCodeKind::FunctionPointer | SemanticCodeKind::BorrowedFunctionPointer)
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
        let mut parameters = parameters.clone();
        let mut result = *result;
        if borrowed {
            self.instantiate_code_terms(&mut parameters, &mut result, None, b)?;
        }
        for term in parameters.iter().chain(std::iter::once(&result)) {
            if borrowed && matches!(self.state.arena[self.type_root(*term, b)?], Kind::Address { .. }) {
                continue;
            }
            Self::push(
                &mut self.needs,
                Need {
                    term: *term,
                    predicate: Predicate::Copy,
                },
                b,
            )?;
        }
        let anchor = match &head.kind {
            CoreExprKind::Name(name) => {
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
                CodeCallAnchor::Local(anchor)
            }
            CoreExprKind::Call { .. }
                if matches!(self.state.arena[self.root(term, b)?],
                    Kind::CallResult { caller, node, .. }
                        if caller == frame.definition && node == head.id) =>
            {
                // The enclosing expression already checked this exact Call.
                // Rechecking it would duplicate its original call identity.
                CodeCallAnchor::CallResult(head.id)
            }
            CoreExprKind::Member { .. } => {
                let place = self.place(frame, head, true, b, depth + 1)?
                    .ok_or_else(|| unknown("original code field has no exact projected place"))?;
                self.unify(place.selected, root, depth + 1, b)?;
                Self::code_projection_anchor(place, None, b)?
            }
            CoreExprKind::Index { object, index } => {
                let place = self.place(frame, object, false, b, depth + 1)?
                    .ok_or_else(|| unknown("original code index has no exact projected base"))?;
                Self::code_projection_anchor(place, Some(index.id), b)?
            }
            _ => return Err(unknown("original indirect code call needs its exact value anchor")),
        };
        let invocation = if borrowed {
            b.spend(parameters.len().saturating_add(1), WORK)?;
            Some((parameters.clone(), result))
        } else {
            None
        };
        b.lookup(self.state.code_calls.len(), WORK)?;
        if self
            .state
            .code_calls
            .insert((frame.definition, node), CodeCall { code: root, anchor, invocation })
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
    ) -> CompileResult<Option<CodeCallBoundary<'_>>> {
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
                        let code = self.value(call.code, context, program, inventory, b, 0)?;
                        let invocation = if let Some((inputs, result)) = &call.invocation {
                            let expression = inventory.original_index(context.definition, b)?.get_expr(node, b)?;
                            let CoreExprKind::Call { arguments, .. } = &expression.kind else {
                                return Err(fail("borrowed code invocation has no original call arguments"));
                            };
                            if arguments.len() != inputs.len() {
                                return Err(fail("borrowed code invocation changes its original argument inventory"));
                            }
                            let mut actuals = Vec::new();
                            for (argument, term) in arguments.iter().zip(inputs) {
                                let value = self.value(*term, context, program, inventory, b, 0)?;
                                Solver::push(&mut actuals, (argument.id, value), b)?;
                            }
                            let output = self.value(*result, context, program, inventory, b, 0)?;
                            Some((actuals, output))
                        } else {
                            None
                        };
                        if found.replace(CodeCallBoundary { anchor: &call.anchor, code, invocation }).is_some() {
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
    ) -> CompileResult<Option<(NodeId, SemanticDefinitionId)>> {
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
                        if found.replace((node, *target)).is_some() {
                            return Err(fail("original code value has multiple node links"));
                        }
                    }
                }
            }
        }
        Ok(found)
    }
}
