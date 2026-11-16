//! Immutable request-local metadata indices. Values are offsets, never proof
//! of a type, source operation, permission, or original instance theorem.
use super::*;
#[derive(Default)]
pub(super) struct Index {
    pub definitions: BTreeMap<SemanticDefinitionId, usize>,
    pub instances: BTreeMap<SemanticInstanceId, usize>,
    pub functions: BTreeMap<FunctionId, usize>,
    pub frozen_definitions: BTreeMap<SemanticDefinitionId, usize>,
    pub frozen_instances: BTreeMap<SemanticInstanceId, usize>,
    pub points: BTreeMap<(SemanticInstanceId, BlockId, usize), Vec<usize>>,
    pub sites: BTreeMap<SemanticInstanceId, Vec<usize>>,
    pub recipes: BTreeMap<(SemanticDefinitionId, SemanticSiteId), usize>,
    pub expressions: BTreeMap<(FunctionId, ValueId), Vec<zil_base::NodeId>>,
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
        for (offset, row) in program.instances.iter().enumerate() {
            if index.instances.insert(row.id, offset).is_some() {
                return Err(bad("actual world repeats an original instance identity"));
            }
        }
        for (offset, row) in module.functions.iter().enumerate() {
            if index.functions.insert(row.id, offset).is_some() {
                return Err(bad("actual world repeats a function identity"));
            }
        }
        for (offset, row) in program.sites.iter().enumerate() {
            let sites = index.sites.entry(row.instance).or_default();
            let ordinal = sites.len();
            sites.push(offset);
            if let SemanticSiteAddress::Typed {
                block, instruction, ..
            } = row.at
            {
                index
                    .points
                    .entry((row.instance, block, instruction as usize))
                    .or_default()
                    .push(ordinal);
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
        for (offset, site) in sites.iter().enumerate() {
            if self.recipes.len() >= limits.max_nodes {
                return Err(Diagnostic::limit(
                    Phase::Types,
                    "original recipe index node limit exceeded",
                    None,
                ));
            }
            pay(left, search(self.recipes.len()).saturating_add(2))?;
            if self
                .recipes
                .insert((definition.id, site.id), offset)
                .is_some()
            {
                return Err(bad("original definition repeats a recipe identity"));
            }
        }
        Ok(())
    }
}
