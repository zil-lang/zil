#![forbid(unsafe_code)]

//! Lowering starts at the verifier's immutable capability. The public LIR is
//! data, not authority: the LLVM backend rebinds it to this same capability.

use zil_base::{CompileResult, Diagnostic, Phase, SCHEMA_VERSION};
use zil_ir::{
    CallableCleanup, CallableKind, CallableReturnSource, LirModule, LirView, ScalarLayout, Type,
    TypedFunction,
};
use zil_verify::VerifiedModule;
mod owned_sums;

/// Retain the independently checked CFG and concrete closure definitions, and
/// attach the supported target layout. RegisterDefer stays in the retained CFG
/// as a checked fact; its adjacent TakeLocal/CallClosureOnce carries the action.
/// No source AST, candidate plan, or raw
/// typed module is an entry point.
///
/// ```compile_fail
/// fn bypass(raw: zil_ir::TypedModule) {
///     let _ = zil_lower::lower(&raw);
/// }
/// ```
pub fn lower(verified: &VerifiedModule) -> CompileResult<LirModule> {
    let (view, mut budget) = lower_view_with_budget(verified)?;
    // Clone through the existing instruction walk, paying new variable-length
    // byte payloads before allocating them. Borrowed lowering has no such copy.
    let functions = view
        .functions
        .iter()
        .map(|function| clone_function(function, &mut budget))
        .collect::<CompileResult<_>>()?;
    Ok(LirModule {
        schema_version: view.schema_version,
        module_id: view.module_id,
        verified_fingerprint: view.verified_fingerprint,
        target: view.target.clone(),
        functions,
        closures: view.closures.to_vec(),
        resources: view.resources.to_vec(),
        layouts: view.layouts.into_owned(),
    })
}

/// Run the same lowering admission checks, borrowing immutable verified CFG
/// storage. The returned public raw view must be rebound by the backend.
pub fn lower_view(verified: &VerifiedModule) -> CompileResult<LirView<'_>> {
    lower_view_with_budget(verified).map(|(view, _)| view)
}

