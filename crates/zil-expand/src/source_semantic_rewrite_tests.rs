//! Exercise the actual Engine and substitution entry points, not a parallel
//! semantic walker. Tokens below are private test events, never accepted raw
//! source input or permission evidence.
use super::*;
use zil_base::ModuleId;

fn parsed_function(text: &str) -> Function {
    let source = zil_base::SourceUnit::new(
        zil_base::FileId(0),
        ModuleId(0),
        "source-boundary.zil",
        text,
    );
    let mut syntax = zil_syntax::parse(&source).into_result().unwrap();
    let ItemKind::Function(function) = syntax.items.remove(0).kind else {
        panic!("original function")
    };
    function
}

#[test]
fn callable_sync_drop_remains_an_independent_explicit_premise() {
    let span = Span::default();
    let module = Module {
        id: ModuleId(0),
        items: vec![],
        span,
    };
    let limits = Limits::default();
    let mut budget = Budget::new(&limits);
    let traits = trait_bounds::Index::new(&module, &mut budget).unwrap();
    for (bounds, ordinal) in [
        ("SyncDrop + Fn(i32)->i32 + ClosedEnvironment", 1),
        ("Fn(i32)->i32 + ClosedEnvironment + SyncDrop", 0),
    ] {
        let function = parsed_function(&format!("fn helper<F:{bounds}>(f:&F)->i32{{return 0}}"));
        let roles = callable::template(&function, &mut budget).unwrap().unwrap();
        assert_eq!(roles.roles[0].source_bound_ordinal, ordinal);
        let available = traits
            .parameter_bounds_except(&function.generics, &mut budget, Some(&roles))
            .unwrap();
        assert!(matches!(
            available.get("F").unwrap().as_slice(),
            [trait_bounds::Bound::SyncDrop]
        ));
        let found = BTreeMap::from([("F".into(), named("F", span))]);
        let variables = BTreeSet::from(["F".into()]);
        // This is the real retained-template boundary: F has no actual entry
        // yet. It leaves the original obligation unresolved, without creating
        // a fake F->F actual or adding SyncDrop to the caller's premises.
        traits
            .check_arguments(
                &function.generics,
                &BTreeMap::new(),
                (
                    &trait_bounds::Bounds::new(),
                    &BTreeSet::new(),
                    callable::StoreCallbacks::None,
                ),
                &mut budget,
                span,
                Some(&roles),
            )
            .unwrap();
        traits
            .check_arguments(
                &function.generics,
                &found,
                (&available, &variables, callable::StoreCallbacks::None),
                &mut budget,
                span,
                Some(&roles),
            )
            .unwrap();
        assert!(
            traits
                .check_arguments(
                    &function.generics,
                    &found,
                    (
                        &trait_bounds::Bounds::new(),
                        &variables,
                        callable::StoreCallbacks::None
                    ),
                    &mut budget,
                    span,
                    Some(&roles),
                )
                .is_err()
        );
    }
    let function =
        parsed_function("fn helper<F:Fn(i32)->i32 + ClosedEnvironment>(f:&F)->i32{return 0}");
    let roles = callable::template(&function, &mut budget).unwrap().unwrap();
    let available = traits
        .parameter_bounds_except(&function.generics, &mut budget, Some(&roles))
        .unwrap();
    assert!(available.get("F").is_none_or(Vec::is_empty));
    let duplicate =
        parsed_function("fn helper<F:Fn(i32)->i32 + SyncDrop + SyncDrop>(f:&F)->i32{return 0}");
    assert!(callable::template(&duplicate, &mut budget).is_err());
}

#[test]
fn normalized_registration_owner_is_not_a_descriptor_slot() {
    let function = parsed_function(
        "fn close<F:FnMut(i32)->i32 + ClosedEnvironment>(owner:&mut AsyncRegistration<F>)->i32 contract{requires(state(owner,Closing,Closed));}{return 0}",
    );
    let TypeKind::Reference { inner, .. } = &function.params[0].ty.kind else {
        panic!()
    };
    let raw = inner.as_ref().clone();
    for shell in [crate::receivers::REFERENCE_OWNER, "Managed"] {
        let mut input = function.params[0].ty.clone();
        let TypeKind::Reference { inner, .. } = &mut input.kind else {
            panic!()
        };
        **inner = TypeRef {
            span: raw.span,
            kind: TypeKind::Named {
                path: vec![shell.into()],
                args: vec![GenericArg::Type(raw.clone())],
            },
        };
        assert!(callable::borrowed_registration(&input, "F"));
        let mut normalized = function.clone();
        normalized.params[0].ty = input.clone();
        let limits = Limits::default();
        assert!(callable::template(&normalized, &mut Budget::new(&limits)).is_ok());
    }
    for shell in ["Ref", "Desc", crate::receivers::VALUE_DESCRIPTION] {
        let mut descriptor = function.params[0].ty.clone();
        let TypeKind::Reference { inner, .. } = &mut descriptor.kind else {
            panic!()
        };
        **inner = TypeRef {
            span: raw.span,
            kind: TypeKind::Named {
                path: vec![shell.into()],
                args: vec![GenericArg::Type(raw.clone())],
            },
        };
        assert!(!callable::borrowed_registration(&descriptor, "F"));
    }
}

