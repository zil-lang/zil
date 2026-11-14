//! Memoized interpretation of immutable type recipes, not live class/loan
//! validation. Instance and complete hint belong to every request-local key.
use super::*;
type Key = (
    SemanticInstanceId,
    zil_base::Fingerprint,
    zil_base::Fingerprint,
);
struct Entry {
    recipe: SemanticTypeRecipe,
    hint: Type,
    value: SemanticTypeValue,
}
#[derive(Default)]
pub(super) struct Table {
    buckets: BTreeMap<Key, Vec<Entry>>,
    entries: usize,
    pub hits: usize,
    pub misses: usize,
    pub lookup_work: usize,
    pub evaluation_work: usize,
}
impl Table {
    pub(super) fn lookup(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticTypeRecipe,
        hint: &Type,
        limits: &Limits,
        left: &mut usize,
    ) -> R<(Key, Option<SemanticTypeValue>)> {
        let before = *left;
        let recipe_key = super::super::original::protocol::recipe_key(recipe, limits, left)?;
        inference::charge_type_snapshot(hint, Span::default(), limits, left, 2)?;
        let key = (scope, recipe_key, hint.fingerprint());
        index::pay(left, index::search(self.buckets.len()))?;
        if let Some(bucket) = self.buckets.get(&key) {
            index::pay(left, bucket.len())?;
            for entry in bucket {
                crate::dyn_metadata::semantic_operand_snapshot(&entry.recipe, limits, left, 1)?;
                crate::dyn_metadata::semantic_operand_snapshot(recipe, limits, left, 1)?;
                inference::charge_type_snapshot(&entry.hint, Span::default(), limits, left, 1)?;
                inference::charge_type_snapshot(hint, Span::default(), limits, left, 1)?;
                if entry.recipe == *recipe && entry.hint == *hint {
                    crate::dyn_metadata::semantic_operand_snapshot(&entry.value, limits, left, 1)?;
                    self.hits = self.hits.saturating_add(1);
                    self.lookup_work = self
                        .lookup_work
                        .saturating_add(before.saturating_sub(*left));
                    return Ok((key, Some(entry.value.clone())));
                }
            }
        }
        self.misses = self.misses.saturating_add(1);
        self.lookup_work = self
            .lookup_work
            .saturating_add(before.saturating_sub(*left));
        Ok((key, None))
    }
    pub(super) fn insert(
        &mut self,
        key: Key,
        recipe: &SemanticTypeRecipe,
        hint: &Type,
        value: &SemanticTypeValue,
        limits: &Limits,
        left: &mut usize,
    ) -> R<()> {
        if self.entries >= limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "world type cache entry limit exceeded",
                None,
            ));
        }
        crate::dyn_metadata::semantic_operand_snapshot(recipe, limits, left, 1)?;
        inference::charge_type_snapshot(hint, Span::default(), limits, left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(value, limits, left, 1)?;
        index::pay(
            left,
            index::search(self.buckets.len())
                .saturating_add(std::mem::size_of::<Key>())
                .saturating_add(std::mem::size_of::<Entry>()),
        )?;
        self.buckets.entry(key).or_default().push(Entry {
            recipe: recipe.clone(),
            hint: hint.clone(),
            value: value.clone(),
        });
        self.entries += 1;
        Ok(())
    }
}