fn lower_view_with_budget(
    verified: &VerifiedModule,
) -> CompileResult<(LirView<'_>, owned_sums::Budget<'_>)> {
    let module = verified.module();
    let target = &module.target;
    if module.schema_version != SCHEMA_VERSION {
        return Err(vec![Diagnostic::error(
            Phase::Lower,
            "E-LOWER-SCHEMA",
            None,
            "the verified module has an unsupported schema version",
        )]);
    }
    if !target.is_supported_x64_llvm() {
        return Err(vec![Diagnostic::unsupported(
            Phase::Lower,
            "lowering supports checked x86_64 Linux and Win64 LLVM targets",
            None,
        )]);
    }
    let use_sum_extension = verified.uses_owned_sum_extension();
    let mut sum_budget = owned_sums::Budget::new(verified.limits());
    // Reserve the complete new callable relation before LIR retains/clones it.
    // This budget is also consumed by ordinary owning/stored lowering below.
    sum_budget.callable_metadata(module)?;
    sum_budget.dyn_tables(module)?;
    for closure in &module.closures {
        for ty in &closure.captures {
            if matches!(ty,Type::Address{inner,..} if matches!(inner.as_ref(),Type::Tuple(_) | Type::Enum{..} | Type::ErrorUnion{..}))
                && !sum_budget.admits(ty, &module.resources, 0)?
            {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Lower,
                    "captured owning-base address needs a complete admitted pointee",
                    None,
                )]);
            }
        }
        // An owning callback signature is an ordinary complete data boundary.
        // Charge its actual type trees to the same request budget used by
        // ordinary function signatures. Capture admission remains separate.
        let owning_result = if matches!(
            closure.result,
            Type::Package { .. } | Type::Enum { .. } | Type::ErrorUnion { .. }
        ) && !closure.result.is_copy_value()
        {
            callback_data_shape(&closure.result)
                && sum_budget.admits(&closure.result, &module.resources, 0)?
        } else {
            false
        };
        let mut owning_parameters = false;
        let mut complete_parameters = true;
        for ty in &closure.parameters {
            if ty.is_copy_value() {
                continue;
            }
            if owning_result && let Type::Address { inner, .. } = ty {
                // Verify already proved the actual synchronous input loan and
                // closed owned result separately. Retain the ordinary pointer
                // ABI and pay for its complete physical pointee layout.
                let pointee = match inner.as_ref() {
                    Type::Slice { element } => element.as_ref(),
                    other => other,
                };
                if !sum_budget.admits(pointee, &module.resources, 0)? {
                    complete_parameters = false;
                    break;
                }
                continue;
            }
            if !callback_data_shape(ty) {
                complete_parameters = false;
                break;
            }
            owning_parameters = true;
            let admitted = sum_budget.admits(ty, &module.resources, 0)?;
            if !admitted {
                complete_parameters = false;
                break;
            }
        }
        let owning_signature = (owning_result || owning_parameters)
            && complete_parameters
            && (closure.result.is_copy_value() || owning_result);
        if verified.uses_stored_borrows() {
            for t in &closure.captures {
                sum_budget.borrowed(t, 0)?;
            }
        }
        // Actual retained Fixed metadata is already independently proved. This
        // only admits the existing shared pointer ABI after capability binding.
        let captured_fixed_signature = verified.uses_stored_borrows()
            && closure
                .storage_life
                .as_ref()
                .is_some_and(|life| life.fixed_binder_count > 0)
            && ((closure.lending_captures.is_empty() && closure.result.is_b0_scalar())
                || (!closure.lending_captures.is_empty()
                    && matches!(closure.kind, CallableKind::Fn | CallableKind::FnMut)
                    && matches!(&closure.result, Type::Address { mutable:false, inner } if inner.is_b0_scalar())));
        let qualified_signature = verified.uses_stored_borrows()
            && closure.captures.iter().all(|t| {
                matches!(
                    t,
                    Type::Unit
                        | Type::Bool
                        | Type::I32
                        | Type::I8
                        | Type::I16
                        | Type::Isize
                        | Type::I64
                        | Type::U64
                        | Type::Usize
                        | Type::U8
                        | Type::U16
                        | Type::U32
                        | Type::F32
                        | Type::F64
                )
            })
            && closure.lending_captures.is_empty()
            && closure.storage_life.is_none()
            && matches!(
                closure.result,
                Type::Unit
                    | Type::Bool
                    | Type::I32
                    | Type::I8
                    | Type::I16
                    | Type::Isize
                    | Type::I64
                    | Type::U64
                    | Type::Usize
                    | Type::U8
                    | Type::U16
                    | Type::U32
                    | Type::F32
                    | Type::F64
            );
        if qualified_signature || captured_fixed_signature {
            for ty in &closure.parameters {
                if matches!(ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::BorrowedStorage { .. }))
                {
                    sum_budget.borrowed(ty, 0)?;
                }
            }
        }
        let invocation_signature = closure.lending_captures.is_empty()
            && matches!(&closure.result,Type::Address{inner,..} if inner.is_address_pointee())
            && closure
                .parameters
                .iter()
                .any(|p| matches!(p, Type::Address { .. }));
        let mut complete_captures = true;
        for ty in &closure.captures {
            let legacy_capture = {
                ty.is_copy_value()
                    || callable_parameter_type(ty, &module.closures)
                    || ty.is_shared_sync_cell_address()
                    || (captured_fixed_signature && shared_qualified_capture_address(ty))
                    || matches!(ty, Type::Address { inner, .. } if inner.is_copy_value() || matches!(inner.as_ref(),Type::Tuple(_) | Type::Enum{..} | Type::ErrorUnion{..}))
                    // Only the immutable verifier capability reaches Lower;
                    // ordinary shared Product captures retain plain pointers.
                    || ty.is_package_address()
                    // Verify already qualified the complete captured Store
                    // leaf, canonical definition, result and actual operands.
                    // A borrowed Store description still has no owning ABI.
                    || (resource_type(ty, &module.resources)
                        && !matches!(ty, Type::Desc(p) if matches!(p.as_ref(), Type::OwnedStore { .. })))
                    || (package_capture_type(ty, &module.resources)
                        && !package_array_fields(ty))
            };
            if !legacy_capture && !sum_budget.inline_store_capture(ty, &module.resources)? {
                complete_captures = false;
                break;
            }
        }
        if !matches!(
            closure.kind,
            CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce
        ) || !(owning_signature || invocation_signature || closure.result.is_copy_value()
            || (!closure.lending_captures.is_empty()
                && matches!(closure.kind, CallableKind::Fn | CallableKind::FnMut)
                && matches!(&closure.result, Type::Address { inner, mutable: false }
                    if inner.is_copy_value() || matches!(&**inner, Type::Slice { element } if element.is_copy_value()))))
            || !complete_captures
            || !closure.parameters.iter().all(|ty| {
                owning_signature || ty.is_copy_value() || ty.is_shared_sync_cell_address()
                    || (captured_fixed_signature && shared_qualified_capture_address(ty)) || (qualified_signature
                        && matches!(ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::BorrowedStorage { .. })))
                    || ((closure.result.is_copy_value() || invocation_signature)
                        && matches!(ty, Type::Address { inner, .. } if inner.is_address_pointee()))
            })
        {
            return Err(vec![Diagnostic::unsupported(
                Phase::Lower,
                "lowering requires a finite closure environment, Copy or Copy/slice-address parameters and a verified result contract",
                None,
            )]);
        }
    }
    for function in &module.functions {
        if verified.uses_stored_borrows()
            && let Some(life) = &function.storage_life
        {
            sum_budget.stored_contract(life)?;
        }
        if !forwarded_callable_signature(function) {
            return Err(vec![Diagnostic::unsupported(
                Phase::Lower,
                "forwarded callable result requires the same actual by-value parameter and Returned role",
                Some(function.span),
            )]);
        }
        let old_return = sum_budget.borrowed(&function.return_type, 0)?
            || function.return_type.is_copy_value()
            || function.return_type.is_scalar_or_address()
            || matches!(function.return_type, Type::MutexGuard { .. })
            || matches!(function.return_type, Type::OwnedDyn { .. })
            || resource_type(&function.return_type, &module.resources)
            || package_value_type(&function.return_type, &module.resources)
            || error_type(&function.return_type, &module.resources)
            || owned_array_type(&function.return_type, &module.resources)
            || (matches!(function.return_type, Type::Closure(_))
                && function.callable_return_contract.is_some());
        let extra_return = if old_return {
            false
        } else if use_sum_extension
            || matches!(&function.return_type, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }))
        {
            sum_budget.admits(&function.return_type, &module.resources, 0)?
        } else {
            false
        };
        if !old_return && !extra_return && !sum_budget.pure_composite(&function.return_type, 0)? {
            return Err(vec![Diagnostic::unsupported(
                Phase::Lower,
                "lowering requires a supported ordinary function result",
                Some(function.span),
            )]);
        }
        for id in &function.parameters {
            let ty = &function.locals[id.0 as usize].ty;
            let old = sum_budget.borrowed(ty, 0)?
                || ty.is_copy_value()
                || ty.is_scalar_or_address()
                || matches!(ty, Type::MutexGuard { .. })
                || matches!(ty, Type::OwnedDyn { .. })
                || resource_type(ty, &module.resources)
                || resource_head_address(ty)
                || package_value_type(ty, &module.resources)
                || error_type(ty, &module.resources)
                || owned_array_type(ty, &module.resources)
                || callable_parameter_type(ty, &module.closures);
            let extra = if old {
                false
            } else if use_sum_extension
                || matches!(ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }))
            {
                sum_budget.admits(ty, &module.resources, 0)?
            } else {
                false
            };
            if !old && !extra && !sum_budget.pure_composite(ty, 0)? {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Lower,
                    "lowering requires a supported ordinary parameter",
                    Some(function.span),
                )]);
            }
        }
        for ty in function
            .locals
            .iter()
            .map(|local| &local.ty)
            .chain(function.values.iter())
        {
            let concrete_closure = callable_parameter_type(ty, &module.closures);
            if !sum_budget.borrowed(ty, 0)?
                && !ty.is_copy_value()
                && !concrete_closure
                && !ty.is_shared_sync_cell_address()
                && !matches!(ty, Type::Address { inner, .. } if inner.is_address_pointee())
                && !matches!(ty, Type::MutexGuard { .. })
                && !matches!(ty, Type::OwnedDyn { .. })
                && !matches!(ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::MutexGuard { .. }))
                && !resource_head_address(ty)
                && !ty.is_exclusive_task_head_address()
                && !ty.is_package_address()
                && !resource_type(ty, &module.resources)
                && !package_value_type(ty, &module.resources)
                && !error_type(ty, &module.resources)
                && !owned_array_type(ty, &module.resources)
                && !if use_sum_extension
                    || matches!(ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }))
                {
                    sum_budget.admits(ty, &module.resources, 0)?
                } else {
                    false
                }
                && !sum_budget.pure_value_or_address(ty, 0)?
            {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Lower,
                    format!("lowering type {}", ty.name()),
                    Some(function.span),
                )]);
            }
        }
    }
    Ok((
        LirView {
            resources: &module.resources,
            schema_version: SCHEMA_VERSION,
            module_id: module.module_id,
            verified_fingerprint: verified.fingerprint(),
            target,
            functions: &module.functions,
            closures: &module.closures,
            layouts: std::borrow::Cow::Owned(scalar_layouts_for_width(target.pointer_width)),
        },
        sum_budget,
    ))
}

