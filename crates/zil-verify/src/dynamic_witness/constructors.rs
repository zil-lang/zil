//! Fresh ordinary constructors preserve complete child directories and partition
//! only equal intrinsic origins. Existing child type values are never rewritten.
use super::*;

#[derive(Default)]
struct Rows {
    slots: Vec<(Vec<PlaceProjection>, StoredBorrowMode, terms::LifeTerm)>,
    retained: Vec<(Vec<LifeTypeProjection>, u32, terms::LifeTerm)>,
    nominal: Vec<(NominalLifeArgument, terms::LifeTerm)>,
}
impl Rows {
    fn child(
        &mut self,
        child: &terms::Value,
        physical: &[PlaceProjection],
        semantic: &[LifeTypeProjection],
        inline: bool,
        b: &mut Budget,
    ) -> Check<()> {
        dynamic_metadata::recipe(&child.raw, b)?;
        let boundary = matches!(
            child.raw.ty,
            Type::BorrowedStorage { .. }
                | Type::Closure(_)
                | Type::Task { .. }
                | Type::Dyn { .. }
                | Type::OwnedDyn { .. }
        );
        if boundary {
            for (class, term) in child.terms.iter().enumerate() {
                b.spend(semantic.len().saturating_add(1))?;
                self.retained.push((semantic.to_vec(), class as u32, *term));
            }
        } else {
            for row in &child.raw.life.retained {
                b.spend(
                    semantic
                        .len()
                        .saturating_add(row.path.len())
                        .saturating_add(1),
                )?;
                let mut path = semantic.to_vec();
                path.extend_from_slice(&row.path);
                self.retained
                    .push((path, row.class, child.term(row.binder, b)?));
            }
        }
        if inline {
            for row in &child.raw.life.slots {
                b.spend(
                    physical
                        .len()
                        .saturating_add(row.path.len())
                        .saturating_add(1),
                )?;
                let mut path = physical.to_vec();
                path.extend_from_slice(&row.path);
                self.slots
                    .push((path, row.mode, child.term(row.binder, b)?));
            }
        } else if !boundary && !child.raw.life.slots.is_empty() {
            return Err(bad(
                "raw semantic pointee borrow needs an independent complete child scope",
            ));
        }
        for row in &child.raw.life.nominal_arguments {
            b.spend(
                semantic
                    .len()
                    .saturating_add(row.path.len())
                    .saturating_add(1),
            )?;
            let mut row = row.clone();
            let term = child.term(row.binder, b)?;
            let mut path = semantic.to_vec();
            path.extend_from_slice(&row.path);
            row.path = path;
            self.nominal.push((row, term));
        }
        Ok(())
    }
    fn finish(self, mut ty: Type, qualified: bool, b: &mut Budget) -> Check<terms::Value> {
        b.spend(
            self.nominal
                .len()
                .saturating_add(self.slots.len())
                .saturating_add(self.retained.len()),
        )?;
        let occurrences = self
            .nominal
            .iter()
            .map(|(_, term)| *term)
            .chain(self.slots.iter().map(|(_, _, term)| *term))
            .chain(self.retained.iter().map(|(_, _, term)| *term))
            .collect::<Vec<_>>();
        let (partition, unique) = terms::partition(&occurrences, b)?;
        let mut partition = partition.into_iter();
        let mut nominal_arguments = Vec::with_capacity(self.nominal.len());
        for (mut row, _) in self.nominal {
            row.binder = partition
                .next()
                .ok_or_else(|| bad("fresh nominal partition is incomplete"))?;
            nominal_arguments.push(row);
        }
        let mut slots = Vec::with_capacity(self.slots.len());
        for (path, mode, _) in self.slots {
            slots.push(StorageLifeSlot {
                path,
                mode,
                binder: partition
                    .next()
                    .ok_or_else(|| bad("fresh borrow partition is incomplete"))?,
            });
        }
        let mut retained = Vec::with_capacity(self.retained.len());
        for (path, class, _) in self.retained {
            retained.push(StorageRetainedBinding {
                path,
                class,
                binder: partition
                    .next()
                    .ok_or_else(|| bad("fresh child partition is incomplete"))?,
            });
        }
        let life = StorageLifeShape {
            binder_count: unique.len() as u32,
            slots,
            retained,
            nominal_arguments,
        };
        dynamic_metadata::ty(&ty, b)?;
        if qualified && !unique.is_empty() {
            ty = Type::BorrowedStorage {
                storage: Box::new(ty),
                life: life.clone(),
            };
        }
        Ok(terms::Value {
            raw: DynTypeValue {
                ty,
                life,
                classes: unique.iter().map(|v| v.image).collect(),
            },
            terms: unique,
        })
    }
}
pub(super) fn address(
    child: &terms::Value,
    term: terms::LifeTerm,
    mode: StoredBorrowMode,
    b: &mut Budget,
) -> Check<terms::Value> {
    let mut rows = Rows::default();
    rows.slots.push((vec![], mode, term));
    rows.child(child, &[], &[LifeTypeProjection::Pointee], false, b)?;
    let ty = match mode {
        StoredBorrowMode::SharedAddress | StoredBorrowMode::ExclusiveAddress => Type::Address {
            mutable: mode == StoredBorrowMode::ExclusiveAddress,
            inner: Box::new(child.raw.ty.clone()),
        },
        StoredBorrowMode::ReadOnlyDescription | StoredBorrowMode::WritableDescription => {
            Type::Desc(Box::new(child.raw.ty.clone()))
        }
    };
    rows.finish(ty, false, b)
}
pub(super) fn managed(child: &terms::Value, b: &mut Budget) -> Check<terms::Value> {
    let mut rows = Rows::default();
    rows.child(child, &[], &[LifeTypeProjection::Pointee], false, b)?;
    rows.finish(Type::Managed(Box::new(child.raw.ty.clone())), false, b)
}
pub(super) fn tuple(children: &[terms::Value], b: &mut Budget) -> Check<terms::Value> {
    b.spend(children.len())?;
    let mut rows = Rows::default();
    let mut fields = Vec::with_capacity(children.len());
    for (index, child) in children.iter().enumerate() {
        let field = PlaceProjection::Field(index as u32);
        rows.child(
            child,
            &[field],
            &[LifeTypeProjection::Place(field)],
            true,
            b,
        )?;
        fields.push(child.raw.ty.clone());
    }
    rows.finish(if fields.is_empty(){Type::Unit}else{Type::Tuple(fields)}, true, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn same_image_children_keep_full_nominal_occurrences_through_real_wrappers() {
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        let declaration = LifeDeclarationId {
            module: zil_base::ModuleId(0),
            declaration: zil_base::NodeId(90),
            compiler: false,
        };
        let life = StorageLifeShape {
            binder_count: 1,
            slots: vec![],
            retained: vec![],
            nominal_arguments: vec![NominalLifeArgument {
                path: vec![],
                declaration,
                argument: 0,
                binder: 0,
            }],
        };
        let child = |class| terms::Value {
            raw: DynTypeValue {
                ty: Type::BorrowedStorage {
                    storage: Box::new(Type::Record {
                        name: "Tag".into(),
                        fields: vec![],
                    }),
                    life: life.clone(),
                },
                life: life.clone(),
                classes: vec![StorageBinder::Fixed(0)],
            },
            terms: vec![terms::LifeTerm {
                origin: terms::Origin {
                    owner: terms::Owner::Function(FunctionId(0)),
                    root: terms::Root::Receiver,
                    class,
                },
                image: StorageBinder::Fixed(0),
            }],
        };
        let first = child(0);
        let second = child(1);
        let tuple_value = tuple(&[first.clone(), second], &mut budget).unwrap();
        assert_eq!(tuple_value.life.binder_count, 2);
        assert_eq!(tuple_value.classes, [StorageBinder::Fixed(0); 2]);
        assert_eq!(tuple_value.life.retained.len(), 2);
        assert_eq!(tuple_value.life.nominal_arguments.len(), 2);
        assert_ne!(
            tuple_value.life.nominal_arguments[0].binder,
            tuple_value.life.nominal_arguments[1].binder
        );
        let same = tuple(&[first.clone(), first.clone()], &mut budget).unwrap();
        assert_eq!(same.life.binder_count, 1);
        assert_eq!(same.life.retained.len(), 2);
        let owner = managed(&first, &mut budget).unwrap();
        let head = terms::LifeTerm {
            origin: terms::Origin {
                owner: terms::Owner::Function(FunctionId(0)),
                root: terms::Root::Receiver,
                class: 2,
            },
            image: StorageBinder::Fixed(0),
        };
        let borrowed = address(&owner, head, StoredBorrowMode::SharedAddress, &mut budget).unwrap();
        assert_eq!(borrowed.life.binder_count, 2);
        assert_eq!(borrowed.life.slots.len(), 1);
        assert_eq!(
            borrowed.life.nominal_arguments[0].path,
            [LifeTypeProjection::Pointee, LifeTypeProjection::Pointee]
        );
        assert_eq!(
            borrowed.life.retained[0].path,
            [LifeTypeProjection::Pointee, LifeTypeProjection::Pointee]
        );
    }
}
