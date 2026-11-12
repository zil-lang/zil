//! Bounded Source candidate construction, never an identity or permission proof.
//!
//! A kernel must independently resolve the actual parent, physical path, full
//! child scope and every class map. In particular these candidates provide no
//! root, initialization, tag, access-mode or parent-loan authority.
use crate::*;
use zil_base::Limits;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Projection {
    pub ty: Type,
    /// Child class ordinal -> actual parent class. Repeated parent classes are
    /// preserved; this is a complete relation, not an injectivity assertion.
    pub parent_classes: Vec<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectionError {
    Limit,
    Invalid,
    UnknownScope,
}
type Result<T> = std::result::Result<T, ProjectionError>;
struct Fuel<'a> {
    limits: &'a Limits,
    remaining: &'a mut usize,
    nodes: usize,
}
impl Fuel<'_> {
    fn pay(&mut self, count: usize) -> Result<()> {
        *self.remaining = self
            .remaining
            .checked_sub(count)
            .ok_or(ProjectionError::Limit)?;
        Ok(())
    }
    fn node(&mut self, depth: usize) -> Result<()> {
        if depth > self.limits.max_call_depth || self.nodes >= self.limits.max_nodes {
            return Err(ProjectionError::Limit);
        }
        self.nodes += 1;
        self.pay(1)
    }
    fn domain(&mut self, count: u32) -> Result<()> {
        let count = usize::try_from(count).map_err(|_| ProjectionError::Limit)?;
        if count > self.limits.max_nodes {
            return Err(ProjectionError::Limit);
        }
        self.pay(count.saturating_add(1))
    }
    fn rows(
        &mut self,
        retained: &[StorageRetainedBinding],
        nominal: &[NominalLifeArgument],
    ) -> Result<()> {
        for row in retained {
            self.pay(
                row.path
                    .len()
                    .checked_add(3)
                    .ok_or(ProjectionError::Limit)?,
            )?;
        }
        for row in nominal {
            self.pay(
                row.path
                    .len()
                    .checked_add(4)
                    .ok_or(ProjectionError::Limit)?,
            )?;
        }
        Ok(())
    }
    fn retained(&mut self, life: &RetainedLifeShape, depth: usize) -> Result<()> {
        self.domain(life.binder_count)?;
        self.rows(&life.retained, &life.nominal_arguments)?;
        for port in &life.ports {
            self.pay(2)?;
            self.ty(&port.leaf, depth + 1)?;
        }
        Ok(())
    }
    // Full prepayment precedes all recursive cloning and output allocation.
    fn ty(&mut self, ty: &Type, depth: usize) -> Result<()> {
        self.node(depth)?;
        match ty {
            Type::BorrowedStorage { storage, life } => {
                self.domain(life.binder_count)?;
                for slot in &life.slots {
                    self.pay(
                        slot.path
                            .len()
                            .checked_add(3)
                            .ok_or(ProjectionError::Limit)?,
                    )?;
                }
                self.rows(&life.retained, &life.nominal_arguments)?;
                self.ty(storage, depth + 1)?;
            }
            Type::Task {
                result,
                retained_life,
            } => {
                self.retained(retained_life, depth + 1)?;
                self.ty(result, depth + 1)?;
            }
            Type::MutexGuard {
                payload,
                retained_life,
            } => {
                self.retained(retained_life, depth + 1)?;
                self.ty(payload, depth + 1)?;
            }
            Type::Tuple(fields) => {
                for field in fields {
                    self.ty(field, depth + 1)?;
                }
            }
            Type::Record { name, fields } | Type::Package { name, fields } => {
                self.pay(name.len())?;
                for (name, field) in fields {
                    self.pay(name.len())?;
                    self.ty(field, depth + 1)?;
                }
            }
            Type::Enum { name, variants } => {
                self.pay(name.len())?;
                for (name, fields) in variants {
                    self.pay(name.len())?;
                    for field in fields {
                        self.ty(field, depth + 1)?;
                    }
                }
            }
            Type::Address { inner, .. }
            | Type::Managed(inner)
            | Type::Desc(inner)
            | Type::OwnedStore { element: inner }
            | Type::Slice { element: inner }
            | Type::Array { element: inner, .. }
            | Type::Mutex { payload: inner }
            | Type::Registration { environment: inner }
            | Type::AsyncRegistration { environment: inner }
            | Type::Opaque { hidden: inner, .. } => self.ty(inner, depth + 1)?,
            Type::ErrorUnion { error, result } => {
                self.ty(error, depth + 1)?;
                self.ty(result, depth + 1)?;
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result }
            | Type::BorrowedFunctionPointer {
                parameters, result, ..
            } => {
                for parameter in parameters {
                    self.ty(parameter, depth + 1)?;
                }
                self.ty(result, depth + 1)?;
                if let Type::BorrowedFunctionPointer { life, .. } = ty {
                    self.pay(
                        life.bindings
                            .len()
                            .checked_mul(4)
                            .ok_or(ProjectionError::Limit)?,
                    )?;
                    for binding in &life.bindings {
                        self.pay(binding.position.path.len())?;
                    }
                    self.pay(
                        life.classes
                            .len()
                            .checked_mul(4)
                            .ok_or(ProjectionError::Limit)?,
                    )?;
                    for class in &life.classes {
                        self.pay(class.position.path.len())?;
                    }
                }
            }
            Type::Named(name) | Type::Unsupported(name) => self.pay(name.len())?,
            _ => {}
        }
        Ok(())
    }
}