fn clone_function(
    function: &TypedFunction,
    budget: &mut owned_sums::Budget<'_>,
) -> CompileResult<TypedFunction> {
    // Exhaustive destructuring makes new retained fields explicit consumers.
    let TypedFunction {
        storage_life,
        call_kind,
        reference_return_bound,
        callable_return_contract,
        callable_contract,
        protocol_contract,
        id,
        name,
        foreign,
        environment,
        parameters,
        return_type,
        result_mode,
        locals,
        values,
        blocks,
        entry,
        span,
    } = function;
    let blocks = blocks
        .iter()
        .map(|block| {
            let instructions = block
                .instructions
                .iter()
                .map(|instruction| {
                    if let zil_ir::InstructionKind::StaticUtf8 { bytes } = &instruction.kind {
                        budget.static_bytes(bytes.len())?;
                    }
                    if let zil_ir::InstructionKind::CallDynSlot { arguments, .. } =
                        &instruction.kind
                    {
                        budget.static_bytes(
                            arguments
                                .len()
                                .saturating_mul(std::mem::size_of::<zil_ir::ValueId>()),
                        )?;
                    }
                    Ok(instruction.clone())
                })
                .collect::<CompileResult<_>>()?;
            Ok(zil_ir::BasicBlock {
                id: block.id,
                instructions,
                terminator: block.terminator.clone(),
            })
        })
        .collect::<CompileResult<_>>()?;
    Ok(TypedFunction {
        storage_life: storage_life.clone(),
        call_kind: *call_kind,
        reference_return_bound: reference_return_bound.clone(),
        callable_return_contract: callable_return_contract.clone(),
        callable_contract: callable_contract.clone(),
        protocol_contract: protocol_contract.clone(),
        id: *id,
        name: name.clone(),
        foreign: foreign.clone(),
        environment: *environment,
        parameters: parameters.clone(),
        return_type: return_type.clone(),
        result_mode: *result_mode,
        locals: locals.clone(),
        values: values.clone(),
        blocks,
        entry: *entry,
        span: *span,
    })
}

