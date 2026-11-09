//! Consumer-specific prepayment for the two audited physical paths. Source
//! classification and every unlisted consumer retain the frozen planner fee.
use super::*;

fn exhausted() -> Vec<Diagnostic> {
    vec![Diagnostic::limit(
        Phase::Codegen,
        "snapshot physical emission budget exceeded",
        None,
    )]
}
fn pay(work: &mut usize, cost: usize) -> CompileResult<()> {
    *work = work.checked_sub(cost).ok_or_else(exhausted)?;
    Ok(())
}
fn times(a: usize, b: usize) -> CompileResult<usize> {
    a.checked_mul(b).ok_or_else(exhausted)
}

// Match the existing snapshot pool's conservative BTree search bound. This
// explicitly pays comparisons per access; fixed event/clone fees are separate.
fn lookups(work: &mut usize, entries: usize, accesses: usize) -> CompileResult<()> {
    let search = entries
        .checked_mul(2)
        .and_then(|n| n.checked_add(4))
        .ok_or_else(exhausted)?;
    pay(work, times(search, accesses)?)
}

/// One paid inventory walk returns the cost of cloning the retained machine
/// type. An Array owns one element type; a Struct owns all its actual fields.
/// Named bytes are real retained String data, never erased source names.
fn machine(ty: &m::Ty, work: &mut usize, limits: &Limits, depth: usize) -> CompileResult<()> {
    pay(work, 16)?;
    if depth > limits.max_call_depth {
        return Err(exhausted());
    }
    match ty {
        m::Ty::Array(_, element) => machine(element, work, limits, depth + 1)?,
        m::Ty::Struct(fields) => {
            pay(work, fields.len())?;
            for field in fields {
                machine(field, work, limits, depth + 1)?;
            }
        }
        m::Ty::Named(name) => pay(work, name.len())?,
        m::Ty::Void | m::Ty::Int(_) | m::Ty::F32 | m::Ty::F64 | m::Ty::Ptr => {}
    }
    Ok(())
}

pub(super) fn allocations(
    values: &Plan,
    work: &mut usize,
    limits: &Limits,
) -> CompileResult<BTreeMap<ValueId, usize>> {
    pay(work, values.len())?;
    let mut machines = BTreeMap::new();
    for (id, slot) in values {
        let before = *work;
        machine(&slot.ty, work, limits, 0)?;
        let clone = before - *work;
        // copy_allocations: one retained type clone and its fixed alloca event.
        // Pooling, if later selected, uses at most a two-node byte array instead
        // of this shape plus a GEP; 64 also reserves that fixed GEP/container.
        pay(work, clone.checked_add(64).ok_or_else(exhausted)?)?;
        lookups(work, machines.len(), 1)?;
        machines.insert(*id, clone);
    }
    Ok(machines)
}

fn retained(
    life: &zil_ir::RetainedLifeShape,
    work: &mut usize,
    limits: &Limits,
    depth: usize,
) -> CompileResult<bool> {
    pay(work, life.ports.len())?;
    pay(work, life.retained.len())?;
    pay(work, life.nominal_arguments.len())?;
    let mut complete = true;
    for port in &life.ports {
        pay(work, 16)?;
        complete &= source(&port.leaf, work, limits, depth + 1)?;
    }
    for row in &life.retained {
        pay(work, row.path.len().checked_add(16).ok_or_else(exhausted)?)?;
    }
    for row in &life.nominal_arguments {
        pay(work, row.path.len().checked_add(16).ok_or_else(exhausted)?)?;
    }
    Ok(complete)
}

