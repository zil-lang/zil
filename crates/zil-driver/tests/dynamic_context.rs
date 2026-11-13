//! Concrete Context producers are retained as raw Typed input to both kernels.
use std::{collections::BTreeMap, fs, process::Command};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, NativeBackend, Optimization, ProjectSources, Stage, compile_project,
};

#[test]
fn borrowed_provider_produces_complete_typed_dependency_contract() {
    let text = r#"
struct Context<life a>{leaf:&'a i32}
struct Provider<life a>{pointer:&'a i32}
trait Read{fn read(self:&Self)->i32;}
impl<life a> Read for Provider<'a>{fn read(self:&Self)->i32{return *self.pointer}}
fn read_owned<life a>(p:Provider<'a>)->i32{
    match owned_dyn::<dyn Read,Context<'a>>(p){.err(_)=>{return 99},.ok(owner)=>{return owner.read()}}
}
fn create<life a>(value:&'a i32,other:&'a i32)->DynCreateError!OwnedDyn<dyn Read,Context<'a>>{
    return owned_dyn::<dyn Read,Context<'a>>(Provider::<'a>{pointer:value})
}
fn keep<life a>(owner:OwnedDyn<dyn Read,Context<'a>>)->OwnedDyn<dyn Read,Context<'a>>{
    return owner
}
fn start<life a>(value:&'a i32,other:&'a i32)->i32{
    match create(value,other){.err(_)=>{return 99},.ok(owner)=>{let forwarded=keep(owner);return forwarded.read()}}
}
fn main()->i32{let value=42;let other=99;return start(&value,&other)-42}
"#;
    let (project, module, candidate, dir) = context_candidate(text, "typed");
    let limits = zil_base::Limits::default();
    let target = zil_base::TargetProfile::default();
    let candidate_wire =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &limits).unwrap();
    let (decoded_module, decoded_candidate) =
        zil_ir::wire::decode_module_candidate(&candidate_wire, &limits).unwrap();
    let decoded_result =
        zil_verify::verify_module(decoded_module, decoded_candidate, &target, &limits);
    fs::write(
        dir.join("verify-decoded-result.txt"),
        format!("{decoded_result:?}"),
    )
    .unwrap();
    let cloned_result =
        zil_verify::verify_module(module.clone(), candidate.clone(), &target, &limits);
    fs::write(
        dir.join("verify-cloned-result.txt"),
        format!("{cloned_result:?}"),
    )
    .unwrap();
    assert!(decoded_result.is_ok(), "decoded: {decoded_result:?}");
    assert!(cloned_result.is_ok(), "cloned: {cloned_result:?}");
    let context_table = module
        .dyn_vtables
        .iter()
        .position(|t| !t.ports.is_empty())
        .unwrap();
    let mut attacks = Vec::new();
    let create = module
        .functions
        .iter()
        .position(|f| f.name == "create")
        .unwrap();
    let mut wrong = module.clone();
    wrong.functions[create].retained_return_sources = None;
    attacks.push(("unknown-helper-result-sources", wrong));
    let mut wrong = module.clone();
    wrong.functions[create].retained_return_sources = Some(vec![]);
    attacks.push(("empty-helper-result-upper", wrong));
    let mut wrong = module.clone();
    let zil_ir::StorageSource::Exact(source) = &mut wrong.functions[create]
        .retained_return_sources
        .as_mut()
        .unwrap()[0]
        .sources[0]
    else {
        panic!("expected the actual exact source");
    };
    source.base = zil_ir::StorageBase::Parameter(2);
    attacks.push(("wrong-helper-source-parameter", wrong));
    let mut wrong = module.clone();
    wrong.functions[create]
        .retained_return_sources
        .as_mut()
        .unwrap()[0]
        .sources = vec![zil_ir::StorageSource::Exact(zil_ir::StoragePosition {
        base: zil_ir::StorageBase::Parameter(1),
        path: vec![],
    })];
    attacks.push(("same-class-different-helper-source", wrong));
    let mut wrong = module.clone();
    let rows = wrong.functions[create]
        .retained_return_sources
        .as_mut()
        .unwrap();
    rows.push(rows[0].clone());
    attacks.push(("duplicate-helper-result-row", wrong));
    let mut wrong = module.clone();
    let function = &mut wrong.functions[create];
    let input = function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .find_map(|instruction| match instruction.kind {
            zil_ir::InstructionKind::Load(local) if local == function.parameters[0] => {
                instruction.result
            }
            _ => None,
        })
        .unwrap();
    let function_id = function.id;
    let allocation = function
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
        .find(|instruction| {
            matches!(
                instruction.kind,
                zil_ir::InstructionKind::MakeDynOwned { .. }
            )
        })
        .unwrap();
    allocation.kind = zil_ir::InstructionKind::Call {
        function: function_id,
        arguments: vec![input, input],
    };
    attacks.push(("recursive-upper-is-not-an-actual-producer", wrong));
    let mut wrong = module.clone();
    let caller = wrong
        .functions
        .iter_mut()
        .find(|f| f.name == "start")
        .unwrap();
    let conditions = caller
        .blocks
        .iter()
        .filter_map(|block| match block.terminator {
            zil_ir::Terminator::Branch { condition, .. } => Some(condition),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        conditions.len(),
        2,
        "fixture must retain both actual match guards"
    );
    for (index, condition) in conditions.iter().enumerate() {
        let definition = caller
            .blocks
            .iter_mut()
            .flat_map(|block| &mut block.instructions)
            .find(|instruction| instruction.result == Some(*condition))
            .unwrap();
        definition.kind = zil_ir::InstructionKind::Constant(zil_ir::Constant::Bool(index != 0));
    }
    attacks.push(("owner-take-without-any-actual-tag-guard", wrong));
    let mut missing = module.clone();
    missing.dyn_vtables[context_table].ports.clear();
    attacks.push(("missing-port", missing));
    let mut wrong = module.clone();
    wrong.dyn_vtables[context_table].ports[0].sources[0].path =
        vec![zil_ir::ContractProjection::Place(
            zil_ir::PlaceProjection::Field(9),
        )];
    attacks.push(("wrong-self-source", wrong));
    let mut wrong = module.clone();
    wrong.dyn_vtables[context_table].self_classes.clear();
    attacks.push(("missing-self-class", wrong));
    let mut wrong = module.clone();
    wrong.dyn_vtables[context_table].actuals[0] =
        zil_ir::DynGenericActual::Life(zil_ir::StorageBinder::Fixed(1));
    attacks.push(("wrong-impl-life", wrong));
    let mut wrong = module.clone();
    let implementation = wrong.dyn_vtables[context_table].implementation;
    wrong
        .dyn_declarations
        .iter_mut()
        .find(|d| d.id == implementation)
        .unwrap()
        .implemented_trait = None;
    attacks.push(("unknown-membership", wrong));
    for (name, attack) in attacks {
        assert!(
            zil_analyze::analyze(&attack, &limits).is_err(),
            "raw {name}"
        );
        let mut forged = candidate.clone();
        forged.module_fingerprint = attack.fingerprint();
        assert!(
            zil_verify::verify_module(attack.clone(), forged.clone(), &target, &limits).is_err(),
            "raw Verify {name}"
        );
        let wire = zil_ir::wire::encode_module_candidate(&attack, &forged, &limits).unwrap();
        let (decoded, forged) = zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap();
        assert!(
            zil_analyze::analyze(&decoded, &limits).is_err(),
            "Wire {name}"
        );
        assert!(
            zil_verify::verify_module(decoded, forged, &target, &limits).is_err(),
            "Wire Verify {name}"
        );
        fs::write(dir.join(format!("attack-{name}.wire")), wire).unwrap();
    }
    let candidate_wire =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &limits).unwrap();
    fs::write(dir.join("analyzed-unverified.wire"), &candidate_wire).unwrap();
    native_context(&project, &dir);
}

