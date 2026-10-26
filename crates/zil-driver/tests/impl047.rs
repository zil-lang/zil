//! Real resource sources cross parsing, CT rewriting, independent verification
//! and native code. Cleanup traces are observable external calls, not hand IR.
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, CompileReport, ProjectSources, Stage, compile_project, compile_source,
};
use zil_ir::PassingMode;

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-047-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn project(files: &[(&str, &str)]) -> ProjectSources {
    ProjectSources {
        root: ModuleId(20),
        modules: files
            .iter()
            .enumerate()
            .map(|(i, (name, text))| {
                SourceUnit::new(
                    FileId(100 + i as u32),
                    ModuleId(20 + i as u32),
                    format!("{name}.zil"),
                    *text,
                )
            })
            .collect(),
        paths: files
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, (name, _))| {
                (
                    name.split('.').map(str::to_owned).collect(),
                    ModuleId(20 + i as u32),
                )
            })
            .collect::<BTreeMap<_, _>>(),
    }
}
fn success(report: &CompileReport) {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
}
fn native(files: &[(&str, &str)], expected: &str) -> (String, String) {
    let temp = Temp::new();
    let report = compile_project(
        &project(files),
        &CompileOptions {
            stop_after: Stage::Build,
            output: Some(temp.0.join("plain")),
            emit_dir: Some(temp.0.join("ir")),
            ..CompileOptions::default()
        },
    );
    success(&report);
    for optimized in [false, true] {
        let binary = if optimized {
            let path = temp.0.join("optimized");
            let out = Command::new("clang")
                .args(["-x", "ir", "-O2"])
                .arg(&report.artifacts["llvm"])
                .arg("-o")
                .arg(&path)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            path
        } else {
            report.artifacts["executable"].clone()
        };
        let out = Command::new(binary).output().unwrap();
        assert!(out.status.success(), "{out:?}");
        assert_eq!(out.stdout, expected.as_bytes());
        assert!(out.stderr.is_empty());
    }
    let typed = fs::read_to_string(&report.artifacts["typed"]).unwrap();
    let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
    assert!(
        !llvm.contains("@malloc"),
        "inline resource acquired an implicit heap allocation"
    );
    (typed, llvm)
}
const PRELUDE: &str = r#"
extern "C" fn putchar(value:i32)->i32;
resource Token { code:i32 }
impl Drop for Token { fn drop(self:Self) { unsafe { putchar(.code) } } }
fn make(code:i32)->Token contract {result(create)} {return Token{code}}
fn read(token:Token)->i32 {return token.code}
fn consume(give token:Token) {}
"#;
fn source_report(text: &str, stage: Stage) -> CompileReport {
    compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "resources.zil", text),
        &CompileOptions {
            stop_after: stage,
            ..CompileOptions::default()
        },
    )
}
fn reject(text: &str, code: &str) {
    let report = source_report(text, Stage::Verify);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        report.diagnostics.iter().any(|d| d.code == code),
        "{}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
}

