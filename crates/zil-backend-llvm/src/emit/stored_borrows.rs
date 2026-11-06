//! Physical representation after immutable capability/LIR binding. Lifetime
//! metadata never enters machine fields and never supplies an address/loan.
use super::*;
use zil_base::Limits;
use zil_ir::{
    PlaceProjection, StorageLifeContract, StorageLifeShape, StoredBorrowLoad, TypedModule,
};

fn exhausted() -> Vec<Diagnostic> {
    vec![Diagnostic::limit(
        Phase::Codegen,
        "stored physical representation budget exceeded",
        None,
    )]
}
fn pay(work: &mut usize, n: usize) -> CompileResult<()> {
    *work = work.checked_sub(n).ok_or_else(exhausted)?;
    Ok(())
}
pub(super) fn shape(life: &StorageLifeShape, work: &mut usize) -> CompileResult<()> {
    pay(
        work,
        life.slots
            .len()
            .saturating_add(life.retained.len())
            .saturating_add(life.nominal_arguments.len())
            .saturating_add(1),
    )?;
    for row in &life.slots {
        pay(work, row.path.len().saturating_add(2))?;
    }
    for row in &life.retained {
        pay(work, row.path.len().saturating_add(2))?;
    }
    for row in &life.nominal_arguments {
        pay(work, row.path.len().saturating_add(4))?;
    }
    Ok(())
}
pub(super) fn contract(life: &StorageLifeContract, work: &mut usize) -> CompileResult<()> {
    // A public successor must update this visitor instead of silently adding
    // unmetered semantic signature metadata.
    let StorageLifeContract {
        fixed_binder_count: _,
        invocation_binder_count: _,
        bindings: _,
        classes: _,
    } = life;
    pay(
        work,
        life.bindings
            .len()
            .saturating_add(life.classes.len())
            .saturating_mul(16)
            .saturating_add(32),
    )?;
    for row in &life.bindings {
        pay(work, row.position.path.len().saturating_mul(8))?;
    }
    for row in &life.classes {
        pay(work, row.position.path.len().saturating_mul(8))?;
    }
    Ok(())
}
fn type_work(t: &Type, work: &mut usize, limits: &Limits, depth: usize) -> CompileResult<()> {
    pay(work, 8)?;
    if depth > limits.max_call_depth {
        return Err(exhausted());
    }
    match t {
        Type::BorrowedStorage { storage, life } => {
            shape(life, work)?;
            type_work(storage, work, limits, depth + 1)?;
        }
        Type::Record { name, fields } | Type::Package { name, fields } => {
            pay(work, name.len().saturating_add(fields.len()))?;
            for (name, t) in fields {
                pay(work, name.len())?;
                type_work(t, work, limits, depth + 1)?;
            }
        }
        Type::Tuple(fields) => {
            pay(work, fields.len())?;
            for t in fields {
                type_work(t, work, limits, depth + 1)?;
            }
        }
        Type::Enum { name, variants } => {
            pay(work, name.len().saturating_add(variants.len()))?;
            for (name, fs) in variants {
                pay(work, name.len().saturating_add(fs.len()))?;
                for t in fs {
                    type_work(t, work, limits, depth + 1)?;
                }
            }
        }
        Type::Address { inner, .. }
        | Type::Managed(inner)
        | Type::Desc(inner)
        | Type::Array { element: inner, .. }
        | Type::Slice { element: inner }
        | Type::OwnedStore { element: inner }
        | Type::Registration { environment: inner }
        | Type::AsyncRegistration { environment: inner }
        | Type::Opaque { hidden: inner, .. } => type_work(inner, work, limits, depth + 1)?,
        Type::ErrorUnion { error, result } => {
            type_work(error, work, limits, depth + 1)?;
            type_work(result, work, limits, depth + 1)?;
        }
        Type::BorrowedFunctionPointer {
            parameters,
            result,
            life,
        } => {
            contract(life, work)?;
            pay(work, parameters.len())?;
            for t in parameters {
                type_work(t, work, limits, depth + 1)?;
            }
            type_work(result, work, limits, depth + 1)?;
        }
        Type::FunctionPointer { parameters, result }
        | Type::ForeignCallback { parameters, result } => {
            pay(work, parameters.len())?;
            for t in parameters {
                type_work(t, work, limits, depth + 1)?;
            }
            type_work(result, work, limits, depth + 1)?;
        }
        Type::Task {
            result,
            retained_life,
        } => {
            pay(
                work,
                retained_life
                    .ports
                    .len()
                    .saturating_add(retained_life.retained.len())
                    .saturating_add(retained_life.nominal_arguments.len())
                    .saturating_add(1),
            )?;
            for port in &retained_life.ports {
                pay(work, 2)?;
                type_work(&port.leaf, work, limits, depth + 1)?;
            }
            for row in &retained_life.retained {
                pay(work, row.path.len().saturating_add(2))?;
            }
            for row in &retained_life.nominal_arguments {
                pay(work, row.path.len().saturating_add(4))?;
            }
            type_work(result, work, limits, depth + 1)?;
        }
        Type::Named(text) | Type::Unsupported(text) => pay(work, text.len())?,
        Type::Dyn { .. }
        | Type::Unit
        | Type::Bool
        | Type::I32
        | Type::I64
        | Type::U64
        | Type::Usize
        | Type::F32
        | Type::F64
        | Type::U8
        | Type::U16
        | Type::ByteBuffer
        | Type::ForeignHandle
        | Type::SyncCell(_)
        | Type::TaskI32
        | Type::CWideString
        | Type::Closure(_) => {}
    }
    Ok(())
}

