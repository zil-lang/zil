use super::*;
use std::{fs, path::Path, process::Command};

#[test]
fn inline_result_retains_pending_progress_and_selects_the_actual_arm() {
    let mark = Type::Record { name: "Mark".into(), fields: vec![("code".into(), Type::I32)] };
    let owner = Type::Managed(Box::new(mark.clone()));
    let child = Type::Managed(Box::new(Type::TaskI32));
    let result = Type::ErrorUnion {
        error: Box::new(owner.clone()),
        result: Box::new(Type::Tuple(vec![
            owner.clone(),
            child.clone(),
            owner.clone(),
        ])),
    };
    let mut actual = layout::test_environments(vec![vec![Type::Package {
        name: "Captures".into(),
        fields: vec![("before".into(), owner.clone()), ("child".into(), child), ("after".into(), owner)],
    }]]);
    actual.resources = vec![
        ResourceDefinition { name: "Mark".into(), payload: mark, teardown: Some(FunctionId(0)) },
        ResourceDefinition { name: "@builtin:Task<i32>".into(), payload: Type::TaskI32, teardown: None },
    ];
    let limits = zil_base::Limits::default();
    let layouts = LayoutContext::for_test_module(&actual, &limits).unwrap();
    let mut items = owners::helpers(layouts.meter(), false).unwrap();
    let mut closures = vec![];
    items.extend(functions(&layouts, &actual, &result, "test.inline", false, &mut closures, layouts.meter()).unwrap());
    let id = ClosureId(0);
    let m::Ty::Named(name) = environment_type(id) else { unreachable!() };
    items.push(m::Item::NamedType(name, layouts.environment_fields(id).unwrap()));
    items.extend(owner_groups::functions(&layouts, &actual, id, &mut closures, layouts.meter()).unwrap());
    assert!(closures.is_empty());
    for item in &mut items {
        if let m::Item::Function(function) = item {
            function.linkage = m::Linkage::External;
        }
    }
    for (name, result) in [
        ("zil.fn.0", m::Ty::Void),
        ("zil.task.request_cancel", m::Ty::Void),
        ("zil.task.poll_close", m::Ty::Int(1)),
        ("zil.task.finish_destroy", m::Ty::Void),
    ] {
        items.push(m::Item::Function(m::Function {
            name: name.to_owned(),
            linkage: m::Linkage::External,
            signature: m::Signature::new(result, vec![m::Param::new(m::Ty::Ptr, "value")]),
            attrs: vec![],
            blocks: None,
        }));
    }
    let module = m::Module { triple: actual.target.triple.clone(), fingerprint: String::new(), items };
    let directory = std::env::temp_dir().join(format!("zil-inline-task-drop-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let text = directory.join("cleanup.ll");
    fs::write(&text, module.to_text()).unwrap();
    let observer = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/language-readiness/task-aggregate-results/inline-drop-observer.c");
    let clang = std::env::var_os("ZIL_CLANG").unwrap_or_else(|| "clang".into());
    for optimized in [false, true] {
        let level = if optimized { "-O2" } else { "-O0" };
        let objects = vec![text.clone()];
        #[cfg(feature = "llvm-api")]
        let objects = {
            let mut objects = objects;
            let object = zil_llvm_api::compile_module(
                &module,
                &zil_llvm_api::Config {
                    triple: &module.triple,
                    optimization: if optimized {
                        zil_llvm_api::Optimization::O2
                    } else {
                        zil_llvm_api::Optimization::O0
                    },
                },
                &|| false,
            )
            .unwrap();
            assert_eq!(object.receipt.input_kind, "builder");
            assert!(object.receipt.verify_before_after);
            let path = directory.join("cleanup.o");
            fs::write(&path, object.bytes).unwrap();
            objects.push(path);
            objects
        };
        for object in objects {
            let executable = directory.join("observe");
            let output = Command::new(&clang)
                .arg(level).arg(&object).arg(&observer).arg("-o").arg(&executable)
                .output().unwrap();
            assert!(output.status.success(), "{object:?} {level}: {output:?}");
            for mode in ["normal", "cancel"] {
                let output = Command::new(&executable).arg(mode).output().unwrap();
                assert!(output.status.success(), "{object:?} {level} {mode}: {output:?}");
                assert_eq!(output.stdout, b"ATBXTB");
                assert!(output.stderr.is_empty());
            }
        }
    }
    fs::remove_dir_all(directory).unwrap();
}
