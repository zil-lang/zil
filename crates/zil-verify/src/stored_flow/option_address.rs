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

pub(super) fn shape(ty: &Type, limits: &Limits, b: &mut Budget) -> Check<bool> {
    let Some((storage, life)) = qualified(ty) else {
        return Ok(false);
    };
    let Type::Enum { name, variants } = storage else {
        return Ok(false);
    };
    if life.slots.is_empty() {
        return Ok(false);
    }
    b.spend(8)?;
    if name.is_empty() || variants.len() != 2 {
        return Ok(false);
    }
    for (i, (name, payload)) in variants.iter().enumerate() {
        if (i == 0 && (name != "None" || !payload.is_empty()))
            || (i == 1 && (name != "Some" || payload.len() != 1))
        {
            return Ok(false);
        }
    }
    match &variants[1].1[0] {
        Type::Address {
            mutable: false,
            inner,
        } => {
            type_work(inner, limits, b)?;
            let pointee = match inner.as_ref() {
                Type::Slice { element } => element.as_ref(),
                other => other,
            };
            if !closed_pointee(pointee, limits, b)? {
                return Ok(false);
            }
        }
        _ => return Ok(false),
    }
    if life.binder_count != 1
        || !life.nominal_arguments.is_empty()
        || life.slots.len() != 1
        || life.retained.len() != 1
    {
        return Ok(false);
    }
    let slot = &life.slots[0];
    let map = &life.retained[0];
    Ok(
        matches!(slot.path.as_slice(), [PlaceProjection::Variant(p)] if p.variant == 1 && p.index == 0)
            && slot.mode == StoredBorrowMode::SharedAddress
            && slot.binder == 0
            && matches!(map.path.as_slice(), [LifeTypeProjection::TypeArgument(0)])
            && map.class == 0
            && map.binder == 0,
    )
}

