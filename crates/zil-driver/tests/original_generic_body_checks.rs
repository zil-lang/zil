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

#[test]
fn rigid_header_cannot_substitute_the_other_implicit_parameter() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let source = SourceUnit::new(FileId(0), ModuleId(0), "two-inputs.zil",
        "fn sum(a:&i32,b:&i32)->i32{return *a + *b}\nfn main()->i32{let a=17;let b=25;return sum(&a,&b)-42}");
    let core = original_core(source, &limits, &target);
    let mut module = zil_types::check(&core, &target, &limits).unwrap();
    let mut plan = zil_analyze::analyze(&module, &limits).unwrap();
    let original = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    zil_verify::verify_module_bytes(&original, &target, &limits).unwrap();

    let sum = module.functions.iter().find(|f| f.parameters.len() == 2).unwrap().id;
    let world = module.semantic.as_mut().unwrap().instances.iter_mut()
        .find(|w| w.target == SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(sum)))
        .unwrap();
    let [first, second] = world.header_embedding.as_mut_slice() else {
        panic!("each independent reference parameter supplies one implicit header image");
    };
    assert!(matches!((&first.actual, &second.actual),
        (SemanticClassExpr::Implicit { scope: a, ordinal: i }, SemanticClassExpr::Implicit { scope: b, ordinal: j })
        if a == b && i != j));
    first.actual = second.actual.clone();
    plan.module_fingerprint = module.fingerprint();
    let changed = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    for errors in [
        zil_verify::verify_module(module, plan, &target, &limits).unwrap_err(),
        zil_verify::verify_module_bytes(&changed, &target, &limits).unwrap_err(),
    ] {
        assert!(errors.iter().any(|d| d.message.contains("original class ordinal")));
        require_semantic_rejection(errors);
    }
}

#[test]
fn scalar_call_after_question_mark_uses_this_calls_success_payload() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let source = SourceUnit::new(FileId(0), ModuleId(0), "two-successes.zil", r#"
        enum Fault { Bad }
        fn read(n:i32)->Fault!i32 { if n<0 {return Fault.Bad} return n }
        fn keep(value:i32)->i32 {return value}
        fn pair(x:i32)->Fault!i32 {
            let first=keep(read(x)?)
            let second=keep(read(25)?)
            return first+second
        }
        fn main()->Fault!i32 {return pair(17)?-42}
    "#);
    let core = original_core(source, &limits, &target);
    let mut module = zil_types::check(&core, &target, &limits).unwrap();
    let mut plan = zil_analyze::analyze(&module, &limits).unwrap();
    let original = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    zil_verify::verify_module_bytes(&original, &target, &limits).unwrap();

    let pair = module.functions.iter_mut().find(|function| function.name == "pair").unwrap();
    let mut projections = pair.blocks.iter_mut().flat_map(|block| &mut block.instructions)
        .filter_map(|instruction| match &mut instruction.kind {
            InstructionKind::ReadErrorUnion { value, is_error: false } => Some(value),
            _ => None,
        });
    let first = *projections.next().unwrap();
    let second = projections.next().unwrap();
    assert_ne!(first, *second);
    assert_eq!(pair.values[first.0 as usize], pair.values[second.0 as usize]);
    *second = first;
    assert!(projections.next().is_none());
    require_semantic_rejection(zil_analyze::analyze(&module, &limits).unwrap_err());
    plan.module_fingerprint = module.fingerprint();
    let changed = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    require_semantic_rejection(zil_verify::verify_module(module, plan, &target, &limits).unwrap_err());
    require_semantic_rejection(zil_verify::verify_module_bytes(&changed, &target, &limits).unwrap_err());
}

