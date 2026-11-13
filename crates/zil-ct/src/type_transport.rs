//! Complete borrowed Runtime Type data traversal, including inactive arms,
//! retained/phantom rows and full nominal identities. No interpretation or
//! class/source/capability proof is supplied by this transport helper.
use zil_base::{ModuleId, NodeId};
use zil_ir::*;
enum Work<'a> {
    W0(&'a Type),
    W1(&'a DynInterfaceId),
    W2(&'a StorageLifeShape),
    W3(&'a SyncCellKind),
    W4(&'a RetainedLifeShape),
    W5(&'a Vec<Type>),
    W6(&'a u32),
    W7(&'a String),
    W8(&'a Vec<(String, Type)>),
    W9(&'a (String, Type)),
    W10(&'a Vec<(String, Vec<Type>)>),
    W11(&'a (String, Vec<Type>)),
    W12(&'a bool),
    W13(&'a StorageLifeContract),
    W14(&'a ClosureId),
    W15(&'a NodeId),
    W16(&'a Vec<StorageLifeBinding>),
    W17(&'a StorageLifeBinding),
    W18(&'a Vec<StoragePosition>),
    W19(&'a StoragePosition),
    W20(&'a Vec<SignatureLifeClassBinding>),
    W21(&'a SignatureLifeClassBinding),
    W22(&'a SignatureLifeClass),
    W23(&'a StorageBinder),
    W24(&'a StorageBase),
    W25(&'a Vec<LifeTypeProjection>),
    W26(&'a LifeTypeProjection),
    W27(&'a PlaceProjection),
    W28(&'a VariantProjection),
    W29(&'a Vec<ContractProjection>),
    W30(&'a ContractProjection),
    W31(&'a Vec<RetainedBorrowPort>),
    W32(&'a RetainedBorrowPort),
    W33(&'a Vec<StorageRetainedBinding>),
    W34(&'a StorageRetainedBinding),
    W35(&'a Vec<NominalLifeArgument>),
    W36(&'a NominalLifeArgument),
    W37(&'a LifeDeclarationId),
    W38(&'a ModuleId),
    W39(&'a StoredBorrowMode),
    W40(&'a Vec<StorageLifeSlot>),
    W41(&'a StorageLifeSlot),
    W42(&'a Vec<PlaceProjection>),
}
pub fn visit_runtime_type<V: DynRecipeVisitor>(
    value: &Type,
    visitor: &mut V,
    depth: usize,
) -> Result<(), V::Error> {
    visitor.rows(1, depth)?;
    let mut pending = vec![(Work::W0(value), depth)];
    while let Some((next, depth)) = pending.pop() {
        match next {
            Work::W0(value) => {
                visitor.node(depth)?;
                match value {
                    Type::Dyn { interface } => {
                        pending.push((Work::W1(interface), depth + 1));
                    }
                    Type::OwnedDyn { interface } => {
                        pending.push((Work::W1(interface), depth + 1));
                    }
                    Type::BorrowedStorage { storage, life } => {
                        pending.push((Work::W2(life), depth + 1));
                        pending.push((Work::W0(storage), depth + 1));
                    }
                    Type::Unit => {}
                    Type::Bool => {}
                    Type::I32 => {}
                    Type::I64 => {}
                    Type::U64 => {}
                    Type::Usize => {}
                    Type::F32 => {}
                    Type::F64 => {}
                    Type::U8 => {}
                    Type::U16 => {}
                    Type::U32 => {}
                    Type::I8 => {}
                    Type::I16 => {}
                    Type::Isize => {}
                    Type::ByteBuffer => {}
                    Type::OwnedStore { element } => {
                        pending.push((Work::W0(element), depth + 1));
                    }
                    Type::ForeignHandle => {}
                    Type::SyncCell(value_0) => {
                        pending.push((Work::W3(value_0), depth + 1));
                    }
                    Type::Mutex { payload } => {
                        pending.push((Work::W0(payload), depth + 1));
                    }
                    Type::MutexGuard {
                        payload,
                        retained_life,
                    } => {
                        pending.push((Work::W4(retained_life), depth + 1));
                        pending.push((Work::W0(payload), depth + 1));
                    }
                    Type::Registration { environment } => {
                        pending.push((Work::W0(environment), depth + 1));
                    }
                    Type::AsyncRegistration { environment } => {
                        pending.push((Work::W0(environment), depth + 1));
                    }
                    Type::TaskI32 => {}
                    Type::Task {
                        result,
                        retained_life,
                    } => {
                        pending.push((Work::W4(retained_life), depth + 1));
                        pending.push((Work::W0(result), depth + 1));
                    }
                    Type::CWideString => {}
                    Type::ForeignCallback { parameters, result } => {
                        pending.push((Work::W0(result), depth + 1));
                        pending.push((Work::W5(parameters), depth + 1));
                    }
                    Type::Tuple(value_0) => {
                        pending.push((Work::W5(value_0), depth + 1));
                    }
                    Type::Array { element, length } => {
                        pending.push((Work::W6(length), depth + 1));
                        pending.push((Work::W0(element), depth + 1));
                    }
                    Type::Slice { element } => {
                        pending.push((Work::W0(element), depth + 1));
                    }
                    Type::Named(value_0) => {
                        pending.push((Work::W7(value_0), depth + 1));
                    }
                    Type::Record { name, fields } => {
                        pending.push((Work::W8(fields), depth + 1));
                        pending.push((Work::W7(name), depth + 1));
                    }
                    Type::Package { name, fields } => {
                        pending.push((Work::W8(fields), depth + 1));
                        pending.push((Work::W7(name), depth + 1));
                    }
                    Type::Enum { name, variants } => {
                        pending.push((Work::W10(variants), depth + 1));
                        pending.push((Work::W7(name), depth + 1));
                    }
                    Type::ErrorUnion { error, result } => {
                        pending.push((Work::W0(result), depth + 1));
                        pending.push((Work::W0(error), depth + 1));
                    }
                    Type::Address { mutable, inner } => {
                        pending.push((Work::W0(inner), depth + 1));
                        pending.push((Work::W12(mutable), depth + 1));
                    }
                    Type::Managed(value_0) => {
                        pending.push((Work::W0(value_0), depth + 1));
                    }
                    Type::Desc(value_0) => {
                        pending.push((Work::W0(value_0), depth + 1));
                    }
                    Type::FunctionPointer { parameters, result } => {
                        pending.push((Work::W0(result), depth + 1));
                        pending.push((Work::W5(parameters), depth + 1));
                    }
                    Type::BorrowedFunctionPointer {
                        parameters,
                        result,
                        life,
                    } => {
                        pending.push((Work::W13(life), depth + 1));
                        pending.push((Work::W0(result), depth + 1));
                        pending.push((Work::W5(parameters), depth + 1));
                    }
                    Type::Closure(value_0) => {
                        pending.push((Work::W14(value_0), depth + 1));
                    }
                    Type::Opaque { identity, hidden } => {
                        pending.push((Work::W0(hidden), depth + 1));
                        pending.push((Work::W15(identity), depth + 1));
                    }
                    Type::Unsupported(value_0) => {
                        pending.push((Work::W7(value_0), depth + 1));
                    }
                }
            }
            Work::W1(value) => {
                visitor.node(depth)?;
                pending.push((Work::W6(&value.index), depth + 1));
                pending.push((Work::W38(&value.module), depth + 1));
            }
            Work::W2(value) => {
                visitor.node(depth)?;
                pending.push((Work::W35(&value.nominal_arguments), depth + 1));
                pending.push((Work::W33(&value.retained), depth + 1));
                pending.push((Work::W40(&value.slots), depth + 1));
                pending.push((Work::W6(&value.binder_count), depth + 1));
            }
            Work::W3(value) => {
                visitor.node(depth)?;
                match value {
                    SyncCellKind::MutexI32 => {}
                    SyncCellKind::AtomicI32 => {}
                    SyncCellKind::AtomicI64 => {}
                    SyncCellKind::AtomicU64 => {}
                    SyncCellKind::AtomicUsize => {}
                }
            }
            Work::W4(value) => {
                visitor.node(depth)?;
                pending.push((Work::W35(&value.nominal_arguments), depth + 1));
                pending.push((Work::W33(&value.retained), depth + 1));
                pending.push((Work::W31(&value.ports), depth + 1));
                pending.push((Work::W6(&value.binder_count), depth + 1));
            }
            Work::W5(value) => {
                visitor.rows(value.len(), depth)?;
                for child in value.iter().rev() {
                    pending.push((Work::W0(child), depth));
                }
            }
            Work::W6(value) => {
                let _ = value;
                visitor.node(depth)?;
            }
            Work::W7(value) => {
                visitor.text(value, depth)?;
            }
            Work::W8(value) => {
                visitor.rows(value.len(), depth)?;
                for child in value.iter().rev() {
                    pending.push((Work::W9(child), depth));
                }
            }
            Work::W9(value) => {
                visitor.node(depth)?;
                pending.push((Work::W0(&value.1), depth + 1));
                pending.push((Work::W7(&value.0), depth + 1));
            }
            Work::W10(value) => {
                visitor.rows(value.len(), depth)?;
                for child in value.iter().rev() {
                    pending.push((Work::W11(child), depth));
                }
            }
            Work::W11(value) => {
                visitor.node(depth)?;
                pending.push((Work::W5(&value.1), depth + 1));
                pending.push((Work::W7(&value.0), depth + 1));
            }
            Work::W12(value) => {
                let _ = value;
                visitor.node(depth)?;
            }
            Work::W13(value) => {
                visitor.node(depth)?;
                pending.push((Work::W20(&value.classes), depth + 1));
                pending.push((Work::W18(&value.static_bindings), depth + 1));
                pending.push((Work::W16(&value.bindings), depth + 1));
                pending.push((Work::W6(&value.invocation_binder_count), depth + 1));
                pending.push((Work::W6(&value.fixed_binder_count), depth + 1));
            }
            Work::W14(value) => {
                let _ = value;
                visitor.node(depth)?;
            }
            Work::W15(value) => {
                let _ = value;
                visitor.node(depth)?;
            }
            Work::W16(value) => {
                visitor.rows(value.len(), depth)?;
                for child in value.iter().rev() {
                    pending.push((Work::W17(child), depth));
                }
            }
            Work::W17(value) => {
                visitor.node(depth)?;
                pending.push((Work::W23(&value.binder), depth + 1));
                pending.push((Work::W19(&value.position), depth + 1));
            }
            Work::W18(value) => {
                visitor.rows(value.len(), depth)?;
                for child in value.iter().rev() {
                    pending.push((Work::W19(child), depth));
                }
            }
            Work::W19(value) => {
                visitor.node(depth)?;
                pending.push((Work::W29(&value.path), depth + 1));
                pending.push((Work::W24(&value.base), depth + 1));
            }
            Work::W20(value) => {
                visitor.rows(value.len(), depth)?;
                for child in value.iter().rev() {
                    pending.push((Work::W21(child), depth));
                }
            }
            Work::W21(value) => {
                visitor.node(depth)?;
                pending.push((Work::W23(&value.binder), depth + 1));
                pending.push((Work::W22(&value.position), depth + 1));
            }
            Work::W22(value) => {
                visitor.node(depth)?;
                pending.push((Work::W6(&value.class), depth + 1));
                pending.push((Work::W25(&value.path), depth + 1));
                pending.push((Work::W24(&value.base), depth + 1));
            }
            Work::W23(value) => {
                visitor.node(depth)?;
                match value {
                    StorageBinder::Fixed(value_0) => {
                        pending.push((Work::W6(value_0), depth + 1));
                    }
                    StorageBinder::Invocation(value_0) => {
                        pending.push((Work::W6(value_0), depth + 1));
                    }
                }
            }
            Work::W24(value) => {
                visitor.node(depth)?;
                match value {
                    StorageBase::Parameter(value_0) => {
                        pending.push((Work::W6(value_0), depth + 1));
                    }
                    StorageBase::Capture(value_0) => {
                        pending.push((Work::W6(value_0), depth + 1));
                    }
                    StorageBase::Receiver => {}
                    StorageBase::Result => {}
                }
            }
            Work::W25(value) => {
                visitor.rows(value.len(), depth)?;
                for child in value.iter().rev() {
                    pending.push((Work::W26(child), depth));
                }
            }
            Work::W26(value) => {
                visitor.node(depth)?;
                match value {
                    LifeTypeProjection::Place(value_0) => {
                        pending.push((Work::W27(value_0), depth + 1));
                    }
                    LifeTypeProjection::TypeArgument(value_0) => {
                        pending.push((Work::W6(value_0), depth + 1));
                    }
                    LifeTypeProjection::Port(value_0) => {
                        pending.push((Work::W6(value_0), depth + 1));
                    }
                    LifeTypeProjection::AssociatedType(value_0) => {
                        pending.push((Work::W6(value_0), depth + 1));
                    }
                    LifeTypeProjection::Pointee => {}
                    LifeTypeProjection::Element => {}
                    LifeTypeProjection::Parameter(value_0) => {
                        pending.push((Work::W6(value_0), depth + 1));
                    }
                    LifeTypeProjection::Result => {}
                }
            }
            Work::W27(value) => {
                visitor.node(depth)?;
                match value {
                    PlaceProjection::Field(value_0) => {
                        pending.push((Work::W6(value_0), depth + 1));
                    }
                    PlaceProjection::Variant(value_0) => {
                        pending.push((Work::W28(value_0), depth + 1));
                    }
                    PlaceProjection::ErrorArm(value_0) => {
                        pending.push((Work::W12(value_0), depth + 1));
                    }
                }
            }
            Work::W28(value) => {
                visitor.node(depth)?;
                pending.push((Work::W6(&value.index), depth + 1));
                pending.push((Work::W6(&value.variant), depth + 1));
            }
            Work::W29(value) => {
                visitor.rows(value.len(), depth)?;
                for child in value.iter().rev() {
                    pending.push((Work::W30(child), depth));
                }
            }
            Work::W30(value) => {
                visitor.node(depth)?;
                match value {
                    ContractProjection::Place(value_0) => {
                        pending.push((Work::W27(value_0), depth + 1));
                    }
                    ContractProjection::RetainedPort(value_0) => {
                        pending.push((Work::W6(value_0), depth + 1));
                    }
                    ContractProjection::RetainedClass(value_0) => {
                        pending.push((Work::W6(value_0), depth + 1));
                    }
                    ContractProjection::Capture(value_0) => {
                        pending.push((Work::W6(value_0), depth + 1));
                    }
                }
            }
            Work::W31(value) => {
                visitor.rows(value.len(), depth)?;
                for child in value.iter().rev() {
                    pending.push((Work::W32(child), depth));
                }
            }
            Work::W32(value) => {
                visitor.node(depth)?;
                pending.push((Work::W6(&value.binder), depth + 1));
                pending.push((Work::W39(&value.mode), depth + 1));
                pending.push((Work::W0(&value.leaf), depth + 1));
            }
            Work::W33(value) => {
                visitor.rows(value.len(), depth)?;
                for child in value.iter().rev() {
                    pending.push((Work::W34(child), depth));
                }
            }
            Work::W34(value) => {
                visitor.node(depth)?;
                pending.push((Work::W6(&value.binder), depth + 1));
                pending.push((Work::W6(&value.class), depth + 1));
                pending.push((Work::W25(&value.path), depth + 1));
            }
            Work::W35(value) => {
                visitor.rows(value.len(), depth)?;
                for child in value.iter().rev() {
                    pending.push((Work::W36(child), depth));
                }
            }
            Work::W36(value) => {
                visitor.node(depth)?;
                pending.push((Work::W6(&value.binder), depth + 1));
                pending.push((Work::W6(&value.argument), depth + 1));
                pending.push((Work::W37(&value.declaration), depth + 1));
                pending.push((Work::W25(&value.path), depth + 1));
            }
            Work::W37(value) => {
                visitor.node(depth)?;
                pending.push((Work::W12(&value.compiler), depth + 1));
                pending.push((Work::W15(&value.declaration), depth + 1));
                pending.push((Work::W38(&value.module), depth + 1));
            }
            Work::W38(value) => {
                let _ = value;
                visitor.node(depth)?;
            }
            Work::W39(value) => {
                visitor.node(depth)?;
                match value {
                    StoredBorrowMode::SharedAddress => {}
                    StoredBorrowMode::ExclusiveAddress => {}
                    StoredBorrowMode::ReadOnlyDescription => {}
                    StoredBorrowMode::WritableDescription => {}
                }
            }
            Work::W40(value) => {
                visitor.rows(value.len(), depth)?;
                for child in value.iter().rev() {
                    pending.push((Work::W41(child), depth));
                }
            }
            Work::W41(value) => {
                visitor.node(depth)?;
                pending.push((Work::W39(&value.mode), depth + 1));
                pending.push((Work::W6(&value.binder), depth + 1));
                pending.push((Work::W42(&value.path), depth + 1));
            }
            Work::W42(value) => {
                visitor.rows(value.len(), depth)?;
                for child in value.iter().rev() {
                    pending.push((Work::W27(child), depth));
                }
            }
        }
    }
    Ok(())
}
