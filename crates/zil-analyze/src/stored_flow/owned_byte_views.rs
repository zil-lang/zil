//! A checked ByteBuffer payload edge pins its actual owning head.
//! The head is a conservative lifetime/conflict anchor, never a payload address.
use super::*;
use zil_base::Span;

fn closed(span: Span) -> Vec<Diagnostic> {
    vec![Diagnostic::unsupported(
        Phase::Analyze,
        "stored byte view requires an independently grounded ByteBuffer head and a synchronous read consumer",
        Some(span),
    )]
}

#[derive(Clone, Copy)]
enum Head {
    Local { local: LocalId, field: Option<u32> },
    Address(ValueId),
    Flow,
}
/// Immutable IR structure only; no live/initialized/epoch or permission result.
pub(crate) struct Heads {
    rows: Vec<Vec<Option<Head>>>,
    flow: bool,
}
impl Heads {
    fn new(module: &TypedModule, b: &mut Budget) -> CompileResult<Self> {
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
        fee(b, module.functions.len())?;
        let mut rows = Vec::with_capacity(module.functions.len());
        let mut flow = false;
        for f in &module.functions {
            fee(
                b,
                f.values
                    .len()
                    .saturating_mul(3)
                    .saturating_add(f.blocks.len()),
            )?;
            let mut row = vec![None; f.values.len()];
            let mut seen = vec![false; f.values.len()];
            for block in &f.blocks {
                fee(b, block.instructions.len())?;
                for i in &block.instructions {
                    let operation_buffer = match i.kind {
                        InstructionKind::ByteBufferBorrow { buffer, .. }
                        | InstructionKind::ByteBufferReserve { buffer, .. }
                        | InstructionKind::ByteBufferPush { buffer, .. }
                        | InstructionKind::ByteBufferTruncate { buffer, .. } => Some(buffer),
                        _ => None,
                    };
                    if let Some(buffer) = operation_buffer {
                        flow |= !matches!(
                            row.get(buffer.0 as usize),
                            Some(Some(Head::Local { .. } | Head::Address(_)))
                        );
                    }
                    let Some(result) = i.result else {
                        continue;
                    };
                    let present = seen
                        .get_mut(result.0 as usize)
                        .ok_or_else(|| failure("byte source SSA is outside actual values"))?;
                    if *present {
                        return Err(failure("byte source SSA has duplicate producers"));
                    }
                    *present = true;
                    if !matches!(value_type(f, result)?, Type::Desc(inner) if inner.as_ref() == &Type::ByteBuffer)
                    {
                        continue;
                    }
                    if let InstructionKind::DescribeAddress { address } = i.kind {
                        if !matches!(value_type(f, address)?, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Managed(payload) if payload.as_ref() == &Type::ByteBuffer))
                        {
                            return Err(failure(
                                "byte description address does not name an actual managed byte head",
                            ));
                        }
                        row[result.0 as usize] = Some(Head::Address(address));
                        continue;
                    }
                    row[result.0 as usize] = Some(Head::Flow);
                    let (local, field) = match i.kind {
                        InstructionKind::DescribeLocal { local } => (local, None),
                        InstructionKind::DescribePackageField { local, index } => {
                            (local, Some(index))
                        }
                        _ => continue,
                    };
                    fee(b, f.parameters.len().saturating_add(4))?;
                    if f.parameters.contains(&local) {
                        continue;
                    }
                    let ty = &f
                        .locals
                        .get(local.0 as usize)
                        .ok_or_else(|| failure("byte owner local is absent"))?
                        .ty;
                    let ty = if let Some(index) = field {
                        let Type::Package { fields, .. } = ty else {
                            continue;
                        };
                        &fields
                            .get(index as usize)
                            .ok_or_else(|| failure("byte owner field is absent"))?
                            .1
                    } else {
                        ty
                    };
                    if matches!(ty, Type::Managed(inner) if inner.as_ref() == &Type::ByteBuffer) {
                        row[result.0 as usize] = Some(Head::Local { local, field });
                    }
                }
            }
            rows.push(row);
        }
        Ok(Self { rows, flow })
    }
}
fn selected(
    module: &TypedModule,
    f: &TypedFunction,
    buffer: ValueId,
    b: &mut Budget,
) -> CompileResult<Option<Head>> {
    if b.owned_byte_heads.is_none() {
        let heads = Heads::new(module, b)?;
        b.owned_byte_heads = Some(heads);
    }
    fee(b, 1)?;
    Ok(b.owned_byte_heads
        .as_ref()
        .and_then(|heads| heads.rows.get(f.id.0 as usize))
        .and_then(|row| row.get(buffer.0 as usize))
        .copied()
        .flatten())
}

