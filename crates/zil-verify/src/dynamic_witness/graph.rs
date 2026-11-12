//! Witness identities and original associated-definition selection. All keys
//! retain their own lexical domain until the explicit outer map is composed.
use super::*;

fn declaration<'a>(
    module: &'a TypedModule,
    id: LifeDeclarationId,
    b: &mut Budget,
) -> Check<&'a DynDeclaration> {
    b.spend(module.dyn_declarations.len())?;
    let mut rows = module.dyn_declarations.iter().filter(|row| row.id == id);
    let row = rows
        .next()
        .ok_or_else(|| bad("witness original declaration is absent"))?;
    if rows.next().is_some() {
        return Err(bad("witness original declaration identity is ambiguous"));
    }
    Ok(row)
}
fn kinds(
    module: &TypedModule,
    formals: &[LifeParameterKind],
    actuals: &[DynGenericActual],
    scope: DynInstantiationScope,
    b: &mut Budget,
) -> Check<()> {
    if formals.len() != actuals.len() {
        return Err(bad("witness omits complete original mixed actual order"));
    }
    scopes::actuals(module, actuals, scope, b)?;
    for (formal, actual) in formals.iter().zip(actuals) {
        b.spend(1)?;
        match (formal, actual) {
            (LifeParameterKind::Lifetime, DynGenericActual::Life(_))
            | (
                LifeParameterKind::Type,
                DynGenericActual::Type(_) | DynGenericActual::QualifiedType(_),
            ) => {}
            (LifeParameterKind::Const(carrier), DynGenericActual::Const(value))
                if *carrier == value.ty() =>
            {
                constants::closed(
                    &DynConstPattern::Value(value.clone()),
                    carrier,
                    module.target.pointer_width,
                    b,
                )?
                .ok_or_else(|| bad("witness actual constant has no exact scalar value"))?;
            }
            _ => {
                return Err(bad(
                    "witness full actual kind or exact Const carrier differs",
                ));
            }
        }
    }
    Ok(())
}
fn image(rows: &[DynLifeEmbedding], class: StorageBinder, b: &mut Budget) -> Check<StorageBinder> {
    b.spend(rows.len())?;
    let mut found = rows.iter().filter(|row| row.from == class);
    let row = found
        .next()
        .ok_or_else(|| bad("witness lexical class has no complete outer embedding"))?;
    if found.next().is_some() {
        return Err(bad("witness lexical class has repeated outer images"));
    }
    Ok(row.to)
}
fn mapped_actual(
    actual: &DynGenericActual,
    rows: &[DynLifeEmbedding],
    b: &mut Budget,
) -> Check<DynGenericActual> {
    dynamic_metadata::recipe(actual, b)?;
    Ok(match actual {
        DynGenericActual::Life(class) => DynGenericActual::Life(image(rows, *class, b)?),
        DynGenericActual::QualifiedType(value) => {
            let mut actual = value.clone();
            for class in &mut actual.classes {
                *class = image(rows, *class, b)?;
            }
            DynGenericActual::QualifiedType(actual)
        }
        other => other.clone(),
    })
}
fn application_image(
    actual: &DynTraitApplication,
    rows: &[DynLifeEmbedding],
    b: &mut Budget,
) -> Check<DynTraitApplication> {
    b.spend(actual.actuals.len())?;
    Ok(DynTraitApplication {
        declaration: actual.declaration,
        actuals: actual
            .actuals
            .iter()
            .map(|a| mapped_actual(a, rows, b))
            .collect::<Check<_>>()?,
    })
}
fn arguments<'a>(
    decl: &DynDeclaration,
    actuals: &'a [DynGenericActual],
    b: &mut Budget,
) -> Check<matching::Arguments<'a>> {
    let count = decl
        .implicit_lifetimes
        .as_ref()
        .ok_or_else(|| bad("witness original implicit occurrence directory is Unknown"))?
        .len();
    b.spend(count)?;
    Ok(matching::Arguments {
        declaration: actuals,
        method: &[],
        invocation: &[],
        implicit: vec![None; count],
    })
}

