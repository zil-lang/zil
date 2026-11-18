//! Resolve Core-only operands against the actual emitted function and value.
//! Resulting operands remain untrusted. This is not the original W, body class
//! flow, environment seal or rigid/partition proof at the publication boundary.
use super::emission::Ledger;
use crate::*;
use std::collections::BTreeSet;

fn bad(message: &str) -> Diagnostic {
    error("E-SEMANTIC-MATERIALIZE", Span::default(), message)
}
fn unknown(message: &str) -> Diagnostic {
    Diagnostic::unsupported(Phase::Types, message, None)
}
// A private tagged failure from the exact header reader. The retained identity
// alone is never enough to turn a later unrelated failure into readiness.
const PENDING_HEADER: &str = "E-SEMANTIC-HEADER-DEPENDENCY";
fn is_pending_header(diagnostic: &Diagnostic) -> bool {
    diagnostic.category == zil_base::ResultCategory::Unsupported
        && diagnostic.code == PENDING_HEADER
}
fn empty() -> StorageLifeShape {
    StorageLifeShape {
        binder_count: 0,
        slots: Vec::new(),
        retained: Vec::new(),
        nominal_arguments: Vec::new(),
    }
}
fn resolved(
    ty: Type,
    life: StorageLifeShape,
    classes: Vec<SemanticClassExpr>,
) -> SemanticTypeValue {
    SemanticTypeValue::Resolved { ty, life, classes }
}
/// A logical whole-type hint has an explicit source. It cannot be created
/// from an arbitrary physical Type (notably the storage of a callback slot).
#[derive(Clone, Copy)]
pub(super) struct LogicalExpected<'t> { ty: &'t Type }
impl<'t> LogicalExpected<'t> {
    pub(super) fn local(value: super::emission::LogicalLocalType<'t>) -> Self {
        Self { ty: value.as_type() }
    }
    pub(super) fn whole(value: &'t SemanticTypeValue) -> R<Self> {
        match value {
            SemanticTypeValue::Resolved { ty, life, classes }
                if life.binder_count as usize == classes.len() => Ok(Self { ty }),
            _ => Err(bad("logical expected type lacks its complete whole-type value")),
        }
    }
    // Only the checked logical entry may unwrap into the pure type evaluator.
    pub(super) fn materialize<T>(
        self, run: impl FnOnce(&'t Type) -> R<T>,
    ) -> R<T> { run(self.ty) }
}

fn logical_operand<'t>(
    ledger: &Ledger, directory: &'t ActualDirectory<'_>, function: crate::FunctionRef<'t>,
    operand: &SemanticOperand, limits: &Limits, left: &mut usize,
) -> R<LogicalExpected<'t>> {
    let ty = match operand {
        SemanticOperand::Value(value) => function.values.get(value.0 as usize),
        SemanticOperand::Local(local) => return Ok(LogicalExpected::local(
            ledger.logical_local_type(function, *local, limits, left)?)),
        SemanticOperand::Parameter(ordinal) => {
            let local = *function.parameters.get(*ordinal as usize)
                .ok_or_else(|| bad("logical parameter has no exact actual input local"))?;
            return Ok(LogicalExpected::local(ledger.logical_local_type(function, local, limits, left)?));
        }
        SemanticOperand::Capture(index) => function.environment
            .and_then(|id| directory.closures.iter().find(|row| row.id == id))
            .and_then(|row| row.captures.get(*index as usize)),
        _ => None,
    }.ok_or_else(|| bad("logical expected operand has no exact actual value/capture type"))?;
    Ok(LogicalExpected { ty })
}

/// The existing persistent callback slot has one physical element. Its
/// representation does not change the child's complete intrinsic domain.
pub(super) fn closure_slot_array(
    element: &SemanticTypeValue, limits: &Limits, left: &mut usize,
) -> R<SemanticTypeValue> {
    crate::dyn_metadata::semantic_operand_snapshot(element, limits, left, 1)?;
    let SemanticTypeValue::Resolved { ty, life, classes } = element else {
        return Err(bad("callback slot element retains an unresolved original type"));
    };
    if !matches!(ty, Type::Closure(_)) || life.binder_count as usize != classes.len()
        || life.slots.len() > limits.max_nodes
    {
        return Err(bad("callback slot omits its exact closure or complete element domain"));
    }
    let mut shape = empty();
    shape.binder_count = life.binder_count;
    let work = life.slots.iter().fold(classes.len().saturating_add(1), |work, slot| {
        work.saturating_add(slot.path.len().saturating_add(2))
    });
    *left = left.checked_sub(work).ok_or_else(|| Diagnostic::limit(
        Phase::Types, "callback slot complete element transport exceeded", None,
    ))?;
    for slot in &life.slots {
        let mut path = vec![PlaceProjection::Field(0)];
        path.extend_from_slice(&slot.path);
        shape.slots.push(StorageLifeSlot { path, ..slot.clone() });
    }
    let map: Vec<_> = (0..life.binder_count).collect();
    inherit_child_shape(&mut shape, life, LifeTypeProjection::Element, &map, limits, left)?;
    Ok(resolved(Type::Array { element: Box::new(ty.clone()), length: 1 }, shape, classes.clone()))
}