fn context_candidate(
    text: &str,
    label: &str,
) -> (
    ProjectSources,
    zil_ir::TypedModule,
    zil_ir::CandidatePlan,
    std::path::PathBuf,
) {
    let dir = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("dyn-context-{label}-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("main.zil"), text).unwrap();
    let project = ProjectSources {
        root: ModuleId(0),
        modules: vec![SourceUnit::new(FileId(0), ModuleId(0), "main.zil", text)],
        paths: BTreeMap::new(),
    };
    let report = compile_project(
        &project,
        &CompileOptions {
            stop_after: Stage::Types,
            emit_dir: Some(dir.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let limits = zil_base::Limits::default();
    let target = zil_base::TargetProfile::default();
    let source = &project.modules[0];
    let syntax = zil_syntax::parse(source).into_result().unwrap();
    let core = zil_expand::expand_with_map_limits_target_context(
        source,
        &syntax,
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: source.module_id,
            aliases: &BTreeMap::new(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let empty = zil_ir::CandidatePlan::empty_for(&module);
    let wire = zil_ir::wire::encode_module_candidate(&module, &empty, &limits).unwrap();
    fs::write(dir.join("typed-unverified-empty-plan.wire"), &wire).unwrap();
    let (decoded, plan) = zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap();
    assert_eq!(decoded, module);
    assert_eq!(plan, empty);
    let candidate = zil_analyze::analyze(&module, &limits).unwrap();
    let candidate_wire =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &limits).unwrap();
    fs::write(dir.join("analyzed-unverified.wire"), &candidate_wire).unwrap();
    (project, module, candidate, dir)
}
fn native_context(project: &ProjectSources, dir: &std::path::Path) {
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let run_dir = dir.join(format!("{backend:?}-{optimization:?}"));
            fs::create_dir_all(&run_dir).unwrap();
            let executable = run_dir.join("program");
            let report = compile_project(
                project,
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    output: Some(executable.clone()),
                    emit_dir: Some(run_dir.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(run_dir.join("compile.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let output = Command::new(executable).output().unwrap();
            assert_eq!(
                output.status.code(),
                Some(0),
                "{backend:?} {optimization:?}: {output:?}"
            );
            assert!(output.stdout.is_empty(), "{output:?}");
            assert!(output.stderr.is_empty(), "{output:?}");
        }
    }
}

#[test]
fn unused_context_port_is_explicitly_empty_without_becoming_unknown() {
    let text = r#"
struct Context<life a,life b>{left:&'a i32,right:&'b i32}
struct Provider<life a>{pointer:&'a i32}
trait Read{fn read(self:&Self)->i32;}
impl<life a> Read for Provider<'a>{fn read(self:&Self)->i32{return *self.pointer}}
fn run<life a,life b>(value:&'a i32,other:&'b i32)->i32{
    match owned_dyn::<dyn Read,Context<'a,'b>>(Provider::<'a>{pointer:value}){
        .err(_)=>{return 99},.ok(owner)=>{return owner.read()}
    }
}
fn main()->i32{let first=42;let second=99;return run(&first,&second)-42}
"#;
    let (project, module, candidate, dir) = context_candidate(text, "unused");
    let table = module
        .dyn_vtables
        .iter()
        .find(|t| t.ports.len() == 2)
        .unwrap();
    assert_eq!(table.ports[0].sources.len(), 1);
    assert!(table.ports[1].sources.is_empty());
    let limits = zil_base::Limits::default();
    let target = module.target.clone();
    zil_verify::verify_module(module.clone(), candidate.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &candidate, &limits).unwrap();
    let (module, candidate) = zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap();
    zil_verify::verify_module(module, candidate, &target, &limits).unwrap();
    native_context(&project, &dir);
}

#[test]
fn trait_object_definition_is_verified_without_a_local_provider() {
    let text = include_str!("../../../tests/language-readiness/pending/dyn-trait.zil");
    let project = ProjectSources {
        root: ModuleId(0),
        modules: vec![SourceUnit::new(FileId(0), ModuleId(0), "library.zil", text)],
        paths: BTreeMap::new(),
    };
    let report = compile_project(
        &project,
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

#[test]
fn exclusive_context_preserves_mutation_and_the_original_borrow_parent() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/dyn-context/exclusive-provider.zil"
    ))
    .unwrap();
    let (project, module, candidate, dir) = context_candidate(&text, "exclusive");
    let limits = zil_base::Limits::default();
    let target = module.target.clone();
    zil_verify::verify_module(module.clone(), candidate.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &candidate, &limits).unwrap();
    let (module, candidate) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
    zil_verify::verify_module(module, candidate, &target, &limits).unwrap();
    native_context(&project, &dir);
}

#[test]
fn retained_exclusive_context_rejects_original_mutable_ssa_writes_in_raw_and_wire() {
    let text = include_str!("../../../tests/fixtures/dyn-context/exclusive-provider.zil");
    let (_, module, _, _) = context_candidate(text, "exclusive-near-write");
    let mut attack = module.clone();
    let function = attack
        .functions
        .iter_mut()
        .find(|f| {
            f.blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .any(|i| matches!(i.kind, zil_ir::InstructionKind::MakeDynOwned { .. }))
        })
        .unwrap();
    let parameter = function.parameters[0];
    let address = function
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            matches!(i.kind,zil_ir::InstructionKind::Load(local)|zil_ir::InstructionKind::TakeLocal {local} if local==parameter)
                .then_some(i.result)
                .flatten()
        })
        .unwrap();
    assert!(matches!(
        function.values[address.0 as usize],
        zil_ir::Type::Address { mutable: true, .. }
    ));
    let constant = zil_ir::ValueId(function.values.len() as u32);
    function.values.push(zil_ir::Type::I32);
    let block = function
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, zil_ir::InstructionKind::MakeDynOwned { .. }))
        })
        .unwrap();
    let index = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, zil_ir::InstructionKind::MakeDynOwned { .. }))
        .unwrap()
        + 1;
    let span = block.instructions[index - 1].span;
    block.instructions.splice(
        index..index,
        [
            zil_ir::Instruction {
                result: Some(constant),
                kind: zil_ir::InstructionKind::Constant(zil_ir::Constant::I32(0)),
                span,
            },
            zil_ir::Instruction {
                result: None,
                kind: zil_ir::InstructionKind::WriteAddress {
                    address,
                    value: constant,
                },
                span,
            },
        ],
    );
    let limits = zil_base::Limits::default();
    let assert_rejected = |input: &zil_ir::TypedModule| {
        let errors = zil_analyze::analyze(input, &limits)
            .expect_err("retained child must exclude writes through original mutable SSA");
        assert!(
            errors.iter().any(|d| d.code == "E-ANALYZE-IR"
                && d.message
                    .contains("exclusive stored SSA value was used after transfer")),
            "{errors:?}"
        );
    };
    assert_rejected(&attack);
    let empty = zil_ir::CandidatePlan::empty_for(&attack);
    let bytes = zil_ir::wire::encode_module_candidate(&attack, &empty, &limits).unwrap();
    let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
    assert_rejected(&decoded);
}