/// Reserve full new callable metadata before raw LIR equality or any retained
/// source-type clone. ABI erasure is physical only; no relation is flattened.
pub(super) fn reserve_callable_metadata(
    module: &TypedModule,
    limits: &Limits,
    work: &mut usize,
) -> CompileResult<()> {
    pay(
        work,
        module
            .functions
            .len()
            .saturating_add(module.closures.len())
            .saturating_add(module.resources.len()),
    )?;
    for function in &module.functions {
        if let Some(life) = &function.storage_life {
            contract(life, work)?;
        }
        if let Some(c) = &function.callable_contract {
            pay(work, c.roles.len())?;
            for role in &c.roles {
                if let Some(life) = &role.life {
                    contract(life, work)?;
                }
            }
        }
        type_work(&function.return_type, work, limits, 0)?;
        for ty in function
            .locals
            .iter()
            .map(|l| &l.ty)
            .chain(&function.values)
        {
            type_work(ty, work, limits, 0)?;
        }
    }
    for closure in &module.closures {
        pay(work, closure.captures.len())?;
        if let Some(life) = &closure.storage_life {
            contract(life, work)?;
        }
        type_work(&closure.result, work, limits, 0)?;
        for ty in closure.parameters.iter().chain(&closure.captures) {
            type_work(ty, work, limits, 0)?;
        }
    }
    for resource in &module.resources {
        type_work(&resource.payload, work, limits, 0)?;
    }
    Ok(())
}

/// Only this representation slice is currently implemented. This predicate is
/// used after Verify; it grants no source Copy/Store/capture qualification.
pub(super) fn representation_type(t: &Type) -> bool {
    matches!(t,Type::BorrowedStorage{storage,..}if matches!(storage.as_ref(),Type::Record{..}|Type::Tuple(_)))
        || matches!(t,Type::Address{inner,..}if matches!(inner.as_ref(),Type::BorrowedStorage{storage,..}if matches!(storage.as_ref(),Type::Record{..}|Type::Tuple(_))))
        || pure_composite(t)
}

/// An actual verified root pointer to pure storage. Never use this predicate
/// for the pointee's fields: pure_composite rejects nested Address nodes.
/// The complete type visit was prepaid before physical representation queries.
pub(super) fn pure_storage_address(t: &Type) -> bool {
    matches!(t, Type::Address { inner, .. } if pure_composite(inner))
}

// reserve_callable_metadata has already bounded and paid the complete type
// walk before bind_lir calls this predicate. All variant payloads are visited,
// including inactive ones; no address or owning/resource leaf enters this new
// physical-only path. Existing Record/Tuple borrow representations above keep
// their separately verified profile.
pub(super) fn pure_composite(t: &Type) -> bool {
    match t {
        Type::BorrowedStorage { storage, life } => life.slots.is_empty() && pure_composite(storage),
        Type::Tuple(fields) => fields.iter().all(pure_composite),
        Type::Record { fields, .. } => fields.iter().all(|(_, t)| pure_composite(t)),
        Type::Enum { variants, .. } => variants
            .iter()
            .all(|(_, fields)| fields.iter().all(pure_composite)),
        Type::Array { element, .. } => pure_composite(element),
        Type::Unit
        | Type::Bool
        | Type::I32
        | Type::I64
        | Type::U64
        | Type::Usize
        | Type::F32
        | Type::F64
        | Type::U8
        | Type::U16 => true,
        _ => false,
    }
}

