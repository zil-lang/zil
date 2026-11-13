//! Original type variables retain declaration/full-ordinal identity even when
//! every currently materialized instance happens to use the same machine Type.
use super::*;
#[derive(Clone,Debug,PartialEq,Eq)]
pub(super) enum Ty {
    Atom(Type),
    Variable(SemanticDefinitionId,u32),
    SelfType(SemanticDefinitionId),
    Nominal(SemanticDefinitionId,Vec<Arg>),
    Tuple(Vec<Ty>),
    Array(Box<Ty>,Box<Const>),
    Slice(Box<Ty>),
    Address{mutable:bool,life:Life,inner:Box<Ty>},
    Desc{life:Life,inner:Box<Ty>},
    Managed(Box<Ty>),
    Store(Box<Ty>),
    ErrorUnion(Box<Ty>,Box<Ty>),
    Closure(SemanticDefinitionId,Vec<Arg>,Vec<Arg>),
    Function(SemanticDefinitionId),
    Code{kind:SemanticCodeKind,parameters:Vec<Ty>,result:Box<Ty>,modes:Vec<PassingMode>,result_mode:PassingMode,call_kind:CallKind,bound_lifetimes:u32,invocation_lifetimes:u32},
    Infer(u64),
}
#[derive(Clone,Debug,PartialEq,Eq)]
pub(super) enum Life {Formal(SemanticDefinitionId,u32),Implicit(SemanticDefinitionId,u32),Bound(FrameToken,u32),Invocation(FrameToken,u32),Local(SemanticDefinitionId,NodeId),Static}
#[derive(Clone,Debug,PartialEq,Eq)]
pub(super) enum Arg {Type(Ty),Const(Const),Life(Life)}
#[derive(Clone,Debug,PartialEq,Eq)]
pub(super) struct Const {pub(super) carrier:Box<Ty>,pub(super) value:ConstValue}
#[derive(Clone,Debug,PartialEq,Eq)]
pub(super) enum ConstValue {Variable(SemanticDefinitionId,u32),Value(Constant),Unary(UnaryOp,Box<Const>),Binary(BinaryOp,Box<Const>,Box<Const>),Convert(Box<Const>)}
#[derive(Clone)]
pub(super) struct Frame {pub(super) definition:SemanticDefinitionId,pub(super) actuals:Vec<Arg>,pub(super) environment:Vec<Arg>}
#[derive(Clone)]
pub(super) struct Env {pub(super) lexical:Vec<Frame>,pub(super) bounds:Vec<Vec<Life>>,pub(super) invocation:Vec<Life>}
impl Env {
    pub(super) fn current(&self)->Check<SemanticDefinitionId>{self.lexical.first().map(|frame|frame.definition).ok_or_else(||bad("original body has no lexical definition"))}
    pub(super) fn frame(&self,scope:SemanticScopeRef)->Check<&Frame>{let SemanticScopeRef::Ancestor(depth)=scope;self.lexical.get(depth as usize).ok_or_else(||bad("original recipe escapes its real lexical parent"))}
}

#[derive(Clone,Debug,PartialEq,Eq)]
pub(super) struct Binding {pub(super) ty:Ty,pub(super) mutable:bool}
