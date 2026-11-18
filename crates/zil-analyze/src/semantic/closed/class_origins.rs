//! Bridge complete original phantom constructor classes into the independent
//! actual class equations. This never manufactures a physical source.
use super::*;

impl Check<'_, '_> {
    pub(super) fn constructor_class_sources(&self, b: &mut Budget) -> CompileResult<()> {
        let mut completed = BTreeMap::new();
        for instance in &self.program.instances {
            let SemanticInstanceTarget::Function(target) = &instance.target else {
                continue;
            };
            let function = self.inventory.function(target, b)?;
            for block in &function.blocks {
                for instruction in &block.instructions {
                    b.spend(1, WORK)?;
                    if !matches!(instruction.kind, InstructionKind::MakeAggregate { .. }) {
                        continue;
                    }
                    let Some(id) = instruction.result else {
                        return Err(fail("original aggregate class source has no actual result"));
                    };
                    let Some(Type::BorrowedStorage { life, .. }) =
                        function.values.get(id.0 as usize)
                    else {
                        continue;
                    };
                    // Physical retained/data propagation remains in the full
                    // access and constructor kernels. These rows describe only
                    // the complete ghost domain of this actual constructor.
                    if life.binder_count == 0 || !life.slots.is_empty() || !life.retained.is_empty()
                    {
                        continue;
                    }
                    let value = self.ssa_value(instance, function, id, b, 0)?;
                    if value.ty != function.values[id.0 as usize]
                        || value.life != *life
                        || value.classes.len() != life.binder_count as usize
                    {
                        return Err(fail(
                            "original aggregate class source loses its complete domain",
                        ));
                    }
                    for (class, source) in value.classes.iter().enumerate() {
                        let mut header = None;
                        for row in &instance.header_embedding {
                            b.spend(1, WORK)?;
                            if self.header_class(instance, row.header, b)? == *source {
                                // Several occurrences of the same original
                                // class are legal. The already checked header
                                // theorem, rather than an equal local number,
                                // identifies this source.
                                header.get_or_insert(row.header);
                            }
                        }
                        // A locally introduced class requires its real local
                        // occurrence source; do not turn a symbolic recipe or
                        // unbound phantom actual into a physical Address.
                        let Some(header) = header else { continue };
                        let class = u32::try_from(class)
                            .map_err(|_| fail("original aggregate class ordinal overflow"))?;
                        b.lookup(completed.len(), WORK)?;
                        if completed
                            .insert((function.id, id, class), header)
                            .is_some_and(|old| old != header)
                        {
                            return Err(fail(
                                "shared aggregate code has different actual header origins in its original worlds",
                            ));
                        }
                    }
                }
            }
        }
        for (key, header) in completed {
            b.lookup(b.semantic_aggregate_classes.len(), WORK)?;
            if b.semantic_aggregate_classes.insert(key, header).is_some() {
                return Err(fail("completed original class source is repeated"));
            }
        }
        Ok(())
    }

    /// An ordinary producer can establish a persistent result class from a
    /// caller's original header while using a distinct temporary access
    /// region for its actual parameter. Keep that equation separate from
    /// reborrow/holding permission and require every original world to agree.
    pub(super) fn call_class_sources(&self, b: &mut Budget) -> CompileResult<()> {
        let mut common = BTreeMap::new();
        for instance in &self.program.instances {
            let SemanticInstanceTarget::Function(target) = &instance.target else {
                continue;
            };
            let function = self.inventory.function(target, b)?;
            if function.storage_life.is_none() {
                continue;
            }
            for block in &function.blocks {
                for instruction in &block.instructions {
                    b.spend(1, WORK)?;
                    if !matches!(instruction.kind, InstructionKind::Call { .. } | InstructionKind::SuspendCall { .. }) {
                        continue;
                    }
                    let Some(id) = instruction.result else { continue };
                    let ty = function.values.get(id.0 as usize)
                        .ok_or_else(|| fail("actual call has no complete output type"))?;
                    let count = match ty {
                        Type::Closure(id) => self.module.closures
                            .get(id.0 as usize)
                            .filter(|actual| actual.id == *id)
                            .and_then(|actual| actual.storage_life.as_ref())
                            .map_or(0, |life| life.fixed_binder_count),
                        Type::BorrowedStorage { storage, life }
                            if matches!(storage.as_ref(), Type::Package { .. })
                                && !life.retained.is_empty() => life.binder_count,
                        _ => continue,
                    };
                    if count == 0 {
                        continue;
                    }
                    let output = self.ssa_value(instance, function, id, b, 0)?;
                    if output.ty != *ty || output.life.binder_count != count
                        || output.classes.len() != count as usize
                        || matches!(ty, Type::BorrowedStorage { life, .. } if output.life != *life)
                    {
                        return Err(fail("original call output loses its complete persistent class directory"));
                    }
                    for (index, source) in output.classes.iter().enumerate() {
                        let mut header = None;
                        let mut ambiguous = false;
                        for row in &instance.header_embedding {
                            b.spend(1, WORK)?;
                            if self.header_class(instance, row.header, b)? == *source {
                                ambiguous |= header.is_some_and(|prior| prior != row.header);
                                header = Some(row.header);
                            }
                        }
                        if ambiguous {
                            header = None;
                        }
                        let ordinal = u32::try_from(index)
                            .map_err(|_| fail("original call output class ordinal overflow"))?;
                        b.lookup(common.len(), WORK)?;
                        common.entry((function.id, id, ordinal))
                            .and_modify(|prior| {
                                if *prior != header {
                                    *prior = None;
                                }
                            })
                            .or_insert(header);
                    }
                }
            }
        }
        for (key, header) in common {
            let Some(header) = header else { continue };
            b.lookup(b.semantic_aggregate_classes.len(), WORK)?;
            if b.semantic_aggregate_classes.insert(key, header).is_some() {
                return Err(fail("completed call class source is repeated"));
            }
        }
        Ok(())
    }

}
