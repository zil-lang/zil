//! Bare product construction solves the original declaration's entire mixed
//! domain. All field occurrences constrain one local solution; no Typed claim,
//! first field or outside class image supplies a missing actual.
use super::*;
impl CheckedOriginal{
    #[allow(clippy::too_many_arguments)]
    pub(super) fn record_constructor(&mut self,module:&TypedModule,context:&CheckedContext,expression:&SemanticOriginalExpr,declared:Option<SemanticDefinitionId>,fields:&[(String,SemanticOriginalExpr)],env:&Env,bindings:&BTreeMap<String,Binding>,wanted:Option<&Ty>,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty>{
        let owner=env.current()?;let wanted=wanted.or(self.binding_families.get(&(owner,expression.id))).map(|ty|self.resolved(ty,b)).transpose()?;
        let wanted=match wanted{Some(Ty::Managed(inner))=>Some(*inner),other=>other};
        let contextual=match &wanted{Some(Ty::Nominal(id,arguments))=>Some((*id,arguments.as_slice())),_=>None};
        let definition=declared.or_else(||contextual.map(|(id,_)|id)).ok_or_else(||bad("original record constructor has no exact declaration or nominal context"))?;
        if contextual.is_some_and(|(id,_)|id!=definition){return Err(bad("original record constructor changes declaration"));}
        let original=context.definition(module,definition,b)?;
        let SemanticDefinitionBody::Nominal{body:SemanticNominalBody::Product{fields:parameters},..}=&original.body else{return Err(bad("original record constructor selects a nonproduct declaration"));};
        if fields.len()!=parameters.len(){return Err(bad("original record constructor does not supply every declared field"));}
        let mut names=BTreeSet::new();for(name,_)in fields{b.spend(name.len().saturating_add(parameters.len()))?;if !names.insert(name)||!parameters.iter().any(|field|field.name==*name){return Err(bad("original record constructor duplicates or changes a declared field"));}}
        if contextual.is_some_and(|(_,arguments)|arguments.len()!=original.parameters.len()){return Err(bad("original record context changes its complete mixed arity"));}
        let mut selected=self.environment(module,context,definition,active,b)?;
        let mut heads=Vec::new();let mut pending=BTreeSet::new();
        for(ordinal,formal)in original.parameters.iter().enumerate(){
            let hint=contextual.and_then(|(_,arguments)|arguments.get(ordinal)).map(|value|self.resolve_arguments(std::slice::from_ref(value),0,b).map(|mut values|values.remove(0))).transpose()?;
            let value=match(formal,hint){
                (SemanticFormal::Type{..},Some(Arg::Type(ty)))if !matches!(ty,Ty::Infer(_))=>Arg::Type(ty),
                (SemanticFormal::Type{..},Some(Arg::Type(_))|None)=>Arg::Type(self.fresh(b)?),
                (SemanticFormal::Lifetime,Some(Arg::Life(life)))if !matches!(life,Life::Infer(_))=>Arg::Life(life),
                (SemanticFormal::Lifetime,Some(Arg::Life(_))|None)=>{let life=self.fresh_life(b)?;let Life::Infer(id)=life else{unreachable!()};self.outer_inferences.insert(id,Vec::new());heads.push((id,u32::try_from(ordinal).map_err(|_|limit("original constructor Life ordinal exceeds its full domain"))?));Arg::Life(life)},
                (SemanticFormal::Const{..},Some(value@Arg::Const(_)))=>value,
                (SemanticFormal::Const{..},None)=>{pending.insert(ordinal as u32);selected.lexical[0].actuals[ordinal].clone()},
                _=>return Err(bad("original record context changes a full-ordinal mixed kind")),
            };b.spend(1)?;selected.lexical[0].actuals[ordinal]=value;
        }
        for ordinal in &pending{let SemanticFormal::Const{carrier}=&original.parameters[*ordinal as usize]else{unreachable!()};let carrier=self.ty(module,context,carrier,&selected,active,b)?;selected.lexical[0].actuals[*ordinal as usize]=Arg::Const(Const{carrier:Box::new(carrier),value:ConstValue::Variable(definition,*ordinal)});}
        // Pure numeric literals carry no move or effect. Delay only these
        // until all nonliteral fields constrain their contextual carriers.
        let mut constants=BTreeMap::new();
        for literal in [false,true]{for(name,value)in fields{
            if Self::record_literal(value,b)?!=literal{continue;}
            let field=parameters.iter().find(|field|field.name==*name).ok_or_else(||bad("original record field disappeared from its exact declaration"))?;
            let expected=self.ty(module,context,&field.ty,&selected,active,b)?;
            let needs=self.record_const_types(&expected,None,definition,&pending,&mut constants,0,b)?;
            let actual=self.expression(module,context,value,env,bindings,if needs{None}else{Some(&expected)},active,b)?;
            self.record_const_types(&expected,Some(&actual),definition,&pending,&mut constants,0,b)?;
            for(ordinal,value)in &constants{selected.lexical[0].actuals[*ordinal as usize]=Arg::Const(value.clone());pending.remove(ordinal);}
            let expected=self.ty(module,context,&field.ty,&selected,active,b)?;self.relate_value(&actual,&expected,b)?;
        }}
        if !pending.is_empty(){return Err(bad("original record Const actual is not determined by all fields and its complete context"));}
        self.finish_record_heads(&heads,expression.id,b)?;
        let actuals=self.resolve_arguments(&selected.lexical[0].actuals,0,b)?;
        for(formal,actual)in original.parameters.iter().zip(&actuals){match(formal,actual){
            (SemanticFormal::Type{..},Arg::Type(ty))if !usage::has_inference(ty,&self.substitutions,b)?=>{},
            (SemanticFormal::Lifetime,Arg::Life(life))if !matches!(life,Life::Infer(_))=>{},
            (SemanticFormal::Const{..},Arg::Const(value))if !usage::has_inference(&value.carrier,&self.substitutions,b)?=>{},
            _=>return Err(bad("original record mixed actual remains ambiguous or changes its formal kind")),
        }}
        selected.lexical[0].actuals=actuals.clone();
        for(formal,actual)in original.parameters.iter().zip(&actuals){if let(SemanticFormal::Const{carrier},Arg::Const(value))=(formal,actual){let expected=self.ty(module,context,carrier,&selected,active,b)?;self.unify(&value.carrier,&expected,b)?;}}
        for requirement in &original.requirements{self.obligation(module,context,requirement,&selected,env,active,b)?;}
        for formal in &original.parameters{if let SemanticFormal::Type{requirements}=formal{for requirement in requirements{self.obligation(module,context,requirement,&selected,env,active,b)?;}}}
        self.constructed_resource(module,context,Ty::Nominal(definition,actuals),expression,env,b)
    }
    fn record_literal(expression:&SemanticOriginalExpr,b:&mut Budget)->Check<bool>{
        b.spend(1)?;if Self::contextual_numeric_literal(expression,b)?{return Ok(true);}
        match &expression.kind{CoreExprKind::Array(fields)|CoreExprKind::Tuple(fields)=>{for field in fields{if !Self::record_literal(field,b)?{return Ok(false);}}Ok(true)},CoreExprKind::Group(inner)=>Self::record_literal(inner,b),_=>Ok(false)}
    }
    fn finish_record_heads(&mut self,heads:&[(u64,u32)],node:NodeId,b:&mut Budget)->Check<()>{
        for(id,ordinal)in heads{
            let sources=self.outer_inferences.remove(id).ok_or_else(||bad("original constructor lost its independent outer inference role"))?;
            let target=self.resolved_life(&Life::Infer(*id),b)?;
            if matches!(target,Life::Infer(_)){
                if sources.is_empty(){continue;}
                let mut unique=Vec::new();for source in sources{let source=self.resolved_life(&source,b)?;b.spend(unique.len().saturating_add(1))?;if !unique.contains(&source){unique.push(source);}}
                let first=&unique[0];let mut same=true;for source in &unique{if !self.same_life(first,source,b)?{same=false;}}
                let owner=self.current.ok_or_else(||bad("original constructor region has no exact caller"))?;
                let input=matches!(first,Life::Formal(..))||self.prototypes.get(&owner).is_some_and(|prototype|prototype.parameters.iter().any(|parameter|matches!(parameter,Ty::Address{life,..}|Ty::Desc{life,..}if life==first)));
                if same&&input&&self.local_outer_sources(first,b)?.is_none(){self.unify_life(&target,first,b)?;}
                else{b.spend(unique.len().saturating_add(4))?;self.call_regions.insert(*id,(owner,node,vec![u32::MAX-1,0,*ordinal],unique));self.unify_life(&target,&Life::CallRegion(*id),b)?;}
            }else{for source in sources{self.relate_outer(&source,&target,b)?;}}
        }Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn record_const_types(&mut self,pattern:&Ty,actual:Option<&Ty>,owner:SemanticDefinitionId,pending:&BTreeSet<u32>,values:&mut BTreeMap<u32,Const>,depth:usize,b:&mut Budget)->Check<bool>{
        if pending.is_empty(){return Ok(false);}
        b.spend(1)?;if depth>b.limits.max_call_depth.min(128){return Err(limit("original record Const field structure exceeds its complete depth"));}
        let mut used=false;
        match pattern{
            Ty::Array(child,length)=>{let pair=match actual{Some(Ty::Array(child,length))=>Some((child.as_ref(),length.as_ref())),_=>None};used|=self.record_const_value(length,pair.map(|(_,length)|length),owner,pending,values,depth+1,b)?;used|=self.record_const_types(child,pair.map(|(child,_)|child),owner,pending,values,depth+1,b)?;},
            Ty::Address{inner,..}|Ty::Desc{inner,..}|Ty::Managed(inner)|Ty::Store(inner)|Ty::Slice(inner)=>{let child=match actual{Some(Ty::Address{inner,..}|Ty::Desc{inner,..}|Ty::Managed(inner)|Ty::Store(inner)|Ty::Slice(inner))=>Some(inner.as_ref()),_=>None};used|=self.record_const_types(inner,child,owner,pending,values,depth+1,b)?;},
            Ty::Tuple(fields)=>{let children=match actual{Some(Ty::Tuple(fields))=>Some(fields),_=>None};for(index,field)in fields.iter().enumerate(){used|=self.record_const_types(field,children.and_then(|fields|fields.get(index)),owner,pending,values,depth+1,b)?;}},
            Ty::Nominal(_,arguments)|Ty::Opaque(_,arguments)|Ty::Closure(_,arguments,_)|Ty::Projection{arguments,..}=>{let children=match actual{Some(Ty::Nominal(_,arguments)|Ty::Opaque(_,arguments)|Ty::Closure(_,arguments,_)|Ty::Projection{arguments,..})=>Some(arguments),_=>None};for(index,argument)in arguments.iter().enumerate(){match argument{Arg::Type(ty)=>used|=self.record_const_types(ty,children.and_then(|arguments|arguments.get(index)).and_then(|arg|match arg{Arg::Type(ty)=>Some(ty),_=>None}),owner,pending,values,depth+1,b)?,Arg::Const(value)=>used|=self.record_const_value(value,children.and_then(|arguments|arguments.get(index)).and_then(|arg|match arg{Arg::Const(value)=>Some(value),_=>None}),owner,pending,values,depth+1,b)?,_=>{}}}},
            Ty::ErrorUnion(error,result)=>{let pair=match actual{Some(Ty::ErrorUnion(error,result))=>Some((error.as_ref(),result.as_ref())),_=>None};used|=self.record_const_types(error,pair.map(|(error,_)|error),owner,pending,values,depth+1,b)?;used|=self.record_const_types(result,pair.map(|(_,result)|result),owner,pending,values,depth+1,b)?;},
            Ty::Carrier{child,..}=>used|=self.record_const_types(child,match actual{Some(Ty::Carrier{child,..})=>Some(child.as_ref()),_=>None},owner,pending,values,depth+1,b)?,
            Ty::Code{parameters,result,..}=>{let pair=match actual{Some(Ty::Code{parameters,result,..})=>Some((parameters,result.as_ref())),_=>None};for(index,parameter)in parameters.iter().enumerate(){used|=self.record_const_types(parameter,pair.and_then(|(parameters,_)|parameters.get(index)),owner,pending,values,depth+1,b)?;}used|=self.record_const_types(result,pair.map(|(_,result)|result),owner,pending,values,depth+1,b)?;},
            _=>{},
        }Ok(used)
    }
    #[allow(clippy::too_many_arguments)]
    fn record_const_value(&mut self,pattern:&Const,actual:Option<&Const>,owner:SemanticDefinitionId,pending:&BTreeSet<u32>,values:&mut BTreeMap<u32,Const>,depth:usize,b:&mut Budget)->Check<bool>{
        b.spend(1)?;if depth>b.limits.max_call_depth.min(128){return Err(limit("original record Const expression exceeds its complete depth"));}
        if let ConstValue::Variable(definition,ordinal)=&pattern.value{if *definition==owner&&pending.contains(ordinal){if let Some(actual)=actual{self.unify(&pattern.carrier,&actual.carrier,b)?;if let Some(old)=values.get(ordinal){self.unify(&old.carrier,&actual.carrier,b)?;if old.value!=actual.value{return Err(bad("original record fields disagree on one complete Const actual"));}}else{values.insert(*ordinal,actual.clone());}}return Ok(true);}}
        match &pattern.value{
            ConstValue::Unary(op,value)=>self.record_const_value(value,actual.and_then(|actual|match &actual.value{ConstValue::Unary(other,value)if op==other=>Some(value.as_ref()),_=>None}),owner,pending,values,depth+1,b),
            ConstValue::Binary(op,left,right)=>{let pair=actual.and_then(|actual|match &actual.value{ConstValue::Binary(other,left,right)if op==other=>Some((left.as_ref(),right.as_ref())),_=>None});let a=self.record_const_value(left,pair.map(|(left,_)|left),owner,pending,values,depth+1,b)?;let z=self.record_const_value(right,pair.map(|(_,right)|right),owner,pending,values,depth+1,b)?;Ok(a||z)},
            ConstValue::Convert(value)=>self.record_const_value(value,actual.and_then(|actual|match &actual.value{ConstValue::Convert(value)=>Some(value.as_ref()),_=>None}),owner,pending,values,depth+1,b),
            _=>Ok(false),
        }
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    fn owner()->SemanticDefinitionId{SemanticDefinitionId{module:zil_base::ModuleId(0),index:7}}
    #[test]
    fn constructor_region_keeps_every_borrow_source_and_never_uses_its_parent_as_identity(){
        let mut checked=CheckedOriginal::default();checked.current=Some(owner());let mut b=Budget::new(&Limits::default());
        let a=Life::Formal(owner(),0);let z=Life::Formal(owner(),1);let local=Life::Local(owner(),NodeId(10));let other=Life::Local(owner(),NodeId(11));
        checked.borrow_parents.insert((owner(),NodeId(10)),a.clone());checked.borrow_parents.insert((owner(),NodeId(11)),z);
        let Life::Infer(id)=checked.fresh_life(&mut b).unwrap()else{unreachable!()};checked.outer_inferences.insert(id,vec![local.clone()]);checked.finish_record_heads(&[(id,2)],NodeId(12),&mut b).unwrap();
        let region=checked.resolved_life(&Life::Infer(id),&mut b).unwrap();assert_eq!(region,Life::CallRegion(id));assert_eq!(checked.call_regions[&id].2,vec![u32::MAX-1,0,2]);assert_eq!(checked.call_regions[&id].3,vec![local]);
        assert!(!checked.same_life(&region,&a,&mut b).unwrap());assert!(checked.outer_outlives(&region,&a,&mut BTreeSet::new(),&mut b).unwrap());
        checked.relate_outer(&other,&region,&mut b).unwrap();assert!(!checked.outer_outlives(&region,&a,&mut BTreeSet::new(),&mut b).unwrap(),"a later field source must invalidate an earlier export proof");
        let Life::Infer(id)=checked.fresh_life(&mut b).unwrap()else{unreachable!()};checked.outer_inferences.insert(id,vec![a.clone(),a.clone()]);checked.finish_record_heads(&[(id,4)],NodeId(13),&mut b).unwrap();assert_eq!(checked.resolved_life(&Life::Infer(id),&mut b).unwrap(),a);
        let Life::Infer(id)=checked.fresh_life(&mut b).unwrap()else{unreachable!()};checked.outer_inferences.insert(id,vec![]);checked.finish_record_heads(&[(id,5)],NodeId(14),&mut b).unwrap();assert_eq!(checked.resolved_life(&Life::Infer(id),&mut b).unwrap(),Life::Infer(id),"phantom roles are not padded with a fabricated solution");
    }
    #[test]
    fn constructor_const_constraints_keep_full_ordinal_carriers_and_repeated_fields(){
        let mut checked=CheckedOriginal::default();let mut b=Budget::new(&Limits::default());let scalar=Ty::Atom(Type::I32);
        let pattern=Ty::Array(Box::new(scalar.clone()),Box::new(Const{carrier:Box::new(Ty::Atom(Type::Usize)),value:ConstValue::Variable(owner(),2)}));
        let array=|length|Ty::Array(Box::new(scalar.clone()),Box::new(Const{carrier:Box::new(Ty::Atom(Type::Usize)),value:ConstValue::Value(Constant::Usize(length))}));
        let pending=BTreeSet::from([2]);let mut values=BTreeMap::new();assert!(checked.record_const_types(&pattern,None,owner(),&pending,&mut values,0,&mut b).unwrap());assert!(values.is_empty());
        checked.record_const_types(&pattern,Some(&array(3)),owner(),&pending,&mut values,0,&mut b).unwrap();assert_eq!(values[&2].value,ConstValue::Value(Constant::Usize(3)));
        checked.record_const_types(&pattern,Some(&array(3)),owner(),&pending,&mut values,0,&mut b).unwrap();assert!(checked.record_const_types(&pattern,Some(&array(4)),owner(),&pending,&mut values,0,&mut b).is_err());
        let wrong=Const{carrier:Box::new(Ty::Atom(Type::U32)),value:ConstValue::Value(Constant::U32(3))};let expected=match &pattern{Ty::Array(_,length)=>length,_=>unreachable!()};assert!(checked.record_const_value(expected,Some(&wrong),owner(),&pending,&mut BTreeMap::new(),0,&mut b).is_err());
    }
}
