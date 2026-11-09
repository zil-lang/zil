//! A lookup index over the original, ordered physical leaf rows.
//! The index carries no permission and never changes iteration or first-match
//! semantics. Small payloads keep their previous representation and fees.
use super::*;

const INDEX_THRESHOLD: usize = 32;

#[derive(Clone, Debug, Default)]
pub(super) struct Rows {
    rows: Vec<(Path, Permission)>,
    index: Option<Index>,
}

#[derive(Clone, Debug)]
struct Index {
    nodes: Vec<Node>,
    // Arena container, every node header and every complete child-edge record.
    copy_work: usize,
}
#[derive(Clone, Debug, Default)]
struct Node {
    terminal: Option<usize>,
    children: Vec<(PlaceProjection, usize)>,
}

impl PartialEq for Rows {
    fn eq(&self, other: &Self) -> bool {
        self.rows == other.rows
    }
}
impl Eq for Rows {}

impl std::ops::Deref for Rows {
    type Target = [(Path, Permission)];
    fn deref(&self) -> &Self::Target {
        &self.rows
    }
}
impl<'a> IntoIterator for &'a Rows {
    type Item = &'a (Path, Permission);
    type IntoIter = std::slice::Iter<'a, (Path, Permission)>;
    fn into_iter(self) -> Self::IntoIter {
        self.rows.iter()
    }
}

impl Index {
    fn child(
        &self,
        node: usize,
        step: &PlaceProjection,
        b: &mut Budget,
    ) -> Check<Result<usize, usize>> {
        b.spend(1)?;
        let children = &self.nodes[node].children;
        b.spend(1)?;
        if children.is_empty() {
            return Ok(Err(0));
        }
        // Compare the actual complete upper key before probing a position:
        // growing tries commonly ask for an edge beyond the current last one.
        b.spend(2)?;
        let last = children.len() - 1;
        match children[last].0.cmp(step) {
            std::cmp::Ordering::Less => return Ok(Err(children.len())),
            std::cmp::Ordering::Equal => return Ok(Ok(last)),
            std::cmp::Ordering::Greater => {}
        }
        b.spend(1)?;
        if last == 0 {
            return Ok(Err(0));
        }
        b.spend(1)?;
        if children.len() >= INDEX_THRESHOLD {
            // This is only a position hint. Authenticate the actual full key
            // before returning; sparse or mixed children still use binary search.
            b.spend(2)?;
            if let (PlaceProjection::Field(first), PlaceProjection::Field(wanted)) =
                (&children[0].0, step)
            {
                b.spend(1)?;
                if let Some(offset) = wanted.checked_sub(*first) {
                    b.spend(2)?;
                    let at = offset as usize;
                    if children.get(at).is_some_and(|(key, _)| key == step) {
                        return Ok(Ok(at));
                    }
                }
            }
        }
        let (mut low, mut high) = (0usize, children.len());
        while low < high {
            let mid = low + (high - low) / 2;
            // One edge-record visit and one complete scalar Projection comparison.
            b.spend(2)?;
            match children[mid].0.cmp(step) {
                std::cmp::Ordering::Less => low = mid + 1,
                std::cmp::Ordering::Greater => high = mid,
                std::cmp::Ordering::Equal => return Ok(Ok(mid)),
            }
        }
        Ok(Err(low))
    }
    fn position(&self, path: &[PlaceProjection], b: &mut Budget) -> Check<Option<usize>> {
        let mut node = 0;
        for step in path {
            let Ok(at) = self.child(node, step, b)? else {
                return Ok(None);
            };
            b.spend(1)?;
            node = self.nodes[node].children[at].1;
        }
        b.spend(1)?;
        Ok(self.nodes[node].terminal)
    }
    fn insert(&mut self, path: &[PlaceProjection], row: usize, b: &mut Budget) -> Check<()> {
        let mut node = 0;
        for (offset, step) in path.iter().enumerate() {
            match self.child(node, step, b)? {
                Ok(at) => {
                    b.spend(1)?;
                    node = self.nodes[node].children[at].1;
                }
                Err(at) => {
                    let count = path.len() - offset;
                    let children = &self.nodes[node].children;
                    let edge_growth = if children.len() == children.capacity() {
                        children.len()
                    } else {
                        0
                    };
                    let arena_growth = if count > self.nodes.capacity() - self.nodes.len() {
                        self.nodes.len()
                    } else {
                        0
                    };
                    let records = amount(count, 2)?;
                    // Pay every suffix node/edge, terminal write, parent shift,
                    // allocation record and actual old-buffer copy before mutation.
                    b.spend(
                        records
                            .saturating_add(3)
                            .saturating_add(children.len() - at)
                            .saturating_add(edge_growth)
                            .saturating_add(arena_growth),
                    )?;
                    self.nodes.reserve(count);
                    let first = self.nodes.len();
                    for index in 0..count {
                        let last = index + 1 == count;
                        self.nodes.push(Node {
                            terminal: last.then_some(row),
                            children: if last {
                                Vec::new()
                            } else {
                                vec![(path[offset + index + 1], first + index + 1)]
                            },
                        });
                    }
                    self.nodes[node].children.insert(at, (*step, first));
                    self.copy_work = self.copy_work.saturating_add(records);
                    return Ok(());
                }
            }
        }
        b.spend(1)?;
        if self.nodes[node].terminal.is_none() {
            b.spend(1)?;
            self.nodes[node].terminal = Some(row);
        }
        Ok(())
    }
    fn build<'a>(
        rows: impl Iterator<Item = &'a (Path, Permission)>,
        b: &mut Budget,
    ) -> Check<Self> {
        b.spend(2)?;
        let mut index = Self {
            nodes: vec![Node::default()],
            copy_work: 2,
        };
        for (ordinal, (path, _)) in rows.enumerate() {
            b.spend(1)?;
            index.insert(path, ordinal, b)?;
        }
        Ok(index)
    }
}