#[test]
fn scalar_call_observes_the_selected_managed_field_of_a_captured_package() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let source = SourceUnit::new(FileId(0), ModuleId(0), "captured-fields.zil", r#"
        resource Ticket { code: i32 }
        impl Drop for Ticket { fn drop(self: Self) {} }
        package Bag { manages left: Ticket; manages right: Ticket; counter: i32 }
        fn keep(value: i32) -> i32 { return value }
        fn make() -> impl FnMut(i32) -> i32 {
            var bag = Bag { left: Ticket { code: 17 }, right: Ticket { code: 25 }, counter: 0 }
            return move (step: i32) => { bag.counter += 1; return keep(bag.left.code + step) }
        }
        fn main() -> i32 { var callback = make(); return callback(0) - 17 }
    "#);
    let core = original_core(source, &limits, &target);
    let mut module = zil_types::check(&core, &target, &limits).unwrap();
    let mut plan = zil_analyze::analyze(&module, &limits).unwrap();
    let original = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    zil_verify::verify_module_bytes(&original, &target, &limits).unwrap();

    let mut changed = 0;
    for function in &mut module.functions {
        for instruction in function.blocks.iter_mut().flat_map(|block| &mut block.instructions) {
            if let InstructionKind::BorrowPackageField { local, index, mutable: false } = &mut instruction.kind {
                let Type::Package { fields, .. } = &function.locals[local.0 as usize].ty else { panic!() };
                assert_eq!((&fields[0].0[..], &fields[1].0[..]), ("left", "right"));
                assert_eq!(fields[0].1, fields[1].1);
                assert_eq!(*index, 0);
                *index = 1;
                changed += 1;
            }
        }
    }
    assert_eq!(changed, 1);
    require_semantic_rejection(zil_analyze::analyze(&module, &limits).unwrap_err());
    plan.module_fingerprint = module.fingerprint();
    let changed = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    require_semantic_rejection(zil_verify::verify_module(module, plan, &target, &limits).unwrap_err());
    require_semantic_rejection(zil_verify::verify_module_bytes(&changed, &target, &limits).unwrap_err());
}

#[test]
fn shared_pipeline_reborrows_the_selected_original_input() {
    let limits = Limits {
        max_analysis_steps: 10_000_000,
        ..Limits::default()
    };
    let target = TargetProfile::host_llvm();
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "pipeline-shared-address.zil",
        r#"
        fn read<life a>(x:&'a i32)->i32{return *x}
        fn pipe<life a>(x:&'a mut i32,y:&'a mut i32)->i32{
            let observed=x |> read
            return observed+*x
        }
        fn main()->i32{
            var first=42
            var second=17
            return pipe(&mut first,&mut second)-84
        }
    "#,
    );
    let core = original_core(source, &limits, &target);
    let mut module = zil_types::check(&core, &target, &limits).unwrap();
    let mut plan = zil_analyze::analyze(&module, &limits).unwrap();
    let bytes = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();

    let caller = module
        .functions
        .iter_mut()
        .find(|f| f.parameters.len() == 2)
        .unwrap();
    let [left, right] = caller.parameters[..] else {
        unreachable!()
    };
    let address = &caller.locals[left.0 as usize].ty;
    assert!(matches!(address, Type::Address { mutable: true, inner } if **inner == Type::I32));
    assert_eq!(*address, caller.locals[right.0 as usize].ty);
    let mut edits = 0;
    for block in &mut caller.blocks {
        let staged_sources: Vec<_> = block
            .instructions
            .windows(4)
            .enumerate()
            .filter_map(|(index, instructions)| {
                let [load, reborrow, live, store] = instructions else {
                    unreachable!()
                };
                let (Some(loaded), Some(borrowed)) = (load.result, reborrow.result) else {
                    return None;
                };
                let InstructionKind::StorageLive(temporary) = live.kind else {
                    return None;
                };
                (load.kind == InstructionKind::Load(left)
                    && reborrow.kind
                        == (InstructionKind::ReborrowAddress {
                            address: loaded,
                            mutable: false,
                        })
                    && store.kind
                        == (InstructionKind::Store {
                            local: temporary,
                            value: borrowed,
                        }))
                .then_some(index)
            })
            .collect();
        for index in staged_sources {
            block.instructions[index].kind = InstructionKind::Load(right);
            edits += 1;
        }
    }
    assert_eq!(edits, 1, "only the pipe's prepared source should change");
    let errors = zil_analyze::analyze(&module, &limits).unwrap_err();
    assert!(errors.iter().any(|d| {
        d.message
            .contains("prepared shared Address selects another original input")
    }));
    require_semantic_rejection(errors);
    plan.module_fingerprint = module.fingerprint();
    let bytes = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    for errors in [
        zil_verify::verify_module(module, plan, &target, &limits).unwrap_err(),
        zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err(),
    ] {
        require_semantic_rejection(errors);
    }
}

