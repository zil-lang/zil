//! Independent source and hostile raw/Wire probes for mandatory defer calls.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{Diagnostic, FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
use zil_ir::{
    CandidatePlan, Instruction, InstructionKind as I, LocalDecl, LocalId, Type, TypedModule,
    ValueId,
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
const HEAD: &str = r#"extern "C" fn putchar(code:i32)->i32;
fn emit(code:i32){unsafe{putchar(code)}}
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){emit(.code)}}"#;
fn dir(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let out = root.join(format!(
        "review227-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&out).unwrap();
    out
}
fn unit(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "review227.zil", text)
}
fn typed(text: &str) -> TypedModule {
    let u = unit(text);
    let ast = zil_syntax::parse(&u).into_result().unwrap();
    let core = zil_expand::expand(&u, &ast).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}
fn rejected(result: Result<impl Sized, Vec<Diagnostic>>, label: &str) {
    let errors = match result {
        Ok(_) => panic!("{label}: accepted invalid cleanup"),
        Err(e) => e,
    };
    assert!(
        errors.iter().any(|d| matches!(
            d.category,
            ResultCategory::SafeReject | ResultCategory::Unsupported
        )),
        "{label}: {errors:#?}"
    );
    assert!(
        errors.iter().all(|d| !d.code.starts_with("E-VERIFY-PLAN")),
        "{label}: rejected only candidate claims: {errors:#?}"
    );
}
fn raw_wire(label: &str, module: TypedModule, mut plan: CandidatePlan, accept: bool) {
    let out = dir(label);
    let limits = Limits::default();
    plan.module_fingerprint = module.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    fs::write(out.join("input.wire"), &wire).unwrap();
    fs::write(out.join("module.txt"), format!("{module:#?}")).unwrap();
    let attempts = [
        (
            "raw",
            zil_verify::verify_module(module.clone(), plan, &module.target, &limits),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&wire, &module.target, &limits),
        ),
    ];
    for (route, result) in &attempts {
        fs::write(out.join(format!("{route}.txt")), format!("{result:#?}")).unwrap();
    }
    for (route, result) in attempts {
        if accept {
            result.unwrap_or_else(|e| panic!("{label}/{route}: {e:#?}"));
        } else {
            rejected(result, &format!("{label}/{route}"));
        }
    }
}
fn verified(text: &str) -> (TypedModule, CandidatePlan) {
    let m = typed(text);
    let p = zil_analyze::analyze(&m, &Limits::default()).unwrap();
    raw_wire("control", m.clone(), p.clone(), true);
    (m, p)
}
fn source(label: &str, text: &str, accept: bool) {
    let out = dir(label);
    fs::write(out.join("source.zil"), text).unwrap();
    let r = compile_source(
        &unit(text),
        &CompileOptions {
            stop_after: Stage::Verify,
            emit_dir: Some(out.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(out.join("report.json"), r.to_json()).unwrap();
    if accept {
        assert_eq!(
            r.status,
            ResultCategory::Success,
            "{label}: {}",
            r.to_json()
        );
    } else {
        assert!(
            matches!(
                r.status,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            ),
            "{label}: {}",
            r.to_json()
        );
    }
    assert!(!r.artifacts.contains_key("executable"));
}
fn native(label: &str, text: &str, expected: &str) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let out = dir(label);
        fs::write(out.join("source.zil"), text).unwrap();
        let backend = if cfg!(feature = "llvm-api") {
            NativeBackend::LlvmApi
        } else {
            NativeBackend::Clang
        };
        let r = compile_source(
            &unit(text),
            &CompileOptions {
                stop_after: Stage::Build,
                backend,
                optimization,
                output: Some(out.join("program")),
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), r.to_json()).unwrap();
        assert_eq!(
            r.status,
            ResultCategory::Success,
            "{label}: {}",
            r.to_json()
        );
        let summary = r.native_codegen.as_ref().unwrap();
        assert_eq!(summary.backend, backend);
        assert!(summary.fallback.is_none());
        let result = Command::new(&r.artifacts["executable"]).output().unwrap();
        fs::write(out.join("stdout"), &result.stdout).unwrap();
        fs::write(out.join("stderr"), &result.stderr).unwrap();
        assert_eq!(result.status.code(), Some(0), "{label}: {result:?}");
        assert_eq!(result.stdout, expected.as_bytes(), "{label}: {result:?}");
        assert!(result.stderr.is_empty());
    }
}

#[test]
fn pending_outer_action_crosses_internal_loops_and_inner_actions_close_first() {
    let text = format!(
        "{HEAD}fn main()->i32{{defer{{emit(90)}}for i in 0..3{{if i==1{{continue}}defer{{emit(65+i)}}for j in 0..2{{if j==1{{break}}emit(48+j)}}}}return 0}}"
    );
    verified(&text);
    native("nested-loops", &text, "0A0CZ");
}

#[test]
fn owned_resource_and_callback_returns_are_prepared_before_cleanup() {
    let text = format!(
        "{HEAD}fn make_ticket()->Ticket contract{{result(create)}}{{defer{{emit(68)}}return Ticket{{code:65}}}}fn callback()->impl Fn()->i32{{let token=Ticket{{code:66}};defer{{emit(69)}}return move()=>token.code}}fn main()->i32{{let token=make_ticket();let f=callback();if f()!=66{{return 1}}return 0}}"
    );
    verified(&text);
    native("returned-carriers", &text, "DEBA");
}

#[test]
fn deferred_capture_mutation_survives_outer_callback_writeback() {
    let text = format!(
        "{HEAD}fn main()->i32{{var count=0;var callback=move()=>{{if true{{defer{{count+=1}}}}return count}};if callback()!=1{{return 1}}if callback()!=2{{return 2}}if callback()!=3{{return 3}}return 0}}"
    );
    verified(&text);
    native("capture-writeback", &text, "");
}

#[test]
fn borrowing_a_callback_does_not_erase_the_deferred_future_resource_use() {
    source(
        "nested-callback-live",
        &format!(
            "{HEAD}fn main()->i32{{let token=Ticket{{code:65}};let read=()=>token.code;defer{{emit(read())}}let consumed=give token;return 0}}"
        ),
        false,
    );
    source(
        "copy-shared-live",
        &format!("{HEAD}fn main()->i32{{var number=65;defer{{emit(number)}}number=66;return 0}}"),
        false,
    );
}

#[test]
fn empty_actions_still_require_the_actual_adjacent_call_at_scope_exit() {
    let (base, plan) = verified("fn main(){defer{}}");
    for mode in ["drop", "missing-call", "wrong-scope", "ordinary-loan"] {
        let mut m = base.clone();
        let f = m.functions.iter_mut().find(|f| f.name == "main").unwrap();
        let local = f
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .find_map(|i| {
                if let I::RegisterDefer { local, .. } = i.kind {
                    Some(local)
                } else {
                    None
                }
            })
            .unwrap();
        let block = f
            .blocks
            .iter_mut()
            .find(|b| {
                b.instructions
                    .iter()
                    .any(|i| matches!(i.kind,I::TakeLocal{local:l}if l==local))
            })
            .unwrap();
        let take = block
            .instructions
            .iter()
            .position(|i| matches!(i.kind,I::TakeLocal{local:l}if l==local))
            .unwrap();
        match mode {
            "drop" => {
                let span = block.instructions[take].span;
                assert_eq!(
                    block.instructions[take].result.unwrap().0 as usize,
                    f.values.len() - 1
                );
                f.values.pop();
                block.instructions.splice(
                    take..take + 2,
                    [Instruction {
                        result: None,
                        kind: I::DropClosure { local },
                        span,
                    }],
                );
            }
            "missing-call" => {
                block.instructions.remove(take + 1);
            }
            "wrong-scope" => {
                for i in &mut block.instructions {
                    if let I::RegisterDefer { scope, .. } = &mut i.kind {
                        scope.0 += 99
                    }
                }
            }
            "ordinary-loan" => {
                let span = block.instructions[take].span;
                let result = ValueId(f.values.len() as u32);
                f.values.push(Type::Address {
                    mutable: false,
                    inner: Box::new(f.locals[local.0 as usize].ty.clone()),
                });
                block.instructions.insert(
                    take,
                    Instruction {
                        result: Some(result),
                        kind: I::BorrowLocal { local },
                        span,
                    },
                );
            }
            _ => unreachable!(),
        }
        rejected(zil_analyze::analyze(&m, &Limits::default()), mode);
        raw_wire(mode, m, plan.clone(), false);
    }
}

#[test]
fn an_early_actual_call_cannot_resume_the_ordinary_body() {
    let (mut m, plan) = verified(&format!("{HEAD}fn main(){{defer{{}}emit(65)}}"));
    let f = m.functions.iter_mut().find(|f| f.name == "main").unwrap();
    let b = &mut f.blocks[0];
    let register = b
        .instructions
        .iter()
        .position(|i| matches!(i.kind, I::RegisterDefer { .. }))
        .unwrap();
    let take = b
        .instructions
        .iter()
        .position(|i| matches!(i.kind, I::TakeLocal { .. }))
        .unwrap();
    let pair: Vec<_> = b.instructions.drain(take..take + 2).collect();
    b.instructions.splice(register + 1..register + 1, pair);
    rejected(zil_analyze::analyze(&m, &Limits::default()), "early-call");
    raw_wire("early-call", m, plan, false);
}

// Transfer a real borrowed environment through an ordinary FnOnce carrier
// before it reaches the registered slot. Neither transfer can erase its loan.
fn ordinary_carrier(mut module: TypedModule) -> TypedModule {
    let f = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let block = f
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, I::RegisterDefer { .. }))
        })
        .unwrap();
    let register = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, I::RegisterDefer { .. }))
        .unwrap();
    let I::RegisterDefer {
        local: action,
        scope,
    } = block.instructions[register].kind
    else {
        unreachable!()
    };
    let I::Store {
        value: environment, ..
    } = block.instructions[register - 1].kind
    else {
        unreachable!()
    };
    let slot = LocalId(f.locals.len() as u32);
    let ty = f.locals[action.0 as usize].ty.clone();
    let span = block.instructions[register].span;
    f.locals.push(LocalDecl {
        id: slot,
        name: "independent-ordinary-carrier".into(),
        ty: ty.clone(),
        mutable: false,
        span,
    });
    let taken = ValueId(f.values.len() as u32);
    f.values.push(ty);
    block.instructions.splice(
        register - 1..register - 1,
        [
            Instruction {
                result: None,
                kind: I::StorageLive(slot),
                span,
            },
            Instruction {
                result: None,
                kind: I::Store {
                    local: slot,
                    value: environment,
                },
                span,
            },
            Instruction {
                result: Some(taken),
                kind: I::TakeLocal { local: slot },
                span,
            },
        ],
    );
    let I::Store { value, .. } = &mut block.instructions[register + 2].kind else {
        unreachable!()
    };
    *value = taken;
    let close = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind,I::ScopeExit{scope:owner,..}if owner==scope))
        .unwrap();
    block.instructions.insert(
        close,
        Instruction {
            result: None,
            kind: I::StorageDead(slot),
            span,
        },
    );
    module
}