impl Rows {
    pub(super) fn copy_work(&self, b: &mut Budget) -> Check<()> {
        if let Some(index) = &self.index {
            b.spend(index.copy_work)?;
        }
        Ok(())
    }
    pub(super) fn position(
        &self,
        path: &[PlaceProjection],
        b: &mut Budget,
    ) -> Check<Option<usize>> {
        if let Some(index) = &self.index {
            index.position(path, b)
        } else {
            b.spend(amount(
                self.rows.len().saturating_add(1),
                path.len().saturating_add(1),
            )?)?;
            Ok(self.rows.iter().position(|(existing, _)| existing == path))
        }
    }
    pub(super) fn permission_mut(&mut self, row: usize) -> &mut Permission {
        &mut self.rows[row].1
    }
    pub(super) fn push(&mut self, row: (Path, Permission), b: &mut Budget) -> Check<()> {
        if let Some(index) = &mut self.index {
            index.insert(&row.0, self.rows.len(), b)?;
        } else if self.rows.len().saturating_add(1) >= INDEX_THRESHOLD {
            // Build and pay before adding the row; failed prepayment cannot
            // expose a partially indexed new permission.
            self.index = Some(Index::build(
                self.rows.iter().chain(std::iter::once(&row)),
                b,
            )?);
        }
        self.rows.push(row);
        Ok(())
    }
    pub(super) fn retain(
        &mut self,
        keep: impl FnMut(&(Path, Permission)) -> bool,
        b: &mut Budget,
    ) -> Check<()> {
        self.copy_work(b)?;
        self.index = None;
        self.rows.retain(keep);
        if self.rows.len() >= INDEX_THRESHOLD {
            self.index = Some(Index::build(self.rows.iter(), b)?);
        }
        Ok(())
    }
    pub(super) fn into_vec(self, b: &mut Budget) -> Check<Vec<(Path, Permission)>> {
        self.copy_work(b)?;
        Ok(self.rows)
    }
    pub(super) fn from_vec(rows: Vec<(Path, Permission)>, b: &mut Budget) -> Check<Self> {
        let index = if rows.len() >= INDEX_THRESHOLD {
            Some(Index::build(rows.iter(), b)?)
        } else {
            None
        };
        Ok(Self { rows, index })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(ordinal: usize) -> (Path, Permission) {
        (
            vec![PlaceProjection::Field(ordinal as u32)],
            Permission {
                sources: Vec::new(),
                loans: vec![ordinal],
            },
        )
    }

    #[test]
    fn lookup_keeps_first_match_order_after_clone_delete_and_reinsert() {
        let mut b = Budget::new(&Limits::default());
        let mut rows = Rows::default();
        let order: Vec<_> = (0..48).map(|i| i * 17 % 48).collect();
        for ordinal in &order {
            rows.push(row(*ordinal), &mut b).unwrap();
        }
        rows.push(row(17), &mut b).unwrap();
        assert_eq!(
            rows.iter().map(|(_, p)| p.loans[0]).collect::<Vec<_>>(),
            [order.clone(), vec![17]].concat()
        );
        rows.copy_work(&mut b).unwrap();
        let mut copied = rows.clone();
        copied.retain(|(_, p)| p.loans[0] != 0, &mut b).unwrap();
        copied.push(row(0), &mut b).unwrap();
        for ordinal in 0..48 {
            let path = [PlaceProjection::Field(ordinal)];
            let actual = copied.position(&path, &mut b).unwrap();
            assert_eq!(actual, copied.iter().position(|(p, _)| p == &path));
            assert_eq!(copied[actual.unwrap()].1.loans, [ordinal as usize]);
        }
        let plain = copied.clone().into_vec(&mut b).unwrap();
        assert_eq!(Rows::from_vec(plain, &mut b).unwrap(), copied);
        assert_eq!(
            rows.position(&[PlaceProjection::Field(17)], &mut b)
                .unwrap(),
            Some(1)
        );
    }

    #[test]
    fn threshold_index_growth_requires_prepaid_work() {
        let mut b = Budget::new(&Limits {
            max_analysis_steps: 0,
            ..Limits::default()
        });
        let mut rows = Rows::default();
        for ordinal in 0..INDEX_THRESHOLD - 1 {
            rows.push(row(ordinal), &mut b).unwrap();
        }
        let before = rows.clone();
        assert!(rows.push(row(INDEX_THRESHOLD - 1), &mut b).is_err());
        assert_eq!(rows, before);
        assert!(rows.index.is_none());
    }

    #[test]
    fn deep_common_prefix_comparison_is_prepaid_in_full() {
        let path = vec![PlaceProjection::Field(0); 17];
        let mut rows = Rows::default();
        let mut b = Budget::new(&Limits::default());
        for ordinal in 0..INDEX_THRESHOLD {
            rows.push((path.clone(), row(ordinal).1), &mut b).unwrap();
        }
        // The independent old flat-path index retains its original exact fee.
        let mut old = oracle::Rows::default();
        for ordinal in 0..INDEX_THRESHOLD {
            old.push((path.clone(), row(ordinal).1), &mut b).unwrap();
        }
        // Both complete old keys (17 + 17), comparison and ordinal read: 36.
        let mut exact = Budget::new(&Limits {
            max_analysis_steps: 36,
            ..Limits::default()
        });
        assert_eq!(old.position(&path, &mut exact).unwrap(), Some(0));
        let mut short = Budget::new(&Limits {
            max_analysis_steps: 35,
            ..Limits::default()
        });
        assert!(old.position(&path, &mut short).is_err());
        // Each of the 17 child lookups now prepays its actual probe
        // eligibility check: the previous trie work 69 plus 17 is 86.
        for steps in [86, 85, 86] {
            let result = rows.position(
                &path,
                &mut Budget::new(&Limits {
                    max_analysis_steps: steps,
                    ..Limits::default()
                }),
            );
            if steps == 86 {
                assert_eq!(result.unwrap(), Some(0));
            } else {
                assert_eq!(
                    result.unwrap_err().category,
                    zil_base::ResultCategory::ResourceLimit
                );
            }
        }
    }
}

#[cfg(test)]
#[path = "leaf_rows_oracle.rs"]
mod oracle;
#[cfg(test)]
#[path = "leaf_rows_trie_tests.rs"]
mod trie_tests;

#[cfg(test)]
#[path = "leaf_rows_probe_oracle.rs"]
mod probe_oracle;
#[cfg(test)]
#[path = "leaf_rows_probe_tests.rs"]
mod probe_tests;

#[cfg(test)]
#[path = "leaf_rows_end_oracle.rs"]
mod end_oracle;
#[cfg(test)]
#[path = "leaf_rows_end_tests.rs"]
mod end_tests;
