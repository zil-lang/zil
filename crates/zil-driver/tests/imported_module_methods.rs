use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, Optimization, ProjectSources, Stage, compile_project};
static NEXT: AtomicUsize = AtomicUsize::new(0);
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
fn check(files: &[(&str, &str)]) -> zil_driver::CompileReport {
    compile_project(&project(files), &CompileOptions::default())
}
fn native(files: &[(&str, &str)]) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let dir = root.join(format!(
            "methods156-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        for (name, text) in files {
            fs::write(dir.join(format!("{name}.zil")), text).unwrap();
        }
        let executable = dir.join("program");
        let report = compile_project(
            &project(files),
            &CompileOptions {
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
        assert_eq!(report.verification_runs, 1);
        let output = Command::new(&executable).output().unwrap();
        fs::write(dir.join("stdout"), &output.stdout).unwrap();
        fs::write(dir.join("stderr"), &output.stderr).unwrap();
        assert_eq!(output.status.code(), Some(0), "{output:?}");
    }
}
const MODEL: &str = r#"pub struct Counter{pub value:i32}
impl Counter { pub fn get(self:&Self)->i32=>.value; pub fn add(self:&mut Self,n:i32){.value+=n} fn secret(self:&Self)->i32=>99 }
pub fn make(x:i32)->Counter=>Counter{value:x}"#;
#[test]
fn imported_methods_work_on_locals_and_borrowed_receivers() {
    native(&[
        (
            "main",
            "import model; fn main()->i32{var c=model.make(1);c.add(20);let b=&c;return b.get()-21}",
        ),
        ("model", MODEL),
    ]);
}
#[test]
fn imported_methods_work_inside_returned_owned_and_nonmove_closures() {
    native(&[
        (
            "main",
            r#"import model
fn make()->impl FnMut(i32)->i32{var c=model.make(0);return move(n:i32)->i32=>{c.add(n);return c.get()}}
fn main()->i32{var cb=make();if cb(10)!=10 || cb(32)!=42{return 1};var c=model.make(5);if true{var f=(n:i32)=>{c.add(n)};f(7)};return c.get()-12}"#,
        ),
        ("model", MODEL),
    ]);
}
#[test]
fn different_modules_with_same_spelling_keep_exact_nominal_owner() {
    native(&[
        (
            "main",
            r#"import left; import right; fn main()->i32{let a=left.make();let b=right.make();return a.get()+b.get()-42}"#,
        ),
        (
            "left",
            "pub struct C{v:i32} impl C{pub fn get(self:&Self)->i32=>.v} pub fn make()->C=>C{v:10}",
        ),
        (
            "right",
            "pub struct C{v:i32} impl C{pub fn get(self:&Self)->i32=>.v*2} pub fn make()->C=>C{v:16}",
        ),
    ]);
}
#[test]
fn public_trait_methods_from_defining_module_use_checked_signatures() {
    native(&[
        (
            "main",
            "import model; fn main()->i32{let c=model.make(14);let f=move()->i32=>c.measure();return f()-42}",
        ),
        (
            "model",
            "pub trait Measure{fn measure(self:&Self)->i32;} pub struct Counter{value:i32} impl Measure for Counter{fn measure()->i32=>.value*3} pub fn make(n:i32)->Counter=>Counter{value:n}",
        ),
    ]);
}
#[test]
fn private_methods_fields_and_incorrect_permissions_remain_rejected() {
    for source in [
        "import model; fn main()->i32{let c=model.make(1);return c.secret()}",
        "import model; fn main(){let c=model.make(1);c.add(2)}",
        "import model; fn main(){var c=model.make(1);let b=&c;b.add(2)}",
        "import model; fn main()->i32{var c=model.make(1);let b=&c;c.add(2);return b.get()}",
        "import model; fn main(){var c=model.make(1);c.add(true)}",
        "import model; fn main(){var c=model.make(1);c.add()}",
    ] {
        let report = check(&[("main", source), ("model", MODEL)]);
        assert_ne!(report.phase, zil_base::Phase::Parse, "{}", report.to_json());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
    }
}
#[test]
fn field_callable_precedence_is_not_reinterpreted_as_method() {
    native(&[
        (
            "main",
            "import model; fn main()->i32{let v=model.make();return v.run(21)-42}",
        ),
        (
            "model",
            "fn twice(x:i32)->i32=>x*2; pub struct Ops{pub run:fn(i32)->i32} pub fn make()->Ops=>Ops{run:twice}",
        ),
    ]);
}
#[test]
fn private_trait_and_private_nominal_do_not_leak_through_returned_values() {
    for model in [
        "trait Secret{fn get(self:&Self)->i32;} pub struct C{x:i32} impl Secret for C{pub fn get()->i32=>.x} pub fn make()->C=>C{x:42}",
        "struct C{x:i32} impl C{pub fn get(self:&Self)->i32=>.x} pub fn make()->C=>C{x:42}",
    ] {
        let r = check(&[
            (
                "main",
                "import model; fn main()->i32{let c=model.make();return c.get()-42}",
            ),
            ("model", model),
        ]);
        assert_ne!(r.phase, zil_base::Phase::Parse, "{}", r.to_json());
        assert_ne!(r.status, ResultCategory::Success, "{}", r.to_json());
    }
}
#[test]
fn inline_declared_public_methods_and_private_fields_compose() {
    native(&[
        (
            "main",
            "import model; fn main()->i32{let c=model.make();return c.get()-42}",
        ),
        (
            "model",
            "pub struct C { x:i32; pub fn get(self:&Self)->i32=>.x } pub fn make()->C=>C{x:42}",
        ),
    ]);
    let r = check(&[
        (
            "main",
            "import model; fn main()->i32{let c=model.make(42);return c.x}",
        ),
        (
            "model",
            MODEL
                .replace("Counter", "C")
                .replace("pub value", "value")
                .replace("value", "x")
                .as_str(),
        ),
    ]);
    assert_ne!(r.status, ResultCategory::Success, "{}", r.to_json());
}
