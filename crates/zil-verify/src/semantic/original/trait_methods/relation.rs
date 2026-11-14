//! Universal impl-method head refinement. Concrete instance actuals are not
//! inputs: the impl's own rigid full mixed domain is used on both sides.
use super::*;

impl CheckedOriginal {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::semantic) fn method_relation(&mut self,module:&TypedModule,context:&CheckedContext,trait_id:LifeDeclarationId,impl_id:LifeDeclarationId,expected_id:SemanticDefinitionId,body_id:SemanticDefinitionId,source:DynDefinitionScope,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        self.checked_method_body(module,body_id,b)?;
        let trait_scope=dynamic_witness::semantic_external::original_scope(module,trait_id,b)?;
        let impl_scope=dynamic_witness::semantic_external::original_scope(module,impl_id,b)?;
        let expected=context.definition(module,expected_id,b)?;let body=context.definition(module,body_id,b)?;
        let programme=module.semantic.as_ref().ok_or_else(||bad("universal method relation has no original programme"))?;
        b.spend(programme.dyn_scopes.len().saturating_add(module.dyn_declarations.len()))?;
        let mut identities=programme.dyn_scopes.iter().filter(|row|row.scope==expected_id).filter_map(|row|match row.owner{SemanticDynOwner::MethodSignature{declaration,member}if declaration==trait_id=>Some(member),_=>None});
        let member=identities.next().ok_or_else(||bad("universal expected method has no exact trait member"))?;
        if identities.next().is_some()||expected.parent!=Some(trait_scope.id){return Err(bad("universal expected method has an ambiguous member or another parent"));}
        let mut raw=module.dyn_declarations.iter().filter(|row|row.id==trait_id);let trait_decl=raw.next().ok_or_else(||bad("universal method trait declaration is absent"))?;if raw.next().is_some(){return Err(bad("universal method trait declaration is ambiguous"));}
        let mut raw=module.dyn_declarations.iter().filter(|row|row.id==impl_id);let impl_decl=raw.next().ok_or_else(||bad("universal method impl declaration is absent"))?;if raw.next().is_some(){return Err(bad("universal method impl declaration is ambiguous"));}
        dynamic_metadata::declaration(trait_decl,b)?;dynamic_metadata::declaration(impl_decl,b)?;
        let original_self=match &impl_scope.body{SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl{self_type,trait_application,..})if trait_application.declaration==trait_id=>Some((self_type,trait_application)),_=>None}.ok_or_else(||bad("universal method request selects another impl/trait application"))?;
        let impl_env=self.environment(module,context,impl_scope.id,active,b)?;
        let receiver=self.ty(module,context,original_self.0,&impl_env,active,b)?;
        let mut trait_env=self.environment(module,context,trait_scope.id,active,b)?;let mut args=Vec::new();for recipe in &original_self.1.arguments{args.push(self.actual(module,context,recipe,&impl_env,active,b)?);}trait_env.lexical[0].actuals=args;trait_env.self_type=Some((trait_scope.id,receiver));
        let mut expected_env=self.environment(module,context,expected_id,active,b)?;
        replace_parents(&mut expected_env,&trait_env,b)?;
        let mut body_env=self.environment(module,context,body_id,active,b)?;
        replace_parents(&mut body_env,if source==DynDefinitionScope::TraitDefault{&trait_env}else{&impl_env},b)?;
        if body.parent!=Some(if source==DynDefinitionScope::TraitDefault{trait_scope.id}else{impl_scope.id}){return Err(bad("universal method body belongs to another exact original parent"));}
        let mut body_owners=programme.dyn_scopes.iter().filter(|row|row.scope==body_id).filter_map(|row|if let SemanticDynOwner::MethodDefinition(id)=row.owner{Some(id)}else{None});
        let identity=body_owners.next().ok_or_else(||bad("universal method body has no exact original definition identity"))?;
        if body_owners.next().is_some(){return Err(bad("universal method body has multiple original definition identities"));}
        let owner=if source==DynDefinitionScope::TraitDefault{trait_decl}else{impl_decl};
        let raw_definitions=owner.method_definitions.as_ref().ok_or_else(||unsupported("universal method body inventory is Unknown",None))?;
        let mut definitions=raw_definitions.iter().filter(|row|row.id==identity&&row.scope==source);
        let definition=definitions.next().ok_or_else(||bad("universal method body is absent from its actual original owner"))?;
        if definitions.next().is_some(){return Err(bad("universal method body definition is duplicated"));}
        let expected_member=trait_decl.members.get(member as usize).ok_or_else(||bad("universal trait method ordinal is absent"))?;
        let actual_member=owner.members.get(definition.member as usize).ok_or_else(||bad("universal impl method ordinal is absent"))?;
        if expected_member.name!=actual_member.name||!matches!((&expected_member.kind,&actual_member.kind),(DynMemberKind::Method(a),DynMemberKind::Method(z))if a==z){return Err(bad("universal method changes the original member identity or complete formal kinds"));}
        if expected.parameters.len()!=body.parameters.len(){return Err(bad("universal method changes its complete mixed method arity"));}
        if let DynMemberKind::Method(kinds)=&actual_member.kind{for(formal,kind)in body.parameters.iter().zip(kinds){if let(SemanticFormal::Const{carrier},LifeParameterKind::Const(ty))=(formal,kind){if !matches!(carrier,SemanticTypeRecipe::Atom(actual)if actual==ty){return Err(unsupported("universal method Const formal has no exact original complete scalar carrier",None));}}}}
        for(a,z)in expected.parameters.iter().zip(&body.parameters){if !matches!((a,z),(SemanticFormal::Type{..},SemanticFormal::Type{..})|(SemanticFormal::Lifetime,SemanticFormal::Lifetime)|(SemanticFormal::Const{..},SemanticFormal::Const{..})){return Err(bad("universal method changes an ordered formal kind"));}}
        let actuals=self.apply_arguments(&expected_env.lexical[0].actuals,&expected_env,0,b)?;
        body_env.lexical[0].actuals=actuals;
        let SemanticDefinitionBody::FunctionWithBody{signature:expected_signature,receiver_parameter:expected_receiver,protocol_contracts:expected_protocol,..}=&expected.body else{return Err(bad("universal expected method omits its full original signature"));};
        let SemanticDefinitionBody::FunctionWithBody{signature:body_signature,receiver_parameter:body_receiver,protocol_contracts:body_protocol,..}=&body.body else{return Err(bad("universal body omits its full original signature"));};
        if expected_receiver!=body_receiver||expected_protocol!=body_protocol{return Err(unsupported("universal method receiver/protocol refinement is not exact",None));}
        if definition.signature.parameters.len()!=body_signature.parameters.len()||definition.signature.result_mode!=body_signature.result_mode||definition.signature.call_kind!=body_signature.call_kind||definition.signature.invocation_binder_count!=body_signature.invocation_lifetimes{return Err(bad("raw method definition changes its original complete header"));}
        for(a,z)in definition.signature.parameters.iter().zip(&body_signature.parameters){if a.mode!=z.mode||!matches!(&a.ty,DynTypePattern::Semantic{scope,ty}if *scope==body_id&&ty.as_ref()==&z.ty){return Err(bad("raw method body changes its source parameter type/mode"));}}
        if !matches!(&definition.signature.result,DynTypePattern::Semantic{scope,ty}if *scope==body_id&&ty.as_ref()==&body_signature.result){return Err(bad("raw method body changes its source result type"));}
        let previous=self.current.replace(expected_id);
        let projection_depth=self.projection_terms.len();
        // The checked impl declaration is a premise of this universal
        // refinement goal. The body's additional method premises are not.
        let saved_equalities=self.equalities.remove(&expected_id);let saved_lives=self.life_equalities.remove(&expected_id);
        let result=(||{
            self.current=Some(impl_scope.id);
            let trait_arguments=self.resolve_arguments(&trait_env.lexical[0].actuals,0,b)?;
            for(ordinal,member)in trait_decl.members.iter().enumerate(){if member.kind==DynMemberKind::Type{
                let receiver=self.resolved(&trait_env.self_type.as_ref().ok_or_else(||bad("universal method omits its receiver role"))?.1,b)?;
                let value=self.associated_type(module,context,&receiver,trait_id,&trait_arguments,ordinal as u32,&impl_env,active,b)?;
                let arguments=self.resolve_arguments(&trait_arguments,0,b)?;let value=self.resolved(&value,b)?;
                self.projection_terms.push((receiver,trait_id,arguments,ordinal as u32,value));
            }}
            self.current=Some(expected_id);
            let mut equalities=Vec::new();
            for(definition,environment)in [(impl_scope,&impl_env),(trait_scope,&trait_env),(expected,&expected_env)]{
                for requirement in &definition.requirements{self.collect_equalities(module,context,requirement,environment,&mut equalities,active,b)?;}
                for formal in &definition.parameters{if let SemanticFormal::Type{requirements}=formal{for requirement in requirements{self.collect_equalities(module,context,requirement,environment,&mut equalities,active,b)?;}}}
            }
            self.equalities.insert(expected_id,equalities);
            let a=self.signature(module,context,expected_signature,&expected_env,active,b)?;let z=self.signature(module,context,body_signature,&body_env,active,b)?;
            self.abstract_method(module,context,expected,&a,active,b)?;
            if a.parameters.len()!=z.parameters.len()||a.modes!=z.modes||a.result_mode!=z.result_mode||a.call_kind!=z.call_kind||a.bound_lifetimes!=z.bound_lifetimes||a.invocation_lifetimes!=z.invocation_lifetimes{return Err(bad("universal method changes its complete P/R modes, call kind or quantifier domains"));}
            let mut pairs=z.environment.invocation.iter().zip(&a.environment.invocation).map(|(a,z)|(a.clone(),z.clone())).collect::<Vec<_>>();
            if a.bound_lifetimes!=0{pairs.extend(z.environment.bounds[0].iter().zip(&a.environment.bounds[0]).map(|(a,z)|(a.clone(),z.clone())));}
            for(actual,expected)in z.parameters.iter().zip(&a.parameters){if !self.method_type_equal(actual,expected,body_id,expected_id,&mut pairs,0,b)?{return Err(bad("universal method parameter changes its full rigid type/lifetime partition"));}}
            if !self.method_type_equal(&z.result,&a.result,body_id,expected_id,&mut pairs,0,b)?{return Err(bad("universal method result changes its full rigid type/lifetime partition"));}
            for requirement in &body.requirements{self.obligation(module,context,requirement,&body_env,&expected_env,active,b)?;}
            for formal in &body.parameters{if let SemanticFormal::Type{requirements}=formal{for requirement in requirements{self.obligation(module,context,requirement,&body_env,&expected_env,active,b)?;}}}
            Ok(())
        })();
        self.equalities.remove(&expected_id);if let Some(value)=saved_equalities{self.equalities.insert(expected_id,value);}
        self.life_equalities.remove(&expected_id);if let Some(value)=saved_lives{self.life_equalities.insert(expected_id,value);}
        self.projection_terms.truncate(projection_depth);
        self.current=previous;result
    }
}

