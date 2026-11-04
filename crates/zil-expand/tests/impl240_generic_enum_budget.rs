//! Attributed 240/322 legacy budget migrations preserve exact source controls.
//! Pre322 work336/nodes126; paid whole-AST life validation yields403/165.
#[path = "../test-support/budget_boundary.rs"]
mod budget_boundary;
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit};
const SAMPLE: &str = include_str!("../../../tests/fixtures/budgets/generic-enum.zil");
#[test]
fn copy_generic_enum_instance_has_exact_attributed_work_and_node_boundaries() {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "copy-generic-enum.zil", SAMPLE);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let expected = zil_expand::expand(&source, &syntax).unwrap();
    let work_n = budget_boundary::minimum(true, |limits| {
        zil_expand::expand_with_map_limits(&source, &syntax, limits)
    });
    let node_n = budget_boundary::minimum(false, |limits| {
        zil_expand::expand_with_map_limits(&source, &syntax, limits)
    });
    eprintln!("generic Enum work={work_n} nodes={node_n}");
    for (work, nodes) in [
        (work_n, Limits::default().max_nodes),
        (Limits::default().max_analysis_steps, node_n),
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
        let below = if work == work_n {
            Limits {
                max_analysis_steps: work_n - 1,
                ..limits.clone()
            }
        } else {
            Limits {
                max_nodes: node_n - 1,
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
    assert_eq!((work_n, node_n), (468, 201));
}
#[test]
fn actual_owned_field_is_rejected_before_generic_identity_is_erased() {
    let text =
        "resource R{n:i32}package P{manages r:R}enum E<T>{V(T)}fn main(){let e=E.V(P{r:R{n:1}})}";
    let source = SourceUnit::new(FileId(0), ModuleId(0), "owner-control.zil", text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    assert!(zil_expand::expand(&source, &syntax).is_err());
}
