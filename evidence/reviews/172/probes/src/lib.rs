#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        fs,
        path::PathBuf,
        process::Command,
        sync::atomic::{AtomicUsize, Ordering},
    };
    use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
    use zil_driver::{CompileOptions, CompileReport, Optimization, ProjectSources, Stage};
    use zil_ir::{CandidatePlan, InstructionKind as I, Type, TypedModule};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    fn directory() -> PathBuf {
        let base = PathBuf::from(std::env::var_os("ZIL_REVIEW_REPORT_DIR").unwrap());
        let dir = base.join(format!(
            "probe-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn project(source: &str) -> ProjectSources {
        ProjectSources {
            root: ModuleId(0),
            modules: vec![SourceUnit::new(FileId(0), ModuleId(0), "main.zil", source)],
            paths: BTreeMap::new(),
        }
    }

    fn check(source: &str) -> CompileReport {
        let dir = directory();
        fs::write(dir.join("main.zil"), source).unwrap();
        let report = zil_driver::compile_project(
            &project(source),
            &CompileOptions {
                emit_dir: Some(dir.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(dir.join("compile.json"), report.to_json()).unwrap();
        report
    }

    fn native(source: &str, expected: &[u8], fault_probe: bool) {
        for opt in [Optimization::O0, Optimization::O2] {
            let dir = directory();
            fs::write(dir.join("main.zil"), source).unwrap();
            let program = dir.join("program");
            let phases = dir.join("phases");
            let report = zil_driver::compile_project(
                &project(source),
                &CompileOptions {
                    stop_after: Stage::Build,
                    optimization: opt,
                    output: Some(program.clone()),
                    emit_dir: Some(phases.clone()),
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
            let output = Command::new(&program).output().unwrap();
            fs::write(dir.join("stdout"), &output.stdout).unwrap();
            fs::write(dir.join("stderr"), &output.stderr).unwrap();
            fs::write(
                dir.join("native.json"),
                format!(
                    "{{\"opt\":\"{opt:?}\",\"exit\":{:?},\"stdout\":{:?}}}\n",
                    output.status.code().unwrap(),
                    String::from_utf8_lossy(&output.stdout)
                ),
            )
            .unwrap();
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert_eq!(output.stdout, expected);
            assert!(output.stderr.is_empty(), "{output:?}");
            if fault_probe {
                let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..");
                let binary = dir.join("allocation-probe");
                let clang = std::env::var("ZIL_CLANG").unwrap();
                let option = if opt == Optimization::O0 {
                    "-O0"
                } else {
                    "-O2"
                };
                let linked = Command::new(&clang)
                    .args([option])
                    .arg(phases.join("llvm.ll"))
                    .arg(repo.join("tools/research/142/allocator_probe.c"))
                    .args(["-Wl,--wrap=realloc", "-Wl,--wrap=free", "-o"])
                    .arg(&binary)
                    .output()
                    .unwrap();
                fs::write(dir.join("probe-link.stderr"), &linked.stderr).unwrap();
                assert!(linked.status.success(), "{linked:?}");
                for fail_at in [0, 2] {
                    let observed = Command::new(&binary)
                        .env("ZIL_TEST_FAIL_AT", fail_at.to_string())
                        .output()
                        .unwrap();
                    fs::write(
                        dir.join(format!("allocation-{fail_at}.stdout")),
                        &observed.stdout,
                    )
                    .unwrap();
                    fs::write(
                        dir.join(format!("allocation-{fail_at}.json")),
                        &observed.stderr,
                    )
                    .unwrap();
                    assert_eq!(observed.status.code(), Some(0), "{observed:?}");
                    assert_eq!(observed.stdout, expected);
                    let counts = String::from_utf8(observed.stderr).unwrap();
                    for needle in [
                        "\"alloc_calls\":2",
                        "\"allocated\":1",
                        "\"frees\":1",
                        "\"live\":0",
                    ] {
                        assert!(counts.contains(needle), "{counts}");
                    }
                    assert!(
                        counts.contains(&format!("\"failures\":{}", usize::from(fail_at != 0))),
                        "{counts}"
                    );
                }
            }
        }
    }

    #[test]
    fn mixed_alignment_full_error_and_failed_reallocation_preserve_prefix() {
        native(
            r#"
extern "C" fn putchar(c:i32)->i32;
resource Mark{code:i32}
impl Drop for Mark{fn drop(self:Self){unsafe{putchar(.code)}}}
package Wide{lead:u8;manages first:Mark;large:i64;small:u16;items:[i32;3];manages last:Mark}
fn make(a:i32,b:i32,n:i32)->Wide=>Wide{lead:7,first:Mark{code:a},large:4294967297,small:513,items:[n,n+1,n+2],last:Mark{code:b}}
fn empty()->Store<Wide> contract{result(create)}{return store_new()}
fn main()->void!i32{
 let s=empty();if !store_reserve(s,2){return ()};
 store_push(s,make(65,97,10));store_push(s,make(66,98,30));
 store_push(s,make(67,99,40));
 let grew=store_reserve(s,8);
 if store_len(s)!=2||(grew&&store_capacity(s)!=8)||(!grew&&store_capacity(s)!=2){return 4};
 let p=store_pop(s)?;
 if p.lead!=7||p.large!=4294967297||p.small!=513||p.items[0]!=30||p.items[1]!=31{return 5};
 return p.items[2]-32
}
"#,
            b"CcBbAa",
            true,
        );
    }

    #[test]
    fn zero_sized_managed_field_with_nonzero_copy_payload_keeps_real_drop_count() {
        native(
            r#"
extern "C" fn putchar(c:i32)->i32;
resource Empty{}
impl Drop for Empty{fn drop(self:Self){unsafe{putchar(90)}}}
package Item{manages marker:Empty;tag:u8}
fn main()->void!i32{let s:Store<Item>=store_new();if !store_reserve(s,2){return ()};store_push(s,Item{marker:Empty{},tag:7});store_push(s,Item{marker:Empty{},tag:9});let item=store_pop(s)?;return i32(item.tag)-9}
"#,
            b"ZZ",
            false,
        );
    }

    const SIMPLE: &str = "resource R{code:i32}package P{manages a:R;manages b:R}fn empty()->Store<P> contract{result(create)}{return store_new()}fn item()->P=>P{a:R{code:1},b:R{code:2}};";

    #[test]
    fn rhs_transfer_cannot_invalidate_prepared_store_descriptor() {
        let source = format!(
            "{SIMPLE}fn consume(give s:Store<P>)->P{{return item()}}fn main(){{let s=empty();store_push(s,consume(give s));}}"
        );
        let report = check(&source);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert!(
            report.diagnostics.iter().any(|d| d.phase == Phase::Analyze
                && d.code == "E-ANALYZE-IR"
                && d.message.contains("description required later")),
            "{}",
            report.to_json()
        );
        assert!(report.commands.is_empty());
    }

    #[test]
    fn partially_moved_package_cannot_be_published_as_complete_element() {
        let source = format!(
            "{SIMPLE}fn main(){{let s=empty();let p=item();let a=give p.a;store_push(s,p);}}"
        );
        let report = check(&source);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert!(
            report.diagnostics.iter().any(|d| d.phase == Phase::Analyze
                && d.code == "E-ANALYZE-IR"
                && d.message.contains("whole aggregate move")),
            "{}",
            report.to_json()
        );
        assert!(report.commands.is_empty());
    }

    fn candidate() -> (TypedModule, CandidatePlan) {
        let text = format!("{SIMPLE}fn main(){{let s=empty();store_len(s);}}");
        let source = SourceUnit::new(FileId(0), ModuleId(0), "raw.zil", text);
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let core = zil_expand::expand(&source, &syntax).unwrap();
        let target = TargetProfile::host_llvm();
        let limits = Limits::default();
        let module = zil_types::check(&core, &target, &limits).unwrap();
        let plan = zil_analyze::analyze(&module, &limits).unwrap();
        zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
        (module, plan)
    }

    fn reject(
        label: &str,
        module: TypedModule,
        mut plan: CandidatePlan,
        target: &TargetProfile,
        code: &str,
        needle: &str,
    ) {
        let limits = Limits::default();
        plan.module_fingerprint = module.fingerprint();
        let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
        let dir = directory();
        fs::write(dir.join("label.txt"), label).unwrap();
        fs::write(dir.join("candidate.bin"), &bytes).unwrap();
        for (route, outcome) in [
            (
                "raw",
                zil_verify::verify_module(module, plan, target, &limits),
            ),
            (
                "wire",
                zil_verify::verify_module_bytes(&bytes, target, &limits),
            ),
        ] {
            let errors = outcome.expect_err(label);
            fs::write(dir.join(format!("{route}.txt")), format!("{errors:#?}\n")).unwrap();
            assert!(
                errors.iter().all(|d| d.phase == Phase::Verify),
                "{label}/{route}: {errors:?}"
            );
            assert!(
                errors
                    .iter()
                    .any(|d| d.code == code && d.message.contains(needle)),
                "{label}/{route}: {errors:?}"
            );
        }
    }

    #[test]
    fn raw_wire_reconstruct_stale_descriptors_definition_presence_and_target_gate() {
        let (original, plan) = candidate();
        let mut stale = original.clone();
        let main = stale
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
                    .any(|i| matches!(i.kind, I::OwnedStoreLen { .. }))
            })
            .unwrap();
        let len = block
            .instructions
            .iter()
            .position(|i| matches!(i.kind, I::OwnedStoreLen { .. }))
            .unwrap();
        let drop = block
            .instructions
            .iter()
            .position(|i| matches!(i.kind, I::DropManaged { .. }))
            .unwrap();
        assert!(drop > len);
        let instruction = block.instructions.remove(drop);
        block.instructions.insert(len, instruction);
        reject(
            "stale-store-desc-after-drop",
            stale,
            plan.clone(),
            &TargetProfile::host_llvm(),
            "E-VERIFY-RESOURCE",
            "invalidate a live description",
        );
        let mut missing = original.clone();
        missing
            .resources
            .retain(|r| !matches!(r.payload, Type::OwnedStore { .. }));
        reject(
            "missing-sealed-store-definition",
            missing,
            plan.clone(),
            &TargetProfile::host_llvm(),
            "E-VERIFY-OWNED-STORE",
            "lacks its canonical sealed definition",
        );
        let mut windows = original;
        let target = TargetProfile::windows_x64_llvm();
        windows.target = target.clone();
        reject(
            "store-nonlinux-target",
            windows,
            plan,
            &target,
            "E-UNSUPPORTED",
            "requires checked Linux x64 LLVM",
        );
    }
}
