//! Source-only binding of verified fixed metadata to its real lexical owner.
//! Names are spellings; definition identity and full original ordinal are keys.
use super::*;
impl<'a> Compiler<'a> {
    pub(super) fn source_life_scope(&mut self,definition:&CtSymbolId,span:Span)->R<Option<SourceLifeScope>> {
        for index in 0..self.declarations.len() {
            self.budget.work(1,span)?;
            let declaration=self.declarations[index];
            if self.symbol(declaration.module,declaration.item.span)?!=*definition {continue;}
            if span.file!=declaration.item.span.file||span.start<declaration.item.span.start||span.end>declaration.item.span.end {return Err(error(span,"scoped query is outside its real source definition"));}
            let mut binders=Vec::new();
            for (ordinal,parameter) in declaration.function.generics.iter().enumerate() {
                self.budget.node(parameter.span)?;
                if matches!(parameter.kind,s::GenericParamKind::Lifetime) {
                    self.budget.text(&parameter.name,parameter.span)?;
                    Budget::take(&mut self.budget.bytes,32,span,"CT lexical scope binding bytes exhausted")?;
                    binders.push((parameter.name.clone(),u32::try_from(ordinal).map_err(|_|error(span,"source binder ordinal overflow"))?));
                }
            }
            if binders.is_empty() {return Ok(None);}
            return Ok(Some(SourceLifeScope {definition:definition.clone(),span:declaration.item.span,binders}));
        }
        Ok(None)
    }
    pub(super) fn source_scoped_nominal(&mut self,module:usize,owner:usize,item:&'a s::Item,args:&[s::GenericArg],depth:usize,span:Span)->R<CtTy> {
        self.budget.depth(depth,span)?;
        let parameters=match &item.kind {s::ItemKind::Aggregate(a)=>&a.generics,s::ItemKind::Enum(e)=>&e.generics,_=>return Err(error(span,"scoped name is not a nominal declaration"))};
        if args.len()!=parameters.len() {return Err(error(span,"scoped nominal actual arity differs"));}
        let scope=self.source_life_scope.as_ref().ok_or_else(||error(span,"nominal lifetime has no actual lexical source owner"))?;
        if scope.span.file!=span.file||span.start<scope.span.start||span.end>scope.span.end {return Err(error(span,"nominal lifetime cannot capture another definition's scope"));}
        let mut actual_parameters=Vec::new();let mut nodes=Vec::new();let mut arguments=Vec::new();let mut rows=Vec::new();
        let symbol=self.symbol(owner,item.span)?;
        for (ordinal,(parameter,arg)) in parameters.iter().zip(args).enumerate() {
            self.budget.node(span)?;
            Budget::take(&mut self.budget.bytes,128,span,"CT scoped nominal arguments exhausted")?;
            match (&parameter.kind,arg) {
                (s::GenericParamKind::Lifetime,s::GenericArg::Lifetime(name))=>{
                    let scope=self.source_life_scope.as_ref().ok_or_else(||error(span,"nominal lifetime scope disappeared"))?;
                    let mut actual=None;
                    for (bound,ordinal) in scope.binders.iter().rev() {
                        self.budget.work(name.len().min(bound.len()).saturating_add(1),span)?;
                        if name==bound {actual=Some(*ordinal);break;}
                    }
                    let actual=actual.ok_or_else(||error(span,"nominal lifetime is not declared in the actual source definition"))?;
                    self.budget.work(actual_parameters.len()+1,span)?;
                    let binder=if let Some(index)=actual_parameters.iter().position(|old|*old==actual) {index as u32}else{actual_parameters.push(actual);(actual_parameters.len()-1) as u32};
                    arguments.push(CtTypeArgument::Lifetime(CtLifetimeArgument {owner:CtTypeNodeId(u32::MAX),binder:zil_ir::StorageBinder::Fixed(binder)}));
                    rows.push(zil_ir::NominalLifeArgument {path:vec![],declaration:zil_ir::LifeDeclarationId {module:symbol.module,declaration:symbol.declaration,compiler:false},argument:ordinal as u32,binder});
                }
                (s::GenericParamKind::Type(_),s::GenericArg::Type(ty))=>{
                    let graph=self.metadata_type(module,ty,depth+1)?;
                    self.metadata_copy(&graph,depth+1,span)?;
                    self.budget.metadata(&graph,span)?;
                    let offset=u32::try_from(nodes.len()).map_err(|_|error(span,"scoped graph ID overflow"))?;
                    for mut node in graph.nodes {
                        let mut invalid=false;
                        metadata::ids(&mut node,|id|if let Some(value)=id.0.checked_add(offset){id.0=value}else{invalid=true});
                        if invalid {return Err(error(span,"scoped graph relocation overflow"));}
                        nodes.push(node);
                    }
                    arguments.push(CtTypeArgument::Type(CtTypeNodeId(graph.root.0.checked_add(offset).ok_or_else(||error(span,"scoped root overflow"))?)));
                }
                (s::GenericParamKind::Const(_),arg)=>arguments.push(CtTypeArgument::Scalar(self.source_scalar_argument(parameter,arg,span)?)),
                _=>return Err(error(span,"scoped nominal actual kind differs from its original parameter")),
            }
        }
        let nominal=CtTypeNodeId(u32::try_from(nodes.len()).map_err(|_|error(span,"scoped nominal ID overflow"))?);
        let fixed=CtTypeNodeId(nominal.0.checked_add(1).ok_or_else(||error(span,"scoped owner ID overflow"))?);
        self.budget.work(arguments.len(),span)?;
        for argument in &mut arguments {if let CtTypeArgument::Lifetime(value)=argument {value.owner=fixed;}}
        self.budget.node(span)?;self.budget.node(span)?;
        Budget::take(&mut self.budget.bytes,256,span,"CT scoped graph root bytes exhausted")?;
        nodes.push(CtTypeNode::Nominal {symbol,arguments});
        nodes.push(CtTypeNode::BorrowedStorage {storage:nominal,life:zil_ir::StorageLifeShape {binder_count:actual_parameters.len() as u32,slots:vec![],retained:vec![],nominal_arguments:rows}});
        let context=self.meta_normalize(CtTypeRef {nodes,root:fixed},span)?;
        self.budget.metadata(&context,span)?;
        let CtTypeNode::BorrowedStorage {storage,..}=context.nodes[context.root.0 as usize] else {unreachable!()};
        let CtTypeNode::Nominal {arguments,..}=&context.nodes[storage.0 as usize] else {unreachable!()};
        let mut schema_args=Vec::new();
        for argument in arguments {
            self.budget.node(span)?;
            Budget::take(&mut self.budget.bytes,std::mem::size_of::<CtSchemaArgument>(),span,"CT scoped schema argument bytes exhausted")?;
            schema_args.push(match argument {
                CtTypeArgument::Type(id)=>CtSchemaArgument::TypeInContext(*id),
                CtTypeArgument::Scalar(value)=>CtSchemaArgument::Scalar(value.clone()),
                CtTypeArgument::Lifetime(value)=>CtSchemaArgument::Lifetime(*value),
            });
        }
        // Pay both the instance table and later private source-binding capture.
        self.budget.metadata(&context,span)?;
        self.budget.work(actual_parameters.len()+32,span)?;
        Budget::take(&mut self.budget.bytes,64+actual_parameters.len()*4,span,"CT scoped source binding bytes exhausted")?;
        self.closed_nominal_context(owner,item,schema_args,Some((context,actual_parameters)),depth+1,span)
    }
}
