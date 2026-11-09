//! Private physical snapshots for large values. These pointers never represent
//! source addresses or grant Copy. Owned snapshots follow verified moves and
//! cleanup flags; copying their bytes never creates another cleanup obligation.
use super::*;
use zil_base::Limits;
mod emission_work;
#[cfg(test)]
mod emission_work_tests;
#[cfg(test)]
mod max_plan_reference;

const MIN_BYTES: u64 = 4096;

#[derive(Clone)]
#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
pub(super) struct Slot {
    pub(super) ty: m::Ty,
    pub(super) bytes: u64,
    pub(super) align: u32,
    pub(super) storage: Option<super::copy_slots::Storage>,
    work: usize,
}
pub(super) type Plan = BTreeMap<ValueId, Slot>;

fn task_port_pointee(ty: &Type) -> bool {
    match ty {
        Type::Unit
        | Type::Bool
        | Type::U8
        | Type::U16
        | Type::I32
        | Type::I64
        | Type::U64
        | Type::Usize
        | Type::F32
        | Type::F64 => true,
        Type::Record { fields, .. } => fields.iter().all(|(_, t)| task_port_pointee(t)),
        Type::Tuple(fields) => fields.iter().all(task_port_pointee),
        Type::Array { element, .. } | Type::Slice { element } => task_port_pointee(element),
        _ => false,
    }
}

pub(super) fn plan(
    lir: &LirView<'_>,
    limits: &Limits,
    remaining_work: usize,
) -> CompileResult<super::large_copy_abi::Plans> {
    #[cfg(test)]
    if max_plan_reference::enabled() {
        return max_plan_reference::plan(lir, limits, remaining_work);
    }
    let mut work = remaining_work;
    let mut plans = BTreeMap::new();
    let store_protocols = dynamic_store::plan(lir.resources, limits, &mut work)?;
    for function in lir.functions {
        dynamic_store::reserve_operations(
            function,
            lir.resources.len(),
            &store_protocols,
            &mut work,
        )?;
        let mut values = Plan::new();
        for (index, ty) in function.values.iter().enumerate() {
            if !matches!(
                enums::storage_shape(ty),
                Type::Array { .. }
                    | Type::Tuple(_)
                    | Type::Record { .. }
                    | Type::Package { .. }
                    | Type::Enum { .. }
                    | Type::ErrorUnion { .. }
            ) {
                continue;
            }
            // Reserve all recursive predicates, layout construction and bounded
            // metadata clones before examining or retaining the complete shape.
            let before = work;
            reserve(ty, &mut work, 0, limits)?;
            if !matches!(enums::storage_shape(ty), Type::Package { .. })
                && !ty.is_copy_value()
                && !stored_borrows::pure_composite(ty)
                && !stored_borrows::shared_array_snapshot(ty)
            {
                continue;
            }
            let machine = llvm_type(ty)?;
            if unresolved_layout(&machine) {
                continue;
            }
            let (bytes, align) = layout(&machine)?;
            if bytes >= MIN_BYTES {
                if matches!(enums::storage_shape(ty), Type::Enum { .. }) {
                    // make_variant/read_variant clone the source enum as well
                    // as its machine layout. Source names are absent from the
                    // machine type; pay their bytes before retaining the slot,
                    // then include this fee in repeated emission reservations.
                    reserve_enum_metadata(ty, &mut work, 0, limits)?;
                }
                values.insert(
                    ValueId(index as u32),
                    Slot {
                        ty: machine,
                        bytes,
                        align,
                        storage: None,
                        work: before - work,
                    },
                );
            }
        }
        if !values.is_empty() {
            // Reserve repeated machine-type clones, slot accesses and emitted
            // operations before growing function event buffers. Use a paid
            // shape upper bound; wide construction/call operands count too.
            let shape = values.values().map(|slot| slot.work).max().unwrap();
            // Keep complete source planning fees in Slot.work. Repeated
            // physical consumers use a separately paid machine-clone metric.
            let machines = emission_work::allocations(&values, &mut work, limits)?;
            for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
                let operands = match &instruction.kind {
                    InstructionKind::MakeAggregate { fields }
                    | InstructionKind::MakePackage { fields }
                    | InstructionKind::MakeVariant { fields, .. } => fields.len(),
                    InstructionKind::Call { arguments, .. }
                    | InstructionKind::CallIndirect { arguments, .. }
                    | InstructionKind::CallDynSlot { arguments, .. }
                    | InstructionKind::CallClosure { arguments, .. }
                    | InstructionKind::CallClosureShared { arguments, .. }
                    | InstructionKind::CallClosureMut { arguments, .. }
                    | InstructionKind::CallClosureBorrowed { arguments, .. }
                    | InstructionKind::CallClosureOnce { arguments, .. } => arguments.len(),
                    _ => 2,
                };
                let precise = emission_work::instruction(
                    function,
                    instruction,
                    &values,
                    &machines,
                    &mut work,
                    limits,
                )?;
                let cost = if precise {
                    0
                } else {
                    shape
                        .saturating_mul(operands.saturating_add(4))
                        .saturating_add(32)
                };
                work = work.checked_sub(cost).ok_or_else(|| {
                    vec![Diagnostic::limit(
                        Phase::Codegen,
                        "Copy aggregate snapshot emission budget exceeded",
                        Some(instruction.span),
                    )]
                })?;
                if let InstructionKind::MakeVariant { fields, .. } = &instruction.kind
                    && instruction
                        .result
                        .is_some_and(|id| values.contains_key(&id))
                {
                    // Small fields retain canonical enum payload stores. Their
                    // recursive scalar expansion (including array cardinality)
                    // must be paid before store_payload grows event buffers.
                    // Large fields use one already-paid snapshot memcpy.
                    for field in fields {
                        if !values.contains_key(field) {
                            let cost = canonical_payload_work(&function.values[field.0 as usize])
                                .saturating_mul(16);
                            work = work.checked_sub(cost).ok_or_else(|| {
                                vec![Diagnostic::limit(
                                    Phase::Codegen,
                                    "Copy enum canonical payload budget exceeded",
                                    Some(instruction.span),
                                )]
                            })?;
                        }
                    }
                }
            }
            // Ownership snapshots stay independent. The Copy slot pool has
            // no inventory for ownership/cleanup transfers.
            if !store_protocols.is_empty()
                && !values.keys().any(|id| {
                    matches!(
                        enums::storage_shape(&function.values[id.0 as usize]),
                        Type::Package { .. }
                    )
                })
            {
                super::copy_slots::coalesce(function, &mut values, &mut work)?;
            }
            plans.insert(function.id, values);
        }
    }
    let abi = super::large_copy_abi::plan(lir, limits, &mut work)?;
    if !plans.is_empty()
        || abi.functions.values().any(|s| s.large())
        || store_protocols
            .values()
            .any(dynamic_store::Protocols::large)
    {
        work = work.checked_sub(256).ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Codegen,
                "Copy helper emission budget exceeded",
                None,
            )]
        })?;
    }
    Ok(super::large_copy_abi::Plans {
        remaining_work: work,
        values: plans,
        functions: abi.functions,
        indirect: abi.indirect,
        store_protocols,
    })
}

