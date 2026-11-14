//! Exact generated-wrapper structure is checked against S's actual query tree.
//! No pretty name, digest, machine signature or unused literal supplies it.
use super::*;
impl FrozenContext<'_> {
    pub(super) fn wrapper_identity(&self,view:&TypedModule,execution:&TypedModule,b:&mut Budget)->Check<()> {
        let raw=execution.semantic.as_ref().ok_or_else(||bad("CT executable wrapper original data is absent"))?;
        b.spend(raw.definitions.len())?;
        let mut generated=raw.definitions.iter().filter(|row|!self.definitions.contains_key(&row.id));
        let definition=generated.next().ok_or_else(||bad("CT package lacks its new original wrapper definition"))?;
        if generated.next().is_some(){return Err(bad("CT package contains an additional original definition outside its frozen source"));}
        let mut rows=raw.instances.iter().filter(|row|matches!(row.target,SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id))if self.expected.query.wrapper==SemanticFunctionTarget::Typed(id)));
        let instance=rows.next().ok_or_else(||bad("CT wrapper has no actual executable instance"))?;
        if rows.next().is_some()||instance.parent!=Some(self.query.1.id)||!instance.actuals.is_empty()||instance.environment.is_some()||instance.self_type.is_some()||!matches!(instance.lifetime_instantiation,SemanticLifetimeInstantiation::Rigid){return Err(bad("CT wrapper changes its exact frozen caller instance or introduces a fake environment"));}
        if definition.id!=instance.definition||definition.id.module!=self.query.0.id.module||definition.parent!=Some(self.query.0.id)||definition.kind!=SemanticDefinitionKind::Function||!definition.parameters.is_empty()||!definition.environment.is_empty()||!definition.implicit_lifetimes.is_empty()||!definition.requirements.is_empty(){return Err(bad("CT wrapper changes its true frozen declaration parent or original formal domain"));}
        if definition.identity.package!=self.query.0.identity.package||definition.identity.module_path!=self.query.0.identity.module_path||!definition.identity.declaration_path.starts_with(&self.query.0.identity.declaration_path)||definition.identity.declaration_path.len()!=self.query.0.identity.declaration_path.len()+1||!definition.identity.generation_path.starts_with(&self.query.0.identity.generation_path){return Err(bad("CT wrapper original identity is not derived from its actual frozen caller"));}
        let SemanticDefinitionBody::FunctionWithBody{signature,parameters,receiver_parameter,body:SemanticOriginalFunctionBody::Defined(body),..}=&definition.body else{return Err(bad("CT wrapper lacks its whole original executable body"));};
        if !parameters.is_empty()||receiver_parameter.is_some()||!signature.parameters.is_empty()||signature.bound_lifetimes!=0||signature.invocation_lifetimes!=0||signature.call_kind!=CallKind::Direct{return Err(bad("CT wrapper introduces caller/runtime parameters or another invocation role"));}
        if body.statements.len()!=1||!matches!(body.statements[0].kind,CoreStatementKind::Return(Some(_))){return Err(bad("CT wrapper is not exactly the generated return of the actual frozen query"));}
        let _=self.wrapper_annotations(view,definition.id,b)?;
        Ok(())
    }
    pub(super) fn wrapper_annotations<'a>(&'a self,module:&'a TypedModule,id:SemanticDefinitionId,b:&mut Budget)->Check<Vec<(&'a Type,&'a SemanticTypeRecipe)>> {
        let definition=module.semantic.as_ref().and_then(|program|program.definitions.iter().find(|row|row.id==id)).ok_or_else(||bad("CT wrapper original definition is absent"))?;
        let SemanticDefinitionBody::FunctionWithBody{body:SemanticOriginalFunctionBody::Defined(body),..}=&definition.body else{return Err(bad("CT wrapper original body is not defined"));};
        let [statement]=body.statements.as_slice() else{return Err(bad("CT wrapper contains additional original statements"));};
        let CoreStatementKind::Return(Some(expression))=&statement.kind else{return Err(bad("CT wrapper does not return its original query tree"));};
        let mut annotations=Vec::new();
        expression_pair(self.query.2,expression,&mut annotations,0,b)?;
        Ok(annotations)
    }
}
fn depth(depth:usize,b:&mut Budget)->Check<()> {b.spend(1)?;if depth>b.limits.max_call_depth.min(128){return Err(limit("frozen query/wrapper structural depth exceeded"));}Ok(())}
fn expression_pair<'a>(mut source:&'a CoreExpr,actual:&'a SemanticOriginalExpr,annotations:&mut Vec<(&'a Type,&'a SemanticTypeRecipe)>,level:usize,b:&mut Budget)->Check<()> {
    depth(level,b)?;
    while let CoreExprKind::CompileTime(value)=&source.kind{b.spend(1)?;source=value;}
    if source.id!=actual.id{return Err(bad("CT wrapper expression identity differs from the actual frozen query payload"));}
    use CoreExprKind as K;
    match (&source.kind,&actual.kind) {
        (K::Unit,K::Unit)=>{},
        (K::Bool(a),K::Bool(c)) if a==c=>{},
        (K::Literal(a),K::Literal(c)) if a==c=>{},
        (K::Integer(a),K::Integer(c))|(K::Float(a),K::Float(c))|(K::String(a),K::String(c))|(K::Name(a),K::Name(c))|(K::Character(a),K::Character(c)) if a==c=>b.spend(a.len())?,
        (K::Unary{op:a,operand:x},K::Unary{op:c,operand:y}) if a==c=>expression_pair(x,y,annotations,level+1,b)?,
        (K::Binary{op:a,left:x,right:y},K::Binary{op:c,left:z,right:w}) if a==c=>{expression_pair(x,z,annotations,level+1,b)?;expression_pair(y,w,annotations,level+1,b)?;},
        (K::Group(x),K::Group(y))|(K::Try(x),K::Try(y))|(K::Spread(x),K::Spread(y))=>expression_pair(x,y,annotations,level+1,b)?,
        (K::Ascribed{ty:a,value:x},K::Ascribed{ty:c,value:y})|(K::NumericConvert{target:a,value:x},K::NumericConvert{target:c,value:y})|(K::IntegerConvert{target:a,value:x},K::IntegerConvert{target:c,value:y})=>{annotations.push((a,c));expression_pair(x,y,annotations,level+1,b)?;},
        (K::Call{callee:a,arguments:x},K::Call{callee:c,arguments:y})=>{expression_pair(a,c,annotations,level+1,b)?;expressions_pair(x,y,annotations,level+1,b)?;},
        (K::Tuple(a),K::Tuple(c))|(K::Array(a),K::Array(c))=>expressions_pair(a,c,annotations,level+1,b)?,
        (K::Index{object:a,index:x},K::Index{object:c,index:y})=>{expression_pair(a,c,annotations,level+1,b)?;expression_pair(x,y,annotations,level+1,b)?;},
        (K::Member{object:a,member:x,owner:p},K::Member{object:c,member:y,owner:q}) if x==y=>{optional_type(p,q,annotations)?;expression_pair(a,c,annotations,level+1,b)?;},
        (K::Record{ty:a,fields:x},K::Record{ty:c,fields:y})=>{annotations.push((a,c));fields_pair(x,y,annotations,level+1,b)?;},
        (K::RecordConstructor{definition:a,fields:x},K::RecordConstructor{definition:c,fields:y}) if a==c=>fields_pair(x,y,annotations,level+1,b)?,
        (K::RecordUpdate{ty:a,base:x,fields:p},K::RecordUpdate{ty:c,base:y,fields:q})=>{optional_type(a,c,annotations)?;expression_pair(x,y,annotations,level+1,b)?;fields_pair(p,q,annotations,level+1,b)?;},
        (K::If{condition:a,then_value:x,else_value:p},K::If{condition:c,then_value:y,else_value:q})=>{expression_pair(a,c,annotations,level+1,b)?;expression_pair(x,y,annotations,level+1,b)?;expression_pair(p,q,annotations,level+1,b)?;},
        (K::CompileTimeBlock(a),K::CompileTimeBlock(c))=>block_pair(a,c,annotations,level+1,b)?,
        (K::TypeApply{callee:a,type_arguments:x},K::TypeApply{callee:c,type_arguments:y})=>{if x.len()!=y.len(){return Err(bad("CT wrapper omits original type arguments"));}b.spend(x.len())?;annotations.extend(x.iter().zip(y));expression_pair(a,c,annotations,level+1,b)?;},
        (K::GenericApply{callee:a,arguments:x},K::GenericApply{callee:c,arguments:y})=>{if x.len()!=y.len(){return Err(bad("CT wrapper omits original mixed arguments"));}b.spend(x.len())?;for(a,c)in x.iter().zip(y){match(a,c){(CoreGenericArgument::Type(a),CoreGenericArgument::Type(c))=>annotations.push((a,c)),(CoreGenericArgument::Const(a),CoreGenericArgument::Const(c))=>expression_pair(a,c,annotations,level+1,b)?,(CoreGenericArgument::Lifetime(a),CoreGenericArgument::Lifetime(c))if a==c=>{},_=>return Err(bad("CT wrapper changes original mixed argument kind/value"))}}expression_pair(a,c,annotations,level+1,b)?;},
        _=>return Err(unsupported("CT query/wrapper requires complete structural matching for this original operation",None)),
    }Ok(())
}
fn optional_type<'a>(source:&'a Option<Type>,actual:&'a Option<SemanticTypeRecipe>,out:&mut Vec<(&'a Type,&'a SemanticTypeRecipe)>)->Check<()> {match(source,actual){(None,None)=>Ok(()),(Some(a),Some(c))=>{out.push((a,c));Ok(())},_=>Err(bad("CT wrapper changes an original annotation occurrence"))}}
fn expressions_pair<'a>(source:&'a [CoreExpr],actual:&'a [SemanticOriginalExpr],out:&mut Vec<(&'a Type,&'a SemanticTypeRecipe)>,level:usize,b:&mut Budget)->Check<()> {if source.len()!=actual.len(){return Err(bad("CT wrapper changes original operand arity"));}b.spend(source.len())?;for(a,c)in source.iter().zip(actual){expression_pair(a,c,out,level,b)?;}Ok(())}
fn fields_pair<'a>(source:&'a [(String,CoreExpr)],actual:&'a [(String,SemanticOriginalExpr)],out:&mut Vec<(&'a Type,&'a SemanticTypeRecipe)>,level:usize,b:&mut Budget)->Check<()> {if source.len()!=actual.len(){return Err(bad("CT wrapper changes original field inventory"));}b.spend(source.len())?;for((name,a),(other,c))in source.iter().zip(actual){if name!=other{return Err(bad("CT wrapper reorders original named fields"));}expression_pair(a,c,out,level,b)?;}Ok(())}
fn block_pair<'a>(source:&'a CoreBlock,actual:&'a SemanticOriginalBlock,out:&mut Vec<(&'a Type,&'a SemanticTypeRecipe)>,level:usize,b:&mut Budget)->Check<()> {depth(level,b)?;if source.statements.len()!=actual.statements.len(){return Err(bad("CT wrapper changes original block statement inventory"));}b.spend(source.statements.len())?;for(a,c)in source.statements.iter().zip(&actual.statements){statement_pair(a,c,out,level+1,b)?;}Ok(())}
fn statement_pair<'a>(source:&'a CoreStatement,actual:&'a SemanticOriginalStatement,out:&mut Vec<(&'a Type,&'a SemanticTypeRecipe)>,level:usize,b:&mut Budget)->Check<()> {
    depth(level,b)?;if source.id!=actual.id{return Err(bad("CT wrapper changes original statement identity"));}
    use CoreStatementKind as K;
    match(&source.kind,&actual.kind){
        (K::Binding{name:a,mutable:m,ty:t,initializer:x},K::Binding{name:c,mutable:n,ty:u,initializer:y})if a==c&&m==n=>{optional_type(t,u,out)?;match(x,y){(Some(a),Some(c))=>expression_pair(a,c,out,level+1,b)?,(None,None)=>{},_=>return Err(bad("CT wrapper changes original initialization"))}},
        (K::Expression(a),K::Expression(c))=>expression_pair(a,c,out,level+1,b)?,
        (K::Return(a),K::Return(c))=>match(a,c){(Some(a),Some(c))=>expression_pair(a,c,out,level+1,b)?,(None,None)=>{},_=>return Err(bad("CT wrapper changes original Return value"))},
        (K::Assign{target:a,value:x},K::Assign{target:c,value:y})=>{expression_pair(a,c,out,level+1,b)?;expression_pair(x,y,out,level+1,b)?;},
        (K::CompoundAssign{target:a,op:x,value:p},K::CompoundAssign{target:c,op:y,value:q})if x==y=>{expression_pair(a,c,out,level+1,b)?;expression_pair(p,q,out,level+1,b)?;},
        (K::If{condition:a,then_body:x,else_body:p},K::If{condition:c,then_body:y,else_body:q})=>{expression_pair(a,c,out,level+1,b)?;block_pair(x,y,out,level+1,b)?;match(p,q){(Some(a),Some(c))=>block_pair(a,c,out,level+1,b)?,(None,None)=>{},_=>return Err(bad("CT wrapper changes original branch inventory"))}},
        (K::While{condition:a,body:x},K::While{condition:c,body:y})=>{expression_pair(a,c,out,level+1,b)?;block_pair(x,y,out,level+1,b)?;},
        (K::ForRange{name:a,start:x,end:p,body:r},K::ForRange{name:c,start:y,end:q,body:s})if a==c=>{expression_pair(x,y,out,level+1,b)?;expression_pair(p,q,out,level+1,b)?;block_pair(r,s,out,level+1,b)?;},
        (K::Block(a),K::Block(c))|(K::Loop(a),K::Loop(c))=>block_pair(a,c,out,level+1,b)?,
        (K::Break,K::Break)|(K::Continue,K::Continue)=>{},
        _=>return Err(unsupported("CT query/wrapper requires complete matching for this statement family",None)),
    }Ok(())
}
