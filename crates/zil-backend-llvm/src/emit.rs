mod arrays;
mod byte_buffer;
mod constant_divisors;
mod drop_plan;
mod dynamic_store;
mod enums;
mod errors;
mod foreign;
mod owned_sums;
mod packages;
mod parallel_lowering;
mod resource_closures;
mod resources;
mod scalar_leaf;
mod scalar_locals;
mod scalar_summaries;
mod slices;
use std::collections::{BTreeMap, BTreeSet};
use zil_llvm_api::model as m;

use zil_base::{CompileResult, Diagnostic, Phase, SCHEMA_VERSION, Span, TargetProfile};
use zil_ir::{
    BinaryOp, CallableKind, ClosureDefinition, ClosureId, Constant, FunctionId, Instruction,
    InstructionKind, LirModule, LirView, LocalId, ResourceDefinition, ScalarLayout, Terminator,
    Type, TypedFunction, UnaryOp, ValueId,
};
use zil_verify::VerifiedModule;

use crate::{EmitOptions, EntryPoint};

/// Emit a module without an executable entry wrapper. Use `emit_with_options`
/// when an executable or the full-return-value harness is required.
pub fn emit(verified: &VerifiedModule, lir: &LirModule) -> CompileResult<String> {
    emit_with_options(verified, lir, &EmitOptions::default())
}

pub fn emit_with_options(
    verified: &VerifiedModule,
    lir: &LirModule,
    options: &EmitOptions,
) -> CompileResult<String> {
    Ok(lower_with_options(verified, lir, options)?.to_text())
}

pub(crate) fn lower_with_options(
    verified: &VerifiedModule,
    lir: &LirModule,
    options: &EmitOptions,
) -> CompileResult<m::Module> {
    lower_view_with_options(verified, &lir.as_view(), options)
}

pub(crate) fn lower_view_with_options(
    verified: &VerifiedModule,
    lir: &LirView<'_>,
    options: &EmitOptions,
) -> CompileResult<m::Module> {
    lower_view_with_incremental_scalar_policy(verified, lir, options, false)
}

pub(crate) fn lower_view_with_incremental_scalar_policy(
    verified: &VerifiedModule,
    lir: &LirView<'_>,
    options: &EmitOptions,
    incremental_scalar_policy: bool,
) -> CompileResult<m::Module> {
    lower_view_with_closed_scalar_policy(verified, lir, options, incremental_scalar_policy, false)
        .map(|(module, _)| module)
}

pub(crate) fn lower_view_with_closed_scalar_policy(
    verified: &VerifiedModule,
    lir: &LirView<'_>,
    options: &EmitOptions,
    incremental_scalar_policy: bool,
    closed_scalar_collection: bool,
) -> CompileResult<(m::Module, bool)> {
    lower_view_with_closed_scalar_policy_and_jobs(
        verified,
        lir,
        options,
        incremental_scalar_policy,
        closed_scalar_collection,
        1,
    )
}

pub(crate) fn lower_view_with_closed_scalar_policy_and_jobs(
    verified: &VerifiedModule,
    lir: &LirView<'_>,
    options: &EmitOptions,
    incremental_scalar_policy: bool,
    closed_scalar_collection: bool,
    jobs: usize,
) -> CompileResult<(m::Module, bool)> {
    let sum_helpers = bind_lir(verified, lir, options.entry)?;
    let functions = lir
        .functions
        .iter()
        .map(|function| (function.id, function))
        .collect::<BTreeMap<_, _>>();
    let closures = lir
        .closures
        .iter()
        .map(|closure| (closure.id, closure))
        .collect::<BTreeMap<_, _>>();
    let wrapper = entry_wrapper(&functions, options.entry, lir.resources)?;
    let summaries = incremental_scalar_policy.then(|| scalar_summaries::plan(&functions));
    let collected = (closed_scalar_collection
        && options.entry == EntryPoint::Main
        && lir.target.triple == "x86_64-unknown-linux-gnu")
        .then(|| {
            summaries
                .as_ref()
                .and_then(|facts| scalar_summaries::collect_closed(lir, &functions, facts))
        })
        .flatten();
    let windows = lir.target.triple == "x86_64-pc-windows-msvc";
    // Constant summaries only need tiny result stubs, and a closed collection
    // can discard most source functions. Neither warrants worker allocation.
    // This is a conservative upper bound: foreign declarations still count.
    let remaining_bodies = lir
        .functions
        .len()
        .saturating_sub(summaries.as_ref().map_or(0, BTreeMap::len));
    let jobs = if remaining_bodies < parallel_lowering::MIN_PARALLEL_FUNCTIONS
        || collected
            .as_ref()
            .is_some_and(|needed| needed.len() < parallel_lowering::MIN_PARALLEL_FUNCTIONS)
    {
        1
    } else {
        jobs
    };
    let (jobs, prepared_leaves) = if jobs > 1 {
        let mut work = parallel_lowering::RemainingWork::default();
        let mut leaves = incremental_scalar_policy.then(|| Vec::with_capacity(lir.functions.len()));
        let mut batch_work = 0usize;
        for (index, function) in lir.functions.iter().enumerate() {
            let ordinary = function.foreign.is_none()
                && collected
                    .as_ref()
                    .is_none_or(|needed| needed.contains(&function.id))
                && summaries
                    .as_ref()
                    .is_none_or(|facts| !facts.contains_key(&function.id));
            let prepared = (ordinary && incremental_scalar_policy)
                .then(|| scalar_leaf::prepare(function))
                .flatten();
            if ordinary && prepared.is_none() {
                let initial = function
                    .parameters
                    .len()
                    .saturating_add(function.locals.len())
                    .saturating_add(function.values.len());
                let cost = function.blocks.iter().fold(initial, |sum, block| {
                    sum.saturating_add(block.instructions.len())
                        .saturating_add(1)
                });
                batch_work = batch_work.saturating_add(cost);
            }
            if index % parallel_lowering::BATCH_SIZE + 1 == parallel_lowering::BATCH_SIZE {
                work.add(batch_work);
                batch_work = 0;
            }
            if let Some(leaves) = &mut leaves {
                leaves.push(prepared);
            }
        }
        work.add(batch_work);
        (work.jobs(jobs), leaves)
    } else {
        (jobs, None)
    };
    let batches = parallel_lowering::ordered_batches(lir.functions, jobs, |start, batch| {
        lower_function_batch(
            batch,
            lir,
            options,
            incremental_scalar_policy,
            &functions,
            &closures,
            summaries.as_ref(),
            collected.as_ref(),
            prepared_leaves
                .as_ref()
                .map(|leaves| &leaves[start..start + batch.len()]),
        )
    })?;
    let mut batches = batches.into_iter();
    let FunctionBatch {
        mut body,
        mut intrinsics,
    } = batches.next().expect("lowering always returns a batch");
    body.reserve(
        collected
            .as_ref()
            .map_or(lir.functions.len(), BTreeSet::len)
            .saturating_sub(body.len()),
    );
    for batch in batches {
        body.extend(batch.body);
        intrinsics.extend(batch.intrinsics);
    }
    let mut text = m::Code::new();
    text.append(sum_helpers);
    for closure in lir.closures {
        let mut fields = closure
            .captures
            .iter()
            .map(capture_type)
            .collect::<CompileResult<Vec<_>>>()?;
        fields.extend(
            closure
                .captures
                .iter()
                .flat_map(|ty| std::iter::repeat_n("i1".into(), resource_closures::flag_count(ty))),
        );

        text.push(m::Event::NamedType(
            m::type_name(&(environment_type(closure.id)).clone()),
            (fields).clone(),
        ));
    }
    text.append(resource_closures::drop_functions(
        lir.closures,
        lir.resources,
    )?);
    for function in lir.functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                if let InstructionKind::Constant(Constant::WideString(units)) = &instruction.kind {
                    let id = result(instruction)?;
                    let data = units
                        .iter()
                        .map(|unit| m::Arg::new(m::Ty::Int(16), *unit))
                        .collect::<Vec<_>>();
                    text.push(m::Event::Global(m::Global {
                        name: format!("zil.wstr.{}.{}", function.id.0, id.0),
                        linkage: m::Linkage::Private,
                        ty: m::Ty::array((units.len()) as u64, m::Ty::Int(16)),
                        initializer: Some(m::Val::Array({
                            let mut args = vec![];
                            args.extend((data).clone());
                            args
                        })),
                        constant: true,
                        unnamed_addr: true,
                        alignment: 2,
                    }));
                }
            }
        }
    }
    text.push(m::Event::Declare(m::Function {
        name: "llvm.trap".into(),
        linkage: m::Linkage::External,
        signature: m::Signature {
            result: m::Ty::Void,
            result_attrs: vec![],
            params: vec![],
            variadic: false,
        },
        attrs: vec![],
        blocks: None,
    }));
    text.append(byte_buffer::runtime(lir)?);
    text.append(dynamic_store::runtime(lir, &closures)?);
    for (operation, bits) in intrinsics {
        text.push(m::Event::Declare(m::Function {
            name: format!("llvm.{}.with.overflow.i{}", operation, bits),
            linkage: m::Linkage::External,
            signature: m::Signature {
                result: m::Ty::Struct(vec![m::Ty::Int(bits), m::Ty::Int(1)]),
                result_attrs: vec![],
                params: vec![
                    m::Param {
                        ty: m::Ty::Int(bits),
                        name: None,
                        attrs: vec![],
                    },
                    m::Param {
                        ty: m::Ty::Int(bits),
                        name: None,
                        attrs: vec![],
                    },
                ],
                variadic: false,
            },
            attrs: vec![],
            blocks: None,
        }));
    }
    if matches!(options.entry, EntryPoint::ReturnValue(_)) {
        text.push(m::Event::Global(m::Global {
            name: "zil.return.format".into(),
            linkage: m::Linkage::Private,
            ty: m::Ty::array(6, m::Ty::Int(8)),
            initializer: Some(m::Val::Bytes(vec![37, 108, 108, 100, 10, 0])),
            constant: true,
            unnamed_addr: true,
            alignment: 0,
        }));
        text.push(m::Event::Declare(m::Function {
            name: "printf".into(),
            linkage: m::Linkage::External,
            signature: m::Signature {
                result: m::Ty::Int(32),
                result_attrs: vec![],
                params: vec![m::Param {
                    ty: m::Ty::Ptr,
                    name: None,
                    attrs: vec![],
                }],
                variadic: true,
            },
            attrs: vec![],
            blocks: None,
        }));
    }

    let mut items = text.into_items().map_err(machine_error)?;
    items.extend(body);
    items.extend(wrapper.into_items().map_err(machine_error)?);
    let mut text = m::Code::new();
    if windows && options.entry != EntryPoint::None {
        if let Some(existing) = lir.functions.iter().find(|f| {
            f.foreign
                .as_ref()
                .is_some_and(|x| x.symbol == "ExitProcess")
        }) {
            if existing.return_type != Type::Unit
                || existing.parameters.len() != 1
                || existing.locals[existing.parameters[0].0 as usize].ty != Type::I32
            {
                return invalid(
                    "E-CODEGEN-FOREIGN",
                    "ExitProcess conflicts with the Win64 entry ABI",
                    Some(existing.span),
                );
            }
        } else {
            text.push(m::Event::Declare(m::Function {
                name: "ExitProcess".into(),
                linkage: m::Linkage::External,
                signature: m::Signature {
                    result: m::Ty::Void,
                    result_attrs: vec![],
                    params: vec![m::Param {
                        ty: m::Ty::Int(32),
                        name: None,
                        attrs: vec![],
                    }],
                    variadic: false,
                },
                attrs: vec![],
                blocks: None,
            }));
        }
        text.append(m::Code::from(vec![
            m::Event::Begin(m::Function {
                name: "zil.pe.entry".into(),
                linkage: m::Linkage::External,
                signature: m::Signature {
                    result: m::Ty::Void,
                    result_attrs: vec![],
                    params: vec![],
                    variadic: false,
                },
                attrs: vec![],
                blocks: None,
            }),
            m::Event::Label("entry".into()),
            m::Event::Instruction(m::Inst::new(
                Some("code".into()),
                m::Op::Call {
                    result: m::Ty::Int(32),
                    result_attrs: vec![],
                    callee: m::global("main".into()),
                    args: vec![],
                    vararg: None,
                },
            )),
            m::Event::Instruction(m::Inst::new(
                None,
                m::Op::Call {
                    result: m::Ty::Void,
                    result_attrs: vec![],
                    callee: m::global("ExitProcess".into()),
                    args: vec![m::Arg::new(m::Ty::Int(32), m::local("code".into()))],
                    vararg: None,
                },
            )),
            m::Event::Instruction(m::Inst::new(None, m::Op::Unreachable)),
            m::Event::End,
        ]));
    }
    items.extend(text.into_items().map_err(machine_error)?);
    Ok((
        m::Module {
            triple: lir.target.triple.clone(),
            fingerprint: verified.fingerprint().to_hex(),
            items,
        },
        collected.is_some(),
    ))
}

