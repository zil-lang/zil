use super::*;
impl CheckedOriginal {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn expression(&mut self,module:&TypedModule,context:&CheckedContext,expression:&SemanticOriginalExpr,env:&Env,bindings:&BTreeMap<String,Binding>,expected:Option<&Ty>,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty>{
        let token=active.fresh(b)?;active.with_query(DependencyKey{kind:QueryKind::Type,token},b,|active,b|{
            let id=env.current()?;
            let actual=match &expression.kind{
                CoreExprKind::Unit=>Ty::Atom(Type::Unit),
                CoreExprKind::Bool(_)=>Ty::Atom(Type::Bool),
                CoreExprKind::Literal(value)=>Ty::Atom(value.ty()),
                CoreExprKind::Integer(text)|CoreExprKind::Float(text)=>self.literal_original(module,text,matches!(expression.kind,CoreExprKind::Float(_)),expected,b)?,
                CoreExprKind::String(_)=>Ty::Address{mutable:false,life:Life::Static,inner:Box::new(Ty::Slice(Box::new(Ty::Atom(Type::U8))))},
                CoreExprKind::Name(name)=>match bindings.get(name){Some(binding)=>binding.ty.clone(),None=>Ty::Function(self.resolve_function(module,context,id,name,b)?)},
                CoreExprKind::Group(value)=>self.expression(module,context,value,env,bindings,expected,active,b)?,
                CoreExprKind::Ascribed{ty,value}=>{let expected=self.ty(module,context,ty,env,active,b)?;let actual=self.expression(module,context,value,env,bindings,Some(&expected),active,b)?;self.unify(&actual,&expected,b)?;expected},
                CoreExprKind::NumericConvert{value,target}|CoreExprKind::IntegerConvert{value,target}=>{let source=self.expression(module,context,value,env,bindings,None,active,b)?;let target=self.ty(module,context,target,env,active,b)?;if !matches!(self.resolved(&source,b)?,Ty::Atom(ref ty)if ty.is_integer()||ty.is_float())||!matches!(self.resolved(&target,b)?,Ty::Atom(ref ty)if ty.is_integer()||ty.is_float()){return Err(bad("original numeric conversion has nonnumeric endpoint"));}target},
                CoreExprKind::Unary{op:UnaryOp::Negate,operand}if matches!(operand.kind,CoreExprKind::Integer(_))=>{let CoreExprKind::Integer(text)=&operand.kind else{unreachable!()};let ty=self.literal_original(module,&format!("-{text}"),false,expected,b)?;self.expressions.insert((id,operand.id),ty.clone());ty},
                CoreExprKind::Unary{op,operand}=>{
                    let child=self.expression(module,context,operand,env,bindings,if matches!(op,UnaryOp::Negate|UnaryOp::BitNot|UnaryOp::Not){expected}else{None},active,b)?;
                    match op{
                        UnaryOp::Borrow|UnaryOp::BorrowMut=>{if *op==UnaryOp::BorrowMut{self.mutable_place(module,context,operand,env,bindings,active,b)?;}Ty::Address{mutable:*op==UnaryOp::BorrowMut,life:Life::Local(id,expression.id),inner:Box::new(child)}},
                        UnaryOp::Dereference=>match self.resolved(&child,b)?{Ty::Address{inner,..}=>*inner,_=>return Err(bad("original dereference has no address operand"))},
                        UnaryOp::Give=>child,
                        UnaryOp::Not=>{self.unify(&child,&Ty::Atom(Type::Bool),b)?;Ty::Atom(Type::Bool)},
                        UnaryOp::Negate|UnaryOp::BitNot=>{if !matches!(self.resolved(&child,b)?,Ty::Atom(ref ty)if ty.is_integer()||(*op==UnaryOp::Negate&&ty.is_float())){return Err(bad("original unary scalar operation is not proved"));}child},
                    }
                },
                CoreExprKind::Binary{op,left,right}=>{let operand_expected=if matches!(op,BinaryOp::Equal|BinaryOp::NotEqual|BinaryOp::Less|BinaryOp::LessEqual|BinaryOp::Greater|BinaryOp::GreaterEqual|BinaryOp::And|BinaryOp::Or){None}else{expected};let left=self.expression(module,context,left,env,bindings,operand_expected,active,b)?;let right=self.expression(module,context,right,env,bindings,Some(&left),active,b)?;self.numeric_binary_original(*op,&left,&right,b)?},
                CoreExprKind::Call{callee,arguments}=>self.call(module,context,expression,callee,arguments,env,bindings,expected,active,b)?,
                CoreExprKind::Tuple(fields)=>{let expected=expected.map(|ty|self.resolved(ty,b)).transpose()?;let mut actuals=Vec::new();for(index,field)in fields.iter().enumerate(){let field_expected=match &expected{Some(Ty::Tuple(fields))=>fields.get(index),_=>None};actuals.push(self.expression(module,context,field,env,bindings,field_expected,active,b)?);}Ty::Tuple(actuals)},
                CoreExprKind::Array(fields)=>{let contextual=expected.map(|ty|self.resolved(ty,b)).transpose()?;let mut child=match contextual{Some(Ty::Array(child,_))=>*child,_=>self.fresh(b)?};for field in fields{let actual=self.expression(module,context,field,env,bindings,Some(&child),active,b)?;self.unify(&actual,&child,b)?;child=self.resolved(&child,b)?;}Ty::Array(Box::new(child),Box::new(Const{carrier:Box::new(Ty::Atom(Type::Usize)),value:ConstValue::Value(Constant::Usize(fields.len() as u64))}))},
                CoreExprKind::Index{object,index}=>{let object=self.expression(module,context,object,env,bindings,None,active,b)?;let index=self.expression(module,context,index,env,bindings,Some(&Ty::Atom(Type::Usize)),active,b)?;if !matches!(self.resolved(&index,b)?,Ty::Atom(ref ty)if ty.is_integer()){return Err(bad("original index is not an integer"));}let mut object=self.resolved(&object,b)?;if let Ty::Address{inner,..}=object{object=*inner;}match object{Ty::Array(child,_)|Ty::Slice(child)=>*child,_=>return Err(bad("original indexing requires an actual array/slice"))}},
                CoreExprKind::Member{object,member,owner}=>{let object=self.expression(module,context,object,env,bindings,None,active,b)?;let mut object=self.resolved(&object,b)?;if let Ty::Address{inner,..}|Ty::Desc{inner,..}=object{object=*inner;}if let Some(owner)=owner{let owner=self.ty(module,context,owner,env,active,b)?;self.unify(&object,&owner,b)?;}self.field(module,context,&object,member,env,active,b)?},
                CoreExprKind::Record{ty,fields}=>{let ty=self.ty(module,context,ty,env,active,b)?;self.record(module,context,&ty,fields,None,env,bindings,active,b)?;ty},
                CoreExprKind::RecordConstructor{definition,fields}=>{let ty=match expected{Some(ty)=>self.resolved(ty,b)?,None=>{let definition=definition.ok_or_else(||bad("original record constructor has no identity or context"))?;let row=context.definition(module,definition,b)?;if !row.parameters.is_empty(){return Err(unsupported("original generic record constructor needs full context inference",None));}Ty::Nominal(definition,vec![])}};if definition.is_some_and(|id|!matches!(ty,Ty::Nominal(actual,_)if id==actual)){return Err(bad("original record constructor changes declaration"));}self.record(module,context,&ty,fields,None,env,bindings,active,b)?;ty},
                CoreExprKind::RecordUpdate{ty,base,fields}=>{let annotation=ty.as_ref().map(|ty|self.ty(module,context,ty,env,active,b)).transpose()?;let base=self.expression(module,context,base,env,bindings,annotation.as_ref().or(expected),active,b)?;self.record(module,context,&base,fields,Some(&base),env,bindings,active,b)?;base},
                CoreExprKind::Sequence{bindings:initializers,value}=>{let mut locals=bindings.clone();for(name,initializer)in initializers{let actual=self.expression(module,context,initializer,env,&locals,None,active,b)?;locals.insert(name.clone(),Binding{ty:actual,mutable:false});}self.expression(module,context,value,env,&locals,expected,active,b)?},
                CoreExprKind::If{condition,then_value,else_value}=>{self.condition_original(module,context,condition,env,bindings,active,b)?;let yes=self.expression(module,context,then_value,env,bindings,expected,active,b)?;let no=self.expression(module,context,else_value,env,bindings,Some(&yes),active,b)?;self.unify(&yes,&no,b)?;yes},
                CoreExprKind::Closure{..}=>{
                    let original=context.definition(module,id,b)?;let SemanticDefinitionBody::FunctionWithBody{sites,site_nodes,..}=&original.body else{return Err(bad("closure creator has no original operations"));};
                    let mut children=site_nodes.iter().filter_map(|row|if row.node==SemanticOriginalNode::Expression(expression.id){sites.iter().find(|site|site.id==row.site).and_then(|site|if let SemanticSiteRecipeKind::Closure{definition,..}=site.kind{Some(definition)}else{None})}else{None});
                    let child=children.next().ok_or_else(||bad("original closure lacks its exact creation site"))?;if children.next().is_some(){return Err(bad("original closure has ambiguous child definitions"));}
                    if let Some(previous)=self.free_values.get(&child){if previous!=bindings{return Err(bad("original closure free values differ between creator sites"));}}else{self.free_values.insert(child,bindings.clone());}
                    self.definition(module,context,child,active,b)?;
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
    pub(super) fn resolve_function(&self,module:&TypedModule,context:&CheckedContext,caller:SemanticDefinitionId,name:&str,b:&mut Budget)->Check<SemanticDefinitionId>{
        let original=context.definition(module,caller,b)?;let program=module.semantic.as_ref().ok_or_else(||bad("original program absent"))?;b.spend(program.definitions.len())?;
        let names=name.split("::").collect::<Vec<_>>();let mut matches=Vec::new();
        for definition in &program.definitions{if !matches!(definition.kind,SemanticDefinitionKind::Function|SemanticDefinitionKind::Method){continue;}let identity=&definition.identity;if identity.package!=original.identity.package{continue;}let matched=if names.len()==1{identity.declaration_path.last().is_some_and(|last|last==name)&&identity.module_path==original.identity.module_path&&definition.parent==None}else{identity.module_path.iter().chain(&identity.declaration_path).map(String::as_str).eq(names.iter().copied())};if matched{matches.push(definition.id);}}
        if matches.len()!=1{return Err(unsupported("original function name has no unique lexical definition/import resolution",None));}Ok(matches[0])
    }
    pub(super) fn field(&mut self,module:&TypedModule,context:&CheckedContext,ty:&Ty,name:&str,_env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Ty>{
        let ty=self.resolved(ty,b)?;
        if let Ty::Tuple(fields)=&ty{let ordinal=name.parse::<usize>().map_err(|_|bad("original tuple field is not an ordinal"))?;return fields.get(ordinal).cloned().ok_or_else(||bad("original tuple field out of bounds"));}
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
            CoreExprKind::Name(name)if bindings.get(name).is_some_and(|row|row.mutable)=>Ok(()),
            CoreExprKind::Group(inner)=>self.mutable_place(module,context,inner,env,bindings,active,b),
            CoreExprKind::Unary{op:UnaryOp::Dereference,operand}=>{
                let ty=self.expression(module,context,operand,env,bindings,None,active,b)?;
                if matches!(self.resolved(&ty,b)?,Ty::Address{mutable:true,..}){Ok(())}else{Err(bad("original write/borrow-mut dereferences a nonexclusive Address"))}
            },
            CoreExprKind::Member{object,..}|CoreExprKind::Index{object,..}=>{
                let ty=self.expression(module,context,object,env,bindings,None,active,b)?;
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