pub(super) fn reserve_enum_metadata(
    ty: &Type,
    work: &mut usize,
    depth: usize,
    limits: &Limits,
) -> CompileResult<()> {
    let exhausted = || {
        vec![Diagnostic::limit(
            Phase::Codegen,
            "Copy enum source metadata budget exceeded",
            None,
        )]
    };
    let containers = match ty {
        Type::Enum { variants, .. } => variants.len().saturating_mul(16),
        _ => 0,
    };
    // Every variant owns a String/Vec header even when its payload is empty.
    // Reserve all those fixed containers before inspecting names or fields.
    *work = work
        .checked_sub(16usize.saturating_add(containers))
        .ok_or_else(exhausted)?;
    if depth > limits.max_call_depth {
        return Err(exhausted());
    }
    if let Type::BorrowedFunctionPointer { life, .. } = ty {
        stored_borrows::contract(life, work)?;
    }
    let mut name = |name: &str| -> CompileResult<()> {
        *work = work.checked_sub(name.len()).ok_or_else(exhausted)?;
        Ok(())
    };
    match ty {
        Type::BorrowedStorage { storage, life } => {
            stored_borrows::shape(life, work)?;
            reserve_enum_metadata(storage, work, depth + 1, limits)?;
        }
        Type::Named(text) | Type::Unsupported(text) => name(text)?,
        Type::Record { name: text, fields } | Type::Package { name: text, fields } => {
            name(text)?;
            for (text, _) in fields {
                name(text)?;
            }
            for (_, field) in fields {
                reserve_enum_metadata(field, work, depth + 1, limits)?;
            }
        }
        Type::Enum {
            name: text,
            variants,
        } => {
            name(text)?;
            for (text, _) in variants {
                name(text)?;
            }
            for (_, fields) in variants {
                for field in fields {
                    reserve_enum_metadata(field, work, depth + 1, limits)?;
                }
            }
        }
        Type::Task {
            result,
            retained_life,
        } => {
            // Reserve complete leaf walks before selecting possible result ports.
            for row in &retained_life.ports {
                reserve_enum_metadata(&row.leaf, work, depth + 1, limits)?;
            }
            reserve_enum_metadata(result, work, depth + 1, limits)?;
            let closed = retained_life.binder_count == 0
                && retained_life.ports.is_empty()
                && retained_life.retained.is_empty()
                && retained_life.nominal_arguments.is_empty();
            let borrowed_result = if retained_life.retained.is_empty() {
                false
            } else {
                *work = work
                    .checked_sub(retained_life.retained.len())
                    .ok_or_else(exhausted)?;
                *work = work
                    .checked_sub(retained_life.ports.len().saturating_mul(2))
                    .ok_or_else(exhausted)?;
                for row in &retained_life.retained {
                    *work = work.checked_sub(row.path.len()).ok_or_else(exhausted)?;
                }
                matches!(result.as_ref(), Type::Address { mutable: false, inner } if task_port_pointee(inner))
                    && retained_life.binder_count > 0
                    && matches!(retained_life.retained.as_slice(), [row]
                        if row.path == [zil_ir::LifeTypeProjection::Result] && row.class == 0
                        && row.binder < retained_life.binder_count
                        && retained_life.ports.iter().any(|port| port.binder == row.binder && port.leaf == **result)
                        && retained_life.ports.iter().filter(|port| port.binder == row.binder).all(|port|
                            port.mode == zil_ir::StoredBorrowMode::SharedAddress))
            };
            let mut next = 0;
            let mapped = retained_life.binder_count > 0
                && (retained_life.retained.is_empty() || borrowed_result)
                && retained_life.nominal_arguments.is_empty()
                && !retained_life.ports.is_empty()
                && retained_life.ports.iter().all(|row| {
                    let dense = if row.binder == next {
                        next += 1;
                        true
                    } else {
                        row.binder < next
                    };
                    dense
                        && matches!(&row.leaf, Type::Address { mutable, inner }
                        if task_port_pointee(inner)
                        && row.mode == if *mutable {
                            zil_ir::StoredBorrowMode::ExclusiveAddress
                        } else {
                            zil_ir::StoredBorrowMode::SharedAddress
                        })
                })
                && next == retained_life.binder_count;
            if !closed && !mapped {
                return unsupported(
                    "borrowed Task native frame mapping is not implemented",
                    None,
                );
            }
        }
        Type::Array { element, .. }
        | Type::Slice { element }
        | Type::AsyncRegistration {
            environment: element,
        }
        | Type::Registration {
            environment: element,
        }
        | Type::OwnedStore { element }
        | Type::Address { inner: element, .. }
        | Type::Managed(element)
        | Type::Desc(element)
        | Type::Opaque {
            hidden: element, ..
        } => reserve_enum_metadata(element, work, depth + 1, limits)?,
        Type::Tuple(fields) => {
            for field in fields {
                reserve_enum_metadata(field, work, depth + 1, limits)?;
            }
        }
        Type::ErrorUnion { error, result } => {
            reserve_enum_metadata(error, work, depth + 1, limits)?;
            reserve_enum_metadata(result, work, depth + 1, limits)?;
        }
        Type::FunctionPointer { parameters, result }
        | Type::BorrowedFunctionPointer {
            parameters, result, ..
        }
        | Type::ForeignCallback { parameters, result } => {
            for parameter in parameters {
                reserve_enum_metadata(parameter, work, depth + 1, limits)?;
            }
            reserve_enum_metadata(result, work, depth + 1, limits)?;
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn reserve(
    ty: &Type,
    work: &mut usize,
    depth: usize,
    limits: &Limits,
) -> CompileResult<()> {
    *work = work.checked_sub(16).ok_or_else(|| {
        vec![Diagnostic::limit(
            Phase::Codegen,
            "Copy aggregate snapshot planning budget exceeded",
            None,
        )]
    })?;
    if depth > limits.max_call_depth {
        return Err(vec![Diagnostic::limit(
            Phase::Codegen,
            "Copy snapshot type depth exceeded",
            None,
        )]);
    }
    if let Type::BorrowedFunctionPointer { life, .. } = ty {
        stored_borrows::contract(life, work)?;
    }
    match ty {
        Type::BorrowedStorage { storage, life } => {
            stored_borrows::shape(life, work)?;
            reserve(storage, work, depth + 1, limits)?;
        }
        Type::Array { element, .. } => {
            reserve(element, work, depth + 1, limits)?;
            if let Type::Address {
                mutable: false,
                inner,
            } = element.as_ref()
            {
                reserve(inner, work, depth + 2, limits)?;
            }
        }
        Type::Tuple(fields) => {
            for field in fields {
                reserve(field, work, depth + 1, limits)?;
            }
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            for (_, field) in fields {
                reserve(field, work, depth + 1, limits)?;
            }
        }
        Type::Managed(payload) => reserve(payload, work, depth + 1, limits)?,
        Type::Enum { variants, .. } => {
            for (_, fields) in variants {
                for field in fields {
                    reserve(field, work, depth + 1, limits)?;
                }
            }
        }
        Type::ErrorUnion { error, result } => {
            reserve(error, work, depth + 1, limits)?;
            reserve(result, work, depth + 1, limits)?;
        }
        Type::FunctionPointer { parameters, result }
        | Type::BorrowedFunctionPointer {
            parameters, result, ..
        }
        | Type::ForeignCallback { parameters, result } => {
            for parameter in parameters {
                reserve(parameter, work, depth + 1, limits)?;
            }
            reserve(result, work, depth + 1, limits)?;
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn classify(
    ty: &Type,
    work: &mut usize,
    limits: &Limits,
) -> CompileResult<Option<Slot>> {
    if !matches!(
        enums::storage_shape(ty),
        Type::Array { .. }
            | Type::Tuple(_)
            | Type::Record { .. }
            | Type::Package { .. }
            | Type::Enum { .. }
            | Type::ErrorUnion { .. }
    ) {
        return Ok(None);
    }
    let before = *work;
    reserve(ty, work, 0, limits)?;
    if !matches!(enums::storage_shape(ty), Type::Package { .. })
        && !ty.is_copy_value()
        && !stored_borrows::pure_composite(ty)
        && !stored_borrows::shared_array_snapshot(ty)
    {
        return Ok(None);
    }
    let machine = llvm_type(ty)?;
    if unresolved_layout(&machine) {
        return Ok(None);
    }
    let (bytes, align) = layout(&machine)?;
    Ok((bytes >= MIN_BYTES).then_some(Slot {
        ty: machine,
        bytes,
        align,
        storage: None,
        work: before - *work,
    }))
}

// Named closure environments retain their existing helper convention. This
// paid machine-shape walk cannot assign them a concrete layout or change their
// ABI; unsupported large consumers still fail before publishing artifacts.
fn unresolved_layout(ty: &m::Ty) -> bool {
    match ty {
        m::Ty::Named(_) => true,
        m::Ty::Array(_, element) => unresolved_layout(element),
        m::Ty::Struct(fields) => fields.iter().any(unresolved_layout),
        _ => false,
    }
}

pub(super) fn layout(ty: &m::Ty) -> CompileResult<(u64, u32)> {
    let overflow = || {
        vec![Diagnostic::limit(
            Phase::Codegen,
            "Copy snapshot layout overflow",
            None,
        )]
    };
    let round = |bytes: u64, align: u32| {
        bytes
            .checked_add(u64::from(align) - 1)
            .map(|n| n / u64::from(align) * u64::from(align))
            .ok_or_else(overflow)
    };
    Ok(match ty {
        m::Ty::Void => (0, 1),
        m::Ty::Int(bits) => {
            let bytes = u64::from(*bits).div_ceil(8);
            (bytes, bytes.next_power_of_two().min(8) as u32)
        }
        m::Ty::F32 => (4, 4),
        m::Ty::F64 => (8, 8),
        m::Ty::Ptr => (8, 8),
        m::Ty::Array(length, element) => {
            let (bytes, align) = layout(element)?;
            (
                bytes
                    .checked_mul(*length)
                    .filter(|n| *n <= i64::MAX as u64)
                    .ok_or_else(overflow)?,
                align,
            )
        }
        m::Ty::Struct(fields) => {
            let (mut bytes, mut align) = (0u64, 1u32);
            for field in fields {
                let (size, field_align) = layout(field)?;
                align = align.max(field_align);
                bytes = round(bytes, field_align)?
                    .checked_add(size)
                    .ok_or_else(overflow)?;
            }
            (round(bytes, align)?, align)
        }
        m::Ty::Named(_) => {
            return invalid(
                "E-CODEGEN-CFG",
                "ordinary Copy snapshot has an unresolved machine type",
                None,
            );
        }
    })
}

pub(super) fn declarations() -> m::Code {
    let mut code = m::Code::new();
    for (name, second) in [
        ("llvm.memcpy.p0.p0.i64", m::Ty::Ptr),
        ("llvm.memset.p0.i64", m::Ty::Int(8)),
    ] {
        code.push(m::Event::Declare(m::Function {
            name: name.into(),
            linkage: m::Linkage::External,
            signature: m::Signature {
                result: m::Ty::Void,
                result_attrs: vec![],
                params: vec![
                    m::Param::new(m::Ty::Ptr, "destination"),
                    m::Param::new(second.clone(), "source"),
                    m::Param::new(m::Ty::Int(64), "bytes"),
                    m::Param::new(m::Ty::Int(1), "volatile"),
                ],
                variadic: false,
            },
            attrs: vec![],
            blocks: None,
        }));
        // Retain the requested optimization pipeline while keeping one bounded
        // memory operation opaque to caller SROA. Inlining this helper would
        // recreate hundreds of thousands of promoted scalar allocas at O2.
        code.push(m::Event::Begin(m::Function {
            name: if second == m::Ty::Ptr {
                "zil.copy.bytes"
            } else {
                "zil.zero.bytes"
            }
            .into(),
            linkage: m::Linkage::Internal,
            signature: m::Signature {
                result: m::Ty::Void,
                result_attrs: vec![],
                variadic: false,
                params: vec![
                    m::Param::new(m::Ty::Ptr, "destination"),
                    m::Param::new(second.clone(), "source"),
                    m::Param::new(m::Ty::Int(64), "bytes"),
                ],
            },
            attrs: vec![m::Attr::NoInline],
            blocks: None,
        }));
        code.push(m::Event::Label("entry".into()));
        code.push(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Call {
                result: m::Ty::Void,
                result_attrs: vec![],
                callee: m::global(name.into()),
                args: vec![
                    m::Arg::new(m::Ty::Ptr, m::local("destination".into())),
                    m::Arg::new(second, m::local("source".into())),
                    m::Arg::new(m::Ty::Int(64), m::local("bytes".into())),
                    m::Arg::new(m::Ty::Int(1), m::Val::Bool(false)),
                ],
                vararg: None,
            },
        )));
        code.push(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Return(None),
        )));
        code.push(m::Event::End);
    }
    code
}

pub(super) fn pointer(value: ValueId) -> m::Val {
    m::local(format!("copy.value.{}", value.0))
}

// reserve has already bounded/paid this complete type walk and its depth.
// Estimate the existing canonical store's actual repeated field operations
// without allocating aggregate_fields or expanding an array in the planner.
pub(super) fn canonical_payload_work(ty: &Type) -> usize {
    match ty {
        Type::BorrowedStorage { storage, .. } => canonical_payload_work(storage),
        Type::Unit => 0,
        Type::Bool => 2,
        Type::Array { element, length } => canonical_payload_work(element)
            .saturating_add(2)
            .saturating_mul(*length as usize),
        Type::Tuple(fields) => fields.iter().fold(0usize, |n, field| {
            n.saturating_add(canonical_payload_work(field).saturating_add(2))
        }),
        Type::Record { fields, .. } => fields.iter().fold(0usize, |n, (_, field)| {
            n.saturating_add(canonical_payload_work(field).saturating_add(2))
        }),
        Type::ErrorUnion { error, result } => canonical_payload_work(error)
            .saturating_add(canonical_payload_work(result))
            .saturating_add(8),
        Type::Enum { .. } => 8,
        _ => 1,
    }
}

impl FunctionEmitter<'_> {
    pub(super) fn copy_allocations(&mut self) {
        for (id, slot) in self.copy_values {
            if let Some(storage) = slot.storage {
                if storage.owner == *id {
                    self.text.push(m::Event::Instruction(m::Inst::new(
                        Some(format!("copy.slot.{}", storage.owner.0)),
                        m::Op::Alloca {
                            ty: m::Ty::array(storage.bytes, m::Ty::Int(8)),
                            align: storage.align,
                        },
                    )));
                }
                self.text.push(m::Event::Instruction(m::Inst::new(
                    Some(format!("copy.value.{}", id.0)),
                    m::Op::Gep {
                        ty: m::Ty::Int(8),
                        ptr: m::local(format!("copy.slot.{}", storage.owner.0)),
                        indices: vec![m::Arg::new(m::Ty::Int(64), 0)],
                    },
                )));
            } else {
                self.text.push(m::Event::Instruction(m::Inst::new(
                    Some(format!("copy.value.{}", id.0)),
                    m::Op::Alloca {
                        ty: slot.ty.clone(),
                        align: slot.align,
                    },
                )));
            }
        }
    }

    pub(super) fn copy_bytes(&mut self, destination: m::Val, source: m::Val, bytes: u64) {
        // One end is always a private value snapshot. Source addresses cannot
        // name it; closed live intervals keep simultaneous snapshots distinct
        // even when dead values share an underlying physical stack slot.
        self.text.push(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Call {
                result: m::Ty::Void,
                result_attrs: vec![],
                callee: m::global("zil.copy.bytes".into()),
                args: vec![
                    m::Arg::new(m::Ty::Ptr, destination),
                    m::Arg::new(m::Ty::Ptr, source),
                    m::Arg::new(m::Ty::Int(64), m::Val::Int(i128::from(bytes))),
                ],
                vararg: None,
            },
        )));
    }
    pub(super) fn copy_zero(&mut self, output: ValueId) {
        self.text.push(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Call {
                result: m::Ty::Void,
                result_attrs: vec![],
                callee: m::global("zil.zero.bytes".into()),
                args: vec![
                    m::Arg::new(m::Ty::Ptr, pointer(output)),
                    m::Arg::new(m::Ty::Int(8), 0),
                    m::Arg::new(
                        m::Ty::Int(64),
                        m::Val::Int(i128::from(self.copy_values[&output].bytes)),
                    ),
                ],
                vararg: None,
            },
        )));
    }
    pub(super) fn copy_read(&mut self, output: ValueId, source: m::Val) -> CompileResult<()> {
        if let Some(slot) = self.copy_values.get(&output) {
            self.copy_bytes(pointer(output), source, slot.bytes);
        } else if self.value_type(output)? != &Type::Unit {
            self.event(m::Event::Instruction(m::Inst::new(
                Some(format!("v.{}", output.0)),
                m::Op::Load {
                    ty: llvm_type(self.value_type(output)?)?,
                    ptr: source,
                    align: alignment(self.value_type(output)?, self.closures)? as u32,
                },
            )));
        }
        Ok(())
    }
    pub(super) fn copy_write(&mut self, destination: m::Val, input: ValueId) -> CompileResult<()> {
        if let Some(slot) = self.copy_values.get(&input) {
            self.copy_bytes(destination, pointer(input), slot.bytes);
        } else {
            let ty = self.value_type(input)?;
            self.event(m::Event::Instruction(m::Inst::new(
                None,
                m::Op::Store {
                    arg: m::Arg::new(
                        aggregate_field_type(ty)?,
                        if *ty == Type::Unit {
                            m::Val::Zero
                        } else {
                            m::local(format!("v.{}", input.0))
                        },
                    ),
                    ptr: destination,
                    align: alignment(ty, self.closures)? as u32,
                },
            )));
        }
        Ok(())
    }
    pub(super) fn copy_field(&mut self, aggregate: ValueId, index: m::Arg) -> m::Val {
        let address = self.temp();
        let slot = &self.copy_values[&aggregate];
        self.text.push(m::Event::Instruction(m::Inst::new(
            Some(m::name(address.clone())),
            m::Op::Gep {
                ty: slot.ty.clone(),
                ptr: pointer(aggregate),
                indices: vec![m::Arg::new(m::Ty::Int(32), 0), index],
            },
        )));
        m::val(address)
    }
    pub(super) fn copy_instruction(&mut self, instruction: &Instruction) -> CompileResult<bool> {
        let output = instruction.result;
        let output_memory = output.is_some_and(|id| self.copy_values.contains_key(&id));
        match &instruction.kind {
            InstructionKind::ReadAddress { address } if output_memory => {
                self.copy_read(result(instruction)?, m::local(format!("v.{}", address.0)))?;
            }
            InstructionKind::WriteAddress { address, value }
                if self.copy_values.contains_key(value) =>
            {
                self.copy_write(m::local(format!("v.{}", address.0)), *value)?;
            }
            InstructionKind::MakeAggregate { fields } | InstructionKind::MakePackage { fields }
                if output_memory =>
            {
                let out = result(instruction)?;
                let bytes = self.copy_values[&out].bytes;
                // Initialize padding/Unit placeholders too; no undefined bytes
                // are promoted into a source value while assembling the fields.
                self.text.push(m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::Call {
                        result: m::Ty::Void,
                        result_attrs: vec![],
                        callee: m::global("zil.zero.bytes".into()),
                        args: vec![
                            m::Arg::new(m::Ty::Ptr, pointer(out)),
                            m::Arg::new(m::Ty::Int(8), 0),
                            m::Arg::new(m::Ty::Int(64), m::Val::Int(i128::from(bytes))),
                        ],
                        vararg: None,
                    },
                )));
                for (index, field) in fields.iter().enumerate() {
                    let address = self
                        .copy_field(out, m::Arg::new(m::Ty::Int(32), m::Val::Int(index as i128)));
                    self.copy_write(address, *field)?;
                }
            }
            InstructionKind::ExtractField { aggregate, index }
                if self.copy_values.contains_key(aggregate) =>
            {
                let address = self.copy_field(
                    *aggregate,
                    m::Arg::new(m::Ty::Int(32), m::Val::Int(i128::from(*index))),
                );
                self.copy_read(result(instruction)?, address)?;
            }
            InstructionKind::UpdateField {
                aggregate,
                index,
                value,
            } if output_memory => {
                let out = result(instruction)?;
                self.copy_bytes(
                    pointer(out),
                    pointer(*aggregate),
                    self.copy_values[&out].bytes,
                );
                let address = self.copy_field(
                    out,
                    m::Arg::new(m::Ty::Int(32), m::Val::Int(i128::from(*index))),
                );
                self.copy_write(address, *value)?;
            }
            InstructionKind::ExtractIndex { aggregate, index }
                if self.copy_values.contains_key(aggregate) =>
            {
                self.copy_index(instruction, *aggregate, *index, None)?;
            }
            InstructionKind::UpdateIndex {
                aggregate,
                index,
                value,
            } if output_memory => {
                self.copy_index(instruction, *aggregate, *index, Some(*value))?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
    fn copy_index(
        &mut self,
        instruction: &Instruction,
        aggregate: ValueId,
        index: ValueId,
        update: Option<ValueId>,
    ) -> CompileResult<()> {
        let Type::Array { length, .. } = enums::storage_shape(self.value_type(aggregate)?) else {
            unreachable!("verified array")
        };
        let length = *length;
        let index_value = self.widened_index(index)?;
        let valid = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name(valid.clone())),
            m::Op::Compare {
                predicate: "ult".into(),
                ty: m::Ty::Int(64),
                left: m::val(index_value.clone()),
                right: m::val(length),
            },
        )));
        let continuation = self.continuation;
        self.continuation += 1;
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::CondBranch {
                condition: m::val(valid),
                yes: format!("index.ok.{continuation}"),
                no: "zil.trap".into(),
            },
        )));
        self.event(m::Event::Label(format!("index.ok.{continuation}")));
        let selected = if let Some(input) = update {
            let out = result(instruction)?;
            self.copy_bytes(
                pointer(out),
                pointer(aggregate),
                self.copy_values[&out].bytes,
            );
            let address = self.copy_field(out, m::Arg::new(m::Ty::Int(64), m::val(index_value)));
            self.copy_write(address, input)?;
            return Ok(());
        } else {
            self.copy_field(aggregate, m::Arg::new(m::Ty::Int(64), m::val(index_value)))
        };
        self.copy_read(result(instruction)?, selected)
    }

    // Ordinary memory instructions and LargeCopy ABI consumers are handled
    // explicitly. Any remaining snapshot consumer must stop before publishing
    // machine IR; never materialize a giant SSA bridge for an unsupported path.
    fn check_snapshot_argument(&mut self, argument: &m::Arg) {
        let m::Val::Local(name) = &argument.value else {
            return;
        };
        let Some(id) = name
            .strip_prefix("v.")
            .and_then(|n| n.parse::<u32>().ok())
            .map(ValueId)
        else {
            return;
        };
        if self
            .copy_values
            .get(&id)
            .is_some_and(|slot| slot.ty == argument.ty)
        {
            self.unsupported_large_copy = true;
        }
    }
    pub(super) fn copy_event(&mut self, event: m::Event) {
        if let m::Event::Instruction(instruction) = &event {
            match &instruction.op {
                m::Op::Store { arg, .. }
                | m::Op::Cast { arg, .. }
                | m::Op::Extract { arg, .. }
                | m::Op::Freeze(arg)
                | m::Op::Return(Some(arg)) => self.check_snapshot_argument(arg),
                m::Op::Insert { arg, field, .. } => {
                    self.check_snapshot_argument(arg);
                    self.check_snapshot_argument(field);
                }
                m::Op::Call { args, .. } => {
                    for argument in args {
                        self.check_snapshot_argument(argument);
                    }
                }
                // The source LIR has no aggregate phi; scalar_locals only adds
                // scalar phis after this emitter has finished.
                _ => {}
            }
            let output = instruction
                .result
                .as_deref()
                .and_then(|name| name.strip_prefix("v."))
                .and_then(|id| id.parse::<u32>().ok())
                .map(ValueId);
            if output.is_some_and(|id| self.copy_values.contains_key(&id)) {
                self.unsupported_large_copy = true;
                return;
            }
        }
        self.text.push(event);
    }
}

