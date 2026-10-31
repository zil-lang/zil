//! Real source admission and nearest boundaries for partial local packages.
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, ProjectSources, Stage, compile_project, compile_source};
use zil_ir::{CallableKind, InstructionKind, Type, TypedModule};

#[path = "impl232_fixture_support/mod.rs"]
mod fixture232;

const DEFINITIONS: &str = "resource R{n:i32} package P{manages a:R,manages b:R,n:i32} fn make()->P{return P{a:R{n:20},b:R{n:21},n:1}} fn relay(p:P)->P{return p} fn consume(give r:R)->i32{return r.n} fn keep(give r:R,n:i32)->R contract{result(give)}{return give r} fn step(ok:bool)->bool!i32{if ok{return 0}return true}";

fn checked(body: &str) {
    let text = format!("{DEFINITIONS} {body}");
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "packages072.zil", text),
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
    assert!(report.verification_runs > 0);
}

fn rejected(body: &str, phase: Phase, code: &str) {
    let text = format!("{DEFINITIONS} {body}");
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "packages072-negative.zil", text),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "unexpected success: {body}"
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.phase == phase && d.code == code),
        "{body}\n{}",
        report.to_json()
    );
}

fn typed(body: &str) -> TypedModule {
    let unit = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "packages072-ir.zil",
        format!("{DEFINITIONS} {body}"),
    );
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap()
}

#[test]
fn packages072_immutable_field_take_preserves_sibling_and_copy_fields() {
    checked(
        "fn main()->i32{let p=make();let a=give p.a;let n=p.b.n+p.n;return consume(give a)+n-42}",
    );
    checked(
        "fn main()->i32{let p=make();let a=give p.a;let b=give p.b;return consume(give a)+consume(give b)+p.n-42}",
    );
    checked(
        "fn take(p:P)->i32{return consume(give p.a)+p.b.n+p.n} fn main()->i32{return take(make())-42}",
    );
}

#[test]
fn packages072_refill_restores_whole_transfer_and_returns() {
    checked(
        "fn update(p:P)->P{var q=p;let a=give q.a;q.n=2;q.a=R{n:a.n+1};return q} fn main()->i32{let p=relay(update(make()));return p.a.n+p.b.n+p.n-44}",
    );
    checked(
        "fn main()->i32{var p=make();let a=give p.a;let b=give p.b;p.a=give b;p.b=give a;let q=relay(p);return q.a.n+q.b.n+q.n-42}",
    );
}

#[test]
fn packages072_active_replacement_and_rhs_self_take() {
    checked("fn main()->i32{var p=make();p.a=R{n:p.a.n+1};return p.a.n+p.b.n-42}");
    checked("fn main()->i32{var p=make();p.a=keep(give p.a,0);return p.a.n+p.b.n+p.n-42}");
    checked("fn main()->i32{var p=make();p.a=give p.a;return p.a.n+p.b.n+p.n-42}");
}

#[test]
fn packages072_conditional_empty_slot_can_be_refilled_after_join() {
    checked(
        "fn update(yes:bool)->i32{var p=make();if yes{let a=give p.a}p.a=R{n:21};let q=relay(p);return q.a.n+q.b.n-42} fn main()->i32{return update(true)+update(false)}",
    );
    checked(
        "fn update(yes:bool)->i32{let p=make();if yes{let a=give p.a}return p.b.n-21} fn main()->i32{return update(true)+update(false)}",
    );
}

#[test]
fn packages072_loop_take_refill_reestablishes_whole_shape() {
    checked(
        "fn main()->i32{var p=make();var i=0;while i<3{let a=give p.a;p.a=R{n:a.n+1};i+=1}let q=relay(p);return q.a.n+q.b.n-44}",
    );
    checked("fn main()->i32{var p=make();for i in 0..3{p.a=R{n:20+i}}return p.a.n-22}");
}

#[test]
fn packages072_live_sibling_descriptor_and_finished_old_descriptor_are_allowed() {
    checked("fn main()->i32{var p=make();let b=p.b;p.a=R{n:21};return b.n+p.a.n-42}");
    checked("fn main()->i32{var p=make();let a=p.a;let n=a.n;p.a=R{n:22};return n+p.a.n-42}");
    checked("fn main()->i32{var p=make();let b=p.b;let f=()=>b.n;p.a=R{n:21};return f()+p.a.n-42}");
}