#[test]
fn resource047_01_factory_describe_give_and_cleanup_native() {
    let (typed, llvm) = native(
        &[("main", include_str!("../../../examples/impl047/main.zil"))],
        "CB",
    );
    for op in [
        "MakeManaged",
        "DescribeLocal",
        "ReadDesc",
        "WriteDesc",
        "TakeLocal",
        "DropManaged",
    ] {
        assert!(typed.contains(op), "{op} missing");
    }
    assert!(typed.contains("result_mode: Create"));
    assert!(typed.contains("result_mode: Give"));
    assert!(
        !llvm.contains(" noalias "),
        "descriptor is not exclusive Address"
    );
}
#[test]
fn resource047_02_all_normal_exits_and_conditional_transfer_native() {
    native(
        &[(
            "main",
            include_str!("../../../examples/impl047/lifecycle.zil"),
        )],
        "BACDFAGHJJKL",
    );
}
#[test]
fn resource047_03_temporaries_short_circuit_and_conditional_results_native() {
    let text = format!(
        r#"{PRELUDE}
fn yes(token:Token)->bool {{return token.code>0}}
fn read2(token:Token,add:i32)->i32 {{return token.code+add}}
fn main()->i32 {{
    read2(make(65),if yes(make(73)) {{1}} else {{2}})
    let both=yes(make(66)) && yes(make(67))
    let skipped=false && yes(make(68))
    let n=if true {{read(make(69))}} else {{read(make(70))}}
    let held=if false {{make(71)}} else {{make(72)}}
    return read(held)-72
}}
"#
    );
    native(&[("main", &text)], "IACBEH");
}
#[test]
fn resource047_04_pattern_conditions_end_temporary_scope_native() {
    let text = format!(
        r#"{PRELUDE}
enum Maybe {{Some(i32),None}}
fn value(token:Token,more:bool)->Maybe {{
    if more {{return Maybe.Some(token.code)}}
    return Maybe.None
}}
fn main()->i32 {{
    if let Maybe.Some(n)=value(make(74),true) {{make(75)}}
    var count=0
    while let Maybe.Some(n)=value(make(76),count<2) {{
        let item=make(77+count)
        count+=1
    }}
    match value(make(79),true) {{
        Maybe.Some(n)=>{{make(80)}},
        Maybe.None()=>{{make(81)}}
    }}
    return 0
}}
"#
    );
    native(&[("main", &text)], "JKLMLNLOP");
}
#[test]
fn resource047_05_explicit_descriptor_and_default_cleanup_native() {
    let text = r#"
resource Plain {value:i32}
fn make()->Plain contract {result(create)} {return Plain{value:42}}
fn read(value:Ref<Plain>)->i32 {return value.value}
fn main()->i32 {let x:Plain=make();let y=x;return read(y)-42}
"#;
    native(&[("main", text)], "");
}
#[test]
fn resource047_06_source_transfer_errors_reject() {
    reject(
        &format!(
            "{PRELUDE}\nfn main()->i32{{let x=make(65);let alias=x;consume(give alias);return 0}}"
        ),
        "E-GIVE-DESCRIPTOR",
    );
    reject(
        "fn main()->i32{let x=1;let y=give x;return y}",
        "E-GIVE-TYPE",
    );
    reject(
        &format!("{PRELUDE}\nfn main()->i32{{let x=make(65);consume(x);return 0}}"),
        "E-TYPE-MISMATCH",
    );
    reject(
        &format!("{PRELUDE}\nfn main()->i32{{let x=make(65);consume(give x);return x.code}}"),
        "E-UNINITIALIZED",
    );
}
#[test]
fn resource047_07_malformed_drop_and_result_contracts_reject() {
    for text in [
        "resource R{x:i32} impl Drop for R{fn drop(give self:Self){}}",
        "resource R{x:i32} impl Drop for R{fn drop(self:&Self){}}",
        "resource R{x:i32} impl Drop for R{fn close(self:Self){}}",
        "struct R{x:i32} impl Drop for R{fn drop(self:Self){}}",
    ] {
        reject(text, "E-DROP-SIGNATURE");
    }
    reject(
        "resource R{x:i32} impl Drop for R{fn drop(self:Self){}} impl Drop for R{fn drop(self:Self){}}",
        "E-DROP-DUPLICATE",
    );
    reject(
        "resource R{x:i32} fn make()->R contract{result(create);result(give)}{return R{x:1}}",
        "E-RESOURCE-RESULT-MODE",
    );
}
const LIBRARY: &str = r#"
extern "C" fn putchar(value:i32)->i32;
pub resource Token {code:i32}
impl Drop for Token {fn drop(self:Self){unsafe{putchar(.code)}}}
pub fn make(code:i32)->Token contract{result(create)}{return Token{code}}
pub fn read(token:Token)->i32{return token.code}
pub fn change(token:Token,code:i32){token.code=code}
pub fn forward(give token:Token)->Token contract{result(give)}{return give token}
"#;
#[test]
fn resource047_08_exported_opaque_factory_and_hidden_drop_native() {
    let main = "import model\nfn main()->i32{let x:model.Token=model.make(65);model.change(x,66);let y=model.forward(give x);return model.read(y)-66}";
    native(&[("main", main), ("model", LIBRARY)], "B");
    for body in [
        "let x=model.make(65);return x.code",
        "let x=model.make(65);x.code=66;return 0",
        "let x=model.Token{code:65};return 0",
    ] {
        let text = format!("import model\nfn main()->i32{{{body}}}");
        let report = compile_project(
            &project(&[("main", &text), ("model", LIBRARY)]),
            &CompileOptions::default(),
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
                .any(|d| d.code == "E-FIELD-PRIVATE"),
            "{}",
            report.to_json()
        );
    }
}
#[test]
fn resource047_09_result_mode_is_exported_and_invalidates_signature() {
    let mut fingerprints = Vec::new();
    for (word, mode) in [("create", PassingMode::Create), ("give", PassingMode::Give)] {
        let library = format!(
            "pub resource Token{{code:i32}} pub fn make()->Token contract{{result({word})}}{{return Token{{code:65}}}}"
        );
        let report = compile_project(
            &project(&[
                ("main", "import model\nfn main()->i32{return 0}"),
                ("model", &library),
            ]),
            &CompileOptions {
                stop_after: Stage::Types,
                ..CompileOptions::default()
            },
        );
        success(&report);
        let interface = report
            .scalar_interfaces
            .iter()
            .find(|i| i.module == ModuleId(21))
            .unwrap();
        let make = interface.exports.iter().find(|f| f.name == "make").unwrap();
        assert_eq!(make.result_mode, mode);
        fingerprints.push(interface.signature_fingerprint);
    }
    assert_ne!(fingerprints[0], fingerprints[1]);
}