fn resource_head_address(ty: &Type) -> bool {
    matches!(ty,Type::Address{inner,..} if matches!(inner.as_ref(),Type::Managed(payload) if matches!(payload.as_ref(),Type::ByteBuffer|Type::OwnedStore{..}|Type::AsyncRegistration{..}) || matches!(payload.as_ref(),Type::Record{..}) && payload.is_copy_value()))
}

fn resource_type(ty: &Type, resources: &[zil_ir::ResourceDefinition]) -> bool {
    if matches!(ty, Type::Managed(p) | Type::Desc(p) if matches!(p.as_ref(), Type::Mutex { .. }) && resources.iter().any(|r| r.payload == **p && r.teardown.is_none()))
    {
        return true;
    }
    if matches!(ty, Type::Managed(p) | Type::Desc(p) if matches!(p.as_ref(),Type::Registration{..}|Type::AsyncRegistration{..}) && resources.iter().any(|r|r.payload==**p && r.teardown.is_none()))
    {
        return true;
    }
    if matches!(ty, Type::Managed(p) | Type::Desc(p) if matches!(p.as_ref(),Type::TaskI32|Type::Task{..}))
        && resources.iter().any(|r| {
            (r.name == zil_ir::TASK_I32_RESOURCE_NAME
                || r.name.starts_with(zil_ir::TASK_RESOURCE_NAME_PREFIX))
                && matches!(r.payload, Type::TaskI32 | Type::Task { .. })
                && r.teardown.is_none()
        })
    {
        return true;
    }
    if matches!(ty, Type::Managed(payload) | Type::Desc(payload)
        if matches!(payload.as_ref(), Type::OwnedStore { .. })
        && resources.iter().any(|r| r.payload == **payload && r.teardown.is_none()))
    {
        return true;
    }
    if matches!(ty, Type::Managed(payload) | Type::Desc(payload)
        if payload.as_ref() == &Type::ByteBuffer
        && resources.iter().any(|r| r.name == zil_ir::BYTE_BUFFER_RESOURCE_NAME
            && r.payload == Type::ByteBuffer && r.teardown.is_none()))
    {
        return true;
    }
    match ty {
        Type::Managed(payload) | Type::Desc(payload) => {
            matches!(payload.as_ref(), Type::Record { name, .. }
                if payload.is_copy_value() && resources.iter().any(|r| &r.name == name && &r.payload == payload.as_ref()))
        }
        _ => false,
    }
}

