//! Author source-to-machine coverage for stable ordinary Product replacement.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
use zil_ir::{CandidatePlan, HoldRoot, InstructionKind as I, TypedModule};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const MUTABLE_PRODUCT_HELPER: &str =
    include_str!("../../../tests/language-readiness/pending/mutable-product-helper-gap.zil");
const HEAD: &str = r#"extern "C" fn putchar(code:i32)->i32;fn emit(code:i32){unsafe{putchar(code)}}resource Mark{code:i32}impl Drop for Mark{fn drop(self:Self){emit(.code)}}"#;
fn directory(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = root.join(format!(
        "defer238-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
fn typed(text: &str) -> TypedModule {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "defer238.zil", text);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}

// Independent reviewer probes for the exact c88cbe1f7 production tree.
#[test]
fn review318_nested_missing_child_and_same_root_raw_attacks() {
    let source = "resource Mark{n:i32}package Child{manages mark:Mark;count:i32}package Root{child:Child;count:i32}fn bump(p:&mut Root){p.count+=1}fn main()->i32{var root=Root{child:Child{mark:Mark{n:3},count:0},count:0};bump(&mut root);let removed=root.child;return 0}";
    let mut nested = typed(source);
    let plan = zil_analyze::analyze(&nested, &Limits::default()).unwrap();
    zil_verify::verify_module(nested.clone(), plan.clone(), &nested.target, &Limits::default()).unwrap();
    let f = nested.functions.iter_mut().find(|f| f.name == "main").unwrap();
    let root = f.locals.iter().find(|l| l.name == "root").unwrap().id;
    let b = f.blocks.iter_mut().find(|b| b.instructions.iter().any(|i| matches!(i.kind, I::BorrowLocalMut { local } if local == root))).unwrap();
    let at = b.instructions.iter().position(|i| matches!(i.kind, I::TakePackageField { local, .. } if local == root)).unwrap();
    let moved = b.instructions.remove(at);
    let before = b.instructions.iter().position(|i| matches!(i.kind, I::BorrowLocalMut { local } if local == root)).unwrap();
    b.instructions.insert(before, moved);
    let e = zil_analyze::analyze(&nested, &Limits::default()).expect_err("nested child omission");
    eprintln!("review318/nested/analyze: {e:?}");
    assert!(e.iter().all(|e| e.category != ResultCategory::ResourceLimit));
    raw_reject315("review318-nested-missing-child", nested, plan, "E-VERIFY-RESOURCE");

    let source = product315("var other=Panel{mark:Mark{n:9},count:0};both(&mut panel,&mut other);return panel.count+other.count-3", "fn both(a:&mut Panel,b:&mut Panel){a.count+=1;b.count+=2}");
    let mut aliases = typed(&source);
    let mut plan = zil_analyze::analyze(&aliases, &Limits::default()).unwrap();
    zil_verify::verify_module(aliases.clone(), plan.clone(), &aliases.target, &Limits::default()).unwrap();
    let f = aliases.functions.iter_mut().find(|f| f.name == "main").unwrap();
    let panel = f.locals.iter().find(|l| l.name == "panel").unwrap().id;
    let other = f.locals.iter().find(|l| l.name == "other").unwrap().id;
    for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let I::BorrowLocalMut { local } = &mut i.kind && *local == other { *local = panel; }
    }
    let fp = plan.functions.iter_mut().find(|p| p.function == f.id).unwrap();
    for hold in &mut fp.holds { for root in &mut hold.roots { if *root == HoldRoot::Local(other) { *root = HoldRoot::Local(panel); } } }
    let result = zil_analyze::analyze(&aliases, &Limits::default());
    eprintln!("review318/same-root/analyze candidate accepted: {}", result.is_ok());
    plan.module_fingerprint = aliases.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&aliases, &plan, &Limits::default()).unwrap();
    for (route, result) in [
        ("raw", zil_verify::verify_module(aliases.clone(), plan, &aliases.target, &Limits::default())),
        ("wire", zil_verify::verify_module_bytes(&wire, &aliases.target, &Limits::default())),
    ] {
        let e = result.expect_err("same-root forgery");
        eprintln!("review318/same-root/{route}: {e:?}");
        assert!(e.iter().all(|e| e.category != ResultCategory::ResourceLimit && !e.message.contains("fingerprint")));
        assert!(e.iter().any(|e| e.code == "E-BORROW-CONFLICT"));
    }
}
#[test]
fn review318_frozen223_product_parameter_has_real_exclusive_authority() {
    let bytes = include_bytes!("review223_fixtures/schema32-mutable-product.wire");
    let limits = Limits::default();
    let (module, plan) = zil_ir::wire::decode_module_candidate(bytes, &limits).unwrap();
    let write = module.functions.iter().find(|f| f.name == "write").unwrap();
    let zil_ir::Type::Address { mutable: true, inner } = &write.locals[write.parameters[0].0 as usize].ty else { panic!("exclusive incoming address"); };
    let zil_ir::Type::Package { name, fields } = inner.as_ref() else { panic!("ordinary Product"); };
    assert!(name.ends_with(":Root"));
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[1].1, zil_ir::Type::I32);
    assert_eq!(write.blocks.iter().flat_map(|b| &b.instructions).filter(|i| matches!(i.kind, I::AddressField { index: 1, .. })).count(), 1);
    assert_eq!(write.blocks.iter().flat_map(|b| &b.instructions).filter(|i| matches!(i.kind, I::WriteAddress { .. })).count(), 1);
    eprintln!("review318/frozen223: &mut Root, one top-level i32 projection/write; raw and Wire checked");
    zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits).unwrap();
    zil_verify::verify_module_bytes(bytes, &module.target, &limits).unwrap();
    let mut shared = module.clone();
    let f = shared.functions.iter_mut().find(|f| f.name == "write").unwrap();
    let parameter = f.parameters[0];
    if let zil_ir::Type::Address { mutable, .. } = &mut f.locals[parameter.0 as usize].ty { *mutable = false; } else { panic!("address parameter"); }
    raw_reject315("review318-frozen223-shared-upgrade", shared.clone(), plan.clone(), "E-VERIFY-TYPE");
    let f = shared.functions.iter_mut().find(|f| f.name == "write").unwrap();
    for ty in &mut f.values { if let zil_ir::Type::Address { mutable, .. } = ty { *mutable = false; } }
    raw_reject315("review318-frozen223-coherent-shared-write", shared, plan, "E-VERIFY-ADDRESS");
    let head = "resource R{n:i32}package Leaf{manages owner:R;tag:i32}package Root{child:Leaf;tag:i32}fn write(p:&mut Root){p.tag=7}";
    native("review318-frozen223-source", &format!("{head}fn main()->i32{{var root=Root{{child:Leaf{{owner:R{{n:1}},tag:2}},tag:3}};write(&mut root);return root.tag-7}}"), "", 0);
    for (body, code) in [
        ("let child=root.child;write(&mut root);return 0", "E-ANALYZE-IR"),
        ("let held=&root;write(&mut root);return held.tag", "E-BORROW-CONFLICT"),
        ("let held=&mut root;write(&mut root);return held.tag", "E-BORROW-CONFLICT"),
    ] {
        let source = format!("{head}fn main()->i32{{var root=Root{{child:Leaf{{owner:R{{n:1}},tag:2}},tag:3}};{body}}}");
        let report = compile_source(&SourceUnit::new(FileId(0),ModuleId(0),"frozen223-review.zil",source), &CompileOptions::default());
        assert_eq!(report.status,ResultCategory::SafeReject,"{}",report.to_json());
        assert!(report.diagnostics.iter().any(|d| d.code == code), "{}",report.to_json());
        assert!(!report.artifacts.contains_key("executable"));
        eprintln!("review318/frozen223 conflict: {}",report.to_json());
    }
}
#[test]
fn ordinary_local_product_mutable_helper_original() {
    native("mutable-product-original315", MUTABLE_PRODUCT_HELPER, "", 0);
    let module = typed(MUTABLE_PRODUCT_HELPER);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module(module.clone(), plan, &module.target, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();
}
fn product315(body: &str, helpers: &str) -> String {
    format!(
        "resource Mark{{n:i32}}package Panel{{manages mark:Mark;count:i32}}fn bump(panel:&mut Panel){{panel.count+=1}}{helpers}fn main()->i32{{var panel=Panel{{mark:Mark{{n:3}},count:0}};{body}}}"
    )
}

#[test]
fn ordinary_local_product_mutable_helper_reborrows_and_cleanup() {
    native(
        "mutable-product-refill315",
        &product315(
            "let removed=give panel.mark;panel.mark=give removed;bump(&mut panel);return panel.count-1",
            "",
        ),
        "",
        0,
    );
    native(
        "mutable-product-nested315",
        &product315(
            "var frame=Frame{child:panel,tag:0};bump_frame(&mut frame);return frame.tag-1",
            "package Frame{child:Panel;tag:i32}fn bump_frame(frame:&mut Frame){frame.tag+=1}",
        ),
        "",
        0,
    );
    native(
        "mutable-product-reborrow315",
        &product315(
            "let parent=&mut panel;twice(parent);bump(parent);let read=&*parent;let n=read.count;bump(&mut panel);return n-3+panel.count-4",
            "fn twice(panel:&mut Panel){bump(panel);bump(panel)}",
        ),
        "",
        0,
    );
    native(
        "mutable-product-apart315",
        &product315(
            "var other=Panel{mark:Mark{n:9},count:0};both(&mut panel,&mut other);return panel.count+other.count-3",
            "fn both(left:&mut Panel,right:&mut Panel){left.count+=1;right.count+=2}",
        ),
        "",
        0,
    );
    for (failure, stdout, exit) in [(false, "QA", 0), (true, "QA", 1)] {
        native(
            "mutable-product-error-cleanup315",
            &format!(
                "{HEAD}package Panel{{manages mark:Mark;count:i32}}fn bump(panel:&mut Panel,failure:bool)->void!i32{{panel.count+=1;if failure{{return ()}}return 0}}fn main()->void!i32{{var panel=Panel{{mark:Mark{{code:65}},count:0}};defer{{emit(81)}}bump(&mut panel,{failure})?;return panel.count-1}}"
            ),
            stdout,
            exit,
        );
    }
}

#[test]
fn ordinary_local_product_mutable_helper_nearest_rejections() {
    for (label, body, helpers, category) in [
        (
            "shared-live315",
            "let held=&panel;bump(&mut panel);return held.count",
            "",
            ResultCategory::SafeReject,
        ),
        (
            "exclusive-live315",
            "let held=&mut panel;bump(&mut panel);return held.count",
            "",
            ResultCategory::SafeReject,
        ),
        (
            "exclusive-direct-read315",
            "let held=&mut panel;let n=panel.count;bump(held);return n",
            "",
            ResultCategory::SafeReject,
        ),
        (
            "exclusive-direct-write315",
            "let held=&mut panel;panel.count=9;bump(held);return 0",
            "",
            ResultCategory::SafeReject,
        ),
        (
            "child-live315",
            "let held=&mut panel;let child=&mut *held;bump(held);bump(child);return 0",
            "",
            ResultCategory::SafeReject,
        ),
        (
            "replace-live315",
            "let held=&mut panel;panel=Panel{mark:Mark{n:4},count:0};bump(held);return 0",
            "",
            ResultCategory::SafeReject,
        ),
        (
            "move-live315",
            "let held=&mut panel;let moved=panel;bump(held);return 0",
            "",
            ResultCategory::SafeReject,
        ),
        (
            "partial315",
            "let removed=give panel.mark;bump(&mut panel);return 0",
            "",
            ResultCategory::SafeReject,
        ),
        (
            "moved315",
            "let moved=panel;bump(&mut panel);return 0",
            "",
            ResultCategory::SafeReject,
        ),
        (
            "same-actual315",
            "both(&mut panel,&mut panel);return 0",
            "fn both(a:&mut Panel,b:&mut Panel){a.count+=1;b.count+=1}",
            ResultCategory::SafeReject,
        ),
        (
            "capture315",
            "let held=&mut panel;let f=()=>held.count;return f()",
            "",
            ResultCategory::Unsupported,
        ),
        (
            "return315",
            "let held=escape(&mut panel);return held.count",
            "fn escape(panel:&mut Panel)->&mut Panel{return panel}",
            ResultCategory::Unsupported,
        ),
        (
            "owned-write315",
            "replace(&mut panel);return 0",
            "fn replace(panel:&mut Panel){panel.mark=Mark{n:9}}",
            ResultCategory::Unsupported,
        ),
    ] {
        reject(label, &product315(body, helpers), category);
    }
    reject(
        "immutable315",
        &MUTABLE_PRODUCT_HELPER.replace("var panel=", "let panel="),
        ResultCategory::SafeReject,
    );
    reject(
        "store-head315",
        "package Panel{manages items:Store<i32>;count:i32}fn bump(panel:&mut Panel){panel.count+=1}fn main(){var panel=Panel{items:store_new(),count:0};bump(&mut panel)}",
        ResultCategory::Unsupported,
    );
    reject(
        "mutable-child-projection315",
        &product315(
            "var frame=Frame{child:panel,tag:0};deep(&mut frame);return 0",
            "package Frame{child:Panel;tag:i32}fn deep(frame:&mut Frame){frame.child.count+=1}",
        ),
        ResultCategory::Unsupported,
    );
}

fn raw_reject315(label: &str, module: TypedModule, mut plan: CandidatePlan, code: &str) {
    plan.module_fingerprint = module.fingerprint();
    let limits = Limits::default();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    for (route, result) in [
        (
            "raw",
            zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&wire, &module.target, &limits),
        ),
    ] {
        let errors = result.expect_err(label);
        assert!(
            errors.iter().any(|d| d.code == code),
            "{label}/{route}: {errors:?}"
        );
        assert!(
            errors
                .iter()
                .all(|d| d.category != ResultCategory::ResourceLimit
                    && !d.message.contains("fingerprint")),
            "{label}/{route}: {errors:?}"
        );
    }
}