#[test]
fn packages072_replacement_preparation_can_fail_before_or_after_self_take() {
    checked(
        "fn run(ok:bool)->bool!i32{var p=make();p.a=R{n:21+step(ok)?};return p.a.n+p.b.n-42} fn main()->bool!i32{return run(false)?}",
    );
    checked(
        "fn run(ok:bool)->bool!i32{var p=make();p.a=keep(give p.a,step(ok)?);return p.a.n+p.b.n+p.n-42} fn main()->bool!i32{return run(false)?}",
    );
    checked(
        "fn combine(give a:R,give b:R,n:i32)->R contract{result(give)}{return give a} fn run(ok:bool)->bool!i32{var p=make();p.a=combine(R{n:20},give p.a,step(ok)?);return p.a.n+p.b.n+p.n-42} fn main()->bool!i32{return run(false)?}",
    );
}

#[test]
fn packages072_closure_body_owns_its_own_local_package() {
    checked(
        "fn main()->i32{let f=()=>{var p=make();let a=give p.a;p.a=R{n:a.n+1};return p.a.n+p.b.n-42};return f()}",
    );
}

#[test]
fn packages072_immutable_shell_resource_payload_writes_match_inside_closures() {
    checked("fn main()->i32{let p=make();p.a.n+=1;return p.a.n+p.b.n-42}");
    checked("fn main()->i32{let f=()=>{let p=make();p.a.n+=1;return p.a.n+p.b.n-42};return f()}");
    checked(
        "fn main()->i32{let f=()=>{let p=make();let moved=give p.a;p.b.n=22;return consume(give moved)+p.b.n-42};return f()}",
    );
    checked("fn main()->i32{let f=()=>0;let p=make();p.a.n=21;return p.a.n+p.b.n+f()-42}");
    for body in [
        "fn main()->i32{let f=()=>{let p=make();p.a=R{n:21};return 0};return f()}",
        "fn main()->i32{let f=()=>{let p=make();p.n=2;return 0};return f()}",
    ] {
        rejected(body, Phase::Types, "E-IMMUTABLE-ASSIGNMENT");
    }
}

#[test]
fn packages072_prepared_owner_precedes_explicit_old_cleanup_and_commit() {
    let module = typed("fn main()->i32{var p=make();p.a=R{n:21};return p.a.n+p.b.n-42}");
    zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let main = module.functions.iter().find(|f| f.name == "main").unwrap();
    let block = main
        .blocks
        .iter()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, InstructionKind::DropPackageField { .. }))
        })
        .unwrap();
    let position = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::DropPackageField { .. }))
        .unwrap();
    let InstructionKind::Store {
        local: prepared, ..
    } = block.instructions[position - 1].kind
    else {
        panic!("RHS has no real prepared owner")
    };
    assert!(matches!(
        main.locals[prepared.0 as usize].ty,
        Type::Managed(_)
    ));
    assert!(
        matches!(block.instructions[position + 1].kind, InstructionKind::TakeLocal { local } if local == prepared)
    );
    assert!(matches!(
        block.instructions[position + 2].kind,
        InstructionKind::StorePackageField { .. }
    ));
}

#[test]
fn packages072_partial_whole_boundary_and_repeated_take_are_rejected() {
    for body in [
        "fn main()->i32{let p=make();let a=give p.a;let again=give p.a;return 0}",
        "fn main()->i32{let p=make();let a=give p.a;let q=p;return 0}",
        "fn main()->i32{let p=make();let a=give p.a;let q=relay(p);return 0}",
        "fn broken()->P{let p=make();let a=give p.a;return p} fn main()->i32{return 0}",
        "fn main()->i32{let p=make();let a=give p.a;return p.a.n}",
        "fn create_bad(p:P)->R contract{result(create)}{return give p.a} fn main()->i32{return 0}",
    ] {
        rejected(body, Phase::Analyze, "E-ANALYZE-IR");
    }
}

