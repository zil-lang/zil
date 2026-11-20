//! Source→IR correspondence. typeck is its only producer; it is candidate
//! evidence that `zil-check::fidelity` re-derives from the original syntax
//! and declarations before trusting any relation.

use super::ids::{BlockId, FnId, Local};
use super::place::FieldRef;
use zil_base::{NodeId, Span};

/// Position of one operation inside a body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OpLocation {
    pub block: BlockId,
    /// Instruction index; `None` is the terminator.
    pub instruction: Option<u32>,
}

/// One key relation between a source node and the IR.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceRelation {
    /// A source binding is stored in `local`.
    Binding { node: NodeId, local: Local },
    /// A source field selection `base.name` resolved to `field`; the
    /// operation at `at` projects that field.
    FieldSelect {
        node: NodeId,
        field: FieldRef,
        at: OpLocation,
    },
    /// The n-th evaluated operand of a source expression is evaluated by the
    /// operation at `at`; ordering of `at` must follow source order.
    Evaluation {
        node: NodeId,
        ordinal: u32,
        at: OpLocation,
    },
    /// A source resource operation (`give`, `create`, `close`, `drop`).
    ResourceOp { node: NodeId, at: OpLocation },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionSourceMap {
    pub function: FnId,
    /// Source node of the function declaration.
    pub declaration: NodeId,
    pub relations: Vec<SourceRelation>,
    pub spans: Vec<(OpLocation, Span)>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceMap {
    pub functions: Vec<FunctionSourceMap>,
}
