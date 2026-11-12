//! Complete external type domains. Opaque rows are not physical storage rows.
//! These structural proofs do not discharge static sources or actual CFG loans.
use super::*;

/// Static obligations remain pending until the ordinary constructor/write/return
/// kernel has checked them. A directory cardinality is never that proof.
#[derive(Clone, Copy, Debug)]
pub(super) struct Domain {
    pub count: u32,
    pub pending_sources: bool,
}

pub(super) struct Inventory<'a> {
    module: &'a TypedModule,
    active: Vec<Type>,
}
impl Inventory<'_> {
    pub(super) fn domain(&mut self, ty: &Type, b: &mut Budget) -> Check<Domain> {
        dynamic_metadata::ty(ty, b)?;
        b.spend(self.active.len().saturating_add(1))?;
        if self.active.len() >= b.limits.max_call_depth.min(128) {
            return Err(limit("dynamic actual inventory depth exceeded"));
        }
        if self.active.contains(ty) {
            return Err(bad(
                "dynamic actual inventory has an unproved recursive domain",
            ));
        }
        self.active.push(ty.clone());
        let result = self.node(ty, b);
        self.active.pop();
        result
    }
    fn node(&mut self, ty: &Type, b: &mut Budget) -> Check<Domain> {
        let closed = Domain {
            count: 0,
            pending_sources: false,
        };
        match ty {
            Type::BorrowedStorage { storage, life } => {
                if matches!(storage.as_ref(), Type::Tuple(_)) {
                    return super::compound::check(self, storage, life, b);
                }
                let limits = b.limits.clone();
                stored_flow::qualification(self.module, ty, &limits, b)?;
                Ok(Domain {
                    count: life.binder_count,
                    pending_sources: true,
                })
            }
            Type::Closure(id) => {
                b.spend(
                    self.module
                        .closures
                        .len()
                        .saturating_add(self.module.functions.len()),
                )?;
                let mut rows = self.module.closures.iter().filter(|row| row.id == *id);
                let d = rows
                    .next()
                    .ok_or_else(|| bad("opaque Closure definition is absent"))?;
                if rows.next().is_some() {
                    return Err(bad("opaque Closure definition identity is ambiguous"));
                }
                let entry = self
                    .module
                    .functions
                    .get(d.function.0 as usize)
                    .ok_or_else(|| bad("opaque Closure exact entry is absent"))?;
                if entry.id != d.function
                    || entry.environment != Some(*id)
                    || entry.parameters.len() != d.parameters.len()
                {
                    return Err(bad("opaque Closure actual entry identity differs"));
                }
                // A missing relation is Unknown even for an apparently empty
                // capture list. Invocation classes are not free Fixed classes.
                let relation = d
                    .storage_life
                    .as_ref()
                    .ok_or_else(|| bad("opaque Closure complete relation is Unknown"))?;
                signature_life::charge(relation, b)?;
                if entry.storage_life.as_ref() != Some(relation) {
                    return Err(bad("opaque Closure and actual entry relations differ"));
                }
                for (parameter, expected) in entry.parameters.iter().zip(&d.parameters) {
                    b.spend(1)?;
                    let actual = entry
                        .locals
                        .get(parameter.0 as usize)
                        .ok_or_else(|| bad("opaque Closure actual parameter is absent"))?;
                    dynamic_metadata::ty(&actual.ty, b)?;
                    dynamic_metadata::ty(expected, b)?;
                    if actual.ty != *expected {
                        return Err(bad("opaque Closure actual parameter type differs"));
                    }
                }
                dynamic_metadata::ty(&d.result, b)?;
                dynamic_metadata::ty(&entry.return_type, b)?;
                if d.result != entry.return_type {
                    return Err(bad("opaque Closure actual result type differs"));
                }
                // Independently checks every capture and complete P/R class
                // occurrence against the real definition, not a Self directory.
                if relation.fixed_binder_count == 0 && relation.invocation_binder_count == 0 {
                    if !relation.bindings.is_empty() || !relation.classes.is_empty() {
                        return Err(bad(
                            "zero Closure relation contains undeclared class occurrences",
                        ));
                    }
                    for child in d
                        .captures
                        .iter()
                        .chain(&d.parameters)
                        .chain(std::iter::once(&d.result))
                    {
                        self.zero(child, b)?;
                    }
                } else {
                    signature_life::closure_relation(
                        self.module,
                        &d.parameters,
                        &d.captures,
                        d.kind,
                        &d.result,
                        relation,
                        Some(entry.span),
                    )?;
                }
                for child in d
                    .captures
                    .iter()
                    .chain(&d.parameters)
                    .chain(std::iter::once(&d.result))
                {
                    self.children(child, b)?;
                }
                Ok(Domain {
                    count: relation.fixed_binder_count,
                    pending_sources: true,
                })
            }
            Type::Task { retained_life, .. } => {
                // The actual frame is indispensable: result-only Type equality
                // or an empty port list does not prove a worker/frame exists.
                let mut found = false;
                for f in &self.module.functions {
                    b.spend(f.blocks.len())?;
                    for instruction in f.blocks.iter().flat_map(|block| &block.instructions) {
                        b.spend(1)?;
                        if !matches!(
                            instruction.kind,
                            InstructionKind::TaskCreate { .. }
                                | InstructionKind::TaskCreateOwned { .. }
                        ) {
                            continue;
                        }
                        let actual = instruction
                            .result
                            .and_then(|v| f.values.get(v.0 as usize))
                            .ok_or_else(|| bad("Task frame constructor result is absent"))?;
                        dynamic_metadata::ty(actual, b)?;
                        let Type::ErrorUnion { result, .. } = actual else {
                            continue;
                        };
                        let Type::Managed(payload) = result.as_ref() else {
                            continue;
                        };
                        if payload.as_ref() != ty {
                            continue;
                        }
                        let inferred = tasks::instruction(self.module, f, instruction, b)?
                            .ok_or_else(|| bad("Task frame constructor has no actual type"))?;
                        dynamic_metadata::ty(&inferred, b)?;
                        if inferred != *actual {
                            return Err(bad(
                                "Task frame differs from its exact complete retained type",
                            ));
                        }
                        found = true;
                    }
                }
                if !found {
                    return Err(bad("opaque Task has no actual worker/frame inventory"));
                }
                Ok(Domain {
                    count: retained_life.binder_count,
                    pending_sources: true,
                })
            }
            Type::Dyn { interface } | Type::OwnedDyn { interface } => {
                b.spend(self.module.dyn_interfaces.len())?;
                let mut rows = self
                    .module
                    .dyn_interfaces
                    .iter()
                    .filter(|row| row.id == *interface);
                let d = rows
                    .next()
                    .ok_or_else(|| bad("opaque dyn interface is absent"))?;
                if rows.next().is_some() {
                    return Err(bad("opaque dyn interface identity is ambiguous"));
                }
                dynamic_metadata::interface(self.module, d, b)?;
                dynamic_context::validate(self.module, d, b)?;
                let scope = DynInstantiationScope {
                    fixed_binder_count: d.retained_life.binder_count,
                    invocation_binder_count: 0,
                };
                for app in std::iter::once(&d.application).chain(&d.inherited) {
                    self.actuals(&app.actuals, scope, b)?;
                }
                for associated in &d.associated_types {
                    let value = DynTypeValue {
                        ty: associated.ty.clone(),
                        life: associated.life.clone(),
                        classes: associated.classes.clone(),
                    };
                    self.value(&value, scope, b)?;
                }
                // Constant carriers must be truly closed scalar domains, not
                // silently omit the lifetime part of a qualified carrier.
                for associated in &d.associated_constants {
                    if self.domain(&associated.ty, b)?.count != 0
                        || associated.value.ty() != associated.ty
                    {
                        return Err(bad(
                            "opaque dyn associated constant carrier is not complete",
                        ));
                    }
                }
                Ok(Domain {
                    count: d.retained_life.binder_count,
                    pending_sources: true,
                })
            }
            Type::Managed(inner) => self.domain(inner, b),
            Type::Registration { environment } | Type::AsyncRegistration { environment } => {
                if !matches!(environment.as_ref(), Type::Closure(_)) {
                    return Err(bad("registration has no actual Closure environment"));
                }
                let domain = self.domain(environment, b)?;
                if domain.count != 0 {
                    return Err(bad("registration environment is not closed"));
                }
                Ok(domain)
            }
            Type::Tuple(fields) => {
                for child in fields {
                    self.zero(child, b)?;
                }
                Ok(closed)
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                for (_, child) in fields {
                    self.zero(child, b)?;
                }
                Ok(closed)
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    for child in fields {
                        self.zero(child, b)?;
                    }
                }
                Ok(closed)
            }
            Type::ErrorUnion { error, result } => {
                self.zero(error, b)?;
                self.zero(result, b)?;
                Ok(closed)
            }
            Type::Array { element, .. }
            | Type::Slice { element }
            | Type::OwnedStore { element }
            | Type::Mutex { payload: element } => {
                self.zero(element, b)?;
                Ok(closed)
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                for child in parameters {
                    self.zero(child, b)?;
                }
                self.zero(result, b)?;
                Ok(closed)
            }
            Type::BorrowedFunctionPointer {
                parameters,
                result,
                life,
            } => {
                signature_life::charge(life, b)?;
                signature_life::relation(parameters, result, life, None)?;
                Ok(Domain {
                    count: life.fixed_binder_count,
                    pending_sources: true,
                })
            }
            Type::Unit
            | Type::Bool
            | Type::ByteBuffer
            | Type::ForeignHandle
            | Type::CWideString
            | Type::SyncCell(_) => Ok(closed),
            scalar if scalar.is_integer() || scalar.is_float() => Ok(closed),
            _ => Err(bad(
                "dynamic actual type has no complete intrinsic domain consumer",
            )),
        }
    }
    fn zero(&mut self, ty: &Type, b: &mut Budget) -> Check<()> {
        if self.domain(ty, b)?.count != 0 {
            return Err(bad(
                "unqualified container omits its child class occurrences",
            ));
        }
        Ok(())
    }
    // Outer borrows are represented by their enclosing complete contract. This
    // traversal validates the nested semantic domains without counting them a
    // second time or inventing capture/parameter paths in opaque Self.
    fn children(&mut self, ty: &Type, b: &mut Budget) -> Check<()> {
        match ty {
            Type::Address { inner, .. } | Type::Desc(inner) => self.children(inner, b),
            Type::Tuple(fields) => {
                for child in fields {
                    self.children(child, b)?;
                }
                Ok(())
            }
            _ => self.domain(ty, b).map(|_| ()),
        }
    }
    fn actuals(
        &mut self,
        actuals: &[DynGenericActual],
        scope: DynInstantiationScope,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(actuals.len())?;
        for actual in actuals {
            match actual {
                DynGenericActual::QualifiedType(value) => {
                    self.value(value, scope, b)?;
                }
                DynGenericActual::Type(ty) => self.zero(ty, b)?,
                DynGenericActual::Life(class) => scopes::class(scope, *class, b)?,
                DynGenericActual::Const(_) => {}
            }
        }
        Ok(())
    }
    fn value(
        &mut self,
        value: &DynTypeValue,
        scope: DynInstantiationScope,
        b: &mut Budget,
    ) -> Check<Domain> {
        dynamic_metadata::recipe(value, b)?;
        if value.life.binder_count as usize > b.limits.max_nodes {
            return Err(limit("dynamic complete type domain exceeds selected bound"));
        }
        b.spend(value.life.binder_count as usize)?;
        if value.classes.len() != value.life.binder_count as usize {
            return Err(bad("dynamic complete type class coverage differs"));
        }
        for class in &value.classes {
            scopes::class(scope, *class, b)?;
        }
        if let Type::BorrowedStorage { life, .. } = &value.ty {
            if life != &value.life {
                return Err(bad("dynamic external and embedded complete shapes differ"));
            }
        } else if let Type::Managed(inner) = &value.ty {
            let child = self.domain(inner, b)?;
            return self.wrapper(value, child, LifeTypeProjection::Pointee, b);
        } else if matches!(value.ty, Type::Address { .. } | Type::Desc(_)) {
            return self.borrow(value, b);
        } else if !value.life.slots.is_empty()
            || !value.life.retained.is_empty()
            || !value.life.nominal_arguments.is_empty()
        {
            return Err(bad(
                "dynamic opaque/closed root invents physical or nominal rows",
            ));
        }
        let domain = self.domain(&value.ty, b)?;
        if domain.count != value.life.binder_count {
            return Err(bad(
                "dynamic type intrinsic domain differs from its complete inventory",
            ));
        }
        Ok(domain)
    }
    fn wrapper(
        &mut self,
        value: &DynTypeValue,
        child: Domain,
        path: LifeTypeProjection,
        b: &mut Budget,
    ) -> Check<Domain> {
        let life = &value.life;
        if !life.slots.is_empty()
            || !life.nominal_arguments.is_empty()
            || life.retained.len() != child.count as usize
        {
            return Err(bad(
                "dynamic wrapper omits its actual child class directory",
            ));
        }
        let mut seen = vec![false; life.binder_count as usize];
        for (class, row) in life.retained.iter().enumerate() {
            b.spend(row.path.len().saturating_add(1))?;
            if row.path != [path] || row.class as usize != class || row.binder >= life.binder_count
            {
                return Err(bad(
                    "dynamic wrapper class differs from its actual child domain",
                ));
            }
            seen[row.binder as usize] = true;
        }
        if seen.contains(&false) {
            return Err(bad("dynamic wrapper has an ungrounded intrinsic class"));
        }
        Ok(Domain {
            count: life.binder_count,
            pending_sources: child.pending_sources,
        })
    }
    fn borrow(&mut self, value: &DynTypeValue, b: &mut Budget) -> Check<Domain> {
        let (inner, address) = match &value.ty {
            Type::Address { mutable, inner } => (inner.as_ref(), Some(*mutable)),
            Type::Desc(inner) => (inner.as_ref(), None),
            _ => unreachable!(),
        };
        let life = &value.life;
        if life.slots.len() != 1
            || !life.slots[0].path.is_empty()
            || !life.nominal_arguments.is_empty()
        {
            return Err(bad(
                "dynamic borrow omits its outer occurrence or invents physical child rows",
            ));
        }
        let slot = &life.slots[0];
        let mode_ok = match address {
            Some(true) => slot.mode == StoredBorrowMode::ExclusiveAddress,
            Some(false) => slot.mode == StoredBorrowMode::SharedAddress,
            None => matches!(
                slot.mode,
                StoredBorrowMode::ReadOnlyDescription | StoredBorrowMode::WritableDescription
            ),
        };
        if !mode_ok || slot.binder >= life.binder_count {
            return Err(bad("dynamic outer borrow mode or class differs"));
        }
        let child = self.domain(inner, b)?;
        if life.retained.len() != child.count as usize {
            return Err(bad(
                "dynamic borrow omits a complete pointee class occurrence",
            ));
        }
        let mut seen = vec![false; life.binder_count as usize];
        seen[slot.binder as usize] = true;
        for (class, row) in life.retained.iter().enumerate() {
            b.spend(row.path.len().saturating_add(1))?;
            if row.path != [LifeTypeProjection::Pointee]
                || row.class as usize != class
                || row.binder >= life.binder_count
            {
                return Err(bad(
                    "dynamic borrow child map differs from its actual Pointee domain",
                ));
            }
            seen[row.binder as usize] = true;
        }
        if seen.contains(&false) {
            return Err(bad(
                "dynamic borrow directory contains an unbound intrinsic class",
            ));
        }
        Ok(Domain {
            count: life.binder_count,
            pending_sources: child.pending_sources,
        })
    }
}
pub(super) fn check(
    module: &TypedModule,
    value: &DynTypeValue,
    scope: DynInstantiationScope,
    b: &mut Budget,
) -> Check<Domain> {
    Inventory {
        module,
        active: vec![],
    }
    .value(value, scope, b)
}
pub(super) fn intrinsic(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<Domain> {
    Inventory {
        module,
        active: vec![],
    }
    .domain(ty, b)
}
pub(super) fn closed(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<()> {
    Inventory {
        module,
        active: vec![],
    }
    .zero(ty, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn value(ty: Type, count: u32) -> DynTypeValue {
        DynTypeValue {
            ty,
            life: StorageLifeShape {
                binder_count: count,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
            classes: (0..count).map(StorageBinder::Fixed).collect(),
        }
    }
    fn closure(count: u32) -> TypedModule {
        let (mut module, _) = crate::tests::baseline();
        let captures: Vec<_> = (0..count)
            .map(|_| Type::Address {
                mutable: false,
                inner: Box::new(Type::I32),
            })
            .collect();
        let relation = StorageLifeContract {
            fixed_binder_count: count,
            invocation_binder_count: 0,
            bindings: (0..count)
                .map(|i| StorageLifeBinding {
                    position: StoragePosition {
                        base: StorageBase::Capture(i),
                        path: vec![],
                    },
                    binder: StorageBinder::Fixed(i),
                })
                .collect(),
            classes: vec![],
        };
        module.functions[0].environment = Some(ClosureId(0));
        module.functions[0].storage_life = Some(relation.clone());
        module.closures.push(ClosureDefinition {
            id: ClosureId(0),
            function: FunctionId(0),
            captures,
            parameters: vec![],
            result: Type::I32,
            kind: CallableKind::Fn,
            storage_life: Some(relation),
            lending_captures: vec![],
        });
        module
    }
    fn run(module: &TypedModule, value: &DynTypeValue) -> Check<Domain> {
        let scope = DynInstantiationScope {
            fixed_binder_count: 4,
            invocation_binder_count: 0,
        };
        let limits = Limits::default();
        let raw = check(module, value, scope, &mut Budget::new(&limits));
        let (_, plan) = crate::tests::baseline();
        let bytes = zil_ir::wire::encode_module_candidate(module, &plan, &limits).unwrap();
        let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        let wire = check(&decoded, value, scope, &mut Budget::new(&limits));
        assert_eq!(
            raw.as_ref()
                .map(|d| d.count)
                .map_err(|e| (&e.code, &e.message)),
            wire.as_ref()
                .map(|d| d.count)
                .map_err(|e| (&e.code, &e.message))
        );
        raw
    }
    #[test]
    fn opaque_domains_keep_intrinsic_occurrences_under_a_many_to_one_image() {
        let module = closure(2);
        let mut actual = value(Type::Closure(ClosureId(0)), 2);
        actual.classes = vec![StorageBinder::Fixed(0); 2];
        let proof = run(&module, &actual).unwrap();
        assert_eq!(proof.count, 2);
        assert!(proof.pending_sources);
        let compressed = value(Type::Closure(ClosureId(0)), 1);
        assert!(run(&module, &compressed).is_err());
        actual.life.retained.push(StorageRetainedBinding {
            path: vec![],
            class: 0,
            binder: 0,
        });
        assert!(run(&module, &actual).is_err());
        assert!(run(&module, &value(Type::I32, 2)).is_err());
    }
    #[test]
    fn opaque_none_is_unknown_and_same_count_does_not_replace_actual_definition() {
        let mut module = closure(0);
        let actual = value(Type::Closure(ClosureId(0)), 0);
        assert!(run(&module, &actual).unwrap().pending_sources);
        module.closures[0].storage_life = None;
        module.functions[0].storage_life = None;
        assert!(run(&module, &actual).is_err());
        let mut module = closure(2);
        module.closures[0].function = FunctionId(9);
        assert!(run(&module, &value(Type::Closure(ClosureId(0)), 2)).is_err());
        let module = closure(2);
        let task = Type::Task {
            result: Box::new(Type::I32),
            retained_life: RetainedLifeShape {
                binder_count: 0,
                ports: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
        };
        assert!(
            run(&module, &value(task, 0))
                .unwrap_err()
                .message
                .contains("worker/frame")
        );
    }
    #[test]
    fn two_same_opaque_children_keep_two_complete_occurrence_directories() {
        let module = closure(2);
        let life = StorageLifeShape {
            binder_count: 4,
            slots: vec![],
            nominal_arguments: vec![],
            retained: (0..2)
                .flat_map(|field| {
                    (0..2).map(move |class| StorageRetainedBinding {
                        path: vec![LifeTypeProjection::Place(PlaceProjection::Field(field))],
                        class,
                        binder: field * 2 + class,
                    })
                })
                .collect(),
        };
        let storage = Type::Tuple(vec![Type::Closure(ClosureId(0)); 2]);
        let mut actual = DynTypeValue {
            ty: Type::BorrowedStorage {
                storage: Box::new(storage.clone()),
                life: life.clone(),
            },
            life,
            classes: vec![StorageBinder::Fixed(0); 4],
        };
        assert_eq!(run(&module, &actual).unwrap().count, 4);
        let mut omitted = actual.clone();
        omitted.life.retained.pop();
        if let Type::BorrowedStorage { life, .. } = &mut omitted.ty {
            *life = omitted.life.clone();
        }
        assert!(run(&module, &omitted).is_err());
        let mut repeated = actual.clone();
        repeated.life.retained[2].path = repeated.life.retained[0].path.clone();
        if let Type::BorrowedStorage { life, .. } = &mut repeated.ty {
            *life = repeated.life.clone();
        }
        assert!(run(&module, &repeated).is_err());
        // Actual parent equality may also merge its own classes, while both
        // fields still have the actual Closure's independent two-class domain.
        actual.life.binder_count = 2;
        for row in &mut actual.life.retained {
            row.binder = row.class;
        }
        actual.classes.truncate(2);
        if let Type::BorrowedStorage { life, .. } = &mut actual.ty {
            *life = actual.life.clone();
        }
        assert_eq!(run(&module, &actual).unwrap().count, 2);
        assert!(run(&module, &value(storage, 2)).is_err());
    }

    #[test]
    fn address_keeps_outer_borrow_and_the_whole_opaque_pointee_domain() {
        let module = closure(2);
        let mut actual = value(
            Type::Address {
                mutable: false,
                inner: Box::new(Type::Closure(ClosureId(0))),
            },
            3,
        );
        actual.life.slots.push(StorageLifeSlot {
            path: vec![],
            binder: 0,
            mode: StoredBorrowMode::SharedAddress,
        });
        for class in 0..2 {
            actual.life.retained.push(StorageRetainedBinding {
                path: vec![LifeTypeProjection::Pointee],
                class,
                binder: class + 1,
            });
        }
        assert_eq!(run(&module, &actual).unwrap().count, 3);
        let mut bad = actual.clone();
        bad.life.slots.clear();
        assert!(run(&module, &bad).is_err());
        let mut bad = actual.clone();
        bad.life.retained.pop();
        assert!(run(&module, &bad).is_err());
        let mut bad = actual.clone();
        bad.life.retained[1].class = 0;
        assert!(run(&module, &bad).is_err());
        let mut bad = actual.clone();
        bad.life.slots[0].mode = StoredBorrowMode::ExclusiveAddress;
        assert!(run(&module, &bad).is_err());
    }
}
