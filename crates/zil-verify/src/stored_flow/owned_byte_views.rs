//! Reconstruct the actual owned head behind a synchronous ByteBuffer payload
//! view. A conservative head pin does not identify header bytes as payload.
use super::*;
use zil_base::Span;

fn unavailable(span: Span) -> Diagnostic {
    unsupported(
        "stored byte view needs its actual local owning head and a synchronous read consumer",
        Some(span),
    )
}

#[derive(Clone, Copy)]
struct Head {
    owner: LocalId,
    field: Option<u32>,
}
pub(crate) struct Heads {
    functions: Vec<Vec<Option<Head>>>,
}
impl Heads {
    fn reconstruct(module: &TypedModule, b: &mut Budget) -> Check<Self> {
        b.spend(module.resources.len().saturating_add(1))?;
        let mut sealed = 0;
        for definition in &module.resources {
            b.spend(definition.name.len().saturating_add(19))?;
            if definition.name == "@builtin:ByteBuffer" || definition.payload == Type::ByteBuffer {
                if definition.name != "@builtin:ByteBuffer"
                    || definition.payload != Type::ByteBuffer
                    || definition.teardown.is_some()
                {
                    return Err(err(
                        "byte payload view resource definition is not canonical and sealed",
                    ));
                }
                sealed += 1;
            }
        }
        if sealed != 1 || !module.target.is_supported_x64_llvm() {
            return Err(err(
                "byte payload view needs one canonical sealed ByteBuffer resource",
            ));
        }
        b.spend(module.functions.len())?;
        let mut functions = Vec::with_capacity(module.functions.len());
        for f in &module.functions {
            b.spend(
                f.values
                    .len()
                    .saturating_mul(3)
                    .saturating_add(f.blocks.len()),
            )?;
            let mut sources = vec![None; f.values.len()];
            let mut defined = vec![false; f.values.len()];
            for block in &f.blocks {
                b.spend(block.instructions.len())?;
                for operation in &block.instructions {
                    let Some(result) = operation.result else {
                        continue;
                    };
                    let actual = defined
                        .get_mut(result.0 as usize)
                        .ok_or_else(|| err("byte source definition is outside actual values"))?;
                    if *actual {
                        return Err(err("byte source has more than one actual SSA definition"));
                    }
                    *actual = true;
                    if !matches!(vt(f, result)?, Type::Desc(payload) if payload.as_ref() == &Type::ByteBuffer)
                    {
                        continue;
                    }
                    let (owner, field) = match operation.kind {
                        InstructionKind::DescribePackageField { local, index } => {
                            (local, Some(index))
                        }
                        InstructionKind::DescribeLocal { local } => (local, None),
                        _ => continue,
                    };
                    b.spend(f.parameters.len().saturating_add(4))?;
                    if f.parameters.contains(&owner) {
                        continue;
                    }
                    let local = f
                        .locals
                        .get(owner.0 as usize)
                        .ok_or_else(|| err("byte owning local is absent"))?;
                    let ty = match (field, &local.ty) {
                        (None, ty) => ty,
                        (Some(index), Type::Package { fields, .. }) => {
                            &fields
                                .get(index as usize)
                                .ok_or_else(|| err("byte owning Package field is absent"))?
                                .1
                        }
                        _ => continue,
                    };
                    if matches!(ty, Type::Managed(inner) if inner.as_ref() == &Type::ByteBuffer) {
                        sources[result.0 as usize] = Some(Head { owner, field });
                    }
                }
            }
            functions.push(sources);
        }
        Ok(Self { functions })
    }
}
fn selected(
    module: &TypedModule,
    f: &TypedFunction,
    buffer: ValueId,
    b: &mut Budget,
) -> Check<Option<Head>> {
    if b.owned_byte_heads.is_none() {
        let heads = Heads::reconstruct(module, b)?;
        b.owned_byte_heads = Some(heads);
    }
    b.spend(1)?;
    Ok(b.owned_byte_heads
        .as_ref()
        .and_then(|heads| heads.functions.get(f.id.0 as usize))
        .and_then(|row| row.get(buffer.0 as usize))
        .copied()
        .flatten())
}

