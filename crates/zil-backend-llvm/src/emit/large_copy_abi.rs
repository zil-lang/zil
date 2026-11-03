//! LargeCopy ABI v1: source values stay values; physical pointers only name
//! independent snapshots, with a mandatory callee copy-in before source code.
use super::*;
use copy_aggregates::{Slot, classify};
use zil_base::Limits;

#[derive(Clone, Default)]
pub(super) struct Signature {
    pub(super) parameters: Vec<Option<Slot>>,
    pub(super) result: Option<Slot>,
    pub(super) versioned: bool,
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
// A data pointer to Big keeps its old ABI. A pointer (or any carrier) to
// language code whose invocation ABI changed must isolate old consumers.
fn dependency(
    ty: &Type,
    remaining: &mut usize,
    limits: &Limits,
    depth: usize,
) -> CompileResult<bool> {
    *remaining = remaining.checked_sub(1).ok_or_else(|| {
        vec![Diagnostic::limit(
            Phase::Codegen,
            "large Copy ABI dependency budget exceeded",
            None,
        )]
    })?;
    if depth > limits.max_call_depth {
        return Err(vec![Diagnostic::limit(
            Phase::Codegen,
            "large Copy ABI dependency depth exceeded",
            None,
        )]);
    }
    let mut changed = false;
    match ty {
        Type::FunctionPointer { parameters, result } => {
            for parameter in parameters {
                changed |= classify(parameter, remaining, limits)?.is_some();
                changed |= dependency(parameter, remaining, limits, depth + 1)?;
            }
            changed |= classify(result, remaining, limits)?.is_some();
            changed |= dependency(result, remaining, limits, depth + 1)?;
        }
        Type::ForeignCallback { parameters, result } => {
            for parameter in parameters {
                changed |= classify(parameter, remaining, limits)?.is_some();
                changed |= dependency(parameter, remaining, limits, depth + 1)?;
            }
            changed |= classify(result, remaining, limits)?.is_some();
            changed |= dependency(result, remaining, limits, depth + 1)?;
            if changed {
                return unsupported(
                    "foreign callback cannot carry the LargeCopy language ABI",
                    None,
                );
            }
        }
        Type::Array { element, .. } | Type::Slice { element } | Type::OwnedStore { element } => {
            changed |= dependency(element, remaining, limits, depth + 1)?
        }
        Type::Address { inner, .. }
        | Type::Managed(inner)
        | Type::Desc(inner)
        | Type::Opaque { hidden: inner, .. } => {
            changed |= dependency(inner, remaining, limits, depth + 1)?
        }
        Type::Tuple(fields) => {
            for field in fields {
                changed |= dependency(field, remaining, limits, depth + 1)?;
            }
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            for (_, field) in fields {
                changed |= dependency(field, remaining, limits, depth + 1)?;
            }
        }
        Type::Enum { variants, .. } => {
            for (_, fields) in variants {
                for field in fields {
                    changed |= dependency(field, remaining, limits, depth + 1)?;
                }
            }
        }
        Type::ErrorUnion { error, result } => {
            changed |= dependency(error, remaining, limits, depth + 1)?;
            changed |= dependency(result, remaining, limits, depth + 1)?;
        }
        _ => {}
    }
    Ok(changed)
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
    let mut versioned = false;
    let parameters = parameters
        .map(|ty| {
            let slot = classify(ty, remaining, limits)?;
            versioned |= slot.is_some();
            versioned |= dependency(ty, remaining, limits, 0)?;
            Ok(slot)
        })
        .collect::<CompileResult<Vec<_>>>()?;
    let result_slot = classify(result, remaining, limits)?;
    versioned |= result_slot.is_some();
    versioned |= dependency(result, remaining, limits, 0)?;
    Ok(Signature {
        parameters,
        result: result_slot,
        versioned,
    })
}
pub(super) fn plan(
    lir: &LirView<'_>,
    limits: &Limits,
    remaining: &mut usize,
) -> CompileResult<Plans> {
    let mut plans = Plans::default();
    // Special generated consumers retain their previous complete convention.
    // Reject affected hidden interfaces rather than emitting an old-name call
    // to a new-name definition (or silently exposing a changed code pointer).
    for closure in lir.closures {
        for capture in &closure.captures {
            if dependency(capture, remaining, limits, 0)? {
                return unsupported("LargeCopy function pointer in closure environment", None);
            }
        }
    }
    for resource in lir.resources {
        if dependency(&resource.payload, remaining, limits, 0)? {
            return unsupported(
                "LargeCopy function pointer in resource helper interface",
                None,
            );
        }
    }
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
        if abi.versioned && (function.foreign.is_some() || function.environment.is_some()) {
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
                && plans
                    .functions
                    .get(&worker)
                    .is_some_and(|abi| abi.versioned)
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
            versioned: false,
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
            .is_some_and(|abi| abi.versioned)
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

#[cfg(test)]
mod tests {
    use super::*;
    fn worker() -> Type {
        Type::FunctionPointer {
            parameters: vec![Type::Array {
                element: Box::new(Type::U8),
                length: 4096,
            }],
            result: Box::new(Type::I32),
        }
    }
    #[test]
    fn version_dependency_covers_carriers_without_changing_pointer_physics() {
        let limits = Limits::default();
        let fp = worker();
        let carriers = vec![
            fp.clone(),
            Type::Array {
                element: Box::new(fp.clone()),
                length: 1,
            },
            Type::Tuple(vec![Type::Unit, fp.clone()]),
            Type::Record {
                name: "Handle".into(),
                fields: vec![("call".into(), fp.clone())],
            },
            Type::Package {
                name: "Package".into(),
                fields: vec![("call".into(), fp.clone())],
            },
            Type::Enum {
                name: "Choice".into(),
                variants: vec![("No".into(), vec![]), ("Yes".into(), vec![fp.clone()])],
            },
            Type::ErrorUnion {
                error: Box::new(Type::I32),
                result: Box::new(fp.clone()),
            },
            Type::Address {
                mutable: false,
                inner: Box::new(fp.clone()),
            },
            Type::Desc(Box::new(fp.clone())),
            Type::Managed(Box::new(fp.clone())),
            Type::Opaque {
                identity: zil_base::NodeId(0),
                hidden: Box::new(fp.clone()),
            },
            Type::FunctionPointer {
                parameters: vec![],
                result: Box::new(fp),
            },
        ];
        for carrier in carriers {
            let mut remaining = limits.max_analysis_steps;
            let abi = signature(
                std::iter::once(&carrier),
                &Type::Unit,
                &mut remaining,
                &limits,
            )
            .unwrap();
            assert!(abi.versioned);
            assert!(
                !abi.large(),
                "carrier pointer/aggregate must retain its physical convention"
            );
        }
        let data = Type::Address {
            mutable: false,
            inner: Box::new(Type::Array {
                element: Box::new(Type::U8),
                length: 4096,
            }),
        };
        let mut remaining = limits.max_analysis_steps;
        let abi = signature(std::iter::once(&data), &Type::Unit, &mut remaining, &limits).unwrap();
        assert!(!abi.versioned && !abi.large());
        let c_pointer = Type::ForeignCallback {
            parameters: vec![Type::I32],
            result: Box::new(Type::I32),
        };
        assert!(
            !signature(std::iter::empty(), &c_pointer, &mut remaining, &limits)
                .unwrap()
                .versioned
        );
        let foreign = Type::ForeignCallback {
            parameters: vec![worker()],
            result: Box::new(Type::I32),
        };
        assert_eq!(
            dependency(&foreign, &mut remaining, &limits, 0).unwrap_err()[0].category,
            zil_base::ResultCategory::Unsupported
        );
    }
    #[test]
    fn recursive_dependency_cost_shares_the_existing_remainder_exactly() {
        let limits = Limits::default();
        let carrier = Type::Tuple(vec![
            worker(),
            Type::Address {
                mutable: false,
                inner: Box::new(worker()),
            },
        ]);
        let mut remaining = limits.max_analysis_steps;
        signature(
            std::iter::once(&carrier),
            &Type::Unit,
            &mut remaining,
            &limits,
        )
        .unwrap();
        let cost = limits.max_analysis_steps - remaining;
        for available in [cost, cost] {
            let mut remaining = available;
            assert!(
                signature(
                    std::iter::once(&carrier),
                    &Type::Unit,
                    &mut remaining,
                    &limits
                )
                .unwrap()
                .versioned
            );
            assert_eq!(remaining, 0);
        }
        let mut remaining = cost - 1;
        assert_eq!(
            signature(
                std::iter::once(&carrier),
                &Type::Unit,
                &mut remaining,
                &limits
            )
            .err()
            .unwrap()[0]
                .category,
            zil_base::ResultCategory::ResourceLimit
        );
    }
}
