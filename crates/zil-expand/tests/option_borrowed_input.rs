use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
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
        original.replace(
            "let result=identity(Option::<&'a i32>.Some(value));",
            "let result:Option<&'a i32>=identity(Option.None());",
        ),
        original.replace(
            "let result=identity(Option::<&'a i32>.Some(value));",
            "let result:Option<&'a i32>=identity((Option.None()));",
        ),
        format!(
            "fn joined<life c>(first:Option<&'c i32>,second:Option<&'c i32>)->Option<&'c i32>{{return second}}{}",
            original.replace(
                "identity(Option::<&'a i32>.Some(value))",
                "joined(Option.None(),Option::<&'a i32>.Some(value))"
            )
        ),
        original.replace(
            "let result=identity(Option::<&'a i32>.Some(value));",
            "let pending=identity(Option.None());let alias=pending;let result:Option<&'a i32>=alias;",
        ),
        format!(
            "fn joined<life c>(first:Option<&'c i32>,second:Option<&'c i32>)->Option<&'c i32>{{return second}}{}",
            original.replace(
                "let result=identity(Option::<&'a i32>.Some(value));",
                "let first=identity(Option.None());let second=identity(Option.None());let combined=joined(first,second);let result:Option<&'a i32>=combined;"
            )
        ),
        format!(
            "fn joined<life c>(first:Option<&'c i32>,second:Option<&'c i32>)->Option<&'c i32>{{return second}}{}",
            original.replace(
                "let result=identity(Option::<&'a i32>.Some(value));",
                "let first=identity(Option.None());let result=joined(first,Option::<&'a i32>.Some(value));"
            )
        ),
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

// The original source bytes remain unchanged. The complete body supplies no
// constraint for the phantom life: semantic-scopes requires an inference
// error, not Unsupported and not a guessed enclosing lifetime.
#[test]
fn uncontextualized_none_reports_underdetermined_lifetime() {
    let text = "fn identity<life a>(x:Option<&'a i32>)->Option<&'a i32>{return x}\nfn run<life a>(value:&'a i32)->i32{let result=identity(Option.None());return 0}\nfn main()->i32{let value=42;return run(&value)}\n";
    let errors = expand(text).unwrap_err();
    assert!(
        errors.iter().any(|error| error.phase == Phase::Expand
            && error.category == ResultCategory::SafeReject
            && error.message == "original call lifetime inference is underdetermined"),
        "{errors:?}"
    );
}

#[test]
fn deferred_none_keeps_ambiguity_and_conflicting_rigid_classes() {
    let identity = "fn identity<life a>(x:Option<&'a i32>)->Option<&'a i32>{return x}";
    for body in [
        "fn main(){let pending=identity(Option.None());}",
        "fn join<life c>(x:Option<&'c i32>,y:Option<&'c i32>)->Option<&'c i32>{return x}fn run<life b>(input:&'b i32){let x=identity(Option.None());let y=identity(Option.None());let still_unknown=join(x,y);}fn main(){}",
        "fn run<life a,life b>(left:&'a i32,right:&'b i32){let x=identity(Option.None());let first:Option<&'a i32>=x;let second:Option<&'b i32>=x;}fn main(){}",
    ] {
        let text = format!("{identity}{body}");
        let errors = expand(&text).unwrap_err();
        assert!(
            errors.iter().any(|error| error.phase == Phase::Expand
                && error.category == ResultCategory::SafeReject),
            "{text}: {errors:?}"
        );
        assert!(
            !errors
                .iter()
                .any(|error| error.category == ResultCategory::Unsupported),
            "{text}: {errors:?}"
        );
    }
}

