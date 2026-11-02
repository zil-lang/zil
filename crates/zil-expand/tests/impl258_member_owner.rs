//! Untrusted parser-shaped AST fixture; owner attachment is normally performed
//! by project CT materialization. These tests do not claim CT execution.
use zil_base::{FileId, Limits, ModuleId, SourceUnit, Span};
use zil_ir::{CoreExprKind as C, CoreStatementKind as S, Type};
use zil_syntax::ast as s;
fn parsed(text: &str) -> (SourceUnit, s::Module) {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "owner.zil", text);
    let module = zil_syntax::parse(&source).into_result().unwrap();
    (source, module)
}
fn named(name: &str, span: Span) -> s::TypeRef {
    s::TypeRef {
        kind: s::TypeKind::Named {
            path: vec![name.into()],
            args: vec![],
        },
        span,
    }
}
fn return_member(module: &mut s::Module, index: usize) -> &mut s::MemberName {
    let s::ItemKind::Function(f) = &mut module.items[index].kind else {
        panic!()
    };
    let s::FunctionBody::Block(b) = &mut f.body else {
        panic!()
    };
    let s::StmtKind::Return(Some(s::Expr {
        kind: s::ExprKind::Member { member, .. },
        ..
    })) = &mut b.statements[0].kind
    else {
        panic!()
    };
    member
}
fn attach(member: &mut s::MemberName, owner: s::TypeRef) {
    let s::MemberName::Name(name) = member else {
        panic!()
    };
    *member = s::MemberName::Constrained {
        name: name.clone(),
        owner: Box::new(owner),
    }
}
fn core_return(core: &zil_ir::CoreFunction) -> &zil_ir::CoreExpr {
    let S::Return(Some(e)) = &core.body.statements[0].kind else {
        panic!()
    };
    e
}
#[test]
fn owner_is_preserved_on_original_receiver_and_ordinary_members_remain_unconstrained() {
    let (source, mut module) = parsed("struct A{pub x:i32}\nfn read(a:A)->i32{return a.x}");
    let ordinary = zil_expand::expand(&source, &module).unwrap();
    assert!(matches!(
        &core_return(&ordinary.functions[0]).kind,
        C::Member { owner: None, .. }
    ));
    attach(return_member(&mut module, 1), named("A", source.span()));
    let core = zil_expand::expand(&source, &module).unwrap();
    let C::Member {
        object,
        member,
        owner,
    } = &core_return(&core.functions[0]).kind
    else {
        panic!()
    };
    assert_eq!(member, "x");
    assert_eq!(owner, &Some(Type::Named("A".into())));
    assert!(matches!(&object.kind,C::Name(n) if n=="a"));
}
#[test]
fn generic_owner_type_is_substituted_and_monomorphized_with_the_real_nominal() {
    let (source, mut module) = parsed(
        "struct Wrap<T>{pub value:T}\nfn read<T:Copy>(w:Wrap<T>)->T{return w.value}\nfn main()->i32{return read(Wrap::<i32>{value:7})}",
    );
    let span = source.span();
    let owner = s::TypeRef {
        span,
        kind: s::TypeKind::Named {
            path: vec!["Wrap".into()],
            args: vec![s::GenericArg::Type(named("T", span))],
        },
    };
    attach(return_member(&mut module, 1), owner);
    let core = zil_expand::expand(&source, &module).unwrap();
    let read = core
        .functions
        .iter()
        .find(|f| f.name.contains("read"))
        .unwrap();
    let C::Member {
        owner: Some(owner), ..
    } = &core_return(read).kind
    else {
        panic!()
    };
    assert_eq!(owner, &read.parameters[0].ty);
    assert!(matches!(owner,Type::Named(n) if n!="Wrap"));
}
#[test]
fn unused_generic_definition_cannot_substitute_same_shaped_wrong_owner() {
    let (source, mut module) = parsed(
        "struct A{pub x:i32}\nstruct B{pub x:i32}\nfn unused<T:Copy>(a:A,v:T)->i32{return a.x}\nfn main()->i32{return 0}",
    );
    let mut positive = module.clone();
    attach(return_member(&mut positive, 2), named("A", source.span()));
    assert!(zil_expand::expand(&source, &positive).is_ok());
    attach(return_member(&mut module, 2), named("B", source.span()));
    let errors = zil_expand::expand(&source, &module).unwrap_err();
    assert!(errors.iter().any(
        |error| error.message == "generic body types must agree before concrete instantiation"
    ));
}
#[test]
fn selection_budget_and_remap_visit_the_complete_nested_owner_type() {
    let (source, mut module) = parsed("fn read(a:i32)->i32{return a.x}");
    let span = source.span();
    let mut owner = named("A", span);
    for _ in 0..4 {
        owner = s::TypeRef {
            span,
            kind: s::TypeKind::Tuple(vec![owner, named("i32", span)]),
        };
    }
    attach(return_member(&mut module, 0), owner);
    let s::ItemKind::Function(f) = &module.items[0].kind else {
        panic!()
    };
    let s::FunctionBody::Block(b) = &f.body else {
        panic!()
    };
    let s::StmtKind::Return(Some(e)) = &b.statements[0].kind else {
        panic!()
    };
    let mut fragment = zil_syntax::GeneratedSyntax::Expr(e.clone());
    let n = zil_expand::remap_generated_spans(
        &mut fragment,
        Span::new(FileId(7), 3, 6),
        &Limits::default(),
    )
    .unwrap();
    assert!(
        zil_expand::remap_generated_spans(
            &mut zil_syntax::GeneratedSyntax::Expr(e.clone()),
            span,
            &Limits {
                max_analysis_steps: n - 1,
                ..Limits::default()
            }
        )
        .is_err()
    );
    let zil_syntax::GeneratedSyntax::Expr(s::Expr {
        kind:
            s::ExprKind::Member {
                member: s::MemberName::Constrained { owner, .. },
                ..
            },
        ..
    }) = fragment
    else {
        panic!()
    };
    assert_eq!(owner.span.file, FileId(7));
    let cost = zil_expand::ct_selection::expression_cost(e, &Limits::default()).unwrap();
    assert!(cost.nodes > 10);
    assert!(
        zil_expand::ct_selection::expression_cost(
            e,
            &Limits {
                max_nodes: cost.nodes - 1,
                ..Limits::default()
            }
        )
        .is_err()
    );
}
#[test]
fn transparent_owner_alias_is_normalized_with_parameter_type() {
    let (source, mut module) =
        parsed("struct A{pub x:i32}\ntype Owner=A\nfn read(a:Owner)->i32{return a.x}");
    attach(return_member(&mut module, 2), named("Owner", source.span()));
    let core = zil_expand::expand(&source, &module).unwrap();
    let C::Member {
        owner: Some(owner), ..
    } = &core_return(&core.functions[0]).kind
    else {
        panic!()
    };
    assert_eq!(owner, &core.functions[0].parameters[0].ty);
    assert_eq!(owner, &Type::Named("A".into()));
}
#[test]
fn module_qualified_owner_keeps_same_type_identity_as_parameter() {
    let sources = [
        SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "model.zil",
            "pub struct A{pub x:i32}",
        ),
        SourceUnit::new(
            FileId(1),
            ModuleId(1),
            "main.zil",
            "import model as m\nfn read(a:m.A)->i32{return a.x}",
        ),
    ];
    let mut modules = sources
        .iter()
        .map(|s| zil_syntax::parse(s).into_result().unwrap())
        .collect::<Vec<_>>();
    let span = sources[1].span();
    attach(
        return_member(&mut modules[1], 1),
        s::TypeRef {
            span,
            kind: s::TypeKind::Named {
                path: vec!["m".into(), "A".into()],
                args: vec![],
            },
        },
    );
    let output = zil_expand::expand_program_with_limits(
        &sources,
        &modules.iter().collect::<Vec<_>>(),
        &[
            vec![],
            vec![zil_ir::CoreImport {
                alias: "m".into(),
                module: ModuleId(0),
                span,
            }],
        ],
        &Limits::default(),
    )
    .unwrap();
    let read = &output[1].0.core.functions[0];
    let C::Member {
        owner: Some(owner), ..
    } = &core_return(read).kind
    else {
        panic!()
    };
    assert_eq!(owner, &read.parameters[0].ty);
    assert_eq!(owner, &Type::Named("$module0_A".into()));
}
#[test]
fn compound_place_preserves_owner_on_target_and_read_without_ascription() {
    let (source, mut module) = parsed("struct A{pub x:i32}\nfn update(a:&mut A){a.x+=1}");
    let s::ItemKind::Function(f) = &mut module.items[1].kind else {
        panic!()
    };
    let s::FunctionBody::Block(b) = &mut f.body else {
        panic!()
    };
    let s::StmtKind::Assign {
        target:
            s::Expr {
                kind: s::ExprKind::Member { member, .. },
                ..
            },
        ..
    } = &mut b.statements[0].kind
    else {
        panic!()
    };
    attach(member, named("A", source.span()));
    let core = zil_expand::expand(&source, &module).unwrap();
    let S::Assign { target, value } = &core.functions[0].body.statements[0].kind else {
        panic!()
    };
    let C::Binary { left, .. } = &value.kind else {
        panic!()
    };
    for e in [target, left] {
        let C::Member {
            object,
            owner: Some(owner),
            ..
        } = &e.kind
        else {
            panic!()
        };
        assert_eq!(owner, &Type::Named("A".into()));
        assert!(matches!(&object.kind,C::Name(name) if name=="a"));
    }
}
fn boundary(mut accepts: impl FnMut(usize) -> bool) -> usize {
    let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
    assert!(accepts(high));
    while low < high {
        let mid = low + (high - low) / 2;
        if accepts(mid) {
            high = mid
        } else {
            low = mid + 1
        }
    }
    low
}
#[test]
fn single_module_owner_obeys_actual_remaining_work_n_minus_one_and_fresh() {
    let (source, mut module) = parsed("struct A{pub x:i32}\nfn read(a:A)->i32{return a.x}");
    attach(return_member(&mut module, 1), named("A", source.span()));
    let n = boundary(|n| {
        zil_expand::expand_with_map_limits(
            &source,
            &module,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .is_ok()
    });
    let limits = Limits {
        max_analysis_steps: n,
        ..Limits::default()
    };
    let expected = zil_expand::expand_with_map_limits(&source, &module, &limits)
        .unwrap()
        .core;
    assert!(
        zil_expand::expand_with_map_limits(
            &source,
            &module,
            &Limits {
                max_analysis_steps: n - 1,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .iter()
        .all(|e| e.category == zil_base::ResultCategory::ResourceLimit)
    );
    assert_eq!(
        zil_expand::expand_with_map_limits(&source, &module, &limits)
            .unwrap()
            .core,
        expected
    );
    println!("member owner single-module Expand work N={n}; N-1 ResourceLimit; fresh=N");
}
#[test]
fn project_owner_work_is_accumulated_across_modules() {
    let sources = [
        SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "a.zil",
            "struct A{pub x:i32}\nfn read(a:A)->i32{return a.x}",
        ),
        SourceUnit::new(
            FileId(1),
            ModuleId(1),
            "b.zil",
            "struct B{pub x:i32}\nfn read(b:B)->i32{return b.x}",
        ),
    ];
    let mut modules = sources
        .iter()
        .map(|s| zil_syntax::parse(s).into_result().unwrap())
        .collect::<Vec<_>>();
    for (index, name) in ["A", "B"].into_iter().enumerate() {
        attach(
            return_member(&mut modules[index], 1),
            named(name, sources[index].span()),
        );
    }
    let refs = modules.iter().collect::<Vec<_>>();
    let run = |n| {
        zil_expand::expand_program_with_limits(
            &sources,
            &refs,
            &[vec![], vec![]],
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
    };
    let n = boundary(|n| run(n).is_ok());
    let expected = run(n).unwrap();
    assert!(
        run(n - 1)
            .unwrap_err()
            .iter()
            .all(|e| e.category == zil_base::ResultCategory::ResourceLimit)
    );
    assert_eq!(
        run(n)
            .unwrap()
            .iter()
            .map(|(m, _)| m.core.clone())
            .collect::<Vec<_>>(),
        expected
            .iter()
            .map(|(m, _)| m.core.clone())
            .collect::<Vec<_>>()
    );
    println!("member owner two-module Expand work N={n}; N-1 ResourceLimit; fresh=N");
}
#[test]
fn self_owner_prepays_actual_host_name_before_substitution_and_clone() {
    let mut measured = Vec::new();
    for name in ["A", "NominalWithALongerActualDefinitionName"] {
        let (source, mut module) = parsed(&format!(
            "struct {name}{{pub x:i32;fn read(self:&Self)->i32{{return self.x}}}}"
        ));
        let s::ItemKind::Aggregate(a) = &mut module.items[0].kind else {
            panic!()
        };
        let s::ItemKind::Function(f) = &mut a.methods[0].kind else {
            panic!()
        };
        let s::FunctionBody::Block(b) = &mut f.body else {
            panic!()
        };
        let s::StmtKind::Return(Some(s::Expr {
            kind: s::ExprKind::Member { member, .. },
            ..
        })) = &mut b.statements[0].kind
        else {
            panic!()
        };
        attach(member, named("Self", source.span()));
        let n = boundary(|n| {
            zil_expand::expand_with_map_limits(
                &source,
                &module,
                &Limits {
                    max_analysis_steps: n,
                    ..Limits::default()
                },
            )
            .is_ok()
        });
        let core = zil_expand::expand_with_map_limits(
            &source,
            &module,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .unwrap()
        .core;
        let C::Member {
            owner: Some(Type::Named(owner)),
            object,
            ..
        } = &core_return(&core.functions[0]).kind
        else {
            panic!()
        };
        assert_eq!(owner, name);
        assert!(matches!(&object.kind,C::Name(n) if n=="self"));
        assert!(
            zil_expand::expand_with_map_limits(
                &source,
                &module,
                &Limits {
                    max_analysis_steps: n - 1,
                    ..Limits::default()
                }
            )
            .unwrap_err()
            .iter()
            .all(|e| e.category == zil_base::ResultCategory::ResourceLimit)
        );
        measured.push(n);
    }
    assert!(measured[1] > measured[0]);
    println!("member owner Self actual host work N={measured:?}; N-1 ResourceLimit");
}
