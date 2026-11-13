use super::*;
impl CheckedOriginal {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn pattern(&mut self,module:&TypedModule,context:&CheckedContext,pattern:&SemanticOriginalPattern,subject:&Ty,mutable:bool,env:&Env,bindings:&mut BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let token=active.fresh(b)?;active.with_query(DependencyKey{kind:QueryKind::Type,token},b,|active,b|{
            match &pattern.kind {
                CorePatternKind::Wildcard=>{},
                CorePatternKind::Name(name)=>{bindings.insert(name.clone(),Binding{ty:subject.clone(),mutable});},
                CorePatternKind::Tuple(patterns)=>{let Ty::Tuple(fields)=self.resolved(subject,b)? else{return Err(bad("original tuple pattern has non-Tuple subject"));};if fields.len()!=patterns.len(){return Err(bad("original tuple pattern arity differs"));}for(pattern,field)in patterns.iter().zip(fields){self.pattern(module,context,pattern,&field,mutable,env,bindings,active,b)?;}},
                CorePatternKind::Reference{mutable:exclusive,pattern}=>{let Ty::Address{mutable:actual,inner,..}=self.resolved(subject,b)? else{return Err(bad("original reference pattern has no Address subject"));};if *exclusive&&!actual{return Err(bad("original mutable pattern upgrades shared Address"));}self.pattern(module,context,pattern,&inner,mutable,env,bindings,active,b)?;},
                CorePatternKind::Record{ty,fields}=>{let expected=self.ty(module,context,ty,env,active,b)?;self.unify(subject,&expected,b)?;let mut seen=BTreeSet::new();for(name,pattern)in fields{if !seen.insert(name){return Err(bad("duplicate original record pattern field"));}let field=self.field(module,context,subject,name,env,active,b)?;self.pattern(module,context,pattern,&field,mutable,env,bindings,active,b)?;}},
                CorePatternKind::RecordConstructor{definition,fields}=>{let Ty::Nominal(actual,_)=self.resolved(subject,b)? else{return Err(bad("original record pattern subject lacks nominal identity"));};if definition.is_some_and(|id|id!=actual){return Err(bad("original record pattern changes declaration"));}let mut seen=BTreeSet::new();for(name,pattern)in fields{if !seen.insert(name){return Err(bad("duplicate original record pattern field"));}let field=self.field(module,context,subject,name,env,active,b)?;self.pattern(module,context,pattern,&field,mutable,env,bindings,active,b)?;}},
                CorePatternKind::ErrorArm{is_error,pattern}=>{let Ty::ErrorUnion(error,result)=self.resolved(subject,b)? else{return Err(bad("original ErrorArm subject is not ErrorUnion"));};self.pattern(module,context,pattern,if *is_error{&error}else{&result},mutable,env,bindings,active,b)?;},
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
        for(pattern,recipe)in patterns.iter().zip(&variant.fields){let ty=self.ty(module,context,recipe,&selected,active,b)?;self.pattern(module,context,pattern,&ty,mutable,env,bindings,active,b)?;}Ok(())
    }
}
