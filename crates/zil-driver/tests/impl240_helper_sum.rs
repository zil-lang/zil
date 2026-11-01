//! Ordinary Sum data in an existing callable helper keeps real field ownership.
use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::*;
fn expand(text: &str) -> Result<CoreModule, Vec<zil_base::Diagnostic>> {
    let s = SourceUnit::new(FileId(0), ModuleId(0), "helper240.zil", text);
    zil_expand::expand(&s, &zil_syntax::parse(&s).into_result().unwrap())
}
fn checked(text: &str) {
    zil_types::check(
        &expand(text).unwrap(),
        &TargetProfile::host_llvm(),
        &Limits::default(),
    )
    .unwrap();
}
#[test]
fn real_enum_managed_name_binding_remains_owned_in_a_finite_helper() {
    checked(include_str!("impl240_fixtures/callback_param.zil"));
}
#[test]
fn unused_helper_checks_real_marker_and_actual_linear_give() {
    let head = "resource R{n:i32}package P{manages r:R}";
    for (field, body) in [
        ("R", "let p=P{r:give r};return callback()+p.r.n"),
        ("Ref<R>", "let p=P{r:give r};return callback()+p.r.n"),
        (
            "manages Ref<R>",
            "let p=P{r:give r};return callback()+p.r.n",
        ),
        (
            "manages R",
            "let p=P{r:give r};let q=P{r:give r};return callback()+p.r.n",
        ),
    ] {
        let text = format!(
            "{head}enum E{{V({field})}}fn consume<F:Fn()->i32>(value:E,callback:F)->i32{{match value{{E.V(r)=>{{{body}}}}}}}fn main(){{}}"
        );
        let result = expand(&text).and_then(|c| {
            zil_types::check(&c, &TargetProfile::host_llvm(), &Limits::default()).map(|_| c)
        });
        assert!(
            result.is_err(),
            "{field}/{body}: unexpectedly accepted unused invalid definition"
        );
    }
}
#[test]
fn a_plain_resource_descriptor_parameter_cannot_use_the_enum_ownership_route() {
    let c = "resource R{n:i32}package P{manages r:R}fn consume<F:Fn()->i32>(r:R,callback:F)->i32{let p=P{r:give r};return callback()+p.r.n}fn main(){}";
    assert!(expand(c).is_err());
}
#[test]
fn module_declared_field_ownership_crosses_the_real_helper_instantiation() {
    let sources = [
        SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "main.zil",
            "import model;fn consume<F:Fn()->i32>(value:model.E,callback:F)->i32{match value{model.E.V(r)=>{let p=model.wrap(give r);return callback()+model.number(p)}}}fn main()->i32{return consume(model.make(),()=>3)-5}",
        ),
        SourceUnit::new(
            FileId(1),
            ModuleId(1),
            "model.zil",
            "pub resource R{pub n:i32}pub package P{manages r:R}pub enum E{V(manages R)}pub fn make()->E{return E.V(R{n:2})}pub fn wrap(give r:R)->P{return P{r:give r}}pub fn number(p:P)->i32{return p.r.n}",
        ),
    ];
    let syntax = sources
        .iter()
        .map(|s| zil_syntax::parse(s).into_result().unwrap())
        .collect::<Vec<_>>();
    let modules = zil_expand::expand_program_with_limits(
        &sources,
        &syntax.iter().collect::<Vec<_>>(),
        &[
            vec![CoreImport {
                alias: "model".into(),
                module: ModuleId(1),
                span: zil_base::Span::default(),
            }],
            vec![],
        ],
        &Limits::default(),
    )
    .unwrap()
    .into_iter()
    .map(|(m, _)| m)
    .collect::<Vec<_>>();
    let p = CoreProgram {
        root: ModuleId(0),
        source_fingerprint: modules[0].core.source_fingerprint,
        modules,
    };
    zil_types::check_program(&p, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
}

#[test]
fn old_explicit_give_resource_parameter_control_remains_legal() {
    checked(
        "resource R{n:i32}fn consume<F:Fn()->i32>(give r:R,callback:F)->i32{return callback()+r.n}fn main()->i32{let r=R{n:2};return consume(give r,()=>3)-5}",
    );
}
