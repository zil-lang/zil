//! Original lifetime worlds and actual whole-type class flow. Classes here are
//! semantic identities; data targets, permissions and resource phases are not
//! manufactured by this interpreter.
use super::emission::Ledger;
use crate::*;
use std::collections::BTreeMap;
#[path = "world_arguments.rs"]
mod arguments;
#[path = "world_calls.rs"]
mod calls;
#[path = "world_capture.rs"]
mod capture;
#[path = "world_dyn.rs"]
mod dynamic;
#[path = "world_flow.rs"]
mod flow;
#[path = "world_values.rs"]
mod function_values;
#[path = "world_index.rs"]
mod index;
#[path = "world_intrinsics.rs"]
mod intrinsics;
#[path = "world_methods.rs"]
mod methods;
#[path = "world_places.rs"]
mod places;
#[path = "world_projection.rs"]
mod projection;
#[path = "world_protocol.rs"]
mod protocol;
#[path = "world_protocol_flow.rs"]
mod protocol_flow;
#[path = "world_receiver.rs"]
mod receiver;
#[path = "world_roles.rs"]
mod roles;
#[path = "world_sourcecall.rs"]
mod sourcecall;
#[path = "world_types.rs"]
mod types;
/// Relocation may identify a sealed opcode only by its independently checked
/// original scheme. This authenticates operation correspondence, not W/classes.
pub(super) fn intrinsic_operation_matches(
    kind: &super::original::IntrinsicKind,
    instruction: &InstructionKind,
    left: &mut usize,
) -> R<bool> {
    let count = intrinsics::operand_work(instruction);
    *left = left.checked_sub(count.saturating_add(1)).ok_or_else(|| {
        Diagnostic::limit(Phase::Types, "sealed opcode relocation work exceeded", None)
    })?;
    Ok(intrinsics::operand_ids(kind, instruction).is_ok())
}
fn bad(message: &str) -> Diagnostic {
    error("E-SEMANTIC-WORLD", Span::default(), message)
}
fn pending(message: &str) -> Diagnostic {
    Diagnostic::unsupported(Phase::Types, message, None)
}
fn empty() -> StorageLifeShape {
    StorageLifeShape {
        binder_count: 0,
        slots: Vec::new(),
        retained: Vec::new(),
        nominal_arguments: Vec::new(),
    }
}
struct World<'a, 'b> {
    program: &'a SemanticModule,
    module: &'a TypedModule,
    ledger: &'a Ledger,
    limits: &'b Limits,
    left: &'b mut usize,
    vacant_captures: std::collections::BTreeSet<(SemanticInstanceId, u32)>,
    stale_values: std::collections::BTreeSet<ValueId>,
    stale_locals: std::collections::BTreeSet<LocalId>,
    vacant: std::collections::BTreeSet<(LocalId, Vec<PlaceProjection>)>,
    role_sources: BTreeMap<LocalId, roles::Source>,
    value_role_sources: BTreeMap<ValueId, roles::Source>,
    reborrows: BTreeMap<ValueId, arguments::Edge>,
    value_roots: BTreeMap<ValueId, std::collections::BTreeSet<arguments::Root>>,
    local_roots: BTreeMap<LocalId, std::collections::BTreeSet<arguments::Root>>,
    argument_witnesses: Vec<arguments::Witness>,
    receiver_adaptations: Vec<receiver::Adaptation>,
    index: index::Index,
    types: types::Table,
    snapshots: usize,
    snapshot_work: usize,
    comparisons: usize,
    compare_work: usize,
    current_point: Option<(
        SemanticInstanceId,
        FunctionId,
        BlockId,
        usize,
        std::mem::Discriminant<InstructionKind>,
    )>,
}
impl<'a> World<'a, '_> {
    fn pay(&mut self, n: usize) -> R<()> {
        *self.left = self.left.checked_sub(n).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "original world class-flow work exceeded",
                None,
            )
        })?;
        Ok(())
    }
    fn instance(&mut self, id: SemanticInstanceId) -> R<&'a SemanticInstance> {
        self.pay(index::search(self.index.instances.len()))?;
        if let Some(offset) = self.index.instances.get(&id) {
            return Ok(&self.program.instances[*offset]);
        }
        let ledger = self.ledger;
        if let Some(source) = &ledger.frozen {
            self.pay(index::search(self.index.frozen_instances.len()))?;
            if let Some(offset) = self.index.frozen_instances.get(&id) {
                return Ok(&source.semantic.instances[*offset]);
            }
        }
        Err(bad(
            "world has no actual instance in the current or frozen source domain",
        ))
    }
    fn definition(&mut self, id: SemanticDefinitionId) -> R<&'a SemanticDefinition> {
        self.pay(index::search(self.index.definitions.len()))?;
        if let Some(offset) = self.index.definitions.get(&id) {
            return Ok(&self.program.definitions[*offset]);
        }
        let ledger = self.ledger;
        if let Some(source) = &ledger.frozen {
            self.pay(index::search(self.index.frozen_definitions.len()))?;
            if let Some(offset) = self.index.frozen_definitions.get(&id) {
                return Ok(&source.semantic.definitions[*offset]);
            }
        }
        Err(bad(
            "world has no original definition in the current or frozen source domain",
        ))
    }
    fn function(&mut self, id: FunctionId) -> R<&'a TypedFunction> {
        self.pay(index::search(self.index.functions.len()))?;
        self.index
            .functions
            .get(&id)
            .map(|offset| &self.module.functions[*offset])
            .ok_or_else(|| bad("world has no actual function"))
    }
    fn child(&mut self, id: ClosureId) -> R<(&'a ClosureDefinition, &'a SemanticInstance)> {
        self.pay(
            self.module
                .closures
                .len()
                .saturating_add(self.program.instances.len()),
        )?;
        let d = self
            .module
            .closures
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| bad("actual closure is absent"))?;
        let mut children = self.program.instances.iter().filter(|i| matches!(i.target, SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(f)) if f == d.function));
        let child = children
            .next()
            .ok_or_else(|| bad("actual closure has no original child world"))?;
        if children.next().is_some() {
            return Err(bad("actual closure has ambiguous original child worlds"));
        }
        Ok((d, child))
    }
    fn copy(&mut self, value: &SemanticTypeValue) -> R<SemanticTypeValue> {
        crate::dyn_metadata::semantic_operand_snapshot(value, self.limits, self.left, 1)?;
        Ok(value.clone())
    }
    fn resolved(
        &mut self,
        ty: &Type,
        life: StorageLifeShape,
        classes: Vec<SemanticClassExpr>,
    ) -> R<SemanticTypeValue> {
        inference::charge_type_snapshot(ty, Span::default(), self.limits, self.left, 1)?;
        if life.binder_count as usize != classes.len() {
            return Err(bad("whole type loses its intrinsic class count"));
        }
        Ok(SemanticTypeValue::Resolved {
            ty: ty.clone(),
            life,
            classes,
        })
    }
    fn zero(&mut self, ty: &Type) -> R<SemanticTypeValue> {
        self.zero_at(ty, 0)
    }
    fn zero_at(&mut self, ty: &Type, depth: usize) -> R<SemanticTypeValue> {
        self.pay(1)?;
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "whole-type zero-domain depth exceeded",
                None,
            ));
        }
        match ty {
            ty if ty.is_b0_scalar() => self.resolved(ty, empty(), Vec::new()),
            Type::Record { .. } | Type::Package { .. } | Type::Enum { .. } => {
                stored::ordinary_zero_type(self.module, ty, self.limits, self.left)?;
                self.resolved(ty, empty(), Vec::new())
            }
            Type::ByteBuffer => self.resolved(ty, empty(), Vec::new()),
            Type::Task { .. } | Type::MutexGuard { .. } => {
                let shape = projection::type_shape_at(self, ty, depth + 1)?;
                if shape.binder_count != 0 {
                    return Err(bad(
                        "actual zero carrier retains a nonzero complete class domain",
                    ));
                }
                self.resolved(ty, shape, Vec::new())
            }
            Type::FunctionPointer { parameters, result } => {
                for parameter in parameters {
                    self.zero_at(parameter, depth + 1)?;
                }
                self.zero_at(result, depth + 1)?;
                self.resolved(ty, empty(), Vec::new())
            }
            Type::Managed(inner)
            | Type::Mutex { payload: inner }
            | Type::OwnedStore { element: inner } => {
                self.zero_at(inner, depth + 1)?;
                self.resolved(ty, empty(), Vec::new())
            }
            Type::ErrorUnion { error, result } => {
                self.zero_at(error, depth + 1)?;
                self.zero_at(result, depth + 1)?;
                self.resolved(ty, empty(), Vec::new())
            }
            Type::Tuple(fields) => {
                for field in fields {
                    self.zero_at(field, depth + 1)?;
                }
                self.resolved(ty, empty(), Vec::new())
            }
            Type::Array { element, .. } | Type::Slice { element } => {
                self.zero_at(element, depth + 1)?;
                self.resolved(ty, empty(), Vec::new())
            }
            _ => Err(pending(
                "actual zero-domain type requires its complete original whole-type equation",
            )),
        }
    }
    fn symbolic(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticTypeRecipe,
    ) -> R<SemanticTypeRecipe> {
        super::materialize::symbolic_at(
            self.program,
            self.module,
            self.ledger,
            scope,
            recipe,
            self.limits,
            self.left,
        )
    }
    fn emitted_value(
        &mut self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        block: BlockId,
        index: usize,
        result: ValueId,
    ) -> R<Option<SemanticTypeValue>> {
        let ledger = self.ledger;
        let frame = ledger
            .frames
            .get(&function.id)
            .ok_or_else(|| bad("actual result has no private emission owner"))?;
        self.pay(frame.expressions.len())?;
        let original = self.definition(instance.definition)?;
        let mut selected = None;
        for expression in frame.expressions.values() {
            self.pay(expression.instructions.len())?;
            if expression.value != Some(result)
                || !expression
                    .instructions
                    .iter()
                    .any(|point| point.block == block && point.instruction == index)
            {
                continue;
            }
            let Some(end) = expression.end else {
                continue;
            };
            self.pay(self.program.sites.len())?;
            for site in &self.program.sites {
                if site.instance != instance.id
                    || !matches!(site.at, SemanticSiteAddress::Typed {
                    block: actual_block, instruction, edge: SemanticSiteEdge::Before,
                } if actual_block == end.block && instruction as usize == end.instruction)
                {
                    continue;
                }
                let SemanticSiteAction::TypeUse {
                    subject: SemanticOperand::Value(actual),
                    ..
                } = &site.action
                else {
                    continue;
                };
                if *actual != result {
                    continue;
                }
                let SemanticSiteRecipeKind::TypeUse { expected } =
                    &self.original_recipe(original, site.recipe)?.kind
                else {
                    return Err(bad(
                        "emitted result changes its original type-use operation",
                    ));
                };
                let value = self.ty(
                    instance.id,
                    expected,
                    function
                        .values
                        .get(result.0 as usize)
                        .ok_or_else(|| bad("emitted result type is absent"))?,
                )?;
                if let Some(old) = &selected {
                    self.equal(old, &value)?;
                }
                selected = Some(value);
            }
        }
        Ok(selected)
    }
    fn field_value(&mut self, parent: &SemanticTypeValue, index: u32) -> R<SemanticTypeValue> {
        projection::field(self, parent, index)
    }
    fn ty(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticTypeRecipe,
        hint: &Type,
    ) -> R<SemanticTypeValue> {
        let (key, cached) = self
            .types
            .lookup(scope, recipe, hint, self.limits, self.left)?;
        if let Some(value) = cached {
            return Ok(value);
        }
        let before = *self.left;
        let value = super::materialize::type_at(
            self.program,
            self.module,
            self.ledger,
            scope,
            recipe,
            Some(hint),
            self.limits,
            self.left,
        );
        self.types.evaluation_work = self
            .types
            .evaluation_work
            .saturating_add(before.saturating_sub(*self.left));
        let value = value?;
        self.types
            .insert(key, recipe, hint, &value, self.limits, self.left)?;
        Ok(value)
    }
    fn parameter_value(
        &mut self,
        scope: SemanticInstanceId,
        ordinal: usize,
        hint: &Type,
    ) -> R<SemanticTypeValue> {
        self.ty(
            scope,
            &SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source: SemanticOriginalTypeSource::Parameter(ordinal as u32),
            },
            hint,
        )
    }
    fn result_value(&mut self, scope: SemanticInstanceId, hint: &Type) -> R<SemanticTypeValue> {
        self.ty(
            scope,
            &SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source: SemanticOriginalTypeSource::Result,
            },
            hint,
        )
    }
    fn type_of(value: &SemanticTypeValue) -> R<&Type> {
        match value {
            SemanticTypeValue::Resolved { ty, .. } => Ok(ty),
            _ => Err(bad("Core type survives world materialization")),
        }
    }
    fn get(&mut self, values: &[Option<SemanticTypeValue>], id: ValueId) -> R<SemanticTypeValue> {
        if self.stale_values.contains(&id) {
            return Err(bad(
                "loop-carried whole value requires a distinct live occurrence correspondence",
            ));
        }
        let value = values
            .get(id.0 as usize)
            .and_then(|v| v.as_ref())
            .ok_or_else(|| bad("SSA has no reaching whole-type equation"))?;
        self.copy(value)
    }
    fn local(&mut self, locals: &[Option<SemanticTypeValue>], id: LocalId) -> R<SemanticTypeValue> {
        self.pay(self.vacant.len())?;
        if self.vacant.iter().any(|(local, _)| *local == id) {
            return Err(bad("whole local reads an uninitialized projected occupant"));
        }
        if self.stale_locals.contains(&id) {
            return Err(bad(
                "loop-carried occupant requires a distinct live occurrence correspondence",
            ));
        }
        let value = locals
            .get(id.0 as usize)
            .and_then(|v| v.as_ref())
            .ok_or_else(|| bad("local has no initialized whole-type occupant"))?;
        self.copy(value)
    }
    fn clear_vacant(&mut self, local: LocalId) -> R<()> {
        self.pay(self.vacant.len())?;
        self.vacant.retain(|(owner, _)| *owner != local);
        Ok(())
    }
    fn projected_local(
        &mut self,
        locals: &[Option<SemanticTypeValue>],
        local: LocalId,
        path: &[PlaceProjection],
    ) -> R<SemanticTypeValue> {
        if path.len() > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "class projection depth exceeded",
                None,
            ));
        }
        self.pay(
            self.vacant
                .len()
                .saturating_mul(path.len().saturating_add(1)),
        )?;
        if self.vacant.iter().any(|(owner, vacant)| {
            *owner == local && (vacant.starts_with(path) || path.starts_with(vacant))
        }) {
            return Err(bad("projected local reads an uninitialized occupant"));
        }
        if self.stale_locals.contains(&local) {
            return Err(bad(
                "projected loop carrier lacks its distinct occurrence correspondence",
            ));
        }
        let value = locals
            .get(local.0 as usize)
            .and_then(|value| value.as_ref())
            .ok_or_else(|| bad("projected local has no initialized shell"))?;
        let mut value = self.copy(value)?;
        for selector in path {
            self.pay(1)?;
            value = projection::place(self, &value, *selector)?;
        }
        Ok(value)
    }
    fn equal(&mut self, a: &SemanticTypeValue, b: &SemanticTypeValue) -> R<()> {
        crate::dyn_metadata::semantic_operand_snapshot(a, self.limits, self.left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(b, self.limits, self.left, 1)?;
        if a != b {
            return Err(bad("actual whole type or rigid original classes disagree"));
        }
        Ok(())
    }
    fn binding(
        &mut self,
        instance: &SemanticInstance,
        binder: StorageBinder,
    ) -> R<SemanticClassExpr> {
        self.pay(instance.header_embedding.len())?;
        let mut rows = instance
            .header_embedding
            .iter()
            .filter(|r| r.header == binder);
        let row = rows
            .next()
            .ok_or_else(|| bad("actual header lacks its original class image"))?;
        if rows.next().is_some() {
            return Err(bad("actual header repeats an original class image"));
        }
        crate::dyn_metadata::semantic_operand_snapshot(&row.actual, self.limits, self.left, 1)?;
        Ok(row.actual.clone())
    }
    fn capture(&mut self, instance: &SemanticInstance, index: u32) -> R<SemanticTypeValue> {
        self.pay(self.vacant_captures.len().saturating_add(1))?;
        if self.vacant_captures.contains(&(instance.id, index)) {
            return Err(bad("actual capture reads a transferred environment field"));
        }
        self.capture_initial(instance, index)
    }
    fn capture_initial(&mut self, instance: &SemanticInstance, index: u32) -> R<SemanticTypeValue> {
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) =
            instance.target
        else {
            return Err(bad("capture is outside an actual function"));
        };
        let f = self.function(function)?;
        let id = f
            .environment
            .ok_or_else(|| bad("capture has no actual environment"))?;
        let (d, child) = self.child(id)?;
        if child.id != instance.id {
            return Err(bad("capture uses another child world"));
        }
        let ty = d
            .captures
            .get(index as usize)
            .ok_or_else(|| bad("capture index is outside its actual directory"))?;
        let header = d
            .storage_life
            .as_ref()
            .ok_or_else(|| pending("capture complete domain remains unknown"))?;
        if f.storage_life.as_ref() != Some(header) {
            return Err(bad("entry and environment class directories disagree"));
        }
        projection::capture(self, instance, index, ty, header)
    }
    fn address(
        &mut self,
        ty: &Type,
        image: SemanticClassExpr,
        pointee: &SemanticTypeValue,
    ) -> R<SemanticTypeValue> {
        let (inner, mode) = match ty {
            Type::Address { mutable, inner } => (
                inner,
                if *mutable {
                    StoredBorrowMode::ExclusiveAddress
                } else {
                    StoredBorrowMode::SharedAddress
                },
            ),
            Type::Desc(inner) => (inner, StoredBorrowMode::WritableDescription),
            _ => return Err(bad("borrow producer changes its actual constructor")),
        };
        let SemanticTypeValue::Resolved {
            ty: expected,
            life: child,
            classes,
        } = pointee
        else {
            return Err(bad("borrow producer retains an unresolved pointee"));
        };
        if inner.as_ref() != expected {
            return Err(bad("address producer changes its actual pointee type"));
        }
        self.pay(classes.len().saturating_mul(2))?;
        let mut images = vec![image];
        images.extend(classes.iter().cloned());
        self.resolved(
            ty,
            StorageLifeShape {
                binder_count: child
                    .binder_count
                    .checked_add(1)
                    .ok_or_else(|| bad("address domain overflows"))?,
                slots: vec![StorageLifeSlot {
                    path: Vec::new(),
                    mode,
                    binder: 0,
                }],
                retained: (0..child.binder_count)
                    .map(|class| StorageRetainedBinding {
                        path: vec![LifeTypeProjection::Pointee],
                        class,
                        binder: class + 1,
                    })
                    .collect(),
                nominal_arguments: Vec::new(),
            },
            images,
        )
    }
    fn address_projection(
        &mut self,
        ty: &Type,
        parent: &SemanticTypeValue,
        child: &SemanticTypeValue,
    ) -> R<SemanticTypeValue> {
        let SemanticTypeValue::Resolved { life, classes, .. } = parent else {
            return Err(bad("address projection has an unresolved parent"));
        };
        self.pay(life.slots.len())?;
        let mut outer = life.slots.iter().filter(|slot| slot.path.is_empty());
        if let Some(slot) = outer.next() {
            if outer.next().is_some() {
                return Err(bad("address projection repeats its outer occurrence"));
            }
            let image = classes
                .get(slot.binder as usize)
                .ok_or_else(|| bad("address projection loses its outer class"))?;
            crate::dyn_metadata::semantic_operand_snapshot(image, self.limits, self.left, 1)?;
            return self.address(ty, image.clone(), child);
        }
        let (
            Type::Address {
                mutable: false,
                inner,
            },
            SemanticTypeValue::Resolved {
                ty: child_ty,
                life,
                classes,
            },
        ) = (ty, child)
        else {
            return Err(bad(
                "static address projection changes its exact outer mode",
            ));
        };
        if inner.as_ref() != child_ty {
            return Err(bad("static projection changes its complete child type"));
        }
        self.pay(classes.len().saturating_mul(2))?;
        self.resolved(
            ty,
            StorageLifeShape {
                binder_count: life.binder_count,
                retained: (0..life.binder_count)
                    .map(|class| StorageRetainedBinding {
                        path: vec![LifeTypeProjection::Pointee],
                        class,
                        binder: class,
                    })
                    .collect(),
                ..empty()
            },
            classes.clone(),
        )
    }
    fn pointee(&mut self, value: &SemanticTypeValue) -> R<SemanticTypeValue> {
        if !matches!(Self::type_of(value)?, Type::Address { .. } | Type::Desc(_)) {
            return Err(bad(
                "pointee projection has no actual address or description",
            ));
        }
        projection::pointee(self, value)
    }
    fn map_value(
        &mut self,
        value: &SemanticTypeValue,
        actuals: &[SemanticActual],
        environment: Option<(&StorageLifeContract, &SemanticInstance, &SemanticTypeValue)>,
    ) -> R<SemanticTypeValue> {
        self.map_call_value(value, actuals, environment, &[])
    }
    fn map_call_value(
        &mut self,
        value: &SemanticTypeValue,
        actuals: &[SemanticActual],
        environment: Option<(&StorageLifeContract, &SemanticInstance, &SemanticTypeValue)>,
        invocation: &[(SemanticClassExpr, SemanticClassExpr)],
    ) -> R<SemanticTypeValue> {
        let mut value = self.copy(value)?;
        let SemanticTypeValue::Resolved { classes, .. } = &mut value else {
            return Err(bad("Core type survives call interpretation"));
        };
        for image in classes {
            self.pay(invocation.len())?;
            let mut mapped = invocation.iter().filter(|(source, _)| source == image);
            if let Some((_, actual)) = mapped.next() {
                if mapped.next().is_some() {
                    return Err(bad(
                        "one actual invocation class has repeated boundary images",
                    ));
                }
                crate::dyn_metadata::semantic_operand_snapshot(actual, self.limits, self.left, 1)?;
                *image = actual.clone();
                continue;
            }
            if matches!(
                image,
                SemanticClassExpr::Implicit {
                    scope: SemanticScopeRef::Ancestor(0),
                    ..
                }
            ) {
                if let Some((
                    header,
                    child,
                    SemanticTypeValue::Resolved {
                        classes: actual_images,
                        ..
                    },
                )) = environment
                {
                    self.pay(child.header_embedding.len())?;
                    let mut rows = child.header_embedding.iter().filter(|row| {
                        row.actual == *image && matches!(row.header, StorageBinder::Fixed(_))
                    });
                    if let Some(row) = rows.next() {
                        if rows.next().is_some() {
                            return Err(bad("implicit capture has ambiguous actual Fixed images"));
                        }
                        let StorageBinder::Fixed(fixed) = row.header else {
                            unreachable!()
                        };
                        if fixed >= header.fixed_binder_count {
                            return Err(bad(
                                "implicit capture Fixed image exceeds complete environment domain",
                            ));
                        }
                        let actual = actual_images
                            .get(fixed as usize)
                            .ok_or_else(|| bad("call omits an implicit captured Fixed image"))?;
                        crate::dyn_metadata::semantic_operand_snapshot(
                            actual,
                            self.limits,
                            self.left,
                            1,
                        )?;
                        *image = actual.clone();
                        continue;
                    }
                }
            }
            match image {
                SemanticClassExpr::Formal {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal,
                } => {
                    let Some(SemanticActual::Life(actual)) = actuals.get(*ordinal as usize) else {
                        return Err(bad("call omits an original lifetime actual"));
                    };
                    crate::dyn_metadata::semantic_operand_snapshot(
                        actual,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    *image = actual.clone();
                }
                SemanticClassExpr::IntrinsicClass {
                    operand:
                        SemanticOperand::TypeArgument {
                            scope: SemanticScopeRef::Ancestor(0),
                            ordinal,
                        },
                    path,
                    class,
                } if path.is_empty() => {
                    let Some(SemanticActual::Type(SemanticTypeValue::Resolved { classes, .. })) =
                        actuals.get(*ordinal as usize)
                    else {
                        return Err(bad("call omits a whole-Type actual"));
                    };
                    let actual = classes
                        .get(*class as usize)
                        .ok_or_else(|| bad("call truncates the whole-Type actual domain"))?;
                    crate::dyn_metadata::semantic_operand_snapshot(
                        actual,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    *image = actual.clone();
                }
                SemanticClassExpr::Environment { .. }
                | SemanticClassExpr::IntrinsicClass {
                    operand: SemanticOperand::Environment(_),
                    ..
                } => {
                    let Some((
                        header,
                        child,
                        SemanticTypeValue::Resolved {
                            classes: actual_images,
                            ..
                        },
                    )) = environment
                    else {
                        return Err(bad("call reads an unsealed child environment class"));
                    };
                    let mut selected = None;
                    for row in &child.header_embedding {
                        self.pay(1)?;
                        if row.actual == *image {
                            let StorageBinder::Fixed(fixed) = row.header else {
                                continue;
                            };
                            if fixed >= header.fixed_binder_count
                                || selected.replace(fixed).is_some()
                            {
                                return Err(bad("environment class has ambiguous Fixed images"));
                            }
                        }
                    }
                    *image = actual_images
                        .get(
                            selected
                                .ok_or_else(|| bad("environment class has no actual Fixed image"))?
                                as usize,
                        )
                        .ok_or_else(|| bad("call truncates an environment's complete domain"))?
                        .clone();
                }
                _ => {
                    crate::dyn_metadata::semantic_operand_snapshot(
                        image,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    self.pay(128)?;
                    let mut diagnostic = pending(
                        "call class requires its complete scoped invocation/outer relation",
                    );
                    diagnostic.notes.push(format!("unmapped original call class: {image:?}; mixed actual positions: {}; sealed environment: {}",actuals.len(),environment.is_some()));
                    return Err(diagnostic);
                }
            }
        }
        Ok(value)
    }
    fn occurrence(&mut self, f: &TypedFunction, value: ValueId) -> R<SemanticClassExpr> {
        let ty = f
            .values
            .get(value.0 as usize)
            .ok_or_else(|| bad("borrow result lacks a type"))?;
        let mode = match ty {
            Type::Address { mutable: true, .. } => StoredBorrowMode::ExclusiveAddress,
            Type::Address { mutable: false, .. } => StoredBorrowMode::SharedAddress,
            Type::Desc(_) => StoredBorrowMode::WritableDescription,
            _ => {
                return Err(bad(
                    "borrow producer result is not an address or description",
                ));
            }
        };
        Ok(SemanticClassExpr::OuterOccurrence {
            operand: SemanticOperand::Value(value),
            path: Vec::new(),
            mode,
        })
    }
}

impl World<'_, '_> {
    fn class(
        &mut self,
        scope: SemanticInstanceId,
        image: &SemanticClassExpr,
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<SemanticClassExpr> {
        crate::dyn_metadata::semantic_operand_snapshot(image, self.limits, self.left, 1)?;
        Ok(match image {
            SemanticClassExpr::Implicit {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            } => match implicit.get(ordinal) {
                Some(actual) => {
                    crate::dyn_metadata::semantic_operand_snapshot(
                        actual,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    actual.clone()
                }
                None => image.clone(),
            },
            SemanticClassExpr::Formal {
                scope: relative,
                ordinal,
            } => super::materialize::class_at(
                self.program,
                self.module,
                self.ledger,
                scope,
                &SemanticLifeRecipe::Formal(SemanticFormalRef {
                    scope: *relative,
                    ordinal: *ordinal,
                }),
                self.limits,
                self.left,
            )?,
            SemanticClassExpr::OuterOccurrence {
                operand: SemanticOperand::Environment(ordinal),
                path,
                mode,
            } => roles::environment_outer(self, scope, *ordinal, path, *mode)?,
            _ => image.clone(),
        })
    }
    fn same_class(
        &mut self,
        scope: SemanticInstanceId,
        a: &SemanticClassExpr,
        b: &SemanticClassExpr,
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<()> {
        let a = self.class(scope, a, implicit)?;
        let b = self.class(scope, b, implicit)?;
        if a != b {
            return Err(bad("rigid original class images disagree"));
        }
        Ok(())
    }
    fn normalize(
        &mut self,
        scope: SemanticInstanceId,
        value: &SemanticTypeValue,
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<SemanticTypeValue> {
        let mut value = self.copy(value)?;
        let SemanticTypeValue::Resolved { classes, .. } = &mut value else {
            return Err(bad("Core value survives normalization"));
        };
        for image in classes {
            *image = self.class(scope, image, implicit)?;
        }
        Ok(value)
    }
    fn same(
        &mut self,
        scope: SemanticInstanceId,
        a: &SemanticTypeValue,
        b: &SemanticTypeValue,
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<()> {
        let a = self.normalize(scope, a, implicit)?;
        let b = self.normalize(scope, b, implicit)?;
        self.equal(&a, &b)
    }
    fn original_recipe<'a>(
        &mut self,
        original: &'a SemanticDefinition,
        id: SemanticSiteId,
    ) -> R<&'a SemanticSiteRecipe> {
        let (SemanticDefinitionBody::Function { sites, .. }
        | SemanticDefinitionBody::FunctionWithBody { sites, .. }) = &original.body
        else {
            return Err(bad("operation is outside its original function"));
        };
        self.pay(sites.len())?;
        sites
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| bad("actual operation has no original recipe"))
    }
    fn point_sites<'a>(
        &mut self,
        sites: &'a [&'a SemanticSite],
        block: BlockId,
        instruction: usize,
    ) -> R<Vec<&'a SemanticSite>> {
        let Some(first) = sites.first() else {
            return Ok(Vec::new());
        };
        self.pay(index::search(self.index.points.len()))?;
        let Some(positions) = self.index.points.get(&(first.instance, block, instruction)) else {
            return Ok(Vec::new());
        };
        index::pay(self.left, positions.len())?;
        let mut result = Vec::with_capacity(positions.len());
        for position in positions {
            let row = sites
                .get(*position)
                .ok_or_else(|| bad("actual point index has another instance site directory"))?;
            if row.instance != first.instance
                || !matches!(row.at, SemanticSiteAddress::Typed { block: b, instruction: i, .. } if b == block && i as usize == instruction)
            {
                return Err(bad(
                    "actual point index changes its exact instance or instruction",
                ));
            }
            result.push(*row);
        }
        Ok(result)
    }
    fn constraints(
        &mut self,
        instance: &SemanticInstance,
        original: &SemanticDefinition,
        function: &TypedFunction,
        sites: &[&SemanticSite],
        block: BlockId,
        instruction: usize,
        edge: SemanticSiteEdge,
        values: &[Option<SemanticTypeValue>],
        locals: &[Option<SemanticTypeValue>],
        declared: &mut BTreeMap<LocalId, SemanticTypeValue>,
        implicit: &mut BTreeMap<u32, SemanticClassExpr>,
    ) -> R<()> {
        for site in self.point_sites(sites, block, instruction)? {
            if !matches!(site.at, SemanticSiteAddress::Typed { edge: actual, .. } if actual == edge)
            {
                continue;
            }
            let recipe = self.original_recipe(original, site.recipe)?;
            match (&site.action, &recipe.kind) {
                (
                    SemanticSiteAction::DeclareLocal {
                        local: SemanticLocalTarget::Typed(local),
                        expected,
                    },
                    SemanticSiteRecipeKind::DeclareLocal { expected: recipe },
                ) => {
                    let ty = &function
                        .locals
                        .get(local.0 as usize)
                        .ok_or_else(|| bad("declaration local is absent"))?
                        .ty;
                    let actual = self.ty(instance.id, recipe, ty)?;
                    self.same(instance.id, expected, &actual, implicit)?;
                    if let Some(old) = declared.insert(*local, actual) {
                        self.same(instance.id, &old, expected, implicit)?;
                    }
                }
                (
                    SemanticSiteAction::TypeUse { subject, expected },
                    SemanticSiteRecipeKind::TypeUse {
                        expected: recipe_ty,
                    },
                ) => {
                    let value = match subject {
                        SemanticOperand::Value(value) => self.get(values, *value)?,
                        SemanticOperand::Local(local) => self.local(locals, *local)?,
                        SemanticOperand::Parameter(parameter) => self.local(
                            locals,
                            *function
                                .parameters
                                .get(*parameter as usize)
                                .ok_or_else(|| {
                                    bad("type-use parameter has no exact actual input local")
                                })?,
                        )?,
                        SemanticOperand::Capture(index) => self.capture(instance, *index)?,
                        _ => {
                            return Err(pending(
                                "type-use needs its complete actual operand world",
                            ));
                        }
                    };
                    let original_value = self.ty(instance.id, recipe_ty, Self::type_of(&value)?)?;
                    self.same(instance.id, expected, &original_value, implicit)?;
                    // Only the original body's exact implicit occurrence can
                    // name a freshly emitted local borrow. A signature input,
                    // Formal or already sealed environment is never unified.
                    if let (
                        SemanticTypeValue::Resolved {
                            classes: images, ..
                        },
                        SemanticTypeValue::Resolved {
                            classes: actual_images,
                            ..
                        },
                    ) = (&original_value, &value)
                    {
                        self.pay(
                            images
                                .len()
                                .saturating_add(original.implicit_lifetimes.len()),
                        )?;
                        for (expected, actual) in images.iter().zip(actual_images) {
                            if let SemanticClassExpr::Implicit {
                                scope: SemanticScopeRef::Ancestor(0),
                                ordinal,
                            } = expected
                            {
                                if original.implicit_lifetimes.iter().any(|i| {
                                    i.ordinal == *ordinal
                                        && i.occurrence_path == recipe.occurrence_path
                                }) && matches!(
                                    actual,
                                    SemanticClassExpr::OuterOccurrence {
                                        operand: SemanticOperand::Value(_),
                                        ..
                                    }
                                ) {
                                    if let Some(old) = implicit.insert(*ordinal, actual.clone()) {
                                        if old != *actual {
                                            return Err(bad(
                                                "one original occurrence selects different emitted class heads",
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    self.same(instance.id, &value, &original_value, implicit)?;
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn construction_recipe(
        &mut self,
        original: &SemanticDefinition,
        site: SemanticSiteId,
        recipe: &SemanticTypeRecipe,
    ) -> R<SemanticTypeRecipe> {
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &original.body else {
            crate::dyn_metadata::semantic_operand_snapshot(recipe, self.limits, self.left, 1)?;
            return Ok(recipe.clone());
        };
        self.pay(site_nodes.len().saturating_add(1))?;
        let mut nodes = site_nodes.iter().filter(|row| row.site == site);
        let node = nodes.next().map(|row| row.node);
        if nodes.next().is_some() {
            return Err(bad("constructor has ambiguous original source occurrences"));
        }
        if let Some(SemanticOriginalNode::Expression(node)) = node {
            let ledger = self.ledger;
            let proof = ledger.original.borrow();
            let proof = proof
                .as_ref()
                .ok_or_else(|| bad("constructor has no checked original injection theorem"))?;
            self.pay(proof.injections.len())?;
            if let Some(injection) = proof.injections.get(&(original.id, node)) {
                // The actual root is emitted before this expression's implicit
                // ErrorUnion injection. Its payload is an original Gamma
                // result, never reconstructed from the actual result type.
                if !matches!(recipe,SemanticTypeRecipe::OriginalType{scope:SemanticScopeRef::Ancestor(0),source:SemanticOriginalTypeSource::Expression(at)} if *at==node)
                {
                    return Err(bad(
                        "constructor injection changes its exact original expression query",
                    ));
                }
                crate::dyn_metadata::semantic_operand_snapshot(
                    &injection.payload,
                    self.limits,
                    self.left,
                    1,
                )?;
                return Ok(injection.payload.clone());
            }
        }
        crate::dyn_metadata::semantic_operand_snapshot(recipe, self.limits, self.left, 1)?;
        Ok(recipe.clone())
    }
    fn constructed_value(
        &mut self,
        instance: &SemanticInstance,
        original: &SemanticDefinition,
        sites: &[&SemanticSite],
        block: BlockId,
        index: usize,
        result: ValueId,
        ty: &Type,
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<Option<SemanticTypeValue>> {
        let mut selected = None;
        for site in self.point_sites(sites, block, index)? {
            let SemanticSiteAction::Construct {
                result: SemanticOperand::Value(actual),
                value: claimed,
            } = &site.action
            else {
                continue;
            };
            if *actual != result || selected.is_some() {
                return Err(bad("constructor repeats or changes its actual result"));
            }
            let SemanticSiteRecipeKind::Construct { result: recipe } =
                &self.original_recipe(original, site.recipe)?.kind
            else {
                return Err(bad("constructor changes its original recipe kind"));
            };
            let recipe = self.construction_recipe(original, site.recipe, recipe)?;
            let expected = self.ty(instance.id, &recipe, ty)?;
            self.same(instance.id, claimed, &expected, implicit)?;
            selected = Some(expected);
        }
        Ok(selected)
    }
    fn constructor(
        &mut self,
        instance: &SemanticInstance,
        original: &SemanticDefinition,
        f: &TypedFunction,
        sites: &[&SemanticSite],
        block: BlockId,
        index: usize,
        result: ValueId,
        variant: Option<u32>,
        fields: &[ValueId],
        values: &[Option<SemanticTypeValue>],
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<SemanticTypeValue> {
        let mut selected = None;
        for site in self.point_sites(sites, block, index)? {
            if let SemanticSiteAction::Construct {
                result: SemanticOperand::Value(value),
                value: claimed,
            } = &site.action
            {
                if *value != result || selected.is_some() {
                    return Err(bad(
                        "constructor selects a different or repeated actual result",
                    ));
                }
                let recipe = self.original_recipe(original, site.recipe)?;
                let SemanticSiteRecipeKind::Construct { result: recipe } = &recipe.kind else {
                    return Err(bad("constructor changes original recipe kind"));
                };
                let recipe = self.construction_recipe(original, site.recipe, recipe)?;
                let expected = self.ty(instance.id, &recipe, &f.values[result.0 as usize])?;
                self.same(instance.id, claimed, &expected, implicit)?;
                let symbolic = self.symbolic(instance.id, &recipe)?;
                let symbolic = match &symbolic {
                    SemanticTypeRecipe::Managed(inner) | SemanticTypeRecipe::Desc { inner, .. } => {
                        inner.as_ref()
                    }
                    other => other,
                };
                let SemanticTypeRecipe::Nominal {
                    definition,
                    arguments,
                } = symbolic
                else {
                    return Err(pending(
                        "constructor needs its original compound field instantiation",
                    ));
                };
                let nominal = self.definition(*definition)?;
                let SemanticDefinitionBody::Nominal { body, .. } = &nominal.body else {
                    return Err(bad(
                        "constructor target is not its original nominal declaration",
                    ));
                };
                let physical = stored::physical(Self::type_of(&expected)?);
                let physical = if let Type::Managed(payload) = physical {
                    stored::physical(payload)
                } else {
                    physical
                };
                match (body, physical, variant) {
                    (
                        SemanticNominalBody::Product {
                            fields: original_fields,
                        },
                        Type::Record {
                            fields: actual_fields,
                            ..
                        }
                        | Type::Package {
                            fields: actual_fields,
                            ..
                        },
                        None,
                    ) => {
                        if fields.len() != original_fields.len()
                            || actual_fields.len() != original_fields.len()
                        {
                            return Err(bad("constructor omits original physical fields"));
                        }
                        self.pay(fields.len())?;
                        for ((field, recipe), (name, ty)) in
                            fields.iter().zip(original_fields).zip(actual_fields)
                        {
                            if name != &recipe.name {
                                return Err(bad("constructor changes original field identity"));
                            }
                            let recipe = {
                                let ledger = self.ledger;
                                let proof = ledger.original.borrow();
                                let proof = proof.as_ref().ok_or_else(|| {
                                    bad("nominal construction has no checked original domain proof")
                                })?;
                                super::instantiate::ty(
                                    self.program,
                                    proof,
                                    &recipe.ty,
                                    super::instantiate::DomainMapping {
                                        source: nominal.id,
                                        use_definition: instance.definition,
                                        arguments,
                                        parent_call: None,
                                    },
                                    self.limits,
                                    self.left,
                                )?
                            };
                            let expected_field = self.ty(instance.id, &recipe, ty)?;
                            let value = self.get(values, *field)?;
                            self.same(instance.id, &value, &expected_field, implicit)?;
                        }
                    }
                    (
                        SemanticNominalBody::Enum { variants },
                        Type::Enum {
                            variants: actual, ..
                        },
                        Some(variant),
                    ) => {
                        if variants.len() != actual.len() {
                            return Err(bad(
                                "enum constructor changes its complete original variants",
                            ));
                        }
                        let recipe = variants
                            .get(variant as usize)
                            .ok_or_else(|| bad("constructor selects an absent original variant"))?;
                        let (name, actual_fields) = actual
                            .get(variant as usize)
                            .ok_or_else(|| bad("constructor selects an absent actual variant"))?;
                        if name != &recipe.name
                            || fields.len() != recipe.fields.len()
                            || actual_fields.len() != fields.len()
                        {
                            return Err(bad(
                                "constructor changes its exact selected original variant",
                            ));
                        }
                        self.pay(fields.len())?;
                        for ((field, recipe), ty) in
                            fields.iter().zip(&recipe.fields).zip(actual_fields)
                        {
                            let recipe = {
                                let ledger = self.ledger;
                                let proof = ledger.original.borrow();
                                let proof = proof.as_ref().ok_or_else(|| {
                                    bad("variant construction has no checked original domain proof")
                                })?;
                                super::instantiate::ty(
                                    self.program,
                                    proof,
                                    recipe,
                                    super::instantiate::DomainMapping {
                                        source: nominal.id,
                                        use_definition: instance.definition,
                                        arguments,
                                        parent_call: None,
                                    },
                                    self.limits,
                                    self.left,
                                )?
                            };
                            let expected_field = self.ty(instance.id, &recipe, ty)?;
                            let value = self.get(values, *field)?;
                            self.same(instance.id, &value, &expected_field, implicit)?;
                        }
                    }
                    _ => {
                        return Err(bad(
                            "constructor changes its original nominal kind or selected variant",
                        ));
                    }
                }
                selected = Some(expected);
            }
        }
        if selected.is_none() {
            self.pay(128)?;
        }
        selected.ok_or_else(|| {
            let mut error = bad("actual qualified constructor has no original constructor site");
            error.notes.push(format!(
                "actual constructor: function {:?} block {:?} instruction {} result {:?}",
                f.id, block, index, result
            ));
            error
        })
    }
    fn body_call(
        &mut self,
        instance: &SemanticInstance,
        original: &SemanticDefinition,
        site: &SemanticSite,
        actuals: &[SemanticActual],
        target: SemanticDefinitionId,
    ) -> R<()> {
        if !matches!(
            self.original_recipe(original, site.recipe)?.kind,
            SemanticSiteRecipeKind::OriginalCall
        ) {
            return Err(bad("actual body call changes original operation kind"));
        }
        let recipes = {
            let proof = self.ledger.original.borrow();
            let proof = proof
                .as_ref()
                .ok_or_else(|| bad("body call lacks complete original Gamma"))?;
            let key = (original.id, site.recipe);
            if proof.call_targets.get(&key) != Some(&target) {
                return Err(bad("actual body call selects another original target"));
            }
            let recipes = proof
                .calls
                .get(&key)
                .ok_or_else(|| bad("body call lacks original fresh actuals"))?;
            if recipes.len() != actuals.len() {
                return Err(bad("body call omits mixed actuals"));
            }
            for recipe in recipes {
                crate::dyn_metadata::semantic_operand_snapshot(recipe, self.limits, self.left, 1)?;
            }
            recipes.clone()
        };
        for (recipe, actual) in recipes.iter().zip(actuals) {
            let expected = super::materialize::actual_at(
                self.program,
                self.module,
                self.ledger,
                instance.id,
                recipe,
                self.limits,
                self.left,
            )?;
            crate::dyn_metadata::semantic_operand_snapshot(actual, self.limits, self.left, 1)?;
            crate::dyn_metadata::semantic_operand_snapshot(&expected, self.limits, self.left, 1)?;
            if actual != &expected {
                return Err(bad("body call actuals differ from original inference"));
            }
        }
        Ok(())
    }
    fn requirements(
        &mut self,
        instance: &SemanticInstance,
        original: &SemanticDefinition,
        actuals: Option<&[SemanticActual]>,
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<()> {
        self.pay(original.requirements.len())?;
        for requirement in &original.requirements {
            match requirement {
                SemanticRequirementRecipe::LifeEqual { left, right } => {
                    if let Some(actuals) = actuals {
                        // No callee equality assumption participates in the
                        // caller's proof of the same premise.
                        let select = |recipe: &SemanticLifeRecipe| -> R<SemanticClassExpr> {
                            let SemanticLifeRecipe::Formal(SemanticFormalRef {
                                scope: SemanticScopeRef::Ancestor(0),
                                ordinal,
                            }) = recipe
                            else {
                                return Err(pending(
                                    "where equality needs its checked lexical selector map",
                                ));
                            };
                            let Some(SemanticActual::Life(image)) = actuals.get(*ordinal as usize)
                            else {
                                return Err(bad("where equality selects a non-Life actual"));
                            };
                            Ok(image.clone())
                        };
                        let a = select(left)?;
                        let b = select(right)?;
                        self.same_class(instance.id, &a, &b, implicit)?;
                    }
                }
                SemanticRequirementRecipe::Callable {
                    ty:
                        SemanticTypeRecipe::OriginalType {
                            scope: SemanticScopeRef::Ancestor(0),
                            source: SemanticOriginalTypeSource::Result,
                        },
                    ..
                } => {
                    let proof = self.ledger.original.borrow();
                    if !proof
                        .as_ref()
                        .is_some_and(|proof| proof.complete.contains(&original.id))
                    {
                        return Err(bad(
                            "result callable goal lacks complete original code/capture proof",
                        ));
                    }
                    // Every real target instance, its complete header and every
                    // operation are independently interpreted by this World.
                }
                _ => {
                    return Err(pending(
                        "original where premise requires its complete rigid-role proof",
                    ));
                }
            }
        }
        Ok(())
    }
}

impl World<'_, '_> {
    fn header_value(
        &mut self,
        value: &SemanticTypeValue,
        base: StorageBase,
        bindings: &mut Vec<(StoragePosition, SemanticClassExpr)>,
        classes_out: &mut Vec<(SignatureLifeClass, SemanticClassExpr)>,
        statics: &mut Vec<StoragePosition>,
    ) -> R<()> {
        super::creator_headers::rows::append(
            value,
            base,
            bindings,
            classes_out,
            statics,
            self.limits,
            self.left,
        )
    }
    fn check_header(
        &mut self,
        instance: &SemanticInstance,
        f: &TypedFunction,
        signature: &SemanticSignatureRecipe,
    ) -> R<()> {
        let mut expected_bindings = Vec::new();
        let mut expected_classes = Vec::new();
        let mut expected_static = Vec::new();
        for (index, (local, _recipe)) in f.parameters.iter().zip(&signature.parameters).enumerate()
        {
            let ty = &f.locals[local.0 as usize].ty;
            let value = self.parameter_value(instance.id, index, ty)?;
            self.header_value(
                &value,
                StorageBase::Parameter(index as u32),
                &mut expected_bindings,
                &mut expected_classes,
                &mut expected_static,
            )?;
        }
        let result = self.result_value(instance.id, &f.return_type)?;
        self.header_value(
            &result,
            StorageBase::Result,
            &mut expected_bindings,
            &mut expected_classes,
            &mut expected_static,
        )?;
        if let Some(closure) = f.environment {
            let (d, child) = self.child(closure)?;
            if child.id != instance.id || d.storage_life != f.storage_life {
                return Err(bad("entry header differs from its real environment"));
            }
            for index in 0..d.captures.len() {
                let value = self.capture(instance, index as u32)?;
                self.header_value(
                    &value,
                    StorageBase::Capture(index as u32),
                    &mut expected_bindings,
                    &mut expected_classes,
                    &mut expected_static,
                )?;
            }
        }
        let Some(header) = &f.storage_life else {
            if expected_bindings.is_empty()
                && expected_classes.is_empty()
                && expected_static.is_empty()
                && instance.header_embedding.is_empty()
            {
                return Ok(());
            }
            return Err(bad("actual nonempty header class directory is unknown"));
        };
        self.pay(
            header
                .bindings
                .len()
                .saturating_add(header.classes.len())
                .saturating_add(instance.header_embedding.len()),
        )?;
        if header.bindings.len() != expected_bindings.len()
            || header.classes.len() != expected_classes.len()
            || header.static_bindings.len() != expected_static.len()
        {
            return Err(bad(
                "actual header omits or adds original class/borrow occurrences",
            ));
        }
        for position in expected_static {
            self.pay(header.static_bindings.len())?;
            if header
                .static_bindings
                .iter()
                .filter(|p| **p == position)
                .count()
                != 1
            {
                return Err(bad("actual header changes a complete static occurrence"));
            }
        }
        for (position, image) in expected_bindings {
            let mut rows = header.bindings.iter().filter(|r| r.position == position);
            let row = rows
                .next()
                .ok_or_else(|| bad("actual header omits an original borrow occurrence"))?;
            if rows.next().is_some() {
                return Err(bad("actual header repeats an original borrow occurrence"));
            }
            let actual = self.binding(instance, row.binder)?;
            self.same_class(instance.id, &actual, &image, &BTreeMap::new())?;
        }
        for (position, image) in expected_classes {
            let mut rows = header.classes.iter().filter(|r| r.position == position);
            let row = rows
                .next()
                .ok_or_else(|| bad("actual header omits an original intrinsic class"))?;
            if rows.next().is_some() {
                return Err(bad("actual header repeats an original intrinsic class"));
            }
            let actual = self.binding(instance, row.binder)?;
            self.same_class(instance.id, &actual, &image, &BTreeMap::new())?;
        }
        let fixed = instance
            .header_embedding
            .iter()
            .filter(|r| matches!(r.header, StorageBinder::Fixed(_)))
            .count();
        let invocation = instance.header_embedding.len() - fixed;
        if fixed != header.fixed_binder_count as usize
            || invocation != header.invocation_binder_count as usize
        {
            return Err(bad(
                "actual header changes its complete original binder domain",
            ));
        }
        Ok(())
    }
    fn invoke(
        &mut self,
        instance: &SemanticInstance,
        original: &SemanticDefinition,
        sites: &[&SemanticSite],
        block: BlockId,
        index: usize,
        operand: SemanticOperand,
        value: &SemanticTypeValue,
        requested: CallableKind,
        arguments: &[ValueId],
        values: &[Option<SemanticTypeValue>],
        locals: &[Option<SemanticTypeValue>],
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<SemanticTypeValue> {
        let raw_value = self.copy(value)?;
        let value = if matches!(Self::type_of(value)?, Type::Address { .. }) {
            self.pointee(value)?
        } else {
            self.copy(value)?
        };
        let Type::Closure(id) = Self::type_of(&value)? else {
            return Err(pending(
                "borrowed callable needs its complete current pointee world",
            ));
        };
        let (d, child) = self.child(*id)?;
        let target = self.function(d.function)?;
        let target_original = self.definition(child.definition)?;
        let (SemanticDefinitionBody::Function { signature, .. }
        | SemanticDefinitionBody::FunctionWithBody { signature, .. }) = &target_original.body
        else {
            return Err(bad("closure has no original code signature"));
        };
        let header = d
            .storage_life
            .as_ref()
            .ok_or_else(|| pending("called closure domain is unknown"))?;
        let mut selected = false;
        let mut checked_result = None;
        for site in self.point_sites(sites, block, index)? {
            if let SemanticSiteAction::BodyCall { actuals } = &site.action {
                if let Some(result) = methods::callable(
                    self, instance, original, site, &raw_value, &value, d, child, target,
                    requested, actuals, arguments, values, locals, implicit,
                )? {
                    if selected {
                        return Err(bad("callable repeats original selected call site"));
                    }
                    checked_result = Some(result);
                    selected = true;
                    continue;
                }
                if selected {
                    return Err(bad("callable has multiple original body-call sites"));
                }
                self.body_call(instance, original, site, actuals, child.definition)?;
                if !actuals.is_empty() {
                    return Err(pending(
                        "mixed callable actuals need complete child substitution",
                    ));
                }
                selected = true;
                continue;
            }
            if let SemanticSiteAction::CallableCall { callee: actual } = &site.action {
                if actual != &operand || selected {
                    return Err(bad("callable call changes its actual operand"));
                }
                let recipe = self.original_recipe(original, site.recipe)?;
                let SemanticSiteRecipeKind::CallableCall { callee: expected } = &recipe.kind else {
                    return Err(bad("callable call changes original recipe kind"));
                };
                let expected = self.ty(instance.id, expected, Self::type_of(&value)?)?;
                self.same(instance.id, &value, &expected, implicit)?;
                selected = true;
            }
        }
        if !selected {
            return Err(bad("actual callable call has no original operand site"));
        }
        if let Some(result) = checked_result {
            return Ok(result);
        }
        if arguments.len() != signature.parameters.len()
            || target.parameters.len() != arguments.len()
        {
            return Err(bad("callable call changes original arity"));
        }
        for (ordinal, ((argument, _recipe), local)) in arguments
            .iter()
            .zip(&signature.parameters)
            .zip(&target.parameters)
            .enumerate()
        {
            let expected =
                self.parameter_value(child.id, ordinal, &target.locals[local.0 as usize].ty)?;
            let expected = self.map_value(&expected, &[], Some((header, child, &value)))?;
            let actual = self.get(values, *argument)?;
            self.same(instance.id, &actual, &expected, implicit)?;
        }
        let result = self.result_value(child.id, &target.return_type)?;
        self.map_value(&result, &[], Some((header, child, &value)))
    }
    fn run(&mut self, instance: &SemanticInstance) -> R<()> {
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) = instance.target
        else {
            return Err(pending(
                "lexical scope instance requires its complete original implementation witness",
            ));
        };
        if instance.lifetime_instantiation != SemanticLifetimeInstantiation::Rigid {
            return Err(pending(
                "partition requires full rigid-body substitution correspondence",
            ));
        }
        let f = self.function(id)?;
        let original = self.definition(instance.definition)?;
        let (SemanticDefinitionBody::Function { signature, .. }
        | SemanticDefinitionBody::FunctionWithBody { signature, .. }) = &original.body
        else {
            return Err(bad("actual function changes original scope kind"));
        };
        {
            let proof = self.ledger.original.borrow();
            if !proof
                .as_ref()
                .is_some_and(|p| p.complete.contains(&original.id))
            {
                return Err(bad(
                    "actual body lacks its complete original symbolic proof",
                ));
            }
        }
        if signature.parameters.len() != f.parameters.len()
            || signature.call_kind != f.call_kind
            || signature.result_mode != f.result_mode
        {
            return Err(bad(
                "actual function changes its complete original signature",
            ));
        }
        protocol::header(self, original, f)?;
        self.requirements(instance, original, None, &BTreeMap::new())?;
        self.pay(
            index::search(self.index.sites.len())
                .saturating_add(f.locals.len())
                .saturating_add(f.values.len()),
        )?;
        let sites: Vec<_> = self
            .index
            .sites
            .get(&instance.id)
            .into_iter()
            .flatten()
            .map(|offset| &self.program.sites[*offset])
            .collect();
        self.pay(sites.len())?;
        let mut locals = vec![None; f.locals.len()];
        let values = vec![None; f.values.len()];
        let mut declared = BTreeMap::new();
        let implicit = BTreeMap::new();
        for (ordinal, (parameter, _recipe)) in
            f.parameters.iter().zip(&signature.parameters).enumerate()
        {
            let ty = &f
                .locals
                .get(parameter.0 as usize)
                .ok_or_else(|| bad("parameter is absent"))?
                .ty;
            let value = self.parameter_value(instance.id, ordinal, ty)?;
            locals[parameter.0 as usize] = Some(self.copy(&value)?);
            declared.insert(*parameter, value);
        }
        let expected_result = self.result_value(instance.id, &f.return_type)?;
        self.check_header(instance, f, signature)?;
        if matches!(
            &original.body,
            SemanticDefinitionBody::FunctionWithBody {
                body: SemanticOriginalFunctionBody::External,
                ..
            }
        ) {
            let declaration = f
                .foreign
                .as_ref()
                .ok_or_else(|| bad("original external declaration has no actual foreign ABI"))?;
            if declaration.abi != "C"
                || !f.blocks.is_empty()
                || !f.values.is_empty()
                || f.environment.is_some()
                || !original.parameters.is_empty()
            {
                return Err(bad(
                    "external actual changes its complete bodyless ABI or generic environment",
                ));
            }
            return Ok(());
        }
        if f.foreign.is_some() {
            return Err(bad(
                "defined original body was replaced by an external declaration",
            ));
        }
        let cfg = flow::Cfg::new(self, instance, f)?;
        cfg.check_sites(self, f, &sites)?;
        let mut incoming_roots = BTreeMap::new();
        self.pay(f.parameters.len().saturating_mul(2))?;
        for local in &f.parameters {
            if matches!(
                stored::physical(&f.locals[local.0 as usize].ty),
                Type::Address { .. } | Type::Desc(_)
            ) {
                incoming_roots.insert(
                    *local,
                    std::collections::BTreeSet::from([arguments::Root {
                        local: *local,
                        path: Vec::new(),
                    }]),
                );
            }
        }
        let initial = flow::State {
            schema: f.id,
            instance: instance.id,
            locals,
            values,
            declared,
            implicit,
            vacant_captures: Default::default(),
            stale_values: Default::default(),
            stale_locals: Default::default(),
            scopes: Vec::new(),
            unsafe_regions: Default::default(),
            owners: BTreeMap::new(),
            vacant: Default::default(),
            role_sources: BTreeMap::new(),
            value_role_sources: BTreeMap::new(),
            reborrows: BTreeMap::new(),
            value_roots: BTreeMap::new(),
            local_roots: incoming_roots,
        };
        let mut reaching: Vec<Vec<flow::State>> = (0..f.blocks.len()).map(|_| Vec::new()).collect();
        let mut work = std::collections::VecDeque::new();
        cfg.offer(self, None, f.entry, initial, &mut reaching, &mut work)?;
        while let Some((block_index, state_index)) = work.pop_front() {
            self.pay(1)?;
            let flow::State {
                schema: _,
                instance: _,
                mut locals,
                mut values,
                mut declared,
                mut implicit,
                vacant_captures,
                stale_values,
                stale_locals,
                mut scopes,
                mut unsafe_regions,
                mut owners,
                vacant,
                role_sources,
                value_role_sources,
                reborrows,
                value_roots,
                local_roots,
            } = reaching[block_index][state_index].snapshot(self)?;
            self.vacant_captures = vacant_captures;
            self.stale_values = stale_values;
            self.stale_locals = stale_locals;
            self.vacant = vacant;
            self.role_sources = role_sources;
            self.value_role_sources = value_role_sources;
            self.reborrows = reborrows;
            self.value_roots = value_roots;
            self.local_roots = local_roots;
            let block = &f.blocks[block_index];
            for (index, instruction) in block.instructions.iter().enumerate() {
                self.pay(1)?;
                self.current_point = Some((
                    instance.id,
                    f.id,
                    block.id,
                    index,
                    std::mem::discriminant(&instruction.kind),
                ));
                self.constraints(
                    instance,
                    original,
                    f,
                    &sites,
                    block.id,
                    index,
                    SemanticSiteEdge::Before,
                    &values,
                    &locals,
                    &mut declared,
                    &mut implicit,
                )?;
                self.pay(1)?;
                let ty = instruction
                    .result
                    .and_then(|id| f.values.get(id.0 as usize));
                let propagated_role = match &instruction.kind {
                    InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                        self.role_sources.get(local)
                    }
                    InstructionKind::ReborrowAddress { address, .. } => {
                        self.value_role_sources.get(address)
                    }
                    _ => None,
                };
                let propagated_role = propagated_role
                    .map(|source| roles::snapshot_source(source, self.limits, self.left))
                    .transpose()?;
                let mut propagated_roots = std::collections::BTreeSet::new();
                match &instruction.kind {
                    InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                        if let Some(roots) = self.local_roots.get(local) {
                            *self.left = self
                                .left
                                .checked_sub(roots.len().saturating_add(1))
                                .ok_or_else(|| {
                                    Diagnostic::limit(
                                        Phase::Types,
                                        "local argument roots work exceeded",
                                        None,
                                    )
                                })?;
                            for root in roots {
                                *self.left =
                                    self.left.checked_sub(root.path.len()).ok_or_else(|| {
                                        Diagnostic::limit(
                                            Phase::Types,
                                            "local argument root path work exceeded",
                                            None,
                                        )
                                    })?;
                            }
                            propagated_roots = roots.clone();
                        }
                    }
                    InstructionKind::BorrowLocal { local }
                    | InstructionKind::BorrowDynOwned { local, .. }
                    | InstructionKind::BorrowLocalMut { local }
                    | InstructionKind::DescribeLocal { local } => {
                        self.pay(1)?;
                        propagated_roots.insert(arguments::Root {
                            local: *local,
                            path: Vec::new(),
                        });
                    }
                    InstructionKind::BorrowPackageField { local, index, .. }
                    | InstructionKind::DescribePackageField { local, index }
                    | InstructionKind::BorrowOwnedArrayElement { local, index, .. } => {
                        self.pay(2)?;
                        propagated_roots.insert(arguments::Root {
                            local: *local,
                            path: vec![PlaceProjection::Field(*index)],
                        });
                    }
                    InstructionKind::BorrowPlaceLocal { local, path, .. } => {
                        self.pay(path.len().saturating_add(1))?;
                        propagated_roots.insert(arguments::Root {
                            local: *local,
                            path: path.clone(),
                        });
                    }
                    InstructionKind::BorrowFieldLocal { local, path, .. } => {
                        self.pay(path.len().saturating_add(1))?;
                        propagated_roots.insert(arguments::Root {
                            local: *local,
                            path: path
                                .iter()
                                .map(|index| PlaceProjection::Field(*index))
                                .collect(),
                        });
                    }
                    InstructionKind::BorrowVariantLocal { local, path, .. } => {
                        self.pay(path.len().saturating_add(1))?;
                        propagated_roots.insert(arguments::Root {
                            local: *local,
                            path: path
                                .iter()
                                .map(|item| PlaceProjection::Variant(*item))
                                .collect(),
                        });
                    }
                    InstructionKind::ReborrowAddress { address, .. }
                    | InstructionKind::AddressIndex { address, .. }
                    | InstructionKind::AddressSlice { address, .. }
                    | InstructionKind::AddressField { address, .. } => {
                        let parent = self.get(&values, *address)?;
                        propagated_roots = arguments::roots(self, *address, &parent)?;
                        if let InstructionKind::AddressField { index, .. } = &instruction.kind {
                            let mut fields = std::collections::BTreeSet::new();
                            for mut root in propagated_roots {
                                self.pay(1)?;
                                if root.path.len() >= self.limits.max_call_depth {
                                    return Err(Diagnostic::limit(
                                        Phase::Types,
                                        "argument field-root depth exceeded",
                                        None,
                                    ));
                                }
                                root.path.push(PlaceProjection::Field(*index));
                                fields.insert(root);
                            }
                            propagated_roots = fields;
                        }
                    }
                    _ => {}
                }
                let intrinsic = intrinsics::evaluate(
                    self,
                    instance,
                    original,
                    &sites,
                    block.id,
                    index,
                    &instruction.kind,
                    ty.or(Some(&Type::Unit)),
                    &values,
                    &locals,
                    &implicit,
                )?;
                let produced = if intrinsic.is_some() {
                    intrinsic
                } else {
                    match &instruction.kind {
                        InstructionKind::Constant(Constant::Unit) => {
                            let value = self.zero(&Type::Unit)?;
                            let result = instruction
                                .result
                                .ok_or_else(|| bad("Unit literal has no actual SSA result"))?;
                            if let Some(expected) = self.constructed_value(
                                instance,
                                original,
                                &sites,
                                block.id,
                                index,
                                result,
                                &Type::Unit,
                                &implicit,
                            )? {
                                self.same(instance.id, &value, &expected, &implicit)?;
                            }
                            Some(value)
                        }
                        InstructionKind::FunctionRef { function }
                        | InstructionKind::ForeignFunctionRef { function } => {
                            let result = instruction
                                .result
                                .ok_or_else(|| bad("function address has no actual SSA result"))?;
                            Some(function_values::reference(
                                self,
                                instance,
                                original,
                                &sites,
                                block.id,
                                index,
                                result,
                                *function,
                                matches!(
                                    instruction.kind,
                                    InstructionKind::ForeignFunctionRef { .. }
                                ),
                                ty.ok_or_else(|| bad("function address has no actual type"))?,
                                &implicit,
                            )?)
                        }
                        InstructionKind::CallIndirect { callee, arguments } => {
                            Some(function_values::indirect(
                                self,
                                instance,
                                original,
                                &sites,
                                block.id,
                                index,
                                *callee,
                                arguments,
                                ty.unwrap_or(&Type::Unit),
                                &values,
                                &implicit,
                            )?)
                        }
                        InstructionKind::Load(local) => Some(self.local(&locals, *local)?),
                        InstructionKind::TakeLocal { local } => {
                            let value = self.local(&locals, *local)?;
                            self.clear_vacant(*local)?;
                            self.role_sources.remove(local);
                            self.local_roots.remove(local);
                            locals[local.0 as usize] = None;
                            Some(value)
                        }
                        InstructionKind::Store { local, value } => {
                            let operand = *value;
                            let mut value = self.get(&values, operand)?;
                            if Self::type_of(&value)?
                                != &f
                                    .locals
                                    .get(local.0 as usize)
                                    .ok_or_else(|| bad("store local is absent"))?
                                    .ty
                            {
                                return Err(bad("store changes actual whole local type"));
                            }
                            let stored_roots = if matches!(
                                Self::type_of(&value)?,
                                Type::Address { .. } | Type::Desc(_)
                            ) {
                                arguments::roots(self, operand, &value)?
                            } else {
                                std::collections::BTreeSet::new()
                            };
                            let adapted = roles::store(
                                self, instance, original, &sites, *local, operand, &value,
                                &implicit,
                            )?;
                            let source = if let Some((adapted, source)) = adapted {
                                value = adapted;
                                Some(source)
                            } else {
                                None
                            };
                            if let Some(expected) = declared.get(local) {
                                self.same(instance.id, &value, expected, &implicit)?;
                            }
                            self.stale_locals.remove(local);
                            self.clear_vacant(*local)?;
                            self.role_sources.remove(local);
                            self.local_roots.remove(local);
                            if let Some(source) = source {
                                self.role_sources.insert(*local, source);
                            }
                            if !stored_roots.is_empty() {
                                self.local_roots.insert(*local, stored_roots);
                            }
                            *locals
                                .get_mut(local.0 as usize)
                                .ok_or_else(|| bad("store local is absent"))? = Some(value);
                            None
                        }
                        InstructionKind::StorageLive(local)
                        | InstructionKind::StorageDead(local) => {
                            owners.remove(local);
                            if matches!(instruction.kind, InstructionKind::StorageLive(_)) {
                                if let Some(scope) = scopes.last() {
                                    owners.insert(*local, *scope);
                                }
                            }
                            declared.remove(local);
                            self.stale_locals.remove(local);
                            self.clear_vacant(*local)?;
                            self.role_sources.remove(local);
                            self.local_roots.remove(local);
                            *locals
                                .get_mut(local.0 as usize)
                                .ok_or_else(|| bad("activation local is absent"))? = None;
                            None
                        }
                        InstructionKind::BorrowPackageField {
                            local,
                            index: field,
                            mutable,
                        }
                        | InstructionKind::BorrowOwnedArrayElement {
                            local,
                            index: field,
                            mutable,
                        } => {
                            places::local_mutable(self, f, *local, *mutable)?;
                            places::borrow_mode(ty, *mutable)?;
                            let value = self.projected_local(
                                &locals,
                                *local,
                                &[PlaceProjection::Field(*field)],
                            )?;
                            let result = instruction
                                .result
                                .ok_or_else(|| bad("field borrow has no actual result"))?;
                            let image = self.occurrence(f, result)?;
                            Some(self.address(
                                ty.ok_or_else(|| bad("field borrow has no complete result type"))?,
                                image,
                                &value,
                            )?)
                        }
                        InstructionKind::BorrowPlaceLocal {
                            local,
                            path,
                            mutable,
                        } => {
                            if path.is_empty() {
                                return Err(bad("actual inline place borrow has no projection"));
                            }
                            places::local_mutable(self, f, *local, *mutable)?;
                            if !matches!(ty,Some(Type::Address{mutable:actual,..}) if actual==mutable)
                            {
                                return Err(bad(
                                    "actual inline place borrow changes its requested mode",
                                ));
                            }
                            let value = self.projected_local(&locals, *local, path)?;
                            let result = instruction
                                .result
                                .ok_or_else(|| bad("place borrow has no actual result"))?;
                            let image = self.occurrence(f, result)?;
                            Some(self.address(
                                ty.ok_or_else(|| bad("place borrow has no complete result type"))?,
                                image,
                                &value,
                            )?)
                        }
                        InstructionKind::PlaceTagLocal { local, path } => {
                            places::local_tag(self, &locals, *local, path, false)?;
                            Some(self.zero(&Type::I32)?)
                        }
                        InstructionKind::VariantTagLocal { local, path } => {
                            self.pay(path.len())?;
                            let path = path
                                .iter()
                                .map(|item| PlaceProjection::Variant(*item))
                                .collect::<Vec<_>>();
                            places::local_tag(self, &locals, *local, &path, true)?;
                            Some(self.zero(&Type::I32)?)
                        }
                        InstructionKind::PlaceTagAddress { address, path } => {
                            let value = self.get(&values, *address)?;
                            places::address_mutable(self, &value, false)?;
                            let Type::Address { inner, .. } = Self::type_of(&value)? else {
                                unreachable!()
                            };
                            places::tag_type(self, inner, path, false)?;
                            Some(self.zero(&Type::I32)?)
                        }
                        InstructionKind::VariantTagAddress { address, path } => {
                            self.pay(path.len())?;
                            let path = path
                                .iter()
                                .map(|item| PlaceProjection::Variant(*item))
                                .collect::<Vec<_>>();
                            let value = self.get(&values, *address)?;
                            places::address_mutable(self, &value, false)?;
                            let Type::Address { inner, .. } = Self::type_of(&value)? else {
                                unreachable!()
                            };
                            places::tag_type(self, inner, &path, true)?;
                            Some(self.zero(&Type::I32)?)
                        }
                        InstructionKind::BorrowFieldLocal {
                            local,
                            path,
                            mutable,
                        } => {
                            places::local_mutable(self, f, *local, *mutable)?;
                            places::borrow_mode(ty, *mutable)?;
                            self.pay(path.len())?;
                            let path = path
                                .iter()
                                .map(|index| PlaceProjection::Field(*index))
                                .collect::<Vec<_>>();
                            if path.is_empty() {
                                return Err(bad("inline field borrow has an empty actual path"));
                            }
                            let root = &f
                                .locals
                                .get(local.0 as usize)
                                .ok_or_else(|| bad("inline field owner is absent"))?
                                .ty;
                            if !root.is_copy_value()
                                || !matches!(
                                    stored::physical(root),
                                    Type::Tuple(_) | Type::Record { .. }
                                )
                            {
                                return Err(bad(
                                    "inline Copy field borrow changes its actual owner family",
                                ));
                            }
                            let value = self.projected_local(&locals, *local, &path)?;
                            let image = self.occurrence(
                                f,
                                instruction
                                    .result
                                    .ok_or_else(|| bad("inline field borrow has no result"))?,
                            )?;
                            Some(self.address(
                                ty.ok_or_else(|| bad("inline field borrow has no result type"))?,
                                image,
                                &value,
                            )?)
                        }
                        InstructionKind::ReadVariantLocal { local, path }
                        | InstructionKind::BorrowVariantLocal { local, path, .. } => {
                            self.pay(path.len())?;
                            let path = path
                                .iter()
                                .map(|item| PlaceProjection::Variant(*item))
                                .collect::<Vec<_>>();
                            if path.is_empty() {
                                return Err(bad(
                                    "actual variant payload projection has an empty path",
                                ));
                            }
                            let value = self.projected_local(&locals, *local, &path)?;
                            if let InstructionKind::BorrowVariantLocal { mutable, .. } =
                                instruction.kind
                            {
                                places::local_mutable(self, f, *local, mutable)?;
                                places::borrow_mode(ty, mutable)?;
                                let image = self.occurrence(
                                    f,
                                    instruction.result.ok_or_else(|| {
                                        bad("variant payload borrow has no result")
                                    })?,
                                )?;
                                Some(self.address(
                                    ty.ok_or_else(|| bad("variant payload borrow has no type"))?,
                                    image,
                                    &value,
                                )?)
                            } else {
                                if !Self::type_of(&value)?.is_copy_value() {
                                    return Err(bad(
                                        "actual variant read copies a non-Copy terminal",
                                    ));
                                }
                                Some(value)
                            }
                        }
                        InstructionKind::BorrowPlaceAddress {
                            address,
                            path,
                            mutable,
                        } => {
                            if path.is_empty() {
                                return Err(bad("actual addressed place borrow has no projection"));
                            }
                            let parent = self.get(&values, *address)?;
                            places::address_mutable(self, &parent, *mutable)?;
                            places::borrow_mode(ty, *mutable)?;
                            let value = self.pointee(&parent)?;
                            let value = places::path(self, &value, path)?;
                            Some(self.address_projection(
                                ty.ok_or_else(|| bad("addressed place borrow has no type"))?,
                                &parent,
                                &value,
                            )?)
                        }
                        InstructionKind::BorrowVariantAddress {
                            address,
                            path,
                            mutable,
                        } => {
                            if path.is_empty() {
                                return Err(bad(
                                    "actual addressed variant borrow has no projection",
                                ));
                            }
                            self.pay(path.len())?;
                            let path = path
                                .iter()
                                .map(|item| PlaceProjection::Variant(*item))
                                .collect::<Vec<_>>();
                            let parent = self.get(&values, *address)?;
                            places::address_mutable(self, &parent, *mutable)?;
                            places::borrow_mode(ty, *mutable)?;
                            let value = self.pointee(&parent)?;
                            let value = places::path(self, &value, &path)?;
                            Some(self.address_projection(
                                ty.ok_or_else(|| bad("addressed variant borrow has no type"))?,
                                &parent,
                                &value,
                            )?)
                        }
                        InstructionKind::BorrowDynOwned { local, mutable } => {
                            let source = self.local(&locals, *local)?;
                            let result = instruction
                                .result
                                .ok_or_else(|| bad("dynamic borrow has no actual SSA result"))?;
                            let image = self.occurrence(f, result)?;
                            dynamic::seal_borrow(
                                self,
                                original,
                                f,
                                &sites,
                                block.id,
                                index,
                                *local,
                                &image,
                                &mut implicit,
                            )?;
                            Some(dynamic::borrow(
                                self,
                                f,
                                *local,
                                *mutable,
                                ty.ok_or_else(|| bad("dynamic borrow has no actual type"))?,
                                image,
                                &source,
                            )?)
                        }
                        InstructionKind::BorrowLocal { local }
                        | InstructionKind::BorrowLocalMut { local } => {
                            let inner = self.local(&locals, *local)?;
                            let result = instruction
                                .result
                                .ok_or_else(|| bad("borrow has no result"))?;
                            let image = self.occurrence(f, result)?;
                            Some(self.address(
                                ty.ok_or_else(|| bad("borrow has no type"))?,
                                image,
                                &inner,
                            )?)
                        }
                        InstructionKind::ReborrowAddress { address, .. } => {
                            let parent = self.get(&values, *address)?;
                            let child = self.pointee(&parent)?;
                            if matches!(ty, Some(Type::Address { mutable: true, .. }))
                                && !matches!(
                                    Self::type_of(&parent)?,
                                    Type::Address { mutable: true, .. }
                                )
                            {
                                return Err(bad(
                                    "reborrow strengthens actual parent access permission",
                                ));
                            }
                            let result = instruction
                                .result
                                .ok_or_else(|| bad("reborrow has no actual result occurrence"))?;
                            let source = roles::outer(self, &parent)?;
                            self.pay(1)?;
                            self.reborrows.insert(
                                result,
                                arguments::Edge {
                                    parent: *address,
                                    source,
                                },
                            );
                            let image = self.occurrence(f, result)?;
                            Some(self.address(
                                ty.ok_or_else(|| bad("reborrow has no type"))?,
                                image,
                                &child,
                            )?)
                        }
                        InstructionKind::ReadCapture { index } => {
                            let value = self.capture(instance, *index)?;
                            let output =
                                ty.ok_or_else(|| bad("actual capture read has no result type"))?;
                            match (Self::type_of(&value)?, output) {
                                (source, target)
                                    if source == target
                                        && (stored::physical(source).is_copy_value()
                                            || matches!(
                                                stored::physical(source),
                                                Type::Address { .. } | Type::Desc(_)
                                            )) =>
                                {
                                    Some(value)
                                }
                                (
                                    Type::Address {
                                        mutable: true,
                                        inner,
                                    },
                                    Type::Address {
                                        mutable: false,
                                        inner: target,
                                    },
                                ) if inner == target => {
                                    let child = self.pointee(&value)?;
                                    Some(self.address_projection(output, &value, &child)?)
                                }
                                _ => {
                                    return Err(bad(
                                        "actual capture read changes its complete Copy/view mode or moves an owning field",
                                    ));
                                }
                            }
                        }
                        InstructionKind::WriteCapture { index, value } => {
                            let closure = f
                                .environment
                                .ok_or_else(|| bad("capture write has no active environment"))?;
                            let (definition, child) = self.child(closure)?;
                            if child.id != instance.id
                                || !matches!(
                                    definition.kind,
                                    CallableKind::FnMut | CallableKind::FnOnce
                                )
                            {
                                return Err(bad(
                                    "actual capture write lacks an exclusive active environment",
                                ));
                            }
                            let expected = self.capture_initial(instance, *index)?;
                            self.pay(self.vacant_captures.len())?;
                            if !self.vacant_captures.contains(&(instance.id, *index))
                                && !Self::type_of(&expected)?.is_copy_value()
                                && !matches!(
                                    Self::type_of(&expected)?,
                                    Type::Address { .. } | Type::Desc(_)
                                )
                            {
                                return Err(bad(
                                    "actual capture write overwrites an initialized owning field",
                                ));
                            }
                            let value = self.get(&values, *value)?;
                            self.same(instance.id, &value, &expected, &implicit)?;
                            self.vacant_captures.remove(&(instance.id, *index));
                            None
                        }
                        InstructionKind::TakeCapture { index } => {
                            let closure = f
                                .environment
                                .ok_or_else(|| bad("capture transfer has no active environment"))?;
                            let (definition, child) = self.child(closure)?;
                            if child.id != instance.id
                                || !matches!(
                                    definition.kind,
                                    CallableKind::FnOnce | CallableKind::FnMut
                                )
                            {
                                return Err(bad(
                                    "actual capture transfer has no exclusive owning environment",
                                ));
                            }
                            let value = self.capture(instance, *index)?;
                            self.pay(self.vacant_captures.len().saturating_add(1))?;
                            self.vacant_captures.insert((instance.id, *index));
                            Some(value)
                        }
                        InstructionKind::DescribeCapture { index } => {
                            let value = self.capture(instance, *index)?;
                            if !matches!(Self::type_of(&value)?, Type::Managed(_)) {
                                return Err(bad("actual capture description has no owning field"));
                            }
                            let value = projection::pointee(self, &value)?;
                            let image = self.occurrence(
                                f,
                                instruction
                                    .result
                                    .ok_or_else(|| bad("capture description has no result"))?,
                            )?;
                            Some(self.address(
                                ty.ok_or_else(|| bad("capture description has no type"))?,
                                image,
                                &value,
                            )?)
                        }
                        InstructionKind::BorrowCapture { index, mutable } => {
                            let closure = f
                                .environment
                                .ok_or_else(|| bad("capture borrow has no active environment"))?;
                            let (definition, child) = self.child(closure)?;
                            if child.id != instance.id
                                || *mutable && definition.kind == CallableKind::Fn
                            {
                                return Err(bad(
                                    "actual mutable capture borrow has only a shared environment",
                                ));
                            }
                            if !matches!(ty,Some(Type::Address{mutable:actual,..}) if actual==mutable)
                            {
                                return Err(bad(
                                    "actual capture borrow changes its requested outer mode",
                                ));
                            }
                            let value = self.capture(instance, *index)?;
                            let result = instruction
                                .result
                                .ok_or_else(|| bad("capture borrow has no result"))?;
                            let image = self.occurrence(f, result)?;
                            Some(self.address(
                                ty.ok_or_else(|| bad("capture borrow has no type"))?,
                                image,
                                &value,
                            )?)
                        }
                        InstructionKind::ReadAddress { address } => {
                            let value = self.get(&values, *address)?;
                            Some(self.pointee(&value)?)
                        }
                        InstructionKind::AddressField {
                            address,
                            index: field,
                        } => {
                            let parent = self.get(&values, *address)?;
                            let pointee = self.pointee(&parent)?;
                            let value = self.field_value(&pointee, *field)?;
                            Some(self.address_projection(
                                ty.ok_or_else(|| bad("field address has no type"))?,
                                &parent,
                                &value,
                            )?)
                        }
                        InstructionKind::AddressIndex {
                            address,
                            index: offset,
                        } => {
                            let parent = self.get(&values, *address)?;
                            self.get(&values, *offset)?;
                            let child = self.pointee(&parent)?;
                            let element = match Self::type_of(&child)? {
                                Type::Array { element, .. } | Type::Slice { element } => element,
                                _ => {
                                    return Err(bad(
                                        "indexed address changes its actual complete container",
                                    ));
                                }
                            };
                            let value = self.zero(element)?;
                            Some(self.address_projection(
                                ty.ok_or_else(|| bad("indexed address has no type"))?,
                                &parent,
                                &value,
                            )?)
                        }
                        InstructionKind::AddressSlice {
                            address,
                            start,
                            end,
                        } => {
                            let parent = self.get(&values, *address)?;
                            for bound in start.iter().chain(end.iter()) {
                                self.get(&values, *bound)?;
                            }
                            let source = self.pointee(&parent)?;
                            let element = match Self::type_of(&source)? {
                                Type::Array { element, .. } | Type::Slice { element } => element,
                                _ => {
                                    return Err(bad(
                                        "slice address changes its actual complete container",
                                    ));
                                }
                            };
                            let value = self.zero(&Type::Slice {
                                element: element.clone(),
                            })?;
                            Some(self.address_projection(
                                ty.ok_or_else(|| bad("slice address has no type"))?,
                                &parent,
                                &value,
                            )?)
                        }
                        InstructionKind::MakeAggregate { fields }
                        | InstructionKind::MakeOwnedTuple { fields }
                        | InstructionKind::MakeOwnedArray { elements: fields }
                        | InstructionKind::MakePackage { fields }
                        | InstructionKind::MakeManaged { fields }
                        | InstructionKind::MakeVariant { fields, .. } => {
                            if matches!(
                                ty.map(stored::physical),
                                Some(
                                    Type::Record { .. }
                                        | Type::Package { .. }
                                        | Type::Enum { .. }
                                        | Type::Managed(_)
                                )
                            ) {
                                Some(
                                    self.constructor(
                                        instance,
                                        original,
                                        f,
                                        &sites,
                                        block.id,
                                        index,
                                        instruction
                                            .result
                                            .ok_or_else(|| bad("constructor has no result"))?,
                                        match instruction.kind {
                                            InstructionKind::MakeVariant { variant, .. } => {
                                                Some(variant)
                                            }
                                            _ => None,
                                        },
                                        fields,
                                        &values,
                                        &implicit,
                                    )?,
                                )
                            } else {
                                let actual_ty = ty.ok_or_else(|| bad("aggregate has no type"))?;
                                let result = instruction
                                    .result
                                    .ok_or_else(|| bad("aggregate has no actual result"))?;
                                let expected = match self.constructed_value(
                                    instance, original, &sites, block.id, index, result, actual_ty,
                                    &implicit,
                                )? {
                                    Some(value) => value,
                                    None => match self
                                        .emitted_value(instance, f, block.id, index, result)?
                                    {
                                        Some(value) => value,
                                        None => self.zero(actual_ty)?,
                                    },
                                };
                                match stored::physical(actual_ty) {
                                    Type::Tuple(types) if types.len() == fields.len() => {
                                        for (index, field) in fields.iter().enumerate() {
                                            let target =
                                                self.field_value(&expected, index as u32)?;
                                            let value = self.get(&values, *field)?;
                                            self.same(instance.id, &value, &target, &implicit)?;
                                        }
                                    }
                                    Type::Array { element, length }
                                        if *length as usize == fields.len() =>
                                    {
                                        let target = self.zero(element)?;
                                        for field in fields {
                                            let value = self.get(&values, *field)?;
                                            self.same(instance.id, &value, &target, &implicit)?;
                                        }
                                    }
                                    _ => {
                                        return Err(bad(
                                            "aggregate changes its complete original product arity",
                                        ));
                                    }
                                }
                                Some(expected)
                            }
                        }
                        InstructionKind::MakeClosure { closure, captures } => {
                            let (d, child) = self.child(*closure)?;
                            if child.parent != Some(instance.id)
                                || captures.len() != d.captures.len()
                            {
                                return Err(bad(
                                    "closure constructor changes its actual lexical creator/captures",
                                ));
                            }
                            let mut selected = None;
                            for site in self.point_sites(&sites, block.id, index)? {
                                if let SemanticSiteAction::Closure {
                                    closure: SemanticClosureTarget::Typed(id),
                                    environment,
                                    ..
                                } = &site.action
                                {
                                    if id != closure || selected.is_some() {
                                        return Err(bad(
                                            "closure constructor changes its actual environment identity",
                                        ));
                                    }
                                    let recipe = self.original_recipe(original, site.recipe)?;
                                    let SemanticSiteRecipeKind::Closure {
                                        definition,
                                        environment: recipes,
                                    } = &recipe.kind
                                    else {
                                        return Err(bad(
                                            "closure constructor changes original recipe kind",
                                        ));
                                    };
                                    if *definition != child.definition
                                        || recipes.len() != environment.len()
                                    {
                                        return Err(bad(
                                            "closure constructor omits original ghost slots",
                                        ));
                                    }
                                    let expected = child
                                        .environment
                                        .as_ref()
                                        .ok_or_else(|| bad("child ghost environment is absent"))?;
                                    if environment.len() != expected.actuals.len() {
                                        return Err(bad(
                                            "child ghost environment omits complete actuals",
                                        ));
                                    }
                                    let child_original = self.definition(child.definition)?;
                                    if child_original.environment.len() != expected.actuals.len() {
                                        return Err(bad(
                                            "original environment omits full mixed sealed slots",
                                        ));
                                    }
                                    for (formal, claimed) in
                                        child_original.environment.iter().zip(&expected.actuals)
                                    {
                                        let source = super::materialize::actual_at(
                                            self.program,
                                            self.module,
                                            self.ledger,
                                            instance.id,
                                            &formal.source,
                                            self.limits,
                                            self.left,
                                        )?;
                                        crate::dyn_metadata::semantic_operand_snapshot(
                                            &source,
                                            self.limits,
                                            self.left,
                                            1,
                                        )?;
                                        crate::dyn_metadata::semantic_operand_snapshot(
                                            claimed,
                                            self.limits,
                                            self.left,
                                            1,
                                        )?;
                                        if &source != claimed {
                                            return Err(bad(
                                                "sealed environment actual changes its original creator-domain source",
                                            ));
                                        }
                                    }
                                    for ((recipe, actual), claimed) in
                                        recipes.iter().zip(environment).zip(&expected.actuals)
                                    {
                                        let value = super::materialize::actual_at(
                                            self.program,
                                            self.module,
                                            self.ledger,
                                            instance.id,
                                            recipe,
                                            self.limits,
                                            self.left,
                                        )?;
                                        crate::dyn_metadata::semantic_operand_snapshot(
                                            &value,
                                            self.limits,
                                            self.left,
                                            2,
                                        )?;
                                        if &value != actual || &value != claimed {
                                            return Err(bad(
                                                "closure ghost slot changes its original creator actual",
                                            ));
                                        }
                                    }
                                    selected = Some(());
                                }
                            }
                            if selected.is_none() {
                                return Err(bad("actual closure lacks its original creation site"));
                            }
                            let header = d.storage_life.as_ref().ok_or_else(|| {
                                pending("constructed closure full domain is unknown")
                            })?;
                            let env = child.environment.as_ref().ok_or_else(|| {
                                bad("constructed closure has no sealed environment")
                            })?;
                            let mut images = vec![None; header.fixed_binder_count as usize];
                            for binding in &child.header_embedding {
                                self.pay(1)?;
                                let StorageBinder::Fixed(fixed) = binding.header else {
                                    continue;
                                };
                                let image = match &binding.actual {
                                    SemanticClassExpr::Environment {
                                        scope: SemanticScopeRef::Ancestor(0),
                                        ordinal,
                                    } => {
                                        let Some(SemanticActual::Life(image)) =
                                            env.actuals.get(*ordinal as usize)
                                        else {
                                            return Err(bad(
                                                "constructed Fixed class selects non-Life slot",
                                            ));
                                        };
                                        crate::dyn_metadata::semantic_operand_snapshot(
                                            image,
                                            self.limits,
                                            self.left,
                                            1,
                                        )?;
                                        image.clone()
                                    }
                                    SemanticClassExpr::IntrinsicClass {
                                        operand: SemanticOperand::Environment(ordinal),
                                        path,
                                        class,
                                    } if path.is_empty() => {
                                        let Some(SemanticActual::Type(
                                            SemanticTypeValue::Resolved { life, classes, .. },
                                        )) = env.actuals.get(*ordinal as usize)
                                        else {
                                            return Err(bad(
                                                "constructed Fixed class selects non-Type slot",
                                            ));
                                        };
                                        if life.binder_count as usize != classes.len() {
                                            return Err(bad(
                                                "constructed Type environment omits complete classes",
                                            ));
                                        }
                                        let image = classes.get(*class as usize).ok_or_else(|| {
                                        bad("constructed Fixed class exceeds sealed Type domain")
                                    })?;
                                        crate::dyn_metadata::semantic_operand_snapshot(
                                            image,
                                            self.limits,
                                            self.left,
                                            1,
                                        )?;
                                        image.clone()
                                    }
                                    SemanticClassExpr::Implicit {
                                        scope: SemanticScopeRef::Ancestor(0),
                                        ordinal,
                                    } => {
                                        let capture_index = capture::input(self, child, *ordinal)?;
                                        self.pay(header.bindings.len())?;
                                        let mut rows = header.bindings.iter().filter(|row| {
                                            row.binder == StorageBinder::Fixed(fixed)
                                        });
                                        let mut image = None;
                                        for row in &mut rows {
                                            let StoragePosition {
                                                base: StorageBase::Capture(index),
                                                path,
                                            } = &row.position
                                            else {
                                                return Err(bad(
                                                    "implicit captured Fixed class is attached to another header role",
                                                ));
                                            };
                                            if !path.is_empty() || *index != capture_index {
                                                return Err(bad(
                                                    "implicit captured Fixed class changes its original actual capture observation",
                                                ));
                                            }
                                            let operand = captures.get(*index as usize).ok_or_else(|| bad("implicit capture omits its actual constructor operand"))?;
                                            let actual = self.get(&values, *operand)?;
                                            let SemanticTypeValue::Resolved {
                                                life, classes, ..
                                            } = &actual
                                            else {
                                                return Err(bad(
                                                    "implicit capture input retains an unresolved whole type",
                                                ));
                                            };
                                            self.pay(life.slots.len())?;
                                            let mut outer = life
                                                .slots
                                                .iter()
                                                .filter(|slot| slot.path.is_empty());
                                            let slot = outer.next().ok_or_else(|| bad("implicit capture constructor operand has no real outer occurrence"))?;
                                            if outer.next().is_some() {
                                                return Err(bad(
                                                    "implicit capture constructor repeats an outer occurrence",
                                                ));
                                            }
                                            let actual_image = classes.get(slot.binder as usize).ok_or_else(|| bad("implicit capture outer image exceeds its complete domain"))?;
                                            crate::dyn_metadata::semantic_operand_snapshot(
                                                actual_image,
                                                self.limits,
                                                self.left,
                                                1,
                                            )?;
                                            if let Some(old) = &image {
                                                self.same_class(
                                                    instance.id,
                                                    old,
                                                    actual_image,
                                                    &implicit,
                                                )?;
                                            }
                                            image = Some(actual_image.clone());
                                        }
                                        image.ok_or_else(|| bad("implicit captured Fixed class has no actual capture header occurrence"))?
                                    }
                                    _ => {
                                        return Err(pending(
                                            "constructed Fixed class requires its complete actual capture or ghost seal",
                                        ));
                                    }
                                };
                                *images
                                    .get_mut(fixed as usize)
                                    .ok_or_else(|| bad("Fixed image exceeds actual domain"))? =
                                    Some(image);
                            }
                            let images =
                                images.into_iter().collect::<Option<Vec<_>>>().ok_or_else(
                                    || bad("constructed closure omits actual Fixed images"),
                                )?;
                            let closure_value = self.resolved(
                                ty.ok_or_else(|| bad("closure constructor has no type"))?,
                                StorageLifeShape {
                                    binder_count: header.fixed_binder_count,
                                    ..empty()
                                },
                                images,
                            )?;
                            for (capture, source) in captures.iter().enumerate() {
                                let expected = self.capture(child, capture as u32)?;
                                let expected = self.map_value(
                                    &expected,
                                    &[],
                                    Some((header, child, &closure_value)),
                                )?;
                                let actual = self.get(&values, *source)?;
                                self.same(instance.id, &actual, &expected, &implicit)?;
                            }
                            Some(closure_value)
                        }
                        InstructionKind::Call {
                            function: target,
                            arguments,
                        }
                        | InstructionKind::ForeignCall {
                            function: target,
                            arguments,
                            ..
                        }
                        | InstructionKind::SuspendCall {
                            function: target,
                            arguments,
                        } => {
                            let callee = sourcecall::callee(
                                self, instance, *target, &sites, block.id, index,
                            )?;
                            let target_function = self.function(*target)?;
                            let target_definition = self.definition(callee.definition)?;
                            protocol::header(self, target_definition, target_function)?;
                            calls::abi(
                                self,
                                original,
                                target_function,
                                &sites,
                                block.id,
                                index,
                                &instruction.kind,
                                &unsafe_regions,
                            )?;
                            if let Some(result) = methods::evaluate(
                                self,
                                instance,
                                original,
                                callee,
                                target_function,
                                &sites,
                                block.id,
                                index,
                                arguments,
                                &values,
                                &locals,
                                &mut implicit,
                            )? {
                                Some(result)
                            } else if let Some(result) = methods::direct(
                                self,
                                instance,
                                original,
                                callee,
                                target_function,
                                &sites,
                                block.id,
                                index,
                                arguments,
                                &values,
                                &locals,
                                &implicit,
                            )? {
                                Some(result)
                            } else {
                                let target_original = self.definition(callee.definition)?;
                                let (SemanticDefinitionBody::Function {
                                    signature: target_signature,
                                    ..
                                }
                                | SemanticDefinitionBody::FunctionWithBody {
                                    signature: target_signature,
                                    ..
                                }) = &target_original.body
                                else {
                                    return Err(bad("call target changes original scope kind"));
                                };
                                let mut selected = None;
                                for site in self.point_sites(&sites, block.id, index)? {
                                    let actuals = match &site.action {
                                        SemanticSiteAction::BodyCall { actuals } => {
                                            self.body_call(
                                                instance,
                                                original,
                                                site,
                                                actuals,
                                                callee.definition,
                                            )?;
                                            actuals
                                        }
                                        SemanticSiteAction::Call {
                                            target: SemanticFunctionTarget::Typed(target_id),
                                            actuals,
                                        } => {
                                            if target_id != target {
                                                return Err(bad("call changes actual callee"));
                                            }
                                            let recipe =
                                                self.original_recipe(original, site.recipe)?;
                                            let SemanticSiteRecipeKind::Call {
                                                target: definition,
                                                actuals: recipes,
                                            } = &recipe.kind
                                            else {
                                                return Err(bad(
                                                    "call changes original recipe kind",
                                                ));
                                            };
                                            if *definition != callee.definition
                                                || recipes.len() != actuals.len()
                                            {
                                                return Err(bad("call omits original actuals"));
                                            }
                                            for (recipe, actual) in recipes.iter().zip(actuals) {
                                                let expected = super::materialize::actual_at(
                                                    self.program,
                                                    self.module,
                                                    self.ledger,
                                                    instance.id,
                                                    recipe,
                                                    self.limits,
                                                    self.left,
                                                )?;
                                                crate::dyn_metadata::semantic_operand_snapshot(
                                                    actual,
                                                    self.limits,
                                                    self.left,
                                                    1,
                                                )?;
                                                crate::dyn_metadata::semantic_operand_snapshot(
                                                    &expected,
                                                    self.limits,
                                                    self.left,
                                                    1,
                                                )?;
                                                if actual != &expected {
                                                    return Err(bad(
                                                        "call changes original mixed actual expression",
                                                    ));
                                                }
                                            }
                                            actuals
                                        }
                                        _ => continue,
                                    };
                                    if selected.is_some()
                                        || actuals.len() != target_original.parameters.len()
                                    {
                                        return Err(bad(
                                            "call has repeated or incomplete actual world",
                                        ));
                                    }
                                    {
                                        self.requirements(
                                            instance,
                                            target_original,
                                            Some(actuals),
                                            &implicit,
                                        )?;
                                        if arguments.len() != target_signature.parameters.len()
                                            || arguments.len() != target_function.parameters.len()
                                        {
                                            return Err(bad(
                                                "call omits actual original parameters",
                                            ));
                                        }
                                        for (ordinal, ((argument, _recipe), local)) in arguments
                                            .iter()
                                            .zip(&target_signature.parameters)
                                            .zip(&target_function.parameters)
                                            .enumerate()
                                        {
                                            let expected = self.parameter_value(
                                                callee.id,
                                                ordinal,
                                                &target_function.locals[local.0 as usize].ty,
                                            )?;
                                            let expected =
                                                self.map_value(&expected, actuals, None)?;
                                            let value = self.get(&values, *argument)?;
                                            self.same(instance.id, &value, &expected, &implicit)?;
                                        }
                                        let result = self.result_value(
                                            callee.id,
                                            &target_function.return_type,
                                        )?;
                                        selected = Some(self.map_value(&result, actuals, None)?);
                                    }
                                }
                                Some(selected.ok_or_else(|| {
                                    bad("actual call has no original full-actual site")
                                })?)
                            }
                        }
                        InstructionKind::CallClosure { callee, arguments }
                        | InstructionKind::CallClosureOnce { callee, arguments }
                        | InstructionKind::CallClosureBorrowed {
                            callee, arguments, ..
                        } => {
                            let value = self.get(&values, *callee)?;
                            Some(self.invoke(
                                instance,
                                original,
                                &sites,
                                block.id,
                                index,
                                SemanticOperand::Value(*callee),
                                &value,
                                match instruction.kind {
                                    InstructionKind::CallClosureOnce { .. } => CallableKind::FnOnce,
                                    InstructionKind::CallClosureBorrowed { kind, .. } => kind,
                                    InstructionKind::CallClosureMut { .. } => CallableKind::FnMut,
                                    _ => CallableKind::Fn,
                                },
                                arguments,
                                &values,
                                &locals,
                                &implicit,
                            )?)
                        }
                        InstructionKind::CallClosureShared { callee, arguments }
                        | InstructionKind::CallClosureMut { callee, arguments } => {
                            let value = self.local(&locals, *callee)?;
                            Some(self.invoke(
                                instance,
                                original,
                                &sites,
                                block.id,
                                index,
                                SemanticOperand::Local(*callee),
                                &value,
                                match instruction.kind {
                                    InstructionKind::CallClosureOnce { .. } => CallableKind::FnOnce,
                                    InstructionKind::CallClosureBorrowed { kind, .. } => kind,
                                    InstructionKind::CallClosureMut { .. } => CallableKind::FnMut,
                                    _ => CallableKind::Fn,
                                },
                                arguments,
                                &values,
                                &locals,
                                &implicit,
                            )?)
                        }
                        InstructionKind::TakePackageField {
                            local,
                            index: field,
                        } => {
                            let path = vec![PlaceProjection::Field(*field)];
                            let value = self.projected_local(&locals, *local, &path)?;
                            self.pay(path.len().saturating_add(1))?;
                            self.vacant.insert((*local, path));
                            Some(value)
                        }
                        InstructionKind::TakeTupleField { local, path }
                        | InstructionKind::TakeVariantField { local, path } => {
                            if path.is_empty() {
                                return Err(bad("partial take has no actual projection"));
                            }
                            let value = self.projected_local(&locals, *local, path)?;
                            self.pay(path.len().saturating_add(1))?;
                            self.vacant.insert((*local, path.clone()));
                            Some(value)
                        }
                        InstructionKind::StorePackageField {
                            local,
                            index: field,
                            value,
                        } => {
                            let path = vec![PlaceProjection::Field(*field)];
                            let parent = locals
                                .get(local.0 as usize)
                                .and_then(|value| value.as_ref())
                                .ok_or_else(|| bad("partial store has no active local shell"))?;
                            let parent = self.copy(parent)?;
                            let expected = self.field_value(&parent, *field)?;
                            let actual = self.get(&values, *value)?;
                            self.same(instance.id, &actual, &expected, &implicit)?;
                            self.pay(
                                self.vacant
                                    .len()
                                    .saturating_mul(path.len().saturating_add(1)),
                            )?;
                            if self.vacant.iter().any(|(owner, vacant)| {
                                *owner == *local && path.starts_with(vacant) && vacant != &path
                            }) {
                                return Err(bad("partial store crosses an uninitialized parent"));
                            }
                            self.vacant.retain(|(owner, vacant)| {
                                *owner != *local || !vacant.starts_with(&path)
                            });
                            None
                        }
                        InstructionKind::DropPackageField {
                            local,
                            index: field,
                        } => {
                            let path = vec![PlaceProjection::Field(*field)];
                            self.pay(path.len().saturating_add(1))?;
                            if locals
                                .get(local.0 as usize)
                                .and_then(|value| value.as_ref())
                                .is_some()
                            {
                                self.vacant.insert((*local, path));
                            }
                            None
                        }
                        InstructionKind::UnpackTuple { local, fields }
                        | InstructionKind::UnpackVariant { local, fields, .. } => {
                            let value = self.local(&locals, *local)?;
                            let actual_fields = match (
                                stored::physical(Self::type_of(&value)?),
                                &instruction.kind,
                            ) {
                                (Type::Tuple(fields), InstructionKind::UnpackTuple { .. }) => {
                                    fields
                                }
                                (
                                    Type::Enum { variants, .. },
                                    InstructionKind::UnpackVariant { variant, .. },
                                ) => {
                                    &variants
                                        .get(*variant as usize)
                                        .ok_or_else(|| {
                                            bad("actual unpack selects an absent variant")
                                        })?
                                        .1
                                }
                                _ => {
                                    return Err(bad(
                                        "actual unpack changes its complete product/sum constructor",
                                    ));
                                }
                            };
                            if fields.len() != actual_fields.len() {
                                return Err(bad("tuple unpack changes actual field arity"));
                            }
                            self.pay(fields.len())?;
                            let mut targets = std::collections::BTreeSet::new();
                            for (index, target) in fields.iter().enumerate() {
                                if *target == *local || !targets.insert(*target) {
                                    return Err(bad("tuple unpack repeats an actual destination"));
                                }
                                if locals.get(target.0 as usize).is_some_and(Option::is_some) {
                                    return Err(bad(
                                        "actual unpack overwrites an initialized destination",
                                    ));
                                }
                                let field =
                                    if let InstructionKind::UnpackVariant { variant, .. } =
                                        instruction.kind
                                    {
                                        projection::place(
                                            self,
                                            &value,
                                            PlaceProjection::Variant(VariantProjection {
                                                variant,
                                                index: index as u32,
                                            }),
                                        )?
                                    } else {
                                        self.field_value(&value, index as u32)?
                                    };
                                if Self::type_of(&field)?
                                    != &f
                                        .locals
                                        .get(target.0 as usize)
                                        .ok_or_else(|| bad("tuple unpack destination is absent"))?
                                        .ty
                                {
                                    return Err(bad(
                                        "tuple unpack changes a complete destination type",
                                    ));
                                }
                                if let Some(expected) = declared.get(target) {
                                    self.same(instance.id, &field, expected, &implicit)?;
                                }
                                self.clear_vacant(*target)?;
                                self.stale_locals.remove(target);
                                locals[target.0 as usize] = Some(field);
                            }
                            self.clear_vacant(*local)?;
                            self.role_sources.remove(local);
                            self.local_roots.remove(local);
                            locals[local.0 as usize] = None;
                            None
                        }
                        InstructionKind::DropClosure { local }
                        | InstructionKind::DropManaged { local }
                        | InstructionKind::DropMutexGuard { local }
                        | InstructionKind::DropPackage { local }
                        | InstructionKind::DropTuple { local }
                        | InstructionKind::DropEnum { local }
                        | InstructionKind::DropErrorUnion { local }
                        | InstructionKind::DropOwnedArray { local }
                        | InstructionKind::DropDynOwned { local } => {
                            let local_ty = &f
                                .locals
                                .get(local.0 as usize)
                                .ok_or_else(|| bad("cleanup local is absent"))?
                                .ty;
                            let valid = match (&instruction.kind, stored::physical(local_ty)) {
                                (InstructionKind::DropClosure { .. }, Type::Closure(_))
                                | (InstructionKind::DropManaged { .. }, Type::Managed(_))
                                | (
                                    InstructionKind::DropMutexGuard { .. },
                                    Type::MutexGuard { .. },
                                )
                                | (InstructionKind::DropPackage { .. }, Type::Package { .. })
                                | (InstructionKind::DropTuple { .. }, Type::Tuple(_))
                                | (InstructionKind::DropEnum { .. }, Type::Enum { .. })
                                | (
                                    InstructionKind::DropErrorUnion { .. },
                                    Type::ErrorUnion { .. },
                                )
                                | (InstructionKind::DropOwnedArray { .. }, Type::Array { .. })
                                | (InstructionKind::DropDynOwned { .. }, Type::OwnedDyn { .. }) => {
                                    true
                                }
                                _ => false,
                            };
                            if !valid {
                                return Err(bad("cleanup changes the actual complete local kind"));
                            }
                            // Cleanup is conditional on physical initialization. It
                            // does not read a vacant occupant or equate old epochs.
                            self.clear_vacant(*local)?;
                            self.role_sources.remove(local);
                            self.local_roots.remove(local);
                            locals[local.0 as usize] = None;
                            self.stale_locals.remove(local);
                            None
                        }
                        InstructionKind::WriteAddress { address, value } => {
                            let address = self.get(&values, *address)?;
                            let expected = self.pointee(&address)?;
                            let value = self.get(&values, *value)?;
                            self.same(instance.id, &value, &expected, &implicit)?;
                            None
                        }
                        InstructionKind::StaticUtf8 { .. } => {
                            let actual = ty.ok_or_else(|| bad("static utf8 has no type"))?;
                            if !matches!(actual, Type::Address { mutable: false, inner } if matches!(inner.as_ref(), Type::Slice { element } if element.as_ref() == &Type::U8))
                            {
                                return Err(bad(
                                    "static utf8 changes its exact declared address type",
                                ));
                            }
                            Some(self.resolved(actual, empty(), Vec::new())?)
                        }
                        InstructionKind::MakeErrorUnion { is_error, value } => {
                            let result = instruction
                                .result
                                .ok_or_else(|| bad("error union constructor has no result"))?;
                            let actual_ty = ty
                                .ok_or_else(|| bad("error union constructor has no actual type"))?;
                            let expected = match self.constructed_value(
                                instance, original, &sites, block.id, index, result, actual_ty,
                                &implicit,
                            )? {
                                Some(value) => value,
                                None => match self
                                    .emitted_value(instance, f, block.id, index, result)?
                                {
                                    Some(value) => value,
                                    None if ty == Some(&f.return_type) => {
                                        self.copy(&expected_result)?
                                    }
                                    None => {
                                        return Err(pending(
                                            "error union constructor lacks its original complete result equation",
                                        ));
                                    }
                                },
                            };
                            let payload = projection::place(
                                self,
                                &expected,
                                PlaceProjection::ErrorArm(*is_error),
                            )?;
                            let actual = self.get(&values, *value)?;
                            self.same(instance.id, &actual, &payload, &implicit)?;
                            Some(expected)
                        }
                        InstructionKind::ReadErrorUnion { value, is_error }
                        | InstructionKind::TakeErrorUnion { value, is_error } => {
                            let value = self.get(&values, *value)?;
                            Some(projection::place(
                                self,
                                &value,
                                PlaceProjection::ErrorArm(*is_error),
                            )?)
                        }
                        InstructionKind::ReadVariant {
                            value,
                            variant,
                            index: field,
                        } => {
                            let value = self.get(&values, *value)?;
                            Some(projection::place(
                                self,
                                &value,
                                PlaceProjection::Variant(VariantProjection {
                                    variant: *variant,
                                    index: *field,
                                }),
                            )?)
                        }
                        InstructionKind::DescribeLocal { local } => {
                            let value = self.local(&locals, *local)?;
                            if !matches!(Self::type_of(&value)?, Type::Managed(_)) {
                                return Err(bad("description local is not its actual owner"));
                            }
                            let value = projection::pointee(self, &value)?;
                            let result = instruction
                                .result
                                .ok_or_else(|| bad("local description has no result"))?;
                            let image = self.occurrence(f, result)?;
                            Some(self.address(
                                ty.ok_or_else(|| bad("local description has no type"))?,
                                image,
                                &value,
                            )?)
                        }
                        InstructionKind::DescribePackageField {
                            local,
                            index: field,
                        } => {
                            let owner = self.projected_local(
                                &locals,
                                *local,
                                &[PlaceProjection::Field(*field)],
                            )?;
                            if !matches!(Self::type_of(&owner)?, Type::Managed(_)) {
                                return Err(bad(
                                    "actual package description does not select its live owning resource field",
                                ));
                            }
                            let payload = projection::pointee(self, &owner)?;
                            let result = instruction.result.ok_or_else(|| {
                                bad("actual package description has no result occurrence")
                            })?;
                            let image = self.occurrence(f, result)?;
                            Some(self.address(
                                ty.ok_or_else(|| {
                                    bad("actual package description has no complete result type")
                                })?,
                                image,
                                &payload,
                            )?)
                        }
                        InstructionKind::DescribeAddress { address } => {
                            let parent = self.get(&values, *address)?;
                            places::address_mutable(self, &parent, true)?;
                            let owner = self.pointee(&parent)?;
                            if !matches!(Self::type_of(&owner)?, Type::Managed(_)) {
                                return Err(bad(
                                    "actual addressed description has no owning carrier",
                                ));
                            }
                            let child = projection::pointee(self, &owner)?;
                            Some(self.address_projection(
                                ty.ok_or_else(|| bad("addressed description has no type"))?,
                                &parent,
                                &child,
                            )?)
                        }
                        InstructionKind::ReadDesc { desc, index: field } => {
                            let desc = self.get(&values, *desc)?;
                            let payload = self.pointee(&desc)?;
                            Some(self.field_value(&payload, *field)?)
                        }
                        InstructionKind::WriteDesc {
                            desc,
                            index: field,
                            value,
                        } => {
                            let desc = self.get(&values, *desc)?;
                            let payload = self.pointee(&desc)?;
                            let expected = self.field_value(&payload, *field)?;
                            let actual = self.get(&values, *value)?;
                            self.same(instance.id, &actual, &expected, &implicit)?;
                            None
                        }
                        InstructionKind::BindPackageCapture {
                            local,
                            index: capture,
                        } => {
                            let value = self.capture(instance, *capture)?;
                            if Self::type_of(&value)?
                                != &f
                                    .locals
                                    .get(local.0 as usize)
                                    .ok_or_else(|| bad("capture view local is absent"))?
                                    .ty
                            {
                                return Err(bad("capture view changes its complete actual type"));
                            }
                            locals[local.0 as usize] = Some(value);
                            None
                        }
                        InstructionKind::EndPackageCapture { local, .. } => {
                            self.clear_vacant(*local)?;
                            self.role_sources.remove(local);
                            self.local_roots.remove(local);
                            locals[local.0 as usize] = None;
                            self.stale_locals.remove(local);
                            None
                        }
                        InstructionKind::VariantTag { value }
                        | InstructionKind::ErrorTag { value } => {
                            self.get(&values, *value)?;
                            Some(self.zero(ty.ok_or_else(|| bad("tag read has no type"))?)?)
                        }
                        InstructionKind::SliceLength { address } => {
                            self.get(&values, *address)?;
                            Some(self.zero(ty.ok_or_else(|| bad("slice length has no type"))?)?)
                        }
                        InstructionKind::ExtractIndex {
                            aggregate,
                            index: offset,
                        } => {
                            let value = self.get(&values, *aggregate)?;
                            self.get(&values, *offset)?;
                            let Type::Array { element, .. } =
                                stored::physical(Self::type_of(&value)?)
                            else {
                                return Err(bad(
                                    "array projection changes its complete actual constructor",
                                ));
                            };
                            Some(self.zero(element)?)
                        }
                        InstructionKind::UpdateIndex {
                            aggregate,
                            index: offset,
                            value,
                        } => {
                            let parent = self.get(&values, *aggregate)?;
                            self.get(&values, *offset)?;
                            let Type::Array { element, .. } =
                                stored::physical(Self::type_of(&parent)?)
                            else {
                                return Err(bad(
                                    "array update changes its complete actual constructor",
                                ));
                            };
                            let expected = self.zero(element)?;
                            let value = self.get(&values, *value)?;
                            self.same(instance.id, &value, &expected, &implicit)?;
                            Some(parent)
                        }
                        InstructionKind::UpdateField {
                            aggregate,
                            index: field,
                            value,
                        } => {
                            let parent = self.get(&values, *aggregate)?;
                            let expected = self.field_value(&parent, *field)?;
                            let value = self.get(&values, *value)?;
                            self.same(instance.id, &value, &expected, &implicit)?;
                            Some(parent)
                        }
                        InstructionKind::ExtractField { aggregate, index } => {
                            let value = self.get(&values, *aggregate)?;
                            Some(self.field_value(&value, *index)?)
                        }
                        InstructionKind::ReadPackageField { local, index } => {
                            Some(self.projected_local(
                                &locals,
                                *local,
                                &[PlaceProjection::Field(*index)],
                            )?)
                        }
                        InstructionKind::RegisterDefer { scope, local } => {
                            if scopes.last() != Some(scope) {
                                return Err(bad(
                                    "defer registers outside its actual lexical activation",
                                ));
                            }
                            let value = self.local(&locals, *local)?;
                            if !matches!(Self::type_of(&value)?, Type::Closure(_)) {
                                return Err(bad("defer local is not its actual closure"));
                            }
                            None
                        }
                        InstructionKind::UnsafeEnter { region } => {
                            self.pay(unsafe_regions.len().saturating_add(1))?;
                            if unsafe_regions.contains(region) {
                                return Err(bad(
                                    "actual unsafe region reenters an active lexical boundary",
                                ));
                            }
                            if unsafe_regions.len() >= self.limits.max_call_depth {
                                return Err(Diagnostic::limit(
                                    Phase::Types,
                                    "actual unsafe region depth exceeded",
                                    None,
                                ));
                            }
                            unsafe_regions.push(*region);
                            None
                        }
                        InstructionKind::UnsafeExit { region } => {
                            self.pay(unsafe_regions.len().saturating_add(1))?;
                            if unsafe_regions.pop() != Some(*region) {
                                return Err(bad("actual unsafe exit has no reaching region entry"));
                            }
                            None
                        }
                        InstructionKind::ScopeEnter { scope } => {
                            self.pay(scopes.len().saturating_add(1))?;
                            if scopes.contains(scope) {
                                return Err(bad("class flow reenters an active lexical scope"));
                            }
                            if scopes.len() >= self.limits.max_call_depth {
                                return Err(Diagnostic::limit(
                                    Phase::Types,
                                    "class-flow scope depth exceeded",
                                    None,
                                ));
                            }
                            scopes.push(*scope);
                            None
                        }
                        InstructionKind::ScopeExit { scope, .. } => {
                            if scopes.pop() != Some(*scope) {
                                return Err(bad("class flow exits another lexical scope"));
                            }
                            self.pay(owners.len())?;
                            for (local, owner) in &owners {
                                if owner == scope {
                                    self.clear_vacant(*local)?;
                                    self.role_sources.remove(local);
                                    self.local_roots.remove(local);
                                    locals[local.0 as usize] = None;
                                    declared.remove(local);
                                    self.stale_locals.remove(local);
                                }
                            }
                            owners.retain(|_, owner| owner != scope);
                            None
                        }
                        InstructionKind::Constant(value) => {
                            let actual = ty.ok_or_else(|| bad("constant has no actual type"))?;
                            if value.ty() != *actual {
                                return Err(bad(
                                    "constant opcode changes its complete literal type",
                                ));
                            }
                            Some(self.zero(actual)?)
                        }
                        InstructionKind::Binary { left, right, .. }
                            if ty.is_some_and(Type::is_b0_scalar) =>
                        {
                            self.get(&values, *left)?;
                            self.get(&values, *right)?;
                            Some(self.zero(ty.ok_or_else(|| bad("scalar producer has no type"))?)?)
                        }
                        InstructionKind::Unary { operand, .. }
                            if ty.is_some_and(Type::is_b0_scalar) =>
                        {
                            self.get(&values, *operand)?;
                            Some(self.zero(ty.ok_or_else(|| bad("scalar producer has no type"))?)?)
                        }
                        InstructionKind::ConvertInteger { value, .. }
                        | InstructionKind::ConvertNumber { value, .. }
                            if ty.is_some_and(Type::is_b0_scalar) =>
                        {
                            self.get(&values, *value)?;
                            Some(self.zero(ty.ok_or_else(|| bad("scalar producer has no type"))?)?)
                        }
                        _ => {
                            return Err(calls::missing(
                                self,
                                f,
                                block.id,
                                index,
                                &instruction.kind,
                            )?);
                        }
                    }
                };
                if let Some(result) = instruction.result {
                    let value = produced
                        .ok_or_else(|| bad("actual SSA producer has no complete class equation"))?;
                    let value = self.normalize(instance.id, &value, &implicit)?;
                    if Self::type_of(&value)?
                        != f.values
                            .get(result.0 as usize)
                            .ok_or_else(|| bad("actual SSA result is absent"))?
                    {
                        return Err(bad(
                            "actual SSA type changes its whole original class value",
                        ));
                    }
                    self.stale_values.remove(&result);
                    self.value_roots.remove(&result);
                    if !propagated_roots.is_empty() {
                        self.value_roots.insert(result, propagated_roots);
                    }
                    self.value_role_sources.remove(&result);
                    if let Some(source) = propagated_role {
                        self.value_role_sources.insert(result, source);
                    }
                    *values
                        .get_mut(result.0 as usize)
                        .ok_or_else(|| bad("SSA class result is absent"))? = Some(value);
                } else if let Some(value) = produced {
                    if Self::type_of(&value)? != &Type::Unit {
                        return Err(bad("class-producing operation has no actual SSA result"));
                    }
                }
                self.constraints(
                    instance,
                    original,
                    f,
                    &sites,
                    block.id,
                    index,
                    SemanticSiteEdge::After,
                    &values,
                    &locals,
                    &mut declared,
                    &mut implicit,
                )?;
            }
            self.constraints(
                instance,
                original,
                f,
                &sites,
                block.id,
                block.instructions.len(),
                SemanticSiteEdge::Before,
                &values,
                &locals,
                &mut declared,
                &mut implicit,
            )?;
            let successors = match block.terminator {
                Terminator::Return(Some(result)) => {
                    places::capture_exit(self, instance, f)?;
                    let value = self.get(&values, result)?;
                    self.same(instance.id, &value, &expected_result, &implicit)?;
                    Vec::new()
                }
                Terminator::Return(None) if f.return_type == Type::Unit => {
                    places::capture_exit(self, instance, f)?;
                    Vec::new()
                }
                Terminator::Trap => Vec::new(),
                Terminator::Jump(target) => vec![target],
                Terminator::Branch {
                    condition,
                    then_block,
                    else_block,
                } => {
                    let condition = self.get(&values, condition)?;
                    let expected = self.zero(&Type::Bool)?;
                    self.equal(&condition, &expected)?;
                    vec![then_block, else_block]
                }
                _ => {
                    return Err(bad(
                        "reachable CFG terminator has no complete class-flow equation",
                    ));
                }
            };
            let state = flow::State {
                schema: f.id,
                instance: instance.id,
                locals,
                values,
                declared,
                implicit,
                vacant_captures: std::mem::take(&mut self.vacant_captures),
                stale_values: std::mem::take(&mut self.stale_values),
                stale_locals: std::mem::take(&mut self.stale_locals),
                scopes,
                unsafe_regions,
                owners,
                vacant: std::mem::take(&mut self.vacant),
                role_sources: std::mem::take(&mut self.role_sources),
                value_role_sources: std::mem::take(&mut self.value_role_sources),
                reborrows: std::mem::take(&mut self.reborrows),
                value_roots: std::mem::take(&mut self.value_roots),
                local_roots: std::mem::take(&mut self.local_roots),
            };
            // The final outgoing edge owns this complete state. Only real
            // sibling edges need a deep independent copy; no consumer uses
            // the original after the last transfer. Each offer still applies
            // its own dominance, loop epoch and full dynamic state checks.
            let outgoing_count = successors.len();
            let mut outgoing_state = Some(state);
            for (ordinal, successor) in successors.into_iter().enumerate() {
                self.pay(1)?;
                let state = if ordinal.saturating_add(1) == outgoing_count {
                    outgoing_state
                        .take()
                        .ok_or_else(|| bad("CFG state transferred more than once"))?
                } else {
                    outgoing_state
                        .as_ref()
                        .ok_or_else(|| bad("CFG sibling state was already transferred"))?
                        .snapshot(self)?
                };
                cfg.offer(
                    self,
                    Some(block_index),
                    successor,
                    state,
                    &mut reaching,
                    &mut work,
                )?;
            }
        }
        protocol_flow::check(self, instance, f)?;
        Ok(())
    }
}
pub(super) fn program(
    program: &SemanticModule,
    module: &TypedModule,
    ledger: &Ledger,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let entered = *left;
    let mut stage = ("immutable actual directory", None, None);
    let result = program_at(program, module, ledger, limits, left, &mut stage);
    result.map_err(|mut diagnostic| {
        diagnostic.notes.push(format!("complete actual world pass: stage {}; instance {:?}; definition {:?}; entry allowance {}; consumed {}; remaining {}", stage.0, stage.1, stage.2, entered, entered.saturating_sub(*left), *left));
        diagnostic
    })
}
fn program_at(
    program: &SemanticModule,
    module: &TypedModule,
    ledger: &Ledger,
    limits: &Limits,
    left: &mut usize,
    stage: &mut (
        &'static str,
        Option<SemanticInstanceId>,
        Option<SemanticDefinitionId>,
    ),
) -> R<()> {
    let entered = *left;
    let index = index::Index::new(program, module, ledger, limits, left)?;
    let mut world = World {
        program,
        module,
        ledger,
        limits,
        left,
        vacant_captures: Default::default(),
        stale_values: Default::default(),
        stale_locals: Default::default(),
        vacant: Default::default(),
        role_sources: BTreeMap::new(),
        value_role_sources: BTreeMap::new(),
        reborrows: BTreeMap::new(),
        value_roots: BTreeMap::new(),
        local_roots: BTreeMap::new(),
        argument_witnesses: Vec::new(),
        receiver_adaptations: Vec::new(),
        index,
        types: types::Table::default(),
        snapshots: 0,
        snapshot_work: 0,
        comparisons: 0,
        compare_work: 0,
        current_point: None,
    };
    world.pay(
        program
            .instances
            .len()
            .saturating_add(module.functions.len()),
    )?;
    let mut targets = BTreeMap::new();
    for instance in &program.instances {
        *stage = (
            "instance identity and complete mixed actuals",
            Some(instance.id),
            Some(instance.definition),
        );
        match instance.target {
            SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) => {
                if let Some(previous) = targets.insert(function, instance.id) {
                    let first = world.instance(previous)?;
                    sourcecall::shared(&mut world, first, instance, function)?;
                }
            }
            SemanticInstanceTarget::Scope => {}
            _ => {
                return Err(bad(
                    "Core target survives the complete actual semantic world",
                ));
            }
        }
        let original = world.definition(instance.definition)?;
        if original.parameters.len() != instance.actuals.len() {
            return Err(bad("world omits original mixed actuals"));
        }
        for (ordinal, (formal, actual)) in original
            .parameters
            .iter()
            .zip(&instance.actuals)
            .enumerate()
        {
            world.pay(1)?;
            match (formal, actual) {
                (SemanticFormal::Lifetime, SemanticActual::Life(actual)) => {
                    let expected = super::materialize::class_at(
                        program,
                        module,
                        ledger,
                        instance.id,
                        &SemanticLifeRecipe::Formal(SemanticFormalRef {
                            scope: SemanticScopeRef::Ancestor(0),
                            ordinal: ordinal as u32,
                        }),
                        limits,
                        world.left,
                    )?;
                    if actual != &expected {
                        return Err(bad(
                            "rigid original world was replaced by a caller's lifetime coincidence",
                        ));
                    }
                }
                (SemanticFormal::Type { .. }, SemanticActual::Type(actual)) => {
                    let ty = World::type_of(actual)?;
                    let expected = world.ty(
                        instance.id,
                        &SemanticTypeRecipe::Formal(SemanticFormalRef {
                            scope: SemanticScopeRef::Ancestor(0),
                            ordinal: ordinal as u32,
                        }),
                        ty,
                    )?;
                    let (
                        SemanticTypeValue::Resolved { life, .. },
                        SemanticTypeValue::Resolved {
                            life: expected_life,
                            ..
                        },
                    ) = (actual, &expected)
                    else {
                        return Err(bad("whole-Type formal retains an unresolved actual"));
                    };
                    if life != expected_life {
                        return Err(bad(
                            "whole-Type actual changes its complete intrinsic domain",
                        ));
                    }
                }
                (SemanticFormal::Const { carrier }, SemanticActual::Const { ty, value }) => {
                    let expected = world.ty(instance.id, carrier, World::type_of(ty)?)?;
                    world.equal(ty, &expected)?;
                    if value.ty() != *World::type_of(&expected)? {
                        return Err(bad("Const actual changes its complete original carrier"));
                    }
                }
                _ => return Err(bad("actual world changes the original mixed formal kind")),
            }
        }
        if matches!(instance.target, SemanticInstanceTarget::Scope) {
            if instance.lifetime_instantiation != SemanticLifetimeInstantiation::Rigid
                || !instance.header_embedding.is_empty()
                || instance.environment.is_some()
            {
                return Err(bad(
                    "lexical scope changes its rigid nonphysical instance domain",
                ));
            }
            match (original.parent, instance.parent) {
                (Some(expected), Some(actual))
                    if world.instance(actual)?.definition == expected => {}
                (None, None) => {}
                _ => {
                    return Err(bad(
                        "actual lexical scope changes its real original parent chain",
                    ));
                }
            }
            let recipe = match &original.body {
                SemanticDefinitionBody::Scope(
                    SemanticScopeRecipe::InherentImpl { self_type, .. }
                    | SemanticScopeRecipe::TraitImpl { self_type, .. }
                    | SemanticScopeRecipe::BuiltinImpl { self_type, .. },
                ) => Some(self_type),
                SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { .. }) => None,
                _ => return Err(bad("actual lexical scope names another original body kind")),
            };
            if let Some(recipe) = recipe {
                let actual = instance
                    .self_type
                    .as_ref()
                    .ok_or_else(|| bad("actual implementation scope omits its complete Self"))?;
                let expected = world.ty(instance.id, recipe, World::type_of(actual)?)?;
                world.equal(actual, &expected)?;
            }
        } else {
            if original.kind == SemanticDefinitionKind::Method {
                let parent_id = original
                    .parent
                    .ok_or_else(|| bad("original method omits its real lexical scope"))?;
                let parent = world.instance(instance.parent.ok_or_else(|| {
                    bad("actual method omits its real lexical parent instance")
                })?)?;
                if parent.definition != parent_id
                    || !matches!(parent.target, SemanticInstanceTarget::Scope)
                {
                    return Err(bad(
                        "actual method changes its original lexical parent scope",
                    ));
                }
                let actual = instance
                    .self_type
                    .as_ref()
                    .ok_or_else(|| bad("actual method omits its complete lexical Self metadata"))?;
                let parent_definition = world.definition(parent_id)?;
                if matches!(
                    parent_definition.body,
                    SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { .. })
                ) {
                    let expected = parent.self_type.as_ref().ok_or_else(|| {
                        bad("actual trait method parent omits its independently bound Self")
                    })?;
                    // A default trait body has no impl Self expression. Its
                    // substitution is checked at every real provider boundary.
                    // Scalar Self has no relative class domain to rebase.
                    if matches!(actual, SemanticTypeValue::Resolved { classes, .. } if classes.is_empty())
                    {
                        world.equal(actual, expected)?;
                    } else {
                        return Err(pending(
                            "trait default Self metadata requires its complete parent-domain class rebasing",
                        ));
                    }
                } else {
                    let expected = world.ty(
                        instance.id,
                        &SemanticTypeRecipe::SelfType {
                            scope: SemanticScopeRef::Ancestor(1),
                        },
                        World::type_of(actual)?,
                    )?;
                    world.equal(actual, &expected)?;
                }
            }
            *stage = (
                "complete actual CFG",
                Some(instance.id),
                Some(instance.definition),
            );
            let before = *world.left;
            if let Err(mut diagnostic) = world.run(instance) {
                diagnostic.notes.push(format!("actual world work: entry allowance {}, failing instance {:?}/definition {:?} began {}, consumed {}, remaining {}; type cache hits {}, misses {}, lookup work {}, evaluation work {}; state snapshots {} work {}, comparisons {} work {}", entered, instance.id, instance.definition, before, before.saturating_sub(*world.left), *world.left, world.types.hits, world.types.misses, world.types.lookup_work, world.types.evaluation_work, world.snapshots, world.snapshot_work, world.comparisons, world.compare_work));
                return Err(diagnostic);
            }
        }
    }
    *stage = ("actual constructor inventory", None, None);
    let mut constructed = std::collections::BTreeSet::new();
    for function in &module.functions {
        world.pay(function.blocks.len())?;
        for block in &function.blocks {
            world.pay(block.instructions.len())?;
            for instruction in &block.instructions {
                if let InstructionKind::MakeClosure { closure, .. } = instruction.kind {
                    constructed.insert(closure);
                }
            }
        }
    }
    for definition in &module.closures {
        world.pay(1)?;
        if !constructed.contains(&definition.id) {
            return Err(bad(
                "actual closure domain has no real constructor in the complete program",
            ));
        }
    }
    for function in &module.functions {
        world.pay(1)?;
        if !targets.contains_key(&function.id) {
            return Err(bad("actual function has no complete original world"));
        }
    }
    arguments::audit(&mut world)?;
    Ok(())
}
