//! Resource syntax denotes a payload. Only a real signature mode chooses an
//! owning carrier or a parameter description; a nominal Record cannot do so.
use super::*;
impl CheckedContext {
    fn resource_recipe(&self,module:&TypedModule,recipe:&SemanticTypeRecipe,environment:&EvalEnv,depth:usize,b:&mut Budget)->Check<bool>{
        b.spend(1)?;if depth>b.limits.max_call_depth.min(128){return Err(limit("resource signature recipe depth exceeded"));}
        Ok(match recipe{
            SemanticTypeRecipe::Atom(Type::ByteBuffer)|SemanticTypeRecipe::OwnedStore(_)|SemanticTypeRecipe::Mutex(_)|SemanticTypeRecipe::Task{..}|SemanticTypeRecipe::Registration(_)|SemanticTypeRecipe::AsyncRegistration(_)=>true,
            SemanticTypeRecipe::Nominal{definition,..}=>matches!(self.definition(module,*definition,b)?.body,SemanticDefinitionBody::Nominal{kind:LifeNominalKind::Resource,..}),
            SemanticTypeRecipe::SelfType{scope}=>{let frame=self.frame(environment,*scope,b)?;let definition=self.definition(module,frame.definition,b)?;match &definition.body{SemanticDefinitionBody::Scope(SemanticScopeRecipe::BuiltinImpl{self_type,..}|SemanticScopeRecipe::InherentImpl{self_type,..}|SemanticScopeRecipe::TraitImpl{self_type,..})=>{let SemanticScopeRef::Ancestor(index)=scope;let mut parent=environment.clone();b.spend(parent.lexical.len())?;parent.lexical=parent.lexical[*index as usize..].to_vec();self.resource_recipe(module,self_type,&parent,depth+1,b)?},_=>false}},
            _=>false,
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn signature_mode_type(&self,module:&TypedModule,recipe:&SemanticTypeRecipe,value:CheckedType,mode:PassingMode,parameter:Option<u32>,environment:&EvalEnv,active:&mut ActiveQueries,b:&mut Budget)->Check<CheckedType>{
        if !self.resource_recipe(module,recipe,environment,0,b)?{return Ok(value);}
        let constructor=match mode{
            PassingMode::Create|PassingMode::Give=>dynamic_witness::semantic_external::Constructor::Managed,
            PassingMode::Infer|PassingMode::Describe=>{let ordinal=parameter.ok_or_else(||unsupported("bare resource result has no original owning/borrow result mode",None))?;let frame=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?;let mode=StoredBorrowMode::ReadOnlyDescription;dynamic_witness::semantic_external::Constructor::Address{mode,outer:Some(ClassKey::Outer{scope:frame.key,operand:OperandKey::Parameter(ordinal),path:vec![],mode})}},
            _=>return Err(bad("original resource payload uses a nonresource signature mode")),
        };
        dynamic_witness::semantic_external::construct(self,environment,constructor,&[value],active,b)
    }
}
