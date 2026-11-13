use super::*;
impl CheckedOriginal {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn pattern(&mut self,module:&TypedModule,context:&CheckedContext,pattern:&SemanticOriginalPattern,subject:&Ty,mutable:bool,env:&Env,bindings:&mut BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let token=active.fresh(b)?;active.with_query(DependencyKey{kind:QueryKind::Type,token},b,|active,b|{
            match &pattern.kind {
                CorePatternKind::Wildcard=>{},
                CorePatternKind::Name(name)=>{if let Some((node,path))=&self.pattern_origin{b.spend(path.len())?;self.pattern_types.insert((env.current()?,*node,path.clone()),subject.clone());}let binding=Binding{ty:subject.clone(),mutable,identity:Binder::Pattern{definition:env.current()?,token:active.fresh(b)?}};self.declare(env,&binding,b)?;bindings.insert(name.clone(),binding);},
                CorePatternKind::Tuple(patterns)=>{let Ty::Tuple(fields)=self.resolved(subject,b)? else{return Err(bad("original tuple pattern has non-Tuple subject"));};if fields.len()!=patterns.len(){return Err(bad("original tuple pattern arity differs"));}for(index,(pattern,field))in patterns.iter().zip(fields).enumerate(){self.pattern_child(module,context,index as u32,pattern,&field,mutable,env,bindings,active,b)?;}},
                CorePatternKind::Reference{mutable:exclusive,pattern}=>{let Ty::Address{mutable:actual,inner,..}=self.resolved(subject,b)? else{return Err(bad("original reference pattern has no Address subject"));};if *exclusive&&!actual{return Err(bad("original mutable pattern upgrades shared Address"));}self.pattern_child(module,context,0,pattern,&inner,mutable,env,bindings,active,b)?;},
                CorePatternKind::Record{ty,fields}=>{let expected=self.ty(module,context,ty,env,active,b)?;self.unify(subject,&expected,b)?;let mut seen=BTreeSet::new();for(index,(name,pattern))in fields.iter().enumerate(){if !seen.insert(name){return Err(bad("duplicate original record pattern field"));}let field=self.field(module,context,subject,name,env,active,b)?;self.pattern_child(module,context,index as u32,pattern,&field,mutable,env,bindings,active,b)?;}},
                CorePatternKind::RecordConstructor{definition,fields}=>{let Ty::Nominal(actual,_)=self.resolved(subject,b)? else{return Err(bad("original record pattern subject lacks nominal identity"));};if definition.is_some_and(|id|id!=actual){return Err(bad("original record pattern changes declaration"));}let mut seen=BTreeSet::new();for(index,(name,pattern))in fields.iter().enumerate(){if !seen.insert(name){return Err(bad("duplicate original record pattern field"));}let field=self.field(module,context,subject,name,env,active,b)?;self.pattern_child(module,context,index as u32,pattern,&field,mutable,env,bindings,active,b)?;}},
                CorePatternKind::ErrorArm{is_error,pattern}=>{let Ty::ErrorUnion(error,result)=self.resolved(subject,b)? else{return Err(bad("original ErrorArm subject is not ErrorUnion"));};self.pattern_child(module,context,0,pattern,if *is_error{&error}else{&result},mutable,env,bindings,active,b)?;},
                CorePatternKind::Variant{ty,variant,fields}=>{let expected=self.ty(module,context,ty,env,active,b)?;self.unify(subject,&expected,b)?;self.variant_pattern(module,context,subject,variant,fields,mutable,env,bindings,active,b)?;},
                CorePatternKind::Constructor{definition,variant,fields}=>{let Ty::Nominal(actual,_)=self.resolved(subject,b)? else{return Err(bad("original variant subject lacks nominal identity"));};if definition.is_some_and(|id|id!=actual){return Err(bad("original variant changes declaration"));}self.variant_pattern(module,context,subject,variant,fields,mutable,env,bindings,active,b)?;},
                CorePatternKind::Integer(text)=>{let ty=self.literal_original(module,text,false,Some(subject),b)?;self.unify(subject,&ty,b)?;},
                CorePatternKind::Float(text)=>{let ty=self.literal_original(module,text,true,Some(subject),b)?;self.unify(subject,&ty,b)?;},
                CorePatternKind::Bool(_)=>self.unify(subject,&Ty::Atom(Type::Bool),b)?,
                _=>return Err(unsupported("original pattern needs its complete symbolic literal/constructor rule",Some(pattern.span))),
            }Ok(())
        })
    }
    #[allow(clippy::too_many_arguments)]
    fn variant_pattern(&mut self,module:&TypedModule,context:&CheckedContext,subject:&Ty,name:&str,patterns:&[SemanticOriginalPattern],mutable:bool,env:&Env,bindings:&mut BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let Ty::Nominal(id,arguments)=self.resolved(subject,b)? else{return Err(bad("original variant subject is not nominal"));};
        let definition=context.definition(module,id,b)?;let SemanticDefinitionBody::Nominal{body:SemanticNominalBody::Enum{variants},..}=&definition.body else{return Err(bad("original variant owner is not Enum"));};let variant=variants.iter().find(|variant|variant.name==name).ok_or_else(||bad("original enum has no such variant"))?;if variant.fields.len()!=patterns.len(){return Err(bad("original variant pattern arity differs"));}
        let mut selected=self.environment(module,context,id,active,b)?;selected.lexical[0].actuals=arguments;
        for(index,(pattern,recipe))in patterns.iter().zip(&variant.fields).enumerate(){let ty=self.ty(module,context,recipe,&selected,active,b)?;self.pattern_child(module,context,index as u32,pattern,&ty,mutable,env,bindings,active,b)?;}Ok(())
    }
}

impl CheckedOriginal {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn pattern_root(&mut self,module:&TypedModule,context:&CheckedContext,node:NodeId,path:&[u32],pattern:&SemanticOriginalPattern,subject:&Ty,mutable:bool,env:&Env,bindings:&mut BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let mut pending=vec![pattern];let mut names=BTreeSet::new();
        while let Some(pattern)=pending.pop(){b.spend(1)?;match &pattern.kind{
            CorePatternKind::Name(name)=>{b.spend(name.len())?;if !names.insert(name){return Err(bad("original pattern binds the same value name twice"));}},
            CorePatternKind::Tuple(fields)|CorePatternKind::Variant{fields,..}|CorePatternKind::Constructor{fields,..}=>pending.extend(fields),
            CorePatternKind::Record{fields,..}|CorePatternKind::RecordConstructor{fields,..}=>pending.extend(fields.iter().map(|(_,pattern)|pattern)),
            CorePatternKind::Reference{pattern,..}|CorePatternKind::ErrorArm{pattern,..}=>pending.push(pattern),_=>{},
        }}
        b.spend(path.len())?;let previous=self.pattern_origin.replace((node,path.to_vec()));
        let result=self.pattern(module,context,pattern,subject,mutable,env,bindings,active,b);self.pattern_origin=previous;result
    }
    #[allow(clippy::too_many_arguments)]
    fn pattern_child(&mut self,module:&TypedModule,context:&CheckedContext,index:u32,pattern:&SemanticOriginalPattern,subject:&Ty,mutable:bool,env:&Env,bindings:&mut BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let(node,mut path)=self.pattern_origin.clone().ok_or_else(||bad("original pattern child has no actual binder root"))?;b.spend(path.len().saturating_add(1))?;path.push(index);
        let previous=self.pattern_origin.replace((node,path));let result=self.pattern(module,context,pattern,subject,mutable,env,bindings,active,b);self.pattern_origin=previous;result
    }
}
