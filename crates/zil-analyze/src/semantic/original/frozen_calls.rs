//! Selected CT calls retain the exact universal call solution. Full mixed
//! actuals are compared in their actual worlds; no machine signature chooses
//! a source definition or merges lifetime classes.
use super::*;
impl Checked {
    pub(in crate::semantic) fn frozen_call(
        &self,
        caller: &SemanticInstance,
        node: NodeId,
        target: &SemanticInstance,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        b.lookup(self.calls.len(), WORK)?;
        let call = self
            .calls
            .get(&(caller.definition, node))
            .ok_or_else(|| fail("selected CT call has no independently checked original call"))?;
        if call.closure
            || call.target != target.definition
            || call.actuals.len() != target.actuals.len()
        {
            return Err(fail(
                "selected CT call changes its original target or complete mixed arity",
            ));
        }
        for (term, actual) in call.actuals.iter().zip(&target.actuals) {
            let expected = self.argument(*term, caller, program, inventory, b, 0)?;
            crate::dyn_metadata::recipe(actual, b)?;
            let actual = match actual {
                SemanticActual::Type(value) => {
                    value::Argument::Type(value::resolved(target, value, inventory, b, 0)?)
                }
                SemanticActual::Const {
                    ty,
                    value: constant,
                } => {
                    let value = value::resolved(target, ty, inventory, b, 0)?;
                    if constant.ty() != value.ty
                        || !value.classes.is_empty()
                        || value.life != value::empty()
                    {
                        return Err(fail(
                            "selected CT constant changes its complete scalar carrier",
                        ));
                    }
                    value::Argument::Const {
                        carrier: value.into_type(b)?,
                        value: constant.clone(),
                    }
                }
                SemanticActual::Life(value) => {
                    value::Argument::Life(value::class(target, value, inventory, b, 0)?)
                }
                _ => return Err(fail("selected CT call retains an unresolved mixed actual")),
            };
            // Both sides were formed with complete life/N/phantom domains.
            let equal = match (&expected, &actual) {
                (value::Argument::Type(a), value::Argument::Type(z)) => a.equal_paid(z, b)?,
                _ => expected == actual,
            };
            if !equal {
                return Err(fail(
                    "selected CT call changes its full original mixed solution",
                ));
            }
        }
        Ok(())
    }
}
