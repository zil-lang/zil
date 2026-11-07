//! Ordinary shared-borrow sums reconstructed from the complete nominal instance.
//! The verifier maintains its own sorted tag sets and actual source leaves.
use super::*;

pub(super) fn poll_declaration(d: &LifeNominalDeclaration) -> bool {
    d.id.compiler
        && d.id.module.0 == u32::MAX
        && d.id.declaration.0 == 1
        && d.identity.package == "@zil.builtin"
        && d.identity.module_path == ["task"]
        && d.identity.declaration_path == ["enum", "Poll"]
        && d.identity.generation_path.is_empty()
        && d.kind == LifeNominalKind::Enum
        && d.parameters == [zil_ir::LifeParameterKind::Type]
}

// Reserved domains are rejection fences; spelling never grants the role.
pub(super) fn poll_reserved(d: &LifeNominalDeclaration) -> bool {
    (d.id.module.0 == u32::MAX && d.id.declaration.0 == 1)
        || (d.identity.package == "@zil.builtin"
            && d.identity.module_path == ["task"]
            && d.identity.declaration_path == ["enum", "Poll"])
}

pub(super) fn candidate(ty: &Type) -> bool {
    matches!(ty, Type::BorrowedStorage { storage, life }
        if matches!(storage.as_ref(), Type::Enum { .. }) && !life.slots.is_empty())
}

pub(crate) fn signature(ty: &Type) -> bool {
    if let Type::ErrorUnion { error, result } = ty {
        return (signature(error) || signature(result))
            && (error.is_copy_value() || signature(error))
            && (result.is_copy_value() || signature(result));
    }
    matches!(ty, Type::BorrowedStorage { storage, life }
        if matches!(storage.as_ref(), Type::Enum { .. }) && !life.slots.is_empty()
        && life.slots.iter().all(|s| s.mode == StoredBorrowMode::SharedAddress))
}

pub(super) fn arguments(
    instance: &LifeNominalInstance,
    limits: &Limits,
    b: &mut Budget,
) -> Check<()> {
    if !candidate(&instance.ty) {
        return Ok(());
    }
    let Type::BorrowedStorage { storage, life } = &instance.ty else {
        unreachable!()
    };
    let Type::Enum { variants, .. } = storage.as_ref() else {
        unreachable!()
    };
    let mut actual_rows = Vec::new();
    for (ordinal, argument) in instance.arguments.iter().enumerate() {
        b.spend(1)?;
        if let LifeNominalActual::Type(ty @ Type::Address { mutable: false, .. }) = argument {
            actual_rows.push((ordinal, ty));
        }
    }
    b.spend(actual_rows.len().saturating_add(life.retained.len()))?;
    if actual_rows.len() != life.retained.len() {
        return Err(err("Enum Address actual class coverage differs"));
    }
    for ((ordinal, actual), row) in actual_rows.iter().zip(&life.retained) {
        b.spend(row.path.len().saturating_add(life.slots.len()))?;
        if row.path.as_slice() != [LifeTypeProjection::TypeArgument(*ordinal as u32)]
            || row.class != 0
            || row.binder >= life.binder_count
        {
            return Err(err(
                "Enum source class is not the actual generic Address boundary",
            ));
        }
        let mut present = false;
        for slot in &life.slots {
            if let [PlaceProjection::Variant(p)] = slot.path.as_slice()
                && let Some(field) = variants
                    .get(p.variant as usize)
                    .and_then(|(_, fields)| fields.get(p.index as usize))
            {
                type_work(field, limits, b)?;
                type_work(actual, limits, b)?;
                if field == *actual && slot.binder == row.binder {
                    present = true;
                }
            }
        }
        if !present {
            return Err(err(
                "Enum Address actual has no complete physical occurrence in its class",
            ));
        }
    }
    Ok(())
}

