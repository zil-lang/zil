//! Actual canonical Poll values retain their whole payload. This classifier
//! proves a type constructor only; no tag or empty Context proves Closed.
use super::*;
impl Check<'_, '_> {
    pub(super) fn protocol_value_type(&self,ty:&Type,b:&mut Budget,depth:usize)->CompileResult<bool> {
        b.spend(1,WORK)?;
        if depth>b.partial_depth { return Err(unknown("actual protocol carrier exceeds its complete type depth")); }
        crate::dyn_metadata::ty(ty,b)?;
        let storage=match ty { Type::BorrowedStorage { storage,.. }=>storage.as_ref(),ty=>ty };
        let Type::Enum { variants,.. }=storage else { return Ok(false); };
        let mut declared=false;
        for instance in &self.module.life_instances {
            b.spend(1,WORK)?;
            if instance.declaration!=zil_ir::builtin_life::POLL { continue; }
            crate::dyn_metadata::ty(&instance.ty,b)?;
            if instance.ty==*ty { if declared { return Err(fail("actual Poll has competing complete nominal instances")); } declared=true; }
        }
        if !declared { return Ok(false); }
        let definition=self.inventory.protocol_poll_definition(b)?;
        let [(pending,pending_fields),(ready,ready_fields)]=variants.as_slice() else { return Err(fail("actual canonical Poll changes its complete alternative inventory")); };
        if pending!="Pending" || !pending_fields.is_empty() || ready!="Ready" || ready_fields.len()!=1 { return Err(fail("actual canonical Poll changes its tag order or Ready payload arity")); }
        let payload=&ready_fields[0];
        self.ordinary_type_at(payload,b,depth+1)?;
        if matches!(ty,Type::BorrowedStorage { .. }) {
            // The complete qualified nominal consumer must retain every
            // payload and phantom class, not reconstruct N from leaf layout.
            self.nominal_inventory(ty,b)?;
        } else {
            let expected=original::nominal::actual(definition,std::slice::from_ref(payload),self.inventory,b,depth+1)?;
            crate::dyn_metadata::ty(&expected,b)?;
            if expected!=*ty { return Err(fail("actual Poll differs from its complete original generic substitution")); }
        }
        Ok(true)
    }
}