#[test]
fn resource_borrow_hints_match_normalized_headers_without_changing_descriptors() {
    let source = zil_base::SourceUnit::new(
        zil_base::FileId(0),
        ModuleId(0),
        "resource-borrow-hints.zil",
        "resource File{code:i32}",
    );
    let module = zil_syntax::parse(&source).into_result().unwrap();
    let limits = Limits::default();
    let patterns = BTreeMap::new();
    let mut engine = Engine::with_budget(
        &module,
        &limits,
        &patterns,
        None,
        &[],
        Budget::new(&limits),
        None,
    )
    .unwrap();
    let span = module.span;
    let mut env = Env::new();
    env.insert("file".into(), Some(named("File", span)));
    for mutable in [false, true] {
        let mut expression = Expr {
            span,
            semantic: Default::default(),
            kind: ExprKind::Unary {
                op: if mutable {
                    UnaryOp::BorrowMut
                } else {
                    UnaryOp::Borrow
                },
                operand: Box::new(Expr {
                    span,
                    semantic: Default::default(),
                    kind: ExprKind::Name("file".into()),
                }),
            },
        };
        let hinted = engine
            .value_hint(&expression, &env, 0, false)
            .unwrap()
            .unwrap();
        let emitted = engine.expr(&mut expression, &mut env, 0).unwrap().unwrap();
        assert_eq!(hinted, emitted);
        let mut header = parsed_function(if mutable {
            "fn f(file:&mut File){}"
        } else {
            "fn f(file:&File){}"
        });
        engine.preserve_resource_header(&mut header).unwrap();
        unify_argument(
            &header.params[0].ty,
            &hinted,
            &BTreeSet::new(),
            &mut BTreeMap::new(),
            span,
            &mut engine.budget,
        )
        .unwrap();
        let TypeKind::Reference { inner, .. } = hinted.kind else {
            panic!()
        };
        assert!(
            matches!(inner.kind, TypeKind::Named { path, .. } if path.as_slice()==[crate::receivers::REFERENCE_OWNER])
        );
    }
    let desc = TypeRef {
        span,
        kind: TypeKind::Named {
            path: vec![crate::receivers::VALUE_DESCRIPTION.into()],
            args: vec![GenericArg::Type(named("File", span))],
        },
    };
    let mut desc = desc;
    engine.preserve_resource_reference(&mut desc).unwrap();
    assert!(
        matches!(desc.kind,TypeKind::Named{path,..} if path.as_slice()==[crate::receivers::VALUE_DESCRIPTION])
    );
}

#[test]
fn actual_task_poll_preserves_its_error_union_through_the_owner_category() {
    let span = Span::default();
    let module = Module {
        id: ModuleId(0),
        items: vec![],
        span,
    };
    let limits = Limits::default();
    let patterns = BTreeMap::new();
    let mut engine = Engine::with_budget(
        &module,
        &limits,
        &patterns,
        None,
        &[],
        Budget::new(&limits),
        None,
    )
    .unwrap();
    let task = TypeRef {
        span,
        kind: TypeKind::Named {
            path: vec!["Task".into()],
            args: vec![GenericArg::Type(named("i32", span))],
        },
    };
    let mut env = Env::new();
    env.insert("task".into(), Some(task));
    let mut expression = Expr {
        span,
        semantic: Default::default(),
        kind: ExprKind::Call {
            callee: Box::new(Expr {
                span,
                semantic: Default::default(),
                kind: ExprKind::Name("task_poll".into()),
            }),
            arguments: vec![Argument {
                span,
                spread: false,
                value: Expr {
                    span,
                    semantic: Default::default(),
                    kind: ExprKind::Unary {
                        op: UnaryOp::BorrowMut,
                        operand: Box::new(Expr {
                            span,
                            semantic: Default::default(),
                            kind: ExprKind::Name("task".into()),
                        }),
                    },
                },
            }],
        },
    };
    let ty = engine.expr(&mut expression, &mut env, 0).unwrap().unwrap();
    assert!(matches!(ty.kind, TypeKind::Error { .. }));
}

#[test]
fn real_engine_rewrite_keeps_root_and_child_bundles_on_their_actual_nodes() {
    let span = Span::default();
    let module = Module {
        id: ModuleId(0),
        items: Vec::new(),
        span,
    };
    let limits = Limits::default();
    let patterns = BTreeMap::new();
    let mut engine = Engine::with_budget(
        &module,
        &limits,
        &patterns,
        None,
        &[],
        Budget::new(&limits),
        None,
    )
    .unwrap();
    let mut expression = Expr {
        kind: ExprKind::Tuple(vec![
            Expr {
                kind: ExprKind::Literal(Literal::Integer("20".into())),
                span,
                semantic: SemanticContextMarker::untrusted(2),
            },
            Expr {
                kind: ExprKind::Literal(Literal::Integer("22".into())),
                span,
                semantic: SemanticContextMarker::untrusted(3),
            },
        ]),
        span,
        semantic: SemanticContextMarker::untrusted(1),
    };
    engine.expr(&mut expression, &mut Env::new(), 0).unwrap();
    assert_eq!(expression.semantic.raw_index(), Some(1));
    let ExprKind::Tuple(children) = &expression.kind else {
        panic!()
    };
    assert_eq!(children[0].semantic.raw_index(), Some(2));
    assert_eq!(children[1].semantic.raw_index(), Some(3));
}

#[test]
fn const_substitution_changes_syntax_without_importing_a_declaration_root_event() {
    let span = Span::default();
    let literal = Expr {
        kind: ExprKind::Literal(Literal::Integer("42".into())),
        span,
        semantic: Default::default(),
    };
    let substitutions = BTreeMap::from([(
        "N".to_owned(),
        TypeRef {
            kind: TypeKind::MetaEval(Box::new(literal)),
            span,
        },
    )]);
    let mut expression = Expr {
        kind: ExprKind::Name("N".into()),
        span,
        semantic: SemanticContextMarker::untrusted(7),
    };
    Substitution {
        substitutions: &substitutions,
    }
    .expr(&mut expression);
    assert_eq!(expression.semantic.raw_index(), Some(7));
    assert!(
        matches!(&expression.kind, ExprKind::Literal(Literal::Integer(value)) if value == "42")
    );
}
