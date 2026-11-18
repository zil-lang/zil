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
    active: Vec<&'a Type>,
}
// The recursion stack borrows immutable input nodes. It is not a domain cache:
// the same type in another sibling occurrence still gets a complete proof.
pub(crate) fn active_contains(active: &[&Type], ty: &Type, b: &mut Budget) -> Check<bool> {
    for previous in active {
        b.spend(1)?;
        if std::ptr::eq(*previous, ty) {
            return Ok(true);
        }
        if std::mem::discriminant(*previous) != std::mem::discriminant(ty) {
            continue;
        }
        let different_head = match (*previous, ty) {
            (Type::Record { name: a, fields: x }, Type::Record { name: z, fields: y })
            | (Type::Package { name: a, fields: x }, Type::Package { name: z, fields: y }) => {
                if x.len() != y.len() || a.len() != z.len() {
                    true
                } else {
                    b.spend(a.len().saturating_add(z.len()))?;
                    a != z
                }
            }
            (Type::Enum { name: a, variants: x }, Type::Enum { name: z, variants: y }) => {
                if x.len() != y.len() || a.len() != z.len() {
                    true
                } else {
                    b.spend(a.len().saturating_add(z.len()))?;
                    a != z
                }
            }
            (Type::Tuple(a), Type::Tuple(z)) => a.len() != z.len(),
            (Type::Closure(a), Type::Closure(z)) => a != z,
            (Type::Array { length: a, .. }, Type::Array { length: z, .. }) => a != z,
            (Type::Address { mutable: a, inner: x }, Type::Address { mutable: z, inner: y }) => {
                a != z || std::mem::discriminant(x.as_ref()) != std::mem::discriminant(y.as_ref())
            }
            (Type::Managed(x), Type::Managed(y)) | (Type::Desc(x), Type::Desc(y))
            | (Type::OwnedStore { element: x }, Type::OwnedStore { element: y })
            | (Type::Slice { element: x }, Type::Slice { element: y }) => {
                std::mem::discriminant(x.as_ref()) != std::mem::discriminant(y.as_ref())
            }
            _ => false,
        };
        if different_head {
            continue;
        }
        // Equal heads alone never establish a recursive domain. Prepay the
        // genuine complete structural comparison, including all class fields.
        dynamic_metadata::ty(previous, b)?;
        dynamic_metadata::ty(ty, b)?;
        if *previous == ty {
            return Ok(true);
        }
    }
    Ok(false)
}
impl<'a> Inventory<'a> {
    pub(super) fn raw_scope(&mut self, ty: &'a Type, b: &mut Budget) -> Check<(StorageLifeShape, Domain)> {
        if !matches!(ty, Type::Address { .. } | Type::Desc(_)) {
            return Err(bad("raw child scope selects another actual constructor"));
        }
        let life = crate::tasks::frame_shape::declared_life(self.module, ty, b)?;
        let domain = super::compound::external(self, ty, &life, b)?;
        Ok((life, domain))
    }
    pub(super) fn dyn_context(&self,ty:&Type,b:&mut Budget)->Check<Option<&'a RetainedLifeShape>>{
        let(Type::Dyn{interface}|Type::OwnedDyn{interface})=ty else{return Ok(None);};
        b.spend(self.module.dyn_interfaces.len())?;
        let mut rows=self.module.dyn_interfaces.iter().filter(|row|row.id==*interface);
        let row=rows.next().ok_or_else(||bad("opaque dyn logical directory has no actual interface"))?;
        if rows.next().is_some(){return Err(bad("opaque dyn logical directory has ambiguous interface identity"));}
        dynamic_metadata::retained_shape(&row.retained_life,b)?;
        Ok(Some(&row.retained_life))
    }
    pub(super) fn domain(&mut self, ty: &'a Type, b: &mut Budget) -> Check<Domain> {
        dynamic_metadata::bounds_type(ty, b)?;
        b.spend(1)?;
        if self.active.len() >= b.limits.max_call_depth.min(128) {
            return Err(limit("dynamic actual inventory depth exceeded"));
        }
        // A leaf returns without entering the recursion stack. No active
        // ancestor can be one of these leaves, and the depth cap still holds.
        if matches!(ty, Type::Unit | Type::Bool | Type::ByteBuffer | Type::ForeignHandle | Type::CWideString | Type::SyncCell(_))
            || ty.is_integer() || ty.is_float()
        {
            return Ok(Domain { count: 0, pending_sources: false });
        }
        b.spend(self.active.len())?;
        if active_contains(&self.active,ty,b)? {
            return Err(bad(
                "dynamic actual inventory has an unproved recursive domain",
            ));
        }
        self.active.push(ty);
        let result = self.node(ty, b);
        self.active.pop();
        result
    }
    fn node(&mut self, ty: &'a Type, b: &mut Budget) -> Check<Domain> {
        let mut closed = Domain {
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
                    let mut expected = Vec::new();
                    for (ordinal, child) in d.parameters.iter().enumerate() {
                        self.static_occurrences(
                            child,
                            StoragePosition {
                                base: StorageBase::Parameter(ordinal as u32),
                                path: vec![],
                            },
                            &mut expected,
                            0,
                            b,
                        )?;
                    }
                    for (ordinal, child) in d.captures.iter().enumerate() {
                        self.static_occurrences(
                            child,
                            StoragePosition {
                                base: StorageBase::Capture(ordinal as u32),
                                path: vec![],
                            },
                            &mut expected,
                            0,
                            b,
                        )?;
                    }
                    self.static_occurrences(
                        &d.result,
                        StoragePosition {
                            base: StorageBase::Result,
                            path: vec![],
                        },
                        &mut expected,
                        0,
                        b,
                    )?;
                    b.spend(
                        expected
                            .len()
                            .saturating_add(relation.static_bindings.len()),
                    )?;
                    if expected != relation.static_bindings {
                        return Err(bad(
                            "zero Closure relation omits or changes an actual static borrow occurrence",
                        ));
                    }
                } else {
                    signature_life::closure_relation_resource_scopes(
                        self.module,
                        &d.parameters,
                        &d.captures,
                        d.kind,
                        &d.result,
                        relation,
                        Some(entry.span),
                        b,
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
            Type::Task { .. } => {
                let count =
                    crate::semantic::task_domains::task_metadata_domain(self.module, ty, b)?;
                Ok(Domain {
                    count,
                    pending_sources: true,
                })
            }
            Type::MutexGuard{payload,retained_life}=>{
                crate::generic_mutex::guard_shape(payload,retained_life,b)?;
                self.zero(payload,b)?;
                Ok(Domain{count:retained_life.binder_count,pending_sources:true})
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
                let module = self.module;
                context_ports::check(
                    module,
                    d,
                    |inner, b| self.domain(inner, b).map(|domain| domain.count),
                    b,
                )?;
                let scope = DynInstantiationScope {
                    fixed_binder_count: d.retained_life.binder_count,
                    invocation_binder_count: 0,
                };
                for app in std::iter::once(&d.application).chain(&d.inherited) {
                    self.actuals(&app.actuals, scope, b)?;
                }
                for associated in &d.associated_types {
                    self.parts(&associated.ty,&associated.life,&associated.classes,scope,b)?;
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
            // A bare type query has no outside lifetime images. Rebuild the
            // same complete raw boundary used for qualified values: one outer
            // borrow class, the whole pointee N, and every deeper occurrence.
            // This proves metadata only; no source, loan or static grounding.
            Type::Address { .. } | Type::Desc(_) => self.raw_scope(ty, b).map(|(_, domain)| domain),
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
            Type::Array{element,length:1}if matches!(element.as_ref(),Type::Closure(_))=>self.domain(element,b),
            Type::Tuple(fields) => {
                for child in fields {
                    closed.pending_sources |= self.zero(child, b)?.pending_sources;
                }
                Ok(closed)
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                for (_, child) in fields {
                    closed.pending_sources |= self.zero(child, b)?.pending_sources;
                }
                Ok(closed)
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    for child in fields {
                        closed.pending_sources |= self.zero(child, b)?.pending_sources;
                    }
                }
                Ok(closed)
            }
            Type::ErrorUnion { error, result } => {
                closed.pending_sources |= self.zero(error, b)?.pending_sources;
                closed.pending_sources |= self.zero(result, b)?.pending_sources;
                Ok(closed)
            }
            Type::Array { element, .. }
            | Type::Slice { element }
            | Type::OwnedStore { element }
            | Type::Mutex { payload: element } => {
                closed.pending_sources |= self.zero(element, b)?.pending_sources;
                Ok(closed)
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                for child in parameters {
                    closed.pending_sources |= self.zero(child, b)?.pending_sources;
                }
                closed.pending_sources |= self.zero(result, b)?.pending_sources;
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
            _ => Err(bad(&format!("dynamic actual type has no complete intrinsic domain consumer: {ty:?}"))),
        }
    }
    fn static_occurrences(
        &mut self,
        ty: &'a Type,
        position: StoragePosition,
        expected: &mut Vec<StoragePosition>,
        depth: usize,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(position.path.len().saturating_add(1))?;
        if depth > b.limits.max_call_depth.min(128) || expected.len() > b.limits.max_nodes {
            return Err(limit(
                "zero Closure actual static directory exceeds selected bound",
            ));
        }
        let mut child = |this: &mut Self, ty: &'a Type, step: PlaceProjection, b: &mut Budget| {
            b.spend(position.path.len().saturating_add(1))?;
            let mut next = position.clone();
            next.path.push(ContractProjection::Place(step));
            this.static_occurrences(ty, next, expected, depth + 1, b)
        };
        match ty {
            Type::Address { mutable: true, .. } => {
                return Err(bad(
                    "constant static borrow cannot manufacture exclusive mutable storage",
                ));
            }
            Type::Address { inner, .. } | Type::Desc(inner) => {
                // Even a constant outer borrow cannot erase the complete inner
                // nominal/phantom domain. The actual origin stays pending.
                self.zero(inner, b)?;
                expected.push(position);
            }
            Type::Tuple(fields) => {
                for (i, ty) in fields.iter().enumerate() {
                    child(self, ty, PlaceProjection::Field(i as u32), b)?;
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                for (i, (_, ty)) in fields.iter().enumerate() {
                    child(self, ty, PlaceProjection::Field(i as u32), b)?;
                }
            }
            Type::Enum { variants, .. } => {
                for (variant, (_, fields)) in variants.iter().enumerate() {
                    for (index, ty) in fields.iter().enumerate() {
                        child(
                            self,
                            ty,
                            PlaceProjection::Variant(VariantProjection {
                                variant: variant as u32,
                                index: index as u32,
                            }),
                            b,
                        )?;
                    }
                }
            }
            Type::ErrorUnion { error, result } => {
                child(self, error, PlaceProjection::ErrorArm(true), b)?;
                child(self, result, PlaceProjection::ErrorArm(false), b)?;
            }
            Type::Array { element, length } => {
                if *length as usize > b.limits.max_nodes {
                    return Err(limit(
                        "static array occurrence directory exceeds selected bound",
                    ));
                }
                b.spend(*length as usize)?;
                for i in 0..*length {
                    child(self, element, PlaceProjection::Field(i), b)?;
                }
            }
            _ => {
                self.zero(ty, b)?;
            }
        }
        Ok(())
    }
    fn zero(&mut self, ty: &'a Type, b: &mut Budget) -> Check<Domain> {
        let domain = self.domain(ty, b)?;
        if domain.count != 0 {
            dynamic_metadata::ty(ty,b)?;
            for ancestor in &self.active{dynamic_metadata::ty(ancestor,b)?;}
            return Err(bad("unqualified container omits its child class occurrences")
                .with_note(format!("actual unqualified child {ty:?}; complete query ancestors {:?}",self.active)));
        }
        Ok(domain)
    }
    // Outer borrows are represented by their enclosing complete contract. This
    // traversal validates the nested semantic domains without counting them a
    // second time or inventing capture/parameter paths in opaque Self.
    fn children(&mut self, ty: &'a Type, b: &mut Budget) -> Check<()> {
        dynamic_metadata::bounds_type(ty,b)?;
        self.children_at(ty,b,0)
    }
    fn children_at(&mut self, ty: &'a Type, b: &mut Budget, depth:usize) -> Check<()> {
        b.spend(1)?;
        if depth>b.limits.max_call_depth.min(128){return Err(limit("complete child inventory depth exceeded"));}
        match ty {
            Type::Address { inner, .. } | Type::Desc(inner) | Type::Managed(inner) => self.children_at(inner,b,depth+1),
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                for child in fields {
                    self.children_at(child,b,depth+1)?;
                }
                Ok(())
            }
            Type::Record{fields,..}|Type::Package{fields,..}=>{
                b.spend(fields.len())?;for(_,child)in fields{self.children_at(child,b,depth+1)?;}Ok(())
            },
            Type::Enum{variants,..}=>{
                b.spend(variants.len())?;for(_,fields)in variants{b.spend(fields.len())?;for child in fields{self.children_at(child,b,depth+1)?;}}Ok(())
            },
            Type::ErrorUnion{error,result}=>{b.spend(2)?;self.children_at(error,b,depth+1)?;self.children_at(result,b,depth+1)},
            _ => self.domain(ty, b).map(|_| ()),
        }
    }
    fn actuals(
        &mut self,
        actuals: &'a [DynGenericActual],
        scope: DynInstantiationScope,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(actuals.len())?;
        for actual in actuals {
            match actual {
                DynGenericActual::QualifiedType(value) => {
                    self.value(value, scope, b)?;
                }
                DynGenericActual::Type(ty) => {
                    self.zero(ty, b)?;
                }
                DynGenericActual::Life(class) => scopes::class(scope, *class, b)?,
                DynGenericActual::Const(_) => {}
            }
        }
        Ok(())
    }
    fn value(
        &mut self,
        value: &'a DynTypeValue,
        scope: DynInstantiationScope,
        b: &mut Budget,
    ) -> Check<Domain> {
        self.parts(&value.ty,&value.life,&value.classes,scope,b)
    }
    fn parts(&mut self,ty:&'a Type,life:&StorageLifeShape,classes:&[StorageBinder],scope:DynInstantiationScope,b:&mut Budget)->Check<Domain>{
        dynamic_metadata::bounds_type_parts(ty,life,classes,b)?;
        if life.binder_count as usize > b.limits.max_nodes {
            return Err(limit("dynamic complete type domain exceeds selected bound"));
        }
        b.spend(life.binder_count as usize)?;
        if classes.len() != life.binder_count as usize {
            return Err(bad("dynamic complete type class coverage differs"));
        }
        for class in classes {
            scopes::class(scope, *class, b)?;
        }
        self.shape(ty, life, b)
    }
    fn shape(&mut self, ty: &'a Type, life: &StorageLifeShape, b: &mut Budget) -> Check<Domain> {
        if let Type::BorrowedStorage { life: embedded, .. } = ty {
            dynamic_metadata::life_shape(embedded,b)?;
            dynamic_metadata::life_shape(life,b)?;
            if embedded != life {
                return Err(bad("dynamic external and embedded complete shapes differ"));
            }
        } else if matches!(ty,Type::Array{element,length:1}if matches!(element.as_ref(),Type::Closure(_))){
            return super::compound::external(self,ty,life,b);
        } else if matches!(
            *ty,
            Type::Managed(_) | Type::Address { .. } | Type::Desc(_) | Type::Tuple(_)
                | Type::Record{..} | Type::Package{..} | Type::ErrorUnion{..}
        ) {
            // An explicit complete value directory distinguishes persistent
            // variable rows from pending constant physical occurrences. A bare
            // Type-only Tuple query remains strict in domain()/zero().
            return super::compound::external(self, ty, life, b);
        } else if let Some(context)=self.dyn_context(ty,b)?{
            // These are the exact interface-owned logical Context rows, not
            // physical fields of the two-pointer runtime erased carrier.
            if !life.slots.is_empty()||life.binder_count!=context.binder_count||life.retained!=context.retained||life.nominal_arguments!=context.nominal_arguments{return Err(bad("opaque dyn value changes its complete interface-owned logical Context directory"));}
        } else if !life.slots.is_empty()
            || !life.retained.is_empty()
            || !life.nominal_arguments.is_empty()
        {
            return Err(bad(
                "dynamic opaque/closed root invents physical or nominal rows",
            ));
        }
        let domain = self.domain(ty, b)?;
        if domain.count != life.binder_count {
            return Err(bad(
                "dynamic type intrinsic domain differs from its complete inventory",
            ));
        }
        Ok(domain)
    }
}
/// Shape only: no outside image, lifetime term, source or Static proof is
/// created here. Header consumers already carry separately checked terms.
pub(super) fn shape(module: &TypedModule, ty: &Type, life: &StorageLifeShape, b: &mut Budget) -> Check<Domain> {
    dynamic_metadata::bounds_type(ty, b)?;
    dynamic_metadata::life_shape(life, b)?;
    if life.binder_count as usize > b.limits.max_nodes { return Err(limit("header complete shape domain exceeded")); }
    Inventory { module, active: vec![] }.shape(ty, life, b)
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
pub(super) fn check_parts(module:&TypedModule,ty:&Type,life:&StorageLifeShape,classes:&[StorageBinder],scope:DynInstantiationScope,b:&mut Budget)->Check<Domain>{
    Inventory{module,active:vec![]}.parts(ty,life,classes,scope,b)
}
#[track_caller]
pub(super) fn intrinsic(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<Domain> {
    let caller=std::panic::Location::caller();
    let result=Inventory {module,active:vec![]}.domain(ty,b);
    query_consumer(result,caller,b)
}
/// A structural bound neither flattens child classes nor supplies outside
/// images. Independently check the complete opaque/qualified child domains;
/// no aggregate domain, zero claim, permission or Static proof is returned.
pub(super) fn structural_children(module:&TypedModule,ty:&Type,b:&mut Budget)->Check<()> {
    Inventory{module,active:vec![]}.children(ty,b)
}
/// Proves only a zero class domain, never ClosedEnvironment or static sources.
#[track_caller]
pub(super) fn closed(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<()> {
    let caller=std::panic::Location::caller();
    let result=Inventory {module,active:vec![]}.zero(ty,b).map(|_|());
    query_consumer(result,caller,b)
}

fn query_consumer<T>(result:Check<T>,caller:&std::panic::Location<'_>,b:&mut Budget)->Check<T>{
    match result{Ok(value)=>Ok(value),Err(error)=>{
        b.spend(caller.file().len().saturating_add(48))?;
        Err(error.with_note(format!("complete actual domain consumer {}:{}:{}",caller.file(),caller.line(),caller.column())))
    }}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn structural_inventory_preserves_nonzero_children_without_claiming_a_bare_parent_domain_raw_wire(){
        let mut m=closure(1);
        let aggregate=Type::ErrorUnion{error:Box::new(Type::Enum{name:"Allocation".into(),variants:vec![("Failure".into(),vec![])]}),result:Box::new(Type::Closure(ClosureId(0)))};
        m.functions[0].values=vec![aggregate.clone()];
        let limits=Limits::default();let bytes=zil_ir::wire::encode_module_candidate(&m,&CandidatePlan::empty_for(&m),&limits).unwrap();
        let(wire,_)=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap();
        for m in [&m,&wire]{
            assert!(intrinsic(m,&aggregate,&mut Budget::new(&limits)).is_err());
            structural_children(m,&aggregate,&mut Budget::new(&limits)).unwrap();
            super::super::structural_actual(m,&aggregate,DynStructuralBound::Value,&mut Budget::new(&limits)).unwrap();
            assert!(super::super::structural_actual(m,&aggregate,DynStructuralBound::Copy,&mut Budget::new(&limits)).is_err());
            assert_eq!(intrinsic(m,&Type::Closure(ClosureId(0)),&mut Budget::new(&limits)).unwrap().count,1);
            let mut missing=m.clone();missing.closures[0].storage_life=None;
            assert!(structural_children(&missing,&aggregate,&mut Budget::new(&limits)).is_err());
            let task=Type::Task{result:Box::new(Type::I32),retained_life:RetainedLifeShape{binder_count:2,ports:vec![],retained:vec![StorageRetainedBinding{path:vec![LifeTypeProjection::Parameter(0)],class:0,binder:0},StorageRetainedBinding{path:vec![LifeTypeProjection::Parameter(0)],class:1,binder:1}],nominal_arguments:vec![]}};
            let task_outcome=Type::ErrorUnion{error:Box::new(Type::I32),result:Box::new(Type::Managed(Box::new(task)))};
            assert!(intrinsic(m,&task_outcome,&mut Budget::new(&limits)).is_err());
            structural_children(m,&task_outcome,&mut Budget::new(&limits)).unwrap();
            let arm=LifeTypeProjection::Place(PlaceProjection::ErrorArm(false));
            let mut full=StorageLifeShape{binder_count:2,slots:vec![],retained:vec![],nominal_arguments:vec![]};
            for path in [vec![arm],vec![arm,LifeTypeProjection::Pointee]]{for class in 0..2{full.retained.push(StorageRetainedBinding{path:path.clone(),class,binder:class});}}
            shape(m,&task_outcome,&full,&mut Budget::new(&limits)).unwrap();
            let mut omitted=full.clone();omitted.retained.pop();assert!(shape(m,&task_outcome,&omitted,&mut Budget::new(&limits)).is_err());
            let mut swapped=full.clone();swapped.retained[2].binder=1;swapped.retained[3].binder=0;assert!(shape(m,&task_outcome,&swapped,&mut Budget::new(&limits)).is_err());
            let mut coalesced=full.clone();coalesced.retained[1].binder=0;assert!(shape(m,&task_outcome,&coalesced,&mut Budget::new(&limits)).is_err());
            let mut phantom=full.clone();phantom.binder_count=3;assert!(shape(m,&task_outcome,&phantom,&mut Budget::new(&limits)).is_err());
        }
    }
    #[test]
    fn borrowed_cycle_stack_keeps_full_identity_including_child_and_life_fields() {
        let nominal=|child|Type::Record{name:"same_nominal_head".into(),fields:vec![("value".into(),child)]};
        let life=|n|StorageLifeShape{binder_count:n,slots:vec![],retained:vec![],nominal_arguments:vec![]};
        let types=[nominal(Type::I32),nominal(Type::Bool),Type::BorrowedStorage{storage:Box::new(nominal(Type::I32)),life:life(0)},Type::BorrowedStorage{storage:Box::new(nominal(Type::I32)),life:life(1)},Type::Closure(ClosureId(0)),Type::Closure(ClosureId(1))];
        for original in &types {
            for actual in &types {
                let detached=actual.clone();
                let mut b=Budget::new(&Limits::default());
                assert_eq!(active_contains(&[original],&detached,&mut b).unwrap(),original==actual);
                assert_eq!(active_contains(&[original],actual,&mut b).unwrap(),original==actual);
            }
        }
        let (module,_)=crate::tests::baseline();
        let ty=Type::Record{name:"borrowed_inventory_no_owned_copy_".repeat(128),fields:vec![("huge_field_".repeat(128),Type::I32)]};
        let mut inventory=Budget::new(&Limits::default());
        let result=intrinsic(&module,&ty,&mut inventory).unwrap();
        assert_eq!(result.count,0);assert!(!result.pending_sources);
        let mut copy=Budget::new(&Limits::default());dynamic_metadata::ty(&ty,&mut copy).unwrap();
        assert!(inventory.remaining>copy.remaining,"a borrowed traversal does not clone the nominal payload into its recursion stack");
    }
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
            static_bindings: vec![],
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
    fn private_callback_array_preserves_every_element_class_even_with_equal_images(){
        let module=closure(2);
        let mut cell=value(Type::Array{element:Box::new(Type::Closure(ClosureId(0))),length:1},2);
        cell.classes=vec![StorageBinder::Fixed(0);2];
        cell.life.retained=(0..2).map(|class|StorageRetainedBinding{path:vec![LifeTypeProjection::Element],class,binder:class}).collect();
        assert_eq!(run(&module,&cell).unwrap().count,2);
        let mut missing=cell.clone();missing.life.retained.pop();assert!(run(&module,&missing).is_err());
        let mut merged=cell.clone();merged.life.retained[1].binder=0;assert!(run(&module,&merged).is_err());
        let mut foreign=cell.clone();foreign.life.retained[1].class=2;assert!(run(&module,&foreign).is_err());
        let mut multiple=cell.clone();if let Type::Array{length,..}=&mut multiple.ty{*length=2;}assert!(run(&module,&multiple).is_err());
        let limits=Limits::default();let bytes=zil_ir::wire::encode_module_candidate(&module,&CandidatePlan::empty_for(&module),&limits).unwrap();
        let(decoded,_)=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap();
        assert_eq!(run(&decoded,&cell).unwrap().count,2);
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
        let domain=run(&module,&value(task.clone(),0)).unwrap();
        assert_eq!(domain.count,0);assert!(domain.pending_sources,"a known shape cannot discharge an actual worker/result source");
        let Type::Task{mut retained_life,result}=task else{unreachable!()};
        retained_life.ports.push(RetainedBorrowPort{leaf:Type::Address{mutable:false,inner:Box::new(Type::I32)},mode:StoredBorrowMode::SharedAddress,binder:0});
        let malformed=Type::Task{result,retained_life};
        assert!(run(&module,&value(malformed,0)).is_err(),"a zero-domain Task cannot declare port binder zero");
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
    fn static_zero_closure_keeps_actual_borrow_positions_pending_without_erasing_classes() {
        let mut module = closure(0);
        module.closures[0].captures = vec![Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        }];
        let position = StoragePosition {
            base: StorageBase::Capture(0),
            path: vec![],
        };
        let mut relation = module.closures[0].storage_life.clone().unwrap();
        relation.static_bindings = vec![position.clone()];
        module.closures[0].storage_life = Some(relation.clone());
        module.functions[0].storage_life = Some(relation);
        let actual = value(Type::Closure(ClosureId(0)), 0);
        assert!(run(&module, &actual).unwrap().pending_sources);
        let aggregate = value(Type::Tuple(vec![Type::Closure(ClosureId(0))]), 0);
        assert!(run(&module, &aggregate).unwrap().pending_sources);
        let mut missing = module.clone();
        missing.closures[0]
            .storage_life
            .as_mut()
            .unwrap()
            .static_bindings
            .clear();
        missing.functions[0].storage_life = missing.closures[0].storage_life.clone();
        assert!(run(&missing, &actual).is_err());
        let mut wrong = module.clone();
        wrong.closures[0]
            .storage_life
            .as_mut()
            .unwrap()
            .static_bindings[0]
            .base = StorageBase::Parameter(0);
        wrong.functions[0].storage_life = wrong.closures[0].storage_life.clone();
        assert!(run(&wrong, &actual).is_err());
        let mut duplicated = module.clone();
        duplicated.closures[0]
            .storage_life
            .as_mut()
            .unwrap()
            .static_bindings
            .push(position);
        duplicated.functions[0].storage_life = duplicated.closures[0].storage_life.clone();
        assert!(run(&duplicated, &actual).is_err());
    }

    #[test]
    fn opaque_free_domain_excludes_invocation_and_retains_mixed_static_occurrences() {
        let mut module = closure(1);
        let address = Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        };
        module.closures[0].captures.insert(0, address.clone());
        let mut relation = module.closures[0].storage_life.clone().unwrap();
        relation.bindings[0].position.base = StorageBase::Capture(1);
        relation.static_bindings = vec![StoragePosition {
            base: StorageBase::Capture(0),
            path: vec![],
        }];
        relation.invocation_binder_count = 1;
        relation.bindings.insert(
            0,
            StorageLifeBinding {
                position: StoragePosition {
                    base: StorageBase::Parameter(0),
                    path: vec![],
                },
                binder: StorageBinder::Invocation(0),
            },
        );
        module.closures[0].parameters = vec![address.clone()];
        module.functions[0].parameters = vec![LocalId(0)];
        module.functions[0].locals[0].ty = address;
        module.closures[0].storage_life = Some(relation.clone());
        module.functions[0].storage_life = Some(relation);
        let actual = value(Type::Closure(ClosureId(0)), 1);
        assert!(run(&module, &actual).unwrap().pending_sources);
        assert!(run(&module, &value(Type::Closure(ClosureId(0)), 2)).is_err());
        let mut duplicated = module.clone();
        duplicated.closures[0]
            .storage_life
            .as_mut()
            .unwrap()
            .bindings
            .insert(
                1,
                StorageLifeBinding {
                    position: StoragePosition {
                        base: StorageBase::Capture(0),
                        path: vec![],
                    },
                    binder: StorageBinder::Fixed(0),
                },
            );
        duplicated.functions[0].storage_life = duplicated.closures[0].storage_life.clone();
        assert!(run(&duplicated, &actual).is_err());
    }

    #[test]
    fn ordinary_qualified_pointee_keeps_nominal_occurrences_and_managed_path_depth() {
        let (mut module, _) = crate::tests::baseline();
        let declaration = LifeDeclarationId {
            module: zil_base::ModuleId(0),
            declaration: zil_base::NodeId(80),
            compiler: false,
        };
        let tag_life = StorageLifeShape {
            binder_count: 1,
            slots: vec![],
            retained: vec![],
            nominal_arguments: vec![NominalLifeArgument {
                path: vec![],
                declaration,
                argument: 0,
                binder: 0,
            }],
        };
        let tag = Type::BorrowedStorage {
            storage: Box::new(Type::Record {
                name: "test.Tag".into(),
                fields: vec![("code".into(), Type::I32)],
            }),
            life: tag_life.clone(),
        };
        module.life_declarations.push(LifeNominalDeclaration {
            id: declaration,
            identity: LifeDeclarationIdentity {
                package: "test".into(),
                module_path: vec![],
                declaration_path: vec!["Tag".into()],
                generation_path: vec![],
            },
            kind: LifeNominalKind::Record,
            parameters: vec![LifeParameterKind::Lifetime],
        });
        module.life_instances.push(LifeNominalInstance {
            declaration,
            ty: tag.clone(),
            arguments: vec![LifeNominalActual::Lifetime(0)],
        });
        for managed in [false, true] {
            let inner = if managed {
                Type::Managed(Box::new(tag.clone()))
            } else {
                tag.clone()
            };
            let mut actual = value(
                Type::Address {
                    mutable: false,
                    inner: Box::new(inner),
                },
                2,
            );
            let path = vec![LifeTypeProjection::Pointee; if managed { 2 } else { 1 }];
            actual.life.slots = vec![StorageLifeSlot {
                path: vec![],
                binder: 0,
                mode: StoredBorrowMode::SharedAddress,
            }];
            actual.life.retained = vec![StorageRetainedBinding {
                path: path.clone(),
                class: 0,
                binder: 1,
            }];
            if managed {
                actual.life.retained.insert(0, StorageRetainedBinding {
                    path: vec![LifeTypeProjection::Pointee],
                    class: 0,
                    binder: 1,
                });
            }
            actual.life.nominal_arguments = vec![NominalLifeArgument {
                path: path.clone(),
                declaration,
                argument: 0,
                binder: 1,
            }];
            assert_eq!(run(&module, &actual).unwrap().count, 2);
            let mut missing = actual.clone();
            missing.life.nominal_arguments.clear();
            assert!(run(&module, &missing).is_err());
            let mut wrong = actual.clone();
            wrong.life.nominal_arguments[0].binder = 0;
            assert!(run(&module, &wrong).is_err());
            let mut collapsed = actual.clone();
            collapsed.life.binder_count = 1;
            collapsed.classes.truncate(1);
            for row in &mut collapsed.life.retained { row.binder = 0; }
            collapsed.life.nominal_arguments[0].binder = 0;
            assert_eq!(run(&module, &collapsed).unwrap().count, 1);
            if managed {
                let mut flattened = actual.clone();
                flattened.life.retained[0].path.pop();
                flattened.life.nominal_arguments[0].path.pop();
                assert!(run(&module, &flattened).is_err());
            }
        }
        let mut owner = value(Type::Managed(Box::new(tag)), 1);
        owner.life.retained = vec![StorageRetainedBinding {
            path: vec![LifeTypeProjection::Pointee],
            class: 0,
            binder: 0,
        }];
        owner.life.nominal_arguments = vec![NominalLifeArgument {
            path: vec![LifeTypeProjection::Pointee],
            declaration,
            argument: 0,
            binder: 0,
        }];
        assert_eq!(run(&module, &owner).unwrap().count, 1);
        owner.life.nominal_arguments.clear();
        assert!(run(&module, &owner).is_err());
    }

    #[test]
    fn qualified_option_pointee_keeps_its_complete_type_argument_occurrence_raw_wire() {
        let (mut module, _) = crate::tests::baseline();
        let limits = Limits::default();
        let element = Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        };
        let element_life = StorageLifeShape {
            binder_count: 1,
            slots: vec![StorageLifeSlot {
                path: vec![],
                binder: 0,
                mode: StoredBorrowMode::SharedAddress,
            }],
            retained: vec![],
            nominal_arguments: vec![],
        };
        let some_field = PlaceProjection::Variant(VariantProjection {
            variant: 1,
            index: 0,
        });
        let option = Type::BorrowedStorage {
            storage: Box::new(Type::Enum {
                name: "OptionItem".into(),
                variants: vec![
                    ("None".into(), vec![]),
                    ("Some".into(), vec![element.clone()]),
                ],
            }),
            life: StorageLifeShape {
                binder_count: 1,
                slots: vec![StorageLifeSlot {
                    path: vec![some_field],
                    ..element_life.slots[0].clone()
                }],
                retained: vec![StorageRetainedBinding {
                    path: vec![LifeTypeProjection::TypeArgument(0)],
                    class: 0,
                    binder: 0,
                }],
                nominal_arguments: vec![],
            },
        };
        module.life_declarations.push(LifeNominalDeclaration {
            id: zil_ir::builtin_life::OPTION,
            identity: LifeDeclarationIdentity {
                package: "@zil.builtin".into(),
                module_path: vec!["option".into()],
                declaration_path: vec!["enum".into(), "Option".into()],
                generation_path: vec![],
            },
            kind: LifeNominalKind::Enum,
            parameters: vec![LifeParameterKind::Type],
        });
        module.life_instances.push(LifeNominalInstance {
            declaration: zil_ir::builtin_life::OPTION,
            ty: option.clone(),
            arguments: vec![LifeNominalActual::QualifiedType {
                ty: element,
                life: element_life,
            }],
        });
        module.functions[0].values.push(Type::Address {
            mutable: true,
            inner: Box::new(option),
        });
        let bytes = zil_ir::wire::encode_module_candidate(
            &module,
            &CandidatePlan::empty_for(&module),
            &limits,
        )
        .unwrap();
        let (wire, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        for module in [&module, &wire] {
            let mut budget = Budget::new(&limits);
            let ty = module.functions[0].values.last().unwrap();
            let Type::Address { inner, .. } = ty else {
                unreachable!()
            };
            let Type::BorrowedStorage { life, .. } = inner.as_ref() else {
                unreachable!()
            };
            let child = terms::Value::anchored(
                &DynTypeValue {
                    ty: inner.as_ref().clone(),
                    life: life.clone(),
                    classes: vec![StorageBinder::Fixed(0)],
                },
                terms::Owner::Function(FunctionId(0)),
                terms::Root::Receiver,
                &mut budget,
            )
            .unwrap();
            let head = terms::LifeTerm {
                origin: terms::Origin {
                    owner: terms::Owner::Function(FunctionId(0)),
                    root: terms::Root::Lexical(StorageBinder::Fixed(1)),
                    class: 0,
                },
                image: StorageBinder::Fixed(1),
            };
            let actual = constructors::address(
                &child,
                head,
                StoredBorrowMode::ExclusiveAddress,
                &mut budget,
            )
            .unwrap();
            assert_eq!(&actual.ty, ty);
            assert_eq!(
                actual.life,
                crate::tasks::frame_shape::declared_life(module, ty, &mut budget).unwrap(),
            );
            assert_eq!(run(module, &actual.raw).unwrap().count, 2);
            assert_eq!(actual.life.slots.len(), 1);
            for attack in 0..4 {
                let mut forged = actual.raw.clone();
                match attack {
                    0 => {
                        forged.life.retained.pop();
                    }
                    1 => forged.life.retained.swap(0, 1),
                    2 => forged.life.retained[1].binder = 0,
                    _ => forged.life.slots.push(StorageLifeSlot {
                        path: vec![some_field],
                        binder: 1,
                        mode: StoredBorrowMode::SharedAddress,
                    }),
                }
                assert!(
                    run(module, &forged).is_err(),
                    "pointee directory attack {attack}"
                );
            }
        }
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

    #[test]
    fn borrowed_owner_preserves_wrapper_and_opaque_child_maps_without_image_equality() {
        let module = closure(2);
        let task = Type::Task {
            result: Box::new(Type::I32),
            retained_life: RetainedLifeShape {
                binder_count: 2,
                ports: vec![],
                retained: (0..2).map(|class| StorageRetainedBinding {
                    path: vec![LifeTypeProjection::Parameter(0)], class, binder: class,
                }).collect(),
                nominal_arguments: vec![],
            },
        };
        for inner in [Type::Closure(ClosureId(0)), task] {
            let ty = Type::Address { mutable: false, inner: Box::new(Type::Managed(Box::new(inner))) };
            let mut actual = value(ty, 3);
            actual.classes = vec![StorageBinder::Fixed(0); 3];
            actual.life.slots.push(StorageLifeSlot { path: vec![], mode: StoredBorrowMode::SharedAddress, binder: 0 });
            for depth in [1, 2] {
                for class in 0..2 {
                    actual.life.retained.push(StorageRetainedBinding {
                        path: vec![LifeTypeProjection::Pointee; depth], class, binder: class + 1,
                    });
                }
            }
            let mut budget = Budget::new(&Limits::default());
            assert_eq!(crate::tasks::frame_shape::declared_life(&module, &actual.ty, &mut budget).unwrap(), actual.life);
            assert_eq!(run(&module, &actual).unwrap().count, 3);
            let mut omitted = actual.clone(); omitted.life.retained.remove(0);
            assert!(run(&module, &omitted).is_err());
            let mut reordered = actual.clone();
            reordered.life.retained[2].binder = 2;
            reordered.life.retained[3].binder = 1;
            assert!(run(&module, &reordered).is_err(), "equal images cannot swap the child intrinsic indices");
            let mut coalesced = actual.clone(); coalesced.life.retained[3].binder = 1;
            assert!(run(&module, &coalesced).is_err());
            let mut head = actual.clone(); head.life.retained[0].binder = 0;
            assert!(run(&module, &head).is_err(), "the owner domain cannot become the outer reference head");
            let mut phantom = actual.clone(); phantom.life.binder_count += 1; phantom.classes.push(StorageBinder::Fixed(0));
            assert!(run(&module, &phantom).is_err());
        }
    }

    #[test]
    fn intrinsic_task_reference_queries_preserve_the_complete_borrowed_owner_domain() {
        let task=Type::Task{result:Box::new(Type::I32),retained_life:RetainedLifeShape{
            binder_count:2,
            ports:vec![
                RetainedBorrowPort{leaf:Type::Address{mutable:true,inner:Box::new(Type::I32)},mode:StoredBorrowMode::ExclusiveAddress,binder:0},
                RetainedBorrowPort{leaf:Type::Address{mutable:false,inner:Box::new(Type::I32)},mode:StoredBorrowMode::SharedAddress,binder:1},
            ],
            retained:(0..2).map(|class|StorageRetainedBinding{path:vec![LifeTypeProjection::Parameter(0)],class,binder:class}).collect(),
            nominal_arguments:vec![],
        }};
        for mode in 0..3 {
            for depth in 1..=3 {
                let (mut module,_)=crate::tests::baseline();
                let mut ty=Type::Managed(Box::new(task.clone()));
                for _ in 0..depth {
                    ty=match mode {
                        0=>Type::Address{mutable:false,inner:Box::new(ty)},
                        1=>Type::Address{mutable:true,inner:Box::new(ty)},
                        _=>Type::Desc(Box::new(ty)),
                    };
                }
                // Serialize the actual queried constructor, not only an
                // unrelated module, to cover the complete raw/Wire boundary.
                module.functions[0].return_type=ty;
                let limits=Limits::default();
                let bytes=zil_ir::wire::encode_module_candidate(&module,&CandidatePlan::empty_for(&module),&limits).unwrap();
                let (wire,_)=zil_ir::wire::decode_module_candidate(&bytes,&limits).unwrap();
                let mut measured=None;
                for actual in [&module,&wire] {
                    let ty=&actual.functions[0].return_type;
                    let mut b=Budget::new(&limits);
                    let domain=intrinsic(actual,ty,&mut b).unwrap();
                    assert_eq!(domain.count,2+depth);assert!(domain.pending_sources);
                    let cost=limits.max_analysis_steps-b.remaining;
                    if let Some(previous)=measured {assert_eq!(previous,cost);}else{measured=Some(cost);}
                    let life=crate::tasks::frame_shape::declared_life(actual,ty,&mut Budget::new(&limits)).unwrap();
                    let value=DynTypeValue{ty:ty.clone(),classes:vec![StorageBinder::Fixed(0);life.binder_count as usize],life};
                    assert_eq!(run(actual,&value).unwrap().count,2+depth);
                    let mut forged=value.clone();forged.life.retained.pop();assert!(run(actual,&forged).is_err());
                    let mut forged=value;forged.life.slots[0].mode=if mode==1{StoredBorrowMode::SharedAddress}else{StoredBorrowMode::ExclusiveAddress};assert!(run(actual,&forged).is_err());
                    let mut short=limits.clone();short.max_analysis_steps=cost-1;
                    assert_eq!(intrinsic(actual,ty,&mut Budget::new(&short)).unwrap_err().category,zil_base::ResultCategory::ResourceLimit);
                }
            }
        }
        let (module,_)=crate::tests::baseline();
        let mut malformed=task;
        let Type::Task{retained_life,..}=&mut malformed else{unreachable!()};
        retained_life.retained[1].binder=0;
        for ty in [Type::Address{mutable:true,inner:Box::new(Type::Managed(Box::new(malformed.clone())))},Type::Desc(Box::new(Type::Managed(Box::new(malformed))))] {
            assert!(intrinsic(&module,&ty,&mut Budget::new(&Limits::default())).is_err(),"an outer wrapper cannot hide a coalesced Task child domain");
        }
    }
    #[test]
    fn raw_reference_layers_keep_each_local_scope_and_never_flatten_inner_slots() {
        let (module, _) = crate::tests::baseline();
        for mutable in [false, true] {
            let mut ty = Type::Address { mutable, inner: Box::new(Type::I32) };
            ty = Type::Address { mutable: false, inner: Box::new(ty) };
            ty = Type::Address { mutable: false, inner: Box::new(ty) };
            let mut b = Budget::new(&Limits::default());
            let mut actual = value(ty, 3);
            actual.life = crate::tasks::frame_shape::declared_life(&module, &actual.ty, &mut b).unwrap();
            actual.classes = vec![StorageBinder::Fixed(0); 3];
            assert_eq!(run(&module, &actual).unwrap().count, 3);
            assert_eq!(actual.life.slots.len(), 1);
            let mut swap = actual.clone();
            swap.life.retained[0].binder = 2; swap.life.retained[1].binder = 1;
            assert!(run(&module, &swap).is_err(), "same images cannot change the innermost class occurrence");
            let mut omitted = actual.clone(); omitted.life.retained.pop();
            assert!(run(&module, &omitted).is_err());
            let mut coalesced = actual.clone(); coalesced.life.retained[1].binder = 1;
            assert!(run(&module, &coalesced).is_err());
            let mut flattened = actual.clone(); flattened.life.slots.push(StorageLifeSlot{path:vec![PlaceProjection::Field(0)],mode:StoredBorrowMode::SharedAddress,binder:2});
            assert!(run(&module, &flattened).is_err());
        }
    }
    #[test]
    fn explicit_tuple_directory_keeps_static_pending_and_complete_inner_classes_raw_wire() {
        let limits = Limits::default();
        let mut module = closure(1);
        let pointer = Type::Address { mutable: false, inner: Box::new(Type::Slice { element: Box::new(Type::U8) }) };
        module.functions[0].locals[0].ty = Type::Tuple(vec![pointer, Type::I32]);
        let bytes = zil_ir::wire::encode_module_candidate(&module, &CandidatePlan::empty_for(&module), &limits).unwrap();
        let (wire, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        for module in [&module, &wire] {
            let value = DynTypeValue { ty: module.functions[0].locals[0].ty.clone(), life: StorageLifeShape { binder_count: 0, slots: vec![], retained: vec![], nominal_arguments: vec![] }, classes: vec![] };
            let scope = DynInstantiationScope { fixed_binder_count: 1, invocation_binder_count: 0 };
            let checked = check(module, &value, scope, &mut Budget::new(&limits)).unwrap();
            assert_eq!(checked.count, 0);
            assert!(checked.pending_sources, "metadata supplies no Static literal proof");
            assert!(intrinsic(module, &value.ty, &mut Budget::new(&limits)).is_err(), "a bare Type-only query has no enclosing occurrence directory");
            for attack in 0..4 {
                let mut wrong = value.clone();
                let Type::Tuple(fields) = &mut wrong.ty else { unreachable!() };
                match attack {
                    0 => { let Type::Address { mutable, .. } = &mut fields[0] else { unreachable!() }; *mutable = true; },
                    1 => { wrong.life.binder_count = 1; wrong.classes.push(StorageBinder::Fixed(0)); },
                    2 => { let Type::Address { inner, .. } = &mut fields[0] else { unreachable!() }; *inner = Box::new(Type::Closure(ClosureId(0))); },
                    _ => wrong.life.retained.push(StorageRetainedBinding { path: vec![LifeTypeProjection::Place(PlaceProjection::Field(0))], class: 0, binder: 0 }),
                }
                assert!(check(module, &wrong, scope, &mut Budget::new(&limits)).is_err(), "attack {attack}");
            }
        }
    }

}
