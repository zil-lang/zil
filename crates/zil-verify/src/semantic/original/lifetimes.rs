//! Only fresh call inference variables are solved. Declaration lifetimes keep
//! their rigid identity; source equality premises are queried, never invented.
use super::*;
impl CheckedOriginal {
    pub(super) fn fresh_life(&mut self,b:&mut Budget)->Check<Life>{b.spend(1)?;let id=self.next_inference;self.next_inference=id.checked_add(1).ok_or_else(||limit("original life inference identity exhausted"))?;Ok(Life::Infer(id))}
    pub(super) fn resolved_life(&self,life:&Life,b:&mut Budget)->Check<Life>{
        let mut current=life;let mut seen=BTreeSet::new();
        while let Life::Infer(id)=current{b.spend(1)?;if !seen.insert(*id){return Err(bad("original lifetime inference cycle"));}match self.life_substitutions.get(id){Some(value)=>current=value,None=>break}}
        b.spend(1)?;Ok(current.clone())
    }
    pub(super) fn same_life(&self,left:&Life,right:&Life,b:&mut Budget)->Check<bool>{
        let left=self.resolved_life(left,b)?;let right=self.resolved_life(right,b)?;if left==right{return Ok(true);}
        let equations=self.current.and_then(|id|self.life_equalities.get(&id)).map(Vec::as_slice).unwrap_or(&[]);
        let mut pending=vec![left];let mut visited=BTreeSet::new();
        while let Some(value)=pending.pop(){b.spend(equations.len().saturating_add(1))?;if !visited.insert(value.clone()){continue;}for(a,c)in equations{let a=self.resolved_life(a,b)?;let c=self.resolved_life(c,b)?;if a==value{if c==right{return Ok(true);}pending.push(c);}else if c==value{if a==right{return Ok(true);}pending.push(a);}}}Ok(false)
    }
    pub(super) fn unify_life(&mut self,left:&Life,right:&Life,b:&mut Budget)->Check<()> {
        let left=self.resolved_life(left,b)?;let right=self.resolved_life(right,b)?;if self.same_life(&left,&right,b)?{return Ok(());}
        match(&left,&right){(Life::Infer(id),value)|(value,Life::Infer(id))=>{b.spend(1)?;self.life_substitutions.insert(*id,value.clone());Ok(())},_=>Err(bad("original call relates independent rigid lifetimes without an original equality"))}
    }
}

