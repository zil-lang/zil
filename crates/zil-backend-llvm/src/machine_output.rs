//! Private actual machine outputs. Pieces contain real emitted items, not keys
//! or verification authority. Compatibility materialization is explicit.
use std::sync::{Arc, OnceLock};
use zil_ir::PersistentVec;
use zil_llvm_api::model::{Item, Module, SharedVec};

#[derive(Clone, Debug)]
pub(crate) struct MachineOutput {
    pub(crate) triple: String,
    pub(crate) fingerprint: String,
    data: Data,
}
#[derive(Clone, Debug)]
enum Data {
    Flat(SharedVec<Item>),
    Pieces(Arc<Pieces>),
}
#[derive(Debug)]
struct Pieces {
    prefix: SharedVec<Item>,
    bodies: PersistentVec<SharedVec<Item>>,
    suffix: SharedVec<Item>,
    // Body starts, followed by suffix start. Reused only when all actual
    // piece lengths match; changed branches establish this without a full walk.
    starts: Arc<Vec<usize>>,
    len: usize,
    flat: OnceLock<SharedVec<Item>>,
}
impl From<Module> for MachineOutput {
    fn from(module: Module) -> Self {
        Self {
            triple: module.triple,
            fingerprint: module.fingerprint,
            data: Data::Flat(module.items),
        }
    }
}
impl PartialEq for MachineOutput {
    fn eq(&self, other: &Self) -> bool {
        if self.triple != other.triple
            || self.fingerprint != other.fingerprint
            || self.len() != other.len()
        {
            return false;
        }
        match (&self.data, &other.data) {
            (Data::Flat(a), Data::Flat(b)) => a == b,
            (Data::Pieces(a), Data::Pieces(b)) if Arc::ptr_eq(a, b) => true,
            _ => self.changed_items(other).map_or_else(
                || (0..self.len()).all(|index| self.item(index) == other.item(index)),
                |changed| changed.is_empty(),
            ),
        }
    }
}
impl Eq for MachineOutput {}
impl MachineOutput {
    pub(crate) fn from_pieces(
        triple: String,
        fingerprint: String,
        prefix: SharedVec<Item>,
        bodies: PersistentVec<SharedVec<Item>>,
        suffix: SharedVec<Item>,
        previous: Option<&Self>,
    ) -> Self {
        let old = previous.and_then(|old| match &old.data {
            Data::Pieces(p) => Some(p),
            _ => None,
        });
        let prefix = old
            .filter(|old| old.prefix == prefix)
            .map_or(prefix, |old| old.prefix.clone());
        let suffix = old
            .filter(|old| old.suffix == suffix)
            .map_or(suffix, |old| old.suffix.clone());
        let starts = old
            .filter(|old| {
                old.prefix.len() == prefix.len()
                    && old.bodies.len() == bodies.len()
                    && bodies
                        .changed_indices(&old.bodies)
                        .into_iter()
                        .all(|index| bodies[index].len() == old.bodies[index].len())
            })
            .map(|old| old.starts.clone())
            .unwrap_or_else(|| {
                let mut starts = Vec::with_capacity(bodies.len() + 1);
                let mut offset = prefix.len();
                for body in &bodies {
                    starts.push(offset);
                    offset += body.len();
                }
                starts.push(offset);
                Arc::new(starts)
            });
        let len = starts.last().copied().unwrap_or(prefix.len()) + suffix.len();
        if let Some(old) = old.filter(|old| {
            old.prefix.ptr_eq(&prefix) && old.suffix.ptr_eq(&suffix) && old.bodies.ptr_eq(&bodies)
        }) {
            return Self {
                triple,
                fingerprint,
                data: Data::Pieces(old.clone()),
            };
        }
        Self {
            triple,
            fingerprint,
            data: Data::Pieces(Arc::new(Pieces {
                prefix,
                bodies,
                suffix,
                starts,
                len,
                flat: OnceLock::new(),
            })),
        }
    }
    pub(crate) fn len(&self) -> usize {
        match &self.data {
            Data::Flat(items) => items.len(),
            Data::Pieces(p) => p.len,
        }
    }
    pub(crate) fn item(&self, index: usize) -> Option<&Item> {
        match &self.data {
            Data::Flat(items) => items.get(index),
            Data::Pieces(p) => {
                if index < p.prefix.len() {
                    return p.prefix.get(index);
                }
                let suffix = *p.starts.last()?;
                if index >= suffix {
                    return p.suffix.get(index - suffix);
                }
                let body = p
                    .starts
                    .partition_point(|&start| start <= index)
                    .checked_sub(1)?;
                p.bodies.get(body)?.get(index - p.starts[body])
            }
        }
    }
    pub(crate) fn to_module(&self) -> Module {
        let items = match &self.data {
            Data::Flat(items) => items.clone(),
            Data::Pieces(p) => p
                .flat
                .get_or_init(|| {
                    p.prefix
                        .iter()
                        .chain(p.bodies.iter().flat_map(|body| body.iter()))
                        .chain(p.suffix.iter())
                        .cloned()
                        .collect()
                })
                .clone(),
        };
        Module {
            triple: self.triple.clone(),
            fingerprint: self.fingerprint.clone(),
            items,
        }
    }
    /// Complete actual differences, never a caller-provided dirty hint. Unknown
    /// placement/foreign flat owners force the caller's original full comparison.
    pub(crate) fn changed_items(&self, previous: &Self) -> Option<Vec<usize>> {
        if self.triple != previous.triple || self.len() != previous.len() {
            return None;
        }
        let (Data::Pieces(current), Data::Pieces(old)) = (&self.data, &previous.data) else {
            return None;
        };
        if Arc::ptr_eq(current, old) {
            return Some(Vec::new());
        }
        if !Arc::ptr_eq(&current.starts, &old.starts) || current.suffix.len() != old.suffix.len() {
            return None;
        }
        let mut changed = Vec::new();
        let mut compare = |offset: usize, a: &SharedVec<Item>, b: &SharedVec<Item>| {
            if !a.ptr_eq(b) {
                for (index, (a, b)) in a.iter().zip(b).enumerate() {
                    if a != b {
                        changed.push(offset + index);
                    }
                }
            }
        };
        compare(0, &current.prefix, &old.prefix);
        for index in current.bodies.changed_indices(&old.bodies) {
            compare(
                current.starts[index],
                &current.bodies[index],
                &old.bodies[index],
            );
        }
        compare(*current.starts.last()?, &current.suffix, &old.suffix);
        Some(changed)
    }
    #[cfg(test)]
    pub(crate) fn is_materialized(&self) -> bool {
        match &self.data {
            Data::Flat(_) => true,
            Data::Pieces(p) => p.flat.get().is_some(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_llvm_api::model::Ty;
    fn item(name: &str, bits: u32) -> Item {
        Item::NamedType(name.into(), vec![Ty::Int(bits)])
    }
    fn output(
        prefix: SharedVec<Item>,
        bodies: PersistentVec<SharedVec<Item>>,
        suffix: SharedVec<Item>,
        prior: Option<&MachineOutput>,
    ) -> MachineOutput {
        MachineOutput::from_pieces(
            "x86_64-unknown-linux-gnu".into(),
            "current".into(),
            prefix,
            bodies,
            suffix,
            prior,
        )
    }
    #[test]
    fn actual_pieces_keep_order_and_stop_equal_output_without_flattening() {
        let prefix: SharedVec<_> = vec![item("prefix", 8)].into();
        let suffix: SharedVec<_> = vec![item("suffix", 64)].into();
        let bodies: PersistentVec<SharedVec<_>> = vec![
            Vec::new().into(),
            vec![item("first", 32)].into(),
            Vec::new().into(),
            vec![item("second", 16)].into(),
            Vec::new().into(),
        ]
        .into();
        let cold = output(prefix.clone(), bodies.clone(), suffix.clone(), None);
        assert!(!cold.is_materialized());
        assert_eq!(cold.len(), 4);
        assert_eq!(cold.item(1), Some(&item("first", 32)));
        assert_eq!(cold.item(2), Some(&item("second", 16)));
        assert_eq!(cold.item(3), Some(&item("suffix", 64)));
        assert!(cold.item(4).is_none());
        let mut next = bodies.clone();
        next.replace(3, vec![item("second", 64)].into());
        let current = output(prefix.clone(), next.clone(), suffix.clone(), Some(&cold));
        assert_eq!(current.changed_items(&cold), Some(vec![2]));
        assert!(!current.is_materialized());
        let equal = output(prefix.clone(), next.clone(), suffix.clone(), Some(&current));
        assert_eq!(equal.changed_items(&current), Some(vec![]));
        assert!(!equal.is_materialized());
        let mut changed_prefix = prefix.clone();
        changed_prefix[0] = item("prefix", 16);
        let header = output(changed_prefix, next.clone(), suffix.clone(), Some(&current));
        assert_eq!(header.changed_items(&current), Some(vec![0]));
        assert!(!header.is_materialized());
        next.replace(1, vec![item("first", 32), item("extra", 32)].into());
        let resized = output(prefix, next, suffix, Some(&current));
        assert_eq!(resized.changed_items(&current), None);
        assert!(!resized.is_materialized());
        assert_eq!(
            resized.to_module().items.as_slice(),
            &[
                item("prefix", 8),
                item("first", 32),
                item("extra", 32),
                item("second", 64),
                item("suffix", 64)
            ]
        );
        assert!(resized.is_materialized());
        assert!(!current.is_materialized());
        let flat: MachineOutput = current.to_module().into();
        assert_eq!(flat, current);
        assert_eq!(flat.changed_items(&current), None);
    }
}
