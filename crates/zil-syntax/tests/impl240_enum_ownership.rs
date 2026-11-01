use zil_base::{FileId, ModuleId, SourceUnit};
use zil_syntax::{ast::*, parse};
#[test]
fn ownership_marker_is_an_enum_field_property() {
    let text = "enum E{Nil,Several(i32,manages R,manages Store<P>,P)}";
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "ownership240.zil", text);
    let module = parse(&unit).into_result().unwrap();
    let ItemKind::Enum(e) = &module.items[0].kind else {
        panic!("enum")
    };
    let VariantFields::Tuple(fields) = &e.variants[1].fields else {
        panic!("tuple variant")
    };
    assert_eq!(
        fields.iter().map(|f| f.manages).collect::<Vec<_>>(),
        vec![false, true, true, false]
    );
    assert!(matches!(&fields[1].ty.kind,TypeKind::Named{path,..} if path==&["R"]));
    let cloned = e.clone();
    assert_eq!(cloned, e.clone());
}
#[test]
fn managed_marker_does_not_become_a_general_type_constructor() {
    let unit = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "type240.zil",
        "fn invalid(value:manages R){}",
    );
    assert!(parse(&unit).into_result().is_err());
}
