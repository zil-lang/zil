//! Declaration bindings and lexical snapshots are distinct namespaces. A name
//! table cannot shadow an actual value binder or create a new where premise.
use super::*;
impl CheckedOriginal {
    /// An imported/static member chain is a declaration path only when its
    /// root is not an actual runtime value and the complete path has a real
    /// Value binding. A module alias or a Type binding alone grants no call.
    pub(super) fn static_call_type(&self,module:&TypedModule,context:&CheckedContext,base:&SemanticOriginalExpr,env:&Env,bindings:&BTreeMap<String,Binding>,b:&mut Budget)->Check<Option<Ty>>{
        let mut current=base;let mut tail=Vec::new();
        loop{
            b.spend(1)?;
            match &current.kind{
                CoreExprKind::Member{object,member,owner:None}=>{b.spend(member.len())?;tail.push(member.as_str());current=object;},
                CoreExprKind::Group(inner)=>current=inner,
                CoreExprKind::Name(name)=>{
                    if tail.is_empty()||bindings.contains_key(name){return Ok(None);}
                    b.spend(name.len().saturating_add(tail.len()))?;
                    let mut path=name.split("::").collect::<Vec<_>>();path.extend(tail.iter().rev().copied());
                    let definition=context.definition(module,env.current()?,b)?;
                    let SemanticDefinitionBody::FunctionWithBody{names,..}=&definition.body else{return Err(bad("static call lookup has no original body"));};
                    b.spend(names.len().saturating_mul(path.iter().fold(0usize,|total,part|total.saturating_add(part.len()).saturating_add(1)).saturating_add(1)))?;
                    let mut selected=None;
                    for row in names{if row.namespace==SemanticOriginalNamespace::Value&&row.path.iter().map(String::as_str).eq(path.iter().copied()){
                        if selected.replace(&row.value).is_some(){return Err(bad("static call path has ambiguous original declaration bindings"));}
                    }}
                    let Some(SemanticOriginalNameValue::Definition(id))=selected else{return Ok(None);};
                    let target=context.definition(module,*id,b)?;
                    if !matches!(target.kind,SemanticDefinitionKind::Function|SemanticDefinitionKind::Method){return Err(bad("static call path selects a noncallable declaration"));}
                    return Ok(Some(Ty::Function(*id)));
                },
                _=>return Ok(None),
            }
        }
    }
    pub(super) fn original_type(&self,definition:SemanticDefinitionId,source:&SemanticOriginalTypeSource,b:&mut Budget)->Check<Ty>{
        b.spend(1)?;
        let value=match source{
            SemanticOriginalTypeSource::Expression(node)=>self.expressions.get(&(definition,*node)),
            SemanticOriginalTypeSource::Binding(node)=>self.bindings.get(&(definition,*node)),
            SemanticOriginalTypeSource::Parameter(index)=>self.parameters.get(&(definition,*index)),
            SemanticOriginalTypeSource::Result=>self.results.get(&definition),
            SemanticOriginalTypeSource::BindingProjection{statement,pattern}=>{b.spend(pattern.len())?;self.pattern_types.get(&(definition,*statement,pattern.clone()))},
            SemanticOriginalTypeSource::NameAt{at,name}=>{let(kind,node)=node_key(*at);b.spend(name.len())?;self.names_at.get(&(definition,kind,node,name.clone()))},
            SemanticOriginalTypeSource::CallConstCarrier{..}=>return Err(unsupported("original const carrier needs its exact solved call/formal",None)),
        }.ok_or_else(||bad("original type selects no independently checked lexical node/binder occurrence"))?;
        self.resolved(value,b)
    }
    pub(super) fn remember_names(&mut self,env:&Env,at:SemanticOriginalNode,bindings:&BTreeMap<String,Binding>,b:&mut Budget)->Check<()> {
        let definition=env.current()?;let(kind,node)=node_key(at);
        for(name,binding)in bindings{b.spend(name.len().saturating_add(1))?;let ty=self.resolved(&binding.ty,b)?;self.names_at.insert((definition,kind,node,name.clone()),ty);}Ok(())
    }
    pub(super) fn named<'m>(&self,module:&'m TypedModule,context:&CheckedContext,definition:SemanticDefinitionId,name:&str,namespace:SemanticOriginalNamespace,b:&mut Budget)->Check<Option<&'m SemanticOriginalNameValue>>{
        let definition=context.definition(module,definition,b)?;let SemanticDefinitionBody::FunctionWithBody{names,..}=&definition.body else{return Err(bad("original name lookup has no complete function body"));};
        let path=name.split("::").collect::<Vec<_>>();let mut result=None;
        b.spend(names.len().saturating_mul(name.len().saturating_add(1)))?;
        for row in names{if row.namespace==namespace&&row.path.iter().map(String::as_str).eq(path.iter().copied()){if result.replace(&row.value).is_some(){return Err(bad("ambiguous original name binding in the same namespace"));}}}Ok(result)
    }
    pub(super) fn bound_name(&mut self,module:&TypedModule,context:&CheckedContext,name:&str,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<Arg>{
        let value=self.named(module,context,env.current()?,name,SemanticOriginalNamespace::Value,b)?.ok_or_else(||bad("original value name has no real lexical/declaration binding"))?;
        match value{
            SemanticOriginalNameValue::Recipe(recipe @ SemanticActualRecipe::Const{..})=>self.actual(module,context,recipe,env,active,b),
            SemanticOriginalNameValue::Recipe(_)=>Err(bad("original runtime value name does not denote a real const or function definition")),
            SemanticOriginalNameValue::Definition(definition)=>{let definition=context.definition(module,*definition,b)?;if !matches!(definition.kind,SemanticDefinitionKind::Function|SemanticDefinitionKind::Method){return Err(unsupported("original value definition needs its exact constructor/member rule",None));}Ok(Arg::Type(Ty::Function(definition.id)))},
        }
    }
}
fn node_key(node:SemanticOriginalNode)->(bool,NodeId){match node{SemanticOriginalNode::Expression(node)=>(false,node),SemanticOriginalNode::Statement(node)=>(true,node)}}

