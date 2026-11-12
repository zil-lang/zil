//! Function-local metadata indices for final return-relation validation.
//! Shapes are actual borrowed type nodes; no relation/permission conclusion is cached.
use super::*;

pub(super) fn boundary<'t, 'p>(
    ty: &'t Type,
    path: &'p [PlaceProjection],
    b: &mut Budget,
) -> Check<(
    &'t StorageLifeShape,
    &'p [PlaceProjection],
    Vec<LifeTypeProjection>,
)> {
    let mut current = ty;
    let mut rest = path;
    let mut classes = Vec::new();
    loop {
        b.spend(1)?;
        if classes.len() > b.limits.max_call_depth {
            return Err(limit("stored relation boundary depth exceeded"));
        }
        match current {
            Type::BorrowedStorage { life, .. } => return Ok((life, rest, classes)),
            Type::Address { inner, .. } => {
                b.spend(1)?;
                classes.push(LifeTypeProjection::Pointee);
                current = inner;
            }
            Type::Tuple(children) => {
                let Some((PlaceProjection::Field(index), tail)) = rest.split_first() else {
                    return Err(err("compound relation omits its actual child ordinal"));
                };
                current = children
                    .get(*index as usize)
                    .ok_or_else(|| err("compound relation child is absent"))?;
                b.spend(1)?;
                classes.push(LifeTypeProjection::Place(PlaceProjection::Field(*index)));
                rest = tail;
            }
            Type::ErrorUnion { error, result } if b.stored_enum_addresses => {
                let Some((PlaceProjection::ErrorArm(arm), tail)) = rest.split_first() else {
                    return Err(err(
                        "stored sum relation omits its actual ErrorArm boundary",
                    ));
                };
                current = if *arm { error } else { result };
                b.spend(1)?;
                classes.push(LifeTypeProjection::Place(PlaceProjection::ErrorArm(*arm)));
                rest = tail;
            }
            _ => return Err(err("physical leaf has no actual qualified boundary")),
        }
    }
}

struct ShapeIndex {
    rows: Vec<(Path, u32, bool)>,
}
impl ShapeIndex {
    fn at(&self, path: &[PlaceProjection], b: &mut Budget) -> Check<Result<usize, usize>> {
        let (mut low, mut high) = (0usize, self.rows.len());
        while low < high {
            let mid = low + (high - low) / 2;
            b.spend(
                self.rows[mid]
                    .0
                    .len()
                    .saturating_add(path.len())
                    .saturating_add(1),
            )?;
            match self.rows[mid].0.as_slice().cmp(path) {
                std::cmp::Ordering::Less => low = mid + 1,
                std::cmp::Ordering::Greater => high = mid,
                std::cmp::Ordering::Equal => return Ok(Ok(mid)),
            }
        }
        Ok(Err(low))
    }
    fn build(shape: &StorageLifeShape, b: &mut Budget) -> Check<Self> {
        b.spend(shape.slots.len().saturating_add(1))?;
        let mut index = Self { rows: Vec::new() };
        for row in &shape.slots {
            match index.at(&row.path, b)? {
                Ok(at) => {
                    b.spend(1)?;
                    index.rows[at].2 = true;
                }
                Err(at) => {
                    let growth = if index.rows.len() == index.rows.capacity() {
                        index.rows.len()
                    } else {
                        0
                    };
                    b.spend(
                        row.path
                            .len()
                            .saturating_add(1)
                            .saturating_add(index.rows.len() - at)
                            .saturating_add(growth),
                    )?;
                    index.rows.insert(at, (row.path.clone(), row.binder, false));
                }
            }
        }
        Ok(index)
    }
}

pub(super) struct Relations<'f> {
    function: &'f TypedFunction,
    shapes: Vec<(&'f StorageLifeShape, ShapeIndex)>,
}
impl<'f> Relations<'f> {
    pub(super) fn new(function: &'f TypedFunction, b: &mut Budget) -> Check<Self> {
        b.spend(1)?;
        Ok(Self {
            function,
            shapes: Vec::new(),
        })
    }
    fn local(
        &mut self,
        shape: &'f StorageLifeShape,
        path: &[PlaceProjection],
        b: &mut Budget,
    ) -> Check<u32> {
        b.spend(self.shapes.len().saturating_add(1))?;
        let at = if let Some(at) = self
            .shapes
            .iter()
            .position(|(actual, _)| std::ptr::eq(*actual, shape))
        {
            at
        } else {
            let index = ShapeIndex::build(shape, b)?;
            let growth = if self.shapes.len() == self.shapes.capacity() {
                self.shapes.len()
            } else {
                0
            };
            b.spend(growth.saturating_add(1))?;
            let at = self.shapes.len();
            self.shapes.push((shape, index));
            at
        };
        let index = &self.shapes[at].1;
        let at = index
            .at(path, b)?
            .map_err(|_| err("relation selected an absent physical stored leaf"))?;
        b.spend(1)?;
        let (_, binder, duplicate) = index.rows[at];
        if duplicate {
            return Err(err("duplicate actual stored leaf"));
        }
        Ok(binder)
    }
    pub(super) fn get(
        &mut self,
        base: StorageBase,
        path: &[PlaceProjection],
        b: &mut Budget,
    ) -> Check<StorageBinder> {
        let f: &'f TypedFunction = self.function;
        let c = f
            .storage_life
            .as_ref()
            .ok_or_else(|| err("stored signature lacks complete relation"))?;
        if path.is_empty() {
            b.spend(c.bindings.len())?;
            return super::signature_life::outer(c, base)
                .ok_or_else(|| err("signature omits actual outer borrow"));
        }
        let ty = match base {
            StorageBase::Parameter(i) => f
                .parameters
                .get(i as usize)
                .and_then(|l| f.locals.get(l.0 as usize))
                .map(|l| &l.ty),
            StorageBase::Result => Some(&f.return_type),
            StorageBase::Capture(_) => None,
        }
        .ok_or_else(|| err("stored relation base is absent"))?;
        let array = if let Type::Address { inner, .. } = ty {
            inner.as_ref()
        } else {
            ty
        };
        if shared_array_shape(array, b)? {
            let Type::Array { length, .. } = array else {
                unreachable!()
            };
            if !matches!(path, [PlaceProjection::Field(index)] if *index < *length) {
                return Err(err("array relation has no actual bounded element"));
            }
            b.spend(c.bindings.len().saturating_mul(2))?;
            return c.bindings.iter().find(|row| row.position.base == base
            && matches!(row.position.path.as_slice(), [ContractProjection::Place(step)] if *step == path[0]))
            .map(|row| row.binder).ok_or_else(|| err("array element lacks its actual signature binding"));
        }
        let (shape, leaf_path, class_path) = boundary(ty, path, b)?;
        let local = self.local(shape, leaf_path, b)?;
        b.spend(c.classes.len())?;
        for row in &c.classes {
            b.spend(row.position.path.len().saturating_add(1))?;
            if row.position.base == base
                && row.position.class == local
                && row.position.path == class_path
            {
                return Ok(row.binder);
            }
        }
        Err(err("signature omits actual qualified local class"))
    }
}

#[cfg(test)]
#[path = "relation_index_tests.rs"]
mod tests;
