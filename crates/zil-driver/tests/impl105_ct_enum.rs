//! Actual source, untrusted CFG/wire controls and native CT-enum consumers.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{
    FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile, json_string,
};
use zil_ct::CompileTimeValue;
use zil_driver::{
    CompileOptions, CompileReport, Optimization, Stage, compile_path, compile_source,
};
use zil_ir::{Constant, CtProgram, InstructionKind, Type};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Case(PathBuf, bool);
impl Case {
    fn new(label: &str) -> Self {
        let evidence = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let root = evidence
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        fs::create_dir_all(&root).unwrap();
        let path = root.join(format!(
            "enum105-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path, evidence.is_some())
    }
    fn report(&self, report: &CompileReport) {
        fs::write(self.0.join("compile.json"), report.to_json()).unwrap();
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "enum105.zil", text)
}
fn check(text: &str) -> CompileReport {
    let case = Case::new("source");
    fs::write(case.0.join("source.zil"), text).unwrap();
    let report = compile_source(
        &source(text),
        &CompileOptions {
            stop_after: Stage::Lower,
            emit_dir: Some(case.0.join("phases")),
            ..CompileOptions::default()
        },
    );
    case.report(&report);
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
fn reject(text: &str, category: ResultCategory, phase: Phase, code: &str) {
    let report = check(text);
    assert_eq!(report.status, category, "{}", report.to_json());
    assert_eq!(report.phase, phase, "{}", report.to_json());
    assert_eq!(report.diagnostics[0].code, code, "{}", report.to_json());
    assert!(!report.artifacts.contains_key("executable"));
}
fn execute(case: &Case, path: &Path, input: &[u8], output: &[u8], code: i32) {
    let prefix = case
        .0
        .join(format!("run-{}", NEXT.fetch_add(1, Ordering::Relaxed)));
    fs::write(
        prefix.with_extension("command.json"),
        format!(
            "{{\"command\":[{}],\"expected_exit\":{code}}}",
            json_string(&path.to_string_lossy())
        ),
    )
    .unwrap();
    fs::write(prefix.with_extension("stdin"), input).unwrap();
    fs::write(prefix.with_extension("expected.stdout"), output).unwrap();
    let mut child = Command::new(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let result = child.wait_with_output().unwrap();
    fs::write(prefix.with_extension("stdout"), &result.stdout).unwrap();
    fs::write(prefix.with_extension("stderr"), &result.stderr).unwrap();
    fs::write(
        prefix.with_extension("status"),
        format!("{:?}\n", result.status),
    )
    .unwrap();
    assert_eq!(result.status.code(), Some(code), "{result:?}");
    assert_eq!(result.stdout, output);
    assert!(result.stderr.is_empty(), "{result:?}");
}
fn native(text: &str, expected: i64) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new("native");
        fs::write(case.0.join("source.zil"), text).unwrap();
        let report = compile_source(
            &source(text),
            &CompileOptions {
                stop_after: Stage::Build,
                entry: Some("value".into()),
                output: Some(case.0.join("program")),
                emit_dir: Some(case.0.join("phases")),
                optimization,
                ..CompileOptions::default()
            },
        );
        case.report(&report);
        success(&report);
        assert!(
            !fs::read_to_string(&report.artifacts["ct-core"])
                .unwrap()
                .contains("CompileTime")
        );
        execute(
            &case,
            &report.artifacts["executable"],
            &[],
            format!("{expected}\n").as_bytes(),
            0,
        );
    }
}
fn raw(text: &str) -> (Case, CtProgram) {
    let case = Case::new("raw");
    fs::write(case.0.join("source.zil"), text).unwrap();
    let input = source(text);
    let limits = Limits::default();
    let target = TargetProfile::default();
    let parsed = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &parsed).unwrap();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    let entry = module
        .functions
        .iter()
        .find(|f| f.name == "value")
        .unwrap()
        .id;
    fs::write(case.0.join("core.txt"), format!("{core:#?}")).unwrap();
    let raw = CtProgram {
        module,
        plan,
        entry,
        max_steps: limits.max_ct_steps,
        max_call_depth: limits.max_call_depth,
        max_live_slots: limits.max_values + limits.max_locals,
    };
    fs::write(case.0.join("program.txt"), format!("{raw:#?}")).unwrap();
    fs::write(
        case.0.join("program.bin"),
        zil_ir::wire::encode_ct_program(&raw, &limits).unwrap(),
    )
    .unwrap();
    (case, raw)
}
fn evaluated(raw: CtProgram) -> zil_base::CompileResult<zil_ct::ProgramEvaluation> {
    let checked =
        zil_verify::verify_ct_program(raw, &TargetProfile::default(), &Limits::default())?;
    zil_ct::evaluate_program(&checked)
}
fn reject_raw(case: &Case, name: &str, mut raw: CtProgram, code: &str) {
    raw.plan.module_fingerprint = raw.module.fingerprint();
    let limits = Limits::default();
    let target = TargetProfile::default();
    fs::write(case.0.join(format!("{name}.txt")), format!("{raw:#?}")).unwrap();
    let bytes = zil_ir::wire::encode_ct_program(&raw, &limits).unwrap();
    fs::write(case.0.join(format!("{name}.bin")), &bytes).unwrap();
    let direct = zil_verify::verify_ct_program(raw, &target, &limits).unwrap_err();
    let wire = zil_verify::verify_ct_program_bytes(&bytes, &target, &limits).unwrap_err();
    fs::write(
        case.0.join(format!("{name}.diagnostics.txt")),
        format!("direct={direct:#?}\nwire={wire:#?}"),
    )
    .unwrap();
    assert_eq!(direct[0].code, code, "{direct:?}");
    assert_eq!(wire[0].code, code, "{wire:?}");
}

#[test]
fn direct_and_helper_queries_reify_unit_and_payload_variants() {
    native(
        "enum M<T>{None,Some(T)}type Int=M<i64>;fn make()->Int{return Int.Some(42)}fn value()->i64{let x=#(make());match x{Int.None()=>{return 0},Int.Some(n)=>{return n}}}",
        42,
    );
    native(
        "enum E{U(void),N(i64)}fn value()->i64{let x=#(E.U(()));match x{E.U(_)=>{return 42},E.N(n)=>{return n}}}",
        42,
    );
    native(
        "enum E{Empty,Number(i64)}fn make()->E{return E.Number(42)}fn value()->i64{let x=#(make());match x{E.Empty()=>{return 0},E.Number(n)=>{return n}}}",
        42,
    );
    native(
        "enum E{Empty,Number(i64)}fn value()->i64{let x=#(E.Number(42));let empty=#(E.Empty);match empty{E.Empty()=>{},E.Number(_)=>{return 0}}match x{E.Empty()=>{return 0},E.Number(n)=>{return n}}}",
        42,
    );
    native(
        "enum E{Unit(void),Pair(i64,bool)}fn inspect(x:E)->i64{match x{E.Unit(_)=>{return 0},E.Pair(n,flag)=>{if flag{return n}return 1}}}fn value()->i64{return #(inspect(E.Pair(42,true)))}",
        42,
    );
}

#[test]
fn nominal_reification_is_hygienic_in_runtime_scopes_and_closures() {
    native(
        "enum E{N(i64)}fn make()->E{return E.N(42)}fn value()->i64{let E:i64=7;let x=#(make());match x{E.N(n)=>{return n+E-7}}}",
        42,
    );
    native(
        "enum E{N(i64)}fn make()->E{return E.N(42)}fn value()->i64{let f=()=>{let E:i64=7;let x=#(make());match x{E.N(n)=>{return n+E-7}}};return f()}",
        42,
    );
}

#[test]
fn nested_record_array_and_enum_values_keep_active_tags() {
    native(
        "enum Inner{Empty,N(i64)}struct Row{tag:Inner;values:[Inner;2];}enum Outer{Empty,Row(Row)}fn make()->Outer{var a=[Inner.Empty(),Inner.Empty()];a[1]=Inner.N(42);return Outer.Row(Row{tag:Inner.Empty(),values:a})}fn value()->i64{let x=#(make());match x{Outer.Empty()=>{return 0},Outer.Row(row)=>{match row.tag{Inner.Empty()=>{},Inner.N(_)=>{return 1}}match row.values[1]{Inner.Empty()=>{return 2},Inner.N(n)=>{return n}}}}}",
        42,
    );
    native(
        "enum E{A(i64),B(i64)}fn choose()->E{var x=E.A(1);for n in 0..4{x=E.B(i64(n)+39)}return x}fn value()->i64{let x=#(choose());match x{E.A(n)=>{return n},E.B(n)=>{return n}}}",
        42,
    );
}

#[test]
fn source_capability_and_payload_controls_remain_rejections() {
    reject(
        "enum E{N(i64)}fn make()->E{return E.N(true)}fn value()->E{return #(make())}",
        ResultCategory::SafeReject,
        Phase::Ct,
        "E-TYPE-MISMATCH",
    );
    reject(
        "enum E{N(i64)}fn value()->E{let x:i64=42;return #(E.N(x))}",
        ResultCategory::SafeReject,
        Phase::Ct,
        "E-CT-RUNTIME",
    );
    reject(
        "enum E{Empty,Handle(c_handle)}fn make()->E{return E.Empty()}fn value()->E{return #(make())}",
        ResultCategory::SafeReject,
        Phase::Ct,
        "E-CT-FORBIDDEN",
    );
    reject(
        "extern \"C\" fn opaque()->i64;enum E{N(i64)}fn make()->E{unsafe{return E.N(opaque())}}fn value()->E{return #(make())}",
        ResultCategory::SafeReject,
        Phase::Ct,
        "E-CT-FORBIDDEN",
    );
}

#[test]
fn raw_and_wire_reconstruct_tags_payloads_initialization_and_capabilities() {
    let (case, original) = raw("enum E{A(i64),B(i64)}fn value()->E{return E.A(42)}");
    let limits = Limits::default();
    let target = TargetProfile::default();
    let bytes = zil_ir::wire::encode_ct_program(&original, &limits).unwrap();
    assert_eq!(
        zil_ir::wire::decode_ct_program(&bytes, &limits).unwrap(),
        original
    );
    let expected = CompileTimeValue::Variant {
        variant: 0,
        fields: vec![CompileTimeValue::Scalar(Constant::I64(42))],
    };
    assert_eq!(evaluated(original.clone()).unwrap().value, expected);
    let checked = zil_verify::verify_ct_program_bytes(&bytes, &target, &limits).unwrap();
    assert_eq!(zil_ct::evaluate_program(&checked).unwrap().value, expected);
    for (name, kind) in [
        ("invalid-tag", 0),
        ("missing-payload", 1),
        ("wrong-payload-type", 2),
        ("undefined-payload", 3),
    ] {
        let mut bad = original.clone();
        let function = &mut bad.module.functions[bad.entry.0 as usize];
        let instruction = function
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find(|i| matches!(i.kind, InstructionKind::MakeVariant { .. }))
            .unwrap();
        let InstructionKind::MakeVariant { variant, fields } = &mut instruction.kind else {
            unreachable!()
        };
        match kind {
            0 => *variant = 99,
            1 => fields.clear(),
            2 => {
                let value = fields[0];
                function.values[value.0 as usize] = Type::Bool;
            }
            _ => fields[0] = zil_ir::ValueId(999),
        }
        let code = if kind == 2 {
            "E-VERIFY-TYPE"
        } else if kind == 3 {
            "E-VERIFY-ID"
        } else {
            "E-VERIFY-ENUM"
        };
        reject_raw(&case, name, bad, code);
    }
    let mut early = original.clone();
    let f = &mut early.module.functions[early.entry.0 as usize];
    let block = f
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, InstructionKind::MakeVariant { .. }))
        })
        .unwrap();
    let constructor = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::MakeVariant { .. }))
        .unwrap();
    let constant = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::Constant(_)))
        .unwrap();
    block.instructions.swap(constant, constructor);
    reject_raw(
        &case,
        "payload-before-definition",
        early,
        "E-VERIFY-DOMINANCE",
    );
    let mut bad = bytes;
    bad.push(0);
    fs::write(case.0.join("trailing-wire.bin"), &bad).unwrap();
    assert!(zil_verify::verify_ct_program_bytes(&bad, &target, &limits).is_err());
    let (hidden_case, hidden) =
        raw("enum E{Empty,Handle(c_handle)}fn value()->E{return E.Empty()}");
    assert!(
        zil_verify::verify_module(hidden.module.clone(), hidden.plan.clone(), &target, &limits)
            .is_ok()
    );
    reject_raw(
        &hidden_case,
        "inactive-capability",
        hidden,
        "E-CT-FORBIDDEN",
    );
}

