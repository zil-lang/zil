//! Ordinary Sum admission is distinct from capture/array/StoreElement profiles.
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
            fixed_binder_count: _,
            invocation_binder_count: _,
            bindings: _,
            classes: _,
        } = life;
        self.metadata(
            life.bindings
                .len()
                .saturating_add(life.classes.len())
                .saturating_mul(16)
                .saturating_add(32),
        )?;
        for row in &life.bindings {
            self.metadata(row.position.path.len().saturating_mul(8))?;
        }
        for row in &life.classes {
            self.metadata(row.position.path.len().saturating_mul(8))?;
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
    fn copy_work(&mut self, ty: &Type, depth: usize) -> CompileResult<()> {
        if matches!(ty, Type::AsyncRegistration { .. }) {
            return Err(vec![Diagnostic::unsupported(
                Phase::Lower,
                "pollable registration type consumer is not implemented",
                None,
            )]);
        }
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
}
fn limit() -> Vec<Diagnostic> {
    vec![Diagnostic::limit(
        Phase::Lower,
        "ordinary Sum layout admission budget exceeded",
        None,
    )]
}

#[cfg(test)]
mod impl243_tests {
    #[test]
    fn callable40_lower_complete_relation_uses_existing_remaining_ledger() {
        use zil_ir::{
            LifeTypeProjection, SignatureLifeClass, SignatureLifeClassBinding, StorageBase,
            StorageBinder, StorageLifeContract,
        };
        let defaults = zil_base::Limits::default();
        for (rows, path) in [(0, 0), (8, 4), (32, 32)] {
            let life = StorageLifeContract {
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
    fn impl243_lower_uses_one_actual_request_budget_and_fresh_boundary() {
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
    fn impl249_lower_callback_argument_and_result_share_one_budget() {
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
        eprintln!("impl249 lower signature budget: owning N={needed}");
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
