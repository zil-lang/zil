//! Bounded retention of independently verified IR, not machine admission.
use super::*;
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
            if let Some(sources) = &function.retained_return_sources {
                self.metadata(sources.len())?;
                for source in sources {
                    zil_ir::DynRecipeMetadata::visit_dyn_metadata(source, self, 0)?;
                }
            }
            if let Some(bound) = &function.reference_return_bound {
                self.metadata(bound.len())?;
            }
            self.metadata(function.protocol_contracts.len())?;
            for protocol in &function.protocol_contracts {
                zil_ir::DynRecipeMetadata::visit_dyn_metadata(protocol, self, 0)?;
            }
            if let Some(contract) = &function.callable_return_contract {
                let zil_ir::CallableReturnContract {
                    life,
                    capture_sources,
                    source: _,
                    closure: _,
                    kind: _,
                    hold,
                    call,
                    writes,
                    cleanup,
                    lending_captures,
                } = contract;
                if let Some(life) = life {
                    self.stored_contract(life)?;
                }
                self.metadata(
                    hold.len()
                        .saturating_add(call.len())
                        .saturating_add(writes.len())
                        .saturating_add(cleanup.len())
                        .saturating_add(lending_captures.len())
                        .saturating_add(1),
                )?;
                if let Some(sources) = capture_sources {
                    self.metadata(sources.len())?;
                    for source in sources {
                        zil_ir::DynRecipeMetadata::visit_dyn_metadata(source, self, 0)?;
                    }
                }
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
            self.metadata(closure.lending_captures.len())?;
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
    fn copy_work(&mut self, ty: &Type, depth: usize) -> CompileResult<()> {
        match ty {
            Type::BorrowedStorage { storage, life } => {
                self.stored_shape(life)?;
                self.retained_type(storage, depth + 1)?;
            }
            Type::Record { name: _, fields } | Type::Package { name: _, fields } => {
                for (_, field) in fields {
                    self.retained_type(field, depth + 1)?;
                }
            }
            Type::Tuple(fields) => {
                for field in fields {
                    self.retained_type(field, depth + 1)?;
                }
            }
            Type::Enum { name: _, variants } => {
                for (_, fields) in variants {
                    for field in fields {
                        self.retained_type(field, depth + 1)?;
                    }
                }
            }
            Type::Managed(child)
            | Type::Desc(child)
            | Type::OwnedStore { element: child }
            | Type::Mutex { payload: child }
            | Type::Registration { environment: child }
            | Type::AsyncRegistration { environment: child }
            | Type::Array {
                element: child,
                length: _,
            }
            | Type::Slice { element: child }
            | Type::Address {
                inner: child,
                mutable: _,
            }
            | Type::Opaque {
                hidden: child,
                identity: _,
            } => self.retained_type(child, depth + 1)?,
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                for parameter in parameters {
                    self.retained_type(parameter, depth + 1)?;
                }
                self.retained_type(result, depth + 1)?;
            }
            Type::BorrowedFunctionPointer {
                parameters,
                result,
                life,
            } => {
                self.stored_contract(life)?;
                for parameter in parameters {
                    self.retained_type(parameter, depth + 1)?;
                }
                self.retained_type(result, depth + 1)?;
            }
            Type::Task {
                result: child,
                retained_life,
            }
            | Type::MutexGuard {
                payload: child,
                retained_life,
            } => {
                let zil_ir::RetainedLifeShape {
                    binder_count: _,
                    ports,
                    retained,
                    nominal_arguments,
                } = retained_life;
                self.metadata(
                    ports
                        .len()
                        .saturating_add(retained.len())
                        .saturating_add(nominal_arguments.len())
                        .saturating_add(1),
                )?;
                for port in ports {
                    self.retained_type(&port.leaf, depth + 1)?;
                }
                for row in retained {
                    self.metadata(row.path.len().saturating_add(2))?;
                }
                for row in nominal_arguments {
                    self.metadata(row.path.len().saturating_add(4))?;
                }
                self.retained_type(child, depth + 1)?;
            }
            Type::ErrorUnion { error, result } => {
                self.retained_type(error, depth + 1)?;
                self.retained_type(result, depth + 1)?;
            }
            Type::Named(name) | Type::Unsupported(name) => self.static_bytes(name.len())?,
            Type::Dyn { interface: _ }
            | Type::OwnedDyn { interface: _ }
            | Type::Closure(_)
            | Type::SyncCell(_)
            | Type::ByteBuffer
            | Type::TaskI32
            | Type::ForeignHandle
            | Type::CWideString
            | Type::Unit
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
            | Type::F64 => {}
        }
        Ok(())
    }
    // Structural retention only. No target support or semantic capability is
    // derived here; the caller must own the immutable VerifiedModule.
    fn retained_type(&mut self, ty: &Type, depth: usize) -> CompileResult<()> {
        self.pay(ty, depth)?;
        self.copy_work(ty, depth)
    }
}
fn limit() -> Vec<Diagnostic> {
    vec![Diagnostic::limit(
        Phase::Lower,
        "verified IR retention budget exceeded",
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
mod tests {
    use super::*;
    #[test]
    fn nested_retention_charges_owned_children_and_every_enum_alternative() {
        let shape = Type::ErrorUnion {
            error: Box::new(Type::Unit),
            result: Box::new(Type::Managed(Box::new(Type::OwnedStore {
                element: Box::new(Type::Enum {
                    name: "Choice".into(),
                    variants: vec![
                        ("Empty".into(), vec![]),
                        (
                            "Live".into(),
                            vec![Type::Tuple(vec![
                                Type::Closure(zil_ir::ClosureId(7)),
                                Type::U64,
                            ])],
                        ),
                    ],
                }),
            }))),
        };
        let defaults = zil_base::Limits::default();
        let mut measured = Budget::new(&defaults);
        measured.retained_type(&shape, 0).unwrap();
        let exact = zil_base::Limits {
            max_analysis_steps: measured.work,
            ..defaults.clone()
        };
        let mut shared = Budget::new(&exact);
        shared.retained_type(&shape, 0).unwrap();
        assert_eq!(
            shared.retained_type(&shape, 0).unwrap_err()[0].category,
            zil_base::ResultCategory::ResourceLimit
        );
        let short = zil_base::Limits {
            max_analysis_steps: measured.work - 1,
            ..defaults.clone()
        };
        assert_eq!(
            Budget::new(&short).retained_type(&shape, 0).unwrap_err()[0].category,
            zil_base::ResultCategory::ResourceLimit
        );
        Budget::new(&exact).retained_type(&shape, 0).unwrap();
        let shallow = zil_base::Limits {
            max_call_depth: 2,
            ..defaults
        };
        assert_eq!(
            Budget::new(&shallow).retained_type(&shape, 0).unwrap_err()[0].category,
            zil_base::ResultCategory::ResourceLimit
        );
    }
    #[test]
    fn retained_array_is_compact_but_preserves_qualified_child_work() {
        let defaults = zil_base::Limits::default();
        let shape = |length| Type::Array {
            length,
            element: Box::new(Type::BorrowedStorage {
                storage: Box::new(Type::Address {
                    mutable: true,
                    inner: Box::new(Type::I32),
                }),
                life: zil_ir::StorageLifeShape {
                    binder_count: 1,
                    slots: vec![zil_ir::StorageLifeSlot {
                        path: vec![],
                        binder: 0,
                        mode: zil_ir::StoredBorrowMode::ExclusiveAddress,
                    }],
                    retained: vec![],
                    nominal_arguments: vec![],
                },
            }),
        };
        let mut small = Budget::new(&defaults);
        let mut large = Budget::new(&defaults);
        small.retained_type(&shape(1), 0).unwrap();
        large.retained_type(&shape(u32::MAX), 0).unwrap();
        assert_eq!((small.work, small.nodes), (large.work, large.nodes));
        let mut plain = Budget::new(&defaults);
        plain
            .retained_type(
                &Type::Array {
                    length: 1,
                    element: Box::new(Type::I32),
                },
                0,
            )
            .unwrap();
        assert!(small.work > plain.work);
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
}
