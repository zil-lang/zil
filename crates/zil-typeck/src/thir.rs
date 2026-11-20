//! Typed, name-resolved function bodies.
//!
//! Every expression carries its (inference) type; after
//! [`crate::infer`] finishes, every type is zonked to a closed form that
//! may still mention the declaration's own `Param`s. Place expressions are
//! `Local`, `Field`, `TupleField`, `Index` and `Deref`; every other kind is
//! a value expression.

use crate::ty::{AdtRef, ClosureRef, TraitRef, Ty};
use zil_base::Span;
use zil_ir::body::{BinOp, ClosureCallMode, ClosureKind, IntrinsicOp, UnOp};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LocalId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FnRef(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ForeignRef(pub u32);

#[derive(Clone, Debug)]
pub struct LocalInfo {
    pub name: String,
    pub ty: Ty,
    pub mutable: bool,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct ThirClosure {
    pub kind: ClosureKind,
    /// Types of the captured environment fields (in capture order).
    pub capture_tys: Vec<Ty>,
    pub locals: Vec<LocalInfo>,
    pub params: Vec<LocalId>,
    pub param_tys: Vec<Ty>,
    pub result: Ty,
    pub body: Expr,
}

#[derive(Clone, Debug)]
pub struct ThirFn {
    pub locals: Vec<LocalInfo>,
    pub params: Vec<LocalId>,
    pub body: Expr,
    pub closures: Vec<ThirClosure>,
    /// If this function returns `impl Fn/FnMut/FnOnce`, the concrete closure it returns.
    pub opaque_return: Option<ClosureRef>,
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
    /// Static UTF-8 byte slice literal (`&[u8]`).
    StaticUtf(Vec<u8>),
    Local(LocalId),
    /// The n-th captured environment field inside a closure body.
    Capture(u32),
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
    /// `*base`: the target of an address, `Managed` carrier or `Desc`.
    /// Automatic dereference of a member or index base produces the same node.
    Deref(Box<Expr>),
    /// `&place` / `&mut place`. A value operand is materialised in a
    /// temporary first.
    Borrow {
        mutable: bool,
        place: Box<Expr>,
    },
    /// `&place[start..end]` / `&mut place[start..end]`.
    SliceBorrow {
        mutable: bool,
        place: Box<Expr>,
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
    },
    /// `seq.len() -> i64`.
    Len(Box<Expr>),
    /// `give place`: explicit responsibility transfer out of a `Managed` place.
    Give(Box<Expr>),
    /// Produce a `Desc<R>` of a `Managed<R>` or `Desc<R>` place.
    Describe(Box<Expr>),
    /// Allocate a `Managed<R>` resource from its Copy payload fields.
    ManagedCreate {
        args: Vec<Ty>,
        fields: Vec<Expr>,
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
    FnRef(FnRef),
    Call {
        callee: FnRef,
        type_args: Vec<Ty>,
        args: Vec<Expr>,
    },
    IndirectCall {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Closure {
        id: ClosureRef,
        captures: Vec<Expr>,
    },
    ClosureCall {
        closure: Box<Expr>,
        mode: ClosureCallMode,
        args: Vec<Expr>,
    },
    TraitCall {
        trait_ref: TraitRef,
        trait_args: Vec<Ty>,
        self_ty: Ty,
        method: String,
        args: Vec<Expr>,
    },
    ForeignCall {
        callee: ForeignRef,
        args: Vec<Expr>,
    },
    Intrinsic {
        op: IntrinsicOp,
        args: Vec<Expr>,
    },
    /// A Record/Package value or an Enum variant; fields in declaration order.
    Adt {
        adt: AdtRef,
        variant: u32,
        args: Vec<Ty>,
        fields: Vec<Expr>,
    },
    /// Inject a value into the success (`ok = true`) or error (`ok = false`)
    /// arm of an error union.
    ErrorInject {
        ok: bool,
        value: Box<Expr>,
    },
    /// `expr?`: unwrap success or return error early.
    Try(Box<Expr>),
    /// `#(expr)`: compile-time evaluation.
    CtEval(Box<Expr>),
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
        | ExprKind::Capture(_)
        | ExprKind::Field { .. }
        | ExprKind::TupleField { .. }
        | ExprKind::Index { .. }
        | ExprKind::Deref(_) => true,
        ExprKind::Lit(_)
        | ExprKind::StaticUtf(_)
        | ExprKind::Borrow { .. }
        | ExprKind::SliceBorrow { .. }
        | ExprKind::Len(_)
        | ExprKind::Give(_)
        | ExprKind::Describe(_)
        | ExprKind::ManagedCreate { .. }
        | ExprKind::Unary { .. }
        | ExprKind::Binary { .. }
        | ExprKind::Logical { .. }
        | ExprKind::Convert { .. }
        | ExprKind::FnRef(_)
        | ExprKind::Call { .. }
        | ExprKind::IndirectCall { .. }
        | ExprKind::Closure { .. }
        | ExprKind::ClosureCall { .. }
        | ExprKind::TraitCall { .. }
        | ExprKind::ForeignCall { .. }
        | ExprKind::Intrinsic { .. }
        | ExprKind::Adt { .. }
        | ExprKind::ErrorInject { .. }
        | ExprKind::Try(_)
        | ExprKind::CtEval(_)
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
    /// `.ok(inner)` (`ok = true`) or `.err(inner)` (`ok = false`).
    ErrorArm {
        ok: bool,
        inner: Box<Pat>,
    },
}