#[test]
fn borrowed_exclusive_factory_keeps_a_distinct_retained_lease_after_call() {
    let text = include_str!("../../../tests/fixtures/dyn-context/exclusive-factory.zil");
    let (_, module, _, dir) = context_candidate(text, "exclusive-factory-lease");
    let mut attack = module.clone();
    let function=attack.functions.iter_mut().find(|f| f.blocks.iter().flat_map(|b|&b.instructions).any(|i|
        matches!(i.kind,zil_ir::InstructionKind::Call{..}) && i.result.is_some_and(|v|
            matches!(&f.values[v.0 as usize],zil_ir::Type::ErrorUnion{result,..} if matches!(result.as_ref(),zil_ir::Type::OwnedDyn{..}))))).unwrap();
    let call=function.blocks.iter().flat_map(|b|&b.instructions).find(|i|
        matches!(i.kind,zil_ir::InstructionKind::Call{..}) && i.result.is_some_and(|v|
            matches!(&function.values[v.0 as usize],zil_ir::Type::ErrorUnion{result,..} if matches!(result.as_ref(),zil_ir::Type::OwnedDyn{..})))).unwrap();
    let zil_ir::InstructionKind::Call { arguments, .. } = &call.kind else {
        unreachable!()
    };
    let address = arguments[0];
    let call_result = call.result;
    assert!(matches!(
        function.values[address.0 as usize],
        zil_ir::Type::Address { mutable: true, .. }
    ));
    let constant = zil_ir::ValueId(function.values.len() as u32);
    function.values.push(zil_ir::Type::I32);
    let block = function
        .blocks
        .iter_mut()
        .find(|b| b.instructions.iter().any(|i| i.result == call_result))
        .unwrap();
    let index = block
        .instructions
        .iter()
        .position(|i| i.result == call_result)
        .unwrap()
        + 1;
    let span = block.instructions[index - 1].span;
    block.instructions.splice(
        index..index,
        [
            zil_ir::Instruction {
                result: Some(constant),
                kind: zil_ir::InstructionKind::Constant(zil_ir::Constant::I32(0)),
                span,
            },
            zil_ir::Instruction {
                result: None,
                kind: zil_ir::InstructionKind::WriteAddress {
                    address,
                    value: constant,
                },
                span,
            },
        ],
    );
    let limits = zil_base::Limits::default();
    let raw_bytes = zil_ir::wire::encode_module_candidate(
        &attack,
        &zil_ir::CandidatePlan::empty_for(&attack),
        &limits,
    )
    .unwrap();
    fs::write(
        dir.join("original-call-argument-write-unverified.wire"),
        &raw_bytes,
    )
    .unwrap();
    let assert_rejected = |input: &zil_ir::TypedModule| {
        let outcome = zil_analyze::analyze(input, &limits);
        if let Ok(candidate) = &outcome {
            fs::write(
                dir.join("original-call-argument-write-accepted-by-analyze.wire"),
                zil_ir::wire::encode_module_candidate(input, candidate, &limits).unwrap(),
            )
            .unwrap();
        }
        let errors = outcome.expect_err("borrowed helper input must gain a retained child lease");
        assert!(
            errors.iter().any(|d| d.code == "E-BORROW-CONFLICT"),
            "{errors:?}"
        );
    };
    assert_rejected(&attack);
    let bytes = zil_ir::wire::encode_module_candidate(
        &attack,
        &zil_ir::CandidatePlan::empty_for(&attack),
        &limits,
    )
    .unwrap();
    let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
    assert_rejected(&decoded);
}

#[test]
fn actual_inline_field_address_uses_an_explicit_place_address_source() {
    let text = include_str!("../../../tests/fixtures/dyn-context/field-address-factory.zil");
    let (project, module, candidate, dir) = context_candidate(text, "place-address");
    let factory = module
        .functions
        .iter()
        .find(|f| f.name == "create")
        .unwrap();
    assert!(
        factory
            .retained_return_sources
            .as_ref()
            .unwrap()
            .iter()
            .flat_map(|row| &row.sources)
            .any(
                |source| matches!(source,zil_ir::StorageSource::PlaceAddress{place,..}
            if place.path==[zil_ir::ContractProjection::Place(zil_ir::PlaceProjection::Field(0))])
            )
    );
    let limits = zil_base::Limits::default();
    let target = module.target.clone();
    zil_verify::verify_module(module.clone(), candidate.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &candidate, &limits).unwrap();
    let (decoded, plan) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
    zil_verify::verify_module(decoded, plan, &target, &limits).unwrap();
    native_context(&project, &dir);
}

#[test]
fn exclusive_context_factory_preserves_repeated_mutations_and_cleanup() {
    let text = include_str!("../../../tests/fixtures/dyn-context/exclusive-factory.zil");
    let (project, module, candidate, dir) = context_candidate(text, "exclusive-factory-native");
    let limits = zil_base::Limits::default();
    let target = module.target.clone();
    zil_verify::verify_module(module.clone(), candidate.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &candidate, &limits).unwrap();
    let (decoded, plan) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
    zil_verify::verify_module(decoded, plan, &target, &limits).unwrap();
    native_context(&project, &dir);
}

