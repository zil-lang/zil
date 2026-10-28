//! Reify serializable VM data into ordinary untrusted Core with fresh identities.
use crate::CompileTimeValue;
use zil_base::{CompileResult, Diagnostic, Limits, NodeId, Phase, Span};
use zil_ir::*;
fn quota(span: Span) -> Vec<Diagnostic> {
    vec![Diagnostic::limit(
        Phase::Ct,
        "CT materialized value node/depth/work budget exhausted",
        Some(span),
    )]
}

pub(super) fn scalar(c: Constant) -> CoreExprKind {
    match c {
        Constant::Unit => CoreExprKind::Unit,
        Constant::Bool(v) => CoreExprKind::Bool(v),
        Constant::U8(v) => CoreExprKind::Integer(format!("{v}u8")),
        Constant::U16(v) => CoreExprKind::Integer(format!("{v}u16")),
        Constant::NullHandle | Constant::NullWideString | Constant::WideString(_) => {
            unreachable!("independent CT whitelist excludes foreign constants")
        }
        Constant::I32(v) => CoreExprKind::Integer(format!("{v}i32")),
        Constant::I64(v) => CoreExprKind::Integer(format!("{v}i64")),
    }
}
pub(super) struct Reifier<'a> {
    pub next: &'a mut u32,
    pub nodes: &'a mut usize,
    pub work_left: &'a mut usize,
    pub limits: &'a Limits,
}
impl Reifier<'_> {
    fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
        if depth > self.limits.max_call_depth || *self.nodes >= self.limits.max_nodes {
            return Err(quota(span));
        }
        *self.work_left = self.work_left.checked_sub(1).ok_or_else(|| quota(span))?;
        *self.nodes += 1;
        Ok(())
    }
    fn fresh(&mut self, span: Span) -> CompileResult<NodeId> {
        let id = NodeId(*self.next);
        *self.next = self.next.checked_add(1).ok_or_else(|| quota(span))?;
        Ok(id)
    }
    fn text(&mut self, text: &str, span: Span) -> CompileResult<String> {
        *self.work_left = self
            .work_left
            .checked_sub(text.len())
            .ok_or_else(|| quota(span))?;
        Ok(text.to_owned())
    }
    fn child(
        &mut self,
        value: CompileTimeValue,
        ty: &Type,
        span: Span,
        depth: usize,
    ) -> CompileResult<CoreExpr> {
        let id = self.fresh(span)?;
        self.value(value, ty, id, span, depth)
    }
    pub fn value(
        &mut self,
        value: CompileTimeValue,
        ty: &Type,
        id: NodeId,
        span: Span,
        depth: usize,
    ) -> CompileResult<CoreExpr> {
        self.node(span, depth)?;
        let kind = match value {
            CompileTimeValue::Scalar(c) => scalar(c),
            CompileTimeValue::Aggregate(values) => match ty {
                Type::Tuple(types) => CoreExprKind::Tuple(
                    values
                        .into_iter()
                        .zip(types)
                        .map(|(v, t)| self.child(v, t, span, depth + 1))
                        .collect::<CompileResult<_>>()?,
                ),
                Type::Array { element, .. } => CoreExprKind::Array(
                    values
                        .into_iter()
                        .map(|v| self.child(v, element, span, depth + 1))
                        .collect::<CompileResult<_>>()?,
                ),
                Type::Record { fields, .. } => {
                    crate::source::hint_type(ty, depth, self.nodes, self.work_left, self.limits)?;
                    CoreExprKind::Record {
                        ty: ty.clone(),
                        fields: values
                            .into_iter()
                            .zip(fields)
                            .map(|(v, (n, t))| {
                                Ok((self.text(n, span)?, self.child(v, t, span, depth + 1)?))
                            })
                            .collect::<CompileResult<_>>()?,
                    }
                }
                _ => unreachable!("verified serializable result shape"),
            },
            CompileTimeValue::Variant { variant, fields } => {
                let Type::Enum { name, variants } = ty else {
                    unreachable!("verified enum result shape")
                };
                let (variant_name, types) = &variants[variant as usize];
                // These are actual ordinary Core nodes, not a privileged tag.
                self.node(span, depth + 1)?;
                self.node(span, depth + 2)?;
                let member_id = self.fresh(span)?;
                let namespace_id = self.fresh(span)?;
                let namespace = self.text(name, span)?;
                let member = self.text(variant_name, span)?;
                let callee = CoreExpr {
                    id: member_id,
                    span,
                    kind: CoreExprKind::Member {
                        object: Box::new(CoreExpr {
                            id: namespace_id,
                            span,
                            kind: CoreExprKind::Name(namespace),
                        }),
                        member,
                    },
                };
                let arguments = fields
                    .into_iter()
                    .zip(types)
                    .map(|(v, t)| self.child(v, t, span, depth + 1))
                    .collect::<CompileResult<_>>()?;
                CoreExprKind::Call {
                    callee: Box::new(callee),
                    arguments,
                }
            }
        };
        Ok(CoreExpr { id, kind, span })
    }
}