struct FunctionBatch {
    body: Vec<m::Item>,
    intrinsics: BTreeSet<(&'static str, u32)>,
}

#[allow(clippy::too_many_arguments)]
fn lower_function_batch(
    batch: &[TypedFunction],
    lir: &LirView<'_>,
    options: &EmitOptions,
    incremental_scalar_policy: bool,
    functions: &BTreeMap<FunctionId, &TypedFunction>,
    closures: &BTreeMap<ClosureId, &ClosureDefinition>,
    summaries: Option<&BTreeMap<FunctionId, Constant>>,
    collected: Option<&BTreeSet<FunctionId>>,
    prepared_leaves: Option<&[Option<scalar_leaf::Prepared>]>,
) -> CompileResult<FunctionBatch> {
    let mut intrinsics = BTreeSet::new();
    // Finish each function before retaining it: no whole-project Event buffer.
    let mut body =
        Vec::with_capacity(collected.map_or(batch.len(), |needed| needed.len().min(batch.len())));
    for (offset, function) in batch.iter().enumerate() {
        if function.foreign.is_some() {
            if matches!(options.entry, EntryPoint::ReturnValue(_))
                && function
                    .foreign
                    .as_ref()
                    .is_some_and(|f| f.symbol == "printf")
            {
                return invalid(
                    "E-CODEGEN-FOREIGN",
                    "foreign printf conflicts with the optional return-value harness",
                    Some(function.span),
                );
            }
            body.extend(
                foreign::declaration(function, lir.target)?
                    .into_items()
                    .map_err(machine_error)?,
            );
            continue;
        }
        if collected.is_some_and(|needed| !needed.contains(&function.id)) {
            continue;
        }
        if let Some(value) = summaries.and_then(|plan| plan.get(&function.id)) {
            body.push(m::Item::Function(scalar_summaries::emit(function, value)));
            continue;
        }
        if incremental_scalar_policy {
            let leaf = if let Some(prepared) = prepared_leaves {
                prepared[offset].map(|leaf| leaf.emit(&mut intrinsics))
            } else {
                scalar_leaf::emit(function, &mut intrinsics)
            };
            if let Some(machine) = leaf {
                body.push(m::Item::Function(machine));
                continue;
            }
        }
        let emitted = FunctionEmitter {
            function,
            target: lir.target,
            functions,
            scalar_summaries: summaries,
            closures,
            resources: lir.resources,
            intrinsics: &mut intrinsics,
            text: m::Code::new(),
            temporary: 0,
            continuation: 0,
            partial_packages: BTreeSet::new(),
            safe_divisors: incremental_scalar_policy.then(BTreeSet::new),
        }
        .emit()?
        .into_items()
        .map_err(machine_error)?;
        for mut item in emitted {
            if let m::Item::Function(machine) = &mut item {
                scalar_locals::promote(function, machine);
            }
            body.push(item);
        }
    }
    Ok(FunctionBatch { body, intrinsics })
}

fn machine_error(message: String) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Codegen,
        "E-CODEGEN-CFG",
        None,
        message,
    )]
}

// Exact immutable aliasing proves equality, not authority. For independently
// allocated public raw inputs, compare every element as before.
fn same_slice<T: PartialEq>(a: &[T], b: &[T]) -> bool {
    std::ptr::eq(a, b) || a == b
}

fn bind_lir(
    verified: &VerifiedModule,
    lir: &LirView<'_>,
    entry: EntryPoint,
) -> CompileResult<m::Code> {
    let module = verified.module();
    let layouts = vec![
        ScalarLayout {
            ty: Type::Unit,
            size: 0,
            alignment: 1,
        },
        ScalarLayout {
            ty: Type::Bool,
            size: 1,
            alignment: 1,
        },
        ScalarLayout {
            ty: Type::I32,
            size: 4,
            alignment: 4,
        },
        ScalarLayout {
            ty: Type::I64,
            size: 8,
            alignment: 8,
        },
        ScalarLayout {
            ty: Type::U8,
            size: 1,
            alignment: 1,
        },
        ScalarLayout {
            ty: Type::U16,
            size: 2,
            alignment: 2,
        },
        ScalarLayout {
            ty: Type::ForeignHandle,
            size: 8,
            alignment: 8,
        },
        ScalarLayout {
            ty: Type::CWideString,
            size: 8,
            alignment: 8,
        },
    ];
    // LIR is publicly constructible. A copied fingerprint alone is insufficient:
    // the actual executable CFG, concrete environments, target and layout must
    // match this capability.
    if lir.schema_version != SCHEMA_VERSION
        || lir.schema_version != module.schema_version
        || lir.module_id != module.module_id
        || lir.verified_fingerprint != verified.fingerprint()
        || *lir.target != module.target
        || !same_slice(lir.functions, &module.functions)
        || !same_slice(lir.closures, &module.closures)
        || !same_slice(lir.resources, &module.resources)
        || lir.layouts.as_ref() != layouts
    {
        return invalid(
            "E-CODEGEN-UNBOUND-LIR",
            "LIR does not exactly match the supplied verified module, environments and scalar layout",
            None,
        );
    }
    if !lir.target.is_supported_x64_llvm() {
        return unsupported(
            "LLVM code generation supports checked x86_64 Linux and Win64 targets",
            None,
        );
    }
    // The public LIR has now been exactly rebound. The sealed bit only
    // enables payment; qualification and actual layouts are still rebuilt.
    let callback_owned_data = lir.closures.iter().any(|closure| {
        std::iter::once(&closure.result)
            .chain(closure.parameters.iter())
            .any(|ty| {
                matches!(
                    ty,
                    Type::Package { .. } | Type::Enum { .. } | Type::ErrorUnion { .. }
                ) && !ty.is_copy_value()
            })
    });
    let sum_helpers = drop_plan::functions(
        lir,
        verified.limits(),
        verified.uses_owned_sum_extension() || callback_owned_data,
        entry,
    )?;
    for closure in lir.closures {
        // Complete ordinary owning data travels through the same aggregate
        // argument/result ABI as a direct call. Keep capture qualification and
        // the older Copy/address callable profile independent of this path.
        let owning_result = matches!(
            closure.result,
            Type::Package { .. } | Type::Enum { .. } | Type::ErrorUnion { .. }
        ) && !closure.result.is_copy_value()
            && drop_plan::value_type(&closure.result, lir.resources);
        let owning_parameters = closure.parameters.iter().any(|ty| {
            matches!(
                ty,
                Type::Package { .. } | Type::Enum { .. } | Type::ErrorUnion { .. }
            ) && !ty.is_copy_value()
        });
        let owning_signature = (owning_result || owning_parameters)
            && (closure.result.is_copy_value() || owning_result)
            && closure.parameters.iter().all(|ty| {
                ty.is_copy_value()
                    || (matches!(
                        ty,
                        Type::Package { .. } | Type::Enum { .. } | Type::ErrorUnion { .. }
                    ) && drop_plan::value_type(ty, lir.resources))
            });
        if !matches!(
            closure.kind,
            CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce
        ) || !(owning_signature || closure.result.is_copy_value()
            || (!closure.lending_captures.is_empty()
                && matches!(closure.kind, CallableKind::Fn | CallableKind::FnMut)
                && matches!(&closure.result, Type::Address { inner, mutable: false }
                    if inner.is_copy_value() || matches!(&**inner, Type::Slice { element } if element.is_copy_value()))))
            || !closure.captures.iter().all(|ty| {
                ty.is_copy_value()
                    || matches!(ty, Type::Closure(_))
                    || matches!(ty, Type::Address { inner, .. } if inner.is_copy_value() || matches!(inner.as_ref(), Type::Closure(_)))
                    // The exact capability/LIR comparison above already binds
                    // origin admission; use the existing pointer field layout.
                    || (ty.is_package_address() && matches!(ty, Type::Address { mutable: false, .. }))
                    // The exact VerifiedModule binding above carries the
                    // independently checked owning Store capture contract.
                    // Borrowed descriptions do not gain an owning ABI here.
                    || (resources::supported_type(ty, lir.resources)
                        && !matches!(ty, Type::Desc(p) if matches!(p.as_ref(), Type::OwnedStore { .. })))
                    || packages::supported_capture_type(ty, lir.resources)
            })
            || !closure.parameters.iter().all(|ty| {
                owning_signature || ty.is_copy_value()
                    || (closure.result.is_copy_value()
                        && matches!(ty, Type::Address { inner, .. } if inner.is_address_pointee()))
            })
        {
            return unsupported("LLVM finite resource closure representation", None);
        }
    }
    for function in lir.functions {
        if !(function.return_type.is_copy_value()
            || function.return_type.is_scalar_or_address()
            || resources::supported_type(&function.return_type, lir.resources)
            || packages::supported_value_type(&function.return_type, lir.resources)
            || errors::supported_type(&function.return_type, lir.resources)
            || arrays::supported_type(&function.return_type, lir.resources)
            || (matches!(function.return_type, Type::Closure(_))
                && function.callable_return_contract.is_some()))
            || function.parameters.iter().any(|id| {
                let ty = &function.locals[id.0 as usize].ty;
                !(ty.is_copy_value()
                    || ty.is_scalar_or_address()
                    || resources::supported_type(ty, lir.resources)
                    || packages::supported_value_type(ty, lir.resources)
                    || errors::supported_type(ty, lir.resources)
                    || arrays::supported_type(ty, lir.resources)
                    || callable_parameter_type(ty, lir.closures))
            })
        {
            return unsupported("LLVM scalar function signature", Some(function.span));
        }
        for ty in function
            .locals
            .iter()
            .map(|local| &local.ty)
            .chain(function.values.iter())
        {
            let concrete_closure = callable_parameter_type(ty, lir.closures);
            if !ty.is_copy_value()
                && !concrete_closure
                && !matches!(ty, Type::Address { inner, .. } if inner.is_address_pointee())
                && !ty.is_shared_resource_head_address()
                && !ty.is_package_address()
                && !resources::supported_type(ty, lir.resources)
                && !packages::supported_value_type(ty, lir.resources)
                && !errors::supported_type(ty, lir.resources)
                && !arrays::supported_type(ty, lir.resources)
            {
                return unsupported(
                    format!("LLVM representation for {}", ty.name()),
                    Some(function.span),
                );
            }
        }
    }
    Ok(sum_helpers)
}