/// Pay comparison against separately allocated public LIR from the same
/// request ledger later consumed by Copy ABI and task/registration emission.
pub(super) fn reserve_binding(
    module: &TypedModule,
    limits: &Limits,
    work: &mut usize,
) -> CompileResult<()> {
    // Both stored-profile kernels structurally reject every other instruction
    // in every block, including unreachable blocks. The only variable-length
    // instruction payloads in that admitted profile are enumerated below.
    pay(work, module.functions.len())?;
    for f in &module.functions {
        pay(
            work,
            f.name
                .len()
                .saturating_add(f.locals.len())
                .saturating_add(f.values.len())
                .saturating_add(f.blocks.len()),
        )?;
        if let Some(declaration) = &f.foreign {
            pay(
                work,
                declaration
                    .abi
                    .len()
                    .saturating_add(declaration.symbol.len())
                    .saturating_add(declaration.noescape_parameters.len())
                    .saturating_add(3),
            )?;
        }
        if let Some(c) = &f.storage_life {
            contract(c, work)?;
        }
        if let Some(c) = &f.callable_contract {
            pay(work, c.roles.len())?;
            for role in &c.roles {
                pay(
                    work,
                    role.hold
                        .len()
                        .saturating_add(role.call.len())
                        .saturating_add(role.writes.len())
                        .saturating_add(8),
                )?;
            }
            match &c.traces {
                zil_ir::CallableTraces::Finite(traces) => {
                    pay(work, traces.len().saturating_add(1))?;
                    for trace in traces {
                        pay(work, trace.calls.len().saturating_mul(2).saturating_add(1))?;
                    }
                }
                zil_ir::CallableTraces::Flow(flow) => {
                    pay(work, flow.nodes.len().saturating_add(1))?;
                    for node in &flow.nodes {
                        pay(work, node.calls.len().saturating_mul(2).saturating_add(1))?;
                        if let zil_ir::CallableFlowNext::Edges(edges) = &node.next {
                            pay(work, edges.len())?;
                        }
                    }
                }
            }
        }
        type_work(&f.return_type, work, limits, 0)?;
        for l in &f.locals {
            pay(work, l.name.len())?;
            type_work(&l.ty, work, limits, 0)?;
        }
        for t in &f.values {
            type_work(t, work, limits, 0)?;
        }
        for block in &f.blocks {
            pay(work, block.instructions.len().saturating_add(1))?;
            for i in &block.instructions {
                let n = match &i.kind {
                    InstructionKind::Call { arguments, .. }
                    | InstructionKind::ForeignCall { arguments, .. }
                    | InstructionKind::MakeAggregate { fields: arguments }
                    | InstructionKind::MakeVariant {
                        fields: arguments, ..
                    }
                    | InstructionKind::MakeManaged { fields: arguments } => arguments.len(),
                    InstructionKind::CallIndirect { arguments, .. }
                    | InstructionKind::CallClosure { arguments, .. }
                    | InstructionKind::CallClosureOnce { arguments, .. }
                    | InstructionKind::CallClosureBorrowed { arguments, .. }
                    | InstructionKind::CallClosureShared { arguments, .. }
                    | InstructionKind::CallClosureMut { arguments, .. } => {
                        arguments.len().saturating_add(1)
                    }
                    InstructionKind::MakeClosure { captures, .. } => {
                        captures.len().saturating_add(1)
                    }
                    InstructionKind::FunctionRef { .. } => 1,
                    InstructionKind::ExtractIndex { .. } => 2,
                    InstructionKind::VariantTag { .. } | InstructionKind::ReadVariant { .. } => 1,
                    InstructionKind::ExtractField { .. }
                    | InstructionKind::ReadCapture { .. }
                    | InstructionKind::BorrowCapture { .. }
                    | InstructionKind::WriteCapture { .. }
                    | InstructionKind::TakeLocal { .. }
                    | InstructionKind::TakeCapture { .. }
                    | InstructionKind::DescribeLocal { .. }
                    | InstructionKind::DescribeCapture { .. }
                    | InstructionKind::ReadDesc { .. }
                    | InstructionKind::DropManaged { .. }
                    | InstructionKind::DropClosure { .. } => 1,
                    InstructionKind::LoadStoredBorrow { path, .. }
                    | InstructionKind::StoreStoredBorrow { path, .. } => path.len(),
                    InstructionKind::Constant(Constant::WideString(s)) => s.len(),
                    InstructionKind::StaticUtf8 { bytes } => bytes.len(),
                    InstructionKind::Constant(_)
                    | InstructionKind::Load(_)
                    | InstructionKind::Store { .. }
                    | InstructionKind::StorageLive(_)
                    | InstructionKind::StorageDead(_)
                    | InstructionKind::BorrowLocal { .. }
                    | InstructionKind::BorrowLocalMut { .. }
                    | InstructionKind::ReborrowAddress { .. }
                    | InstructionKind::AddressField { .. }
                    | InstructionKind::AddressIndex { .. }
                    | InstructionKind::AddressSlice { .. }
                    | InstructionKind::ReadAddress { .. }
                    | InstructionKind::WriteAddress { .. }
                    | InstructionKind::SliceLength { .. }
                    | InstructionKind::Unary { .. }
                    | InstructionKind::Binary { .. }
                    | InstructionKind::ConvertInteger { .. }
                    | InstructionKind::ConvertNumber { .. }
                    | InstructionKind::ScopeEnter { .. }
                    | InstructionKind::ScopeExit { .. }
                    | InstructionKind::UnsafeEnter { .. }
                    | InstructionKind::UnsafeExit { .. } => 1,
                    _ => {
                        return invalid(
                            "E-CODEGEN-STORAGE-PROFILE",
                            "stored profile instruction comparison has no prepaid consumer",
                            Some(i.span),
                        );
                    }
                };
                pay(work, n.saturating_add(4))?;
            }
        }
    }
    Ok(())
}

