//! Completed substitutions for one immutable actual directory. A receipt
//! carries every typed-formal dependency visited by its original computation.
use super::*;
use std::{cell::RefCell, rc::Rc};

pub(super) type Key = (SemanticInstanceId, u32);

struct Receipt {
    generation: usize,
    depth: usize,
    dependencies: Rc<[Key]>,
    value: Rc<SemanticTypeValue>,
}

pub(super) struct Mark {
    start: usize,
    portable_epoch: usize,
}

#[derive(Default)]
struct Trace {
    frames: usize,
    keys: Vec<Key>,
    portable_epoch: usize,
}

#[derive(Default)]
pub(super) struct Directory {
    receipts: RefCell<BTreeMap<Key, Receipt>>,
    trace: RefCell<Trace>,
}

impl Directory {
    pub(super) fn begin(
        &self,
        key: Key,
        stable_mode: bool,
        limits: &Limits,
        left: &mut usize,
    ) -> R<Mark> {
        let mut trace = self.trace.borrow_mut();
        if trace.keys.len() >= limits.max_nodes {
            return Err(limit(
                "formal substitution dependency work exceeds node limit",
            ));
        }
        pay(left, 1)?;
        let mark = Mark {
            start: trace.keys.len(),
            portable_epoch: trace.portable_epoch,
        };
        if !stable_mode {
            trace.portable_epoch = trace.portable_epoch.saturating_add(1);
        }
        trace.frames += 1;
        trace.keys.push(key);
        Ok(mark)
    }

    pub(super) fn load(
        &self,
        key: Key,
        generation: usize,
        depth: usize,
        active: &BTreeSet<Key>,
        limits: &Limits,
        left: &mut usize,
    ) -> R<Option<SemanticTypeValue>> {
        let receipts = self.receipts.borrow();
        pay(left, search(receipts.len()))?;
        let Some(receipt) = receipts
            .get(&key)
            .filter(|row| row.generation == generation && row.depth >= depth)
        else {
            return Ok(None);
        };
        if !active.is_empty() {
            pay(
                left,
                active
                    .len()
                    .saturating_mul(search(receipt.dependencies.len())),
            )?;
            if active
                .iter()
                .any(|key| receipt.dependencies.binary_search(key).is_ok())
            {
                // The full resolver reports a cycle in the current active
                // derivation, even though the answer was proven elsewhere.
                return Ok(None);
            }
        }
        crate::dyn_metadata::semantic_operand_snapshot(receipt.value.as_ref(), limits, left, 1)?;
        let mut trace = self.trace.borrow_mut();
        if trace.frames > 1 {
            let count = trace.keys.len().saturating_add(receipt.dependencies.len());
            if count > limits.max_nodes {
                return Err(limit(
                    "formal substitution dependency work exceeds node limit",
                ));
            }
            pay(left, receipt.dependencies.len())?;
            trace.keys.extend(receipt.dependencies.iter().copied());
        }
        Ok(Some(receipt.value.as_ref().clone()))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn finish(
        &self,
        mark: Mark,
        key: Key,
        generation: usize,
        depth: usize,
        result: Option<&SemanticTypeValue>,
        limits: &Limits,
        left: &mut usize,
    ) -> R<()> {
        let mut trace = self.trace.borrow_mut();
        trace.frames -= 1;
        let store = (|| {
            let Some(value @ SemanticTypeValue::Resolved { life, classes, .. }) = result else {
                return Ok(());
            };
            if trace.portable_epoch != mark.portable_epoch
                || life.binder_count != 0
                || !classes.is_empty()
            {
                return Ok(());
            }
            let mut receipts = self.receipts.borrow_mut();
            let work = search(receipts.len());
            pay(left, work)?;
            if receipts
                .get(&key)
                .is_some_and(|row| row.generation == generation && row.depth >= depth)
            {
                return Ok(());
            }
            if receipts.len() >= limits.max_nodes && !receipts.contains_key(&key) {
                return Err(limit("formal substitution receipts exceed node limit"));
            }
            let used = &trace.keys[mark.start..];
            pay(
                left,
                used.len()
                    .saturating_mul(search(used.len()).saturating_add(1)),
            )?;
            let mut dependencies = used.to_vec();
            dependencies.sort_unstable();
            dependencies.dedup();
            crate::dyn_metadata::semantic_operand_snapshot(value, limits, left, 1)?;
            pay(
                left,
                work.saturating_add(std::mem::size_of::<(Key, Receipt)>()),
            )?;
            receipts.insert(
                key,
                Receipt {
                    generation,
                    depth,
                    dependencies: dependencies.into(),
                    value: Rc::new(value.clone()),
                },
            );
            Ok(())
        })();
        if trace.frames == 0 {
            trace.keys.clear();
        }
        store
    }
}

fn search(count: usize) -> usize {
    count.saturating_add(1).ilog2() as usize + 1
}
fn limit(message: &'static str) -> Diagnostic {
    Diagnostic::limit(Phase::Types, message, None)
}
fn pay(left: &mut usize, work: usize) -> R<()> {
    *left = left
        .checked_sub(work)
        .ok_or_else(|| limit("formal substitution receipt work exceeded"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(instance: u32) -> Key {
        (
            SemanticInstanceId {
                module: zil_base::ModuleId(0),
                index: instance,
            },
            0,
        )
    }

    #[test]
    fn formal_receipt_preserves_active_dependencies_depth_and_generation() {
        let limits = Limits::default();
        let mut left = limits.max_analysis_steps;
        let table = Directory::default();
        let parent = key(0);
        let child = key(1);
        let value = resolved(Type::I32, empty(), vec![]);
        let outer = table.begin(parent, true, &limits, &mut left).unwrap();
        let inner = table.begin(child, true, &limits, &mut left).unwrap();
        table
            .finish(inner, child, 2, 4, Some(&value), &limits, &mut left)
            .unwrap();
        table
            .finish(outer, parent, 2, 2, Some(&value), &limits, &mut left)
            .unwrap();
        let none = BTreeSet::new();
        assert_eq!(
            table.load(parent, 2, 2, &none, &limits, &mut left).unwrap(),
            Some(value)
        );
        assert!(
            table
                .load(parent, 2, 2, &BTreeSet::from([child]), &limits, &mut left)
                .unwrap()
                .is_none()
        );
        assert!(
            table
                .load(parent, 2, 3, &none, &limits, &mut left)
                .unwrap()
                .is_none()
        );
        assert!(
            table
                .load(parent, 3, 2, &none, &limits, &mut left)
                .unwrap()
                .is_none()
        );
        assert!(
            table
                .load(key(7), 2, 2, &none, &limits, &mut left)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            table
                .load(parent, 2, 2, &none, &limits, &mut 0)
                .unwrap_err()
                .category,
            zil_base::ResultCategory::ResourceLimit
        );
    }

    #[test]
    fn invocation_probe_cannot_publish_a_parent_substitution_receipt() {
        let limits = Limits::default();
        let mut left = limits.max_analysis_steps;
        let table = Directory::default();
        let value = resolved(Type::I32, empty(), vec![]);
        let outer = table.begin(key(0), true, &limits, &mut left).unwrap();
        let invocation = table.begin(key(1), false, &limits, &mut left).unwrap();
        table
            .finish(invocation, key(1), 0, 3, Some(&value), &limits, &mut left)
            .unwrap();
        table
            .finish(outer, key(0), 0, 1, Some(&value), &limits, &mut left)
            .unwrap();
        assert!(
            table
                .load(key(0), 0, 1, &BTreeSet::new(), &limits, &mut left)
                .unwrap()
                .is_none()
        );
    }
}
