//! Storage for exact immutable physical constructors. Logical occurrences are
//! formed independently on every use; this directory carries no Γ or source.
use super::*;
use std::rc::Rc;

#[derive(Clone,Copy,PartialEq,Eq,PartialOrd,Ord)]
pub(crate) enum PhysicalConstructor{Managed,SharedAddress,ExclusiveAddress,Description,Tuple,Slice,OwnedStore,Mutex,Registration,AsyncRegistration}
impl PhysicalConstructor {
    pub(crate) fn child_projection(self) -> Option<LifeTypeProjection> {
        match self {
            Self::Tuple => None,
            Self::Slice | Self::OwnedStore => Some(LifeTypeProjection::Element),
            Self::Managed | Self::Mutex | Self::Registration | Self::AsyncRegistration |
            Self::SharedAddress | Self::ExclusiveAddress | Self::Description => Some(LifeTypeProjection::Pointee),
        }
    }
}
#[derive(PartialEq,Eq,PartialOrd,Ord)]
pub(super) struct ConstructionKey{kind:PhysicalConstructor,children:Vec<usize>,embedded:Option<StorageLifeShape>}
pub(super) struct ConstructedType{
    // Pointer keys remain live even if the logical child values are released.
    _children:Vec<Rc<CheckedTypeData>>,
    value:Rc<CheckedTypeData>,
    checked_bounds:(usize,usize),
}
impl CheckedContext{
    pub(crate) fn physical_constructor(&self,kind:PhysicalConstructor,children:&[CheckedType],life:&StorageLifeShape,b:&mut Budget)->Check<Rc<CheckedTypeData>>{
        let embedded=if kind==PhysicalConstructor::Tuple&&life.binder_count!=0{
            dynamic_metadata::life_shape(life,b)?;Some(life.clone())
        }else{None};
        b.spend(children.len().saturating_add(1))?;
        let key=ConstructionKey{kind,children:children.iter().map(|child|Rc::as_ptr(&child.type_cell) as usize).collect(),embedded};
        let mut key_units=children.len().saturating_add(2);
        if let Some(life)=&key.embedded{
            key_units=key_units.saturating_add(life.slots.len()).saturating_add(life.retained.len()).saturating_add(life.nominal_arguments.len());
            for path in life.slots.iter().map(|row|row.path.len()).chain(life.retained.iter().map(|row|row.path.len())).chain(life.nominal_arguments.iter().map(|row|row.path.len())){key_units=key_units.saturating_add(path);}
        }
        let levels=self.constructed_types.borrow().len().checked_ilog2().map_or(1,|n|n as usize+2);
        b.spend(levels.saturating_mul(key_units))?;
        let bounds=(b.limits.max_nodes,b.limits.max_call_depth.min(128));
        if let Some(value)=self.constructed_types.borrow().get(&key){
            b.spend(2)?;
            if value.checked_bounds!=bounds{dynamic_metadata::bounds_type(&value.value.ty,b)?;}
            return Ok(value.value.clone());
        }
        use PhysicalConstructor as K;
        if kind!=K::Tuple&&children.len()!=1{return Err(bad("physical unary constructor has another child count"));}
        let mut ty=match kind{
            K::Managed=>Type::Managed(Box::new(children[0].raw_type_copy(b)?)),
            K::SharedAddress|K::ExclusiveAddress=>Type::Address{mutable:kind==K::ExclusiveAddress,inner:Box::new(children[0].raw_type_copy(b)?)},
            K::Description=>Type::Desc(Box::new(children[0].raw_type_copy(b)?)),
            K::Slice=>Type::Slice{element:Box::new(children[0].raw_type_copy(b)?)},
            K::OwnedStore=>Type::OwnedStore{element:Box::new(children[0].raw_type_copy(b)?)},
            K::Mutex=>Type::Mutex{payload:Box::new(children[0].raw_type_copy(b)?)},
            K::Registration=>Type::Registration{environment:Box::new(children[0].raw_type_copy(b)?)},
            K::AsyncRegistration=>Type::AsyncRegistration{environment:Box::new(children[0].raw_type_copy(b)?)},
            K::Tuple=>{let mut fields=Vec::with_capacity(children.len());for child in children{fields.push(child.raw_type_copy(b)?);}if fields.is_empty(){Type::Unit}else{Type::Tuple(fields)}},
        };
        if let Some(life)=&key.embedded{dynamic_metadata::life_shape(life,b)?;ty=Type::BorrowedStorage{storage:Box::new(ty),life:life.clone()};}
        let value=dynamic_metadata::adopt_checked_type(ty,b)?;
        b.spend(levels.saturating_mul(key_units).saturating_add(children.len()).saturating_add(2))?;
        self.constructed_types.borrow_mut().insert(key,ConstructedType{_children:children.iter().map(|child|child.type_cell.clone()).collect(),value:value.clone(),checked_bounds:bounds});
        Ok(value)
    }
}

