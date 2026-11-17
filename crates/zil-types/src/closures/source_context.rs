//! Formation from raw original-source context. These rows are not permission.
use super::*;
use zil_base::NodeId;

#[derive(Default)]
pub(super) struct Ledger {
    rows: BTreeMap<(FunctionId, NodeId), CoreClosureLifeContext>,
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Fixed {
    Header(StorageBinder),
    Local(LocalId),
}
#[derive(Clone, Copy)]
enum Key {
    Fixed(Fixed),
    Invocation(u32),
}
struct CaptureKeys {
    outer: Option<Fixed>,
    classes: Vec<Fixed>,
}
pub(super) struct Formation {
    proposal: StorageLifeContract,
    fixed: Vec<Fixed>,
    captures: Vec<CaptureKeys>,
}
fn invalid(span: Span, message: &str) -> Diagnostic {
    error("E-CLOSURE-CONTEXT", span, message)
}
fn pay(left: &mut usize, count: usize, span: Span) -> R<()> {
    *left = left
        .checked_sub(count)
        .ok_or_else(|| limit("source capture context work exceeded", span))?;
    Ok(())
}

/// Call before any raw Core snapshot. The metadata is flat; all vector/path
/// bounds and source kind restrictions precede copies or domain allocation.
pub(super) fn reserve(
    rows: &[CoreClosureLifeContext],
    limits: &Limits,
    left: &mut usize,
    copies: usize,
) -> R<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let span = Span::default();
    if rows.len() > limits.max_nodes {
        return Err(limit("source closure context count exceeded", span));
    }
    pay(left, rows.len(), span)?;
    for row in rows {
        for count in [
            row.parameters.len(),
            row.named_fixed.len(),
            row.inputs.len(),
            row.signature.bindings.len(),
            row.signature.classes.len(),
            row.identity.module_path.len(),
            row.identity.declaration_path.len(),
            row.identity.generation_path.len(),
        ] {
            if count > limits.max_nodes {
                return Err(limit("source closure context vector exceeded", span));
            }
            pay(left, count.saturating_mul(copies.saturating_add(1)), span)?;
        }
        if row
            .parameters
            .iter()
            .any(|p| *p != LifeParameterKind::Lifetime)
            || row.declaration.compiler
            || !row.identity.generation_path.is_empty()
        {
            return Err(unsupported(
                span,
                "source closure context requires an original life-only free-function Definition",
            ));
        }
        for text in std::iter::once(&row.identity.package)
            .chain(&row.identity.module_path)
            .chain(&row.identity.declaration_path)
        {
            pay(
                left,
                text.len()
                    .saturating_add(1)
                    .saturating_mul(copies.saturating_add(1)),
                span,
            )?;
        }
        pay(
            left,
            borrowed_fp::metadata_work(&row.signature).saturating_mul(copies.saturating_add(1)),
            span,
        )?;
        for path in row
            .signature
            .bindings
            .iter()
            .map(|r| r.position.path.len())
            .chain(row.signature.classes.iter().map(|r| r.position.path.len()))
            .chain(row.inputs.iter().map(|r| match &r.position {
                CoreClosureHeaderPosition::Borrow(p) => p.path.len(),
                CoreClosureHeaderPosition::Class(p) => p.path.len(),
            }))
        {
            if path > limits.max_call_depth {
                return Err(limit("source closure context path depth exceeded", span));
            }
            pay(
                left,
                path.saturating_add(1)
                    .saturating_mul(copies.saturating_add(1)),
                span,
            )?;
        }
    }
    Ok(())
}
fn qualified(ty: &Type) -> Option<(&StorageLifeShape, bool)> {
    let (ty, pointer) = if let Type::Address { inner, .. } = ty {
        (inner.as_ref(), true)
    } else {
        (ty, false)
    };
    if let Type::BorrowedStorage { life, .. } = ty {
        Some((life, pointer))
    } else {
        None
    }
}
fn actual_header(
    anchor: &CoreClosureInputAnchor,
    creator: FunctionRef<'_>,
    left: &mut usize,
    span: Span,
) -> R<StorageBinder> {
    let local = creator
        .parameters
        .get(anchor.parameter as usize)
        .ok_or_else(|| invalid(span, "context input is not an actual creator parameter"))?;
    let ty = &creator.locals[local.0 as usize].ty;
    let header = creator.storage_life.as_ref().ok_or_else(|| {
        unsupported(
            span,
            "source capture requires its complete actual creator header",
        )
    })?;
    let binder = match &anchor.position {
        CoreClosureHeaderPosition::Borrow(p) => {
            if p.base != StorageBase::Parameter(anchor.parameter)
                || !p.path.is_empty()
                || !matches!(ty, Type::Address { .. })
            {
                return Err(invalid(
                    span,
                    "context borrow does not select its actual input Address",
                ));
            }
            pay(left, header.bindings.len(), span)?;
            header
                .bindings
                .iter()
                .find(|r| r.position == *p)
                .map(|r| r.binder)
        }
        CoreClosureHeaderPosition::Class(p) => {
            let (shape, pointer) = qualified(ty)
                .ok_or_else(|| invalid(span, "context class has no actual qualified input"))?;
            if p.base != StorageBase::Parameter(anchor.parameter)
                || p.class >= shape.binder_count
                || if pointer {
                    p.path.as_slice() != [LifeTypeProjection::Pointee]
                } else {
                    !p.path.is_empty()
                }
            {
                return Err(invalid(
                    span,
                    "context class does not select its actual input boundary",
                ));
            }
            pay(left, header.classes.len().saturating_mul(2), span)?;
            header
                .classes
                .iter()
                .find(|r| r.position == *p)
                .map(|r| r.binder)
        }
    }
    .ok_or_else(|| {
        invalid(
            span,
            "context input position is absent from the actual header",
        )
    })?;
    if binder != anchor.header_binder
        || match binder {
            StorageBinder::Fixed(i) => i >= header.fixed_binder_count,
            StorageBinder::Invocation(i) => i >= header.invocation_binder_count,
        }
    {
        return Err(invalid(
            span,
            "context input binder contradicts the actual header",
        ));
    }
    Ok(binder)
}
fn input_key(
    row: &CoreClosureLifeContext,
    parameter: u32,
    class: Option<u32>,
    pointer: bool,
    left: &mut usize,
    span: Span,
) -> R<Fixed> {
    pay(left, row.inputs.len().saturating_mul(2), span)?;
    row.inputs
        .iter()
        .find(|r| {
            r.parameter == parameter
                && match (&r.position, class) {
                    (CoreClosureHeaderPosition::Borrow(_), None) => true,
                    (CoreClosureHeaderPosition::Class(p), Some(c)) => {
                        p.class == c
                            && if pointer {
                                p.path.as_slice() == [LifeTypeProjection::Pointee]
                            } else {
                                p.path.is_empty()
                            }
                    }
                    _ => false,
                }
        })
        .map(|r| Fixed::Header(r.header_binder))
        .ok_or_else(|| unsupported(span, "actual capture has no matching creator input context"))
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum InputOrigin {
    Local(LocalId, BlockId, usize),
    Value(ValueId),
}
/// Authenticate a local alias at this real mint boundary. Only actual loads,
/// stores and permission-preserving reborrows transport an input identity.
/// Unknown writes/calls and an ungrounded loop are not alias evidence.
fn actual_input_origin(
    creator: FunctionRef<'_>, local: LocalId, at: (BlockId, usize),
    limits: &Limits, left: &mut usize, span: Span,
) -> R<Option<u32>> {
    let mut pending = vec![InputOrigin::Local(local, at.0, at.1)];
    let mut seen = BTreeSet::new();
    let mut sources = BTreeSet::new();
    while let Some(query) = pending.pop() {
        pay(left, seen.len().saturating_add(1).ilog2() as usize + 2, span)?;
        if !seen.insert(query) { continue; }
        if seen.len() > limits.max_nodes { return Err(limit("capture input-alias node limit exceeded", span)); }
        let mut next = Vec::new();
        match query {
            InputOrigin::Value(value) => {
                let mut selected = None;
                pay(left, creator.blocks.len(), span)?;
                for block in creator.blocks {
                    pay(left, block.instructions.len(), span)?;
                    for (index, instruction) in block.instructions.iter().enumerate() {
                        if instruction.result == Some(value)
                            && selected.replace((block.id, index, &instruction.kind)).is_some()
                        { return Err(invalid(span, "capture alias SSA has multiple actual producers")); }
                    }
                }
                pay(left, 1, span)?;
                match selected {
                    Some((block, index, InstructionKind::Load(local))) =>
                        next.push(InputOrigin::Local(*local, block, index)),
                    Some((_, _, InstructionKind::ReborrowAddress { address, .. })) =>
                        next.push(InputOrigin::Value(*address)),
                    _ => return Ok(None),
                }
            }
            InputOrigin::Local(local, block, before) => {
                pay(left, creator.parameters.len().saturating_add(creator.blocks.len()), span)?;
                let parameter = creator.parameters.iter().position(|input| *input == local);
                let body = creator.blocks.iter().find(|body| body.id == block)
                    .ok_or_else(|| invalid(span, "capture alias has no actual block"))?;
                if before > body.instructions.len() { return Err(invalid(span, "capture alias leaves its actual mint boundary")); }
                let mut assigned = None;
                for instruction in body.instructions[..before].iter().rev() {
                    pay(left, 1, span)?;
                    match &instruction.kind {
                        InstructionKind::Store { local: target, value } if *target == local => {
                            assigned = Some(*value); break;
                        }
                        InstructionKind::StorageLive(target) | InstructionKind::StorageDead(target)
                        | InstructionKind::TakeLocal { local: target }
                        | InstructionKind::BorrowLocalMut { local: target }
                            if *target == local => return Ok(None),
                        InstructionKind::Constant(_) | InstructionKind::Load(_)
                        | InstructionKind::Store { .. } | InstructionKind::StorageLive(_)
                        | InstructionKind::StorageDead(_) | InstructionKind::ScopeEnter { .. }
                        | InstructionKind::ScopeExit { .. } | InstructionKind::BorrowLocal { .. }
                        | InstructionKind::ReborrowAddress { .. }
                        | InstructionKind::Unary { .. } | InstructionKind::Binary { .. } => {}
                        // Calls/indirect writes and every unsupported operation
                        // are not evidence that an input identity survived.
                        _ => return Ok(None),
                    }
                }
                if let Some(value) = assigned {
                    pay(left, 1, span)?;
                    next.push(InputOrigin::Value(value));
                } else if block == creator.entry {
                    let Some(parameter) = parameter else { return Ok(None); };
                    let parameter = u32::try_from(parameter)
                        .map_err(|_| limit("capture input ordinal exceeded", span))?;
                    pay(left, sources.len().saturating_add(1).ilog2() as usize + 2, span)?;
                    sources.insert(parameter);
                } else {
                    pay(left, creator.blocks.len(), span)?;
                    for predecessor in creator.blocks {
                        let incoming = match predecessor.terminator {
                            Terminator::Jump(target) => target == block,
                            Terminator::Branch { then_block, else_block, .. } => then_block == block || else_block == block,
                            _ => false,
                        };
                        if incoming {
                            pay(left, 1, span)?;
                            next.push(InputOrigin::Local(local, predecessor.id, predecessor.instructions.len()));
                        }
                    }
                    if next.is_empty() { return Ok(None); }
                }
            }
        }
        if pending.len().saturating_add(next.len()) > limits.max_nodes {
            return Err(limit("capture input-alias frontier limit exceeded", span));
        }
        pay(left, next.len(), span)?;
        pending.extend(next);
    }
    // Revisited loop states contribute no new source; a cyclic graph with no
    // genuine entry source cannot turn the empty fixed point into evidence.
    Ok((sources.len() == 1).then(|| *sources.iter().next().expect("one actual input")))
}

fn prepare(
    row: CoreClosureLifeContext,
    creator: FunctionRef<'_>,
    captures: &[Capture], at: (BlockId, usize), limits: &Limits,
    left: &mut usize,
    span: Span,
) -> R<Formation> {
    pay(
        left,
        row.inputs
            .len()
            .saturating_add(row.named_fixed.len())
            .saturating_add(row.parameters.len()),
        span,
    )?;
    let mut named = BTreeMap::new();
    for (i, anchor) in row.inputs.iter().enumerate() {
        let binder = actual_header(anchor, creator, left, span)?;
        pay(left, i.saturating_mul(2).saturating_add(1), span)?;
        if row.inputs[..i]
            .iter()
            .any(|p| match (&p.position, &anchor.position) {
                (CoreClosureHeaderPosition::Borrow(a), CoreClosureHeaderPosition::Borrow(b)) => {
                    a == b
                }
                (CoreClosureHeaderPosition::Class(a), CoreClosureHeaderPosition::Class(b)) => {
                    a == b
                }
                _ => false,
            })
        {
            return Err(invalid(span, "duplicate source input position"));
        }
        if let Some(source) = anchor.source_parameter {
            if row.parameters.get(source as usize) != Some(&LifeParameterKind::Lifetime) {
                return Err(invalid(
                    span,
                    "source input does not name a full lifetime ordinal",
                ));
            }
            pay(left, named.len().saturating_add(1), span)?;
            if named
                .insert(source, binder)
                .is_some_and(|prior| prior != binder)
            {
                return Err(invalid(
                    span,
                    "one original lifetime contradicts its actual header selections",
                ));
            }
        }
    }
    if row.named_fixed.len() != row.signature.fixed_binder_count as usize {
        return Err(invalid(
            span,
            "source signature omits its complete original Fixed map",
        ));
    }
    let mut fixed = Vec::new();
    for (i, source) in row.named_fixed.iter().enumerate() {
        pay(left, named.len().saturating_add(1), span)?;
        if source.fixed as usize != i
            || row.parameters.get(source.parameter as usize) != Some(&LifeParameterKind::Lifetime)
        {
            return Err(invalid(
                span,
                "source Fixed map is not canonical at its full generic ordinal",
            ));
        }
        fixed.push(Fixed::Header(*named.get(&source.parameter).ok_or_else(
            || unsupported(span, "free source Fixed has no actual creator input anchor"),
        )?));
    }
    let mut keys = Vec::new();
    pay(left, captures.len(), span)?;
    for capture in captures {
        pay(left, creator.parameters.len().saturating_add(1), span)?;
        let input = actual_input_origin(creator, capture.source, at, limits, left, span)?;
        let source = creator
            .locals
            .get(capture.source.0 as usize)
            .ok_or_else(|| invalid(span, "capture source is not an actual creator local"))?;
        // Full type snapshots are prepaid by the caller before this comparison.
        if source.ty != capture.ty {
            return Err(invalid(
                span,
                "capture source type differs from the actual creator local",
            ));
        }
        let stored = capture.stored_type();
        let outer = if matches!(stored, Type::Address { .. }) {
            if capture.borrowed {
                Some(Fixed::Local(capture.source))
            } else {
                Some(input_key(
                    &row,
                    input.ok_or_else(|| {
                        unsupported(
                            span,
                            "source capture alias requires its actual input-origin consumer",
                        )
                    })?,
                    None,
                    false,
                    left,
                    span,
                )?)
            }
        } else {
            None
        };
        let mut classes = Vec::new();
        if let Some((shape, pointer)) = qualified(&capture.ty) {
            let parameter = input.ok_or_else(|| {
                unsupported(
                    span,
                    "qualified local capture requires its actual input-class origin consumer",
                )
            })?;
            pay(left, shape.binder_count as usize, span)?;
            for class in 0..shape.binder_count {
                classes.push(input_key(
                    &row,
                    parameter,
                    Some(class),
                    pointer,
                    left,
                    span,
                )?);
            }
        }
        keys.push(CaptureKeys { outer, classes });
    }
    Ok(Formation {
        proposal: row.signature,
        fixed,
        captures: keys,
    })
}

impl State {
    pub(crate) fn install_source_contexts(
        &mut self,
        core: &CoreModule,
        signatures: &BTreeMap<String, Signature>,
    ) -> R<()> {
        if core.closure_life_contexts.is_empty() {
            return Ok(());
        }
        reserve(
            &core.closure_life_contexts,
            &self.limits,
            &mut self.work_left,
            1,
        )?;
        for row in &core.closure_life_contexts {
            self.snapshot(core.functions.len(), Span::default())?;
            let mut found = core.functions.iter().filter(|f| f.id == row.creator);
            let f = found.next().ok_or_else(|| {
                invalid(Span::default(), "context creator is not an actual function")
            })?;
            if found.next().is_some() {
                return Err(invalid(f.span, "context creator identity is duplicated"));
            }
            if f.foreign.is_some()
                || f.receiver.is_some()
                || !f.callable_parameters.is_empty()
                || !f.type_parameters.is_empty()
                || !f.state_parameters.is_empty()
                || f.call_kind.is_some_and(|k| k != CallKind::Direct)
            {
                return Err(unsupported(
                    f.span,
                    "source closure creator requires an original ordinary free-function Definition",
                ));
            }
            if let Some(returned) = &f.callable_return {
                let scope = f.source_scope.as_ref().ok_or_else(|| invalid(f.span,
                    "callable-return creator has no complete original input scope"))?;
                let returned_scope = returned.source_life.as_ref().ok_or_else(|| invalid(f.span,
                    "callable-return creator has no original return scope"))?;
                self.snapshot(scope.parameters.len().saturating_add(returned_scope.parameters.len())
                    .saturating_add(scope.inputs.len()).saturating_add(row.inputs.len()), f.span)?;
                for identity in [&scope.identity, &returned_scope.identity] {
                    self.snapshot(identity.package.len().saturating_add(identity.generation_path.len()), f.span)?;
                    for name in identity.module_path.iter().chain(&identity.declaration_path) {
                        self.snapshot(name.len(), f.span)?;
                    }
                }
                if scope.parameters.iter().chain(&returned_scope.parameters)
                    .any(|kind| *kind != LifeParameterKind::Lifetime)
                    || scope.declaration != row.declaration || returned_scope.declaration != row.declaration
                    || scope.identity != row.identity || returned_scope.identity != row.identity
                    || scope.parameters != row.parameters || returned_scope.parameters != row.parameters
                { return Err(invalid(f.span, "closure and callable return change their exact original Definition scope")); }
                for anchor in &row.inputs {
                    let anchor_path = match &anchor.position {
                        CoreClosureHeaderPosition::Borrow(position) => position.path.len(),
                        CoreClosureHeaderPosition::Class(position) => position.path.len(),
                    };
                    self.snapshot(scope.inputs.len().saturating_mul(anchor_path.saturating_add(1)), f.span)?;
                    for candidate in &scope.inputs {
                        let path = match &candidate.position {
                            CoreClosureHeaderPosition::Borrow(position) => position.path.len(),
                            CoreClosureHeaderPosition::Class(position) => position.path.len(),
                        };
                        self.snapshot(path.saturating_add(1), f.span)?;
                    }
                    if !scope.inputs.iter().any(|candidate| candidate == anchor) {
                        return Err(invalid(f.span, "closure input is outside its exact callable-return creator scope"));
                    }
                }
            }
            self.snapshot(f.name.len().saturating_add(signatures.len()), f.span)?;
            let id = signatures
                .get(&f.name)
                .ok_or_else(|| invalid(f.span, "source creator signature is absent"))?
                .concrete_id(f.span)?;
            self.snapshot(self.source_contexts.rows.len().saturating_add(1), f.span)?;
            if self
                .source_contexts
                .rows
                .insert((id, row.expression), row.clone())
                .is_some()
            {
                return Err(invalid(
                    f.span,
                    "duplicate actual creator/closure source context",
                ));
            }
        }
        Ok(())
    }
    pub(crate) fn bind_source_context(
        &mut self,
        id: ClosureId,
        creator: FunctionRef<'_>,
        expression: NodeId,
        at: Option<(BlockId, usize)>,
        span: Span,
    ) -> R<()> {
        if self.source_contexts.rows.is_empty() {
            return Ok(());
        }
        self.snapshot(self.source_contexts.rows.len().saturating_add(1), span)?;
        let Some(row) = self.source_contexts.rows.remove(&(creator.id, expression)) else {
            return Ok(());
        };
        for capture in &self.pending[id.0 as usize].captures {
            inference::charge_type_snapshot(
                &capture.ty,
                span,
                &self.limits,
                &mut self.work_left,
                3,
            )?;
        }
        let formation = prepare(
            row,
            creator,
            &self.pending[id.0 as usize].captures,
            at.ok_or_else(|| invalid(span, "source capture has no real mint position"))?,
            &self.limits,
            &mut self.work_left,
            span,
        )?;
        self.pending[id.0 as usize].source_context = Some(formation);
        Ok(())
    }
    pub(crate) fn form_source_contract(
        &mut self,
        id: ClosureId,
        parameters: &[Type],
        result: &Type,
        span: Span,
    ) -> R<Option<StorageLifeContract>> {
        let pending = &self.pending[id.0 as usize];
        let Some(formation) = &pending.source_context else {
            return Ok(None);
        };
        for ty in parameters.iter().chain(std::iter::once(result)) {
            inference::charge_type_snapshot(ty, span, &self.limits, &mut self.work_left, 2)?;
        }
        let mut captures = Vec::new();
        for capture in &pending.captures {
            inference::charge_type_snapshot(
                &capture.ty,
                span,
                &self.limits,
                &mut self.work_left,
                3,
            )?;
            captures.push(capture.stored_type());
        }
        formation
            .form(parameters, &captures, result, &mut self.work_left, span)
            .map(Some)
    }
    pub(super) fn finish_source_contexts(&mut self) -> R<()> {
        if self.source_contexts.rows.is_empty() {
            return Ok(());
        }
        self.snapshot(self.source_contexts.rows.len(), Span::default())?;
        Err(invalid(
            Span::default(),
            "source closure context has no actual consumed expression",
        ))
    }
}

#[derive(Clone)]
enum Position {
    Borrow(StoragePosition),
    Class(SignatureLifeClass),
}
fn positions(
    ty: &Type,
    base: StorageBase,
    capture: bool,
    left: &mut usize,
    span: Span,
) -> R<Vec<Position>> {
    pay(left, 1, span)?;
    let mut positions = Vec::new();
    let (inner, pointer) = if let Type::Address { inner, .. } = ty {
        positions.push(Position::Borrow(StoragePosition { base, path: vec![] }));
        (inner.as_ref(), true)
    } else {
        (ty, false)
    };
    if let Type::BorrowedStorage { storage, life } = inner {
        if !(matches!(storage.as_ref(), Type::Record { .. } | Type::Tuple(_))
            || capture && matches!(storage.as_ref(), Type::Package { .. }))
            || !life.retained.is_empty()
        {
            return Err(unsupported(
                span,
                "source closure qualified boundary requires its complete nested consumer",
            ));
        }
        pay(left, (life.binder_count as usize).saturating_mul(2), span)?;
        for class in 0..life.binder_count {
            positions.push(Position::Class(SignatureLifeClass {
                base,
                path: if pointer {
                    vec![LifeTypeProjection::Pointee]
                } else {
                    vec![]
                },
                class,
            }));
        }
    } else if pointer {
        if !inner.is_address_pointee() {
            return Err(unsupported(
                span,
                "source closure address pointee is outside its finite profile",
            ));
        }
    } else {
        if capture
            && let Type::Managed(payload) = inner
            && let Type::Record { fields, .. } = payload.as_ref()
        {
            pay(left, fields.len(), span)?;
        }
        if !stored_call_value(inner) && !(capture && owned_scalar_resource(inner)) {
            return Err(unsupported(
                span,
                "source closure capture or argument needs its owning/nested consumer",
            ));
        }
    }
    Ok(positions)
}
fn stored_call_value(ty: &Type) -> bool {
    matches!(
        ty,
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
            | Type::F64
    )
}
pub(super) fn owned_scalar_resource(ty: &Type) -> bool {
    matches!(ty, Type::Managed(payload) if matches!(payload.as_ref(), Type::Record { fields, .. } if fields.iter().all(|(_, t)| stored_call_value(t))))
}
struct Writer {
    fixed: BTreeMap<Fixed, u32>,
    invocation: BTreeMap<u32, u32>,
    grounded: BTreeSet<Fixed>,
    bindings: Vec<StorageLifeBinding>,
    classes: Vec<SignatureLifeClassBinding>,
}
impl Writer {
    fn push(
        &mut self,
        position: Position,
        key: Key,
        capture: bool,
        left: &mut usize,
        span: Span,
    ) -> R<()> {
        let binder = match key {
            Key::Fixed(key) => {
                pay(left, self.fixed.len().saturating_add(1), span)?;
                let next = u32::try_from(self.fixed.len())
                    .map_err(|_| limit("source Fixed domain exceeded", span))?;
                let index = *self.fixed.entry(key).or_insert(next);
                if capture {
                    pay(left, self.grounded.len().saturating_add(1), span)?;
                    self.grounded.insert(key);
                }
                StorageBinder::Fixed(index)
            }
            Key::Invocation(key) => {
                if capture {
                    return Err(invalid(
                        span,
                        "actual capture cannot become a fresh Invocation",
                    ));
                }
                pay(left, self.invocation.len().saturating_add(1), span)?;
                let next = u32::try_from(self.invocation.len())
                    .map_err(|_| limit("source Invocation domain exceeded", span))?;
                StorageBinder::Invocation(*self.invocation.entry(key).or_insert(next))
            }
        };
        pay(left, 1, span)?;
        match position {
            Position::Borrow(position) => {
                self.bindings.push(StorageLifeBinding { position, binder })
            }
            Position::Class(position) => self
                .classes
                .push(SignatureLifeClassBinding { position, binder }),
        }
        Ok(())
    }
}
struct Proposal<'a> {
    formation: &'a Formation,
    borrow: usize,
    class: usize,
    fixed: u32,
    invocation: u32,
}
impl Proposal<'_> {
    fn key(&mut self, position: &Position, left: &mut usize, span: Span) -> R<Key> {
        pay(left, 2, span)?;
        let binder = match position {
            Position::Borrow(position) => {
                let row = self
                    .formation
                    .proposal
                    .bindings
                    .get(self.borrow)
                    .ok_or_else(|| invalid(span, "source proposal omits actual outer Address"))?;
                if row.position != *position {
                    return Err(invalid(span, "source proposal has a wrong borrow position"));
                }
                self.borrow += 1;
                row.binder
            }
            Position::Class(position) => {
                let row = self
                    .formation
                    .proposal
                    .classes
                    .get(self.class)
                    .ok_or_else(|| invalid(span, "source proposal omits actual qualified class"))?;
                if row.position != *position {
                    return Err(invalid(
                        span,
                        "source proposal has a wrong qualified boundary",
                    ));
                }
                self.class += 1;
                row.binder
            }
        };
        match binder {
            StorageBinder::Fixed(i) => {
                if i > self.fixed {
                    return Err(invalid(
                        span,
                        "source Fixed first occurrence is not canonical",
                    ));
                }
                if i == self.fixed {
                    self.fixed += 1;
                }
                Ok(Key::Fixed(
                    *self.formation.fixed.get(i as usize).ok_or_else(|| {
                        invalid(span, "source Fixed has no original lifetime mapping")
                    })?,
                ))
            }
            StorageBinder::Invocation(i) => {
                if i > self.invocation || i >= self.formation.proposal.invocation_binder_count {
                    return Err(invalid(
                        span,
                        "source Invocation first occurrence is not canonical",
                    ));
                }
                if i == self.invocation {
                    self.invocation += 1;
                }
                Ok(Key::Invocation(i))
            }
        }
    }
}
impl Formation {
    fn form(
        &self,
        parameters: &[Type],
        captures: &[Type],
        result: &Type,
        left: &mut usize,
        span: Span,
    ) -> R<StorageLifeContract> {
        let environment_result = lending::transferred_result(result);
        let declared_result = self
            .proposal
            .bindings
            .iter()
            .any(|row| row.position.base == StorageBase::Result)
            || self
                .proposal
                .classes
                .iter()
                .any(|row| row.position.base == StorageBase::Result);
        if captures.len() != self.captures.len()
            || !(stored_call_value(result) || environment_result)
        {
            return Err(unsupported(
                span,
                "source context formation requires its actual captures and scalar result",
            ));
        }
        let mut writer = Writer {
            fixed: BTreeMap::new(),
            invocation: BTreeMap::new(),
            grounded: BTreeSet::new(),
            bindings: vec![],
            classes: vec![],
        };
        let mut proposal = Proposal {
            formation: self,
            borrow: 0,
            class: 0,
            fixed: 0,
            invocation: 0,
        };
        pay(
            left,
            parameters
                .len()
                .saturating_add(captures.len())
                .saturating_add(1),
            span,
        )?;
        for (i, parameter) in parameters.iter().enumerate() {
            let base = StorageBase::Parameter(
                u32::try_from(i).map_err(|_| limit("source parameter ordinal exceeded", span))?,
            );
            for position in positions(parameter, base, false, left, span)? {
                let key = proposal.key(&position, left, span)?;
                writer.push(position, key, false, left, span)?;
            }
        }
        for (i, (ty, keys)) in captures.iter().zip(&self.captures).enumerate() {
            let base = StorageBase::Capture(
                u32::try_from(i).map_err(|_| limit("actual capture ordinal exceeded", span))?,
            );
            for position in positions(ty, base, true, left, span)? {
                let key = match &position {
                    Position::Borrow(_) => keys.outer,
                    Position::Class(p) => keys.classes.get(p.class as usize).copied(),
                }
                .ok_or_else(|| invalid(span, "actual capture position lacks its creator source"))?;
                writer.push(position, Key::Fixed(key), true, left, span)?;
            }
        }
        for position in positions(
            if environment_result && !declared_result {
                &Type::Unit
            } else {
                result
            },
            StorageBase::Result,
            false,
            left,
            span,
        )? {
            let key = proposal.key(&position, left, span)?;
            writer.push(position, key, false, left, span)?;
        }
        if proposal.borrow != self.proposal.bindings.len()
            || proposal.class != self.proposal.classes.len()
            || proposal.fixed != self.proposal.fixed_binder_count
            || proposal.invocation != self.proposal.invocation_binder_count
        {
            return Err(invalid(
                span,
                "source proposal has missing, extra or unused domains",
            ));
        }
        pay(
            left,
            writer
                .fixed
                .len()
                .saturating_mul(writer.grounded.len().saturating_add(1)),
            span,
        )?;
        if writer
            .fixed
            .keys()
            .any(|key| !writer.grounded.contains(key))
        {
            return Err(unsupported(
                span,
                "free source Fixed is not established by an actual retained capture",
            ));
        }
        Ok(StorageLifeContract {
            static_bindings: Vec::new(),
            fixed_binder_count: u32::try_from(writer.fixed.len())
                .map_err(|_| limit("formed Fixed domain exceeded", span))?,
            invocation_binder_count: u32::try_from(writer.invocation.len())
                .map_err(|_| limit("formed Invocation domain exceeded", span))?,
            bindings: writer.bindings,
            classes: writer.classes,
        })
    }
}

