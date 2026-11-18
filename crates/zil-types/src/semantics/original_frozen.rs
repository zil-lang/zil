//! A selected CT wrapper is checked in the exact original query Gamma. The
//! frozen source remains a read-only interface and is never an E code target.
use super::super::frozen_context::FrozenSource;
use super::super::original_walk::{self, Context};
use super::*;

fn spend(left: &mut usize, n: usize, depth: usize, limits: &Limits) -> R<()> {
    *left = left.checked_sub(n).ok_or_else(|| {
        Diagnostic::limit(Phase::Types, "frozen original context work exceeded", None)
    })?;
    if depth > limits.max_call_depth {
        return Err(Diagnostic::limit(
            Phase::Types,
            "frozen original context depth exceeded",
            None,
        ));
    }
    Ok(())
}
struct Find<'a> {
    selected: NodeId,
    found: Option<SemanticOriginalExprKind>,
    limits: &'a Limits,
    left: &'a mut usize,
}
impl Context for Find<'_> {
    fn node(&mut self, depth: usize) -> R<()> {
        spend(self.left, 1, depth, self.limits)
    }
    fn ty(&mut self, _: &mut SemanticTypeRecipe, depth: usize) -> R<()> {
        self.node(depth)
    }
    fn descend_closure(&self) -> bool {
        false
    }
    fn expression(&mut self, id: NodeId, kind: &SemanticOriginalExprKind, depth: usize) -> R<()> {
        self.node(depth)?;
        if id == self.selected {
            if self.found.is_some() {
                return Err(bad("frozen original query node is duplicated"));
            }
            crate::dyn_metadata::semantic_operand_snapshot(kind, self.limits, self.left, 1)?;
            self.found = Some(kind.clone());
        }
        Ok(())
    }
}
struct ComparisonText<'a> {
    text: String,
    left: &'a mut usize,
}
impl std::fmt::Write for ComparisonText<'_> {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        let growth = if value.len() > self.text.capacity().saturating_sub(self.text.len()) {
            self.text.len()
        } else {
            0
        };
        *self.left = self
            .left
            .checked_sub(value.len().saturating_add(growth))
            .ok_or(std::fmt::Error)?;
        self.text
            .try_reserve(value.len())
            .map_err(|_| std::fmt::Error)?;
        self.text.push_str(value);
        Ok(())
    }
}
struct Normalize<'a> {
    proof: &'a Proof,
    definition: &'a SemanticDefinition,
    shift: u32,
    next: u32,
    limits: &'a Limits,
    left: &'a mut usize,
}
impl Context for Normalize<'_> {
    fn rewrite_expression(&mut self, value: &mut SemanticOriginalExpr, depth: usize) -> R<()> {
        // These are exactly the transparent nodes removed by CT's strip_expr.
        // S itself, including every branch and nested stage, was checked first.
        while matches!(
            value.kind,
            CoreExprKind::Group(_) | CoreExprKind::CompileTime(_)
        ) {
            spend(self.left, 1, depth, self.limits)?;
            let kind = std::mem::replace(&mut value.kind, CoreExprKind::Unit);
            let (CoreExprKind::Group(inner) | CoreExprKind::CompileTime(inner)) = kind else {
                unreachable!()
            };
            *value = *inner;
        }
        if !matches!(value.kind, CoreExprKind::Call { .. }) {
            return Ok(());
        }
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &self.definition.body
        else {
            return Err(bad(
                "normalized frozen expression loses its checked original body",
            ));
        };
        spend(self.left, site_nodes.len(), depth, self.limits)?;
        let mut selected = None;
        for link in site_nodes {
            if link.node != SemanticOriginalNode::Expression(value.id) {
                continue;
            }
            spend(
                self.left,
                self.proof.call_targets.len().saturating_add(1).ilog2() as usize + 1,
                depth,
                self.limits,
            )?;
            if let Some(target) = self
                .proof
                .call_targets
                .get(&(self.definition.id, link.site))
            {
                if selected.replace((link.site, *target)).is_some() {
                    return Err(bad(
                        "frozen call expression has several original direct targets",
                    ));
                }
            }
        }
        let Some((site, target)) = selected else {
            return Ok(());
        };
        spend(
            self.left,
            self.proof.calls.len().saturating_add(1).ilog2() as usize + 1,
            depth,
            self.limits,
        )?;
        let arguments = self
            .proof
            .calls
            .get(&(self.definition.id, site))
            .ok_or_else(|| bad("frozen direct call lacks its complete original mixed vector"))?;
        spend(self.left, arguments.len(), depth, self.limits)?;
        let empty = BTreeMap::new();
        let substitution = substitute::Substitution {
            frames: &[],
            selves: &[],
            environments: &[],
            implicit: &empty,
            invocation: &empty,
            shift: self.shift,
        };
        let mut actuals = Vec::with_capacity(arguments.len());
        for argument in arguments {
            actuals.push(substitute::actual(
                argument,
                &substitution,
                self.limits,
                self.left,
            )?);
        }
        // A comparison-only marker carries the checked declaration and entire
        // mixed vector. It is never published, emitted or accepted as code.
        let mut marker = ComparisonText {
            text: String::new(),
            left: self.left,
        };
        std::fmt::Write::write_fmt(
            &mut marker,
            format_args!("CT checked direct target {target:?}; full mixed arguments {actuals:?}"),
        )
        .map_err(|_| {
            Diagnostic::limit(
                Phase::Types,
                "frozen complete call identity text exceeded",
                None,
            )
        })?;
        let marker = marker.text;
        let CoreExprKind::Call { callee, .. } = &mut value.kind else {
            unreachable!()
        };
        callee.kind = CoreExprKind::Unsupported(marker);
        Ok(())
    }
    fn node(&mut self, depth: usize) -> R<()> {
        spend(self.left, 1, depth, self.limits)
    }
    fn ty(&mut self, value: &mut SemanticTypeRecipe, depth: usize) -> R<()> {
        self.node(depth)?;
        if self.shift != 0 {
            let empty = BTreeMap::new();
            let substitution = substitute::Substitution {
                frames: &[],
                selves: &[],
                environments: &[],
                implicit: &empty,
                invocation: &empty,
                shift: self.shift,
            };
            *value = substitute::ty(value, &substitution, self.limits, self.left)?;
        }
        Ok(())
    }
    fn expression_id(&mut self, id: &mut NodeId, depth: usize) -> R<()> {
        self.statement_id(id, depth)
    }
    fn statement_id(&mut self, id: &mut NodeId, depth: usize) -> R<()> {
        self.node(depth)?;
        *id = NodeId(self.next);
        self.next = self
            .next
            .checked_add(1)
            .ok_or_else(|| bad("frozen structural node correspondence overflows"))?;
        Ok(())
    }
    fn span(&mut self, span: &mut Span, depth: usize) -> R<()> {
        self.node(depth)?;
        *span = Span::default();
        Ok(())
    }
}
fn normalized_expression(
    expression: SemanticOriginalExpr,
    proof: &Proof,
    definition: &SemanticDefinition,
    shift: u32,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticOriginalExpr> {
    spend(left, 2, 0, limits)?;
    let mut block = SemanticOriginalBlock {
        statements: vec![SemanticOriginalStatement {
            id: NodeId(0),
            kind: CoreStatementKind::Return(Some(expression)),
            span: Span::default(),
        }],
        span: Span::default(),
    };
    original_walk::block(
        &mut block,
        &mut Normalize {
            proof,
            definition,
            shift,
            next: 0,
            limits,
            left,
        },
        0,
    )?;
    let statement = block
        .statements
        .pop()
        .ok_or_else(|| bad("frozen structural correspondence lost its root"))?;
    let CoreStatementKind::Return(Some(expression)) = statement.kind else {
        unreachable!()
    };
    Ok(expression)
}
fn clone_proof(
    source: &Proof,
    program: &SemanticModule,
    limits: &Limits,
    left: &mut usize,
) -> R<Proof> {
    spend(
        left,
        source
            .types
            .len()
            .saturating_add(source.capture_candidates.len())
            .saturating_add(source.view_bounds.len())
            .saturating_add(source.borrow_roles.len())
            .saturating_add(source.call_borrows.len().saturating_mul(4))
            .saturating_add(source.return_borrows.len())
            .saturating_add(source.shared_views.len())
            .saturating_add(source.field_reborrows.len())
            .saturating_add(source.calls.len())
            .saturating_add(source.complete.len())
            .saturating_add(source.closed_opaque_domains.len())
            .saturating_add(source.body_costs.len())
            .saturating_add(source.protocol_body_costs.len().saturating_mul(4))
            .saturating_add(source.protocol_method_costs.len().saturating_mul(4))
            .saturating_add(source.stage_costs.len())
            .saturating_add(source.task_returns.len())
            .saturating_add(source.task_result_types.len())
            .saturating_add(source.task_binding_sources.len())
            .saturating_add(source.task_name_sources.len())
            .saturating_add(source.protocol_complete.len())
            .saturating_add(source.cleanup_complete.len())
            .saturating_add(source.associated_complete.len())
            .saturating_add(source.capture_names.len())
            .saturating_add(source.closure_move.len())
            .saturating_add(source.injections.len())
            .saturating_add(source.kinds.len())
            .saturating_add(source.call_targets.len())
            .saturating_add(source.providers.len())
            .saturating_add(source.intrinsics.len())
            .saturating_add(source.method_calls.len())
            .saturating_add(source.callable_calls.len())
            .saturating_add(source.direct_calls.len())
            .saturating_add(source.function_values.len())
            .saturating_add(source.contextual_code_frames.len())
            .saturating_add(source.nominal_constructors.len())
            .saturating_add(source.call_result_origins.len())
            .saturating_add(source.call_callees.len())
            .saturating_add(source.call_injections.values().fold(0usize, |work, (_, operands)| {
                work.saturating_add(operands.len().saturating_add(1))
            }))
            .saturating_add(source.inferred_type_sources.len())
            .saturating_add(source.inferred_type_origins.len())
            .saturating_add(source.scope_calls.len())
            .saturating_add(source.local_roles.len())
            .saturating_add(source.capture_writes.len().saturating_mul(4))
            .saturating_add(source.environments.len())
            .saturating_add(source.effective_call_kind.len())
            .saturating_add(source.borrowed_capture_heads.len())
            .saturating_add(source.compile_time_calls.len())
            .saturating_add(source.compile_time_results.len())
            .saturating_add(source.cleanup_calls.len())
            .saturating_add(source.frozen_environments.len()),
        0,
        limits,
    )?;
    for ((_, query), ty) in &source.types {
        spend(left, query.copy_work(), 0, limits)?;
        crate::dyn_metadata::semantic_operand_snapshot(ty, limits, left, 1)?;
    }
    for name in source.task_name_sources.values() {
        spend(left, name.origin.copy_work().saturating_add(2), 0, limits)?;
    }
    for ((_, query), _) in &source.call_result_origins {
        spend(left, query.copy_work(), 0, limits)?;
    }
    for actuals in source.calls.values().chain(source.scope_calls.values()) {
        spend(left, actuals.len(), 0, limits)?;
        for actual in actuals {
            crate::dyn_metadata::semantic_operand_snapshot(actual, limits, left, 1)?;
        }
    }
    for call in source.intrinsics.values() {
        spend(left, call.parameters.len(), 0, limits)?;
        for ty in &call.parameters {
            crate::dyn_metadata::semantic_operand_snapshot(ty, limits, left, 1)?;
        }
        crate::dyn_metadata::semantic_operand_snapshot(&call.result, limits, left, 1)?;
        if let Some((signature, actuals)) = &call.worker_scheme {
            crate::dyn_metadata::semantic_operand_snapshot(signature, limits, left, 1)?;
            spend(left, actuals.len(), 0, limits)?;
            for actual in actuals {
                crate::dyn_metadata::semantic_operand_snapshot(actual, limits, left, 1)?;
            }
        }
        if let IntrinsicKind::RegistrationStart { providers, .. } = &call.kind {
            spend(left, providers.len(), 0, limits)?;
        }
        if let IntrinsicKind::DynCall {
            parameter_modes, ..
        } = &call.kind
        {
            spend(left, parameter_modes.len(), 0, limits)?;
        }
    }
    for call in source.method_calls.values() {
        crate::dyn_metadata::semantic_operand_snapshot(&call.self_type, limits, left, 1)?;
        if let Some(application) = &call.application {
            crate::dyn_metadata::semantic_operand_snapshot(application, limits, left, 1)?;
        }
        crate::dyn_metadata::semantic_operand_snapshot(&call.signature, limits, left, 1)?;
    }
    for call in source.callable_calls.values() {
        crate::dyn_metadata::semantic_operand_snapshot(&call.callee, limits, left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(&call.signature, limits, left, 1)?;
    }
    for function in source.function_values.values() {
        spend(left, function.actuals.len(), 0, limits)?;
        for actual in &function.actuals {
            crate::dyn_metadata::semantic_operand_snapshot(actual, limits, left, 1)?;
        }
        crate::dyn_metadata::semantic_operand_snapshot(&function.signature, limits, left, 1)?;
    }
    for origins in source.inferred_type_origins.values() {
        spend(left, origins.len(), 0, limits)?;
    }
    for result in source.task_result_types.values() {
        crate::dyn_metadata::semantic_operand_snapshot(result, limits, left, 1)?;
    }
    for returned in source.task_returns.values() {
        crate::dyn_metadata::semantic_operand_snapshot(&returned.ty, limits, left, 1)?;
        if let Some(values) = &returned.sources {
            task_results::source_snapshot(values, limits, left)?;
        }
    }
    for values in source.task_binding_sources.values().flatten() {
        task_results::source_snapshot(values, limits, left)?;
    }
    for injection in source.injections.values() {
        crate::dyn_metadata::semantic_operand_snapshot(&injection.payload, limits, left, 1)?;
    }
    for ((_, query), bound) in &source.view_bounds {
        spend(
            left,
            query.copy_work().saturating_add(bound.steps.len()),
            0,
            limits,
        )?;
        if let Some(lifetime) = &bound.lifetime {
            crate::dyn_metadata::semantic_operand_snapshot(lifetime, limits, left, 1)?;
        }
        for step in &bound.steps {
            match &step.kind {
                ViewBorrowStepKind::Capture { name } => spend(left, name.len(), 0, limits)?,
                ViewBorrowStepKind::CallInputs { inputs, .. } => {
                    if inputs.len() > limits.max_nodes {
                        return Err(Diagnostic::limit(Phase::Types, "frozen source input domain exceeded", None));
                    }
                    spend(left, inputs.len().saturating_mul(2), 0, limits)?;
                }
                ViewBorrowStepKind::ProjectedLocal { pattern, .. } => {
                    if pattern.len() > limits.max_call_depth {
                        return Err(Diagnostic::limit(Phase::Types, "frozen source projection depth exceeded", None));
                    }
                    spend(left, pattern.len(), 0, limits)?;
                }
                _ => {}
            }
        }
    }
    for role in source
        .borrow_roles
        .values()
        .chain(source.return_borrows.values())
        .chain(source.capture_writes.values().map(|write| &write.role))
    {
        spend(left, role.steps.len(), 0, limits)?;
        for step in &role.steps {
            match &step.kind {
                ViewBorrowStepKind::Capture { name } => spend(left, name.len(), 0, limits)?,
                ViewBorrowStepKind::CallInputs { inputs, .. } => {
                    if inputs.len() > limits.max_nodes {
                        return Err(Diagnostic::limit(Phase::Types, "frozen returned input count exceeded", None));
                    }
                    spend(left, inputs.len().saturating_mul(2), 0, limits)?;
                }
                ViewBorrowStepKind::ProjectedLocal { pattern, .. } => {
                    if pattern.len() > limits.max_call_depth {
                        return Err(Diagnostic::limit(
                            Phase::Types,
                            "frozen projected loan path depth exceeded",
                            None,
                        ));
                    }
                    spend(left, pattern.len(), 0, limits)?;
                }
                _ => {}
            }
        }
        for lifetime in [
            &role.source_lifetime,
            &role.target_lifetime,
            &role.occurrence_lifetime,
        ] {
            crate::dyn_metadata::semantic_operand_snapshot(lifetime, limits, left, 1)?;
        }
        crate::dyn_metadata::semantic_operand_snapshot(&role.inner, limits, left, 1)?;
    }
    for (_, query) in source.shared_views.keys() {
        spend(left, query.copy_work().saturating_add(1), 0, limits)?;
    }
    for view in source.field_reborrows.values() {
        crate::dyn_metadata::semantic_operand_snapshot(&view.place.root, limits, left, 1)?;
        spend(left, view.place.fields.len(), 0, limits)?;
        crate::dyn_metadata::semantic_operand_snapshot(&view.source, limits, left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(&view.result, limits, left, 1)?;
    }
    for names in source.capture_names.values() {
        spend(left, names.len(), 0, limits)?;
        for name in names {
            spend(left, name.len(), 0, limits)?;
        }
    }
    for method in source.method_calls.values() {
        if let Some(receiver) = &method.receiver_borrow {
            crate::dyn_metadata::semantic_operand_snapshot(receiver, limits, left, 1)?;
        }
        if let Some(place) = &method.receiver_place {
            crate::dyn_metadata::semantic_operand_snapshot(&place.root, limits, left, 1)?;
            spend(left, place.fields.len(), 0, limits)?;
        }
    }
    for signature in source.direct_calls.values() {
        crate::dyn_metadata::semantic_operand_snapshot(signature, limits, left, 1)?;
    }
    for role in source.local_roles.values() {
        crate::dyn_metadata::semantic_operand_snapshot(&role.ty, limits, left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(&role.lifetime, limits, left, 1)?;
        spend(left, role.sources.len(), 0, limits)?;
        for source in &role.sources {
            crate::dyn_metadata::semantic_operand_snapshot(&source.lifetime, limits, left, 1)?;
        }
    }
    for (_, ty) in source.environments.values() {
        crate::dyn_metadata::semantic_operand_snapshot(ty, limits, left, 1)?;
    }
    // The same bounded lexical snapshot routine accounts hidden shadow binders.
    let mut checker = Check {
        program,
        limits,
        left,
        proof: ProofAccess::Owned(Proof::default()),
        active: BTreeSet::new(),
        captures: BTreeMap::new(),
        loops: Vec::new(),
        trait_active: Vec::new(),
        contexts: Vec::new(),
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
    };
    for environment in source.frozen_environments.values() {
        checker.charge_variables(environment)?;
    }
    Ok(source.clone())
}
pub(crate) fn check_with_frozen(
    program: &SemanticModule,
    declarations: std::rc::Rc<Vec<super::super::original_context::Declaration>>,
    source: &FrozenSource,
    limits: &Limits,
    left: &mut usize,
) -> R<Proof> {
    spend(
        left,
        source
            .semantic
            .instances
            .len()
            .saturating_add(source.semantic.definitions.len())
            .saturating_add(program.definitions.len())
            .saturating_add(program.instances.len()),
        0,
        limits,
    )?;
    let parent = source
        .semantic
        .instances
        .iter()
        .find(|instance| instance.id == source.owner)
        .ok_or_else(|| bad("frozen original context loses its real S instance"))?;
    let parent_definition = source
        .semantic
        .definitions
        .iter()
        .find(|definition| definition.id == parent.definition)
        .ok_or_else(|| bad("frozen original context loses its S declaration"))?;
    for original in &source.semantic.definitions {
        crate::dyn_metadata::semantic_operand_snapshot(original, limits, left, 1)?;
        spend(left, program.definitions.len(), 0, limits)?;
        let mut matches = program
            .definitions
            .iter()
            .filter(|definition| definition.id == original.id);
        if matches.next() != Some(original) || matches.next().is_some() {
            return Err(bad(
                "E changes or duplicates an immutable S original declaration",
            ));
        }
        if matches!(
            original.body,
            SemanticDefinitionBody::FunctionWithBody { .. }
        ) && !source.original.complete.contains(&original.id)
        {
            return Err(bad("frozen S contains unchecked unused code"));
        }
    }
    let wrapper = program
        .definitions
        .iter()
        .find(|definition| definition.id == source.wrapper_definition)
        .ok_or_else(|| bad("E loses its generated wrapper original declaration"))?;
    let wrapper_instance = program
        .instances
        .iter()
        .find(|instance| instance.id == source.wrapper_instance)
        .ok_or_else(|| bad("E loses its generated wrapper instance"))?;
    if wrapper.parent != Some(parent_definition.id)
        || wrapper_instance.parent != Some(parent.id)
        || wrapper_instance.definition != wrapper.id
    {
        return Err(bad("E wrapper changes its actual S lexical parent"));
    }
    let SemanticOriginalNode::Expression(query) = source.query.original else {
        return Err(pending(
            "frozen statement queries require their complete result statement bridge",
        ));
    };
    let environment = source
        .original
        .frozen_environments
        .get(&(parent_definition.id, true, query))
        .ok_or_else(|| bad("frozen query has no checked Gamma at its exact original point"))?;
    let SemanticDefinitionBody::FunctionWithBody {
        body: SemanticOriginalFunctionBody::Defined(body),
        ..
    } = &parent_definition.body
    else {
        return Err(bad(
            "frozen original query owner omits its full source body",
        ));
    };
    crate::dyn_metadata::semantic_operand_snapshot(body, limits, left, 1)?;
    let mut body = body.clone();
    let mut find = Find {
        selected: query,
        found: None,
        limits,
        left,
    };
    original_walk::block(&mut body, &mut find, 0)?;
    let kind = find
        .found
        .ok_or_else(|| bad("frozen query node is outside its real source owner"))?;
    let original = match kind {
        CoreExprKind::CompileTime(inner) => *inner,
        kind => SemanticOriginalExpr {
            id: query,
            kind,
            span: Span::default(),
        },
    };
    let SemanticDefinitionBody::FunctionWithBody {
        parameters,
        body: SemanticOriginalFunctionBody::Defined(wrapper_body),
        ..
    } = &wrapper.body
    else {
        return Err(bad(
            "generated frozen wrapper omits its full original operation body",
        ));
    };
    if !parameters.is_empty() || wrapper_body.statements.len() != 1 {
        return Err(bad(
            "generated frozen wrapper adds executable statements or runtime parameters",
        ));
    }
    let CoreStatementKind::Return(Some(actual)) = &wrapper_body.statements[0].kind else {
        return Err(bad(
            "generated frozen wrapper does not return the selected original expression",
        ));
    };
    crate::dyn_metadata::semantic_operand_snapshot(actual, limits, left, 1)?;
    let mut proof = clone_proof(&source.original, program, limits, left)?;
    proof.pure_queries_ready = false;
    proof.premises = source_cache::Premises::default();
    proof.declarations = declarations;
    proof.definition_positions = definition_positions(program, limits, left)?;
    let mut check = Check {
        program,
        limits,
        left,
        proof: ProofAccess::Owned(proof),
        active: BTreeSet::new(),
        captures: BTreeMap::new(),
        loops: Vec::new(),
        trait_active: Vec::new(),
        contexts: Vec::new(),
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
    };
    check.associated_inventory(0)?;
    let mut variables = check.clone_variables(environment)?;
    let empty = BTreeMap::new();
    let substitution = substitute::Substitution {
        frames: &[],
        selves: &[],
        environments: &[],
        implicit: &empty,
        invocation: &empty,
        shift: 1,
    };
    for variable in variables.values_mut() {
        variable.ty = substitute::ty(&variable.ty, &substitution, limits, check.left)?;
    }
    check.function(wrapper, Some(variables))?;
    let expected = normalized_expression(
        original,
        &source.original,
        parent_definition,
        1,
        limits,
        check.left,
    )?;
    let actual =
        normalized_expression(actual.clone(), &check.proof, wrapper, 0, limits, check.left)?;
    crate::dyn_metadata::semantic_operand_snapshot(&expected, limits, check.left, 1)?;
    crate::dyn_metadata::semantic_operand_snapshot(&actual, limits, check.left, 1)?;
    if expected != actual {
        return Err(bad(
            "generated wrapper body differs from the selected original query after true parent relocation",
        ));
    }
    for definition in &program.definitions {
        if matches!(
            definition.body,
            SemanticDefinitionBody::FunctionWithBody { .. }
        ) && !check.proof.complete.contains(&definition.id)
        {
            if definition.kind != SemanticDefinitionKind::Closure {
                check.function(definition, None)?;
            }
        }
    }
    for definition in &program.definitions {
        if matches!(
            definition.body,
            SemanticDefinitionBody::FunctionWithBody { .. }
        ) && !check.proof.complete.contains(&definition.id)
        {
            return Err(bad(
                "E code has no checked original creation or frozen wrapper context",
            ));
        }
    }
    check.finish_effects()?;
    check.associated_obligations(0)?;
    check.cache_closed_opaque_domains()?;
    check.proof.premises.enable(check.left)?;
    protocol::check(program, check.proof.proof_mut()?, check.limits, check.left)?;
    check.proof.proof_mut()?.pure_queries_ready = true;
    check.proof.into_owned()
}