/// A positive receipt covers only the empty lifetime and intrinsic-origin
/// inventory of this immutable checked type. A different module verification,
/// physical cell, or limit profile must establish the inventory again.
#[derive(Default)]
pub(super) struct EmptyDomains {
    by_type: BTreeMap<usize, (Rc<()>, Rc<CheckedTypeData>, Limits)>,
}

impl EmptyDomains {
    pub(super) fn checked(&self, scope: &Rc<()>, cell: &Rc<CheckedTypeData>, b: &mut Budget) -> Check<bool> {
        let key = Rc::as_ptr(cell) as usize;
        b.spend(self.by_type.len().checked_ilog2().map_or(1, |n| n as usize + 2))?;
        let Some((module, owner, limits)) = self.by_type.get(&key) else { return Ok(false) };
        b.spend(13)?;
        Ok(Rc::ptr_eq(module, scope) && Rc::ptr_eq(owner, cell) && *limits == b.limits)
    }
    pub(super) fn record(&mut self, scope: Rc<()>, cell: &Rc<CheckedTypeData>, b: &mut Budget) -> Check<()> {
        if !closed_storage(&cell.ty, b)? { return Ok(()); }
        b.spend(self.by_type.len().checked_ilog2().map_or(1, |n| n as usize + 2).saturating_add(14))?;
        self.by_type.insert(Rc::as_ptr(cell) as usize, (scope, cell.clone(), b.limits.clone()));
        Ok(())
    }
}

