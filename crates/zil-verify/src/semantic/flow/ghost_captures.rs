//! Whole logical environments keep every intrinsic occurrence, including
//! physical borrows and ghosts. Every physical capture must still select the
//! exact original free binder at its real creator; no class creates a root.
use super::*;
impl CheckedContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn environment_capture(&self,module:&TypedModule,creator:&SemanticInstance,child:&SemanticInstance,node:zil_base::NodeId,value:ValueId,actual:&CheckedType,values:Option<&[Option<CheckedType>]>,sealed:&[CheckedActual],owner_description:bool,environment:&EvalEnv,b:&mut Budget)->Check<usize>{
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=creator.target else{return Err(bad("logical capture creator is not an actual function"));};
        let f=module.functions.get(function.0 as usize).ok_or_else(||bad("logical capture creator function is absent"))?;
        let mut selected=None;
        for(ordinal,expected)in sealed.iter().enumerate(){
            let CheckedActual::Type(original)=expected else{continue;};
            let projected=if owner_description{
                if !matches!(original.ty,Type::Managed(_)){continue;}
                Some(self.child_value(module,original,&[LifeTypeProjection::Pointee],b)?)
            }else{None};
            let expected=projected.as_ref().unwrap_or(original);
            if expected.ty!=actual.ty{continue;}
            complete_environment(module,original,b)?;
            let source=self.original.borrow().as_ref().ok_or_else(||bad("logical capture has no checked original body"))?.ghost_capture_source(module,creator.definition,child.definition,ordinal,node,b)?;
            // Resolve every same-shaped original candidate by its real binder.
            // A different candidate cannot donate its class inventory.
            if !self.capture_creator_source(module,creator,f,value,source,b)?{continue;}
            if !self.same_type(environment,actual,expected,b)? {
                let Some(values)=values else{return Err(bad("logical capture changes the complete original type/class/origin occurrence"));};
                // Reconstruct a compiler-created reborrow of this exact original
                // binder. The stored-target and loan kernels still check its use.
                let scope=ScopeKey::Instance(creator.id);
                let mut head=value;
                let mut captured=actual;
                let mut proved=false;
                for _ in 0..b.limits.max_call_depth.min(128) {
                    let instruction=super::call_borrows::call_borrow_producer(f,head,b)?;
                    let InstructionKind::ReborrowAddress{address,mutable} = instruction.kind else{break;};
                    let parent=value_at(values,address)?;
                    if parent.ty!=expected.ty {break;}
                    let replay=self.reborrow_value(parent,scope,head,mutable,b)?;
                    if !self.same_type(environment,&replay,captured,b)? {break;}
                    if self.same_type(environment,parent,expected,b)? {proved=true;break;}
                    head=address;captured=parent;
                }
                if !proved{return Err(bad("logical capture changes the complete original type/class/origin occurrence"));}
            }
            if selected.replace(ordinal).is_some(){return Err(bad("logical capture has several original environment sources"));}
        }
        selected.ok_or_else(||bad("logical capture does not select its exact original free-value binding"))
    }
}

