//! The compiler's closed core-operation namespace. A spelling alone never
//! resolves an operation: the complete original lexical namespace is checked
//! first, and every selected operation is rechecked against its actual opcode.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::semantic) enum Intrinsic {
    NumericConvert(Number),
    SliceLength,
    TakeStored,
    OwnedDyn,
    CCallback,
    TaskYield,
    TaskAwait,
    TaskCreate,
    TaskCreateOwned,
    TaskValid,
    TaskPoll,
    TaskPollClose,
    TaskCancel,
    TaskFinish,
    BytesNew,
    BytesReserve,
    BytesPush,
    BytesLen,
    BytesCapacity,
    BytesTruncate,
    BytesView { mutable: bool },
    StoreNew,
    StoreReserve,
    StorePush,
    StorePop,
    StoreAt,
    StoreAtMut,
    StoreLen,
    StoreCapacity,
    MutexCreate,
    MutexLock,
    MutexValue,
    SyncCellScope(SyncCellKind),
    SyncCellOp(SyncCellKind, SyncI32Op),
    SyncCellCompareExchange,
    ThreadScope,
    Register,
    RegisterAsync,
    ScopedForeignCallback,
    RegistrationValid,
    RegistrationClose,
    AsyncValid,
    AsyncRequest,
    AsyncPoll,
    AsyncFinish,
    AsyncFinishChecked,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::semantic) enum Number {
    I8,
    I16,
    I32,
    I64,
    Isize,
    U8,
    U16,
    U32,
    U64,
    Usize,
    F32,
    F64,
}
impl Number {
    pub(in crate::semantic) fn ty(self) -> Type {
        match self {
            Self::I8 => Type::I8,
            Self::I16 => Type::I16,
            Self::I32 => Type::I32,
            Self::I64 => Type::I64,
            Self::Isize => Type::Isize,
            Self::U8 => Type::U8,
            Self::U16 => Type::U16,
            Self::U32 => Type::U32,
            Self::U64 => Type::U64,
            Self::Usize => Type::Usize,
            Self::F32 => Type::F32,
            Self::F64 => Type::F64,
        }
    }
}
impl Intrinsic {
    pub(super) fn lookup(name: &str) -> Option<Self> {
        Some(match name {
            "i8" => Self::NumericConvert(Number::I8),
            "i16" => Self::NumericConvert(Number::I16),
            "i32" => Self::NumericConvert(Number::I32),
            "i64" => Self::NumericConvert(Number::I64),
            "isize" => Self::NumericConvert(Number::Isize),
            "u8" => Self::NumericConvert(Number::U8),
            "u16" => Self::NumericConvert(Number::U16),
            "u32" => Self::NumericConvert(Number::U32),
            "u64" => Self::NumericConvert(Number::U64),
            "usize" => Self::NumericConvert(Number::Usize),
            "owned_dyn" => Self::OwnedDyn,
            "c_callback" => Self::CCallback,
            "f32" => Self::NumericConvert(Number::F32),
            "f64" => Self::NumericConvert(Number::F64),
            "task_yield" => Self::TaskYield,
            "task_await" => Self::TaskAwait,
            "task_create" => Self::TaskCreate,
            "task_create_owned" => Self::TaskCreateOwned,
            "task_valid" => Self::TaskValid,
            "task_poll" => Self::TaskPoll,
            "task_poll_close" => Self::TaskPollClose,
            "task_request_cancel" => Self::TaskCancel,
            "task_finish_destroy" => Self::TaskFinish,
            "bytes_new" => Self::BytesNew,
            "bytes_reserve" => Self::BytesReserve,
            "bytes_push" => Self::BytesPush,
            "bytes_len" => Self::BytesLen,
            "bytes_capacity" => Self::BytesCapacity,
            "bytes_truncate" => Self::BytesTruncate,
            "bytes_view" => Self::BytesView { mutable: false },
            "bytes_view_mut" => Self::BytesView { mutable: true },
            "move_out" => Self::TakeStored,
            "store_new" => Self::StoreNew,
            "store_reserve" => Self::StoreReserve,
            "store_push" => Self::StorePush,
            "store_pop" => Self::StorePop,
            "store_at" => Self::StoreAt,
            "store_at_mut" => Self::StoreAtMut,
            "store_len" => Self::StoreLen,
            "store_capacity" => Self::StoreCapacity,
            "mutex_create" => Self::MutexCreate,
            "mutex_lock" => Self::MutexLock,
            "mutex_value" => Self::MutexValue,
            "mutex_scope" => Self::SyncCellScope(SyncCellKind::MutexI32),
            "atomic_scope" => Self::SyncCellScope(SyncCellKind::AtomicI32),
            "mutex_load" => Self::SyncCellOp(SyncCellKind::MutexI32, SyncI32Op::Load),
            "mutex_store" => Self::SyncCellOp(SyncCellKind::MutexI32, SyncI32Op::Store),
            "mutex_fetch_add" => Self::SyncCellOp(SyncCellKind::MutexI32, SyncI32Op::FetchAdd),
            "atomic_load" => Self::SyncCellOp(SyncCellKind::AtomicI32, SyncI32Op::Load),
            "atomic_store" => Self::SyncCellOp(SyncCellKind::AtomicI32, SyncI32Op::Store),
            "atomic_fetch_add" => Self::SyncCellOp(SyncCellKind::AtomicI32, SyncI32Op::FetchAdd),
            "atomic_compare_exchange" => Self::SyncCellCompareExchange,
            "thread_scope" => Self::ThreadScope,
            "c_register" => Self::Register,
            "c_register_async" => Self::RegisterAsync,
            "c_with_callback" => Self::ScopedForeignCallback,
            "registration_valid" => Self::RegistrationValid,
            "registration_close" => Self::RegistrationClose,
            "async_registration_valid" => Self::AsyncValid,
            "async_registration_request_close" => Self::AsyncRequest,
            "async_registration_poll_close" => Self::AsyncPoll,
            "async_registration_finish_destroy" => Self::AsyncFinish,
            "async_registration_finish_destroy_checked" => Self::AsyncFinishChecked,
            _ => return None,
        })
    }
    /// Fixed ABI of the operation, conditional on its complete scheme. This
    /// is not a proof that its arguments, modes, cleanup or callback satisfy it.
    pub(super) fn call_kind(self) -> CallKind {
        if matches!(self, Self::TaskYield | Self::TaskAwait) {
            CallKind::Suspend
        } else {
            CallKind::Direct
        }
    }
}

