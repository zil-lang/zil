//! Non-Copy Packages provide ordinary shared and exclusive trait methods across modules.
use super::{NEXT, project};
use std::{fs, process::Command, sync::atomic::Ordering};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_project};
use zil_ir::{Instruction, InstructionKind, ValueId};

#[path = "collected_pipeline/consumers.rs"]
mod collected_pipeline;
#[path = "owned_byte_views/consumers.rs"]
mod owned_byte_views;
#[path = "owned_pipeline/safety.rs"]
mod owned_pipeline;
#[path = "owned_results/consumers.rs"]
mod owned_results;
#[path = "store_push_growing/consumers.rs"]
mod store_push_growing;

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
        ResultCategory::SafeReject,
        "E-TRAIT-SIGNATURE",
    );
    assert_eq!(result.verification_runs, 0);
    let owning_parameter = KEY_EQ.replace("other:&Self", "other:Self");
    let result = check(
        &[("main", unused), ("key_eq", &owning_parameter)],
        ResultCategory::Unsupported,
        "E-UNSUPPORTED",
    );
    assert!(result.diagnostics.iter().any(|d| {
        d.message
            .contains("project Package traits require fixed borrowed receivers")
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
            .contains("project Package traits require fixed borrowed receivers")
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

const MUTABLE_PROTOCOL: &str = r#"
pub trait Control {
 fn reset(self:&mut Self);
 fn replace(self:&mut Self,value:i64)->i64;
 fn read(self:&Self)->i64;
}
"#;
const MUTABLE_MODEL: &str = r#"
import control
pub package Item {manages data:ByteBuffer; key:i64}
impl control.Control for Item {
 fn reset(self:&mut Self){self.key=0}
 fn replace(self:&mut Self,value:i64)->i64{let old=self.key;self.key=value;return old}
 fn read(self:&Self)->i64{return self.key}
}
pub enum Error{Allocation}
pub fn make()->Error!Item {
 let data=bytes_new()
 if !bytes_reserve(data,1) || !bytes_push(data,65u8){return Error.Allocation}
 return Item{data:give data,key:9}
}
pub fn byte(value:&Item)->u8{return bytes_view(value.data)[0]}
"#;
const MUTABLE_LIBRARY: &str = r#"
import control
import model
pub fn ordinary(value:&mut model.Item)->i64{value.reset();return value.replace(11)}
pub fn reset<K:StoreElement+control.Control>(value:&mut K){value.reset()}
pub fn replace<K:StoreElement+control.Control>(value:&mut K,n:i64)->i64{return value.replace(n)}
pub fn read<K:StoreElement+control.Control>(value:&K)->i64{return value.read()}
"#;
const MUTABLE_MAIN: &str = r#"
import model
import library
fn main()->model.Error!i32 {
 var value=model.make()?
 if library.ordinary(&mut value)!=0{return 1}
 if library.read(&value)!=11{return 2}
 library.reset(&mut value)
 if library.replace(&mut value,42)!=0{return 3}
 if library.read(&value)!=42 || model.byte(&value)!=65u8{return 4}
 return 0
}
"#;
fn mutable_files() -> [(&'static str, &'static str); 4] {
    [
        ("main", MUTABLE_MAIN),
        ("control", MUTABLE_PROTOCOL),
        ("model", MUTABLE_MODEL),
        ("library", MUTABLE_LIBRARY),
    ]
}

#[test]
#[cfg(target_os = "linux")]
fn owning_package_mutable_trait_crosses_plain_and_generic_helpers_with_exact_cleanup() {
    owning_trait_native_cleanup(
        &mutable_files(),
        &[(None, 0, b"1:1:1\n"), (Some("1"), 1, b"1:0:0\n")],
    );
}

#[cfg(target_os = "linux")]
fn owning_trait_native_cleanup(files: &[(&str, &str)], cases: &[(Option<&str>, i32, &[u8])]) {
    owning_trait_project_native_cleanup(&project(files), cases);
}

#[cfg(target_os = "linux")]
fn owning_trait_project_native_cleanup(
    sources: &zil_driver::ProjectSources,
    cases: &[(Option<&str>, i32, &[u8])],
) {
    use std::{
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
    };
    let evidence = std::env::var_os("ZIL_TEST_REPORT_DIR");
    let keep = evidence.is_some();
    let base = evidence
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!(
        "zil-owning-mutable-traits-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    if keep {
        for source in &sources.modules {
            fs::write(dir.join(&source.path), source.text.as_bytes()).unwrap();
        }
    }
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    let observer = dir.join("observer.o");
    let built=Command::new(&clang).args(["-std=c11","-Wall","-Wextra","-Werror","-c"])
 .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/language-readiness/terminal/generics-and-constants/store-elements/allocation-observer.c"))
 .arg("-o").arg(&observer).output().unwrap();
    assert!(built.status.success(), "{built:?}");
    let quote = |path: &Path| format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"));
    let wrapper = dir.join("link.sh");
    fs::write(&wrapper,format!("#!/bin/sh\nset -eu\nlink=1\nfor arg in \"$@\"; do case \"$arg\" in -c|-S|-E|--version) link=0;; esac; done\nif [ \"$link\" = 1 ]; then exec {} \"$@\" -x none {} -Wl,--wrap=realloc -Wl,--wrap=calloc -Wl,--wrap=free; fi\nexec {} \"$@\"\n",quote(&clang),quote(&observer),quote(&clang))).unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let report = compile_project(
                sources,
                &CompileOptions {
                    backend,
                    optimization,
                    clang: Some(wrapper.clone()),
                    stop_after: Stage::Build,
                    output: Some(dir.join(format!("program-{backend:?}-{optimization:?}"))),
                    ..CompileOptions::default()
                },
            );
            if keep {
                fs::write(
                    dir.join(format!("compile-{backend:?}-{optimization:?}.json")),
                    report.to_json(),
                )
                .unwrap();
            }
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            assert_eq!(report.native_codegen.as_ref().unwrap().backend, backend);
            assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
            for &(failure, exit, stdout) in cases {
                let mut command = Command::new(&report.artifacts["executable"]);
                command.env_remove("ZIL_POD_ALLOC_FAIL_AT");
                if let Some(at) = failure {
                    command.env("ZIL_POD_ALLOC_FAIL_AT", at);
                }
                let output = command.output().unwrap();
                if keep {
                    fs::write(
                        dir.join(format!(
                            "execute-{backend:?}-{optimization:?}-{}.txt",
                            failure.unwrap_or("none")
                        )),
                        format!("failure={failure:?}\n{output:?}\n"),
                    )
                    .unwrap();
                }
                assert_eq!(
                    output.status.code(),
                    Some(exit),
                    "{backend:?}/{optimization:?} fail={failure:?}: {output:?}"
                );
                assert_eq!(
                    output.stdout, stdout,
                    "{backend:?}/{optimization:?} fail={failure:?}"
                );
                assert!(output.stderr.is_empty(), "{output:?}");
                eprintln!(
                    "native {backend:?}/{optimization:?} failure={failure:?}: exit={exit}, stdout={:?}",
                    String::from_utf8_lossy(&output.stdout)
                );
            }
        }
    }
    if !keep {
        fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn owning_package_mutable_trait_keeps_shared_permissions_provider_identity_and_live_views() {
    let shared = MUTABLE_LIBRARY.replace("value:&mut model.Item", "value:&model.Item");
    let immutable = MUTABLE_MAIN.replace("var value=", "let value=");
    let view_model = format!(
        "{MUTABLE_MODEL}\npub fn view(value:&Item)->&[u8]{{return bytes_view(value.data)}}"
    );
    let view_main = MUTABLE_MAIN
        .replace(
            "var value=model.make()?",
            "var value=model.make()?;let live=model.view(&value)",
        )
        .replace("return 0", "return i32(live[0])-65");
    let other_model = MUTABLE_MODEL
        .replace("import control", "import other")
        .replace("impl control.Control", "impl other.Control");
    let bad_signature = MUTABLE_MODEL.replace("fn reset(self:&mut Self)", "fn reset(self:&Self)");
    for (name, main, model, library, other) in [
        (
            "shared-receiver",
            MUTABLE_MAIN,
            MUTABLE_MODEL,
            shared.as_str(),
            false,
        ),
        (
            "immutable-owner",
            immutable.as_str(),
            MUTABLE_MODEL,
            MUTABLE_LIBRARY,
            false,
        ),
        (
            "live-view",
            view_main.as_str(),
            view_model.as_str(),
            MUTABLE_LIBRARY,
            false,
        ),
        (
            "wrong-trait",
            MUTABLE_MAIN,
            other_model.as_str(),
            MUTABLE_LIBRARY,
            true,
        ),
        (
            "wrong-mode",
            MUTABLE_MAIN,
            bad_signature.as_str(),
            MUTABLE_LIBRARY,
            false,
        ),
    ] {
        let mut files = vec![
            ("main", main),
            ("control", MUTABLE_PROTOCOL),
            ("model", model),
            ("library", library),
        ];
        if other {
            files.push(("other", MUTABLE_PROTOCOL));
        }
        let report = compile_project(
            &project(&files),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{name}: {}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .all(|d| !matches!(d.code.as_str(), "E-MODULE-ID" | "E-GENERIC-ORIGIN")),
            "{name}: {}",
            report.to_json()
        );
    }
    // Unused provider bodies retain the declaration's actual exclusive mode.
    let root = "import model;fn unused(value:&mut model.Item){value.reset()}fn main(){}";
    let report = compile_project(
        &project(&[
            ("main", root),
            ("control", MUTABLE_PROTOCOL),
            ("model", bad_signature.as_str()),
        ]),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}

#[test]
fn owning_package_mutable_trait_default_body_preserves_actual_self_and_mode() {
    let protocol = MUTABLE_PROTOCOL.replace(
        "fn reset(self:&mut Self);",
        "fn reset(self:&mut Self){self.replace(0)}",
    );
    let model = MUTABLE_MODEL.replace(" fn reset(self:&mut Self){self.key=0}\n", "");
    let report = compile_project(
        &project(&[
            ("main", MUTABLE_MAIN),
            ("control", protocol.as_str()),
            ("model", model.as_str()),
            ("library", MUTABLE_LIBRARY),
        ]),
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
}

const MUTABLE_RAW: &str = r#"
trait Control{fn replace(self:&mut Self,value:i64)->i64;}
package Item{manages data:ByteBuffer;key:i64}
package Other{manages data:ByteBuffer;key:i64}
impl Control for Item{
 fn replace(self:&mut Self,value:i64)->i64{
  let appended=bytes_push(self.data,66u8);let old=self.key;self.key=value;return old
 }
}
impl Control for Other{
 fn replace(self:&mut Self,value:i64)->i64{let old=self.key;self.key=value;return old}
}
fn apply<K:StoreElement+Control>(value:&mut K,n:i64)->i64{return value.replace(n)}
fn main()->i32{
 var value=Item{data:bytes_new(),key:1}
 let before=bytes_view(value.data)
 let size=before.len()
 let old=apply(&mut value,7)
 return i32(old+size-1)
}
"#;

#[test]
fn owning_package_mutable_trait_raw_and_wire_rebuild_actual_receiver_identity_and_effects() {
    use zil_ir::Type;
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "mutable-owning-trait.zil",
        MUTABLE_RAW,
    );
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::default();
    let original = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&original, &limits).unwrap();
    zil_verify::verify_module(original.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&original, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    let provider = original
        .functions
        .iter()
        .find(|f| f.name.ends_with("Item.replace"))
        .unwrap()
        .id;
    let other = original
        .functions
        .iter()
        .find(|f| f.name.ends_with("Other.replace"))
        .unwrap()
        .id;
    let helper = original
        .functions
        .iter()
        .find(|f| {
            f.blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .any(|i| matches!(i.kind,InstructionKind::Call{function,..}if function==provider))
        })
        .unwrap()
        .id;
    for mutation in [
        "shared-actual-receiver",
        "same-layout-wrong-self",
        "future-view-use",
    ] {
        let mut forged = original.clone();
        match mutation {
            "shared-actual-receiver" => {
                let main = forged
                    .functions
                    .iter_mut()
                    .find(|f| f.name == "main")
                    .unwrap();
                let borrow=main.blocks.iter_mut().flat_map(|b|&mut b.instructions).find(|i|
      matches!(i.kind,InstructionKind::BorrowLocalMut{..})&&i.result.is_some_and(|v|matches!(&main.values[v.0 as usize],Type::Address{mutable:true,inner}if matches!(inner.as_ref(),Type::Package{..})))).unwrap();
                let InstructionKind::BorrowLocalMut { local } = borrow.kind else {
                    unreachable!()
                };
                borrow.kind = InstructionKind::BorrowLocal { local };
            }
            "same-layout-wrong-self" => {
                let call=forged.functions[helper.0 as usize].blocks.iter_mut().flat_map(|b|&mut b.instructions).find(|i|matches!(i.kind,InstructionKind::Call{function,..}if function==provider)).unwrap();
                let InstructionKind::Call { function, .. } = &mut call.kind else {
                    unreachable!()
                };
                *function = other;
            }
            "future-view-use" => {
                let main = forged
                    .functions
                    .iter_mut()
                    .find(|f| f.name == "main")
                    .unwrap();
                let block = main
                    .blocks
                    .iter_mut()
                    .find(|b| {
                        b.instructions
                            .iter()
                            .any(|i| matches!(i.kind, InstructionKind::SliceLength { .. }))
                    })
                    .unwrap();
                let at = block
                    .instructions
                    .iter()
                    .position(|i| matches!(i.kind, InstructionKind::SliceLength { .. }))
                    .unwrap();
                let length = block.instructions.remove(at);
                let value = length.result.unwrap();
                let stored = block
                    .instructions
                    .iter()
                    .position(|i| matches!(i.kind,InstructionKind::Store{value:v,..}if v==value))
                    .unwrap();
                let store = block.instructions.remove(stored);
                let after = block
                    .instructions
                    .iter()
                    .position(
                        |i| matches!(i.kind,InstructionKind::Call{function,..}if function==helper),
                    )
                    .unwrap()
                    + 1;
                block.instructions.splice(after..after, [length, store]);
            }
            _ => unreachable!(),
        }
        // Opcode/type and nominal-call forgeries are checked independently at
        // Verify admission. The well-typed reordered view also reaches the
        // producer's actual resource-window check.
        if mutation == "future-view-use" {
            let errors = zil_analyze::analyze(&forged, &limits).expect_err(mutation);
            assert!(
                errors
                    .iter()
                    .any(|d| d.category == ResultCategory::SafeReject
                        && d.message.contains("conflicts with a live")),
                "{mutation}: {errors:?}"
            );
        }
        let mut claim = plan.clone();
        claim.module_fingerprint = forged.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&forged, &claim, &limits).unwrap();
        for result in [
            zil_verify::verify_module(forged, claim, &target, &limits),
            zil_verify::verify_module_bytes(&wire, &target, &limits),
        ] {
            let errors = result.expect_err(mutation);
            eprintln!(
                "{mutation}: {}",
                errors
                    .iter()
                    .map(|d| d.code.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            );
            assert!(
                errors
                    .iter()
                    .all(|d| d.category == ResultCategory::SafeReject),
                "{mutation}: {errors:?}"
            );
            assert!(
                errors.iter().all(|d| !matches!(
                    d.code.as_str(),
                    "E-VERIFY-PLAN" | "E-VERIFY-PLAN-IDENTITY" | "E-VERIFY-HOLD"
                )),
                "{mutation}: {errors:?}"
            );
            let expected = match mutation {
                "shared-actual-receiver" => "E-VERIFY-TYPE",
                "same-layout-wrong-self" => "E-VERIFY-CALL",
                "future-view-use" => "E-BORROW-CONFLICT",
                _ => unreachable!(),
            };
            assert!(
                errors.iter().any(|d| d.code == expected),
                "{mutation}: {errors:?}"
            );
        }
    }
}

const OWNING_LENDING_PROTOCOL: &str = include_str!("lending/protocol.zil");
const OWNING_LENDING_MODEL: &str = include_str!("lending/model.zil");
const OWNING_LENDING_LIBRARY: &str = include_str!("lending/library.zil");
const OWNING_LENDING_MAIN: &str = include_str!("lending/main.zil");
fn owning_lending_files() -> [(&'static str, &'static str); 4] {
    [
        ("main", OWNING_LENDING_MAIN),
        ("protocol", OWNING_LENDING_PROTOCOL),
        ("model", OWNING_LENDING_MODEL),
        ("library", OWNING_LENDING_LIBRARY),
    ]
}
#[test]
fn owning_item_lending_iterator_preserves_store_element_source() {
    let report = compile_project(
        &project(&owning_lending_files()),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

#[test]
#[cfg(target_os = "linux")]
fn owning_item_lending_iterator_native_values_and_exact_cleanup() {
    owning_trait_native_cleanup(
        &owning_lending_files(),
        &[
            (None, 0, b"3:3:3\n"),
            (Some("1"), 1, b"1:0:0\n"),
            (Some("2"), 5, b"2:1:1\n"),
            (Some("3"), 6, b"3:2:2\n"),
        ],
    );
}

#[test]
fn owning_item_lending_iterator_rejects_live_next_mutation_movement_and_bad_sources() {
    let baseline = compile_project(
        &project(&owning_lending_files()),
        &CompileOptions::default(),
    );
    assert_eq!(
        baseline.status,
        ResultCategory::Success,
        "{}",
        baseline.to_json()
    );
    let hold = "let held=library.next(&mut cursor)";
    let inspect = "match held {Option.Some(value)=>{total+=value.value},Option.None()=>{return 9}}";
    let first = "match library.next(&mut cursor) {Option.Some(value)=>{total+=value.value;if byte(value)!=65u8{return 7}},Option.None()=>{return 2}}";
    assert!(OWNING_LENDING_MODEL.contains(first));
    for action in [
        "library.next(&mut cursor)",
        "store_reserve(cursor.items,3)",
        "let moved=cursor",
        "destroy(give cursor)",
    ] {
        let model = format!(
            "{}\nfn destroy(give cursor:Cursor){{}}",
            OWNING_LENDING_MODEL.replace(first, &format!("{hold};{action};{inspect}"))
        );
        let report = compile_project(
            &project(&[
                ("main", OWNING_LENDING_MAIN),
                ("protocol", OWNING_LENDING_PROTOCOL),
                ("model", &model),
                ("library", OWNING_LENDING_LIBRARY),
            ]),
            &CompileOptions::default(),
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{action}: {}",
            report.to_json()
        );
    }
    for model in [
        OWNING_LENDING_MODEL.replace("type Item=Item;", "type Item=i64;"),
        OWNING_LENDING_MODEL.replace(
            "return Option::<&'a Item>.Some(store_at(self.items,index))",
            "let local=Item{data:bytes_new(),value:8};return Option::<&'a Item>.Some(&local)",
        ),
    ] {
        let report = compile_project(
            &project(&[
                ("main", OWNING_LENDING_MAIN),
                ("protocol", OWNING_LENDING_PROTOCOL),
                ("model", &model),
                ("library", OWNING_LENDING_LIBRARY),
            ]),
            &CompileOptions::default(),
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
    }
    let model = OWNING_LENDING_MODEL
        .replace("import protocol", "import other")
        .replace(
            "impl protocol.LendingIterator",
            "impl other.LendingIterator",
        );
    let report = compile_project(
        &project(&[
            ("main", OWNING_LENDING_MAIN),
            ("protocol", OWNING_LENDING_PROTOCOL),
            ("other", OWNING_LENDING_PROTOCOL),
            ("model", &model),
            ("library", OWNING_LENDING_LIBRARY),
        ]),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}

#[path = "lending/verify.rs"]
mod owning_lending_verify;

#[test]
fn owning_item_lending_iterator_checks_unused_generic_life_and_copy_obligations() {
    let control = compile_project(
        &project(&owning_lending_files()),
        &CompileOptions::default(),
    );
    assert_eq!(
        control.status,
        ResultCategory::Success,
        "{}",
        control.to_json()
    );
    for body in [
        "pub fn wrong<life a,life b,I:protocol.LendingIterator>(first:&'a mut I,second:&'b mut I)->Option<&'a I.Item>{return second.next()}",
        "pub fn duplicate<I:protocol.LendingIterator>(value:I.Item)->(I.Item,I.Item){return (value,value)}",
        "pub fn unbound<I>(cursor:&mut I){cursor.next()}",
    ] {
        let library = format!("{OWNING_LENDING_LIBRARY}\n{body}");
        let report = compile_project(
            &project(&[
                ("main", OWNING_LENDING_MAIN),
                ("protocol", OWNING_LENDING_PROTOCOL),
                ("model", OWNING_LENDING_MODEL),
                ("library", &library),
            ]),
            &CompileOptions::default(),
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{body}: {}",
            report.to_json()
        );
    }
    let model = OWNING_LENDING_MODEL
        .replace("<life a>", "<life yielded>")
        .replace("&'a", "&'yielded");
    let report = compile_project(
        &project(&[
            ("main", OWNING_LENDING_MAIN),
            ("protocol", OWNING_LENDING_PROTOCOL),
            ("model", &model),
            ("library", OWNING_LENDING_LIBRARY),
        ]),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let model = OWNING_LENDING_MODEL.replace(
        "library.next(&mut cursor)",
        "protocol.next_copy(&mut cursor)",
    );
    let report = compile_project(
        &project(&[
            ("main", OWNING_LENDING_MAIN),
            ("protocol", include_str!("../../../lib/std/iter.zil")),
            ("model", &model),
            ("library", OWNING_LENDING_LIBRARY),
        ]),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "next_copy must retain Copy: {}",
        report.to_json()
    );
}

#[test]
fn owning_item_direct_lending_provider_preserves_store_element_source() {
    let model = OWNING_LENDING_MODEL
        .replace("import library", "")
        .replace("library.next(&mut cursor)", "cursor.next()");
    let report = compile_project(
        &project(&[
            ("main", OWNING_LENDING_MAIN),
            ("protocol", OWNING_LENDING_PROTOCOL),
            ("model", &model),
        ]),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

#[test]
fn owning_item_lending_definition_work_is_prepaid_across_many_lives_and_calls() {
    let lives = (0..12)
        .map(|n| format!("life spare{n},"))
        .collect::<String>();
    let calls = "cursor.next();".repeat(6);
    let library = format!(
        "{OWNING_LENDING_LIBRARY}\npub fn repeated<life a,{lives}I:protocol.LendingIterator>(cursor:&'a mut I)->Option<&'a I.Item>{{{calls}return cursor.next()}}"
    );
    let files = project(&[
        ("main", OWNING_LENDING_MAIN),
        ("protocol", OWNING_LENDING_PROTOCOL),
        ("model", OWNING_LENDING_MODEL),
        ("library", &library),
    ]);
    for nodes in [false, true] {
        let options = |n| {
            let mut options = CompileOptions {
                stop_after: Stage::Core,
                ..CompileOptions::default()
            };
            if nodes {
                options.limits.max_nodes = n
            } else {
                options.limits.max_analysis_steps = n
            }
            options
        };
        let mut low = 1;
        let mut high = if nodes {
            Limits::default().max_nodes
        } else {
            Limits::default().max_analysis_steps
        };
        assert_eq!(
            compile_project(&files, &options(high)).status,
            ResultCategory::Success
        );
        while low < high {
            let mid = low + (high - low) / 2;
            let report = compile_project(&files, &options(mid));
            match report.status {
                ResultCategory::Success => high = mid,
                ResultCategory::ResourceLimit => low = mid + 1,
                _ => panic!("{}", report.to_json()),
            }
        }
        let mut session = zil_driver::CompilationSession::default();
        for (n, status) in [
            (low, ResultCategory::Success),
            (low - 1, ResultCategory::ResourceLimit),
            (low, ResultCategory::Success),
        ] {
            for report in [
                session.compile_project(&files, &options(n)),
                compile_project(&files, &options(n)),
            ] {
                assert_eq!(report.status, status, "{}", report.to_json());
            }
            assert_eq!(session.active_query_count(), 0);
        }
        eprintln!(
            "owning lending definition nodes={nodes} N={low}; N-1/cold/warm/fresh agree (13 lives,7 calls)"
        );
    }
}