#[cfg(test)]
mod retained_capture_tests {
    use super::*;
    #[test]
    fn retained_capture_ports_preserve_exact_environment_ordinals_and_child_classes() {
        let limits=Limits::default();let mut b=Budget::new(&limits);
        let (module,_)=zil_ir::wire::decode_module_candidate(include_bytes!("../fixtures/ordinary-identity-complete-head.wire"),&limits).unwrap();
        let context=CheckedContext::catalogue(&module,&mut b).unwrap();
        let mut child=module.semantic.as_ref().unwrap().instances[0].clone();
        child.header_embedding=(0..2).map(|class|SemanticHeaderBinding{
            header:StorageBinder::Fixed(class),actual:SemanticClassExpr::IntrinsicClass{operand:SemanticOperand::Environment(3),path:vec![],class},
        }).collect();
        let leaf=Type::Address{mutable:false,inner:Box::new(Type::I32)};
        let task=Type::Task{result:Box::new(Type::I32),retained_life:RetainedLifeShape{
            binder_count:2,ports:(0..2).map(|binder|RetainedBorrowPort{leaf:leaf.clone(),mode:StoredBorrowMode::SharedAddress,binder}).collect(),
            retained:(0..2).map(|class|StorageRetainedBinding{path:vec![LifeTypeProjection::Parameter(0)],class,binder:class}).collect(),nominal_arguments:vec![],
        }};
        let scope=ScopeKey::Instance(child.id);
        let value=CheckedType{type_cell:crate::semantic::CheckedTypeData::new(Type::Managed(Box::new(task))),
            life:StorageLifeShape{binder_count:2,slots:vec![],retained:(0..2).map(|class|StorageRetainedBinding{path:vec![LifeTypeProjection::Pointee],class,binder:class}).collect(),nominal_arguments:vec![]},
            classes:(0..2).map(|ordinal|ClassKey::Formal{scope,ordinal}).collect(),
            origins:(0..2).map(|class|IntrinsicOrigin::Existing{scope,operand:OperandKey::Environment{scope,ordinal:3},path:vec![],class}).collect(),statics:vec![],
        };
        let relation=StorageLifeContract{fixed_binder_count:2,invocation_binder_count:0,
            bindings:(0..2).map(|port|StorageLifeBinding{position:StoragePosition{base:StorageBase::Capture(0),path:vec![ContractProjection::RetainedPort(port)]},binder:StorageBinder::Fixed(port)}).collect(),
            static_bindings:vec![],classes:(0..2).map(|class|SignatureLifeClassBinding{position:SignatureLifeClass{base:StorageBase::Capture(0),path:vec![LifeTypeProjection::Pointee],class},binder:StorageBinder::Fixed(class)}).collect(),
        };
        let environment=EvalEnv{lexical:vec![],assumptions:vec![],images:vec![],bounds:vec![],invocations:vec![],at:None};
        let mut other_value=value.clone();
        for origin in &mut other_value.origins{let IntrinsicOrigin::Existing{operand,..}=origin else{unreachable!()};*operand=OperandKey::Environment{scope,ordinal:4};}
        let mut sealed=vec![CheckedActual::Type(value.clone());5];sealed[4]=CheckedActual::Type(other_value);
        context.environment_capture_header(&module,&relation,&child,0,3,&value,&sealed,&environment,&mut b).unwrap();
        let mut other=child.clone();other.header_embedding[0].actual=SemanticClassExpr::IntrinsicClass{operand:SemanticOperand::Environment(4),path:vec![],class:0};
        assert!(context.environment_capture_header(&module,&relation,&other,0,3,&value,&sealed,&environment,&mut b).is_err());
        let mut wrong=relation.clone();wrong.bindings[1].position.path=vec![ContractProjection::RetainedPort(2)];
        assert!(context.environment_capture_header(&module,&wrong,&child,0,3,&value,&sealed,&environment,&mut b).is_err());
        let mut swapped=relation.clone();swapped.bindings[1].binder=StorageBinder::Fixed(0);
        assert!(context.environment_capture_header(&module,&swapped,&child,0,3,&value,&sealed,&environment,&mut b).is_err());
        let mut invocation=relation.clone();invocation.bindings[0].binder=StorageBinder::Invocation(0);
        assert!(context.environment_capture_header(&module,&invocation,&child,0,3,&value,&sealed,&environment,&mut b).is_err());
        // Two captures can repeat the very same original source terms. This
        // is distinct from two independent intrinsic identities with equal images.
        let mut derived=value.clone();derived.origins=derived.classes.iter().map(|key|IntrinsicOrigin::DerivedClass{source:key.clone()}).collect();
        let repeated=vec![CheckedActual::Type(derived.clone());2];
        let mut shared=child.clone();
        for row in &mut shared.header_embedding{let SemanticClassExpr::IntrinsicClass{operand,..}=&mut row.actual else{unreachable!()};*operand=SemanticOperand::Environment(0);}
        context.environment_capture_header(&module,&relation,&shared,0,1,&derived,&repeated,&environment,&mut b).unwrap();
        context.borrowed_capture_header(&relation,&shared,0,1,&derived,&repeated,&environment,&mut b).unwrap();
        let mut independent=repeated.clone();
        let CheckedActual::Type(other)=&mut independent[1]else{unreachable!()};
        other.origins=value.origins.clone();
        assert!(context.environment_capture_header(&module,&relation,&shared,0,1,&derived,&independent,&environment,&mut b).is_err(),"equal class images do not substitute an existing child identity");
        assert!(context.borrowed_capture_header(&relation,&shared,0,1,&derived,&independent,&environment,&mut b).is_err());
        let mut missing=relation.clone();missing.bindings.pop();missing.classes.pop();
        assert!(context.environment_capture_header(&module,&missing,&shared,0,1,&derived,&repeated,&environment,&mut b).is_err(),"a phantom class occurrence cannot disappear");
        let mut same_images=value.clone();same_images.classes[1]=same_images.classes[0].clone();
        let same_images_sealed=vec![CheckedActual::Type(same_images.clone());4];
        context.environment_capture_header(&module,&relation,&child,0,3,&same_images,&same_images_sealed,&environment,&mut b).unwrap();
        assert!(context.environment_capture_header(&module,&swapped,&child,0,3,&same_images,&same_images_sealed,&environment,&mut b).is_err(),"distinct retained children keep their indices even with equal images");
    }
}
pub(super) fn complete_environment(module:&TypedModule,value:&CheckedType,b:&mut Budget)->Check<()>{
    dynamic_metadata::ty(&value.ty,b)?;dynamic_metadata::life_shape(&value.life,b)?;
    let count=value.life.binder_count;
    if value.classes.len()!=count as usize||value.origins.len()!=count as usize{return Err(bad("logical environment omits a complete intrinsic class or original origin"));}
    // A bare Address/Desc has no intrinsic static/variable outer decision.
    // Its full occurrence directory supplies that role, still as an untrusted
    // source obligation; the actual producer/loan kernels prove the target.
    b.spend(count as usize)?;
    let classes:Vec<_>=(0..count).map(StorageBinder::Fixed).collect();
    dynamic_witness::check_type_parts(module,&value.ty,&value.life,&classes,DynInstantiationScope{fixed_binder_count:count,invocation_binder_count:0},b)
}

