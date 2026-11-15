//! Match original observation Places to actual source projections. These
//! selectors prove identities only; address/description loans, initialization,
//! field debts and retirement remain obligations of the ordinary kernels.
use super::*;
use original::intrinsics::{AddressProjection,AddressRoot,BufferRoot,BufferSource};
use zil_base::NodeId;

impl CheckedContext{
    pub(super) fn address_place_source(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,value:ValueId,definition:SemanticDefinitionId,node:NodeId,b:&mut Budget)->Check<()>{
        // A borrowed product field may be observed as a descriptor either by
        // DescribeAddress of its Managed head, or by reading a stored Desc.
        // The raw producers and exact original ordered path below prove each
        // of these roles; no descriptor is promoted to an owning position.
        let observed=matches!(f.values.get(value.0 as usize),Some(Type::Desc(_)));
        self.original_projection_source(module,caller,f,value,definition,node,observed,b)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn original_projection_source(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,mut value:ValueId,definition:SemanticDefinitionId,node:NodeId,read_value:bool,b:&mut Budget)->Check<()>{
        if definition!=caller.definition{return Err(bad("indirect borrow selects another original lexical owner"));}
        let(root,projections)=self.original.borrow().as_ref().ok_or_else(||bad("indirect borrow lacks its complete original body proof"))?.address_place(module,definition,node,b)?;
        let mut seen=BTreeSet::new();
        let mut fused=None;
        for(ordinal,projection)in projections.iter().enumerate(){
            loop{
                b.spend(1)?;if !seen.insert(value){return Err(bad("indirect borrow has a cyclic actual projection chain"));}
                let(_,_,instruction)=producer(f,value,b)?;crate::check_instruction(module,f,instruction,b)?;
                if let InstructionKind::ReborrowAddress{address,..}=instruction.kind{value=address;continue;}
                if read_value&&let InstructionKind::ReadAddress{address}=instruction.kind{value=address;continue;}
                if read_value&&let InstructionKind::DescribeAddress{address}=instruction.kind{value=address;continue;}
                match (projection,&instruction.kind){
                    (AddressProjection::Field(name),InstructionKind::AddressField{address,index})=>{
                        let ty=f.values.get(address.0 as usize).ok_or_else(||bad("indirect field parent has no actual type"))?;let mut field=vec![name.clone()];consume_field(ty,*index,&mut field,b)?;value=*address;
                    },
                    (AddressProjection::Field(name),InstructionKind::BorrowPackageField{local,index,..})if ordinal+1==projections.len()=>{
                        let ty=&f.locals.get(local.0 as usize).ok_or_else(||bad("borrowed Package field has no complete actual parent"))?.ty;let mut field=vec![name.clone()];consume_field(ty,*index,&mut field,b)?;fused=Some(*local);
                    },
                    (AddressProjection::Field(name),InstructionKind::ReadPackageField{local,index})if read_value&&ordinal+1==projections.len()=>{
                        let ty=&f.locals.get(local.0 as usize).ok_or_else(||bad("observed Package field has no complete actual parent"))?.ty;let mut field=vec![name.clone()];consume_field(ty,*index,&mut field,b)?;fused=Some(*local);
                    },
                    (AddressProjection::Field(name),InstructionKind::ExtractField{aggregate,index})if read_value=>{
                        let ty=f.values.get(aggregate.0 as usize).ok_or_else(||bad("observed field aggregate is absent"))?;let mut field=vec![name.clone()];consume_field(ty,*index,&mut field,b)?;value=*aggregate;
                    },
                    (AddressProjection::Field(name),InstructionKind::ReadDesc{desc,index})if read_value=>{
                        let ty=f.values.get(desc.0 as usize).ok_or_else(||bad("observed field descriptor is absent"))?;let mut field=vec![name.clone()];consume_field(ty,*index,&mut field,b)?;value=*desc;
                    },
                    (AddressProjection::Index(node),InstructionKind::AddressIndex{address,index})=>{
                        self.exact_expression_value(module,caller,f,*index,*node,b)?;value=*address;
                    },
                    (AddressProjection::Index(node),InstructionKind::ExtractIndex{aggregate,index})if read_value=>{
                        self.exact_expression_value(module,caller,f,*index,*node,b)?;value=*aggregate;
                    },
                    (AddressProjection::Slice{start,end},InstructionKind::AddressSlice{address,start:actual_start,end:actual_end})=>{
                        for(original,actual)in[(start,actual_start),(end,actual_end)]{match(original,actual){(Some(node),Some(value))=>self.exact_expression_value(module,caller,f,*value,*node,b)?,(None,None)=>{},_=>return Err(bad("indirect slice changes an exact original endpoint occurrence")),}}
                        value=*address;
                    },
                    _=>return Err(bad("indirect borrow actual projection differs from its complete original ordered Place")),
                }break;
            }
        }
        if let Some(local)=fused{return match root{
            AddressRoot::Place(node)=>{let(root,fields)=self.original.borrow().as_ref().ok_or_else(||bad("borrowed Package field lacks its independent original parent Place"))?.buffer_place(definition,node,b)?;if !fields.is_empty(){return Err(bad("fused borrowed Package field omitted an original parent projection"));}self.projected_local_root(module,caller,f,local,root,node,b)},
            AddressRoot::Parameter(ordinal)=>{if self.buffer_source_local(module,caller,f,BufferSource::Parameter{definition,ordinal},b)?!=local{return Err(bad("borrowed receiver field selects another actual parameter"));}Ok(())},
            AddressRoot::Value(_)=>Err(bad("fused borrowed Package field replaced its original indirect parent")),
        };}
        match root{
            AddressRoot::Place(node)=>self.projected_buffer_source(module,caller,f,value,definition,node,b),
            AddressRoot::Value(node)=>self.exact_expression_value(module,caller,f,value,node,b),
            AddressRoot::Parameter(ordinal)=>self.intrinsic_buffer_source(module,caller,f,value,BufferSource::Parameter{definition,ordinal},b),
        }
    }
    pub(super) fn exact_expression_value(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,mut value:ValueId,node:NodeId,b:&mut Budget)->Check<()>{
        let definition=self.definition(module,caller.definition,b)?;
        let SemanticDefinitionBody::FunctionWithBody{site_nodes,..}=&definition.body else{return Err(bad("address source occurrence has no complete original body"));};
        let program=module.semantic.as_ref().ok_or_else(||bad("address source occurrence programme is absent"))?;
        let mut sources=BTreeSet::new();
        for original in site_nodes.iter().filter(|row|row.node==SemanticOriginalNode::Expression(node)){
            b.spend(program.sites.len())?;
            for site in program.sites.iter().filter(|site|site.instance==caller.id&&site.recipe==original.site){match site.action{
                SemanticSiteAction::TypeUse{subject:SemanticOperand::Value(value),..}|SemanticSiteAction::Construct{result:SemanticOperand::Value(value),..}=>{sources.insert(value);},
                SemanticSiteAction::BodyCall{..}|SemanticSiteAction::Call{..}=>{
                    let SemanticSiteAddress::Typed{block,instruction,..}=site.at else{return Err(bad("address input retains a nonactual original call"));};
                    if let Some(value)=f.blocks.get(block.0 as usize).and_then(|block|block.instructions.get(instruction as usize)).and_then(|instruction|instruction.result){sources.insert(value);}
                },_=>{},
            }}
        }
        if sources.is_empty(){return self.scalar_expression_source(module,caller,f,value,node,b);}
        let mut seen=BTreeSet::new();loop{
            b.spend(sources.len().saturating_add(1))?;
            if sources.contains(&value){return Ok(());}
            if !seen.insert(value){return Err(bad("address input has a cyclic actual reborrow chain"));}
            let(_,_,instruction)=producer(f,value,b)?;
            let InstructionKind::ReborrowAddress{address,..}=instruction.kind else{return Err(bad("address input selects another same-typed original expression value"));};
            crate::check_instruction(module,f,instruction,b)?;value=address;
        }
    }
    fn scalar_expression_source(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,value:ValueId,node:NodeId,b:&mut Budget)->Check<()>{
        let original=self.original.borrow();
        let(expression,carrier)=original.as_ref().ok_or_else(||bad("scalar address input lacks its complete independent original proof"))?.scalar_input(module,caller.definition,node,b)?;
        if f.values.get(value.0 as usize)!=Some(&carrier){return Err(bad("address scalar input changes its complete original carrier"));}
        let(_,_,instruction)=producer(f,value,b)?;crate::check_instruction(module,f,instruction,b)?;
        match(&expression.kind,&instruction.kind){
            (CoreExprKind::Group(inner)|CoreExprKind::Ascribed{value:inner,..}|CoreExprKind::CompileTime(inner),_)=>self.exact_expression_value(module,caller,f,value,inner.id,b),
            (CoreExprKind::Sequence{bindings,value:inner},_)if bindings.is_empty()=>self.exact_expression_value(module,caller,f,value,inner.id,b),
            (CoreExprKind::Name(_),_)=>self.projected_buffer_source(module,caller,f,value,caller.definition,node,b),
            (CoreExprKind::Unit,InstructionKind::Constant(Constant::Unit))=>Ok(()),
            (CoreExprKind::Bool(original),InstructionKind::Constant(Constant::Bool(actual)))if original==actual=>Ok(()),
            (CoreExprKind::Literal(original),InstructionKind::Constant(actual))if original==actual=>Ok(()),
            (CoreExprKind::Integer(text)|CoreExprKind::Float(text),InstructionKind::Constant(actual))=>{
                b.spend(text.len())?;let pattern=if matches!(expression.kind,CoreExprKind::Integer(_)){DynConstPattern::IntegerLiteral(text.clone())}else{DynConstPattern::FloatLiteral(text.clone())};
                let expected=scalar::closed(&pattern,&carrier,module.target.pointer_width,b)?.ok_or_else(||bad("address scalar literal retains an unresolved original value"))?;
                if &expected!=actual{return Err(bad("address scalar literal changes its exact original bits"));}Ok(())
            },
            (CoreExprKind::Unary{op:UnaryOp::Negate,operand},InstructionKind::Constant(actual))if matches!(operand.kind,CoreExprKind::Integer(_)|CoreExprKind::Float(_))=>{
                let pattern=match &operand.kind{CoreExprKind::Integer(text)=>{b.spend(text.len())?;DynConstPattern::IntegerLiteral(text.clone())},CoreExprKind::Float(text)=>{b.spend(text.len())?;DynConstPattern::FloatLiteral(text.clone())},_=>unreachable!()};
                let pattern=DynConstPattern::Unary{op:UnaryOp::Negate,ty:Box::new(DynTypePattern::Concrete(carrier.clone())),value:Box::new(pattern)};
                let expected=scalar::closed(&pattern,&carrier,module.target.pointer_width,b)?.ok_or_else(||bad("address signed scalar literal remains unresolved"))?;
                if &expected!=actual{return Err(bad("address signed literal changes its exact original bits"));}Ok(())
            },
            (CoreExprKind::Unary{op,operand},InstructionKind::Unary{op:actual,operand:input})if op==actual=>self.exact_expression_value(module,caller,f,*input,operand.id,b),
            (CoreExprKind::Binary{op,left,right},InstructionKind::Binary{op:actual,left:a,right:z})if op==actual=>{self.exact_expression_value(module,caller,f,*a,left.id,b)?;self.exact_expression_value(module,caller,f,*z,right.id,b)},
            (CoreExprKind::NumericConvert{value:input,..}|CoreExprKind::IntegerConvert{value:input,..},InstructionKind::ConvertInteger{value:actual,target}|InstructionKind::ConvertNumber{value:actual,target})if target==&carrier=>self.exact_expression_value(module,caller,f,*actual,input.id,b),
            (CoreExprKind::Member{..}|CoreExprKind::ReceiverMember(_)|CoreExprKind::Index{..},_)=>self.original_projection_source(module,caller,f,value,caller.definition,node,true,b),
            _=>Err(unsupported("address scalar input lacks its complete original operation/operand correspondence",Some(expression.span))),
        }
    }
    pub(super) fn projected_buffer_source(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,mut value:ValueId,definition:SemanticDefinitionId,node:NodeId,b:&mut Budget)->Check<()>{
        if definition!=caller.definition{return Err(bad("projected buffer observation uses another original scope"));}
        let (root,mut fields)=self.original.borrow().as_ref().ok_or_else(||bad("projected buffer has no original body proof"))?.buffer_place(definition,node,b)?;
        let mut seen=std::collections::BTreeSet::new();
        loop{
            b.spend(1)?;if !seen.insert(value){return Err(bad("projected buffer has a cyclic actual producer chain"));}
            let(block,index,instruction)=producer(f,value,b)?;
            match &instruction.kind{
                InstructionKind::ReborrowAddress{address,..}|InstructionKind::AddressSlice{address,..}|InstructionKind::DescribeAddress{address}=>value=*address,
                InstructionKind::AddressField{address,index}=>{let ty=f.values.get(address.0 as usize).ok_or_else(||bad("field address operand is absent"))?;consume_field(ty,*index,&mut fields,b)?;value=*address;},
                InstructionKind::ExtractField{aggregate,index}=>{let ty=f.values.get(aggregate.0 as usize).ok_or_else(||bad("field aggregate operand is absent"))?;consume_field(ty,*index,&mut fields,b)?;value=*aggregate;},
                InstructionKind::DescribePackageField{local,index}|InstructionKind::ReadPackageField{local,index}|InstructionKind::TakePackageField{local,index}|InstructionKind::BorrowPackageField{local,index,..}=>{
                    consume_field(&f.locals.get(local.0 as usize).ok_or_else(||bad("package source local is absent"))?.ty,*index,&mut fields,b)?;
                    if !fields.is_empty(){return Err(unsupported("nested package observation requires its actual local-to-parent projection ledger",Some(instruction.span)));}
                    return self.projected_local_root(module,caller,f,*local,root,node,b);
                },
                InstructionKind::DescribeLocal{local}|InstructionKind::Load(local)|InstructionKind::TakeLocal{local}|InstructionKind::BorrowLocal{local}|InstructionKind::BorrowLocalMut{local}=>{
                    if !fields.is_empty(){return Err(bad("actual buffer descriptor omitted an original field projection"));}
                    return self.projected_local_root(module,caller,f,*local,root,node,b);
                },
                InstructionKind::ReadCapture{index:capture}|InstructionKind::TakeCapture{index:capture}|InstructionKind::DescribeCapture{index:capture}|InstructionKind::BorrowCapture{index:capture,..}=>{
                    if !fields.is_empty()||!matches!(root,BufferRoot::Capture){return Err(bad("actual capture observation changes its original root or field path"));}
                    return self.buffer_capture_use(module,caller,f,*capture,node,block,index,instruction,b);
                },
                _=>return Err(unsupported("buffer observation lacks its complete actual projection/creator chain",Some(instruction.span))),
            }
        }
    }
    fn projected_local_root(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,local:LocalId,root:BufferRoot,node:NodeId,b:&mut Budget)->Check<()>{
        match root{
            BufferRoot::Local{node}=>if self.buffer_source_local(module,caller,f,BufferSource::Local{definition:caller.definition,node},b)?!=local{return Err(bad("field observation selects another original local"));},
            BufferRoot::Parameter{ordinal}=>if self.buffer_source_local(module,caller,f,BufferSource::Parameter{definition:caller.definition,ordinal},b)?!=local{return Err(bad("field observation selects another original parameter"));},
            BufferRoot::Capture=>{
                self.captured_local_alias(module,caller,f,local,node,b)?;
            },
            BufferRoot::Pattern=>self.pattern_local_root(module,caller,f,local,node,b)?,
        }Ok(())
    }
    fn captured_local_alias(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,local:LocalId,node:NodeId,b:&mut Budget)->Check<()>{
        // Every real store contributes to the source upper. A convenient last
        // store, matching Type or untrusted same-numbered capture is not proof.
        let mut work=vec![(true,local.0,false)];let mut active=BTreeSet::new();let mut proven=BTreeSet::new();let mut rooted=false;
        while let Some((local,index,exit))=work.pop(){
            b.spend(1)?;
            if exit{active.remove(&(local,index));proven.insert((local,index));continue;}
            if proven.contains(&(local,index)){continue;}
            if !active.insert((local,index)){return Err(bad("captured observation local/SSA alias has a cyclic source"));}
            work.push((local,index,true));
            if local{
                let local=LocalId(index);let mut binding=None;let mut stores=Vec::new();
                for block in &f.blocks{b.spend(block.instructions.len())?;for(at,instruction)in block.instructions.iter().enumerate(){match instruction.kind{
                    InstructionKind::BindPackageCapture{local:actual,index:capture}if actual==local=>{if binding.replace((block.id,at as u32,capture,instruction)).is_some(){return Err(bad("package capture view has several actual bindings"));}},
                    InstructionKind::Store{local:actual,value}if actual==local=>stores.push(value),_=>{},
                }}}
                if let Some((block,at,capture,instruction))=binding{
                    if !stores.is_empty(){return Err(bad("captured package view mixes environment binding with value replacement"));}
                    crate::packages::check_instruction(module,f,instruction,b)?;self.buffer_capture_use(module,caller,f,capture,node,block,at,instruction,b)?;rooted=true;
                }else{
                    if stores.is_empty(){return Err(bad("captured observation local has no complete actual environment source"));}
                    b.spend(stores.len())?;work.extend(stores.into_iter().map(|value|(false,value.0,false)));
                }
            }else{
                let(block,at,instruction)=producer(f,ValueId(index),b)?;
                match instruction.kind{
                    InstructionKind::ReadCapture{index:capture}|InstructionKind::TakeCapture{index:capture}|InstructionKind::DescribeCapture{index:capture}|InstructionKind::BorrowCapture{index:capture,..}=>{self.buffer_capture_use(module,caller,f,capture,node,block,at,instruction,b)?;rooted=true;},
                    InstructionKind::Load(local)|InstructionKind::TakeLocal{local}|InstructionKind::DescribeLocal{local}|InstructionKind::BorrowLocal{local}|InstructionKind::BorrowLocalMut{local}=>work.push((true,local.0,false)),
                    InstructionKind::ReborrowAddress{address,..}|InstructionKind::DescribeAddress{address}=>work.push((false,address.0,false)),
                    _=>return Err(unsupported("captured observation alias does not select a complete actual environment root",Some(instruction.span))),
                }
            }
        }
        if !rooted{return Err(bad("captured observation alias has no actual creator root"));}Ok(())
    }
    fn buffer_capture_use(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,capture:u32,node:NodeId,_block:BlockId,_index:u32,instruction:&Instruction,b:&mut Budget)->Check<()>{
        let environment=f.environment.ok_or_else(||bad("buffer capture observation has no actual closure environment"))?;
        let definition=crate::closures::definition(module,environment,Some(instruction.span))?;
        let expected=definition.captures.get(capture as usize).ok_or_else(||bad("buffer capture index is outside the actual environment"))?;
        dynamic_metadata::ty(expected,b)?;
        let occurrences=self.occurrences.borrow();let actual=occurrences.get(&(ScopeKey::Instance(caller.id),OperandKey::Capture(capture))).ok_or_else(||bad("buffer capture has no independently sealed actual creator operand"))?;
        if actual.ty!=*expected{return Err(bad("buffer capture actual type differs from its sealed creator operand"));}
        ghost_captures::complete_environment(module,actual,b)?;
        b.spend(actual.classes.len().saturating_add(actual.origins.len()).saturating_add(actual.statics.len()))?;
        let source=self.original.borrow().as_ref().ok_or_else(||bad("buffer capture lacks independent original proof"))?.buffer_capture_source(caller.definition,node,b)?;
        let boundaries=self.closure_boundaries.borrow();let mut matched=false;
        for boundary in boundaries.iter().filter(|row|row.child==caller.id){
            b.spend(1)?;
            let ScopeKey::Instance(creator)=boundary.creator else{return Err(bad("captured buffer creator is not an actual original instance"));};
            let creator=self.instance(module,creator,b)?;
            if caller.parent!=Some(creator.id){return Err(bad("captured buffer creation changes its exact original lexical parent"));}
            let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=creator.target else{return Err(bad("captured buffer creator is not executable"));};
            if function!=boundary.at.function{return Err(bad("captured buffer boundary changes its actual creator function"));}
            let f=module.functions.get(function.0 as usize).filter(|f|f.id==function).ok_or_else(||bad("captured buffer creator function is absent"))?;
            let instruction=f.blocks.get(boundary.at.block.0 as usize).and_then(|block|block.instructions.get(boundary.at.instruction as usize)).ok_or_else(||bad("captured buffer creator point is absent"))?;
            let InstructionKind::MakeClosure{closure,captures}=&instruction.kind else{return Err(bad("captured buffer boundary is not its actual environment constructor"));};
            if *closure!=environment||boundary.value.ty!=Type::Closure(environment){return Err(bad("captured buffer uses another actual creator environment"));}
            let operand=*captures.get(capture as usize).ok_or_else(||bad("captured buffer creator omits its physical capture operand"))?;
            if !self.capture_creator_source(module,creator,f,operand,source,b)?{return Err(bad("buffer projection substitutes another same-typed original free binder"));}
            matched=true;
        }
        if !matched{return Err(bad("buffer capture projection has no exact sealed original creator occurrence"));}Ok(())
    }
}

impl CheckedContext {
    fn pattern_local_root(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,local:LocalId,node:NodeId,b:&mut Budget)->Check<()>{
        let(subject,path,name,span)=self.original.borrow().as_ref().ok_or_else(||bad("pattern address has no independent original body proof"))?.pattern_place(module,caller.definition,node,b)?;
        let declaration=f.locals.get(local.0 as usize).ok_or_else(||bad("pattern address local is absent"))?;
        if declaration.name!=name||declaration.span!=span{return Err(bad("pattern address changes its exact original binding declaration"));}
        let definition=self.definition(module,caller.definition,b)?;
        let SemanticDefinitionBody::FunctionWithBody{site_nodes,..}=&definition.body else{return Err(bad("pattern address owner has no complete original body"));};
        let program=module.semantic.as_ref().ok_or_else(||bad("pattern address programme is absent"))?;
        let mut subjects=BTreeSet::new();
        for original in site_nodes.iter().filter(|row|row.node==SemanticOriginalNode::Expression(subject)){
            b.spend(program.sites.len())?;
            for site in program.sites.iter().filter(|site|site.instance==caller.id&&site.recipe==original.site){
                match site.action{
                    SemanticSiteAction::TypeUse{subject:SemanticOperand::Value(value),..}=>{subjects.insert(value);},
                    SemanticSiteAction::BodyCall{..}|SemanticSiteAction::Call{..}=>{
                        let SemanticSiteAddress::Typed{block,instruction,..}=site.at else{return Err(bad("pattern subject retains a nonactual original call"));};
                        if let Some(value)=f.blocks.get(block.0 as usize).and_then(|block|block.instructions.get(instruction as usize)).and_then(|instruction|instruction.result){subjects.insert(value);}
                    },
                    _=>{},
                }
            }
        }
        if subjects.is_empty(){return Err(bad("pattern address has no exact actual original subject occurrence"));}
        let mut live=None;let mut initial=None;
        for block in &f.blocks{b.spend(block.instructions.len())?;for(index,instruction)in block.instructions.iter().enumerate(){
            if instruction.kind==InstructionKind::StorageLive(local){
                if instruction.span!=span||live.replace((block.id,index)).is_some(){return Err(bad("pattern local changes or repeats its original declaration point"));}
                let store=block.instructions.get(index+1).ok_or_else(||bad("pattern local lacks its actual initial storage transfer"))?;
                let InstructionKind::Store{local:target,value}=store.kind else{return Err(bad("pattern local declaration is not followed by its actual initial storage transfer"));};
                if target!=local||store.span!=span{return Err(bad("pattern local initial transfer changes its original binding point"));}
                initial=Some(value);
            }
        }}
        let value=initial.ok_or_else(||bad("pattern address has no complete actual binding activation"))?;
        self.pattern_projection_source(module,f,value,&path,&subjects,b)
    }
    fn pattern_projection_source(&self,module:&TypedModule,f:&TypedFunction,value:ValueId,path:&[PlaceProjection],subjects:&BTreeSet<ValueId>,b:&mut Budget)->Check<()>{
        let mut pending=vec![(false,value.0,path.to_vec(),false)];let mut active=BTreeSet::new();let mut proven=BTreeSet::new();let mut rooted=false;
        while let Some((local,index,mut path,exit))=pending.pop(){
            b.spend(path.len().saturating_add(1))?;let key=(local,index,path.clone());
            if exit{active.remove(&key);proven.insert(key);continue;}
            if proven.contains(&key){continue;}
            if !active.insert(key.clone()){return Err(bad("pattern binding projection has a cyclic actual source"));}
            pending.push((local,index,path.clone(),true));
            if !local&&path.is_empty()&&subjects.contains(&ValueId(index)){rooted=true;continue;}
            if local{
                let mut found=false;
                for block in &f.blocks{b.spend(block.instructions.len())?;for instruction in &block.instructions{match &instruction.kind{
                    InstructionKind::Store{local:target,value}if target.0==index=>{found=true;pending.push((false,value.0,path.clone(),false));},
                    InstructionKind::UnpackTuple{local:source,fields}|InstructionKind::UnpackVariant{local:source,fields,..}=>{
                        b.spend(fields.len())?;
                        for(ordinal,target)in fields.iter().enumerate().filter(|(_,target)|target.0==index){
                            let mut path=path.clone();
                            let step=match instruction.kind{InstructionKind::UnpackVariant{variant,..}=>PlaceProjection::Variant(VariantProjection{variant,index:ordinal as u32}),_=>PlaceProjection::Field(ordinal as u32)};
                            if path.pop()!=Some(step){return Err(bad("pattern unpack changes its exact original selected field"));}
                            crate::check_instruction(module,f,instruction,b)?;pending.push((true,source.0,path.clone(),false));found=true;
                        }
                    },_=>{},
                }}}
                if !found{return Err(bad("pattern binding lacks its complete actual initializer source"));}
            }else{
                let(_,_,instruction)=producer(f,ValueId(index),b)?;
                crate::check_instruction(module,f,instruction,b)?;
                match &instruction.kind{
                    InstructionKind::Load(source)|InstructionKind::TakeLocal{local:source}=>pending.push((true,source.0,path,false)),
                    InstructionKind::ReadErrorUnion{value,is_error}|InstructionKind::TakeErrorUnion{value,is_error}=>{
                        if path.pop()!=Some(PlaceProjection::ErrorArm(*is_error)){return Err(bad("pattern binding changes its exact original ErrorUnion arm"));}pending.push((false,value.0,path,false));
                    },
                    InstructionKind::ExtractField{aggregate,index}=>{
                        if path.pop()!=Some(PlaceProjection::Field(*index)){return Err(bad("pattern binding changes its exact original product field"));}pending.push((false,aggregate.0,path,false));
                    },
                    InstructionKind::TakeVariantField{local, path:actual}|InstructionKind::TakeTupleField{local,path:actual}=>{
                        for step in actual.iter().rev(){b.spend(1)?;if path.pop()!=Some(*step){return Err(bad("pattern binding changes its exact original selected place"));}}pending.push((true,local.0,path,false));
                    },
                    InstructionKind::ReadVariantLocal{local,path:actual}=>{
                        for step in actual.iter().rev(){b.spend(1)?;if path.pop()!=Some(PlaceProjection::Variant(*step)){return Err(bad("pattern read changes its exact original selected variant"));}}pending.push((true,local.0,path,false));
                    },
                    _=>return Err(unsupported("pattern address needs its complete actual original subject projection",Some(instruction.span))),
                }
            }
        }
        if !rooted{return Err(bad("pattern binding projection never reaches its exact original subject"));}Ok(())
    }
}

pub(super) fn producer<'a>(f:&'a TypedFunction,value:ValueId,b:&mut Budget)->Check<(BlockId,u32,&'a Instruction)>{
    let mut result=None;
    for block in &f.blocks{b.spend(block.instructions.len())?;for(index,instruction)in block.instructions.iter().enumerate(){if instruction.result==Some(value)&&result.replace((block.id,index as u32,instruction)).is_some(){return Err(bad("buffer projection has several actual SSA producers"));}}}
    result.ok_or_else(||bad("buffer projection lacks its actual SSA definition"))
}
fn consume_field(ty:&Type,index:u32,fields:&mut Vec<String>,b:&mut Budget)->Check<()>{
    dynamic_metadata::ty(ty,b)?;let name=fields.pop().ok_or_else(||bad("actual buffer projection adds a field absent from its original Place"))?;
    let mut storage=ty;
    loop{b.spend(1)?;match storage{Type::BorrowedStorage{storage:inner,..}|Type::Managed(inner)|Type::Address{inner,..}|Type::Desc(inner)=>storage=inner,_=>break,}}
    let correct=match storage{Type::Record{fields,..}|Type::Package{fields,..}=>fields.get(index as usize).is_some_and(|(actual,_)|actual==&name),Type::Tuple(fields)=>(index as usize)<fields.len()&&index.to_string()==name,_=>false};
    if !correct{return Err(bad("actual field index differs from the original ordered nominal/tuple field"));}Ok(())
}
