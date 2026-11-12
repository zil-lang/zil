//! Persistent ordered raw inputs. Sharing is an identity fact, never authority.
use std::collections::VecDeque;
use std::ops::{Index, IndexMut};
use std::sync::{Arc, OnceLock};
use zil_base::{Fingerprint, FingerprintBuilder};

/// Canonical content encoding for a persistent leaf. This is not a safety proof.
pub trait PersistentItem {
    fn persistent_fingerprint(&self) -> Fingerprint;
}

#[derive(Debug)]
struct Node<T> {
    len: usize,
    kind: Kind<T>,
    fingerprint: OnceLock<Fingerprint>,
}
#[derive(Debug)]
enum Kind<T> {
    Leaf(Arc<T>),
    Branch(Arc<Node<T>>, Arc<Node<T>>),
}
impl<T> Clone for Node<T> {
    fn clone(&self) -> Self {
        Self {
            len: self.len,
            kind: match &self.kind {
                Kind::Leaf(value) => Kind::Leaf(value.clone()),
                Kind::Branch(left, right) => Kind::Branch(left.clone(), right.clone()),
            },
            fingerprint: self.fingerprint.clone(),
        }
    }
}
impl<T> Node<T> {
    fn leaf(value: T) -> Arc<Self> {
        Self::leaf_owner(Arc::new(value))
    }
    fn leaf_owner(value: Arc<T>) -> Arc<Self> {
        Arc::new(Self {
            len: 1,
            kind: Kind::Leaf(value),
            fingerprint: OnceLock::new(),
        })
    }
    fn branch(left: Arc<Self>, right: Arc<Self>) -> Arc<Self> {
        Arc::new(Self {
            len: left.len + right.len,
            kind: Kind::Branch(left, right),
            fingerprint: OnceLock::new(),
        })
    }
    fn owner(&self, index: usize) -> Option<&Arc<T>> {
        if index >= self.len {
            return None;
        }
        match &self.kind {
            Kind::Leaf(value) => Some(value),
            Kind::Branch(left, right) => {
                if index < left.len {
                    left.owner(index)
                } else {
                    right.owner(index - left.len)
                }
            }
        }
    }
    fn prefix(node: &Arc<Self>, len: usize) -> Arc<Self> {
        assert!(len > 0 && len <= node.len);
        if len == node.len {
            return node.clone();
        }
        let Kind::Branch(left, right) = &node.kind else {
            unreachable!()
        };
        if len <= left.len {
            Self::prefix(left, len)
        } else {
            Self::branch(left.clone(), Self::prefix(right, len - left.len))
        }
    }
    fn set_owner(&mut self, index: usize, value: Arc<T>) -> Option<Arc<T>> {
        if index >= self.len {
            return None;
        }
        self.fingerprint.take();
        match &mut self.kind {
            Kind::Leaf(old) => Some(std::mem::replace(old, value)),
            Kind::Branch(left, right) => {
                if index < left.len {
                    Arc::make_mut(left).set_owner(index, value)
                } else {
                    Arc::make_mut(right).set_owner(index - left.len, value)
                }
            }
        }
    }
    fn append(node: &Arc<Self>, value: T) -> Arc<Self> {
        Self::append_owner(node, Arc::new(value))
    }
    fn append_owner(node: &Arc<Self>, value: Arc<T>) -> Arc<Self> {
        if node.len.is_power_of_two() {
            return Self::branch(node.clone(), Self::leaf_owner(value));
        }
        let Kind::Branch(left, right) = &node.kind else {
            unreachable!()
        };
        Self::branch(left.clone(), Self::append_owner(right, value))
    }
    fn diff(left: &Arc<Self>, right: &Arc<Self>, offset: usize, out: &mut Vec<usize>) {
        if Arc::ptr_eq(left, right) {
            return;
        }
        match (&left.kind, &right.kind) {
            (Kind::Leaf(a), Kind::Leaf(b)) => {
                if !Arc::ptr_eq(a, b) {
                    out.push(offset);
                }
            }
            (Kind::Branch(al, ar), Kind::Branch(bl, br)) if al.len == bl.len => {
                Self::diff(al, bl, offset, out);
                Self::diff(ar, br, offset + al.len, out);
            }
            (Kind::Branch(al, ar), _) if right.len <= al.len => {
                Self::diff(al, right, offset, out);
                out.extend(offset + al.len..offset + al.len + ar.len);
            }
            (_, Kind::Branch(bl, br)) if left.len <= bl.len => {
                Self::diff(left, bl, offset, out);
                out.extend(offset + bl.len..offset + bl.len + br.len);
            }
            _ => {
                for index in 0..left.len.max(right.len) {
                    if !matches!((left.owner(index),right.owner(index)), (Some(a),Some(b)) if Arc::ptr_eq(a,b))
                    {
                        out.push(offset + index);
                    }
                }
            }
        }
    }
}
impl<T: Clone> Node<T> {
    fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        if index >= self.len {
            return None;
        }
        self.fingerprint.take();
        match &mut self.kind {
            Kind::Leaf(value) => Some(Arc::make_mut(value)),
            Kind::Branch(left, right) => {
                if index < left.len {
                    Arc::make_mut(left).get_mut(index)
                } else {
                    Arc::make_mut(right).get_mut(index - left.len)
                }
            }
        }
    }
}
impl<T: PersistentItem> Node<T> {
    fn fingerprint(&self) -> Fingerprint {
        *self.fingerprint.get_or_init(|| {
            let mut hash = FingerprintBuilder::new();
            match &self.kind {
                Kind::Leaf(value) => {
                    hash.update(b"ZIL-PERSISTENT-LEAF-V1\0");
                    hash.update(&value.persistent_fingerprint().0);
                }
                Kind::Branch(left, right) => {
                    hash.update(b"ZIL-PERSISTENT-BRANCH-V1\0");
                    hash.update(&(self.len as u64).to_le_bytes());
                    hash.update(&left.fingerprint().0);
                    hash.update(&right.fingerprint().0);
                }
            }
            hash.digest()
        })
    }
}

