//! Complete Product shared borrows retain actual storage and resource debts.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
use zil_ir::{CandidatePlan, HoldRoot, InstructionKind, Type, TypedModule};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const TYPES: &str =
    "resource R{n:i32}package Leaf{manages owner:R;value:i32}package Root{child:Leaf;tag:i32}";
const READ: &str = "fn value(p:&Root)->i32{return p.child.value+p.child.owner.n+p.tag}";
const ROOT: &str = "Root{child:Leaf{owner:R{n:5},value:7},tag:11}";

fn directory(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = root.join(format!(
        "shared224-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
fn source(body: &str) -> String {
    format!("{TYPES}{READ}{body}")
}
fn native_backend() -> NativeBackend {
    if cfg!(feature = "llvm-api") {
        NativeBackend::LlvmApi
    } else {
        NativeBackend::Clang
    }
}
fn assert_native_route(report: &zil_driver::CompileReport) {
    let summary = report.native_codegen.as_ref().unwrap();
    assert_eq!(summary.backend, native_backend());
    assert!(summary.fallback.is_none());
    if native_backend() == NativeBackend::LlvmApi {
        assert!(!summary.object_reports.is_empty());
        assert!(
            summary
                .object_reports
                .iter()
                .all(|r| r.contains("\"input_kind\":\"builder\"")
                    && r.contains("\"verify_before_after\":true"))
        );
    }
}
fn compile(label: &str, text: &str, limits: Limits) -> zil_driver::CompileReport {
    let dir = directory(label);
    fs::write(dir.join("source.zil"), text).unwrap();
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "shared224.zil", text),
        &CompileOptions {
            limits,
            emit_dir: Some(dir.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(dir.join("report.json"), report.to_json()).unwrap();
    report
}

#[test]
fn shared_parameter_local_alias_nested_fields_and_nll_are_usable() {
    for (label, body) in [
        (
            "parameter",
            format!("fn main()->i32{{let root={ROOT};return value(&root)-23}}"),
        ),
        (
            "alias-nll",
            format!(
                "fn main()->i32{{let root={ROOT};let p=&root;let q=p;let n=value(q);let moved=root;return n+value(&moved)-46}}"
            ),
        ),
        (
            "nested-reborrow",
            format!(
                "fn leaf(p:&Leaf)->i32{{return p.value+p.owner.n}}fn relay(p:&Root)->i32{{return value(&*p)+leaf(&p.child)}}fn main()->i32{{let root={ROOT};let p=&root;let n=relay(p);let child=root.child;return n+child.value-42}}"
            ),
        ),
        (
            "resource-head",
            format!(
                "fn head(p:&R)->i32{{return p.n}}fn main()->i32{{let root={ROOT};let p=&root;return head(&p.child.owner)-5}}"
            ),
        ),
        (
            "branch-loop",
            format!(
                "fn sum(p:&Root,choose:bool)->i32{{var n=0;for i in 0..3{{if choose{{n=n+value(p)}}else{{n=n+p.child.value}}}}return n}}fn main()->i32{{let root={ROOT};return sum(&root,true)+sum(&root,false)-90}}"
            ),
        ),
        (
            "direct-child",
            format!(
                "fn leaf(p:&Leaf)->i32{{return p.value+p.owner.n}}fn main()->i32{{let root={ROOT};let child=&root.child;return leaf(child)-12}}"
            ),
        ),
    ] {
        let report = compile(label, &source(&body), Limits::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
}

#[test]
fn zero_storage_ordinary_heads_do_not_inherit_store_element_rules() {
    let text = "resource Empty{}package Leaf{manages token:Empty}package Root{child:Leaf;tag:i32}fn touch(p:&Leaf){}fn read(p:&Root)->i32{touch(&p.child);return p.tag}fn main()->i32{let root=Root{child:Leaf{token:Empty{}},tag:3};return read(&root)-3}";
    let report = compile("zero-storage-head", text, Limits::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let module = typed("zero-storage-source", text);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    raw_wire("zero-storage-raw", module, plan, false);
}

#[test]
fn live_shared_borrows_prevent_consumption_replacement_and_mutation() {
    for (label, body) in [
        (
            "whole-move",
            format!("fn main()->i32{{let root={ROOT};let p=&root;let moved=root;return value(p)}}"),
        ),
        (
            "child-take",
            format!(
                "fn main()->i32{{let root={ROOT};let p=&root;let moved=root.child;return value(p)}}"
            ),
        ),
        (
            "child-replace",
            format!(
                "fn main()->i32{{var root={ROOT};let p=&root;root.child=Leaf{{owner:R{{n:9}},value:3}};return value(p)}}"
            ),
        ),
        (
            "copy-write",
            format!("fn main()->i32{{var root={ROOT};let p=&root;p.tag=8;return value(p)}}"),
        ),
        (
            "mutable-reborrow",
            format!("fn main()->i32{{var root={ROOT};let p=&root;let q=&mut *p;return value(p)}}"),
        ),
        (
            "after-child-take",
            format!("fn main()->i32{{let root={ROOT};let moved=root.child;return value(&root)}}"),
        ),
        (
            "after-whole-move",
            format!("fn main()->i32{{let root={ROOT};let moved=root;return value(&root)}}"),
        ),
    ] {
        let report = compile(label, &source(&body), Limits::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{label}: {}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
}

#[test]
fn sharing_does_not_add_mutable_owned_reads_retention_or_escape() {
    for (label, body) in [
        ("mutable-product", "fn bad(p:&mut Root){}fn main(){}".into()),
        (
            "borrow-return",
            "fn bad(p:&Root)->&Root{return p}fn main(){}".into(),
        ),
        (
            "copy-child",
            "fn bad(p:&Root){let child=p.child}fn main(){}".into(),
        ),
        (
            "copy-owner",
            "fn bad(p:&Root){let owner=p.child.owner}fn main(){}".into(),
        ),
        (
            "capture",
            format!("fn main()->i32{{let root={ROOT};let p=&root;let f=()=>value(p);return f()}}"),
        ),
    ] {
        let report = compile(label, &source(&body), Limits::default());
        assert_ne!(report.status, ResultCategory::Success, "{label}");
        assert_ne!(report.status, ResultCategory::ResourceLimit, "{label}");
        assert!(!report.artifacts.contains_key("executable"));
    }
}

fn typed(label: &str, text: &str) -> TypedModule {
    let out = directory(label);
    fs::write(out.join("source.zil"), text).unwrap();
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "raw224.zil", text);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}

fn raw_wire(label: &str, module: TypedModule, mut plan: CandidatePlan, reject: bool) {
    // A fresh exact fingerprint prevents an unrelated stale-plan rejection
    // from standing in for an independent semantic/lifetime check.
    plan.module_fingerprint = module.fingerprint();
    let out = directory(label);
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&wire, &Limits::default()).unwrap(),
        (module.clone(), plan.clone())
    );
    fs::write(out.join("input.wire"), &wire).unwrap();
    fs::write(out.join("module.txt"), format!("{module:#?}")).unwrap();
    fs::write(out.join("candidate.txt"), format!("{plan:#?}")).unwrap();
    for (route, result) in [
        (
            "raw",
            zil_verify::verify_module(
                module.clone(),
                plan.clone(),
                &module.target,
                &Limits::default(),
            ),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()),
        ),
    ] {
        fs::write(out.join(format!("{route}.txt")), format!("{result:#?}")).unwrap();
        if reject {
            let errors = result.expect_err(label);
            assert!(
                errors.iter().all(
                    |d| d.phase == Phase::Verify && d.category != ResultCategory::ResourceLimit
                ),
                "{label}/{route}: {errors:#?}"
            );
            assert!(
                errors.iter().all(|d| !d.message.contains("fingerprint")),
                "{label}/{route}: {errors:#?}"
            );
            let (code, message) = match label {
                "mutable" => ("E-UNSUPPORTED", "shared synchronous"),
                "wrong-field" => ("E-VERIFY-ADDRESS", "projection index"),
                "owning-read" => ("E-VERIFY-ADDRESS", "finite Copy"),
                "missing-hold" | "forged-parent" => {
                    ("E-VERIFY-HOLD", "independently reconstructed provenance")
                }
                "whole-move-live" | "child-take-live" | "whole-drop-live" | "child-drop-live" => {
                    ("E-BORROW-CONFLICT", "overlaps a shared address")
                }
                "borrow-incomplete" => ("E-VERIFY-RESOURCE", "every owned child live"),
                "expired-storage" => ("E-VERIFY-CLEANUP", "outside its active storage scope"),
                _ => panic!("unregistered attack {label}"),
            };
            assert!(
                errors
                    .iter()
                    .any(|d| d.code == code && d.message.contains(message)),
                "{label}/{route}: {errors:#?}"
            );
        } else {
            result.unwrap_or_else(|e| panic!("{label}/{route}: {e:#?}"));
        }
    }
}

#[test]
fn real_source_has_independent_raw_and_wire_checks() {
    let text = source(&format!(
        "fn main()->i32{{let root={ROOT};return value(&root)-23}}"
    ));
    let module = typed("raw-source", &text);
    let candidate = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    raw_wire("raw-positive", module.clone(), candidate.clone(), false);
    for attack in [
        "mutable",
        "wrong-field",
        "owning-read",
        "missing-hold",
        "forged-parent",
    ] {
        let mut raw = module.clone();
        let mut plan = candidate.clone();
        match attack {
            "mutable" => {
                let f = raw.functions.iter_mut().find(|f| f.name == "main").unwrap();
                let i = f
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, InstructionKind::BorrowLocal { .. }))
                    .unwrap();
                let InstructionKind::BorrowLocal { local } = i.kind else {
                    unreachable!()
                };
                i.kind = InstructionKind::BorrowLocalMut { local };
                if let Type::Address { mutable, .. } = &mut f.values[i.result.unwrap().0 as usize] {
                    *mutable = true;
                }
            }
            "wrong-field" => {
                let f = raw
                    .functions
                    .iter_mut()
                    .find(|f| f.name == "value")
                    .unwrap();
                let i = f
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, InstructionKind::AddressField { .. }))
                    .unwrap();
                if let InstructionKind::AddressField { index, .. } = &mut i.kind {
                    *index = 99;
                }
            }
            "owning-read" => {
                let f = raw
                    .functions
                    .iter_mut()
                    .find(|f| f.name == "value")
                    .unwrap();
                let i = f
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, InstructionKind::AddressField { .. }))
                    .unwrap();
                let InstructionKind::AddressField { address, .. } = i.kind else {
                    unreachable!()
                };
                let Type::Address { inner, .. } = &f.values[address.0 as usize] else {
                    unreachable!()
                };
                let owner = inner.as_ref().clone();
                i.kind = InstructionKind::ReadAddress { address };
                f.values[i.result.unwrap().0 as usize] = owner;
            }
            "missing-hold" => {
                for f in &mut plan.functions {
                    for hold in &mut f.holds {
                        hold.roots.clear();
                    }
                }
            }
            "forged-parent" => {
                let function = raw.functions.iter().find(|f| f.name == "value").unwrap();
                let f = plan
                    .functions
                    .iter_mut()
                    .find(|f| f.function == function.id)
                    .unwrap();
                let hold = f.holds.iter_mut().find(|h| !h.roots.is_empty()).unwrap();
                // The address is an incoming parameter target, not storage of
                // the local that contains the pointer bytes.
                hold.roots = vec![HoldRoot::Local(function.parameters[0])];
            }
            _ => unreachable!(),
        }
        raw_wire(attack, raw, plan, true);
    }
}