/// Authenticate the complete operation before either preflight or permission flow.
/// This is private to this kernel; no proof is shared with another consumer.
pub(super) fn types<'a>(
    module: &TypedModule,
    f: &'a TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<Option<&'a Type>> {
    let ty = match &i.kind {
        InstructionKind::MakeVariant { .. } => vt(
            f,
            i.result
                .ok_or_else(|| err("Option constructor lacks result"))?,
        )?,
        InstructionKind::ReadVariant { value, .. } | InstructionKind::VariantTag { value } => {
            vt(f, *value)?
        }
        _ => return Ok(None),
    };
    let limits = b.limits.clone();
    if !shape(ty, &limits, b)? {
        return Ok(None);
    }
    qualification(module, ty, &limits, b)?;
    b.spend(4)?;
    let result = vt(
        f,
        i.result
            .ok_or_else(|| err("Option operation lacks result"))?,
    )?;
    type_work(result, &limits, b)?;
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
                .ok_or_else(|| err("Option variant is absent"))?;
            if payload.len() != fields.len() {
                return Err(err("Option constructor payload arity differs"));
            }
            b.spend(fields.len())?;
            for (id, expected) in fields.iter().zip(payload) {
                type_work(expected, &limits, b)?;
                type_work(vt(f, *id)?, &limits, b)?;
                if vt(f, *id)? != expected {
                    return Err(err("Option payload complete type differs"));
                }
            }
        }
        InstructionKind::ReadVariant { variant, index, .. } => {
            type_work(&variants[1].1[0], &limits, b)?;
            if *variant != 1 || *index != 0 || result != &variants[1].1[0] {
                return Err(err("Option projection differs from actual Some payload"));
            }
        }
        InstructionKind::VariantTag { .. } => {
            if result != &Type::I32 {
                return Err(err("Option tag result is not i32"));
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
    values: &[Payload],
    b: &mut Budget,
) -> Check<Option<Payload>> {
    let Some(ty) = types(module, f, i, b)? else {
        return Ok(None);
    };
    let get = |id: ValueId| {
        values
            .get(id.0 as usize)
            .ok_or_else(|| err("Option input outside actual values"))
    };
    let mut out = Payload::default();
    let Type::BorrowedStorage { life, .. } = ty else {
        unreachable!()
    };
    let path = &life.slots[0].path;
    match &i.kind {
        InstructionKind::MakeVariant { variant, fields } => {
            out.option_variants.constructed = 1u8
                .checked_shl(*variant)
                .ok_or_else(|| err("Option variant bit is invalid"))?;
            for id in fields {
                add_leaf(&mut out, path, &get(*id)?.address, b)?;
            }
        }
        InstructionKind::ReadVariant { value, .. } => {
            if let Some(permission) = leaf(get(*value)?, path, b)? {
                join_permission(&mut out.address, permission, b)?;
            }
        }
        InstructionKind::VariantTag { .. } => {}
        _ => unreachable!(),
    }
    Ok(Some(out))
}

// Iteratively reconstruct exactly the ordinary data domain. Code, dyn, owning,
// nested address and qualified classes are not borrowed through this profile.
pub(super) fn closed_pointee(ty: &Type, limits: &Limits, b: &mut Budget) -> Check<bool> {
    b.spend(1)?;
    let mut todo = vec![(ty, 0usize)];
    while let Some((ty, depth)) = todo.pop() {
        b.spend(1)?;
        if depth > limits.max_call_depth {
            return Err(err("Option pointee exceeds actual depth limit"));
        }
        match ty {
            Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::U8
            | Type::U16
            | Type::F32
            | Type::F64 => {}
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                todo.extend(fields.iter().map(|t| (t, depth + 1)));
            }
            Type::Record { name, fields } => {
                b.spend(name.len().saturating_add(fields.len()))?;
                for (name, child) in fields {
                    b.spend(name.len())?;
                    todo.push((child, depth + 1));
                }
            }
            Type::Enum { name, variants } => {
                b.spend(name.len())?;
                for (name, fields) in variants {
                    b.spend(name.len().saturating_add(fields.len()))?;
                    todo.extend(fields.iter().map(|t| (t, depth + 1)));
                }
            }
            Type::Array { element, .. } => {
                b.spend(1)?;
                todo.push((element, depth + 1));
            }
            Type::ErrorUnion { error, result } => {
                b.spend(2)?;
                todo.push((error, depth + 1));
                todo.push((result, depth + 1));
            }
            _ => return Ok(false),
        }
    }
    Ok(true)
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
    pub(super) fn work(&self, b: &mut Budget) -> Check<()> {
        if self.mask() != 0 {
            let n = self.inputs.len().saturating_add(1);
            b.spend(n)?;
        }
        Ok(())
    }
    pub(super) fn join(&mut self, other: &Self, b: &mut Budget) -> Check<bool> {
        if other.mask() == 0 {
            return Ok(false);
        }
        other.work(b)?;
        let bits = self.constructed | other.constructed;
        let mut changed = bits != self.constructed;
        self.constructed = bits;
        for input in &other.inputs {
            let n = self.inputs.len().saturating_add(1);
            b.spend(n)?;
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
            b.spend(n)?;
            self.inputs.sort_unstable();
        }
        Ok(changed)
    }
    pub(super) fn parameter(index: u32, b: &mut Budget) -> Check<Self> {
        let n = 2;
        b.spend(n)?;
        Ok(Self {
            constructed: 0,
            inputs: vec![index],
        })
    }
    pub(super) fn substitute(
        &self,
        arguments: &[ValueId],
        values: &[Payload],
        b: &mut Budget,
    ) -> Check<Self> {
        self.work(b)?;
        let mut out = Self {
            constructed: self.constructed,
            inputs: Vec::new(),
        };
        for input in &self.inputs {
            let n = 1;
            b.spend(n)?;
            let id = arguments
                .get(*input as usize)
                .ok_or_else(|| err("Option summary input is absent"))?;
            let actual = values
                .get(id.0 as usize)
                .ok_or_else(|| err("Option summary actual is absent"))?;
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

pub(super) struct Refinement {
    pub(super) graphs: Vec<FunctionGraph>,
    pub(super) inactive: Vec<Vec<ValueId>>,
}
/// This reads only the fixed point of the original graph. Its output is used
/// for one permission replay; the replay is never allowed to refine again.
pub(super) fn refine(
    module: &TypedModule,
    original: &[FunctionGraph],
    frozen: &[Summary],
    b: &mut Budget,
) -> Check<Option<Refinement>> {
    b.spend(module.functions.len().saturating_mul(4))?;
    let mut result = Refinement {
        graphs: Vec::with_capacity(module.functions.len()),
        inactive: Vec::with_capacity(module.functions.len()),
    };
    let mut removed = false;
    for (fi, f) in module.functions.iter().enumerate() {
        let old = &original[fi];
        b.spend(
            f.values
                .len()
                .saturating_add(f.blocks.len().saturating_mul(6)),
        )?;
        let mut producers = vec![None; f.values.len()];
        for block in &f.blocks {
            for instruction in &block.instructions {
                b.spend(1)?;
                if let Some(id) = instruction.result {
                    producers[id.0 as usize] = Some(&instruction.kind);
                }
            }
        }
        let truth = |condition: ValueId, b: &mut Budget| -> Check<Option<bool>> {
            b.spend(3)?;
            if vt(f, condition)? != &Type::Bool {
                return Ok(None);
            }
            let Some(InstructionKind::Binary { op, left, right }) = producers[condition.0 as usize]
            else {
                return Ok(None);
            };
            if *op != zil_ir::BinaryOp::Equal
                || vt(f, *left)? != &Type::I32
                || vt(f, *right)? != &Type::I32
            {
                return Ok(None);
            }
            for (tag, scalar) in [(*left, *right), (*right, *left)] {
                b.spend(2)?;
                let Some(InstructionKind::Constant(Constant::I32(value))) =
                    producers[scalar.0 as usize]
                else {
                    continue;
                };
                if *value != 0 && *value != 1 {
                    continue;
                }
                let Some(InstructionKind::VariantTag { value: actual }) = producers[tag.0 as usize]
                else {
                    continue;
                };
                b.spend(frozen[fi].none_values.len().saturating_add(1))?;
                if frozen[fi].none_values.contains(actual) {
                    return Ok(Some(*value == 0));
                }
            }
            Ok(None)
        };
        let mut successors = vec![vec![]; f.blocks.len()];
        let mut predecessors = vec![vec![]; f.blocks.len()];
        for (bi, block) in f.blocks.iter().enumerate() {
            b.spend(1)?;
            match block.terminator {
                Terminator::Jump(target) => {
                    b.spend(2)?;
                    successors[bi].push(target.0 as usize);
                }
                Terminator::Branch {
                    condition,
                    then_block,
                    else_block,
                } => match truth(condition, b)? {
                    Some(take_then) => {
                        b.spend(2)?;
                        let target = if take_then { then_block } else { else_block };
                        successors[bi].push(target.0 as usize);
                        removed |= then_block != else_block && old.reachable[bi];
                    }
                    None => {
                        b.spend(4)?;
                        successors[bi].extend([then_block.0 as usize, else_block.0 as usize]);
                    }
                },
                _ => {}
            }
            for target in &successors[bi] {
                b.spend(1)?;
                predecessors[*target].push(bi);
            }
        }
        let mut reachable = vec![false; f.blocks.len()];
        let mut todo = if f.blocks.is_empty() {
            Vec::new()
        } else {
            vec![f.entry.0 as usize]
        };
        while let Some(bi) = todo.pop() {
            b.spend(1)?;
            if reachable[bi] {
                continue;
            }
            reachable[bi] = true;
            b.spend(successors[bi].len().saturating_mul(2))?;
            todo.extend(&successors[bi]);
        }
        let mut inactive = Vec::new();
        for (bi, block) in f.blocks.iter().enumerate() {
            b.spend(1)?;
            if old.reachable[bi] && !reachable[bi] {
                for instruction in &block.instructions {
                    b.spend(1)?;
                    if let Some(id) = instruction.result {
                        b.spend(2)?;
                        inactive.push(id);
                    }
                }
            }
        }
        b.spend(
            old.package_views
                .by_local
                .len()
                .saturating_add(old.package_views.by_capture.len())
                .saturating_add(old.role_data_uses.len()),
        )?;
        result.graphs.push(FunctionGraph {
            successors,
            predecessors,
            reachable,
            package_views: packages::CaptureViews {
                by_local: old.package_views.by_local.clone(),
                by_capture: old.package_views.by_capture.clone(),
            },
            role_data_uses: old.role_data_uses.clone(),
        });
        result.inactive.push(inactive);
    }
    Ok(removed.then_some(result))
}
