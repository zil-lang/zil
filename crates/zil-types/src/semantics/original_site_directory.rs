//! An immutable index of checked source recipes and their original occurrences.
use super::*;
use std::rc::Rc;

type Node = (bool, NodeId);

fn node_key(node: SemanticOriginalNode) -> Node {
    match node {
        SemanticOriginalNode::Expression(node) => (true, node),
        SemanticOriginalNode::Statement(node) => (false, node),
    }
}

fn lookup(len: usize) -> usize {
    len.saturating_add(1).ilog2() as usize + 1
}

fn pay(left: &mut usize, amount: usize) -> R<()> {
    *left = left.checked_sub(amount).ok_or_else(|| {
        Diagnostic::limit(Phase::Types, "original occurrence directory work exceeded", None)
    })?;
    Ok(())
}

#[derive(Default)]
pub(super) struct Directory(BTreeMap<SemanticDefinitionId, Rc<DefinitionSites>>);

pub(super) struct DefinitionSites {
    owner: SemanticDefinitionId,
    recipes: BTreeMap<SemanticSiteId, usize>,
    by_node: BTreeMap<Node, Vec<SemanticSiteId>>,
    by_recipe: BTreeMap<SemanticSiteId, Option<SemanticOriginalNode>>,
    storage_scopes: std::cell::OnceCell<StorageScopes>,
}

impl Directory {
    pub(super) fn build(
        program: &SemanticModule,
        limits: &Limits,
        left: &mut usize,
    ) -> R<Rc<Self>> {
        let mut result = Self::default();
        pay(left, program.definitions.len())?;
        let mut rows = 0usize;
        for definition in &program.definitions {
            let SemanticDefinitionBody::FunctionWithBody { sites, site_nodes, .. } = &definition.body
            else {
                continue;
            };
            rows = rows.saturating_add(sites.len()).saturating_add(site_nodes.len());
            if rows > limits.max_nodes {
                return Err(Diagnostic::limit(
                    Phase::Types, "original occurrence directory node limit exceeded", None,
                ));
            }
            let entry = DefinitionSites::build(definition.id, sites, site_nodes, left)?;
            pay(left, lookup(result.0.len()).saturating_add(1))?;
            if result.0.insert(definition.id, Rc::new(entry)).is_some() {
                return Err(bad("original occurrence directory repeats a definition"));
            }
        }
        Ok(Rc::new(result))
    }

    pub(super) fn owner(&self, definition: SemanticDefinitionId, left: &mut usize)
        -> R<Option<Rc<DefinitionSites>>>
    {
        pay(left, lookup(self.0.len()).saturating_add(1))?;
        Ok(self.0.get(&definition).cloned())
    }
}

impl DefinitionSites {
    pub(super) fn build(
        owner: SemanticDefinitionId,
        sites: &[SemanticSiteRecipe],
        site_nodes: &[SemanticOriginalSiteNode],
        left: &mut usize,
    ) -> R<Self> {
        let mut recipes = BTreeMap::new();
        for (position, site) in sites.iter().enumerate() {
            pay(left, lookup(recipes.len()).saturating_add(1))?;
            if recipes.insert(site.id, position).is_some() {
                return Err(bad("original occurrence directory repeats a recipe"));
            }
        }
        let mut by_node = BTreeMap::<_, Vec<_>>::new();
        let mut by_recipe = BTreeMap::new();
        for row in site_nodes {
            pay(left, lookup(by_node.len()).saturating_add(lookup(by_recipe.len())).saturating_add(2))?;
            by_node.entry(node_key(row.node)).or_default().push(row.site);
            by_recipe.entry(row.site)
                .and_modify(|entry| *entry = None)
                .or_insert(Some(row.node));
        }
        Ok(Self { owner, recipes, by_node, by_recipe, storage_scopes: Default::default() })
    }

    pub(super) fn storage_covers_initializer(
        &self, body: &SemanticOriginalBlock, root: NodeId, call: NodeId,
        limits: &Limits, left: &mut usize,
    ) -> R<bool> {
        if self.storage_scopes.get().is_none() {
            let mut scopes = StorageScopes::default();
            scopes.block(body, None, limits, left, 0)?;
            self.storage_scopes.set(scopes).map_err(|_| bad("original storage scopes initialized twice"))?;
        }
        let scopes = self.storage_scopes.get().expect("original storage scopes initialized");
        pay(left, lookup(scopes.bindings.len()).saturating_add(lookup(scopes.initializers.len())))?;
        let (Some(binding), Some(&holder)) = (scopes.bindings.get(&root), scopes.initializers.get(&call)) else {
            return Ok(false);
        };
        let mut position = holder;
        loop {
            pay(left, 1)?;
            if position.block == binding.block { return Ok(binding.ordinal < position.ordinal); }
            let Some(parent) = scopes.parents[position.block] else { return Ok(false); };
            position = parent;
        }
    }

    pub(super) fn at(&self, node: SemanticOriginalNode, left: &mut usize)
        -> R<&[SemanticSiteId]>
    {
        pay(left, lookup(self.by_node.len()))?;
        let candidates = self.by_node.get(&node_key(node)).map(Vec::as_slice).unwrap_or_default();
        pay(left, candidates.len())?;
        Ok(candidates)
    }