#[test]
fn inline_address_contract_never_becomes_a_same_typed_field_or_stored_load() {
    let text = include_str!("../../../tests/fixtures/dyn-context/field-address-factory.zil");
    let (_, module, _, _) = context_candidate(text, "place-address-attacks");
    let factory = module
        .functions
        .iter()
        .position(|f| f.name == "create")
        .unwrap();
    let written = module.functions[factory]
        .retained_return_sources
        .as_ref()
        .unwrap();
    let field0 = written[0]
        .sources
        .iter()
        .find(|source| {
            matches!(source,zil_ir::StorageSource::PlaceAddress{place,..}
        if place.path==[zil_ir::ContractProjection::Place(zil_ir::PlaceProjection::Field(0))])
        })
        .unwrap()
        .clone();
    let mut cases = Vec::new();
    for mutation in 0..7 {
        let mut attack = module.clone();
        let rows = attack.functions[factory]
            .retained_return_sources
            .as_mut()
            .unwrap();
        rows[0].sources = vec![field0.clone()];
        match mutation {
            0 => {
                let zil_ir::StorageSource::PlaceAddress { place, .. } = &mut rows[0].sources[0]
                else {
                    unreachable!()
                };
                place.path = vec![zil_ir::ContractProjection::Place(
                    zil_ir::PlaceProjection::Field(1),
                )]
            }
            1 => {
                let zil_ir::StorageSource::PlaceAddress { place, .. } = &rows[0].sources[0] else {
                    unreachable!()
                };
                rows[0].sources = vec![zil_ir::StorageSource::Exact(place.clone())]
            }
            2 => {
                let zil_ir::StorageSource::PlaceAddress { place, .. } = &mut rows[0].sources[0]
                else {
                    unreachable!()
                };
                place.path = vec![zil_ir::ContractProjection::Place(
                    zil_ir::PlaceProjection::Field(9),
                )]
            }
            3 => {
                let zil_ir::StorageSource::PlaceAddress { mode, .. } = &mut rows[0].sources[0]
                else {
                    unreachable!()
                };
                *mode = zil_ir::StoredBorrowMode::ExclusiveAddress
            }
            4 => rows[0].sources.clear(),
            5 => {
                let row = rows[0].clone();
                rows.push(row)
            }
            _ => rows[0].sources.push(field0.clone()),
        }
        cases.push((mutation, attack));
    }
    let limits = zil_base::Limits::default();
    for (case, attack) in cases {
        let assert_rejected = |input: &zil_ir::TypedModule| {
            let errors = zil_analyze::analyze(input, &limits)
                .expect_err("invalid address-of source cannot authorize a result");
            assert!(
                errors.iter().any(|d| d.code == "E-ANALYZE-IR"),
                "case {case}: {errors:?}"
            );
        };
        assert_rejected(&attack);
        let bytes = zil_ir::wire::encode_module_candidate(
            &attack,
            &zil_ir::CandidatePlan::empty_for(&attack),
            &limits,
        )
        .unwrap();
        let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        assert_rejected(&decoded);
    }
}

#[test]
fn shared_context_field_view_reborrows_an_actual_mutable_input() {
    let text = include_str!("../../../tests/fixtures/dyn-context/shared-field-from-mutable.zil");
    let (project, module, candidate, dir) = context_candidate(text, "shared-from-mutable");
    let factory = module
        .functions
        .iter()
        .find(|f| f.name == "create")
        .unwrap();
    assert!(
        factory
            .retained_return_sources
            .as_ref()
            .unwrap()
            .iter()
            .flat_map(|r| &r.sources)
            .any(|source| matches!(
                source,
                zil_ir::StorageSource::PlaceAddress {
                    mode: zil_ir::StoredBorrowMode::SharedAddress,
                    ..
                }
            ))
    );
    assert!(
        factory
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(
                i.kind,
                zil_ir::InstructionKind::ReborrowAddress { mutable: false, .. }
            )),
        "{factory:#?}"
    );
    let limits = zil_base::Limits::default();
    let target = module.target.clone();
    zil_verify::verify_module(module.clone(), candidate.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &candidate, &limits).unwrap();
    let (decoded, plan) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
    zil_verify::verify_module(decoded, plan, &target, &limits).unwrap();
    native_context(&project, &dir);
}

#[test]
fn static_slice_target_keeps_its_nonzero_context_with_an_input_header() {
    let text = include_str!("../../../tests/fixtures/dyn-context/static-slice-with-input.zil");
    let (project, module, candidate, dir) = context_candidate(text, "static-with-input");
    let context = module
        .dyn_interfaces
        .iter()
        .find(|i| i.context.is_some())
        .unwrap();
    assert_eq!(context.retained_life.binder_count, 1);
    assert_eq!(context.retained_life.ports.len(), 1);
    assert!(!context.retained_life.nominal_arguments.is_empty());
    let limits = zil_base::Limits::default();
    let target = module.target.clone();
    zil_verify::verify_module(module.clone(), candidate.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &candidate, &limits).unwrap();
    let (decoded, plan) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
    zil_verify::verify_module(decoded, plan, &target, &limits).unwrap();
    native_context(&project, &dir);
}

#[test]
fn static_slice_target_keeps_its_nonzero_context_without_input_roots() {
    let text = include_str!("../../../tests/fixtures/dyn-context/static-slice-without-input.zil");
    let (project, module, candidate, dir) = context_candidate(text, "static-without-input");
    let context = module
        .dyn_interfaces
        .iter()
        .find(|i| i.context.is_some())
        .unwrap();
    assert_eq!(context.retained_life.binder_count, 1);
    assert!(!context.retained_life.nominal_arguments.is_empty());
    let factory = module
        .functions
        .iter()
        .find(|f| f.name == "create")
        .unwrap();
    assert!(factory.parameters.is_empty());
    let life = factory.storage_life.as_ref().unwrap();
    assert_eq!(
        (life.fixed_binder_count, life.invocation_binder_count),
        (0, 1)
    );
    assert!(life.bindings.is_empty() && life.static_bindings.is_empty());
    assert_eq!(life.classes.len(), 1);
    assert_eq!(life.classes[0].binder, zil_ir::StorageBinder::Invocation(0));
    assert_eq!(life.classes[0].position.base, zil_ir::StorageBase::Result);
    assert_eq!(
        life.classes[0].position.path,
        [zil_ir::LifeTypeProjection::Place(
            zil_ir::PlaceProjection::ErrorArm(false)
        )]
    );
    let limits = zil_base::Limits::default();
    let target = module.target.clone();
    zil_verify::verify_module(module.clone(), candidate.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &candidate, &limits).unwrap();
    let (decoded, plan) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
    zil_verify::verify_module(decoded, plan, &target, &limits).unwrap();
    native_context(&project, &dir);
}

