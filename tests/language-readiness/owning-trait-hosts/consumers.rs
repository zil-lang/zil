//! A non-Copy Package provides a static shared trait method across modules.
use super::{NEXT, project};
use std::{fs, process::Command, sync::atomic::Ordering};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_project};
use zil_ir::{Instruction, InstructionKind, ValueId};

const KEY_EQ: &str = "pub trait KeyEq { fn same(self:&Self,other:&Self)->bool; }";
const QUERY: &str =
    "import key_eq; pub fn equal<K:StoreElement+key_eq.KeyEq>(a:&K,b:&K)->bool{return a.same(b)}";
const MAIN: &str = r#"
import key_eq
import query
package Item { manages data:ByteBuffer; key:i64 }
impl key_eq.KeyEq for Item {
    fn same(self:&Self,other:&Self)->bool{return self.key==other.key}
}
fn main()->i32 {
    let buffer=bytes_new()
    if !bytes_reserve(buffer,1) || !bytes_push(buffer,65u8) {return 1}
    let first=Item{data:give buffer,key:7}
    let second=Item{data:bytes_new(),key:7}
    let third=Item{data:bytes_new(),key:8}
    if !query.equal(&first,&second) {return 2}
    if query.equal(&first,&third) {return 3}
    if bytes_view(first.data)[0]!=65u8 {return 4}
    return 0
}
"#;

fn sources() -> zil_driver::ProjectSources {
    project(&[("main", MAIN), ("key_eq", KEY_EQ), ("query", QUERY)])
}

#[test]
fn owning_package_shared_trait_reaches_generic_consumer_and_native() {
    let report = compile_project(
        &sources(),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-owning-trait-host-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            let executable = dir.join("program");
            let report = compile_project(
                &sources(),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    output: Some(executable.clone()),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            assert_eq!(report.native_codegen.as_ref().unwrap().backend, backend);
            assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
            let output = Command::new(&executable).output().unwrap();
            assert_eq!(
                output.status.code(),
                Some(0),
                "{backend:?}/{optimization:?}: {output:?}"
            );
            assert!(output.stdout.is_empty());
            assert!(output.stderr.is_empty());
            fs::remove_dir_all(dir).unwrap();
        }
    }
}

