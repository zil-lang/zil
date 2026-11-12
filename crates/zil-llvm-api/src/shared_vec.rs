//! Shared raw machine sequences. Allocation ownership confers no authority.
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

/// An immutable shared sequence with copy-on-write mutation. Cloning an IR
/// function can retain its large arrays; changing raw input makes a private
/// copy. No verifier capability is stored in this container.
#[derive(Clone, Debug)]
pub struct SharedVec<T>(Arc<Vec<T>>);
impl<T> SharedVec<T> {
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
// Inherent mutators preserve Vec's two-phase receiver borrows (for example
// `values.push(values[0].clone())`) before copy-on-write makes the array unique.
impl<T: Clone> SharedVec<T> {
    pub fn push(&mut self, value: T) {
        Arc::make_mut(&mut self.0).push(value);
    }
    pub fn resize(&mut self, len: usize, value: T) {
        Arc::make_mut(&mut self.0).resize(len, value);
    }
}
impl<T> Default for SharedVec<T> {
    fn default() -> Self {
        Self(Arc::new(Vec::new()))
    }
}
impl<T> From<Vec<T>> for SharedVec<T> {
    fn from(value: Vec<T>) -> Self {
        Self(Arc::new(value))
    }
}
impl<T: Clone> From<SharedVec<T>> for Vec<T> {
    fn from(value: SharedVec<T>) -> Self {
        Arc::unwrap_or_clone(value.0)
    }
}
impl<T> FromIterator<T> for SharedVec<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Vec::from_iter(iter).into()
    }
}
impl<T: PartialEq> PartialEq for SharedVec<T> {
    fn eq(&self, other: &Self) -> bool {
        self.ptr_eq(other) || self.0 == other.0
    }
}
impl<T: Eq> Eq for SharedVec<T> {}
impl<T> Deref for SharedVec<T> {
    type Target = Vec<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<T: Clone> SharedVec<T> {
    pub fn reserve(&mut self, additional: usize) {
        Arc::make_mut(&mut self.0).reserve(additional);
    }
}
impl<T: Clone> DerefMut for SharedVec<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::make_mut(&mut self.0)
    }
}
impl<T> AsRef<[T]> for SharedVec<T> {
    fn as_ref(&self) -> &[T] {
        self
    }
}
impl<T: Clone> IntoIterator for SharedVec<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        Vec::from(self).into_iter()
    }
}
impl<'a, T> IntoIterator for &'a SharedVec<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<'a, T: Clone> IntoIterator for &'a mut SharedVec<T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<T: std::hash::Hash> std::hash::Hash for SharedVec<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(self.0.as_ref(), state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static CLONES: AtomicUsize = AtomicUsize::new(0);
    #[derive(Debug, PartialEq, Eq)]
    struct Value(u32);
    impl Clone for Value {
        fn clone(&self) -> Self {
            CLONES.fetch_add(1, Ordering::SeqCst);
            Self(self.0)
        }
    }
    #[test]
    fn immutable_clone_keeps_elements_and_mutation_makes_a_private_copy() {
        CLONES.store(0, Ordering::SeqCst);
        let original: SharedVec<_> = vec![Value(1), Value(2)].into();
        let mut clone = original.clone();
        assert!(original.ptr_eq(&clone));
        assert_eq!(CLONES.load(Ordering::SeqCst), 0);
        clone.push(Value(3));
        assert!(!original.ptr_eq(&clone));
        assert_eq!(CLONES.load(Ordering::SeqCst), 2);
        assert_eq!(original.len(), 2);
        assert_eq!(clone.len(), 3);
    }
    #[test]
    fn shared_sequence_hash_preserves_the_vec_byte_stream() {
        let values = vec![1u32, 2, 3];
        let shared: SharedVec<_> = values.clone().into();
        let mut old = std::collections::hash_map::DefaultHasher::new();
        let mut current = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&values, &mut old);
        std::hash::Hash::hash(&shared, &mut current);
        assert_eq!(
            std::hash::Hasher::finish(&old),
            std::hash::Hasher::finish(&current)
        );
    }
}