fn selection(
    module: &TypedModule,
    f: &TypedFunction,
    buffer: ValueId,
    span: Span,
    b: &mut Budget,
) -> CompileResult<Head> {
    fee(b, 4)?;
    if !matches!(value_type(f, buffer)?, Type::Desc(inner) if inner.as_ref() == &Type::ByteBuffer) {
        return Err(failure("byte view operand is not actual Desc<ByteBuffer>"));
    }
    let head = selected(module, f, buffer, b)?.ok_or_else(|| closed(span))?;
    fee(b, 1)?;
    Ok(head)
}

pub(super) fn description_instruction(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    // The Option lane already authenticates this operation independently.
    // A metadata-only description must not start an owned-view inventory.
    if !b.shared_byte_borrow || b.store_option_results {
        return Ok(false);
    }
    if !matches!(i.kind, InstructionKind::DescribePackageField { .. }) {
        return Ok(false);
    }
    let Some(result) = i.result else {
        return Ok(false);
    };
    fee(b, 3)?;
    if !matches!(value_type(f, result)?, Type::Desc(inner) if inner.as_ref() == &Type::ByteBuffer) {
        return Ok(false);
    }
    Ok(selected(module, f, result, b)?.is_some())
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
    selection(module, f, buffer, i.span, b)?;
    Ok(true)
}

fn permission(
    module: &TypedModule,
    f: &TypedFunction,
    buffer: ValueId,
    span: Span,
    values: &[Value],
    b: &mut Budget,
) -> CompileResult<Permission> {
    match selection(module, f, buffer, span, b)? {
        Head::Local { local, field } => {
            let target = Target {
                retained_port: None,
                base: HoldRoot::Local(local),
                stored: Vec::new(),
                place: field.map_or_else(Vec::new, |index| vec![PlaceProjection::Field(index)]),
                heap_element: None,
            };
            fee(b, target_width(&target).saturating_add(1))?;
            Ok(Permission {
                targets: BTreeSet::from([target]),
                loans: Loans::new(),
            })
        }
        Head::Address(address) => {
            let actual = values
                .get(address.0 as usize)
                .ok_or_else(|| failure("byte head address is outside actual values"))?;
            let mut out = Permission::default();
            union(&mut out, &actual.pointer, b)?;
            Ok(out)
        }
        Head::Flow => {
            let actual = values
                .get(buffer.0 as usize)
                .ok_or_else(|| failure("byte description is outside actual values"))?;
            let mut out = Permission::default();
            union(&mut out, &actual.store_description, b)?;
            Ok(out)
        }
    }
}

pub(super) fn transfer(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    values: &[Value],
    b: &mut Budget,
) -> CompileResult<Permission> {
    let InstructionKind::ByteBufferBorrow {
        buffer,
        mutable: false,
    } = i.kind
    else {
        return Err(closed(i.span));
    };
    // An address/heap head retains exactly its actual manager/element route
    // and every parent loan. An unsolved predecessor remains bottom until
    // the final grounded-value check; it never receives a synthetic local.
    permission(module, f, buffer, i.span, values, b)
}

pub(super) fn seed_description(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    values: &[Value],
    b: &mut Budget,
) -> CompileResult<Option<Permission>> {
    if !b.owned_byte_heads.as_ref().is_some_and(|heads| heads.flow)
        || !matches!(
            i.kind,
            InstructionKind::DescribeLocal { .. } | InstructionKind::DescribePackageField { .. }
        )
    {
        return Ok(None);
    }
    let Some(result) = i.result else {
        return Ok(None);
    };
    if !matches!(value_type(f,result)?,Type::Desc(payload) if payload.as_ref()==&Type::ByteBuffer) {
        return Ok(None);
    }
    Ok(Some(permission(module, f, result, i.span, values, b)?))
}

/// Select only the new pure-stored ByteBuffer family. This is scheduling;
/// executable sources and every ownership epoch are checked independently.
pub(super) fn profile(module: &TypedModule, b: &mut Budget) -> CompileResult<bool> {
    if !b.shared_byte_borrow {
        return Ok(false);
    }
    fee(b, module.functions.len())?;
    let mut actual = false;
    for f in &module.functions {
        fee(b, f.blocks.len())?;
        for block in &f.blocks {
            fee(b, block.instructions.len())?;
            for i in &block.instructions {
                actual |= instruction(module, f, i, b)?;
            }
        }
    }
    if !actual {
        return Ok(false);
    }
    if module.resources.len() != 1 {
        return Err(closed(Span::default()));
    }
    // Each actual head above independently authenticated this sole definition.
    fee(b, module.closures.len())?;
    for closure in &module.closures {
        fee(b, closure.captures.len())?;
        for capture in &closure.captures {
            if value(capture, b)? {
                return Err(closed(Span::default()));
            }
        }
    }
    Ok(true)
}

fn plain_package(ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    let Type::Package { fields, .. } = ty else {
        return Ok(false);
    };
    fee(b, fields.len())?;
    let mut owns = false;
    let mut closed = true;
    for (_, child) in fields {
        if matches!(child, Type::Managed(inner) if inner.as_ref() == &Type::ByteBuffer) {
            owns = true;
        } else {
            closed &= retained::closed_copy(child, b)?;
        }
    }
    Ok(owns && closed)
}

