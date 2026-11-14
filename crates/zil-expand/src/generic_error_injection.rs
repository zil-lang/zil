//! Definition-time error-arm choices are tied to syntax sites, not to the
//! substituted payload type. This matters when distinct formals become equal.
use super::*;

#[derive(Default)]
pub(super) struct Choices {
    pub expression: Option<bool>,
    pub statements: BTreeMap<usize, (Span, bool)>,
    pub arguments: BTreeMap<usize, ArgumentChoice>,
}
pub(super) struct ArgumentChoice {
    pub span: Span,
    pub error: TypeRef,
    pub result: TypeRef,
    pub is_error: bool,
}
impl Choices {
    pub(super) fn is_empty(&self) -> bool {
        self.expression.is_none() && self.statements.is_empty() && self.arguments.is_empty()
    }
}

/// These identities belong to one owned instance, before rewriting its callees.
/// Ordinals bind the original clone once; subsequent walks consume node anchors.
#[derive(Default)]
pub(super) struct ArgumentAnchors {
    sites: BTreeMap<usize, usize>,
}
pub(super) fn anchor_arguments(
    function: &Function,
    choices: &Choices,
    budget: &mut Budget<'_>,
) -> R<ArgumentAnchors> {
    let mut sites = BTreeMap::new();
    if !choices.arguments.is_empty() {
        let ids = trait_instances::expression_ids(&function.body, budget)?;
        for (pointer, ordinal) in ids {
            budget.node(function.span, 0)?;
            if choices.arguments.contains_key(&ordinal) {
                budget.lineage_bytes(std::mem::size_of::<(usize, usize)>(), function.span)?;
                sites.insert(pointer, ordinal);
            }
        }
        if sites.len() != choices.arguments.len() {
            return Err(error(
                function.span,
                "selected error argument lost its original node",
            ));
        }
    }
    Ok(ArgumentAnchors { sites })
}
struct RewriteSites<'a> {
    semantic: Option<&'a mut crate::source_semantics::program::Program>,
    moved: BTreeMap<usize, usize>,
    arguments: BTreeMap<usize, usize>,
}

pub(super) fn statement_ids(
    body: &FunctionBody,
    budget: &mut Budget<'_>,
) -> R<BTreeMap<usize, usize>> {
    use crate::ct_selection::Visitor;
    struct Ordinals<'a, 'l> {
        budget: &'a mut Budget<'l>,
        ids: BTreeMap<usize, usize>,
    }
    impl Visitor for Ordinals<'_, '_> {
        fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
            self.budget.node(span, depth).map_err(|e| vec![e])
        }
        fn text(&mut self, value: &str, span: Span) -> CompileResult<()> {
            self.budget.text(value, span).map_err(|e| vec![e])
        }
        fn statement(&mut self, statement: &Stmt, depth: usize) -> CompileResult<()> {
            self.node(statement.span, depth)?;
            self.ids
                .insert(statement as *const Stmt as usize, self.ids.len());
            crate::ct_selection::walk_statement(self, statement, depth)
        }
    }
    let mut visitor = Ordinals {
        budget,
        ids: BTreeMap::new(),
    };
    match body {
        FunctionBody::Block(block) => visitor.block(block, 0),
        FunctionBody::Expression(value) => visitor.expression(value, 0),
        FunctionBody::Declaration => Ok(()),
    }
    .map_err(|mut e| e.remove(0))?;
    Ok(visitor.ids)
}