#[test]
fn resource047_10_old_descriptions_and_incoming_create_reject_independently() {
    let reject_resource = |text: &str| {
        success(&source_report(text, Stage::Types));
        let report = source_report(text, Stage::Verify);
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
                .any(|d| { matches!(d.code.as_str(), "E-ANALYZE-IR" | "E-VERIFY-RESOURCE") }),
            "{}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    };
    for body in [
        "let x=make(65);let alias=x;consume(give x);return alias.code",
        "let x=make(65);let alias=x;let y=give x;return alias.code",
    ] {
        let text = format!("{PRELUDE}\nfn main()->i32{{{body}}}");
        reject_resource(&text);
    }
    let text = format!(
        "{PRELUDE}\nfn false_create(give x:Token)->Token contract{{result(create)}}{{return give x}}"
    );
    reject_resource(&text);
}

#[test]
fn resource047_11_unimplemented_resource_shapes_have_explicit_boundaries() {
    for text in [
        "resource R{x:i32} fn escape(x:R)->R{return x}",
        "resource R{x:i32} struct Outer{inner:R}",
        "resource R{x:i32} fn read(x:&R)->i32{return x.x}",
        "resource R{x:i32} fn main()->i32{var x=R{x:1};x=R{x:2};return 0}",
        "resource R{x:i32} fn main()->i32{let x=R{x:1};let xs=(x,1);return 0}",
    ] {
        let report = source_report(text, Stage::Verify);
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
}

#[test]
fn resource047_12_resource_pipeline_and_range_endpoint_temps_native() {
    let text = format!(
        r#"{PRELUDE}
fn offset(token:Token)->i32 {{return token.code-84}}
fn main()->i32 {{
    make(81) |> read
    let x=make(82)
    give x |> consume
    for n in offset(make(84))..offset(make(86)) {{let item=make(87+n)}}
    return 0
}}
"#
    );
    native(&[("main", &text)], "QRVTWX");
}

#[test]
fn resource047_13_factory_temps_end_before_new_local_owns_result_native() {
    let text = format!(
        r#"{PRELUDE}
fn main()->i32 {{
    let outer=make(65)
    let first=make(make(66).code+1)
    let second=Token{{code:make(68).code+1}}
    return first.code+second.code-136
}}
"#
    );
    native(&[("main", &text)], "BDECA");
}

#[test]
fn resource047_14_i64_resource_field_context_native() {
    let text = r#"
extern "C" fn putchar(value:i32)->i32;
resource Wide {value:i64}
impl Drop for Wide {
    fn drop(self:Self) {
        unsafe {
            if .value==1 {putchar(65)} else {putchar(66)}
        }
    }
}
fn main()->i32 {
    let small=Wide{value:1}
    let large=Wide{value:5000000000}
    if small.value==1 && large.value==5000000000 {return 0}
    return 1
}
"#;
    native(&[("main", text)], "BA");
}

#[test]
fn resource047_15_array_resource_field_context_native() {
    let text = r#"
extern "C" fn putchar(value:i32)->i32;
resource Block {cells:[i64;2]}
impl Drop for Block {
    fn drop(self:Self) {
        unsafe {
            if .cells[0]+.cells[1]==7 {putchar(67)} else {putchar(88)}
        }
    }
}
fn main()->i32 {
    let block=Block{cells:[3,4]}
    if block.cells[0]==3 && block.cells[1]==4 {return 0}
    return 1
}
"#;
    native(&[("main", text)], "C");
}
