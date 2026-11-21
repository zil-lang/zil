//! Untrusted stage requests and scoped Type descriptors. Readiness and source
//! provenance must be re-established against an immutable frozen authority.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtStageIdentity {
    pub source_fingerprint: Fingerprint,
    pub epoch: u64,
    pub committed_generation_fingerprint: Fingerprint,
    pub declaration_inventory_fingerprint: Fingerprint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtStageFacet {
    Shape,
    Contract,
    Layout,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtStageSubject {
    Declaration(CtSymbolId),
    Type(CtTypeRef),
    Instance {
        declaration: CtSymbolId,
        context: CtTypeRef,
        arguments: Vec<CtTypeArgument>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtBodySourceRef {
    pub declaration: CtSymbolId,
    pub body_fingerprint: Fingerprint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtStageFacetRequest {
    pub subject: CtStageSubject,
    pub facet: CtStageFacet,
    pub content_fingerprint: Option<Fingerprint>,
    pub body_source: Option<CtBodySourceRef>,
}

/// Pure descriptor storage, not an implementation/provider index. Type keys
/// resolve complete reachable declarations and intrinsic scopes; numeric IDs
/// and the surrounding catalogue inventory are not semantic Type identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtTypeCatalogue {
    pub stage: CtStageIdentity,
    pub types: Vec<CtDynamicTypeNode>,
    pub interfaces: Vec<CtDynInterface>,
}
