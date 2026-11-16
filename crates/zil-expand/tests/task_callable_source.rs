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

#[test]
fn qualified_task_workers_keep_both_inputs_and_original_life_actuals() {
    for source in [
        include_str!("task_worker_actual_fixtures/equal-shared-formal-positive.zil"),
        include_str!("task_worker_actual_fixtures/returned-shared-formal-positive.zil"),
    ] {
        for text in [
            source.to_owned(),
            source
                .replace(
                    "fn worker<life a>(input:&'a mut i32)",
                    "fn worker<life worker_life,life phantom>(input:&'worker_life mut i32)",
                )
                .replace("worker::<'a>", "worker::<'a,'a>"),
        ] {
            let core = expand(&text).unwrap_or_else(|errors| panic!("{text}: {errors:?}"));
            assert!(core.unsupported.is_empty(), "{:?}", core.unsupported);
            let keep = core
                .functions
                .iter()
                .find(|function| function.name == "keep")
                .unwrap();
            assert_eq!(keep.parameters.len(), 2);
            for parameter in &keep.parameters {
                assert_eq!(parameter.mode, zil_ir::PassingMode::Give);
                // Core retains the resource observation type and the separate
                // Give mode; the owning carrier is formed by the next stage.
                let zil_ir::Type::Desc(ty) = &parameter.ty else {
                    panic!(
                        "given Task input lost its resource header: {:?}",
                        parameter.ty
                    )
                };
                let zil_ir::Type::Task {
                    result,
                    retained_life,
                } = ty.as_ref()
                else {
                    panic!("complete Task input was lost: {ty:?}")
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
                        inner: Box::new(zil_ir::Type::I32),
                    }
                );
            }
            let pair = core
                .functions
                .iter()
                .find(|function| function.name == "pair")
                .unwrap();
            let mut physical_workers = 0;
            for statement in &pair.body.statements {
                let zil_ir::CoreStatementKind::Binding {
                    initializer: Some(value),
                    ..
                } = &statement.kind
                else {
                    continue;
                };
                let zil_ir::CoreExprKind::Try(call) = &value.kind else {
                    continue;
                };
                let zil_ir::CoreExprKind::Call { callee, arguments } = &call.kind else {
                    panic!("{call:?}")
                };
                assert!(
                    matches!(&callee.kind, zil_ir::CoreExprKind::Name(name) if name == "task_create")
                );
                let [worker, _] = arguments.as_slice() else {
                    panic!()
                };
                assert!(
                    matches!(&worker.kind, zil_ir::CoreExprKind::Name(name) if name == "worker"),
                    "qualified worker must be an actual code value, never a later-capability placeholder: {worker:?}"
                );
                physical_workers += 1;
            }
            assert_eq!(physical_workers, 2);
            // Inspect the frozen source operations, rather than producer names
            // of subsequently specialized workers. Both actual input names and
            // every written Life ordinal, including phantom, must survive.
            let mut inputs = Vec::new();
            for definition in &core.semantic.as_ref().unwrap().definitions {
                let zil_ir::SemanticDefinitionBody::FunctionWithBody {
                    parameters,
                    body: zil_ir::SemanticOriginalFunctionBody::Defined(body),
                    ..
                } = &definition.body
                else {
                    continue;
                };
                if parameters
                    .first()
                    .is_none_or(|parameter| parameter.name != "first")
                {
                    continue;
                }
                for statement in &body.statements {
                    let zil_ir::CoreStatementKind::Binding {
                        initializer: Some(value),
                        ..
                    } = &statement.kind
                    else {
                        continue;
                    };
                    let zil_ir::CoreExprKind::Try(call) = &value.kind else {
                        continue;
                    };
                    let zil_ir::CoreExprKind::Call { callee, arguments } = &call.kind else {
                        panic!()
                    };
                    assert!(
                        matches!(&callee.kind, zil_ir::CoreExprKind::Name(name) if name == "task_create")
                    );
                    let [worker, input] = arguments.as_slice() else {
                        panic!()
                    };
                    let zil_ir::CoreExprKind::GenericApply { callee, arguments } = &worker.kind
                    else {
                        panic!("written worker application disappeared: {worker:?}")
                    };
                    assert!(
                        matches!(&callee.kind, zil_ir::CoreExprKind::Name(name) if name == "worker")
                    );
                    let expected = if text.contains("life phantom") { 2 } else { 1 };
                    assert_eq!(arguments.len(), expected);
                    assert!(arguments.iter().all(|argument| matches!(argument,
                        zil_ir::CoreGenericArgument::Lifetime(name) if name == "a")));
                    let zil_ir::CoreExprKind::Name(input) = &input.kind else {
                        panic!()
                    };
                    inputs.push(input.as_str());
                }
            }
            assert_eq!(inputs.len(), 2);
            assert_eq!(inputs[0], "first");
            assert!(matches!(inputs[1], "second" | "second_input"));
        }
    }
}

#[test]
fn qualified_task_worker_cannot_equate_distinct_caller_classes() {
    for source in [
        include_str!("task_worker_actual_fixtures/equal-shared-formal-positive.zil"),
        include_str!("task_worker_actual_fixtures/returned-shared-formal-positive.zil"),
    ] {
        let text = source
            .replace("fn pair<life a>(", "fn pair<life a,life b>(")
            .replace("second:&'a mut i32", "second:&'b mut i32")
            .replace("second_input:&'a mut i32", "second_input:&'b mut i32");
        let errors = expand(&text).expect_err("written worker a cannot absorb caller b");
        assert!(
            errors.iter().any(|error| error.phase == Phase::Expand
                && error.category == zil_base::ResultCategory::SafeReject),
            "{errors:?}"
        );
        assert!(
            !errors
                .iter()
                .any(|error| error.category == zil_base::ResultCategory::Unsupported),
            "{errors:?}"
        );
    }
}

