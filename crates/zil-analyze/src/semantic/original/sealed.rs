//! The compiler's closed core-operation namespace. A spelling alone never
//! resolves an operation: the complete original lexical namespace is checked
//! first, and every selected operation is rechecked against its actual opcode.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::semantic) enum Intrinsic {
    NumericConvert(Number),
    SliceLength,
    OwnedDyn,
    TaskYield,
    TaskCreate,
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
    BytesView,
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
    ThreadScope,
    Register,
    RegisterAsync,
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
            "f32" => Self::NumericConvert(Number::F32),
            "f64" => Self::NumericConvert(Number::F64),
            "task_yield" => Self::TaskYield,
            "task_create" => Self::TaskCreate,
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
            "bytes_view" => Self::BytesView,
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
            "thread_scope" => Self::ThreadScope,
            "c_register" => Self::Register,
            "c_register_async" => Self::RegisterAsync,
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
        if self == Self::TaskYield {
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
    let definition = inventory.definition(owner, b)?;
    let SemanticDefinitionBody::FunctionWithBody { names, .. } = &definition.body else {
        return Err(fail(
            "core operation resolution has no complete original name table",
        ));
    };
    let mut bound = None;
    for binding in names {
        b.spend(
            binding
                .path
                .iter()
                .map(String::len)
                .sum::<usize>()
                .saturating_add(path.iter().map(|part| part.len()).sum::<usize>() + 1),
            WORK,
        )?;
        if binding.namespace != SemanticOriginalNamespace::Value
            || binding.path.len() != path.len()
            || !binding.path.iter().zip(path).all(|(a, b)| a == b)
        {
            continue;
        }
        if bound.replace(&binding.value).is_some() {
            return Err(fail(
                "core operation has ambiguous original lexical bindings",
            ));
        }
    }
    if let Some(bound) = bound {
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
    if matches!(intrinsic, Intrinsic::NumericConvert(_)) {
        for binding in names {
            b.spend(
                binding
                    .path
                    .iter()
                    .map(String::len)
                    .sum::<usize>()
                    .saturating_add(path[0].len() + 1),
                WORK,
            )?;
            if binding.namespace == SemanticOriginalNamespace::Type
                && binding.path.as_slice() == [path[0]]
            {
                return Ok(Resolution::Deferred);
            }
        }
    }
    // A same-module declaration cannot disappear from Names to uncover a core
    // operation. Imports must likewise have their real explicit name binding;
    // there is no scan for a convenient same-shaped function or first caller.
    for candidate in inventory.definitions.values() {
        b.spend(
            candidate
                .identity
                .declaration_path
                .iter()
                .map(String::len)
                .sum::<usize>()
                .saturating_add(1),
            WORK,
        )?;
        if candidate.id.module == owner.module
            && candidate.parent.is_none()
            && (candidate.kind == SemanticDefinitionKind::Function
                || matches!(intrinsic, Intrinsic::NumericConvert(_)))
            && candidate
                .identity
                .declaration_path
                .last()
                .is_some_and(|name| name == path[0])
        {
            return Err(fail(
                "core operation name hides an unbound real source declaration",
            ));
        }
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
