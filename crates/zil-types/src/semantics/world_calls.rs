//! Opcode, original call occurrence, fixed ABI and actual unsafe boundary must
//! agree before the common complete P/R class transfer is used.
use super::*;
use std::collections::BTreeSet;

/// Completed input proofs belong to this immutable emitted site and exact
/// original target. Runtime argument values and current classes have separate
/// equations at each reaching call.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Site {
    site: usize,
    instance: SemanticInstanceId,
    target: SemanticDefinitionId,
}

impl Site {
    pub(super) fn new(
        caller: &SemanticInstance,
        site: &SemanticSite,
        target: SemanticDefinitionId,
        actuals: &[SemanticActual],
    ) -> R<Self> {
        let (SemanticSiteAction::Call { actuals: recorded, .. }
        | SemanticSiteAction::BodyCall { actuals: recorded }) = &site.action else {
            return Err(bad("call input proof has no actual call action"));
        };
        if site.instance != caller.id || !std::ptr::eq(recorded.as_slice(), actuals) {
            return Err(bad("call input proof changes its exact world or emitted actuals"));
        }
        Ok(Self { site: site as *const SemanticSite as usize, instance: caller.id, target })
    }
}

#[derive(Default)]
pub(super) struct Proofs {
    pub(super) body: BTreeSet<Site>,
    pub(super) requirements: BTreeSet<Site>,
    pub(super) specialized_headers: BTreeSet<(Site, SemanticInstanceId)>,
}

pub(super) fn abi(
    world: &mut World<'_, '_>,
    original: &SemanticDefinition,
    target: &TypedFunction,
    site: &SemanticSite,
    instruction: &InstructionKind,
    unsafe_regions: &[u32],
) -> R<()> {
    match instruction {
        InstructionKind::Call { .. }
            if target.call_kind == CallKind::Direct && target.foreign.is_none() =>
        {
            Ok(())
        }
        InstructionKind::SuspendCall { .. }
            if target.call_kind == CallKind::Suspend && target.foreign.is_none() =>
        {
            Ok(())
        }
        InstructionKind::ForeignCall { region, .. }
            if target.call_kind == CallKind::Direct && target.foreign.is_some() =>
        {
            world.pay(unsafe_regions.len())?;
            if !unsafe_regions.contains(region) {
                return Err(bad(
                    "actual foreign call has no active actual unsafe region",
                ));
            }
            if !super::super::original::protocol::unsafe_call(
                original,
                site.recipe,
                world.limits,
                world.left,
            )? {
                return Err(bad(
                    "actual foreign call is not its original unsafe operation",
                ));
            }
            Ok(())
        }
        _ => {
            world.pay(target.name.len().saturating_add(1))?;
            let mut diagnostic =
                bad("actual call opcode changes the callee's fixed original ABI convention");
            diagnostic.notes.push(format!("caller original definition {:?}; actual point {:?}; target {:?} {}; actual target convention {:?}; foreign {}; call opcode {:?}", original.id, world.current_point, target.id, target.name, target.call_kind, target.foreign.is_some(), std::mem::discriminant(instruction)));
            Err(diagnostic)
        }
    }
}

pub(super) fn missing(
    world: &mut World<'_, '_>,
    function: &TypedFunction,
    block: BlockId,
    index: usize,
    kind: &InstructionKind,
) -> R<Diagnostic> {
    world.pay(function.name.len().saturating_add(512))?;
    let mut diagnostic = pending("actual operation requires its complete original class transfer");
    diagnostic.notes.push(format!(
        "actual operation: function {:?} {} block {:?} instruction {} opcode {:?}",
        function.id,
        function.name,
        block,
        index,
        std::mem::discriminant(kind)
    ));
    Ok(diagnostic)
}