/// Pure fee inventory: no Task/callable/borrow qualification or machine type
/// construction occurs here. In particular it must not call the enum metadata
/// visitor, which also enforces a retained Task representation boundary.
fn source(ty: &Type, work: &mut usize, limits: &Limits, depth: usize) -> CompileResult<bool> {
    pay(work, 16)?;
    if depth > limits.max_call_depth {
        return Err(exhausted());
    }
    let mut complete = true;
    match ty {
        Type::BorrowedStorage { storage, life } => {
            stored_borrows::shape(life, work)?;
            complete &= source(storage, work, limits, depth + 1)?;
        }
        Type::Record { name, fields } | Type::Package { name, fields } => {
            pay(work, name.len())?;
            pay(work, fields.len())?;
            for (name, field) in fields {
                pay(work, name.len())?;
                complete &= source(field, work, limits, depth + 1)?;
            }
        }
        Type::Tuple(fields) => {
            pay(work, fields.len())?;
            for field in fields {
                complete &= source(field, work, limits, depth + 1)?;
            }
        }
        Type::Enum { name, variants } => {
            pay(work, name.len())?;
            pay(work, variants.len())?;
            for (name, fields) in variants {
                pay(work, name.len())?;
                pay(work, fields.len())?;
                for field in fields {
                    complete &= source(field, work, limits, depth + 1)?;
                }
            }
        }
        Type::BorrowedFunctionPointer {
            parameters,
            result,
            life,
        } => {
            stored_borrows::contract(life, work)?;
            pay(work, parameters.len())?;
            for parameter in parameters {
                complete &= source(parameter, work, limits, depth + 1)?;
            }
            complete &= source(result, work, limits, depth + 1)?;
        }
        Type::FunctionPointer { parameters, result }
        | Type::ForeignCallback { parameters, result } => {
            pay(work, parameters.len())?;
            for parameter in parameters {
                complete &= source(parameter, work, limits, depth + 1)?;
            }
            complete &= source(result, work, limits, depth + 1)?;
        }
        Type::Task {
            result,
            retained_life,
        } => {
            complete &= retained(retained_life, work, limits, depth)?;
            complete &= source(result, work, limits, depth + 1)?;
        }
        Type::MutexGuard {
            payload,
            retained_life,
        } => {
            complete &= retained(retained_life, work, limits, depth)?;
            complete &= source(payload, work, limits, depth + 1)?;
        }
        Type::Array { element, .. }
        | Type::Slice { element }
        | Type::OwnedStore { element }
        | Type::Address { inner: element, .. }
        | Type::Managed(element)
        | Type::Desc(element)
        | Type::Opaque {
            hidden: element, ..
        }
        | Type::Mutex { payload: element }
        | Type::Registration {
            environment: element,
        }
        | Type::AsyncRegistration {
            environment: element,
        } => {
            complete &= source(element, work, limits, depth + 1)?;
        }
        Type::ErrorUnion { error, result } => {
            complete &= source(error, work, limits, depth + 1)?;
            complete &= source(result, work, limits, depth + 1)?;
        }
        Type::Named(name) | Type::Unsupported(name) => pay(work, name.len())?,
        // Closure alignment can follow a separate capture DAG. It has not been
        // audited here: retain the original fee instead of erasing that work.
        Type::Closure(_) => complete = false,
        Type::Dyn { .. }
        | Type::OwnedDyn { .. }
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
        | Type::CWideString => {}
    }
    Ok(complete)
}
fn query(ty: &Type, work: &mut usize, limits: &Limits) -> CompileResult<bool> {
    let before = *work;
    let complete = source(ty, work, limits, 0)?;
    let shape = before - *work;
    // Inventory traversal has just paid for itself. Actual llvm_type can expand
    // one ByteBuffer/Store source node to four machine nodes. Reserve that bound
    // plus two complete source walks for pointer predicates and alignment. A
    // complex pointee is therefore charged even though its ABI is one Ptr.
    if complete {
        pay(work, times(shape, 6)?)?;
    }
    Ok(complete)
}

/// Returns true only when the actual emitter takes an audited path. TakeLocal
/// also updates cleanup/partial flags and deliberately retains the old fee.
pub(super) fn instruction(
    function: &TypedFunction,
    instruction: &Instruction,
    values: &Plan,
    machines: &BTreeMap<ValueId, usize>,
    work: &mut usize,
    limits: &Limits,
) -> CompileResult<bool> {
    match &instruction.kind {
        InstructionKind::Load(_) => {
            let output = instruction.result.expect("bound Load result");
            lookups(work, values.len(), 1)?; // this planner's classification query
            if values.contains_key(&output) {
                // copy_instruction's output test and copy_read's Slot lookup.
                lookups(work, values.len(), 2)?;
                // copy_read -> one three-argument memcpy helper call.
                pay(work, 96)?;
            } else {
                let ty = &function.values[output.0 as usize];
                if !query(ty, work, limits)? {
                    return Ok(false);
                }
                // copy_instruction, copy_read, and copy_event's result check.
                lookups(work, values.len(), 3)?;
                pay(work, 48)?; // actual Load event/result/container
            }
            Ok(true)
        }
        InstructionKind::MakeAggregate { fields } | InstructionKind::MakePackage { fields } => {
            let Some(output) = instruction.result else {
                return Ok(false);
            };
            lookups(work, values.len(), 1)?; // this planner's output test
            if !values.contains_key(&output) {
                return Ok(false);
            }
            // Resolve this planning metric once, not once per emitted field.
            lookups(work, machines.len(), 1)?;
            let clone = machines[&output];
            // The emitter tests output_memory and then reads the output bytes.
            lookups(work, values.len(), 2)?;
            // One zero helper call; scan the operand inventory before walking it.
            pay(work, 96)?;
            pay(work, fields.len())?;
            for field in fields {
                // Planner classification, copy_field's destination Slot, and
                // copy_write's input Slot are three distinct tree accesses.
                lookups(work, values.len(), 3)?;
                // copy_field clones this actual destination machine type, not
                // the function's largest source/lifetime inventory.
                pay(work, clone.checked_add(64).ok_or_else(exhausted)?)?;
                if values.contains_key(field) {
                    pay(work, 96)?; // copy_write -> memcpy, no source type query
                } else {
                    if !query(&function.values[field.0 as usize], work, limits)? {
                        return Ok(false);
                    }
                    // A typed Store also runs copy_event's argument Slot check
                    // (Unit's constant placeholder needs no lookup, so this is
                    // still a conservative bound).
                    lookups(work, values.len(), 1)?;
                    pay(work, 48)?; // typed Store and its actual argument
                }
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}
