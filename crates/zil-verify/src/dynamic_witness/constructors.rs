//! Fresh ordinary constructors preserve complete child directories and partition
//! only equal intrinsic origins. Existing child type values are never rewritten.
use super::*;

#[derive(Clone,Copy)]
pub(super) struct View<'a>{
    pub ty:&'a Type,
    pub life:&'a StorageLifeShape,
    pub classes:&'a [StorageBinder],
    pub terms:&'a [terms::LifeTerm],
}
impl<'a> View<'a>{
    fn of(value:&'a terms::Value)->Self{Self{ty:&value.raw.ty,life:&value.raw.life,classes:&value.raw.classes,terms:&value.terms}}
    fn term(self,class:u32,b:&mut Budget)->Check<terms::LifeTerm>{b.spend(1)?;self.terms.get(class as usize).copied().ok_or_else(||bad("actual intrinsic occurrence has no complete origin term"))}
}

#[derive(Default)]
struct Rows {
    slots: Vec<(Vec<PlaceProjection>, StoredBorrowMode, terms::LifeTerm)>,
    retained: Vec<(Vec<LifeTypeProjection>, u32, terms::LifeTerm)>,
    nominal: Vec<(NominalLifeArgument, terms::LifeTerm)>,
}
impl Rows {
    fn child(
        &mut self,
        child: View<'_>,
        physical: &[PlaceProjection],
        semantic: &[LifeTypeProjection],
        inline: bool,
        b: &mut Budget,
    ) -> Check<()> {
        dynamic_metadata::bounds_type_parts(child.ty,child.life,child.classes,b)?;
        let boundary = matches!(
            child.ty,
            Type::BorrowedStorage { .. }
                | Type::Closure(_)
                | Type::Task { .. }
                | Type::MutexGuard { .. }
                | Type::Dyn { .. }
                | Type::OwnedDyn { .. }
        );
        if boundary {
            for (class, term) in child.terms.iter().enumerate() {
                b.spend(semantic.len().saturating_add(1))?;
                self.retained.push((semantic.to_vec(), class as u32, *term));
            }
        } else {
            for row in &child.life.retained {
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
            for row in &child.life.slots {
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
        } else if !boundary && !child.life.slots.is_empty() {
            return Err(bad(
                "raw semantic pointee borrow needs an independent complete child scope",
            ));
        }
        for row in &child.life.nominal_arguments {
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
    fn shape(self,b:&mut Budget)->Check<Shape>{self.shape_seeded(&[],b)}
    fn shape_seeded(self,seed:&[terms::LifeTerm],b:&mut Budget)->Check<Shape>{
        b.spend(
            self.nominal
                .len()
                .saturating_add(self.slots.len())
                .saturating_add(self.retained.len()),
        )?;
        b.spend(seed.len())?;
        let occurrences = seed.iter().copied().chain(self
            .nominal
            .iter()
            .map(|(_, term)| *term))
            .chain(self.slots.iter().map(|(_, _, term)| *term))
            .chain(self.retained.iter().map(|(_, _, term)| *term))
            .collect::<Vec<_>>();
        let (partition, unique) = terms::partition(&occurrences, b)?;
        let mut partition = partition.into_iter().skip(seed.len());
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
        b.spend(unique.len())?;
        Ok(Shape{life,classes:unique.iter().map(|term|term.image).collect(),terms:unique})
    }
    fn finish(self,ty:Type,qualified:bool,b:&mut Budget)->Check<terms::Value>{self.finish_seeded(ty,qualified,&[],b)}
    fn finish_seeded(self,mut ty:Type,qualified:bool,seed:&[terms::LifeTerm],b:&mut Budget)->Check<terms::Value>{
        let shape=if seed.is_empty(){self.shape(b)?}else{self.shape_seeded(seed,b)?};
        dynamic_metadata::bounds_type(&ty,b)?;
        if qualified&&!shape.terms.is_empty(){
            dynamic_metadata::life_shape(&shape.life,b)?;
            ty=Type::BorrowedStorage{storage:Box::new(ty),life:shape.life.clone()};
        }
        Ok(terms::Value{raw:DynTypeValue{ty,life:shape.life,classes:shape.classes},terms:shape.terms})
    }

}

pub(super) fn address(child:&terms::Value,term:terms::LifeTerm,mode:StoredBorrowMode,b:&mut Budget)->Check<terms::Value>{address_view(View::of(child),Some(term),mode,b)}
pub(super) fn managed(child:&terms::Value,b:&mut Budget)->Check<terms::Value>{managed_view(View::of(child),b)}
pub(super) fn tuple(children:&[terms::Value],b:&mut Budget)->Check<terms::Value>{
    b.spend(children.len())?;let views:Vec<_>=children.iter().map(View::of).collect();tuple_views(&views,b)
}
pub(super) fn address_view(child:View<'_>,term:Option<terms::LifeTerm>,mode:StoredBorrowMode,b:&mut Budget)->Check<terms::Value>{
    let mut rows=Rows::default();
    if let Some(term)=term{rows.slots.push((vec![],mode,term));}
    else if !matches!(mode,StoredBorrowMode::SharedAddress|StoredBorrowMode::ReadOnlyDescription){return Err(bad("static type constructor cannot invent mutable storage"));}
    rows.child(child,&[],&[LifeTypeProjection::Pointee],false,b)?;
    dynamic_metadata::ty(child.ty,b)?;
    let ty=match mode{
        StoredBorrowMode::SharedAddress|StoredBorrowMode::ExclusiveAddress=>Type::Address{mutable:mode==StoredBorrowMode::ExclusiveAddress,inner:Box::new(child.ty.clone())},
        _=>Type::Desc(Box::new(child.ty.clone())),
    };
    b.spend(child.terms.len().saturating_add(1))?;
    let seed=term.into_iter().chain(child.terms.iter().copied()).collect::<Vec<_>>();
    rows.finish_seeded(ty,false,&seed,b)
}
pub(super) fn managed_view(child:View<'_>,b:&mut Budget)->Check<terms::Value>{
    let mut rows=Rows::default();rows.child(child,&[],&[LifeTypeProjection::Pointee],false,b)?;
    dynamic_metadata::ty(child.ty,b)?;
    rows.finish_seeded(Type::Managed(Box::new(child.ty.clone())),false,child.terms,b)
}
pub(super) fn tuple_views(children:&[View<'_>],b:&mut Budget)->Check<terms::Value>{
    b.spend(children.len())?;let mut rows=Rows::default();let mut fields=Vec::with_capacity(children.len());
    for(index,child)in children.iter().copied().enumerate(){
        let field=PlaceProjection::Field(index as u32);
        rows.child(child,&[field],&[LifeTypeProjection::Place(field)],true,b)?;
        dynamic_metadata::ty(child.ty,b)?;fields.push(child.ty.clone());
    }
    rows.finish(if fields.is_empty(){Type::Unit}else{Type::Tuple(fields)},true,b)
}

/// Fresh logical occurrence directories, independent of physical Type storage.
pub(super) struct Shape{pub life:StorageLifeShape,pub classes:Vec<StorageBinder>,pub terms:Vec<terms::LifeTerm>}
pub(super) fn shape_views(kind:crate::semantic::PhysicalConstructor,children:&[View<'_>],head:Option<terms::LifeTerm>,mode:Option<StoredBorrowMode>,b:&mut Budget)->Check<Shape>{
    use crate::semantic::PhysicalConstructor as K;
    b.spend(children.len())?;let mut rows=Rows::default();
    if kind==K::Tuple{
        for(index,child)in children.iter().copied().enumerate(){let field=PlaceProjection::Field(index as u32);rows.child(child,&[field],&[LifeTypeProjection::Place(field)],true,b)?;}
    }else{
        if children.len()!=1{return Err(bad("unary semantic constructor has another child count"));}
        if let Some(mode)=mode{
            if let Some(head)=head{rows.slots.push((vec![],mode,head));}
            else if !matches!(mode,StoredBorrowMode::SharedAddress|StoredBorrowMode::ReadOnlyDescription){return Err(bad("static type constructor cannot invent mutable storage"));}
        }
        rows.child(children[0],&[],&[LifeTypeProjection::Pointee],false,b)?;
    }
    if kind==K::Tuple{rows.shape(b)}else{
        b.spend(children[0].terms.len().saturating_add(1))?;
        let seed=head.into_iter().chain(children[0].terms.iter().copied()).collect::<Vec<_>>();
        rows.shape_seeded(&seed,b)
    }
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

#[cfg(test)]
mod wrapper_order_tests {
    use super::*;
    #[test]
    fn fresh_wrappers_preserve_child_ordinals_despite_nominal_row_order_and_equal_images(){
        let limits=Limits::default();let mut b=Budget::new(&limits);
        let term=|class|terms::LifeTerm{origin:terms::Origin{owner:terms::Owner::Function(FunctionId(0)),root:terms::Root::Receiver,class},image:StorageBinder::Fixed(0)};
        let declaration=LifeDeclarationId{module:zil_base::ModuleId(0),declaration:zil_base::NodeId(80),compiler:false};
        let life=StorageLifeShape{binder_count:2,slots:vec![],retained:vec![],nominal_arguments:vec![
            NominalLifeArgument{path:vec![],declaration,argument:0,binder:1},
            NominalLifeArgument{path:vec![],declaration,argument:1,binder:0},
        ]};
        let child=terms::Value{raw:DynTypeValue{ty:Type::BorrowedStorage{storage:Box::new(Type::Record{name:"WrapperOrder".into(),fields:vec![]}),life:life.clone()},life,classes:vec![StorageBinder::Fixed(0);2]},terms:vec![term(0),term(1)]};
        // Logical constructor control: no SSA, source frame or permission is
        // admitted by this standalone complete child-domain value.
        let address=address(&child,term(2),StoredBorrowMode::SharedAddress,&mut b).unwrap();
        let shape=shape_views(crate::semantic::PhysicalConstructor::SharedAddress,&[View::of(&child)],Some(term(2)),Some(StoredBorrowMode::SharedAddress),&mut b).unwrap();
        assert_eq!(address.terms,vec![term(2),term(0),term(1)]);
        assert_eq!(shape.terms,address.terms);assert_eq!(shape.life,address.life);
        assert_eq!(address.life.slots[0].binder,0);
        assert_eq!(address.life.retained.iter().map(|row|(row.class,row.binder)).collect::<Vec<_>>(),vec![(0,1),(1,2)]);
        assert_eq!(address.life.nominal_arguments.iter().map(|row|(row.argument,row.binder)).collect::<Vec<_>>(),vec![(0,2),(1,1)]);
        let managed=managed(&child,&mut b).unwrap();assert_eq!(managed.terms,child.terms);
        let static_view=address_view(View::of(&child),None,StoredBorrowMode::SharedAddress,&mut b).unwrap();
        assert_eq!(static_view.terms,child.terms);assert!(static_view.life.slots.is_empty());
    }
}
