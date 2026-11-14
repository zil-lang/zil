//! Frozen declarations are checked as original schemes and real Core ABI data.
//! They are not executable targets and supply no caller equality or CT rights.
use super::*;
impl CheckedOriginal {
    pub(in crate::semantic) fn frozen_names(&mut self,module:&TypedModule,context:&CheckedContext,caller:SemanticDefinitionId,wrapper:SemanticDefinitionId,aliases:&BTreeMap<(zil_base::ModuleId,String),SemanticDefinitionId>,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let definition=context.definition(module,caller,b)?;let SemanticDefinitionBody::FunctionWithBody{names:expected,..}=&definition.body else{return Err(bad("frozen caller names have no original body"));};
        let definition=context.definition(module,wrapper,b)?;let SemanticDefinitionBody::FunctionWithBody{names:actual,..}=&definition.body else{return Err(bad("CT wrapper names have no original body"));};
        let caller_env=self.environments.get(&caller).cloned().ok_or_else(||bad("frozen original caller environment was not checked"))?;
        let wrapper_env=self.environments.get(&wrapper).cloned().ok_or_else(||bad("generated wrapper original environment was not checked"))?;
        let previous=self.current.replace(caller);
        let result=(||{
            let mut seen=BTreeSet::new();
            for row in actual {
                b.spend(expected.len().saturating_add(row.path.iter().map(String::len).sum::<usize>()))?;
                let candidates=expected.iter().filter(|old|old.path==row.path&&old.namespace==row.namespace).collect::<Vec<_>>();
                if candidates.len()>1{return Err(bad("frozen caller has duplicate original names"));}
                if let Some(old)=candidates.first(){
                    seen.insert((row.namespace as u8,row.path.clone()));
                    match(&old.value,&row.value){
                        (SemanticOriginalNameValue::Definition(a),SemanticOriginalNameValue::Definition(c))if a==c=>{},
                        (SemanticOriginalNameValue::Recipe(a),SemanticOriginalNameValue::Recipe(c))=>{let a=self.actual(module,context,a,&caller_env,active,b)?;let c=self.actual(module,context,c,&wrapper_env,active,b)?;self.same_argument(&a,&c,b)?;},
                        _=>return Err(bad("CT wrapper changes a real frozen caller name binding")),
                    }
                }else{
                    let SemanticOriginalNameValue::Definition(target)=row.value else{return Err(bad("CT wrapper introduces a nondeclaration original name"));};
                    let name=row.path.join("::");
                    if row.namespace!=SemanticOriginalNamespace::Value||aliases.get(&(caller.module,name))!=Some(&target){return Err(bad("CT wrapper generated value name has no exact frozen Core/original declaration link"));}
                }
            }
            if expected.iter().any(|row|!seen.contains(&(row.namespace as u8,row.path.clone()))){return Err(bad("CT wrapper omits a complete original frozen caller name binding"));}
            Ok(())
        })();
        self.current=previous;result
    }
    pub(super) fn typed_external(&mut self,module:&TypedModule,context:&CheckedContext,prototype:&Prototype,function:FunctionId,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let f=module.functions.get(function.0 as usize).ok_or_else(||bad("external original ABI selects an absent actual function"))?;
        if f.foreign.is_none()||f.call_kind!=prototype.call_kind||f.parameters.len()!=prototype.parameters.len()||f.result_mode!=prototype.result_mode{return Err(bad("original external declaration differs from its exact actual ABI"));}
        for (local,expected)in f.parameters.iter().zip(&prototype.parameters){let actual=f.locals.get(local.0 as usize).ok_or_else(||bad("external parameter local is absent"))?;if !self.core_abi_type(module,context,expected,&actual.ty,&prototype.environment,active,b)?{return Err(bad("original external parameter differs from its full actual C ABI"));}}
        if !self.core_abi_type(module,context,&prototype.result,&f.return_type,&prototype.environment,active,b)?{return Err(bad("original external result differs from its actual complete C ABI"));}Ok(())
    }
    pub(super) fn frozen_external(&mut self,module:&TypedModule,context:&CheckedContext,prototype:&Prototype,source:&super::super::frozen::FrozenFunction,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        if source.foreign.is_none()||source.parameters.len()!=prototype.parameters.len()||source.result_mode!=prototype.result_mode{return Err(bad("frozen original external declaration lacks its actual complete Core ABI"));}
        for((mode,actual),(expected,expected_mode))in source.parameters.iter().zip(prototype.parameters.iter().zip(&prototype.modes)){
            if mode!=expected_mode||!self.core_abi_type(module,context,expected,actual,&prototype.environment,active,b)?{return Err(bad("frozen external original parameter differs from its real Core ABI type/mode"));}
        }
        if !self.core_abi_type(module,context,&prototype.result,&source.result,&prototype.environment,active,b)?{return Err(bad("frozen external original result differs from its real Core ABI type"));}
        Ok(())
    }
    pub(super) fn core_abi_type(&mut self,module:&TypedModule,context:&CheckedContext,expected:&Ty,actual:&Type,env:&Env,active:&mut ActiveQueries,b:&mut Budget)->Check<bool>{
        let token=active.fresh(b)?;active.with_query(DependencyKey{kind:QueryKind::Type,token},b,|active,b|{
            dynamic_metadata::ty(actual,b)?;
            let expected=self.resolved(expected,b)?;
            Ok(match(&expected,actual){
                (Ty::Atom(a),c)=>a==c,
                (Ty::Address{mutable:a,inner:x,..},Type::Address{mutable:c,inner:y})if a==c=>self.core_abi_type(module,context,x,y,env,active,b)?,
                (Ty::Desc{inner,..},Type::Desc(actual))=>self.core_abi_type(module,context,inner,actual,env,active,b)?,
                (Ty::Slice(inner),Type::Slice{element})=>self.core_abi_type(module,context,inner,element,env,active,b)?,
                (Ty::Tuple(a),Type::Tuple(c))if a.len()==c.len()=>{for(a,c)in a.iter().zip(c){if !self.core_abi_type(module,context,a,c,env,active,b)?{return Ok(false);}}true},
                (Ty::Variable(..)|Ty::SelfType(_)|Ty::Infer(_)|Ty::Nominal(..)|Ty::Code{..},_)=>return Err(unsupported("frozen generic/nominal external ABI requires its complete original specialization and source-domain correspondence",None)),
                _=>false,
            })
        })
    }
}