fn entry_wrapper(
    functions: &BTreeMap<FunctionId, &TypedFunction>,
    entry: EntryPoint,
    resources: &[ResourceDefinition],
) -> CompileResult<m::Code> {
    let function = match entry {
        EntryPoint::None => return Ok(m::Code::new()),
        EntryPoint::Main => {
            let mains = functions
                .values()
                .filter(|function| function.name == "main")
                .collect::<Vec<_>>();
            if mains.len() != 1 {
                return invalid(
                    "E-CODEGEN-ENTRY",
                    "an executable requires exactly one main function",
                    None,
                );
            }
            *mains[0]
        }
        EntryPoint::ReturnValue(id) => *functions.get(&id).ok_or_else(|| {
            vec![Diagnostic::error(
                Phase::Codegen,
                "E-CODEGEN-ENTRY",
                None,
                "the requested entry function is absent",
            )]
        })?,
    };
    if function.foreign.is_some() {
        return invalid(
            "E-CODEGEN-ENTRY",
            "a foreign declaration cannot be an executable entry",
            Some(function.span),
        );
    }
    if function.environment.is_some() {
        return invalid(
            "E-CODEGEN-ENTRY",
            "a closure entry requires its environment and cannot be a native entry",
            Some(function.span),
        );
    }
    if !function.parameters.is_empty() {
        return invalid(
            "E-CODEGEN-ENTRY",
            "the executable entry must have no parameters",
            Some(function.span),
        );
    }
    let symbol = symbol(function.id);
    let mut text = m::Code::from(vec![
        m::Event::Begin(m::Function {
            name: "main".into(),
            linkage: m::Linkage::External,
            signature: m::Signature {
                result: m::Ty::Int(32),
                result_attrs: vec![],
                params: vec![],
                variadic: false,
            },
            attrs: vec![],
            blocks: None,
        }),
        m::Event::Label("entry".into()),
    ]);
    match entry {
        EntryPoint::Main => match &function.return_type {
            Type::Unit => text.append(m::Code::from(vec![
                m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::Call {
                        result: m::Ty::Void,
                        result_attrs: vec![],
                        callee: m::global(symbol.to_string()),
                        args: vec![],
                        vararg: None,
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::Return(Some(m::Arg::new(m::Ty::Int(32), m::Val::Int(0)))),
                )),
            ])),
            Type::I32 => text.append(m::Code::from(vec![
                m::Event::Instruction(m::Inst::new(
                    Some("entry.result".into()),
                    m::Op::Call {
                        result: m::Ty::Int(32),
                        result_attrs: vec![],
                        callee: m::global(symbol.to_string()),
                        args: vec![],
                        vararg: None,
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::Return(Some(m::Arg::new(
                        m::Ty::Int(32),
                        m::local("entry.result".into()),
                    ))),
                )),
            ])),
            Type::ErrorUnion { result, .. }
                if matches!(result.as_ref(), Type::I32 | Type::Unit) =>
            {
                let llvm = llvm_type(&function.return_type)?;
                text.append(m::Code::from(vec![
                    m::Event::Instruction(m::Inst::new(
                        Some("entry.result".into()),
                        m::Op::Call {
                            result: (llvm).clone(),
                            result_attrs: vec![],
                            callee: m::global(symbol.to_string()),
                            args: vec![],
                            vararg: None,
                        },
                    )),
                    m::Event::Instruction(m::Inst::new(
                        Some("entry.error".into()),
                        m::Op::Extract {
                            arg: m::Arg::new((llvm).clone(), m::local("entry.result".into())),
                            index: 0,
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
                ]));
                text.append(errors::entry_cleanup(&function.return_type, resources)?);
                text.append(m::Code::from(vec![
                    m::Event::Instruction(m::Inst::new(
                        None,
                        m::Op::Return(Some(m::Arg::new(m::Ty::Int(32), m::Val::Int(1)))),
                    )),
                    m::Event::Label("success".into()),
                ]));
                if **result == Type::I32 {
                    text.append(m::Code::from(vec![
                        m::Event::Instruction(m::Inst::new(
                            Some("entry.success".into()),
                            m::Op::Extract {
                                arg: m::Arg::new((llvm).clone(), m::local("entry.result".into())),
                                index: 2,
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
                    text.push(m::Event::Instruction(m::Inst::new(
                        None,
                        m::Op::Return(Some(m::Arg::new(m::Ty::Int(32), m::Val::Int(0)))),
                    )));
                }
            }
            _ => {
                return unsupported(
                    "native main must return i32, unit, E!i32, or E!unit",
                    Some(function.span),
                );
            }
        },
        EntryPoint::ReturnValue(_) => {
            match function.return_type {
                Type::I64 => text.push(m::Event::Instruction(m::Inst::new(
                    Some("entry.wide".into()),
                    m::Op::Call {
                        result: m::Ty::Int(64),
                        result_attrs: vec![],
                        callee: m::global(symbol.to_string()),
                        args: vec![],
                        vararg: None,
                    },
                ))),
                Type::U16 => text.append(m::Code::from(vec![
                    m::Event::Instruction(m::Inst::new(
                        Some("entry.result".into()),
                        m::Op::Call {
                            result: m::Ty::Int(16),
                            result_attrs: vec![],
                            callee: m::global(symbol.to_string()),
                            args: vec![],
                            vararg: None,
                        },
                    )),
                    m::Event::Instruction(m::Inst::new(
                        Some("entry.wide".into()),
                        m::Op::Cast {
                            opcode: "zext".into(),
                            arg: m::Arg::new(m::Ty::Int(16), m::local("entry.result".into())),
                            to: m::Ty::Int(64),
                        },
                    )),
                ])),
                Type::U8 => text.append(m::Code::from(vec![
                    m::Event::Instruction(m::Inst::new(
                        Some("entry.result".into()),
                        m::Op::Call {
                            result: m::Ty::Int(8),
                            result_attrs: vec![],
                            callee: m::global(symbol.to_string()),
                            args: vec![],
                            vararg: None,
                        },
                    )),
                    m::Event::Instruction(m::Inst::new(
                        Some("entry.wide".into()),
                        m::Op::Cast {
                            opcode: "zext".into(),
                            arg: m::Arg::new(m::Ty::Int(8), m::local("entry.result".into())),
                            to: m::Ty::Int(64),
                        },
                    )),
                ])),
                Type::I32 => text.append(m::Code::from(vec![
                    m::Event::Instruction(m::Inst::new(
                        Some("entry.result".into()),
                        m::Op::Call {
                            result: m::Ty::Int(32),
                            result_attrs: vec![],
                            callee: m::global(symbol.to_string()),
                            args: vec![],
                            vararg: None,
                        },
                    )),
                    m::Event::Instruction(m::Inst::new(
                        Some("entry.wide".into()),
                        m::Op::Cast {
                            opcode: "sext".into(),
                            arg: m::Arg::new(m::Ty::Int(32), m::local("entry.result".into())),
                            to: m::Ty::Int(64),
                        },
                    )),
                ])),
                Type::Bool => text.append(m::Code::from(vec![
                    m::Event::Instruction(m::Inst::new(
                        Some("entry.result".into()),
                        m::Op::Call {
                            result: m::Ty::Int(1),
                            result_attrs: vec![],
                            callee: m::global(symbol.to_string()),
                            args: vec![],
                            vararg: None,
                        },
                    )),
                    m::Event::Instruction(m::Inst::new(
                        Some("entry.wide".into()),
                        m::Op::Cast {
                            opcode: "zext".into(),
                            arg: m::Arg::new(m::Ty::Int(1), m::local("entry.result".into())),
                            to: m::Ty::Int(64),
                        },
                    )),
                ])),
                _ => {
                    return unsupported(
                        "a return-value harness requires u8, u16, i32, i64, or bool",
                        Some(function.span),
                    );
                }
            }
            text.append(m::Code::from(vec![
                m::Event::Instruction(m::Inst::new(
                    Some("entry.printed".into()),
                    m::Op::Call {
                        result: m::Ty::Int(32),
                        result_attrs: vec![],
                        callee: m::global("printf".into()),
                        args: vec![
                            m::Arg::new(m::Ty::Ptr, m::global("zil.return.format".into())),
                            m::Arg::new(m::Ty::Int(64), m::local("entry.wide".into())),
                        ],
                        vararg: Some(vec![m::Ty::Ptr]),
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::Return(Some(m::Arg::new(m::Ty::Int(32), m::Val::Int(0)))),
                )),
            ]));
        }
        EntryPoint::None => unreachable!("the no-wrapper case returns above"),
    }
    text.push(m::Event::End);
    Ok(text)
}

struct FunctionEmitter<'a> {
    function: &'a TypedFunction,
    target: &'a TargetProfile,
    functions: &'a BTreeMap<FunctionId, &'a TypedFunction>,
    scalar_summaries: Option<&'a BTreeMap<FunctionId, Constant>>,
    closures: &'a BTreeMap<ClosureId, &'a ClosureDefinition>,
    resources: &'a [ResourceDefinition],
    intrinsics: &'a mut BTreeSet<(&'static str, u32)>,
    text: m::Code,
    temporary: u64,
    continuation: u64,
    // Only locals with field ownership/update operations need field flags.
    // Complete values retain the existing inline ABI and whole-local path.
    partial_packages: BTreeSet<LocalId>,
    safe_divisors: Option<BTreeSet<ValueId>>,
}

impl FunctionEmitter<'_> {
    fn emit(mut self) -> CompileResult<m::Code> {
        let mut parameters = self
            .function
            .parameters
            .iter()
            .enumerate()
            .map(|(index, local)| {
                let ty = self.local_type(*local)?;
                if *ty == Type::Unit {
                    Ok(None)
                } else {
                    Ok(Some(m::Param::new(llvm_type(ty)?, format!("arg.{index}"))))
                }
            })
            .collect::<CompileResult<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        if let Some(environment) = self.function.environment {
            let ty = if resource_closures::by_pointer(self.closure_definition(environment)?) {
                "ptr".into()
            } else {
                environment_type(environment)
            };
            parameters.insert(0, m::Param::new(ty, "env"));
        }

        self.event(m::Event::Begin(m::Function {
            name: symbol(self.function.id),
            linkage: m::Linkage::External,
            signature: m::Signature {
                result: llvm_type(&self.function.return_type)?,
                result_attrs: vec![],
                params: parameters,
                variadic: false,
            },
            attrs: vec![],
            blocks: None,
        }));
        self.event(m::Event::Label("entry".into()));
        let mut address_taken = BTreeSet::new();
        let mut package_views = BTreeMap::new();
        for instruction in self
            .function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
        {
            match instruction.kind {
                InstructionKind::Binary {
                    op: BinaryOp::Divide | BinaryOp::Remainder,
                    right,
                    ..
                } => constant_divisors::record(&mut self.safe_divisors, right),
                InstructionKind::TakePackageField { local, .. }
                | InstructionKind::DropPackageField { local, .. }
                | InstructionKind::StorePackageField { local, .. } => {
                    self.partial_packages.insert(local);
                }
                InstructionKind::BindPackageCapture { local, index } => {
                    self.partial_packages.insert(local);
                    package_views.insert(local, index);
                }
                InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local } => {
                    address_taken.insert(local);
                }
                _ => {}
            }
        }
        constant_divisors::retain_safe(self.function, &mut self.safe_divisors);
        for local in &self.function.locals {
            if let Some(index) = package_views.get(&local.id) {
                self.package_capture_storage(local.id, *index)?;
                continue;
            }
            if self.owns_storage(&local.ty) {
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(format!("resource.live.{}", local.id.0)),
                    m::Op::Alloca {
                        ty: m::Ty::Int(1),
                        align: 1,
                    },
                )));
                self.event(m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::Store {
                        arg: m::Arg::new(
                            m::Ty::Int(1),
                            m::val(self.function.parameters.contains(&local.id)),
                        ),
                        ptr: m::local(format!("resource.live.{}", local.id.0)),
                        align: 1,
                    },
                )));
            }
            if local.ty == Type::Unit && address_taken.contains(&local.id) {
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(format!("local.{}", local.id.0)),
                    m::Op::Alloca {
                        ty: m::Ty::Int(8),
                        align: 1,
                    },
                )));
            }
            if local.ty != Type::Unit {
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(format!("local.{}", local.id.0)),
                    m::Op::Alloca {
                        ty: (llvm_type(&local.ty)?).clone(),
                        align: (alignment(&local.ty, self.closures)?) as u32,
                    },
                )));
            }
            self.package_flags(local.id, true, self.function.parameters.contains(&local.id))?;
            self.owned_array_flags(local.id, true, self.function.parameters.contains(&local.id))?;
        }
        // Fixed entry-block scratch slots prevent dynamic indexing inside a
        // loop from growing the stack on every iteration.
        let mut once_slots = BTreeSet::new();
        for instruction in self.function.blocks.iter().flat_map(|b| &b.instructions) {
            self.enum_slot(instruction)?;
            if let InstructionKind::CallClosureOnce { callee, .. } = instruction.kind
                && once_slots.insert(callee)
            {
                let ty = self.value_type(callee)?;
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(format!("closure.once.{}", callee.0)),
                    m::Op::Alloca {
                        ty: (llvm_type(ty)?).clone(),
                        align: (alignment(ty, self.closures)?) as u32,
                    },
                )));
            }
            if let InstructionKind::ExtractIndex { aggregate, .. }
            | InstructionKind::UpdateIndex { aggregate, .. } = &instruction.kind
            {
                let result = instruction.result.expect("verified index result");
                let ty = &self.function.values[aggregate.0 as usize];
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(format!("index.slot.{}", result.0)),
                    m::Op::Alloca {
                        ty: (llvm_type(ty)?).clone(),
                        align: (alignment(ty, self.closures)?) as u32,
                    },
                )));
            }
        }
        for (index, local) in self.function.parameters.iter().enumerate() {
            let ty = self.local_type(*local)?;
            if *ty != Type::Unit {
                self.event(m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::Store {
                        arg: m::Arg::new(
                            (llvm_type(ty)?).clone(),
                            m::local(format!("arg.{}", index)),
                        ),
                        ptr: m::local(format!("local.{}", local.0)),
                        align: (alignment(ty, self.closures)?) as u32,
                    },
                )));
            }
        }
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Branch(format!("bb.{}", self.function.entry.0)),
        )));
        for block in &self.function.blocks {
            self.event(m::Event::Label(format!("bb.{}", block.id.0)));
            for instruction in &block.instructions {
                self.instruction(instruction)?;
            }
            self.terminator(&block.terminator)?;
        }
        // Even a statically unreachable source block cannot grant LLVM UB. All
        // explicit failure edges, including checked arithmetic, end in a trap.
        self.event(m::Event::Label("zil.trap".into()));
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Call {
                result: m::Ty::Void,
                result_attrs: vec![],
                callee: m::global("llvm.trap".into()),
                args: vec![],
                vararg: None,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Unreachable,
        )));
        self.event(m::Event::End);
        Ok(self.text)
    }

    fn instruction(&mut self, instruction: &Instruction) -> CompileResult<()> {
        match &instruction.kind {
            InstructionKind::OwnedStoreNew { .. }
            | InstructionKind::OwnedStoreReserve { .. }
            | InstructionKind::OwnedStorePush { .. }
            | InstructionKind::OwnedStorePop { .. }
            | InstructionKind::OwnedStoreLen { .. }
            | InstructionKind::OwnedStoreCapacity { .. }
            | InstructionKind::OwnedStoreBorrow { .. }
            | InstructionKind::OwnedStoreBorrowMut { .. } => {
                self.dynamic_store_instruction(instruction)?;
            }
            InstructionKind::ByteBufferNew
            | InstructionKind::ByteBufferReserve { .. }
            | InstructionKind::ByteBufferTruncate { .. }
            | InstructionKind::ByteBufferPush { .. }
            | InstructionKind::ByteBufferLen { .. }
            | InstructionKind::ByteBufferCapacity { .. }
            | InstructionKind::ByteBufferBorrow { .. } => {
                self.byte_buffer_instruction(instruction)?
            }
            InstructionKind::BorrowPackageField { local, index, .. } => {
                self.borrow_package_field(instruction, *local, *index)?;
            }
            InstructionKind::ReadPackageField { local, index } => {
                self.read_package_field(instruction, *local, *index)?
            }
            InstructionKind::ProjectOwnedArrayField { .. }
            | InstructionKind::TakeOwnedArrayElement { .. }
            | InstructionKind::DropOwnedArrayElement { .. }
            | InstructionKind::StoreOwnedArrayElement { .. }
            | InstructionKind::DropOwnedArray { .. }
            | InstructionKind::BorrowOwnedArrayElement { .. } => {
                self.owned_array_instruction(instruction)?;
            }
            InstructionKind::DescribePackageField { local, index } => {
                self.describe_package_field(instruction, *local, *index)?
            }
            InstructionKind::DropPackage { local } => self.drop_package(instruction, *local)?,
            InstructionKind::DropErrorUnion { local } => {
                self.drop_error_union(instruction, *local)?
            }
            InstructionKind::TakePackageField { local, index } => {
                self.take_package_field(instruction, *local, *index)?
            }
            InstructionKind::DropPackageField { local, index } => {
                self.drop_package_field(instruction, *local, *index)?
            }
            InstructionKind::StorePackageField {
                local,
                index,
                value,
            } => self.store_package_field(instruction, *local, *index, *value)?,
            InstructionKind::DescribeLocal { local } => {
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(format!("v.{}", result(instruction)?.0)),
                    m::Op::Gep {
                        ty: m::Ty::Int(8),
                        ptr: m::local(format!("local.{}", local.0)),
                        indices: vec![m::Arg::new(m::Ty::Int(64), m::Val::Int(0))],
                    },
                )));
            }
            InstructionKind::ReadDesc { desc, index } => {
                self.read_desc(instruction, *desc, *index)?
            }
            InstructionKind::WriteDesc { desc, index, value } => {
                self.write_desc(instruction, *desc, *index, *value)?
            }
            InstructionKind::DropManaged { local } => self.drop_managed(instruction, *local)?,
            InstructionKind::DropClosure { local } => self.drop_closure(instruction, *local)?,
            InstructionKind::BorrowCapture { index, .. } => {
                self.borrow_capture(instruction, *index)?
            }
            InstructionKind::DescribeCapture { index } => {
                self.describe_capture(instruction, *index)?
            }
            InstructionKind::TakeCapture { index } => self.take_capture(instruction, *index)?,
            InstructionKind::CallClosureBorrowed {
                callee, arguments, ..
            } => {
                self.call_closure_borrowed(instruction, *callee, arguments)?;
            }
            InstructionKind::CallClosureShared { callee, arguments } => {
                self.call_closure_shared(instruction, *callee, arguments)?
            }
            InstructionKind::CallClosureOnce { callee, arguments } => {
                self.call_closure_once(instruction, *callee, arguments)?
            }
            InstructionKind::BindPackageCapture { .. }
            | InstructionKind::EndPackageCapture { .. }
            | InstructionKind::UnsafeEnter { .. }
            | InstructionKind::UnsafeExit { .. }
            | InstructionKind::RegisterDefer { .. }
            | InstructionKind::ScopeEnter { .. }
            | InstructionKind::ScopeExit { .. } => {}
            InstructionKind::MakeErrorUnion { is_error, value } => {
                self.make_error_union(instruction, *is_error, *value)?;
            }
            InstructionKind::ErrorTag { value } => self.error_tag(instruction, *value)?,
            InstructionKind::ReadErrorUnion { value, is_error }
            | InstructionKind::TakeErrorUnion { value, is_error } => {
                self.read_error_union(instruction, *value, *is_error)?;
            }
            InstructionKind::ForeignCall {
                function,
                arguments,
                ..
            } => {
                self.foreign_call(instruction, *function, arguments)?;
            }
            InstructionKind::Constant(constant) => {
                let result = result(instruction)?;
                match constant {
                    Constant::Unit => {}
                    Constant::Bool(value) => self.event(m::Event::Instruction(m::Inst::new(
                        Some(format!("v.{}", result.0)),
                        m::Op::Binary {
                            opcode: "or".into(),
                            ty: m::Ty::Int(1),
                            left: m::Val::Bool(false),
                            right: m::val(*(value)),
                        },
                    ))),
                    Constant::I32(value) => self.event(m::Event::Instruction(m::Inst::new(
                        Some(format!("v.{}", result.0)),
                        m::Op::Binary {
                            opcode: "add".into(),
                            ty: m::Ty::Int(32),
                            left: m::Val::Int(0),
                            right: m::val(*(value)),
                        },
                    ))),
                    Constant::I64(value) => self.event(m::Event::Instruction(m::Inst::new(
                        Some(format!("v.{}", result.0)),
                        m::Op::Binary {
                            opcode: "add".into(),
                            ty: m::Ty::Int(64),
                            left: m::Val::Int(0),
                            right: m::val(*(value)),
                        },
                    ))),
                    Constant::U8(value) => self.event(m::Event::Instruction(m::Inst::new(
                        Some(format!("v.{}", result.0)),
                        m::Op::Binary {
                            opcode: "add".into(),
                            ty: m::Ty::Int(8),
                            left: m::Val::Int(0),
                            right: m::val(*(value)),
                        },
                    ))),
                    Constant::U16(value) => self.event(m::Event::Instruction(m::Inst::new(
                        Some(format!("v.{}", result.0)),
                        m::Op::Binary {
                            opcode: "add".into(),
                            ty: m::Ty::Int(16),
                            left: m::Val::Int(0),
                            right: m::val(*(value)),
                        },
                    ))),
                    Constant::NullHandle | Constant::NullWideString => {
                        self.event(m::Event::Instruction(m::Inst::new(
                            Some(format!("v.{}", result.0)),
                            m::Op::Freeze(m::Arg::new(m::Ty::Ptr, m::Val::Null)),
                        )))
                    }
                    Constant::WideString(_) => self.event(m::Event::Instruction(m::Inst::new(
                        Some(format!("v.{}", result.0)),
                        m::Op::Freeze(m::Arg::new(
                            m::Ty::Ptr,
                            m::global(format!("zil.wstr.{}.{}", self.function.id.0, result.0)),
                        )),
                    ))),
                }
            }
            InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                let result = result(instruction)?;
                let ty = self.local_type(*local)?;
                if *ty != Type::Unit {
                    self.event(m::Event::Instruction(m::Inst::new(
                        Some(format!("v.{}", result.0)),
                        m::Op::Load {
                            ty: (llvm_type(ty)?).clone(),
                            ptr: m::local(format!("local.{}", local.0)),
                            align: (alignment(ty, self.closures)?) as u32,
                        },
                    )));
                }
                if matches!(instruction.kind, InstructionKind::TakeLocal { .. })
                    && self.owns_storage(self.local_type(*local)?)
                {
                    self.event(m::Event::Instruction(m::Inst::new(
                        None,
                        m::Op::Store {
                            arg: m::Arg::new(m::Ty::Int(1), m::Val::Bool(false)),
                            ptr: m::local(format!("resource.live.{}", local.0)),
                            align: 1,
                        },
                    )));
                    self.package_flags(*local, false, false)?;
                    self.owned_array_flags(*local, false, false)?;
                }
            }
            InstructionKind::Store { local, value } => {
                let ty = self.local_type(*local)?;
                if *ty != Type::Unit {
                    self.event(m::Event::Instruction(m::Inst::new(
                        None,
                        m::Op::Store {
                            arg: m::Arg::new(
                                (llvm_type(ty)?).clone(),
                                m::local(format!("v.{}", value.0)),
                            ),
                            ptr: m::local(format!("local.{}", local.0)),
                            align: (alignment(ty, self.closures)?) as u32,
                        },
                    )));
                }
                if self.owns_storage(self.local_type(*local)?) {
                    self.event(m::Event::Instruction(m::Inst::new(
                        None,
                        m::Op::Store {
                            arg: m::Arg::new(m::Ty::Int(1), m::Val::Bool(true)),
                            ptr: m::local(format!("resource.live.{}", local.0)),
                            align: 1,
                        },
                    )));
                    self.package_flags(*local, false, true)?;
                    self.owned_array_flags(*local, false, true)?;
                }
            }
            InstructionKind::BorrowLocal { local } | InstructionKind::BorrowLocalMut { local } => {
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(format!("v.{}", result(instruction)?.0)),
                    m::Op::Gep {
                        ty: m::Ty::Int(8),
                        ptr: m::local(format!("local.{}", local.0)),
                        indices: vec![m::Arg::new(m::Ty::Int(64), m::Val::Int(0))],
                    },
                )));
            }
            InstructionKind::AddressField { address, index } => {
                let Type::Address { inner, .. } = self.value_type(*address)? else {
                    return invalid(
                        "E-CODEGEN-CFG",
                        "field projection requires an address",
                        Some(instruction.span),
                    );
                };
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(format!("v.{}", result(instruction)?.0)),
                    m::Op::Gep {
                        ty: (llvm_type(inner)?).clone(),
                        ptr: m::local(format!("v.{}", address.0)),
                        indices: vec![
                            m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                            m::Arg::new(m::Ty::Int(32), m::val(*(index))),
                        ],
                    },
                )));
            }
            InstructionKind::AddressIndex { address, index } => {
                if self.value_type(*address)?.is_slice_address() {
                    return self.slice_index(instruction, *address, *index);
                }
                let Type::Address { inner, .. } = self.value_type(*address)? else {
                    return invalid(
                        "E-CODEGEN-CFG",
                        "index projection requires an address",
                        Some(instruction.span),
                    );
                };
                let Type::Array { length, .. } = inner.as_ref() else {
                    return invalid(
                        "E-CODEGEN-CFG",
                        "index projection requires an array",
                        Some(instruction.span),
                    );
                };
                let length = *length;
                let array_type = llvm_type(inner)?;
                let index_value = self.widened_index(*index)?;
                let valid = self.temp();
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((valid).clone())),
                    m::Op::Compare {
                        predicate: "ult".into(),
                        ty: m::Ty::Int(64),
                        left: m::val((index_value).clone()),
                        right: m::val(length),
                    },
                )));
                let continuation = self.continuation;
                self.continuation += 1;
                self.event(m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::CondBranch {
                        condition: m::val((valid).clone()),
                        yes: format!("address.ok.{}", continuation),
                        no: "zil.trap".into(),
                    },
                )));
                self.event(m::Event::Label(format!("address.ok.{}", continuation)));
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(format!("v.{}", result(instruction)?.0)),
                    m::Op::Gep {
                        ty: (array_type).clone(),
                        ptr: m::local(format!("v.{}", address.0)),
                        indices: vec![
                            m::Arg::new(m::Ty::Int(64), m::Val::Int(0)),
                            m::Arg::new(m::Ty::Int(64), m::val((index_value).clone())),
                        ],
                    },
                )));
            }
            InstructionKind::ReborrowAddress { address, .. } => {
                if self.value_type(*address)?.is_slice_address() {
                    return self.reborrow_slice(instruction, *address);
                }
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(format!("v.{}", result(instruction)?.0)),
                    m::Op::Gep {
                        ty: m::Ty::Int(8),
                        ptr: m::local(format!("v.{}", address.0)),
                        indices: vec![m::Arg::new(m::Ty::Int(64), m::Val::Int(0))],
                    },
                )));
            }
            InstructionKind::AddressSlice {
                address,
                start,
                end,
            } => {
                self.slice_range(instruction, *address, *start, *end)?;
            }
            InstructionKind::SliceLength { address } => {
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(format!("v.{}", result(instruction)?.0)),
                    m::Op::Extract {
                        arg: m::Arg::new(
                            m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64)]),
                            m::local(format!("v.{}", address.0)),
                        ),
                        index: 1,
                    },
                )));
            }
            InstructionKind::WriteAddress { address, value } => {
                let Type::Address { inner, .. } = self.value_type(*address)? else {
                    return invalid(
                        "E-CODEGEN-CFG",
                        "write requires address",
                        Some(instruction.span),
                    );
                };
                if **inner != Type::Unit {
                    self.event(m::Event::Instruction(m::Inst::new(
                        None,
                        m::Op::Store {
                            arg: m::Arg::new(
                                (llvm_type(inner)?).clone(),
                                m::local(format!("v.{}", value.0)),
                            ),
                            ptr: m::local(format!("v.{}", address.0)),
                            align: (alignment(inner, self.closures)?) as u32,
                        },
                    )));
                }
            }
            InstructionKind::ReadAddress { address } => {
                let Type::Address { inner, .. } = self.value_type(*address)? else {
                    return invalid(
                        "E-CODEGEN-CFG",
                        "ReadAddress requires an address",
                        Some(instruction.span),
                    );
                };
                if **inner != Type::Unit {
                    self.event(m::Event::Instruction(m::Inst::new(
                        Some(format!("v.{}", result(instruction)?.0)),
                        m::Op::Load {
                            ty: (llvm_type(inner)?).clone(),
                            ptr: m::local(format!("v.{}", address.0)),
                            align: (alignment(inner, self.closures)?) as u32,
                        },
                    )));
                }
            }
            InstructionKind::StorageLive(local) | InstructionKind::StorageDead(local) => {
                if self.owns_storage(self.local_type(*local)?) {
                    self.event(m::Event::Instruction(m::Inst::new(
                        None,
                        m::Op::Store {
                            arg: m::Arg::new(m::Ty::Int(1), m::Val::Bool(false)),
                            ptr: m::local(format!("resource.live.{}", local.0)),
                            align: 1,
                        },
                    )));
                    self.package_flags(*local, false, false)?;
                    self.owned_array_flags(*local, false, false)?;
                }
                // S0 keeps the real stack allocation until return. Lexical
                // validity was independently proved; no runtime table needed.
            }
            InstructionKind::ConvertInteger { value, target } => {
                self.convert_integer(*value, target, result(instruction)?)?;
            }
            InstructionKind::Unary { op, operand } => {
                let result = result(instruction)?;
                let ty = self.value_type(*operand)?.clone();
                match op {
                    UnaryOp::Negate => {
                        self.checked_binary("ssub", &ty, "0", &value(*operand), result)?
                    }
                    UnaryOp::Not => self.event(m::Event::Instruction(m::Inst::new(
                        Some(format!("v.{}", result.0)),
                        m::Op::Binary {
                            opcode: "xor".into(),
                            ty: m::Ty::Int(1),
                            left: m::local(format!("v.{}", operand.0)),
                            right: m::Val::Bool(true),
                        },
                    ))),
                    UnaryOp::BitNot => self.event(m::Event::Instruction(m::Inst::new(
                        Some(format!("v.{}", result.0)),
                        m::Op::Binary {
                            opcode: "xor".into(),
                            ty: (llvm_type(&ty)?).clone(),
                            left: m::local(format!("v.{}", operand.0)),
                            right: m::Val::Int(-1),
                        },
                    ))),
                    _ => {
                        return unsupported(
                            format!("LLVM unary operation {op:?}"),
                            Some(instruction.span),
                        );
                    }
                }
            }
            InstructionKind::Binary { op, left, right } => {
                self.binary(*op, *left, *right, result(instruction)?, instruction.span)?;
            }
            InstructionKind::Call {
                function,
                arguments,
            } => {
                let callee = self.functions.get(function).ok_or_else(|| {
                    vec![Diagnostic::error(
                        Phase::Codegen,
                        "E-CODEGEN-CFG",
                        Some(instruction.span),
                        "call target is absent",
                    )]
                })?;
                if callee.environment.is_some() {
                    return invalid(
                        "E-CODEGEN-CFG",
                        "an ordinary call cannot enter a closure environment function",
                        Some(instruction.span),
                    );
                }
                // Reuse a fact about this exact bound source before constructing
                // a machine call that a later pass would have to rediscover.
                // The bounded evaluator proves a returning, effect-free scalar
                // computation. Other instructions and their order are untouched.
                if arguments.is_empty()
                    && callee.parameters.is_empty()
                    && callee.foreign.is_none()
                    && callee.return_type != Type::Unit
                    && let Some(value) = self.scalar_summaries.and_then(|plan| plan.get(function))
                    && value.ty() == callee.return_type
                {
                    self.event(m::Event::Instruction(m::Inst::new(
                        Some(format!("v.{}", result(instruction)?.0)),
                        m::Op::Freeze(scalar_summaries::argument(value)),
                    )));
                    return Ok(());
                }
                let mut emitted_arguments = Vec::new();
                for argument in arguments {
                    let ty = self.value_type(*argument)?;
                    if *ty != Type::Unit {
                        emitted_arguments.push(m::Arg::new(
                            llvm_type(ty)?,
                            m::local(format!("v.{}", argument.0)),
                        ));
                    }
                }
                let assignment = if callee.return_type == Type::Unit {
                    None
                } else {
                    Some(format!("v.{}", result(instruction)?.0))
                };
                self.event(m::Event::Instruction(m::Inst::new(
                    assignment,
                    m::Op::Call {
                        result: llvm_type(&callee.return_type)?,
                        result_attrs: vec![],
                        callee: m::global(symbol(*function)),
                        args: emitted_arguments,
                        vararg: None,
                    },
                )));
            }
            InstructionKind::VariantTagLocal { local, path } => {
                self.variant_tag_local(instruction, *local, path)?
            }
            InstructionKind::ReadVariantLocal { local, path } => {
                self.read_variant_local(instruction, *local, path)?
            }
            InstructionKind::UnpackVariant {
                local,
                variant,
                fields,
            } => self.unpack_variant(instruction, *local, *variant, fields)?,
            InstructionKind::DropEnum { local } => self.drop_enum(instruction, *local)?,
            InstructionKind::MakeVariant { variant, fields } => {
                self.make_variant(instruction, *variant, fields)?
            }
            InstructionKind::VariantTag { value } => self.variant_tag(instruction, *value)?,
            InstructionKind::ReadVariant {
                value,
                variant,
                index,
            } => self.read_variant(instruction, *value, *variant, *index)?,
            InstructionKind::FunctionRef { function }
            | InstructionKind::ForeignFunctionRef { function } => {
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(format!("v.{}", result(instruction)?.0)),
                    m::Op::Freeze(m::Arg::new(
                        m::Ty::Ptr,
                        m::global(symbol(*function).to_string()),
                    )),
                )));
            }
            InstructionKind::CallIndirect { callee, arguments } => {
                let Type::FunctionPointer {
                    result: return_type,
                    ..
                } = self.value_type(*callee)?
                else {
                    return invalid(
                        "E-CODEGEN-CFG",
                        "indirect call requires a function pointer",
                        Some(instruction.span),
                    );
                };
                let mut emitted_arguments = Vec::new();
                for argument in arguments {
                    let ty = self.value_type(*argument)?;
                    if *ty != Type::Unit {
                        emitted_arguments.push(m::Arg::new(
                            llvm_type(ty)?,
                            m::local(format!("v.{}", argument.0)),
                        ));
                    }
                }
                let assignment = if **return_type == Type::Unit {
                    None
                } else {
                    Some(format!("v.{}", result(instruction)?.0))
                };
                self.event(m::Event::Instruction(m::Inst::new(
                    assignment,
                    m::Op::Call {
                        result: llvm_type(return_type)?,
                        result_attrs: vec![],
                        callee: m::local(format!("v.{}", callee.0)),
                        args: emitted_arguments,
                        vararg: None,
                    },
                )));
            }
            InstructionKind::ExtractIndex { aggregate, index } => {
                self.indexed(instruction, *aggregate, *index, None)?
            }
            InstructionKind::UpdateIndex {
                aggregate,
                index,
                value,
            } => self.indexed(instruction, *aggregate, *index, Some(*value))?,
            InstructionKind::MakeAggregate { fields }
            | InstructionKind::MakeManaged { fields }
            | InstructionKind::MakeOwnedArray { elements: fields }
            | InstructionKind::MakePackage { fields } => {
                let result = instruction.result.expect("verified aggregate result");
                let ty = &self.function.values[result.0 as usize];
                let llvm = llvm_type(ty)?;
                let mut previous = "zeroinitializer".to_owned();
                if fields.is_empty() {
                    self.event(m::Event::Instruction(m::Inst::new(
                        Some(m::name((value(result)).clone())),
                        m::Op::Freeze(m::Arg::new((llvm).clone(), m::Val::Zero)),
                    )));
                }
                for (index, field) in fields.iter().enumerate() {
                    let ft = &self.function.values[field.0 as usize];
                    let target = if index + 1 == fields.len() {
                        value(result)
                    } else {
                        self.temp()
                    };
                    let fv = if *ft == Type::Unit {
                        "zeroinitializer".into()
                    } else {
                        value(*field)
                    };
                    self.event(m::Event::Instruction(m::Inst::new(
                        Some(m::name((target).clone())),
                        m::Op::Insert {
                            arg: m::Arg::new((llvm).clone(), m::val((previous).clone())),
                            index: (index) as u32,
                            field: m::Arg::new(
                                (aggregate_field_type(ft)?).clone(),
                                m::val((fv).clone()),
                            ),
                        },
                    )));
                    previous = target;
                }
            }
            InstructionKind::ExtractField { aggregate, index } => {
                let result = instruction.result.expect("verified field result");
                if self.function.values[result.0 as usize] != Type::Unit {
                    self.event(m::Event::Instruction(m::Inst::new(
                        Some(m::name((value(result)).clone())),
                        m::Op::Extract {
                            arg: m::Arg::new(
                                (llvm_type(&self.function.values[aggregate.0 as usize])?).clone(),
                                m::val((value(*aggregate)).clone()),
                            ),
                            index: *index,
                        },
                    )));
                }
            }
            InstructionKind::UpdateField {
                aggregate,
                index,
                value: field,
            } => {
                let result = instruction.result.expect("verified update result");
                let ft = &self.function.values[field.0 as usize];
                let fv = if *ft == Type::Unit {
                    "zeroinitializer".into()
                } else {
                    value(*field)
                };
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((value(result)).clone())),
                    m::Op::Insert {
                        arg: m::Arg::new(
                            (llvm_type(&self.function.values[aggregate.0 as usize])?).clone(),
                            m::val((value(*aggregate)).clone()),
                        ),
                        index: *index,
                        field: m::Arg::new(
                            (aggregate_field_type(ft)?).clone(),
                            m::val((fv).clone()),
                        ),
                    },
                )));
            }
            InstructionKind::MakeClosure { closure, captures } => {
                let output = result(instruction)?;
                let environment = environment_type(*closure);
                let definition = self.closure_definition(*closure)?;
                let mut fields = Vec::new();
                for (index, capture) in captures.iter().enumerate() {
                    let ty = self.value_type(*capture)?;
                    fields.push((
                        index,
                        capture_type(ty)?,
                        if *ty == Type::Unit {
                            "0".into()
                        } else {
                            value(*capture)
                        },
                    ));
                }
                let owned_count = definition
                    .captures
                    .iter()
                    .map(resource_closures::flag_count)
                    .sum::<usize>();
                for offset in 0..owned_count {
                    fields.push((captures.len() + offset, "i1".into(), "true".into()));
                }
                if fields.is_empty() {
                    self.event(m::Event::Instruction(m::Inst::new(
                        Some(m::name((value(output)).clone())),
                        m::Op::Freeze(m::Arg::new((environment).clone(), m::Val::Zero)),
                    )));
                } else {
                    let mut aggregate = String::from("zeroinitializer");
                    let count = fields.len();
                    for (position, (index, field, captured)) in fields.into_iter().enumerate() {
                        let next = if position + 1 == count {
                            value(output)
                        } else {
                            self.temp()
                        };
                        self.event(m::Event::Instruction(m::Inst::new(
                            Some(m::name((next).clone())),
                            m::Op::Insert {
                                arg: m::Arg::new(
                                    (environment).clone(),
                                    m::val((aggregate).clone()),
                                ),
                                index: (index) as u32,
                                field: m::Arg::new((field).clone(), m::val((captured).clone())),
                            },
                        )));
                        aggregate = next;
                    }
                }
            }
            InstructionKind::ReadCapture { index } => {
                let environment = self
                    .function
                    .environment
                    .expect("verified capture environment");
                let definition = self.closure_definition(environment)?;
                let ty = definition.captures[*index as usize].clone();
                let mutable = resource_closures::by_pointer(definition);
                if ty != Type::Unit {
                    if mutable {
                        let address = self.temp();
                        self.event(m::Event::Instruction(m::Inst::new(
                            Some(m::name((address).clone())),
                            m::Op::Gep {
                                ty: (environment_type(environment)).clone(),
                                ptr: m::local("env".into()),
                                indices: vec![
                                    m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                                    m::Arg::new(m::Ty::Int(32), m::val(*(index))),
                                ],
                            },
                        )));
                        self.event(m::Event::Instruction(m::Inst::new(
                            Some(format!("v.{}", result(instruction)?.0)),
                            m::Op::Load {
                                ty: (llvm_type(&ty)?).clone(),
                                ptr: m::val((address).clone()),
                                align: (alignment(&ty, self.closures)?) as u32,
                            },
                        )));
                    } else {
                        self.event(m::Event::Instruction(m::Inst::new(
                            Some(format!("v.{}", result(instruction)?.0)),
                            m::Op::Extract {
                                arg: m::Arg::new(
                                    (environment_type(environment)).clone(),
                                    m::local("env".into()),
                                ),
                                index: *index,
                            },
                        )));
                    }
                }
            }
            InstructionKind::WriteCapture {
                index,
                value: written,
            } => {
                let environment = self
                    .function
                    .environment
                    .expect("verified capture environment");
                let ty = self.closure_definition(environment)?.captures[*index as usize].clone();
                if ty != Type::Unit {
                    let address = self.temp();
                    self.event(m::Event::Instruction(m::Inst::new(
                        Some(m::name((address).clone())),
                        m::Op::Gep {
                            ty: (environment_type(environment)).clone(),
                            ptr: m::local("env".into()),
                            indices: vec![
                                m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                                m::Arg::new(m::Ty::Int(32), m::val(*(index))),
                            ],
                        },
                    )));
                    self.event(m::Event::Instruction(m::Inst::new(
                        None,
                        m::Op::Store {
                            arg: m::Arg::new(
                                (llvm_type(&ty)?).clone(),
                                m::val((value(*written)).clone()),
                            ),
                            ptr: m::val((address).clone()),
                            align: (alignment(&ty, self.closures)?) as u32,
                        },
                    )));
                }
            }
            InstructionKind::CallClosureMut { callee, arguments } => {
                // Requested Exclusive may adapt a concrete Shared entry. Use its
                // actual ABI; Verify already checked the mutable location.
                self.call_closure_shared(instruction, *callee, arguments)?;
            }
            InstructionKind::CallClosure { callee, arguments } => {
                let Type::Closure(environment) = self.value_type(*callee)? else {
                    return invalid(
                        "E-CODEGEN-CFG",
                        "closure call has no concrete environment type",
                        Some(instruction.span),
                    );
                };
                let definition = self.closure_definition(*environment)?;
                let function = definition.function;
                let return_type = llvm_type(&definition.result)?;
                let assignment = if definition.result == Type::Unit {
                    None
                } else {
                    Some(format!("v.{}", result(instruction)?.0))
                };
                let mut emitted_arguments = vec![m::Arg::new(
                    environment_type(*environment),
                    m::val(value(*callee)),
                )];
                for argument in arguments {
                    let ty = self.value_type(*argument)?;
                    if *ty != Type::Unit {
                        emitted_arguments
                            .push(m::Arg::new(llvm_type(ty)?, m::val(value(*argument))));
                    }
                }
                self.event(m::Event::Instruction(m::Inst::new(
                    assignment,
                    m::Op::Call {
                        result: return_type,
                        result_attrs: vec![],
                        callee: m::global(symbol(function)),
                        args: emitted_arguments,
                        vararg: None,
                    },
                )));
            }
            InstructionKind::Unsupported(feature) => {
                return unsupported(feature.clone(), Some(instruction.span));
            }
        }
        Ok(())
    }

    fn binary(
        &mut self,
        operation: BinaryOp,
        left: ValueId,
        right: ValueId,
        result: ValueId,
        span: Span,
    ) -> CompileResult<()> {
        let ty = self.value_type(left)?.clone();
        let operand_type = llvm_type(&ty)?;
        let (left_text, right_text) = (value(left), value(right));
        match operation {
            BinaryOp::Add => self.checked_binary("sadd", &ty, &left_text, &right_text, result)?,
            BinaryOp::Subtract => {
                self.checked_binary("ssub", &ty, &left_text, &right_text, result)?
            }
            BinaryOp::Multiply => {
                self.checked_binary("smul", &ty, &left_text, &right_text, result)?
            }
            BinaryOp::Divide | BinaryOp::Remainder => {
                if !self
                    .safe_divisors
                    .as_ref()
                    .is_some_and(|safe| safe.contains(&right))
                {
                    let is_zero = self.temp();
                    self.event(m::Event::Instruction(m::Inst::new(
                        Some(m::name((is_zero).clone())),
                        m::Op::Compare {
                            predicate: "eq".into(),
                            ty: (operand_type).clone(),
                            left: m::val((right_text).clone()),
                            right: m::Val::Int(0),
                        },
                    )));
                    if matches!(ty, Type::U8 | Type::U16) {
                        self.trap_if(&is_zero);
                    } else {
                        let minimum = match ty {
                            Type::I32 => i128::from(i32::MIN),
                            Type::I64 => i128::from(i64::MIN),
                            _ => return unsupported("non-integer division", Some(span)),
                        };
                        let is_minimum = self.temp();
                        let is_minus_one = self.temp();
                        let is_overflow = self.temp();
                        let is_invalid = self.temp();
                        self.event(m::Event::Instruction(m::Inst::new(
                            Some(m::name((is_minimum).clone())),
                            m::Op::Compare {
                                predicate: "eq".into(),
                                ty: (operand_type).clone(),
                                left: m::val((left_text).clone()),
                                right: m::val(minimum),
                            },
                        )));
                        self.event(m::Event::Instruction(m::Inst::new(
                            Some(m::name((is_minus_one).clone())),
                            m::Op::Compare {
                                predicate: "eq".into(),
                                ty: (operand_type).clone(),
                                left: m::val((right_text).clone()),
                                right: m::Val::Int(-1),
                            },
                        )));
                        self.event(m::Event::Instruction(m::Inst::new(
                            Some(m::name((is_overflow).clone())),
                            m::Op::Binary {
                                opcode: "and".into(),
                                ty: m::Ty::Int(1),
                                left: m::val((is_minimum).clone()),
                                right: m::val((is_minus_one).clone()),
                            },
                        )));
                        self.event(m::Event::Instruction(m::Inst::new(
                            Some(m::name((is_invalid).clone())),
                            m::Op::Binary {
                                opcode: "or".into(),
                                ty: m::Ty::Int(1),
                                left: m::val((is_zero).clone()),
                                right: m::val((is_overflow).clone()),
                            },
                        )));
                        self.trap_if(&is_invalid);
                    }
                }
                let op = match (
                    matches!(ty, Type::U8 | Type::U16),
                    operation == BinaryOp::Divide,
                ) {
                    (true, true) => "udiv",
                    (true, false) => "urem",
                    (false, true) => "sdiv",
                    (false, false) => "srem",
                };
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(format!("v.{}", result.0)),
                    m::Op::Binary {
                        opcode: op.to_string(),
                        ty: (operand_type).clone(),
                        left: m::val((left_text).clone()),
                        right: m::val((right_text).clone()),
                    },
                )));
            }
            BinaryOp::AddWrapping | BinaryOp::SubtractWrapping | BinaryOp::MultiplyWrapping => {
                let op = match operation {
                    BinaryOp::AddWrapping => "add",
                    BinaryOp::SubtractWrapping => "sub",
                    _ => "mul",
                };
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(format!("v.{}", result.0)),
                    m::Op::Binary {
                        opcode: op.to_string(),
                        ty: (operand_type).clone(),
                        left: m::val((left_text).clone()),
                        right: m::val((right_text).clone()),
                    },
                )));
            }
            BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual => {
                if ty == Type::Unit {
                    match operation {
                        BinaryOp::Equal => self.event(m::Event::Instruction(m::Inst::new(
                            Some(format!("v.{}", result.0)),
                            m::Op::Binary {
                                opcode: "or".into(),
                                ty: m::Ty::Int(1),
                                left: m::Val::Bool(false),
                                right: m::Val::Bool(true),
                            },
                        ))),
                        BinaryOp::NotEqual => self.event(m::Event::Instruction(m::Inst::new(
                            Some(format!("v.{}", result.0)),
                            m::Op::Binary {
                                opcode: "or".into(),
                                ty: m::Ty::Int(1),
                                left: m::Val::Bool(false),
                                right: m::Val::Bool(false),
                            },
                        ))),
                        _ => return unsupported("ordered comparison of unit", Some(span)),
                    }
                } else {
                    let predicate = match operation {
                        BinaryOp::Equal => "eq",
                        BinaryOp::NotEqual => "ne",
                        BinaryOp::Less if matches!(ty, Type::U8 | Type::U16) => "ult",
                        BinaryOp::LessEqual if matches!(ty, Type::U8 | Type::U16) => "ule",
                        BinaryOp::Greater if matches!(ty, Type::U8 | Type::U16) => "ugt",
                        BinaryOp::GreaterEqual if matches!(ty, Type::U8 | Type::U16) => "uge",
                        BinaryOp::Less => "slt",
                        BinaryOp::LessEqual => "sle",
                        BinaryOp::Greater => "sgt",
                        _ => "sge",
                    };
                    self.event(m::Event::Instruction(m::Inst::new(
                        Some(format!("v.{}", result.0)),
                        m::Op::Compare {
                            predicate: predicate.to_string(),
                            ty: (operand_type).clone(),
                            left: m::val((left_text).clone()),
                            right: m::val((right_text).clone()),
                        },
                    )));
                }
            }
            BinaryOp::And
            | BinaryOp::Or
            | BinaryOp::BitAnd
            | BinaryOp::BitOr
            | BinaryOp::BitXor => {
                let op = match operation {
                    BinaryOp::And | BinaryOp::BitAnd => "and",
                    BinaryOp::Or | BinaryOp::BitOr => "or",
                    _ => "xor",
                };
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(format!("v.{}", result.0)),
                    m::Op::Binary {
                        opcode: op.to_string(),
                        ty: (operand_type).clone(),
                        left: m::val((left_text).clone()),
                        right: m::val((right_text).clone()),
                    },
                )));
            }
            BinaryOp::ShiftLeft | BinaryOp::ShiftRight => {
                let bits = match ty {
                    Type::U8 => 8,
                    Type::U16 => 16,
                    Type::I32 => 32,
                    Type::I64 => 64,
                    _ => return unsupported("LLVM non-integer shift lowering", Some(span)),
                };
                let invalid = self.temp();
                // Unsigned comparison also rejects every negative signed count.
                // Check before the shift, whose out-of-range LLVM result is poison.
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((invalid).clone())),
                    m::Op::Compare {
                        predicate: "uge".into(),
                        ty: (operand_type).clone(),
                        left: m::val((right_text).clone()),
                        right: m::val(bits),
                    },
                )));
                self.trap_if(&invalid);
                let op = if operation == BinaryOp::ShiftLeft {
                    "shl"
                } else if matches!(ty, Type::I32 | Type::I64) {
                    "ashr"
                } else {
                    "lshr"
                };
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(format!("v.{}", result.0)),
                    m::Op::Binary {
                        opcode: op.to_string(),
                        ty: (operand_type).clone(),
                        left: m::val((left_text).clone()),
                        right: m::val((right_text).clone()),
                    },
                )));
            }
        }
        Ok(())
    }

    fn widened_index(&mut self, index: ValueId) -> CompileResult<String> {
        match self.value_type(index)? {
            Type::U8 | Type::U16 => {
                let from = llvm_type(self.value_type(index)?)?;
                let extended = self.temp();
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((extended).clone())),
                    m::Op::Cast {
                        opcode: "zext".into(),
                        arg: m::Arg::new((from).clone(), m::val((value(index)).clone())),
                        to: m::Ty::Int(64),
                    },
                )));
                Ok(extended)
            }
            Type::I32 => {
                let extended = self.temp();
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((extended).clone())),
                    m::Op::Cast {
                        opcode: "sext".into(),
                        arg: m::Arg::new(m::Ty::Int(32), m::val((value(index)).clone())),
                        to: m::Ty::Int(64),
                    },
                )));
                Ok(extended)
            }
            Type::I64 => Ok(value(index)),
            _ => unsupported(
                "array index must have a verified integer type",
                Some(self.function.span),
            ),
        }
    }

    fn convert_integer(
        &mut self,
        input: ValueId,
        target: &Type,
        result: ValueId,
    ) -> CompileResult<()> {
        let source = self.value_type(input)?.clone();
        let from = llvm_type(&source)?;
        let to = llvm_type(target)?;
        let input = value(input);
        let operation = match (&source, target) {
            (a, b) if a == b && a.is_integer() => {
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(format!("v.{}", result.0)),
                    m::Op::Binary {
                        opcode: "add".into(),
                        ty: (to).clone(),
                        left: m::val((input).clone()),
                        right: m::Val::Int(0),
                    },
                )));
                return Ok(());
            }
            (Type::U8, Type::U16 | Type::I32 | Type::I64) | (Type::U16, Type::I32 | Type::I64) => {
                "zext"
            }
            (Type::I32, Type::I64) => "sext",
            (Type::I32 | Type::I64 | Type::U16, Type::U8) | (Type::I32 | Type::I64, Type::U16) => {
                // Unsigned comparison also rejects every negative signed input.
                let maximum = if *target == Type::U8 { 255 } else { 65535 };
                let invalid = self.temp();
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((invalid).clone())),
                    m::Op::Compare {
                        predicate: "ugt".into(),
                        ty: (from).clone(),
                        left: m::val((input).clone()),
                        right: m::val(maximum),
                    },
                )));
                self.trap_if(&invalid);
                "trunc"
            }
            (Type::I64, Type::I32) => {
                let below = self.temp();
                let above = self.temp();
                let invalid = self.temp();
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((below).clone())),
                    m::Op::Compare {
                        predicate: "slt".into(),
                        ty: m::Ty::Int(64),
                        left: m::val((input).clone()),
                        right: m::val(i32::MIN),
                    },
                )));
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((above).clone())),
                    m::Op::Compare {
                        predicate: "sgt".into(),
                        ty: m::Ty::Int(64),
                        left: m::val((input).clone()),
                        right: m::val(i32::MAX),
                    },
                )));
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((invalid).clone())),
                    m::Op::Binary {
                        opcode: "or".into(),
                        ty: m::Ty::Int(1),
                        left: m::val((below).clone()),
                        right: m::val((above).clone()),
                    },
                )));
                self.trap_if(&invalid);
                "trunc"
            }
            _ => return unsupported("verified integer conversion type", Some(self.function.span)),
        };
        self.event(m::Event::Instruction(m::Inst::new(
            Some(format!("v.{}", result.0)),
            m::Op::Cast {
                opcode: operation.to_string(),
                arg: m::Arg::new((from).clone(), m::val((input).clone())),
                to: (to).clone(),
            },
        )));
        Ok(())
    }

    fn checked_binary(
        &mut self,
        operation: &'static str,
        ty: &Type,
        left: &str,
        right: &str,
        result: ValueId,
    ) -> CompileResult<()> {
        let operation = if matches!(ty, Type::U8 | Type::U16) {
            match operation {
                "sadd" => "uadd",
                "ssub" => "usub",
                "smul" => "umul",
                other => other,
            }
        } else {
            operation
        };
        let bits = match ty {
            Type::U8 => 8,
            Type::U16 => 16,
            Type::I32 => 32,
            Type::I64 => 64,
            _ => return unsupported("checked non-integer arithmetic", Some(self.function.span)),
        };
        self.intrinsics.insert((operation, bits));
        let pair = self.temp();
        let overflow = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((pair).clone())),
            m::Op::Call {
                result: m::Ty::Struct(vec![m::Ty::Int((bits) as u32), m::Ty::Int(1)]),
                result_attrs: vec![],
                callee: m::global(format!("llvm.{}.with.overflow.i{}", operation, bits)),
                args: vec![
                    m::Arg::new(m::Ty::Int((bits) as u32), m::val(left)),
                    m::Arg::new(m::Ty::Int((bits) as u32), m::val(right)),
                ],
                vararg: None,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some(format!("v.{}", result.0)),
            m::Op::Extract {
                arg: m::Arg::new(
                    m::Ty::Struct(vec![m::Ty::Int((bits) as u32), m::Ty::Int(1)]),
                    m::val((pair).clone()),
                ),
                index: 0,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((overflow).clone())),
            m::Op::Extract {
                arg: m::Arg::new(
                    m::Ty::Struct(vec![m::Ty::Int((bits) as u32), m::Ty::Int(1)]),
                    m::val((pair).clone()),
                ),
                index: 1,
            },
        )));
        self.trap_if(&overflow);
        Ok(())
    }

    fn trap_if(&mut self, condition: &str) {
        let continuation = self.continuation;
        self.continuation += 1;
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::CondBranch {
                condition: m::val(condition),
                yes: "zil.trap".into(),
                no: format!("checked.{}", continuation),
            },
        )));
        self.event(m::Event::Label(format!("checked.{}", continuation)));
    }

    fn terminator(&mut self, terminator: &Terminator) -> CompileResult<()> {
        match terminator {
            Terminator::Return(value) => {
                if self.function.return_type == Type::Unit {
                    self.event(m::Event::Instruction(m::Inst::new(
                        None,
                        m::Op::Return(None),
                    )));
                } else {
                    let returned = value.ok_or_else(|| {
                        vec![Diagnostic::error(
                            Phase::Codegen,
                            "E-CODEGEN-CFG",
                            Some(self.function.span),
                            "a non-unit return has no value",
                        )]
                    })?;
                    self.event(m::Event::Instruction(m::Inst::new(
                        None,
                        m::Op::Return(Some(m::Arg::new(
                            (llvm_type(&self.function.return_type)?).clone(),
                            m::local(format!("v.{}", returned.0)),
                        ))),
                    )));
                }
            }
            Terminator::Jump(block) => self.event(m::Event::Instruction(m::Inst::new(
                None,
                m::Op::Branch(format!("bb.{}", block.0)),
            ))),
            Terminator::Branch {
                condition,
                then_block,
                else_block,
            } => self.event(m::Event::Instruction(m::Inst::new(
                None,
                m::Op::CondBranch {
                    condition: m::local(format!("v.{}", condition.0)),
                    yes: format!("bb.{}", then_block.0),
                    no: format!("bb.{}", else_block.0),
                },
            ))),
            Terminator::Unreachable | Terminator::Trap => self.event(m::Event::Instruction(
                m::Inst::new(None, m::Op::Branch("zil.trap".into())),
            )),
        }
        Ok(())
    }

    fn local_type(&self, id: LocalId) -> CompileResult<&Type> {
        self.function
            .locals
            .get(id.0 as usize)
            .filter(|local| local.id == id)
            .map(|local| &local.ty)
            .ok_or_else(|| {
                vec![Diagnostic::error(
                    Phase::Codegen,
                    "E-CODEGEN-CFG",
                    Some(self.function.span),
                    "invalid local identity",
                )]
            })
    }
    fn value_type(&self, id: ValueId) -> CompileResult<&Type> {
        self.function.values.get(id.0 as usize).ok_or_else(|| {
            vec![Diagnostic::error(
                Phase::Codegen,
                "E-CODEGEN-CFG",
                Some(self.function.span),
                "invalid value identity",
            )]
        })
    }
    fn closure_definition(&self, id: ClosureId) -> CompileResult<&ClosureDefinition> {
        self.closures.get(&id).copied().ok_or_else(|| {
            vec![Diagnostic::error(
                Phase::Codegen,
                "E-CODEGEN-CFG",
                Some(self.function.span),
                "concrete closure definition is absent",
            )]
        })
    }
    fn indexed(
        &mut self,
        instruction: &Instruction,
        aggregate: ValueId,
        index: ValueId,
        update: Option<ValueId>,
    ) -> CompileResult<()> {
        let result = instruction.result.expect("verified array operation result");
        let ty = &self.function.values[aggregate.0 as usize];
        let Type::Array { element, length } = ty else {
            return unsupported("verified index lost array type", Some(instruction.span));
        };
        let element = (**element).clone();
        let length = *length;
        let llvm = llvm_type(ty)?;
        let align = alignment(ty, self.closures)?;
        let index_value = self.widened_index(index)?;
        let valid = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((valid).clone())),
            m::Op::Compare {
                predicate: "ult".into(),
                ty: m::Ty::Int(64),
                left: m::val((index_value).clone()),
                right: m::val(length),
            },
        )));
        let continuation = self.continuation;
        self.continuation += 1;
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::CondBranch {
                condition: m::val((valid).clone()),
                yes: format!("index.ok.{}", continuation),
                no: "zil.trap".into(),
            },
        )));
        self.event(m::Event::Label(format!("index.ok.{}", continuation)));
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Store {
                arg: m::Arg::new((llvm).clone(), m::val((value(aggregate)).clone())),
                ptr: m::local(format!("index.slot.{}", result.0)),
                align: (align) as u32,
            },
        )));
        let address = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((address).clone())),
            m::Op::Gep {
                ty: (llvm).clone(),
                ptr: m::local(format!("index.slot.{}", result.0)),
                indices: vec![
                    m::Arg::new(m::Ty::Int(64), m::Val::Int(0)),
                    m::Arg::new(m::Ty::Int(64), m::val((index_value).clone())),
                ],
            },
        )));
        if let Some(update) = update {
            let v = if element == Type::Unit {
                "zeroinitializer".into()
            } else {
                value(update)
            };
            self.event(m::Event::Instruction(m::Inst::new(
                None,
                m::Op::Store {
                    arg: m::Arg::new(
                        (aggregate_field_type(&element)?).clone(),
                        m::val((v).clone()),
                    ),
                    ptr: m::val((address).clone()),
                    align: (alignment(&element, self.closures)?) as u32,
                },
            )));
            self.event(m::Event::Instruction(m::Inst::new(
                Some(m::name((value(result)).clone())),
                m::Op::Load {
                    ty: (llvm).clone(),
                    ptr: m::local(format!("index.slot.{}", result.0)),
                    align: (align) as u32,
                },
            )));
        } else if element != Type::Unit {
            self.event(m::Event::Instruction(m::Inst::new(
                Some(m::name((value(result)).clone())),
                m::Op::Load {
                    ty: (llvm_type(&element)?).clone(),
                    ptr: m::val((address).clone()),
                    align: (alignment(&element, self.closures)?) as u32,
                },
            )));
        }
        Ok(())
    }
    fn temp(&mut self) -> String {
        let id = self.temporary;
        self.temporary += 1;
        format!("%tmp.{id}")
    }
    fn event(&mut self, event: m::Event) {
        self.text.push(event);
    }

    fn line(&mut self, code: m::Code) {
        self.text.append(code);
    }
}