fn operands(i: &InstructionKind) -> Vec<ValueId> {
    match i {
        InstructionKind::Call { arguments, .. }
        | InstructionKind::ForeignCall { arguments, .. }
        | InstructionKind::MakeAggregate { fields: arguments }
        | InstructionKind::MakeVariant {
            fields: arguments, ..
        }
        | InstructionKind::MakeManaged { fields: arguments }
        | InstructionKind::MakeClosure {
            captures: arguments,
            ..
        }
        | InstructionKind::CallClosureShared { arguments, .. }
        | InstructionKind::CallClosureMut { arguments, .. } => arguments.clone(),
        InstructionKind::CallIndirect { callee, arguments }
        | InstructionKind::CallClosure { callee, arguments }
        | InstructionKind::CallClosureOnce { callee, arguments }
        | InstructionKind::CallClosureBorrowed {
            callee, arguments, ..
        } => std::iter::once(*callee)
            .chain(arguments.iter().copied())
            .collect(),
        InstructionKind::ExtractField {
            aggregate: value, ..
        }
        | InstructionKind::VariantTag { value }
        | InstructionKind::ReadVariant { value, .. }
        | InstructionKind::ReadDesc { desc: value, .. }
        | InstructionKind::Store { value, .. }
        | InstructionKind::WriteCapture { value, .. }
        | InstructionKind::Unary { operand: value, .. }
        | InstructionKind::ConvertInteger { value, .. }
        | InstructionKind::ConvertNumber { value, .. } => vec![*value],
        InstructionKind::Binary { left, right, .. } => vec![*left, *right],
        InstructionKind::ExtractIndex { aggregate, index } => vec![*aggregate, *index],
        InstructionKind::ReadAddress { address }
        | InstructionKind::ReborrowAddress { address, .. }
        | InstructionKind::AddressField { address, .. }
        | InstructionKind::SliceLength { address }
        | InstructionKind::LoadStoredBorrow {
            storage: address, ..
        } => vec![*address],
        InstructionKind::WriteAddress { address, value }
        | InstructionKind::StoreStoredBorrow {
            storage: address,
            value,
            ..
        } => vec![*address, *value],
        InstructionKind::AddressIndex { address, index } => vec![*address, *index],
        InstructionKind::AddressSlice {
            address,
            start,
            end,
        } => std::iter::once(*address)
            .chain(*start)
            .chain(*end)
            .collect(),
        _ => vec![],
    }
}
pub(super) fn reserve_emission(
    lir: &LirView<'_>,
    limits: &Limits,
    snapshots: &BTreeMap<FunctionId, copy_aggregates::Plan>,
    work: &mut usize,
) -> CompileResult<()> {
    pay(work, lir.functions.len())?;
    let total_instructions = lir.functions.iter().try_fold(0usize, |n, f| {
        pay(work, f.blocks.len())?;
        f.blocks.iter().try_fold(n, |n, b| {
            n.checked_add(b.instructions.len()).ok_or_else(exhausted)
        })
    })?;
    for f in lir.functions {
        for i in f.blocks.iter().flat_map(|b| &b.instructions) {
            let count = match &i.kind {
                InstructionKind::Call { arguments, .. }
                | InstructionKind::ForeignCall { arguments, .. }
                | InstructionKind::MakeAggregate { fields: arguments }
                | InstructionKind::MakeVariant {
                    fields: arguments, ..
                }
                | InstructionKind::MakeManaged { fields: arguments }
                | InstructionKind::MakeClosure {
                    captures: arguments,
                    ..
                }
                | InstructionKind::CallClosureShared { arguments, .. }
                | InstructionKind::CallClosureMut { arguments, .. } => arguments.len(),
                InstructionKind::CallIndirect { arguments, .. }
                | InstructionKind::CallClosure { arguments, .. }
                | InstructionKind::CallClosureOnce { arguments, .. }
                | InstructionKind::CallClosureBorrowed { arguments, .. } => {
                    arguments.len().saturating_add(1)
                }
                _ => 3,
            };
            pay(work, count.saturating_add(32))?;
            if let InstructionKind::Constant(Constant::WideString(s)) = &i.kind {
                let cost = s
                    .len()
                    .checked_add(1)
                    .and_then(|n| n.checked_mul(total_instructions.saturating_add(1)))
                    .and_then(|n| n.checked_mul(16))
                    .ok_or_else(exhausted)?;
                pay(work, cost)?;
            }
            if let InstructionKind::CallClosureShared { callee, .. }
            | InstructionKind::CallClosureMut { callee, .. }
            | InstructionKind::TakeLocal { local: callee }
            | InstructionKind::DescribeLocal { local: callee }
            | InstructionKind::DropManaged { local: callee }
            | InstructionKind::DropClosure { local: callee } = &i.kind
            {
                type_work(&f.locals[callee.0 as usize].ty, work, limits, 0)?;
            }
            if let InstructionKind::TakeCapture { index }
            | InstructionKind::DescribeCapture { index }
            | InstructionKind::BorrowCapture { index, .. } = &i.kind
            {
                let definition =
                    &lir.closures[f.environment.expect("verified capture entry").0 as usize];
                type_work(&definition.captures[*index as usize], work, limits, 0)?;
            }
            if let InstructionKind::MakeVariant { fields, .. } = &i.kind
                && let Some(result) = i.result
                && !f.values[result.0 as usize].is_copy_value()
                && pure_composite(&f.values[result.0 as usize])
                && snapshots
                    .get(&f.id)
                    .is_none_or(|plan| !plan.contains_key(&result))
            {
                // Planned enum results already prepay each nonsnapshot field's
                // canonical store in copy_aggregates::plan. Only an SSA enum
                // result reaches this additional reservation.
                for field in fields {
                    pay(
                        work,
                        copy_aggregates::canonical_payload_work(&f.values[field.0 as usize])
                            .saturating_mul(16),
                    )?;
                }
            }
            for v in operands(&i.kind).into_iter().chain(i.result) {
                type_work(&f.values[v.0 as usize], work, limits, 0)?;
            }
            if let InstructionKind::MakeAggregate { fields }
            | InstructionKind::MakeManaged { fields } = &i.kind
                && let Some(v) = i.result
            {
                for _ in fields {
                    type_work(&f.values[v.0 as usize], work, limits, 0)?;
                }
            }
        }
    }
    Ok(())
}

