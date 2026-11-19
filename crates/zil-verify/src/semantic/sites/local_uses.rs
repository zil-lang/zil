//! Coordinates of instructions which mention a storage local, in actual body
//! order. Queries return every occurrence. The caller establishes the epoch,
//! dominance, access permissions and cleanup on the relevant path.
use super::*;

type Coordinate = (usize, usize);

struct Body {
    identity: usize,
    by_local: Vec<Vec<Coordinate>>,
}

#[derive(Default)]
pub(in crate::semantic) struct LocalUses(RefCell<BTreeMap<FunctionId, Body>>);

impl LocalUses {
    pub(in crate::semantic) fn instructions<'a>(
        &self, f: &'a TypedFunction, local: LocalId, b: &mut Budget,
    ) -> Check<Vec<(&'a BasicBlock, usize, &'a Instruction)>> {
        let mut bodies = self.0.borrow_mut();
        b.spend(lookup(bodies.len()).saturating_add(1))?;
        let identity = std::ptr::from_ref(f).addr();
        let body = match bodies.entry(f.id) {
            std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::btree_map::Entry::Vacant(entry) => {
                b.spend(f.locals.len().saturating_mul(3).saturating_add(f.blocks.len()))?;
                let mut by_local = vec![Vec::new(); f.locals.len()];
                for (block, row) in f.blocks.iter().enumerate() {
                    b.spend(row.instructions.len())?;
                    for (at, instruction) in row.instructions.iter().enumerate() {
                        for operand in instruction.kind.local_operands() {
                            b.spend(1)?;
                            let uses = by_local.get_mut(operand.0 as usize)
                                .ok_or_else(|| bad("actual instruction names an absent storage local"))?;
                            // Unpack can mention the same local in several operand roles.
                            // The consumer still checks every ordered field in the instruction.
                            if uses.last() != Some(&(block, at)) {
                                b.spend(1 + if uses.len() == uses.capacity() { uses.len() } else { 0 })?;
                                uses.push((block, at));
                            }
                        }
                    }
                }
                entry.insert(Body { identity, by_local })
            }
        };
        if body.identity != identity { return Err(bad("local occurrence query selects another actual function")); }
        let coordinates = body.by_local.get(local.0 as usize)
            .ok_or_else(|| bad("local occurrence query selects an absent local"))?;
        b.spend(coordinates.len().saturating_mul(3))?;
        Ok(coordinates.iter().map(|&(block, at)| {
            let row = &f.blocks[block];
            (row, at, &row.instructions[at])
        }).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordered_local_occurrences_preserve_unpack_operand_roles_and_body_identity() {
        let (mut module, _) = crate::tests::baseline();
        let f = &mut module.functions[0];
        let local = f.locals[0].clone();
        f.locals = (0..3).map(|id| LocalDecl { id: LocalId(id), ..local.clone() }).collect();
        f.blocks[0].instructions = [
            InstructionKind::StorageLive(LocalId(1)),
            InstructionKind::UnpackTuple { local: LocalId(0), fields: vec![LocalId(1), LocalId(1)] },
            InstructionKind::StorageDead(LocalId(2)),
        ].into_iter().map(|kind| Instruction { result: None, kind, span: Span::default() }).collect();
        f.blocks.truncate(1);
        let mut b = Budget::new(&Limits::default());
        let index = LocalUses::default();
        for local in [LocalId(1), LocalId(2), LocalId(0)] {
            let actual = index.instructions(f, local, &mut b).unwrap();
            let expected: Vec<_> = f.blocks[0].instructions.iter().enumerate()
                .filter(|(_, op)| op.kind.local_operands().any(|operand| operand == local)).collect();
            assert_eq!(actual.len(), expected.len());
            for ((_, ordinal, operation), (index, original)) in actual.into_iter().zip(expected) {
                assert_eq!(ordinal, index);
                assert!(std::ptr::eq(operation, original));
            }
        }
        assert!(index.instructions(&f.clone(), LocalId(1), &mut b).is_err());
    }
}
