//! LargeCopy physical ABI v1: source values stay values; pointers only name
//! independent snapshots, with a mandatory callee copy-in before source code.
//! Owned Package transfers use the same machine convention without gaining
//! language Copy or duplicating the verified cleanup obligation.
use super::*;
use copy_aggregates::{Slot, classify};
use zil_base::Limits;

#[derive(Clone, Default)]
#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
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
#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
pub(super) struct Plans {
    pub(super) remaining_work: usize,
    pub(super) values: BTreeMap<FunctionId, copy_aggregates::Plan>,
    pub(super) functions: BTreeMap<FunctionId, Signature>,
    pub(super) indirect: BTreeMap<(FunctionId, ValueId), Signature>,
    pub(super) store_protocols: BTreeMap<usize, dynamic_store::Protocols>,
}
impl Plans {
    pub(super) fn needs_helpers(&self) -> bool {
        !self.values.is_empty()
            || self.functions.values().any(Signature::large)
            || self
                .store_protocols
                .values()
                .any(dynamic_store::Protocols::large)
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
        Type::BorrowedStorage { storage, life } => {
            stored_borrows::shape(life, remaining)?;
            changed |= dependency(storage, remaining, limits, depth + 1)?;
        }
        Type::FunctionPointer { parameters, result }
        | Type::BorrowedFunctionPointer {
            parameters, result, ..
        } => {
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
        if function.name == "main"
            && let Some(slot) = &abi.result
            && matches!(&function.return_type, Type::ErrorUnion { result, .. }
                if matches!(result.as_ref(), Type::I32 | Type::Unit))
        {
            // A native main wrapper must call this same physical signature.
            // Reserve its complete type clones and fixed tag/success events
            // before the wrapper grows a machine event buffer.
            copy_aggregates::reserve(&function.return_type, remaining, 0, limits)?;
            *remaining = remaining.checked_sub(64).ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Codegen,
                    "large Copy entry wrapper budget exceeded",
                    Some(function.span),
                )]
            })?;
            debug_assert!(slot.bytes >= 4096);
        }
        if abi.versioned && function.foreign.is_some() {
            return unsupported("large Copy foreign entry ABI", Some(function.span));
        }
        plans.functions.insert(function.id, abi);
        for (index, ty) in function.values.iter().enumerate() {
            if let Type::FunctionPointer { parameters, result }
            | Type::BorrowedFunctionPointer {
                parameters, result, ..
            } = ty
            {
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
            if let InstructionKind::StaticUtf8 { bytes } = &instruction.kind {
                // Reuse the existing full instruction inventory. Reserve exact
                // data copying, escaped text/native constants, symbol/hash work
                // and the two slice instructions before any machine emission.
                super::static_utf8::reserve_emission(bytes.len(), remaining)?;
            }
            // Generated callback/thread/cell bridges keep their old complete
            // convention. Only ordinary closure invocation uses env/out/copy-in.
            let helpers = match instruction.kind {
                InstructionKind::WithForeignCallback { callback, body, .. } => {
                    [Some(callback), Some(body)]
                }
                InstructionKind::RegistrationStart { callback, .. }
                | InstructionKind::AsyncRegistrationStart { callback, .. } => {
                    [Some(callback), None]
                }
                InstructionKind::WithScopedThread { body, .. }
                | InstructionKind::WithSyncCell { body, .. } => [Some(body), None],
                _ => [None, None],
            };
            for helper in helpers.into_iter().flatten() {
                *remaining = remaining
                    .checked_sub(lir.closures.len().saturating_add(1))
                    .ok_or_else(|| {
                        vec![Diagnostic::limit(
                            Phase::Codegen,
                            "large Copy closure helper budget exceeded",
                            Some(instruction.span),
                        )]
                    })?;
                let ty = &function.values[helper.0 as usize];
                let ty = match ty {
                    Type::Address { inner, .. } => inner.as_ref(),
                    ty => ty,
                };
                if let Type::Closure(id) = ty
                    && let Some(closure) = lir.closures.iter().find(|closure| closure.id == *id)
                    && plans
                        .functions
                        .get(&closure.function)
                        .is_some_and(|abi| abi.versioned)
                {
                    return unsupported(
                        "large Copy generated closure helper ABI",
                        Some(instruction.span),
                    );
                }
            }
            if let InstructionKind::TaskCreate { worker, .. }
            | InstructionKind::SuspendCall {
                function: worker, ..
            }
            | InstructionKind::TaskStart { worker, .. }
            | InstructionKind::WithScopedThread { worker, .. } = instruction.kind
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

// Only ordinary Copy ErrorUnion mains are classified. Owned entry cleanup
// remains in the original wrapper; here the error boundary only consumes a tag.
pub(super) fn entry_error_wrapper(
    mut code: m::Code,
    symbol: &str,
    slot: &Slot,
    result: &Type,
) -> m::Code {
    code.append(m::Code::from(vec![
        m::Event::Instruction(m::Inst::new(
            Some("entry.storage".into()),
            m::Op::Alloca {
                ty: slot.ty.clone(),
                align: slot.align,
            },
        )),
        m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Call {
                result: m::Ty::Void,
                result_attrs: vec![],
                callee: m::global(symbol.into()),
                args: vec![m::Arg::new(m::Ty::Ptr, m::local("entry.storage".into()))],
                vararg: None,
            },
        )),
        m::Event::Instruction(m::Inst::new(
            Some("entry.tag".into()),
            m::Op::Gep {
                ty: slot.ty.clone(),
                ptr: m::local("entry.storage".into()),
                indices: vec![
                    m::Arg::new(m::Ty::Int(32), 0),
                    m::Arg::new(m::Ty::Int(32), 0),
                ],
            },
        )),
        m::Event::Instruction(m::Inst::new(
            Some("entry.error".into()),
            m::Op::Load {
                ty: m::Ty::Int(1),
                ptr: m::local("entry.tag".into()),
                align: 1,
            },
        )),
        m::Event::Instruction(m::Inst::new(
            None,
            m::Op::CondBranch {
                condition: m::local("entry.error".into()),
                yes: "error".into(),
                no: "success".into(),
            },
        )),
        m::Event::Label("error".into()),
        m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Return(Some(m::Arg::new(m::Ty::Int(32), 1))),
        )),
        m::Event::Label("success".into()),
    ]));
    if *result == Type::I32 {
        code.append(m::Code::from(vec![
            m::Event::Instruction(m::Inst::new(
                Some("entry.success.address".into()),
                m::Op::Gep {
                    ty: slot.ty.clone(),
                    ptr: m::local("entry.storage".into()),
                    indices: vec![
                        m::Arg::new(m::Ty::Int(32), 0),
                        m::Arg::new(m::Ty::Int(32), 2),
                    ],
                },
            )),
            m::Event::Instruction(m::Inst::new(
                Some("entry.success".into()),
                m::Op::Load {
                    ty: m::Ty::Int(32),
                    ptr: m::local("entry.success.address".into()),
                    align: 4,
                },
            )),
            m::Event::Instruction(m::Inst::new(
                None,
                m::Op::Return(Some(m::Arg::new(
                    m::Ty::Int(32),
                    m::local("entry.success".into()),
                ))),
            )),
        ]));
    } else {
        code.push(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Return(Some(m::Arg::new(m::Ty::Int(32), 0))),
        )));
    }
    code.push(m::Event::End);
    code
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
        self.large_copy_call_with_prefix(instruction, signature, callee, Vec::new(), arguments)
    }
    // Ordinary closure entries prepend their real environment before the same
    // out-pointer and source-parameter mapping used by ordinary functions.
    pub(super) fn large_copy_call_with_prefix(
        &mut self,
        instruction: &Instruction,
        signature: &Signature,
        callee: m::Val,
        mut args: Vec<m::Arg>,
        arguments: &[ValueId],
    ) -> CompileResult<()> {
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
    #[test]
    fn sum_threshold_and_nested_function_pointer_carriers_share_the_lc1_plan() {
        let limits = Limits::default();
        for (sum, bytes) in [
            (
                Type::Enum {
                    name: "AtThreshold".into(),
                    variants: vec![(
                        "Data".into(),
                        vec![Type::Array {
                            element: Box::new(Type::U8),
                            length: 4092,
                        }],
                    )],
                },
                4096,
            ),
            (
                Type::ErrorUnion {
                    error: Box::new(Type::Bool),
                    result: Box::new(Type::Array {
                        element: Box::new(Type::U8),
                        length: 4094,
                    }),
                },
                4096,
            ),
        ] {
            let mut remaining = limits.max_analysis_steps;
            let direct = signature(std::iter::once(&sum), &sum, &mut remaining, &limits).unwrap();
            assert!(direct.versioned && direct.large());
            assert_eq!(direct.parameters[0].as_ref().unwrap().bytes, bytes);
            assert_eq!(direct.result.unwrap().bytes, bytes);
            let worker = Type::FunctionPointer {
                parameters: vec![sum.clone()],
                result: Box::new(sum.clone()),
            };
            for carrier in [
                Type::Enum {
                    name: "Handle".into(),
                    variants: vec![("Call".into(), vec![worker.clone()])],
                },
                Type::ErrorUnion {
                    error: Box::new(Type::Bool),
                    result: Box::new(worker),
                },
            ] {
                let abi = signature(std::iter::empty(), &carrier, &mut remaining, &limits).unwrap();
                assert!(abi.versioned && !abi.large());
            }
            let address = Type::Address {
                mutable: false,
                inner: Box::new(sum.clone()),
            };
            let abi = signature(
                std::iter::once(&address),
                &Type::I32,
                &mut remaining,
                &limits,
            )
            .unwrap();
            assert!(!abi.versioned && !abi.large());
            let foreign = Type::ForeignCallback {
                parameters: vec![sum],
                result: Box::new(Type::I32),
            };
            assert_eq!(
                dependency(&foreign, &mut remaining, &limits, 0).unwrap_err()[0].category,
                zil_base::ResultCategory::Unsupported
            );
        }
        for small in [
            Type::Enum {
                name: "BelowThreshold".into(),
                variants: vec![(
                    "Data".into(),
                    vec![Type::Array {
                        element: Box::new(Type::U8),
                        length: 4088,
                    }],
                )],
            },
            Type::ErrorUnion {
                error: Box::new(Type::Bool),
                result: Box::new(Type::Array {
                    element: Box::new(Type::U8),
                    length: 4093,
                }),
            },
        ] {
            let mut remaining = limits.max_analysis_steps;
            let abi = signature(std::iter::once(&small), &small, &mut remaining, &limits).unwrap();
            assert!(!abi.versioned && !abi.large());
        }
    }
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