#[test]
fn ordinary_local_product_mutable_helper_independent_roots_and_permission() {
    let module = typed(MUTABLE_PRODUCT_HELPER);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    for name in ["main", "bump"] {
        let f = module.functions.iter().find(|f| f.name == name).unwrap();
        let expected = if name == "bump" {
            HoldRoot::Parameter(0)
        } else {
            HoldRoot::Local(f.locals.iter().find(|l| l.name == "panel").unwrap().id)
        };
        let function_plan = plan.functions.iter().find(|p| p.function == f.id).unwrap();
        assert!(function_plan.holds.iter().any(|h| h.roots == [expected]));
        let mut forged = plan.clone();
        let forged_function = forged
            .functions
            .iter_mut()
            .find(|p| p.function == f.id)
            .unwrap();
        for hold in &mut forged_function.holds {
            if hold.roots == [expected] {
                hold.roots = vec![if name == "bump" {
                    HoldRoot::Local(f.parameters[0])
                } else {
                    HoldRoot::Parameter(0)
                }];
            }
        }
        raw_reject315(name, module.clone(), forged, "E-VERIFY-HOLD");
    }
    let mut immutable = module.clone();
    let f = immutable
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    f.locals
        .iter_mut()
        .find(|l| l.name == "panel")
        .unwrap()
        .mutable = false;
    raw_reject315(
        "immutable-raw315",
        immutable,
        plan.clone(),
        "E-VERIFY-MUTABILITY",
    );
    let mut shared = module;
    let f = shared
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let borrow = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find(|i| matches!(i.kind, I::BorrowLocalMut { .. }))
        .unwrap();
    let zil_ir::Type::Address { mutable, .. } = &mut f.values[borrow.result.unwrap().0 as usize]
    else {
        unreachable!()
    };
    *mutable = false;
    raw_reject315("forged-permission315", shared, plan, "E-VERIFY-TYPE");
}

