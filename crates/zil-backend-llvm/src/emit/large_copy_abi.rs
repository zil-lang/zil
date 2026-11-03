//! LargeCopy ABI v1: source values stay values; physical pointers only name
//! independent snapshots, with a mandatory callee copy-in before source code.
use super::*;
use copy_aggregates::{Slot, classify};
use zil_base::Limits;

#[derive(Clone, Default)]
pub(super) struct Signature {
    pub(super) parameters: Vec<Option<Slot>>,
    pub(super) result: Option<Slot>,
}
impl Signature {
    pub(super) fn large(&self) -> bool {
        self.result.is_some() || self.parameters.iter().any(Option::is_some)
    }
}
#[derive(Default)]
pub(super) struct Plans {
    pub(super) values: BTreeMap<FunctionId, copy_aggregates::Plan>,
    pub(super) functions: BTreeMap<FunctionId, Signature>,
    pub(super) indirect: BTreeMap<(FunctionId, ValueId), Signature>,
}
impl Plans {
    pub(super) fn needs_helpers(&self) -> bool {
        !self.values.is_empty() || self.functions.values().any(Signature::large)
    }
}
fn signature<'a>(
    parameters: impl ExactSizeIterator<Item = &'a Type>,
    result: &Type,
    remaining: &mut usize,
    limits: &Limits,
) -> CompileResult<Signature> {
    // Pay the retained parameter mapping before allocating it, including Unit
    // erasure positions. This shares the current module's snapshot remainder.
    *remaining = remaining
        .checked_sub(parameters.len().saturating_add(1))
        .ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Codegen,
                "large Copy ABI mapping budget exceeded",
                None,
            )]
        })?;
    let parameters = parameters
        .map(|ty| classify(ty, remaining, limits))
        .collect::<CompileResult<Vec<_>>>()?;
    let result = classify(result, remaining, limits)?;
    Ok(Signature { parameters, result })
}
pub(super) fn plan(
    lir: &LirView<'_>,
    limits: &Limits,
    remaining: &mut usize,
) -> CompileResult<Plans> {
    let mut plans = Plans::default();
    for function in lir.functions {
        let abi = signature(
            function
                .parameters
                .iter()
                .map(|id| &function.locals[id.0 as usize].ty),
            &function.return_type,
            remaining,
            limits,
        )?;
        if abi.large() && (function.foreign.is_some() || function.environment.is_some()) {
            return unsupported(
                "large Copy foreign or closure entry ABI",
                Some(function.span),
            );
        }
        plans.functions.insert(function.id, abi);
        for (index, ty) in function.values.iter().enumerate() {
            if let Type::FunctionPointer { parameters, result } = ty {
                let abi = signature(parameters.iter(), result, remaining, limits)?;
                if abi.large() {
                    plans
                        .indirect
                        .insert((function.id, ValueId(index as u32)), abi);
                }
            }
        }
    }
    for function in lir.functions {
        for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
            if let InstructionKind::WithScopedThread { worker, .. } = instruction.kind
                && plans.functions.get(&worker).is_some_and(Signature::large)
            {
                return unsupported(
                    "large Copy scoped-thread helper ABI",
                    Some(instruction.span),
                );
            }
        }
    }
    Ok(plans)
}
impl FunctionEmitter<'_> {
    pub(super) fn abi(&self) -> &Signature {
        static EMPTY: Signature = Signature {
            parameters: Vec::new(),
            result: None,
        };
        self.abi_plans
            .functions
            .get(&self.function.id)
            .unwrap_or(&EMPTY)
    }
    pub(super) fn abi_symbol(&self, id: FunctionId) -> String {
        let base = symbol(id);
        if self
            .abi_plans
            .functions
            .get(&id)
            .is_some_and(Signature::large)
        {
            format!("{base}.lc1")
        } else {
            base
        }
    }
    pub(super) fn large_copy_call(
        &mut self,
        instruction: &Instruction,
        signature: &Signature,
        callee: m::Val,
        arguments: &[ValueId],
    ) -> CompileResult<()> {
        let mut args = Vec::new();
        if signature.result.is_some() {
            let output = result(instruction)?;
            if !self.copy_values.contains_key(&output) {
                return invalid(
                    "E-CODEGEN-CFG",
                    "large ABI result has no snapshot",
                    Some(instruction.span),
                );
            }
            args.push(m::Arg::new(m::Ty::Ptr, copy_aggregates::pointer(output)));
        }
        for (index, argument) in arguments.iter().enumerate() {
            let ty = self.value_type(*argument)?;
            if signature.parameters[index].is_some() {
                if !self.copy_values.contains_key(argument) {
                    return invalid(
                        "E-CODEGEN-CFG",
                        "large ABI argument has no snapshot",
                        Some(instruction.span),
                    );
                }
                args.push(m::Arg::new(m::Ty::Ptr, copy_aggregates::pointer(*argument)));
            } else if *ty != Type::Unit {
                args.push(m::Arg::new(
                    llvm_type(ty)?,
                    m::local(format!("v.{}", argument.0)),
                ));
            }
        }
        let output_type = instruction
            .result
            .map(|id| self.value_type(id))
            .transpose()?;
        let machine_result = if signature.result.is_some() {
            m::Ty::Void
        } else {
            output_type
                .map(llvm_type)
                .transpose()?
                .unwrap_or(m::Ty::Void)
        };
        let assignment =
            (machine_result != m::Ty::Void).then(|| format!("v.{}", instruction.result.unwrap().0));
        self.event(m::Event::Instruction(m::Inst::new(
            assignment,
            m::Op::Call {
                result: machine_result,
                result_attrs: vec![],
                callee,
                args,
                vararg: None,
            },
        )));
        Ok(())
    }
}