#[test]
fn packages072_maybe_missing_field_and_loop_missing_refill_are_rejected() {
    for body in [
        "fn broken(yes:bool)->i32{let p=make();if yes{let a=give p.a}return p.a.n} fn main()->i32{return broken(true)}",
        "fn broken(yes:bool)->P{let p=make();if yes{let a=give p.a}return p} fn main()->i32{return 0}",
        "fn main()->i32{var p=make();var i=0;while i<2{let a=give p.a;i+=1}return p.n}",
    ] {
        rejected(body, Phase::Analyze, "E-ANALYZE-IR");
    }
}

#[test]
fn packages072_live_old_descriptors_aliases_and_captures_block_invalidation() {
    for body in [
        "fn main()->i32{var p=make();let a=p.a;p.a=R{n:22};return a.n}",
        "fn main()->i32{var p=make();let a=p.a;let alias=a;p.a=R{n:22};return alias.n}",
        "fn main()->i32{var p=make();let a=p.a;let f=()=>a.n;p.a=R{n:22};return f()}",
        "fn main()->i32{let p=make();let a=p.a;let moved=give p.a;return a.n}",
        "fn main()->i32{var p=make();let a=p.a;var i=0;while i<2{let n=a.n;p.a=R{n:n+1};i+=1}return 0}",
    ] {
        rejected(body, Phase::Analyze, "E-ANALYZE-IR");
    }
}

#[test]
fn packages072_rhs_moving_shell_cannot_commit_into_it() {
    rejected(
        "fn extract(p:P)->R contract{result(give)}{return give p.a} fn main()->i32{var p=make();p.a=extract(p);return 0}",
        Phase::Analyze,
        "E-UNINITIALIZED",
    );
}

#[test]
fn packages072_mutability_exact_resource_type_and_ownership_are_preserved() {
    for body in [
        "fn main()->i32{let p=make();p.a=R{n:1};return 0}",
        "fn main()->i32{let p=make();let a=give p.a;p.a=give a;return 0}",
        "fn main()->i32{let p=make();p.n=2;return 0}",
    ] {
        rejected(body, Phase::Types, "E-IMMUTABLE-ASSIGNMENT");
    }
    rejected(
        "fn main()->i32{let p=make();let n=give p.n;return 0}",
        Phase::Types,
        "E-GIVE-TYPE",
    );
    rejected(
        "fn main()->i32{var p=make();let a=p.a;p.a=a;return 0}",
        Phase::Types,
        "E-TYPE-MISMATCH",
    );
    rejected(
        "resource S{n:i32} fn main()->i32{var p=make();p.a=S{n:1};return 0}",
        Phase::Types,
        "E-TYPE-MISMATCH",
    );
}

#[test]
fn packages072_default_managed_take_captures_the_complete_package_once() {
    // Original rejection input, including its return value, is unchanged.
    let body = "fn main()->i32{var p=make();let f=()=>consume(give p.a);return f()}";
    let source = |body: &str| {
        SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "packages072-negative.zil",
            format!("{DEFINITIONS} {body}"),
        )
    };
    let module = typed(body);
    assert_eq!(module.closures.len(), 1);
    assert_eq!(module.closures[0].kind, CallableKind::FnOnce);
    let main = module.functions.iter().find(|f| f.name == "main").unwrap();
    let owner = main.locals.iter().find(|l| l.name == "p").unwrap();
    assert!(matches!(owner.ty, Type::Package { .. }));
    assert_eq!(module.closures[0].captures, vec![owner.ty.clone()]);
    let (block, at, capture) = main
        .blocks
        .iter()
        .find_map(|block| {
            block.instructions.iter().enumerate().find_map(|(at, i)| {
                if let InstructionKind::MakeClosure { captures, .. } = &i.kind {
                    assert_eq!(captures.len(), 1);
                    Some((block, at, captures[0]))
                } else {
                    None
                }
            })
        })
        .unwrap();
    assert!(block.instructions[..at].iter().any(|i| {
        i.result == Some(capture)
            && matches!(i.kind, InstructionKind::TakeLocal { local } if local == owner.id)
    }));
    fixture232::verified("072-owned-original", &source(body), &module);
    fixture232::native("072-owned-original", &source(body), 20);
    for (label, replacement) in [
        ("072-owner-after-creation", "return p.n+f()"),
        ("072-second-call", "let first=f();return f()+first"),
    ] {
        let invalid = body.replace("return f()", replacement);
        assert_ne!(invalid, body);
        fixture232::rejected(
            label,
            &source(&invalid),
            ResultCategory::SafeReject,
            Phase::Analyze,
            "E-UNINITIALIZED",
        );
    }
}