#[test]
fn reified_nominal_owner_is_checked_again_by_ordinary_types() {
    struct Producer(CtProgram);
    impl zil_ct::SourceCompiler for Producer {
        fn compile(
            &self,
            _: &zil_ir::CoreExpr,
            _: Option<&Type>,
            _: &Limits,
        ) -> zil_base::CompileResult<CtProgram> {
            Ok(self.0.clone())
        }
    }
    let (case, original) = raw("enum E{N(i64)}fn value()->E{return E.N(42)}");
    let input = source("enum E{N(i64)}fn seed()->E{return E.N(42)}fn value()->E{return #(seed())}");
    let parsed = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &parsed).unwrap();
    for (owner, forged_payload) in [
        ("@module0:E", false),
        ("@module99:E", false),
        ("@module0:Missing", false),
        ("@module0:E", true),
    ] {
        let mut candidate = original.clone();
        for f in &mut candidate.module.functions {
            for ty in std::iter::once(&mut f.return_type)
                .chain(f.values.iter_mut())
                .chain(f.locals.iter_mut().map(|l| &mut l.ty))
            {
                if let Type::Enum { name, variants } = ty {
                    *name = owner.to_owned();
                    if forged_payload {
                        variants[0].1[0] = Type::Bool;
                    }
                } else if forged_payload && *ty == Type::I64 {
                    *ty = Type::Bool;
                }
            }
            if forged_payload {
                for instruction in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
                    if matches!(
                        instruction.kind,
                        InstructionKind::Constant(Constant::I64(_))
                    ) {
                        instruction.kind = InstructionKind::Constant(Constant::Bool(true));
                    }
                }
            }
        }
        candidate.plan.module_fingerprint = candidate.module.fingerprint();
        let label = if forged_payload {
            "forged-payload".to_owned()
        } else {
            owner.replace(['@', ':'], "_")
        };
        let bytes = zil_ir::wire::encode_ct_program(&candidate, &Limits::default()).unwrap();
        fs::write(case.0.join(format!("{label}.program.bin")), &bytes).unwrap();
        let candidate = zil_ir::wire::decode_ct_program(&bytes, &Limits::default()).unwrap();
        // Pure raw data may carry a nominal label; it cannot authorize a new
        // declaration or a constructor in the source module that consumes it.
        let (materialized, _) = zil_ct::evaluate_source_constants_with_compiler(
            &core,
            &TargetProfile::default(),
            &Limits::default(),
            None,
            &Producer(candidate),
        )
        .unwrap();
        let result = zil_types::check(&materialized, &TargetProfile::default(), &Limits::default());
        fs::write(
            case.0.join(format!("{label}.core.txt")),
            format!("{materialized:#?}"),
        )
        .unwrap();
        fs::write(
            case.0.join(format!("{label}.types.txt")),
            format!("{result:#?}"),
        )
        .unwrap();
        if owner == "@module0:E" && !forged_payload {
            assert!(result.is_ok(), "{result:?}");
        } else {
            let errors = result.unwrap_err();
            assert_eq!(errors[0].category, ResultCategory::SafeReject);
            assert_eq!(errors[0].phase, Phase::Types);
            assert_eq!(
                errors[0].code,
                if forged_payload {
                    "E-TYPE-MISMATCH"
                } else {
                    "E-UNKNOWN-NAME"
                }
            );
        }
    }
}

