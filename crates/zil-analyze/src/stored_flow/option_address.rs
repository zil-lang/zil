//! Sealed Option address-actual proof. Metadata never supplies a target root.
use super::*;

pub(super) fn declaration(d: &LifeNominalDeclaration) -> bool {
    d.id.compiler
        && d.id.module.0 == u32::MAX
        && d.id.declaration.0 == 0
        && d.identity.package == "@zil.builtin"
        && d.identity.module_path == ["option"]
        && d.identity.declaration_path == ["enum", "Option"]
        && d.identity.generation_path.is_empty()
        && d.kind == LifeNominalKind::Enum
        && d.parameters == [LifeParameterKind::Type]
}

pub(crate) fn shape(ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    let Some((storage, life)) = carrier(ty) else {
        return Ok(false);
    };
    let Type::Enum { name, variants } = storage else {
        return Ok(false);
    };
    if life.slots.is_empty() {
        return Ok(false);
    }
    fee(b, 8)?;
    let [(none, empty), (some, fields)] = variants.as_slice() else {
        return Ok(false);
    };
    let [payload] = fields.as_slice() else {
        return Ok(false);
    };
    if name.is_empty() || none != "None" || !empty.is_empty() || some != "Some" {
        return Ok(false);
    }
    let Type::Address {
        mutable: false,
        inner,
    } = payload
    else {
        return Ok(false);
    };
    b.type_work(inner, true, "Option actual shared pointee")?;
    let pointee = match inner.as_ref() {
        Type::Slice { element } => element.as_ref(),
        other => other,
    };
    if !closed_pointee(pointee, b, 0)? && !matches!(pointee, Type::Package { .. }) {
        return Ok(false);
    }
    let [slot] = life.slots.as_slice() else {
        return Ok(false);
    };
    let [map] = life.retained.as_slice() else {
        return Ok(false);
    };
    Ok(life.binder_count == 1
        && life.nominal_arguments.is_empty()
        && slot.path
            == [PlaceProjection::Variant(zil_ir::VariantProjection {
                variant: 1,
                index: 0,
            })]
        && slot.binder == 0
        && slot.mode == StoredBorrowMode::SharedAddress
        && map.path == [LifeTypeProjection::TypeArgument(0)]
        && map.class == 0
        && map.binder == slot.binder)
}

/// Authenticate the complete operation before either preflight or permission flow.
/// This is private to this kernel; no proof is shared with another consumer.
pub(super) fn types<'a>(
    module: &TypedModule,
    f: &'a TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<Option<&'a Type>> {
    let ty = match &i.kind {
        InstructionKind::MakeVariant { .. } => value_type(
            f,
            i.result
                .ok_or_else(|| failure("Option constructor lacks result"))?,
        )?,
        InstructionKind::ReadVariant { value, .. } | InstructionKind::VariantTag { value } => {
            value_type(f, *value)?
        }
        _ => return Ok(None),
    };
    if !shape(ty, b)? {
        return Ok(None);
    }
    fee(b, module.life_instances.len())?;
    let mut matches = 0usize;
    for instance in &module.life_instances {
        b.type_work(&instance.ty, true, "Option complete instance identity")?;
        b.type_work(ty, true, "Option operation complete identity")?;
        if instance.ty == *ty {
            matches += 1;
        }
    }
    if matches != 1 {
        return Err(failure("Option operation lacks a unique complete instance"));
    }
    fee(b, 4)?;
    let result = value_type(
        f,
        i.result
            .ok_or_else(|| failure("Option operation lacks result"))?,
    )?;
    b.type_work(result, true, "Option actual result equality")?;
    let Type::BorrowedStorage { storage, .. } = ty else {
        unreachable!()
    };
    let Type::Enum { variants, .. } = storage.as_ref() else {
        unreachable!()
    };
    match &i.kind {
        InstructionKind::MakeVariant { variant, fields } => {
            let (_, payload) = variants
                .get(*variant as usize)
                .ok_or_else(|| failure("Option variant is absent"))?;
            if payload.len() != fields.len() {
                return Err(failure("Option constructor payload arity differs"));
            }
            fee(b, fields.len())?;
            for (id, expected) in fields.iter().zip(payload) {
                b.type_work(expected, true, "Option payload equality")?;
                b.type_work(value_type(f, *id)?, true, "Option actual payload equality")?;
                if value_type(f, *id)? != expected {
                    return Err(failure("Option payload complete type differs"));
                }
            }
        }
        InstructionKind::ReadVariant { variant, index, .. } => {
            b.type_work(&variants[1].1[0], true, "Option projected payload equality")?;
            if *variant != 1 || *index != 0 || result != &variants[1].1[0] {
                return Err(failure(
                    "Option projection differs from actual Some payload",
                ));
            }
        }
        InstructionKind::VariantTag { .. } => {
            if result != &Type::I32 {
                return Err(failure("Option tag result is not i32"));
            }
        }
        _ => unreachable!(),
    }
    Ok(Some(ty))
}

