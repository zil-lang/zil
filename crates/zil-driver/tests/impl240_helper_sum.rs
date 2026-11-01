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

#[test]
fn actual_give_resource_results_retain_ownership_through_local_bindings() {
    let source = include_str!("impl240_fixtures/callback_param.zil");
    for transfer in [
        "let saved=give r;let p=P{r:give saved}",
        "let saved=(give (r));let p=P{r:give saved}",
    ] {
        checked(&source.replace("let p=P{r:give r}", transfer));
    }
    let unused = source.replace(
        "let p=P{r:give r}",
        "let saved=give r;let p=P{r:give saved}",
    );
    checked(unused.split("fn main").next().unwrap());
    for transfer in [
        "let saved=r;let p=P{r:give saved}",
        "let saved=give r;let p=P{r:give r}",
        "let saved=give r;let p=P{r:give saved};let again=P{r:give saved}",
    ] {
        assert!(
            expand(&source.replace("let p=P{r:give r}", transfer)).is_err(),
            "{transfer}"
        );
    }
}
#[test]
fn managed_store_pattern_relay_preserves_an_incoming_carrier() {
    checked(
        "resource R{n:i32}package P{manages r:R}package Bag{manages values:Store<P>}enum E{V(manages Store<P>)}fn consume<F:Fn()->i32>(value:E,cb:F)->i32{match value{E.V(items)=>{let saved=give items;let bag=Bag{values:give saved};return cb()+i32(store_len(bag.values))}}}fn main()->i32{let items:Store<P>=store_new();return consume(E.V(give items),()=>3)-3}",
    );
}

#[test]
fn concrete_enum_field_uses_its_declaration_scope_when_helper_binder_has_same_name() {
    let source = include_str!("impl240_fixtures/resource_named_f.zil");
    checked(source);
    for body in [
        "let saved=give a;sink(give saved);",
        "let observed=a.n;sink(give a);",
        "let description=a;let observed=description.n;sink(give a);",
        "let description=a;let alias=description;let observed=alias.n;sink(give a);",
        "let saved=give a;let description=saved;let observed=description.n;sink(give saved);",
    ] {
        let actual = source.replace("sink(give a);", body);
        checked(&actual);
        checked(actual.split("fn main").next().unwrap());
    }
    for body in [
        "let saved=a;sink(give saved);",
        "let saved:F=give a;sink(give saved);",
        "sink(give a);sink(give a);",
        "let observed=callback.n;sink(give a);",
        "sink(give callback);sink(give a);",
        "let description=a;description();sink(give a);",
        "let saved=give a;let description=saved;sink(give description);",
        "let saved=give a;sink(give saved);let observed=saved.n;",
    ] {
        assert!(
            expand(&source.replace("sink(give a);", body)).is_err(),
            "{body}"
        );
    }
    for field in ["F", "Ref<F>", "manages Ref<F>"] {
        let invalid = source.replace("manages F", field);
        let actual = invalid.split("fn main").next().unwrap();
        assert!(expand(actual).is_err(), "{field}");
    }
}

#[test]
fn unused_helpers_cannot_duplicate_or_rematch_complete_owned_sums() {
    for declarations in [
        "resource R{n:i32}enum E{V(manages R)}",
        "resource R{n:i32}package P{manages r:R}enum E{V(P)}",
        "resource R{n:i32}enum Inner{V(manages R)}enum E{V(Inner)}",
    ] {
        for ty in ["E", "void!E"] {
            let source = format!(
                "{declarations}fn duplicate<F:Fn()->i32>(cb:F,e:{ty})->i32{{let first=e;let second=e;return cb()}}fn main(){{}}"
            );
            assert!(expand(&source).is_err(), "{source}");
        }
    }
    let rematch = "resource R{n:i32}package P{manages r:R}enum E{V(manages R)}fn duplicate<F:Fn()->i32>(cb:F,e:E)->i32{match e{E.V(r)=>{let first=P{r:give r};}}match e{E.V(r)=>{let second=P{r:give r};}}return cb()}fn main(){}";
    assert!(expand(rematch).is_err());
    checked(
        "enum E{V(i32)}fn duplicate<F:Fn()->i32>(cb:F,e:E)->i32{let first=e;let second=e;return cb()}fn main(){}",
    );
}
