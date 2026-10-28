//! Independent 200 review: two values of one concrete closure type retain
//! separate storage, transfer, field provenance and cleanup obligations.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{
    Diagnostic, FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
};
use zil_driver::{CompileOptions, CompileReport, Optimization, Stage, compile_source};
use zil_ir::{
    CallableCleanup, CallableExit, CallableKind, CallableParameterContract, CallableRole,
    CallableTrace, CallableTraces, CandidatePlan, FunctionId, InstructionKind, Type, TypedModule,
    ValueId,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Case(PathBuf, bool);
impl Case {
    fn new(label: &str) -> Self {
        let retained = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let root = retained
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = root.join(format!(
            "independent200-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path, retained.is_some())
    }
    fn write(&self, name: &str, bytes: impl AsRef<[u8]>) {
        fs::write(self.0.join(name), bytes).unwrap();
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

fn compile(case: &Case, source: &str, native: Option<Optimization>) -> CompileReport {
    case.write("source.zil", source);
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "independent200.zil", source),
        &CompileOptions {
            stop_after: if native.is_some() {
                Stage::Build
            } else {
                Stage::Verify
            },
            optimization: native.unwrap_or(Optimization::O0),
            output: native.map(|_| case.0.join("program")),
            emit_dir: Some(case.0.join("phases")),
            ..CompileOptions::default()
        },
    );
    case.write("compile.json", report.to_json());
    report
}

fn native(label: &str, source: &str, expected: &str, expected_exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new(label);
        let report = compile(&case, source, Some(optimization));
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let output = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        case.write("stdout.log", &output.stdout);
        case.write("stderr.log", &output.stderr);
        case.write("native.txt", format!("optimization={optimization:?}\nexit={:?}\nexpected_exit={expected_exit}\nexpected_stdout={expected:?}\n", output.status.code()));
        assert_eq!(output.status.code(), Some(expected_exit));
        assert_eq!(output.stdout, expected.as_bytes());
        assert!(output.stderr.is_empty());
    }
}

fn baseline(label: &str, source: &str) -> (TypedModule, CandidatePlan) {
    let case = Case::new(label);
    case.write("source.zil", source);
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "independent200.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result();
    case.write("syntax.txt", format!("{syntax:#?}"));
    let core = zil_expand::expand(&unit, &syntax.unwrap());
    case.write("core.txt", format!("{core:#?}"));
    let typed = zil_types::check(
        &core.unwrap(),
        &TargetProfile::host_llvm(),
        &Limits::default(),
    );
    case.write("typed.txt", format!("{typed:#?}"));
    let module = typed.unwrap();
    let candidate = zil_analyze::analyze(&module, &Limits::default());
    case.write("analyze.txt", format!("{candidate:#?}"));
    let candidate = candidate.unwrap();
    let wire =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &Limits::default()).unwrap();
    case.write("baseline.wire", &wire);
    let raw = zil_verify::verify_module(
        module.clone(),
        candidate.clone(),
        &module.target,
        &Limits::default(),
    );
    let decoded = zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default());
    case.write("verify.txt", format!("raw={raw:#?}\nwire={decoded:#?}\n"));
    raw.unwrap();
    decoded.unwrap();
    (module, candidate)
}

fn reject(
    label: &str,
    module: TypedModule,
    mut candidate: CandidatePlan,
    category: ResultCategory,
    clue: &str,
    analyze_too: bool,
) {
    let case = Case::new(label);
    candidate.module_fingerprint = module.fingerprint();
    let wire =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &Limits::default()).unwrap();
    case.write("attacked.wire", &wire);
    case.write("attacked-typed.txt", format!("{module:#?}"));
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&wire, &Limits::default()).unwrap(),
        (module.clone(), candidate.clone())
    );
    let raw = zil_verify::verify_module(
        module.clone(),
        candidate,
        &module.target,
        &Limits::default(),
    );
    let decoded = zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default());
    let analyzed = zil_analyze::analyze(&module, &Limits::default());
    case.write(
        "diagnostics.txt",
        format!("raw={raw:#?}\nwire={decoded:#?}\nanalyze={analyzed:#?}\n"),
    );
    for result in [raw, decoded] {
        let errors = result.unwrap_err();
        assert!(
            errors.iter().any(|d| d.phase == Phase::Verify
                && d.category == category
                && (d.code == clue || d.message.contains(clue))),
            "{label}: {errors:#?}"
        );
    }
    if analyze_too {
        let errors = analyzed.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.phase == Phase::Analyze && d.category == category),
            "{label}: {errors:#?}"
        );
    }
}

