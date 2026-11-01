//! Real source -> current Verify -> direct Builder/cache -> native execution.
#![cfg(all(feature = "llvm-api", target_os = "linux"))]
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, CompileReport, NativeBackend, Optimization,
    Stage, compile_path,
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "zil-obj212-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn write(&self, name: &str, text: &str) {
        fs::write(self.0.join(name), text).unwrap();
    }
    fn source(&self) {
        self.write(
            "main.zil",
            "import a\nimport b\nfn main()->i32 { return a.get()+b.get(); }",
        );
        self.write(
            "a.zil",
            "fn hidden()->i32 { return 19; }\npub fn get()->i32 { return hidden(); }",
        );
        self.write("b.zil", "pub fn get()->i32 { return 23; }");
    }
    fn options(&self, units: usize, opt: Optimization) -> CompileOptions {
        CompileOptions {
            backend: NativeBackend::LlvmApi,
            stop_after: Stage::Build,
            output: Some(self.0.join("program")),
            object_cache: Some(self.0.join("objects")),
            optimization: opt,
            codegen: CodegenOptions { units, jobs: 4 },
            ..CompileOptions::default()
        }
    }
    fn build(&self, options: &CompileOptions) -> CompileReport {
        compile_path(&self.0.join("main.zil"), options)
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn success(r: &CompileReport, expected: i32) {
    assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
    assert_eq!(r.verification_runs, 1, "current Verify must execute");
    let patches = r.native_codegen.as_ref().map_or(0, |codegen| {
        codegen
            .object_reports
            .iter()
            .filter(|report| report.contains("\"input_kind\":\"object-patch\""))
            .count()
    });
    assert_eq!(
        r.commands.len(),
        1 + 2 * patches,
        "linking must execute on cache hits too"
    );
    let out = Command::new(&r.artifacts["executable"])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(expected), "{out:?}");
}
fn entries(root: &Path) -> Vec<PathBuf> {
    let mut files = vec![];
    if let Ok(dir) = fs::read_dir(root) {
        for p in dir {
            let p = p.unwrap().path();
            if p.is_dir() {
                files.extend(entries(&p));
            } else if p.extension().is_some_and(|e| e == "zobj") {
                files.push(p);
            }
        }
    }
    files.sort();
    files
}
#[test]
fn parallel_lowering_keeps_source_order_text_cache_keys_and_owned_cleanup() {
    let d = Dir::new();
    let mut source = String::from(
        "extern \"C\" fn putchar(n:i32)->i32;\nresource Token{code:i32}\nimpl Drop for Token{fn drop(self:Self){unsafe{putchar(.code)}}}\nfn make(n:i32)->Token contract{result(create)}{return Token{code:n}}\nfn early()->i32{let t=make(65);let f=move()=>t.code;return f()-65;}\n",
    );
    // Cross several lowering batches, including a short final batch. Each
    // parameter-dependent helper has at least nine SSA values/instructions,
    // one parameter/local and three blocks: >=23 estimated work units. Thus
    // 401 helpers contribute >=9223 total and >=7751 outside any 64-function
    // batch, forcing two lowering workers under the 4096-per-worker gate.
    // Branching also prevents the compact scalar-leaf fast path.
    for i in 0..401 {
        source += &format!("fn padding{i}(x:i32)->i32{{if x<0{{return x-1;}}return x+1;}}\n");
    }
    source += "fn late()->i32{let t=make(66);let f=move()=>t.code;let p:fn(i32)->i32=padding0;return f()+p(0)-67;}\nfn main()->i32{return early()+late();}\n";
    d.write("main.zil", &source);
    for optimization in [Optimization::O0, Optimization::O2] {
        let mut options = d.options(8, optimization);
        options.incremental_o2 = optimization == Optimization::O2;
        options.codegen.jobs = 1;
        let serial = d.build(&options);
        success(&serial, 0);
        let text = fs::read(&serial.artifacts["llvm"]).unwrap();
        let binary = fs::read(&serial.artifacts["executable"]).unwrap();
        options.codegen.jobs = 2;
        let parallel = d.build(&options);
        success(&parallel, 0);
        let codegen = parallel.native_codegen.as_ref().unwrap();
        assert_eq!(codegen.object_cache.hits, codegen.actual_units);
        assert_eq!(codegen.object_cache.misses, 0);
        assert_eq!(fs::read(&parallel.artifacts["llvm"]).unwrap(), text);
        assert_eq!(fs::read(&parallel.artifacts["executable"]).unwrap(), binary);
        let output = Command::new(&parallel.artifacts["executable"])
            .output()
            .unwrap();
        assert_eq!(output.stdout, b"AB");
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn unused_foreign_declarations_do_not_invalidate_independent_objects() {
    for optimization in [Optimization::O0, Optimization::O2] {
        let d = Dir::new();
        d.source();
        let o = d.options(8, optimization);
        assert!(!o.incremental_o2);
        let first = d.build(&o);
        success(&first, 42);
        // Ordinary O2 retains the original machine calls: the opt-in scalar
        // evaluator must not consume hidden()/b.get() before partitioning.
        let machine = fs::read_to_string(&first.artifacts["llvm"]).unwrap();
        assert!(machine.matches("call i32 @zil.fn.").count() >= 3);
        let units = first.native_codegen.as_ref().unwrap().actual_units;
        assert!(units > 1);
        let executable = fs::read(&first.artifacts["executable"]).unwrap();
        // Append to the final module so existing definition IDs do not move.
        for signature in ["x:i32)->i32", "x:i64)->i64"] {
            d.write("b.zil", &format!(
                "pub fn get()->i32 {{ return 23; }}\nextern \"C\" fn unused_foreign({signature};"
            ));
            let changed = d.build(&o);
            success(&changed, 42);
            let n = changed.native_codegen.as_ref().unwrap();
            assert_eq!(n.actual_units, units);
            assert_eq!(n.object_cache.hits, units, "{}", changed.to_json());
            assert_eq!(n.object_cache.misses, 0);
            assert_eq!(
                fs::read(&changed.artifacts["executable"]).unwrap(),
                executable
            );
        }
        // A newly consumed foreign symbol must actually enter code generation.
        d.write(
            "b.zil",
            "pub fn get()->i32 { unsafe { return abs(-24); } }\nextern \"C\" fn abs(x:i32)->i32;",
        );
        let used = d.build(&o);
        success(&used, 43);
        let n = used.native_codegen.as_ref().unwrap();
        assert_eq!(n.object_cache.misses, 1, "{}", used.to_json());
        assert_eq!(n.object_cache.hits, units - 1);
        // Signature/argument mismatch must be rejected before any cache reuse.
        d.write(
            "b.zil",
            "pub fn get()->i32 { unsafe { return abs(1,2); } }\nextern \"C\" fn abs(x:i32)->i32;",
        );
        let bad = d.build(&o);
        assert_eq!(bad.status, ResultCategory::SafeReject);
        assert_eq!(bad.phase, Phase::Types);
        assert_eq!(bad.diagnostics[0].code, "E-CALL-ARITY");
        assert!(bad.native_codegen.is_none());
    }
}
#[test]
fn cold_warm_private_public_edits_preserve_current_verify_and_output() {
    let d = Dir::new();
    d.source();
    let o = d.options(8, Optimization::O0);
    let a = d.build(&o);
    success(&a, 42);
    let summary = &a.native_codegen.as_ref().unwrap().object_cache;
    assert!(summary.enabled, "{}", a.to_json());
    assert_eq!(summary.hits, 0);
    assert_eq!(summary.stored, summary.misses);
    let executable = fs::read(&a.artifacts["executable"]).unwrap();
    let b = d.build(&o);
    success(&b, 42);
    let n = b.native_codegen.as_ref().unwrap();
    assert_eq!(n.object_cache.hits, n.actual_units);
    assert_eq!(n.object_cache.misses, 0);
    assert!(
        n.object_reports
            .iter()
            .all(|s| s.contains("\"input_kind\":\"object-cache\"")
                && s.contains("\"llvm_verification_this_request\":false"))
    );
    assert_eq!(executable, fs::read(&b.artifacts["executable"]).unwrap());
    d.write(
        "a.zil",
        "fn hidden()->i32 { return 20; }\npub fn get()->i32 { return hidden(); }",
    );
    let c = d.build(&o);
    success(&c, 43);
    let n = &c.native_codegen.as_ref().unwrap().object_cache;
    assert!(n.hits > 0 && n.misses > 0, "{}", c.to_json());
    d.write(
        "a.zil",
        "fn hidden()->i32 { return 20; }\npub fn get(x:i32)->i32 { return hidden()+x; }",
    );
    d.write(
        "main.zil",
        "import a\nimport b\nfn main()->i32 { return a.get(1)+b.get(); }",
    );
    let e = d.build(&o);
    success(&e, 44);
    let n = &e.native_codegen.as_ref().unwrap().object_cache;
    assert!(n.hits > 0 && n.misses > 0, "{}", e.to_json());
}
#[test]
fn optimization_and_units_invalidate_but_job_count_does_not() {
    let d = Dir::new();
    d.source();
    let mut o = d.options(8, Optimization::O0);
    success(&d.build(&o), 42);
    o.codegen.jobs = 1;
    let warm = d.build(&o);
    success(&warm, 42);
    assert_eq!(warm.native_codegen.as_ref().unwrap().object_cache.misses, 0);
    o.optimization = Optimization::O2;
    let changed = d.build(&o);
    success(&changed, 42);
    assert_eq!(
        changed.native_codegen.as_ref().unwrap().object_cache.hits,
        0
    );
    let warm = d.build(&o);
    success(&warm, 42);
    assert_eq!(warm.native_codegen.as_ref().unwrap().object_cache.misses, 0);
    o.codegen.units = 1;
    let single = d.build(&o);
    success(&single, 42);
    assert_eq!(
        single.native_codegen.as_ref().unwrap().object_cache.misses,
        1
    );
    success(&d.build(&o), 42);
}
#[test]
fn corrupt_and_truncated_entries_rebuild_without_changed_native_bytes() {
    let d = Dir::new();
    d.source();
    let o = d.options(8, Optimization::O0);
    let first = d.build(&o);
    success(&first, 42);
    let executable = fs::read(&first.artifacts["executable"]).unwrap();
    let files = entries(o.object_cache.as_ref().unwrap());
    assert!(!files.is_empty());
    for (i, p) in files.iter().enumerate() {
        if i % 2 == 0 {
            fs::write(p, b"truncated").unwrap();
        } else {
            let mut bytes = fs::read(p).unwrap();
            *bytes.last_mut().unwrap() ^= 1;
            fs::write(p, bytes).unwrap();
        }
    }
    let rebuilt = d.build(&o);
    success(&rebuilt, 42);
    let n = rebuilt.native_codegen.as_ref().unwrap();
    assert_eq!(n.object_cache.hits, 0);
    assert_eq!(n.object_cache.corrupt, n.actual_units);
    assert_eq!(n.object_cache.stored, n.actual_units);
    assert_eq!(
        executable,
        fs::read(&rebuilt.artifacts["executable"]).unwrap()
    );
    assert_eq!(d.build(&o).native_codegen.unwrap().object_cache.misses, 0);
}
#[test]
fn unsafe_or_invalid_current_source_never_reuses_a_success_capability() {
    let d = Dir::new();
    d.source();
    let mut o = d.options(8, Optimization::O0);
    let a = d.build(&o);
    success(&a, 42);
    let bytes = fs::read(o.output.as_ref().unwrap()).unwrap();
    let before = entries(o.object_cache.as_ref().unwrap());
    d.write(
        "main.zil",
        "fn main()->i32 { var a=1; let r=&a; a=2; return *r; }",
    );
    let bad = d.build(&o);
    assert_eq!(bad.status, ResultCategory::SafeReject);
    assert_eq!(bad.diagnostics[0].code, "E-BORROW-CONFLICT");
    assert!(bad.commands.is_empty());
    assert!(bad.native_codegen.is_none());
    assert_eq!(fs::read(o.output.as_ref().unwrap()).unwrap(), bytes);
    assert_eq!(entries(o.object_cache.as_ref().unwrap()), before);
    d.source();
    o.limits.max_analysis_steps = 0;
    let limited = d.build(&o);
    assert_ne!(limited.status, ResultCategory::Success);
    assert_eq!(entries(o.object_cache.as_ref().unwrap()), before);
}
#[test]
fn linker_failure_discards_pending_objects_and_preserves_previous_output() {
    let d = Dir::new();
    d.source();
    let mut initial = d.options(8, Optimization::O0);
    initial.object_cache = None;
    success(&d.build(&initial), 42);
    let previous = fs::read(initial.output.as_ref().unwrap()).unwrap();
    let mut o = d.options(8, Optimization::O0);
    o.clang = Some("/bin/false".into());
    let failed = d.build(&o);
    assert_ne!(failed.status, ResultCategory::Success);
    assert!(entries(o.object_cache.as_ref().unwrap()).is_empty());
    assert_eq!(fs::read(o.output.as_ref().unwrap()).unwrap(), previous);
}
#[test]
fn cancellation_after_backend_success_does_not_commit_object_cache() {
    let d = Dir::new();
    d.write("main.zil", "fn main()->i32 { return 42; }");
    let o = d.options(1, Optimization::O0);
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        d.0.join("main.zil").to_string_lossy(),
        "fn main()->i32 { return 42; }",
    );
    let mut session = CompilationSession::default();
    let token = o.cancellation.clone();
    let r = session.compile_source_with_observer(&source, &o, &mut |event| {
        if event.phase == Phase::Link {
            token.cancel();
        }
    });
    assert_eq!(r.status, ResultCategory::Cancelled, "{}", r.to_json());
    assert!(entries(o.object_cache.as_ref().unwrap()).is_empty());
    assert!(!o.output.as_ref().unwrap().exists());
}
#[test]
fn cache_directory_failure_is_a_visible_bypass_not_a_build_failure() {
    let d = Dir::new();
    d.source();
    d.write("objects", "must remain a file");
    let o = d.options(8, Optimization::O0);
    let r = d.build(&o);
    success(&r, 42);
    let c = &r.native_codegen.as_ref().unwrap().object_cache;
    assert!(c.requested && !c.enabled);
    assert!(c.disabled_reason.is_some());
    assert_eq!(
        fs::read(o.object_cache.as_ref().unwrap()).unwrap(),
        b"must remain a file"
    );
}
#[test]
fn diagnostic_fingerprint_change_does_not_pollute_machine_keys() {
    let d = Dir::new();
    d.source();
    let o = d.options(8, Optimization::O0);
    let first = d.build(&o);
    success(&first, 42);
    let ir = fs::read(&first.artifacts["llvm"]).unwrap();
    // A source comment changes the current verification input, not instructions.
    d.write("main.zil","// current request has different source\nimport a\nimport b\nfn main()->i32 { return a.get()+b.get(); }");
    let again = d.build(&o);
    success(&again, 42);
    assert_ne!(ir, fs::read(&again.artifacts["llvm"]).unwrap());
    let n = again.native_codegen.as_ref().unwrap();
    assert_eq!(n.object_cache.hits, n.actual_units);
}
#[test]
fn concurrent_requests_share_only_complete_native_objects() {
    let d = Dir::new();
    d.source();
    let source = d.0.join("main.zil");
    let options = d.options(8, Optimization::O0);
    std::thread::scope(|scope| {
        let mut handles = vec![];
        for i in 0..4 {
            let mut o = options.clone();
            o.output = Some(d.0.join(format!("p{i}")));
            let p = &source;
            handles.push(scope.spawn(move || compile_path(p, &o)));
        }
        for h in handles {
            success(&h.join().unwrap(), 42);
        }
    });
    let warm = d.build(&options);
    success(&warm, 42);
    assert_eq!(warm.native_codegen.as_ref().unwrap().object_cache.misses, 0);
}

#[test]
fn owned_state_store_and_callbacks_match_on_cold_and_warm_o0_o2() {
    use std::io::Write;
    use std::process::Stdio;
    let d = Dir::new();
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/mainline/zui_language177");
    for entry in ["main", "field_exit"] {
        for opt in [Optimization::O0, Optimization::O2] {
            for units in [1, 8] {
                let o = d.options(units, opt);
                for warm in [false, true] {
                    let r = compile_path(&root.join(format!("{entry}.zil")), &o);
                    assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
                    assert_eq!(r.verification_runs, 1);
                    let c = r.native_codegen.as_ref().unwrap();
                    if warm {
                        assert_eq!(c.object_cache.hits, c.actual_units);
                    }
                    for (input, main, field) in [
                        (b'0', "HZMK", "HZ"),
                        (b'1', "DHZMNK", "DHZ"),
                        (b'2', "DEHZMNOK", "DEHZ"),
                        (b'3', "DEFHZMNOPK", "DEFHZ"),
                    ] {
                        let mut child = Command::new(&r.artifacts["executable"])
                            .stdin(Stdio::piped())
                            .stdout(Stdio::piped())
                            .stderr(Stdio::piped())
                            .spawn()
                            .unwrap();
                        child.stdin.take().unwrap().write_all(&[input]).unwrap();
                        let out = child.wait_with_output().unwrap();
                        assert_eq!(
                            out.status.code(),
                            Some(if entry == "main" { 0 } else { 1 }),
                            "{entry} {opt:?} units={units} warm={warm} {out:?}"
                        );
                        assert_eq!(
                            out.stdout,
                            if entry == "main" { main } else { field }.as_bytes()
                        );
                        assert!(out.stderr.is_empty());
                    }
                }
            }
        }
    }
}
#[test]
fn current_checked_overflow_still_traps_on_cache_hits() {
    let d = Dir::new();
    d.write(
        "main.zil",
        "fn add(x:i32)->i32 { return x+1; } fn main()->i32 { return add(2147483647); }",
    );
    for opt in [Optimization::O0, Optimization::O2] {
        let o = d.options(8, opt);
        for _ in 0..2 {
            let r = d.build(&o);
            assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
            assert_eq!(r.verification_runs, 1);
            let out = Command::new(&r.artifacts["executable"]).output().unwrap();
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(out.status.signal(), Some(4));
        }
    }
}

#[test]
fn large_file_o0_chunks_reuse_unchanged_objects_and_keep_current_verification() {
    fn source(count: usize, delta: i32, extra: bool) -> String {
        let mut s = if extra {
            "fn inserted()->i32{return 7;}\n".to_owned()
        } else {
            String::new()
        };
        for i in 0..count {
            let n = (i % 3) as i32 + if i == 0 { delta } else { 0 };
            s += &format!("fn f{i}(x:i32)->i32{{return x+{n};}}\n");
        }
        s += "fn main()->i32{var total:i32=0;\n";
        for i in 0..count {
            s += &format!("total=total+f{i}(0);\n");
        }
        s += &format!(
            "return total-{};}}\n",
            (0..count).map(|i| i % 3).sum::<usize>()
        );
        s
    }
    let d = Dir::new();
    let original = source(600, 0, false);
    d.write("main.zil", &original);
    let o = d.options(8, Optimization::O0);
    let cold = d.build(&o);
    success(&cold, 0);
    let n = cold.native_codegen.as_ref().unwrap();
    assert_eq!(n.actual_units, 3, "{}", cold.to_json());
    assert_eq!(n.object_cache.misses, 3);
    let warm = d.build(&o);
    success(&warm, 0);
    assert_eq!(warm.native_codegen.as_ref().unwrap().object_cache.hits, 3);
    d.write("main.zil", &source(600, 1, false));
    let edit = d.build(&o);
    success(&edit, 1);
    let n = edit.native_codegen.as_ref().unwrap();
    assert_eq!(n.object_cache.misses, 1, "{}", edit.to_json());
    assert_eq!(n.object_cache.hits, 2);
    // Declaration insertion/deletion can move IDs; they must remain current,
    // even when their cache reuse is conservatively worse than a body edit.
    for (count, inserted) in [(600, true), (599, false)] {
        d.write("main.zil", &source(count, 2, inserted));
        success(&d.build(&o), 2);
    }
    d.write("main.zil", &original);
    let restored = d.build(&o);
    success(&restored, 0);
    assert_eq!(
        restored.native_codegen.as_ref().unwrap().object_cache.hits,
        3
    );
    let executable = fs::read(o.output.as_ref().unwrap()).unwrap();
    d.write("main.zil", &original.replace("f300(0)", "f300(0,1)"));
    let bad = d.build(&o);
    assert_eq!(bad.status, ResultCategory::SafeReject);
    assert_eq!(bad.phase, Phase::Types);
    assert!(bad.native_codegen.is_none());
    assert_eq!(fs::read(o.output.as_ref().unwrap()).unwrap(), executable);
    d.write("main.zil", &original);
    // A single requested unit and O2 preserve the original file grouping.
    for (units, optimization) in [(1, Optimization::O0), (8, Optimization::O2)] {
        let r = d.build(&d.options(units, optimization));
        success(&r, 0);
        assert_eq!(r.native_codegen.as_ref().unwrap().actual_units, 1);
    }
}

#[test]
fn o2_folded_results_and_imported_bodies_preserve_current_verification() {
    fn source(count: usize, edited: usize, extra: bool, opaque: bool) -> String {
        let mut text = if extra {
            "fn inserted()->i32{return 7;}\n".to_owned()
        } else {
            String::new()
        };
        if opaque {
            text += "extern \"C\" fn getchar()->i32;\n";
        }
        for i in 0..count {
            text += &format!(
                "fn f{i}(x:i32)->i32{{return x+{};}}\n",
                i32::from(i == edited)
            );
        }
        text += "fn main()->i32{var total:i32=0;\n";
        if opaque {
            // success() supplies EOF, so the runtime value is exactly zero;
            // it remains unknown to the compile-time scalar evaluator.
            text += "var seed:i32=0;unsafe{seed=getchar()+1;}\n";
        }
        let argument = if opaque { "seed" } else { "0" };
        for i in 0..count {
            text += &format!("total=total+f{i}({argument});\n");
        }
        text += "return total;}\n";
        text
    }
    for (closed, opaque) in [(false, false), (true, false), (false, true), (true, true)] {
        let d = Dir::new();
        let original = source(600, usize::MAX, false, opaque);
        d.write("main.zil", &original);
        let mut options = d.options(8, Optimization::O2);
        options.incremental_o2 = true;
        options.closed_executable = closed;
        // Only the fully folded closed program satisfies the flat reachability
        // gate. Its helper definitions are removed before object construction.
        let pruned = closed && !opaque;
        let units = if pruned { 1 } else { 3 };
        let cold = d.build(&options);
        success(&cold, 0);
        assert_eq!(cold.native_codegen.as_ref().unwrap().actual_units, units);
        assert_eq!(
            cold.native_codegen.as_ref().unwrap().object_cache.misses,
            units
        );
        let machine = fs::read_to_string(&cold.artifacts["llvm"]).unwrap();
        if pruned {
            assert_eq!(
                machine
                    .lines()
                    .filter(|line| line.starts_with("define "))
                    .count(),
                2
            );
            assert!(machine.contains("@zil.stable."));
            assert!(!machine.contains("@zil.fn."));
        } else {
            assert_eq!(
                machine
                    .lines()
                    .filter(|line| line.starts_with("define "))
                    .count(),
                602
            );
            assert!(!machine.contains("@zil.stable."));
        }
        let direct_calls = machine.matches("call i32 @zil.fn.").count();
        assert_eq!(
            direct_calls >= 600,
            opaque,
            "opaque={opaque}: {direct_calls}"
        );
        let warm = d.build(&options);
        success(&warm, 0);
        let summary = warm.native_codegen.as_ref().unwrap();
        assert_eq!(summary.object_cache.hits, units);
        assert!(
            summary
                .object_reports
                .iter()
                .all(|r| r.contains("\"input_kind\":\"object-cache\"")
                    && r.contains("\"producer_verified_before_after\":true")
                    && r.contains("\"preflight_ns\":")),
            "{}",
            warm.to_json()
        );
        // A real body edit preserves the known result. A removed helper has no
        // remaining owner; retained owners and opaque consumers still depend
        // on the actual body under their respective export contracts.
        let old_body = "fn f0(x:i32)->i32{return x+0;}";
        let new_body = "fn f0(x:i32)->i32{return x*1;}";
        assert_eq!(original.matches(old_body).count(), 1);
        let neutral_source = original.replacen(old_body, new_body, 1);
        assert_ne!(neutral_source, original);
        assert_eq!(neutral_source.matches(new_body).count(), 1);
        d.write("main.zil", &neutral_source);
        let neutral = d.build(&options);
        success(&neutral, 0);
        let neutral_cache = &neutral.native_codegen.as_ref().unwrap().object_cache;
        let neutral_misses = if pruned {
            0
        } else if opaque {
            2
        } else {
            1
        };
        assert_eq!(
            neutral_cache.misses,
            neutral_misses,
            "{}",
            neutral.to_json()
        );
        assert_eq!(
            neutral_cache.hits,
            units - neutral_misses,
            "{}",
            neutral.to_json()
        );
        d.write("main.zil", &original);
        let neutral_restored = d.build(&options);
        success(&neutral_restored, 0);
        let restored_cache = &neutral_restored
            .native_codegen
            .as_ref()
            .unwrap()
            .object_cache;
        assert_eq!(restored_cache.hits, units, "{}", neutral_restored.to_json());
        assert_eq!(restored_cache.misses, 0, "{}", neutral_restored.to_json());
        // Retained f0/main owners both change under the ordinary contract.
        // Closed pruning keeps one actual entry unit whose result now changes.
        d.write("main.zil", &source(600, 0, false, opaque));
        let early = d.build(&options);
        success(&early, 1);
        let early_machine = fs::read_to_string(&early.artifacts["llvm"]).unwrap();
        assert_eq!(
            early.native_codegen.as_ref().unwrap().object_cache.misses,
            if pruned { 1 } else { 2 },
            "{}",
            early.to_json()
        );
        assert_eq!(
            early.native_codegen.as_ref().unwrap().object_cache.hits,
            if pruned { 0 } else { 1 }
        );
        d.write("main.zil", &source(600, 599, false, opaque));
        let late = d.build(&options);
        success(&late, 1);
        // The joint summary replaces main with Return(1). After closed pruning,
        // both retained definitions therefore equal the preceding edit; their
        // actual object is reusable. Ordinary owners still retain f599's body.
        if pruned {
            let late_machine = fs::read_to_string(&late.artifacts["llvm"]).unwrap();
            assert_eq!(
                early_machine.split_once("define ").unwrap().1,
                late_machine.split_once("define ").unwrap().1
            );
            fn action_key(report: &zil_driver::CompileReport) -> &str {
                report.native_codegen.as_ref().unwrap().object_reports[0]
                    .split("\"action_key\":\"")
                    .nth(1)
                    .unwrap()
                    .split('"')
                    .next()
                    .unwrap()
            }
            assert_eq!(action_key(&early), action_key(&late));
        }
        let late_misses = usize::from(!pruned);
        assert_eq!(
            late.native_codegen.as_ref().unwrap().object_cache.misses,
            late_misses,
            "{}",
            late.to_json()
        );
        assert_eq!(
            late.native_codegen.as_ref().unwrap().object_cache.hits,
            units - late_misses
        );
        for (count, extra) in [(600, true), (599, false)] {
            d.write("main.zil", &source(count, 0, extra, opaque));
            success(&d.build(&options), 1);
        }
        d.write("main.zil", &original);
        let restored = d.build(&options);
        success(&restored, 0);
        assert_eq!(
            restored.native_codegen.as_ref().unwrap().object_cache.hits,
            units
        );
        let executable = fs::read(options.output.as_ref().unwrap()).unwrap();
        let argument = if opaque { "seed" } else { "0" };
        d.write(
            "main.zil",
            &original.replace(&format!("f300({argument})"), &format!("f300({argument},1)")),
        );
        let bad = d.build(&options);
        assert_eq!(bad.status, ResultCategory::SafeReject);
        assert_eq!(bad.phase, Phase::Types);
        assert!(bad.native_codegen.is_none());
        assert_eq!(
            fs::read(options.output.as_ref().unwrap()).unwrap(),
            executable
        );
    }
}

#[test]
fn object_patch_preserves_merged_exports_and_always_uses_a_full_base() {
    fn source(changed: usize, delta: usize, called: usize, neighbor: usize) -> String {
        let mut source = String::new();
        for i in 0..600 {
            source += &format!(
                "fn f{i}(x:i32)->i32{{return x+{};}}\n",
                if i == changed { delta } else { 1 }
            );
        }
        source += &format!(
            "extern \"C\" fn getchar()->i32;fn main()->i32{{let p:fn(i32)->i32=f{called};let q:fn(i32)->i32=f{neighbor};var x:i32=0;unsafe{{x=getchar();}}if x==0{{return p(2147483647);}}return p(x)-q(x);}}\n"
        );
        source
    }
    fn execute(report: &CompileReport, seed: u8, result: Option<i32>) {
        use std::io::Write;
        use std::os::unix::process::ExitStatusExt;
        use std::process::Stdio;
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let mut child = Command::new(&report.artifacts["executable"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(&[seed]).unwrap();
        let output = child.wait_with_output().unwrap();
        if let Some(code) = result {
            assert_eq!(output.status.code(), Some(code), "{output:?}");
        } else {
            assert_eq!(output.status.signal(), Some(4), "{output:?}");
        }
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
    fn patch_report(report: &CompileReport) -> &str {
        report
            .native_codegen
            .as_ref()
            .unwrap()
            .object_reports
            .iter()
            .find(|r| r.contains("\"input_kind\":\"object-patch\""))
            .unwrap_or_else(|| panic!("{}", report.to_json()))
    }
    fn base_key(report: &str) -> &str {
        report
            .split("\"base_action_key\":\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
    }
    let d = Dir::new();
    let original = source(usize::MAX, 1, 0, 1);
    d.write("main.zil", &original);
    let mut options = d.options(8, Optimization::O2);
    options.incremental_o2 = true;
    options.function_merging = zil_driver::FunctionMerging::AfterOptimization;
    let cold = d.build(&options);
    execute(&cold, 7, Some(0));
    // Inspect actual LLVM output, choosing a merged representative rather than
    // assuming that f0 is the shared implementation. Calls stay opaque in ZIL.
    let symbols = Command::new("nm")
        .args(["-S", "--defined-only"])
        .arg(&cold.artifacts["executable"])
        .output()
        .unwrap();
    assert!(symbols.status.success());
    let leader = String::from_utf8(symbols.stdout)
        .unwrap()
        .lines()
        .find_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.len() != 4 || fields[2] != "T" {
                return None;
            }
            let id = fields[3].strip_prefix("zil.fn.")?.parse::<usize>().ok()?;
            (id < 256 && usize::from_str_radix(fields[1], 16).ok()? > 5).then_some(id)
        })
        .expect("LLVM emitted a full merged representative in the first leaf unit");
    let assembly = Command::new("objdump")
        .args(["-d", "--no-show-raw-insn"])
        .arg(&cold.artifacts["executable"])
        .output()
        .unwrap();
    assert!(assembly.status.success());
    let mut current = None;
    let neighbor = String::from_utf8(assembly.stdout)
        .unwrap()
        .lines()
        .find_map(|line| {
            if line.ends_with(">:") {
                current = line
                    .split("<zil.fn.")
                    .nth(1)
                    .and_then(|name| name.strip_suffix(">:"))
                    .and_then(|id| id.parse::<usize>().ok());
                return None;
            }
            let id = current?;
            let instruction = line
                .split_once(':')?
                .1
                .split_whitespace()
                .collect::<Vec<_>>();
            (id < 256
                && id != leader
                && instruction.first() == Some(&"jmp")
                && instruction.last() == Some(&format!("<zil.fn.{leader}>").as_str()))
            .then_some(id)
        })
        .expect("an unchanged canonical function really jumps to the chosen merged representative");
    d.write("main.zil", &source(leader, 2, leader, neighbor));
    let first = d.build(&options);
    execute(&first, 7, Some(1));
    execute(&first, 0, None);
    let first_patch = patch_report(&first);
    assert!(first_patch.contains("\"current_full_machine_preflight\":true"));
    assert_eq!(first.commands.len(), 3);
    let cumulative = source(leader, 2, leader, neighbor).replace(
        &format!("fn f{neighbor}(x:i32)->i32{{return x+1;}}"),
        &format!("fn f{neighbor}(x:i32)->i32{{return x+3;}}"),
    );
    d.write("main.zil", &cumulative);
    let second = d.build(&options);
    execute(&second, 7, Some(255));
    assert_eq!(base_key(first_patch), base_key(patch_report(&second)));
    assert!(patch_report(&second).contains("\"patched_functions\":2"));
    d.write("main.zil", &original);
    let restored = d.build(&options);
    execute(&restored, 7, Some(0));
    assert_eq!(
        restored
            .native_codegen
            .as_ref()
            .unwrap()
            .object_cache
            .misses,
        0
    );
    // A damaged optional base index cannot authorize reuse or prevent a fresh
    // build. Corrupt only private base records, preserving exact-object entries.
    fn corrupt_bases(path: &Path, inside: bool) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                let is_base = inside || path.file_name().unwrap() == "full-bases-v1";
                corrupt_bases(&path, is_base);
            } else if inside {
                fs::write(path, b"broken base record").unwrap();
            }
        }
    }
    corrupt_bases(options.object_cache.as_ref().unwrap(), false);
    d.write("main.zil", &source(leader, 4, leader, neighbor));
    let recovered = d.build(&options);
    execute(&recovered, 7, Some(3));
    assert!(
        recovered
            .native_codegen
            .as_ref()
            .unwrap()
            .object_reports
            .iter()
            .all(|r| !r.contains("\"input_kind\":\"object-patch\""))
    );
    let output = fs::read(&recovered.artifacts["executable"]).unwrap();
    d.write(
        "main.zil",
        &source(leader, 4, leader, neighbor).replace("return x+4;", "return true;"),
    );
    let rejected = d.build(&options);
    assert_eq!(rejected.status, ResultCategory::SafeReject);
    assert!(rejected.native_codegen.is_none());
    assert_eq!(fs::read(options.output.as_ref().unwrap()).unwrap(), output);
}

