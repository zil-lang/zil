use zil_analyze::analyze;
use zil_base::{
    FileId, Fingerprint, Limits, ModuleId, Phase, ResultCategory, SCHEMA_VERSION, Span,
    TargetProfile,
};
use zil_ir::*;

fn span() -> Span {
    Span::new(FileId(0), 0, 1)
}

fn module(functions: usize, constants: usize, parameter: bool) -> TypedModule {
    TypedModule {
        resources: Vec::new(),
        closures: vec![],
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::host_llvm(),
        functions: (0..functions)
            .map(|index| TypedFunction {
                callable_return_contract: None,
                callable_contract: None,
                result_mode: zil_ir::PassingMode::Infer,
                foreign: None,
                environment: None,
                id: FunctionId(index as u32),
                name: format!("f{index}"),
                parameters: if parameter { vec![LocalId(0)] } else { vec![] },
                return_type: Type::Unit,
                locals: if parameter {
                    vec![LocalDecl {
                        id: LocalId(0),
                        name: "x".into(),
                        ty: Type::I32,
                        mutable: false,
                        span: span(),
                    }]
                } else {
                    vec![]
                },
                values: vec![Type::Unit; constants],
                blocks: vec![BasicBlock {
                    id: BlockId(0),
                    instructions: (0..constants)
                        .map(|index| Instruction {
                            result: Some(ValueId(index as u32)),
                            kind: InstructionKind::Constant(Constant::Unit),
                            span: span(),
                        })
                        .collect(),
                    terminator: Terminator::Return(None),
                }],
                entry: BlockId(0),
                span: span(),
            })
            .collect(),
        unsupported: vec![],
    }
}

fn assert_limit(module: &TypedModule, limits: &Limits) {
    let errors = analyze(module, limits).expect_err("over-budget raw input must fail closed");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].phase, Phase::Analyze);
    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
    assert_eq!(errors[0].code, "E-RESOURCE-LIMIT");
}

#[test]
fn original_raw_reproducer_checks_each_limit_independently() {
    let raw = module(1, 100, false);
    for limits in [
        Limits {
            max_functions: 0,
            ..Limits::default()
        },
        Limits {
            max_blocks: 0,
            ..Limits::default()
        },
        Limits {
            max_instructions: 99,
            ..Limits::default()
        },
        Limits {
            max_values: 99,
            ..Limits::default()
        },
        Limits {
            max_analysis_steps: 3,
            ..Limits::default()
        },
    ] {
        assert_limit(&raw, &limits);
    }
    assert_limit(
        &module(1, 0, true),
        &Limits {
            max_locals: 0,
            ..Limits::default()
        },
    );
}

#[test]
fn structural_limits_accumulate_across_functions() {
    let raw = module(2, 3, true);
    // Every function fits every bound by itself. Only the module total fails.
    for limits in [
        Limits {
            max_functions: 1,
            ..Limits::default()
        },
        Limits {
            max_blocks: 1,
            ..Limits::default()
        },
        Limits {
            max_instructions: 5,
            ..Limits::default()
        },
        Limits {
            max_locals: 1,
            ..Limits::default()
        },
        Limits {
            max_values: 5,
            ..Limits::default()
        },
    ] {
        assert_limit(&raw, &limits);
    }
    let exact_counts = Limits {
        max_functions: 2,
        max_blocks: 2,
        max_instructions: 6,
        max_locals: 2,
        max_values: 6,
        ..Limits::default()
    };
    assert!(analyze(&raw, &exact_counts).is_ok());
}

fn target_text_bytes(module: &TypedModule) -> usize {
    module.target.id.len()
        + module.target.triple.len()
        + module.target.backend.len()
        + module.target.rule_version.len()
}

#[test]
fn exact_work_budget_covers_instruction_visits_and_fingerprinting() {
    let raw = module(1, 100, false);
    // 2 module units; 26 units for f0's preflight/CFG/empty sets;
    // 8 per constant: type/instruction preflight+hash, ordered initial/fixed
    // point/final transfers, and the dense ValueHold output row.
    // Target text is visited by preflight and both independent fingerprints.
    // Finite aggregate admission also reserves 2 visits for each of 101 type roots.
    // Explicit-scope admission scans each instruction once even for legacy raw IR.
    // Owning-array presence checks visit all 100 SSA types, including Copy-only
    // modules whose callback arrays may still need affine element checking.
    // Callback-field admission visits 100 SSA types and one return type.
    let required = 1230 + 101 + 3 * target_text_bytes(&raw);
    let exact = Limits {
        max_functions: 1,
        max_blocks: 1,
        max_instructions: 100,
        max_locals: 0,
        max_values: 100,
        max_analysis_steps: required,
        ..Limits::default()
    };
    let plan = analyze(&raw, &exact).expect("exact work budget must suffice");
    assert_eq!(plan.module_fingerprint, raw.fingerprint());
    assert_eq!(plan.functions.len(), 1);
    assert_limit(
        &raw,
        &Limits {
            max_analysis_steps: required - 1,
            ..exact
        },
    );
}