#[cfg(test)]
mod qualified_snapshot_tests {
    use super::*;

    #[test]
    fn owned_package_snapshot_is_physical_and_charges_the_complete_layout() {
        let owned = Type::Package {
            name: "Large".into(),
            fields: vec![
                ("bytes".into(), Type::Managed(Box::new(Type::ByteBuffer))),
                (
                    "values".into(),
                    Type::Array {
                        element: Box::new(Type::I32),
                        length: 4096,
                    },
                ),
            ],
        };
        assert!(!owned.is_copy_value());
        assert!(!stored_borrows::pure_composite(&owned));
        let mut work = usize::MAX;
        let slot = classify(&owned, &mut work, &Limits::default())
            .unwrap()
            .unwrap();
        assert_eq!((slot.bytes, slot.align), (16408, 8));
        assert!(slot.storage.is_none());
        let cost = usize::MAX - work;
        let mut exact = cost;
        assert!(
            classify(&owned, &mut exact, &Limits::default())
                .unwrap()
                .is_some()
        );
        assert_eq!(exact, 0);
        let errors = classify(&owned, &mut (cost - 1), &Limits::default())
            .err()
            .unwrap();
        assert_eq!(errors[0].category, zil_base::ResultCategory::ResourceLimit);
        assert!(!owned.is_copy_value());
    }

