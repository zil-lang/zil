//! Original integer iteration binders retain their exact source occurrence.
use super::*;
pub(in crate::semantic) struct RangeBinding {
    pub(in crate::semantic) statement:NodeId,
    pub(in crate::semantic) name:String,
    pub(in crate::semantic) span:Span,
    pub(in crate::semantic) start:NodeId,
    pub(in crate::semantic) end:NodeId,
}
impl CheckedOriginal {
    pub(in crate::semantic) fn range_statement(&self,module:&TypedModule,definition:SemanticDefinitionId,statement:NodeId,b:&mut Budget)->Check<Option<RangeBinding>> {
        if !self.checked.contains(&definition){return Err(bad("range source owner has no complete original proof"));}
        // Ordinary Binding/TupleBinding entries already have their checked
        // original declaration transport. Do not rebuild a function cursor
        // for each ordinary scalar/manager use merely to exclude a loop.
        b.spend(self.bindings.len().checked_ilog2().map_or(1,|depth|depth as usize+1))?;
        if self.bindings.contains_key(&(definition,statement)){return Ok(None);}
        let index=self.source_index(module,&[definition],b)?;
        let function=index.definitions.get(&definition).ok_or_else(||bad("range source original owner is absent"))?;
        let Some(statement)=function.statements.get(&statement)else{return Ok(None);};
        let(name,start,end)=match &statement.kind {
            CoreStatementKind::ForRange{name,start,end,..}=>(name,start,end),
            CoreStatementKind::ForEach{pattern,iterator,..}=>{
                let CorePatternKind::Name(name)=&pattern.kind else{return Ok(None);};
                let mut iterator=iterator;while let CoreExprKind::Group(inner)=&iterator.kind{b.spend(1)?;iterator=inner;}
                let CoreExprKind::Range{start,end}=&iterator.kind else{return Ok(None);};
                (name,start.as_ref(),end.as_ref())
            },
            _=>return Ok(None),
        };
        b.spend(name.len().saturating_add(1))?;
        Ok(Some(RangeBinding{statement:statement.id,name:name.clone(),span:statement.span,start:start.id,end:end.id}))
    }
    pub(in crate::semantic) fn range_name(&self,module:&TypedModule,definition:SemanticDefinitionId,node:NodeId,b:&mut Budget)->Check<Option<RangeBinding>> {
        let Some(place)=self.value_places.get(&(definition,node))else{return Ok(None);};
        let statement=match &place.binder {
            Binder::Local{definition:owner,node,..}if *owner==definition=>*node,
            Binder::Pattern{definition:owner,..}if *owner==definition=>{
                let mut found=None;
                for((scope,statement,path),binder)in &self.pattern_bindings {
                    b.spend(path.len().saturating_add(1))?;
                    if *scope==definition&&binder==&place.binder {
                        if !path.is_empty(){return Ok(None);}
                        if found.replace(*statement).is_some(){return Err(bad("range pattern has several original binder occurrences"));}
                    }
                }
                let Some(statement)=found else{return Ok(None);};statement
            },
            _=>return Ok(None),
        };
        self.range_statement(module,definition,statement,b)
    }
}
