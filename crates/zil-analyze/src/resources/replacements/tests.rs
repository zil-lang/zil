use super::*;
use crate::place_replacements::fixture;
fn with_caller(fresh: bool) -> TypedModule {
    fixture::module_with_relay_and_caller(fresh)
}

#[test]
fn replacement_transfer_updates_actual_field_birth_through_two_calls() {
    for fresh in [false, true] {
        let module = with_caller(fresh);
        let graphs = vec![(vec![vec![]], vec![true]); 3];
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        b.head_descriptions = true;
        let program = Program::new(&module, &graphs, &mut b).unwrap();
        let effects = infer(&module, &graphs, &program, &mut b).unwrap();
        let target = Target {
            root: origins::Root::Parameter(0),
            path: vec![PlaceProjection::Field(0)],
            mutable: true,
        };
        assert_eq!(
            effects[0].replacements[&target],
            BTreeSet::from([Birth::Argument(1)])
        );
        assert_eq!(
            effects[1].replacements[&target],
            BTreeSet::from([Birth::Argument(1)])
        );
        let result = analyze_function_program(
            &module,
            &module.functions[2],
            &[true],
            &effects,
            true,
            &mut b,
            None,
            Some(&program),
        );
        if fresh {
            result.unwrap();
        } else {
            assert!(result.unwrap_err().iter().any(|d| {
                d.message
                    .contains("create result forwards an incoming responsibility")
            }));
        }
    }
}

#[test]
fn replacement_through_capture_keeps_real_fnmut_authority() {
    for kind in [CallableKind::Fn, CallableKind::FnMut] {
        let mut module = fixture::module();
        let address = module.functions[0].locals[0].ty.clone();
        let resource = module.functions[0].locals[1].ty.clone();
        let f = &mut module.functions[0];
        f.parameters = vec![LocalId(1)];
        f.environment = Some(ClosureId(0));
        f.blocks[0].instructions[0].kind = InstructionKind::ReadCapture { index: 0 };
        module.closures.push(ClosureDefinition {
            storage_life: None,
            id: ClosureId(0),
            function: FunctionId(0),
            captures: vec![address],
            parameters: vec![resource],
            result: Type::Unit,
            kind,
            lending_captures: vec![],
        });
        let graphs = vec![(vec![vec![]], vec![true])];
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        b.head_descriptions = true;
        let program = Program::new(&module, &graphs, &mut b).unwrap();
        let result = infer(&module, &graphs, &program, &mut b);
        if kind == CallableKind::Fn {
            assert!(
                result
                    .unwrap_err()
                    .iter()
                    .any(|d| d.message.contains("shared Fn entry cannot replace"))
            );
        } else {
            let effects = result.unwrap();
            assert!(effects[0].borrowed_fields.contains(&0));
            assert!(
                effects[0]
                    .replacements
                    .keys()
                    .any(|at| at.root == origins::Root::Capture(0))
            );
        }
    }
}

#[test]
fn replacement_birth_frontier_has_a_repeatable_live_budget_boundary() {
    let module = with_caller(true);
    let graphs = vec![(vec![vec![]], vec![true]); 3];
    let run = |work| -> CompileResult<()> {
        let limits = Limits {
            max_analysis_steps: work,
            ..Limits::default()
        };
        let mut b = Budget::new(&limits);
        b.head_descriptions = true;
        let program = Program::new(&module, &graphs, &mut b)?;
        let effects = infer(&module, &graphs, &program, &mut b)?;
        for (f, (_, reachable)) in module.functions.iter().zip(&graphs) {
            analyze_function_program(
                &module,
                f,
                reachable,
                &effects,
                true,
                &mut b,
                None,
                Some(&program),
            )?;
        }
        Ok(())
    };
    let (mut lo, mut hi) = (1, Limits::default().max_analysis_steps);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if run(mid).is_ok() {
            hi = mid
        } else {
            lo = mid + 1
        }
    }
    run(lo).unwrap();
    assert!(
        run(lo - 1)
            .unwrap_err()
            .iter()
            .all(|d| d.category == zil_base::ResultCategory::ResourceLimit)
    );
    run(lo).unwrap();
    println!("340 Analyze replacement birth/actual-route frontier N={lo}");
}
