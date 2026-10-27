//! Component preparation helpers compose with self-contained returned handlers.
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{
    FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile, json_string,
};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
};
use zil_ir::{CallableKind, InstructionKind};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Case(PathBuf, bool);
impl Case {
    fn new(name: &str) -> Self {
        let keep = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let root = keep
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        fs::create_dir_all(&root).unwrap();
        let path = root.join(format!(
            "factory125-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path, keep.is_some())
    }
    fn write(&self, name: &str, data: impl AsRef<[u8]>) {
        fs::write(self.0.join(name), data).unwrap();
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

fn project(files: &[(&str, &str)]) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: files
            .iter()
            .enumerate()
            .map(|(i, (name, text))| {
                SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    format!("{name}.zil"),
                    *text,
                )
            })
            .collect(),
        paths: files
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, (name, _))| (vec![name.to_string()], ModuleId(i as u32)))
            .collect::<BTreeMap<_, _>>(),
    }
}
fn compile(
    input: &ProjectSources,
    case: &Case,
    optimization: Option<Optimization>,
) -> CompileReport {
    let native_main = input
        .modules
        .iter()
        .find(|unit| unit.module_id == input.root)
        .unwrap()
        .text
        .contains("fn main(");
    for unit in &input.modules {
        case.write(&format!("module{}.zil", unit.module_id.0), &unit.text);
    }
    let report = compile_project(
        input,
        &CompileOptions {
            stop_after: if optimization.is_some() {
                Stage::Build
            } else {
                Stage::Verify
            },
            optimization: optimization.unwrap_or(Optimization::O0),
            entry: if native_main {
                None
            } else {
                Some("value".into())
            },
            output: optimization.map(|_| case.0.join("program")),
            emit_dir: Some(case.0.join("phases")),
            ..CompileOptions::default()
        },
    );
    case.write("compile.json", report.to_json());
    report
}
fn success(report: &CompileReport) {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1, "{}", report.to_json());
}
fn native(input: &ProjectSources, label: &str, expected: &str, code: i32) {
    native_checked(input, label, expected, code, |report| {
        let factory = report
            .scalar_interfaces
            .iter()
            .find(|i| i.module == ModuleId(2))
            .unwrap();
        assert_eq!(factory.callable_returns.as_ref().unwrap().len(), 1);
        let helper = report
            .scalar_interfaces
            .iter()
            .find(|i| i.module == ModuleId(1))
            .unwrap();
        assert!(!helper.callable_contracts.as_ref().unwrap().is_empty());
    });
}

fn native_checked(
    input: &ProjectSources,
    label: &str,
    expected: &str,
    code: i32,
    check: impl Fn(&CompileReport),
) {
    for opt in [Optimization::O0, Optimization::O2] {
        let case = Case::new(&format!("{label}-{opt:?}"));
        let report = compile(input, &case, Some(opt));
        success(&report);
        let path = &report.artifacts["executable"];
        case.write(
            "native-command.json",
            format!("[{}]\n", json_string(&path.to_string_lossy())),
        );
        let output = Command::new(path).output().unwrap();
        case.write("native.stdout", &output.stdout);
        case.write("native.stderr", &output.stderr);
        case.write("native.exit", format!("{:?}\n", output.status.code()));
        assert_eq!(output.status.code(), Some(code), "{label}: {output:?}");
        assert_eq!(output.stdout, expected.as_bytes(), "{label}: {output:?}");
        assert!(output.stderr.is_empty(), "{label}: {output:?}");
        check(&report);
    }
}

const HELPERS: &str = "pub fn apply<F:Fn(i64)->i64>(f:&F,n:i64)->i64{return f(n)}\n pub fn twice<F:FnMut(i64)->i64>(f:&mut F)->i64{return f(1)+f(2)}\n pub fn once<F:FnOnce(i64)->i64>(f:F,n:i64)->i64{return f(n)}\n pub fn once_error<F:FnOnce()->bool!i64>(f:F)->bool!i64{return f()}";
const IO: &str = "extern \"C\" fn putchar(n:i32)->i32;pub fn emit(n:i32){unsafe{putchar(n)}}";
const PREFIX: &str = "import helpers\nimport io\nresource Ticket{code:i32}\nimpl Drop for Ticket{fn drop(self:Self){io.emit(.code)}}\nfn consume(give ticket:Ticket){}\nresource Model{count:i64}\nimpl Drop for Model{fn drop(self:Self){io.emit(82)}}\n";
fn component(body: &str, main: &str) -> ProjectSources {
    project(&[
        ("main", main),
        ("helpers", HELPERS),
        ("factory", &format!("{PREFIX}{body}")),
        ("io", IO),
    ])
}

