//! Concrete failure carrier production. This metadata describes physical input
//! slots; only the actual constructor/projection consumers may bind their roots.
use super::*;
use zil_ir::{LifeTypeProjection as L, PlaceProjection as P};
fn limit(message: &str, span: Span) -> Diagnostic {
    Diagnostic::limit(Phase::Types, message, Some(span))
}

fn owns(t: &Type) -> bool {
    match t {
        Type::Address { .. } | Type::Desc(_) => false,
        Type::Managed(_) | Type::Package { .. } | Type::Closure(_) | Type::OwnedDyn { .. } => true,
        Type::Tuple(ts) => ts.iter().any(owns),
        Type::Record { fields, .. } => fields.iter().any(|(_, t)| owns(t)),
        Type::Enum { variants, .. } => variants.iter().any(|(_, ts)| ts.iter().any(owns)),
        Type::ErrorUnion { error, result } => owns(error) || owns(result),
        Type::Array { element, .. } => owns(element),
        Type::BorrowedStorage { storage, .. } => owns(storage),
        _ => false,
    }
}

pub(crate) fn is_carrier(ty: &Type) -> bool {
    matches!(ty, Type::BorrowedStorage { storage, .. } if matches!(storage.as_ref(), Type::Record {name,fields} | Type::Package {name,fields} if name.starts_with("@std.task:v1:TaskCreateFailure:") && matches!(fields.as_slice(), [(reason,_),(inputs,Type::Tuple(_))] if reason=="reason" && inputs=="inputs")))
}

/// A child view has its own dense spelling. The private mapping is rebuilt
/// from the real parent's selected rows; equal numeric class IDs are not used
/// as evidence that the two scopes share a loan or a target.
pub(crate) fn inputs_view(parent: &Type) -> Option<Type> {
    if !is_carrier(parent) {
        return None;
    }
    let Type::BorrowedStorage { storage, life } = parent else {
        return None;
    };
    let (Type::Record { fields, .. } | Type::Package { fields, .. }) = storage.as_ref() else {
        return None;
    };
    let mut map = BTreeMap::new();
    let mut slots = Vec::new();
    for slot in &life.slots {
        let tail = slot.path.strip_prefix(&[P::Field(1)])?;
        let next = map.len() as u32;
        let binder = *map.entry(slot.binder).or_insert(next);
        slots.push(zil_ir::StorageLifeSlot {
            path: tail.to_vec(),
            mode: slot.mode,
            binder,
        });
    }
    Some(Type::BorrowedStorage {
        storage: Box::new(fields[1].1.clone()),
        life: zil_ir::StorageLifeShape {
            binder_count: map.len() as u32,
            slots,
            retained: vec![],
            nominal_arguments: vec![],
        },
    })
}

/// Inputs are in the actual worker's complete parameter order. Its retained
/// context supplies class equality, never permission or a cleanup capability.
pub(super) fn carrier(inputs: &[Type], context: &zil_ir::RetainedLifeShape) -> Type {
    let input_tuple = Type::Tuple(inputs.to_vec());
    let physical = super::create_failure(input_tuple);
    if context.ports.is_empty() || inputs.iter().any(owns) {
        // Mixed owning + borrowing input views need the same exact per-leaf
        // responsibility consumer before this qualified representation opens.
        return physical;
    }
    let physical = match physical {
        Type::Package { name, fields } if !inputs.iter().any(owns) => Type::Record { name, fields },
        other => other,
    };
    let mut slots = Vec::new();
    let mut retained = Vec::new();
    let mut port = 0;
    for (index, input) in inputs.iter().enumerate() {
        if let Type::Address { mutable, .. } = input {
            // Ordinary Task parameter checking independently authenticates the
            // exact count and leaf of every port before this type is published.
            let Some(actual) = context.ports.get(port).filter(|p| p.leaf == *input) else {
                return physical;
            };
            slots.push(zil_ir::StorageLifeSlot {
                path: vec![P::Field(1), P::Field(index as u32)],
                binder: actual.binder,
                mode: if *mutable {
                    zil_ir::StoredBorrowMode::ExclusiveAddress
                } else {
                    zil_ir::StoredBorrowMode::SharedAddress
                },
            });
            retained.push(zil_ir::StorageRetainedBinding {
                path: vec![L::TypeArgument(0), L::Place(P::Field(index as u32))],
                class: 0,
                binder: actual.binder,
            });
            port += 1;
        }
    }
    if port != context.ports.len() {
        return physical;
    }
    for binder in 0..context.binder_count {
        retained.push(zil_ir::StorageRetainedBinding {
            path: vec![L::TypeArgument(1)],
            class: binder,
            binder,
        });
    }
    Type::BorrowedStorage {
        storage: Box::new(physical),
        life: zil_ir::StorageLifeShape {
            binder_count: context.binder_count,
            slots,
            retained,
            nominal_arguments: vec![],
        },
    }
}

/// Keep the implicit actual context in the nominal key. Two equal physical
/// input tuples with different fixed class maps are different full instances.
pub(super) fn context_actual(context: &zil_ir::RetainedLifeShape) -> Type {
    Type::Task {
        result: Box::new(Type::Unit),
        retained_life: zil_ir::RetainedLifeShape {
            binder_count: context.binder_count,
            ports: context.ports.clone(),
            retained: vec![],
            nominal_arguments: vec![],
        },
    }
}