impl CheckedContext{
    /// A fresh Closure domain may reuse an original source term at several
    /// capture occurrences. This is the same origin-and-class theorem used by
    /// fresh retained domains: equal outside images alone are insufficient.
    /// No occurrence or child domain is rewritten by this boundary query.
    pub(super) fn same_capture_anchor(&self,left:&SemanticClassExpr,right:&SemanticClassExpr,sealed:&[CheckedActual],environment:&EvalEnv,b:&mut Budget)->Check<bool>{
        b.spend(1)?;
        if left==right{return Ok(true);}
        fn term<'a>(source:&SemanticClassExpr,sealed:&'a [CheckedActual])->Check<Option<(&'a ClassKey,Option<&'a IntrinsicOrigin>)>>{
            let value=match source {
                SemanticClassExpr::Environment{scope:SemanticScopeRef::Ancestor(0),ordinal}=>match sealed.get(*ordinal as usize){
                    Some(CheckedActual::Life(key))=>(key,None),
                    _=>return Err(bad("capture class source changes its original Life formal kind")),
                },
                SemanticClassExpr::IntrinsicClass{operand:SemanticOperand::Environment(ordinal),path,class}if path.is_empty()=>{
                    let Some(CheckedActual::Type(value))=sealed.get(*ordinal as usize)else{return Err(bad("capture class source changes its original Type environment"));};
                    if value.classes.len()!=value.life.binder_count as usize||value.origins.len()!=value.classes.len(){return Err(bad("capture class source omits its complete original domain"));}
                    let key=value.classes.get(*class as usize).ok_or_else(||bad("capture class source escapes its original class directory"))?;
                    let origin=value.origins.get(*class as usize).ok_or_else(||bad("capture class source omits its original intrinsic identity"))?;
                    (key,Some(origin))
                },
                _=>return Ok(None),
            };Ok(Some(value))
        }
        let(Some((left_key,left_origin)),Some((right_key,right_origin)))=(term(left,sealed)?,term(right,sealed)?)else{return Ok(false);};
        let origin_same=match(left_origin,right_origin){
            (Some(left),Some(right))=>self.same_origin(environment,left,right,b)?,
            (None,None)=>true,
            (None,Some(IntrinsicOrigin::DerivedClass{source}))=>self.same(environment,left_key,source,b)?,
            (Some(IntrinsicOrigin::DerivedClass{source}),None)=>self.same(environment,source,right_key,b)?,
            _=>false,
        };
        Ok(origin_same&&self.same(environment,left_key,right_key,b)?)
    }
    pub(super) fn environment_capture_header(&self,module:&TypedModule,relation:&StorageLifeContract,child:&SemanticInstance,index:u32,ordinal:usize,value:&CheckedType,sealed:&[CheckedActual],environment:&EvalEnv,b:&mut Budget)->Check<()>{
        b.spend(value.classes.len().saturating_add(relation.bindings.len()).saturating_add(relation.classes.len()).saturating_add(relation.static_bindings.len()))?;
        let mut seen=vec![false;value.classes.len()];
        let mut occurrence=|binder:StorageBinder,class:u32,b:&mut Budget|->Check<()>{
            let slot=seen.get_mut(class as usize).ok_or_else(||bad("captured header class escapes its complete original environment"))?;*slot=true;
            b.spend(child.header_embedding.len())?;
            let mut maps=child.header_embedding.iter().filter(|row|row.header==binder);
            let map=maps.next().ok_or_else(||bad("logical capture class has no original environment embedding"))?;
            let expected=SemanticClassExpr::IntrinsicClass{operand:SemanticOperand::Environment(ordinal as u32),path:vec![],class};
            if !matches!(binder,StorageBinder::Fixed(_))||maps.next().is_some()||!self.same_capture_anchor(&map.actual,&expected,sealed,environment,b)?{return Err(bad("logical capture changes an exact original environment occurrence"));}Ok(())
        };
        for row in relation.bindings.iter().filter(|row|row.position.base==StorageBase::Capture(index)){
            let (_,_,class)=task_domains::raw_input_port(module,&value.ty,&value.life,&row.position.path,b)?;
            occurrence(row.binder,class,b)?;
        }
        for row in relation.classes.iter().filter(|row|row.position.base==StorageBase::Capture(index)){
            let class=if row.position.path.is_empty(){row.position.class}else{
                let mut rows=value.life.retained.iter().filter(|actual|actual.path==row.position.path&&actual.class==row.position.class);
                let row=rows.next().ok_or_else(||bad("logical capture omits a full child class occurrence"))?;
                if rows.next().is_some(){return Err(bad("logical capture child occurrence is duplicated"));}row.binder
            };occurrence(row.binder,class,b)?;
        }
        if seen.iter().any(|seen|!*seen){return Err(bad("logical capture header omits an original intrinsic class including ghosts"));}
        for position in relation.static_bindings.iter().filter(|row|row.base==StorageBase::Capture(index)){
            let path=position.path.iter().map(|step|match step{ContractProjection::Place(step)=>Ok(LifeTypeProjection::Place(*step)),_=>Err(unsupported("static captured port requires its complete original leaf selector",None))}).collect::<Check<Vec<_>>>()?;
            if !value.statics.iter().any(|row|matches!(row,StaticOccurrence::Direct{path:actual,..}if actual==&path)){return Err(bad("logical capture Static header has no original pending source occurrence"));}
        }Ok(())
    }
    pub(super) fn borrowed_capture_header(&self,relation:&StorageLifeContract,child:&SemanticInstance,index:u32,ordinal:usize,pointee:&CheckedType,sealed:&[CheckedActual],environment:&EvalEnv,b:&mut Budget)->Check<()>{
        let described=matches!(sealed.get(ordinal),Some(CheckedActual::Type(owner)) if matches!(owner.ty,Type::Managed(_)) && owner.ty!=pointee.ty);
        for class in 0..pointee.life.binder_count{
            b.spend(relation.classes.len().saturating_add(child.header_embedding.len()))?;
            let mut rows=relation.classes.iter().filter(|row|row.position.base==StorageBase::Capture(index)&&row.position.path==[LifeTypeProjection::Pointee]&&row.position.class==class);
            let row=rows.next().ok_or_else(||bad("borrowed capture loses a complete pointee intrinsic occurrence"))?;
            if rows.next().is_some(){return Err(bad("borrowed capture repeats a pointee intrinsic occurrence"));}
            let mut maps=child.header_embedding.iter().filter(|map|map.header==row.binder);
            let map=maps.next().ok_or_else(||bad("borrowed capture pointee has no original environment embedding"))?;
            if maps.next().is_some()||!matches!(row.binder,StorageBinder::Fixed(_))||!self.same_capture_anchor(&map.actual,&SemanticClassExpr::IntrinsicClass{operand:SemanticOperand::Environment(ordinal as u32),path:if described{vec![LifeTypeProjection::Pointee]}else{vec![]},class},sealed,environment,b)?{return Err(bad("borrowed capture changes an independent original pointee class or aliases its outer head"));}
        }Ok(())
    }

}
