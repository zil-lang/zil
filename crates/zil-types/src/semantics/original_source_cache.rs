//! Successful pure queries of one finalized original Gamma. This cache never
//! contains emitted targets, actual class images, loans or execution facts.
use super::*;
use std::cell::RefCell;
use std::fmt::Write;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Query {
    Type(SemanticTypeRecipe),
    Parent {
        caller: SemanticDefinitionId,
        call: SemanticSiteId,
        provider: SemanticDefinitionId,
        holder: SemanticDefinitionId,
        receiver: SemanticTypeRecipe,
        application: Option<SemanticTraitRecipe>,
    },
}
#[derive(Clone)]
pub(super) enum Value {
    Type(Rc<SemanticTypeRecipe>),
    Parent(Rc<Vec<SemanticActualRecipe>>),
}
type Key = (SemanticDefinitionId, zil_base::Fingerprint);
#[derive(Default)]
struct Store {
    entries: usize,
    rows: BTreeMap<Key, Vec<(Query, Value)>>,
}
#[derive(Default)]
pub(super) struct Cache(RefCell<Store>);
// A frozen E delta changes the set of declarations available to pure selection.
// The immutable source proof remains valid; its optimization table is rebuilt.
impl Clone for Cache {
    fn clone(&self) -> Self {
        Self::default()
    }
}
fn fee(left: &mut usize, n: usize) -> R<()> {
    *left = left.checked_sub(n).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Types,
            "original pure source query cache work exceeded",
            None,
        )
    })?;
    Ok(())
}
impl Query {
    pub(super) fn snapshot(&self, limits: &Limits, left: &mut usize) -> R<()> {
        fee(left, 8)?;
        match self {
            Self::Type(ty) => crate::dyn_metadata::semantic_operand_snapshot(ty, limits, left, 1),
            Self::Parent {
                receiver,
                application,
                ..
            } => {
                crate::dyn_metadata::semantic_operand_snapshot(receiver, limits, left, 1)?;
                if let Some(application) = application {
                    crate::dyn_metadata::semantic_operand_snapshot(application, limits, left, 1)?;
                }
                Ok(())
            }
        }
    }
}
struct Sink<'a> {
    digest: zil_base::FingerprintBuilder,
    left: &'a mut usize,
}
impl Write for Sink<'_> {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        *self.left = self.left.checked_sub(value.len()).ok_or(std::fmt::Error)?;
        self.digest.update(value.as_bytes());
        Ok(())
    }
}
impl Cache {
    pub(super) fn lookup(
        &self,
        owner: SemanticDefinitionId,
        query: &Query,
        limits: &Limits,
        left: &mut usize,
    ) -> R<(Key, Option<Value>)> {
        query.snapshot(limits, left)?;
        let mut sink = Sink {
            digest: zil_base::FingerprintBuilder::new(),
            left,
        };
        write!(&mut sink, "{query:?}").map_err(|_| {
            Diagnostic::limit(
                Phase::Types,
                "original pure source query digest work exceeded",
                None,
            )
        })?;
        let key = (owner, sink.digest.digest());
        let rows = self.0.borrow();
        fee(left, rows.rows.len().saturating_add(1).ilog2() as usize + 1)?;
        if let Some(bucket) = rows.rows.get(&key) {
            fee(left, bucket.len())?;
            for (candidate, value) in bucket {
                candidate.snapshot(limits, left)?;
                query.snapshot(limits, left)?;
                if candidate == query {
                    fee(left, 2)?;
                    return Ok((key, Some(value.clone())));
                }
            }
        }
        Ok((key, None))
    }
    pub(super) fn insert(
        &self,
        key: Key,
        query: Query,
        value: Value,
        limits: &Limits,
        left: &mut usize,
    ) -> R<()> {
        let mut rows = self.0.borrow_mut();
        if rows.entries >= limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "original pure query cache entry limit exceeded",
                None,
            ));
        }
        fee(
            left,
            rows.rows.len().saturating_add(1).ilog2() as usize
                + std::mem::size_of::<Key>()
                + std::mem::size_of::<(Query, Value)>(),
        )?;
        rows.rows.entry(key).or_default().push((query, value));
        rows.entries += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn owner(module: u32) -> SemanticDefinitionId {
        SemanticDefinitionId {
            module: zil_base::ModuleId(module),
            index: 0,
        }
    }
    #[test]
    fn colliding_source_query_keeps_complete_type_identity() {
        let limits = Limits::default();
        let mut left = limits.max_analysis_steps;
        let cache = Cache::default();
        let expected = Query::Type(SemanticTypeRecipe::Atom(Type::I32));
        let different = Query::Type(SemanticTypeRecipe::Atom(Type::I64));
        let (key, _) = cache
            .lookup(owner(0), &expected, &limits, &mut left)
            .unwrap();
        cache
            .insert(
                key,
                different,
                Value::Type(Rc::new(SemanticTypeRecipe::Atom(Type::I64))),
                &limits,
                &mut left,
            )
            .unwrap();
        assert!(
            cache
                .lookup(owner(0), &expected, &limits, &mut left)
                .unwrap()
                .1
                .is_none()
        );
    }
    #[test]
    fn same_local_ordinal_in_another_module_cannot_reuse_source_theorem() {
        let limits = Limits::default();
        let mut left = limits.max_analysis_steps;
        let cache = Cache::default();
        let query = Query::Type(SemanticTypeRecipe::Atom(Type::I32));
        let (key, _) = cache.lookup(owner(0), &query, &limits, &mut left).unwrap();
        cache
            .insert(
                key,
                query.clone(),
                Value::Type(Rc::new(SemanticTypeRecipe::Atom(Type::I32))),
                &limits,
                &mut left,
            )
            .unwrap();
        assert!(
            cache
                .lookup(owner(0), &query, &limits, &mut left)
                .unwrap()
                .1
                .is_some()
        );
        assert!(
            cache
                .lookup(owner(1), &query, &limits, &mut left)
                .unwrap()
                .1
                .is_none()
        );
        assert!(
            cache
                .clone()
                .lookup(owner(0), &query, &limits, &mut left)
                .unwrap()
                .1
                .is_none()
        );
    }
}