fn wrap(
    value: &mut Expr,
    error: &TypeRef,
    result: &TypeRef,
    is_error: bool,
    budget: &mut Budget<'_>,
    moved_sites: &mut RewriteSites<'_>,
) -> R<()> {
    budget.const_expression(value)?;
    budget.ty(error)?;
    budget.ty(result)?;
    let span = value.span;
    budget.lineage_bytes(std::mem::size_of::<(usize, usize)>(), span)?;
    let old_site = value as *const Expr as usize;
    let mut payload = std::mem::replace(
        value,
        Expr {
            semantic: Default::default(),
            kind: ExprKind::Tuple(vec![]),
            span,
        },
    );
    let wrapper_marker = if let Some(program) = moved_sites.semantic.as_deref_mut() {
        let (payload_marker, result_marker) =
            program.split_implicit_injection(payload.semantic, is_error, budget, span)?;
        payload.semantic = payload_marker;
        result_marker
    } else {
        if payload.semantic.raw_index().is_some() {
            return Err(error(
                span,
                "implicit injection marker has no Source producer",
            ));
        }
        Default::default()
    };
    let name = if is_error { "error_err" } else { "error_ok" };
    *value = Expr {
        semantic: wrapper_marker,
        kind: ExprKind::Call {
            callee: Box::new(Expr {
                semantic: Default::default(),
                kind: ExprKind::TypeApply {
                    callee: Box::new(Expr {
                        semantic: Default::default(),
                        kind: ExprKind::Name(name.into()),
                        span,
                    }),
                    arguments: vec![
                        GenericArg::Type(error.clone()),
                        GenericArg::Type(result.clone()),
                    ],
                },
                span,
            }),
            arguments: vec![Argument {
                spread: false,
                value: payload,
                span,
            }],
        },
        span,
    };
    let ExprKind::Call { arguments, .. } = &value.kind else {
        unreachable!()
    };
    moved_sites
        .moved
        .insert(old_site, &arguments[0].value as *const Expr as usize);
    Ok(())
}

pub(super) fn apply(
    function: &mut Function,
    choices: &Choices,
    semantic: Option<&mut crate::source_semantics::program::Program>,
    budget: &mut Budget<'_>,
) -> R<BTreeMap<usize, usize>> {
    let anchors = anchor_arguments(function, choices, budget)?;
    apply_with_arguments(function, choices, anchors, semantic, budget)
}

pub(super) fn apply_with_arguments(
    function: &mut Function,
    choices: &Choices,
    anchors: ArgumentAnchors,
    semantic: Option<&mut crate::source_semantics::program::Program>,
    budget: &mut Budget<'_>,
) -> R<BTreeMap<usize, usize>> {
    if choices.is_empty() {
        return Ok(BTreeMap::new());
    }
    if anchors.sites.len() != choices.arguments.len() {
        return Err(error(
            function.span,
            "selected error argument anchors are incomplete",
        ));
    }
    if !anchors.sites.is_empty() {
        let current = trait_instances::expression_ids(&function.body, budget)?;
        for pointer in anchors.sites.keys() {
            budget.node(function.span, 0)?;
            if !current.contains_key(pointer) {
                return Err(error(
                    function.span,
                    "selected error argument was removed during rewriting",
                ));
            }
        }
    }
    // This feature-only walk follows every nested statement site, including
    // closure bodies and meta expressions. Prepay the complete current tree.
    budget.function_snapshot(function)?;
    let Some(TypeKind::Error { error, result }) =
        function.return_type.as_ref().map(|t| &grouped(t).kind)
    else {
        // Local annotated unions may be present even when the result is ordinary.
        return apply_body(function, choices, anchors, None, semantic, budget);
    };
    let pair = Some(((**error).clone(), (**result).clone()));
    apply_body(function, choices, anchors, pair, semantic, budget)
}

fn apply_body(
    function: &mut Function,
    choices: &Choices,
    anchors: ArgumentAnchors,
    return_pair: Option<(TypeRef, TypeRef)>,
    semantic: Option<&mut crate::source_semantics::program::Program>,
    budget: &mut Budget<'_>,
) -> R<BTreeMap<usize, usize>> {
    let mut next = 0;
    let mut moved_sites = RewriteSites {
        semantic,
        moved: BTreeMap::new(),
        arguments: anchors.sites,
    };
    match &mut function.body {
        FunctionBody::Expression(value) => {
            if let (Some(is_error), Some((error, result))) = (choices.expression, &return_pair) {
                wrap(value, error, result, is_error, budget, &mut moved_sites)?;
            }
            rewrite_expr(
                value,
                choices,
                &return_pair,
                &mut next,
                budget,
                &mut moved_sites,
            )?;
        }
        FunctionBody::Block(block) => rewrite_block(
            block,
            choices,
            &return_pair,
            &mut next,
            budget,
            &mut moved_sites,
        )?,
        FunctionBody::Declaration => {
            return Err(error(
                function.span,
                "selected generic error arm lacks a body",
            ));
        }
    }
    if choices
        .statements
        .keys()
        .next_back()
        .is_some_and(|last| *last >= next)
    {
        return Err(error(
            function.span,
            "selected generic error arm lost its source site",
        ));
    }
    if !moved_sites.arguments.is_empty() {
        return Err(error(
            function.span,
            "selected error argument was removed during rewriting",
        ));
    }
    Ok(moved_sites.moved)
}

