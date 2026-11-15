use super::*;
impl CheckedOriginal {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn pattern(&mut self,module:&TypedModule,context:&CheckedContext,pattern:&SemanticOriginalPattern,subject:&Ty,mutable:bool,env:&Env,bindings:&mut BTreeMap<String,Binding>,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let token=active.fresh(b)?;active.with_query(DependencyKey{kind:QueryKind::Type,token},b,|active,b|{
            if let Some((node,path))=&self.pattern_origin{b.spend(path.len().saturating_add(1))?;self.pattern_types.insert((env.current()?,*node,path.clone()),subject.clone());}
            match &pattern.kind {
                CorePatternKind::Wildcard=>{},
                CorePatternKind::Name(name)=>{let identity=Binder::Pattern{definition:env.current()?,token:active.fresh(b)?};if let Some((node,path))=&self.pattern_origin{b.spend(path.len().saturating_mul(2).saturating_add(1))?;self.pattern_types.insert((env.current()?,*node,path.clone()),subject.clone());self.pattern_bindings.insert((env.current()?,*node,path.clone()),identity.clone());}let binding=Binding{ty:subject.clone(),mutable,identity};self.declare(env,&binding,b)?;bindings.insert(name.clone(),binding);},
                CorePatternKind::Tuple(patterns)=>{let fields=match self.resolved(subject,b)?{Ty::Tuple(fields)=>fields,Ty::Atom(Type::Unit)if patterns.is_empty()=>vec![],_=>return Err(bad("original tuple pattern has non-Tuple subject"))};if fields.len()!=patterns.len(){return Err(bad("original tuple pattern arity differs"));}for(index,(pattern,field))in patterns.iter().zip(fields).enumerate(){self.pattern_child(module,context,index as u32,pattern,&field,mutable,env,bindings,active,b)?;}},
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
    pub(in crate::semantic) fn pattern_place(&self,module:&TypedModule,definition:SemanticDefinitionId,node:NodeId,b:&mut Budget)->Check<(NodeId,Vec<PlaceProjection>,String,Span)>{
        let place=self.value_places.get(&(definition,node)).ok_or_else(||bad("pattern observation has no independently checked original Place"))?;
        let Binder::Pattern{definition:owner,..}=&place.binder else{return Err(bad("pattern observation changes its original binder category"));};
        if *owner!=definition{return Err(bad("pattern observation changes its original lexical owner"));}
        let mut selected=None;
        for((scope,statement,path),binder)in &self.pattern_bindings{b.spend(path.len().saturating_add(1))?;if *scope==definition&&binder==&place.binder{if selected.replace((*statement,path.clone())).is_some(){return Err(bad("pattern binder has multiple original binding occurrences"));}}}
        let(statement,path)=selected.ok_or_else(||bad("pattern binder lacks its complete original occurrence"))?;
        let index=self.source_index(module,&[definition],b)?;let function=index.definitions.get(&definition).ok_or_else(||bad("pattern owner has no original body"))?;
        let statement=function.statements.get(&statement).ok_or_else(||bad("pattern binding statement is absent"))?;
        let(subject,mut pattern,mut consumed)=match &statement.kind{
            CoreStatementKind::Match{value,arms}=>{let arm=*path.first().ok_or_else(||bad("match binder omits its original arm ordinal"))?;let selected=arms.get(arm as usize).ok_or_else(||bad("match binder arm is absent"))?;(value,&selected.pattern,1usize)},
            CoreStatementKind::IfLet{value,pattern,..}|CoreStatementKind::WhileLet{value,pattern,..}=>(value,pattern,0),
            CoreStatementKind::TupleBinding{initializer,pattern,..}=>(initializer,pattern,0),
            _=>return Err(bad("pattern binder selects another original statement role")),
        };
        let mut projections=Vec::new();
        while consumed<path.len(){
            b.spend(1)?;let child=path[consumed] as usize;
            let parent=self.pattern_types.get(&(definition,statement.id,path[..consumed].to_vec())).ok_or_else(||bad("pattern projection lacks its independently checked whole parent type"))?;
            pattern=match &pattern.kind{
                CorePatternKind::ErrorArm{is_error,pattern}if child==0=>{projections.push(PlaceProjection::ErrorArm(*is_error));pattern},
                CorePatternKind::Tuple(fields)=>{projections.push(PlaceProjection::Field(child as u32));fields.get(child).ok_or_else(||bad("tuple pattern child is absent"))?},
                CorePatternKind::Variant{variant,fields,..}|CorePatternKind::Constructor{variant,fields,..}=>{
                    let Ty::Nominal(owner,_)=self.resolved(parent,b)? else{return Err(bad("variant pattern parent has no exact original nominal identity"));};
                    let definition=function_definition(module,owner,b)?;let SemanticDefinitionBody::Nominal{body:SemanticNominalBody::Enum{variants},..}=&definition.body else{return Err(bad("variant pattern owner is not its original Enum"));};
                    b.spend(variants.len())?;let ordinal=variants.iter().position(|row|row.name==*variant).ok_or_else(||bad("pattern original variant is absent"))?;
                    projections.push(PlaceProjection::Variant(VariantProjection{variant:ordinal as u32,index:child as u32}));fields.get(child).ok_or_else(||bad("variant pattern child is absent"))?
                },
                CorePatternKind::Record{fields,..}|CorePatternKind::RecordConstructor{fields,..}=>{
                    let(name,pattern)=fields.get(child).ok_or_else(||bad("record pattern child is absent"))?;
                    let Ty::Nominal(owner,_)=self.resolved(parent,b)? else{return Err(bad("record pattern parent lacks its exact original nominal identity"));};
                    let definition=function_definition(module,owner,b)?;let SemanticDefinitionBody::Nominal{body:SemanticNominalBody::Product{fields},..}=&definition.body else{return Err(bad("record pattern owner is not its original product"));};
                    b.spend(fields.len())?;let ordinal=fields.iter().position(|row|row.name==*name).ok_or_else(||bad("record pattern field is absent"))?;
                    projections.push(PlaceProjection::Field(ordinal as u32));pattern
                },
                _=>return Err(unsupported("pattern source requires its complete original physical projection rule",Some(pattern.span))),
            };consumed+=1;
        }
        let CorePatternKind::Name(name)=&pattern.kind else{return Err(bad("pattern source does not end at its actual original value binder"));};
        b.spend(name.len())?;Ok((subject.id,projections,name.clone(),pattern.span))
    }
}

fn function_definition<'a>(module:&'a TypedModule,id:SemanticDefinitionId,b:&mut Budget)->Check<&'a SemanticDefinition>{
    let definitions=&module.semantic.as_ref().ok_or_else(||bad("pattern nominal original programme is absent"))?.definitions;b.spend(definitions.len())?;
    let mut rows=definitions.iter().filter(|row|row.id==id);let row=rows.next().ok_or_else(||bad("pattern nominal original owner is absent"))?;if rows.next().is_some(){return Err(bad("pattern nominal original owner is duplicated"));}Ok(row)
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
