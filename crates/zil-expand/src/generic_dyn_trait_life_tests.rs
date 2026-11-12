//! Source producers are checked independently from the runtime port kernels.
use super::*;
use zil_base::{FileId, ModuleId, SourceUnit};
fn expand(text: &str) -> CompileResult<zil_ir::CoreModule> {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "dyn-trait-life.zil", text);
    let parsed = zil_syntax::parse(&source).into_result()?;
    crate::expand_with_map_limits_target_context(
        &source,
        &parsed,
        &Limits::default(),
        &zil_base::TargetProfile::default(),
        crate::source_context::SourceIdentityContext::ClosedSnapshot {
            root: source.module_id,
            aliases: &BTreeMap::new(),
            declarations: crate::source_context::DeclarationPlacement::Original,
        },
    )
    .map(|expanded| expanded.core)
}
const SET: &str = r#"
struct C<life a,life b>{one:&'a i32,two:&'b i32}
struct P<life p>{pointer:&'p i32}
trait Set<life a>{fn set(self:&mut Self,next:&'a i32);}
impl<life p> Set<'p> for P<'p>{fn set(self:&mut Self,next:&'p i32){self.pointer=next}}
"#;
#[test]
fn trait_life_preserves_same_independent_and_merged_context_classes() {
    let module = expand(&format!(r#"{SET}
fn first<life x,life y>(p:P<'x>,anchor:C<'x,'y>)->DynCreateError!OwnedDyn<dyn Set<'x>,C<'x,'y>>{{return owned_dyn::<dyn Set<'x>,C<'x,'y>>(p)}}
fn second<life x,life y>(p:P<'y>,anchor:C<'x,'y>)->DynCreateError!OwnedDyn<dyn Set<'y>,C<'x,'y>>{{return owned_dyn::<dyn Set<'y>,C<'x,'y>>(p)}}
fn same<life x>(p:P<'x>,anchor:C<'x,'x>)->DynCreateError!OwnedDyn<dyn Set<'x>,C<'x,'x>>{{return owned_dyn::<dyn Set<'x>,C<'x,'x>>(p)}}
fn main()->i32{{return 0}}
"#)).unwrap();
    let interfaces: Vec<_> = module
        .dyn_interfaces
        .iter()
        .filter(|i| i.context.is_some())
        .collect();
    assert_eq!(interfaces.len(), 3);
    for (count, class) in [(2, 0), (2, 1), (1, 0)] {
        let i = interfaces
            .iter()
            .find(|i| {
                i.retained_life.binder_count == count
                    && i.application.actuals
                        == [zil_ir::DynGenericActual::Life(
                            zil_ir::StorageBinder::Fixed(class),
                        )]
            })
            .unwrap();
        let t = module
            .dyn_vtables
            .iter()
            .find(|t| t.interface == i.id)
            .unwrap();
        assert_eq!(
            t.actuals,
            [zil_ir::DynGenericActual::Life(
                zil_ir::StorageBinder::Fixed(class)
            )]
        );
        assert_eq!(t.self_classes, [zil_ir::StorageBinder::Fixed(class)]);
    }
}
#[test]
fn trait_life_phantom_actual_is_preserved_without_inventing_a_port() {
    let module = expand(r#"
struct C<life a,life phantom>{one:&'a i32}
struct P<life p>{pointer:&'p i32}
trait Read<life a,life phantom>{fn read(self:&Self)->i32;}
impl<life p,life q> Read<'p,'q> for P<'p>{fn read(self:&Self)->i32{return *self.pointer}}
fn erase<life x,life y>(p:P<'x>,anchor:C<'x,'y>)->DynCreateError!OwnedDyn<dyn Read<'x,'y>,C<'x,'y>>{return owned_dyn::<dyn Read<'x,'y>,C<'x,'y>>(p)}
fn main()->i32{return 0}
"#).unwrap();
    let i = module
        .dyn_interfaces
        .iter()
        .find(|i| i.context.is_some())
        .unwrap();
    assert_eq!(i.retained_life.binder_count, 2);
    assert_eq!(i.retained_life.ports.len(), 1);
    let expected = vec![
        zil_ir::DynGenericActual::Life(zil_ir::StorageBinder::Fixed(0)),
        zil_ir::DynGenericActual::Life(zil_ir::StorageBinder::Fixed(1)),
    ];
    assert_eq!(i.application.actuals, expected);
    let t = module
        .dyn_vtables
        .iter()
        .find(|t| t.interface == i.id)
        .unwrap();
    assert_eq!(t.actuals, expected);
    assert_eq!(t.self_classes, [zil_ir::StorageBinder::Fixed(0)]);
    let d = module
        .dyn_declarations
        .iter()
        .find(|d| d.id == t.implementation)
        .unwrap();
    assert_eq!(
        d.implemented_trait.as_ref().unwrap().arguments,
        [
            zil_ir::DynPatternActual::LifeParameter(0),
            zil_ir::DynPatternActual::LifeParameter(1)
        ]
    );
}
#[test]
fn trait_life_wrong_actual_and_incomplete_context_are_rejected() {
    for suffix in [
        r#"fn bad<life x,life y>(p:P<'x>,anchor:C<'x,'y>)->DynCreateError!OwnedDyn<dyn Set<'y>,C<'x,'y>>{return owned_dyn::<dyn Set<'y>,C<'x,'y>>(p)}"#,
        r#"struct Narrow<life x>{one:&'x i32}fn bad<life x,life y>(p:P<'x>,anchor:C<'x,'y>)->DynCreateError!OwnedDyn<dyn Set<'y>,Narrow<'x>>{return owned_dyn::<dyn Set<'y>,Narrow<'x>>(p)}"#,
        r#"fn bad<life x>(p:P<'x>)->DynCreateError!OwnedDyn<dyn Set<i32>,C<'x,'x>>{return owned_dyn::<dyn Set<i32>,C<'x,'x>>(p)}"#,
        r#"fn bad<life x>(p:P<'x>)->DynCreateError!OwnedDyn<dyn Set<'missing>,C<'x,'x>>{return owned_dyn::<dyn Set<'missing>,C<'x,'x>>(p)}"#,
    ] {
        let source = format!("{SET}\n{suffix}\nfn main()->i32{{return 0}}");
        let errors = expand(&source).unwrap_err();
        assert!(
            errors
                .iter()
                .all(|e| e.category != zil_base::ResultCategory::ResourceLimit),
            "{errors:?}"
        );
    }
}
#[test]
fn trait_life_unused_impl_signature_is_still_checked_in_its_own_scope() {
    let prefix = r#"
struct P<life p,life q>{one:&'p i32,two:&'q i32}
trait Set<life a>{fn set(self:&mut Self,next:&'a i32);}
"#;
    let good = format!(
        "{prefix}impl<life p,life q> Set<'p> for P<'p,'q>{{fn set(self:&mut Self,next:&'p i32){{self.one=next}}}}fn main()->i32{{return 0}}"
    );
    expand(&good).unwrap();
    for bad in [
        good.replace("next:&'p i32", "next:&'q i32"),
        good.replace("Set<'p> for", "Set<'q> for"),
        good.replace("next:&'p i32", "next:&'missing i32"),
    ] {
        assert!(expand(&bad).is_err(), "unexpected acceptance: {bad}");
    }
}
#[test]
fn trait_life_keeps_type_and_life_actuals_in_original_order() {
    let m = expand(r#"
struct C<life x>{one:&'x i32}
struct P<life p>{pointer:&'p i32}
trait Set<T:Copy,life a,U:Copy>{fn set(self:&mut Self,next:&'a T,flag:U);}
impl<life p> Set<i32,'p,bool> for P<'p>{fn set(self:&mut Self,next:&'p i32,flag:bool){self.pointer=next}}
fn erase<life x>(p:P<'x>)->DynCreateError!OwnedDyn<dyn Set<i32,'x,bool>,C<'x>>{return owned_dyn::<dyn Set<i32,'x,bool>,C<'x>>(p)}
fn main()->i32{return 0}
"#).unwrap();
    let i = m
        .dyn_interfaces
        .iter()
        .find(|i| i.context.is_some())
        .unwrap();
    assert_eq!(
        i.application.actuals,
        [
            zil_ir::DynGenericActual::Type(zil_ir::Type::I32),
            zil_ir::DynGenericActual::Life(zil_ir::StorageBinder::Fixed(0)),
            zil_ir::DynGenericActual::Type(zil_ir::Type::Bool)
        ]
    );
}
#[test]
fn trait_life_cross_module_keeps_real_trait_and_impl_origins() {
    let sources = [
        SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "main.zil",
            r#"
import rules;
struct C<life x>{one:&'x i32}
struct P<life p>{pointer:&'p i32}
impl<life p> rules.Set<'p> for P<'p>{fn set(self:&mut Self,next:&'p i32){self.pointer=next}}
fn erase<life x>(p:P<'x>)->DynCreateError!OwnedDyn<dyn rules.Set<'x>,C<'x>>{return owned_dyn::<dyn rules.Set<'x>,C<'x>>(p)}
fn main()->i32{return 0}
"#,
        ),
        SourceUnit::new(
            FileId(1),
            ModuleId(1),
            "rules.zil",
            "pub trait Set<life a>{fn set(self:&mut Self,next:&'a i32);}",
        ),
    ];
    let syntax: Vec<_> = sources
        .iter()
        .map(|s| zil_syntax::parse(s).into_result().unwrap())
        .collect();
    let modules: Vec<_> = syntax.iter().collect();
    let imports = [
        vec![zil_ir::CoreImport {
            alias: "rules".into(),
            module: ModuleId(1),
            span: syntax[0].items[0].span,
        }],
        Vec::new(),
    ];
    let expanded = crate::expand_program_with_limits_target_context(
        &sources,
        &modules,
        &imports,
        &Limits::default(),
        &zil_base::TargetProfile::default(),
        crate::source_context::SourceIdentityContext::ClosedSnapshot {
            root: ModuleId(0),
            aliases: &BTreeMap::from([(vec!["rules".into()], ModuleId(1))]),
            declarations: crate::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap();
    let interface = expanded
        .iter()
        .flat_map(|(m, _)| &m.core.dyn_interfaces)
        .find(|i| i.context.is_some())
        .unwrap();
    assert_eq!(interface.application.declaration.module, ModuleId(1));
    let table = expanded
        .iter()
        .flat_map(|(m, _)| &m.core.dyn_vtables)
        .find(|t| t.interface == interface.id)
        .unwrap();
    assert_eq!(table.implementation.module, ModuleId(0));
}
#[test]
fn trait_life_inventory_preserves_unrelated_resource_drop_and_borrowed_factory() {
    let m = expand(include_str!(
        "../../../tests/fixtures/returned-owning-holders/forward-owned-dyn.zil"
    ))
    .unwrap();
    for table in &m.dyn_vtables {
        let d = m
            .dyn_declarations
            .iter()
            .find(|d| d.id == table.implementation)
            .unwrap();
        assert!(d.implemented_trait.is_some());
    }
    assert!(!m.dyn_vtables.is_empty());
}
#[test]
fn trait_life_phantom_argument_uses_actual_entry_invocation_relation() {
    let m = expand(r#"
struct C<life a,life b>{one:&'a i32}
struct P<life p>{pointer:&'p i32}
trait Pick<life a,life b>{fn pick(self:&Self,next:&'b i32)->&'b i32;}
impl<life p,life q> Pick<'p,'q> for P<'p>{fn pick(self:&Self,next:&'q i32)->&'q i32{return next}}
fn erase<life x,life y>(p:P<'x>,anchor:C<'x,'y>)->DynCreateError!OwnedDyn<dyn Pick<'x,'y>,C<'x,'y>>{return owned_dyn::<dyn Pick<'x,'y>,C<'x,'y>>(p)}
fn main()->i32{return 0}
"#).unwrap();
    let i = m
        .dyn_interfaces
        .iter()
        .find(|i| i.context.is_some())
        .unwrap();
    let slot = &i.slots[0];
    let life = slot.storage_life.as_ref().unwrap();
    for base in [
        zil_ir::StorageBase::Parameter(1),
        zil_ir::StorageBase::Result,
    ] {
        assert!(
            life.bindings
                .iter()
                .any(|b| b.position.base == base && b.binder == zil_ir::StorageBinder::Fixed(1))
        );
    }
    let zil_ir::DynResultSource::Borrow { sources, .. } = slot.contract.result.as_ref().unwrap()
    else {
        panic!("missing borrow result")
    };
    assert_eq!(
        sources,
        &[zil_ir::StoragePosition {
            base: zil_ir::StorageBase::Parameter(1),
            path: Vec::new()
        }]
    );
    let table = m.dyn_vtables.iter().find(|t| t.interface == i.id).unwrap();
    let entry = m
        .functions
        .iter()
        .find(|f| f.id == table.entries[0].function)
        .unwrap();
    let entry_life = entry.storage_life.as_ref().unwrap();
    let argument = entry_life
        .bindings
        .iter()
        .find(|b| b.position.base == zil_ir::StorageBase::Parameter(1))
        .unwrap();
    assert!(matches!(
        argument.binder,
        zil_ir::StorageBinder::Invocation(_)
    ));
    assert!(
        entry_life
            .bindings
            .iter()
            .any(|b| b.position.base == zil_ir::StorageBase::Result && b.binder == argument.binder)
    );
    println!("phantom pick actual entry relation: {entry_life:?}");
}
#[test]
fn trait_life_real_owner_setter_call_survives_helper_and_factory() {
    let m = expand(include_str!(
        "../../../tests/fixtures/dyn-context/trait-life-setter.zil"
    ))
    .unwrap();
    assert!(m.functions.iter().any(|f| f.name == "update"));
    assert!(
        m.dyn_interfaces
            .iter()
            .any(|i| i.context.is_some() && i.slots.len() == 2)
    );
    assert!(!m.dyn_vtables.is_empty());
}
#[test]
fn trait_life_phantom_self_actual_cannot_be_relabelled_by_same_layout() {
    let source = r#"
struct C<life a,life b>{one:&'a i32}
struct P<life p,life phantom>{pointer:&'p i32}
trait Read<life a,life phantom>{fn read(self:&Self)->i32;}
impl<life p,life q> Read<'p,'q> for P<'p,'q>{fn read(self:&Self)->i32{return *self.pointer}}
fn erase<life x,life y>(p:P<'x,'y>,anchor:C<'x,'y>)->DynCreateError!OwnedDyn<dyn Read<'x,'y>,C<'x,'y>>{return owned_dyn::<dyn Read<'x,'y>,C<'x,'y>>(p)}
fn main()->i32{return 0}
"#;
    expand(source).unwrap();
    for bad in [
        source.replace("dyn Read<'x,'y>", "dyn Read<'x,'x>"),
        source.replace("dyn Read<'x,'y>", "dyn Read<'y,'x>"),
    ] {
        let errors = expand(&bad).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("no actual lifetime impl witness")),
            "{errors:?}"
        );
    }
}

#[test]
fn trait_life_error_wrapped_owner_result_keeps_actual_input_class_relation() {
    use zil_ir::{LifeTypeProjection, PlaceProjection, StorageBase};
    let m = expand(include_str!(
        "../../../tests/fixtures/dyn-context/trait-life-setter.zil"
    ))
    .unwrap();
    let create = m.functions.iter().find(|f| f.name == "create").unwrap();
    let life = create.storage_life.as_ref().unwrap();
    let input = life
        .bindings
        .iter()
        .find(|b| b.position.base == StorageBase::Parameter(0) && b.position.path.is_empty())
        .unwrap();
    let output = life
        .classes
        .iter()
        .find(|c| {
            c.position.base == StorageBase::Result
                && c.position.path == [LifeTypeProjection::Place(PlaceProjection::ErrorArm(false))]
                && c.position.class == 0
        })
        .unwrap();
    assert_eq!(output.binder, input.binder);
    let update = m.functions.iter().find(|f| f.name == "update").unwrap();
    let life = update.storage_life.as_ref().unwrap();
    let argument = life
        .bindings
        .iter()
        .find(|b| b.position.base == StorageBase::Parameter(1))
        .unwrap();
    let receiver = life
        .classes
        .iter()
        .find(|c| {
            c.position.base == StorageBase::Parameter(0)
                && c.position.path == [LifeTypeProjection::Pointee]
                && c.position.class == 0
        })
        .unwrap();
    assert_eq!(receiver.binder, argument.binder);
}