pub(super) fn declaration(package: bool) -> zil_ir::LifeNominalDeclaration {
    zil_ir::LifeNominalDeclaration {
        id: if package {
            zil_ir::builtin_life::TASK_CREATE_FAILURE_PACKAGE
        } else {
            zil_ir::builtin_life::TASK_CREATE_FAILURE_RECORD
        },
        identity: zil_ir::LifeDeclarationIdentity {
            package: "@zil.builtin".into(),
            module_path: vec!["task".into()],
            declaration_path: vec![
                if package {
                    "package".into()
                } else {
                    "record".into()
                },
                "TaskCreateFailure".into(),
            ],
            generation_path: vec![],
        },
        kind: if package {
            zil_ir::LifeNominalKind::Package
        } else {
            zil_ir::LifeNominalKind::Record
        },
        parameters: vec![
            zil_ir::LifeParameterKind::Type,
            zil_ir::LifeParameterKind::Type,
        ],
    }
}

/// Publish nominal instances only for actual owned constructors. A type name
/// or a requested annotation never creates a declaration instance or a root.
pub(crate) fn append_inventory(
    module: &mut TypedModule,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    if !module.functions.iter().any(|f| {
        f.blocks.iter().any(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, InstructionKind::TaskCreateOwned { .. }))
        })
    }) {
        return Ok(());
    }
    let mut actuals = Vec::new();
    for f in &module.functions {
        for block in &f.blocks {
            for i in &block.instructions {
                *left = left
                    .checked_sub(1)
                    .ok_or_else(|| limit("Task failure inventory work exceeded", i.span))?;
                let InstructionKind::TaskCreateOwned { worker, .. } = i.kind else {
                    continue;
                };
                let Some(Type::ErrorUnion {
                    error: failure,
                    result,
                }) = i.result.and_then(|v| f.values.get(v.0 as usize))
                else {
                    continue;
                };
                let Type::BorrowedStorage { storage, .. } = failure.as_ref() else {
                    continue;
                };
                let Type::Managed(task) = result.as_ref() else {
                    return Err(unsupported(
                        i.span,
                        "owned constructor result has no actual Task",
                    ));
                };
                let Type::Task { retained_life, .. } = task.as_ref() else {
                    return Err(unsupported(
                        i.span,
                        "owned constructor result has no actual retained context",
                    ));
                };
                let target = module
                    .functions
                    .get(worker.0 as usize)
                    .ok_or_else(|| unsupported(i.span, "owned constructor worker is absent"))?;
                // All source trees are charged before their snapshots or the
                // canonical fingerprint calculation inside carrier().
                for local in &target.parameters {
                    inference::charge_type_snapshot(
                        &target.locals[local.0 as usize].ty,
                        i.span,
                        limits,
                        left,
                        4,
                    )?;
                }
                inference::charge_type_snapshot(failure, i.span, limits, left, 2)?;
                inference::charge_type_snapshot(task, i.span, limits, left, 2)?;
                let inputs: Vec<Type> = target
                    .parameters
                    .iter()
                    .map(|local| target.locals[local.0 as usize].ty.clone())
                    .collect();
                if carrier(&inputs, retained_life) != **failure {
                    return Err(error(
                        "E-TASK-CREATE-FAILURE",
                        i.span,
                        "failure carrier differs from its actual worker input/context",
                    ));
                }
                actuals.push((
                    matches!(storage.as_ref(), Type::Package { .. }),
                    zil_ir::LifeNominalInstance {
                        declaration: declaration(matches!(storage.as_ref(), Type::Package { .. }))
                            .id,
                        ty: (**failure).clone(),
                        arguments: vec![
                            zil_ir::LifeNominalActual::Type(Type::Tuple(inputs)),
                            zil_ir::LifeNominalActual::Type(context_actual(retained_life)),
                        ],
                    },
                ));
            }
        }
    }
    for (package, instance) in actuals {
        let declared = declaration(package);
        for earlier in &module.life_declarations {
            *left = left.checked_sub(128).ok_or_else(|| {
                limit(
                    "Task failure declaration comparison work exceeded",
                    Span::default(),
                )
            })?;
            if earlier.id == declared.id && *earlier != declared {
                return Err(error(
                    "E-TASK-CREATE-FAILURE",
                    Span::default(),
                    "reserved Task failure declaration differs",
                ));
            }
        }
        if !module.life_declarations.iter().any(|d| d.id == declared.id) {
            module.life_declarations.push(declared);
        }
        let mut exists = false;
        for earlier in &module.life_instances {
            inference::charge_type_snapshot(&earlier.ty, Span::default(), limits, left, 1)?;
            inference::charge_type_snapshot(&instance.ty, Span::default(), limits, left, 1)?;
            if earlier.ty == instance.ty {
                if *earlier != instance {
                    return Err(error(
                        "E-TASK-CREATE-FAILURE",
                        Span::default(),
                        "failure type has conflicting actual retained context",
                    ));
                }
                exists = true;
            }
        }
        if !exists {
            module.life_instances.push(instance);
        }
    }
    Ok(())
}
