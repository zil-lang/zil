//! Hand-built TypedIR controls for the independent Analyze constructor boundary.
//! These are raw producer tests, not source or native-execution evidence.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_analyze::analyze;
use zil_base::{
    FileId, Fingerprint, Limits, ModuleId, Phase, ResultCategory, SCHEMA_VERSION, Span,
    TargetProfile,
};
use zil_ir::*;

fn span() -> Span {
    Span::new(FileId(0), 0, 1)
}
fn ins(result: Option<ValueId>, kind: InstructionKind) -> Instruction {
    Instruction {
        result,
        kind,
        span: span(),
    }
}
fn function(id: u32) -> TypedFunction {
    TypedFunction {
        id: FunctionId(id),
        name: format!("ordinary_name_{id}"),
        foreign: None,
        environment: None,
        callable_return_contract: None,
        callable_contract: None,
        parameters: vec![],
        return_type: Type::Unit,
        result_mode: PassingMode::Infer,
        locals: vec![],
        values: vec![],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![],
            terminator: Terminator::Return(None),
        }],
        entry: BlockId(0),
        span: span(),
    }
}
fn local(index: u32, ty: Type) -> LocalDecl {
    LocalDecl {
        id: LocalId(index),
        name: format!("p{index}"),
        ty,
        mutable: false,
        span: span(),
    }
}
fn fixture(inputs: usize, fields: usize) -> TypedModule {
    assert!((1..=2).contains(&inputs) && fields <= inputs);
    let closure = Type::Closure(ClosureId(0));
    let payload = Type::Record {
        name: "R".into(),
        fields: vec![("n".into(), Type::I32)],
    };
    let managed = Type::Managed(Box::new(payload.clone()));
    let package = Type::Package {
        name: "State".into(),
        fields: std::iter::once(("resource".into(), managed.clone()))
            .chain((0..fields).map(|i| (format!("field{i}"), closure.clone())))
            .collect(),
    };
    let mut body = function(0);
    body.environment = Some(ClosureId(0));
    let mut constructor = function(1);
    constructor.parameters = (0..=inputs).map(|i| LocalId(i as u32)).collect();
    constructor.locals = (0..inputs)
        .map(|i| local(i as u32, closure.clone()))
        .collect();
    constructor
        .locals
        .push(local(inputs as u32, managed.clone()));
    constructor.return_type = package.clone();
    constructor.blocks[0]
        .instructions
        .push(ins(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }));
    for index in 0..fields {
        constructor.values.push(closure.clone());
        constructor.blocks[0].instructions.push(ins(
            Some(ValueId(index as u32)),
            InstructionKind::TakeLocal {
                local: LocalId(index as u32),
            },
        ));
    }
    let resource = ValueId(constructor.values.len() as u32);
    constructor.values.push(managed);
    constructor.blocks[0].instructions.push(ins(
        Some(resource),
        InstructionKind::TakeLocal {
            local: LocalId(inputs as u32),
        },
    ));
    let value = ValueId(constructor.values.len() as u32);
    constructor.values.push(package);
    constructor.blocks[0].instructions.push(ins(
        Some(value),
        InstructionKind::MakePackage {
            fields: std::iter::once(resource)
                .chain((0..fields).map(|i| ValueId(i as u32)))
                .collect(),
        },
    ));
    for index in fields..inputs {
        constructor.blocks[0].instructions.push(ins(
            None,
            InstructionKind::DropClosure {
                local: LocalId(index as u32),
            },
        ));
    }
    for index in (0..=inputs).rev() {
        constructor.blocks[0].instructions.push(ins(
            None,
            InstructionKind::StorageDead(LocalId(index as u32)),
        ));
    }
    constructor.blocks[0].instructions.push(ins(
        None,
        InstructionKind::ScopeExit {
            scope: ScopeId(0),
            kind: CleanupExitKind::Return,
        },
    ));
    constructor.blocks[0].terminator = Terminator::Return(Some(value));
    TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(b"raw two-input State constructor"),
        target: TargetProfile::host_llvm(),
        resources: vec![ResourceDefinition {
            name: "R".into(),
            payload,
            teardown: None,
        }],
        functions: vec![body, constructor],
        closures: vec![ClosureDefinition {
            id: ClosureId(0),
            function: FunctionId(0),
            captures: vec![],
            parameters: vec![],
            result: Type::Unit,
            kind: CallableKind::FnOnce,
            lending_captures: vec![],
        }],
        unsupported: vec![],
    }
}
fn record(module: &TypedModule, result: &str) {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let Some(root) = std::env::var_os("ZIL_TEST_REPORT_DIR") else {
        return;
    };
    let path = PathBuf::from(root).join(format!(
        "raw200-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("module.txt"), format!("{module:#?}")).unwrap();
    fs::write(path.join("result.txt"), result).unwrap();
}
fn accepted(module: &TypedModule) {
    let result = analyze(module, &Limits::default());
    record(module, &format!("{result:#?}"));
    result.unwrap();
}
fn rejected(module: &TypedModule, category: ResultCategory, message: &str) {
    let result = analyze(module, &Limits::default());
    record(module, &format!("{result:#?}"));
    let errors = result.unwrap_err();
    assert!(
        errors.iter().any(|d| d.phase == Phase::Analyze
            && d.category == category
            && d.message.contains(message)),
        "{errors:?}"
    );
}
fn extra(module: &mut TypedModule, ty: Type) {
    let f = &mut module.functions[1];
    let index = f.locals.len() as u32;
    f.locals.push(local(index, ty));
    f.parameters.push(LocalId(index));
    let before_exit = f.blocks[0].instructions.len() - 1;
    f.blocks[0].instructions.insert(
        before_exit,
        ins(None, InstructionKind::StorageDead(LocalId(index))),
    );
}

#[test]
fn one_and_two_inputs_keep_individual_complete_state_responsibilities() {
    for (inputs, fields) in [(1, 1), (2, 1), (2, 2)] {
        accepted(&fixture(inputs, fields));
    }
    let mut named_differently = fixture(2, 2);
    named_differently.functions[1].name = "not_pack_or_make".into();
    accepted(&named_differently);
}

#[test]
fn constructor_does_not_erase_actual_type_or_borrowed_parameter_modes() {
    let good = fixture(2, 2);
    let mut borrowed = good.clone();
    borrowed.functions[1].locals[1].ty = Type::Address {
        mutable: false,
        inner: Box::new(Type::Closure(ClosureId(0))),
    };
    borrowed.functions[1].values[1] = borrowed.functions[1].locals[1].ty.clone();
    rejected(
        &borrowed,
        ResultCategory::SafeReject,
        "no public helper contract",
    );
    let mut other = good.clone();
    let mut definition = other.closures[0].clone();
    definition.id = ClosureId(1);
    definition.function = FunctionId(2);
    other.closures.push(definition);
    let mut body = other.functions[0].clone();
    body.id = FunctionId(2);
    body.environment = Some(ClosureId(1));
    other.functions.push(body);
    other.functions[1].locals[1].ty = Type::Closure(ClosureId(1));
    other.functions[1].values[1] = Type::Closure(ClosureId(1));
    rejected(
        &other,
        ResultCategory::SafeReject,
        "no public helper contract",
    );
    let mut duplicate = good.clone();
    duplicate.functions[1].parameters[1] = LocalId(0);
    rejected(
        &duplicate,
        ResultCategory::SafeReject,
        // The removed actual parameter leaves local 1 uninitialized before
        // helper-contract checks; this is an initialization rejection.
        "not initialized on every incoming path",
    );
    accepted(&good);
}

#[test]
fn returned_field_count_and_identity_bound_the_new_exception() {
    let good = fixture(2, 2);
    for count in [0, 3] {
        let mut module = good.clone();
        let result = Type::Package {
            name: "State".into(),
            fields: std::iter::once(match &module.functions[1].return_type {
                Type::Package { fields, .. } => fields[0].clone(),
                _ => unreachable!(),
            })
            .chain((0..count).map(|i| (format!("f{i}"), Type::Closure(ClosureId(0)))))
            .collect(),
        };
        module.functions[1].return_type = result.clone();
        *module.functions[1].values.last_mut().unwrap() = result;
        rejected(
            &module,
            ResultCategory::SafeReject,
            "no public helper contract",
        );
    }
}

#[test]
fn duplicated_ssa_missing_assembly_field_and_unpaid_second_input_still_reject() {
    let good = fixture(2, 2);
    for duplicate in [true, false] {
        let mut module = good.clone();
        let i = module.functions[1].blocks[0]
            .instructions
            .iter_mut()
            .find(|i| matches!(i.kind, InstructionKind::MakePackage { .. }))
            .unwrap();
        let InstructionKind::MakePackage { fields } = &mut i.kind else {
            unreachable!()
        };
        if duplicate {
            fields[2] = fields[1];
        } else {
            fields.pop();
        }
        rejected(
            &module,
            ResultCategory::SafeReject,
            if duplicate { "resource" } else { "field arity" },
        );
    }
    let mut unpaid = fixture(2, 1);
    unpaid.functions[1].blocks[0]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::DropClosure { local: LocalId(1) }));
    rejected(&unpaid, ResultCategory::SafeReject, "resource");
}

#[test]
fn self_containment_is_rebuilt_from_actual_capture_types() {
    let good = fixture(2, 2);
    let mut module = good.clone();
    module.closures[0].captures.push(Type::Address {
        mutable: false,
        inner: Box::new(Type::I32),
    });
    rejected(&module, ResultCategory::Unsupported, "self-contained");
    accepted(&good);
}

#[test]
fn new_extra_parameter_boundary_does_not_rewrite_the_old_single_input_rule() {
    let address = Type::Address {
        mutable: false,
        inner: Box::new(Type::I32),
    };
    let mut old = fixture(1, 1);
    extra(&mut old, address.clone());
    accepted(&old);
    let mut new = fixture(2, 2);
    extra(&mut new, address);
    rejected(
        &new,
        ResultCategory::SafeReject,
        "no public helper contract",
    );
    let mut copy = fixture(2, 2);
    extra(&mut copy, Type::Tuple(vec![Type::I32, Type::I64]));
    accepted(&copy);
}

#[test]
fn ordinary_noescape_helpers_still_require_their_actual_public_contract() {
    let mut module = fixture(2, 2);
    let f = &mut module.functions[1];
    f.return_type = Type::Unit;
    f.values.clear();
    f.blocks[0].instructions = vec![
        ins(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
        ins(None, InstructionKind::DropManaged { local: LocalId(2) }),
        ins(None, InstructionKind::DropClosure { local: LocalId(1) }),
        ins(None, InstructionKind::DropClosure { local: LocalId(0) }),
        ins(None, InstructionKind::StorageDead(LocalId(2))),
        ins(None, InstructionKind::StorageDead(LocalId(1))),
        ins(None, InstructionKind::StorageDead(LocalId(0))),
        ins(
            None,
            InstructionKind::ScopeExit {
                scope: ScopeId(0),
                kind: CleanupExitKind::Return,
            },
        ),
    ];
    f.blocks[0].terminator = Terminator::Return(None);
    rejected(
        &module,
        ResultCategory::SafeReject,
        "no public helper contract",
    );
    module.functions[1].callable_contract = Some(CallableParameterContract {
        roles: (0..2)
            .map(|parameter| CallableRole {
                parameter,
                closure: ClosureId(0),
                kind: CallableKind::FnOnce,
                hold: vec![],
                call: vec![],
                writes: vec![],
                cleanup: CallableCleanup::Owned,
            })
            .collect(),
        traces: CallableTraces::Finite(vec![CallableTrace {
            calls: vec![],
            exit: CallableExit::Return,
        }]),
    });
    accepted(&module);
}

#[test]
fn constructor_scan_and_graph_work_obey_exact_fresh_budget_boundaries() {
    for module in [fixture(1, 1), fixture(2, 1), fixture(2, 2)] {
        let reference = analyze(&module, &Limits::default()).unwrap();
        let run = |work| {
            analyze(
                &module,
                &Limits {
                    max_analysis_steps: work,
                    ..Limits::default()
                },
            )
        };
        let mut low = 0;
        let mut high = Limits::default().max_analysis_steps;
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            match run(middle) {
                Ok(_) => high = middle,
                Err(errors) => {
                    assert!(
                        errors.iter().all(|d| d.phase == Phase::Analyze
                            && d.category == ResultCategory::ResourceLimit),
                        "{errors:?}"
                    );
                    low = middle;
                }
            }
        }
        assert_eq!(run(high).unwrap(), reference);
        assert!(
            run(high - 1)
                .unwrap_err()
                .iter()
                .all(|d| d.category == ResultCategory::ResourceLimit)
        );
        assert_eq!(run(high).unwrap(), reference);
        println!(
            "raw constructors: parameters={} fields={} N={high}",
            module.functions[1].parameters.len(),
            match &module.functions[1].return_type {
                Type::Package { fields, .. } => fields.len(),
                _ => 0,
            }
        );
    }
}