#[test]
fn same_spelling_in_distinct_modules_preserves_nominal_identity() {
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new("same-name");
        fs::write(
            case.0.join("left.zil"),
            "pub enum E{N(i64)}pub fn make()->E{return E.N(42)}",
        )
        .unwrap();
        fs::write(
            case.0.join("right.zil"),
            "pub enum E{N(i64)}pub fn make()->E{return E.N(7)}",
        )
        .unwrap();
        let main = "import left\nimport right\nfn value()->i64{let a=#(left.make());let b=#(right.E.N(7));match a{left.E.N(n)=>{match b{right.E.N(m)=>{return n+m}}}}}";
        fs::write(case.0.join("main.zil"), main).unwrap();
        let report = compile_path(
            &case.0.join("main.zil"),
            &CompileOptions {
                stop_after: Stage::Build,
                entry: Some("value".into()),
                output: Some(case.0.join("program")),
                emit_dir: Some(case.0.join("phases")),
                optimization,
                ..CompileOptions::default()
            },
        );
        case.report(&report);
        success(&report);
        execute(&case, &report.artifacts["executable"], &[], b"49\n", 0);
    }
    let case = Case::new("wrong-module");
    fs::write(case.0.join("left.zil"), "pub enum E{N(i64)}").unwrap();
    fs::write(
        case.0.join("right.zil"),
        "pub enum E{N(i64)}pub fn make()->E{return E.N(7)}",
    )
    .unwrap();
    fs::write(
        case.0.join("main.zil"),
        "import left\nimport right\nfn value(){let a:left.E=#(right.make())}",
    )
    .unwrap();
    let report = compile_path(
        &case.0.join("main.zil"),
        &CompileOptions {
            stop_after: Stage::Lower,
            emit_dir: Some(case.0.join("phases")),
            ..CompileOptions::default()
        },
    );
    case.report(&report);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Ct);
    assert_eq!(report.diagnostics[0].code, "E-TYPE-MISMATCH");
}

