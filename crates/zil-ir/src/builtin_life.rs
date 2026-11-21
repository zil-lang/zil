//! Central identity allocation for compiler-distributed nominal declarations.
//! These IDs and `compiler` are untrusted identity data, never capabilities.
use crate::LifeDeclarationId;
use zil_base::{ModuleId, NodeId};

const fn id(declaration: u32) -> LifeDeclarationId {
    LifeDeclarationId {
        module: ModuleId(u32::MAX),
        declaration: NodeId(declaration),
        compiler: true,
    }
}

pub const OPTION: LifeDeclarationId = id(0);
pub const POLL: LifeDeclarationId = id(1);
pub const TASK_CREATE_FAILURE_RECORD: LifeDeclarationId = id(2);
pub const TASK_CREATE_FAILURE_PACKAGE: LifeDeclarationId = id(3);
pub const TASK_CREATE_ERROR: LifeDeclarationId = id(4);
pub const TASK_POLL_STATE: LifeDeclarationId = id(5);
