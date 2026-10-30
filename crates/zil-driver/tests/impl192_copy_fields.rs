//! Actual inline Copy fields retain both their parent and field permissions.
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const SIMPLE: &str =
    "resource R{n:i32}package P{manages r:R;x:i32;y:i32}fn make()->P{return P{r:R{n:1},x:20,y:1}}";

fn directory() -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!(
        "fields192-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}
fn compile(files: &[(&str, &str)], options: CompileOptions, dir: &Path) -> CompileReport {
    let project = ProjectSources {
        root: ModuleId(0),
        modules: files
            .iter()
            .enumerate()
            .map(|(index, (name, text))| {
                fs::write(dir.join(format!("{name}.zil")), text).unwrap();
                SourceUnit::new(
                    FileId(index as u32),
                    ModuleId(index as u32),
                    format!("{name}.zil"),
                    *text,
                )
            })
            .collect(),
        paths: files
            .iter()
            .enumerate()
            .skip(1)
            .map(|(index, (name, _))| (vec![name.to_string()], ModuleId(index as u32)))
            .collect::<BTreeMap<_, _>>(),
    };
    let report = compile_project(
        &project,
        &CompileOptions {
            emit_dir: Some(dir.join("phases")),
            ..options
        },
    );
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    report
}
fn check(source: &str) -> CompileReport {
    compile(&[("main", source)], CompileOptions::default(), &directory())
}
fn native(files: &[(&str, &str)], output: &[u8], code: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = directory();
        let program = dir.join("program");
        let report = compile(
            files,
            CompileOptions {
                stop_after: Stage::Build,
                optimization,
                output: Some(program.clone()),
                ..CompileOptions::default()
            },
            &dir,
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let result = Command::new(program).output().unwrap();
        fs::write(dir.join("stdout"), &result.stdout).unwrap();
        fs::write(dir.join("stderr"), &result.stderr).unwrap();
        fs::write(dir.join("exit.txt"), format!("{:?}", result.status.code())).unwrap();
        assert_eq!(result.status.code(), Some(code), "{result:?}");
        assert_eq!(result.stdout, output, "{result:?}");
        assert!(result.stderr.is_empty(), "{result:?}");
    }
}
fn conflict(body: &str) {
    let source = format!("{SIMPLE}fn main()->i32{{var state=make();{body}}}");
    let report = check(&source);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{source}: {}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Verify, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-BORROW-CONFLICT"),
        "{}",
        report.to_json()
    );
}

#[test]
fn actual_scalar_field_reborrows_end_at_last_use_before_parent_move() {
    let source = format!(
        "{SIMPLE}fn read(p:&i32)->i32=>*p;fn bump(p:&mut i32)->i32{{*p+=1;return *p}}fn main()->i32{{var state=make();let p=&mut state.x;let old=read(p);let a=bump(p);let b=bump(p);state.y+=old;let moved=state;return moved.x+moved.y+old+a+b-106}}"
    );
    native(&[("main", &source)], b"", 0);
}

#[test]
fn separate_inline_fields_have_independent_exclusive_windows() {
    let source = format!(
        "{SIMPLE}fn main()->i32{{var state=make();let left=&mut state.x;let right=&mut state.y;*left+=2;*right+=3;return *left+*right-26}}"
    );
    native(&[("main", &source)], b"", 0);
    let source = format!(
        "{SIMPLE}fn main()->i32{{var state=make();let left=&mut state.x;let right=&state.y;*left+=*right;let second=state.y;return *left+second-22}}"
    );
    native(&[("main", &source)], b"", 0);
}

#[test]
fn shared_fields_can_alias_without_becoming_mutable() {
    let source = format!(
        "{SIMPLE}fn main()->i32{{let state=make();let a=&state.x;let b=&state.x;return *a+*b+state.x-60}}"
    );
    native(&[("main", &source)], b"", 0);
    for body in [
        "let state=make();let p=&mut state.x;*p+=1;",
        "var state=make();let p=&state.x;*p+=1;",
        "var state=make();let p=&state.x;let q=&mut *p;*q+=1;",
    ] {
        let report = check(&format!("{SIMPLE}fn main(){{{body}}}"));
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Types, "{}", report.to_json());
    }
}

