//! The one explicitly attributed legacy budget migration keeps exact controls.
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit};
const SAMPLE: &str = include_str!("../../../evidence/impl/240/generic-enum-budget/SAMPLE.zil");
#[test]
fn copy_generic_enum_instance_has_exact_attributed_work_and_node_boundaries() {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "copy-generic-enum.zil", SAMPLE);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let expected = zil_expand::expand(&source, &syntax).unwrap();
    for (work, nodes) in [
        (336, Limits::default().max_nodes),
        (Limits::default().max_analysis_steps, 126),
    ] {
        let limits = Limits {
            max_analysis_steps: work,
            max_nodes: nodes,
            ..Limits::default()
        };
        assert_eq!(
            zil_expand::expand_with_map_limits(&source, &syntax, &limits)
                .unwrap()
                .core,
            expected
        );
        let below = if work == 336 {
            Limits {
                max_analysis_steps: 335,
                ..limits.clone()
            }
        } else {
            Limits {
                max_nodes: 125,
                ..limits.clone()
            }
        };
        assert!(
            zil_expand::expand_with_map_limits(&source, &syntax, &below)
                .unwrap_err()
                .iter()
                .all(|e| e.category == ResultCategory::ResourceLimit)
        );
        let fresh = zil_syntax::parse(&source).into_result().unwrap();
        assert_eq!(
            zil_expand::expand_with_map_limits(&source, &fresh, &limits)
                .unwrap()
                .core,
            expected
        );
    }
}
#[test]
fn actual_owned_field_is_rejected_before_generic_identity_is_erased() {
    let text =
        "resource R{n:i32}package P{manages r:R}enum E<T>{V(T)}fn main(){let e=E.V(P{r:R{n:1}})}";
    let source = SourceUnit::new(FileId(0), ModuleId(0), "owner-control.zil", text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    assert!(zil_expand::expand(&source, &syntax).is_err());
}
