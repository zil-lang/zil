//! Original lifetime worlds and actual whole-type class flow. Classes here are
//! semantic identities; data targets, permissions and resource phases are not
//! manufactured by this interpreter.
use super::emission::Ledger;
use crate::*;
use std::collections::BTreeMap;
use std::rc::Rc;
// State cells are immutable complete values; live flow facts remain per state.
type SharedType = Rc<SemanticTypeValue>;
#[path = "world_arguments.rs"]
mod arguments;
#[path = "world_borrow_roles.rs"]
mod borrow_roles;
#[path = "world_calls.rs"]
mod calls;
#[path = "world_capture.rs"]
mod capture;
#[path = "world_ct_data.rs"]
mod ct_data;
#[path = "world_dyn.rs"]
mod dynamic;
#[path = "world_flow.rs"]
mod flow;
#[path = "world_ssa.rs"]
mod ssa;
#[path = "world_values.rs"]
mod function_values;
#[path = "world_index.rs"]
mod index;
#[path = "world_error_injection.rs"]
mod error_injection;
#[path = "world_intrinsics.rs"]
mod intrinsics;
#[path = "world_methods.rs"]
mod methods;
#[path = "world_places.rs"]
mod places;
#[path = "world_occurrences.rs"]
mod occurrences;
#[path = "world_persistent_slots.rs"]
mod persistent_slots;
#[path = "world_projection.rs"]
mod projection;
#[path = "world_protocol.rs"]
mod protocol;
#[path = "world_protocol_flow.rs"]
mod protocol_flow;
#[path = "world_receiver.rs"]
mod receiver;
#[path = "world_reservations.rs"]
mod reservations;
#[path = "world_roles.rs"]
mod roles;
#[path = "world_sourcecall.rs"]
mod sourcecall;
#[path = "world_types.rs"]
mod types;
#[path = "world_type_contexts.rs"]
mod type_contexts;
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
pub(super) struct World<'a, 'b> {
    pub(super) program: &'a SemanticModule,
    pub(super) module: &'a TypedModule,
    pub(super) ledger: &'a Ledger,
    pub(super) limits: &'b Limits,
    pub(super) left: &'b mut usize,
    vacant_captures: std::collections::BTreeSet<(SemanticInstanceId, u32)>,
    stale_values: std::collections::BTreeSet<ValueId>,
    stale_locals: std::collections::BTreeSet<LocalId>,
    vacant: std::collections::BTreeSet<(LocalId, Vec<PlaceProjection>)>,
    role_sources: BTreeMap<LocalId, Rc<roles::Incoming>>,
    value_role_sources: BTreeMap<ValueId, Rc<roles::Incoming>>,
    reborrows: BTreeMap<ValueId, Rc<arguments::Edge>>,
    reservations: BTreeMap<ReservationId, Rc<reservations::Reservation>>,
    value_roots: BTreeMap<ValueId, arguments::SharedRoots>,
    local_roots: BTreeMap<LocalId, arguments::SharedRoots>,
    argument_witnesses: Vec<arguments::Witness>,
    receiver_adaptations: Vec<receiver::Adaptation>,
    index: index::Index,
    call_proofs: calls::Proofs,
    types: types::Table<'a>,
    zero_scalars: [Option<SharedType>; 14],
    cfg_shapes: BTreeMap<FunctionId, Rc<flow::Shape>>,
    view: super::materialize::ActualDirectory<'a>,
    snapshots: usize,
    snapshot_work: usize,
    comparisons: usize,
    compare_work: usize,
    retired_metadata: usize,
    canonical_work: usize,
    key_work: usize,
    where_work: usize,
    same_calls: usize,
    same_work: usize,
    direct_work: BTreeMap<(&'static str, u32), (usize, usize)>,
    last_instance: Option<&'a SemanticInstance>,
    last_definition: Option<&'a SemanticDefinition>,
    last_point_block: Option<((SemanticInstanceId, BlockId), Option<Rc<index::BlockPoints>>)>,
    last_point_rows: Option<((SemanticInstanceId, BlockId, usize), Option<Rc<Vec<usize>>>)>,
    current_point: Option<(
        SemanticInstanceId,
        FunctionId,
        BlockId,
        usize,
        std::mem::Discriminant<InstructionKind>,
    )>,
}
impl<'a> World<'a, '_> {
    #[track_caller]
    fn pay(&mut self, n: usize) -> R<()> {
        let location = std::panic::Location::caller();
        *self.left = self.left.checked_sub(n).ok_or_else(|| {
            let mut diagnostic = Diagnostic::limit(
                Phase::Types,
                "original world class-flow work exceeded",
                None,
            );
            diagnostic.notes.push(format!(
                "actual direct work at {}:{}; requested {}; remaining {}",
                location.file(),
                location.line(),
                n,
                *self.left
            ));
            diagnostic
        })?;
        let row = self
            .direct_work
            .entry((location.file(), location.line()))
            .or_default();
        row.0 = row.0.saturating_add(1);
        row.1 = row.1.saturating_add(n);
        Ok(())
    }
    fn work_note(&self) -> String {
        let mut rows: Vec<_> = self.direct_work.iter().collect();
        rows.sort_by_key(|(_, (_, work))| std::cmp::Reverse(*work));
        format!(
            "actual direct work by call site (calls, paid work): {:?}",
            &rows[..rows.len().min(16)]
        )
    }
    fn instance_sites(&mut self, instance: SemanticInstanceId) -> R<Vec<&'a SemanticSite>> {
        self.pay(index::search(self.index.sites.len()))?;
        let offsets = self.index.sites.get(&instance).map(Vec::as_slice).unwrap_or(&[]);
        index::pay(self.left, offsets.len())?;
        Ok(offsets.iter().map(|&offset| &self.program.sites[offset]).collect())
    }
    fn instance(&mut self, id: SemanticInstanceId) -> R<&'a SemanticInstance> {
        self.pay(1)?;
        if let Some(instance) = self.last_instance && instance.id == id {
            return Ok(instance);
        }
        self.pay(index::search(self.index.instances.len()))?;
        if let Some(offset) = self.index.instances.get(&id) {
            let instance = &self.program.instances[*offset];
            self.last_instance = Some(instance);
            return Ok(instance);
        }
        let ledger = self.ledger;
        if let Some(source) = &ledger.frozen {
            self.pay(index::search(self.index.frozen_instances.len()))?;
            if let Some(offset) = self.index.frozen_instances.get(&id) {
                let instance = &source.semantic.instances[*offset];
                self.last_instance = Some(instance);
                return Ok(instance);
            }
        }
        Err(bad(
            "world has no actual instance in the current or frozen source domain",
        ))
    }
    fn definition(&mut self, id: SemanticDefinitionId) -> R<&'a SemanticDefinition> {
        self.pay(1)?;
        if let Some(definition) = self.last_definition && definition.id == id {
            return Ok(definition);
        }
        self.pay(index::search(self.index.definitions.len()))?;
        if let Some(offset) = self.index.definitions.get(&id) {
            let definition = &self.program.definitions[*offset];
            self.last_definition = Some(definition);
            return Ok(definition);
        }
        let ledger = self.ledger;
        if let Some(source) = &ledger.frozen {
            self.pay(index::search(self.index.frozen_definitions.len()))?;
            if let Some(offset) = self.index.frozen_definitions.get(&id) {
                let definition = &source.semantic.definitions[*offset];
                self.last_definition = Some(definition);
                return Ok(definition);
            }
        }
        Err(bad(
            "world has no original definition in the current or frozen source domain",
        ))
    }
    fn function(&mut self, id: FunctionId) -> R<&'a TypedFunction> {
        self.pay(1)?;
        if let Some(function) = self.module.functions.get(id.0 as usize)
            && function.id == id
        {
            return Ok(function);
        }
        self.pay(index::search(self.index.functions.len()))?;
        self.index
            .functions
            .get(&id)
            .map(|offset| &self.module.functions[*offset])
            .ok_or_else(|| bad("world has no actual function"))
    }
    fn closure_definition(&mut self, id: ClosureId) -> R<&'a ClosureDefinition> {
        self.pay(self.module.closures.len())?;
        let mut rows = self.module.closures.iter().filter(|row| row.id == id);
        let definition = rows.next().ok_or_else(|| bad("actual closure is absent"))?;
        if rows.next().is_some() { return Err(bad("actual closure identity repeats")); }
        if self.function(definition.function)?.environment != Some(id) {
            return Err(bad("actual closure definition changes its exact entry environment"));
        }
        Ok(definition)
    }
    fn child_at(&mut self, id: ClosureId, scope: SemanticInstanceId)
        -> R<(&'a ClosureDefinition, &'a SemanticInstance)> {
        let definition = self.closure_definition(id)?;
        let child = self.instance(scope)?;
        if child.target != SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(definition.function))
            || !matches!(&child.environment, Some(environment)
                if environment.closure == SemanticClosureTarget::Typed(id)) {
            return Err(bad("actual child world changes its exact environment or entry identity"));
        }
        let creator = self.instance(child.parent.ok_or_else(|| bad("actual closure world has no true creator"))?)?;
        let original = self.definition(child.definition)?;
        self.pay(1)?;
        if child.lifetime_instantiation != SemanticLifetimeInstantiation::Rigid
            || child.actuals.len() != original.parameters.len()
            || child.environment.as_ref().map_or(0, |environment| environment.actuals.len()) != original.environment.len() {
            return Err(bad("actual child world omits its complete rigid mixed or environment domain"));
        }
        if original.kind != SemanticDefinitionKind::Closure || original.parent != Some(creator.definition) {
            return Err(bad("actual child world changes its original lexical parent"));
        }
        let ledger = self.ledger;
        self.pay(index::search(ledger.frames.len()))?;
        let frame = ledger.frames.get(&definition.function)
            .ok_or_else(|| bad("actual child world lacks its real entry emission receipt"))?;
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(creator_function)) = creator.target else {
            return Err(bad("actual child world creator is not its emitted function"));
        };
        if frame.parent != Some((creator_function, Some(id))) {
            return Err(bad("actual child world changes its real creator emission"));
        }
        let proof = ledger.original.borrow();
        let proof = proof.as_ref().ok_or_else(|| bad("actual child world lacks complete original Gamma"))?;
        self.pay(index::search(proof.complete.len()))?;
        if !proof.complete.contains(&child.definition) {
            return Err(bad("actual child world lacks its independently proved original body"));
        }
        Ok((definition, child))
    }
    fn child_worlds(&mut self, id: ClosureId) -> R<(&'a ClosureDefinition, Vec<&'a SemanticInstance>)> {
        let definition = self.closure_definition(id)?;
        self.pay(self.program.instances.len().saturating_mul(2))?;
        let mut children = Vec::new();
        for row in &self.program.instances {
            if row.target == SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(definition.function))
                && matches!(&row.environment, Some(environment)
                    if environment.closure == SemanticClosureTarget::Typed(id)) {
                let (_, child) = self.child_at(id, row.id)?;
                children.push(child);
            }
        }
        if children.is_empty() { return Err(bad("actual closure has no complete original child worlds")); }
        let original = children[0].definition;
        if children.iter().any(|child| child.definition != original) {
            return Err(bad("one actual closure changes its original definition between worlds"));
        }
        Ok((definition, children))
    }
    fn created_child(&mut self, id: ClosureId, parent: SemanticInstanceId)
        -> R<(&'a ClosureDefinition, &'a SemanticInstance)> {
        let (definition, children) = self.child_worlds(id)?;
        let mut selected = children.into_iter().filter(|child| child.parent == Some(parent));
        let child = selected.next().ok_or_else(|| bad("actual closure has no child at its exact creator occurrence"))?;
        if selected.next().is_some() { return Err(bad("actual creator repeats an environment occurrence")); }
        Ok((definition, child))
    }
    fn copy(&mut self, value: &SemanticTypeValue) -> R<SharedType> {
        crate::dyn_metadata::semantic_operand_snapshot(value, self.limits, self.left, 1)?;
        Ok(Rc::new(value.clone()))
    }
    fn share(&mut self, value: &SharedType) -> R<SharedType> {
        self.pay(1)?;
        Ok(value.clone())
    }
    fn owned(&mut self, value: SharedType) -> R<SemanticTypeValue> {
        self.pay(1)?;
        match Rc::try_unwrap(value) {
            Ok(value) => Ok(value),
            Err(value) => {
                crate::dyn_metadata::semantic_operand_snapshot(
                    value.as_ref(),
                    self.limits,
                    self.left,
                    1,
                )?;
                Ok(value.as_ref().clone())
            }
        }
    }
    fn resolved(
        &mut self,
        ty: &Type,
        life: StorageLifeShape,
        classes: Vec<SemanticClassExpr>,
    ) -> R<SharedType> {
        inference::charge_type_snapshot(ty, Span::default(), self.limits, self.left, 1)?;
        if life.binder_count as usize != classes.len() {
            return Err(bad("whole type loses its intrinsic class count"));
        }
        self.pay(1)?;
        Ok(Rc::new(SemanticTypeValue::Resolved {
            ty: ty.clone(),
            life,
            classes,
        }))
    }
    fn zero(&mut self, ty: &Type) -> R<SharedType> {
        let slot = match ty {
            Type::Unit => 0,
            Type::Bool => 1,
            Type::I8 => 2,
            Type::I16 => 3,
            Type::I32 => 4,
            Type::I64 => 5,
            Type::Isize => 6,
            Type::U8 => 7,
            Type::U16 => 8,
            Type::U32 => 9,
            Type::U64 => 10,
            Type::Usize => 11,
            Type::F32 => 12,
            Type::F64 => 13,
            _ => return self.zero_at(ty, 0),
        };
        self.pay(1)?;
        if let Some(value) = &self.zero_scalars[slot] {
            return Ok(value.clone());
        }
        let value = self.zero_at(ty, 0)?;
        self.pay(1)?;
        self.zero_scalars[slot] = Some(value.clone());
        Ok(value)
    }
    fn zero_at(&mut self, ty: &Type, depth: usize) -> R<SharedType> {
        let shape = self.zero_shape_at(ty, depth)?;
        self.resolved(ty, shape, Vec::new())
    }
    fn zero_shape_at(&mut self, ty: &Type, depth: usize) -> R<StorageLifeShape> {
        self.pay(1)?;
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "whole-type zero-domain depth exceeded",
                None,
            ));
        }
        match ty {
            ty if ty.is_b0_scalar() => Ok(empty()),
            Type::Record { .. } | Type::Package { .. } | Type::Enum { .. } => {
                self.view
                    .ordinary_zero(ty, self.limits, self.left, &self.ledger.ordinary_stats)?;
                Ok(empty())
            }
            Type::ByteBuffer => Ok(empty()),
            Type::Task { .. } | Type::MutexGuard { .. } => {
                let shape = projection::type_shape_at(self, ty, depth + 1)?;
                if shape.binder_count != 0 {
                    return Err(bad(
                        "actual zero carrier retains a nonzero complete class domain",
                    ));
                }
                Ok(shape)
            }
            Type::FunctionPointer { parameters, result } => {
                for parameter in parameters {
                    self.zero_shape_at(parameter, depth + 1)?;
                }
                self.zero_shape_at(result, depth + 1)?;
                Ok(empty())
            }
            Type::Managed(inner)
            | Type::Mutex { payload: inner }
            | Type::OwnedStore { element: inner } => {
                self.zero_shape_at(inner, depth + 1)?;
                Ok(empty())
            }
            Type::ErrorUnion { error, result } => {
                self.zero_shape_at(error, depth + 1)?;
                self.zero_shape_at(result, depth + 1)?;
                Ok(empty())
            }
            Type::Tuple(fields) => {
                for field in fields {
                    self.zero_shape_at(field, depth + 1)?;
                }
                Ok(empty())
            }
            Type::Array { element, .. } | Type::Slice { element } => {
                self.zero_shape_at(element, depth + 1)?;
                Ok(empty())
            }
            _ => {
                // The module-backed visitor rebuilds opaque/callable and
                // resource children from their complete actual inventories.
                // An absent equation, unknown scope or nonzero child still
                // rejects; no address lifetime or runtime permission is
                // recovered from erased pointer/layout bytes here.
                self.view.ordinary_zero(
                    ty,
                    self.limits,
                    self.left,
                    &self.ledger.ordinary_stats,
                )?;
                Ok(empty())
            }
        }
    }
    fn symbolic(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticTypeRecipe,
    ) -> R<SemanticTypeRecipe> {
        super::materialize::symbolic_in_view(
            self.program,
            &self.view,
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
        function: &'a TypedFunction,
        block: BlockId,
        index: usize,
        result: ValueId,
    ) -> R<Option<SharedType>> {
        let ledger = self.ledger;
        let frame = ledger
            .frames
            .get(&function.id)
            .ok_or_else(|| bad("actual result has no private emission owner"))?;
        self.pay(index::search(self.index.expressions.len()))?;
        let nodes = match self.index.expressions.get(&(function.id, result)) {
            Some(nodes) => {
                index::pay(self.left, nodes.len())?;
                nodes.clone()
            }
            None => return Ok(None),
        };
        let original = self.definition(instance.definition)?;
        let mut selected: Option<SharedType> = None;
        for node in nodes {
            self.pay(index::search(frame.expressions.len()))?;
            let expression = frame
                .expressions
                .get(&node)
                .ok_or_else(|| bad("actual expression index changes its emitter owner"))?;
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
            for site in self.point_rows(instance.id, end.block, end.instruction)? {
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
                let value = self.actual_type(
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
    fn field_value(&mut self, parent: &SemanticTypeValue, index: u32) -> R<SharedType> {
        projection::field(self, parent, index)
    }
    fn logical_type(
        &mut self, scope: SemanticInstanceId, recipe: &SemanticTypeRecipe,
        expected: super::materialize::LogicalExpected<'_>,
    ) -> R<SharedType> {
        expected.materialize(|hint| self.physical_type(scope, recipe, hint))
    }
    fn physical_type(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticTypeRecipe,
        hint: &Type,
    ) -> R<SharedType> {
        self.type_with_hint(scope, recipe, types::Hint::Temporary(hint))
    }
    fn actual_type(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticTypeRecipe,
        hint: &'a Type,
    ) -> R<SharedType> {
        self.type_with_hint(scope, recipe, types::Hint::Actual(hint))
    }
    fn type_with_hint(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticTypeRecipe,
        hint: types::Hint<'a, '_>,
    ) -> R<SharedType> {
        self.pay(index::search(self.index.type_contexts.len()))?;
        let shared_scope = if let Some(context) = self.index.type_contexts.get(&scope).copied()
            && let Some(proof) = self.ledger.original.borrow().as_ref()
        {
            self.types.shared_scope(context, recipe, proof, self.limits, self.left)?
        } else { None };
        let (key, cached) = self.types.lookup(
            shared_scope.unwrap_or(scope), recipe, hint, self.limits, self.left,
        )?;
        if let Some(value) = cached {
            return Ok(value);
        }
        let before = *self.left;
        let value = super::materialize::type_in_view(
            self.program,
            &self.view,
            self.ledger,
            scope,
            recipe,
            Some(hint.as_type()),
            self.limits,
            self.left,
        );
        self.types.evaluation_work = self
            .types
            .evaluation_work
            .saturating_add(before.saturating_sub(*self.left));
        self.pay(1)?;
        let value = Rc::new(value?);
        self.types
            .insert(key, recipe, hint, &value, self.limits, self.left)?;
        Ok(value)
    }
    fn parameter_value<'h>(
        &mut self,
        scope: SemanticInstanceId,
        ordinal: usize,
        hint: impl Into<types::Hint<'a, 'h>>,
    ) -> R<SharedType> {
        self.type_with_hint(
            scope,
            &SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source: SemanticOriginalTypeSource::Parameter(ordinal as u32),
            },
            hint.into(),
        )
    }
    fn result_value<'h>(&mut self, scope: SemanticInstanceId, hint: impl Into<types::Hint<'a, 'h>>) -> R<SharedType> {
        self.type_with_hint(
            scope,
            &SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source: SemanticOriginalTypeSource::Result,
            },
            hint.into(),
        )
    }
    fn type_of(value: &SemanticTypeValue) -> R<&Type> {
        match value {
            SemanticTypeValue::Resolved { ty, .. } => Ok(ty),
            _ => Err(bad("Core type survives world materialization")),
        }
    }
    fn get(&mut self, values: &ssa::Values, id: ValueId) -> R<SharedType> {
        if self.stale_values.contains(&id) {
            return Err(bad(
                "loop-carried whole value requires a distinct live occurrence correspondence",
            ));
        }
        let value = values
            .get(id.0 as usize)
            .ok_or_else(|| bad("SSA has no reaching whole-type equation"))?;
        self.share(value)
    }
    fn local(&mut self, locals: &[Option<SharedType>], id: LocalId) -> R<SharedType> {
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
        self.share(value)
    }
    fn clear_vacant(&mut self, local: LocalId) -> R<()> {
        self.pay(self.vacant.len())?;
        self.vacant.retain(|(owner, _)| *owner != local);
        Ok(())
    }
    fn projected_local(
        &mut self,
        locals: &[Option<SharedType>],
        local: LocalId,
        path: &[PlaceProjection],
    ) -> R<SharedType> {
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
        let mut value = self.share(value)?;
        for selector in path {
            self.pay(1)?;
            value = projection::place(self, &value, *selector)?;
        }
        Ok(value)
    }
    #[track_caller]
    fn equal(&mut self, a: &SemanticTypeValue, b: &SemanticTypeValue) -> R<()> {
        let location = std::panic::Location::caller();
        self.pay(1)?;
        if std::ptr::eq(a, b) {
            return Ok(());
        }
        if !crate::dyn_metadata::same_formed_value(a, b, self.limits, self.left)? {
            crate::dyn_metadata::semantic_operand_snapshot(a, self.limits, self.left, 1)?;
            crate::dyn_metadata::semantic_operand_snapshot(b, self.limits, self.left, 1)?;
            let mut diagnostic = bad("actual whole type or rigid original classes disagree");
            diagnostic.notes.push(format!("actual class equation at {}:{}; actual point {:?}; left complete value {:?}; right complete value {:?}", location.file(), location.line(), self.current_point, a, b));
            return Err(diagnostic);
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
    fn capture(&mut self, instance: &SemanticInstance, index: u32) -> R<SharedType> {
        self.pay(self.vacant_captures.len().saturating_add(1))?;
        if self.vacant_captures.contains(&(instance.id, index)) {
            return Err(bad("actual capture reads a transferred environment field"));
        }
        self.capture_initial(instance, index)
    }
    fn capture_initial(&mut self, instance: &SemanticInstance, index: u32) -> R<SharedType> {
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) =
            instance.target
        else {
            return Err(bad("capture is outside an actual function"));
        };
        let f = self.function(function)?;
        let id = f
            .environment
            .ok_or_else(|| bad("capture has no actual environment"))?;
        let (d, child) = self.child_at(id, instance.id)?;
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
    ) -> R<SharedType> {
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
        let mut life = StorageLifeShape {
            binder_count: child
                .binder_count
                .checked_add(1)
                .ok_or_else(|| bad("address domain overflows"))?,
            slots: vec![StorageLifeSlot {
                path: Vec::new(),
                mode,
                binder: 0,
            }],
            ..empty()
        };
        // This map comes from the actual image vector just constructed. Child
        // nominal occurrences follow it even when an outside image is equal
        // to the new outer image; child intrinsic indices remain distinct.
        let map = (1..life.binder_count).collect::<Vec<_>>();
        super::materialize::inherit_child_shape(
            &mut life,
            child,
            LifeTypeProjection::Pointee,
            &map,
            self.limits,
            self.left,
        )?;
        self.resolved(ty, life, images)
    }
    fn address_projection(
        &mut self,
        ty: &Type,
        parent: &SemanticTypeValue,
        child: &SemanticTypeValue,
    ) -> R<SharedType> {
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
        let mut shape = StorageLifeShape {
            binder_count: life.binder_count,
            ..empty()
        };
        let map = (0..life.binder_count).collect::<Vec<_>>();
        super::materialize::inherit_child_shape(
            &mut shape,
            life,
            LifeTypeProjection::Pointee,
            &map,
            self.limits,
            self.left,
        )?;
        self.resolved(ty, shape, classes.clone())
    }
    fn pointee(&mut self, value: &SemanticTypeValue) -> R<SharedType> {
        if !matches!(Self::type_of(value)?, Type::Address { .. } | Type::Desc(_)) {
            return Err(bad(
                "pointee projection has no actual address or description",
            ));
        }
        projection::pointee(self, value)
    }
    /// Complete local κ -> signature binder map from the same occurrence
    /// traversal used to form headers. Payload syntax and equal Life images do
    /// not select the binder. All physical, retained and phantom rows remain.
    fn boundary_binders(
        &mut self, value: &SemanticTypeValue, base: StorageBase, header: &StorageLifeContract,
    ) -> R<Vec<StorageBinder>> {
        let SemanticTypeValue::Resolved { life, classes, .. } = value else { return Err(bad("call boundary retains an unresolved whole type")); };
        let mut physical = Vec::new();
        let mut intrinsic = Vec::new();
        let mut statics = Vec::new();
        super::creator_headers::rows::append_ordinals(value, base, &mut physical, &mut intrinsic, &mut statics, self.limits, self.left)?;
        self.pay(header.bindings.len().saturating_add(header.classes.len()).saturating_add(header.static_bindings.len()).saturating_add(classes.len()))?;
        if physical.len() != header.bindings.iter().filter(|row| row.position.base == base).count()
            || intrinsic.len() != header.classes.iter().filter(|row| row.position.base == base).count()
            || statics.len() != header.static_bindings.iter().filter(|position| position.base == base).count() {
            return Err(bad("call boundary changes its complete original occurrence inventory"));
        }
        let mut mapped = vec![None; life.binder_count as usize];
        let mut put = |ordinal: u32, binder: StorageBinder| -> R<()> {
            let slot = mapped.get_mut(ordinal as usize).ok_or_else(|| bad("call occurrence exceeds its complete local class domain"))?;
            if slot.is_some_and(|previous| previous != binder) { return Err(bad("one original local class has conflicting signature binders")); }
            *slot = Some(binder);
            Ok(())
        };
        for (position, ordinal) in physical {
            let mut selected = None;
            for row in &header.bindings {
                self.pay(row.position.path.len().saturating_add(position.path.len()).saturating_add(1))?;
                if row.position == position && selected.replace(row.binder).is_some() { return Err(bad("call physical occurrence repeats its original header binder")); }
            }
            put(ordinal, selected.ok_or_else(|| bad("call physical occurrence has no exact original header binder"))?)?;
        }
        for (position, ordinal) in intrinsic {
            let mut selected = None;
            for row in &header.classes {
                self.pay(row.position.path.len().saturating_add(position.path.len()).saturating_add(1))?;
                if row.position == position && selected.replace(row.binder).is_some() { return Err(bad("call intrinsic occurrence repeats its original header binder")); }
            }
            put(ordinal, selected.ok_or_else(|| bad("call intrinsic occurrence has no exact original header binder"))?)?;
        }
        for position in statics {
            let mut count = 0;
            for row in &header.static_bindings {
                self.pay(row.path.len().saturating_add(position.path.len()).saturating_add(1))?;
                if row == &position { count += 1; }
            }
            if count != 1 { return Err(bad("call Static occurrence changes its exact original header position")); }
        }
        mapped.into_iter().collect::<Option<Vec<_>>>().ok_or_else(|| bad("call boundary has an unrepresented local intrinsic class"))
    }
    fn map_value(
        &mut self,
        value: &SharedType,
        actuals: &[SemanticActual],
        environment: Option<(&StorageLifeContract, &SemanticInstance, &SemanticTypeValue, StorageBase)>,
    ) -> R<SharedType> {
        self.map_call_value(value, actuals, environment, &[], None)
    }
    fn map_call_value(
        &mut self,
        value: &SharedType,
        actuals: &[SemanticActual],
        environment: Option<(&StorageLifeContract, &SemanticInstance, &SemanticTypeValue, StorageBase)>,
        invocation: &[(SemanticClassExpr, SemanticClassExpr)],
        prepared_binders: Option<&[StorageBinder]>,
    ) -> R<SharedType> {
        let mut owned_binders = None;
        if let Some((header, _, actual, base)) = environment {
            let SemanticTypeValue::Resolved { life, classes, .. } = actual else { return Err(bad("call environment retains an unresolved whole type")); };
            if classes.len() != life.binder_count as usize || life.binder_count != header.fixed_binder_count {
                return Err(bad("call environment loses its complete persistent class domain"));
            }
            if prepared_binders.is_none() { owned_binders = Some(self.boundary_binders(value, base, header)?); }
        }
        let binders = prepared_binders.or(owned_binders.as_deref());
        let SemanticTypeValue::Resolved { ty, life, classes } = value.as_ref() else {
            return Err(bad("Core type survives call interpretation"));
        };
        if let Some(binders) = binders {
            self.pay(binders.len())?;
            if binders.len() != classes.len() { return Err(bad("call boundary loses its complete local class ordinal map")); }
        }
        if classes.is_empty() {
            return self.share(value);
        }
        self.pay(classes.len().saturating_add(1))?;
        let mut mapped = Vec::with_capacity(classes.len());
        for (ordinal, image) in classes.iter().enumerate() {
            crate::dyn_metadata::semantic_operand_snapshot(image, self.limits, self.left, 1)?;
            if let Some((header, child, SemanticTypeValue::Resolved { life, classes: actual_images, .. }, _)) = environment {
                if actual_images.len() != life.binder_count as usize || life.binder_count != header.fixed_binder_count {
                    return Err(bad("call environment loses its complete persistent class domain"));
                }
                let binder = *binders.and_then(|rows| rows.get(ordinal))
                    .ok_or_else(|| bad("call boundary omits an original whole-Type class coordinate"))?;
                let source = self.binding(child, binder)?;
                // Position selects the binder. Equality in the original child
                // Gamma checks that relation; it never chooses among equal κ.
                self.same_class(child.id, &source, image, &BTreeMap::new())?;
                let actual = match binder {
                    StorageBinder::Fixed(fixed) => actual_images.get(fixed as usize)
                        .ok_or_else(|| bad("call boundary truncates its actual Fixed environment domain"))?,
                    StorageBinder::Invocation(_) => {
                        self.pay(invocation.len())?;
                        let mut mappings = invocation.iter().filter(|(origin, _)| origin == &source);
                        let (_, target) = mappings.next().ok_or_else(|| pending("call boundary lacks its complete original Invocation substitution"))?;
                        if mappings.next().is_some() { return Err(bad("one Invocation binder has repeated actual boundary images")); }
                        target
                    }
                };
                crate::dyn_metadata::semantic_operand_snapshot(actual, self.limits, self.left, 1)?;
                mapped.push(actual.clone());
                continue;
            }
            self.pay(invocation.len())?;
            let mut matching = invocation.iter().filter(|(source, _)| source == image);
            if let Some((_, actual)) = matching.next() {
                if matching.next().is_some() { return Err(bad("one actual invocation class has repeated boundary images")); }
                crate::dyn_metadata::semantic_operand_snapshot(actual, self.limits, self.left, 1)?;
                mapped.push(actual.clone());
                continue;
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
                    mapped.push(actual.clone());
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
                    mapped.push(actual.clone());
                }
                SemanticClassExpr::Environment { .. }
                | SemanticClassExpr::IntrinsicClass {
                    operand: SemanticOperand::Environment(_),
                    ..
                }
                | SemanticClassExpr::OuterOccurrence {
                    operand: SemanticOperand::Environment(_),
                    ..
                } => {
                    return Err(bad(
                        "call environment class has no exact original Fixed image",
                    ));
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
        if mapped == *classes {
            return self.share(value);
        }
        crate::dyn_metadata::resolved_geometry_snapshot(ty, life, self.limits, self.left)?;
        Ok(Rc::new(SemanticTypeValue::Resolved {
            ty: ty.clone(),
            life: life.clone(),
            classes: mapped,
        }))
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

impl<'p> World<'p, '_> {
    fn class(
        &mut self,
        scope: SemanticInstanceId,
        image: &SemanticClassExpr,
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<SemanticClassExpr> {
        if let Some(mapped) = self.class_replacement(scope, image, implicit)? {
            return Ok(mapped);
        }
        crate::dyn_metadata::semantic_operand_snapshot(image, self.limits, self.left, 1)?;
        Ok(image.clone())
    }

    fn class_replacement(
        &mut self,
        scope: SemanticInstanceId,
        image: &SemanticClassExpr,
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<Option<SemanticClassExpr>> {
        match image {
            SemanticClassExpr::Implicit {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            } => {
                self.pay(index::search(implicit.len()))?;
                let Some(actual) = implicit.get(ordinal) else { return Ok(None); };
                crate::dyn_metadata::semantic_operand_snapshot(actual, self.limits, self.left, 1)?;
                Ok(Some(actual.clone()))
            }
            SemanticClassExpr::Formal { scope: relative, ordinal } => {
                crate::dyn_metadata::semantic_operand_snapshot(image, self.limits, self.left, 1)?;
                super::materialize::class_in_view(
                    self.program,
                    &self.view,
                    self.ledger,
                    scope,
                    &SemanticLifeRecipe::Formal(SemanticFormalRef {
                        scope: *relative,
                        ordinal: *ordinal,
                    }),
                    self.limits,
                    self.left,
                ).map(Some)
            }
            SemanticClassExpr::OuterOccurrence {
                operand: SemanticOperand::Environment(_),
                ..
            }
            | SemanticClassExpr::IntrinsicClass {
                operand: SemanticOperand::Environment(_),
                ..
            }
            | SemanticClassExpr::Environment { .. } => {
                crate::dyn_metadata::semantic_operand_snapshot(image, self.limits, self.left, 1)?;
                roles::environment_class(self, scope, image).map(Some)
            }
            _ => Ok(None),
        }
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
    fn normalized_images<'v>(
        &mut self,
        scope: SemanticInstanceId,
        classes: &'v [SemanticClassExpr],
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<std::borrow::Cow<'v, [SemanticClassExpr]>> {
        self.pay(classes.len().saturating_add(1))?;
        let mut mapped = None;
        for (ordinal, image) in classes.iter().enumerate() {
            let replacement = self.class_replacement(scope, image, implicit)?;
            if mapped.is_none()
                && let Some(actual) = replacement.as_ref()
                && !crate::dyn_metadata::same_live_classes(
                    std::slice::from_ref(actual), std::slice::from_ref(image),
                    self.limits, self.left,
                )?
            {
                self.pay(classes.len())?;
                let mut prefix = Vec::with_capacity(classes.len());
                for prior in &classes[..ordinal] {
                    crate::dyn_metadata::semantic_operand_snapshot(prior, self.limits, self.left, 1)?;
                    prefix.push(prior.clone());
                }
                mapped = Some(prefix);
            }
            if let Some(mapped) = &mut mapped {
                if let Some(actual) = replacement {
                    mapped.push(actual);
                } else {
                    crate::dyn_metadata::semantic_operand_snapshot(image, self.limits, self.left, 1)?;
                    mapped.push(image.clone());
                }
            }
        }
        Ok(mapped.map_or_else(|| std::borrow::Cow::Borrowed(classes), std::borrow::Cow::Owned))
    }
    fn normalized<'v>(
        &mut self,
        scope: SemanticInstanceId,
        value: &'v SemanticTypeValue,
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<std::borrow::Cow<'v, SemanticTypeValue>> {
        let SemanticTypeValue::Resolved { ty, life, classes } = value else {
            return Err(bad("Core type survives normalization"));
        };
        let std::borrow::Cow::Owned(mapped) = self.normalized_images(scope, classes, implicit)? else {
            return Ok(std::borrow::Cow::Borrowed(value));
        };
        crate::dyn_metadata::resolved_geometry_snapshot(ty, life, self.limits, self.left)?;
        Ok(std::borrow::Cow::Owned(SemanticTypeValue::Resolved {
            ty: ty.clone(),
            life: life.clone(),
            classes: mapped,
        }))
    }
    fn normalize(
        &mut self,
        scope: SemanticInstanceId,
        value: &SharedType,
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<SharedType> {
        match self.normalized(scope, value, implicit)? {
            std::borrow::Cow::Borrowed(_) => self.share(value),
            std::borrow::Cow::Owned(value) => {
                self.pay(1)?;
                Ok(Rc::new(value))
            }
        }
    }
    #[track_caller]
    fn same(
        &mut self,
        scope: SemanticInstanceId,
        a: &SemanticTypeValue,
        b: &SemanticTypeValue,
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<()> {
        let location = std::panic::Location::caller();
        let before = *self.left;
        self.same_calls = self.same_calls.saturating_add(1);
        let result = (|| {
            let (
                SemanticTypeValue::Resolved {
                    ty: a_ty,
                    life: a_life,
                    classes: a_classes,
                },
                SemanticTypeValue::Resolved {
                    ty: b_ty,
                    life: b_life,
                    classes: b_classes,
                },
            ) = (a, b)
            else {
                return Err(bad("Core value survives normalization"));
            };
            self.pay(1)?;
            if a_classes.is_empty() && std::ptr::eq(a, b) {
                return Ok(());
            }
            // Rebuild only the class images in this exact live world. The
            // physical type and complete intrinsic domain are immutable and
            // borrowed; an equation never needs two cloned whole values.
            let a_images = self.normalized_images(scope, a_classes, implicit)?;
            if std::ptr::eq(a, b) {
                return Ok(());
            }
            let b_images = self.normalized_images(scope, b_classes, implicit)?;
            if !crate::dyn_metadata::same_resolved_geometry(
                a_ty, a_life, b_ty, b_life, self.limits, self.left,
            )? || !crate::dyn_metadata::same_live_classes(
                &a_images, &b_images, self.limits, self.left,
            )? {
                crate::dyn_metadata::semantic_operand_snapshot(a, self.limits, self.left, 1)?;
                crate::dyn_metadata::semantic_operand_snapshot(b, self.limits, self.left, 1)?;
                let mut diagnostic = bad("actual whole type or rigid original classes disagree");
                diagnostic.notes.push(format!("actual class equation at {}:{}; actual point {:?}; left complete value {:?}; right complete value {:?}; current left images {:?}; current right images {:?}", location.file(), location.line(), self.current_point, a, b, a_images, b_images));
                return Err(diagnostic);
            }
            Ok(())
        })();
        self.same_work = self
            .same_work
            .saturating_add(before.saturating_sub(*self.left));
        result
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
        // The directory construction has already rejected duplicate recipes.
        // Source may also use sparse ordinals, which keep the exact index.
        self.pay(1)?;
        if let Some(recipe) = sites.get(id.0 as usize)
            && recipe.id == id
        {
            return Ok(recipe);
        }
        self.pay(index::search(self.index.recipes.len()))?;
        let entry = self
            .index
            .recipes
            .get(&(original.id, id))
            .ok_or_else(|| bad("actual operation has no original recipe"))?;
        let recipe = sites
            .get(entry.offset)
            .ok_or_else(|| bad("actual recipe index leaves its original definition"))?;
        if recipe.id != id {
            return Err(bad("actual recipe index changes its original identity"));
        }
        Ok(recipe)
    }
    fn point_sites(
        &mut self,
        sites: &[&SemanticSite],
        block: BlockId,
        instruction: usize,
    ) -> R<index::SiteRows<'p>> {
        let Some(first) = sites.first() else {
            return Ok(index::SiteRows::new(&self.program.sites, None));
        };
        self.point_rows(first.instance, block, instruction)
    }
    fn local_declarations(
        &mut self,
        instance: SemanticInstanceId,
        local: LocalId,
    ) -> R<index::SiteRows<'p>> {
        self.pay(index::search(self.index.local_declarations.len()).saturating_add(1))?;
        let positions = self.index.local_declarations.get(&(instance, local)).cloned();
        self.pay(positions.as_ref().map_or(0, |positions| positions.len()))?;
        Ok(index::SiteRows::new(&self.program.sites, positions))
    }
    fn point_rows(
        &mut self,
        instance: SemanticInstanceId,
        block: BlockId,
        instruction: usize,
    ) -> R<index::SiteRows<'p>> {
        let point = (instance, block, instruction);
        self.pay(1)?;
        if !self.last_point_rows.as_ref().is_some_and(|(current, _)| *current == point) {
            let key = (instance, block);
            if !self.last_point_block.as_ref().is_some_and(|(current, _)| *current == key) {
                self.pay(index::search(self.index.points.len()).saturating_add(1))?;
                self.last_point_block = Some((key, self.index.points.get(&key).cloned()));
            }
            let offsets = if let Some((_, Some(points))) = &self.last_point_block {
                index::pay(self.left, index::search(points.len()))?;
                points.get(&instruction).cloned()
            } else {
                None
            };
            self.last_point_rows = Some((point, offsets));
        }
        let offsets = self.last_point_rows.as_ref().and_then(|(_, rows)| rows.clone());
        index::pay(self.left, offsets.as_ref().map_or(0, |rows| rows.len().saturating_add(1)))?;
        Ok(index::SiteRows::new(&self.program.sites, offsets))
    }
    fn constraints(
        &mut self,
        instance: &SemanticInstance,
        original: &SemanticDefinition,
        function: &'p TypedFunction,
        sites: &[&SemanticSite],
        block: BlockId,
        instruction: usize,
        edge: SemanticSiteEdge,
        values: &mut ssa::Values,
        locals: &[Option<SharedType>],
        declared: &mut Rc<BTreeMap<LocalId, SharedType>>,
        implicit: &mut BTreeMap<u32, SemanticClassExpr>,
    ) -> R<()> {
        for site in self.point_sites(sites, block, instruction)? {
            if !matches!(site.at, SemanticSiteAddress::Typed { edge: actual, .. } if actual == edge)
            {
                continue;
            }
            let recipe = self.original_recipe(original, site.recipe)?;
            match (&site.action, &recipe.kind) {
                (SemanticSiteAction::BodyCall { .. }, SemanticSiteRecipeKind::OriginalCall)
                    if edge == SemanticSiteEdge::After =>
                {
                    if let Some((operand, value, source)) = borrow_roles::call_result(
                        self, instance, original, function, site, values, locals, implicit,
                    )? {
                        values.set(self, operand.0 as usize, value)?;
                        let source = roles::incoming(self, operand, source)?;
                        self.value_role_sources.insert(operand, source);
                    }
                }
                (
                    SemanticSiteAction::DeclareLocal {
                        local: SemanticLocalTarget::Typed(local),
                        expected,
                    },
                    SemanticSiteRecipeKind::DeclareLocal { expected: recipe },
                ) => {
                    let logical = self.ledger.logical_local_type(function, *local, self.limits, self.left)?;
                    let persistent = logical.is_persistent_closure();
                    let actual = super::materialize::LogicalExpected::local(logical)
                        .materialize(|hint| self.actual_type(instance.id, recipe, hint))?;
                    self.same(instance.id, expected, &actual, implicit)?;
                    let storage = if persistent {
                        persistent_slots::array(self, &actual)?
                    } else { actual };
                    if let Some(old) = edit_declarations(self, declared)?.insert(*local, storage.clone()) {
                        self.same(instance.id, &old, &storage, implicit)?;
                    }
                }
                (
                    SemanticSiteAction::TypeUse { subject, expected },
                    SemanticSiteRecipeKind::TypeUse {
                        expected: recipe_ty,
                    },
                ) => {
                    let mut value = match subject {
                        SemanticOperand::Value(value) => self.get(values, *value)?,
                        SemanticOperand::Local(local) => {
                            if self.ledger.logical_local_type(function, *local, self.limits, self.left)?.is_persistent_closure() {
                                self.projected_local(locals, *local, &[PlaceProjection::Field(0)])?
                            } else { self.local(locals, *local)? }
                        },
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
                    let original_value = self.logical_type(instance.id, recipe_ty, super::materialize::LogicalExpected::whole(&value)?)?;
                    self.same(instance.id, expected, &original_value, implicit)?;
                    if let SemanticOperand::Value(operand) = subject
                        && let Some((adapted, source)) = borrow_roles::shared_expression_view(
                            self,
                            instance,
                            original,
                            function,
                            sites,
                            site,
                            *operand,
                            &value,
                            &original_value,
                            values,
                            locals,
                            implicit,
                        )?
                    {
                        let retained = self.share(&adapted)?;
                        values.set(self, operand.0 as usize, retained)?;
                        let source = roles::incoming(self, *operand, source)?;
                        self.value_role_sources.insert(*operand, source);
                        value = adapted;
                    }
                    if let SemanticOperand::Value(operand) = subject
                        && let Some((adapted, source)) = borrow_roles::apply(
                            self,
                            instance,
                            original,
                            function,
                            site,
                            *operand,
                            &value,
                            &original_value,
                            values,
                            locals,
                            implicit,
                        )?
                    {
                        let retained = self.share(&adapted)?;
                        values.set(self, operand.0 as usize, retained)?;
                        let source = roles::incoming(self, *operand, source)?;
                        self.value_role_sources.insert(*operand, source);
                        value = adapted;
                    }
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
                    ) = (original_value.as_ref(), value.as_ref())
                    {
                        for (expected, actual) in images.iter().zip(actual_images) {
                            self.pay(1)?;
                            if let SemanticClassExpr::Implicit {
                                scope: SemanticScopeRef::Ancestor(0),
                                ordinal,
                            } = expected
                                && matches!(
                                    actual,
                                    SemanticClassExpr::OuterOccurrence {
                                        operand: SemanticOperand::Value(_),
                                        ..
                                    }
                                )
                            {
                                let mut found = false;
                                for occurrence in &original.implicit_lifetimes {
                                    self.pay(occurrence.occurrence_path.len().saturating_add(1))?;
                                    if occurrence.ordinal == *ordinal
                                        && occurrence.occurrence_path == recipe.occurrence_path
                                    {
                                        found = true;
                                        break;
                                    }
                                }
                                if found {
                                    crate::dyn_metadata::semantic_operand_snapshot(
                                        actual,
                                        self.limits,
                                        self.left,
                                        1,
                                    )?;
                                    self.pay(index::search(implicit.len()).saturating_add(1))?;
                                    if let Some(old) = implicit.insert(*ordinal, actual.clone()) {
                                        crate::dyn_metadata::semantic_operand_snapshot(
                                            &old,
                                            self.limits,
                                            self.left,
                                            1,
                                        )?;
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
            borrow_roles::record_source(self, instance, original, function, site,
                values, locals, implicit)?;
        }
        Ok(())
    }
    fn construction_recipe(
        &mut self,
        original: &SemanticDefinition,
        site: SemanticSiteId,
        recipe: &SemanticTypeRecipe,
    ) -> R<SemanticTypeRecipe> {
        if let Some(SemanticOriginalNode::Expression(node)) =
            occurrences::original_node(self, original.id, site)?
        {
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
        ty: &'p Type,
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<Option<SharedType>> {
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
            let expected = self.actual_type(instance.id, &recipe, ty)?;
            self.same(instance.id, claimed, &expected, implicit)?;
            selected = Some(expected);
        }
        Ok(selected)
    }
    fn constructor(
        &mut self,
        instance: &SemanticInstance,
        original: &SemanticDefinition,
        f: &'p TypedFunction,
        sites: &[&SemanticSite],
        block: BlockId,
        index: usize,
        result: ValueId,
        variant: Option<u32>,
        fields: &[ValueId],
        ct_constructor: Option<&SharedType>,
        values: &ssa::Values,
        locals: &[Option<SharedType>],
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<SharedType> {
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
                let expected = self.actual_type(instance.id, &recipe, &f.values[result.0 as usize])?;
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
                        for (ordinal, ((field, recipe), (name, ty))) in
                            fields.iter().zip(original_fields).zip(actual_fields).enumerate()
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
                            let expected_field = self.physical_type(instance.id, &recipe, ty)?;
                            let value = self.get(values, *field)?;
                            if matches!(Self::type_of(&expected_field)?, Type::Address { .. } | Type::Desc(_))
                                && Self::type_of(&value)? == Self::type_of(&expected_field)?
                                && borrow_roles::call_region_argument(
                                    self, instance, site, ordinal, *field, &expected_field,
                                    values, locals, implicit,
                                )?
                            {
                                // A constructor region changes only this
                                // actual field's outer placement. Keep the
                                // entire pointee domain and original SSA sigma.
                                let actual = self.pointee(&value)?;
                                let expected = self.pointee(&expected_field)?;
                                self.same(instance.id, &actual, &expected, implicit)?;
                                continue;
                            }
                            self.constructor_field_source(
                                instance.id,
                                *field,
                                &value,
                                &expected_field,
                                implicit,
                            )?;
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
                            let expected_field = self.physical_type(instance.id, &recipe, ty)?;
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
        if let Some(expected) = ct_constructor {
            if let Some(ordinary) = &selected {
                self.same(instance.id, ordinary, expected, implicit)?;
            } else {
                ct_data::check_operands(self, instance.id, expected, variant, fields, values, implicit)?;
                selected = Some(self.share(expected)?);
            }
        }
        if selected.is_none()
            && matches!(&f.blocks[block.0 as usize].instructions[index].kind, InstructionKind::MakePackage { .. })
            && let Some(rebuilt) = self.rebuilt_package(instance, f, block, index, result, fields, values, locals, implicit)?
        {
            return Ok(rebuilt);
        }
        if selected.is_none() {
            self.pay(128)?;
        }
        selected.ok_or_else(|| {
            let mut error = bad("actual qualified constructor has no original constructor site");
            error.notes.push(format!(
                "actual constructor: function {:?} block {:?} instruction {} result {:?}, variant {:?}, operands {}",
                f.id, block, index, result, variant, fields.len()
            ));
            error
        })
    }
    #[allow(clippy::too_many_arguments)]
    fn rebuilt_package(
        &mut self,
        instance: &SemanticInstance,
        function: &'p TypedFunction,
        block: BlockId,
        index: usize,
        result: ValueId,
        fields: &[ValueId],
        values: &ssa::Values,
        locals: &[Option<SharedType>],
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<Option<SharedType>> {
        let Some(ty @ Type::Package { fields: declaration, .. }) = function.values.get(result.0 as usize) else {
            return Ok(None);
        };
        let Some(start) = index.checked_sub(fields.len()) else { return Ok(None); };
        if fields.is_empty() || fields.len() != declaration.len() {
            return Ok(None);
        }
        let children = &function.blocks[block.0 as usize].instructions[start..index];
        let mut root = None;
        for (ordinal, ((instruction, operand), (_, field))) in children.iter().zip(fields).zip(declaration).enumerate() {
            self.pay(1)?;
            let (local, selected, take) = match instruction.kind {
                InstructionKind::TakePackageField { local, index } => (local, index, true),
                InstructionKind::ReadPackageField { local, index } => (local, index, false),
                _ => return Ok(None),
            };
            self.pay(inference::type_snapshot_work(field, instruction.span, self.limits)?)?;
            if instruction.result != Some(*operand)
                || selected as usize != ordinal
                || take != owned_shapes::field(field)
                || root.is_some_and(|root| root != local)
            {
                return Ok(None);
            }
            root = Some(local);
        }
        let root = root.expect("checked nonempty reconstruction");
        let parent = locals.get(root.0 as usize).and_then(Option::as_ref)
            .ok_or_else(|| bad("rebuilt package loses its exact local occupant"))?;
        if Self::type_of(parent)? != ty {
            return Err(bad("rebuilt package changes its complete local type"));
        }
        let expected = self.emitted_value(instance, function, block, index, result)?
            .ok_or_else(|| bad("rebuilt package loses its original whole-value observation"))?;
        self.same(instance.id, parent, &expected, implicit)?;
        for (ordinal, operand) in fields.iter().enumerate() {
            let child = self.field_value(parent, ordinal as u32)?;
            let actual = self.get(values, *operand)?;
            self.same(instance.id, &actual, &child, implicit)?;
        }
        Ok(Some(expected))
    }
    fn body_call(
        &mut self,
        instance: &SemanticInstance,
        original: &SemanticDefinition,
        site: &'p SemanticSite,
        actuals: &[SemanticActual],
        target: SemanticDefinitionId,
    ) -> R<()> {
        if original.id != instance.definition {
            return Err(bad("body call changes its original caller definition"));
        }
        let key = calls::Site::new(instance, site, target, actuals)?;
        self.pay(index::search(self.call_proofs.body.len()))?;
        if self.call_proofs.body.contains(&key) {
            return Ok(());
        }
        if !matches!(
            self.original_recipe(original, site.recipe)?.kind,
            SemanticSiteRecipeKind::OriginalCall
        ) {
            return Err(bad("actual body call changes original operation kind"));
        }
        let ledger = self.ledger;
        let proof = ledger.original.borrow();
        let proof = proof.as_ref().ok_or_else(|| bad("body call lacks complete original Gamma"))?;
        self.pay(index::search(proof.call_targets.len()) + index::search(proof.calls.len()))?;
        let coordinate = (original.id, site.recipe);
        if proof.call_targets.get(&coordinate) != Some(&target) {
            return Err(bad("actual body call selects another original target"));
        }
        let recipes = proof.calls.get(&coordinate)
            .ok_or_else(|| bad("body call lacks original fresh actuals"))?;
        if recipes.len() != actuals.len() {
            return Err(bad("body call omits mixed actuals"));
        }
        for (ordinal, (recipe, actual)) in recipes.iter().zip(actuals).enumerate() {
            let expected = super::materialize::call_actual_in_view(
                self.program,
                &self.view,
                self.ledger,
                instance.id,
                site.recipe,
                ordinal as u32,
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
        self.pay(index::search(self.call_proofs.body.len()).saturating_add(1))?;
        self.call_proofs.body.insert(key);
        Ok(())
    }
    fn requirements(
        &mut self,
        instance: &SemanticInstance,
        original: &SemanticDefinition,
        actuals: Option<&[SemanticActual]>,
        source: Option<&'p SemanticSite>,
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<()> {
        let before = *self.left;
        let ledger = self.ledger;
        match (actuals, source) {
            (Some(actuals), Some(site)) => {
                let key = calls::Site::new(instance, site, original.id, actuals)?;
                self.pay(index::search(self.call_proofs.requirements.len()))?;
                if !self.call_proofs.requirements.contains(&key) {
                    let proof = ledger.original.borrow();
                    let proof = proof.as_ref()
                        .ok_or_else(|| bad("where requirement lacks independent original Gamma"))?;
                    let recipes = proof.where_call(
                        self.program, instance.definition, site.recipe, original.id,
                        self.limits, self.left,
                    )?;
                    if recipes.len() != actuals.len() {
                        return Err(bad("actual where boundary changes full mixed arity"));
                    }
                    // where_call returns the same original inferred recipe
                    // vector that body_call proved at this exact occurrence.
                    self.pay(index::search(self.call_proofs.body.len()))?;
                    if !self.call_proofs.body.contains(&key) {
                        for (ordinal, (recipe, actual)) in recipes.iter().zip(actuals).enumerate() {
                            let expected = super::materialize::call_actual_in_view(
                                self.program, &self.view, ledger, instance.id, site.recipe,
                                ordinal as u32, recipe, self.limits, self.left,
                            )?;
                            crate::dyn_metadata::semantic_operand_snapshot(actual, self.limits, self.left, 1)?;
                            crate::dyn_metadata::semantic_operand_snapshot(&expected, self.limits, self.left, 1)?;
                            if actual != &expected {
                                return Err(bad(
                                    "actual where boundary changes independently solved original mixed argument",
                                ));
                            }
                        }
                    }
                    self.pay(index::search(self.call_proofs.requirements.len()).saturating_add(1))?;
                    self.call_proofs.requirements.insert(key);
                }
            }
            (None, None) => ledger.original.borrow().as_ref()
                .ok_or_else(|| bad("where requirement lacks independent original Gamma"))?
                .where_entry(self.program, original.id, self.limits, self.left)?,
            _ => {
                return Err(bad(
                    "actual where boundary omits its exact original call site or arguments",
                ));
            }
        }
        self.where_work = self.where_work.saturating_add(before.saturating_sub(*self.left));
        // Original Gamma establishes the entire requirement grammar in the
        // caller domain. Independently retain the actual rigid Life equality
        // check when both sides select full mixed arguments directly.
        self.pay(original.requirements.len())?;
        if let Some(actuals) = actuals {
            for requirement in &original.requirements {
                if let SemanticRequirementRecipe::LifeEqual {
                    left:
                        SemanticLifeRecipe::Formal(SemanticFormalRef {
                            scope: SemanticScopeRef::Ancestor(0),
                            ordinal: a,
                        }),
                    right:
                        SemanticLifeRecipe::Formal(SemanticFormalRef {
                            scope: SemanticScopeRef::Ancestor(0),
                            ordinal: b,
                        }),
                } = requirement
                {
                    let (Some(SemanticActual::Life(left)), Some(SemanticActual::Life(right))) =
                        (actuals.get(*a as usize), actuals.get(*b as usize))
                    else {
                        return Err(bad("where Life equality selects a non-Life actual"));
                    };
                    self.same_class(instance.id, left, right, implicit)?;
                }
            }
        }
        Ok(())
    }
}

impl<'p> World<'p, '_> {
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
            let (d, child) = self.child_at(closure, instance.id)?;
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
        let receiver_head = super::headers::receiver_head(
            self.program, self.ledger, instance.definition, self.limits, self.left)?;
        if super::headers::receiver_binding(&mut expected_bindings, receiver_head, self.limits, self.left)? {
            let closure = f.environment.ok_or_else(|| bad("receiver header is outside a real closure entry"))?;
            let definition = self.closure_definition(closure)?;
            if !matches!(definition.kind, CallableKind::Fn | CallableKind::FnMut) {
                return Err(bad("consuming environment cannot publish a lending receiver head"));
            }
            if f.call_kind != CallKind::Direct {
                return Err(bad("suspending entry cannot publish a receiver access head"));
            }
        }
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
        values: &ssa::Values,
        locals: &[Option<SharedType>],
        implicit: &BTreeMap<u32, SemanticClassExpr>,
    ) -> R<SharedType> {
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(caller_id)) = instance.target else {
            return Err(bad("callable invocation has no exact actual caller"));
        };
        let caller = self.function(caller_id)?;
        self.pay(caller.blocks.len())?;
        let output = caller.blocks.iter().find(|row| row.id == block)
            .and_then(|row| row.instructions.get(index)).and_then(|op| op.result);
        if let Some(output) = output {
            // A repeated CFG visit must not reuse the previous invocation's
            // receiver occupant. The actual operation may install fresh roots.
            self.value_roots.remove(&output);
            self.value_role_sources.remove(&output);
        }
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
        let (definition, children) = self.child_worlds(*id)?;
        // Code sharing does not share a source world. Every real child is
        // checked from the same caller prestate, with its own original Gamma,
        // full header and complete Fixed/P/R mapping. No candidate contributes
        // a permission or a value fact to the next candidate's proof.
        let witness_base = self.argument_witnesses.len();
        let adaptation_base = self.receiver_adaptations.len();
        let mut witnesses = Vec::new();
        let mut adaptations: Option<Vec<receiver::Adaptation>> = None;
        let mut result: Option<SharedType> = None;
        let mut facts = None;
        for child in children {
            self.pay(index::search(self.value_role_sources.len().saturating_add(1))
                .saturating_add(index::search(self.value_roots.len().saturating_add(1)))
                .saturating_mul(2).saturating_add(1))?;
            let checked = self.invoke_world(instance, original, sites, block, index, &operand,
                &raw_value, &value, requested, arguments, values, locals, implicit, definition, child);
            if let Err(error) = checked {
                self.argument_witnesses.truncate(witness_base);
                self.receiver_adaptations.truncate(adaptation_base);
                if let Some(output) = output {
                    self.value_role_sources.remove(&output);
                    self.value_roots.remove(&output);
                }
                return Err(error);
            }
            let value = checked?;
            let current_facts = output.map(|output| (
                self.value_role_sources.remove(&output), self.value_roots.remove(&output),
            )).unwrap_or((None, None));
            let count = self.argument_witnesses.len().saturating_sub(witness_base);
            self.pay(witnesses.len().saturating_add(count))?;
            witnesses.extend(self.argument_witnesses.split_off(witness_base));
            let count = self.receiver_adaptations.len().saturating_sub(adaptation_base);
            self.pay(count)?;
            let current_adaptations = self.receiver_adaptations.split_off(adaptation_base);
            if let Some(previous) = &adaptations {
                self.same_invocation_adaptations(previous, &current_adaptations)?;
            } else { adaptations = Some(current_adaptations); }
            if let Some(previous) = &result {
                self.same(instance.id, previous, &value, implicit)?;
            } else { result = Some(value); }
            if let Some(previous) = &facts {
                self.same_invocation_facts(previous, &current_facts)?;
            } else { facts = Some(current_facts); }
        }
        let result = result.ok_or_else(|| bad("callable has no fully checked actual child worlds"))?;
        // Retain every world's independent audit receipt (including child.id).
        // The caller result and output facts are committed once, after all
        // mapped worlds satisfy the same complete actual boundary contract.
        self.pay(self.argument_witnesses.len().saturating_add(witnesses.len()))?;
        self.argument_witnesses.extend(witnesses);
        let adaptations = adaptations.ok_or_else(|| bad("callable omits its checked adaptation inventory"))?;
        self.pay(self.receiver_adaptations.len().saturating_add(adaptations.len()))?;
        self.receiver_adaptations.extend(adaptations);
        if let Some(output) = output {
            let (role, roots) = facts.ok_or_else(|| bad("callable omits its checked output facts"))?;
            self.pay(index::search(self.value_role_sources.len().saturating_add(1))
                .saturating_add(index::search(self.value_roots.len().saturating_add(1))))?;
            if let Some(role) = role { self.value_role_sources.insert(output, role); }
            if let Some(roots) = roots { self.value_roots.insert(output, roots); }
        }
        Ok(result)
    }
    fn same_invocation_facts(
        &mut self,
        first: &(Option<Rc<roles::Incoming>>, Option<arguments::SharedRoots>),
        other: &(Option<Rc<roles::Incoming>>, Option<arguments::SharedRoots>),
    ) -> R<()> {
        for role in [&first.0, &other.0].into_iter().flatten() {
            flow::charge_incoming(self, role)?;
        }
        for roots in [&first.1, &other.1].into_iter().flatten() {
            self.pay(roots.len())?;
            for root in roots.iter() { self.pay(root.path.len())?; }
        }
        if first != other { return Err(bad("actual callable worlds disagree in their complete output source facts")); }
        Ok(())
    }
    fn same_invocation_adaptations(
        &mut self, first: &[receiver::Adaptation], other: &[receiver::Adaptation],
    ) -> R<()> {
        self.pay(first.len().saturating_add(other.len()))?;
        if first.len() != other.len() { return Err(bad("actual callable worlds disagree in their directed adaptation inventory")); }
        for (a, b) in first.iter().zip(other) {
            roles::snapshot_source(&a.source, self.limits, self.left)?;
            roles::snapshot_source(&b.source, self.limits, self.left)?;
            crate::dyn_metadata::semantic_operand_snapshot(&a.target, self.limits, self.left, 1)?;
            crate::dyn_metadata::semantic_operand_snapshot(&b.target, self.limits, self.left, 1)?;
            if a.caller != b.caller || a.site != b.site || a.operand != b.operand
                || a.source != b.source || a.target != b.target {
                return Err(bad("actual callable worlds disagree in an exact directed adaptation"));
            }
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn invoke_world(
        &mut self, instance: &SemanticInstance, original: &SemanticDefinition,
        sites: &[&SemanticSite], block: BlockId, index: usize,
        operand: &SemanticOperand, raw_value: &SharedType, value: &SharedType,
        requested: CallableKind, arguments: &[ValueId], values: &ssa::Values,
        locals: &[Option<SharedType>], implicit: &BTreeMap<u32, SemanticClassExpr>,
        d: &ClosureDefinition, child: &SemanticInstance,
    ) -> R<SharedType> {
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
                    self, instance, original, site, operand, raw_value, value, d, child, target,
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
                if actual != operand || selected {
                    return Err(bad("callable call changes its actual operand"));
                }
                let recipe = self.original_recipe(original, site.recipe)?;
                let SemanticSiteRecipeKind::CallableCall { callee: expected } = &recipe.kind else {
                    return Err(bad("callable call changes original recipe kind"));
                };
                let expected = self.physical_type(instance.id, expected, Self::type_of(&value)?)?;
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
                self.parameter_value(child.id, ordinal, types::Hint::Actual(&target.locals[local.0 as usize].ty))?;
            let expected = self.map_value(&expected, &[], Some((header, child, value, StorageBase::Parameter(ordinal as u32))))?;
            let actual = self.get(values, *argument)?;
            self.same(instance.id, &actual, &expected, implicit)?;
        }
        let result = self.result_value(child.id, types::Hint::Actual(&target.return_type))?;
        self.map_value(&result, &[], Some((header, child, value, StorageBase::Result)))
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
        self.requirements(instance, original, None, None, &BTreeMap::new())?;
        let sites = self.instance_sites(instance.id)?;
        self.pay(f.locals.len())?;
        let mut locals = vec![None; f.locals.len()];
        let values = ssa::Values::new(self, f.values.len())?;
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
            let value = self.parameter_value(instance.id, ordinal, types::Hint::Actual(ty))?;
            self.pay(2)?;
            locals[parameter.0 as usize] = Some(value.clone());
            declared.insert(*parameter, value);
        }
        let expected_result = self.result_value(instance.id, types::Hint::Actual(&f.return_type))?;
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
        let mut cfg = flow::Cfg::new(self, instance, f)?;
        cfg.check_sites(self, f, &sites)?;
        let ct_constructors = ct_data::constructors(self, &cfg, instance, original, f, &sites, &implicit)?;
        let mut incoming_roots = BTreeMap::new();
        self.pay(f.parameters.len().saturating_mul(2))?;
        for local in &f.parameters {
            if matches!(
                stored::physical(&f.locals[local.0 as usize].ty),
                Type::Address { .. } | Type::Desc(_)
            ) {
                self.pay(1)?;
                incoming_roots.insert(
                    *local,
                    Rc::new(std::collections::BTreeSet::from([arguments::Root {
                        local: *local,
                        path: Vec::new(),
                    }])),
                );
            }
        }
        let initial = flow::State {
            schema: f.id,
            instance: instance.id,
            locals: Rc::new(locals),
            values,
            declared: Rc::new(declared),
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
            reservations: BTreeMap::new(),
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
                reservations,
                value_roots,
                local_roots,
            } = cfg.take_entry(self, block_index, state_index, &mut reaching)?;
            self.vacant_captures = vacant_captures;
            self.stale_values = stale_values;
            self.stale_locals = stale_locals;
            self.vacant = vacant;
            self.role_sources = role_sources;
            self.value_role_sources = value_role_sources;
            self.reborrows = reborrows;
            self.reservations = reservations;
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
                    &mut values,
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
                let mut propagated_role = propagated_role
                    .map(|source| roles::share_incoming(source, self.left))
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
                            for root in roots.iter() {
                                *self.left =
                                    self.left.checked_sub(root.path.len()).ok_or_else(|| {
                                        Diagnostic::limit(
                                            Phase::Types,
                                            "local argument root path work exceeded",
                                            None,
                                        )
                                    })?;
                            }
                            propagated_roots = roots.as_ref().clone();
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
                    | InstructionKind::DescribeAddress { address }
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
                                &locals,
                                &implicit,
                            )?)
                        }
                        InstructionKind::Load(local) => Some(self.local(&locals, *local)?),
                        InstructionKind::TakeLocal { local } => {
                            let value = self.local(&locals, *local)?;
                            self.clear_vacant(*local)?;
                            self.role_sources.remove(local);
                            self.local_roots.remove(local);
                            set_local(self, &mut locals, *local, None)?;
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
                            let adapted = if let Some(view) = borrow_roles::shared_binding_view(
                                self, instance, original, f, &sites, *local, operand, &value,
                                &values, &locals, &implicit,
                            )? {
                                Some(view)
                            } else {
                                roles::store(
                                    self, instance, original, *local, operand, &value,
                                    &implicit,
                                )?
                            };
                            let source = if let Some((adapted, source)) = adapted {
                                value = adapted;
                                Some(roles::incoming(self, operand, source)?)
                            } else {
                                self.value_role_sources.get(&operand)
                                    .map(|source| roles::share_incoming(source, self.left))
                                    .transpose()?
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
                                self.pay(1)?;
                                self.local_roots.insert(*local, Rc::new(stored_roots));
                            }
                            set_local(self, &mut locals, *local, Some(value))?;
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
                            if declared.contains_key(local) {
                                edit_declarations(self, &mut declared)?.remove(local);
                            }
                            self.stale_locals.remove(local);
                            self.clear_vacant(*local)?;
                            self.role_sources.remove(local);
                            self.local_roots.remove(local);
                            set_local(self, &mut locals, *local, None)?;
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
                        InstructionKind::ReservePlaceLocal {
                            reservation,
                            local,
                            path,
                        } => {
                            reservations::reserve(
                                self,
                                f,
                                *reservation,
                                reservations::Target::Local(*local),
                                path,
                                &values,
                                &locals,
                            )?;
                            None
                        }
                        InstructionKind::ReservePlaceAddress {
                            reservation,
                            address,
                            path,
                        } => {
                            reservations::reserve(
                                self,
                                f,
                                *reservation,
                                reservations::Target::Address(*address),
                                path,
                                &values,
                                &locals,
                            )?;
                            None
                        }
                        InstructionKind::CommitPlaceReplace { reservation, value } => {
                            self.pay(index::search(self.reservations.len()))?;
                            let work = self.reservations.get(reservation).map_or(0, |row|
                                row.roots.iter().fold(1usize, |work, root|
                                    work.saturating_add(root.path.len().saturating_add(1))));
                            self.pay(work.saturating_add(index::search(self.reservations.len())))?;
                            let roots = self.reservations.get(reservation).map(|row| row.roots.clone());
                            reservations::commit(
                                self,
                                instance,
                                f,
                                *reservation,
                                *value,
                                &values,
                                &locals,
                                &implicit,
                            )?;
                            if let Some(roots) = roots { roles::invalidate_roots(self, &roots)?; }
                            None
                        }
                        InstructionKind::AbortPlaceReservation { reservation } => {
                            reservations::abort(self, *reservation)?;
                            None
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
                            let image = {
                                let ledger = self.ledger;
                                let proof = ledger.original.borrow();
                                let proof = proof.as_ref().ok_or_else(|| bad("inline place borrow lacks its original Gamma"))?;
                                self.pay(index::search(proof.pattern_borrows.len()))?;
                                match proof.pattern_borrows.get(&(original.id, instruction.span)) {
                                    Some((ordinal, mode)) if mode == mutable => SemanticClassExpr::Implicit {
                                        scope: SemanticScopeRef::Ancestor(0), ordinal: *ordinal,
                                    },
                                    Some(_) => return Err(bad("actual reference pattern changes its original access mode")),
                                    None => self.occurrence(f, result)?,
                                }
                            };
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
                            let static_source = matches!(source, roles::Source::Static);
                            self.pay(1)?;
                            self.reborrows.insert(
                                result,
                                Rc::new(arguments::Edge {
                                    parent: *address,
                                    source,
                                }),
                            );
                            // The actual parent SSA has already been checked;
                            // its Static source is not inferred from a proposed
                            // type or from a zero class count. Preserve this
                            // source through a shared view, with the complete
                            // child domain and the real reborrow receipt above.
                            // Variable sources still get a fresh occurrence.
                            if static_source {
                                if !matches!(ty, Some(Type::Address { mutable: false, .. })) {
                                    return Err(bad(
                                        "static reborrow cannot provide an exclusive outer mode",
                                    ));
                                }
                                Some(self.address_projection(
                                    ty.ok_or_else(|| bad("static reborrow has no complete type"))?,
                                    &parent,
                                    &child,
                                )?)
                            } else {
                                let image = self.occurrence(f, result)?;
                                Some(self.address(
                                    ty.ok_or_else(|| bad("reborrow has no type"))?,
                                    image,
                                    &child,
                                )?)
                            }
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
                                            )
                                            || stored::shared_compound(
                                                source,
                                                self.left,
                                                self.limits,
                                                instruction.span,
                                            )?) =>
                                {
                                    // This is a complete shared value view,
                                    // not a Copy bound or an owner transfer.
                                    // Keep the exact checked capture value:
                                    // every leaf's source and full class domain
                                    // remain unchanged by the actual read.
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
                            let (definition, child) = self.child_at(closure, instance.id)?;
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
                            let operand = *value;
                            let value = self.get(&values, operand)?;
                            if !borrow_roles::capture_write(self, instance, original, f, *index, operand,
                                &value, &expected, &values, &locals, &implicit)? {
                                self.same(instance.id, &value, &expected, &implicit)?;
                            }
                            self.vacant_captures.remove(&(instance.id, *index));
                            None
                        }
                        InstructionKind::TakeCapture { index } => {
                            let closure = f
                                .environment
                                .ok_or_else(|| bad("capture transfer has no active environment"))?;
                            let (definition, child) = self.child_at(closure, instance.id)?;
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
                            let (definition, child) = self.child_at(closure, instance.id)?;
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
                                let result = instruction.result
                                    .ok_or_else(|| bad("constructor has no result"))?;
                                self.pay(index::search(ct_constructors.len()))?;
                                Some(
                                    self.constructor(
                                        instance,
                                        original,
                                        f,
                                        &sites,
                                        block.id,
                                        index,
                                        result,
                                        match instruction.kind {
                                            InstructionKind::MakeVariant { variant, .. } => {
                                                Some(variant)
                                            }
                                            _ => None,
                                        },
                                        fields,
                                        ct_constructors.get(&result),
                                        &values,
                                        &locals,
                                        &implicit,
                                    )?,
                                )
                            } else {
                                let actual_ty = ty.ok_or_else(|| bad("aggregate has no type"))?;
                                let result = instruction
                                    .result
                                    .ok_or_else(|| bad("aggregate has no actual result"))?;
                                let slot = persistent_slots::constructor(
                                    self, f, block.id, index, fields, &values,
                                )?;
                                let expected = match if slot.is_some() { slot } else { self.constructed_value(
                                    instance, original, &sites, block.id, index, result, actual_ty,
                                    &implicit,
                                )? } {
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
                                            arguments::tuple_field(
                                                self, instance.id, *field, &target, &values, &implicit,
                                            )?;
                                        }
                                    }
                                    Type::Array { element, length }
                                        if *length as usize == fields.len() =>
                                    {
                                        for (index, field) in fields.iter().enumerate() {
                                            let target = projection::place(self, &expected, PlaceProjection::Field(index as u32))?;
                                            if Self::type_of(&target)? != element.as_ref() {
                                                return Err(bad("array constructor changes its complete element type"));
                                            }
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
                            let (d, child) = self.created_child(*closure, instance.id)?;
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
                                        let source = super::materialize::actual_in_view(
                                            self.program,
                                            &self.view,
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
                                        let value = super::materialize::actual_in_view(
                                            self.program,
                                            &self.view,
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
                                    selected = Some(site);
                                }
                            }
                            let creation_site = selected.ok_or_else(|| bad("actual closure lacks its original creation site"))?;
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
                                            } = actual.as_ref()
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
                                    Some((header, child, &closure_value, StorageBase::Capture(capture as u32))),
                                )?;
                                let actual = self.get(&values, *source)?;
                                arguments::capture(self, instance, child, creation_site, capture as u32, *source, &actual, &expected, &values, &locals, &implicit)?;
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
                            let (callee, call_site) = sourcecall::resolve(
                                self, instance, *target, &sites, block.id, index,
                            )?;
                            let target_function = self.function(*target)?;
                            let target_definition = self.definition(callee.definition)?;
                            protocol::header(self, target_definition, target_function)?;
                            calls::abi(
                                self,
                                original,
                                target_function,
                                call_site,
                                &instruction.kind,
                                &unsafe_regions,
                            )?;
                            if let Some(result) = methods::evaluate(
                                self,
                                instance,
                                original,
                                callee,
                                target_function,
                                call_site,
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
                                call_site,
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
                                let site = call_site;
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
                                            let expected = super::materialize::actual_in_view(
                                                self.program,
                                                &self.view,
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
                                    _ => return Err(bad("actual call has no original full-actual site")),
                                };
                                if actuals.len() != target_original.parameters.len() {
                                    return Err(bad(
                                        "call has incomplete actual world",
                                    ));
                                }
                                {
                                    self.requirements(
                                        instance,
                                        target_original,
                                        Some(actuals),
                                        Some(site),
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
                                            types::Hint::Actual(&target_function.locals[local.0 as usize].ty),
                                        )?;
                                        let expected =
                                            self.map_value(&expected, actuals, None)?;
                                        let value = self.get(&values, *argument)?;
                                        self.same(instance.id, &value, &expected, &implicit)?;
                                    }
                                    let result = self.result_value(
                                        callee.id,
                                        types::Hint::Actual(&target_function.return_type),
                                    )?;
                                    Some(self.map_value(&result, actuals, None)?)
                                }
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
                        } | InstructionKind::TakeOwnedArrayElement { local, index: field } => {
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
                        } | InstructionKind::StoreOwnedArrayElement { local, index: field, value } => {
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
                        } | InstructionKind::DropOwnedArrayElement { local, index: field } => {
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
                                self.role_sources.remove(target);
                                self.local_roots.remove(target);
                                set_local(self, &mut locals, *target, Some(field))?;
                            }
                            self.clear_vacant(*local)?;
                            self.role_sources.remove(local);
                            self.local_roots.remove(local);
                            set_local(self, &mut locals, *local, None)?;
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
                            set_local(self, &mut locals, *local, None)?;
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
                                        error_injection::call_operand(
                                            self, instance, f, &sites, result, *is_error, actual_ty,
                                        )?.ok_or_else(|| pending(
                                            "error union constructor lacks its original complete result equation",
                                        ))?
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
                            let result_type =
                                ty.ok_or_else(|| bad("local description has no type"))?;
                            if !matches!(result_type, Type::Desc(_)) {
                                return Err(bad(
                                    "description opcode has another actual outer constructor",
                                ));
                            }
                            let image = self.occurrence(f, result)?;
                            Some(self.address(result_type, image, &value)?)
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
                            let result_type = ty.ok_or_else(|| {
                                bad("actual package description has no complete result type")
                            })?;
                            if !matches!(result_type, Type::Desc(_)) {
                                return Err(bad(
                                    "description opcode has another actual outer constructor",
                                ));
                            }
                            let image = self.occurrence(f, result)?;
                            Some(self.address(result_type, image, &payload)?)
                        }
                        InstructionKind::DescribeAddress { address } => {
                            let parent = self.get(&values, *address)?;
                            // A description may be observed through a shared
                            // head. It retains that real parent loan/root; its
                            // physical Desc shape grants no write authority.
                            places::address_mutable(self, &parent, false)?;
                            let owner = self.pointee(&parent)?;
                            if !matches!(Self::type_of(&owner)?, Type::Managed(_)) {
                                return Err(bad(
                                    "actual addressed description has no owning carrier",
                                ));
                            }
                            let child = projection::pointee(self, &owner)?;
                            let result = instruction.result.ok_or_else(|| {
                                bad("addressed description has no actual result occurrence")
                            })?;
                            let result_type =
                                ty.ok_or_else(|| bad("addressed description has no type"))?;
                            if !matches!(result_type, Type::Desc(_)) {
                                return Err(bad(
                                    "description opcode has another actual outer constructor",
                                ));
                            }
                            let image = self.occurrence(f, result)?;
                            Some(self.address(result_type, image, &child)?)
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
                            self.role_sources.remove(local);
                            self.local_roots.remove(local);
                            self.stale_locals.remove(local);
                            set_local(self, &mut locals, *local, Some(value))?;
                            None
                        }
                        InstructionKind::EndPackageCapture { local, .. } => {
                            self.clear_vacant(*local)?;
                            self.role_sources.remove(local);
                            self.local_roots.remove(local);
                            set_local(self, &mut locals, *local, None)?;
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
                        InstructionKind::LoadStoredBorrow {
                            storage: source_storage,
                            path,
                            mode,
                        } => {
                            let storage = self.get(&values, *source_storage)?;
                            let Type::Address { mutable, inner } = Self::type_of(&storage)? else {
                                return Err(bad("stored leaf read has no actual storage address"));
                            };
                            let task_inputs = *mutable
                                && path.as_slice() == [PlaceProjection::Field(1)]
                                && crate::tasks::failure::is_carrier(inner);
                            let mut selected = self.pointee(&storage)?;
                            for step in path {
                                selected = projection::place(self, &selected, *step)?;
                            }
                            let mut leaf_roots = std::collections::BTreeSet::new();
                            for mut root in arguments::roots(self, *source_storage, &storage)? {
                                self.pay(root.path.len().saturating_add(path.len()))?;
                                root.path.extend_from_slice(path);
                                if root.path.len() > self.limits.max_call_depth
                                    || self.stale_locals.contains(&root.local)
                                    || locals
                                        .get(root.local.0 as usize)
                                        .and_then(Option::as_ref)
                                        .is_none()
                                {
                                    return Err(bad(
                                        "stored leaf read has an inactive or excessive actual root",
                                    ));
                                }
                                self.pay(
                                    self.vacant
                                        .len()
                                        .saturating_mul(root.path.len().saturating_add(1)),
                                )?;
                                if self.vacant.iter().any(|(local, vacant)| {
                                    *local == root.local
                                        && (root.path.starts_with(vacant)
                                            || vacant.starts_with(&root.path))
                                }) {
                                    return Err(bad(
                                        "stored leaf read uses a vacant actual projection",
                                    ));
                                }
                                leaf_roots.insert(root);
                            }
                            match (mode, Self::type_of(&selected)?) {
                                (
                                    StoredBorrowLoad::CopyShared,
                                    Type::Address { mutable: false, .. },
                                )
                                | (StoredBorrowLoad::CopyDescription, Type::Desc(_))
                                | (StoredBorrowLoad::Take, Type::Address { .. } | Type::Desc(_)) => {
                                }
                                (StoredBorrowLoad::Take, ty)
                                    if task_inputs && matches!(crate::stored::physical(ty), Type::Tuple(_)) => {}
                                (
                                    StoredBorrowLoad::Reborrow { mutable },
                                    Type::Address {
                                        mutable: available, ..
                                    },
                                ) if !*mutable || *available => {
                                    let inner = self.pointee(&selected)?;
                                    // This is the stored leaf's actual outer
                                    // source, not the storage address's source.
                                    // Selecting a different referent invalidates
                                    // any parent original-expression receipt;
                                    // record_source forms this leaf's own full
                                    // field trace at its real expression boundary.
                                    self.pay(1)?;
                                    propagated_role = Some(Rc::new(roles::Incoming {
                                        source: roles::outer(self, &selected)?,
                                        origin: None,
                                    }));
                                    places::borrow_mode(ty, *mutable)?;
                                    let result = instruction.result.ok_or_else(|| {
                                        bad("stored reborrow has no actual result")
                                    })?;
                                    let image = self.occurrence(f, result)?;
                                    selected = self.address(
                                        ty.ok_or_else(|| {
                                            bad("stored reborrow has no actual type")
                                        })?,
                                        image,
                                        &inner,
                                    )?;
                                    propagated_roots = leaf_roots.clone();
                                }
                                _ => {
                                    return Err(bad(
                                        "stored leaf read changes its exact access mode",
                                    ));
                                }
                            }
                            if *mode == StoredBorrowLoad::Take {
                                for root in leaf_roots {
                                    self.vacant.insert((root.local, root.path));
                                }
                            }
                            // The complete selected leaf carries the retained
                            // input class, independently of the storage loan.
                            Some(selected)
                        }
                        InstructionKind::StoreStoredBorrow {
                            storage,
                            path,
                            value,
                        } => {
                            let parent = self.get(&values, *storage)?;
                            places::address_mutable(self, &parent, true)?;
                            let mut selected = self.pointee(&parent)?;
                            for step in path {
                                selected = projection::place(self, &selected, *step)?;
                            }
                            if !matches!(
                                Self::type_of(&selected)?,
                                Type::Address { .. } | Type::Desc(_)
                            ) {
                                return Err(bad(
                                    "stored leaf write does not select an actual borrowed leaf",
                                ));
                            }
                            let actual = self.get(&values, *value)?;
                            self.same(instance.id, &actual, &selected, &implicit)?;
                            for mut root in arguments::roots(self, *storage, &parent)? {
                                self.pay(root.path.len().saturating_add(path.len()))?;
                                root.path.extend_from_slice(path);
                                if root.path.len() > self.limits.max_call_depth
                                    || self.stale_locals.contains(&root.local)
                                    || locals
                                        .get(root.local.0 as usize)
                                        .and_then(Option::as_ref)
                                        .is_none()
                                {
                                    return Err(bad(
                                        "stored leaf write has an inactive or excessive actual root",
                                    ));
                                }
                                self.pay(
                                    self.vacant
                                        .len()
                                        .saturating_mul(root.path.len().saturating_add(1)),
                                )?;
                                if self.vacant.iter().any(|(local, vacant)| {
                                    *local == root.local
                                        && vacant != &root.path
                                        && (root.path.starts_with(vacant)
                                            || vacant.starts_with(&root.path))
                                }) {
                                    return Err(bad(
                                        "stored leaf write changes a different vacant parent projection",
                                    ));
                                }
                                self.vacant.remove(&(root.local, root.path));
                            }
                            None
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
                                    set_local(self, &mut locals, *local, None)?;
                                    if declared.contains_key(local) {
                                        edit_declarations(self, &mut declared)?.remove(local);
                                    }
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
                roles::invalidate_operation(self, &instruction.kind, &values)?;
                if let Some(result) = instruction.result {
                    let value = produced
                        .ok_or_else(|| bad("actual SSA producer has no complete class equation"))?;
                    if matches!(instruction.kind,
                        InstructionKind::CallClosure { .. } | InstructionKind::CallClosureOnce { .. }
                        | InstructionKind::CallClosureShared { .. } | InstructionKind::CallClosureMut { .. }
                        | InstructionKind::CallClosureBorrowed { .. })
                        && let Some(roots) = self.value_roots.remove(&result)
                    {
                        propagated_roots = match Rc::try_unwrap(roots) {
                            Ok(roots) => roots,
                            Err(roots) => arguments::copy_roots(self, &roots)?,
                        };
                    }
                    if matches!(instruction.kind,
                        InstructionKind::CallClosure { .. } | InstructionKind::CallClosureOnce { .. }
                        | InstructionKind::CallClosureShared { .. } | InstructionKind::CallClosureMut { .. }
                        | InstructionKind::CallClosureBorrowed { .. })
                        && let Some(source) = self.value_role_sources.remove(&result)
                    {
                        propagated_role = Some(source);
                    }
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
                        self.pay(1)?;
                        self.value_roots.insert(result, Rc::new(propagated_roots));
                    }
                    self.value_role_sources.remove(&result);
                    if let Some(source) = propagated_role {
                        self.value_role_sources.insert(result, source);
                    }
                    values.set(self, result.0 as usize, value)?;
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
                    &mut values,
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
                &mut values,
                &locals,
                &mut declared,
                &mut implicit,
            )?;
            let successors = match block.terminator {
                Terminator::Return(Some(result)) => {
                    reservations::exit(self)?;
                    places::capture_exit(self, instance, f)?;
                    let value = self.get(&values, result)?;
                    if !borrow_roles::return_view(
                        self,
                        instance,
                        original,
                        f,
                        &sites,
                        result,
                        &value,
                        &expected_result,
                        &values,
                        &locals,
                        &implicit,
                    )? {
                        if Self::type_of(&value)? != &f.return_type
                            && task_factories::result_task(&f.return_type).is_some() {
                            crate::dyn_metadata::semantic_operand_snapshot(value.as_ref(), self.limits, self.left, 1)?;
                            let mut projected = value.as_ref().clone();
                            let SemanticTypeValue::Resolved {ty, ..} = &mut projected else {
                                return Err(bad("Task return lost its resolved constructor value"));
                            };
                            if !task_factories::emitted_projection(block, &f.parameters, &f.locals,
                                result, ty, |n| self.pay(n))? {
                                return Err(bad("Task return lacks its actual worker-to-factory mapping"));
                            }
                            self.same(instance.id, &projected, &expected_result, &implicit)?;
                        } else {
                            self.same(instance.id, &value, &expected_result, &implicit)?;
                        }
                    }
                    Vec::new()
                }
                Terminator::Return(None) if f.return_type == Type::Unit => {
                    reservations::exit(self)?;
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
                reservations: std::mem::take(&mut self.reservations),
                value_roots: std::mem::take(&mut self.value_roots),
                local_roots: std::mem::take(&mut self.local_roots),
            };
            // The final outgoing edge owns this complete state. Only real
            // sibling edges need an independent live state with shared immutable
            // value trees; no consumer uses the original after the last transfer.
            // Each offer still applies
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
        protocol_flow::check(self, &cfg, instance, f)?;
        Ok(())
    }
}
fn set_local(
    world: &mut World<'_, '_>,
    locals: &mut Rc<Vec<Option<SharedType>>>,
    local: LocalId,
    value: Option<SharedType>,
) -> R<()> {
    world.pay(1)?;
    let index = local.0 as usize;
    let old = locals.get(index).ok_or_else(|| bad("write selects an absent actual local"))?;
    match (old, &value) {
        (None, None) => return Ok(()),
        (Some(old), Some(value)) if Rc::ptr_eq(old, value) => return Ok(()),
        _ => {}
    }
    if Rc::strong_count(locals) != 1 {
        world.pay(locals.len().saturating_add(1))?;
    }
    Rc::make_mut(locals)[index] = value;
    Ok(())
}

fn edit_declarations<'s>(
    world: &mut World<'_, '_>,
    declared: &'s mut Rc<BTreeMap<LocalId, SharedType>>,
) -> R<&'s mut BTreeMap<LocalId, SharedType>> {
    world.pay(1)?;
    if Rc::strong_count(declared) != 1 {
        world.pay(declared.len().saturating_add(1))?;
    }
    Ok(Rc::make_mut(declared))
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
    let view = super::materialize::ActualDirectory::for_world(
        module,
        program,
        ledger
            .original
            .context()
            .ok_or_else(|| bad("actual world lacks its immutable original Gamma"))?,
        limits,
        left,
    )?;
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
        reservations: BTreeMap::new(),
        value_roots: BTreeMap::new(),
        local_roots: BTreeMap::new(),
        argument_witnesses: Vec::new(),
        receiver_adaptations: Vec::new(),
        index,
        call_proofs: Default::default(),
        types: types::Table::default(),
        zero_scalars: Default::default(),
        cfg_shapes: BTreeMap::new(),
        view,
        snapshots: 0,
        snapshot_work: 0,
        comparisons: 0,
        compare_work: 0,
        retired_metadata: 0,
        canonical_work: 0,
        key_work: 0,
        where_work: 0,
        same_calls: 0,
        same_work: 0,
        direct_work: BTreeMap::new(),
        last_instance: None,
        last_definition: None,
        last_point_block: None,
        last_point_rows: None,
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
                    let expected = super::materialize::class_in_view(
                        program,
                        &world.view,
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
                    let expected = world.physical_type(
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
                    ) = (actual, expected.as_ref())
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
                    let expected = world.physical_type(instance.id, carrier, World::type_of(ty)?)?;
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
            let nominal_self = SemanticTypeRecipe::SelfType {
                scope: SemanticScopeRef::Ancestor(0),
            };
            let recipe = match &original.body {
                SemanticDefinitionBody::Nominal { .. } => Some(&nominal_self),
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
                let expected = world.physical_type(instance.id, recipe, World::type_of(actual)?)?;
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
                    let expected = world.physical_type(
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
                diagnostic.notes.push(world.work_note());
                diagnostic.notes.push(format!(
                    "actual instance directory position {:?} of {}; immutable CFG structures {}",
                    world
                        .index
                        .instances
                        .get(&instance.id)
                        .map(|offset| offset + 1),
                    program.instances.len(),
                    world.cfg_shapes.len()
                ));
                diagnostic.notes.push(format!("actual world work: entry allowance {}, failing instance {:?}/definition {:?} began {}, consumed {}, remaining {}; type cache hits {}, misses {}, lookup work {}, evaluation work {}; state snapshots {} work {}, comparisons {} work {}; unreachable correspondence entries retired {}; canonical work {}, digest work {}, immutable where work {}; live normalized equations {} work {}", entered, instance.id, instance.definition, before, before.saturating_sub(*world.left), *world.left, world.types.hits, world.types.misses, world.types.lookup_work, world.types.evaluation_work, world.snapshots, world.snapshot_work, world.comparisons, world.compare_work, world.retired_metadata, world.canonical_work, world.key_work, world.where_work, world.same_calls, world.same_work));
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
