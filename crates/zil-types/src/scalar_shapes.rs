//! Identity shape resolution for the current immutable prepared scalar input.
//! Keep the traversal and charges aligned with aggregates::Resolver. Unknown
//! shapes fall back; this is not a shortcut through subsequent type checking.
use super::*;

enum Stop {
    Unsupported,
    Error(Diagnostic),
}
type Probe = Result<(), Stop>;

pub(super) fn validate(program: &CoreProgram, limits: &Limits) -> Option<CompileResult<()>> {
    // Check every header before reporting any speculative body error. These
    // conditions make declaration/owner/callable indexes empty, header checks
    // trivial, and all owning-mode and receiver transformations inapplicable.
    // Current inference bounded this immutable input before preparation.
    if program.modules.iter().any(|module| {
        !module.core.aggregates.is_empty()
            || !module.core.enums.is_empty()
            || module.core.functions.iter().any(|function| {
                !function.type_parameters.is_empty()
                    || !function.callable_parameters.is_empty()
                    || !function.state_parameters.is_empty()
                    || function.callable_return.is_some()
                    || function.receiver.is_some()
                    || matches!(
                        function.result_mode,
                        PassingMode::Create | PassingMode::Give
                    )
                    || !scalar(&function.return_type)
                    || function
                        .parameters
                        .iter()
                        .any(|p| p.mode == PassingMode::Give || !scalar(&p.ty))
            })
    }) {
        return None;
    }
    let mut walker = Walker {
        remaining: limits.max_analysis_steps,
        depth_limit: limits.max_call_depth,
    };
    let result = (|| {
        for module in &program.modules {
            for function in &module.core.functions {
                walker.ty(&function.return_type, 0, function.span)?;
                for parameter in &function.parameters {
                    walker.ty(&parameter.ty, 0, parameter.span)?;
                }
                walker.block(&function.body, 0)?;
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => Some(Ok(())),
        Err(Stop::Error(error)) => Some(Err(vec![error])),
        Err(Stop::Unsupported) => None,
    }
}

fn scalar(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Unit | Type::Bool | Type::I32 | Type::I64 | Type::U8 | Type::U16
    )
}

struct Walker {
    remaining: usize,
    depth_limit: usize,
}
impl Walker {
    fn charge(&mut self, depth: usize, span: Span) -> Probe {
        if depth > self.depth_limit || self.remaining == 0 {
            return Err(Stop::Error(Diagnostic::limit(
                Phase::Types,
                "aggregate resolution budget exceeded",
                Some(span),
            )));
        }
        self.remaining -= 1;
        Ok(())
    }

    fn ty(&mut self, ty: &Type, depth: usize, span: Span) -> Probe {
        self.charge(depth, span)?;
        if scalar(ty) {
            Ok(())
        } else {
            Err(Stop::Unsupported)
        }
    }

    fn block(&mut self, block: &CoreBlock, depth: usize) -> Probe {
        self.charge(depth, block.span)?;
        // Resolver charges blocks and expressions, not statement wrappers.
        for statement in &block.statements {
            match &statement.kind {
                CoreStatementKind::Binding {
                    ty, initializer, ..
                } => {
                    if let Some(ty) = ty {
                        self.ty(ty, depth + 1, statement.span)?;
                    }
                    if let Some(value) = initializer {
                        self.expr(value, depth + 1)?;
                    }
                }
                CoreStatementKind::Assign { target, value } => {
                    self.expr(target, depth + 1)?;
                    self.expr(value, depth + 1)?;
                }
                CoreStatementKind::Expression(value) | CoreStatementKind::Return(Some(value)) => {
                    self.expr(value, depth + 1)?;
                }
                CoreStatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.expr(condition, depth + 1)?;
                    self.block(then_body, depth + 1)?;
                    if let Some(body) = else_body {
                        self.block(body, depth + 1)?;
                    }
                }
                CoreStatementKind::While { condition, body } => {
                    self.expr(condition, depth + 1)?;
                    self.block(body, depth + 1)?;
                }
                CoreStatementKind::Block(body)
                | CoreStatementKind::Unsafe(body)
                | CoreStatementKind::Defer(body) => {
                    self.block(body, depth + 1)?;
                }
                CoreStatementKind::Return(None)
                | CoreStatementKind::Break
                | CoreStatementKind::Continue => {}
                _ => return Err(Stop::Unsupported),
            }
        }
        Ok(())
    }

    fn expr(&mut self, expression: &CoreExpr, depth: usize) -> Probe {
        self.charge(depth, expression.span)?;
        match &expression.kind {
            CoreExprKind::Ascribed { ty, value }
            | CoreExprKind::IntegerConvert { target: ty, value } => {
                self.ty(ty, depth + 1, expression.span)?;
                self.expr(value, depth + 1)?;
            }
            CoreExprKind::Unary { operand, .. } => self.expr(operand, depth + 1)?,
            CoreExprKind::Binary { left, right, .. } => {
                self.expr(left, depth + 1)?;
                self.expr(right, depth + 1)?;
            }
            CoreExprKind::Call { callee, arguments } => {
                self.expr(callee, depth + 1)?;
                for argument in arguments {
                    self.expr(argument, depth + 1)?;
                }
            }
            CoreExprKind::Member { object, .. } => self.expr(object, depth + 1)?,
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expr(condition, depth + 1)?;
                self.expr(then_value, depth + 1)?;
                self.expr(else_value, depth + 1)?;
            }
            CoreExprKind::Integer(_)
            | CoreExprKind::Bool(_)
            | CoreExprKind::Unit
            | CoreExprKind::Name(_)
            | CoreExprKind::Literal(
                Constant::Unit
                | Constant::Bool(_)
                | Constant::I32(_)
                | Constant::I64(_)
                | Constant::U8(_)
                | Constant::U16(_),
            ) => {}
            _ => return Err(Stop::Unsupported),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;
    use zil_base::{FileId, Fingerprint, ModuleId, NodeId};

    fn span(offset: u32) -> Span {
        Span::new(FileId(0), offset as usize, offset as usize + 1)
    }
    fn expr(offset: u32, kind: CoreExprKind) -> CoreExpr {
        CoreExpr {
            id: NodeId(offset),
            kind,
            span: span(offset),
        }
    }
    fn statement(offset: u32, kind: CoreStatementKind) -> CoreStatement {
        CoreStatement {
            id: NodeId(offset),
            kind,
            span: span(offset),
        }
    }
    fn scalar_program() -> CoreProgram {
        let value = expr(
            20,
            CoreExprKind::If {
                condition: Box::new(expr(21, CoreExprKind::Bool(true))),
                then_value: Box::new(expr(
                    22,
                    CoreExprKind::Binary {
                        op: BinaryOp::Add,
                        left: Box::new(expr(23, CoreExprKind::Name("x".into()))),
                        right: Box::new(expr(
                            24,
                            CoreExprKind::IntegerConvert {
                                target: Type::I32,
                                value: Box::new(expr(25, CoreExprKind::Literal(Constant::U16(2)))),
                            },
                        )),
                    },
                )),
                else_value: Box::new(expr(
                    26,
                    CoreExprKind::Ascribed {
                        ty: Type::I32,
                        value: Box::new(expr(27, CoreExprKind::Integer("0".into()))),
                    },
                )),
            },
        );
        CoreProgram {
            root: ModuleId(0),
            source_fingerprint: Fingerprint::ZERO,
            modules: vec![ProgramModule {
                imports: vec![],
                exports: vec![],
                core: CoreModule {
                    schema_version: SCHEMA_VERSION,
                    module_id: ModuleId(0),
                    source_fingerprint: Fingerprint::ZERO,
                    aggregates: vec![],
                    aggregate_templates: vec![],
                    aggregate_applications: vec![],
                    enums: vec![],
                    unsupported: vec![],
                    functions: vec![CoreFunction {
                        id: NodeId(1),
                        name: "result".into(),
                        type_parameters: vec![],
                        callable_parameters: vec![],
                        state_parameters: vec![],
                        callable_return: None,
                        foreign: None,
                        receiver: None,
                        parameters: vec![CoreParameter {
                            name: "x".into(),
                            ty: Type::I32,
                            mode: PassingMode::Infer,
                            span: span(2),
                        }],
                        return_type: Type::I32,
                        result_mode: PassingMode::Infer,
                        span: span(1),
                        body: CoreBlock {
                            span: span(3),
                            statements: vec![
                                statement(
                                    4,
                                    CoreStatementKind::Binding {
                                        name: "i".into(),
                                        mutable: true,
                                        ty: Some(Type::I32),
                                        initializer: Some(expr(
                                            5,
                                            CoreExprKind::Integer("0".into()),
                                        )),
                                    },
                                ),
                                statement(
                                    6,
                                    CoreStatementKind::While {
                                        condition: expr(7, CoreExprKind::Bool(false)),
                                        body: CoreBlock {
                                            span: span(8),
                                            statements: vec![statement(
                                                9,
                                                CoreStatementKind::Break,
                                            )],
                                        },
                                    },
                                ),
                                statement(19, CoreStatementKind::Return(Some(value))),
                            ],
                        },
                    }],
                },
            }],
        }
    }

    fn compare(program: &CoreProgram, limits: &Limits, selected: bool) {
        let expected = aggregates::resolve_program(program, limits);
        if selected {
            assert!(validate(program, limits).is_some());
        }
        let actual = aggregates::resolve_prepared_program(program, limits);
        if let Ok(Cow::Borrowed(borrowed)) = &actual {
            assert!(std::ptr::eq(*borrowed, program));
            assert_eq!(expected.as_ref().unwrap(), program);
        }
        assert_eq!(actual.map(Cow::into_owned), expected);
    }

    #[test]
    fn scalar_shapes_borrow_the_complete_current_input() {
        let program = scalar_program();
        let limits = Limits::default();
        assert!(matches!(
            aggregates::resolve_prepared_program(&program, &limits),
            Ok(Cow::Borrowed(_))
        ));
        compare(&program, &limits, true);
        let target = TargetProfile::default();
        let prepared = infer_program_for_ct(&program, &limits)
            .unwrap()
            .into_prepared();
        assert_eq!(
            check_prepared_program(&prepared, &target, &limits),
            check_program(&program, &target, &limits),
        );
    }

    #[test]
    fn scalar_shapes_preserve_exact_work_depth_and_error_spans() {
        let mut program = scalar_program();
        // The old remaining work is shared across modules, not reset per module.
        let mut second = program.modules[0].clone();
        second.core.module_id = ModuleId(1);
        program.modules.push(second);
        for depth in [false, true] {
            let limits = |n| {
                let mut limits = Limits::default();
                if depth {
                    limits.max_call_depth = n;
                } else {
                    limits.max_analysis_steps = n;
                }
                limits
            };
            let (mut low, mut high) = (0, 1);
            while aggregates::resolve_program(&program, &limits(high)).is_err() {
                high *= 2;
                assert!(high < 1024);
            }
            while low + 1 < high {
                let middle = (low + high) / 2;
                if aggregates::resolve_program(&program, &limits(middle)).is_ok() {
                    high = middle;
                } else {
                    low = middle;
                }
            }
            for n in [0, high - 1, high, high + 1] {
                compare(&program, &limits(n), true);
            }
            let errors =
                aggregates::resolve_prepared_program(&program, &limits(high - 1)).unwrap_err();
            assert_eq!(errors[0].message, "aggregate resolution budget exceeded");
        }
    }

    #[test]
    fn scalar_shapes_restart_original_resolution_for_nearby_shapes() {
        let base = scalar_program();
        let mut cases = Vec::new();
        for mode in [PassingMode::Create, PassingMode::Give] {
            let mut program = base.clone();
            program.modules[0].core.functions[0].result_mode = mode;
            cases.push(program);
        }
        let mut parameter = base.clone();
        parameter.modules[0].core.functions[0].parameters[0].mode = PassingMode::Give;
        cases.push(parameter);
        let mut named = base.clone();
        named.modules[0].core.functions[0].return_type = Type::Named("Missing".into());
        cases.push(named);
        let mut aggregate = base.clone();
        aggregate.modules[0].core.aggregates.push(CoreAggregate {
            id: NodeId(40),
            name: "Empty".into(),
            kind: AggregateKind::Struct,
            teardown: None,
            fields: vec![],
            span: span(40),
        });
        cases.push(aggregate);
        let mut generic = base.clone();
        generic.modules[0].core.functions[0]
            .type_parameters
            .push(CoreTypeParameter {
                index: 0,
                name: "T".into(),
                bound: CoreTypeBound::StoreElement,
            });
        cases.push(generic);
        // Decline after an already charged valid prefix; no speculative work
        // may be deducted from the fallback's allowance.
        let mut closure = base.clone();
        closure.modules[0].core.functions[0]
            .body
            .statements
            .push(statement(
                41,
                CoreStatementKind::Expression(expr(
                    42,
                    CoreExprKind::Closure {
                        move_capture: false,
                        parameters: vec![],
                        return_type: Some(Type::I32),
                        body: CoreBlock {
                            span: span(43),
                            statements: vec![],
                        },
                    },
                )),
            ));
        cases.push(closure);
        for program in cases {
            assert!(validate(&program, &Limits::default()).is_none());
            compare(&program, &Limits::default(), false);
            for work in [0, 2, 9] {
                compare(
                    &program,
                    &Limits {
                        max_analysis_steps: work,
                        ..Limits::default()
                    },
                    false,
                );
            }
        }
    }

    #[test]
    fn scalar_shapes_do_not_skip_current_type_errors() {
        let base = scalar_program();
        let mut cases = Vec::new();
        let mut schema = base.clone();
        schema.modules[0].core.schema_version = 0;
        cases.push(schema);
        let mut conversion = base.clone();
        conversion.modules[0].core.functions[0].body.statements = vec![statement(
            45,
            CoreStatementKind::Return(Some(expr(
                46,
                CoreExprKind::IntegerConvert {
                    target: Type::Bool,
                    value: Box::new(expr(47, CoreExprKind::Integer("1".into()))),
                },
            ))),
        )];
        cases.push(conversion);
        let mut unknown = base;
        unknown.modules[0].core.functions[0].body.statements = vec![statement(
            48,
            CoreStatementKind::Return(Some(expr(49, CoreExprKind::Name("absent".into())))),
        )];
        cases.push(unknown);
        let limits = Limits::default();
        let target = TargetProfile::default();
        for program in cases {
            let old = aggregates::resolve_program(&program, &limits).unwrap();
            let current = aggregates::resolve_prepared_program(&program, &limits).unwrap();
            assert!(matches!(current, Cow::Borrowed(_)));
            let expected = program::check_resolved_program(&old, &target, &limits).unwrap_err();
            assert_eq!(
                program::check_resolved_program(&current, &target, &limits).unwrap_err(),
                expected
            );
        }
    }
}