#[test]
fn direct_reads_writes_and_other_loans_cannot_cross_the_same_field_window() {
    for body in [
        "let p=&mut state.x;let value=state.x;return *p+value;",
        "let p=&mut state.x;state.x=4;return *p;",
        "let p=&state.x;state.x+=1;return *p;",
        "let p=&mut state.x;let q=&state.x;return *p+*q;",
        "let p=&mut state.x;let q=&mut state.x;*q+=1;return *p+*q;",
        "let p=&state.x;let q=&mut state.x;*q+=1;return *p;",
    ] {
        conflict(body);
    }
}

#[test]
fn moving_replacing_or_destroying_the_whole_parent_invalidates_its_field_address() {
    for body in [
        "let p=&state.x;let moved=state;return *p+moved.y;",
        "let p=&mut state.x;let moved=state;return *p+moved.y;",
    ] {
        conflict(body);
    }
    let source = format!(
        "{SIMPLE}fn eat(p:P){{}}fn main()->i32{{var state=make();let p=&state.x;eat(state);return *p}}"
    );
    let report = check(&source);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-BORROW-CONFLICT"),
        "{}",
        report.to_json()
    );
}

#[test]
fn finite_record_and_array_fields_use_actual_inline_storage() {
    native(
        &[(
            "main",
            "struct Pair{a:i32;b:i32}resource R{n:i32}package P{manages r:R;pair:Pair;items:[i32;2]}fn main()->i32{var state=P{r:R{n:0},pair:Pair{a:20,b:1},items:[30,2]};let p=&mut state.pair;p.a+=p.b;let value=p.a;let items=&mut state.items;items[0]+=items[1];let extra=&state.pair.b;return value+items[0]+*extra-54}",
        )],
        b"",
        0,
    );
}

#[test]
fn whole_parent_overwrite_keeps_its_existing_unpaid_owner_gate() {
    // Whole-Package overwrite already rejects an unpaid owner, with or
    // without a field loan. This is not credited as a new loan proof.
    for body in [
        "state=make();return state.x;",
        "let p=&state.x;state=make();return *p;",
    ] {
        let report = check(&format!(
            "{SIMPLE}fn main()->i32{{var state=make();{body}}}"
        ));
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Analyze, "{}", report.to_json());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.message.contains("unpaid resource")),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn captured_field_addresses_meet_current_callback_arguments() {
    let source = format!(
        "{SIMPLE}fn main()->i32{{var state=make();let captured=&state.x;let f=(p:&mut i32)=>{{*p+=1;return *captured+*p}};return f(&mut state.y)-22}}"
    );
    native(&[("main", &source)], b"", 0);
    let report = check(&source.replace("&mut state.y", "&mut state.x"));
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-BORROW-CONFLICT"),
        "{}",
        report.to_json()
    );
}

#[test]
fn owning_package_captures_borrow_the_actual_environment_field() {
    let source = format!(
        "{SIMPLE}fn main()->i32{{var state=make();var f=move()=>{{let p=&mut state.x;*p+=1;return *p}};return f()+f()-43}}"
    );
    native(&[("main", &source)], b"", 0);
    let ordinary = check(&source.replace("move()=>", "()=>"));
    assert_eq!(
        ordinary.status,
        ResultCategory::Unsupported,
        "{}",
        ordinary.to_json()
    );
    assert_eq!(ordinary.phase, Phase::Types);
    assert!(
        ordinary
            .diagnostics
            .iter()
            .any(|d| d.message.contains("Package capture requires explicit move")),
        "{}",
        ordinary.to_json()
    );
    let readonly = check(&source.replace("var f=", "let f="));
    assert_eq!(
        readonly.status,
        ResultCategory::SafeReject,
        "{}",
        readonly.to_json()
    );
    assert_eq!(readonly.phase, Phase::Types);
    let overlap = check(&source.replace(
        "*p+=1;return *p",
        "let observed=state.x;*p+=1;return *p+observed",
    ));
    assert_eq!(
        overlap.status,
        ResultCategory::SafeReject,
        "{}",
        overlap.to_json()
    );
    assert_eq!(overlap.phase, Phase::Verify);
    assert!(
        overlap
            .diagnostics
            .iter()
            .any(|d| d.code == "E-BORROW-CONFLICT"),
        "{}",
        overlap.to_json()
    );
}