#[cfg(test)]
mod input_alias_tests {
    use super::*;

    fn block(id: u32, input: u32, result: u32, next: u32) -> BasicBlock {
        BasicBlock {
            id: BlockId(id),
            instructions: vec![
                Instruction { result: Some(ValueId(result)), kind: InstructionKind::Load(LocalId(input)), span: Span::default() },
                Instruction { result: None, kind: InstructionKind::Store { local: LocalId(2), value: ValueId(result) }, span: Span::default() },
            ],
            terminator: Terminator::Jump(BlockId(next)),
        }
    }
    fn function(blocks: Vec<BasicBlock>) -> TypedFunction {
        let ty = Type::Address { mutable: false, inner: Box::new(Type::I32) };
        TypedFunction {
            dyn_instance: None, retained_return_sources: None, storage_life: None,
            call_kind: CallKind::Direct, resume_bound: zil_ir::TaskResumeBound::Local,
            reference_return_bound: None, callable_return_contract: None,
            callable_contract: None, protocol_contracts: Vec::new(),
            id: FunctionId(0), name: "input-alias".into(), foreign: None, environment: None,
            parameters: vec![LocalId(0), LocalId(1)], return_type: Type::Unit,
            result_mode: PassingMode::Infer,
            locals: (0..3).map(|id| LocalDecl { id: LocalId(id), name: format!("p{id}"),
                ty: ty.clone(), mutable: true, span: Span::default() }).collect(),
            values: vec![Type::Bool, ty.clone(), ty], blocks, entry: BlockId(0), span: Span::default(),
        }
    }
    #[test]
    fn capture_alias_requires_one_exact_parameter_on_every_reaching_edge() {
        for (left, right, expected) in [(0, 0, Some(0)), (1, 1, Some(1)), (0, 1, None)] {
            let f = function(vec![
                BasicBlock { id: BlockId(0), instructions: vec![], terminator: Terminator::Branch {
                    condition: ValueId(0), then_block: BlockId(1), else_block: BlockId(2) } },
                block(1, left, 1, 3), block(2, right, 2, 3),
                BasicBlock { id: BlockId(3), instructions: vec![], terminator: Terminator::Return(None) },
            ]);
            let limits = Limits::default();
            let mut left = limits.max_analysis_steps;
            assert_eq!(actual_input_origin(crate::FunctionRef::from(&f), LocalId(2),
                (BlockId(3), 0), &limits, &mut left, Span::default()).unwrap(), expected);
        }
    }
    #[test]
    fn missing_or_replaced_input_is_not_recovered_from_equal_address_types() {
        let mut f = function(vec![block(0, 0, 1, 1), BasicBlock {
            id: BlockId(1), instructions: vec![], terminator: Terminator::Return(None) }]);
        let limits = Limits::default();
        f.blocks[0].instructions.clear();
        let mut left = limits.max_analysis_steps;
        assert_eq!(actual_input_origin(crate::FunctionRef::from(&f), LocalId(2),
            (BlockId(1), 0), &limits, &mut left, Span::default()).unwrap(), None);
        f.blocks[0] = block(0, 1, 1, 1);
        let mut left = limits.max_analysis_steps;
        assert_eq!(actual_input_origin(crate::FunctionRef::from(&f), LocalId(2),
            (BlockId(1), 0), &limits, &mut left, Span::default()).unwrap(), Some(1));
    }
}
