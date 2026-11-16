//! Request-local exact structural geometry identities. Hashes select bounded
//! candidates only; every new identity follows complete Type/domain equality.
use super::*;
use std::hash::{Hash, Hasher};

#[derive(Clone)]
pub(super) struct Receipt {
    epoch: Arc<()>,
    limits: (usize, usize),
    module: Option<usize>,
    pub(super) type_id: usize,
    pub(super) geometry_id: usize,
    raw_work: usize,
    shape_work: usize,
}
#[derive(Clone, Copy)]
enum Node {
    Root,
    Field(Option<usize>, usize),
}
#[derive(Clone)]
pub(crate) struct TypeRecord {
    owner: Arc<Geometry>,
    node: Node,
    id: usize,
    raw_work: usize,
    construction: Option<Arc<Construction>>,
}
#[derive(PartialEq, Eq, Hash)]
struct Construction {
    kind: Constructor,
    children: Vec<usize>,
}
#[derive(Clone)]
pub(crate) struct GeometryRecord {
    owner: Arc<Geometry>,
    receipt: Receipt,
}
fn limits(b: &Budget) -> (usize, usize) {
    (b.partial_depth, b.helper_nodes)
}
fn quota(message: &str) -> Vec<zil_base::Diagnostic> {
    vec![zil_base::Diagnostic::limit(
        zil_base::Phase::Analyze,
        message,
        None,
    )]
}
fn next(counter: &mut usize) -> CompileResult<usize> {
    let id = *counter;
    *counter = counter
        .checked_add(1)
        .ok_or_else(|| quota("geometry identity count exceeded"))?;
    Ok(id)
}

