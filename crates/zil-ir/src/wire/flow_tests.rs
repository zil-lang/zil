use super::*;
use zil_base::ResultCategory;
fn claim() -> CallableParameterContract {
    CallableParameterContract {
        roles: vec![CallableRole {
            parameter: 0,
            closure: ClosureId(7),
            kind: CallableKind::FnMut,
            hold: vec![0],
            call: vec![0],
            writes: vec![0],
            cleanup: CallableCleanup::Borrowed,
        }],
        traces: CallableTraces::Flow(CallableFlow {
            entry: 0,
            nodes: vec![
                CallableFlowNode {
                    calls: vec![],
                    next: CallableFlowNext::Edges(vec![1, 2, 3]),
                },
                CallableFlowNode {
                    calls: vec![CallEvent {
                        parameter: 0,
                        kind: CallableKind::FnMut,
                    }],
                    next: CallableFlowNext::Edges(vec![0]),
                },
                CallableFlowNode {
                    calls: vec![],
                    next: CallableFlowNext::Exit(CallableExit::Return),
                },
                CallableFlowNode {
                    calls: vec![],
                    next: CallableFlowNext::Exit(CallableExit::Trap),
                },
            ],
        }),
    }
}
fn encoded(c: &CallableParameterContract, limits: &Limits) -> Vec<u8> {
    let mut writer = Writer::new(limits);
    writer.callable_contract(c).unwrap();
    writer.output
}
fn decode(bytes: &[u8], limits: &Limits) -> WireResult<CallableParameterContract> {
    let mut reader = Reader::new(bytes, limits)?;
    let c = reader.callable_contract()?;
    reader.finish()?;
    Ok(c)
}
#[test]
fn flow_roundtrip_keeps_backedges_zero_calls_and_both_exits() {
    let c = claim();
    let limits = Limits::default();
    let bytes = encoded(&c, &limits);
    assert_eq!(decode(&bytes, &limits).unwrap(), c);
    assert!(c.traces.as_finite().is_none());
    for n in 0..bytes.len() {
        assert!(decode(&bytes[..n], &limits).is_err(), "prefix {n}");
    }
}
#[test]
fn each_flow_field_and_representation_changes_contract_identity() {
    let c = claim();
    let original = callable_contract_fingerprint(&c);
    for mutation in 0..8 {
        let mut changed = c.clone();
        let CallableTraces::Flow(flow) = &mut changed.traces else {
            unreachable!()
        };
        match mutation {
            0 => flow.entry = 1,
            1 => flow.nodes[1].calls[0].kind = CallableKind::Fn,
            2 => flow.nodes[1].calls.push(CallEvent {
                parameter: 0,
                kind: CallableKind::FnMut,
            }),
            3 => flow.nodes[0].next = CallableFlowNext::Edges(vec![2, 1, 3]),
            4 => flow.nodes[1].next = CallableFlowNext::Edges(vec![1]),
            5 => flow.nodes[2].next = CallableFlowNext::Exit(CallableExit::Trap),
            6 => flow.nodes.push(flow.nodes[3].clone()),
            _ => {
                changed.traces = CallableTraces::Finite(vec![CallableTrace {
                    calls: vec![CallEvent {
                        parameter: 0,
                        kind: CallableKind::FnMut,
                    }],
                    exit: CallableExit::Return,
                }])
            }
        };
        assert_ne!(
            callable_contract_fingerprint(&changed),
            original,
            "mutation {mutation}"
        );
    }
}
#[test]
fn unknown_flow_variants_and_untrusted_lengths_fail_before_allocation() {
    let limits = Limits::default();
    let bytes = encoded(&claim(), &limits);
    let mut reader = Reader::new(&bytes, &limits).unwrap();
    assert_eq!(reader.u32().unwrap(), 1); // role count
    reader.u32().unwrap();
    reader.u32().unwrap();
    reader.callable_kind().unwrap();
    for _ in 0..3 {
        reader.callable_indices().unwrap();
    }
    reader.u8().unwrap();
    let tag = reader.position;
    assert_eq!(reader.u8().unwrap(), 1);
    reader.u32().unwrap();
    let nodes_count = reader.position;
    assert_eq!(reader.u32().unwrap(), 4);
    reader.u32().unwrap();
    let next_tag = reader.position;
    assert_eq!(reader.u8().unwrap(), 0);
    let edges_count = reader.position;
    let n = reader.u32().unwrap();
    for _ in 0..n {
        reader.u32().unwrap();
    }
    reader.u32().unwrap();
    reader.u32().unwrap(); // actual role parameter for the event
    let mode_tag = reader.position;
    for at in [tag, next_tag, mode_tag] {
        let mut attack = bytes.clone();
        attack[at] = 255;
        assert_eq!(decode(&attack, &limits).unwrap_err().code, "E-WIRE-TAG");
    }
    for at in [nodes_count, edges_count] {
        let mut attack = bytes.clone();
        attack[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(
            decode(&attack, &limits).unwrap_err().category,
            ResultCategory::ResourceLimit
        );
    }
}
#[test]
fn distributed_calls_and_edges_obey_cumulative_contract_limits() {
    let normal = Limits::default();
    for calls in [true, false] {
        let mut c = claim();
        let CallableTraces::Flow(flow) = &mut c.traces else {
            unreachable!()
        };
        if calls {
            flow.nodes[0].calls = vec![
                CallEvent {
                    parameter: 0,
                    kind: CallableKind::FnMut
                };
                3
            ];
            flow.nodes[1].calls = vec![
                CallEvent {
                    parameter: 0,
                    kind: CallableKind::FnMut
                };
                3
            ];
        } else {
            flow.nodes[0].next = CallableFlowNext::Edges(vec![0, 1, 2, 3]);
            flow.nodes[1].next = CallableFlowNext::Edges(vec![0, 1, 2, 3]);
        }
        let small = if calls {
            Limits {
                max_instructions: 5,
                ..normal.clone()
            }
        } else {
            Limits {
                max_analysis_steps: 5,
                ..normal.clone()
            }
        };
        let bytes = encoded(&c, &normal);
        let err = decode(&bytes, &small).unwrap_err();
        assert_eq!(err.category, ResultCategory::ResourceLimit);
        assert!(err.message.contains("cumulative"));
        let err = Writer::new(&small).callable_contract(&c).unwrap_err();
        assert_eq!(err.category, ResultCategory::ResourceLimit);
        assert!(err.message.contains("cumulative"));
    }
    assert_eq!(
        decode(&encoded(&claim(), &normal), &normal).unwrap(),
        claim()
    );
}

#[test]
fn joint_roles_and_event_order_are_transport_identity_not_layout_aliases() {
    let limits = Limits::default();
    let mut c = claim();
    let mut second = c.roles[0].clone();
    second.parameter = 3;
    c.roles.push(second);
    c.traces = CallableTraces::Finite(vec![CallableTrace {
        calls: vec![
            CallEvent {
                parameter: 0,
                kind: CallableKind::FnMut,
            },
            CallEvent {
                parameter: 3,
                kind: CallableKind::FnMut,
            },
        ],
        exit: CallableExit::Return,
    }]);
    let bytes = encoded(&c, &limits);
    assert_eq!(decode(&bytes, &limits).unwrap(), c);
    let identity = callable_contract_fingerprint(&c);
    for mutation in 0..8 {
        let mut changed = c.clone();
        match mutation {
            0 => changed.roles.reverse(),
            1 => {
                changed.roles.pop();
            }
            2 => changed.roles.push(changed.roles[1].clone()),
            3 => changed.roles[1].parameter = 2,
            4 => changed.roles[1].writes.clear(),
            5 => changed.traces.as_finite_mut().unwrap()[0].calls.reverse(),
            6 => changed.traces.as_finite_mut().unwrap()[0].calls[1].parameter = 0,
            _ => changed.traces.as_finite_mut().unwrap()[0].calls[1].kind = CallableKind::Fn,
        }
        assert_ne!(
            callable_contract_fingerprint(&changed),
            identity,
            "mutation {mutation}"
        );
    }
    for length in 0..bytes.len() {
        assert!(decode(&bytes[..length], &limits).is_err());
    }
    let mut bomb = bytes.clone();
    bomb[..4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        decode(&bomb, &limits).unwrap_err().category,
        ResultCategory::ResourceLimit
    );
    let small = Limits {
        max_locals: 1,
        ..limits.clone()
    };
    assert_eq!(
        decode(&bytes, &small).unwrap_err().category,
        ResultCategory::ResourceLimit
    );
    assert_eq!(decode(&bytes, &limits).unwrap(), c);
}
