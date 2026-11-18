//! Ordinary borrowed Enum proof. Nominal metadata supplies no actual target.
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
        if matches!(storage.as_ref(), Type::Enum { .. })
        && !life.slots.is_empty()
        && life.slots.iter().all(|s| s.mode == StoredBorrowMode::SharedAddress))
}

pub(super) fn arguments(instance: &LifeNominalInstance, b: &mut Budget) -> CompileResult<()> {
    if !candidate(&instance.ty) {
        return Ok(());
    }
    let Type::BorrowedStorage { storage, life } = &instance.ty else {
        unreachable!()
    };
    let Type::Enum { variants, .. } = storage.as_ref() else {
        unreachable!()
    };
    let mut next = 0;
    for (ordinal, actual) in instance.arguments.iter().enumerate() {
        fee(b, 1)?;
        let Some(ty) = option_address::shared_actual(actual, b)? else {
            continue;
        };
        let row = life
            .retained
            .get(next)
            .ok_or_else(|| failure("Enum Address actual lacks its source class map"))?;
        fee(b, row.path.len().saturating_add(life.slots.len()))?;
        if row.path.as_slice() != [LifeTypeProjection::TypeArgument(ordinal as u32)]
            || row.class != 0
            || row.binder >= life.binder_count
        {
            return Err(failure(
                "Enum Address actual ordinal or local class differs",
            ));
        }
        let mut occurrence = false;
        for slot in &life.slots {
            let [PlaceProjection::Variant(p)] = slot.path.as_slice() else {
                continue;
            };
            if let Some(field) = variants
                .get(p.variant as usize)
                .and_then(|(_, fields)| fields.get(p.index as usize))
            {
                b.type_work(field, true, "Enum Address actual physical occurrence")?;
                b.type_work(ty, true, "Enum Address actual complete identity")?;
                occurrence |= field == ty && slot.binder == row.binder;
            }
        }
        if !occurrence {
            return Err(failure(
                "Enum Address actual has no physical slot at its mapped class",
            ));
        }
        next += 1;
    }
    if next != life.retained.len() {
        return Err(failure("Enum retained class map has no matching actual"));
    }
    Ok(())
}

pub(super) fn shape(ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    if !candidate(ty) {
        return Ok(false);
    }
    let Type::BorrowedStorage { storage, life } = ty else {
        unreachable!()
    };
    let Type::Enum { name, variants } = storage.as_ref() else {
        unreachable!()
    };
    fee(
        b,
        name.len().saturating_add(variants.len()).saturating_add(1),
    )?;
    if name.is_empty() || variants.is_empty() || life.binder_count == 0 {
        return Err(failure(
            "borrowed Enum has an empty identity or class domain",
        ));
    }
    let mut names = BTreeSet::new();
    let mut paths = Vec::new();
    for (variant, (name, fields)) in variants.iter().enumerate() {
        ordered(b, names.len(), name.len())?;
        if name.is_empty() || !names.insert(name) {
            return Err(failure(
                "borrowed Enum variants have duplicate or empty names",
            ));
        }
        for (index, field) in fields.iter().enumerate() {
            fee(b, 1)?;
            if let Type::Address {
                mutable: false,
                inner,
            } = field
            {
                b.type_work(inner, true, "borrowed Enum actual pointee")?;
                let pointee = match inner.as_ref() {
                    Type::Slice { element } => element.as_ref(),
                    t => t,
                };
                if !option_address::closed_pointee(pointee, b, 0)? {
                    return Err(failure(
                        "borrowed Enum shared pointee is outside finite data",
                    ));
                }
                paths.push(PlaceProjection::Variant(zil_ir::VariantProjection {
                    variant: u32::try_from(variant)
                        .map_err(|_| failure("Enum variant ordinal overflow"))?,
                    index: u32::try_from(index)
                        .map_err(|_| failure("Enum payload ordinal overflow"))?,
                }));
            } else if !option_address::closed_pointee(field, b, 0)? {
                return Err(failure(
                    "borrowed Enum payload is neither a shared address nor finite data",
                ));
            }
        }
    }
    fee(b, paths.len().saturating_add(life.slots.len()))?;
    if paths.len() != life.slots.len() {
        return Err(failure("borrowed Enum physical slot coverage differs"));
    }
    for (path, row) in paths.iter().zip(&life.slots) {
        fee(b, row.path.len().saturating_add(1))?;
        if row.path.as_slice() != [*path]
            || row.mode != StoredBorrowMode::SharedAddress
            || row.binder >= life.binder_count
        {
            return Err(failure(
                "borrowed Enum slot differs from its actual payload",
            ));
        }
    }
    let mut classes = BTreeSet::new();
    for class in life
        .nominal_arguments
        .iter()
        .map(|r| r.binder)
        .chain(life.retained.iter().map(|r| r.binder))
        .chain(life.slots.iter().map(|r| r.binder))
    {
        ordered(b, classes.len(), 0)?;
        if class >= life.binder_count
            || (!classes.contains(&class) && class as usize != classes.len())
        {
            return Err(failure("borrowed Enum class numbering is not canonical"));
        }
        classes.insert(class);
    }
    if classes.len() != life.binder_count as usize {
        return Err(failure("borrowed Enum has unused trailing classes"));
    }
    for map in &life.retained {
        fee(b, map.path.len().saturating_add(1))?;
        if !matches!(map.path.as_slice(), [LifeTypeProjection::TypeArgument(_)])
            || map.class != 0
            || map.binder >= life.binder_count
        {
            return Err(failure("borrowed Enum retained Type actual class differs"));
        }
    }
    Ok(true)
}