pub(super) fn shape(ty: &Type, limits: &Limits, b: &mut Budget) -> Check<bool> {
    if !candidate(ty) {
        return Ok(false);
    }
    let Type::BorrowedStorage { storage, life } = ty else {
        unreachable!()
    };
    let Type::Enum { name, variants } = storage.as_ref() else {
        unreachable!()
    };
    b.spend(name.len().saturating_add(variants.len()).saturating_add(1))?;
    if name.is_empty() || variants.is_empty() || life.binder_count == 0 {
        return Err(err("borrowed Enum identity or class domain is empty"));
    }
    let mut names: Vec<&str> = Vec::new();
    let mut leaves = Vec::new();
    for (variant, (name, fields)) in variants.iter().enumerate() {
        b.spend(amount(
            name.len().saturating_add(1),
            names.len().saturating_add(1),
        )?)?;
        if name.is_empty() || names.contains(&name.as_str()) {
            return Err(err("borrowed Enum variant names are ambiguous"));
        }
        names.push(name);
        b.spend(fields.len())?;
        for (index, field) in fields.iter().enumerate() {
            if let Type::Address {
                mutable: false,
                inner,
            } = field
            {
                type_work(inner, limits, b)?;
                let pointee = match inner.as_ref() {
                    Type::Slice { element } => element.as_ref(),
                    t => t,
                };
                if !option_address::closed_pointee(pointee, limits, b)? {
                    return Err(err(
                        "borrowed Enum pointee has no complete finite data consumer",
                    ));
                }
                leaves.push(PlaceProjection::Variant(zil_ir::VariantProjection {
                    variant: u32::try_from(variant)
                        .map_err(|_| err("Enum tag ordinal overflow"))?,
                    index: u32::try_from(index).map_err(|_| err("Enum field ordinal overflow"))?,
                }));
            } else if !option_address::closed_pointee(field, limits, b)? {
                return Err(err(
                    "borrowed Enum payload is not a shared leaf or finite Copy data",
                ));
            }
        }
    }
    b.spend(leaves.len().saturating_add(life.slots.len()))?;
    if leaves.len() != life.slots.len() {
        return Err(err(
            "borrowed Enum slots do not cover its actual shared payloads",
        ));
    }
    for (path, slot) in leaves.iter().zip(&life.slots) {
        b.spend(slot.path.len().saturating_add(1))?;
        if slot.path.as_slice() != [*path]
            || slot.mode != StoredBorrowMode::SharedAddress
            || slot.binder >= life.binder_count
        {
            return Err(err(
                "borrowed Enum slot path, mode or class differs from the physical payload",
            ));
        }
    }
    let mut seen = Vec::new();
    for class in life
        .nominal_arguments
        .iter()
        .map(|r| r.binder)
        .chain(life.retained.iter().map(|r| r.binder))
        .chain(life.slots.iter().map(|r| r.binder))
    {
        b.spend(seen.len().saturating_add(1))?;
        if class >= life.binder_count {
            return Err(err("borrowed Enum class exceeds domain"));
        }
        if !seen.contains(&class) {
            if class as usize != seen.len() {
                return Err(err("borrowed Enum classes are not canonical"));
            }
            seen.push(class);
        }
    }
    if seen.len() != life.binder_count as usize {
        return Err(err("borrowed Enum class domain has an unused tail"));
    }
    for row in &life.retained {
        b.spend(row.path.len().saturating_add(1))?;
        if !matches!(row.path.as_slice(), [LifeTypeProjection::TypeArgument(_)])
            || row.class != 0
            || row.binder >= life.binder_count
        {
            return Err(err(
                "borrowed Enum Type actual map has no valid class boundary",
            ));
        }
    }
    Ok(true)
}

pub(crate) fn ordinary(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
    if b.task_borrowed_results && super::task_address::shape(ty) {
        return Ok(false);
    }
    if !b.stored_enum_addresses {
        return Ok(false);
    }
    if let Type::ErrorUnion { error, result } = ty {
        b.spend(1)?;
        let error_sum = ordinary(module, error, b)?;
        let result_sum = ordinary(module, result, b)?;
        let limits = b.limits.clone();
        if (!error_sum && !option_address::closed_pointee(error, &limits, b)?)
            || (!result_sum && !option_address::closed_pointee(result, &limits, b)?)
        {
            return Err(err(
                "borrowed error wrapper contains an unqualified payload",
            ));
        }
        return Ok(error_sum || result_sum);
    }
    if !candidate(ty) {
        return Ok(false);
    }
    let limits = b.limits.clone();
    b.spend(module.life_instances.len())?;
    let mut selected = None;
    for instance in &module.life_instances {
        type_work(&instance.ty, &limits, b)?;
        type_work(ty, &limits, b)?;
        if instance.ty == *ty && selected.replace(instance).is_some() {
            return Err(err(
                "borrowed Enum operation has duplicate complete instances",
            ));
        }
    }
    let instance = selected
        .ok_or_else(|| err("borrowed Enum operation lacks its complete nominal instance"))?;
    b.spend(module.life_declarations.len())?;
    let declaration = module
        .life_declarations
        .iter()
        .find(|d| d.id == instance.declaration)
        .ok_or_else(|| err("borrowed Enum declaration is absent"))?;
    if declaration.id.compiler {
        if !poll_declaration(declaration) {
            return Ok(false);
        }
        poll_instance(instance, b)?;
        return Ok(true);
    }
    if declaration.kind != LifeNominalKind::Enum {
        return Err(err("borrowed Enum declaration kind differs"));
    }
    shape(ty, &limits, b)
}