#[test]
fn static_context_sources_are_distinct_from_same_typed_formal_and_empty_targets() {
    use zil_ir::*;
    let text = include_str!("../../../tests/fixtures/dyn-context/static-slice-with-input.zil");
    let (_, module, _, _) = context_candidate(text, "static-same-leaf-attacks");
    let factory = module
        .functions
        .iter()
        .position(|f| f.name == "create")
        .unwrap();
    let input = module.functions[factory].parameters[0];
    let exact = StorageSource::Exact(StoragePosition {
        base: StorageBase::Parameter(0),
        path: vec![],
    });
    let static_atom = module.functions[factory]
        .retained_return_sources
        .as_ref()
        .unwrap()[0]
        .sources
        .iter()
        .find(|a| matches!(a, StorageSource::Static { .. }))
        .unwrap()
        .clone();
    let Type::Address { mutable: false, .. } =
        module.functions[factory].locals[input.0 as usize].ty
    else {
        panic!("full same-typed input required")
    };
    let limits = zil_base::Limits::default();
    let roundtrip = |module: &TypedModule| {
        let bytes = zil_ir::wire::encode_module_candidate(
            module,
            &CandidatePlan::empty_for(module),
            &limits,
        )
        .unwrap();
        zil_ir::wire::decode_module_candidate(&bytes, &limits)
            .unwrap()
            .0
    };
    for mutation in 0..5 {
        let mut attack = module.clone();
        let f = &mut attack.functions[factory];
        f.retained_return_sources.as_mut().unwrap()[0].sources = vec![static_atom.clone()];
        match mutation {
            0 => f.retained_return_sources.as_mut().unwrap()[0].sources = vec![exact.clone()],
            1 => f.retained_return_sources.as_mut().unwrap()[0]
                .sources
                .clear(),
            2 => {
                let instruction = f
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, InstructionKind::StaticUtf8 { .. }))
                    .unwrap();
                // Same full SSA result Type and same class; only the actual
                // target changes from static bytes to the input permission.
                assert_eq!(
                    f.values[instruction.result.unwrap().0 as usize],
                    f.locals[input.0 as usize].ty
                );
                instruction.kind = InstructionKind::Load(input);
            }
            3 => f.storage_life.as_mut().unwrap().classes.clear(),
            _ => f.retained_return_sources.as_mut().unwrap()[0]
                .sources
                .push(static_atom.clone()),
        }
        for input_module in [&attack, &roundtrip(&attack)] {
            let errors = zil_analyze::analyze(input_module, &limits).expect_err(
                "static dependency proof must use actual targets and complete occurrences",
            );
            assert!(
                errors.iter().any(|d| d.code == "E-ANALYZE-IR"),
                "case {mutation}: {errors:?}"
            );
        }
        if mutation == 2 {
            // A real input target is valid under Exact, with the same body,
            // header and full leaf that was rejected under the Static claim.
            attack.functions[factory]
                .retained_return_sources
                .as_mut()
                .unwrap()[0]
                .sources = vec![exact.clone()];
            zil_analyze::analyze(&attack, &limits).unwrap();
            zil_analyze::analyze(&roundtrip(&attack), &limits).unwrap();
        }
    }
}

#[test]
fn retained_field_child_conflicts_only_with_the_actual_projected_window() {
    use zil_ir::*;
    let text = include_str!("../../../tests/fixtures/dyn-context/shared-field-from-mutable.zil");
    let (_, mut module, _, _) = context_candidate(text, "retained-field-sibling");
    let factory = module
        .functions
        .iter()
        .position(|f| f.name == "create")
        .unwrap();
    module.functions[factory]
        .retained_return_sources
        .as_mut()
        .unwrap()[0]
        .sources
        .retain(|source| {
            matches!(source, StorageSource::PlaceAddress { place, .. }
            if place.path == [ContractProjection::Place(PlaceProjection::Field(0))])
        });
    assert_eq!(
        module.functions[factory]
            .retained_return_sources
            .as_ref()
            .unwrap()[0]
            .sources
            .len(),
        1
    );
    let limits = zil_base::Limits::default();
    zil_analyze::analyze(&module, &limits).unwrap();
    for field in [0, 1] {
        let mut changed = module.clone();
        let f = changed
            .functions
            .iter_mut()
            .find(|f| f.name == "run")
            .unwrap();
        let (block_index, call_index, argument) = f
            .blocks
            .iter()
            .enumerate()
            .find_map(|(bi, block)| {
                block
                    .instructions
                    .iter()
                    .enumerate()
                    .find_map(|(ii, instruction)| {
                        if let InstructionKind::Call {
                            function,
                            arguments,
                        } = &instruction.kind
                            && function.0 as usize == factory
                        {
                            Some((bi, ii, arguments[0]))
                        } else {
                            None
                        }
                    })
            })
            .unwrap();
        assert!(matches!(
            f.values[argument.0 as usize],
            Type::Address { mutable: true, .. }
        ));
        let projected = ValueId(f.values.len() as u32);
        f.values.push(Type::Address {
            mutable: true,
            inner: Box::new(Type::I32),
        });
        let constant = ValueId(f.values.len() as u32);
        f.values.push(Type::I32);
        let block = &mut f.blocks[block_index];
        let span = block.instructions[call_index].span;
        block.instructions.splice(
            call_index + 1..call_index + 1,
            [
                Instruction {
                    result: Some(projected),
                    kind: InstructionKind::AddressField {
                        address: argument,
                        index: field,
                    },
                    span,
                },
                Instruction {
                    result: Some(constant),
                    kind: InstructionKind::Constant(Constant::I32(0)),
                    span,
                },
                Instruction {
                    result: None,
                    kind: InstructionKind::WriteAddress {
                        address: projected,
                        value: constant,
                    },
                    span,
                },
            ],
        );
        let bytes = zil_ir::wire::encode_module_candidate(
            &changed,
            &CandidatePlan::empty_for(&changed),
            &limits,
        )
        .unwrap();
        let decoded = zil_ir::wire::decode_module_candidate(&bytes, &limits)
            .unwrap()
            .0;
        for input in [&changed, &decoded] {
            let outcome = zil_analyze::analyze(input, &limits);
            if field == 0 {
                let errors =
                    outcome.expect_err("the retained left child must forbid writes to left");
                assert!(
                    errors.iter().any(|d| d.code == "E-BORROW-CONFLICT"),
                    "{errors:?}"
                );
            } else {
                outcome.expect(
                    "writing the disjoint right field must preserve the retained left child",
                );
            }
        }
    }
}

#[test]
fn static_context_preserves_an_unused_phantom_class_without_a_target() {
    use zil_ir::*;
    let text = include_str!("../../../tests/fixtures/dyn-context/static-slice-phantom.zil");
    let (project, module, candidate, dir) = context_candidate(text, "static-phantom");
    let iface = module
        .dyn_interfaces
        .iter()
        .find(|i| i.context.is_some())
        .unwrap();
    assert_eq!(iface.retained_life.binder_count, 2);
    assert_eq!(iface.retained_life.ports.len(), 1);
    assert_eq!(iface.retained_life.ports[0].binder, 0);
    assert_eq!(iface.retained_life.nominal_arguments.len(), 2);
    let factory = module
        .functions
        .iter()
        .find(|f| f.name == "create")
        .unwrap();
    let life = factory.storage_life.as_ref().unwrap();
    assert!(factory.parameters.is_empty());
    assert_eq!(life.classes.len(), 2);
    assert_eq!(
        (life.fixed_binder_count, life.invocation_binder_count),
        (0, 2)
    );
    assert_eq!(life.classes[0].binder, StorageBinder::Invocation(0));
    assert_eq!(life.classes[1].binder, StorageBinder::Invocation(1));
    assert_eq!(life.classes[0].position.class, 0);
    assert_eq!(life.classes[1].position.class, 1);
    let limits = zil_base::Limits::default();
    let target = module.target.clone();
    zil_verify::verify_module(module.clone(), candidate.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &candidate, &limits).unwrap();
    let (decoded, plan) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
    zil_verify::verify_module(decoded, plan, &target, &limits).unwrap();
    native_context(&project, &dir);
}

