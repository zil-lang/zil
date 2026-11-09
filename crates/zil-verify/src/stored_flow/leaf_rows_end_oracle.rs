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
