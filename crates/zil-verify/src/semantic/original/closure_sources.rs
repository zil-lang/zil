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
        let index=self.source_index(module,&[definition],b)?;let function=index.definitions.get(&definition).ok_or_else(||bad("original Call result has no complete body index"))?;
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
        if self.injections.contains_key(&(definition,expression.id)){return Ok(None);}
        while let CoreExprKind::Group(inner)=&expression.kind{b.spend(1)?;expression=inner;}
        // A contextual TypeUse observes the injected sum. The source Call
        // remains its sole payload operation and has a different result role.
        if self.injections.contains_key(&(definition,expression.id)){return Ok(None);}
        if !matches!(expression.kind,CoreExprKind::Call{..}){return Ok(None);}
        let result=self.expressions.get(&(definition,expression.id)).ok_or_else(||bad("original Call result lacks its checked effective type"))?;
        // A later Describe occurrence observes the same manager through a new
        // outer head. Only the unchanged whole result role can reuse its Call
        // receipt; all adapted roles retain their own original recipe.
        if !self.same(&checked,result,b)?{return Ok(None);}
        let mut selected=None;
        for(site,node)in &function.nodes{b.spend(1)?;if matches!(node,index::Node::Expression(node)if node.id==expression.id)&&(self.call_targets.contains_key(&(definition,*site))||self.trait_calls.contains_key(&(definition,*site))||self.callable_nodes.contains_key(&(definition,expression.id))){if selected.replace(*site).is_some(){return Err(bad("original Call result has several source call roles"));}}}
        Ok(selected)
    }
    pub(in crate::semantic) fn original_address_source(&self,module:&TypedModule,definition:SemanticDefinitionId,source:&SemanticOriginalTypeSource,b:&mut Budget)->Check<Option<intrinsics::BufferSource>>{
        let SemanticOriginalTypeSource::Expression(node)=source else{return Ok(None);};
        let index=self.source_index(module,&[definition],b)?;let function=index.definitions.get(&definition).ok_or_else(||bad("original address has no complete body index"))?;
        let mut expression=*function.expressions.get(node).ok_or_else(||bad("original address expression is absent"))?;
        loop{b.spend(1)?;match &expression.kind{CoreExprKind::Group(value)|CoreExprKind::Ascribed{value,..}=>expression=value, _=>break}}
        let CoreExprKind::Unary{op:UnaryOp::Borrow|UnaryOp::BorrowMut,operand}=&expression.kind else{return Ok(None);};
        if !matches!(operand.kind,CoreExprKind::Name(_)){
            self.address_place(module,definition,operand.id,b)?;
            return Ok(Some(intrinsics::BufferSource::Address{definition,node:operand.id}));
        }
        let Some(place)=self.value_places.get(&(definition,operand.id))else{
            self.address_place(module,definition,operand.id,b)?;
            return Ok(Some(intrinsics::BufferSource::Address{definition,node:operand.id}));
        };
        if !place.fields.is_empty()||matches!(place.binder,Binder::Capture{..}|Binder::Pattern{..}){return Ok(Some(intrinsics::BufferSource::Place{definition,node:operand.id}));}
        Ok(Some(match &place.binder{Binder::Local{definition,node,..}=>intrinsics::BufferSource::Local{definition:*definition,node:*node},Binder::Parameter{definition,ordinal}=>intrinsics::BufferSource::Parameter{definition:*definition,ordinal:*ordinal},_=>return Err(unsupported("original address needs its complete captured/indirect Place boundary",None))}))
    }
    pub(in crate::semantic) fn ghost_capture_source(&self,module:&TypedModule,creator:SemanticDefinitionId,child:SemanticDefinitionId,ordinal:usize,node:NodeId,b:&mut Budget)->Check<intrinsics::BufferSource>{
        let index=self.source_index(module,&[creator,child],b)?;
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
        let root=match &place.binder{Binder::Local{definition:owner,node,..}if *owner==definition=>intrinsics::BufferRoot::Local{node:*node},Binder::Parameter{definition:owner,ordinal}if *owner==definition=>intrinsics::BufferRoot::Parameter{ordinal:*ordinal},Binder::Capture{definition:owner,..}if *owner==definition=>intrinsics::BufferRoot::Capture,Binder::Pattern{definition:owner,..}if *owner==definition=>intrinsics::BufferRoot::Pattern,_=>return Err(bad("original buffer observation uses another source scope or unsupported pattern root"))};
        Ok((root,place.fields.clone()))
    }
    pub(in crate::semantic) fn buffer_capture_source(&self,child:SemanticDefinitionId,node:NodeId,b:&mut Budget)->Check<intrinsics::BufferSource>{
        b.spend(1)?;let place=self.value_places.get(&(child,node)).ok_or_else(||bad("captured buffer has no independently checked original Place"))?;
        let Binder::Capture{definition,source}=&place.binder else{return Err(bad("captured buffer selects another original root category"));};
        if *definition!=child||!self.used.get(&child).is_some_and(|used|used.contains(&place.binder)){return Err(bad("captured buffer is not a checked original free-value use"));}
        source.charge_clone(b)?;
        match source.as_ref(){Binder::Local{definition,node,..}=>Ok(intrinsics::BufferSource::Local{definition:*definition,node:*node}),Binder::Parameter{definition,ordinal}=>Ok(intrinsics::BufferSource::Parameter{definition:*definition,ordinal:*ordinal}),_=>Err(unsupported("nested captured buffer needs its complete original creator source boundary",None))}
    }
    pub(in crate::semantic) fn closure_creation(&self,module:&TypedModule,definition:SemanticDefinitionId,source:&SemanticOriginalTypeSource,b:&mut Budget)->Check<Option<NodeId>> {
        if !matches!(self.original_type(definition,source,b)?,Ty::Closure(..)){return Ok(None);}
        let index=self.source_index(module,&[definition],b)?;
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
    pub(in crate::semantic) fn implicit_capture_binding(&self,module:&TypedModule,child:SemanticDefinitionId,ordinal:u32,b:&mut Budget)->Check<intrinsics::BufferSource> {
        if self.creator_moves.get(&child)!=Some(&false){return Err(bad("implicit captured head does not have its original default-borrow creation mode"));}
        let index=self.source_index(module,&[child],b)?;let function=index.definitions.get(&child).ok_or_else(||bad("implicit capture has no original child"))?;
        let implicit=function.definition.implicit_lifetimes.iter().find(|row|row.ordinal==ordinal).ok_or_else(||bad("implicit capture has no original lifetime occurrence"))?;
        let mut selected=None;
        for site in function.sites.iter().filter(|site|site.occurrence_path==implicit.occurrence_path){
            b.spend(1)?;let Some(index::Node::Expression(expression))=function.nodes.get(&site.id)else{continue;};
            let CoreExprKind::Name(name)=&expression.kind else{return Err(bad("implicit captured head does not select its exact original free Name occurrence"));};
            let binding=self.free_values.get(&child).and_then(|bindings|bindings.get(name)).ok_or_else(||bad("implicit capture Name is not an original lexical free value"))?;
            let captured=Binder::Capture{definition:child,source:Box::new(binding.identity.clone())};
            if self.value_bindings.get(&(child,expression.id))!=Some(&captured)||!self.used.get(&child).is_some_and(|used|used.contains(&captured)){return Err(bad("implicit capture occurrence does not name its checked original free binder"));}
            // occurrence_path is checked against the original AST by index.
            // Its lexicographic first Name use is a source position, not a
            // minimum arbitrary NodeId or a selected same-typed capture.
            let mut first=None;
            for candidate in function.sites{
                b.spend(1)?;let Some(index::Node::Expression(read))=function.nodes.get(&candidate.id)else{continue;};
                if !matches!(read.kind,CoreExprKind::Name(_))||self.value_bindings.get(&(child,read.id))!=Some(&captured){continue;}
                if first.as_ref().is_none_or(|path:&Vec<u32>|candidate.occurrence_path<*path){first=Some(candidate.occurrence_path.clone());}
            }
            if first.as_ref()!=Some(&implicit.occurrence_path){return Err(bad("implicit capture head is not its original first lexical free-value use"));}
            let source=match &binding.identity{Binder::Local{definition,node,..}=>intrinsics::BufferSource::Local{definition:*definition,node:*node},Binder::Parameter{definition,ordinal}=>intrinsics::BufferSource::Parameter{definition:*definition,ordinal:*ordinal},_=>return Err(unsupported("nested implicit capture needs its complete original parent capture source",None))};
            if selected.replace(source).is_some_and(|old|old!=source){return Err(bad("implicit capture selects several original source binders"));}
        }
        selected.ok_or_else(||bad("implicit captured head has no exact original first free-value use"))
    }

}
impl CheckedOriginal {
    pub(in crate::semantic) fn static_literal(&self,module:&TypedModule,definition:SemanticDefinitionId,site:SemanticSiteId,b:&mut Budget)->Check<Option<Vec<u8>>>{
        let index=self.source_index(module,&[definition],b)?;let function=index.definitions.get(&definition).ok_or_else(||bad("static literal original body is absent"))?;
        let expression=match function.nodes.get(&site).copied(){Some(index::Node::Expression(expression))=>expression,Some(index::Node::Statement(statement))=>match &statement.kind{CoreStatementKind::Binding{initializer:Some(expression),..}=>expression,_=>return Ok(None)},None=>return Ok(None)};
        self.checked_static_literal(definition,expression,b)
    }
    pub(in crate::semantic) fn static_call_literals(&self,module:&TypedModule,definition:SemanticDefinitionId,site:SemanticSiteId,receiver_argument:bool,b:&mut Budget)->Check<Option<(usize,Vec<(usize,Vec<u8>)>)>>{
        let index=self.source_index(module,&[definition],b)?;let function=index.definitions.get(&definition).ok_or_else(||bad("static Call literal has no exact original body"))?;
        let Some(index::Node::Expression(call))=function.nodes.get(&site).copied()else{return Ok(None);};
        let CoreExprKind::Call{arguments,..}=&call.kind else{return Ok(None);};
        if !self.call_targets.contains_key(&(definition,site))&&!self.trait_calls.contains_key(&(definition,site))&&!self.callable_nodes.contains_key(&(definition,call.id)){return Ok(None);}
        let offset=usize::from(receiver_argument&&self.trait_calls.get(&(definition,site)).is_some_and(|row|row.receiver_mode.is_some()));
        let mut literals=Vec::new();for(ordinal,argument)in arguments.iter().enumerate(){if let Some(bytes)=self.checked_static_literal(definition,argument,b)?{literals.push((ordinal+offset,bytes));}}
        Ok(Some((arguments.len()+offset,literals)))
    }
    fn checked_static_literal(&self,definition:SemanticDefinitionId,mut expression:&SemanticOriginalExpr,b:&mut Budget)->Check<Option<Vec<u8>>>{
        loop{b.spend(1)?;match &expression.kind{CoreExprKind::Group(inner)|CoreExprKind::Ascribed{value:inner,..}=>expression=inner,_=>break}}
        let CoreExprKind::String(text)=&expression.kind else{return Ok(None);};
        let ty=if let Some((_,payload))=self.injections.get(&(definition,expression.id)){payload.clone()}else{self.original_type(definition,&SemanticOriginalTypeSource::Expression(expression.id),b)?};
        if !matches!(self.resolved(&ty,b)?,Ty::Address{mutable:false,life:Life::Static,ref inner}if matches!(inner.as_ref(),Ty::Slice(child)if child.as_ref()==&Ty::Atom(Type::U8))){return Err(bad("original static literal changes its complete byte-slice role"));}
        b.spend(text.len().saturating_mul(2))?;
        let invalid=||bad("original static string has an invalid scalar/escape spelling");
        let mut chars=text.strip_prefix('"').and_then(|text|text.strip_suffix('"')).ok_or_else(invalid)?.chars();
        let mut bytes=Vec::new();
        while let Some(mut ch)=chars.next(){
            if ch=='\\'{ch=match chars.next().ok_or_else(invalid)?{
                'n'=>'\n','r'=>'\r','t'=>'\t','0'=>'\0','\\'=>'\\','\''=>'\'','"'=>'"',
                'u'=>{if chars.next()!=Some('{'){return Err(invalid());}let mut digits=0;let mut scalar=0u32;loop{let ch=chars.next().ok_or_else(invalid)?;if ch=='}'{if digits==0{return Err(invalid());}break;}if digits==6||!ch.is_ascii_hexdigit(){return Err(invalid());}scalar=scalar*16+ch.to_digit(16).ok_or_else(invalid)?;digits+=1;}char::from_u32(scalar).ok_or_else(invalid)?},
                _=>return Err(invalid()),
            };}else if matches!(ch,'"'|'\n'|'\r'){return Err(invalid());}
            let mut encoded=[0;4];bytes.extend_from_slice(ch.encode_utf8(&mut encoded).as_bytes());
        }
        b.spend(bytes.len())?;Ok(Some(bytes))
    }
}