#[test]
fn nested_owned_regions_are_explicit_and_keep_the_actual_head() {
    use zil_ir::*;
    let text = include_str!("../../../tests/fixtures/dyn-context/owned-inline-region.zil");
    let (project, module, candidate, dir) = context_candidate(text, "owned-inline-region");
    let f = module
        .functions
        .iter()
        .find(|f| f.name == "create")
        .unwrap();
    let sources = &f.retained_return_sources.as_ref().unwrap()[0].sources;
    assert_eq!(sources.len(), 2);
    assert!(
        sources
            .iter()
            .all(|source| matches!(source, StorageSource::OwnedView { .. }))
    );
    let limits = zil_base::Limits::default();
    let target = module.target.clone();
    zil_verify::verify_module(module.clone(), candidate.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &candidate, &limits).unwrap();
    let (decoded, plan) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
    zil_verify::verify_module(decoded, plan, &target, &limits).unwrap();
    native_context(&project, &dir);
}

#[test]
fn owned_region_claims_cannot_exchange_same_typed_heads_or_cross_pointer_selectors() {
    use zil_ir::*;
    let text = include_str!("../../../tests/fixtures/dyn-context/owned-inline-region.zil");
    let (_, module, _, _) = context_candidate(text, "owned-region-attacks");
    let factory = module
        .functions
        .iter()
        .position(|f| f.name == "create")
        .unwrap();
    let first = module.functions[factory]
        .retained_return_sources
        .as_ref()
        .unwrap()[0]
        .sources
        .iter()
        .find(|source| {
            matches!(source, StorageSource::OwnedView { head, .. }
            if head.path == [ContractProjection::Place(PlaceProjection::Field(0))])
        })
        .unwrap()
        .clone();
    let limits = zil_base::Limits::default();
    let roundtrip = |input: &TypedModule| {
        let bytes =
            zil_ir::wire::encode_module_candidate(input, &CandidatePlan::empty_for(input), &limits)
                .unwrap();
        zil_ir::wire::decode_module_candidate(&bytes, &limits)
            .unwrap()
            .0
    };
    let mut narrow = module.clone();
    narrow.functions[factory]
        .retained_return_sources
        .as_mut()
        .unwrap()[0]
        .sources = vec![first.clone()];
    zil_analyze::analyze(&narrow, &limits).unwrap();
    zil_analyze::analyze(&roundtrip(&narrow), &limits).unwrap();
    for mutation in 0..6 {
        let mut attack = narrow.clone();
        let sources = &mut attack.functions[factory]
            .retained_return_sources
            .as_mut()
            .unwrap()[0]
            .sources;
        match mutation {
            0 => {
                let StorageSource::OwnedView { head, .. } = &mut sources[0] else {
                    unreachable!()
                };
                head.path = vec![ContractProjection::Place(PlaceProjection::Field(1))];
            }
            1 => {
                let StorageSource::OwnedView { head, .. } = &sources[0] else {
                    unreachable!()
                };
                sources[0] = StorageSource::Exact(head.clone());
            }
            2 => {
                let StorageSource::OwnedView { leaf, .. } = &mut sources[0] else {
                    unreachable!()
                };
                *leaf = Type::Address {
                    mutable: false,
                    inner: Box::new(Type::U8),
                };
            }
            3 => {
                let StorageSource::OwnedView { mode, .. } = &mut sources[0] else {
                    unreachable!()
                };
                *mode = StoredBorrowMode::ExclusiveAddress;
            }
            4 => sources.push(first.clone()),
            _ => {
                let StorageSource::OwnedView { head, .. } = &mut sources[0] else {
                    unreachable!()
                };
                head.path = vec![ContractProjection::RetainedPort(0)];
            }
        }
        for input in [&attack, &roundtrip(&attack)] {
            let errors = zil_analyze::analyze(input, &limits)
                .expect_err("only the actual owning region can authorize its targets");
            assert!(
                errors.iter().any(|d| d.code == "E-ANALYZE-IR"),
                "case {mutation}: {errors:?}"
            );
        }
        if mutation == 0 {
            let f = &mut attack.functions[factory];
            let instruction = f
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| {
                    if let InstructionKind::AddressField { address, index: 0 } = i.kind {
                        matches!(&f.values[address.0 as usize],Type::Address{inner,..}
                        if matches!(inner.as_ref(),Type::Record{name,..} if name.contains("Frame")))
                    } else {
                        false
                    }
                })
                .unwrap();
            let InstructionKind::AddressField { index, .. } = &mut instruction.kind else {
                unreachable!()
            };
            *index = 1;
            // The same head claim becomes valid only after the actual physical
            // projection changes too, with all full Types/classes unchanged.
            zil_analyze::analyze(&attack, &limits).unwrap();
            zil_analyze::analyze(&roundtrip(&attack), &limits).unwrap();
        }
    }
}

#[test]
fn holder_factory_preserves_the_actual_saved_leaf_and_its_fixed_class() {
    use zil_ir::*;
    let text = include_str!("../../../tests/fixtures/dyn-context/holder-leaf-factory.zil");
    let (project, module, candidate, dir) = context_candidate(text, "holder-leaf-factory");
    let f = module
        .functions
        .iter()
        .find(|f| f.name == "create")
        .unwrap();
    let life = f.storage_life.as_ref().unwrap();
    assert!(
        life.bindings
            .iter()
            .any(|row| row.position.base == StorageBase::Parameter(0)
                && row.position.path.is_empty()
                && row.binder == StorageBinder::Invocation(0))
    );
    assert!(life.classes.iter().any(
        |row| row.position.base == StorageBase::Result && row.binder == StorageBinder::Fixed(0)
    ));
    assert!(f.retained_return_sources.as_ref().unwrap()[0].sources.iter().any(|source|
        matches!(source, StorageSource::Exact(position) if position.base==StorageBase::Parameter(0)
            &&position.path==[ContractProjection::Place(PlaceProjection::Field(0))])));
    let limits = zil_base::Limits::default();
    let target = module.target.clone();
    zil_verify::verify_module(module.clone(), candidate.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &candidate, &limits).unwrap();
    let (decoded, plan) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
    zil_verify::verify_module(decoded, plan, &target, &limits).unwrap();
    native_context(&project, &dir);
}

