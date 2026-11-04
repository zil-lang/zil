//! Candidate shared-address provenance. The verifier has its own implementation.
//! Only actual Hold relations are stored; unrelated locals are absent from CFG
//! states and roots are introduced only by executable borrow/capture operations.
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Root {
    Local(LocalId),
    Field(LocalId, u32),
    Projection(LocalId, u32, u32),
    Window(HoldRoot,u32,u32),
    Capture(u32),
    Parameter(u32),
    // A borrowed environment keeps these positions alive, but its pointer
    // addresses the environment itself. They are not additional pointees.
    RetainedLocal(LocalId),
    RetainedField(LocalId, u32),
    RetainedProjection(LocalId, u32, u32),
    RetainedWindow(HoldRoot,u32,u32),
    RetainedCapture(u32),
    RetainedParameter(u32),
}

impl Root {
    fn position(self) -> Self {
        match self {
            Self::RetainedLocal(local) => Self::Local(local),
            Self::RetainedField(local, index) => Self::Field(local, index),
            Self::RetainedProjection(local, start, end) => Self::Projection(local, start, end),
            Self::RetainedWindow(root,start,end)=>Self::Window(root,start,end),
            Self::RetainedCapture(index) => Self::Capture(index),
            Self::RetainedParameter(index) => Self::Parameter(index),
            root => root,
        }
    }
    fn retained(self) -> Self {
        match self.position() {
            Self::Local(local) => Self::RetainedLocal(local),
            Self::Field(local, index) => Self::RetainedField(local, index),
            Self::Projection(local, start, end) => Self::RetainedProjection(local, start, end),
            Self::Window(root,start,end)=>Self::RetainedWindow(root,start,end),
            Self::Capture(index) => Self::RetainedCapture(index),
            Self::Parameter(index) => Self::RetainedParameter(index),
            _ => unreachable!(),
        }
    }
}

type Roots = BTreeSet<Root>;
type State = BTreeMap<LocalId, Roots>;

// This is a private access map, not a permission in ValueHold. Keeping it
// separate preserves which actual capture carried exclusive authority: a
// closure sharing x while mutating y must not make the x root exclusive.
fn thread_exclusive_type(
    module: &TypedModule,
    ty: &Type,
    budget: &mut Budget,
) -> CompileResult<bool> {
    budget.spend(1, "thread capture access traversal allocation")?;
    let mut pending = vec![ty];
    let mut seen = BTreeSet::new();
    while let Some(ty) = pending.pop() {
        budget.spend(1, "thread capture access shape")?;
        match ty {
            Type::Address { mutable: true, .. } => return Ok(true),
            Type::Closure(id) => {
                budget.lookup(seen.len(), "thread nested capture access lookup")?;
                if seen.insert(*id) {
                    let definition = module
                        .closures
                        .get(id.0 as usize)
                        .ok_or_else(|| bad("thread captured closure is absent"))?;
                    budget.spend(
                        definition.captures.len().saturating_add(1),
                        "thread nested capture access allocation",
                    )?;
                    pending.extend(&definition.captures);
                }
            }
            _ => {}
        }
    }
    Ok(false)
}

fn thread_access_step(
    module: &TypedModule,
    f: &TypedFunction,
    instruction: &Instruction,
    values: &[Roots],
    exclusive: &mut [Roots],
    state: &mut State,
    budget: &mut Budget,
) -> CompileResult<bool> {
    budget.spend(1, "thread per-capture access transfer")?;
    let mut found = Roots::new();
    match &instruction.kind {
        InstructionKind::MakeClosure { captures, .. } => {
            for value in captures {
                budget.spend(1, "thread actual capture access")?;
                let ty = &f.values[value.0 as usize];
                if matches!(ty, Type::Address { mutable: true, .. } | Type::Closure(_)) {
                    extend(&mut found, &exclusive[value.0 as usize], budget)?;
                }
            }
        }
        InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
            budget.lookup(state.len(), "thread actual local access lookup")?;
            if let Some(roots) = state.get(local) {
                extend(&mut found, roots, budget)?;
            }
            if matches!(instruction.kind, InstructionKind::TakeLocal { .. }) {
                state.remove(local);
            }
        }
        InstructionKind::Store { local, value } => {
            budget.lookup(state.len(), "thread access local store")?;
            budget.spend(
                exclusive[value.0 as usize].len().saturating_add(1),
                "thread access local copy",
            )?;
            if exclusive[value.0 as usize].is_empty() {
                state.remove(local);
            } else {
                state.insert(*local, exclusive[value.0 as usize].clone());
            }
        }
        InstructionKind::StorageLive(local)
        | InstructionKind::StorageDead(local)
        | InstructionKind::DropClosure { local } => {
            budget.lookup(state.len(), "thread access local activation change")?;
            state.remove(local);
        }
        InstructionKind::ReadCapture { index }
        | InstructionKind::TakeCapture { index }
        | InstructionKind::BorrowCapture { index, .. } => {
            if let Some(value) = instruction.result
                && thread_exclusive_type(module, &f.values[value.0 as usize], budget)?
            {
                budget.spend(1, "thread actual capture exclusive root")?;
                found.insert(Root::Capture(*index));
            }
        }
        InstructionKind::BorrowLocalMut { local } => {
            if matches!(f.locals[local.0 as usize].ty, Type::Closure(_)) {
                budget.spend(1, "thread exclusive environment position")?;
                found.insert(Root::Local(*local));
                if let Some(held) = state.get(local) {
                    extend(&mut found, held, budget)?;
                }
            } else if let Some(value) = instruction.result {
                extend(&mut found, &values[value.0 as usize], budget)?;
            }
        }
        InstructionKind::ReborrowAddress {
            address,
            mutable: true,
        } => {
            extend(&mut found, &exclusive[address.0 as usize], budget)?;
        }
        _ => {}
    }
    if let Some(value) = instruction.result {
        // Ordinary mutable address producers (including direct lending
        // helpers and mutable projections) all retain their independently
        // inferred pointee roots. An environment position is special: its
        // retained shared pointees are not writable just because it is &mut.
        if matches!(&f.values[value.0 as usize], Type::Address { mutable: true, inner } if !matches!(inner.as_ref(), Type::Closure(_)))
        {
            extend(&mut found, &values[value.0 as usize], budget)?;
        }
        extend(&mut exclusive[value.0 as usize], &found, budget)
    } else {
        Ok(false)
    }
}

fn shared_thread_window(
    module: &TypedModule,
    f: &TypedFunction,
    instruction: &Instruction,
    values: &[Roots],
    exclusive: &[Roots],
    budget: &mut Budget,
) -> CompileResult<()> {
    let InstructionKind::WithScopedThread { argument, body, .. } = instruction.kind else {
        return Ok(());
    };
    if !matches!(&f.values[argument.0 as usize], Type::Address { mutable: false, inner } if **inner == Type::I32)
    {
        return Ok(());
    }
    let roots = &values[argument.0 as usize];
    if roots.is_empty() {
        return Err(bad("shared thread payload has no actual storage origin"));
    }
    for root in roots {
        budget.spend(1, "shared thread stable storage qualification")?;
        let ty = match root {
            Root::Local(local) => f.locals.get(local.0 as usize).map(|local| &local.ty),
            Root::Parameter(index) => f
                .parameters
                .get(*index as usize)
                .and_then(|local| f.locals.get(local.0 as usize))
                .map(|local| &local.ty),
            Root::Capture(index) => f
                .environment
                .and_then(|id| module.closures.get(id.0 as usize))
                .and_then(|definition| definition.captures.get(*index as usize)),
            _ => None,
        };
        let supported = match root {
            Root::Local(_) => ty == Some(&Type::I32),
            Root::Parameter(_) => {
                matches!(ty, Some(Type::Address { inner, .. }) if **inner == Type::I32)
            }
            Root::Capture(_) => {
                ty == Some(&Type::I32)
                    || matches!(ty, Some(Type::Address { inner, .. }) if **inner == Type::I32)
            }
            _ => false,
        };
        if !supported {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "shared thread dynamic, lending or aggregate payload storage is not implemented",
                Some(instruction.span),
            )]);
        }
    }
    if overlaps(roots, &exclusive[body.0 as usize], budget)? {
        return Err(window_conflict(
            instruction,
            "shared child payload overlaps an exclusive body capture, including nested captures",
        ));
    }
    Ok(())
}

fn window_conflict(instruction: &Instruction, message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Analyze,
        "E-BORROW-CONFLICT",
        Some(instruction.span),
        message,
    )]
}

// Only ancestors of this actual reborrow may remain live across its child.
// Equal roots or equal ClosureId alone never establish delegated permission.
fn delegated(
    f: &TypedFunction,
    callback: ValueId,
    active: resources::Place,
    budget: &mut Budget,
) -> CompileResult<bool> {
    use resources::Place;
    let mut pending = vec![callback];
    let mut seen = BTreeSet::new();
    while let Some(value) = pending.pop() {
        budget.lookup(seen.len(), "scoped delegation identity")?;
        if !seen.insert(value) {
            continue;
        }
        if active == Place::Value(value) {
            return Ok(true);
        }
        for block in &f.blocks {
            budget.spend(
                block.instructions.len().saturating_add(1),
                "scoped actual delegation producer",
            )?;
            for instruction in &block.instructions {
                if instruction.result != Some(value) {
                    continue;
                }
                match instruction.kind {
                    InstructionKind::ReborrowAddress { address, .. }
                    | InstructionKind::AddressField { address, .. }
                    | InstructionKind::AddressIndex { address, .. }
                    | InstructionKind::AddressSlice { address, .. } => pending.push(address),
                    InstructionKind::Load(local) | InstructionKind::TakeLocal { local }
                        if active == Place::Local(local) =>
                    {
                        return Ok(true);
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(false)
}

fn root_overlap(a: Root, b: Root) -> bool {
    match (a.position(), b.position()) {
        (Root::Window(x,a,b),Root::Window(y,c,d))=>x==y && a<d && c<b,
        (Root::Window(HoldRoot::Local(x),a,b),Root::Projection(y,c,d))|(Root::Projection(y,c,d),Root::Window(HoldRoot::Local(x),a,b))=>x==y && a<d && c<b,
        (Root::Window(root,..),other)|(other,Root::Window(root,..))=>match(root,other){(HoldRoot::Local(x),Root::Local(y)|Root::Field(y,_)|Root::Projection(y,..))=>x==y,(HoldRoot::Parameter(x),Root::Parameter(y))=>x==y,(HoldRoot::Capture(x),Root::Capture(y))=>x==y,_=>false},
        (Root::Projection(x, a, b), Root::Projection(y, c, d)) => x == y && a < d && c < b,
        (Root::Projection(x, ..), Root::Local(y) | Root::Field(y, _))
        | (Root::Local(x) | Root::Field(x, _), Root::Projection(y, ..)) => x == y,
        (a, b) => {
            a == b
                || matches!((a,b),(Root::Local(x),Root::Field(y,_))|(Root::Field(x,_),Root::Local(y)) if x==y)
        }
    }
}
fn overlaps(a: &Roots, b: &Roots, budget: &mut Budget) -> CompileResult<bool> {
    budget.product(a.len(), b.len(), 1, "actual storage interval overlap")?;
    Ok(a.iter().any(|a| b.iter().any(|b| root_overlap(*a, *b))))
}
fn variant_root(
    f: &TypedFunction,
    local: LocalId,
    path: &[VariantProjection],
    budget: &mut Budget,
) -> CompileResult<Root> {
    fn size(t: &Type, b: &mut Budget) -> CompileResult<u32> {
        b.spend(1, "payload interval node")?;
        let mut total = 1u32;
        let mut add = |child: &Type| -> CompileResult<()> {
            total = total.checked_add(size(child, b)?).ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Analyze,
                    "payload interval overflow",
                    None,
                )]
            })?;
            Ok(())
        };
        match t {
            Type::Tuple(fs) => {
                for t in fs {
                    add(t)?
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                for (_, t) in fields {
                    add(t)?
                }
            }
            Type::Enum { variants, .. } => {
                for (_, fs) in variants {
                    for t in fs {
                        add(t)?
                    }
                }
            }
            _ => {}
        }
        Ok(total)
    }
    let mut ty = &f.locals[local.0 as usize].ty;
    let mut offset = 0u32;
    for part in path {
        let Type::Enum { variants, .. } = ty else {
            return Err(bad("payload interval crosses non-Enum"));
        };
        offset = offset.checked_add(1).ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Analyze,
                "payload interval overflow",
                None,
            )]
        })?;
        for (_, fs) in variants.iter().take(part.variant as usize) {
            for t in fs {
                offset = offset.checked_add(size(t, budget)?).ok_or_else(|| {
                    vec![Diagnostic::limit(
                        Phase::Analyze,
                        "payload interval overflow",
                        None,
                    )]
                })?;
            }
        }
        let fs = &variants
            .get(part.variant as usize)
            .ok_or_else(|| bad("payload interval tag outside declaration"))?
            .1;
        for t in fs.iter().take(part.index as usize) {
            offset = offset.checked_add(size(t, budget)?).ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Analyze,
                    "payload interval overflow",
                    None,
                )]
            })?;
        }
        ty = fs
            .get(part.index as usize)
            .ok_or_else(|| bad("payload interval field outside declaration"))?;
    }
    let end = offset.checked_add(size(ty, budget)?).ok_or_else(|| {
        vec![Diagnostic::limit(
            Phase::Analyze,
            "payload interval overflow",
            None,
        )]
    })?;
    Ok(if path.is_empty() {
        Root::Local(local)
    } else {
        Root::Projection(local, offset, end)
    })
}

