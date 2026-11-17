//! Conservative invalidation of literal-source equations. These destinations
//! grant no access; an unresolved writable alias makes the source Unknown.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Root {
    Local(LocalId),
    Capture(u32),
    Value(ValueId),
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Place {
    root: Root,
    path: Path,
}
enum Mutation {
    Place(Place),
    Unknown,
}
#[derive(Default)]
pub(super) struct Writes(Vec<Mutation>);
impl Writes {
    fn place(&mut self, place: Place, b: &mut Budget) -> CompileResult<()> {
        pay(b, place.path.len().saturating_add(2))?;
        self.0.push(Mutation::Place(place));
        Ok(())
    }
    fn unknown(&mut self, b: &mut Budget) -> CompileResult<()> {
        pay(b, 2)?;
        self.0.push(Mutation::Unknown);
        Ok(())
    }
    pub(super) fn check(
        &self,
        root: Root,
        path: &[ContractProjection],
        b: &mut Budget,
    ) -> CompileResult<()> {
        for mutation in &self.0 {
            pay(b, path.len().saturating_add(1))?;
            match mutation {
                Mutation::Unknown => {
                    return Err(err(
                        "literal source crosses an unresolved writable alias or effect",
                    ));
                }
                Mutation::Place(place) => {
                    pay(b, place.path.len())?;
                    if place.root == root
                        && (path.starts_with(&place.path) || place.path.starts_with(path))
                    {
                        return Err(err(
                            "literal source crosses an intersecting field or indirect replacement",
                        ));
                    }
                }
            }
        }
        Ok(())
    }
    pub(super) fn check_value(
        &self,
        value: ValueId,
        path: &[ContractProjection],
        b: &mut Budget,
    ) -> CompileResult<()> {
        // An unknown memory write cannot change an immutable SSA snapshot.
        // A mutably invoked SSA environment is different: its exact receiver
        // state must not be reread from the original constructor afterward.
        for mutation in &self.0 {
            pay(b, path.len().saturating_add(1))?;
            if let Mutation::Place(place) = mutation {
                pay(b, place.path.len())?;
                if place.root == Root::Value(value)
                    && (path.starts_with(&place.path) || place.path.starts_with(path))
                {
                    return Err(err(
                        "literal source crosses a mutably invoked actual environment",
                    ));
                }
            }
        }
        Ok(())
    }
    fn local(
        &mut self,
        c: &Context<'_>,
        local: LocalId,
        path: Path,
        b: &mut Budget,
    ) -> CompileResult<()> {
        for place in local_places(c, local, &path, b)? {
            self.place(place, b)?;
        }
        Ok(())
    }
    fn address(
        &mut self,
        c: &Context<'_>,
        value: ValueId,
        prefix: Path,
        b: &mut Budget,
    ) -> CompileResult<()> {
        if let Some(places) = pointer_places(c, value, &[], &mut BTreeSet::new(), 0, b)? {
            for mut place in places {
                pay(b, prefix.len())?;
                place.path.extend_from_slice(&prefix);
                self.place(place, b)?;
            }
        } else {
            self.unknown(b)?;
        }
        Ok(())
    }
}

