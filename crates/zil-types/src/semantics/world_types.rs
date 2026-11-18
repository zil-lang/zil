//! Memoized interpretation of immutable type recipes, not live class/loan
//! validation. Instance and complete hint belong to every request-local key.
use super::*;
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum RecipeKey {
    Formal(u32, u32),
    Expression(u32, zil_base::NodeId),
    Binding(u32, zil_base::NodeId),
    Parameter(u32, u32),
    Result(u32),
    Structural(zil_base::Fingerprint, zil_base::Fingerprint),
}
type Key = (SemanticInstanceId, RecipeKey);
struct Entry {
    recipe: Option<SemanticTypeRecipe>,
    hint: Type,
    value: SharedType,
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
    fn recipe_key(
        recipe: &SemanticTypeRecipe,
        hint: &Type,
        limits: &Limits,
        left: &mut usize,
    ) -> R<RecipeKey> {
        index::pay(left, 1)?;
        match recipe {
            SemanticTypeRecipe::Formal(SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(hops),
                ordinal,
            }) => return Ok(RecipeKey::Formal(*hops, *ordinal)),
            SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(hops),
                source,
            } => match source {
                SemanticOriginalTypeSource::Expression(node) => {
                    return Ok(RecipeKey::Expression(*hops, *node));
                }
                SemanticOriginalTypeSource::Binding(node) => {
                    return Ok(RecipeKey::Binding(*hops, *node));
                }
                SemanticOriginalTypeSource::Parameter(ordinal) => {
                    return Ok(RecipeKey::Parameter(*hops, *ordinal));
                }
                SemanticOriginalTypeSource::Result => return Ok(RecipeKey::Result(*hops)),
                _ => {}
            },
            _ => {}
        }
        let recipe_key = super::super::original::protocol::recipe_key(recipe, limits, left)?;
        inference::charge_type_snapshot(hint, Span::default(), limits, left, 2)?;
        Ok(RecipeKey::Structural(recipe_key, hint.fingerprint()))
    }
    pub(super) fn lookup(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticTypeRecipe,
        hint: &Type,
        limits: &Limits,
        left: &mut usize,
    ) -> R<(Key, Option<SharedType>)> {
        let before = *left;
        let key = (scope, Self::recipe_key(recipe, hint, limits, left)?);
        index::pay(left, index::search(self.buckets.len()))?;
        if let Some(bucket) = self.buckets.get(&key) {
            index::pay(left, bucket.len())?;
            for entry in bucket {
                if let Some(expected) = &entry.recipe {
                    crate::dyn_metadata::semantic_operand_snapshot(expected, limits, left, 1)?;
                    crate::dyn_metadata::semantic_operand_snapshot(recipe, limits, left, 1)?;
                    if expected != recipe {
                        continue;
                    }
                }
                inference::charge_type_snapshot(&entry.hint, Span::default(), limits, left, 1)?;
                inference::charge_type_snapshot(hint, Span::default(), limits, left, 1)?;
                if entry.hint == *hint {
                    index::pay(left, 1)?;
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
        value: &SharedType,
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
        // Compact keys encode every recipe field. Structural keys remain a
        // partition only, and retain the full recipe for exact comparison.
        let recipe = if matches!(key.1, RecipeKey::Structural(..)) {
            crate::dyn_metadata::semantic_operand_snapshot(recipe, limits, left, 1)?;
            Some(recipe.clone())
        } else {
            None
        };
        inference::charge_type_snapshot(hint, Span::default(), limits, left, 1)?;
        index::pay(left, 1)?;
        index::pay(
            left,
            index::search(self.buckets.len())
                .saturating_add(std::mem::size_of::<Key>())
                .saturating_add(std::mem::size_of::<Entry>()),
        )?;
        self.buckets.entry(key).or_default().push(Entry {
            recipe,
            hint: hint.clone(),
            value: value.clone(),
        });
        self.entries += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_keys_require_the_exact_query_scope_source_and_type_hint() {
        let mut table = Table::default();
        let limits = Limits::default();
        let mut left = limits.max_analysis_steps;
        let scope = SemanticInstanceId { module: zil_base::ModuleId(0), index: 1 };
        let recipe = |hops, source| SemanticTypeRecipe::OriginalType {
            scope: SemanticScopeRef::Ancestor(hops), source,
        };
        let id = zil_base::NodeId(7);
        let expected = recipe(0, SemanticOriginalTypeSource::Expression(id));
        let same_name = |child| Type::Record { name: "Record".into(), fields: vec![("field".into(), child)] };
        let hint = same_name(Type::I32);
        let alternate = same_name(Type::I64);
        let (key, found) = table.lookup(scope, &expected, &hint, &limits, &mut left).unwrap();
        assert!(found.is_none());
        let value = Rc::new(SemanticTypeValue::Resolved { ty: hint.clone(), life: empty(), classes: Vec::new() });
        table.insert(key, &expected, &hint, &value, &limits, &mut left).unwrap();
        let (_, found) = table.lookup(scope, &expected, &hint, &limits, &mut left).unwrap();
        assert!(Rc::ptr_eq(&found.unwrap(), &value));
        for (scope, recipe, hint) in [
            (scope, expected, &alternate),
            (scope, recipe(1, SemanticOriginalTypeSource::Expression(id)), &hint),
            (scope, recipe(0, SemanticOriginalTypeSource::Binding(id)), &hint),
            (SemanticInstanceId { index: 2, ..scope }, recipe(0, SemanticOriginalTypeSource::Expression(id)), &hint),
        ] {
            assert!(table.lookup(scope, &recipe, hint, &limits, &mut left).unwrap().1.is_none());
        }
    }
}