/// Each operation transports actual permissions; the class row alone never
/// creates a source. ReadVariant retains its existing runtime tag check.
pub(super) fn transfer(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    values: &[Value],
    b: &mut Budget,
) -> CompileResult<Option<Value>> {
    let Some(ty) = types(module, f, i, b)? else {
        return Ok(None);
    };
    let get = |id: ValueId| {
        values
            .get(id.0 as usize)
            .ok_or_else(|| failure("Option input outside actual values"))
    };
    let mut out = Value::default();
    let Type::BorrowedStorage { life, .. } = ty else {
        unreachable!()
    };
    let path = &life.slots[0].path;
    match &i.kind {
        InstructionKind::MakeVariant { variant, fields } => {
            out.option_variants.constructed = 1u8
                .checked_shl(*variant)
                .ok_or_else(|| failure("Option variant bit is invalid"))?;
            for id in fields {
                let mut permission = Permission::default();
                union(&mut permission, &get(*id)?.pointer, b)?;
                fee(b, path.len().saturating_add(1))?;
                out.slots.insert(path.clone(), permission);
            }
        }
        InstructionKind::ReadVariant { value, .. } => {
            if let Some(permission) = get(*value)?.slots.get(path) {
                union(&mut out.pointer, permission, b)?;
            }
        }
        InstructionKind::VariantTag { .. } => {}
        _ => unreachable!(),
    }
    Ok(Some(out))
}

// The sealed profile matches Types' finite ordinary data domain, including
// inactive payloads. General is_address_pointee also admits dyn and code values.
pub(super) fn closed_pointee(ty: &Type, b: &mut Budget, depth: usize) -> CompileResult<bool> {
    fee(b, 1)?;
    if depth > b.partial_depth {
        return Err(failure("Option pointee exceeds actual depth limit"));
    }
    Ok(match ty {
        Type::Unit
        | Type::Bool
        | Type::I32
        | Type::I64
        | Type::U64
        | Type::Usize
        | Type::U8
        | Type::U16
        | Type::F32
        | Type::F64 => true,
        Type::Tuple(fields) => {
            for child in fields {
                if !closed_pointee(child, b, depth + 1)? {
                    return Ok(false);
                }
            }
            true
        }
        Type::Record { name, fields } => {
            fee(b, name.len())?;
            for (name, child) in fields {
                fee(b, name.len())?;
                if !closed_pointee(child, b, depth + 1)? {
                    return Ok(false);
                }
            }
            true
        }
        Type::Enum { name, variants } => {
            fee(b, name.len())?;
            for (name, fields) in variants {
                fee(b, name.len())?;
                for child in fields {
                    if !closed_pointee(child, b, depth + 1)? {
                        return Ok(false);
                    }
                }
            }
            true
        }
        Type::Array { element, .. } => closed_pointee(element, b, depth + 1)?,
        Type::ErrorUnion { error, result } => {
            closed_pointee(error, b, depth + 1)? & closed_pointee(result, b, depth + 1)?
        }
        _ => false,
    })
}

