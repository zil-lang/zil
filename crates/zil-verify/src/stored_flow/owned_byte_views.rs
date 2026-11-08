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

fn source(
    module: &TypedModule,
    f: &TypedFunction,
    buffer: ValueId,
    span: Span,
    b: &mut Budget,
) -> Check<Source> {
    b.spend(3)?;
    if !matches!(vt(f, buffer)?, Type::Desc(payload) if payload.as_ref() == &Type::ByteBuffer) {
        return Err(err("byte view lacks an actual ByteBuffer description"));
    }
    b.spend(f.blocks.len())?;
    let mut producer = None;
    for block in &f.blocks {
        b.spend(block.instructions.len())?;
        for i in &block.instructions {
            if i.result == Some(buffer) {
                if producer.is_some() {
                    return Err(err("byte description has duplicate actual SSA producers"));
                }
                producer = Some(&i.kind);
            }
        }
    }
    let (owner, field) = match producer {
        Some(InstructionKind::DescribePackageField { local, index }) => (*local, Some(*index)),
        Some(InstructionKind::DescribeLocal { local }) => (*local, None),
        _ => return Err(unavailable(span)),
    };
    b.spend(f.parameters.len().saturating_add(4))?;
    if f.parameters.contains(&owner) {
        return Err(unavailable(span));
    }
    let local = f
        .locals
        .get(owner.0 as usize)
        .ok_or_else(|| err("byte owning local is absent"))?;
    let payload = match (field, &local.ty) {
        (None, ty) => ty,
        (Some(index), Type::Package { fields, .. }) => {
            &fields
                .get(index as usize)
                .ok_or_else(|| err("byte owning Package field is absent"))?
                .1
        }
        _ => return Err(unavailable(span)),
    };
    if !matches!(payload, Type::Managed(inner) if inner.as_ref() == &Type::ByteBuffer) {
        return Err(err(
            "byte payload source is not an actual Managed<ByteBuffer> field",
        ));
    }
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
    b.spend(2)?;
    Ok(Source {
        retained_port: None,
        base: HoldRoot::Local(owner),
        leaf: Vec::new(),
        inline: field.into_iter().map(PlaceProjection::Field).collect(),
        heap_element: None,
    })
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

pub(super) struct Sites {
    values: Vec<usize>,
}
impl Sites {
    pub(super) fn collect(f: &TypedFunction, b: &mut Budget) -> Check<Self> {
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
        values: &[Payload],
        live: &[usize],
        loans: &[Loan],
        requirements: &mut Vec<(Source, Source)>,
        b: &mut Budget,
    ) -> Check<()> {
        if self.values.is_empty() {
            return Ok(());
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
                        | InstructionKind::ReadAddress { .. }
                        | InstructionKind::SliceLength { .. }
                )
            {
                return Err(unavailable(i.span));
            }
        }
        Ok(())
    }
}