const PREFIX: &str = r#"
extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
fn emit(n:i32){unsafe{putchar(n)}}
"#;

// Callback arguments are positions 0 and 3; their fields are also 0 and 3.
// The name and interleaved Copy/Managed arguments exercise the actual signature.
const MIXED: &str = r#"
package Capsule<T,F>{left:F;manages guard:Ticket;value:T;right:F}
fn stitch<T:Copy,F:FnMut(i32)->i32>(first:F,value:T,give guard:Ticket,second:F)->Capsule<T,F>{
    return Capsule::<T,F>{left:first,guard:give guard,value:value,right:second}
}
"#;
const OWNED_FACTORY: &str = r#"
fn handler(code:i32)->impl FnMut(i32)->i32{
    let ticket=Ticket{code:code};var total=0;
    return move(n:i32)=>{emit(ticket.code);total+=n;return total}
}
"#;
const OWNED_MAIN: &str = r#"
fn main()->i32{
    var state=stitch(handler(65),10,Ticket{code:84},handler(66));
    let first=state.left(1);let second=state.right(2);
    let again=state.left(3)+state.right(4);
    return first+second+again+state.value-23
}
"#;
const COPY_FACTORY: &str = r#"
fn counter(seed:i32)->impl FnMut(i32)->i32{
    var total=seed;return move(n:i32)=>{total+=n;return total}
}
"#;
const COPY_MAIN: &str = r#"
fn main()->i32{
    var original=counter(10);
    var state=stitch(original,1,Ticket{code:84},original);
    let left=state.left(1);let right=state.right(2);let retained=original(3);
    return left+right+retained+state.value-37
}
"#;

fn main_index(module: &TypedModule) -> usize {
    module
        .functions
        .iter()
        .position(|f| f.name == "main")
        .unwrap()
}

fn constructor_index(module: &TypedModule) -> usize {
    let indexes = module
        .functions
        .iter()
        .enumerate()
        .filter_map(|(index, f)| {
            (matches!(f.return_type, Type::Package { .. })
                && f.parameters
                    .iter()
                    .filter(|p| matches!(f.locals[p.0 as usize].ty, Type::Closure(_)))
                    .count()
                    == 2)
                .then_some(index)
        })
        .collect::<Vec<_>>();
    assert_eq!(indexes.len(), 1, "actual two-value constructor");
    indexes[0]
}

fn call_arguments(module: &TypedModule, callee: FunctionId) -> Vec<ValueId> {
    module.functions[main_index(module)]
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match &i.kind {
            InstructionKind::Call {
                function,
                arguments,
            } if *function == callee => Some(arguments.clone()),
            _ => None,
        })
        .unwrap()
}

fn source_reject(
    label: &str,
    source: &str,
    category: ResultCategory,
    phases: &[Phase],
    clue: &str,
) {
    let case = Case::new(label);
    let report = compile(&case, source, None);
    assert_eq!(report.status, category, "{}", report.to_json());
    assert!(phases.contains(&report.phase), "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == clue || d.message.contains(clue)),
        "{}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
}

#[test]
fn structural_copy_inputs_become_two_owned_places_and_leave_the_original_callable() {
    let source = format!("{PREFIX}{MIXED}{COPY_FACTORY}{COPY_MAIN}");
    native("copy-fnmut-original-survives", &source, "T", 0);
    let (module, plan) = baseline("copy-transfer-control", &source);
    let constructor = &module.functions[constructor_index(&module)];
    assert!(constructor.callable_contract.is_none());
    assert_eq!(constructor.parameters.len(), 4);
    assert_eq!(
        constructor.locals[constructor.parameters[0].0 as usize].ty,
        constructor.locals[constructor.parameters[3].0 as usize].ty
    );
    let arguments = call_arguments(&module, constructor.id);
    let main = &module.functions[main_index(&module)];
    let places = [arguments[0], arguments[3]].map(|value| {
        let instruction = main
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .find(|i| i.result == Some(value))
            .unwrap();
        let InstructionKind::TakeLocal { local } = instruction.kind else {
            panic!("Copy callback input must transfer its own staged place: {instruction:?}")
        };
        local
    });
    assert_ne!(places[0], places[1]);
    for (number, argument) in [arguments[0], arguments[3]].into_iter().enumerate() {
        let mut attacked = module.clone();
        let index = main_index(&attacked);
        let instruction = attacked.functions[index]
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find(|i| i.result == Some(argument))
            .unwrap();
        instruction.kind = InstructionKind::Load(places[number]);
        // This attack must get a matching, freshly reconstructed candidate;
        // the independent rejection cannot rely on stale producer provenance.
        let candidate = zil_analyze::analyze(&attacked, &Limits::default()).unwrap();
        reject(
            &format!("copy-input-{number}-access-view-not-transfer"),
            attacked,
            candidate,
            ResultCategory::SafeReject,
            "E-VERIFY-MOVE",
            false,
        );
    }
    let mut attacked = module;
    let index = main_index(&attacked);
    let instruction = attacked.functions[index]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| i.result == Some(arguments[3]))
        .unwrap();
    instruction.kind = InstructionKind::TakeLocal { local: places[0] };
    reject(
        "copy-second-transfer-reuses-first-slot",
        attacked,
        plan,
        ResultCategory::SafeReject,
        "E-VERIFY-INITIALIZATION",
        false,
    );
}

