//! Synthetic invocation heads carry no Type domain or memory permission.
//! Actual entry, callee occurrence and final memory/loan kernels prove them.
use super::*;
impl CheckedContext{
    pub(super) fn receiver_entry_head(&self,module:&TypedModule,environment:&EvalEnv,b:&mut Budget)->Check<ClassKey>{
        let frame=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?;
        let ScopeKey::Instance(id)=frame.key else{return Err(bad("Receiver head has no exact actual closure entry world"));};
        let instance=self.instance(module,id,b)?;
        if instance.definition!=frame.definition{return Err(bad("Receiver head changes its original declaration world"));}
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=instance.target else{return Err(bad("Receiver head retains a nonexecutable entry"));};
        let entry=module.functions.get(function.0 as usize).filter(|row|row.id==function).ok_or_else(||bad("Receiver entry is absent"))?;
        let closure=entry.environment.ok_or_else(||bad("ordinary function cannot introduce a closure Receiver"))?;
        let closure=crate::closures::definition(module,closure,None)?;
        let header=closure.storage_life.as_ref().ok_or_else(||bad("Receiver entry complete header is Unknown"))?;
        if entry.call_kind!=CallKind::Direct||!matches!(closure.kind,CallableKind::Fn|CallableKind::FnMut)||entry.storage_life.as_ref()!=Some(header)||!matches!(closure.result,Type::Address{mutable:false,..}){
            return Err(bad("Receiver is not its complete Direct nonconsuming shared-result entry"));
        }
        signature_life::charge(header,b)?;b.spend(header.bindings.len().saturating_mul(3).saturating_add(header.classes.len()))?;
        let mut rows=header.bindings.iter().filter(|row|row.position.base==StorageBase::Receiver);
        let row=rows.next().ok_or_else(||bad("Receiver head has no explicit actual input binding"))?;
        if rows.next().is_some()||!row.position.path.is_empty()||!matches!(row.binder,StorageBinder::Invocation(index)if index<header.invocation_binder_count)||signature_life::outer(header,StorageBase::Result).is_none()&&!header.static_bindings.contains(&StoragePosition{base:StorageBase::Result,path:vec![]})||header.classes.iter().any(|row|row.position.base==StorageBase::Receiver){return Err(bad("Receiver head changes its exact binding or invents a persistent class directory"));}
        if !self.original.borrow().as_ref().ok_or_else(||bad("Receiver entry lacks original universal proof"))?.receiver_result_proven(frame.definition,b)?{return Err(bad("Receiver entry has no all-return original lending proof"));}
        Ok(ClassKey::Outer{scope:frame.key,operand:OperandKey::Receiver,path:vec![],mode:StoredBorrowMode::SharedAddress})
    }
    /// A prospective Callable signature owns a fresh subject access role.
    /// This is a demand, not an address value, constructor or runtime loan.
    pub(super) fn evaluate_callable_signature(&self,module:&TypedModule,recipe:&SemanticSignatureRecipe,kind:CallableKind,environment:&EvalEnv,external:&mut impl ExternalProofs,active:&mut ActiveQueries,b:&mut Budget)->Check<CheckedSignature>{
        if !original::CheckedOriginal::receiver_signature_recipe(recipe){return self.evaluate_signature(module,recipe,environment,external,active,b);}
        if recipe.call_kind!=CallKind::Direct||!matches!(kind,CallableKind::Fn|CallableKind::FnMut){return Err(bad("only a Direct nonconsuming Callable subject has a Receiver result"));}
        let definition=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?.definition;
        let key=ClassKey::Outer{scope:ScopeKey::Witness{definition,frame:active.fresh(b)?},operand:OperandKey::Receiver,path:vec![],mode:StoredBorrowMode::SharedAddress};
        let mut value=self.evaluate_signature_receiver(module,recipe,environment,Some(key.clone()),external,active,b)?;
        b.spend(1)?;value.invocation.push(key);Ok(value)
    }
    pub(crate) fn callable_receiver_key<'a>(&self,signature:&'a CheckedSignature,b:&mut Budget)->Check<Option<&'a ClassKey>>{
        b.spend(signature.invocation.len())?;
        let mut keys=signature.invocation.iter().filter(|key|matches!(key,ClassKey::Outer{scope:ScopeKey::Witness{..},operand:OperandKey::Receiver,path,mode:StoredBorrowMode::SharedAddress}if path.is_empty()));
        let key=keys.next();if keys.next().is_some(){return Err(bad("Callable signature has multiple subject Receiver access roles"));}Ok(key)
    }
}
