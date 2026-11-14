//! Opcode, original call occurrence, fixed ABI and actual unsafe boundary must
//! agree before the common complete P/R class transfer is used.
use super::*;

pub(super) fn abi(
    world: &mut World<'_, '_>,
    original: &SemanticDefinition,
    target: &TypedFunction,
    sites: &[&SemanticSite],
    block: BlockId,
    index: usize,
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
            let mut source = None;
            for site in world.point_sites(sites, block, index)? {
                if matches!(
                    site.action,
                    SemanticSiteAction::Call { .. } | SemanticSiteAction::BodyCall { .. }
                ) {
                    if source.replace(site.recipe).is_some() {
                        return Err(bad(
                            "actual foreign call repeats its original operation seal",
                        ));
                    }
                }
            }
            let source = source
                .ok_or_else(|| bad("actual foreign call has no original operation occurrence"))?;
            if !super::super::original::protocol::unsafe_call(
                original,
                source,
                world.limits,
                world.left,
            )? {
                return Err(bad(
                    "actual foreign call is not its original unsafe operation",
                ));
            }
            Ok(())
        }
        _ => Err(bad(
            "actual call opcode changes the callee's fixed original ABI convention",
        )),
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