#[test]
fn pending_once_returns_the_selected_owned_task_with_empty_context() {
    let text = include_str!(
        "../../../tests/language-readiness/task-owner-transport/once-pending-returned-task-cleanup.zil"
    );
    let core = expand(text).unwrap_or_else(|errors| panic!("{errors:?}"));
    assert!(core.unsupported.is_empty(), "{:?}", core.unsupported);
    assert!(core.semantic.is_some());
    let keep = core
        .functions
        .iter()
        .find(|function| function.name == "keep")
        .unwrap();
    for parameter in &keep.parameters[..2] {
        assert_eq!(parameter.mode, zil_ir::PassingMode::Give);
        let zil_ir::Type::Desc(payload) = &parameter.ty else {
            panic!("{:?}", parameter.ty)
        };
        let zil_ir::Type::Task {
            result,
            retained_life,
        } = payload.as_ref()
        else {
            panic!("{payload:?}")
        };
        assert_eq!(result.as_ref(), &zil_ir::Type::I32);
        assert_eq!(retained_life.binder_count, 0);
        assert!(retained_life.ports.is_empty());
        assert!(retained_life.retained.is_empty());
        assert!(retained_life.nominal_arguments.is_empty());
    }
    let result = &keep.callable_return.as_ref().unwrap().result;
    let zil_ir::Type::Managed(payload) = result else {
        panic!("{result:?}")
    };
    assert_eq!(
        payload.as_ref(),
        match &keep.parameters[1].ty {
            zil_ir::Type::Desc(payload) => payload.as_ref(),
            _ => unreachable!(),
        }
    );
    let zil_ir::CoreStatementKind::Return(Some(closure)) =
        &keep.body.statements.last().unwrap().kind
    else {
        panic!()
    };
    let zil_ir::CoreExprKind::Closure {
        move_capture, body, ..
    } = &closure.kind
    else {
        panic!("{closure:?}")
    };
    assert!(*move_capture);
    let zil_ir::CoreStatementKind::Return(Some(value)) = &body.statements.last().unwrap().kind
    else {
        panic!()
    };
    assert!(
        matches!(&value.kind, zil_ir::CoreExprKind::Unary { op: zil_ir::UnaryOp::Give, operand }
        if matches!(&operand.kind, zil_ir::CoreExprKind::Name(name) if name == "selected"))
    );
}

#[test]
fn task_give_preserves_owner_transfer_and_rejects_descriptors_or_repeated_moves() {
    let valid = "fn relay<F:Fn()->i32>(f:&F,give selected:Task<i32>)->Task<i32> contract{result(give)}{let moved=give selected;return give moved;}fn main(){}";
    let core = expand(valid).unwrap_or_else(|errors| panic!("{errors:?}"));
    assert!(core.unsupported.is_empty(), "{:?}", core.unsupported);
    for invalid in [
        valid.replace("give selected:Task<i32>", "selected:Task<i32>"),
        valid.replace(
            "return give moved;",
            "let duplicate=give moved;return give moved;",
        ),
    ] {
        let errors = expand(&invalid).unwrap_err();
        assert!(
            errors.iter().any(|error| error.phase == Phase::Expand
                && error.category == zil_base::ResultCategory::SafeReject
                && matches!(
                    error.message.as_str(),
                    "give requires an actual owned resource; descriptions do not own"
                        | "owned resource used after give"
                )),
            "{errors:?}"
        );
        assert!(
            !errors
                .iter()
                .any(|error| error.category == zil_base::ResultCategory::Unsupported),
            "{errors:?}"
        );
    }
}

#[test]
fn callback_task_values_transfer_through_names_and_nested_tuple_patterns() {
    for body in [
        "let answer=callback();let moved=answer;return give moved;",
        "let (answer,(marker,))=(callback(),(17,));let moved=give answer;return give moved;",
        "let values=(callback(),17);let (answer,marker)=values;return give answer;",
        "let answer=callback();let values=(give answer,17);let (moved,marker)=values;return give moved;",
    ] {
        let text = format!(
            "fn relay<F:FnOnce()->Managed<Task<i32>> + ClosedEnvironment>(callback:F)->Task<i32> contract{{result(give)}}{{{body}}}fn main(){{}}"
        );
        let core = expand(&text).unwrap_or_else(|errors| panic!("{text}: {errors:?}"));
        assert!(core.semantic.is_some());
        assert!(core.unsupported.is_empty(), "{:?}", core.unsupported);
    }
}

#[test]
fn callback_task_values_do_not_duplicate_or_change_the_complete_payload() {
    let prefix = "fn relay<F:FnOnce()->Managed<Task<i32>> + ClosedEnvironment>(callback:F)->Task<i32> contract{result(give)}{";
    for body in [
        "let answer=callback();let moved=answer;return give answer;",
        "let (answer,marker)=(callback(),17);let moved=give answer;return give answer;",
        "let values=(callback(),17);let (answer,marker)=values;let again=values;return give answer;",
        "let answer=callback();let (moved,marker)=(give answer,17);return give answer;",
        "let answer:Managed<Task<i64>>=callback();return give answer;",
    ] {
        let text = format!("{prefix}{body}}}fn main(){{}}");
        let errors =
            expand(&text).expect_err("owning Task results keep affine identity and full type");
        assert!(
            errors.iter().any(|error| error.phase == Phase::Expand
                && error.category == zil_base::ResultCategory::SafeReject),
            "{text}: {errors:?}"
        );
        assert!(
            !errors
                .iter()
                .any(|error| error.category == zil_base::ResultCategory::Unsupported),
            "{text}: {errors:?}"
        );
    }
}
