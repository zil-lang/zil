//! Independent 176 review: alternate layout, source identities and untrusted IR.
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
};
use zil_ir::{CandidatePlan, InstructionKind as I, Type, TypedModule};
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn dir(label: &str) -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = base.join(format!(
        "review176-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
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
fn compile(label: &str, files: &[(&str, &str)]) -> CompileReport {
    let path = dir(label);
    for (name, text) in files {
        fs::write(path.join(format!("{name}.zil")), text).unwrap();
    }
    let report = compile_project(
        &project(files),
        &CompileOptions {
            emit_dir: Some(path.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(path.join("compile.json"), report.to_json()).unwrap();
    report
}
fn native(label: &str, files: &[(&str, &str)], expected: &[u8], exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let path = dir(label);
        for (name, text) in files {
            fs::write(path.join(format!("{name}.zil")), text).unwrap();
        }
        let binary = path.join("program");
        let report = compile_project(
            &project(files),
            &CompileOptions {
                stop_after: Stage::Build,
                optimization,
                output: Some(binary.clone()),
                emit_dir: Some(path.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(path.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let result = Command::new(binary).output().unwrap();
        fs::write(
            path.join("native.txt"),
            format!("optimization={optimization:?}\n{result:?}\n"),
        )
        .unwrap();
        fs::write(path.join("stdout"), &result.stdout).unwrap();
        fs::write(path.join("stderr"), &result.stderr).unwrap();
        assert_eq!(result.status.code(), Some(exit));
        assert_eq!(result.stdout, expected);
        assert!(result.stderr.is_empty());
        let llvm = fs::read_to_string(path.join("phases/llvm.ll")).unwrap();
        assert!(!llvm.contains(" noalias "));
    }
}
fn reject_source(label: &str, source: &str) {
    let report = compile(label, &[("main", source)]);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        matches!(
            report.phase,
            Phase::Expand | Phase::Types | Phase::Analyze | Phase::Verify
        ),
        "{}",
        report.to_json()
    );
    assert!(!report.diagnostics.is_empty());
    assert!(report.commands.is_empty());
}
fn candidate(source: &str) -> (TypedModule, CandidatePlan) {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "review-raw176.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let module = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &TargetProfile::host_llvm(),
        &Limits::default(),
    )
    .unwrap();
    (module, plan)
}
fn raw_reject(label: &str, module: TypedModule, mut plan: CandidatePlan, code: &str) {
    plan.module_fingerprint = module.fingerprint();
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let path = dir(label);
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    fs::write(path.join("candidate.bin"), &bytes).unwrap();
    for (route, errors) in [
        (
            "raw",
            zil_verify::verify_module(module, plan, &target, &limits).unwrap_err(),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err(),
        ),
    ] {
        fs::write(path.join(format!("{route}.txt")), format!("{errors:#?}")).unwrap();
        assert!(errors.iter().all(|d| d.phase == Phase::Verify));
        assert!(
            errors
                .iter()
                .any(|d| d.code == code && d.category == ResultCategory::SafeReject),
            "{label}/{route}: {errors:?}"
        );
    }
}

const MODEL: &str = r#"
extern "C" fn putchar(x:i32)->i32;
pub resource Stamp{pub code:i32}
impl Drop for Stamp{fn drop(self:Self){unsafe{putchar(.code)}}}
pub package Scene<F>{pub lead:[i64;3];pub callback:F;manages stamp:Stamp;pub tail:u16;pub seen:i32}
pub fn wrap<F:FnMut(i32)->i32>(callback:F)->Scene<F>{
    return Scene::<F>{lead:[17i64,19i64,23i64],callback:callback,stamp:Stamp{code:84},tail:513u16,seen:0}
}
"#;
const FACTORY: &str = r#"
import model
pub fn counter(seed:i32,mark:i32)->impl FnMut(i32)->i32{
    var count=seed;let witness=model.Stamp{code:mark};
    return move(delta:i32)=>{count+=delta;return count+witness.code-mark}
}
pub fn same_environment(seed:i32,mark:i32)->impl FnMut(i32)->i32{return counter(seed,mark)}
"#;
const UPDATE: &str = r#"
import model
pub fn step<G:FnMut(i32)->i32>(state:model.Scene<G>,amount:i32)->model.Scene<G>{
    var next=state;let callback=&mut next.callback;next.seen=callback(amount);return next
}
pub fn twice<H:FnMut(i32)->i32>(state:model.Scene<H>,amount:i32)->model.Scene<H>{return step(step(state,amount),amount)}
"#;
#[test]
fn independent_nonzero_field_offset_and_four_module_f_g_h_state_chain() {
    let main = r#"import model;import factory;import update;
fn main()->i32{
    var state=model.wrap(factory.counter(5,65));
    state=update.step(state,3);state=update.twice(state,2);
    if state.seen!=12||state.lead[2]!=23i64||state.tail!=513u16{return 10}
    let first=&mut state.callback;if first(1)!=13{return 11}
    state.callback=factory.counter(20,66);
    let second=&mut state.callback;if second(2)!=22{return 12}
    if state.lead[0]+state.lead[1]!=36i64{return 13}
    return 0
}"#;
    native(
        "four-module-offset",
        &[
            ("main", main),
            ("model", MODEL),
            ("factory", FACTORY),
            ("update", UPDATE),
        ],
        b"ABT",
        0,
    );
}

const SINGLE: &str = r#"
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package Holder<F>{prefix:[i64;2];callback:F;manages guard:R;last:u8}
fn pack<F:FnMut(i32)->i32>(callback:F)->Holder<F>{return Holder::<F>{prefix:[31i64,37i64],callback:callback,guard:R{code:84},last:9u8}}
fn make(seed:i32,mark:i32)->impl FnMut(i32)->i32{var n=seed;let r=R{code:mark};return move(delta:i32)=>{n+=delta;return n+r.code-mark}}
fn forward(seed:i32,mark:i32)->impl FnMut(i32)->i32{return make(seed,mark)}
fn relay<G:FnMut(i32)->i32>(s:Holder<G>)->Holder<G>{return s}
"#;
#[test]
fn independently_reject_equal_layout_opaque_forwarder_and_live_parent_move() {
    for (label, body) in [
        (
            "opaque-field",
            "var s=pack(make(1,65));s.callback=forward(2,66);return 0",
        ),
        (
            "opaque-whole-state",
            "var s=pack(make(1,65));s=pack(forward(2,66));return 0",
        ),
        (
            "opaque-join-after-helper",
            "var s=relay(pack(make(1,65)));s=relay(pack(forward(2,66)));return 0",
        ),
        (
            "live-parent-move",
            "var s=pack(make(1,65));let f=&mut s.callback;let moved=relay(s);return f(0)",
        ),
        (
            "live-field-take",
            "var s=pack(make(1,65));let f=&mut s.callback;let taken=s.callback;return f(0)",
        ),
    ] {
        reject_source(label, &format!("{SINGLE}fn main()->i32{{{body}}}"));
    }
}

#[test]
fn independent_failure_after_new_owned_argument_and_before_field_publication() {
    let source = format!(
        "{SINGLE}enum E{{Fail}}fn fail()->E!i32=>E.Fail;fn prepare(give r:R,x:i32)->impl FnMut(i32)->i32{{return move(y:i32)=>r.code+x+y}}fn main()->E!i32{{var s=pack(make(1,65));s.callback=prepare(R{{code:66}},fail()?);return 0}}"
    );
    // Even though publication is never reached, the two factory identities
    // still differ: use a same-factory replacement preparation below instead.
    reject_source("different-factory-on-error-path", &source);
    let source = r#"
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package State<F>{callback:F;manages parent:R}
fn pack<F:Fn(i32)->i32>(f:F)->State<F>{return State::<F>{callback:f,parent:R{code:84}}}
fn make(give r:R,x:i32)->impl Fn(i32)->i32{return move(y:i32)=>r.code+x+y}
enum E{Fail}fn fail()->E!i32=>E.Fail;
fn main()->E!i32{var s=pack(make(R{code:65},1));s.callback=make(R{code:66},fail()?);return 0}
"#;
    native("failed-rhs-prepared-owner", &[("main", source)], b"BAT", 1);
}

#[test]
fn independent_fnonce_double_take_call_and_no_resource_closure_uniqueness() {
    let common = r#"
resource R{code:i32}
package State<F>{callback:F;manages owner:R}
fn pack<F:FnOnce()->i32>(f:F)->State<F>{return State::<F>{callback:f,owner:R{code:1}}}
fn once()->impl FnOnce()->i32{let r=R{code:7};return move()=>{let moved=give r;return moved.code}}
fn pure()->impl Fn()->i32{return ()=>7}
"#;
    for (label, body) in [
        (
            "once-double-call",
            "let s=pack(once());let f=s.callback;let a=f();return a+f()",
        ),
        (
            "once-double-take",
            "let s=pack(once());let a=s.callback;let b=s.callback;return a()+b()",
        ),
        (
            "pure-two-owners",
            "let f=pure();let a=pack(f);let b=pack(f);return 0",
        ),
    ] {
        reject_source(label, &format!("{common}fn main()->i32{{{body}}}"));
    }
    let source =
        format!("{common}fn main()->i32{{let s=pack(once());let f=s.callback;return f()-7}}");
    native("once-single-consumption", &[("main", &source)], b"", 0);
}

#[test]
fn independent_raw_and_wire_check_real_field_and_call_permission() {
    let source =
        format!("{SINGLE}fn main()->i32{{var s=pack(make(1,65));return (&mut s.callback)(2)}}");
    let (original, plan) = candidate(&source);
    for variant in 0..3 {
        let mut module = original.clone();
        let f = module
            .functions
            .iter_mut()
            .find(|f| f.name == "main")
            .unwrap();
        let (block, index) = f
            .blocks
            .iter()
            .enumerate()
            .find_map(|(b, block)| {
                block
                    .instructions
                    .iter()
                    .position(|i| matches!(i.kind, I::BorrowPackageField { .. }))
                    .map(|i| (b, i))
            })
            .unwrap();
        let instruction = &mut f.blocks[block].instructions[index];
        let output = instruction.result.unwrap();
        let I::BorrowPackageField {
            local,
            index,
            mutable,
        } = &mut instruction.kind
        else {
            unreachable!()
        };
        match variant {
            0 => *index = 0,
            1 => f.locals[local.0 as usize].mutable = false,
            2 => {
                *mutable = false;
                if let Type::Address { mutable, .. } = &mut f.values[output.0 as usize] {
                    *mutable = false;
                }
            }
            _ => unreachable!(),
        }
        raw_reject(
            [
                "field-is-copy-data",
                "immutable-parent",
                "shared-call-upgrade",
            ][variant],
            module,
            plan.clone(),
            [
                "E-VERIFY-PACKAGE",
                "E-VERIFY-MUTABILITY",
                "E-VERIFY-CLOSURE-CALL",
            ][variant],
        );
    }
}

#[test]
fn independent_raw_and_wire_reject_pure_callback_published_twice_without_addresses() {
    let source = r#"
resource R{code:i32}package State<F>{manages owner:R;callback:F}
fn pack<F:Fn()->i32>(f:F)->State<F>{return State::<F>{owner:R{code:1},callback:f}}
fn make(x:i32)->impl Fn()->i32{return move()=>x}
fn main()->i32{let a=make(1);let b=make(2);let first=pack(a);let second=pack(b);return 0}
"#;
    let (mut module, plan) = candidate(source);
    let constructors: Vec<_> = module
        .functions
        .iter()
        .filter(|f| {
            matches!(&f.return_type,Type::Package{fields,..}
        if fields.iter().any(|(_,ty)|matches!(ty,Type::Closure(_))))
        })
        .map(|f| f.id)
        .collect();
    let main = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    assert!(
        !main
            .values
            .iter()
            .any(|t| matches!(t, Type::Address { .. }))
    );
    let mut first = None;
    let mut changed = 0;
    for instruction in main.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let I::Call {
            function,
            arguments,
        } = &mut instruction.kind
            && constructors.contains(function)
        {
            if let Some(v) = first {
                arguments[0] = v;
                changed += 1;
            } else {
                first = Some(arguments[0]);
            }
        }
    }
    assert_eq!(changed, 1);
    raw_reject(
        "pure-double-publication-no-address",
        module,
        plan,
        "E-VERIFY-MOVE",
    );
}

#[test]
fn independent_raw_and_wire_reject_parent_drop_with_only_field_window_remaining() {
    let source = format!(
        "{SINGLE}fn main()->i32{{var s=pack(make(1,65));let callback=&mut s.callback;return callback(2)}}"
    );
    let (mut module, plan) = candidate(&source);
    let f = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let (b, i) = f
        .blocks
        .iter()
        .enumerate()
        .find_map(|(b, block)| {
            block
                .instructions
                .iter()
                .position(|i| matches!(i.kind, I::DropPackage { .. }))
                .map(|i| (b, i))
        })
        .unwrap();
    let cleanup = f.blocks[b].instructions.remove(i);
    let block = f
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, I::BorrowPackageField { .. }))
        })
        .unwrap();
    let at = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, I::BorrowPackageField { .. }))
        .unwrap();
    block.instructions.insert(at + 1, cleanup);
    raw_reject("parent-drop-live-field", module, plan, "E-BORROW-CONFLICT");
}
