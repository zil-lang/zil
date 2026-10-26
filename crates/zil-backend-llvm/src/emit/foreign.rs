//! Default LLVM C calling convention on the two checked x64 targets.
use super::*;

// SysV's unsigned narrow C arguments/results carry zeroext. Win64 uses the
// native low register bits without that ABI attribute. No other attributes are
// inferred from a source contract.
fn extension(ty: &Type, target: &TargetProfile) -> &'static str {
    if target.triple == "x86_64-unknown-linux-gnu" && matches!(ty, Type::U8 | Type::U16) {
        "zeroext "
    } else {
        ""
    }
}

pub(super) fn declaration(
    function: &TypedFunction,
    target: &TargetProfile,
) -> CompileResult<String> {
    let foreign = function.foreign.as_ref().expect("foreign declaration");
    let parameters = function
        .parameters
        .iter()
        .map(|id| {
            let ty = &function.locals[id.0 as usize].ty;
            let llvm = llvm_type(ty)?;
            let attr = extension(ty, target).trim_end();
            Ok(if attr.is_empty() {
                llvm
            } else {
                format!("{llvm} {attr}")
            })
        })
        .collect::<CompileResult<Vec<_>>>()?
        .join(", ");
    // This is a Linux ELF binding policy, not a claim about the callee's
    // effects. Keep symbol interposition, the C ABI and all call-site checks.
    let binding = if target.triple == "x86_64-unknown-linux-gnu" {
        " nonlazybind"
    } else {
        ""
    };
    Ok(format!(
        "declare {}{} @{}({parameters}){binding}\n",
        extension(&function.return_type, target),
        llvm_type(&function.return_type)?,
        foreign.symbol
    ))
}

impl FunctionEmitter<'_> {
    pub(super) fn foreign_call(
        &mut self,
        instruction: &Instruction,
        function: FunctionId,
        arguments: &[ValueId],
    ) -> CompileResult<()> {
        let callee = self.functions[&function];
        let foreign = callee
            .foreign
            .as_ref()
            .expect("independently verified foreign target");
        let arguments = arguments
            .iter()
            .map(|argument| {
                Ok(format!(
                    "{} {}%v.{}",
                    llvm_type(self.value_type(*argument)?)?,
                    extension(self.value_type(*argument)?, self.target),
                    argument.0
                ))
            })
            .collect::<CompileResult<Vec<_>>>()?
            .join(", ");
        let assignment = if callee.return_type == Type::Unit {
            String::new()
        } else {
            format!("%v.{} = ", result(instruction)?.0)
        };
        self.line(format!(
            "  {assignment}call {}{} @{}({arguments})",
            extension(&callee.return_type, self.target),
            llvm_type(&callee.return_type)?,
            foreign.symbol
        ));
        Ok(())
    }
}
