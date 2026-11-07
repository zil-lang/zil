//! Independent source cache/invalidation control, included by the existing borrow target.
use super::*;

#[test]
fn disk_cache_live_disk_bounds_and_actual_roots_stay_distinct() {
    let temp = Temp::new();
    let main = include_str!("project/main.zil");
    let api = include_str!("project/api.zil");
    let project = |api: &str| zil_driver::ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(FileId(0), ModuleId(0), "main.zil", main),
            SourceUnit::new(FileId(1), ModuleId(1), "api.zil", api),
        ],
        paths: std::collections::BTreeMap::from([(vec!["api".into()], ModuleId(1))]),
    };
    let options = CompileOptions {
        semantic_cache: Some(temp.0.join("disk-cache")),
        ..CompileOptions::default()
    };
    let mut session = zil_driver::CompilationSession::default();
    let cold = session.compile_project(&project(api), &options);
    assert_eq!(cold.status, ResultCategory::Success, "{}", cold.to_json());
    let narrow = cold
        .scalar_interfaces
        .iter()
        .find(|interface| {
            interface
                .exports
                .iter()
                .any(|export| export.reference_return_bound.is_some())
        })
        .unwrap();
    assert!(
        narrow
            .exports
            .iter()
            .any(|export| export.reference_return_bound == Some(vec![0]))
    );
    for (disk, report) in [
        (false, session.compile_project(&project(api), &options)),
        (true, zil_driver::compile_project(&project(api), &options)),
    ] {
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        assert_eq!(report.scalar_interfaces, cold.scalar_interfaces);
        if disk {
            assert_eq!(report.disk_cache, zil_driver::DiskCacheStatus::Hit);
        }
    }
    // A wider declaration leaves the real return source unchanged; the caller
    // still modifies the second object while holding a view of the first.
    let wide_api = api.replace("other:&'b T", "other:&'a T");
    assert_eq!(wide_api.len(), api.len());
    for report in [
        session.compile_project(&project(&wide_api), &options),
        zil_driver::compile_project(&project(&wide_api), &options),
    ] {
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let wide = report
            .scalar_interfaces
            .iter()
            .find(|interface| {
                interface
                    .exports
                    .iter()
                    .any(|export| export.reference_return_bound.is_some())
            })
            .unwrap();
        assert!(
            wide.exports
                .iter()
                .any(|export| export.reference_return_bound == Some(vec![0, 1]))
        );
        assert_ne!(narrow.signature_fingerprint, wide.signature_fingerprint);
        assert_eq!(narrow.result_origins, wide.result_origins);
        assert_ne!(narrow.contract_fingerprint, wide.contract_fingerprint);
    }
    // Equal source length cannot retain a candidate for a body returning the
    // second object under the original first-only declaration.
    let wrong_api = api.replace("return value", "return other");
    assert_eq!(wrong_api.len(), api.len());
    for report in [
        session.compile_project(&project(&wrong_api), &options),
        zil_driver::compile_project(&project(&wrong_api), &options),
    ] {
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand);
        assert_eq!(report.diagnostics[0].code, "E-LIFE-ORIGIN");
        assert_eq!(report.verification_runs, 0);
    }
}