/// Embed a real child's complete occurrence forest through its proved class
/// map. Physical slots belong to the carrier's own layout and are transported
/// separately; these rows describe type occurrences, never access permissions.
pub(super) fn inherit_child_shape(
    parent: &mut StorageLifeShape,
    child: &StorageLifeShape,
    prefix: LifeTypeProjection,
    map: &[u32],
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let retained_count = parent
        .retained
        .len()
        .saturating_add(map.len())
        .saturating_add(child.retained.len());
    let nominal_count = parent
        .nominal_arguments
        .len()
        .saturating_add(child.nominal_arguments.len());
    if retained_count > limits.max_nodes || nominal_count > limits.max_nodes {
        return Err(Diagnostic::limit(
            Phase::Types,
            "complete child occurrence row limit exceeded",
            None,
        ));
    }
    let mut pay = |work: usize| -> R<()> {
        *left = left.checked_sub(work).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "complete child occurrence transport exceeded",
                None,
            )
        })?;
        Ok(())
    };
    pay(map
        .len()
        .saturating_add(child.retained.len())
        .saturating_add(child.nominal_arguments.len()))?;
    if child.binder_count as usize != map.len()
        || map.iter().any(|binder| *binder >= parent.binder_count)
    {
        return Err(bad(
            "child occurrence map leaves its complete parent domain",
        ));
    }
    let mapped = |class: u32| {
        map.get(class as usize)
            .copied()
            .ok_or_else(|| bad("child occurrence leaves its complete intrinsic domain"))
    };
    for (class, binder) in map.iter().enumerate() {
        parent.retained.push(StorageRetainedBinding {
            path: vec![prefix],
            class: class as u32,
            binder: *binder,
        });
    }
    for row in &child.retained {
        pay(row.path.len().saturating_add(1))?;
        let mut path = vec![prefix];
        path.extend_from_slice(&row.path);
        parent.retained.push(StorageRetainedBinding {
            path,
            class: row.class,
            binder: mapped(row.binder)?,
        });
    }
    for row in &child.nominal_arguments {
        pay(row.path.len().saturating_add(1))?;
        let mut path = vec![prefix];
        path.extend_from_slice(&row.path);
        parent.nominal_arguments.push(NominalLifeArgument {
            path,
            declaration: row.declaration,
            argument: row.argument,
            binder: mapped(row.binder)?,
        });
    }
    Ok(())
}
#[path = "materialize_deferred.rs"]
mod deferred;
#[path = "materialize_dyn.rs"]
pub(super) mod dynamic;
#[path = "materialize_nominal.rs"]
mod nominal;
pub(super) use deferred::populate as populate_deferred_nominals;
#[path = "materialize_opaque.rs"]
mod opaque;
pub(super) use opaque::call_result_at;
#[path = "materialize_domains.rs"]
mod domains;
#[path = "materialize_emitter.rs"]
mod emitter;
#[path = "creator_invocations.rs"]
mod invocations;
#[path = "materialize_scopes.rs"]
mod scopes;
#[path = "materialize_self.rs"]
mod self_type;
#[path = "materialize_shapes.rs"]
mod shapes;
#[path = "materialize_tasks.rs"]
mod task_domains;
#[path = "materialize_task_results.rs"]
mod task_results;
pub(crate) use emitter::{PreparedPoll, PreparedTask, prepare_poll, prepare_task};
#[path = "materialize_view.rs"]
mod view;
pub(super) use view::ActualDirectory;
struct Eval<'a, 'm, 'b> {
    input: &'a SemanticModule,
    module: &'a ActualDirectory<'m>,
    ledger: &'a Ledger,
    limits: &'b Limits,
    left: &'b mut usize,
    active: BTreeSet<(SemanticInstanceId, u32)>,
    invocation_probe: Option<(SemanticInstanceId, u32)>,
    pending_header: Option<ClosureId>,
}
impl<'a, 'm> Eval<'a, 'm, '_> {
    #[track_caller]
    fn fee(&mut self, count: usize) -> R<()> {
        let location = std::panic::Location::caller();
        *self.left = self.left.checked_sub(count).ok_or_else(|| {
            let mut diagnostic = Diagnostic::limit(
                Phase::Types,
                "semantic type materialization work exceeded",
                None,
            );
            diagnostic.notes.push(format!("ordinary-domain request totals {:?}", self.ledger.ordinary_stats.borrow()));
            diagnostic.notes.push(format!("actual type evaluation at {}:{}; requested work {}; remaining {}; active original holder {:?}; holder count {}", location.file(), location.line(), count, *self.left, self.active.iter().next(), self.active.len()));
            diagnostic
        })?;
        Ok(())
    }
    fn rows(&mut self, count: usize) -> R<()> {
        if count > self.limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "semantic materialization row limit exceeded",
                None,
            ));
        }
        self.fee(count)
    }
    #[track_caller]
    fn node(&mut self, depth: usize) -> R<()> {
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "semantic type materialization depth exceeded",
                None,
            ));
        }
        self.fee(1)
    }
    fn instance(&mut self, id: SemanticInstanceId) -> R<&'a SemanticInstance> {
        self.fee(self.module.instances.len().saturating_add(1).ilog2() as usize + 1)?;
        if let Some(row) = self.module.instances.get(&id) {
            return Ok(row);
        }
        if let Some(row) = self.module.indexed_instance(self.input, id, self.left)? {
            return Ok(row);
        }
        self.fee(self.input.instances.len())?;
        if let Some(row) = self.input.instances.iter().find(|row| row.id == id) {
            return Ok(row);
        }
        if let Some(source) = self.ledger.frozen.as_deref() {
            self.fee(source.semantic.instances.len())?;
            if let Some(row) = source.semantic.instances.iter().find(|row| row.id == id) {
                return Ok(row);
            }
        }
        Err(bad("materialized operand has no real semantic instance"))
    }
    fn definition(&mut self, id: SemanticDefinitionId) -> R<&'a SemanticDefinition> {
        // Definitions are the immutable original declarations, not emitted
        // instance claims. Reuse this request's exact checked directory; the
        // publication boundary separately verifies all copied definitions.
        if let Some(context) = self.ledger.original.context() {
            return context.definition(id, self.left);
        }
        self.fee(self.input.definitions.len())?;
        if let Some(row) = self.input.definitions.iter().find(|row| row.id == id) {
            return Ok(row);
        }
        if let Some(source) = self.ledger.frozen.as_deref() {
            self.fee(source.semantic.definitions.len())?;
            if let Some(row) = source.semantic.definitions.iter().find(|row| row.id == id) {
                return Ok(row);
            }
        }
        Err(bad("materialized operand lost its original declaration"))
    }
    fn ancestor(
        &mut self,
        mut id: SemanticInstanceId,
        hops: SemanticScopeRef,
        depth: usize,
    ) -> R<SemanticInstanceId> {
        let SemanticScopeRef::Ancestor(hops) = hops;
        for step in 0..hops as usize {
            self.node(depth.saturating_add(step))?;
            id = self
                .instance(id)?
                .parent
                .ok_or_else(|| bad("materialized type crosses its lexical root"))?;
        }
        Ok(id)
    }
    fn function(&mut self, scope: SemanticInstanceId) -> R<crate::FunctionRef<'m>> {
        let instance = self.instance(scope)?;
        let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(id)) = instance.target
        else {
            return Err(bad(
                "runtime type operand is outside its actual emitted function",
            ));
        };
        self.fee(self.module.function_lookup_work())?;
        self.module
            .function(id)
            .ok_or_else(|| bad("materialized type has no actual Typed function"))
    }
    fn copy(&mut self, value: &SemanticTypeValue) -> R<SemanticTypeValue> {
        crate::dyn_metadata::semantic_operand_snapshot(value, self.limits, self.left, 1)?;
        Ok(value.clone())
    }
    fn physical(&mut self, ty: &Type) -> R<Type> {
        inference::charge_type_snapshot(ty, Span::default(), self.limits, self.left, 1)?;
        Ok(ty.clone())
    }
    fn operand(
        &mut self,
        scope: SemanticInstanceId,
        operand: &SemanticOperand,
    ) -> R<SemanticOperand> {
        if let SemanticOperand::CoreExpression {
            expression,
            occurrence: 0,
        } = operand
        {
            let f = self.function(scope)?;
            self.fee(self.ledger.frames.len().saturating_add(1).ilog2() as usize + 1)?;
            let frame = self
                .ledger
                .frames
                .get(&f.id)
                .ok_or_else(|| bad("type operand has no actual emitter frame"))?;
            self.fee(frame.expressions.len().saturating_add(1).ilog2() as usize + 1)?;
            let row = frame
                .expressions
                .get(expression)
                .ok_or_else(|| bad("type operand expression was not emitted"))?;
            row.value
                .map(SemanticOperand::Value)
                .or_else(|| row.place.clone())
                .ok_or_else(|| bad("type operand expression has no actual value or place"))
        } else if matches!(operand, SemanticOperand::CoreExpression { .. }) {
            Err(bad("type operand occurrence is not a real Core mint"))
        } else {
            Ok(operand.clone())
        }
    }
    fn class(
        &mut self,
        scope: SemanticInstanceId,
        class: &SemanticClassExpr,
    ) -> R<SemanticClassExpr> {
        crate::dyn_metadata::semantic_operand_snapshot(class, self.limits, self.left, 1)?;
        let mut class = class.clone();
        if let SemanticClassExpr::OuterOccurrence { operand, .. }
        | SemanticClassExpr::IntrinsicClass { operand, .. } = &mut class
        {
            *operand = self.operand(scope, operand)?;
        }
        Ok(class)
    }
    fn rehome_classes(&mut self, value: &mut SemanticTypeValue, hops: u32) -> R<()> {
        if hops == 0 {
            return Ok(());
        }
        let SemanticTypeValue::Resolved { classes, .. } = value else {
            return Err(bad("class relocation retains a Core whole type"));
        };
        self.fee(classes.len())?;
        for class in classes {
            crate::dyn_metadata::semantic_operand_snapshot(class, self.limits, self.left, 1)?;
            let scope = match class {
                SemanticClassExpr::Formal { scope, .. }
                | SemanticClassExpr::Implicit { scope, .. }
                | SemanticClassExpr::Environment { scope, .. } => Some(scope),
                SemanticClassExpr::Bound { .. } => None,
                SemanticClassExpr::IntrinsicClass { operand, .. }
                | SemanticClassExpr::OuterOccurrence { operand, .. } => match operand {
                    SemanticOperand::TypeArgument { scope, .. } => Some(scope),
                    _ => {
                        return Err(unknown(
                            "ancestor whole type needs its actual sealed operand class relocation",
                        ));
                    }
                },
            };
            if let Some(SemanticScopeRef::Ancestor(depth)) = scope {
                *depth = depth
                    .checked_add(hops)
                    .ok_or_else(|| bad("whole-type class scope relocation overflows"))?;
            }
        }
        Ok(())
    }
    fn recipe_operand(&mut self, operand: &SemanticRecipeOperand) -> R<SemanticOperand> {
        self.fee(1)?;
        Ok(match operand {
            SemanticRecipeOperand::TypeFormal(formal) => SemanticOperand::TypeArgument {
                scope: formal.scope,
                ordinal: formal.ordinal,
            },
            SemanticRecipeOperand::Environment {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            } => SemanticOperand::Environment(*ordinal),
            SemanticRecipeOperand::Parameter(index) => SemanticOperand::Parameter(*index),
            SemanticRecipeOperand::Receiver => SemanticOperand::Receiver,
            SemanticRecipeOperand::Environment { .. } => {
                return Err(unknown(
                    "ancestor Type environment requires its checked sealed whole-type map",
                ));
            }
        })
    }
    fn canonical_formal(
        &mut self,
        scope: SemanticInstanceId,
        reference: &SemanticFormalRef,
    ) -> R<SemanticClassExpr> {
        let owner = self.ancestor(scope, reference.scope, 0)?;
        let instance = self.instance(owner)?;
        let definition = self.definition(instance.definition)?;
        self.fee(
            definition
                .parameters
                .len()
                .saturating_add(definition.requirements.len()),
        )?;
        let mut representatives: Vec<_> = (0..definition.parameters.len() as u32).collect();
        for requirement in &definition.requirements {
            let SemanticRequirementRecipe::LifeEqual {
                left:
                    SemanticLifeRecipe::Formal(SemanticFormalRef {
                        scope: SemanticScopeRef::Ancestor(0),
                        ordinal: left,
                    }),
                right:
                    SemanticLifeRecipe::Formal(SemanticFormalRef {
                        scope: SemanticScopeRef::Ancestor(0),
                        ordinal: right,
                    }),
            } = requirement
            else {
                continue;
            };
            let a = *representatives
                .get(*left as usize)
                .ok_or_else(|| bad("original equality selects an absent mixed formal"))?;
            let b = *representatives
                .get(*right as usize)
                .ok_or_else(|| bad("original equality selects an absent mixed formal"))?;
            self.fee(representatives.len())?;
            let root = a.min(b);
            for representative in &mut representatives {
                if *representative == a || *representative == b {
                    *representative = root;
                }
            }
        }
        let ordinal = *representatives
            .get(reference.ordinal as usize)
            .ok_or_else(|| bad(&format!(
                "original life formal is outside its definition: current instance {scope:?}, selected instance {owner:?}, definition {:?}, mixed formals {}, reference {reference:?}",
                definition.id, definition.parameters.len()
            )))?;
        Ok(SemanticClassExpr::Formal {
            scope: reference.scope,
            ordinal,
        })
    }
    fn life(
        &mut self,
        scope: SemanticInstanceId,
        life: &SemanticLifeRecipe,
    ) -> R<SemanticClassExpr> {
        self.fee(1)?;
        Ok(match life {
            SemanticLifeRecipe::Formal(reference) => self.canonical_formal(scope, reference)?,
            SemanticLifeRecipe::Implicit { scope, ordinal } => SemanticClassExpr::Implicit {
                scope: *scope,
                ordinal: *ordinal,
            },
            SemanticLifeRecipe::Environment { scope, ordinal } => SemanticClassExpr::Environment {
                scope: *scope,
                ordinal: *ordinal,
            },
            SemanticLifeRecipe::Bound { depth, index } => SemanticClassExpr::Bound {
                depth: *depth,
                index: *index,
            },
            SemanticLifeRecipe::Invocation(index) => {
                if let Some((owner, count)) = self.invocation_probe
                    && owner == scope
                {
                    if *index >= count {
                        return Err(bad("invocation discovery exceeds its original frame"));
                    }
                    // This private draft marker is consumed only while finding
                    // real input coordinates. It never enters a Typed proposal.
                    SemanticClassExpr::Implicit {
                        scope: SemanticScopeRef::Ancestor(u32::MAX),
                        ordinal: *index,
                    }
                } else {
                    self.invocation_input(scope, *index)?
                }
            }
            SemanticLifeRecipe::OuterOccurrence {
                operand,
                path,
                mode,
            } => {
                self.fee(path.len())?;
                SemanticClassExpr::OuterOccurrence {
                    operand: self.recipe_operand(operand)?,
                    path: path.clone(),
                    mode: *mode,
                }
            }
            SemanticLifeRecipe::IntrinsicClass {
                operand,
                path,
                class,
            } => {
                self.fee(path.len())?;
                SemanticClassExpr::IntrinsicClass {
                    operand: self.recipe_operand(operand)?,
                    path: path.clone(),
                    class: *class,
                }
            }
        })
    }
    fn original_type(
        &mut self,
        scope: SemanticInstanceId,
        selector: SemanticScopeRef,
        source: &SemanticOriginalTypeSource,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        match self.original_type_view(scope, selector, source, depth)? {
            std::borrow::Cow::Borrowed(recipe) => {
                crate::dyn_metadata::semantic_operand_snapshot(recipe, self.limits, self.left, 1)?;
                Ok(recipe.clone())
            }
            std::borrow::Cow::Owned(recipe) => Ok(recipe),
        }
    }
    fn original_type_view(
        &mut self,
        scope: SemanticInstanceId,
        selector: SemanticScopeRef,
        source: &SemanticOriginalTypeSource,
        depth: usize,
    ) -> R<std::borrow::Cow<'a, SemanticTypeRecipe>> {
        let owner = self.ancestor(scope, selector, depth + 1)?;
        let current = self.instance(scope)?.definition;
        let owner = self.instance(owner)?.definition;
        let proof = self.ledger.original.borrow();
        if let Some(proof) = proof
            .as_ref()
            .filter(|proof| proof.complete.contains(&owner))
        {
            return proof.type_recipe_view(
                self.input,
                current,
                owner,
                source,
                self.limits,
                self.left,
            );
        }
        let frozen = self.ledger.frozen.as_deref().ok_or_else(|| {
            bad("original symbolic proof was not completed before materialization")
        })?;
        let recipe = frozen.original.type_recipe_view(
            &frozen.semantic,
            owner,
            owner,
            source,
            self.limits,
            self.left,
        )?;
        let SemanticScopeRef::Ancestor(shift) = selector;
        if shift == 0 {
            return Ok(recipe);
        }
        let empty = std::collections::BTreeMap::new();
        Ok(std::borrow::Cow::Owned(super::original::substitute::ty(
            &recipe,
            &super::original::substitute::Substitution {
                frames: &[],
                selves: &[],
                environments: &[],
                implicit: &empty,
                invocation: &empty,
                shift,
            },
            self.limits,
            self.left,
        )?))
    }
    fn original_constructor_type(
        &mut self,
        scope: SemanticInstanceId,
        selector: SemanticScopeRef,
        source: &SemanticOriginalTypeSource,
    ) -> R<std::borrow::Cow<'a, SemanticTypeRecipe>> {
        let current = self.instance(scope)?.definition;
        let owner = self.ancestor(scope, selector, 0)?;
        let owner = self.instance(owner)?.definition;
        let proof = self.ledger.original.borrow();
        if let Some(proof) = proof
            .as_ref()
            .filter(|proof| proof.complete.contains(&owner))
        {
            return proof.constructor_type_recipe(
                self.input,
                current,
                owner,
                source,
                self.limits,
                self.left,
            );
        }
        let frozen = self
            .ledger
            .frozen
            .as_deref()
            .ok_or_else(|| bad("constructor lacks its completed exact original proof"))?;
        frozen.original.constructor_type_recipe(
            &frozen.semantic,
            owner,
            owner,
            source,
            self.limits,
            self.left,
        )
    }
    fn inferred_recipe(
        &mut self,
        scope: SemanticInstanceId,
        call: SemanticSiteId,
        formal: u32,
        holder: Option<SemanticInstanceId>,
    ) -> R<std::borrow::Cow<'a, SemanticActualRecipe>> {
        let owner = self.instance(scope)?.definition;
        let holder = holder
            .map(|id| self.instance(id).map(|row| row.definition))
            .transpose()?;
        self.fee(self.input.definitions.len().saturating_add(2))?;
        if let Some(source) = self.ledger.frozen.as_deref() {
            self.fee(source.semantic.definitions.len().saturating_mul(2))?;
        }
        let proof = self.ledger.original.borrow();
        let proof = if let Some(proof) = proof
            .as_ref()
            .filter(|proof| proof.complete.contains(&owner))
        {
            proof
        } else {
            &self
                .ledger
                .frozen
                .as_deref()
                .ok_or_else(|| bad("original call inference was not completed"))?
                .original
        };
        let Some(holder) = holder else {
            return proof.call_actual_view(owner, call, formal, self.limits, self.left);
        };
        let target = proof
            .inferred_call_target(owner, call, self.limits, self.left)?
            .ok_or_else(|| bad("inferred holder has no original resolved call target"))?;
        // A selected implementation method has a distinct declaration from
        // its trait member. Their own mixed frames correspond only through
        // the independently checked complete provider theorem; neither is the
        // other's lexical parent Scope.
        let provider = proof.providers.get(&holder) == Some(&target);
        if target == holder || provider {
            if provider && !proof.complete.contains(&holder) {
                return Err(bad(
                    "inferred provider holder lacks its complete original method proof",
                ));
            }
            return proof.call_actual_view(owner, call, formal, self.limits, self.left);
        }
        let target = self
            .input
            .definitions
            .iter()
            .find(|row| row.id == target)
            .or_else(|| {
                self.ledger.frozen.as_deref().and_then(|source| {
                    source
                        .semantic
                        .definitions
                        .iter()
                        .find(|row| row.id == target)
                })
            })
            .ok_or_else(|| bad("inferred method lost its original target"))?;
        let parent = self
            .input
            .definitions
            .iter()
            .find(|row| row.id == holder)
            .or_else(|| {
                self.ledger.frozen.as_deref().and_then(|source| {
                    source
                        .semantic
                        .definitions
                        .iter()
                        .find(|row| row.id == holder)
                })
            })
            .ok_or_else(|| bad("inferred parent scope lost its original definition"))?;
        if target.parent != Some(holder)
            || !matches!(parent.body, SemanticDefinitionBody::Scope(_))
            || !proof.complete.contains(&owner)
        {
            return Err(bad(
                "inferred scope holder is not the original method lexical parent",
            ));
        }
        let recipe = proof
            .scope_calls
            .get(&(owner, call, holder))
            .and_then(|actuals| actuals.get(formal as usize))
            .ok_or_else(|| unknown("original receiver has no checked parent scope solution"))?;
        Ok(std::borrow::Cow::Borrowed(recipe))
    }
    fn inferred_actual(
        &mut self,
        scope: SemanticInstanceId,
        call: SemanticSiteId,
        formal: u32,
        holder: Option<SemanticInstanceId>,
        depth: usize,
    ) -> R<SemanticActual> {
        if let Some(actual) = self.inferred_parent_actual(scope, call, formal, holder, depth + 1)? {
            return Ok(actual);
        }
        if let Some(actual) =
            self.multiple_inferred_actual(scope, call, formal, holder, depth + 1)?
        {
            return Ok(actual);
        }
        let recipe = self.inferred_recipe(scope, call, formal, holder)?;
        self.call_recipe_actual(scope, call, formal, holder, &recipe, depth + 1)
    }

    fn call_recipe_actual(
        &mut self,
        scope: SemanticInstanceId,
        call: SemanticSiteId,
        formal: u32,
        holder: Option<SemanticInstanceId>,
        recipe: &SemanticActualRecipe,
        depth: usize,
    ) -> R<SemanticActual> {
        if matches!(recipe, SemanticActualRecipe::Type(SemanticTypeRecipe::Opaque { .. }))
            && let Some(value) = self.inferred_capture_pointee(scope, call, formal, holder, depth + 1)?
        {
            return Ok(SemanticActual::Type(value));
        }
        self.recipe_actual(scope, recipe, depth)
    }

    fn inferred_capture_pointee(
        &mut self,
        scope: SemanticInstanceId,
        call: SemanticSiteId,
        formal: u32,
        holder: Option<SemanticInstanceId>,
        depth: usize,
    ) -> R<Option<SemanticTypeValue>> {
        self.node(depth)?;
        let instance = self.instance(scope)?;
        let owner = instance.definition;
        let holder = holder
            .map(|id| self.instance(id).map(|row| row.definition))
            .transpose()?;
        let environments = {
            let proof = self.ledger.original.borrow();
            let proof = proof
                .as_ref()
                .filter(|proof| proof.complete.contains(&owner))
                .or_else(|| self.ledger.frozen.as_deref().map(|source| &source.original))
                .ok_or_else(|| bad("inferred captured Type has no complete original proof"))?;
            let Some(environments) =
                proof.inferred_pointee_environments(owner, call, formal, holder, self.limits, self.left)?
            else {
                return Ok(None);
            };
            self.fee(environments.len())?;
            environments.iter().copied().collect::<Vec<_>>()
        };
        let definition = self.definition(owner)?;
        let creator = instance
            .parent
            .ok_or_else(|| bad("inferred capture Type has no actual creator"))?;
        if definition.parent != Some(self.instance(creator)?.definition) {
            return Err(bad("inferred capture Type changes its real lexical creator"));
        }
        let mut result = None;
        for ordinal in environments {
            let Some(SemanticActualRecipe::Type(source)) = definition
                .environment
                .get(ordinal as usize)
                .map(|row| &row.source)
            else {
                return Err(bad("inferred capture Type has no original sealed slot"));
            };
            let SemanticTypeValue::Resolved {
                ty: Type::Address { inner, .. },
                life: capture_life,
                classes: capture_images,
            } = self.ty(
                scope,
                &SemanticTypeRecipe::Environment {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal,
                },
                None,
                depth + 1,
            )?
            else {
                return Err(bad(
                    "inferred capture pointee does not select an actual address",
                ));
            };
            let child = self
                .endpoint_recipe(creator, source, LifeTypeProjection::Pointee, depth + 1)?
                .ok_or_else(|| {
                    unknown("inferred capture pointee lacks its complete original type equation")
                })?;
            let SemanticTypeValue::Resolved { ty, life, .. } =
                self.ty(creator, &child, Some(&inner), depth + 1)?
            else {
                unreachable!()
            };
            inference::charge_type_snapshot(&ty, Span::default(), self.limits, self.left, 1)?;
            inference::charge_type_snapshot(&inner, Span::default(), self.limits, self.left, 1)?;
            let map = self.endpoint_direct(&capture_life, &[LifeTypeProjection::Pointee])?;
            let map = map.as_deref().unwrap_or_default();
            if ty != *inner || map.len() != life.binder_count as usize {
                return Err(bad(
                    "sealed address Type changes its original complete pointee domain",
                ));
            }
            let mut recomposed = empty();
            recomposed.binder_count = capture_life.binder_count;
            inherit_child_shape(
                &mut recomposed,
                &life,
                LifeTypeProjection::Pointee,
                map,
                self.limits,
                self.left,
            )?;
            for row in &capture_life.retained {
                self.fee(row.path.len().saturating_add(1))?;
            }
            for row in &capture_life.nominal_arguments {
                self.fee(row.path.len().saturating_add(1))?;
            }
            if recomposed.retained != capture_life.retained
                || recomposed.nominal_arguments != capture_life.nominal_arguments
            {
                return Err(bad("sealed address loses original pointee occurrence rows"));
            }
            let mut images = Vec::new();
            self.fee(map.len())?;
            for binder in map {
                let image = capture_images
                    .get(*binder as usize)
                    .ok_or_else(|| bad("sealed capture pointee selects an absent class"))?;
                crate::dyn_metadata::semantic_operand_snapshot(image, self.limits, self.left, 1)?;
                images.push(image.clone());
            }
            let value = resolved(ty, life, images);
            if let Some(previous) = &result {
                crate::dyn_metadata::semantic_operand_snapshot(previous, self.limits, self.left, 1)?;
                crate::dyn_metadata::semantic_operand_snapshot(&value, self.limits, self.left, 1)?;
                if previous != &value {
                    return Err(unknown(
                        "inferred opaque pointee has distinct sealed capture Type domains",
                    ));
                }
            } else {
                result = Some(value);
            }
        }
        Ok(result)
    }

    fn type_actual(
        &mut self,
        scope: SemanticInstanceId,
        ordinal: u32,
        depth: usize,
    ) -> R<SemanticTypeValue> {
        self.node(depth)?;
        // Prepay both insertion and removal: a NotReady return is resumed by
        // the same evaluator and must not retain an abandoned active query.
        self.fee((self.active.len().saturating_add(1).ilog2() as usize + 1).saturating_mul(2))?;
        if !self.active.insert((scope, ordinal)) {
            return Err(bad(
                "whole-type actual depends cyclically on its own asserted class table",
            ));
        }
        let value = (|| -> R<SemanticTypeValue> {
            let instance = self.instance(scope)?;
            let value = instance
                .actuals
                .get(ordinal as usize)
                .ok_or_else(|| bad("whole-type actual omits its original full ordinal"))?;
            let value = match value {
                SemanticActual::Type(value) => self.value(scope, value, None, depth + 1)?,
                SemanticActual::CoreRecipe { scope, recipe } => match recipe.as_ref() {
                    SemanticActualRecipe::Type(recipe) => {
                        self.ty(*scope, recipe, None, depth + 1)?
                    }
                    _ => return Err(bad("Type formal selects a non-Type actual")),
                },
                SemanticActual::CoreInferred {
                    scope,
                    call,
                    formal,
                } => {
                    if *formal != ordinal {
                        return Err(bad("inferred Type actual changes its full holder ordinal"));
                    }
                    let SemanticActual::Type(value) =
                        self.inferred_actual(*scope, *call, *formal, Some(instance.id), depth + 1)?
                    else {
                        return Err(bad("original inference selected a non-Type argument"));
                    };
                    value
                }
                _ => return Err(bad("Type formal selects a non-Type actual")),
            };
            Ok(value)
        })();
        self.active.remove(&(scope, ordinal));
        let value = value?;
        let SemanticTypeValue::Resolved { ty, life, .. } = value else {
            unreachable!()
        };
        self.fee(life.binder_count as usize)?;
        let classes = (0..life.binder_count)
            .map(|class| SemanticClassExpr::IntrinsicClass {
                operand: SemanticOperand::TypeArgument {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal,
                },
                path: Vec::new(),
                class,
            })
            .collect();
        Ok(resolved(ty, life, classes))
    }
    fn logical_type(
        &mut self, scope: SemanticInstanceId, recipe: &SemanticTypeRecipe,
        hint: Option<LogicalExpected<'_>>, depth: usize,
    ) -> R<SemanticTypeValue> {
        match hint {
            Some(hint) => hint.materialize(|ty| self.ty(scope, recipe, Some(ty), depth)),
            None => self.ty(scope, recipe, None, depth),
        }
    }
    fn value(
        &mut self,
        owner: SemanticInstanceId,
        value: &SemanticTypeValue,
        hint: Option<LogicalExpected<'_>>,
        depth: usize,
    ) -> R<SemanticTypeValue> {
        self.node(depth)?;
        match value {
            SemanticTypeValue::Resolved { ty, life, classes } => {
                if life.binder_count as usize != classes.len() {
                    return Err(bad("whole-type class directory omits intrinsic classes"));
                }
                if let Some(hint) = hint {
                    let hint = hint.ty;
                    inference::charge_type_snapshot(
                        ty,
                        Span::default(),
                        self.limits,
                        self.left,
                        1,
                    )?;
                    inference::charge_type_snapshot(
                        hint,
                        Span::default(),
                        self.limits,
                        self.left,
                        1,
                    )?;
                    if ty != hint {
                        return Err(bad(
                            "resolved type does not match its actual emitted operand",
                        ));
                    }
                }
                let mut value = self.copy(value)?;
                if let SemanticTypeValue::Resolved { ty, life, classes } = &mut value {
                    if let Type::BorrowedStorage { life: actual, .. } = ty {
                        if actual != life {
                            return Err(bad(
                                "resolved type domain contradicts its actual qualified type",
                            ));
                        }
                    }
                    for class in classes {
                        if let SemanticClassExpr::OuterOccurrence { operand, .. }
                        | SemanticClassExpr::IntrinsicClass { operand, .. } = class
                        {
                            *operand = self.operand(owner, operand)?;
                        }
                    }
                }
                Ok(value)
            }
            SemanticTypeValue::Core { scope, source } => match source {
                SemanticCoreTypeSource::OriginalRecipe(recipe) => {
                    self.logical_type(*scope, recipe, hint, depth + 1)
                }
                SemanticCoreTypeSource::Expression {
                    module,
                    function,
                    expression,
                    occurrence: 0,
                    recipe,
                } => self.expression_value(
                    *scope,
                    *module,
                    *function,
                    *expression,
                    recipe,
                    hint,
                    depth,
                ),
                SemanticCoreTypeSource::OriginalExpression {
                    module,
                    function,
                    expression,
                    occurrence: 0,
                    source,
                } => {
                    let recipe = self.original_type_view(
                        *scope,
                        SemanticScopeRef::Ancestor(0),
                        source,
                        depth + 1,
                    )?;
                    self.expression_value(
                        *scope,
                        *module,
                        *function,
                        *expression,
                        &recipe,
                        hint,
                        depth,
                    )
                }
                SemanticCoreTypeSource::Expression { .. }
                | SemanticCoreTypeSource::OriginalExpression { .. } => {
                    Err(bad("inferred whole-type occurrence is not a real mint"))
                }
            },
        }
    }
    fn expression_value(
        &mut self,
        scope: SemanticInstanceId,
        module: zil_base::ModuleId,
        function: zil_base::NodeId,
        expression: zil_base::NodeId,
        recipe: &SemanticTypeRecipe,
        hint: Option<LogicalExpected<'_>>,
        depth: usize,
    ) -> R<SemanticTypeValue> {
        let f = self.function(scope)?;
        self.fee(self.ledger.frames.len().saturating_add(1).ilog2() as usize + 1)?;
        let frame = self
            .ledger
            .frames
            .get(&f.id)
            .ok_or_else(|| bad("inferred type has no actual emitter owner"))?;
        if frame.origin.module != module || frame.origin.creator != function {
            return Err(bad("inferred type selects another actual Core owner"));
        }
        self.fee(frame.expressions.len().saturating_add(1).ilog2() as usize + 1)?;
        let row = frame
            .expressions
            .get(&expression)
            .ok_or_else(|| bad("inferred whole-type expression was not emitted"))?;
        let operand = row.value.map(SemanticOperand::Value)
            .or_else(|| row.place.clone())
            .ok_or_else(|| bad("inferred type expression has no actual complete value/place type"))?;
        let actual = logical_operand(self.ledger, self.module, f, &operand, self.limits, self.left)?;
        if let Some(hint) = hint {
            inference::charge_type_snapshot(actual.ty, Span::default(), self.limits, self.left, 1)?;
            inference::charge_type_snapshot(hint.ty, Span::default(), self.limits, self.left, 1)?;
            if actual.ty != hint.ty {
                return Err(bad("inferred type and actual site type disagree"));
            }
        }
        self.logical_type(scope, recipe, Some(actual), depth + 1)
    }
    fn constructor_value(
        &mut self,
        owner: SemanticInstanceId,
        proposed: &SemanticTypeValue,
        result: ValueId,
        site: SemanticSiteId,
    ) -> R<SemanticTypeValue> {
        let function = self.function(owner)?;
        let expected = logical_operand(self.ledger, self.module, function,
            &SemanticOperand::Value(result), self.limits, self.left)?;
        let definition = self.instance(owner)?.definition;
        let original = self.definition(definition)?;
        let original_node =
            if let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &original.body {
                self.fee(site_nodes.len())?;
                let mut nodes = site_nodes.iter().filter(|row| row.site == site);
                let node = nodes
                    .next()
                    .ok_or_else(|| bad("constructor site has no exact original node"))?;
                if nodes.next().is_some() {
                    return Err(bad("constructor site has multiple original nodes"));
                }
                let SemanticOriginalNode::Expression(node) = node.node else {
                    return Err(bad(
                        "constructor site does not select an original expression",
                    ));
                };
                Some(node)
            } else {
                None
            };
        let SemanticTypeValue::Core { scope, source } = proposed else {
            return self.value(owner, proposed, Some(expected), 0);
        };
        let (module, creator, expression, occurrence) = match source {
            SemanticCoreTypeSource::Expression {
                module,
                function,
                expression,
                occurrence,
                ..
            }
            | SemanticCoreTypeSource::OriginalExpression {
                module,
                function,
                expression,
                occurrence,
                ..
            } => (*module, *function, *expression, *occurrence),
            SemanticCoreTypeSource::OriginalRecipe(recipe) => {
                // Source records original-site recipes directly; these are
                // the ordinary constructor path, rather than a reified Core
                // expression wrapper. The exact original injection theorem
                // still selects the producer's payload role.
                if let SemanticTypeRecipe::OriginalType {
                    scope: selector,
                    source,
                } = recipe.as_ref()
                {
                    if original_node.is_some_and(|node| {
                        *scope != owner
                            || *selector != SemanticScopeRef::Ancestor(0)
                            || *source != SemanticOriginalTypeSource::Expression(node)
                    }) {
                        return Err(bad(
                            "constructor recipe changes its exact original site expression",
                        ));
                    }
                    let recipe = self.original_constructor_type(*scope, *selector, source)?;
                    return self.logical_type(*scope, &recipe, Some(expected), 0);
                }
                return self.value(owner, proposed, Some(expected), 0);
            }
        };
        if occurrence != 0 {
            return Err(bad("constructor type selects a non-mint occurrence"));
        }
        let function = self.function(*scope)?;
        self.fee(self.ledger.frames.len().saturating_add(1))?;
        let frame = self
            .ledger
            .frames
            .get(&function.id)
            .ok_or_else(|| bad("constructor type has no real emitter frame"))?;
        if frame.origin.module != module || frame.origin.creator != creator {
            return Err(bad("constructor type changes its actual source owner"));
        }
        self.fee(frame.expressions.len().saturating_add(1))?;
        let receipt = frame
            .expressions
            .get(&expression)
            .and_then(|r| r.construction)
            .ok_or_else(|| bad("constructor type has no actual producer receipt"))?;
        if receipt.value != result {
            return Err(bad(
                "constructor type selects another actual creation result",
            ));
        }
        match source {
            SemanticCoreTypeSource::Expression { recipe, .. } => {
                self.logical_type(*scope, recipe, Some(expected), 0)
            }
            SemanticCoreTypeSource::OriginalExpression { source, .. } => {
                if original_node.is_some_and(|node| {
                    *scope != owner || *source != SemanticOriginalTypeSource::Expression(node)
                }) {
                    return Err(bad(
                        "constructor wrapper changes its exact original site expression",
                    ));
                }
                let recipe =
                    self.original_constructor_type(*scope, SemanticScopeRef::Ancestor(0), source)?;
                self.logical_type(*scope, &recipe, Some(expected), 0)
            }
            SemanticCoreTypeSource::OriginalRecipe(_) => unreachable!(),
        }
    }
    fn constant(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticConstRecipe,
        carrier: &Type,
        depth: usize,
    ) -> R<Constant> {
        self.node(depth)?;
        let result = match recipe {
            SemanticConstRecipe::Value(value) => {
                self.fee(match value {
                    Constant::WideString(units) => units.len().saturating_add(1),
                    _ => 1,
                })?;
                value.clone()
            }
            SemanticConstRecipe::IntegerLiteral(text) => {
                self.fee(text.len().saturating_mul(3).saturating_add(1))?;
                let (text, negative) = text
                    .strip_prefix('-')
                    .map_or((text.as_str(), false), |value| (value, true));
                integer(text, Some(carrier), negative, Span::default())?
            }
            SemanticConstRecipe::FloatLiteral(text) => {
                self.fee(text.len().saturating_mul(3).saturating_add(1))?;
                parse_float_literal(text, Some(carrier), Span::default())
                    .map_err(|mut errors| errors.remove(0))?
            }
            SemanticConstRecipe::Formal(reference) => {
                let owner = self.ancestor(scope, reference.scope, depth + 1)?;
                let actual = self
                    .instance(owner)?
                    .actuals
                    .get(reference.ordinal as usize)
                    .ok_or_else(|| bad("const actual omits its original full ordinal"))?;
                let actual = self.holder_actual(owner, reference.ordinal, actual, depth + 1)?;
                let SemanticActual::Const { value, .. } = actual else {
                    return Err(bad("Const formal selects a different actual kind"));
                };
                value
            }
            _ => {
                return Err(unknown(
                    "semantic const expression needs its complete carrier evaluation",
                ));
            }
        };
        if result.ty() != *carrier {
            return Err(bad(
                "semantic constant differs from its resolved actual carrier",
            ));
        }
        Ok(result)
    }
    fn nominal(
        &mut self,
        scope: SemanticInstanceId,
        definition: SemanticDefinitionId,
        args: &[SemanticActualRecipe],
        hint: Option<&Type>,
        depth: usize,
    ) -> R<SemanticTypeValue> {
        self.nominal_application(scope, definition, args, hint, depth)
    }
    fn ty(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticTypeRecipe,
        hint: Option<&Type>,
        depth: usize,
    ) -> R<SemanticTypeValue> {
        if matches!(
            recipe,
            SemanticTypeRecipe::Tuple(_) | SemanticTypeRecipe::ErrorUnion { .. }
        ) {
            Ok(self.type_domain(scope, recipe, hint, depth)?.value)
        } else {
            self.ty_raw(scope, recipe, hint, depth)
        }
    }
    fn ty_raw(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticTypeRecipe,
        hint: Option<&Type>,
        depth: usize,
    ) -> R<SemanticTypeValue> {
        self.node(depth)?;
        let value = match recipe {
            SemanticTypeRecipe::Atom(ty) => resolved(self.physical(ty)?, empty(), Vec::new()),
            SemanticTypeRecipe::Exact(value) => {
                crate::dyn_metadata::semantic_operand_snapshot(
                    value.as_ref(),
                    self.limits,
                    self.left,
                    1,
                )?;
                let mut classes = Vec::new();
                for image in &value.classes {
                    classes.push(self.life(scope, image)?);
                }
                resolved(self.physical(&value.ty)?, value.life.clone(), classes)
            }
            SemanticTypeRecipe::OriginalType {
                scope: selector,
                source,
            } => {
                if let Some(value) =
                    self.original_result_boundary(scope, *selector, source, hint, depth + 1)?
                {
                    return Ok(value);
                }
                let recipe = self.original_type_view(scope, *selector, source, depth + 1)?;
                if let SemanticTypeRecipe::Opaque { definition, .. } = recipe.as_ref()
                    && self.instance(scope)?.definition != *definition
                {
                    if let Some(value) =
                        self.closed_opaque_definition(scope, &recipe, hint, depth + 1)?
                    {
                        return Ok(value);
                    }
                    let diagnostic = unknown(&format!(
                        "opaque original Query has no actual call-result boundary: instance {scope:?}, selector {selector:?}, source {source:?}, opaque definition {definition:?}"
                    ));
                    return Err(self.opaque_boundary_evidence(scope, &recipe, diagnostic, depth + 1)?);
                }
                return self.ty(scope, &recipe, hint, depth + 1);
            }
            SemanticTypeRecipe::Environment {
                scope: selector,
                ordinal,
            } => {
                let owner = self.ancestor(scope, *selector, depth + 1)?;
                let instance = self.instance(owner)?;
                let creator = instance
                    .parent
                    .ok_or_else(|| bad("Type environment has no actual creator"))?;
                let actual = instance
                    .environment
                    .as_ref()
                    .and_then(|env| env.actuals.get(*ordinal as usize))
                    .ok_or_else(|| bad("Type environment omits its sealed slot"))?;
                let SemanticActual::Type(value) = self.actual(creator, actual, depth + 1)? else {
                    return Err(bad("Type environment selector chooses a non-Type slot"));
                };
                let SemanticTypeValue::Resolved { ty, life, .. } = value else {
                    unreachable!()
                };
                self.fee(life.binder_count as usize)?;
                if *selector != SemanticScopeRef::Ancestor(0) && life.binder_count != 0 {
                    return Err(unknown(
                        "ancestor Type environment needs its complete sealed class relocation",
                    ));
                }
                let classes = (0..life.binder_count)
                    .map(|class| SemanticClassExpr::IntrinsicClass {
                        operand: SemanticOperand::Environment(*ordinal),
                        path: Vec::new(),
                        class,
                    })
                    .collect();
                resolved(ty, life, classes)
            }
            SemanticTypeRecipe::Formal(reference) => {
                let owner = self.ancestor(scope, reference.scope, depth + 1)?;
                let mut value = self.type_actual(owner, reference.ordinal, depth + 1)?;
                if let SemanticTypeValue::Resolved { classes, .. } = &mut value {
                    for image in classes {
                        if let SemanticClassExpr::IntrinsicClass {
                            operand: SemanticOperand::TypeArgument { scope, .. },
                            ..
                        } = image
                        {
                            *scope = reference.scope;
                        }
                    }
                }
                value
            }
            SemanticTypeRecipe::SelfType { scope: selector } => {
                let owner = self.ancestor(scope, *selector, depth + 1)?;
                let declaration = self.instance(owner)?.definition;
                let definition = self.definition(declaration)?;
                // Interpret the original declaration in its own solved domain.
                // An instance's claimed Self cannot participate in this derivation.
                let mut value = match &definition.body {
                    SemanticDefinitionBody::Scope(
                        SemanticScopeRecipe::InherentImpl { self_type, .. }
                        | SemanticScopeRecipe::TraitImpl { self_type, .. }
                        | SemanticScopeRecipe::BuiltinImpl { self_type, .. },
                    ) => self.ty(owner, self_type, hint, depth + 1)?,
                    SemanticDefinitionBody::Nominal { .. } => {
                        self.rows(definition.parameters.len())?;
                        let mut arguments = Vec::new();
                        for (ordinal, formal) in definition.parameters.iter().enumerate() {
                            let reference = SemanticFormalRef {
                                scope: SemanticScopeRef::Ancestor(0),
                                ordinal: u32::try_from(ordinal)
                                    .map_err(|_| bad("nominal Self formal ordinal overflow"))?,
                            };
                            arguments.push(match formal {
                                SemanticFormal::Lifetime => SemanticActualRecipe::Life(
                                    SemanticLifeRecipe::Formal(reference),
                                ),
                                SemanticFormal::Type { .. } => SemanticActualRecipe::Type(
                                    SemanticTypeRecipe::Formal(reference),
                                ),
                                SemanticFormal::Const { carrier } => {
                                    crate::dyn_metadata::semantic_operand_snapshot(
                                        carrier, self.limits, self.left, 1,
                                    )?;
                                    SemanticActualRecipe::Const {
                                        carrier: carrier.clone(),
                                        value: SemanticConstRecipe::Formal(reference),
                                    }
                                }
                            });
                        }
                        self.nominal(owner, declaration, &arguments, hint, depth + 1)?
                    }
                    _ => {
                        return Err(unknown(
                            "Self requires its checked original implementation or nominal scope",
                        ));
                    }
                };
                let SemanticScopeRef::Ancestor(hops) = selector;
                self.rehome_classes(&mut value, *hops)?;
                value
            }
            SemanticTypeRecipe::Nominal {
                definition,
                arguments,
            } => self.nominal(scope, *definition, arguments, hint, depth + 1)?,
            SemanticTypeRecipe::Address {
                lifetime, inner, ..
            }
            | SemanticTypeRecipe::Desc { lifetime, inner } => {
                let SemanticTypeValue::Resolved {
                    ty: inner,
                    life: inner_life,
                    classes: inner_classes,
                } = self.ty(scope, inner, None, depth + 1)?
                else {
                    unreachable!()
                };
                let is_address = matches!(recipe, SemanticTypeRecipe::Address { .. });
                let mutable = match recipe {
                    SemanticTypeRecipe::Address { mutable, .. } => *mutable,
                    _ => true,
                };
                if matches!(lifetime, SemanticOuterLifeRecipe::Static) && mutable {
                    return Err(bad("Static outer occurrence cannot grant mutable access"));
                }
                let mut classes = Vec::new();
                let mut life = empty();
                if let SemanticOuterLifeRecipe::Class(image) = lifetime {
                    classes.push(self.life(scope, image)?);
                    life.slots.push(StorageLifeSlot {
                        path: Vec::new(),
                        binder: 0,
                        mode: if is_address {
                            if mutable {
                                StoredBorrowMode::ExclusiveAddress
                            } else {
                                StoredBorrowMode::SharedAddress
                            }
                        } else {
                            StoredBorrowMode::WritableDescription
                        },
                    });
                }
                let offset = classes.len() as u32;
                self.fee(inner_classes.len().saturating_mul(2))?;
                classes.extend(inner_classes);
                if inner_life.binder_count as usize + offset as usize != classes.len() {
                    return Err(bad("pointee complete class inventory is inconsistent"));
                }
                life.binder_count = u32::try_from(classes.len())
                    .map_err(|_| bad("outer class inventory overflows"))?;
                let map = (offset..life.binder_count).collect::<Vec<_>>();
                inherit_child_shape(
                    &mut life,
                    &inner_life,
                    LifeTypeProjection::Pointee,
                    &map,
                    self.limits,
                    self.left,
                )?;
                resolved(
                    if is_address {
                        Type::Address {
                            mutable,
                            inner: Box::new(inner),
                        }
                    } else {
                        Type::Desc(Box::new(inner))
                    },
                    life,
                    classes,
                )
            }
            SemanticTypeRecipe::Slice(inner) => {
                let SemanticTypeValue::Resolved {
                    ty,
                    life: child,
                    classes,
                } = self.ty(scope, inner, None, depth + 1)?
                else {
                    unreachable!()
                };
                self.fee(classes.len())?;
                let mut life = empty();
                life.binder_count = child.binder_count;
                let map = (0..child.binder_count).collect::<Vec<_>>();
                inherit_child_shape(
                    &mut life,
                    &child,
                    LifeTypeProjection::Element,
                    &map,
                    self.limits,
                    self.left,
                )?;
                resolved(
                    Type::Slice {
                        element: Box::new(ty),
                    },
                    life,
                    classes,
                )
            }
            SemanticTypeRecipe::Array { element, length } => {
                let SemanticTypeValue::Resolved { ty, life, classes } =
                    self.ty(scope, element, None, depth + 1)?
                else {
                    unreachable!()
                };
                if life.binder_count as usize != classes.len() {
                    return Err(bad(
                        "array element omits its complete intrinsic class domain",
                    ));
                }
                let length = self.constant(scope, length, &Type::Usize, depth + 1)?;
                let Constant::Usize(length) = length else {
                    return Err(bad("array length has another original constant carrier"));
                };
                let length = u32::try_from(length)
                    .map_err(|_| bad("array length exceeds its complete runtime layout"))?;
                if length as usize > self.limits.max_nodes {
                    return Err(Diagnostic::limit(
                        Phase::Types,
                        "array length exceeds semantic node budget",
                        None,
                    ));
                }
                self.fee(length as usize)?;
                let mut array_life = empty();
                array_life.binder_count = life.binder_count;
                // Every physical element occurrence survives, including N=0.
                // Element is the one complete type-domain projection; it
                // grants neither a StoreElement premise nor a byte address.
                for index in 0..length {
                    for slot in &life.slots {
                        self.fee(slot.path.len().saturating_add(2))?;
                        let mut path = vec![PlaceProjection::Field(index)];
                        path.extend_from_slice(&slot.path);
                        array_life.slots.push(StorageLifeSlot {
                            path,
                            ..slot.clone()
                        });
                    }
                }
                for row in &life.retained {
                    self.fee(row.path.len().saturating_add(2))?;
                    let mut path = vec![LifeTypeProjection::Element];
                    path.extend_from_slice(&row.path);
                    array_life.retained.push(StorageRetainedBinding {
                        path,
                        ..row.clone()
                    });
                }
                // Element is a complete type-domain boundary even when its
                // classes have no physical slot or only a phantom occurrence.
                self.fee(classes.len())?;
                for class in 0..life.binder_count {
                    array_life.retained.push(StorageRetainedBinding {
                        path: vec![LifeTypeProjection::Element],
                        class,
                        binder: class,
                    });
                }
                for row in &life.nominal_arguments {
                    self.fee(row.path.len().saturating_add(2))?;
                    let mut path = vec![LifeTypeProjection::Element];
                    path.extend_from_slice(&row.path);
                    array_life.nominal_arguments.push(NominalLifeArgument {
                        path,
                        ..row.clone()
                    });
                }
                resolved(
                    Type::Array {
                        element: Box::new(ty),
                        length,
                    },
                    array_life,
                    classes,
                )
            }
            SemanticTypeRecipe::Tuple(_) => self.ty(scope, recipe, hint, depth + 1)?,
            SemanticTypeRecipe::Managed(inner)
            | SemanticTypeRecipe::OwnedStore(inner)
            | SemanticTypeRecipe::Mutex(inner)
            | SemanticTypeRecipe::Registration(inner)
            | SemanticTypeRecipe::AsyncRegistration(inner) => {
                let SemanticTypeValue::Resolved {
                    ty,
                    life: child,
                    classes,
                } = self.ty(scope, inner, None, depth + 1)?
                else {
                    unreachable!()
                };
                if child.binder_count as usize != classes.len() {
                    return Err(bad("owner carrier omits its complete child domain"));
                }
                self.fee(classes.len())?;
                let projection = if matches!(recipe, SemanticTypeRecipe::OwnedStore(_)) {
                    LifeTypeProjection::Element
                } else {
                    LifeTypeProjection::Pointee
                };
                let mut life = StorageLifeShape {
                    binder_count: child.binder_count,
                    ..empty()
                };
                let map = (0..child.binder_count).collect::<Vec<_>>();
                inherit_child_shape(&mut life, &child, projection, &map, self.limits, self.left)?;
                let ty = match recipe {
                    SemanticTypeRecipe::Managed(_) => Type::Managed(Box::new(ty)),
                    SemanticTypeRecipe::OwnedStore(_) => Type::OwnedStore {
                        element: Box::new(ty),
                    },
                    SemanticTypeRecipe::Mutex(_) => Type::Mutex {
                        payload: Box::new(ty),
                    },
                    SemanticTypeRecipe::Registration(_) => Type::Registration {
                        environment: Box::new(ty),
                    },
                    SemanticTypeRecipe::AsyncRegistration(_) => Type::AsyncRegistration {
                        environment: Box::new(ty),
                    },
                    _ => unreachable!(),
                };
                resolved(ty, life, classes)
            }
            SemanticTypeRecipe::MutexGuard { payload, retained }
            | SemanticTypeRecipe::Task {
                result: payload,
                retained,
            } => {
                let SemanticTypeValue::Resolved {
                    ty: payload,
                    life: payload_life,
                    classes: payload_classes,
                } = self.ty(scope, payload, None, depth + 1)?
                else {
                    unreachable!()
                };
                let (shape, classes) = if matches!(recipe, SemanticTypeRecipe::Task { .. }) {
                    self.task_shape(scope, retained, depth + 1)?
                } else {
                    self.retained_shape(scope, retained, depth + 1)?
                };
                if matches!(recipe, SemanticTypeRecipe::Task { .. }) {
                    self.retained_child(
                        &shape,
                        &classes,
                        &[LifeTypeProjection::Result],
                        &payload_life,
                        &payload_classes,
                    )?;
                } else if payload_life.binder_count != 0 {
                    return Err(unknown(
                        "guard payload needs its complete actual retained port class projection",
                    ));
                }
                let life = StorageLifeShape {
                    binder_count: shape.binder_count,
                    slots: Vec::new(),
                    retained: Vec::new(),
                    nominal_arguments: Vec::new(),
                };
                resolved(
                    if matches!(recipe, SemanticTypeRecipe::MutexGuard { .. }) {
                        Type::MutexGuard {
                            payload: Box::new(payload),
                            retained_life: shape,
                        }
                    } else {
                        Type::Task {
                            result: Box::new(payload),
                            retained_life: shape,
                        }
                    },
                    life,
                    classes,
                )
            }
            SemanticTypeRecipe::Qualified { storage, life } => {
                let SemanticTypeValue::Resolved { ty: storage, .. } =
                    self.ty(scope, storage, None, depth + 1)?
                else {
                    unreachable!()
                };
                let (life, classes) = self.storage_shape(scope, life, depth + 1)?;
                let ty = if let Type::BorrowedStorage { life: existing, .. } = &storage {
                    if existing != &life {
                        return Err(bad(
                            "qualified recipe changes an existing complete nominal boundary",
                        ));
                    }
                    storage
                } else {
                    Type::BorrowedStorage {
                        storage: Box::new(storage),
                        life: self.copy_shape(&life)?,
                    }
                };
                resolved(ty, life, classes)
            }
            SemanticTypeRecipe::Closure {
                definition,
                arguments,
                environment,
            } => {
                let declaration = self.definition(*definition)?;
                if arguments.len() != declaration.parameters.len()
                    || environment.len() != declaration.environment.len()
                {
                    return Err(bad(
                        "closure recipe omits original mixed arguments or ghost slots",
                    ));
                }
                let inferred_id;
                let id = match hint {
                    Some(Type::Closure(id)) => id,
                    None => {
                        let creator = self.function(scope)?.id;
                        self.fee(
                            self.input
                                .instances
                                .len()
                                .saturating_add(self.ledger.frames.len()),
                        )?;
                        let mut found = None;
                        for child in self.module.semantic_instances(self.input) {
                            if child.parent != Some(scope) || child.definition != *definition {
                                continue;
                            }
                            let Some(SemanticEnvironmentInstance {
                                closure: SemanticClosureTarget::Typed(id),
                                ..
                            }) = &child.environment
                            else {
                                continue;
                            };
                            let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(
                                entry,
                            )) = child.target
                            else {
                                continue;
                            };
                            let Some(frame) = self.ledger.frames.get(&entry) else {
                                continue;
                            };
                            if frame.parent != Some((creator, *id)) {
                                continue;
                            }
                            if found.replace(*id).is_some() {
                                return Err(unknown(
                                    "closure recipe has multiple actual creator-specialization identities",
                                ));
                            }
                        }
                        let Some(found) = found else {
                            let current = self.instance(scope)?;
                            crate::dyn_metadata::semantic_operand_snapshot(current, self.limits, self.left, 1)?;
                            for actual in arguments.iter().chain(environment) {
                                crate::dyn_metadata::semantic_operand_snapshot(actual, self.limits, self.left, 1)?;
                            }
                            let detail = format!(
                                "closure creator request: use {scope:?}, use definition {:?}, actual creator function {creator:?}; closure definition {definition:?}, declared parent {:?}; full arguments {arguments:?}; full environment {environment:?}; use actuals {:?}",
                                current.definition, declaration.parent, current.actuals,
                            );
                            self.fee(detail.len())?;
                            let mut error = unknown("closure recipe has no exact actual creator emission");
                            error.notes.push(detail);
                            let module = self.module;
                            let input = self.input;
                            let ledger = self.ledger;
                            self.fee(input.instances.len().saturating_add(ledger.frames.len()))?;
                            for child in module.semantic_instances(input) {
                                if child.definition != *definition { continue; }
                                crate::dyn_metadata::semantic_operand_snapshot(child, self.limits, self.left, 1)?;
                                let frame = match child.target {
                                    SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(entry)) => ledger.frames.get(&entry),
                                    _ => None,
                                };
                                let detail = format!("actual closure candidate: {child:?}; emitted origin {:?}; emitted creator {:?}", frame.map(|frame| frame.origin), frame.and_then(|frame| frame.parent));
                                self.fee(detail.len())?;
                                error.notes.push(detail);
                            }
                            return Err(error);
                        };
                        inferred_id = found;
                        &inferred_id
                    }
                    _ => {
                        return Err(bad(
                            "closure recipe contradicts its actual whole operand type",
                        ));
                    }
                };
                self.fee(
                    self.module
                        .closures
                        .len()
                        .saturating_add(self.input.instances.len()),
                )?;
                let closure = self
                    .module
                    .closures
                    .iter()
                    .find(|row| row.id == *id)
                    .ok_or_else(|| bad("whole closure type has no actual definition"))?;
                let mut children = self.module.semantic_instances(self.input).filter(|child| child.parent == Some(scope) && child.definition == *definition && matches!(child.target, SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(f)) if f == closure.function));
                let child = children
                    .next()
                    .ok_or_else(|| bad("closure recipe has no matching actual original child"))?;
                if children.next().is_some() {
                    return Err(bad("closure recipe has ambiguous actual child worlds"));
                }
                let Some(life) = closure.storage_life.as_ref() else {
                    // Only the formation query interprets this exact missing
                    // declaration dependency as NotReady. Ordinary semantic
                    // readers still fail and cannot observe an empty domain.
                    self.pending_header = Some(*id);
                    return Err(Diagnostic::new(Phase::Types, zil_base::ResultCategory::Unsupported,
                        PENDING_HEADER, format!("whole closure type requires a complete actual header: instance {scope:?}, definition {definition:?}, child {:?}, actual closure {id:?}, entry {:?}", child.id, closure.function), None));
                };
                self.fee(life.fixed_binder_count as usize + child.header_embedding.len())?;
                let mut images = vec![None; life.fixed_binder_count as usize];
                for row in &child.header_embedding {
                    let StorageBinder::Fixed(fixed) = row.header else {
                        continue;
                    };
                    let image = match &row.actual {
                        SemanticClassExpr::Environment {
                            scope: SemanticScopeRef::Ancestor(0),
                            ordinal,
                        } => {
                            let recipe = environment.get(*ordinal as usize).ok_or_else(|| {
                                bad("closure Fixed image selects an absent ghost slot")
                            })?;
                            let SemanticActual::Life(image) =
                                self.recipe_actual(scope, recipe, depth + 1)?
                            else {
                                return Err(bad(
                                    "closure Fixed image selects a non-Life ghost slot",
                                ));
                            };
                            image
                        }
                        SemanticClassExpr::IntrinsicClass {
                            operand: SemanticOperand::Environment(ordinal),
                            path,
                            class,
                        } if path.is_empty() => {
                            let recipe = environment.get(*ordinal as usize).ok_or_else(|| {
                                bad("closure Fixed image selects an absent Type environment slot")
                            })?;
                            let SemanticActual::Type(SemanticTypeValue::Resolved {
                                life,
                                classes,
                                ..
                            }) = self.recipe_actual(scope, recipe, depth + 1)?
                            else {
                                return Err(bad(
                                    "closure Fixed image does not select a whole-Type slot",
                                ));
                            };
                            if life.binder_count as usize != classes.len() {
                                return Err(bad("sealed Type environment omits intrinsic classes"));
                            }
                            let image = classes.get(*class as usize).ok_or_else(|| {
                                bad("closure Fixed image exceeds sealed Type environment domain")
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
                            ..
                        } => {
                            // The original implicit is checked against the actual
                            // child read by headers::program. Its image at this
                            // use comes from this creator's unique actual ctor.
                            let creator = self.function(scope)?;
                            self.fee(life.bindings.len())?;
                            let mut positions = life
                                .bindings
                                .iter()
                                .filter(|b| b.binder == StorageBinder::Fixed(fixed));
                            let position = positions.next().ok_or_else(|| {
                                bad("implicit closure class has no actual capture occurrence")
                            })?;
                            if positions.next().is_some() || !position.position.path.is_empty() {
                                return Err(unknown(
                                    "implicit closure class requires full nested occurrence sealing",
                                ));
                            }
                            let StorageBase::Capture(capture) = position.position.base else {
                                return Err(bad(
                                    "implicit environment class is not a capture head",
                                ));
                            };
                            let mut source = None;
                            for block in creator.blocks {
                                self.fee(block.instructions.len())?;
                                for op in &block.instructions {
                                    if let InstructionKind::MakeClosure {
                                        closure: actual,
                                        captures,
                                    } = &op.kind
                                    {
                                        if actual != id {
                                            continue;
                                        }
                                        let value =
                                            *captures.get(capture as usize).ok_or_else(|| {
                                                bad("actual closure ctor omits capture head")
                                            })?;
                                        if source.replace(value).is_some() {
                                            return Err(unknown(
                                                "multiple actual closure creations need occurrence-specific whole-type images",
                                            ));
                                        }
                                    }
                                }
                            }
                            let value = source.ok_or_else(|| {
                                bad("closure implicit class has no actual creator operand")
                            })?;
                            let mode = match closure.captures.get(capture as usize) {
                                Some(Type::Address { mutable: false, .. }) => {
                                    StoredBorrowMode::SharedAddress
                                }
                                Some(Type::Address { mutable: true, .. }) => {
                                    StoredBorrowMode::ExclusiveAddress
                                }
                                _ => {
                                    return Err(bad(
                                        "implicit capture head is not an address occurrence",
                                    ));
                                }
                            };
                            SemanticClassExpr::OuterOccurrence {
                                operand: SemanticOperand::Value(value),
                                path: Vec::new(),
                                mode,
                            }
                        }
                        _ => {
                            return Err(unknown(
                                "closure Fixed image needs its complete lexical seal",
                            ));
                        }
                    };
                    let slot = images
                        .get_mut(fixed as usize)
                        .ok_or_else(|| bad("closure Fixed embedding exceeds its full domain"))?;
                    if slot.replace(image).is_some() {
                        return Err(bad("closure Fixed embedding repeats a class"));
                    }
                }
                if images.iter().any(Option::is_none) {
                    self.fee(images.len().saturating_add(self.input.sites.len()))?;
                    let missing: Vec<_> = images
                        .iter()
                        .enumerate()
                        .filter_map(|(k, image)| image.is_none().then_some(k))
                        .collect();
                    for row in &child.header_embedding {
                        crate::dyn_metadata::semantic_operand_snapshot(
                            &row.actual,
                            self.limits,
                            self.left,
                            1,
                        )?;
                    }
                    let observations: Vec<_> = self
                        .input
                        .sites
                        .iter()
                        .filter(|site| site.instance == child.id)
                        .filter_map(|site| {
                            if let SemanticSiteAction::TypeUse { subject, .. } = &site.action {
                                Some((site.recipe, subject))
                            } else {
                                None
                            }
                        })
                        .collect();
                    self.pending_header = Some(*id);
                    return Err(Diagnostic::new(
                        Phase::Types,
                        zil_base::ResultCategory::Unsupported,
                        PENDING_HEADER,
                        format!(
                            "whole closure type is missing an actual Fixed image: closure {:?}, child {:?}, definition {:?}, missing {:?}, embeddings {:?}, type-use subjects {:?}",
                            closure.id,
                            child.id,
                            child.definition,
                            missing,
                            child.header_embedding,
                            observations
                        ),
                        None,
                    ));
                }
                let images = images
                    .into_iter()
                    .collect::<Option<Vec<_>>>()
                    .ok_or_else(|| bad("completed Fixed image collection changed"))?;
                resolved(
                    Type::Closure(*id),
                    StorageLifeShape {
                        binder_count: life.fixed_binder_count,
                        slots: Vec::new(),
                        retained: Vec::new(),
                        nominal_arguments: Vec::new(),
                    },
                    images,
                )
            }
            SemanticTypeRecipe::Code { kind, signature } => {
                if signature.bound_lifetimes != 0 || signature.invocation_lifetimes != 0 {
                    return Err(unknown(
                        "code type needs its own complete quantified invocation relation",
                    ));
                }
                if signature.call_kind != CallKind::Direct
                    || !matches!(
                        signature.result_mode,
                        PassingMode::Infer | PassingMode::Copy
                    )
                {
                    return Err(unknown(
                        "code pointer type cannot erase its declared ABI or result transfer mode",
                    ));
                }
                self.fee(signature.parameters.len())?;
                let mut parameters = Vec::new();
                for parameter in &signature.parameters {
                    if !matches!(parameter.mode, PassingMode::Infer | PassingMode::Copy) {
                        return Err(unknown(
                            "code pointer parameter needs its exact transfer-mode representation",
                        ));
                    }
                    let SemanticTypeValue::Resolved { ty, life, classes } =
                        self.ty(scope, &parameter.ty, None, depth + 1)?
                    else {
                        unreachable!()
                    };
                    if life != empty()
                        || !classes.is_empty()
                        || !functions::closed_code_slot(*kind, &ty, self.limits, self.left, depth + 1)?
                    {
                        return Err(unknown(
                            "code pointer parameter needs its full class/signature relation",
                        ));
                    }
                    parameters.push(ty);
                }
                let SemanticTypeValue::Resolved {
                    ty: result,
                    life,
                    classes,
                } = self.ty(scope, &signature.result, None, depth + 1)?
                else {
                    unreachable!()
                };
                if life != empty()
                    || !classes.is_empty()
                    || !functions::closed_code_slot(*kind, &result, self.limits, self.left, depth + 1)?
                {
                    return Err(unknown(
                        "code pointer result needs its full class/signature relation",
                    ));
                }
                let ty = match kind {
                    SemanticCodeKind::ForeignCallback => {
                        if !foreign::callback_signature(&parameters, &result) {
                            return Err(bad("foreign callback changes its complete C scalar ABI"));
                        }
                        Type::ForeignCallback {
                            parameters,
                            result: Box::new(result),
                        }
                    }
                    SemanticCodeKind::FunctionPointer => Type::FunctionPointer {
                        parameters,
                        result: Box::new(result),
                    },
                    SemanticCodeKind::BorrowedFunctionPointer => {
                        return Err(unknown(
                            "borrowed code pointer needs its complete invocation-local header",
                        ));
                    }
                };
                resolved(ty, empty(), Vec::new())
            }
            SemanticTypeRecipe::Dyn { owned, interface } => {
                self.dyn_type(scope, *owned, interface, hint, depth + 1)?
            }
            SemanticTypeRecipe::ErrorUnion { .. } => self.ty(scope, recipe, hint, depth + 1)?,
            SemanticTypeRecipe::AssociatedType { .. } => {
                self.associated_source_type(scope, recipe, hint, depth + 1)?
            }
            SemanticTypeRecipe::Opaque {
                definition,
                arguments,
                hidden,
            } => {
                let original = self.definition(*definition)?;
                if arguments.len() != original.parameters.len() {
                    return Err(bad(
                        "opaque application omits original mixed actual positions",
                    ));
                }
                if self.instance(scope)?.definition != *definition {
                    if let Some(value) =
                        self.closed_opaque_definition(scope, recipe, hint, depth + 1)?
                    {
                        return Ok(value);
                    }
                    let diagnostic = unknown(&format!(
                        "opaque hidden type requires its actual original call-result boundary: instance {scope:?}, actual definition {:?}, opaque definition {definition:?}",
                        self.instance(scope)?.definition
                    ));
                    return Err(self.opaque_boundary_evidence(scope, recipe, diagnostic, depth + 1)?);
                }
                let value = self.ty(scope, hidden, hint, depth + 1)?;
                let SemanticTypeValue::Resolved { classes, .. } = &value else {
                    unreachable!()
                };
                // An opaque nominal occurrence cannot disappear merely because
                // its physical hidden representation currently has no leaf.
                self.fee(arguments.len().saturating_add(classes.len()))?;
                for argument in arguments {
                    let actual = self.recipe_actual(scope, argument, depth + 1)?;
                    match actual {
                        SemanticActual::Life(image) if classes.contains(&image) => {}
                        SemanticActual::Type(SemanticTypeValue::Resolved {
                            life,
                            classes: images,
                            ..
                        }) if life.binder_count == 0 && images.is_empty() => {}
                        SemanticActual::Const {
                            ty:
                                SemanticTypeValue::Resolved {
                                    life,
                                    classes: images,
                                    ..
                                },
                            ..
                        } if life.binder_count == 0 && images.is_empty() => {}
                        _ => {
                            return Err(unknown(
                                "opaque full original domain needs additional persistent hidden class occurrences",
                            ));
                        }
                    }
                }
                value
            }
            _ => {
                return Err(unknown(&format!(
                    "semantic type recipe requires its complete actual class materializer: instance {scope:?}, recipe {:?}",
                    std::mem::discriminant(recipe)
                )));
            }
        };
        if let Some(hint) = hint {
            let SemanticTypeValue::Resolved { ty, .. } = &value else {
                unreachable!()
            };
            inference::charge_type_snapshot(ty, Span::default(), self.limits, self.left, 1)?;
            inference::charge_type_snapshot(hint, Span::default(), self.limits, self.left, 1)?;
            if ty != hint {
                return Err(bad(&format!(
                    "original type recipe differs from its actual emitted complete type: instance {scope:?}, expected {}, actual {}",
                    ty.name(),
                    hint.name()
                )));
            }
        }
        Ok(value)
    }
    fn recipe_actual(
        &mut self,
        scope: SemanticInstanceId,
        recipe: &SemanticActualRecipe,
        depth: usize,
    ) -> R<SemanticActual> {
        self.node(depth)?;
        Ok(match recipe {
            SemanticActualRecipe::Infer => {
                return Err(bad(
                    "unresolved original inference variable survived its call",
                ));
            }
            SemanticActualRecipe::Type(recipe) => {
                SemanticActual::Type(self.ty(scope, recipe, None, depth + 1)?)
            }
            SemanticActualRecipe::Life(recipe) => SemanticActual::Life(self.life(scope, recipe)?),
            SemanticActualRecipe::Const { carrier, value } => {
                let ty = self.ty(scope, carrier, None, depth + 1)?;
                let SemanticTypeValue::Resolved {
                    ty: carrier, life, ..
                } = &ty
                else {
                    unreachable!()
                };
                if life.binder_count != 0 {
                    return Err(bad("const recipe has a free-class carrier"));
                }
                let value = self.constant(scope, value, carrier, depth + 1)?;
                SemanticActual::Const { ty, value }
            }
        })
    }
    fn holder_actual(
        &mut self,
        holder: SemanticInstanceId,
        ordinal: u32,
        actual: &SemanticActual,
        depth: usize,
    ) -> R<SemanticActual> {
        if let SemanticActual::CoreInferred {
            scope,
            call,
            formal,
        } = actual
        {
            if *formal != ordinal {
                return Err(bad("inferred actual changes its full holder ordinal"));
            }
            return self.inferred_actual(*scope, *call, *formal, Some(holder), depth + 1);
        }
        self.actual(holder, actual, depth)
    }
    fn actual(
        &mut self,
        owner: SemanticInstanceId,
        actual: &SemanticActual,
        depth: usize,
    ) -> R<SemanticActual> {
        if let SemanticActual::CoreRecipe { scope, recipe } = actual {
            return self.recipe_actual(*scope, recipe, depth);
        }
        self.node(depth)?;
        Ok(match actual {
            SemanticActual::Type(value) => {
                SemanticActual::Type(self.value(owner, value, None, depth + 1)?)
            }
            SemanticActual::Const { ty, value } => {
                let ty = self.value(owner, ty, None, depth + 1)?;
                let SemanticTypeValue::Resolved {
                    ty: carrier, life, ..
                } = &ty
                else {
                    unreachable!()
                };
                if value.ty() != *carrier || life.binder_count != 0 {
                    return Err(bad("const actual has another complete carrier"));
                }
                self.fee(match value {
                    Constant::WideString(units) => units.len().saturating_add(1),
                    _ => 1,
                })?;
                SemanticActual::Const {
                    ty,
                    value: value.clone(),
                }
            }
            SemanticActual::Life(image) => SemanticActual::Life(self.class(owner, image)?),
            SemanticActual::CoreInferred {
                scope,
                call,
                formal,
            } => self.inferred_actual(*scope, *call, *formal, None, depth + 1)?,
            SemanticActual::CoreRecipe { .. } => unreachable!(),
        })
    }
}

