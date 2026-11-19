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

#[test]
fn generic_copy_assignment_binds_both_dynamic_indices_of_the_original_slice() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let source = SourceUnit::new(FileId(0), ModuleId(0), "slice-copy-source.zil", r#"
        struct Packet { value:u64; tag:i32 }
        fn copy_at<T:Copy>(values:&mut[T],dst:i64,src:i64) {values[dst]=values[src]}
        fn main()->i32 {
            var values=[Packet{value:1u64,tag:2},Packet{value:3u64,tag:4}]
            copy_at(&mut values[..],0,1)
            return values[0].tag-4
        }
    "#);
    let core = original_core(source, &limits, &target);
    let mut module = zil_types::check(&core, &target, &limits).unwrap();
    let mut plan = zil_analyze::analyze(&module, &limits).unwrap();
    let wire = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();

    let function = module.functions.iter_mut().find(|f| f.parameters.len() == 3).unwrap();
    let mut indices = function.blocks.iter_mut().flat_map(|b| &mut b.instructions)
        .filter_map(|i| match &mut i.kind {
            InstructionKind::AddressIndex { index, .. } => Some(index),
            _ => None,
        });
    let dst = *indices.next().unwrap();
    let src = indices.next().unwrap();
    assert_ne!(dst, *src);
    assert_eq!(function.values[dst.0 as usize], function.values[src.0 as usize]);
    *src = dst;
    assert!(indices.next().is_none());

    plan.module_fingerprint = module.fingerprint();
    let bytes = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let raw = zil_verify::verify_module(module, plan, &target, &limits).unwrap_err();
    let decoded = zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err();
    assert_eq!(raw, decoded);
    assert!(raw.iter().any(|d| d.category == ResultCategory::SafeReject
        && d.code == "E-VERIFY-SEMANTIC" && d.notes.iter().any(|n| n.contains("assignment RHS"))), "{raw:?}");
}

#[test]
fn direct_call_uses_its_original_scalar_field_and_computed_argument() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let source = SourceUnit::new(FileId(0), ModuleId(0), "scalar-call.zil",
        "struct Pair { index:i64; marker:i64 }\nfn difference(a:i64,b:i64)->i64{return a-b}\nfn read(pair:Pair)->i64{return difference(pair.index,pair.marker+1)}\nfn main(){}");
    let core = original_core(source, &limits, &target);
    let baseline = zil_types::check(&core, &target, &limits).unwrap();
    let baseline_plan = zil_analyze::analyze(&baseline, &limits).unwrap();
    zil_verify::verify_module(baseline.clone(), baseline_plan.clone(), &target, &limits).unwrap();
    let bytes = wire::encode_module_candidate(&baseline, &baseline_plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();

    for change_call_input in [true, false] {
        let mut typed = baseline.clone();
        let caller = typed.functions.iter_mut().find(|f| f.name == "read").unwrap();
        let mut edits = 0;
        for instruction in caller.blocks.iter_mut().flat_map(|block| &mut block.instructions) {
            match &mut instruction.kind {
                InstructionKind::Call { arguments, .. } if change_call_input => {
                    let [index, marker] = arguments.as_mut_slice() else { panic!("two original arguments") };
                    assert_ne!(index, marker);
                    assert_eq!(caller.values[index.0 as usize], Type::I64);
                    assert_eq!(caller.values[marker.0 as usize], Type::I64);
                    *index = *marker;
                    edits += 1;
                }
                InstructionKind::ExtractField { aggregate, index } if !change_call_input && *index == 0 => {
                    let Type::Record { fields, .. } = &caller.values[aggregate.0 as usize] else { panic!("Pair") };
                    assert_eq!(fields, &[("index".into(), Type::I64), ("marker".into(), Type::I64)]);
                    *index = 1;
                    edits += 1;
                }
                _ => {}
            }
        }
        assert_eq!(edits, 1);
        let errors = zil_analyze::analyze(&typed, &limits).unwrap_err();
        assert!(errors.iter().any(|d| d.message.contains("actual scalar Call argument")), "{errors:?}");
        require_semantic_rejection(errors);
        let mut plan = baseline_plan.clone();
        plan.module_fingerprint = typed.fingerprint();
        let bytes = wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
        for errors in [zil_verify::verify_module(typed, plan, &target, &limits).unwrap_err(),
            zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err()] {
            assert!(errors.iter().any(|d| d.notes.iter().any(|note|note.contains("actual scalar call"))), "{errors:?}");
            require_semantic_rejection(errors);
        }
    }
}