#[test]
fn candidate_cannot_hide_live_loan_or_incomplete_product_attacks() {
    for attack in [
        "whole-move-live",
        "child-take-live",
        "whole-drop-live",
        "child-drop-live",
        "borrow-incomplete",
        "expired-storage",
    ] {
        let transfer = match attack {
            "whole-move-live" => "let moved=root;",
            "child-take-live" | "borrow-incomplete" => "let child=root.child;",
            _ => "",
        };
        let text = source(&format!(
            "fn main()->i32{{let root={ROOT};let p=&root;let n=value(p);{transfer}return n-23}}"
        ));
        let mut module = typed(&format!("{attack}-source"), &text);
        let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
        raw_wire(
            &format!("{attack}-control"),
            module.clone(),
            plan.clone(),
            false,
        );
        let function = module
            .functions
            .iter_mut()
            .find(|f| f.name == "main")
            .unwrap();
        let root = function
            .locals
            .iter()
            .find(|l| l.name == "root")
            .unwrap()
            .id;
        let block = function
            .blocks
            .iter_mut()
            .find(|b| {
                b.instructions
                    .iter()
                    .any(|i| matches!(i.kind,InstructionKind::BorrowLocal{local} if local==root))
            })
            .unwrap();
        let moving = block
            .instructions
            .iter()
            .position(|i| match attack {
                "whole-move-live" => {
                    matches!(i.kind,InstructionKind::TakeLocal{local} if local==root)
                }
                "child-take-live" | "borrow-incomplete" => {
                    matches!(i.kind,InstructionKind::TakePackageField{local,..} if local==root)
                }
                "whole-drop-live" | "child-drop-live" => {
                    matches!(i.kind,InstructionKind::DropPackage{local} if local==root)
                }
                "expired-storage" => {
                    matches!(i.kind,InstructionKind::StorageDead(local) if local==root)
                }
                _ => unreachable!(),
            })
            .unwrap();
        let mut instruction = block.instructions.remove(moving);
        if attack == "child-drop-live" {
            instruction.kind = InstructionKind::DropPackageField {
                local: root,
                index: 0,
            };
        }
        let before = block
            .instructions
            .iter()
            .position(|i| {
                if attack == "borrow-incomplete" {
                    matches!(i.kind,InstructionKind::BorrowLocal{local} if local==root)
                } else {
                    matches!(i.kind, InstructionKind::Call { .. })
                }
            })
            .unwrap();
        block.instructions.insert(before, instruction);
        if attack == "borrow-incomplete" {
            let errors = zil_analyze::analyze(&module, &Limits::default()).expect_err(attack);
            assert!(
                errors
                    .iter()
                    .all(|d| d.category != ResultCategory::ResourceLimit),
                "{attack}: {errors:#?}"
            );
        }
        raw_wire(attack, module, plan, true);
    }
}

