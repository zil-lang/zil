//! Source-driven owning replacement through independent nonlocal positions.
use super::*;
const CAPTURE: &str = include_str!("captured-whole.zil");
const MULTIPLE: &str = include_str!("multiple-inputs.zil");
const STORE: &str = include_str!("store-whole.zil");
fn typed(text: &str) -> zil_ir::TypedModule {
    let input = source(text);
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &syntax).unwrap();
    zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap()
}
fn check(text: &str) {
    let module = typed(text);
    let limits = Limits::default();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &module.target, &limits).unwrap();
}
#[test]
fn g139_nonlocal_complete_package_source_raw_wire() {
    for text in [
        CAPTURE,
        MULTIPLE,
        STORE,
        include_str!("two-leaf-helper.zil"),
    ] {
        check(text);
    }
}
