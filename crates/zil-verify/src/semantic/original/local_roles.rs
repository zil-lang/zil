//! A mutable shared-address local has one lexical outer role. Every original
//! initializer/assignment contributes a directed dependency; none establishes
//! equality between independent inputs or changes the referent's full type.
use super::*;
pub(super) struct LocalRole{pub(super) ordinal:u32,pub(super) sources:BTreeSet<Life>}
pub(in crate::semantic) struct LocalRoleRecipe{pub(in crate::semantic) ordinal:u32,pub(in crate::semantic) sources:Vec<SemanticOuterLifeRecipe>}
impl CheckedOriginal{
    fn local_role_ordinal(&self,module:&TypedModule,context:&CheckedContext,owner:SemanticDefinitionId,node:NodeId,b:&mut Budget)->Check<u32>{
        let definition=context.definition(module,owner,b)?;
        let SemanticDefinitionBody::FunctionWithBody{sites,site_nodes,..}=&definition.body else{return Err(bad("shared local role has no original function body"));};
        b.spend(site_nodes.len().saturating_add(sites.len()))?;
        let mut found=BTreeSet::new();
        for row in site_nodes.iter().filter(|row|row.node==SemanticOriginalNode::Statement(node)){
            if let Some(site)=sites.iter().find(|site|site.id==row.site&&matches!(site.kind,SemanticSiteRecipeKind::DeclareLocal{..})){
                for implicit in &definition.implicit_lifetimes{b.spend(implicit.occurrence_path.len().saturating_add(1))?;if implicit.occurrence_path==site.occurrence_path{found.insert(implicit.ordinal);}}
            }
        }
        if found.len()!=1{return Err(bad("shared local role has no unique original declaration occurrence"));}
        Ok(*found.iter().next().unwrap())
    }
    pub(super) fn introduce_shared_local(&mut self,module:&TypedModule,context:&CheckedContext,node:NodeId,actual:&Ty,env:&Env,b:&mut Budget)->Check<Ty>{
        let actual=self.resolved(actual,b)?;
        let Ty::Address{mutable:false,life,inner}=actual else{return Ok(actual);};
        let owner=env.current()?;let ordinal=self.local_role_ordinal(module,context,owner,node,b)?;
        let source=self.resolved_life(&life,b)?;
        if matches!(source,Life::Infer(_)){return Err(bad("shared local initializer has an unresolved original lifetime"));}
        b.spend(self.local_roles.len().checked_ilog2().map_or(1,|n|n as usize+2).saturating_add(2))?;
        if !self.local_roles.contains_key(&(owner,node))&&self.local_roles.len()>=b.limits.max_nodes{return Err(limit("original shared local role directory exceeds selected bound"));}
        let role=self.local_roles.entry((owner,node)).or_insert_with(||LocalRole{ordinal,sources:BTreeSet::new()});
        if role.ordinal!=ordinal{return Err(bad("shared local changes its original lifetime role"));}
        b.spend(role.sources.len().saturating_add(1))?;role.sources.insert(source);
        Ok(Ty::Address{mutable:false,life:Life::Implicit(owner,ordinal),inner})
    }
    pub(super) fn shared_local_assignment(&mut self,module:&TypedModule,context:&CheckedContext,target:&SemanticOriginalExpr,value:&SemanticOriginalExpr,expected:&Ty,place:Option<&Place>,env:&Env,bindings:&BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<bool>{
        let Some(Place{binder:Binder::Local{definition,node,..},fields})=place else{return Ok(false);};
        if !fields.is_empty()||*definition!=env.current()?{return Ok(false);}
        let Some(role)=self.local_roles.get(&(*definition,*node))else{return Ok(false);};let ordinal=role.ordinal;
        b.spend(self.value_places.len().checked_ilog2().map_or(1,|n|n as usize+2))?;
        if self.value_places.get(&(*definition,target.id))!=place{return Err(bad("shared local assignment loses its exact original target Place"));}
        let Ty::Address{mutable:false,life,inner}=self.resolved(expected,b)? else{return Err(bad("shared local role changed its original address category"));};
        if self.resolved_life(&life,b)?!=Life::Implicit(*definition,ordinal){return Err(bad("shared local assignment selects another lexical lifetime role"));}
        let contextual=Ty::Address{mutable:false,life:self.fresh_life(b)?,inner};
        let actual=self.expression(module,context,value,env,bindings,Some(&contextual),active,b)?;
        let Ty::Address{mutable:false,life:source,inner:actual_inner}=self.resolved(&actual,b)?else{return Err(bad("shared local assignment has another actual address mode"));};
        let Ty::Address{inner:expected_inner,..}=expected else{return Err(bad("shared local expected type was not its original address"));};
        self.unify(&actual_inner,expected_inner,b)?;
        let source=self.resolved_life(&source,b)?;if matches!(source,Life::Infer(_)){return Err(bad("shared local assignment has an unresolved original lifetime"));}let role=self.local_roles.get_mut(&(*definition,*node)).unwrap();
        b.spend(role.sources.len().saturating_add(1))?;role.sources.insert(source);
        Ok(true)
    }
    pub(in crate::semantic) fn local_role_recipes(&self,module:&TypedModule,definition:SemanticDefinitionId,env:&EvalEnv,b:&mut Budget)->Check<Vec<LocalRoleRecipe>>{
        if !self.checked.contains(&definition){return Err(bad("shared local roles lack their completed original body"));}
        let mut rows=Vec::new();
        for((owner,_),role)in &self.local_roles{b.spend(1)?;if *owner!=definition{continue;}
            let mut sources=Vec::new();for source in &role.sources{if matches!(self.resolved_life(source,b)?,Life::ReceiverRole(..)){continue;}b.spend(1)?;sources.push(if *source==Life::Static{SemanticOuterLifeRecipe::Static}else{SemanticOuterLifeRecipe::Class(self.reify_life(module,source,env,b)?)});}
            rows.push(LocalRoleRecipe{ordinal:role.ordinal,sources});
        }Ok(rows)
    }
    pub(in crate::semantic) fn has_local_role(&self,definition:SemanticDefinitionId,ordinal:u32,b:&mut Budget)->Check<bool>{
        for((owner,_),role)in &self.local_roles{b.spend(1)?;if *owner==definition&&role.ordinal==ordinal{return Ok(true);}}Ok(false)
    }
}

impl CheckedOriginal {
    pub(super) fn local_outer_sources(&self,life:&Life,b:&mut Budget)->Check<Option<Vec<Life>>>{
        let Life::Implicit(owner,ordinal)=life else{return Ok(None);};
        for((definition,_),role)in &self.local_roles{b.spend(1)?;if definition==owner&&role.ordinal==*ordinal{b.spend(role.sources.len())?;return Ok(Some(role.sources.iter().cloned().collect()));}}Ok(None)
    }
}