// A change to a stored byte pointer, or a structural child containing one,
// can invalidate literal provenance. Mutating an unrelated scalar or the
// bytes of an actual buffer cannot replace that pointer slot.
fn byte_slot(ty: &Type, b: &mut Budget, depth: usize) -> CompileResult<bool> {
    pay(b, 1)?;
    if depth > b.partial_depth {
        return Err(err(
            "Static mutation Type traversal exceeds its finite depth",
        ));
    }
    match ty {
        Type::Address { inner, .. }
            if **inner == Type::U8
                || matches!(inner.as_ref(), Type::Slice { element } if **element == Type::U8) =>
        {
            Ok(true)
        }
        Type::Address { inner, .. }
        | Type::Desc(inner)
        | Type::Managed(inner)
        | Type::BorrowedStorage { storage: inner, .. }
        | Type::Array { element: inner, .. }
        | Type::Slice { element: inner }
        | Type::Opaque { hidden: inner, .. } => byte_slot(inner, b, depth + 1),
        Type::Tuple(fields) => {
            for field in fields {
                if byte_slot(field, b, depth + 1)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            for (_, field) in fields {
                if byte_slot(field, b, depth + 1)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Type::Enum { variants, .. } => {
            for (_, fields) in variants {
                for field in fields {
                    if byte_slot(field, b, depth + 1)? {
                        return Ok(true);
                    }
                }
            }
            Ok(false)
        }
        Type::ErrorUnion { error, result } => {
            Ok(byte_slot(error, b, depth + 1)? || byte_slot(result, b, depth + 1)?)
        }
        // Replacing an opaque environment may replace an unmodeled stored
        // pointer. This is an effect upper bound, never a Static assertion.
        Type::Closure(_) | Type::Dyn { .. } | Type::OwnedDyn { .. } => Ok(true),
        _ => Ok(false),
    }
}
fn type_of<'a>(c: &'a Context<'_>, value: ValueId) -> CompileResult<&'a Type> {
    c.f.values
        .get(value.0 as usize)
        .ok_or_else(|| err("Static mutation operand is absent"))
}
fn field_path(path: &[PlaceProjection], b: &mut Budget) -> CompileResult<Path> {
    pay(b, path.len())?;
    Ok(path
        .iter()
        .copied()
        .map(ContractProjection::Place)
        .collect())
}
// A capture-bound Package local is a physical view, not a separate root.
// Union all bindings conservatively across activations; this never chooses an
// owner or grants access, and cannot miss a write to the actual capture.
fn local_places(
    c: &Context<'_>,
    local: LocalId,
    path: &[ContractProjection],
    b: &mut Budget,
) -> CompileResult<Vec<Place>> {
    pay(b, path.len().saturating_add(2))?;
    let mut out = vec![Place {
        root: Root::Local(local),
        path: path.to_vec(),
    }];
    for block in &c.f.blocks {
        for instruction in &block.instructions {
            pay(b, 1)?;
            if let InstructionKind::BindPackageCapture {
                local: actual,
                index,
            } = instruction.kind
                && actual == local
            {
                pay(b, path.len().saturating_add(2))?;
                out.push(Place {
                    root: Root::Capture(index),
                    path: path.to_vec(),
                });
            }
        }
    }
    Ok(out)
}
fn local_pointer(
    c: &Context<'_>,
    local: LocalId,
    path: &[ContractProjection],
    active: &mut BTreeSet<(ValueId, Path)>,
    depth: usize,
    b: &mut Budget,
) -> CompileResult<Option<Vec<Place>>> {
    let Some(stores) = c.stores.get(&local) else {
        return Ok(None);
    };
    pay(b, c.stores.len().saturating_add(stores.len()))?;
    let mut out = Vec::new();
    for value in stores {
        let Some(mut places) = pointer_places(c, *value, path, active, depth + 1, b)? else {
            return Ok(None);
        };
        pay(b, places.len())?;
        out.append(&mut places);
    }
    Ok((!out.is_empty()).then_some(out))
}
fn pointer_places(
    c: &Context<'_>,
    value: ValueId,
    path: &[ContractProjection],
    active: &mut BTreeSet<(ValueId, Path)>,
    depth: usize,
    b: &mut Budget,
) -> CompileResult<Option<Vec<Place>>> {
    pay(
        b,
        path.len()
            .saturating_mul(active.len().saturating_add(2))
            .saturating_add(2),
    )?;
    if depth > b.partial_depth {
        return Err(err(
            "Static writable alias traversal exceeds its finite depth",
        ));
    }
    let key = (value, path.to_vec());
    if !active.insert(key.clone()) {
        return Ok(None);
    }
    let result = (|| {
        let Some(kind) = c.defs.get(value.0 as usize).copied().flatten() else {
            return Ok(None);
        };
        let direct = match kind {
            InstructionKind::BorrowLocal { local } | InstructionKind::BorrowLocalMut { local }
                if path.is_empty() =>
            {
                Some(Place {
                    root: Root::Local(*local),
                    path: vec![],
                })
            }
            InstructionKind::BorrowCapture { index, .. } if path.is_empty() => Some(Place {
                root: Root::Capture(*index),
                path: vec![],
            }),
            InstructionKind::BorrowPlaceLocal {
                local,
                path: selected,
                ..
            } if path.is_empty() => Some(Place {
                root: Root::Local(*local),
                path: field_path(selected, b)?,
            }),
            InstructionKind::BorrowFieldLocal {
                local,
                path: selected,
                ..
            } if path.is_empty() => {
                pay(b, selected.len())?;
                Some(Place {
                    root: Root::Local(*local),
                    path: selected
                        .iter()
                        .map(|field| ContractProjection::Place(PlaceProjection::Field(*field)))
                        .collect(),
                })
            }
            InstructionKind::BorrowPackageField { local, index, .. } if path.is_empty() => {
                Some(Place {
                    root: Root::Local(*local),
                    path: vec![ContractProjection::Place(PlaceProjection::Field(*index))],
                })
            }
            _ => None,
        };
        if let Some(place) = direct {
            if let Root::Local(local) = place.root {
                return local_places(c, local, &place.path, b).map(Some);
            }
            pay(b, place.path.len().saturating_add(2))?;
            return Ok(Some(vec![place]));
        }
        match kind {
            InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                local_pointer(c, *local, path, active, depth, b)
            }
            InstructionKind::ReborrowAddress { address, .. }
            | InstructionKind::DescribeAddress { address } => {
                pointer_places(c, *address, path, active, depth + 1, b)
            }
            InstructionKind::AddressField { address, index } if path.is_empty() => {
                let Some(mut places) = pointer_places(c, *address, &[], active, depth + 1, b)?
                else {
                    return Ok(None);
                };
                for place in &mut places {
                    pay(b, 1)?;
                    place
                        .path
                        .push(ContractProjection::Place(PlaceProjection::Field(*index)));
                }
                Ok(Some(places))
            }
            InstructionKind::BorrowPlaceAddress {
                address,
                path: selected,
                ..
            } if path.is_empty() => {
                let Some(mut places) = pointer_places(c, *address, &[], active, depth + 1, b)?
                else {
                    return Ok(None);
                };
                for place in &mut places {
                    pay(b, selected.len())?;
                    place.path.extend(field_path(selected, b)?);
                }
                Ok(Some(places))
            }
            InstructionKind::MakeAggregate { fields }
            | InstructionKind::MakePackage { fields }
            | InstructionKind::MakeManaged { fields }
            | InstructionKind::MakeOwnedTuple { fields } => {
                let Some((ContractProjection::Place(PlaceProjection::Field(index)), tail)) =
                    path.split_first()
                else {
                    return Ok(None);
                };
                let Some(value) = fields.get(*index as usize) else {
                    return Ok(None);
                };
                pointer_places(c, *value, tail, active, depth + 1, b)
            }
            InstructionKind::MakeErrorUnion { is_error, value } => {
                let Some((ContractProjection::Place(PlaceProjection::ErrorArm(selected)), tail)) =
                    path.split_first()
                else {
                    return Ok(None);
                };
                if selected != is_error {
                    return Ok(Some(Vec::new()));
                }
                pointer_places(c, *value, tail, active, depth + 1, b)
            }
            InstructionKind::MakeVariant { variant, fields } => {
                let Some((ContractProjection::Place(PlaceProjection::Variant(selected)), tail)) =
                    path.split_first()
                else {
                    return Ok(None);
                };
                if selected.variant != *variant {
                    return Ok(Some(Vec::new()));
                }
                let Some(value) = fields.get(selected.index as usize) else {
                    return Ok(None);
                };
                pointer_places(c, *value, tail, active, depth + 1, b)
            }
            InstructionKind::MakeClosure { captures, .. } => {
                let Some((ContractProjection::Capture(index), tail)) = path.split_first() else {
                    return Ok(None);
                };
                let Some(value) = captures.get(*index as usize) else {
                    return Ok(None);
                };
                pointer_places(c, *value, tail, active, depth + 1, b)
            }
            InstructionKind::ExtractField { aggregate, index } => {
                pay(b, path.len().saturating_add(1))?;
                let mut selected = vec![ContractProjection::Place(PlaceProjection::Field(*index))];
                selected.extend_from_slice(path);
                pointer_places(c, *aggregate, &selected, active, depth + 1, b)
            }
            // Field/capture pointer-cell reads could have been indirectly
            // replaced. Do not recover their address from an initial Store.
            _ => Ok(None),
        }
    })();
    pay(
        b,
        key.1
            .len()
            .saturating_mul(active.len().saturating_add(1))
            .saturating_add(1),
    )?;
    active.remove(&key);
    result
}
fn mutable_paths(
    module: &TypedModule,
    ty: &Type,
    path: &mut Path,
    out: &mut Vec<(Path, bool)>,
    depth: usize,
    b: &mut Budget,
) -> CompileResult<()> {
    pay(b, path.len().saturating_add(1))?;
    if depth > b.partial_depth {
        return Err(err(
            "Static mutable effect traversal exceeds its finite depth",
        ));
    }
    match ty {
        Type::Address {
            mutable: true,
            inner,
        } if byte_slot(inner, b, depth + 1)? => {
            pay(b, path.len().saturating_add(2))?;
            let reroutes = matches!(inner.as_ref(), Type::Address { inner, .. } if byte_slot(inner, b, depth + 1)?);
            out.push((path.clone(), reroutes));
        }
        Type::Tuple(fields) => {
            for (index, field) in fields.iter().enumerate() {
                pay(b, 1)?;
                path.push(ContractProjection::Place(PlaceProjection::Field(
                    u32::try_from(index)
                        .map_err(|_| err("Static effect field ordinal overflow"))?,
                )));
                mutable_paths(module, field, path, out, depth + 1, b)?;
                path.pop();
            }
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            for (index, (_, field)) in fields.iter().enumerate() {
                pay(b, 1)?;
                path.push(ContractProjection::Place(PlaceProjection::Field(
                    u32::try_from(index)
                        .map_err(|_| err("Static effect field ordinal overflow"))?,
                )));
                mutable_paths(module, field, path, out, depth + 1, b)?;
                path.pop();
            }
        }
        Type::BorrowedStorage { storage, .. } | Type::Managed(storage) => {
            mutable_paths(module, storage, path, out, depth + 1, b)?
        }
        Type::ErrorUnion { error, result } => {
            for (is_error, child) in [(false, result.as_ref()), (true, error.as_ref())] {
                pay(b, 1)?;
                path.push(ContractProjection::Place(PlaceProjection::ErrorArm(
                    is_error,
                )));
                mutable_paths(module, child, path, out, depth + 1, b)?;
                path.pop();
            }
        }
        Type::Enum { variants, .. } => {
            for (variant, (_, fields)) in variants.iter().enumerate() {
                for (index, child) in fields.iter().enumerate() {
                    pay(b, 1)?;
                    path.push(ContractProjection::Place(PlaceProjection::Variant(
                        zil_ir::VariantProjection {
                            variant: u32::try_from(variant)
                                .map_err(|_| err("Static effect variant ordinal overflow"))?,
                            index: u32::try_from(index)
                                .map_err(|_| err("Static effect field ordinal overflow"))?,
                        },
                    )));
                    mutable_paths(module, child, path, out, depth + 1, b)?;
                    path.pop();
                }
            }
        }
        Type::Array { element, .. }
        | Type::Slice { element }
        | Type::Opaque {
            hidden: element, ..
        } => {
            // No exact finite physical element/opaque selector exists in this
            // source equation. A writable contained pointer is Unknown, not
            // an empty effect or a guessed first element.
            let mut nested = Vec::new();
            mutable_paths(module, element, &mut Vec::new(), &mut nested, depth + 1, b)?;
            if !nested.is_empty() {
                pay(b, path.len().saturating_add(2))?;
                out.push((path.clone(), true));
            }
        }
        Type::Desc(inner) if byte_slot(inner, b, depth + 1)? => {
            pay(b, path.len().saturating_add(2))?;
            out.push((path.clone(), true));
        }
        Type::Closure(id) => {
            let definition = module
                .closures
                .get(id.0 as usize)
                .ok_or_else(|| err("Static effect closure is absent"))?;
            for (index, field) in definition.captures.iter().enumerate() {
                pay(b, 1)?;
                path.push(ContractProjection::Capture(
                    u32::try_from(index)
                        .map_err(|_| err("Static effect capture ordinal overflow"))?,
                ));
                mutable_paths(module, field, path, out, depth + 1, b)?;
                path.pop();
            }
        }
        _ => {}
    }
    Ok(())
}
fn escape(
    c: &Context<'_>,
    value: ValueId,
    writes: &mut Writes,
    b: &mut Budget,
) -> CompileResult<()> {
    let mut paths = Vec::new();
    mutable_paths(
        c.module,
        type_of(c, value)?,
        &mut Vec::new(),
        &mut paths,
        0,
        b,
    )?;
    for (path, reroutes) in paths {
        if reroutes {
            writes.unknown(b)?;
            continue;
        }
        if let Some(places) = pointer_places(c, value, &path, &mut BTreeSet::new(), 0, b)? {
            for place in places {
                writes.place(place, b)?;
            }
        } else {
            writes.unknown(b)?;
        }
    }
    Ok(())
}
pub(super) fn collect(
    c: &Context<'_>,
    reachable: &[bool],
    b: &mut Budget,
) -> CompileResult<Writes> {
    let mut writes = Writes::default();
    for (block, live) in c.f.blocks.iter().zip(reachable) {
        if !*live {
            continue;
        }
        for instruction in &block.instructions {
            pay(b, 1)?;
            let kind = &instruction.kind;
            match kind {
                InstructionKind::CallClosureMut { callee, .. } => {
                    writes.local(c, *callee, vec![], b)?
                }
                InstructionKind::CallClosureBorrowed {
                    callee,
                    kind: CallableKind::FnMut,
                    ..
                } => {
                    // Preserve both the actual SSA receiver and any proved
                    // address/local/capture storage identity. Unknown aliases
                    // cannot recover an environment's initial literal state.
                    writes.place(
                        Place {
                            root: Root::Value(*callee),
                            path: vec![],
                        },
                        b,
                    )?;
                    match type_of(c, *callee)? {
                        Type::Address { .. } => writes.address(c, *callee, vec![], b)?,
                        _ => match c.defs.get(callee.0 as usize).copied().flatten() {
                            Some(
                                InstructionKind::Load(local) | InstructionKind::TakeLocal { local },
                            ) => writes.local(c, *local, vec![], b)?,
                            Some(
                                InstructionKind::ReadCapture { index }
                                | InstructionKind::TakeCapture { index },
                            ) => writes.place(
                                Place {
                                    root: Root::Capture(*index),
                                    path: vec![],
                                },
                                b,
                            )?,
                            Some(InstructionKind::MakeClosure { .. }) => {}
                            _ => writes.unknown(b)?,
                        },
                    }
                }
                _ => {}
            }
            // An indirectly replaced pointer-to-storage cell could reroute a
            // later alias Load. Its initial Store is not an address witness.
            let written = match kind {
                InstructionKind::WriteAddress { value, .. }
                | InstructionKind::StoreStoredBorrow { value, .. }
                | InstructionKind::StorePackageField { value, .. }
                | InstructionKind::WriteDesc { value, .. }
                | InstructionKind::CommitPlaceReplace { value, .. } => Some(*value),
                _ => None,
            };
            if let Some(value) = written
                && let Type::Address { inner, .. } = type_of(c, value)?
                && byte_slot(inner, b, 0)?
            {
                writes.unknown(b)?;
            }
            match kind {
                InstructionKind::StorePackageField {
                    local,
                    index,
                    value,
                } if byte_slot(type_of(c, *value)?, b, 0)? => writes.local(
                    c,
                    *local,
                    vec![ContractProjection::Place(PlaceProjection::Field(*index))],
                    b,
                )?,
                InstructionKind::WriteAddress { address, value }
                    if byte_slot(type_of(c, *value)?, b, 0)? =>
                {
                    writes.address(c, *address, vec![], b)?
                }
                InstructionKind::StoreStoredBorrow {
                    storage,
                    path,
                    value,
                } if byte_slot(type_of(c, *value)?, b, 0)? => {
                    writes.address(c, *storage, field_path(path, b)?, b)?
                }
                InstructionKind::WriteDesc { desc, index, value }
                    if byte_slot(type_of(c, *value)?, b, 0)? =>
                {
                    writes.address(
                        c,
                        *desc,
                        vec![ContractProjection::Place(PlaceProjection::Field(*index))],
                        b,
                    )?
                }
                InstructionKind::CommitPlaceReplace { reservation, value }
                    if byte_slot(type_of(c, *value)?, b, 0)? =>
                {
                    let mut found = false;
                    for (other, live) in c.f.blocks.iter().zip(reachable) {
                        if !*live {
                            continue;
                        }
                        for prepare in &other.instructions {
                            pay(b, 1)?;
                            match &prepare.kind {
                                InstructionKind::ReservePlaceLocal {
                                    reservation: actual,
                                    local,
                                    path,
                                } if actual == reservation => {
                                    found = true;
                                    writes.local(c, *local, field_path(path, b)?, b)?;
                                }
                                InstructionKind::ReservePlaceAddress {
                                    reservation: actual,
                                    address,
                                    path,
                                } if actual == reservation => {
                                    found = true;
                                    writes.address(c, *address, field_path(path, b)?, b)?;
                                }
                                _ => {}
                            }
                        }
                    }
                    if !found {
                        writes.unknown(b)?;
                    }
                }
                _ => {}
            }
            // Whole Store/WriteCapture already join all assigned sources.
            // Transparent address formation/transport does not itself mutate.
            // Every other operation conservatively exposes mutable byte-slot
            // paths, covering calls, indirect/closure calls and escaping
            // capture/task/registration construction without trusting effects.
            if !matches!(
                kind,
                InstructionKind::Constant(_)
                    | InstructionKind::StaticUtf8 { .. }
                    | InstructionKind::Load(_)
                    | InstructionKind::TakeLocal { .. }
                    | InstructionKind::Store { .. }
                    | InstructionKind::WriteCapture { .. }
                    | InstructionKind::StorageLive(_)
                    | InstructionKind::StorageDead(_)
                    | InstructionKind::BorrowLocal { .. }
                    | InstructionKind::BorrowLocalMut { .. }
                    | InstructionKind::BorrowCapture { .. }
                    | InstructionKind::BorrowPlaceLocal { .. }
                    | InstructionKind::BorrowFieldLocal { .. }
                    | InstructionKind::BorrowPackageField { .. }
                    | InstructionKind::BorrowPlaceAddress { .. }
                    | InstructionKind::AddressField { .. }
                    | InstructionKind::ReborrowAddress { .. }
                    | InstructionKind::DescribeAddress { .. }
                    | InstructionKind::ReadAddress { .. }
                    | InstructionKind::LoadStoredBorrow { .. }
                    | InstructionKind::ReservePlaceLocal { .. }
                    | InstructionKind::ReservePlaceAddress { .. }
                    | InstructionKind::AbortPlaceReservation { .. }
                    | InstructionKind::MakeAggregate { .. }
                    | InstructionKind::MakePackage { .. }
                    | InstructionKind::MakeOwnedTuple { .. }
                    | InstructionKind::MakeVariant { .. }
                    | InstructionKind::MakeErrorUnion { .. }
                    | InstructionKind::ExtractField { .. }
                    | InstructionKind::ReadPackageField { .. }
                    | InstructionKind::TakePackageField { .. }
                    | InstructionKind::TakeTupleField { .. }
                    | InstructionKind::TakeVariantField { .. }
                    | InstructionKind::WriteAddress { .. }
                    | InstructionKind::StoreStoredBorrow { .. }
                    | InstructionKind::WriteDesc { .. }
                    | InstructionKind::StorePackageField { .. }
                    | InstructionKind::CommitPlaceReplace { .. }
            ) {
                for value in crate::resources::uses(kind) {
                    pay(b, 1)?;
                    escape(c, value, &mut writes, b)?;
                }
                if let InstructionKind::CallClosureMut { callee, .. }
                | InstructionKind::CallClosureShared { callee, .. } = kind
                {
                    if let Some(values) = c.stores.get(callee) {
                        pay(b, c.stores.len().saturating_add(values.len()))?;
                        for value in values {
                            escape(c, *value, &mut writes, b)?;
                        }
                    } else {
                        let ty =
                            &c.f.locals
                                .get(callee.0 as usize)
                                .ok_or_else(|| err("Static called environment local is absent"))?
                                .ty;
                        let mut paths = Vec::new();
                        mutable_paths(c.module, ty, &mut Vec::new(), &mut paths, 0, b)?;
                        if !paths.is_empty() {
                            writes.unknown(b)?;
                        }
                    }
                }
            }
        }
    }
    Ok(writes)
}