#[test]
fn repeated_enum_queries_share_the_actual_execution_budget() {
    let text = "enum E{N(i64)}fn make()->E{return E.N(21)}fn value()->i64{let a=#(make());let b=#(make());match a{E.N(n)=>{match b{E.N(m)=>{return n+m}}}}}";
    let case = Case::new("shared-budget");
    fs::write(case.0.join("source.zil"), text).unwrap();
    let probe = |n| {
        let mut options = CompileOptions {
            stop_after: Stage::Lower,
            ..CompileOptions::default()
        };
        options.limits.max_ct_steps = n;
        let report = compile_source(&source(text), &options);
        fs::write(case.0.join(format!("steps-{n}.json")), report.to_json()).unwrap();
        report
    };
    let mut low = 0;
    let mut high = Limits::default().max_ct_steps;
    success(&probe(high));
    while low + 1 < high {
        let middle = low + (high - low) / 2;
        let report = probe(middle);
        if report.status == ResultCategory::Success {
            high = middle
        } else {
            assert_eq!(
                report.status,
                ResultCategory::ResourceLimit,
                "{}",
                report.to_json()
            );
            low = middle;
        }
    }
    success(&probe(high));
    let rejected = probe(high - 1);
    assert_eq!(rejected.status, ResultCategory::ResourceLimit);
    assert_eq!(rejected.phase, Phase::Ct);
    fs::write(
        case.0.join("boundary.json"),
        format!("{{\"steps\":{high},\"at_N\":\"Success\",\"at_N_minus_1\":\"ResourceLimit\"}}"),
    )
    .unwrap();
}

