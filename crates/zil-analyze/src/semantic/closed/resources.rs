//! Whole resource scope reconstruction. Complete Task domains retain every
//! original child class. Only a full payload and creator inventory can prove
//! zero; an owning wrapper or an empty physical port list cannot do so.
use super::*;
use crate::semantic::value::{self, Value};
impl Check<'_, '_> {
    pub(super) fn task_domain(&self, ty: &Type, b: &mut Budget) -> CompileResult<Option<StorageLifeShape>> {
        value::task_domain(ty, b)
    }
    pub(super) fn resource_constructor(&self, kind: geometry::Unary, child: Value, b: &mut Budget) -> CompileResult<Value> {
        if !empty(&child.life) || !child.classes.is_empty() {
            if kind != geometry::Unary::Managed || !matches!(child.ty, Type::Task { .. }) {
                return Err(unknown("semantic resource constructor needs complete child retained and phantom class transport"));
            }
            if self.task_domain(&child.ty, b)?.as_ref() != Some(&child.life) {
                return Err(fail("Task resource constructor changes its full original child domain"));
            }
        }
        value::resource(kind, child, b)
    }
    pub(super) fn task_child(&self, input: &Value, b: &mut Budget) -> CompileResult<Value> {
        let Type::Managed(inner) = &input.ty else { return Err(fail("Task child projection has no owning carrier")); };
        let life = self.task_domain(inner, b)?.ok_or_else(|| fail("Task child changes its complete payload family"))?;
        if self.task_domain(&input.ty, b)?.as_ref() != Some(&input.life)
            || input.classes.len() != life.binder_count as usize {
            return Err(fail("Task child projection changes its complete owning domain"));
        }
        let mut classes = Vec::new();
        for class in &input.classes { self.push_value(&mut classes, class.clone(), b)?; }
        value::borrowed_full(inner, &life, classes, self.inventory, b)
    }
    pub(super) fn task_projection(&self, parent: &Value, child: &Type, selector: PlaceProjection,
        b: &mut Budget) -> CompileResult<Value> {
        let life = self.task_domain(child, b)?.ok_or_else(|| fail("Task projection changes its child family"))?;
        if self.task_domain(&parent.ty, b)?.as_ref() != Some(&parent.life)
            || parent.classes.len() != parent.life.binder_count as usize {
            return Err(fail("Task projection has another complete parent domain"));
        }
        let mut classes = Vec::new();
        for class in 0..life.binder_count {
            let mut selected = None;
            for row in &parent.life.retained {
                b.spend(row.path.len().saturating_add(1), WORK)?;
                if row.path.as_slice() == [LifeTypeProjection::Place(selector)] && row.class == class
                    && selected.replace(row.binder).is_some() {
                    return Err(fail("Task sum repeats a child class occurrence"));
                }
            }
            let binder = selected.ok_or_else(|| fail("Task sum omits a complete child class"))?;
            self.push_value(&mut classes, parent.classes.get(binder as usize)
                .ok_or_else(|| fail("Task sum class exceeds its complete parent domain"))?.clone(), b)?;
        }
        value::borrowed_full(child, &life, classes, self.inventory, b)
    }
    /// Physical retained ports select the independently preserved opaque
    /// classes. No port is created for a phantom class.
    pub(super) fn task_ports(&self, value: &Value, b: &mut Budget)
        -> CompileResult<Vec<(Vec<ContractProjection>, u32)>> {
        let (task, prefix): (&Type, Vec<ContractProjection>) = match &value.ty {
            Type::Managed(inner) if matches!(inner.as_ref(), Type::Task { .. }) => (inner, Vec::new()),
            Type::ErrorUnion { result, .. } if matches!(result.as_ref(), Type::Managed(inner) if matches!(inner.as_ref(), Type::Task { .. })) => {
                let Type::Managed(inner) = result.as_ref() else { unreachable!() };
                (inner, vec![ContractProjection::Place(PlaceProjection::ErrorArm(false))])
            }
            _ => return Ok(Vec::new()),
        };
        if self.task_domain(&value.ty, b)?.as_ref() != Some(&value.life) {
            return Err(fail("Task retained port value changes its complete domain forest"));
        }
        let Type::Task { retained_life, .. } = task else { unreachable!() };
        let mut rows = Vec::new();
        for (ordinal, port) in retained_life.ports.iter().enumerate() {
            let mut path = Vec::new();
            for step in &prefix { self.push_value(&mut path, step.clone(), b)?; }
            self.push_value(&mut path, ContractProjection::RetainedPort(
                u32::try_from(ordinal).map_err(|_| fail("Task retained port ordinal overflows"))?), b)?;
            self.push_value(&mut rows, (path, port.binder), b)?;
        }
        Ok(rows)
    }
    pub(super) fn zero_data(&self, ty: &Type, b: &mut Budget, depth: usize) -> CompileResult<()> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "complete zero-domain capture graph exceeds its bounded depth",
            ));
        }
        if ty.is_b0_scalar()
            || matches!(ty, Type::ForeignHandle | Type::CWideString)
            || *ty == crate::generic_mutex::error_type()
            || crate::dyn_dispatch::is_create_error(ty)
        {
            return Ok(());
        }
        match ty {
            Type::Dyn { .. } | Type::OwnedDyn { .. } => {
                if !empty(&value::dyn_domain(ty, self.inventory, b)?) {
                    return Err(fail("zero-domain dynamic value retains complete Context classes"));
                }
                if !self.dyn_value_type(ty, b, depth + 1)? {
                    return Err(fail(
                        "dynamic zero-domain type has no complete original formation",
                    ));
                }
                Ok(())
            }
            Type::Managed(_) => self.resource_zero(ty, b, depth + 1),
            Type::Closure(id) => {
                if !self.capture_directory(*id, b)?.is_empty() {
                    return Err(fail(
                        "zero-domain capture retains a complete original head class",
                    ));
                }
                let closure = self
                    .module
                    .closures
                    .get(id.0 as usize)
                    .filter(|closure| closure.id == *id)
                    .ok_or_else(|| fail("zero-domain child closure has another actual identity"))?;
                let life = closure
                    .storage_life
                    .as_ref()
                    .ok_or_else(|| unknown("zero-domain child environment is Unknown"))?;
                if life.fixed_binder_count != 0 {
                    return Err(fail(
                        "zero-domain child environment retains nominal or phantom classes",
                    ));
                }
                for (index, capture) in closure.captures.iter().enumerate() {
                    // The directory has already checked the original full
                    // value and every actual creator's literal leaves. Reuse
                    // those complete values, never the proposed Static header
                    // or an omitted raw Address slot, for nested captures.
                    if !self.physical_byte_paths(capture, b)?.is_empty() {
                        let worlds = self.function_worlds(closure.function, b)?;
                        if worlds.is_empty() {
                            return Err(fail("Static captured aggregate has no original world"));
                        }
                        for world in worlds {
                            let bindings = self.capture_creator_bindings(world, closure, b)?;
                            let mut selected = None;
                            for binding in bindings.values() {
                                b.spend(1, WORK)?;
                                if binding.capture == index as u32
                                    && selected.replace(&binding.stored).is_some() {
                                    return Err(fail("Static captured aggregate repeats its original creator"));
                                }
                            }
                            let stored = selected.ok_or_else(|| fail("Static captured aggregate lacks its complete creator"))?;
                            if !geometry::same_type(&stored.ty, capture, b)?
                                || !empty(&stored.life) || !stored.classes.is_empty() {
                                return Err(fail("Static captured aggregate retains an original variable domain"));
                            }
                        }
                    } else {
                        self.zero_data(capture, b, depth + 1)?;
                    }
                }
                Ok(())
            }
            Type::Tuple(fields) => {
                for field in fields {
                    self.zero_data(field, b, depth + 1)?;
                }
                Ok(())
            }
            Type::Array { element, .. } => self.zero_data(element, b, depth + 1),
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                self.nominal_inventory(ty, b)?;
                for (_, field) in fields {
                    self.zero_data(field, b, depth + 1)?;
                }
                Ok(())
            }
            Type::Enum { variants, .. } => {
                self.nominal_inventory(ty, b)?;
                for (_, fields) in variants {
                    for field in fields {
                        self.zero_data(field, b, depth + 1)?;
                    }
                }
                Ok(())
            }
            Type::BorrowedStorage { storage, life } => {
                self.nominal_inventory(ty, b)?;
                if !empty(life) {
                    return Err(fail(
                        "zero-domain capture retains nominal or phantom classes",
                    ));
                }
                match storage.as_ref() {
                    Type::Record { fields, .. } | Type::Package { fields, .. } => {
                        for (_, field) in fields {
                            self.zero_data(field, b, depth + 1)?;
                        }
                        Ok(())
                    }
                    Type::Enum { variants, .. } => {
                        for (_, fields) in variants {
                            for field in fields {
                                self.zero_data(field, b, depth + 1)?;
                            }
                        }
                        Ok(())
                    }
                    _ => Err(unknown(
                        "zero-domain qualified capture needs its complete intrinsic inventory",
                    )),
                }
            }
            Type::ErrorUnion { error, result } => {
                self.zero_data(error, b, depth + 1)?;
                self.zero_data(result, b, depth + 1)
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                for parameter in parameters {
                    self.zero_data(parameter, b, depth + 1)?;
                }
                self.zero_data(result, b, depth + 1)
            }
            _ => Err(unknown(
                "capture has a nonzero, opaque or incomplete intrinsic domain",
            )),
        }
    }
    pub(super) fn resource_zero(
        &self,
        ty: &Type,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "actual resource scope exceeds its bounded inventory depth",
            ));
        }
        let Type::Managed(payload) = ty else {
            return Err(fail("actual resource scope has no complete owning carrier"));
        };
        if let Type::Task { retained_life, .. } = payload.as_ref() {
            crate::dyn_metadata::retained_shape(retained_life, b)?;
            if retained_life.binder_count != 0 || !retained_life.ports.is_empty()
                || !retained_life.retained.is_empty() || !retained_life.nominal_arguments.is_empty() {
                return Err(fail("zero resource retains the complete Task input/result/phantom domain"));
            }
        }
        self.resource_payload_scope(ty, payload, b, depth)
    }
    /// Qualify a real owning or weak carrier's payload without manufacturing a
    /// Managed Type. This grants neither an initialized owner nor a weak loan.
    pub(super) fn borrowed_resource_scope(
        &self,
        ty: &Type,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "actual resource scope exceeds its bounded inventory depth",
            ));
        }
        let (Type::Managed(payload) | Type::Desc(payload)) = ty else {
            return Err(fail("actual resource scope has no complete carrier"));
        };
        self.resource_payload_scope(ty, payload, b, depth)
    }
    fn resource_payload_scope(&self, ty: &Type, payload: &Type, b: &mut Budget, depth: usize) -> CompileResult<()> {
        match payload {
            Type::Mutex { payload } => self.mutex_payload(payload, b),
            Type::Task { .. } => {
                crate::tasks::semantic_resource_scope(
                    self.module,
                    payload,
                    self.inventory.limits,
                    b,
                )
            }
            Type::ByteBuffer => self.actual_type(ty, b),
            Type::OwnedStore { element } => {
                self.actual_type(ty, b)?;
                crate::stored_flow::validate_plain_nominal_zero(
                    self.module,
                    element,
                    self.inventory.limits,
                    b,
                )
            }
            Type::Record { .. } | Type::BorrowedStorage { .. } => {
                self.resource_nominal(payload, b, depth + 1)
            }
            Type::Registration { environment } | Type::AsyncRegistration { environment } => {
                let Type::Closure(id) = environment.as_ref() else {
                    return Err(unknown(
                        "actual registration scope lacks its real concrete callback environment",
                    ));
                };
                self.ordinary_type_at(environment, b, depth + 1)?;
                if !self.capture_directory(*id, b)?.is_empty() {
                    return Err(fail(
                        "actual registration callback retains a free original head class",
                    ));
                }
                let closure = self
                    .module
                    .closures
                    .get(id.0 as usize)
                    .filter(|closure| closure.id == *id)
                    .ok_or_else(|| fail("actual registration callback identity is absent"))?;
                let life = closure
                    .storage_life
                    .as_ref()
                    .ok_or_else(|| unknown("actual registration environment domain is Unknown"))?;
                if life.fixed_binder_count != 0 {
                    return Err(fail(
                        "actual registration environment retains a nominal or phantom class",
                    ));
                }
                crate::registration::semantic_resource_scope(self.module, payload, b)
            }
            _ => Err(unknown(
                "actual resource needs its complete original payload, cleanup and retained inventory",
            )),
        }
    }
    pub(super) fn resource_nominal(
        &self,
        payload: &Type,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "actual nominal cleanup exceeds its complete inventory depth",
            ));
        }
        let source_life = match payload { Type::BorrowedStorage { life, .. } => Some(life), _ => None };
        let geometry = self.inventory.geometry.type_value(payload, source_life, b)?;
        crate::stored_flow::validate_plain_nominal_zero(
            self.module,
            &geometry.ty,
            self.inventory.limits,
            b,
        )?;
        // This same complete call freshly reconstructs all original mixed
        // arguments, fields and N/kappa before returning the unique identity.
        // Cleanup/provider obligations below remain independent and uncached.
        let (original, arguments) = self.nominal_inventory_source_at(payload, b, depth + 1)?;
        if !matches!(
            self.inventory.definition(original, b)?.body,
            SemanticDefinitionBody::Nominal {
                kind: LifeNominalKind::Resource,
                ..
            }
        ) {
            return Err(fail(
                "actual resource payload is not its original resource declaration",
            ));
        }
        self.qualify_resource_nominal(original, &arguments, b, depth + 1)?;
        let drop = original::resources::drop_method(original, self.inventory, b)?;
        let mut actual = None;
        for resource in &self.module.resources {
            b.spend(1, WORK)?;
            let source_life = match &resource.payload { Type::BorrowedStorage { life, .. } => Some(life), _ => None };
            let candidate = self.inventory.geometry.type_value(&resource.payload, source_life, b)?;
            if geometry::same_type(&candidate.ty, &geometry.ty, b)? && actual.replace(resource).is_some() {
                return Err(fail(
                    "actual resource payload has multiple cleanup definitions",
                ));
            }
        }
        let actual =
            actual.ok_or_else(|| fail("actual resource payload has no cleanup definition"))?;
        match (drop, actual.teardown) {
            (None, None) => {}
            (Some(original), Some(target)) => {
                let worlds = self.function_worlds(target, b)?;
                if worlds.is_empty() {
                    return Err(fail(
                        "actual resource teardown lacks its original method world",
                    ));
                }
                for world in worlds {
                    if world.definition != original {
                        return Err(fail(
                            "actual resource teardown selects another original method",
                        ));
                    }
                }
                let function = self
                    .inventory
                    .functions
                    .get(&target)
                    .ok_or_else(|| fail("actual resource teardown has no real function"))?;
                if function.call_kind != CallKind::Direct || function.return_type != Type::Unit {
                    return Err(fail(
                        "actual resource teardown changes its synchronous cleanup ABI",
                    ));
                }
            }
            _ => {
                return Err(fail(
                    "actual resource cleanup presence differs from the original Drop declaration",
                ));
            }
        }
        Ok(())
    }
    pub(super) fn resolved(
        &self,
        context: &SemanticInstance,
        input: &SemanticTypeValue,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        let value = self.resolved_raw(context, input, b, depth)?;
        self.capture_class_value(value, b)
    }
    /// An original call can return another closure while transporting the
    /// entire intrinsic directory of the closure it receives. Check that
    /// exact operation and both whole values before reading its result map.
    fn closure_call_claim(
        &self,
        context: &SemanticInstance,
        function: &TypedFunction,
        source: ValueId,
        actual: &Value,
        expected_ty: &Type,
        expected_life: &StorageLifeShape,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<Value>> {
        if !matches!((&actual.ty, expected_ty), (Type::Closure(a), Type::Closure(b)) if a != b)
            || actual.classes.len() != actual.life.binder_count as usize
            || actual.classes.len() != expected_life.binder_count as usize
        {
            return Ok(None);
        }
        let actual = self.capture_class_value(actual.copy(b)?, b)?;
        let mut selected = None;
        for block in &function.blocks {
            for (index, instruction) in block.instructions.iter().enumerate() {
                b.spend(1, WORK)?;
                let (InstructionKind::Call { arguments, .. }
                    | InstructionKind::SuspendCall { arguments, .. }) = &instruction.kind else {
                        continue;
                    };
                let Some(result) = instruction.result else { continue; };
                if function.values.get(result.0 as usize) != Some(expected_ty) {
                    continue;
                }
                b.spend(arguments.len(), WORK)?;
                if !arguments.contains(&source) {
                    continue;
                }
                operations::available_before(
                    function, (block.id, index, SemanticSiteEdge::Before), source, b,
                )?;
                let result = self.ssa_value(context, function, result, b, depth + 1)?;
                let result = self.capture_class_value(result, b)?;
                crate::dyn_metadata::shape(&result.life, b)?;
                b.spend(actual.classes.len().saturating_add(result.classes.len()), WORK)?;
                if result.life != *expected_life || result.classes != actual.classes {
                    continue;
                }
                if selected.replace(result).is_some() {
                    return Err(fail("closure call claim has competing original input/result occurrences"));
                }
            }
        }
        Ok(selected)
    }

    fn resolved_raw(
        &self,
        context: &SemanticInstance,
        input: &SemanticTypeValue,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "resource whole value exceeds its bounded scope depth",
            ));
        }
        let SemanticTypeValue::Resolved { ty, life, classes } = input else {
            return Err(fail(
                "actual resource whole value retains Core-only metadata",
            ));
        };
        let geometry = self.inventory.geometry.resolved(ty, life, b)?;
        let ty = &geometry.ty;
        let life = &geometry.life;
        if let Some(value) = self.parameter_address_claim(context, input, b, depth + 1)? {
            return Ok(value);
        }
        if let Some(SemanticClassExpr::IntrinsicClass {
            operand: SemanticOperand::Value(source), path, ..
        }) = classes.first() && path.is_empty() {
            // Whole-SSA identity transport reads a separately reconstructed
            // complete value. It cannot let a claim supply its own RHS class,
            // change the child directory, or turn a same-shaped Type into a
            // different source. Nonidentity/projected recipes retain their
            // dedicated checked consumers.
            for (ordinal, expression) in classes.iter().enumerate() {
                b.spend(1, WORK)?;
                if !matches!(expression, SemanticClassExpr::IntrinsicClass {
                    operand: SemanticOperand::Value(actual), path, class,
                } if actual == source && path.is_empty() && *class as usize == ordinal) {
                    return Err(unknown("whole SSA class transport needs its complete canonical child coordinates"));
                }
            }
            let key = (context.id, *source);
            {
                let mut active = self.value_queries.borrow_mut();
                b.lookup(active.len(), WORK)?;
                if active.len() >= b.helper_nodes {
                    return Err(vec![zil_base::Diagnostic::limit(zil_base::Phase::Analyze,
                        "whole SSA class source queries exceed their finite node limit", None)]);
                }
                if !active.insert(key) {
                    return Err(fail("whole SSA class source is cyclic without an independent completed value"));
                }
            }
            let result = (|| {
            let SemanticInstanceTarget::Function(target) = &context.target else {
                return Err(fail("whole SSA class transport has no actual function world"));
            };
            let function = self.inventory.function(target, b)?;
            let actual = self.ssa_value(context, function, *source, b, depth + 1)?;
            crate::dyn_metadata::shape(&actual.life, b)?;
            crate::dyn_metadata::shape(life, b)?;
            if geometry::same_type(&actual.ty, ty, b)?
                && actual.life == *life
                && actual.classes.len() == classes.len()
                && classes.len() == life.binder_count as usize
            {
                return Ok(actual);
            }
            if classes.len() == life.binder_count as usize
                && let Some(result) = self.closure_call_claim(
                    context, function, *source, &actual, ty, life, b, depth + 1,
                )?
            {
                return Ok(result);
            }
            Err(fail("whole SSA claim changes its complete actual Type, domain or class directory"))
            })();
            self.value_queries.borrow_mut().remove(&key);
            return result;
        }
        match ty {
            Type::Managed(payload) if matches!(payload.as_ref(), Type::Task { .. }) => {
                self.borrowed_resource_scope(ty, b, depth + 1)?;
                if self.task_domain(ty, b)?.as_ref() != Some(life)
                    || classes.len() != life.binder_count as usize {
                    return Err(fail("actual Task owner changes its complete child domain"));
                }
                let mut images = Vec::new();
                for expression in classes {
                    self.push_value(&mut images, value::class(context, expression, self.inventory, b, depth + 1)?, b)?;
                }
                Ok(Value::from_geometry(geometry, images))
            }
            Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Managed(payload) if matches!(payload.as_ref(), Type::Task { .. })) =>
                value::resolved(context, input, self.inventory, b, depth + 1),
            Type::MutexGuard { .. } => {
                self.ordinary_type_at(ty, b, depth + 1)?;
                crate::dyn_metadata::shape(life, b)?;
                if life.binder_count != 1
                    || !life.slots.is_empty()
                    || !life.retained.is_empty()
                    || !life.nominal_arguments.is_empty()
                    || classes.len() != 1
                {
                    return Err(fail(
                        "actual Mutex guard changes its opaque intrinsic domain",
                    ));
                }
                let class = value::class(context, &classes[0], self.inventory, b, depth + 1)?;
                Ok(Value::from_geometry(geometry, vec![class]))
            }
            Type::Address { mutable, inner }
                if matches!(inner.as_ref(), Type::MutexGuard { .. }) =>
            {
                self.ordinary_type_at(inner, b, depth + 1)?;
                crate::dyn_metadata::shape(life, b)?;
                let [row] = life.retained.as_slice() else {
                    return Err(fail(
                        "Mutex guard reference omits its complete Pointee embedding",
                    ));
                };
                if row.path.as_slice() != [LifeTypeProjection::Pointee]
                    || row.class != 0
                    || classes.len() != life.binder_count as usize
                    || classes.get(row.binder as usize).is_none()
                {
                    return Err(fail(
                        "Mutex guard reference changes its intrinsic class selector",
                    ));
                }
                crate::dyn_metadata::ty(inner, b)?;
                let expected = value::address(
                    *mutable,
                    value::Class::Local(0),
                    Value::new(inner.as_ref().clone(), StorageLifeShape {
                            binder_count: 1,
                            slots: Vec::new(),
                            retained: Vec::new(),
                            nominal_arguments: Vec::new(),
                        }, vec![value::Class::Local(row.binder)]),
                    b,
                )?;
                if expected.life != *life {
                    return Err(fail(
                        "Mutex guard reference changes its complete outer occurrence",
                    ));
                }
                let mut images = Vec::new();
                for expression in classes {
                    let image = value::class(context, expression, self.inventory, b, depth + 1)?;
                    self.push_value(&mut images, image, b)?;
                }
                Ok(Value::from_geometry(geometry, images))
            }
            Type::Desc(inner) => {
                crate::dyn_metadata::ty(inner, b)?;
                self.resource_zero(&Type::Managed(inner.clone()), b, depth + 1)?;
                crate::dyn_metadata::shape(life, b)?;
                if life.binder_count != 1
                    || !matches!(life.slots.as_slice(), [slot] if slot.path.is_empty() && slot.binder == 0 && slot.mode == StoredBorrowMode::WritableDescription)
                    || !life.retained.is_empty()
                    || !life.nominal_arguments.is_empty()
                    || classes.len() != 1
                {
                    return Err(fail(
                        "actual zero-domain resource description changes its complete weak occurrence",
                    ));
                }
                let class = value::class(context, &classes[0], self.inventory, b, depth + 1)?;
                Ok(Value::from_geometry(geometry, vec![class]))
            }
            Type::Managed(_) => {
                self.resource_zero(ty, b, depth + 1)?;
                crate::dyn_metadata::shape(life, b)?;
                if !empty(life) || !classes.is_empty() {
                    return Err(fail(
                        "actual zero resource carries another claimed intrinsic domain",
                    ));
                }
                Ok(Value::from_geometry(geometry, Vec::new()))
            }
            Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Managed(_)) => {
                self.resource_zero(inner, b, depth + 1)?;
                crate::dyn_metadata::shape(life, b)?;
                let Type::Address { mutable, .. } = ty else {
                    return Err(fail("actual resource reference changes its carrier"));
                };
                if life.binder_count != 1
                    || life.slots.len() != 1
                    || !life.slots[0].path.is_empty()
                    || life.slots[0].binder != 0
                    || life.slots[0].mode
                        != if *mutable {
                            StoredBorrowMode::ExclusiveAddress
                        } else {
                            StoredBorrowMode::SharedAddress
                        }
                    || !life.retained.is_empty()
                    || !life.nominal_arguments.is_empty()
                    || classes.len() != 1
                {
                    return Err(fail(
                        "actual resource reference changes its complete outer occurrence or child domain",
                    ));
                }
                let class = value::class(context, &classes[0], self.inventory, b, depth + 1)?;
                Ok(Value::from_geometry(geometry, vec![class]))
            }
            _ => value::resolved(context, input, self.inventory, b, depth + 1),
        }
    }
    pub(super) fn pointee(&self, input: &Value, b: &mut Budget) -> CompileResult<Value> {
        if let Type::Address { inner, .. } = &input.ty
            && matches!(inner.as_ref(), Type::Managed(_))
            && self.task_domain(inner, b)?.is_none()
        {
            self.resource_zero(inner, b, 0)?;
            if !input.life.retained.is_empty() {
                return Err(fail(
                    "actual zero resource reference invents child class occurrences",
                ));
            }
            crate::dyn_metadata::ty(inner, b)?;
            return Ok(value::scalar(inner.as_ref().clone()));
        }
        value::pointee(input, self.inventory, b)
    }
}
