//! Actual original-source proposal, not a certificate from Types: its final
//! proof gate was still closed when materialized. Analyze discards the empty
//! transported plan and rebuilds every claim. The Source bytes remain in
//! tests/fixtures/semantic-scopes/ordinary-generic.zil.
use zil_analyze::analyze;
use zil_base::{Limits, Phase, ResultCategory};
use zil_ir::*;

fn original() -> TypedModule {
    let limits = Limits::default();
    let (module, plan) = wire::decode_module_candidate(
        include_bytes!("fixtures/semantic-identity.typed.wire"),
        &limits,
    )
    .unwrap();
    assert!(
        plan.functions.is_empty(),
        "untrusted materialization has no A receipt"
    );
    module
}

fn decoded(module: &TypedModule) -> TypedModule {
    let limits = Limits::default();
    let bytes =
        wire::encode_module_candidate(module, &CandidatePlan::empty_for(module), &limits).unwrap();
    wire::decode_module_candidate(&bytes, &limits).unwrap().0
}

fn rejects(module: &TypedModule, message: &str) {
    for input in [module.clone(), decoded(module)] {
        let errors = analyze(&input, &Limits::default())
            .expect_err("invalid original relation must not produce a candidate");
        assert!(
            errors
                .iter()
                .any(|error| error.phase == Phase::Analyze && error.message.contains(message)),
            "{errors:#?}"
        );
    }
}

#[test]
fn actual_source_instances_calls_and_full_actuals_are_reconstructed() {
    let module = original();
    for input in [module.clone(), decoded(&module)] {
        let plan = analyze(&input, &Limits::default()).unwrap();
        assert_eq!(plan.functions.len(), input.functions.len());
    }

    let mut missing = module.clone();
    missing.semantic.as_mut().unwrap().sites.clear();
    rejects(&missing, "original emission recipe has no actual operation");

    let mut wrong_kind = module.clone();
    let proposal = wrong_kind.semantic.as_mut().unwrap();
    let SemanticSiteAction::Call { actuals, .. } = &mut proposal.sites[0].action else {
        panic!("actual call fixture");
    };
    actuals.clear();
    rejects(&wrong_kind, "full mixed substitution");

    let mut wrong_definition = module;
    let proposal = wrong_definition.semantic.as_mut().unwrap();
    let SemanticSiteAction::Call { target, .. } = &mut proposal.sites[0].action else {
        panic!("actual call fixture");
    };
    *target = SemanticFunctionTarget::Typed(FunctionId(0));
    rejects(&wrong_definition, "actual called definition");
}

fn another_real_call(module: &mut TypedModule) {
    let function = &mut module.functions[0];
    let call = function.blocks[0].instructions[3].clone();
    assert!(matches!(
        call.kind,
        InstructionKind::Call {
            function: FunctionId(1),
            ..
        }
    ));
    let value = ValueId(function.values.len() as u32);
    function.values.push(Type::I32);
    function.blocks[0].instructions.insert(
        4,
        Instruction {
            result: Some(value),
            ..call
        },
    );
    let program = module.semantic.as_mut().unwrap();
    let mut second = program.sites[0].clone();
    second.at = SemanticSiteAddress::Typed {
        block: BlockId(0),
        instruction: 4,
        edge: SemanticSiteEdge::After,
    };
    program.sites.push(second);
}

#[test]
fn one_original_event_cannot_authorize_two_actual_calls() {
    let mut module = original();
    another_real_call(&mut module);
    rejects(&module, "one original semantic event");

    // A different raw program with a genuine second original operation is
    // legal. This control prevents an opcode-count or identity failure from
    // masquerading as proof of linear original-event consumption.
    let program = module.semantic.as_mut().unwrap();
    let owner = program
        .instances
        .iter()
        .find(|instance| instance.id == program.sites[0].instance)
        .unwrap()
        .definition;
    let definition = program
        .definitions
        .iter_mut()
        .find(|definition| definition.id == owner)
        .unwrap();
    let SemanticDefinitionBody::Function { sites, .. } = &mut definition.body else {
        panic!("actual original function");
    };
    let mut recipe = sites[0].clone();
    recipe.id = SemanticSiteId(1);
    recipe.occurrence_path = vec![20, 1];
    sites.push(recipe);
    program.sites[1].recipe = SemanticSiteId(1);
    for input in [module.clone(), decoded(&module)] {
        analyze(&input, &Limits::default()).unwrap();
    }
}

#[test]
fn actual_source_proof_pays_each_fresh_request_at_its_exact_boundary() {
    let module = original();
    let defaults = Limits::default();
    analyze(&module, &defaults).unwrap();
    let mut low = 0;
    let mut high = defaults.max_analysis_steps;
    while low < high {
        let middle = low + (high - low) / 2;
        let limits = Limits {
            max_analysis_steps: middle,
            ..defaults.clone()
        };
        if analyze(&module, &limits).is_ok() {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    assert!(low > 0);
    let exact = Limits {
        max_analysis_steps: low,
        ..defaults.clone()
    };
    let short = Limits {
        max_analysis_steps: low - 1,
        ..defaults
    };
    for _ in 0..2 {
        analyze(&module, &exact).unwrap();
        let errors = analyze(&module, &short).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.category == ResultCategory::ResourceLimit),
            "{errors:#?}"
        );
    }
}