#[test]
fn well_formed_wrong_active_projection_traps_before_reading_payload() {
    let (case, mut raw) = raw(
        "enum E{A(i64),B(i64)}fn value()->i64{let x=E.A(42);match x{E.A(n)=>{return n},E.B(n)=>{return n+1}}}",
    );
    for (name, kind) in [
        ("invalid-read-tag", 0),
        ("invalid-read-index", 1),
        ("tag-of-scalar", 2),
    ] {
        let mut bad = raw.clone();
        let f = &mut bad.module.functions[bad.entry.0 as usize];
        if kind == 2 {
            let scalar = f
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .find(|i| matches!(i.kind, InstructionKind::Constant(Constant::I64(_))))
                .unwrap()
                .result
                .unwrap();
            let instruction = f
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| matches!(i.kind, InstructionKind::VariantTag { .. }))
                .unwrap();
            let InstructionKind::VariantTag { value } = &mut instruction.kind else {
                unreachable!()
            };
            *value = scalar;
        } else {
            let instruction = f
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| matches!(i.kind, InstructionKind::ReadVariant { .. }))
                .unwrap();
            let InstructionKind::ReadVariant { variant, index, .. } = &mut instruction.kind else {
                unreachable!()
            };
            if kind == 0 {
                *variant = 99;
            } else {
                *index = 99;
            }
        }
        reject_raw(&case, name, bad, "E-VERIFY-ENUM");
    }
    let f = &mut raw.module.functions[raw.entry.0 as usize];
    let i = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::ReadVariant { variant: 0, .. }))
        .unwrap();
    let InstructionKind::ReadVariant { variant, .. } = &mut i.kind else {
        unreachable!()
    };
    *variant = 1;
    raw.plan.module_fingerprint = raw.module.fingerprint();
    let bytes = zil_ir::wire::encode_ct_program(&raw, &Limits::default()).unwrap();
    fs::write(case.0.join("wrong-active.bin"), &bytes).unwrap();
    let direct = evaluated(raw).unwrap_err();
    let checked =
        zil_verify::verify_ct_program_bytes(&bytes, &TargetProfile::default(), &Limits::default())
            .unwrap();
    let wire = zil_ct::evaluate_program(&checked).unwrap_err();
    fs::write(
        case.0.join("wrong-active.diagnostics.txt"),
        format!("{direct:#?}\n{wire:#?}"),
    )
    .unwrap();
    assert_eq!(direct[0].code, "E-CT-TRAP");
    assert_eq!(wire[0].code, "E-CT-TRAP");
}

