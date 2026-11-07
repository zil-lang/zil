//! Ordinary nominal bounds are checked symbolically and at each instantiation.
#[path = "../../../tests/language-readiness/dynamic-dispatch/consumers.rs"]
mod shared_dynamic_dispatch_consumers;
#[path = "../../../tests/language-readiness/factory-interface-flow/consumers.rs"]
mod static_associated_method_consumers;
#[path = "../../../tests/language-readiness/borrowed-interface-flow/consumers.rs"]
mod trait_shared_lending_consumers;
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
    compile_source,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

#[test]
fn ordinary_library_bounds_call_the_consumers_exact_trait_provider() {
    let files = [
        (
            "main",
            "import maplib\nstruct Key{n:i32}\nimpl maplib.KeyOps for Key{fn hash()->i64{return i64(.n)}fn equals(other:Key)->bool{return .n==other.n}}\nfn main()->i32{if !maplib.same(Key{n:7},Key{n:7},i64(9)){return 1}if maplib.same(Key{n:7},Key{n:8},i64(9)){return 2}return 0}",
        ),
        (
            "maplib",
            "pub trait KeyOps{fn hash(self:&Self)->i64;fn equals(self:&Self,other:Self)->bool;}\npub fn same<K:Copy+StoreElement+KeyOps,V:Copy+StoreElement>(left:K,right:K,value:V)->bool{value;if left.hash()!=7{return false}return left.equals(right)}",
        ),
    ];
    let report = compile_project(&project(&files), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    native_both(&files);

    // A same-spelled provider from a different trait cannot satisfy the
    // library's bound, even though its receiver and method signatures match.
    let wrong = [
        (
            "main",
            "import maplib\ntrait KeyOps{fn hash(self:&Self)->i64;fn equals(self:&Self,other:Self)->bool;}struct Key{n:i32}impl KeyOps for Key{fn hash()->i64{return 7}fn equals(other:Key)->bool{return true}}fn main()->i32{if maplib.same(Key{n:7},Key{n:7},i64(9)){return 0}return 1}",
        ),
        files[1],
    ];
    let report = compile_project(&project(&wrong), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}

#[test]
#[cfg(target_os = "linux")]
fn mutable_holder_trait_task_and_queued_registration_share_one_native_program() {
    use std::{os::unix::fs::PermissionsExt, path::Path, process::Command};
    use zil_driver::NativeBackend;

    let files = [
        (
            "main",
            include_str!("../../../tests/language-readiness/native-composition/main.zil"),
        ),
        (
            "rules",
            include_str!("../../../tests/language-readiness/native-composition/rules.zil"),
        ),
        (
            "model",
            include_str!("../../../tests/language-readiness/native-composition/model.zil"),
        ),
    ];
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    let quote = |path: &Path| format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"));
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-native-composition-{}-{}-{backend:?}-{optimization:?}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed),
            ));
            fs::create_dir_all(&dir).unwrap();
            let host = dir.join("host.o");
            let compiled = Command::new(&clang)
                .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
                .arg(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../tests/language-readiness/async-registration/queued-host.c"),
                )
                .arg("-o")
                .arg(&host)
                .output()
                .unwrap();
            assert!(compiled.status.success(), "{compiled:?}");
            let wrapper = dir.join("link.sh");
            fs::write(&wrapper, format!("#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in \"$@\"; do\ncase \"$arg\" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ \"$link\" = 1 ] && [ \"$input\" = 1 ]; then\nexec {} \"$@\" -x none {} -Wl,--wrap=calloc -Wl,--wrap=free\nfi\nexec {} \"$@\"\n", quote(&clang), quote(&host), quote(&clang))).unwrap();
            fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
            let report = compile_project(
                &project(&files),
                &CompileOptions {
                    backend,
                    optimization,
                    clang: Some(wrapper),
                    stop_after: Stage::Build,
                    output: Some(dir.join("program")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1, "{}", report.to_json());
            let codegen = report.native_codegen.as_ref().unwrap();
            assert_eq!(codegen.backend, backend);
            assert!(codegen.fallback.is_none(), "{codegen:?}");
            let actual = Command::new("timeout")
                .arg("10s")
                .arg(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert_eq!(actual.status.code(), Some(42), "{actual:?}");
            assert_eq!(
                actual.stdout,
                b"pending retried; ready drained; registration dropped once\n"
            );
            fs::remove_dir_all(dir).unwrap();
        }
    }
}

#[test]
fn cross_module_retained_task_annotation_and_holder_identity_reach_verify() {
    let main = r#"import api
fn main()->TaskPollState!i32 {
    let value=21;
    match api.start(&value) {
        .ok(created)=>{
            var task=give created;
            match task_poll(&mut task)? {
                Poll.Pending()=>{},
                Poll.Ready(_)=>{return 91;}
            }
            match task_poll(&mut task)? {
                Poll.Pending()=>{return 92;},
                Poll.Ready(result)=>{if result!=42{return 93;}}
            }
            match task_poll_close(&mut task) {
                Poll.Pending()=>{return 94;},
                Poll.Ready(_)=>{}
            }
            task_finish_destroy(give task);
        },
        .err(_)=>{return 95;}
    }
    return 0;
}"#;
    let api = r#"fn worker(input:&i32)->i32 {
    let first=*input;
    task_yield();
    return first+*input;
}
pub fn start<life a>(input:&'a i32)->TaskCreateError!Task<i32> contract{result(create)} {
    return task_create(worker,input);
}"#;
    let options = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let positive = compile_project(&project(&[("main", main), ("api", api)]), &options);
    assert_eq!(
        positive.status,
        ResultCategory::Success,
        "{}",
        positive.to_json()
    );
    assert_eq!(positive.verification_runs, 1, "{}", positive.to_json());

    let wrong_result = api.replace("Task<i32>", "Task<bool>");
    let mismatch = compile_project(
        &project(&[("main", main), ("api", &wrong_result)]),
        &options,
    );
    assert_eq!(
        mismatch.status,
        ResultCategory::SafeReject,
        "{}",
        mismatch.to_json()
    );
    assert_eq!(mismatch.phase, Phase::Types, "{}", mismatch.to_json());
    assert!(
        mismatch
            .diagnostics
            .iter()
            .any(|d| d.code == "E-TASK-RETURN-CONTEXT"),
        "{}",
        mismatch.to_json()
    );
    assert_eq!(mismatch.verification_runs, 0);

    let fake = compile_project(
        &project(&[
            (
                "main",
                r#"import model
struct MutableHolder<life a>{pointer:&'a mut i32}
fn fake<life a>(p:&'a mut i32)->MutableHolder<'a>{return MutableHolder::<'a>{pointer:p}}
fn main()->i32 {
    var value=41;
    if true {
        var holder=fake(&mut value);
        model.increment(&mut holder);
    }
    return 0;
}"#,
            ),
            (
                "model",
                r#"pub struct MutableHolder<life a>{pointer:&'a mut i32}
pub fn increment<life a>(holder:&mut MutableHolder<'a>){
    let pointer=holder.pointer;
    *pointer=*pointer+1;
}"#,
            ),
        ]),
        &options,
    );
    assert_eq!(
        fake.status,
        ResultCategory::SafeReject,
        "{}",
        fake.to_json()
    );
    assert_eq!(fake.phase, Phase::Expand, "{}", fake.to_json());
    assert!(
        fake.diagnostics
            .iter()
            .any(|d| d.code == "E-GENERIC-ARGUMENT"),
        "{}",
        fake.to_json()
    );
    assert_eq!(fake.verification_runs, 0);
}

#[test]
fn self_associated_constant_self_associated_constants_effective_and_default_checks() {
    for text in [
        "trait C{const A:i32;const B:i32=Self.A+1;}struct P{}impl C for P{const A:i32=41;}fn main()->i32{return P.B-42}",
        "trait C{const B:i32=Self.A+1;const A:i32;}struct P{}impl C for P{const A:i32=41;}fn main()->i32{return P.B-42}",
        "trait C{const A:i32;const B:i32=Self.A+1;}struct P{}struct Q{}impl C for Q{const A:i32=99;}impl C for P{const A:i32=41;}fn main()->i32{return P.B+Q.B-142}",
        "trait C{const A:i32=Self.B+1;const B:i32=Self.A+1;}struct P{}impl C for P{const A:i32=41;}fn main()->i32{return P.A+P.B-83}",
        "trait C{const A:i32;const B:i32;}struct P{}impl C for P{const A:i32=41;const B:i32=Self.A+1;}fn main()->i32{return P.B-42}",
        "fn add(x:i32)->i32{return x+1}trait C{const A:i32;const B:i32=add(Self.A);}struct P{}impl C for P{const A:i32=41;}fn main()->i32{return P.B-42}",
        "trait C{const A:i32;const B:i32=Self.A+1;}fn main(){}",
        "trait C{const A:i32=Self.B;const B:i32=Self.A;}fn main(){}",
        "trait C{const A:bool;const B:bool=!Self.A;}struct P{}impl C for P{const A:bool=false;}fn main()->i32{if P.B{return 0}return 1}",
        "trait C{const A:i64;const B:i64=Self.A+3;}struct P{}impl C for P{const A:i64=-2;}fn main()->i32{return i32(P.B)-1}",
        "trait C{const A:f64;const B:f64=Self.A+1.0f64;}struct P{}impl C for P{const A:f64=41.0f64;}fn main()->i32{return i32(P.B)-42}",
    ] {
        let report = check(text);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{text}: {}",
            report.to_json()
        );
    }
    for (text, code) in [
        (
            "trait C{const A:i32=Self.B;const B:i32=Self.A;}struct P{}impl C for P{}fn main(){}",
            "E-CT-ASSOCIATED-CYCLE",
        ),
        (
            "trait C{const A:i32;const B:i32=Self.A+1;}struct P{}impl C for P{const A:i32=2147483647;}fn main(){}",
            "E-CT-ARITHMETIC",
        ),
        (
            "trait C{const A:i32;const B:i32=Self.A+1;}struct P{}impl C for P{const A:i32=2147483647;const B:i32=0;}fn main(){}",
            "E-CT-ARITHMETIC",
        ),
        (
            "trait C{const A:i32;const B:i32=Self.Missing+1;}fn main(){}",
            "",
        ),
        ("trait C{const A:bool;const B:i32=Self.A+1;}fn main(){}", ""),
        (
            "extern \"C\" fn forbidden()->i32;fn impure()->i32{unsafe{return forbidden()}}trait C{const A:i32;const B:i32=Self.A+impure();}fn main(){}",
            "",
        ),
    ] {
        let report = check(text);
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{text}: {}",
            report.to_json()
        );
        assert_ne!(
            report.status,
            ResultCategory::InfraError,
            "{text}: {}",
            report.to_json()
        );
        if !code.is_empty() {
            assert!(
                report.diagnostics.iter().any(|d| d.code == code),
                "{text}: {}",
                report.to_json()
            );
        }
    }
    // Standalone Expand is a definition/consumption stage, not a CT executor.
    let pending = "trait C{const A:i32;const B:i32=Self.A+1;}struct P{}impl C for P{const A:i32=41;}fn main(){}";
    let unit = source(pending);
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let errors = zil_expand::expand(&unit, &ast).unwrap_err();
    assert_eq!(
        errors[0].category,
        ResultCategory::Unsupported,
        "{errors:?}"
    );
    assert!(
        errors[0]
            .message
            .contains("Source Driver verified CT materialization"),
        "{errors:?}"
    );
    // Structural Drop and ordinary nominal associated constants coexist; the
    // old nominal-bound pass incorrectly treated Drop as a missing trait.
    for value in ["42", "Self.A+1"] {
        let text = format!(
            "resource R{{n:i32}}impl Drop for R{{fn drop(self:Self){{}}}}trait C{{const A:i32;const B:i32={value};}}struct P{{}}impl C for P{{const A:i32=41;}}fn main()->i32{{let r=R{{n:1}};return P.B-42}}"
        );
        let report = check(&text);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Verify, "{}", report.to_json());
        assert_eq!(report.verification_runs, 1, "{}", report.to_json());
    }
}

#[test]
fn self_associated_constant_generated_self_default_impls_require_completed_ct_jobs() {
    for text in [
        "trait C{const A:i32;const B:i32=Self.A+1;}struct P{}#emit quote<ItemList>{impl C for P{const A:i32=2147483647;const B:i32=0;}};fn main(){}",
        "trait C{const A:i32;const B:i32=Self.A+1;}struct P{}#for(i in 0..1){impl C for P{const A:i32=2147483647;const B:i32=0;}}fn main(){}",
    ] {
        let report = check(text);
        eprintln!(
            "Self associated constant generation nearest status={:?} phase={:?} diagnostics={:?}",
            report.status, report.phase, report.diagnostics
        );
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_ne!(
            report.status,
            ResultCategory::InfraError,
            "{}",
            report.to_json()
        );
    }
    let closed = check(
        "trait C{const A:i32;const B:i32=42;}struct P{}#emit quote<ItemList>{impl C for P{const A:i32=41;const B:i32=0;}};fn main(){}",
    );
    assert_eq!(
        closed.status,
        ResultCategory::Success,
        "{}",
        closed.to_json()
    );
}

