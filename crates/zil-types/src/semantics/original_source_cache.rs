//! Successful pure queries of one finalized original Gamma. This cache never
//! contains emitted targets, actual class images, loans or execution facts.
use super::*;
use std::cell::RefCell;
use std::rc::Rc;

// Declared lexical premises have a narrower readiness boundary than the
// general pure query cache. A frozen E delta resets both rows and readiness.
#[derive(Default)]
pub(super) struct Premises {
    ready: std::cell::Cell<bool>,
    rows: RefCell<BTreeMap<SemanticDefinitionId, Rc<Vec<SemanticRequirementRecipe>>>>,
    nodes: std::cell::Cell<usize>,
}
impl Clone for Premises {
    fn clone(&self) -> Self {
        Self::default()
    }
}
impl Premises {
    pub(super) fn enable(&self, left: &mut usize) -> R<()> {
        fee(left, 1)?;
        self.ready.set(true);
        Ok(())
    }
    pub(super) fn lookup(
        &self,
        owner: SemanticDefinitionId,
        left: &mut usize,
    ) -> R<Option<Rc<Vec<SemanticRequirementRecipe>>>> {
        if !self.ready.get() {
            return Ok(None);
        }
        let rows = self.rows.borrow();
        fee(left, rows.len().saturating_add(1).ilog2() as usize + 2)?;
        Ok(rows.get(&owner).cloned())
    }
    pub(super) fn remember(
        &self,
        owner: SemanticDefinitionId,
        value: &Rc<Vec<SemanticRequirementRecipe>>,
        limits: &Limits,
        left: &mut usize,
    ) -> R<()> {
        if !self.ready.get() {
            return Ok(());
        }
        let nodes = self
            .nodes
            .get()
            .saturating_add(value.len())
            .saturating_add(1);
        if nodes > limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "original declared premise cache node limit exceeded",
                None,
            ));
        }
        let mut rows = self.rows.borrow_mut();
        fee(
            left,
            rows.len().saturating_add(1).ilog2() as usize
                + std::mem::size_of::<(SemanticDefinitionId, Rc<Vec<SemanticRequirementRecipe>>)>()
                + 2,
        )?;
        if rows.insert(owner, value.clone()).is_some() {
            return Err(bad(
                "original declared premise cache repeats its immutable owner",
            ));
        }
        self.nodes.set(nodes);
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum Query {
    Type(SemanticTypeRecipe),
    WhereEntry,
    WhereCall {
        site: SemanticSiteId,
        target: SemanticDefinitionId,
        arguments: Vec<SemanticActualRecipe>,
    },
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
    WhereEntry,
    WhereCall(Rc<Vec<SemanticActualRecipe>>),
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
            Self::WhereEntry => Ok(()),
            Self::WhereCall { arguments, .. } => {
                fee(left, arguments.len())?;
                for argument in arguments {
                    crate::dyn_metadata::semantic_operand_snapshot(argument, limits, left, 1)?;
                }
                Ok(())
            }
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
impl Cache {
    pub(super) fn lookup(
        &self,
        owner: SemanticDefinitionId,
        query: &Query,
        limits: &Limits,
        left: &mut usize,
    ) -> R<(Key, Option<Value>)> {
        query.snapshot(limits, left)?;
        let mut digest = zil_base::FingerprintBuilder::new();
        protocol::hash_into(&mut digest, left, query)?;
        let key = (owner, digest.digest());
        let rows = self.0.borrow();
        fee(left, rows.rows.len().saturating_add(1).ilog2() as usize + 1)?;
        if let Some(bucket) = rows.rows.get(&key) {
            fee(left, bucket.len())?;
            for (candidate, value) in bucket {
                // The query was bounded before hashing. The retained
                // candidate bounds this exact paired comparison.
                candidate.snapshot(limits, left)?;
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
    #[test]
    fn declared_premises_require_their_own_phase_and_reset_on_delta_clone() {
        let limits = Limits::default();
        let mut left = limits.max_analysis_steps;
        let cache = Premises::default();
        let values = Rc::new(Vec::new());
        cache
            .remember(owner(0), &values, &limits, &mut left)
            .unwrap();
        assert!(cache.lookup(owner(0), &mut left).unwrap().is_none());
        cache.enable(&mut left).unwrap();
        cache
            .remember(owner(0), &values, &limits, &mut left)
            .unwrap();
        assert!(Rc::ptr_eq(
            &values,
            &cache.lookup(owner(0), &mut left).unwrap().unwrap()
        ));
        assert!(cache.lookup(owner(1), &mut left).unwrap().is_none());
        let delta = cache.clone();
        assert!(delta.lookup(owner(0), &mut left).unwrap().is_none());
        delta.enable(&mut left).unwrap();
        assert!(delta.lookup(owner(0), &mut left).unwrap().is_none());
    }
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
