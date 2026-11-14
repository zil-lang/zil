//! Builtin error values have exact closed language identities and complete
//! variants. Arbitrary same-layout user enums do not enter this namespace.
use super::*;
impl SealedError {
    pub(super) fn ty(self)->Type{
        let(name,variants)=match self{
            Self::MutexCreate=>("@std.sync:v1:MutexCreateError",&["Allocation"][..]),
            Self::TaskCreate=>("@std.task:v1:TaskCreateError",&["Allocation"][..]),
            Self::TaskPoll=>("@std.task:v1:TaskPollState",&["Complete","Cancelled","Invalid"][..]),
            Self::DynCreate=>return crate::dynamic_dispatch::create_error(),
        };Type::Enum{name:name.into(),variants:variants.iter().map(|name|((*name).into(),vec![])).collect()}
    }
    pub(super) fn from_type(ty:&Type)->Option<Self>{[Self::MutexCreate,Self::TaskCreate,Self::TaskPoll,Self::DynCreate].into_iter().find(|kind|kind.ty()==*ty)}
}
impl CheckedOriginal {
    pub(super) fn poll_type(&self,module:&TypedModule,result:Ty,b:&mut Budget)->Check<Ty>{
        let program=module.semantic.as_ref().ok_or_else(||bad("Poll scheme has no original nominal directory"))?;let mut selected=None;
        for definition in &program.definitions{b.spend(1)?;let SemanticDefinitionBody::Nominal{declaration,kind,body}=&definition.body else{continue;};
            if *declaration!=zil_ir::builtin_life::POLL{continue;}
            if selected.replace(definition.id).is_some(){return Err(bad("canonical Poll declaration appears more than once"));}
            if *kind!=LifeNominalKind::Enum||definition.parent.is_some()||definition.parameters.len()!=1||!matches!(&definition.parameters[0],SemanticFormal::Type{requirements}if requirements.is_empty())||!definition.requirements.is_empty()||!definition.environment.is_empty()||!definition.implicit_lifetimes.is_empty(){return Err(bad("Poll declaration has another complete original scheme"));}
            let SemanticNominalBody::Enum{variants}=body else{return Err(bad("Poll declaration has another original body"));};
            if variants.len()!=2||variants[0].name!="Pending"||!variants[0].fields.is_empty()||variants[1].name!="Ready"||variants[1].fields!=[SemanticTypeRecipe::Formal(SemanticFormalRef{scope:SemanticScopeRef::Ancestor(0),ordinal:0})]{return Err(bad("Poll declaration has another complete original variant inventory"));}
        }
        Ok(Ty::Nominal(selected.ok_or_else(||bad("Poll scheme is missing its actual canonical original declaration"))?,vec![Arg::Type(result)]))
    }
}
