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

pub(crate) fn shape(ty: &Type, limits: &Limits, b: &mut Budget) -> Check<bool> {
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
            if !closed_pointee(pointee, limits, b)?
                && !(matches!(pointee, Type::Package { .. })
                    && pure_copy::ordinary_owned_value(pointee, b)?)
            {
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

/// The Type actual owns its own class coordinates. Its outer Address class
/// maps through TypeArgument(0) into the enum class; it is not a zero-domain
/// Type merely because the pointee has no stored targets.
pub(super) fn instance(
    module: &TypedModule,
    instance: &LifeNominalInstance,
    limits: &Limits,
    b: &mut Budget,
) -> Check<()> {
    if !shape(&instance.ty, limits, b)? {
        return Err(err("builtin Option has no complete shared-address layout"));
    }
    let Type::BorrowedStorage { storage, .. } = &instance.ty else {
        unreachable!()
    };
    let Type::Enum { variants, .. } = storage.as_ref() else {
        unreachable!()
    };
    let [actual] = instance.arguments.as_slice() else {
        return Err(err("builtin Option omits its complete Type actual"));
    };
    let actual_ty = match actual {
        LifeNominalActual::Type(ty) => ty,
        LifeNominalActual::QualifiedType { ty, life } => {
            type_work(ty, limits, b)?;
            b.spend(
                life.slots
                    .len()
                    .saturating_add(life.retained.len())
                    .saturating_add(life.nominal_arguments.len())
                    .saturating_add(1),
            )?;
            if life.binder_count != 1
                || !life.retained.is_empty()
                || !life.nominal_arguments.is_empty()
                || !matches!(life.slots.as_slice(), [slot]
                    if slot.path.is_empty() && slot.binder == 0
                        && slot.mode == StoredBorrowMode::SharedAddress)
            {
                return Err(err(
                    "Option Type actual omits or changes its own outer borrow class",
                ));
            }
            ty
        }
        _ => {
            return Err(err(
                "builtin Option changes its full-ordinal Type actual kind",
            ));
        }
    };
    type_work(actual_ty, limits, b)?;
    if actual_ty != &variants[1].1[0] {
        return Err(err(
            "Option Type actual differs from the complete Some payload",
        ));
    }
    let Type::Address {
        inner,
        mutable: false,
    } = actual_ty
    else {
        return Err(err("Option Type actual is not its shared outer borrow"));
    };
    let pointee = match inner.as_ref() {
        Type::Slice { element } => element.as_ref(),
        other => other,
    };
    let (classes, pending) = crate::dynamic_witness::intrinsic_domain(module, pointee, b)?;
    if classes != 0 || pending {
        return Err(err(
            "Option outer borrow cannot erase its pointee's persistent domain",
        ));
    }
    crate::dynamic_witness::structural_actual(module, pointee, DynStructuralBound::Value, b)
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
            out.option_variants = Variants::construction(*variant, b)?;
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
            | Type::I8
            | Type::I16
            | Type::Isize
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::U8
            | Type::U16
            | Type::U32
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Input {
    Parameter(u32),
    Capture(u32),
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Variants {
    constructed: u8,
    inputs: Vec<(Input, Path)>,
}
impl Variants {
    /// Record the tag of an independently checked actual Option constructor.
    /// The bit alone supplies no nominal, initialization, source or loan proof.
    pub(super) fn construction(variant: u32, b: &mut Budget) -> Check<Self> {
        b.spend(1)?;
        if variant > 1 {
            return Err(err(
                "Option constructor tag is outside its two real variants",
            ));
        }
        Ok(Self {
            constructed: 1u8 << variant,
            inputs: vec![],
        })
    }
    pub(super) fn mask(&self) -> u8 {
        self.constructed | if self.inputs.is_empty() { 0 } else { 3 }
    }
    pub(super) fn work(&self, b: &mut Budget) -> Check<()> {
        if self.mask() != 0 {
            let n = self.inputs.len().saturating_add(1);
            b.spend(n)?;
            for (_, path) in &self.inputs {
                b.spend(path.len())?;
            }
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
            for (_, path) in &self.inputs {
                b.spend(path.len().saturating_add(input.1.len()).saturating_add(1))?;
            }
            if !self.inputs.contains(input) {
                b.spend(input.1.len())?;
                self.inputs.push(input.clone());
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
            let width = self
                .inputs
                .iter()
                .fold(1usize, |n, (_, path)| n.max(path.len().saturating_add(1)));
            b.spend(
                self.inputs
                    .len()
                    .saturating_mul(self.inputs.len())
                    .saturating_mul(width),
            )?;
            self.inputs.sort_unstable();
        }
        Ok(changed)
    }
    pub(super) fn parameter(index: u32, b: &mut Budget) -> Check<Self> {
        let n = 2;
        b.spend(n)?;
        Ok(Self {
            constructed: 0,
            inputs: vec![(Input::Parameter(index), Vec::new())],
        })
    }
    pub(super) fn parameter_at(
        index: u32,
        path: &[PlaceProjection],
        b: &mut Budget,
    ) -> Check<Self> {
        b.spend(path.len().saturating_add(2))?;
        Ok(Self {
            constructed: 0,
            inputs: vec![(Input::Parameter(index), path.to_vec())],
        })
    }
    pub(super) fn input_at(input: Input, path: &[PlaceProjection], b: &mut Budget) -> Check<Self> {
        b.spend(path.len().saturating_add(2))?;
        Ok(Self {
            constructed: 0,
            inputs: vec![(input, path.to_vec())],
        })
    }
    pub(super) fn substitute_with(
        &self,
        b: &mut Budget,
        mut actual: impl FnMut(Input, &[PlaceProjection], &mut Budget) -> Check<Self>,
    ) -> Check<(Self, bool)> {
        self.work(b)?;
        let mut out = Self {
            constructed: self.constructed,
            inputs: vec![],
        };
        let mut known = true;
        for (input, path) in &self.inputs {
            b.spend(path.len().saturating_add(1))?;
            let value = actual(*input, path, b)?;
            known &= value.mask() != 0;
            out.join(&value, b)?;
        }
        Ok((out, known))
    }
    pub(super) fn substitute_known(
        &self,
        arguments: &[ValueId],
        values: &[Payload],
        b: &mut Budget,
    ) -> Check<(Self, bool)> {
        self.substitute_with(b, |input, path, b| {
            let Input::Parameter(index) = input else {
                return Ok(Self::default());
            };
            let id = arguments
                .get(index as usize)
                .ok_or_else(|| err("Option summary input is absent"))?;
            let actual = values
                .get(id.0 as usize)
                .ok_or_else(|| err("Option summary actual is absent"))?;
            value_transport::option_at(actual, path, b)
        })
    }
    pub(super) fn substitute(
        &self,
        arguments: &[ValueId],
        values: &[Payload],
        b: &mut Budget,
    ) -> Check<Self> {
        let (value, known) = self.substitute_known(arguments, values, b)?;
        Ok(if known { value } else { Self::default() })
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

#[cfg(test)]
mod dependency_tests {
    use super::*;
    #[test]
    fn constructed_none_does_not_discharge_an_unknown_actual_dependency() {
        let mut b = Budget::new(&Limits::default());
        let input = Variants {
            constructed: 1,
            inputs: vec![(Input::Capture(0), vec![PlaceProjection::Field(1)])],
        };
        for mask in [0u8, 1, 2] {
            let (value, known) = input
                .substitute_with(&mut b, |namespace, path, _| {
                    assert_eq!(namespace, Input::Capture(0));
                    assert_eq!(path, [PlaceProjection::Field(1)]);
                    Ok(Variants {
                        constructed: mask,
                        inputs: vec![],
                    })
                })
                .unwrap();
            assert_eq!(known, mask != 0);
            assert_eq!(value.mask(), 1 | mask);
        }
    }
}