#[test]
fn nested_shared_projection_executes_with_real_drop_order_at_o0_and_o2() {
    let text = include_str!("../../../examples/mainline/shared_product224/main.zil");
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = directory(&format!("native-{optimization:?}"));
        let executable = dir.join("program");
        fs::write(dir.join("source.zil"), text).unwrap();
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "native224.zil", text),
            &CompileOptions {
                backend: native_backend(),
                stop_after: Stage::Build,
                optimization,
                output: Some(executable.clone()),
                emit_dir: Some(dir.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(dir.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_native_route(&report);
        let output = Command::new(executable).output().unwrap();
        fs::write(
            dir.join("exit.json"),
            format!("{{\"exit_code\":{}}}\n", output.status.code().unwrap()),
        )
        .unwrap();
        fs::write(dir.join("stdout"), &output.stdout).unwrap();
        fs::write(dir.join("stderr"), &output.stderr).unwrap();
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(output.stdout, b"XAB");
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn direct_owning_field_addresses_reach_native_storage_and_nll() {
    let resource = r#"
extern "C" fn putchar(c:i32)->i32;
resource Token{code:i32;value:i32}
impl Drop for Token{fn drop(self:Self){unsafe{putchar(.code)}}}
package Packet{manages owned:Token;weight:i32}
fn head(p:&Token)->i32{return p.value}
fn main()->i32{
 let packet=Packet{owned:Token{code:65,value:5},weight:7}
 let p=&packet.owned
 let n=head(p)
 let owner=packet.owned
 unsafe{putchar(88)}
 return n-5
}
"#;
    for (label, text, expected) in [
        (
            "direct-child-review",
            include_str!("../../../examples/mainline/shared_product224/child-nll-control.zil"),
            b"XAB".as_slice(),
        ),
        ("direct-resource", resource, b"XA".as_slice()),
    ] {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = directory(&format!("{label}-{optimization:?}"));
            let executable = dir.join("program");
            fs::write(dir.join("source.zil"), text).unwrap();
            let report = compile_source(
                &SourceUnit::new(FileId(0), ModuleId(0), "direct224.zil", text),
                &CompileOptions {
                    backend: native_backend(),
                    stop_after: Stage::Build,
                    optimization,
                    output: Some(executable.clone()),
                    emit_dir: Some(dir.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(dir.join("report.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{label}: {}",
                report.to_json()
            );
            assert_native_route(&report);
            let output = Command::new(executable).output().unwrap();
            fs::write(dir.join("stdout"), &output.stdout).unwrap();
            fs::write(dir.join("stderr"), &output.stderr).unwrap();
            fs::write(
                dir.join("exit.json"),
                format!("{{\"exit_code\":{}}}\n", output.status.code().unwrap()),
            )
            .unwrap();
            assert_eq!(output.status.code(), Some(0));
            assert_eq!(output.stdout, expected);
            assert!(output.stderr.is_empty());
        }
    }
}

#[cfg(unix)]
#[test]
fn allocated_product_leaves_are_cleaned_on_each_real_allocator_failure() {
    use std::os::unix::fs::PermissionsExt;
    let text = include_str!("../../../examples/mainline/shared_product224/allocation.zil");
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("clang"));
    let shim = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/language/173/allocator_probe.c");
    let quote =
        |path: &std::path::Path| format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"));
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = directory(&format!("allocation-{optimization:?}"));
        let executable = dir.join("normal");
        let captured = dir.join("actual-api.o");
        // Keep the normal compiler link unchanged. The wrapper merely retains
        // the actual API-produced object before the driver's temp cleanup.
        let driver_clang = if native_backend() == NativeBackend::LlvmApi {
            let wrapper = dir.join("capture-clang.sh");
            fs::write(&wrapper,format!("#!/bin/sh\nfor arg in \"$@\"; do\n  case \"$arg\" in *.o|*.obj) cp -- \"$arg\" {} ;; esac\ndone\nexec {} \"$@\"\n",quote(&captured),quote(&clang))).unwrap();
            fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
            wrapper
        } else {
            clang.clone()
        };
        fs::write(dir.join("source.zil"), text).unwrap();
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "allocation224.zil", text),
            &CompileOptions {
                backend: native_backend(),
                clang: Some(driver_clang),
                stop_after: Stage::Build,
                optimization,
                output: Some(executable.clone()),
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
        assert_native_route(&report);
        let normal = Command::new(&executable).output().unwrap();
        fs::write(dir.join("normal.stdout"), &normal.stdout).unwrap();
        fs::write(dir.join("normal.stderr"), &normal.stderr).unwrap();
        fs::write(
            dir.join("normal.json"),
            format!("{{\"exit_code\":{}}}\n", normal.status.code().unwrap()),
        )
        .unwrap();
        assert_eq!(normal.status.code(), Some(0));
        assert_eq!(normal.stdout, b"XA");
        assert!(normal.stderr.is_empty());
        let input = if native_backend() == NativeBackend::LlvmApi {
            assert!(captured.is_file());
            captured
        } else {
            dir.join("phases/llvm.ll")
        };
        let instrumented = dir.join("instrumented");
        let opt = match optimization {
            Optimization::O0 => "-O0",
            Optimization::O2 => "-O2",
        };
        let linked = Command::new(&clang)
            .arg(opt)
            .arg(input)
            .arg(&shim)
            .args(["-Wl,--wrap=realloc", "-Wl,--wrap=free", "-o"])
            .arg(&instrumented)
            .output()
            .unwrap();
        fs::write(dir.join("link.stdout"), &linked.stdout).unwrap();
        fs::write(dir.join("link.stderr"), &linked.stderr).unwrap();
        assert!(
            linked.status.success(),
            "{}",
            String::from_utf8_lossy(&linked.stderr)
        );
        for (fail, exit, stdout, allocated) in [
            (0, 0, b"XA".as_slice(), 2),
            (1, 10, b"F".as_slice(), 0),
            (2, 11, b"GA".as_slice(), 1),
        ] {
            let output = Command::new(&instrumented)
                .env("ZIL_TEST_FAIL_AT", fail.to_string())
                .output()
                .unwrap();
            fs::write(dir.join(format!("failure-{fail}.stdout")), &output.stdout).unwrap();
            fs::write(dir.join(format!("failure-{fail}.stderr")), &output.stderr).unwrap();
            fs::write(
                dir.join(format!("failure-{fail}.json")),
                format!(
                    "{{\"fail_at\":{fail},\"exit_code\":{}}}\n",
                    output.status.code().unwrap()
                ),
            )
            .unwrap();
            assert_eq!(output.status.code(), Some(exit), "fail={fail}");
            assert_eq!(output.stdout, stdout);
            let allocation = String::from_utf8(output.stderr).unwrap();
            for fragment in [
                "\"live\":0".to_string(),
                format!("\"allocated\":{allocated}"),
                format!("\"frees\":{allocated}"),
                format!("\"failures\":{}", usize::from(fail != 0)),
            ] {
                assert!(allocation.contains(&fragment), "{allocation}");
            }
        }
    }
}

#[test]
fn shared_product_work_has_exact_n_minus_one_and_fresh_boundaries() {
    let text = source(&format!(
        "fn main()->i32{{let root={ROOT};let p=&root;return value(p)-23}}"
    ));
    let mut low = 0usize;
    let mut high = Limits::default().max_analysis_steps;
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        let report = compile(
            "budget-search",
            &text,
            Limits {
                max_analysis_steps: middle,
                ..Limits::default()
            },
        );
        match report.status {
            ResultCategory::Success => high = middle,
            ResultCategory::ResourceLimit => low = middle,
            other => panic!("unexpected budget result {other:?}: {}", report.to_json()),
        }
    }
    assert!(high > 0);
    for (label, n, expected) in [
        ("n", high, ResultCategory::Success),
        ("n-minus-one", high - 1, ResultCategory::ResourceLimit),
        ("fresh-n", high, ResultCategory::Success),
    ] {
        let report = compile(
            label,
            &text,
            Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        );
        assert_eq!(report.status, expected, "{label} {n}: {}", report.to_json());
    }
    let dir = directory("budget-boundary");
    fs::write(dir.join("boundary.json"),format!("{{\"max_analysis_steps\":{high},\"n\":\"Success\",\"n_minus_one\":\"ResourceLimit\",\"fresh_n\":\"Success\"}}\n")).unwrap();
}

#[test]
fn each_changed_phase_has_its_own_exact_budget_boundary() {
    fn boundary<T: std::fmt::Debug + PartialEq>(
        label: &str,
        phase: Phase,
        run: impl Fn(usize) -> Result<T, Vec<zil_base::Diagnostic>>,
    ) -> usize {
        let mut low = 0;
        let mut high = Limits::default().max_analysis_steps;
        let expected = run(high).unwrap();
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            match run(middle) {
                Ok(actual) => {
                    assert_eq!(actual, expected, "{label}");
                    high = middle;
                }
                Err(errors) => {
                    assert!(
                        errors.iter().all(
                            |d| d.phase == phase && d.category == ResultCategory::ResourceLimit
                        ),
                        "{label}: {errors:#?}"
                    );
                    low = middle;
                }
            }
        }
        assert_eq!(run(high).unwrap(), expected, "{label}/N");
        let errors = run(high - 1).unwrap_err();
        assert!(
            errors
                .iter()
                .all(|d| d.phase == phase && d.category == ResultCategory::ResourceLimit),
            "{label}/N-1: {errors:#?}"
        );
        assert_eq!(run(high).unwrap(), expected, "{label}/fresh-N");
        high
    }
    let text = source(&format!(
        "fn main()->i32{{let root={ROOT};let p=&root;return value(p)-23}}"
    ));
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "budget224.zil", &text);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let limits = |n| Limits {
        max_analysis_steps: n,
        ..Limits::default()
    };
    let types = boundary("types", Phase::Types, |n| {
        zil_types::check(&core, &TargetProfile::host_llvm(), &limits(n))
    });
    let module = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let analyze = boundary("analyze", Phase::Analyze, |n| {
        zil_analyze::analyze(&module, &limits(n))
    });
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let verify = boundary("verify", Phase::Verify, |n| {
        zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits(n))
            .map(|v| v.fingerprint())
    });
    let out = directory("phase-boundaries");
    fs::write(out.join("boundaries.json"),format!("{{\"types\":{types},\"analyze\":{analyze},\"verify\":{verify},\"n\":\"Success\",\"n_minus_one\":\"ResourceLimit\",\"fresh_n\":\"Success\"}}\n")).unwrap();
}