/// Materialize every holding point, including unused instance actuals and
/// declaration constraints. Original recipes remain present for the subsequent
/// independent checks; no residual Core operand is interpreted as an empty one.
pub(super) fn program(
    program: &mut SemanticModule,
    module: &TypedModule,
    ledger: &Ledger,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    // The immutable original definitions never change in this pass. Borrow
    // the complete old input while constructing independent mutable outputs;
    // do not deep-copy every original body merely to avoid a borrow conflict.
    let directory = if let Some(context) = ledger.original.context() {
        ActualDirectory::indexed(
            module,
            &*program,
            context,
            "materialize::program",
            limits,
            left,
        )?
    } else {
        ActualDirectory::from_module_at(module, "materialize::program")
    };
    let mut eval = Eval {
        input: &*program,
        module: &directory,
        ledger,
        limits,
        left,
        active: BTreeSet::new(),
        invocation_probe: None,
        pending_header: None,
    };
    eval.rows(program.instances.len())?;
    eval.rows(program.sites.len())?;
    eval.fee(program.instances.len().saturating_add(program.sites.len()))?;
    // Keep scalar identities and placement rows in place. Only transformed
    // fields need new storage; the complete old input remains visible until
    // every proposal has succeeded, so failures cannot publish partial output.
    let mut instances = Vec::with_capacity(program.instances.len());
    for instance in &program.instances {
        eval.rows(instance.actuals.len())?;
        let mut actuals = Vec::with_capacity(instance.actuals.len());
        for (ordinal, actual) in instance.actuals.iter().enumerate() {
            let mut value = eval.holder_actual(instance.id, ordinal as u32, actual, 0)?;
            if matches!(value, SemanticActual::Life(_))
                && instance.lifetime_instantiation == SemanticLifetimeInstantiation::Rigid
            {
                let canonical = eval.canonical_formal(
                    instance.id,
                    &SemanticFormalRef {
                        scope: SemanticScopeRef::Ancestor(0),
                        ordinal: ordinal as u32,
                    },
                )?;
                if !matches!(actual, SemanticActual::CoreInferred { .. })
                    && value != SemanticActual::Life(canonical.clone())
                {
                    return Err(bad(
                        "rigid instance life claim imports a caller coincidence",
                    ));
                }
                value = SemanticActual::Life(canonical);
            }
            if let SemanticActual::Type(SemanticTypeValue::Resolved { life, classes, .. }) =
                &mut value
            {
                // Actual Type class N is intrinsic. Callee code never imports
                // caller SSA or recursively consults its own asserted images.
                eval.fee(life.binder_count as usize)?;
                *classes = (0..life.binder_count)
                    .map(|class| SemanticClassExpr::IntrinsicClass {
                        operand: SemanticOperand::TypeArgument {
                            scope: SemanticScopeRef::Ancestor(0),
                            ordinal: ordinal as u32,
                        },
                        path: Vec::new(),
                        class,
                    })
                    .collect();
            }
            actuals.push(value);
        }
        let environment = if let Some(environment) = &instance.environment {
            let creator = instance
                .parent
                .ok_or_else(|| bad("actual ghost environment has no real lexical creator"))?;
            eval.rows(environment.actuals.len())?;
            let mut values = Vec::with_capacity(environment.actuals.len());
            for value in &environment.actuals {
                values.push(eval.actual(creator, value, 0)?);
            }
            Some(values)
        } else {
            None
        };
        let self_type = instance
            .self_type
            .as_ref()
            .map(|claim| eval.self_claim(instance.id, claim))
            .transpose()?;
        eval.rows(instance.header_embedding.len())?;
        let mut headers = Vec::with_capacity(instance.header_embedding.len());
        for binding in &instance.header_embedding {
            headers.push(eval.class(instance.id, &binding.actual)?);
        }
        instances.push((actuals, environment, self_type, headers));
    }
    let mut sites = Vec::with_capacity(program.sites.len());
    for site in &program.sites {
        let f = eval.function(site.instance)?;
        let action = match &site.action {
            SemanticSiteAction::CoreBodyCall => {
                let owner = eval.instance(site.instance)?.definition;
                let count = {
                    let proof = ledger.original.borrow();
                    let proof = proof
                        .as_ref()
                        .ok_or_else(|| bad("call lacks completed original inference"))?;
                    proof
                        .calls
                        .get(&(owner, site.recipe))
                        .ok_or_else(|| bad("actual call lacks original fresh-variable solution"))?
                        .len()
                };
                eval.rows(count)?;
                let mut actuals = Vec::with_capacity(count);
                for formal in 0..count {
                    let formal = u32::try_from(formal)
                        .map_err(|_| bad("actual call full formal ordinal overflows"))?;
                    let value =
                        eval.inferred_actual(site.instance, site.recipe, formal, None, 0)?;
                    actuals.push(eval.actual(site.instance, &value, 0)?);
                }
                SemanticSiteAction::BodyCall { actuals }
            }
            SemanticSiteAction::Construct {
                result: SemanticOperand::Value(result),
                value,
            } => {
                SemanticSiteAction::Construct {
                    result: SemanticOperand::Value(*result),
                    value: eval.constructor_value(
                        site.instance,
                        value,
                        *result,
                        site.recipe,
                    )?,
                }
            }
            SemanticSiteAction::Construct { .. } => {
                return Err(bad("constructor result was not relocated to actual SSA"));
            }
            SemanticSiteAction::Closure {
                result,
                closure,
                environment,
            } => {
                eval.rows(environment.len())?;
                let mut values = Vec::with_capacity(environment.len());
                for value in environment {
                    values.push(eval.actual(site.instance, value, 0)?);
                }
                SemanticSiteAction::Closure {
                    result: result.clone(),
                    closure: closure.clone(),
                    environment: values,
                }
            }
            SemanticSiteAction::TypeUse { subject, expected } => {
                let hint = logical_operand(eval.ledger, eval.module, f, subject, eval.limits, eval.left)?;
                let value = eval.value(site.instance, expected, Some(hint), 0).map_err(|mut diagnostic| {
                    diagnostic.notes.push(format!(
                        "actual type-use: instance {:?}, recipe {:?}, subject {:?}, type constructor {:?}, function {:?}, return constructor {:?}",
                        site.instance, site.recipe, subject, std::mem::discriminant(hint.ty), f.id, std::mem::discriminant(&f.return_type)
                    ));
                    diagnostic
                })?;
                SemanticSiteAction::TypeUse {
                    subject: subject.clone(),
                    expected: value,
                }
            }
            SemanticSiteAction::DeclareLocal {
                local: SemanticLocalTarget::Typed(local),
                expected,
            } => {
                let logical = LogicalExpected::local(eval.ledger.logical_local_type(f, *local, eval.limits, eval.left)?);
                let value = eval.value(site.instance, expected, Some(logical), 0).map_err(|mut diagnostic| {
                    diagnostic.notes.push(format!(
                        "actual local declaration: instance {:?}, recipe {:?}, local {:?}, type constructor {:?}",
                        site.instance, site.recipe, local, std::mem::discriminant(logical.ty)
                    ));
                    diagnostic
                })?;
                SemanticSiteAction::DeclareLocal {
                    local: SemanticLocalTarget::Typed(*local),
                    expected: value,
                }
            }
            SemanticSiteAction::Call { actuals, .. } | SemanticSiteAction::BodyCall { actuals } => {
                eval.rows(actuals.len())?;
                let mut values = Vec::with_capacity(actuals.len());
                for actual in actuals {
                    values.push(eval.actual(site.instance, actual, 0)?);
                }
                match &site.action {
                    SemanticSiteAction::Call { target, .. } => SemanticSiteAction::Call {
                        target: target.clone(),
                        actuals: values,
                    },
                    _ => SemanticSiteAction::BodyCall { actuals: values },
                }
            }
            SemanticSiteAction::CallableCall { callee } => SemanticSiteAction::CallableCall {
                callee: callee.clone(),
            },
            SemanticSiteAction::DeclareLocal { .. } => {
                return Err(bad("Core declaration target survived actual relocation"));
            }
        };
        sites.push(action);
    }
    drop(eval);
    drop(directory);
    for (instance, (actuals, environment, self_type, headers)) in
        program.instances.iter_mut().zip(instances)
    {
        instance.actuals = actuals;
        if let (Some(target), Some(actuals)) = (&mut instance.environment, environment) {
            target.actuals = actuals;
        }
        instance.self_type = self_type;
        for (binding, image) in instance.header_embedding.iter_mut().zip(headers) {
            binding.actual = image;
        }
    }
    for (site, action) in program.sites.iter_mut().zip(sites) {
        site.action = action;
    }
    Ok(())
}

