//! Constructor-bound captured heads name one exact original free use. A
//! lowered body may address fields/elements without emitting a separate read
//! site; neither same-typed fields nor a header image selects the source.
use super::*;
impl CheckedContext {
    pub(super) fn capture_implicit(&self,module:&TypedModule,child:&SemanticInstance,index:u32,b:&mut Budget)->Check<Option<ClassKey>>{
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function))=child.target else{return Err(bad("capture head is not an executable original instance"));};
        let f=module.functions.get(function.0 as usize).ok_or_else(||bad("capture-head actual entry is absent"))?;
        let header=f.storage_life.as_ref().ok_or_else(||bad("capture-head complete actual header is Unknown"))?;
        b.spend(header.bindings.len().saturating_add(child.header_embedding.len()))?;
        let mut selected=None;
        for row in header.bindings.iter().filter(|row|row.position.base==StorageBase::Capture(index)&&row.position.path.is_empty()){
            let mut maps=child.header_embedding.iter().filter(|map|map.header==row.binder);
            let map=maps.next().ok_or_else(||bad("captured head has no exact original class occurrence"))?;
            if maps.next().is_some(){return Err(bad("captured head original class occurrence is duplicated"));}
            let SemanticClassExpr::Implicit{scope:SemanticScopeRef::Ancestor(0),ordinal}=map.actual else{continue;};
            self.original.borrow().as_ref().ok_or_else(||bad("captured head has no independent original body"))?.implicit_capture_binding(module,child.definition,ordinal,b)?;
            let key=ClassKey::Implicit{scope:ScopeKey::Instance(child.id),ordinal};
            if selected.replace(key.clone()).is_some_and(|old|old!=key){return Err(bad("one physical capture has several original head occurrences"));}
        }
        Ok(selected)
    }
}
