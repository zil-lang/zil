//! The source CT adapter must reject dynamic storage before execution. Raw
//! shapes below test bounded traversal only, never language acceptance.
#[path = "../src/ct_program.rs"]
mod adapter;

use zil_base::{
    FileId, Limits, ModuleId, NodeId, Phase, ResultCategory, SourceUnit, Span, TargetProfile,
};
use zil_ct::SourceCompiler;
use zil_ir::*;

fn core() -> CoreModule {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "impl094-ct.zil",
        "fn pure()->i32{return 7;}",
    );
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    zil_expand::expand(&source, &syntax).unwrap()
}
fn program(core: CoreModule) -> CoreProgram {
    CoreProgram {
        root: core.module_id,
        source_fingerprint: core.source_fingerprint,
        modules: vec![ProgramModule {
            core,
            imports: vec![],
            exports: vec![],
        }],
    }
}
fn expression(kind: CoreExprKind) -> CoreExpr {
    CoreExpr {
        id: NodeId(1000),
        span: Span::default(),
        kind,
    }
}
fn limited(errors: Vec<zil_base::Diagnostic>) {
    assert_eq!(
        errors[0].category,
        ResultCategory::ResourceLimit,
        "{errors:#?}"
    );
}

#[test]
fn byte_payload_headers_are_traversed_and_materialized_with_exact_ct_cost() {
    let mut input = core();
    input.functions[0].return_type = Type::Desc(Box::new(Type::ByteBuffer));
    let target = TargetProfile::default();
    let (output, usage) =
        zil_ct::evaluate_source_constants_with_usage(&input, &target, &Limits::default()).unwrap();
    assert_eq!(output, input);
    let exact = Limits {
        max_nodes: usage.nodes,
        ..Limits::default()
    };
    assert!(zil_ct::evaluate_source_constants(&input, &target, &exact).is_ok());
    limited(
        zil_ct::evaluate_source_constants(
            &input,
            &target,
            &Limits {
                max_nodes: usage.nodes - 1,
                ..Limits::default()
            },
        )
        .unwrap_err(),
    );
    // A raw copy is still untrusted: the forged mismatching result must not be
    // confused with successful source typing or a CT-serializable hint.
    assert!(zil_types::check(&input, &target, &Limits::default()).is_err());
    println!("impl094 CT sealed header exact nodes={}", usage.nodes);
}

#[test]
fn unusable_byte_type_trees_are_bounded_before_cloning_or_query_execution() {
    let desc = Type::Desc(Box::new(Type::ByteBuffer));
    let shapes = [
        Type::Tuple(vec![desc.clone(); 64]),
        Type::ForeignCallback {
            parameters: vec![desc.clone(); 64],
            result: Box::new(Type::ByteBuffer),
        },
        Type::Record {
            name: "Hostile".into(),
            fields: (0..64).map(|i| (format!("p{i}"), desc.clone())).collect(),
        },
    ];
    let target = TargetProfile::default();
    let limits = Limits {
        max_nodes: 16,
        ..Limits::default()
    };
    for shape in shapes {
        let mut input = core();
        input.functions[0].return_type = shape.clone();
        limited(zil_types::infer_source_closures(&input, &limits).unwrap_err());
        limited(zil_ct::evaluate_source_constants(&input, &target, &limits).unwrap_err());
        limited(adapter::single(&input, &target, &limits).unwrap_err());
        let input = program(input);
        match zil_ct::SourceProgramContext::new(&input, &limits) {
            Err(errors) => limited(errors),
            Ok(_) => panic!("unusable headers bypassed the complete raw shape budget"),
        }
        let compiler = adapter::Compiler {
            program: &input,
            module: input.root,
            target: &target,
        };
        limited(
            compiler
                .compile(
                    &expression(CoreExprKind::Integer("1".into())),
                    Some(&shape),
                    &limits,
                )
                .unwrap_err(),
        );
    }
}

#[test]
fn all_raw_byte_primitives_are_closed_at_the_ct_query_boundary() {
    let input = program(core());
    let target = TargetProfile::default();
    let compiler = adapter::Compiler {
        program: &input,
        module: input.root,
        target: &target,
    };
    for name in [
        "bytes_new",
        "bytes_reserve",
        "bytes_push",
        "bytes_len",
        "bytes_capacity",
        "bytes_view",
        "bytes_view_mut",
    ] {
        let query = expression(CoreExprKind::Call {
            callee: Box::new(expression(CoreExprKind::Name(name.into()))),
            arguments: vec![],
        });
        let errors = compiler
            .compile(&query, None, &Limits::default())
            .unwrap_err();
        assert_eq!(
            errors[0].category,
            ResultCategory::SafeReject,
            "{name}: {errors:#?}"
        );
        assert_eq!(errors[0].phase, Phase::Ct);
        assert_eq!(errors[0].code, "E-CT-FORBIDDEN");
    }
}

#[test]
fn raw_core_cannot_rebind_reserved_storage_symbols() {
    for name in ["ByteBuffer", "bytes_new", "bytes_push", "bytes_view_mut"] {
        let mut input = core();
        input.functions[0].name = name.into();
        let errors =
            zil_types::check(&input, &TargetProfile::default(), &Limits::default()).unwrap_err();
        assert_eq!(errors[0].phase, Phase::Types);
        assert_eq!(errors[0].code, "E-BYTE-BUFFER-NAME", "{name}: {errors:#?}");
    }
}