#[test]
fn ordinary_local_product_mutable_helper_independent_completeness_and_liveness() {
    for (label, transfer) in [
        ("partial-raw315", "let removed=give panel.mark;"),
        ("moved-raw315", "let moved=panel;"),
        ("drop-raw315", ""),
        ("dead-raw315", ""),
    ] {
        let mut module = typed(&product315(
            &format!("bump(&mut panel);{transfer}return 0"),
            "",
        ));
        let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
        let f = module
            .functions
            .iter_mut()
            .find(|f| f.name == "main")
            .unwrap();
        let local = f.locals.iter().find(|l| l.name == "panel").unwrap().id;
        let block = f
            .blocks
            .iter_mut()
            .find(|b| {
                b.instructions
                    .iter()
                    .any(|i| matches!(i.kind,I::BorrowLocalMut{local:l} if l==local))
            })
            .unwrap();
        let moved = block
            .instructions
            .iter()
            .position(|i| match label {
                "partial-raw315" => matches!(i.kind,I::TakePackageField{local:l,..} if l==local),
                "moved-raw315" => matches!(i.kind,I::TakeLocal{local:l} if l==local),
                "drop-raw315" => matches!(i.kind,I::DropPackage{local:l} if l==local),
                "dead-raw315" => matches!(i.kind,I::StorageDead(l) if l==local),
                _ => unreachable!(),
            })
            .unwrap();
        let instruction = block.instructions.remove(moved);
        let before = block
            .instructions
            .iter()
            .position(|i| matches!(i.kind,I::BorrowLocalMut{local:l} if l==local))
            .unwrap();
        block.instructions.insert(before, instruction);
        let errors = zil_analyze::analyze(&module, &Limits::default()).expect_err(label);
        assert!(
            errors
                .iter()
                .all(|d| d.category != ResultCategory::ResourceLimit),
            "{label}: {errors:?}"
        );
        raw_reject315(
            label,
            module,
            plan,
            match label {
                "moved-raw315" | "drop-raw315" | "dead-raw315" => "E-VERIFY-INITIALIZATION",
                _ => "E-VERIFY-RESOURCE",
            },
        );
    }
}

