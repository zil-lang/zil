//! Move-path table: the tracked inline places of one body.
//!
//! A place is tracked up to (excluding) its first `Deref`, `Index` or
//! `ConstIndex` step; storage behind an indirection or inside an array
//! element is never moved out of or strongly updated individually. The table
//! is built deterministically from the body alone, so a solver and a checker
//! that build it independently obtain the same numbering, and certificate
//! claims are mapped through [`MovePaths::lookup`].

use std::collections::HashMap;
use zil_ir::body::{Body, Place, PlaceRoot, Projection};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PathId(pub u32);

impl PathId {
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Debug)]
pub struct MovePaths {
    places: Vec<Place>,
    parent: Vec<Option<PathId>>,
    /// Paths are numbered in preorder; `end[p]` is one past the last
    /// descendant, so a subtree is the contiguous range `p..end[p]`.
    end: Vec<u32>,
    index: HashMap<Place, PathId>,
}

/// Number of leading steps of `place` that address inline storage.
pub fn tracked_len(place: &Place) -> usize {
    place
        .projections
        .iter()
        .position(|p| match p {
            Projection::Deref | Projection::Index(_) | Projection::ConstIndex(_) => true,
            Projection::Field(_) | Projection::TupleField(_) | Projection::ErrorArm(_) => false,
        })
        .unwrap_or(place.projections.len())
}

impl MovePaths {
    pub fn build(body: &Body) -> Self {
        let mut set: Vec<Place> = Vec::new();
        let mut add = |place: &Place| {
            for len in 0..=tracked_len(place) {
                set.push(place.prefix(len));
            }
        };
        for i in 0..body.locals.len() {
            add(&Place::local(zil_ir::body::Local::from_index(i)));
        }
        for i in 0..body.captures.len() {
            add(&Place::capture(i as u32));
        }
        for block in &body.blocks {
            for inst in &block.instructions {
                for a in inst.kind.accesses() {
                    add(&a.place);
                }
            }
            for a in block.terminator.accesses() {
                add(&a.place);
            }
            for (kind, _) in block.terminator.edges() {
                for a in block.terminator.edge_accesses(kind) {
                    add(&a.place);
                }
            }
        }
        // Lexicographic order of (root, steps) is a preorder of the tree.
        set.sort();
        set.dedup();
        let mut index = HashMap::with_capacity(set.len());
        for (i, p) in set.iter().enumerate() {
            index.insert(p.clone(), PathId(i as u32));
        }
        let parent: Vec<Option<PathId>> = set
            .iter()
            .map(|p| p.parent().and_then(|q| index.get(&q).copied()))
            .collect();
        let mut end: Vec<u32> = (1..=set.len() as u32).collect();
        for i in (0..set.len()).rev() {
            if let Some(p) = parent[i] {
                end[p.index()] = end[p.index()].max(end[i]);
            }
        }
        MovePaths {
            places: set,
            parent,
            end,
            index,
        }
    }

    pub fn len(&self) -> usize {
        self.places.len()
    }
    pub fn is_empty(&self) -> bool {
        self.places.is_empty()
    }
    pub fn place(&self, path: PathId) -> &Place {
        &self.places[path.index()]
    }
    pub fn parent(&self, path: PathId) -> Option<PathId> {
        self.parent[path.index()]
    }
    /// Exact lookup of a tracked place.
    pub fn lookup(&self, place: &Place) -> Option<PathId> {
        self.index.get(place).copied()
    }
    /// The tracked inline prefix of an accessed place.
    pub fn tracked(&self, place: &Place) -> Option<PathId> {
        self.index.get(&place.prefix(tracked_len(place))).copied()
    }
    pub fn root(&self, root: PlaceRoot) -> Option<PathId> {
        self.lookup(&Place {
            root,
            projections: Vec::new(),
        })
    }
    /// `path` and all its descendants.
    pub fn subtree(&self, path: PathId) -> impl Iterator<Item = PathId> {
        (path.0..self.end[path.index()]).map(PathId)
    }
    /// Direct children of `path`.
    pub fn children(&self, path: PathId) -> impl Iterator<Item = PathId> + '_ {
        self.subtree(path)
            .skip(1)
            .filter(move |c| self.parent(*c) == Some(path))
    }
    /// Strict ancestors, nearest first.
    pub fn ancestors(&self, path: PathId) -> impl Iterator<Item = PathId> + '_ {
        std::iter::successors(self.parent(path), move |p| self.parent(*p))
    }
}