fn physical_child(ty: &Type, step: PlaceProjection) -> Option<&Type> {
    let ty = if let Type::BorrowedStorage { storage, .. } = ty {
        storage.as_ref()
    } else {
        ty
    };
    match (ty, step) {
        (Type::Tuple(fields), PlaceProjection::Field(i)) => fields.get(i as usize),
        (Type::Record { fields, .. } | Type::Package { fields, .. }, PlaceProjection::Field(i)) => {
            fields.get(i as usize).map(|(_, ty)| ty)
        }
        (Type::Array { element, length }, PlaceProjection::Field(i)) if i < *length => {
            Some(element)
        }
        (Type::Enum { variants, .. }, PlaceProjection::Variant(v)) => {
            variants.get(v.variant as usize)?.1.get(v.index as usize)
        }
        (Type::ErrorUnion { error, result }, PlaceProjection::ErrorArm(error_arm)) => {
            Some(if error_arm { error } else { result })
        }
        _ => None,
    }
}
fn scope(ty: &Type) -> Option<u32> {
    match ty {
        Type::BorrowedStorage { life, .. } => Some(life.binder_count),
        Type::Task { retained_life, .. } | Type::MutexGuard { retained_life, .. } => {
            Some(retained_life.binder_count)
        }
        _ => None,
    }
}
fn retained_class(
    life: &StorageLifeShape,
    path: &[LifeTypeProjection],
    class: u32,
    fuel: &mut Fuel<'_>,
) -> Result<u32> {
    let mut actual = None;
    for row in &life.retained {
        fuel.pay(row.path.len().saturating_add(path.len()).saturating_add(1))?;
        if row.path == path && row.class == class && actual.replace(row.binder).is_some() {
            return Err(ProjectionError::Invalid);
        }
    }
    let actual = actual.ok_or(ProjectionError::UnknownScope)?;
    if actual >= life.binder_count {
        return Err(ProjectionError::Invalid);
    }
    Ok(actual)
}
fn slot_class(
    life: &StorageLifeShape,
    path: &[PlaceProjection],
    fuel: &mut Fuel<'_>,
) -> Result<u32> {
    let mut actual = None;
    for row in &life.slots {
        fuel.pay(row.path.len().saturating_add(path.len()).saturating_add(1))?;
        if row.path == path && actual.replace(row.binder).is_some() {
            return Err(ProjectionError::Invalid);
        }
    }
    let actual = actual.ok_or(ProjectionError::UnknownScope)?;
    if actual >= life.binder_count {
        return Err(ProjectionError::Invalid);
    }
    Ok(actual)
}
fn add(classes: &mut Vec<u32>, binder: u32, fuel: &mut Fuel<'_>) -> Result<()> {
    fuel.pay(classes.len().saturating_add(1))?;
    if !classes.contains(&binder) {
        classes.push(binder);
    }
    Ok(())
}
// Discover classes in actual ordered child scopes, including leafless classes.
// A nested qualified node owns its own class domain; never rescan its slots as
// though their local binder ordinals were in the enclosing parent domain.
#[allow(clippy::too_many_arguments)]
fn discover(
    ty: &Type,
    path: &mut Vec<PlaceProjection>,
    logical: &mut Vec<LifeTypeProjection>,
    life: &StorageLifeShape,
    classes: &mut Vec<u32>,
    fuel: &mut Fuel<'_>,
    depth: usize,
    physical: bool,
) -> Result<()> {
    fuel.node(depth)?;
    if let Some(count) = scope(ty) {
        for class in 0..count {
            add(classes, retained_class(life, logical, class, fuel)?, fuel)?;
        }
        return Ok(());
    }
    match ty {
        Type::Address { inner, .. } | Type::Desc(inner) => {
            if !physical {
                return Err(ProjectionError::UnknownScope);
            }
            add(classes, slot_class(life, path, fuel)?, fuel)?;
            logical.push(LifeTypeProjection::Pointee);
            discover(inner, path, logical, life, classes, fuel, depth + 1, false)?;
            logical.pop();
            Ok(())
        }
        Type::Managed(inner) => {
            logical.push(LifeTypeProjection::Pointee);
            discover(inner, path, logical, life, classes, fuel, depth + 1, false)?;
            logical.pop();
            Ok(())
        }
        Type::OwnedStore { element } | Type::Array { element, .. } if !physical => {
            logical.push(LifeTypeProjection::Element);
            discover(
                element,
                path,
                logical,
                life,
                classes,
                fuel,
                depth + 1,
                false,
            )?;
            logical.pop();
            Ok(())
        }
        Type::Tuple(fields) => {
            for (i, field) in fields.iter().enumerate() {
                let step =
                    PlaceProjection::Field(u32::try_from(i).map_err(|_| ProjectionError::Limit)?);
                path.push(step);
                logical.push(LifeTypeProjection::Place(step));
                discover(
                    field,
                    path,
                    logical,
                    life,
                    classes,
                    fuel,
                    depth + 1,
                    physical,
                )?;
                path.pop();
                logical.pop();
            }
            Ok(())
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            for (i, (_, field)) in fields.iter().enumerate() {
                let step =
                    PlaceProjection::Field(u32::try_from(i).map_err(|_| ProjectionError::Limit)?);
                path.push(step);
                logical.push(LifeTypeProjection::Place(step));
                discover(
                    field,
                    path,
                    logical,
                    life,
                    classes,
                    fuel,
                    depth + 1,
                    physical,
                )?;
                path.pop();
                logical.pop();
            }
            Ok(())
        }
        Type::Enum { variants, .. } => {
            for (v, (_, fields)) in variants.iter().enumerate() {
                for (i, field) in fields.iter().enumerate() {
                    let step = PlaceProjection::Variant(VariantProjection {
                        variant: u32::try_from(v).map_err(|_| ProjectionError::Limit)?,
                        index: u32::try_from(i).map_err(|_| ProjectionError::Limit)?,
                    });
                    path.push(step);
                    logical.push(LifeTypeProjection::Place(step));
                    discover(
                        field,
                        path,
                        logical,
                        life,
                        classes,
                        fuel,
                        depth + 1,
                        physical,
                    )?;
                    path.pop();
                    logical.pop();
                }
            }
            Ok(())
        }
        Type::ErrorUnion { error, result } => {
            for (arm, ty) in [(true, error.as_ref()), (false, result.as_ref())] {
                let step = PlaceProjection::ErrorArm(arm);
                path.push(step);
                logical.push(LifeTypeProjection::Place(step));
                discover(ty, path, logical, life, classes, fuel, depth + 1, physical)?;
                path.pop();
                logical.pop();
            }
            Ok(())
        }
        Type::Array { element, length } => {
            for i in 0..*length {
                fuel.pay(1)?;
                let step = PlaceProjection::Field(i);
                path.push(step);
                logical.push(LifeTypeProjection::Place(step));
                discover(
                    element,
                    path,
                    logical,
                    life,
                    classes,
                    fuel,
                    depth + 1,
                    physical,
                )?;
                path.pop();
                logical.pop();
            }
            Ok(())
        }
        _ => closed(ty, fuel, depth + 1),
    }
}
fn closed(ty: &Type, fuel: &mut Fuel<'_>, depth: usize) -> Result<()> {
    fuel.node(depth)?;
    match ty {
        Type::Unit
        | Type::Bool
        | Type::I8
        | Type::I16
        | Type::I32
        | Type::I64
        | Type::Isize
        | Type::U8
        | Type::U16
        | Type::U32
        | Type::U64
        | Type::Usize
        | Type::F32
        | Type::F64
        | Type::ForeignHandle
        | Type::CWideString
        | Type::ByteBuffer
        | Type::SyncCell(_)
        | Type::TaskI32 => Ok(()),
        Type::Managed(inner)
        | Type::OwnedStore { element: inner }
        | Type::Mutex { payload: inner } => closed(inner, fuel, depth + 1),
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            for (_, field) in fields {
                closed(field, fuel, depth + 1)?;
            }
            Ok(())
        }
        Type::Tuple(fields) => {
            for field in fields {
                closed(field, fuel, depth + 1)?;
            }
            Ok(())
        }
        Type::Array { element, .. } => closed(element, fuel, depth + 1),
        Type::FunctionPointer { .. } | Type::ForeignCallback { .. } => Ok(()),
        Type::BorrowedFunctionPointer { life, .. } if life.fixed_binder_count == 0 => Ok(()),
        _ => Err(ProjectionError::UnknownScope),
    }
}

