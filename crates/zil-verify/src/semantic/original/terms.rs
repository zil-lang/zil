//! Original type variables retain declaration/full-ordinal identity even when
//! every currently materialized instance happens to use the same machine Type.
use super::*;
#[derive(Clone,Debug,PartialEq,Eq)]
pub(super) enum Ty {
    Atom(Type),
    SealedError(SealedError),
    Variable(SemanticDefinitionId,u32),
    SelfType(SemanticDefinitionId),
    Nominal(SemanticDefinitionId,Vec<Arg>),
    Opaque(SemanticDefinitionId,Vec<Arg>),
    Projection{receiver:Box<Ty>,declaration:LifeDeclarationId,arguments:Vec<Arg>,member:u32},
    Tuple(Vec<Ty>),
    Array(Box<Ty>,Box<Const>),
    Slice(Box<Ty>),
    Address{mutable:bool,life:Life,inner:Box<Ty>},
    Desc{life:Life,inner:Box<Ty>},
    Managed(Box<Ty>),
    Store(Box<Ty>),
    Carrier{kind:CarrierKind,child:Box<Ty>,retained:Box<Retained>},
    ErrorUnion(Box<Ty>,Box<Ty>),
    Closure(SemanticDefinitionId,Vec<Arg>,Vec<Arg>),
    Function(SemanticDefinitionId),
    Code{kind:SemanticCodeKind,parameters:Vec<Ty>,result:Box<Ty>,modes:Vec<PassingMode>,result_mode:PassingMode,call_kind:CallKind,bound_lifetimes:u32,invocation_lifetimes:u32},
    Infer(u64),
}
#[derive(Clone,Debug,PartialEq,Eq,PartialOrd,Ord)]
pub(super) enum Life {Formal(SemanticDefinitionId,u32),Implicit(SemanticDefinitionId,u32),Bound(FrameToken,u32),Invocation(FrameToken,u32),Infer(u64),Head(SemanticDefinitionId,u32),Local(SemanticDefinitionId,NodeId),ReceiverRole(SemanticDefinitionId,NodeId,StoredBorrowMode),Static}
#[derive(Clone,Debug,PartialEq,Eq)]
pub(super) enum Arg {Type(Ty),Const(Const),Life(Life)}
#[derive(Clone,Debug,PartialEq,Eq)]
pub(super) struct Const {pub(super) carrier:Box<Ty>,pub(super) value:ConstValue}
#[derive(Clone,Debug,PartialEq,Eq)]
pub(super) enum ConstValue {Variable(SemanticDefinitionId,u32),Value(Constant),Unary(UnaryOp,Box<Const>),Binary(BinaryOp,Box<Const>,Box<Const>),Convert(Box<Const>)}
#[derive(Clone)]
pub(super) struct Frame {pub(super) definition:SemanticDefinitionId,pub(super) actuals:Vec<Arg>,pub(super) environment:Vec<Arg>}
#[derive(Clone)]
pub(super) struct Env {pub(super) lexical:Vec<Frame>,pub(super) bounds:Vec<Vec<Life>>,pub(super) invocation:Vec<Life>,pub(super) self_type:Option<(SemanticDefinitionId,Ty)>}
impl Env {
    pub(super) fn current(&self)->Check<SemanticDefinitionId>{self.lexical.first().map(|frame|frame.definition).ok_or_else(||bad("original body has no lexical definition"))}
    pub(super) fn frame(&self,scope:SemanticScopeRef)->Check<&Frame>{let SemanticScopeRef::Ancestor(depth)=scope;self.lexical.get(depth as usize).ok_or_else(||bad("original recipe escapes its real lexical parent"))}
}

#[derive(Clone,Debug,PartialEq,Eq)]
pub(super) struct Binding {pub(super) ty:Ty,pub(super) mutable:bool,pub(super) identity:Binder}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) enum CarrierKind {Mutex,MutexGuard,Registration,AsyncRegistration,Task}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(super) enum SealedError {MutexCreate,TaskCreate,TaskPoll,DynCreate}
#[derive(Clone,Debug,PartialEq,Eq,Default)]
pub(super) struct Retained {pub(super) domain:Vec<ClassPart>,pub(super) ports:Vec<PortPart>,pub(super) retained:Vec<RetainedPart>,pub(super) nominal:Vec<NominalPart>}
#[derive(Clone,Debug,PartialEq,Eq)]
pub(super) enum ClassPart {One(Life),All{ty:Ty,path:Vec<LifeTypeProjection>}}
#[derive(Clone,Debug,PartialEq,Eq)]
pub(super) enum PortPart {One{leaf:Ty,mode:StoredBorrowMode,class:Life},FromType{ty:Ty,classes:Vec<ClassPart>}}
#[derive(Clone,Debug,PartialEq,Eq)]
pub(super) enum RetainedPart {One{path:Vec<LifeTypeProjection>,child_class:u32,parent:Life},All{path:Vec<LifeTypeProjection>,parents:Vec<ClassPart>}}
#[derive(Clone,Debug,PartialEq,Eq)]
pub(super) enum NominalPart {One{path:Vec<LifeTypeProjection>,declaration:LifeDeclarationId,argument:u32,class:Life},FromType{path:Vec<LifeTypeProjection>,ty:Ty,classes:Vec<ClassPart>}}