impl CheckedOriginal {
    /// Value transport changes only a root borrow role. Complete referents,
    /// including nominal actuals and all of their intrinsic classes, stay invariant.
    pub(super) fn relate_value(&mut self,actual:&Ty,expected:&Ty,b:&mut Budget)->Check<()> {
        let actual=self.resolved(actual,b)?;let expected=self.resolved(expected,b)?;
        match (&actual,&expected) {
            (Ty::Address{mutable:a,life:source,inner:x},Ty::Address{mutable:z,life:target,inner:y}) if a==z=>{self.unify(x,y,b)?;self.relate_outer(source,target,b)},
            (Ty::Desc{life:source,inner:x},Ty::Desc{life:target,inner:y})=>{self.unify(x,y,b)?;self.relate_outer(source,target,b)},
            _=>self.unify(&actual,&expected,b),
        }
    }
    pub(super) fn defer_call_heads(&mut self,prototype:&Prototype,actuals:&[Arg],floor:u64,b:&mut Budget)->Check<Vec<(u64,Vec<u32>,bool)>> {
        let mut ids=BTreeSet::new();
        for parameter in &prototype.parameters{self.map_invocation_type(parameter,&mut |life,b|{b.spend(1)?;if let Life::Infer(id)=life{if *id>=floor||actuals.iter().any(|actual|matches!(actual,Arg::Life(Life::Infer(value))if value==id)){ids.insert(*id);}}Ok(life.clone())},0,b)?;}
        b.spend(ids.len().saturating_mul(actuals.len().saturating_add(1)))?;let mut roles=Vec::new();for id in ids{
            let formal=actuals.iter().position(|actual|matches!(actual,Arg::Life(Life::Infer(value))if *value==id));
            let role=if let Some(ordinal)=formal{vec![0,u32::try_from(ordinal).map_err(|_|limit("original call region formal exceeds its complete domain"))?]}else{let role=self.call_head_roles.get(&id).ok_or_else(||bad("original Call input has no exact fresh role directory"))?;b.spend(role.len())?;role.clone()};
            self.outer_inferences.entry(id).or_default();roles.push((id,role,formal.is_some()));
        }Ok(roles)
    }
    pub(super) fn finish_call_heads(&mut self,ids:&[(u64,Vec<u32>,bool)],node:NodeId,b:&mut Budget)->Check<()> {
        for (id,ordinal,formal) in ids {
            let Some(sources)=self.outer_inferences.remove(id)else{continue;};
            if sources.is_empty(){continue;}
            let target=self.resolved_life(&Life::Infer(*id),b)?;
            if matches!(target,Life::Infer(_)) {
                let mut unique=Vec::new();for source in sources{let source=self.resolved_life(&source,b)?;b.spend(unique.len().saturating_add(1))?;if !unique.contains(&source){unique.push(source);}}
                let first=unique.first().ok_or_else(||bad("original Call outer role has no inducing source"))?.clone();
                let mut equal=true;for source in &unique{if !self.same_life(source,&first,b)?{equal=false;}}
                // Static is an actual source obligation, not a value in the
                // variable Life domain. An inferred formal still denotes this
                // call's original region when all of its sources are Static.
                if equal&&(!*formal||!matches!(first,Life::Static|Life::Local(..)|Life::CallRegion(_))){self.life_substitutions.insert(*id,first);}
                else{let owner=self.current.ok_or_else(||bad("original Call region has no caller"))?;b.spend(ordinal.len())?;self.call_regions.insert(*id,(owner,node,ordinal.clone(),unique));self.life_substitutions.insert(*id,Life::CallRegion(*id));}
            }else{for source in sources{self.relate_outer(&source,&target,b)?;}}
        }Ok(())
    }
    pub(super) fn relate_outer(&mut self,actual:&Life,expected:&Life,b:&mut Budget)->Check<()> {
        let actual=self.resolved_life(actual,b)?;let expected=self.resolved_life(expected,b)?;
        if self.same_life(&actual,&expected,b)?{return Ok(());}
        if let Life::Infer(id)=expected{if let Some(sources)=self.outer_inferences.get_mut(&id){b.spend(sources.len().saturating_add(1))?;if !sources.contains(&actual){sources.push(actual);}return Ok(());}}
        if matches!(actual,Life::Infer(_))||matches!(expected,Life::Infer(_)){return self.unify_life(&actual,&expected,b);}
        if let Life::CallRegion(id)=expected {
            let owner=self.current.ok_or_else(||bad("original region flow has no caller"))?;
            let inherited=self.outer_outlives(&actual,&expected,&mut BTreeSet::new(),b)?;
            let region=self.call_regions.get_mut(&id).ok_or_else(||bad("original region has no complete Call sources"))?;
            if region.0!=owner{return Err(bad("original value cannot enlarge another declaration's Call region"));}
            b.spend(region.3.len().saturating_add(1))?;if !inherited&&!region.3.contains(&actual){region.3.push(actual.clone());}
        }else if !self.outer_outlives(&actual,&expected,&mut BTreeSet::new(),b)?{return Err(bad("original borrow value has no directed source relation to its required outer role"));}
        let owner=self.current.ok_or_else(||bad("original outer flow has no declaration"))?;
        let rows=self.outer_flows.entry(owner).or_default();b.spend(rows.len().saturating_add(1))?;
        if !rows.contains(&(actual.clone(),expected.clone())){rows.push((actual,expected));}Ok(())
    }
    pub(super) fn outer_outlives(&self,actual:&Life,expected:&Life,seen:&mut BTreeSet<Life>,b:&mut Budget)->Check<bool> {
        let expected=self.resolved_life(expected,b)?;
        if matches!(&expected,Life::Local(owner,_)if Some(*owner)==self.current){return Ok(true);}
        let mut pending=vec![self.resolved_life(actual,b)?];let mut graph=BTreeMap::<Life,Vec<Life>>::new();let mut grounded=BTreeSet::new();
        while let Some(source)=pending.pop(){
            b.spend(seen.len().checked_ilog2().map_or(1,|n|n as usize+2))?;
            if !seen.insert(source.clone()){continue;}
            if source==Life::Static||self.declared_outer_outlives(&source,&expected,b)?{grounded.insert(source);continue;}
            let dependencies=match &source{
                Life::Local(owner,node)=>self.borrow_parents.get(&(*owner,*node)).cloned().map(|source|vec![source]),
                Life::CallRegion(id)=>{let(_,_,_,sources)=self.call_regions.get(id).ok_or_else(||bad("original returned Call region has no sources"))?;b.spend(sources.len())?;Some(sources.clone())},
                _=>self.local_outer_sources(&source,b)?,
            };
            let Some(dependencies)=dependencies else{return Ok(false);};if dependencies.is_empty(){return Ok(false);}
            b.spend(dependencies.len().saturating_mul(2))?;let mut resolved=Vec::new();for dependency in dependencies{let dependency=self.resolved_life(&dependency,b)?;pending.push(dependency.clone());resolved.push(dependency);}
            graph.insert(source,resolved);
        }
        // Every reachable source must be justified. Cyclic local roles are
        // usable only when each reachable component has a genuine input/static
        // anchor; a disconnected source-only cycle cannot borrow another arm's.
        let mut changed=true;while changed{changed=false;for(source,dependencies)in &graph{b.spend(dependencies.len().saturating_add(1))?;if !grounded.contains(source)&&dependencies.iter().any(|source|grounded.contains(source)){grounded.insert(source.clone());changed=true;}}}
        b.spend(graph.len())?;Ok(graph.keys().all(|source|grounded.contains(source)))
    }
    fn declared_outer_outlives(&self,actual:&Life,expected:&Life,b:&mut Budget)->Check<bool>{
        let rows=self.current.and_then(|owner|self.life_outlives.get(&owner)).map(Vec::as_slice).unwrap_or(&[]);
        let mut pending=vec![actual.clone()];let mut seen=BTreeSet::new();
        while let Some(source)=pending.pop(){b.spend(rows.len().saturating_add(1))?;if !seen.insert(source.clone()){continue;}if self.same_life(&source,expected,b)?{return Ok(true);}for(longer,shorter)in rows{if self.same_life(&source,longer,b)?{pending.push(self.resolved_life(shorter,b)?);}}}Ok(false)
    }
    pub(super) fn remember_borrow_parent(&mut self,module:&TypedModule,owner:SemanticDefinitionId,node:NodeId,operand:NodeId,b:&mut Budget)->Check<()> {
        let(root,projections)=self.address_place(module,owner,operand,b)?;
        if matches!(root,intrinsics::AddressRoot::Place(node)|intrinsics::AddressRoot::Value(node)if node==operand)&&projections.is_empty(){return Ok(());}
        let ty=match root{intrinsics::AddressRoot::Place(node)|intrinsics::AddressRoot::Value(node)=>self.original_type(owner,&SemanticOriginalTypeSource::Expression(node),b)?,intrinsics::AddressRoot::Parameter(ordinal)=>self.original_type(owner,&SemanticOriginalTypeSource::Parameter(ordinal),b)?};
        if let Ty::Address{life,..}|Ty::Desc{life,..}=self.resolved(&ty,b)?{b.spend(1)?;self.borrow_parents.insert((owner,node),life);}Ok(())
    }
    pub(in crate::semantic) fn outer_flow_recipes(&self,module:&TypedModule,owner:SemanticDefinitionId,env:&EvalEnv,b:&mut Budget)->Check<Vec<(SemanticOuterLifeRecipe,SemanticLifeRecipe)>> {
        let mut result=Vec::new();
        if let Some(rows)=self.outer_flows.get(&owner){for(source,target)in rows{
            // These are directed outer dependencies, never exact equalities
            // or the intrinsic classes inside a complete child type.
            b.spend(1)?;result.push((self.reify_outer(module,source,env,b)?,self.reify_life(module,target,env,b)?));
        }}
        for(id,(definition,_,_,sources))in &self.call_regions{if *definition!=owner{continue;}
            let target=self.reify_life(module,&Life::CallRegion(*id),env,b)?;
            for source in sources{b.spend(1)?;result.push((self.reify_outer(module,source,env,b)?,target.clone()));}
        }
        Ok(result)
    }
}