/// Interpret an original type in an exact emitted instance. The caller still
/// proves the original world and the actual body's class flow independently.
pub(super) fn type_at(
    program: &SemanticModule,
    module: &TypedModule,
    ledger: &Ledger,
    scope: SemanticInstanceId,
    recipe: &SemanticTypeRecipe,
    hint: Option<&Type>,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticTypeValue> {
    Eval {
        input: program,
        module: &ActualDirectory::from_module_at(module, "materialize::type_at"),
        ledger,
        limits,
        left,
        active: BTreeSet::new(),
        invocation_probe: None,
        pending_header: None,
    }
    .ty(scope, recipe, hint, 0)
}

pub(super) enum Formation<T> {
    Complete(T),
    NotReady { dependencies: BTreeSet<ClosureId> },
}

/// A header formation request may depend on an exact still-unformed child.
/// All other errors retain their original diagnostic and fail the request.
pub(super) fn form_type_in_view(
    program: &SemanticModule, module: &ActualDirectory<'_>, ledger: &Ledger,
    scope: SemanticInstanceId, recipe: &SemanticTypeRecipe, hint: Option<&Type>,
    limits: &Limits, left: &mut usize,
) -> R<Formation<SemanticTypeValue>> {
    let mut eval = Eval { input: program, module, ledger, limits, left,
        active: BTreeSet::new(), invocation_probe: None, pending_header: None };
    let result = eval.ty(scope, recipe, hint, 0);
    match (result, eval.pending_header) {
        (Ok(value), None) => Ok(Formation::Complete(value)),
        (Err(diagnostic), Some(id)) if is_pending_header(&diagnostic) => {
            eval.fee(1)?;
            Ok(Formation::NotReady { dependencies: BTreeSet::from([id]) })
        }
        (Err(diagnostic), _) => Err(diagnostic),
        (Ok(_), Some(_)) => Err(bad("complete type formation retained a pending actual header")),
    }
}

pub(super) fn type_in_view(
    program: &SemanticModule,
    module: &ActualDirectory<'_>,
    ledger: &Ledger,
    scope: SemanticInstanceId,
    recipe: &SemanticTypeRecipe,
    hint: Option<&Type>,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticTypeValue> {
    Eval {
        input: program,
        module,
        ledger,
        limits,
        left,
        active: BTreeSet::new(),
        invocation_probe: None,
        pending_header: None,
    }
    .ty(scope, recipe, hint, 0)
}

pub(super) fn symbolic_in_view(
    program: &SemanticModule,
    module: &ActualDirectory<'_>,
    ledger: &Ledger,
    scope: SemanticInstanceId,
    recipe: &SemanticTypeRecipe,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticTypeRecipe> {
    if let SemanticTypeRecipe::OriginalType {
        scope: selector,
        source,
    } = recipe
    {
        Eval {
            input: program,
            module,
            ledger,
            limits,
            left,
            active: BTreeSet::new(),
            invocation_probe: None,
        pending_header: None,
        }
        .original_type(scope, *selector, source, 0)
    } else {
        crate::dyn_metadata::semantic_operand_snapshot(recipe, limits, left, 1)?;
        Ok(recipe.clone())
    }
}
pub(super) fn actual_at(
    program: &SemanticModule,
    module: &TypedModule,
    ledger: &Ledger,
    scope: SemanticInstanceId,
    recipe: &SemanticActualRecipe,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticActual> {
    actual_in_view(
        program,
        &ActualDirectory::from_module_at(module, "materialize::actual_at"),
        ledger,
        scope,
        recipe,
        limits,
        left,
    )
}
pub(super) fn actual_in_view(
    program: &SemanticModule,
    module: &ActualDirectory<'_>,
    ledger: &Ledger,
    scope: SemanticInstanceId,
    recipe: &SemanticActualRecipe,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticActual> {
    Eval {
        input: program,
        module,
        ledger,
        limits,
        left,
        active: BTreeSet::new(),
        invocation_probe: None,
        pending_header: None,
    }
    .recipe_actual(scope, recipe, 0)
}

pub(super) fn call_actual_in_view(
    program: &SemanticModule,
    module: &ActualDirectory<'_>,
    ledger: &Ledger,
    scope: SemanticInstanceId,
    call: SemanticSiteId,
    formal: u32,
    recipe: &SemanticActualRecipe,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticActual> {
    Eval {
        input: program,
        module,
        ledger,
        limits,
        left,
        active: BTreeSet::new(),
        invocation_probe: None,
        pending_header: None,
    }
    .call_recipe_actual(scope, call, formal, None, recipe, 0)
}

/// Interpret the solved signature at the exact original call. Open opaque
/// Type actuals taken through a sealed address use that address's original
/// pointee domain in each substituted occurrence of the full mixed formal.
pub(super) fn captured_call_type_in_view(
    program: &SemanticModule,
    module: &ActualDirectory<'_>,
    ledger: &Ledger,
    scope: SemanticInstanceId,
    call: SemanticSiteId,
    recipe: &SemanticTypeRecipe,
    hint: &Type,
    limits: &Limits,
    left: &mut usize,
) -> R<Option<SemanticTypeValue>> {
    Eval {
        input: program,
        module,
        ledger,
        limits,
        left,
        active: BTreeSet::new(),
        invocation_probe: None,
        pending_header: None,
    }
    .captured_call_type(scope, call, recipe, hint, 0)
}

impl Eval<'_, '_, '_> {
    fn captured_call_type(
        &mut self,
        scope: SemanticInstanceId,
        call: SemanticSiteId,
        recipe: &SemanticTypeRecipe,
        hint: &Type,
        depth: usize,
    ) -> R<Option<SemanticTypeValue>> {
        use super::original::substitute::{DelayedType, replace_delayed};
        let eval = self;
        let ledger = eval.ledger;
        let actuals = {
            let proof = ledger.original.borrow();
            let proof = proof
                .as_ref()
                .ok_or_else(|| bad("actual call lacks its original proof"))?;
            if !proof.has_inferred_pointee_environments() {
                return Ok(None);
            }
            eval.node(depth)?;
            let owner = eval.instance(scope)?.definition;
            eval.fee(proof.calls.len().saturating_add(1).ilog2() as usize + 1)?;
            let Some(actuals) = proof.calls.get(&(owner, call)) else {
                return Ok(None);
            };
            eval.fee(actuals.len())?;
            let mut selected = Vec::new();
            for (ordinal, actual) in actuals.iter().enumerate() {
                let SemanticActualRecipe::Type(ty @ SemanticTypeRecipe::Opaque { .. }) = actual else {
                    continue;
                };
                if proof
                    .inferred_pointee_environments(
                        owner, call, ordinal as u32, None, eval.limits, eval.left,
                    )?
                    .is_some()
                {
                    crate::dyn_metadata::semantic_operand_snapshot(ty, eval.limits, eval.left, 1)?;
                    selected.push((ordinal as u32, ty.clone()));
                }
            }
            selected
        };
        let mut equations = Vec::<(DelayedType, SemanticTypeRecipe)>::new();
        for (ordinal, original) in actuals {
            let Some(SemanticTypeValue::Resolved { ty, life, classes }) =
                eval.inferred_capture_pointee(scope, call, ordinal, None, depth + 1)?
            else {
                return Err(bad("solved call loses its original capture pointee Type"));
            };
            let mut images = Vec::new();
            eval.fee(classes.len())?;
            for image in classes {
                let SemanticClassExpr::IntrinsicClass {
                    operand: SemanticOperand::Environment(ordinal),
                    path,
                    class,
                } = image
                else {
                    return Err(bad("sealed call Type has an unrelated class occurrence"));
                };
                images.push(SemanticLifeRecipe::IntrinsicClass {
                    operand: SemanticRecipeOperand::Environment {
                        scope: SemanticScopeRef::Ancestor(0),
                        ordinal,
                    },
                    path,
                    class,
                });
            }
            let exact = SemanticTypeRecipe::Exact(Box::new(SemanticExactTypeRecipe {
                ty,
                life,
                classes: images,
            }));
            for (prior, prior_exact) in &equations {
                crate::dyn_metadata::semantic_operand_snapshot(&prior.ty, eval.limits, eval.left, 1)?;
                crate::dyn_metadata::semantic_operand_snapshot(&original, eval.limits, eval.left, 1)?;
                if prior.ty == original {
                    crate::dyn_metadata::semantic_operand_snapshot(prior_exact, eval.limits, eval.left, 1)?;
                    crate::dyn_metadata::semantic_operand_snapshot(&exact, eval.limits, eval.left, 1)?;
                    if prior_exact != &exact {
                        return Err(unknown(
                            "solved call Type has distinct sealed capture domains",
                        ));
                    }
                }
            }
            equations.push((
                DelayedType { ty: original, bound_depth: 0, invocation_depth: 0 },
                exact,
            ));
        }
        if equations.is_empty() {
            return Ok(None);
        }
        let rewritten = replace_delayed(recipe, &equations, eval.limits, eval.left)?;
        crate::dyn_metadata::semantic_operand_snapshot(recipe, eval.limits, eval.left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(&rewritten, eval.limits, eval.left, 1)?;
        if &rewritten == recipe {
            return Ok(None);
        }
        eval.ty(scope, &rewritten, Some(hint), depth + 1).map(Some)
    }
}

pub(super) fn parent_actuals_in_view(
    program: &SemanticModule,
    module: &ActualDirectory<'_>,
    ledger: &Ledger,
    caller: SemanticInstanceId,
    call: SemanticSiteId,
    holder: SemanticInstanceId,
    limits: &Limits,
    left: &mut usize,
) -> R<Vec<SemanticActual>> {
    Eval {
        input: program,
        module,
        ledger,
        limits,
        left,
        active: BTreeSet::new(),
        invocation_probe: None,
        pending_header: None,
    }
    .parent_actuals(caller, call, holder)
}

pub(super) fn class_at(
    program: &SemanticModule,
    module: &TypedModule,
    ledger: &Ledger,
    scope: SemanticInstanceId,
    recipe: &SemanticLifeRecipe,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticClassExpr> {
    Eval {
        input: program,
        module: &ActualDirectory::from_module_at(module, "materialize::class_at"),
        ledger,
        limits,
        left,
        active: BTreeSet::new(),
        invocation_probe: None,
        pending_header: None,
    }
    .life(scope, recipe)
}

pub(super) fn class_in_view(
    program: &SemanticModule,
    module: &ActualDirectory<'_>,
    ledger: &Ledger,
    scope: SemanticInstanceId,
    recipe: &SemanticLifeRecipe,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticClassExpr> {
    Eval {
        input: program,
        module,
        ledger,
        limits,
        left,
        active: BTreeSet::new(),
        invocation_probe: None,
        pending_header: None,
    }
    .life(scope, recipe)
}
