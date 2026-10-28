//! Source admission is separate from resource proof and native execution.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, Stage, compile_source};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const INPUTS: &[(&str, &str)] = &[
    (
        "original120",
        include_str!("impl206_fixtures/00_original120.zil"),
    ),
    (
        "child-only",
        include_str!("impl206_fixtures/00_child_only.zil"),
    ),
    (
        "mixed-created",
        include_str!("impl206_fixtures/01_mixed_child_created.zil"),
    ),
    (
        "mixed-incoming",
        include_str!("impl206_fixtures/01_mixed_child_incoming.zil"),
    ),
    (
        "whole-created",
        include_str!("impl206_fixtures/02_whole_transfer_twice_created.zil"),
    ),
    (
        "whole-incoming",
        include_str!("impl206_fixtures/02_whole_transfer_twice_incoming.zil"),
    ),
    (
        "wrappers-created",
        include_str!("impl206_fixtures/03_child_only_wrappers_created.zil"),
    ),
    (
        "wrappers-incoming",
        include_str!("impl206_fixtures/03_child_only_wrappers_incoming.zil"),
    ),
    (
        "ordinary-calls",
        include_str!("impl206_fixtures/04_unknown_ordinary_calls_do_not_create.zil"),
    ),
];

fn directory(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = root.join(format!(
        "product206-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn original_sources_reach_types_without_claiming_freshness_or_execution() {
    for (label, text) in INPUTS {
        let dir = directory(label);
        fs::write(dir.join("source.zil"), text).unwrap();
        let source = SourceUnit::new(FileId(0), ModuleId(0), "product206.zil", *text);
        let report = compile_source(
            &source,
            &CompileOptions {
                stop_after: Stage::Types,
                emit_dir: Some(dir.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(dir.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
        assert!(!report.artifacts.contains_key("executable"));
    }
}

#[test]
fn both_public_types_apis_check_unused_complete_product_metadata() {
    use zil_base::{Limits, TargetProfile};
    use zil_ir::{CoreProgram, ProgramModule, Type};
    let text = "resource R{n:i32}package P{manages leaf:R}package Outer{child:P}fn unused(p:Outer){}fn main(){}";
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "unused206.zil", text);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let resolved = zil_types::check(&core, &target, &limits).unwrap();
    let unused = resolved
        .functions
        .iter()
        .find(|f| f.name == "unused")
        .unwrap();
    let product = unused.locals[unused.parameters[0].0 as usize].ty.clone();
    for label in [
        "valid",
        "empty-parent-name",
        "empty-child-name",
        "duplicate-child-field",
        "child-without-managed",
    ] {
        let mut core = core.clone();
        let mut actual = product.clone();
        let Type::Package { name, fields } = &mut actual else {
            panic!("Product")
        };
        if label == "empty-parent-name" {
            name.clear();
        }
        let Type::Package { name, fields } = &mut fields[0].1 else {
            panic!("child")
        };
        match label {
            "empty-child-name" => name.clear(),
            "duplicate-child-field" => fields.push(fields[0].clone()),
            "child-without-managed" => fields[0].1 = Type::I32,
            _ => {}
        }
        core.functions
            .iter_mut()
            .find(|f| f.name == "unused")
            .unwrap()
            .parameters[0]
            .ty = actual;
        let program = CoreProgram {
            root: core.module_id,
            source_fingerprint: core.source_fingerprint,
            modules: vec![ProgramModule {
                imports: vec![],
                exports: core
                    .functions
                    .iter()
                    .filter(|f| f.name == "main")
                    .map(|f| f.id)
                    .collect(),
                core: core.clone(),
            }],
        };
        let dir = directory(&format!("metadata-{label}"));
        fs::write(dir.join("source.zil"), text).unwrap();
        fs::write(dir.join("core.txt"), format!("{core:#?}")).unwrap();
        let single = zil_types::check(&core, &target, &limits);
        let program = zil_types::check_program(&program, &target, &limits).map(|p| p.module);
        fs::write(dir.join("single.txt"), format!("{single:#?}")).unwrap();
        fs::write(dir.join("program.txt"), format!("{program:#?}")).unwrap();
        for result in [single, program] {
            if label == "valid" {
                result.unwrap();
            } else {
                let errors = result.expect_err(label);
                assert!(
                    errors.iter().all(|d| d.phase == zil_base::Phase::Types
                        && matches!(
                            d.category,
                            ResultCategory::SafeReject | ResultCategory::Unsupported
                        )),
                    "{label}: {errors:#?}"
                );
            }
        }
    }
}