#[test]
fn exact_work_budget_includes_set_scans_across_functions() {
    let raw = module(2, 0, true);
    // Each one-parameter function costs 47 units: the 26-unit fN baseline,
    // 6 additional input/hash units and 15 set construction/scan/export units.
    // Each of the two functions has a result and a parameter type shape.
    // Owning-array presence checks add one visit per local, hence two here.
    // Two locals and two return types add four callback-field shape visits.
    let required = 106 + 4 + 3 * target_text_bytes(&raw);
    let exact = Limits {
        max_analysis_steps: required,
        ..Limits::default()
    };
    let plan = analyze(&raw, &exact).expect("exact global set budget must suffice");
    for function in &plan.functions {
        assert_eq!(function.blocks[0].initialized_in, vec![LocalId(0)]);
        assert_eq!(function.blocks[0].initialized_out, vec![LocalId(0)]);
    }
    assert_limit(
        &raw,
        &Limits {
            max_analysis_steps: required - 1,
            ..exact
        },
    );
}

#[test]
fn exact_work_budget_covers_cfg_edges_and_predecessor_visits() {
    let mut raw = module(1, 0, false);
    raw.functions[0].blocks = (0..8)
        .map(|index| BasicBlock {
            id: BlockId(index),
            instructions: vec![],
            terminator: if index == 7 {
                Terminator::Return(None)
            } else {
                Terminator::Jump(BlockId(index + 1))
            },
        })
        .collect();
    // A chain of n empty blocks costs 2 module units + 4 name units +
    // 22*n structural, matrix, edge, reachability, and ordered transfer units.
    // The return type shape contributes 2 preflight/hash visits.
    // Callback-field admission visits the single return type.
    let required = 184 + 1 + 3 * target_text_bytes(&raw);
    let exact = Limits {
        max_analysis_steps: required,
        ..Limits::default()
    };
    let plan = analyze(&raw, &exact).expect("exact CFG budget must suffice");
    assert_eq!(plan.functions[0].blocks.len(), 8);
    assert_limit(
        &raw,
        &Limits {
            max_analysis_steps: required - 1,
            ..exact
        },
    );
}

#[test]
fn initialization_facts_survive_a_branch_and_backedge() {
    let mut raw = module(1, 0, false);
    let function = &mut raw.functions[0];
    function.return_type = Type::I32;
    function.parameters = vec![LocalId(0)];
    function.locals = vec![
        LocalDecl {
            id: LocalId(0),
            name: "condition".into(),
            ty: Type::Bool,
            mutable: false,
            span: span(),
        },
        LocalDecl {
            id: LocalId(1),
            name: "x".into(),
            ty: Type::I32,
            mutable: true,
            span: span(),
        },
    ];
    function.values = vec![Type::I32, Type::Bool, Type::I32, Type::I32];
    let instruction = |result, kind| Instruction {
        result,
        kind,
        span: span(),
    };
    function.blocks = vec![
        BasicBlock {
            id: BlockId(0),
            instructions: vec![
                instruction(
                    Some(ValueId(0)),
                    InstructionKind::Constant(Constant::I32(1)),
                ),
                instruction(
                    None,
                    InstructionKind::Store {
                        local: LocalId(1),
                        value: ValueId(0),
                    },
                ),
            ],
            terminator: Terminator::Jump(BlockId(1)),
        },
        BasicBlock {
            id: BlockId(1),
            instructions: vec![instruction(
                Some(ValueId(1)),
                InstructionKind::Load(LocalId(0)),
            )],
            terminator: Terminator::Branch {
                condition: ValueId(1),
                then_block: BlockId(2),
                else_block: BlockId(3),
            },
        },
        BasicBlock {
            id: BlockId(2),
            instructions: vec![
                instruction(
                    Some(ValueId(2)),
                    InstructionKind::Constant(Constant::I32(2)),
                ),
                instruction(
                    None,
                    InstructionKind::Store {
                        local: LocalId(1),
                        value: ValueId(2),
                    },
                ),
            ],
            terminator: Terminator::Jump(BlockId(1)),
        },
        BasicBlock {
            id: BlockId(3),
            instructions: vec![instruction(
                Some(ValueId(3)),
                InstructionKind::Load(LocalId(1)),
            )],
            terminator: Terminator::Return(Some(ValueId(3))),
        },
    ];
    let plan = analyze(&raw, &Limits::default()).unwrap();
    assert_eq!(plan.functions[0].blocks[0].initialized_in, vec![LocalId(0)]);
    for block in &plan.functions[0].blocks {
        assert_eq!(block.initialized_out, vec![LocalId(0), LocalId(1)]);
        if block.block != BlockId(0) {
            assert_eq!(block.initialized_in, vec![LocalId(0), LocalId(1)]);
        }
    }
    // Removing the initial store leaves the zero-iteration exit uninitialized,
    // even though the backedge stores x on every completed loop iteration.
    raw.functions[0].blocks[0].instructions.pop();
    assert_eq!(
        analyze(&raw, &Limits::default()).unwrap_err()[0].code,
        "E-UNINITIALIZED"
    );
}

