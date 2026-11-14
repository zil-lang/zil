//! Lowered finite Copy products, including a CT tuple materialized in the
//! runtime CFG, do not acquire nominal, borrowed or owning authority from the
//! absence of a source constructor site.
use super::*;
impl CheckedContext {
    pub(super) fn internal_copy_aggregate(&self,module:&TypedModule,f:&TypedFunction,instruction:&Instruction,fields:&[ValueId],values:&[Option<CheckedType>],b:&mut Budget)->Check<CheckedType>{
        if !matches!(instruction.kind,InstructionKind::MakeAggregate{..}){return Err(bad("owning or nominal construction has no exact original semantic site"));}
        let result=instruction.result.ok_or_else(||bad("internal Copy aggregate has no defining result"))?;
        let ty=f.values.get(result.0 as usize).ok_or_else(||bad("internal Copy aggregate result type is absent"))?;
        dynamic_metadata::ty(ty,b)?;
        // A source nominal constructor and every qualified/owning carrier keep
        // their original occurrence obligation. Only structural products can
        // be an internal lowering product in this path.
        if !matches!(ty,Type::Tuple(_)|Type::Array{..})||!ty.is_copy_value(){return Err(bad("actual aggregate construction has no original semantic site"));}
        crate::check_instruction(module,f,instruction,b)?;
        let value=self.closed_value(module,ty,b)?;
        if !value.classes.is_empty()||!value.statics.is_empty(){return Err(bad("internal Copy aggregate has an unresolved class/static domain"));}
        b.spend(fields.len())?;
        for field in fields{
            let field=value_at(values,*field)?;
            dynamic_metadata::ty(&field.ty,b)?;
            if !field.classes.is_empty()||!field.statics.is_empty()||!field.ty.is_copy_value(){return Err(bad("internal Copy aggregate imports a borrowed or owning child"));}
            self.closed_value(module,&field.ty,b)?;
        }
        Ok(value)
    }
}