pub(crate) fn ordinary(module: &TypedModule, ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    if b.task_borrowed_results && super::task_address::shape(ty).is_some() {
        return Ok(false);
    }
    if !b.stored_enum_addresses {
        return Ok(false);
    }
    if let Type::ErrorUnion { error, result } = ty {
        fee(b, 1)?;
        let a = ordinary(module, error, b)?;
        let z = ordinary(module, result, b)?;
        // An unrelated owning union belongs to its ordinary resource proof.
        // Only an actual borrowed Enum arm selects this wrapper consumer.
        if !a && !z {
            return Ok(false);
        }
        if !a && !option_address::closed_pointee(error, b, 0)? {
            return Err(failure(
                "borrowed error wrapper has an unqualified error payload",
            ));
        }
        if !z && !option_address::closed_pointee(result, b, 0)? {
            return Err(failure(
                "borrowed error wrapper has an unqualified success payload",
            ));
        }
        return Ok(a || z);
    }
    if !candidate(ty) {
        return Ok(false);
    }
    fee(b, module.life_instances.len())?;
    let mut selected = None;
    for instance in &module.life_instances {
        b.type_work(
            &instance.ty,
            true,
            "borrowed Enum complete instance equality",
        )?;
        b.type_work(ty, true, "borrowed Enum operation complete type")?;
        if instance.ty == *ty && selected.replace(instance).is_some() {
            return Err(failure("borrowed Enum instance is ambiguous"));
        }
    }
    let instance = selected.ok_or_else(|| failure("borrowed Enum complete instance is absent"))?;
    fee(b, module.life_declarations.len())?;
    let declaration = module
        .life_declarations
        .iter()
        .find(|d| d.id == instance.declaration)
        .ok_or_else(|| failure("borrowed Enum declaration is absent"))?;
    if declaration.id.compiler {
        if !poll_declaration(declaration) {
            return Ok(false);
        }
        poll_instance(instance, b)?;
        return Ok(true);
    }
    if declaration.kind != LifeNominalKind::Enum {
        return Err(failure("borrowed Enum declaration kind differs"));
    }
    shape(ty, b)
}

pub(super) fn instruction(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    if !b.stored_enum_addresses
        || !matches!(
            i.kind,
            InstructionKind::ErrorTag { .. } | InstructionKind::ReadErrorUnion { .. }
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
) -> CompileResult<Option<&'a Type>> {
    let ty = match i.kind {
        InstructionKind::MakeErrorUnion { .. } => value_type(
            f,
            i.result
                .ok_or_else(|| failure("error constructor has no result"))?,
        )?,
        InstructionKind::ReadErrorUnion { value, .. } | InstructionKind::ErrorTag { value } => {
            value_type(f, value)?
        }
        _ => return Ok(None),
    };
    if !matches!(ty, Type::ErrorUnion { .. }) {
        return Ok(None);
    }
    if !ordinary(module, ty, b)? && !option_address::closed_pointee(ty, b, 0)? {
        return Ok(None);
    }
    let Type::ErrorUnion { error, result } = ty else {
        unreachable!()
    };
    let output = value_type(
        f,
        i.result
            .ok_or_else(|| failure("borrowed error operation has no result"))?,
    )?;
    b.type_work(output, true, "borrowed error operation result")?;
    match i.kind {
        InstructionKind::MakeErrorUnion { value, is_error } => {
            let arm = if is_error {
                error.as_ref()
            } else {
                result.as_ref()
            };
            b.type_work(arm, true, "borrowed error actual arm")?;
            b.type_work(value_type(f, value)?, true, "borrowed error input")?;
            if value_type(f, value)? != arm {
                return Err(failure(
                    "borrowed error constructor changes its complete arm type",
                ));
            }
        }
        InstructionKind::ReadErrorUnion { is_error, .. } => {
            let arm = if is_error {
                error.as_ref()
            } else {
                result.as_ref()
            };
            b.type_work(arm, true, "borrowed error projected arm")?;
            if output != arm {
                return Err(failure(
                    "borrowed error projection differs from its actual arm",
                ));
            }
        }
        InstructionKind::ErrorTag { .. } if output == &Type::Bool => {}
        _ => return Err(failure("borrowed error tag has invalid result")),
    }
    Ok(Some(ty))
}

