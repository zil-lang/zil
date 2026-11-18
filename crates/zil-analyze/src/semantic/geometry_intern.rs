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
pub(super) enum Node {
    Root,
    Child(Child),
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
pub(super) fn node(owner: &Geometry, selector: Node) -> CompileResult<&Type> {
    let Node::Child(child) = selector else {
        return Ok(&owner.ty);
    };
    match child {
        Child::Inner => match &owner.ty {
            Type::Address { inner, .. }
            | Type::Managed(inner)
            | Type::Desc(inner)
            | Type::Slice { element: inner }
            | Type::Array { element: inner, .. }
            | Type::OwnedStore { element: inner }
            | Type::Mutex { payload: inner }
            | Type::Registration { environment: inner }
            | Type::AsyncRegistration { environment: inner }
            | Type::Opaque { hidden: inner, .. }
            | Type::BorrowedStorage { storage: inner, .. }
            | Type::Task { result: inner, .. }
            | Type::MutexGuard { payload: inner, .. } => Ok(inner.as_ref()),
            _ => Err(fail("retained child Type has another physical constructor")),
        },
        Child::UnionError | Child::UnionResult => match &owner.ty {
            Type::ErrorUnion { error, result } => Ok(if child == Child::UnionError {
                error
            } else {
                result
            }),
            _ => Err(fail(
                "retained sum child Type has another physical constructor",
            )),
        },
        Child::Field(variant, field) => {
            let storage = match &owner.ty {
                Type::BorrowedStorage { storage, .. } => storage.as_ref(),
                ty => ty,
            };
            match (storage, variant) {
                // Anonymous products use the same exact physical field
                // coordinate; their complete domain is supplied separately
                // by the caller, just as for a nominal product.
                (Type::Tuple(fields), None) => fields.get(field),
                (Type::Record { fields, .. } | Type::Package { fields, .. }, None) => {
                    fields.get(field).map(|(_, ty)| ty)
                }
                (Type::Enum { variants, .. }, Some(variant)) => variants
                    .get(variant)
                    .and_then(|(_, fields)| fields.get(field)),
                _ => None,
            }
            .ok_or_else(|| fail("child geometry has no exact physical field coordinate"))
        }
    }
}

fn structural_type_candidate(
    ty: &Type,
    raw_work: usize,
    b: &mut Budget,
) -> CompileResult<(u64, Option<usize>)> {
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
            coordinate: Node::Root,
        },
    );
    let _ = owner.receipt.set(receipt.clone());
    Ok(())
}
pub(super) fn retain_payload(parent: &Type, payload: &Type, b: &mut Budget) -> CompileResult<()> {
    b.lookup(b.geometry_types.len(), "semantic geometry payload parent")?;
    b.spend(1, "semantic geometry shared result")?;
    let owner = b
        .geometry_types
        .get(&(parent as *const Type as usize))
        .filter(|row| {
            std::ptr::eq(&row.owner.ty, parent)
                && row.raw_limits == limits(b)
                && row.module == b.geometry_module
        })
        .map(|row| Arc::clone(&row.owner));
    let Some(owner) = owner else {
        return Ok(());
    };
    if !std::ptr::eq(node(&owner, Node::Child(Child::Inner))?, payload) {
        return Err(fail("retained payload changes its exact parent Type"));
    }
    if registered_type(payload, b)?.is_some() {
        return Ok(());
    }
    let before = b.remaining();
    crate::dyn_metadata::ty(payload, b)?;
    let raw_work = before - b.remaining();
    let (digest, id) = structural_type_candidate(payload, raw_work, b)?;
    let type_id = match id {
        Some(id) => id,
        None => add_type(&owner, Node::Child(Child::Inner), digest, raw_work, b)?,
    };
    b.lookup(
        b.geometry_types.len(),
        "semantic geometry payload publication",
    )?;
    b.spend(1, "semantic geometry shared result")?;
    let raw_limits = limits(b);
    let module = b.geometry_module;
    b.geometry_types.insert(
        payload as *const Type as usize,
        TypeEntry {
            owner,
            raw_work,
            raw_limits,
            module,
            nominal: None,
            type_id,
            coordinate: Node::Child(Child::Inner),
        },
    );
    Ok(())
}
/// The authenticated coordinate supplies both the type identity and the
/// structural comparison bound. On a miss, measure the complete type once.
fn bounded_type_candidate(ty: &Type, b: &mut Budget) -> CompileResult<(usize, u64, Option<usize>)> {
    if let Some((id, work)) = registered_type(ty, b)? {
        return Ok((work, 0, Some(id)));
    }
    let before = b.remaining();
    crate::dyn_metadata::ty(ty, b)?;
    let work = before - b.remaining();
    let (digest, id) = structural_type_candidate(ty, work, b)?;
    Ok((work, digest, id))
}
fn shape_work(life: &StorageLifeShape, b: &mut Budget) -> CompileResult<usize> {
    let before = b.remaining();
    crate::dyn_metadata::shape(life, b)?;
    Ok(before - b.remaining())
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
    // This is a private candidate-bucket discriminator. Exact constructor and
    // ordered child identities below, rather than this hash, select an alias.
    let digest = hash(&(u8::MAX, &construction), b)?;
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
pub(super) fn with_life(
    source: &Arc<Geometry>,
    life: &StorageLifeShape,
    b: &mut Budget,
) -> CompileResult<Arc<Geometry>> {
    let source_receipt = owned(source, b)?;
    let before = b.remaining();
    crate::dyn_metadata::shape(life, b)?;
    let shape_work = before - b.remaining();
    b.spend(
        source_receipt.shape_work.max(shape_work),
        "complete geometry domain equality comparison",
    )?;
    if source.life == *life {
        b.spend(1, "semantic geometry shared result")?;
        return Ok(Arc::clone(source));
    }
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
    let (raw_work, type_digest, type_id) = bounded_type_candidate(ty, b)?;
    let shape_work = shape_work(life, b)?;
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
    let (raw_work, type_digest, type_id) = bounded_type_candidate(&owner.ty, b)?;
    let shape_work = shape_work(&owner.life, b)?;
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
fn child_type(owner: &Arc<Geometry>, child: Child, b: &mut Budget) -> CompileResult<usize> {
    // A Type-only entry retains this exact real parent, whose whole domain was
    // checked independently. It creates no empty-domain child Value.
    b.spend(1, "semantic geometry physical field selector")?;
    let actual = node(owner, Node::Child(child))?;
    // A published coordinate retains its actual parent. Repeated projection
    // can query the child's complete Type directly even under a new life map.
    if let Some((id, _)) = registered_type(actual, b)? {
        return Ok(id);
    }
    let owner_receipt = owned(owner, b)?;
    let key = (owner_receipt.type_id, child);
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
        let (digest, id) = structural_type_candidate(actual, raw, b)?;
        let id = match id {
            Some(id) => id,
            None => add_type(owner, Node::Child(child), digest, raw, b)?,
        };
        b.lookup(
            b.geometry_child_types.len(),
            "semantic geometry physical child Type publication",
        )?;
        b.spend(1, "semantic geometry child raw comparison bound")?;
        b.geometry_child_types.insert(key, (id, raw));
        (id, raw)
    };
    b.lookup(
        b.geometry_types.len(),
        "semantic geometry physical child coordinate",
    )?;
    let address = actual as *const Type as usize;
    b.spend(1, "semantic geometry shared result")?;
    b.geometry_types.insert(
        address,
        TypeEntry {
            owner: Arc::clone(owner),
            raw_work,
            raw_limits: limits(b),
            module: b.geometry_module,
            nominal: None,
            type_id: id,
            coordinate: Node::Child(child),
        },
    );
    Ok(id)
}
pub(super) fn retain_child(
    owner: &Arc<Geometry>,
    child: Child,
    b: &mut Budget,
) -> CompileResult<()> {
    child_type(owner, child, b)?;
    Ok(())
}
pub(super) fn project(
    owner: &Arc<Geometry>,
    child: Child,
    life: &StorageLifeShape,
    b: &mut Budget,
) -> CompileResult<Arc<Geometry>> {
    child_type(owner, child, b)?;
    // The caller's fresh full domain, rather than the parent's shape or a
    // cached class vector, forms the child geometry on every projection.
    borrowed(node(owner, Node::Child(child))?, life, b)
}
pub(super) fn field_equal(
    owner: &Arc<Geometry>,
    variant: Option<usize>,
    field: usize,
    expected: &Arc<Geometry>,
    b: &mut Budget,
) -> CompileResult<bool> {
    let id = child_type(owner, Child::Field(variant, field), b)?;
    // Retain this actual expected owner after complete Type/domain checking.
    // Every current class image and nominal field obligation stays separate.
    let expected = owned(expected, b)?;
    Ok(id == expected.type_id)
}