struct PaidHash<'a> {
    inner: std::collections::hash_map::DefaultHasher,
    budget: &'a mut Budget,
    error: Option<Vec<zil_base::Diagnostic>>,
}
impl PaidHash<'_> {
    fn number(&mut self, mut value: u128) {
        let mut bytes = [0u8; 19];
        let mut length = 0;
        loop {
            let mut byte = (value & 127) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 128;
            }
            bytes[length] = byte;
            length += 1;
            if value == 0 {
                break;
            }
        }
        self.write(&bytes[..length]);
    }
}
impl Hasher for PaidHash<'_> {
    fn finish(&self) -> u64 {
        self.inner.finish()
    }
    fn write(&mut self, bytes: &[u8]) {
        if self.error.is_some() {
            return;
        }
        if let Err(error) = self.budget.spend(
            bytes.len().saturating_add(1),
            "semantic geometry complete structure hash",
        ) {
            self.error = Some(error);
        } else {
            self.inner.write(bytes);
        }
    }
    fn write_u8(&mut self, n: u8) {
        self.number(n as u128);
    }
    fn write_u16(&mut self, n: u16) {
        self.number(n as u128);
    }
    fn write_u32(&mut self, n: u32) {
        self.number(n as u128);
    }
    fn write_u64(&mut self, n: u64) {
        self.number(n as u128);
    }
    fn write_u128(&mut self, n: u128) {
        self.number(n);
    }
    fn write_usize(&mut self, n: usize) {
        self.number(n as u128);
    }
    fn write_i8(&mut self, n: i8) {
        self.number(n as u8 as u128);
    }
    fn write_i16(&mut self, n: i16) {
        self.number(n as u16 as u128);
    }
    fn write_i32(&mut self, n: i32) {
        self.number(n as u32 as u128);
    }
    fn write_i64(&mut self, n: i64) {
        self.number(n as u64 as u128);
    }
    fn write_i128(&mut self, n: i128) {
        self.number(n as u128);
    }
    fn write_isize(&mut self, n: isize) {
        self.number(n as usize as u128);
    }
}
fn hash<T: Hash + ?Sized>(value: &T, b: &mut Budget) -> CompileResult<u64> {
    let mut hash = PaidHash {
        inner: std::collections::hash_map::DefaultHasher::new(),
        budget: b,
        error: None,
    };
    value.hash(&mut hash);
    let digest = hash.finish();
    if let Some(error) = hash.error {
        Err(error)
    } else {
        Ok(digest)
    }
}
fn node(owner: &Geometry, selector: Node) -> CompileResult<&Type> {
    let Node::Field(variant, field) = selector else {
        return Ok(&owner.ty);
    };
    let storage = match &owner.ty {
        Type::BorrowedStorage { storage, .. } => storage.as_ref(),
        ty => ty,
    };
    match (storage, variant) {
        (Type::Record { fields, .. } | Type::Package { fields, .. }, None) => {
            fields.get(field).map(|(_, ty)| ty)
        }
        (Type::Enum { variants, .. }, Some(variant)) => variants
            .get(variant)
            .and_then(|(_, fields)| fields.get(field)),
        _ => None,
    }
    .ok_or_else(|| fail("nominal child geometry has no exact physical field coordinate"))
}
fn type_candidates(
    ty: &Type,
    raw_work: usize,
    b: &mut Budget,
) -> CompileResult<(u64, Option<usize>)> {
    if let Some((id, _)) = registered_type(ty, b)? {
        return Ok((0, Some(id)));
    }
    let digest = hash(ty, b)?;
    let key = (b.geometry_module, limits(b), digest);
    b.lookup(
        b.geometry_type_catalog.len(),
        "semantic geometry structural Type query",
    )?;
    let width = b.geometry_type_catalog.get(&key).map_or(0, Vec::len);
    b.spend(width.saturating_add(1), "semantic geometry Type candidates")?;
    let candidates = b
        .geometry_type_catalog
        .get(&key)
        .cloned()
        .unwrap_or_default();
    for candidate in candidates {
        let value = node(&candidate.owner, candidate.node)?;
        b.spend(
            raw_work.max(candidate.raw_work),
            "complete Type equality comparison",
        )?;
        if ty == value {
            return Ok((digest, Some(candidate.id)));
        }
    }
    Ok((digest, None))
}
fn geometry_candidate(
    type_id: usize,
    life: &StorageLifeShape,
    shape_work: usize,
    b: &mut Budget,
) -> CompileResult<(u64, Option<GeometryRecord>)> {
    let digest = hash(&(type_id, life), b)?;
    let key = (b.geometry_module, limits(b), digest);
    b.lookup(
        b.geometry_catalog.len(),
        "semantic geometry structural domain query",
    )?;
    let width = b.geometry_catalog.get(&key).map_or(0, Vec::len);
    b.spend(
        width.saturating_add(1),
        "semantic geometry domain candidates",
    )?;
    let candidates = b.geometry_catalog.get(&key).cloned().unwrap_or_default();
    for candidate in candidates {
        if candidate.receipt.type_id != type_id {
            continue;
        }
        b.spend(
            shape_work.max(candidate.receipt.shape_work),
            "complete geometry domain equality comparison",
        )?;
        if candidate.owner.life == *life {
            return Ok((digest, Some(candidate)));
        }
    }
    Ok((digest, None))
}
fn add_type(
    owner: &Arc<Geometry>,
    selector: Node,
    digest: u64,
    raw_work: usize,
    b: &mut Budget,
) -> CompileResult<usize> {
    let id = next(&mut b.geometry_next_type)?;
    let key = (b.geometry_module, limits(b), digest);
    b.lookup(
        b.geometry_type_catalog.len(),
        "semantic geometry structural Type publication",
    )?;
    let moved = b.geometry_type_catalog.get(&key).map_or(0, |rows| {
        if rows.len() == rows.capacity() {
            rows.len()
        } else {
            0
        }
    });
    b.spend(
        moved.saturating_add(1),
        "semantic geometry Type record allocation",
    )?;
    let rows = b.geometry_type_catalog.entry(key).or_default();
    rows.try_reserve(1)
        .map_err(|_| quota("geometry Type record allocation failed"))?;
    rows.push(TypeRecord {
        owner: Arc::clone(owner),
        node: selector,
        id,
        raw_work,
        construction: None,
    });
    Ok(id)
}
fn add_geometry(
    owner: &Arc<Geometry>,
    digest: u64,
    type_id: usize,
    raw_work: usize,
    shape_work: usize,
    b: &mut Budget,
) -> CompileResult<Receipt> {
    let geometry_id = next(&mut b.geometry_next_value)?;
    let receipt = Receipt {
        epoch: Arc::clone(&b.geometry_epoch),
        limits: limits(b),
        module: b.geometry_module,
        type_id,
        geometry_id,
        raw_work,
        shape_work,
    };
    let key = (b.geometry_module, limits(b), digest);
    b.lookup(
        b.geometry_catalog.len(),
        "semantic geometry structural domain publication",
    )?;
    let moved = b.geometry_catalog.get(&key).map_or(0, |rows| {
        if rows.len() == rows.capacity() {
            rows.len()
        } else {
            0
        }
    });
    b.spend(
        moved.saturating_add(1),
        "semantic geometry domain record allocation",
    )?;
    let rows = b.geometry_catalog.entry(key).or_default();
    rows.try_reserve(1)
        .map_err(|_| quota("geometry domain record allocation failed"))?;
    rows.push(GeometryRecord {
        owner: Arc::clone(owner),
        receipt: receipt.clone(),
    });
    Ok(receipt)
}
fn register(owner: &Arc<Geometry>, receipt: &Receipt, b: &mut Budget) -> CompileResult<()> {
    b.lookup(b.geometry_types.len(), "semantic geometry root publication")?;
    b.spend(1, "semantic geometry shared result")?;
    b.geometry_types.insert(
        &owner.ty as *const Type as usize,
        TypeEntry {
            owner: Arc::clone(owner),
            raw_work: receipt.raw_work,
            raw_limits: receipt.limits,
            module: receipt.module,
            nominal: None,
            type_id: receipt.type_id,
        },
    );
    let _ = owner.receipt.set(receipt.clone());
    Ok(())
}
fn bounds(ty: &Type, life: &StorageLifeShape, b: &mut Budget) -> CompileResult<(usize, usize)> {
    let raw = if let Some((_, raw)) = registered_type(ty, b)? {
        raw
    } else {
        let before = b.remaining();
        crate::dyn_metadata::ty(ty, b)?;
        before - b.remaining()
    };
    let before = b.remaining();
    crate::dyn_metadata::shape(life, b)?;
    Ok((raw, before - b.remaining()))
}
pub(super) fn construct(
    kind: Constructor,
    children: &[&Arc<Geometry>],
    life: StorageLifeShape,
    b: &mut Budget,
) -> CompileResult<Arc<Geometry>> {
    let valid = match kind {
        Constructor::Unary(_) => children.len() == 1,
        Constructor::Tuple => true,
        Constructor::ErrorUnion => children.len() == 2,
    };
    if !valid {
        return Err(fail(
            "pure geometry constructor changes its exact operand inventory",
        ));
    }
    if children.len() > b.helper_nodes {
        return Err(quota("geometry constructor operand count exceeded"));
    }
    b.spend(
        children.len().saturating_add(1),
        "semantic geometry constructor operands",
    )?;
    let mut ids = Vec::new();
    ids.try_reserve_exact(children.len())
        .map_err(|_| quota("geometry constructor allocation failed"))?;
    let mut copy_work = 0usize;
    for child in children {
        let receipt = owned(child, b)?;
        ids.push(receipt.type_id);
        copy_work = copy_work
            .checked_add(receipt.raw_work)
            .ok_or_else(|| quota("geometry constructor copy work overflow"))?;
    }
    let construction = Construction {
        kind,
        children: ids,
    };
    let digest = hash(&(b"geometry-constructor-v1", &construction), b)?;
    let key = (b.geometry_module, limits(b), digest);
    b.lookup(
        b.geometry_type_catalog.len(),
        "semantic geometry constructor query",
    )?;
    let width = b.geometry_type_catalog.get(&key).map_or(0, Vec::len);
    b.spend(
        width.saturating_add(1),
        "semantic geometry constructor candidates",
    )?;
    let candidates = b
        .geometry_type_catalog
        .get(&key)
        .cloned()
        .unwrap_or_default();
    for candidate in candidates {
        b.spend(
            children.len().saturating_add(1),
            "semantic geometry constructor identity",
        )?;
        if candidate.construction.as_deref() == Some(&construction) {
            // The alias was published only after the complete cold Type and
            // domain check. It contains no current semantic class images.
            if !matches!(candidate.node, Node::Root) {
                return Err(fail(
                    "geometry constructor alias has another owned Type coordinate",
                ));
            }
            return with_life(&candidate.owner, &life, b);
        }
    }
    // A miss really constructs this Type. Charge every actual child clone;
    // cold bounds, hashing and full equality still follow below.
    b.spend(
        copy_work.saturating_add(children.len()).saturating_add(1),
        "semantic geometry constructor owned copy",
    )?;
    let ty = match kind {
        Constructor::Unary(op) => {
            let inner = Box::new(children[0].ty.clone());
            match op {
                Unary::Address(mutable) => Type::Address { mutable, inner },
                Unary::Managed => Type::Managed(inner),
                Unary::Desc => Type::Desc(inner),
                Unary::OwnedStore => Type::OwnedStore { element: inner },
                Unary::Mutex => Type::Mutex { payload: inner },
                Unary::Registration => Type::Registration { environment: inner },
                Unary::AsyncRegistration => Type::AsyncRegistration { environment: inner },
                Unary::Slice => Type::Slice { element: inner },
                Unary::Array(length) => Type::Array {
                    element: inner,
                    length,
                },
            }
        }
        Constructor::Tuple => {
            let mut fields = Vec::new();
            fields
                .try_reserve_exact(children.len())
                .map_err(|_| quota("geometry tuple allocation failed"))?;
            for child in children {
                fields.push(child.ty.clone());
            }
            Type::Tuple(fields)
        }
        Constructor::ErrorUnion => Type::ErrorUnion {
            error: Box::new(children[0].ty.clone()),
            result: Box::new(children[1].ty.clone()),
        },
    };
    let owner = Arc::new(Geometry::new(ty, life));
    let receipt = owned(&owner, b)?;
    b.lookup(
        b.geometry_type_catalog.len(),
        "semantic geometry constructor publication",
    )?;
    let moved = b.geometry_type_catalog.get(&key).map_or(0, |rows| {
        if rows.len() == rows.capacity() {
            rows.len()
        } else {
            0
        }
    });
    b.spend(
        moved.saturating_add(1),
        "semantic geometry constructor record allocation",
    )?;
    let rows = b.geometry_type_catalog.entry(key).or_default();
    rows.try_reserve(1)
        .map_err(|_| quota("geometry constructor record allocation failed"))?;
    rows.push(TypeRecord {
        owner: Arc::clone(&owner),
        node: Node::Root,
        id: receipt.type_id,
        raw_work: receipt.raw_work,
        construction: Some(Arc::new(construction)),
    });
    Ok(owner)
}
fn with_life(
    source: &Arc<Geometry>,
    life: &StorageLifeShape,
    b: &mut Budget,
) -> CompileResult<Arc<Geometry>> {
    let source_receipt = owned(source, b)?;
    let before = b.remaining();
    crate::dyn_metadata::shape(life, b)?;
    let shape_work = before - b.remaining();
    let (digest, existing) = geometry_candidate(source_receipt.type_id, life, shape_work, b)?;
    if let Some(existing) = existing {
        b.spend(1, "semantic geometry shared result")?;
        return Ok(existing.owner);
    }
    b.spend(
        source_receipt
            .raw_work
            .saturating_add(shape_work)
            .saturating_add(1),
        "semantic geometry owned copy",
    )?;
    let owner = Arc::new(Geometry::new(source.ty.clone(), life.clone()));
    let receipt = add_geometry(
        &owner,
        digest,
        source_receipt.type_id,
        source_receipt.raw_work,
        shape_work,
        b,
    )?;
    register(&owner, &receipt, b)?;
    Ok(owner)
}
pub(super) fn borrowed(
    ty: &Type,
    life: &StorageLifeShape,
    b: &mut Budget,
) -> CompileResult<Arc<Geometry>> {
    let (raw_work, shape_work) = bounds(ty, life, b)?;
    let (type_digest, type_id) = type_candidates(ty, raw_work, b)?;
    let (geometry_digest, existing) = if let Some(type_id) = type_id {
        geometry_candidate(type_id, life, shape_work, b)?
    } else {
        (0, None)
    };
    if let Some(existing) = existing {
        b.spend(1, "semantic geometry shared result")?;
        return Ok(existing.owner);
    }
    b.spend(
        raw_work.saturating_add(shape_work).saturating_add(1),
        "semantic geometry owned copy",
    )?;
    let owner = Arc::new(Geometry::new(ty.clone(), life.clone()));
    let type_id = match type_id {
        Some(id) => id,
        None => add_type(&owner, Node::Root, type_digest, raw_work, b)?,
    };
    let digest = if geometry_digest == 0 {
        hash(&(type_id, life), b)?
    } else {
        geometry_digest
    };
    let receipt = add_geometry(&owner, digest, type_id, raw_work, shape_work, b)?;
    register(&owner, &receipt, b)?;
    Ok(owner)
}
fn current_receipt<'a>(owner: &'a Geometry, b: &Budget) -> Option<&'a Receipt> {
    owner.receipt.get().filter(|row| {
        row.module == b.geometry_module
            && row.limits == limits(b)
            && Arc::ptr_eq(&row.epoch, &b.geometry_epoch)
    })
}
pub(super) fn owned(owner: &Arc<Geometry>, b: &mut Budget) -> CompileResult<Receipt> {
    b.spend(1, "semantic geometry immutable receipt")?;
    if let Some(receipt) = current_receipt(owner, b) {
        return Ok(receipt.clone());
    }
    let (raw_work, shape_work) = bounds(&owner.ty, &owner.life, b)?;
    let (type_digest, type_id) = type_candidates(&owner.ty, raw_work, b)?;
    let type_id = match type_id {
        Some(id) => id,
        None => add_type(owner, Node::Root, type_digest, raw_work, b)?,
    };
    let (digest, existing) = geometry_candidate(type_id, &owner.life, shape_work, b)?;
    let receipt = if let Some(existing) = existing {
        Receipt {
            raw_work,
            shape_work,
            ..existing.receipt
        }
    } else {
        add_geometry(owner, digest, type_id, raw_work, shape_work, b)?
    };
    register(owner, &receipt, b)?;
    Ok(receipt)
}
pub(super) fn field_equal(
    owner: &Arc<Geometry>,
    variant: Option<usize>,
    field: usize,
    expected: &Arc<Geometry>,
    b: &mut Budget,
) -> CompileResult<bool> {
    // Validate the exact coordinate on every query, including a cache hit.
    let actual = node(owner, Node::Field(variant, field))?;
    b.spend(1, "semantic geometry physical field selector")?;
    let owner_receipt = owned(owner, b)?;
    let key = (owner_receipt.type_id, variant, field);
    b.lookup(
        b.geometry_child_types.len(),
        "semantic geometry physical child Type query",
    )?;
    let (id, raw_work) = if let Some(row) = b.geometry_child_types.get(&key).copied() {
        row
    } else {
        let before = b.remaining();
        crate::dyn_metadata::ty(actual, b)?;
        let raw = before - b.remaining();
        let (digest, id) = type_candidates(actual, raw, b)?;
        let id = match id {
            Some(id) => id,
            None => add_type(owner, Node::Field(variant, field), digest, raw, b)?,
        };
        b.lookup(
            b.geometry_child_types.len(),
            "semantic geometry physical child Type publication",
        )?;
        b.spend(1, "semantic geometry child raw comparison bound")?;
        b.geometry_child_types.insert(key, (id, raw));
        (id, raw)
    };
    b.spend(1, "semantic geometry immutable field Type receipt")?;
    if let Some(receipt) = current_receipt(expected, b) {
        return Ok(id == receipt.type_id);
    }
    // This consumer compares only a physical child Type. A temporary expected
    // value need not become a globally retained geometry merely for equality.
    // Preserve full raw Type AND domain validation, then compare every Type
    // component directly; hashes or actual class images grant no shortcut.
    let (expected_work, _) = bounds(&expected.ty, &expected.life, b)?;
    b.spend(
        raw_work.max(expected_work),
        "complete nominal child Type equality comparison",
    )?;
    Ok(actual == &expected.ty)
}