#[test]
fn propagated_results_do_not_invalidate_unmodified_leaf_units() {
    let d = Dir::new();
    let mut main = String::new();
    for module in 0..4 {
        main += &format!("import m{module}\n");
        let mut source = String::new();
        for function in 0..256 {
            source += &format!("fn f{function}(x:i32)->i32{{return x+0;}}\n");
        }
        source += "pub fn value()->i32{var total:i32=0;\n";
        for function in 0..256 {
            source += &format!("total=total+f{function}(0);\n");
        }
        source += "return total;}\n";
        d.write(&format!("m{module}.zil"), &source);
    }
    main += "fn main()->i32{return m0.value()+m1.value()+m2.value()+m3.value();}";
    d.write("main.zil", &main);
    let mut options = d.options(8, Optimization::O2);
    options.incremental_o2 = true;
    let cold = d.build(&options);
    success(&cold, 0);
    let codegen = cold.native_codegen.as_ref().unwrap();
    assert_eq!(codegen.actual_units, 5, "{}", cold.to_json());
    assert_eq!(codegen.object_cache.misses, 5);
    // All 1,024 external leaf definitions, four value functions, source main
    // and the native wrapper remain present. This is not closed deletion.
    let machine = fs::read_to_string(&cold.artifacts["llvm"]).unwrap();
    assert_eq!(
        machine
            .lines()
            .filter(|line| line.starts_with("define "))
            .count(),
        1030
    );
    let warm = d.build(&options);
    success(&warm, 0);
    assert_eq!(warm.native_codegen.as_ref().unwrap().object_cache.hits, 5);
    let original = fs::read_to_string(d.0.join("m0.zil")).unwrap();
    d.write(
        "m0.zil",
        &original.replacen("return x+0;", "return x+1;", 1),
    );
    let edited = d.build(&options);
    success(&edited, 1);
    let cache = &edited.native_codegen.as_ref().unwrap().object_cache;
    // Only the changed leaf unit and the small propagated-caller unit miss;
    // the other three modules' leaf objects remain current and reusable.
    assert_eq!(cache.misses, 2, "{}", edited.to_json());
    assert_eq!(cache.hits, 3);
    d.write("m0.zil", &original);
    let restored = d.build(&options);
    success(&restored, 0);
    assert_eq!(
        restored.native_codegen.as_ref().unwrap().object_cache.hits,
        5
    );
}