#[test]
fn vm_counts_full_enum_shapes_and_exact_steps_cells_and_call_depth() {
    let (case, original) =
        raw("enum E{A(i64),B(i64)}fn make()->E{return E.A(42)}fn value()->E{return make()}");
    let probe = |name: &str, candidate: CtProgram| {
        let bytes = zil_ir::wire::encode_ct_program(&candidate, &Limits::default()).unwrap();
        fs::write(case.0.join(format!("{name}.bin")), &bytes).unwrap();
        let direct = evaluated(candidate);
        let wire = zil_verify::verify_ct_program_bytes(
            &bytes,
            &TargetProfile::default(),
            &Limits::default(),
        )
        .and_then(|verified| zil_ct::evaluate_program(&verified));
        fs::write(
            case.0.join(format!("{name}.results.txt")),
            format!("direct={direct:#?}\nwire={wire:#?}"),
        )
        .unwrap();
        assert_eq!(direct, wire);
        direct
    };
    let full = evaluated(original.clone()).unwrap();
    let mut boundary = original.clone();
    boundary.max_steps = full.steps;
    assert_eq!(probe("steps-N", boundary.clone()).unwrap(), full);
    boundary.max_steps -= 1;
    assert_eq!(
        probe("steps-N-1", boundary).unwrap_err()[0].category,
        ResultCategory::ResourceLimit
    );
    let mut boundary = original.clone();
    boundary.max_live_slots = full.peak_live_slots;
    assert_eq!(probe("cells-N", boundary.clone()).unwrap(), full);
    boundary.max_live_slots -= 1;
    assert_eq!(
        probe("cells-N-1", boundary).unwrap_err()[0].category,
        ResultCategory::ResourceLimit
    );
    let mut boundary = original;
    boundary.max_call_depth = full.peak_call_depth;
    assert_eq!(probe("depth-N", boundary.clone()).unwrap(), full);
    boundary.max_call_depth -= 1;
    assert_eq!(
        probe("depth-N-1", boundary).unwrap_err()[0].category,
        ResultCategory::ResourceLimit
    );
    fs::write(case.0.join("boundaries.json"),format!("{{\"steps\":{},\"cells\":{},\"call_depth\":{},\"at_N\":\"Success\",\"at_N_minus_1\":\"ResourceLimit\"}}",full.steps,full.peak_live_slots,full.peak_call_depth)).unwrap();
    let (wide_case, mut wide) =
        raw("enum E{Empty,Left([i64;32]),Right([i64;32])}fn value()->E{return E.Empty()}");
    let full = evaluated(wide.clone()).unwrap();
    assert!(full.peak_live_slots >= 136);
    fs::write(wide_case.0.join("full.txt"), format!("{full:#?}")).unwrap();
    wide.max_live_slots = 70;
    assert_eq!(
        evaluated(wide).unwrap_err()[0].category,
        ResultCategory::ResourceLimit
    );
}