/// Bottom is no producer evidence; input dependencies conservatively evaluate
/// to both variants until a call substitutes the actual caller's evidence.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Variants {
    constructed: u8,
    inputs: Vec<u32>,
}
impl Variants {
    pub(super) fn mask(&self) -> u8 {
        self.constructed | if self.inputs.is_empty() { 0 } else { 3 }
    }
    pub(super) fn none(&self) -> bool {
        self.mask() == 1
    }
    pub(super) fn work(&self, b: &mut Budget) -> CompileResult<()> {
        if self.mask() != 0 {
            let n = self.inputs.len().saturating_add(1);
            fee(b, n)?;
        }
        Ok(())
    }
    pub(super) fn join(&mut self, other: &Self, b: &mut Budget) -> CompileResult<bool> {
        if other.mask() == 0 {
            return Ok(false);
        }
        other.work(b)?;
        let bits = self.constructed | other.constructed;
        let mut changed = bits != self.constructed;
        self.constructed = bits;
        for input in &other.inputs {
            let n = self.inputs.len().saturating_add(1);
            fee(b, n)?;
            if !self.inputs.contains(input) {
                self.inputs.push(*input);
                changed = true;
            }
        }
        if changed {
            let n = self
                .inputs
                .len()
                .saturating_mul(self.inputs.len())
                .saturating_add(1);
            fee(b, n)?;
            self.inputs.sort_unstable();
        }
        Ok(changed)
    }
    pub(super) fn parameter(index: u32, b: &mut Budget) -> CompileResult<Self> {
        let n = 2;
        fee(b, n)?;
        Ok(Self {
            constructed: 0,
            inputs: vec![index],
        })
    }
    pub(super) fn substitute(
        &self,
        arguments: &[ValueId],
        values: &[Value],
        b: &mut Budget,
    ) -> CompileResult<Self> {
        self.work(b)?;
        let mut out = Self {
            constructed: self.constructed,
            inputs: Vec::new(),
        };
        for input in &self.inputs {
            let n = 1;
            fee(b, n)?;
            let id = arguments
                .get(*input as usize)
                .ok_or_else(|| failure("Option summary input is absent"))?;
            let actual = values
                .get(id.0 as usize)
                .ok_or_else(|| failure("Option summary actual is absent"))?;
            out.join(&actual.option_variants, b)?;
        }
        Ok(out)
    }
}

// Signature class traversal is metadata-only dispatch. Full declaration,
// pointee, slot and map authentication remains in this kernel's inventory.
// Its bounded comparisons are prepaid by the stored signature caller.
pub(super) fn candidate(ty: &Type) -> bool {
    matches!(ty, Type::BorrowedStorage { storage, life } if matches!(storage.as_ref(), Type::Enum { .. }) && !life.slots.is_empty())
}

pub(crate) fn signature(ty: &Type) -> bool {
    let Type::BorrowedStorage { storage, life } = ty else {
        return false;
    };
    let Type::Enum { variants, .. } = storage.as_ref() else {
        return false;
    };
    if variants.len() != 2
        || variants[0].0 != "None"
        || !variants[0].1.is_empty()
        || variants[1].0 != "Some"
        || variants[1].1.len() != 1
        || !matches!(&variants[1].1[0], Type::Address { mutable: false, .. })
        || life.binder_count != 1
        || !life.nominal_arguments.is_empty()
    {
        return false;
    }
    matches!((life.slots.as_slice(), life.retained.as_slice()), ([slot], [map])
        if slot.binder == 0 && slot.mode == StoredBorrowMode::SharedAddress
        && matches!(slot.path.as_slice(), [PlaceProjection::Variant(p)] if p.variant == 1 && p.index == 0)
        && map.class == 0 && map.binder == 0 && map.path == [LifeTypeProjection::TypeArgument(0)])
}

