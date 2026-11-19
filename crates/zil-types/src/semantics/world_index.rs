//! Immutable request-local metadata indices. Values are offsets, never proof
//! of a type, source operation, permission, or original instance theorem.
use super::*;
#[derive(Clone, Copy)]
pub(super) struct OriginalSite {
    pub offset: usize,
    pub node: Option<SemanticOriginalNode>,
}
pub(super) enum DefinitionRecipes {
    Dense(Vec<OriginalSite>),
    Sparse(BTreeMap<SemanticSiteId, OriginalSite>),
}

impl DefinitionRecipes {
    pub(super) fn get(&self, id: SemanticSiteId, left: &mut usize) -> R<Option<OriginalSite>> {
        Ok(match self {
            Self::Dense(rows) => {
                pay(left, 1)?;
                rows.get(id.0 as usize).copied()
            }
            Self::Sparse(rows) => {
                pay(left, search(rows.len()))?;
                rows.get(&id).copied()
            }
        })
    }

    fn get_mut(&mut self, id: SemanticSiteId, left: &mut usize) -> R<Option<&mut OriginalSite>> {
        Ok(match self {
            Self::Dense(rows) => {
                pay(left, 1)?;
                rows.get_mut(id.0 as usize)
            }
            Self::Sparse(rows) => {
                pay(left, search(rows.len()))?;
                rows.get_mut(&id)
            }
        })
    }
}
pub(super) type BlockPoints = BTreeMap<usize, Rc<Vec<usize>>>;

pub(super) struct BlockSites<'a> {
    rows: std::iter::Peekable<std::collections::btree_map::Iter<'a, usize, Rc<Vec<usize>>>>,
}

impl<'a> BlockSites<'a> {
    pub(super) fn new(rows: &'a BlockPoints) -> Self {
        Self { rows: rows.iter().peekable() }
    }

    pub(super) fn at(&mut self, instruction: usize, left: &mut usize) -> R<Option<Rc<Vec<usize>>>> {
        pay(left, 1)?;
        let Some((&next, _)) = self.rows.peek().copied() else { return Ok(None); };
        if next < instruction {
            return Err(bad("ordered block traversal skipped an actual original site"));
        }
        if next == instruction {
            Ok(self.rows.next().map(|(_, offsets)| offsets.clone()))
        } else {
            Ok(None)
        }
    }
}

pub(super) struct SiteRows<'a> {
    sites: &'a [SemanticSite],
    offsets: Option<Rc<Vec<usize>>>,
    next: usize,
}
impl<'a> SiteRows<'a> {
    pub(super) fn new(sites: &'a [SemanticSite], offsets: Option<Rc<Vec<usize>>>) -> Self {
        Self { sites, offsets, next: 0 }
    }
}
impl<'a> Iterator for SiteRows<'a> {
    type Item = &'a SemanticSite;
    fn next(&mut self) -> Option<Self::Item> {
        let offset = *self.offsets.as_ref()?.get(self.next)?;
        self.next += 1;
        Some(&self.sites[offset])
    }
}

