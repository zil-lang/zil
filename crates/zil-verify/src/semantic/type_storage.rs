//! Storage for exact immutable physical constructors. Logical occurrences are
//! formed independently on every use; this directory carries no Γ or source.
use super::*;
use std::rc::Rc;

#[derive(Clone,Copy,PartialEq,Eq,PartialOrd,Ord)]
pub(crate) enum PhysicalConstructor{Managed,SharedAddress,ExclusiveAddress,Description,Tuple}
#[derive(PartialEq,Eq,PartialOrd,Ord)]
pub(super) struct ConstructionKey{kind:PhysicalConstructor,children:Vec<usize>,embedded:Option<StorageLifeShape>}
pub(super) struct ConstructedType{
    // Pointer keys remain live even if the logical child values are released.
    _children:Vec<Rc<CheckedTypeData>>,
    value:Rc<CheckedTypeData>,
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
        if let Some(value)=self.constructed_types.borrow().get(&key){
            dynamic_metadata::bounds_type(&value.value.ty,b)?;
            return Ok(value.value.clone());
        }
        use PhysicalConstructor as K;
        if kind!=K::Tuple&&children.len()!=1{return Err(bad("physical unary constructor has another child count"));}
        let mut ty=match kind{
            K::Managed=>Type::Managed(Box::new(children[0].raw_type_copy(b)?)),
            K::SharedAddress|K::ExclusiveAddress=>Type::Address{mutable:kind==K::ExclusiveAddress,inner:Box::new(children[0].raw_type_copy(b)?)},
            K::Description=>Type::Desc(Box::new(children[0].raw_type_copy(b)?)),
            K::Tuple=>{let mut fields=Vec::with_capacity(children.len());for child in children{fields.push(child.raw_type_copy(b)?);}if fields.is_empty(){Type::Unit}else{Type::Tuple(fields)}},
        };
        if let Some(life)=&key.embedded{dynamic_metadata::life_shape(life,b)?;ty=Type::BorrowedStorage{storage:Box::new(ty),life:life.clone()};}
        dynamic_metadata::bounds_type(&ty,b)?;
        let value=CheckedTypeData::new(ty);
        dynamic_metadata::adopt_type_cell(&value,b)?;
        dynamic_metadata::bounds_type(&value.ty,b)?;
        b.spend(levels.saturating_mul(key_units).saturating_add(children.len()).saturating_add(2))?;
        self.constructed_types.borrow_mut().insert(key,ConstructedType{_children:children.iter().map(|child|child.type_cell.clone()).collect(),value:value.clone()});
        Ok(value)
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    fn empty()->StorageLifeShape{StorageLifeShape{binder_count:0,slots:vec![],retained:vec![],nominal_arguments:vec![]}}
    fn child()->CheckedType{CheckedType{type_cell:CheckedTypeData::new(Type::Record{name:"immutable_child_payload_".repeat(64),fields:vec![("field".repeat(16),Type::I32)]}),life:empty(),classes:vec![],origins:vec![],statics:vec![]}}
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
