//! Exact original cleanup occurrences, mapped after all ordinary worlds/sites
//! have been checked. Original Gamma premises and actual frame proofs stay
//! separate: no concrete caller supplies an abstract SyncDrop predicate.
use super::*;

impl Check<'_, '_> {
    pub(super) fn cleanup_occurrences(&self, b: &mut Budget) -> CompileResult<()> {
        for (ordinal, pending) in self.original.pending_cleanups().iter().enumerate() {
            b.spend(1, WORK)?;
            let mut instances = 0usize;
            for instance in &self.program.instances {
                b.spend(1, WORK)?;
                if instance.definition != pending.definition { continue; }
                let SemanticInstanceTarget::Function(target) = &instance.target else {
                    return Err(fail("original cleanup occurrence has no complete actual function instance"));
                };
                let function = self.inventory.function(target, b)?;
                let expected = self.original.cleanup_value(ordinal, instance, self.program, self.inventory, b)?;
                let local = self.anchored_local_at(instance, function, pending.anchor, b, 0)?;
                let declaration = function.locals.get(local.0 as usize).filter(|declaration| declaration.id == local)
                    .ok_or_else(|| fail("original cleanup occurrence loses its exact actual local"))?;
                if !geometry::same_type(&expected.ty, &declaration.ty, b)? {
                    return Err(fail("original cleanup occurrence changes its complete instantiated subject type"));
                }
                // A Suspend declaration may dispose of its actual closed
                // environment through its frame. This does not establish
                // SyncDrop for the original formal or grant a Direct drop.
                if function.call_kind == CallKind::Suspend
                    && self.structural_actual(&expected.ty, original::Predicate::SyncDrop, b, 0)?
                {
                    crate::tasks::cleanup_frame_profile(self.module, function, b)?;
                    instances += 1;
                    continue;
                }
                let closure = match &declaration.ty {
                    Type::Managed(payload) if matches!(payload.as_ref(), Type::Task { .. }) => None,
                    Type::Closure(id) => Some(*id),
                    _ => return Err(unknown("original cleanup occurrence needs its complete actual owning Task frame proof")),
                };
                let mut sites = 0usize;
                for block in &function.blocks {
                    for (position, instruction) in block.instructions.iter().enumerate() {
                        b.spend(1, WORK)?;
                        let matches = match closure {
                            None => matches!(instruction.kind, InstructionKind::DropManaged { local: selected } if selected == local),
                            Some(_) => matches!(instruction.kind, InstructionKind::DropClosure { local: selected } if selected == local),
                        };
                        if matches {
                            let actual = self.local_value(instance, function, local, (block.id, position), b, 0)?;
                            if actual != expected {
                                return Err(fail("original cleanup occurrence changes its complete actual child/class value"));
                            }
                            b.lookup(b.semantic_cleanup_sites.len(), WORK)?;
                            b.semantic_cleanup_sites.insert((function.id, block.id, position, local, closure));
                            sites += 1;
                        }
                    }
                }
                if sites == 0 {
                    return Err(fail("original cleanup occurrence has no exact actual cleanup site"));
                }
                instances += 1;
            }
            if instances == 0 {
                return Err(fail("original cleanup occurrence has no complete actual instances"));
            }
        }
        Ok(())
    }
}
