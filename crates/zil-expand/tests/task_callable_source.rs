use std::collections::BTreeMap;
use zil_base::{FileId, Limits, ModuleId, Phase, SourceUnit, TargetProfile};
use zil_expand::source_context::{DeclarationPlacement, SourceIdentityContext};

fn expand(text: &str) -> Result<zil_ir::CoreModule, Vec<zil_base::Diagnostic>> {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "task-callable-source.zil", text);
    let parsed = zil_syntax::parse(&source).into_result().unwrap();
    let aliases = BTreeMap::new();
    zil_expand::expand_with_map_semantics(
        &source,
        &parsed,
        &Limits::default(),
        &TargetProfile::default(),
        SourceIdentityContext::ClosedSnapshot {
            root: ModuleId(0),
            aliases: &aliases,
            declarations: DeclarationPlacement::Original,
        },
    )
    .map(|expanded| expanded.core)
}

#[test]
fn task_callable_sources_keep_original_context_and_poll_results() {
    for text in [
        include_str!(
            "../../../tests/language-readiness/task-owner-transport/returned-task-result-fnonce.zil"
        ),
        include_str!(
            "../../../tests/language-readiness/task-owner-transport/returned-second-task-fnonce.zil"
        ),
        include_str!(
            "../../../tests/language-readiness/task-owner-transport/fnmut-async-poll-close.zil"
        ),
    ] {
        let core = expand(text).unwrap_or_else(|errors| panic!("{text}: {errors:?}"));
        assert!(
            core.semantic.is_some(),
            "full original program must remain present"
        );
        assert!(core.unsupported.is_empty(), "{:?}", core.unsupported);
        assert!(!core.functions.is_empty());
    }
}

#[test]
fn scalar_observer_and_mutable_callable_check_the_original_body() {
    for text in [
        "extern \"C\" fn observed(code:i32);fn observe<F:FnOnce()->i32>(callback:F)->i32{let answer=callback();unsafe{observed(answer);}return answer;}fn main()->i32{let callback=move()=>42;return observe(callback);}",
        "fn run<F:FnMut()->i32>(original:F)->i32{var callback=original;let first=callback();return first+callback();}fn main()->i32{var n=0;let callback=move()=>{n+=1;return n;};return run(callback);}",
    ] {
        let core = expand(text).unwrap_or_else(|errors| panic!("{text}: {errors:?}"));
        assert!(core.semantic.is_some());
        assert!(core.unsupported.is_empty(), "{:?}", core.unsupported);
    }
    // These unused declarations must still be checked before specialization.
    for text in [
        "extern \"C\" fn observed(code:i32);fn observe<F:FnOnce()->i32>(callback:F)->i32{let answer=callback();observed(answer);return answer;}fn main(){}",
        "extern \"C\" fn observed(code:i32);fn observe<F:FnOnce()->i32>(callback:F)->i32{let answer=callback();unsafe{observed(true);}return answer;}fn main(){}",
        "fn run<F:FnMut()->i32>(original:F)->i32{let callback=original;return callback();}fn main(){}",
        "fn run<F:FnMut()->i32>(original:F)->i32{var first=original;var second=original;return first()+second();}fn main(){}",
    ] {
        let errors = expand(text).unwrap_err();
        assert!(
            errors.iter().any(|error| error.phase == Phase::Expand),
            "{text}: {errors:?}"
        );
    }
}

#[test]
fn returned_owned_task_signature_keeps_context_ports_and_creator_selection() {
    for (text, selected) in [
        (
            include_str!(
                "../../../tests/language-readiness/task-owner-transport/returned-task-result-fnonce.zil"
            ),
            0,
        ),
        (
            include_str!(
                "../../../tests/language-readiness/task-owner-transport/returned-second-task-fnonce.zil"
            ),
            1,
        ),
    ] {
        let core = expand(text).unwrap_or_else(|errors| panic!("{text}: {errors:?}"));
        let function = core
            .functions
            .iter()
            .find(|function| function.name == "keep")
            .unwrap();
        let returned = function.callable_return.as_ref().unwrap();
        let zil_ir::Type::Managed(payload) = &returned.result else {
            panic!("owning Task result lost its carrier: {:?}", returned.result);
        };
        let zil_ir::Type::Task {
            result,
            retained_life,
        } = payload.as_ref()
        else {
            panic!("owning result lost its complete Task type: {payload:?}");
        };
        assert_eq!(result.as_ref(), &zil_ir::Type::I32);
        assert_eq!(retained_life.binder_count, 1);
        assert_eq!(retained_life.ports.len(), 1);
        assert_eq!(retained_life.ports[0].binder, 0);
        assert_eq!(
            retained_life.ports[0].mode,
            zil_ir::StoredBorrowMode::ExclusiveAddress
        );
        assert_eq!(
            retained_life.ports[0].leaf,
            zil_ir::Type::Address {
                mutable: true,
                inner: Box::new(zil_ir::Type::I32)
            }
        );
        assert_eq!(
            retained_life.retained,
            [zil_ir::StorageRetainedBinding {
                path: vec![zil_ir::LifeTypeProjection::Parameter(0)],
                class: 0,
                binder: 0,
            }]
        );
        assert!(retained_life.nominal_arguments.is_empty());
        let original = returned.source_life.as_ref().unwrap();
        assert_eq!(original.fixed.len(), 1);
        assert_eq!(original.fixed[0].input.parameter, selected);
        let life = returned.life.as_ref().unwrap();
        assert_eq!(life.fixed_binder_count, 1);
        assert_eq!(life.invocation_binder_count, 0);
        assert_eq!(
            life.classes,
            [zil_ir::SignatureLifeClassBinding {
                position: zil_ir::SignatureLifeClass {
                    base: zil_ir::StorageBase::Result,
                    path: vec![zil_ir::LifeTypeProjection::Pointee],
                    class: 0,
                },
                binder: zil_ir::StorageBinder::Fixed(0),
            }]
        );
        assert_eq!(
            life.bindings,
            [zil_ir::StorageLifeBinding {
                position: zil_ir::StoragePosition {
                    base: zil_ir::StorageBase::Result,
                    path: vec![zil_ir::ContractProjection::RetainedPort(0)],
                },
                binder: zil_ir::StorageBinder::Fixed(0),
            }]
        );
    }
}