fn package_type(ty: &Type, resources: &[zil_ir::ResourceDefinition]) -> bool {
    let Type::Package { fields, .. } = ty else {
        return false;
    };
    fields.iter().any(|(_, ty)| matches!(ty, Type::Managed(_)))
        && fields.iter().all(|(_, ty)| {
            ty.is_copy_value() || matches!(ty, Type::Closure(_)) || matches!(ty, Type::Managed(_) if resource_type(ty, resources))
                || matches!(ty, Type::Array { element, length } if *length > 0 && package_type(element, resources) && !package_array_fields(element) && !package_store_fields(element))
        })
}

fn copy_only_package(ty: &Type) -> bool {
    matches!(ty, Type::Package { name, fields }
    if !name.is_empty() && !fields.is_empty()
        && fields.iter().all(|(name, field)| {
            !name.is_empty() && (field.is_copy_value() || copy_only_package(field))
        }))
}

// Keep the existing predicate above for error-payload and array qualification.
// Verify has already bounded the full inline type and its cleanup paths.
fn package_value_type(ty: &Type, resources: &[zil_ir::ResourceDefinition]) -> bool {
    package_type(ty, resources) || owned_product_type(ty, resources) || copy_only_package(ty)
}

// Representation only: the caller has an immutable verified capability and
// has already paid the complete captured/parameter type metadata inventory.
fn shared_qualified_capture_address(ty: &Type) -> bool {
    matches!(ty, Type::Address { inner, mutable: false }
        if matches!(inner.as_ref(), Type::BorrowedStorage { storage, .. }
            if matches!(storage.as_ref(), Type::Record { .. } | Type::Tuple(_))))
}

fn package_capture_type(ty: &Type, resources: &[zil_ir::ResourceDefinition]) -> bool {
    package_type(ty, resources) || product_type(ty, resources)
}

fn product_type(ty: &Type, resources: &[zil_ir::ResourceDefinition]) -> bool {
    let qualified = matches!(ty, Type::BorrowedStorage { .. });
    let physical = match ty {
        Type::BorrowedStorage { storage, .. }
            if matches!(storage.as_ref(), Type::Package { .. }) =>
        {
            storage.as_ref()
        }
        other => other,
    };
    let Type::Package { fields, .. } = physical else {
        return false;
    };
    let mut owns = false;
    for (_, field) in fields {
        match field {
            Type::Managed(payload)
                if !matches!(payload.as_ref(), Type::OwnedStore { .. })
                    && resource_type(field, resources) =>
            {
                owns = true;
            }
            Type::Package { .. } if product_type(field, resources) => owns = true,
            Type::Package { .. } if copy_only_package(field) => {}
            Type::BorrowedStorage { storage, .. }
                if matches!(storage.as_ref(), Type::Package { .. })
                    && product_type(field, resources) =>
            {
                owns = true
            }
            Type::Address { inner, .. } if qualified && inner.is_address_pointee() => {}
            field if field.is_copy_value() => {}
            _ => return false,
        }
    }
    owns
}

fn owned_product_type(ty: &Type, resources: &[zil_ir::ResourceDefinition]) -> bool {
    let qualified = matches!(ty, Type::BorrowedStorage { .. });
    let physical = match ty {
        Type::BorrowedStorage { storage, .. }
            if matches!(storage.as_ref(), Type::Package { .. }) =>
        {
            storage.as_ref()
        }
        other => other,
    };
    let Type::Package { fields, .. } = physical else {
        return false;
    };
    let mut owns = false;
    for (_, field) in fields {
        match field {
            Type::Managed(_) if resource_type(field, resources) => {
                owns = true;
            }
            Type::Package { .. } if owned_product_type(field, resources) => owns = true,
            Type::Package { .. } if copy_only_package(field) => {}
            Type::BorrowedStorage { storage, .. }
                if matches!(storage.as_ref(), Type::Package { .. })
                    && owned_product_type(field, resources) =>
            {
                owns = true
            }
            Type::Address { inner, .. } if qualified && inner.is_address_pointee() => {}
            field if field.is_copy_value() => {}
            _ => return false,
        }
    }
    owns
}

