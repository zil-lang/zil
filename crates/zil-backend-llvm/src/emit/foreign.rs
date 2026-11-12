//! Actual foreign ABI declarations and calls for the two checked x64 profiles.
use super::*;
fn extension(ty: &Type, target: &TargetProfile) -> Vec<m::Attr> {
    if target.triple == "x86_64-unknown-linux-gnu" {
        match ty {
            Type::U8 | Type::U16 => vec![m::Attr::ZeroExt],
            Type::I8 | Type::I16 => vec![m::Attr::SignExt],
            _ => vec![],
        }
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

fn callback_symbol(id: FunctionId) -> String {
    format!("zil.c.callback.{}", id.0)
}
fn needs_callback_adapter(function: &TypedFunction, target: &TargetProfile) -> bool {
    target.triple == "x86_64-unknown-linux-gnu"
        && (matches!(function.return_type, Type::I8 | Type::I16)
            || function
                .parameters
                .iter()
                .any(|id| matches!(function.locals[id.0 as usize].ty, Type::I8 | Type::I16)))
}
impl FunctionEmitter<'_> {
    pub(super) fn foreign_callback_ref(
        &mut self,
        instruction: &Instruction,
        function: FunctionId,
    ) -> CompileResult<()> {
        let definition = self.functions[&function];
        self.machine_meter
            .spend(definition.parameters.len().saturating_add(1))?;
        let symbol = if needs_callback_adapter(definition, self.target) {
            self.machine_meter
                .spend(self.foreign_adapters.len().saturating_add(1))?;
            self.foreign_adapters.insert(function);
            callback_symbol(function)
        } else {
            self.abi_symbol(function)
        };
        self.event(m::Event::Instruction(m::Inst::new(
            Some(format!("v.{}", result(instruction)?.0)),
            m::Op::Freeze(m::Arg::new(m::Ty::Ptr, m::global(symbol))),
        )));
        Ok(())
    }
}
/// Bridge the ordinary ZIL scalar entry to the target's signed narrow C ABI.
/// One adapter per actual function preserves callback pointer identity.
pub(super) fn callback_adapter(
    function: &TypedFunction,
    target: &TargetProfile,
    meter: &tasks::Meter,
) -> CompileResult<m::Code> {
    meter.spend(
        function
            .parameters
            .len()
            .saturating_mul(24)
            .saturating_add(48),
    )?;
    let mut parameters = Vec::with_capacity(function.parameters.len());
    let mut arguments = Vec::with_capacity(function.parameters.len());
    for (index, local) in function.parameters.iter().enumerate() {
        let ty = &function.locals[local.0 as usize].ty;
        let machine = llvm_type(ty)?;
        let name = format!("arg.{index}");
        parameters.push(m::Param {
            ty: machine.clone(),
            name: Some(name.clone()),
            attrs: extension(ty, target),
        });
        arguments.push(m::Arg::new(machine, m::local(name)));
    }
    let result = llvm_type(&function.return_type)?;
    let value = (function.return_type != Type::Unit).then(|| "result".to_owned());
    Ok(m::Code::from(vec![
        m::Event::Begin(m::Function {
            name: callback_symbol(function.id),
            linkage: m::Linkage::Internal,
            signature: m::Signature {
                result: result.clone(),
                result_attrs: extension(&function.return_type, target),
                params: parameters,
                variadic: false,
            },
            attrs: vec![],
            blocks: None,
        }),
        m::Event::Label("entry".into()),
        m::Event::Instruction(m::Inst::new(
            value.clone(),
            m::Op::Call {
                result: result.clone(),
                result_attrs: vec![],
                callee: m::global(symbol(function.id)),
                args: arguments,
                vararg: None,
            },
        )),
        m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Return(value.map(|name| m::Arg::new(result, m::local(name)))),
        )),
        m::Event::End,
    ]))
}