#[test]
fn two_noncopy_values_keep_each_field_debt_and_reject_duplicate_actual_arguments() {
    let source = format!("{PREFIX}{MIXED}{OWNED_FACTORY}{OWNED_MAIN}");
    native("nonadjacent-owned-inputs", &source, "ABABATB", 0);
    let (module, plan) = baseline("two-owned-values-control", &source);
    let constructor = constructor_index(&module);
    let target = module.functions[constructor].id;
    let arguments = call_arguments(&module, target);
    assert_ne!(arguments[0], arguments[3]);
    let mut duplicate_call = module.clone();
    let index = main_index(&duplicate_call);
    let instruction = duplicate_call.functions[index]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind,InstructionKind::Call{function,..}if function==target))
        .unwrap();
    let InstructionKind::Call { arguments, .. } = &mut instruction.kind else {
        unreachable!()
    };
    arguments[3] = arguments[0];
    reject(
        "duplicate-owned-actual-argument",
        duplicate_call,
        plan.clone(),
        ResultCategory::SafeReject,
        "E-VERIFY-MOVE",
        false,
    );

    let mut duplicate_field = module.clone();
    let instruction = duplicate_field.functions[constructor]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::MakePackage { .. }))
        .unwrap();
    let InstructionKind::MakePackage { fields } = &mut instruction.kind else {
        unreachable!()
    };
    assert_eq!(fields.len(), 4);
    fields[3] = fields[0];
    reject(
        "duplicate-owned-assembly-value",
        duplicate_field,
        plan.clone(),
        ResultCategory::SafeReject,
        "E-VERIFY-MOVE",
        false,
    );

    let mut duplicate_local = module;
    let f = &mut duplicate_local.functions[constructor];
    f.parameters[3] = f.parameters[0];
    reject(
        "two-parameters-one-local",
        duplicate_local,
        plan,
        ResultCategory::SafeReject,
        "E-VERIFY-PARAMETER",
        false,
    );
    let bad = format!(
        "{PREFIX}{MIXED}{OWNED_FACTORY}fn main()->i32{{let f=handler(65);let state=stitch(f,10,Ticket{{code:84}},f);return 0}}"
    );
    source_reject(
        "noncopy-source-repeated-input",
        &bad,
        ResultCategory::SafeReject,
        &[Phase::Analyze],
        "E-UNINITIALIZED",
    );
}

const ONCE: &str = r#"
package Capsule<F>{left:F;manages guard:Ticket;right:F}
fn assemble<F:FnOnce()->i32>(left:F,give guard:Ticket,right:F)->Capsule<F>{
    return Capsule::<F>{left:left,guard:give guard,right:right}
}
fn make(code:i32)->impl Fn()->i32{
    let ticket=Ticket{code:code};return move()=>{emit(67);return ticket.code}
}
fn consume<G:FnOnce()->i32>(state:Capsule<G>)->i32{
    let result=state.left()+state.right();emit(88);return result
}
fn main()->i32{return consume(assemble(make(65),Ticket{code:84},make(66)))-131}
"#;