fn replace_parents(target:&mut Env,source:&Env,b:&mut Budget)->Check<()>{
    b.spend(target.lexical.len().saturating_mul(source.lexical.len().saturating_add(1)))?;
    for frame in target.lexical.iter_mut().skip(1){if let Some(actual)=source.lexical.iter().find(|row|row.definition==frame.definition){*frame=actual.clone();}}
    target.self_type=source.self_type.clone();Ok(())
}

impl CheckedOriginal {
    fn method_life_equal(&self,actual:&Life,expected:&Life,body:SemanticDefinitionId,declaration:SemanticDefinitionId,pairs:&mut Vec<(Life,Life)>,b:&mut Budget)->Check<bool>{
        b.spend(pairs.len().saturating_add(1))?;
        if let Some((_,known))=pairs.iter().find(|(known,_)|known==actual){return self.same_life(known,expected,b);}
        if self.same_life(actual,expected,b)?{return Ok(true);}
        let a=matches!(actual,Life::Implicit(owner,_)|Life::Head(owner,_)if *owner==body);
        let z=matches!(expected,Life::Implicit(owner,_)|Life::Head(owner,_)if *owner==declaration);
        if !a||!z||pairs.iter().any(|(_,known)|known==expected){return Ok(false);}
        pairs.push((actual.clone(),expected.clone()));Ok(true)
    }
    fn method_argument_equal(&self,actual:&Arg,expected:&Arg,body:SemanticDefinitionId,declaration:SemanticDefinitionId,pairs:&mut Vec<(Life,Life)>,depth:usize,b:&mut Budget)->Check<bool>{
        match(actual,expected){
            (Arg::Type(a),Arg::Type(z))=>self.method_type_equal(a,z,body,declaration,pairs,depth+1,b),
            (Arg::Life(a),Arg::Life(z))=>self.method_life_equal(a,z,body,declaration,pairs,b),
            (Arg::Const(a),Arg::Const(z))=>Ok(a.value==z.value&&self.method_type_equal(&a.carrier,&z.carrier,body,declaration,pairs,depth+1,b)?),
            _=>Ok(false),
        }
    }
    fn method_type_equal(&self,actual:&Ty,expected:&Ty,body:SemanticDefinitionId,declaration:SemanticDefinitionId,pairs:&mut Vec<(Life,Life)>,depth:usize,b:&mut Budget)->Check<bool>{
        if depth>b.limits.max_call_depth.min(128){return Err(limit("universal method type relation depth exceeded"));}
        let actual=self.resolved(actual,b)?;let expected=self.resolved(expected,b)?;
        if self.same(&actual,&expected,b)?{return Ok(true);}
        Ok(match(&actual,&expected){
            (Ty::Nominal(a,aa),Ty::Nominal(z,za))|(Ty::Opaque(a,aa),Ty::Opaque(z,za))if a==z&&aa.len()==za.len()=>{for(a,z)in aa.iter().zip(za){if !self.method_argument_equal(a,z,body,declaration,pairs,depth+1,b)?{return Ok(false);}}true},
            (Ty::Projection{receiver:a,declaration:ad,arguments:aa,member:am},Ty::Projection{receiver:z,declaration:zd,arguments:za,member:zm})if ad==zd&&am==zm&&aa.len()==za.len()=>{if !self.method_type_equal(a,z,body,declaration,pairs,depth+1,b)?{return Ok(false);}for(a,z)in aa.iter().zip(za){if !self.method_argument_equal(a,z,body,declaration,pairs,depth+1,b)?{return Ok(false);}}true},
            (Ty::Tuple(a),Ty::Tuple(z))if a.len()==z.len()=>{for(a,z)in a.iter().zip(z){if !self.method_type_equal(a,z,body,declaration,pairs,depth+1,b)?{return Ok(false);}}true},
            (Ty::Address{mutable:a,life:al,inner:ai},Ty::Address{mutable:z,life:zl,inner:zi})if a==z=>self.method_life_equal(al,zl,body,declaration,pairs,b)?&&self.method_type_equal(ai,zi,body,declaration,pairs,depth+1,b)?,
            (Ty::Desc{life:al,inner:ai},Ty::Desc{life:zl,inner:zi})=>self.method_life_equal(al,zl,body,declaration,pairs,b)?&&self.method_type_equal(ai,zi,body,declaration,pairs,depth+1,b)?,
            (Ty::Managed(a),Ty::Managed(z))|(Ty::Store(a),Ty::Store(z))|(Ty::Slice(a),Ty::Slice(z))=>self.method_type_equal(a,z,body,declaration,pairs,depth+1,b)?,
            (Ty::ErrorUnion(a,r),Ty::ErrorUnion(z,s))=>self.method_type_equal(a,z,body,declaration,pairs,depth+1,b)?&&self.method_type_equal(r,s,body,declaration,pairs,depth+1,b)?,
            (Ty::Array(a,n),Ty::Array(z,m))=>n.value==m.value&&self.method_type_equal(&n.carrier,&m.carrier,body,declaration,pairs,depth+1,b)?&&self.method_type_equal(a,z,body,declaration,pairs,depth+1,b)?,
            (Ty::Carrier{kind:a,child:ac,retained:ar},Ty::Carrier{kind:z,child:zc,retained:zr})if a==z=>ar==zr&&self.method_type_equal(ac,zc,body,declaration,pairs,depth+1,b)?,
            (Ty::Code{kind:a,parameters:ap,result:ar,modes:am,result_mode:arm,call_kind:ak,bound_lifetimes:ab,invocation_lifetimes:ai},Ty::Code{kind:z,parameters:zp,result:zr,modes:zm,result_mode:zrm,call_kind:zk,bound_lifetimes:zb,invocation_lifetimes:zi})if a==z&&am==zm&&arm==zrm&&ak==zk&&ab==zb&&ai==zi&&ap.len()==zp.len()=>{for(a,z)in ap.iter().zip(zp){if !self.method_type_equal(a,z,body,declaration,pairs,depth+1,b)?{return Ok(false);}}self.method_type_equal(ar,zr,body,declaration,pairs,depth+1,b)?},
            _=>false,
        })
    }
}
