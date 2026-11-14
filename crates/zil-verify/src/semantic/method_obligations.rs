//! Discharge source-method obligations in every independent callback session.
//! A helper's selected body identity is a request, not an executable ticket.
use super::*;
impl CheckedContext {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn check_original_method_relation(&self,module:&TypedModule,trait_declaration:LifeDeclarationId,implementation_declaration:LifeDeclarationId,trait_signature:SemanticDefinitionId,body:SemanticDefinitionId,source:DynDefinitionScope,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let mut original=self.original.borrow_mut();
        let original=original.as_mut().ok_or_else(||unsupported("method relation has no completed universal original programme",None))?;
        original.method_relation(module,self,trait_declaration,implementation_declaration,trait_signature,body,source,active,b)
    }
    pub(super) fn finish_external_methods(&self,module:&TypedModule,proofs:&mut dynamic_witness::semantic_external::Proofs,active:&mut ActiveQueries,b:&mut Budget)->Check<()> {
        let mut completed=Vec::new();
        while !proofs.pending_original_methods.is_empty()||!proofs.pending_original_method_relations.is_empty(){
            b.spend(proofs.pending_original_methods.len().saturating_add(proofs.pending_original_method_relations.len()).saturating_add(1))?;
            let methods=std::mem::take(&mut proofs.pending_original_methods);
            if !methods.is_empty(){let original=self.original.borrow();let original=original.as_ref().ok_or_else(||unsupported("selected method has no complete original body proof",None))?;for method in methods{original.checked_method_body(module,method,b)?;}}
            for relation in std::mem::take(&mut proofs.pending_original_method_relations){
                let key=(relation.trait_declaration,relation.implementation_declaration,relation.trait_signature,relation.body,relation.source);
                b.spend(completed.len().saturating_add(1))?;
                if completed.contains(&key){continue;}
                self.check_original_method_relation(module,relation.trait_declaration,relation.implementation_declaration,relation.trait_signature,relation.body,relation.source,active,b)?;
                completed.push(key);
            }
        }
        Ok(())
    }
}