#[test]
fn direct_call_preserves_scalar_conversions_literals_and_inferred_arguments() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let text = "fn same<T:Value>(value:T)->T{return value}\nfn read(a:i32)->i64{return same(i64(a)+(-3i64))}\nfn real(v:f64)->f64{return v}\nfn floating()->f64{return real(-2.5f64)}\nfn piped()->i64{return 7i64 |> same}\nfn nested(x:i64)->i64{let n=x+1;return ((n |> same) |> same)}\nfn main(){}";
    let source = SourceUnit::new(FileId(0), ModuleId(0), "scalar-forms.zil", text);
    let core = original_core(source, &limits, &target);
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    let bytes = wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
    let mut changed = typed;
    let piped = changed.functions.iter_mut().find(|function|function.name == "piped").unwrap();
    let mut edits = 0;
    for instruction in piped.blocks.iter_mut().flat_map(|block| &mut block.instructions) {
        if let InstructionKind::Constant(Constant::I64(number @ 7)) = &mut instruction.kind {
            *number = 8;
            edits += 1;
        }
    }
    assert_eq!(edits, 1);
    let errors = zil_analyze::analyze(&changed, &limits).unwrap_err();
    assert!(errors.iter().any(|d| d.message.contains("actual scalar Call argument")), "{errors:?}");
    require_semantic_rejection(errors);
    let mut forged_plan = plan;
    forged_plan.module_fingerprint = changed.fingerprint();
    let bytes = wire::encode_module_candidate(&changed, &forged_plan, &limits).unwrap();
    for errors in [zil_verify::verify_module(changed, forged_plan, &target, &limits).unwrap_err(),
        zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err()] {
        assert!(errors.iter().any(|d| d.notes.iter().any(|note|note.contains("actual scalar call"))), "{errors:?}");
        require_semantic_rejection(errors);
    }
}

#[test]
fn generic_pipeline_preserves_its_original_parameter_before_preparation() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let source = SourceUnit::new(FileId(0), ModuleId(0), "pipeline-parameter.zil",
        "fn identity<T:Copy>(x:T)->T{return x}\nfn pipe<T:Copy>(a:T,b:T)->T{return a |> identity}\nfn main()->i32{return pipe(42,17)-42}");
    let core = original_core(source, &limits, &target);
    let mut typed = zil_types::check(&core, &target, &limits).unwrap();
    let mut plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let bytes = wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();

    let caller = typed.functions.iter_mut().find(|f| f.parameters.len() == 2).unwrap();
    let [left, right] = caller.parameters[..] else { unreachable!() };
    assert_eq!(caller.locals[left.0 as usize].ty, Type::I32);
    assert_eq!(caller.locals[right.0 as usize].ty, Type::I32);
    let mut edits = 0;
    for instruction in caller.blocks.iter_mut().flat_map(|block| &mut block.instructions) {
        if let InstructionKind::Load(input) = &mut instruction.kind && *input == left {
            *input = right;
            edits += 1;
        }
    }
    assert_eq!(edits, 1);
    let errors = zil_analyze::analyze(&typed, &limits).unwrap_err();
    assert!(errors.iter().any(|d| d.message.contains("actual scalar Call argument")), "{errors:?}");
    require_semantic_rejection(errors);
    plan.module_fingerprint = typed.fingerprint();
    let bytes = wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    for errors in [zil_verify::verify_module(typed, plan, &target, &limits).unwrap_err(),
        zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err()] {
        assert!(errors.iter().any(|d| d.notes.iter().any(|n| n.contains("actual scalar call"))), "{errors:?}");
        require_semantic_rejection(errors);
    }
}
