//! Author checks for the public source-only Product identity contract.
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::*;
const ORIGINAL: &str = include_str!("impl237_fixtures/original235.zil");
fn core(text: &str) -> CoreModule {
    let u = SourceUnit::new(FileId(0), ModuleId(0), "types237.zil", text);
    let ast = zil_syntax::parse(&u).into_result().unwrap();
    zil_expand::expand(&u, &ast).unwrap()
}
fn accepted(text: &str) -> TypedModule {
    zil_types::check(&core(text), &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}
fn reject(c: &CoreModule) {
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    for r in [
        zil_types::check(c, &target, &limits),
        zil_types::check_program(
            &CoreProgram {
                root: c.module_id,
                source_fingerprint: c.source_fingerprint,
                modules: vec![ProgramModule {
                    core: c.clone(),
                    imports: vec![],
                    exports: vec![],
                }],
            },
            &target,
            &limits,
        )
        .map(|p| p.module),
    ] {
        let errors =
            r.expect_err("malformed public Product metadata must fail independently in Types");
        assert!(
            errors.iter().all(|d| d.phase == Phase::Types
                && matches!(
                    d.category,
                    ResultCategory::SafeReject | ResultCategory::Unsupported
                )),
            "{errors:#?}"
        );
    }
}
#[test]
fn original235_generic_wrapper_reaches_types() {
    accepted(ORIGINAL);
}
#[test]
fn direct_and_retained_application_have_one_complete_nominal_identity() {
    accepted(&ORIGINAL.replace(
        "let root=wrap(give items)",
        "let root:Root<Item<F>>=wrap(give items)",
    ));
}
#[test]
fn unused_owner_and_recipe_mutations_are_rejected() {
    let input = core(ORIGINAL);
    assert!(!input.aggregate_templates.is_empty());
    assert!(!input.aggregate_applications.is_empty());
    for kind in [
        "owner",
        "owner-slot",
        "family",
        "header-field",
        "recipe-field",
        "duplicate-application",
        "duplicate-family",
    ] {
        let mut c = input.clone();
        let a = c
            .aggregate_applications
            .iter()
            .position(|a| {
                a.arguments
                    .iter()
                    .any(|a| matches!(a, CoreAggregateArgument::Parameter { .. }))
            })
            .unwrap();
        match kind {
            "owner" => {
                c.aggregate_applications[a].owner.as_mut().unwrap().function =
                    zil_base::NodeId(u32::MAX)
            }
            "owner-slot" => {
                c.aggregate_applications[a].arguments[0] =
                    CoreAggregateArgument::Parameter { index: 1 }
            }
            "family" => c.aggregate_applications[a].template = zil_base::NodeId(u32::MAX),
            "header-field" => {
                let id = c.aggregate_applications[a].aggregate;
                c.aggregates.iter_mut().find(|d| d.id == id).unwrap().fields[0].public ^= true;
            }
            "recipe-field" => c.aggregate_templates[0].fields[0].name.push('x'),
            "duplicate-application" => c
                .aggregate_applications
                .push(c.aggregate_applications[a].clone()),
            "duplicate-family" => c.aggregate_templates.push(c.aggregate_templates[0].clone()),
            _ => unreachable!(),
        }
        reject(&c);
    }
}
