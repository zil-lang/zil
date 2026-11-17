use super::*;
use std::sync::Arc;
use zil_base::{FileId, Fingerprint, ModuleId, SourceUnit};
use zil_expand::{
    ExpandSession,
    source_context::{DeclarationPlacement, SourceIdentityContext},
};
use zil_syntax::{ParsedSource, attributes::AttributeContext};

fn prepared() -> PreparedProgram {
    let texts = [
        "fn shape_first(x:i32)->i32 {let y:i32=i32(x+1); if x<10 {return -y;} else {while false {let z:i32=y*2;} return y+3;}} fn shape_second(x:i32)->i32 {unsafe {let y:i32=x+2;} return shape_first(x);}",
        "fn shape_third(x:i32)->i32 {let y:i32=(x+1)*2; return y;}",
    ];
    let limits = Limits::default();
    let sources = texts
        .iter()
        .enumerate()
        .map(|(index, text)| {
            let parsed = ParsedSource::parse(
                Arc::new(SourceUnit::new(
                    FileId(index as u32),
                    ModuleId(index as u32),
                    format!("shape{index}.zil"),
                    *text,
                )),
                &limits,
                &AttributeContext::default(),
            );
            assert!(parsed.parsed().is_ok(), "{:?}", parsed.parsed().diagnostics);
            parsed
        })
        .collect::<Vec<_>>();
    let produced = ExpandSession::default()
        .expand_parsed(
            &sources,
            &[vec![], vec![]],
            &limits,
            &TargetProfile::default(),
            SourceIdentityContext::ClosedSnapshot {
                root: ModuleId(0),
                aliases: &BTreeMap::new(),
                declarations: DeclarationPlacement::Original,
            },
        )
        .unwrap()
        .expect("closed scalar input")
        .into_program(ModuleId(0), Fingerprint::ZERO)
        .0;
    infer_produced_program_for_ct_with_session(
        &produced,
        &limits,
        &mut RawInferenceSession::default(),
    )
    .unwrap()
    .into_prepared()
}

#[test]
fn current_inference_shape_observations_equal_original_visits_and_every_fee_prefix() {
    let prepared = prepared();
    let limits = Limits::default();
    let program = prepared.program();
    let inputs = prepared.current_inputs(&limits).unwrap();
    let mut total = 0usize;
    for (index, function) in program
        .modules
        .iter()
        .flat_map(|module| &module.core.functions)
        .enumerate()
    {
        let fee = validate_function(function, &limits).unwrap().unwrap();
        assert_eq!(inputs.shape(index), Some(fee));
        total += fee;
    }
    for allowance in 0..=total + 1 {
        let limits = Limits {
            max_analysis_steps: allowance,
            ..limits.clone()
        };
        for omit in [None, Some(1)] {
            let mut actual_costs = Vec::new();
            let mut expected_costs = Vec::new();
            let mut work = ShapeWork::default();
            // Exercise the independent shape ledger with exact observations
            // produced by the real current inference visitor. Runtime callers
            // additionally require matching Core and every Limits field.
            let actual = validate_visiting(
                program,
                &limits,
                Some(&mut actual_costs),
                |index| {
                    if omit == Some(index) {
                        None
                    } else {
                        inputs.shape(index)
                    }
                },
                &mut work,
            );
            let expected = validate_recording(program, &limits, Some(&mut expected_costs));
            assert_eq!(actual, expected, "allowance={allowance} omit={omit:?}");
            assert_eq!(actual_costs, expected_costs);
        }
    }
    for depth in 0..=12 {
        let limits = Limits {
            max_call_depth: depth,
            ..limits.clone()
        };
        let mut work = ShapeWork::default();
        assert_eq!(
            validate_observed(program, &limits, None, Some(inputs), &mut work),
            validate(program, &limits)
        );
        assert_eq!(
            work.observed_functions, 0,
            "different depth declines current observations"
        );
    }
}

#[test]
fn shape_observations_bind_actual_current_owner_and_complete_limits() {
    let prepared = prepared();
    let limits = Limits::default();
    let inputs = prepared.current_inputs(&limits).unwrap();
    let mut work = ShapeWork::default();
    assert_eq!(
        validate_observed(prepared.program(), &limits, None, Some(inputs), &mut work),
        Some(Ok(()))
    );
    assert_eq!((work.observed_functions, work.checked_functions), (3, 0));
    let copied = prepared.program().clone();
    let mut work = ShapeWork::default();
    assert_eq!(
        validate_observed(&copied, &limits, None, Some(inputs), &mut work),
        validate(&copied, &limits)
    );
    assert_eq!((work.observed_functions, work.checked_functions), (0, 3));
    let limits = Limits {
        max_ct_steps: limits.max_ct_steps - 1,
        ..limits
    };
    let mut work = ShapeWork::default();
    assert_eq!(
        validate_observed(prepared.program(), &limits, None, Some(inputs), &mut work),
        validate(prepared.program(), &limits)
    );
    assert_eq!((work.observed_functions, work.checked_functions), (0, 3));
}

#[test]
fn whole_current_header_gate_precedes_any_observed_shape_body() {
    let prepared = prepared();
    let mut program = prepared.program().clone();
    Arc::make_mut(&mut program.modules[1].core).functions[0].return_type =
        Type::Named("MissingShape".into());
    let mut work = ShapeWork::default();
    let mut costs = Vec::new();
    let mut read = false;
    let actual = validate_visiting(
        &program,
        &Limits::default(),
        Some(&mut costs),
        |_| {
            read = true;
            Some(0)
        },
        &mut work,
    );
    assert!(actual.is_none());
    assert!(!read);
    assert!(costs.is_empty());
    assert_eq!((work.observed_functions, work.checked_functions), (0, 0));
}
