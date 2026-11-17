//! Original place observations retain binder identity and ordered field paths.
//! They do not create physical addresses or executable borrow permissions.
use super::*;
pub(in crate::semantic) struct Assignment {
    pub(in crate::semantic) statement: NodeId,
    pub(in crate::semantic) target: NodeId,
    pub(in crate::semantic) value: NodeId,
    pub(in crate::semantic) span: Span,
    pub(in crate::semantic) statement_span: Span,
    pub(in crate::semantic) compound: Option<BinaryOp>,
}
fn nonnegative_index(value:&Constant)->Option<u64>{match value{Constant::I8(value)=>u64::try_from(*value).ok(),Constant::I16(value)=>u64::try_from(*value).ok(),Constant::I32(value)=>u64::try_from(*value).ok(),Constant::I64(value)|Constant::Isize(value)=>u64::try_from(*value).ok(),Constant::U8(value)=>Some(u64::from(*value)),Constant::U16(value)=>Some(u64::from(*value)),Constant::U32(value)=>Some(u64::from(*value)),Constant::U64(value)|Constant::Usize(value)=>Some(*value),_=>None}}
impl CheckedOriginal {
    /// The private callback cell represents this actual mutable source binder,
    /// not an arbitrary one-element array of a coincidentally equal type.
    pub(in crate::semantic) fn persistent_binding(&self,module:&TypedModule,definition:SemanticDefinitionId,site:SemanticSiteId,b:&mut Budget)->Check<(NodeId,NodeId)>{
        if !self.checked.contains(&definition){return Err(bad("persistent callback binder lacks its complete original proof"));}
        let index=self.source_index(module,&[definition],b)?;
        let function=index.definitions.get(&definition).ok_or_else(||bad("persistent callback binder has no original function"))?;
        let Some(index::Node::Statement(statement))=function.nodes.get(&site)else{return Err(bad("persistent callback cell selects no original declaration"));};
        let CoreStatementKind::Binding{mutable:true,initializer:Some(initializer),..}=&statement.kind else{return Err(bad("persistent callback cell does not represent an initialized mutable binder"));};
        Ok((statement.id,initializer.id))
    }
    /// These source wrappers emit no value operation. Their full types and
    /// ownership effects have already passed the complete original body check;
    /// actual transport must still prove the selected operand's real origin.
    pub(in crate::semantic) fn assignment_value_source(&self,module:&TypedModule,definition:SemanticDefinitionId,node:NodeId,b:&mut Budget)->Check<(NodeId,bool)>{
        if !self.checked.contains(&definition){return Err(bad("assignment value source has no complete original body proof"));}
        let index=self.source_index(module,&[definition],b)?;
        let function=index.definitions.get(&definition).ok_or_else(||bad("assignment value owner has no original body"))?;
        let mut expression=*function.expressions.get(&node).ok_or_else(||bad("assignment value original expression is absent"))?;
        loop{
            b.spend(1)?;
            self.original_type(definition,&SemanticOriginalTypeSource::Expression(expression.id),b)?;
            match &expression.kind{
                CoreExprKind::Group(value)|CoreExprKind::Ascribed{value,..}|CoreExprKind::Unary{op:UnaryOp::Give,operand:value}=>expression=value,
                _=>break,
            }
        }
        b.spend(self.value_places.len().checked_ilog2().map_or(1,|n|n as usize+2))?;
        Ok((expression.id,self.value_places.contains_key(&(definition,expression.id))))
    }
    pub(in crate::semantic) fn bound_value_name(&self,definition:SemanticDefinitionId,node:NodeId,b:&mut Budget)->Check<bool>{
        b.spend(self.value_bindings.len().checked_ilog2().map_or(1,|depth|depth as usize+1))?;
        Ok(self.value_bindings.contains_key(&(definition,node)))
    }
    pub(in crate::semantic) fn assignments(&self,module:&TypedModule,definition:SemanticDefinitionId,b:&mut Budget)->Check<Vec<Assignment>> {
        if !self.checked.contains(&definition){return Err(bad("assignment source body is not completely checked"));}
        let index=self.source_index(module,&[definition],b)?;
        let function=index.definitions.get(&definition).ok_or_else(||bad("assignment source has no complete original body"))?;
        let mut result=Vec::new();
        for statement in function.statements.values(){
            b.spend(1)?;
            let(target,value,compound)=match &statement.kind{
                CoreStatementKind::Assign{target,value}=>(target,value,None),
                CoreStatementKind::CompoundAssign{target,value,op}=>(target,value,Some(*op)),
                _=>continue,
            };
            b.spend(result.len().saturating_add(1))?;
            result.push(Assignment{statement:statement.id,target:target.id,value:value.id,span:target.span,statement_span:statement.span,compound});
        }
        Ok(result)
    }
    pub(in crate::semantic) fn scalar_input<'a>(&'a self,module:&TypedModule,definition:SemanticDefinitionId,node:NodeId,b:&mut Budget)->Check<(&'a SemanticOriginalExpr,Type)>{
        let Ty::Atom(carrier)=self.original_type(definition,&SemanticOriginalTypeSource::Expression(node),b)? else{return Err(bad("scalar source input lacks its independent original carrier"));};
        if !carrier.is_integer()&&!carrier.is_float()&&!matches!(carrier,Type::Unit|Type::Bool){return Err(bad("source input is not an independently checked scalar expression"));}
        let index=self.source_index(module,&[definition],b)?;let function=index.definitions.get(&definition).ok_or_else(||bad("scalar source input original owner is absent"))?;
        let expression=*function.expressions.get(&node).ok_or_else(||bad("scalar source input original expression is absent"))?;
        Ok((expression,carrier))
    }
    pub(in crate::semantic) fn address_place(&self,module:&TypedModule,definition:SemanticDefinitionId,node:NodeId,b:&mut Budget)->Check<(intrinsics::AddressRoot,Vec<intrinsics::AddressProjection>)>{
        let index=self.source_index(module,&[definition],b)?;let function=index.definitions.get(&definition).ok_or_else(||bad("indirect address has no complete original owner body"))?;
        let mut expression=*function.expressions.get(&node).ok_or_else(||bad("indirect address original operand is absent"))?;
        let mut projections=Vec::new();
        loop{
            b.spend(1)?;
            if expression.id!=node&&matches!(self.resolved(self.expressions.get(&(definition,expression.id)).ok_or_else(||bad("address parent has no independent original type"))?,b)?,Ty::Address{..}){
                if self.value_places.contains_key(&(definition,expression.id)){return Ok((intrinsics::AddressRoot::Place(expression.id),projections));}
                return Ok((intrinsics::AddressRoot::Value(expression.id),projections));
            }
            expression=match &expression.kind{
                CoreExprKind::Group(inner)|CoreExprKind::Ascribed{value:inner,..}=>inner,
                CoreExprKind::Member{object,member,..}=>{b.spend(member.len())?;projections.push(intrinsics::AddressProjection::Field(member.clone()));object},
                CoreExprKind::Index{object,index}=>{projections.push(intrinsics::AddressProjection::Index(index.id));object},
                CoreExprKind::Slice{object,start,end}=>{projections.push(intrinsics::AddressProjection::Slice{start:start.as_ref().map(|value|value.id),end:end.as_ref().map(|value|value.id)});object},
                CoreExprKind::Unary{op:UnaryOp::Dereference,operand}=>operand,
                CoreExprKind::ReceiverMember(member)=>{let ordinal=function.receiver_parameter.ok_or_else(||bad("borrowed receiver field lacks its exact original receiver parameter"))?;b.spend(member.len())?;projections.push(intrinsics::AddressProjection::Field(member.clone()));return Ok((intrinsics::AddressRoot::Parameter(ordinal),projections));},
                _ if self.value_places.contains_key(&(definition,expression.id))=>return Ok((intrinsics::AddressRoot::Place(expression.id),projections)),
                _=>return Err(unsupported("indirect address has no complete original Place/address-parent projection",Some(expression.span))),
            };
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn place(&mut self,module:&TypedModule,context:&CheckedContext,expression:&SemanticOriginalExpr,env:&Env,bindings:&BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<(Ty,Option<Place>)>{
        let token=active.fresh(b)?;active.with_query(DependencyKey{kind:QueryKind::Type,token},b,|active,b|{
            self.remember_names(env,SemanticOriginalNode::Expression(expression.id),bindings,b)?;
            let result=match &expression.kind{
                CoreExprKind::Name(name)=>match bindings.get(name){Some(binding)=>{self.value_bindings.insert((env.current()?,expression.id),binding.identity.clone());(binding.ty.clone(),Some(Place{binder:binding.identity.clone(),fields:vec![]}))},None=>match self.bound_name(module,context,name,env,active,b)?{Arg::Type(ty)=>(ty,None),Arg::Const(value)=>(*value.carrier,None),Arg::Life(_)=>return Err(bad("original value place selects a lifetime name"))}},
                CoreExprKind::ReceiverMember(member)=>{
                    let definition=context.definition(module,env.current()?,b)?;let SemanticDefinitionBody::FunctionWithBody{receiver_parameter:Some(ordinal),..}=&definition.body else{return Err(bad("original receiver shorthand has no true source receiver parameter"));};
                    let owner=env.current()?;let receiver=bindings.values().find(|row|matches!(row.identity,Binder::Parameter{definition,ordinal:actual} if definition==owner&&actual==*ordinal)).ok_or_else(||bad("original receiver parameter is absent"))?;
                    self.read_original(module,context,&Place{binder:receiver.identity.clone(),fields:vec![]},&receiver.ty,env,false,active,b)?;
                    let(storage,observed)=self.field_observation_storage(module,context,&receiver.ty,b)?;
                    if !observed{return Err(bad("original receiver shorthand lacks an actual resource or borrowed receiver"));}
                    let field=self.field(module,context,&storage,member,env,active,b)?;
                    self.observe_projection_parent(expression.id,&receiver.ty,expression.id,env,b)?;
                    (field,None)
                },
                CoreExprKind::Group(inner)=>{let result=self.place(module,context,inner,env,bindings,active,b)?;self.observe_projection_parent(inner.id,&result.0,expression.id,env,b)?;result},
                CoreExprKind::Unary{op:UnaryOp::Dereference,operand}=>{
                    let (pointer,place)=self.place(module,context,operand,env,bindings,active,b)?;if let Some(place)=place{self.read_original(module,context,&place,&pointer,env,false,active,b)?;}
                    self.observe_projection_parent(operand.id,&pointer,expression.id,env,b)?;
                    match self.resolved(&pointer,b)?{Ty::Address{inner,..}=>(*inner,None),_=>return Err(bad("original dereference requires a real Address"))}
                },
                CoreExprKind::Member{object:source_object,member,owner}=>{
                    let (object,mut place)=self.place(module,context,source_object,env,bindings,active,b)?;
                    let(storage,observed)=self.field_observation_storage(module,context,&object,b)?;
                    if observed{if let Some(place)=&place{self.read_original(module,context,place,&object,env,false,active,b)?;}self.observed_argument(source_object,&object,env,b)?;place=None;}
                    else if let Some(place)=&place{self.read_original_parent(module,context,place,&object,env,active,b)?;}
                    if let Some(owner)=owner{let expected=self.ty(module,context,owner,env,active,b)?;self.unify(&storage,&expected,b)?;}
                    let ty=self.field(module,context,&storage,member,env,active,b)?;
                    self.observe_projection_parent(source_object.id,&object,expression.id,env,b)?;
                    if let Some(place)=&mut place{place.fields.push(member.clone());}(ty,place)
                },
                CoreExprKind::Slice{object,start,end}=>{
                    let(parent,place)=self.place(module,context,object,env,bindings,active,b)?;
                    if let Some(place)=&place{self.read_original(module,context,place,&parent,env,false,active,b)?;}
                    let storage=match self.resolved(&parent,b)?{Ty::Address{inner,..}=>self.resolved(&inner,b)?,ty=>ty};
                    let element=match storage{Ty::Array(element,_)|Ty::Slice(element)=>element,_=>return Err(bad("original slice projection has no complete Array/Slice parent"))};
                    for bound in [start.as_deref(),end.as_deref()].into_iter().flatten(){let ty=self.expression(module,context,bound,env,bindings,None,active,b)?;if !matches!(self.resolved(&ty,b)?,Ty::Atom(ref ty)if ty.is_integer()){return Err(bad("original slice endpoint lacks an actual integer carrier"));}}
                    // The projection observes the original whole parent root;
                    // range checks and the actual slice loan remain separate.
                    self.observe_projection_parent(object.id,&parent,expression.id,env,b)?;
                    (Ty::Slice(element),place)
                },
                CoreExprKind::Index{object:source_object,index}=>{
                    let (object,mut place)=self.place(module,context,source_object,env,bindings,active,b)?;let mut storage=self.resolved(&object,b)?;
                    if let Some(place)=&place{self.read_original_parent(module,context,place,&object,env,active,b)?;place.binder.charge_clone(b)?;b.spend(place.fields.iter().fold(1usize,|n,field|n.saturating_add(field.len()).saturating_add(1)))?;}
                    let parent=place.clone();
                    if let Ty::Address{inner,..}=storage{storage=*inner;place=None;}
                    // Runtime indices accept every actual integer carrier.
                    // An existing u32/i16 index is not implicitly a usize, and
                    // an unsuffixed value keeps normal original inference.
                    let index_ty=self.expression(module,context,index,env,bindings,None,active,b)?;
                    if !matches!(self.resolved(&index_ty,b)?,Ty::Atom(ref ty)if ty.is_integer()){return Err(bad("original index is not integer"));}
                    // An index expression may execute a call. Recheck the
                    // already selected parent after it, without replaying it.
                    if let Some(parent)=&parent{self.read_original_parent(module,context,parent,&object,env,active,b)?;}
                    let child=match storage{Ty::Array(child,_)|Ty::Slice(child)=>*child,_=>return Err(bad("original indexed place is not an Array/Slice"))};
                    if let Some(whole)=&mut place{if let CoreExprKind::Integer(text)=&index.kind{let Ty::Atom(carrier)=self.resolved(&index_ty,b)? else{return Err(bad("original constant index has no integer carrier"));};b.spend(text.len())?;let value=scalar::closed(&DynConstPattern::IntegerLiteral(text.clone()),&carrier,module.target.pointer_width,b)?.ok_or_else(||bad("original constant index unresolved"))?;let value=nonnegative_index(&value);if let Some(value)=value{b.spend(24)?;whole.fields.push(format!("[{value}]"));}else{place=None;}}else{place=None;}}
                    if place.is_none(){if let Some(parent)=&parent{self.read_original(module,context,parent,&object,env,false,active,b)?;}}
                    self.observe_projection_parent(source_object.id,&object,expression.id,env,b)?;
                    (child,place)
                },
                _=>(self.expression(module,context,expression,env,bindings,None,active,b)?,None),
            };
            if let Some(place)=&result.1{place.binder.charge_clone(b)?;b.spend(place.fields.iter().fold(2usize,|n,field|n.saturating_add(field.len()).saturating_add(1)))?;self.value_places.insert((env.current()?,expression.id),place.clone());}
            let place_type=self.resolved(&result.0,b)?;
            b.spend(self.place_types.len().checked_ilog2().map_or(1,|n|n as usize+1))?;
            self.place_types.insert((env.current()?,expression.id),place_type);
            self.expressions.insert((env.current()?,expression.id),result.0.clone());Ok(result)
        })
    }
    pub(super) fn take_place(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,place:Option<&Place>,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        if let Some(place)=place{return self.read_original(module,context,place,ty,env,true,active,b);}
        if self.is_copy(module,context,ty,env,active,b)?{return Ok(());}
        Err(unsupported("original read cannot move a non-Copy value out of a borrowed/unknown element",None))
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn argument(&mut self,module:&TypedModule,context:&CheckedContext,expression:&SemanticOriginalExpr,expected:&Ty,mode:PassingMode,env:&Env,bindings:&BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty>{
        if let Ty::Desc{life:expected_life,inner:payload}=self.resolved(expected,b)?{if matches!(mode,PassingMode::Infer|PassingMode::Describe){
            let(actual,place)=self.place(module,context,expression,env,bindings,active,b)?;if let Some(place)=place{self.read_original(module,context,&place,&actual,env,false,active,b)?;}
            let effective=match self.resolved(&actual,b)?{Ty::Managed(inner)=>{self.unify(&inner,&payload,b)?;let life=Life::Local(env.current()?,expression.id);self.relate_outer(&life,&expected_life,b)?;Ty::Desc{life,inner}},Ty::Desc{life,inner}=>{self.unify(&inner,&payload,b)?;self.relate_outer(&life,&expected_life,b)?;Ty::Desc{life,inner}},_=>return Err(bad("original description argument has no actual owner or description"))};
            // Only this actual argument occurrence is a description. The
            // lexical binding and parameter declaration retain their owning
            // category, source identity and complete intrinsic domain.
            b.spend(1)?;self.expressions.insert((env.current()?,expression.id),effective.clone());
            return Ok(effective);
        }}
        let actual=match mode{
            PassingMode::Borrow|PassingMode::BorrowMut|PassingMode::Describe=>{
                let (source,place)=self.place(module,context,expression,env,bindings,active,b)?;if let Some(place)=place{self.read_original(module,context,&place,&source,env,false,active,b)?;}let actual=self.contextual_borrow_value(&source,expected,b)?;self.relate_value(&actual,expected,b)?;Ok(actual)
            },
            PassingMode::Copy=>{
                let (source,place)=self.place(module,context,expression,env,bindings,active,b)?;
                let actual=self.contextual_borrow_value(&source,expected,b)?;
                self.require_copy(module,context,&actual,env,active,b)?;
                if let Some(place)=place{self.read_original(module,context,&place,&source,env,false,active,b)?;}self.relate_value(&actual,expected,b)?;Ok(actual)
            },
            PassingMode::Give=>{let actual=self.expression(module,context,expression,env,bindings,Some(expected),active,b)?;if !self.transfer_operand(env.current()?,expression,b)?{return Err(bad("original owning resource argument omits explicit give"));}Ok(actual)},
            _=>self.expression(module,context,expression,env,bindings,Some(expected),active,b),
        }?;
        self.require_input_address_wf(module,context,&actual,expected,active,b)?;
        Ok(actual)
    }
}

impl CheckedOriginal {
    /// Closed observation schemes use the same argument-category adaptation
    /// as ordinary Describe parameters. Exclusive owner addresses stay intact.
    pub(super) fn observed_argument(&mut self,expression:&SemanticOriginalExpr,value:&Ty,env:&Env,b:&mut Budget)->Check<Ty>{
        let effective=match self.resolved(value,b)?{
            Ty::Managed(inner)=>Ty::Desc{life:Life::Local(env.current()?,expression.id),inner},
            value=>value,
        };
        b.spend(1)?;self.expressions.insert((env.current()?,expression.id),effective.clone());
        Ok(effective)
    }
}

impl CheckedOriginal {
    /// Resource fields are observed through the actual owner, just as through
    /// an explicit description. The returned storage is a nominal selector;
    /// it grants no field move, new manager, copy qualification or access mode.
    fn field_observation_storage(&self,module:&TypedModule,context:&CheckedContext,ty:&Ty,b:&mut Budget)->Check<(Ty,bool)>{
        let mut storage=self.resolved(ty,b)?;let mut observed=false;
        loop{b.spend(1)?;match storage{
            Ty::Address{inner,..}|Ty::Desc{inner,..}=>{storage=self.resolved(&inner,b)?;observed=true;},
            Ty::Managed(inner)=>{
                let payload=self.resolved(&inner,b)?;
                let Ty::Nominal(id,_) = &payload else{return Err(bad("original managed field receiver has no nominal resource declaration"));};
                if !matches!(context.definition(module,*id,b)?.body,SemanticDefinitionBody::Nominal{kind:LifeNominalKind::Resource,..}){return Err(bad("original managed field receiver changes its canonical resource category"));}
                storage=payload;observed=true;
            },
            _=>return Ok((storage,observed)),
        }}
    }
}

impl CheckedOriginal {
    /// Observing an owned field through an existing borrowed product retains
    /// that exact observer's lifetime. This records no movable Place, owner,
    /// permission or equality, and does not change the selected child's type.
    fn observe_projection_parent(&mut self,source:NodeId,source_type:&Ty,projection:NodeId,env:&Env,b:&mut Budget)->Check<()> {
        let owner=env.current()?;
        b.spend(self.borrow_parents.len().checked_ilog2().map_or(1,|n|n as usize+2))?;
        let parent=match self.resolved(source_type,b)? {
            Ty::Address{life,..}|Ty::Desc{life,..}=>Some(life),
            Ty::Managed(_)if source!=projection=>Some(Life::Local(owner,source)),
            _=>self.borrow_parents.get(&(owner,source)).cloned(),
        };
        let Some(parent)=parent else{return Ok(());};
        if parent==Life::Local(owner,projection){return Ok(());}
        if let Some(previous)=self.borrow_parents.get(&(owner,projection)){
            if !self.same_life(previous,&parent,b)?{return Err(bad("original storage projection has conflicting observer source roles"));}
        }else{b.spend(1)?;self.borrow_parents.insert((owner,projection),parent);}
        Ok(())
    }
}

#[cfg(test)]
mod projection_source_tests {
    use super::*;
    #[test]
    fn observed_owner_chain_preserves_the_exact_rigid_source_and_local_owner_boundary(){
        let owner=SemanticDefinitionId{module:zil_base::ModuleId(0),index:0};
        let a=Life::Formal(owner,0);let z=Life::Formal(owner,1);
        let env=Env{lexical:vec![Frame{definition:owner,actuals:vec![Arg::Life(a.clone()),Arg::Life(z.clone())],environment:vec![]}],bounds:vec![],invocation:vec![],self_type:None};
        let owned=Ty::Managed(Box::new(Ty::Store(Box::new(Ty::Variable(owner,2)))));
        let product=Ty::Tuple(vec![owned.clone()]);
        let address=|life|Ty::Address{mutable:false,life,inner:Box::new(product.clone())};
        let mut checked=CheckedOriginal::default();checked.current=Some(owner);let mut b=Budget::new(&Limits::default());
        checked.observe_projection_parent(NodeId(0),&address(a.clone()),NodeId(1),&env,&mut b).unwrap();
        checked.observe_projection_parent(NodeId(1),&product,NodeId(2),&env,&mut b).unwrap();
        checked.observe_projection_parent(NodeId(2),&owned,NodeId(3),&env,&mut b).unwrap();
        checked.observe_projection_parent(NodeId(3),&owned,NodeId(4),&env,&mut b).unwrap();
        assert!(checked.outer_outlives(&Life::Local(owner,NodeId(4)),&a,&mut BTreeSet::new(),&mut b).unwrap());
        assert!(!checked.outer_outlives(&Life::Local(owner,NodeId(4)),&z,&mut BTreeSet::new(),&mut b).unwrap());
        checked.observe_projection_parent(NodeId(10),&owned,NodeId(11),&env,&mut b).unwrap();
        assert!(!checked.outer_outlives(&Life::Local(owner,NodeId(11)),&a,&mut BTreeSet::new(),&mut b).unwrap(),"a local manager cannot acquire an unrelated rigid output lifetime");
        assert!(checked.observe_projection_parent(NodeId(5),&address(z.clone()),NodeId(2),&env,&mut b).is_err(),"one original projection cannot silently replace its observer source");
        assert_eq!(checked.borrow_parents[&(owner,NodeId(2))],a);
        assert!(!checked.same_life(&a,&z,&mut b).unwrap());
    }
}

impl CheckedOriginal {
    /// A contextual shared reborrow consumes a view, while the source
    /// expression and lexical binder retain their actual exclusive category.
    pub(super) fn contextual_borrow_value(&self,actual:&Ty,expected:&Ty,b:&mut Budget)->Check<Ty>{
        let actual=self.resolved(actual,b)?;
        if matches!(self.resolved(expected,b)?,Ty::Address{mutable:false,..}){if let Ty::Address{mutable:true,life,inner}=actual{return Ok(Ty::Address{mutable:false,life,inner});}}
        Ok(actual)
    }
}

#[cfg(test)]
mod shared_reborrow_tests {
    use super::*;
    #[test]
    fn shared_reborrow_changes_only_the_outer_view_and_never_upgrades_or_retags_the_child(){
        let owner=SemanticDefinitionId{module:zil_base::ModuleId(0),index:0};let a=Life::Formal(owner,0);let z=Life::Formal(owner,1);
        let inner=Ty::Nominal(owner,vec![Arg::Life(a.clone()),Arg::Life(z.clone())]);
        let address=|mutable,inner|Ty::Address{mutable,life:a.clone(),inner:Box::new(inner)};
        let exclusive=address(true,inner.clone());let shared=address(false,inner.clone());
        let mut checked=CheckedOriginal::default();checked.current=Some(owner);let mut b=Budget::new(&Limits::default());
        let view=checked.contextual_borrow_value(&exclusive,&shared,&mut b).unwrap();assert_eq!(view,shared);assert_eq!(exclusive,address(true,inner));
        checked.relate_value(&exclusive,&shared,&mut b).unwrap();
        let reverse=checked.contextual_borrow_value(&shared,&exclusive,&mut b).unwrap();assert!(checked.relate_value(&reverse,&exclusive,&mut b).is_err());
        let swapped=address(false,Ty::Nominal(owner,vec![Arg::Life(z.clone()),Arg::Life(a.clone())]));
        assert!(checked.relate_value(&exclusive,&swapped,&mut b).is_err());
        let scalar=Ty::Atom(Type::I32);let nested_exclusive=address(true,address(true,scalar.clone()));let nested_shared=address(false,address(false,scalar));
        assert!(checked.relate_value(&nested_exclusive,&nested_shared,&mut b).is_err(),"root reborrow cannot change a pointee's own address mode");
        assert!(!checked.same_life(&a,&z,&mut b).unwrap());
    }
}