#[test]
fn ordinary_local_product_mutable_helper_independent_live_windows() {
    for attack in ["drop-live315", "take-live315", "second-loan315"] {
        let body = match attack {
            "take-live315" => {
                "let first=&mut panel;bump(first);let removed=give panel.mark;return 0"
            }
            "second-loan315" => {
                "let first=&mut panel;bump(first);let second=&mut panel;bump(second);return 0"
            }
            _ => "let first=&mut panel;bump(first);return 0",
        };
        let mut module = typed(&product315(body, ""));
        let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
        let f = module
            .functions
            .iter_mut()
            .find(|f| f.name == "main")
            .unwrap();
        let local = f.locals.iter().find(|l| l.name == "panel").unwrap().id;
        let block = f
            .blocks
            .iter_mut()
            .find(|b| {
                b.instructions
                    .iter()
                    .any(|i| matches!(i.kind,I::BorrowLocalMut{local:l} if l==local))
            })
            .unwrap();
        let moved = block
            .instructions
            .iter()
            .rposition(|i| match attack {
                "take-live315" => matches!(i.kind,I::TakePackageField{local:l,..} if l==local),
                "second-loan315" => matches!(i.kind,I::BorrowLocalMut{local:l} if l==local),
                _ => matches!(i.kind,I::DropPackage{local:l} if l==local),
            })
            .unwrap();
        let instruction = block.instructions.remove(moved);
        let before = block
            .instructions
            .iter()
            .position(|i| matches!(i.kind, I::Call { .. }))
            .unwrap();
        block.instructions.insert(before, instruction);
        // Analyze may publish an untrusted candidate; ordinary strong-loan
        // conflicts are decided by the independent Verify loan checker.
        raw_reject315(attack, module, plan, "E-BORROW-CONFLICT");
    }
}