impl CheckedOriginal {
    pub(super) fn check_outer_flows(&mut self,b:&mut Budget)->Check<()> {
        b.spend(self.outer_flows.values().fold(self.outer_flows.len(),|n,rows|n.saturating_add(rows.len().saturating_mul(2))))?;
        let previous=self.current;let rows=self.outer_flows.clone();
        let result=(||{for (owner,rows) in &rows{self.current=Some(*owner);for(source,target)in rows {
            b.spend(1)?;
            if matches!(target,Life::CallRegion(_)|Life::Local(..)){continue;}
            if !self.outer_outlives(source,target,&mut BTreeSet::new(),b)?{return Err(bad("original exported borrow role no longer covers every source after the complete body"));}
        }}Ok(())})();self.current=previous;result
    }
}

impl CheckedOriginal {
    pub(in crate::semantic) fn has_call_regions(&self,owner:SemanticDefinitionId,site:SemanticSiteId,b:&mut Budget)->Check<bool>{
        b.spend(self.call_regions.len())?;
        for(definition,node,_,_)in self.call_regions.values(){if *definition==owner&&self.original_call_site(owner,*node,b)?.1==site{return Ok(true);}}Ok(false)
    }
}

#[cfg(test)]
mod source_relation_tests {
    use super::*;
    fn owner()->SemanticDefinitionId{SemanticDefinitionId{module:zil_base::ModuleId(0),index:0}}
    fn address(life:Life,inner:Ty)->Ty{Ty::Address{mutable:false,life,inner:Box::new(inner)}}
    #[test]
    fn projected_borrow_keeps_its_rigid_source_and_invariant_nominal_child(){
        let mut checked=CheckedOriginal::default();checked.current=Some(owner());let mut b=Budget::new(&Limits::default());
        let a=Life::Formal(owner(),0);let z=Life::Formal(owner(),1);let projected=Life::Local(owner(),NodeId(3));
        checked.borrow_parents.insert((owner(),NodeId(3)),a.clone());
        let scalar=Ty::Atom(Type::I32);
        checked.relate_value(&address(projected.clone(),scalar.clone()),&address(a.clone(),scalar.clone()),&mut b).unwrap();
        assert!(checked.relate_value(&address(projected,scalar.clone()),&address(z.clone(),scalar),&mut b).is_err(),"&*x cannot acquire another rigid output lifetime");
        let nominal=|life:Life|Ty::Nominal(owner(),vec![Arg::Life(life)]);
        assert!(checked.relate_value(&nominal(a.clone()),&nominal(z.clone()),&mut b).is_err());
        assert!(checked.relate_value(&address(Life::Local(owner(),NodeId(4)),nominal(a)),&address(Life::Local(owner(),NodeId(5)),nominal(z)),&mut b).is_err(),"outer shortening cannot rename an intrinsic nominal argument");
    }
    #[test]
    fn common_call_region_checks_every_source_again_after_later_writes(){
        let mut checked=CheckedOriginal::default();checked.current=Some(owner());let mut b=Budget::new(&Limits::default());
        let a=Life::Formal(owner(),0);let z=Life::Formal(owner(),1);let inference=checked.fresh_life(&mut b).unwrap();let Life::Infer(id)=inference else{unreachable!()};
        checked.outer_inferences.insert(id,vec![a.clone(),z.clone()]);checked.finish_call_heads(&[(id,vec![0,0],true)],NodeId(7),&mut b).unwrap();
        let region=checked.resolved_life(&Life::Infer(id),&mut b).unwrap();assert!(matches!(region,Life::CallRegion(_)));
        assert!(!checked.same_life(&a,&z,&mut b).unwrap());
        assert!(checked.relate_outer(&region,&a,&mut b).is_err(),"choose(x:&a,y:&b) cannot promise &a without b:a");
        let local=Life::Local(owner(),NodeId(8));checked.borrow_parents.insert((owner(),NodeId(8)),a.clone());
        checked.call_regions.insert(99,(owner(),NodeId(9),vec![0,0],vec![local]));let region=Life::CallRegion(99);
        checked.relate_outer(&region,&a,&mut b).unwrap();checked.relate_outer(&z,&region,&mut b).unwrap();
        assert!(checked.check_outer_flows(&mut b).is_err(),"a later source must invalidate an earlier export that lacks its rigid bound");
    }
}