#[test]
fn shared_initialization_helper_then_owned_snapshot_handler() {
    let body = "pub fn make(seed:i64)->impl Fn()->i64{let ticket=Ticket{code:73};let init=move (n:i64)=>{let observed=ticket.code;return n+2};let prepared=helpers.apply(&init,seed);let model=Model{count:prepared};return move ()=>model.count}";
    native(
        &component(
            body,
            "import factory\nfn value()->i64{let handler=factory.make(40);return handler()}",
        ),
        "shared",
        "IR42\n",
        0,
    );
}

#[test]
fn mutable_preparation_then_retained_component_events() {
    let body = "pub fn make(seed:i64)->impl FnMut(i64)->i64{let ticket=Ticket{code:73};var count=seed;var init=move (n:i64)=>{let observed=ticket.code;count+=n;return count};let prepared=helpers.twice(&mut init);let model=Model{count:prepared};return move (delta:i64)=>{model.count+=delta;return model.count}}";
    native(
        &component(
            body,
            "import factory\nfn value()->i64{var handler=factory.make(10);let a=handler(2);return a+handler(3)}",
        ),
        "mutable",
        "IR55\n",
        0,
    );
}

#[test]
fn consuming_initialization_and_consumed_or_unused_returned_handler() {
    let body = "pub fn make(seed:i64)->impl FnOnce()->i64{let ticket=Ticket{code:73};let init=move (n:i64)=>{consume(give ticket);return seed+n};let prepared=helpers.once(init,2);let held=Ticket{code:82};return move ()=>{consume(give held);return prepared}}";
    for (label, main, output) in [
        (
            "once-called",
            "import factory\nfn value()->i64{let handler=factory.make(40);return handler()}",
            "IR42\n",
        ),
        (
            "once-unused",
            "import factory\nfn value()->i64{let handler=factory.make(40);return 0}",
            "IR0\n",
        ),
    ] {
        native(&component(body, main), label, output, 0);
    }
}

#[test]
fn helper_warms_the_returned_handler_and_two_instances_remain_independent() {
    let body = "pub fn make(seed:i64)->impl FnMut(i64)->i64{let model=Model{count:seed};var handler=move (delta:i64)=>{model.count+=delta;return model.count};let warmed=helpers.twice(&mut handler);return handler}";
    native(
        &component(
            body,
            "import factory\nfn value()->i64{var first=factory.make(10);var second=factory.make(20);let a=first(2);let b=second(3);return a+b+first(1)}",
        ),
        "warm-independent",
        "RR57\n",
        0,
    );
}

#[test]
fn checked_in_component_prepares_warms_and_retains_two_owned_handlers() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/mainline/component_factory_helpers");
    let files: Vec<_> = ["main", "helpers", "factory", "io"]
        .into_iter()
        .map(|name| {
            (
                name,
                fs::read_to_string(root.join(format!("{name}.zil"))).unwrap(),
            )
        })
        .collect();
    let borrowed: Vec<_> = files
        .iter()
        .map(|(name, text)| (*name, text.as_str()))
        .collect();
    native(&project(&borrowed), "checked-in-component", "IIRR63\n", 0);
}

#[test]
fn initializer_error_value_and_returned_environment_cleanup_are_preserved() {
    let body = "pub fn make(bad:bool)->impl Fn()->bool!i64{let ticket=Ticket{code:73};let init=move ()->bool!i64=>{consume(give ticket);if bad{return true}let answer:i64=42;return answer};let prepared=helpers.once_error(init);let held=Ticket{code:82};return move ()->bool!i64=>{let observed=held.code;return prepared}}";
    native(
        &component(
            body,
            "import factory\nimport io\nfn main()->bool!i32{let handler=factory.make(false);let n=handler()?;if n!=42{return true}io.emit(83);return 0}",
        ),
        "prepared-ok",
        "ISR",
        0,
    );
    native(
        &component(
            body,
            "import factory\nfn main()->bool!i32{let handler=factory.make(true);let n=handler()?;return 0}",
        ),
        "prepared-error",
        "IR",
        1,
    );
}