#[test]
fn packages072_raw_field_store_cannot_skip_old_cleanup_or_change_permissions() {
    let valid = typed("fn main()->i32{var p=make();p.a=R{n:21};return p.a.n+p.b.n-42}");
    zil_analyze::analyze(&valid, &Limits::default()).unwrap();
    for mutation in 0..6 {
        let mut module = valid.clone();
        let main = module
            .functions
            .iter_mut()
            .find(|f| f.name == "main")
            .unwrap();
        let block = main
            .blocks
            .iter_mut()
            .find(|b| {
                b.instructions
                    .iter()
                    .any(|i| matches!(i.kind, InstructionKind::StorePackageField { .. }))
            })
            .unwrap();
        let at = block
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::StorePackageField { .. }))
            .unwrap();
        let InstructionKind::StorePackageField {
            local,
            index,
            value,
        } = block.instructions[at].kind
        else {
            unreachable!()
        };
        match mutation {
            0 => block
                .instructions
                .retain(|i| !matches!(i.kind, InstructionKind::DropPackageField { .. })),
            1 => main.locals[local.0 as usize].mutable = false,
            2 => {
                block.instructions[at].kind = InstructionKind::StorePackageField {
                    local,
                    index: u32::MAX,
                    value,
                }
            }
            3 => {
                block.instructions[at].kind = InstructionKind::StorePackageField {
                    local: zil_ir::LocalId(u32::MAX),
                    index,
                    value,
                }
            }
            4 => {
                block.instructions[at].kind = InstructionKind::StorePackageField {
                    local,
                    index: 2,
                    value,
                }
            }
            5 => block
                .instructions
                .retain(|i| !matches!(i.kind, InstructionKind::DropPackage { .. })),
            _ => unreachable!(),
        }
        let errors = zil_analyze::analyze(&module, &Limits::default())
            .expect_err("raw mutation must not be admitted");
        assert!(
            errors.iter().all(|d| d.phase == Phase::Analyze),
            "{errors:?}"
        );
    }
    let mut unscoped = valid;
    for f in &mut unscoped.functions {
        for block in &mut f.blocks {
            block.instructions.retain(|i| {
                !matches!(
                    i.kind,
                    InstructionKind::ScopeEnter { .. } | InstructionKind::ScopeExit { .. }
                )
            });
        }
    }
    assert!(zil_analyze::analyze(&unscoped, &Limits::default()).is_err());
}

#[test]
fn packages072_direct_operations_keep_defining_module_field_privacy() {
    for body in ["let a=give p.a", "p.a=factory.fresh()", "p.n=2"] {
        let report = compile_project(
            &ProjectSources {
                root: ModuleId(10),
                modules: vec![
                    SourceUnit::new(
                        FileId(10),
                        ModuleId(10),
                        "main.zil",
                        format!(
                            "import factory;fn main()->i32{{var p=factory.make();{body};return 0}}"
                        ),
                    ),
                    SourceUnit::new(
                        FileId(20),
                        ModuleId(20),
                        "factory.zil",
                        "resource R{n:i32} pub package P{manages a:R,n:i32} pub fn make()->P{return P{a:R{n:1},n:0}} pub fn fresh()->R contract{result(create)}{return R{n:2}}",
                    ),
                ],
                paths: std::collections::BTreeMap::from([(vec!["factory".into()], ModuleId(20))]),
            },
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.phase == Phase::Types && d.code == "E-FIELD-PRIVATE"),
            "{}",
            report.to_json()
        );
    }
}