fn rewrite_block(
    block: &mut Block,
    choices: &Choices,
    return_pair: &Option<(TypeRef, TypeRef)>,
    next: &mut usize,
    budget: &mut Budget<'_>,
    moved_sites: &mut RewriteSites<'_>,
) -> R<()> {
    for statement in &mut block.statements {
        rewrite_statement(statement, choices, return_pair, next, budget, moved_sites)?;
    }
    Ok(())
}

fn rewrite_statement(
    statement: &mut Stmt,
    choices: &Choices,
    return_pair: &Option<(TypeRef, TypeRef)>,
    next: &mut usize,
    budget: &mut Budget<'_>,
    moved_sites: &mut RewriteSites<'_>,
) -> R<()> {
    let site = *next;
    *next += 1;
    if let Some(&(original_span, is_error)) = choices.statements.get(&site) {
        if original_span != statement.span {
            return Err(error(
                statement.span,
                "selected generic error arm changed source site",
            ));
        }
        match &mut statement.kind {
            StmtKind::Return(Some(value)) => {
                let (error, result) = return_pair.as_ref().ok_or_else(|| {
                    error(statement.span, "selected error return lacks union type")
                })?;
                wrap(value, error, result, is_error, budget, moved_sites)?;
            }
            StmtKind::Binding {
                ty: Some(ty),
                value,
                ..
            } => {
                let TypeKind::Error { error, result } = &grouped(ty).kind else {
                    return Err(error(
                        statement.span,
                        "selected error binding lacks union type",
                    ));
                };
                wrap(value, error, result, is_error, budget, moved_sites)?;
            }
            _ => {
                return Err(error(
                    statement.span,
                    "selected generic error arm changed statement kind",
                ));
            }
        }
    }
    match &mut statement.kind {
        StmtKind::Binding { ty, value, .. } => {
            if let Some(ty) = ty {
                rewrite_type(ty, choices, return_pair, next, budget, moved_sites)?;
            }
            rewrite_expr(value, choices, return_pair, next, budget, moved_sites)?;
        }
        StmtKind::Return(Some(value)) | StmtKind::Expression(value) | StmtKind::Yield(value) => {
            rewrite_expr(value, choices, return_pair, next, budget, moved_sites)?;
        }
        StmtKind::Assign { target, value, .. } => {
            rewrite_expr(target, choices, return_pair, next, budget, moved_sites)?;
            rewrite_expr(value, choices, return_pair, next, budget, moved_sites)?;
        }
        StmtKind::If {
            condition,
            then_body,
            else_body,
        } => {
            rewrite_condition(condition, choices, return_pair, next, budget, moved_sites)?;
            rewrite_block(then_body, choices, return_pair, next, budget, moved_sites)?;
            match else_body {
                Some(ElseBranch::Block(body)) => {
                    rewrite_block(body, choices, return_pair, next, budget, moved_sites)?
                }
                Some(ElseBranch::If(statement)) => {
                    rewrite_statement(statement, choices, return_pair, next, budget, moved_sites)?
                }
                None => {}
            }
        }
        StmtKind::While { condition, body } => {
            rewrite_condition(condition, choices, return_pair, next, budget, moved_sites)?;
            rewrite_block(body, choices, return_pair, next, budget, moved_sites)?;
        }
        StmtKind::For { iterable, body, .. }
        | StmtKind::With {
            value: iterable,
            body,
            ..
        } => {
            rewrite_expr(iterable, choices, return_pair, next, budget, moved_sites)?;
            rewrite_block(body, choices, return_pair, next, budget, moved_sites)?;
        }
        StmtKind::Loop(body) | StmtKind::Unsafe(body) | StmtKind::Defer(body) => {
            rewrite_block(body, choices, return_pair, next, budget, moved_sites)?
        }
        StmtKind::Match { value, arms } => {
            rewrite_expr(value, choices, return_pair, next, budget, moved_sites)?;
            for arm in arms {
                rewrite_arrow(
                    &mut arm.body,
                    choices,
                    return_pair,
                    next,
                    budget,
                    moved_sites,
                )?;
            }
        }
        StmtKind::Meta(MetaStatement::If {
            condition,
            then_body,
            else_body,
        }) => {
            rewrite_expr(condition, choices, return_pair, next, budget, moved_sites)?;
            rewrite_block(then_body, choices, return_pair, next, budget, moved_sites)?;
            if let Some(body) = else_body {
                rewrite_block(body, choices, return_pair, next, budget, moved_sites)?;
            }
        }
        StmtKind::Meta(MetaStatement::For { iterable, body, .. }) => {
            rewrite_expr(iterable, choices, return_pair, next, budget, moved_sites)?;
            rewrite_block(body, choices, return_pair, next, budget, moved_sites)?;
        }
        StmtKind::Meta(MetaStatement::Emit(value)) => {
            rewrite_expr(value, choices, return_pair, next, budget, moved_sites)?
        }
        _ => {}
    }
    Ok(())
}
fn rewrite_condition(
    condition: &mut Condition,
    choices: &Choices,
    return_pair: &Option<(TypeRef, TypeRef)>,
    next: &mut usize,
    budget: &mut Budget<'_>,
    moved_sites: &mut RewriteSites<'_>,
) -> R<()> {
    match condition {
        Condition::Expression(value) | Condition::Let { value, .. } => {
            rewrite_expr(value, choices, return_pair, next, budget, moved_sites)
        }
    }
}