#[test]
fn current_dynamic_fixture_sources_produce_fresh_typed_and_analyzed_candidates() {
    use zil_ir::*;
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../zil-verify/tests/fixtures/dyn-context");
    let limits = zil_base::Limits::default();
    for name in [
        "context-read",
        "context-factory",
        "context-unused",
        "context-exclusive",
        "context-exclusive-factory",
        "context-place-address",
        "context-static-input",
        "context-static-no-input",
        "context-static-phantom",
        "context-owned-region",
        "static-callback-getter-cycle",
    ] {
        let source = fs::read_to_string(fixtures.join(format!("{name}.zil"))).unwrap();
        let (_, module, plan, dir) = context_candidate(&source, &format!("current-{name}"));
        zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits).unwrap();
        let bytes = fs::read(dir.join("analyzed-unverified.wire")).unwrap();
        let (decoded, candidate) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        zil_verify::verify_module(decoded, candidate, &module.target, &limits).unwrap();
        if name != "context-exclusive-factory" {
            continue;
        }
        let mut attack = module.clone();
        let function = attack
            .functions
            .iter_mut()
            .find(|f| f.name == "run")
            .unwrap();
        let (block_index,call_index,address)=function.blocks.iter().enumerate().find_map(|(bi,block)|{
            block.instructions.iter().enumerate().find_map(|(ii,instruction)|{
                if let InstructionKind::Call{arguments,..}=&instruction.kind
                    &&instruction.result.is_some_and(|id|matches!(&function.values[id.0 as usize],Type::ErrorUnion{result,..}if matches!(result.as_ref(),Type::OwnedDyn{..}))) {
                    Some((bi,ii,arguments[0]))
                } else {None}
            })
        }).unwrap();
        let id = function.id;
        let constant = ValueId(function.values.len() as u32);
        function.values.push(Type::I32);
        let block = &mut function.blocks[block_index];
        let span = block.instructions[call_index].span;
        block.instructions.splice(
            call_index + 1..call_index + 1,
            [
                Instruction {
                    result: Some(constant),
                    kind: InstructionKind::Constant(Constant::I32(0)),
                    span,
                },
                Instruction {
                    result: None,
                    kind: InstructionKind::WriteAddress {
                        address,
                        value: constant,
                    },
                    span,
                },
            ],
        );
        let mut forged = plan.clone();
        forged.module_fingerprint = attack.fingerprint();
        forged
            .functions
            .iter_mut()
            .find(|f| f.function == id)
            .unwrap()
            .holds
            .push(ValueHold {
                value: constant,
                roots: vec![],
            });
        let bytes = zil_ir::wire::encode_module_candidate(&attack, &forged, &limits).unwrap();
        fs::write(
            dir.join("current-original-argument-write.candidate.wire"),
            &bytes,
        )
        .unwrap();
        let (decoded, candidate) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        for (module, plan) in [(attack.clone(), forged.clone()), (decoded, candidate)] {
            let errors = zil_analyze::analyze(&module, &limits)
                .expect_err("a retained child forbids writing its exact borrowed call operand");
            assert!(
                errors.iter().any(|d| d.code == "E-BORROW-CONFLICT"),
                "{errors:?}"
            );
            let errors =
                zil_verify::verify_module(module, plan, &attack.target, &limits).unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|d| d.message.contains("live incompatible loan")),
                "{errors:?}"
            );
        }
        // With precisely the conflicting write removed, the extra constant
        // and its legitimate empty Hold remain accepted by independent V.
        attack.functions[id.0 as usize].blocks[block_index]
            .instructions
            .remove(call_index + 2);
        forged.module_fingerprint = attack.fingerprint();
        zil_verify::verify_module(attack.clone(), forged.clone(), &attack.target, &limits).unwrap();
        let bytes = zil_ir::wire::encode_module_candidate(&attack, &forged, &limits).unwrap();
        let (decoded, candidate) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        zil_verify::verify_module(decoded, candidate, &attack.target, &limits).unwrap();
    }
}

#[test]
fn context_owner_survives_repeated_calls_from_a_moved_environment() {
    let text = include_str!("../../../tests/fixtures/dyn-context/captured-owner.zil");
    let (project, module, candidate, dir) = context_candidate(text, "captured-owner");
    let limits = zil_base::Limits::default();
    zil_verify::verify_module(module.clone(), candidate.clone(), &module.target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &candidate, &limits).unwrap();
    let (decoded, plan) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
    zil_verify::verify_module(decoded, plan, &module.target, &limits).unwrap();
    native_context(&project, &dir);
}

#[test]
fn captured_context_scope_and_every_returned_port_source_are_required_in_raw_and_wire() {
    let text = include_str!("../../../tests/fixtures/dyn-context/captured-owner.zil");
    let (_, module, _, _) = context_candidate(text, "captured-owner-claims");
    let limits = zil_base::Limits::default();
    let closure = module.closures.iter().find(|d| {
        d.captures.iter().any(|ty| matches!(ty, zil_ir::Type::OwnedDyn { interface }
            if module.dyn_interfaces.iter().any(|i|i.id==*interface && i.retained_life.binder_count>0)))
    }).expect("the actual Source must capture a nonzero Context owner");
    assert!(
        closure
            .storage_life
            .as_ref()
            .is_some_and(|c| c.fixed_binder_count > 0)
    );
    let forwarding = module
        .functions
        .iter()
        .find(|f| {
            f.callable_return_contract.as_ref().is_some_and(|c| {
                c.closure == closure.id
                    && matches!(c.source, zil_ir::CallableReturnSource::Parameter(_))
            })
        })
        .expect("the same Source must forward the actual callable");
    assert!(
        forwarding
            .callable_return_contract
            .as_ref()
            .unwrap()
            .capture_sources
            .as_ref()
            .is_some_and(|rows| rows.iter().any(|row| matches!(
                row.position.path.last(),
                Some(zil_ir::ContractProjection::RetainedPort(_))
            )))
    );
    assert!(
        forwarding
            .callable_return_contract
            .as_ref()
            .unwrap()
            .life
            .as_ref()
            .is_some_and(|life| life.fixed_binder_count > 0)
    );
    for attack in 0..4 {
        let mut changed = module.clone();
        match attack {
            0 => changed.closures[closure.id.0 as usize].storage_life = None,
            1 => {
                changed.closures[closure.id.0 as usize].storage_life = None;
                changed.functions[closure.function.0 as usize].storage_life = None;
            }
            2 => changed.functions[forwarding.id.0 as usize]
                .callable_return_contract
                .as_mut()
                .unwrap()
                .capture_sources
                .as_mut()
                .unwrap()
                .clear(),
            _ => {
                changed.functions[forwarding.id.0 as usize]
                    .callable_return_contract
                    .as_mut()
                    .unwrap()
                    .life = None
            }
        }
        assert!(
            zil_analyze::analyze(&changed, &limits).is_err(),
            "raw attack {attack}"
        );
        let bytes = zil_ir::wire::encode_module_candidate(
            &changed,
            &zil_ir::CandidatePlan::empty_for(&changed),
            &limits,
        )
        .unwrap();
        let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        assert!(
            zil_analyze::analyze(&decoded, &limits).is_err(),
            "Wire attack {attack}"
        );
    }
}

