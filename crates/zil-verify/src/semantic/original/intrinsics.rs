//! Closed language schemes have no fabricated user definition. Lexical values
//! and real declaration bindings take precedence; spelling is considered only
//! for the explicitly reserved bare entry points after shadow checks.
use super::*;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(in crate::semantic) enum Intrinsic {TaskYield,BytesNew,BytesReserve,BytesPush,BytesTruncate,BytesLen,BytesCapacity}
#[derive(Clone,Copy)]
pub(in crate::semantic) enum BufferSource {Local{definition:SemanticDefinitionId,node:NodeId},Parameter{definition:SemanticDefinitionId,ordinal:u32}}
#[derive(Clone,Copy)]
pub(in crate::semantic) struct Use {pub(in crate::semantic) kind:Intrinsic,pub(in crate::semantic) buffer:Option<BufferSource>}
impl Intrinsic {
    pub(in crate::semantic) fn actual(kind:&InstructionKind)->Option<Self>{Some(match kind{
        InstructionKind::TaskYield=>Self::TaskYield,
        InstructionKind::ByteBufferNew=>Self::BytesNew,
        InstructionKind::ByteBufferReserve{..}=>Self::BytesReserve,
        InstructionKind::ByteBufferPush{..}=>Self::BytesPush,
        InstructionKind::ByteBufferTruncate{..}=>Self::BytesTruncate,
        InstructionKind::ByteBufferLen{..}=>Self::BytesLen,
        InstructionKind::ByteBufferCapacity{..}=>Self::BytesCapacity,
        _=>return None,
    })}
    fn name(name:&str)->Option<Self>{Some(match name{"task_yield"=>Self::TaskYield,"bytes_new"=>Self::BytesNew,"bytes_reserve"=>Self::BytesReserve,"bytes_push"=>Self::BytesPush,"bytes_truncate"=>Self::BytesTruncate,"bytes_len"=>Self::BytesLen,"bytes_capacity"=>Self::BytesCapacity,_=>return None})}
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
    pub(super) fn intrinsic_call(&mut self,module:&TypedModule,context:&CheckedContext,intrinsic:Intrinsic,site:SemanticSiteId,arguments:&[SemanticOriginalExpr],env:&Env,bindings:&BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty> {
        let arity=match intrinsic{Intrinsic::TaskYield|Intrinsic::BytesNew=>0,Intrinsic::BytesLen|Intrinsic::BytesCapacity=>1,_=>2};
        if arguments.len()!=arity{return Err(bad("registered intrinsic value arity differs from its closed scheme"));}
        self.execute_effect(env,if intrinsic==Intrinsic::TaskYield{CallKind::Suspend}else{CallKind::Direct},b)?;
        if intrinsic==Intrinsic::TaskYield{self.intrinsic_calls.insert((env.current()?,site),Use{kind:intrinsic,buffer:None});return Ok(Ty::Atom(Type::Unit));}
        if intrinsic==Intrinsic::BytesNew{self.intrinsic_calls.insert((env.current()?,site),Use{kind:intrinsic,buffer:None});return Ok(Ty::Managed(Box::new(Ty::Atom(Type::ByteBuffer))));}
        let (buffer,place)=self.place(module,context,&arguments[0],env,bindings,active,b)?;
        if !matches!(self.resolved(&buffer,b)?,Ty::Managed(ref inner)|Ty::Desc{ref inner,..} if **inner==Ty::Atom(Type::ByteBuffer)){return Err(bad("registered byte operation requires its actual owner or description"));}
        let source=match &place{
            Some(Place{binder:Binder::Local{definition,node,..},fields}) if fields.is_empty()=>BufferSource::Local{definition:*definition,node:*node},
            Some(Place{binder:Binder::Parameter{definition,ordinal},fields}) if fields.is_empty()=>BufferSource::Parameter{definition:*definition,ordinal:*ordinal},
            _=>return Err(unsupported("byte intrinsic needs its complete original projected/captured buffer identity",None)),
        };
        if let Some(place)=place{self.read_original(module,context,&place,&buffer,env,false,active,b)?;}
        if arity==2{let expected=Ty::Atom(if intrinsic==Intrinsic::BytesPush{Type::U8}else{Type::I64});let value=self.expression(module,context,&arguments[1],env,bindings,Some(&expected),active,b)?;self.unify(&value,&expected,b)?;}
        self.intrinsic_calls.insert((env.current()?,site),Use{kind:intrinsic,buffer:Some(source)});
        Ok(Ty::Atom(if matches!(intrinsic,Intrinsic::BytesLen|Intrinsic::BytesCapacity){Type::I64}else{Type::Bool}))
    }
    pub(in crate::semantic) fn intrinsic(&self,definition:SemanticDefinitionId,site:SemanticSiteId,b:&mut Budget)->Check<Use>{b.spend(1)?;self.intrinsic_calls.get(&(definition,site)).copied().ok_or_else(||bad("actual intrinsic lacks its independently checked original closed scheme"))}
}
