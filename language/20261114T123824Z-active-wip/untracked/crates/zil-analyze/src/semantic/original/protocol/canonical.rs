//! Canonical protocol carriers are whole original declarations. The builtin ID
//! is a lookup key; the formal, both alternatives and every field are checked.
use super::*;
impl Inventory<'_> {
    pub(in crate::semantic) fn protocol_poll_definition(&self,b:&mut Budget)->CompileResult<SemanticDefinitionId> {
        let mut selected=None;
        for candidate in self.definitions.values() {
            b.spend(1,WORK)?;
            if !matches!(candidate.body,SemanticDefinitionBody::Nominal { declaration,.. } if declaration==zil_ir::builtin_life::POLL) { continue; }
            crate::dyn_metadata::recipe(*candidate,b)?;
            if selected.replace(candidate.id).is_some() { return Err(fail("original Poll has competing complete canonical declarations")); }
            let valid_identity=candidate.identity.package=="@zil.builtin"
                && candidate.identity.module_path.as_slice()==["task"]
                && candidate.identity.declaration_path.as_slice()==["enum","Poll"]
                && candidate.identity.generation_path.is_empty();
            let SemanticDefinitionBody::Nominal { kind:LifeNominalKind::Enum,body:SemanticNominalBody::Enum { variants },.. }=&candidate.body else { return Err(fail("original Poll canonical identity has another nominal constructor")); };
            let formal=matches!(candidate.parameters.as_slice(),[SemanticFormal::Type { requirements }] if requirements.is_empty());
            let variants=matches!(variants.as_slice(),[pending,ready]
                if pending.name=="Pending" && pending.fields.is_empty()
                && ready.name=="Ready" && matches!(ready.fields.as_slice(),[SemanticTypeRecipe::Formal(SemanticFormalRef { scope:SemanticScopeRef::Ancestor(0),ordinal:0 })]));
            if !valid_identity || candidate.parent.is_some() || candidate.kind!=SemanticDefinitionKind::Nominal || !formal || !variants || !candidate.implicit_lifetimes.is_empty() || !candidate.requirements.is_empty() || !candidate.environment.is_empty() { return Err(fail("original Poll changes its full canonical identity, unbounded Type formal, or payload substitution")); }
        }
        selected.ok_or_else(||fail("original protocol has no complete canonical Poll declaration"))
    }
    pub(in crate::semantic) fn protocol_poll_recipe<'r>(&self,recipe:&'r SemanticTypeRecipe,b:&mut Budget)->CompileResult<Option<&'r SemanticTypeRecipe>> {
        let SemanticTypeRecipe::Nominal { definition,arguments }=recipe else { return Ok(None); };
        let original=self.definition(*definition,b)?;
        if !matches!(original.body,SemanticDefinitionBody::Nominal { declaration,.. } if declaration==zil_ir::builtin_life::POLL) { return Ok(None); }
        if self.protocol_poll_definition(b)?!=*definition { return Err(fail("original Poll recipe selects another canonical definition")); }
        let [SemanticActualRecipe::Type(payload)]=arguments.as_slice() else { return Err(fail("original Poll changes its complete Type actual arity or kind")); };
        crate::dyn_metadata::recipe(payload,b)?;
        Ok(Some(payload))
    }
}
impl Proof<'_, '_> {
    pub(super) fn expression_term(&self,node:NodeId,b:&mut Budget)->CompileResult<Term> {
        b.lookup(self.checked.frames.len(),WORK)?;
        let frame=self.checked.frames.get(&self.definition.id).ok_or_else(||fail("original protocol operation has no checked declaration frame"))?;
        b.lookup(frame.expressions.len(),WORK)?;
        let mut term=*frame.expressions.get(&node).ok_or_else(||fail("original protocol operation loses its exact source expression type"))?;
        for _ in 0..=b.partial_depth {
            term=self.checked.root(term,b)?;
            if let Kind::CallResult { hidden,.. }=self.checked.arena[term] { term=hidden; } else { return Ok(term); }
        }
        Err(resource("original protocol expression type exceeds its finite call-result depth"))
    }
    pub(super) fn term_poll(&self,term:Term,b:&mut Budget)->CompileResult<Option<SemanticDefinitionId>> {
        let term=self.checked.root(term,b)?;
        let Kind::Nominal { definition,arguments,fields }=&self.checked.arena[term] else { return Ok(None); };
        let original=self.inventory.definition(*definition,b)?;
        if !matches!(original.body,SemanticDefinitionBody::Nominal { declaration,.. } if declaration==zil_ir::builtin_life::POLL) { return Ok(None); }
        if self.inventory.protocol_poll_definition(b)?!=*definition || arguments.len()!=1 || fields.len()!=1 { return Err(fail("original Poll term changes its complete canonical application or Ready field")); }
        // The original application checker constructed the actual Ready field
        // by substituting this full mixed Type argument, including its domain.
        Ok(Some(*definition))
    }
}
