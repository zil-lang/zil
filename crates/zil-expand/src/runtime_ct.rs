//! Runtime specialization facts passed to the CT phase. These are untrusted
//! recipes: the CT consumer must recover the declaration and call from its
//! immutable source trees before accepting any actual argument.
use zil_base::{CompileResult, Limits, ModuleId};
use zil_syntax::ast;

use crate::source_context::DeclarationOrigin;

/// One slot in the original declaration's complete generic parameter order.
/// Runtime specialization retains lifetime polymorphism; a lifetime slot names
/// its original definition binder, never an inferred caller loan.
#[derive(Clone, Debug)]
pub enum RuntimeArgument {
    Type(ast::TypeRef),
    Const(ast::Expr),
    LifeDefinition { ordinal: u32 },
}

#[derive(Clone, Debug)]
pub struct RuntimeDefinition {
    pub module: ModuleId,
    pub origin: DeclarationOrigin,
    pub member: Option<u32>,
}

/// Expression ordinal is within the original declaration, in the public
/// ct_selection visitor's expression preorder. It is not a byte offset.
#[derive(Clone, Debug)]
pub struct RuntimeCallSite {
    pub caller: RuntimeDefinition,
    pub parent_instance: Option<u32>,
    pub expression_ordinal: u32,
}

#[derive(Clone, Debug)]
pub struct RuntimeInstance {
    pub id: u32,
    pub definition: RuntimeDefinition,
    pub arguments: Vec<RuntimeArgument>,
    pub call: RuntimeCallSite,
}

/// Called after actual substitution and before recursively expanding the
/// resulting body. Implementations must authenticate original source sites,
/// remove all deferred CT expressions, and debit the supplied remaining limits.
/// The template and instance are syntax, not certificates.
pub trait RuntimeCt {
    /// Check the actual original CT expression under symbolic declaration
    /// binders, even when the runtime function is never instantiated. The
    /// returned type is computed from that expression, not its receiving slot.
    fn definition_expression(
        &self,
        definition: &RuntimeDefinition,
        expression_ordinal: u32,
        remaining: &mut Limits,
    ) -> CompileResult<ast::TypeRef>;

    fn instantiated(
        &self,
        instance: &RuntimeInstance,
        template: &ast::Item,
        output: &mut ast::Item,
        remaining: &mut Limits,
    ) -> CompileResult<()>;
}
