//! Reconstruct logical port identity from the complete actual Context. The
//! physical paths are used only to validate Context, never as opaque Self GEPs.
use super::*;

pub(super) struct Port {
    pub leaf: Type,
    pub mode: StoredBorrowMode,
    pub binder: u32,
    pub inner: Vec<(u32, u32)>,
}
pub(super) fn check(
    module: &TypedModule,
    interface: &DynInterface,
    mut domain: impl FnMut(&Type, &mut Budget) -> Check<u32>,
    b: &mut Budget,
) -> Check<Vec<Port>> {
    let Some(context) = &interface.context else {
        let life = &interface.retained_life;
        if life.binder_count != 0
            || !life.ports.is_empty()
            || !life.retained.is_empty()
            || !life.nominal_arguments.is_empty()
        {
            return Err(bad(
                "opaque interface without Context invents an internal class or port",
            ));
        }
        return Ok(vec![]);
    };
    let limits = b.limits.clone();
    stored_flow::qualification(module, context, &limits, b)?;
    let Type::BorrowedStorage { storage, life } = context else {
        return Err(bad(
            "opaque Context requires its complete actual qualified type",
        ));
    };
    dynamic_metadata::ty(context, b)?;
    dynamic_metadata::retained_shape(&interface.retained_life, b)?;
    if life.binder_count != interface.retained_life.binder_count
        || life.retained != interface.retained_life.retained
        || life.nominal_arguments != interface.retained_life.nominal_arguments
    {
        return Err(bad("opaque Context class and nominal inventories disagree"));
    }
    b.spend(life.slots.len())?;
    let mut ports: Vec<Port> = vec![];
    for slot in &life.slots {
        let leaf = dynamic_ports::physical_path(storage, &slot.path, b)?;
        let inner = match (leaf, slot.mode) {
            (
                Type::Address {
                    mutable: false,
                    inner,
                },
                StoredBorrowMode::SharedAddress,
            )
            | (
                Type::Address {
                    mutable: true,
                    inner,
                },
                StoredBorrowMode::ExclusiveAddress,
            )
            | (
                Type::Desc(inner),
                StoredBorrowMode::ReadOnlyDescription | StoredBorrowMode::WritableDescription,
            ) => inner,
            _ => return Err(bad("opaque port mode differs from the actual Context leaf")),
        };
        let count = domain(inner, b)?;
        if count as usize > b.limits.max_nodes {
            return Err(limit(
                "opaque port inner class domain exceeds selected bound",
            ));
        }
        b.spend(count as usize + slot.path.len())?;
        let mut prefix = slot
            .path
            .iter()
            .copied()
            .map(LifeTypeProjection::Place)
            .collect::<Vec<_>>();
        prefix.push(LifeTypeProjection::Pointee);
        let mut mapping = Vec::with_capacity(count as usize);
        for class in 0..count {
            b.spend(life.retained.len())?;
            let mut rows = life
                .retained
                .iter()
                .filter(|row| row.path == prefix && row.class == class);
            let row = rows
                .next()
                .ok_or_else(|| bad("opaque port loses an inner class or phantom occurrence"))?;
            if rows.next().is_some() || row.binder >= life.binder_count {
                return Err(bad("opaque port repeats or misbinds an inner occurrence"));
            }
            mapping.push((class, row.binder));
        }
        let mut duplicate = false;
        for previous in &ports {
            dynamic_metadata::ty(&previous.leaf, b)?;
            dynamic_metadata::ty(leaf, b)?;
            b.spend(
                previous
                    .inner
                    .len()
                    .saturating_add(mapping.len())
                    .saturating_add(1),
            )?;
            if previous.leaf == *leaf
                && previous.mode == slot.mode
                && previous.binder == slot.binder
                && previous.inner == mapping
            {
                duplicate = true;
                break;
            }
        }
        if duplicate {
            continue;
        }
        let claimed = interface
            .retained_life
            .ports
            .get(ports.len())
            .ok_or_else(|| bad("opaque port directory coalesces distinct inner class mappings"))?;
        dynamic_metadata::ty(&claimed.leaf, b)?;
        dynamic_metadata::ty(leaf, b)?;
        if claimed.leaf != *leaf || claimed.mode != slot.mode || claimed.binder != slot.binder {
            return Err(bad(
                "opaque logical port order or whole leaf type is not its actual Context",
            ));
        }
        ports.push(Port {
            leaf: leaf.clone(),
            mode: slot.mode,
            binder: slot.binder,
            inner: mapping,
        });
    }
    if ports.len() != interface.retained_life.ports.len() {
        return Err(bad("opaque Context has extra or missing logical ports"));
    }
    Ok(ports)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn equal_outer_binders_do_not_coalesce_different_inner_context_occurrences() {
        let (mut module, _) = crate::tests::baseline();
        let tag_id = LifeDeclarationId {
            module: module.module_id,
            declaration: zil_base::NodeId(940),
            compiler: false,
        };
        let context_id = LifeDeclarationId {
            declaration: zil_base::NodeId(941),
            ..tag_id
        };
        let tag_life = StorageLifeShape {
            binder_count: 1,
            slots: vec![],
            retained: vec![],
            nominal_arguments: vec![NominalLifeArgument {
                path: vec![],
                declaration: tag_id,
                argument: 0,
                binder: 0,
            }],
        };
        let tag = Type::BorrowedStorage {
            storage: Box::new(Type::Record {
                name: "ports.Tag".into(),
                fields: vec![("code".into(), Type::I32)],
            }),
            life: tag_life.clone(),
        };
        let address = Type::Address {
            mutable: false,
            inner: Box::new(tag.clone()),
        };
        let mut nominal_arguments = (0..3)
            .map(|class| NominalLifeArgument {
                path: vec![],
                declaration: context_id,
                argument: class,
                binder: class,
            })
            .collect::<Vec<_>>();
        for field in 0..2 {
            nominal_arguments.push(NominalLifeArgument {
                path: vec![
                    LifeTypeProjection::Place(PlaceProjection::Field(field)),
                    LifeTypeProjection::Pointee,
                ],
                declaration: tag_id,
                argument: 0,
                binder: field + 1,
            });
        }
        let life = StorageLifeShape {
            binder_count: 3,
            slots: (0..2)
                .map(|field| StorageLifeSlot {
                    path: vec![PlaceProjection::Field(field)],
                    mode: StoredBorrowMode::SharedAddress,
                    binder: 0,
                })
                .collect(),
            retained: (0..2)
                .map(|field| StorageRetainedBinding {
                    path: vec![
                        LifeTypeProjection::Place(PlaceProjection::Field(field)),
                        LifeTypeProjection::Pointee,
                    ],
                    class: 0,
                    binder: field + 1,
                })
                .collect(),
            nominal_arguments,
        };
        let context = Type::BorrowedStorage {
            storage: Box::new(Type::Record {
                name: "ports.Context".into(),
                fields: vec![
                    ("left".into(), address.clone()),
                    ("right".into(), address.clone()),
                ],
            }),
            life: life.clone(),
        };
        for (id, name, count) in [(tag_id, "Tag", 1), (context_id, "Context", 3)] {
            module.life_declarations.push(LifeNominalDeclaration {
                id,
                identity: LifeDeclarationIdentity {
                    package: "ports".into(),
                    module_path: vec![],
                    declaration_path: vec![name.into()],
                    generation_path: vec![],
                },
                kind: LifeNominalKind::Record,
                parameters: vec![LifeParameterKind::Lifetime; count],
            });
        }
        module.life_instances.push(LifeNominalInstance {
            declaration: tag_id,
            ty: tag,
            arguments: vec![LifeNominalActual::Lifetime(0)],
        });
        module.life_instances.push(LifeNominalInstance {
            declaration: context_id,
            ty: context.clone(),
            arguments: (0..3).map(LifeNominalActual::Lifetime).collect(),
        });
        let interface = DynInterface {
            context: Some(context),
            owning_cleanup: None,
            id: DynInterfaceId {
                module: module.module_id,
                index: 0,
            },
            application: DynTraitApplication {
                declaration: tag_id,
                actuals: vec![],
            },
            inherited: vec![],
            associated_types: vec![],
            associated_constants: vec![],
            retained_life: RetainedLifeShape {
                binder_count: 3,
                ports: vec![
                    RetainedBorrowPort {
                        leaf: address,
                        mode: StoredBorrowMode::SharedAddress,
                        binder: 0
                    };
                    2
                ],
                retained: life.retained,
                nominal_arguments: life.nominal_arguments,
            },
            slots: vec![],
        };
        let limits = Limits::default();
        let ports = check(
            &module,
            &interface,
            |ty, b| values::intrinsic(&module, ty, b).map(|d| d.count),
            &mut Budget::new(&limits),
        )
        .unwrap();
        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].inner, vec![(0, 1)]);
        assert_eq!(ports[1].inner, vec![(0, 2)]);
        let mut coalesced = interface.clone();
        coalesced.retained_life.ports.pop();
        assert!(
            check(
                &module,
                &coalesced,
                |ty, b| values::intrinsic(&module, ty, b).map(|d| d.count),
                &mut Budget::new(&limits)
            )
            .is_err()
        );
        let mut missing = interface;
        missing.retained_life.retained.pop();
        assert!(
            check(
                &module,
                &missing,
                |ty, b| values::intrinsic(&module, ty, b).map(|d| d.count),
                &mut Budget::new(&limits)
            )
            .is_err()
        );
    }
}