#[test]
fn declared_once_consumes_real_fields_of_one_actual_fn_type_separately() {
    let source = format!("{PREFIX}{ONCE}");
    native("fn-to-once-both-fields", &source, "CACBXT", 0);
    let (module, plan) = baseline("fn-to-once-field-control", &source);
    assert_eq!(module.closures.len(), 1);
    assert_eq!(module.closures[0].kind, CallableKind::Fn);
    let consume = module
        .functions
        .iter()
        .position(|f| {
            f.blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .filter(|i| matches!(i.kind, InstructionKind::CallClosureOnce { .. }))
                .count()
                == 2
        })
        .unwrap();
    let takes = module.functions[consume]
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .filter_map(|i| match i.kind {
            InstructionKind::TakePackageField { local, index } => Some((local, index)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(takes.len(), 2);
    assert_eq!(takes[0].0, takes[1].0);
    assert_eq!((takes[0].1, takes[1].1), (0, 2));
    let mut attacked = module;
    let take = attacked.functions[consume]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::TakePackageField { index: 2, .. }))
        .unwrap();
    take.kind = InstructionKind::TakePackageField {
        local: takes[0].0,
        index: 0,
    };
    reject(
        "once-second-take-same-field",
        attacked,
        plan,
        ResultCategory::SafeReject,
        "E-VERIFY-RESOURCE",
        false,
    );
    source_reject(
        "once-source-same-field-twice",
        &source.replace("state.left()+state.right()", "state.left()+state.left()"),
        ResultCategory::SafeReject,
        &[Phase::Expand],
        "E-GENERIC-ARGUMENT",
    );

    for (choice, expression, expected) in [
        ("left", "state.left()", "CAXTB"),
        ("right", "state.right()", "CBXAT"),
    ] {
        let single = source
            .replace("state.left()+state.right()", expression)
            .replace(")-131", if choice == "left" { ")-65" } else { ")-66" });
        native(
            &format!("once-{choice}-other-still-owned"),
            &single,
            expected,
            0,
        );
    }
}

#[test]
fn interleaved_argument_preparation_cleans_only_the_values_actually_acquired() {
    let source = format!(
        r#"{PREFIX}{MIXED}{OWNED_FACTORY}
enum E{{Fail}}
fn mark()->i32{{emit(80);return 10}}
fn next_code(fail:bool)->E!i32{{emit(81);if fail{{return E.Fail}}return 66}}
fn main()->E!i32{{
    let state=stitch(handler(65),mark(),Ticket{{code:84}},handler(next_code(false)?));
    return state.value-10
}}
"#
    );
    native("staging-complete", &source, "PQATB", 0);
    native(
        "staging-second-closure-error",
        &source.replace("next_code(false)", "next_code(true)"),
        "PQTA",
        1,
    );
}

#[test]
fn symbolic_resource_ownership_survives_real_give_but_not_description_aliases() {
    let source = format!(
        r#"{PREFIX}
package Capsule<F>{{left:F;manages guard:Ticket;right:F}}
fn assemble<F:Fn()->i32>(left:F,give root:Ticket,right:F,choose:bool)->Capsule<F>{{
    let adopted=if choose{{(give root)}}else{{Ticket{{code:85}}}};
    let forwarded=(give adopted);
    return Capsule::<F>{{left:left,guard:give forwarded,right:right}}
}}
fn value(n:i32)->impl Fn()->i32{{return move()=>n}}
fn main()->i32{{let state=assemble(value(1),Ticket{{code:84}},value(2),true);return state.left()+state.right()-3}}
"#
    );
    native("conditional-real-resource-owner", &source, "T", 0);
    native(
        "conditional-new-resource-owner",
        &source.replace("value(2),true", "value(2),false"),
        "TU",
        0,
    );

    let prefix = format!("{PREFIX}package Capsule<F>{{left:F;manages guard:Ticket;right:F}}");
    for (label, body, clue) in [
        (
            "unused-conditional-description-alias",
            "let adopted=if choose{give root}else{root};return Capsule::<F>{left:left,guard:give adopted,right:right}",
            "descriptions do not own",
        ),
        (
            "unused-conditional-owner-double-give",
            "let adopted=if choose{give root}else{Ticket{code:85}};let stolen=give root;return Capsule::<F>{left:left,guard:give adopted,right:right}",
            "after give",
        ),
        (
            "unused-forwarded-resource-double-give",
            "let adopted=(give root);let forwarded=(give adopted);return Capsule::<F>{left:left,guard:give adopted,right:right}",
            "after give",
        ),
    ] {
        let bad = format!(
            "{prefix}fn uncalled<F:Fn()->i32>(left:F,give root:Ticket,right:F,choose:bool)->Capsule<F>{{{body}}}fn main(){{}}"
        );
        source_reject(
            label,
            &bad,
            ResultCategory::SafeReject,
            &[Phase::Expand],
            clue,
        );
    }
}

const LENDING: &str = r#"
package Capsule<F>{left:F;manages storage:ByteBuffer;count:i32;right:F}
fn make(seed:u8)->impl FnMut()->&[u8]{
    let bytes=bytes_new();
    return move()=>{bytes_reserve(bytes,bytes_len(bytes)+1);bytes_push(bytes,seed);return bytes_view(bytes)}
}
fn stitch<F:FnMut()->&[u8]>(left:F,right:F)->Capsule<F>{
    return Capsule::<F>{left:left,storage:bytes_new(),count:0,right:right}
}
fn main()->i32{
    var state=stitch(make(7u8),make(9u8));
    let first=state.left();let other=state.right();state.count+=1;
    return i32(first[0])+i32(other[0])+state.count-17
}
"#;

#[test]
fn lending_fields_with_one_closure_id_have_separate_origins_but_keep_the_parent() {
    native("two-fields-lending", LENDING, "", 0);
    native(
        "lending-live-sibling-replacement",
        &LENDING
            .replace(
                "let other=state.right()",
                "state.right=make(11u8);let other=state.right()",
            )
            .replace("state.count-17", "state.count-19"),
        "",
        0,
    );
    for (label, replacement) in [
        ("same-field-recall", "let other=state.left()"),
        (
            "same-field-take",
            "let taken=state.left;let other=state.right()",
        ),
    ] {
        source_reject(
            label,
            &LENDING.replace("let other=state.right()", replacement),
            ResultCategory::SafeReject,
            &[Phase::Verify],
            "E-BORROW-CONFLICT",
        );
    }
    // Keep the first attempt's immutable-position diagnostic distinct from
    // the actual owner invalidation test below; it did not reach loan checking.
    source_reject(
        "parent-move-immutable-diagnostic",
        &LENDING.replace(
            "let other=state.right()",
            "let moved=state;let other=moved.right()",
        ),
        ResultCategory::SafeReject,
        &[Phase::Types],
        "E-IMMUTABLE-ASSIGNMENT",
    );
    let moved = format!(
        "{}fn main()->i32{{var state=stitch(make(7u8),make(9u8));let first=state.left();var moved=state;return i32(first[0])+moved.count-7}}",
        LENDING.split("fn main()").next().unwrap()
    );
    source_reject(
        "live-lending-parent-move",
        &moved,
        ResultCategory::SafeReject,
        &[Phase::Verify],
        "E-BORROW-CONFLICT",
    );
    let (mut module, _) = baseline("lending-field-provenance-control", LENDING);
    let main = main_index(&module);
    let state = module.functions[main]
        .locals
        .iter()
        .find(|l| l.name == "state")
        .unwrap()
        .id;
    let borrow=module.functions[main].blocks.iter_mut().flat_map(|b| &mut b.instructions)
        .find(|i|matches!(i.kind,InstructionKind::BorrowPackageField{local,index:3,..}if local==state)).unwrap();
    let InstructionKind::BorrowPackageField { index, .. } = &mut borrow.kind else {
        unreachable!()
    };
    *index = 0;
    let candidate = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    reject(
        "lending-second-field-retargeted-to-first",
        module,
        candidate,
        ResultCategory::SafeReject,
        "E-BORROW-CONFLICT",
        false,
    );
}

const EXTRA: &str = r#"
package Capsule<F>{left:F;manages guard:Ticket;right:F}
fn assemble<F:FnMut(i32)->i32>(left:F,unused:i32,give guard:Ticket,right:F)->Capsule<F>{
    return Capsule::<F>{left:left,guard:give guard,right:right}
}
fn main()->i32{
    let bytes=bytes_new();let length=bytes_len(bytes);
    let marker=3;let address=&marker;let seen=*address;
    var state=assemble(handler(65),seen,Ticket{code:84},handler(66));
    return state.left(1)+state.right(2)+i32(length)-3
}
"#;

#[test]
fn raw_extra_descriptions_addresses_and_fake_empty_contracts_do_not_gain_authority() {
    let source = format!("{PREFIX}{OWNED_FACTORY}{EXTRA}");
    let (module, plan) = baseline("ordinary-extras-control", &source);
    let constructor = constructor_index(&module);
    let id = module.functions[constructor].id;
    let main = main_index(&module);
    let description=module.functions[main].blocks.iter().flat_map(|b|&b.instructions)
        .find_map(|i|matches!(i.kind,InstructionKind::DescribeLocal{..})
            .then_some(i.result).flatten()
            .filter(|v|matches!(&module.functions[main].values[v.0 as usize],Type::Desc(inner)if inner.as_ref()==&Type::ByteBuffer))).unwrap();
    let address = module.functions[main]
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            matches!(i.kind, InstructionKind::BorrowLocal { .. })
                .then_some(i.result)
                .flatten()
        })
        .unwrap();
    for (label, value) in [
        ("forbidden-description-extra", description),
        ("forbidden-address-extra", address),
    ] {
        let mut attacked = module.clone();
        let ty = attacked.functions[main].values[value.0 as usize].clone();
        let parameter = attacked.functions[constructor].parameters[1];
        attacked.functions[constructor].locals[parameter.0 as usize].ty = ty;
        let call = attacked.functions[main]
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find(|i| matches!(i.kind,InstructionKind::Call{function,..}if function==id))
            .unwrap();
        let InstructionKind::Call { arguments, .. } = &mut call.kind else {
            unreachable!()
        };
        arguments[1] = value;
        reject(
            label,
            attacked,
            plan.clone(),
            ResultCategory::Unsupported,
            "finite Copy or direct Managed",
            false,
        );
    }
    let mut fake = module;
    let f = &mut fake.functions[constructor];
    let Type::Closure(closure) = f.locals[f.parameters[0].0 as usize].ty else {
        unreachable!()
    };
    f.callable_contract = Some(CallableParameterContract {
        roles: [0, 3]
            .into_iter()
            .map(|parameter| CallableRole {
                parameter,
                closure,
                kind: CallableKind::FnMut,
                hold: vec![],
                call: vec![],
                writes: vec![],
                cleanup: CallableCleanup::Owned,
            })
            .collect(),
        traces: CallableTraces::Finite(vec![CallableTrace {
            calls: vec![],
            exit: CallableExit::Return,
        }]),
    });
    reject(
        "fake-empty-noescape-contract-on-owning-constructor",
        fake,
        plan,
        ResultCategory::Unsupported,
        "concrete callback helper results",
        false,
    );
}