#[test]
fn large_file_chunks_preserve_owned_closures_drop_and_function_addresses() {
    let d = Dir::new();
    let mut source = String::from(
        "extern \"C\" fn putchar(n:i32)->i32;\nresource Token{code:i32}\nimpl Drop for Token{fn drop(self:Self){unsafe{putchar(.code)}}}\nfn make(n:i32)->Token contract{result(create)}{return Token{code:n}}\nfn early()->i32{let t=make(65);let f=move()=>t.code;return f()-65;}\n",
    );
    for i in 0..600 {
        source += &format!("fn padding{i}(x:i32)->i32{{return x+1;}}\n");
    }
    source += "fn late()->i32{let t=make(66);let f=move()=>t.code;let p:fn(i32)->i32=padding0;return f()+p(0)-67;}\nfn main()->i32{return early()+late();}\n";
    d.write("main.zil", &source);
    let mut o = d.options(8, Optimization::O0);
    use zil_driver::FunctionMerging::{AfterOptimization, BeforeAndAfter, Disabled};
    for (backend, closed, optimization, incremental, merging) in [
        (
            NativeBackend::LlvmApi,
            false,
            Optimization::O0,
            false,
            Disabled,
        ),
        (
            NativeBackend::LlvmApi,
            true,
            Optimization::O0,
            false,
            Disabled,
        ),
        (
            NativeBackend::Clang,
            true,
            Optimization::O0,
            false,
            Disabled,
        ),
        (
            NativeBackend::LlvmApi,
            false,
            Optimization::O2,
            true,
            Disabled,
        ),
        (
            NativeBackend::LlvmApi,
            true,
            Optimization::O2,
            true,
            Disabled,
        ),
        (NativeBackend::Clang, true, Optimization::O2, true, Disabled),
        (
            NativeBackend::LlvmApi,
            false,
            Optimization::O2,
            true,
            AfterOptimization,
        ),
        (
            NativeBackend::LlvmApi,
            true,
            Optimization::O2,
            true,
            AfterOptimization,
        ),
        (
            NativeBackend::LlvmApi,
            false,
            Optimization::O2,
            true,
            BeforeAndAfter,
        ),
        (
            NativeBackend::LlvmApi,
            true,
            Optimization::O2,
            true,
            BeforeAndAfter,
        ),
        (
            NativeBackend::Clang,
            true,
            Optimization::O2,
            true,
            AfterOptimization,
        ),
    ] {
        o.optimization = optimization;
        o.incremental_o2 = incremental;
        o.function_merging = merging;
        o.object_cache = Some(d.0.join("objects"));
        o.closed_executable = closed;
        o.backend = backend;
        if backend == NativeBackend::Clang {
            o.object_cache = None;
        }
        let report = d.build(&o);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        if backend == NativeBackend::LlvmApi {
            assert_eq!(report.commands.len(), 1);
        }
        let native = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        assert_eq!(native.status.code(), Some(0));
        assert_eq!(native.stdout, b"AB");
        let codegen = report.native_codegen.as_ref().unwrap();
        if incremental {
            // Complete O2 closures must preserve the shared local resource
            // implementation instead of importing only a safe-looking prefix.
            assert_eq!(codegen.actual_units, 1);
            assert_eq!(
                codegen.fallback.as_deref(),
                Some("complete O2 call closure requires a local definition")
            );
        } else {
            assert!(codegen.actual_units > 1, "{}", report.to_json());
        }
        if backend == NativeBackend::LlvmApi && incremental {
            let warm = d.build(&o);
            assert_eq!(warm.status, ResultCategory::Success, "{}", warm.to_json());
            assert_eq!(warm.verification_runs, 1);
            assert_eq!(warm.commands.len(), 1);
            let cached = warm.native_codegen.as_ref().unwrap();
            assert_eq!(cached.actual_units, 1);
            assert_eq!(cached.fallback, codegen.fallback);
            assert_eq!(cached.object_cache.hits, 1);
            assert_eq!(cached.object_cache.misses, 0);
            let native = Command::new(&warm.artifacts["executable"])
                .output()
                .unwrap();
            assert_eq!(native.status.code(), Some(0));
            assert_eq!(native.stdout, b"AB");
        }
    }
}