#[test]
fn owning_package_shared_trait_rejects_wrong_provider_and_bad_definitions() {
    let check = |files: &[(&str, &str)], expected: ResultCategory, code: &str| {
        let report = compile_project(&project(files), &CompileOptions::default());
        assert_eq!(report.status, expected, "{}", report.to_json());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == code),
            "{}",
            report.to_json()
        );
        report
    };
    let missing = MAIN.replace("impl key_eq.KeyEq for Item {\n    fn same(self:&Self,other:&Self)->bool{return self.key==other.key}\n}", "");
    check(
        &[("main", &missing), ("key_eq", KEY_EQ), ("query", QUERY)],
        ResultCategory::SafeReject,
        "E-GENERIC-ARGUMENT",
    );
    let wrong = MAIN
        .replace("impl key_eq.KeyEq for Item", "impl other.KeyEq for Item")
        .replace("import query", "import query\nimport other");
    check(
        &[
            ("main", &wrong),
            ("key_eq", KEY_EQ),
            ("query", QUERY),
            ("other", KEY_EQ),
        ],
        ResultCategory::SafeReject,
        "E-GENERIC-ARGUMENT",
    );
    let private = KEY_EQ.replace("pub trait", "trait");
    check(
        &[("main", MAIN), ("key_eq", &private), ("query", QUERY)],
        ResultCategory::SafeReject,
        "E-MODULE-PRIVATE",
    );
    let duplicate = MAIN.replace("fn main()->i32", "impl key_eq.KeyEq for Item {fn same(self:&Self,other:&Self)->bool{return true}}\nfn main()->i32");
    check(
        &[("main", &duplicate), ("key_eq", KEY_EQ), ("query", QUERY)],
        ResultCategory::SafeReject,
        "E-TRAIT-DUPLICATE",
    );
    let writing = MAIN.replace(
        "return self.key==other.key",
        "self.key=0;return self.key==other.key",
    );
    check(
        &[("main", &writing), ("key_eq", KEY_EQ), ("query", QUERY)],
        ResultCategory::SafeReject,
        "E-ADDRESS-PERMISSION",
    );
    let consuming = MAIN.replace(
        "return self.key==other.key",
        "let taken=*self;return taken.key==other.key",
    );
    check(
        &[("main", &consuming), ("key_eq", KEY_EQ), ("query", QUERY)],
        ResultCategory::Unsupported,
        "E-UNSUPPORTED",
    );
    let orphan = "import key_eq;import external;impl key_eq.KeyEq for external.Item {fn same(self:&Self,other:&Self)->bool{return true}}fn main(){}";
    let external = "pub package Item {manages data:ByteBuffer;key:i64}";
    check(
        &[("main", orphan), ("key_eq", KEY_EQ), ("external", external)],
        ResultCategory::Unsupported,
        "E-UNSUPPORTED",
    );
    let private_host = "import owner;fn main(){let value=owner.Item{data:bytes_new(),key:7}}";
    let owner = "import key_eq;package Item{manages data:ByteBuffer;key:i64}impl key_eq.KeyEq for Item{fn same(self:&Self,other:&Self)->bool{return self.key==other.key}}";
    check(
        &[("main", private_host), ("owner", owner), ("key_eq", KEY_EQ)],
        ResultCategory::SafeReject,
        "E-MODULE-PRIVATE",
    );

    // Definition checking must reject the newly imported Package route even
    // when no generic consumer or concrete call asks for the bad method.
    let unused = "import key_eq;package Item{manages data:ByteBuffer;key:i64}impl key_eq.KeyEq for Item{fn same(self:&Self,other:&Self)->bool{return true}}fn main(){}";
    let unused_write = unused.replace("return true", "self.key=0;return true");
    check(
        &[("main", &unused_write), ("key_eq", KEY_EQ)],
        ResultCategory::SafeReject,
        "E-ADDRESS-PERMISSION",
    );
    let owning_result = KEY_EQ.replace("->bool", "->Self");
    let result = check(
        &[("main", unused), ("key_eq", &owning_result)],
        ResultCategory::Unsupported,
        "E-UNSUPPORTED",
    );
    assert!(result.diagnostics.iter().any(|d| {
        d.message
            .contains("project Package traits require fixed shared receivers")
    }));
    let owning_parameter = KEY_EQ.replace("other:&Self", "other:Self");
    let result = check(
        &[("main", unused), ("key_eq", &owning_parameter)],
        ResultCategory::Unsupported,
        "E-UNSUPPORTED",
    );
    assert!(result.diagnostics.iter().any(|d| {
        d.message
            .contains("project Package traits require fixed shared receivers")
    }));
    let generic_trait = KEY_EQ.replace("KeyEq {", "KeyEq<T:Copy> {");
    let generic_impl = unused.replace(
        "impl key_eq.KeyEq for Item",
        "impl key_eq.KeyEq<i64> for Item",
    );
    let result = check(
        &[("main", &generic_impl), ("key_eq", &generic_trait)],
        ResultCategory::Unsupported,
        "E-UNSUPPORTED",
    );
    assert!(result.diagnostics.iter().any(|d| {
        d.message
            .contains("project Package traits require fixed shared receivers")
    }));
}