#[test]
fn all_metadata_positions_are_bounded_before_fingerprinting() {
    let limits = Limits {
        max_analysis_steps: 1_000,
        ..Limits::default()
    };
    for location in 0..6 {
        let mut raw = module(1, 0, true);
        let text = match location {
            0 => &mut raw.target.id,
            1 => &mut raw.target.triple,
            2 => &mut raw.target.backend,
            3 => &mut raw.target.rule_version,
            4 => &mut raw.functions[0].name,
            _ => &mut raw.functions[0].locals[0].name,
        };
        *text = "x".repeat(1_001);
        assert_limit(&raw, &limits);
    }
}

#[test]
fn call_argument_visits_are_bounded_before_fingerprinting() {
    let mut raw = module(1, 1, false);
    raw.functions[0].blocks[0].instructions.push(Instruction {
        result: None,
        kind: InstructionKind::Call {
            function: FunctionId(0),
            arguments: vec![ValueId(0); 1_001],
        },
        span: span(),
    });
    // This is deliberately raw, not a well-typed call. Its size must be rejected
    // without relying on the type checker or independent verifier first.
    assert_limit(
        &raw,
        &Limits {
            max_analysis_steps: 1_000,
            ..Limits::default()
        },
    );
}

#[test]
fn every_type_position_rejects_unknown_semantics_before_hashing() {
    for position in 0..3 {
        let mut raw = module(1, 1, true);
        let set_type = |raw: &mut TypedModule, ty| match position {
            0 => raw.functions[0].return_type = ty,
            1 => raw.functions[0].locals[0].ty = ty,
            _ => raw.functions[0].values[0] = ty,
        };
        let address = Type::Address {
            mutable: true,
            inner: Box::new(Type::I32),
        };
        // 042 admits one address layer in each raw metadata position.
        set_type(&mut raw, address.clone());
        assert!(analyze(&raw, &Limits::default()).is_ok());
        // Nested addresses remain outside the admitted shape before hashing.
        set_type(
            &mut raw,
            Type::Address {
                mutable: false,
                inner: Box::new(address),
            },
        );
        let errors = analyze(&raw, &Limits::default()).unwrap_err();
        assert_eq!(errors[0].phase, Phase::Analyze);
        assert_eq!(errors[0].category, ResultCategory::Unsupported);
        assert_eq!(errors[0].code, "E-UNSUPPORTED");
    }
}

#[test]
fn known_recursive_aggregate_shape_obeys_selected_depth() {
    let mut raw = module(1, 0, false);
    let mut ty = Type::I32;
    for _ in 0..256 {
        ty = Type::Tuple(vec![ty]);
    }
    raw.functions[0].return_type = ty;
    assert!(analyze(&raw, &Limits::default()).is_ok());
    assert_limit(
        &raw,
        &Limits {
            max_call_depth: 255,
            ..Limits::default()
        },
    );
}

#[test]
fn unsupported_metadata_does_not_allocate_unbounded_diagnostics() {
    let mut raw = module(1, 0, false);
    raw.unsupported.push(UnsupportedFeature {
        feature: "x".repeat(1_001),
        span: None,
    });
    assert_limit(
        &raw,
        &Limits {
            max_analysis_steps: 1_000,
            ..Limits::default()
        },
    );
    raw.unsupported.clear();
    raw.functions[0].blocks[0].instructions.push(Instruction {
        result: None,
        kind: InstructionKind::Unsupported("x".repeat(1_001)),
        span: span(),
    });
    assert_limit(
        &raw,
        &Limits {
            max_analysis_steps: 1_000,
            ..Limits::default()
        },
    );
}

#[test]
fn callable_return_metadata_is_reserved_before_fingerprinting_or_validation() {
    let mut raw = module(1, 0, false);
    raw.functions[0].callable_return_contract = Some(CallableReturnContract {
        closure: ClosureId(0),
        kind: CallableKind::Fn,
        hold: Vec::new(),
        call: Vec::new(),
        writes: Vec::new(),
        cleanup: vec![0; 1_001],
    });
    assert_limit(
        &raw,
        &Limits {
            max_analysis_steps: 1_000,
            ..Limits::default()
        },
    );
}
