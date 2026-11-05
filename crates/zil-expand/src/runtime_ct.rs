//! Runtime specialization facts passed to the CT phase. These are untrusted
//! recipes: the CT consumer must recover the declaration and call from its
//! immutable source trees before accepting any actual argument.
use zil_base::{CompileResult, Limits, ModuleId};
use zil_syntax::ast;

use crate::source_context::DeclarationOrigin;

/// Scheduling only: these roots need no specialization binder lookup. Keep
/// their original MetaEval and expected-type context in the ordinary scalar
/// checker; this predicate grants no declaration or expression-site authority.
pub fn native_scalar_root(
    mut expression: &ast::Expr,
    max_depth: usize,
    mut pay: impl FnMut(usize) -> CompileResult<()>,
) -> CompileResult<bool> {
    for depth in 0..=max_depth {
        pay(1)?;
        match &expression.kind {
            ast::ExprKind::Literal(
                ast::Literal::Bool(_) | ast::Literal::Integer(_) | ast::Literal::Float(_),
            ) => return Ok(true),
            ast::ExprKind::Group(inner) if depth < max_depth => expression = inner,
            ast::ExprKind::Group(_) => {
                return Err(vec![zil_base::Diagnostic::limit(
                    zil_base::Phase::Expand,
                    "native scalar CT root depth exhausted",
                    Some(expression.span),
                )]);
            }
            _ => return Ok(false),
        }
    }
    unreachable!()
}

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

/// Call ordinal counts runtime calls in source order, excluding
/// type syntax and CT subtrees. Normalizing a type cannot insert a call event.
#[derive(Clone, Debug)]
pub struct RuntimeCallSite {
    pub caller: RuntimeDefinition,
    pub parent_instance: Option<u32>,
    pub call_ordinal: u32,
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
    /// Recheck a complete ordinary generic host against a successful actual CT
    /// call and immutable source. The syntax and descriptor are untrusted;
    /// implementations check all sibling definitions, including unused ones.
    /// This never authorizes a runtime instance of the host.
    fn check_static_host(
        &self,
        _definition: &RuntimeDefinition,
        actual: &ast::Item,
        _remaining: &mut Limits,
    ) -> CompileResult<()> {
        Err(vec![zil_base::Diagnostic::unsupported(
            zil_base::Phase::Expand,
            "generic resource or methods require capability resolution",
            Some(actual.span),
        )])
    }
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
        ledger: &[RuntimeInstance],
        template: &ast::Item,
        output: &mut ast::Item,
        remaining: &mut Limits,
    ) -> CompileResult<()>;
}

pub enum RuntimeCallEvent<'a> {
    Work(usize),
    Call {
        expression: &'a ast::Expr,
        ordinal: u32,
    },
}

/// Shared structural selector. This selects an untrusted source address only;
/// the CT consumer still resolves the callee and all actuals independently.
pub fn observe_runtime_calls(
    item: &ast::Item,
    max_depth: usize,
    mut observe: impl FnMut(RuntimeCallEvent<'_>) -> CompileResult<()>,
) -> CompileResult<()> {
    struct Walk<'a, F> {
        observe: &'a mut F,
        next: u32,
        max_depth: usize,
    }
    impl<F: FnMut(RuntimeCallEvent<'_>) -> CompileResult<()>> crate::ct_selection::Visitor
        for Walk<'_, F>
    {
        fn node(&mut self, span: zil_base::Span, depth: usize) -> CompileResult<()> {
            if depth > self.max_depth {
                return Err(vec![zil_base::Diagnostic::limit(
                    zil_base::Phase::Expand,
                    "runtime call source depth exhausted",
                    Some(span),
                )]);
            }
            (self.observe)(RuntimeCallEvent::Work(1))
        }
        fn text(&mut self, text: &str, _: zil_base::Span) -> CompileResult<()> {
            (self.observe)(RuntimeCallEvent::Work(text.len()))
        }
        fn ty(&mut self, ty: &ast::TypeRef, depth: usize) -> CompileResult<()> {
            self.node(ty.span, depth)
        }
        fn expression(&mut self, expression: &ast::Expr, depth: usize) -> CompileResult<()> {
            self.node(expression.span, depth)?;
            match &expression.kind {
                ast::ExprKind::MetaEval(_) | ast::ExprKind::MetaBlock(_) => Ok(()),
                ast::ExprKind::Call { callee, arguments } => {
                    {
                        let ordinal = self.next;
                        self.next = self.next.checked_add(1).ok_or_else(|| {
                            vec![zil_base::Diagnostic::limit(
                                zil_base::Phase::Expand,
                                "runtime call source ordinal exhausted",
                                Some(expression.span),
                            )]
                        })?;
                        (self.observe)(RuntimeCallEvent::Call {
                            expression,
                            ordinal,
                        })?;
                    }
                    self.expression(callee, depth + 1)?;
                    for argument in arguments {
                        self.expression(&argument.value, depth + 1)?;
                    }
                    Ok(())
                }
                _ => crate::ct_selection::walk_expression(self, expression, depth),
            }
        }
    }
    crate::ct_selection::Visitor::item(
        &mut Walk {
            observe: &mut observe,
            next: 0,
            max_depth,
        },
        item,
        0,
    )
}
