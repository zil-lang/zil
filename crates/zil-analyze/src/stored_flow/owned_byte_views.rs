//! A checked ByteBuffer payload edge pins its actual local owning head.
//! The head is a conservative lifetime/conflict anchor, never a payload address.
use super::*;
use zil_base::Span;

fn closed(span: Span) -> Vec<Diagnostic> {
    vec![Diagnostic::unsupported(
        Phase::Analyze,
        "stored byte view requires an actual local owning ByteBuffer head and a synchronous read consumer",
        Some(span),
    )]
}

fn head(
    module: &TypedModule,
    f: &TypedFunction,
    buffer: ValueId,
    span: Span,
    b: &mut Budget,
) -> CompileResult<Target> {
    fee(b, 3)?;
    if !matches!(value_type(f, buffer)?, Type::Desc(inner) if inner.as_ref() == &Type::ByteBuffer) {
        return Err(failure("byte view operand is not actual Desc<ByteBuffer>"));
    }
    fee(b, f.blocks.len())?;
    let mut owner = None;
    for block in &f.blocks {
        fee(b, block.instructions.len())?;
        for producer in &block.instructions {
            if producer.result != Some(buffer) {
                continue;
            }
            if owner.is_some() {
                return Err(failure(
                    "byte description has duplicate actual SSA producers",
                ));
            }
            owner = match producer.kind {
                InstructionKind::DescribeLocal { local } => Some((local, None)),
                InstructionKind::DescribePackageField { local, index } => {
                    Some((local, Some(index)))
                }
                _ => return Err(closed(span)),
            };
        }
    }
    let (local, field) = owner.ok_or_else(|| closed(span))?;
    fee(b, f.parameters.len().saturating_add(4))?;
    if f.parameters.contains(&local) {
        return Err(closed(span));
    }
    let actual = &f
        .locals
        .get(local.0 as usize)
        .ok_or_else(|| failure("byte owner local is absent"))?
        .ty;
    let actual = if let Some(index) = field {
        let Type::Package { fields, .. } = actual else {
            return Err(closed(span));
        };
        &fields
            .get(index as usize)
            .ok_or_else(|| failure("byte owner field is absent"))?
            .1
    } else {
        actual
    };
    if !matches!(actual, Type::Managed(inner) if inner.as_ref() == &Type::ByteBuffer) {
        return Err(failure(
            "byte payload edge lacks the actual Managed<ByteBuffer> owner",
        ));
    }
    fee(b, module.resources.len().saturating_add(1))?;
    let mut canonical = 0;
    for resource in &module.resources {
        fee(b, resource.name.len().saturating_add(19))?;
        if resource.payload == Type::ByteBuffer || resource.name == "@builtin:ByteBuffer" {
            if resource.payload != Type::ByteBuffer
                || resource.name != "@builtin:ByteBuffer"
                || resource.teardown.is_some()
            {
                return Err(failure(
                    "byte view requires the canonical sealed ByteBuffer resource",
                ));
            }
            canonical += 1;
        }
    }
    if canonical != 1 || !module.target.is_supported_x64_llvm() {
        return Err(failure(
            "byte view requires one canonical sealed ByteBuffer resource",
        ));
    }
    fee(b, 1)?;
    Ok(Target {
        retained_port: None,
        base: HoldRoot::Local(local),
        stored: Vec::new(),
        place: field.map_or_else(Vec::new, |index| vec![PlaceProjection::Field(index)]),
        heap_element: None,
    })
}

pub(super) fn instruction(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    let InstructionKind::ByteBufferBorrow {
        buffer,
        mutable: false,
    } = i.kind
    else {
        return Ok(false);
    };
    fee(b, 4)?;
    let result = i.result.ok_or_else(|| failure("byte view has no result"))?;
    if !matches!(value_type(f, result)?, Type::Address { mutable: false, inner }
        if matches!(inner.as_ref(), Type::Slice { element } if element.as_ref() == &Type::U8))
    {
        return Err(failure(
            "byte view must produce an actual shared byte slice",
        ));
    }
    head(module, f, buffer, i.span, b)?;
    Ok(true)
}

