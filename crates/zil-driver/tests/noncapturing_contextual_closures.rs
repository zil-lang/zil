//! Contextual noncapturing closures become actual no-environment functions.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{
    CompileOptions, CompileReport, ProjectSources, Stage, compile_project, compile_source,
};
use zil_ir::{InstructionKind, TypedModule};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-noncapturing-contextual-closures-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "noncapturing_contextual_closures.zil",
        text,
    )
}
fn execute(report: &CompileReport, out: &Temp, expected: i64) -> String {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
    for optimized in [false, true] {
        let executable = if optimized {
            let p = out.0.join("optimized");
            let built = Command::new("clang")
                .args(["-x", "ir", "-O2"])
                .arg(&report.artifacts["llvm"])
                .arg("-o")
                .arg(&p)
                .output()
                .unwrap();
            assert!(
                built.status.success(),
                "{}",
                String::from_utf8_lossy(&built.stderr)
            );
            p
        } else {
            report.artifacts["executable"].clone()
        };
        let observed = Command::new(executable).output().unwrap();
        assert!(observed.status.success(), "{observed:?}");
        assert_eq!(observed.stdout, format!("{expected}\n").as_bytes());
        assert!(observed.stderr.is_empty());
    }
    llvm
}
fn native(text: &str, expected: i64) -> String {
    let out = Temp::new();
    let report = compile_source(
        &source(text),
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some("result".into()),
            output: Some(out.0.join("plain")),
            ..CompileOptions::default()
        },
    );
    execute(&report, &out, expected)
}
fn reject(text: &str, category: ResultCategory, code: &str) {
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(report.status, category, "{}", report.to_json());
    assert_eq!(report.diagnostics[0].code, code, "{}", report.to_json());
    assert!(!report.artifacts.contains_key("llvm"));
}
fn core(text: &str) -> zil_ir::CoreModule {
    let src = source(text);
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    zil_expand::expand(&src, &ast).unwrap()
}
fn typed(text: &str) -> TypedModule {
    zil_types::check(&core(text), &TargetProfile::default(), &Limits::default()).unwrap()
}

#[test]
fn cl13_runs_and_cl14_uses_the_required_capture_diagnostic() {
    let positive =
        include_str!("../../../conformance/cases/closure/CL13-empty-capture-function-pointer.zil");
    let llvm = native(&format!("{positive}\nfn result()->i64=>check()"), 42);
    assert!(!llvm.contains("ptr %env"));
    assert!(!llvm.contains("@malloc"));
    reject(
        include_str!(
            "../../../conformance/cases/closure/CL14-capturing-function-pointer-rejected.zil"
        ),
        ResultCategory::SafeReject,
        "E-CAPTURED-FN-POINTER",
    );
}

#[test]
fn function_return_argument_record_array_tuple_and_assignment_contexts_execute() {
    let llvm = native(
        include_str!("../../../examples/function-pointer-pipeline/main.zil"),
        5000000042,
    );
    assert!(llvm.contains("call i64 %v."));
    assert!(!llvm.contains("ptr %env"));
    assert!(!llvm.contains("@malloc"));
}

#[test]
fn record_update_and_projected_assignments_preserve_pointer_contexts() {
    native(
        "struct Callbacks{f:fn(i64)->i64;a:[fn(i64)->i64;1];}fn result()->i64{let x=Callbacks{f:n=>n+1,a:[n=>n+2]};var y=.{...x,f:n=>n+3};y.f=n=>n*2;y.a[0]=n=>n+7;return y.f(2500000000)+y.a[0](0)+x.f(1)+x.a[0](2)}",
        5000000013,
    );
}

#[test]
fn nested_thin_functions_preserve_unit_and_aggregate_abi() {
    native(
        "fn result()->i64{let consume:fn(())->()=x=>x;let select:fn()->fn(i64)->i64=()=>x=>x;let pair:fn((),i64,())->(i64,())=(a,n,b)=>(n,b);consume(());return pair((),select()(9223372036854775807),()).0}",
        i64::MAX,
    );
    native(
        "fn result()->i64{let sum:fn([i64;0],((),i64))->i64=(empty,pair)=>pair.1;return sum([],((),5000000000))}",
        5000000000,
    );
}