#[test]
fn module_local_factory_and_helper_names_keep_definition_identity() {
    let helper = "pub fn apply<F:Fn(i64)->i64>(f:&F,n:i64)->i64{return f(n)}";
    let mut input = project(&[
        (
            "main",
            "import first\nimport second\nfn value()->i64{let a=first.make(20);let b=second.make(20);return a()+b()}",
        ),
        ("helpers", helper),
        (
            "first",
            "import helpers\npub fn make(seed:i64)->impl Fn()->i64{let init=(n:i64)=>n+1;let state=helpers.apply(&init,seed);return move ()=>state}",
        ),
        (
            "second",
            "import helpers\npub fn make(seed:i64)->impl Fn()->i64{let init=(n:i64)=>n+2;let state=helpers.apply(&init,seed);return move ()=>state}",
        ),
    ]);
    for order in ["forward", "reverse"] {
        let case = Case::new(order);
        let report = compile(&input, &case, None);
        success(&report);
        let mut ids = Vec::new();
        for interface in &report.scalar_interfaces {
            for contract in interface.callable_returns.as_ref().unwrap() {
                ids.push(contract.contract.closure);
            }
        }
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1]);
        input.modules.reverse();
    }
}

#[test]
fn helper_factory_dependencies_resolve_in_either_order_but_reject_cycles() {
    let helper = "fn apply<F:Fn()->i64>(f:&F)->i64{let dependent=other();return f()}";
    let make = "fn make()->impl Fn()->i64{let init=()=>42;let n=apply(&init);return move ()=>n}";
    let other = "fn other()->impl Fn()->i64{return move ()=>7}";
    for (label, source) in [
        (
            "other-before",
            format!("{other}{helper}{make}fn value()->i64{{return 0}}"),
        ),
        (
            "other-after",
            format!("{make}{helper}{other}fn value()->i64{{return 0}}"),
        ),
    ] {
        let case = Case::new(label);
        let report = compile(&project(&[("main", &source)]), &case, None);
        success(&report);
        // The public single-module Types entry uses the same dependency rule.
        let unit = SourceUnit::new(FileId(0), ModuleId(0), "order.zil", &source);
        let syntax = zil_syntax::parse(&unit).into_result().unwrap();
        let core = zil_expand::expand(&unit, &syntax).unwrap();
        let limits = Limits::default();
        let target = TargetProfile::default();
        let module = zil_types::check(&core, &target, &limits).unwrap();
        let plan = zil_analyze::analyze(&module, &limits).unwrap();
        zil_verify::verify_module(module, plan, &target, &limits).unwrap();
        let executable =
            format!("{source}fn main()->i32{{let f=make();if f()!=42{{return 1}}return 0}}");
        native_checked(&project(&[("main", &executable)]), label, "", 0, |_| {});
    }
    let recursive = "fn apply<F:Fn()->i64>(f:&F)->i64{let dependent=make();return f()}fn make()->impl Fn()->i64{let init=()=>42;let n=apply(&init);return move ()=>n}fn value()->i64{return 0}";
    let case = Case::new("indirect-recursive");
    let report = compile(&project(&[("main", recursive)]), &case, None);
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Types, "{}", report.to_json());
    assert!(
        report.diagnostics[0].message.contains("recursive"),
        "{}",
        report.to_json()
    );
    let ordinary = format!("{other}{helper}fn value()->i64{{let init=()=>42;return apply(&init)}}");
    let case = Case::new("ordinary-helper-after-prepass");
    success(&compile(&project(&[("main", &ordinary)]), &case, None));
}