fn scoped_captures_address(
    module: &TypedModule,
    ty: &Type,
    budget: &mut Budget,
) -> CompileResult<bool> {
    let Type::Closure(id) = ty else {
        return Ok(false);
    };
    let mut pending = vec![*id];
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        budget.spend(1, "scoped nested capture identity")?;
        if !seen.insert(id) {
            continue;
        }
        let definition = module
            .closures
            .get(id.0 as usize)
            .ok_or_else(|| bad("scoped capture definition is absent"))?;
        budget.spend(definition.captures.len(), "scoped nested capture fields")?;
        for capture in &definition.captures {
            match capture {
                Type::Address { .. } => return Ok(true),
                Type::Closure(child) => pending.push(*child),
                _ => {}
            }
        }
    }
    Ok(false)
}

fn capture_inventory(module: &TypedModule, budget: &mut Budget) -> CompileResult<()> {
    if !budget.address_capture {
        return Ok(());
    }
    // A child environment transports actual dependencies, just as a direct
    // Address field does. Type membership selects work; it never proves roots.
    loop {
        let mut changed = false;
        for (index, definition) in module.closures.iter().enumerate() {
            budget.spend(
                definition.captures.len().saturating_add(1),
                "address capture dependency scan",
            )?;
            let mut borrowed = false;
            for ty in &definition.captures {
                borrowed |= match ty {
                    Type::Address { .. } => true,
                    Type::Closure(id) => {
                        budget.lookup(
                            budget.address_capture_closures.len(),
                            "address capture child lookup",
                        )?;
                        budget.address_capture_closures.contains(id)
                    }
                    _ => false,
                };
            }
            if borrowed {
                budget.lookup(
                    budget.address_capture_closures.len(),
                    "address capture closure insertion",
                )?;
                changed |= budget
                    .address_capture_closures
                    .insert(ClosureId(index as u32));
            }
        }
        if !changed {
            return Ok(());
        }
    }
}

fn capture_transition(
    f: &TypedFunction,
    instruction: &Instruction,
    values: &[Roots],
    state: &State,
    live: &resources::Live,
    budget: &mut Budget,
) -> CompileResult<()> {
    budget.spend(1, "captured address caller transition")?;
    let root = match instruction.kind {
        InstructionKind::Store { local, .. }
        | InstructionKind::StorageLive(local)
        | InstructionKind::StorageDead(local)
        | InstructionKind::EndPackageCapture { local, .. }
        | InstructionKind::DropClosure { local }
        | InstructionKind::DropManaged { local }
        | InstructionKind::DropPackage { local }
        | InstructionKind::DropErrorUnion { local }
        | InstructionKind::DropEnum { local }
        | InstructionKind::DropOwnedArray { local }
        | InstructionKind::TakeOwnedArrayElement { local, .. }
        | InstructionKind::DropOwnedArrayElement { local, .. }
        | InstructionKind::StoreOwnedArrayElement { local, .. }
        | InstructionKind::BorrowLocalMut { local }
        | InstructionKind::BorrowOwnedArrayElement {
            local,
            mutable: true,
            ..
        } => Some(Root::Local(local)),
        InstructionKind::BorrowCapture {
            index,
            mutable: true,
        }
        | InstructionKind::WriteCapture { index, .. } => Some(Root::Capture(index)),
        InstructionKind::TakeLocal { local }
            if matches!(
                f.locals[local.0 as usize].ty,
                Type::Closure(_) | Type::Package { .. } | Type::Managed(_)
            ) || resources::owned_error(&f.locals[local.0 as usize].ty)
                || resources::sums::owned(&f.locals[local.0 as usize].ty) =>
        {
            Some(Root::Local(local))
        }
        InstructionKind::TakePackageField { local, index }
        | InstructionKind::DropPackageField { local, index }
        | InstructionKind::StorePackageField { local, index, .. }
        | InstructionKind::BorrowPackageField {
            local,
            index,
            mutable: true,
        } => Some(Root::Field(local, index)),
        _ => None,
    };
    let mut accesses = Vec::new();
    if let Some(root) = root {
        budget.spend(2, "captured address storage access")?;
        accesses.push((Roots::from([root]), None));
    }
    let address = match instruction.kind {
        InstructionKind::WriteAddress { address, .. }
        | InstructionKind::ReborrowAddress {
            address,
            mutable: true,
        } => Some(address),
        InstructionKind::AddressField { address, .. }
        | InstructionKind::AddressIndex { address, .. }
        | InstructionKind::AddressSlice { address, .. }
            if instruction.result.is_some_and(|v| {
                matches!(f.values[v.0 as usize], Type::Address { mutable: true, .. })
            }) =>
        {
            Some(address)
        }
        _ => None,
    };
    let arguments = match &instruction.kind {
        InstructionKind::Call { arguments, .. }
        | InstructionKind::ForeignCall { arguments, .. }
        | InstructionKind::CallIndirect { arguments, .. }
        | InstructionKind::CallClosure { arguments, .. }
        | InstructionKind::CallClosureShared { arguments, .. }
        | InstructionKind::CallClosureMut { arguments, .. }
        | InstructionKind::CallClosureOnce { arguments, .. }
        | InstructionKind::CallClosureBorrowed { arguments, .. } => arguments.as_slice(),
        _ => &[],
    };
    budget.spend(arguments.len(), "captured address mutable call arguments")?;
    for address in address.into_iter().chain(
        arguments
            .iter()
            .copied()
            .filter(|v| matches!(f.values[v.0 as usize], Type::Address { mutable: true, .. })),
    ) {
        budget.spend(1, "captured address access entry")?;
        let mut changed = Roots::new();
        for root in &values[address.0 as usize] {
            budget.lookup(changed.len(), "captured address pointee root")?;
            // Borrowing an environment retains its captured dependencies,
            // but exclusive access to that environment does not write them.
            if root.position() == *root {
                changed.insert(*root);
            }
        }
        accesses.push((changed, Some(address)));
    }
    if accesses.is_empty() {
        return Ok(());
    }
    let mut current = Vec::new();
    if !arguments.is_empty() {
        for operand in resources::uses(&instruction.kind) {
            budget.spend(1, "captured address current call operand")?;
            if resources::capture_carrier(&f.values[operand.0 as usize], budget)? {
                current.push(resources::Place::Value(operand));
            }
        }
        if let InstructionKind::CallClosureShared { callee, .. }
        | InstructionKind::CallClosureMut { callee, .. } = instruction.kind
        {
            budget.spend(1, "captured address current call position")?;
            current.push(resources::Place::Local(callee));
        }
    }
    for (changed, permission) in accesses {
        for place in live.iter().chain(&current) {
            budget.spend(1, "captured address live carrier")?;
            let (ty, held) = match place {
                resources::Place::Value(value) if instruction.result == Some(*value) => continue,
                resources::Place::Value(value) => {
                    (&f.values[value.0 as usize], values.get(value.0 as usize))
                }
                resources::Place::Local(local) => {
                    budget.lookup(state.len(), "captured address local roots")?;
                    (&f.locals[local.0 as usize].ty, state.get(local))
                }
                _ => continue,
            };
            if !resources::capture_carrier(ty, budget)? {
                continue;
            }
            if let Some(held) = held
                && overlaps(&changed, held, budget)?
            {
                if let Some(address) = permission
                    && delegated(f, address, *place, budget)?
                {
                    continue;
                }
                return Err(window_conflict(
                    instruction,
                    "caller write or storage change overlaps a captured address required by a later use",
                ));
            }
        }
    }
    Ok(())
}

