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
        Ok(Self { owner, recipes, by_node, by_recipe })
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