#[test]
fn range_call_uses_the_guarded_iteration_and_its_original_endpoints() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let source = SourceUnit::new(FileId(0), ModuleId(0), "range-call.zil", r#"
        fn keep(value: i32) -> i32 { return value }
        fn count(start: i32, end: i32) -> i32 {
            var total = 0
            for i in start..end { total += keep(i) }
            return total
        }
        fn main() -> i32 { return count(1,4) - 6 }
    "#);
    let core = original_core(source, &limits, &target);
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    let wire = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();

    for changed in ["body", "end", "step"] {
        let mut candidate = module.clone();
        let f = candidate.functions.iter_mut().find(|f| f.name == "count").unwrap();
        let (head, body, condition) = f.blocks.iter().find_map(|b| match b.terminator {
            Terminator::Branch { then_block, condition, .. } => Some((b.id, then_block, condition)),
            _ => None,
        }).unwrap();
        let guard = &f.blocks[head.0 as usize];
        let comparison = guard.instructions.iter().find(|i| i.result == Some(condition)).unwrap();
        let InstructionKind::Binary { op: BinaryOp::Less, left: current, .. } = comparison.kind else { panic!() };
        let load = guard.instructions.iter().find(|i| i.result == Some(current)).unwrap();
        let InstructionKind::Load(counter) = load.kind else { panic!() };
        let body_block = &f.blocks[body.0 as usize];
        let InstructionKind::Store { local: snapshot, value } = body_block.instructions[0].kind else { panic!() };
        assert_eq!(value, current);
        assert_eq!(f.locals[snapshot.0 as usize].ty, f.locals[counter.0 as usize].ty);
        assert_ne!(snapshot, counter);
        match changed {
            "body" => {
                let load = f.blocks[body.0 as usize].instructions.iter_mut().find(|ins| ins.kind == InstructionKind::Load(snapshot)).unwrap();
                load.kind = InstructionKind::Load(counter);
            }
            "end" => {
                let start = f.blocks[0].instructions.iter().find_map(|ins| match ins.kind {
                    InstructionKind::Store { local, value } if local == counter => Some(value), _ => None,
                }).unwrap();
                let comparison = f.blocks[head.0 as usize].instructions.iter_mut().find(|i| i.result == Some(condition)).unwrap();
                let InstructionKind::Binary { right: end, .. } = &mut comparison.kind else { panic!() };
                assert_ne!(start, *end);
                *end = start;
            }
            "step" => {
                let step = f.blocks.iter_mut().find(|b| b.id != BlockId(0) && matches!(b.terminator, Terminator::Jump(to) if to == head)).unwrap();
                let unit = step.instructions.iter_mut().find(|ins| ins.kind == InstructionKind::Constant(Constant::I32(1))).unwrap();
                unit.kind = InstructionKind::Constant(Constant::I32(2));
            }
            _ => unreachable!(),
        }
        require_semantic_rejection(zil_analyze::analyze(&candidate, &limits).unwrap_err());
        let mut derived = plan.clone();
        derived.module_fingerprint = candidate.fingerprint();
        let wire = wire::encode_module_candidate(&candidate, &derived, &limits).unwrap();
        require_semantic_rejection(zil_verify::verify_module(candidate, derived, &target, &limits).unwrap_err());
        require_semantic_rejection(zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err());
    }
}

