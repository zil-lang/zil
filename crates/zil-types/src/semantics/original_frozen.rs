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
struct Normalize<'a> {
    shift: u32,
    next: u32,
    limits: &'a Limits,
    left: &'a mut usize,
}
impl Context for Normalize<'_> {
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
            .saturating_add(source.calls.len())
            .saturating_add(source.complete.len())
            .saturating_add(source.kinds.len())
            .saturating_add(source.call_targets.len())
            .saturating_add(source.providers.len())
            .saturating_add(source.intrinsics.len())
            .saturating_add(source.method_calls.len())
            .saturating_add(source.callable_calls.len())
            .saturating_add(source.direct_calls.len())
            .saturating_add(source.scope_calls.len())
            .saturating_add(source.local_roles.len())
            .saturating_add(source.environments.len())
            .saturating_add(source.effective_call_kind.len())
            .saturating_add(source.borrowed_capture_heads.len())
            .saturating_add(source.compile_time_calls.len())
            .saturating_add(source.cleanup_calls.len())
            .saturating_add(source.frozen_environments.len()),
        0,
        limits,
    )?;
    for ((_, query), ty) in &source.types {
        spend(left, query.copy_work(), 0, limits)?;
        crate::dyn_metadata::semantic_operand_snapshot(ty, limits, left, 1)?;
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
        proof: Proof::default(),
        active: BTreeSet::new(),
        captures: BTreeMap::new(),
        loops: Vec::new(),
        trait_active: Vec::new(),
        contexts: Vec::new(),
        predicate_active: Vec::new(),
        stage_depth: 0,
        cleanup_depth: 0,
        requested_snapshot: None,
        escape_active: Vec::new(),
    };
    for environment in source.frozen_environments.values() {
        checker.charge_variables(environment)?;
    }
    Ok(source.clone())
}
pub(crate) fn check_with_frozen(
    program: &SemanticModule,
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
    let expected = normalized_expression(original, 1, limits, left)?;
    let actual = normalized_expression(actual.clone(), 0, limits, left)?;
    crate::dyn_metadata::semantic_operand_snapshot(&expected, limits, left, 1)?;
    crate::dyn_metadata::semantic_operand_snapshot(&actual, limits, left, 1)?;
    if expected != actual {
        return Err(bad("generated wrapper body differs from the selected original query after true parent relocation"));
    }
    let proof = clone_proof(&source.original, program, limits, left)?;
    let mut check = Check {
        program,
        limits,
        left,
        proof,
        active: BTreeSet::new(),
        captures: BTreeMap::new(),
        loops: Vec::new(),
        trait_active: Vec::new(),
        contexts: Vec::new(),
        predicate_active: Vec::new(),
        stage_depth: 0,
        cleanup_depth: 0,
        requested_snapshot: None,
        escape_active: Vec::new(),
    };
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
    Ok(check.proof)
}
