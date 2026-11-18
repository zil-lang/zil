//! Immutable occurrence lookup. Every row, duplicate, edge and action remains
//! visible; a lookup supplies no class, occupant, source or access permission.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Action {
    Construct,
    Call,
    CallableCall,
    TypeUse,
    DeclareLocal,
    Closure,
    CoreBodyCall,
    BodyCall,
}
impl Action {
    fn of(action: &SemanticSiteAction) -> Self {
        match action {
            SemanticSiteAction::Construct { .. } => Self::Construct,
            SemanticSiteAction::Call { .. } => Self::Call,
            SemanticSiteAction::CallableCall { .. } => Self::CallableCall,
            SemanticSiteAction::TypeUse { .. } => Self::TypeUse,
            SemanticSiteAction::DeclareLocal { .. } => Self::DeclareLocal,
            SemanticSiteAction::Closure { .. } => Self::Closure,
            SemanticSiteAction::CoreBodyCall => Self::CoreBodyCall,
            SemanticSiteAction::BodyCall { .. } => Self::BodyCall,
        }
    }
}
pub(super) struct Index {
    module: usize,
    limits: Limits,
    by_recipe: BTreeMap<(SemanticInstanceId, SemanticSiteId), Vec<usize>>,
    by_position: BTreeMap<(SemanticInstanceId, BlockId, u32), Vec<usize>>,
    pub(super) producers: Producers,
}

/// Physical SSA definitions in the immutable body. The result gives an exact
/// instruction coordinate. Callers still prove that it reaches the use and
/// that the original operand, storage, permissions and generation agree.
#[derive(Default)]
pub(super) struct Producers(RefCell<BTreeMap<FunctionId, BodyProducers>>);

#[derive(Clone, Copy)]
enum Definition {
    Missing,
    Unique { block: usize, instruction: u32 },
    Duplicate,
}

struct BodyProducers {
    body: usize,
    definitions: Vec<Definition>,
}

impl Producers {
    pub(super) fn get<'a>(
        &self,
        function: &'a TypedFunction,
        value: ValueId,
        b: &mut Budget,
    ) -> Check<(BlockId, u32, &'a Instruction)> {
        let mut bodies = self.0.borrow_mut();
        b.spend(lookup(bodies.len()).saturating_add(1))?;
        let body = function as *const TypedFunction as usize;
        let indexed = match bodies.entry(function.id) {
            std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::btree_map::Entry::Vacant(entry) => {
                b.spend(function.values.len().saturating_add(function.blocks.len()))?;
                let mut definitions = vec![Definition::Missing; function.values.len()];
                for (block, row) in function.blocks.iter().enumerate() {
                    b.spend(row.instructions.len())?;
                    for (instruction, op) in row.instructions.iter().enumerate() {
                        if let Some(result) = op.result {
                            let slot = definitions
                                .get_mut(result.0 as usize)
                                .ok_or_else(|| bad("actual SSA producer names an absent value"))?;
                            *slot = match *slot {
                                Definition::Missing => Definition::Unique {
                                    block,
                                    instruction: u32::try_from(instruction).map_err(|_| {
                                        bad("actual SSA producer ordinal overflows")
                                    })?,
                                },
                                Definition::Unique { .. } | Definition::Duplicate => {
                                    Definition::Duplicate
                                }
                            };
                        }
                    }
                }
                entry.insert(BodyProducers { body, definitions })
            }
        };
        if indexed.body != body {
            return Err(bad("actual SSA directory selects another immutable body"));
        }
        match indexed.definitions.get(value.0 as usize) {
            Some(Definition::Unique { block, instruction }) => {
                let block = function
                    .blocks
                    .get(*block)
                    .ok_or_else(|| bad("actual SSA coordinate loses its indexed block"))?;
                let op = block
                    .instructions
                    .get(*instruction as usize)
                    .filter(|op| op.result == Some(value))
                    .ok_or_else(|| {
                        bad("actual SSA coordinate no longer defines its indexed value")
                    })?;
                Ok((block.id, *instruction, op))
            }
            Some(Definition::Duplicate) => Err(bad("actual value has several SSA producers")),
            Some(Definition::Missing) | None => Err(bad("actual value lacks its SSA definition")),
        }
    }
}
fn lookup(n: usize) -> usize {
    n.checked_ilog2().map_or(1, |n| n as usize + 2)
}
impl Index {
    pub(super) fn same_input(&self, module: &TypedModule, b: &Budget) -> bool {
        self.module == module as *const TypedModule as usize && self.limits == b.limits
    }