const MIXED: &str = "fn result()->i64{let n=3;let first=(x:i64)=>{let local:fn(i64)->i64=y=>y+1;return local(x)+n};let p:fn(i64)->i64=x=>x+2;let second=move(x:i64)=>x+n;let factory:fn()->fn(i64)->i64=()=>x=>x+3;return first(5000000000)+second(2)+p(4)+factory()(2)}";

#[test]
fn mixed_pending_and_thin_entries_have_dense_independent_id_spaces() {
    native(MIXED, 5000000020);
    let raw = typed(MIXED);
    assert_eq!(raw.closures.len(), 2);
    for (index, function) in raw.functions.iter().enumerate() {
        assert_eq!(function.id.0 as usize, index);
    }
    for (index, closure) in raw.closures.iter().enumerate() {
        assert_eq!(closure.id.0 as usize, index);
        assert_eq!(
            raw.functions[closure.function.0 as usize].environment,
            Some(closure.id)
        );
    }
    let mut references = 0;
    for function in &raw.functions {
        for instruction in function.blocks.iter().flat_map(|b| &b.instructions) {
            if let InstructionKind::FunctionRef { function } = instruction.kind {
                assert!(raw.functions[function.0 as usize].environment.is_none());
                references += 1;
            }
        }
    }
    assert_eq!(references, 4);
    let limits = Limits::default();
    let target = TargetProfile::default();
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    assert_eq!(
        zil_verify::verify_module_bytes(&wire, &target, &limits)
            .unwrap()
            .module(),
        &raw
    );
}

#[test]
fn exported_factory_retains_its_module_resolution_and_thin_identity() {
    let out = Temp::new();
    let project = ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "main.zil",
                "import ops;fn result()->i64=>ops.make()(5000000000)",
            ),
            SourceUnit::new(
                FileId(1),
                ModuleId(1),
                "ops.zil",
                "fn plus(x:i64)->i64=>x+41;pub fn make()->fn(i64)->i64=>x=>plus(x)",
            ),
        ],
        paths: std::collections::BTreeMap::from([(vec!["ops".into()], ModuleId(1))]),
    };
    let report = compile_project(
        &project,
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some("result".into()),
            output: Some(out.0.join("plain")),
            ..CompileOptions::default()
        },
    );
    execute(&report, &out, 5000000041);
}

#[test]
fn captures_and_incompatible_contexts_are_rejected_before_codegen() {
    for text in [
        "fn result()->i32{let n=3;let f:fn(i32)->i32=x=>x+n;return f(1)}",
        "fn result()->i32{let n=3;let f:fn(i32)->i32=move x=>x+n;return f(1)}",
        "fn result()->i32{var n=3;let f:fn(i32)->i32=move x=>{n=x;return n};return f(1)}",
        "fn result()->i32{let f:fn(i32)->fn()->i32=x=>()=>x;return 0}",
        "fn inc(x:i32)->i32=>x+1;fn result()->i32{let p=inc;let f:fn(i32)->i32=x=>p(x);return f(1)}",
    ] {
        reject(text, ResultCategory::SafeReject, "E-CAPTURED-FN-POINTER");
    }
    for (text, code) in [
        (
            "fn result()->i32{let f:fn(i32)->i32=()=>1;return 0}",
            "E-CALL-ARITY",
        ),
        (
            "fn result()->i32{let f:fn(i64)->i64=(x:i32)=>x;return 0}",
            "E-TYPE-MISMATCH",
        ),
        (
            "fn result()->i32{let f:fn(i32)->bool=x=>x;return 0}",
            "E-TYPE-MISMATCH",
        ),
    ] {
        reject(text, ResultCategory::SafeReject, code);
    }
    native("fn result()->i32{let f:fn(&i32)->i32=x=>*x;return 0}", 0);
    native(
        "fn result()->i32{let f:fn(&i32)->i32=x=>*x;let n=42;return f(&n)}",
        42,
    );
}

#[test]
fn converted_and_environment_functions_share_the_function_budget() {
    let input = core(
        "fn result()->i64{let n=3;let a=x=>x+n;let b:fn(i64)->i64=x=>x+1;let c=x=>x+n;let d:fn(i64)->i64=x=>x+2;return a(1)+b(2)+c(3)+d(4)}",
    );
    let target = TargetProfile::default();
    let exact = Limits {
        max_functions: 5,
        ..Limits::default()
    };
    let module = zil_types::check(&input, &target, &exact).unwrap();
    assert_eq!(module.functions.len(), 5);
    for limits in [
        Limits {
            max_functions: 4,
            ..Limits::default()
        },
        Limits {
            max_analysis_steps: 32,
            ..Limits::default()
        },
    ] {
        let errors = zil_types::check(&input, &target, &limits).unwrap_err();
        assert_eq!(errors[0].phase, Phase::Types);
        assert_eq!(
            errors[0].category,
            ResultCategory::ResourceLimit,
            "{errors:?}"
        );
    }
}

