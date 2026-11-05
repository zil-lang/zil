//! Frozen syntax sources are separate from output placement and permissions.
use std::sync::Arc;
use zil_base::{ModuleId, NodeId};
use zil_syntax::ast::Module;

/// Structural source identity. No diagnostic span or hygienic name enters it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SourceTreeIdentity {
    Original(ModuleId),
    Generated { module: ModuleId, event_path: Vec<u32> },
}
impl SourceTreeIdentity {
    pub fn module(&self) -> ModuleId {
        match self {
            Self::Original(module) | Self::Generated { module, .. } => *module,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceDeclarationSite {
    pub tree: SourceTreeIdentity,
    pub declaration: NodeId,
    /// Actual item/member structural path to the emission in that source tree.
    pub event_path: Vec<u32>,
}

/// One parse+hygiene result, frozen before diagnostic span remapping. The
/// inventory maps SourceIndex local declaration ordinals to distinct output
/// module IDs, including fields/members, never to the emitter's NodeId.
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedSourceTree {
    pub identity: SourceTreeIdentity,
    pub emitter: SourceDeclarationSite,
    pub module: Arc<Module>,
    pub declarations: Vec<NodeId>,
}
impl GeneratedSourceTree {
    /// Address encoding only. A matching digest never authenticates the tree,
    /// emitter, declaration inventory, or any permission.
    pub fn generation_fingerprint(&self) -> Option<zil_base::Fingerprint> {
        let SourceTreeIdentity::Generated { event_path, .. } = &self.identity else {
            return None;
        };
        let mut hash = zil_base::FingerprintBuilder::new();
        hash.update(b"ZIL_GENERATED_SOURCE_TREE_V1");
        for part in event_path { hash.update(&part.to_le_bytes()); }
        Some(hash.finish())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedDeclarationOrigin {
    pub tree: Arc<GeneratedSourceTree>,
    /// Root ordinal in the frozen ItemList, independent of output placement.
    pub root: u32,
}

/// Request-local rows only address immutable trees. The row number is not a
/// semantic identity and confers no trust in the contained raw descriptors.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GeneratedSourceCatalog {
    pub trees: Vec<Arc<GeneratedSourceTree>>,
}
impl GeneratedSourceCatalog {
    pub fn get(&self, row: u32) -> Option<&Arc<GeneratedSourceTree>> {
        self.trees.get(row as usize)
    }
    pub fn find(&self, identity: &SourceTreeIdentity) -> Option<&Arc<GeneratedSourceTree>> {
        self.trees.iter().find(|tree| &tree.identity == identity)
    }
}
