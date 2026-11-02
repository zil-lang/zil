//! Actual three-module source produces public Parameter(k)/Returned claims;
//! those same claims must survive independent raw and Wire verification.
use std::{fs, path::PathBuf};
use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::{CallableCleanup, CallableKind, CallableReturnSource, CoreImport, CoreProgram};

#[test]
fn actual_parameter_indices_and_declared_modes_survive_raw_and_wire() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/impl256/extended");
    let sources = ["main", "model", "relay"]
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let path = root.join(format!("{name}.zil"));
            SourceUnit::new(
                FileId(i as u32),
                ModuleId(i as u32),
                path.to_string_lossy().into_owned(),
                fs::read_to_string(path).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let syntax = sources
        .iter()
        .map(|s| zil_syntax::parse(s).into_result().unwrap())
        .collect::<Vec<_>>();
    let import = |alias: &str, index| CoreImport {
        alias: alias.into(),
        module: ModuleId(index),
        span: zil_base::Span::default(),
    };
    let limits = Limits::default();
    let expanded = zil_expand::expand_program_with_limits(
        &sources,
        &syntax.iter().collect::<Vec<_>>(),
        &[
            vec![import("model", 1), import("relay", 2)],
            vec![],
            vec![import("model", 1)],
        ],
        &limits,
    )
    .unwrap();
    let modules = expanded
        .into_iter()
        .map(|(module, _)| module)
        .collect::<Vec<_>>();
    let program = CoreProgram {
        root: ModuleId(0),
        source_fingerprint: modules[0].core.source_fingerprint,
        modules,
    };
    let target = TargetProfile::host_llvm();
    let typed = zil_types::check_program(&program, &target, &limits).unwrap();
    let module = typed.module;
    let out = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("consumer256-contracts-{}", std::process::id()));
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("typed.txt"), format!("{module:#?}")).unwrap();
    for (name, index, kind) in [
        ("identity", 0, CallableKind::FnOnce),
        ("store_first", 1, CallableKind::Fn),
        ("union_last", 2, CallableKind::FnOnce),
        ("address", 1, CallableKind::FnMut),
        ("slice", 1, CallableKind::Fn),
        ("spin", 1, CallableKind::FnOnce),
        ("tick", 1, CallableKind::FnMut),
        ("later", 1, CallableKind::FnOnce),
    ] {
        let functions = module
            .functions
            .iter()
            .filter(|f| f.name.contains(name) && f.callable_return_contract.is_some())
            .collect::<Vec<_>>();
        assert!(
            !functions.is_empty(),
            "missing real instantiated helper {name}"
        );
        for function in functions {
            let result = function.callable_return_contract.as_ref().unwrap();
            assert_eq!(
                result.source,
                CallableReturnSource::Parameter(index),
                "{}",
                function.name
            );
            assert_eq!(result.kind, kind, "{}", function.name);
            let roles = &function.callable_contract.as_ref().unwrap().roles;
            assert_eq!(roles.len(), 1);
            assert_eq!(roles[0].parameter, index);
            assert_eq!(roles[0].kind, kind);
            assert_eq!(roles[0].cleanup, CallableCleanup::Returned);
        }
    }
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    fs::write(out.join("candidate.wire"), &wire).unwrap();
    zil_verify::verify_module(module, plan, &target, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
}