fn boundary(mut run: impl FnMut(usize) -> Result<(), Vec<Diagnostic>>) -> usize {
    let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
    run(high).unwrap();
    while low + 1 < high {
        let n = low + (high - low) / 2;
        match run(n) {
            Ok(()) => high = n,
            Err(ds) => {
                assert!(
                    ds.iter()
                        .any(|d| d.category == ResultCategory::ResourceLimit),
                    "{ds:#?}"
                );
                low = n;
            }
        }
    }
    run(high).unwrap();
    let errors = run(high - 1).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.category == ResultCategory::ResourceLimit)
    );
    run(high).unwrap();
    high
}

#[test]
fn two_actual_values_and_interleaved_inputs_have_exact_fresh_phase_budgets() {
    let source = format!("{PREFIX}{MIXED}{OWNED_FACTORY}{OWNED_MAIN}");
    let (module, candidate) = baseline("budget-two-values-control", &source);
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "independent200.zil", &source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let expand = boundary(|n| {
        zil_expand::expand_with_map_limits(
            &unit,
            &syntax,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|_| ())
    });
    let types = boundary(|n| {
        zil_types::check(
            &core,
            &module.target,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|_| ())
    });
    let analyze = boundary(|n| {
        zil_analyze::analyze(
            &module,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|_| ())
    });
    let verify = boundary(|n| {
        zil_verify::verify_module(
            module.clone(),
            candidate.clone(),
            &module.target,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|_| ())
    });
    Case::new("two-input-boundaries").write("boundaries.json",format!("{{\"expand\":{expand},\"types\":{types},\"analyze\":{analyze},\"verify\":{verify},\"checks\":\"N, N-1 ResourceLimit, fresh N\",\"defaults_unchanged\":true}}\n"));
}