/// Construct an untrusted physical subvalue view and complete child->parent
/// relation. `prefix` never dereferences an address or owning manager. Empty
/// explicit class domains are preserved; unresolved scopes are errors.
pub fn project(
    parent: &Type,
    prefix: &[PlaceProjection],
    limits: &Limits,
    remaining: &mut usize,
) -> Result<Projection> {
    let mut fuel = Fuel {
        limits,
        remaining,
        nodes: 0,
    };
    fuel.ty(parent, 0)?;
    fuel.pay(prefix.len())?;
    project_paid(parent, prefix, &mut fuel)
}
fn project_paid(
    parent: &Type,
    prefix: &[PlaceProjection],
    fuel: &mut Fuel<'_>,
) -> Result<Projection> {
    fuel.node(prefix.len())?;
    let Type::BorrowedStorage { storage, life } = parent else {
        return Err(ProjectionError::UnknownScope);
    };
    let mut selected = storage.as_ref();
    for (index, step) in prefix.iter().enumerate() {
        if index > 0 && scope(selected).is_some() {
            let mapping_path = prefix[..index]
                .iter()
                .copied()
                .map(LifeTypeProjection::Place)
                .collect::<Vec<_>>();
            let mut projection = project_paid(selected, &prefix[index..], fuel)?;
            for class in &mut projection.parent_classes {
                *class = retained_class(life, &mapping_path, *class, fuel)?;
            }
            return Ok(projection);
        }
        fuel.pay(1)?;
        selected = physical_child(selected, *step).ok_or(ProjectionError::Invalid)?;
    }
    if prefix.is_empty() {
        return Ok(Projection {
            ty: parent.clone(),
            parent_classes: (0..life.binder_count).collect(),
        });
    }
    let logical = prefix
        .iter()
        .copied()
        .map(LifeTypeProjection::Place)
        .collect::<Vec<_>>();
    if let Some(count) = scope(selected) {
        let mut parent_classes = Vec::new();
        for class in 0..count {
            parent_classes.push(retained_class(life, &logical, class, fuel)?);
        }
        return Ok(Projection {
            ty: selected.clone(),
            parent_classes,
        });
    }
    let mut parent_classes = Vec::new();
    discover(
        selected,
        &mut prefix.to_vec(),
        &mut logical.clone(),
        life,
        &mut parent_classes,
        fuel,
        0,
        true,
    )?;
    if matches!(selected, Type::Address { .. } | Type::Desc(_)) {
        return Ok(Projection {
            ty: selected.clone(),
            parent_classes,
        });
    }
    let binder = |actual: u32| -> Result<u32> {
        parent_classes
            .iter()
            .position(|binder| *binder == actual)
            .and_then(|i| u32::try_from(i).ok())
            .ok_or(ProjectionError::Invalid)
    };
    let mut slots = Vec::new();
    for slot in &life.slots {
        fuel.pay(
            prefix
                .len()
                .saturating_add(slot.path.len())
                .saturating_add(parent_classes.len()),
        )?;
        if let Some(path) = slot.path.strip_prefix(prefix) {
            slots.push(StorageLifeSlot {
                path: path.to_vec(),
                binder: binder(slot.binder)?,
                mode: slot.mode,
            });
        }
    }
    let mut retained = Vec::new();
    for row in &life.retained {
        fuel.pay(
            logical
                .len()
                .saturating_add(row.path.len())
                .saturating_add(parent_classes.len()),
        )?;
        if let Some(path) = row.path.strip_prefix(logical.as_slice()) {
            retained.push(StorageRetainedBinding {
                path: path.to_vec(),
                class: row.class,
                binder: binder(row.binder)?,
            });
        }
    }
    let mut nominal_arguments = Vec::new();
    for row in &life.nominal_arguments {
        fuel.pay(
            logical
                .len()
                .saturating_add(row.path.len())
                .saturating_add(parent_classes.len()),
        )?;
        if let Some(path) = row.path.strip_prefix(logical.as_slice()) {
            nominal_arguments.push(NominalLifeArgument {
                path: path.to_vec(),
                declaration: row.declaration,
                argument: row.argument,
                binder: binder(row.binder)?,
            });
        }
    }
    let ty = Type::BorrowedStorage {
        storage: Box::new(selected.clone()),
        life: StorageLifeShape {
            binder_count: u32::try_from(parent_classes.len())
                .map_err(|_| ProjectionError::Limit)?,
            slots,
            retained,
            nominal_arguments,
        },
    };
    Ok(Projection { ty, parent_classes })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(index: u32) -> PlaceProjection {
        PlaceProjection::Field(index)
    }
    fn l(index: u32) -> LifeTypeProjection {
        LifeTypeProjection::Place(p(index))
    }
    fn declaration() -> LifeDeclarationId {
        LifeDeclarationId {
            module: zil_base::ModuleId(3),
            declaration: zil_base::NodeId(7),
            compiler: false,
        }
    }
    fn rich() -> Type {
        Type::BorrowedStorage {
            storage: Box::new(Type::Record {
                name: "View".into(),
                fields: vec![(
                    "value".into(),
                    Type::Address {
                        mutable: false,
                        inner: Box::new(Type::I32),
                    },
                )],
            }),
            life: StorageLifeShape {
                binder_count: 2,
                slots: vec![StorageLifeSlot {
                    path: vec![p(0)],
                    binder: 1,
                    mode: StoredBorrowMode::SharedAddress,
                }],
                retained: vec![],
                nominal_arguments: vec![
                    NominalLifeArgument {
                        path: vec![],
                        declaration: declaration(),
                        argument: 0,
                        binder: 0,
                    },
                    NominalLifeArgument {
                        path: vec![],
                        declaration: declaration(),
                        argument: 2,
                        binder: 1,
                    },
                ],
            },
        }
    }
    fn parent() -> Type {
        Type::BorrowedStorage {
            storage: Box::new(Type::Tuple(vec![
                Type::I32,
                Type::Tuple(vec![
                    Type::Address {
                        mutable: true,
                        inner: Box::new(Type::I32),
                    },
                    rich(),
                ]),
            ])),
            life: StorageLifeShape {
                binder_count: 3,
                slots: vec![
                    StorageLifeSlot {
                        path: vec![p(1), p(0)],
                        binder: 1,
                        mode: StoredBorrowMode::ExclusiveAddress,
                    },
                    StorageLifeSlot {
                        path: vec![p(1), p(1), p(0)],
                        binder: 0,
                        mode: StoredBorrowMode::SharedAddress,
                    },
                ],
                retained: vec![
                    StorageRetainedBinding {
                        path: vec![l(1), l(1)],
                        class: 0,
                        binder: 2,
                    },
                    StorageRetainedBinding {
                        path: vec![l(1), l(1)],
                        class: 1,
                        binder: 0,
                    },
                ],
                nominal_arguments: vec![
                    NominalLifeArgument {
                        path: vec![l(1), l(1)],
                        declaration: declaration(),
                        argument: 0,
                        binder: 2,
                    },
                    NominalLifeArgument {
                        path: vec![l(1), l(1)],
                        declaration: declaration(),
                        argument: 2,
                        binder: 0,
                    },
                ],
            },
        }
    }
    #[test]
    fn ordered_joint_scope_keeps_phantom_and_full_nominal_identity() {
        let mut remaining = 10000;
        let result = project(&parent(), &[p(1)], &Limits::default(), &mut remaining).unwrap();
        assert_eq!(result.parent_classes, vec![1, 2, 0]);
        let Type::BorrowedStorage { storage, life } = result.ty else {
            panic!()
        };
        assert_eq!(life.binder_count, 3);
        assert_eq!(
            life.slots
                .iter()
                .map(|slot| slot.binder)
                .collect::<Vec<_>>(),
            vec![0, 2]
        );
        assert_eq!(
            life.retained
                .iter()
                .map(|row| (row.path.clone(), row.class, row.binder))
                .collect::<Vec<_>>(),
            vec![(vec![l(1)], 0, 1), (vec![l(1)], 1, 2)]
        );
        assert_eq!(
            life.nominal_arguments[0],
            NominalLifeArgument {
                path: vec![l(1)],
                declaration: declaration(),
                argument: 0,
                binder: 1
            }
        );
        assert_eq!(life.nominal_arguments[1].argument, 2);
        assert!(matches!(storage.as_ref(),Type::Tuple(fields) if fields[1]==rich()));
    }
    #[test]
    fn missing_leafless_child_mapping_is_unknown() {
        let mut parent = parent();
        let Type::BorrowedStorage { life, .. } = &mut parent else {
            panic!()
        };
        life.retained.remove(0);
        assert_eq!(
            project(&parent, &[p(1)], &Limits::default(), &mut 10000),
            Err(ProjectionError::UnknownScope)
        );
    }
    #[test]
    fn nested_projection_rebases_each_actual_child_domain() {
        let result = project(
            &parent(),
            &[p(1), p(1), p(0)],
            &Limits::default(),
            &mut 10000,
        )
        .unwrap();
        assert_eq!(result.parent_classes, vec![0]);
        assert_eq!(
            result.ty,
            Type::Address {
                mutable: false,
                inner: Box::new(Type::I32)
            }
        );
        let result = project(&parent(), &[p(1), p(1)], &Limits::default(), &mut 10000).unwrap();
        assert_eq!(result.parent_classes, vec![2, 0]);
        assert_eq!(result.ty, rich());
    }
    #[test]
    fn exact_fresh_budget_and_physical_only_paths() {
        let limits = Limits::default();
        let mut remaining = 10000;
        let result = project(&parent(), &[p(1)], &limits, &mut remaining).unwrap();
        let cost = 10000 - remaining;
        assert_eq!(
            project(&parent(), &[p(1)], &limits, &mut cost.clone()).unwrap(),
            result
        );
        assert_eq!(
            project(&parent(), &[p(1)], &limits, &mut (cost - 1)),
            Err(ProjectionError::Limit)
        );
        assert_eq!(
            project(&parent(), &[p(1), p(0), p(0)], &limits, &mut 10000),
            Err(ProjectionError::Invalid)
        );
        assert_eq!(
            project(&parent(), &[p(99)], &limits, &mut 10000),
            Err(ProjectionError::Invalid)
        );
    }
    #[test]
    fn explicit_empty_scope_and_unknown_scope_are_distinct() {
        let parent = Type::BorrowedStorage {
            storage: Box::new(Type::Tuple(vec![
                Type::BorrowedStorage {
                    storage: Box::new(Type::Tuple(vec![])),
                    life: StorageLifeShape {
                        binder_count: 0,
                        slots: vec![],
                        retained: vec![],
                        nominal_arguments: vec![],
                    },
                },
                Type::Named("Unresolved".into()),
            ])),
            life: StorageLifeShape {
                binder_count: 0,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
        };
        assert!(
            matches!(project(&parent,&[p(0)],&Limits::default(),&mut 10000).unwrap().ty,Type::BorrowedStorage {life,..} if life.binder_count==0)
        );
        assert_eq!(
            project(&parent, &[p(1)], &Limits::default(), &mut 10000),
            Err(ProjectionError::UnknownScope)
        );
    }
}