fn rewrite_arrow(
    arrow: &mut ArrowBody,
    choices: &Choices,
    return_pair: &Option<(TypeRef, TypeRef)>,
    next: &mut usize,
    budget: &mut Budget<'_>,
    moved_sites: &mut RewriteSites<'_>,
) -> R<()> {
    match arrow {
        ArrowBody::Block(body) => {
            rewrite_block(body, choices, return_pair, next, budget, moved_sites)
        }
        ArrowBody::Expression(value) => {
            rewrite_expr(value, choices, return_pair, next, budget, moved_sites)
        }
    }
}

fn rewrite_expr(
    value: &mut Expr,
    choices: &Choices,
    return_pair: &Option<(TypeRef, TypeRef)>,
    next: &mut usize,
    budget: &mut Budget<'_>,
    moved_sites: &mut RewriteSites<'_>,
) -> R<()> {
    let pointer = value as *const Expr as usize;
    if let Some(ordinal) = moved_sites.arguments.remove(&pointer) {
        let selected = &choices.arguments[&ordinal];
        if selected.span != value.span {
            return Err(error(
                value.span,
                "selected error argument changed source site",
            ));
        }
        wrap(
            value,
            &selected.error,
            &selected.result,
            selected.is_error,
            budget,
            moved_sites,
        )?;
    }
    match &mut value.kind {
        ExprKind::Ascribed { ty, value } => {
            rewrite_type(ty, choices, return_pair, next, budget, moved_sites)?;
            rewrite_expr(value, choices, return_pair, next, budget, moved_sites)?;
        }
        ExprKind::Group(inner)
        | ExprKind::Try(inner)
        | ExprKind::MetaEval(inner)
        | ExprKind::MetaSplice(inner)
        | ExprKind::Unary { operand: inner, .. } => {
            rewrite_expr(inner, choices, return_pair, next, budget, moved_sites)?
        }
        ExprKind::Tuple(items) | ExprKind::Array(items) => {
            for item in items {
                rewrite_expr(item, choices, return_pair, next, budget, moved_sites)?;
            }
        }
        ExprKind::Binary { left, right, .. }
        | ExprKind::Pipeline {
            input: left,
            target: right,
        } => {
            rewrite_expr(left, choices, return_pair, next, budget, moved_sites)?;
            rewrite_expr(right, choices, return_pair, next, budget, moved_sites)?;
        }
        ExprKind::Member { object, member } => {
            rewrite_expr(object, choices, return_pair, next, budget, moved_sites)?;
            if let MemberName::Constrained { owner, .. } = member {
                rewrite_type(owner, choices, return_pair, next, budget, moved_sites)?;
            }
        }
        ExprKind::TypeApply { callee, arguments } => {
            rewrite_expr(callee, choices, return_pair, next, budget, moved_sites)?;
            for argument in arguments {
                rewrite_arg(argument, choices, return_pair, next, budget, moved_sites)?;
            }
        }
        ExprKind::Call { callee, arguments } => {
            rewrite_expr(callee, choices, return_pair, next, budget, moved_sites)?;
            for argument in arguments {
                rewrite_expr(
                    &mut argument.value,
                    choices,
                    return_pair,
                    next,
                    budget,
                    moved_sites,
                )?;
            }
        }
        ExprKind::Index { object, index } => {
            rewrite_expr(object, choices, return_pair, next, budget, moved_sites)?;
            rewrite_expr(index, choices, return_pair, next, budget, moved_sites)?;
        }
        ExprKind::Slice { object, start, end } => {
            rewrite_expr(object, choices, return_pair, next, budget, moved_sites)?;
            for part in [start, end].into_iter().flatten() {
                rewrite_expr(part, choices, return_pair, next, budget, moved_sites)?;
            }
        }
        ExprKind::Record { ty, base, fields } => {
            for part in [ty, base].into_iter().flatten() {
                rewrite_expr(part, choices, return_pair, next, budget, moved_sites)?;
            }
            for field in fields {
                if let Some(part) = &mut field.value {
                    rewrite_expr(part, choices, return_pair, next, budget, moved_sites)?;
                }
            }
        }
        ExprKind::Closure {
            params,
            return_type,
            body,
            ..
        } => {
            for param in params {
                if let Some(ty) = &mut param.ty {
                    rewrite_type(ty, choices, return_pair, next, budget, moved_sites)?;
                }
            }
            if let Some(ty) = return_type {
                rewrite_type(ty, choices, return_pair, next, budget, moved_sites)?;
            }
            rewrite_arrow(body, choices, return_pair, next, budget, moved_sites)?
        }
        ExprKind::If {
            condition,
            then_value,
            else_value,
        } => {
            for part in [condition, then_value, else_value] {
                rewrite_expr(part, choices, return_pair, next, budget, moved_sites)?;
            }
        }
        ExprKind::MetaBlock(body) => {
            rewrite_block(body, choices, return_pair, next, budget, moved_sites)?
        }
        ExprKind::Quote { template, .. } => {
            for hole in &mut template.holes {
                rewrite_expr(
                    &mut hole.value,
                    choices,
                    return_pair,
                    next,
                    budget,
                    moved_sites,
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn rewrite_arg(
    argument: &mut GenericArg,
    choices: &Choices,
    return_pair: &Option<(TypeRef, TypeRef)>,
    next: &mut usize,
    budget: &mut Budget<'_>,
    moved_sites: &mut RewriteSites<'_>,
) -> R<()> {
    match argument {
        GenericArg::AssociatedBinding(binding) => Err(error(
            binding.span,
            "associated equations require the original trait application obligation consumer",
        )),
        GenericArg::Type(ty) => rewrite_type(ty, choices, return_pair, next, budget, moved_sites),
        GenericArg::Const(value) => {
            rewrite_expr(value, choices, return_pair, next, budget, moved_sites)
        }
        GenericArg::Lifetime(_) => Ok(()),
    }
}

fn rewrite_type(
    ty: &mut TypeRef,
    choices: &Choices,
    return_pair: &Option<(TypeRef, TypeRef)>,
    next: &mut usize,
    budget: &mut Budget<'_>,
    moved_sites: &mut RewriteSites<'_>,
) -> R<()> {
    if let TypeKind::Dyn { bindings, .. } = &mut ty.kind {
        for binding in bindings {
            match &mut binding.kind {
                DynBindingKind::Type { value, .. } => {
                    rewrite_type(value, choices, return_pair, next, budget, moved_sites)?
                }
                DynBindingKind::Constant { ty, value, .. } => {
                    rewrite_type(ty, choices, return_pair, next, budget, moved_sites)?;
                    rewrite_expr(value, choices, return_pair, next, budget, moved_sites)?;
                }
                DynBindingKind::Method { arguments, .. } => {
                    for argument in arguments {
                        rewrite_arg(argument, choices, return_pair, next, budget, moved_sites)?;
                    }
                }
            }
        }
    }
    match &mut ty.kind {
        TypeKind::Named { args, .. } | TypeKind::Dyn { args, .. } => {
            for argument in args {
                rewrite_arg(argument, choices, return_pair, next, budget, moved_sites)?;
            }
        }
        TypeKind::Reference { inner, .. }
        | TypeKind::Group(inner)
        | TypeKind::Optional(inner)
        | TypeKind::ForLifetime { inner, .. } => {
            rewrite_type(inner, choices, return_pair, next, budget, moved_sites)?;
        }
        TypeKind::Tuple(items) | TypeKind::Union(items) | TypeKind::Impl(items) => {
            for item in items {
                rewrite_type(item, choices, return_pair, next, budget, moved_sites)?;
            }
        }
        TypeKind::Array { element, length } => {
            rewrite_type(element, choices, return_pair, next, budget, moved_sites)?;
            if let Some(length) = length {
                rewrite_expr(length, choices, return_pair, next, budget, moved_sites)?;
            }
        }
        TypeKind::Function { params, result }
        | TypeKind::ForeignFunction { params, result, .. } => {
            for param in params {
                rewrite_type(param, choices, return_pair, next, budget, moved_sites)?;
            }
            rewrite_type(result, choices, return_pair, next, budget, moved_sites)?;
        }
        TypeKind::Error { error, result } => {
            rewrite_type(error, choices, return_pair, next, budget, moved_sites)?;
            rewrite_type(result, choices, return_pair, next, budget, moved_sites)?;
        }
        TypeKind::MetaEval(value) | TypeKind::MetaSplice(value) => {
            rewrite_expr(value, choices, return_pair, next, budget, moved_sites)?;
        }
        TypeKind::MetaBlock(block) => {
            rewrite_block(block, choices, return_pair, next, budget, moved_sites)?
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, ModuleId, SourceUnit};

    #[test]
    fn argument_anchors_survive_callee_growth_and_reject_removed_nodes() {
        for remove in [false, true] {
            let input = SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "argument-anchor.zil",
                "fn relay<U:Copy>(w:U)->i32{return sink(7,w)}",
            );
            let mut syntax = zil_syntax::parse(&input).into_result().unwrap();
            let ItemKind::Function(function) = &mut syntax.items[0].kind else {
                unreachable!()
            };
            let budget_limits = Limits::default();
            let mut budget = Budget::new(&budget_limits);
            let ids = trait_instances::expression_ids(&function.body, &mut budget).unwrap();
            let FunctionBody::Block(body) = &mut function.body else {
                unreachable!()
            };
            let StmtKind::Return(Some(call)) = &mut body.statements[0].kind else {
                unreachable!()
            };
            let ExprKind::Call { arguments, .. } = &call.kind else {
                unreachable!()
            };
            let pointer = &arguments[0].value as *const Expr as usize;
            let span = arguments[0].value.span;
            let mut choices = Choices::default();
            choices.arguments.insert(
                ids[&pointer],
                ArgumentChoice {
                    span,
                    error: named("U", span),
                    result: named("i32", span),
                    is_error: false,
                },
            );
            let anchors = anchor_arguments(function, &choices, &mut budget).unwrap();
            let FunctionBody::Block(body) = &mut function.body else {
                unreachable!()
            };
            let StmtKind::Return(Some(call)) = &mut body.statements[0].kind else {
                unreachable!()
            };
            let ExprKind::Call { callee, arguments } = &mut call.kind else {
                unreachable!()
            };
            if remove {
                arguments.clear();
                let error = apply_with_arguments(function, &choices, anchors, None, &mut budget)
                    .unwrap_err();
                assert!(error.message.contains("removed during rewriting"));
            } else {
                // Callee rewriting grows a subtree before this argument in traversal order.
                // The argument is the same actual node, regardless of its new ordinal.
                callee.kind = ExprKind::Group(Box::new(Expr {
                    semantic: Default::default(),
                    span: callee.span,
                    kind: ExprKind::Name("rewritten_sink".into()),
                }));
                let moved =
                    apply_with_arguments(function, &choices, anchors, None, &mut budget).unwrap();
                assert!(moved.contains_key(&pointer));
                let FunctionBody::Block(body) = &function.body else {
                    unreachable!()
                };
                let StmtKind::Return(Some(call)) = &body.statements[0].kind else {
                    unreachable!()
                };
                let ExprKind::Call { arguments, .. } = &call.kind else {
                    unreachable!()
                };
                let ExprKind::Call { callee, .. } = &arguments[0].value.kind else {
                    panic!("argument choice lost")
                };
                let ExprKind::TypeApply { callee, arguments } = &callee.kind else {
                    unreachable!()
                };
                assert!(matches!(&callee.kind, ExprKind::Name(name) if name == "error_ok"));
                assert!(
                    matches!(&arguments[0], GenericArg::Type(TypeRef { kind: TypeKind::Named { path, .. }, .. }) if path == &["U"])
                );
            }
        }
    }

    #[test]
    fn generated_ascription_and_type_meta_sites_precede_selected_return() {
        let input = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "generated-ascription.zil",
            "fn choose<E:Copy,T:Copy>(value:T)->E!T{let marker=()=>{let n=1;return n};return value}",
        );
        let mut syntax = zil_syntax::parse(&input).into_result().unwrap();
        let ItemKind::Function(function) = &mut syntax.items[0].kind else {
            unreachable!()
        };
        let FunctionBody::Block(body) = &mut function.body else {
            unreachable!()
        };
        let StmtKind::Binding { value, .. } = &mut body.statements[0].kind else {
            unreachable!()
        };
        let span = value.span;
        let nested = Stmt {
            semantic: Default::default(),
            kind: StmtKind::Expression(Expr {
                semantic: Default::default(),
                kind: ExprKind::Literal(Literal::Integer("0".into())),
                span,
            }),
            span,
        };
        let closure = value.clone();
        value.kind = ExprKind::Ascribed {
            ty: TypeRef {
                kind: TypeKind::MetaBlock(Block {
                    statements: vec![nested],
                    span,
                }),
                span,
            },
            value: Box::new(closure),
        };
        let return_site = &body.statements[1];
        let return_pointer = return_site as *const Stmt as usize;
        let return_span = return_site.span;
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        let ids = statement_ids(&function.body, &mut budget).unwrap();
        let ordinal = ids[&return_pointer];
        assert_eq!(
            ordinal, 4,
            "type meta and ascribed closure bodies are real earlier sites"
        );
        let mut choices = Choices::default();
        choices.statements.insert(ordinal, (return_span, false));
        apply(function, &choices, None, &mut budget).unwrap();
        let FunctionBody::Block(body) = &function.body else {
            unreachable!()
        };
        let StmtKind::Return(Some(Expr {
            kind: ExprKind::Call { callee, .. },
            ..
        })) = &body.statements[1].kind
        else {
            panic!("selected return was not rewritten")
        };
        let ExprKind::TypeApply { callee, .. } = &callee.kind else {
            unreachable!()
        };
        assert!(matches!(&callee.kind, ExprKind::Name(name) if name == "error_ok"));
    }
}
