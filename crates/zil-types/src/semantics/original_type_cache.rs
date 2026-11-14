//! Request-local memoization of complete original recipes. A digest selects a
//! bucket only: owner and full recipe equality remain mandatory on every hit.
use super::*;
use std::fmt::Write;
type Key = (SemanticDefinitionId, zil_base::Fingerprint);

pub(super) struct Table<T> {
    buckets: BTreeMap<Key, Vec<(SemanticTypeRecipe, T)>>,
    entries: usize,
}
impl<T: Copy> Table<T> {
    pub(super) fn new() -> Self {
        Self {
            buckets: BTreeMap::new(),
            entries: 0,
        }
    }
    pub(super) fn lookup(
        &self,
        owner: SemanticDefinitionId,
        recipe: &SemanticTypeRecipe,
        limits: &Limits,
        left: &mut usize,
    ) -> R<(Key, Option<T>)> {
        let key = (owner, digest(recipe, limits, left)?);
        fee(
            left,
            self.buckets.len().saturating_add(1).ilog2() as usize + 1,
        )?;
        if let Some(bucket) = self.buckets.get(&key) {
            fee(left, bucket.len())?;
            for (candidate, result) in bucket {
                crate::dyn_metadata::semantic_operand_snapshot(candidate, limits, left, 1)?;
                crate::dyn_metadata::semantic_operand_snapshot(recipe, limits, left, 1)?;
                if candidate == recipe {
                    return Ok((key, Some(*result)));
                }
            }
        }
        Ok((key, None))
    }
    pub(super) fn insert(
        &mut self,
        key: Key,
        recipe: &SemanticTypeRecipe,
        value: T,
        limits: &Limits,
        left: &mut usize,
    ) -> R<()> {
        if self.entries >= limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "original type cache entry limit exceeded",
                None,
            ));
        }
        fee(
            left,
            (self.buckets.len().saturating_add(1).ilog2() as usize + 1)
                .saturating_add(std::mem::size_of::<Key>())
                .saturating_add(std::mem::size_of::<(SemanticTypeRecipe, T)>()),
        )?;
        let recipe = copy_recipe(recipe, limits, left)?;
        self.buckets.entry(key).or_default().push((recipe, value));
        self.entries += 1;
        Ok(())
    }
}
fn fee(left: &mut usize, amount: usize) -> R<()> {
    *left = left.checked_sub(amount).ok_or_else(|| {
        Diagnostic::limit(Phase::Types, "original type cache work exceeded", None)
    })?;
    Ok(())
}
struct Sink<'a> {
    digest: zil_base::FingerprintBuilder,
    left: &'a mut usize,
}
impl Write for Sink<'_> {
    fn write_str(&mut self, bytes: &str) -> std::fmt::Result {
        *self.left = self.left.checked_sub(bytes.len()).ok_or(std::fmt::Error)?;
        self.digest.update(bytes.as_bytes());
        Ok(())
    }
}
fn digest(
    recipe: &SemanticTypeRecipe,
    limits: &Limits,
    left: &mut usize,
) -> R<zil_base::Fingerprint> {
    // Validate the entire grammar, its depth and allocation bounds before
    // recursive Debug traversal. Derived Debug includes every recipe field;
    // streaming avoids a second unbounded textual allocation. This identity is
    // private to this request and never an external or persistent certificate.
    crate::dyn_metadata::semantic_operand_snapshot(recipe, limits, left, 1)?;
    let mut sink = Sink {
        digest: zil_base::FingerprintBuilder::new(),
        left,
    };
    write!(&mut sink, "{recipe:?}").map_err(|_| {
        Diagnostic::limit(
            Phase::Types,
            "original type cache digest budget exceeded",
            None,
        )
    })?;
    Ok(sink.digest.digest())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn owner(index: u32) -> SemanticDefinitionId {
        SemanticDefinitionId {
            module: zil_base::ModuleId(0),
            index,
        }
    }
    #[test]
    fn colliding_bucket_does_not_supply_another_recipes_cleanup_fact() {
        let limits = Limits::default();
        let mut left = limits.max_analysis_steps;
        let expected = SemanticTypeRecipe::Atom(Type::I32);
        let different = SemanticTypeRecipe::Atom(Type::I64);
        let mut table = Table::<bool>::new();
        let (key, _) = table
            .lookup(owner(0), &expected, &limits, &mut left)
            .unwrap();
        table
            .insert(key, &different, true, &limits, &mut left)
            .unwrap();
        assert_eq!(
            table
                .lookup(owner(0), &expected, &limits, &mut left)
                .unwrap()
                .1,
            None
        );
    }
    #[test]
    fn same_ordinal_in_another_definition_cannot_reuse_a_capability() {
        let limits = Limits::default();
        let mut left = limits.max_analysis_steps;
        let recipe = SemanticTypeRecipe::Formal(SemanticFormalRef {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal: 0,
        });
        let mut table = Table::<bool>::new();
        let (key, _) = table.lookup(owner(0), &recipe, &limits, &mut left).unwrap();
        table
            .insert(key, &recipe, true, &limits, &mut left)
            .unwrap();
        assert_eq!(
            table
                .lookup(owner(1), &recipe, &limits, &mut left)
                .unwrap()
                .1,
            None
        );
        assert_eq!(
            table
                .lookup(owner(0), &recipe, &limits, &mut left)
                .unwrap()
                .1,
            Some(true)
        );
    }
}