#[test]
fn independent_verify_still_rejects_environment_entry_as_function_ref() {
    let mut raw = typed(MIXED);
    let limits = Limits::default();
    let target = TargetProfile::default();
    let mut claims = zil_analyze::analyze(&raw, &limits).unwrap();
    let captured_entry = raw.closures[0].function;
    let reference = raw
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::FunctionRef { .. }))
        .unwrap();
    reference.kind = InstructionKind::FunctionRef {
        function: captured_entry,
    };
    claims.module_fingerprint = raw.fingerprint();
    let errors = zil_verify::verify_module(raw, claims, &target, &limits).unwrap_err();
    assert_eq!(errors[0].phase, Phase::Verify);
    assert!(errors[0].code.starts_with("E-VERIFY-"), "{errors:?}");
}

fn semantic_typed(unit: SourceUnit, target: &TargetProfile, limits: &Limits) -> TypedModule {
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let aliases = std::collections::BTreeMap::new();
    let context = zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
        root: unit.module_id,
        aliases: &aliases,
        declarations: zil_expand::source_context::DeclarationPlacement::Original,
    };
    let core = zil_expand::expand_program_with_semantics(
        &[unit],
        &[&ast],
        &[Vec::new()],
        limits,
        target,
        context,
    )
    .unwrap()
    .remove(0)
    .0
    .core;
    zil_types::check(&core, target, limits).unwrap()
}

