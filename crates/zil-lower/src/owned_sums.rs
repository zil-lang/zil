//! Ordinary Sum admission is distinct from capture/array/StoreElement profiles.
use super::*;
// Physical classification only, under the immutable Verify capability. The
// complete type/metadata copy walks are charged by `borrowed` below.
fn shared_sum(ty: &Type) -> bool {
    match ty {
        Type::BorrowedStorage { storage, life } => {
            matches!(storage.as_ref(), Type::Enum { .. })
                && !life.slots.is_empty()
                && life
                    .slots
                    .iter()
                    .all(|s| s.mode == zil_ir::StoredBorrowMode::SharedAddress)
        }
        Type::ErrorUnion { error, result } => {
            (shared_sum(error) || shared_sum(result))
                && (error.is_copy_value() || shared_sum(error))
                && (result.is_copy_value() || shared_sum(result))
        }
        _ => false,
    }
}
pub(super) struct Budget<'a> {
    limits: &'a zil_base::Limits,
    nodes: usize,
    work: usize,
}
impl<'a> Budget<'a> {
    pub(super) fn new(limits: &'a zil_base::Limits) -> Self {
        Self {
            limits,
            nodes: 0,
            work: 0,
        }
    }
    pub(super) fn static_bytes(&mut self, count: usize) -> CompileResult<()> {
        self.work = self
            .work
            .checked_add(count.saturating_add(1))
            .ok_or_else(limit)?;
        if self.work > self.limits.max_analysis_steps {
            return Err(limit());
        }
        Ok(())
    }
    fn pay(&mut self, ty: &Type, depth: usize) -> CompileResult<()> {
        self.nodes = self.nodes.checked_add(1).ok_or_else(limit)?;
        let names = match ty {
            Type::Package { name, fields } | Type::Record { name, fields } => name
                .len()
                .saturating_add(fields.iter().map(|(n, _)| n.len()).sum()),
            Type::Enum { name, variants } => name
                .len()
                .saturating_add(variants.iter().map(|(n, _)| n.len()).sum()),
            _ => 0,
        };
        self.work = self
            .work
            .checked_add(names.saturating_add(1))
            .ok_or_else(limit)?;
        if depth > self.limits.max_call_depth
            || self.nodes > self.limits.max_nodes
            || self.work > self.limits.max_analysis_steps
        {
            return Err(limit());
        }
        Ok(())
    }
    fn metadata(&mut self, count: usize) -> CompileResult<()> {
        self.nodes = self.nodes.checked_add(count).ok_or_else(limit)?;
        self.work = self.work.checked_add(count).ok_or_else(limit)?;
        if self.nodes > self.limits.max_nodes || self.work > self.limits.max_analysis_steps {
            return Err(limit());
        }
        Ok(())
    }
    fn stored_shape(&mut self, life: &zil_ir::StorageLifeShape) -> CompileResult<()> {
        self.metadata(
            life.slots
                .len()
                .saturating_add(life.retained.len())
                .saturating_add(life.nominal_arguments.len())
                .saturating_add(1),
        )?;
        for row in &life.slots {
            self.metadata(row.path.len().saturating_add(2))?;
        }
        for row in &life.retained {
            self.metadata(row.path.len().saturating_add(2))?;
        }
        for row in &life.nominal_arguments {
            self.metadata(row.path.len().saturating_add(4))?;
        }
        Ok(())
    }
    pub(super) fn stored_contract(
        &mut self,
        life: &zil_ir::StorageLifeContract,
    ) -> CompileResult<()> {
        let zil_ir::StorageLifeContract {
            static_bindings: _,
            fixed_binder_count: _,
            invocation_binder_count: _,
            bindings: _,
            classes: _,
        } = life;
        self.metadata(
            life.bindings
                .len()
                .saturating_add(life.classes.len())
                .saturating_add(life.static_bindings.len())
                .saturating_mul(16)
                .saturating_add(32),
        )?;
        for row in &life.bindings {
            self.metadata(row.position.path.len().saturating_mul(8))?;
        }
        for position in &life.static_bindings {
            self.metadata(position.path.len().saturating_mul(8))?;
        }
        for row in &life.classes {
            self.metadata(row.position.path.len().saturating_mul(8))?;
        }
        Ok(())
    }
    pub(super) fn dyn_tables(&mut self, module: &zil_ir::TypedModule) -> CompileResult<()> {
        if module.dyn_interfaces.is_empty() && module.dyn_vtables.is_empty() {
            return Ok(());
        }
        self.metadata(
            module
                .dyn_interfaces
                .len()
                .saturating_add(module.dyn_vtables.len()),
        )?;
        for interface in &module.dyn_interfaces {
            self.metadata(interface.slots.len())?;
            for slot in &interface.slots {
                self.metadata(slot.parameters.len().saturating_add(2))?;
                self.pay(&slot.result, 0)?;
                self.copy_work(&slot.result, 0)?;
                for parameter in &slot.parameters {
                    self.pay(parameter, 0)?;
                    self.copy_work(parameter, 0)?;
                }
            }
        }
        for table in &module.dyn_vtables {
            self.metadata(table.entries.len())?;
        }
        Ok(())
    }
    pub(super) fn callable_metadata(&mut self, module: &zil_ir::TypedModule) -> CompileResult<()> {
        self.metadata(
            module
                .functions
                .len()
                .saturating_add(module.closures.len())
                .saturating_add(module.resources.len()),
        )?;
        for function in &module.functions {
            self.metadata(1)?;
            if let Some(instance) = &function.dyn_instance {
                zil_ir::DynRecipeMetadata::visit_dyn_metadata(instance, self, 0)?;
            }
            if let Some(life) = &function.storage_life {
                self.stored_contract(life)?;
            }
            if let Some(contract) = &function.callable_contract {
                self.metadata(contract.roles.len())?;
                for role in &contract.roles {
                    self.metadata(
                        role.hold
                            .len()
                            .saturating_add(role.call.len())
                            .saturating_add(role.writes.len())
                            .saturating_add(8),
                    )?;
                    if let Some(life) = &role.life {
                        self.stored_contract(life)?;
                    }
                }
                match &contract.traces {
                    zil_ir::CallableTraces::Finite(traces) => {
                        self.metadata(traces.len().saturating_add(1))?;
                        for trace in traces {
                            self.metadata(trace.calls.len().saturating_mul(2).saturating_add(1))?;
                        }
                    }
                    zil_ir::CallableTraces::Flow(flow) => {
                        self.metadata(flow.nodes.len().saturating_add(1))?;
                        for node in &flow.nodes {
                            self.metadata(node.calls.len().saturating_mul(2).saturating_add(1))?;
                            if let zil_ir::CallableFlowNext::Edges(edges) = &node.next {
                                self.metadata(edges.len())?;
                            }
                        }
                    }
                }
            }
            self.pay(&function.return_type, 0)?;
            self.copy_work(&function.return_type, 0)?;
            for ty in function
                .locals
                .iter()
                .map(|l| &l.ty)
                .chain(&function.values)
            {
                self.pay(ty, 0)?;
                self.copy_work(ty, 0)?;
            }
        }
        for closure in &module.closures {
            if let Some(life) = &closure.storage_life {
                self.stored_contract(life)?;
            }
            self.pay(&closure.result, 0)?;
            self.copy_work(&closure.result, 0)?;
            for ty in closure.parameters.iter().chain(&closure.captures) {
                self.pay(ty, 0)?;
                self.copy_work(ty, 0)?;
            }
        }
        for resource in &module.resources {
            self.pay(&resource.payload, 0)?;
            self.copy_work(&resource.payload, 0)?;
        }
        Ok(())
    }
    /// The caller owns an immutable VerifiedModule. This only pays the actual
    /// representation/metadata walks and recognizes a transparent layout; it
    /// neither derives nor grants stored target permissions.
    pub(super) fn borrowed(&mut self, ty: &Type, depth: usize) -> CompileResult<bool> {
        match ty {
            Type::ErrorUnion { error, result }
                if matches!(error.as_ref(),Type::BorrowedStorage{storage,..} if matches!(storage.as_ref(),Type::Record{name,..}|Type::Package{name,..} if name.starts_with("@std.task:v1:TaskCreateFailure:")))
                    && matches!(result.as_ref(),Type::Managed(t) if matches!(t.as_ref(),Type::Task{..})) =>
            {
                // Representation after immutable independent Verify: failure
                // and success keep the ordinary tagged physical ABI. This
                // walk pays every original qualified input and Task port.
                self.pay(ty, depth)?;
                self.borrowed(error, depth + 1)?;
                self.pay(result, depth + 1)?;
                self.copy_work(result, depth + 2)?;
                Ok(true)
            }
            Type::Tuple(fields) if !ty.is_copy_value() => {
                self.pay(ty, depth)?;
                for field in fields {
                    if !self.borrowed(field, depth + 1)? {
                        self.copy_work(field, depth + 1)?;
                        if !field.is_copy_value() {
                            return Ok(false);
                        }
                    }
                }
                Ok(true)
            }
            Type::ErrorUnion { error, result } if shared_sum(ty) => {
                self.pay(ty, depth)?;
                for arm in [error.as_ref(), result.as_ref()] {
                    if !self.borrowed(arm, depth + 1)? {
                        self.pay(arm, depth + 1)?;
                        self.copy_work(arm, depth + 2)?;
                    }
                }
                Ok(true)
            }
            Type::Array { element, length }
                if *length > 0
                    && matches!(element.as_ref(), Type::Address { mutable: false, .. }) =>
            {
                self.pay(ty, depth)?;
                self.copy_work(ty, depth + 1)?;
                let Type::Address { inner, .. } = element.as_ref() else {
                    unreachable!()
                };
                self.pure_composite(inner, depth + 2)
            }
            Type::Address { inner, .. }
                if matches!(inner.as_ref(), Type::Array { element, length }
                    if *length > 0 && matches!(element.as_ref(), Type::Address { mutable: false, .. })) =>
            {
                self.pay(ty, depth)?;
                self.borrowed(inner, depth + 1)
            }
            Type::BorrowedStorage { storage, life } => {
                self.pay(ty, depth)?;
                self.stored_shape(life)?;
                self.pay(storage, depth + 1)?;
                self.copy_work(storage, depth + 2)?;
                Ok(true)
            }
            Type::Address { inner, .. }
                if matches!(inner.as_ref(), Type::BorrowedStorage { .. }) =>
            {
                self.pay(ty, depth)?;
                self.borrowed(inner, depth + 1)
            }
            _ => Ok(false),
        }
    }
    /// Local/SSA representation only. The verifier already proved this actual
    /// root address and access mode; only the outer pointer is transparent.
    /// pure_composite still rejects every embedded target-bearing leaf.
    pub(super) fn pure_value_or_address(&mut self, ty: &Type, depth: usize) -> CompileResult<bool> {
        if let Type::Address { inner, .. } = ty {
            self.pay(ty, depth)?;
            self.pure_composite(inner, depth + 1)
        } else {
            self.pure_composite(ty, depth)
        }
    }