fn error_transfer(
    _f: &TypedFunction,
    i: &Instruction,
    _ty: &Type,
    values: &[Value],
    b: &mut Budget,
) -> CompileResult<Value> {
    let mut out = Value::default();
    match i.kind {
        InstructionKind::MakeErrorUnion { value, is_error } => {
            let actual = values
                .get(value.0 as usize)
                .ok_or_else(|| failure("borrowed error input is absent"))?;
            let step = PlaceProjection::ErrorArm(is_error);
            out.enum_variants = Variants {
                domain: 2,
                constructed: BTreeSet::from([u32::from(is_error)]),
                ..Variants::default()
            };
            if actual.enum_variants.present() {
                actual.enum_variants.work(b)?;
                out.enum_variants
                    .nested
                    .insert(vec![step], actual.enum_variants.clone());
            }
            for (path, permission) in &actual.slots {
                fee(b, path.len().saturating_add(2))?;
                let mut selected = vec![step];
                selected.extend_from_slice(path);
                let mut copy = Permission::default();
                union(&mut copy, permission, b)?;
                out.slots.insert(selected, copy);
            }
        }
        InstructionKind::ReadErrorUnion { value, is_error } => {
            let actual = values
                .get(value.0 as usize)
                .ok_or_else(|| failure("borrowed error source is absent"))?;
            let step = PlaceProjection::ErrorArm(is_error);
            if let Some(tags) = actual.enum_variants.nested.get(&vec![step]) {
                tags.work(b)?;
                out.enum_variants = tags.clone();
            }
            for (path, permission) in &actual.slots {
                fee(b, path.len().saturating_add(1))?;
                if path.first() == Some(&step) {
                    let mut copy = Permission::default();
                    union(&mut copy, permission, b)?;
                    out.slots.insert(path[1..].to_vec(), copy);
                }
            }
        }
        InstructionKind::ErrorTag { .. } => {}
        _ => unreachable!(),
    }
    Ok(out)
}

