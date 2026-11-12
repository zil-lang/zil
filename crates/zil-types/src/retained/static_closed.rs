//! Deferred compiler obligations retain actual environment identities. A
//! proposed zero domain is never itself a ClosedEnvironment proof.
use super::*;

pub(super) fn potential(
    definitions: &[closures::Pending],
    root: ClosureId,
    left: &mut usize,
    span: Span,
) -> R<bool> {
    let mut pending = vec![root];
    let mut seen = BTreeSet::new();
    let mut has_constant_candidate = false;
    while let Some(id) = pending.pop() {
        spend(left, seen.len().saturating_add(1), span)?;
        if !seen.insert(id) { continue; }
        let d = definitions.get(id.0 as usize)
            .ok_or_else(|| unsupported(span, "deferred closed environment identity is absent"))?;
        spend(left, d.captures.len(), span)?;
        for capture in &d.captures {
            if capture.borrowed || matches!(capture.ty,Type::Managed(_)) && !capture.moved {
                return Ok(false);
            }
            match &capture.ty {
                Type::Closure(child) => pending.push(*child),
                Type::Address {mutable:false,inner}
                    if capture.moved && matches!(inner.as_ref(),Type::Slice{element}if **element==Type::U8) => {
                    has_constant_candidate=true;
                }
                Type::Managed(_) | Type::Package{..} | Type::OwnedDyn{..} => {}
                ty if ty.is_copy_value() => {}
                _ => return Ok(false),
            }
        }
    }
    Ok(has_constant_candidate)
}

pub(super) fn check(
    module: &TypedModule,
    obligations: &BTreeMap<ClosureId,Span>,
    left: &mut usize,
    limits: &Limits,
) -> R<()> {
    if obligations.is_empty() { return Ok(()); }
    let proven = sources::prove_static_constructors(module,left,limits)?;
    let mut pending = obligations.iter().map(|(id,span)|(*id,*span)).collect::<Vec<_>>();
    let mut seen = BTreeSet::new();
    while let Some((id,span)) = pending.pop() {
        spend(left, seen.len().saturating_add(1), span)?;
        if !seen.insert(id) { continue; }
        let d = module.closures.get(id.0 as usize)
            .ok_or_else(|| unsupported(span,"deferred actual ClosureDefinition is absent"))?;
        let life = d.storage_life.as_ref().ok_or_else(||
            unsupported(span,"ClosedEnvironment requires a complete actual domain; None is unknown"))?;
        if life.fixed_binder_count != 0 {
            return Err(error("E-CLOSED-ENVIRONMENT",span,"actual environment retains free input classes"));
        }
        for (index,ty) in d.captures.iter().enumerate() {
            inference::charge_type_snapshot(ty,span,limits,left,1)?;
            match ty {
                Type::Closure(child) => pending.push((*child,span)),
                Type::Address{mutable:false,..} => {
                    let position=StoragePosition{base:StorageBase::Capture(index as u32),path:Vec::new()};
                    if !life.static_bindings.contains(&position)
                        || !proven.contains(&(id,position)) {
                        return Err(error("E-CLOSED-ENVIRONMENT",span,"constant environment field lacks complete actual constructor and write proofs"));
                    }
                }
                _ if local_scopes::closed(ty,module,left,span,0,limits.max_call_depth)? => {}
                _ => return Err(error("E-CLOSED-ENVIRONMENT",span,"actual environment retains an unproved borrowed or nominal domain")),
            }
        }
    }
    Ok(())
}