    pub(super) fn recipe<'s>(
        &self,
        owner: SemanticDefinitionId,
        sites: &'s [SemanticSiteRecipe],
        id: SemanticSiteId,
        left: &mut usize,
    ) -> R<Option<&'s SemanticSiteRecipe>> {
        pay(left, lookup(self.recipes.len()))?;
        if self.owner != owner {
            return Err(bad("original occurrence changes its declaration"));
        }
        let Some(offset) = self.recipes.get(&id) else {
            return Ok(None);
        };
        let recipe = sites.get(*offset).filter(|recipe| recipe.id == id)
            .ok_or_else(|| bad("original occurrence changes its exact checked site"))?;
        Ok(Some(recipe))
    }

    pub(super) fn node(
        &self,
        id: SemanticSiteId,
        left: &mut usize,
    ) -> R<Option<Option<SemanticOriginalNode>>> {
        pay(left, lookup(self.by_recipe.len()))?;
        Ok(self.by_recipe.get(&id).copied())
    }
}

#[derive(Clone, Copy)]
struct Position { block: usize, ordinal: usize }

/// Lazily indexed from this definition's actual body, once per checked source
/// directory. An initialized local covers a later holder in its own or a
/// nested lexical block. No lifetime equality or pointer identity is stored.
#[derive(Default)]
struct StorageScopes {
    parents: Vec<Option<Position>>,
    bindings: BTreeMap<NodeId, Position>,
    initializers: BTreeMap<NodeId, Position>,
}
impl StorageScopes {
    fn block(
        &mut self, body: &SemanticOriginalBlock, parent: Option<Position>,
        limits: &Limits, left: &mut usize, depth: usize,
    ) -> R<()> {
        let block = self.parents.len();
        if depth > limits.max_call_depth || block >= limits.max_nodes {
            return Err(Diagnostic::limit(Phase::Types, "original storage scope index exceeds its bounds", None));
        }
        pay(left, body.statements.len().saturating_add(1).saturating_add(
            if block == self.parents.capacity() { block } else { 0 }))?;
        self.parents.push(parent);
        for (ordinal, statement) in body.statements.iter().enumerate() {
            let position = Position { block, ordinal };
            let mut descend = |body: &SemanticOriginalBlock, left: &mut usize| {
                self.block(body, Some(position), limits, left, depth + 1)
            };
            match &statement.kind {
                CoreStatementKind::Binding { initializer: Some(value), .. } => {
                    pay(left, lookup(self.bindings.len()).saturating_add(1))?;
                    if self.bindings.insert(statement.id, position).is_some() { return Err(bad("duplicate original storage binding")); }
                    let mut value = value;
                    while let CoreExprKind::Group(inner) | CoreExprKind::Ascribed { value: inner, .. } = &value.kind {
                        pay(left, 1)?;
                        value = inner;
                    }
                    if matches!(value.kind, CoreExprKind::Call { .. }) {
                        pay(left, lookup(self.initializers.len()).saturating_add(1))?;
                        if self.initializers.insert(value.id, position).is_some() { return Err(bad("duplicate original holder initializer")); }
                    }
                }
                CoreStatementKind::If { then_body, else_body, .. } | CoreStatementKind::IfLet { then_body, else_body, .. } => {
                    descend(then_body, left)?;
                    if let Some(body) = else_body { descend(body, left)?; }
                }
                CoreStatementKind::Match { arms, .. } => {
                    pay(left, arms.len())?;
                    for arm in arms { descend(&arm.body, left)?; }
                }
                CoreStatementKind::Block(body) | CoreStatementKind::Unsafe(body) | CoreStatementKind::Loop(body)
                | CoreStatementKind::While { body, .. } | CoreStatementKind::WhileLet { body, .. }
                | CoreStatementKind::ForRange { body, .. } | CoreStatementKind::ForEach { body, .. }
                | CoreStatementKind::With { body, .. } => descend(body, left)?,
                _ => {}
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::ModuleId;

    #[test]
    fn original_occurrences_keep_owner_kind_and_ambiguity() {
        let owner = SemanticDefinitionId { module: ModuleId(0), index: 0 };
        let sibling = SemanticDefinitionId { index: 1, ..owner };
        let expression = SemanticOriginalNode::Expression(NodeId(4));
        let statement = SemanticOriginalNode::Statement(NodeId(4));
        let recipes = [
            SemanticSiteRecipe {
                id: SemanticSiteId(0),
                occurrence_path: vec![1],
                kind: SemanticSiteRecipeKind::OriginalCall,
            },
            SemanticSiteRecipe {
                id: SemanticSiteId(1),
                occurrence_path: vec![1],
                kind: SemanticSiteRecipeKind::TypeUse { expected: SemanticTypeRecipe::Atom(Type::I32) },
            },
        ];
        let rows = [
            SemanticOriginalSiteNode { site: recipes[0].id, node: expression },
            SemanticOriginalSiteNode { site: recipes[1].id, node: expression },
            SemanticOriginalSiteNode { site: recipes[0].id, node: statement },
        ];
        let mut left = 10_000;
        let directory = DefinitionSites::build(owner, &recipes, &rows, &mut left).unwrap();
        assert_eq!(directory.at(expression, &mut left).unwrap(), &[recipes[0].id, recipes[1].id]);
        assert_eq!(directory.at(statement, &mut left).unwrap(), &[recipes[0].id]);
        assert!(matches!(directory.recipe(owner, &recipes, recipes[0].id, &mut left).unwrap(), Some(recipe) if recipe.id == recipes[0].id));
        assert!(directory.recipe(sibling, &recipes, recipes[0].id, &mut left).is_err());
        assert_eq!(directory.node(recipes[0].id, &mut left).unwrap(), Some(None));
        assert_eq!(directory.node(recipes[1].id, &mut left).unwrap(), Some(Some(expression)));
    }
}
