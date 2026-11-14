//! Exact original-member selection at an ordinary static Call. A matching ABI
//! or another body in the provider's pending set cannot select the entry.
use super::*;
use dynamic_witness::semantic_external::{Proofs,select_original_member};

impl CheckedContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn actual_trait_member(&self,module:&TypedModule,site:&SemanticSite,callee:FunctionId,arguments:&[ValueId],values:&[Option<CheckedType>],environment:&EvalEnv,original:original::TraitCallRecipes,external:&mut Proofs,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<CheckedType>>{
        let receiver=self.evaluate_type(module,&original.receiver,environment,external,active,b)?;
        let application=self.application(module,&original.application,environment,external,active,b)?;
        let mut actuals=Vec::new();for recipe in &original.actuals{actuals.push(match recipe{original::MethodActualRecipe::Recipe(recipe)=>self.evaluate_actual(module,recipe,environment,external,active,b)?,original::MethodActualRecipe::ReceiverRole(role)=>CheckedActual::Life(self.class(module,role,environment,b)?)});}
        let receipt=select_original_member(external,module,self,environment,&receiver,&application,original.member,&actuals,active,b)?;
        if receipt.member!=original.member||receipt.application!=application||receipt.receiver!=receiver{return Err(bad("actual member selection changes its complete requested identity"));}
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
        self.call_worlds(module,callee,receipt.body,&receipt.environment,environment,active,b)?;
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
pub(super) struct Boundary{classes:Vec<(ClassKey,ClassKey)>,origins:Vec<(IntrinsicOrigin,IntrinsicOrigin)>,scopes:Vec<(ScopeKey,ScopeKey)>}
impl Boundary{
    pub(super) fn scope(&mut self,actual:ScopeKey,expected:ScopeKey,b:&mut Budget)->Check<()>{
        b.spend(self.scopes.len().saturating_add(1))?;
        if let Some((_,known))=self.scopes.iter().find(|(key,_)|*key==actual){if *known!=expected{return Err(bad("Call world changes one original lexical scope image"));}}else{self.scopes.push((actual,expected));}Ok(())
    }
    fn image(&self,key:&ClassKey,b:&mut Budget)->Check<ClassKey>{
        b.spend(self.classes.len().saturating_add(self.scopes.len()).saturating_add(1))?;
        if let Some((_,image))=self.classes.iter().find(|(source,_)|source==key){return Ok(image.clone());}
        // An inferred outer occurrence belongs to this fresh call frame. Its
        // correspondence is a role substitution, never global equality.
        if let ClassKey::Implicit{scope,ordinal}=key{if let Some((_,image))=self.scopes.iter().find(|(source,_)|source==scope){return Ok(ClassKey::Implicit{scope:*image,ordinal:*ordinal});}}
        Ok(key.clone())
    }
    pub(super) fn project_type(&mut self,context:&CheckedContext,env:&EvalEnv,value:&CheckedType,b:&mut Budget)->Check<CheckedType>{
        charge_value(value,b)?;
        let mut output=value.clone();
        for(key,source)in output.classes.iter_mut().zip(&value.classes){*key=self.image(source,b)?;}
        for(origin,source)in output.origins.iter_mut().zip(&value.origins){*origin=self.project_origin(context,env,source,0,b)?;}
        Ok(output)
    }
    fn project_origin(&self,context:&CheckedContext,env:&EvalEnv,origin:&IntrinsicOrigin,depth:usize,b:&mut Budget)->Check<IntrinsicOrigin>{
        if depth>b.limits.max_call_depth.min(128){return Err(limit("Call boundary intrinsic origin depth exceeded"));}
        super::super::nominal_origins::charge_origin(origin,b)?;
        b.spend(self.origins.len())?;
        for(source,image)in &self.origins{super::super::nominal_origins::charge_origin(source,b)?;if source==origin{return context.canonical_origin(image,b);}}
        Ok(match origin{
            IntrinsicOrigin::DerivedClass{source}=>IntrinsicOrigin::DerivedClass{source:self.image(source,b)?},
            IntrinsicOrigin::Existing{..}=>context.canonical_origin(origin,b)?,
            IntrinsicOrigin::Nominal{source,class}=>{
                let mut actuals=Vec::new();let mut environment=Vec::new();let mut class_sources=Vec::new();
                for actual in &source.actuals{actuals.push(self.project_actual(context,env,actual,depth+1,b)?);}
                for actual in &source.environment{environment.push(self.project_actual(context,env,actual,depth+1,b)?);}
                for origin in &source.class_sources{class_sources.push(self.project_origin(context,env,origin,depth+1,b)?);}
                dynamic_metadata::life_shape(&source.domain,b)?;
                IntrinsicOrigin::Nominal{source:std::rc::Rc::new(NominalOrigin{declaration:source.declaration,actuals,environment,domain:source.domain.clone(),class_sources}),class:*class}
            },
        })
    }
    fn project_actual(&self,context:&CheckedContext,env:&EvalEnv,actual:&CheckedActual,depth:usize,b:&mut Budget)->Check<CheckedActual>{
        if depth>b.limits.max_call_depth.min(128){return Err(limit("Call boundary actual origin depth exceeded"));}
        Ok(match actual{
            CheckedActual::Life(key)=>CheckedActual::Life(self.image(key,b)?),
            CheckedActual::Type(value)|CheckedActual::Const{carrier:value,..}=>{
                charge_value(value,b)?;
                let mut output=value.clone();for key in &mut output.classes{*key=self.image(key,b)?;}
                for(origin,source)in output.origins.iter_mut().zip(&value.origins){*origin=self.project_origin(context,env,source,depth+1,b)?;}
                match actual{CheckedActual::Type(_)=>CheckedActual::Type(output),CheckedActual::Const{value,..}=>{dynamic_metadata::constant(value,b)?;CheckedActual::Const{carrier:output,value:value.clone()}},_=>unreachable!()}
            },
        })
    }
    pub(super) fn class(&mut self,context:&CheckedContext,env:&EvalEnv,actual:&ClassKey,expected:&ClassKey,b:&mut Budget)->Check<()>{
        b.spend(self.classes.len().saturating_add(1))?;
        if let Some((_,known))=self.classes.iter().find(|(known,_)|known==actual){if !context.same(env,known,expected,b)?{return Err(bad("one member body class has inconsistent call-boundary images"));}}
        else{self.classes.push((actual.clone(),expected.clone()));}Ok(())
    }
    pub(super) fn value(&mut self,context:&CheckedContext,env:&EvalEnv,actual:&CheckedType,expected:&CheckedType,b:&mut Budget)->Check<()>{
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
    pub(super) fn actual(&mut self,context:&CheckedContext,env:&EvalEnv,actual:&CheckedActual,expected:&CheckedActual,b:&mut Budget)->Check<()>{
        match(actual,expected){(CheckedActual::Type(a),CheckedActual::Type(z))=>self.value(context,env,a,z,b),(CheckedActual::Life(a),CheckedActual::Life(z))=>self.class(context,env,a,z,b),(CheckedActual::Const{carrier:a,value:av},CheckedActual::Const{carrier:z,value:zv})if av==zv=>self.value(context,env,a,z,b),_=>Err(bad("member call boundary changes its ordered actual kind or constant value"))}
    }
}

fn charge_value(value:&CheckedType,b:&mut Budget)->Check<()>{
    dynamic_metadata::ty(&value.ty,b)?;dynamic_metadata::life_shape(&value.life,b)?;
    b.spend(value.classes.len().saturating_add(value.origins.len()).saturating_add(value.statics.len()))?;
    for key in &value.classes{if let ClassKey::Outer{path,..}|ClassKey::Intrinsic{path,..}=key{b.spend(path.len())?;}}
    for origin in &value.origins{super::super::nominal_origins::charge_origin(origin,b)?;}
    for row in &value.statics{match row{StaticOccurrence::Direct{path,leaf,..}=>{b.spend(path.len())?;dynamic_metadata::ty(leaf,b)?;},StaticOccurrence::Logical{position,leaf,..}=>{dynamic_metadata::position(position,b)?;dynamic_metadata::ty(leaf,b)?;}}}
    Ok(())
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn outside_call_images_preserve_whole_nominal_and_outer_domains(){
        let limits=Limits::default();
        let bytes=include_bytes!("../fixtures/phantom-closure-complete-creator.wire");
        let(module,_)=zil_ir::wire::decode_module_candidate(bytes,&limits).unwrap();
        let mut b=Budget::new(&limits);let context=CheckedContext::catalogue(&module,&mut b).unwrap();
        let program=module.semantic.as_ref().unwrap();
        let child=program.instances.iter().find(|row|row.environment.is_some()).unwrap();
        let parent=program.instances.iter().find(|row|Some(row.id)==child.parent).unwrap();
        let scope=ScopeKey::Instance(parent.id);let a=ClassKey::Formal{scope,ordinal:0};
        let env=EvalEnv{lexical:vec![LexicalFrame{definition:parent.definition,key:scope,actuals:vec![CheckedActual::Life(a.clone())],environment:vec![],self_type:None}],assumptions:vec![scope],images:vec![],bounds:vec![],invocations:vec![],at:None};
        let ty=&module.life_instances[0].ty;let Type::BorrowedStorage{life,..}=ty else{panic!("real nominal domain")};
        let nominal=context.checked_value(&module,ty,life,vec![a.clone()],&mut b).unwrap();
        let mut active=ActiveQueries::new();
        let address=dynamic_witness::semantic_external::construct(&context,&env,dynamic_witness::semantic_external::Constructor::Address{mode:StoredBorrowMode::SharedAddress,outer:Some(a.clone())},&[nominal],&mut active,&mut b).unwrap();
        let image=ClassKey::Invocation{frame:active.fresh(&mut b).unwrap(),index:0};
        let mut boundary=Boundary::default();boundary.class(&context,&env,&a,&image,&mut b).unwrap();
        let projected=boundary.project_type(&context,&env,&address,&mut b).unwrap();
        assert_eq!(projected.ty,address.ty);assert_eq!(projected.life,address.life);assert_eq!(projected.life.binder_count,2);
        assert_eq!(projected.classes,vec![image.clone(),image]);
        assert!(!context.same_origin(&env,&projected.origins[0],&projected.origins[1],&mut b).unwrap());
        assert!(!context.same(&env,&a,&projected.classes[0],&mut b).unwrap(),"private boundary must not add a global equality");
        assert!(context.compatible(&env,&projected,&projected,&mut b).unwrap());
        let mut permuted=projected.clone();permuted.origins.swap(0,1);
        assert!(!context.compatible(&env,&projected,&permuted,&mut b).unwrap(),"outer adaptation cannot permute a pointee intrinsic with the same outside image");
    }
}