fn source(
    module: &TypedModule,
    f: &TypedFunction,
    buffer: ValueId,
    span: Span,
    b: &mut Budget,
) -> Check<Source> {
    b.spend(4)?;
    if !matches!(vt(f, buffer)?, Type::Desc(payload) if payload.as_ref() == &Type::ByteBuffer) {
        return Err(err("byte view lacks an actual ByteBuffer description"));
    }
    let Head { owner, field } = selected(module, f, buffer, b)?.ok_or_else(|| unavailable(span))?;
    b.spend(2)?;
    Ok(Source {
        retained_port: None,
        base: HoldRoot::Local(owner),
        leaf: Vec::new(),
        inline: field.into_iter().map(PlaceProjection::Field).collect(),
        heap_element: None,
    })
}

pub(super) fn description_instruction(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    // Preserve the existing Option description check without opening the new
    // head inventory for a module containing only ordinary helper views.
    if !b.shared_byte_borrow || b.store_option_results {
        return Ok(false);
    }
    if !matches!(i.kind, InstructionKind::DescribePackageField { .. }) {
        return Ok(false);
    }
    let Some(result) = i.result else {
        return Ok(false);
    };
    b.spend(3)?;
    if !matches!(vt(f, result)?, Type::Desc(inner) if inner.as_ref() == &Type::ByteBuffer) {
        return Ok(false);
    }
    Ok(selected(module, f, result, b)?.is_some())
}

pub(super) fn instruction(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    if let InstructionKind::ByteBufferBorrow {
        buffer,
        mutable: false,
    } = i.kind
    {
        b.spend(4)?;
        let result = i
            .result
            .ok_or_else(|| err("byte payload view result is absent"))?;
        let valid = matches!(vt(f, result)?, Type::Address { mutable: false, inner }
            if matches!(inner.as_ref(), Type::Slice { element } if element.as_ref() == &Type::U8));
        if !valid {
            return Err(err(
                "byte payload view must be a shared address of a byte slice",
            ));
        }
        source(module, f, buffer, i.span, b)?;
        return Ok(true);
    }
    Ok(false)
}

