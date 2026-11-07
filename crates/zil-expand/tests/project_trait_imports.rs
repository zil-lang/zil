use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_syntax::ast::{ItemKind, TypeKind};

#[test]
fn raw_hygienic_trait_spelling_cannot_replace_an_actual_import() {
    let sources = [
        SourceUnit::new(FileId(0), ModuleId(0), "main.zil", "fn main()->i32=>0"),
        SourceUnit::new(
            FileId(1),
            ModuleId(1),
            "rules.zil",
            "pub trait Read{fn read(self:&Self)->i32;}",
        ),
        SourceUnit::new(
            FileId(2),
            ModuleId(2),
            "data.zil",
            "pub struct Data{n:i32}impl Fake for Data{fn read()->i32=>.n}",
        ),
    ];
    let mut ast = sources
        .iter()
        .map(|s| zil_syntax::parse(s).into_result().unwrap())
        .collect::<Vec<_>>();
    let implementation = ast[2]
        .items
        .iter_mut()
        .find_map(|item| {
            if let ItemKind::Impl(i) = &mut item.kind {
                Some(i)
            } else {
                None
            }
        })
        .unwrap();
    let ty = implementation.trait_type.as_mut().unwrap();
    let TypeKind::Named { path, .. } = &mut ty.kind else {
        panic!("nominal fixture")
    };
    *path = vec!["$module1_Read".into()];
    let errors = zil_expand::expand_program_with_limits(
        &sources,
        &ast.iter().collect::<Vec<_>>(),
        &[vec![], vec![], vec![]],
        &Limits::default(),
    )
    .unwrap_err();
    assert!(
        errors.iter().any(|e| e.phase == Phase::Expand
            && e.code == "E-MODULE-MEMBER"
            && e.message == "trait implementation declaration must belong to an imported module"),
        "{errors:#?}"
    );
}

#[test]
fn static_reserved_exclusion_uses_the_exact_module_prefix_without_weakening_identity() {
    for (name, reserved) in [
        ("$module17_Send", true),
        ("$module0_FnOnce", true),
        ("$module_Send", false),
        ("$modulex_Send", false),
        ("$module1x_Send", false),
        ("$module7_User_Send", false),
        ("User_Send", false),
    ] {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "raw.zil",
            "trait Tag{}fn main()->i32=>0",
        );
        let mut ast = zil_syntax::parse(&source).into_result().unwrap();
        let ItemKind::Trait(t) = &mut ast.items[0].kind else {
            panic!("trait fixture")
        };
        t.name = name.into();
        let result = zil_expand::expand_with_map_limits(&source, &ast, &Limits::default());
        if reserved {
            let errors = result.unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|e| e.category == ResultCategory::SafeReject
                        && e.code == "E-TRAIT-RESERVED"),
                "{name}: {errors:#?}"
            );
        } else {
            let result = result.unwrap();
            assert!(
                result.core.unsupported.is_empty(),
                "{name}: {:?}",
                result.core.unsupported
            );
        }
    }
}