#[test]
fn top_level_copy_fields_do_not_claim_new_nested_field_separation() {
    let report = check(
        "struct Pair{a:i32;b:i32}resource R{n:i32}package P{manages r:R;pair:Pair}fn main()->i32{var state=P{r:R{n:0},pair:Pair{a:20,b:1}};let p=&mut state.pair.a;let q=&mut state.pair.b;*p+=1;*q+=1;return *p+*q}",
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-BORROW-CONFLICT"),
        "{}",
        report.to_json()
    );
}

#[test]
fn byte_owner_sibling_operations_preserve_the_copy_field_window() {
    native(
        &[(
            "main",
            "package P{manages bytes:ByteBuffer;count:i32}fn main()->i32{var state=P{bytes:bytes_new(),count:40};let count=&mut state.count;if !bytes_reserve(state.bytes,1){return 8}bytes_push(state.bytes,2u8);let view=bytes_view(state.bytes);*count+=i32(view[0]);return *count-42}",
        )],
        b"",
        0,
    );
    native(
        &[(
            "main",
            "package P{manages bytes:ByteBuffer;count:i32}fn main()->i32{var state=P{bytes:bytes_new(),count:42};let count=&state.count;let bytes=give state.bytes;return *count+i32(bytes_len(bytes))-42}",
        )],
        b"",
        0,
    );
}

#[test]
fn generic_state_calls_an_actual_callback_field_with_its_copy_sibling() {
    native(
        &[
            (
                "main",
                "import factory;import model;fn main()->i32{return model.run(model.make(factory.make()),3)-7}",
            ),
            (
                "factory",
                "pub fn make()->impl FnMut(&mut i32)->i32{var calls=0;return move(p:&mut i32)=>{calls+=1;*p+=calls;return *p}}",
            ),
            (
                "model",
                "pub package State<F>{manages bytes:ByteBuffer;callback:F;value:i32}pub fn make<F:FnMut(&mut i32)->i32>(callback:F)->State<F>{return State::<F>{bytes:bytes_new(),callback:callback,value:1}}pub fn run<F:FnMut(&mut i32)->i32>(state:State<F>,count:i32)->i32{var current=state;var i=0;while i<count{let callback=&mut current.callback;let value=&mut current.value;callback(value);i+=1}return current.value}",
            ),
        ],
        b"",
        0,
    );
}

#[test]
fn helper_call_arguments_keep_field_identity_and_shared_permissions() {
    let prefix = format!(
        "{SIMPLE}fn apply<F:Fn(&mut i32,&i32)->i32>(f:&F,a:&mut i32,b:&i32)->i32{{return f(a,b)}}"
    );
    let source = format!(
        "{prefix}fn main()->i32{{var state=make();let f=(a:&mut i32,b:&i32)=>{{*a+=*b;return *a}};let result=apply(&f,&mut state.x,&state.y);return result+state.x-42}}"
    );
    native(&[("main", &source)], b"", 0);
    let report = check(&source.replace("&state.y", "&state.x"));
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-BORROW-CONFLICT"),
        "{}",
        report.to_json()
    );
}

#[test]
fn error_and_early_exit_cleanup_never_duplicate_the_parent_owner() {
    let source = "extern \"C\" fn putchar(n:i32)->i32;resource R{n:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.n)}}}package P{manages r:R;x:i32}fn run(fail:bool)->bool!i32{var state=P{r:R{n:65},x:40};let p=&mut state.x;*p+=2;if fail{return true}return *p}fn main()->i32{let first=run(true);let second=run(false);return 0}";
    native(&[("main", source)], b"AA", 0);
}

