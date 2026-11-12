//! Bounded Source candidate construction, never an identity or permission proof.
//!
//! A kernel must independently resolve the actual parent, physical path, full
//! child scope and every class map. In particular these candidates provide no
//! root, initialization, tag, access-mode or parent-loan authority.
use crate::*;
use zil_base::Limits;

/// Complete actual module scope snapshots, not certificates. An unresolved
/// closure/interface is deliberately different from an explicit empty domain.
/// Kernels must recover these scopes from their own actual module inventory.
#[derive(Clone, Copy, Default)]
pub struct ScopeCandidates<'a> {
    pub closures: &'a [ClosureDefinition],
    pub interfaces: &'a [DynInterface],
}
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
    scopes: ScopeCandidates<'a>,
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
    fn contract(&mut self, life: &StorageLifeContract) -> Result<()> {
        self.domain(life.fixed_binder_count)?;
        self.domain(life.invocation_binder_count)?;
        for row in &life.bindings {
            self.pay(row.position.path.len().saturating_add(3))?;
        }
        for position in &life.static_bindings {
            self.pay(position.path.len().saturating_add(2))?;
        }
        for row in &life.classes {
            self.pay(row.position.path.len().saturating_add(4))?;
        }
        Ok(())
    }
    fn actuals(&mut self, actuals: &[DynGenericActual], depth: usize) -> Result<()> {
        for actual in actuals {
            self.pay(1)?;
            if let DynGenericActual::Type(ty) = actual {
                self.ty(ty, depth + 1)?;
            }
        }
        Ok(())
    }
    fn scopes(&mut self) -> Result<()> {
        if self.scopes.closures.len() > self.limits.max_nodes
            || self.scopes.interfaces.len() > self.limits.max_nodes
        {
            return Err(ProjectionError::Limit);
        }
        let mut closures = std::collections::BTreeSet::new();
        for definition in self.scopes.closures {
            self.pay(closures.len().saturating_add(1).ilog2() as usize + 5)?;
            if !closures.insert(definition.id) {
                return Err(ProjectionError::Invalid);
            }
            for ty in definition
                .captures
                .iter()
                .chain(&definition.parameters)
                .chain(std::iter::once(&definition.result))
            {
                self.ty(ty, 0)?;
            }
            if let Some(life) = &definition.storage_life {
                self.contract(life)?;
            }
            self.pay(definition.lending_captures.len().saturating_add(4))?;
        }
        let mut interfaces = std::collections::BTreeSet::new();
        for interface in self.scopes.interfaces {
            self.pay(interfaces.len().saturating_add(1).ilog2() as usize + 4)?;
            if !interfaces.insert(interface.id) {
                return Err(ProjectionError::Invalid);
            }
            self.retained(&interface.retained_life, 0)?;
            self.actuals(&interface.application.actuals, 0)?;
            for parent in &interface.inherited {
                self.pay(1)?;
                self.actuals(&parent.actuals, 0)?;
            }
            for member in &interface.associated_types {
                self.pay(1)?;
                self.actuals(&member.member.application.actuals, 0)?;
                self.ty(&member.ty, 0)?;
            }
            for member in &interface.associated_constants {
                self.pay(2)?;
                self.actuals(&member.member.application.actuals, 0)?;
                self.ty(&member.ty, 0)?;
            }
            for slot in &interface.slots {
                self.pay(1)?;
                self.actuals(&slot.member.application.actuals, 0)?;
                self.actuals(&slot.actuals, 0)?;
                for ty in slot.parameters.iter().chain(std::iter::once(&slot.result)) {
                    self.ty(ty, 0)?;
                }
                if let Some(life) = &slot.storage_life {
                    self.contract(life)?;
                }
            }
        }
        Ok(())
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
fn intrinsic_scope(ty: &Type) -> Option<u32> {
    match ty {
        Type::BorrowedStorage { life, .. } => Some(life.binder_count),
        Type::Task { retained_life, .. } | Type::MutexGuard { retained_life, .. } => {
            Some(retained_life.binder_count)
        }
        _ => None,
    }
}
fn scope(ty: &Type, fuel: &mut Fuel<'_>, depth: usize) -> Result<Option<u32>> {
    if let Some(count) = intrinsic_scope(ty) {
        return Ok(Some(count));
    }
    match ty {
        Type::Closure(id) => {
            fuel.node(depth)?;
            let definitions = fuel.scopes.closures;
            for definition in definitions {
                fuel.pay(1)?;
                if definition.id != *id {
                    continue;
                }
                if let Some(life) = &definition.storage_life {
                    return Ok(Some(life.fixed_binder_count));
                }
                // Legacy closure entries have no free class domain only when
                // every actual capture and signature type is closed. Direct
                // signature addresses are invocation-local, not captures.
                for capture in &definition.captures {
                    closed(capture, fuel, depth + 1)?;
                }
                for ty in definition
                    .parameters
                    .iter()
                    .chain(std::iter::once(&definition.result))
                {
                    signature_closed(ty, fuel, depth + 1)?;
                }
                return Ok(Some(0));
            }
            Err(ProjectionError::UnknownScope)
        }
        Type::Dyn { interface } | Type::OwnedDyn { interface } => {
            let interfaces = fuel.scopes.interfaces;
            for actual in interfaces {
                fuel.pay(1)?;
                if actual.id == *interface {
                    return Ok(Some(actual.retained_life.binder_count));
                }
            }
            Err(ProjectionError::UnknownScope)
        }
        Type::Registration { environment } | Type::AsyncRegistration { environment } => {
            // Registration requires its actual environment. Zero classes say
            // nothing about ClosedEnvironment, no-future, or synchronous Drop.
            if !matches!(environment.as_ref(), Type::Closure(_)) {
                return Err(ProjectionError::UnknownScope);
            }
            match scope(environment, fuel, depth + 1)? {
                Some(0) => Ok(Some(0)),
                Some(_) => Err(ProjectionError::Invalid),
                None => Err(ProjectionError::UnknownScope),
            }
        }
        _ => Ok(None),
    }
}
fn signature_closed(ty: &Type, fuel: &mut Fuel<'_>, depth: usize) -> Result<()> {
    fuel.node(depth)?;
    match ty {
        Type::Address { inner, .. } => closed(inner, fuel, depth + 1),
        Type::Tuple(fields) => {
            for field in fields {
                signature_closed(field, fuel, depth + 1)?;
            }
            Ok(())
        }
        _ => closed(ty, fuel, depth + 1),
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
    if let Some(count) = scope(ty, fuel, depth + 1)? {
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
    if let Some(count) = scope(ty, fuel, depth + 1)? {
        return if count == 0 {
            Ok(())
        } else {
            Err(ProjectionError::UnknownScope)
        };
    }
    match ty {
        number if number.is_integer() || number.is_float() => Ok(()),
        Type::Unit
        | Type::Bool
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
    project_with_scopes(
        parent,
        prefix,
        ScopeCandidates::default(),
        limits,
        remaining,
    )
}
/// Use only complete snapshots of actual module definitions. This optional
/// inventory supplies missing semantic domains, never roots or access rights.
pub fn project_with_scopes(
    parent: &Type,
    prefix: &[PlaceProjection],
    scopes: ScopeCandidates<'_>,
    limits: &Limits,
    remaining: &mut usize,
) -> Result<Projection> {
    let mut fuel = Fuel {
        limits,
        remaining,
        nodes: 0,
        scopes,
    };
    fuel.ty(parent, 0)?;
    fuel.pay(prefix.len())?;
    fuel.scopes()?;
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
        if index > 0 && intrinsic_scope(selected).is_some() {
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
    if let Some(count) = scope(selected, fuel, 0)? {
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
    fn closure(
        id: u32,
        captures: Vec<Type>,
        storage_life: Option<StorageLifeContract>,
    ) -> ClosureDefinition {
        ClosureDefinition {
            id: ClosureId(id),
            function: FunctionId(id),
            captures,
            parameters: vec![],
            result: Type::Unit,
            storage_life,
            kind: CallableKind::FnMut,
            lending_captures: vec![],
        }
    }
    fn registration_parent() -> Type {
        Type::BorrowedStorage {
            storage: Box::new(Type::Tuple(vec![Type::Tuple(vec![
                Type::Managed(Box::new(Type::AsyncRegistration {
                    environment: Box::new(Type::Closure(ClosureId(0))),
                })),
                Type::Address {
                    mutable: false,
                    inner: Box::new(Type::I32),
                },
            ])])),
            life: StorageLifeShape {
                binder_count: 1,
                slots: vec![StorageLifeSlot {
                    path: vec![p(0), p(1)],
                    binder: 0,
                    mode: StoredBorrowMode::SharedAddress,
                }],
                retained: vec![],
                nominal_arguments: vec![],
            },
        }
    }
    #[test]
    fn actual_closed_closure_snapshot_supplies_empty_registration_scope() {
        let parent = registration_parent();
        assert_eq!(
            project(&parent, &[p(0)], &Limits::default(), &mut 10000),
            Err(ProjectionError::UnknownScope)
        );
        let definitions = [closure(0, vec![Type::I32], None)];
        let scopes = ScopeCandidates {
            closures: &definitions,
            interfaces: &[],
        };
        let mut left = 10000;
        let projected =
            project_with_scopes(&parent, &[p(0)], scopes, &Limits::default(), &mut left).unwrap();
        assert_eq!(projected.parent_classes, vec![0]);
        assert!(
            matches!(projected.ty,Type::BorrowedStorage{life,..} if life.binder_count==1 && life.slots.len()==1)
        );
        let cost = 10000 - left;
        assert_eq!(
            project_with_scopes(
                &parent,
                &[p(0)],
                scopes,
                &Limits::default(),
                &mut (cost - 1)
            ),
            Err(ProjectionError::Limit)
        );
        assert!(
            project_with_scopes(
                &parent,
                &[p(0)],
                scopes,
                &Limits::default(),
                &mut cost.clone()
            )
            .is_ok()
        );
    }
    #[test]
    fn unscoped_borrowed_capture_and_duplicate_snapshots_cannot_become_empty() {
        let parent = registration_parent();
        let borrowed = [closure(
            0,
            vec![Type::Address {
                mutable: false,
                inner: Box::new(Type::I32),
            }],
            None,
        )];
        assert_eq!(
            project_with_scopes(
                &parent,
                &[p(0)],
                ScopeCandidates {
                    closures: &borrowed,
                    interfaces: &[]
                },
                &Limits::default(),
                &mut 10000
            ),
            Err(ProjectionError::UnknownScope)
        );
        let duplicates = [closure(0, vec![], None), closure(0, vec![], None)];
        assert_eq!(
            project_with_scopes(
                &parent,
                &[p(0)],
                ScopeCandidates {
                    closures: &duplicates,
                    interfaces: &[]
                },
                &Limits::default(),
                &mut 10000
            ),
            Err(ProjectionError::Invalid)
        );
        let cycle = [closure(0, vec![Type::Closure(ClosureId(0))], None)];
        assert!(matches!(
            project_with_scopes(
                &parent,
                &[p(0)],
                ScopeCandidates {
                    closures: &cycle,
                    interfaces: &[]
                },
                &Limits {
                    max_call_depth: 16,
                    ..Limits::default()
                },
                &mut 10000
            ),
            Err(ProjectionError::Limit)
        ));
    }
    #[test]
    fn complete_closure_domain_keeps_leafless_classes_and_never_becomes_registration_zero() {
        let definitions = [closure(
            0,
            vec![rich()],
            Some(StorageLifeContract {
                static_bindings: Vec::new(),
                fixed_binder_count: 2,
                invocation_binder_count: 0,
                bindings: vec![],
                classes: vec![
                    SignatureLifeClassBinding {
                        position: SignatureLifeClass {
                            base: StorageBase::Capture(0),
                            path: vec![],
                            class: 0,
                        },
                        binder: StorageBinder::Fixed(0),
                    },
                    SignatureLifeClassBinding {
                        position: SignatureLifeClass {
                            base: StorageBase::Capture(0),
                            path: vec![],
                            class: 1,
                        },
                        binder: StorageBinder::Fixed(1),
                    },
                ],
            }),
        )];
        let scopes = ScopeCandidates {
            closures: &definitions,
            interfaces: &[],
        };
        let parent = Type::BorrowedStorage {
            storage: Box::new(Type::Tuple(vec![Type::Tuple(vec![Type::Closure(
                ClosureId(0),
            )])])),
            life: StorageLifeShape {
                binder_count: 2,
                slots: vec![],
                retained: vec![
                    StorageRetainedBinding {
                        path: vec![l(0), l(0)],
                        class: 0,
                        binder: 1,
                    },
                    StorageRetainedBinding {
                        path: vec![l(0), l(0)],
                        class: 1,
                        binder: 0,
                    },
                ],
                nominal_arguments: vec![],
            },
        };
        let projected =
            project_with_scopes(&parent, &[p(0)], scopes, &Limits::default(), &mut 10000).unwrap();
        assert_eq!(projected.parent_classes, vec![1, 0]);
        let Type::BorrowedStorage { life, .. } = projected.ty else {
            panic!()
        };
        assert_eq!(life.binder_count, 2);
        assert_eq!(life.retained.len(), 2);
        assert_eq!(
            project_with_scopes(
                &registration_parent(),
                &[p(0)],
                scopes,
                &Limits::default(),
                &mut 10000
            ),
            Err(ProjectionError::Invalid)
        );
    }
}
