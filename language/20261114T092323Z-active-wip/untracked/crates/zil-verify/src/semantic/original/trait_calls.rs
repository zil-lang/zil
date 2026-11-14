//! An abstract call is derived from the caller's original trait requirements.
//! Its selected executable body is deliberately deferred to the actual bridge.
use super::*;
mod applications;
mod materialize;

#[derive(Clone)]
pub(super) struct TraitCall{pub receiver:Ty,pub declaration:LifeDeclarationId,pub arguments:Vec<Arg>,pub member:u32,pub actuals:Vec<Arg>,pub node:NodeId,pub receiver_mode:Option<StoredBorrowMode>}
impl CheckedOriginal {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn trait_member_call(&mut self,module:&TypedModule,context:&CheckedContext,call:&SemanticOriginalExpr,callee:&SemanticOriginalExpr,written_types:Option<&[SemanticTypeRecipe]>,written_mixed:Option<&[CoreGenericArgument<SemanticTypeRecipe>]>,arguments:&[SemanticOriginalExpr],env:&Env,bindings:&BTreeMap<String,Binding>,expected:Option<&Ty>,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<Ty>>{
        let CoreExprKind::Member{object,member,owner:None}=&callee.kind else{return Ok(None);};
        // A declaration path and a real field retain normal shadowing. Only a
        // value receiver with a complete trait application enters this path.
        let mut root=object.as_ref();while let CoreExprKind::Group(inner)|CoreExprKind::Member{object:inner,..}=&root.kind{root=inner;}
        if !matches!(&root.kind,CoreExprKind::Name(name)if bindings.contains_key(name)){return Ok(None);}
        let(object_type,place)=self.place(module,context,object,env,bindings,active,b)?;
        let receiver=match self.resolved(&object_type,b)?{Ty::Address{inner,..}|Ty::Desc{inner,..}=>*inner,ty=>ty};
        if let Ty::Nominal(id,..)=&receiver{let definition=context.definition(module,*id,b)?;if matches!(&definition.body,SemanticDefinitionBody::Nominal{body:SemanticNominalBody::Product{fields},..}if fields.iter().any(|row|row.name==*member)){return Ok(None);}}
        let applications=self.original_method_applications(module,context,&receiver,member,env,active,b)?;
        if applications.is_empty(){return Ok(None);}if applications.len()!=1{return Err(bad("original member call has multiple complete trait applications"));}
        let(declaration,trait_arguments,ordinal,method)=applications.into_iter().next().unwrap();
        let original=context.definition(module,method,b)?;
        let SemanticDefinitionBody::FunctionWithBody{signature,receiver_parameter:Some(0),..}=&original.body else{return Err(unsupported("original trait call requires its exact first receiver parameter",None));};
        let scope=dynamic_witness::semantic_external::original_scope(module,declaration,b)?;
        let mut selected=self.environment(module,context,method,active,b)?;
        let frame=selected.lexical.iter_mut().find(|row|row.definition==scope.id).ok_or_else(||bad("original member omits its true trait parent"))?;
        frame.actuals=self.resolve_arguments(&trait_arguments,0,b)?;selected.self_type=Some((scope.id,self.resolved(&receiver,b)?));
        let written=written_types.map_or_else(||written_mixed.map_or(0,<[CoreGenericArgument<SemanticTypeRecipe>]>::len),<[SemanticTypeRecipe]>::len);
        if written>original.parameters.len(){return Err(bad("original member has excess written mixed actuals"));}
        let mut actuals=Vec::new();for(ordinal,formal)in original.parameters.iter().enumerate(){
            let written=if let Some(types)=written_types{types.get(ordinal).map(|ty|self.ty(module,context,ty,env,active,b).map(Arg::Type)).transpose()?}else if let Some(mixed)=written_mixed{match mixed.get(ordinal){Some(CoreGenericArgument::Type(ty))=>Some(Arg::Type(self.ty(module,context,ty,env,active,b)?)),Some(CoreGenericArgument::Lifetime(name))=>{let value=self.named(module,context,env.current()?,name,SemanticOriginalNamespace::Lifetime,b)?.ok_or_else(||bad("original method life argument is unresolved"))?;let SemanticOriginalNameValue::Recipe(recipe @ SemanticActualRecipe::Life(_))=value else{return Err(bad("original method life actual has another kind"));};Some(self.actual(module,context,recipe,env,active,b)?)},Some(CoreGenericArgument::Const(_))=>return Err(unsupported("original method Const inference needs its exact declared carrier",None)),None=>None}}else{None};
            let actual=match(written,formal){(Some(value),_)=>value,(None,SemanticFormal::Type{..})=>Arg::Type(self.fresh(b)?),(None,SemanticFormal::Lifetime)=>Arg::Life(self.fresh_life(b)?),_=>return Err(unsupported("original method Const argument is not independently inferred",None))};
            if !matches!((formal,&actual),(SemanticFormal::Type{..},Arg::Type(_))|(SemanticFormal::Lifetime,Arg::Life(_))|(SemanticFormal::Const{..},Arg::Const(_))){return Err(bad("original member changes a full-ordinal actual kind"));}actuals.push(actual);
        }
        selected.lexical[0].actuals=actuals;
        let prototype=self.call_signature(module,context,method,signature,&selected,active,b)?;
        if prototype.parameters.len()!=arguments.len()+1{return Err(bad("original member receiver/value arity differs"));}
        self.execute_effect(env,prototype.call_kind,b)?;
        let mode=prototype.modes[0];let receiver_parameter=self.resolved(&prototype.parameters[0],b)?;
        let receiver_mode=match &receiver_parameter{Ty::Address{mutable,..}=>Some(if *mutable{StoredBorrowMode::ExclusiveAddress}else{StoredBorrowMode::SharedAddress}),Ty::Desc{..}=>Some(StoredBorrowMode::ReadOnlyDescription),_=>None};
        if mode==PassingMode::BorrowMut{
            match self.resolved(&object_type,b)?{Ty::Address{mutable:true,..}=>{if let Some(place)=&place{self.written.entry(env.current()?).or_default().insert(place.binder.clone());}},Ty::Address{mutable:false,..}|Ty::Desc{..}=>return Err(bad("original mutable trait receiver crosses a shared observation")),_=>self.mutable_place(module,context,object,env,bindings,active,b)?,}
        }
        if let Some(place)=&place{self.read_original(module,context,place,&object_type,env,!matches!(mode,PassingMode::Borrow|PassingMode::BorrowMut|PassingMode::Describe),active,b)?;}
        let actual_receiver=match receiver_parameter{Ty::Address{mutable,..}=>Ty::Address{mutable,life:Life::ReceiverRole(env.current()?,call.id,receiver_mode.unwrap()),inner:Box::new(receiver.clone())},Ty::Desc{..}=>Ty::Desc{life:Life::ReceiverRole(env.current()?,call.id,StoredBorrowMode::ReadOnlyDescription),inner:Box::new(receiver.clone())},_=>object_type};
        self.unify(&actual_receiver,&prototype.parameters[0],b)?;
        for((argument,parameter),mode)in arguments.iter().zip(&prototype.parameters[1..]).zip(&prototype.modes[1..]){let parameter=self.normalize_original_type(module,context,parameter,env,active,b)?;let actual=self.argument(module,context,argument,&parameter,*mode,env,bindings,active,b)?;self.unify(&actual,&parameter,b)?;}
        for requirement in &original.requirements{self.obligation(module,context,requirement,&selected,env,active,b)?;}
        for formal in &original.parameters{if let SemanticFormal::Type{requirements}=formal{for requirement in requirements{self.obligation(module,context,requirement,&selected,env,active,b)?;}}}
        let result=self.normalize_original_type(module,context,&prototype.result,env,active,b)?;
        if let Some(expected)=expected{self.unify(&result,expected,b)?;}
        let caller=context.definition(module,env.current()?,b)?;let SemanticDefinitionBody::FunctionWithBody{sites,site_nodes,..}=&caller.body else{unreachable!()};
        let mut sites=site_nodes.iter().filter(|row|row.node==SemanticOriginalNode::Expression(call.id)).filter_map(|row|sites.iter().find(|site|site.id==row.site)).filter(|site|matches!(site.kind,SemanticSiteRecipeKind::OriginalCall));
        let site=sites.next().ok_or_else(||bad("abstract original member requires its exact BodyCall site"))?;if sites.next().is_some(){return Err(bad("original trait member call site is ambiguous"));}
        let actuals=self.resolve_arguments(&selected.lexical[0].actuals,0,b)?;
        self.trait_calls.insert((env.current()?,site.id),TraitCall{receiver,declaration,arguments:trait_arguments,member:ordinal,actuals,node:call.id,receiver_mode});
        Ok(Some(result))
    }
}
