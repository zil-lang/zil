//! Input-component reservation controls; these raw records do not authorize
//! a task or substitute for source/kernel/native semantic checks.
use super::*;
#[test]
fn task343_classifier_many_blocks_and_low_remaining_are_prepaid() {
    for (blocks, instructions) in [(1, 8), (32, 128)] {
        let input = (0..blocks)
            .map(|b| BasicBlock {
                id: BlockId(b),
                instructions: (0..instructions)
                    .map(|_| Instruction {
                        result: None,
                        kind: InstructionKind::ScopeEnter { scope: ScopeId(0) },
                        span: zil_base::Span::default(),
                    })
                    .collect(),
                terminator: Terminator::Return(None),
            })
            .collect::<Vec<_>>();
        let n = blocks as usize * 2 + blocks as usize * instructions;
        for (amount, ok) in [(n, true), (n - 1, false), (n, true)] {
            let limits = zil_base::Limits {
                max_analysis_steps: amount,
                ..zil_base::Limits::default()
            };
            let mut b = Budget::new(&limits);
            let result = has_await(&input, &mut b);
            assert_eq!(result.is_ok(), ok, "{result:?}");
            if ok {
                assert!(!result.unwrap());
                assert_eq!(b.remaining(), 0);
            } else {
                assert_eq!(
                    result.unwrap_err()[0].category,
                    zil_base::ResultCategory::ResourceLimit
                );
            }
        }
        let limits = zil_base::Limits {
            max_analysis_steps: 1,
            ..zil_base::Limits::default()
        };
        let mut b = Budget::new(&limits);
        assert_eq!(
            has_await(&input, &mut b).unwrap_err()[0].category,
            zil_base::ResultCategory::ResourceLimit
        );
        eprintln!("task343 classifier blocks={blocks} instructions={instructions} N={n}");
    }
}
#[test]
fn task343_carrier_repeated_prefix_and_name_comparisons_use_remaining_ledger() {
    let mut growth = vec![];
    for (rows, width) in [(8, 16), (32, 64)] {
        let empty = || zil_ir::RetainedLifeShape {
            binder_count: 0,
            ports: vec![],
            retained: vec![],
            nominal_arguments: vec![],
        };
        let query = Type::Task {
            result: Box::new(Type::Tuple(vec![Type::I32; width])),
            retained_life: empty(),
        };
        let name = format!("{TASK_RESOURCE_NAME_PREFIX}{}", query.fingerprint());
        let mut resources = (0..rows - 1)
            .map(|i| {
                let mut fields = vec![Type::I32; width];
                fields[width - 1] = Type::Enum {
                    name: format!("Other{i}"),
                    variants: vec![("Only".into(), vec![])],
                };
                let payload = Type::Task {
                    result: Box::new(Type::Tuple(fields)),
                    retained_life: empty(),
                };
                ResourceDefinition {
                    name: format!("{TASK_RESOURCE_NAME_PREFIX}{}", payload.fingerprint()),
                    payload,
                    teardown: None,
                }
            })
            .collect::<Vec<_>>();
        resources.push(ResourceDefinition {
            name: name.clone(),
            payload: query.clone(),
            teardown: None,
        });
        let run = |n| -> CompileResult<(bool, usize)> {
            let limits = zil_base::Limits {
                max_analysis_steps: n,
                ..zil_base::Limits::default()
            };
            let mut b = Budget::new(&limits);
            b.spend(4, "spent task lookup prefix")?;
            let result = matching_resources(&resources, &query, &mut b).map(|n| n == 1)?;
            Ok((result, b.remaining()))
        };
        let before = zil_base::Limits::default().max_analysis_steps;
        let (matched, left) = run(before).unwrap();
        assert!(matched);
        let n = before - left;
        assert_eq!(run(n).unwrap(), (true, 0));
        assert_eq!(
            run(n - 1).unwrap_err()[0].category,
            zil_base::ResultCategory::ResourceLimit
        );
        assert_eq!(run(n).unwrap(), (true, 0));
        assert_eq!(
            run(4).unwrap_err()[0].category,
            zil_base::ResultCategory::ResourceLimit
        );
        growth.push(n);
        eprintln!("task343 carrier rows={rows} width={width} N={n}");
    }
    assert!(growth[1] > growth[0] * 4, "{growth:?}");
}
