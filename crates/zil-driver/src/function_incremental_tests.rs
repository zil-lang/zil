//! Real source controls for function-scoped Types reuse and current provenance.
use super::*;
use crate::ProjectSources;
use std::sync::atomic::{AtomicUsize, Ordering};
use zil_base::{FileId, ModuleId};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "zil-function-types-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn project(&self, main: &str, library: &str) -> ProjectSources {
        let modules = [("main.zil", main), ("library.zil", library)]
            .into_iter()
            .enumerate()
            .map(|(i, (name, text))| {
                let path = self.0.join(name);
                fs::write(&path, text).unwrap();
                SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    path.to_string_lossy(),
                    text,
                )
            })
            .collect();
        ProjectSources {
            root: ModuleId(0),
            modules,
            paths: BTreeMap::from([(vec!["library".into()], ModuleId(1))]),
        }
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
const MAIN: &str = "import library\nfn value()->i32{return library.answer();}";
const LIBRARY: &str = "fn leaf(x:i32)->i32{return x+1;} pub fn answer()->i32{return leaf(41);}";
fn success(r: &CompileReport) {
    assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
    assert_eq!(r.verification_runs, 1, "{}", r.to_json());
}
fn typed<'a>(s: &'a CompilationSession, r: &CompileReport) -> &'a TypedProgram {
    s.raw
        .values()
        .find_map(|v| match v {
            RawSnapshot::ProjectTyped(p) if p.module.source_fingerprint == r.input_fingerprint => {
                Some(p.as_ref())
            }
            _ => None,
        })
        .expect("current successful Types owner")
}
fn compare_fresh(
    s: &CompilationSession,
    r: &CompileReport,
    p: &ProjectSources,
    options: &CompileOptions,
) {
    let mut options = options.clone();
    options.semantic_cache = None;
    options.cancellation = Default::default();
    let mut fresh = CompilationSession::default();
    let f = fresh.compile_project(p, &options);
    assert_eq!(r.status, f.status, "{}\n{}", r.to_json(), f.to_json());
    assert_eq!(r.phase, f.phase);
    assert_eq!(r.diagnostics, f.diagnostics);
    assert_eq!(r.source_files, f.source_files);
    assert_eq!(r.scalar_interfaces, f.scalar_interfaces);
    if r.status == ResultCategory::Success {
        assert_eq!(typed(s, r), typed(&fresh, &f));
    }
}

#[test]
fn function_types_reuse_private_edits_and_rebind_every_shifted_span() {
    let d = Directory::new();
    let options = CompileOptions::default();
    let mut s = CompilationSession::default();
    success(&s.compile_project(&d.project(MAIN, LIBRARY), &options));
    assert_eq!(s.function_type_stats().executed_functions, 3);
    for literal in ["2", "123"] {
        let p = d.project(MAIN, &LIBRARY.replace("x+1", &format!("x+{literal}")));
        let r = s.compile_project(&p, &options);
        success(&r);
        assert_eq!(s.function_type_stats().reused_functions, 2);
        assert_eq!(s.function_type_stats().executed_functions, 1);
        assert!(
            r.query_events
                .iter()
                .any(|e| e.phase == Phase::Types && e.disposition == QueryDisposition::Executed)
        );
        compare_fresh(&s, &r, &p, &options);
    }
}

#[test]
fn function_types_declarations_visibility_and_id_shifts_invalidate_context() {
    let d = Directory::new();
    let options = CompileOptions::default();
    let mut s = CompilationSession::default();
    success(&s.compile_project(&d.project(MAIN, LIBRARY), &options));
    for library in [
        format!("fn inserted()->i32{{return 5;}} {LIBRARY}"),
        format!("fn inserted(x:bool)->bool{{return x;}} {LIBRARY}"),
        LIBRARY.replace("pub fn answer", "fn answer"),
    ] {
        let p = d.project(MAIN, &library);
        let r = s.compile_project(&p, &options);
        compare_fresh(&s, &r, &p, &options);
        // A failed Types query rolls back its previous successful cache epoch.
        if r.status == ResultCategory::Success {
            assert_eq!(s.function_type_stats().reused_functions, 0);
        } else {
            assert!(r.diagnostics.iter().any(|e| e.code == "E-MODULE-PRIVATE"));
        }
    }
}

#[test]
fn function_types_unsafe_io_main_does_not_disable_scalar_helper_reuse() {
    let d = Directory::new();
    let main = "import library\nextern \"C\" fn putchar(x:i32)->i32; fn main()->i32{unsafe {putchar(65);} return library.answer();}";
    let options = CompileOptions::default();
    let mut s = CompilationSession::default();
    success(&s.compile_project(&d.project(main, LIBRARY), &options));
    let p = d.project(main, &LIBRARY.replace("x+1", "x+2"));
    let r = s.compile_project(&p, &options);
    success(&r);
    assert_eq!(s.function_type_stats().reused_functions, 1);
    compare_fresh(&s, &r, &p, &options);
    // A distinct external symbol is part of the complete declaration context.
    let p = d.project(
        &main.replace("putchar", "getchar_argument"),
        &LIBRARY.replace("x+1", "x+3"),
    );
    let r = s.compile_project(&p, &options);
    assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
    assert!(r.verification_runs >= 1);
    assert_eq!(s.function_type_stats().reused_functions, 0);
    compare_fresh(&s, &r, &p, &options);
}