    pub(super) fn new(module: &TypedModule, b: &mut Budget) -> Check<Self> {
        b.spend(12)?;
        Ok(Self {
            module: module as *const TypedModule as usize,
            limits: b.limits.clone(),
            by_recipe: BTreeMap::new(),
            by_position: BTreeMap::new(),
            producers: Producers::default(),
        })
    }
    pub(super) fn insert(
        &mut self,
        position: usize,
        site: &SemanticSite,
        b: &mut Budget,
    ) -> Check<()> {
        let SemanticSiteAddress::Typed {
            block, instruction, ..
        } = site.at
        else {
            return Err(bad("actual occurrence index cannot retain a Core address"));
        };
        b.spend(
            lookup(self.by_recipe.len())
                .saturating_add(lookup(self.by_position.len()))
                .saturating_add(2),
        )?;
        let rows = self
            .by_recipe
            .entry((site.instance, site.recipe))
            .or_default();
        push(rows, position, b)?;
        let rows = self
            .by_position
            .entry((site.instance, block, instruction))
            .or_default();
        push(rows, position, b)?;
        Ok(())
    }
    fn programme<'a>(&self, module: &'a TypedModule, b: &mut Budget) -> Check<&'a SemanticModule> {
        if self.module != module as *const TypedModule as usize {
            return Err(bad(
                "actual occurrence directory selects another immutable module",
            ));
        }
        let programme = module
            .semantic
            .as_ref()
            .ok_or_else(|| bad("actual occurrence directory has no original programme"))?;
        b.spend(12)?;
        if self.limits != b.limits {
            dynamic_metadata::recipe(programme, b)?;
        }
        Ok(programme)
    }
    fn rows<'a>(
        programme: &'a SemanticModule,
        positions: Option<&[usize]>,
        b: &mut Budget,
    ) -> Check<Vec<&'a SemanticSite>> {
        let positions = positions.unwrap_or(&[]);
        b.spend(positions.len())?;
        positions
            .iter()
            .map(|index| {
                programme.sites.get(*index).ok_or_else(|| {
                    bad("actual occurrence directory selects an absent immutable row")
                })
            })
            .collect()
    }
    pub(super) fn at<'a>(
        &self,
        module: &'a TypedModule,
        instance: SemanticInstanceId,
        block: BlockId,
        instruction: u32,
        b: &mut Budget,
    ) -> Check<Vec<&'a SemanticSite>> {
        let programme = self.programme(module, b)?;
        b.spend(lookup(self.by_position.len()))?;
        let rows = Self::rows(
            programme,
            self.by_position
                .get(&(instance, block, instruction))
                .map(Vec::as_slice),
            b,
        )?;
        for row in &rows {
            b.spend(1)?;
            if row.instance != instance
                || !matches!(row.at,SemanticSiteAddress::Typed{block:id,instruction:at,..}if id==block&&at==instruction)
            {
                return Err(bad(
                    "actual occurrence directory changes its exact lexical/position identity",
                ));
            }
        }
        Ok(rows)
    }
    pub(super) fn all_roles<'a>(
        &self, module: &'a TypedModule, instance: SemanticInstanceId, recipe: SemanticSiteId,
        b: &mut Budget,
    ) -> Check<Vec<&'a SemanticSite>> {
        let programme = self.programme(module, b)?;
        b.spend(lookup(self.by_recipe.len()))?;
        let rows = Self::rows(programme, self.by_recipe.get(&(instance, recipe)).map(Vec::as_slice), b)?;
        for row in &rows {
            b.spend(1)?;
            if row.instance != instance || row.recipe != recipe {
                return Err(bad("actual occurrence directory changes its original instance or recipe"));
            }
        }
        Ok(rows)
    }
    pub(super) fn recipe<'a>(
        &self, module: &'a TypedModule, instance: SemanticInstanceId, recipe: SemanticSiteId,
        action: Action, b: &mut Budget,
    ) -> Check<Vec<&'a SemanticSite>> {
        let mut rows = self.all_roles(module, instance, recipe, b)?;
        b.spend(rows.len())?;
        rows.retain(|row| Action::of(&row.action) == action);
        Ok(rows)
    }
    pub(super) fn block_from<'a>(
        &self, module: &'a TypedModule, instance: SemanticInstanceId, block: BlockId, first: u32, b: &mut Budget,
    ) -> Check<Vec<&'a SemanticSite>> {
        let program = self.programme(module, b)?;
        b.spend(lookup(self.by_position.len()).saturating_mul(2))?;
        let mut rows = vec![];
        for ((_, _, at), positions) in self.by_position.range((instance,block,first)..=(instance,block,u32::MAX)) {
            b.spend(positions.len().saturating_add(1))?;
            for site in Self::rows(program, Some(positions), b)? {
                if site.instance != instance || !matches!(site.at, SemanticSiteAddress::Typed{block:id,instruction,..} if id==block && instruction==*at) {
                    return Err(bad("actual occurrence directory changes its indexed block or instruction"));
                }
                b.spend(1usize.saturating_add(if rows.capacity()==rows.len(){rows.len()}else{0}))?;
                rows.push(site);
            }
        }
        Ok(rows)
    }

}
fn push(rows: &mut Vec<usize>, position: usize, b: &mut Budget) -> Check<()> {
    b.spend(1usize.saturating_add(if rows.len() == rows.capacity() {
        rows.len()
    } else {
        0
    }))?;
    rows.push(position);
    Ok(())
}

