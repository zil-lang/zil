//! Actual foreign ABI declarations and calls for the two checked x64 profiles.
use super::*;
fn extension(ty: &Type, target: &TargetProfile) -> Vec<m::Attr> {
    if target.triple == "x86_64-unknown-linux-gnu" && matches!(ty, Type::U8 | Type::U16) {
        vec![m::Attr::ZeroExt]
    } else {
        vec![]
    }
}
pub(super) fn declaration(
    function: &TypedFunction,
    target: &TargetProfile,
) -> CompileResult<m::Code> {
    let foreign = function.foreign.as_ref().expect("foreign declaration");
    let parameters = function
        .parameters
        .iter()
        .map(|id| {
            let ty = &function.locals[id.0 as usize].ty;
            Ok(m::Param {
                ty: llvm_type(ty)?,
                name: None,
                attrs: extension(ty, target),
            })
        })
        .collect::<CompileResult<Vec<_>>>()?;
    Ok(m::Event::Declare(m::Function {
        name: foreign.symbol.clone(),
        linkage: m::Linkage::External,
        signature: m::Signature {
            result: llvm_type(&function.return_type)?,
            result_attrs: extension(&function.return_type, target),
            params: parameters,
            variadic: false,
        },
        attrs: if target.triple == "x86_64-unknown-linux-gnu" {
            vec![m::Attr::NonLazyBind]
        } else {
            vec![]
        },
        blocks: None,
    })
    .into())
}
impl FunctionEmitter<'_> {
    pub(super) fn foreign_call(
        &mut self,
        instruction: &Instruction,
        function: FunctionId,
        arguments: &[ValueId],
    ) -> CompileResult<()> {
        let callee = self.functions[&function];
        let foreign = callee.foreign.as_ref().expect("verified foreign target");
        let arguments = arguments
            .iter()
            .map(|argument| {
                let ty = self.value_type(*argument)?;
                Ok(m::Arg::new(llvm_type(ty)?, m::val(value(*argument)))
                    .attributed(extension(ty, self.target)))
            })
            .collect::<CompileResult<Vec<_>>>()?;
        let output = if callee.return_type == Type::Unit {
            None
        } else {
            Some(format!("v.{}", result(instruction)?.0))
        };
        self.line(
            m::Event::Instruction(m::Inst::new(
                output,
                m::Op::Call {
                    result: llvm_type(&callee.return_type)?,
                    result_attrs: extension(&callee.return_type, self.target),
                    callee: m::global(foreign.symbol.clone()),
                    args: arguments,
                    vararg: None,
                },
            ))
            .into(),
        );
        Ok(())
    }
}
