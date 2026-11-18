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
    original_core(source, limits, target)
}

fn original_core(source: SourceUnit, limits: &Limits, target: &TargetProfile) -> CoreModule {
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

#[test]
fn enum_constructor_infers_its_actual_error_union_arm_and_rejects_a_swapped_arm() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "enum-arm.zil",
        r#"
        enum Failure<T:Copy> { Unknown, Detail(T) }
        fn success()->Failure<i32>!Option<i64>{return Option.None}
        fn failure()->Failure<i32>!Option<i64>{return Failure.Unknown}
        fn main(){}
    "#,
    );
    let core = original_core(source, &limits, &target);
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let claim = zil_analyze::analyze(&module, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &claim, &limits).unwrap();
    zil_verify::verify_module(module.clone(), claim.clone(), &target, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    let mut selected = Vec::new();
    for (fid, f) in module.functions.iter().enumerate() {
        for (bid, block) in f.blocks.iter().enumerate() {
            for (iid, instruction) in block.instructions.iter().enumerate() {
                if let InstructionKind::MakeErrorUnion { is_error, .. } = instruction.kind {
                    selected.push((fid, bid, iid, is_error));
                }
            }
        }
    }
    assert_eq!(selected.iter().filter(|site| site.3).count(), 1);
    assert_eq!(selected.len(), 2);
    for (f, b, i, original_arm) in selected {
        let mut altered = module.clone();
        let instruction = &mut altered.functions[f].blocks[b].instructions[i];
        let span = instruction.span;
        let InstructionKind::MakeErrorUnion { is_error, .. } = &mut instruction.kind else {
            unreachable!()
        };
        *is_error = !original_arm;
        let mut re_signed = claim.clone();
        re_signed.module_fingerprint = altered.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&altered, &re_signed, &limits).unwrap();
        let raw = zil_verify::verify_module(altered, re_signed, &target, &limits).unwrap_err();
        let decoded = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
        assert_eq!(raw, decoded);
        assert!(
            raw.iter().any(|e| e.code == "E-VERIFY-ERROR"
                && e.category == ResultCategory::SafeReject
                && e.span == Some(span)),
            "{raw:?}"
        );
    }
}

#[test]
fn saved_store_element_preserves_its_actual_input_through_an_ordinary_view_call() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "store-input.zil",
        r#"
        package Entry { manages data:ByteBuffer }
        fn view(value:&Entry)->&[u8]{return bytes_view(value.data)}
        fn measure(first:Store<Entry>,second:Store<Entry>)->i64{
            let saved=store_at(first,0)
            let bytes=view(saved)
            return bytes.len()
        }
        fn main(){}
    "#,
    );
    let core = original_core(source, &limits, &target);
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let claim = zil_analyze::analyze(&module, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &claim, &limits).unwrap();
    zil_verify::verify_module(module.clone(), claim.clone(), &target, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    let function = module
        .functions
        .iter()
        .find(|f| f.parameters.len() == 2)
        .unwrap();
    let (first, second) = (function.parameters[0], function.parameters[1]);
    assert_eq!(
        function.locals[first.0 as usize].ty,
        function.locals[second.0 as usize].ty
    );
    let owner = module.semantic.as_ref().unwrap().instances.iter().find(|instance|
        matches!(instance.target, SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id))
            if id == function.id)).unwrap().id;
    for change_read in [true, false] {
        let mut altered = module.clone();
        if change_read {
            let read = altered.functions[function.id.0 as usize]
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| matches!(i.kind, InstructionKind::Load(id) if id == first))
                .unwrap();
            read.kind = InstructionKind::Load(second);
        } else {
            let classes = altered
                .semantic
                .as_mut()
                .unwrap()
                .sites
                .iter_mut()
                .filter(|site| site.instance == owner)
                .find_map(|site| {
                    let SemanticSiteAction::DeclareLocal {
                        expected:
                            SemanticTypeValue::Resolved {
                                ty: Type::Address { inner, .. },
                                classes,
                                ..
                            },
                        ..
                    } = &mut site.action
                    else {
                        return None;
                    };
                    matches!(&**inner, Type::Slice { .. }).then_some(classes)
                })
                .unwrap();
            let [SemanticClassExpr::Implicit { ordinal, .. }] = classes.as_mut_slice() else {
                panic!("the returned byte view must name exactly one source class");
            };
            assert_eq!(*ordinal, 0);
            *ordinal = 1;
        }
        let producer = zil_analyze::analyze(&altered, &limits).unwrap_err();
        assert!(
            producer
                .iter()
                .any(|e| e.code == "E-ANALYZE-IR" && e.category == ResultCategory::SafeReject)
        );
        let mut claim = claim.clone();
        claim.module_fingerprint = altered.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&altered, &claim, &limits).unwrap();
        let raw = zil_verify::verify_module(altered, claim, &target, &limits).unwrap_err();
        let decoded = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
        assert_eq!(raw, decoded);
        assert!(
            raw.iter()
                .any(|e| e.code == "E-VERIFY-SEMANTIC" && e.category == ResultCategory::SafeReject)
        );
    }
}
