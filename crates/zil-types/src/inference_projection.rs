//! Source-relative input bytes observed during the *current* bounded raw
//! inference walk. Never decoded from a cache and never a checking capability.
use super::*;
use std::hash::Hash;
use zil_base::{Fingerprint, FingerprintBuilder};

#[derive(Clone, Debug)]
pub(crate) struct BodyInput {
    pub digest: Fingerprint,
    pub calls: BTreeSet<(Option<String>, String)>,
}

pub(super) struct Projection {
    origin: Span,
    hash: FingerprintBuilder,
    calls: BTreeSet<(Option<String>, String)>,
    valid: bool,
}
impl Projection {
    pub fn new(origin: Span) -> Self {
        Self {
            origin,
            hash: FingerprintBuilder::new(),
            calls: BTreeSet::new(),
            valid: true,
        }
    }
    fn span(&mut self, span: Span) {
        if span.file != self.origin.file
            || span.start < self.origin.start
            || span.end > self.origin.end
            || span.start > span.end
        {
            self.valid = false;
            return;
        }
        (span.start - self.origin.start, span.end - self.origin.start).hash(&mut self.hash);
    }
    pub fn block(&mut self, block: &CoreBlock) {
        if !self.valid {
            return;
        }
        self.span(block.span);
        block.statements.len().hash(&mut self.hash);
    }
    pub fn statement(&mut self, statement: &CoreStatement) {
        if !self.valid {
            return;
        }
        self.span(statement.span);
        match &statement.kind {
            CoreStatementKind::Binding {
                name,
                mutable,
                ty,
                initializer,
            } => {
                0u8.hash(&mut self.hash);
                name.hash(&mut self.hash);
                mutable.hash(&mut self.hash);
                if ty.as_ref().is_some_and(|ty| !incremental::scalar(ty)) {
                    self.valid = false;
                    return;
                }
                ty.hash(&mut self.hash);
                initializer.is_some().hash(&mut self.hash);
            }
            CoreStatementKind::Assign {
                target: _,
                value: _,
            } => 1u8.hash(&mut self.hash),
            CoreStatementKind::Expression(_) => 2u8.hash(&mut self.hash),
            CoreStatementKind::Return(value) => {
                3u8.hash(&mut self.hash);
                value.is_some().hash(&mut self.hash);
            }
            CoreStatementKind::If {
                condition: _,
                then_body: _,
                else_body,
            } => {
                4u8.hash(&mut self.hash);
                else_body.is_some().hash(&mut self.hash);
            }
            CoreStatementKind::While {
                condition: _,
                body: _,
            } => 5u8.hash(&mut self.hash),
            CoreStatementKind::ForRange {
                name,
                start: _,
                end: _,
                body: _,
            } => {
                6u8.hash(&mut self.hash);
                name.hash(&mut self.hash);
            }
            CoreStatementKind::Block(_) => 7u8.hash(&mut self.hash),
            CoreStatementKind::Break => 8u8.hash(&mut self.hash),
            CoreStatementKind::Continue => 9u8.hash(&mut self.hash),
            _ => self.valid = false,
        }
    }
    pub fn expression(&mut self, expression: &CoreExpr) {
        if !self.valid {
            return;
        }
        self.span(expression.span);
        match &expression.kind {
            CoreExprKind::Literal(value) if incremental::scalar(&value.ty()) => {
                0u8.hash(&mut self.hash);
                value.hash(&mut self.hash);
            }
            CoreExprKind::Integer(text) => {
                1u8.hash(&mut self.hash);
                text.hash(&mut self.hash);
            }
            CoreExprKind::Bool(value) => {
                2u8.hash(&mut self.hash);
                value.hash(&mut self.hash);
            }
            CoreExprKind::Unit => 3u8.hash(&mut self.hash),
            CoreExprKind::Name(name) => {
                4u8.hash(&mut self.hash);
                name.hash(&mut self.hash);
            }
            CoreExprKind::Unary { op, operand: _ }
                if matches!(op, UnaryOp::Negate | UnaryOp::Not | UnaryOp::BitNot) =>
            {
                5u8.hash(&mut self.hash);
                op.hash(&mut self.hash);
            }
            CoreExprKind::Binary {
                op,
                left: _,
                right: _,
            } => {
                6u8.hash(&mut self.hash);
                op.hash(&mut self.hash);
            }
            CoreExprKind::IntegerConvert { target, value: _ } if incremental::scalar(target) => {
                7u8.hash(&mut self.hash);
                target.hash(&mut self.hash);
            }
            CoreExprKind::Ascribed { ty, value: _ } if incremental::scalar(ty) => {
                8u8.hash(&mut self.hash);
                ty.hash(&mut self.hash);
            }
            CoreExprKind::Call { callee, arguments } => {
                9u8.hash(&mut self.hash);
                arguments.len().hash(&mut self.hash);
                let call = match &callee.kind {
                    CoreExprKind::Name(name) => Some((None, name.clone())),
                    CoreExprKind::Member {
                        object,
                        member,
                        owner: None,
                    } => match &object.kind {
                        CoreExprKind::Name(alias) => Some((Some(alias.clone()), member.clone())),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(call) = call {
                    self.calls.insert(call);
                } else {
                    self.valid = false;
                }
            }
            CoreExprKind::Member {
                object,
                member,
                owner: None,
            } if matches!(object.kind, CoreExprKind::Name(_)) => {
                10u8.hash(&mut self.hash);
                member.hash(&mut self.hash);
            }
            CoreExprKind::If {
                condition: _,
                then_value: _,
                else_value: _,
            } => 11u8.hash(&mut self.hash),
            CoreExprKind::Float(text) => {
                12u8.hash(&mut self.hash);
                text.hash(&mut self.hash);
            }
            CoreExprKind::NumericConvert { target, value: _ } if incremental::scalar(target) => {
                13u8.hash(&mut self.hash);
                target.hash(&mut self.hash);
            }
            _ => self.valid = false,
        }
    }
    pub fn finish(self) -> Option<BodyInput> {
        self.valid.then(|| BodyInput {
            digest: self.hash.digest(),
            calls: self.calls,
        })
    }
}