#[test]
fn explicit_closed_build_preserves_native_behavior_and_separates_cache_identity() {
    fn globals(path: &Path) -> String {
        let output = Command::new("nm")
            .args(["-g", "--defined-only"])
            .arg(path)
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap()
    }
    for optimization in [Optimization::O0, Optimization::O2] {
        let d = Dir::new();
        d.write(
            "main.zil",
            "fn helper()->i32{return 21;}\nfn main()->i32{return helper()+helper();}\n",
        );
        let mut options = d.options(8, optimization);
        for (closed, hits) in [(false, 0), (true, 0), (true, 1), (false, 1)] {
            options.closed_executable = closed;
            let report = d.build(&options);
            success(&report, 42);
            assert_eq!(
                report.native_codegen.as_ref().unwrap().object_cache.hits,
                hits
            );
            assert_eq!(
                globals(&report.artifacts["executable"]).contains("zil.fn."),
                !closed
            );
        }
        options.closed_executable = true;
        options.object_cache = None;
        options.backend = NativeBackend::Clang;
        let report = d.build(&options);
        success(&report, 42);
        assert!(!globals(&report.artifacts["executable"]).contains("zil.fn."));
        let before = fs::read(&report.artifacts["executable"]).unwrap();
        options.clang = Some(d.0.join("missing-clang"));
        let failed = d.build(&options);
        assert_eq!(failed.status, ResultCategory::InfraError);
        assert_eq!(before, fs::read(&report.artifacts["executable"]).unwrap());
    }
    let d = Dir::new();
    d.source();
    let mut options = d.options(8, Optimization::O2);
    options.closed_executable = true;
    let report = d.build(&options);
    success(&report, 42);
    assert!(report.native_codegen.as_ref().unwrap().actual_units > 1);
    assert!(
        globals(&report.artifacts["executable"]).contains("zil.fn."),
        "cross-unit definitions must remain available"
    );
}

