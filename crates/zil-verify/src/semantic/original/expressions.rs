use super::*;
impl CheckedOriginal {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn expression(&mut self,module:&TypedModule,context:&CheckedContext,expression:&SemanticOriginalExpr,env:&Env,bindings:&BTreeMap<String,Binding>,expected:Option<&Ty>,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty>{
        let token=active.fresh(b)?;active.with_query(DependencyKey{kind:QueryKind::Type,token},b,|active,b|{
            let id=env.current()?;self.remember_names(env,SemanticOriginalNode::Expression(expression.id),bindings,b)?;
            let actual=match &expression.kind{
                CoreExprKind::Unit=>Ty::Atom(Type::Unit),
                CoreExprKind::Bool(_)=>Ty::Atom(Type::Bool),
                CoreExprKind::Literal(value)=>Ty::Atom(value.ty()),
                CoreExprKind::Integer(text)|CoreExprKind::Float(text)=>self.literal_original(module,text,matches!(expression.kind,CoreExprKind::Float(_)),expected,b)?,
                CoreExprKind::String(_)=>Ty::Address{mutable:false,life:Life::Static,inner:Box::new(Ty::Slice(Box::new(Ty::Atom(Type::U8))))},
                CoreExprKind::Name(name)=>match bindings.get(name){Some(binding)=>{self.read_original(module,context,&Place{binder:binding.identity.clone(),fields:vec![]},&binding.ty,env,true,active,b)?;binding.ty.clone()},None=>match self.bound_name(module,context,name,env,active,b)?{Arg::Type(ty)=>ty,Arg::Const(value)=>*value.carrier,Arg::Life(_)=>return Err(bad("original value expression selects a lifetime name"))}},
                CoreExprKind::Group(value)=>self.expression(module,context,value,env,bindings,expected,active,b)?,
                CoreExprKind::Ascribed{ty,value}=>{let expected=self.ty(module,context,ty,env,active,b)?;let actual=self.expression(module,context,value,env,bindings,Some(&expected),active,b)?;self.unify(&actual,&expected,b)?;expected},
                CoreExprKind::NumericConvert{value,target}|CoreExprKind::IntegerConvert{value,target}=>{let source=self.expression(module,context,value,env,bindings,None,active,b)?;let target=self.ty(module,context,target,env,active,b)?;if !matches!(self.resolved(&source,b)?,Ty::Atom(ref ty)if ty.is_integer()||ty.is_float())||!matches!(self.resolved(&target,b)?,Ty::Atom(ref ty)if ty.is_integer()||ty.is_float()){return Err(bad("original numeric conversion has nonnumeric endpoint"));}target},
                CoreExprKind::Unary{op:UnaryOp::Negate,operand}if matches!(operand.kind,CoreExprKind::Integer(_))=>{let CoreExprKind::Integer(text)=&operand.kind else{unreachable!()};let ty=self.literal_original(module,&format!("-{text}"),false,expected,b)?;self.expressions.insert((id,operand.id),ty.clone());ty},
                CoreExprKind::Unary{op:UnaryOp::Borrow|UnaryOp::BorrowMut,operand}=>{
                    let mutable=matches!(expression.kind,CoreExprKind::Unary{op:UnaryOp::BorrowMut,..});
                    if mutable{self.mutable_place(module,context,operand,env,bindings,active,b)?;}
                    let(child,place)=self.place(module,context,operand,env,bindings,active,b)?;if let Some(place)=place{self.read_original(module,context,&place,&child,env,false,active,b)?;}
                    Ty::Address{mutable,life:Life::Local(id,expression.id),inner:Box::new(child)}
                },
                CoreExprKind::Unary{op:UnaryOp::Dereference,..}|CoreExprKind::Member{..}|CoreExprKind::Index{..}=>{
                    let(ty,place)=self.place(module,context,expression,env,bindings,active,b)?;self.take_place(module,context,&ty,place.as_ref(),env,active,b)?;ty
                },
                CoreExprKind::Unary{op,operand}=>{
                    let child=self.expression(module,context,operand,env,bindings,expected,active,b)?;
                    match op{
                        UnaryOp::Give=>{if !matches!(self.resolved(&child,b)?,Ty::Managed(_)|Ty::Store(_)){return Err(bad("original give has no actual owning resource type"));}child},
                        UnaryOp::Not=>{self.unify(&child,&Ty::Atom(Type::Bool),b)?;Ty::Atom(Type::Bool)},
                        UnaryOp::Negate|UnaryOp::BitNot=>{if !matches!(self.resolved(&child,b)?,Ty::Atom(ref ty)if ty.is_integer()||(*op==UnaryOp::Negate&&ty.is_float())){return Err(bad("original unary scalar operation is not proved"));}child},
                        _=>unreachable!(),
                    }
                },
                CoreExprKind::Binary{op,left,right}=>{let operand_expected=if matches!(op,BinaryOp::Equal|BinaryOp::NotEqual|BinaryOp::Less|BinaryOp::LessEqual|BinaryOp::Greater|BinaryOp::GreaterEqual|BinaryOp::And|BinaryOp::Or){None}else{expected};let left=self.expression(module,context,left,env,bindings,operand_expected,active,b)?;let right=self.expression(module,context,right,env,bindings,Some(&left),active,b)?;self.numeric_binary_original(*op,&left,&right,b)?},
                CoreExprKind::Call{callee,arguments}=>self.call(module,context,expression,callee,arguments,env,bindings,expected,active,b)?,
                CoreExprKind::Tuple(fields)=>{let expected=expected.map(|ty|self.resolved(ty,b)).transpose()?;let mut actuals=Vec::new();for(index,field)in fields.iter().enumerate(){let field_expected=match &expected{Some(Ty::Tuple(fields))=>fields.get(index),_=>None};actuals.push(self.expression(module,context,field,env,bindings,field_expected,active,b)?);}Ty::Tuple(actuals)},
                CoreExprKind::Array(fields)=>{let contextual=expected.map(|ty|self.resolved(ty,b)).transpose()?;let mut child=match contextual{Some(Ty::Array(child,_))=>*child,_=>self.fresh(b)?};for field in fields{let actual=self.expression(module,context,field,env,bindings,Some(&child),active,b)?;self.unify(&actual,&child,b)?;child=self.resolved(&child,b)?;}Ty::Array(Box::new(child),Box::new(Const{carrier:Box::new(Ty::Atom(Type::Usize)),value:ConstValue::Value(Constant::Usize(fields.len() as u64))}))},
                CoreExprKind::Record{ty,fields}=>{let ty=self.ty(module,context,ty,env,active,b)?;self.record(module,context,&ty,fields,None,env,bindings,active,b)?;ty},
                CoreExprKind::RecordConstructor{definition,fields}=>{let ty=match expected{Some(ty)=>self.resolved(ty,b)?,None=>{let definition=definition.ok_or_else(||bad("original record constructor has no identity or context"))?;let row=context.definition(module,definition,b)?;if !row.parameters.is_empty(){return Err(unsupported("original generic record constructor needs full context inference",None));}Ty::Nominal(definition,vec![])}};if definition.is_some_and(|id|!matches!(ty,Ty::Nominal(actual,_)if id==actual)){return Err(bad("original record constructor changes declaration"));}self.record(module,context,&ty,fields,None,env,bindings,active,b)?;ty},
                CoreExprKind::RecordUpdate{ty,base,fields}=>{let annotation=ty.as_ref().map(|ty|self.ty(module,context,ty,env,active,b)).transpose()?;let base=self.expression(module,context,base,env,bindings,annotation.as_ref().or(expected),active,b)?;self.record(module,context,&base,fields,Some(&base),env,bindings,active,b)?;base},
                CoreExprKind::Sequence{bindings:initializers,value}=>{let before=self.live_keys(env,b)?;let mut locals=bindings.clone();for(name,initializer)in initializers{let actual=self.expression(module,context,initializer,env,&locals,None,active,b)?;let binding=Binding{ty:actual,mutable:false,identity:Binder::Local{definition:id,node:initializer.id,name:name.clone()}};self.declare(env,&binding,b)?;locals.insert(name.clone(),binding);}let value=self.expression(module,context,value,env,&locals,expected,active,b)?;self.exit_scope(env,&before,b)?;value},
                CoreExprKind::If{condition,then_value,else_value}=>{self.condition_original(module,context,condition,env,bindings,active,b)?;let before=self.use_snapshot(env,b)?;let yes=self.expression(module,context,then_value,env,bindings,expected,active,b)?;let yes_uses=self.use_snapshot(env,b)?;self.set_uses(env,before)?;let no=self.expression(module,context,else_value,env,bindings,Some(&yes),active,b)?;let no_uses=self.use_snapshot(env,b)?;self.unify(&yes,&no,b)?;self.join_uses(env,&yes_uses,&no_uses,b)?;yes},
                CoreExprKind::Closure{..}=>{
                    let original=context.definition(module,id,b)?;let SemanticDefinitionBody::FunctionWithBody{sites,site_nodes,..}=&original.body else{return Err(bad("closure creator has no original operations"));};
                    let mut children=site_nodes.iter().filter_map(|row|if row.node==SemanticOriginalNode::Expression(expression.id){sites.iter().find(|site|site.id==row.site).and_then(|site|if let SemanticSiteRecipeKind::Closure{definition,..}=site.kind{Some(definition)}else{None})}else{None});
                    let child=children.next().ok_or_else(||bad("original closure lacks its exact creation site"))?;if children.next().is_some(){return Err(bad("original closure has ambiguous child definitions"));}
                    if let Some(previous)=self.free_values.get(&child){if previous!=bindings{return Err(bad("original closure free values differ between creator sites"));}}else{self.free_values.insert(child,bindings.clone());}
                    let child_definition=context.definition(module,child,b)?;
                    for formal in &child_definition.environment{if let SemanticFormal::Type{requirements}=&formal.kind{for requirement in requirements{self.obligation(module,context,requirement,env,env,active,b)?;}}}
                    let CoreExprKind::Closure{move_capture,..}=expression.kind else{unreachable!()};self.creator_moves.insert(child,move_capture);
                    self.definition(module,context,child,active,b)?;
                    let CoreExprKind::Closure{move_capture,..}=&expression.kind else{unreachable!()};
                    let used=self.used.get(&child).cloned().unwrap_or_default();let consumed=self.uses.get(&child).cloned().unwrap_or_default();
                    for binding in bindings.values(){let child_key=Binder::Capture{definition:child,source:Box::new(binding.identity.clone())};if !used.contains(&child_key){continue;}
                        let moved=consumed.moved.iter().any(|place|place.binder==child_key);
                        if !*move_capture&&moved&&!self.is_copy(module,context,&binding.ty,env,active,b)?{return Err(unsupported("default closure capture cannot consume an unproved borrowed generic value",None));}
                        self.read_original(module,context,&Place{binder:binding.identity.clone(),fields:vec![]},&binding.ty,env,*move_capture,active,b)?;
                    }
                    let child_env=self.environment(module,context,child,active,b)?;let frame=child_env.lexical.first().ok_or_else(||bad("original child scope absent"))?;
                    Ty::Closure(child,frame.actuals.clone(),frame.environment.clone())
                },
                CoreExprKind::Try(value)=>match self.expression(module,context,value,env,bindings,None,active,b)?{Ty::ErrorUnion(_,result)=>*result,_=>return Err(bad("original propagation has no ErrorUnion operand"))},
                _=>return Err(unsupported("original expression has no complete symbolic operation rule",Some(expression.span))),
            };
            if let Some(expected)=expected{self.unify(&actual,expected,b)?;}
            self.expressions.insert((id,expression.id),actual.clone());Ok(actual)
        })
    }
    pub(super) fn literal_original(&self,module:&TypedModule,text:&str,floating:bool,expected:Option<&Ty>,b:&mut Budget)->Check<Ty>{
        let contextual=expected.map(|ty|self.resolved(ty,b)).transpose()?;
        let suffixes=[("isize",Type::Isize),("usize",Type::Usize),("i64",Type::I64),("i32",Type::I32),("i16",Type::I16),("i8",Type::I8),("u64",Type::U64),("u32",Type::U32),("u16",Type::U16),("u8",Type::U8),("f64",Type::F64),("f32",Type::F32)];
        let suffix=suffixes.iter().find(|(suffix,_)|text.ends_with(suffix)).map(|(_,ty)|ty.clone());
        let ty=match(suffix,contextual){(Some(ty),_)=>ty,(None,Some(Ty::Atom(ty)))if ty.is_integer()||ty.is_float()=>ty,_=>if floating{Type::F64}else{Type::I32}};
        let pattern=if floating{DynConstPattern::FloatLiteral(text.into())}else{DynConstPattern::IntegerLiteral(text.into())};scalar::closed(&pattern,&ty,module.target.pointer_width,b)?.ok_or_else(||bad("original literal carrier is unresolved"))?;Ok(Ty::Atom(ty))
    }
    pub(super) fn field(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,name:&str,_env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty>{
        let ty=self.resolved(ty,b)?;
        if let Ty::Tuple(fields)=&ty{let ordinal=name.parse::<usize>().map_err(|_|bad("original tuple field is not an ordinal"))?;if ordinal.to_string()!=name{return Err(bad("original tuple field ordinal spelling is noncanonical"));}return fields.get(ordinal).cloned().ok_or_else(||bad("original tuple field out of bounds"));}
        let Ty::Nominal(id,arguments)=ty else{return Err(unsupported("original field receiver is not a proved nominal product",None));};
        let definition=context.definition(module,id,b)?;let SemanticDefinitionBody::Nominal{body:SemanticNominalBody::Product{fields},..}=&definition.body else{return Err(bad("original field owner is not a product"));};
        let mut env=self.environment(module,context,id,active,b)?;env.lexical[0].actuals=arguments;
        let field=fields.iter().find(|field|field.name==name).ok_or_else(||bad("original nominal has no such field"))?;self.ty(module,context,&field.ty,&env,active,b)
    }
    #[allow(clippy::too_many_arguments)]
    fn record(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,fields:&[(String,SemanticOriginalExpr)],base:Option<&Ty>,env:&Env,bindings:&BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let Ty::Nominal(id,_)=self.resolved(ty,b)? else{return Err(bad("original record literal has no nominal identity"));};let definition=context.definition(module,id,b)?;let SemanticDefinitionBody::Nominal{body:SemanticNominalBody::Product{fields:declared},..}=&definition.body else{return Err(bad("original record literal targets nonproduct"));};
        let mut seen=BTreeSet::new();for(name,value)in fields{if !seen.insert(name){return Err(bad("duplicate original record initializer"));}let expected=self.field(module,context,ty,name,env,active,b)?;self.expression(module,context,value,env,bindings,Some(&expected),active,b)?;}
        if base.is_none()&&seen.len()!=declared.len(){return Err(bad("original record initializer omits a field"));}Ok(())
    }
}

impl CheckedOriginal {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn mutable_place(&mut self,module:&TypedModule,context:&CheckedContext,target:&SemanticOriginalExpr,env:&Env,bindings:&BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        b.spend(1)?;
        match &target.kind {
            CoreExprKind::Name(name)if bindings.get(name).is_some_and(|row|row.mutable)=>{let binding=bindings.get(name).unwrap();self.written.entry(env.current()?).or_default().insert(binding.identity.clone());Ok(())},
            CoreExprKind::Group(inner)=>self.mutable_place(module,context,inner,env,bindings,active,b),
            CoreExprKind::Unary{op:UnaryOp::Dereference,operand}=>{
                let(ty,place)=self.place(module,context,operand,env,bindings,active,b)?;if let Some(place)=place{self.read_original(module,context,&place,&ty,env,false,active,b)?;self.written.entry(env.current()?).or_default().insert(place.binder);}
                if matches!(self.resolved(&ty,b)?,Ty::Address{mutable:true,..}){Ok(())}else{Err(bad("original write/borrow-mut dereferences a nonexclusive Address"))}
            },
            CoreExprKind::Member{object,..}|CoreExprKind::Index{object,..}=>{
                let(ty,place)=self.place(module,context,object,env,bindings,active,b)?;if let Some(place)=place{self.read_original(module,context,&place,&ty,env,false,active,b)?;self.written.entry(env.current()?).or_default().insert(place.binder);}
                match self.resolved(&ty,b)?{
                    Ty::Address{mutable:true,..}=>Ok(()),
                    Ty::Address{mutable:false,..}|Ty::Desc{..}=>Err(bad("original projected write crosses a shared observation")),
                    _=>self.mutable_place(module,context,object,env,bindings,active,b),
                }
            },
            _=>Err(bad("original write does not select an exclusive address or mutable place")),
        }
    }
}
