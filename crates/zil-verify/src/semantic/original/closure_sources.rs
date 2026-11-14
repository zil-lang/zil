//! Original value identities used at an actual closure creation. Type equality
//! and equal outside lifetime images never choose the captured value.
use super::*;
impl CheckedOriginal {
    // Select a proved original call by its lexical node or its inferred local
    // binding. Explicit type ascriptions remain separate invariant obligations.
    pub(in crate::semantic) fn call_result_source(&self,module:&TypedModule,definition:SemanticDefinitionId,source:&SemanticOriginalTypeSource,b:&mut Budget)->Check<Option<SemanticSiteId>>{
        if !matches!(source,SemanticOriginalTypeSource::Binding(_)|SemanticOriginalTypeSource::Expression(_)|SemanticOriginalTypeSource::NameAt{..}){return Ok(None);}
        let checked=self.original_type(definition,source,b)?;
        // These forms already reuse a complete original type argument or have
        // no fresh class construction whose domain could be compressed.
        if matches!(checked,Ty::Atom(_)|Ty::SealedError(_)|Ty::Variable(..)|Ty::SelfType(_)|Ty::Function(_)){return Ok(None);}
        let index=index::build(module,b)?;let function=index.definitions.get(&definition).ok_or_else(||bad("original Call result has no complete body index"))?;
        let binder=match source{
            SemanticOriginalTypeSource::Binding(node)=>Some(*node),
            SemanticOriginalTypeSource::Expression(node)=>match self.value_bindings.get(&(definition,*node)){Some(Binder::Local{definition:owner,node,..})if *owner==definition=>Some(*node),_=>None},
            SemanticOriginalTypeSource::NameAt{at,name}=>{let(kind,node)=match at{SemanticOriginalNode::Expression(node)=>(false,*node),SemanticOriginalNode::Statement(node)=>(true,*node)};b.spend(name.len())?;match self.name_binders_at.get(&(definition,kind,node,name.clone())){Some(Binder::Local{definition:owner,node,..})if *owner==definition=>Some(*node),_=>None}},
            _=>None,
        };
        let mut expression=if let Some(node)=binder{
            let statement=function.bindings.get(&node).ok_or_else(||bad("original Call result binding is absent"))?;
            let CoreStatementKind::Binding{ty:None,initializer:Some(initializer),..}=&statement.kind else{return Ok(None);};initializer
        }else if let SemanticOriginalTypeSource::Expression(node)=source{let Some(expression)=function.expressions.get(node)else{return Ok(None);};*expression}else{return Ok(None);};
        while let CoreExprKind::Group(inner)=&expression.kind{b.spend(1)?;expression=inner;}
        if !matches!(expression.kind,CoreExprKind::Call{..}){return Ok(None);}
        let mut selected=None;
        for(site,node)in &function.nodes{b.spend(1)?;if matches!(node,index::Node::Expression(node)if node.id==expression.id)&&(self.call_targets.contains_key(&(definition,*site))||self.trait_calls.contains_key(&(definition,*site))){if selected.replace(*site).is_some(){return Err(bad("original Call result has several source call roles"));}}}
        Ok(selected)
    }
    pub(in crate::semantic) fn original_address_source(&self,module:&TypedModule,definition:SemanticDefinitionId,source:&SemanticOriginalTypeSource,b:&mut Budget)->Check<Option<intrinsics::BufferSource>>{
        let SemanticOriginalTypeSource::Expression(node)=source else{return Ok(None);};
        let index=index::build(module,b)?;let function=index.definitions.get(&definition).ok_or_else(||bad("original address has no complete body index"))?;
        let mut expression=*function.expressions.get(node).ok_or_else(||bad("original address expression is absent"))?;
        loop{b.spend(1)?;match &expression.kind{CoreExprKind::Group(value)|CoreExprKind::Ascribed{value,..}=>expression=value, _=>break}}
        let CoreExprKind::Unary{op:UnaryOp::Borrow|UnaryOp::BorrowMut,operand}=&expression.kind else{return Ok(None);};
        let place=self.value_places.get(&(definition,operand.id)).ok_or_else(||unsupported("original address needs its complete checked source Place",None))?;
        if !place.fields.is_empty(){return Ok(Some(intrinsics::BufferSource::Place{definition,node:operand.id}));}
        Ok(Some(match &place.binder{Binder::Local{definition,node,..}=>intrinsics::BufferSource::Local{definition:*definition,node:*node},Binder::Parameter{definition,ordinal}=>intrinsics::BufferSource::Parameter{definition:*definition,ordinal:*ordinal},_=>return Err(unsupported("original address needs its complete captured/indirect Place boundary",None))}))
    }
    pub(in crate::semantic) fn ghost_capture_source(&self,module:&TypedModule,creator:SemanticDefinitionId,child:SemanticDefinitionId,ordinal:usize,node:NodeId,b:&mut Budget)->Check<intrinsics::BufferSource>{
        let index=index::build(module,b)?;
        let parent=index.definitions.get(&creator).ok_or_else(||bad("logical capture has no original creator body"))?;
        let function=index.definitions.get(&child).ok_or_else(||bad("logical capture has no original child body"))?;
        if function.definition.parent!=Some(creator)||parent.closures.get(&node)!=Some(&child){return Err(bad("logical capture environment belongs to another original creation"));}
        let formal=function.definition.environment.get(ordinal).ok_or_else(||bad("logical capture environment ordinal is absent"))?;
        let SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType{scope:SemanticScopeRef::Ancestor(0),source:SemanticOriginalTypeSource::NameAt{at:SemanticOriginalNode::Expression(at),name}})=&formal.source else{return Err(unsupported("logical value capture needs its exact original parent NameAt source",None));};
        if *at!=node{return Err(bad("logical capture source selects another original creation point"));}
        let binding=self.free_values.get(&child).and_then(|rows|rows.get(name)).ok_or_else(||bad("logical capture name has no checked original creator binding"))?;
        binding.identity.charge_clone(b)?;
        let captured=Binder::Capture{definition:child,source:Box::new(binding.identity.clone())};
        if !self.used.get(&child).is_some_and(|rows|rows.contains(&captured)){return Err(bad("physical logical capture is not a checked original free-value use"));}
        match &binding.identity{Binder::Local{definition,node,..}=>Ok(intrinsics::BufferSource::Local{definition:*definition,node:*node}),Binder::Parameter{definition,ordinal}=>Ok(intrinsics::BufferSource::Parameter{definition:*definition,ordinal:*ordinal}),_=>Err(unsupported("nested logical capture needs its complete parent capture transport",None))}
    }
    pub(in crate::semantic) fn buffer_place(&self,definition:SemanticDefinitionId,node:NodeId,b:&mut Budget)->Check<(intrinsics::BufferRoot,Vec<String>)>{
        b.spend(1)?;let place=self.value_places.get(&(definition,node)).ok_or_else(||bad("original buffer observation has no complete checked Place"))?;
        b.spend(place.fields.iter().fold(2usize,|n,field|n.saturating_add(field.len()).saturating_add(1)))?;
        let root=match &place.binder{Binder::Local{definition:owner,node,..}if *owner==definition=>intrinsics::BufferRoot::Local{node:*node},Binder::Parameter{definition:owner,ordinal}if *owner==definition=>intrinsics::BufferRoot::Parameter{ordinal:*ordinal},Binder::Capture{definition:owner,..}if *owner==definition=>intrinsics::BufferRoot::Capture,_=>return Err(bad("original buffer observation uses another source scope or unsupported pattern root"))};
        Ok((root,place.fields.clone()))
    }
    pub(in crate::semantic) fn buffer_capture_matches(&self,definition:SemanticDefinitionId,place:NodeId,read:NodeId,b:&mut Budget)->Check<bool>{
        b.spend(2)?;let place=self.value_places.get(&(definition,place)).ok_or_else(||bad("original buffer source Place is absent"))?;
        if !matches!(&place.binder,Binder::Capture{definition:owner,..}if *owner==definition){return Ok(false);}
        place.binder.charge_clone(b)?;Ok(self.value_bindings.get(&(definition,read))==Some(&place.binder))
    }
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