/// Validate every node and exact original associated source before dependency
/// interpretation. This prerequisite grants no reachability or implementation
/// proof, and is not sufficient to remove the dispatcher's new-claim guard.
pub(super) fn claims(module: &TypedModule, b: &mut Budget) -> Check<()> {
    for table in &module.dyn_vtables {
        if table.self_life.is_none() && table.implementation_witnesses.is_none() {
            continue;
        }
        let witnesses = table
            .implementation_witnesses
            .as_ref()
            .ok_or_else(|| bad("complete implementation obligation graph is Unknown"))?;
        let self_life = table
            .self_life
            .as_ref()
            .ok_or_else(|| bad("table complete external Self domain is Unknown"))?;
        dynamic_metadata::recipe_table(table, b)?;
        b.spend(module.dyn_interfaces.len())?;
        let mut interfaces = module
            .dyn_interfaces
            .iter()
            .filter(|row| row.id == table.interface);
        let interface = interfaces
            .next()
            .ok_or_else(|| bad("witness table interface is absent"))?;
        if interfaces.next().is_some() {
            return Err(bad("witness table interface identity is ambiguous"));
        }
        dynamic_context::validate(module, interface, b)?;
        let context = DynInstantiationScope {
            fixed_binder_count: interface.retained_life.binder_count,
            invocation_binder_count: 0,
        };
        scopes::bounded(context, b)?;
        let identity: Vec<_> = (0..context.fixed_binder_count)
            .map(|i| DynLifeEmbedding {
                from: StorageBinder::Fixed(i),
                to: StorageBinder::Fixed(i),
            })
            .collect();
        let expected_receiver = DynTypeValue {
            ty: table.self_type.clone(),
            life: self_life.clone(),
            classes: table.self_classes.clone(),
        };
        let mut roots = 0;
        for witness in witnesses {
            dynamic_metadata::recipe(witness, b)?;
            scopes::embedding(witness.scope, context, &witness.to_context, b)?;
            values::check(module, &witness.receiver, witness.scope, b)?;
            let implementation = declaration(module, witness.implementation, b)?;
            if implementation.kind != DynDeclarationKind::Impl {
                return Err(bad("witness provider is not an original implementation"));
            }
            kinds(
                module,
                &implementation.parameters,
                &witness.actuals,
                witness.scope,
                b,
            )?;
            let application = declaration(module, witness.application.declaration, b)?;
            if application.kind != DynDeclarationKind::Trait {
                return Err(bad("witness application is not an original trait"));
            }
            kinds(
                module,
                &application.parameters,
                &witness.application.actuals,
                witness.scope,
                b,
            )?;
            let mut args = arguments(implementation, &witness.actuals, b)?;
            let self_recipe = implementation
                .self_pattern
                .as_ref()
                .ok_or_else(|| bad("witness original implementation Self recipe is Unknown"))?;
            matching::ty(
                module,
                self_recipe,
                &witness.receiver,
                &witness.receiver,
                &mut args,
                b,
            )?;
            let trait_recipe = implementation
                .implemented_trait
                .as_ref()
                .ok_or_else(|| bad("witness original implemented trait recipe is Unknown"))?;
            matching::application(
                module,
                trait_recipe,
                &witness.application,
                &witness.receiver,
                &mut args,
                b,
            )?;
            associated(module, witness, application, implementation, b)?;
            if witness.implementation == table.implementation
                && witness.receiver.ty == table.self_type
                && witness.receiver.life == *self_life
            {
                let Some(relation) = keys::relate(
                    module,
                    keys::Key {
                        receiver: &witness.receiver,
                        application: &witness.application,
                        scope: witness.scope,
                    },
                    keys::Key {
                        receiver: &expected_receiver,
                        application: &interface.application,
                        scope: context,
                    },
                    b,
                )?
                else {
                    continue;
                };
                relation.context(&witness.to_context, &identity, b)?;
                if relation.mapped(&witness.receiver, b)? != expected_receiver {
                    return Err(bad(
                        "root query substitution changes its complete intrinsic receiver",
                    ));
                }
                b.spend(
                    witness
                        .receiver
                        .classes
                        .len()
                        .saturating_add(table.self_classes.len()),
                )?;
                let classes = witness
                    .receiver
                    .classes
                    .iter()
                    .map(|c| image(&witness.to_context, *c, b))
                    .collect::<Check<Vec<_>>>()?;
                let actuals = witness
                    .actuals
                    .iter()
                    .map(|a| mapped_actual(a, &witness.to_context, b))
                    .collect::<Check<Vec<_>>>()?;
                if classes == table.self_classes
                    && actuals == table.actuals
                    && application_image(&witness.application, &witness.to_context, b)?
                        == interface.application
                {
                    roots += 1;
                }
            }
        }
        if roots != 1 {
            return Err(bad("table has no unique complete original root witness"));
        }
    }
    Ok(())
}
fn associated(
    module: &TypedModule,
    witness: &DynImplementationWitness,
    trait_decl: &DynDeclaration,
    impl_decl: &DynDeclaration,
    b: &mut Budget,
) -> Check<()> {
    let mut seen = vec![false; trait_decl.members.len()];
    b.spend(seen.len().saturating_add(witness.associated.len()))?;
    for resolution in &witness.associated {
        dynamic_metadata::recipe(resolution, b)?;
        if resolution.member.application != witness.application {
            return Err(bad(
                "associated result selects a different complete trait application",
            ));
        }
        let ordinal = resolution.member.ordinal as usize;
        let member = trait_decl
            .members
            .get(ordinal)
            .ok_or_else(|| bad("associated result original trait member is absent"))?;
        if matches!(member.kind, DynMemberKind::Method(_))
            || std::mem::replace(&mut seen[ordinal], true)
        {
            return Err(bad(
                "associated result repeats a member or selects a method",
            ));
        }
        let (owner, own_member, actuals) = match resolution.source {
            DynAssociatedSource::TraitDefault {
                declaration,
                member: own,
            } => {
                if declaration != trait_decl.id || own as usize != ordinal {
                    return Err(bad(
                        "associated default selects a different original trait member",
                    ));
                }
                (trait_decl, own, witness.application.actuals.as_slice())
            }
            DynAssociatedSource::ImplOverride {
                declaration,
                member: own,
            } => {
                if declaration != impl_decl.id {
                    return Err(bad(
                        "associated override selects a different original implementation",
                    ));
                }
                let actual = impl_decl
                    .members
                    .get(own as usize)
                    .ok_or_else(|| bad("associated override original impl member is absent"))?;
                if actual.name != member.name || actual.kind != member.kind {
                    return Err(bad(
                        "associated override original name or kind differs; impl ordinals are not trait ordinals",
                    ));
                }
                (impl_decl, own, witness.actuals.as_slice())
            }
        };
        // An actual override wins over a default of the same real member.
        if matches!(resolution.source, DynAssociatedSource::TraitDefault { .. }) {
            b.spend(impl_decl.members.len())?;
            if impl_decl
                .members
                .iter()
                .any(|m| m.name == member.name && m.kind == member.kind)
            {
                return Err(bad(
                    "associated default bypasses the original implementation override",
                ));
            }
        }
        let definitions = owner
            .associated_definitions
            .as_ref()
            .ok_or_else(|| bad("original associated definitions are Unknown"))?;
        b.spend(definitions.len())?;
        let mut rows = definitions.iter().filter(|row| row.member == own_member);
        let definition = rows
            .next()
            .ok_or_else(|| bad("associated source has no original RHS declaration"))?;
        if rows.next().is_some() {
            return Err(bad(
                "associated source has ambiguous original RHS declarations",
            ));
        }
        let recipe = definition
            .value
            .as_ref()
            .ok_or_else(|| bad("associated source RHS is abstract or Unknown"))?;
        let mut args = arguments(owner, actuals, b)?;
        match (recipe, &resolution.value, &member.kind) {
            (
                DynAssociatedValueRecipe::Type(pattern),
                DynAssociatedValue::Type(actual),
                DynMemberKind::Type,
            ) => {
                values::check(module, actual, witness.scope, b)?;
                matching::ty(module, pattern, actual, &witness.receiver, &mut args, b)?;
            }
            (
                DynAssociatedValueRecipe::Constant { ty, value },
                DynAssociatedValue::Constant {
                    ty: actual_ty,
                    value: actual,
                },
                DynMemberKind::Constant,
            ) => {
                values::check(module, actual_ty, witness.scope, b)?;
                matching::ty(module, ty, actual_ty, &witness.receiver, &mut args, b)?;
                let carrier = definition
                    .carrier
                    .as_ref()
                    .ok_or_else(|| bad("original associated constant carrier is Unknown"))?;
                matching::ty(module, carrier, actual_ty, &witness.receiver, &mut args, b)?;
                if matching::constant(module, value, &actual_ty.ty, &args, b)? != *actual {
                    return Err(bad(
                        "associated constant differs from the original exact carrier and evaluated bits",
                    ));
                }
            }
            _ => {
                return Err(bad(
                    "associated source RHS kind differs from its original trait member",
                ));
            }
        }
    }
    if trait_decl
        .members
        .iter()
        .enumerate()
        .any(|(i, m)| !matches!(m.kind, DynMemberKind::Method(_)) && !seen[i])
    {
        return Err(bad(
            "witness omits an original associated type or constant member",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{ModuleId, NodeId};
    fn empty_life() -> StorageLifeShape {
        StorageLifeShape {
            binder_count: 0,
            slots: vec![],
            retained: vec![],
            nominal_arguments: vec![],
        }
    }
    fn value(ty: Type) -> DynTypeValue {
        DynTypeValue {
            ty,
            life: empty_life(),
            classes: vec![],
        }
    }
    fn declaration(id: u32, kind: DynDeclarationKind) -> DynDeclaration {
        DynDeclaration {
            id: LifeDeclarationId {
                module: ModuleId(0),
                declaration: NodeId(id),
                compiler: false,
            },
            identity: LifeDeclarationIdentity {
                package: "test".into(),
                module_path: vec![],
                declaration_path: vec![format!("Decl{id}")],
                generation_path: vec![],
            },
            implicit_lifetimes: Some(vec![]),
            direct_supertraits: Some(vec![]),
            requirements: Some(vec![]),
            associated_definitions: Some(vec![]),
            method_signatures: Some(vec![]),
            method_definitions: Some(vec![]),
            kind,
            parameters: vec![],
            members: vec![],
            self_pattern: None,
            implemented_trait: None,
        }
    }
    fn fixture() -> TypedModule {
        let (mut module, _) = crate::tests::baseline();
        let mut trait_decl = declaration(90, DynDeclarationKind::Trait);
        trait_decl.members = vec![
            DynMemberDeclaration {
                name: "Item".into(),
                kind: DynMemberKind::Type,
            },
            DynMemberDeclaration {
                name: "Count".into(),
                kind: DynMemberKind::Constant,
            },
        ];
        let mut implementation = declaration(91, DynDeclarationKind::Impl);
        implementation.members = vec![trait_decl.members[1].clone(), trait_decl.members[0].clone()];
        implementation.self_pattern = Some(DynTypePattern::Concrete(Type::I32));
        implementation.implemented_trait = Some(DynSelfPattern {
            declaration: trait_decl.id,
            arguments: vec![],
        });
        implementation.associated_definitions = Some(vec![
            DynAssociatedDefinition {
                member: 0,
                carrier: Some(DynTypePattern::Concrete(Type::I32)),
                requirements: vec![],
                value: Some(DynAssociatedValueRecipe::Constant {
                    ty: DynTypePattern::Concrete(Type::I32),
                    value: DynConstPattern::IntegerLiteral("7".into()),
                }),
            },
            DynAssociatedDefinition {
                member: 1,
                carrier: None,
                requirements: vec![],
                value: Some(DynAssociatedValueRecipe::Type(DynTypePattern::Concrete(
                    Type::I32,
                ))),
            },
        ]);
        let application = DynTraitApplication {
            declaration: trait_decl.id,
            actuals: vec![],
        };
        let interface = DynInterfaceId {
            module: ModuleId(0),
            index: 0,
        };
        module.dyn_interfaces.push(DynInterface {
            id: interface,
            context: None,
            owning_cleanup: None,
            application: application.clone(),
            inherited: vec![],
            associated_types: vec![],
            associated_constants: vec![],
            retained_life: RetainedLifeShape {
                binder_count: 0,
                ports: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
            slots: vec![],
        });
        let witness = DynImplementationWitness {
            scope: DynInstantiationScope {
                fixed_binder_count: 0,
                invocation_binder_count: 0,
            },
            to_context: vec![],
            receiver: value(Type::I32),
            application: application.clone(),
            implementation: implementation.id,
            actuals: vec![],
            associated: vec![
                DynAssociatedResolution {
                    member: DynMember {
                        application: application.clone(),
                        ordinal: 0,
                    },
                    source: DynAssociatedSource::ImplOverride {
                        declaration: implementation.id,
                        member: 1,
                    },
                    value: DynAssociatedValue::Type(value(Type::I32)),
                },
                DynAssociatedResolution {
                    member: DynMember {
                        application,
                        ordinal: 1,
                    },
                    source: DynAssociatedSource::ImplOverride {
                        declaration: implementation.id,
                        member: 0,
                    },
                    value: DynAssociatedValue::Constant {
                        ty: value(Type::I32),
                        value: Constant::I32(7),
                    },
                },
            ],
        };
        module.dyn_vtables.push(DynVtable {
            id: DynVtableId {
                module: ModuleId(0),
                index: 0,
            },
            interface,
            self_life: Some(empty_life()),
            implementation_witnesses: Some(vec![witness]),
            self_classes: vec![],
            self_type: Type::I32,
            implementation: implementation.id,
            actuals: vec![],
            ports: vec![],
            entries: vec![],
            cleanup: None,
            owned_storage: None,
        });
        module.dyn_declarations = vec![trait_decl, implementation];
        module
    }
    fn run(module: &TypedModule) -> Check<()> {
        let limits = Limits::default();
        let raw = claims(module, &mut Budget::new(&limits));
        let (_, plan) = crate::tests::baseline();
        let bytes = zil_ir::wire::encode_module_candidate(module, &plan, &limits).unwrap();
        let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        let wire = claims(&decoded, &mut Budget::new(&limits));
        assert_eq!(
            raw.as_ref().map_err(|e| (&e.code, &e.message)),
            wire.as_ref().map_err(|e| (&e.code, &e.message))
        );
        raw
    }
    #[test]
    fn associated_original_impl_ordinals_are_not_trait_ordinals_or_inferred_from_abi() {
        let module = fixture();
        assert!(run(&module).is_ok());
        let mut wrong = module.clone();
        wrong.dyn_vtables[0]
            .implementation_witnesses
            .as_mut()
            .unwrap()[0]
            .associated[0]
            .source = DynAssociatedSource::ImplOverride {
            declaration: module.dyn_declarations[1].id,
            member: 0,
        };
        assert!(run(&wrong).is_err());
        let mut wrong = module.clone();
        if let DynAssociatedValue::Constant { value, .. } = &mut wrong.dyn_vtables[0]
            .implementation_witnesses
            .as_mut()
            .unwrap()[0]
            .associated[1]
            .value
        {
            *value = Constant::I32(8);
        }
        assert!(run(&wrong).is_err());
        let mut wrong = module.clone();
        wrong.dyn_vtables[0]
            .implementation_witnesses
            .as_mut()
            .unwrap()[0]
            .associated
            .pop();
        assert!(run(&wrong).is_err());
        let mut wrong = module.clone();
        let node = wrong.dyn_vtables[0]
            .implementation_witnesses
            .as_ref()
            .unwrap()[0]
            .clone();
        wrong.dyn_vtables[0]
            .implementation_witnesses
            .as_mut()
            .unwrap()
            .push(node);
        assert!(
            run(&wrong)
                .unwrap_err()
                .message
                .contains("unique complete original root")
        );
    }
}