#[test]
fn shared_pipeline_keeps_the_original_lexical_loan() {
    let limits = Limits {
        max_analysis_steps: 10_000_000,
        ..Limits::default()
    };
    let target = TargetProfile::host_llvm();
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "pipeline-local-address.zil",
        r#"
        fn read<life a>(x:&'a i32)->i32{return *x}
        fn main()->i32{
            var first=42
            var second=17
            let position=&mut first
            let sibling=&mut second
            let observed=position |> read
            return observed+*position+*sibling-101
        }
    "#,
    );
    let core = original_core(source, &limits, &target);
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    let bytes = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module(module, plan, &target, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
}

#[test]
fn scalar_call_reads_its_original_capture_after_description_storage() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let source = SourceUnit::new(FileId(0), ModuleId(0), "two-captured-resources.zil", r#"
        resource Ticket { code:i32; count:i32 }
        impl Drop for Ticket { fn drop(self:Self){} }
        fn keep(v:i32)->i32{return v}
        fn make()->impl FnMut()->i32 {
            var left=Ticket{code:13,count:4}
            var right=Ticket{code:99,count:7}
            return move ()=> {
                left.count+=1
                right.count+=2
                return keep(left.count)+keep(right.count)
            }
        }
        fn main()->i32 { var callback=make(); return callback()-14 }
    "#);
    let core = original_core(source, &limits, &target);
    let mut module = zil_types::check(&core, &target, &limits).unwrap();
    let mut plan = zil_analyze::analyze(&module, &limits).unwrap();
    let original = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    zil_verify::verify_module_bytes(&original, &target, &limits).unwrap();

    let mut changed = 0;
    let keep = module.functions.iter().find(|f| f.name == "keep").unwrap().id;
    for function in &mut module.functions {
        if function.environment.is_none() { continue; }
        for block in &mut function.blocks {
            let Some(argument) = block.instructions.iter().find_map(|row| match &row.kind {
                InstructionKind::Call { function, arguments } if *function == keep => arguments.first().copied(),
                _ => None,
            }) else { continue; };
            let desc = block.instructions.iter().find_map(|row| match row.kind {
                InstructionKind::ReadDesc { desc, index: 1 } if row.result == Some(argument) => Some(desc),
                _ => None,
            }).unwrap();
            let first = block.instructions.iter().position(|row| row.result == Some(desc)).unwrap();
            let InstructionKind::Load(left) = block.instructions[first].kind else { panic!() };
            let other_origin = block.instructions.iter().find_map(|row| match row.kind {
                InstructionKind::DescribeCapture { index: 1 } => row.result, _ => None,
            }).unwrap();
            let right = block.instructions.iter().find_map(|row| match row.kind {
                InstructionKind::Store { local, value } if value == other_origin => Some(local), _ => None,
            }).unwrap();
            assert_ne!(left, right);
            assert_eq!(function.locals[left.0 as usize].ty, function.locals[right.0 as usize].ty);
            block.instructions[first].kind = InstructionKind::Load(right);
            changed += 1;
        }
    }
    assert_eq!(changed, 1);
    require_semantic_rejection(zil_analyze::analyze(&module, &limits).unwrap_err());
    plan.module_fingerprint = module.fingerprint();
    let bytes = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    require_semantic_rejection(zil_verify::verify_module(module, plan, &target, &limits).unwrap_err());
    require_semantic_rejection(zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err());
}

fn check_stored_origin_mutation(
    source: &str,
    mutate: impl FnOnce(&mut TypedModule),
    analyze_reason: &str,
    verify_reason: &str,
) {
    let limits = Limits::default();
    let target = TargetProfile::default();
    let core = original_core(SourceUnit::new(FileId(0), ModuleId(0), "stored-origin.zil", source), &limits, &target);
    let program = CoreProgram {
        root: ModuleId(0),
        source_fingerprint: core.source_fingerprint,
        modules: vec![ProgramModule { core, imports: vec![], exports: vec![] }],
    };
    let prepared = zil_types::infer_program_for_ct(&program, &limits).unwrap().into_prepared();
    let mut module = zil_types::check_prepared_program(&prepared, &target, &limits).unwrap().module;
    let mut plan = zil_analyze::analyze(&module, &limits).unwrap();
    let wire = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();

    mutate(&mut module);
    let errors = zil_analyze::analyze(&module, &limits).unwrap_err();
    assert!(errors[0].message.contains(analyze_reason), "{errors:?}");
    require_semantic_rejection(errors);
    plan.module_fingerprint = module.fingerprint();
    let wire = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    for result in [
        zil_verify::verify_module(module, plan, &target, &limits),
        zil_verify::verify_module_bytes(&wire, &target, &limits),
    ] {
        let errors = result.err().expect("the changed original stored source must be rejected");
        assert!(errors[0].message.contains(verify_reason), "{errors:?}");
        require_semantic_rejection(errors);
    }
}

#[test]
fn returned_stored_slot_uses_the_address_from_its_own_call() {
    let source = include_str!("../../../tests/fixtures/resource-packages/returned-stored-address.zil");
    check_stored_origin_mutation(source, |module| {
        let identity = module.functions.iter().find(|f| f.name == "identity").unwrap().id;
        let step = module.functions.iter_mut().find(|f| f.name == "step").unwrap();
        let block = &mut step.blocks[0];
        let calls = block.instructions.iter().filter_map(|instruction| {
            match instruction.kind {
                InstructionKind::Call { function, .. } if function == identity => instruction.result,
                _ => None,
            }
        }).collect::<Vec<_>>();
        assert_eq!(calls.len(), 2);
        assert_eq!(step.values[calls[0].0 as usize], step.values[calls[1].0 as usize]);
        let mut changed = 0;
        for instruction in &mut block.instructions {
            if let InstructionKind::LoadStoredBorrow { storage, .. } = &mut instruction.kind
                && *storage == calls[1]
            {
                *storage = calls[0];
                changed += 1;
            }
        }
        assert_eq!(changed, 1);
    }, "stored reborrow selects another original receiver producer", "another same-typed original expression value");
}

#[test]
fn stored_reborrow_checks_the_selected_inline_package_before_its_slot() {
    let source = include_str!("../../../tests/fixtures/resource-packages/projected-stored-address.zil");
    check_stored_origin_mutation(source, |module| {
        let bump = module.functions.iter_mut().find(|f| f.name == "bump").unwrap();
        let mut changed = 0;
        for block in &mut bump.blocks {
            for instruction in &mut block.instructions {
                let InstructionKind::AddressField { address, index } = &mut instruction.kind else { continue };
                let Type::Address { inner, .. } = &bump.values[address.0 as usize] else { panic!() };
                let Type::BorrowedStorage { storage, .. } = inner.as_ref() else { panic!() };
                let Type::Package { fields, .. } = storage.as_ref() else { panic!() };
                assert_eq!(*index, 0);
                assert_eq!((&*fields[0].0, &*fields[1].0), ("left", "right"));
                assert_eq!(fields[0].1, fields[1].1);
                *index = 1;
                changed += 1;
            }
        }
        assert_eq!(changed, 1);
    }, "stored reborrow selects another original field", "actual field index differs from the original ordered");
}

#[test]
fn stored_exclusive_reborrow_keeps_its_selected_original_field() {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "two-exclusive-slots.zil", r#"
pub struct Pair<life a> { left:&'a mut i64, right:&'a mut i64 }
pub fn make<life a>(left:&'a mut i64,right:&'a mut i64)->Pair<'a>{
    return Pair::<'a>{left:left,right:right}
}
fn step<life a>(holder:&mut Pair<'a>){
    let pointer=holder.left
    *pointer=*pointer+1
}
fn main()->i32{
    var x:i64=40
    var y:i64=10
    var pair=make(&mut x,&mut y)
    step(&mut pair)
    return i32(x-y)-31
}
"#);
    let limits = Limits::default();
    let target = TargetProfile::default();
    let core = original_core(source, &limits, &target);
    let core = CoreProgram {
        root: ModuleId(0), source_fingerprint: core.source_fingerprint,
        modules: vec![ProgramModule { core, imports: vec![], exports: vec![] }],
    };
    let prepared = zil_types::infer_program_for_ct(&core, &limits).unwrap().into_prepared();
    let mut module = zil_types::check_prepared_program(&prepared, &target, &limits).unwrap().module;
    let mut plan = zil_analyze::analyze(&module, &limits).unwrap();
    let wire = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();

    let step = module.functions.iter_mut().find(|f| f.name == "step").unwrap();
    let mut changed = 0;
    for block in &mut step.blocks {
        for instruction in &mut block.instructions {
            let InstructionKind::LoadStoredBorrow {
                storage, path, mode: StoredBorrowLoad::Reborrow { mutable: true },
            } = &mut instruction.kind else { continue; };
            let Type::Address { inner, .. } = &step.values[storage.0 as usize] else { panic!() };
            let Type::BorrowedStorage { storage, .. } = inner.as_ref() else { panic!() };
            let Type::Record { fields, .. } = storage.as_ref() else { panic!() };
            assert_eq!((&fields[0].0[..], &fields[1].0[..]), ("left", "right"));
            assert_eq!(fields[0].1, fields[1].1);
            assert_eq!(path.as_slice(), &[PlaceProjection::Field(0)]);
            path[0] = PlaceProjection::Field(1);
            changed += 1;
        }
    }
    assert_eq!(changed, 1);
    let errors = zil_analyze::analyze(&module, &limits).unwrap_err();
    assert!(errors[0].message.contains("stored reborrow selects another original field"), "{errors:?}");
    require_semantic_rejection(errors);
    plan.module_fingerprint = module.fingerprint();
    let wire = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    for result in [
        zil_verify::verify_module(module, plan, &target, &limits),
        zil_verify::verify_module_bytes(&wire, &target, &limits),
    ] {
        let errors = result.err().expect("same-type sibling changed the actual stored target");
        assert!(errors[0].message.contains("actual field index differs from the original ordered"), "{errors:?}");
        require_semantic_rejection(errors);
    }
}