// Only a completed proof on the original graph may supply these None facts.
// The caller invokes this once; the replacement solve cannot refine again.
pub(super) struct Refinement {
    pub(super) graphs: Vec<(Vec<Vec<usize>>, Vec<bool>)>,
    pub(super) inactive: Vec<BTreeSet<ValueId>>,
}
pub(super) fn refine(
    module: &TypedModule,
    original: &[(Vec<Vec<usize>>, Vec<bool>)],
    frozen: &[Facts],
    b: &mut Budget,
) -> CompileResult<Option<Refinement>> {
    fee(b, module.functions.len().saturating_mul(4))?;
    let mut proof = Refinement {
        graphs: Vec::with_capacity(module.functions.len()),
        inactive: Vec::with_capacity(module.functions.len()),
    };
    let mut changed = false;
    for (fi, f) in module.functions.iter().enumerate() {
        fee(
            b,
            f.values
                .len()
                .saturating_add(f.blocks.len().saturating_mul(5)),
        )?;
        let mut definitions = vec![None; f.values.len()];
        for block in &f.blocks {
            for i in &block.instructions {
                fee(b, 1)?;
                if let Some(id) = i.result {
                    definitions[id.0 as usize] = Some(&i.kind);
                }
            }
        }
        let none = &frozen[fi].none_values;
        let known_equal = |condition: ValueId, b: &mut Budget| -> CompileResult<Option<bool>> {
            fee(b, 3)?;
            if value_type(f, condition)? != &Type::Bool {
                return Ok(None);
            }
            let Some(InstructionKind::Binary {
                op: zil_ir::BinaryOp::Equal,
                left,
                right,
            }) = definitions[condition.0 as usize]
            else {
                return Ok(None);
            };
            if value_type(f, *left)? != &Type::I32 || value_type(f, *right)? != &Type::I32 {
                return Ok(None);
            }
            for (tag, constant) in [(*left, *right), (*right, *left)] {
                fee(b, 2)?;
                let (
                    Some(InstructionKind::VariantTag { value }),
                    Some(InstructionKind::Constant(Constant::I32(n))),
                ) = (
                    definitions[tag.0 as usize],
                    definitions[constant.0 as usize],
                )
                else {
                    continue;
                };
                if !matches!(n, 0 | 1) {
                    continue;
                }
                ordered(b, none.len(), 0)?;
                if none.contains(value) {
                    return Ok(Some(*n == 0));
                }
            }
            Ok(None)
        };
        let mut successors = vec![Vec::new(); f.blocks.len()];
        let mut predecessors = vec![Vec::new(); f.blocks.len()];
        for (bi, block) in f.blocks.iter().enumerate() {
            fee(b, 1)?;
            match block.terminator {
                Terminator::Jump(target) => {
                    fee(b, 2)?;
                    successors[bi].push(target.0 as usize);
                }
                Terminator::Branch {
                    condition,
                    then_block,
                    else_block,
                } => {
                    if let Some(equal) = known_equal(condition, b)? {
                        fee(b, 2)?;
                        successors[bi].push(if equal { then_block } else { else_block }.0 as usize);
                        changed |= then_block != else_block && original[fi].1[bi];
                    } else {
                        fee(b, 4)?;
                        successors[bi].extend([then_block.0 as usize, else_block.0 as usize]);
                    }
                }
                _ => {}
            }
            for target in &successors[bi] {
                fee(b, 1)?;
                predecessors[*target].push(bi);
            }
        }
        let mut reachable = vec![false; f.blocks.len()];
        let mut pending = if f.blocks.is_empty() {
            Vec::new()
        } else {
            vec![f.entry.0 as usize]
        };
        while let Some(bi) = pending.pop() {
            fee(b, 1)?;
            if reachable[bi] {
                continue;
            }
            reachable[bi] = true;
            fee(b, successors[bi].len().saturating_mul(2))?;
            pending.extend(&successors[bi]);
        }
        let mut inactive = BTreeSet::new();
        for (bi, block) in f.blocks.iter().enumerate() {
            fee(b, 1)?;
            if original[fi].1[bi] && !reachable[bi] {
                for i in &block.instructions {
                    fee(b, 1)?;
                    if let Some(id) = i.result {
                        ordered(b, inactive.len(), 0)?;
                        fee(b, 1)?;
                        inactive.insert(id);
                    }
                }
            }
        }
        proof.graphs.push((predecessors, reachable));
        proof.inactive.push(inactive);
    }
    Ok(changed.then_some(proof))
}

#[cfg(test)]
#[test]
fn snapshot_keeps_original_join_normalization_of_option_input_rows() {
    let value = Value {
        option_variants: Variants {
            constructed: 1,
            inputs: vec![3, 1, 3],
        },
        ..Value::default()
    };
    let mut old = Budget::new(&Limits::default());
    let mut new = Budget::new(&Limits::default());
    let snapshot = super::snapshot(&value, &mut new).unwrap();
    assert_eq!(
        snapshot,
        super::snapshot_by_union(&value, &mut old).unwrap()
    );
    assert_eq!(snapshot.option_variants.inputs, vec![1, 3]);
}