#[derive(Default)]
pub(super) struct Index {
    pub definitions: BTreeMap<SemanticDefinitionId, usize>,
    pub type_contexts: BTreeMap<SemanticInstanceId, type_contexts::Context>,
    pub instances: BTreeMap<SemanticInstanceId, usize>,
    pub functions: BTreeMap<FunctionId, usize>,
    pub closures: BTreeMap<ClosureId, Vec<usize>>,
    pub closure_children: BTreeMap<ClosureId, Rc<Vec<usize>>>,
    pub frozen_definitions: BTreeMap<SemanticDefinitionId, usize>,
    pub frozen_instances: BTreeMap<SemanticInstanceId, usize>,
    pub points: BTreeMap<(SemanticInstanceId, BlockId), Rc<BlockPoints>>,
    pub sites: BTreeMap<SemanticInstanceId, Vec<usize>>,
    pub recipes: BTreeMap<SemanticDefinitionId, Rc<DefinitionRecipes>>,
    recipe_count: usize,
    pub local_declarations: BTreeMap<(SemanticInstanceId, LocalId), Rc<Vec<usize>>>,
    pub expressions: BTreeMap<(FunctionId, ValueId), Vec<zil_base::NodeId>>,
    pub requested_calls: BTreeMap<(SemanticInstanceId, SemanticSiteId, FunctionId), Vec<usize>>,
    pub ordinary_calls: BTreeMap<FunctionId, Vec<usize>>,
    pub source_call_occurrences: BTreeMap<(SemanticInstanceId, SemanticSiteId), usize>,
}
pub(super) fn search(n: usize) -> usize {
    n.saturating_add(1).ilog2() as usize + 1
}
pub(super) fn pay(left: &mut usize, n: usize) -> R<()> {
    *left = left.checked_sub(n).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Types,
            "actual world metadata index budget exceeded",
            None,
        )
    })?;
    Ok(())
}
impl Index {
    pub(super) fn new(
        program: &SemanticModule,
        module: &TypedModule,
        ledger: &Ledger,
        limits: &Limits,
        left: &mut usize,
    ) -> R<Self> {
        let count = program
            .definitions
            .len()
            .saturating_add(program.instances.len())
            .saturating_add(program.sites.len())
            .saturating_add(module.functions.len());
        if count > limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "actual world metadata index node limit exceeded",
                None,
            ));
        }
        pay(left, count.saturating_mul(search(count).saturating_add(4)))?;
        let mut index = Self::default();
        for (offset, row) in program.definitions.iter().enumerate() {
            if index.definitions.insert(row.id, offset).is_some() {
                return Err(bad("actual world repeats an original definition identity"));
            }
        }
        {
            let requests = ledger.instance_requests.borrow();
            for (offset, row) in program.instances.iter().enumerate() {
                if index.instances.insert(row.id, offset).is_some() {
                    return Err(bad("actual world repeats an original instance identity"));
                }
                let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(target)) =
                    row.target
                else {
                    continue;
                };
                if let Some(SemanticEnvironmentInstance {
                    closure: SemanticClosureTarget::Typed(closure), ..
                }) = &row.environment {
                    pay(left, search(index.closure_children.len()).saturating_add(1))?;
                    let offsets = index.closure_children.entry(*closure).or_default();
                    Rc::get_mut(offsets).expect("closure directory is private while indexing").push(offset);
                }
                pay(left, search(requests.len()))?;
                if let Some(request) = requests.get(&row.id) {
                    if request.2 == target {
                        pay(left, search(index.requested_calls.len()).saturating_add(1))?;
                        index.requested_calls.entry(*request).or_default().push(offset);
                    }
                } else {
                    pay(left, search(index.ordinary_calls.len()).saturating_add(1))?;
                    index.ordinary_calls.entry(target).or_default().push(offset);
                }
            }
        }
        for (offset, row) in module.functions.iter().enumerate() {
            if index.functions.insert(row.id, offset).is_some() {
                return Err(bad("actual world repeats a function identity"));
            }
        }
        pay(left, module.closures.len())?;
        for (offset, row) in module.closures.iter().enumerate() {
            pay(left, search(index.closures.len()).saturating_add(1))?;
            index.closures.entry(row.id).or_default().push(offset);
        }
        for (offset, row) in program.sites.iter().enumerate() {
            if let SemanticSiteAction::DeclareLocal {
                local: SemanticLocalTarget::Typed(local),
                ..
            } = row.action
            {
                pay(left, search(index.local_declarations.len()).saturating_add(1))?;
                let offsets = index.local_declarations.entry((row.instance, local)).or_default();
                Rc::get_mut(offsets)
                    .expect("actual local directory is private while indexing")
                    .push(offset);
            }
            if matches!(
                row.action,
                SemanticSiteAction::Call { .. } | SemanticSiteAction::BodyCall { .. }
            ) {
                pay(left, search(index.source_call_occurrences.len()).saturating_add(1))?;
                let count = index.source_call_occurrences.entry((row.instance, row.recipe)).or_default();
                *count = count.saturating_add(1);
            }
            index.sites.entry(row.instance).or_default().push(offset);
            if let SemanticSiteAddress::Typed { block, instruction, .. } = row.at {
                pay(left, search(index.points.len()))?;
                let points = index.points.entry((row.instance, block)).or_default();
                let points = Rc::get_mut(points).expect("actual point directory is private while indexing");
                pay(left, search(points.len()).saturating_add(1))?;
                let offsets = points.entry(instruction as usize).or_default();
                Rc::get_mut(offsets).expect("actual point array is private while indexing").push(offset);
            }
        }
        if let Some(source) = &ledger.frozen {
            let count = source
                .semantic
                .definitions
                .len()
                .saturating_add(source.semantic.instances.len());
            if count > limits.max_nodes {
                return Err(Diagnostic::limit(
                    Phase::Types,
                    "frozen world metadata index node limit exceeded",
                    None,
                ));
            }
            pay(left, count.saturating_mul(search(count).saturating_add(2)))?;
            for (offset, row) in source.semantic.definitions.iter().enumerate() {
                if index.frozen_definitions.insert(row.id, offset).is_some() {
                    return Err(bad("frozen source repeats an original definition"));
                }
            }
            for (offset, row) in source.semantic.instances.iter().enumerate() {
                if index.frozen_instances.insert(row.id, offset).is_some() {
                    return Err(bad("frozen source repeats an original instance"));
                }
            }
        }
        for definition in &program.definitions {
            index.add_recipes(definition, limits, left)?;
        }
        if let Some(source) = &ledger.frozen {
            for definition in &source.semantic.definitions {
                pay(left, search(index.definitions.len()))?;
                if !index.definitions.contains_key(&definition.id) {
                    index.add_recipes(definition, limits, left)?;
                }
            }
        }
        let mut expressions = 0usize;
        for (function, frame) in &ledger.frames {
            pay(left, frame.expressions.len().saturating_add(1))?;
            for (node, expression) in &frame.expressions {
                if let Some(value) = expression.value {
                    expressions = expressions.saturating_add(1);
                    if expressions > limits.max_nodes {
                        return Err(Diagnostic::limit(
                            Phase::Types,
                            "actual expression index node limit exceeded",
                            None,
                        ));
                    }
                    pay(left, search(index.expressions.len()).saturating_add(2))?;
                    index
                        .expressions
                        .entry((*function, value))
                        .or_default()
                        .push(*node);
                }
            }
        }
        index.type_contexts = type_contexts::group(program, limits, left)?;
        Ok(index)
    }
    fn add_recipes(
        &mut self,
        definition: &SemanticDefinition,
        limits: &Limits,
        left: &mut usize,
    ) -> R<()> {
        let (SemanticDefinitionBody::Function { sites, .. }
        | SemanticDefinitionBody::FunctionWithBody { sites, .. }) = &definition.body
        else {
            return Ok(());
        };
        pay(left, sites.len())?;
        self.recipe_count = self.recipe_count.saturating_add(sites.len());
        if self.recipe_count > limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "original recipe index node limit exceeded",
                None,
            ));
        }
        let mut recipes = if sites.iter().enumerate().all(|(offset, site)| site.id.0 as usize == offset) {
            pay(left, sites.len())?;
            DefinitionRecipes::Dense((0..sites.len()).map(|offset| OriginalSite { offset, node: None }).collect())
        } else {
            let mut rows = BTreeMap::new();
            for (offset, site) in sites.iter().enumerate() {
                pay(left, search(rows.len()).saturating_add(2))?;
                if rows.insert(site.id, OriginalSite { offset, node: None }).is_some() {
                    return Err(bad("original definition repeats a recipe identity"));
                }
            }
            DefinitionRecipes::Sparse(rows)
        };
        if let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &definition.body {
            pay(left, site_nodes.len())?;
            for row in site_nodes {
                let site = recipes.get_mut(row.site, left)?
                    .ok_or_else(|| bad("original node has no recipe in its definition"))?;
                if site.node.replace(row.node).is_some() {
                    return Err(bad("original occurrence has multiple immutable source nodes"));
                }
            }
        }
        pay(left, search(self.recipes.len()).saturating_add(1))?;
        if self.recipes.insert(definition.id, Rc::new(recipes)).is_some() {
            return Err(bad("original recipe directory repeats a declaration"));
        }
        Ok(())
    }
}
