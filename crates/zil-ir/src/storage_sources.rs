//! Untrusted source expressions. A variant names a proof obligation, never a
//! source, a region, a permission or a lifetime by itself.
use crate::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageSource {
    /// The existing complete leaf Type and binder must remain identical.
    Exact(StoragePosition),
    /// Address of a real physical place, distinct from loading its contents.
    PlaceAddress { place: StoragePosition, mode: StoredBorrowMode },
    /// A real owning head's reachable owned region. Address/Desc edges stop it.
    /// `leaf` is the complete borrowed leaf Type, including its qualification.
    OwnedView { head: StoragePosition, leaf: Type, mode: StoredBorrowMode },
    /// Preserve the original pointee Fixed classes; derive only the child
    /// invocation upper bound from the actual selected head signature.
    ReborrowedLeaf { source: StoragePosition, head: StoragePosition },
    /// Explicit static provenance. An empty source list never means Static.
    Static { leaf: Type, mode: StoredBorrowMode },
}