#[test]
fn raw_core_helper_factory_dependencies_respect_parameter_shadowing() {
    // A false edge to the global `other` would make a spurious factory cycle.
    // Generic expansion has additional source restrictions for same-name
    // aliases. Exercise the public untrusted Core entry without widening them.
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "shadow-core.zil",
        "fn apply<F:Fn()->i64>(f:&F)->i64{return f()}fn make()->impl Fn()->i64{let init=()=>42;let n=apply(&init);return move ()=>n}fn other()->impl Fn()->i64{return move ()=>7}fn value()->i64{return 0}",
    );
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let mut core = zil_expand::expand(&source, &syntax).unwrap();
    let helper = core
        .functions
        .iter_mut()
        .find(|f| f.name == "apply")
        .unwrap();
    helper.parameters[0].name = "other".into();
    let zil_ir::CoreStatementKind::Return(Some(expression)) = &mut helper.body.statements[0].kind
    else {
        panic!("helper return")
    };
    let zil_ir::CoreExprKind::Call { callee, .. } = &mut expression.kind else {
        panic!("helper invocation")
    };
    callee.kind = zil_ir::CoreExprKind::Name("other".into());
    let other = core
        .functions
        .iter_mut()
        .find(|f| f.name == "other")
        .unwrap();
    let zil_ir::CoreStatementKind::Return(Some(expression)) = &mut other.body.statements[0].kind
    else {
        panic!("factory return")
    };
    expression.kind = zil_ir::CoreExprKind::Call {
        callee: Box::new(zil_ir::CoreExpr {
            id: zil_base::NodeId(u32::MAX),
            kind: zil_ir::CoreExprKind::Name("make".into()),
            span: expression.span,
        }),
        arguments: vec![],
    };
    let limits = Limits::default();
    let target = TargetProfile::default();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module, plan, &target, &limits).unwrap();
}

#[test]
fn helper_factory_dependencies_use_definition_module_and_imports() {
    let input = project(&[
        (
            "main",
            "import factory\nfn main()->i32{let f=factory.make();if f()!=42{return 1}return 0}",
        ),
        (
            "factory",
            "import helpers\npub fn make()->impl Fn()->i64{let init=()=>42;let n=helpers.apply(&init);return move ()=>n}fn other()->impl Fn()->i64{return move ()=>99}",
        ),
        (
            "helpers",
            "import dep\npub fn apply<F:Fn()->i64>(f:&F)->i64{let independent=other();let imported=dep.other();return f()}fn other()->impl Fn()->i64{return move ()=>7}",
        ),
        ("dep", "pub fn other()->impl Fn()->i64{return move ()=>7}"),
    ]);
    native_checked(&input, "helper-definition-context", "", 0, |_| {});
}

#[test]
fn invalid_escape_modes_and_factory_dependencies_stay_rejected() {
    let cases = [
        (
            "borrowed-result",
            "fn apply<F:Fn(i64)->i64>(f:&F,n:i64)->i64{return f(n)}fn make(n:i64)->impl Fn()->i64{let init=(x:i64)=>x;let state=apply(&init,n);return ()=>state}fn value()->i64{return 0}",
            ResultCategory::Unsupported,
            Phase::Types,
            "E-UNSUPPORTED",
        ),
        (
            "recursive",
            "fn make()->impl Fn()->i64{return make()}fn value()->i64{return 0}",
            ResultCategory::Unsupported,
            Phase::Types,
            "E-UNSUPPORTED",
        ),
        (
            "once-twice",
            "fn twice<F:FnOnce()->i64>(f:F)->i64{return f()+f()}fn make()->impl Fn()->i64{return move ()=>42}fn value()->i64{return 0}",
            ResultCategory::SafeReject,
            Phase::Expand,
            "E-GENERIC-ARGUMENT",
        ),
        (
            "wrong-kind",
            "fn apply<F:Fn()->i64>(f:&F)->i64{return f()}fn make()->impl Fn()->i64{var n:i64=0;var init=()=>{n+=1;return n};let state=apply(&init);return move ()=>state}fn value()->i64{return 0}",
            ResultCategory::SafeReject,
            Phase::Types,
            "E-CLOSURE-MODE",
        ),
    ];
    for (label, source, status, phase, code) in cases {
        let case = Case::new(label);
        let report = compile(&project(&[("main", source)]), &case, None);
        assert_eq!(report.status, status, "{}", report.to_json());
        assert_eq!(report.phase, phase, "{}", report.to_json());
        assert_eq!(report.diagnostics[0].code, code, "{}", report.to_json());
        assert!(!report.artifacts.contains_key("executable"));
    }
    // Schema19 accepts grounded concrete factory forwarding. Recursion and
    // borrowed escape above remain independently rejected.
    let forwarded = "fn inner()->impl Fn()->i64{return move ()=>42}fn make()->impl Fn()->i64{return inner()}fn value()->i64{return 0}";
    let case = Case::new("forwarded");
    success(&compile(&project(&[("main", forwarded)]), &case, None));
}

const RAW: &str = "resource R{n:i64}fn apply<F:Fn(i64)->i64>(f:&F,n:i64)->i64{return f(n)}fn make(n:i64)->impl Fn()->i64{let init=(x:i64)=>x+2;let state=apply(&init,n);let held=R{n:state};return move ()=>held.n}fn value()->i64{let f=make(40);return f()}";
fn core() -> zil_ir::CoreModule {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "raw125.zil", RAW);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    zil_expand::expand(&source, &ast).unwrap()
}