#[test]
fn function_merging_pipeline_changes_actual_cache_keys_and_keeps_failure_transactional() {
    use zil_driver::FunctionMerging;
    let d = Dir::new();
    d.source();
    let mut options = d.options(8, Optimization::O2);
    let policies = [
        FunctionMerging::Disabled,
        FunctionMerging::AfterOptimization,
        FunctionMerging::BeforeAndAfter,
    ];
    let mut first_keys = Vec::<Vec<String>>::new();
    for round in 0..2 {
        for policy in policies {
            options.function_merging = policy;
            let report = d.build(&options);
            success(&report, 42);
            let native = report.native_codegen.as_ref().unwrap();
            assert!(native.actual_units > 1);
            assert_eq!(
                native.object_cache.hits,
                if round == 0 { 0 } else { native.actual_units }
            );
            assert_eq!(
                native.object_cache.misses,
                if round == 0 { native.actual_units } else { 0 }
            );
            let keys = native
                .object_reports
                .iter()
                .map(|r| {
                    r.split("\"action_key\":\"")
                        .nth(1)
                        .unwrap()
                        .split('"')
                        .next()
                        .unwrap()
                        .to_owned()
                })
                .collect::<Vec<_>>();
            assert!(
                native
                    .object_reports
                    .iter()
                    .all(|r| r.contains("\"pipeline\":")
                        && r.contains(match policy {
                            FunctionMerging::Disabled => "default<O2>",
                            FunctionMerging::AfterOptimization =>
                                "default<O2>|merge-functions=after|aliases=false",
                            FunctionMerging::BeforeAndAfter =>
                                "mergefunc,default<O2>|merge-functions=after|aliases=false",
                        }))
            );
            if round == 0 {
                assert!(
                    first_keys
                        .iter()
                        .all(|prior| keys.iter().all(|key| !prior.contains(key)))
                );
                first_keys.push(keys);
            } else {
                assert!(
                    first_keys.contains(&keys),
                    "switching back must reuse that policy's original keys"
                );
            }
        }
    }
    let executable = fs::read(options.output.as_ref().unwrap()).unwrap();
    let before = entries(&d.0.join("objects"))
        .into_iter()
        .map(|p| {
            let bytes = fs::read(&p).unwrap();
            (p, bytes)
        })
        .collect::<Vec<_>>();
    d.write(
        "main.zil",
        "import a\nimport b\nfn main()->i32{return a.get(1)+b.get();}",
    );
    let bad = d.build(&options);
    assert_eq!(bad.status, ResultCategory::SafeReject);
    assert_eq!(bad.phase, Phase::Types);
    assert!(bad.native_codegen.is_none());
    assert_eq!(
        fs::read(options.output.as_ref().unwrap()).unwrap(),
        executable
    );
    let after = entries(&d.0.join("objects"))
        .into_iter()
        .map(|p| {
            let bytes = fs::read(&p).unwrap();
            (p, bytes)
        })
        .collect::<Vec<_>>();
    assert_eq!(before, after);
}

