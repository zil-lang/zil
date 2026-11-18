//! Reconstruct the actual owned head behind a synchronous ByteBuffer payload
//! view. A conservative head pin does not identify header bytes as payload.
use super::*;
use zil_base::Span;

fn unavailable(span: Span) -> Diagnostic {
    unsupported(
        "stored byte view needs its actual owning head and a synchronous payload consumer",
        Some(span),
    )
}

#[derive(Clone, Copy)]
enum Head {
    Owner { owner: LocalId, field: Option<u32> },
    Pointer(ValueId),
    Flow,
}
pub(crate) struct Heads {
    functions: Vec<Vec<Option<Head>>>,
    flow: bool,
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
        let mut flow = false;
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
                    let operand = match operation.kind {
                        InstructionKind::ByteBufferBorrow { buffer, .. }
                        | InstructionKind::ByteBufferReserve { buffer, .. }
                        | InstructionKind::ByteBufferPush { buffer, .. }
                        | InstructionKind::ByteBufferTruncate { buffer, .. } => Some(buffer),
                        _ => None,
                    };
                    if let Some(operand) = operand {
                        flow |= !matches!(
                            sources.get(operand.0 as usize),
                            Some(Some(Head::Owner { .. } | Head::Pointer(_)))
                        );
                    }
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
                    if let InstructionKind::DescribeAddress { address } = operation.kind {
                        if !matches!(vt(f, address)?, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Managed(payload) if payload.as_ref() == &Type::ByteBuffer))
                        {
                            return Err(err(
                                "byte descriptor lacks its actual managed head address",
                            ));
                        }
                        sources[result.0 as usize] = Some(Head::Pointer(address));
                        continue;
                    }
                    sources[result.0 as usize] = Some(Head::Flow);
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
                        sources[result.0 as usize] = Some(Head::Owner { owner, field });
                    }
                }
            }
            functions.push(sources);
        }
        Ok(Self { functions, flow })
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

fn selection(
    module: &TypedModule,
    f: &TypedFunction,
    buffer: ValueId,
    span: Span,
    b: &mut Budget,
) -> Check<Head> {
    b.spend(4)?;
    if !matches!(vt(f, buffer)?, Type::Desc(payload) if payload.as_ref() == &Type::ByteBuffer) {
        return Err(err("byte view lacks an actual ByteBuffer description"));
    }
    let head = selected(module, f, buffer, b)?.ok_or_else(|| unavailable(span))?;
    b.spend(2)?;
    Ok(head)
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
    let InstructionKind::ByteBufferBorrow { buffer, mutable } = i.kind else {
        return Ok(false);
    };
    b.spend(4)?;
    let result = i.result.ok_or_else(|| err("byte payload view result is absent"))?;
    let valid = matches!(vt(f, result)?, Type::Address { mutable: mode, inner }
        if *mode == mutable && matches!(inner.as_ref(), Type::Slice { element } if element.as_ref() == &Type::U8));
    if !valid {
        return Err(err("byte payload view changes its actual access mode or byte slice type"));
    }
    selection(module, f, buffer, i.span, b)?;
    Ok(true)
}

fn actual_permission(
    module: &TypedModule,
    f: &TypedFunction,
    buffer: ValueId,
    span: Span,
    values: &[Payload],
    b: &mut Budget,
) -> Check<Permission> {
    match selection(module, f, buffer, span, b)? {
        Head::Owner { owner, field } => {
            let source = Source {
                static_dependency: StaticProvenance::NotStatic,
                retained_port: None,
                base: HoldRoot::Local(owner),
                leaf: Vec::new(),
                inline: field.into_iter().map(PlaceProjection::Field).collect(),
                heap_parents: Vec::new(),
                heap_element: None,
            };
            b.spend(width(&source).saturating_add(1))?;
            Ok(Permission {
                sources: vec![source],
                loans: Vec::new(),
            })
        }
        Head::Pointer(address) => {
            let actual = values
                .get(address.0 as usize)
                .ok_or_else(|| err("byte head pointer exceeds actual values"))?;
            let mut p = Permission::default();
            join_permission(&mut p, &actual.address, b)?;
            Ok(p)
        }
        Head::Flow => {
            let actual = values
                .get(buffer.0 as usize)
                .ok_or_else(|| err("byte descriptor exceeds actual values"))?;
            let mut p = Permission::default();
            join_permission(&mut p, &actual.store_description, b)?;
            Ok(p)
        }
    }
}