    #[test]
    fn qualified_snapshot_classification_preserves_layout_and_full_metadata_fee() {
        // Private physical planner probe only. Source nominal and borrowing
        // authentication is performed before this planner in real requests.
        let leaf = Type::BorrowedStorage {
            storage: Box::new(Type::Record {
                name: "Leaf".into(),
                fields: vec![("bits".into(), Type::U64)],
            }),
            life: zil_ir::StorageLifeShape {
                binder_count: 1,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
        };
        let qualified = Type::Array {
            element: Box::new(leaf.clone()),
            length: 1024,
        };
        assert!(!qualified.is_copy_value());
        let mut remaining = usize::MAX;
        let slot = classify(&qualified, &mut remaining, &Limits::default())
            .unwrap()
            .unwrap();
        assert_eq!((slot.bytes, slot.align), (8192, 8));
        let cost = usize::MAX - remaining;
        let mut exact = cost;
        assert!(
            classify(&qualified, &mut exact, &Limits::default())
                .unwrap()
                .is_some()
        );
        assert_eq!(exact, 0);
        let errors = classify(&qualified, &mut (cost - 1), &Limits::default())
            .err()
            .expect("N-1 must exhaust the snapshot planner");
        assert_eq!(errors[0].category, zil_base::ResultCategory::ResourceLimit);
        let mut borrowed = leaf;
        let Type::BorrowedStorage { life, .. } = &mut borrowed else {
            unreachable!()
        };
        life.slots.push(zil_ir::StorageLifeSlot {
            path: vec![],
            binder: 0,
            mode: zil_ir::StoredBorrowMode::SharedAddress,
        });
        let borrowed = Type::Array {
            element: Box::new(borrowed),
            length: 1024,
        };
        let mut available = usize::MAX;
        assert!(
            classify(&borrowed, &mut available, &Limits::default())
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn fixed_shared_address_array_snapshot_layout_and_budget_are_exact() {
        let array = Type::Array {
            element: Box::new(Type::Address {
                mutable: false,
                inner: Box::new(Type::I32),
            }),
            length: 512,
        };
        let batch = Type::BorrowedStorage {
            storage: Box::new(Type::Record {
                name: "Batch".into(),
                fields: vec![("items".into(), array.clone())],
            }),
            life: zil_ir::StorageLifeShape {
                binder_count: 1,
                slots: (0..512)
                    .map(|index| zil_ir::StorageLifeSlot {
                        path: vec![
                            zil_ir::PlaceProjection::Field(0),
                            zil_ir::PlaceProjection::Field(index),
                        ],
                        binder: 0,
                        mode: zil_ir::StoredBorrowMode::SharedAddress,
                    })
                    .collect(),
                retained: vec![],
                nominal_arguments: vec![],
            },
        };
        for actual in [array, batch] {
            assert!(!actual.is_copy_value());
            assert!(!stored_borrows::pure_composite(&actual));
            let mut left = usize::MAX;
            let slot = classify(&actual, &mut left, &Limits::default())
                .unwrap()
                .unwrap();
            assert_eq!((slot.bytes, slot.align), (4096, 8));
            let cost = usize::MAX - left;
            let mut exact = cost;
            assert!(
                classify(&actual, &mut exact, &Limits::default())
                    .unwrap()
                    .is_some()
            );
            assert_eq!(exact, 0);
            let failure = classify(&actual, &mut (cost - 1), &Limits::default())
                .err()
                .expect("N-1 must exhaust physical snapshot planning");
            assert_eq!(failure[0].category, zil_base::ResultCategory::ResourceLimit);
        }
    }
}