fn package_array_fields(ty: &Type) -> bool {
    matches!(ty, Type::Package { fields, .. } if fields.iter().any(|(_, ty)| matches!(ty, Type::Array { element, .. } if matches!(element.as_ref(), Type::Package { .. }))))
}

fn package_store_fields(ty: &Type) -> bool {
    matches!(ty, Type::Package { fields, .. } if fields.iter().any(|(_, ty)| matches!(ty, Type::Managed(payload) | Type::Desc(payload) if matches!(payload.as_ref(), Type::OwnedStore { .. }))))
}

fn error_type(ty: &Type, resources: &[zil_ir::ResourceDefinition]) -> bool {
    matches!(ty, Type::ErrorUnion { error, result }
        if ((error.is_copy_value() || package_value_type(error, resources))
            && (result.is_copy_value() || package_value_type(result, resources)))
            || (error.is_copy_value() && matches!(result.as_ref(), Type::Managed(_))
                && resource_type(result, resources))
            || (matches!(result.as_ref(), Type::OwnedDyn { .. })
                && matches!(error.as_ref(), Type::Enum { name, variants }
                    if name == "@std.dyn:v1:DynCreateError"
                    && matches!(variants.as_slice(), [(case, fields)] if case == "Allocation" && fields.is_empty())))
            || (**error == Type::Unit && product_type(result, resources))
            || (**result == Type::Unit && product_type(error, resources)))
}

fn owned_array_type(ty: &Type, resources: &[zil_ir::ResourceDefinition]) -> bool {
    matches!(ty, Type::Array { element, length }
        if *length > 0 && (package_type(element, resources) || matches!(element.as_ref(), Type::Closure(_))))
}

/// The B0 ABI uses no storage for unit and natural scalar size/alignment. This
/// table describes storage; Boolean arithmetic still uses LLVM's `i1` values.
pub fn scalar_layouts() -> Vec<ScalarLayout> {
    scalar_layouts_for_width(64)
}
fn scalar_layouts_for_width(pointer_width: u16) -> Vec<ScalarLayout> {
    vec![
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
            ty: Type::F32,
            size: 4,
            alignment: 4,
        },
        ScalarLayout {
            ty: Type::F64,
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
            ty: Type::U32,
            size: 4,
            alignment: 4,
        },
        ScalarLayout {
            ty: Type::I8,
            size: 1,
            alignment: 1,
        },
        ScalarLayout {
            ty: Type::I16,
            size: 2,
            alignment: 2,
        },
        ScalarLayout {
            ty: Type::Isize,
            size: u32::from(pointer_width / 8),
            alignment: u32::from(pointer_width / 8),
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
    ]
}