pub(super) fn transfer(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<Permission> {
    let InstructionKind::ByteBufferBorrow {
        buffer,
        mutable: false,
    } = i.kind
    else {
        return Err(closed(i.span));
    };
    let target = head(module, f, buffer, i.span, b)?;
    fee(b, target_width(&target).saturating_add(1))?;
    Ok(Permission {
        targets: BTreeSet::from([target]),
        loans: Loans::new(),
    })
}

pub(super) struct Sites {
    ids: BTreeSet<usize>,
}
impl Sites {
    pub(super) fn collect(f: &TypedFunction, b: &mut Budget) -> CompileResult<Self> {
        fee(b, f.blocks.len())?;
        let mut ids = BTreeSet::new();
        for block in &f.blocks {
            fee(b, block.instructions.len())?;
            for i in &block.instructions {
                if matches!(i.kind, InstructionKind::ByteBufferBorrow { .. }) {
                    let value = i
                        .result
                        .ok_or_else(|| failure("byte view site lacks result"))?;
                    ordered(b, ids.len(), 0)?;
                    fee(b, 1)?;
                    ids.insert(value.0 as usize);
                }
            }
        }
        Ok(Self { ids })
    }
    fn active(&self, active: &Loans, b: &mut Budget) -> CompileResult<bool> {
        for id in &self.ids {
            ordered(b, active.len(), 0)?;
            if active.contains(id) {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn carried(&self, p: &Permission, loans: &[Loan], b: &mut Budget) -> CompileResult<bool> {
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
        values: &[Value],
        active: &Loans,
        loans: &[Loan],
        apart: &mut BTreeSet<(Target, Target)>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        if self.ids.is_empty() {
            return Ok(());
        }
        let access = match i.kind {
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
            } if self.active(active, b)? => Some((buffer, true)),
            _ => None,
        };
        if let Some((buffer, write)) = access {
            let target = head(module, f, buffer, i.span, b)?;
            fee(b, target_width(&target).saturating_add(1))?;
            let actual = Permission {
                targets: BTreeSet::from([target]),
                loans: Loans::new(),
            };
            use_permission(&actual, write, active, loans, apart, b)?;
        }
        if let InstructionKind::DropManaged { local } | InstructionKind::DropPackage { local } =
            i.kind
            && self.active(active, b)?
        {
            fee(b, 1)?;
            let actual = Permission {
                targets: BTreeSet::from([Target {
                    retained_port: None,
                    base: HoldRoot::Local(local),
                    stored: Vec::new(),
                    place: Vec::new(),
                    heap_element: None,
                }]),
                loans: Loans::new(),
            };
            use_permission(&actual, true, active, loans, apart, b)?;
        }
        for input in resources::uses(&i.kind) {
            fee(b, 1)?;
            let p = &values
                .get(input.0 as usize)
                .ok_or_else(|| failure("byte view consumer input is absent"))?
                .pointer;
            if !self.carried(p, loans, b)? {
                continue;
            }
            let allowed = matches!(
                i.kind,
                InstructionKind::AddressIndex { .. }
                    | InstructionKind::AddressSlice { .. }
                    | InstructionKind::ReborrowAddress { mutable: false, .. }
                    | InstructionKind::ReadAddress { .. }
                    | InstructionKind::SliceLength { .. }
            ) || matches!(i.kind, InstructionKind::Store { local, .. }
                    if matches!(&f.locals[local.0 as usize].ty, Type::Address { mutable: false, inner }
                        if inner.as_ref() == &Type::U8 || matches!(inner.as_ref(), Type::Slice { element } if element.as_ref() == &Type::U8)));
            if !allowed {
                return Err(closed(i.span));
            }
        }
        Ok(())
    }
}