pub(super) fn slots(ty: &Type, b: &mut Budget) -> CompileResult<Vec<(Path, StoredBorrowMode)>> {
    fn walk(
        ty: &Type,
        path: &mut Path,
        out: &mut Vec<(Path, StoredBorrowMode)>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        fee(b, 1)?;
        match ty {
            Type::BorrowedStorage { life, .. } => {
                for slot in &life.slots {
                    fee(
                        b,
                        path.len().saturating_add(slot.path.len()).saturating_add(1),
                    )?;
                    let mut full = path.clone();
                    full.extend_from_slice(&slot.path);
                    out.push((full, slot.mode));
                }
            }
            Type::ErrorUnion { error, result } => {
                for (arm, child) in [(false, result.as_ref()), (true, error.as_ref())] {
                    path.push(PlaceProjection::ErrorArm(arm));
                    walk(child, path, out, b)?;
                    path.pop();
                }
            }
            _ => {}
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(ty, &mut Vec::new(), &mut out, b)?;
    Ok(out)
}

pub(super) fn types<'a>(
    module: &TypedModule,
    f: &'a TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<Option<&'a Type>> {
    if !b.stored_enum_addresses {
        return Ok(None);
    }
    if let Some(ty) = error_types(module, f, i, b)? {
        return Ok(Some(ty));
    }
    let ty = match &i.kind {
        InstructionKind::MakeVariant { .. } => value_type(
            f,
            i.result
                .ok_or_else(|| failure("Enum constructor lacks result"))?,
        )?,
        InstructionKind::ReadVariant { value, .. } | InstructionKind::VariantTag { value } => {
            value_type(f, *value)?
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
    let result = value_type(
        f,
        i.result
            .ok_or_else(|| failure("Enum operation lacks result"))?,
    )?;
    match &i.kind {
        InstructionKind::MakeVariant { variant, fields } => {
            let (_, expected) = variants
                .get(*variant as usize)
                .ok_or_else(|| failure("Enum constructor tag is absent"))?;
            if fields.len() != expected.len() {
                return Err(failure("Enum constructor payload arity differs"));
            }
            fee(b, fields.len())?;
            for (value, ty) in fields.iter().zip(expected) {
                b.type_work(ty, true, "Enum actual constructor field")?;
                b.type_work(
                    value_type(f, *value)?,
                    true,
                    "Enum supplied constructor field",
                )?;
                if value_type(f, *value)? != ty {
                    return Err(failure("Enum constructor complete payload type differs"));
                }
            }
        }
        InstructionKind::ReadVariant { variant, index, .. } => {
            let expected = variants
                .get(*variant as usize)
                .and_then(|(_, fields)| fields.get(*index as usize))
                .ok_or_else(|| failure("Enum selected payload is absent"))?;
            b.type_work(expected, true, "Enum read complete payload")?;
            b.type_work(result, true, "Enum read actual result")?;
            if result != expected {
                return Err(failure("Enum read complete payload type differs"));
            }
        }
        InstructionKind::VariantTag { .. } if result != &Type::I32 => {
            return Err(failure("Enum tag result is not i32"));
        }
        _ => {}
    }
    Ok(Some(ty))
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct VariantInput {
    base: HoldRoot,
    pointer: Option<Path>,
    path: Path,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Variants {
    domain: u32,
    constructed: BTreeSet<u32>,
    inputs: BTreeSet<VariantInput>,
    nested: BTreeMap<Path, Variants>,
}
impl Variants {
    pub(super) fn constructed(domain: usize, tag: u32, b: &mut Budget) -> CompileResult<Self> {
        fee(b, 2)?;
        let domain = u32::try_from(domain).map_err(|_| failure("sum tag domain overflows"))?;
        if tag >= domain {
            return Err(failure(
                "sum constructed tag is outside its complete domain",
            ));
        }
        Ok(Self {
            domain,
            constructed: BTreeSet::from([tag]),
            ..Self::default()
        })
    }
    pub(super) fn value_input(base: HoldRoot, ty: &Type, b: &mut Budget) -> CompileResult<Self> {
        Self::input(base, None, ty, Vec::new(), b, 0)
    }
    pub(super) fn runtime_poll(b: &mut Budget) -> CompileResult<Self> {
        fee(b, 7)?;
        Ok(Self {
            domain: 2,
            constructed: BTreeSet::from([0, 1]),
            nested: BTreeMap::from([(
                vec![PlaceProjection::ErrorArm(false)],
                Self {
                    domain: 2,
                    constructed: BTreeSet::from([0, 1]),
                    ..Self::default()
                },
            )]),
            ..Self::default()
        })
    }
    pub(super) fn present(&self) -> bool {
        self.domain != 0 || self.nested.values().any(Self::present)
    }
    pub(super) fn grounded(&self, ty: &Type, b: &mut Budget) -> CompileResult<bool> {
        fee(b, 1)?;
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
                        let Some(actual) = self.nested.get(&vec![PlaceProjection::ErrorArm(arm)])
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

    pub(super) fn active(&self, tag: u32) -> bool {
        tag < self.domain && (!self.inputs.is_empty() || self.constructed.contains(&tag))
    }
    pub(super) fn possible(&self) -> BTreeSet<u32> {
        if self.inputs.is_empty() {
            self.constructed.clone()
        } else {
            (0..self.domain).collect()
        }
    }
    pub(super) fn work(&self, b: &mut Budget) -> CompileResult<()> {
        if self.present() {
            fee(
                b,
                self.constructed
                    .len()
                    .saturating_add(self.inputs.len())
                    .saturating_add(1),
            )?;
            for input in &self.inputs {
                fee(
                    b,
                    input
                        .path
                        .len()
                        .saturating_add(input.pointer.as_ref().map_or(0, Vec::len)),
                )?;
            }
            for (path, child) in &self.nested {
                fee(b, path.len().saturating_add(1))?;
                child.work(b)?;
            }
        }
        Ok(())
    }
    pub(super) fn parameter(index: u32, ty: &Type, b: &mut Budget) -> CompileResult<Self> {
        Self::input(HoldRoot::Parameter(index), None, ty, Vec::new(), b, 0)
    }
    pub(super) fn pointee(
        base: HoldRoot,
        pointer: Path,
        ty: &Type,
        b: &mut Budget,
    ) -> CompileResult<Self> {
        Self::input(base, Some(pointer), ty, Vec::new(), b, 0)
    }
    fn has_sums(ty: &Type, b: &mut Budget, depth: usize) -> CompileResult<bool> {
        fee(b, 1)?;
        if depth > b.partial_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Analyze,
                "sum-state inventory exceeds finite depth",
                None,
            )]);
        }
        Ok(match ty {
            Type::Enum { .. } | Type::ErrorUnion { .. } => true,
            Type::BorrowedStorage { storage, .. }
            | Type::Array {
                element: storage, ..
            } => Self::has_sums(storage, b, depth + 1)?,
            Type::Tuple(fields) => {
                let mut found = false;
                for field in fields {
                    found |= Self::has_sums(field, b, depth + 1)?;
                }
                found
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                let mut found = false;
                for (_, field) in fields {
                    found |= Self::has_sums(field, b, depth + 1)?;
                }
                found
            }
            _ => false,
        })
    }
    fn input(
        base: HoldRoot,
        pointer: Option<Path>,
        ty: &Type,
        selector: Path,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Self> {
        if !Self::has_sums(ty, b, depth)? {
            return Ok(Self::default());
        }
        fee(
            b,
            selector
                .len()
                .saturating_add(pointer.as_ref().map_or(0, Vec::len))
                .saturating_add(2),
        )?;
        if let Type::BorrowedStorage { storage, .. } = ty {
            return Self::input(base, pointer, storage, selector, b, depth + 1);
        }
        let mut out = Self::default();
        let mut add = |step: PlaceProjection, child: &Type| -> CompileResult<()> {
            if !Self::has_sums(child, b, depth + 1)? {
                return Ok(());
            }
            fee(
                b,
                selector
                    .len()
                    .saturating_add(pointer.as_ref().map_or(0, Vec::len))
                    .saturating_add(2),
            )?;
            let mut path = selector.clone();
            path.push(step);
            out.nested.insert(
                vec![step],
                Self::input(base, pointer.clone(), child, path, b, depth + 1)?,
            );
            Ok(())
        };
        match ty {
            Type::Enum { variants, .. } => {
                for (variant, (_, fields)) in variants.iter().enumerate() {
                    for (index, child) in fields.iter().enumerate() {
                        add(
                            PlaceProjection::Variant(zil_ir::VariantProjection {
                                variant: variant as u32,
                                index: index as u32,
                            }),
                            child,
                        )?;
                    }
                }
                out.domain = u32::try_from(variants.len())
                    .map_err(|_| failure("sum variant count overflows"))?;
            }
            Type::ErrorUnion { error, result } => {
                add(PlaceProjection::ErrorArm(true), error)?;
                add(PlaceProjection::ErrorArm(false), result)?;
                out.domain = 2;
            }
            Type::Tuple(fields) => {
                for (index, child) in fields.iter().enumerate() {
                    add(PlaceProjection::Field(index as u32), child)?;
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                for (index, (_, child)) in fields.iter().enumerate() {
                    add(PlaceProjection::Field(index as u32), child)?;
                }
            }
            Type::Array { element, length } => {
                for index in 0..*length {
                    add(PlaceProjection::Field(index), element)?;
                }
            }
            _ => {}
        }
        if out.domain != 0 {
            out.inputs.insert(VariantInput {
                base,
                pointer,
                path: selector,
            });
        }
        Ok(out)
    }
    pub(super) fn project(&self, path: &[PlaceProjection], b: &mut Budget) -> CompileResult<Self> {
        let mut current = self;
        for step in path {
            fee(b, 1)?;
            let Some(child) = current.nested.get(&vec![*step]) else {
                return Ok(Self::default());
            };
            current = child;
        }
        current.work(b)?;
        Ok(current.clone())
    }
    pub(super) fn replace(
        &mut self,
        path: &[PlaceProjection],
        incoming: &Self,
        strong: bool,
        b: &mut Budget,
    ) -> CompileResult<()> {
        fee(b, path.len().saturating_add(1))?;
        if let Some((first, tail)) = path.split_first() {
            self.nested
                .entry(vec![*first])
                .or_default()
                .replace(tail, incoming, strong, b)?;
            self.nested.retain(|_, child| child.present());
        } else if strong {
            incoming.work(b)?;
            *self = incoming.clone();
        } else {
            self.join(incoming, b)?;
        }
        Ok(())
    }
    pub(super) fn complete_tree(&self, ty: &Type, b: &mut Budget) -> CompileResult<bool> {
        if !Self::has_sums(ty, b, 0)? {
            return Ok(true);
        }
        if let Type::BorrowedStorage { storage, .. } = ty {
            return self.complete_tree(storage, b);
        }
        let mut children = Vec::new();
        match ty {
            Type::Enum { variants, .. } => {
                if self.domain as usize != variants.len()
                    || (self.constructed.is_empty() && self.inputs.is_empty())
                {
                    return Ok(false);
                }
                for (variant, (_, fields)) in variants.iter().enumerate() {
                    if self.active(variant as u32) {
                        for (index, child) in fields.iter().enumerate() {
                            children.push((
                                PlaceProjection::Variant(zil_ir::VariantProjection {
                                    variant: variant as u32,
                                    index: index as u32,
                                }),
                                child,
                            ));
                        }
                    }
                }
            }
            Type::ErrorUnion { error, result } => {
                if self.domain != 2 || (self.constructed.is_empty() && self.inputs.is_empty()) {
                    return Ok(false);
                }
                for (arm, child) in [(true, error.as_ref()), (false, result.as_ref())] {
                    if self.active(u32::from(arm)) {
                        children.push((PlaceProjection::ErrorArm(arm), child));
                    }
                }
            }
            Type::Tuple(fields) => {
                for (index, child) in fields.iter().enumerate() {
                    children.push((PlaceProjection::Field(index as u32), child));
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                for (index, (_, child)) in fields.iter().enumerate() {
                    children.push((PlaceProjection::Field(index as u32), child));
                }
            }
            Type::Array { element, length } => {
                fee(b, *length as usize)?;
                for index in 0..*length {
                    children.push((PlaceProjection::Field(index), element.as_ref()));
                }
            }
            _ => {}
        }
        for (step, ty) in children {
            fee(b, 1)?;
            if !Self::has_sums(ty, b, 0)? {
                continue;
            }
            let Some(child) = self.nested.get(&vec![step]) else {
                return Ok(false);
            };
            if !child.complete_tree(ty, b)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub(super) fn join(&mut self, other: &Self, b: &mut Budget) -> CompileResult<bool> {
        if !other.present() {
            return Ok(false);
        }
        other.work(b)?;
        self.work(b)?;
        if self.present() && self.domain != other.domain {
            return Err(failure("Enum tag domains differ across value flow"));
        }
        let mut changed = self.domain != other.domain;
        self.domain = other.domain;
        for tag in &other.constructed {
            ordered(b, self.constructed.len(), 0)?;
            changed |= self.constructed.insert(*tag);
        }
        for input in &other.inputs {
            ordered(
                b,
                self.inputs.len(),
                input
                    .path
                    .len()
                    .saturating_add(input.pointer.as_ref().map_or(0, Vec::len)),
            )?;
            changed |= self.inputs.insert(input.clone());
        }
        for (path, child) in &other.nested {
            ordered(b, self.nested.len(), path.len())?;
            changed |= self
                .nested
                .entry(path.clone())
                .or_default()
                .join(child, b)?;
        }
        Ok(changed)
    }
    pub(super) fn substitute(
        &self,
        arguments: &[ValueId],
        values: &[Value],
        state: &State,
        captures: Option<&Value>,
        b: &mut Budget,
    ) -> CompileResult<Self> {
        self.substitute_inner(arguments, values, state, captures, false, b)
    }
    /// Bottom is permitted while summaries converge; the final call window
    /// proves every input occurrence at the exact pre-call state before join.
    pub(super) fn check_inputs(
        &self,
        arguments: &[ValueId],
        values: &[Value],
        state: &State,
        captures: Option<&Value>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        self.substitute_inner(arguments, values, state, captures, true, b)?;
        Ok(())
    }
    fn substitute_inner(
        &self,
        arguments: &[ValueId],
        values: &[Value],
        state: &State,
        captures: Option<&Value>,
        check: bool,
        b: &mut Budget,
    ) -> CompileResult<Self> {
        if !self.present() {
            return Ok(Self::default());
        }
        self.work(b)?;
        let mut out = Self {
            domain: self.domain,
            constructed: self.constructed.clone(),
            ..Self::default()
        };
        for input in &self.inputs {
            let (direct, head) = match input.base {
                HoldRoot::Parameter(index) => {
                    let value = arguments
                        .get(index as usize)
                        .and_then(|id| values.get(id.0 as usize))
                        .ok_or_else(|| failure("sum summary has no actual argument"))?;
                    if let Some(pointer) = &input.pointer {
                        let head = if pointer.is_empty() {
                            copy_permission(&value.pointer, b)?
                        } else {
                            let rows = environment_memory::current(value, state, b)?;
                            rows.get(pointer)
                                .map(|p| copy_permission(p, b))
                                .transpose()?
                                .unwrap_or_default()
                        };
                        (None, Some(head))
                    } else {
                        (Some(snapshot(value, b)?), None)
                    }
                }
                HoldRoot::Capture(index) => {
                    let receiver = captures
                        .ok_or_else(|| failure("sum effect loses its actual captured receiver"))?;
                    let mut path = vec![PlaceProjection::Field(index)];
                    if let Some(pointer) = &input.pointer {
                        fee(b, pointer.len().saturating_add(1))?;
                        path.extend(pointer);
                        let head = receiver
                            .environment
                            .get(&path)
                            .map(|p| copy_permission(p, b))
                            .transpose()?
                            .unwrap_or_default();
                        (None, Some(head))
                    } else {
                        (
                            Some(pointee_memory::project(receiver, &path, None, b)?),
                            None,
                        )
                    }
                }
                _ => return Err(failure("sum effect retains an activation-local source")),
            };
            let mut candidates = Vec::new();
            if let Some(value) = direct {
                candidates.push(value);
            }
            if let Some(head) = head {
                if check && head.targets.is_empty() {
                    return Err(failure("sum input has no actual pointer candidate"));
                }
                for target in &head.targets {
                    fee(b, target_width(target).saturating_add(1))?;
                    let address = Permission {
                        targets: Targets::from([target.clone()]),
                        loans: Loans::new(),
                    };
                    candidates.push(pointee_memory::read(state, &address, None, check, b)?);
                }
            }
            for actual in candidates {
                let selected = actual.enum_variants.project(&input.path, b)?;
                if check
                    && (selected.domain != self.domain
                        || (selected.constructed.is_empty() && selected.inputs.is_empty()))
                {
                    return Err(failure(
                        "sum input candidate omits its exact active domain or occurrence source",
                    ));
                }
                let root = Self {
                    domain: selected.domain,
                    constructed: selected.constructed,
                    inputs: selected.inputs,
                    ..Self::default()
                };
                out.join(&root, b)?;
            }
        }
        for (path, child) in &self.nested {
            if !out.slot_active(path) {
                continue;
            }
            fee(b, path.len().saturating_add(1))?;
            out.nested.insert(
                path.clone(),
                child.substitute_inner(arguments, values, state, captures, check, b)?,
            );
        }
        Ok(out)
    }
    pub(super) fn slot_active(&self, path: &[PlaceProjection]) -> bool {
        let Some((step, tail)) = path.split_first() else {
            return true;
        };
        let active = match step {
            PlaceProjection::Variant(p) => self.active(p.variant),
            PlaceProjection::ErrorArm(arm) => self.active(u32::from(*arm)),
            _ => true,
        };
        active
            && self
                .nested
                .get(&vec![*step])
                .is_none_or(|child| child.slot_active(tail))
    }
}

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
    if matches!(ty, Type::ErrorUnion { .. }) {
        return error_transfer(f, i, ty, values, b).map(Some);
    }
    let Type::BorrowedStorage { storage, life } = ty else {
        unreachable!()
    };
    let Type::Enum { variants, .. } = storage.as_ref() else {
        unreachable!()
    };
    let mut out = Value::default();
    match &i.kind {
        InstructionKind::MakeVariant { variant, fields } => {
            fee(b, 2)?;
            out.enum_variants = Variants {
                domain: variants.len() as u32,
                constructed: BTreeSet::from([*variant]),
                ..Variants::default()
            };
            for slot in &life.slots {
                fee(b, slot.path.len().saturating_add(1))?;
                let [PlaceProjection::Variant(projection)] = slot.path.as_slice() else {
                    unreachable!()
                };
                if projection.variant != *variant {
                    continue;
                }
                let actual = values
                    .get(fields[projection.index as usize].0 as usize)
                    .ok_or_else(|| failure("Enum actual payload is absent"))?;
                let mut permission = Permission::default();
                union(&mut permission, &actual.pointer, b)?;
                out.slots.insert(slot.path.clone(), permission);
            }
        }
        InstructionKind::ReadVariant {
            value,
            variant,
            index,
        } => {
            let path = vec![PlaceProjection::Variant(zil_ir::VariantProjection {
                variant: *variant,
                index: *index,
            })];
            let actual = values
                .get(value.0 as usize)
                .ok_or_else(|| failure("Enum read operand is absent"))?;
            ordered(b, actual.slots.len(), path.len())?;
            if let Some(permission) = actual.slots.get(&path) {
                union(&mut out.pointer, permission, b)?;
            }
        }
        InstructionKind::VariantTag { .. } => {}
        _ => unreachable!(),
    }
    Ok(Some(out))
}

/// Projection authority follows the actual immutable value and CFG tag guard.
/// A permission for another alternative never licenses reading this payload.
pub(super) fn projections(
    f: &TypedFunction,
    graph: &(Vec<Vec<usize>>, Vec<bool>),
    facts: &Facts,
    b: &mut Budget,
) -> CompileResult<()> {
    if facts.enum_values.is_empty() {
        return Ok(());
    }
    let mut definitions = vec![None; f.values.len()];
    fee(b, definitions.len())?;
    for block in &f.blocks {
        for i in &block.instructions {
            fee(b, 1)?;
            if let Some(id) = i.result {
                definitions[id.0 as usize] = Some(&i.kind);
            }
        }
    }
    let mut initial = BTreeMap::new();
    for (id, tags) in &facts.enum_values {
        fee(b, tags.domain as usize)?;
        initial.insert(*id, tags.possible());
    }
    let guard = |condition: ValueId| -> Option<(ValueId, u32)> {
        if let Some(InstructionKind::ErrorTag { value }) = definitions[condition.0 as usize] {
            return Some((*value, 1));
        }
        let Some(InstructionKind::Binary {
            op: zil_ir::BinaryOp::Equal,
            left,
            right,
        }) = definitions[condition.0 as usize]
        else {
            return None;
        };
        for (tag, scalar) in [(*left, *right), (*right, *left)] {
            if let (
                Some(InstructionKind::ErrorTag { value }),
                Some(InstructionKind::Constant(Constant::Bool(n))),
            ) = (definitions[tag.0 as usize], definitions[scalar.0 as usize])
            {
                return Some((*value, u32::from(*n)));
            }
            if let (
                Some(InstructionKind::VariantTag { value }),
                Some(InstructionKind::Constant(Constant::I32(n))),
            ) = (definitions[tag.0 as usize], definitions[scalar.0 as usize])
                && *n >= 0
                && facts.enum_values.contains_key(value)
            {
                return Some((*value, *n as u32));
            }
        }
        None
    };
    let mut incoming = vec![None; f.blocks.len()];
    fee(b, incoming.len())?;
    incoming[f.entry.0 as usize] = Some(initial);
    loop {
        let mut changed = false;
        for (bi, block) in f.blocks.iter().enumerate() {
            if !graph.1[bi] {
                continue;
            }
            let Some(state) = incoming[bi].as_ref() else {
                continue;
            };
            let mut edges = Vec::new();
            match block.terminator {
                Terminator::Jump(to) => edges.push((to, None)),
                Terminator::Branch {
                    condition,
                    then_block,
                    else_block,
                } => {
                    let test = guard(condition);
                    edges.push((then_block, test.map(|(id, tag)| (id, tag, true))));
                    edges.push((else_block, test.map(|(id, tag)| (id, tag, false))));
                }
                _ => {}
            }
            fee(
                b,
                state
                    .values()
                    .map(BTreeSet::len)
                    .sum::<usize>()
                    .saturating_mul(edges.len()),
            )?;
            let state = state.clone();
            for (to, test) in edges {
                if !graph.1[to.0 as usize] || !graph.0[to.0 as usize].contains(&bi) {
                    continue;
                }
                let mut next = state.clone();
                if let Some((id, tag, equal)) = test
                    && let Some(tags) = next.get_mut(&id)
                {
                    tags.retain(|v| (*v == tag) == equal);
                    if tags.is_empty() {
                        continue;
                    }
                }
                match &mut incoming[to.0 as usize] {
                    None => {
                        incoming[to.0 as usize] = Some(next);
                        changed = true;
                    }
                    Some(old) => {
                        for (id, tags) in next {
                            let target = old.entry(id).or_default();
                            for tag in tags {
                                ordered(b, target.len(), 0)?;
                                changed |= target.insert(tag);
                            }
                        }
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    for (bi, block) in f.blocks.iter().enumerate() {
        if !graph.1[bi] {
            continue;
        }
        for i in &block.instructions {
            fee(b, 1)?;
            let projection = match i.kind {
                InstructionKind::ReadVariant { value, variant, .. } => Some((value, variant)),
                InstructionKind::ReadErrorUnion { value, is_error } => {
                    Some((value, u32::from(is_error)))
                }
                _ => None,
            };
            if let Some((value, variant)) = projection
                && facts.enum_values.contains_key(&value)
            {
                let tags = incoming[bi].as_ref().and_then(|s| s.get(&value));
                if tags.is_none_or(|tags| tags.len() != 1 || !tags.contains(&variant)) {
                    return Err(failure(
                        "borrowed Enum projection lacks its reachable active tag",
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
    pub(super) graphs: Vec<(Vec<Vec<usize>>, Vec<bool>)>,
    pub(super) inactive: Vec<BTreeSet<ValueId>>,
}
pub(super) fn refine(
    module: &TypedModule,
    original: &[(Vec<Vec<usize>>, Vec<bool>)],
    frozen: &[Facts],
    b: &mut Budget,
) -> CompileResult<Option<Refinement>> {
    if !b.stored_enum_addresses {
        return Ok(None);
    }
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
        let tags = &frozen[fi].enum_values;
        let known_equal = |condition: ValueId, b: &mut Budget| -> CompileResult<Option<bool>> {
            fee(b, 3)?;
            if let Some(InstructionKind::ErrorTag { value }) = definitions[condition.0 as usize]
                && let Some(tags) = tags.get(value)
            {
                tags.work(b)?;
                let possible = tags.possible();
                if possible.len() == 1 {
                    return Ok(Some(possible.contains(&1)));
                }
            }
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
            for (tag, constant) in [(*left, *right), (*right, *left)] {
                if let (
                    Some(InstructionKind::ErrorTag { value }),
                    Some(InstructionKind::Constant(Constant::Bool(n))),
                ) = (
                    definitions[tag.0 as usize],
                    definitions[constant.0 as usize],
                ) && let Some(actual) = tags.get(value)
                {
                    actual.work(b)?;
                    let possible = actual.possible();
                    if possible.len() == 1 {
                        return Ok(Some(possible.contains(&u32::from(*n))));
                    }
                }
            }
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
                // Compose the sealed Option proof from the same original fixed point.
                // The replay may not discover a second opportunity to trim an edge.
                ordered(b, frozen[fi].none_values.len(), 0)?;
                if matches!(*n, 0 | 1) && frozen[fi].none_values.contains(value) {
                    return Ok(Some(*n == 0));
                }
                ordered(b, tags.len(), 0)?;
                if let Some(tags) = tags.get(value) {
                    tags.work(b)?;
                    fee(b, tags.domain as usize)?;
                    let possible = tags.possible();
                    if !possible.is_empty() {
                        if *n < 0 || !possible.contains(&(*n as u32)) {
                            return Ok(Some(false));
                        }
                        if possible.len() == 1 {
                            return Ok(Some(true));
                        }
                    }
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

pub(super) fn poll_instance(instance: &LifeNominalInstance, b: &mut Budget) -> CompileResult<()> {
    fee(b, 96)?;
    let (Type::BorrowedStorage { storage, life }, [argument]) =
        (&instance.ty, instance.arguments.as_slice())
    else {
        return Err(failure("Poll has no complete qualified Type actual"));
    };
    let actual = option_address::shared_actual(argument, b)?
        .ok_or_else(|| failure("Poll has no complete shared Address actual"))?;
    let Type::Enum { name, variants } = storage.as_ref() else {
        return Err(failure("Poll has no actual Enum layout"));
    };
    b.type_work(actual, true, "Poll actual fingerprint")?;
    fee(b, name.len().saturating_add(96))?;
    if name != &format!("@std.task:v1:Poll:{}", actual.fingerprint())
        || !matches!(variants.as_slice(),[(pending,empty),(ready,fields)] if pending=="Pending" && empty.is_empty() && ready=="Ready" && fields.len()==1 && fields[0]==*actual)
        || !matches!(actual, Type::Address { mutable: false, .. })
        || life.binder_count != 1
        || !life.nominal_arguments.is_empty()
        || !matches!(life.retained.as_slice(),[m] if m.path==[LifeTypeProjection::TypeArgument(0)] && m.class==0 && m.binder==0)
        || !matches!(life.slots.as_slice(),[s] if s.path==[PlaceProjection::Variant(zil_ir::VariantProjection {variant:1,index:0})] && s.binder==0 && s.mode==StoredBorrowMode::SharedAddress)
        || !shape(&instance.ty, b)?
    {
        return Err(failure(
            "Poll full actual identity or shared payload relation differs",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[test]
fn snapshot_keeps_original_join_normalization_of_nonpresent_enum_rows() {
    let child = Variants {
        domain: 0,
        constructed: BTreeSet::from([1]),
        ..Variants::default()
    };
    for domain in [0, 2] {
        let value = Value {
            enum_variants: Variants {
                domain,
                constructed: BTreeSet::from([1]),
                nested: BTreeMap::from([(vec![PlaceProjection::ErrorArm(false)], child.clone())]),
                ..Variants::default()
            },
            ..Value::default()
        };
        let mut old = Budget::new(&Limits::default());
        let mut new = Budget::new(&Limits::default());
        assert_eq!(
            super::snapshot(&value, &mut new).unwrap(),
            super::snapshot_by_union(&value, &mut old).unwrap()
        );
    }
}