pub(super) fn transfer(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<Permission> {
    let InstructionKind::ByteBufferBorrow {
        buffer,
        mutable: false,
    } = i.kind
    else {
        return Err(unavailable(i.span));
    };
    let source = source(module, f, buffer, i.span, b)?;
    b.spend(width(&source).saturating_add(1))?;
    Ok(Permission {
        sources: vec![source],
        loans: Vec::new(),
    })
}

pub(super) fn profile(module: &TypedModule, b: &mut Budget) -> Check<bool> {
    if !b.shared_byte_borrow {
        return Ok(false);
    }
    b.spend(module.functions.len())?;
    let mut seen = false;
    for function in &module.functions {
        b.spend(function.blocks.len())?;
        for block in &function.blocks {
            b.spend(block.instructions.len())?;
            for operation in &block.instructions {
                seen |= instruction(module, function, operation, b)?;
            }
        }
    }
    if !seen {
        return Ok(false);
    }
    if module.resources.len() != 1 {
        return Err(unavailable(Span::default()));
    }
    b.spend(module.closures.len())?;
    for definition in &module.closures {
        b.spend(definition.captures.len())?;
        for capture in &definition.captures {
            if value(capture, b)? {
                return Err(unavailable(Span::default()));
            }
        }
    }
    Ok(true)
}

pub(super) fn owner_instruction(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    if !b.shared_byte_borrow {
        return Ok(false);
    }
    let local = match i.kind {
        InstructionKind::DescribeLocal { local }
        | InstructionKind::TakeLocal { local }
        | InstructionKind::DropManaged { local } => local,
        _ => return Ok(false),
    };
    b.spend(5)?;
    let ty = &f
        .locals
        .get(local.0 as usize)
        .ok_or_else(|| err("byte owner operation local absent"))?
        .ty;
    if !matches!(ty, Type::Managed(inner) if inner.as_ref() == &Type::ByteBuffer) {
        return Ok(false);
    }
    let result = i.result.and_then(|id| f.values.get(id.0 as usize));
    let valid = match i.kind {
        InstructionKind::DescribeLocal { .. } => {
            matches!(result, Some(Type::Desc(inner)) if inner.as_ref() == &Type::ByteBuffer)
        }
        InstructionKind::TakeLocal { .. } => {
            matches!(result, Some(Type::Managed(inner)) if inner.as_ref() == &Type::ByteBuffer)
        }
        InstructionKind::DropManaged { .. } => i.result.is_none(),
        _ => false,
    };
    if !valid {
        return Err(err(
            "byte owner operation result differs from its actual carrier mode",
        ));
    }
    if b.owned_byte_heads.is_none() {
        b.owned_byte_heads = Some(Heads::reconstruct(module, b)?);
    }
    Ok(true)
}

pub(super) fn value(ty: &Type, b: &mut Budget) -> Check<bool> {
    b.spend(3)?;
    let direct = matches!(ty, Type::Managed(payload) | Type::Desc(payload) if payload.as_ref() == &Type::ByteBuffer);
    if direct {
        return Ok(true);
    }
    if let Type::Package { fields, .. } = ty {
        b.spend(fields.len())?;
        let mut count = 0;
        let mut closed = true;
        for (_, child) in fields {
            match child {
                Type::Managed(payload) if payload.as_ref() == &Type::ByteBuffer => count += 1,
                _ => closed &= retained::closed_copy(child, b)?,
            }
        }
        if count != 0 && closed {
            return Ok(true);
        }
    }
    type_work(ty, &b.limits.clone(), b)?;
    b.spend(1)?;
    let mut todo = vec![ty];
    while let Some(ty) = todo.pop() {
        b.spend(1)?;
        match ty {
            Type::ByteBuffer => return Err(unavailable(Span::default())),
            Type::Address { inner, .. }
            | Type::Managed(inner)
            | Type::Desc(inner)
            | Type::Opaque { hidden: inner, .. }
            | Type::Array { element: inner, .. }
            | Type::Slice { element: inner }
            | Type::OwnedStore { element: inner }
            | Type::BorrowedStorage { storage: inner, .. } => {
                b.spend(1)?;
                todo.push(inner);
            }
            Type::ErrorUnion { error, result } => {
                b.spend(2)?;
                todo.push(error);
                todo.push(result);
            }
            Type::Tuple(children) => {
                b.spend(children.len())?;
                todo.extend(children);
            }
            Type::Package { fields, .. } | Type::Record { fields, .. } => {
                b.spend(fields.len())?;
                todo.extend(fields.iter().map(|(_, child)| child));
            }
            Type::Enum { variants, .. } => {
                b.spend(variants.len())?;
                for (_, children) in variants {
                    b.spend(children.len())?;
                    todo.extend(children);
                }
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result }
            | Type::BorrowedFunctionPointer {
                parameters, result, ..
            } => {
                b.spend(parameters.len().saturating_add(1))?;
                todo.push(result);
                todo.extend(parameters);
            }
            _ => {}
        }
    }
    Ok(false)
}

pub(super) struct Sites {
    values: Vec<usize>,
}
impl Sites {
    pub(super) fn present(&self) -> bool {
        !self.values.is_empty()
    }
    pub(super) fn collect(f: &TypedFunction, b: &mut Budget) -> Check<Self> {
        // Every selected local payload view was independently authenticated
        // by the unconditional instruction inventory before solving windows.
        if !b.shared_byte_borrow || b.owned_byte_heads.is_none() {
            return Ok(Self { values: Vec::new() });
        }
        b.spend(f.blocks.len().saturating_add(f.values.len()))?;
        let mut values = Vec::with_capacity(f.values.len());
        for block in &f.blocks {
            b.spend(block.instructions.len())?;
            for i in &block.instructions {
                if matches!(i.kind, InstructionKind::ByteBufferBorrow { .. }) {
                    let result = i
                        .result
                        .ok_or_else(|| err("byte view site result absent"))?;
                    b.spend(1)?;
                    values.push(result.0 as usize);
                }
            }
        }
        Ok(Self { values })
    }
    fn active(&self, active: &[usize], b: &mut Budget) -> Check<bool> {
        b.spend(amount(self.values.len(), active.len().saturating_add(1))?)?;
        Ok(self.values.iter().any(|id| active.contains(id)))
    }
    fn carried(&self, p: &Permission, loans: &[Loan], b: &mut Budget) -> Check<bool> {
        if p.loans.is_empty() {
            return Ok(false);
        }
        self.active(&ancestry(&p.loans, loans, b)?, b)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check(
        &self,
        module: &TypedModule,
        f: &TypedFunction,
        i: &Instruction,
        state: &Frame,
        values: &[Payload],
        live: &[usize],
        loans: &[Loan],
        requirements: &mut Vec<(Source, Source)>,
        b: &mut Budget,
    ) -> Check<()> {
        if self.values.is_empty() {
            return Ok(());
        }
        if matches!(
            i.kind,
            InstructionKind::Call { .. }
                | InstructionKind::ForeignCall { .. }
                | InstructionKind::CallIndirect { .. }
                | InstructionKind::CallDynSlot { .. }
                | InstructionKind::CallClosure { .. }
                | InstructionKind::CallClosureBorrowed { .. }
                | InstructionKind::CallClosureShared { .. }
                | InstructionKind::CallClosureMut { .. }
                | InstructionKind::CallClosureOnce { .. }
                | InstructionKind::MakeClosure { .. }
                | InstructionKind::WithForeignCallback { .. }
                | InstructionKind::WithScopedThread { .. }
                | InstructionKind::WithSyncCell { .. }
        ) && self.active(live, b)?
        {
            return Err(unavailable(i.span));
        }
        if let InstructionKind::BorrowLocal { local } | InstructionKind::BorrowLocalMut { local } =
            i.kind
        {
            b.spend(1)?;
            if let Some(value) = state.locals.get(local.0 as usize)
                && self.carried(&value.address, loans, b)?
            {
                return Err(unavailable(i.span));
            }
        }
        let byte_access = match i.kind {
            InstructionKind::ByteBufferBorrow {
                buffer,
                mutable: false,
            } => Some((buffer, false)),
            InstructionKind::ByteBufferReserve { buffer, .. }
            | InstructionKind::ByteBufferPush { buffer, .. }
            | InstructionKind::ByteBufferTruncate { buffer, .. }
            | InstructionKind::ByteBufferBorrow {
                buffer,
                mutable: true,
            } if self.active(live, b)? => Some((buffer, true)),
            _ => None,
        };
        if let Some((buffer, write)) = byte_access {
            let source = source(module, f, buffer, i.span, b)?;
            b.spend(width(&source).saturating_add(1))?;
            let p = Permission {
                sources: vec![source],
                loans: Vec::new(),
            };
            access(&p, write, live, loans, requirements, b)?;
        }
        if let InstructionKind::DropManaged { local } | InstructionKind::DropPackage { local } =
            i.kind
            && self.active(live, b)?
        {
            b.spend(1)?;
            let p = Permission {
                sources: vec![Source {
                    retained_port: None,
                    base: HoldRoot::Local(local),
                    leaf: Vec::new(),
                    inline: Vec::new(),
                    heap_element: None,
                }],
                loans: Vec::new(),
            };
            access(&p, true, live, loans, requirements, b)?;
        }
        for input in instruction_operands(&i.kind) {
            b.spend(1)?;
            let p = &values
                .get(input.0 as usize)
                .ok_or_else(|| err("byte view consumer input absent"))?
                .address;
            if !self.carried(p, loans, b)? {
                continue;
            }
            let ordinary_local = if let InstructionKind::Store { local, .. } = i.kind {
                matches!(&f.locals[local.0 as usize].ty, Type::Address { mutable: false, inner }
                    if inner.as_ref() == &Type::U8 || matches!(inner.as_ref(), Type::Slice { element } if element.as_ref() == &Type::U8))
            } else {
                false
            };
            if !ordinary_local
                && !matches!(
                    i.kind,
                    InstructionKind::AddressIndex { .. }
                        | InstructionKind::AddressSlice { .. }
                        | InstructionKind::ReborrowAddress { mutable: false, .. }
                        | InstructionKind::SliceLength { .. }
                )
                && !matches!(i.kind, InstructionKind::ReadAddress { address }
                    if matches!(vt(f, address)?, Type::Address { mutable: false, inner } if inner.as_ref() == &Type::U8)
                        && i.result.is_some_and(|id| f.values.get(id.0 as usize) == Some(&Type::U8)))
            {
                return Err(unavailable(i.span));
            }
        }
        Ok(())
    }
}