pub(super) fn instruction(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    if !b.stored_enum_addresses
        || !matches!(
            i.kind,
            InstructionKind::ReadErrorUnion { .. } | InstructionKind::ErrorTag { .. }
        )
    {
        return Ok(false);
    }
    Ok(error_types(module, f, i, b)?.is_some())
}

fn error_types<'a>(
    module: &TypedModule,
    f: &'a TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<Option<&'a Type>> {
    let union = match i.kind {
        InstructionKind::MakeErrorUnion { .. } => vt(
            f,
            i.result
                .ok_or_else(|| err("error constructor lacks result"))?,
        )?,
        InstructionKind::ErrorTag { value } | InstructionKind::ReadErrorUnion { value, .. } => {
            vt(f, value)?
        }
        _ => return Ok(None),
    };
    if !matches!(union, Type::ErrorUnion { .. }) {
        return Ok(None);
    }
    if !ordinary(module, union, b)? && !option_address::closed_pointee(union, &b.limits.clone(), b)?
    {
        return Ok(None);
    }
    let Type::ErrorUnion { error, result } = union else {
        unreachable!()
    };
    let output = vt(
        f,
        i.result
            .ok_or_else(|| err("borrowed error operation lacks result"))?,
    )?;
    let limits = b.limits.clone();
    type_work(output, &limits, b)?;
    match i.kind {
        InstructionKind::MakeErrorUnion { is_error, value } => {
            let actual = vt(f, value)?;
            let expected = if is_error {
                error.as_ref()
            } else {
                result.as_ref()
            };
            type_work(actual, &limits, b)?;
            type_work(expected, &limits, b)?;
            if actual != expected {
                return Err(err(
                    "borrowed error input changes its complete selected arm",
                ));
            }
        }
        InstructionKind::ReadErrorUnion { is_error, .. } => {
            let expected = if is_error {
                error.as_ref()
            } else {
                result.as_ref()
            };
            type_work(expected, &limits, b)?;
            if output != expected {
                return Err(err(
                    "borrowed error projected result differs from selected arm",
                ));
            }
        }
        InstructionKind::ErrorTag { .. } => {
            if output != &Type::Bool {
                return Err(err("borrowed error tag result is not Bool"));
            }
        }
        _ => unreachable!(),
    }
    Ok(Some(union))
}

fn error_transfer(
    _f: &TypedFunction,
    i: &Instruction,
    _ty: &Type,
    values: &[Payload],
    b: &mut Budget,
) -> Check<Payload> {
    let mut out = Payload::default();
    match i.kind {
        InstructionKind::MakeErrorUnion { value, is_error } => {
            let actual = values
                .get(value.0 as usize)
                .ok_or_else(|| err("borrowed error construction lacks its actual input"))?;
            out.enum_variants = Variants {
                domain: 2,
                constructed: vec![u32::from(is_error)],
                ..Variants::default()
            };
            let prefix = PlaceProjection::ErrorArm(is_error);
            if actual.enum_variants.present() {
                actual.enum_variants.work(b)?;
                out.enum_variants
                    .nested
                    .push((vec![prefix], actual.enum_variants.clone()));
            }
            for (path, p) in &actual.leaves {
                b.spend(path.len().saturating_add(2))?;
                let mut full = vec![prefix];
                full.extend_from_slice(path);
                add_leaf(&mut out, &full, p, b)?;
            }
        }
        InstructionKind::ReadErrorUnion { value, is_error } => {
            let actual = values
                .get(value.0 as usize)
                .ok_or_else(|| err("borrowed error projection lacks its actual source"))?;
            let prefix = PlaceProjection::ErrorArm(is_error);
            b.spend(actual.enum_variants.nested.len())?;
            if let Some((_, tags)) = actual
                .enum_variants
                .nested
                .iter()
                .find(|(path, _)| path.as_slice() == [prefix])
            {
                tags.work(b)?;
                out.enum_variants = tags.clone();
            }
            for (path, p) in &actual.leaves {
                b.spend(path.len().saturating_add(1))?;
                if path.first() == Some(&prefix) {
                    add_leaf(&mut out, &path[1..], p, b)?;
                }
            }
        }
        InstructionKind::ErrorTag { .. } => {}
        _ => unreachable!(),
    }
    Ok(out)
}

