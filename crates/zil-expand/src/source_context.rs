//! Source declaration lineage at the Driver -> Expand boundary. Names below
//! are raw identity data; they never establish visibility, builtin or loan facts.
use std::collections::BTreeMap;

use zil_base::{ModuleId, NodeId};
use zil_ir::LifeDeclarationIdentity;

/// One actual output item retains its original source declaration, or an
/// explicitly allocated generated declaration. Vector position is current AST
/// placement, not a source declaration identity. Spans are diagnostics only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclarationOrigin {
    pub declaration: NodeId,
    pub identity: LifeDeclarationIdentity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleDeclarationOrigins {
    pub module: ModuleId,
    /// Aligned with the actual module item vector, including imports until the
    /// same transformation removes both. No spelling/span reconstruction.
    pub items: Vec<DeclarationOrigin>,
}

/// Borrowed actual request context. Constructing this view allocates nothing.
/// The producer supplies its package namespace and logical module map; a public
/// SourceUnit path alone never certifies canonical filesystem provenance.
#[derive(Clone, Copy)]
pub enum DeclarationPlacement<'a> {
    Original,
    Aligned(&'a [ModuleDeclarationOrigins]),
    /// A transforming producer has not supplied lineage. Consumers needing
    /// declaration identity fail closed rather than renumber the resulting AST.
    Unavailable,
}

#[derive(Clone, Copy)]
pub struct SourceIdentityContext<'a> {
    pub package: &'a str,
    pub root: ModuleId,
    pub modules: &'a BTreeMap<Vec<String>, ModuleId>,
    /// None is valid only for unchanged original source AST. Selection and CT
    /// materialization must supply the explicit aligned lineage after mutation.
    pub declarations: DeclarationPlacement<'a>,
}