#[test]
fn function_types_packed_epoch_survives_new_session_and_corruption_is_a_miss() {
    let d = Directory::new();
    let options = CompileOptions {
        semantic_cache: Some(d.0.join("cache")),
        ..Default::default()
    };
    let mut first = CompilationSession::default();
    success(&first.compile_project(&d.project(MAIN, LIBRARY), &options));
    let files = fs::read_dir(d.0.join("cache/function-types-v1"))
        .unwrap()
        .map(|p| p.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(files.len(), 1);
    let p = d.project(MAIN, &LIBRARY.replace("x+1", "x+123"));
    let mut next = CompilationSession::default();
    let r = next.compile_project(&p, &options);
    success(&r);
    assert_eq!(next.function_type_stats().reused_functions, 2);
    compare_fresh(&next, &r, &p, &options);
    fs::write(&files[0], b"corrupt previous function epoch").unwrap();
    let p = d.project(MAIN, &LIBRARY.replace("x+1", "x+124"));
    let mut next = CompilationSession::default();
    let r = next.compile_project(&p, &options);
    success(&r);
    assert_eq!(next.function_type_stats().reused_functions, 0);
    compare_fresh(&next, &r, &p, &options);
}

#[test]
fn function_types_shared_budget_fallback_preserves_current_first_diagnostic() {
    let d = Directory::new();
    let initial = d.project(MAIN, LIBRARY);
    let mut fresh = CompilationSession::default();
    let r = fresh.compile_project(&initial, &CompileOptions::default());
    success(&r);
    let instructions = typed(&fresh, &r)
        .module
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .map(|b| b.instructions.len())
        .sum();
    let mut options = CompileOptions::default();
    options.limits.max_instructions = instructions;
    let mut s = CompilationSession::default();
    success(&s.compile_project(&initial, &options));
    let p = d.project(
        &MAIN.replace("library.answer()", "library.answer()+1"),
        LIBRARY,
    );
    let r = s.compile_project(&p, &options);
    assert_eq!(r.status, ResultCategory::ResourceLimit, "{}", r.to_json());
    compare_fresh(&s, &r, &p, &options);

    let before = s.function_type_stats();
    for phase in [Phase::Types, Phase::Analyze, Phase::Verify] {
        let options = CompileOptions::default();
        let r = s.compile_project_with_observer(&p, &options, &mut |event| {
            if event.phase == phase {
                options.cancellation.cancel();
            }
        });
        assert_eq!(r.status, ResultCategory::Cancelled);
        assert_eq!(s.function_type_stats(), before);
    }
}

#[test]
fn function_types_decodable_rejected_work_product_rebuilds_current_source() {
    let d = Directory::new();
    let options = CompileOptions {
        semantic_cache: Some(d.0.join("cache")),
        ..Default::default()
    };
    success(&CompilationSession::default().compile_project(&d.project(MAIN, LIBRARY), &options));
    let directory = d.0.join("cache/function-types-v1");
    let file = fs::read_dir(&directory)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let key = Fingerprint::from_hex(file.file_stem().unwrap().to_str().unwrap()).unwrap();
    let cache = ArtifactCache::with_limit(&directory, options.limits.max_source_bytes);
    let CacheRead::Candidate(mut bytes) = cache.inspect_candidate(key).unwrap() else {
        panic!("seed epoch");
    };
    let count = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let boundary = 12 + count * 80;
    let (mut module, candidate) =
        zil_ir::wire::decode_module_candidate(&bytes[boundary..], &options.limits).unwrap();
    // This still decodes and fits scalar metadata, but the actual current
    // Analyze/Verify must reject the nonexistent direct callee.
    let mut changed = false;
    for instruction in module
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .flat_map(|b| &mut b.instructions)
    {
        if let zil_ir::InstructionKind::Call { function, .. } = &mut instruction.kind {
            *function = zil_ir::FunctionId(9999);
            changed = true;
            break;
        }
    }
    assert!(changed);
    bytes.truncate(boundary);
    bytes.extend(
        zil_ir::wire::encode_module_candidate(&module, &candidate, &options.limits).unwrap(),
    );
    cache.write_candidate(key, &bytes).unwrap();
    let p = d.project(MAIN, &LIBRARY.replace("x+1", "x+123"));
    let mut s = CompilationSession::default();
    let r = s.compile_project(&p, &options);
    assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
    assert!(r.verification_runs >= 1);
    assert_eq!(s.function_type_stats().reused_functions, 0);
    compare_fresh(&s, &r, &p, &options);
}