impl CheckedOriginal {
    pub(super) fn check_names(&mut self,module:&TypedModule,context:&CheckedContext,definition:&SemanticDefinition,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let SemanticDefinitionBody::FunctionWithBody{names,..}=&definition.body else{return Err(bad("original name inventory lacks body"));};let mut keys=BTreeSet::new();
        for row in names{b.spend(row.path.iter().map(String::len).sum::<usize>().saturating_add(row.path.len()))?;let namespace=match row.namespace{SemanticOriginalNamespace::Value=>0u8,SemanticOriginalNamespace::Type=>1,SemanticOriginalNamespace::Lifetime=>2};
            if row.path.is_empty()||row.path.iter().any(String::is_empty)||!keys.insert((namespace,row.path.clone())){return Err(bad("original name namespace contains empty/duplicate bindings"));}
            match &row.value{
                SemanticOriginalNameValue::Definition(id)=>{let target=context.definition(module,*id,b)?;let allowed=match row.namespace{SemanticOriginalNamespace::Lifetime=>false,SemanticOriginalNamespace::Value=>matches!(target.kind,SemanticDefinitionKind::Function|SemanticDefinitionKind::Method),SemanticOriginalNamespace::Type=>target.kind==SemanticDefinitionKind::Nominal||matches!(target.body,SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait{..}))};if !allowed{return Err(bad("original declaration name has a different namespace kind"));}},
                SemanticOriginalNameValue::Recipe(recipe)=>{let value=self.actual(module,context,recipe,env,active,b)?;if !matches!((row.namespace,value),(SemanticOriginalNamespace::Type,Arg::Type(_))|(SemanticOriginalNamespace::Lifetime,Arg::Life(_))|(SemanticOriginalNamespace::Value,Arg::Const(_))){return Err(bad("original name recipe has a different full formal/value kind"));}},
            }
        }Ok(())
    }
}