impl FunctionEmitter<'_> {
    pub(super) fn stored_load(
        &mut self,
        i: &Instruction,
        storage: ValueId,
        path: &[PlaceProjection],
        mode: StoredBorrowLoad,
    ) -> CompileResult<()> {
        if mode != StoredBorrowLoad::CopyShared {
            return unsupported(
                "stored mutable/reborrow/description native operation is not yet implemented",
                Some(i.span),
            );
        }
        let Type::Address { inner, .. } = self.value_type(storage)? else {
            return invalid(
                "E-CODEGEN-STORAGE",
                "stored base is not an actual Address",
                Some(i.span),
            );
        };
        let Type::BorrowedStorage {
            storage: physical, ..
        } = inner.as_ref()
        else {
            return invalid(
                "E-CODEGEN-STORAGE",
                "stored base lacks its actual qualified type",
                Some(i.span),
            );
        };
        if path.is_empty() {
            return invalid(
                "E-CODEGEN-STORAGE",
                "stored physical path is empty",
                Some(i.span),
            );
        }
        let mut current = physical.as_ref();
        let mut indices = vec![m::Arg::new(m::Ty::Int(32), m::Val::Int(0))];
        for part in path {
            let PlaceProjection::Field(index) = part else {
                return unsupported(
                    "stored enum/error native path is not yet implemented",
                    Some(i.span),
                );
            };
            current = match current {
                Type::Record { fields, .. } => fields.get(*index as usize).map(|(_, t)| t),
                Type::Tuple(fs) => fs.get(*index as usize),
                _ => None,
            }
            .ok_or_else(|| {
                vec![Diagnostic::error(
                    Phase::Codegen,
                    "E-CODEGEN-STORAGE",
                    Some(i.span),
                    "stored physical path crosses non-inline storage",
                )]
            })?;
            indices.push(m::Arg::new(m::Ty::Int(32), m::Val::Int(i128::from(*index))));
        }
        if !matches!(current, Type::Address { mutable: false, .. })
            || self.value_type(result(i)?)? != current
        {
            return invalid(
                "E-CODEGEN-STORAGE",
                "stored shared leaf differs from actual result",
                Some(i.span),
            );
        }
        let root_type = llvm_type(physical)?;
        let leaf_type = llvm_type(current)?;
        let align = alignment(current, self.closures)?;
        let slot = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name(slot.clone())),
            m::Op::Gep {
                ty: root_type,
                ptr: m::local(format!("v.{}", storage.0)),
                indices,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some(format!("v.{}", result(i)?.0)),
            m::Op::Load {
                ty: leaf_type,
                ptr: m::val(slot),
                align,
            },
        )));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g4463_pure_representation_checks_every_variant_and_array_element() {
        let qualify = |storage| Type::BorrowedStorage {
            storage: Box::new(storage),
            life: StorageLifeShape {
                binder_count: 1,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
        };
        let mut sum = qualify(Type::Enum {
            name: "Choice".into(),
            variants: vec![("Empty".into(), vec![]), ("Value".into(), vec![Type::U64])],
        });
        assert!(representation_type(&sum));
        let Type::BorrowedStorage { storage, .. } = &mut sum else {
            unreachable!()
        };
        let Type::Enum { variants, .. } = storage.as_mut() else {
            unreachable!()
        };
        variants.push((
            "Inactive".into(),
            vec![Type::Address {
                mutable: false,
                inner: Box::new(Type::I32),
            }],
        ));
        assert!(!representation_type(&sum));
        let array = |element| Type::Array {
            element: Box::new(element),
            length: 2,
        };
        assert!(representation_type(&array(qualify(Type::Record {
            name: "Leaf".into(),
            fields: vec![("bits".into(), Type::U64)]
        }))));
        assert!(!representation_type(&array(Type::Managed(Box::new(
            Type::ByteBuffer
        )))));
        let mut slotted = qualify(Type::Enum {
            name: "Empty".into(),
            variants: vec![("None".into(), vec![])],
        });
        let Type::BorrowedStorage { life, .. } = &mut slotted else {
            unreachable!()
        };
        life.slots.push(zil_ir::StorageLifeSlot {
            path: vec![],
            binder: 0,
            mode: zil_ir::StoredBorrowMode::SharedAddress,
        });
        assert!(!representation_type(&slotted));
    }
    #[test]
    fn g4463_index_root_address_is_pointer_not_snapshot_or_embedded_value() {
        // Synthetic representation probe only; not source/loan authority.
        let qualify = |storage| Type::BorrowedStorage {
            storage: Box::new(storage),
            life: StorageLifeShape {
                binder_count: 1,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
        };
        let leaf = qualify(Type::Record {
            name: "Leaf".into(),
            fields: vec![("bits".into(), Type::U64)],
        });
        let pointee = Type::Array {
            element: Box::new(leaf),
            length: 4096,
        };
        let root = Type::Address {
            inner: Box::new(pointee.clone()),
            mutable: false,
        };
        assert!(pure_storage_address(&root));
        assert!(!pure_composite(&root));
        assert_eq!(llvm_type(&root).unwrap(), m::Ty::Ptr);
        assert_eq!(alignment(&root, &BTreeMap::new()).unwrap(), 8);
        let mut work = usize::MAX;
        assert!(
            copy_aggregates::classify(&root, &mut work, &Limits::default())
                .unwrap()
                .is_none(),
            "pointer itself must not become a pointee-sized snapshot"
        );
        assert!(
            copy_aggregates::classify(&pointee, &mut work, &Limits::default())
                .unwrap()
                .is_some(),
            "ReadAddress result retains the existing large snapshot"
        );
        let bad = Type::Address {
            inner: Box::new(Type::Array {
                element: Box::new(root),
                length: 2,
            }),
            mutable: false,
        };
        assert!(!pure_storage_address(&bad));
        assert!(llvm_type(&bad).is_err());
    }

    #[test]
    fn callable40_rich_relation_rows_paths_and_pointer_layout_are_metered() {
        use zil_ir::{
            LifeTypeProjection, SignatureLifeClass, SignatureLifeClassBinding, StorageBase,
            StorageBinder,
        };
        let limits = Limits::default();
        let mut growth = vec![];
        // Raw metadata growth probe only: these selectors grant no kernel
        // permission. Real verified semantic/native controls are separate.
        for (rows, path) in [(0, 0), (8, 4), (32, 32)] {
            let life = StorageLifeContract {
                fixed_binder_count: 0,
                invocation_binder_count: 1,
                bindings: vec![],
                classes: (0..rows)
                    .map(|class| SignatureLifeClassBinding {
                        position: SignatureLifeClass {
                            base: StorageBase::Parameter(0),
                            path: vec![LifeTypeProjection::Pointee; path],
                            class,
                        },
                        binder: StorageBinder::Invocation(0),
                    })
                    .collect(),
            };
            let ty = Type::BorrowedFunctionPointer {
                parameters: vec![Type::I32],
                result: Box::new(Type::I32),
                life,
            };
            let mut left = usize::MAX;
            type_work(&ty, &mut left, &limits, 0).unwrap();
            let needed = usize::MAX - left;
            let mut exact = needed;
            type_work(&ty, &mut exact, &limits, 0).unwrap();
            assert_eq!(exact, 0);
            let copied = ty.clone();
            assert_eq!(copied, ty);
            assert_eq!(llvm_type(&ty).unwrap(), m::Ty::Ptr);
            let mut short = needed - 1;
            assert_eq!(
                type_work(&ty, &mut short, &limits, 0).unwrap_err()[0].category,
                zil_base::ResultCategory::ResourceLimit
            );
            let mut fresh = needed;
            type_work(&ty, &mut fresh, &limits, 0).unwrap();
            assert_eq!(fresh, 0);
            if rows > 0 {
                let mut changed = ty.clone();
                let Type::BorrowedFunctionPointer { life, .. } = &mut changed else {
                    unreachable!()
                };
                life.classes[0].binder = StorageBinder::Fixed(0);
                assert_ne!(changed.fingerprint(), ty.fingerprint());
                assert_eq!(llvm_type(&changed).unwrap(), m::Ty::Ptr);
            }
            growth.push(needed);
            eprintln!("callable40 metadata rows={rows} path={path} Codegen N={needed}");
        }
        assert!(growth[2] > growth[1] * 8);
    }

    use zil_ir::{BlockId, BlockPlan, CandidatePlan, FunctionPlan, HoldRoot, ValueHold};
    #[allow(dead_code)]
    mod fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/language-readiness/terminal/342/stored_flow_fixture.rs"
        ));
    }
    #[test]
    fn g4463_snapshot_enum_canonical_store_is_reserved_once() {
        // Synthetic physical budgeting probe, not a verified source program.
        let mut module = fixture::module();
        module.functions.truncate(1);
        let qualify = |storage| Type::BorrowedStorage {
            storage: Box::new(storage),
            life: StorageLifeShape {
                binder_count: 1,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
        };
        let input = qualify(Type::Record {
            name: "Leaf".into(),
            fields: vec![("bits".into(), Type::U64)],
        });
        let output = qualify(Type::Enum {
            name: "Wide".into(),
            variants: vec![
                ("Small".into(), vec![input.clone()]),
                (
                    "Wide".into(),
                    vec![Type::Array {
                        element: Box::new(Type::U64),
                        length: 1024,
                    }],
                ),
            ],
        });
        let f = &mut module.functions[0];
        f.parameters.clear();
        f.locals.clear();
        f.return_type = Type::Unit;
        f.values = vec![input.clone(), output];
        f.blocks.truncate(1);
        f.blocks[0].instructions = vec![fixture::instruction(
            Some(1),
            InstructionKind::MakeVariant {
                variant: 0,
                fields: vec![ValueId(0)],
            },
        )];
        f.blocks[0].terminator = Terminator::Return(None);
        let view = LirView {
            schema_version: module.schema_version,
            module_id: module.module_id,
            verified_fingerprint: zil_base::Fingerprint::ZERO,
            target: &module.target,
            functions: &module.functions,
            closures: &[],
            resources: &[],
            layouts: std::borrow::Cow::Borrowed(&[]),
        };
        let limits = Limits::default();
        let planned = copy_aggregates::plan(&view, &limits, usize::MAX).unwrap();
        let snapshot = &planned.values[&module.functions[0].id];
        assert!(snapshot.contains_key(&ValueId(1)));
        assert!(!snapshot.contains_key(&ValueId(0)));
        let mut with_snapshot = usize::MAX;
        reserve_emission(&view, &limits, &planned.values, &mut with_snapshot).unwrap();
        let mut without_snapshot = usize::MAX;
        reserve_emission(&view, &limits, &BTreeMap::new(), &mut without_snapshot).unwrap();
        assert_eq!(
            with_snapshot - without_snapshot,
            copy_aggregates::canonical_payload_work(&input) * 16
        );
        let needed = usize::MAX - with_snapshot;
        let mut exact = needed;
        reserve_emission(&view, &limits, &planned.values, &mut exact).unwrap();
        assert_eq!(exact, 0);
        let errors =
            reserve_emission(&view, &limits, &planned.values, &mut (needed - 1)).unwrap_err();
        assert_eq!(errors[0].category, zil_base::ResultCategory::ResourceLimit);
    }

    fn verified() -> VerifiedModule {
        let module = fixture::wide_shared(64);
        let origins = vec![
            vec![
                vec![HoldRoot::Parameter(0)],
                vec![HoldRoot::Parameter(0)],
                vec![HoldRoot::Parameter(0)],
                vec![],
            ],
            vec![vec![HoldRoot::Parameter(0)]; 2],
            vec![vec![HoldRoot::Parameter(0)]; 2],
            vec![
                vec![],
                vec![HoldRoot::Local(LocalId(0))],
                vec![HoldRoot::Local(LocalId(0))],
                vec![HoldRoot::Local(LocalId(0)), HoldRoot::Local(LocalId(1))],
                vec![HoldRoot::Local(LocalId(0))],
                vec![],
            ],
            vec![vec![HoldRoot::Parameter(0)]],
        ];
        let mut plan = CandidatePlan::empty_for(&module);
        plan.functions = module
            .functions
            .iter()
            .zip(origins)
            .enumerate()
            .map(|(i, (f, rows))| FunctionPlan {
                function: f.id,
                blocks: vec![BlockPlan {
                    block: BlockId(0),
                    initialized_in: if i == 3 { vec![] } else { vec![LocalId(0)] },
                    initialized_out: vec![LocalId(0)],
                }],
                holds: rows
                    .into_iter()
                    .enumerate()
                    .map(|(i, roots)| ValueHold {
                        value: ValueId(i as u32),
                        roots,
                    })
                    .collect(),
            })
            .collect();
        zil_verify::verify_module(module.clone(), plan, &module.target, &Limits::default()).unwrap()
    }
    fn lir(v: &VerifiedModule) -> LirModule {
        let m = v.module();
        LirModule {
            schema_version: m.schema_version,
            module_id: m.module_id,
            verified_fingerprint: v.fingerprint(),
            target: m.target.clone(),
            functions: m.functions.clone(),
            closures: m.closures.clone(),
            resources: m.resources.clone(),
            layouts: vec![
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
                    ty: Type::ForeignHandle,
                    size: 8,
                    alignment: 8,
                },
                ScalarLayout {
                    ty: Type::CWideString,
                    size: 8,
                    alignment: 8,
                },
            ],
        }
    }
    #[test]
    fn stored_lir_binding_reservation_is_exact_before_separate_matching_or_forged_input() {
        let verified = verified();
        let lir = lir(&verified);
        let full = verified.limits().max_analysis_steps;
        let mut left = full;
        reserve_binding(verified.module(), verified.limits(), &mut left).unwrap();
        let needed = full - left;
        let run = |n, raw: &LirModule| -> CompileResult<usize> {
            let mut left = n;
            reserve_binding(verified.module(), verified.limits(), &mut left)?;
            bind_lir(&verified, &raw.as_view(), EntryPoint::None, &mut left)?;
            Ok(left)
        };
        assert_eq!(run(needed, &lir).unwrap(), 0);
        assert!(
            run(needed - 1, &lir)
                .unwrap_err()
                .iter()
                .all(|d| d.phase == Phase::Codegen
                    && d.category == zil_base::ResultCategory::ResourceLimit)
        );
        assert_eq!(run(needed, &lir).unwrap(), 0);
        let mut forged = lir.clone();
        forged.functions[1].storage_life = None;
        assert!(
            run(needed, &forged)
                .unwrap_err()
                .iter()
                .any(|d| d.code == "E-CODEGEN-UNBOUND-LIR")
        );
        assert!(
            run(needed - 1, &forged)
                .unwrap_err()
                .iter()
                .all(|d| d.category == zil_base::ResultCategory::ResourceLimit)
        );
        println!(
            "342 actual verified wide stored LIR binding component Codegen N={needed}; N-1/fresh and forged-input prepayment verified"
        );
    }
}
