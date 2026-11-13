//! Exercise the actual Engine and substitution entry points, not a parallel
//! semantic walker. Tokens below are private test events, never accepted raw
//! source input or permission evidence.
use super::*;
use zil_base::ModuleId;

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
