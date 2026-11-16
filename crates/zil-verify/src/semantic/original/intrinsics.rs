//! Closed language schemes have no fabricated user definition. Lexical values
//! and real declaration bindings take precedence; spelling is considered only
//! for the explicitly reserved bare entry points after shadow checks.
use super::*;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(in crate::semantic) enum Intrinsic {SliceLen,OwnedDyn,Convert(Numeric),CCallback,TaskYield,BytesNew,BytesReserve,BytesPush,BytesTruncate,BytesLen,BytesCapacity,BytesView,BytesViewMut,StoreNew,StoreReserve,StorePush,StorePop,StoreGet,StoreGetMut,StoreLen,StoreCapacity,TaskCreate,TaskCreateOwned,TaskValid,TaskPoll,TaskRequest,TaskPollClose,TaskFinish,MutexCreate,MutexLock,MutexValue,MutexUnlock,AsyncStart,AsyncValid,AsyncRequest,AsyncPoll,AsyncFinish,AsyncFinishChecked,ThreadScope}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(in crate::semantic) enum Numeric {I8,I16,I32,I64,Isize,U8,U16,U32,U64,Usize,F32,F64}
impl Numeric {
    pub(in crate::semantic) fn ty(self)->Type{match self{Self::I8=>Type::I8,Self::I16=>Type::I16,Self::I32=>Type::I32,Self::I64=>Type::I64,Self::Isize=>Type::Isize,Self::U8=>Type::U8,Self::U16=>Type::U16,Self::U32=>Type::U32,Self::U64=>Type::U64,Self::Usize=>Type::Usize,Self::F32=>Type::F32,Self::F64=>Type::F64}}
    fn name(name:&str)->Option<Self>{Some(match name{"i8"=>Self::I8,"i16"=>Self::I16,"i32"=>Self::I32,"i64"=>Self::I64,"isize"=>Self::Isize,"u8"=>Self::U8,"u16"=>Self::U16,"u32"=>Self::U32,"u64"=>Self::U64,"usize"=>Self::Usize,"f32"=>Self::F32,"f64"=>Self::F64,_=>return None})}
    fn from_type(ty:&Type)->Option<Self>{[Self::I8,Self::I16,Self::I32,Self::I64,Self::Isize,Self::U8,Self::U16,Self::U32,Self::U64,Self::Usize,Self::F32,Self::F64].into_iter().find(|value|value.ty()==*ty)}
}
#[derive(Clone,Copy,PartialEq,Eq)]
pub(in crate::semantic) enum BufferSource {Local{definition:SemanticDefinitionId,node:NodeId},Parameter{definition:SemanticDefinitionId,ordinal:u32},Place{definition:SemanticDefinitionId,node:NodeId},Address{definition:SemanticDefinitionId,node:NodeId}}
#[derive(Clone,Copy)]
pub(in crate::semantic) enum BufferRoot {Local{node:NodeId},Parameter{ordinal:u32},Capture,Pattern}
#[derive(Clone,Copy)]
pub(in crate::semantic) enum AddressRoot {Place(NodeId),Value(NodeId),Parameter(u32)}
pub(in crate::semantic) enum AddressProjection {Field(String),Index(NodeId),Slice{start:Option<NodeId>,end:Option<NodeId>}}
#[derive(Clone,Copy)]
pub(in crate::semantic) struct Use {pub(in crate::semantic) kind:Intrinsic,pub(in crate::semantic) buffer:Option<BufferSource>,pub(in crate::semantic) worker:Option<SemanticDefinitionId>,pub(in crate::semantic) providers:Option<[SemanticDefinitionId;3]>}
impl Intrinsic {
    pub(in crate::semantic) fn actual(kind:&InstructionKind)->Option<Self>{Some(match kind{
        InstructionKind::ConvertInteger{target,..}|InstructionKind::ConvertNumber{target,..}=>Self::Convert(Numeric::from_type(target)?),
        InstructionKind::ForeignFunctionRef{..}=>Self::CCallback,
        InstructionKind::SliceLength{..}=>Self::SliceLen,
        InstructionKind::MakeDynOwned{..}=>Self::OwnedDyn,
        InstructionKind::TaskYield=>Self::TaskYield,
        InstructionKind::ByteBufferNew=>Self::BytesNew,
        InstructionKind::ByteBufferReserve{..}=>Self::BytesReserve,
        InstructionKind::ByteBufferPush{..}=>Self::BytesPush,
        InstructionKind::ByteBufferTruncate{..}=>Self::BytesTruncate,
        InstructionKind::ByteBufferLen{..}=>Self::BytesLen,
        InstructionKind::ByteBufferCapacity{..}=>Self::BytesCapacity,
        InstructionKind::ByteBufferBorrow{mutable:false,..}=>Self::BytesView,InstructionKind::ByteBufferBorrow{mutable:true,..}=>Self::BytesViewMut,
        InstructionKind::OwnedStoreNew{..}=>Self::StoreNew,InstructionKind::OwnedStoreReserve{..}=>Self::StoreReserve,InstructionKind::OwnedStorePush{..}=>Self::StorePush,InstructionKind::OwnedStorePop{..}=>Self::StorePop,InstructionKind::OwnedStoreBorrow{..}=>Self::StoreGet,InstructionKind::OwnedStoreBorrowMut{..}=>Self::StoreGetMut,InstructionKind::OwnedStoreLen{..}=>Self::StoreLen,InstructionKind::OwnedStoreCapacity{..}=>Self::StoreCapacity,
        InstructionKind::TaskCreate{..}=>Self::TaskCreate,InstructionKind::TaskCreateOwned{..}=>Self::TaskCreateOwned,InstructionKind::TaskValid{..}=>Self::TaskValid,InstructionKind::TaskPoll{..}=>Self::TaskPoll,InstructionKind::TaskRequestCancel{..}=>Self::TaskRequest,InstructionKind::TaskPollClose{..}=>Self::TaskPollClose,InstructionKind::TaskFinishDestroy{..}=>Self::TaskFinish,
        InstructionKind::MutexCreate{..}=>Self::MutexCreate,InstructionKind::MutexLock{..}=>Self::MutexLock,InstructionKind::MutexValue{..}=>Self::MutexValue,InstructionKind::MutexUnlock{..}=>Self::MutexUnlock,
        InstructionKind::AsyncRegistrationStart{..}=>Self::AsyncStart,InstructionKind::AsyncRegistrationValid{..}=>Self::AsyncValid,InstructionKind::AsyncRegistrationRequestClose{..}=>Self::AsyncRequest,InstructionKind::AsyncRegistrationPollClose{..}=>Self::AsyncPoll,InstructionKind::AsyncRegistrationFinishDestroy{..}=>Self::AsyncFinish,InstructionKind::AsyncRegistrationFinishDestroyChecked{..}=>Self::AsyncFinishChecked,InstructionKind::WithScopedThread{..}=>Self::ThreadScope,
        _=>return None,
    })}
    fn name(name:&str)->Option<Self>{if let Some(target)=Numeric::name(name){return Some(Self::Convert(target));}Some(match name{"owned_dyn"=>Self::OwnedDyn,"c_callback"=>Self::CCallback,"task_yield"=>Self::TaskYield,"bytes_new"=>Self::BytesNew,"bytes_reserve"=>Self::BytesReserve,"bytes_push"=>Self::BytesPush,"bytes_truncate"=>Self::BytesTruncate,"bytes_len"=>Self::BytesLen,"bytes_capacity"=>Self::BytesCapacity,"bytes_view"=>Self::BytesView,"bytes_view_mut"=>Self::BytesViewMut,
        "store_new"=>Self::StoreNew,"store_reserve"=>Self::StoreReserve,"store_push"=>Self::StorePush,"store_pop"=>Self::StorePop,"store_at"=>Self::StoreGet,"store_at_mut"=>Self::StoreGetMut,"store_len"=>Self::StoreLen,"store_capacity"=>Self::StoreCapacity,
        "task_create"=>Self::TaskCreate,"task_create_owned"=>Self::TaskCreateOwned,"task_valid"=>Self::TaskValid,"task_poll"=>Self::TaskPoll,"task_request_cancel"=>Self::TaskRequest,"task_poll_close"=>Self::TaskPollClose,"task_finish_destroy"=>Self::TaskFinish,
        "mutex_create"=>Self::MutexCreate,"mutex_lock"=>Self::MutexLock,"mutex_value"=>Self::MutexValue,"mutex_unlock"=>Self::MutexUnlock,
        "c_register_async"=>Self::AsyncStart,"async_registration_valid"=>Self::AsyncValid,"async_registration_request_close"=>Self::AsyncRequest,"async_registration_poll_close"=>Self::AsyncPoll,"async_registration_finish_destroy"=>Self::AsyncFinish,"async_registration_finish_destroy_checked"=>Self::AsyncFinishChecked,"thread_scope"=>Self::ThreadScope,_=>return None})}
}
impl CheckedOriginal {
    pub(super) fn intrinsic_name(&self,module:&TypedModule,context:&CheckedContext,base:&SemanticOriginalExpr,env:&Env,bindings:&BTreeMap<String,Binding>,b:&mut Budget)->Check<Option<Intrinsic>> {
        let CoreExprKind::Name(name)=&base.kind else{return Ok(None);};
        let Some(intrinsic)=Intrinsic::name(name) else{return Ok(None);};
        if bindings.contains_key(name){return Ok(None);}
        let caller=context.definition(module,env.current()?,b)?;
        let SemanticDefinitionBody::FunctionWithBody{names,..}=&caller.body else{return Err(bad("intrinsic lookup has no original lexical body"));};
        b.spend(names.len().saturating_mul(name.len().saturating_add(1)))?;
        if names.iter().any(|row|row.path.first()==Some(name)&&matches!(row.namespace,SemanticOriginalNamespace::Value|SemanticOriginalNamespace::Type)){return Ok(None);}
        let program=module.semantic.as_ref().ok_or_else(||bad("intrinsic lookup has no complete original declarations"))?;
        // Deleting a real Names binding must not turn that declaration into a
        // builtin. The original module and each actual lexical scope remain
        // shadows even if the proposed name table is incomplete.
        for definition in &program.definitions {
            b.spend(definition.identity.declaration_path.len().saturating_add(env.lexical.len()).saturating_add(name.len()).saturating_add(1))?;
            if definition.id.module==caller.id.module&&definition.identity.declaration_path.last()==Some(name)&&(definition.parent.is_none()||env.lexical.iter().any(|frame|Some(frame.definition)==definition.parent)){return Err(bad("reserved intrinsic name has an unresolved real declaration shadow"));}
        }
        Ok(Some(intrinsic))
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn intrinsic_call(&mut self,module:&TypedModule,context:&CheckedContext,intrinsic:Intrinsic,site:SemanticSiteId,written:&[Arg],expected:Option<&Ty>,arguments:&[SemanticOriginalExpr],env:&Env,bindings:&BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty> {
        if intrinsic==Intrinsic::OwnedDyn{return self.owned_dyn(module,context,site,written,arguments,env,bindings,active,b);}
        if intrinsic==Intrinsic::CCallback{return self.c_callback(module,context,site,written,arguments,env,bindings,active,b);}
        if let Some(ty)=self.operation(module,context,intrinsic,site,written,expected,arguments,env,bindings,active,b)?{return Ok(ty);}
        if !written.is_empty(){return Err(bad("closed nongeneric intrinsic has written mixed arguments"));}
        let arity=match intrinsic{Intrinsic::TaskYield|Intrinsic::BytesNew=>0,Intrinsic::Convert(_)=>1,Intrinsic::BytesLen|Intrinsic::BytesCapacity=>1,_=>2};
        if arguments.len()!=arity{return Err(bad("registered intrinsic value arity differs from its closed scheme"));}
        self.execute_effect(env,if intrinsic==Intrinsic::TaskYield{CallKind::Suspend}else{CallKind::Direct},b)?;
        if let Intrinsic::Convert(target)=intrinsic{let source=self.expression(module,context,&arguments[0],env,bindings,None,active,b)?;if !matches!(self.resolved(&source,b)?,Ty::Atom(ref ty)if ty.is_integer()||ty.is_float()){return Err(bad("original numeric conversion lacks a complete numeric source carrier"));}self.intrinsic_calls.insert((env.current()?,site),Use{kind:intrinsic,buffer:None,worker:None,providers:None});return Ok(Ty::Atom(target.ty()));}
        if intrinsic==Intrinsic::TaskYield{self.intrinsic_calls.insert((env.current()?,site),Use{kind:intrinsic,buffer:None,worker:None,providers:None});return Ok(Ty::Atom(Type::Unit));}
        if intrinsic==Intrinsic::BytesNew{self.intrinsic_calls.insert((env.current()?,site),Use{kind:intrinsic,buffer:None,worker:None,providers:None});return Ok(Ty::Managed(Box::new(Ty::Atom(Type::ByteBuffer))));}
        let (buffer,place)=self.place(module,context,&arguments[0],env,bindings,active,b)?;
        if !matches!(self.resolved(&buffer,b)?,Ty::Managed(ref inner)|Ty::Desc{ref inner,..} if **inner==Ty::Atom(Type::ByteBuffer)){return Err(bad("registered byte operation requires its actual owner or description"));}
        let source=match &place{
            Some(Place{binder:Binder::Local{definition,node,..},fields}) if fields.is_empty()=>BufferSource::Local{definition:*definition,node:*node},
            Some(Place{binder:Binder::Parameter{definition,ordinal},fields}) if fields.is_empty()=>BufferSource::Parameter{definition:*definition,ordinal:*ordinal},
            Some(_)=>BufferSource::Place{definition:env.current()?,node:arguments[0].id},
            None=>return Err(unsupported("byte intrinsic needs its complete original projected/captured buffer identity",None)),
        };
        if let Some(place)=place{self.read_original(module,context,&place,&buffer,env,false,active,b)?;}
        self.observed_argument(&arguments[0],&buffer,env,b)?;
        if arity==2{let expected=Ty::Atom(if intrinsic==Intrinsic::BytesPush{Type::U8}else{Type::I64});let value=self.expression(module,context,&arguments[1],env,bindings,Some(&expected),active,b)?;self.unify(&value,&expected,b)?;}
        self.intrinsic_calls.insert((env.current()?,site),Use{kind:intrinsic,buffer:Some(source),worker:None,providers:None});
        Ok(Ty::Atom(if matches!(intrinsic,Intrinsic::BytesLen|Intrinsic::BytesCapacity){Type::I64}else{Type::Bool}))
    }
    pub(in crate::semantic) fn intrinsic(&self,definition:SemanticDefinitionId,site:SemanticSiteId,b:&mut Budget)->Check<Use>{b.spend(1)?;self.intrinsic_calls.get(&(definition,site)).copied().ok_or_else(||bad("actual intrinsic lacks its independently checked original closed scheme"))}
}
impl CheckedOriginal {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn slice_len_call(&mut self,module:&TypedModule,context:&CheckedContext,call:&SemanticOriginalExpr,callee:&SemanticOriginalExpr,written:bool,arguments:&[SemanticOriginalExpr],env:&Env,bindings:&BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<Ty>>{
        let CoreExprKind::Member{object,member,owner:None}=&callee.kind else{return Ok(None);};if member!="len"{return Ok(None);}
        let (receiver,place)=self.place(module,context,object,env,bindings,active,b)?;
        if !matches!(self.resolved(&receiver,b)?,Ty::Address{inner,..}if matches!(*inner,Ty::Slice(_))){return Ok(None);}
        if written||!arguments.is_empty(){return Err(bad("original slice length has written actuals or value arguments"));}
        if let Some(place)=place{self.read_original(module,context,&place,&receiver,env,false,active,b)?;}
        let definition=context.definition(module,env.current()?,b)?;let SemanticDefinitionBody::FunctionWithBody{sites,site_nodes,..}=&definition.body else{return Err(bad("slice length has no complete original body"));};
        b.spend(site_nodes.len().saturating_mul(sites.len().saturating_add(1)))?;let mut uses=site_nodes.iter().filter(|row|row.node==SemanticOriginalNode::Expression(call.id)).filter_map(|row|sites.iter().find(|site|site.id==row.site)).filter(|row|matches!(row.kind,SemanticSiteRecipeKind::OriginalCall));let site=uses.next().ok_or_else(||bad("slice length has no exact original Call occurrence"))?;if uses.next().is_some(){return Err(bad("slice length original Call occurrence is ambiguous"));}
        let mut source=object.as_ref();while let CoreExprKind::Group(inner)|CoreExprKind::Ascribed{value:inner,..}=&source.kind{source=inner;}
        let buffer=if self.value_places.contains_key(&(env.current()?,source.id)){Some(self.operation_source(module,source,env,bindings,b)?)}else{None};
        self.intrinsic_type_sources.insert((env.current()?,site.id,0),source.id);
        self.intrinsic_calls.insert((env.current()?,site.id),Use{kind:Intrinsic::SliceLen,buffer,worker:None,providers:None});
        self.execute_effect(env,CallKind::Direct,b)?;Ok(Some(Ty::Atom(Type::I64)))
    }
    pub(in crate::semantic) fn intrinsic_receiver(&self,definition:SemanticDefinitionId,site:SemanticSiteId,b:&mut Budget)->Check<NodeId>{b.spend(1)?;self.intrinsic_type_sources.get(&(definition,site,0)).copied().ok_or_else(||bad("intrinsic has no independently checked original receiver"))}
}
