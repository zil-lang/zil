//! Original value identities used at an actual closure creation. Type equality
//! and equal outside lifetime images never choose the captured value.
use super::*;
impl CheckedOriginal {
    pub(in crate::semantic) fn capture_read_source(&self,child:SemanticDefinitionId,node:NodeId,b:&mut Budget)->Check<intrinsics::BufferSource>{
        b.spend(1)?;let Some(Binder::Capture{definition,source})=self.value_bindings.get(&(child,node)) else{return Err(bad("actual capture read has no original sealed free binder"));};
        if *definition!=child{return Err(bad("actual capture read selects another original child"));}
        match source.as_ref(){Binder::Local{definition,node,..}=>Ok(intrinsics::BufferSource::Local{definition:*definition,node:*node}),Binder::Parameter{definition,ordinal}=>Ok(intrinsics::BufferSource::Parameter{definition:*definition,ordinal:*ordinal}),_=>Err(unsupported("nested captured owner needs its complete original parent capture identity",None))}
    }
    pub(in crate::semantic) fn closure_creation(&self,module:&TypedModule,definition:SemanticDefinitionId,source:&SemanticOriginalTypeSource,b:&mut Budget)->Check<Option<NodeId>> {
        if !matches!(self.original_type(definition,source,b)?,Ty::Closure(..)){return Ok(None);}
        let index=index::build(module,b)?;
        let function=index.definitions.get(&definition).ok_or_else(||bad("closure source has no original function"))?;
        let binding=match source {
            SemanticOriginalTypeSource::Binding(node)=>Some(*node),
            SemanticOriginalTypeSource::Expression(node)=>{
                if function.closures.contains_key(node){return Ok(Some(*node));}
                match self.value_bindings.get(&(definition,*node)) {
                    Some(Binder::Local{definition:owner,node,..}) if *owner==definition=>Some(*node),
                    _=>return Ok(None),
                }
            }
            _=>return Ok(None),
        };
        let statement=function.bindings.get(&binding.unwrap()).ok_or_else(||bad("original closure source binding is absent"))?;
        let CoreStatementKind::Binding{initializer:Some(initializer),..}=&statement.kind else {return Err(unsupported("closure source needs its complete original initializer projection",None));};
        if !function.closures.contains_key(&initializer.id){return Ok(None);}
        Ok(Some(initializer.id))
    }
    pub(in crate::semantic) fn implicit_capture_binding(&self,module:&TypedModule,child:SemanticDefinitionId,ordinal:u32,b:&mut Budget)->Check<(SemanticDefinitionId,NodeId)> {
        let index=index::build(module,b)?;
        let function=index.definitions.get(&child).ok_or_else(||bad("implicit capture has no original child"))?;
        let implicit=function.definition.implicit_lifetimes.iter().find(|row|row.ordinal==ordinal).ok_or_else(||bad("implicit capture has no original lifetime occurrence"))?;
        let mut selected=None;
        for site in function.sites.iter().filter(|site|site.occurrence_path==implicit.occurrence_path) {
            b.spend(1)?;
            let Some(index::Node::Expression(expression))=function.nodes.get(&site.id) else {continue;};
            let CoreExprKind::Name(name)=&expression.kind else {return Err(unsupported("implicit capture needs its exact original free-value projection",None));};
            let binding=self.free_values.get(&child).and_then(|bindings|bindings.get(name)).ok_or_else(||bad("implicit capture name is not a sealed original free value"))?;
            let Binder::Local{definition,node,..}=&binding.identity else {return Err(unsupported("implicit capture parameter/outer capture needs its complete original source bridge",None));};
            let identity=(*definition,*node);
            if selected.replace(identity).is_some_and(|old|old!=identity){return Err(bad("implicit capture selects several original value binders"));}
        }
        selected.ok_or_else(||bad("implicit capture has no exact original free-value use"))
    }
}
