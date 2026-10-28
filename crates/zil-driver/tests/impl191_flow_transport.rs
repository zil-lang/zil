//! Public graph claims remain untrusted when runtime helper semantics exist.
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::{
    CallableExit, CallableFlow, CallableFlowNext, CallableFlowNode, CallableKind, CallableTraces,
    CandidatePlan, TypedModule,
};
const SOURCE: &str = "fn twice<F:Fn()->i32>(f:&F)->i32{return f()+f()}fn main()->i32{let n=7;let f=()=>n;return twice(&f)-14}";
fn fixture() -> (TypedModule, CandidatePlan) {
    let u = SourceUnit::new(FileId(0), ModuleId(0), "flow191.zil", SOURCE);
    let a = zil_syntax::parse(&u).into_result().unwrap();
    let c = zil_expand::expand(&u, &a).unwrap();
    let m = zil_types::check(&c, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let p = zil_analyze::analyze(&m, &Limits::default()).unwrap();
    zil_verify::verify_module(m.clone(), p.clone(), &m.target, &Limits::default()).unwrap();
    (m, p)
}
fn flow(m: &mut TypedModule) -> &mut CallableFlow {
    let c = m
        .functions
        .iter_mut()
        .find_map(|f| f.callable_contract.as_mut())
        .unwrap();
    if !matches!(c.traces, CallableTraces::Flow(_)) {
        c.traces = CallableTraces::Flow(CallableFlow {
            entry: 0,
            nodes: vec![
                CallableFlowNode {
                    calls: vec![],
                    next: CallableFlowNext::Edges(vec![1, 2]),
                },
                CallableFlowNode {
                    calls: vec![zil_ir::CallEvent {
                        parameter: 0,
                        kind: CallableKind::Fn,
                    }],
                    next: CallableFlowNext::Edges(vec![0]),
                },
                CallableFlowNode {
                    calls: vec![],
                    next: CallableFlowNext::Exit(CallableExit::Return),
                },
            ],
        });
    }
    let CallableTraces::Flow(f) = &mut c.traces else {
        unreachable!()
    };
    f
}
#[test]
fn flow_transport_roundtrip_does_not_grant_analysis_or_verification() {
    let (mut m, mut p) = fixture();
    flow(&mut m);
    p.module_fingerprint = m.fingerprint();
    let limits = Limits::default();
    let bytes = zil_ir::wire::encode_module_candidate(&m, &p, &limits).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap(),
        (m.clone(), p.clone())
    );
    for (phase, errors) in [
        (
            Phase::Analyze,
            zil_analyze::analyze(&m, &limits).unwrap_err(),
        ),
        (
            Phase::Verify,
            zil_verify::verify_module(m.clone(), p, &m.target, &limits).unwrap_err(),
        ),
        (
            Phase::Verify,
            zil_verify::verify_module_bytes(&bytes, &m.target, &limits).unwrap_err(),
        ),
    ] {
        assert!(
            errors.iter().any(|d| d.phase == phase
                && d.category == ResultCategory::SafeReject
                && d.code
                    == if phase == Phase::Analyze {
                        "E-ANALYZE-IR"
                    } else {
                        "E-VERIFY-CALLABLE-CONTRACT"
                    }),
            "{errors:?}"
        );
    }
}
#[test]
fn raw_graph_bounds_precede_stale_identity_and_fresh_retry_is_unchanged() {
    let (original, plan) = fixture();
    for mutation in 0..3 {
        let mut m = original.clone();
        let f = flow(&mut m);
        let limits = match mutation {
            0 => {
                f.nodes.resize(
                    129,
                    CallableFlowNode {
                        calls: vec![],
                        next: CallableFlowNext::Exit(CallableExit::Return),
                    },
                );
                Limits {
                    max_nodes: 128,
                    ..Limits::default()
                }
            }
            1 => {
                f.nodes[0].calls = vec![
                    zil_ir::CallEvent {
                        parameter: 0,
                        kind: CallableKind::Fn
                    };
                    40
                ];
                f.nodes[1].calls = vec![
                    zil_ir::CallEvent {
                        parameter: 0,
                        kind: CallableKind::Fn
                    };
                    40
                ];
                Limits {
                    max_instructions: 64,
                    ..Limits::default()
                }
            }
            _ => {
                f.nodes[0].next = CallableFlowNext::Edges(vec![0; 2001]);
                Limits {
                    max_analysis_steps: 2000,
                    ..Limits::default()
                }
            }
        };
        let analysis = zil_analyze::analyze(&m, &limits).unwrap_err();
        let verified =
            zil_verify::verify_module(m.clone(), plan.clone(), &m.target, &limits).unwrap_err();
        for errors in [analysis, verified] {
            assert_eq!(
                errors[0].category,
                ResultCategory::ResourceLimit,
                "{mutation}: {errors:?}"
            );
            assert!(errors[0].message.contains("flow"), "{mutation}: {errors:?}");
        }
    }
    let fresh = zil_analyze::analyze(&original, &Limits::default()).unwrap();
    assert_eq!(fresh, plan);
    zil_verify::verify_module(
        original.clone(),
        fresh,
        &original.target,
        &Limits::default(),
    )
    .unwrap();
}
