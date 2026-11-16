use zil_base::{FileId, ModuleId, Phase, SourceUnit};
use zil_ir::{LifeNominalActual, LifeParameterKind, StorageBase, Type};

fn expand(text: &str) -> Result<zil_ir::CoreModule, Vec<zil_base::Diagnostic>> {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "option-borrowed-input.zil", text);
    let parsed = zil_syntax::parse(&source).into_result().unwrap();
    zil_expand::expand(&source, &parsed)
}

#[test]
fn borrowed_option_input_keeps_joint_actual_and_complete_variants() {
    // Keep the original failure, including identity's whole Option input and
    // result and the unused result binding. No Holder or payload-only surrogate.
    let original = "fn identity<life a>(x:Option<&'a i32>)->Option<&'a i32>{return x}\nfn run<life a>(value:&'a i32)->i32{let result=identity(Option::<&'a i32>.Some(value));return 0}\nfn main()->i32{let value=42;return run(&value)}\n";
    for text in [
        original.to_owned(),
        original.replace("let result=", "let result:Option<&'a i32>="),
        original.replace("Option::<&'a i32>.Some(value)", "Option.Some(value)"),
        original.replace("Option::<&'a i32>.Some(value)", "Option.None()"),
        original.replace("Option::<&'a i32>.Some(value)", "Option::<&'a i32>.None()"),
        original.replace(
            "identity(Option::<&'a i32>.Some(value))",
            "identity(identity(Option::<&'a i32>.Some(value)))",
        ),
        original
            .replace(
                "fn run<life a>(value:&'a i32)",
                "fn run<life caller>(value:&'caller i32)",
            )
            .replace(
                "Option::<&'a i32>.Some(value)",
                "Option::<&'caller i32>.Some(value)",
            ),
    ] {
        let core = expand(&text).unwrap_or_else(|errors| panic!("{text}: {errors:?}"));
        assert!(
            core.unsupported.is_empty(),
            "{text}: {:?}",
            core.unsupported
        );
        let identity = core
            .functions
            .iter()
            .find(|function| function.name == "identity")
            .unwrap();
        assert_eq!(identity.parameters[0].ty, identity.return_type);
        // Source headers name the physical declaration. Qualification is
        // published separately in the exact nominal catalog, then resolved by
        // Types; a header Named is not evidence that its class domain is empty.
        let Type::Named(name) = &identity.return_type else {
            panic!(
                "Option header lost its physical declaration: {:?}",
                identity.return_type
            );
        };
        let instances = core
            .life_instances
            .iter()
            .filter(|instance| {
                matches!(&instance.ty, Type::BorrowedStorage { storage, .. }
                if matches!(storage.as_ref(), Type::Enum { name: actual, .. } if actual == name))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            instances.len(),
            1,
            "whole Option needs one unambiguous qualified instance: {text}"
        );
        let instance = instances[0];
        let Type::BorrowedStorage { storage, life } = &instance.ty else {
            unreachable!()
        };
        assert_eq!(life.binder_count, 1);
        let Type::Enum { variants, .. } = storage.as_ref() else {
            unreachable!()
        };
        let physical = core
            .enums
            .iter()
            .filter(|item| &item.name == name)
            .collect::<Vec<_>>();
        assert_eq!(physical.len(), 1);
        assert_eq!(
            &physical[0].variants, variants,
            "catalog must preserve every actual variant and field"
        );
        assert_eq!(variants.len(), 2);
        assert!(
            variants
                .iter()
                .any(|(name, fields)| name == "None" && fields.is_empty())
        );
        assert!(variants.iter().any(|(name, fields)| name == "Some" && matches!(fields.as_slice(), [Type::Address { mutable: false, inner: pointee }] if **pointee == Type::I32)));
        assert_eq!(
            life.retained,
            [zil_ir::StorageRetainedBinding {
                path: vec![zil_ir::LifeTypeProjection::TypeArgument(0)],
                class: 0,
                binder: 0,
            }]
        );
        let some = variants
            .iter()
            .position(|(name, _)| name == "Some")
            .unwrap() as u32;
        assert_eq!(
            life.slots,
            [zil_ir::StorageLifeSlot {
                path: vec![zil_ir::PlaceProjection::Variant(
                    zil_ir::VariantProjection {
                        variant: some,
                        index: 0
                    }
                )],
                binder: 0,
                mode: zil_ir::StoredBorrowMode::SharedAddress,
            }]
        );
        assert!(life.nominal_arguments.is_empty());
        let declaration = core
            .life_declarations
            .iter()
            .find(|item| item.id == instance.declaration)
            .unwrap();
        assert_eq!(declaration.parameters, [LifeParameterKind::Type]);
        let [
            LifeNominalActual::QualifiedType {
                ty,
                life: argument_life,
            },
        ] = instance.arguments.as_slice()
        else {
            panic!(
                "whole borrowed Type argument was erased: {:?}",
                instance.arguments
            );
        };
        assert!(
            matches!(ty, Type::Address { mutable: false, inner: pointee } if **pointee == Type::I32)
        );
        assert_eq!(argument_life.binder_count, 1);
        assert_eq!(
            argument_life.slots,
            [zil_ir::StorageLifeSlot {
                path: Vec::new(),
                binder: 0,
                mode: zil_ir::StoredBorrowMode::SharedAddress,
            }]
        );
        assert!(argument_life.retained.is_empty());
        assert!(argument_life.nominal_arguments.is_empty());
        let contract = identity.storage_life.as_ref().unwrap();
        assert_eq!(contract.classes.len(), 2);
        let input = contract
            .classes
            .iter()
            .find(|row| {
                row.position.base == StorageBase::Parameter(0)
                    && row.position.path.is_empty()
                    && row.position.class == 0
            })
            .unwrap();
        let result = contract
            .classes
            .iter()
            .find(|row| {
                row.position.base == StorageBase::Result
                    && row.position.path.is_empty()
                    && row.position.class == 0
            })
            .unwrap();
        assert_eq!(input.binder, result.binder);
    }
}

#[test]
fn option_input_layout_does_not_admit_another_pointee_or_mode() {
    for constructor in ["Option::<&'b i64>.Some(value)", "Option.Some(value)"] {
        let text = format!(
            "fn identity<life a>(x:Option<&'a i32>)->Option<&'a i32>{{return x}}fn run<life b>(value:&'b i64)->i32{{let result=identity({constructor});return 0}}fn main()->i32{{return 0}}"
        );
        let errors = expand(&text).unwrap_err();
        assert!(
            errors.iter().any(|error| error.phase == Phase::Expand),
            "{errors:?}"
        );
    }
    let text = "fn identity<life a>(x:Option<&'a i32>)->Option<&'a i32>{return x}fn run<life b>(value:&'b mut i32)->i32{let result=identity(Option.Some(value));return 0}fn main()->i32{return 0}";
    let errors = expand(text).unwrap_err();
    assert!(
        errors.iter().any(|error| error.phase == Phase::Expand),
        "{errors:?}"
    );
}