pub(super) enum Resolution {
    Definition(SemanticDefinitionId),
    Intrinsic(Intrinsic),
    Deferred,
}

pub(super) fn resolve(
    inventory: &Inventory<'_>,
    owner: SemanticDefinitionId,
    path: &[&str],
    locals: &BTreeSet<String>,
    b: &mut Budget,
) -> CompileResult<Resolution> {
    if path.is_empty() {
        return Ok(Resolution::Deferred);
    }
    b.lookup(locals.len(), WORK)?;
    if locals.contains(path[0]) {
        return Ok(Resolution::Deferred);
    }
    let index = inventory.original_index(owner, b)?;
    if let Some(bound) = index.name(SemanticOriginalNamespace::Value, path, b)? {
        return match bound {
            SemanticOriginalNameValue::Definition(id) => {
                inventory.definition(*id, b)?;
                Ok(Resolution::Definition(*id))
            }
            SemanticOriginalNameValue::Recipe(_) => Ok(Resolution::Deferred),
        };
    }
    if path.len() != 1 {
        return Ok(Resolution::Deferred);
    }
    let Some(intrinsic) = Intrinsic::lookup(path[0]) else {
        return Ok(Resolution::Deferred);
    };
    if matches!(intrinsic, Intrinsic::NumericConvert(_))
        && index.name(SemanticOriginalNamespace::Type, path, b)?.is_some()
    {
        return Ok(Resolution::Deferred);
    }
    // A same-module declaration cannot disappear from Names to uncover a core
    // operation. Imports must likewise have their real explicit name binding;
    // there is no scan for a convenient same-shaped function or first caller.
    if inventory.hidden_top_level(owner.module, path[0], matches!(intrinsic, Intrinsic::NumericConvert(_)), b)? {
        return Err(fail("core operation name hides an unbound real source declaration"));
    }
    Ok(Resolution::Intrinsic(intrinsic))
}

pub(super) fn path<'a>(
    value: &'a SemanticOriginalExpr,
    b: &mut Budget,
    depth: usize,
) -> CompileResult<Option<Vec<&'a str>>> {
    b.spend(1, WORK)?;
    if depth > b.partial_depth {
        return Err(resource("original callee path exceeds its finite depth"));
    }
    match &value.kind {
        CoreExprKind::Name(name) => Ok(Some(vec![name.as_str()])),
        CoreExprKind::Member {
            object,
            member,
            owner: None,
        } => {
            let Some(mut path) = path(object, b, depth + 1)? else {
                return Ok(None);
            };
            b.spend(path.len().saturating_add(member.len() + 1), WORK)?;
            path.try_reserve(1)
                .map_err(|_| resource("original callee path allocation failed"))?;
            path.push(member);
            Ok(Some(path))
        }
        CoreExprKind::Group(inner)
        | CoreExprKind::TypeApply { callee: inner, .. }
        | CoreExprKind::GenericApply { callee: inner, .. } => path(inner, b, depth + 1),
        _ => Ok(None),
    }
}
