//! Compile-time evaluator (decision A2): CT runs as an interpreter over
//! already-verified body IR.
//!
//! Contract: `spec/source-lowering.md` §1 (CT: “`#(...)` 与 meta 项在已核验的
//! Body IR 上解释求值，与运行期共用同一份 IR 和内核验证”) and `_zil_design.md`
//! [`ZIL-COMPTIME`] (“编译期函数使用同一语言的已核控制流和数值规则，以受限 CTIR/VM
//! 执行……禁止泄出 VM 临时地址” and “预算不足为 ResourceLimit，不改变到运行时执行”).
//!
//! Admission is [`zil_check::VerifiedModule`] only; there is no entry point
//! taking a raw module. Every IR enumeration is matched exhaustively (the
//! crate denies wildcard arms), so a new operation is a compile error here.
//! Operations CT cannot perform return [`EvalError::Unsupported`]; nothing
//! is skipped silently.
#![forbid(unsafe_code)]
#![deny(clippy::wildcard_enum_match_arm)]

mod arith;
mod machine;

pub use arith::TARGET_POINTER_BITS;

use machine::{Cell, Machine};
use zil_check::VerifiedModule;
use zil_ir::body::{
    AdtKind, BlockId, ConstValue, FieldIdx, FloatTy, FnId, IntTy, TrapKind, TyId, TyKind, TyTable,
    VariantIdx,
};

/// A CT value. References never appear here: they are interpreter places
/// that must not escape the evaluation (an escaping reference is
/// [`EvalError::EscapingRef`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Unit,
    Bool(bool),
    /// Two's complement bits truncated to the width of `ty`.
    Int {
        ty: IntTy,
        bits: u128,
    },
    /// Raw IEEE bits (`f32` in the low 32 bits).
    Float {
        ty: FloatTy,
        bits: u64,
    },
    /// Record, package, enum variant, tuple, array or closure environment
    /// of type `ty`. `variant` is `Some` exactly for enums.
    Aggregate {
        ty: TyId,
        variant: Option<u32>,
        fields: Vec<Value>,
    },
    /// An error union of type `ty` in its success (`ok`) or error arm.
    ErrorUnion {
        ty: TyId,
        ok: bool,
        payload: Box<Value>,
    },
    /// A function pointer value.
    FnRef(FnId),
}

impl Value {
    /// The scalar constant of this value, if it is a scalar.
    pub fn to_const(&self) -> Option<ConstValue> {
        match self {
            Value::Unit => Some(ConstValue::Unit),
            Value::Bool(b) => Some(ConstValue::Bool(*b)),
            Value::Int { ty, bits } => Some(ConstValue::Int {
                ty: *ty,
                bits: *bits,
            }),
            Value::Float { ty, bits } => Some(ConstValue::Float {
                ty: *ty,
                bits: *bits,
            }),
            Value::Aggregate { .. } | Value::ErrorUnion { .. } | Value::FnRef(_) => None,
        }
    }

    pub fn from_const(value: ConstValue) -> Self {
        match value {
            ConstValue::Unit => Value::Unit,
            ConstValue::Bool(b) => Value::Bool(b),
            ConstValue::Int { ty, bits } => Value::Int { ty, bits },
            ConstValue::Float { ty, bits } => Value::Float { ty, bits },
        }
    }
}