#[test]
fn independent_raw_and_wire_checks_reject_both_helper_and_factory_forgery() {
    let limits = Limits::default();
    let target = TargetProfile::default();
    let module = zil_types::check(&core(), &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    let factory = module
        .functions
        .iter()
        .find(|f| f.callable_return_contract.is_some())
        .unwrap();
    let helper = module
        .functions
        .iter()
        .find(|f| f.callable_contract.is_some())
        .unwrap();
    assert!(
        factory
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind,InstructionKind::Call{function,..} if function==helper.id))
    );
    let original = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    zil_verify::verify_module_bytes(&original, &target, &limits).unwrap();
    let case = Case::new("raw-wire");
    case.write("source.zil", RAW);
    case.write("original.wire", original);
    case.write("original-typed.txt", format!("{module:#?}"));
    case.write("original-plan.txt", format!("{plan:#?}"));
    for mutation in ["factory-cleanup", "helper-trace", "factory-kind"] {
        let mut changed = module.clone();
        match mutation {
            "factory-cleanup" => changed
                .functions
                .iter_mut()
                .find_map(|f| f.callable_return_contract.as_mut())
                .unwrap()
                .cleanup
                .clear(),
            "helper-trace" => changed
                .functions
                .iter_mut()
                .find_map(|f| f.callable_contract.as_mut())
                .unwrap()
                .traces
                .clear(),
            "factory-kind" => {
                changed
                    .functions
                    .iter_mut()
                    .find_map(|f| f.callable_return_contract.as_mut())
                    .unwrap()
                    .kind = CallableKind::FnOnce
            }
            _ => unreachable!(),
        }
        let mut candidate = plan.clone();
        candidate.module_fingerprint = changed.fingerprint();
        let bytes = zil_ir::wire::encode_module_candidate(&changed, &candidate, &limits).unwrap();
        case.write(&format!("{mutation}.wire"), &bytes);
        case.write(&format!("{mutation}-typed.txt"), format!("{changed:#?}"));
        let raw = zil_verify::verify_module(changed, candidate, &target, &limits).unwrap_err();
        let wire = zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err();
        for errors in [&raw, &wire] {
            assert_eq!(errors[0].phase, Phase::Verify);
            assert_eq!(errors[0].category, ResultCategory::SafeReject);
            assert_eq!(errors[0].code, "E-VERIFY-CALLABLE-CONTRACT", "{errors:?}");
        }
        case.write(
            &format!("{mutation}-diagnostics.txt"),
            format!("raw: {raw:#?}\nwire: {wire:#?}"),
        );
    }
}

#[test]
fn new_composition_still_pays_types_and_verify_budgets() {
    let input = core();
    let defaults = Limits::default();
    let target = TargetProfile::default();
    let module = zil_types::check(&input, &target, &defaults).unwrap();
    let plan = zil_analyze::analyze(&module, &defaults).unwrap();
    let case = Case::new("budget");
    case.write("source.zil", RAW);
    let mut records = Vec::new();
    for phase in [Phase::Types, Phase::Verify] {
        let succeeds = |steps| {
            let limits = Limits {
                max_analysis_steps: steps,
                ..defaults.clone()
            };
            match phase {
                Phase::Types => zil_types::check(&input, &target, &limits)
                    .map(|_| ())
                    .map_err(|e| e[0].category),
                Phase::Verify => {
                    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits)
                        .map(|_| ())
                        .map_err(|e| e[0].category)
                }
                _ => unreachable!(),
            }
        };
        let (mut low, mut high) = (0, defaults.max_analysis_steps);
        while low < high {
            let mid = low + (high - low) / 2;
            if succeeds(mid).is_ok() {
                high = mid
            } else {
                low = mid + 1
            }
        }
        assert!(low > 0);
        assert!(succeeds(low).is_ok());
        assert_eq!(succeeds(low - 1), Err(ResultCategory::ResourceLimit));
        records.push(format!("{{\"phase\":\"{phase:?}\",\"N\":{low},\"N_minus_1\":{},\"status_at_N\":\"Pass\",\"status_at_N_minus_1\":\"ResourceLimit\"}}",low-1));
    }
    case.write("boundaries.json", format!("[{}]\n", records.join(",")));
}