/// Canonically shaped binary sequence: replacement and append copy O(log n)
/// metadata; clones retain immutable leaves. Dense insertion/removal necessarily
/// reindexes following entries and currently rebuilds the sequence explicitly.
/// There is deliberately no Deref-to-slice or hidden flattening cache.
pub struct PersistentVec<T> {
    root: Option<Arc<Node<T>>>,
}
impl<T> Clone for PersistentVec<T> {
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
        }
    }
}
impl<T> Default for PersistentVec<T> {
    fn default() -> Self {
        Self { root: None }
    }
}
impl<T: std::fmt::Debug> std::fmt::Debug for PersistentVec<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}
impl<T> PersistentVec<T> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        self.root.as_ref().map_or(0, |n| n.len)
    }
    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }
    pub fn ptr_eq(&self, other: &Self) -> bool {
        match (&self.root, &other.root) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
    pub fn owner(&self, index: usize) -> Option<&Arc<T>> {
        self.root.as_ref()?.owner(index)
    }
    pub fn get(&self, index: usize) -> Option<&T> {
        self.owner(index).map(Arc::as_ref)
    }
    pub fn first(&self) -> Option<&T> {
        self.get(0)
    }
    pub fn last(&self) -> Option<&T> {
        self.len().checked_sub(1).and_then(|i| self.get(i))
    }
    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            nodes: self.root.iter().map(Arc::as_ref).collect(),
            remaining: self.len(),
        }
    }
    /// Borrow a range by descending only its boundary paths. Iteration then
    /// visits the retained subtrees in order without indexing each leaf.
    pub fn range(&self, range: std::ops::Range<usize>) -> Iter<'_, T> {
        assert!(
            range.start <= range.end && range.end <= self.len(),
            "persistent sequence range out of bounds"
        );
        let mut nodes = VecDeque::new();
        if range.start != range.end {
            collect_range(
                self.root.as_deref().expect("nonempty range"),
                range.start,
                range.end,
                &mut nodes,
            );
        }
        Iter {
            nodes,
            remaining: range.end - range.start,
        }
    }
    pub fn clear(&mut self) {
        self.root = None;
    }
    pub fn push(&mut self, value: T) {
        self.root = Some(match &self.root {
            Some(root) => Node::append(root, value),
            None => Node::leaf(value),
        });
    }
    /// Replace a leaf without cloning the discarded input. The returned owner
    /// remains immutable and may still be held by an earlier snapshot.
    pub fn replace(&mut self, index: usize, value: T) -> Arc<T> {
        self.set_owner(index, Arc::new(value))
    }
    pub fn set_owner(&mut self, index: usize, value: Arc<T>) -> Arc<T> {
        let old = self
            .owner(index)
            .expect("persistent sequence index out of bounds");
        if Arc::ptr_eq(old, &value) {
            return old.clone();
        }
        Arc::make_mut(self.root.as_mut().expect("nonempty sequence"))
            .set_owner(index, value)
            .expect("checked index")
    }
    pub fn push_owner(&mut self, value: Arc<T>) {
        self.root = Some(match &self.root {
            Some(root) => Node::append_owner(root, value),
            None => Node::leaf_owner(value),
        });
    }
    /// Complete changed-owner positions, including removed indices. It skips
    /// identical subtrees; a caller-supplied dirty list is not trusted.
    pub fn changed_indices(&self, previous: &Self) -> Vec<usize> {
        let mut result = Vec::new();
        match (&self.root, &previous.root) {
            (Some(a), Some(b)) => Node::diff(a, b, 0, &mut result),
            _ => result.extend(0..self.len().max(previous.len())),
        }
        result
    }
}
impl<T: PersistentItem> PersistentVec<T> {
    /// Fingerprint of the exact leaf, including the structural leaf domain.
    /// Reuses the same memo populated by the sequence root fingerprint.
    pub fn item_fingerprint(&self, mut index: usize) -> Option<Fingerprint> {
        let mut node = self.root.as_deref()?;
        if index >= node.len {
            return None;
        }
        loop {
            match &node.kind {
                Kind::Leaf(_) => return Some(node.fingerprint()),
                Kind::Branch(left, right) => {
                    if index < left.len {
                        node = left;
                    } else {
                        index -= left.len;
                        node = right;
                    }
                }
            }
        }
    }
    pub fn structural_fingerprint(&self) -> Fingerprint {
        self.root.as_ref().map_or_else(
            || Fingerprint::of(b"ZIL-PERSISTENT-EMPTY-V1\0"),
            |root| root.fingerprint(),
        )
    }
}
impl<T: Clone> PersistentVec<T> {
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        Arc::make_mut(self.root.as_mut()?).get_mut(index)
    }
    pub fn first_mut(&mut self) -> Option<&mut T> {
        self.get_mut(0)
    }
    pub fn last_mut(&mut self) -> Option<&mut T> {
        let index = self.len().checked_sub(1)?;
        self.get_mut(index)
    }
    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        let remaining = self.len();
        IterMut {
            nodes: self.root.iter_mut().map(Arc::make_mut).collect(),
            remaining,
        }
    }
    /// Explicit compatibility materialization; never called by immutable reads.
    pub fn to_vec(&self) -> Vec<T> {
        self.iter().cloned().collect()
    }
    pub fn insert(&mut self, index: usize, value: T) {
        if index == self.len() {
            self.push(value);
        } else {
            let mut values = self.to_vec();
            values.insert(index, value);
            *self = values.into();
        }
    }
    pub fn remove(&mut self, index: usize) -> T {
        let mut values = self.to_vec();
        let result = values.remove(index);
        *self = values.into();
        result
    }
    pub fn pop(&mut self) -> Option<T> {
        let index = self.len().checked_sub(1)?;
        let value = self[index].clone();
        self.truncate(index);
        Some(value)
    }
    pub fn truncate(&mut self, len: usize) {
        if len == 0 {
            self.clear();
        } else if len < self.len() {
            self.root = Some(Node::prefix(
                self.root.as_ref().expect("nonempty prefix"),
                len,
            ));
        }
    }
    pub fn rotate_left(&mut self, mid: usize) {
        assert!(
            mid <= self.len(),
            "rotation exceeds persistent sequence length"
        );
        let mut values: Vec<T> = std::mem::take(self).into_iter().collect();
        values.rotate_left(mid);
        *self = values.into();
    }
    pub fn reverse(&mut self) {
        *self = self.iter().rev().cloned().collect();
    }
    pub fn retain(&mut self, mut keep: impl FnMut(&T) -> bool) {
        *self = self.iter().filter(|x| keep(x)).cloned().collect();
    }
    pub fn resize(&mut self, len: usize, value: T) {
        self.truncate(len);
        while self.len() < len {
            self.push(value.clone());
        }
    }
}
impl<T: PartialEq> PartialEq for PersistentVec<T> {
    fn eq(&self, other: &Self) -> bool {
        self.ptr_eq(other) || (self.len() == other.len() && self.iter().eq(other.iter()))
    }
}
impl<T: Eq> Eq for PersistentVec<T> {}
impl<T> Index<usize> for PersistentVec<T> {
    type Output = T;
    fn index(&self, index: usize) -> &T {
        self.get(index)
            .expect("persistent sequence index out of bounds")
    }
}
impl<T: Clone> IndexMut<usize> for PersistentVec<T> {
    fn index_mut(&mut self, index: usize) -> &mut T {
        self.get_mut(index)
            .expect("persistent sequence index out of bounds")
    }
}
impl<T> From<Vec<T>> for PersistentVec<T> {
    fn from(values: Vec<T>) -> Self {
        values.into_iter().collect()
    }
}
impl<T> FromIterator<T> for PersistentVec<T> {
    fn from_iter<I: IntoIterator<Item = T>>(values: I) -> Self {
        fn build<T>(values: &mut std::vec::IntoIter<T>, len: usize) -> Arc<Node<T>> {
            if len == 1 {
                return Node::leaf(values.next().expect("known sequence length"));
            }
            let left = len.next_power_of_two() / 2;
            Node::branch(build(values, left), build(values, len - left))
        }
        let values: Vec<_> = values.into_iter().collect();
        let len = values.len();
        Self {
            root: (len != 0).then(|| build(&mut values.into_iter(), len)),
        }
    }
}
impl<T> Extend<T> for PersistentVec<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, values: I) {
        for value in values {
            self.push(value);
        }
    }
}
impl<T: Clone> From<PersistentVec<T>> for Vec<T> {
    fn from(values: PersistentVec<T>) -> Self {
        values.into_iter().collect()
    }
}
pub struct IntoIter<T> {
    nodes: Vec<Arc<Node<T>>>,
    remaining: usize,
}
impl<T: Clone> Iterator for IntoIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        loop {
            let node = Arc::unwrap_or_clone(self.nodes.pop()?);
            match node.kind {
                Kind::Leaf(value) => {
                    self.remaining -= 1;
                    return Some(Arc::unwrap_or_clone(value));
                }
                Kind::Branch(left, right) => {
                    self.nodes.push(right);
                    self.nodes.push(left);
                }
            }
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}
impl<T: Clone> ExactSizeIterator for IntoIter<T> {}
impl<T: Clone> std::iter::FusedIterator for IntoIter<T> {}
impl<T: Clone> IntoIterator for PersistentVec<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        let remaining = self.len();
        IntoIter {
            nodes: self.root.into_iter().collect(),
            remaining,
        }
    }
}
impl<'a, T> IntoIterator for &'a PersistentVec<T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<'a, T: Clone> IntoIterator for &'a mut PersistentVec<T> {
    type Item = &'a mut T;
    type IntoIter = IterMut<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

fn collect_range<'a, T>(
    node: &'a Node<T>,
    start: usize,
    end: usize,
    nodes: &mut VecDeque<&'a Node<T>>,
) {
    if start == end {
        return;
    }
    if start == 0 && end == node.len {
        nodes.push_back(node);
        return;
    }
    let Kind::Branch(left, right) = &node.kind else {
        unreachable!()
    };
    if start < left.len {
        collect_range(left, start, end.min(left.len), nodes);
    }
    if end > left.len {
        collect_range(right, start.saturating_sub(left.len), end - left.len, nodes);
    }
}
pub struct Iter<'a, T> {
    nodes: VecDeque<&'a Node<T>>,
    remaining: usize,
}
impl<T> Clone for Iter<'_, T> {
    fn clone(&self) -> Self {
        Self {
            nodes: self.nodes.clone(),
            remaining: self.remaining,
        }
    }
}
impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match &self.nodes.pop_front()?.kind {
                Kind::Leaf(value) => {
                    self.remaining -= 1;
                    return Some(value);
                }
                Kind::Branch(left, right) => {
                    self.nodes.push_front(right);
                    self.nodes.push_front(left);
                }
            }
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}
impl<T> DoubleEndedIterator for Iter<'_, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        loop {
            match &self.nodes.pop_back()?.kind {
                Kind::Leaf(value) => {
                    self.remaining -= 1;
                    return Some(value);
                }
                Kind::Branch(left, right) => {
                    self.nodes.push_back(left);
                    self.nodes.push_back(right);
                }
            }
        }
    }
}
impl<T> ExactSizeIterator for Iter<'_, T> {}
impl<T> std::iter::FusedIterator for Iter<'_, T> {}
pub struct IterMut<'a, T> {
    nodes: VecDeque<&'a mut Node<T>>,
    remaining: usize,
}
impl<'a, T: Clone> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let node = self.nodes.pop_front()?;
            node.fingerprint.take();
            match &mut node.kind {
                Kind::Leaf(value) => {
                    self.remaining -= 1;
                    return Some(Arc::make_mut(value));
                }
                Kind::Branch(left, right) => {
                    self.nodes.push_front(Arc::make_mut(right));
                    self.nodes.push_front(Arc::make_mut(left));
                }
            }
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}
impl<T: Clone> ExactSizeIterator for IterMut<'_, T> {}
impl<T: Clone> std::iter::FusedIterator for IterMut<'_, T> {}