/// These physical carriers have no runtime receiver, external signature, or
/// embedded borrow domain. Callers separately check the accompanying life
/// shape and every actual origin before reusing an immutable result.
pub(super) fn closed_storage(ty: &Type, b: &mut Budget) -> Check<bool> {
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        b.spend(1)?;
        match ty {
            ty if ty.is_b0_scalar() => {}
            Type::Array { element, .. } | Type::Managed(element) | Type::OwnedStore { element } => {
                pending.push(element)
            }
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                pending.extend(fields);
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, ty)| ty));
            }
            Type::Enum { variants, .. } => {
                b.spend(variants.len())?;
                for (_, fields) in variants {
                    b.spend(fields.len())?;
                    pending.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                pending.push(error);
                pending.push(result);
            }
            _ => return Ok(false),
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests{
    use super::*;
    fn empty()->StorageLifeShape{StorageLifeShape{binder_count:0,slots:vec![],retained:vec![],nominal_arguments:vec![]}}
    fn child()->CheckedType{CheckedType{type_cell:CheckedTypeData::new(Type::Record{name:"immutable_child_payload_".repeat(64),fields:vec![("field".repeat(16),Type::I32)]}),life:empty(),classes:vec![],origins:vec![],statics:vec![]}}
    #[test]
    fn empty_domain_receipts_bind_the_checked_module_cell_and_limits() {
        let limits = Limits::default();
        let (module, _) = zil_ir::wire::decode_module_candidate(
            include_bytes!("fixtures/ordinary-identity-complete-head.wire"), &limits
        ).unwrap();
        let mut b = Budget::new(&limits);
        b.structural = Some(dynamic_metadata::Structure::register(&module, &mut b).unwrap());
        let context = CheckedContext::definition_catalogue(&module, None, &mut b).unwrap();
        let ty = Type::Tuple(vec![Type::I32, Type::Bool]);
        let value = context.checked_value(&module, &ty, &empty(), vec![], &mut b).unwrap();
        let scope = b.structural.as_ref().unwrap().identity_for(&module).unwrap();
        assert!(context.empty_domains.borrow().checked(&scope, &value.type_cell, &mut b).unwrap());
        let mut other = Budget::new(&limits);
        let other_scope = dynamic_metadata::Structure::register(&module, &mut other).unwrap().identity_for(&module).unwrap();
        assert!(!context.empty_domains.borrow().checked(&other_scope, &value.type_cell, &mut b).unwrap());
        assert!(!context.empty_domains.borrow().checked(&scope, &CheckedTypeData::new(ty), &mut b).unwrap());
        let mut impossible = empty();
        impossible.binder_count = 1;
        assert!(context.checked_value(&module, &value.ty, &impossible, vec![], &mut b).is_err());
        b.limits.max_nodes = 1;
        assert!(context.checked_value(&module, &value.ty, &empty(), vec![], &mut b).is_err());
        assert!(context.checked_value(&module, &Type::I32, &empty(), vec![], &mut b).is_err());
    }
    #[test]
    fn physical_constructors_reuse_exact_live_children_without_merging_domain_shapes(){
        let limits=Limits::default();
        let(module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("fixtures/ordinary-identity-complete-head.wire"),&limits).unwrap();
        let mut b=Budget::new(&limits);b.structural=Some(dynamic_metadata::Structure::register(&module,&mut b).unwrap());
        let context=CheckedContext::catalogue(&module,&mut b).unwrap();
        let child=child();let values=[child.clone()];
        let before=b.remaining;let first=context.physical_constructor(PhysicalConstructor::Managed,&values,&empty(),&mut b).unwrap();let cold=before-b.remaining;
        let before=b.remaining;let second=context.physical_constructor(PhysicalConstructor::Managed,&values,&empty(),&mut b).unwrap();let hot=before-b.remaining;
        assert!(Rc::ptr_eq(&first,&second));assert!(hot<cold);
        // Physical bytes never constitute a resource declaration or owner proof.
        assert!(crate::resources::check_type_for(&module,&first.ty,None,&mut b).is_err());
        let mut detached=child.clone();detached.replace_type(child.raw_type_copy(&mut b).unwrap());
        let distinct=context.physical_constructor(PhysicalConstructor::Managed,&[detached],&empty(),&mut b).unwrap();
        assert!(!Rc::ptr_eq(&first,&distinct),"same shape is not an immutable child allocation identity");
        let mut one=empty();one.binder_count=1;one.retained=vec![StorageRetainedBinding{path:vec![LifeTypeProjection::Place(PlaceProjection::Field(0))],class:0,binder:0}];
        let mut two=one.clone();two.binder_count=2;two.retained[0].binder=1;
        let tuple=context.physical_constructor(PhysicalConstructor::Tuple,&values,&one,&mut b).unwrap();
        let other=context.physical_constructor(PhysicalConstructor::Tuple,&values,&two,&mut b).unwrap();
        assert!(!Rc::ptr_eq(&tuple,&other));assert_ne!(tuple.ty,other.ty);
        b.limits.max_nodes=1;
        assert!(context.physical_constructor(PhysicalConstructor::Managed,&values,&empty(),&mut b).is_err());
        b.limits.max_nodes=limits.max_nodes;b.limits.max_call_depth=0;
        assert!(context.physical_constructor(PhysicalConstructor::Managed,&values,&empty(),&mut b).is_err());
        b.limits.max_call_depth=limits.max_call_depth;
        assert!(Rc::ptr_eq(&first,&context.physical_constructor(PhysicalConstructor::Managed,&values,&empty(),&mut b).unwrap()));
    }
}