    /// Physical representation only, after all prior admission paths failed.
    /// Keep the full verified type: an empty stored slot inventory and ordinary
    /// inline leaves do not provide an address, loan or owning capability.
    pub(super) fn pure_composite(&mut self, ty: &Type, depth: usize) -> CompileResult<bool> {
        self.pay(ty, depth)?;
        match ty {
            Type::BorrowedStorage { storage, life } => {
                self.stored_shape(life)?;
                if !life.slots.is_empty() {
                    return Ok(false);
                }
                self.pure_composite(storage, depth + 1)
            }
            Type::Tuple(fields) => {
                for field in fields {
                    if !self.pure_composite(field, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Type::Record { fields, .. } => {
                for (_, field) in fields {
                    if !self.pure_composite(field, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    for field in fields {
                        if !self.pure_composite(field, depth + 1)? {
                            return Ok(false);
                        }
                    }
                }
                Ok(true)
            }
            Type::Array { element, .. } => self.pure_composite(element, depth + 1),
            Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I8
            | Type::I16
            | Type::Isize
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::F32
            | Type::F64
            | Type::U8
            | Type::U16
            | Type::U32 => Ok(true),
            _ => Ok(false),
        }
    }

    fn copy_work(&mut self, ty: &Type, depth: usize) -> CompileResult<()> {
        if let Type::BorrowedFunctionPointer { life, .. } = ty {
            self.stored_contract(life)?;
        }
        match ty {
            Type::BorrowedStorage { storage, life } => {
                self.stored_shape(life)?;
                self.pay(storage, depth)?;
                self.copy_work(storage, depth + 1)?;
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                for (_, t) in fields {
                    self.pay(t, depth)?;
                    self.copy_work(t, depth + 1)?;
                }
            }
            Type::Tuple(fields) => {
                for t in fields {
                    self.pay(t, depth)?;
                    self.copy_work(t, depth + 1)?;
                }
            }
            Type::Managed(element)
            | Type::Desc(element)
            | Type::OwnedStore { element }
            | Type::AsyncRegistration {
                environment: element,
            }
            | Type::Registration {
                environment: element,
            }
            | Type::Array { element, .. }
            | Type::Slice { element }
            | Type::Address { inner: element, .. }
            | Type::Opaque {
                hidden: element, ..
            } => {
                self.pay(element, depth)?;
                self.copy_work(element, depth + 1)?;
            }
            Type::FunctionPointer { parameters, result }
            | Type::BorrowedFunctionPointer {
                parameters, result, ..
            }
            | Type::ForeignCallback { parameters, result } => {
                for t in parameters {
                    self.pay(t, depth)?;
                    self.copy_work(t, depth + 1)?;
                }
                self.pay(result, depth)?;
                self.copy_work(result, depth + 1)?;
            }
            Type::Task {
                result,
                retained_life,
            } => {
                self.metadata(
                    retained_life
                        .ports
                        .len()
                        .saturating_add(retained_life.retained.len())
                        .saturating_add(retained_life.nominal_arguments.len())
                        .saturating_add(1),
                )?;
                for port in &retained_life.ports {
                    self.pay(&port.leaf, depth + 1)?;
                    self.copy_work(&port.leaf, depth + 1)?;
                }
                for row in &retained_life.retained {
                    self.metadata(row.path.len().saturating_add(2))?;
                }
                for row in &retained_life.nominal_arguments {
                    self.metadata(row.path.len().saturating_add(4))?;
                }
                self.pay(result, depth + 1)?;
                self.copy_work(result, depth + 1)?;
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    for t in fields {
                        self.pay(t, depth)?;
                        self.copy_work(t, depth + 1)?;
                    }
                }
            }
            Type::ErrorUnion { error, result } => {
                self.pay(error, depth)?;
                self.copy_work(error, depth + 1)?;
                self.pay(result, depth)?;
                self.copy_work(result, depth + 1)?;
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn admits(
        &mut self,
        ty: &Type,
        resources: &[zil_ir::ResourceDefinition],
        depth: usize,
    ) -> CompileResult<bool> {
        self.pay(ty, depth)?;
        match ty {
            Type::BorrowedStorage { .. } => self.borrowed(ty, depth + 1),
            Type::Address { inner, .. }
                if matches!(
                    inner.as_ref(),
                    Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }
                ) =>
            {
                self.admits(inner, resources, depth + 1)
            }
            Type::Managed(payload) => {
                for _ in resources {
                    self.pay(ty, depth)?;
                    self.copy_work(payload, depth + 1)?;
                }
                Ok(resource_type(ty, resources))
            }
            Type::Tuple(fields) => {
                for t in fields {
                    if !self.admits(t, resources, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Type::Package { fields, .. } => {
                for (_, t) in fields {
                    if !self.admits(t, resources, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    for t in fields {
                        if !self.admits(t, resources, depth + 1)? {
                            return Ok(false);
                        }
                    }
                }
                Ok(true)
            }
            Type::ErrorUnion { error, result } => Ok(self.admits(error, resources, depth + 1)?
                && self.admits(result, resources, depth + 1)?),
            _ => {
                self.copy_work(ty, depth + 1)?;
                Ok(ty.is_copy_value())
            }
        }
    }

    // This is a representation admission after independent Verify, not a
    // source ownership rule. Only visit it when legacy capture admission failed.
    pub(super) fn inline_store_capture(
        &mut self,
        ty: &Type,
        resources: &[zil_ir::ResourceDefinition],
    ) -> CompileResult<bool> {
        fn contains(b: &mut Budget<'_>, ty: &Type, depth: usize) -> CompileResult<bool> {
            b.pay(ty, depth)?;
            let mut found = false;
            match ty {
                Type::Managed(payload) => {
                    return Ok(matches!(payload.as_ref(), Type::OwnedStore { element }
                        if !matches!(element.as_ref(), Type::Package { .. })));
                }
                Type::Package { fields, .. } => {
                    b.metadata(fields.len())?;
                    for (_, field) in fields {
                        found |= contains(b, field, depth + 1)?;
                    }
                }
                Type::Tuple(fields) => {
                    b.metadata(fields.len())?;
                    for field in fields {
                        found |= contains(b, field, depth + 1)?;
                    }
                }
                Type::Enum { variants, .. } => {
                    b.metadata(variants.len())?;
                    for (_, fields) in variants {
                        b.metadata(fields.len())?;
                        for field in fields {
                            found |= contains(b, field, depth + 1)?;
                        }
                    }
                }
                Type::ErrorUnion { error, result } => {
                    b.metadata(2)?;
                    found = contains(b, error, depth + 1)? | contains(b, result, depth + 1)?;
                }
                _ => {}
            }
            Ok(found)
        }
        Ok(contains(self, ty, 0)? && self.admits(ty, resources, 0)?)
    }
}
fn limit() -> Vec<Diagnostic> {
    vec![Diagnostic::limit(
        Phase::Lower,
        "ordinary Sum layout admission budget exceeded",
        None,
    )]
}

// Structural snapshot accounting only; the immutable Verify capability supplies
// semantic authority. This preserves new function-instance records in owned LIR.
impl zil_ir::DynRecipeVisitor for Budget<'_> {
    type Error = Vec<Diagnostic>;
    fn node(&mut self, depth: usize) -> CompileResult<()> {
        if depth > self.limits.max_call_depth.min(128) {
            return Err(limit());
        }
        self.metadata(1)
    }
    fn rows(&mut self, n: usize, depth: usize) -> CompileResult<()> {
        zil_ir::DynRecipeVisitor::node(self, depth)?;
        self.metadata(n)
    }
    fn ty(&mut self, ty: &Type, depth: usize) -> CompileResult<()> {
        zil_ir::DynRecipeVisitor::node(self, depth)?;
        self.pay(ty, depth)?;
        self.copy_work(ty, depth)
    }
    fn life(&mut self, life: &zil_ir::StorageLifeShape, depth: usize) -> CompileResult<()> {
        zil_ir::DynRecipeVisitor::node(self, depth)?;
        self.metadata(life.binder_count as usize)?;
        self.stored_shape(life)
    }
    fn identity(
        &mut self,
        identity: &zil_ir::LifeDeclarationIdentity,
        depth: usize,
    ) -> CompileResult<()> {
        zil_ir::DynRecipeVisitor::node(self, depth)?;
        self.metadata(
            identity
                .module_path
                .len()
                .saturating_add(identity.declaration_path.len())
                .saturating_add(identity.generation_path.len()),
        )?;
        self.static_bytes(identity.package.len())?;
        for part in identity
            .module_path
            .iter()
            .chain(&identity.declaration_path)
        {
            self.static_bytes(part.len())?;
        }
        Ok(())
    }
    fn position(&mut self, position: &zil_ir::StoragePosition, depth: usize) -> CompileResult<()> {
        zil_ir::DynRecipeVisitor::rows(
            self,
            position.path.len(),
            depth.saturating_add(position.path.len()),
        )
    }
    fn constant(&mut self, value: &zil_ir::Constant, depth: usize) -> CompileResult<()> {
        zil_ir::DynRecipeVisitor::node(self, depth)?;
        if let zil_ir::Constant::WideString(units) = value {
            self.metadata(units.len())?;
        }
        Ok(())
    }
    fn text(&mut self, value: &str, depth: usize) -> CompileResult<()> {
        zil_ir::DynRecipeVisitor::node(self, depth)?;
        self.static_bytes(value.len())
    }
}

#[cfg(test)]
mod owned_sum_tests {
    fn pure_leaf() -> Type {
        Type::BorrowedStorage {
            storage: Box::new(Type::Record {
                name: "Leaf".into(),
                fields: vec![("bits".into(), Type::U64)],
            }),
            life: zil_ir::StorageLifeShape {
                binder_count: 1,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![zil_ir::NominalLifeArgument {
                    path: vec![],
                    declaration: zil_ir::LifeDeclarationId {
                        module: zil_base::ModuleId(0),
                        declaration: zil_base::NodeId(1),
                        compiler: false,
                    },
                    argument: 0,
                    binder: 0,
                }],
            },
        }
    }

    #[test]
    fn compound_representation_keeps_child_scopes_and_one_remaining_budget() {
        // Representation probe only; complete actual nominal/loan authority is
        // independently checked by Verify before any public Lower entry.
        let child = |mutable| {
            let mut ty = pure_leaf();
            let Type::BorrowedStorage { storage, life } = &mut ty else {
                unreachable!()
            };
            let Type::Record { fields, .. } = storage.as_mut() else {
                unreachable!()
            };
            fields[0].1 = Type::Address {
                mutable,
                inner: Box::new(Type::I32),
            };
            life.slots.push(zil_ir::StorageLifeSlot {
                path: vec![zil_ir::PlaceProjection::Field(0)],
                binder: 0,
                mode: if mutable {
                    zil_ir::StoredBorrowMode::ExclusiveAddress
                } else {
                    zil_ir::StoredBorrowMode::SharedAddress
                },
            });
            ty
        };
        for mutable in [false, true] {
            let ty = Type::Tuple(vec![child(mutable), child(mutable)]);
            let snapshot = ty.clone();
            let defaults = zil_base::Limits::default();
            let mut measured = Budget::new(&defaults);
            assert!(measured.borrowed(&ty, 0).unwrap());
            let limits = zil_base::Limits {
                max_analysis_steps: measured.work,
                max_nodes: measured.nodes,
                ..defaults.clone()
            };
            let mut exact = Budget::new(&limits);
            assert!(exact.borrowed(&ty, 0).unwrap());
            assert_eq!(
                exact.borrowed(&ty, 0).unwrap_err()[0].category,
                zil_base::ResultCategory::ResourceLimit
            );
            let short = zil_base::Limits {
                max_analysis_steps: measured.work - 1,
                ..defaults
            };
            assert_eq!(
                Budget::new(&short).borrowed(&ty, 0).unwrap_err()[0].category,
                zil_base::ResultCategory::ResourceLimit
            );
            assert!(Budget::new(&limits).borrowed(&ty, 0).unwrap());
            assert_eq!(ty, snapshot);
            assert!(!ty.is_copy_value());
            eprintln!(
                "compound representation mutable={mutable} N={} nodes={}",
                measured.work, measured.nodes
            );
        }
    }

    #[test]
    fn lower_qualified_array_preserves_metadata_and_shared_budget() {
        // Isolated Lower representation probe: this raw type is not source or
        // nominal authority. Actual Source tests traverse Verify beforehand.
        let ty = Type::Array {
            element: Box::new(pure_leaf()),
            length: 4096,
        };
        let defaults = zil_base::Limits::default();
        let mut measured = Budget::new(&defaults);
        assert!(measured.pure_composite(&ty, 0).unwrap());
        let needed = measured.work;
        let nodes = measured.nodes;
        let limits = zil_base::Limits {
            max_analysis_steps: needed,
            max_nodes: nodes,
            ..defaults.clone()
        };
        let mut exact = Budget::new(&limits);
        assert!(exact.pure_composite(&ty, 0).unwrap());
        assert_eq!(exact.work, needed);
        assert_eq!(exact.nodes, nodes);
        assert_eq!(
            exact.pure_composite(&ty, 0).unwrap_err()[0].category,
            zil_base::ResultCategory::ResourceLimit
        );
        let short = zil_base::Limits {
            max_analysis_steps: needed - 1,
            ..defaults.clone()
        };
        assert_eq!(
            Budget::new(&short).pure_composite(&ty, 0).unwrap_err()[0].category,
            zil_base::ResultCategory::ResourceLimit
        );
        assert!(Budget::new(&limits).pure_composite(&ty, 0).unwrap());
        let one = Type::Array {
            element: Box::new(pure_leaf()),
            length: 1,
        };
        let mut homogeneous = Budget::new(&defaults);
        assert!(homogeneous.pure_composite(&one, 0).unwrap());
        assert_eq!(
            homogeneous.work, needed,
            "Lower visits homogeneous element metadata once; codegen pays physical cardinality"
        );
        let Type::Array { element, length } = &ty else {
            unreachable!()
        };
        assert_eq!(*length, 4096);
        let Type::BorrowedStorage { life, .. } = element.as_ref() else {
            unreachable!()
        };
        assert_eq!(life.nominal_arguments[0].argument, 0);
        assert_eq!(life.nominal_arguments[0].binder, 0);
        eprintln!("enum_member_controls Lower pure qualified Array workN={needed} nodesN={nodes}");
    }

    #[test]
    fn lower_index_root_address_keeps_one_budget_and_rejects_embedded_targets() {
        // Private physical admission probe; actual root/access permission must
        // already have been proved by Verify for a real lowering request.
        let defaults = zil_base::Limits::default();
        let mut nested = Type::Array {
            element: Box::new(pure_leaf()),
            length: 1,
        };
        for _ in 0..13 {
            nested = Type::Array {
                element: Box::new(nested),
                length: 2,
            };
        }
        for mutable in [false, true] {
            let root = Type::Address {
                inner: Box::new(nested.clone()),
                mutable,
            };
            let mut measured = Budget::new(&defaults);
            assert!(measured.pure_value_or_address(&root, 0).unwrap());
            assert!(
                !Budget::new(&defaults).pure_composite(&root, 0).unwrap(),
                "pure values and new signatures do not admit Address"
            );
            let needed = measured.work;
            let limits = zil_base::Limits {
                max_analysis_steps: needed,
                ..defaults.clone()
            };
            assert!(
                Budget::new(&limits)
                    .pure_value_or_address(&root, 0)
                    .unwrap()
            );
            let short = zil_base::Limits {
                max_analysis_steps: needed - 1,
                ..defaults.clone()
            };
            assert_eq!(
                Budget::new(&short)
                    .pure_value_or_address(&root, 0)
                    .unwrap_err()[0]
                    .category,
                zil_base::ResultCategory::ResourceLimit
            );
            assert!(
                Budget::new(&limits)
                    .pure_value_or_address(&root, 0)
                    .unwrap()
            );
            let Type::Address {
                mutable: actual, ..
            } = &root
            else {
                unreachable!()
            };
            assert_eq!(*actual, mutable);
            eprintln!("enum_member_controls Lower index root mutable={mutable} workN={needed}");
        }
        let embedded = Type::Address {
            inner: Box::new(Type::Array {
                element: Box::new(Type::Address {
                    inner: Box::new(pure_leaf()),
                    mutable: false,
                }),
                length: 4096,
            }),
            mutable: false,
        };
        assert!(
            !Budget::new(&defaults)
                .pure_value_or_address(&embedded, 0)
                .unwrap()
        );
        let owning = Type::Address {
            inner: Box::new(Type::Array {
                element: Box::new(Type::Managed(Box::new(Type::ByteBuffer))),
                length: 4096,
            }),
            mutable: false,
        };
        assert!(
            !Budget::new(&defaults)
                .pure_value_or_address(&owning, 0)
                .unwrap()
        );
    }

    #[test]
    fn lower_new_pure_path_rejects_inactive_targets_and_slots() {
        let limits = zil_base::Limits::default();
        let mut choice = Type::Enum {
            name: "Choice".into(),
            variants: vec![("Value".into(), vec![pure_leaf()])],
        };
        assert!(Budget::new(&limits).pure_composite(&choice, 0).unwrap());
        let Type::Enum { variants, .. } = &mut choice else {
            unreachable!()
        };
        variants.push((
            "Inactive".into(),
            vec![Type::Address {
                inner: Box::new(Type::U64),
                mutable: false,
            }],
        ));
        assert!(!Budget::new(&limits).pure_composite(&choice, 0).unwrap());
        let managed = Type::Array {
            element: Box::new(Type::Managed(Box::new(Type::ByteBuffer))),
            length: 4096,
        };
        assert!(!Budget::new(&limits).pure_composite(&managed, 0).unwrap());
        let mut leaf = pure_leaf();
        let Type::BorrowedStorage { life, .. } = &mut leaf else {
            unreachable!()
        };
        life.slots.push(zil_ir::StorageLifeSlot {
            path: vec![],
            binder: 0,
            mode: zil_ir::StoredBorrowMode::SharedAddress,
        });
        let slotted = Type::Array {
            element: Box::new(leaf),
            length: 4096,
        };
        assert!(!Budget::new(&limits).pure_composite(&slotted, 0).unwrap());
    }

    #[test]
    fn callable_relation_lowering_uses_existing_remaining_ledger() {
        use zil_ir::{
            LifeTypeProjection, SignatureLifeClass, SignatureLifeClassBinding, StorageBase,
            StorageBinder, StorageLifeContract,
        };
        let defaults = zil_base::Limits::default();
        for (rows, path) in [(0, 0), (8, 4), (32, 32)] {
            let life = StorageLifeContract {
                static_bindings: vec![],
                fixed_binder_count: 0,
                invocation_binder_count: 1,
                bindings: vec![],
                classes: (0..rows)
                    .map(|class| SignatureLifeClassBinding {
                        position: SignatureLifeClass {
                            base: StorageBase::Parameter(0),
                            path: vec![LifeTypeProjection::Pointee; path],
                            class,
                        },
                        binder: StorageBinder::Invocation(0),
                    })
                    .collect(),
            };
            let needed = {
                let mut budget = Budget::new(&defaults);
                budget.pay(&Type::I32, 0).unwrap();
                budget.stored_contract(&life).unwrap();
                budget.work
            };
            let run = |n| {
                let limits = zil_base::Limits {
                    max_analysis_steps: n,
                    ..defaults.clone()
                };
                let mut budget = Budget::new(&limits);
                budget.pay(&Type::I32, 0)?;
                budget.stored_contract(&life)
            };
            run(needed).unwrap();
            assert_eq!(
                run(needed - 1).unwrap_err()[0].category,
                zil_base::ResultCategory::ResourceLimit
            );
            run(needed).unwrap();
            eprintln!("callable40 relation rows={rows} path={path} Lower N={needed}");
        }
    }

    use super::*;
    #[test]
    fn lower_uses_one_actual_request_budget_and_fresh_boundary() {
        let ty = Type::Enum {
            name: "Choice".into(),
            variants: vec![
                ("None".into(), vec![]),
                (
                    "Some".into(),
                    vec![Type::Managed(Box::new(Type::ByteBuffer))],
                ),
            ],
        };
        let resources = vec![zil_ir::ResourceDefinition {
            name: zil_ir::BYTE_BUFFER_RESOURCE_NAME.into(),
            payload: Type::ByteBuffer,
            teardown: None,
        }];
        let mut limits = zil_base::Limits::default();
        let needed = {
            let mut budget = Budget::new(&limits);
            assert!(budget.admits(&ty, &resources, 0).unwrap());
            budget.work
        };
        limits.max_analysis_steps = needed;
        assert!(Budget::new(&limits).admits(&ty, &resources, 0).unwrap());
        limits.max_analysis_steps = needed - 1;
        assert_eq!(
            Budget::new(&limits).admits(&ty, &resources, 0).unwrap_err()[0].category,
            zil_base::ResultCategory::ResourceLimit
        );
        limits.max_analysis_steps = needed;
        assert!(Budget::new(&limits).admits(&ty, &resources, 0).unwrap());
        // The same phase may not restart its remaining count for a second type.
        let mut shared = Budget::new(&limits);
        assert!(shared.admits(&ty, &resources, 0).unwrap());
        assert_eq!(
            shared.admits(&ty, &resources, 0).unwrap_err()[0].category,
            zil_base::ResultCategory::ResourceLimit
        );
    }

    #[test]
    fn lower_callback_argument_and_result_share_one_budget() {
        let event = Type::Package {
            name: "Event".into(),
            fields: vec![
                ("payload".into(), Type::Managed(Box::new(Type::ByteBuffer))),
                ("tag".into(), Type::I32),
            ],
        };
        let reply = Type::Package {
            name: "Reply".into(),
            fields: vec![
                ("payload".into(), Type::Managed(Box::new(Type::ByteBuffer))),
                ("tag".into(), Type::I32),
            ],
        };
        let resources = vec![zil_ir::ResourceDefinition {
            name: zil_ir::BYTE_BUFFER_RESOURCE_NAME.into(),
            payload: Type::ByteBuffer,
            teardown: None,
        }];
        let mut limits = zil_base::Limits::default();
        let needed = {
            let mut budget = Budget::new(&limits);
            assert!(budget.admits(&reply, &resources, 0).unwrap());
            assert!(budget.admits(&event, &resources, 0).unwrap());
            budget.work
        };
        eprintln!("lower signature budget: owning N={needed}");
        limits.max_analysis_steps = needed;
        let mut exact = Budget::new(&limits);
        assert!(exact.admits(&reply, &resources, 0).unwrap());
        assert!(exact.admits(&event, &resources, 0).unwrap());
        limits.max_analysis_steps = needed - 1;
        let mut short = Budget::new(&limits);
        assert!(short.admits(&reply, &resources, 0).unwrap());
        assert_eq!(
            short.admits(&event, &resources, 0).unwrap_err()[0].category,
            zil_base::ResultCategory::ResourceLimit
        );
        limits.max_analysis_steps = needed;
        let mut fresh = Budget::new(&limits);
        assert!(fresh.admits(&reply, &resources, 0).unwrap());
        assert!(fresh.admits(&event, &resources, 0).unwrap());
    }
}