#[cfg(test)]
mod tests {
    use super::*;
    impl PersistentItem for u32 {
        fn persistent_fingerprint(&self) -> Fingerprint {
            Fingerprint::of(&self.to_le_bytes())
        }
    }
    #[test]
    fn edit_retains_unaffected_owners_and_invalidates_only_its_path() {
        let original: PersistentVec<u32> = (0..1025).collect();
        let original_hash = original.structural_fingerprint();
        let mut edited = original.clone();
        edited[512] = 9999;
        assert_eq!(edited.changed_indices(&original), vec![512]);
        assert!(Arc::ptr_eq(
            edited.owner(511).unwrap(),
            original.owner(511).unwrap()
        ));
        assert_ne!(edited.structural_fingerprint(), original_hash);
        assert_eq!(original[512], 512);
        edited[512] = 512;
        assert_eq!(edited.structural_fingerprint(), original_hash);
    }
    #[test]
    fn history_independent_order_and_mutation_identity() {
        let mut values: PersistentVec<u32> = (0..70).collect();
        values.insert(4, 999);
        values.remove(4);
        let pristine: PersistentVec<u32> = (0..70).collect();
        assert_eq!(
            values.structural_fingerprint(),
            pristine.structural_fingerprint()
        );
        assert_eq!(
            values.iter().rev().copied().collect::<Vec<_>>(),
            (0..70).rev().collect::<Vec<_>>()
        );
        let old = values.structural_fingerprint();
        for value in &mut values {
            *value += 1;
        }
        assert_ne!(old, values.structural_fingerprint());
        assert_eq!(
            values.structural_fingerprint(),
            (1..71)
                .collect::<PersistentVec<u32>>()
                .structural_fingerprint()
        );
    }
    #[test]
    fn borrowed_ranges_match_dense_parallel_batches() {
        let values: PersistentVec<u32> = (0..129).collect();
        for start in 0..=129 {
            for end in start..=129 {
                let range = values.range(start..end);
                assert_eq!(range.len(), end - start);
                assert_eq!(
                    range.clone().copied().collect::<Vec<_>>(),
                    (start as u32..end as u32).collect::<Vec<_>>()
                );
                assert_eq!(
                    range.rev().copied().collect::<Vec<_>>(),
                    (start as u32..end as u32).rev().collect::<Vec<_>>()
                );
            }
        }
    }
    #[test]
    fn append_delete_and_rollback_are_complete() {
        let original: PersistentVec<u32> = (0..8).collect();
        let mut next = original.clone();
        next.push(8);
        assert_eq!(next.changed_indices(&original), vec![8]);
        assert_eq!(original.changed_indices(&next), vec![8]);
        next.clear();
        assert_eq!(next.changed_indices(&original), (0..8).collect::<Vec<_>>());
        assert_eq!(original.len(), 8);
    }
}
