//! One real unused definition, then hostile original-body data at each gate.
//! The actual main, Typed instructions and concrete instances stay unchanged.
use zil_base::{FileId, Limits, ModuleId, NodeId, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::*;

fn source_core(limits: &Limits, target: &TargetProfile) -> CoreModule {
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "unused-generic-identity.zil",
        include_str!("../../../tests/fixtures/semantic-scopes/unused-generic-identity.zil"),
    );
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let aliases = std::collections::BTreeMap::new();
    let context = zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
        root: source.module_id,
        aliases: &aliases,
        declarations: zil_expand::source_context::DeclarationPlacement::Original,
    };
    zil_expand::expand_program_with_semantics(
        &[source],
        &[&syntax],
        &[Vec::new()],
        limits,
        target,
        context,
    )
    .unwrap()
    .remove(0)
    .0
    .core
}

fn duplicate_original_value(program: &mut SemanticModule) {
    let definition = program
        .definitions
        .iter_mut()
        .find(|definition| {
            definition
                .identity
                .declaration_path
                .last()
                .map(String::as_str)
                == Some("identity")
        })
        .expect("the real original identity declaration");
    assert!(
        !program
            .instances
            .iter()
            .any(|i| i.definition == definition.id)
    );
    let SemanticDefinitionBody::FunctionWithBody {
        signature,
        body: SemanticOriginalFunctionBody::Defined(body),
        ..
    } = &mut definition.body
    else {
        panic!("the complete original body must exist without an instance");
    };
    let CoreStatementKind::Return(Some(value)) = &mut body.statements[0].kind else {
        panic!("the original identity must return its parameter");
    };
    assert!(matches!(&value.kind, CoreExprKind::Name(name) if name == "value"));
    let mut first = value.clone();
    let mut second = value.clone();
    first.id = NodeId(u32::MAX - 2);
    second.id = NodeId(u32::MAX - 1);
    value.kind = CoreExprKind::Tuple(vec![first, second]);
    signature.result =
        SemanticTypeRecipe::Tuple(vec![signature.result.clone(), signature.result.clone()]);
}

fn require_semantic_rejection(errors: Vec<zil_base::Diagnostic>) {
    assert!(!errors.is_empty());
    assert!(
        errors.iter().all(|error| {
            let semantic_gate = match error.phase {
                zil_base::Phase::Types => error.code == "E-SEMANTIC-ORIGINAL",
                zil_base::Phase::Analyze => {
                    error.code == "E-ANALYZE-IR" && error.message.starts_with("semantic program:")
                }
                zil_base::Phase::Verify => error.code == "E-VERIFY-SEMANTIC",
                _ => false,
            };
            error.category == ResultCategory::SafeReject && semantic_gate
        }),
        "unsupported, transport and plan failures are not a symbolic rejection: {errors:#?}"
    );
}

#[test]
fn unused_generic_copy_is_checked_by_types_analyze_and_verify_raw_and_wire() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let core = source_core(&limits, &target);
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    let baseline = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&baseline, &target, &limits).unwrap();

    let mut bad_core = core;
    duplicate_original_value(bad_core.semantic.as_mut().unwrap());
    require_semantic_rejection(zil_types::check(&bad_core, &target, &limits).unwrap_err());

    let mut bad_typed = typed;
    duplicate_original_value(bad_typed.semantic.as_mut().unwrap());
    require_semantic_rejection(zil_analyze::analyze(&bad_typed, &limits).unwrap_err());

    // Keep every actual CFG claim from the valid baseline. Recompute only the
    // public module identity so Verify must inspect the modified original body.
    let mut untrusted_plan = plan;
    untrusted_plan.module_fingerprint = bad_typed.fingerprint();
    let bytes =
        zil_ir::wire::encode_module_candidate(&bad_typed, &untrusted_plan, &limits).unwrap();
    require_semantic_rejection(
        zil_verify::verify_module(bad_typed, untrusted_plan, &target, &limits).unwrap_err(),
    );
    require_semantic_rejection(
        zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err(),
    );
}