#[test]
fn named_and_literal_function_addresses_keep_their_original_identity_on_raw_and_wire() {
    use zil_ir::*;
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let unit = source(
        "fn other(x:i32)->i32=>x-1\nfn main()->i32{let n=7;let named:fn(i32)->i32=other;let literal:fn(i32)->i32=x=>x+1;return literal(named(n))}",
    );
    let raw = semantic_typed(unit, &target, &limits);
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();

    let program = raw.semantic.as_ref().unwrap();
    let child = program
        .instances
        .iter()
        .find(|i| {
            program
                .definitions
                .iter()
                .any(|d| d.id == i.definition && d.kind == SemanticDefinitionKind::Closure)
        })
        .unwrap();
    let parent = program
        .instances
        .iter()
        .find(|i| Some(i.id) == child.parent)
        .unwrap();
    let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(owner)) = parent.target
    else {
        panic!()
    };
    let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(literal)) = child.target
    else {
        panic!()
    };
    let site = program.sites.iter().position(|s| {
        s.instance == parent.id && program.definitions.iter().find(|d| d.id == parent.definition).is_some_and(|d| {
            matches!(&d.body, SemanticDefinitionBody::FunctionWithBody { sites, .. } if sites.iter().any(|r| r.id == s.recipe && matches!(r.kind, SemanticSiteRecipeKind::Closure { definition, .. } if definition == child.definition)))
        })
    }).unwrap();
    let uses = &program.sites[site];
    let SemanticSiteAddress::Typed {
        block,
        instruction,
        edge: SemanticSiteEdge::Before,
    } = uses.at
    else {
        panic!()
    };
    let refs = &raw.functions[owner.0 as usize].blocks[block.0 as usize].instructions;
    assert!(
        matches!(refs[instruction as usize-1].kind, InstructionKind::FunctionRef{function} if function == literal)
    );
    let (named_at, named, named_value) = refs
        .iter()
        .enumerate()
        .find_map(|(i, op)| match op.kind {
            InstructionKind::FunctionRef { function } if function != literal => {
                Some((i, function, op.result.unwrap()))
            }
            _ => None,
        })
        .unwrap();

    fn introduce_capture(body: &mut SemanticOriginalBlock) {
        let CoreStatementKind::Return(Some(value)) = &mut body.statements[0].kind else {
            panic!()
        };
        let CoreExprKind::Binary { left, .. } = &mut value.kind else {
            panic!()
        };
        assert!(matches!(&left.kind, CoreExprKind::Name(name) if name == "x"));
        left.kind = CoreExprKind::Name("n".into());
    }
    for mutation in [
        "wrong child",
        "wrong named target",
        "SSA",
        "position",
        "node",
        "claim",
        "parent",
        "capture",
        "duplicate child",
    ] {
        let mut changed = raw.clone();
        let encoded = changed.semantic.as_mut().unwrap();
        let ops = &mut changed.functions[owner.0 as usize].blocks[block.0 as usize].instructions;
        match mutation {
            "wrong child" => {
                ops[instruction as usize - 1].kind =
                    InstructionKind::FunctionRef { function: named }
            }
            "wrong named target" => {
                ops[named_at].kind = InstructionKind::FunctionRef { function: literal }
            }
            "SSA" => {
                let SemanticSiteAction::TypeUse { subject, .. } = &mut encoded.sites[site].action
                else {
                    panic!()
                };
                *subject = SemanticOperand::Value(named_value);
            }
            "position" => {
                let SemanticSiteAddress::Typed { instruction, .. } = &mut encoded.sites[site].at
                else {
                    panic!()
                };
                *instruction -= 1;
            }
            "node" => {
                let owner = encoded
                    .definitions
                    .iter_mut()
                    .find(|d| d.id == parent.definition)
                    .unwrap();
                let SemanticDefinitionBody::FunctionWithBody {
                    site_nodes,
                    body: SemanticOriginalFunctionBody::Defined(body),
                    ..
                } = &mut owner.body
                else {
                    panic!()
                };
                let CoreStatementKind::Binding {
                    initializer: Some(named),
                    ..
                } = &body.statements[1].kind
                else {
                    panic!()
                };
                let link = site_nodes
                    .iter_mut()
                    .find(|r| r.site == uses.recipe)
                    .unwrap();
                link.node = SemanticOriginalNode::Expression(named.id);
            }
            "claim" => {
                let SemanticSiteAction::TypeUse {
                    expected: SemanticTypeValue::Resolved { ty, .. },
                    ..
                } = &mut encoded.sites[site].action
                else {
                    panic!()
                };
                *ty = Type::FunctionPointer {
                    parameters: vec![Type::Bool],
                    result: Box::new(Type::I32),
                };
            }
            "parent" => {
                encoded
                    .instances
                    .iter_mut()
                    .find(|i| i.id == child.id)
                    .unwrap()
                    .parent = None
            }
            "duplicate child" => {
                let mut duplicate = child.clone();
                duplicate.id.index =
                    encoded.instances.iter().map(|i| i.id.index).max().unwrap() + 1;
                encoded.instances.push(duplicate);
            }
            "capture" => {
                // Change both original copies so structural source correspondence still succeeds.
                for definition in &mut encoded.definitions {
                    let SemanticDefinitionBody::FunctionWithBody {
                        body: SemanticOriginalFunctionBody::Defined(body),
                        ..
                    } = &mut definition.body
                    else {
                        continue;
                    };
                    if definition.id == child.definition {
                        introduce_capture(body);
                    }
                    if definition.id == parent.definition {
                        let literal = body
                            .statements
                            .iter_mut()
                            .find_map(|s| match &mut s.kind {
                                CoreStatementKind::Binding {
                                    initializer: Some(expr),
                                    ..
                                } => match &mut expr.kind {
                                    CoreExprKind::Closure { body, .. } => Some(body),
                                    _ => None,
                                },
                                _ => None,
                            })
                            .unwrap();
                        introduce_capture(literal);
                    }
                }
            }
            _ => unreachable!(),
        }
        let mut claims = plan.clone();
        claims.module_fingerprint = changed.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&changed, &claims, &limits).unwrap();
        for errors in [
            zil_verify::verify_module(changed, claims, &target, &limits).unwrap_err(),
            zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err(),
        ] {
            assert!(
                errors.iter().any(|e| e.phase == Phase::Verify
                    && e.category == ResultCategory::SafeReject
                    && e.code == "E-VERIFY-SEMANTIC"),
                "{mutation}: {errors:?}"
            );
        }
    }
}

