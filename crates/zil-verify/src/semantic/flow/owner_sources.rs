//! Match original observation Places to actual source projections. These
//! selectors prove identities only; address/description loans, initialization,
//! field debts and retirement remain obligations of the ordinary kernels.
use super::*;
use original::intrinsics::{AddressProjection,AddressRoot,BufferRoot,BufferSource};
use zil_base::NodeId;
#[path = "scalar_preparations.rs"]
pub(in crate::semantic::flow) mod scalar_preparations;
#[path = "range_bindings.rs"]
mod range_bindings;
#[path = "capture_mirrors.rs"]
mod capture_mirrors;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PatternStart { Value(ValueId), Unpack { local: LocalId, at: (BlockId, usize) } }

// The activation is one exact Store or atomic unpack immediately following
// its ordered declaration bundle, never another local with an equal class.
fn pattern_initial_transfer(module: &TypedModule, f: &TypedFunction, local: LocalId, span: Span, b: &mut Budget) -> Check<PatternStart> {
    let mut initial = None;
    for block in &f.blocks {
        b.spend(block.instructions.len())?;
        for (index, instruction) in block.instructions.iter().enumerate() {
            if instruction.kind != InstructionKind::StorageLive(local) { continue; }
            if instruction.span != span || initial.is_some() { return Err(bad("pattern local changes or repeats its original declaration point")); }
            let mut end = index + 1;
            while matches!(block.instructions.get(end).map(|i| &i.kind), Some(InstructionKind::StorageLive(_))) { b.spend(1)?; end += 1; }
            let transfer = block.instructions.get(end).ok_or_else(||bad("pattern local lacks its actual initial storage transfer"))?;
            initial = Some(match &transfer.kind {
                InstructionKind::Store { local: target, value } if end == index + 1 => {
                    if *target != local || transfer.span != span { return Err(bad("pattern local initial transfer changes its original binding point")); }
                    crate::check_instruction(module, f, transfer, b)?;
                    PatternStart::Value(*value)
                }
                InstructionKind::UnpackTuple { local: source, fields }
                | InstructionKind::UnpackVariant { local: source, fields, .. } => {
                    let mut begin = index;
                    while begin > 0 && matches!(block.instructions[begin - 1].kind, InstructionKind::StorageLive(_)) { b.spend(1)?; begin -= 1; }
                    b.spend(fields.len().saturating_mul(2))?;
                    if end - begin != fields.len() || fields.contains(source) || fields.iter().filter(|field| **field == local).count() != 1 {
                        return Err(bad("pattern unpack does not initialize its exact declaration bundle"));
                    }
                    for (instruction, field) in block.instructions[begin..end].iter().zip(fields) {
                        if instruction.kind != InstructionKind::StorageLive(*field) { return Err(bad("pattern unpack changes its ordered actual declaration bundle")); }
                    }
                    crate::check_instruction(module, f, transfer, b)?;
                    PatternStart::Unpack { local, at: (block.id, end) }
                }
                _ => return Err(bad("pattern local declaration is not followed by its actual initial storage transfer")),
            });
        }
    }
    initial.ok_or_else(||bad("pattern address has no complete actual binding activation"))
}

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
    pub(in crate::semantic::flow) fn original_projection_source(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,value:ValueId,definition:SemanticDefinitionId,node:NodeId,read_value:bool,b:&mut Budget)->Check<()>{
        self.original_projection_source_prefix(module,caller,f,value,definition,node,read_value,None,b)
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::semantic::flow) fn original_projection_source_prefix(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,value:ValueId,definition:SemanticDefinitionId,node:NodeId,read_value:bool,prefix:Option<(&Type,&[PlaceProjection])>,b:&mut Budget)->Check<()>{
        if definition!=caller.definition{return Err(bad("indirect borrow selects another original lexical owner"));}
        let(root,projections)=self.original.borrow().as_ref().ok_or_else(||bad("indirect borrow lacks its complete original body proof"))?.address_place(module,definition,node,b)?;
        let skip=if let Some((ty,path))=prefix{consume_reserved_fields(ty,path,&projections,b)?}else{0};
        self.original_projection_path(module,caller,f,value,definition,root,&projections[skip..],read_value,b)
    }
    pub(in crate::semantic::flow) fn copy_assignment_target(
        &self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,parent:ValueId,read:&InstructionKind,node:NodeId,b:&mut Budget,
    )->Check<()> {
        let (root,path)=self.original.borrow().as_ref().ok_or_else(||bad("Copy destination lacks its original Place"))?
            .address_place(module,caller.definition,node,b)?;
        let (leaf,ancestors)=path.split_first().ok_or_else(||bad("Copy update adds a projection to the original destination"))?;
        match (leaf,read) {
            (AddressProjection::Field(name),InstructionKind::ExtractField{index,..})=>{
                let ty=f.values.get(parent.0 as usize).ok_or_else(||bad("Copy update has no aggregate type"))?;
                b.spend(name.len())?;
                consume_field(ty,*index,&mut vec![name.clone()],b)?;
            }
            (AddressProjection::Index(node),InstructionKind::ExtractIndex{index,..})=>
                self.exact_expression_value(module,caller,f,*index,*node,b)?,
            _=>return Err(bad("Copy update changes its original destination projection")),
        }
        self.original_projection_path(module,caller,f,parent,caller.definition,root,ancestors,true,b)
    }
    #[allow(clippy::too_many_arguments)]
    fn original_projection_path(
        &self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,mut value:ValueId,definition:SemanticDefinitionId,
        root:AddressRoot,projections:&[AddressProjection],read_value:bool,b:&mut Budget,
    )->Check<()> {
        let mut seen=BTreeSet::new();
        let mut fused=None;
        // An implicit scalar capture read can have an empty original field
        // path. It still returns to the exact original binder below.
        if read_value {loop{
            b.spend(1)?;
            let(_,_,instruction)=producer(f,value,b)?;
            let address=match instruction.kind{InstructionKind::ReadAddress{address}|InstructionKind::DescribeAddress{address}=>address,_=>break};
            if !seen.insert(value){return Err(bad("observed original root has a cyclic actual read/description chain"));}
            crate::check_instruction(module,f,instruction,b)?;value=address;
        }}
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
    pub(in crate::semantic::flow) fn replacement_target_source(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,place:&crate::place_replacements::Place,node:NodeId,b:&mut Budget)->Check<()>{
        match place.origin{
            crate::place_replacements::Origin::Address(address)=>{
                let Type::Address{mutable:true,inner}=f.values.get(address.0 as usize).ok_or_else(||bad("reserved address source has no actual type"))? else{return Err(bad("reserved address source lacks exclusive authority"));};
                self.original_projection_source_prefix(module,caller,f,address,caller.definition,node,false,Some((inner,&place.steps)),b)
            },
            crate::place_replacements::Origin::Local(local)=>{
                let ty=&f.locals.get(local.0 as usize).ok_or_else(||bad("reserved local source is absent"))?.ty;
                let(root,projections)=self.original.borrow().as_ref().ok_or_else(||bad("reserved local lacks its complete original Place"))?.address_place(module,caller.definition,node,b)?;
                if consume_reserved_fields(ty,&place.steps,&projections,b)?!=projections.len(){return Err(bad("reserved local omits an original destination projection"));}
                match root{
                    AddressRoot::Place(root)=>{
                        let(root,fields)=self.original.borrow().as_ref().ok_or_else(||bad("reserved local lacks its original binder"))?.buffer_place(caller.definition,root,b)?;
                        if !fields.is_empty(){return Err(bad("reserved local changes its original root projection"));}
                        self.projected_local_root(module,caller,f,local,root,node,b)
                    },
                    AddressRoot::Parameter(ordinal)=>{
                        if self.buffer_source_local(module,caller,f,BufferSource::Parameter{definition:caller.definition,ordinal},b)?!=local{return Err(bad("reserved local selects another original parameter"));}Ok(())
                    },
                    AddressRoot::Value(_)=>Err(bad("reserved local replaces an original indirect value destination")),
                }
            },
        }
    }
    pub(in crate::semantic::flow) fn assignment_value_source(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,value:ValueId,node:NodeId,b:&mut Budget)->Check<()>{
        let(operand,place)=self.original.borrow().as_ref().ok_or_else(||bad("assignment RHS lacks its complete original occurrence proof"))?.assignment_value_source(module,caller.definition,node,b)?;
        let result=if place{
            self.original_projection_source(module,caller,f,value,caller.definition,operand,true,b)
        }else{
            self.exact_expression_value(module,caller,f,value,operand,b)
        };
        result.map_err(|error|error.with_note(format!("original assignment RHS {:?} expression {:?}, transparent operand {:?}, actual {:?}",caller.definition,node,operand,value)))
    }
    pub(in crate::semantic::flow) fn exact_expression_value(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,mut value:ValueId,node:NodeId,b:&mut Budget)->Check<()>{
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
        let mut seen=BTreeSet::new();loop{
            b.spend(sources.len().saturating_add(1))?;
            if sources.contains(&value){
                let ty=f.values.get(value.0 as usize).ok_or_else(||bad("exact source SSA type is absent"))?;
                if ty.is_integer()||ty.is_float()||matches!(ty,Type::Unit|Type::Bool){
                    let name=self.original.borrow().as_ref().ok_or_else(||bad("exact scalar occurrence lacks its original binding proof"))?.bound_value_name(caller.definition,node,b)?;
                    if name{self.original_projection_source(module,caller,f,value,caller.definition,node,true,b).map_err(|error|{
                        if error.category==zil_base::ResultCategory::Unsupported{bad("exact scalar source occurrence selects another original binding").with_note(error.message)}
                        else{error.with_note(format!("original bound scalar {:?} expression {:?}, actual {:?}",caller.definition,node,value))}
                    })?;}
                }
                return Ok(());
            }
            if !seen.insert(value){return Err(bad("address input has a cyclic actual preparation/reborrow chain"));}
            if let Some(source)=self.prepared_closure_source(module,caller,f,value,b)?{value=source;continue;}
            let(block,at,instruction)=producer(f,value,b)?;
            crate::check_instruction(module,f,instruction,b)?;
            if let InstructionKind::Load(local)=instruction.kind {
                if let Some(source)=scalar_preparations::source(module,f,block,at,local,b)?{value=source;continue;}
            }
            if sources.is_empty(){return self.scalar_expression_source(module,caller,f,value,node,b);}
            if let InstructionKind::ReborrowAddress{address,..}=instruction.kind{value=address;continue;}
            return Err(bad("address input selects another same-typed original expression value").with_note(format!(
                "original {:?} expression {:?}, exact source SSA {:?}; actual {:?} at {:?}:{at} has producer {:?}",caller.definition,node,sources,value,block,instruction.kind)));
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
            if let Some(source)=self.prepared_closure_source(module,caller,f,value,b)?{value=source;continue;}
            let(block,index,instruction)=producer(f,value,b)?;
            match &instruction.kind{
                InstructionKind::ReborrowAddress{address,..}|InstructionKind::AddressSlice{address,..}|InstructionKind::DescribeAddress{address}=>value=*address,
                InstructionKind::AddressField{address,index}=>{let ty=f.values.get(address.0 as usize).ok_or_else(||bad("field address operand is absent"))?;consume_field(ty,*index,&mut fields,b)?;value=*address;},
                InstructionKind::ExtractField{aggregate,index}=>{let ty=f.values.get(aggregate.0 as usize).ok_or_else(||bad("field aggregate operand is absent"))?;consume_field(ty,*index,&mut fields,b)?;value=*aggregate;},
                InstructionKind::BorrowPlaceLocal{local,path,..}=>{
                    crate::check_instruction(module,f,instruction,b)?;
                    let ty=&f.locals.get(local.0 as usize).ok_or_else(||bad("borrowed Place source local is absent"))?.ty;
                    consume_place_fields(ty,path,&mut fields,b)?;
                    if !fields.is_empty(){return Err(bad("borrowed Place omitted an original buffer field projection"));}
                    return self.projected_local_root(module,caller,f,*local,root,node,b);
                },
                InstructionKind::DescribePackageField{local,index}|InstructionKind::ReadPackageField{local,index}|InstructionKind::TakePackageField{local,index}|InstructionKind::BorrowPackageField{local,index,..}=>{
                    consume_field(&f.locals.get(local.0 as usize).ok_or_else(||bad("package source local is absent"))?.ty,*index,&mut fields,b)?;
                    if !fields.is_empty(){return Err(unsupported("nested package observation requires its actual local-to-parent projection ledger",Some(instruction.span)));}
                    return self.projected_local_root(module,caller,f,*local,root,node,b);
                },
                InstructionKind::TakeOwnedArrayElement{local,index:0}|InstructionKind::BorrowOwnedArrayElement{local,index:0,..}=>{
                    if !fields.is_empty(){return Err(bad("private callback source omitted an original field projection"));}
                    self.persistent_initialization(module,caller,f,*local,b)?;
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
        let range=self.original.borrow().as_ref().ok_or_else(||bad("local source lacks its complete original binder proof"))?.range_name(module,caller.definition,node,b)?;
        if let Some(range)=range{return self.range_local_root(module,caller,f,local,&range,b);}
        match root{
            BufferRoot::Local{node}=>if self.buffer_source_local(module,caller,f,BufferSource::Local{definition:caller.definition,node},b)?!=local{return Err(bad("field observation selects another original local"));},
            BufferRoot::Parameter{ordinal}=>if self.buffer_source_local(module,caller,f,BufferSource::Parameter{definition:caller.definition,ordinal},b)?!=local{return Err(bad("field observation selects another original parameter"));},
            BufferRoot::Capture=>{
                self.captured_local_alias(module,caller,f,local,node,b)?;
            },
            BufferRoot::Pattern=>self.pattern_local_root(module,caller,f,local,node,b)?,
        }Ok(())
    }
    pub(super) fn range_local_root(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,local:LocalId,range:&original::RangeBinding,b:&mut Budget)->Check<()> {
        let(start,end)=range_bindings::check(module,f,local,range,b)?;
        self.exact_expression_value(module,caller,f,start,range.start,b)?;
        self.exact_expression_value(module,caller,f,end,range.end,b)
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
                    if stores.len() > 1
                        && let Some(initial) = capture_mirrors::initial(module, f, local, b)?
                    {
                        work.push((false, initial.0, false));
                    } else {
                        b.spend(stores.len())?;
                        work.extend(stores.into_iter().map(|value| (false, value.0, false)));
                    }
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
        let(subject,path,name,span,mutable)=self.original.borrow().as_ref().ok_or_else(||bad("pattern address has no independent original body proof"))?.pattern_place(module,caller.definition,node,b)?;
        let declaration=f.locals.get(local.0 as usize).ok_or_else(||bad("pattern address local is absent"))?;
        if declaration.name!=name||declaration.span!=span||declaration.mutable!=mutable{return Err(bad("pattern address changes its exact original binding declaration"));}
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
        let initial = pattern_initial_transfer(module, f, local, span, b)?;
        if !mutable && (declaration.ty.is_integer() || declaration.ty.is_float() || matches!(declaration.ty, Type::Unit | Type::Bool)) {
            match initial {
                PatternStart::Value(value) => {
                    let slot = scalar_preparations::slot(f, local, true, b)?.ok_or_else(||bad("scalar pattern source storage is mutable, aliased or has no unique activation"))?;
                    if slot.stores.len() != 1 || slot.stores[0].2 != value { return Err(bad("scalar pattern binding has another actual write or initial source")); }
                }
                PatternStart::Unpack { at, .. } => {
                    let slot = scalar_preparations::unpacked_slot(f, local, at, b)?.ok_or_else(||bad("scalar pattern unpack is aliased or has another initialization"))?;
                    if !slot.stores.is_empty() { return Err(bad("scalar unpack pattern has another actual write")); }
                }
            }
        }
        self.pattern_projection_start(module, caller, f, initial, subject, &path, &subjects, b)
    }
    fn pattern_projection_source(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,value:ValueId,subject:NodeId,path:&[LifeTypeProjection],subjects:&BTreeSet<ValueId>,b:&mut Budget)->Check<()>{
        self.pattern_projection_start(module, caller, f, PatternStart::Value(value), subject, path, subjects, b)
    }
    fn pattern_projection_start(&self,module:&TypedModule,caller:&SemanticInstance,f:&TypedFunction,start:PatternStart,subject:NodeId,path:&[LifeTypeProjection],subjects:&BTreeSet<ValueId>,b:&mut Budget)->Check<()>{
        let (local, index) = match start { PatternStart::Value(value) => (false, value.0), PatternStart::Unpack { local, .. } => (true, local.0) };
        let mut pending=vec![(local,index,path.to_vec(),false)];let mut active=BTreeSet::new();let mut proven=BTreeSet::new();let mut rooted=false;
        while let Some((local,index,mut path,exit))=pending.pop(){
            b.spend(path.len().saturating_add(1))?;let key=(local,index,path.clone());
            if exit{active.remove(&key);proven.insert(key);continue;}
            if proven.contains(&key){continue;}
            if !active.insert(key.clone()){return Err(bad("pattern binding projection has a cyclic actual source"));}
            pending.push((local,index,path.clone(),true));
            if !local&&path.is_empty(){
                if self.original.borrow().as_ref().ok_or_else(||bad("pattern subject original proof is absent"))?.bound_value_name(caller.definition,subject,b)?{
                    self.original_projection_source(module,caller,f,ValueId(index),caller.definition,subject,true,b)?;rooted=true;continue;
                }
                if subjects.contains(&ValueId(index)){rooted=true;continue;}
                if subjects.is_empty(){self.exact_expression_value(module,caller,f,ValueId(index),subject,b)?;rooted=true;continue;}
            }
            if local{
                let mut found=false;
                for block in &f.blocks{b.spend(block.instructions.len())?;for instruction in &block.instructions{match &instruction.kind{
                    InstructionKind::Store{local:target,value}if target.0==index=>{found=true;pending.push((false,value.0,path.clone(),false));},
                    InstructionKind::UnpackTuple{local:source,fields}|InstructionKind::UnpackVariant{local:source,fields,..}=>{
                        b.spend(fields.len())?;
                        for(ordinal,target)in fields.iter().enumerate().filter(|(_,target)|target.0==index){
                            let mut path=path.clone();
                            let step=match instruction.kind{InstructionKind::UnpackVariant{variant,..}=>PlaceProjection::Variant(VariantProjection{variant,index:ordinal as u32}),_=>PlaceProjection::Field(ordinal as u32)};
                            if path.pop()!=Some(LifeTypeProjection::Place(step)){return Err(bad("pattern unpack changes its exact original selected field"));}
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
                        if path.pop()!=Some(LifeTypeProjection::Place(PlaceProjection::ErrorArm(*is_error))){return Err(bad("pattern binding changes its exact original ErrorUnion arm"));}pending.push((false,value.0,path,false));
                    },
                    InstructionKind::ExtractField{aggregate,index}=>{
                        if path.pop()!=Some(LifeTypeProjection::Place(PlaceProjection::Field(*index))){return Err(bad("pattern binding changes its exact original product field"));}pending.push((false,aggregate.0,path,false));
                    },
                    InstructionKind::ReadVariant{value,variant,index}=>{
                        if path.pop()!=Some(LifeTypeProjection::Place(PlaceProjection::Variant(VariantProjection{variant:*variant,index:*index}))){return Err(bad("pattern binding changes its exact original variant field"));}
                        pending.push((false,value.0,path,false));
                    },
                    InstructionKind::ReadAddress{address}=>{
                        if path.pop()!=Some(LifeTypeProjection::Pointee){return Err(bad("pattern binding changes its original dereference boundary"));}
                        pending.push((false,address.0,path,false));
                    },
                    InstructionKind::TakeVariantField{local, path:actual}|InstructionKind::TakeTupleField{local,path:actual}=>{
                        for step in actual.iter().rev(){b.spend(1)?;if path.pop()!=Some(LifeTypeProjection::Place(*step)){return Err(bad("pattern binding changes its exact original selected place"));}}pending.push((true,local.0,path,false));
                    },
                    InstructionKind::ReadVariantLocal{local,path:actual}=>{
                        for step in actual.iter().rev(){b.spend(1)?;if path.pop()!=Some(LifeTypeProjection::Place(PlaceProjection::Variant(*step))){return Err(bad("pattern read changes its exact original selected variant"));}}pending.push((true,local.0,path,false));
                    },
                    _=>return Err(unsupported("pattern address needs its complete actual original subject projection",Some(instruction.span))),
                }
            }
        }
        if !rooted{return Err(bad("pattern binding projection never reaches its exact original subject"));}Ok(())
    }
}

pub(in crate::semantic::flow) fn producer<'a>(f:&'a TypedFunction,value:ValueId,b:&mut Budget)->Check<(BlockId,u32,&'a Instruction)>{
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

fn consume_place_fields(ty:&Type,path:&[PlaceProjection],fields:&mut Vec<String>,b:&mut Budget)->Check<()>{
    if path.is_empty()||path.len()>b.limits.max_call_depth{return Err(bad("buffer Place must contain its complete bounded field path"));}
    b.spend(path.len().saturating_mul(2))?;
    let mut parents=Vec::with_capacity(path.len());
    let mut current=ty;
    for step in path{
        let PlaceProjection::Field(index)=step else{return Err(bad("buffer Place field path adds another original projection kind"));};
        parents.push((current,*index));
        current=lifetimes::select(current,&[LifeTypeProjection::Place(*step)],b)?;
    }
    // One fused Place operation walks from the root to its leaf. The original
    // field matcher consumes the same chain from leaf to root, just like the
    // separate AddressField/ExtractField operations above.
    for(parent,index)in parents.into_iter().rev(){consume_field(parent,index,fields,b)?;}
    Ok(())
}

fn consume_reserved_fields(ty:&Type,path:&[PlaceProjection],original:&[AddressProjection],b:&mut Budget)->Check<usize>{
    if path.len()>b.limits.max_call_depth{return Err(limit("reserved original projection exceeds depth"));}
    b.spend(path.len().saturating_mul(2))?;
    let mut parents=Vec::with_capacity(path.len());let mut current=ty;
    for step in path{
        let PlaceProjection::Field(index)=step else{return Err(bad("reserved inline path changes its original field projection kind"));};
        parents.push((current,*index));current=lifetimes::select(current,&[LifeTypeProjection::Place(*step)],b)?;
    }
    for(index,(parent,field))in parents.into_iter().rev().enumerate(){
        let Some(AddressProjection::Field(name))=original.get(index)else{return Err(bad("reserved field path differs from its complete original destination"));};
        b.spend(name.len().saturating_add(1))?;let mut names=vec![name.clone()];consume_field(parent,field,&mut names,b)?;
    }
    Ok(path.len())
}

#[cfg(test)]
mod pattern_source_tests {
    use super::*;
    mod fixture{include!(concat!(env!("CARGO_MANIFEST_DIR"),"/../../tests/language-readiness/terminal/higher-rank-borrows/stored_flow_fixture.rs"));}
    #[test]
    fn reserved_projection_uses_original_fields_not_same_typed_siblings_raw_wire(){
        let limits=Limits::default();let(mut module,plan)=crate::tests::baseline();
        let store=Type::Managed(Box::new(Type::OwnedStore{element:Box::new(Type::I64)}));
        module.functions[0].locals[0].ty=Type::Package{name:"Map".into(),fields:vec![
            ("entries".into(),store.clone()),("buckets".into(),store),
        ]};
        for wire in [false,true]{
            let bytes=zil_ir::wire::encode_module_candidate(&module,&plan,&limits).unwrap();
            let(raw,_)=if wire{zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap()}else{(module.clone(),plan.clone())};
            let ty=&raw.functions[0].locals[0].ty;
            let original=[AddressProjection::Field("buckets".into())];
            assert_eq!(consume_reserved_fields(ty,&[PlaceProjection::Field(1)],&original,&mut Budget::new(&limits)).unwrap(),1);
            assert!(consume_reserved_fields(ty,&[PlaceProjection::Field(0)],&original,&mut Budget::new(&limits)).is_err(),"same payload type cannot select the sibling owner");
            assert!(consume_reserved_fields(ty,&[PlaceProjection::Field(1)],&[],&mut Budget::new(&limits)).is_err());
            assert!(consume_reserved_fields(ty,&[PlaceProjection::ErrorArm(false)],&original,&mut Budget::new(&limits)).is_err());
            assert!(consume_reserved_fields(ty,&[PlaceProjection::Field(1),PlaceProjection::Field(0)],&original,&mut Budget::new(&limits)).is_err());
        }
    }
    #[test]
    fn fused_owned_place_matches_every_original_field_raw_wire(){
        let limits=Limits::default();let(mut module,plan)=crate::tests::baseline();
        module.functions[0].locals[0].ty=Type::Package{name:"Entry".into(),fields:vec![
            ("key".into(),Type::I32),
            ("value".into(),Type::Package{name:"Value".into(),fields:vec![
                ("bytes".into(),Type::Managed(Box::new(Type::ByteBuffer))),
                ("id".into(),Type::I32),
            ]}),
        ]};
        for wire in [false,true]{
            let bytes=zil_ir::wire::encode_module_candidate(&module,&plan,&limits).unwrap();
            let(raw,_)=if wire{zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap()}else{(module.clone(),plan.clone())};
            let ty=&raw.functions[0].locals[0].ty;
            let original=vec!["value".into(),"bytes".into()];
            let path=vec![PlaceProjection::Field(1),PlaceProjection::Field(0)];
            let mut fields=original.clone();
            consume_place_fields(ty,&path,&mut fields,&mut Budget::new(&limits)).unwrap();assert!(fields.is_empty());
            // This is only the ordered projection sub-proof. The executable
            // path still authenticates the original binding, actual opcode,
            // initialized owner and independent description/loan window.
            for attack in 0..6{
                let mut fields=original.clone();let mut wrong=path.clone();
                match attack{
                    0=>wrong.reverse(),
                    1=>wrong[1]=PlaceProjection::Field(1),
                    2=>{wrong.remove(0);},
                    3=>wrong.push(PlaceProjection::Field(0)),
                    4=>fields[1]="other".into(),
                    _=>wrong[0]=PlaceProjection::ErrorArm(false),
                }
                assert!(consume_place_fields(ty,&wrong,&mut fields,&mut Budget::new(&limits)).is_err(),"wire={wire} attack={attack}");
            }
        }
    }
    #[test]
    fn pattern_subject_preserves_success_variant_pointee_and_exact_anchor(){
        // This isolates the private projection sub-proof with an already
        // supplied subject anchor; it does not construct a VerifiedModule or
        // give an empty programme an executable original-body proof.
        let mut module=fixture::module();
        module.semantic=Some(SemanticModule{definitions:vec![],instances:vec![],sites:vec![],dyn_scopes:vec![]});
        let limits=Limits::default();let mut budget=Budget::new(&limits);
        let context=CheckedContext::catalogue(&module,&mut budget).unwrap();
        *context.original.borrow_mut()=Some(Default::default());
        let caller=SemanticInstance{
            id:SemanticInstanceId{module:zil_base::ModuleId(0),index:0},
            definition:SemanticDefinitionId{module:zil_base::ModuleId(0),index:0},
            target:SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(FunctionId(0))),
            lifetime_instantiation:SemanticLifetimeInstantiation::Rigid,parent:None,actuals:vec![],header_embedding:vec![],environment:None,self_type:None,
        };
        let option=Type::Enum{name:"OptionByte".into(),variants:vec![("None".into(),vec![]),("Some".into(),vec![Type::U8])]};
        let union=Type::ErrorUnion{error:Box::new(Type::I32),result:Box::new(option.clone())};
        for borrowed in [false,true]{
            let mut f=module.functions[0].clone();f.environment=None;f.parameters=vec![LocalId(0)];f.locals.truncate(1);
            f.locals[0].ty=if borrowed{Type::Address{mutable:false,inner:Box::new(union.clone())}}else{union.clone()};
            f.return_type=Type::U8;f.blocks.truncate(1);
            let mut instructions=vec![fixture::instruction(Some(0),InstructionKind::Load(LocalId(0)))];
            let mut values=vec![f.locals[0].ty.clone()];let mut path=Vec::new();let mut value=ValueId(0);
            if borrowed {
                values.push(union.clone());instructions.push(fixture::instruction(Some(1),InstructionKind::ReadAddress{address:value}));
                value=ValueId(1);path.push(LifeTypeProjection::Pointee);
            }
            let union_value=value;let option_value=ValueId(values.len() as u32);values.push(option.clone());
            instructions.push(fixture::instruction(Some(option_value.0),InstructionKind::ReadErrorUnion{value,is_error:false}));
            value=ValueId(values.len() as u32);values.push(Type::U8);
            instructions.push(fixture::instruction(Some(value.0),InstructionKind::ReadVariant{value:option_value,variant:1,index:0}));
            path.extend([LifeTypeProjection::Place(PlaceProjection::ErrorArm(false)),LifeTypeProjection::Place(PlaceProjection::Variant(VariantProjection{variant:1,index:0}))]);
            f.values=values;f.blocks[0].instructions=instructions;f.blocks[0].terminator=Terminator::Return(Some(value));
            let anchors=BTreeSet::from([ValueId(0)]);
            context.pattern_projection_source(&module,&caller,&f,value,NodeId(100),&path,&anchors,&mut Budget::new(&limits)).unwrap();
            for attack in 0..5{
                let mut wrong=path.clone();let mut anchors=anchors.clone();
                match attack{
                    0=>wrong[borrowed as usize]=LifeTypeProjection::Place(PlaceProjection::ErrorArm(true)),
                    1=>*wrong.last_mut().unwrap()=LifeTypeProjection::Place(PlaceProjection::Variant(VariantProjection{variant:0,index:0})),
                    2=>*wrong.last_mut().unwrap()=LifeTypeProjection::Place(PlaceProjection::Variant(VariantProjection{variant:1,index:1})),
                    3=>{wrong.remove(borrowed as usize);},
                    _=>{anchors=BTreeSet::from([option_value]);},
                }
                assert!(context.pattern_projection_source(&module,&caller,&f,value,NodeId(100),&wrong,&anchors,&mut Budget::new(&limits)).is_err(),"borrowed={borrowed} attack={attack}");
            }
            let mut changed=f;let index=option_value.0 as usize;
            changed.blocks[0].instructions[index].kind=InstructionKind::ReadErrorUnion{value:union_value,is_error:true};
            assert!(context.pattern_projection_source(&module,&caller,&changed,value,NodeId(100),&path,&anchors,&mut Budget::new(&limits)).is_err());
        }
    }
}

#[cfg(test)]
#[path = "pattern_initialization_tests.rs"]
mod pattern_initialization_tests;
