//! Bridge complete original phantom constructor classes into the independent
//! actual class equations. This never manufactures a physical source.
use super::*;

impl Check<'_, '_> {
    pub(super) fn constructor_class_sources(&self, b: &mut Budget) -> CompileResult<()> {
        let mut completed = BTreeMap::new();
        for function in &self.module.functions {
            let instance = self.function_instance(function.id, b)?;
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
                        if completed.insert((function.id, id, class), header).is_some() {
                            return Err(fail("original aggregate class source is repeated"));
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
}
