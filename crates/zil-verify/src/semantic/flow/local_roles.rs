//! Directed regions proved from every original shared-local assignment. This
//! is an outer observation role; actual sources, loans and child κ stay with
//! the ordinary transfer/return kernels and never enter equality Γ.
use super::*;
impl CheckedContext{
    pub(super) fn prepare_local_roles(&self,module:&TypedModule,instance:&SemanticInstance,environment:&EvalEnv,b:&mut Budget)->Check<()>{
        let frame=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?;
        if frame.definition!=instance.definition{return Err(bad("shared local roles select another original function"));}
        let recipes=self.original.borrow().as_ref().ok_or_else(||bad("shared local roles lack their complete original programme"))?.local_role_recipes(module,instance.definition,environment,b)?;
        let mut edges=Vec::new();
        for role in recipes{
            let target=ClassKey::Implicit{scope:frame.key,ordinal:role.ordinal};
            for source in role.sources{
                if let SemanticOuterLifeRecipe::Class(source)=source{
                    let source=self.life(module,&source,environment,b)?;
                    b.spend(1)?;edges.push((source,target.clone()));
                }
            }
        }
        let flows=self.original.borrow().as_ref().ok_or_else(||bad("outer value flows lack their complete original programme"))?.outer_flow_recipes(module,instance.definition,environment,b)?;
        for(source,target)in flows{if let SemanticOuterLifeRecipe::Class(source)=source{let source=self.life(module,&source,environment,b)?;let target=self.life(module,&target,environment,b)?;b.spend(1)?;edges.push((source,target));}}
        if edges.is_empty(){return Ok(());}
        let mut premises=self.premises.borrow_mut();let rows=premises.entry(frame.key).or_default();let mut changed=false;
        for edge in edges{
            b.spend(rows.relations.outlives.len().saturating_add(1))?;
            if !rows.relations.outlives.contains(&edge){rows.relations.outlives.push(edge);changed=true;}
        }
        drop(premises);if changed{self.invalidate_closed_frames();}Ok(())
    }
    pub(super) fn prepare_receiver_role_flows(&self,module:&TypedModule,instance:&SemanticInstance,site:&SemanticSite,environment:&EvalEnv,b:&mut Budget)->Check<()> {
        let recipes=self.original.borrow().as_ref().ok_or_else(||bad("receiver flow lacks its complete original body"))?.receiver_role_flow_recipes(module,instance.definition,site.recipe,environment,b)?;
        let mut edges=Vec::new();
        for(source,target)in recipes{edges.push((self.class(module,&source,environment,b)?,self.life(module,&target,environment,b)?));}
        if edges.is_empty(){return Ok(());}
        let frame=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?;
        let mut premises=self.premises.borrow_mut();let rows=premises.entry(frame.key).or_default();let mut changed=false;
        for edge in edges{b.spend(rows.relations.outlives.len().saturating_add(1))?;if !rows.relations.outlives.contains(&edge){rows.relations.outlives.push(edge);changed=true;}}
        drop(premises);if changed{self.invalidate_closed_frames();}Ok(())
    }
    pub(super) fn local_role_value(&self,module:&TypedModule,actual:&CheckedType,expected:&CheckedType,environment:&EvalEnv,active:&mut ActiveQueries,b:&mut Budget)->Check<Option<CheckedType>>{
        if !matches!(expected.ty,Type::Address{mutable:false,..}){return Ok(None);}
        let frame=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?;
        let mut slots=expected.life.slots.iter().filter(|slot|slot.path.is_empty());
        let Some(slot)=slots.next()else{return Ok(None);};
        if slots.next().is_some()||slot.mode!=StoredBorrowMode::SharedAddress{return Err(bad("shared local role has another complete outer occurrence"));}
        let key=expected.classes.get(slot.binder as usize).ok_or_else(||bad("shared local role has no actual class"))?;
        let ClassKey::Implicit{scope,ordinal}=key else{return Ok(None);};
        if *scope!=frame.key||!self.original.borrow().as_ref().ok_or_else(||bad("shared local value lacks its original role proof"))?.has_local_role(frame.definition,*ordinal,b)?{return Ok(None);}
        // The complete invariant referent and all its original domains have
        // already passed compatible(). Rebuild only this shared outer role.
        if !self.compatible(environment,actual,expected,b)?{return Err(bad("shared local write violates its original directed region or complete child type"));}
        let child=self.child_value(module,actual,&[LifeTypeProjection::Pointee],b)?;
        let value=dynamic_witness::semantic_external::construct(self,environment,dynamic_witness::semantic_external::Constructor::Address{mode:StoredBorrowMode::SharedAddress,outer:Some(key.clone())},&[child],active,b)?;
        if !self.same_type(environment,&value,expected,b)?{return Err(bad("shared local normalization changes its child domain or intrinsic partition"));}
        Ok(Some(value))
    }
}