fn result(instruction: &Instruction) -> CompileResult<ValueId> {
    instruction.result.ok_or_else(|| {
        vec![Diagnostic::error(
            Phase::Codegen,
            "E-CODEGEN-CFG",
            Some(instruction.span),
            "instruction result is absent",
        )]
    })
}
fn symbol(id: FunctionId) -> String {
    format!("zil.fn.{}", id.0)
}
fn value(id: ValueId) -> String {
    format!("%v.{}", id.0)
}
fn environment_type(id: ClosureId) -> m::Ty {
    m::Ty::Named(format!("zil_env_{}", id.0))
}
fn llvm_type(ty: &Type) -> CompileResult<m::Ty> {
    match ty {
        Type::Unit => Ok("void".into()),
        Type::Bool => Ok("i1".into()),
        Type::I32 => Ok("i32".into()),
        Type::I64 => Ok("i64".into()),
        Type::U8 => Ok("i8".into()),
        Type::U16 => Ok("i16".into()),
        Type::ByteBuffer | Type::OwnedStore { .. } => Ok(m::Ty::Struct(vec![
            m::Ty::Ptr,
            m::Ty::Int(64),
            m::Ty::Int(64),
        ])),
        Type::ForeignHandle | Type::CWideString | Type::ForeignCallback { .. } => Ok("ptr".into()),
        Type::Array { element, length } => Ok(m::Ty::array(
            u64::from(*length),
            aggregate_field_type(element)?,
        )),
        Type::Tuple(fields) => Ok(m::Ty::Struct(
            fields
                .iter()
                .map(aggregate_field_type)
                .collect::<CompileResult<Vec<_>>>()?,
        )),
        Type::Record { fields, .. } | Type::Package { fields, .. } => Ok(m::Ty::Struct(
            fields
                .iter()
                .map(|(_, t)| aggregate_field_type(t))
                .collect::<CompileResult<Vec<_>>>()?,
        )),
        Type::Enum { .. } => enums::llvm_enum_type(ty),
        Type::ErrorUnion { error, result } => Ok(m::Ty::Struct(vec![
            m::Ty::Int(1),
            aggregate_field_type(error)?,
            aggregate_field_type(result)?,
        ])),
        Type::Closure(id) => Ok(environment_type(*id)),
        Type::FunctionPointer { .. } => Ok("ptr".into()),
        Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Slice { .. }) => {
            Ok(m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64)]))
        }
        Type::Address { inner, .. }
            if inner.is_copy_value()
                || matches!(inner.as_ref(), Type::Closure(_))
                || ty.is_shared_resource_head_address()
                || ty.is_package_address() =>
        {
            Ok("ptr".into())
        }
        Type::Desc(payload)
            if (matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value())
                || payload.as_ref() == &Type::ByteBuffer
                || matches!(payload.as_ref(), Type::OwnedStore { .. }) =>
        {
            Ok("ptr".into())
        }
        Type::Managed(payload)
            if (matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value())
                || payload.as_ref() == &Type::ByteBuffer
                || matches!(payload.as_ref(), Type::OwnedStore { .. }) =>
        {
            llvm_type(payload)
        }
        _ => unsupported(format!("LLVM type {}", ty.name()), None),
    }
}
fn aggregate_field_type(ty: &Type) -> CompileResult<m::Ty> {
    if *ty == Type::Unit {
        Ok("{}".into())
    } else {
        llvm_type(ty)
    }
}
fn capture_type(ty: &Type) -> CompileResult<m::Ty> {
    match ty {
        Type::Unit => Ok("i8".into()),
        _ if ty.is_copy_value()
            || matches!(ty, Type::Closure(_))
            || matches!(ty, Type::Address { inner, .. } if inner.is_copy_value() || matches!(inner.as_ref(), Type::Closure(_)))
            || (ty.is_package_address() && matches!(ty, Type::Address { mutable: false, .. }))
            || matches!(ty, Type::Managed(payload) | Type::Desc(payload) if payload.is_copy_value())
            || matches!(ty, Type::Managed(payload) | Type::Desc(payload) if payload.as_ref() == &Type::ByteBuffer)
            || matches!(ty, Type::Managed(payload) if matches!(payload.as_ref(), Type::OwnedStore { .. }))
            || matches!(ty, Type::Package { .. }) =>
        {
            llvm_type(ty)
        }
        _ => unsupported(format!("LLVM capture field {}", ty.name()), None),
    }
}
fn alignment(ty: &Type, closures: &BTreeMap<ClosureId, &ClosureDefinition>) -> CompileResult<u32> {
    match ty {
        Type::Unit | Type::Bool => Ok(1),
        Type::U8 => Ok(1),
        Type::U16 => Ok(2),
        Type::ByteBuffer | Type::OwnedStore { .. } => Ok(8),
        Type::ForeignHandle | Type::CWideString | Type::ForeignCallback { .. } => Ok(8),
        Type::I32 => Ok(4),
        Type::I64 => Ok(8),
        Type::FunctionPointer { .. } => Ok(8),
        Type::Address { inner, .. }
            if inner.is_address_pointee()
                || matches!(inner.as_ref(), Type::Closure(_))
                || ty.is_shared_resource_head_address()
                || ty.is_package_address() =>
        {
            Ok(8)
        }
        Type::Desc(payload)
            if (matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value())
                || payload.as_ref() == &Type::ByteBuffer
                || matches!(payload.as_ref(), Type::OwnedStore { .. }) =>
        {
            Ok(8)
        }
        Type::Managed(payload)
            if (matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value())
                || payload.as_ref() == &Type::ByteBuffer
                || matches!(payload.as_ref(), Type::OwnedStore { .. }) =>
        {
            alignment(payload, closures)
        }
        Type::Array { element, .. } => alignment(element, closures),
        Type::Tuple(fields) => fields
            .iter()
            .try_fold(1, |a, t| Ok(a.max(alignment(t, closures)?))),
        Type::Record { fields, .. } | Type::Package { fields, .. } => fields
            .iter()
            .try_fold(1, |a, (_, t)| Ok(a.max(alignment(t, closures)?))),
        Type::Enum { .. } => Ok(enums::union_layout(ty)?.align),
        Type::ErrorUnion { error, result } => {
            Ok(alignment(error, closures)?.max(alignment(result, closures)?))
        }
        Type::Closure(id) => {
            let definition = closures.get(id).ok_or_else(|| {
                vec![Diagnostic::error(
                    Phase::Codegen,
                    "E-CODEGEN-CFG",
                    None,
                    "closure layout has no definition",
                )]
            })?;
            definition.captures.iter().try_fold(1, |maximum, field| {
                // Verify checked the complete inline environment DAG and depth
                // before this recursive machine-alignment calculation.
                if !field.is_copy_value()
                    && !matches!(field, Type::Closure(_))
                    && !matches!(field,Type::Address { inner, .. } if inner.is_copy_value() || matches!(inner.as_ref(), Type::Closure(_)))
                    && !(field.is_package_address() && matches!(field, Type::Address { mutable: false, .. }))
                    && !matches!(field,Type::Managed(payload) | Type::Desc(payload) if payload.is_copy_value())
                    && !matches!(field,Type::Managed(payload) | Type::Desc(payload) if payload.as_ref() == &Type::ByteBuffer)
                    && !matches!(field, Type::Managed(payload) if matches!(payload.as_ref(), Type::OwnedStore { .. }))
                    && !matches!(field, Type::Package { .. })
                {
                    return unsupported("unsupported closure field alignment", None);
                }
                let field_alignment = alignment(field, closures)?;
                Ok(maximum.max(field_alignment))
            })
        }
        _ => unsupported(format!("LLVM alignment for {}", ty.name()), None),
    }
}
fn invalid<T>(code: &str, message: impl Into<String>, span: Option<Span>) -> CompileResult<T> {
    Err(vec![Diagnostic::error(Phase::Codegen, code, span, message)])
}
fn unsupported<T>(message: impl Into<String>, span: Option<Span>) -> CompileResult<T> {
    Err(vec![Diagnostic::unsupported(Phase::Codegen, message, span)])
}

fn callable_parameter_type(ty: &Type, closures: &[ClosureDefinition]) -> bool {
    let id = match ty {
        Type::Closure(id) => id,
        Type::Address { inner, .. } => match inner.as_ref() {
            Type::Closure(id) => id,
            _ => return false,
        },
        _ => return false,
    };
    closures
        .get(id.0 as usize)
        .is_some_and(|definition| definition.id == *id)
}
