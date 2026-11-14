//! Exact original-member selection at an ordinary static Call. A matching ABI
//! or another body in the provider's pending set cannot select the entry.
use super::*;
use dynamic_witness::semantic_external::{Proofs,select_original_member};

impl CheckedContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn actual_trait_member(&self,module:&TypedModule,site:&SemanticSite,callee:FunctionId,arguments:&[ValueId],values:&[Option<CheckedType>],environment:&EvalEnv,original:original::TraitCallRecipes,external:&mut Proofs,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<CheckedType>>{
        let receiver=self.evaluate_type(module,&original.receiver,environment,external,active,b)?;
        let application=self.application(module,&original.application,environment,external,active,b)?;
        let mut actuals=Vec::new();for recipe in &original.actuals{actuals.push(self.evaluate_actual(module,recipe,environment,external,active,b)?);}
        let receipt=select_original_member(external,module,self,environment,&receiver,&application,original.member,&actuals,active,b)?;
        if receipt.member!=original.member||receipt.application!=application||receipt.receiver!=receiver{return Err(bad("actual member selection changes its complete requested identity"));}
        let programme=module.semantic.as_ref().ok_or_else(||bad("actual member call lacks the original programme"))?;
        b.spend(programme.instances.len())?;
        let mut instances=programme.instances.iter().filter(|row|matches!(row.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id))if id==callee));
        let selected=instances.next().ok_or_else(||bad("actual member target has no exact source instance"))?;
        if instances.next().is_some()||selected.definition!=receipt.body{return Err(bad("actual member Call selects another original override/default body"));}
        let function=module.functions.get(callee.0 as usize).filter(|row|row.id==callee).ok_or_else(||bad("actual member function identity is absent"))?;
        if function.foreign.is_some()||function.environment.is_some(){return Err(bad("ordinary member Call selects foreign or hidden closure authority"));}
        if let Some(instance)=&function.dyn_instance{
            dynamic_metadata::recipe(instance,b)?;
            if instance.definition!=receipt.definition{return Err(bad("actual member Dyn instance selects another original definition"));}
            // This is redundant dynamic authority, not needed to name a
            // static method. Its full independent dynamic proof is mandatory.
            return Err(unsupported("static member carrying dynamic-instance authority needs its complete dynamic bridge",None));
        }
        let claims=match &site.action{SemanticSiteAction::BodyCall{actuals}=>actuals,SemanticSiteAction::Call{target:SemanticFunctionTarget::Typed(id),actuals}if *id==callee=>actuals,_=>return Err(bad("actual member Call has another source emission role"))};
        if claims.len()!=actuals.len(){return Err(bad("actual member call omits its full mixed method actuals"));}
        for(expected,claim)in actuals.iter().zip(claims){self.member_actual_claim(module,expected,claim,environment,b)?;}
        let selected_env=self.environment_for_instance(module,selected.id,active,b)?;
        let mut boundary=Boundary::default();
        if selected_env.lexical.len()!=receipt.environment.lexical.len(){return Err(bad("actual member body changes its full true lexical ancestry"));}
        for(actual,expected)in selected_env.lexical.iter().zip(&receipt.environment.lexical){
            if actual.definition!=expected.definition||actual.actuals.len()!=expected.actuals.len()||actual.environment.len()!=expected.environment.len(){return Err(bad("actual member body changes an original parent or full mixed arity"));}
            for(a,z)in actual.actuals.iter().zip(&expected.actuals).chain(actual.environment.iter().zip(&expected.environment)){boundary.actual(self,environment,a,z,b)?;}
            match(&actual.self_type,&expected.self_type){(Some(a),Some(z))=>boundary.value(self,environment,a,z,b)?,(None,None)=>{},_=>return Err(bad("actual member body loses its complete Self domain"))}
        }
        let signature=&receipt.actual_signature;
        if signature.parameters.len()!=arguments.len()||function.parameters.len()!=arguments.len()||function.call_kind!=signature.call_kind||function.result_mode!=signature.result_mode||function.return_type!=signature.result.ty{return Err(bad("actual member whole function ABI differs from its exact source specialization"));}
        for(((expected,_mode),argument),parameter)in signature.parameters.iter().zip(arguments).zip(&function.parameters){
            let actual=value_at(values,*argument)?;
            let local=function.locals.get(parameter.0 as usize).ok_or_else(||bad("actual member parameter local is absent"))?;
            if local.ty!=expected.ty||!self.compatible(environment,actual,expected,b)?{return Err(bad("actual member argument violates its complete invariant type/mode/class relation"));}
        }
        // These are signatures and original identity only. All actual function
        // CFG/loans/cleanup and the pending universal relation must still pass;
        // raw trait/body contract uppers have supplied no permission here.
        Ok(if signature.result.ty==Type::Unit{None}else{Some(signature.result.clone())})
    }
    fn member_actual_claim(&self,module:&TypedModule,expected:&CheckedActual,claim:&SemanticActual,environment:&EvalEnv,b:&mut Budget)->Check<()>{
        let equal=match(expected,claim){(CheckedActual::Type(expected),SemanticActual::Type(claim))=>{self.claim(module,claim,expected,environment,b)?;true},(CheckedActual::Const{carrier,value},SemanticActual::Const{ty,value:actual})=>{self.claim(module,ty,carrier,environment,b)?;value==actual},(CheckedActual::Life(expected),SemanticActual::Life(actual))=>self.same(environment,expected,&self.class(module,actual,environment,b)?,b)?,_=>false};
        if !equal{return Err(bad("actual member method argument differs from its original symbolic inference"));}Ok(())
    }
}