#[test]
fn self_associated_constant_self_associated_identity_budgets_and_machine_values() {
    use zil_driver::CompilationSession;
    let original = include_str!(
        "../../../tests/language-readiness/terminal/generics-and-constants/associated-constant-self.zil"
    );
    let unit = source(original);
    for ledger in ["work", "nodes", "steps", "bytes"] {
        let options = |n: usize| {
            let mut options = CompileOptions::default();
            match ledger {
                "work" => options.limits.max_analysis_steps = n,
                "nodes" => options.limits.max_nodes = n,
                "steps" => options.limits.max_ct_steps = n as u64,
                "bytes" => options.limits.max_source_bytes = n,
                _ => unreachable!(),
            }
            options
        };
        let limits = Limits::default();
        let (mut low, mut high) = (
            1,
            match ledger {
                "work" => limits.max_analysis_steps,
                "nodes" => limits.max_nodes,
                "steps" => limits.max_ct_steps as usize,
                "bytes" => limits.max_source_bytes,
                _ => unreachable!(),
            },
        );
        while low < high {
            let n = low + (high - low) / 2;
            let report = compile_source(&unit, &options(n));
            match report.status {
                ResultCategory::Success => high = n,
                ResultCategory::ResourceLimit => low = n + 1,
                _ => panic!("{ledger} n={n}: {}", report.to_json()),
            }
        }
        let mut session = CompilationSession::default();
        for (n, expected) in [
            (low, ResultCategory::Success),
            (low - 1, ResultCategory::ResourceLimit),
            (low, ResultCategory::Success),
        ] {
            for report in [
                session.compile_source(&unit, &options(n)),
                CompilationSession::default().compile_source(&unit, &options(n)),
            ] {
                assert_eq!(
                    report.status,
                    expected,
                    "{ledger} N={low}: {}",
                    report.to_json()
                );
            }
            assert_eq!(session.active_query_count(), 0);
        }
        eprintln!("Self associated constant {ledger} N={low}; N-1/cold/warm/fresh agree");
    }
    native_both(&[("main", original)]);
    let mut session = CompilationSession::default();
    for text in [
        original.to_owned(),
        original
            .replace("const B:i32=70", "const B:i32=80")
            .replace("first::<Q>()!=70", "first::<Q>()!=80"),
        original
            .replace("const A:i32=40", "const A:i32=41")
            .replace("first::<P>()!=41", "first::<P>()!=42"),
        original.to_owned(),
    ] {
        let dir = std::env::temp_dir().join(format!(
            "self_associated_constant-warm-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let executable = dir.join("program");
        let report = session.compile_source(
            &source(&text),
            &CompileOptions {
                stop_after: Stage::Build,
                backend: zil_driver::NativeBackend::Clang,
                optimization: Optimization::O0,
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
        assert!(report.verification_runs >= 1, "{}", report.to_json());
        let output = Command::new(&executable).output().unwrap();
        assert_eq!(output.status.code(), Some(0), "{text}: {output:?}");
        fs::remove_dir_all(dir).unwrap();
    }
    for width in [32, 64] {
        let report = compile_source(
            &source(
                "trait C{const A:usize;const B:usize=Self.A+1usize;}struct P{}impl C for P{const A:usize=4294967295usize;}fn main(){}",
            ),
            &CompileOptions {
                target: zil_base::TargetProfile {
                    pointer_width: width,
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        assert_eq!(
            report.status,
            if width == 32 {
                ResultCategory::SafeReject
            } else {
                ResultCategory::Success
            },
            "{}",
            report.to_json()
        );
    }
    let owners = [
        (
            "main",
            "import a;import b;fn main()->i32{if a.read()!=41{return 1}if b.read()!=92{return 2}return 0}",
        ),
        (
            "a",
            "fn inc(x:i32)->i32{return x+1}trait C{const A:i32;const B:i32=inc(Self.A);}trait Flag{const A:bool;const B:bool=!Self.A;}struct P{}impl C for P{const A:i32=40;}impl Flag for P{const A:bool=false;}fn flag<T:Flag>()->bool{return T.B}fn number<T:C>()->i32{return T.B}pub fn read()->i32{if flag::<P>(){return number::<P>()}return 0}",
        ),
        (
            "b",
            "fn inc(x:i32)->i32{return x+2}trait C{const A:i32;const B:i32=inc(Self.A);}trait Float{const X:f64;const Y:f64=Self.X+1.0f64;}struct P{}impl C for P{const A:i32=90;}impl Float for P{const X:f64=41.0f64;}pub fn read()->i32{if P.Y!=42.0f64{return 0}return P.B}",
        ),
    ];
    let owned = compile_project(&project(&owners), &CompileOptions::default());
    assert_eq!(owned.status, ResultCategory::Success, "{}", owned.to_json());
    native_both(&owners);
    let outside = compile_project(
        &project(&[
            (
                "main",
                "import api;struct P{}impl api.C for P{const A:i32=41;}fn main(){}",
            ),
            ("api", "pub trait C{const A:i32;const B:i32=Self.A+1;}"),
        ]),
        &CompileOptions::default(),
    );
    assert_eq!(
        outside.status,
        ResultCategory::Unsupported,
        "{}",
        outside.to_json()
    );
    assert!(
        outside
            .diagnostics
            .iter()
            .any(|d| d.message.contains("cross-module constant witness")),
        "{}",
        outside.to_json()
    );
    let closed = compile_project(
        &project(&[
            (
                "main",
                "import api;struct P{}impl api.C for P{const A:i32=41;}fn main()->i32{return P.B-42}",
            ),
            ("api", "pub trait C{const A:i32;const B:i32=42;}"),
        ]),
        &CompileOptions::default(),
    );
    assert_eq!(
        closed.status,
        ResultCategory::Success,
        "{}",
        closed.to_json()
    );
}

#[test]
fn owned_replacement_dyn_source_metadata_keeps_same_named_project_traits_and_runs_both_providers() {
    let files = [
        (
            "main",
            include_str!(
                "../../../tests/language-readiness/terminal/generics-and-constants/dyn-source/project/main.zil"
            ),
        ),
        (
            "a",
            include_str!(
                "../../../tests/language-readiness/terminal/generics-and-constants/dyn-source/project/a.zil"
            ),
        ),
        (
            "b",
            include_str!(
                "../../../tests/language-readiness/terminal/generics-and-constants/dyn-source/project/b.zil"
            ),
        ),
    ];
    let directory = std::env::temp_dir().join(format!(
        "zil-dyn-source341-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let report = compile_project(
        &project(&files),
        &CompileOptions {
            stop_after: Stage::Core,
            emit_dir: Some(directory.join("core")),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let dump = fs::read_to_string(&report.artifacts["core"]).unwrap();
    assert_eq!(dump.matches("DynInterface {").count(), 2, "{dump}");
    assert_eq!(dump.matches("CoreDynVtable {").count(), 2, "{dump}");
    assert_eq!(dump.matches("requires: Some(").count(), 2, "{dump}");
    assert!(dump.contains("\"a\"") && dump.contains("\"b\""), "{dump}");
    native_both(&files);

    // Same spelling and layout do not make b.Meter a provider for a.Read.
    let wrong_main = files[0]
        .1
        .replace("left(&left_value)", "left(&right_value)");
    assert_ne!(wrong_main, files[0].1);
    let wrong_files = [("main", wrong_main.as_str()), files[1], files[2]];
    let output = directory.join("wrong-interface-must-not-build");
    let report = compile_project(
        &project(&wrong_files),
        &CompileOptions {
            stop_after: Stage::Build,
            output: Some(output.clone()),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(report.commands.is_empty());
    assert!(!output.exists());
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn owned_replacement_method_receiver_origins_survive_type_and_const_substitution() {
    let original = include_str!(
        "../../../tests/language-readiness/terminal/generics-and-constants/generic-method-substitution-growth.zil"
    );
    for count in [0, 1, 2, 4, 7] {
        let (ty, value) = if count == 0 {
            ("bool".to_string(), "true".to_string())
        } else if count == 1 {
            ("[i32;1]".to_string(), "[1]".to_string())
        } else {
            (
                format!("({})", vec!["[i32;1]"; count].join(",")),
                format!("({})", vec!["[1]"; count].join(",")),
            )
        };
        for reverse in [false, true] {
            for between in [false, true] {
                let body = if between {
                    "let first=p.a(true);let same=identity::<T>(value);return first+p.b(true)"
                } else {
                    "let same=identity::<T>(value);return p.a(true)+p.b(true)"
                };
                let body = if reverse {
                    body.replace("p.a(true)", "p.TEMP(true)")
                        .replace("p.b(true)", "p.a(true)")
                        .replace("p.TEMP(true)", "p.b(true)")
                } else {
                    body.into()
                };
                let text = original
                    .replace("([i32;1],[i32;1],[i32;1],[i32;1])", &ty)
                    .replace("([1],[2],[3],[4])", &value)
                    .replace(
                        "let same=identity::<T>(value);return p.a(true)+p.b(true)",
                        &body,
                    );
                native_both(&[("main", &text)]);
            }
        }
    }
    for text in [
        include_str!(
            "../../../tests/language-readiness/terminal/generics-and-constants/generic-method-impl-growth.zil"
        ),
        include_str!(
            "../../../tests/language-readiness/terminal/generics-and-constants/generic-method-default-growth.zil"
        ),
        include_str!(
            "../../../tests/language-readiness/terminal/generics-and-constants/generic-method-const-growth.zil"
        ),
    ] {
        native_both(&[("main", text)]);
    }
}

#[test]
fn owned_replacement_generic_methods_keep_provider_modules_and_exact_trait_instances() {
    native_both(&[(
        "main",
        include_str!(
            "../../../tests/language-readiness/terminal/generics-and-constants/generic-method-no-outer.zil"
        ),
    )]);

    native_both(&[(
        "main",
        "trait Run{fn pick<T:Copy>(self:&Self,x:T)->i32;fn run(self:&Self)->i32{return self.pick(true)}}struct P{}impl Run for P{fn pick<T:Copy>(x:T)->i32{return 7}}fn main()->i32{let p=P{};return p.run()-7}",
    )]);
    native_both(&[(
        "main",
        "trait Run{fn pick<T:Copy>(self:&Self,x:T)->i32;fn run(self:&Self)->i32;}struct Box<T:Copy>{value:T}impl<T:Copy> Run for Box<T>{fn pick<U:Copy>(x:U)->i32{return 7}fn run()->i32{return self.pick(true)}}fn main()->i32{let p=Box::<bool>{value:true};return p.run()-7}",
    )]);

    native_both(&[(
        "main",
        include_str!(
            "../../../tests/language-readiness/terminal/generics-and-constants/generic-methods.zil"
        ),
    )]);
    native_both(&[(
        "main",
        "struct P{n:i32}trait Change{fn set<T:Copy>(self:&mut Self,x:T)->i32;}impl Change for P{fn set<T:Copy>(x:T)->i32{.n=.n+1;return .n}}fn update<V:Change>(p:&mut V)->i32{return p.set(true)}fn main()->i32{var p=P{n:6};return update(&mut p)-7}",
    )]);
    native_both(&[
        (
            "main",
            "import api;import data;fn main()->i32{let p=data.Point{};return api.apply(&p)-7}",
        ),
        (
            "api",
            "fn helper()->i32{return 7}trait Hidden{fn mark(self:&Self)->i32;}pub trait Read<Tag:Copy>{fn read<T:Copy>(self:&Self,x:T)->i32{return helper()}}pub fn apply<V:Read<bool>>(p:&V)->i32{return p.read(true)}",
        ),
        (
            "data",
            "import api;pub struct Point{}fn helper()->i32{return 99}impl api.Read<bool> for Point{}",
        ),
    ]);
    native_both(&[
        (
            "main",
            "import api;import data;fn main()->i32{let p=data.Point{};return p.read(api.Token{})-7}",
        ),
        (
            "api",
            "fn helper()->i32{return 7}trait Hidden{}pub struct Token{}impl Hidden for Token{}pub trait Read{fn read<T:Hidden>(self:&Self,x:T)->i32{return helper()}}",
        ),
        (
            "data",
            "import api;trait Hidden{fn hidden<T:Copy>(self:&Self,x:T)->i32;}pub struct Point{}impl Hidden for Point{fn hidden<T:Copy>(x:T)->i32{return 99}}impl api.Read for Point{}",
        ),
    ]);
    native_both(&[
        (
            "main",
            "import a;import b;import data;fn main()->i32{let p=data.Point{};return a.apply(&p)+b.apply(&p)-18}",
        ),
        (
            "a",
            "pub trait Read<Tag>{fn read<T:Copy>(self:&Self,x:T)->i32;}pub fn apply<V:Read<bool>>(p:&V)->i32{return p.read(true)}",
        ),
        (
            "b",
            "pub trait Read<Tag>{fn read<T:Copy>(self:&Self,x:T)->i32;}pub fn apply<V:Read<bool>>(p:&V)->i32{return p.read(true)}",
        ),
        (
            "data",
            "import a;import b;pub struct Point{}impl a.Read<bool> for Point{fn read<T:Copy>(x:T)->i32{return 7}}impl b.Read<bool> for Point{fn read<T:Copy>(x:T)->i32{return 11}}",
        ),
    ]);
}

#[test]
fn owned_replacement_associated_constant_member_ordinals_survive_shared_spans() {
    use zil_syntax::ast::ItemKind;
    let unit = source(include_str!(
        "../../../tests/language-readiness/terminal/generics-and-constants/associated-constants.zil"
    ));
    let mut ast = zil_syntax::parse(&unit).into_result().unwrap();
    let shared = ast.items[0].span;
    for item in &mut ast.items {
        let members = match &mut item.kind {
            ItemKind::Trait(declaration) => &mut declaration.members,
            ItemKind::Impl(implementation) => &mut implementation.members,
            _ => continue,
        };
        item.span = shared;
        for member in members {
            member.span = shared;
            match &mut member.kind {
                ItemKind::Const(constant) => {
                    if let Some(value) = &mut constant.value {
                        value.span = shared;
                    }
                    if let Some(ty) = &mut constant.ty {
                        ty.span = shared;
                    }
                }
                ItemKind::Function(function) => function.span = shared,
                _ => {}
            }
        }
    }
    let core = zil_expand::expand(&unit, &ast).unwrap();
    let typed = zil_types::check(
        &core,
        &zil_base::TargetProfile::default(),
        &Limits::default(),
    )
    .unwrap();
    let plan = zil_analyze::analyze(&typed, &Limits::default()).unwrap();
    let verified = zil_verify::verify_module(
        typed.clone(),
        plan.clone(),
        &typed.target,
        &Limits::default(),
    )
    .unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &typed.target, &Limits::default()).unwrap();
    native_verified(&verified);
}

#[test]
fn owned_replacement_associated_constants_keep_declared_types_and_committed_values() {
    for width in [32, 64] {
        let target = zil_base::TargetProfile {
            pointer_width: width,
            ..Default::default()
        };
        for text in [
            "trait C{const N:usize=4294967296usize;}fn main(){}",
            "trait C{const N:(((usize)))=(((4294967295usize+1usize)));}fn main(){}",
        ] {
            let report = compile_source(
                &source(text),
                &CompileOptions {
                    target: target.clone(),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                if width == 32 {
                    ResultCategory::SafeReject
                } else {
                    ResultCategory::Success
                },
                "{text}: {}",
                report.to_json()
            );
        }
    }
    for text in [
        include_str!(
            "../../../tests/language-readiness/terminal/generics-and-constants/associated-constants.zil"
        ),
        include_str!(
            "../../../tests/language-readiness/terminal/generics-and-constants/associated-constant-computed.zil"
        ),
        "fn calculate(x:i32)->i32{return x*7}trait C{const N:i32=calculate(1);}struct P{}impl C for P{}fn get<T:C>()->i32{return T.N}fn main()->i32{return get::<P>()-7}",
        "trait C{const N:i64;}struct P{}impl C for P{const N:i64=3000000000+7;}fn get<T:C>()->i64{return T.N}fn main()->i32{return i32(get::<P>()-3000000007)}",
        "trait C{const N:u64=18446744073709551615u64-1u64;}struct P{}impl C for P{}fn main()->i32{if P.N==18446744073709551614u64{return 0}return 1}",
        "trait C{const N:i32=7;}struct P{}impl C for P{}fn get<T:C>()->i32{return T.N}fn main()->i32{return get::<P>()-7}",
        "trait C{const N:i32;}fn unused<T:C>()->i32{return T.N+1}fn main(){}",
        "trait C{const N:i32;}struct P{N:i32}impl C for P{const N:i32=1;}fn get<T:C>()->i32{let T=P{N:7};return T.N}fn main()->i32{return get::<P>()-7}",
        "trait C{const N:i32;}struct T{}struct P{}impl C for T{const N:i32=1;}impl C for P{const N:i32=7;}fn get<T:C>()->i32{return T.N}fn main()->i32{return get::<P>()-7}",
        "trait Base{const N:i32;}trait Child:Base{}struct P{}impl Base for P{const N:i32=7;}impl Child for P{}fn get<T:Child>()->i32{return T.N}fn main()->i32{return get::<P>()-7}",
        "trait C{const N:i32;}struct P{}impl C for P{const N:i32=7;}fn main()->i32{return P.N-7}",
        "trait C{const N:i32;}struct Box<T:Copy>{value:T}impl<T:Copy> C for Box<T>{const N:i32=7;}fn get<T:Copy>(x:T)->i32{return Box::<T>.N}fn main()->i32{return get(true)-7}",
        "trait C{type Item:Copy;const N:Self.Item;}struct P{}impl C for P{type Item=i32;const N:i32=7;}fn get<T:C>()->T.Item{return T.N}fn main()->i32{return get::<P>()-7}",
        "trait C{const N:f64=7.0f64;}struct P{}impl C for P{}fn get<T:C>()->f64{return T.N}fn main()->i32{return i32(get::<P>())-7}",
        "trait C{const N:i32;}struct P{N:i32}impl C for P{const N:i32=1;}fn get<T:C>(T:P)->i32{return T.N}fn main()->i32{return get::<P>(P{N:7})-7}",
    ] {
        let report = check(text);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{text}: {}",
            report.to_json()
        );
    }
    for text in [
        "trait C{const N:i32;}struct P{}impl C for P{}fn main(){}",
        "trait C{const N:i32=1/0;}struct P{}impl C for P{const N:i32=7;}fn main(){}",
        "trait C{const N:i32;}struct P{}impl C for P{const N:i32=2147483647+1;}fn main(){}",
        "extern \"C\" fn forbidden()->i32;trait C{const N:i32=forbidden();}fn main(){}",
        "trait C{const N:i32;}struct P{}impl C for P{const N:bool=true;}fn main(){}",
        "trait C{const N:i32=true;}struct P{}impl C for P{const N:i32=7;}fn main(){}",
        "trait C{const N:i32;}struct P{}impl C for P{const N:i32=true;}fn main(){}",
        "trait C{const N:i32;const N:i32;}fn main(){}",
        "trait C{const N:i32;}struct P{}impl C for P{const N:i32=7;const N:i32=8;}fn main(){}",
        "trait C{const N:i32;}struct P{}impl C for P{const M:i32=7;}fn main(){}",
        "trait C{const N:Missing;}fn main(){}",
        "struct Box<T:Copy>{value:T}resource R{n:i32}trait C{const N:Box<R>;}fn main(){}",
        "trait C{const N:i32;}struct P{}impl C for P{const N:i32=7;}fn get<T:C>()->i32{let T=true;return T.N}fn main(){}",
        "trait C{const N:bool;}fn bad<T:C>()->i32{return T.N+1}fn main(){}",
        "trait C{const N:i32;}fn bad<T>()->i32{return T.N}fn main(){}",
        "trait A{const N:i32;}trait B{const N:i32;}fn bad<T:A+B>()->i32{return T.N}fn main(){}",
    ] {
        let report = check(text);
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{text}: {}",
            report.to_json()
        );
    }
    let directory = std::env::temp_dir().join(format!(
        "341-constant-reject-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&directory).unwrap();
    let executable = directory.join("must-not-exist");
    let rejected = compile_source(
        &source("trait C{const N:i32=1/0;}fn main(){}"),
        &CompileOptions {
            stop_after: Stage::Build,
            output: Some(executable.clone()),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        rejected.status,
        ResultCategory::SafeReject,
        "{}",
        rejected.to_json()
    );
    assert_eq!(rejected.phase, Phase::Ct, "{}", rejected.to_json());
    assert!(rejected.commands.is_empty());
    assert!(rejected.native_codegen.is_none());
    assert!(!executable.exists());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn owned_replacement_associated_types_remain_bound_supported_in_unused_definitions() {
    for text in [
        include_str!(
            "../../../tests/language-readiness/terminal/generics-and-constants/associated-type-instances.zil"
        ),
        "trait Read<T>{type Item:Copy;fn read(self:&Self)->Self.Item;}struct Box<T:Copy>{value:T}impl<T:Copy> Read<bool> for Box<T>{type Item=T;fn read()->Self.Item{return .value}}fn get<V:Read<bool>>(p:&V)->V.Item{return p.read()}fn main()->i32{let p=Box::<i32>{value:7};return get::<Box<i32>>(&p)-7}",
        "trait Read<T:Copy>{type Item:Copy=T;}fn main(){}",
        "trait Read{type Item;}fn once<T:Read>(x:T.Item)->T.Item{return x}fn main(){}",
        "trait Read{type Item:Copy;}trait Outer{type Item:Copy;}struct P{}impl Read for P{type Item=i32;}struct Box<T:Copy>{value:T}impl<T:Copy+Read> Outer for Box<T>{type Item=T.Item;}fn identity<V:Outer>(x:V.Item)->V.Item{return x}fn main()->i32{return identity::<Box<P>>(7)-7}",
        "trait Read{type Item:Copy;fn read(self:&Self)->Self.Item;}struct P{n:i32}impl Read for P{type Item=i32;fn read()->i32{return .n}}fn get<T:Read>(p:&T)->T.Item{return p.read()}fn relay<U:Read>(p:&U)->U.Item{return get(p)}fn main()->i32{let p=P{n:7};return relay(&p)-7}",
        "trait Read{type Item:Copy;fn identity(self:&Self,x:Self.Item)->Self.Item{return x}}struct P{}impl Read for P{type Item=i32;}fn main()->i32{let p=P{};return p.identity(7)-7}",
        "trait Read:Copy{type Item:Copy=Self;}fn main(){}",
        "trait Read{type A:Copy;type B:Copy;fn read(self:&Self)->Self.B;}struct Box<T:Copy>{value:T}impl<T:Copy> Read for Box<T>{type A=T;type B=Self.A;fn read()->Self.B{return .value}}fn get<V:Read>(p:&V)->V.B{return p.read()}fn main()->i32{let p=Box::<i32>{value:7};return get::<Box<i32>>(&p)-7}",
        "trait Read{type Item:Copy=i32;}struct P{}impl Read for P{}fn main(){}",
        "trait Read{type Item:Copy;}struct T{}fn unused<T:Read>(x:T.Item)->T.Item{return x}fn main(){}",
        "type Scalar=i32;trait Read{type Item:Copy;fn read(self:&Self)->Self.Item;}struct P{}impl Read for P{type Item=Scalar;fn read()->Scalar{return 7}}fn get<T:Read>(p:&T)->T.Item{return p.read()}fn main()->i32{let p=P{};return get::<P>(&p)-7}",
        "trait A{type Item:Copy;}trait B{type Item:Copy;}fn unused<T:A+B>(x:&T){}fn main(){}",
        "trait Read{type Item:Copy;fn read(self:&Self)->Self.Item;}fn unused<T:Read>(p:&T)->T.Item{return p.read()}fn main(){}",
        "trait Read{type Item:Copy;fn read(self:&Self)->Self.Item;}struct P{n:i32}impl Read for P{type Item=i32;fn read()->i32{return .n}}fn get<T:Read>(p:&T)->T.Item{return p.read()}fn main()->i32{let p=P{n:7};return get::<P>(&p)-7}",
        "trait Read{type Item:Copy;fn read(self:&Self)->Self.Item;}struct Box<T:Copy>{value:T}impl<T:Copy> Read for Box<T>{type Item=T;fn read()->T{return .value}}fn get<T:Read>(p:&T)->T.Item{return p.read()}fn main()->i32{let p=Box::<i32>{value:7};return get::<Box<i32>>(&p)-7}",
    ] {
        let report = check(text);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{text}: {}",
            report.to_json()
        );
    }
    for text in [
        include_str!(
            "../../../tests/language-readiness/terminal/generics-and-constants/associated-type-unused-invalid.zil"
        ),
        include_str!(
            "../../../tests/language-readiness/terminal/generics-and-constants/associated-type-unbounded-equation.zil"
        ),
        include_str!(
            "../../../tests/language-readiness/terminal/generics-and-constants/associated-type-cycle.zil"
        ),
        "trait Tag<T>{}trait Read{type Item:Tag<Missing>;}fn main(){}",
        "trait Tag<T:Copy>{}trait Read<T>{type Item:Tag<T>;}fn main(){}",
        "trait Read<T>{type Item:Copy=T;}fn main(){}",
        "trait Read{type Item:Copy;fn bad(self:&Self,x:Self.Item)->Self.Item{return x+1}}struct P{}impl Read for P{type Item=i32;fn bad(x:i32)->i32{return x}}fn main(){}",
        "trait Read{type Item:Copy=Self.Item;}fn main(){}",
        "trait Read{type Item:Copy;type Item:Copy;}fn main(){}",
        "trait Marker{}trait Read{type Item:Copy;}struct P{}impl Read for P where Self.Item:Marker{type Item=P;}fn get<T:Read>(p:&T){}fn main(){let p=P{};get(&p)}",
        "trait Read{type Item:Copy;}struct Box<T>{}impl<T> Read for Box<T>{type Item=T;}fn main(){}",
        "resource R{n:i32}trait Read{type Item:Copy;}struct P{}impl Read for P{type Item=R;}fn main(){}",
        "trait Read{type A:Copy;type B:Copy;}struct P{}impl Read for P{type A=Self.B;type B=Self.A;}fn main(){}",
        "trait A{type Item:Copy;}trait B{type Item:Copy;}fn bad<T:A+B>(x:T.Item){}fn main(){}",
        "trait Read{type Item:Copy;}fn bad<T:Read>(x:T.Item)->T.Item{return x+1}fn main(){}",
        "trait Read{type Item;}fn bad<T:Read>(x:T.Item)->(T.Item,T.Item){return (x,x)}fn main(){}",
        "trait Read{type Item:Copy;}struct P{}impl Read for P{type Item=Missing;}fn main(){}",
        "trait Read{type Item:Copy;}struct P{}impl Read for P{}fn main(){}",
        "trait Read{type Item:Copy;}struct P{}impl Read for P{type Item=Self.Item;}fn main(){}",
        "trait Read{type Item:Copy;}fn bad<T>(x:T.Item){}fn main(){}",
    ] {
        let report = check(text);
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{text}: {}",
            report.to_json()
        );
    }
}

#[test]
fn impl_associated_bounds_are_proved_at_definition_and_relay_across_modules() {
    let main = include_str!(
        "../../../tests/language-readiness/terminal/generics-and-constants/associated-bounds/project/main.zil"
    );
    let api = include_str!(
        "../../../tests/language-readiness/terminal/generics-and-constants/associated-bounds/project/api.zil"
    );
    let provider = include_str!(
        "../../../tests/language-readiness/terminal/generics-and-constants/associated-bounds/project/provider.zil"
    );
    let consumer = include_str!(
        "../../../tests/language-readiness/terminal/generics-and-constants/associated-bounds/project/consumer.zil"
    );
    let files = [
        ("main", main),
        ("api", api),
        ("provider", provider),
        ("consumer", consumer),
    ];
    let report = compile_project(&project(&files), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert!(report.verification_runs >= 1, "{}", report.to_json());
    native_both(&files);

    // Number satisfies Marker at the only concrete call, but the recipe's T
    // cannot use that later instance as evidence for its own equation.
    for (label, invalid, diagnostic) in [
        (
            "unproved symbolic bound",
            provider.replace("T:Copy+api.Marker<T>", "T:Copy"),
            "nominal obligation is not among the declared bounds",
        ),
        (
            "wrong nominal provider",
            provider.replace("T:Copy+api.Marker<T>", "T:Copy+api.Marker<i32>"),
            "nominal obligation is not among the declared bounds",
        ),
        (
            "active equation cycle",
            provider.replace(
                "type Item:api.Marker<T>=T",
                "type Item:api.Marker<T>=Self.Item",
            ),
            "active associated type equation cycle",
        ),
        (
            "overlapping implementation head",
            format!(
                "{provider}\nimpl<T:Copy+api.Marker<T>> api.Read<i32> for Box<T>{{type Item=T;fn read()->T{{return .value}}}}"
            ),
            "overlapping nominal implementation heads",
        ),
    ] {
        assert_ne!(invalid, provider, "{label}");
        let altered = [
            ("main", "fn main(){}"),
            ("api", api),
            ("provider", invalid.as_str()),
            ("consumer", consumer),
        ];
        let report = compile_project(&project(&altered), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{label}: {}", report.to_json());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "E-GENERIC-ARGUMENT" && d.message.contains(diagnostic)),
            "{label}: {}",
            report.to_json()
        );
    }
}

#[test]
fn ordinary_containers_and_aliases_preserve_associated_provider_identity() {
    let main = include_str!(
        "../../../tests/language-readiness/terminal/generics-and-constants/associated-container/main.zil"
    );
    let api = include_str!(
        "../../../tests/language-readiness/terminal/generics-and-constants/associated-container/api.zil"
    );
    let model = include_str!(
        "../../../tests/language-readiness/terminal/generics-and-constants/associated-container/model.zil"
    );
    let consumer = include_str!(
        "../../../tests/language-readiness/terminal/generics-and-constants/associated-container/consumer.zil"
    );
    let files = [
        ("main", main),
        ("api", api),
        ("model", model),
        ("consumer", consumer),
    ];
    let report = compile_project(&project(&files), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert!(report.verification_runs >= 1, "{}", report.to_json());
    native_both(&files);

    for invalid in [
        "trait Read{type Item:Copy;}struct Holder<T>{value:T.Item}fn main(){}",
        "trait Read{type Item:Copy;}struct P{}type Ghost=P.Item;fn main(){}",
        "trait Read{type Item:Copy;}struct P{}type Maybe<T:Read>=Option<T.Item>;fn unused(x:Maybe<P>){}fn main(){}",
        "trait Read{type Item:Copy;}type Maybe<T:Read>=Option<T.Item>;fn unused<U>(x:Maybe<U>){}fn main(){}",
        "trait Read{type Item:Copy;}type Phantom<T:Read>=i32;fn main(){}",
        "trait A{type Item:Copy;}trait B{type Item:Copy;}struct P{}impl A for P{type Item=i32;}impl B for P{type Item=bool;}fn bad(x:P.Item)->P.Item{return x}fn main(){}",
        "trait Read{type Item:Copy;}struct P{}impl Read for P{type Item=P.Item;}fn main(){}",
    ] {
        let report = check(invalid);
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{invalid}: {}",
            report.to_json()
        );
        assert_eq!(
            report.phase,
            Phase::Expand,
            "{invalid}: {}",
            report.to_json()
        );
    }
}

#[test]
fn ordinary_iterator_item_can_be_held_in_a_generic_container() {
    let files = [
        (
            "main",
            include_str!(
                "../../../tests/language-readiness/terminal/generics-and-constants/associated-container/iterator/main.zil"
            ),
        ),
        ("iter", include_str!("../../../lib/std/iter.zil")),
        (
            "cursor",
            include_str!(
                "../../../tests/language-readiness/terminal/generics-and-constants/associated-container/iterator/cursor.zil"
            ),
        ),
        (
            "consumer",
            include_str!(
                "../../../tests/language-readiness/terminal/generics-and-constants/associated-container/iterator/consumer.zil"
            ),
        ),
    ];
    let report = compile_project(&project(&files), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert!(report.verification_runs >= 1, "{}", report.to_json());
    native_both(&files);
}

#[test]
fn independent_copy_and_store_element_bounds_prove_generic_entry_storage() {
    let program = include_str!(
        "../../../tests/language-readiness/terminal/generics-and-constants/associated-container/pod-map.zil"
    );
    let report = check(program);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    native_both(&[("main", program)]);

    for invalid in [
        program.replace(
            "K:Copy+StoreElement,V:Copy+StoreElement",
            "K:StoreElement,V:StoreElement",
        ),
        program.replace("K:Copy+StoreElement,V:Copy+StoreElement", "K:Copy,V:Copy"),
        program.replace(
            "struct Entry<K:Copy,V:Copy>{key:K;value:V}",
            "struct Entry<K:Copy,V:Copy>{key:K;value:V;zero:[i32;0]}",
        ),
        program.replace("+Hash,V:Copy+StoreElement", ",V:Copy+StoreElement"),
        program.replace("impl Hash for Key{fn hash()->i64{return i64(.n)}}", ""),
    ] {
        let report = check(&invalid);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    }
}

#[test]
fn noncopy_associated_package_moves_through_ordinary_generic_result() {
    let files = [
        (
            "main",
            include_str!(
                "../../../tests/language-readiness/terminal/generics-and-constants/associated-container/owned/main.zil"
            ),
        ),
        (
            "api",
            include_str!(
                "../../../tests/language-readiness/terminal/generics-and-constants/associated-container/owned/api.zil"
            ),
        ),
        (
            "model",
            include_str!(
                "../../../tests/language-readiness/terminal/generics-and-constants/associated-container/owned/model.zil"
            ),
        ),
    ];
    let report = compile_project(&project(&files), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    native_both(&files);

    let invalid = files[1]
        .1
        .replace("return value", "let moved=value;return value");
    let rejected = compile_project(
        &project(&[
            ("main", files[0].1),
            ("api", &invalid),
            ("model", files[2].1),
        ]),
        &CompileOptions::default(),
    );
    assert_eq!(
        rejected.status,
        ResultCategory::SafeReject,
        "{}",
        rejected.to_json()
    );
    let unused = compile_project(
        &project(&[("main", "import api;fn main(){}"), ("api", &invalid)]),
        &CompileOptions::default(),
    );
    assert_eq!(
        unused.status,
        ResultCategory::SafeReject,
        "{}",
        unused.to_json()
    );
}

#[test]
fn enum_payloads_use_only_declared_copy_associated_projections() {
    for source in [
        include_str!(
            "../../../tests/language-readiness/terminal/generics-and-constants/associated-container/enum.zil"
        ),
        include_str!(
            "../../../tests/language-readiness/terminal/generics-and-constants/associated-container/enum-provider.zil"
        ),
    ] {
        let report = check(source);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert!(report.verification_runs >= 1, "{}", report.to_json());
        native_both(&[("main", source)]);
    }

    for invalid in [
        "trait Read{type Item;}enum Bucket<T:Read>{Value(T.Item)}fn main(){}",
        "trait Read{type Item:Copy;}enum Bucket<T:Read>{Value(T.Missing)}fn main(){}",
        "trait A{type Item:Copy;}trait B{type Item:Copy;}enum Bucket<T:A+B>{Value(T.Item)}fn main(){}",
        "trait Read{type Item:Copy;}struct P{}impl Read for P{type Item=P.Item;}enum Bucket<T:Read>{Value(T.Item)}fn main(){}",
    ] {
        let report = check(invalid);
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{invalid}: {}",
            report.to_json()
        );
        assert_eq!(
            report.phase,
            Phase::Expand,
            "{invalid}: {}",
            report.to_json()
        );
    }
}

#[test]
fn owned_replacement_symbolic_trait_arguments_preserve_declared_and_actual_targets() {
    for text in [
        include_str!(
            "../../../tests/language-readiness/terminal/generics-and-constants/symbolic-trait-arguments.zil"
        ),
        "trait Read<R:Copy>{fn read(self:&Self)->R;}struct P{}impl Read<i32> for P{fn read()->i32{return 7}}fn get<R:Copy,T:Read<R>>(p:&T)->R{return p.read()}fn main()->i32{let p=P{};return get::<i32,P>(&p)-7}",
        "trait Read<R:Copy>{fn read(self:&Self)->R;}struct P{}impl Read<i32> for P{fn read()->i32{return 7}}fn get<R:Copy,T:Read<R>>(p:&T)->R{return p.read()}fn relay<U:Copy,V:Read<U>>(p:&V)->U{return get::<U,V>(p)}fn main()->i32{let p=P{};return relay::<i32,P>(&p)-7}",
        "trait Read<R:Copy>{fn read(self:&Self)->R;}struct Box<T:Copy>{value:T}impl<T:Copy> Read<T> for Box<T>{fn read()->T{return .value}}fn get<R:Copy,T:Read<R>>(p:&T)->R{return p.read()}fn main()->i32{let p=Box::<i32>{value:7};return get::<i32,Box<i32>>(&p)-7}",
        "trait Read<R:Copy>{fn read(self:&Self)->i32;}struct Box<T:Copy>{value:T}impl<T:Copy,R:Copy> Read<R> for Box<T>{fn read()->i32{return 7}}fn identity<T:Copy>(x:T)->T{return x}fn get<R:Copy,T:Read<R>>(p:&T)->i32{return identity(p.read())}fn main()->i32{let p=Box::<bool>{value:true};return get::<i32,Box<bool>>(&p)-7}",
        "trait Flag<R>{}struct U{}fn unused<U,T:Flag<U>>(x:&T){}fn main(){}",
    ] {
        let report = check(text);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{text}: {}",
            report.to_json()
        );
    }
    for text in [
        "trait Read<R:Copy>{fn read(self:&Self)->R;}fn bad<R:Copy,T:Read<R>>(p:&T)->R{return p.read()+1}fn main(){}",
        "trait Read<R:Copy>{fn read(self:&Self)->R;}fn bad<R,T:Read<R>>(p:&T)->R{return p.read()}fn main(){}",
        "trait Read<R>{}struct U{}fn unused<U:Copy,T:Read<U>>(x:&T)->U{return 0}fn main(){}",
    ] {
        let report = check(text);
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{text}: {}",
            report.to_json()
        );
    }
}

#[test]
fn owned_replacement_generic_method_ordinals_reach_raw_wire_and_native() {
    use zil_syntax::ast::{ExprKind, FunctionBody, ItemKind, StmtKind};
    let unit = source(include_str!(
        "../../../tests/language-readiness/terminal/generics-and-constants/generic-method-substitution-growth.zil"
    ));
    let mut ast = zil_syntax::parse(&unit).into_result().unwrap();
    let function = ast
        .items
        .iter_mut()
        .find_map(|item| match &mut item.kind {
            ItemKind::Function(f) if f.name == "choose" => Some(f),
            _ => None,
        })
        .unwrap();
    let FunctionBody::Block(block) = &mut function.body else {
        unreachable!()
    };
    let StmtKind::Return(Some(expression)) = &mut block.statements.last_mut().unwrap().kind else {
        unreachable!()
    };
    let ExprKind::Binary { left, right, .. } = &mut expression.kind else {
        unreachable!()
    };
    fn receiver(expression: &mut zil_syntax::ast::Expr) -> &mut zil_syntax::ast::Expr {
        let ExprKind::Call { callee, .. } = &mut expression.kind else {
            unreachable!()
        };
        let ExprKind::Member { object, .. } = &mut callee.kind else {
            unreachable!()
        };
        object
    }
    receiver(right).span = receiver(left).span;
    fn call_site(expression: &mut zil_syntax::ast::Expr) -> &mut zil_syntax::ast::Expr {
        let ExprKind::Call { callee, .. } = &mut expression.kind else {
            unreachable!()
        };
        callee
    }
    call_site(right).span = call_site(left).span;
    let positions = ast
        .items
        .iter()
        .enumerate()
        .filter_map(|(position, item)| matches!(item.kind, ItemKind::Impl(_)).then_some(position))
        .collect::<Vec<_>>();
    let first_impl_span = ast.items[positions[0]].span;
    let ItemKind::Impl(first_impl) = &ast.items[positions[0]].kind else {
        unreachable!()
    };
    let first_member_span = first_impl.members[0].span;
    let ItemKind::Function(first_method) = &first_impl.members[0].kind else {
        unreachable!()
    };
    let first_function_span = first_method.span;
    ast.items[positions[1]].span = first_impl_span;
    let ItemKind::Impl(second_impl) = &mut ast.items[positions[1]].kind else {
        unreachable!()
    };
    second_impl.members[0].span = first_member_span;
    let ItemKind::Function(second_method) = &mut second_impl.members[0].kind else {
        unreachable!()
    };
    second_method.span = first_function_span;
    let core = zil_expand::expand(&unit, &ast).unwrap();
    let typed = zil_types::check(
        &core,
        &zil_base::TargetProfile::default(),
        &Limits::default(),
    )
    .unwrap();
    let plan = zil_analyze::analyze(&typed, &Limits::default()).unwrap();
    let verified = zil_verify::verify_module(
        typed.clone(),
        plan.clone(),
        &typed.target,
        &Limits::default(),
    )
    .unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &typed.target, &Limits::default()).unwrap();
    native_verified(&verified);
}

#[test]
fn owned_replacement_trait_call_ordinals_keep_shared_source_spans_distinct_in_native_code() {
    use zil_syntax::ast::{ExprKind, FunctionBody, ItemKind, StmtKind};
    let unit = source(include_str!(
        "../../../tests/language-readiness/terminal/generics-and-constants/trait-call-shared-spans.zil"
    ));
    let mut ast = zil_syntax::parse(&unit).into_result().unwrap();
    let function = ast
        .items
        .iter_mut()
        .find_map(|item| match &mut item.kind {
            ItemKind::Function(f) if f.name == "choose" => Some(f),
            _ => None,
        })
        .unwrap();
    let FunctionBody::Block(block) = &mut function.body else {
        unreachable!()
    };
    let StmtKind::Return(Some(expression)) = &mut block.statements[0].kind else {
        unreachable!()
    };
    let ExprKind::Binary { left, right, .. } = &mut expression.kind else {
        unreachable!()
    };
    fn receiver(expression: &mut zil_syntax::ast::Expr) -> &mut zil_syntax::ast::Expr {
        let ExprKind::Call { callee, .. } = &mut expression.kind else {
            unreachable!()
        };
        let ExprKind::Member { object, .. } = &mut callee.kind else {
            unreachable!()
        };
        object
    }
    receiver(right).span = receiver(left).span;
    let positions = ast
        .items
        .iter()
        .enumerate()
        .filter_map(|(position, item)| matches!(item.kind, ItemKind::Impl(_)).then_some(position))
        .collect::<Vec<_>>();
    let first_impl_span = ast.items[positions[0]].span;
    let ItemKind::Impl(first_impl) = &ast.items[positions[0]].kind else {
        unreachable!()
    };
    let first_member_span = first_impl.members[0].span;
    let ItemKind::Function(first_method) = &first_impl.members[0].kind else {
        unreachable!()
    };
    let first_function_span = first_method.span;
    ast.items[positions[1]].span = first_impl_span;
    let ItemKind::Impl(second_impl) = &mut ast.items[positions[1]].kind else {
        unreachable!()
    };
    second_impl.members[0].span = first_member_span;
    let ItemKind::Function(second_method) = &mut second_impl.members[0].kind else {
        unreachable!()
    };
    second_method.span = first_function_span;
    let core = zil_expand::expand(&unit, &ast).unwrap();
    let typed = zil_types::check(
        &core,
        &zil_base::TargetProfile::default(),
        &Limits::default(),
    )
    .unwrap();
    let plan = zil_analyze::analyze(&typed, &Limits::default()).unwrap();
    let verified = zil_verify::verify_module(
        typed.clone(),
        plan.clone(),
        &typed.target,
        &Limits::default(),
    )
    .unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &typed.target, &Limits::default()).unwrap();
    native_verified(&verified);
}

#[test]
fn owned_replacement_nominal_impl_recipes_check_unused_bodies_and_prove_premises() {
    for text in [
        include_str!(
            "../../../tests/language-readiness/terminal/generics-and-constants/generic-impls.zil"
        ),
        "fn id(x:i32)->i32{return x}trait Read{fn read(self:&Self)->i32;}struct Box<F:Copy>{value:F}impl<T:Copy> Read for Box<fn(T)->i32>{fn read()->i32{return 7}}fn main()->i32{let p=Box::<fn(i32)->i32>{value:id};return p.read()-7}",
        "trait Read{fn read(self:&Self)->i32;}struct Box<T:Copy>{value:T}impl<T:Copy> Read for Box<(T,T)>{fn read()->i32{return 7}}impl Read for Box<(i32,bool)>{fn read()->i32{return 8}}fn main(){}",
        include_str!(
            "../../../tests/language-readiness/terminal/generics-and-constants/generic-impl-trait-instances.zil"
        ),
        "trait Left<Tag>{fn left(self:&Self)->i32;}trait Right<Tag>{fn right(self:&Self)->i32;}trait Run{fn run(self:&Self)->i32;}struct P{}impl Left<i32> for P{fn left()->i32{return 7}}impl Right<i32> for P{fn right()->i32{return 11}}struct Box<T:Copy>{value:T}impl<T:Copy> Run for Box<T> where T:Left<i32>+Right<i32>{fn run()->i32{return .value.left()+.value.right()}}fn main()->i32{let p=Box::<P>{value:P{}};return p.run()-18}",
        "trait Read{fn read(self:&Self)->i32;}struct Box<T:Copy>{value:T,n:i32}impl<T:Copy> Read for Box<T>{fn read()->i32{return .n}}fn main()->i32{let p=Box::<i32>{value:3,n:7};return p.read()-7}",
        "trait Base{fn base(self:&Self)->i32;}trait Read:Base{fn read(self:&Self)->i32;}struct Box<T:Copy>{value:T,n:i32}impl<T:Copy> Base for Box<T>{fn base()->i32{return .n}}impl<T:Copy> Read for Box<T>{fn read()->i32{return .n}}fn consume<T:Read>(p:&T)->i32{return p.read()+p.base()}fn main()->i32{let p=Box::<i32>{value:3,n:7};return consume(&p)-14}",
        "trait Base{fn base(self:&Self)->i32;}trait Read{fn read(self:&Self)->i32;}struct P{n:i32}impl Base for P{fn base()->i32{return .n}}struct Box<T:Copy>{value:T}impl<T:Copy> Read for Box<T> where T:Base{fn read(self:&Self)->i32{return self.value.base()}}fn consume<T:Read>(p:&T)->i32{return p.read()}fn main()->i32{let p=Box::<P>{value:P{n:7}};return consume(&p)-7}",
        "trait Read{fn read(self:&Self)->i32;}struct Box<T:Copy>{value:T,n:i32}impl<T:Copy> Read for Box<T>{fn read(self:&Self)->i32{return self.n}}fn consume<T:Read>(p:&T)->i32{return p.read()}fn main()->i32{let p=Box::<i32>{value:3,n:7};return consume(&p)-7}",
        "trait Read{fn read(self:&Self)->i32;}struct Box<T:Copy>{value:T,n:i32}impl<T:Copy> Read for Box<T>{fn read()->i32{return .n}}fn consume<T:Read>(p:&T)->i32{return p.read()}fn main()->i32{let p=Box::<bool>{value:true,n:7};return consume(&p)-7}",
        "trait Read{fn read(self:&Self)->i32;}struct Box<T:Copy>{value:T,n:i32}impl<T> Read for Box<T> where T:Copy{fn read()->i32{return .n}}fn consume<T:Read>(p:&T)->i32{return p.read()}fn main()->i32{let p=Box::<i32>{value:3,n:7};return consume(&p)-7}",
        "trait Read{fn read(self:&Self)->i32;}struct Box<T:Copy>{value:T,n:i32}impl<T:Copy> Read for Box<T>{fn read()->i32{return .n}}fn main(){}",
    ] {
        let report = check(text);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{text}: {}",
            report.to_json()
        );
    }
    for text in [
        include_str!(
            "../../../tests/language-readiness/terminal/generics-and-constants/generic-impl-unused-invalid.zil"
        ),
        "trait Read{}struct Box<T:Copy>{value:T}impl<T:Copy> Read for Box<fn(T)->i32>{}impl<U:Copy> Read for Box<fn(i32)->U>{}fn main(){}",
        include_str!(
            "../../../tests/language-readiness/terminal/generics-and-constants/generic-impl-cycle.zil"
        ),
        include_str!(
            "../../../tests/language-readiness/terminal/generics-and-constants/generic-impl-unknown-trait.zil"
        ),
        "struct Box<T>{}impl Missing for Box<i32>{}fn main(){}",
        include_str!(
            "../../../tests/language-readiness/terminal/generics-and-constants/generic-impl-builtin-drop.zil"
        ),
        "trait Read{fn read(self:&Self)->i32;}struct Box<T:Copy>{value:T}impl<T:Copy> Read for Box<T>{fn read(self:&Self)->i32{return 1}}impl Read for Box<i32>{fn read(self:&Self)->i32{return 2}}fn main(){}",
        "trait Read{}struct Box<T:Copy>{value:T}impl<T:Copy,U> Read for Box<T>{}fn main(){}",
        "trait Read{}struct Box<T:Copy>{value:T}impl<T:Copy> Read for Box<T>{}impl<U:Copy> Read for Box<U>{}fn main(){}",
        "trait Base{}trait Read:Base{}struct Box<T:Copy>{value:T}impl<T:Copy> Read for Box<T>{}fn consume<T:Read>(p:&T){}fn main(){let p=Box::<i32>{value:3};consume(&p)}",
        "trait Base{}trait Read{}struct Box<T:Copy>{value:T}impl<T:Copy> Read for Box<T> where T:Base{}fn consume<T:Read>(p:&T){}fn main(){let p=Box::<i32>{value:3};consume(&p)}",
        "trait Grow{}struct Box<T:Copy>{value:T}impl<T:Copy> Grow for Box<T> where Box<T>:Grow{}fn consume<T:Grow>(p:&T){}fn main(){let p=Box::<i32>{value:3};consume(&p)}",
        "trait Base<T>{}trait Child:Base<Missing>{}fn main(){}",
        "resource R{n:i32}trait Base<T:Copy>{}trait Child:Base<R>{}fn main(){}",
    ] {
        let report = check(text);
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{text}: {}",
            report.to_json()
        );
    }
}

#[test]
fn owned_replacement_impl_growth_is_bounded_and_declared_applied_premises_are_symbolic() {
    let report = compile_source(
        &source(
            "trait Grow{}struct Box<T:Copy>{value:T}impl<T:Copy> Grow for Box<T> where Box<Box<T>>:Grow{}fn consume<T:Grow>(p:&T){}fn main(){let p=Box::<i32>{value:3};consume(&p)}",
        ),
        &CompileOptions {
            limits: Limits {
                max_analysis_steps: 12000,
                ..Limits::default()
            },
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::ResourceLimit,
        "{}",
        report.to_json()
    );
    for text in [
        "trait Base{fn base(self:&Self)->i32;}trait Read{fn read(self:&Self)->i32;}struct Box<T:Copy>{value:T}impl Base for Box<i32>{fn base()->i32{return 7}}impl<T:Copy> Read for Box<T> where Box<T>:Base{fn read(self:&Self)->i32{return self.base()}}fn main()->i32{let p=Box::<i32>{value:3};return p.read()-7}",
        "trait Grow{}struct Box<T:Copy>{value:T}impl<T:Copy> Grow for Box<T> where Box<T>:Grow{}fn main(){}",
        "struct R{n:i32}trait Base<T:Copy>{}trait Child:Base<R>{}fn main(){}",
        "trait Base<T:Copy>{}trait Child:Copy+Base<Self>{}fn main(){}",
    ] {
        let report = check(text);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{text}: {}",
            report.to_json()
        );
    }
    for text in [
        "trait Base<T>{}trait Child:Base<Missing>{}fn main(){}",
        "resource R{n:i32}trait Base<T:Copy>{}trait Child:Base<R>{}fn main(){}",
        "resource R{n:i32}struct Box<T:Copy>{value:T}trait Base<T>{}trait Child:Base<Box<R>>{}fn main(){}",
    ] {
        let report = check(text);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    }
}

#[test]
fn owned_replacement_supertrait_evidence_is_committed_after_inherited_obligations() {
    for text in [
        "trait Base{fn base(self:&Self)->i32;}trait Child:Base{fn child(self:&Self)->i32;}struct P{n:i32}impl Base for P{fn base()->i32{return .n}}impl Child for P{fn child()->i32{return .n}}fn consume<T:Child>(p:&T)->i32{return p.base()+p.child()}fn main()->i32{let p=P{n:7};return consume(&p)-14}",
        "trait Base{fn base(self:&Self)->i32;}trait Child:Base{}trait Top:Child{}struct P{n:i32}impl Top for P{}impl Child for P{}impl Base for P{fn base()->i32{return .n}}fn consume<T:Top>(p:&T)->i32{return p.base()}fn main()->i32{let p=P{n:7};return consume(&p)-7}",
        "trait Base{}trait Child:Base{}fn unused<T:Child>(p:&T){}fn main(){}",
        "trait Base<R>{fn base(self:&Self)->R;}trait Child:Base<i32>{fn child(self:&Self)->i32{return self.base()}}struct P{n:i32}impl Child for P{}impl Base<i32> for P{fn base()->i32{return .n}}fn consume<T:Child>(p:&T)->i32{return p.child()}fn main()->i32{let p=P{n:7};return consume(&p)-7}",
        "trait Child:Copy{}struct P{n:i32}impl Child for P{}fn duplicate<T:Child>(p:T)->T{let q=p;return p}fn main()->i32{let p=duplicate(P{n:7});return p.n-7}",
        "trait Child:Copy{fn identity(self:Self)->Self{let q=self;return self}}struct P{n:i32}impl Child for P{}fn main()->i32{let p=P{n:7}.identity();return p.n-7}",
        "trait A{fn get(self:&Self)->i32;}trait B:A{}trait C:A{}trait D:B+C{}struct P{}impl D for P{}impl C for P{}impl B for P{}impl A for P{fn get()->i32{return 7}}fn consume<T:D>(p:&T)->i32{return p.get()}fn main()->i32{let p=P{};return consume(&p)-7}",
    ] {
        let report = check(text);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{text}: {}",
            report.to_json()
        );
    }
    for text in [
        include_str!(
            "../../../tests/language-readiness/terminal/generics-and-constants/supertrait-missing-premise.zil"
        ),
        "trait A:B{}trait B:A{}fn main(){}",
        "trait A:A{}fn main(){}",
        "trait Base{}trait Child:Base{}fn unused<T:Child>(p:&T)->i32{return p.missing()}fn main(){}",
        "trait Base<R>{}trait Child:Base<i64>{}struct P{}impl Base<i32> for P{}impl Child for P{}fn main(){}",
    ] {
        let report = check(text);
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{text}: {}",
            report.to_json()
        );
    }
    let growing = compile_source(
        &source("trait Grow<T>:Grow<(T,T)>{}fn main(){}"),
        &CompileOptions {
            limits: Limits {
                max_analysis_steps: 4000,
                ..Limits::default()
            },
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        growing.status,
        ResultCategory::ResourceLimit,
        "{}",
        growing.to_json()
    );
}

#[test]
fn owned_replacement_const_module_namespaces_and_forwarding_preserve_actual_binders() {
    let files = [
        (
            "main",
            "import values;fn main()->i32{return values.forwarded::<7>()-7}",
        ),
        (
            "values",
            "pub struct N{n:i32}pub fn scalar<#N:i32>()->i32{return N}pub fn forwarded<#N:i32>()->i32{return scalar::<N>()}",
        ),
    ];
    let report = compile_project(&project(&files), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    native_both(&files);
    for text in [
        "struct N{n:i32}fn value<#N:i32>(x:N)->i32{return x.n+N}fn main()->i32{return value::<7>(N{n:2})-9}",
        "struct N{n:i32}fn scalar<#N:i32>()->i32{return N}fn main()->i32{return scalar::<7>()-7}",
    ] {
        let report = check(text);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{text}: {}",
            report.to_json()
        );
    }
}

#[test]
fn owned_replacement_supertrait_wire_native_and_const_shadow_budgets() {
    use zil_driver::{CompilationSession, DiskCacheStatus};
    let super_source = include_str!(
        "../../../tests/language-readiness/terminal/generics-and-constants/supertraits.zil"
    );
    let const_source = include_str!(
        "../../../tests/language-readiness/terminal/generics-and-constants/const-match-shadow.zil"
    );
    let impl_source = include_str!(
        "../../../tests/language-readiness/terminal/generics-and-constants/generic-impls.zil"
    );
    let symbolic_source = include_str!(
        "../../../tests/language-readiness/terminal/generics-and-constants/symbolic-trait-arguments.zil"
    );
    let unit = source(super_source);
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    let typed = zil_types::check(
        &core,
        &zil_base::TargetProfile::default(),
        &Limits::default(),
    )
    .unwrap();
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
    for width in [32, 64] {
        let target = zil_base::TargetProfile {
            pointer_width: width,
            ..Default::default()
        };
        let report = compile_project(
            &project(&[
                (
                    "main",
                    "import model;fn main(){model.value::<4294967296>()}",
                ),
                ("model", "pub fn value<#N:usize>(){}"),
            ]),
            &CompileOptions {
                target,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            if width == 32 {
                ResultCategory::SafeReject
            } else {
                ResultCategory::Success
            },
            "{}",
            report.to_json()
        );
    }
    native_both(&[("main", super_source)]);
    native_both(&[("main", impl_source)]);
    native_both(&[("main", symbolic_source)]);
    let associated_source = include_str!(
        "../../../tests/language-readiness/terminal/generics-and-constants/associated-type-instances.zil"
    );
    native_both(&[("main", associated_source)]);
    let unsigned_source = include_str!(
        "../../../tests/language-readiness/terminal/generics-and-constants/const-unsigned.zil"
    );
    let ct_source = include_str!(
        "../../../tests/language-readiness/terminal/generics-and-constants/ct-generics-reflection-syntax.zil"
    );
    let associated_constants_source = include_str!(
        "../../../tests/language-readiness/terminal/generics-and-constants/associated-constants.zil"
    );
    let computed_constants_source = include_str!(
        "../../../tests/language-readiness/terminal/generics-and-constants/associated-constant-computed.zil"
    );
    for text in [
        unsigned_source,
        ct_source,
        associated_constants_source,
        computed_constants_source,
    ] {
        native_both(&[("main", text)]);
    }
    // Recover the actual CT-materialized raw candidate from the existing trusted
    // frontend cache, then replay both independent verifier entry points.
    for ct_source in [ct_source, computed_constants_source] {
        let ct_cache = std::env::temp_dir().join(format!(
            "zil-nominal-trait-bounds-ct-wire-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let ct_options = CompileOptions {
            semantic_cache: Some(ct_cache.clone()),
            ..CompileOptions::default()
        };
        let ct_report = compile_source(&source(ct_source), &ct_options);
        assert_eq!(
            ct_report.status,
            ResultCategory::Success,
            "{}",
            ct_report.to_json()
        );
        let ct_store = ct_cache.join("semantic-v4");
        let mut replayed = 0;
        for entry in fs::read_dir(&ct_store).unwrap() {
            let path = entry.unwrap().path();
            if path
                .extension()
                .is_none_or(|extension| extension != "candidate")
            {
                continue;
            }
            let key = zil_base::Fingerprint::from_hex(path.file_stem().unwrap().to_str().unwrap())
                .unwrap();
            let zil_db::CacheRead::Candidate(payload) =
                zil_db::ArtifactCache::with_limit(&ct_store, Limits::default().max_source_bytes)
                    .inspect_candidate(key)
                    .unwrap()
            else {
                panic!("actual CT raw cache candidate absent")
            };
            assert_eq!(&payload[..8], b"ZILSEM04");
            let length = u32::from_le_bytes(payload[8..12].try_into().unwrap()) as usize;
            let bytes = &payload[12..12 + length];
            let (typed, plan) =
                zil_ir::wire::decode_module_candidate(bytes, &Limits::default()).unwrap();
            zil_verify::verify_module(typed.clone(), plan, &typed.target, &Limits::default())
                .unwrap();
            zil_verify::verify_module_bytes(bytes, &typed.target, &Limits::default()).unwrap();
            replayed += 1;
        }
        assert!(replayed > 0);
        fs::remove_dir_all(ct_cache).unwrap();
    }
    native_both(&[
        (
            "main",
            "import rules;import model;fn main()->i32{return rules.get::<model.P>()-7}",
        ),
        (
            "rules",
            "fn default_value()->i32{return 7}pub trait C{const N:i32=default_value();}pub fn get<T:C>()->i32{return T.N}",
        ),
        (
            "model",
            "import rules;pub struct P{}fn default_value()->i32{return 99}impl rules.C for P{}",
        ),
    ]);
    for text in [unsigned_source, associated_constants_source] {
        let unit = source(text);
        let ast = zil_syntax::parse(&unit).into_result().unwrap();
        let core = zil_expand::expand(&unit, &ast).unwrap();
        let typed = zil_types::check(
            &core,
            &zil_base::TargetProfile::default(),
            &Limits::default(),
        )
        .unwrap();
        let plan = zil_analyze::analyze(&typed, &Limits::default()).unwrap();
        zil_verify::verify_module(
            typed.clone(),
            plan.clone(),
            &typed.target,
            &Limits::default(),
        )
        .unwrap();
        let wire =
            zil_ir::wire::encode_module_candidate(&typed, &plan, &Limits::default()).unwrap();
        zil_verify::verify_module_bytes(&wire, &typed.target, &Limits::default()).unwrap();
    }
    native_both(&[
        (
            "main",
            "import model;import rules;fn main()->i32{let p=model.make();return rules.get::<model.Box<i32>>(&p)-7}",
        ),
        (
            "rules",
            "pub trait Read{type Item:Copy;fn read(self:&Self)->Self.Item;}pub fn get<T:Read>(p:&T)->T.Item{return p.read()}",
        ),
        (
            "model",
            "import rules;pub struct Box<T:Copy>{value:T}impl<T:Copy> rules.Read for Box<T>{type Item=T;fn read()->Self.Item{return .value}}pub fn make()->Box<i32>{return Box::<i32>{value:7}}",
        ),
    ]);

    let associated_unit = source(associated_source);
    let associated_ast = zil_syntax::parse(&associated_unit).into_result().unwrap();
    let associated_core = zil_expand::expand(&associated_unit, &associated_ast).unwrap();
    let associated_typed = zil_types::check(
        &associated_core,
        &zil_base::TargetProfile::default(),
        &Limits::default(),
    )
    .unwrap();
    let associated_plan = zil_analyze::analyze(&associated_typed, &Limits::default()).unwrap();
    zil_verify::verify_module(
        associated_typed.clone(),
        associated_plan.clone(),
        &associated_typed.target,
        &Limits::default(),
    )
    .unwrap();
    let associated_wire = zil_ir::wire::encode_module_candidate(
        &associated_typed,
        &associated_plan,
        &Limits::default(),
    )
    .unwrap();
    zil_verify::verify_module_bytes(
        &associated_wire,
        &associated_typed.target,
        &Limits::default(),
    )
    .unwrap();

    native_both(&[
        (
            "main",
            "import model;import rules;fn main()->i32{let p=model.make();if rules.send::<bool,model.Box<bool>>(&p,true){return rules.send::<i32,model.Box<bool>>(&p,7)-7}return 1}",
        ),
        (
            "rules",
            "pub trait Echo<R:Copy>{fn echo(self:&Self,value:R)->R;}pub fn send<R:Copy,T:Echo<R>>(p:&T,value:R)->R{return p.echo(value)}",
        ),
        (
            "model",
            "import rules;pub struct Box<T:Copy>{value:T}impl<T:Copy,R:Copy> rules.Echo<R> for Box<T>{fn echo(value:R)->R{return value}}pub fn make()->Box<bool>{return Box::<bool>{value:true}}",
        ),
    ]);
    let symbolic_unit = source(symbolic_source);
    let symbolic_ast = zil_syntax::parse(&symbolic_unit).into_result().unwrap();
    let symbolic_core = zil_expand::expand(&symbolic_unit, &symbolic_ast).unwrap();
    let symbolic_typed = zil_types::check(
        &symbolic_core,
        &zil_base::TargetProfile::default(),
        &Limits::default(),
    )
    .unwrap();
    let symbolic_plan = zil_analyze::analyze(&symbolic_typed, &Limits::default()).unwrap();
    zil_verify::verify_module(
        symbolic_typed.clone(),
        symbolic_plan.clone(),
        &symbolic_typed.target,
        &Limits::default(),
    )
    .unwrap();
    let symbolic_wire =
        zil_ir::wire::encode_module_candidate(&symbolic_typed, &symbolic_plan, &Limits::default())
            .unwrap();
    zil_verify::verify_module_bytes(&symbolic_wire, &symbolic_typed.target, &Limits::default())
        .unwrap();
    native_both(&[(
        "main",
        include_str!(
            "../../../tests/language-readiness/terminal/generics-and-constants/generic-impl-trait-instances.zil"
        ),
    )]);
    native_both(&[
        (
            "main",
            "import model;import rules;fn main()->i32{let p=model.make();return rules.consume(&p)-7}",
        ),
        (
            "rules",
            "pub trait Read{fn read(self:&Self)->i32;}pub fn consume<T:Read>(p:&T)->i32{return p.read()}",
        ),
        (
            "model",
            "import rules;pub struct Box<T:Copy>{value:T,n:i32}impl<T:Copy> rules.Read for Box<T>{fn read()->i32{return .n}}pub fn make()->Box<i32>{return Box::<i32>{value:3,n:7}}",
        ),
    ]);
    let orphan = compile_project(
        &project(&[
            (
                "main",
                "import model;import rules;struct Local{}impl<T:Copy> rules.Read for model.Box<T>{}fn main(){}",
            ),
            ("rules", "pub trait Read{}"),
            ("model", "pub struct Box<T:Copy>{value:T}"),
        ]),
        &CompileOptions::default(),
    );
    assert_eq!(
        orphan.status,
        ResultCategory::SafeReject,
        "{}",
        orphan.to_json()
    );
    assert!(
        orphan.diagnostics[0].message.contains("outer Self"),
        "{}",
        orphan.to_json()
    );
    let impl_unit = source(impl_source);
    let impl_ast = zil_syntax::parse(&impl_unit).into_result().unwrap();
    let expand_at = |work| {
        zil_expand::expand_with_map_limits(
            &impl_unit,
            &impl_ast,
            &Limits {
                max_analysis_steps: work,
                ..Limits::default()
            },
        )
    };
    let (mut low, mut high) = (1, Limits::default().max_analysis_steps);
    while low < high {
        let middle = low + (high - low) / 2;
        match expand_at(middle) {
            Ok(_) => high = middle,
            Err(errors) => {
                assert_eq!(
                    errors[0].category,
                    ResultCategory::ResourceLimit,
                    "{errors:?}"
                );
                low = middle + 1;
            }
        }
    }
    assert!(expand_at(low).is_ok());
    assert!(expand_at(low).is_ok());
    assert_eq!(
        expand_at(low - 1).unwrap_err()[0].category,
        ResultCategory::ResourceLimit
    );
    eprintln!("341 generic-impl Expand work N={low}; N-1 ResourceLimit; fresh N identical");
    let impl_core = zil_expand::expand(&impl_unit, &impl_ast).unwrap();
    let impl_typed = zil_types::check(
        &impl_core,
        &zil_base::TargetProfile::default(),
        &Limits::default(),
    )
    .unwrap();
    let impl_plan = zil_analyze::analyze(&impl_typed, &Limits::default()).unwrap();
    zil_verify::verify_module(
        impl_typed.clone(),
        impl_plan.clone(),
        &impl_typed.target,
        &Limits::default(),
    )
    .unwrap();
    let impl_wire =
        zil_ir::wire::encode_module_candidate(&impl_typed, &impl_plan, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&impl_wire, &impl_typed.target, &Limits::default()).unwrap();
    native_both(&[
        (
            "main",
            "import rules;import model;fn main()->i32{let p=model.make();return rules.consume(&p)-7}",
        ),
        (
            "rules",
            "pub trait Base{fn base(self:&Self)->i32;}pub trait Child:Base{}pub fn consume<T:Child>(p:&T)->i32{return p.base()}",
        ),
        (
            "model",
            "import rules;pub struct P{n:i32}pub fn make()->P{return P{n:7}}impl rules.Child for P{}impl rules.Base for P{fn base()->i32{return .n}}",
        ),
    ]);
    for (label, text) in [
        ("supertrait", super_source),
        ("const-match-shadow", const_source),
        ("generic-impl", impl_source),
        ("symbolic-trait-arguments", symbolic_source),
        ("associated-types", associated_source),
        ("unsigned-const", unsigned_source),
        ("associated-constants", associated_constants_source),
        ("associated-constant-computed", computed_constants_source),
        ("ct-generic-type-syntax-reflection", ct_source),
        (
            "generic-method-substitution-growth",
            include_str!(
                "../../../tests/language-readiness/terminal/generics-and-constants/generic-method-substitution-growth.zil"
            ),
        ),
        (
            "generic-method-const-growth",
            include_str!(
                "../../../tests/language-readiness/terminal/generics-and-constants/generic-method-const-growth.zil"
            ),
        ),
        (
            "generic-method-no-outer",
            include_str!(
                "../../../tests/language-readiness/terminal/generics-and-constants/generic-method-no-outer.zil"
            ),
        ),
        (
            "generic-methods",
            include_str!(
                "../../../tests/language-readiness/terminal/generics-and-constants/generic-methods.zil"
            ),
        ),
    ] {
        let unit = source(text);
        for ledger in ["work", "nodes"] {
            let options = |n| {
                let mut options = CompileOptions::default();
                if ledger == "work" {
                    options.limits.max_analysis_steps = n;
                } else {
                    options.limits.max_nodes = n;
                }
                options
            };
            let (mut low, mut high) = (
                1,
                if ledger == "work" {
                    Limits::default().max_analysis_steps
                } else {
                    Limits::default().max_nodes
                },
            );
            while low < high {
                let n = low + (high - low) / 2;
                let report = compile_source(&unit, &options(n));
                match report.status {
                    ResultCategory::Success => high = n,
                    ResultCategory::ResourceLimit => low = n + 1,
                    _ => panic!("{}", report.to_json()),
                }
            }
            let mut session = CompilationSession::default();
            for (n, expected) in [
                (low, ResultCategory::Success),
                (low - 1, ResultCategory::ResourceLimit),
                (low, ResultCategory::Success),
            ] {
                for report in [
                    session.compile_source(&unit, &options(n)),
                    CompilationSession::default().compile_source(&unit, &options(n)),
                ] {
                    assert_eq!(report.status, expected, "{}", report.to_json());
                }
                assert_eq!(session.active_query_count(), 0);
            }
            eprintln!("341 {label} request {ledger} N={low}; N-1/cold/warm/fresh agree");
        }
        let cache = std::env::temp_dir().join(format!(
            "zil-nominal-trait-bounds-cache-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let options = CompileOptions {
            cache_dir: Some(cache.clone()),
            ..CompileOptions::default()
        };
        let cold = compile_source(&unit, &options);
        let warm = compile_source(&unit, &options);
        assert_eq!(cold.status, ResultCategory::Success, "{}", cold.to_json());
        assert_eq!(warm.status, ResultCategory::Success, "{}", warm.to_json());
        assert_eq!(warm.disk_cache, DiskCacheStatus::Hit);
        assert_eq!(warm.verification_runs, 1);
        fs::remove_dir_all(cache).unwrap();
    }
}

#[test]
fn trait_default_unused_and_instantiated_reference_defaults_keep_the_same_boundary() {
    let cases = [
        (
            "local-address",
            "trait Leak{fn bad(self:&Self)->&i32{let local=7;return &local}}",
            "struct P{}impl Leak for P{}",
        ),
        (
            "byvalue-receiver",
            "trait Leak{fn borrow(self:&Self)->&i32;fn bad(self:Self)->&i32{return self.borrow()}}",
            "struct P{n:i32}impl Leak for P{fn borrow(self:&Self)->&i32{return &self.n}}",
        ),
        (
            "concrete-local",
            "trait Leak{fn borrow(self:&Self)->&i32;fn bad(self:&Self)->&i32{let local=P{n:7};return local.borrow()}}",
            "struct P{n:i32}impl Leak for P{fn borrow(self:&Self)->&i32{return &self.n}}",
        ),
        (
            "shadowed-reference",
            "trait Leak{fn bad(self:&Self,value:&i32)->&i32{let value=7;return &value}}",
            "struct P{}impl Leak for P{}",
        ),
    ];
    let mut mismatches = Vec::new();
    for (label, declaration, implementation) in cases {
        let unused = if label == "concrete-local" {
            "struct P{n:i32}impl P{fn borrow(self:&Self)->&i32{return &self.n}}trait Leak{fn bad(self:&Self)->&i32{let local=P{n:7};return local.borrow()}}fn main(){}".to_owned()
        } else {
            format!("{declaration}fn main(){{}}")
        };
        let implemented = format!("{declaration}{implementation}fn main(){{}}");
        let called = match label {
            "local-address" => format!(
                "{declaration}{implementation}fn main()->i32{{let p=P{{}};return *p.bad()-7}}"
            ),
            "shadowed-reference" => format!(
                "{declaration}{implementation}fn main()->i32{{let p=P{{}};let value=1;return *p.bad(&value)-7}}"
            ),
            _ => format!(
                "{declaration}{implementation}fn main()->i32{{let p=P{{n:7}};return *p.bad()-7}}"
            ),
        };
        for (form, text) in [
            ("unused", unused),
            ("implemented", implemented),
            ("called", called),
        ] {
            let report = check(&text);
            eprintln!(
                "review330 {label}/{form}: {:?}/{:?}/{:?}",
                report.status,
                report.phase,
                report
                    .diagnostics
                    .iter()
                    .map(|d| &d.code)
                    .collect::<Vec<_>>()
            );
            if report.status != ResultCategory::Unsupported
                || report.phase != Phase::Expand
                || report
                    .diagnostics
                    .first()
                    .is_none_or(|d| d.code != "E-UNSUPPORTED")
                || report.verification_runs != 0
            {
                mismatches.push((
                    label,
                    form,
                    format!(
                        "{:?}/{:?}/{:?}",
                        report.status,
                        report.phase,
                        report
                            .diagnostics
                            .iter()
                            .map(|d| &d.code)
                            .collect::<Vec<_>>()
                    ),
                ));
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "unused or instantiated default escaped the finite declaration boundary: {mismatches:?}"
    );
}

#[test]
fn trait_default_receiver_shadow_is_rejected_before_an_instance_is_selected() {
    for suffix in [
        "fn main(){}",
        "struct P{}impl Shadow for P{}fn main()->i32{let p=P{};return p.read()}",
    ] {
        let report = check(&format!(
            "trait Shadow{{fn read(self:&Self)->i32{{let self=7;return 7}}}}{suffix}"
        ));
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert_eq!(report.diagnostics[0].code, "E-RECEIVER-RESERVED-NAME");
        assert_eq!(report.verification_runs, 0);
    }
}

#[test]
fn trait_default_real_reference_forwarding_and_parameter_shadow_keep_native_meaning() {
    native_both(&[(
        "main",
        "trait Borrow{fn required(self:&Self)->&i32;fn forward(self:&Self)->&i32{return self.required()}fn shadow(self:&Self,value:i32)->i32{let value=value+1;return value}}struct P{n:i32}impl Borrow for P{fn required(self:&Self)->&i32{return &self.n}}fn main()->i32{let p=P{n:41};let q=p.forward();return *q+p.shadow(0)-42}",
    )]);
    native_both(&[(
        "main",
        "trait Score<Tag>{fn required(self:&Self)->i32;fn score(self:&Self)->i32{return self.required()+1}}struct P{}impl Score<bool> for P{fn required()->i32{return 10}}impl Score<i32> for P{fn required()->i32{return 20}}fn left<T:Score<bool>>(p:&T)->i32{return p.score()}fn right<T:Score<i32>>(p:&T)->i32{return p.score()}fn main()->i32{let p=P{};if left(&p)!=11{return 1}return right(&p)-21}",
    )]);
    native_both(&[
        (
            "main",
            "import rules;import data;fn main()->i32{let p=data.P{};return rules.run(&p)-43}",
        ),
        (
            "rules",
            "fn hidden()->i32{return 41}pub trait Score{fn score(self:&Self)->i32{return hidden()+2}}pub fn run<T:Score>(p:&T)->i32{return p.score()}",
        ),
        (
            "data",
            "import rules;fn hidden()->i32{return 99}pub struct P{}impl rules.Score for P{}",
        ),
    ]);
}

#[test]
fn trait_default_required_body_escape_cannot_gain_a_verify_proof() {
    let unit = source(
        "trait Borrow{fn required(self:&Self)->&i32;fn forward(self:&Self)->&i32{return self.required()}}struct P{n:i32}impl Borrow for P{fn required(self:&Self)->&i32{let local=7;let view=&local;let consumed=*view;return &self.n}}fn main()->i32{let p=P{n:7};return *p.forward()-7}",
    );
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    let mut module = zil_types::check(
        &core,
        &zil_base::TargetProfile::default(),
        &Limits::default(),
    )
    .unwrap();
    let mut plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let function=module.functions.iter_mut().find(|f|f.blocks.iter().flat_map(|b|&b.instructions).any(|i|matches!(i.kind,zil_ir::InstructionKind::BorrowLocal{local} if f.locals[local.0 as usize].ty==zil_ir::Type::I32))).unwrap();
    let function_id = function.id;
    let root=function.blocks.iter().flat_map(|b|&b.instructions).find_map(|i|matches!(i.kind,zil_ir::InstructionKind::BorrowLocal{local} if function.locals[local.0 as usize].ty==zil_ir::Type::I32).then_some(i.result).flatten()).unwrap();
    for block in &mut function.blocks {
        if matches!(block.terminator, zil_ir::Terminator::Return(_)) {
            block.terminator = zil_ir::Terminator::Return(Some(root));
        }
    }
    // Unannotated ordinary returns use the established architecture: Analyze
    // produces a candidate and records the local root; Verify independently
    // refuses to turn that candidate into a safety proof.
    plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    assert!(
        plan.functions
            .iter()
            .find(|f| f.function == function_id)
            .unwrap()
            .holds[root.0 as usize]
            .roots
            .iter()
            .any(|r| matches!(r, zil_ir::HoldRoot::Local(_)))
    );
    eprintln!("review330 raw Analyze: CandidatePlan, local origin retained");
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    for result in [
        zil_verify::verify_module(module.clone(), plan, &module.target, &Limits::default()),
        zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()),
    ] {
        let errors = result.unwrap_err();
        eprintln!(
            "review330 raw Verify: {:?}",
            errors.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(
            errors.iter().any(|d| d.code == "E-BORROW-ESCAPE"),
            "{errors:?}"
        );
    }
}

#[test]
fn trait_default_original_pending_sources() {
    // Original Unsupported/Types baseline remains in commit 43bcafbd0; preserve source bytes.
    for text in [
        include_str!("../../../tests/language-readiness/pending/default-trait-method.zil"),
        include_str!("../../../tests/language-readiness/recovery/pending/trait-default-method.zil"),
    ] {
        let report = check(text);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
}

#[test]
fn trait_default_default_delegation_and_instance_override_are_native() {
    native_both(&[(
        "main",
        include_str!("../../../tests/language-readiness/recovery/pending/trait-default-method.zil"),
    )]);
    native_both(&[(
        "main",
        r#"
trait Read<R:Copy>{
 fn read(self:&Self)->R;
 fn delegated(self:&Self)->R{return self.read()}
}
struct Point{value:i32}
impl Read<i32> for Point{fn read()->i32{return .value}}
impl Read<i64> for Point{
 fn read()->i64{return i64(.value)+2}
 fn delegated()->i64{return 29}
}
fn narrow<T:Read<i32>>(x:&T)->i32{return x.delegated()}
fn wide<T:Read<i64>>(x:&T)->i64{return x.delegated()}
fn main()->i32{let p=Point{value:7};if narrow(&p)!=7{return 1}if wide(&p)!=29{return 2}return 0}
"#,
    )]);
}

#[test]
fn trait_default_same_self_two_default_instances_keep_ordered_phantom_identity() {
    native_both(&[(
        "main",
        r#"
trait Tag<A:Copy,B:Copy>{
 fn base(self:&Self)->i32;
 fn delegated(self:&Self)->i32{return self.base()}
}
struct Point{}
impl Tag<i32,bool> for Point{fn base()->i32{return 17}}
impl Tag<bool,i32> for Point{fn base()->i32{return 29}}
fn first<T:Tag<i32,bool>>(x:&T)->i32{return x.delegated()}
fn second<T>(x:&T)->i32 where T:Tag<bool,i32>{return x.delegated()}
fn main()->i32{let p=Point{};if first(&p)!=17{return 1}if second(&p)!=29{return 2}return 0}
"#,
    )]);
}

#[test]
fn trait_default_default_body_keeps_private_helper_module_and_actual_host_dependencies() {
    native_both(&[
        (
            "main",
            "import rules;import data;fn main()->i32{let p=data.Point{};return rules.apply(&p)-7}",
        ),
        (
            "rules",
            "fn helper()->i32{return 7}pub trait Read<R:Copy>{fn read(self:&Self)->i32{return helper()}}pub fn apply<T:Read<i32>>(x:&T)->i32{return x.read()}",
        ),
        (
            "data",
            "import rules;pub struct Point{}fn helper()->i32{return 99}impl rules.Read<i32> for Point{}",
        ),
    ]);
}

#[test]
fn trait_owned_helper_original_global_function_value_reaches_both_native_backends() {
    native_both(&[(
        "main",
        include_str!(
            "../../../tests/language-readiness/function-pointer-gaps/default-global-fnptr-known-incomplete.zil"
        ),
    )]);
}

#[test]
fn trait_owned_helper_private_function_values_keep_trait_module_and_full_signature() {
    native_both(&[
        (
            "main",
            include_str!(
                "../../../tests/language-readiness/function-pointer-shadowing/private-fnptr/main.zil"
            ),
        ),
        (
            "rules",
            include_str!(
                "../../../tests/language-readiness/function-pointer-shadowing/private-fnptr/rules.zil"
            ),
        ),
        (
            "component",
            include_str!(
                "../../../tests/language-readiness/function-pointer-shadowing/private-fnptr/component.zil"
            ),
        ),
    ]);
}

#[test]
fn trait_owned_helper_real_local_bindings_and_existing_fnptr_forwarding_take_precedence() {
    native_both(&[(
        "main",
        include_str!(
            "../../../tests/language-readiness/function-pointer-shadowing/local-shadow.zil"
        ),
    )]);
}

#[test]
fn trait_owned_helper_unused_and_overridden_function_values_keep_complete_symbolic_identity() {
    for text in [
        "fn identity(value:i32)->i32{return value}trait Factory{fn handler(self:&Self)->fn(i32)->i32{return identity}}fn main(){}",
        "fn identity(value:i32)->i32{return value}trait Factory<R:Copy>{fn handler(self:&Self)->fn(i32)->i32{return identity}}struct P{}impl Factory<bool> for P{fn handler()->fn(i32)->i32{return identity}}fn main(){}",
    ] {
        let report = check(text);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
    for declaration in [
        "fn identity(value:i64)->i32{return 0}trait Factory{fn handler(self:&Self)->fn(i32)->i32{return identity}}",
        "fn identity(value:i32)->i64{return 0}trait Factory{fn handler(self:&Self)->fn(i32)->i32{return identity}}",
        "fn identity(value:i32,other:i32)->i32{return value}trait Factory{fn handler(self:&Self)->fn(i32)->i32{return identity}}",
        "struct A{n:i32}struct B{n:i32}fn identity(value:A)->A{return value}trait Factory{fn handler(self:&Self)->fn(B)->B{return identity}}",
        "struct Tag<A:Copy,B:Copy>{}fn identity(value:Tag<i32,bool>)->Tag<i32,bool>{return value}trait Factory{fn handler(self:&Self)->fn(Tag<bool,i32>)->Tag<bool,i32>{return identity}}",
        "fn identity(value:i32)->i32{return value}trait Factory{fn handler(self:&Self)->fn(i32)->i32{let identity=7;return identity}}",
    ] {
        for suffix in [
            "fn main(){}",
            "struct P{}impl Factory for P{fn handler()->fn(i32)->i32{return fallback}}fn fallback(value:i32)->i32{return value}fn main(){}",
        ] {
            // An unrelated/wrong override cannot hide the original bad default.
            let report = check(&format!("{declaration}{suffix}"));
            assert_eq!(
                report.status,
                ResultCategory::SafeReject,
                "{}",
                report.to_json()
            );
            assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
            assert_eq!(
                report.diagnostics[0].code,
                "E-GENERIC-ARGUMENT",
                "{}",
                report.to_json()
            );
            assert!(
                report.diagnostics[0]
                    .message
                    .contains("before concrete instantiation"),
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 0);
        }
    }
    for symbolic in [
        "fn identity(value:i32)->i32{return value}trait Factory<R:Copy>{fn handler(self:&Self)->fn(R)->R{return identity}}",
        "struct R{n:i32}fn identity(value:R)->R{return value}trait Factory<R:Copy>{fn handler(self:&Self)->fn(R)->R{return identity}}",
    ] {
        for suffix in [
            "fn main(){}",
            "struct P{}impl Factory<i32> for P{}fn main(){}",
            "struct P{}impl Factory<i32> for P{fn handler()->fn(i32)->i32{return identity}}fn main(){}",
        ] {
            reject(
                &format!("{symbolic}{suffix}"),
                "before concrete instantiation",
            );
        }
    }
}

#[test]
fn trait_owned_helper_foreign_generic_inferred_and_noncopy_function_values_stay_explicitly_unsupported()
 {
    for text in [
        "fn identity<T:Copy>(value:T)->T{return value}trait Factory{fn handler(self:&Self)->fn(i32)->i32{return identity}}fn main(){}",
        "fn identity(value:i32){return value}trait Factory{fn handler(self:&Self)->fn(i32)->i32{return identity}}fn main(){}",
        "fn identity(value:&i32)->i32{return *value}trait Factory{fn handler(self:&Self)->fn(&i32)->i32{return identity}}fn main(){}",
        "resource R{}fn identity(value:R)->R{return value}trait Factory{fn handler(self:&Self)->fn(R)->R{return identity}}fn main(){}",
        "resource R{}fn identity(value:R)->R{return value}trait Factory<R:Copy>{fn handler(self:&Self)->fn(R)->R{return identity}}fn main(){}",
        "extern \"C\" fn identity(value:i32)->i32;trait Factory{fn handler(self:&Self)->fn(i32)->i32{return identity}}fn main(){}",
    ] {
        let report = check(text);
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert_eq!(
            report.diagnostics[0].code,
            "E-UNSUPPORTED",
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
    }
}

#[test]
fn trait_owned_helper_function_value_lookup_keeps_private_names_and_unknown_names_closed() {
    let report = compile_project(
        &project(&[
            (
                "main",
                "import hidden;trait Factory{fn handler(self:&Self)->fn(i32)->i32{return hidden.identity}}fn main(){}",
            ),
            ("hidden", "fn identity(value:i32)->i32{return value}"),
        ]),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    // Qualified module values in default bodies still stop at the existing
    // symbolic Member boundary. This slice only admits resolved Name values.
    assert_eq!(
        report.diagnostics[0].code,
        "E-GENERIC-ARGUMENT",
        "{}",
        report.to_json()
    );
    assert!(
        report.diagnostics[0]
            .message
            .contains("unknown value hidden"),
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 0);
    let ordinary = compile_project(
        &project(&[
            (
                "main",
                "import hidden;fn main()->i32{let handler=hidden.identity;return handler(0)}",
            ),
            ("hidden", "fn identity(value:i32)->i32{return value}"),
        ]),
        &CompileOptions::default(),
    );
    assert_eq!(
        ordinary.status,
        ResultCategory::SafeReject,
        "{}",
        ordinary.to_json()
    );
    assert_eq!(
        ordinary.diagnostics[0].code,
        "E-MODULE-PRIVATE",
        "{}",
        ordinary.to_json()
    );
    reject(
        "trait Factory{fn handler(self:&Self)->fn(i32)->i32{return missing}}fn main(){}",
        "unknown value",
    );
    reject(
        "fn identity(value:i32)->i32{return value}fn ordinary<T:Copy>(value:T)->fn(i32)->i32{return identity}fn main(){}",
        "unknown value",
    );
}

#[test]
fn trait_owned_helper_cached_default_values_recheck_the_actual_global_signature() {
    use zil_driver::CompilationSession;
    let valid = "fn identity(value:i32)->i32{return 7}trait Factory{fn handler(self:&Self)->fn(i32)->i32{return identity}}struct P{}impl Factory for P{}fn main(){}";
    let invalid = valid.replace("value:i32", "value:i64");
    let mut session = CompilationSession::default();
    for (text, expected) in [
        (valid, ResultCategory::Success),
        (invalid.as_str(), ResultCategory::SafeReject),
        (valid, ResultCategory::Success),
    ] {
        let unit = source(text);
        for report in [
            session.compile_source(&unit, &CompileOptions::default()),
            CompilationSession::default().compile_source(&unit, &CompileOptions::default()),
        ] {
            assert_eq!(report.status, expected, "{}", report.to_json());
            if expected == ResultCategory::Success {
                assert_eq!(report.verification_runs, 1);
            } else {
                assert_eq!(report.phase, Phase::Expand);
                assert_eq!(report.diagnostics[0].code, "E-GENERIC-ARGUMENT");
                assert!(
                    report.diagnostics[0]
                        .message
                        .contains("before concrete instantiation")
                );
                assert_eq!(report.verification_runs, 0);
            }
        }
        assert_eq!(session.active_query_count(), 0);
    }
}

#[test]
fn trait_owned_helper_default_function_value_request_budgets_are_exact_across_cache_replay() {
    use zil_driver::CompilationSession;
    let unit = source(include_str!(
        "../../../tests/language-readiness/function-pointer-gaps/default-global-fnptr-known-incomplete.zil"
    ));
    for ledger in ["work", "nodes", "functions"] {
        let mut low = 1;
        let mut high = match ledger {
            "work" => Limits::default().max_analysis_steps,
            "nodes" => Limits::default().max_nodes,
            _ => Limits::default().max_functions,
        };
        let options = |n| {
            let mut options = CompileOptions::default();
            match ledger {
                "work" => options.limits.max_analysis_steps = n,
                "nodes" => options.limits.max_nodes = n,
                _ => options.limits.max_functions = n,
            }
            options
        };
        while low < high {
            let n = low + (high - low) / 2;
            let report = compile_source(&unit, &options(n));
            match report.status {
                ResultCategory::Success => high = n,
                ResultCategory::ResourceLimit => low = n + 1,
                _ => panic!("{}", report.to_json()),
            }
        }
        let mut session = CompilationSession::default();
        for (n, expected) in [
            (low, ResultCategory::Success),
            (low - 1, ResultCategory::ResourceLimit),
            (low, ResultCategory::Success),
        ] {
            for report in [
                session.compile_source(&unit, &options(n)),
                CompilationSession::default().compile_source(&unit, &options(n)),
            ] {
                assert_eq!(report.status, expected, "{}", report.to_json());
                if expected == ResultCategory::Success {
                    assert_eq!(report.verification_runs, 1);
                } else {
                    assert!(!report.diagnostics.is_empty());
                    assert!(
                        report
                            .diagnostics
                            .iter()
                            .all(|d| d.category == ResultCategory::ResourceLimit)
                    );
                }
            }
            assert_eq!(session.active_query_count(), 0);
        }
        println!(
            "336 default function value request {ledger} N={low}; N-1 ResourceLimit, cold/warm/fresh agree"
        );
    }
}

#[test]
fn trait_default_recursive_default_is_a_static_call_edge() {
    native_both(&[(
        "main",
        r#"
trait Down{fn down(self:&Self,n:i32)->i32{if n==0{return 0}return self.down(n-1)}}
struct P{}
impl Down for P{}
fn main()->i32{let p=P{};return p.down(4)}
"#,
    )]);
}

#[test]
fn trait_default_default_substitutes_trait_arguments_through_record_bodies_and_signatures() {
    native_both(&[(
        "main",
        r#"
struct Box<T:Copy>{value:T}
trait Wrap<R:Copy>{fn wrap(self:&Self,value:R)->Box<R>{return Box::<R>{value:value}}}
struct P{}
impl Wrap<i32> for P{}
fn wrapped<T:Wrap<i32>>(x:&T)->Box<i32>{return x.wrap(7)}
fn main()->i32{let p=P{};return wrapped(&p).value-7}
"#,
    )]);
}

#[test]
fn trait_default_direct_reference_inputs_and_required_borrow_delegation_are_native() {
    native_both(&[(
        "main",
        r#"
trait Read{
 fn raw(self:&Self)->&i32;
 fn same(self:&Self)->&Self{return self}
 fn delegated(self:&Self)->&i32{return self.raw()}
}
struct P{n:i32}
impl Read for P{fn raw()->&i32{return &.n}}
fn main()->i32{let p=P{n:7};let same=p.same();let raw=p.delegated();if same.n!=7{return 1}return *raw-7}
"#,
    )]);
}

#[test]
fn trait_default_missing_wrong_override_duplicate_and_ambiguous_targets_reject() {
    for (text, phase, code) in [
        (
            "trait Read{fn required(self:&Self)->i32;fn delegated(self:&Self)->i32{return self.required()}}struct P{}impl Read for P{}fn main(){}",
            Phase::Expand,
            "E-TRAIT-METHOD-SET",
        ),
        (
            "trait Read{fn read(self:&Self)->i32{return 7}}struct P{}impl Read for P{fn read()->bool{return true}}fn main(){}",
            Phase::Expand,
            "E-TRAIT-SIGNATURE",
        ),
        (
            "type Int=i32;trait Read<R>{fn read(self:&Self)->i32{return 7}}struct P{}impl Read<Int> for P{}impl Read<i32> for P{}fn main(){}",
            Phase::Expand,
            "E-TRAIT-DUPLICATE",
        ),
        (
            "trait Read<R>{fn read(self:&Self)->i32{return 7}}struct P{}impl Read<i32> for P{}impl Read<i64> for P{}fn main()->i32{let p=P{};return p.read()}",
            Phase::Expand,
            "E-GENERIC-ARGUMENT",
        ),
    ] {
        let report = check(text);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{text}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, phase, "{}", report.to_json());
        assert_eq!(report.verification_runs, 0);
        assert!(
            report.diagnostics.iter().any(|d| d.code == code),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn trait_default_default_request_budgets_are_exact_across_cache_replay() {
    use zil_driver::CompilationSession;
    let unit = source(
        "trait Tag<A:Copy,B:Copy>{fn base(self:&Self)->i32;fn delegated(self:&Self)->i32{return self.base()}}struct P{}impl Tag<i32,bool> for P{fn base()->i32{return 17}}impl Tag<bool,i32> for P{fn base()->i32{return 29}}fn a<T:Tag<i32,bool>>(x:&T)->i32{return x.delegated()}fn b<T:Tag<bool,i32>>(x:&T)->i32{return x.delegated()}fn main()->i32{let p=P{};return a(&p)+b(&p)-46}",
    );
    for ledger in ["work", "nodes", "functions"] {
        let mut low = 1;
        let mut high = match ledger {
            "work" => Limits::default().max_analysis_steps,
            "nodes" => Limits::default().max_nodes,
            _ => Limits::default().max_functions,
        };
        let options = |n| {
            let mut options = CompileOptions::default();
            match ledger {
                "work" => options.limits.max_analysis_steps = n,
                "nodes" => options.limits.max_nodes = n,
                _ => options.limits.max_functions = n,
            }
            options
        };
        while low < high {
            let n = low + (high - low) / 2;
            let report = compile_source(&unit, &options(n));
            match report.status {
                ResultCategory::Success => high = n,
                ResultCategory::ResourceLimit => low = n + 1,
                _ => panic!("{}", report.to_json()),
            }
        }
        let mut session = CompilationSession::default();
        for (n, expected) in [
            (low, ResultCategory::Success),
            (low - 1, ResultCategory::ResourceLimit),
            (low, ResultCategory::Success),
        ] {
            for report in [
                session.compile_source(&unit, &options(n)),
                CompilationSession::default().compile_source(&unit, &options(n)),
            ] {
                assert_eq!(report.status, expected, "{}", report.to_json());
                if expected == ResultCategory::ResourceLimit {
                    assert!(!report.diagnostics.is_empty());
                } else {
                    assert_eq!(report.verification_runs, 1);
                }
            }
            assert_eq!(session.active_query_count(), 0);
        }
        println!("330 default request {ledger} N={low}; N-1 ResourceLimit, cold/warm/fresh agree");
    }
}

#[test]
fn trait_default_unused_and_overridden_defaults_check_symbolic_permissions() {
    for (text, category, code) in [
        (
            "trait Read{fn read(self:&Self)->&i32{let x=7;return &x}}fn main(){}",
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        ),
        (
            "trait Read{fn raw(self:&Self)->&i32;fn leak(self:Self)->&i32{return self.raw()}}fn main(){}",
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        ),
        (
            "struct P{n:i32}impl P{fn borrow(self:&Self)->&i32{return &.n}}trait Read{fn read(self:&Self)->&i32{let p=P{n:7};return p.borrow()}}fn main(){}",
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        ),
        (
            "struct P{n:i32}impl P{fn borrow(self:&Self)->&i32{return &.n}}trait Read{fn read(self:&Self,p:&i32)->&i32{let p=P{n:7};return p.borrow()}}fn main(){}",
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        ),
        (
            "trait Read{fn read(self:&Self)->i32{let self=7;return 7}}fn main(){}",
            ResultCategory::SafeReject,
            "E-RECEIVER-RESERVED-NAME",
        ),
        (
            "trait Read{fn read(self:&Self)->i32{return self.secret}}fn main(){}",
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        ),
        (
            "trait Read<R:Copy>{fn read(self:&Self,x:R)->R{return x+1}}fn main(){}",
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        ),
        (
            "trait Read{fn read(self:&Self)->i32{return true}}fn main(){}",
            ResultCategory::SafeReject,
            "E-GENERIC-ARGUMENT",
        ),
        (
            "struct R{n:i32}fn concrete(x:R)->i32{return x.n}trait Read<R:Copy>{fn read(self:&Self,x:R)->i32{return concrete(x)}}fn main(){}",
            ResultCategory::SafeReject,
            "E-GENERIC-ARGUMENT",
        ),
        (
            "trait Read{fn read(self:&Self)->i32{return self.other()}}struct P{}impl Read for P{fn read()->i32{return 7}}fn main(){}",
            ResultCategory::SafeReject,
            "E-GENERIC-ARGUMENT",
        ),
        (
            "trait Read<R>{fn read(self:&Self,x:R)->R{let y=x;return x}}fn main(){}",
            ResultCategory::SafeReject,
            "E-GENERIC-ARGUMENT",
        ),
        (
            "struct Box<T:Copy>{value:T}trait Read<R>{fn read(self:&Self,x:R)->Box<R>{return Box::<R>{value:x}}}fn main(){}",
            ResultCategory::SafeReject,
            "E-GENERIC-ARGUMENT",
        ),
    ] {
        let report = check(text);
        assert_eq!(report.status, category, "{text}: {}", report.to_json());
        assert_eq!(report.phase, Phase::Expand, "{text}: {}", report.to_json());
        assert_eq!(report.verification_runs, 0);
        assert!(!report.diagnostics.is_empty());
        assert!(
            report.diagnostics.iter().all(|d| d.code == code),
            "{}",
            report.to_json()
        );
    }
    for text in [
        "trait Read<R>{fn read(self:&Self)->R;fn delegated(self:&Self)->R{return self.read()}}fn main(){}",
        "trait Again{fn again(self:&Self)->i32{return self.again()}}fn main(){}",
        "trait Read<R:Copy>{fn read(self:&Self,x:R)->R{let y=x;return x}}fn main(){}",
    ] {
        let report = check(text);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
}
#[test]
fn record_update_generic_trait_declaration_and_ordered_argument_identity() {
    native(&[
        (
            "main",
            "import left;import right;import data;fn main()->i32{let p=data.P{n:7};if left.apply(&p)!=7{return 1}if right.apply(&p)!=9{return 2}return 0}",
        ),
        (
            "left",
            "pub trait Read<R>{fn read(self:&Self)->R;}pub fn apply<T:Read<i32>>(x:&T)->i32{return x.read()}",
        ),
        (
            "right",
            "pub trait Read<R>{fn read(self:&Self)->R;}pub fn apply<T:Read<i32>>(x:&T)->i32{return x.read()}",
        ),
        (
            "data",
            "import left;import right;pub struct P{pub n:i32}impl left.Read<i32> for P{fn read()->i32{return .n}}impl right.Read<i32> for P{fn read()->i32{return .n+2}}",
        ),
    ]);
    native(&[(
        "main",
        r#"
trait Tag<A,B>{fn value(self:&Self)->i32;}
struct P{}
impl Tag<i32,bool> for P{fn value()->i32{return 17}}
impl Tag<bool,i32> for P{fn value()->i32{return 29}}
fn first<T:Tag<i32,bool>>(x:&T)->i32{return x.value()}
fn second<T>(x:&T)->i32 where T:Tag<bool,i32>{return x.value()}
fn main()->i32{let p=P{};if first(&p)!=17{return 1}if second(&p)!=29{return 2}return 0}
"#,
    )]);
}

#[test]
fn record_update_unused_trait_obligations_and_alias_coherence() {
    let declaration = "struct Box<T:Copy>{value:T}struct Wrap<T:Copy>{value:T}trait Read<R>{fn read(self:&Self)->Wrap<Box<R>>;}fn main(){}";
    let bad = check(declaration);
    assert_eq!(bad.status, ResultCategory::SafeReject, "{}", bad.to_json());
    assert_eq!(bad.phase, Phase::Expand, "{}", bad.to_json());
    assert!(
        bad.diagnostics.iter().any(|d| d.message.contains("Copy")),
        "{}",
        bad.to_json()
    );
    let good = check(&declaration.replace("trait Read<R>", "trait Read<R:Copy>"));
    assert_eq!(good.status, ResultCategory::Success, "{}", good.to_json());
    let duplicate = check(
        "type Int=i32;trait Flag<A,B>{}struct P{}impl Flag<Int,bool> for P{}impl Flag<i32,bool> for P{}fn main(){}",
    );
    assert_eq!(
        duplicate.status,
        ResultCategory::SafeReject,
        "{}",
        duplicate.to_json()
    );
    assert_eq!(duplicate.phase, Phase::Expand, "{}", duplicate.to_json());
    assert!(
        duplicate
            .diagnostics
            .iter()
            .any(|d| d.code == "E-TRAIT-DUPLICATE"),
        "{}",
        duplicate.to_json()
    );
    let missing = check(
        "trait Flag<A,B>{}struct P{}impl Flag<i32,bool> for P{}fn needed<T:Flag<bool,i32>>(x:&T){}fn main(){let p=P{};needed(&p)}",
    );
    assert_eq!(
        missing.status,
        ResultCategory::SafeReject,
        "{}",
        missing.to_json()
    );
    assert_eq!(missing.phase, Phase::Expand, "{}", missing.to_json());
    assert!(
        missing
            .diagnostics
            .iter()
            .any(|d| d.message.contains("does not satisfy nominal trait bound")),
        "{}",
        missing.to_json()
    );
}

#[test]
fn concrete_generic_trait_arguments_and_bounds_call_real_instances() {
    native(&[(
        "main",
        r#"
trait Read<R>{fn read(self:&Self)->R;}
struct Point{n:i32}
impl Read<i32> for Point{fn read()->i32{return .n}}
impl Read<i64> for Point{fn read()->i64{return i64(.n)+1}}
fn read32<T:Read<i32>>(x:&T)->i32{return x.read()}
fn read64<T>(x:&T)->i64 where T:Read<i64>{return x.read()}
fn main()->i32{let p=Point{n:7};if read32(&p)!=7{return 1}if read64(&p)!=8{return 2}return 0}
"#,
    )]);
}

#[test]
fn original_concrete_generic_trait_probe_calls_directly() {
    native(&[(
        "main",
        include_str!("../../../tests/language-readiness/recovery/pending/trait-type-argument.zil"),
    )]);
}

#[test]
fn generic_trait_cross_module_bound_calls_keep_identity_and_exports() {
    native(&[
        (
            "main",
            "import api;import data;fn main()->i32{let p=data.Point{n:7};return api.get(&p)-7}",
        ),
        (
            "api",
            "pub trait Read<R>{fn read(self:&Self)->R;}pub fn get<T:Read<i32>>(x:&T)->i32{return x.read()}",
        ),
        (
            "data",
            "import api;pub struct Point{pub n:i32}impl api.Read<i32> for Point{fn read()->i32{return .n}}",
        ),
    ]);
}

#[test]
fn generic_trait_same_layout_nominals_and_phantom_arguments_stay_distinct() {
    native(&[(
        "main",
        r#"
struct A{n:i32}struct B{n:i32}
trait Build<R,P>{fn build(self:&Self)->R;}
struct Point{n:i32}
impl Build<A,bool> for Point{fn build()->A{return A{n:.n}}}
impl Build<B,bool> for Point{fn build()->B{return B{n:.n+1}}}
impl Build<A,i32> for Point{fn build()->A{return A{n:.n+2}}}
fn a<T:Build<A,bool>>(x:&T)->A{return x.build()}
fn b<T:Build<B,bool>>(x:&T)->B{return x.build()}
fn c<T:Build<A,i32>>(x:&T)->A{return x.build()}
fn main()->i32{let p=Point{n:7};if a(&p).n!=7{return 1}if b(&p).n!=8{return 2}return c(&p).n-9}
"#,
    )]);
}

#[test]
fn generic_trait_record_arguments_normalize_required_signatures_from_declaration() {
    native(&[(
        "main",
        r#"
struct Box<T:Copy>{value:T}
trait Read<R:Copy>{fn read(self:&Self)->R;}
struct Point{n:i32}
impl Read<Box<i32>> for Point{fn read()->Box<i32>{return Box::<i32>{value:.n}}}
fn get<T:Read<Box<i32>>>(x:&T)->Box<i32>{return x.read()}
fn id<T:Copy>(value:T)->T{return value}
fn main()->i32{let p=Point{n:7};let a=get(&p);let b=id(p.read());return a.value+b.value-14}
"#,
    )]);
    for (actual, value) in [("B", "B{n:1}"), ("i64", "1")] {
        let text = format!(
            "struct A{{n:i32}}struct B{{n:i32}}struct Box<T:Copy>{{value:T}}trait Read<R>{{fn read(self:&Self)->R;}}struct P{{}}impl Read<Box<A>> for P{{fn read()->Box<{actual}>{{return Box::<{actual}>{{value:{value}}}}}}}fn main(){{}}"
        );
        let report = check(&text);
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
                .any(|d| d.code == "E-TRAIT-SIGNATURE"),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn generic_trait_direct_static_and_value_call_arguments_keep_context() {
    native(&[(
        "main",
        r#"
trait Add<R>{fn add(self:&Self,value:R)->R;}
struct Point{n:i64}
impl Add<i64> for Point{fn add(value:i64)->i64{return .n+value}}
fn id<T:Copy>(value:T)->T{return value}
fn main()->i32{let p=Point{n:1};let a=p.add(id(6));let b=Point.add(&p,id(6));if a!=7{return 1}if b!=7{return 2}return 0}
"#,
    )]);
}

#[test]
fn generic_trait_direct_ambiguity_does_not_reject_distinct_impl_headers() {
    let declarations = "trait Read<R>{fn read(self:&Self)->R;}struct P{n:i32}impl Read<i32> for P{fn read()->i32{return .n}}impl Read<i64> for P{fn read()->i64{return i64(.n)}}";
    let good = check(&format!("{declarations}fn main()->i32{{return 0}}"));
    assert_eq!(good.status, ResultCategory::Success, "{}", good.to_json());
    let bad = check(&format!(
        "{declarations}fn main()->i32{{let p=P{{n:7}};return p.read()}}"
    ));
    assert_eq!(bad.status, ResultCategory::SafeReject, "{}", bad.to_json());
    assert!(
        bad.diagnostics
            .iter()
            .any(|d| d.message.contains("ambiguous")),
        "{}",
        bad.to_json()
    );
    let text = "trait Read<R>{fn read(self:&Self)->R;}struct P{n:i32}impl Read<i32> for P{fn read()->i32{return .n}}impl P{fn read(self:&Self)->i32{return .n}}fn get<T:Read<i32>>(x:&T)->i32{return x.read()}fn main()->i32{let p=P{n:7};return get(&p)-7}";
    assert_eq!(check(text).status, ResultCategory::Success);
    let bad = check(&text.replace("get(&p)-7", "p.read()-7"));
    assert_eq!(bad.status, ResultCategory::SafeReject, "{}", bad.to_json());
    assert!(
        bad.diagnostics
            .iter()
            .any(|d| d.message.contains("ambiguous"))
    );
}

#[test]
fn generic_trait_request_budgets_are_exact_across_cache_replay() {
    use zil_driver::CompilationSession;
    let unit = source(
        "trait Read<R>{fn read(self:&Self)->R;}struct P{n:i32}impl Read<i32> for P{fn read()->i32{return .n}}impl Read<i64> for P{fn read()->i64{return i64(.n)}}fn a<T:Read<i32>>(x:&T)->i32{return x.read()}fn b<T:Read<i64>>(x:&T)->i64{return x.read()}fn main()->i32{let p=P{n:7};let x=a(&p);let y=b(&p);return x-7}",
    );
    for ledger in ["work", "nodes", "instances"] {
        let mut low = 1;
        let mut high = match ledger {
            "work" => Limits::default().max_analysis_steps,
            "nodes" => Limits::default().max_nodes,
            _ => Limits::default().max_functions,
        };
        let options = |n| {
            let mut options = CompileOptions::default();
            match ledger {
                "work" => options.limits.max_analysis_steps = n,
                "nodes" => options.limits.max_nodes = n,
                _ => options.limits.max_functions = n,
            };
            options
        };
        while low < high {
            let n = low + (high - low) / 2;
            let report = compile_source(&unit, &options(n));
            match report.status {
                ResultCategory::Success => high = n,
                ResultCategory::ResourceLimit => low = n + 1,
                _ => panic!("{}", report.to_json()),
            }
        }
        let mut session = CompilationSession::default();
        for (n, expected) in [
            (low, ResultCategory::Success),
            (low - 1, ResultCategory::ResourceLimit),
            (low, ResultCategory::Success),
        ] {
            assert_eq!(session.compile_source(&unit, &options(n)).status, expected);
            assert_eq!(
                CompilationSession::default()
                    .compile_source(&unit, &options(n))
                    .status,
                expected
            );
            assert_eq!(session.active_query_count(), 0);
        }
        println!("323 trait request {ledger} N={low}; N-1 ResourceLimit, cold/warm/fresh agree");
    }
}

#[test]
fn generic_trait_unused_signatures_and_instantiated_conformance_reject() {
    for source in [
        "trait Read<R>{fn read(self:&Self)->Missing;}fn main(){}",
        "struct Box<T:Copy>{value:T}trait Read<R>{fn read(self:&Self)->Box<R>;}fn main(){}",
        "trait Read<R>{fn read(self:&Self)->R;}struct P{n:i32}impl Read<i32> for P{fn read()->i64{return 7}}fn main(){}",
        "trait Read<R>{fn read(self:&Self)->R;}struct P{n:i32}impl Read<i32> for P{}fn main(){}",
        "trait Read<R>{fn read(self:&Self)->R;}fn unused<T:Read<i32>>(x:&T)->i64{return x.read()}fn main(){}",
        "trait Flag<R>{}struct P{}impl Flag<i32> for P{}fn needed<T:Flag<i64>>(x:&T){}fn main(){let p=P{};needed(&p)}",
        "trait Read<R>{fn read(self:&Self)->R;}struct P{n:i32}impl Read<i32> for P{fn read()->i32{return .n}}impl Read<i32> for P{fn read()->i32{return .n}}fn main(){}",
        "type Int=i32;trait Flag<R>{}struct P{}impl Flag<Int> for P{}impl Flag<i32> for P{}fn main(){}",
    ] {
        let report = check(source);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{source}: {}",
            report.to_json()
        );
    }
    let good = check(
        "struct Box<T:Copy>{value:T}trait Read<R:Copy>{fn read(self:&Self)->Box<R>;}fn main(){}",
    );
    assert_eq!(good.status, ResultCategory::Success, "{}", good.to_json());
}

#[test]
fn trait_argument_lexical_binders_and_structural_capabilities_are_separate() {
    for source in [
        "trait Flag<R>{}struct U{}fn unused<U:Copy,T:Flag<U>>(x:&T)->U{return 0}fn main(){}",
        "trait Flag<R>{}struct P{}impl Flag<&i32> for P{}fn main(){}",
        "trait Flag<R>{}resource R{n:i32}struct P{}impl Flag<R> for P{}fn main(){}",
        "trait Transfer<R>{}struct P{}impl Transfer<i32> for P{}fn main(){}",
        "trait Copy<R>{}struct P{}impl Copy<i32> for P{}fn main(){}",
        "trait Share<R>{}struct P{}impl Share<i32> for P{}fn main(){}",
    ] {
        let report = check(source);
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{source}: {}",
            report.to_json()
        );
    }
    native(&[(
        "main",
        "type R=i64;trait Read<R>{fn read(self:&Self)->R;}struct P{n:i32}impl Read<i32> for P{fn read()->i32{return .n}}fn get<T:Read<i32>>(x:&T)->i32{return x.read()}fn main()->i32{let p=P{n:7};return get(&p)-7}",
    )]);
}
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "main.zil", text)
}
fn check(text: &str) -> CompileReport {
    compile_source(&source(text), &CompileOptions::default())
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
fn native(files: &[(&str, &str)]) {
    native_backend(files, zil_driver::NativeBackend::default());
}
fn native_verified(verified: &zil_verify::VerifiedModule) {
    let lir = zil_lower::lower(verified).unwrap();
    let mut backends = vec![zil_driver::NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(zil_driver::NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "341-shared-span-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            let mut options = zil_backend_llvm::BuildOptions::new(dir.join("program"));
            options.backend = backend;
            options.optimization = optimization;
            let artifact = zil_backend_llvm::build(verified, &lir, &options).unwrap();
            assert_eq!(artifact.codegen.backend, backend);
            assert!(artifact.codegen.fallback.is_none());
            let output = Command::new(&artifact.executable).output().unwrap();
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert!(output.stdout.is_empty());
            assert!(output.stderr.is_empty());
            fs::remove_dir_all(dir).unwrap();
        }
    }
}
fn native_both(files: &[(&str, &str)]) {
    native_backend(files, zil_driver::NativeBackend::Clang);
    if cfg!(feature = "llvm-api") {
        native_backend(files, zil_driver::NativeBackend::LlvmApi);
    }
}
fn native_backend(files: &[(&str, &str)], backend: zil_driver::NativeBackend) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let dir = root.join(format!(
            "bounds162-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        for (name, text) in files {
            fs::write(dir.join(format!("{name}.zil")), text).unwrap();
        }
        let executable = dir.join("program");
        let options = CompileOptions {
            backend,
            stop_after: Stage::Build,
            optimization,
            output: Some(executable.clone()),
            emit_dir: Some(dir.join("phases")),
            ..CompileOptions::default()
        };
        let report = if files.len() == 1 {
            compile_source(&source(files[0].1), &options)
        } else {
            compile_project(&project(files), &options)
        };
        fs::write(dir.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let codegen = report.native_codegen.as_ref().unwrap();
        assert_eq!(codegen.backend, backend);
        assert!(codegen.fallback.is_none());
        if backend == zil_driver::NativeBackend::LlvmApi {
            assert!(!codegen.object_reports.is_empty());
            assert!(
                codegen
                    .object_reports
                    .iter()
                    .all(|object| object.contains("\"input_kind\":\"builder\""))
            );
        }
        let output = Command::new(&executable).output().unwrap();
        fs::write(dir.join("stdout"), &output.stdout).unwrap();
        fs::write(dir.join("stderr"), &output.stderr).unwrap();
        assert_eq!(output.status.code(), Some(0), "{output:?}");
    }
}
fn reject(text: &str, fragment: &str) {
    let report = check(text);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains(fragment)),
        "{}",
        report.to_json()
    );
}

#[test]
fn shared_bounds_dispatch_for_multiple_nominal_types_and_forward_symbolically() {
    native(&[(
        "main",
        r#"
trait Measure { fn measure(self:&Self)->i32; }
struct Point { value:i32 }
struct Area { width:i32,height:i32 }
impl Measure for Point { fn measure()->i32=>.value }
impl Measure for Area { fn measure()->i32=>.width*.height }
fn measured<T:Measure>(value:&T)->i32=>value.measure()
fn relay<U:Measure>(value:&U)->i32=>measured(value)
fn main()->i32 { let p=Point{value:7};let a=Area{width:5,height:7};return relay(&p)+measured::<Area>(&a)-42 }
"#,
    )]);
}

#[test]
fn mutable_multiple_bounds_and_value_receiver_keep_passing_modes() {
    native(&[(
        "main",
        r#"
trait Read { fn get(self:&Self)->i32; }
trait Change { fn set(self:&mut Self,n:i32); }
trait Consume { fn finish(self:Self)->i32; }
struct Counter { n:i32 }
impl Read for Counter { fn get()->i32=>.n }
impl Change for Counter { fn set(n:i32){.n=n} }
impl Consume for Counter { fn finish()->i32=>.n }
fn update<T:Read+Change>(value:&mut T,n:i32)->i32{value.set(n);return value.get()}
fn local<T:Read+Change>(value:T,n:i32)->i32{var x=value;x.set(n);return x.get()}
fn finish<T:Consume>(value:T)->i32=>value.finish()
fn main()->i32 {var c=Counter{n:1};if update(&mut c,20)!=20{return 1};if local(c,40)!=40{return 2};return finish(Counter{n:42})-42}
"#,
    )]);
}

#[test]
fn bound_method_named_len_and_self_parameters_are_symbolic() {
    native(&[(
        "main",
        r#"
trait Measure {fn len(self:&Self)->i32;fn plus(self:&Self,other:&Self)->i32;}
struct Number {n:i32}
impl Measure for Number {fn len()->i32=>.n;fn plus(other:&Self)->i32=>.n+other.n}
fn combine<T:Measure>(a:&T,b:&T)->i32=>a.len()+a.plus(b)
fn main()->i32{let a=Number{n:7};let b=Number{n:28};return combine(&a,&b)-42}
"#,
    )]);
}

#[test]
fn imported_public_bounds_keep_definition_and_nominal_module_identity() {
    native(&[
        (
            "main",
            "import left;import right;import helpers;fn main()->i32{let a=left.make(7);let b=right.make(35);return helpers.apply(&a)+right.apply(&b)-42}",
        ),
        (
            "left",
            "pub trait Measure{fn measure(self:&Self)->i32;}pub struct Value{n:i32}impl Measure for Value{fn measure()->i32=>.n}pub fn make(n:i32)->Value=>Value{n:n}",
        ),
        (
            "right",
            "pub trait Measure{fn measure(self:&Self)->i32;}pub struct Value{n:i32}impl Measure for Value{fn measure()->i32=>.n}pub fn make(n:i32)->Value=>Value{n:n};pub fn apply<T:Measure>(x:&T)->i32=>x.measure()",
        ),
        (
            "helpers",
            "import left;pub fn apply<T:left.Measure>(x:&T)->i32=>x.measure()",
        ),
    ]);
}

#[test]
fn same_named_inherent_method_does_not_satisfy_nominal_bound() {
    reject(
        "trait Read{fn get(self:&Self)->i32;}struct C{n:i32}impl C{fn get(self:&Self)->i32=>.n}fn read<T:Read>(x:&T)->i32=>x.get();fn main()->i32{let c=C{n:7};return read(&c)}",
        "does not satisfy nominal trait bound",
    );
}

#[test]
fn unused_definitions_cannot_gain_members_types_or_copy_from_instances() {
    for (body, reason) in [
        (
            "fn bad<T:Read>(x:&T)->i32=>x.missing()",
            "not provided by the declared trait bounds",
        ),
        (
            "fn bad<T:Read>(x:&T)->bool=>x.get()",
            "generic body types must agree",
        ),
        (
            "fn bad<T:Read>(x:&T)->i32=>x.get(true)",
            "argument count differs",
        ),
        ("fn bad<T:Read>(x:T)->(T,T)=>(x,x)", "used after move"),
        ("fn bad<T:Read>(x:T)->T=>x+1", "requires a declared bound"),
    ] {
        let text = format!("trait Read{{fn get(self:&Self)->i32;}}{body};fn main(){{}}");
        let report = check(&text);
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.message.contains(reason)),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn forwarding_requires_a_prior_bound_even_when_unused() {
    reject(
        "trait Read{fn get(self:&Self)->i32;}fn read<T:Read>(x:&T)->i32=>x.get();fn bad<U>(x:&U)->i32=>read(x);fn main(){}",
        "does not satisfy nominal trait bound",
    );
    reject(
        "trait Read{fn get(self:&Self)->i32;}struct T{n:i32}impl Read for T{fn get()->i32=>.n}fn read<U:Read>(x:&U)->i32=>x.get();fn bad<T>(x:&T)->i32=>read(x);fn main(){}",
        "does not satisfy nominal trait bound",
    );
}

#[test]
fn missing_trait_wrong_arguments_and_ambiguous_bounds_reject_before_specialization() {
    reject(
        "fn bad<T:Missing>(x:T)->T=>x;fn main(){}",
        "not a declared nominal trait",
    );
    reject(
        "trait Read{fn get(self:&Self,n:i32)->i32;}fn bad<T:Read>(x:&T)->i32=>x.get(true);fn main(){}",
        "generic body types must agree",
    );
    reject(
        "trait A{fn get(self:&Self)->i32;}trait B{fn get(self:&Self)->i32;}fn bad<T:A+B>(x:&T)->i32=>x.get();fn main(){}",
        "ambiguous between declared trait bounds",
    );
}

#[test]
fn borrowed_receivers_do_not_gain_mutation_or_move_permissions() {
    reject(
        "trait Change{fn set(self:&mut Self,n:i32);}fn bad<T:Change>(x:&T){x.set(1)}fn main(){}",
        "shared reference cannot grant exclusive access",
    );
    reject(
        "trait Change{fn set(self:&mut Self,n:i32);}fn bad<T:Change>(x:T){x.set(1)}fn main(){}",
        "requires a writable place",
    );
    reject(
        "trait Consume{fn take(self:Self)->i32;}fn bad<T:Consume>(x:&T)->i32=>x.take();fn main(){}",
        "cannot move an unknown non-Copy trait receiver",
    );
    reject(
        "trait Consume{fn take(self:Self)->i32;}fn bad<T:Consume>(x:T)->i32{x.take();return x.take()}fn main(){}",
        "used after move",
    );
}

#[test]
fn actual_impl_signatures_and_independent_loan_checks_still_run() {
    let report = check(
        "trait Read{fn get(self:&Self)->i32;}struct C{n:i32}impl Read for C{fn get()->bool=>true}fn read<T:Read>(x:&T)->i32=>x.get();fn main()->i32{let c=C{n:7};return read(&c)}",
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
            .any(|d| d.code == "E-TRAIT-SIGNATURE"),
        "{}",
        report.to_json()
    );
    let report = check(
        "trait Change{fn set(self:&mut Self,n:i32);}struct C{n:i32}impl Change for C{fn set(n:i32){.n=n}}fn update<T:Change>(x:&mut T){x.set(1)}fn main()->i32{var c=C{n:7};let old=&c;update(&mut c);return old.n}",
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
            .any(|d| d.code.starts_with("E-BORROW") || d.code.starts_with("E-ADDRESS")),
        "{}",
        report.to_json()
    );
}

#[test]
fn private_and_same_spelling_foreign_traits_cannot_satisfy_bound() {
    let files = [
        (
            "main",
            "import model;fn read<T:model.Secret>(x:&T)->i32=>x.get();fn main(){}",
        ),
        ("model", "trait Secret{fn get(self:&Self)->i32;}"),
    ];
    let r = compile_project(&project(&files), &CompileOptions::default());
    assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
    assert!(
        r.diagnostics.iter().any(|d| d.code.contains("PRIVATE")),
        "{}",
        r.to_json()
    );
    let files = [
        (
            "main",
            "import left;import right;fn main()->i32{let c=right.make();return left.read(&c)}",
        ),
        (
            "left",
            "pub trait Read{fn get(self:&Self)->i32;}pub fn read<T:Read>(x:&T)->i32=>x.get()",
        ),
        (
            "right",
            "pub trait Read{fn get(self:&Self)->i32;}pub struct C{n:i32}impl Read for C{fn get()->i32=>.n}pub fn make()->C=>C{n:7}",
        ),
    ];
    let r = compile_project(&project(&files), &CompileOptions::default());
    assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.message.contains("does not satisfy nominal trait bound")),
        "{}",
        r.to_json()
    );
}

#[test]
fn trait_bound_scans_obey_actual_work_budget() {
    let source = source(
        "trait Read{fn get(self:&Self)->i32;}struct C{n:i32}impl Read for C{fn get()->i32=>.n}fn read<T:Read>(x:&T)->i32=>x.get();fn main()->i32{let c=C{n:7};return read(&c)}",
    );
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    assert!(zil_expand::expand_with_map_limits(&source, &ast, &Limits::default()).is_ok());
    let mut low = 0usize;
    let mut high = Limits::default().max_analysis_steps;
    while low + 1 < high {
        let middle = low + (high - low) / 2;
        let limits = Limits {
            max_analysis_steps: middle,
            ..Limits::default()
        };
        match zil_expand::expand_with_map_limits(&source, &ast, &limits) {
            Ok(_) => high = middle,
            Err(errors) => {
                assert_eq!(
                    errors[0].category,
                    ResultCategory::ResourceLimit,
                    "{errors:?}"
                );
                low = middle;
            }
        }
    }
    assert!(
        zil_expand::expand_with_map_limits(
            &source,
            &ast,
            &Limits {
                max_analysis_steps: high,
                ..Limits::default()
            }
        )
        .is_ok()
    );
    let errors = zil_expand::expand_with_map_limits(
        &source,
        &ast,
        &Limits {
            max_analysis_steps: high - 1,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
}

#[path = "../../../tests/language-readiness/borrowed-interface-flow/stdlib.rs"]
mod trait_shared_lending_stdlib;