#[test]
fn affine_package_initializer_keeps_its_original_owning_enum_field() {
    let limits = Limits::default();
    let target = TargetProfile::default();
    let text = r#"
resource Mark { pub code:i32 }
enum State { Held(manages Mark), Blank }
package Pair { left:State, right:State, n:i32 }
fn run(live:bool)->i32 {
    let pair=Pair{
        left:if live {State.Held(Mark{code:41})}else{State.Blank},
        right:State.Blank,n:1
    }
    let chosen=pair.left
    match chosen {
        State.Held(mark)=>{return mark.code+pair.n}
        State.Blank()=>{return pair.n}
    }
}
fn main()->i32{return run(true)+run(false)-43}
"#;
    let source = SourceUnit::new(FileId(0), ModuleId(0), "owning-enum-field.zil", text);
    let core = original_core(source, &limits, &target);
    let core = CoreProgram {
        root: ModuleId(0),
        source_fingerprint: core.source_fingerprint,
        modules: vec![ProgramModule { core, imports: vec![], exports: vec![] }],
    };
    let prepared = zil_types::infer_program_for_ct(&core, &limits).unwrap().into_prepared();
    let mut module = zil_types::check_prepared_program(&prepared, &target, &limits).unwrap().module;
    let mut plan = zil_analyze::analyze(&module, &limits).unwrap();
    let verified = |module: &TypedModule, plan: &CandidatePlan| {
        let wire = wire::encode_module_candidate(module, plan, &limits).unwrap();
        [
            zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits),
            zil_verify::verify_module_bytes(&wire, &target, &limits),
        ]
    };
    for result in verified(&module, &plan) { result.unwrap(); }

    let run = module.functions.iter_mut().find(|f| f.name == "run").unwrap();
    let mut changes = 0;
    for block in &mut run.blocks {
        for instruction in &mut block.instructions {
            if let InstructionKind::TakePackageField { local, index } = &mut instruction.kind
                && let Type::Package { fields, .. } = &run.locals[local.0 as usize].ty
                && fields.get(*index as usize).is_some_and(|(name, _)| name == "left")
            {
                assert_eq!(fields[1].0, "right");
                assert_eq!(fields[0].1, fields[1].1);
                *index = 1;
                changes += 1;
            }
        }
    }
    assert_eq!(changes, 1);
    let errors = zil_analyze::analyze(&module, &limits).unwrap_err();
    assert!(errors[0].message.contains("affine field initializer selects another original owner or field"), "{errors:?}");
    require_semantic_rejection(errors);
    plan.module_fingerprint = module.fingerprint();
    for result in verified(&module, &plan) {
        let errors = result.err().expect("same-type sibling must not replace the original owning field");
        assert!(errors[0].message.contains("actual field index differs from the original ordered"), "{errors:?}");
        require_semantic_rejection(errors);
    }
}
