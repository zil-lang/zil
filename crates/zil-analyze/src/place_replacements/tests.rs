use super::*;
use crate::place_replacements::fixture;

#[test]
fn replacement_protocol_retains_real_base_and_rejects_stale_activations() {
    let limits = Limits::default();
    let module = fixture::module();
    let f = &module.functions[0];
    let mut b = Budget::new(&limits);
    let definitions = Definitions::new(&module, f, &mut b).unwrap();
    let schedule = definitions.schedule(f, &[true], &mut b).unwrap();
    assert_eq!(schedule.uses[0][2], vec![ValueId(0)]);
    assert_eq!(schedule.uses[0][3], vec![ValueId(0)]);
    for case in 0..7 {
        let mut module = fixture::module();
        let f = &mut module.functions[0];
        match case {
            0 => {
                f.blocks[0].instructions.remove(1);
            }
            1 => {
                let duplicate = f.blocks[0].instructions[3].clone();
                f.blocks[0].instructions.push(duplicate);
            }
            2 => {
                f.blocks[0].instructions.pop();
            }
            3 => {
                f.blocks[0].instructions[1].result = Some(ValueId(1));
            }
            4 => {
                let InstructionKind::ReservePlaceAddress { path, .. } =
                    &mut f.blocks[0].instructions[1].kind
                else {
                    unreachable!()
                };
                path[0] = PlaceProjection::Field(9);
            }
            _ => f.blocks[0].instructions.insert(
                2,
                Instruction {
                    result: None,
                    kind: if case == 5 {
                        InstructionKind::TaskYield
                    } else {
                        InstructionKind::SuspendCall {
                            function: f.id,
                            arguments: vec![],
                        }
                    },
                    span: zil_base::Span::default(),
                },
            ),
        }
        let mut b = Budget::new(&limits);
        let result = Definitions::new(&module, &module.functions[0], &mut b)
            .and_then(|d| d.schedule(&module.functions[0], &[true], &mut b));
        if case >= 5 {
            let Err(errors) = result else {
                panic!("invalid suspension must be rejected");
            };
            assert!(
                errors.iter().any(|d| d
                    .message
                    .contains("prepared owning replacement cannot suspend")),
                "case{case}"
            );
        } else {
            assert!(result.is_err(), "case{case}");
        }
    }
}

#[test]
fn replacement_actual_origin_does_not_upgrade_a_shared_parent_claim() {
    let limits = Limits::default();
    let mut module = fixture::module();
    let Type::Address { mutable, .. } = &mut module.functions[0].locals[0].ty else {
        unreachable!()
    };
    *mutable = false;
    // The forged SSA row still claims &mut; actual origin comes from the input.
    let facts = origins::module(
        &module,
        &[(vec![vec![]].into(), vec![true].into())],
        false,
        &mut Budget::new(&limits),
    )
    .unwrap();
    assert_eq!(facts[0].values[0].len(), 1);
    assert!(!facts[0].values[0].first().unwrap().mutable);
}

#[test]
fn replacement_protocol_budget_is_paid_before_route_and_activation_clones() {
    let module = fixture::module();
    let mut low = 1;
    let mut high = Limits::default().max_analysis_steps;
    let run = |work| {
        let limits = Limits {
            max_analysis_steps: work,
            ..Limits::default()
        };
        let mut b = Budget::new(&limits);
        Definitions::new(&module, &module.functions[0], &mut b)
            .and_then(|d| d.schedule(&module.functions[0], &[true], &mut b))
    };
    while low < high {
        let middle = low + (high - low) / 2;
        if run(middle).is_ok() {
            high = middle
        } else {
            low = middle + 1
        }
    }
    assert!(run(low).is_ok());
    let failed = run(low - 1).err().unwrap();
    assert!(
        failed
            .iter()
            .all(|d| d.category == zil_base::ResultCategory::ResourceLimit)
    );
    assert!(run(low).is_ok());
    println!("340 Analyze static replacement protocol N={low}");
}