pub(super) fn first_free_id(core: &CoreModule, limits: &Limits) -> CompileResult<u32> {
    enum Visit<'a> {
        Block(&'a CoreBlock),
        Statement(&'a CoreStatement),
        Expr(&'a CoreExpr),
        Children(Children<'a>),
    }
    // These borrowed cursors preserve the former stack's reverse sibling order
    // without allocating one work item for every unchecked sibling. A cursor
    // is bookkeeping, not another Core node, so only yielded visits are charged.
    enum Children<'a> {
        Functions(&'a [CoreFunction]),
        Statements(&'a [CoreStatement]),
        Expressions(&'a [CoreExpr]),
        Fields(&'a [(String, CoreExpr)]),
        Arms(&'a [CoreMatchArm]),
    }
    impl<'a> Children<'a> {
        fn next_back(&mut self) -> Option<Visit<'a>> {
            Some(match self {
                Self::Functions(items) => {
                    let (last, rest) = items.split_last()?;
                    *items = rest;
                    Visit::Block(&last.body)
                }
                Self::Statements(items) => {
                    let (last, rest) = items.split_last()?;
                    *items = rest;
                    Visit::Statement(last)
                }
                Self::Expressions(items) => {
                    let (last, rest) = items.split_last()?;
                    *items = rest;
                    Visit::Expr(last)
                }
                Self::Fields(items) => {
                    let (last, rest) = items.split_last()?;
                    *items = rest;
                    Visit::Expr(&last.1)
                }
                Self::Arms(items) => {
                    let (last, rest) = items.split_last()?;
                    *items = rest;
                    Visit::Block(&last.body)
                }
            })
        }
    }
    let mut next = 0u32;
    let mut count = 0usize;
    let mut pending = Vec::new();
    let mut observe = |id: NodeId, span: Span| -> CompileResult<()> {
        next = next.max(id.0.checked_add(1).ok_or_else(|| quota(span))?);
        Ok(())
    };
    for aggregate in &core.aggregates {
        observe(aggregate.id, aggregate.span)?;
    }
    for enumeration in &core.enums {
        observe(enumeration.id, enumeration.span)?;
    }
    for f in &core.functions {
        observe(f.id, f.span)?;
    }
    pending.push((
        Visit::Children(Children::Functions(&core.functions)),
        0usize,
    ));
    while let Some((visit, depth)) = pending.pop() {
        let visit = match visit {
            Visit::Children(mut children) => {
                if let Some(child) = children.next_back() {
                    pending.push((Visit::Children(children), depth));
                    pending.push((child, depth));
                }
                continue;
            }
            node => node,
        };
        count += 1;
        if count > limits.max_nodes || depth > limits.max_call_depth {
            return Err(quota(match &visit {
                Visit::Block(b) => b.span,
                Visit::Statement(s) => s.span,
                Visit::Expr(e) => e.span,
                Visit::Children(_) => unreachable!("cursors yield above without charging"),
            }));
        }
        match visit {
            Visit::Block(b) => {
                pending.push((
                    Visit::Children(Children::Statements(&b.statements)),
                    depth + 1,
                ));
            }
            Visit::Statement(s) => {
                observe(s.id, s.span)?;
                match &s.kind {
                    CoreStatementKind::Binding { initializer, .. }
                    | CoreStatementKind::Return(initializer) => {
                        if let Some(e) = initializer {
                            pending.push((Visit::Expr(e), depth + 1));
                        }
                    }
                    CoreStatementKind::Assign { target, value } => {
                        pending.push((Visit::Expr(target), depth + 1));
                        pending.push((Visit::Expr(value), depth + 1));
                    }
                    CoreStatementKind::Expression(e) => pending.push((Visit::Expr(e), depth + 1)),
                    CoreStatementKind::If {
                        condition,
                        then_body,
                        else_body,
                    }
                    | CoreStatementKind::IfLet {
                        value: condition,
                        then_body,
                        else_body,
                        ..
                    } => {
                        pending.push((Visit::Expr(condition), depth + 1));
                        pending.push((Visit::Block(then_body), depth + 1));
                        if let Some(b) = else_body {
                            pending.push((Visit::Block(b), depth + 1));
                        }
                    }
                    CoreStatementKind::While { condition, body }
                    | CoreStatementKind::WhileLet {
                        value: condition,
                        body,
                        ..
                    } => {
                        pending.push((Visit::Expr(condition), depth + 1));
                        pending.push((Visit::Block(body), depth + 1));
                    }
                    CoreStatementKind::ForRange {
                        start, end, body, ..
                    } => {
                        pending.push((Visit::Expr(start), depth + 1));
                        pending.push((Visit::Expr(end), depth + 1));
                        pending.push((Visit::Block(body), depth + 1));
                    }
                    CoreStatementKind::Match { value, arms } => {
                        pending.push((Visit::Expr(value), depth + 1));
                        pending.push((Visit::Children(Children::Arms(arms)), depth + 1));
                    }
                    CoreStatementKind::Block(b) | CoreStatementKind::Unsafe(b) => {
                        pending.push((Visit::Block(b), depth + 1))
                    }
                    _ => {}
                }
            }
            Visit::Expr(e) => {
                observe(e.id, e.span)?;
                match &e.kind {
                    CoreExprKind::Unary { operand, .. }
                    | CoreExprKind::CompileTime(operand)
                    | CoreExprKind::Try(operand)
                    | CoreExprKind::TypeApply {
                        callee: operand, ..
                    }
                    | CoreExprKind::Ascribed { value: operand, .. }
                    | CoreExprKind::IntegerConvert { value: operand, .. } => {
                        pending.push((Visit::Expr(operand), depth + 1))
                    }
                    CoreExprKind::Binary { left, right, .. } => {
                        pending.push((Visit::Expr(left), depth + 1));
                        pending.push((Visit::Expr(right), depth + 1));
                    }
                    CoreExprKind::Call { callee, arguments } => {
                        pending.push((Visit::Expr(callee), depth + 1));
                        pending
                            .push((Visit::Children(Children::Expressions(arguments)), depth + 1));
                    }
                    CoreExprKind::Tuple(xs) | CoreExprKind::Array(xs) => {
                        pending.push((Visit::Children(Children::Expressions(xs)), depth + 1));
                    }
                    CoreExprKind::Member { object, .. } => {
                        pending.push((Visit::Expr(object), depth + 1))
                    }
                    CoreExprKind::Index { object, index } => {
                        pending.push((Visit::Expr(object), depth + 1));
                        pending.push((Visit::Expr(index), depth + 1));
                    }
                    CoreExprKind::Slice { object, start, end } => {
                        pending.push((Visit::Expr(object), depth + 1));
                        for endpoint in [start, end].into_iter().flatten() {
                            pending.push((Visit::Expr(endpoint), depth + 1));
                        }
                    }
                    CoreExprKind::Record { fields, .. } => {
                        pending.push((Visit::Children(Children::Fields(fields)), depth + 1));
                    }
                    CoreExprKind::RecordUpdate { base, fields, .. } => {
                        pending.push((Visit::Expr(base), depth + 1));
                        pending.push((Visit::Children(Children::Fields(fields)), depth + 1));
                    }
                    CoreExprKind::Sequence { bindings, value } => {
                        pending.push((Visit::Children(Children::Fields(bindings)), depth + 1));
                        pending.push((Visit::Expr(value), depth + 1));
                    }
                    CoreExprKind::If {
                        condition,
                        then_value,
                        else_value,
                    } => {
                        for v in [condition, then_value, else_value] {
                            pending.push((Visit::Expr(v), depth + 1));
                        }
                    }
                    CoreExprKind::Closure { body, .. } => {
                        pending.push((Visit::Block(body), depth + 1))
                    }
                    _ => {}
                }
            }
            Visit::Children(_) => unreachable!("cursors yield above without charging"),
        }
    }
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, Fingerprint, ModuleId, SCHEMA_VERSION};

    #[test]
    fn impl105_enum_reification_charges_fresh_constructor_nodes_text_and_depth() {
        let ty = Type::Enum {
            name: "@module0:E".into(),
            variants: vec![("N".into(), vec![Type::I64])],
        };
        let value = CompileTimeValue::Variant {
            variant: 0,
            fields: vec![CompileTimeValue::Scalar(Constant::I64(42))],
        };
        let base = Limits {
            max_nodes: 4,
            max_call_depth: 2,
            max_analysis_steps: 15,
            ..Limits::default()
        };
        for field in ["exact", "nodes", "depth", "work", "identity"] {
            let mut limits = base.clone();
            match field {
                "nodes" => limits.max_nodes -= 1,
                "depth" => limits.max_call_depth -= 1,
                "work" => limits.max_analysis_steps -= 1,
                _ => {}
            }
            let mut next = if field == "identity" { u32::MAX } else { 100 };
            let mut nodes = 0;
            let mut work_left = limits.max_analysis_steps;
            let result = Reifier {
                next: &mut next,
                nodes: &mut nodes,
                work_left: &mut work_left,
                limits: &limits,
            }
            .value(value.clone(), &ty, NodeId(5), span(5), 0);
            if field != "exact" {
                let errors = result.unwrap_err();
                assert_eq!(
                    errors[0].category,
                    zil_base::ResultCategory::ResourceLimit,
                    "{field}: {errors:?}"
                );
                assert_eq!(errors[0].phase, Phase::Ct);
                continue;
            }
            let root = result.unwrap();
            assert_eq!(root.id, NodeId(5));
            let CoreExprKind::Call { callee, arguments } = root.kind else {
                panic!("ordinary constructor call")
            };
            assert_eq!(callee.id, NodeId(100));
            let CoreExprKind::Member { object, member } = callee.kind else {
                panic!("ordinary namespace member")
            };
            assert_eq!(object.id, NodeId(101));
            assert_eq!(object.kind, CoreExprKind::Name("@module0:E".into()));
            assert_eq!(member, "N");
            assert_eq!(arguments.len(), 1);
            assert_eq!(arguments[0].id, NodeId(102));
            assert_eq!(arguments[0].kind, CoreExprKind::Integer("42i64".into()));
            assert_eq!((nodes, next, work_left), (4, 103, 0));
        }
    }

    fn span(id: u32) -> Span {
        Span::new(FileId(0), id as usize, id as usize + 1)
    }
    fn expression(id: u32, kind: CoreExprKind) -> CoreExpr {
        CoreExpr {
            id: NodeId(id),
            span: span(id),
            kind,
        }
    }
    fn statement(id: u32, kind: CoreStatementKind) -> CoreStatement {
        CoreStatement {
            id: NodeId(id),
            span: span(id),
            kind,
        }
    }
    fn block(id: u32, statements: Vec<CoreStatement>) -> CoreBlock {
        CoreBlock {
            span: span(id),
            statements,
        }
    }
    fn function(id: u32, body: CoreBlock) -> CoreFunction {
        CoreFunction {
            callable_return: None,
            type_parameters: Vec::new(),
            callable_parameters: Vec::new(),
            foreign: None,
            id: NodeId(id),
            name: format!("f{id}"),
            parameters: Vec::new(),
            return_type: Type::Unit,
            result_mode: PassingMode::Infer,
            receiver: None,
            body,
            span: span(id),
        }
    }
    fn input() -> CoreModule {
        let sequence = expression(
            32,
            CoreExprKind::Sequence {
                bindings: vec![
                    ("a".into(), expression(33, CoreExprKind::Unit)),
                    ("b".into(), expression(34, CoreExprKind::Unit)),
                ],
                value: Box::new(expression(35, CoreExprKind::Unit)),
            },
        );
        let arm = |id, statements| CoreMatchArm {
            pattern: CorePattern {
                kind: CorePatternKind::Wildcard,
                span: span(id),
            },
            body: block(id, statements),
            span: span(id),
        };
        CoreModule {
            schema_version: SCHEMA_VERSION,
            module_id: ModuleId(0),
            source_fingerprint: Fingerprint::ZERO,
            unsupported: Vec::new(),
            aggregates: vec![CoreAggregate {
                id: NodeId(200),
                name: "A".into(),
                kind: AggregateKind::Struct,
                teardown: None,
                fields: Vec::new(),
                span: span(200),
            }],
            enums: vec![CoreEnum {
                id: NodeId(300),
                name: "E".into(),
                variants: Vec::new(),
                span: span(300),
            }],
            functions: vec![
                function(1, block(1000, Vec::new())),
                function(
                    2,
                    block(
                        10,
                        vec![statement(
                            11,
                            CoreStatementKind::Match {
                                value: expression(12, CoreExprKind::Unit),
                                arms: vec![
                                    arm(
                                        20,
                                        vec![statement(
                                            21,
                                            CoreStatementKind::Expression(expression(
                                                22,
                                                CoreExprKind::Unit,
                                            )),
                                        )],
                                    ),
                                    arm(
                                        30,
                                        vec![statement(
                                            31,
                                            CoreStatementKind::Expression(sequence),
                                        )],
                                    ),
                                ],
                            },
                        )],
                    ),
                ),
            ],
        }
    }

    #[test]
    fn lazy_identity_cursor_preserves_exact_node_depth_and_diagnostic_order() {
        let core = input();
        // Independent expected visit order: later function first, final match
        // arm first, sequence value before its reversed bindings, match value
        // after both arms. Empty cursor frames must consume no Core-node quota.
        let order = [10, 11, 30, 31, 32, 35, 34, 33, 20, 21, 22, 12, 1000];
        let depths = [0, 1, 2, 3, 4, 5, 5, 5, 2, 3, 4, 2, 0];
        for (count, expected) in order.iter().enumerate() {
            let limits = Limits {
                max_nodes: count,
                ..Limits::default()
            };
            assert_eq!(
                first_free_id(&core, &limits).unwrap_err(),
                quota(span(*expected))
            );
        }
        assert_eq!(
            first_free_id(
                &core,
                &Limits {
                    max_nodes: order.len(),
                    ..Limits::default()
                }
            )
            .unwrap(),
            301
        );
        for depth in 0..5 {
            let expected = order[depths.iter().position(|value| *value > depth).unwrap()];
            let limits = Limits {
                max_call_depth: depth,
                ..Limits::default()
            };
            assert_eq!(
                first_free_id(&core, &limits).unwrap_err(),
                quota(span(expected))
            );
        }
        assert_eq!(
            first_free_id(
                &core,
                &Limits {
                    max_call_depth: 5,
                    ..Limits::default()
                }
            )
            .unwrap(),
            301
        );
    }

    #[test]
    fn identity_header_overflow_precedes_body_budget_and_empty_input_is_free() {
        let mut core = input();
        let limits = Limits {
            max_nodes: 0,
            ..Limits::default()
        };
        core.functions[0].id = NodeId(u32::MAX);
        core.enums[0].id = NodeId(u32::MAX);
        core.aggregates[0].id = NodeId(u32::MAX);
        assert_eq!(first_free_id(&core, &limits).unwrap_err(), quota(span(200)));
        core.aggregates.clear();
        assert_eq!(first_free_id(&core, &limits).unwrap_err(), quota(span(300)));
        core.enums.clear();
        assert_eq!(first_free_id(&core, &limits).unwrap_err(), quota(span(1)));
        core.functions.clear();
        assert_eq!(first_free_id(&core, &limits).unwrap(), 0);
    }

    #[test]
    fn impl090_slice_identity_cursor_visits_object_and_both_bounds() {
        for selected in 0..3 {
            let mut core = input();
            let mut children = [
                expression(41, CoreExprKind::Unit),
                expression(42, CoreExprKind::Unit),
                expression(43, CoreExprKind::Unit),
            ];
            children[selected].id = NodeId(u32::MAX);
            let [object, start, end] = children;
            core.functions = vec![function(
                1,
                block(
                    10,
                    vec![statement(
                        11,
                        CoreStatementKind::Expression(expression(
                            40,
                            CoreExprKind::Slice {
                                object: Box::new(object),
                                start: Some(Box::new(start)),
                                end: Some(Box::new(end)),
                            },
                        )),
                    )],
                ),
            )];
            assert_eq!(
                first_free_id(&core, &Limits::default()).unwrap_err(),
                quota(span(41 + selected as u32))
            );
        }
    }
}