pub(super) fn slots(ty: &Type, b: &mut Budget) -> Check<Vec<(Path, StoredBorrowMode)>> {
    let mut pending = vec![(ty, Vec::new())];
    let mut out = Vec::new();
    while let Some((ty, prefix)) = pending.pop() {
        b.spend(prefix.len().saturating_add(1))?;
        match ty {
            Type::BorrowedStorage { life, .. } => {
                for slot in &life.slots {
                    b.spend(
                        prefix
                            .len()
                            .saturating_add(slot.path.len())
                            .saturating_add(1),
                    )?;
                    let mut path = prefix.clone();
                    path.extend_from_slice(&slot.path);
                    out.push((path, slot.mode));
                }
            }
            Type::ErrorUnion { error, result } => {
                for (arm, child) in [(true, error.as_ref()), (false, result.as_ref())] {
                    let mut path = prefix.clone();
                    path.push(PlaceProjection::ErrorArm(arm));
                    pending.push((child, path));
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

pub(super) fn types<'a>(
    module: &TypedModule,
    f: &'a TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<Option<&'a Type>> {
    if !b.stored_enum_addresses {
        return Ok(None);
    }
    if let Some(ty) = error_types(module, f, i, b)? {
        return Ok(Some(ty));
    }
    let ty = match &i.kind {
        InstructionKind::MakeVariant { .. } => vt(
            f,
            i.result
                .ok_or_else(|| err("Enum construction lacks a result"))?,
        )?,
        InstructionKind::ReadVariant { value, .. } | InstructionKind::VariantTag { value } => {
            vt(f, *value)?
        }
        _ => return Ok(None),
    };
    if !ordinary(module, ty, b)? {
        return Ok(None);
    }
    let Type::BorrowedStorage { storage, .. } = ty else {
        unreachable!()
    };
    let Type::Enum { variants, .. } = storage.as_ref() else {
        unreachable!()
    };
    let result = vt(
        f,
        i.result.ok_or_else(|| err("Enum operation lacks result"))?,
    )?;
    let limits = b.limits.clone();
    type_work(result, &limits, b)?;
    match &i.kind {
        InstructionKind::MakeVariant { variant, fields } => {
            let (_, expected) = variants
                .get(*variant as usize)
                .ok_or_else(|| err("Enum constructor tag is absent"))?;
            b.spend(fields.len())?;
            if fields.len() != expected.len() {
                return Err(err("Enum constructor payload arity differs"));
            }
            for (id, expected) in fields.iter().zip(expected) {
                type_work(vt(f, *id)?, &limits, b)?;
                type_work(expected, &limits, b)?;
                if vt(f, *id)? != expected {
                    return Err(err("Enum constructor actual payload type differs"));
                }
            }
        }
        InstructionKind::ReadVariant { variant, index, .. } => {
            let expected = variants
                .get(*variant as usize)
                .and_then(|(_, fields)| fields.get(*index as usize))
                .ok_or_else(|| err("Enum projection path is absent"))?;
            type_work(expected, &limits, b)?;
            if expected != result {
                return Err(err("Enum projection result differs from its actual field"));
            }
        }
        InstructionKind::VariantTag { .. } if result == &Type::I32 => {}
        _ => return Err(err("Enum operation has an invalid result")),
    }
    Ok(Some(ty))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Variants {
    domain: u32,
    constructed: Vec<u32>,
    inputs: Vec<(u32, Path)>,
    nested: Vec<(Path, Variants)>,
}
impl Variants {
    pub(super) fn runtime_poll(b: &mut Budget) -> Check<Self> {
        b.spend(7)?;
        Ok(Self {
            domain: 2,
            constructed: vec![0, 1],
            nested: vec![(
                vec![PlaceProjection::ErrorArm(false)],
                Self {
                    domain: 2,
                    constructed: vec![0, 1],
                    ..Self::default()
                },
            )],
            ..Self::default()
        })
    }
    pub(super) fn present(&self) -> bool {
        self.domain != 0
    }
    pub(super) fn grounded(&self, ty: &Type, b: &mut Budget) -> Check<bool> {
        b.spend(1)?;
        if !self.present() || (self.constructed.is_empty() && self.inputs.is_empty()) {
            return Ok(false);
        }
        match ty {
            Type::ErrorUnion { error, result } => {
                if self.domain != 2 {
                    return Ok(false);
                }
                for (arm, child) in [(false, result.as_ref()), (true, error.as_ref())] {
                    if self.active(u32::from(arm)) && signature(child) {
                        let Some(actual) = self
                            .nested
                            .iter()
                            .find(|(p, _)| p.as_slice() == [PlaceProjection::ErrorArm(arm)])
                            .map(|(_, v)| v)
                        else {
                            return Ok(false);
                        };
                        if !actual.grounded(child, b)? {
                            return Ok(false);
                        }
                    }
                }
                Ok(true)
            }
            Type::BorrowedStorage { storage, .. } => Ok(
                matches!(storage.as_ref(),Type::Enum { variants,.. } if self.domain as usize==variants.len()),
            ),
            _ => Ok(false),
        }
    }

    fn active(&self, tag: u32) -> bool {
        tag < self.domain
            && (!self.inputs.is_empty() || self.constructed.binary_search(&tag).is_ok())
    }
    pub(super) fn slot_active(&self, path: &[PlaceProjection]) -> bool {
        match path.first() {
            Some(PlaceProjection::Variant(p)) => self.active(p.variant),
            Some(PlaceProjection::ErrorArm(arm)) => {
                self.active(u32::from(*arm))
                    && self
                        .nested
                        .iter()
                        .find(|(p, _)| p.as_slice() == [PlaceProjection::ErrorArm(*arm)])
                        .is_some_and(|(_, child)| child.slot_active(&path[1..]))
            }
            _ => false,
        }
    }
    pub(super) fn work(&self, b: &mut Budget) -> Check<()> {
        if self.present() {
            b.spend(
                self.constructed
                    .len()
                    .saturating_add(self.inputs.len())
                    .saturating_add(1),
            )?;
            for (_, path) in &self.inputs {
                b.spend(path.len())?;
            }
            for (path, child) in &self.nested {
                b.spend(path.len().saturating_add(1))?;
                child.work(b)?;
            }
        }
        Ok(())
    }
    pub(super) fn join(&mut self, other: &Self, b: &mut Budget) -> Check<bool> {
        if !other.present() {
            return Ok(false);
        }
        self.work(b)?;
        other.work(b)?;
        if self.present() && self.domain != other.domain {
            return Err(err("Enum tag domain differs across payload flow"));
        }
        let mut changed = self.domain != other.domain;
        self.domain = other.domain;
        for tag in &other.constructed {
            b.spend(self.constructed.len().saturating_add(1))?;
            if let Err(at) = self.constructed.binary_search(tag) {
                self.constructed.insert(at, *tag);
                changed = true;
            }
        }
        for input in &other.inputs {
            b.spend(amount(
                self.inputs.len().saturating_add(1),
                input.1.len().saturating_add(1),
            )?)?;
            if let Err(at) = self.inputs.binary_search(input) {
                self.inputs.insert(at, input.clone());
                changed = true;
            }
        }
        for (path, child) in &other.nested {
            b.spend(amount(
                self.nested.len().saturating_add(1),
                path.len().saturating_add(1),
            )?)?;
            if let Some((_, actual)) = self.nested.iter_mut().find(|(p, _)| p == path) {
                changed |= actual.join(child, b)?;
            } else {
                child.work(b)?;
                self.nested.push((path.clone(), child.clone()));
                changed = true;
            }
        }
        Ok(changed)
    }
    pub(super) fn parameter(index: u32, ty: &Type, b: &mut Budget) -> Check<Self> {
        Self::input(index, ty, Vec::new(), b)
    }
    fn input(index: u32, ty: &Type, selector: Path, b: &mut Budget) -> Check<Self> {
        b.spend(selector.len().saturating_add(2))?;
        if let Type::ErrorUnion { error, result } = ty {
            let mut nested = Vec::new();
            for (arm, child) in [(false, result.as_ref()), (true, error.as_ref())] {
                if signature(child) {
                    let mut position = selector.clone();
                    position.push(PlaceProjection::ErrorArm(arm));
                    nested.push((
                        vec![PlaceProjection::ErrorArm(arm)],
                        Self::input(index, child, position, b)?,
                    ));
                }
            }
            return Ok(Self {
                domain: 2,
                inputs: vec![(index, selector)],
                nested,
                ..Self::default()
            });
        }
        let Type::BorrowedStorage { storage, .. } = ty else {
            return Err(err("Enum input has no qualified boundary"));
        };
        let Type::Enum { variants, .. } = storage.as_ref() else {
            unreachable!()
        };
        Ok(Self {
            domain: variants.len() as u32,
            inputs: vec![(index, selector)],
            ..Self::default()
        })
    }
    pub(super) fn substitute(
        &self,
        arguments: &[ValueId],
        values: &[Payload],
        b: &mut Budget,
    ) -> Check<Self> {
        if !self.present() {
            return Ok(Self::default());
        }
        self.work(b)?;
        let mut out = Self {
            domain: self.domain,
            constructed: self.constructed.clone(),
            ..Self::default()
        };
        for (input, path) in &self.inputs {
            b.spend(path.len().saturating_add(1))?;
            let payload = arguments
                .get(*input as usize)
                .and_then(|v| values.get(v.0 as usize))
                .ok_or_else(|| err("Enum result dependency has no actual argument"))?;
            let empty = Self::default();
            let mut selected = &payload.enum_variants;
            for step in path {
                b.spend(selected.nested.len())?;
                let Some((_, child)) = selected
                    .nested
                    .iter()
                    .find(|(p, _)| p.as_slice() == [*step])
                else {
                    selected = &empty;
                    break;
                };
                selected = child;
            }
            let root = Self {
                domain: selected.domain,
                constructed: selected.constructed.clone(),
                inputs: selected.inputs.clone(),
                ..Self::default()
            };
            out.join(&root, b)?;
        }
        for (path, child) in &self.nested {
            if matches!(path.as_slice(), [PlaceProjection::ErrorArm(arm)] if !out.active(u32::from(*arm)))
            {
                continue;
            }
            out.nested
                .push((path.clone(), child.substitute(arguments, values, b)?));
        }
        Ok(out)
    }
}

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
    if matches!(ty, Type::ErrorUnion { .. }) {
        return error_transfer(f, i, ty, values, b).map(Some);
    }
    let Type::BorrowedStorage { storage, life } = ty else {
        unreachable!()
    };
    let Type::Enum { variants, .. } = storage.as_ref() else {
        unreachable!()
    };
    let mut out = Payload::default();
    match &i.kind {
        InstructionKind::MakeVariant { variant, fields } => {
            b.spend(2)?;
            out.enum_variants = Variants {
                domain: variants.len() as u32,
                constructed: vec![*variant],
                ..Variants::default()
            };
            for slot in &life.slots {
                b.spend(slot.path.len().saturating_add(1))?;
                let [PlaceProjection::Variant(p)] = slot.path.as_slice() else {
                    unreachable!()
                };
                if p.variant == *variant {
                    let value = values
                        .get(fields[p.index as usize].0 as usize)
                        .ok_or_else(|| err("Enum actual payload is absent"))?;
                    add_leaf(&mut out, &slot.path, &value.address, b)?;
                }
            }
        }
        InstructionKind::ReadVariant {
            value,
            variant,
            index,
        } => {
            let input = values
                .get(value.0 as usize)
                .ok_or_else(|| err("Enum projected operand is absent"))?;
            let path = [PlaceProjection::Variant(zil_ir::VariantProjection {
                variant: *variant,
                index: *index,
            })];
            if let Some(permission) = leaf(input, &path, b)? {
                join_permission(&mut out.address, permission, b)?;
            }
        }
        InstructionKind::VariantTag { .. } => {}
        _ => unreachable!(),
    }
    Ok(Some(out))
}

pub(super) fn projections(
    f: &TypedFunction,
    graph: &FunctionGraph,
    summary: &Summary,
    b: &mut Budget,
) -> Check<()> {
    if summary.enum_values.is_empty() {
        return Ok(());
    }
    b.spend(f.values.len().saturating_add(f.blocks.len()))?;
    let mut producers = vec![None; f.values.len()];
    for block in &f.blocks {
        for i in &block.instructions {
            b.spend(1)?;
            if let Some(id) = i.result {
                producers[id.0 as usize] = Some(&i.kind);
            }
        }
    }
    let mut start = Vec::new();
    for (id, tags) in &summary.enum_values {
        b.spend(tags.domain as usize)?;
        let possible = if tags.inputs.is_empty() {
            tags.constructed.clone()
        } else {
            (0..tags.domain).collect()
        };
        start.push((*id, possible));
    }
    let test = |id: ValueId| -> Option<(ValueId, u32)> {
        if let Some(InstructionKind::ErrorTag { value }) = producers[id.0 as usize] {
            return Some((*value, 1));
        }
        let Some(InstructionKind::Binary {
            op: zil_ir::BinaryOp::Equal,
            left,
            right,
        }) = producers[id.0 as usize]
        else {
            return None;
        };
        for (tag, scalar) in [(*left, *right), (*right, *left)] {
            if let (
                Some(InstructionKind::ErrorTag { value }),
                Some(InstructionKind::Constant(Constant::Bool(n))),
            ) = (producers[tag.0 as usize], producers[scalar.0 as usize])
            {
                return Some((*value, u32::from(*n)));
            }
            match (producers[tag.0 as usize], producers[scalar.0 as usize]) {
                (
                    Some(InstructionKind::VariantTag { value }),
                    Some(InstructionKind::Constant(Constant::I32(n))),
                ) if *n >= 0 => return Some((*value, *n as u32)),
                _ => {}
            }
        }
        None
    };
    type TagRefinements = Vec<(ValueId, Vec<u32>)>;
    let mut entries: Vec<Option<TagRefinements>> = vec![None; f.blocks.len()];
    entries[f.entry.0 as usize] = Some(start);
    let mut pending = vec![f.entry.0 as usize];
    while let Some(bi) = pending.pop() {
        b.spend(1)?;
        if !graph.reachable[bi] {
            continue;
        }
        let state = entries[bi]
            .as_ref()
            .ok_or_else(|| err("Enum guard worklist lacks its actual state"))?;
        b.spend(
            state
                .iter()
                .map(|(_, tags)| tags.len().saturating_add(1))
                .sum(),
        )?;
        let state = state.clone();
        let condition = match f.blocks[bi].terminator {
            Terminator::Branch {
                condition,
                then_block,
                ..
            } => test(condition).map(|(id, n)| (id, n, then_block.0 as usize)),
            _ => None,
        };
        for target in &graph.successors[bi] {
            if !graph.reachable[*target] {
                continue;
            }
            b.spend(
                state
                    .iter()
                    .map(|(_, tags)| tags.len().saturating_add(1))
                    .sum(),
            )?;
            let mut next = state.clone();
            if let Some((id, n, then_block)) = condition {
                b.spend(next.len())?;
                if let Some((_, tags)) = next.iter_mut().find(|(actual, _)| *actual == id) {
                    tags.retain(|tag| (*tag == n) == (*target == then_block));
                    if tags.is_empty() {
                        continue;
                    }
                }
            }
            let mut changed = false;
            if let Some(old) = &mut entries[*target] {
                for (id, tags) in next {
                    b.spend(old.len())?;
                    let (_, actual) = old
                        .iter_mut()
                        .find(|(value, _)| *value == id)
                        .ok_or_else(|| err("Enum guard state loses its value domain"))?;
                    for tag in tags {
                        b.spend(actual.len().saturating_add(1))?;
                        if let Err(at) = actual.binary_search(&tag) {
                            actual.insert(at, tag);
                            changed = true;
                        }
                    }
                }
            } else {
                entries[*target] = Some(next);
                changed = true;
            }
            if changed {
                b.spend(1)?;
                pending.push(*target);
            }
        }
    }
    for (bi, block) in f.blocks.iter().enumerate() {
        if !graph.reachable[bi] {
            continue;
        }
        for i in &block.instructions {
            b.spend(1)?;
            let projected = match i.kind {
                InstructionKind::ReadVariant { value, variant, .. } => Some((value, variant)),
                InstructionKind::ReadErrorUnion { value, is_error } => {
                    Some((value, u32::from(is_error)))
                }
                _ => None,
            };
            if let Some((value, variant)) = projected {
                b.spend(summary.enum_values.len())?;
                if !summary.enum_values.iter().any(|(id, _)| *id == value) {
                    continue;
                }
                let active = entries[bi]
                    .as_ref()
                    .and_then(|s| s.iter().find(|(id, _)| *id == value))
                    .map(|(_, tags)| tags.as_slice());
                if active != Some(&[variant][..]) {
                    return Err(err(
                        "borrowed Enum projection is not guarded by its actual active tag",
                    ));
                }
            }
        }
    }
    Ok(())
}

// A single replay uses only the converged original graph, never facts
// produced by the replay itself.
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
    if !b.stored_enum_addresses {
        return Ok(None);
    }
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
            if let Some(InstructionKind::ErrorTag { value }) = producers[condition.0 as usize] {
                b.spend(frozen[fi].enum_values.len())?;
                if let Some((_, tags)) = frozen[fi].enum_values.iter().find(|(id, _)| id == value) {
                    tags.work(b)?;
                    if tags.inputs.is_empty() && tags.constructed.len() == 1 {
                        return Ok(Some(tags.constructed[0] == 1));
                    }
                }
            }
            if vt(f, condition)? != &Type::Bool {
                return Ok(None);
            }
            let Some(InstructionKind::Binary { op, left, right }) = producers[condition.0 as usize]
            else {
                return Ok(None);
            };
            if *op == zil_ir::BinaryOp::Equal {
                for (tag, constant) in [(*left, *right), (*right, *left)] {
                    if let (
                        Some(InstructionKind::ErrorTag { value }),
                        Some(InstructionKind::Constant(Constant::Bool(n))),
                    ) = (producers[tag.0 as usize], producers[constant.0 as usize])
                    {
                        b.spend(frozen[fi].enum_values.len())?;
                        if let Some((_, actual)) =
                            frozen[fi].enum_values.iter().find(|(id, _)| id == value)
                        {
                            actual.work(b)?;
                            if actual.inputs.is_empty() && actual.constructed.len() == 1 {
                                return Ok(Some(actual.constructed[0] == u32::from(*n)));
                            }
                        }
                    }
                }
            }
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
                let Some(InstructionKind::VariantTag { value: actual }) = producers[tag.0 as usize]
                else {
                    continue;
                };
                // Both sums read the original solution; there is only one replay.
                b.spend(frozen[fi].none_values.len().saturating_add(1))?;
                if matches!(*value, 0 | 1) && frozen[fi].none_values.contains(actual) {
                    return Ok(Some(*value == 0));
                }
                b.spend(frozen[fi].enum_values.len().saturating_add(1))?;
                if let Some((_, tags)) = frozen[fi].enum_values.iter().find(|(id, _)| id == actual)
                {
                    tags.work(b)?;
                    if tags.inputs.is_empty() && !tags.constructed.is_empty() {
                        if *value < 0 || tags.constructed.binary_search(&(*value as u32)).is_err() {
                            return Ok(Some(false));
                        }
                        if tags.constructed.len() == 1 {
                            return Ok(Some(true));
                        }
                    }
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

pub(super) fn poll_instance(instance: &LifeNominalInstance, b: &mut Budget) -> Check<()> {
    b.spend(96)?;
    let Type::BorrowedStorage { storage, life } = &instance.ty else {
        return Err(err("Poll omits its qualified type"));
    };
    let [LifeNominalActual::Type(actual)] = instance.arguments.as_slice() else {
        return Err(err("Poll omits its complete Type argument"));
    };
    let Type::Enum { name, variants } = storage.as_ref() else {
        return Err(err("Poll physical type is not Enum"));
    };
    type_work(actual, &b.limits.clone(), b)?;
    b.spend(name.len().saturating_add(96))?;
    if name != &format!("@std.task:v1:Poll:{}", actual.fingerprint())
        || variants.len() != 2
        || variants[0].0 != "Pending"
        || !variants[0].1.is_empty()
        || variants[1].0 != "Ready"
        || variants[1].1.len() != 1
        || variants[1].1[0] != *actual
        || !matches!(actual, Type::Address { mutable: false, .. })
        || life.binder_count != 1
        || !life.nominal_arguments.is_empty()
        || life.retained.len() != 1
        || life.retained[0].path != [LifeTypeProjection::TypeArgument(0)]
        || life.retained[0].class != 0
        || life.retained[0].binder != 0
        || life.slots.len() != 1
        || life.slots[0].path
            != [PlaceProjection::Variant(zil_ir::VariantProjection {
                variant: 1,
                index: 0,
            })]
        || life.slots[0].mode != StoredBorrowMode::SharedAddress
        || life.slots[0].binder != 0
    {
        return Err(err(
            "Poll actual nominal, variants, or shared class mapping differs",
        ));
    }
    if !shape(&instance.ty, &b.limits.clone(), b)? {
        return Err(err("Poll lacks its complete shared payload shape"));
    }
    Ok(())
}
