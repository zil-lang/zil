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
    owner: terms::Owner,
    b: &mut Budget,
) -> Check<matching::Arguments<'a>> {
    let count = decl
        .implicit_lifetimes
        .as_ref()
        .ok_or_else(|| bad("witness original implicit occurrence directory is Unknown"))?
        .len();
    b.spend(count)?;
    Ok(matching::Arguments {
        owner,
        terms: terms::Substitution::default(),
        projections: vec![],
        constants: vec![],
        collecting_anchors: false,
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
        let mut root = None;
        for (node, witness) in witnesses.iter().enumerate() {
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
            let owner = terms::Owner::Witness(table.id, node as u32);
            let mut args = arguments(implementation, &witness.actuals, owner, b)?;
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
            matching::complete_anchors(
                module,
                implementation,
                &witness.application,
                &witness.receiver,
                &mut args,
                b,
            )?;
            matching::application(
                module,
                trait_recipe,
                &witness.application,
                &witness.receiver,
                &mut args,
                b,
            )?;
            associated(module, witness, application, implementation, owner, b)?;
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
                    && external::actuals_equal(&actuals, &table.actuals, b)?
                    && external::applications_equal(
                        &application_image(&witness.application, &witness.to_context, b)?,
                        &interface.application,
                        b,
                    )?
                {
                    roots += 1;
                    root = Some(node);
                }
            }
        }
        if roots != 1 {
            return Err(bad("table has no unique complete original root witness"));
        }
        queries::check(
            module,
            table,
            root.ok_or_else(|| bad("complete witness root is absent"))?,
            b,
        )?;
    }
    Ok(())
}
fn associated(
    module: &TypedModule,
    witness: &DynImplementationWitness,
    trait_decl: &DynDeclaration,
    impl_decl: &DynDeclaration,
    scope_owner: terms::Owner,
    b: &mut Budget,
) -> Check<()> {
    let mut seen = vec![false; trait_decl.members.len()];
    b.spend(seen.len().saturating_add(witness.associated.len()))?;
    for resolution in &witness.associated {
        dynamic_metadata::recipe(resolution, b)?;
        if !external::applications_equal(&resolution.member.application, &witness.application, b)? {
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
        let trait_definitions = trait_decl
            .associated_definitions
            .as_ref()
            .ok_or_else(|| bad("original trait associated declarations are Unknown"))?;
        b.spend(trait_definitions.len())?;
        let mut originals = trait_definitions
            .iter()
            .filter(|row| row.member as usize == ordinal);
        let original = originals
            .next()
            .ok_or_else(|| bad("associated result has no original trait declaration"))?;
        if originals.next().is_some() {
            return Err(bad(
                "associated result original trait declaration is ambiguous",
            ));
        }
        if let DynAssociatedValue::Constant { ty, .. } = &resolution.value {
            let carrier = original
                .carrier
                .as_ref()
                .ok_or_else(|| bad("original trait constant carrier is Unknown"))?;
            let mut trait_args =
                arguments(trait_decl, &witness.application.actuals, scope_owner, b)?;
            if !type_projection(carrier, b)? {
                matching::ty(module, carrier, ty, &witness.receiver, &mut trait_args, b)?;
            }
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
        let mut args = arguments(owner, actuals, scope_owner, b)?;
        if owner.kind == DynDeclarationKind::Impl {
            let self_pattern = owner
                .self_pattern
                .as_ref()
                .ok_or_else(|| bad("associated original Self recipe is Unknown"))?;
            matching::ty(
                module,
                self_pattern,
                &witness.receiver,
                &witness.receiver,
                &mut args,
                b,
            )?;
            matching::complete_anchors(
                module,
                impl_decl,
                &witness.application,
                &witness.receiver,
                &mut args,
                b,
            )?;
        } else {
            let mut original = arguments(impl_decl, &witness.actuals, scope_owner, b)?;
            let self_pattern = impl_decl
                .self_pattern
                .as_ref()
                .ok_or_else(|| bad("default implementation Self recipe is Unknown"))?;
            matching::ty(
                module,
                self_pattern,
                &witness.receiver,
                &witness.receiver,
                &mut original,
                b,
            )?;
            matching::complete_anchors(
                module,
                impl_decl,
                &witness.application,
                &witness.receiver,
                &mut original,
                b,
            )?;
            let implemented = impl_decl
                .implemented_trait
                .as_ref()
                .ok_or_else(|| bad("default implemented trait recipe is Unknown"))?;
            args.terms = matching::transfer_application(
                module,
                implemented,
                &witness.application,
                &witness.receiver,
                &mut original,
                b,
            )?;
        }
        match (recipe, &resolution.value, &member.kind) {
            (
                DynAssociatedValueRecipe::Type(pattern),
                DynAssociatedValue::Type(actual),
                DynMemberKind::Type,
            ) => {
                values::check(module, actual, witness.scope, b)?;
                if !type_projection(pattern, b)? {
                    instantiate::against(module, pattern, actual, &witness.receiver, &mut args, b)?;
                }
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
                if !type_projection(ty, b)? {
                    matching::ty(module, ty, actual_ty, &witness.receiver, &mut args, b)?;
                }
                let carrier = definition
                    .carrier
                    .as_ref()
                    .ok_or_else(|| bad("original associated constant carrier is Unknown"))?;
                if !type_projection(carrier, b)? {
                    matching::ty(module, carrier, actual_ty, &witness.receiver, &mut args, b)?;
                }
                if !const_projection(value, b)?
                    && !type_projection(ty, b)?
                    && matching::constant(module, value, &actual_ty.ty, &args, b)? != *actual
                {
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

fn type_projection(pattern: &DynTypePattern, b: &mut Budget) -> Check<bool> {
    b.spend(1)?;
    Ok(match pattern {
        DynTypePattern::AssociatedProjection { .. } => true,
        DynTypePattern::Nominal(n) => {
            let mut found = false;
            for arg in &n.arguments {
                found |= match arg {
                    DynPatternActual::Type(t) => type_projection(t, b)?,
                    DynPatternActual::Const(c) => const_projection(c, b)?,
                    _ => false,
                };
            }
            found
        }
        DynTypePattern::Address { inner, .. }
        | DynTypePattern::Managed(inner)
        | DynTypePattern::Desc { inner, .. }
        | DynTypePattern::Slice(inner)
        | DynTypePattern::OwnedStore(inner) => type_projection(inner, b)?,
        DynTypePattern::Tuple(fields) => {
            let mut found = false;
            for t in fields {
                found |= type_projection(t, b)?;
            }
            found
        }
        DynTypePattern::Array { element, length } => {
            type_projection(element, b)? || const_projection(length, b)?
        }
        DynTypePattern::ErrorUnion { error, result } => {
            type_projection(error, b)? || type_projection(result, b)?
        }
        _ => false,
    })
}
fn const_projection(pattern: &DynConstPattern, b: &mut Budget) -> Check<bool> {
    b.spend(1)?;
    Ok(match pattern {
        DynConstPattern::AssociatedProjection { .. } => true,
        DynConstPattern::Unary { value, ty, .. } | DynConstPattern::Convert { value, ty } => {
            const_projection(value, b)? || type_projection(ty, b)?
        }
        DynConstPattern::Binary {
            left, right, ty, ..
        } => const_projection(left, b)? || const_projection(right, b)? || type_projection(ty, b)?,
        _ => false,
    })
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
        trait_decl.associated_definitions = Some(vec![
            DynAssociatedDefinition {
                member: 0,
                carrier: None,
                value: None,
                requirements: vec![],
            },
            DynAssociatedDefinition {
                member: 1,
                carrier: Some(DynTypePattern::Concrete(Type::I32)),
                value: None,
                requirements: vec![],
            },
        ]);
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
    fn add_nominal(module: &mut TypedModule, count: u32) -> DynTypeValue {
        let declaration = LifeDeclarationId {
            module: ModuleId(0),
            declaration: NodeId(50),
            compiler: false,
        };
        if !module.life_declarations.iter().any(|d| d.id == declaration) {
            module.life_declarations.push(LifeNominalDeclaration {
                id: declaration,
                identity: LifeDeclarationIdentity {
                    package: "test".into(),
                    module_path: vec![],
                    declaration_path: vec!["P".into()],
                    generation_path: vec![],
                },
                kind: LifeNominalKind::Record,
                parameters: vec![LifeParameterKind::Lifetime; 2],
            });
        }
        let life = StorageLifeShape {
            binder_count: count,
            slots: vec![],
            retained: vec![],
            nominal_arguments: (0..2)
                .map(|i| NominalLifeArgument {
                    path: vec![],
                    declaration,
                    argument: i,
                    binder: i % count,
                })
                .collect(),
        };
        let ty = Type::BorrowedStorage {
            storage: Box::new(Type::Record {
                name: format!("test.P{count}"),
                fields: vec![],
            }),
            life: life.clone(),
        };
        module.life_instances.push(LifeNominalInstance {
            declaration,
            ty: ty.clone(),
            arguments: (0..2)
                .map(|i| LifeNominalActual::Lifetime(i % count))
                .collect(),
        });
        DynTypeValue {
            ty,
            life,
            classes: vec![StorageBinder::Fixed(0); count as usize],
        }
    }
    fn intrinsic_fixture(count: u32, default: bool) -> (TypedModule, DynTypeValue, DynTypeValue) {
        let mut module = fixture();
        let p2 = add_nominal(&mut module, 2);
        let p1 = add_nominal(&mut module, 1);
        let receiver = if count == 2 { &p2 } else { &p1 };
        let context_id = LifeDeclarationId {
            module: ModuleId(0),
            declaration: NodeId(60),
            compiler: false,
        };
        let context_life = StorageLifeShape {
            binder_count: 1,
            slots: vec![],
            retained: vec![],
            nominal_arguments: vec![NominalLifeArgument {
                path: vec![],
                declaration: context_id,
                argument: 0,
                binder: 0,
            }],
        };
        let context = Type::BorrowedStorage {
            storage: Box::new(Type::Record {
                name: "test.Context".into(),
                fields: vec![("code".into(), Type::I32)],
            }),
            life: context_life.clone(),
        };
        module.life_declarations.push(LifeNominalDeclaration {
            id: context_id,
            identity: LifeDeclarationIdentity {
                package: "test".into(),
                module_path: vec![],
                declaration_path: vec!["Context".into()],
                generation_path: vec![],
            },
            kind: LifeNominalKind::Record,
            parameters: vec![LifeParameterKind::Lifetime],
        });
        module.life_instances.push(LifeNominalInstance {
            declaration: context_id,
            ty: context.clone(),
            arguments: vec![LifeNominalActual::Lifetime(0)],
        });
        let lifetimes = vec![DynGenericActual::Life(StorageBinder::Fixed(0)); 2];
        let application = DynTraitApplication {
            declaration: module.dyn_declarations[0].id,
            actuals: lifetimes.clone(),
        };
        let p_pattern = DynTypePattern::Nominal(Box::new(DynSelfPattern {
            declaration: module.life_declarations[0].id,
            arguments: (0..2)
                .map(|i| DynPatternActual::Life(DynLifePattern::DeclarationParameter(i)))
                .collect(),
        }));
        module.dyn_declarations[0].parameters = vec![LifeParameterKind::Lifetime; 2];
        module.dyn_declarations[1].parameters = vec![LifeParameterKind::Lifetime; 2];
        module.dyn_declarations[1].self_pattern = Some(p_pattern.clone());
        module.dyn_declarations[1].implemented_trait = Some(DynSelfPattern {
            declaration: application.declaration,
            arguments: (0..2)
                .map(|i| DynPatternActual::Life(DynLifePattern::DeclarationParameter(i)))
                .collect(),
        });
        if default {
            module.dyn_declarations[1].members.pop();
            module.dyn_declarations[1]
                .associated_definitions
                .as_mut()
                .unwrap()
                .pop();
            module.dyn_declarations[0]
                .associated_definitions
                .as_mut()
                .unwrap()[0]
                .value = Some(DynAssociatedValueRecipe::Type(p_pattern));
        } else {
            module.dyn_declarations[1]
                .associated_definitions
                .as_mut()
                .unwrap()[1]
                .value = Some(DynAssociatedValueRecipe::Type(p_pattern));
        }
        let interface = &mut module.dyn_interfaces[0];
        interface.application = application.clone();
        interface.context = Some(context);
        interface.retained_life.binder_count = 1;
        interface.retained_life.nominal_arguments = context_life.nominal_arguments;
        interface.associated_types = vec![DynAssociatedType {
            member: DynMember {
                application: application.clone(),
                ordinal: 0,
            },
            ty: receiver.ty.clone(),
            life: receiver.life.clone(),
            classes: receiver.classes.clone(),
        }];
        let table = &mut module.dyn_vtables[0];
        table.self_type = receiver.ty.clone();
        table.self_life = Some(receiver.life.clone());
        table.self_classes = receiver.classes.clone();
        table.actuals = lifetimes.clone();
        let witness = &mut table.implementation_witnesses.as_mut().unwrap()[0];
        witness.scope.fixed_binder_count = 1;
        witness.to_context = vec![DynLifeEmbedding {
            from: StorageBinder::Fixed(0),
            to: StorageBinder::Fixed(0),
        }];
        witness.receiver = receiver.clone();
        witness.application = application.clone();
        witness.actuals = lifetimes;
        for row in &mut witness.associated {
            row.member.application = application.clone();
        }
        witness.associated[0].value = DynAssociatedValue::Type(receiver.clone());
        if default {
            witness.associated[0].source = DynAssociatedSource::TraitDefault {
                declaration: application.declaration,
                member: 0,
            };
        }
        (module, p2, p1)
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
    fn intrinsic_origin_partition_survives_raw_wire_override_and_trait_default_boundaries() {
        for default in [false, true] {
            let (module, p2, p1) = intrinsic_fixture(2, default);
            assert!(run(&module).is_ok(), "{:?}", run(&module));
            let mut compressed = module.clone();
            compressed.dyn_vtables[0]
                .implementation_witnesses
                .as_mut()
                .unwrap()[0]
                .associated[0]
                .value = DynAssociatedValue::Type(p1.clone());
            assert!(run(&compressed).is_err());
            let (shared, _, _) = intrinsic_fixture(1, default);
            assert!(run(&shared).is_ok(), "{:?}", run(&shared));
            let mut split = shared.clone();
            split.dyn_vtables[0]
                .implementation_witnesses
                .as_mut()
                .unwrap()[0]
                .associated[0]
                .value = DynAssociatedValue::Type(p2.clone());
            assert!(run(&split).is_err());
        }
        let (mut repeated, _, _) = intrinsic_fixture(2, false);
        if let Some(DynTypePattern::Nominal(pattern)) =
            &mut repeated.dyn_declarations[1].self_pattern
        {
            pattern.arguments[1] = pattern.arguments[0].clone();
        }
        assert!(run(&repeated).is_err());
    }

    #[test]
    fn where_fresh_nominal_keys_keep_intrinsic_origins_before_context() {
        for count in [1, 2] {
            let (mut module, _, _) = intrinsic_fixture(count, false);
            let rhs = module.dyn_declarations[1].self_pattern.clone().unwrap();
            let application = module.dyn_declarations[1]
                .implemented_trait
                .clone()
                .unwrap();
            module.dyn_declarations[0].requirements = Some(vec![DynObligation::TypeEqual {
                left: DynTypePattern::AssociatedProjection {
                    receiver: Box::new(DynTypePattern::SelfType),
                    application: Box::new(application),
                    member: 0,
                },
                right: rhs,
            }]);
            assert!(run(&module).is_ok(), "{:?}", run(&module));
            let mut wrong = module.clone();
            let DynObligation::TypeEqual {
                right: DynTypePattern::Nominal(rhs),
                ..
            } = &mut wrong.dyn_declarations[0].requirements.as_mut().unwrap()[0]
            else {
                unreachable!()
            };
            // Repeated a/a is P1 even when a/b both map to the same Context.
            rhs.arguments[1] = rhs.arguments[0].clone();
            assert_eq!(run(&wrong).is_ok(), count == 1);
        }
    }

    fn closed_self(module: &mut TypedModule) {
        module.dyn_declarations[1].self_pattern = Some(DynTypePattern::Concrete(Type::I32));
        module.dyn_vtables[0].self_type = Type::I32;
        module.dyn_vtables[0].self_life = Some(empty_life());
        module.dyn_vtables[0].self_classes.clear();
        module.dyn_vtables[0]
            .implementation_witnesses
            .as_mut()
            .unwrap()[0]
            .receiver = value(Type::I32);
    }
    #[test]
    fn unused_lexical_lifetimes_are_fallback_only_after_all_application_type_anchors() {
        // No Self/type occurrence distinguishes the two actuals: real Lex0 is
        // one origin, even though both original formals remain in the ledger.
        let (mut same, p2, _) = intrinsic_fixture(1, false);
        closed_self(&mut same);
        assert!(run(&same).is_ok(), "{:?}", run(&same));
        let mut wrong = same.clone();
        wrong.dyn_vtables[0]
            .implementation_witnesses
            .as_mut()
            .unwrap()[0]
            .associated[0]
            .value = DynAssociatedValue::Type(p2.clone());
        assert!(run(&wrong).is_err());
        // Different LOCAL binders remain distinct even when Context is one.
        let mut separate = same.clone();
        let witness = &mut separate.dyn_vtables[0]
            .implementation_witnesses
            .as_mut()
            .unwrap()[0];
        witness.scope.fixed_binder_count = 2;
        witness.to_context.push(DynLifeEmbedding {
            from: StorageBinder::Fixed(1),
            to: StorageBinder::Fixed(0),
        });
        witness.actuals[1] = DynGenericActual::Life(StorageBinder::Fixed(1));
        witness.application.actuals = witness.actuals.clone();
        let mut actual = p2.clone();
        actual.classes[1] = StorageBinder::Fixed(1);
        witness.associated[0].value = DynAssociatedValue::Type(actual);
        for resolution in &mut witness.associated {
            resolution.member.application = witness.application.clone();
        }
        assert!(run(&separate).is_ok(), "{:?}", run(&separate));
        // A finer intrinsic directory in implemented_trait precedes fallback.
        // Self is closed and impl actuals still [Lex0,Lex0], but Trait<P2>
        // supplies actual class0/class1 anchors for a/b.
        let (mut anchored, p2, p1) = intrinsic_fixture(2, false);
        closed_self(&mut anchored);
        let pattern = match anchored.dyn_declarations[1]
            .associated_definitions
            .as_ref()
            .unwrap()[1]
            .value
            .as_ref()
            .unwrap()
        {
            DynAssociatedValueRecipe::Type(pattern) => pattern.clone(),
            _ => unreachable!(),
        };
        anchored.dyn_declarations[0].parameters = vec![LifeParameterKind::Type];
        anchored.dyn_declarations[1]
            .implemented_trait
            .as_mut()
            .unwrap()
            .arguments = vec![DynPatternActual::Type(pattern)];
        let application = DynTraitApplication {
            declaration: anchored.dyn_declarations[0].id,
            actuals: vec![DynGenericActual::QualifiedType(p2.clone())],
        };
        anchored.dyn_interfaces[0].application = application.clone();
        anchored.dyn_interfaces[0].associated_types[0]
            .member
            .application = application.clone();
        let witness = &mut anchored.dyn_vtables[0]
            .implementation_witnesses
            .as_mut()
            .unwrap()[0];
        witness.application = application.clone();
        for resolution in &mut witness.associated {
            resolution.member.application = application.clone();
        }
        assert!(run(&anchored).is_ok(), "{:?}", run(&anchored));
        let mut compressed = anchored.clone();
        compressed.dyn_vtables[0]
            .implementation_witnesses
            .as_mut()
            .unwrap()[0]
            .associated[0]
            .value = DynAssociatedValue::Type(p1);
        assert!(run(&compressed).is_err());
        let mut repeated = anchored.clone();
        let DynPatternActual::Type(DynTypePattern::Nominal(nominal)) = &mut repeated
            .dyn_declarations[1]
            .implemented_trait
            .as_mut()
            .unwrap()
            .arguments[0]
        else {
            unreachable!()
        };
        nominal.arguments[1] = nominal.arguments[0].clone();
        assert!(run(&repeated).is_err());
    }

    fn projected_item(module: &TypedModule) -> DynTypePattern {
        DynTypePattern::AssociatedProjection {
            receiver: Box::new(DynTypePattern::SelfType),
            application: Box::new(DynSelfPattern {
                declaration: module.dyn_declarations[0].id,
                arguments: vec![],
            }),
            member: 0,
        }
    }
    #[test]
    fn original_where_associated_type_and_const_equations_are_evaluated_raw_and_wire() {
        let mut module = fixture();
        let item = projected_item(&module);
        let app = DynSelfPattern {
            declaration: module.dyn_declarations[0].id,
            arguments: vec![],
        };
        module.dyn_declarations[0].requirements = Some(vec![
            DynObligation::TypeEqual {
                left: item.clone(),
                right: DynTypePattern::Concrete(Type::I32),
            },
            DynObligation::ConstEqual {
                ty: DynTypePattern::Concrete(Type::I32),
                left: DynConstPattern::AssociatedProjection {
                    receiver: Box::new(DynTypePattern::SelfType),
                    application: Box::new(app),
                    member: 1,
                },
                right: DynConstPattern::IntegerLiteral("7".into()),
            },
        ]);
        assert!(run(&module).is_ok(), "{:?}", run(&module));
        let mut wrong = module.clone();
        if let DynObligation::TypeEqual { right, .. } =
            &mut wrong.dyn_declarations[0].requirements.as_mut().unwrap()[0]
        {
            *right = DynTypePattern::Concrete(Type::U32);
        }
        assert!(run(&wrong).is_err());
        let mut wrong = module.clone();
        if let DynObligation::ConstEqual { right, .. } =
            &mut wrong.dyn_declarations[0].requirements.as_mut().unwrap()[1]
        {
            *right = DynConstPattern::IntegerLiteral("8".into());
        }
        assert!(run(&wrong).is_err());
        // An implementation cannot use its own as-yet unproved selection as a premise.
        let mut circular = fixture();
        circular.dyn_declarations[1].requirements = Some(vec![DynObligation::TypeEqual {
            left: item,
            right: DynTypePattern::Concrete(Type::I32),
        }]);
        assert!(run(&circular).is_err());
    }

    #[test]
    fn associated_dependency_edges_are_exact_finite_and_reachable() {
        let mut module = fixture();
        let mut dependency = module.dyn_declarations[0].clone();
        dependency.id.declaration = NodeId(92);
        dependency.identity.declaration_path = vec!["Dependency".into()];
        let mut implementation = module.dyn_declarations[1].clone();
        implementation.id.declaration = NodeId(93);
        implementation.identity.declaration_path = vec!["DependencyImpl".into()];
        implementation
            .implemented_trait
            .as_mut()
            .unwrap()
            .declaration = dependency.id;
        let mut witness = module.dyn_vtables[0]
            .implementation_witnesses
            .as_ref()
            .unwrap()[0]
            .clone();
        witness.implementation = implementation.id;
        witness.application.declaration = dependency.id;
        for resolution in &mut witness.associated {
            resolution.member.application = witness.application.clone();
            if let DynAssociatedSource::ImplOverride { declaration, .. } = &mut resolution.source {
                *declaration = implementation.id;
            }
        }
        let dep_app = DynSelfPattern {
            declaration: dependency.id,
            arguments: vec![],
        };
        let projection = DynTypePattern::AssociatedProjection {
            receiver: Box::new(DynTypePattern::SelfType),
            application: Box::new(dep_app),
            member: 0,
        };
        module.dyn_declarations[1]
            .associated_definitions
            .as_mut()
            .unwrap()[1]
            .value = Some(DynAssociatedValueRecipe::Type(projection));
        module.dyn_declarations.extend([dependency, implementation]);
        module.dyn_vtables[0]
            .implementation_witnesses
            .as_mut()
            .unwrap()
            .push(witness);
        assert!(run(&module).is_ok(), "{:?}", run(&module));
        let mut missing = module.clone();
        missing.dyn_vtables[0]
            .implementation_witnesses
            .as_mut()
            .unwrap()
            .pop();
        assert!(run(&missing).is_err());
        let mut cycle = module.clone();
        let root_item = projected_item(&cycle);
        cycle.dyn_declarations[3]
            .associated_definitions
            .as_mut()
            .unwrap()[1]
            .value = Some(DynAssociatedValueRecipe::Type(root_item));
        assert!(run(&cycle).is_err());
        let mut extra = module.clone();
        extra.dyn_declarations[1]
            .associated_definitions
            .as_mut()
            .unwrap()[1]
            .value = Some(DynAssociatedValueRecipe::Type(DynTypePattern::Concrete(
            Type::I32,
        )));
        assert!(run(&extra).is_err());
    }

    #[test]
    fn associated_original_impl_ordinals_are_not_trait_ordinals_or_inferred_from_abi() {
        let module = fixture();
        assert!(run(&module).is_ok());
        let mut wrong_carrier = module.clone();
        wrong_carrier.dyn_declarations[0]
            .associated_definitions
            .as_mut()
            .unwrap()[1]
            .carrier = Some(DynTypePattern::Concrete(Type::U32));
        assert!(run(&wrong_carrier).is_err());
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