#[cfg(test)]
mod static_call_region_tests {
    use super::*;

    #[test]
    fn a_static_call_source_preserves_the_formal_region_without_exact_equality() {
        let limits=Limits::default();let mut b=Budget::new(&limits);
        let owner=SemanticDefinitionId{module:zil_base::ModuleId(0),index:0};
        let mut original=CheckedOriginal::default();original.current=Some(owner);
        let fresh=original.fresh_life(&mut b).unwrap();
        let Life::Infer(id)=fresh else{panic!("fresh call lifetime must be an inference role")};
        original.outer_inferences.insert(id,vec![]);
        original.relate_outer(&Life::Static,&fresh,&mut b).unwrap();
        original.finish_call_heads(&[(id,vec![0,2],true)],NodeId(7),&mut b).unwrap();
        let region=original.resolved_life(&fresh,&mut b).unwrap();
        assert_eq!(region,Life::CallRegion(id));
        let source=original.call_regions.get(&id).unwrap();
        assert_eq!(source,&(owner,NodeId(7),vec![0,2],vec![Life::Static]));
        let rigid=Life::Formal(owner,3);
        assert!(original.outer_outlives(&region,&rigid,&mut BTreeSet::new(),&mut b).unwrap());
        assert!(!original.same_life(&region,&Life::Static,&mut b).unwrap());
        assert!(!original.same_life(&region,&rigid,&mut b).unwrap());
    }
}
