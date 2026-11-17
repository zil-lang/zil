use super::*;
impl CheckedOriginal {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn expression(&mut self,module:&TypedModule,context:&CheckedContext,expression:&SemanticOriginalExpr,env:&Env,bindings:&BTreeMap<String,Binding>,expected:Option<&Ty>,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty>{
        let token=active.fresh(b)?;active.with_query(DependencyKey{kind:QueryKind::Type,token},b,|active,b|{
            let id=env.current()?;self.remember_names(env,SemanticOriginalNode::Expression(expression.id),bindings,b)?;
            if !matches!(expression.kind,CoreExprKind::If{..}|CoreExprKind::Sequence{..}) {
                if let Some(expected)=expected {
                    let sum=self.resolved(expected,b)?;
                    if let Ty::ErrorUnion(error,result)=&sum {
                        // Elaborating a contextual injection never equates
                        // the payload with the sum or one arm with the other.
                        let payload=self.expression(module,context,expression,env,bindings,None,active,b)?;
                        if self.same(&payload,&sum,b)?{return Ok(payload);}
                        let failed=self.same(&payload,error,b)?;
                        let succeeded=self.same(&payload,result,b)?;
                        if failed==succeeded{
                            let payload=self.resolved(&payload,b)?;
                            let error=self.resolved(error,b)?;
                            let result=self.resolved(result,b)?;
                            let detail=format!("original injection complete types: payload {payload:?}; error {error:?}; result {result:?}; matching arms error={failed}, result={succeeded}");
                            b.spend(detail.len())?;
                            return Err(bad("original implicit error injection has no unique complete payload arm").with_note(detail));
                        }
                        b.spend(2)?;
                        self.injections.insert((id,expression.id),(failed,payload));
                        self.expressions.insert((id,expression.id),sum.clone());
                        return Ok(sum);
                    }
                }
            }
            let shared_context=expected.map(|ty|self.resolved(ty,b)).transpose()?.is_some_and(|ty|matches!(ty,Ty::Address{mutable:false,..}));
            let actual=if shared_context&&matches!(expression.kind,CoreExprKind::Name(_)|CoreExprKind::Member{..}|CoreExprKind::ReceiverMember(_)|CoreExprKind::Index{..}|CoreExprKind::Unary{op:UnaryOp::Dereference,..}){
                let(actual,place)=self.place(module,context,expression,env,bindings,active,b)?;
                if matches!(self.resolved(&actual,b)?,Ty::Address{mutable:true,..}){if let Some(place)=&place{self.read_original(module,context,place,&actual,env,false,active,b)?;}}
                else{self.take_place(module,context,&actual,place.as_ref(),env,active,b)?;}
                actual
            }else{match &expression.kind{
                CoreExprKind::Unit=>Ty::Atom(Type::Unit),
                CoreExprKind::Bool(_)=>Ty::Atom(Type::Bool),
                CoreExprKind::Literal(value)=>Ty::Atom(value.ty()),
                CoreExprKind::Integer(text)|CoreExprKind::Float(text)=>self.literal_original(module,text,matches!(expression.kind,CoreExprKind::Float(_)),expected,b)?,
                CoreExprKind::String(_)=>Ty::Address{mutable:false,life:Life::Static,inner:Box::new(Ty::Slice(Box::new(Ty::Atom(Type::U8))))},
                CoreExprKind::Name(name)=>match bindings.get(name){Some(binding)=>{self.value_bindings.insert((id,expression.id),binding.identity.clone());let place=Place{binder:binding.identity.clone(),fields:vec![]};self.read_original(module,context,&place,&binding.ty,env,true,active,b)?;binding.identity.charge_clone(b)?;self.value_places.insert((id,expression.id),place);binding.ty.clone()},None=>match self.bound_name(module,context,name,env,active,b)?{Arg::Type(ty)=>ty,Arg::Const(value)=>*value.carrier,Arg::Life(_)=>return Err(bad("original value expression selects a lifetime name"))}},
                CoreExprKind::CompileTime(value)|CoreExprKind::Group(value)=>self.expression(module,context,value,env,bindings,expected,active,b)?,
                CoreExprKind::Ascribed{ty,value}=>{let expected=self.ty(module,context,ty,env,active,b)?;let actual=self.expression(module,context,value,env,bindings,Some(&expected),active,b)?;self.relate_value(&actual,&expected,b)?;expected},
                CoreExprKind::NumericConvert{value,target}|CoreExprKind::IntegerConvert{value,target}=>{let source=self.expression(module,context,value,env,bindings,None,active,b)?;let target=self.ty(module,context,target,env,active,b)?;if !matches!(self.resolved(&source,b)?,Ty::Atom(ref ty)if ty.is_integer()||ty.is_float())||!matches!(self.resolved(&target,b)?,Ty::Atom(ref ty)if ty.is_integer()||ty.is_float()){return Err(bad("original numeric conversion has nonnumeric endpoint"));}target},
                CoreExprKind::Unary{op:UnaryOp::Negate,operand}if matches!(operand.kind,CoreExprKind::Integer(_))=>{let CoreExprKind::Integer(text)=&operand.kind else{unreachable!()};let ty=self.literal_original_sign(module,text,false,true,expected,b)?;self.expressions.insert((id,operand.id),ty.clone());ty},
                CoreExprKind::Unary{op:UnaryOp::Borrow|UnaryOp::BorrowMut,operand}=>{
                    let mutable=matches!(expression.kind,CoreExprKind::Unary{op:UnaryOp::BorrowMut,..});
                    let(child,place)=self.place(module,context,operand,env,bindings,active,b)?;
                    if mutable{self.mutable_place(module,context,operand,env,bindings,active,b)?;}
                    if let Some(place)=place{self.read_original(module,context,&place,&child,env,false,active,b)?;}
                    let mut source=operand.as_ref();
                    while let CoreExprKind::Group(inner)|CoreExprKind::Ascribed{value:inner,..}=&source.kind{b.spend(1)?;source=inner;}
                    self.remember_borrow_parent(module,id,expression.id,source.id,b)?;
                    Ty::Address{mutable,life:Life::Local(id,expression.id),inner:Box::new(child)}
                },
                CoreExprKind::Unary{op:UnaryOp::Dereference,..}|CoreExprKind::Member{..}|CoreExprKind::Index{..}|CoreExprKind::ReceiverMember(_)=>{
                    if matches!(expression.kind,CoreExprKind::Member{..}) {
                        if let Some(value)=self.variant_constructor(module,context,expression,expression,None,None,&[],env,bindings,expected,active,b)? {
                            self.expressions.insert((id,expression.id),value.clone());return Ok(value);
                        }
                    }
                    if let Some(ty)=self.static_call_type(module,context,expression,env,bindings,b)?{ty}else{let(ty,place)=self.place(module,context,expression,env,bindings,active,b)?;self.take_place(module,context,&ty,place.as_ref(),env,active,b)?;ty}
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
                CoreExprKind::Binary{op,left,right}=>{
                    let operand_expected=if matches!(op,BinaryOp::Equal|BinaryOp::NotEqual|BinaryOp::Less|BinaryOp::LessEqual|BinaryOp::Greater|BinaryOp::GreaterEqual|BinaryOp::And|BinaryOp::Or){None}else{expected};
                    let(left,right)=if Self::contextual_numeric_literal(left,b)?{
                        // A pure untyped literal has no move, observation or
                        // effect to order. Obtain the other operand's carrier
                        // before applying the language's default literal type.
                        let right=self.expression(module,context,right,env,bindings,operand_expected,active,b)?;
                        let left=self.expression(module,context,left,env,bindings,Some(&right),active,b)?;(left,right)
                    }else{
                        let left=self.expression(module,context,left,env,bindings,operand_expected,active,b)?;
                        let right=self.expression(module,context,right,env,bindings,Some(&left),active,b)?;(left,right)
                    };
                    self.numeric_binary_original(*op,&left,&right,b)?
                },
                CoreExprKind::Call{callee,arguments}=>self.call(module,context,expression,callee,arguments,env,bindings,expected,active,b)?,
                CoreExprKind::Tuple(fields)=>{let expected=expected.map(|ty|self.resolved(ty,b)).transpose()?;let mut actuals=Vec::new();for(index,field)in fields.iter().enumerate(){let field_expected=match &expected{Some(Ty::Tuple(fields))=>fields.get(index),_=>None};actuals.push(self.expression(module,context,field,env,bindings,field_expected,active,b)?);}if actuals.is_empty(){Ty::Atom(Type::Unit)}else{Ty::Tuple(actuals)}},
                CoreExprKind::Array(fields)=>{let contextual=expected.map(|ty|self.resolved(ty,b)).transpose()?;let mut child=match contextual{Some(Ty::Array(child,_))=>*child,_=>self.fresh(b)?};for field in fields{let actual=self.expression(module,context,field,env,bindings,Some(&child),active,b)?;self.relate_value(&actual,&child,b)?;child=self.resolved(&child,b)?;}Ty::Array(Box::new(child),Box::new(Const{carrier:Box::new(Ty::Atom(Type::Usize)),value:ConstValue::Value(Constant::Usize(fields.len() as u64))}))},
                CoreExprKind::Record{ty,fields}=>{let ty=self.ty(module,context,ty,env,active,b)?;self.record(module,context,&ty,fields,None,env,bindings,active,b)?;self.constructed_resource(module,context,ty,expression,env,b)?},
                CoreExprKind::RecordConstructor{definition,fields}=>self.record_constructor(module,context,expression,*definition,fields,env,bindings,expected,active,b)?,
                CoreExprKind::RecordUpdate{ty,base,fields}=>{let annotation=ty.as_ref().map(|ty|self.ty(module,context,ty,env,active,b)).transpose()?;let base=self.expression(module,context,base,env,bindings,annotation.as_ref().or(expected),active,b)?;self.record(module,context,&base,fields,Some(&base),env,bindings,active,b)?;base},
                CoreExprKind::Sequence{bindings:initializers,value}=>{let before=self.live_keys(env,b)?;let mut locals=bindings.clone();for(name,initializer)in initializers{let actual=self.expression(module,context,initializer,env,&locals,None,active,b)?;let binding=Binding{ty:actual,mutable:false,identity:Binder::Local{definition:id,node:initializer.id,name:name.clone()}};self.declare(env,&binding,b)?;locals.insert(name.clone(),binding);}let value=self.expression(module,context,value,env,&locals,expected,active,b)?;self.exit_scope(env,&before,b)?;value},
                CoreExprKind::If{condition,then_value,else_value}=>{
                    self.condition_original(module,context,condition,env,bindings,active,b)?;let before=self.use_snapshot(env,b)?;
                    let(yes,yes_uses,no,no_uses)=if Self::contextual_numeric_literal(then_value,b)?{
                        // A pure unsuffixed arm has no value-use/effect order
                        // to execute. Collect its peer's carrier first, while
                        // checking both original arms from the same live Γ.
                        let no=self.expression(module,context,else_value,env,bindings,expected,active,b)?;let no_uses=self.use_snapshot(env,b)?;self.set_uses(env,before)?;
                        let yes=self.expression(module,context,then_value,env,bindings,Some(&no),active,b)?;let yes_uses=self.use_snapshot(env,b)?;(yes,yes_uses,no,no_uses)
                    }else{
                        let yes=self.expression(module,context,then_value,env,bindings,expected,active,b)?;let yes_uses=self.use_snapshot(env,b)?;self.set_uses(env,before)?;
                        let no=self.expression(module,context,else_value,env,bindings,Some(&yes),active,b)?;let no_uses=self.use_snapshot(env,b)?;(yes,yes_uses,no,no_uses)
                    };
                    self.unify(&yes,&no,b)?;self.join_uses(env,&yes_uses,&no_uses,b)?;yes
                },
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
            }};
            let actual=self.code_value(module,context,expression,&actual,expected,false,env,active,b)?;
            if let Some(expected)=expected{self.relate_value(&actual,expected,b)?;}
            if self.creation(id,expression,bindings,b)?{b.spend(1)?;self.created.insert((id,expression.id));}
            self.expressions.insert((id,expression.id),actual.clone());if let Some(expected)=expected{self.contextual_borrow_value(&actual,expected,b)}else{Ok(actual)}
        }).map_err(|mut error|{error.message.push_str(&format!(" [original definition {:?}, expression {:?}]",env.current().ok(),expression.id));if error.span.is_none(){error.span=Some(expression.span);}error})
    }
    pub(super) fn literal_original(&self,module:&TypedModule,text:&str,floating:bool,expected:Option<&Ty>,b:&mut Budget)->Check<Ty>{
        self.literal_original_sign(module,text,floating,false,expected,b)
    }
    fn literal_original_sign(&self,module:&TypedModule,text:&str,floating:bool,negate:bool,expected:Option<&Ty>,b:&mut Budget)->Check<Ty>{
        // Integer patterns can carry their original leading minus; ordinary
        // expressions retain UnaryNeg. Feed both to the existing exact-width
        // unary evaluator, never a signed string to the magnitude parser.
        b.spend(text.len().saturating_add(3))?;
        let(text,negate)=if !floating{match text.strip_prefix('-'){Some(text)=>(text,!negate),None=>(text,negate)}}else{(text,negate)};
        let contextual=expected.map(|ty|self.resolved(ty,b)).transpose()?;
        let suffixes=[("isize",Type::Isize),("usize",Type::Usize),("i64",Type::I64),("i32",Type::I32),("i16",Type::I16),("i8",Type::I8),("u64",Type::U64),("u32",Type::U32),("u16",Type::U16),("u8",Type::U8),("f64",Type::F64),("f32",Type::F32)];
        let suffix=suffixes.iter().find(|(suffix,_)|text.ends_with(suffix)).map(|(_,ty)|ty.clone());
        let ty=match(suffix,contextual){(Some(ty),_)=>ty,(None,Some(Ty::Atom(ty)))if ty.is_integer()||ty.is_float()=>ty,_=>if floating{Type::F64}else{Type::I32}};
        let mut pattern=if floating{DynConstPattern::FloatLiteral(text.into())}else{DynConstPattern::IntegerLiteral(text.into())};
        if negate{pattern=DynConstPattern::Unary{op:UnaryOp::Negate,ty:Box::new(DynTypePattern::Concrete(ty.clone())),value:Box::new(pattern)};}
        scalar::closed(&pattern,&ty,module.target.pointer_width,b)?.ok_or_else(||bad("original literal carrier is unresolved"))?;Ok(Ty::Atom(ty))
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
    fn write_parent_type(&mut self,module:&TypedModule,context:&CheckedContext,expression:&SemanticOriginalExpr,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty>{
        let definition=env.current()?;
        b.spend(self.place_types.len().checked_ilog2().map_or(1,|n|n as usize+1))?;
        let ty=self.resolved(self.place_types.get(&(definition,expression.id)).ok_or_else(||bad("original mutable projection has no evaluated parent Place type"))?,b)?;
        if let Some(place)=self.value_places.get(&(definition,expression.id)){
            place.binder.charge_clone(b)?;b.spend(place.fields.iter().fold(1usize,|n,field|n.saturating_add(field.len()).saturating_add(1)))?;
            let place=place.clone();
            self.read_original_parent(module,context,&place,&ty,env,active,b)?;
            self.written.entry(definition).or_default().insert(place.binder);
        }
        self.resolved(&ty,b)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn mutable_place(&mut self,module:&TypedModule,context:&CheckedContext,target:&SemanticOriginalExpr,env:&Env,bindings:&BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        // place/expression has evaluated this exact target once already. This
        // pass proves lexical/exclusive mutability from its checked types; it
        // must not execute a base, index or call expression a second time.
        b.spend(1)?;
        match &target.kind {
            CoreExprKind::Name(name)if bindings.get(name).is_some_and(|row|row.mutable)=>{let binding=bindings.get(name).unwrap();self.written.entry(env.current()?).or_default().insert(binding.identity.clone());Ok(())},
            CoreExprKind::Group(inner)=>self.mutable_place(module,context,inner,env,bindings,active,b),
            CoreExprKind::Unary{op:UnaryOp::Dereference,operand}=>{
                let ty=self.write_parent_type(module,context,operand,env,active,b)?;
                if matches!(ty,Ty::Address{mutable:true,..}){Ok(())}else{Err(bad("original write/borrow-mut dereferences a nonexclusive Address"))}
            },
            CoreExprKind::Member{object,..}|CoreExprKind::Index{object,..}|CoreExprKind::Slice{object,..}=>{
                let ty=self.write_parent_type(module,context,object,env,active,b)?;
                match ty{
                    Ty::Address{mutable:true,..}=>Ok(()),
                    Ty::Address{mutable:false,..}|Ty::Desc{..}=>Err(bad("original projected write crosses a shared observation")),
                    _=>self.mutable_place(module,context,object,env,bindings,active,b),
                }
            },
            _=>Err(bad("original write does not select an exclusive address or mutable place")),
        }
    }
}
