//! Complete raw-reference geometry at actual SSA/local coordinates. This reads
//! immutable type structure only; initializedness, classes and loans stay in
//! the actual equation and memory passes.
use super::*;

pub(super) fn nested(ty: &Type) -> bool {
    matches!(ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Address { .. } | Type::Desc(_)))
}

pub(super) fn shape(
    module: &TypedModule,
    function: &TypedFunction,
    operand: SemanticOperand,
    ty: &Type,
    b: &mut Budget,
) -> CompileResult<Option<StorageLifeShape>> {
    if !nested(ty) {
        return Ok(None);
    }
    let mut values = BTreeSet::new();
    let mut locals = BTreeSet::new();
    match operand {
        SemanticOperand::Value(value) => {
            values.insert(value);
        }
        SemanticOperand::Local(local) => {
            locals.insert(local);
        }
        _ => {
            return Err(bad(
                "nested raw reference has no actual whole-value coordinate",
            ));
        }
    }
    // Load/whole Store preserve geometry even across a join. This supplies no
    // initialized occupant or target; the ordinary CFG pass checks those.
    loop {
        let before = values.len().saturating_add(locals.len());
        for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
            b.spend(1, "nested raw reference geometry edges")?;
            let edge = match instruction.kind {
                InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                    instruction.result.map(|value| (local, value))
                }
                InstructionKind::Store { local, value } => Some((local, value)),
                _ => None,
            };
            let Some((local, value)) = edge else {
                continue;
            };
            if function.locals.get(local.0 as usize).map(|row| &row.ty) != Some(ty)
                || function.values.get(value.0 as usize) != Some(ty)
            {
                continue;
            }
            b.lookup(
                values.len().saturating_add(locals.len()),
                "nested raw reference geometry coordinates",
            )?;
            if values.contains(&value) || locals.contains(&local) {
                values.insert(value);
                locals.insert(local);
            }
        }
        if values.len().saturating_add(locals.len()) == before {
            break;
        }
    }
    let mut selected = None;
    if let Some(program) = &module.semantic {
        let mut worlds = BTreeSet::new();
        for world in &program.instances {
            b.spend(1, "nested raw reference geometry worlds")?;
            if matches!(world.target, SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) if id == function.id)
            {
                worlds.insert(world.id);
            }
        }
        for site in &program.sites {
            b.lookup(worlds.len(), "nested raw reference geometry world")?;
            if !worlds.contains(&site.instance) {
                continue;
            }
            let claim = match &site.action {
                SemanticSiteAction::TypeUse {
                    subject: SemanticOperand::Value(value),
                    expected,
                }
                | SemanticSiteAction::Construct {
                    result: SemanticOperand::Value(value),
                    value: expected,
                } if values.contains(value) => Some(expected),
                SemanticSiteAction::TypeUse {
                    subject: SemanticOperand::Local(local),
                    expected,
                } if locals.contains(local) => Some(expected),
                SemanticSiteAction::DeclareLocal {
                    local: SemanticLocalTarget::Typed(local),
                    expected,
                } if locals.contains(local) => Some(expected),
                _ => None,
            };
            let Some(SemanticTypeValue::Resolved {
                ty: actual, life, ..
            }) = claim
            else {
                continue;
            };
            super::super::dyn_metadata::ty(actual, b)?;
            super::super::dyn_metadata::shape(life, b)?;
            if actual != ty {
                return Err(bad(
                    "nested raw reference geometry changes its actual complete Type",
                ));
            }
            if selected.as_ref().is_some_and(|old| old != life) {
                return Err(bad(
                    "one actual nested reference has different complete original geometries",
                ));
            }
            if selected.is_none() {
                selected = Some(life.clone());
            }
        }
    }
    if selected.is_some() {
        return Ok(selected);
    }
    // An unused actual parameter may have no executable Name site. Its full
    // header still names each local child coordinate, separately from images.
    if let Some(header) = &function.storage_life {
        super::super::signature_life::charge(header, b)?;
        for (parameter, local) in function.parameters.iter().enumerate() {
            if !locals.contains(local) {
                continue;
            }
            let base = StorageBase::Parameter(parameter as u32);
            let mut head = None;
            for row in &header.bindings {
                if row.position.base == base
                    && row.position.path.is_empty()
                    && head.replace(row.binder).is_some()
                {
                    return Err(bad(
                        "nested reference parameter repeats its actual outer header",
                    ));
                }
            }
            let Some(head) = head else {
                continue;
            };
            let mut binders = BTreeMap::from([(head, 0u32)]);
            let mut retained = Vec::new();
            for row in &header.classes {
                b.spend(
                    row.position.path.len().saturating_add(1),
                    "nested reference complete header child",
                )?;
                if row.position.base != base {
                    continue;
                }
                if row.position.path.first() != Some(&LifeTypeProjection::Pointee) {
                    return Err(bad(
                        "nested reference header child has another intrinsic path",
                    ));
                }
                let next = u32::try_from(binders.len())
                    .map_err(|_| bad("nested reference header domain overflows"))?;
                let binder = *binders.entry(row.binder).or_insert(next);
                retained.push(StorageRetainedBinding {
                    path: row.position.path.clone(),
                    class: row.position.class,
                    binder,
                });
            }
            let Type::Address { mutable, .. } = ty else {
                unreachable!()
            };
            let life = StorageLifeShape {
                binder_count: binders.len() as u32,
                slots: vec![StorageLifeSlot {
                    path: Vec::new(),
                    binder: 0,
                    mode: if *mutable {
                        StoredBorrowMode::ExclusiveAddress
                    } else {
                        StoredBorrowMode::SharedAddress
                    },
                }],
                retained,
                nominal_arguments: Vec::new(),
            };
            if selected.as_ref().is_some_and(|old| old != &life) {
                return Err(bad(
                    "nested raw parameter has several actual full geometries",
                ));
            }
            selected = Some(life);
        }
    }
    selected.map(Some).ok_or_else(|| {
        vec![Diagnostic::unsupported(
            Phase::Analyze,
            "nested raw reference has no complete original SSA/local or actual parameter geometry",
            None,
        )]
    })
}

pub(super) fn whole(ports: &Ports, b: &mut Budget) -> CompileResult<Vec<usize>> {
    if !ports.raw_whole.is_empty() {
        b.spend(
            ports.raw_whole.len(),
            "nested reference whole coordinate copy",
        )?;
        return Ok(ports.raw_whole.clone());
    }
    let outer = ports
        .outer
        .ok_or_else(|| bad("raw reference whole source has no actual outer slot"))?;
    if !ports.classes.is_empty() {
        return Err(bad(
            "raw reference whole source omits its complete child coordinate map",
        ));
    }
    b.spend(1, "raw reference scalar pointee whole coordinate")?;
    Ok(vec![outer])
}
