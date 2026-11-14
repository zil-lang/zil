//! Physical input occurrences from an already complete whole-type directory.
//! This projection creates no roots, loans, closedness or cleanup permission.
use super::*;

pub(crate) struct Port {
    pub port: RetainedBorrowPort,
    pub path: Vec<ContractProjection>,
}

pub(crate) trait Catalog {
    fn lookup_work(&self) -> usize;
    fn closure(&self, id: ClosureId) -> Option<&ClosureDefinition>;
    fn interface(&self, id: DynInterfaceId) -> Option<&DynInterface>;
}
impl Catalog for TypedModule {
    fn lookup_work(&self) -> usize {
        self.closures
            .len()
            .saturating_add(self.dyn_interfaces.len())
    }
    fn closure(&self, id: ClosureId) -> Option<&ClosureDefinition> {
        self.closures.iter().find(|row| row.id == id)
    }
    fn interface(&self, id: DynInterfaceId) -> Option<&DynInterface> {
        self.dyn_interfaces.iter().find(|row| row.id == id)
    }
}

fn bad(message: &str) -> Diagnostic {
    error("E-TASK-CONTEXT", Span::default(), message)
}
struct Project<'a, C> {
    catalog: &'a C,
    limits: &'a Limits,
    left: &'a mut usize,
}
impl<C: Catalog> Project<'_, C> {
    fn fee(&mut self, n: usize) -> R<()> {
        *self.left = self.left.checked_sub(n).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "Task input port projection work exceeded",
                None,
            )
        })?;
        Ok(())
    }
    fn depth(&mut self, depth: usize) -> R<()> {
        self.fee(1)?;
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "Task input port projection depth exceeded",
                None,
            ));
        }
        Ok(())
    }
    fn leaf<'a>(&mut self, mut ty: &'a Type, path: &[PlaceProjection]) -> R<&'a Type> {
        self.fee(path.len().saturating_add(1))?;
        if path.len() > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "Task physical input path depth exceeded",
                None,
            ));
        }
        for step in path {
            while let Type::BorrowedStorage { storage, .. } = ty {
                self.fee(1)?;
                ty = storage;
            }
            ty = match (step, ty) {
                (PlaceProjection::Field(i), Type::Tuple(fields)) => fields.get(*i as usize),
                (
                    PlaceProjection::Field(i),
                    Type::Record { fields, .. } | Type::Package { fields, .. },
                ) => fields.get(*i as usize).map(|(_, ty)| ty),
                (PlaceProjection::Field(i), Type::Array { element, length }) if i < length => {
                    Some(element.as_ref())
                }
                (PlaceProjection::Variant(part), Type::Enum { variants, .. }) => variants
                    .get(part.variant as usize)
                    .and_then(|(_, fields)| fields.get(part.index as usize)),
                (PlaceProjection::ErrorArm(error_arm), Type::ErrorUnion { error, result }) => {
                    Some(if *error_arm {
                        error.as_ref()
                    } else {
                        result.as_ref()
                    })
                }
                _ => None,
            }
            .ok_or_else(|| bad("Task input port leaves its true physical input shape"))?;
        }
        Ok(ty)
    }
    fn remap(
        &mut self,
        life: &StorageLifeShape,
        path: &[LifeTypeProjection],
        class: u32,
    ) -> R<u32> {
        self.fee(
            life.retained
                .len()
                .saturating_mul(path.len().saturating_add(1)),
        )?;
        let mut rows = life
            .retained
            .iter()
            .filter(|row| row.path == path && row.class == class);
        let row = rows
            .next()
            .ok_or_else(|| bad("Task opaque input port lacks its whole-child class map"))?;
        if rows.next().is_some() || row.binder >= life.binder_count {
            return Err(bad(
                "Task opaque input port has an ambiguous or out-of-domain child map",
            ));
        }
        Ok(row.binder)
    }
    fn push(
        &mut self,
        result: &mut Vec<Port>,
        port: RetainedBorrowPort,
        path: Vec<ContractProjection>,
    ) -> R<()> {
        if result.len() >= self.limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "Task input port occurrence limit exceeded",
                None,
            ));
        }
        self.fee(path.len().saturating_add(1))?;
        result.push(Port { port, path });
        Ok(())
    }
    fn whole(&mut self, ty: &Type, life: &StorageLifeShape, depth: usize) -> R<Vec<Port>> {
        self.depth(depth)?;
        let mut ports = Vec::new();
        for slot in &life.slots {
            if slot.binder >= life.binder_count {
                return Err(bad(
                    "Task physical input port leaves its whole intrinsic domain",
                ));
            }
            let leaf = self.leaf(ty, &slot.path)?;
            if !matches!(
                (leaf, slot.mode),
                (
                    Type::Address { mutable: false, .. },
                    StoredBorrowMode::SharedAddress
                ) | (
                    Type::Address { mutable: true, .. },
                    StoredBorrowMode::ExclusiveAddress
                ) | (
                    Type::Desc(_),
                    StoredBorrowMode::ReadOnlyDescription | StoredBorrowMode::WritableDescription
                )
            ) {
                return Err(bad(
                    "Task physical input port changes its true complete leaf or mode",
                ));
            }
            inference::charge_type_snapshot(leaf, Span::default(), self.limits, self.left, 1)?;
            self.push(
                &mut ports,
                RetainedBorrowPort {
                    leaf: leaf.clone(),
                    mode: slot.mode,
                    binder: slot.binder,
                },
                slot.path
                    .iter()
                    .copied()
                    .map(ContractProjection::Place)
                    .collect(),
            )?;
        }
        self.opaque(
            ty,
            life,
            &mut Vec::new(),
            &mut Vec::new(),
            &mut ports,
            depth + 1,
        )?;
        self.fee(
            ports
                .len()
                .saturating_mul(ports.len().saturating_add(1).ilog2() as usize + 1)
                .saturating_mul(self.limits.max_call_depth.saturating_add(1)),
        )?;
        ports.sort_by(|a, b| a.path.cmp(&b.path));
        // Canonical ports follow the actual type tree, including an opaque
        // directory at its declaration position. Metadata paths are not byte
        // GEPs and never open an address pointee or a Type argument.
        let maximum = ports.iter().map(|port| port.path.len()).max().unwrap_or(0);
        self.fee(
            ports
                .len()
                .saturating_mul(ports.len().saturating_add(1).ilog2() as usize + 1)
                .saturating_mul(maximum.saturating_add(1)),
        )?;
        for port in &ports {
            self.fee(port.path.len().saturating_add(1))?;
            if port
                .path
                .iter()
                .any(|step| matches!(step, ContractProjection::RetainedClass(_)))
            {
                return Err(bad(
                    "a class-only coordinate cannot become a physical Task port",
                ));
            }
        }
        ports.sort_by(|left, right| {
            fn key(step: &ContractProjection) -> (u8, u32, u32) {
                match step {
                    ContractProjection::Place(PlaceProjection::Field(index)) => (0, *index, 0),
                    ContractProjection::Place(PlaceProjection::Variant(part)) => {
                        (0, part.variant, part.index)
                    }
                    ContractProjection::Place(PlaceProjection::ErrorArm(is_error)) => {
                        (0, u32::from(!*is_error), 0)
                    }
                    ContractProjection::Capture(index) => (1, *index, 0),
                    ContractProjection::RetainedPort(index) => (2, *index, 0),
                    ContractProjection::RetainedClass(index) => (3, *index, 0),
                }
            }
            left.path.iter().map(key).cmp(right.path.iter().map(key))
        });
        for pair in ports.windows(2) {
            self.fee(
                pair[0]
                    .path
                    .len()
                    .saturating_add(pair[1].path.len())
                    .saturating_add(1),
            )?;
            if pair[0].path == pair[1].path {
                return Err(bad(
                    "one actual Task input occurrence has repeated physical ports",
                ));
            }
        }
        Ok(ports)
    }
    fn opaque(
        &mut self,
        ty: &Type,
        whole: &StorageLifeShape,
        physical: &mut Vec<ContractProjection>,
        semantic: &mut Vec<LifeTypeProjection>,
        ports: &mut Vec<Port>,
        depth: usize,
    ) -> R<()> {
        self.depth(depth)?;
        match ty {
            // A physical outer occurrence never authorizes a pointee walk.
            Type::Address { .. } | Type::Desc(_) => {}
            Type::BorrowedStorage { storage, .. } => {
                self.opaque(storage, whole, physical, semantic, ports, depth + 1)?
            }
            Type::Managed(inner) => {
                semantic.push(LifeTypeProjection::Pointee);
                self.opaque(inner, whole, physical, semantic, ports, depth + 1)?;
                semantic.pop();
            }
            Type::Tuple(fields) => {
                for (i, child) in fields.iter().enumerate() {
                    let step = PlaceProjection::Field(
                        u32::try_from(i).map_err(|_| bad("Task input tuple ordinal overflows"))?,
                    );
                    physical.push(ContractProjection::Place(step));
                    semantic.push(LifeTypeProjection::Place(step));
                    self.opaque(child, whole, physical, semantic, ports, depth + 1)?;
                    physical.pop();
                    semantic.pop();
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                for (i, (_, child)) in fields.iter().enumerate() {
                    let step = PlaceProjection::Field(
                        u32::try_from(i).map_err(|_| bad("Task input field ordinal overflows"))?,
                    );
                    physical.push(ContractProjection::Place(step));
                    semantic.push(LifeTypeProjection::Place(step));
                    self.opaque(child, whole, physical, semantic, ports, depth + 1)?;
                    physical.pop();
                    semantic.pop();
                }
            }
            Type::Array { element, length } => {
                if *length as usize > self.limits.max_nodes {
                    return Err(Diagnostic::limit(
                        Phase::Types,
                        "Task input array occurrence limit exceeded",
                        None,
                    ));
                }
                for i in 0..*length {
                    physical.push(ContractProjection::Place(PlaceProjection::Field(i)));
                    semantic.push(LifeTypeProjection::Element);
                    self.opaque(element, whole, physical, semantic, ports, depth + 1)?;
                    physical.pop();
                    semantic.pop();
                }
            }
            Type::Task { retained_life, .. } | Type::MutexGuard { retained_life, .. } => {
                self.retained(retained_life, whole, physical, semantic, ports)?;
            }
            Type::Dyn { interface } | Type::OwnedDyn { interface } => {
                self.fee(self.catalog.lookup_work())?;
                let shape = &self
                    .catalog
                    .interface(*interface)
                    .ok_or_else(|| bad("Task dynamic input has no actual interface directory"))?
                    .retained_life;
                self.retained(shape, whole, physical, semantic, ports)?;
            }
            Type::Closure(id) => {
                self.fee(self.catalog.lookup_work())?;
                let closure = self
                    .catalog
                    .closure(*id)
                    .ok_or_else(|| bad("Task closure input has no actual environment directory"))?;
                let header = closure.storage_life.as_ref().ok_or_else(|| {
                    unsupported(
                        Span::default(),
                        "Task closure input awaits its complete actual environment header",
                    )
                })?;
                for row in &header.bindings {
                    let StorageBase::Capture(capture) = row.position.base else {
                        continue;
                    };
                    let StorageBinder::Fixed(class) = row.binder else {
                        return Err(bad("Task capture port uses an invocation class"));
                    };
                    let mut path = Vec::new();
                    for step in &row.position.path {
                        let ContractProjection::Place(step) = step else {
                            return Err(unsupported(
                                Span::default(),
                                "Task capture port requires its exact opaque child occurrence",
                            ));
                        };
                        path.push(*step);
                    }
                    let capture_ty = closure
                        .captures
                        .get(capture as usize)
                        .ok_or_else(|| bad("Task capture port names an absent real capture"))?;
                    let leaf = self.leaf(capture_ty, &path)?;
                    let mode = match leaf {
                        Type::Address { mutable: false, .. } => StoredBorrowMode::SharedAddress,
                        Type::Address { mutable: true, .. } => StoredBorrowMode::ExclusiveAddress,
                        Type::Desc(_) => {
                            return Err(unsupported(
                                Span::default(),
                                "Task description capture port awaits its exact original occurrence mode",
                            ));
                        }
                        _ => {
                            return Err(bad(
                                "Task capture port is not a true physical borrow leaf",
                            ));
                        }
                    };
                    inference::charge_type_snapshot(
                        leaf,
                        Span::default(),
                        self.limits,
                        self.left,
                        1,
                    )?;
                    let binder = if semantic.is_empty() {
                        class
                    } else {
                        self.remap(whole, semantic, class)?
                    };
                    if binder >= whole.binder_count {
                        return Err(bad("Task capture port leaves its full environment domain"));
                    }
                    self.fee(
                        physical
                            .len()
                            .saturating_add(row.position.path.len())
                            .saturating_add(2),
                    )?;
                    let mut source = physical.clone();
                    source.push(ContractProjection::Capture(capture));
                    source.extend_from_slice(&row.position.path);
                    self.push(
                        ports,
                        RetainedBorrowPort {
                            leaf: leaf.clone(),
                            mode,
                            binder,
                        },
                        source,
                    )?;
                }
            }
            Type::Enum { variants, .. } => {
                for (variant, (_, fields)) in variants.iter().enumerate() {
                    for (index, child) in fields.iter().enumerate() {
                        let step = PlaceProjection::Variant(VariantProjection {
                            variant: u32::try_from(variant)
                                .map_err(|_| bad("Task variant ordinal overflows"))?,
                            index: u32::try_from(index)
                                .map_err(|_| bad("Task variant field ordinal overflows"))?,
                        });
                        physical.push(ContractProjection::Place(step));
                        semantic.push(LifeTypeProjection::Place(step));
                        self.opaque(child, whole, physical, semantic, ports, depth + 1)?;
                        physical.pop();
                        semantic.pop();
                    }
                }
            }
            Type::ErrorUnion { error, result } => {
                for (is_error, child) in [(true, error.as_ref()), (false, result.as_ref())] {
                    let step = PlaceProjection::ErrorArm(is_error);
                    physical.push(ContractProjection::Place(step));
                    semantic.push(LifeTypeProjection::Place(step));
                    self.opaque(child, whole, physical, semantic, ports, depth + 1)?;
                    physical.pop();
                    semantic.pop();
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn retained(
        &mut self,
        child: &RetainedLifeShape,
        whole: &StorageLifeShape,
        physical: &[ContractProjection],
        semantic: &[LifeTypeProjection],
        ports: &mut Vec<Port>,
    ) -> R<()> {
        self.fee(child.ports.len())?;
        for (ordinal, port) in child.ports.iter().enumerate() {
            if port.binder >= child.binder_count {
                return Err(bad(
                    "Task opaque input port exceeds its own complete domain",
                ));
            }
            let binder = if semantic.is_empty() {
                port.binder
            } else {
                self.remap(whole, semantic, port.binder)?
            };
            if binder >= whole.binder_count {
                return Err(bad(
                    "Task opaque input port exceeds its parent complete domain",
                ));
            }
            inference::charge_type_snapshot(
                &port.leaf,
                Span::default(),
                self.limits,
                self.left,
                1,
            )?;
            self.fee(physical.len().saturating_add(2))?;
            let mut source = physical.to_vec();
            source.push(ContractProjection::RetainedPort(
                u32::try_from(ordinal).map_err(|_| bad("Task opaque port ordinal overflows"))?,
            ));
            self.push(
                ports,
                RetainedBorrowPort {
                    binder,
                    ..port.clone()
                },
                source,
            )?;
        }
        Ok(())
    }
}

pub(crate) fn from_type(
    catalog: &impl Catalog,
    ty: &Type,
    life: &StorageLifeShape,
    limits: &Limits,
    left: &mut usize,
) -> R<Vec<Port>> {
    Project {
        catalog,
        limits,
        left,
    }
    .whole(ty, life, 0)
}