pub(super) fn transfer(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    values: &[Payload],
    b: &mut Budget,
) -> Check<Permission> {
    let InstructionKind::ByteBufferBorrow { buffer, .. } = i.kind
    else {
        return Err(unavailable(i.span));
    };
    // Reconstruct the real source domain, including heap element descendants,
    // and retain every delegated parent. No local is fabricated for a pointer.
    actual_permission(module, f, buffer, i.span, values, b)
}

pub(super) fn seed_description(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    values: &[Payload],
    b: &mut Budget,
) -> Check<Option<Permission>> {
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
    if !matches!(vt(f,result)?,Type::Desc(payload) if payload.as_ref()==&Type::ByteBuffer) {
        return Ok(None);
    }
    if matches!(selection(module, f, result, i.span, b)?, Head::Flow) {
        // The step has just published this descriptor's independently formed
        // local/field root. Reading the old SSA result at the fixed-point
        // bottom would overwrite that new producer with an empty permission.
        return Ok(None);
    }
    Ok(Some(actual_permission(
        module, f, result, i.span, values, b,
    )?))
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
    // The inventory has authenticated the unique sealed ByteBuffer resource
    // and every actual view operation. Resource and environment qualification
    // belong to their complete independent consumers, rather than a second
    // module-wide restriction to one resource and no owning captures here.
    // In particular, a captured manager is not a view permission: Flow below
    // still needs the actual descriptor, and an escaping captured view must
    // retain its independently established receiver parent.
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
    if anonymous_tuple::layout(ty, b)?.is_some() {
        // This is not the local-byte-family value route. The actual anonymous
        // carrier has its own complete physical, loan and resource proof.
        return Ok(false);
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
            // This narrow value classifier does not own an arbitrary nested
            // carrier. Let the complete finite-value/owner classifier prove
            // that carrier; a failed match here must not veto its proof.
            Type::ByteBuffer => return Ok(false),
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

/// A direct body call already checks each exact address argument and exports
/// its actual returned sources through the common call/loan kernel. This
/// admission is limited to its address operand, never the enclosing payload.
fn direct_address_argument(
    module: &TypedModule,
    caller: &TypedFunction,
    operation: &InstructionKind,
    actual: ValueId,
    b: &mut Budget,
) -> Check<bool> {
    let InstructionKind::Call { function, arguments } = operation else { return Ok(false); };
    let target = module.functions.get(function.0 as usize)
        .filter(|target| target.id == *function && target.call_kind == CallKind::Direct
            && target.environment.is_none() && target.foreign.is_none())
        .ok_or_else(|| err("byte view direct transfer has no exact ordinary callee"))?;
    let ty = vt(caller, actual)?;
    if !matches!(ty, Type::Address { .. }) || arguments.len() != target.parameters.len() {
        return Ok(false);
    }
    b.spend(arguments.len())?;
    let mut found = false;
    for (arg, local) in arguments.iter().zip(&target.parameters) {
        if *arg == actual {
            if target.locals.get(local.0 as usize).is_none_or(|local| &local.ty != ty) {
                return Ok(false);
            }
            found = true;
        }
    }
    Ok(found)
}

pub(super) struct Sites {
    values: Vec<usize>,
}
impl Sites {
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
    fn boundary_permission(
        &self,
        p: &Permission,
        live: &[usize],
        loans: &[Loan],
        requirements: &mut Vec<(Source, Source)>,
        span: Span,
        b: &mut Budget,
    ) -> Check<()> {
        if p.sources.is_empty() {
            if !p.loans.is_empty() {
                return Err(err(
                    "byte view boundary retains a loan without its actual target",
                ));
            }
            return Ok(());
        }
        if self.carried(p, loans, b)? {
            return Err(reject(
                "E-VERIFY-STORED",
                Some(span),
                "live byte payload view cannot escape through an actual call or capture boundary",
            ));
        }
        // A boundary with a disjoint input cannot reach this view's manager,
        // even if its complete callee body mutates or consumes that input.
        // This is separation against the actual byte loans, not permission to
        // write a shared input or to bypass unrelated live child loans.
        access_head(p, false, b)?;
        let delegated = ancestry(&p.loans, loans, b)?;
        access_tail(p, true, live, loans, &delegated, requirements, b)
    }
    fn boundary_payload(
        &self,
        value: &Payload,
        live: &[usize],
        loans: &[Loan],
        requirements: &mut Vec<(Source, Source)>,
        span: Span,
        b: &mut Budget,
    ) -> Check<()> {
        value_transport::visit_fields(value, b, |permission, b| {
            self.boundary_permission(permission, live, loans, requirements, span, b)
        })?;
        self.boundary_permission(&value.address, live, loans, requirements, span, b)?;
        self.boundary_permission(&value.store_description, live, loans, requirements, span, b)?;
        b.spend(
            value
                .leaves
                .len()
                .saturating_add(value.task_ports.len())
                .saturating_add(value.dyn_ports.0.len())
                .saturating_add(value.dyn_views.len()),
        )?;
        for (_, p) in &value.leaves {
            self.boundary_permission(p, live, loans, requirements, span, b)?;
        }
        for (_, p) in &value.task_ports {
            self.boundary_permission(p, live, loans, requirements, span, b)?;
        }
        for (_, port) in &value.dyn_ports.0 {
            if !port.known {
                return Err(err("byte view boundary has an unknown actual opaque port"));
            }
            self.boundary_permission(&port.permission, live, loans, requirements, span, b)?;
            for source in port.parent_sources.iter().chain(&port.head_sources) {
                b.spend(width(source).saturating_add(1))?;
                self.boundary_permission(
                    &Permission {
                        sources: vec![source.clone()],
                        loans: vec![],
                    },
                    live,
                    loans,
                    requirements,
                    span,
                    b,
                )?;
            }
        }
        for view in &value.dyn_views {
            self.boundary_permission(&view.head, live, loans, requirements, span, b)?;
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn independent_boundary(
        &self,
        module: &TypedModule,
        f: &TypedFunction,
        i: &Instruction,
        state: &Frame,
        values: &[Payload],
        active: &[usize],
        loans: &[Loan],
        requirements: &mut Vec<(Source, Source)>,
        b: &mut Budget,
    ) -> Check<bool> {
        let local_receiver = match i.kind {
            InstructionKind::CallClosureShared { callee, .. }
            | InstructionKind::CallClosureMut { callee, .. } => Some(callee),
            _ => None,
        };
        let supported = match &i.kind {
            InstructionKind::MakeClosure { closure, captures } => {
                let definition = crate::closures::definition(module, *closure, Some(i.span))?;
                if definition.captures.len() != captures.len() {
                    return Err(err("byte view creation boundary omits an actual capture"));
                }
                true
            }
            InstructionKind::Call { function, .. }
            | InstructionKind::SuspendCall { function, .. } => {
                let target = module
                    .functions
                    .get(function.0 as usize)
                    .filter(|row| row.id == *function)
                    .ok_or_else(|| err("byte view call boundary has no exact actual function"))?;
                target.foreign.is_none() && target.environment.is_none()
            }
            InstructionKind::CallIndirect { .. } => true,
            InstructionKind::CallClosure { .. }
            | InstructionKind::CallClosureBorrowed { .. }
            | InstructionKind::CallClosureShared { .. }
            | InstructionKind::CallClosureMut { .. }
            | InstructionKind::CallClosureOnce { .. } => {
                stored_callables::called(module, f, &i.kind).is_some()
            }
            _ => false,
        };
        if !supported {
            return Ok(false);
        }
        b.spend(active.len())?;
        let mut live = Vec::new();
        for loan in active {
            if self.active(&ancestry(std::slice::from_ref(loan), loans, b)?, b)? {
                b.spend(1)?;
                live.push(*loan);
            }
        }
        if let Some(local) = local_receiver {
            let value = state
                .locals
                .get(local.0 as usize)
                .ok_or_else(|| err("byte view callable receiver local is absent"))?;
            self.boundary_payload(value, &live, loans, requirements, i.span, b)?;
            let p = Permission {
                sources: vec![Source {
                    static_dependency: StaticProvenance::NotStatic,
                    retained_port: None,
                    base: HoldRoot::Local(local),
                    leaf: vec![],
                    inline: vec![],
                    heap_parents: vec![],
                    heap_element: None,
                }],
                loans: vec![],
            };
            self.boundary_permission(&p, &live, loans, requirements, i.span, b)?;
        }
        for input in instruction_operands(&i.kind) {
            b.spend(1)?;
            let value = values
                .get(input.0 as usize)
                .ok_or_else(|| err("byte view boundary input is absent"))?;
            let ty = vt(f, input)?;
            if matches!(ty, Type::Address { .. } | Type::Desc(_))
                && value.address.sources.is_empty()
                && value.store_description.sources.is_empty()
            {
                return Err(err(
                    "byte view boundary input has no complete actual pointer/description source",
                ));
            }
            if self.carried(&value.address, loans, b)?
                && direct_address_argument(module, f, &i.kind, input, b)?
            {
                continue;
            }
            self.boundary_payload(value, &live, loans, requirements, i.span, b)?;
            if let Type::Address { inner, .. } = ty
                && let Some(id) = returned_environment::closure(inner)
            {
                for (path, _) in returned_environment::leaves(module, id, b)? {
                    let p = select(state, &value.address, &path, b)?;
                    if p.sources.is_empty() {
                        return Err(err(
                            "byte view boundary borrowed receiver omits an actual environment leaf",
                        ));
                    }
                    self.boundary_permission(&p, &live, loans, requirements, i.span, b)?;
                }
            }
        }
        Ok(true)
    }
    /// Export only independently reconstructed loan ancestry, never candidate
    /// claims or matching result types, to the ordinary physical-root adapter.
    pub(super) fn payload_values(
        &self,
        f: &TypedFunction,
        values: &[Payload],
        loans: &[Loan],
        b: &mut Budget,
    ) -> Check<Vec<ValueId>> {
        if self.values.is_empty() {
            return Ok(Vec::new());
        }
        b.spend(values.len())?;
        let mut result = Vec::new();
        for (index, value) in values.iter().enumerate() {
            if !matches!(&f.values[index], Type::Address { mutable: false, inner }
                if inner.as_ref() == &Type::U8
                    || matches!(inner.as_ref(), Type::Slice { element } if element.as_ref() == &Type::U8))
            {
                continue;
            }
            if self.carried(&value.address, loans, b)? {
                b.spend(1)?;
                result.push(ValueId(index as u32));
            }
        }
        Ok(result)
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
    ) -> Check<Option<(Permission, bool)>> {
        let byte_access = if b.shared_byte_borrow {
            match i.kind {
                InstructionKind::ByteBufferBorrow { buffer, mutable } => Some((buffer, mutable)),
                InstructionKind::ByteBufferReserve { buffer, .. }
                | InstructionKind::ByteBufferPush { buffer, .. }
                | InstructionKind::ByteBufferTruncate { buffer, .. } => Some((buffer, true)),
                _ => None,
            }
        } else {
            None
        };
        let access_effect = if let Some((buffer, write)) = byte_access {
            let permission = actual_permission(module, f, buffer, i.span, values, b)?;
            if permission.sources.is_empty() {
                return Err(err("byte operation has no independently grounded head source"));
            }
            access(&permission, write, live, loans, requirements, b)?;
            Some((permission, write))
        } else {
            None
        };
        if self.values.is_empty() {
            return Ok(access_effect);
        }
        // The ordinary C proof checks the complete ABI, exact noescape vector
        // and scalar result. Its access consumer separately checks each actual
        // pointer, loan ancestry and simultaneous mutable argument. A live byte
        // view may cross precisely that call without becoming an escaping root.
        let noescape_call = matches!(i.kind, InstructionKind::ForeignCall { .. })
            && foreign_calls::operation(module, f, i, b)?;
        if matches!(
            i.kind,
            InstructionKind::Call { .. }
                | InstructionKind::SuspendCall { .. }
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
        ) && !noescape_call
            && self.active(live, b)?
            && !self.independent_boundary(
                module,
                f,
                i,
                state,
                values,
                live,
                loans,
                requirements,
                b,
            )?
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
        if let InstructionKind::DropManaged { local } | InstructionKind::DropPackage { local } =
            i.kind
            && self.active(live, b)?
        {
            b.spend(1)?;
            let p = Permission {
                sources: vec![Source {
                    static_dependency: StaticProvenance::NotStatic,
                    retained_port: None,
                    base: HoldRoot::Local(local),
                    leaf: Vec::new(),
                    inline: Vec::new(),
                    heap_parents: Vec::new(),
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
                matches!(&f.locals[local.0 as usize].ty, Type::Address { inner, .. }
                    if inner.as_ref() == &Type::U8 || matches!(inner.as_ref(), Type::Slice { element } if element.as_ref() == &Type::U8))
            } else {
                false
            };
            if !ordinary_local
                && !noescape_call
                && !direct_address_argument(module, f, &i.kind, input, b)?
                && !matches!(
                    i.kind,
                    InstructionKind::AddressIndex { .. }
                        | InstructionKind::AddressSlice { .. }
                        | InstructionKind::ReborrowAddress { .. }
                        | InstructionKind::SliceLength { .. }
                )
                && !matches!(i.kind, InstructionKind::ReadAddress { address }
                    if matches!(vt(f, address)?, Type::Address { inner, .. } if inner.as_ref() == &Type::U8)
                        && i.result.is_some_and(|id| f.values.get(id.0 as usize) == Some(&Type::U8)))
                && !matches!(i.kind, InstructionKind::WriteAddress { address, value }
                    if matches!(vt(f, address)?, Type::Address { mutable: true, inner } if inner.as_ref() == &Type::U8)
                        && vt(f, value)? == &Type::U8)
            {
                return Err(unavailable(i.span));
            }
        }
        Ok(access_effect)
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;
    fn source(local: u32) -> Source {
        Source {
            static_dependency: StaticProvenance::NotStatic,
            retained_port: None,
            base: HoldRoot::Local(LocalId(local)),
            leaf: vec![],
            inline: vec![],
            heap_parents: vec![],
            heap_element: None,
        }
    }
    fn permission(local: u32) -> Permission {
        Permission {
            sources: vec![source(local)],
            loans: vec![1],
        }
    }
    // This is only the independent boundary-separation sub-proof. Complete
    // Source/callee/CFG/ownership admission is exercised by the real consumers.
    #[test]
    fn byte_view_boundaries_preserve_all_actual_carrier_paths_and_parent_loans() {
        let sites = Sites { values: vec![0] };
        let mut loans = vec![
            Loan {
                sources: vec![source(0)],
                ..Loan::default()
            },
            Loan {
                sources: vec![source(1)],
                ..Loan::default()
            },
        ];
        for route in 0..8 {
            for alias in [false, true] {
                let mut b = Budget::new(&Limits::default());
                let p = permission(if alias { 0 } else { 1 });
                let mut value = Payload::default();
                match route {
                    0 => value.address = p,
                    1 => value.store_description = p,
                    2 => value
                        .leaves
                        .push((vec![PlaceProjection::Field(0)], p), &mut b)
                        .unwrap(),
                    3 => value
                        .task_ports
                        .push((vec![ContractProjection::Capture(0)], p)),
                    4 => value.dyn_ports.0.push((
                        vec![],
                        dyn_ports::Port {
                            known: true,
                            permission: p,
                            ..dyn_ports::Port::default()
                        },
                    )),
                    5 => value.dyn_ports.0.push((
                        vec![],
                        dyn_ports::Port {
                            known: true,
                            parent_sources: p.sources,
                            ..dyn_ports::Port::default()
                        },
                    )),
                    6 => value.dyn_ports.0.push((
                        vec![],
                        dyn_ports::Port {
                            known: true,
                            head_sources: p.sources,
                            ..dyn_ports::Port::default()
                        },
                    )),
                    _ => value.dyn_views.push(dyn_ports::View {
                        table: None,
                        head: p,
                    }),
                }
                let result = sites.boundary_payload(
                    &value,
                    &[0],
                    &loans,
                    &mut Vec::new(),
                    Span::default(),
                    &mut b,
                );
                if alias {
                    assert_eq!(
                        result.unwrap_err().category,
                        zil_base::ResultCategory::SafeReject,
                        "route {route}"
                    );
                } else {
                    result.unwrap();
                }
            }
        }
        // Passing an actual descendant of the view is forbidden even when a
        // purported separate root attempts to hide that delegated ancestry.
        loans[1].parents = vec![0];
        let mut b = Budget::new(&Limits::default());
        assert_eq!(
            sites
                .boundary_permission(
                    &permission(1),
                    &[0],
                    &loans,
                    &mut Vec::new(),
                    Span::default(),
                    &mut b
                )
                .unwrap_err()
                .category,
            zil_base::ResultCategory::SafeReject
        );
        let mut unknown = Payload::default();
        unknown
            .dyn_ports
            .0
            .push((vec![], dyn_ports::Port::default()));
        assert!(
            sites
                .boundary_payload(
                    &unknown,
                    &[0],
                    &loans,
                    &mut Vec::new(),
                    Span::default(),
                    &mut b
                )
                .unwrap_err()
                .message
                .contains("unknown actual opaque port")
        );
        assert!(
            sites
                .boundary_permission(
                    &Permission {
                        sources: vec![],
                        loans: vec![1]
                    },
                    &[0],
                    &loans,
                    &mut Vec::new(),
                    Span::default(),
                    &mut b
                )
                .is_err()
        );
    }
}

#[cfg(test)]
mod captured_flow_tests {
    use super::*;

    fn fixture(kind: CallableKind) -> (TypedModule, CandidatePlan) {
        let (mut module, plan) = crate::tests::baseline();
        let description = Type::Desc(Box::new(Type::ByteBuffer));
        let result = Type::Address {
            mutable: false,
            inner: Box::new(Type::Slice {
                element: Box::new(Type::U8),
            }),
        };
        let relation = StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: 1,
            bindings: [StorageBase::Receiver, StorageBase::Result]
                .into_iter()
                .map(|base| StorageLifeBinding {
                    position: StoragePosition { base, path: vec![] },
                    binder: StorageBinder::Invocation(0),
                })
                .collect(),
            classes: vec![],
            static_bindings: vec![],
        };
        let f = &mut module.functions[0];
        f.environment = Some(ClosureId(0));
        f.storage_life = Some(relation.clone());
        f.return_type = result.clone();
        f.locals[0].ty = description.clone();
        f.values = vec![description.clone(), description, result.clone(), Type::I64];
        let span = f.span;
        f.blocks[0].instructions = vec![
            Instruction {
                result: Some(ValueId(0)),
                kind: InstructionKind::DescribeCapture { index: 0 },
                span,
            },
            Instruction {
                result: None,
                kind: InstructionKind::Store {
                    local: LocalId(0),
                    value: ValueId(0),
                },
                span,
            },
            Instruction {
                result: Some(ValueId(1)),
                kind: InstructionKind::Load(LocalId(0)),
                span,
            },
            Instruction {
                result: Some(ValueId(2)),
                kind: InstructionKind::ByteBufferBorrow {
                    buffer: ValueId(1),
                    mutable: false,
                },
                span,
            },
            Instruction {
                result: Some(ValueId(3)),
                kind: InstructionKind::Constant(Constant::I64(4)),
                span,
            },
        ];
        f.blocks[0].terminator = Terminator::Return(Some(ValueId(2)));
        module.closures.push(ClosureDefinition {
            id: ClosureId(0),
            function: FunctionId(0),
            captures: vec![
                Type::Managed(Box::new(Type::ByteBuffer)),
                Type::Managed(Box::new(Type::ByteBuffer)),
            ],
            parameters: vec![],
            result,
            kind,
            storage_life: Some(relation),
            lending_captures: vec![0],
        });
        module.resources = vec![
            zil_ir::ResourceDefinition {
                name: "@builtin:ByteBuffer".into(),
                payload: Type::ByteBuffer,
                teardown: None,
            },
            zil_ir::ResourceDefinition {
                name: "OtherOwner".into(),
                payload: Type::I32,
                teardown: None,
            },
        ];
        (module, plan)
    }
    fn source(index: u32) -> Source {
        Source {
            static_dependency: StaticProvenance::NotStatic,
            retained_port: None,
            base: HoldRoot::Capture(index),
            leaf: vec![],
            inline: vec![],
            heap_parents: vec![],
            heap_element: None,
        }
    }

    // This tests the descriptor-flow and access components, not complete raw
    // closure admission. Full Source execution separately checks the receiver,
    // creator, initialization, result relation and ownership retirement.
    #[test]
    fn captured_byte_flow_preserves_sources_parents_and_growth_conflicts_raw_wire() {
        let limits = Limits::default();
        for kind in [CallableKind::Fn, CallableKind::FnMut] {
            let (module, plan) = fixture(kind);
            let encoded = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
            let decoded = zil_ir::wire::decode_module_candidate(&encoded, &limits)
                .unwrap()
                .0;
            for module in [&module, &decoded] {
                let mut b = Budget::new(&limits);
                b.shared_byte_borrow = true;
                assert!(profile(module, &mut b).unwrap());
                let f = &module.functions[0];
                let sites = Sites::collect(f, &mut b).unwrap();
                let mut values = vec![Payload::default(); f.values.len()];
                values[1].store_description = Permission {
                    sources: vec![source(0)],
                    loans: vec![0],
                };
                let borrow = &f.blocks[0].instructions[3];
                let view = transfer(module, f, borrow, &values, &mut b).unwrap();
                assert_eq!(view.sources, vec![source(0)]);
                assert_eq!(view.loans, vec![0]);
                let mut loans = vec![Loan::default(); f.values.len()];
                loans[0].sources = vec![source(0)];
                loans[2].sources = vec![source(0)];
                loans[2].parents = vec![0];
                values[2].address = view;
                values[2].address.loans.push(2);
                // Repeated shared views do not mutate or relocate the manager.
                sites
                    .check(
                        module,
                        f,
                        borrow,
                        &Frame::default(),
                        &values,
                        &[2],
                        &loans,
                        &mut vec![],
                        &mut b,
                    )
                    .unwrap();
                let growth = Instruction {
                    result: None,
                    kind: InstructionKind::ByteBufferReserve {
                        buffer: ValueId(1),
                        capacity: ValueId(3),
                    },
                    span: f.span,
                };
                assert_eq!(
                    sites
                        .check(
                            module,
                            f,
                            &growth,
                            &Frame::default(),
                            &values,
                            &[2],
                            &loans,
                            &mut vec![],
                            &mut b
                        )
                        .unwrap_err()
                        .category,
                    zil_base::ResultCategory::SafeReject
                );
                // Once the old view is dead, the same manager may grow.
                sites
                    .check(
                        module,
                        f,
                        &growth,
                        &Frame::default(),
                        &values,
                        &[],
                        &loans,
                        &mut vec![],
                        &mut b,
                    )
                    .unwrap();
                // The descriptor's full actual source, not its same-shaped
                // capture type, determines whether a live view aliases it.
                values[1].store_description = Permission {
                    sources: vec![source(1)],
                    loans: vec![],
                };
                sites
                    .check(
                        module,
                        f,
                        &growth,
                        &Frame::default(),
                        &values,
                        &[2],
                        &loans,
                        &mut vec![],
                        &mut b,
                    )
                    .unwrap();
                values[1].store_description = Permission::default();
                assert!(
                    sites
                        .check(
                            module,
                            f,
                            borrow,
                            &Frame::default(),
                            &values,
                            &[],
                            &loans,
                            &mut vec![],
                            &mut b
                        )
                        .unwrap_err()
                        .message
                        .contains("independently grounded head")
                );
            }
        }
    }

    #[test]
    fn byte_profile_still_requires_the_unique_sealed_resource_and_typed_descriptor() {
        let limits = Limits::default();
        for attack in 0..4 {
            let (mut module, _) = fixture(CallableKind::Fn);
            match attack {
                0 => {
                    module.resources.remove(0);
                }
                1 => {
                    module.resources.push(module.resources[0].clone());
                }
                2 => {
                    module.resources[0].name = "unsealed".into();
                }
                _ => {
                    module.functions[0].values[1] = Type::I32;
                }
            }
            let mut b = Budget::new(&limits);
            b.shared_byte_borrow = true;
            assert!(profile(&module, &mut b).is_err(), "attack {attack}");
        }
        let mut b = Budget::new(&limits);
        assert!(
            !value(
                &Type::Address {
                    mutable: true,
                    inner: Box::new(Type::Managed(Box::new(Type::ByteBuffer)))
                },
                &mut b
            )
            .unwrap(),
            "nested owners must use their complete independent qualifier"
        );
    }
}