/// Logical work quota of one evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvalBudget {
    /// Executed instructions plus executed terminators.
    pub steps: u64,
    /// Maximum number of simultaneously active frames (the entry counts).
    pub call_depth: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    Steps,
    CallDepth,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvalError {
    /// The program trapped (checked arithmetic, conversion, bounds, failed
    /// assertion or explicit trap) in `block` of `function`.
    Trap {
        kind: TrapKind,
        function: FnId,
        block: BlockId,
    },
    /// The budget ran out. Never a pass and never deferred to run time.
    ResourceLimit { resource: Resource },
    /// An operation the CT stage does not perform.
    Unsupported { what: &'static str, function: FnId },
    /// A state verified code cannot reach (a checker/interpreter defect).
    Internal { what: &'static str, function: FnId },
    /// The result contains a reference to interpreter storage.
    EscapingRef,
    /// The request does not match the entry function's signature.
    InvalidArgument { reason: &'static str },
}

/// Evaluates `function` of a verified module on `args`.
pub fn evaluate(
    module: &VerifiedModule,
    function: FnId,
    args: Vec<Value>,
    budget: EvalBudget,
) -> Result<Value, EvalError> {
    let m = module.module();
    let f = m
        .functions
        .get(function.index())
        .ok_or(EvalError::InvalidArgument {
            reason: "unknown function",
        })?;
    if f.sig.type_params != 0 {
        return Err(EvalError::Unsupported {
            what: "generic function body",
            function,
        });
    }
    if args.len() != f.sig.params.len() {
        return Err(EvalError::InvalidArgument {
            reason: "argument count differs from the signature",
        });
    }
    for (arg, ty) in args.iter().zip(&f.sig.params) {
        if !conforms(arg, *ty, m) {
            return Err(EvalError::InvalidArgument {
                reason: "argument does not inhabit the parameter type",
            });
        }
    }
    let cells = args.into_iter().map(Cell::from_value).collect();
    let mut machine = Machine::new(m, budget);
    let result = machine.run(function, cells)?;
    let value = result.into_value(function)?;
    if !conforms(&value, f.sig.result, m) {
        return Err(EvalError::Internal {
            what: "result does not inhabit the declared type",
            function,
        });
    }
    Ok(value)
}

/// Whether `value` is a well-formed inhabitant of `ty`. Types with no CT
/// value (addresses, resources, erased and opaque types) answer `false`.
fn conforms(value: &Value, ty: TyId, module: &zil_ir::body::Module) -> bool {
    conforms_in(value, ty, &module.tys, module, 0)
}

fn conforms_in(
    value: &Value,
    ty: TyId,
    tys: &TyTable,
    module: &zil_ir::body::Module,
    depth: u32,
) -> bool {
    const LIMIT: u32 = 256;
    if depth > LIMIT {
        return false;
    }
    let Some(kind) = tys.get(ty) else {
        return false;
    };
    let each = |values: &[Value], types: &[TyId]| {
        values.len() == types.len()
            && values
                .iter()
                .zip(types)
                .all(|(v, t)| conforms_in(v, *t, tys, module, depth + 1))
    };
    match kind {
        TyKind::Unit => matches!(value, Value::Unit),
        TyKind::Bool => matches!(value, Value::Bool(_)),
        TyKind::Int(i) => {
            matches!(value, Value::Int { ty, bits } if ty == i && *bits & !arith::mask(*i) == 0)
        }
        TyKind::Float(f) => match value {
            Value::Float { ty, bits } if ty == f => match f {
                FloatTy::F32 => *bits <= u64::from(u32::MAX),
                FloatTy::F64 => true,
            },
            Value::Float { .. }
            | Value::Unit
            | Value::Bool(_)
            | Value::Int { .. }
            | Value::Aggregate { .. }
            | Value::ErrorUnion { .. }
            | Value::FnRef(_) => false,
        },
        TyKind::Tuple(items) => match value {
            Value::Aggregate {
                ty: t,
                variant: None,
                fields,
            } if *t == ty => each(fields, items),
            Value::Aggregate { .. }
            | Value::Unit
            | Value::Bool(_)
            | Value::Int { .. }
            | Value::Float { .. }
            | Value::ErrorUnion { .. }
            | Value::FnRef(_) => false,
        },
        TyKind::Array { element, length } => match value {
            Value::Aggregate {
                ty: t,
                variant: None,
                fields,
            } if *t == ty => {
                fields.len() == *length as usize
                    && fields
                        .iter()
                        .all(|v| conforms_in(v, *element, tys, module, depth + 1))
            }
            Value::Aggregate { .. }
            | Value::Unit
            | Value::Bool(_)
            | Value::Int { .. }
            | Value::Float { .. }
            | Value::ErrorUnion { .. }
            | Value::FnRef(_) => false,
        },
        TyKind::Adt { adt, args } => {
            let Value::Aggregate {
                ty: t,
                variant,
                fields,
            } = value
            else {
                return false;
            };
            let Some(def) = tys.get_adt(*adt) else {
                return false;
            };
            let v = match (def.kind, variant) {
                (AdtKind::Enum, Some(v)) => *v,
                (AdtKind::Record | AdtKind::Package, None) => 0,
                (AdtKind::Enum, None)
                | (AdtKind::Record | AdtKind::Package, Some(_))
                | (AdtKind::Resource, _) => return false,
            };
            let Some(vdef) = def.variant(VariantIdx(v)) else {
                return false;
            };
            let mut types = Vec::with_capacity(vdef.fields.len());
            for i in 0..vdef.fields.len() {
                match tys.field_ty(*adt, VariantIdx(v), FieldIdx::from_index(i), *args) {
                    Some(f) => types.push(f),
                    None => return false,
                }
            }
            *t == ty && each(fields, &types)
        }
        TyKind::Closure { def, args } => {
            let Value::Aggregate {
                ty: t,
                variant: None,
                fields,
            } = value
            else {
                return false;
            };
            let Some(closure) = tys.closure(*def) else {
                return false;
            };
            let mut types = Vec::with_capacity(closure.captures.len());
            for c in &closure.captures {
                match tys.subst_lookup(*c, *args) {
                    Some(c) => types.push(c),
                    None => return false,
                }
            }
            *t == ty && each(fields, &types)
        }
        TyKind::ErrorUnion { ok, err } => match value {
            Value::ErrorUnion {
                ty: t,
                ok: arm,
                payload,
            } if *t == ty => conforms_in(
                payload,
                if *arm { *ok } else { *err },
                tys,
                module,
                depth + 1,
            ),
            Value::ErrorUnion { .. }
            | Value::Unit
            | Value::Bool(_)
            | Value::Int { .. }
            | Value::Float { .. }
            | Value::Aggregate { .. }
            | Value::FnRef(_) => false,
        },
        TyKind::FnPtr { params, result } => match value {
            Value::FnRef(f) => module.functions.get(f.index()).is_some_and(|f| {
                f.sig.type_params == 0 && f.sig.params == *params && f.sig.result == *result
            }),
            Value::Unit
            | Value::Bool(_)
            | Value::Int { .. }
            | Value::Float { .. }
            | Value::Aggregate { .. }
            | Value::ErrorUnion { .. } => false,
        },
        TyKind::Never
        | TyKind::Slice(_)
        | TyKind::Ref { .. }
        | TyKind::Managed(_)
        | TyKind::Desc(_)
        | TyKind::Dyn(_)
        | TyKind::OwnedDyn(_)
        | TyKind::Param(_)
        | TyKind::Opaque(_) => false,
    }
}
