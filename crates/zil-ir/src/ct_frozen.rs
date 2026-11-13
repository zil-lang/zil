//! Untrusted binding of a selected CT executable to its immutable source view.
//! The request never embeds an authority bit. Verification must also receive
//! the complete frozen CoreProgram and the expected source-stage identity.
use crate::ct_artifact::CtStageIdentity;
use crate::*;
use zil_base::{ModuleId, NodeId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtFrozenContext {
    pub stage: CtStageIdentity,
    pub query: CtQueryOrigin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CtQueryOrigin {
    pub owner: CtQueryOwner,
    pub original: SemanticOriginalNode,
    pub expression: NodeId,
    pub occurrence: u32,
    pub wrapper: FunctionId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtQueryOwner {
    Instance(SemanticInstanceId),
    Definition(SemanticDefinitionId),
    Module(ModuleId),
}
