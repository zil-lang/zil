//! Bounded occurrence encoding of exported types and actual closure layouts.
//! A ClosureId is a relocation reference, never an opaque unit-cost ABI type.
use super::*;

enum Event<'a> {
    Type(&'a Type, usize),
    Text(&'a str),
    Word(usize),
    EndClosure(ClosureId),
}
fn fee(left: &mut usize, count: usize) -> R<()> {
    *left = left.checked_sub(count).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Types,
            "complete interface identity budget exceeded",
            None,
        )
    })?;
    Ok(())
}
fn invalid(message: &str) -> Diagnostic {
    error("E-TYPE-APPLICATION", Span::default(), message)
}

pub(super) fn encode(
    ty: &Type,
    closures: &[ClosureDefinition],
    bytes: &mut Vec<u8>,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    fee(left, 1)?;
    let mut pending = vec![Event::Type(ty, 0)];
    let mut active = Vec::new();
    let mut nodes = limits.max_nodes;
    while let Some(event) = pending.pop() {
        match event {
            Event::Text(text) => {
                fee(left, text.len().saturating_add(8).saturating_mul(2))?;
                bytes.extend_from_slice(&(text.len() as u64).to_le_bytes());
                bytes.extend_from_slice(text.as_bytes());
            }
            Event::Word(word) => {
                fee(left, 16)?;
                bytes.extend_from_slice(&(word as u64).to_le_bytes());
            }
            Event::EndClosure(id) => {
                fee(left, 1)?;
                if active.pop() != Some(id) {
                    return Err(invalid("interface closure traversal is inconsistent"));
                }
            }
            Event::Type(ty, depth) => {
                fee(left, 64)?;
                nodes = nodes.checked_sub(1).ok_or_else(|| {
                    Diagnostic::limit(
                        Phase::Types,
                        "interface identity node budget exceeded",
                        None,
                    )
                })?;
                if depth > limits.max_call_depth {
                    return Err(Diagnostic::limit(
                        Phase::Types,
                        "interface identity depth exceeded",
                        None,
                    ));
                }
                let child_depth = depth.checked_add(1).ok_or_else(|| {
                    Diagnostic::limit(Phase::Types, "interface identity depth overflow", None)
                })?;
                match ty {
                    Type::Unit => bytes.push(0),
                    Type::Bool => bytes.push(1),
                    Type::I32 => bytes.push(2),
                    Type::I64 => bytes.push(3),
                    Type::U8 => bytes.push(15),
                    Type::U16 => bytes.push(16),
                    Type::U64 => bytes.push(23),
                    Type::Usize => bytes.push(24),
                    Type::F32 => bytes.push(28),
                    Type::F64 => bytes.push(29),
                    Type::ByteBuffer => bytes.push(21),
                    Type::TaskI32 => bytes.push(33),
                    Type::ForeignHandle => bytes.push(17),
                    Type::CWideString => bytes.push(18),
                    Type::SyncCell(kind) => bytes.extend_from_slice(&[
                        30,
                        match kind {
                            SyncCellKind::MutexI32 => 0,
                            SyncCellKind::AtomicI32 => 1,
                        },
                    ]),
                    Type::AsyncRegistration { environment: inner } | Type::Registration { environment: inner }
                    | Type::OwnedStore { element: inner }
                    | Type::Slice { element: inner }
                    | Type::Managed(inner)
                    | Type::Desc(inner) => {
                        bytes.push(match ty {
                            Type::Registration { .. } => 32, Type::AsyncRegistration { .. } => 37,
                            Type::OwnedStore { .. } => 22,
                            Type::Slice { .. } => 20,
                            Type::Managed(_) => 10,
                            _ => 11,
                        });
                        pending.push(Event::Type(inner, child_depth));
                    }
                    Type::Address { mutable, inner } => {
                        bytes.extend_from_slice(&[4, u8::from(*mutable)]);
                        pending.push(Event::Type(inner, child_depth));
                    }
                    Type::Array { element, length } => {
                        bytes.push(7);
                        bytes.extend_from_slice(&length.to_le_bytes());
                        pending.push(Event::Type(element, child_depth));
                    }
                    Type::ForeignCallback { parameters, result }
                    | Type::FunctionPointer { parameters, result }
                    | Type::BorrowedFunctionPointer {
                        parameters, result, ..
                    } => {
                        fee(left, parameters.len().saturating_add(1).saturating_mul(8))?;
                        bytes.push(match ty {
                            Type::ForeignCallback { .. } => 19,
                            Type::FunctionPointer { .. } => 8,
                            _ => 31,
                        });
                        if let Type::BorrowedFunctionPointer { life, .. } = ty {
                            contract(life, bytes, limits, left)?;
                        }
                        bytes.extend_from_slice(&(parameters.len() as u64).to_le_bytes());
                        pending.push(Event::Type(result, child_depth));
                        pending.extend(
                            parameters
                                .iter()
                                .rev()
                                .map(|ty| Event::Type(ty, child_depth)),
                        );
                    }
                    Type::Tuple(fields) => {
                        fee(left, fields.len().saturating_mul(8))?;
                        bytes.push(5);
                        bytes.extend_from_slice(&(fields.len() as u64).to_le_bytes());
                        pending.extend(fields.iter().rev().map(|ty| Event::Type(ty, child_depth)));
                    }
                    Type::Record { name, fields } | Type::Package { name, fields } => {
                        fee(left, fields.len().saturating_add(2).saturating_mul(16))?;
                        bytes.push(if matches!(ty, Type::Package { .. }) {
                            13
                        } else {
                            6
                        });
                        for (name, ty) in fields.iter().rev() {
                            pending.push(Event::Type(ty, child_depth));
                            pending.push(Event::Text(name));
                        }
                        pending.push(Event::Word(fields.len()));
                        pending.push(Event::Text(name));
                    }
                    Type::Enum { name, variants } => {
                        fee(left, variants.len().saturating_add(2).saturating_mul(16))?;
                        bytes.push(9);
                        for (name, fields) in variants.iter().rev() {
                            fee(left, fields.len().saturating_add(2).saturating_mul(8))?;
                            pending
                                .extend(fields.iter().rev().map(|ty| Event::Type(ty, child_depth)));
                            pending.push(Event::Word(fields.len()));
                            pending.push(Event::Text(name));
                        }
                        pending.push(Event::Word(variants.len()));
                        pending.push(Event::Text(name));
                    }
                    Type::ErrorUnion { error, result } => {
                        bytes.push(12);
                        pending.push(Event::Type(result, child_depth));
                        pending.push(Event::Type(error, child_depth));
                    }
                    Type::BorrowedStorage { storage: inner, .. }
                    | Type::Task { result: inner, .. } => {
                        // These structural digests include all relation metadata.
                        // The appended occurrence encoding resolves Closure IDs in
                        // their actual payload too, rather than treating IDs as ABI.
                        let cost = inference::type_snapshot_work(
                            ty,
                            Span::default(),
                            &Limits {
                                max_analysis_steps: *left,
                                ..limits.clone()
                            },
                        )?;
                        fee(left, cost.saturating_mul(64))?;
                        bytes.push(if matches!(ty, Type::Task { .. }) {
                            34
                        } else {
                            35
                        });
                        bytes.extend_from_slice(&ty.fingerprint().0);
                        pending.push(Event::Type(inner, child_depth));
                    }
                    Type::Closure(id) => {
                        fee(left, active.len().saturating_add(1))?;
                        if active.contains(id) {
                            return Err(invalid("cyclic closure identity in exported interface"));
                        }
                        let definition = closures
                            .get(id.0 as usize)
                            .filter(|d| d.id == *id)
                            .ok_or_else(|| invalid("missing actual exported closure definition"))?;
                        fee(
                            left,
                            definition
                                .captures
                                .len()
                                .saturating_add(definition.parameters.len())
                                .saturating_add(definition.lending_captures.len())
                                .saturating_add(3)
                                .saturating_mul(16),
                        )?;
                        bytes.push(14);
                        bytes.extend_from_slice(&id.0.to_le_bytes());
                        bytes.push(match definition.kind {
                            CallableKind::Fn => 0,
                            CallableKind::FnMut => 1,
                            CallableKind::FnOnce => 2,
                            CallableKind::FunctionPointer => 3,
                        });
                        bytes.push(u8::from(definition.storage_life.is_some()));
                        if let Some(life) = &definition.storage_life {
                            contract(life, bytes, limits, left)?;
                        }
                        bytes.extend_from_slice(
                            &(definition.lending_captures.len() as u64).to_le_bytes(),
                        );
                        for capture in &definition.lending_captures {
                            bytes.extend_from_slice(&capture.to_le_bytes());
                        }
                        active.push(*id);
                        pending.push(Event::EndClosure(*id));
                        pending.push(Event::Type(&definition.result, child_depth));
                        pending.extend(
                            definition
                                .parameters
                                .iter()
                                .rev()
                                .map(|ty| Event::Type(ty, child_depth)),
                        );
                        pending.push(Event::Word(definition.parameters.len()));
                        pending.extend(
                            definition
                                .captures
                                .iter()
                                .rev()
                                .map(|ty| Event::Type(ty, child_depth)),
                        );
                        pending.push(Event::Word(definition.captures.len()));
                    }
                    Type::Dyn { .. }
                    | Type::Named(_)
                    | Type::Opaque { .. }
                    | Type::Unsupported(_) => {
                        return Err(unsupported(
                            Span::default(),
                            "unresolved exported interface type",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}
fn contract(
    life: &StorageLifeContract,
    bytes: &mut Vec<u8>,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    borrowed_fp::charge_contract(life, |work| fee(left, work), 64)?;
    for length in life
        .bindings
        .iter()
        .map(|r| r.position.path.len())
        .chain(life.classes.iter().map(|r| r.position.path.len()))
    {
        if length > limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "interface relation path depth exceeded",
                None,
            ));
        }
    }
    bytes.extend_from_slice(&life.fingerprint().0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn closure(id: u32, captures: Vec<Type>) -> ClosureDefinition {
        ClosureDefinition {
            id: ClosureId(id),
            function: FunctionId(id),
            captures,
            parameters: vec![Type::I32],
            result: Type::I64,
            kind: CallableKind::Fn,
            lending_captures: vec![],
            storage_life: None,
        }
    }
    fn encoded(
        ty: &Type,
        definitions: &[ClosureDefinition],
        work: usize,
        depth: usize,
    ) -> R<Vec<u8>> {
        let mut bytes = Vec::new();
        encode(
            ty,
            definitions,
            &mut bytes,
            &Limits {
                max_call_depth: depth,
                ..Limits::default()
            },
            &mut { work },
        )?;
        Ok(bytes)
    }
    fn minimum(ty: &Type, definitions: &[ClosureDefinition]) -> usize {
        let (mut low, mut high) = (0, 1_000_000);
        assert!(encoded(ty, definitions, high, 256).is_ok());
        while low < high {
            let middle = low + (high - low) / 2;
            if encoded(ty, definitions, middle, 256).is_ok() {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        low
    }

    #[test]
    fn nested_actual_closure_export_has_exact_occurrence_budget_and_context_identity() {
        let root = Type::Closure(ClosureId(0));
        let narrow = vec![
            closure(0, vec![Type::Closure(ClosureId(1))]),
            closure(1, vec![Type::I64]),
        ];
        let mut wide = narrow.clone();
        wide[1].captures = vec![Type::Tuple(vec![Type::I64; 64]); 4];
        let n = minimum(&root, &wide);
        assert!(n > minimum(&root, &narrow));
        let bytes = encoded(&root, &wide, n, 256).unwrap();
        assert!(encoded(&root, &wide, n - 1, 256).is_err());
        assert_eq!(encoded(&root, &wide, n, 256).unwrap(), bytes);
        assert_ne!(bytes, encoded(&root, &narrow, n, 256).unwrap());
        let mut changed = wide.clone();
        changed[1].lending_captures.push(0);
        assert_ne!(bytes, encoded(&root, &changed, 1_000_000, 256).unwrap());
        let mut changed = wide;
        changed[1].storage_life = StorageLifeContract::from_flat(FunctionLifeContract {
            binder_count: 1,
            parameter_lives: vec![Some(0)],
            result_life: Some(0),
        });
        let with_contract = encoded(&root, &changed, 1_000_000, 256).unwrap();
        assert_ne!(bytes, with_contract);
        changed[1].storage_life.as_mut().unwrap().bindings[1].binder = StorageBinder::Fixed(0);
        assert_ne!(
            with_contract,
            encoded(&root, &changed, 1_000_000, 256).unwrap()
        );
        assert!(encoded(&root, &narrow, 1_000_000, 0).is_err());
    }

    #[test]
    fn invalid_or_cyclic_exported_closure_reference_is_a_bounded_error() {
        let root = Type::Closure(ClosureId(0));
        assert!(encoded(&root, &[], 1000, 256).is_err());
        assert!(encoded(&root, &[closure(1, vec![])], 1000, 256).is_err());
        let cycle = vec![
            closure(0, vec![Type::Closure(ClosureId(1))]),
            closure(1, vec![root.clone()]),
        ];
        let error = encoded(&root, &cycle, 1_000_000, 256).unwrap_err();
        assert!(error.message.contains("cyclic closure"));
        let mut deep = Type::I32;
        for _ in 0..257 {
            deep = Type::Tuple(vec![deep]);
        }
        assert!(encoded(&deep, &[], 1_000_000, 256).is_err());
        assert!(encoded(&deep, &[], 1_000_000, 258).is_ok());
    }
}