#[test]
fn owning_package_shared_trait_scalar_and_unit_methods_reach_generic_library() {
    let protocol = "pub trait Inspect{fn score(self:&Self)->i64;fn touch(self:&Self);}";
    let library = "import protocol;pub fn inspect<K:StoreElement+protocol.Inspect>(value:&K)->i64{value.touch();return value.score()}";
    let main = "import protocol;import library;package Item{manages data:ByteBuffer;key:i64}impl protocol.Inspect for Item{fn score(self:&Self)->i64{return self.key}fn touch(self:&Self){let observed=self.key}}fn main()->i32{let value=Item{data:bytes_new(),key:7};return i32(library.inspect(&value)-7)}";
    let files = [("main", main), ("protocol", protocol), ("library", library)];
    let report = compile_project(
        &project(&files),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let dir = std::env::temp_dir().join(format!(
        "zil-owning-trait-scalar-unit-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    let executable = dir.join("program");
    let report = compile_project(
        &project(&files),
        &CompileOptions {
            stop_after: Stage::Build,
            output: Some(executable.clone()),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let output = Command::new(&executable).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn owning_package_same_spelled_traits_keep_distinct_providers() {
    let main = r#"
import key_eq
import inverse
import query
import inverse_query
package Item {manages data:ByteBuffer; key:i64}
impl key_eq.KeyEq for Item {
    fn same(self:&Self,other:&Self)->bool{return self.key==other.key}
}
impl inverse.KeyEq for Item {
    fn same(self:&Self,other:&Self)->bool{return self.key!=other.key}
}
fn main()->i32 {
    let a=Item{data:bytes_new(),key:7}
    let b=Item{data:bytes_new(),key:7}
    if !query.equal(&a,&b) {return 1}
    if inverse_query.equal(&a,&b) {return 2}
    return 0
}
"#;
    let inverse_query = QUERY
        .replace("import key_eq", "import inverse")
        .replace("key_eq.KeyEq", "inverse.KeyEq");
    let files = [
        ("main", main),
        ("key_eq", KEY_EQ),
        ("query", QUERY),
        ("inverse", KEY_EQ),
        ("inverse_query", inverse_query.as_str()),
    ];
    let report = compile_project(
        &project(&files),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let dir = std::env::temp_dir().join(format!(
        "zil-distinct-owning-trait-providers-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    let executable = dir.join("program");
    let report = compile_project(
        &project(&files),
        &CompileOptions {
            stop_after: Stage::Build,
            output: Some(executable.clone()),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let output = Command::new(&executable).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn owning_package_shared_trait_raw_and_wire_reject_forged_write() {
    let text = "trait KeyEq{fn same(self:&Self,other:&Self)->bool;}package Item{manages data:ByteBuffer;key:i64}impl KeyEq for Item{fn same(self:&Self,other:&Self)->bool{return self.key==other.key}}fn main()->i32{let a=Item{data:bytes_new(),key:7};let b=Item{data:bytes_new(),key:7};if a.same(&b){return 0}return 1}";
    let source = SourceUnit::new(FileId(0), ModuleId(0), "owning-trait-host.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let typed = zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap();
    let plan = zil_analyze::analyze(&typed, &Limits::default()).unwrap();
    zil_verify::verify_module(
        typed.clone(),
        plan.clone(),
        &typed.target,
        &Limits::default(),
    )
    .unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &typed.target, &Limits::default()).unwrap();

    let mut forged = typed;
    let provider = forged
        .functions
        .iter_mut()
        .find(|f| f.name.ends_with("Item.same"))
        .unwrap();
    let block = &mut provider.blocks[0];
    let (position, address, value) = block
        .instructions
        .iter()
        .enumerate()
        .find_map(|(i, instruction)| {
            let InstructionKind::ReadAddress { address } = instruction.kind else {
                return None;
            };
            let value = instruction.result?;
            shared_i64_field(&provider.values, address, value).then_some((i, address, value))
        })
        .unwrap();
    block.instructions.insert(
        position + 1,
        Instruction {
            result: None,
            kind: InstructionKind::WriteAddress { address, value },
            span: provider.span,
        },
    );
    let mut plan = plan;
    plan.module_fingerprint = forged.fingerprint();
    let raw = zil_verify::verify_module(
        forged.clone(),
        plan.clone(),
        &forged.target,
        &Limits::default(),
    );
    let raw = raw.unwrap_err();
    assert_eq!(raw[0].code, "E-VERIFY-ADDRESS", "{raw:?}");
    assert_eq!(raw[0].category, ResultCategory::SafeReject);
    let wire = zil_ir::wire::encode_module_candidate(&forged, &plan, &Limits::default()).unwrap();
    let decoded = zil_verify::verify_module_bytes(&wire, &forged.target, &Limits::default());
    let decoded = decoded.unwrap_err();
    assert_eq!(decoded[0].code, "E-VERIFY-ADDRESS", "{decoded:?}");
    assert_eq!(decoded[0].category, ResultCategory::SafeReject);
}

fn shared_i64_field(types: &[zil_ir::Type], address: ValueId, value: ValueId) -> bool {
    matches!(
        types[address.0 as usize],
        zil_ir::Type::Address { mutable: false, .. }
    ) && types[value.0 as usize] == zil_ir::Type::I64
}
