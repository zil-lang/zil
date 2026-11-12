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
    creator: &TypedFunction,
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
fn prepare(
    row: CoreClosureLifeContext,
    creator: &TypedFunction,
    captures: &[Capture],
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
        let input = creator
            .parameters
            .iter()
            .position(|l| *l == capture.source)
            .map(|i| i as u32);
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
                || f.callable_return.is_some()
                || f.call_kind.is_some_and(|k| k != CallKind::Direct)
            {
                return Err(unsupported(
                    f.span,
                    "source closure creator requires an original ordinary free-function Definition",
                ));
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
        creator: &TypedFunction,
        expression: NodeId,
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
