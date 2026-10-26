//! Real source projects for exported finite templates, aliases and field privacy.
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, CompileReport, ProjectSources, Stage, compile_project};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-043-{}-{}",
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
fn success(r: &CompileReport) {
    assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
}
fn native(files: &[(&str, &str)], expected: i64) -> (String, String, String) {
    let temp = Temp::new();
    let r = compile_project(
        &project(files),
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some("result".into()),
            output: Some(temp.0.join("native")),
            emit_dir: Some(temp.0.join("ir")),
            ..CompileOptions::default()
        },
    );
    success(&r);
    for optimized in [false, true] {
        let binary = if optimized {
            let binary = temp.0.join("optimized");
            let c = Command::new("clang")
                .args(["-x", "ir", "-O2"])
                .arg(&r.artifacts["llvm"])
                .arg("-o")
                .arg(&binary)
                .output()
                .unwrap();
            assert!(c.status.success(), "{}", String::from_utf8_lossy(&c.stderr));
            binary
        } else {
            r.artifacts["executable"].clone()
        };
        let out = Command::new(binary).output().unwrap();
        assert!(out.status.success(), "{out:?}");
        assert_eq!(out.stdout, format!("{expected}\n").as_bytes());
        assert!(out.stderr.is_empty());
    }
    (
        fs::read_to_string(&r.artifacts["core"]).unwrap(),
        fs::read_to_string(&r.artifacts["typed"]).unwrap(),
        fs::read_to_string(&r.artifacts["llvm"]).unwrap(),
    )
}
fn reject(files: &[(&str, &str)], code: &str) {
    let r = compile_project(&project(files), &CompileOptions::default());
    assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
    assert_eq!(r.diagnostics[0].code, code, "{}", r.to_json());
    assert!(!r.artifacts.contains_key("executable"));
}
const MODEL: &str = "
pub enum Option<T>{None,Some(T)}
pub enum Result<E,T>{Err(E),Ok(T)}
pub struct Row<T>{pub value:T;pub weight:i64;}
pub struct Secret{value:i64;pub visible:i64;}
pub type Count=i64
pub type Score=Result<bool,Count>
pub type MaybeRow=Option<Row<Count>>
pub fn some<T>(value:T)->Option<T>{return Option.Some(value)}
pub fn empty<T>()->Option<T>{return Option.None}
pub fn ok<E,T>(value:T)->Result<E,T>{return Result.Ok(value)}
pub fn unwrap<T>(value:Option<T>,fallback:T)->T{match value{Option.Some(v)=>{return v},Option.None()=>{return fallback}}}
pub fn secret(value:i64)->Secret{return Secret{value:value,visible:2}}
pub fn read(value:Secret)->i64{return value.value}
";
#[test]
fn module043_01_public_templates_aliases_and_two_consumers_native() {
    let (core, typed, llvm) = native(
        &[
            (
                "main",
                "import model\nimport process\nfn result()->i64{let row=model.Row::<i64>{value:5000000000,weight:40};let value:model.MaybeRow=model.some(row);let score=process.convert(value);let unused:model.Option<i64>=model.empty();let extra=model.unwrap(unused,2);match score{model.Score.Ok(n)=>{return n+extra},model.Score.Err(_)=>{return 0}}}",
            ),
            ("model", MODEL),
            (
                "process",
                "import model as m\npub fn convert(value:m.MaybeRow)->m.Score{if let m.Option.Some(row)=value{return m.ok(row.value+row.weight)}return m.Result.Err(true)}",
            ),
        ],
        5000000042,
    );
    assert!(core.contains("$generic_$module21_Option"), "{core}");
    assert!(
        typed.contains("@module21:$generic_$module21_Option"),
        "{typed}"
    );
    assert!(llvm.contains("call"));
}
#[test]
fn module043_02_opaque_record_moves_and_public_field_native() {
    native(
        &[
            (
                "main",
                "import model\nfn result()->i64{let x:model.Secret=model.secret(40);let f=move(v:model.Secret)=>model.read(v);return f(x)+x.visible}",
            ),
            ("model", MODEL),
        ],
        42,
    );
}
#[test]
fn module043_03_private_declarations_reject() {
    for (root, library) in [
        (
            "import model\nfn result()->i32=>model.id(1)",
            "fn id<T>(x:T)->T=>x",
        ),
        (
            "import model\nfn result()->i32{let x:model.Hidden=model.Hidden{value:1};return x.value}",
            "struct Hidden{pub value:i32;}",
        ),
        (
            "import model\nfn result()->i32{let x:model.Count=1;return x}",
            "type Count=i32",
        ),
        (
            "import model\nfn result()->i32{let x=model.Hidden::<i32>.Some(1);return 0}",
            "enum Hidden<T>{Some(T)}",
        ),
    ] {
        reject(&[("main", root), ("model", library)], "E-MODULE-PRIVATE");
    }
}
#[test]
fn module043_04_private_field_reads_writes_construction_and_closures_reject() {
    for body in [
        "let x=model.secret(40);return x.value",
        "var x=model.secret(40);x.value=42;return 0",
        "let x=model.secret(40);let p=&x.value;return *p",
        "let x=model.Secret{value:40,visible:2};return 0",
        "let x=model.secret(40);let f=move()=>x.value;return f()",
        "let f:fn(model.Secret)->i64=(x:model.Secret)=>x.value;return f(model.secret(40))",
    ] {
        reject(
            &[
                ("main", &format!("import model\nfn result()->i64{{{body}}}")),
                ("model", MODEL),
            ],
            "E-FIELD-PRIVATE",
        );
    }
}
#[test]
fn module043_05_uninstantiated_templates_and_aliases_still_checked() {
    for library in [
        "pub fn bad<T>(x:T)->T=>missing",
        "pub enum Bad<T>{One(Missing)}",
        "pub type A=B\ntype B=A",
    ] {
        reject(
            &[
                ("main", "import model\nfn result()->i32=>0"),
                ("model", library),
            ],
            "E-GENERIC-ARGUMENT",
        );
    }
}
#[test]
fn module043_06_project_expansion_budget_stops_without_executable() {
    let p = project(&[
        ("main", "import model\nfn result()->i32=>model.id(42)"),
        ("model", "pub fn id<T>(x:T)->T=>x"),
    ]);
    let r = compile_project(
        &p,
        &CompileOptions {
            limits: Limits {
                max_analysis_steps: 100,
                ..Limits::default()
            },
            ..CompileOptions::default()
        },
    );
    assert_eq!(r.status, ResultCategory::ResourceLimit, "{}", r.to_json());
    assert!(!r.artifacts.contains_key("executable"));
}
#[test]
fn module043_07_reexports_aliases_explicit_calls_and_shared_module_identity() {
    let mut p = project(&[
        (
            "main",
            "import facade as f\nimport model as m\nimport duplicate as same\nfn result()->i64{let a:f.Pair=f.Pair{left:40,right:2};let x:f.Maybe=m.some::<same.Pair>(a);match x{f.Maybe.Some(p)=>{return p.left+p.right},f.Maybe.None()=>{return 0}}}",
        ),
        (
            "model",
            "pub struct Pair{pub left:i64;pub right:i64;}\npub enum Option<T>{None,Some(T)}\npub fn some<T>(x:T)->Option<T> => Option.Some(x)",
        ),
        (
            "facade",
            "import model\npub type Pair=model.Pair\npub type Maybe=model.Option<Pair>",
        ),
    ]);
    p.paths.insert(vec!["duplicate".into()], ModuleId(21));
    let temp = Temp::new();
    let r = compile_project(
        &p,
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some("result".into()),
            output: Some(temp.0.join("same")),
            ..CompileOptions::default()
        },
    );
    success(&r);
    let out = Command::new(&r.artifacts["executable"]).output().unwrap();
    assert!(out.status.success());
    assert_eq!(out.stdout, b"42\n");
}
#[test]
fn module043_08_same_spelling_in_different_modules_is_not_the_same_type() {
    reject(
        &[
            (
                "main",
                "import left\nimport right\nfn result()->i32{return left.read(right.Pair{value:42})}",
            ),
            (
                "left",
                "pub struct Pair{pub value:i32;}\npub fn read(x:Pair)->i32=>x.value",
            ),
            ("right", "pub struct Pair{pub value:i32;}"),
        ],
        "E-TYPE-MISMATCH",
    );
    reject(
        &[
            (
                "main",
                "import left\nimport right\nfn result()->i32{return left.read(right.Option::<i32>.Some(42))}",
            ),
            (
                "left",
                "pub enum Option<T>{None,Some(T)}\npub fn read(x:Option<i32>)->i32{match x{Option.Some(v)=>{return v},Option.None()=>{return 0}}}",
            ),
            ("right", "pub enum Option<T>{None,Some(T)}"),
        ],
        "E-GENERIC-ARGUMENT",
    );
}
#[test]
fn module043_09_template_body_uses_definition_scope_and_checks_foreign_private_field() {
    native(
        &[
            (
                "main",
                "import model\nfn helper(x:i64)->i64=>100\nfn result()->i64=>model.value::<i64>(40)+2",
            ),
            (
                "model",
                "fn helper<T>(x:T)->T=>x\npub fn value<T>(x:T)->T=>helper(x)",
            ),
        ],
        42,
    );
    reject(
        &[
            (
                "main",
                "import model\nimport expose\nfn result()->i64=>expose.leak(model.secret())",
            ),
            (
                "model",
                "pub struct Locked<T>{value:T;}\npub fn secret()->Locked<i64>{return Locked::<i64>{value:42}}",
            ),
            (
                "expose",
                "import model\npub fn leak<T>(x:model.Locked<T>)->T{return x.value}",
            ),
        ],
        "E-FIELD-PRIVATE",
    );
}
#[test]
fn module043_10_ct_opaque_results_are_copies_and_private_queries_reject() {
    native(
        &[
            (
                "main",
                "import model\nfn result()->i64{let x=#(model.secret(40));return model.read(x)+x.visible}",
            ),
            ("model", MODEL),
        ],
        42,
    );
    reject(
        &[
            (
                "main",
                "import model\nfn result()->i64=>#(model.secret(42).value)",
            ),
            ("model", MODEL),
        ],
        "E-FIELD-PRIVATE",
    );
    reject(
        &[
            (
                "main",
                "import model\nfn result()->i64{let x=#(model.Secret{value:40,visible:2});return model.read(x)}",
            ),
            ("model", MODEL),
        ],
        "E-FIELD-PRIVATE",
    );
}
#[test]
fn module043_11_lexical_bindings_can_shadow_imports_and_type_names() {
    native(
        &[
            (
                "main",
                "import model\nstruct Local{value:i64;}\nfn result()->i64{let model=Local{value:40};let f=(Option:i64)=>Option+2;return f(model.value)}",
            ),
            ("model", MODEL),
        ],
        42,
    );
}
