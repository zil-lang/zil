//! Sealed language calls are selected by their real instruction, never a
//! fabricated function definition. Original schemes remain in the caller Gamma.
use super::*;
use crate::semantics::original::{IntrinsicCall, IntrinsicKind};

pub(super) fn evaluate(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    original: &SemanticDefinition,
    sites: &[&SemanticSite],
    block: BlockId,
    index: usize,
    instruction: &InstructionKind,
    ty: Option<&Type>,
    values: &[Option<SemanticTypeValue>],
    implicit: &BTreeMap<u32, SemanticClassExpr>,
) -> R<Option<SemanticTypeValue>> {
    let mut selected: Option<IntrinsicCall> = None;
    for site in world.point_sites(sites, block, index)? {
        let SemanticSiteAction::BodyCall { actuals } = &site.action else {
            continue;
        };
        let ledger = world.ledger;
        let proof = ledger.original.borrow();
        let proof = proof
            .as_ref()
            .ok_or_else(|| bad("intrinsic has no checked original Gamma"))?;
        let Some(call) = proof.intrinsics.get(&(original.id, site.recipe)) else {
            continue;
        };
        if !matches!(
            world.original_recipe(original, site.recipe)?.kind,
            SemanticSiteRecipeKind::OriginalCall
        ) {
            return Err(bad("sealed call changes its original operation recipe"));
        }
        if selected.is_some() || !actuals.is_empty() {
            return Err(bad(
                "sealed call repeats a scheme or fabricates mixed actuals",
            ));
        }
        world.pay(call.parameters.len().saturating_add(1))?;
        for parameter in &call.parameters {
            crate::dyn_metadata::semantic_operand_snapshot(parameter, world.limits, world.left, 1)?;
        }
        crate::dyn_metadata::semantic_operand_snapshot(&call.result, world.limits, world.left, 1)?;
        selected = Some(call.clone());
    }
    let Some(call) = selected else {
        return Ok(None);
    };
    world.pay(call.parameters.len())?;
    let operands = match (&call.kind, instruction) {
        (IntrinsicKind::MutexCreate, InstructionKind::MutexCreate { value }) => vec![*value],
        (IntrinsicKind::MutexLock, InstructionKind::MutexLock { mutex }) => vec![*mutex],
        (IntrinsicKind::MutexValue, InstructionKind::MutexValue { guard })
        | (IntrinsicKind::MutexUnlock, InstructionKind::MutexUnlock { guard }) => vec![*guard],
        (IntrinsicKind::BytesNew, InstructionKind::ByteBufferNew) => Vec::new(),
        (IntrinsicKind::BytesReserve, InstructionKind::ByteBufferReserve { buffer, capacity }) => {
            vec![*buffer, *capacity]
        }
        (IntrinsicKind::BytesTruncate, InstructionKind::ByteBufferTruncate { buffer, length }) => {
            vec![*buffer, *length]
        }
        (IntrinsicKind::BytesPush, InstructionKind::ByteBufferPush { buffer, value }) => {
            vec![*buffer, *value]
        }
        (IntrinsicKind::BytesLen, InstructionKind::ByteBufferLen { buffer })
        | (IntrinsicKind::BytesCapacity, InstructionKind::ByteBufferCapacity { buffer }) => {
            vec![*buffer]
        }
        (
            IntrinsicKind::BytesBorrow { mutable: expected },
            InstructionKind::ByteBufferBorrow { buffer, mutable },
        ) if expected == mutable => vec![*buffer],
        (IntrinsicKind::Length, InstructionKind::SliceLength { address }) => vec![*address],
        (
            IntrinsicKind::Numeric { target: expected },
            InstructionKind::ConvertInteger { value, target }
            | InstructionKind::ConvertNumber { value, target },
        ) if expected == target => vec![*value],
        (
            IntrinsicKind::ThreadScope {
                worker: expected_worker,
                body: expected_body,
            },
            InstructionKind::WithScopedThread {
                worker,
                argument,
                body,
            },
        ) => {
            world.pay(world.program.instances.len())?;
            let mut workers = world.program.instances.iter().filter(|candidate| matches!(candidate.target,
                SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(actual)) if actual == *worker));
            let actual_worker = workers
                .next()
                .ok_or_else(|| bad("thread intrinsic worker has no real declaration world"))?;
            if actual_worker.definition != *expected_worker || workers.next().is_some() {
                return Err(bad("thread intrinsic selects another worker declaration"));
            }
            let body_value = world.get(values, *body)?;
            let Type::Closure(closure) = World::type_of(&body_value)? else {
                return Err(bad("thread intrinsic body operand is not its real closure"));
            };
            let (_, body_world) = world.child(*closure)?;
            if body_world.definition != *expected_body {
                return Err(bad(
                    "thread intrinsic selects another closure body declaration",
                ));
            }
            vec![*argument, *body]
        }
        _ => {
            return Err(bad(
                "actual opcode differs from the selected original sealed intrinsic",
            ))
        }
    };
    if operands.len() != call.parameters.len() {
        return Err(bad(
            "sealed intrinsic omits an original receiver or argument",
        ));
    }
    for (operand, parameter) in operands.iter().zip(&call.parameters) {
        let actual = world.get(values, *operand)?;
        let expected = world.ty(instance.id, parameter, World::type_of(&actual)?)?;
        world.same(instance.id, &actual, &expected, implicit)?;
    }
    let ty = ty.ok_or_else(|| bad("sealed intrinsic omits its actual result type"))?;
    let result = world.ty(instance.id, &call.result, ty)?;
    if World::type_of(&result)? != ty {
        return Err(bad(
            "sealed intrinsic changes its complete actual result type",
        ));
    }
    Ok(Some(result))
}