#[test]
fn finite_field_loans_follow_branch_and_loop_last_use() {
    let source = format!(
        "{SIMPLE}fn run(flag:bool)->i32{{var state=make();var total=0;var i=0;while i<3{{let p=&mut state.x;if flag{{*p+=1}}else{{*p+=2}}total+=*p;i+=1}}return total+state.x}}fn main()->i32{{return run(true)+run(false)-187}}"
    );
    native(&[("main", &source)], b"", 0);
    conflict("let p=&mut state.x;if state.y==1{let v=state.x;state.y=v}return *p;");
}

#[test]
fn ordinary_shared_heads_are_admitted_while_bytebuffer_heads_remain_closed() {
    // Preserve all three original sources. 224 admits the ordinary complete
    // Package and its finite Copy resource head, but not a ByteBuffer head.
    for (source, category, phase) in [
        (format!("{SIMPLE}fn main(){{var state=make();let p=&state;}}"), ResultCategory::Success, Phase::Verify),
        (format!("{SIMPLE}fn main(){{var state=make();let p=&state.r;}}"), ResultCategory::Success, Phase::Verify),
        ("package P{manages bytes:ByteBuffer;n:i32}fn main(){var state=P{bytes:bytes_new(),n:1};let p=&state.bytes;}".to_string(), ResultCategory::Unsupported, Phase::Types),
    ] {
        let report = check(&source);
        assert_eq!(report.status, category, "{source}: {}", report.to_json());
        assert_eq!(report.phase, phase, "{}", report.to_json());
        if category == ResultCategory::Success {
            assert_eq!(report.verification_runs, 1, "{}", report.to_json());
        }
    }
}

#[test]
fn actual_four_module_consumer_uses_direct_state_field_addresses() {
    let report = compile(
        &[
            (
                "main",
                include_str!("../../../examples/mainline/fields192/main.zil"),
            ),
            (
                "factory",
                include_str!("../../../examples/mainline/fields192/factory.zil"),
            ),
            (
                "helpers",
                include_str!("../../../examples/mainline/fields192/helpers.zil"),
            ),
            (
                "state",
                include_str!("../../../examples/mainline/fields192/state.zil"),
            ),
        ],
        CompileOptions::default(),
        &directory(),
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    assert_eq!(report.source_files.len(), 4);
}

#[test]
fn shared_entry_may_borrow_a_fresh_local_package_without_mutating_its_environment() {
    let source = format!(
        "{SIMPLE}fn main()->i32{{let amount=20;let f=move()=>{{var local=make();let p=&mut local.x;*p+=amount;return *p}};return f()+f()-80}}"
    );
    native(&[("main", &source)], b"", 0);
}

#[test]
fn original_user_resource_sibling_take_refill_preserves_the_copy_window() {
    // Exact independent review input, including the Drop observations.
    let source = "extern \"C\" fn putchar(n:i32)->i32;resource R{n:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.n)}}}package P{manages owner:R;count:i64}fn main()->i32{var state=P{owner:R{n:65},count:2};let p=&mut state.count;if true{let old=give state.owner;state.owner=R{n:66};*p+=i64(old.n)}return i32(*p)-67}";
    native(&[("main", source)], b"AB", 0);
}

#[test]
fn precise_resource_siblings_do_not_revive_old_descriptions_or_release_the_parent() {
    let source = format!(
        "{SIMPLE}fn main()->i32{{var state=make();let p=&mut state.x;let stale=state.r;let old=give state.r;state.r=R{{n:2}};return *p+stale.n+old.n}}"
    );
    let report = check(&source);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Analyze, "{}", report.to_json());
    assert!(
        report.diagnostics.iter().any(|d| d.code == "E-ANALYZE-IR"
            && d.message
                .contains("head transition invalidates a description required later")),
        "{}",
        report.to_json()
    );
    conflict(
        "let p=&mut state.x;let old=give state.r;state.r=R{n:2};let moved=state;return *p+old.n+moved.y;",
    );
    conflict(
        "let p=&mut state.x;let old=give state.r;state.r=R{n:2};let alias=&state.x;return *p+*alias+old.n;",
    );
}
