//! Raw unused Core definitions independently exercise the new source contract.
//! These checks do not claim concrete execution or TypedIR memory safety.
use std::{
    cell::Cell,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, NodeId, SourceUnit, Span, TargetProfile};
use zil_ir::*;

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Case(PathBuf, bool);
impl Case {
    fn new(label: &str) -> Self {
        let retained = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let root = retained
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = root.join(format!(
            "raw205-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path, retained.is_some())
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn program(core: CoreModule) -> CoreProgram {
    CoreProgram {
        root: core.module_id,
        source_fingerprint: core.source_fingerprint,
        modules: vec![ProgramModule {
            imports: vec![],
            exports: vec![],
            core,
        }],
    }
}
fn check(label: &str, core: &CoreModule, code: Option<&str>) {
    let case = Case::new(label);
    fs::write(case.0.join("core.txt"), format!("{core:#?}")).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let single = zil_types::check(core, &target, &limits);
    let project =
        zil_types::check_program(&program(core.clone()), &target, &limits).map(|p| p.module);
    fs::write(
        case.0.join("types.txt"),
        format!("single={single:#?}\nprogram={project:#?}"),
    )
    .unwrap();
    for result in [single, project] {
        if let Some(code) = code {
            let errors = result.expect_err(label);
            assert!(
                errors.iter().any(|e| e.code == code),
                "{label}: {errors:#?}"
            );
        } else {
            result.unwrap_or_else(|errors| panic!("{label}: {errors:#?}"));
        }
    }
}
fn template(kind: CallableKind, fields: usize, reversed: bool) -> (CoreModule, usize) {
    let second = if fields == 2 { "second:F;" } else { "" };
    let source = format!(
        "resource Ticket{{code:i32}}package State<F>{{manages marker:Ticket;first:F;{second}}}fn inspect<F:Fn()->i32>(state:State<F>)->State<F>{{var next=state;return next}}fn main()->i32{{return 0}}"
    );
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "raw205-base.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let mut core = zil_expand::expand(&unit, &syntax).unwrap();
    let index = core
        .functions
        .iter()
        .position(|f| !f.callable_parameters.is_empty())
        .unwrap();
    let function = &mut core.functions[index];
    let mut bound = function.callable_parameters.remove(0);
    bound.kind = kind;
    bound.index = 1;
    function.parameters.push(CoreParameter {
        name: "replacement".into(),
        ty: Type::Named(bound.generic_name.clone()),
        mode: PassingMode::Infer,
        span: function.span,
    });
    function.state_parameters.push(CoreStateParameter {
        index: 0,
        bindings: vec![CoreStateBinding {
            generic_index: 0,
            generic_name: bound.generic_name.clone(),
            kind,
            parameters: bound.parameters.clone(),
            result: bound.result.clone(),
            fields: (1..=fields as u32).collect(),
        }],
    });
    function.callable_parameters.push(bound);
    if reversed {
        function.parameters.swap(0, 1);
        function.state_parameters[0].index = 1;
        function.callable_parameters[0].index = 0;
    }
    (core, index)
}
struct Ast {
    next: Cell<u32>,
    span: Span,
}
impl Ast {
    fn new() -> Self {
        Self {
            next: Cell::new(960_000),
            span: Span::new(FileId(0), 0, 1),
        }
    }
    fn id(&self) -> NodeId {
        self.next.set(self.next.get() + 1);
        NodeId(self.next.get())
    }
    fn expr(&self, kind: CoreExprKind) -> CoreExpr {
        CoreExpr {
            id: self.id(),
            span: self.span,
            kind,
        }
    }
    fn name(&self, name: &str) -> CoreExpr {
        self.expr(CoreExprKind::Name(name.into()))
    }
    fn field(&self, field: &str) -> CoreExpr {
        self.expr(CoreExprKind::Member {
            object: Box::new(self.name("next")),
            member: field.into(),
        })
    }
    fn call(&self, callee: CoreExpr) -> CoreExpr {
        self.expr(CoreExprKind::Call {
            callee: Box::new(callee),
            arguments: vec![],
        })
    }
    fn statement(&self, kind: CoreStatementKind) -> CoreStatement {
        CoreStatement {
            id: self.id(),
            span: self.span,
            kind,
        }
    }
    fn bind(&self, name: &str, mutable: bool, value: CoreExpr) -> CoreStatement {
        self.statement(CoreStatementKind::Binding {
            name: name.into(),
            mutable,
            ty: None,
            initializer: Some(value),
        })
    }
    fn assign(&self, field: &str, value: CoreExpr) -> CoreStatement {
        self.statement(CoreStatementKind::Assign {
            target: self.field(field),
            value,
        })
    }
    fn use_value(&self, value: CoreExpr) -> CoreStatement {
        self.statement(CoreStatementKind::Expression(value))
    }
    fn borrow(&self, field: &str, mutable: bool) -> CoreExpr {
        self.expr(CoreExprKind::Unary {
            op: if mutable {
                UnaryOp::BorrowMut
            } else {
                UnaryOp::Borrow
            },
            operand: Box::new(self.field(field)),
        })
    }
    fn finish(&self) -> CoreStatement {
        self.statement(CoreStatementKind::Return(Some(self.name("next"))))
    }
}

#[test]
fn unused_headers_require_distinct_owned_parameters_and_complete_selectors() {
    for fields in [1, 2] {
        for reversed in [false, true] {
            let (core, _) = template(CallableKind::Fn, fields, reversed);
            check(&format!("header-control-{fields}-{reversed}"), &core, None);
        }
    }
    let (base, index) = template(CallableKind::Fn, 2, false);
    for attack in 0..15 {
        let mut core = base.clone();
        let f = &mut core.functions[index];
        match attack {
            0 => f.callable_parameters[0].index = 0,
            1 => f.callable_parameters[0].index = 99,
            2 => f.state_parameters[0].bindings[0].fields = vec![1],
            3 => f.state_parameters[0].bindings[0].fields = vec![1, 1],
            4 => f.state_parameters[0].bindings[0].fields = vec![2, 1],
            5 => f.state_parameters[0].bindings[0].fields = vec![0, 2],
            6 => f.state_parameters[0].bindings[0].fields = vec![1, 2, 3],
            7 => f.callable_parameters[0].kind = CallableKind::FnOnce,
            8 => f.callable_parameters[0].generic_index = 1,
            9 => f.callable_parameters[0].parameters.push(Type::I32),
            10 => f.callable_parameters[0].result = Type::I64,
            11 => {
                f.parameters[1].ty = Type::Address {
                    mutable: false,
                    inner: Box::new(f.parameters[1].ty.clone()),
                }
            }
            12 => f.parameters[0].mode = PassingMode::Describe,
            13 => f.return_type = Type::Unit,
            14 => {
                let mut extra = f.parameters[0].clone();
                extra.name = "second_state".into();
                f.parameters.push(extra);
            }
            _ => unreachable!(),
        }
        check(
            &format!("header-attack-{attack}"),
            &core,
            Some("E-STATE-PARAMETERS"),
        );
    }
}

#[test]
fn unused_once_positions_refill_without_merging_other_fields_or_replacement() {
    for reversed in [false, true] {
        let (mut core, index) = template(CallableKind::FnOnce, 2, reversed);
        let a = Ast::new();
        core.functions[index].body.statements = vec![
            a.bind("next", true, a.name("state")),
            a.use_value(a.call(a.field("first"))),
            a.assign("first", a.name("replacement")),
            a.finish(),
        ];
        check(&format!("once-refill-{reversed}"), &core, None);
        let a = Ast::new();
        core.functions[index].body.statements = vec![
            a.bind("next", true, a.name("state")),
            a.bind("old", false, a.field("first")),
            a.assign("first", a.name("replacement")),
            a.use_value(a.call(a.field("second"))),
            a.assign("second", a.name("old")),
            a.finish(),
        ];
        check(
            &format!("three-independent-positions-{reversed}"),
            &core,
            None,
        );
        let a = Ast::new();
        core.functions[index].body.statements = vec![
            a.bind("next", true, a.name("state")),
            a.use_value(a.call(a.field("first"))),
            a.use_value(a.call(a.field("first"))),
            a.assign("first", a.name("replacement")),
            a.finish(),
        ];
        check(
            &format!("same-field-twice-{reversed}"),
            &core,
            Some("E-STATE-MOVE"),
        );
        let a = Ast::new();
        core.functions[index].body.statements = vec![
            a.bind("next", true, a.name("state")),
            a.assign("first", a.name("replacement")),
            a.use_value(a.call(a.name("replacement"))),
            a.finish(),
        ];
        check(
            &format!("replacement-after-transfer-{reversed}"),
            &core,
            Some("E-STATE-MOVE"),
        );
    }
}

#[test]
fn unused_fn_bounds_never_supply_copy_for_a_field_or_direct_value() {
    for kind in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
        let (mut core, index) = template(kind, 2, false);
        let a = Ast::new();
        core.functions[index].body.statements = vec![
            a.bind("next", true, a.name("state")),
            a.bind("first", true, a.field("first")),
            a.assign("first", a.name("replacement")),
            a.use_value(a.call(a.name("first"))),
            a.finish(),
        ];
        check(&format!("new-field-local-{kind:?}"), &core, None);
        let a = Ast::new();
        core.functions[index].body.statements = vec![
            a.bind("next", true, a.name("state")),
            a.bind("first", true, a.field("first")),
            a.bind("second", false, a.field("first")),
            a.assign("first", a.name("replacement")),
            a.finish(),
        ];
        check(
            &format!("field-value-not-copy-{kind:?}"),
            &core,
            Some("E-STATE-MOVE"),
        );
        let a = Ast::new();
        core.functions[index].body.statements = vec![
            a.bind("next", true, a.name("state")),
            a.assign("first", a.name("replacement")),
            a.assign("second", a.name("replacement")),
            a.finish(),
        ];
        check(
            &format!("direct-value-not-copy-{kind:?}"),
            &core,
            Some("E-STATE-MOVE"),
        );
    }
}

#[test]
fn unused_aliases_keep_their_original_parent_across_refill() {
    for mutable in [false, true] {
        let (mut core, index) = template(
            if mutable {
                CallableKind::FnMut
            } else {
                CallableKind::Fn
            },
            2,
            false,
        );
        for sibling in [false, true] {
            let a = Ast::new();
            core.functions[index].body.statements = vec![
                a.bind("next", true, a.name("state")),
                a.bind(
                    "view",
                    false,
                    a.borrow(if sibling { "second" } else { "first" }, mutable),
                ),
                a.assign("first", a.name("replacement")),
                a.use_value(a.call(a.name("view"))),
                a.finish(),
            ];
            check(
                &format!("field-alias-{mutable}-{sibling}"),
                &core,
                if sibling { None } else { Some("E-STATE-ALIAS") },
            );
        }
        let a = Ast::new();
        core.functions[index].body.statements = vec![
            a.bind("next", true, a.name("state")),
            a.bind("view", false, a.borrow("first", mutable)),
            a.use_value(a.call(a.name("view"))),
            a.assign("first", a.name("replacement")),
            a.finish(),
        ];
        check(
            &format!("last-use-before-replacement-{mutable}"),
            &core,
            None,
        );
    }
}

#[test]
fn legacy_loop_refresh_checks_current_locals_and_keeps_stale_copies_invalid() {
    let source = "package State<F>{manages bytes:ByteBuffer;a:F;b:F}fn recycle<F:Fn()->i32>(state:State<F>,flag:bool)->State<F>{var next=state;var view=&next.a;while view()==0 {let moved=next.a;next.a=moved;view=&next.a}view();return next}fn main(){}";
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "legacy-loop.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let base = zil_expand::expand(&unit, &syntax).unwrap();
    let index = base
        .functions
        .iter()
        .position(|f| !f.callable_parameters.is_empty())
        .unwrap();
    assert!(base.functions[index].state_parameters.is_empty());
    check("legacy-loop-current-refresh", &base, None);

    for variant in 0..7 {
        let mut core = base.clone();
        let a = Ast::new();
        let statements = &mut core.functions[index].body.statements;
        if variant <= 2 {
            statements.insert(2, a.bind("stale", false, a.name("view")));
        }
        if variant == 1 {
            statements.insert(statements.len() - 1, a.use_value(a.call(a.name("stale"))));
        }
        let body = statements
            .iter_mut()
            .find_map(|statement| match &mut statement.kind {
                CoreStatementKind::While { body, .. } => Some(body),
                _ => None,
            })
            .unwrap();
        match variant {
            // An unused copied address does not make a refreshed local stale.
            0 | 1 => {}
            // The copied local will be read again on the backedge.
            2 => body
                .statements
                .insert(0, a.use_value(a.call(a.name("stale")))),
            // No refresh leaves the actual next condition's local invalid.
            3 => {
                body.statements.pop().unwrap();
            }
            // A new borrow is not permission to survive another take/refill.
            4 => {
                body.statements.push(a.bind("again", false, a.field("a")));
                body.statements.push(a.assign("a", a.name("again")));
            }
            // One branch refreshes after its transfer; the other never moves.
            5 => {
                let then_body = body.clone();
                body.statements = vec![a.statement(CoreStatementKind::If {
                    condition: a.name("flag"),
                    then_body,
                    else_body: None,
                })];
            }
            // A continue backedge carries the freshly rebound local.
            6 => body
                .statements
                .push(a.statement(CoreStatementKind::Continue)),
            _ => unreachable!(),
        }
        check(
            &format!("legacy-loop-current-variant-{variant}"),
            &core,
            if matches!(variant, 0 | 5 | 6) {
                None
            } else {
                Some("E-STATE-ALIAS")
            },
        );
    }
}

#[test]
fn both_mixed_headers_are_charged_before_comparing_large_metadata() {
    for direct in [false, true] {
        for attack in 0..3 {
            let (mut core, index) = template(CallableKind::Fn, 2, false);
            let f = &mut core.functions[index];
            let (name, parameters, result) = if direct {
                let bound = &mut f.callable_parameters[0];
                (
                    &mut bound.generic_name,
                    &mut bound.parameters,
                    &mut bound.result,
                )
            } else {
                let bound = &mut f.state_parameters[0].bindings[0];
                (
                    &mut bound.generic_name,
                    &mut bound.parameters,
                    &mut bound.result,
                )
            };
            let limits = match attack {
                0 => {
                    *name = "caller_source".repeat(1024);
                    Limits {
                        max_analysis_steps: 2048,
                        ..Limits::default()
                    }
                }
                1 => {
                    *parameters = vec![Type::I32; 1024];
                    Limits {
                        max_analysis_steps: 2048,
                        max_locals: 64,
                        ..Limits::default()
                    }
                }
                _ => {
                    *result = Type::I32;
                    for _ in 0..16 {
                        *result = Type::Tuple(vec![result.clone()]);
                    }
                    Limits {
                        max_call_depth: 8,
                        ..Limits::default()
                    }
                }
            };
            let case = Case::new(&format!("header-budget-{direct}-{attack}"));
            fs::write(case.0.join("core.txt"), format!("{core:#?}")).unwrap();
            let target = TargetProfile::host_llvm();
            let single = zil_types::check(&core, &target, &limits);
            let project = zil_types::check_program(&program(core.clone()), &target, &limits)
                .map(|p| p.module);
            fs::write(
                case.0.join("types.txt"),
                format!("limits={limits:#?}\nsingle={single:#?}\nprogram={project:#?}"),
            )
            .unwrap();
            for result in [single, project] {
                let errors = result.expect_err("oversized raw metadata");
                assert!(
                    errors.iter().any(|e| e.phase == zil_base::Phase::Types
                        && e.category == zil_base::ResultCategory::ResourceLimit),
                    "{direct}/{attack}: {errors:#?}"
                );
            }
        }
    }
}