#[test]
fn deep_and_wide_enum_metadata_is_bounded_on_raw_and_wire_entry() {
    let variants = (0..300)
        .map(|i| format!("V{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let wide = format!("enum E{{{variants}}}fn value()->E{{return E.V0()}}");
    let mut deep = "enum E0{V(i64)}".to_owned();
    for i in 1..33 {
        deep.push_str(&format!("enum E{i}{{V(E{})}}", i - 1));
    }
    let value = (0..33).fold("42".to_owned(), |body, i| format!("E{i}.V({body})"));
    deep.push_str(&format!("fn value()->E32{{return {value}}}"));
    for (name, text) in [("wide", wide), ("deep", deep)] {
        let (case, original) = raw(&text);
        assert!(evaluated(original.clone()).is_ok());
        for bound in if name == "wide" {
            vec!["nodes", "work"]
        } else {
            vec!["depth"]
        } {
            let mut limits = Limits::default();
            match bound {
                "nodes" => limits.max_nodes = 64,
                "work" => limits.max_analysis_steps = 100,
                _ => limits.max_call_depth = 8,
            }
            let mut raw = original.clone();
            raw.max_call_depth = limits.max_call_depth;
            let bytes = zil_ir::wire::encode_ct_program(&raw, &Limits::default()).unwrap();
            fs::write(case.0.join(format!("{bound}.bin")), &bytes).unwrap();
            let direct = zil_verify::verify_ct_program(raw, &TargetProfile::default(), &limits);
            let wire =
                zil_verify::verify_ct_program_bytes(&bytes, &TargetProfile::default(), &limits)
                    .unwrap_err();
            fs::write(
                case.0.join(format!("{bound}.diagnostics.txt")),
                format!("{direct:#?}\n{wire:#?}"),
            )
            .unwrap();
            if bound == "nodes" {
                // max_nodes bounds Core and decoding; already-decoded typed
                // Verify uses the shared work/depth quotas tested below.
                assert!(direct.is_ok(), "{direct:?}");
                let input = source(&text);
                let syntax = zil_syntax::parse(&input).into_result().unwrap();
                let core = zil_expand::expand(&input, &syntax).unwrap();
                let errors =
                    zil_ct::evaluate_source_constants(&core, &TargetProfile::default(), &limits)
                        .unwrap_err();
                fs::write(
                    case.0.join("nodes.core.diagnostics.txt"),
                    format!("{errors:#?}"),
                )
                .unwrap();
                assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
                assert_eq!(errors[0].phase, Phase::Ct);
            } else {
                let direct = direct.unwrap_err();
                assert_eq!(
                    direct[0].category,
                    ResultCategory::ResourceLimit,
                    "{name} {bound}: {direct:?}"
                );
            }
            assert_eq!(
                wire[0].category,
                ResultCategory::ResourceLimit,
                "{name} {bound}: {wire:?}"
            );
        }
    }
}

#[test]
fn multi_module_plan_uses_real_dynamic_inputs_at_o0_and_o2() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/ct_enum");
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new("consumer");
        for name in ["main", "model", "transform"] {
            fs::copy(
                root.join(format!("{name}.zil")),
                case.0.join(format!("{name}.zil")),
            )
            .unwrap();
        }
        let report = compile_path(
            &case.0.join("main.zil"),
            &CompileOptions {
                stop_after: Stage::Build,
                output: Some(case.0.join("program")),
                emit_dir: Some(case.0.join("phases")),
                optimization,
                ..CompileOptions::default()
            },
        );
        case.report(&report);
        success(&report);
        for byte in [0u8, 1, 7, 8, 127, 128, 255] {
            let extra = if byte % 2 == 0 { 3 } else { 40 };
            execute(
                &case,
                &report.artifacts["executable"],
                &[byte],
                &[((u16::from(byte) + extra) % 256) as u8],
                0,
            );
        }
        execute(&case, &report.artifacts["executable"], &[], &[], 1);
    }
}
