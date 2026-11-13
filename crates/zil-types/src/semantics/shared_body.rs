//! Complete occurrence flow for shared scalar capture heads. These identities
//! describe class occurrences only; they never grant an address permission or
//! stand in for either kernel's actual target and loan proof.
use crate::*;

fn bad(message: &str) -> Diagnostic {
    error("E-SEMANTIC-FLOW", Span::default(), message)
}
fn pending(message: &str) -> Diagnostic {
    Diagnostic::unsupported(Phase::Types, message, None)
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Origin {
    Scalar,
    LocalHead(LocalId),
    CaptureHead(u32),
    Closure(ClosureId),
}
fn pay(left: &mut usize, n: usize) -> R<()> {
    *left = left.checked_sub(n).ok_or_else(|| {
        Diagnostic::limit(Phase::Types, "semantic shared body work exceeded", None)
    })?;
    Ok(())
}
pub(super) fn check(function: &TypedFunction, module: &TypedModule, left: &mut usize) -> R<()> {
    pay(
        left,
        function.values.len().saturating_add(function.locals.len()),
    )?;
    let has_occurrences = function
        .values
        .iter()
        .chain(function.locals.iter().map(|l| &l.ty))
        .any(|ty| matches!(ty, Type::Address { .. } | Type::Closure(_)));
    if !has_occurrences {
        return Ok(());
    }
    if function.blocks.len() != 1 {
        return Err(pending(
            "shared capture CFG requires complete reaching class equations",
        ));
    }
    let mut locals = vec![None; function.locals.len()];
    let mut values = vec![None; function.values.len()];
    for parameter in &function.parameters {
        let local = function
            .locals
            .get(parameter.0 as usize)
            .ok_or_else(|| bad("parameter is outside local inventory"))?;
        if !local.ty.is_b0_scalar() {
            return Err(pending(
                "shared capture input requires its full signature class map",
            ));
        }
        locals[parameter.0 as usize] = Some(Origin::Scalar);
    }
    let value = |values: &[Option<Origin>], id: ValueId| {
        values
            .get(id.0 as usize)
            .and_then(|r| *r)
            .ok_or_else(|| bad("actual value has no reaching class occurrence"))
    };
    let get = |locals: &[Option<Origin>], id: LocalId| {
        locals
            .get(id.0 as usize)
            .and_then(|r| *r)
            .ok_or_else(|| bad("actual local has no initialized class occupant"))
    };
    for instruction in &function.blocks[0].instructions {
        pay(left, 1)?;
        let origin = match &instruction.kind {
            InstructionKind::Load(local) => Some(get(&locals, *local)?),
            InstructionKind::TakeLocal { local } => {
                let result = get(&locals, *local)?;
                locals[local.0 as usize] = None;
                Some(result)
            }
            InstructionKind::Store {
                local,
                value: source,
            } => {
                *locals
                    .get_mut(local.0 as usize)
                    .ok_or_else(|| bad("store has no actual local"))? =
                    Some(value(&values, *source)?);
                None
            }
            InstructionKind::StorageLive(local) | InstructionKind::StorageDead(local) => {
                *locals
                    .get_mut(local.0 as usize)
                    .ok_or_else(|| bad("activation has no actual local"))? = None;
                None
            }
            InstructionKind::BorrowLocal { local } => {
                if get(&locals, *local)? != Origin::Scalar {
                    return Err(pending(
                        "capture of a compound head needs its complete original class projection",
                    ));
                }
                Some(Origin::LocalHead(*local))
            }
            InstructionKind::ReadCapture { index } => {
                pay(left, module.closures.len())?;
                let d = function
                    .environment
                    .and_then(|id| module.closures.iter().find(|d| d.id == id))
                    .ok_or_else(|| bad("capture read has no real closure entry"))?;
                match d.captures.get(*index as usize) {
                    Some(Type::Address {
                        mutable: false,
                        inner,
                    }) if inner.is_b0_scalar() => Some(Origin::CaptureHead(*index)),
                    Some(ty) if ty.is_b0_scalar() => Some(Origin::Scalar),
                    _ => {
                        return Err(pending(
                            "capture read needs its full actual compound class map",
                        ));
                    }
                }
            }
            InstructionKind::ReborrowAddress {
                address,
                mutable: false,
            } => match value(&values, *address)? {
                origin @ (Origin::LocalHead(_) | Origin::CaptureHead(_)) => Some(origin),
                _ => return Err(bad("shared reborrow loses its actual head occurrence")),
            },
            InstructionKind::ReadAddress { address } => match value(&values, *address)? {
                Origin::LocalHead(_) | Origin::CaptureHead(_) => Some(Origin::Scalar),
                _ => return Err(bad("actual read has no head class occurrence")),
            },
            InstructionKind::MakeClosure { closure, captures } => {
                pay(left, module.closures.len().saturating_add(captures.len()))?;
                let d = module
                    .closures
                    .iter()
                    .find(|d| d.id == *closure)
                    .ok_or_else(|| bad("constructor has no actual closure definition"))?;
                if captures.len() != d.captures.len() {
                    return Err(bad("constructor omits actual captured class occurrences"));
                }
                for (source, ty) in captures.iter().zip(&d.captures) {
                    let origin = value(&values, *source)?;
                    let valid = match ty {
                        Type::Address {
                            mutable: false,
                            inner,
                        } if inner.is_b0_scalar() => {
                            matches!(origin, Origin::LocalHead(_) | Origin::CaptureHead(_))
                        }
                        ty if ty.is_b0_scalar() => origin == Origin::Scalar,
                        _ => false,
                    };
                    if !valid {
                        return Err(pending(
                            "constructor class source requires the complete compound environment seal",
                        ));
                    }
                }
                Some(Origin::Closure(*closure))
            }
            InstructionKind::CallClosure { callee, .. }
            | InstructionKind::CallClosureOnce { callee, .. }
            | InstructionKind::CallClosureBorrowed { callee, .. } => {
                if !matches!(value(&values, *callee)?, Origin::Closure(_)) {
                    return Err(bad("call has no reaching actual environment occupant"));
                }
                Some(Origin::Scalar)
            }
            InstructionKind::CallClosureShared { callee, .. }
            | InstructionKind::CallClosureMut { callee, .. } => {
                if !matches!(get(&locals, *callee)?, Origin::Closure(_)) {
                    return Err(bad(
                        "local call has no reaching actual environment occupant",
                    ));
                }
                Some(Origin::Scalar)
            }
            InstructionKind::WriteCapture { .. } | InstructionKind::TakeCapture { .. } => {
                return Err(pending(
                    "capture mutation/transfer requires the complete class update proof",
                ));
            }
            _ => instruction
                .result
                .and_then(|id| function.values.get(id.0 as usize))
                .filter(|ty| ty.is_b0_scalar())
                .map(|_| Origin::Scalar),
        };
        if let Some(result) = instruction.result {
            let origin = origin.ok_or_else(|| {
                pending("actual producer needs its full class occurrence transfer")
            })?;
            let ty = function
                .values
                .get(result.0 as usize)
                .ok_or_else(|| bad("actual result is outside its type inventory"))?;
            let valid = match origin {
                Origin::Scalar => ty.is_b0_scalar(),
                Origin::LocalHead(local) => {
                    matches!(ty, Type::Address { mutable: false, inner } if function.locals.get(local.0 as usize).is_some_and(|l| l.ty == **inner))
                }
                Origin::CaptureHead(index) => {
                    function
                        .environment
                        .and_then(|id| module.closures.iter().find(|d| d.id == id))
                        .and_then(|d| d.captures.get(index as usize))
                        == Some(ty)
                }
                Origin::Closure(id) => *ty == Type::Closure(id),
            };
            if !valid {
                return Err(bad(
                    "actual SSA type changes its reaching whole class occurrence",
                ));
            }
            *values
                .get_mut(result.0 as usize)
                .ok_or_else(|| bad("actual result has no occurrence slot"))? = Some(origin);
        }
    }
    Ok(())
}
