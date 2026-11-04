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
fn shape(life: &StorageLifeShape, work: &mut usize) -> CompileResult<()> {
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
fn contract(life: &StorageLifeContract, work: &mut usize) -> CompileResult<()> {
    pay(work, life.bindings.len().saturating_add(2))?;
    for row in &life.bindings {
        pay(work, row.position.path.len().saturating_add(2))?;
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
        | Type::Slice { element: inner } => type_work(inner, work, limits, depth + 1)?,
        Type::ErrorUnion { error, result } => {
            type_work(error, work, limits, depth + 1)?;
            type_work(result, work, limits, depth + 1)?;
        }
        Type::FunctionPointer { parameters, result }
        | Type::BorrowedFunctionPointer {
            parameters, result, ..
        } => {
            pay(work, parameters.len())?;
            for t in parameters {
                type_work(t, work, limits, depth + 1)?;
            }
            type_work(result, work, limits, depth + 1)?;
        }
        _ => {}
    }
    Ok(())
}

/// Only this representation slice is currently implemented. This predicate is
/// used after Verify; it grants no source Copy/Store/capture qualification.
pub(super) fn representation_type(t: &Type) -> bool {
    matches!(t,Type::BorrowedStorage{storage,..}if matches!(storage.as_ref(),Type::Record{..}|Type::Tuple(_)))
        || matches!(t,Type::Address{inner,..}if matches!(inner.as_ref(),Type::BorrowedStorage{storage,..}if matches!(storage.as_ref(),Type::Record{..}|Type::Tuple(_))))
}

/// Pay comparison against separately allocated public LIR from the same
/// request ledger later consumed by Copy ABI and task/registration emission.
pub(super) fn reserve_binding(
    module: &TypedModule,
    limits: &Limits,
    work: &mut usize,
) -> CompileResult<()> {
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
        if let Some(c) = &f.storage_life {
            contract(c, work)?;
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
                    | InstructionKind::MakeAggregate { fields: arguments } => arguments.len(),
                    InstructionKind::LoadStoredBorrow { path, .. }
                    | InstructionKind::StoreStoredBorrow { path, .. } => path.len(),
                    InstructionKind::Constant(Constant::WideString(s)) => s.len(),
                    _ => 1,
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
        | InstructionKind::MakeAggregate { fields: arguments } => arguments.clone(),
        InstructionKind::Store { value, .. }
        | InstructionKind::Unary { operand: value, .. }
        | InstructionKind::ConvertInteger { value, .. }
        | InstructionKind::ConvertNumber { value, .. } => vec![*value],
        InstructionKind::Binary { left, right, .. } => vec![*left, *right],
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
    work: &mut usize,
) -> CompileResult<()> {
    for f in lir.functions {
        for i in f.blocks.iter().flat_map(|b| &b.instructions) {
            let count = match &i.kind {
                InstructionKind::Call { arguments, .. }
                | InstructionKind::MakeAggregate { fields: arguments } => arguments.len(),
                _ => 3,
            };
            pay(work, count.saturating_add(32))?;
            for v in operands(&i.kind).into_iter().chain(i.result) {
                type_work(&f.values[v.0 as usize], work, limits, 0)?;
            }
            if let InstructionKind::MakeAggregate { fields } = &i.kind
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