pub(super) fn drop_package(
    module: &TypedModule,
    f: &TypedFunction,
    local: LocalId,
    b: &mut Budget,
) -> CompileResult<bool> {
    if !b.shared_byte_borrow {
        return Ok(false);
    }
    let ty = &f
        .locals
        .get(local.0 as usize)
        .ok_or_else(|| failure("byte package cleanup local is absent"))?
        .ty;
    if !plain_package(ty, b)? {
        return Ok(false);
    }
    if b.owned_byte_heads.is_none() {
        b.owned_byte_heads = Some(Heads::new(module, b)?);
    }
    Ok(true)
}

pub(super) fn value(ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    fee(b, 3)?;
    if matches!(ty, Type::Managed(inner) | Type::Desc(inner) if inner.as_ref() == &Type::ByteBuffer)
    {
        return Ok(true);
    }
    if plain_package(ty, b)? {
        return Ok(true);
    }
    // No Address/qualification/Sum/container wrapping turns a new owning byte
    // leaf into this finite direct family. Other values keep their old checks.
    b.type_work(ty, true, "stored byte owner nesting inventory")?;
    let mut pending = vec![ty];
    while let Some(current) = pending.pop() {
        fee(b, 1)?;
        match current {
            Type::ByteBuffer => return Err(closed(Span::default())),
            Type::Managed(child)
            | Type::Desc(child)
            | Type::Address { inner: child, .. }
            | Type::Array { element: child, .. }
            | Type::Slice { element: child }
            | Type::OwnedStore { element: child }
            | Type::Opaque { hidden: child, .. }
            | Type::BorrowedStorage { storage: child, .. } => {
                fee(b, 1)?;
                pending.push(child);
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                fee(b, fields.len())?;
                pending.extend(fields.iter().map(|(_, child)| child));
            }
            Type::Tuple(fields) => {
                fee(b, fields.len())?;
                pending.extend(fields);
            }
            Type::Enum { variants, .. } => {
                fee(b, variants.len())?;
                for (_, fields) in variants {
                    fee(b, fields.len())?;
                    pending.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                fee(b, 2)?;
                pending.extend([error.as_ref(), result.as_ref()]);
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result }
            | Type::BorrowedFunctionPointer {
                parameters, result, ..
            } => {
                fee(b, parameters.len().saturating_add(1))?;
                pending.extend(parameters);
                pending.push(result);
            }
            _ => {}
        }
    }
    Ok(false)
}

pub(super) struct Sites {
    ids: BTreeSet<usize>,
}
impl Sites {
    pub(super) fn collect(f: &TypedFunction, b: &mut Budget) -> CompileResult<Self> {
        // Actual ByteBufferBorrow admission always reconstructs its head
        // before the solver/window pass; an unrelated ordinary view does not.
        if !b.shared_byte_borrow || b.owned_byte_heads.is_none() {
            return Ok(Self {
                ids: BTreeSet::new(),
            });
        }
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
        state: &State,
        values: &[Value],
        active: &Loans,
        loans: &[Loan],
        apart: &mut BTreeSet<(Target, Target)>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        if self.ids.is_empty() {
            return Ok(());
        }
        // A callee may mutate a captured/explicit Desc without receiving the
        // view itself. No public replacement/payload effect is assumed here.
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
        ) && self.active(active, b)?
        {
            return Err(closed(i.span));
        }
        if let InstructionKind::BorrowLocal { local } | InstructionKind::BorrowLocalMut { local } =
            i.kind
        {
            ordered(b, state.locals.len(), 0)?;
            if let Some(value) = state.locals.get(&local)
                && self.carried(&value.pointer, loans, b)?
            {
                return Err(closed(i.span));
            }
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
            let actual = permission(module, f, buffer, i.span, values, b)?;
            if actual.targets.is_empty() {
                return Err(failure(
                    "byte view access has no independently grounded actual byte head",
                ));
            }
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
                    | InstructionKind::SliceLength { .. }
            ) || matches!(i.kind, InstructionKind::ReadAddress { address }
                    if matches!(value_type(f, address)?, Type::Address { mutable: false, inner } if inner.as_ref() == &Type::U8)
                        && i.result.is_some_and(|id| f.values.get(id.0 as usize) == Some(&Type::U8)))
                || matches!(i.kind, InstructionKind::Store { local, .. }
                    if matches!(&f.locals[local.0 as usize].ty, Type::Address { mutable: false, inner }
                        if inner.as_ref() == &Type::U8 || matches!(inner.as_ref(), Type::Slice { element } if element.as_ref() == &Type::U8)));
            if !allowed {
                return Err(closed(i.span));
            }
        }
        Ok(())
    }
}
