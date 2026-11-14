//! Original place observations retain binder identity and ordered field paths.
//! They do not create physical addresses or executable borrow permissions.
use super::*;
impl CheckedOriginal {
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
                    let storage=match self.resolved(&receiver.ty,b)?{Ty::Address{inner,..}|Ty::Desc{inner,..}=>*inner,_=>return Err(bad("original receiver shorthand lacks an actual borrowed/description receiver"))};
                    (self.field(module,context,&storage,member,env,active,b)?,None)
                },
                CoreExprKind::Group(inner)=>self.place(module,context,inner,env,bindings,active,b)?,
                CoreExprKind::Unary{op:UnaryOp::Dereference,operand}=>{
                    let (pointer,place)=self.place(module,context,operand,env,bindings,active,b)?;if let Some(place)=place{self.read_original(module,context,&place,&pointer,env,false,active,b)?;}
                    match self.resolved(&pointer,b)?{Ty::Address{inner,..}=>(*inner,None),_=>return Err(bad("original dereference requires a real Address"))}
                },
                CoreExprKind::Member{object,member,owner}=>{
                    let (object,mut place)=self.place(module,context,object,env,bindings,active,b)?;
                    let mut storage=self.resolved(&object,b)?;
                    if let Ty::Address{inner,..}|Ty::Desc{inner,..}=storage{if let Some(place)=&place{self.read_original(module,context,place,&object,env,false,active,b)?;}storage=*inner;place=None;}
                    if let Some(owner)=owner{let expected=self.ty(module,context,owner,env,active,b)?;self.unify(&storage,&expected,b)?;}
                    let ty=self.field(module,context,&storage,member,env,active,b)?;if let Some(place)=&mut place{place.fields.push(member.clone());}(ty,place)
                },
                CoreExprKind::Index{object,index}=>{
                    let (object,mut place)=self.place(module,context,object,env,bindings,active,b)?;let mut storage=self.resolved(&object,b)?;
                    if let Ty::Address{inner,..}=storage{if let Some(place)=&place{self.read_original(module,context,place,&object,env,false,active,b)?;}storage=*inner;place=None;}
                    let index_ty=self.expression(module,context,index,env,bindings,Some(&Ty::Atom(Type::Usize)),active,b)?;
                    if !matches!(self.resolved(&index_ty,b)?,Ty::Atom(ref ty)if ty.is_integer()){return Err(bad("original index is not integer"));}
                    let child=match storage{Ty::Array(child,_)|Ty::Slice(child)=>*child,_=>return Err(bad("original indexed place is not an Array/Slice"))};
                    if let Some(whole)=&mut place{if let CoreExprKind::Integer(text)=&index.kind{let value=scalar::closed(&DynConstPattern::IntegerLiteral(text.clone()),&Type::Usize,module.target.pointer_width,b)?.ok_or_else(||bad("original constant index unresolved"))?;let Constant::Usize(value)=value else{return Err(bad("original index carrier differs"));};whole.fields.push(format!("[{value}]"));}else{place=None;}}
                    (child,place)
                },
                _=>(self.expression(module,context,expression,env,bindings,None,active,b)?,None),
            };
            if let Some(place)=&result.1{place.binder.charge_clone(b)?;b.spend(place.fields.iter().fold(2usize,|n,field|n.saturating_add(field.len()).saturating_add(1)))?;self.value_places.insert((env.current()?,expression.id),place.clone());}
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
            return match self.resolved(&actual,b)?{Ty::Managed(inner)=>{self.unify(&inner,&payload,b)?;let life=Life::Local(env.current()?,expression.id);self.unify_life(&life,&expected_life,b)?;Ok(Ty::Desc{life,inner})},Ty::Desc{life,inner}=>{self.unify(&inner,&payload,b)?;self.unify_life(&life,&expected_life,b)?;Ok(Ty::Desc{life,inner})},_=>Err(bad("original description argument has no actual owner or description"))};
        }}
        match mode{
            PassingMode::Borrow|PassingMode::BorrowMut|PassingMode::Describe=>{
                let (actual,place)=self.place(module,context,expression,env,bindings,active,b)?;if let Some(place)=place{self.read_original(module,context,&place,&actual,env,false,active,b)?;}self.unify(&actual,expected,b)?;Ok(actual)
            },
            PassingMode::Copy=>{
                let (actual,place)=self.place(module,context,expression,env,bindings,active,b)?;
                self.require_copy(module,context,&actual,env,active,b)?;
                if let Some(place)=place{self.read_original(module,context,&place,&actual,env,false,active,b)?;}self.unify(&actual,expected,b)?;Ok(actual)
            },
            PassingMode::Give=>{if !matches!(expression.kind,CoreExprKind::Unary{op:UnaryOp::Give,..}){return Err(bad("original owning resource argument omits explicit give"));}self.expression(module,context,expression,env,bindings,Some(expected),active,b)},
            _=>self.expression(module,context,expression,env,bindings,Some(expected),active,b),
        }
    }
}