#[test]
fn borrowed_code_keeps_its_invocation_relation_on_named_and_literal_addresses() {
    use zil_ir::*;
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let raw = semantic_typed(
        source(
            "fn read(x:&i32)->i32=>*x\nfn main()->i32{let a:fn(&i32)->i32=read;let b:fn(&i32)->i32=x=>*x;let n=21;return a(&n)+b(&n)-42}",
        ),
        &target,
        &limits,
    );
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    let program = raw.semantic.as_ref().unwrap();
    let sites = program
        .sites
        .iter()
        .enumerate()
        .filter_map(|(i, site)| {
            let SemanticSiteAction::TypeUse {
                subject: SemanticOperand::Value(id),
                expected:
                    SemanticTypeValue::Resolved {
                        ty: Type::BorrowedFunctionPointer { .. },
                        ..
                    },
            } = &site.action
            else {
                return None;
            };
            let owner = program
                .instances
                .iter()
                .find(|instance| instance.id == site.instance)?;
            let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(f)) = owner.target
            else {
                return None;
            };
            let SemanticSiteAddress::Typed {
                block,
                instruction,
                edge: SemanticSiteEdge::Before,
            } = site.at
            else {
                return None;
            };
            let source = raw.functions[f.0 as usize].blocks[block.0 as usize]
                .instructions
                .get(instruction.checked_sub(1)? as usize)?;
            (source.result == Some(*id)
                && matches!(source.kind, InstructionKind::FunctionRef { .. }))
            .then_some(i)
        })
        .collect::<Vec<_>>();
    assert_eq!(sites.len(), 2);
    for (i, erase) in sites.into_iter().flat_map(|i| [(i, true), (i, false)]) {
        let mut changed = raw.clone();
        let SemanticSiteAction::TypeUse {
            expected: SemanticTypeValue::Resolved { ty, .. },
            ..
        } = &mut changed.semantic.as_mut().unwrap().sites[i].action
        else {
            panic!()
        };
        let Type::BorrowedFunctionPointer {
            parameters, result, ..
        } = ty
        else {
            panic!()
        };
        let expected_code = if erase {
            *ty = Type::FunctionPointer {
                parameters: parameters.clone(),
                result: result.clone(),
            };
            "E-VERIFY-DYN-WITNESS"
        } else {
            **result = Type::Bool;
            "E-VERIFY-SEMANTIC"
        };
        let mut stale = plan.clone();
        stale.module_fingerprint = changed.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&changed, &stale, &limits).unwrap();
        for errors in [
            zil_verify::verify_module(changed, stale, &target, &limits).unwrap_err(),
            zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err(),
        ] {
            assert!(
                errors.iter().any(|e| e.phase == Phase::Verify
                    && e.category == ResultCategory::SafeReject
                    && e.code == expected_code),
                "{errors:?}"
            );
        }
    }
}

#[test]
fn nominal_thin_inputs_and_results_keep_their_declaration_on_both_routes() {
    use zil_ir::*;
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let raw = semantic_typed(source(
        "struct Sample {value:i64;}\nfn keep(s:Sample)->Sample=>s\nfn main()->i32{\n\
         let named:fn(Sample)->Sample=keep\nlet inline:fn(Sample)->Sample=s=>s\n\
         let first=named(Sample{value:42})\nlet second=inline(first)\nreturn 0\n}",
    ), &target, &limits);
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();

    for replace_input in [true, false] {
        let mut changed = raw.clone();
        let record = changed.semantic.as_mut().unwrap().sites.iter_mut().find_map(|site| {
            match &mut site.action {
                SemanticSiteAction::TypeUse { expected: SemanticTypeValue::Resolved {
                    ty: Type::FunctionPointer { parameters, result }, ..
                }, .. } if matches!(result.as_ref(), Type::Record { .. }) => {
                    Some(if replace_input { &mut parameters[0] } else { result.as_mut() })
                }
                _ => None,
            }
        }).unwrap();
        let Type::Record { name, .. } = record else { panic!("missing nominal ABI value") };
        *name = "same_fields_different_declaration".into();
        let mut stale = plan.clone();
        stale.module_fingerprint = changed.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&changed, &stale, &limits).unwrap();
        for errors in [
            zil_verify::verify_module(changed, stale, &target, &limits).unwrap_err(),
            zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err(),
        ] {
            assert!(errors.iter().any(|e| e.phase == Phase::Verify
                && e.category == ResultCategory::SafeReject && e.code == "E-VERIFY-SEMANTIC"), "{errors:?}");
        }
    }
}