#[derive(Default)]
struct Boundary{classes:Vec<(ClassKey,ClassKey)>,origins:Vec<(IntrinsicOrigin,IntrinsicOrigin)>}
impl Boundary{
    fn class(&mut self,context:&CheckedContext,env:&EvalEnv,actual:&ClassKey,expected:&ClassKey,b:&mut Budget)->Check<()>{
        b.spend(self.classes.len().saturating_add(1))?;
        if let Some((_,known))=self.classes.iter().find(|(known,_)|known==actual){if !context.same(env,known,expected,b)?{return Err(bad("one member body class has inconsistent call-boundary images"));}}
        else{self.classes.push((actual.clone(),expected.clone()));}Ok(())
    }
    fn value(&mut self,context:&CheckedContext,env:&EvalEnv,actual:&CheckedType,expected:&CheckedType,b:&mut Budget)->Check<()>{
        dynamic_metadata::ty(&actual.ty,b)?;dynamic_metadata::ty(&expected.ty,b)?;
        if actual.ty!=expected.ty||actual.life!=expected.life||actual.classes.len()!=expected.classes.len()||actual.origins.len()!=expected.origins.len()||actual.statics!=expected.statics{return Err(bad("member call boundary changes its complete type/domain/static occurrences"));}
        let mut local=Vec::<(&IntrinsicOrigin,&IntrinsicOrigin)>::new();
        for(a,z)in actual.origins.iter().zip(&expected.origins){
            b.spend(local.len().saturating_add(1))?;
            if local.iter().any(|(known,image)|*image==z&&*known!=a){return Err(bad("member call boundary coalesces independent origins within one whole type"));}
            local.push((a,z));
            b.spend(self.origins.len().saturating_add(1))?;
            if let Some((_,known))=self.origins.iter().find(|(known,_)|known==a){if !context.same_origin(env,known,z,b)?{return Err(bad("member call boundary permutes a repeated intrinsic origin"));}}
            else{self.origins.push((a.clone(),z.clone()));}
        }
        for(a,z)in actual.classes.iter().zip(&expected.classes){self.class(context,env,a,z,b)?;}Ok(())
    }
    fn actual(&mut self,context:&CheckedContext,env:&EvalEnv,actual:&CheckedActual,expected:&CheckedActual,b:&mut Budget)->Check<()>{
        match(actual,expected){(CheckedActual::Type(a),CheckedActual::Type(z))=>self.value(context,env,a,z,b),(CheckedActual::Life(a),CheckedActual::Life(z))=>self.class(context,env,a,z,b),(CheckedActual::Const{carrier:a,value:av},CheckedActual::Const{carrier:z,value:zv})if av==zv=>self.value(context,env,a,z,b),_=>Err(bad("member call boundary changes its ordered actual kind or constant value"))}
    }
}