#[test]
fn monotone_chain_edits_reuse_normalized_objects_but_keep_overflow_order() {
    use std::io::Write;
    use std::os::unix::process::ExitStatusExt;
    use std::process::Stdio;

    fn source(body: &str) -> String {
        format!(
            "fn h(x:i32)->i32{{var t:i32=x;{body}return t;}}\n\
             extern \"C\" fn getchar()->i32;\n\
             extern \"C\" fn putchar(x:i32)->i32;\n\
             fn main()->i32{{var seed:i32=0;unsafe{{seed=getchar();}}\
             let x:i32=2147483644+seed;let result:i32=h(x);\
             unsafe{{putchar(67);}}return result%200;}}"
        )
    }
    fn cache_and_native(report: &CompileReport, hits: usize, misses: usize, trap: bool) {
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1, "{}", report.to_json());
        assert_eq!(report.commands.len(), 1);
        let codegen = report.native_codegen.as_ref().unwrap();
        assert_eq!(codegen.actual_units, 1, "{}", report.to_json());
        assert_eq!(codegen.object_cache.hits, hits, "{}", report.to_json());
        assert_eq!(codegen.object_cache.misses, misses, "{}", report.to_json());
        let mut child = Command::new("sh")
            .args(["-c", "ulimit -c 0; exec \"$1\"", "zil-chain-native"])
            .arg(&report.artifacts["executable"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(&[0]).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.stderr.is_empty(), "{output:?}");
        if trap {
            assert_eq!(output.status.signal(), Some(4), "{output:?}");
            assert!(output.stdout.is_empty(), "{output:?}");
        } else {
            assert_eq!(output.status.code(), Some(47), "{output:?}");
            assert_eq!(output.stdout, b"C", "{output:?}");
        }
    }
    fn cache_snapshot(path: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        entries(path)
            .into_iter()
            .map(|entry| {
                let bytes = fs::read(&entry).unwrap();
                (entry, bytes)
            })
            .collect()
    }

    let d = Dir::new();
    let mut options = d.options(1, Optimization::O2);
    options.codegen.jobs = 1;
    options.incremental_o2 = true;
    options.function_merging = zil_driver::FunctionMerging::AfterOptimization;
    let original = source("t=t+1;t=t+2;");
    let swapped = source("t=t+2;t=t+1;");
    let mixed = source("t=t+4;t=t-1;");
    assert_ne!(original, swapped);
    assert_ne!(original, mixed);
    d.write("main.zil", &original);
    cache_and_native(&d.build(&options), 0, 1, false);
    cache_and_native(&d.build(&options), 1, 0, false);

    // Swapping same-direction constants preserves the normalized object input.
    // The cache must still distinguish the mixed-direction overflow below.
    d.write("main.zil", &swapped);
    cache_and_native(&d.build(&options), 1, 0, false);
    d.write("main.zil", &mixed);
    cache_and_native(&d.build(&options), 0, 1, true);
    d.write("main.zil", &original);
    let restored = d.build(&options);
    cache_and_native(&restored, 1, 0, false);

    let executable = fs::read(&restored.artifacts["executable"]).unwrap();
    let llvm = fs::read(&restored.artifacts["llvm"]).unwrap();
    let cache = cache_snapshot(options.object_cache.as_ref().unwrap());
    assert_eq!(original.matches("h(x);").count(), 1);
    let invalid = original.replacen("h(x);", "h(x,0);", 1);
    assert_ne!(invalid, original);
    d.write("main.zil", &invalid);
    let rejected = d.build(&options);
    assert_eq!(rejected.status, ResultCategory::SafeReject);
    assert_eq!(rejected.phase, Phase::Types);
    assert_eq!(rejected.diagnostics[0].code, "E-CALL-ARITY");
    assert!(rejected.native_codegen.is_none());
    assert_eq!(
        fs::read(&restored.artifacts["executable"]).unwrap(),
        executable
    );
    assert_eq!(fs::read(&restored.artifacts["llvm"]).unwrap(), llvm);
    assert_eq!(
        cache_snapshot(options.object_cache.as_ref().unwrap()),
        cache
    );
}