#[test]
fn later_none_context_keeps_the_frozen_original_call_obligations() {
    let text = "fn identity<life a>(x:Option<&'a i32>)->Option<&'a i32>{return x}fn joined<life c>(x:Option<&'c i32>,y:Option<&'c i32>)->Option<&'c i32>{return x}fn run<life b>(input:&'b i32)->i32{let x=identity(Option.None());let y=identity(Option.None());let both=joined(x,y);let result:Option<&'b i32>=both;return 0}fn main()->i32{let input=42;return run(&input)}";
    let source = SourceUnit::new(FileId(0), ModuleId(0), "later-none-original.zil", text);
    let parsed = zil_syntax::parse(&source).into_result().unwrap();
    let aliases = std::collections::BTreeMap::new();
    let core = zil_expand::expand_with_map_semantics(
        &source,
        &parsed,
        &zil_base::Limits::default(),
        &zil_base::TargetProfile::default(),
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: ModuleId(0),
            aliases: &aliases,
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap_or_else(|errors| panic!("{errors:?}"))
    .core;
    let original = core.semantic.as_ref().unwrap();
    // OriginalCall carries its unresolved actuals in the original operation
    // tree, not in the older explicit Call recipe. Inspect both original
    // bindings and their exact call anchors: only the physical clone may gain
    // the resolved constructor TypeApply or a caller lifetime application.
    let mut checked = 0;
    for definition in &original.definitions {
        let zil_ir::SemanticDefinitionBody::FunctionWithBody {
            sites,
            site_nodes,
            body: zil_ir::SemanticOriginalFunctionBody::Defined(body),
            ..
        } = &definition.body
        else {
            continue;
        };
        for statement in &body.statements {
            let zil_ir::CoreStatementKind::Binding {
                name,
                mutable,
                ty,
                initializer: Some(initializer),
            } = &statement.kind
            else {
                continue;
            };
            if name != "x" && name != "y" {
                continue;
            }
            assert!(
                !mutable && ty.is_none(),
                "original pending binding was annotated"
            );
            let zil_ir::CoreExprKind::Call { callee, arguments } = &initializer.kind else {
                panic!("original identity call was rewritten: {initializer:?}")
            };
            assert!(matches!(&callee.kind, zil_ir::CoreExprKind::Name(name) if name == "identity"));
            let [empty] = arguments.as_slice() else {
                panic!("identity lost its original argument")
            };
            let zil_ir::CoreExprKind::Call { callee, arguments } = &empty.kind else {
                panic!("original None call was rewritten: {empty:?}")
            };
            assert!(arguments.is_empty());
            assert!(
                matches!(&callee.kind,
                zil_ir::CoreExprKind::Member { object, member, owner: None }
                if member == "None" && matches!(&object.kind,
                    zil_ir::CoreExprKind::Name(name) if name == "Option")),
                "physical contextualization rewrote the original inferred constructor"
            );
            for call in [initializer, empty] {
                assert!(
                    site_nodes.iter().any(|anchor| anchor.node
                        == zil_ir::SemanticOriginalNode::Expression(call.id)
                        && sites.iter().any(|site| site.id == anchor.site
                            && matches!(site.kind, zil_ir::SemanticSiteRecipeKind::OriginalCall))),
                    "original call lost its exact independently checked occurrence"
                );
            }
            checked += 1;
        }
    }
    assert_eq!(
        checked, 2,
        "both pending original calls must remain unresolved"
    );
    assert!(core.unsupported.is_empty());
}

#[test]
fn deferred_none_preserves_explicit_type_actual_and_full_life_ordinal() {
    for formals in ["T:Copy,life a", "life a,T:Copy"] {
        let text = format!(
            "fn identity<{formals}>(x:Option<&'a T>)->Option<&'a T>{{return x}}fn run<life b>(input:&'b i32)->i32{{let pending=identity::<i32>(Option.None());let result:Option<&'b i32>=pending;return 0}}fn main()->i32{{let input=42;return run(&input)}}"
        );
        let core = expand(&text).unwrap_or_else(|errors| panic!("{text}: {errors:?}"));
        assert!(
            core.unsupported.is_empty(),
            "{text}: {:?}",
            core.unsupported
        );
        assert!(core.life_instances.iter().any(|instance| matches!(&instance.ty,
            Type::BorrowedStorage { storage, life }
                if life.binder_count == 1 && matches!(storage.as_ref(), Type::Enum { variants, .. }
                    if variants.iter().any(|(name, fields)| name == "Some" && matches!(fields.as_slice(),
                        [Type::Address { mutable: false, inner }] if **inner == Type::I32))))));
    }
}