#[test]
fn ordinary_once_transfer_cannot_launder_registered_action_resource_sources() {
    let text = format!(
        "{HEAD}fn main(){{let token=Ticket{{code:65}};let spare=Ticket{{code:66}};defer{{emit(token.code)}}let moved=give spare;}}"
    );
    let base = ordinary_carrier(typed(&text));
    let plan = zil_analyze::analyze(&base, &Limits::default()).unwrap();
    raw_wire("ordinary-carrier-control", base.clone(), plan.clone(), true);
    let mut attacked = base;
    let f = attacked
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let token = f.locals.iter().find(|l| l.name == "token").unwrap().id;
    let spare = f.locals.iter().find(|l| l.name == "spare").unwrap().id;
    let take = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind,I::TakeLocal{local}if local==spare))
        .unwrap();
    take.kind = I::TakeLocal { local: token };
    rejected(
        zil_analyze::analyze(&attacked, &Limits::default()),
        "laundered-source",
    );
    raw_wire("laundered-source", attacked, plan, false);
}

#[test]
fn ordinary_once_transfer_preserves_shared_address_holds_until_actual_invocation() {
    let text = format!(
        "{HEAD}fn main(){{var left=65;var right=66;let view=&left;defer{{emit(*view)}}right=67;}}"
    );
    let module = ordinary_carrier(typed(&text));
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    raw_wire(
        "ordinary-address-control",
        module.clone(),
        plan.clone(),
        true,
    );
    let mut attacked = module;
    let f = attacked
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let left = f.locals.iter().find(|l| l.name == "left").unwrap().id;
    let right = f.locals.iter().find(|l| l.name == "right").unwrap().id;
    let store = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .filter(|i| matches!(i.kind,I::Store{local,..}if local==right))
        .last()
        .unwrap();
    let I::Store { value, .. } = store.kind else {
        unreachable!()
    };
    store.kind = I::Store { local: left, value };
    // The producer exports Hold roots; ordinary shared-write exclusion remains
    // a Verify obligation, as on the pre-existing direct shared-address path.
    let action = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            if let I::RegisterDefer { local, .. } = i.kind {
                Some(local)
            } else {
                None
            }
        })
        .unwrap();
    let callee = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            if matches!(i.kind,I::TakeLocal{local}if local==action) {
                i.result
            } else {
                None
            }
        })
        .unwrap();
    let function = f.id;
    let analyzed = zil_analyze::analyze(&attacked, &Limits::default()).unwrap();
    assert!(
        analyzed
            .functions
            .iter()
            .find(|p| p.function == function)
            .unwrap()
            .holds
            .iter()
            .find(|h| h.value == callee)
            .unwrap()
            .roots
            .contains(&zil_ir::HoldRoot::Local(left))
    );
    let out = dir("laundered-address-analysis");
    fs::write(out.join("analyze.txt"), format!("{analyzed:#?}")).unwrap();
    raw_wire("laundered-address", attacked, plan, false);
}
