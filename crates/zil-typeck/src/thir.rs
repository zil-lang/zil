//! Typed, name-resolved function bodies.
//!
//! Every expression carries its (inference) type; after
//! [`crate::infer`] finishes, every type is zonked to a closed form that
//! may still mention the declaration's own `Param`s. Place expressions are
//! `Local`, `Field`, `TupleField`, `Index` and `Deref`; every other kind is
//! a value expression.

use crate::ty::{AdtRef, Ty};
use zil_base::Span;
use zil_ir::body::{BinOp, UnOp};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LocalId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FnRef(pub u32);

#[derive(Clone, Debug)]
pub struct LocalInfo {
    pub name: String,
    pub ty: Ty,
    pub mutable: bool,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct ThirFn {
    pub locals: Vec<LocalInfo>,
    pub params: Vec<LocalId>,
    pub body: Expr,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Lit {
    Unit,
    Bool(bool),
    /// Mathematical value; range-checked against the zonked type.
    Int(i128),
    /// IEEE bits of the value parsed as `f64`; narrowed when the type is f32.
    Float(f64),
}

#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub ty: Ty,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    Lit(Lit),
    Local(LocalId),
    Field {
        base: Box<Expr>,
        adt: AdtRef,
        variant: u32,
        field: u32,
        args: Vec<Ty>,
    },
    TupleField {
        base: Box<Expr>,
        index: u32,
    },
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    /// `*base`: the target of an address. Automatic dereference of a member
    /// or index base produces the same node.
    Deref(Box<Expr>),
    /// `&place` / `&mut place`. A value operand is materialised in a
    /// temporary first.
    Borrow {
        mutable: bool,
        place: Box<Expr>,
    },
    Unary {
        op: UnOp,
        operand: Box<Expr>,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// Short-circuit `&&` (`and = true`) or `||`.
    Logical {
        and: bool,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Convert {
        operand: Box<Expr>,
    },
    Call {
        callee: FnRef,
        type_args: Vec<Ty>,
        args: Vec<Expr>,
    },
    /// A Record/Package value or an Enum variant; fields in declaration order.
    Adt {
        adt: AdtRef,
        variant: u32,
        args: Vec<Ty>,
        fields: Vec<Expr>,
    },
    Tuple(Vec<Expr>),
    Array(Vec<Expr>),
    If {
        cond: Box<Expr>,
        then: Box<Expr>,
        otherwise: Option<Box<Expr>>,
    },
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<Arm>,
    },
    Block(Vec<Stmt>),
    Loop(Box<Expr>),
    While {
        cond: Box<Expr>,
        body: Box<Expr>,
    },
    Break,
    Continue,
    Return(Option<Box<Expr>>),
    /// `place = value`, or `place = place op value` for a compound
    /// assignment (the place is evaluated once).
    Assign {
        place: Box<Expr>,
        op: Option<BinOp>,
        value: Box<Expr>,
    },
}

/// Whether an expression denotes a place (storage) rather than a value.
pub fn is_place(kind: &ExprKind) -> bool {
    match kind {
        ExprKind::Local(_)
        | ExprKind::Field { .. }
        | ExprKind::TupleField { .. }
        | ExprKind::Index { .. }
        | ExprKind::Deref(_) => true,
        ExprKind::Lit(_)
        | ExprKind::Borrow { .. }
        | ExprKind::Unary { .. }
        | ExprKind::Binary { .. }
        | ExprKind::Logical { .. }
        | ExprKind::Convert { .. }
        | ExprKind::Call { .. }
        | ExprKind::Adt { .. }
        | ExprKind::Tuple(_)
        | ExprKind::Array(_)
        | ExprKind::If { .. }
        | ExprKind::Match { .. }
        | ExprKind::Block(_)
        | ExprKind::Loop(_)
        | ExprKind::While { .. }
        | ExprKind::Break
        | ExprKind::Continue
        | ExprKind::Return(_)
        | ExprKind::Assign { .. } => false,
    }
}

#[derive(Clone, Debug)]
pub enum Stmt {
    /// `let`/`var`: evaluate `init`, then bind by an irrefutable pattern.
    Let {
        pattern: Pat,
        init: Expr,
    },
    Expr(Expr),
}

#[derive(Clone, Debug)]
pub struct Arm {
    pub pattern: Pat,
    pub body: Expr,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Pat {
    pub kind: PatKind,
    pub ty: Ty,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum PatKind {
    Wild,
    Binding(LocalId),
    Lit(Lit),
    Tuple(Vec<Pat>),
    /// A variant of an Enum, or the single variant of a Record/Package;
    /// one sub-pattern per declared field in declaration order.
    Adt {
        adt: AdtRef,
        variant: u32,
        args: Vec<Ty>,
        fields: Vec<Pat>,
    },
}
