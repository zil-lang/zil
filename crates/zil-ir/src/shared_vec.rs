//! Shared raw sequences. Ownership is an allocation optimization, never proof.
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
