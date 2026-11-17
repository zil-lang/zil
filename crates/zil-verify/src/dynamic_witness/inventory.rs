//! Actual Self leaf inventories. Logical captures/ports remain selectors in
//! their own namespaces; class metadata never becomes a physical GEP or loan.
use super::*;

fn zero() -> StorageLifeShape {
    StorageLifeShape {
        binder_count: 0,
        slots: vec![],
        retained: vec![],
        nominal_arguments: vec![],
    }
}
fn field<'a>(ty: &'a Type, path: &[PlaceProjection], b: &mut Budget) -> Check<&'a Type> {
    dynamic_ports::physical_path(ty, path, b)
}
fn selected(
    source: &StoragePosition,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<StoragePosition> {
    b.spend(
        source
            .path
            .len()
            .saturating_add(path.len())
            .saturating_add(1),
    )?;
    let mut result = source.clone();
    result
        .path
        .extend(path.iter().copied().map(ContractProjection::Place));
    Ok(result)
}
fn inner_classes(
    module: &TypedModule,
    parent: &terms::Value,
    inner: &Type,
    prefix: &[LifeTypeProjection],
    b: &mut Budget,
) -> Check<Vec<requests::LeafClass>> {
    let count = values::intrinsic(module, inner, b)?.count;
    b.spend(count as usize)?;
    let mut classes = Vec::with_capacity(count as usize);
    for class in 0..count {
        b.spend(parent.life.retained.len())?;
        let mut rows = parent
            .life
            .retained
            .iter()
            .filter(|r| r.path == prefix && r.class == class);
        let row = rows
            .next()
            .ok_or_else(|| bad("Self leaf loses a complete inner class occurrence"))?;
        if rows.next().is_some() {
            return Err(bad("Self leaf repeats an inner class occurrence"));
        }
        let term = parent.term(row.binder, b)?;
        b.spend(prefix.len())?;
        classes.push(requests::LeafClass {
            path: prefix.to_vec(),
            class,
            binder: term.image,
            origin: headers::symbol(term),
        });
    }
    Ok(classes)
}
struct Walk<'a> {
    module: &'a TypedModule,
    scope: DynInstantiationScope,
    leaves: Vec<requests::Leaf>,
    active: Vec<Type>,
}
impl Walk<'_> {
    fn value(
        &mut self,
        value: &terms::Value,
        source: StoragePosition,
        b: &mut Budget,
    ) -> Check<()> {
        values::check(self.module, &value.raw, self.scope, b)?;
        b.spend(self.active.len().saturating_add(1))?;
        if self.active.len() >= b.limits.max_call_depth.min(128) {
            return Err(limit("Self inventory semantic depth exceeded"));
        }
        if self.active.contains(&value.raw.ty) {
            return Err(bad("Self inventory has a recursive opaque definition"));
        }
        self.active.push(value.raw.ty.clone());
        let result = match &value.raw.ty {
            Type::Closure(id) => self.closure(value, *id, &source, b),
            Type::Task { .. } | Type::Dyn { .. } | Type::OwnedDyn { .. } => {
                self.ports(value, &source, b)
            }
            _ => self.ordinary(value, &source, b),
        };
        self.active.pop();
        result
    }
    fn ordinary(
        &mut self,
        value: &terms::Value,
        source: &StoragePosition,
        b: &mut Budget,
    ) -> Check<()> {
        self.boundaries(value, &value.raw.ty, source, &mut vec![], &mut vec![], b, 0)
    }
    fn boundaries(
        &mut self,
        parent: &terms::Value,
        ty: &Type,
        source: &StoragePosition,
        physical: &mut Vec<PlaceProjection>,
        semantic: &mut Vec<LifeTypeProjection>,
        b: &mut Budget,
        depth: usize,
    ) -> Check<()> {
        b.spend(
            physical
                .len()
                .saturating_add(semantic.len())
                .saturating_add(1),
        )?;
        if depth > b.limits.max_call_depth.min(128) {
            return Err(limit("Self inline inventory depth exceeded"));
        }
        match ty {
            Type::BorrowedStorage { storage, life } => {
                if physical.is_empty() && semantic.is_empty() {
                    self.boundaries(parent, storage, source, physical, semantic, b, depth + 1)?;
                } else {
                    let classes = inner_classes(self.module, parent, ty, semantic, b)?;
                    let terms = (0..life.binder_count)
                        .map(|class| {
                            let row = parent
                                .life
                                .retained
                                .iter()
                                .find(|row| row.path == *semantic && row.class == class)
                                .ok_or_else(|| {
                                    bad("Self qualified child class mapping is absent")
                                })?;
                            parent.term(row.binder, b)
                        })
                        .collect::<Check<Vec<_>>>()?;
                    dynamic_metadata::ty(ty, b)?;
                    let child = terms::Value {
                        raw: DynTypeValue {
                            ty: ty.clone(),
                            life: life.clone(),
                            classes: classes.iter().map(|class| class.binder).collect(),
                        },
                        terms,
                    };
                    self.value(&child, selected(source, physical, b)?, b)?;
                }
            }
            Type::Address { inner, .. } | Type::Desc(inner) => {
                b.spend(parent.life.slots.len())?;
                let mut rows = parent.life.slots.iter().filter(|row| row.path == *physical);
                let row = rows.next();
                if rows.next().is_some() {
                    return Err(bad("Self actual borrowed position is duplicated"));
                }
                let (outer,mode)=if let Some(row)=row {
                if !matches!(
                    (ty, row.mode),
                    (
                        Type::Address { mutable: false, .. },
                        StoredBorrowMode::SharedAddress
                    ) | (
                        Type::Address { mutable: true, .. },
                        StoredBorrowMode::ExclusiveAddress
                    ) | (
                        Type::Desc(_),
                        StoredBorrowMode::ReadOnlyDescription
                            | StoredBorrowMode::WritableDescription
                    )
                ) {
                    return Err(bad(
                        "Self actual leaf mode differs from its full typed source",
                    ));
                }
                let term = parent.term(row.binder, b)?;
                (requests::Borrow::Variable {binder:term.image,origin:headers::symbol(term)},row.mode)
                }else{
                    if !matches!(ty,Type::Address{mutable:false,..}){return Err(bad("Self actual borrowed position is absent"));}
                    // Complete shape validation classified this exact readonly
                    // occurrence as pending Static. This is no actual target
                    // or literal receipt; CFG/source discharge is unchanged.
                    (requests::Borrow::Static,StoredBorrowMode::SharedAddress)
                };
                let mut prefix = semantic.clone();
                prefix.push(LifeTypeProjection::Pointee);
                let mut classes = inner_classes(self.module, parent, inner, &prefix, b)?;
                for class in &mut classes {
                    class.path = vec![LifeTypeProjection::Pointee];
                }
                dynamic_metadata::ty(ty, b)?;
                self.leaves.push(requests::Leaf {
                    source: selected(source, physical, b)?,
                    kind: requests::LeafKind::Physical {
                        outer,
                        mode,
                    },
                    ty: ty.clone(),
                    classes,
                });
            }
            Type::Managed(inner) => {
                // Pointee is semantic metadata. Logical Task ports stay on
                // their actual carrier; no Field dereference is invented.
                let domain = values::intrinsic(self.module, inner, b)?;
                if domain.count != 0 && !matches!(inner.as_ref(), Type::Task { .. }) {
                    return Err(bad(
                        "owning Self payload needs its complete logical retained source inventory",
                    ));
                }
                let mut prefix = semantic.clone();
                prefix.push(LifeTypeProjection::Pointee);
                let mut classes = inner_classes(self.module, parent, inner, &prefix, b)?;
                let mut task_terms = Vec::with_capacity(classes.len());
                for class in &classes {
                    b.spend(parent.life.retained.len())?;
                    let row = parent
                        .life
                        .retained
                        .iter()
                        .find(|row| row.path == prefix && row.class == class.class)
                        .ok_or_else(|| bad("actual Managed payload class occurrence is absent"))?;
                    task_terms.push(parent.term(row.binder, b)?);
                }
                for class in &mut classes {
                    class.path = vec![LifeTypeProjection::Pointee];
                }
                dynamic_metadata::ty(ty, b)?;
                let position = selected(source, physical, b)?;
                self.leaves.push(requests::Leaf {
                    source: position.clone(),
                    kind: requests::LeafKind::OwningHead,
                    ty: ty.clone(),
                    classes,
                });
                if matches!(inner.as_ref(), Type::Task { .. }) {
                    dynamic_metadata::ty(inner, b)?;
                    let task = terms::Value {
                        raw: DynTypeValue {
                            ty: inner.as_ref().clone(),
                            life: StorageLifeShape {
                                binder_count: domain.count,
                                ..zero()
                            },
                            classes: task_terms.iter().map(|term| term.image).collect(),
                        },
                        terms: task_terms,
                    };
                    self.ports(&task, &position, b)?;
                }
            }
            Type::Closure(_) | Type::Task { .. } | Type::Dyn { .. } | Type::OwnedDyn { .. } => {
                let count = values::intrinsic(self.module, ty, b)?.count;
                let classes = inner_classes(self.module, parent, ty, semantic, b)?;
                if classes.len() != count as usize {
                    return Err(bad("Self opaque child class forest is incomplete"));
                }
                let life = zero();
                let value = terms::Value {
                    raw: DynTypeValue {
                        ty: ty.clone(),
                        life: StorageLifeShape {
                            binder_count: count,
                            ..life
                        },
                        classes: classes.iter().map(|c| c.binder).collect(),
                    },
                    terms: classes
                        .iter()
                        .map(|c| {
                            parent
                                .life
                                .retained
                                .iter()
                                .find(|r| r.path == *semantic && r.class == c.class)
                                .ok_or_else(|| bad("Self opaque child class mapping disappeared"))
                                .and_then(|r| parent.term(r.binder, b))
                        })
                        .collect::<Check<Vec<_>>>()?,
                };
                self.value(&value, selected(source, physical, b)?, b)?;
            }
            Type::Tuple(fields) => {
                for (index, child) in fields.iter().enumerate() {
                    self.child(
                        parent,
                        child,
                        source,
                        PlaceProjection::Field(index as u32),
                        physical,
                        semantic,
                        b,
                        depth,
                    )?;
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                for (index, (_, child)) in fields.iter().enumerate() {
                    self.child(
                        parent,
                        child,
                        source,
                        PlaceProjection::Field(index as u32),
                        physical,
                        semantic,
                        b,
                        depth,
                    )?;
                }
            }
            Type::Enum { variants, .. } => {
                for (variant, (_, fields)) in variants.iter().enumerate() {
                    for (index, child) in fields.iter().enumerate() {
                        self.child(
                            parent,
                            child,
                            source,
                            PlaceProjection::Variant(VariantProjection {
                                variant: variant as u32,
                                index: index as u32,
                            }),
                            physical,
                            semantic,
                            b,
                            depth,
                        )?;
                    }
                }
            }
            Type::ErrorUnion { error, result } => {
                self.child(
                    parent,
                    error,
                    source,
                    PlaceProjection::ErrorArm(true),
                    physical,
                    semantic,
                    b,
                    depth,
                )?;
                self.child(
                    parent,
                    result,
                    source,
                    PlaceProjection::ErrorArm(false),
                    physical,
                    semantic,
                    b,
                    depth,
                )?;
            }
            Type::Array { element, length } => {
                if *length as usize > b.limits.max_nodes {
                    return Err(limit("Self array inventory exceeds selected bound"));
                }
                b.spend(*length as usize)?;
                for index in 0..*length {
                    self.child(
                        parent,
                        element,
                        source,
                        PlaceProjection::Field(index),
                        physical,
                        semantic,
                        b,
                        depth,
                    )?;
                }
            }
            _ => {
                values::closed(self.module, ty, b)?;
            }
        }
        Ok(())
    }
    fn child(
        &mut self,
        parent: &terms::Value,
        ty: &Type,
        source: &StoragePosition,
        step: PlaceProjection,
        physical: &mut Vec<PlaceProjection>,
        semantic: &mut Vec<LifeTypeProjection>,
        b: &mut Budget,
        depth: usize,
    ) -> Check<()> {
        physical.push(step);
        semantic.push(LifeTypeProjection::Place(step));
        self.boundaries(parent, ty, source, physical, semantic, b, depth + 1)?;
        physical.pop();
        semantic.pop();
        Ok(())
    }
    fn closure(
        &mut self,
        value: &terms::Value,
        id: ClosureId,
        source: &StoragePosition,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(self.module.closures.len())?;
        let definition = self
            .module
            .closures
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| bad("Self closure definition is absent"))?;
        let header = definition
            .storage_life
            .as_ref()
            .ok_or_else(|| bad("Self closure capture scope is Unknown"))?;
        dynamic_metadata::ty(&value.raw.ty, b)?;
        self.leaves.push(requests::Leaf {
            source: source.clone(),
            kind: requests::LeafKind::OwningHead,
            ty: value.raw.ty.clone(),
            classes: vec![],
        });
        for (index, ty) in definition.captures.iter().enumerate() {
            let base = StorageBase::Capture(index as u32);
            b.spend(source.path.len().saturating_add(1))?;
            let mut position = source.clone();
            position
                .path
                .push(ContractProjection::Capture(index as u32));
            self.capture(
                value,
                ty,
                header,
                base,
                &mut vec![],
                &mut vec![],
                position,
                b,
                0,
            )?;
        }
        Ok(())
    }
    fn capture(
        &mut self,
        owner: &terms::Value,
        ty: &Type,
        header: &StorageLifeContract,
        base: StorageBase,
        physical: &mut Vec<PlaceProjection>,
        semantic: &mut Vec<LifeTypeProjection>,
        source: StoragePosition,
        b: &mut Budget,
        depth: usize,
    ) -> Check<()> {
        b.spend(
            physical
                .len()
                .saturating_add(semantic.len())
                .saturating_add(1),
        )?;
        if depth > b.limits.max_call_depth.min(128) {
            return Err(limit("Self capture inventory depth exceeded"));
        }
        match ty {
            Type::Address { inner, .. } | Type::Desc(inner) => {
                let mode = match ty {
                    Type::Address { mutable: true, .. } => StoredBorrowMode::ExclusiveAddress,
                    Type::Address { .. } => StoredBorrowMode::SharedAddress,
                    _ => StoredBorrowMode::ReadOnlyDescription,
                };
                let position = StoragePosition {
                    base,
                    path: physical
                        .iter()
                        .copied()
                        .map(ContractProjection::Place)
                        .collect(),
                };
                let outer = if header.static_bindings.contains(&position) {
                    requests::Borrow::Static
                } else {
                    b.spend(header.bindings.len())?;
                    let mut rows = header.bindings.iter().filter(|r| r.position == position);
                    let row = rows.next().ok_or_else(|| {
                        bad("Self capture borrow has no actual header occurrence")
                    })?;
                    if rows.next().is_some() {
                        return Err(bad("Self capture borrow header occurrence is duplicated"));
                    }
                    let StorageBinder::Fixed(class) = row.binder else {
                        return Err(bad("Self capture cannot retain a fresh invocation binder"));
                    };
                    let term = owner.term(class, b)?;
                    requests::Borrow::Variable {
                        binder: term.image,
                        origin: headers::symbol(term),
                    }
                };
                let mut path = semantic.clone();
                path.push(LifeTypeProjection::Pointee);
                let classes = self.capture_classes(owner, inner, header, base, &path, b)?;
                dynamic_metadata::ty(ty, b)?;
                self.leaves.push(requests::Leaf {
                    source,
                    kind: requests::LeafKind::Physical { outer, mode },
                    ty: ty.clone(),
                    classes,
                });
            }
            _ => {
                let (life, terms) = self.capture_scope(owner, ty, header, base, Some(physical), semantic, b)?;
                let child = terms::Value {
                    raw: DynTypeValue {
                        ty: ty.clone(),
                        life,
                        classes: terms.iter().map(|term| term.image).collect(),
                    },
                    terms,
                };
                self.value(&child, source, b)?;
            }
        }
        Ok(())
    }
    fn capture_classes(
        &self,
        owner: &terms::Value,
        ty: &Type,
        header: &StorageLifeContract,
        base: StorageBase,
        path: &[LifeTypeProjection],
        b: &mut Budget,
    ) -> Check<Vec<requests::LeafClass>> {
        let (_, terms) = self.capture_scope(owner, ty, header, base, None, path, b)?;
        Ok(terms.into_iter().enumerate().map(|(class, term)| requests::LeafClass {
            path: path.to_vec(), class: class as u32, binder: term.image, origin: headers::symbol(term),
        }).collect())
    }
    #[allow(clippy::too_many_arguments)]
    fn capture_scope(
        &self,
        owner: &terms::Value,
        ty: &Type,
        header: &StorageLifeContract,
        base: StorageBase,
        physical: Option<&[PlaceProjection]>,
        semantic: &[LifeTypeProjection],
        b: &mut Budget,
    ) -> Check<(StorageLifeShape, Vec<terms::LifeTerm>)> {
        let life = if let Some(physical)=physical {
            crate::tasks::frame_shape::header_life(self.module,ty,header,base,physical,b)?
        }else{crate::tasks::frame_shape::declared_life(self.module, ty, b)?};
        b.spend(life.binder_count as usize)?;
        let mut terms = vec![None; life.binder_count as usize];
        // An opaque root has actual root class rows. An ordinary wrapper is
        // reconstructed from its real slots and complete child occurrences;
        // it cannot demand or borrow another scope's same-numbered root row.
        let opaque = matches!(ty, Type::BorrowedStorage{..}|Type::Closure(_)|Type::Task{..}|Type::MutexGuard{..}|Type::Dyn{..}|Type::OwnedDyn{..});
        for class in 0..life.binder_count {
            b.spend(header.classes.len().saturating_add(semantic.len()))?;
            if opaque || header.classes.iter().any(|row| row.position.base==base&&row.position.path==semantic&&row.position.class==class) {
                let term = self.capture_header_term(owner,header,base,semantic,class,b)?;
                Self::capture_term(&mut terms,class,term,b)?;
            }
        }
        if let Some(physical) = physical {
            // Only the current boundary's physical slots belong to this
            // header. A nested closure's capture bindings belong to its own
            // header and are checked when value() enters that child.
            for slot in &life.slots {
                b.spend(physical.len().saturating_add(slot.path.len()).saturating_add(header.bindings.len()))?;
                let mut path=physical.iter().copied().map(ContractProjection::Place).collect::<Vec<_>>();
                path.extend(slot.path.iter().copied().map(ContractProjection::Place));
                let term=self.capture_binding_term(owner,header,base,&path,b)?;
                Self::capture_term(&mut terms,slot.binder,term,b)?;
            }
            let mut endpoint=ty;
            while let Type::Managed(inner)|Type::Address{inner,..}|Type::Desc(inner)=endpoint { b.spend(1)?;endpoint=inner; }
            if matches!(endpoint,Type::Task{..}|Type::MutexGuard{..}|Type::Dyn{..}|Type::OwnedDyn{..}) {
                let ports=crate::semantic::task_domains::raw_input_ports(self.module,ty,&life,b)?;
                for (_,_,class,source) in ports {
                    b.spend(physical.len().saturating_add(source.len()))?;
                    let mut path=physical.iter().copied().map(ContractProjection::Place).collect::<Vec<_>>();path.extend(source);
                    let term=self.capture_binding_term(owner,header,base,&path,b)?;
                    Self::capture_term(&mut terms,class,term,b)?;
                }
            }
        }
        for row in &life.retained {
            // Whole-child occurrences remain visible through ordinary
            // wrappers. Descendants below an opaque N-class boundary do not
            // become duplicate rows in its parent's header.
            let mut cursor=ty;let mut visible=true;
            for step in &row.path {
                b.spend(1)?;
                if matches!(cursor,Type::BorrowedStorage{..}|Type::Closure(_)|Type::Task{..}|Type::MutexGuard{..}|Type::Dyn{..}|Type::OwnedDyn{..}){visible=false;break;}
                match (cursor,step) {
                    (Type::Managed(inner)|Type::Address{inner,..}|Type::Desc(inner),LifeTypeProjection::Pointee)=>cursor=inner,
                    (Type::Tuple(fields),LifeTypeProjection::Place(PlaceProjection::Field(index)))=>cursor=fields.get(*index as usize).ok_or_else(||bad("capture whole-child field is absent"))?,
                    (Type::Record{fields,..}|Type::Package{fields,..},LifeTypeProjection::Place(PlaceProjection::Field(index)))=>cursor=&fields.get(*index as usize).ok_or_else(||bad("capture whole-child field is absent"))?.1,
                    (Type::ErrorUnion{error,result},LifeTypeProjection::Place(PlaceProjection::ErrorArm(arm)))=>cursor=if *arm{error}else{result},
                    (Type::Array{element,..},LifeTypeProjection::Element)=>cursor=element,
                    _=>break,
                }
            }
            if !visible {continue;}
            // A raw scope retains the complete Managed whole→pointee map.
            // The runtime header has only the real opaque pointee boundary;
            // it must not acquire another transparent-wrapper class row.
            if matches!(cursor,Type::Managed(_)){continue;}
            b.spend(semantic.len().saturating_add(row.path.len()).saturating_add(1))?;
            let mut path=semantic.to_vec();path.extend_from_slice(&row.path);
            let term=self.capture_header_term(owner,header,base,&path,row.class,b)?;
            Self::capture_term(&mut terms,row.binder,term,b)?;
        }
        let mut result=Vec::with_capacity(terms.len());
        for term in terms {
            let term=term.ok_or_else(||bad("Self capture omits a complete original class source"))?;
            b.spend(result.len().saturating_add(1))?;
            if result.iter().any(|other:&terms::LifeTerm|other.origin==term.origin){
                return Err(bad("Self capture coalesces independent complete child origins"));
            }
            result.push(term);
        }
        Ok((life,result))
    }
    fn capture_binding_term(&self,owner:&terms::Value,header:&StorageLifeContract,base:StorageBase,path:&[ContractProjection],b:&mut Budget)->Check<terms::LifeTerm>{
        b.spend(header.bindings.len().saturating_add(path.len()))?;
        let mut rows=header.bindings.iter().filter(|row|row.position.base==base&&row.position.path==path);
        let row=rows.next().ok_or_else(||bad("Self capture port has no exact complete header occurrence").with_note(format!("capture {base:?}, path {path:?}; header bindings {:?}",header.bindings)))?;
        if rows.next().is_some(){return Err(bad("Self capture port repeats its complete header occurrence"));}
        let StorageBinder::Fixed(index)=row.binder else{return Err(bad("Self capture port cannot retain a fresh invocation class"));};
        owner.term(index,b)
    }
    fn capture_header_term(&self,owner:&terms::Value,header:&StorageLifeContract,base:StorageBase,path:&[LifeTypeProjection],class:u32,b:&mut Budget)->Check<terms::LifeTerm>{
        b.spend(header.classes.len().saturating_add(path.len()))?;
        let mut rows=header.classes.iter().filter(|row|row.position.base==base&&row.position.path==path&&row.position.class==class);
        let row=rows.next().ok_or_else(||bad("Self capture lacks a complete child class occurrence").with_note(format!("capture {base:?}, complete child path {path:?}, intrinsic class {class}; header class rows {:?}",header.classes)))?;
        if rows.next().is_some(){return Err(bad("Self capture duplicates a complete child class occurrence"));}
        let StorageBinder::Fixed(index)=row.binder else{return Err(bad("Self capture class cannot be invocation-local"));};
        owner.term(index,b)
    }
    fn capture_term(terms:&mut [Option<terms::LifeTerm>],class:u32,term:terms::LifeTerm,b:&mut Budget)->Check<()>{
        b.spend(1)?;
        let slot=terms.get_mut(class as usize).ok_or_else(||bad("Self capture class escapes its complete local domain"))?;
        if slot.is_some_and(|other|other!=term){return Err(bad("Self capture occurrences disagree on an exact original class source").with_note(format!("local class {class}, previous {slot:?}, current {term:?}")));}
        *slot=Some(term);Ok(())
    }
    fn ports(
        &mut self,
        value: &terms::Value,
        source: &StoragePosition,
        b: &mut Budget,
    ) -> Check<()> {
        let complete_ports = match &value.raw.ty {
            Type::Dyn { interface } | Type::OwnedDyn { interface } => {
                let interface = dynamic_ports::interface(self.module, *interface, b)?;
                Some(context_ports::check(
                    self.module,
                    interface,
                    |ty, b| values::intrinsic(self.module, ty, b).map(|domain| domain.count),
                    b,
                )?)
            }
            _ => None,
        };
        let retained = match &value.raw.ty {
            Type::Task { retained_life, .. } => retained_life,
            Type::Dyn { interface } | Type::OwnedDyn { interface } => {
                &dynamic_ports::interface(self.module, *interface, b)?.retained_life
            }
            _ => unreachable!(),
        };
        // A naked Task is its sealed payload, not an owner. Its real head is
        // represented by Managed<Task>; only OwnedDyn owns at this root.
        if matches!(value.raw.ty, Type::OwnedDyn { .. }) {
            dynamic_metadata::ty(&value.raw.ty, b)?;
            self.leaves.push(requests::Leaf {
                source: source.clone(),
                kind: requests::LeafKind::OwningHead,
                ty: value.raw.ty.clone(),
                classes: vec![],
            });
        }
        for (port, row) in retained.ports.iter().enumerate() {
            dynamic_metadata::ty(&row.leaf, b)?;
            let inner = match &row.leaf {
                Type::Address { inner, .. } | Type::Desc(inner) => inner,
                _ => return Err(bad("opaque Self port is not an actual typed borrow")),
            };
            let count = values::intrinsic(self.module, inner, b)?.count;
            b.spend(count as usize)?;
            let mut classes = Vec::with_capacity(count as usize);
            if let Some(complete) = &complete_ports {
                let complete = complete
                    .get(port)
                    .ok_or_else(|| bad("actual opaque port inventory is incomplete"))?;
                for (class, binder) in &complete.inner {
                    let term = value.term(*binder, b)?;
                    classes.push(requests::LeafClass {
                        path: vec![LifeTypeProjection::Pointee],
                        class: *class,
                        binder: term.image,
                        origin: headers::symbol(term),
                    });
                }
            } else {
                // A Task port needs an actual per-port child row, not a
                // copied Context field or a coincidentally numbered class.
                let path = [
                    LifeTypeProjection::Port(port as u32),
                    LifeTypeProjection::Pointee,
                ];
                for class in 0..count {
                    b.spend(retained.retained.len())?;
                    let mut rows = retained
                        .retained
                        .iter()
                        .filter(|row| row.path == path && row.class == class);
                    let actual = rows.next().ok_or_else(|| {
                        bad("Task port lacks its complete actual inner occurrence map")
                    })?;
                    if rows.next().is_some() {
                        return Err(bad("Task port repeats an actual inner occurrence"));
                    }
                    let term = value.term(actual.binder, b)?;
                    classes.push(requests::LeafClass {
                        path: vec![LifeTypeProjection::Pointee],
                        class,
                        binder: term.image,
                        origin: headers::symbol(term),
                    });
                }
            }
            if classes.len() != count as usize {
                return Err(bad("opaque port loses part of its full inner domain"));
            }
            let term = value.term(row.binder, b)?;
            b.spend(source.path.len().saturating_add(1))?;
            let mut position = source.clone();
            position
                .path
                .push(ContractProjection::RetainedPort(port as u32));
            dynamic_metadata::recipe(&value.raw, b)?;
            self.leaves.push(requests::Leaf {
                source: position,
                kind: requests::LeafKind::RetainedPort {
                    outer: requests::Borrow::Variable {
                        binder: term.image,
                        origin: headers::symbol(term),
                    },
                    mode: row.mode,
                    carrier: value.raw.clone(),
                    port: port as u32,
                },
                ty: row.leaf.clone(),
                classes,
            });
        }
        Ok(())
    }
}
pub(super) fn self_leaves(
    module: &TypedModule,
    value: &terms::Value,
    scope: DynInstantiationScope,
    b: &mut Budget,
) -> Check<Vec<requests::Leaf>> {
    let mut walk = Walk {
        module,
        scope,
        leaves: vec![],
        active: vec![],
    };
    walk.value(
        value,
        StoragePosition {
            base: StorageBase::Result,
            path: vec![],
        },
        b,
    )?;
    Ok(walk.leaves)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn check_inventory(module: &TypedModule, value: &terms::Value) -> Check<Vec<requests::Leaf>> {
        let limits = Limits::default();
        let scope = DynInstantiationScope {
            fixed_binder_count: 4,
            invocation_binder_count: 0,
        };
        let raw = self_leaves(module, value, scope, &mut Budget::new(&limits));
        let bytes = zil_ir::wire::encode_module_candidate(
            module,
            &CandidatePlan::empty_for(module),
            &limits,
        )
        .unwrap();
        let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        let wire = self_leaves(&decoded, value, scope, &mut Budget::new(&limits));
        assert_eq!(
            raw.as_ref().map_err(|e| (&e.code, &e.message)),
            wire.as_ref().map_err(|e| (&e.code, &e.message))
        );
        raw
    }
    #[test]
    fn repeated_opaque_children_keep_actual_capture_selectors_and_all_intrinsic_origins() {
        let (module, closure) = super::super::callables::tests::fixture(2, 0);
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
        let limits = Limits::default();
        let value = terms::Value::anchored(
            &DynTypeValue {
                ty: Type::BorrowedStorage {
                    storage: Box::new(Type::Tuple(vec![closure.raw.ty.clone(); 2])),
                    life: life.clone(),
                },
                life,
                classes: vec![StorageBinder::Fixed(0); 4],
            },
            terms::Owner::Function(FunctionId(99)),
            terms::Root::Receiver,
            &mut Budget::new(&limits),
        )
        .unwrap();
        let leaves = check_inventory(&module, &value).unwrap();
        assert_eq!(leaves.len(), 6);
        let borrowed = leaves
            .iter()
            .filter_map(|leaf| match &leaf.kind {
                requests::LeafKind::Physical {
                    outer: requests::Borrow::Variable { origin, .. },
                    ..
                } => Some((leaf.source.clone(), origin.class)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(borrowed.len(), 4);
        for (index, (position, origin)) in borrowed.iter().enumerate() {
            assert_eq!(*origin, index as u32);
            assert_eq!(
                position.path,
                vec![
                    ContractProjection::Place(PlaceProjection::Field(index as u32 / 2)),
                    ContractProjection::Capture(index as u32 % 2)
                ]
            );
        }
        let mut omitted = value.clone();
        omitted.raw.life.retained.pop();
        if let Type::BorrowedStorage { life, .. } = &mut omitted.raw.ty {
            *life = omitted.raw.life.clone();
        }
        assert!(check_inventory(&module, &omitted).is_err());
    }
    #[test]
    fn static_capture_stays_pending_without_a_fake_fixed_class_and_owner_has_no_borrow_mode() {
        let (mut module, _) = super::super::callables::tests::fixture(1, 0);
        module.closures[0].result = Type::I32;
        module.functions[0].return_type = Type::I32;
        module.functions[0].values = vec![Type::I32];
        module.functions[0].blocks[0].instructions = vec![Instruction {
            result: Some(ValueId(0)),
            kind: InstructionKind::Constant(Constant::I32(7)),
            span: module.functions[0].span,
        }];
        let relation = StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: 0,
            bindings: vec![],
            static_bindings: vec![StoragePosition {
                base: StorageBase::Capture(0),
                path: vec![],
            }],
            classes: vec![],
        };
        module.closures[0].storage_life = Some(relation.clone());
        module.functions[0].storage_life = Some(relation);
        let limits = Limits::default();
        let value = terms::Value::anchored(
            &DynTypeValue {
                ty: Type::Closure(ClosureId(0)),
                life: zero(),
                classes: vec![],
            },
            terms::Owner::Function(FunctionId(99)),
            terms::Root::Receiver,
            &mut Budget::new(&limits),
        )
        .unwrap();
        let leaves = check_inventory(&module, &value).unwrap();
        assert_eq!(leaves.len(), 2);
        assert!(matches!(leaves[0].kind, requests::LeafKind::OwningHead));
        assert!(matches!(
            leaves[1].kind,
            requests::LeafKind::Physical {
                outer: requests::Borrow::Static,
                mode: StoredBorrowMode::SharedAddress
            }
        ));
        assert!(leaves[1].classes.is_empty());
        assert_eq!(leaves[1].source.path, vec![ContractProjection::Capture(0)]);
        module.closures[0].storage_life = None;
        module.functions[0].storage_life = None;
        assert!(check_inventory(&module, &value).is_err());
        let value = terms::Value::anchored(
            &DynTypeValue {
                ty: Type::Managed(Box::new(Type::ByteBuffer)),
                life: zero(),
                classes: vec![],
            },
            terms::Owner::Function(FunctionId(99)),
            terms::Root::Receiver,
            &mut Budget::new(&limits),
        )
        .unwrap();
        let leaves = check_inventory(&module, &value).unwrap();
        assert_eq!(leaves.len(), 1);
        assert!(matches!(leaves[0].kind, requests::LeafKind::OwningHead));
        assert_eq!(leaves[0].ty, value.raw.ty);
    }
    #[test]
    fn nested_capture_enters_the_child_header_without_repeating_its_private_rows() {
        let (mut module, _) = super::super::callables::tests::fixture(1, 0);
        module.closures[0].result=Type::I32;
        module.closures[0].storage_life.as_mut().unwrap().bindings.retain(|row|row.position.base!=StorageBase::Result);
        module.functions[0].return_type=Type::I32;
        module.functions[0].storage_life=module.closures[0].storage_life.clone();
        let parent_header=StorageLifeContract{fixed_binder_count:1,invocation_binder_count:0,
            bindings:vec![],static_bindings:vec![],classes:vec![SignatureLifeClassBinding{
                position:SignatureLifeClass{base:StorageBase::Capture(0),path:vec![],class:0},binder:StorageBinder::Fixed(0),
            }],
        };
        let mut parent=module.closures[0].clone();parent.id=ClosureId(1);parent.function=FunctionId(1);
        parent.captures=vec![Type::Closure(ClosureId(0))];parent.storage_life=Some(parent_header.clone());
        let mut body=module.functions[0].clone();body.id=FunctionId(1);body.environment=Some(ClosureId(1));body.storage_life=Some(parent_header);
        module.functions.push(body);module.closures.push(parent);
        let limits=Limits::default();
        let value=terms::Value::anchored(&DynTypeValue{ty:Type::Closure(ClosureId(1)),life:StorageLifeShape{binder_count:1,..zero()},classes:vec![StorageBinder::Fixed(0)]},terms::Owner::Function(FunctionId(99)),terms::Root::Receiver,&mut Budget::new(&limits)).unwrap();
        for wrapped in [false,true] {
            let mut actual=module.clone();
            if wrapped {
                actual.closures[1].captures[0]=Type::BorrowedStorage{storage:Box::new(Type::Tuple(vec![Type::Closure(ClosureId(0))])),life:StorageLifeShape{binder_count:1,slots:vec![],retained:vec![StorageRetainedBinding{path:vec![LifeTypeProjection::Place(PlaceProjection::Field(0))],class:0,binder:0}],nominal_arguments:vec![]}};
            }
            let leaves=check_inventory(&actual,&value).unwrap();
            let leaf=leaves.iter().find(|leaf|matches!(leaf.kind,requests::LeafKind::Physical{..})).unwrap();
            let mut path=vec![ContractProjection::Capture(0)];
            if wrapped {path.push(ContractProjection::Place(PlaceProjection::Field(0)));}
            path.push(ContractProjection::Capture(0));
            assert_eq!(leaf.source.path,path);
            // The parent's whole-N row does not supply the child's borrow.
            let mut forged=actual.clone();forged.closures[0].storage_life.as_mut().unwrap().bindings.clear();
            forged.functions[0].storage_life=forged.closures[0].storage_life.clone();
            assert!(check_inventory(&forged,&value).is_err());
            let mut forged=actual;forged.closures[1].storage_life.as_mut().unwrap().classes.clear();
            forged.functions[1].storage_life=forged.closures[1].storage_life.clone();
            assert!(check_inventory(&forged,&value).is_err());
        }
    }
    #[test]
    fn captured_task_owner_retains_every_port_origin_through_its_exact_child_scope() {
        let (mut module, _) = super::super::callables::tests::fixture(2, 0);
        let leaf=Type::Address{mutable:false,inner:Box::new(Type::I32)};
        let task=Type::Task{result:Box::new(Type::I32),retained_life:RetainedLifeShape{
            binder_count:2,ports:(0..2).map(|binder|RetainedBorrowPort{leaf:leaf.clone(),mode:StoredBorrowMode::SharedAddress,binder}).collect(),
            retained:(0..2).map(|class|StorageRetainedBinding{path:vec![LifeTypeProjection::Parameter(0)],class,binder:class}).collect(),nominal_arguments:vec![],
        }};
        let header=StorageLifeContract{fixed_binder_count:2,invocation_binder_count:0,
            bindings:(0..2).map(|port|StorageLifeBinding{position:StoragePosition{base:StorageBase::Capture(0),path:vec![ContractProjection::RetainedPort(port)]},binder:StorageBinder::Fixed(port)}).collect(),
            classes:(0..2).map(|class|SignatureLifeClassBinding{position:SignatureLifeClass{base:StorageBase::Capture(0),path:vec![LifeTypeProjection::Pointee],class},binder:StorageBinder::Fixed(class)}).collect(),static_bindings:vec![],
        };
        module.closures[0].captures=vec![Type::Managed(Box::new(task))];
        module.closures[0].result=Type::I32;module.closures[0].storage_life=Some(header.clone());
        module.functions[0].return_type=Type::I32;module.functions[0].storage_life=Some(header);
        let limits=Limits::default();
        let value=terms::Value::anchored(&DynTypeValue{ty:Type::Closure(ClosureId(0)),life:StorageLifeShape{binder_count:2,..zero()},classes:vec![StorageBinder::Fixed(0);2]},terms::Owner::Function(FunctionId(99)),terms::Root::Receiver,&mut Budget::new(&limits)).unwrap();
        // This is the metadata obligation only. A real MakeClosure, Task
        // worker, capture source, loan and cleanup still need their CFG proofs.
        let leaves=check_inventory(&module,&value).unwrap();
        assert_eq!(leaves.len(),4);
        assert_eq!(leaves[1].source.path,vec![ContractProjection::Capture(0)]);
        assert!(matches!(leaves[1].kind,requests::LeafKind::OwningHead));
        let ports=leaves.iter().filter_map(|leaf|match &leaf.kind{
            requests::LeafKind::RetainedPort{outer:requests::Borrow::Variable{origin,..},port,..}=>Some((*port,origin.class,leaf.source.path.clone())),_=>None,
        }).collect::<Vec<_>>();
        assert_eq!(ports,vec![(0,0,vec![ContractProjection::Capture(0),ContractProjection::RetainedPort(0)]),(1,1,vec![ContractProjection::Capture(0),ContractProjection::RetainedPort(1)])]);
        for attack in 0..4 {
            let mut forged=module.clone();let header=forged.closures[0].storage_life.as_mut().unwrap();
            match attack {
                0=>{header.classes.pop();},
                1=>{header.classes[1].binder=StorageBinder::Fixed(0);},
                2=>{header.bindings[1].binder=StorageBinder::Fixed(0);},
                3=>{header.classes[1].position.path.clear();},
                _=>unreachable!(),
            }
            forged.functions[0].storage_life=forged.closures[0].storage_life.clone();
            assert!(check_inventory(&forged,&value).is_err(),"capture attack {attack} cannot borrow another port's equal image");
        }
    }
}