impl CheckedContext {
    pub(super) fn actual_sites<'a>(
        &self,
        module: &'a TypedModule,
        instance: SemanticInstanceId,
        block: BlockId,
        instruction: u32,
        b: &mut Budget,
    ) -> Check<Vec<&'a SemanticSite>> {
        self.site_index.at(module, instance, block, instruction, b)
    }
    pub(super) fn actual_declarations<'a>(
        &self,
        module: &'a TypedModule,
        instance: SemanticInstanceId,
        recipe: SemanticSiteId,
        b: &mut Budget,
    ) -> Check<Vec<&'a SemanticSite>> {
        self.site_index
            .recipe(module, instance, recipe, Action::DeclareLocal, b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn producer_index_scans_one_immutable_body_and_keeps_missing_and_duplicate_distinct() {
        let (mut module, _) = crate::tests::baseline();
        let f = &mut module.functions[0];
        f.values = vec![Type::I32; 4];
        let result = |id| Instruction {
            result: Some(ValueId(id)),
            kind: InstructionKind::Constant(Constant::I32(0)),
            span: Span::default(),
        };
        f.blocks[0].instructions = vec![result(0), result(3), result(3)];
        f.blocks[0]
            .instructions
            .extend((0..64).map(|_| Instruction {
                result: None,
                kind: InstructionKind::Unsupported("index test".into()),
                span: Span::default(),
            }));
        let mut budget = Budget::new(&Limits::default());
        let producers = Producers::default();
        let before = budget.remaining;
        for _ in 0..10 {
            let (block, ordinal, _) = producers.get(f, ValueId(0), &mut budget).unwrap();
            assert_eq!((block, ordinal), (f.blocks[0].id, 0));
        }
        assert!(before - budget.remaining < 2 * f.blocks[0].instructions.len());
        assert!(
            producers
                .get(f, ValueId(1), &mut budget)
                .unwrap_err()
                .message
                .contains("lacks")
        );
        assert!(
            producers
                .get(f, ValueId(3), &mut budget)
                .unwrap_err()
                .message
                .contains("several")
        );
        assert!(producers.get(&f.clone(), ValueId(0), &mut budget).is_err());
    }
    #[test]
    fn directories_preserve_duplicate_occurrences_edges_actions_and_instance_scope() {
        let (mut module, _) = crate::tests::baseline();
        let instance = SemanticInstanceId {
            module: module.module_id,
            index: 0,
        };
        let recipe = SemanticSiteId(0);
        let row = |edge| SemanticSite {
            instance,
            recipe,
            at: SemanticSiteAddress::Typed {
                block: BlockId(0),
                instruction: 0,
                edge,
            },
            action: SemanticSiteAction::DeclareLocal {
                local: SemanticLocalTarget::Typed(LocalId(0)),
                expected: SemanticTypeValue::Resolved {
                    ty: Type::I32,
                    life: empty_life(),
                    classes: vec![],
                },
            },
        };
        let first = row(SemanticSiteEdge::Before);
        let second = row(SemanticSiteEdge::After);
        let mut third = row(SemanticSiteEdge::After);
        third.action = SemanticSiteAction::TypeUse {
            subject: SemanticOperand::Value(ValueId(0)),
            expected: SemanticTypeValue::Resolved {
                ty: Type::I32,
                life: empty_life(),
                classes: vec![],
            },
        };
        module.semantic = Some(SemanticModule {
            definitions: vec![],
            instances: vec![],
            sites: vec![first, second, third],
            dyn_scopes: vec![],
        });
        let mut b = Budget::new(&Limits::default());
        let mut index = Index::new(&module, &mut b).unwrap();
        for (position, row) in module.semantic.as_ref().unwrap().sites.iter().enumerate() {
            index.insert(position, row, &mut b).unwrap();
        }
        let rows = index.at(&module, instance, BlockId(0), 0, &mut b).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(index.all_roles(&module, instance, recipe, &mut b).unwrap(), rows);
        assert_eq!(index.block_from(&module, instance, BlockId(0), 0, &mut b).unwrap(), rows);
        assert!(index.block_from(&module, instance, BlockId(0), 1, &mut b).unwrap().is_empty());
        assert!(matches!(
            rows[0].at,
            SemanticSiteAddress::Typed {
                edge: SemanticSiteEdge::Before,
                ..
            }
        ));
        assert!(matches!(
            rows[1].at,
            SemanticSiteAddress::Typed {
                edge: SemanticSiteEdge::After,
                ..
            }
        ));
        assert_eq!(
            index
                .recipe(&module, instance, recipe, Action::DeclareLocal, &mut b)
                .unwrap()
                .len(),
            2,
            "ambiguity is retained for the consumer to reject"
        );
        assert!(
            index
                .at(
                    &module,
                    SemanticInstanceId {
                        index: 1,
                        ..instance
                    },
                    BlockId(0),
                    0,
                    &mut b
                )
                .unwrap()
                .is_empty()
        );
        let detached = module.clone();
        assert!(
            index
                .at(&detached, instance, BlockId(0), 0, &mut b)
                .is_err()
        );
        b.limits.max_nodes = 1;
        assert!(
            index.at(&module, instance, BlockId(0), 0, &mut b).is_err(),
            "a stricter request cannot borrow old whole-program bounds"
        );
    }
}
