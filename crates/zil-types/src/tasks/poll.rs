//! Publish complete nominal rows only for actual TaskPoll emitter receipts.
//! The original payload was selected at the exact call, never by a type search.
use super::*;
fn fee(left: &mut usize, n: usize) -> R<()> {
    *left = left.checked_sub(n).ok_or_else(|| {
        Diagnostic::limit(Phase::Types, "actual Poll inventory work exceeded", None)
    })?;
    Ok(())
}
fn declaration() -> LifeNominalDeclaration {
    LifeNominalDeclaration {
        id: zil_ir::builtin_life::POLL,
        identity: LifeDeclarationIdentity {
            package: "@zil.builtin".into(),
            module_path: vec!["task".into()],
            declaration_path: vec!["enum".into(), "Poll".into()],
            generation_path: Vec::new(),
        },
        kind: LifeNominalKind::Enum,
        parameters: vec![LifeParameterKind::Type],
    }
}
pub(crate) fn append_inventory(
    module: &mut TypedModule,
    ledger: &semantics::emission::Ledger,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    if ledger.formed_polls.is_empty() {
        return Ok(());
    }
    let declared = declaration();
    fee(
        left,
        128usize.saturating_mul(module.life_declarations.len().saturating_add(1)),
    )?;
    if let Some(earlier) = module
        .life_declarations
        .iter()
        .find(|row| row.id == declared.id)
    {
        if earlier != &declared {
            return Err(error(
                "E-TASK-POLL",
                Span::default(),
                "Poll has a different reserved declaration",
            ));
        }
    } else {
        module.life_declarations.push(declared);
    }
    for ((function, value), instance) in &ledger.formed_polls {
        fee(left, module.functions.len())?;
        let actual = module
            .functions
            .iter()
            .find(|row| row.id == *function)
            .ok_or_else(|| unsupported(Span::default(), "Poll formation lost its real function"))?;
        let ty = actual
            .values
            .get(value.0 as usize)
            .ok_or_else(|| unsupported(actual.span, "Poll formation lost its actual SSA result"))?;
        let mut producer = None;
        for block in &actual.blocks {
            fee(left, block.instructions.len())?;
            for operation in &block.instructions {
                if operation.result == Some(*value) {
                    if producer.replace(operation).is_some() {
                        return Err(error(
                            "E-TASK-POLL",
                            operation.span,
                            "Poll result has multiple actual producers",
                        ));
                    }
                }
            }
        }
        let producer = producer
            .ok_or_else(|| unsupported(actual.span, "Poll result has no actual producer"))?;
        let (task, close) = match producer.kind {
            InstructionKind::TaskPoll { task } => (task, false),
            InstructionKind::TaskPollClose { task } => (task, true),
            _ => {
                return Err(error(
                    "E-TASK-POLL",
                    producer.span,
                    "Poll receipt points to a different operation",
                ));
            }
        };
        let task = actual
            .values
            .get(task.0 as usize)
            .and_then(payload)
            .ok_or_else(|| {
                unsupported(producer.span, "Poll argument lost its actual Task carrier")
            })?;
        let Type::Task { result, .. } = task else {
            return Err(unsupported(
                producer.span,
                "full Poll has no complete Task input",
            ));
        };
        let [
            LifeNominalActual::QualifiedType {
                ty: payload_ty,
                life,
            },
        ] = instance.arguments.as_slice()
        else {
            return Err(error(
                "E-TASK-POLL",
                producer.span,
                "Poll omits its full qualified payload argument",
            ));
        };
        let work =
            inference::life_table_snapshot_work(&[], std::slice::from_ref(instance), limits)?;
        fee(left, work.saturating_mul(2))?;
        if instance.declaration != zil_ir::builtin_life::POLL
            || (!close && **result != *payload_ty)
            || (close && (*payload_ty != Type::Unit || life.binder_count != 0))
        {
            return Err(error(
                "E-TASK-POLL",
                producer.span,
                "Poll changes the actual operation payload",
            ));
        }
        let output = match (close, ty) {
            (true, ty) => ty,
            (
                false,
                Type::ErrorUnion {
                    error: failure,
                    result,
                },
            ) if **failure == poll_state() => result,
            _ => {
                return Err(error(
                    "E-TASK-POLL",
                    producer.span,
                    "Poll changes its real output constructor",
                ));
            }
        };
        if output != &instance.ty {
            return Err(error(
                "E-TASK-POLL",
                producer.span,
                "Poll nominal row differs from actual emitted output",
            ));
        }
        let mut exists = false;
        for earlier in &module.life_instances {
            let work =
                inference::life_table_snapshot_work(&[], std::slice::from_ref(earlier), limits)?;
            fee(
                left,
                work.saturating_add(inference::life_table_snapshot_work(
                    &[],
                    std::slice::from_ref(instance),
                    limits,
                )?),
            )?;
            if earlier.ty == instance.ty {
                if earlier.declaration != instance.declaration {
                    return Err(error(
                        "E-TASK-POLL",
                        producer.span,
                        "Poll output has a different nominal declaration",
                    ));
                }
                let same = earlier.arguments == instance.arguments
                    || matches!(earlier.arguments.as_slice(),
                    [LifeNominalActual::Type(ty)] if ty==payload_ty && life.binder_count==0);
                if !same {
                    return Err(error(
                        "E-TASK-POLL",
                        producer.span,
                        "Poll output has conflicting complete payload arguments",
                    ));
                }
                exists = true;
            }
        }
        if !exists {
            module.life_instances.push(instance.clone());
        }
    }
    Ok(())
}