fn callable_parameter_type(ty: &Type, closures: &[zil_ir::ClosureDefinition]) -> bool {
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

// Fresh factories retain their existing admission. A Parameter claim is
// transport metadata, not a new layout: the verified CFG must still carry the
// actual live value into Return. Lower keeps that CFG unchanged and checks the
// machine boundary against its actual parameter slot and declared role.
fn forwarded_callable_signature(function: &TypedFunction) -> bool {
    let Some(contract) = &function.callable_return_contract else {
        return true;
    };
    let CallableReturnSource::Parameter(index) = contract.source else {
        return true;
    };
    if function.foreign.is_some() || function.environment.is_some() {
        return false;
    }
    let Some(parameter) = function.parameters.get(index as usize) else {
        return false;
    };
    let Some(local) = function.locals.get(parameter.0 as usize) else {
        return false;
    };
    let Some(roles) = function
        .callable_contract
        .as_ref()
        .map(|c| c.roles.as_slice())
    else {
        return false;
    };
    let [role] = roles else {
        return false;
    };
    matches!((&local.ty, &function.return_type),
        (Type::Closure(input), Type::Closure(output)) if input == output && *input == contract.closure)
        && role.parameter == index
        && role.closure == contract.closure
        && role.kind == contract.kind
        && role.cleanup == CallableCleanup::Returned
}

// Callback data is complete ordinary Product/Sum, never a naked resource,
// address, description, or a callback nested as data. An ErrorUnion's owning
// arm must itself be a complete Product or Enum.
fn callback_data_shape(ty: &Type) -> bool {
    match ty {
        Type::Package { fields, .. } => fields.iter().all(|(_, field)| {
            field.is_copy_value() || matches!(field, Type::Managed(_)) || callback_data_shape(field)
        }),
        Type::Enum { variants, .. } => variants.iter().all(|(_, fields)| {
            fields.iter().all(|field| {
                field.is_copy_value()
                    || matches!(field, Type::Managed(_))
                    || callback_data_shape(field)
            })
        }),
        Type::ErrorUnion { error, result } => [error.as_ref(), result.as_ref()].iter().all(|arm| {
            arm.is_copy_value()
                || matches!(arm, Type::Package { .. } | Type::Enum { .. })
                    && callback_data_shape(arm)
        }),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{Fingerprint, Limits, ModuleId, ResultCategory, Span, TargetProfile};
    use zil_ir::{
        BasicBlock, BlockId, BlockPlan, CandidatePlan, ClosureDefinition, ClosureId, Constant,
        FunctionId, FunctionPlan, Instruction, InstructionKind, Terminator, TypedFunction,
        TypedModule, ValueId,
    };
    use zil_verify::verify_module;

    fn checked(target: TargetProfile) -> VerifiedModule {
        let module = TypedModule {
            dyn_declarations: Vec::new(),
            dyn_interfaces: Vec::new(),
            dyn_vtables: Vec::new(),
            life_declarations: Vec::new(),
            life_instances: Vec::new(),
            resources: Vec::new(),
            schema_version: SCHEMA_VERSION,
            module_id: ModuleId(1),
            source_fingerprint: Fingerprint::of(b"lower typed-IR fixture"),
            target: target.clone(),
            functions: vec![TypedFunction {
                storage_life: None,
                call_kind: zil_ir::CallKind::Direct,
                reference_return_bound: None,
                callable_return_contract: None,
                callable_contract: None,
                protocol_contract: None,
                result_mode: zil_ir::PassingMode::Infer,
                foreign: None,
                id: FunctionId(0),
                name: "main".into(),
                environment: None,
                parameters: vec![],
                return_type: Type::I32,
                locals: vec![],
                values: vec![Type::I32],
                blocks: vec![BasicBlock {
                    id: BlockId(0),
                    instructions: vec![Instruction {
                        result: Some(ValueId(0)),
                        kind: InstructionKind::Constant(Constant::I32(0)),
                        span: Span::default(),
                    }],
                    terminator: Terminator::Return(Some(ValueId(0))),
                }],
                entry: BlockId(0),
                span: Span::default(),
            }],
            closures: vec![],
            unsupported: vec![],
        };
        let mut plan = CandidatePlan::empty_for(&module);
        plan.functions.push(FunctionPlan {
            function: FunctionId(0),
            holds: (0..module.functions[0].values.len())
                .map(|i| zil_ir::ValueHold {
                    value: ValueId(i as u32),
                    roots: vec![],
                })
                .collect(),
            blocks: vec![BlockPlan {
                block: BlockId(0),
                initialized_in: vec![],
                initialized_out: vec![],
            }],
        });
        verify_module(module, plan, &target, &Limits::default()).unwrap()
    }

    #[test]
    fn owned_static_utf8_clone_prepays_exact_bytes_without_charging_old_clones() {
        // Component-level retained-IR copy check; source/native acceptance is
        // covered in the existing driver target by the parent implementation.
        let original = checked(TargetProfile::host_llvm());
        let scalar = &original.module().functions[0];
        let zero = zil_base::Limits {
            max_analysis_steps: 0,
            ..zil_base::Limits::default()
        };
        assert_eq!(
            clone_function(scalar, &mut owned_sums::Budget::new(&zero)).unwrap(),
            *scalar
        );
        for bytes in [b"".as_slice(), b"A\0Z".as_slice(), "é中😀".as_bytes()] {
            let mut function = scalar.clone();
            function.values[0] = Type::Address {
                mutable: false,
                inner: Box::new(Type::Slice {
                    element: Box::new(Type::U8),
                }),
            };
            function.return_type = function.values[0].clone();
            function.blocks[0].instructions[0].kind = InstructionKind::StaticUtf8 {
                bytes: bytes.to_vec(),
            };
            let needed = bytes.len() + 1;
            for _ in 0..2 {
                let exact = zil_base::Limits {
                    max_analysis_steps: needed,
                    ..zil_base::Limits::default()
                };
                assert_eq!(
                    clone_function(&function, &mut owned_sums::Budget::new(&exact)).unwrap(),
                    function
                );
                let short = zil_base::Limits {
                    max_analysis_steps: needed - 1,
                    ..zil_base::Limits::default()
                };
                let errors =
                    clone_function(&function, &mut owned_sums::Budget::new(&short)).unwrap_err();
                assert!(errors.iter().all(|error| error.phase == Phase::Lower
                    && error.category == zil_base::ResultCategory::ResourceLimit));
            }
        }
    }

    #[test]
    fn preserves_the_exact_checked_cfg_and_target_identity() {
        let verified = checked(TargetProfile::host_llvm());
        let lowered = lower(&verified).unwrap();
        assert_eq!(lowered.functions, verified.module().functions);
        assert_eq!(lowered.closures, verified.module().closures);
        assert_eq!(lowered.target, verified.module().target);
        assert_eq!(lowered.verified_fingerprint, verified.fingerprint());
        assert_eq!(
            lowered
                .layouts
                .iter()
                .map(|layout| (layout.size, layout.alignment))
                .collect::<Vec<_>>(),
            vec![
                (0, 1),
                (1, 1),
                (4, 4),
                (8, 8),
                (4, 4),
                (8, 8),
                (1, 1),
                (2, 2),
                (4, 4),
                (1, 1),
                (2, 2),
                (8, 8),
                (8, 8),
                (8, 8)
            ]
        );
    }

    #[test]
    fn verified_but_unimplemented_target_is_explicitly_unsupported() {
        let mut target = TargetProfile::host_llvm();
        target.triple = "aarch64-unknown-linux-gnu".into();
        let verified = checked(target);
        let diagnostics = lower(&verified).unwrap_err();
        assert_eq!(diagnostics[0].category, ResultCategory::Unsupported);
        assert_eq!(diagnostics[0].phase, Phase::Lower);
    }

    #[test]
    fn preserves_the_verified_environment_table_and_closure_operands_exactly() {
        let mut module = checked(TargetProfile::host_llvm()).module().clone();
        module.functions[0].values = vec![Type::I32, Type::Closure(ClosureId(0)), Type::I32];
        module.functions[0].blocks[0].instructions.extend([
            Instruction {
                result: Some(ValueId(1)),
                kind: InstructionKind::MakeClosure {
                    closure: ClosureId(0),
                    captures: vec![ValueId(0)],
                },
                span: Span::default(),
            },
            Instruction {
                result: Some(ValueId(2)),
                kind: InstructionKind::CallClosure {
                    callee: ValueId(1),
                    arguments: vec![],
                },
                span: Span::default(),
            },
        ]);
        module.functions[0].blocks[0].terminator = Terminator::Return(Some(ValueId(2)));
        module.functions.push(TypedFunction {
            storage_life: None,
            call_kind: zil_ir::CallKind::Direct,
            reference_return_bound: None,
            callable_return_contract: None,
            callable_contract: None,
            protocol_contract: None,
            result_mode: zil_ir::PassingMode::Infer,
            foreign: None,
            id: FunctionId(1),
            name: "$closure0".into(),
            environment: Some(ClosureId(0)),
            parameters: vec![],
            return_type: Type::I32,
            locals: vec![],
            values: vec![Type::I32],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction {
                    result: Some(ValueId(0)),
                    kind: InstructionKind::ReadCapture { index: 0 },
                    span: Span::default(),
                }],
                terminator: Terminator::Return(Some(ValueId(0))),
            }],
            entry: BlockId(0),
            span: Span::default(),
        });
        module.closures.push(ClosureDefinition {
            storage_life: None,
            lending_captures: Vec::new(),
            id: ClosureId(0),
            function: FunctionId(1),
            captures: vec![Type::I32],
            parameters: vec![],
            result: Type::I32,
            kind: CallableKind::Fn,
        });
        let mut plan = CandidatePlan::empty_for(&module);
        plan.functions = (0..2)
            .map(|id| FunctionPlan {
                function: FunctionId(id),
                holds: (0..module.functions[id as usize].values.len())
                    .map(|i| zil_ir::ValueHold {
                        value: ValueId(i as u32),
                        roots: vec![],
                    })
                    .collect(),
                blocks: vec![BlockPlan {
                    block: BlockId(0),
                    initialized_in: vec![],
                    initialized_out: vec![],
                }],
            })
            .collect();
        let target = module.target.clone();
        let verified = verify_module(module, plan, &target, &Limits::default()).unwrap();
        let lowered = lower(&verified).unwrap();
        assert_eq!(lowered.closures, verified.module().closures);
        assert_eq!(lowered.functions, verified.module().functions);
        assert_eq!(lowered.verified_fingerprint, verified.fingerprint());
        assert!(!Type::Closure(ClosureId(0)).is_b0_scalar());
    }
}