// This uses actual Hold roots, never nominal ClosureId equality. It protects
// the preparation interval as well as B's complete captured dependency set.
fn scoped_transition(
    module: &TypedModule,
    f: &TypedFunction,
    instruction: &Instruction,
    values: &[Roots],
    state: &State,
    live: &resources::Live,
    budget: &mut Budget,
) -> CompileResult<()> {
    use resources::Place;
    if let Some(Type::Address { mutable, inner }) =
        instruction.result.map(|v| &f.values[v.0 as usize])
        && matches!(inner.as_ref(), Type::Closure(_))
    {
        let valid = match instruction.kind {
            InstructionKind::BorrowLocal { local } | InstructionKind::BorrowLocalMut { local } => {
                inner.as_ref() == &f.locals[local.0 as usize].ty
                    && *mutable
                        == matches!(instruction.kind, InstructionKind::BorrowLocalMut { .. })
                    && (!*mutable || f.locals[local.0 as usize].mutable)
            }
            InstructionKind::ReborrowAddress {
                address,
                mutable: requested,
            } => {
                *mutable == requested
                    && matches!(&f.values[address.0 as usize], Type::Address { mutable: original, inner: actual } if actual == inner && (!requested || *original))
            }
            InstructionKind::Load(local) => {
                f.locals[local.0 as usize].ty == f.values[instruction.result.unwrap().0 as usize]
            }
            InstructionKind::ReadCapture { index } => {
                f.environment
                    .and_then(|id| module.closures.get(id.0 as usize))
                    .and_then(|c| c.captures.get(index as usize))
                    == instruction.result.map(|v| &f.values[v.0 as usize])
            }
            InstructionKind::BorrowCapture {
                index,
                mutable: requested,
            } => f
                .environment
                .and_then(|id| module.closures.get(id.0 as usize))
                .is_some_and(|c| {
                    c.captures.get(index as usize) == Some(inner.as_ref())
                        && *mutable == requested
                        && (!requested || c.kind != CallableKind::Fn)
                }),
            _ => true,
        };
        if !valid {
            return Err(window_conflict(
                instruction,
                "scoped callback address producer upgrades its actual permission",
            ));
        }
    }
    let accessed_local = match instruction.kind {
        InstructionKind::Load(local)
        | InstructionKind::StorageDead(local)
        | InstructionKind::TakeLocal { local }
        | InstructionKind::Store { local, .. }
        | InstructionKind::BorrowLocal { local }
        | InstructionKind::BorrowLocalMut { local }
        | InstructionKind::DropClosure { local }
        | InstructionKind::CallClosureShared { callee: local, .. }
        | InstructionKind::CallClosureMut { callee: local, .. } => Some(local),
        InstructionKind::BorrowVariantLocal { local, .. }
        | InstructionKind::BorrowPackageField { local, .. }
        | InstructionKind::ReadPackageField { local, .. }
        | InstructionKind::TakePackageField { local, .. }
        | InstructionKind::StorePackageField { local, .. }
        | InstructionKind::DropPackageField { local, .. }
        | InstructionKind::DropPackage { local }
        | InstructionKind::TakeOwnedArrayElement { local, .. }
        | InstructionKind::StoreOwnedArrayElement { local, .. }
        | InstructionKind::DropOwnedArrayElement { local, .. }
        | InstructionKind::BorrowOwnedArrayElement { local, .. }
        | InstructionKind::DropOwnedArray { local } => Some(local),
        _ => None,
    };
    let mut accessed = Roots::new();
    if let Some(local) = accessed_local {
        budget.spend(1, "scoped actual local access")?;
        if matches!(f.locals[local.0 as usize].ty, Type::Address { .. }) {
            if let Some(roots) = state.get(&local) {
                extend(&mut accessed, roots, budget)?;
            }
        } else {
            let root = match instruction.kind {
                InstructionKind::BorrowPackageField { index, .. }
                | InstructionKind::ReadPackageField { index, .. }
                | InstructionKind::TakePackageField { index, .. }
                | InstructionKind::StorePackageField { index, .. }
                | InstructionKind::DropPackageField { index, .. } => Root::Field(local, index),
                _ => Root::Local(local),
            };
            accessed.insert(root);
        }
    }
    // Reborrowing transports the permission; it does not access the pointee.
    // With's F access is checked against B separately below.
    let transported = matches!(
        instruction.kind,
        InstructionKind::ReborrowAddress { .. }
            | InstructionKind::AddressField { .. }
            | InstructionKind::AddressIndex { .. }
            | InstructionKind::WithForeignCallback { .. }
    );
    let empty = Roots::new();
    for active in live {
        budget.spend(1, "scoped live exclusive address")?;
        let (ty, roots) = match active {
            Place::Value(value) if instruction.result != Some(*value) => {
                (&f.values[value.0 as usize], &values[value.0 as usize])
            }
            Place::Local(local) if Some(*local) != accessed_local => (
                &f.locals[local.0 as usize].ty,
                state.get(local).unwrap_or(&empty),
            ),
            _ => continue,
        };
        if !matches!(ty, Type::Address { mutable: true, inner } if matches!(inner.as_ref(), Type::Closure(_)))
        {
            continue;
        }
        if overlaps(&accessed, roots, budget)? {
            return Err(window_conflict(
                instruction,
                "scoped callback preparation accesses an exclusively borrowed position",
            ));
        }
        if !transported {
            for operand in resources::uses(&instruction.kind) {
                budget.spend(1, "scoped actual address operand")?;
                if *active == Place::Value(operand) {
                    continue;
                }
                if overlaps(&values[operand.0 as usize], roots, budget)? {
                    return Err(window_conflict(
                        instruction,
                        "scoped callback preparation accesses a live exclusive alias",
                    ));
                }
            }
        }
    }
    if let InstructionKind::WithForeignCallback { callback, body, .. } = instruction.kind
        && overlaps(
            &values[callback.0 as usize],
            &values[body.0 as usize],
            budget,
        )?
    {
        return Err(window_conflict(
            instruction,
            "scoped body accesses the actual callback position",
        ));
    }
    if let InstructionKind::WithForeignCallback { callback, .. } = instruction.kind {
        for active in live {
            budget.spend(1, "scoped retained caller alias")?;
            let (ty, roots) = match active {
                Place::Value(value) => (&f.values[value.0 as usize], &values[value.0 as usize]),
                Place::Local(local) => (
                    &f.locals[local.0 as usize].ty,
                    state.get(local).unwrap_or(&empty),
                ),
                _ => continue,
            };
            if matches!(ty, Type::Address { .. })
                && overlaps(&values[callback.0 as usize], roots, budget)?
                && !delegated(f, callback, *active, budget)?
            {
                return Err(window_conflict(
                    instruction,
                    "scoped callback overlaps an ordinary caller alias that remains live",
                ));
            }
        }
    }
    let call = match &instruction.kind {
        InstructionKind::Call {
            function,
            arguments,
        } => Some((*function, arguments.as_slice())),
        InstructionKind::MakeClosure { closure, captures } => {
            let definition = &module.closures[closure.0 as usize];
            budget.lookup(budget.scoped_functions.len(), "scoped capture entry lookup")?;
            if budget.scoped_functions.contains(&definition.function) {
                budget.spend(captures.len(), "scoped actual capture signature")?;
                if captures.len() != definition.captures.len() {
                    return Err(bad(
                        "scoped capture arity differs from the actual environment",
                    ));
                }
                for (value, expected) in captures.iter().zip(&definition.captures) {
                    budget.type_work(expected, false, "scoped actual capture type comparison")?;
                    if &f.values[value.0 as usize] != expected {
                        return Err(bad("scoped capture type differs from its actual operand"));
                    }
                }
            }
            Some((definition.function, captures.as_slice()))
        }
        _ => None,
    };
    if let Some((callee, arguments)) = call {
        budget.lookup(
            budget.scoped_functions.len(),
            "scoped call actual window lookup",
        )?;
        if budget.scoped_functions.contains(&callee) {
            for (index, argument) in arguments.iter().enumerate() {
                budget.spend(1, "scoped call actual mutable window")?;
                if !matches!(
                    f.values[argument.0 as usize],
                    Type::Address { mutable: true, .. }
                ) {
                    continue;
                }
                for active in live {
                    budget.spend(1, "scoped call retained alias")?;
                    let (ty, roots) = match active {
                        Place::Value(value) if instruction.result != Some(*value) => {
                            (&f.values[value.0 as usize], &values[value.0 as usize])
                        }
                        Place::Local(local) => (
                            &f.locals[local.0 as usize].ty,
                            state.get(local).unwrap_or(&empty),
                        ),
                        _ => continue,
                    };
                    if matches!(ty, Type::Address { .. })
                        && overlaps(&values[argument.0 as usize], roots, budget)?
                        && !delegated(f, *argument, *active, budget)?
                    {
                        return Err(window_conflict(
                            instruction,
                            "scoped call overlaps an ordinary caller alias that remains live",
                        ));
                    }
                }
                for (other, value) in arguments.iter().enumerate() {
                    if index != other
                        && overlaps(
                            &values[argument.0 as usize],
                            &values[value.0 as usize],
                            budget,
                        )?
                    {
                        return Err(window_conflict(
                            instruction,
                            "scoped helper body aliases an actual exclusive argument",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

// Storage descriptions supply provenance to derived views; they are not
// themselves strong address windows. Ordinary resource descriptions are kept
// by Analyze's separate responsibility facts, not this public address graph.
fn tracked_desc(ty: &Type) -> bool {
    matches!(ty, Type::Desc(inner) if matches!(inner.as_ref(), Type::ByteBuffer | Type::OwnedStore { .. }))
}

fn state_cost(state: &State, budget: &mut Budget) -> CompileResult<()> {
    budget.spend(state.len(), "sparse Hold state")?;
    for roots in state.values() {
        budget.spend(roots.len(), "sparse Hold relations")?;
    }
    Ok(())
}

fn extend(target: &mut Roots, source: &Roots, budget: &mut Budget) -> CompileResult<bool> {
    let before = target.len();
    for root in source {
        budget.lookup(target.len(), "Hold root union")?;
        target.insert(*root);
    }
    Ok(before != target.len())
}

fn contains_owned_store(ty: &Type, budget: &mut Budget) -> CompileResult<bool> {
    if dynamic_store::is_owned_carrier(ty) {
        return Ok(true);
    }
    if let Type::Package { fields, .. } = ty {
        budget.spend(fields.len(), "captured Store whole-position classification")?;
        for (_, field) in fields {
            if dynamic_store::is_owned_carrier(field)
                || (matches!(field, Type::Package { .. }) && contains_owned_store(field, budget)?)
            {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    if !budget.sum_functions.is_empty()
        && let Type::Enum { variants, .. } = ty
    {
        budget.spend(variants.len(), "ordinary Sum Store alternatives")?;
        for (_, fields) in variants {
            budget.spend(fields.len(), "ordinary Sum Store fields")?;
            for t in fields {
                if contains_owned_store(t, budget)? {
                    return Ok(true);
                }
            }
        }
    }
    if !budget.sum_functions.is_empty()
        && let Type::ErrorUnion { error, result } = ty
    {
        return Ok(contains_owned_store(error, budget)? || contains_owned_store(result, budget)?);
    }
    Ok(false)
}

/// Validate against this invocation's private precise roots. The exported
/// candidate coarsens Field into Local and is deliberately not read here.
#[allow(clippy::too_many_arguments)]
fn store_window_transition(
    module: &TypedModule,
    f: &TypedFunction,
    instruction: &Instruction,
    values: &[Roots],
    state: &State,
    live: &resources::Live,
    callback_windows: bool,
    parameter_windows: bool,
    budget: &mut Budget,
) -> CompileResult<()> {
    budget.spend(1, "captured Store window transition scan")?;
    let forward_windows = callback_windows
        && budget
            .forward_functions
            .get(f.id.0 as usize)
            .copied()
            .unwrap_or(false);
    let single = match instruction.kind {
        InstructionKind::TakeCapture { index }
            if f.environment
                .and_then(|id| module.closures.get(id.0 as usize))
                .and_then(|c| c.captures.get(index as usize))
                .is_some_and(|ty| {
                    dynamic_store::is_owned_carrier(ty)
                        || (callback_windows
                            && matches!(ty, Type::Managed(_) | Type::Package { .. }))
                        || (forward_windows && matches!(ty, Type::Closure(_)))
                }) =>
        {
            Some(Root::Capture(index))
        }
        InstructionKind::UnpackVariant { local, .. }
        | InstructionKind::DropEnum { local }
        | InstructionKind::TakeLocal { local }
        | InstructionKind::DropManaged { local }
        | InstructionKind::DropPackage { local }
            if boundary_owner(&f.locals[local.0 as usize].ty, callback_windows, budget)? =>
        {
            Some(Root::Local(local))
        }
        InstructionKind::Store { local, .. } | InstructionKind::DropErrorUnion { local }
            if callback_windows
                && boundary_owner(&f.locals[local.0 as usize].ty, true, budget)? =>
        {
            Some(Root::Local(local))
        }
        InstructionKind::TakePackageField { local, index }
        | InstructionKind::DropPackageField { local, index }
        | InstructionKind::StorePackageField { local, index, .. }
            if match &f.locals[local.0 as usize].ty {
                Type::Package { fields, .. } => match fields.get(index as usize) {
                    Some((_, ty)) => {
                        (forward_windows && matches!(ty, Type::Closure(_)))
                            || (callback_windows && boundary_owner(ty, true, budget)?)
                            || dynamic_store::is_owned_carrier(ty)
                            || (matches!(ty, Type::Package { .. })
                                && contains_owned_store(ty, budget)?)
                    }
                    None => false,
                },
                _ => false,
            } =>
        {
            Some(Root::Field(local, index))
        }
        _ => None,
    };
    let mut changed = Roots::new();
    if let Some(root) = single {
        budget.spend(1, "captured Store transition root")?;
        changed.insert(root);
    }
    if forward_windows {
        // Returning F moves its actual storage position, including the
        // compiler's single replacement slot. A copied concrete caller value
        // uses Load instead and keeps the original root valid.
        let local = match instruction.kind {
            InstructionKind::TakeLocal { local }
            | InstructionKind::Store { local, .. }
            | InstructionKind::DropClosure { local }
            | InstructionKind::DropOwnedArray { local }
            | InstructionKind::TakeOwnedArrayElement { local, .. }
            | InstructionKind::DropOwnedArrayElement { local, .. }
            | InstructionKind::StoreOwnedArrayElement { local, .. } => Some(local),
            _ => None,
        };
        if let Some(local) = local {
            let ty = &f.locals[local.0 as usize].ty;
            if matches!(ty, Type::Closure(_)) || super::forward::internal_slot(ty).is_some() {
                budget.lookup(changed.len(), "forward actual callable storage transition")?;
                changed.insert(Root::Local(local));
            }
        }
    }
    if let InstructionKind::UnpackVariant { fields, .. } = &instruction.kind {
        budget.spend(fields.len(), "Unpack target window roots")?;
        for local in fields {
            budget.spend(1, "Unpack target window insertion")?;
            changed.insert(Root::Local(*local));
        }
    }
    if let InstructionKind::OwnedStoreReserve { store, .. }
    | InstructionKind::OwnedStorePush { store, .. }
    | InstructionKind::OwnedStorePop { store } = instruction.kind
    {
        let roots = values
            .get(store.0 as usize)
            .ok_or_else(|| bad("captured Store transition operand is absent"))?;
        extend(&mut changed, roots, budget)?;
    }
    if callback_windows {
        let buffer = match instruction.kind {
            InstructionKind::ByteBufferReserve { buffer, .. }
            | InstructionKind::ByteBufferPush { buffer, .. }
            | InstructionKind::ByteBufferTruncate { buffer, .. }
            | InstructionKind::ByteBufferBorrow {
                buffer,
                mutable: true,
            } => Some(buffer),
            _ => None,
        };
        if let Some(buffer) = buffer {
            let roots = values
                .get(buffer.0 as usize)
                .ok_or_else(|| bad("owned callback buffer transition operand is absent"))?;
            extend(&mut changed, roots, budget)?;
        }
    }
    if changed.is_empty() {
        return Ok(());
    }
    budget.spend(live.len(), "captured Store live windows")?;
    for place in live {
        let held = match place {
            resources::Place::Value(value) => values.get(value.0 as usize),
            resources::Place::Local(local) => {
                budget.lookup(state.len(), "captured Store live local roots")?;
                state.get(local)
            }
            _ => None,
        };
        let Some(held) = held else {
            continue;
        };
        budget.product(
            changed.len(),
            held.len(),
            1,
            "captured Store window overlap",
        )?;
        if changed
            .iter()
            .any(|changed| held.iter().any(|root| root_overlap(*changed, *root)))
        {
            if parameter_windows {
                return Err(window_conflict(
                    instruction,
                    "Store transition invalidates a live element window",
                ));
            }
            return Err(bad(if callback_windows {
                "owned storage transition invalidates a live address window"
            } else {
                "captured Store transition invalidates a live element window"
            }));
        }
    }
    Ok(())
}

fn boundary_owner(ty: &Type, callback_windows: bool, budget: &mut Budget) -> CompileResult<bool> {
    if callback_windows {
        budget.type_work(ty, false, "owned callback address boundary classification")?;
        if resources::sums::owned(ty) {
            return Ok(true);
        }
    }
    contains_owned_store(ty, budget)
}

fn infer(
    module: &TypedModule,
    summaries: &[Roots],
    f: &TypedFunction,
    predecessors: &[Vec<usize>],
    reachable: &[bool],
    store_windows: bool,
    budget: &mut Budget,
) -> CompileResult<Vec<ValueHold>> {
    infer_with_private(
        module,
        summaries,
        f,
        predecessors,
        reachable,
        store_windows,
        budget,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn infer_with_private(
    module: &TypedModule,
    summaries: &[Roots],
    f: &TypedFunction,
    predecessors: &[Vec<usize>],
    reachable: &[bool],
    store_windows: bool,
    budget: &mut Budget,
    private: Option<&mut Vec<Roots>>,
) -> CompileResult<Vec<ValueHold>> {
    let thread_windows = if budget.shared_thread_functions.is_empty() {
        false
    } else {
        budget.lookup(
            budget.shared_thread_functions.len(),
            "shared thread function lookup",
        )?;
        budget.shared_thread_functions.contains(&f.id)
    };
    let scoped = if budget.scoped_functions.is_empty() {
        false
    } else {
        budget.lookup(
            budget.scoped_functions.len(),
            "scoped window function lookup",
        )?;
        budget.scoped_functions.contains(&f.id)
    };
    let v = f.values.len();
    budget.spend(v, "dense Hold candidate")?;
    let captured_address = |ty: &Type, budget: &mut Budget| -> CompileResult<bool> {
        if scoped {
            return scoped_captures_address(module, ty, budget);
        }
        let Type::Closure(id) = ty else {
            return Ok(false);
        };
        if budget.address_capture {
            budget.lookup(
                budget.address_capture_closures.len(),
                "ordinary capture dependency lookup",
            )?;
            return Ok(budget.address_capture_closures.contains(id));
        }
        let Some(closure) = module.closures.get(id.0 as usize) else {
            return Ok(false);
        };
        budget.spend(closure.captures.len(), "address capture scan")?;
        Ok(closure
            .captures
            .iter()
            .any(|ty| matches!(ty, Type::Address { .. })))
    };
    let mut has_sources = f
        .values
        .iter()
        .any(|ty| matches!(ty, Type::Address { .. }) || tracked_desc(ty));
    if !has_sources {
        for local in &f.parameters {
            if captured_address(&f.locals[local.0 as usize].ty, budget)? {
                has_sources = true;
                break;
            }
        }
    }
    let callback_windows = if budget.owned_callback_window_functions.is_empty() {
        false
    } else {
        budget.spend(1, "owned callback window function lookup")?;
        budget.owned_callback_window_functions[f.id.0 as usize]
    };
    let address_afters = if store_windows || callback_windows || scoped {
        resources::address_afters(f, reachable, budget)?
    } else {
        Vec::new()
    };
    let capture_afters = if budget.address_capture {
        resources::capture_afters(f, reachable, budget)?
    } else {
        Vec::new()
    };
    if !has_sources {
        if let Some(private) = private {
            budget.spend(v, "private Product empty provenance")?;
            *private = vec![Roots::new(); v];
        }
        return Ok((0..v)
            .map(|i| ValueHold {
                value: ValueId(i as u32),
                roots: vec![],
            })
            .collect());
    }
    budget.spend(f.blocks.len(), "sparse Hold CFG states")?;
    let mut values = vec![Roots::new(); v];
    let mut outputs = vec![State::new(); f.blocks.len()];
    let mut exclusive = if thread_windows {
        budget.spend(v.saturating_add(f.blocks.len()), "thread access tables")?;
        vec![Roots::new(); v]
    } else {
        Vec::new()
    };
    let mut exclusive_outputs = if thread_windows {
        vec![State::new(); f.blocks.len()]
    } else {
        Vec::new()
    };
    loop {
        let mut changed = false;
        for (b, block) in f.blocks.iter().enumerate() {
            budget.spend(1, "Hold fixed point")?;
            if !reachable[b] {
                continue;
            }
            let mut state = State::new();
            let mut exclusive_state = State::new();
            if b == f.entry.0 as usize {
                budget.spend(f.parameters.len(), "incoming Hold parameters")?;
                for (index, local) in f.parameters.iter().enumerate() {
                    if matches!(f.locals[local.0 as usize].ty, Type::Address { .. })
                        || tracked_desc(&f.locals[local.0 as usize].ty)
                        || captured_address(&f.locals[local.0 as usize].ty, budget)?
                    {
                        state.insert(*local, Roots::from([Root::Parameter(index as u32)]));
                        if thread_windows
                            && matches!(
                                f.locals[local.0 as usize].ty,
                                Type::Address { mutable: true, .. }
                            )
                        {
                            budget.spend(1, "thread incoming exclusive address")?;
                            exclusive_state
                                .insert(*local, Roots::from([Root::Parameter(index as u32)]));
                        }
                    }
                }
            } else {
                for &predecessor in &predecessors[b] {
                    budget.spend(1, "Hold predecessor")?;
                    if !reachable[predecessor] {
                        continue;
                    }
                    for (local, incoming) in &outputs[predecessor] {
                        budget.lookup(state.len(), "Hold local join")?;
                        extend(state.entry(*local).or_default(), incoming, budget)?;
                    }
                    if thread_windows {
                        for (local, incoming) in &exclusive_outputs[predecessor] {
                            budget
                                .lookup(exclusive_state.len(), "thread access predecessor join")?;
                            extend(exclusive_state.entry(*local).or_default(), incoming, budget)?;
                        }
                    }
                }
            }
            for (position, instruction) in block.instructions.iter().enumerate() {
                budget.spend(1, "Hold instruction")?;
                if !capture_afters.is_empty() {
                    capture_transition(
                        f,
                        instruction,
                        &values,
                        &state,
                        &capture_afters[b][position],
                        budget,
                    )?;
                }
                if scoped {
                    scoped_transition(
                        module,
                        f,
                        instruction,
                        &values,
                        &state,
                        &address_afters[b][position],
                        budget,
                    )?;
                }
                if !address_afters.is_empty() {
                    store_window_transition(
                        module,
                        f,
                        instruction,
                        &values,
                        &state,
                        &address_afters[b][position],
                        callback_windows,
                        private.is_some(),
                        budget,
                    )?;
                }
                let mut found = Roots::new();
                match &instruction.kind {
                    InstructionKind::DescribeLocal { local } if matches!(&f.locals[local.0 as usize].ty, Type::Managed(inner) if matches!(inner.as_ref(), Type::ByteBuffer | Type::OwnedStore { .. })) =>
                    {
                        found.insert(Root::Local(*local));
                    }
                    InstructionKind::DescribePackageField { local, index }
                        if instruction
                            .result
                            .is_some_and(|value| tracked_desc(&f.values[value.0 as usize])) =>
                    {
                        let root = match &f.locals[local.0 as usize].ty {
                            Type::Package { fields, .. } if fields.get(*index as usize)
                                .is_some_and(|(_, ty)| matches!(ty, Type::Managed(inner) if **inner == Type::ByteBuffer || matches!(inner.as_ref(), Type::OwnedStore { .. }))) => Root::Field(*local, *index),
                            _ => Root::Local(*local),
                        };
                        found.insert(root);
                    }
                    InstructionKind::ProjectOwnedArrayField { local, .. }
                        if instruction
                            .result
                            .is_some_and(|value| tracked_desc(&f.values[value.0 as usize])) =>
                    {
                        // Array element slots are independently owned, but the
                        // initial window profile keeps a conservative whole
                        // array storage origin across element replacement.
                        found.insert(Root::Local(*local));
                    }
                    InstructionKind::DescribeCapture { index }
                        if instruction
                            .result
                            .is_some_and(|value| tracked_desc(&f.values[value.0 as usize])) =>
                    {
                        found.insert(Root::Capture(*index));
                    }
                    InstructionKind::ByteBufferBorrow { buffer, .. }
                    | InstructionKind::OwnedStoreBorrow { store: buffer, .. }
                    | InstructionKind::OwnedStoreBorrowMut { store: buffer, .. } => {
                        let source = values
                            .get(buffer.0 as usize)
                            .ok_or_else(|| bad("ByteBuffer source outside table"))?;
                        extend(&mut found, source, budget)?;
                    }
                    InstructionKind::BorrowOwnedArrayElement { local, .. } => {
                        found.insert(Root::Local(*local));
                    }
                    InstructionKind::BorrowFieldLocal{local,path,..}=>{found.insert(inline_root(f,*local,path,budget)?);}
                    InstructionKind::BorrowVariantAddress{address,path,..}=>{
                        for root in &values[address.0 as usize]{found.insert(address_root(f,*address,path,*root,budget)?);}
                    }
                    InstructionKind::BorrowVariantLocal { local, path, .. } => {
                        budget.spend(path.len().saturating_add(1), "payload borrow root")?;
                        found.insert(variant_root(f, *local, path, budget)?);
                    }
                    InstructionKind::BorrowPackageField { local, index, .. } => {
                        // The checked instruction borrows this actual inline
                        // field. Keep sibling fields separate for private
                        // windows; exporting the Hold still retains the parent.
                        found.insert(Root::Field(*local, *index));
                    }
                    InstructionKind::BorrowLocal { local }
                    | InstructionKind::BorrowLocalMut { local } => {
                        found.insert(Root::Local(*local));
                        if matches!(f.locals[local.0 as usize].ty, Type::Closure(_))
                            && let Some(held) = state.get(local)
                        {
                            if budget.address_capture {
                                for root in held {
                                    budget.lookup(
                                        found.len(),
                                        "captured environment retained dependency",
                                    )?;
                                    found.insert(root.retained());
                                }
                            } else {
                                extend(&mut found, held, budget)?;
                            }
                        }
                    }
                    InstructionKind::ReborrowAddress { address, .. }
                    | InstructionKind::AddressSlice { address, .. }
                    | InstructionKind::AddressField { address, .. }
                    | InstructionKind::AddressIndex { address, .. } => {
                        let source = values
                            .get(address.0 as usize)
                            .ok_or_else(|| bad("projection address outside table"))?;
                        extend(&mut found, source, budget)?;
                    }
                    InstructionKind::BorrowCapture { index, .. }
                    | InstructionKind::ReadCapture { index } => {
                        let ty = instruction
                            .result
                            .and_then(|id| f.values.get(id.0 as usize));
                        if matches!(ty, Some(Type::Address { .. })) || ty.is_some_and(tracked_desc)
                        {
                            found.insert(Root::Capture(*index));
                        }
                    }
                    InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                        budget.lookup(state.len(), "Hold load lookup")?;
                        if let Some(source) = state.get(local) {
                            budget.spend(source.len(), "Hold load")?;
                            found = source.clone();
                        }
                        if matches!(instruction.kind, InstructionKind::TakeLocal { .. }) {
                            state.remove(local);
                        }
                    }
                    InstructionKind::Store { local, value } => {
                        let source = values
                            .get(value.0 as usize)
                            .ok_or_else(|| bad("Hold value outside table"))?;
                        budget.lookup(state.len(), "Hold store lookup")?;
                        budget.spend(source.len(), "Hold store")?;
                        if source.is_empty() {
                            state.remove(local);
                        } else {
                            state.insert(*local, source.clone());
                        }
                    }
                    InstructionKind::UnpackVariant { local, fields, .. } => {
                        budget.spend(fields.len().saturating_add(1), "Unpack Hold local reset")?;
                        state.remove(local);
                        for local in fields {
                            state.remove(local);
                        }
                    }
                    InstructionKind::DropEnum { local }
                    | InstructionKind::StorageLive(local)
                    | InstructionKind::StorageDead(local)
                    | InstructionKind::BindPackageCapture { local, .. }
                    | InstructionKind::EndPackageCapture { local, .. }
                    | InstructionKind::DropManaged { local }
                    | InstructionKind::DropClosure { local } => {
                        budget.lookup(state.len(), "Hold storage lookup")?;
                        state.remove(local);
                    }
                    InstructionKind::CallClosureShared { callee, .. }
                    | InstructionKind::CallClosureMut { callee, .. }
                        if instruction.result.is_some_and(|value| {
                            matches!(f.values.get(value.0 as usize), Some(Type::Address { .. }))
                        }) =>
                    {
                        found.insert(Root::Local(*callee));
                    }
                    InstructionKind::CallClosureBorrowed { callee, .. }
                        if instruction.result.is_some_and(|value| {
                            matches!(f.values.get(value.0 as usize), Some(Type::Address { .. }))
                        }) =>
                    {
                        let actual = values
                            .get(callee.0 as usize)
                            .ok_or_else(|| bad("lending callee outside value table"))?;
                        extend(&mut found, actual, budget)?;
                    }
                    InstructionKind::Call {
                        function,
                        arguments,
                    } if instruction.result.is_some_and(|value| {
                        matches!(f.values.get(value.0 as usize), Some(Type::Address { .. }))
                    }) =>
                    {
                        let incoming = summaries
                            .get(function.0 as usize)
                            .ok_or_else(|| bad("call refers to an absent function"))?;
                        for root in incoming {
                            budget.spend(1, "call result Hold substitution")?;
                            let Root::Parameter(index) = root else {
                                return Err(bad("call summary has a non-parameter root"));
                            };
                            let argument = arguments
                                .get(*index as usize)
                                .ok_or_else(|| bad("call summary parameter is absent"))?;
                            let actual = values
                                .get(argument.0 as usize)
                                .ok_or_else(|| bad("call argument outside value table"))?;
                            extend(&mut found, actual, budget)?;
                        }
                    }
                    InstructionKind::MakeClosure { captures, .. } => {
                        for value in captures {
                            budget.spend(1, "Hold capture operand")?;
                            let ty = f
                                .values
                                .get(value.0 as usize)
                                .ok_or_else(|| bad("Hold capture outside table"))?;
                            if matches!(ty, Type::Address { .. }) || captured_address(ty, budget)? {
                                let source = values
                                    .get(value.0 as usize)
                                    .ok_or_else(|| bad("Hold capture outside table"))?;
                                extend(&mut found, source, budget)?;
                            }
                        }
                    }
                    _ => {}
                }
                if let Some(result) = instruction.result {
                    let value = values
                        .get_mut(result.0 as usize)
                        .ok_or_else(|| bad("Hold result outside table"))?;
                    changed |= extend(value, &found, budget)?;
                }
                if thread_windows {
                    changed |= thread_access_step(
                        module,
                        f,
                        instruction,
                        &values,
                        &mut exclusive,
                        &mut exclusive_state,
                        budget,
                    )?;
                }
            }
            state_cost(&state, budget)?;
            state_cost(&outputs[b], budget)?;
            if state != outputs[b] {
                outputs[b] = state;
                changed = true;
            }
            if thread_windows {
                state_cost(&exclusive_state, budget)?;
                state_cost(&exclusive_outputs[b], budget)?;
                if exclusive_state != exclusive_outputs[b] {
                    exclusive_outputs[b] = exclusive_state;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    if thread_windows {
        for (block, reachable) in f.blocks.iter().zip(reachable) {
            if !reachable {
                continue;
            }
            for instruction in &block.instructions {
                budget.spend(1, "shared thread completed concurrent window")?;
                shared_thread_window(module, f, instruction, &values, &exclusive, budget)?;
            }
        }
    }
    if scoped {
        for (block, reachable) in f.blocks.iter().zip(reachable) {
            if !reachable {
                continue;
            }
            for instruction in &block.instructions {
                budget.spend(1, "scoped completed address provenance")?;
                if let InstructionKind::WithForeignCallback { callback, .. } = instruction.kind
                    && values[callback.0 as usize].is_empty()
                {
                    return Err(bad("scoped callback has no actual storage origin"));
                }
                if let InstructionKind::Call {
                    function: callee,
                    arguments,
                } = &instruction.kind
                    && budget.scoped_functions.contains(callee)
                {
                    budget.spend(arguments.len(), "scoped completed call origins")?;
                    for value in arguments {
                        if matches!(
                            f.values[value.0 as usize],
                            Type::Address { mutable: true, .. }
                        ) && values[value.0 as usize].is_empty()
                        {
                            return Err(bad("scoped helper argument has no actual storage origin"));
                        }
                    }
                }
            }
        }
    }
    if let Some(private) = private {
        let work = values
            .iter()
            .fold(values.len(), |n, roots| n.saturating_add(roots.len()));
        budget.spend(work, "private Product actual provenance copy")?;
        *private = values.clone();
    }
    let mut exported = Vec::with_capacity(values.len());
    for (value, roots) in values.into_iter().enumerate() {
        let mut coarse = BTreeSet::new();
        for root in roots {
            budget.lookup(coarse.len(), "public Hold projection")?;
            coarse.insert(match root {
                Root::Local(local)
                | Root::Field(local, _)
                | Root::Projection(local, ..)
                | Root::RetainedLocal(local)
                | Root::RetainedField(local, _)
                | Root::RetainedProjection(local, ..) => HoldRoot::Local(local),
                Root::Window(origin,..)|Root::RetainedWindow(origin,..)=>origin,
                Root::Capture(index) | Root::RetainedCapture(index) => HoldRoot::Capture(index),
                Root::Parameter(index) | Root::RetainedParameter(index) => {
                    HoldRoot::Parameter(index)
                }
            });
        }
        exported.push(ValueHold {
            value: ValueId(value as u32),
            roots: coarse.into_iter().collect(),
        });
    }
    Ok(exported)
}

// This private summary is a may-effect for the complete call, not an ordering
// claim. It uses actual Hold origins at each call; no nominal type or parameter
// name establishes an alias. Only modules with a proven parameter capture pay.
// This is a conservative dependency classifier, never an alias certificate.
// A Store or a transitively Store-capturing closure hidden in any supported
// aggregate means that a call needs actual provenance before it can be opened.
fn product_store_capture_dependency(
    ty: &Type,
    closures: &[bool],
    budget: &mut Budget,
) -> CompileResult<bool> {
    budget.spend(1, "Product Store capture type dependency")?;
    match ty {
        Type::OwnedStore { .. } => Ok(true),
        Type::Closure(id) => closures
            .get(id.0 as usize)
            .copied()
            .ok_or_else(|| bad("Product Store captured closure is absent")),
        Type::Address { inner, .. }
        | Type::Managed(inner)
        | Type::Desc(inner)
        | Type::Array { element: inner, .. }
        | Type::Slice { element: inner }
        | Type::Opaque { hidden: inner, .. } => {
            product_store_capture_dependency(inner, closures, budget)
        }
        Type::Tuple(fields) => {
            for field in fields {
                if product_store_capture_dependency(field, closures, budget)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            for (_, field) in fields {
                if product_store_capture_dependency(field, closures, budget)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Type::Enum { variants, .. } => {
            for (_, fields) in variants {
                for field in fields {
                    if product_store_capture_dependency(field, closures, budget)? {
                        return Ok(true);
                    }
                }
            }
            Ok(false)
        }
        Type::ErrorUnion { error, result } => {
            Ok(product_store_capture_dependency(error, closures, budget)?
                || product_store_capture_dependency(result, closures, budget)?)
        }
        // A code pointer's signature is not captured data. Its actual Store
        // arguments are conservatively accounted at the call site instead.
        Type::FunctionPointer { .. } | Type::ForeignCallback { .. } => Ok(false),
        _ => Ok(false),
    }
}

// A by-value owner may release a Store buried in an owning Product, array,
// Sum, or concrete closure. Addresses and descriptions do not transfer it.
fn product_owned_store_dependency(
    ty: &Type,
    closures: &[bool],
    budget: &mut Budget,
) -> CompileResult<bool> {
    budget.spend(1, "Product owned Store type dependency")?;
    match ty {
        Type::Managed(inner) if matches!(inner.as_ref(), Type::OwnedStore { .. }) => Ok(true),
        Type::Managed(inner) | Type::Opaque { hidden: inner, .. } => {
            product_owned_store_dependency(inner, closures, budget)
        }
        Type::Tuple(fields) => {
            for field in fields {
                if product_owned_store_dependency(field, closures, budget)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Type::Closure(id) => closures
            .get(id.0 as usize)
            .copied()
            .ok_or_else(|| bad("Product owned Store closure is absent")),
        Type::Array { element, length } if *length > 0 => {
            product_owned_store_dependency(element, closures, budget)
        }
        Type::Package { fields, .. } | Type::Record { fields, .. } => {
            for (_, field) in fields {
                if product_owned_store_dependency(field, closures, budget)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Type::Enum { variants, .. } => {
            for (_, fields) in variants {
                for field in fields {
                    if product_owned_store_dependency(field, closures, budget)? {
                        return Ok(true);
                    }
                }
            }
            Ok(false)
        }
        Type::ErrorUnion { error, result } => {
            Ok(product_owned_store_dependency(error, closures, budget)?
                || product_owned_store_dependency(result, closures, budget)?)
        }
        _ => Ok(false),
    }
}

// A captured Store used only for reads is not an invalidating effect. This
// conservative callable graph records possible writes, not source identity;
// identity is still required separately at the actual call boundary.
fn product_may_store_write(
    module: &TypedModule,
    store_closures: &[bool],
    budget: &mut Budget,
) -> CompileResult<Vec<bool>> {
    budget.spend(module.functions.len(), "Product callable may-write table")?;
    let mut writes = vec![false; module.functions.len()];
    loop {
        let mut changed = false;
        for (index, function) in module.functions.iter().enumerate() {
            for block in &function.blocks {
                budget.spend(
                    block.instructions.len().saturating_add(1),
                    "Product callable may-write scan",
                )?;
                for instruction in &block.instructions {
                    let mut effect =
                        dynamic_store::access(&instruction.kind).is_some_and(|(_, write)| write);
                    let owner = match instruction.kind {
                        InstructionKind::TakeLocal { local }
                        | InstructionKind::Store { local, .. }
                        | InstructionKind::DropManaged { local }
                        | InstructionKind::DropPackage { local }
                        | InstructionKind::DropClosure { local }
                        | InstructionKind::DropOwnedArray { local }
                        | InstructionKind::DropEnum { local }
                        | InstructionKind::DropErrorUnion { local }
                        | InstructionKind::TakePackageField { local, .. }
                        | InstructionKind::DropPackageField { local, .. }
                        | InstructionKind::StorePackageField { local, .. }
                        | InstructionKind::TakeOwnedArrayElement { local, .. }
                        | InstructionKind::DropOwnedArrayElement { local, .. }
                        | InstructionKind::StoreOwnedArrayElement { local, .. } => {
                            Some(&function.locals[local.0 as usize].ty)
                        }
                        InstructionKind::WriteAddress { address, .. } => {
                            match &function.values[address.0 as usize] {
                                Type::Address { inner, .. } => Some(inner.as_ref()),
                                _ => None,
                            }
                        }
                        InstructionKind::TakeCapture { index }
                        | InstructionKind::WriteCapture { index, .. } => function
                            .environment
                            .and_then(|id| module.closures.get(id.0 as usize))
                            .and_then(|closure| closure.captures.get(index as usize)),
                        _ => None,
                    };
                    if let Some(owner) = owner {
                        effect |= product_owned_store_dependency(owner, store_closures, budget)?;
                    }
                    if let InstructionKind::WriteDesc { desc, .. } = instruction.kind {
                        budget.spend(1, "Product callable descriptor write")?;
                        effect |= dynamic_store::is_carrier(&function.values[desc.0 as usize]);
                    }
                    let uncertain = match &instruction.kind {
                        InstructionKind::ForeignCall { arguments, .. }
                        | InstructionKind::CallIndirect { arguments, .. }
                        | InstructionKind::CallClosure { arguments, .. }
                        | InstructionKind::CallClosureOnce { arguments, .. }
                        | InstructionKind::CallClosureBorrowed { arguments, .. }
                        | InstructionKind::CallClosureShared { arguments, .. }
                        | InstructionKind::CallClosureMut { arguments, .. } => arguments.as_slice(),
                        _ => &[],
                    };
                    budget.spend(uncertain.len(), "Product callable unknown arguments")?;
                    for value in uncertain {
                        let ty = &function.values[value.0 as usize];
                        effect |= dynamic_store::is_carrier(ty)
                            || product_owned_store_dependency(ty, store_closures, budget)?;
                    }
                    let target = match instruction.kind {
                        InstructionKind::CallClosureShared { callee, .. }
                        | InstructionKind::CallClosureMut { callee, .. } => {
                            Some(&function.locals[callee.0 as usize].ty)
                        }
                        InstructionKind::CallClosure { callee, .. }
                        | InstructionKind::CallClosureOnce { callee, .. }
                        | InstructionKind::WithScopedThread { body: callee, .. }
                        | InstructionKind::CallClosureBorrowed { callee, .. } => {
                            Some(&function.values[callee.0 as usize])
                        }
                        _ => None,
                    };
                    // Ordinary function calls and closure entry calls both
                    // propagate may-write; they are bounded by this request.
                    let callee = match instruction.kind {
                        InstructionKind::Call { function, .. } => Some(function.0 as usize),
                        _ => target.and_then(|ty| {
                            let ty = if let Type::Address { inner, .. } = ty {
                                inner.as_ref()
                            } else {
                                ty
                            };
                            if let Type::Closure(id) = ty {
                                module
                                    .closures
                                    .get(id.0 as usize)
                                    .map(|c| c.function.0 as usize)
                            } else {
                                None
                            }
                        }),
                    };
                    if let Some(callee) = callee {
                        budget.spend(1, "Product callable may-write lookup")?;
                        effect |= *writes
                            .get(callee)
                            .ok_or_else(|| bad("Product callable effect target is absent"))?;
                    }
                    if effect && !writes[index] {
                        writes[index] = true;
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    Ok(writes)
}

fn product_parameter_of(
    function: &TypedFunction,
    root: Root,
    budget: &mut Budget,
) -> CompileResult<Option<u32>> {
    match root.position() {
        Root::Parameter(index) => Ok(Some(index)),
        Root::Local(local) | Root::Field(local, _) | Root::Projection(local, ..) => {
            budget.spend(function.parameters.len(), "Product actual parameter lookup")?;
            Ok(function
                .parameters
                .iter()
                .position(|id| *id == local)
                .map(|n| n as u32))
        }
        _ => Ok(None),
    }
}

fn parameter_product_windows(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    holds: &[Vec<ValueHold>],
    budget: &mut Budget,
) -> CompileResult<()> {
    let count = module.functions.len();
    budget.spend(
        count.saturating_mul(6),
        "Product parameter may summary tables",
    )?;
    let mut summaries = vec![Roots::new(); count];
    for (index, (f, (_, reachable))) in module.functions.iter().zip(graphs).enumerate() {
        for (block, live) in f.blocks.iter().zip(reachable) {
            budget.spend(1, "Product return summary block")?;
            if *live
                && matches!(f.return_type, Type::Address { .. })
                && let Terminator::Return(Some(value)) = block.terminator
            {
                for root in &holds[index][value.0 as usize].roots {
                    budget.spend(1, "Product return summary origin")?;
                    if let HoldRoot::Parameter(parameter) = root {
                        summaries[index].insert(Root::Parameter(*parameter));
                    }
                }
            }
        }
    }
    budget.spend(
        module.closures.len(),
        "Product Store closure dependency table",
    )?;
    let mut store_closures = vec![false; module.closures.len()];
    loop {
        let mut changed = false;
        for (index, closure) in module.closures.iter().enumerate() {
            budget.spend(
                closure.captures.len().saturating_add(1),
                "Product Store closure dependencies",
            )?;
            let mut store = false;
            for capture in &closure.captures {
                store |= product_store_capture_dependency(capture, &store_closures, budget)?;
            }
            if store && !store_closures[index] {
                store_closures[index] = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let callable_writes = product_may_store_write(module, &store_closures, budget)?;
    let mut private = Vec::with_capacity(count);
    let mut owned_known = Vec::with_capacity(count);
    for (f, (predecessors, reachable)) in module.functions.iter().zip(graphs) {
        let mut values = Vec::new();
        let mut element_windows = false;
        for (block, live) in f.blocks.iter().zip(reachable) {
            if !live {
                continue;
            }
            budget.spend(block.instructions.len(), "Product Store window source scan")?;
            for instruction in &block.instructions {
                element_windows |= matches!(
                    instruction.kind,
                    InstructionKind::OwnedStoreBorrow { .. }
                        | InstructionKind::OwnedStoreBorrowMut { .. }
                ) || (matches!(instruction.kind, InstructionKind::Call { .. })
                    && instruction.result.is_some_and(|result| {
                        matches!(f.values[result.0 as usize], Type::Address { .. })
                    }));
            }
        }
        infer_with_private(
            module,
            &summaries,
            f,
            predecessors,
            reachable,
            element_windows,
            budget,
            Some(&mut values),
        )?;
        // Reconstruct owned Store lineage without treating an empty source as
        // fresh. Only known constructors, checked Create results, forwarded
        // owned arguments, and complete aggregates can establish a source.
        budget.spend(
            f.locals
                .len()
                .saturating_mul(5)
                .saturating_add(f.values.len().saturating_mul(2)),
            "Product owned Store provenance tables",
        )?;
        let mut owned = vec![Roots::new(); f.locals.len()];
        let mut local_known = vec![false; f.locals.len()];
        let mut value_known = vec![false; f.values.len()];
        let mut stores = vec![Vec::<ValueId>::new(); f.locals.len()];
        let mut field_stores = vec![BTreeMap::<u32, Vec<ValueId>>::new(); f.locals.len()];
        let mut value_fields = vec![BTreeMap::<u32, Roots>::new(); f.values.len()];
        let mut local_fields = vec![BTreeMap::<u32, Roots>::new(); f.locals.len()];
        // In a straight-line function the preceding window pass checked each
        // move at its actual program point. A field taken after that check is
        // the current unique field position, including after a refill. Do not
        // use this shortcut for branches or loops.
        let current_field_positions = element_windows
            && f.blocks.len() == 1
            && matches!(f.blocks[0].terminator, Terminator::Return(_));
        let mut formal = vec![false; f.locals.len()];
        for (position, local) in f.parameters.iter().enumerate() {
            budget.spend(1, "Product owned Store parameter scan")?;
            if product_owned_store_dependency(
                &f.locals[local.0 as usize].ty,
                &store_closures,
                budget,
            )? {
                formal[local.0 as usize] = true;
                owned[local.0 as usize].insert(Root::Parameter(position as u32));
            }
        }
        for (block, live) in f.blocks.iter().zip(reachable) {
            if !live {
                continue;
            }
            budget.spend(
                block.instructions.len(),
                "Product owned Store assignment scan",
            )?;
            for instruction in &block.instructions {
                if let InstructionKind::StorePackageField {
                    local,
                    index,
                    value,
                } = instruction.kind
                {
                    budget.spend(1, "Product field assignment entry")?;
                    field_stores[local.0 as usize]
                        .entry(index)
                        .or_default()
                        .push(value);
                }
                if let InstructionKind::Store { local, value } = instruction.kind
                    && product_owned_store_dependency(
                        &f.locals[local.0 as usize].ty,
                        &store_closures,
                        budget,
                    )?
                {
                    budget.spend(1, "Product owned Store assignment entry")?;
                    stores[local.0 as usize].push(value);
                }
            }
        }
        loop {
            let mut changed = false;
            for local in 0..f.locals.len() {
                budget.spend(
                    stores[local].len().saturating_add(1),
                    "Product owned Store local proof",
                )?;
                let has_source = formal[local] || !stores[local].is_empty();
                if has_source
                    && stores[local]
                        .iter()
                        .all(|value| value_known[value.0 as usize])
                    && !local_known[local]
                {
                    local_known[local] = true;
                    changed = true;
                }
                let fields = match &f.locals[local].ty {
                    Type::Package { fields, .. } => fields,
                    _ => continue,
                };
                for (field, _) in fields.iter().enumerate() {
                    budget.spend(
                        stores[local].len().saturating_add(1),
                        "Product field source scan",
                    )?;
                    let field = field as u32;
                    let assignments = field_stores[local]
                        .get(&field)
                        .map(Vec::as_slice)
                        .unwrap_or(&[]);
                    budget.spend(assignments.len(), "Product field replacement scan")?;
                    let complete =
                        (formal[local] || !stores[local].is_empty() || !assignments.is_empty())
                            && stores[local]
                                .iter()
                                .all(|value| value_fields[value.0 as usize].contains_key(&field))
                            && assignments
                                .iter()
                                .all(|value| value_known[value.0 as usize]);
                    if complete {
                        changed |= !local_fields[local].contains_key(&field);
                        let entry = local_fields[local].entry(field).or_default();
                        for value in &stores[local] {
                            changed |=
                                extend(entry, &value_fields[value.0 as usize][&field], budget)?;
                        }
                        for value in assignments {
                            changed |= extend(entry, &values[value.0 as usize], budget)?;
                        }
                    }
                }
            }
            for (block, live) in f.blocks.iter().zip(reachable) {
                if !live {
                    continue;
                }
                budget.spend(
                    block.instructions.len(),
                    "Product owned Store provenance scan",
                )?;
                for instruction in &block.instructions {
                    if let InstructionKind::Store { local, value } = instruction.kind
                        && product_owned_store_dependency(
                            &f.locals[local.0 as usize].ty,
                            &store_closures,
                            budget,
                        )?
                        && value_known[value.0 as usize]
                    {
                        let source = &values[value.0 as usize];
                        if source.is_empty() {
                            budget.lookup(
                                owned[local.0 as usize].len(),
                                "Product fresh Store actual slot",
                            )?;
                            changed |= owned[local.0 as usize].insert(Root::Local(local));
                        } else {
                            changed |= extend(&mut owned[local.0 as usize], source, budget)?;
                        }
                    }
                    let Some(result) = instruction.result else {
                        continue;
                    };
                    match &instruction.kind {
                        InstructionKind::MakePackage { fields } => {
                            budget.spend(fields.len(), "Product value field source scan")?;
                            for (field, value) in fields.iter().enumerate() {
                                if value_known[value.0 as usize] {
                                    changed |= !value_fields[result.0 as usize]
                                        .contains_key(&(field as u32));
                                    let entry = value_fields[result.0 as usize]
                                        .entry(field as u32)
                                        .or_default();
                                    changed |= extend(entry, &values[value.0 as usize], budget)?;
                                }
                            }
                        }
                        InstructionKind::TakeLocal { local } => {
                            budget.spend(
                                local_fields[local.0 as usize].len(),
                                "Product moved field source scan",
                            )?;
                            for (field, roots) in &local_fields[local.0 as usize] {
                                changed |= !value_fields[result.0 as usize].contains_key(field);
                                let entry =
                                    value_fields[result.0 as usize].entry(*field).or_default();
                                changed |= extend(entry, roots, budget)?;
                            }
                        }
                        _ => {}
                    }
                    if !product_owned_store_dependency(
                        &f.values[result.0 as usize],
                        &store_closures,
                        budget,
                    )? {
                        continue;
                    }
                    let mut source = Roots::new();
                    let mut children: &[ValueId] = &[];
                    let known = match &instruction.kind {
                        InstructionKind::OwnedStoreNew { .. } => true,
                        InstructionKind::TakeLocal { local } if local_known[local.0 as usize] => {
                            budget.spend(1, "Product Store current move position")?;
                            source.insert(Root::Local(*local));
                            extend(&mut source, &owned[local.0 as usize], budget)?;
                            true
                        }
                        InstructionKind::TakePackageField { local, index }
                            if local_known[local.0 as usize]
                                && (current_field_positions
                                    || local_fields[local.0 as usize].contains_key(index)) =>
                        {
                            budget.spend(1, "Product Store field move origin")?;
                            source.insert(Root::Field(*local, *index));
                            if !current_field_positions {
                                extend(
                                    &mut source,
                                    &local_fields[local.0 as usize][index],
                                    budget,
                                )?;
                            }
                            true
                        }
                        InstructionKind::MakePackage { fields }
                        | InstructionKind::MakeAggregate { fields }
                        | InstructionKind::MakeVariant { fields, .. }
                        | InstructionKind::MakeOwnedArray { elements: fields } => {
                            children = fields;
                            true
                        }
                        InstructionKind::MakeErrorUnion { value, .. } => {
                            children = std::slice::from_ref(value);
                            true
                        }
                        InstructionKind::Call {
                            function,
                            arguments,
                        } => {
                            let callee = module
                                .functions
                                .get(function.0 as usize)
                                .ok_or_else(|| bad("Product owned result callee is absent"))?;
                            if callee.return_type != f.values[result.0 as usize] {
                                false
                            } else if callee.result_mode == PassingMode::Create {
                                true
                            } else if callee.result_mode == PassingMode::Give {
                                children = arguments;
                                true
                            } else {
                                false
                            }
                        }
                        _ => false,
                    };
                    if !known {
                        continue;
                    }
                    let mut relevant = false;
                    let mut complete = true;
                    budget.spend(children.len(), "Product Store aggregate source operands")?;
                    for value in children {
                        if product_owned_store_dependency(
                            &f.values[value.0 as usize],
                            &store_closures,
                            budget,
                        )? {
                            relevant = true;
                            complete &= value_known[value.0 as usize];
                            extend(&mut source, &values[value.0 as usize], budget)?;
                        }
                    }
                    // A Give result must actually have a known owned source.
                    if matches!(instruction.kind, InstructionKind::Call { .. })
                        && !children.is_empty()
                        && !relevant
                    {
                        complete = false;
                    }
                    if complete {
                        if !value_known[result.0 as usize] {
                            value_known[result.0 as usize] = true;
                            changed = true;
                        }
                        changed |= extend(&mut values[result.0 as usize], &source, budget)?;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        if current_field_positions {
            budget.spend(field_stores.len(), "Product current field locals")?;
            for assignments in &field_stores {
                budget.spend(assignments.len(), "Product current field entries")?;
                for values_in_field in assignments.values() {
                    budget.spend(values_in_field.len(), "Product current field source proof")?;
                    for value in values_in_field {
                        if product_owned_store_dependency(
                            &f.values[value.0 as usize],
                            &store_closures,
                            budget,
                        )? && !value_known[value.0 as usize]
                        {
                            return Err(bad("Product field replacement has unknown Store origin"));
                        }
                    }
                }
            }
        }
        owned_known.push(value_known);
        private.push(values);
    }
    let mut captures = vec![BTreeSet::<u32>::new(); count];
    let mut effects = vec![BTreeSet::<u32>::new(); count];
    let mut unknown_effects = vec![false; count];
    let roots = |function: usize, value: ValueId| -> CompileResult<&Roots> {
        private
            .get(function)
            .and_then(|row| row.get(value.0 as usize))
            .ok_or_else(|| bad("Product call actual Hold is absent"))
    };
    for (index, (f, (_, reachable))) in module.functions.iter().zip(graphs).enumerate() {
        for (position, local) in f.parameters.iter().enumerate() {
            budget.spend(1, "Product Store consuming parameter scan")?;
            if product_owned_store_dependency(
                &f.locals[local.0 as usize].ty,
                &store_closures,
                budget,
            )? {
                budget.lookup(
                    effects[index].len(),
                    "Product Store consuming parameter insertion",
                )?;
                effects[index].insert(position as u32);
            }
        }
        for (block, live) in f.blocks.iter().zip(reachable) {
            if !live {
                continue;
            }
            budget.spend(
                block.instructions.len(),
                "Product parameter direct summary scan",
            )?;
            for instruction in &block.instructions {
                if let InstructionKind::MakeClosure {
                    captures: actual, ..
                } = &instruction.kind
                {
                    budget.spend(actual.len(), "Product parameter captured operands")?;
                    for value in actual {
                        if !f.values[value.0 as usize].is_package_address() {
                            continue;
                        }
                        for root in roots(index, *value)? {
                            budget.spend(1, "Product parameter capture origin")?;
                            if let Some(parameter) = product_parameter_of(f, *root, budget)? {
                                budget.lookup(
                                    captures[index].len(),
                                    "Product capture summary insertion",
                                )?;
                                captures[index].insert(parameter);
                            }
                        }
                    }
                }
                let callback = match instruction.kind {
                    InstructionKind::CallClosureShared { callee, .. }
                    | InstructionKind::CallClosureMut { callee, .. } => {
                        Some(&f.locals[callee.0 as usize].ty)
                    }
                    InstructionKind::CallClosure { callee, .. }
                    | InstructionKind::CallClosureOnce { callee, .. }
                    | InstructionKind::WithScopedThread { body: callee, .. }
                    | InstructionKind::CallClosureBorrowed { callee, .. } => {
                        Some(&f.values[callee.0 as usize])
                    }
                    _ => None,
                };
                if let Some(ty) = callback {
                    let ty = if let Type::Address { inner, .. } = ty {
                        inner.as_ref()
                    } else {
                        ty
                    };
                    if let Type::Closure(id) = ty {
                        budget.spend(1, "Product Store called closure lookup")?;
                        let definition = module
                            .closures
                            .get(id.0 as usize)
                            .ok_or_else(|| bad("Product Store called closure is absent"))?;
                        unknown_effects[index] |= store_closures[id.0 as usize]
                            && *callable_writes
                                .get(definition.function.0 as usize)
                                .ok_or_else(|| bad("Product Store closure entry is absent"))?;
                    }
                }
                let uncertain_arguments = match &instruction.kind {
                    InstructionKind::ForeignCall { arguments, .. }
                    | InstructionKind::CallIndirect { arguments, .. }
                    | InstructionKind::CallClosure { arguments, .. }
                    | InstructionKind::CallClosureOnce { arguments, .. }
                    | InstructionKind::CallClosureBorrowed { arguments, .. }
                    | InstructionKind::CallClosureShared { arguments, .. }
                    | InstructionKind::CallClosureMut { arguments, .. } => arguments.as_slice(),
                    _ => &[],
                };
                budget.spend(
                    uncertain_arguments.len(),
                    "Product unknown call Store operands",
                )?;
                for value in uncertain_arguments {
                    let ty = &f.values[value.0 as usize];
                    let owned = product_owned_store_dependency(ty, &store_closures, budget)?;
                    if !dynamic_store::is_carrier(ty) && !owned {
                        continue;
                    }
                    let actual_roots = roots(index, *value)?;
                    unknown_effects[index] |=
                        actual_roots.is_empty() || (owned && !owned_known[index][value.0 as usize]);
                    for root in actual_roots {
                        budget.spend(1, "Product unknown call Store origin")?;
                        unknown_effects[index] |= matches!(root.position(), Root::Capture(_));
                        if let Some(parameter) = product_parameter_of(f, *root, budget)? {
                            budget.lookup(
                                effects[index].len(),
                                "Product unknown call effect insertion",
                            )?;
                            effects[index].insert(parameter);
                        }
                    }
                }
                let store = match instruction.kind {
                    InstructionKind::OwnedStoreReserve { store, .. }
                    | InstructionKind::OwnedStorePush { store, .. }
                    | InstructionKind::OwnedStorePop { store }
                    | InstructionKind::OwnedStoreBorrowMut { store, .. } => Some(store),
                    _ => None,
                };
                if let Some(store) = store {
                    let actual_roots = roots(index, store)?;
                    unknown_effects[index] |= actual_roots.is_empty();
                    for root in actual_roots {
                        budget.spend(1, "Product Store mutation origin")?;
                        unknown_effects[index] |= matches!(root.position(), Root::Capture(_));
                        if let Some(parameter) = product_parameter_of(f, *root, budget)? {
                            budget
                                .lookup(effects[index].len(), "Product Store effect insertion")?;
                            effects[index].insert(parameter);
                        }
                    }
                }
            }
        }
    }
    // Monotone actual-argument substitution covers recursion and intermediate
    // ordinary helpers; every scan and each candidate insertion is prepaid.
    loop {
        let mut changed = false;
        for (index, (f, (_, reachable))) in module.functions.iter().zip(graphs).enumerate() {
            for (block, live) in f.blocks.iter().zip(reachable) {
                if !live {
                    continue;
                }
                budget.spend(
                    block.instructions.len(),
                    "Product parameter call summary scan",
                )?;
                for instruction in &block.instructions {
                    let InstructionKind::Call {
                        function,
                        arguments,
                    } = &instruction.kind
                    else {
                        continue;
                    };
                    let callee = function.0 as usize;
                    let (Some(child_captures), Some(child_effects)) =
                        (captures.get(callee), effects.get(callee))
                    else {
                        return Err(bad("Product parameter callee is absent"));
                    };
                    budget.spend(
                        child_captures.len().saturating_add(child_effects.len()),
                        "Product call summary snapshot",
                    )?;
                    let child_captures = child_captures.clone();
                    let child_effects = child_effects.clone();
                    if unknown_effects[callee] && !unknown_effects[index] {
                        unknown_effects[index] = true;
                        changed = true;
                    }
                    for (child, target) in [
                        (child_captures, &mut captures[index]),
                        (child_effects, &mut effects[index]),
                    ] {
                        for parameter in child {
                            let actual = arguments
                                .get(parameter as usize)
                                .ok_or_else(|| bad("Product call parameter is absent"))?;
                            let actual_roots = roots(index, *actual)?;
                            let unproven_owner = product_owned_store_dependency(
                                &f.values[actual.0 as usize],
                                &store_closures,
                                budget,
                            )? && !owned_known[index][actual.0 as usize];
                            if (actual_roots.is_empty() || unproven_owner)
                                && !unknown_effects[index]
                            {
                                unknown_effects[index] = true;
                                changed = true;
                            }
                            for root in actual_roots {
                                budget.spend(1, "Product call origin substitution")?;
                                if matches!(root.position(), Root::Capture(_))
                                    && !unknown_effects[index]
                                {
                                    unknown_effects[index] = true;
                                    changed = true;
                                }
                                if let Some(parent) = product_parameter_of(f, *root, budget)? {
                                    budget
                                        .lookup(target.len(), "Product call summary insertion")?;
                                    changed |= target.insert(parent);
                                }
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
    for (index, (f, (_, reachable))) in module.functions.iter().zip(graphs).enumerate() {
        if !captures[index].is_empty() && unknown_effects[index] {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "shared Product capture has a Store effect with unknown parent origin",
                Some(f.span),
            )]);
        }
        for (block, live) in f.blocks.iter().zip(reachable) {
            if !live {
                continue;
            }
            budget.spend(block.instructions.len(), "Product actual call window scan")?;
            for instruction in &block.instructions {
                let InstructionKind::Call {
                    function,
                    arguments,
                } = &instruction.kind
                else {
                    continue;
                };
                let callee = function.0 as usize;
                let child_captures = captures
                    .get(callee)
                    .ok_or_else(|| bad("Product call target is absent"))?;
                let child_effects = effects
                    .get(callee)
                    .ok_or_else(|| bad("Product call effects are absent"))?;
                if !child_captures.is_empty() && unknown_effects[callee] {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Analyze,
                        "shared Product call has a Store-capturing callable with unknown parent origin",
                        Some(instruction.span),
                    )]);
                }
                for captured in child_captures {
                    let actual = arguments
                        .get(*captured as usize)
                        .ok_or_else(|| bad("Product capture actual is absent"))?;
                    let captured_roots = roots(index, *actual)?;
                    if captured_roots.is_empty() {
                        return Err(bad("shared Product argument has no actual parent origin"));
                    }
                    for effect in child_effects {
                        let changed = arguments
                            .get(*effect as usize)
                            .ok_or_else(|| bad("Product Store actual is absent"))?;
                        let changed_roots = roots(index, *changed)?;
                        let unproven_owner = product_owned_store_dependency(
                            &f.values[changed.0 as usize],
                            &store_closures,
                            budget,
                        )? && !owned_known[index][changed.0 as usize];
                        if changed_roots.is_empty() || unproven_owner {
                            return Err(bad("Store effect argument has no actual parent origin"));
                        }
                        if overlaps(captured_roots, changed_roots, budget)? {
                            return Err(window_conflict(
                                instruction,
                                "Store effect overlaps a captured Product parameter during this call",
                            ));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// Return contracts form a finite monotone lattice of parameter indices. An
/// empty approximation is never authority: the verifier rejects ungrounded
/// address returns after the independent fixed point.
pub(super) fn infer_module(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    store_profiles: &[bool],
    budget: &mut Budget,
) -> CompileResult<Vec<Vec<ValueHold>>> {
    let result = infer_module_inner(module, graphs, store_profiles, budget)?;
    super::lifetimes::returns(module, graphs, &result, budget)?;
    if !budget.shared_product_capture {
        return Ok(result);
    }
    // Only inspect the producer's own completed solution. In particular, an
    // empty intermediate return-origin approximation is never a rejection or
    // proof before the module-level fixed point has converged.
    budget.spend(module.functions.len(), "shared Product capture functions")?;
    let mut parameter_capture = false;
    for ((f, (_, reachable)), values) in module.functions.iter().zip(graphs).zip(&result) {
        budget.spend(f.blocks.len(), "shared Product capture blocks")?;
        for (block, live) in f.blocks.iter().zip(reachable) {
            if !live {
                continue;
            }
            budget.spend(
                block.instructions.len(),
                "shared Product capture instructions",
            )?;
            for instruction in &block.instructions {
                let InstructionKind::MakeClosure { captures, .. } = &instruction.kind else {
                    continue;
                };
                budget.spend(captures.len(), "shared Product capture operands")?;
                for value in captures {
                    if !f.values[value.0 as usize].is_package_address() {
                        continue;
                    }
                    let origins = &values[value.0 as usize].roots;
                    budget.spend(origins.len(), "shared Product capture origins")?;
                    if origins.is_empty() {
                        return Err(vec![Diagnostic::unsupported(
                            Phase::Analyze,
                            "shared Product capture requires proven ordinary local Product origins",
                            Some(instruction.span),
                        )]);
                    }
                    for origin in origins {
                        let actual = match origin {
                            HoldRoot::Local(local) => &f.locals[local.0 as usize].ty,
                            HoldRoot::Parameter(index) => {
                                parameter_capture = true;
                                budget.spend(1, "shared Product actual parameter position")?;
                                let local = f.parameters.get(*index as usize).ok_or_else(|| {
                                    bad("shared Product capture parameter is absent")
                                })?;
                                match &f.locals[local.0 as usize].ty {
                                    Type::Address {
                                        mutable: false,
                                        inner,
                                    } => inner.as_ref(),
                                    _ => {
                                        return Err(vec![Diagnostic::unsupported(
                                            Phase::Analyze,
                                            "shared Product capture parameter requires an actual shared Product address",
                                            Some(instruction.span),
                                        )]);
                                    }
                                }
                            }
                            _ => {
                                return Err(vec![Diagnostic::unsupported(
                                    Phase::Analyze,
                                    "shared Product capture requires proven ordinary local Product origins",
                                    Some(instruction.span),
                                )]);
                            }
                        };
                        // A coarsened field root is allowed only when its entire
                        // real carrier is an ordinary Product, which excludes
                        // all Store fields. Field-name inequality grants nothing.
                        if !matches!(actual, Type::Package { .. }) {
                            return Err(vec![Diagnostic::unsupported(
                                Phase::Analyze,
                                "shared Product capture requires proven ordinary local Product origins",
                                Some(instruction.span),
                            )]);
                        }
                        super::resources::product::admit(actual, budget)?;
                    }
                }
            }
        }
    }
    if parameter_capture {
        parameter_product_windows(module, graphs, &result, budget)?;
    }
    Ok(result)
}

// Preflight records these private bits during paid complete-type admission.
// Index by the actual function table position, never producer FunctionId.
fn ordinary_store_function(index: usize, budget: &mut Budget) -> CompileResult<bool> {
    if budget.ordinary_store_functions.is_empty() {
        return Ok(false);
    }
    budget.spend(1, "ordinary Product Store function flag lookup")?;
    budget
        .ordinary_store_functions
        .get(index)
        .copied()
        .ok_or_else(|| bad("ordinary Product Store function profile is absent"))
}

fn infer_module_inner(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    store_profiles: &[bool],
    budget: &mut Budget,
) -> CompileResult<Vec<Vec<ValueHold>>> {
    capture_inventory(module, budget)?;
    if !module
        .functions
        .iter()
        .any(|f| matches!(f.return_type, Type::Address { .. }))
    {
        return module
            .functions
            .iter()
            .zip(graphs)
            .enumerate()
            .map(|(index, (f, (preds, reachable)))| {
                infer(
                    module,
                    &[],
                    f,
                    preds,
                    reachable,
                    f.environment.is_some_and(|id| {
                        store_profiles.get(id.0 as usize).copied().unwrap_or(false)
                    }) || ordinary_store_function(index, budget)?,
                    budget,
                )
            })
            .collect();
    }
    let count = module.functions.len();
    // Only executable consumers of a return-origin summary create an edge.
    // Local CFG solving stays independent of this module-level scheduling.
    budget.product(count, 5, 1, "return frontier tables")?;
    let mut callers = vec![Vec::<usize>::new(); count];
    for (caller, (function, (_, reachable))) in module.functions.iter().zip(graphs).enumerate() {
        for (block, live) in function.blocks.iter().zip(reachable) {
            budget.spend(1, "return frontier block")?;
            if !live {
                continue;
            }
            for instruction in &block.instructions {
                budget.spend(1, "return frontier instruction")?;
                if let InstructionKind::Call {
                    function: callee, ..
                } = instruction.kind
                    && instruction.result.is_some_and(|value| {
                        matches!(
                            function.values.get(value.0 as usize),
                            Some(Type::Address { .. })
                        )
                    })
                {
                    let users = callers
                        .get_mut(callee.0 as usize)
                        .ok_or_else(|| bad("call refers to an absent function"))?;
                    // Callers are visited in increasing order, so repeated calls
                    // in one body can be deduplicated without sorting or a set.
                    if users.last() != Some(&caller) {
                        budget.spend(1, "return frontier edge")?;
                        users.push(caller);
                    }
                }
            }
        }
    }
    let mut summaries = vec![Roots::new(); count];
    let mut results: Vec<Option<Vec<ValueHold>>> = (0..count).map(|_| None).collect();
    let mut pending: VecDeque<usize> = (0..count).collect();
    let mut queued = vec![true; count];
    while let Some(index) = pending.pop_front() {
        budget.spend(1, "return frontier pop")?;
        queued[index] = false;
        #[cfg(test)]
        TEST_SOLVES.with(|n| n.set(n.get() + 1));
        let function = &module.functions[index];
        let (predecessors, reachable) = &graphs[index];
        let values = infer(
            module,
            &summaries,
            function,
            predecessors,
            reachable,
            function
                .environment
                .is_some_and(|id| store_profiles.get(id.0 as usize).copied().unwrap_or(false))
                || ordinary_store_function(index, budget)?,
            budget,
        )?;
        let mut changed = false;
        if matches!(function.return_type, Type::Address { .. }) {
            let mut result = Roots::new();
            for (block, live) in function.blocks.iter().zip(reachable) {
                budget.spend(1, "return contract block")?;
                if *live && let Terminator::Return(Some(value)) = block.terminator {
                    let roots = values
                        .get(value.0 as usize)
                        .ok_or_else(|| bad("return outside Hold table"))?;
                    for root in &roots.roots {
                        budget.spend(1, "return parameter origin")?;
                        if let HoldRoot::Parameter(index) = root {
                            result.insert(Root::Parameter(*index));
                        }
                    }
                }
            }
            changed = extend(&mut summaries[index], &result, budget)?;
        }
        results[index] = Some(values);
        if changed {
            budget.spend(callers[index].len(), "return frontier dependents")?;
            for &caller in &callers[index] {
                if !queued[caller] {
                    budget.spend(1, "return frontier push")?;
                    queued[caller] = true;
                    pending.push_back(caller);
                }
            }
        }
    }
    results
        .into_iter()
        .map(|result| result.ok_or_else(|| bad("return frontier omitted a function")))
        .collect()
}

#[cfg(test)]
thread_local! {
    static TEST_SOLVES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod frontier_tests {
    use super::*;

    fn fixture(edges: &[Option<usize>], grounded: &[bool], swap: bool) -> TypedModule {
        let address = Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        };
        let span = zil_base::Span::new(zil_base::FileId(0), 0, 1);
        let instruction = |result, kind| Instruction {
            result: Some(ValueId(result)),
            kind,
            span,
        };
        let functions = edges
            .iter()
            .enumerate()
            .map(|(i, target)| {
                let mut values = vec![address.clone(), address.clone(), Type::Bool];
                let instructions = vec![
                    instruction(0, InstructionKind::Load(LocalId(0))),
                    instruction(1, InstructionKind::Load(LocalId(1))),
                    instruction(2, InstructionKind::Load(LocalId(2))),
                ];
                let mut blocks = vec![BasicBlock {
                    id: BlockId(0),
                    instructions,
                    terminator: Terminator::Return(Some(ValueId(0))),
                }];
                if let Some(target) = target {
                    values.push(address.clone());
                    let arguments = if swap {
                        vec![ValueId(1), ValueId(0), ValueId(2)]
                    } else {
                        vec![ValueId(0), ValueId(1), ValueId(2)]
                    };
                    let call = instruction(
                        3,
                        InstructionKind::Call {
                            function: FunctionId(*target as u32),
                            arguments,
                        },
                    );
                    if grounded[i] {
                        blocks[0].terminator = Terminator::Branch {
                            condition: ValueId(2),
                            then_block: BlockId(1),
                            else_block: BlockId(2),
                        };
                        blocks.push(BasicBlock {
                            id: BlockId(1),
                            instructions: vec![],
                            terminator: Terminator::Return(Some(ValueId(0))),
                        });
                        blocks.push(BasicBlock {
                            id: BlockId(2),
                            instructions: vec![call],
                            terminator: Terminator::Return(Some(ValueId(3))),
                        });
                    } else {
                        blocks[0].instructions.push(call);
                        blocks[0].terminator = Terminator::Return(Some(ValueId(3)));
                    }
                }
                TypedFunction {
                    reference_return_bound: None,
                    callable_return_contract: None,
                    callable_contract: None,
                    result_mode: zil_ir::PassingMode::Infer,
                    id: FunctionId(i as u32),
                    name: format!("f{i}"),
                    foreign: None,
                    environment: None,
                    parameters: vec![LocalId(0), LocalId(1), LocalId(2)],
                    return_type: address.clone(),
                    locals: vec![address.clone(), address.clone(), Type::Bool]
                        .into_iter()
                        .enumerate()
                        .map(|(n, ty)| LocalDecl {
                            id: LocalId(n as u32),
                            name: format!("p{n}"),
                            ty,
                            mutable: false,
                            span,
                        })
                        .collect(),
                    values,
                    blocks,
                    entry: BlockId(0),
                    span,
                }
            })
            .collect();
        TypedModule {
            resources: Vec::new(),
            schema_version: zil_base::SCHEMA_VERSION,
            module_id: zil_base::ModuleId(0),
            source_fingerprint: zil_base::Fingerprint::ZERO,
            target: zil_base::TargetProfile::default(),
            functions,
            closures: vec![],
            unsupported: vec![],
        }
    }
    fn slow(
        module: &TypedModule,
        graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
        budget: &mut Budget,
    ) -> CompileResult<Vec<Vec<ValueHold>>> {
        if !module
            .functions
            .iter()
            .any(|f| matches!(f.return_type, Type::Address { .. }))
        {
            return module
                .functions
                .iter()
                .zip(graphs)
                .map(|(f, (preds, reachable))| {
                    infer(module, &[], f, preds, reachable, false, budget)
                })
                .collect();
        }
        budget.spend(module.functions.len(), "return contract table")?;
        let mut summaries = vec![Roots::new(); module.functions.len()];
        loop {
            let mut changed = false;
            let mut results = Vec::with_capacity(module.functions.len());
            for (index, function) in module.functions.iter().enumerate() {
                let (predecessors, reachable) = &graphs[index];
                TEST_SOLVES.with(|n| n.set(n.get() + 1));
                let values = infer(
                    module,
                    &summaries,
                    function,
                    predecessors,
                    reachable,
                    false,
                    budget,
                )?;
                if matches!(function.return_type, Type::Address { .. }) {
                    let mut result = Roots::new();
                    for (block, live) in function.blocks.iter().zip(reachable) {
                        budget.spend(1, "return contract block")?;
                        if *live && let Terminator::Return(Some(value)) = block.terminator {
                            let roots = values
                                .get(value.0 as usize)
                                .ok_or_else(|| bad("return outside Hold table"))?;
                            for root in &roots.roots {
                                budget.spend(1, "return parameter origin")?;
                                if let HoldRoot::Parameter(index) = root {
                                    result.insert(Root::Parameter(*index));
                                }
                            }
                        }
                    }
                    changed |= extend(&mut summaries[index], &result, budget)?;
                }
                results.push(values);
            }
            if !changed {
                return Ok(results);
            }
        }
    }

    fn graphs(module: &TypedModule) -> Vec<(Vec<Vec<usize>>, Vec<bool>)> {
        module
            .functions
            .iter()
            .map(|f| {
                if f.blocks.len() == 1 {
                    (vec![vec![]], vec![true])
                } else {
                    (vec![vec![], vec![0], vec![0]], vec![true; 3])
                }
            })
            .collect()
    }
    fn compare(module: &TypedModule) -> (usize, usize) {
        let graphs = graphs(module);
        TEST_SOLVES.with(|n| n.set(0));
        let fast =
            infer_module(module, &graphs, &[], &mut Budget::new(&Limits::default())).unwrap();
        let fast_visits = TEST_SOLVES.with(|n| n.get());
        TEST_SOLVES.with(|n| n.set(0));
        let reference = slow(module, &graphs, &mut Budget::new(&Limits::default())).unwrap();
        let slow_visits = TEST_SOLVES.with(|n| n.get());
        assert_eq!(fast, reference);
        (fast_visits, slow_visits)
    }
    #[test]
    fn frontier_matches_full_scan_on_cycles_components_and_parameter_permutations() {
        for seed in 0..24 {
            let count = 13;
            let edges: Vec<_> = (0..count)
                .map(|i| {
                    if (i + seed) % 7 == 0 {
                        None
                    } else {
                        Some((i * 7 + seed) % count)
                    }
                })
                .collect();
            let grounded: Vec<_> = (0..count).map(|i| (i + seed) % 3 == 0).collect();
            compare(&fixture(&edges, &grounded, seed % 2 == 0));
        }
        compare(&fixture(&[Some(1), Some(0)], &[false, false], true));
    }
    #[test]
    fn frontier_rechecks_only_relevant_functions_on_reverse_chain() {
        let n = 96;
        let edges: Vec<_> = (0..n).map(|i| (i + 1 < n).then_some(i + 1)).collect();
        let (fast, slow) = compare(&fixture(&edges, &vec![false; n], false));
        assert_eq!(fast, 2 * n - 1);
        assert_eq!(slow, n * (n + 1));
    }
    #[test]
    fn frontier_rejects_absent_target_and_zero_budget_without_panicking() {
        let module = fixture(&[Some(99)], &[false], false);
        let error = infer_module(
            &module,
            &graphs(&module),
            &[],
            &mut Budget::new(&Limits::default()),
        )
        .unwrap_err();
        assert_eq!(error[0].code, "E-ANALYZE-IR");
        let module = fixture(&[None], &[false], false);
        let limits = Limits {
            max_analysis_steps: 0,
            ..Limits::default()
        };
        let error =
            infer_module(&module, &graphs(&module), &[], &mut Budget::new(&limits)).unwrap_err();
        assert_eq!(error[0].category, zil_base::ResultCategory::ResourceLimit);
    }
}

#[cfg(test)]
mod product_parameter_dependency_tests {
    use super::*;

    #[test]
    fn nested_store_dependency_is_conservative_but_code_signatures_are_not_values() {
        let store = Type::Managed(Box::new(Type::OwnedStore {
            element: Box::new(Type::I32),
        }));
        let nested = Type::Package {
            name: "P".into(),
            fields: vec![(
                "items".into(),
                Type::Array {
                    element: Box::new(store.clone()),
                    length: 1,
                },
            )],
        };
        let address_child = Type::Address {
            mutable: false,
            inner: Box::new(Type::Closure(ClosureId(0))),
        };
        let pointer = Type::FunctionPointer {
            parameters: vec![store],
            result: Box::new(Type::Unit),
        };
        let defaults = Limits::default();
        let check = |steps| {
            let mut budget = Budget::new(&Limits {
                max_analysis_steps: steps,
                ..defaults.clone()
            });
            let flags = [true];
            let value = (
                product_store_capture_dependency(&nested, &flags, &mut budget)?,
                product_owned_store_dependency(&nested, &flags, &mut budget)?,
                product_store_capture_dependency(&address_child, &flags, &mut budget)?,
                product_owned_store_dependency(&address_child, &flags, &mut budget)?,
                product_store_capture_dependency(&pointer, &flags, &mut budget)?,
            );
            Ok::<_, Vec<Diagnostic>>((value, defaults.max_analysis_steps - budget.remaining()))
        };
        // Measure with the same request ceiling used by Budget; then test the
        // shared request boundary separately with a fresh budget each time.
        let (expected, exact) = check(defaults.max_analysis_steps).unwrap();
        assert_eq!(expected, (true, true, true, false, false));
        for steps in [exact, exact - 1, exact] {
            let mut budget = Budget::new(&Limits {
                max_analysis_steps: steps,
                ..defaults.clone()
            });
            let run = (|| {
                let flags = [true];
                Ok::<_, Vec<Diagnostic>>((
                    product_store_capture_dependency(&nested, &flags, &mut budget)?,
                    product_owned_store_dependency(&nested, &flags, &mut budget)?,
                    product_store_capture_dependency(&address_child, &flags, &mut budget)?,
                    product_owned_store_dependency(&address_child, &flags, &mut budget)?,
                    product_store_capture_dependency(&pointer, &flags, &mut budget)?,
                ))
            })();
            if steps == exact {
                assert_eq!(run.unwrap(), expected);
            } else {
                assert_eq!(
                    run.unwrap_err()[0].category,
                    zil_base::ResultCategory::ResourceLimit
                );
            }
        }
    }

    #[test]
    fn callable_store_may_effect_distinguishes_read_and_transitive_write() {
        use zil_base::{Fingerprint, ModuleId, SCHEMA_VERSION, Span, TargetProfile};
        let instruction = |kind| Instruction {
            result: None,
            kind,
            span: Span::default(),
        };
        let function = |id, instructions| TypedFunction {
            reference_return_bound: None,
            id: FunctionId(id),
            name: format!("f{id}"),
            foreign: None,
            environment: None,
            parameters: vec![],
            return_type: Type::Unit,
            result_mode: PassingMode::Infer,
            locals: vec![],
            values: vec![
                Type::Desc(Box::new(Type::OwnedStore {
                    element: Box::new(Type::I32),
                })),
                Type::I64,
            ],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions,
                terminator: Terminator::Return(None),
            }],
            entry: BlockId(0),
            span: Span::default(),
            callable_contract: None,
            callable_return_contract: None,
        };
        let mut module = TypedModule {
            schema_version: SCHEMA_VERSION,
            module_id: ModuleId(0),
            source_fingerprint: Fingerprint::ZERO,
            target: TargetProfile::host_llvm(),
            resources: vec![],
            closures: vec![],
            unsupported: vec![],
            functions: vec![
                function(
                    0,
                    vec![instruction(InstructionKind::OwnedStoreLen {
                        store: ValueId(0),
                    })],
                ),
                function(
                    1,
                    vec![instruction(InstructionKind::Call {
                        function: FunctionId(0),
                        arguments: vec![],
                    })],
                ),
            ],
        };
        let limits = Limits::default();
        assert_eq!(
            product_may_store_write(&module, &[], &mut Budget::new(&limits)).unwrap(),
            vec![false, false]
        );
        module.functions[0].blocks[0].instructions[0].kind = InstructionKind::OwnedStoreReserve {
            store: ValueId(0),
            capacity: ValueId(1),
        };
        assert_eq!(
            product_may_store_write(&module, &[], &mut Budget::new(&limits)).unwrap(),
            vec![true, true]
        );
        module.functions[0].values.push(Type::Package {
            name: "Owner".into(),
            fields: vec![(
                "items".into(),
                Type::Managed(Box::new(Type::OwnedStore {
                    element: Box::new(Type::I32),
                })),
            )],
        });
        module.functions[0].blocks[0].instructions[0].kind = InstructionKind::CallIndirect {
            callee: ValueId(1),
            arguments: vec![ValueId(2)],
        };
        assert_eq!(
            product_may_store_write(&module, &[], &mut Budget::new(&limits)).unwrap(),
            vec![true, true]
        );
        module.functions[0].blocks[0].instructions[0].kind = InstructionKind::CallIndirect {
            callee: ValueId(1),
            arguments: vec![ValueId(1)],
        };
        assert_eq!(
            product_may_store_write(&module, &[], &mut Budget::new(&limits)).unwrap(),
            vec![false, false]
        );
    }
}

fn inline_width(t:&Type,b:&mut Budget)->CompileResult<u32>{b.spend(1,"inline place interval")?;let mut n=1u32;let mut add=|t:&Type|->CompileResult<()>{n=n.checked_add(inline_width(t,b)?).ok_or_else(||vec![Diagnostic::limit(Phase::Analyze,"inline interval overflow",None)])?;Ok(())};match t{Type::Tuple(fs)=>for t in fs{add(t)?},Type::Record{fields,..}|Type::Package{fields,..}=>for(_,t)in fields{add(t)?},Type::Enum{variants,..}=>for(_,fs)in variants{for t in fs{add(t)?}},_=>{}}Ok(n)}
fn inline_root(f:&TypedFunction,local:LocalId,path:&[u32],b:&mut Budget)->CompileResult<Root>{
 let mut ty=&f.locals[local.0 as usize].ty;let mut at=0u32;for field in path{at=at.checked_add(1).ok_or_else(||bad("inline interval overflow"))?;
  ty=match ty{Type::Tuple(fs)=>{for t in fs.iter().take(*field as usize){at=at.checked_add(inline_width(t,b)?).ok_or_else(||bad("inline interval overflow"))?;}fs.get(*field as usize)},Type::Record{fields,..}=>{for(_,t)in fields.iter().take(*field as usize){at=at.checked_add(inline_width(t,b)?).ok_or_else(||bad("inline interval overflow"))?;}fields.get(*field as usize).map(|(_,t)|t)},_=>None}.ok_or_else(||bad("inline field outside actual aggregate"))?;
 }let end=at.checked_add(inline_width(ty,b)?).ok_or_else(||bad("inline interval overflow"))?;Ok(Root::Projection(local,at,end))
}
fn address_root(f:&TypedFunction,address:ValueId,path:&[VariantProjection],root:Root,b:&mut Budget)->CompileResult<Root>{
 let Type::Address{inner,..}=&f.values[address.0 as usize]else{return Err(bad("payload source is not an address"))};let mut ty=inner.as_ref();let mut at=0u32;
 for part in path{let Type::Enum{variants,..}=ty else{return Err(bad("payload interval crosses non-Enum"))};at=at.checked_add(1).ok_or_else(||bad("address interval overflow"))?;for(_,fs)in variants.iter().take(part.variant as usize){for t in fs{at=at.checked_add(inline_width(t,b)?).ok_or_else(||bad("address interval overflow"))?;}}
  let fs=&variants.get(part.variant as usize).ok_or_else(||bad("payload tag outside Enum"))?.1;for t in fs.iter().take(part.index as usize){at=at.checked_add(inline_width(t,b)?).ok_or_else(||bad("address interval overflow"))?;}ty=fs.get(part.index as usize).ok_or_else(||bad("payload field outside Enum"))?;
 }let end=at.checked_add(inline_width(ty,b)?).ok_or_else(||bad("address interval overflow"))?;
 Ok(match root{Root::Local(l)=>Root::Projection(l,at,end),Root::Parameter(p)=>Root::Window(HoldRoot::Parameter(p),at,end),Root::Projection(l,offset,limit)=>{let a=offset.checked_add(at).ok_or_else(||bad("address interval overflow"))?;let z=offset.checked_add(end).ok_or_else(||bad("address interval overflow"))?;if z>limit{return Err(bad("payload escapes its actual parent interval"))}Root::Projection(l,a,z)},Root::Window(origin,offset,limit)=>{let a=offset.checked_add(at).ok_or_else(||bad("address interval overflow"))?;let z=offset.checked_add(end).ok_or_else(||bad("address interval overflow"))?;if z>limit{return Err(bad("payload escapes its actual parent interval"))}Root::Window(origin,a,z)},root=>root})
}
