//! Real Source controls for recursive, source-authenticated pure value copies.
use super::super::invocation_context::FrozenInvocationContext;
use super::*;
use zil_base::ResultCategory;
use zil_ir::{BinaryOp, Constant};

fn fixtures() -> [(&'static str, &'static str); 2] {
    [
        (
            "original",
            include_str!(
                "../../../../tests/language-readiness/terminal/344/anonymous/ct6-cross-factory-pure-composites.zil"
            ),
        ),
        (
            "recursive",
            include_str!(
                "../../../../tests/language-readiness/terminal/344/anonymous/ct6-recursive-cross-schema-composites.zil"
            ),
        ),
    ]
}

#[test]
fn g4454_actual_recursive_values_reach_runtime_types_and_verify() {
    for (name, text) in fixtures() {
        let source = SourceUnit::new(
            zil_base::FileId(0),
            ModuleId(0),
            format!("{name}.zil"),
            text,
        );
        let dir = std::env::temp_dir().join(format!("g4454-types-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let report = crate::compile_source(
            &source,
            &crate::CompileOptions {
                stop_after: crate::Stage::Types,
                emit_dir: Some(dir.clone()),
                ..crate::CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        let typed = std::fs::read_to_string(report.artifacts.get("typed").unwrap()).unwrap();
        for opcode in [
            "MakeAggregate",
            "ExtractField",
            "ExtractIndex",
            "MakeVariant",
            "VariantTag",
            "ReadVariant",
        ] {
            assert!(
                typed.contains(opcode),
                "{name}: actual Typed omitted {opcode}"
            );
        }
        let report = crate::compile_source(&source, &crate::CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn g4454_actual_recursive_values_preserve_wide_bits_native_o0_o2() {
    for (name, text) in fixtures() {
        let source = SourceUnit::new(
            zil_base::FileId(0),
            ModuleId(0),
            format!("{name}.zil"),
            text,
        );
        for optimization in [
            zil_backend_llvm::Optimization::O0,
            zil_backend_llvm::Optimization::O2,
        ] {
            let dir = std::env::temp_dir().join(format!(
                "g4454-native-{name}-{}-{optimization:?}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let report = crate::compile_source(
                &source,
                &crate::CompileOptions {
                    backend: zil_backend_llvm::NativeBackend::Clang,
                    stop_after: crate::Stage::Build,
                    optimization,
                    entry: Some("main".into()),
                    output: Some(dir.join("program")),
                    ..crate::CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            let output = std::process::Command::new(report.artifacts.get("executable").unwrap())
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            assert_eq!(output.stdout, b"18446744073709551557\n");
            eprintln!("G4.4.5.4 {name} Clang {optimization:?}: full u64=18446744073709551557");
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}

fn with_authenticated_source(
    text: &str,
    check: impl FnOnce(&CtArtifactProgram, &compiler::SourceCompilation, &FrozenInvocationContext),
) {
    let sources = vec![SourceUnit::new(
        zil_base::FileId(0),
        ModuleId(0),
        "recursive-call.zil",
        text,
    )];
    let parsed = vec![Arc::new(zil_syntax::parse(&sources[0]))];
    assert!(
        parsed[0].diagnostics.is_empty(),
        "{:?}",
        parsed[0].diagnostics
    );
    let syntax = vec![&parsed[0].module];
    let paths = BTreeMap::new();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let mut environment = Environment::new(
        &sources,
        &parsed,
        &paths,
        &[vec![]],
        ModuleId(0),
        sources[0].fingerprint(),
        &target,
        &limits,
    )
    .unwrap();
    let (ordinal, function) = parsed[0]
        .module
        .items
        .iter()
        .enumerate()
        .find_map(|(ordinal, item)| {
            if let zil_syntax::ast::ItemKind::Function(function) = &item.kind {
                (function.name == "result").then_some((ordinal, function))
            } else {
                None
            }
        })
        .unwrap();
    let zil_syntax::ast::FunctionBody::Block(block) = &function.body else {
        unreachable!()
    };
    let zil_syntax::ast::StmtKind::Binding { value, .. } = &block.statements[0].kind else {
        unreachable!()
    };
    let zil_syntax::ast::ExprKind::MetaEval(expression) = &value.kind else {
        unreachable!()
    };
    let definition = CtSymbolId {
        module: ModuleId(0),
        declaration: zil_base::NodeId(ordinal as u32),
        generation_path: None,
    };
    let entry = || Entry::Expression {
        module: ModuleId(0),
        expression,
        definition: definition.clone(),
        lexical_owner: Some(definition.clone()),
        expected: None,
        parameters: &[],
    };
    let input = environment.empty_input(expression.span).unwrap();
    let modules = environment.index.modules(&sources, &syntax);
    let (expected, facts) = compiler::compile_scoped(CompileRequest {
        target_pointer_width: 64,
        modules: &modules,
        source_fingerprint: sources[0].fingerprint(),
        entry: entry(),
        input_tokens: &input,
        snapshot: &environment.snapshot,
        limits: &limits,
    })
    .unwrap();
    assert_eq!(facts.relocations.len(), 2);
    let (origins, _) = zil_expand::source_context::capture_original_required(
        &sources,
        &syntax,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: ModuleId(0),
            aliases: &paths,
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
        &limits,
    )
    .unwrap();
    let origins = Arc::new(origins);
    let mut context = FrozenInvocationContext::freeze(
        &entry(),
        &parsed,
        &environment.index,
        Some(&origins),
        &mut environment.budget,
    )
    .unwrap()
    .unwrap();
    Arc::get_mut(&mut context)
        .unwrap()
        .extend_calls(
            &facts.calls,
            &facts.scopes,
            &expected,
            &environment.index,
            &mut environment.budget,
        )
        .unwrap();
    Arc::get_mut(&mut context)
        .unwrap()
        .validate_type_contexts(
            &facts,
            &expected,
            &environment.index,
            &mut environment.budget,
        )
        .unwrap();
    let authority = FrozenAuthority {
        source_fingerprint: sources[0].fingerprint(),
        snapshot: &environment.snapshot,
        input: &input,
        index: &environment.index,
        expected_functions: &expected.functions,
        expected_artifact: &expected,
        invocation_context: Some(&context),
    };
    let wire = zil_ir::ct_wire::encode_program(&expected, &limits).unwrap();
    let decoded = zil_ir::ct_wire::decode_program(&wire, &limits).unwrap();
    for candidate in [expected.clone(), decoded] {
        context
            .validate_record_relocations(
                &facts.relocations,
                &facts.calls,
                &candidate,
                &mut CtExecutionBudget::new(expected.budget),
            )
            .unwrap();
        let verified = zil_verify::verify_ct_artifact(candidate, &limits, &authority).unwrap();
        let reader = FrozenReadView {
            record_schemas: &expected.record_schemas,
            enum_schemas: &expected.enum_schemas,
            ambient_life_scopes: &expected.ambient_life_scopes,
            invocation_context: Some(&context),
            index: &environment.index,
            sources: &sources,
            syntax: &syntax,
            shape: environment.shape.as_ref(),
            impl_index: environment.impl_index.as_ref(),
            shape_failure: environment.shape_failure.as_deref(),
            input: None,
            limits: &limits,
            target: &target,
            snapshot: &environment.snapshot,
        };
        let (value, _) = zil_ct::evaluate_ct_artifact(
            &verified,
            &[],
            &reader,
            &mut CtExecutionBudget::new(expected.budget),
        )
        .unwrap();
        assert!(
            matches!(value,CtValue::Record {ref fields,..} if fields.len()==4),
            "{value:?}"
        );
    }
    check(&expected, &facts, &context);
}

#[test]
fn g4454_recursive_trace_independently_rejects_changed_data_and_occurrences() {
    with_authenticated_source(fixtures()[1].1, |expected, facts, context| {
        let reject = |raw: &CtArtifactProgram, calls: &[compiler::SourceInstanceCall]| {
            for candidate in [
                raw.clone(),
                zil_ir::ct_wire::decode_program(
                    &zil_ir::ct_wire::encode_program(raw, &Limits::default()).unwrap(),
                    &Limits::default(),
                )
                .unwrap(),
            ] {
                assert!(
                    context
                        .validate_record_relocations(
                            &facts.relocations,
                            calls,
                            &candidate,
                            &mut CtExecutionBudget::new(expected.budget)
                        )
                        .is_err()
                );
            }
        };
        let row = facts.relocations[0];
        for attack in 0..6 {
            let mut raw = expected.clone();
            let function = &mut raw.functions[row.caller.0 as usize];
            match attack {
                0 => {
                    let made = function
                        .blocks
                        .iter_mut()
                        .flat_map(|b| &mut b.instructions)
                        .find(|i| i.result == Some(row.output))
                        .unwrap();
                    let CtInstructionKind::MakeRecord { fields, .. } = &mut made.kind else {
                        unreachable!()
                    };
                    fields.swap(0, 1);
                }
                1 => {
                    let extracted = function.blocks.iter_mut().flat_map(|b| &mut b.instructions).find(|i| matches!(i.kind,CtInstructionKind::ExtractField {aggregate,..} if aggregate==row.input)).unwrap();
                    let CtInstructionKind::ExtractField { index, .. } = &mut extracted.kind else {
                        unreachable!()
                    };
                    *index = 1;
                }
                2 => {
                    let id = function
                        .blocks
                        .iter()
                        .flat_map(|b| &b.instructions)
                        .find_map(|i| {
                            if let CtInstructionKind::Constant(id) = i.kind {
                                (raw.constants[id.0 as usize]
                                    == CtConstant::Scalar(Constant::U64(0)))
                                .then_some(id)
                            } else {
                                None
                            }
                        })
                        .unwrap();
                    raw.constants[id.0 as usize] = CtConstant::Scalar(Constant::U64(1));
                }
                3 => {
                    let made = function.blocks.iter_mut().flat_map(|b| &mut b.instructions).find(|i| matches!(&i.kind,CtInstructionKind::MakeVariant {tag:1,fields,..} if !fields.is_empty())).unwrap();
                    let CtInstructionKind::MakeVariant { tag, fields, .. } = &mut made.kind else {
                        unreachable!()
                    };
                    *tag = 0;
                    fields.clear();
                }
                4 => {
                    let compared = function
                        .blocks
                        .iter_mut()
                        .flat_map(|b| &mut b.instructions)
                        .find(|i| {
                            matches!(
                                i.kind,
                                CtInstructionKind::Binary {
                                    op: BinaryOp::Equal,
                                    ..
                                }
                            )
                        })
                        .unwrap();
                    let CtInstructionKind::Binary { op, .. } = &mut compared.kind else {
                        unreachable!()
                    };
                    *op = BinaryOp::NotEqual;
                }
                _ => {
                    let block = function
                        .blocks
                        .iter_mut()
                        .find(|b| {
                            b.instructions
                                .iter()
                                .any(|i| matches!(i.kind, CtInstructionKind::Store { .. }))
                        })
                        .unwrap();
                    let at = block
                        .instructions
                        .iter()
                        .position(|i| matches!(i.kind, CtInstructionKind::Store { .. }))
                        .unwrap();
                    let duplicate = block.instructions[at].clone();
                    block.instructions.insert(at + 1, duplicate);
                }
            }
            reject(&raw, &facts.calls);
        }
        let mut calls = facts.calls.clone();
        calls
            .iter_mut()
            .find(|call| call.occurrence == Some((row.block, row.input)))
            .unwrap()
            .occurrence = None;
        reject(expected, &calls);
        let mut calls = facts.calls.clone();
        calls.push(
            calls
                .iter()
                .find(|call| call.occurrence == Some((row.block, row.input)))
                .unwrap()
                .clone(),
        );
        reject(expected, &calls);
    });
}

#[test]
fn g4454_enum_expected_type_keeps_exact_nominal_identity() {
    let text = fixtures()[1]
        .1
        .replace(
            "enum Choice<life a>",
            "enum Other<life a>{Empty,Value(Leaf<'a>)}\nenum Choice<life a>",
        )
        .replace("variant:Choice.Value", "variant:Other.Value");
    let source = SourceUnit::new(zil_base::FileId(0), ModuleId(0), "wrong-enum.zil", text);
    let report = crate::compile_source(&source, &crate::CompileOptions::default());
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("different actual nominal")),
        "{}",
        report.to_json()
    );
}

#[test]
fn g4454_recursive_consumer_budget_is_exact_and_request_local() {
    with_authenticated_source(fixtures()[1].1, |expected, facts, context| {
        for dimension in 0..2 {
            let maximum = if dimension == 0 {
                expected.budget.max_analysis_work
            } else {
                expected.budget.max_temporary_bytes
            };
            let run = |n| {
                let mut allowance = expected.budget;
                if dimension == 0 {
                    allowance.max_analysis_work = n
                } else {
                    allowance.max_temporary_bytes = n
                }
                context.validate_record_relocations(
                    &facts.relocations,
                    &facts.calls,
                    expected,
                    &mut CtExecutionBudget::new(allowance),
                )
            };
            assert!(run(maximum).is_ok());
            let (mut low, mut high) = (1, maximum);
            while low < high {
                let middle = low + (high - low) / 2;
                match run(middle) {
                    Ok(()) => high = middle,
                    Err(errors)
                        if errors
                            .iter()
                            .all(|d| d.category == ResultCategory::ResourceLimit) =>
                    {
                        low = middle + 1
                    }
                    Err(errors) => panic!("{errors:?}"),
                }
            }
            for (n, good) in [(low, true), (low - 1, false), (low, true)] {
                let result = run(n);
                assert_eq!(result.is_ok(), good);
                if let Err(errors) = result {
                    assert!(
                        errors
                            .iter()
                            .all(|d| d.category == ResultCategory::ResourceLimit),
                        "{errors:?}"
                    )
                }
            }
            eprintln!(
                "G4.4.5.4 independent recursive consumer dimension={dimension} N={low}/N-1/fresh"
            );
        }
    });
}
