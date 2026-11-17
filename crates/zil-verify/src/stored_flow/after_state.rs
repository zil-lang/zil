//! Prepared actual memory updates. Preparation reads the one call-before
//! occupant; publication only installs those facts, never resolves another
//! source against an already updated sibling. Unknown is explicit debt.
use super::*;
#[derive(Default)]
pub(super) struct Prepared {
    pub(super) rows:Vec<(HoldRoot,Path,Permission,bool)>,
    pub(super) pending:bool,
}
pub(super) fn publish(state:&mut Frame,prepared:Prepared,b:&mut Budget)->Check<()>{
    state.pending_external_update|=prepared.pending;
    for(base,path,permission,unique)in prepared.rows{
        let holder=match base{
            HoldRoot::Local(local)=>state.locals.get_mut(local.0 as usize),
            HoldRoot::Parameter(index)=>state.external.get_mut(index as usize),
            HoldRoot::Capture(index)=>state.captures.get_mut(index as usize),
            HoldRoot::Static=>None,
        }.ok_or_else(||err("updated caller carrier location is absent"))?;
        if unique && let Some(row)=holder.leaves.position(&path,b)?{*holder.leaves.permission_mut(row)=permission;}
        else{add_leaf(holder,&path,&permission,b)?;}
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn function(module:&TypedModule,f:&TypedFunction,callee:&TypedFunction,i:&Instruction,summary:&Summary,arguments:&[ValueId],values:&[Payload],state:&Frame,sites:&WriteSites,loans:&mut [Loan],b:&mut Budget)->Check<Prepared>{
    let before=call_results::Before::function(module,f,i,values,state,b)?;
    let mut updates = Vec::new();
    let mut pending=false;
            for (parameter, path, sources) in &summary.external_after {
                let index = *parameter as usize;
                let local = *callee.parameters.get(index)
                    .ok_or_else(|| err("external summary parameter is absent"))?;
                let ty = &callee.locals[local.0 as usize].ty;
                let captured = captured_owner::external_capture_tail(module, ty, path, b)?;
                let borrowed = matches!(ty,
                    Type::Address { mutable: true, inner } if qualified(inner).is_some() || returned_environment::closure(inner).is_some());
                if captured.is_none() && !borrowed { continue; }
                let actual = values.get(arguments.get(index).ok_or_else(||err("external summary actual argument is absent"))?.0 as usize).ok_or_else(||err("external summary input snapshot is absent"))?;
                let mut permission = Permission::default();
                for source in sources {
                    join_permission(&mut permission,
                        &before.source(source,b)?, b)?;
                }
                if !summary.complete||permission.sources.is_empty() {
                    // Bottom during the interprocedural fixed point is not an
                    // empty after-state. Preserve an explicit unproved debt.
                    pending = true;
                    continue;
                }
                b.spend(path.len().saturating_add(permission.sources.len()))?;
                b.spend(summary.external_stable.len().saturating_mul(path.len().saturating_add(1)))?;
                let stable = summary.external_stable.iter().any(|(p, row)| p == parameter && row == path);
                let retained = if stable {
                    let previous = if matches!(ty, Type::Address { .. }) {
                        select(state, &actual.address, path, b)?
                    } else {
                        let row = leaf(actual, path, b)?
                            .ok_or_else(|| err("stable environment after-row is absent"))?;
                        permission_work(row, b)?;
                        row.clone()
                    };
                    permission_work(&previous, b)?;
                    permission_work(&permission, b)?;
                    if previous != permission {
                        return Err(err("stable external row changed its actual targets or loan identity"));
                    }
                    permission
                } else {
                    retain_written_slot(&permission, i, *parameter, path, sites, loans, b)?
                };
                if let Some((capture, tail)) = captured {
                    let head = if matches!(ty, Type::Address { .. }) {
                        select(state, &actual.address, &[PlaceProjection::Field(capture)], b)?
                    } else {
                        let head = leaf(actual, &[PlaceProjection::Field(capture)], b)?
                            .ok_or_else(|| err("by-value environment lacks its actual captured pointer"))?;
                        permission_work(head, b)?;
                        head.clone()
                    };
                    if head.sources.is_empty() {
                        return Err(err("captured pointee update has no actual target"));
                    }
                    for root in &head.sources {
                        let holder = match root.base {
                            HoldRoot::Local(local) => state.locals.get(local.0 as usize),
                            HoldRoot::Parameter(index) => state.external.get(index as usize),
                            HoldRoot::Capture(index) => state.captures.get(index as usize),
                            HoldRoot::Static => None,
                        }.ok_or_else(|| err("captured pointee update target memory is absent"))?;
                        let complete = symbolic_slot_path(holder, root, tail, b)?
                            .ok_or_else(|| err("captured pointee update lacks a complete actual target directory"))?;
                        permission_work(&retained, b)?;
                        updates.push((root.base, complete, retained.clone(), head.sources.len() == 1));
                    }
                }
                if borrowed {
                    let [root] = actual.address.sources.as_slice() else {
                        return Err(err("external update needs one exact actual carrier source"));
                    };
                    if !root.leaf.is_empty() {
                        return Err(err("external update crosses a target boundary"));
                    }
                    let actual_path = if root.inline.is_empty() { path.clone() }
                        else { slot_position(&actual.address, path, b)?.inline };
                    updates.push((root.base, actual_path, retained, true));
                }
            }
    Ok(Prepared{rows:updates,pending})
}