#[test]
fn context_owner_survives_actual_initialized_store_element_transport() {
    let text = include_str!("../../../tests/fixtures/dyn-context/store-owner.zil");
    let (project, module, candidate, dir) = context_candidate(text, "store-owner");
    let limits = zil_base::Limits::default();
    zil_verify::verify_module(module.clone(), candidate.clone(), &module.target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &candidate, &limits).unwrap();
    let (decoded, plan) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
    zil_verify::verify_module(decoded, plan, &module.target, &limits).unwrap();
    native_context(&project, &dir);
}

#[test]
fn ordinary_dynamic_library_definition_needs_no_local_provider() {
    let text = include_str!("../../../tests/fixtures/dyn-context/library-without-provider.zil");
    let (_, module, candidate, _) = context_candidate(text, "library-without-provider");
    assert!(
        module.dyn_vtables.is_empty(),
        "the library must be checked without a local impl"
    );
    assert_eq!(module.dyn_interfaces.len(), 1);
    assert!(
        module.dyn_interfaces[0].slots[0]
            .contract
            .effects
            .as_ref()
            .is_some_and(|effects| effects.contains(&zil_ir::DynEffect::Panic)),
        "a declaration's conservative Direct upper must not depend on current provider bodies"
    );
    let limits = zil_base::Limits::default();
    zil_verify::verify_module(module.clone(), candidate.clone(), &module.target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &candidate, &limits).unwrap();
    let (decoded, plan) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
    zil_verify::verify_module(decoded, plan, &module.target, &limits).unwrap();
}

#[test]
fn actual_literal_is_required_across_callback_dynamic_factory_and_long_getter_cycle() {
    let text = include_str!(
        "../../../tests/fixtures/dyn-context/static-callback-getter-cycle-control.zil"
    );
    let (_, module, _, dir) = context_candidate(text, "static-callback-getter-cycle");
    let limits = zil_base::Limits::default();
    let callback=module.closures.iter().find(|d|matches!(&d.result,zil_ir::Type::Address{inner,..}if matches!(inner.as_ref(),zil_ir::Type::Slice{..}))).unwrap();
    let feedback = module
        .functions
        .iter()
        .find(|f| f.name == "feedback")
        .unwrap()
        .id;
    let entry_id = callback.function;
    let original = &module.functions[entry_id.0 as usize];
    assert!(
        original
            .storage_life
            .as_ref()
            .is_some_and(|life| life
                .static_bindings
                .iter()
                .any(|position| position.base == zil_ir::StorageBase::Result
                    && position.path.is_empty())),
        "the actual Source must propose a constant result; an invocation-only result is a different program"
    );

    let literal_count = original
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .filter(|i| matches!(i.kind, zil_ir::InstructionKind::StaticUtf8 { .. }))
        .count();
    assert_eq!(
        literal_count, 2,
        "both original callback returns must be actual literals"
    );
    for keep_literal in [false, true] {
        let mut changed = module.clone();
        let entry = &mut changed.functions[entry_id.0 as usize];
        let parameters = entry.parameters.clone();
        let mut first = true;
        for block in &mut entry.blocks {
            let old = std::mem::take(&mut block.instructions);
            for mut instruction in old {
                if matches!(instruction.kind, zil_ir::InstructionKind::StaticUtf8 { .. }) {
                    if keep_literal && first {
                        first = false;
                        block.instructions.push(instruction);
                        continue;
                    }
                    // This real literal is solely an unused helper input.
                    // It must not ground the callback's returned data cycle.
                    let seed = zil_ir::ValueId(entry.values.len() as u32);
                    entry.values.push(original.return_type.clone());
                    block.instructions.push(zil_ir::Instruction {
                        result: Some(seed),
                        kind: zil_ir::InstructionKind::StaticUtf8 {
                            bytes: b"xyz".to_vec(),
                        },
                        span: instruction.span,
                    });
                    let mut args = vec![seed];
                    for local in &parameters {
                        let value = zil_ir::ValueId(entry.values.len() as u32);
                        entry.values.push(entry.locals[local.0 as usize].ty.clone());
                        block.instructions.push(zil_ir::Instruction {
                            result: Some(value),
                            kind: zil_ir::InstructionKind::Load(*local),
                            span: instruction.span,
                        });
                        args.push(value);
                    }
                    instruction.kind = zil_ir::InstructionKind::Call {
                        function: feedback,
                        arguments: args,
                    };
                }
                block.instructions.push(instruction);
            }
        }
        let label = if keep_literal {
            "literal-control"
        } else {
            "rootless-cycle"
        };
        let empty = zil_ir::CandidatePlan::empty_for(&changed);
        let bytes = zil_ir::wire::encode_module_candidate(&changed, &empty, &limits).unwrap();
        fs::write(dir.join(format!("{label}.untrusted.wire")), &bytes).unwrap();
        let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        for (route, input) in [("raw", changed), ("wire", decoded)] {
            let actual = zil_analyze::analyze(&input, &limits);
            fs::write(
                dir.join(format!("{label}-{route}.txt")),
                format!("{actual:?}"),
            )
            .unwrap();
            if keep_literal {
                let candidate =
                    actual.expect("an actual literal grounds the same callback/Ctx graph");
                fs::write(
                    dir.join(format!("{label}-{route}.analyzed-unverified.wire")),
                    zil_ir::wire::encode_module_candidate(&input, &candidate, &limits).unwrap(),
                )
                .unwrap();
            } else {
                let errors = actual.expect_err(
                    "public Static uppers and unused literal arguments cannot ground a data cycle",
                );
                assert!(
                    errors
                        .iter()
                        .any(|d| d.message.contains("actual literal grounding")
                            || d.message.contains("constant occurrence retains")),
                    "{errors:?}"
                );
            }
        }
    }
}

#[test]
fn owning_callable_context_and_static_combinations_publish_actual_untrusted_wire() {
    for (label, text) in [
        (
            "captured-owner-wire",
            include_str!("../../../tests/fixtures/dyn-context/captured-owner.zil"),
        ),
        (
            "captured-owner-mut-wire",
            include_str!("../../../tests/fixtures/dyn-context/captured-owner-mut.zil"),
        ),
        (
            "closed-owner-static-wire",
            include_str!("../../../tests/fixtures/dyn-context/closed-owner-static-callback.zil"),
        ),
    ] {
        let (_, module, _, _) = context_candidate(text, label);
        let definition = module
            .closures
            .first()
            .expect("real Source must create its owning environment");
        assert_eq!(
            definition
                .storage_life
                .as_ref()
                .expect("actual environment scope")
                .fixed_binder_count,
            1
        );
        assert!(
            module
                .functions
                .iter()
                .filter_map(|f| f.callable_return_contract.as_ref())
                .all(|c| c
                    .life
                    .as_ref()
                    .is_some_and(|life| life.fixed_binder_count == 1)),
            "ordinary F forwarding keeps complete public expected life"
        );
    }
}
