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
        Ok(index)
    }
}