#[test]
fn ordinary_local_product_mutable_helper_exact_phase_budgets() {
    fn boundary<T: std::fmt::Debug + PartialEq>(
        phase: Phase,
        run: impl Fn(usize) -> Result<T, Vec<zil_base::Diagnostic>>,
    ) -> usize {
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        let expected = run(high).unwrap();
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            match run(middle) {
                Ok(actual) => {
                    assert_eq!(actual, expected);
                    high = middle;
                }
                Err(errors) => {
                    assert!(!errors.is_empty());
                    assert!(
                        errors.iter().all(
                            |d| d.phase == phase && d.category == ResultCategory::ResourceLimit
                        ),
                        "{phase:?}: {errors:?}"
                    );
                    low = middle;
                }
            }
        }
        assert_eq!(run(high).unwrap(), expected);
        let errors = run(high - 1).unwrap_err();
        assert!(!errors.is_empty());
        assert!(
            errors
                .iter()
                .all(|d| d.phase == phase && d.category == ResultCategory::ResourceLimit),
            "{phase:?}: {errors:?}"
        );
        assert_eq!(run(high).unwrap(), expected);
        high
    }
    let unit = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "budget315.zil",
        MUTABLE_PRODUCT_HELPER,
    );
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let limits = |n| Limits {
        max_analysis_steps: n,
        ..Limits::default()
    };
    let types = boundary(Phase::Types, |n| {
        zil_types::check(&core, &TargetProfile::host_llvm(), &limits(n))
    });
    let module = typed(MUTABLE_PRODUCT_HELPER);
    let analyze = boundary(Phase::Analyze, |n| {
        zil_analyze::analyze(&module, &limits(n))
    });
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let verify = boundary(Phase::Verify, |n| {
        zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits(n))
            .map(|v| v.fingerprint())
    });
    println!(
        "315 phase budgets: types={types} analyze={analyze} verify={verify}; N/N-1/fresh verified"
    );
}
fn native(label: &str, source: &str, stdout: &str, exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let path = directory(label);
        fs::write(path.join("source.zil"), source).unwrap();
        let backend = if cfg!(feature = "llvm-api") {
            NativeBackend::LlvmApi
        } else {
            NativeBackend::Clang
        };
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "defer238.zil", source),
            &CompileOptions {
                stop_after: Stage::Build,
                backend,
                optimization,
                output: Some(path.join("program")),
                emit_dir: Some(path.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(path.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        let summary = report.native_codegen.as_ref().unwrap();
        assert_eq!(summary.backend, backend);
        assert!(summary.fallback.is_none());
        let output = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        fs::write(path.join("stdout"), &output.stdout).unwrap();
        fs::write(path.join("stderr"), &output.stderr).unwrap();
        assert_eq!(output.status.code(), Some(exit), "{label}: {output:?}");
        assert_eq!(output.stdout, stdout.as_bytes(), "{label}: {output:?}");
        assert!(output.stderr.is_empty(), "{label}: {output:?}");
    }
}
const ORIGINAL: &str = r###"extern "C" fn putchar(code:i32)->i32;
fn emit(code:i32){unsafe{putchar(code)}}
resource Mark{code:i32}
impl Drop for Mark{fn drop(self:Self){emit(.code)}}
package Owner{manages marker:Mark}
fn relay(owned:Owner)->Owner{return owned}
fn main()->i32{
var current=Owner{marker:Mark{code:65}}
defer{emit(81)}
let late=Mark{code:76}
current=relay(current)
return 0
}
"###;

fn reject(label: &str, source: &str, category: ResultCategory) {
    let path = directory(label);
    fs::write(path.join("source.zil"), source).unwrap();
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "reject238.zil", source),
        &CompileOptions {
            stop_after: Stage::Verify,
            emit_dir: Some(path.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(path.join("report.json"), report.to_json()).unwrap();
    assert_eq!(report.status, category, "{label}: {}", report.to_json());
}
#[test]
fn original_relay_retains_shell_age_and_unchanged_cleanup_oracle() {
    native("original-relay", ORIGINAL, "LQA", 0);
    native(
        "original-without-defer",
        &ORIGINAL.replace("defer{emit(81)}", ""),
        "LA",
        0,
    );
    let module = typed(ORIGINAL);
    let function = module.functions.iter().find(|f| f.name == "main").unwrap();
    let local = function
        .locals
        .iter()
        .find(|l| l.name == "current")
        .unwrap()
        .id;
    assert_eq!(
        function
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .filter(|i| matches!(i.kind,I::Store{local:l,..}if l==local))
            .count(),
        1
    );
    assert!(
        !function
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind,I::TakeLocal{local:l}if l==local))
    );
    assert!(
        function
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind,I::TakePackageField{local:l,..}if l==local))
    );
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module(module.clone(), plan, &module.target, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();
}
#[test]
fn independent_rhs_prepares_then_cleans_declared_fields_and_publishes_copy() {
    let source = format!(
        "{HEAD}package Owner{{manages first:Mark;manages second:Mark;tag:i32}}fn make()->Owner{{return Owner{{first:Mark{{code:67}},second:Mark{{code:68}},tag:2}}}}fn main()->i32{{var current=Owner{{first:Mark{{code:65}},second:Mark{{code:66}},tag:1}};defer{{emit(81)}}let late=Mark{{code:76}};current=make();if current.tag!=2{{return 9}}return 0}}"
    );
    native("independent", &source, "ABLQCD", 0);
    let partial = source.replace(
        "defer{emit(81)}",
        "let removed=give current.first;defer{emit(81)}",
    );
    native("partial", &partial, "BLQACD", 0);
    let prepared = source.replace("current=make();", "let prepared=make();current=prepared;");
    native("prepared", &prepared, "ABLQCD", 0);
}
#[test]
fn rhs_error_keeps_old_value_and_self_argument_failure_keeps_actual_stage() {
    let head = format!(
        "{HEAD}package Owner{{manages marker:Mark;tag:i32}}fn fail()->void!i32{{return ()}}fn maker(failure:bool)->void!Owner{{if failure{{return ()}}return Owner{{marker:Mark{{code:66}},tag:2}}}}fn relay_before(n:i32,owned:Owner)->Owner{{return owned}}fn relay_after(owned:Owner,n:i32)->Owner{{return owned}}"
    );
    for (label, rhs, output, exit) in [
        ("independent-error", "maker(true)?", "LQA", 1),
        ("independent-success", "maker(false)?", "ALQB", 0),
        ("argument-before", "relay_before(fail()?,current)", "LQA", 1),
        ("argument-after", "relay_after(current,fail()?)", "ALQ", 1),
    ] {
        let source = format!(
            "{head}fn main()->void!i32{{var current=Owner{{marker:Mark{{code:65}},tag:1}};defer{{emit(81)}}let late=Mark{{code:76}};current={rhs};return 0}}"
        );
        native(label, &source, output, exit);
    }
}
#[test]
fn self_call_keeps_each_actual_binding_and_copy_read_available_after_commit() {
    let head = format!(
        "{HEAD}package Owner{{manages marker:Mark;tag:i32}}fn relay(owned:Owner,n:i32)->Owner{{return owned}}"
    );
    let source = format!(
        "{head}fn main()->i32{{var current=Owner{{marker:Mark{{code:65}},tag:1}};defer{{emit(81)}}let late=Mark{{code:76}};current=relay(current,(()=>{{let current=3;return current}})());if current.tag!=1{{return 8}}return 0}}"
    );
    native("shadow", &source, "LQA", 0);
}
#[test]
fn ordinary_whole_move_copy_reads_live_windows_and_complex_self_remain_closed() {
    let head = format!(
        "{HEAD}package Owner{{manages marker:Mark;tag:i32}}fn make()->Owner{{return Owner{{marker:Mark{{code:66}},tag:2}}}}fn relay(owned:Owner,n:i32)->Owner{{return owned}}"
    );
    for (label, body, category) in [
        (
            "moved-copy",
            "let moved=current;let read=current.tag;",
            ResultCategory::SafeReject,
        ),
        (
            "moved-republish",
            "let moved=current;current=make();",
            ResultCategory::SafeReject,
        ),
        (
            "copy-loan",
            "let held=&current.tag;current=make();let read=*held;",
            ResultCategory::SafeReject,
        ),
        (
            "whole-loan",
            "let held=&current;current=make();let read=held.tag;",
            ResultCategory::SafeReject,
        ),
        (
            "callback-loan",
            "let peek=()=>current.tag;current=relay(current,peek());",
            ResultCategory::SafeReject,
        ),
        (
            "self-copy",
            "current=relay(current,current.tag);",
            ResultCategory::Unsupported,
        ),
        (
            "complex-self",
            "current=relay(if true{current}else{make()},0);",
            ResultCategory::Unsupported,
        ),
    ] {
        let source = format!(
            "{head}fn main()->i32{{var current=Owner{{marker:Mark{{code:65}},tag:1}};{body}return 0}}"
        );
        reject(label, &source, category);
    }
}
#[test]
fn replacement_types_budget_is_paid_before_field_growth() {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "budget238.zil", ORIGINAL);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let defaults = Limits::default();
    let target = TargetProfile::host_llvm();
    let expected = zil_types::check(&core, &target, &defaults).unwrap();
    let run = |n| match zil_types::check(
        &core,
        &target,
        &Limits {
            max_analysis_steps: n,
            ..defaults.clone()
        },
    ) {
        Ok(actual) => {
            assert_eq!(actual, expected);
            true
        }
        Err(errors) => {
            assert!(!errors.is_empty());
            assert!(
                errors
                    .iter()
                    .all(|d| d.category == ResultCategory::ResourceLimit),
                "{errors:?}"
            );
            false
        }
    };
    let (mut lo, mut hi) = (0, defaults.max_analysis_steps);
    assert!(run(hi));
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if run(mid) { hi = mid } else { lo = mid }
    }
    assert!(run(hi));
    assert!(!run(hi - 1));
    assert!(run(hi));
    let path = directory("types-budget");
    fs::write(
        path.join("boundary.json"),
        format!(
            "{{\"types\":{hi},\"n\":true,\"n_minus_one_resource_limit\":true,\"fresh_n\":true}}\n"
        ),
    )
    .unwrap();
}

#[test]
fn self_call_reuses_ordinary_inferred_and_explicit_template_instantiation() {
    let head = format!(
        "{HEAD}package Owner{{manages marker:Mark;tag:i32}}fn relay<T:StoreElement>(owned:T)->T{{return owned}}"
    );
    for (label, call) in [
        ("generic-inferred", "relay(current)"),
        ("generic-explicit", "relay::<Owner>(current)"),
    ] {
        let source = format!(
            "{head}fn main()->i32{{var current=Owner{{marker:Mark{{code:65}},tag:1}};defer{{emit(81)}}let late=Mark{{code:76}};current={call};if current.tag!=1{{return 8}}return 0}}"
        );
        native(label, &source, "LQA", 0);
    }
}
