//! Source-derived Analyze responsibility checks. Raw mutations retain the
//! original source/TypedIR; no native result is inferred from these tests.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::{CandidatePlan, InstructionKind as I, Type, TypedModule};

const ORIGINALS: [&str; 4] = [
    include_str!("impl204_fixtures/00_package_owned_control.zil"),
    include_str!("impl204_fixtures/01_direct_store_capture.zil"),
    include_str!("impl204_fixtures/02_original201_package_store_capture.zil"),
    include_str!("impl204_fixtures/03_mutating_store_capture.zil"),
];
const TWO: &str = r#"
package Item { manages bytes: ByteBuffer; value: i32 }
fn make() -> impl Fn() -> i32 {
    let first: Store<Item> = store_new()
    let second: Store<Item> = store_new()
    return move () => { return i32(store_len(first) + store_len(second)) }
}
fn main() -> i32 { let callback = make(); return callback() }
"#;
const ONCE: &str = r#"
package Item { manages bytes: ByteBuffer; value: i32 }
fn make() -> impl FnOnce() -> i32 {
    let items: Store<Item> = store_new()
    return () => { let taken = give items; return i32(store_len(taken)) }
}
fn main() -> i32 { let callback = make(); return callback() }
"#;

struct Case(PathBuf, bool);
impl Case {
    fn new(label: &str, source: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let retained = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let root = retained
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let directory = root.join(format!(
            "derived204-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("source.zil"), source).unwrap();
        Self(directory, retained.is_some())
    }
    fn write(&self, name: &str, contents: impl AsRef<[u8]>) {
        fs::write(self.0.join(name), contents).unwrap();
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.1 {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
fn candidate(label: &str, source: &str) -> (TypedModule, CandidatePlan) {
    let case = Case::new(label, source);
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "source.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result();
    case.write("syntax.txt", format!("{syntax:#?}"));
    let core = zil_expand::expand(&unit, &syntax.unwrap());
    case.write("core.txt", format!("{core:#?}"));
    let module = zil_types::check(
        &core.unwrap(),
        &TargetProfile::host_llvm(),
        &Limits::default(),
    );
    case.write("types.txt", format!("{module:#?}"));
    let module = module.unwrap();
    let plan = zil_analyze::analyze(&module, &Limits::default());
    case.write("analyze.txt", format!("{plan:#?}"));
    let plan = plan.unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    case.write("candidate.wire", &bytes);
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
            zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()),
        ),
    ] {
        case.write(&format!("verify-{route}.txt"), format!("{result:#?}"));
        result.unwrap();
    }
    (module, plan)
}
fn reject(label: &str, source: &str, module: &TypedModule, expected: &str) {
    let case = Case::new(label, source);
    case.write("mutated-module.txt", format!("{module:#?}"));
    let plan = CandidatePlan::empty_for(module);
    let wire = zil_ir::wire::encode_module_candidate(module, &plan, &Limits::default()).unwrap();
    case.write("untrusted-candidate.wire", &wire);
    let (decoded, _) = zil_ir::wire::decode_module_candidate(&wire, &Limits::default()).unwrap();
    assert_eq!(&decoded, module);
    for (route, module) in [("raw", module), ("wire", &decoded)] {
        let result = zil_analyze::analyze(module, &Limits::default());
        case.write(&format!("analyze-{route}.txt"), format!("{result:#?}"));
        let errors = result.unwrap_err();
        assert!(
            errors.iter().any(|d| d.phase == Phase::Analyze
                && d.category == ResultCategory::SafeReject
                && d.code == "E-ANALYZE-IR"
                && d.message.contains(expected)),
            "{label}/{route}: {errors:?}"
        );
    }
}

fn typed_window(label: &str, source: &str) -> TypedModule {
    let case = Case::new(label, source);
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "source.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result();
    case.write("syntax.txt", format!("{syntax:#?}"));
    let core = zil_expand::expand(&unit, &syntax.unwrap());
    case.write("core.txt", format!("{core:#?}"));
    let module = zil_types::check(
        &core.unwrap(),
        &TargetProfile::host_llvm(),
        &Limits::default(),
    );
    case.write("types.txt", format!("{module:#?}"));
    module.unwrap()
}

#[test]
fn original_live_capture_window_and_stronger_reserve_neighbor_close_in_analyze() {
    const ORIGINAL: &str = include_str!("impl204_fixtures/04_take_capture_live_window.zil");
    for extra_reserve in [false, true] {
        let source = if extra_reserve {
            ORIGINAL.replace(
                "let taken = give items",
                "let taken = give items\n store_reserve(taken, 8)",
            )
        } else {
            ORIGINAL.to_owned()
        };
        let good = source.replace("+ view.value - 42", "+ answer - 42");
        assert_ne!(good, source);
        candidate(
            &format!("window-last-use-before-take-{extra_reserve}"),
            &good,
        );
        let module = typed_window(&format!("window-live-after-take-{extra_reserve}"), &source);
        reject(
            &format!("window-live-after-take-{extra_reserve}"),
            &source,
            &module,
            "captured Store transition invalidates a live element window",
        );
    }
}

#[test]
fn two_same_type_capture_and_package_field_roots_remain_independent() {
    for package in [false, true] {
        for mutable in [false, true] {
            let prepare = "let first:Store<Item> = store_new(); let second:Store<Item> = store_new(); store_reserve(first,1); store_push(first,Item{bytes:bytes_new(),value:42});";
            let (assembly, borrowed, taken) = if package {
                (
                    "let bag=Bag{first:give first,second:give second};",
                    "bag.first",
                    "bag.second",
                )
            } else {
                ("", "first", "second")
            };
            let borrow = if mutable { "store_at_mut" } else { "store_at" };
            let source = format!(
                "package Item{{manages bytes:ByteBuffer;value:i32}} package Bag{{manages first:Store<Item>;manages second:Store<Item>}} fn make()->impl FnOnce()->i32{{{prepare}{assembly}return move()=>{{let view={borrow}({borrowed},0);let taken=give {taken};return view.value+i32(store_len(taken))}}}}fn main()->i32{{let callback=make();return callback()-42}}"
            );
            candidate(&format!("independent-field-{package}-{mutable}"), &source);
            let bad = source.replace(
                &format!("give {taken};return"),
                &format!("give {borrowed};return"),
            );
            assert_ne!(bad, source);
            let module = typed_window("same-root-window", &bad);
            reject(
                "same-root-window",
                &bad,
                &module,
                "captured Store transition invalidates a live element window",
            );
        }
    }
}

#[test]
fn unchanged_originals_pass_analyze_and_independent_raw_and_wire_verify() {
    for (index, source) in ORIGINALS.iter().enumerate() {
        candidate(&format!("original-{index}"), source);
    }
}

#[test]
fn actual_same_type_store_values_remain_distinct_owned_inputs() {
    let (module, _) = candidate("two-store-baseline", TWO);
    let mut duplicate = module.clone();
    let mut changed = false;
    for instruction in duplicate
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .flat_map(|b| &mut b.instructions)
    {
        if let I::MakeClosure { captures, .. } = &mut instruction.kind
            && captures.len() == 2
        {
            assert_ne!(captures[0], captures[1]);
            captures[1] = captures[0];
            changed = true;
        }
    }
    assert!(changed);
    reject(
        "same-value-twice",
        TWO,
        &duplicate,
        "SSA responsibility was already consumed",
    );
    let mut borrowed = module.clone();
    let mut changed = false;
    for function in &mut borrowed.functions {
        for instruction in function.blocks.iter().flat_map(|b| &b.instructions) {
            if let I::MakeClosure { captures, .. } = &instruction.kind
                && captures.len() == 2
            {
                let actual = &mut function.values[captures[0].0 as usize];
                let Type::Managed(payload) = actual else {
                    panic!("actual owned Store operand")
                };
                *actual = Type::Desc(payload.clone());
                changed = true;
            }
        }
    }
    assert!(changed);
    reject(
        "description-is-not-ownership",
        TWO,
        &borrowed,
        "actual capture type differs",
    );
}

#[test]
fn actual_consuming_entry_cannot_copy_or_repeat_taken_store() {
    let (module, _) = candidate("once-baseline", ONCE);
    let mut copied = module.clone();
    let mut changed = false;
    for instruction in copied
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .flat_map(|b| &mut b.instructions)
    {
        if let I::TakeCapture { index } = instruction.kind {
            instruction.kind = I::ReadCapture { index };
            changed = true;
        }
    }
    assert!(changed);
    reject(
        "read-owned-store",
        ONCE,
        &copied,
        "ReadCapture cannot copy an owned",
    );
    let mut repeated = module.clone();
    let mut changed = false;
    for function in &mut repeated.functions {
        for block in &mut function.blocks {
            if let Some(position) = block
                .instructions
                .iter()
                .position(|i| matches!(i.kind, I::TakeCapture { .. }))
            {
                let mut second = block.instructions[position].clone();
                let old = second.result.unwrap();
                second.result = Some(zil_ir::ValueId(function.values.len() as u32));
                function
                    .values
                    .push(function.values[old.0 as usize].clone());
                block.instructions.insert(position + 1, second);
                changed = true;
            }
        }
    }
    assert!(changed);
    reject(
        "take-owned-store-twice",
        ONCE,
        &repeated,
        "owned field was already given away",
    );
}

#[test]
fn callee_preparation_failure_keeps_owned_store_debt_and_body_take_is_observable() {
    for fail in [false, true] {
        let source = format!(
            r#"
extern "C" fn putchar(value:i32)->i32;
resource Ticket {{ code:i32 }}
impl Drop for Ticket {{ fn drop(self:Self) {{ unsafe {{ putchar(.code) }} }} }}
package Item {{ manages ticket:Ticket; value:i32 }}
fn make()->impl FnOnce(i32)->i32 {{
    let ticket=Ticket {{ code:84 }}
    let items:Store<Item>=store_new()
    store_reserve(items,1)
    store_push(items,Item {{ ticket:Ticket {{ code:65 }}, value:1 }})
    return move (delta:i32)=>{{
        unsafe {{ putchar(67) }}
        ticket.code
        let local=give items
        unsafe {{ putchar(88) }}
        return delta+i32(store_len(local))
    }}
}}
fn argument(fail:bool)->void!i32 {{ unsafe {{ putchar(80) }} if fail {{ return () }} return 1 }}
fn main()->void!i32 {{
    let guard=Ticket {{ code:90 }}
    let callback=make()
    return callback(argument({fail})?)-2
}}
"#
        );
        for optimization in [zil_driver::Optimization::O0, zil_driver::Optimization::O2] {
            let case = Case::new(&format!("callee-fail-{fail}-{optimization:?}"), &source);
            let report = zil_driver::compile_project(
                &zil_driver::ProjectSources {
                    root: ModuleId(0),
                    modules: vec![SourceUnit::new(
                        FileId(0),
                        ModuleId(0),
                        "source.zil",
                        &source,
                    )],
                    paths: Default::default(),
                },
                &zil_driver::CompileOptions {
                    stop_after: zil_driver::Stage::Build,
                    optimization,
                    output: Some(case.0.join("program")),
                    emit_dir: Some(case.0.join("phases")),
                    ..Default::default()
                },
            );
            case.write("compile.json", report.to_json());
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let result = std::process::Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            case.write("stdout.log", &result.stdout);
            case.write("stderr.log", &result.stderr);
            case.write(
                "native.txt",
                format!(
                    "exit={:?}; optimization={optimization:?}; argument_failure={fail}",
                    result.status.code()
                ),
            );
            assert_eq!(result.status.code(), Some(i32::from(fail)));
            assert_eq!(
                result.stdout,
                if fail {
                    b"PTAZ".as_slice()
                } else {
                    b"PCXATZ".as_slice()
                }
            );
            assert!(result.stderr.is_empty());
        }
    }
}
