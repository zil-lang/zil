//! TypeUse compares against an existing checked operand. Captures come from
//! their exact sealed creator, never from a same-typed closure declaration.
use super::*;
impl CheckedContext{
    #[allow(clippy::too_many_arguments)]
    pub(super) fn type_use_operand(&self,module:&TypedModule,instance:&SemanticInstance,f:&TypedFunction,subject:&SemanticOperand,required:&CheckedType,state:&State,values:&[Option<CheckedType>],signature:&CheckedSignature,environment:&EvalEnv,b:&mut Budget)->Check<CheckedType>{
        let frame=self.frame(environment,SemanticScopeRef::Ancestor(0),b)?;
        if frame.definition!=instance.definition||!matches!(frame.key,ScopeKey::Instance(id)if id==instance.id){return Err(bad("actual TypeUse selects another source world"));}
        let value=match subject{
            SemanticOperand::Value(value)=>value_at(values,*value)?,
            SemanticOperand::Local(local)=>{
                let value=state.locals.get(local.0 as usize).and_then(Option::as_ref).ok_or_else(||bad("actual type use reads an uninitialized local"))?;
                if let Some((_,selector))=value.ty.binding_cell_element().filter(|(ty,_)| **ty==required.ty){
                    self.persistent_initialization(module,instance,f,*local,b)?;
                    return self.child_value(module,value,&[selector],b);
                }value
            },
            SemanticOperand::Parameter(index)=>{
                let parameter=f.parameters.get(*index as usize).ok_or_else(||bad("actual TypeUse parameter is absent"))?;
                let local=f.locals.get(parameter.0 as usize).ok_or_else(||bad("actual TypeUse parameter has no local declaration"))?;
                let value=&signature.parameters.get(*index as usize).ok_or_else(||bad("actual TypeUse parameter has no original complete signature"))?.0;
                dynamic_metadata::ty(&local.ty,b)?;dynamic_metadata::ty(&value.ty,b)?;
                if local.ty!=value.ty{return Err(bad("actual TypeUse parameter changes its whole original type"));}value
            },
            SemanticOperand::TypeArgument{scope,ordinal}=>match self.frame(environment,*scope,b)?.actuals.get(*ordinal as usize){
                Some(CheckedActual::Type(value))=>value,
                _=>return Err(bad("actual TypeUse full ordinal is not a Type argument")),
            },
            SemanticOperand::Environment(ordinal)=>match self.environment_actual(environment,SemanticScopeRef::Ancestor(0),*ordinal,b)?{
                CheckedActual::Type(value)=>value,
                _=>return Err(bad("actual TypeUse sealed environment slot is not a Type value")),
            },
            SemanticOperand::Capture(index)=>{
                let closure=f.environment.ok_or_else(||bad("actual TypeUse Capture has no actual closure entry"))?;
                let sealed=instance.environment.as_ref().ok_or_else(||bad("actual TypeUse Capture has no original sealed environment"))?;
                if !matches!(sealed.closure,SemanticClosureTarget::Typed(id)if id==closure){return Err(bad("actual TypeUse Capture selects another original closure"));}
                b.spend(module.closures.len())?;
                let mut definitions=module.closures.iter().filter(|row|row.id==closure);
                let definition=definitions.next().ok_or_else(||bad("actual TypeUse closure definition is absent"))?;
                if definitions.next().is_some()||definition.function!=f.id{return Err(bad("actual TypeUse Capture has another entry identity"));}
                let declared=definition.captures.get(*index as usize).ok_or_else(||bad("actual TypeUse Capture ordinal is absent"))?;
                let values=self.occurrences.borrow();let value=values.get(&(frame.key,OperandKey::Capture(*index))).ok_or_else(||bad("actual TypeUse Capture has no checked creator operand"))?;
                dynamic_metadata::ty(declared,b)?;dynamic_metadata::ty(&value.ty,b)?;
                if *declared!=value.ty{return Err(bad("actual TypeUse Capture changes its complete sealed source type"));}
                value.charge_clone(b)?;return Ok(value.clone());
            },
            SemanticOperand::Receiver=>{
                let values=self.occurrences.borrow();let value=values.get(&(frame.key,OperandKey::Receiver)).ok_or_else(||bad("actual TypeUse Receiver has no independently checked invocation head"))?;
                value.charge_clone(b)?;return Ok(value.clone());
            },
            SemanticOperand::CoreExpression{..}=>return Err(bad("Typed TypeUse retains an unresolved Core occurrence")),
        };
        value.charge_clone(b)?;Ok(value.clone())
    }
}
