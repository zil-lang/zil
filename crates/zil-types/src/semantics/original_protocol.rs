//! Conditional protocol relations are proved from original operations. No
//! runtime layout, instance specialization, or proposed Typed table is input.
use super::*;
#[path = "original_protocol_tasks.rs"]
mod tasks;
#[path = "original_protocol_consumption.rs"]
mod consumption;
#[path = "original_protocol_sources.rs"]
mod sources;
#[path = "original_type_cache.rs"]
mod type_cache;
pub(in crate::semantics) use type_cache::hash_into;
pub(in crate::semantics) fn recipe_key(
    value: &SemanticTypeRecipe,
    limits: &Limits,
    left: &mut usize,
) -> R<zil_base::Fingerprint> {
    type_cache::digest(value, limits, left)
}

const OPEN: u8 = 1;
const CLOSING: u8 = 2;
const CLOSED: u8 = 4;
// Private absence of an owning generation, distinct from Closed and from an
// unreachable phase set. Only actual destruction or a proved Give relation
// establishes this state; it is never a usable protocol input.
const RETIRED: u8 = 8;
const UNKNOWN: u8 = OPEN | CLOSING | CLOSED;

fn bit(s: ProtocolState) -> u8 {
    match s {
        ProtocolState::Open => OPEN,
        ProtocolState::Closing => CLOSING,
        ProtocolState::Closed => CLOSED,
    }
}
fn mask(states: &[ProtocolState]) -> u8 {
    states.iter().fold(0, |m, s| m | bit(*s))
}
fn output_mask(states: &[ProtocolState]) -> u8 {
    if states.is_empty() {
        RETIRED
    } else {
        mask(states)
    }
}
fn lookup_work(len: usize) -> usize {
    len.saturating_add(1).ilog2() as usize + 1
}
fn failure(message: &str) -> Diagnostic {
    Diagnostic::error(Phase::Types, "E-SEMANTIC-PROTOCOL", None, message)
}

#[derive(Clone, Copy)]
enum CallSite {
    Unique(SemanticSiteId),
    Repeated,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Root {
    Parameter(SemanticDefinitionId, u32),
    Created(SemanticDefinitionId, NodeId),
    Capture(SemanticDefinitionId, u32),
    Projection(SemanticDefinitionId, u32),
    Result(SemanticDefinitionId, NodeId, u32),
}
#[derive(Clone, PartialEq, Eq)]
struct Outcome {
    exit: ProtocolExit,
    phases: BTreeMap<Root, u8>,
}
// Canonical only within this Engine's immutable original Gamma. The pool
// verifies complete owner+recipe equality before sharing; state equality can
// then compare these stable identities without traversing a static type tree.
#[derive(Clone)]
struct CleanupType(std::rc::Rc<SemanticTypeRecipe>, SemanticDefinitionId, usize);
trait TypeInput {
    fn recipe(&self) -> &SemanticTypeRecipe;
    fn identity(&self) -> Option<(SemanticDefinitionId, usize)> {
        None
    }
}
impl TypeInput for SemanticTypeRecipe {
    fn recipe(&self) -> &SemanticTypeRecipe {
        self
    }
}
impl TypeInput for CleanupType {
    fn recipe(&self) -> &SemanticTypeRecipe {
        self.0.as_ref()
    }
    fn identity(&self) -> Option<(SemanticDefinitionId, usize)> {
        Some((self.1, self.2))
    }
}
impl std::ops::Deref for CleanupType {
    type Target = SemanticTypeRecipe;
    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}
impl std::fmt::Debug for CleanupType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self.0.as_ref(), f)
    }
}
impl PartialEq for CleanupType {
    fn eq(&self, other: &Self) -> bool {
        std::rc::Rc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for CleanupType {}

#[derive(Clone, PartialEq, Eq)]
enum Value {
    Plain,
    Bool(bool),
    Error {
        is_error: bool,
        payload: Box<Value>,
    },
    Boxed(Box<Value>),
    // A generic affine leaf carries its independently checked original type.
    // It is neither an initialized protocol root nor a capability receipt.
    Cleanup {
        owner: SemanticDefinitionId,
        ty: CleanupType,
        // Exact incoming occurrence, carried through moves and local aliases.
        // It is a conditional graph obligation, never a capability of this Type.
        source: Option<Root>,
    },
    Choice(Vec<Value>),
    Variant {
        name: String,
        fields: Vec<Value>,
    },
    Closure {
        definition: SemanticDefinitionId,
        captures: BTreeMap<String, Value>,
    },
    // Root identity, current occurrence validity, actual owning carrier.
    // Address/Desc observations retain identity but never cleanup ownership.
    Root(Root, bool, bool, Option<u8>),
    Valid {
        root: Root,
        fresh: bool,
        negated: bool,
    },
    Poll {
        fresh: bool,
        outcomes: Vec<Outcome>,
    },
    Tuple(Vec<Value>),
    Fields(BTreeMap<String, Value>),
}
impl Value {
    fn work(&self) -> usize {
        match self {
            Self::Error { payload, .. } | Self::Boxed(payload) => payload.work().saturating_add(1),
            Self::Poll { outcomes, .. } => outcomes
                .iter()
                .fold(1, |w, o| w.saturating_add(o.phases.len()).saturating_add(1)),
            Self::Tuple(v) | Self::Choice(v) => v.iter().fold(1, |w, v| w.saturating_add(v.work())),
            Self::Variant { name, fields } => {
                fields.iter().fold(name.len().saturating_add(1), |w, v| {
                    w.saturating_add(v.work())
                })
            }
            Self::Closure { captures, .. } => captures.iter().fold(1, |w, (k, v)| {
                w.saturating_add(k.len()).saturating_add(v.work())
            }),
            Self::Fields(v) => v.iter().fold(1, |w, (k, v)| {
                w.saturating_add(k.len()).saturating_add(v.work())
            }),
            _ => 1,
        }
    }
    fn roots(&self, out: &mut BTreeSet<Root>) {
        match self {
            Self::Error { payload, .. } | Self::Boxed(payload) => payload.roots(out),
            Self::Root(root, true, _, _) => {
                out.insert(*root);
            }
            Self::Tuple(values) | Self::Choice(values) | Self::Variant { fields: values, .. } => {
                for value in values {
                    value.roots(out);
                }
            }
            Self::Closure { captures, .. } => {
                for value in captures.values() {
                    value.roots(out);
                }
            }
            Self::Fields(values) => {
                for value in values.values() {
                    value.roots(out);
                }
            }
            _ => {}
        }
    }
    fn invalidate(&mut self, root: Root, replace: bool, previous: u8) {
        match self {
            Self::Error { payload, .. } | Self::Boxed(payload) => {
                payload.invalidate(root, replace, previous)
            }
            Self::Root(id, live, owning, prior) if *id == root && replace => {
                if *live && *owning {
                    *prior = Some(previous);
                }
                *live = false;
            }
            Self::Poll { fresh, outcomes }
                if outcomes.iter().any(|o| o.phases.contains_key(&root)) =>
            {
                *fresh = false
            }
            Self::Valid {
                root: id, fresh, ..
            } if *id == root => *fresh = false,
            Self::Tuple(values) | Self::Choice(values) | Self::Variant { fields: values, .. } => {
                for value in values {
                    value.invalidate(root, replace, previous);
                }
            }
            Self::Closure { captures, .. } => {
                for value in captures.values_mut() {
                    value.invalidate(root, replace, previous);
                }
            }
            Self::Fields(values) => {
                for value in values.values_mut() {
                    value.invalidate(root, replace, previous);
                }
            }
            _ => {}
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Control {
    Next,
    Return,
    Break,
    Continue,
}
#[derive(Clone, PartialEq, Eq)]
struct State {
    owner: SemanticDefinitionId,
    // Exact original binder origins at this control-flow state. None avoids a
    // second provenance inventory when all complete results are already known.
    task_sources: Option<SourceBindings>,
    tasks: BTreeSet<Root>,
    // Per actual generation, a proved synchronous task graph may depend on
    // still-symbolic incoming owners and callable invocations. Empty requirements
    // are unconditional; missing entries grant nothing. This never establishes Closed.
    task_sync: BTreeMap<Root, BTreeSet<tasks::Requirement>>,
    reachable: bool,
    abrupt: Vec<State>,
    // Stacks preserve hidden lexical binders, including their stale Polls.
    names: BTreeMap<String, Vec<Value>>,
    roots: BTreeMap<Root, u8>,
    control: Control,
    returned: Value,
    temporaries: Vec<Value>,
    // Successful lexical registrations, in construction order. Each entry
    // owns the actual captured operands until a reached exit consumes it.
    deferred: Vec<Value>,
}
fn source_binding_work(bindings: Option<&SourceBindings>) -> usize {
    bindings.map_or(0, |bindings| bindings.iter().fold(1usize, |work, (query, sources)| {
        let paths = sources.as_ref().map_or(1, |sources| sources.iter().fold(1usize, |n, source| {
            n.saturating_add(source.result_path.len())
                .saturating_add(source.input_path.len()).saturating_add(4)
        }));
        work.saturating_add(query.copy_work()).saturating_add(paths)
    }))
}
impl State {
    fn work(&self) -> usize {
        self.names.iter().fold(
            self.abrupt.iter().fold(
                self.temporaries.iter().chain(&self.deferred).fold(
                    self.roots
                        .len()
                        .saturating_add(self.tasks.len())
                        .saturating_add(self.task_sync.values().fold(0usize, |n, r| n.saturating_add(r.len()).saturating_add(1)))
                        .saturating_add(self.returned.work())
                        .saturating_add(source_binding_work(self.task_sources.as_ref()))
                        .saturating_add(1),
                    |w, v| w.saturating_add(v.work()),
                ),
                |w, state| w.saturating_add(state.work()),
            ),
            |w, (name, values)| {
                values
                    .iter()
                    .fold(w.saturating_add(name.len()).saturating_add(1), |w, v| {
                        w.saturating_add(v.work())
                    })
            },
        )
    }
}
const PROTOCOL_COST_KEYS: &[&str] = &[
    "cleanup_value",
    "category",
    "can_drop",
    "definition",
    "charge_value",
    "charge_state",
    "copy",
    "value_copy",
    "root",
    "change",
    "async_head",
    "closure_child",
    "closure_creator",
    "capture_type",
    "query_type",
    "view",
    "choice",
    "typed",
    "query_async",
    "query_ownership",
    "ownership",
    "place_path",
    "replace_place",
    "move_place",
    "observe",
    "drop_value",
    "site",
    "output",
    "nominal_value",
    "call",
    "unknown_call",
    "items",
    "expr",
    "expr_inner",
    "restore",
    "condition",
    "merge",
    "pattern",
    "matches",
    "selected",
    "offer",
    "unselected",
    "loop_body",
    "present",
    "pattern_query",
    "iteration",
    "scope",
    "leave",
    "hide_for_defer",
    "restore_defer",
    "cleanup",
    "block",
    "statement",
    "function",
    "function_inputs",
];

// Only the shape of a fresh abstract Plain value. These rows cannot contain
// an actual Root, protocol phase, constructor occurrence or cleanup receipt.
enum PlainLayout {
    Unchecked,
    Cold,
    Stable,
    Complete(std::rc::Rc<Value>),
}

struct Engine<'a, 'b> {
    program: &'a SemanticModule,
    proof: &'b mut Proof,
    limits: &'b Limits,
    left: &'b mut usize,
    active: BTreeSet<SemanticDefinitionId>,
    relevant: BTreeSet<SemanticDefinitionId>,
    type_ids: type_cache::Table<usize>,
    type_values: Vec<CleanupType>,
    type_categories: Vec<Option<cleanup::Category>>,
    type_cleanup: Vec<Option<bool>>,
    type_fields: Vec<Option<std::rc::Rc<Vec<(String, CleanupType)>>>>,
    type_errors: Vec<Option<(CleanupType, CleanupType)>>,
    type_plain: Vec<PlainLayout>,
    type_shape_nodes: usize,
    query_types: BTreeMap<(SemanticDefinitionId, Query), Option<CleanupType>>,
    method_costs: BTreeMap<&'static str, (usize, usize, usize)>,
    method_cost_total: usize,
    function_cost_total: usize,
    task_exits: BTreeMap<SemanticDefinitionId, Vec<tasks::Exit>>,
    task_exit_count: usize,
    task_source_returns: BTreeSet<(SemanticDefinitionId, NodeId)>,
    synchronous_workers: BTreeMap<SemanticDefinitionId, Option<BTreeSet<tasks::Requirement>>>,
    graph_calls: BTreeMap<(SemanticDefinitionId, SemanticSiteId), Vec<tasks::GraphCall>>,
    graph_call_count: usize,
    local_complete: BTreeSet<SemanticDefinitionId>,
    call_sites: BTreeMap<SemanticDefinitionId, BTreeMap<NodeId, CallSite>>,
    call_site_count: usize,
    graph_requested: BTreeSet<SemanticDefinitionId>,
    graph_complete: BTreeSet<SemanticDefinitionId>,
    projected_sources: Vec<sources::Projection>,
    projected_ids: BTreeMap<sources::Projection, u32>,
    consuming_relevant: BTreeSet<SemanticDefinitionId>,
    consuming_exits: BTreeMap<SemanticDefinitionId, Option<BTreeSet<tasks::Requirement>>>,
    consuming_requirements: BTreeMap<SemanticDefinitionId, BTreeSet<tasks::Requirement>>,
}
impl<'a> Engine<'a, '_> {
    fn record_cost(&mut self, name: &'static str, before: usize, children: usize) {
        let inclusive = before.saturating_sub(*self.left);
        let exclusive = inclusive.saturating_sub(self.method_cost_total.saturating_sub(children));
        self.method_cost_total = self.method_cost_total.saturating_add(exclusive);
        // Every key and counter slot was allocated and paid before traversal.
        let cost = self
            .method_costs
            .get_mut(name)
            .expect("fixed protocol cost key");
        cost.0 = cost.0.saturating_add(1);
        cost.1 = cost.1.saturating_add(inclusive);
        cost.2 = cost.2.saturating_add(exclusive);
    }

    fn type_handle(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<CleanupType> {
        self.pay(1, depth)?;
        let (index, recipe) =
            self.type_ids
                .intern(owner, ty, self.type_values.len(), self.limits, self.left)?;
        self.install_type_handle(owner, index, recipe, depth)
    }
    fn install_type_handle(
        &mut self,
        owner: SemanticDefinitionId,
        index: usize,
        recipe: Option<std::rc::Rc<SemanticTypeRecipe>>,
        depth: usize,
    ) -> R<CleanupType> {
        self.pay(1, depth)?;
        let Some(recipe) = recipe else {
            return self
                .type_values
                .get(index)
                .filter(|value| value.1 == owner)
                .cloned()
                .ok_or_else(|| failure("original cleanup type identity is absent"));
        };
        if index != self.type_values.len() || index >= self.limits.max_nodes {
            return Err(failure("original cleanup type directory index changed"));
        }
        let handle = CleanupType(recipe, owner, index);
        let growth = [
            (self.type_values.len(), self.type_values.capacity()),
            (self.type_categories.len(), self.type_categories.capacity()),
            (self.type_cleanup.len(), self.type_cleanup.capacity()),
            (self.type_fields.len(), self.type_fields.capacity()),
            (self.type_errors.len(), self.type_errors.capacity()),
            (self.type_plain.len(), self.type_plain.capacity()),
        ]
        .into_iter()
        .fold(0usize, |work, (len, capacity)| {
            work.saturating_add(if len == capacity { len } else { 0 })
        });
        self.pay(growth.saturating_add(6), depth)?;
        self.type_values.push(handle.clone());
        self.type_categories.push(None);
        self.type_cleanup.push(None);
        self.type_fields.push(None);
        self.type_errors.push(None);
        self.type_plain.push(PlainLayout::Unchecked);
        Ok(handle)
    }
    fn type_identity<T: TypeInput + ?Sized>(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &T,
        depth: usize,
    ) -> R<Option<usize>> {
        self.pay(1, depth)?;
        let Some((domain, index)) = ty.identity() else {
            return Ok(None);
        };
        let stored = self
            .type_values
            .get(index)
            .ok_or_else(|| failure("original type handle is outside this request"))?;
        if domain != owner || stored.1 != owner || !std::ptr::eq(stored.recipe(), ty.recipe()) {
            return Err(failure(
                "original type handle changes its definition domain",
            ));
        }
        Ok(Some(index))
    }
    fn input_handle<T: TypeInput + ?Sized>(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &T,
        depth: usize,
    ) -> R<CleanupType> {
        if let Some(index) = self.type_identity(owner, ty, depth)? {
            self.pay(1, depth)?;
            Ok(self.type_values[index].clone())
        } else {
            self.type_handle(owner, ty.recipe(), depth + 1)
        }
    }
    fn cleanup_value<T: TypeInput + ?Sized>(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &T,
        depth: usize,
    ) -> R<Value> {
        Ok(Value::Cleanup {
            owner,
            ty: self.input_handle(owner, ty, depth)?,
            source: None,
        })
    }
    fn category<T: TypeInput + ?Sized>(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &T,
        depth: usize,
    ) -> R<cleanup::Category> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result = (|| {
            let ty = self.input_handle(owner, ty, depth)?;
            if let Some(value) = self.type_categories[ty.2] {
                return Ok(value);
            }
            let value = cleanup::category(
                self.program,
                self.proof,
                owner,
                ty.recipe(),
                self.limits,
                self.left,
            )?;
            self.type_categories[ty.2] = Some(value);
            Ok(value)
        })();
        self.record_cost("category", cost_before, cost_children);
        result
    }
    fn can_drop<T: TypeInput + ?Sized>(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &T,
        depth: usize,
    ) -> R<bool> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result = (|| {
            let ty = self.input_handle(owner, ty, depth)?;
            if let Some(value) = self.type_cleanup[ty.2] {
                return Ok(value);
            }
            let value = cleanup::capability(
                self.program,
                self.proof,
                owner,
                ty.recipe(),
                self.limits,
                self.left,
            )?;
            self.type_cleanup[ty.2] = Some(value);
            Ok(value)
        })();
        self.record_cost("can_drop", cost_before, cost_children);
        result
    }
    fn product_fields(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &CleanupType,
        depth: usize,
    ) -> R<Option<std::rc::Rc<Vec<(String, CleanupType)>>>> {
        self.type_identity(owner, ty, depth)?;
        self.pay(1, depth)?;
        if let Some(fields) = &self.type_fields[ty.2] {
            return Ok(Some(fields.clone()));
        }
        if self.type_errors[ty.2].is_some() {
            return Ok(None);
        }
        let Some(fields) = cleanup::fields(
            self.program,
            self.proof,
            owner,
            ty.recipe(),
            self.limits,
            self.left,
        )?
        else {
            // No unknown/non-product result is retained as a positive fact.
            return Ok(None);
        };
        let nodes = self
            .type_shape_nodes
            .saturating_add(fields.len())
            .saturating_add(1);
        if nodes > self.limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "original product shape cache node limit exceeded",
                None,
            ));
        }
        self.pay(fields.len().saturating_add(2), depth)?;
        let mut result = Vec::with_capacity(fields.len());
        for (name, recipe) in fields {
            let child = self.type_handle(owner, &recipe, depth + 1)?;
            result.push((name, child));
        }
        let result = std::rc::Rc::new(result);
        self.type_fields[ty.2] = Some(result.clone());
        self.type_shape_nodes = nodes;
        Ok(Some(result))
    }
    fn error_arms(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &CleanupType,
        depth: usize,
    ) -> R<Option<(CleanupType, CleanupType)>> {
        self.type_identity(owner, ty, depth)?;
        self.pay(2, depth)?;
        if let Some(arms) = &self.type_errors[ty.2] {
            return Ok(Some(arms.clone()));
        }
        // A successfully selected product is disjoint from ErrorUnion. This
        // uses the exact complete constructor, never an unknown/failed query.
        if self.type_fields[ty.2].is_some() {
            return Ok(None);
        }
        let Some((error, result)) = cleanup::error_arms(
            self.program,
            self.proof,
            owner,
            ty.recipe(),
            self.limits,
            self.left,
        )?
        else {
            return Ok(None);
        };
        let nodes = self.type_shape_nodes.saturating_add(3);
        if nodes > self.limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "original error shape cache node limit exceeded",
                None,
            ));
        }
        let arms = (
            self.type_handle(owner, &error, depth + 1)?,
            self.type_handle(owner, &result, depth + 1)?,
        );
        self.type_errors[ty.2] = Some(arms.clone());
        self.type_shape_nodes = nodes;
        Ok(Some(arms))
    }

    fn pay(&mut self, n: usize, depth: usize) -> R<()> {
        *self.left = self.left.checked_sub(n).ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "original protocol work exceeded", None)
        })?;
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "original protocol depth exceeded",
                None,
            ));
        }
        Ok(())
    }
    fn definition(&mut self, id: SemanticDefinitionId) -> R<&'a SemanticDefinition> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<&'a SemanticDefinition> = (|| {
            self.pay(
                self.proof
                    .definition_positions
                    .len()
                    .saturating_add(1)
                    .ilog2() as usize
                    + 1,
                0,
            )?;
            let position = self
                .proof
                .definition_positions
                .get(&id)
                .copied()
                .ok_or_else(|| failure("protocol target has no original definition index"))?;
            self.program
                .definitions
                .get(position)
                .filter(|definition| definition.id == id)
                .ok_or_else(|| failure("protocol original definition index changes identity"))
        })();
        self.record_cost("definition", cost_before, cost_children);
        result
    }
    fn charge_value(&mut self, value: &Value, depth: usize) -> R<()> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<()> = (|| {
            self.pay(1, depth)?;
            match value {
                Value::Error { payload, .. } | Value::Boxed(payload) => {
                    self.charge_value(payload, depth + 1)?
                }
                // Copying/comparing a canonical type handle is constant work.
                // All dynamic Value/State fields are still visited below.
                Value::Cleanup { .. } => {}
                Value::Tuple(values)
                | Value::Choice(values)
                | Value::Variant { fields: values, .. } => {
                    for value in values {
                        self.charge_value(value, depth + 1)?;
                    }
                }
                Value::Fields(values)
                | Value::Closure {
                    captures: values, ..
                } => {
                    for value in values.values() {
                        self.charge_value(value, depth + 1)?;
                    }
                }
                _ => {}
            }
            Ok(())
        })();
        self.record_cost("charge_value", cost_before, cost_children);
        result
    }
    fn charge_state(&mut self, state: &State, depth: usize) -> R<()> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<()> = (|| {
            self.pay(state.abrupt.len(), depth)?;
            for exit in &state.abrupt {
                self.charge_state(exit, depth + 1)?;
            }
            for value in state
                .names
                .values()
                .flatten()
                .chain(&state.temporaries)
                .chain(&state.deferred)
                .chain(std::iter::once(&state.returned))
            {
                self.charge_value(value, depth + 1)?;
            }
            Ok(())
        })();
        self.record_cost("charge_state", cost_before, cost_children);
        result
    }
    fn copy(&mut self, state: &State, depth: usize) -> R<State> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<State> = (|| {
            self.pay(state.work(), depth)?;
            self.charge_state(state, depth + 1)?;
            Ok(state.clone())
        })();
        self.record_cost("copy", cost_before, cost_children);
        result
    }
    fn value_copy(&mut self, value: &Value, depth: usize) -> R<Value> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<Value> = (|| {
            self.pay(value.work(), depth)?;
            self.charge_value(value, depth + 1)?;
            Ok(value.clone())
        })();
        self.record_cost("value_copy", cost_before, cost_children);
        result
    }
    fn root(&mut self, value: &Value, state: &State, depth: usize) -> R<Root> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<Root> = (|| {
            self.pay(value.work().saturating_add(state.roots.len()), depth)?;
            match value {
                Value::Root(root, true, _, _)
                    if state
                        .roots
                        .get(root)
                        .is_some_and(|phase| *phase != 0 && *phase & RETIRED == 0) =>
                {
                    Ok(*root)
                }
                _ => Err(failure(
                    "protocol operation lacks its live original parameter or owning occurrence",
                )),
            }
        })();
        self.record_cost("root", cost_before, cost_children);
        result
    }
    fn change(
        &mut self,
        state: &mut State,
        root: Root,
        phase: u8,
        replace: bool,
        depth: usize,
    ) -> R<()> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<()> = (|| {
            self.pay(state.work(), depth)?;
            let previous = state.roots.get(&root).copied().unwrap_or(UNKNOWN);
            // An unknown observer cannot restore a generation which no longer
            // exists. Only an independently checked mint/replacement can do so.
            if previous == RETIRED && !replace {
                return Ok(());
            }
            for values in state.names.values_mut() {
                for value in values {
                    value.invalidate(root, replace, previous);
                }
            }
            state.returned.invalidate(root, replace, previous);
            for value in state.temporaries.iter_mut().chain(&mut state.deferred) {
                value.invalidate(root, replace, previous);
            }
            if replace {
                // A new occupant cannot inherit its predecessor's graph.
                state.task_sync.remove(&root);
            }
            state.roots.insert(root, phase);
            Ok(())
        })();
        self.record_cost("change", cost_before, cost_children);
        result
    }
    fn async_head(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<bool> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<bool> = (|| {
            self.pay(1, depth)?;
            match ty {
                SemanticTypeRecipe::AsyncRegistration(_) => Ok(true),
                SemanticTypeRecipe::Address { inner, .. }
                | SemanticTypeRecipe::Desc { inner, .. }
                | SemanticTypeRecipe::Managed(inner) => self.async_head(owner, inner, depth + 1),
                SemanticTypeRecipe::Qualified { storage, .. } => {
                    self.async_head(owner, storage, depth + 1)
                }
                SemanticTypeRecipe::OriginalType {
                    scope: SemanticScopeRef::Ancestor(0),
                    source,
                } => {
                    crate::dyn_metadata::semantic_operand_snapshot(
                        source,
                        self.limits,
                        self.left,
                        depth + 1,
                    )?;
                    let Some(ty) =
                        self.query_type(owner, Query::from(source.clone()), depth + 1)?
                    else {
                        return Ok(false);
                    };
                    self.async_head(owner, &ty, depth + 1)
                }
                // Opaque bodies remain in the defining lexical domain.
                SemanticTypeRecipe::Opaque {
                    definition, hidden, ..
                } => self.async_head(*definition, hidden, depth + 1),
                _ => Ok(false),
            }
        })();
        self.record_cost("async_head", cost_before, cost_children);
        result
    }
    fn closure_child(
        &mut self,
        owner: SemanticDefinitionId,
        node: NodeId,
        depth: usize,
    ) -> R<&'a SemanticDefinition> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<&'a SemanticDefinition> = (|| {
            let original = self.definition(owner)?;
            let SemanticDefinitionBody::FunctionWithBody {
                sites, site_nodes, ..
            } = &original.body
            else {
                return Err(failure("original closure creator has no body directory"));
            };
            self.pay(1, depth)?;
            let directory = match self.proof.original_sites.owner(owner, self.left)? {
                Some(directory) => directory,
                None => std::rc::Rc::new(super::sites::DefinitionSites::build(
                    owner, sites, site_nodes, self.left,
                )?),
            };
            let mut selected = None;
            for id in directory.at(SemanticOriginalNode::Expression(node), self.left)? {
                if let Some(site) = directory.recipe(owner, sites, *id, self.left)?
                    && let SemanticSiteRecipeKind::Closure { definition, .. } = site.kind
                    && selected.replace(definition).is_some()
                {
                    return Err(failure("original closure creator selects multiple children"));
                }
            }
            let child =
                self.definition(selected.ok_or_else(|| {
                    failure("original closure lacks its exact creator occurrence")
                })?)?;
            if child.parent != Some(owner) {
                return Err(failure(
                    "original closure creator changes its lexical parent",
                ));
            }
            Ok(child)
        })();
        self.record_cost("closure_child", cost_before, cost_children);
        result
    }
    fn closure_creator(
        &mut self,
        child: &SemanticDefinition,
        depth: usize,
    ) -> R<(SemanticDefinitionId, NodeId)> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<(SemanticDefinitionId, NodeId)> = (|| {
            let parent = child
                .parent
                .ok_or_else(|| failure("original capture has no creator definition"))?;
            let original = self.definition(parent)?;
            let SemanticDefinitionBody::FunctionWithBody {
                sites, site_nodes, ..
            } = &original.body
            else {
                return Err(failure("original capture creator has no body directory"));
            };
            self.pay(sites.len(), depth)?;
            let directory = match self.proof.original_sites.owner(parent, self.left)? {
                Some(directory) => directory,
                None => std::rc::Rc::new(super::sites::DefinitionSites::build(
                    parent, sites, site_nodes, self.left,
                )?),
            };
            let mut selected = None;
            for site in sites {
                if !matches!(site.kind,SemanticSiteRecipeKind::Closure{definition,..} if definition==child.id)
                {
                    continue;
                }
                match directory.node(site.id, self.left)? {
                    None => {}
                    Some(Some(SemanticOriginalNode::Expression(node))) if selected.is_none() => {
                        selected = Some(node);
                    }
                    Some(Some(SemanticOriginalNode::Statement(_))) => {
                        return Err(failure("original closure creator is not an expression"));
                    }
                    Some(_) => {
                        return Err(failure(
                            "original closure has ambiguous creator occurrences",
                        ));
                    }
                }
            }
            Ok((
                parent,
                selected.ok_or_else(|| {
                    failure("original closure has no complete creator occurrence")
                })?,
            ))
        })();
        self.record_cost("closure_creator", cost_before, cost_children);
        result
    }
    fn inferred_capture_move(&mut self, definition: SemanticDefinitionId, name: &str, depth: usize) -> R<bool> {
        if self.proof.inferred_capture_moves.is_empty() { return Ok(false); }
        self.pay(lookup_work(self.proof.inferred_capture_moves.len()), depth)?;
        let Some(count) = self.proof.inferred_capture_moves.get(&definition).map(BTreeSet::len) else { return Ok(false); };
        self.pay(lookup_work(count).saturating_mul(name.len().saturating_add(1)), depth)?;
        Ok(self.proof.inferred_capture_moves[&definition].contains(name))
    }
    fn capture_type(
        &mut self,
        parent: SemanticDefinitionId,
        node: NodeId,
        name: &str,
        child: &SemanticDefinition,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<SemanticTypeRecipe> = (|| {
            self.pay(name.len(), depth)?;
            if let Some(ty) = self.query_type(
                parent,
                Query::NameAt {
                    expression: true,
                    node,
                    name: name.to_owned(),
                },
                depth + 1,
            )? {
                return copy_recipe(&ty, self.limits, self.left);
            }
            self.pay(child.environment.len(), depth)?;
            let mut selected = None;
            for (slot, formal) in child.environment.iter().enumerate() {
                if matches!(&formal.source,SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType{source:SemanticOriginalTypeSource::NameAt{at:SemanticOriginalNode::Expression(at),name:source},scope:SemanticScopeRef::Ancestor(0)}) if *at==node&&source==name)
                {
                    if selected.replace(slot as u32).is_some() {
                        return Err(failure(
                            "original physical capture has ambiguous Type sources",
                        ));
                    }
                }
            }
            let slot = selected.ok_or_else(|| {
                pending("original cleanup capture lacks its exact original whole-Type source")
            })?;
            self.pay(lookup_work(self.proof.environments.len()), depth)?;
            let (owner, ty) = self
                .proof
                .environments
                .get(&(child.id, slot))
                .ok_or_else(|| pending("original cleanup capture Type source is not sealed"))?;
            if *owner != parent {
                return Err(failure(
                    "original capture Type source belongs to another lexical creator",
                ));
            }
            copy_recipe(ty, self.limits, self.left)
        })();
        self.record_cost("capture_type", cost_before, cost_children);
        result
    }
    fn query_type(
        &mut self,
        owner: SemanticDefinitionId,
        query: Query,
        depth: usize,
    ) -> R<Option<CleanupType>> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result = (|| {
            self.pay(
                query
                    .copy_work()
                    .saturating_add(self.query_types.len().saturating_add(1).ilog2() as usize + 1),
                depth,
            )?;
            let key = (owner, query);
            if let Some(value) = self.query_types.get(&key) {
                return Ok(value.clone());
            }
            if self.query_types.len() >= self.limits.max_nodes {
                return Err(Diagnostic::limit(
                    Phase::Types,
                    "original type query directory limit exceeded",
                    None,
                ));
            }
            self.pay(
                self.proof.types.len().saturating_add(1).ilog2() as usize + 1,
                depth,
            )?;
            // The original body proof is immutable for this query. Borrow its
            // recipe directly into the exact pool: no intermediate full clone
            // is needed merely to cross the Engine method boundary.
            let interned = self
                .proof
                .types
                .get(&key)
                .map(|recipe| {
                    self.type_ids.intern(
                        owner,
                        recipe,
                        self.type_values.len(),
                        self.limits,
                        self.left,
                    )
                })
                .transpose()?;
            let value = interned
                .map(|(index, recipe)| self.install_type_handle(owner, index, recipe, depth + 1))
                .transpose()?;
            self.pay(key.1.copy_work().saturating_add(2), depth)?;
            self.query_types.insert(key, value.clone());
            Ok(value)
        })();
        self.record_cost("query_type", cost_before, cost_children);
        result
    }

    fn view(&mut self, value: Value, depth: usize) -> R<Value> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<Value> = (|| {
            self.pay(value.work(), depth)?;
            Ok(match value {
                Value::Error { is_error, payload } => Value::Error {
                    is_error,
                    payload: Box::new(self.view(*payload, depth + 1)?),
                },
                Value::Boxed(payload) => self.view(*payload, depth + 1)?,
                Value::Cleanup { .. } => Value::Plain,
                Value::Root(root, live, _, prior) => Value::Root(root, live, false, prior),
                Value::Tuple(values) => Value::Tuple(
                    values
                        .into_iter()
                        .map(|value| self.view(value, depth + 1))
                        .collect::<R<_>>()?,
                ),
                Value::Choice(values) => Value::Choice(
                    values
                        .into_iter()
                        .map(|value| self.view(value, depth + 1))
                        .collect::<R<_>>()?,
                ),
                Value::Variant { name, fields } => Value::Variant {
                    name,
                    fields: fields
                        .into_iter()
                        .map(|value| self.view(value, depth + 1))
                        .collect::<R<_>>()?,
                },
                Value::Fields(values) => Value::Fields(
                    values
                        .into_iter()
                        .map(|(name, value)| Ok((name, self.view(value, depth + 1)?)))
                        .collect::<R<_>>()?,
                ),
                Value::Closure {
                    definition,
                    captures,
                } => Value::Closure {
                    definition,
                    captures: captures
                        .into_iter()
                        .map(|(name, value)| Ok((name, self.view(value, depth + 1)?)))
                        .collect::<R<_>>()?,
                },
                other => other,
            })
        })();
        self.record_cost("view", cost_before, cost_children);
        result
    }
    fn choice(&mut self, values: Vec<Value>, depth: usize) -> R<Value> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<Value> = (|| {
            self.pay(values.len().saturating_add(1), depth)?;
            let mut pending = values;
            let mut out = Vec::new();
            while let Some(value) = pending.pop() {
                self.pay(value.work().saturating_add(out.len()), depth)?;
                if let Value::Choice(values) = value {
                    self.pay(values.len(), depth)?;
                    pending.extend(values);
                    continue;
                }
                self.charge_value(&value, depth + 1)?;
                let mut duplicate = false;
                for previous in &out {
                    self.charge_value(previous, depth + 1)?;
                    if previous == &value {
                        duplicate = true;
                        break;
                    }
                }
                if !duplicate {
                    out.push(value);
                }
            }
            out.reverse();
            Ok(if out.len() == 1 {
                out.pop().unwrap()
            } else {
                Value::Choice(out)
            })
        })();
        self.record_cost("choice", cost_before, cost_children);
        result
    }
    fn plain_value(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &CleanupType,
        depth: usize,
    ) -> R<Value> {
        self.type_identity(owner, ty, depth)?;
        self.pay(1, depth)?;
        if let PlainLayout::Complete(value) = &self.type_plain[ty.2] {
            let value = value.clone();
            // A fresh value is allocated on every use, with the same complete
            // shape/depth charge as all other Value copies. No dynamic state
            // or ownership occurrence is shared with the saved shape.
            return self.value_copy(&value, depth + 1);
        }
        let mut stable = self.plain_shape_stable(ty, depth + 1)?;
        let value = if let Some((error, result)) = self.error_arms(owner, ty, depth + 1)? {
            let error_type = error;
            let error = self.typed(owner, &error_type, Value::Plain, depth + 1)?;
            let result_value = self.typed(owner, &result, Value::Plain, depth + 1)?;
            if stable {
                stable = self.plain_shape_stable(&error_type, depth + 1)?
                    && self.plain_shape_stable(&result, depth + 1)?;
            }
            self.choice(
                vec![
                    Value::Error {
                        is_error: true,
                        payload: Box::new(error),
                    },
                    Value::Error {
                        is_error: false,
                        payload: Box::new(result_value),
                    },
                ],
                depth + 1,
            )?
        } else if let Some(fields) = self.product_fields(owner, ty, depth + 1)? {
            self.pay(fields.len(), depth)?;
            let mut values = BTreeMap::new();
            for (name, field) in fields.iter() {
                self.pay(
                    name.len()
                        .saturating_add(lookup_work(values.len()))
                        .saturating_add(1),
                    depth,
                )?;
                let value = self.typed(owner, field, Value::Plain, depth + 1)?;
                if stable {
                    stable = self.plain_shape_stable(field, depth + 1)?;
                }
                values.insert(name.clone(), value);
            }
            Value::Fields(values)
        } else {
            self.cleanup_value(owner, ty, depth + 1)?
        };
        if !stable {
            self.type_plain[ty.2] = PlainLayout::Cold;
        }
        if stable {
            let nodes = self
                .type_shape_nodes
                .saturating_add(value.work())
                .saturating_add(1);
            if nodes > self.limits.max_nodes {
                return Err(Diagnostic::limit(
                    Phase::Types,
                    "original Plain shape node limit exceeded",
                    None,
                ));
            }
            let saved = self.value_copy(&value, depth + 1)?;
            self.pay(2, depth)?;
            self.type_plain[ty.2] = PlainLayout::Complete(std::rc::Rc::new(saved));
            self.type_shape_nodes = nodes;
        }
        Ok(value)
    }
    fn plain_shape_stable(&mut self, ty: &CleanupType, depth: usize) -> R<bool> {
        self.pay(1, depth)?;
        if matches!(self.type_plain[ty.2], PlainLayout::Unchecked) {
            self.type_plain[ty.2] = if self.stable_plain_type(ty.recipe(), depth + 1)? {
                PlainLayout::Stable
            } else {
                PlainLayout::Cold
            };
        }
        Ok(!matches!(self.type_plain[ty.2], PlainLayout::Cold))
    }
    // A deliberately conservative optimization domain. Rigid declared formals
    // belong to owner Gamma; unresolved queries, inference, opaque/environment
    // views and associated selections continue through the full cold path.
    fn stable_plain_type(&mut self, ty: &SemanticTypeRecipe, depth: usize) -> R<bool> {
        self.pay(1, depth)?;
        Ok(match ty {
            SemanticTypeRecipe::Atom(_) => true,
            SemanticTypeRecipe::Formal(SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(hops),
                ..
            }) => *hops != u32::MAX,
            SemanticTypeRecipe::Nominal { arguments, .. } => {
                let mut stable = true;
                for argument in arguments {
                    self.pay(1, depth)?;
                    stable &= match argument {
                        SemanticActualRecipe::Type(ty) => self.stable_plain_type(ty, depth + 1)?,
                        SemanticActualRecipe::Life(SemanticLifeRecipe::Formal(reference)) => {
                            reference.scope != SemanticScopeRef::Ancestor(u32::MAX)
                        }
                        SemanticActualRecipe::Const { carrier, value } => {
                            self.stable_plain_type(carrier, depth + 1)?
                                && self.stable_plain_const(value, depth + 1)?
                        }
                        _ => false,
                    };
                }
                stable
            }
            SemanticTypeRecipe::Managed(inner)
            | SemanticTypeRecipe::OwnedStore(inner)
            | SemanticTypeRecipe::Mutex(inner)
            | SemanticTypeRecipe::Registration(inner)
            | SemanticTypeRecipe::AsyncRegistration(inner)
            | SemanticTypeRecipe::Slice(inner) => self.stable_plain_type(inner, depth + 1)?,
            SemanticTypeRecipe::Tuple(fields) | SemanticTypeRecipe::AnonymousSum(fields) => {
                let mut stable = true;
                for field in fields {
                    stable &= self.stable_plain_type(field, depth + 1)?;
                }
                stable
            }
            SemanticTypeRecipe::ErrorUnion { error, result } => {
                self.stable_plain_type(error, depth + 1)?
                    && self.stable_plain_type(result, depth + 1)?
            }
            SemanticTypeRecipe::Array { element, length } => {
                self.stable_plain_type(element, depth + 1)?
                    && self.stable_plain_const(length, depth + 1)?
            }
            _ => false,
        })
    }
    fn stable_plain_const(&mut self, value: &SemanticConstRecipe, depth: usize) -> R<bool> {
        self.pay(1, depth)?;
        Ok(match value {
            SemanticConstRecipe::Value(_) => true,
            SemanticConstRecipe::Formal(reference) => {
                reference.scope != SemanticScopeRef::Ancestor(u32::MAX)
            }
            _ => false,
        })
    }

    fn typed<T: TypeInput + ?Sized>(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &T,
        value: Value,
        depth: usize,
    ) -> R<Value> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<Value> = (|| {
            self.pay(value.work(), depth)?;
            let ty = self.input_handle(owner, ty, depth + 1)?;
            match self.category(owner, &ty, depth + 1)? {
                cleanup::Category::Copy | cleanup::Category::View => {
                    return self.view(value, depth + 1);
                }
                cleanup::Category::Own => {}
            }
            match value {
                Value::Plain => self.plain_value(owner, &ty, depth + 1),
                Value::Choice(values) => {
                    let values = values
                        .into_iter()
                        .map(|value| self.typed(owner, &ty, value, depth + 1))
                        .collect::<R<Vec<_>>>()?;
                    self.choice(values, depth + 1)
                }
                Value::Fields(values) => {
                    if self.product_fields(owner, &ty, depth + 1)?.is_some() {
                        Ok(Value::Fields(values))
                    } else {
                        self.cleanup_value(owner, &ty, depth + 1)
                    }
                }
                other => Ok(other),
            }
        })();
        self.record_cost("typed", cost_before, cost_children);
        result
    }
    fn query_async(&mut self, owner: SemanticDefinitionId, query: Query, depth: usize) -> R<bool> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result = (|| {
            let Some(ty) = self.query_type(owner, query, depth + 1)? else {
                return Ok(false);
            };
            self.async_head(owner, &ty, depth + 1)
        })();
        self.record_cost("query_async", cost_before, cost_children);
        result
    }

    fn query_ownership(
        &mut self,
        owner: SemanticDefinitionId,
        query: Query,
        depth: usize,
    ) -> R<Option<bool>> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result = (|| {
            let Some(ty) = self.query_type(owner, query, depth + 1)? else {
                return Ok(None);
            };
            self.ownership(owner, &ty, depth + 1)
        })();
        self.record_cost("query_ownership", cost_before, cost_children);
        result
    }

    fn ownership(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<Option<bool>> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<Option<bool>> = (|| {
            self.pay(1, depth)?;
            match ty {
                SemanticTypeRecipe::Managed(_) => Ok(Some(true)),
                SemanticTypeRecipe::Address { .. } | SemanticTypeRecipe::Desc { .. } => {
                    Ok(Some(false))
                }
                SemanticTypeRecipe::Qualified { storage, .. } => {
                    self.ownership(owner, storage, depth + 1)
                }
                SemanticTypeRecipe::Opaque {
                    definition, hidden, ..
                } => self.ownership(*definition, hidden, depth + 1),
                SemanticTypeRecipe::OriginalType {
                    scope: SemanticScopeRef::Ancestor(0),
                    source,
                } => {
                    crate::dyn_metadata::semantic_operand_snapshot(
                        source,
                        self.limits,
                        self.left,
                        depth + 1,
                    )?;
                    self.query_ownership(owner, Query::from(source.clone()), depth + 1)
                }
                _ => Ok(None),
            }
        })();
        self.record_cost("ownership", cost_before, cost_children);
        result
    }
    fn place_path(
        &mut self,
        expression: &SemanticOriginalExpr,
        depth: usize,
    ) -> R<Option<(String, Vec<String>)>> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<Option<(String, Vec<String>)>> = (|| {
            self.pay(1, depth)?;
            match &expression.kind {
                CoreExprKind::Name(name) => {
                    self.pay(name.len().saturating_add(1), depth)?;
                    Ok(Some((name.clone(), Vec::new())))
                }
                CoreExprKind::Group(value) | CoreExprKind::Ascribed { value, .. } => {
                    self.place_path(value, depth + 1)
                }
                CoreExprKind::Member { object, member, .. } => {
                    let Some((name, mut path)) = self.place_path(object, depth + 1)? else {
                        return Ok(None);
                    };
                    self.pay(
                        member.len().saturating_add(path.len()).saturating_add(1),
                        depth,
                    )?;
                    path.push(member.clone());
                    Ok(Some((name, path)))
                }
                _ => Ok(None),
            }
        })();
        self.record_cost("place_path", cost_before, cost_children);
        result
    }
    fn replace_place(
        &mut self,
        expression: &SemanticOriginalExpr,
        state: &mut State,
        value: Value,
        depth: usize,
    ) -> R<Option<Value>> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<Option<Value>> = (|| {
            let Some((name, path)) = self.place_path(expression, depth + 1)? else {
                return Ok(None);
            };
            self.pay(
                name.len()
                    .saturating_add(state.names.len())
                    .saturating_add(path.len()),
                depth,
            )?;
            let Some(target) = state.names.get_mut(&name).and_then(|stack| stack.last_mut()) else {
                return Ok(None);
            };
            self.replace_path(target, &path, value, depth)
        })();
        self.record_cost("replace_place", cost_before, cost_children);
        result
    }
    // A join keeps alternatives as complete occupants. For a projected move
    // or replacement, update the same declared path in every alternative and
    // retain the associated siblings. An incomplete path publishes no change.
    fn replace_path(
        &mut self,
        mut target: &mut Value,
        path: &[String],
        value: Value,
        depth: usize,
    ) -> R<Option<Value>> {
        for (step, field) in path.iter().enumerate() {
            match target {
                Value::Choice(alternatives) => {
                    self.pay(alternatives.len().saturating_mul(2).saturating_add(1), depth)?;
                    if alternatives.is_empty() {
                        return Err(failure("original cleanup projection has no reachable occupant"));
                    }
                    let mut survivors = Vec::with_capacity(alternatives.len());
                    let mut removed = Vec::with_capacity(alternatives.len());
                    for alternative in alternatives.iter() {
                        let mut survivor = self.value_copy(alternative, depth + 1)?;
                        let inserted = self.value_copy(&value, depth + 1)?;
                        let Some(old) = self.replace_path(&mut survivor, &path[step..], inserted, depth + 1)? else {
                            return Err(pending("original cleanup projection requires the selected path in every reachable occupant"));
                        };
                        survivors.push(survivor);
                        removed.push(old);
                    }
                    let previous = self.choice(removed, depth + 1)?;
                    *target = self.choice(survivors, depth + 1)?;
                    return Ok(Some(previous));
                }
                Value::Fields(fields) => {
                    self.pay(lookup_work(fields.len()).saturating_mul(field.len().saturating_add(1)), depth)?;
                    target = fields.get_mut(field).ok_or_else(|| failure(
                        "original cleanup place selects an absent complete product field"
                    ))?;
                }
                Value::Tuple(values) => {
                    self.pay(field.len().saturating_add(1), depth)?;
                    let index = field.parse::<usize>().map_err(|_| failure(
                        "original cleanup tuple place has a nonordinal member"
                    ))?;
                    target = values.get_mut(index).ok_or_else(|| failure(
                        "original cleanup tuple place is out of range"
                    ))?;
                }
                _ => return Ok(None),
            }
        }
        self.pay(target.work().saturating_add(value.work()), depth)?;
        Ok(Some(std::mem::replace(target, value)))
    }
    fn move_place(
        &mut self,
        expression: &SemanticOriginalExpr,
        state: &mut State,
        depth: usize,
    ) -> R<()> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<()> = (|| {
            if self
                .replace_place(expression, state, Value::Plain, depth)?
                .is_none()
            {
                return Err(pending(
                    "original cleanup transfer requires its exact owned product/place boundary",
                ));
            }
            Ok(())
        })();
        self.record_cost("move_place", cost_before, cost_children);
        result
    }
    fn observe(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        state: &mut State,
        depth: usize,
    ) -> R<Value> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<Value> = (|| {
            self.pay(1, depth)?;
            match &expression.kind {
                CoreExprKind::Name(name) => {
                    self.pay(
                        name.len().saturating_add(lookup_work(state.names.len())),
                        depth,
                    )?;
                    if let Some(value) = state.names.get(name).and_then(|stack| stack.last()) {
                        self.value_copy(value, depth + 1)
                    } else {
                        Ok(Value::Plain)
                    }
                }
                CoreExprKind::Group(value) | CoreExprKind::Ascribed { value, .. } => {
                    self.observe(owner, value, state, depth + 1)
                }
                CoreExprKind::Member { object, member, .. } => {
                    let value = self.observe(owner, object, state, depth + 1)?;
                    match value {
                        Value::Choice(alternatives) => {
                            // Only complete products authorize distributing this read.
                            // Keep each selected child's actual root and generation.
                            self.pay(alternatives.len().saturating_add(1), depth)?;
                            let mut projected = Vec::with_capacity(alternatives.len());
                            for alternative in alternatives {
                                self.pay(alternative.work().saturating_add(member.len()), depth)?;
                                let child = Self::product_member(alternative, member)
                                    .ok_or_else(|| pending("original cleanup observation requires its complete projected owner relation"))?;
                                projected.push(child);
                            }
                            self.choice(projected, depth + 1)
                        }
                        value @ (Value::Fields(_) | Value::Tuple(_)) =>
                            Ok(Self::product_member(value, member).unwrap_or(Value::Plain)),
                        Value::Cleanup { .. } | Value::Root(..) | Value::Plain => Ok(Value::Plain),
                        _ => Err(pending(
                            "original cleanup observation requires its complete projected owner relation",
                        )),
                    }
                }
                _ => self.expr(owner, expression, state, depth + 1),
            }
        })();
        self.record_cost("observe", cost_before, cost_children);
        result
    }
    fn product_member(value: Value, member: &str) -> Option<Value> {
        match value {
            Value::Fields(mut fields) => fields.remove(member),
            Value::Tuple(mut fields) => member.parse::<usize>().ok()
                .filter(|index| *index < fields.len()).map(|index| fields.remove(index)),
            _ => None,
        }
    }
    fn drop_value(&mut self, value: &Value, state: &State, depth: usize) -> R<()> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<()> = (|| {
            self.pay(value.work(), depth)?;
            match value {
                Value::Error { payload, .. } | Value::Boxed(payload) => {
                    self.drop_value(payload, state, depth + 1)?
                }
                Value::Cleanup { owner, ty, .. } => {
                    if !self.can_drop(*owner, ty, depth + 1)?
                        && !(self.suspend_root(state.owner, depth + 1)?
                            && cleanup::resumable_callable(self.program, self.proof, *owner, ty,
                                self.limits, self.left)?) {
                        self.pay(128, depth)?;
                        let mut error = error(
                            "E-ORIGINAL-CLEANUP",
                            Span::default(),
                            "live original value has no independent SyncDrop proof",
                        );
                        crate::dyn_metadata::semantic_operand_snapshot(
                            &**ty,
                            self.limits,
                            self.left,
                            1,
                        )?;
                        error.notes.push(format!(
                            "original cleanup owner {:?}, complete type recipe {:?}",
                            owner, ty
                        ));
                        return Err(error);
                    }
                }
                Value::Choice(values) | Value::Variant { fields: values, .. } => {
                    for value in values {
                        self.drop_value(value, state, depth + 1)?;
                    }
                }
                Value::Closure { captures, .. } => {
                    for value in captures.values() {
                        self.drop_value(value, state, depth + 1)?;
                    }
                }
                Value::Root(root, true, true, _) if state.tasks.contains(root) => {
                    self.task_drop(*root, state, None, depth + 1)?
                }
                Value::Root(root, false, true, prior) if state.tasks.contains(root) => {
                    let previous = prior.ok_or_else(|| {
                        failure("original Task cleanup loses an unsealed earlier generation")
                    })?;
                    self.task_drop(*root, state, Some(previous), depth + 1)?
                }
                Value::Root(root, true, true, _) => {
                    self.pay(lookup_work(state.roots.len()), depth)?;
                    let phase = state.roots.get(root).copied().unwrap_or(0);
                    if phase != CLOSED {
                        // The current owning generation remains in the parent
                        // continuation until its actual drain reports Ready.
                        // A stale owner, a retired generation, or an ordinary
                        // Direct cleanup has no such execution edge.
                        if phase == 0 || phase & !(OPEN | CLOSING | CLOSED) != 0
                            || !self.suspend_root(state.owner, depth + 1)? {
                            return Err(failure(
                                "original cleanup discards a live AsyncRegistration owner without its current Closed fact or a Suspend execution root",
                            ));
                        }
                    }
                }
                Value::Root(_, false, true, prior) if *prior != Some(CLOSED) => {
                    return Err(failure(
                        "original cleanup loses an earlier registration generation's owning responsibility",
                    ));
                }
                Value::Tuple(values) => {
                    for value in values {
                        self.drop_value(value, state, depth + 1)?;
                    }
                }
                Value::Fields(values) => {
                    for value in values.values() {
                        self.drop_value(value, state, depth + 1)?;
                    }
                }
                _ => {}
            }
            Ok(())
        })();
        self.record_cost("drop_value", cost_before, cost_children);
        result
    }
    fn site(
        &mut self,
        owner: SemanticDefinitionId,
        node: NodeId,
        depth: usize,
    ) -> R<Option<SemanticSiteId>> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<Option<SemanticSiteId>> = (|| {
            self.pay(lookup_work(self.call_sites.len()), depth)?;
            if !self.call_sites.contains_key(&owner) {
                let original = self.definition(owner)?;
                let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &original.body else {
                    return Ok(None);
                };
                self.pay(site_nodes.len(), depth)?;
                let mut sites = BTreeMap::new();
                let call_lookup = lookup_work(self.proof.calls.len());
                for row in site_nodes {
                    let SemanticOriginalNode::Expression(node) = row.node else {
                        continue;
                    };
                    self.pay(call_lookup, depth)?;
                    if !self.proof.calls.contains_key(&(owner, row.site)) {
                        continue;
                    }
                    self.pay(lookup_work(sites.len()), depth)?;
                    match sites.entry(node) {
                        std::collections::btree_map::Entry::Occupied(mut entry) => {
                            entry.insert(CallSite::Repeated);
                        }
                        std::collections::btree_map::Entry::Vacant(entry) => {
                            if self.call_site_count >= self.limits.max_nodes {
                                return Err(Diagnostic::limit(Phase::Types, "original protocol call site index node limit exceeded", None));
                            }
                            self.pay(std::mem::size_of::<(NodeId, CallSite)>(), depth)?;
                            entry.insert(CallSite::Unique(row.site));
                            self.call_site_count += 1;
                        }
                    }
                }
                self.pay(lookup_work(self.call_sites.len()).saturating_add(
                    std::mem::size_of::<(SemanticDefinitionId, BTreeMap<NodeId, CallSite>)>(),
                ), depth)?;
                self.call_sites.insert(owner, sites);
            }
            self.pay(lookup_work(self.call_sites.len()), depth)?;
            let directory = self.call_sites.get(&owner)
                .ok_or_else(|| failure("original protocol call occurrence index disappeared"))?;
            let work = lookup_work(directory.len());
            *self.left = self.left.checked_sub(work).ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "original protocol work exceeded", None)
            })?;
            let selected = directory.get(&node).copied();
            match selected {
                Some(CallSite::Repeated) => Err(failure(
                    "original protocol call occurrence has multiple call sites",
                )),
                Some(CallSite::Unique(site)) => Ok(Some(site)),
                None => Ok(None),
            }
        })();
        self.record_cost("site", cost_before, cost_children);
        result
    }
    fn output(
        &mut self,
        owner: SemanticDefinitionId,
        state: &State,
        contracts: &[FunctionProtocolContract],
        inputs: &BTreeMap<u32, ProtocolState>,
        depth: usize,
    ) -> R<()> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<()> = (|| {
            self.pay(
                contracts.iter().fold(state.work(), |w, c| {
                    w.saturating_add(c.exits.len().saturating_mul(4))
                        .saturating_add(1)
                }),
                depth,
            )?;
            if contracts.is_empty() {
                return Ok(());
            }
            let outcomes = match &state.returned {
                Value::Poll {
                    fresh: true,
                    outcomes,
                } => {
                    self.pay(state.returned.work(), depth)?;
                    outcomes.clone()
                }
                Value::Poll { fresh: false, .. } => {
                    return Err(failure(
                        "returned Poll was invalidated by a later operation on its original root",
                    ));
                }
                _ => {
                    if contracts
                        .iter()
                        .any(|c| c.exits.iter().any(|r| r.exit != ProtocolExit::Return))
                    {
                        return Err(pending(
                            "original conditional Error/Poll result lacks its complete tagged source relation",
                        ));
                    }
                    self.pay(state.roots.len().saturating_add(1), depth)?;
                    vec![Outcome {
                        exit: ProtocolExit::Return,
                        phases: state.roots.clone(),
                    }]
                }
            };
            for outcome in outcomes {
                for contract in contracts {
                    let input = inputs
                        .get(&contract.parameter)
                        .ok_or_else(|| failure("protocol input case is absent"))?;
                    let root = Root::Parameter(owner, contract.parameter);
                    let actual = outcome
                        .phases
                        .get(&root)
                        .or_else(|| state.roots.get(&root))
                        .copied()
                        .ok_or_else(|| failure("protocol exit lost its original parameter root"))?;
                    self.pay(contract.exits.len(), depth)?;
                    let row = contract
                    .exits
                    .iter()
                    .find(|r| r.input == *input && r.exit == outcome.exit)
                    .ok_or_else(|| {
                        failure(
                            "reachable original protocol exit is absent from the declared relation",
                        )
                    })?;
                    if actual == 0 || actual & !output_mask(&row.output) != 0 {
                        return Err(failure(
                            "original protocol exit exceeds its declared conditional phase relation",
                        ));
                    }
                }
            }
            Ok(())
        })();
        self.record_cost("output", cost_before, cost_children);
        result
    }
    fn nominal_value(
        &mut self,
        owner: SemanticDefinitionId,
        node: NodeId,
        values: &[Value],
        state: &State,
        depth: usize,
    ) -> R<Option<Value>> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<Option<Value>> = (|| {
            self.pay(lookup_work(self.proof.nominal_constructors.len()), depth)?;
            let Some((id, variant)) = self.proof.nominal_constructors.get(&(owner, node)).copied()
            else {
                return Ok(None);
            };
            let original = self.definition(id)?;
            let SemanticDefinitionBody::Nominal {
                declaration,
                body: SemanticNominalBody::Enum { variants },
                ..
            } = &original.body
            else {
                return Err(failure(
                    "original constructor theorem names another declaration kind",
                ));
            };
            let selected = variants
                .get(variant as usize)
                .ok_or_else(|| failure("original constructor theorem selects an absent variant"))?;
            if selected.fields.len() != values.len() {
                return Err(failure(
                    "original constructor cleanup changes the complete argument arity",
                ));
            }
            if *declaration == zil_ir::builtin_life::POLL
                && values.iter().all(|value| matches!(value, Value::Plain))
            {
                let exit = match variant {
                    0 => ProtocolExit::PollPending,
                    1 => ProtocolExit::PollReady,
                    _ => {
                        return Err(failure(
                            "canonical Poll constructor changes its original variant",
                        ));
                    }
                };
                self.pay(state.roots.len().saturating_add(1), depth)?;
                return Ok(Some(Value::Poll {
                    fresh: true,
                    outcomes: vec![Outcome {
                        exit,
                        phases: state.roots.clone(),
                    }],
                }));
            }
            self.pay(selected.name.len().saturating_add(values.len()), depth)?;
            let mut fields = Vec::with_capacity(values.len());
            for value in values {
                fields.push(self.value_copy(value, depth + 1)?)
            }
            Ok(Some(Value::Variant {
                name: selected.name.clone(),
                fields,
            }))
        })();
        self.record_cost("nominal_value", cost_before, cost_children);
        result
    }
    fn call(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        _arguments: &[SemanticOriginalExpr],
        values: &[Value],
        state: &mut State,
        depth: usize,
    ) -> R<Value> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<Value> = (|| {
            if let Some(value) =
                self.nominal_value(owner, expression.id, values, state, depth + 1)?
            {
                return Ok(value);
            }
            let Some(site) = self.site(owner, expression.id, depth)? else {
                return self.unknown_call(owner, expression.id, values, state, depth);
            };
            self.pay(lookup_work(self.proof.intrinsics.len()), depth)?;
            if let Some(call) = self.proof.intrinsics.get(&(owner, site)) {
                let work = match &call.kind {
                    IntrinsicKind::DynCall {
                        parameter_modes, ..
                    } => parameter_modes.len().saturating_add(1),
                    IntrinsicKind::RegistrationStart { providers, .. } => {
                        providers.len().saturating_add(1)
                    }
                    _ => 1,
                };
                self.pay(work, depth)?;
            }
            self.pay(lookup_work(self.proof.intrinsics.len()), depth)?;
            let intrinsic = self
                .proof
                .intrinsics
                .get(&(owner, site))
                .map(|c| c.kind.clone());
            if let Some(kind) = intrinsic {
                let body = match (&kind, values) {
                    (IntrinsicKind::ThreadScope { .. }, [Value::Plain, _, body]) => Some(body),
                    (IntrinsicKind::SyncCellScope(_), [_, body]) => Some(body),
                    (IntrinsicKind::RegistrationStart { form: CallbackRegistration::Scoped, .. },
                        [_, body]) => Some(body),
                    (IntrinsicKind::ThreadScope { .. } | IntrinsicKind::SyncCellScope(_) | IntrinsicKind::RegistrationStart {
                        form: CallbackRegistration::Scoped, ..
                    }, _) => return Err(failure("scoped body lost its complete call operands")),
                    _ => None,
                };
                if let Some(body) = body {
                    self.check_scoped_body_consumption(body, state, depth + 1)?;
                }
                if let Some(value) =
                    self.task_call(owner, expression.id, site, &kind, values, state, depth + 1)?
                {
                    return Ok(value);
                }
                match kind {
                    IntrinsicKind::OwnedDyn => {
                        let [value] = values else {
                            return Err(failure(
                                "original owned Dyn construction changes its source arity",
                            ));
                        };
                        self.drop_value(value, state, depth + 1)?;
                        let source = self.value_copy(value, depth + 1)?;
                        return self.choice(
                            vec![
                                Value::Error {
                                    is_error: false,
                                    payload: Box::new(Value::Boxed(Box::new(source))),
                                },
                                Value::Error {
                                    is_error: true,
                                    payload: Box::new(Value::Plain),
                                },
                            ],
                            depth + 1,
                        );
                    }
                    IntrinsicKind::RegistrationStart {
                        form: CallbackRegistration::AsyncOwned, ..
                    } => {
                        let root = Root::Created(owner, expression.id);
                        self.change(state, root, OPEN | CLOSED, true, depth)?;
                        return Ok(Value::Root(root, true, true, None));
                    }
                    IntrinsicKind::NominalConstructor { .. } => {
                        return Err(failure(
                            "original constructor call lacks its complete expression theorem",
                        ));
                    }
                    IntrinsicKind::RegistrationOperation(
                        op @ (RegistrationOp::AsyncValid
                        | RegistrationOp::AsyncRequest
                        | RegistrationOp::AsyncPollClose
                        | RegistrationOp::AsyncFinish
                        | RegistrationOp::AsyncFinishChecked),
                    ) => {
                        let [value] = values else {
                            return Err(failure(
                                "sealed protocol operation changes original argument arity",
                            ));
                        };
                        let root = self.root(value, state, depth)?;
                        let phase = state.roots[&root];
                        if op == RegistrationOp::AsyncValid {
                            return Ok(Value::Valid {
                                root,
                                fresh: true,
                                negated: false,
                            });
                        }
                        if matches!(
                            op,
                            RegistrationOp::AsyncFinish | RegistrationOp::AsyncFinishChecked
                        ) {
                            if op == RegistrationOp::AsyncFinishChecked && phase != CLOSED {
                                return Err(failure(
                                    "original registration reclamation is not dominated by Closed for this owning root",
                                ));
                            }
                            // The unchecked operation retains the sealed runtime
                            // CLOSED guard: a nonclosed execution faults without
                            // reclaiming anything. Only its successful return
                            // retires the owning generation here.
                            self.change(state, root, RETIRED, true, depth)?;
                            return Ok(Value::Plain);
                        }
                        let closing = if phase & (OPEN | CLOSING) != 0 {
                            CLOSING
                        } else {
                            0
                        };
                        if op == RegistrationOp::AsyncRequest {
                            self.change(state, root, closing | (phase & CLOSED), false, depth)?;
                            return Ok(Value::Plain);
                        }
                        self.change(state, root, closing | CLOSED, false, depth)?;
                        self.pay(4, depth)?;
                        let mut outcomes = Vec::new();
                        if closing != 0 {
                            outcomes.push(Outcome {
                                exit: ProtocolExit::PollPending,
                                phases: BTreeMap::from([(root, CLOSING)]),
                            });
                        }
                        outcomes.push(Outcome {
                            exit: ProtocolExit::PollReady,
                            phases: BTreeMap::from([(root, CLOSED)]),
                        });
                        return Ok(Value::Poll {
                            fresh: true,
                            outcomes,
                        });
                    }
                    // Other sealed primitives do not gain protocol facts. Moving a
                    // root through a Task or container requires its separate proof.
                    _ => return self.unknown_call(owner, expression.id, values, state, depth),
                }
            }
            self.pay(lookup_work(self.proof.call_targets.len()), depth)?;
            let target = self.proof.call_targets.get(&(owner, site)).copied();
            let Some(target) = target else {
                // An abstract or environment call may reach a captured root even
                // without an explicit resource parameter. It has no neutral-state
                // theorem merely because its value signature is Copy.
                self.pay(state.roots.len(), depth)?;
                let roots: Vec<_> = state.roots.keys().copied().collect();
                for root in roots {
                    self.change(state, root, UNKNOWN, false, depth)?;
                    state.task_sync.remove(&root);
                }
                return self.unknown_call(owner, expression.id, values, state, depth);
            };
            self.check_consuming_target(target, values, None, state, depth + 1)?;
            let callee = self.definition(target)?;
            let SemanticDefinitionBody::FunctionWithBody {
                protocol_contracts,
                parameters,
                ..
            } = &callee.body
            else {
                return self.unknown_call(owner, expression.id, values, state, depth);
            };
            if protocol_contracts.is_empty() {
                return self.unknown_call(owner, expression.id, values, state, depth);
            }
            self.function(target, depth + 1)?;
            if !self.proof.protocol_complete.contains(&target) {
                return Err(failure(
                    "callee protocol body is not independently complete",
                ));
            }
            if parameters.len() != values.len() {
                return Err(pending(
                    "protocol method call requires its full original receiver argument correspondence",
                ));
            }
            self.pay(protocol_contracts.len().saturating_add(values.len()), depth)?;
            let mut roles = Vec::new();
            let mut seen = BTreeSet::new();
            for contract in protocol_contracts {
                let argument = values.get(contract.parameter as usize).ok_or_else(|| {
                    failure("protocol parameter is not an original call argument")
                })?;
                let root = self.root(argument, state, depth)?;
                if parameters[contract.parameter as usize].mode == PassingMode::Give
                    && !matches!(argument, Value::Root(_, true, true, _))
                {
                    return Err(failure(
                        "retiring protocol argument is not its actual live Give owner",
                    ));
                }
                if !seen.insert(root) {
                    return Err(failure(
                        "independent protocol roles alias one exclusive original root",
                    ));
                }
                let phase = state.roots[&root];
                if phase == 0 || phase & !mask(&contract.entry) != 0 {
                    return Err(failure(
                        "caller has not proved the protocol entry state of this actual argument root",
                    ));
                }
                roles.push((root, phase, contract));
            }
            self.pay(5usize.saturating_mul(roles.len()), depth)?;
            let mut outcomes = Vec::new();
            for exit in [
                ProtocolExit::Return,
                ProtocolExit::ErrorOk,
                ProtocolExit::ErrorErr,
                ProtocolExit::PollPending,
                ProtocolExit::PollReady,
            ] {
                let mut phases = BTreeMap::new();
                for (root, phase, contract) in &roles {
                    let mut output = 0;
                    self.pay(contract.exits.len().saturating_mul(4), depth)?;
                    for row in &contract.exits {
                        if bit(row.input) & *phase != 0 && row.exit == exit {
                            output |= output_mask(&row.output);
                        }
                    }
                    if output == 0 {
                        phases.clear();
                        break;
                    }
                    phases.insert(*root, output);
                }
                if !phases.is_empty() {
                    outcomes.push(Outcome { exit, phases });
                }
            }
            if outcomes.is_empty() {
                return Err(failure(
                    "callee protocol has no reachable result for the proved entry states",
                ));
            }
            for (root, _, _) in &roles {
                let joined = outcomes
                    .iter()
                    .fold(0, |m, o| m | o.phases.get(root).copied().unwrap_or(0));
                self.change(state, *root, joined, joined == RETIRED, depth)?;
            }
            if outcomes.iter().all(|o| o.exit == ProtocolExit::Return) {
                return Ok(Value::Plain);
            }
            if outcomes.iter().any(|o| {
                matches!(
                    o.exit,
                    ProtocolExit::ErrorOk | ProtocolExit::ErrorErr | ProtocolExit::Return
                )
            }) {
                return Err(pending(
                    "original protocol call requires a complete ErrorUnion result correlation",
                ));
            }
            Ok(Value::Poll {
                fresh: true,
                outcomes,
            })
        })();
        self.record_cost("call", cost_before, cost_children);
        result
    }
    fn unknown_call(
        &mut self,
        owner: SemanticDefinitionId,
        node: NodeId,
        values: &[Value],
        state: &mut State,
        depth: usize,
    ) -> R<Value> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<Value> = (|| {
            let mut roots = BTreeSet::new();
            for value in values {
                self.pay(value.work(), depth)?;
                value.roots(&mut roots);
            }
            // Preserve entry facts only for instantiating an independently
            // proved returned-owner summary. Unknown effects invalidate the
            // caller's surviving occupants, including synchronous graph facts.
            let incoming = if roots.is_empty() { None } else { Some(self.copy(state, depth + 1)?) };
            for root in roots {
                self.change(state, root, UNKNOWN, false, depth)?;
                state.task_sync.remove(&root);
            }
            if let Some(site) = self.site(owner, node, depth + 1)? {
                if let Some(target) = self.proof.call_targets.get(&(owner, site)).copied() {
                    if let Some(value) =
                        self.task_return_call(owner, node, target, values, state, incoming.as_ref(), depth + 1)?
                    {
                        return Ok(value);
                    }
                }
            }
            if self.query_async(owner, Query::Expression(node), depth)? {
                let root = Root::Created(owner, node);
                self.change(state, root, UNKNOWN, true, depth)?;
                Ok(Value::Root(root, true, true, None))
            } else {
                Ok(Value::Plain)
            }
        })();
        self.record_cost("unknown_call", cost_before, cost_children);
        result
    }
    fn items(
        &mut self,
        owner: SemanticDefinitionId,
        items: &[SemanticOriginalExpr],
        state: &mut State,
        depth: usize,
    ) -> R<Vec<Value>> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<Vec<Value>> = (|| {
            self.pay(items.len(), depth)?;
            let start = state.temporaries.len();
            for item in items {
                let value = self.expr(owner, item, state, depth + 1)?;
                state.temporaries.push(value);
            }
            Ok(state.temporaries.drain(start..).collect())
        })();
        self.record_cost("items", cost_before, cost_children);
        result
    }
    fn expr(
        &mut self,
        owner: SemanticDefinitionId,
        expr: &SemanticOriginalExpr,
        state: &mut State,
        depth: usize,
    ) -> R<Value> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<Value> = (|| {
            if !state.reachable {
                return Ok(Value::Plain);
            }
            let mut value =
                self.expr_inner(owner, expr, state, depth)
                    .map_err(|mut diagnostic| {
                        diagnostic.notes.push(format!(
                            "original operation owner {:?}, expression {:?}, span {:?}, kind {:?}",
                            owner,
                            expr.id,
                            expr.span,
                            std::mem::discriminant(&expr.kind)
                        ));
                        diagnostic
                    })?;
            self.pay(lookup_work(self.proof.injections.len()), depth)?;
            if let Some(injection) = self.proof.injections.get(&(owner, expr.id)) {
                let is_error = injection.is_error;
                let payload = copy_recipe(&injection.payload, self.limits, self.left)?;
                value = Value::Error {
                    is_error,
                    payload: Box::new(self.typed(owner, &payload, value, depth + 1)?),
                };
            }
            if let Value::Root(_, true, owning, _) = &mut value {
                if let Some(actual) =
                    self.query_ownership(owner, Query::Expression(expr.id), depth + 1)?
                {
                    *owning = actual;
                }
            }
            if let Some(ty) = self.query_type(owner, Query::Expression(expr.id), depth + 1)? {
                let category = self.category(owner, &ty, depth + 1)?;
                if category == cleanup::Category::Own
                    && matches!(
                        expr.kind,
                        CoreExprKind::Name(_) | CoreExprKind::Member { .. }
                    )
                {
                    // A sealed constructor expression can be spelled as a static
                    // member. It creates a value instead of reading a local place.
                    self.pay(lookup_work(self.proof.nominal_constructors.len()), depth)?;
                    let constructed = if self
                        .proof
                        .nominal_constructors
                        .contains_key(&(owner, expr.id))
                    {
                        true
                    } else if let Some(site) = self.site(owner, expr.id, depth + 1)? {
                        self.pay(lookup_work(self.proof.intrinsics.len()), depth)?;
                        matches!(
                            self.proof
                                .intrinsics
                                .get(&(owner, site))
                                .map(|row| &row.kind),
                            Some(IntrinsicKind::NominalConstructor { .. })
                        )
                    } else {
                        false
                    };
                    if !constructed {
                        self.move_place(expr, state, depth + 1).map_err(|mut diagnostic| {
                        diagnostic.notes.push(format!("original transfer owner {:?}, expression {:?}, span {:?}, operation {:?}", owner, expr.id, expr.span, std::mem::discriminant(&expr.kind)));
                        diagnostic
                    })?;
                    }
                }
                value = self.typed(owner, &ty, value, depth + 1)?;
            }
            Ok(value)
        })();
        self.record_cost("expr", cost_before, cost_children);
        result.map_err(|diagnostic| crate::semantics::locate_original(diagnostic, expr.span))
    }
    fn expr_inner(
        &mut self,
        owner: SemanticDefinitionId,
        expr: &SemanticOriginalExpr,
        state: &mut State,
        depth: usize,
    ) -> R<Value> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<Value> = (|| {
            self.pay(1, depth)?;
            match &expr.kind {
                CoreExprKind::Name(name) => {
                    self.pay(
                        name.len().saturating_add(lookup_work(state.names.len())),
                        depth,
                    )?;
                    if let Some(value) = state.names.get(name).and_then(|s| s.last()) {
                        self.value_copy(value, depth)
                    } else {
                        Ok(Value::Plain)
                    }
                }
                CoreExprKind::Call { callee, arguments } => {
                    let site = self.site(owner, expr.id, depth + 1)?;
                    let callback = if let Some(site) = site {
                        self.pay(lookup_work(self.proof.intrinsics.len()), depth)?;
                        self.proof
                            .intrinsics
                            .get(&(owner, site))
                            .is_some_and(|call| {
                                matches!(call.kind, IntrinsicKind::ForeignCallback { .. })
                            })
                    } else {
                        false
                    };
                    if callback {
                        // The original scheme already authenticated the symbol. It
                        // is a code address formation, with no runtime operand.
                        return Ok(Value::Plain);
                    }
                    let once = if let Some(site) = site {
                        self.pay(lookup_work(self.proof.callable_calls.len()), depth)?;
                        self.proof
                            .callable_calls
                            .get(&(owner, site))
                            .is_some_and(|call| call.kind == CallableKind::FnOnce)
                    } else {
                        false
                    };
                    let mut receiver = None;
                    if let Some(site) = site {
                        self.pay(lookup_work(self.proof.method_calls.len()), depth)?;
                        if let Some(method) = self.proof.method_calls.get(&(owner, site)) {
                            let declaration_id = method.declaration;
                            crate::dyn_metadata::semantic_operand_snapshot(
                                &method.signature,
                                self.limits,
                                self.left,
                                1,
                            )?;
                            let signature = method.signature.clone();
                            let declaration = self.definition(declaration_id)?;
                            if let SemanticDefinitionBody::FunctionWithBody {
                                receiver_parameter: Some(index),
                                ..
                            } = &declaration.body
                            {
                                let parameter=signature.parameters.get(*index as usize).ok_or_else(||failure("original method receiver ordinal is absent from its complete scheme"))?;
                                receiver = Some((
                                    *index as usize,
                                    parameter.mode,
                                    copy_recipe(&parameter.ty, self.limits, self.left)?,
                                ));
                            }
                        }
                    }
                    let dynamic = site
                        .and_then(|site| self.proof.intrinsics.get(&(owner, site)))
                        .is_some_and(|call| matches!(call.kind, IntrinsicKind::DynCall { .. }));
                    let mut callee_base = callee.as_ref();
                    while let CoreExprKind::TypeApply { callee, .. }
                    | CoreExprKind::GenericApply { callee, .. } = &callee_base.kind
                    {
                        self.pay(1, depth)?;
                        callee_base = callee;
                    }
                    let object = if receiver.is_some() || dynamic {
                        let CoreExprKind::Member { object, .. } = &callee_base.kind else {
                            return Err(failure(
                                "original receiver call has no actual source object",
                            ));
                        };
                        Some(object.as_ref())
                    } else {
                        None
                    };
                    let source = object.unwrap_or(callee.as_ref());
                    let mut callee_value = self.observe(owner, source, state, depth + 1)?;
                    let callable = site.is_some_and(|site| self.proof.callable_calls.contains_key(&(owner, site)));
                    let graph_callee = if self.graph_requested.contains(&owner) {
                        Some(self.value_copy(&callee_value, depth + 1)?)
                    } else { None };
                    let consume_receiver = if let Some((_, mode, ty)) = &receiver {
                        matches!(mode, PassingMode::Give | PassingMode::Move)
                            || *mode == PassingMode::Infer
                                && cleanup::category(
                                    self.program,
                                    self.proof,
                                    owner,
                                    ty,
                                    self.limits,
                                    self.left,
                                )? == cleanup::Category::Own
                    } else {
                        false
                    };
                    let consume = once || consume_receiver;
                    let temporary = self.place_path(source, depth + 1)?.is_none();
                    let callee_slot = state.temporaries.len();
                    if consume && !temporary {
                        self.move_place(source, state, depth + 1)?;
                    }
                    let receiver_value = if object.is_some() {
                        let value = self.value_copy(&callee_value, depth + 1)?;
                        Some(self.view(value, depth + 1)?)
                    } else {
                        None
                    };
                    if !consume && !temporary {
                        callee_value = self.view(callee_value, depth + 1)?;
                    }
                    // The view of a real stored receiver remains in the same
                    // preparation inventory as owning callees. Argument-side
                    // replacement must invalidate that exact occurrence too.
                    if consume || temporary || callable {
                        self.pay(1, depth)?;
                        state.temporaries.push(callee_value);
                    }
                    // Provider symbols were checked in the declaration namespace. Only
                    // runtime operands can transfer values into the preparation inventory.
                    let runtime_arguments = if let Some(IntrinsicCall {
                        kind: IntrinsicKind::RegistrationStart { form, .. }, ..
                    }) = site.and_then(|site| self.proof.intrinsics.get(&(owner, site))) {
                        let operands = form.operands(arguments).ok_or_else(||
                            failure("registration loses its declared operand roles"))?;
                        &arguments[operands.providers.len()..]
                    } else { arguments.as_slice() };
                    let mut values = self.items(owner, runtime_arguments, state, depth + 1)?;
                    if let Some(value) = receiver_value {
                        let ordinal = receiver.as_ref().map_or(0, |(index, _, _)| *index);
                        if ordinal > values.len() {
                            return Err(failure(
                                "original receiver ordinal changes complete argument arity",
                            ));
                        }
                        self.pay(values.len().saturating_add(1), depth)?;
                        values.insert(ordinal, value);
                    }
                    let consuming_incoming = if once && state.reachable { Some(self.consumption_state(state, depth + 1)?) } else { None };
                    let result = if state.reachable {
                        if let (Some(site), Some(callee)) = (site, graph_callee) {
                            self.record_graph_call(owner, site, callee, &values, state, depth + 1)?;
                        }
                        // Preserve this exact invocation's returned owner and
                        // surviving input states before using unknown effects.
                        // A reusable receiver stays borrowed; its phase facts
                        // never establish cleanup of an uncalled environment.
                        let returned = if callable {
                            let prepared = self.value_copy(&state.temporaries[callee_slot], depth + 1)?;
                            self.task_return_callable(owner, expr.id, callee_base.id,
                                &prepared, &values, state, depth + 1)?
                        } else { None };
                        if let Some(returned) = returned { returned }
                        else { self.call(owner, expr, arguments, &values, state, depth + 1)? }
                    } else {
                        Value::Plain
                    };
                    if consume || temporary || callable {
                        let callee = state.temporaries.remove(callee_slot);
                        if state.reachable {
                            if once {
                                let operation = self.proof.callable_calls.get(&(owner, site.ok_or_else(|| failure("consuming call lost its original site"))?))
                                    .ok_or_else(|| failure("consuming call lost its complete original operation"))?.signature.call_kind;
                                self.check_consuming_invocation(&callee, &values, operation, consuming_incoming.as_ref().ok_or_else(|| failure("consuming call lost its actual prepared inputs"))?, depth + 1)?;
                            } else if !consume {
                                self.drop_value(&callee, state, depth + 1)?;
                            }
                        }
                    }
                    Ok(result)
                }
                CoreExprKind::Unary { op, operand } => {
                    let borrowing = matches!(
                        op,
                        UnaryOp::Borrow | UnaryOp::BorrowMut | UnaryOp::Dereference | UnaryOp::Give
                    );
                    let mut value = if borrowing {
                        self.observe(owner, operand, state, depth + 1)?
                    } else {
                        self.expr(owner, operand, state, depth + 1)?
                    };
                    if *op == UnaryOp::Give {
                        self.move_place(operand, state, depth + 1)?;
                        if let Value::Root(_, _, owning, _) = &mut value {
                            *owning = true;
                        }
                        return Ok(value);
                    }
                    if matches!(
                        op,
                        UnaryOp::Borrow | UnaryOp::BorrowMut | UnaryOp::Dereference
                    ) {
                        if let Value::Root(_, _, owning, _) = &mut value {
                            *owning = false;
                        }
                    }
                    if *op == UnaryOp::Not {
                        if let Value::Valid {
                            root,
                            fresh,
                            negated,
                        } = value
                        {
                            return Ok(Value::Valid {
                                root,
                                fresh,
                                negated: !negated,
                            });
                        }
                        if let Value::Bool(value) = value {
                            return Ok(Value::Bool(!value));
                        }
                        return Ok(Value::Plain);
                    }
                    if matches!(
                        op,
                        UnaryOp::Borrow | UnaryOp::BorrowMut | UnaryOp::Give | UnaryOp::Dereference
                    ) {
                        Ok(value)
                    } else {
                        Ok(Value::Plain)
                    }
                }
                CoreExprKind::Group(value) | CoreExprKind::Ascribed { value, .. } => {
                    self.expr(owner, value, state, depth + 1)
                }
                CoreExprKind::TypeApply { callee, .. }
                | CoreExprKind::GenericApply { callee, .. } => {
                    self.expr(owner, callee, state, depth + 1)
                }
                CoreExprKind::Tuple(items) | CoreExprKind::Array(items) => {
                    Ok(Value::Tuple(self.items(owner, items, state, depth + 1)?))
                }
                CoreExprKind::Record { fields, .. }
                | CoreExprKind::RecordConstructor { fields, .. } => {
                    self.pay(fields.len(), depth)?;
                    let start = state.temporaries.len();
                    for (_, expr) in fields {
                        let value = self.expr(owner, expr, state, depth + 1)?;
                        state.temporaries.push(value);
                    }
                    let values: Vec<_> = state.temporaries.drain(start..).collect();
                    let mut out = BTreeMap::new();
                    for ((name, _), value) in fields.iter().zip(values) {
                        self.pay(name.len().saturating_add(1), depth)?;
                        out.insert(name.clone(), value);
                    }
                    Ok(Value::Fields(out))
                }
                CoreExprKind::RecordUpdate { base, fields, .. } => {
                    let mut value = self.expr(owner, base, state, depth + 1)?;
                    if !state.reachable {
                        return Ok(Value::Plain);
                    }
                    let ty = self.query_type(owner, Query::Expression(base.id), depth + 1)?
                        .ok_or_else(|| failure("original record update has no solved base type"))?;
                    // Ordinary Copy parameters and results can carry Plain in
                    // the cleanup engine. Reconstruct their declaration shape
                    // without adding ownership identities or permissions.
                    if matches!(value, Value::Plain)
                        && self.category(owner, &ty, depth + 1)? == cleanup::Category::Copy
                    {
                        value = self.plain_value(owner, &ty, depth + 1)?;
                    }
                    if !matches!(value, Value::Fields(_)) {
                        return Err(pending(
                            "original record update requires the complete base field inventory",
                        ));
                    }
                    let order = self.product_fields(owner, &ty, depth + 1)?
                        .ok_or_else(|| failure("original record update loses its declared field order"))?;
                    self.pay(1, depth)?;
                    let slot = state.temporaries.len();
                    state.temporaries.push(value);
                    let mut prepared = BTreeMap::new();
                    for (name, expression) in fields {
                        let value = self.expr(owner, expression, state, depth + 1)?;
                        if !state.reachable {
                            state.temporaries.truncate(slot);
                            return Ok(Value::Plain);
                        }
                        self.pay(name.len().saturating_add(lookup_work(prepared.len())).saturating_add(1), depth)?;
                        if prepared.insert(name.as_str(), state.temporaries.len()).is_some() {
                            return Err(failure("original record update repeats a prepared field"));
                        }
                        state.temporaries.push(value);
                    }
                    for (name, _) in order.iter() {
                        self.pay(name.len().saturating_add(lookup_work(prepared.len())), depth)?;
                        let Some(index) = prepared.remove(name.as_str()) else {
                            continue;
                        };
                        let value = std::mem::replace(&mut state.temporaries[index], Value::Plain);
                        let Value::Fields(base) = &mut state.temporaries[slot] else {
                            return Err(failure("original record update lost its in-flight base"));
                        };
                        self.pay(name.len().saturating_add(lookup_work(base.len())), depth)?;
                        let field = base.get_mut(name).ok_or_else(|| {
                            failure("original record update selects an absent base field")
                        })?;
                        let old = std::mem::replace(field, value);
                        self.drop_value(&old, state, depth + 1)?;
                    }
                    if !prepared.is_empty() {
                        return Err(failure("original record update prepared a field outside its declaration"));
                    }
                    state.temporaries.truncate(slot + 1);
                    Ok(state.temporaries.pop().expect("complete update base"))
                }
                CoreExprKind::Member { .. } => {
                    if let Some(value) =
                        self.nominal_value(owner, expr.id, &[], state, depth + 1)?
                    {
                        Ok(value)
                    } else {
                        self.observe(owner, expr, state, depth + 1)
                    }
                }
                CoreExprKind::Index { object, index } => {
                    let value = self.observe(owner, object, state, depth + 1)?;
                    self.expr(owner, index, state, depth + 1)?;
                    let mut roots = BTreeSet::new();
                    self.pay(value.work(), depth)?;
                    value.roots(&mut roots);
                    if !roots.is_empty() {
                        return Err(pending(
                            "protocol root projection requires its exact original constant index",
                        ));
                    }
                    Ok(Value::Plain)
                }
                CoreExprKind::Slice { object, start, end } => {
                    // The original place rule already proves the array/slice and
                    // integer endpoints. Slicing creates a view and transfers no
                    // owning element or container responsibility.
                    let source = self.observe(owner, object, state, depth + 1)?;
                    let begin = state.temporaries.len();
                    state.temporaries.push(self.view(source, depth + 1)?);
                    for bound in start.iter().chain(end.iter()) {
                        let value = self.expr(owner, bound, state, depth + 1)?;
                        self.drop_value(&value, state, depth + 1)?;
                    }
                    state.temporaries.truncate(begin);
                    Ok(Value::Plain)
                }
                CoreExprKind::Sequence { bindings, value } => {
                    let mut old = Vec::new();
                    let error_start = state.abrupt.len();
                    self.pay(bindings.len(), depth)?;
                    for (name, expr) in bindings {
                        let value = self.expr(owner, expr, state, depth + 1)?;
                        self.pay(name.len().saturating_add(1), depth)?;
                        old.push((name.clone(), state.names.get(name).map_or(0, Vec::len)));
                        state.names.entry(name.clone()).or_default().push(value);
                    }
                    let result = self.expr(owner, value, state, depth + 1)?;
                    self.restore(state, &old, depth)?;
                    let exits = state.abrupt.drain(error_start..).collect::<Vec<_>>();
                    for mut exit in exits {
                        self.restore(&mut exit, &old, depth + 1)?;
                        state.abrupt.push(exit);
                    }
                    Ok(result)
                }
                CoreExprKind::If {
                    condition,
                    then_value,
                    else_value,
                } => {
                    let condition = self.expr(owner, condition, state, depth + 1)?;
                    let mut left = self.copy(state, depth)?;
                    let mut right = self.copy(state, depth)?;
                    self.condition(&condition, true, &mut left, depth)?;
                    self.condition(&condition, false, &mut right, depth)?;
                    let a = self.expr(owner, then_value, &mut left, depth + 1)?;
                    let b = self.expr(owner, else_value, &mut right, depth + 1)?;
                    // Unknown result provenance cannot supply a phase refinement.
                    self.pay(a.work().saturating_add(b.work()), depth)?;
                    let result = if !left.reachable {
                        b
                    } else if !right.reachable || a == b {
                        a
                    } else {
                        self.pay(2, depth)?;
                        self.choice(vec![a, b], depth + 1)?
                    };
                    *state = self.merge(&left, &right, depth)?;
                    Ok(result)
                }
                CoreExprKind::Try(value) => {
                    let value = self.expr(owner, value, state, depth + 1)?;
                    let definition = self.definition(owner)?;
                    if matches!(&definition.body,SemanticDefinitionBody::FunctionWithBody{protocol_contracts,..}if !protocol_contracts.is_empty())
                    {
                        return Err(pending(
                            "original protocol ErrorUnion exits require their complete conditional table correlation",
                        ));
                    }
                    let mut payloads = vec![value];
                    let mut normal = Vec::new();
                    while let Some(value) = payloads.pop() {
                        self.pay(value.work().saturating_add(1), depth)?;
                        match value {
                            Value::Choice(values) => {
                                self.pay(values.len(), depth)?;
                                payloads.extend(values);
                            }
                            Value::Error {
                                is_error: true,
                                payload,
                            } => {
                                let mut exit = self.copy(state, depth + 1)?;
                                exit.abrupt.clear();
                                exit.control = Control::Return;
                                exit.returned = Value::Error {
                                    is_error: true,
                                    payload,
                                };
                                for value in &exit.temporaries {
                                    self.drop_value(value, &exit, depth + 1)?;
                                }
                                exit.temporaries.clear();
                                self.pay(1, depth)?;
                                state.abrupt.push(exit);
                            }
                            Value::Error {
                                is_error: false,
                                payload,
                            } => normal.push(*payload),
                            Value::Plain => {
                                // The whole union is Copy; the two ordinary payload
                                // branches have no owning cleanup responsibility.
                                let mut exit = self.copy(state, depth + 1)?;
                                exit.abrupt.clear();
                                exit.control = Control::Return;
                                exit.returned = Value::Plain;
                                for value in &exit.temporaries {
                                    self.drop_value(value, &exit, depth + 1)?;
                                }
                                exit.temporaries.clear();
                                state.abrupt.push(exit);
                                normal.push(Value::Plain);
                            }
                            _ => {
                                return Err(pending(
                                    "original cleanup propagation lacks its exact ErrorUnion payload inventory",
                                ));
                            }
                        }
                    }
                    if normal.is_empty() {
                        state.reachable = false;
                        Ok(Value::Plain)
                    } else {
                        self.choice(normal, depth + 1)
                    }
                }
                CoreExprKind::Binary {
                    op: op @ (BinaryOp::And | BinaryOp::Or),
                    left,
                    right,
                } => {
                    let left = self.expr(owner, left, state, depth + 1)?;
                    let mut skip = self.copy(state, depth + 1)?;
                    let execute = *op == BinaryOp::And;
                    self.condition(&left, !execute, &mut skip, depth + 1)?;
                    self.condition(&left, execute, state, depth + 1)?;
                    let value = if state.reachable {
                        self.expr(owner, right, state, depth + 1)?
                    } else {
                        Value::Plain
                    };
                    let run = self.copy(state, depth + 1)?;
                    *state = self.merge(&run, &skip, depth + 1)?;
                    self.choice(vec![value, Value::Bool(!execute)], depth + 1)
                }
                CoreExprKind::Binary { left, right, .. }
                | CoreExprKind::Range {
                    start: left,
                    end: right,
                } => {
                    self.expr(owner, left, state, depth + 1)?;
                    self.expr(owner, right, state, depth + 1)?;
                    Ok(Value::Plain)
                }
                CoreExprKind::NumericConvert { value, .. }
                | CoreExprKind::IntegerConvert { value, .. } => {
                    self.expr(owner, value, state, depth + 1)?;
                    Ok(Value::Plain)
                }
                CoreExprKind::Closure { move_capture, .. } => {
                    let child = self.closure_child(owner, expr.id, depth + 1)?;
                    self.pay(
                        self.proof.capture_names.len().saturating_add(1).ilog2() as usize
                            + 1
                            + lookup_work(self.proof.closure_move.len()),
                        depth,
                    )?;
                    if self.proof.closure_move.get(&child.id) != Some(move_capture) {
                        return Err(failure(
                            "original cleanup closure mode differs from its true creator",
                        ));
                    }
                    let names = self.proof.capture_names.get(&child.id).ok_or_else(|| {
                        failure("original cleanup closure lacks its complete capture directory")
                    })?;
                    self.pay(
                        names
                            .iter()
                            .fold(names.len(), |n, name| n.saturating_add(name.len())),
                        depth,
                    )?;
                    self.pay(lookup_work(self.proof.capture_names.len()), depth)?;
                    let names = self
                        .proof
                        .capture_names
                        .get(&child.id)
                        .ok_or_else(|| failure("original cleanup capture directory disappeared"))?
                        .clone();
                    let mut captures = BTreeMap::new();
                    for name in names {
                        let source = state
                            .names
                            .get(&name)
                            .and_then(|stack| stack.last())
                            .ok_or_else(|| {
                                failure("original cleanup capture has no live creator binder")
                            })?;
                        let mut value = self.value_copy(source, depth + 1)?;
                        let ty = self.capture_type(owner, expr.id, &name, child, depth + 1)?;
                        let moved = *move_capture || self.inferred_capture_move(child.id, &name, depth + 1)?;
                        if moved {
                            if !cleanup::copy(
                                self.program,
                                self.proof,
                                owner,
                                &ty,
                                self.limits,
                                self.left,
                            )? {
                                *state
                                    .names
                                    .get_mut(&name)
                                    .and_then(|stack| stack.last_mut())
                                    .ok_or_else(|| {
                                        failure("original cleanup capture lost its creator binder")
                                    })? = Value::Plain;
                            }
                        } else {
                            value = self.view(value, depth + 1)?;
                        }
                        self.pay(name.len().saturating_add(1), depth)?;
                        captures.insert(name, value);
                    }
                    Ok(Value::Closure {
                        definition: child.id,
                        captures,
                    })
                }
                CoreExprKind::Bool(value) => Ok(Value::Bool(*value)),
                CoreExprKind::Literal(Constant::Bool(value)) => Ok(Value::Bool(*value)),
                CoreExprKind::Literal(_)
                | CoreExprKind::Integer(_)
                | CoreExprKind::Float(_)
                | CoreExprKind::String(_)
                | CoreExprKind::Character(_)
                | CoreExprKind::Unit => Ok(Value::Plain),
                CoreExprKind::CompileTime(_)
                | CoreExprKind::CompileTimeBlock(_)
                | CoreExprKind::SyntaxQuote { .. } => Ok(Value::Plain),
                _ => Err(pending(
                    "original protocol operation requires its complete source evaluation rule",
                )),
            }
        })();
        self.record_cost("expr_inner", cost_before, cost_children);
        result
    }
    fn restore(&mut self, state: &mut State, old: &[(String, usize)], depth: usize) -> R<()> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<()> = (|| {
            self.pay(
                old.iter().fold(state.names.len(), |w, (n, _)| {
                    w.saturating_add(n.len()).saturating_add(1)
                }),
                depth,
            )?;
            for (name, len) in old.iter().rev() {
                if let Some(values) = state.names.get(name) {
                    for value in values.iter().skip(*len) {
                        self.drop_value(value, state, depth + 1)?;
                    }
                }
                if *len == 0 {
                    state.names.remove(name);
                } else if let Some(values) = state.names.get_mut(name) {
                    values.truncate(*len);
                }
            }
            Ok(())
        })();
        self.record_cost("restore", cost_before, cost_children);
        result
    }
    fn condition(&mut self, value: &Value, truth: bool, state: &mut State, depth: usize) -> R<()> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<()> = (|| {
            self.pay(value.work().saturating_add(state.roots.len()), depth)?;
            if matches!(value,Value::Bool(actual) if *actual != truth) {
                state.reachable = false;
            }
            if let Value::Valid {
                root,
                fresh: true,
                negated,
            } = value
            {
                if truth == *negated {
                    let phase = state.roots.get_mut(root).ok_or_else(|| {
                        failure("validity predicate lost its exact original owning root")
                    })?;
                    *phase &= CLOSED;
                }
            }
            Ok(())
        })();
        self.record_cost("condition", cost_before, cost_children);
        result
    }
    fn merge(&mut self, a: &State, b: &State, depth: usize) -> R<State> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<State> = (|| {
            self.pay(a.work().saturating_add(b.work()), depth)?;
            if !a.reachable {
                return self.copy(b, depth + 1);
            }
            if !b.reachable {
                return self.copy(a, depth + 1);
            }
            self.charge_state(a, depth + 1)?;
            self.charge_state(b, depth + 1)?;
            let mut output = a.clone();
            self.pay(b.tasks.len(), depth)?;
            output.tasks.extend(b.tasks.iter().copied());
            self.pay(a.task_sync.values().chain(b.task_sync.values()).fold(0usize,
                |work, requirements| work.saturating_add(requirements.len()).saturating_add(1)), depth)?;
            output.task_sync.retain(|root, requirements| {
                if let Some(other) = b.task_sync.get(root) {
                    requirements.extend(other.iter().copied());
                    true
                } else {
                    false
                }
            });
            for exit in &b.abrupt {
                let exit = self.copy(exit, depth + 1)?;
                self.offer(&mut output.abrupt, exit, depth + 1)?;
            }
            for (root, phase) in &b.roots {
                *output.roots.entry(*root).or_default() |= *phase;
            }
            output.task_sources = self.task_join_sources(
                a.task_sources.as_ref(),
                b.task_sources.as_ref(),
                depth + 1,
            )?;
            for (name, values) in &mut output.names {
                if let Some(other) = b.names.get(name) {
                    if values.len() != other.len() {
                        return Err(failure("protocol lexical merge changes binder scope"));
                    }
                    for (value, other) in values.iter_mut().zip(other) {
                        if value != other {
                            let old = self.value_copy(value, depth + 1)?;
                            let incoming = self.value_copy(other, depth + 1)?;
                            *value = self.choice(vec![old, incoming], depth + 1)?;
                        }
                    }
                } else {
                    return Err(failure(
                        "original cleanup merge changes the live lexical binder directory",
                    ));
                }
            }
            if output.deferred.len() != b.deferred.len() || output.temporaries.len() != b.temporaries.len() {
                return Err(failure("original join changes the register or preparation stack"));
            }
            for (value, other) in output.temporaries.iter_mut().zip(&b.temporaries)
                .chain(output.deferred.iter_mut().zip(&b.deferred)) {
                if value != other {
                    let old = self.value_copy(value, depth + 1)?;
                    let incoming = self.value_copy(other, depth + 1)?;
                    *value = self.choice(vec![old, incoming], depth + 1)?;
                }
            }
            Ok(output)
        })();
        self.record_cost("merge", cost_before, cost_children);
        result
    }
    fn pattern(
        &mut self,
        pattern: &SemanticOriginalPattern,
        value: Value,
        state: &mut State,
        depth: usize,
    ) -> R<()> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<()> = (|| {
            self.pay(value.work().saturating_add(1), depth)?;
            // Bind every reachable projection of a nested choice with the same
            // rule as the outer match. A name takes the whole chosen value.
            if !matches!(pattern.kind, CorePatternKind::Name(_) | CorePatternKind::Wildcard)
                && let Value::Choice(values) = value
            {
                let mut selected = None;
                for alternative in values {
                    if self.matches(pattern, &alternative, depth + 1)? == Some(false) {
                        continue;
                    }
                    let mut branch = self.copy(state, depth + 1)?;
                    self.pattern(pattern, alternative, &mut branch, depth + 1)?;
                    if let Some(previous) = selected.take() {
                        selected = Some(self.merge(&previous, &branch, depth + 1)?);
                    } else {
                        selected = Some(branch);
                    }
                }
                *state = selected.ok_or_else(|| failure("original pattern has no reachable payload for its selected tag"))?;
                return Ok(());
            }
            match &pattern.kind {
                CorePatternKind::Name(name) => {
                    // Pattern constructors not yet represented by an exact
                    // source projection must erase any prior loop iteration's
                    // projected binder relation, never replay its old source.
                    if let Some(bindings) = &mut state.task_sources {
                        self.pay(bindings.len(), depth)?;
                        for (query, source) in bindings {
                            if matches!(query, Query::BindingProjection { .. }) {
                                *source = None;
                            }
                        }
                    }
                    self.pay(name.len().saturating_add(1), depth)?;
                    state.names.entry(name.clone()).or_default().push(value);
                }
                CorePatternKind::Wildcard
                | CorePatternKind::Integer(_)
                | CorePatternKind::Bool(_)
                | CorePatternKind::Float(_)
                | CorePatternKind::String(_)
                | CorePatternKind::Character(_) => self.drop_value(&value, state, depth + 1)?,
                CorePatternKind::Reference { pattern, .. } => {
                    let value = self.view(value, depth + 1)?;
                    self.pattern(pattern, value, state, depth + 1)?;
                }
                CorePatternKind::Tuple(patterns) => {
                    let values = match value {
                        Value::Tuple(values) => values,
                        Value::Fields(mut fields) => {
                            self.pay(patterns.len().saturating_mul(32), depth)?;
                            let mut values = Vec::with_capacity(patterns.len());
                            for index in 0..patterns.len() {
                                values.push(fields.remove(&index.to_string()).ok_or_else(
                                    || {
                                        failure(
                                            "cleanup tuple pattern omits a complete original field",
                                        )
                                    },
                                )?);
                            }
                            if !fields.is_empty() {
                                return Err(failure(
                                    "cleanup tuple pattern changes original arity",
                                ));
                            }
                            values
                        }
                        Value::Plain => {
                            self.pay(patterns.len(), depth)?;
                            vec![Value::Plain; patterns.len()]
                        }
                        _ => {
                            return Err(pending(
                                "original cleanup tuple pattern lacks its complete owning inventory",
                            ));
                        }
                    };
                    if values.len() != patterns.len() {
                        return Err(failure(
                            "original cleanup tuple pattern changes original arity",
                        ));
                    }
                    for (pattern, value) in patterns.iter().zip(values) {
                        self.pattern(pattern, value, state, depth + 1)?;
                    }
                }
                CorePatternKind::Variant {
                    variant, fields, ..
                }
                | CorePatternKind::Constructor {
                    variant, fields, ..
                } => {
                    let values = match value {
                        Value::Variant {
                            name,
                            fields: values,
                        } => {
                            if name != *variant {
                                return Err(failure(
                                    "original cleanup binds an inactive known constructor arm",
                                ));
                            }
                            values
                        }
                        Value::Cleanup { owner, ty, source } => {
                            let fields = cleanup::variant(
                            self.program,
                            self.proof,
                            owner,
                            &ty,
                            variant,
                            self.limits,
                            self.left,
                        )?
                        .ok_or_else(|| {
                            pending(
                                "original cleanup variant lacks its complete source nominal scheme",
                            )
                        })?;
                            let mut values = Vec::with_capacity(fields.len());
                            for (index, ty) in fields.into_iter().enumerate() {
                                let mut value = self.typed(owner, &ty, Value::Plain, depth + 1)?;
                                if let Some(source) = source {
                                    self.variant_input_sources(&mut value, source, variant, index, depth + 1)?;
                                }
                                values.push(value);
                            }
                            values
                        }
                        Value::Plain | Value::Poll { .. } => {
                            self.pay(fields.len(), depth)?;
                            vec![Value::Plain; fields.len()]
                        }
                        _ => {
                            return Err(pending(
                                "original cleanup constructor pattern lacks its exact owning payload inventory",
                            ));
                        }
                    };
                    if values.len() != fields.len() {
                        return Err(failure(
                            "original cleanup constructor pattern changes source field arity",
                        ));
                    }
                    for (pattern, value) in fields.iter().zip(values) {
                        self.pattern(pattern, value, state, depth + 1)?;
                    }
                }
                CorePatternKind::Record { fields, .. }
                | CorePatternKind::RecordConstructor { fields, .. } => {
                    let mut values = match value {
                        Value::Fields(values) => values,
                        Value::Plain => BTreeMap::new(),
                        _ => {
                            return Err(pending(
                                "original cleanup record pattern lacks its complete physical fields",
                            ));
                        }
                    };
                    for (name, pattern) in fields {
                        self.pay(name.len().saturating_add(values.len()), depth)?;
                        self.pattern(
                            pattern,
                            values.remove(name).unwrap_or(Value::Plain),
                            state,
                            depth + 1,
                        )?;
                    }
                    for value in values.values() {
                        self.drop_value(value, state, depth + 1)?;
                    }
                }
                CorePatternKind::ErrorArm { is_error, pattern } => {
                    let payload = match value {
                        Value::Error {
                            is_error: actual,
                            payload,
                        } if *is_error == actual => *payload,
                        Value::Plain => Value::Plain,
                        _ => {
                            return Err(pending(
                                "original cleanup error pattern lacks its exact owning arm",
                            ));
                        }
                    };
                    self.pattern(pattern, payload, state, depth + 1)?;
                }
            }
            Ok(())
        })();
        self.record_cost("pattern", cost_before, cost_children);
        result
    }
    fn matches(
        &mut self,
        pattern: &SemanticOriginalPattern,
        value: &Value,
        depth: usize,
    ) -> R<Option<bool>> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<Option<bool>> = (|| {
            self.pay(1, depth)?;
            Ok(match (&pattern.kind, value) {
                (CorePatternKind::Name(_) | CorePatternKind::Wildcard, _) => Some(true),
                (_, Value::Choice(alternatives)) => {
                    self.pay(alternatives.len(), depth)?;
                    let mut common = None;
                    for alternative in alternatives {
                        let Some(member) = self.matches(pattern, alternative, depth + 1)? else {
                            return Ok(None);
                        };
                        if common.is_some_and(|other| other != member) { return Ok(None); }
                        common = Some(member);
                    }
                    common
                }
                (CorePatternKind::Reference { pattern, .. }, value) => {
                    return self.matches(pattern, value, depth + 1);
                }
                (
                    CorePatternKind::Variant { variant, fields: patterns, .. }
                    | CorePatternKind::Constructor { variant, fields: patterns, .. },
                    Value::Variant { name, fields },
                ) => {
                    if variant != name { return Ok(Some(false)); }
                    self.product_matches(patterns, fields, depth + 1)?
                }
                (CorePatternKind::Tuple(patterns), Value::Tuple(fields)) =>
                    self.product_matches(patterns, fields, depth + 1)?,
                (
                    CorePatternKind::ErrorArm { is_error, pattern: payload },
                    Value::Error { is_error: actual, payload: value },
                ) => {
                    if is_error != actual { return Ok(Some(false)); }
                    self.matches(payload, value, depth + 1)?
                },
                (CorePatternKind::Bool(expected), Value::Bool(actual)) => Some(expected == actual),
                _ => None,
            })
        })();
        self.record_cost("matches", cost_before, cost_children);
        result
    }
    fn product_matches(&mut self, patterns: &[SemanticOriginalPattern], fields: &[Value], depth: usize) -> R<Option<bool>> {
        self.pay(patterns.len().saturating_add(1), depth)?;
        if patterns.len() != fields.len() {
            return Err(failure("original constructor pattern changes source field arity"));
        }
        let mut result = Some(true);
        for (pattern, field) in patterns.iter().zip(fields) {
            match self.matches(pattern, field, depth + 1)? {
                Some(false) => return Ok(Some(false)),
                None => result = None,
                Some(true) => {}
            }
        }
        Ok(result)
    }
    fn selected(
        &mut self,
        owner: SemanticDefinitionId,
        pattern: &SemanticOriginalPattern,
        value: &Value,
        state: &State,
        depth: usize,
    ) -> R<Vec<State>> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<Vec<State>> = (|| {
            self.pay(value.work(), depth)?;
            if let Value::Choice(values) = value {
                let mut out = Vec::new();
                for value in values {
                    out.extend(self.selected(owner, pattern, value, state, depth + 1)?);
                }
                return Ok(out);
            }
            if self.matches(pattern, value, depth + 1)? == Some(false) {
                return Ok(Vec::new());
            }
            if let Value::Poll { fresh, outcomes } = value {
                if !fresh {
                    // An old Copy discriminator remains an ordinary value; it
                    // supplies no state fact about the current root generation.
                    let mut branch = self.copy(state, depth)?;
                    self.pattern(pattern, Value::Plain, &mut branch, depth + 1)?;
                    return Ok(vec![branch]);
                }
                let variant = match &pattern.kind {
                    CorePatternKind::Variant { variant, .. }
                    | CorePatternKind::Constructor { variant, .. } => Some(variant.as_str()),
                    CorePatternKind::Name(_) | CorePatternKind::Wildcard => None,
                    _ => {
                        return Err(failure(
                            "protocol Poll tag selection is not a nominal constructor pattern",
                        ));
                    }
                };
                // Γ already proved the exact subject nominal and variant. These
                // spellings select its closed Poll arms; they grant no type identity.
                let wanted = match variant {
                    Some("Pending") => Some(ProtocolExit::PollPending),
                    Some("Ready") => Some(ProtocolExit::PollReady),
                    None => None,
                    _ => {
                        return Err(failure(
                            "original Poll pattern selects a nonexistent sealed arm",
                        ));
                    }
                };
                let mut out = Vec::new();
                for outcome in outcomes {
                    if wanted.is_some_and(|w| w != outcome.exit) {
                        continue;
                    }
                    let mut branch = self.copy(state, depth)?;
                    for (root, phase) in &outcome.phases {
                        if !branch.roots.contains_key(root) {
                            return Err(failure("Poll refinement lost its original root"));
                        }
                        branch.roots.insert(*root, *phase);
                    }
                    let bound = self.value_copy(value, depth)?;
                    self.pattern(pattern, bound, &mut branch, depth + 1)?;
                    out.push(branch);
                }
                Ok(out)
            } else {
                let mut state = self.copy(state, depth)?;
                let value = self.value_copy(value, depth)?;
                self.pattern(pattern, value, &mut state, depth + 1)?;
                let _ = owner;
                Ok(vec![state])
            }
        })();
        self.record_cost("selected", cost_before, cost_children);
        result
    }
    fn offer(&mut self, seen: &mut Vec<State>, state: State, depth: usize) -> R<bool> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<bool> = (|| {
            self.charge_state(&state, depth + 1)?;
            for old in seen.iter() {
                self.charge_state(old, depth + 1)?;
            }
            for old in seen.iter() {
                self.pay(old.work().saturating_add(state.work()), depth)?;
                if old == &state {
                    return Ok(false);
                }
            }
            self.pay(state.work().saturating_add(1), depth)?;
            seen.push(state);
            Ok(true)
        })();
        self.record_cost("offer", cost_before, cost_children);
        result
    }
    fn unselected(
        &mut self,
        pattern: &SemanticOriginalPattern,
        value: &Value,
        state: &State,
        depth: usize,
    ) -> R<Vec<State>> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<Vec<State>> = (|| {
            self.pay(value.work(), depth)?;
            if let Value::Choice(values) = value {
                let mut out = Vec::new();
                for value in values {
                    out.extend(self.unselected(pattern, value, state, depth + 1)?)
                }
                return Ok(out);
            }
            if self.matches(pattern, value, depth + 1)? == Some(true) {
                return Ok(Vec::new());
            }
            if let Value::Poll {
                fresh: true,
                outcomes,
            } = value
            {
                let wanted = match &pattern.kind {
                    CorePatternKind::Variant { variant, .. }
                    | CorePatternKind::Constructor { variant, .. } => match variant.as_str() {
                        "Pending" => ProtocolExit::PollPending,
                        "Ready" => ProtocolExit::PollReady,
                        _ => {
                            return Err(failure(
                                "original Poll rejection selects an absent sealed arm",
                            ));
                        }
                    },
                    _ => {
                        return Err(pending(
                            "original Poll rejection requires its exact constructor pattern",
                        ));
                    }
                };
                let mut out = Vec::new();
                for outcome in outcomes {
                    if outcome.exit == wanted {
                        continue;
                    }
                    let mut branch = self.copy(state, depth + 1)?;
                    self.pay(outcome.phases.len(), depth)?;
                    for (root, phase) in &outcome.phases {
                        if !branch.roots.contains_key(root) {
                            return Err(failure("Poll rejection lost its original owner root"));
                        }
                        branch.roots.insert(*root, *phase);
                    }
                    out.push(branch);
                }
                return Ok(out);
            }
            let branch = self.copy(state, depth + 1)?;
            // A failed pattern consumes no binding, but the evaluated temporary's
            // unmatched payload still has to be cleaned on this exact edge.
            self.drop_value(value, &branch, depth + 1)?;
            Ok(vec![branch])
        })();
        self.record_cost("unselected", cost_before, cost_children);
        result
    }
    fn loop_body(
        &mut self,
        owner: SemanticDefinitionId,
        body: &SemanticOriginalBlock,
        condition: Option<&SemanticOriginalExpr>,
        pattern: Option<&SemanticOriginalPattern>,
        may_exit: bool,
        initial: State,
        depth: usize,
    ) -> R<Vec<State>> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<Vec<State>> = (|| {
            let mut seen = Vec::new();
            let mut queue = std::collections::VecDeque::new();
            let mut exits = Vec::new();
            let saved = self.copy(&initial, depth)?;
            self.offer(&mut seen, saved, depth)?;
            queue.push_back(initial);
            while let Some(mut state) = queue.pop_front() {
                self.pay(1, depth)?;
                let scope = self.scope(&state, depth)?;
                let mut entries = Vec::new();
                if let Some(condition) = condition {
                    let value = self.expr(owner, condition, &mut state, depth + 1)?;
                    // Both condition outcomes are real source paths. A Poll
                    // constructor pattern refines only its own matching edge.
                    if let Some(pattern) = pattern {
                        exits.extend(self.unselected(pattern, &value, &state, depth + 1)?);
                        entries = self.selected(owner, pattern, &value, &state, depth + 1)?;
                    } else {
                        let mut exit = self.copy(&state, depth)?;
                        self.condition(&value, false, &mut exit, depth)?;
                        if exit.reachable {
                            exits.push(exit)
                        }
                        self.condition(&value, true, &mut state, depth)?;
                        if state.reachable {
                            entries.push(state);
                        }
                    }
                } else {
                    if may_exit {
                        exits.push(self.copy(&state, depth)?);
                    }
                    entries.push(state);
                }
                for entry in entries {
                    for mut next in self.block(owner, body, entry, depth + 1)? {
                        self.leave(&mut next, &scope, depth)?;
                        match next.control {
                            Control::Break => {
                                next.control = Control::Next;
                                exits.push(next);
                            }
                            Control::Return => exits.push(next),
                            Control::Next | Control::Continue => {
                                next.control = Control::Next;
                                // No fresh κ, epoch counter, or SSA equality is
                                // created at a backedge. Existing stale aliases stay
                                // stale; repeated complete states are the fixed point.
                                let saved = self.copy(&next, depth)?;
                                if self.offer(&mut seen, saved, depth)? {
                                    queue.push_back(next);
                                }
                            }
                        }
                    }
                }
            }
            Ok(exits)
        })();
        self.record_cost("loop_body", cost_before, cost_children);
        result
    }
    fn present(&mut self, expected: &Value, actual: &Value, depth: usize) -> R<bool> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<bool> = (|| {
            self.pay(expected.work().saturating_add(actual.work()), depth)?;
            self.charge_value(expected, depth + 1)?;
            self.charge_value(actual, depth + 1)?;
            if let Value::Choice(values) = actual {
                for value in values {
                    if !self.present(expected, value, depth + 1)? {
                        return Ok(false);
                    }
                }
                return Ok(!values.is_empty());
            }
            Ok(match (expected, actual) {
                (Value::Plain, _) | (Value::Bool(_), _) | (Value::Root(_, _, false, _), _) => true,
                (Value::Cleanup { .. }, Value::Plain) => false,
                (Value::Cleanup { .. }, _) => true,
                (Value::Root(_, _, true, _), Value::Root(_, true, true, _)) => true,
                (Value::Root(_, _, true, _), _) => false,
                (Value::Tuple(expected), Value::Tuple(actual))
                    if expected.len() == actual.len() =>
                {
                    for (expected, actual) in expected.iter().zip(actual) {
                        if !self.present(expected, actual, depth + 1)? {
                            return Ok(false);
                        }
                    }
                    true
                }
                (Value::Fields(expected), Value::Fields(actual))
                | (
                    Value::Closure {
                        captures: expected, ..
                    },
                    Value::Closure {
                        captures: actual, ..
                    },
                ) if expected.len() == actual.len() => {
                    for (name, expected) in expected {
                        let Some(actual) = actual.get(name) else {
                            return Ok(false);
                        };
                        if !self.present(expected, actual, depth + 1)? {
                            return Ok(false);
                        }
                    }
                    true
                }
                (Value::Boxed(_), Value::Boxed(_)) => true,
                (Value::Error { .. }, Value::Error { .. })
                | (Value::Variant { .. }, Value::Variant { .. }) => true,
                (Value::Choice(values), _) => {
                    for expected in values {
                        if self.present(expected, actual, depth + 1)? {
                            return Ok(true);
                        }
                    }
                    false
                }
                (Value::Valid { .. } | Value::Poll { .. }, _) => true,
                _ => false,
            })
        })();
        self.record_cost("present", cost_before, cost_children);
        result
    }
    fn pattern_query(
        &mut self,
        owner: SemanticDefinitionId,
        statement: NodeId,
        pattern: &SemanticOriginalPattern,
        path: &mut Vec<u32>,
        state: &mut State,
        depth: usize,
    ) -> R<()> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<()> = (|| {
            self.pay(path.len().saturating_add(1), depth)?;
            match &pattern.kind {
                CorePatternKind::Name(name) => {
                    let ty = self
                    .query_type(
                        owner,
                        Query::BindingProjection {
                            statement,
                            pattern: path.clone(),
                        },
                        depth + 1,
                    )?
                    .ok_or_else(|| {
                        pending(
                            "original cleanup iteration binding lacks its final Gamma leaf type",
                        )
                    })?;
                    let value = self.typed(owner, &ty, Value::Plain, depth + 1)?;
                    self.pay(name.len().saturating_add(1), depth)?;
                    state.names.entry(name.clone()).or_default().push(value);
                }
                CorePatternKind::Tuple(fields)
                | CorePatternKind::Variant { fields, .. }
                | CorePatternKind::Constructor { fields, .. } => {
                    for (index, field) in fields.iter().enumerate() {
                        path.push(index as u32);
                        self.pattern_query(owner, statement, field, path, state, depth + 1)?;
                        path.pop();
                    }
                }
                CorePatternKind::Record { fields, .. }
                | CorePatternKind::RecordConstructor { fields, .. } => {
                    for (index, (_, field)) in fields.iter().enumerate() {
                        path.push(index as u32);
                        self.pattern_query(owner, statement, field, path, state, depth + 1)?;
                        path.pop();
                    }
                }
                CorePatternKind::Reference { pattern, .. }
                | CorePatternKind::ErrorArm { pattern, .. } => {
                    path.push(0);
                    self.pattern_query(owner, statement, pattern, path, state, depth + 1)?;
                    path.pop();
                }
                _ => {}
            }
            Ok(())
        })();
        self.record_cost("pattern_query", cost_before, cost_children);
        result
    }
    fn iteration(
        &mut self,
        owner: SemanticDefinitionId,
        statement: NodeId,
        pattern: &SemanticOriginalPattern,
        body: &SemanticOriginalBlock,
        initial: State,
        depth: usize,
    ) -> R<Vec<State>> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<Vec<State>> = (|| {
            let mut seen = Vec::new();
            let mut queue = std::collections::VecDeque::new();
            let mut exits = Vec::new();
            let initial_copy = self.copy(&initial, depth + 1)?;
            self.offer(&mut seen, initial_copy, depth + 1)?;
            queue.push_back(initial);
            while let Some(mut state) = queue.pop_front() {
                self.pay(1, depth)?;
                let scope = self.scope(&state, depth + 1)?;
                exits.push(self.copy(&state, depth + 1)?);
                self.pattern_query(
                    owner,
                    statement,
                    pattern,
                    &mut Vec::new(),
                    &mut state,
                    depth + 1,
                )?;
                for mut next in self.block(owner, body, state, depth + 1)? {
                    self.leave(&mut next, &scope, depth + 1)?;
                    match next.control {
                        Control::Break => {
                            next.control = Control::Next;
                            exits.push(next);
                        }
                        Control::Return => exits.push(next),
                        Control::Next | Control::Continue => {
                            next.control = Control::Next;
                            let saved = self.copy(&next, depth + 1)?;
                            if self.offer(&mut seen, saved, depth + 1)? {
                                queue.push_back(next);
                            }
                        }
                    }
                }
            }
            Ok(exits)
        })();
        self.record_cost("iteration", cost_before, cost_children);
        result
    }
    fn scope(&mut self, state: &State, depth: usize) -> R<Vec<(String, usize)>> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<Vec<(String, usize)>> = (|| {
            self.pay(
                state
                    .names
                    .keys()
                    .fold(0usize, |w, n| w.saturating_add(n.len()).saturating_add(1)),
                depth,
            )?;
            Ok(state
                .names
                .iter()
                .map(|(name, values)| (name.clone(), values.len()))
                .collect())
        })();
        self.record_cost("scope", cost_before, cost_children);
        result
    }
    fn leave(&mut self, state: &mut State, scope: &[(String, usize)], depth: usize) -> R<()> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<()> = (|| {
            let mut scope = scope.iter().peekable();
            let mut changes = Vec::new();
            self.pay(state.names.len(), depth)?;
            // Both inventories have the exact BTree ordering from State::names.
            // Check every exiting value against the original, unmodified state.
            for (index, (name, values)) in state.names.iter().enumerate() {
                let keep = loop {
                    let Some((original, keep)) = scope.peek() else { break None; };
                    self.pay(name.len().min(original.len()).saturating_add(1), depth)?;
                    match original.cmp(name) {
                        std::cmp::Ordering::Less => { scope.next(); }
                        std::cmp::Ordering::Equal => { let keep = *keep; scope.next(); break Some(keep); }
                        std::cmp::Ordering::Greater => break None,
                    }
                };
                for value in values.iter().skip(keep.unwrap_or(0)) {
                    self.drop_value(value, state, depth + 1)
                        .map_err(|mut diagnostic| {
                            diagnostic.notes.push(format!(
                                "original scope cleanup binding {:?}, retained lexical depth {}",
                                name, keep.unwrap_or(0)
                            ));
                            diagnostic
                        })?;
                }
                if keep.is_none_or(|keep| keep < values.len()) {
                    self.pay(1, depth)?;
                    changes.push((index, keep));
                }
            }
            if !changes.is_empty() {
                self.pay(state.names.len(), depth)?;
                let mut changes = changes.into_iter().peekable();
                let mut index = 0;
                state.names.retain(|_, values| {
                    let previous = index;
                    index += 1;
                    match changes.next_if(|(ordinal, _)| *ordinal == previous) {
                        Some((_, Some(keep))) => { values.truncate(keep); true }
                        Some((_, None)) => false,
                        None => true,
                    }
                });
            }
            Ok(())
        })();
        self.record_cost("leave", cost_before, cost_children);
        result
    }
    fn hide_for_defer(
        &mut self,
        state: &mut State,
        depth: usize,
    ) -> R<(Vec<(String, usize)>, usize)> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<(Vec<(String, usize)>, usize)> = (|| {
            self.pay(state.work(), depth)?;
            let start = state.temporaries.len();
            let mut hidden = Vec::with_capacity(state.names.len());
            for (name, values) in std::mem::take(&mut state.names) {
                hidden.push((name, values.len()));
                state.temporaries.extend(values);
            }
            Ok((hidden, start))
        })();
        self.record_cost("hide_for_defer", cost_before, cost_children);
        result
    }
    fn restore_defer(
        &mut self,
        state: &mut State,
        hidden: &[(String, usize)],
        start: usize,
        depth: usize,
    ) -> R<()> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<()> = (|| {
            self.pay(
                hidden.iter().fold(hidden.len(), |n, (name, count)| {
                    n.saturating_add(name.len()).saturating_add(*count)
                }),
                depth,
            )?;
            let mut values = state
                .temporaries
                .drain(start..)
                .collect::<Vec<_>>()
                .into_iter();
            for (name, count) in hidden {
                let target = state.names.entry(name.clone()).or_default();
                for _ in 0..*count {
                    target.push(values.next().ok_or_else(|| {
                        failure("defer lost a hidden original binding responsibility")
                    })?);
                }
            }
            if values.next().is_some() {
                return Err(failure(
                    "defer restoration changes the pending temporary inventory",
                ));
            }
            Ok(())
        })();
        self.record_cost("restore_defer", cost_before, cost_children);
        result
    }
    fn invoke_deferred(
        &mut self,
        child: SemanticDefinitionId,
        body: &SemanticOriginalBlock,
        action: Value,
        mut state: State,
        depth: usize,
    ) -> R<Vec<State>> {
        self.pay(action.work(), depth)?;
        if let Value::Choice(alternatives) = action {
            let mut outputs = Vec::new();
            for alternative in alternatives {
                let branch = self.copy(&state, depth + 1)?;
                outputs.extend(self.invoke_deferred(child, body, alternative, branch, depth + 1)?);
            }
            return Ok(outputs);
        }
        let Value::Closure { definition, captures } = action else {
            return Err(failure("scope exit operand is not the registered cleanup environment"));
        };
        if definition != child {
            return Err(failure("scope exit changes the declaration of the registered cleanup"));
        }
        let (hidden, start) = self.hide_for_defer(&mut state, depth + 1)?;
        let caller = std::mem::replace(&mut state.owner, definition);
        let caller_sources = state.task_sources.take();
        for (name, value) in captures {
            state.names.insert(name, vec![value]);
        }
        state.control = Control::Next;
        let mut exits = self.block(definition, body, state, depth + 1)?;
        for exit in &mut exits {
            if exit.control != Control::Next {
                return Err(failure("original cleanup escapes its lexical exit"));
            }
            self.leave(exit, &[], depth + 1)?;
            self.restore_defer(exit, &hidden, start, depth + 1)?;
            exit.owner = caller;
            self.pay(source_binding_work(caller_sources.as_ref()), depth)?;
            exit.task_sources = caller_sources.clone();
        }
        Ok(exits)
    }
    fn cleanup(
        &mut self,
        state: State,
        defers: &[(SemanticDefinitionId, &SemanticOriginalBlock)],
        scope: &[(String, usize)],
        depth: usize,
    ) -> R<Vec<State>> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<Vec<State>> = (|| {
            self.pay(state.roots.len(), depth)?;
            if !state.reachable || state.roots.values().any(|phase| *phase == 0) {
                return Ok(Vec::new());
            }
            let control = state.control;
            let mut states = vec![state];
            for &(child, body) in defers.iter().rev() {
                let mut next = Vec::new();
                for mut state in states {
                    let action = state.deferred.pop().ok_or_else(||
                        failure("scope exit has no matching registered cleanup environment"))?;
                    next.extend(self.invoke_deferred(child, body, action, state, depth + 1)?);
                }
                states = next;
            }
            for state in &mut states {
                state.control = control;
                self.leave(state, scope, depth)?;
            }
            Ok(states)
        })();
        self.record_cost("cleanup", cost_before, cost_children);
        result
    }
    fn block(
        &mut self,
        owner: SemanticDefinitionId,
        body: &SemanticOriginalBlock,
        state: State,
        depth: usize,
    ) -> R<Vec<State>> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<Vec<State>> = (|| {
            self.pay(
                body.statements
                    .len()
                    .saturating_add(state.roots.len())
                    .saturating_add(1),
                depth,
            )?;
            if !state.reachable || state.roots.values().any(|phase| *phase == 0) {
                return Ok(Vec::new());
            }
            let scope = self.scope(&state, depth)?;
            let mut active = vec![state];
            let mut ended = Vec::new();
            let mut defers = Vec::new();
            for statement in &body.statements {
                if active.is_empty() {
                    break;
                }
                if let CoreStatementKind::Defer(cleanup) = &statement.kind {
                    self.pay(1, depth)?;
                    let (action, result, body) = cleanup.deferred_action().ok_or_else(||
                        failure("defer registration lacks its actual lexical closure operand"))?;
                    if result != &SemanticTypeRecipe::Atom(Type::Unit) {
                        return Err(failure("defer action must return Unit"));
                    }
                    let child = self.closure_child(owner, action.id, depth + 1)?.id;
                    for state in &mut active {
                        let value = self.expr(owner, action, state, depth + 1)?;
                        state.deferred.push(value);
                    }
                    defers.push((child, body));
                    continue;
                }
                let mut next = Vec::new();
                for state in active {
                    for mut state in self.statement(owner, statement, state, depth + 1).map_err(
                        |mut diagnostic| {
                            diagnostic.notes.push(format!(
                            "original statement owner {:?}, node {:?}, span {:?}, operation {:?}",
                            owner,
                            statement.id,
                            statement.span,
                            std::mem::discriminant(&statement.kind)
                        ));
                            crate::semantics::locate_original(diagnostic, statement.span)
                        },
                    )? {
                        self.pay(1, depth)?;
                        for exit in std::mem::take(&mut state.abrupt) {
                            ended.extend(self.cleanup(exit, &defers, &scope, depth + 1)?);
                        }
                        if !state.reachable {
                            continue;
                        }
                        if state.control == Control::Next {
                            next.push(state);
                        } else {
                            ended.extend(self.cleanup(state, &defers, &scope, depth + 1)?);
                        }
                    }
                }
                active = next;
            }
            for state in active {
                ended.extend(self.cleanup(state, &defers, &scope, depth + 1)?);
            }
            Ok(ended)
        })();
        self.record_cost("block", cost_before, cost_children);
        result
    }
    fn statement(
        &mut self,
        owner: SemanticDefinitionId,
        statement: &SemanticOriginalStatement,
        mut state: State,
        depth: usize,
    ) -> R<Vec<State>> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let result: R<Vec<State>> = (|| {
            self.pay(state.roots.len().saturating_add(1), depth)?;
            if !state.reachable || state.roots.values().any(|phase| *phase == 0) {
                return Ok(Vec::new());
            }
            match &statement.kind {
                CoreStatementKind::Binding {
                    name, initializer, ..
                } => {
                    let value = if let Some(expr) = initializer {
                        self.expr(owner, expr, &mut state, depth + 1)?
                    } else {
                        Value::Plain
                    };
                    let value = if initializer.is_some() {
                        let ty=self.query_type(owner,Query::Binding(statement.id),depth+1)?.ok_or_else(||failure("original cleanup initialized binding has no final solved Gamma type"))?;
                        self.typed(owner, &ty, value, depth + 1)?
                    } else {
                        value
                    };
                    self.pay(name.len().saturating_add(1), depth)?;
                    state.names.entry(name.clone()).or_default().push(value);
                    if let Some(bindings) = &mut state.task_sources {
                        let source = initializer
                            .as_ref()
                            .map(|expr| {
                                self.proof.task_value_sources_at(
                                    self.program,
                                    owner,
                                    expr,
                                    bindings,
                                    self.limits,
                                    self.left,
                                )
                            })
                            .transpose()?
                            .flatten();
                        self.task_set_sources(
                            bindings,
                            Query::Binding(statement.id),
                            source,
                            depth + 1,
                        )?;
                    }
                }
                CoreStatementKind::TupleBinding {
                    pattern,
                    initializer,
                    ..
                } => {
                    let value = self.expr(owner, initializer, &mut state, depth + 1)?;
                    self.pattern(pattern, value, &mut state, depth + 1)?;
                }
                CoreStatementKind::Return(value) => {
                    state.returned = if let Some(expr) = value {
                        self.expr(owner, expr, &mut state, depth + 1)?
                    } else {
                        Value::Plain
                    };
                    if let (Some(bindings), Some(expr)) = (&state.task_sources, value) {
                        let source = self.proof.task_value_sources_at(
                            self.program,
                            owner,
                            expr,
                            bindings,
                            self.limits,
                            self.left,
                        )?;
                        self.task_record_sources(owner, statement.id, source, depth + 1)?;
                    }
                    state.control = Control::Return;
                }
                CoreStatementKind::Expression(expr) => {
                    let value = self.expr(owner, expr, &mut state, depth + 1)?;
                    self.drop_value(&value, &state, depth + 1)?;
                }
                CoreStatementKind::Assign {
                    target,
                    value: expression,
                }
                | CoreStatementKind::CompoundAssign {
                    target,
                    value: expression,
                    ..
                } => {
                    let value = self.expr(owner, expression, &mut state, depth + 1)?;
                    let source = if let Some(bindings) = &state.task_sources {
                        if matches!(statement.kind, CoreStatementKind::Assign { .. }) {
                            self.proof.task_value_sources_at(
                                self.program,
                                owner,
                                expression,
                                bindings,
                                self.limits,
                                self.left,
                            )?
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    self.task_assign_sources(owner, target, source, &mut state, depth + 1)?;
                    let inserted = self.value_copy(&value, depth + 1)?;
                    if let Some(old) =
                        self.replace_place(target, &mut state, inserted, depth + 1)?
                    {
                        self.drop_value(&old, &state, depth + 1)?;
                    } else {
                        let ty=self.query_type(owner,Query::Expression(target.id),depth+1)?.ok_or_else(||pending("original cleanup external store lacks its independently solved place type"))?;
                        let old = self.typed(owner, &ty, Value::Plain, depth + 1)?;
                        self.drop_value(&old, &state, depth + 1)?;
                        let target = self.observe(owner, target, &mut state, depth + 1)?;
                        let mut roots = BTreeSet::new();
                        target.roots(&mut roots);
                        value.roots(&mut roots);
                        if !roots.is_empty() {
                            return Err(pending(
                                "original cleanup external store lacks its exact protocol owner projection",
                            ));
                        }
                    }
                }
                CoreStatementKind::Block(body) | CoreStatementKind::Unsafe(body) => {
                    return self.block(owner, body, state, depth + 1);
                }
                CoreStatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    let condition = self.expr(owner, condition, &mut state, depth + 1)?;
                    let mut right = self.copy(&state, depth)?;
                    self.condition(&condition, true, &mut state, depth)?;
                    self.condition(&condition, false, &mut right, depth)?;
                    let mut out = self.block(owner, then_body, state, depth + 1)?;
                    if let Some(body) = else_body {
                        out.extend(self.block(owner, body, right, depth + 1)?);
                    } else {
                        out.push(right);
                    }
                    return Ok(out);
                }
                CoreStatementKind::IfLet {
                    pattern,
                    value,
                    then_body,
                    else_body,
                } => {
                    let value = self.expr(owner, value, &mut state, depth + 1)?;
                    let scope = self.scope(&state, depth)?;
                    let mut out = Vec::new();
                    for branch in self.selected(owner, pattern, &value, &state, depth + 1)? {
                        for mut branch in self.block(owner, then_body, branch, depth + 1)? {
                            self.leave(&mut branch, &scope, depth)?;
                            out.push(branch);
                        }
                    }
                    for branch in self.unselected(pattern, &value, &state, depth + 1)? {
                        if let Some(body) = else_body {
                            out.extend(self.block(owner, body, branch, depth + 1)?);
                        } else {
                            out.push(branch);
                        }
                    }
                    return Ok(out);
                }
                CoreStatementKind::Match { value, arms } => {
                    let value = self.expr(owner, value, &mut state, depth + 1)?;
                    let scope = self.scope(&state, depth)?;
                    self.pay(arms.len(), depth)?;
                    let mut out = Vec::new();
                    for arm in arms {
                        for branch in
                            self.selected(owner, &arm.pattern, &value, &state, depth + 1)?
                        {
                            for mut branch in self.block(owner, &arm.body, branch, depth + 1)? {
                                self.leave(&mut branch, &scope, depth)?;
                                out.push(branch);
                            }
                        }
                    }
                    return Ok(out);
                }
                CoreStatementKind::Loop(body) => {
                    return self.loop_body(owner, body, None, None, false, state, depth + 1);
                }
                CoreStatementKind::While { condition, body } => {
                    return self.loop_body(
                        owner,
                        body,
                        Some(condition),
                        None,
                        false,
                        state,
                        depth + 1,
                    );
                }
                CoreStatementKind::WhileLet {
                    pattern,
                    value,
                    body,
                } => {
                    return self.loop_body(
                        owner,
                        body,
                        Some(value),
                        Some(pattern),
                        false,
                        state,
                        depth + 1,
                    );
                }
                CoreStatementKind::ForRange {
                    name,
                    start,
                    end,
                    body,
                } => {
                    self.expr(owner, start, &mut state, depth + 1)?;
                    self.expr(owner, end, &mut state, depth + 1)?;
                    let scope = self.scope(&state, depth + 1)?;
                    self.pay(name.len().saturating_add(1), depth)?;
                    state
                        .names
                        .entry(name.clone())
                        .or_default()
                        .push(Value::Plain);
                    let mut out =
                        self.loop_body(owner, body, None, None, true, state, depth + 1)?;
                    for state in &mut out {
                        self.leave(state, &scope, depth + 1)?;
                    }
                    return Ok(out);
                }
                CoreStatementKind::With { name, value, body } => {
                    let value = self.expr(owner, value, &mut state, depth + 1)?;
                    let scope = self.scope(&state, depth)?;
                    self.pay(name.len().saturating_add(1), depth)?;
                    state.names.entry(name.clone()).or_default().push(value);
                    let mut out = self.block(owner, body, state, depth + 1)?;
                    for state in &mut out {
                        self.leave(state, &scope, depth)?;
                    }
                    return Ok(out);
                }
                CoreStatementKind::Break => state.control = Control::Break,
                CoreStatementKind::Continue => state.control = Control::Continue,
                CoreStatementKind::CompileTime(_) => {}
                CoreStatementKind::ForEach {
                    pattern,
                    iterator,
                    body,
                } => {
                    if let CoreExprKind::Range { start, end } = &iterator.kind {
                        self.expr(owner, start, &mut state, depth + 1)?;
                        self.expr(owner, end, &mut state, depth + 1)?;
                        return self.iteration(
                            owner,
                            statement.id,
                            pattern,
                            body,
                            state,
                            depth + 1,
                        );
                    }
                    let value = self.expr(owner, iterator, &mut state, depth + 1)?;
                    self.drop_value(&value, &state, depth + 1)?;
                    self.pay(1, depth)?;
                    let slot = state.temporaries.len();
                    state.temporaries.push(value);
                    let mut out =
                        self.iteration(owner, statement.id, pattern, body, state, depth + 1)?;
                    for state in &mut out {
                        let value = state.temporaries.remove(slot);
                        self.drop_value(&value, state, depth + 1)?;
                    }
                    return Ok(out);
                }
                CoreStatementKind::Yield(_) | CoreStatementKind::Unsupported(_) => {
                    return Err(pending(
                        "original protocol control form requires its complete CFG relation",
                    ));
                }
                CoreStatementKind::Defer(_) => unreachable!(),
            }
            Ok(vec![state])
        })();
        self.record_cost("statement", cost_before, cost_children);
        result
    }
    /// Check known callees in fresh host frames before callers consume their
    /// exit summaries. An edge only schedules a body. The call itself must still
    /// bind its actual arguments and prove the protocol in the current state.
    fn function_order(&mut self) -> R<Vec<usize>> {
        let count = self.program.definitions.len();
        let edges = self.proof.call_targets.len();
        let index_cost = lookup_work(count).saturating_mul(2);
        self.pay(count.saturating_mul(6)
            .saturating_add(edges.saturating_mul(index_cost.saturating_add(4))), 0)?;
        let mut callers = vec![Vec::new(); count];
        let mut remaining = vec![0usize; count];
        let positions = &self.proof.definition_positions;
        for ((owner, _), target) in &self.proof.call_targets {
            let caller = *positions.get(owner).ok_or_else(|| failure("original call owner is absent"))?;
            let callee = *positions.get(target).ok_or_else(|| failure("original call target is absent"))?;
            callers[callee].push(caller);
            remaining[caller] += 1;
        }
        let mut order = Vec::with_capacity(count);
        order.extend((0..count).filter(|i| remaining[*i] == 0));
        let mut next = 0;
        while next < order.len() {
            let callee = order[next];
            next += 1;
            for &caller in &callers[callee] {
                remaining[caller] -= 1;
                if remaining[caller] == 0 {
                    order.push(caller);
                }
            }
        }
        // Keep the original declaration order for a recursive dependency and
        // its dependents. The active-body and fixed-point checks govern them.
        order.extend((0..count).filter(|i| remaining[*i] != 0));
        Ok(order)
    }

    fn function(&mut self, owner: SemanticDefinitionId, depth: usize) -> R<()> {
        let cost_before = *self.left;
        let cost_children = self.method_cost_total;
        let function_children = self.function_cost_total;
        let result: R<()> = (|| {
            self.pay(
                self.proof
                    .protocol_complete
                    .len()
                    .saturating_add(self.active.len())
                    .saturating_add(1),
                depth,
            )?;
            if self.proof.protocol_complete.contains(&owner)
                && self.proof.cleanup_complete.contains(&owner)
            {
                return Ok(());
            }
            if !self.proof.complete.contains(&owner) {
                return Err(failure(
                    "protocol definition lacks a complete independent original type proof",
                ));
            }
            if !self.active.insert(owner) {
                return Err(pending(
                    "recursive protocol contracts require a jointly proved conditional fixed point",
                ));
            }
            let definition = self.definition(owner)?;
            let SemanticDefinitionBody::FunctionWithBody {
                protocol_contracts,
                parameters,
                body,
                foreign,
                ..
            } = &definition.body
            else {
                self.active.remove(&owner);
                self.local_complete.insert(owner);
                if self.graph_requested.contains(&owner) { self.graph_complete.insert(owner); }
                self.proof.protocol_complete.insert(owner);
                self.proof.cleanup_complete.insert(owner);
                return Ok(());
            };
            self.pay(
                parameters.len().saturating_add(protocol_contracts.len()),
                depth,
            )?;
            if let Some(declaration) = foreign {
                self.pay(
                    declaration
                        .abi
                        .len()
                        .saturating_add(declaration.symbol.len())
                        .saturating_add(declaration.noescape_parameters.len())
                        .saturating_add(1),
                    depth,
                )?;
                if !matches!(body, SemanticOriginalFunctionBody::External)
                    || declaration.abi != "C"
                    || declaration.symbol.is_empty()
                {
                    return Err(failure(
                        "original foreign head is not its bodyless, concrete C declaration",
                    ));
                }
                let mut previous = None;
                for parameter in &declaration.noescape_parameters {
                    if *parameter as usize >= parameters.len()
                        || previous.is_some_and(|p| p >= *parameter)
                    {
                        return Err(failure(
                            "original noescape promise does not name distinct ordered real parameters",
                        ));
                    }
                    previous = Some(*parameter);
                }
            } else if matches!(body, SemanticOriginalFunctionBody::External) {
                return Err(failure(
                    "original external body has no independently produced ABI declaration",
                ));
            }
            let mut roles = BTreeSet::new();
            for contract in protocol_contracts {
                crate::registration::charge_contract(contract, |work| self.pay(work, depth))?;
                let parameter = contract.parameter as usize;
                if parameter >= parameters.len()
                    || !roles.insert(contract.parameter)
                    || contract.entry.is_empty()
                {
                    return Err(failure(
                        "original protocol table has an absent, duplicate, or empty parameter role",
                    ));
                }
                if !self.query_async(owner, Query::Parameter(contract.parameter), depth + 1)? {
                    return Err(failure(
                        "original protocol role is not a genuine AsyncRegistration parameter",
                    ));
                }
                let mut entry = BTreeSet::new();
                for input in &contract.entry {
                    if !entry.insert(*input) {
                        return Err(failure("original protocol entry state is duplicated"));
                    }
                }
                let mut exits = BTreeSet::new();
                for row in &contract.exits {
                    if !entry.contains(&row.input) || !exits.insert((row.input, row.exit)) {
                        return Err(failure(
                            "original protocol relation has an invalid input or duplicate exit",
                        ));
                    }
                    let consumed = parameters[parameter].mode == PassingMode::Give;
                    if row.output.is_empty() != consumed
                        || consumed
                            && self.query_ownership(
                                owner,
                                Query::Parameter(contract.parameter),
                                depth + 1,
                            )? != Some(true)
                    {
                        return Err(failure(
                            "original protocol retired output requires its exact Give owner; borrowed outputs retain states",
                        ));
                    }
                    let mut output = BTreeSet::new();
                    for phase in &row.output {
                        if !output.insert(*phase) {
                            return Err(failure("original protocol output state is duplicated"));
                        }
                    }
                }
            }
            let SemanticOriginalFunctionBody::Defined(body) = body else {
                if !protocol_contracts.is_empty() {
                    return Err(pending(
                        "bodyless original protocol relation has no independent implementation proof",
                    ));
                }
                self.active.remove(&owner);
                self.local_complete.insert(owner);
                if self.graph_requested.contains(&owner) { self.graph_complete.insert(owner); }
                self.proof.protocol_complete.insert(owner);
                self.proof.cleanup_complete.insert(owner);
                return Ok(());
            };
            let mut reusable_inputs_complete = true;
            let mut cases = vec![BTreeMap::new()];
            let task_sources = self.proof.task_control_sources_needed(
                self.program,
                owner,
                self.limits,
                self.left,
            )?;
            for contract in protocol_contracts {
                let mut next = Vec::new();
                for case in cases {
                    for input in &contract.entry {
                        self.pay(case.len().saturating_add(1), depth)?;
                        let mut case = case.clone();
                        case.insert(contract.parameter, *input);
                        next.push(case);
                    }
                }
                cases = next;
            }
            for inputs in cases {
                let inputs_before = *self.left;
                let inputs_children = self.method_cost_total;
                let mut state = State {
                    owner,
                    task_sources: task_sources.then(BTreeMap::new),
                    tasks: BTreeSet::new(),
                    task_sync: BTreeMap::new(),
                    reachable: true,
                    abrupt: Vec::new(),
                    names: BTreeMap::new(),
                    roots: BTreeMap::new(),
                    control: Control::Next,
                    returned: Value::Plain,
                    temporaries: Vec::new(),
                    deferred: Vec::new(),
                };
                let mut capture_inputs = Vec::new();
                let mut borrowed_task_inputs = Vec::new();
                let captured_mode = self.proof.kinds.get(&owner).copied();
                if definition.kind == SemanticDefinitionKind::Closure {
                    let (parent, creator) = self.closure_creator(definition, depth + 1)?;
                    self.pay(
                        self.proof.capture_names.len().saturating_add(1).ilog2() as usize
                            + 1
                            + lookup_work(self.proof.closure_move.len()),
                        depth,
                    )?;
                    let moved = *self.proof.closure_move.get(&owner).ok_or_else(|| {
                        failure("original closure cleanup lacks its creator mode")
                    })?;
                    let names = self.proof.capture_names.get(&owner).ok_or_else(|| {
                        failure("original closure cleanup lacks its physical capture directory")
                    })?;
                    self.pay(
                        names
                            .iter()
                            .fold(names.len(), |work, name| work.saturating_add(name.len())),
                        depth,
                    )?;
                    self.pay(lookup_work(self.proof.capture_names.len()), depth)?;
                    let names = self
                        .proof
                        .capture_names
                        .get(&owner)
                        .ok_or_else(|| failure("original closure capture directory disappeared"))?
                        .clone();
                    for (index, name) in names.into_iter().enumerate() {
                        let moved = moved || self.inferred_capture_move(owner, &name, depth + 1)?;
                        let ty =
                            self.capture_type(parent, creator, &name, definition, depth + 1)?;
                        let mut value = if let Some(mut value) = self.task_input(
                            parent,
                            &ty,
                            Root::Capture(owner, index as u32),
                            &mut state,
                            depth + 1,
                        )? {
                            if !moved {
                                value = self.view(value, depth + 1)?;
                            }
                            value
                        } else if self.async_head(parent, &ty, depth + 1)? {
                            let root = Root::Capture(owner, index as u32);
                            state.roots.insert(root, UNKNOWN);
                            Value::Root(
                                root,
                                true,
                                moved && self.ownership(parent, &ty, depth + 1)?.unwrap_or(false),
                                None,
                            )
                        } else {
                            self.typed(parent, &ty, Value::Plain, depth + 1)?
                        };
                        self.input_sources(&mut value, Root::Capture(owner, index as u32), &mut Vec::new(), depth + 1)?;
                        if !moved {
                            value = self.view(value, depth + 1)?;
                        }
                        self.pay(name.len().saturating_add(1), depth)?;
                        capture_inputs.push((name.clone(), self.value_copy(&value, depth + 1)?));
                        state.names.insert(name, vec![value]);
                    }
                }
                for (index, parameter) in parameters.iter().enumerate() {
                    if let Some(bindings) = &mut state.task_sources {
                        let ordinal = u32::try_from(index)
                            .map_err(|_| failure("Task source formal ordinal overflow"))?;
                        self.task_set_sources(
                            bindings,
                            Query::Parameter(ordinal),
                            Some(vec![TaskResultSource {
                                result_path: Vec::new(),
                                parameter: ordinal,
                                input_path: Vec::new(),
                                class: None,
                            }]),
                            depth + 1,
                        )?;
                    }
                    self.pay(parameter.name.len().saturating_add(1), depth)?;
                    let value = self.consumption_parameter(owner, index as u32,
                        inputs.get(&(index as u32)).copied().map(bit), &mut state, depth + 1)?;
                    if matches!(&value, Value::Root(root, true, false, None) if state.tasks.contains(root)) {
                        borrowed_task_inputs.push((index, self.value_copy(&value, depth + 1)?));
                    }
                    state
                        .names
                        .entry(parameter.name.clone())
                        .or_default()
                        .push(value);
                }
                self.record_cost("function_inputs", inputs_before, inputs_children);
                for mut state in self.block(owner, body, state, depth + 1)? {
                    if matches!(state.control, Control::Break | Control::Continue) {
                        return Err(failure(
                            "original protocol loop control escaped its function",
                        ));
                    }
                    if definition.kind == SemanticDefinitionKind::Closure {
                        for (index, expected) in &borrowed_task_inputs {
                            self.pay(parameters[*index].name.len().saturating_mul(lookup_work(state.names.len())).saturating_add(1), depth)?;
                            let actual = state.names.get(&parameters[*index].name).and_then(|values| values.first());
                            if let Some(actual) = actual {
                                reusable_inputs_complete &= self.same_input_occurrence(expected, actual, depth + 1)?;
                            } else { reusable_inputs_complete = false; }
                        }
                        let mut remaining = Some(BTreeSet::new());
                        for (name, expected) in &capture_inputs {
                            self.pay(
                                name.len().saturating_add(lookup_work(state.names.len())),
                                depth,
                            )?;
                            let actual = state
                                .names
                                .get(name)
                                .and_then(|stack| stack.first())
                                .ok_or_else(|| {
                                    failure("original reusable closure loses its captured binding")
                                })?;
                            if matches!(captured_mode, Some(CallableKind::Fn | CallableKind::FnMut))
                                && !self.present(expected, actual, depth + 1)? {
                                return Err(failure(
                                    "original reusable closure exits without refilling its captured owning responsibility",
                                ));
                            }
                            if matches!(captured_mode, Some(CallableKind::Fn | CallableKind::FnMut)) {
                                reusable_inputs_complete &= self.same_input_occurrence(expected, actual, depth + 1)?;
                            }
                            let needed = self.remaining_consumption(actual, &state, depth + 1)?;
                            match (&mut remaining, needed) {
                                (Some(all), Some(needed)) => { self.pay(needed.len(), depth)?; all.extend(needed); }
                                _ => remaining = None,
                            }
                            let stack = state.names.get_mut(name).ok_or_else(|| {
                                failure("original closure handoff lost its captured binding")
                            })?;
                            self.pay(stack.len(), depth)?;
                            stack.remove(0);
                        }
                        state.names.retain(|_, values| !values.is_empty());
                        self.record_consuming_exit(owner, remaining, depth + 1)?;
                    }
                    for (name, values) in &state.names {
                        for value in values {
                            self.pay(name.len().saturating_add(1), depth)?;
                            self.drop_value(value, &state, depth + 1).map_err(|mut diagnostic| {
                                diagnostic.notes.push(format!(
                                    "cleanup at function exit requires dropping binding {name:?} in {owner:?}"
                                ));
                                crate::semantics::locate_original(diagnostic, body.span)
                            })?;
                        }
                    }
                    self.check_consuming_result(owner, &state, depth + 1)?;
                    self.output(owner, &state, protocol_contracts, &inputs, depth + 1)?;
                    self.task_record_exit(owner, &state, depth + 1)?;
                }
            }
            if !reusable_inputs_complete {
                // Do not publish a phase-only summary for replaced receiver
                // occupants. The ordinary unknown-call path remains available.
                self.task_exits.remove(&owner);
            }
            self.active.remove(&owner);
            self.local_complete.insert(owner);
            if self.graph_requested.contains(&owner) {
                self.graph_complete.insert(owner);
            }
            self.proof.protocol_complete.insert(owner);
            self.proof.cleanup_complete.insert(owner);
            Ok(())
        })();
        self.record_cost("function", cost_before, cost_children);
        let work = cost_before.saturating_sub(*self.left);
        let exclusive =
            work.saturating_sub(self.function_cost_total.saturating_sub(function_children));
        self.function_cost_total = self.function_cost_total.saturating_add(exclusive);
        let cost = self.proof.protocol_body_costs.entry(owner).or_default();
        cost.0 = cost.0.saturating_add(1);
        cost.1 = cost.1.saturating_add(work);
        cost.2 = cost.2.saturating_add(exclusive);
        result
    }
}

pub(super) fn check(
    program: &SemanticModule,
    proof: &mut Proof,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let check_before = *left;
    // An unused foreign call still needs its real original unsafe boundary.
    // The actual marker/ABI is checked again by the actual world below.
    for ((caller, site), target) in &proof.call_targets {
        if proof.protocol_complete.contains(caller) {
            continue;
        }
        *left = left
            .checked_sub(
                program
                    .definitions
                    .len()
                    .saturating_mul(2)
                    .saturating_add(1),
            )
            .ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "original foreign edge work exceeded", None)
            })?;
        let target = program
            .definitions
            .iter()
            .find(|d| d.id == *target)
            .ok_or_else(|| failure("original foreign edge target is absent"))?;
        if matches!(
            &target.body,
            SemanticDefinitionBody::FunctionWithBody {
                foreign: Some(_),
                ..
            }
        ) {
            let original = program
                .definitions
                .iter()
                .find(|d| d.id == *caller)
                .ok_or_else(|| failure("original foreign edge caller is absent"))?;
            if !unsafe_call(original, *site, limits, left)? {
                return Err(failure(
                    "original foreign call is outside its genuine unsafe region",
                ));
            }
        }
    }
    let foreign_work = check_before.saturating_sub(*left);
    // Fixed diagnostic storage only; no AST/type/state is copied for tracing.
    *left = left
        .checked_sub(
            PROTOCOL_COST_KEYS
                .len()
                .saturating_mul(4)
                .saturating_add(program.definitions.len().saturating_mul(4))
                .saturating_add(16),
        )
        .ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "original protocol cost metadata work exceeded",
                None,
            )
        })?;
    proof
        .stage_costs
        .insert("protocol: foreign edges", foreign_work);
    let mut engine = Engine {
        program,
        proof,
        limits,
        left,
        active: BTreeSet::new(),
        relevant: BTreeSet::new(),
        type_ids: type_cache::Table::new(),
        type_values: Vec::new(),
        type_categories: Vec::new(),
        type_cleanup: Vec::new(),
        type_fields: Vec::new(),
        type_errors: Vec::new(),
        type_plain: Vec::new(),
        type_shape_nodes: 0,
        query_types: BTreeMap::new(),
        method_costs: PROTOCOL_COST_KEYS
            .iter()
            .map(|key| (*key, (0, 0, 0)))
            .collect(),
        method_cost_total: 0,
        function_cost_total: 0,
        task_exits: BTreeMap::new(),
        task_exit_count: 0,
        task_source_returns: BTreeSet::new(),
        synchronous_workers: BTreeMap::new(),
        graph_calls: BTreeMap::new(),
        graph_call_count: 0,
        local_complete: BTreeSet::new(),
        call_sites: BTreeMap::new(),
        call_site_count: 0,
        graph_requested: BTreeSet::new(),
        graph_complete: BTreeSet::new(),
        projected_sources: Vec::new(),
        projected_ids: BTreeMap::new(),
        consuming_relevant: BTreeSet::new(),
        consuming_exits: BTreeMap::new(),
        consuming_requirements: BTreeMap::new(),
    };
    // Only callees whose original body can reach a consuming invocation need
    // an application obligation. Unrelated ordinary recursion is unchanged.
    engine.pay(engine.proof.callable_calls.len(), 0)?;
    for ((owner, _), call) in &engine.proof.callable_calls {
        if call.kind == CallableKind::FnOnce { engine.consuming_relevant.insert(*owner); }
    }
    engine.pay(engine.proof.intrinsics.len(), 0)?;
    for ((owner, _), call) in &engine.proof.intrinsics {
        if matches!(call.kind, IntrinsicKind::ThreadScope { .. } | IntrinsicKind::SyncCellScope(_)) { engine.consuming_relevant.insert(*owner); }
    }
    loop {
        let mut changed = false;
        engine.pay(engine.proof.call_targets.len().saturating_add(engine.proof.intrinsics.len()), 0)?;
        for ((owner, _), target) in &engine.proof.call_targets {
            if engine.consuming_relevant.contains(target) { changed |= engine.consuming_relevant.insert(*owner); }
        }
        for ((owner, _), call) in &engine.proof.intrinsics {
            if let IntrinsicKind::TaskCreate { worker, .. } = call.kind {
                if engine.consuming_relevant.contains(&worker) { changed |= engine.consuming_relevant.insert(*owner); }
            }
        }
        if !changed { break; }
    }
    let setup_before = *engine.left;
    engine.pay(
        program
            .definitions
            .len()
            .saturating_add(engine.proof.intrinsics.len()),
        0,
    )?;
    for definition in &program.definitions {
        if matches!(&definition.body,SemanticDefinitionBody::FunctionWithBody{protocol_contracts,..}if !protocol_contracts.is_empty())
        {
            engine.relevant.insert(definition.id);
        }
    }
    for ((owner, _), intrinsic) in &engine.proof.intrinsics {
        if matches!(
            intrinsic.kind,
            IntrinsicKind::RegistrationStart {
                form: CallbackRegistration::AsyncOwned,
                ..
            } | IntrinsicKind::RegistrationOperation(
                RegistrationOp::AsyncValid
                    | RegistrationOp::AsyncRequest
                    | RegistrationOp::AsyncPollClose
                    | RegistrationOp::AsyncFinish
                    | RegistrationOp::AsyncFinishChecked
            )
        ) {
            engine.relevant.insert(*owner);
        }
    }
    loop {
        engine.pay(
            engine
                .proof
                .call_targets
                .len()
                .saturating_add(engine.relevant.len()),
            0,
        )?;
        let mut changed = false;
        for ((caller, site), callee) in &engine.proof.call_targets {
            if !engine.proof.compile_time_calls.contains(&(*caller, *site))
                && engine.relevant.contains(callee)
            {
                changed |= engine.relevant.insert(*caller);
            }
        }
        if !changed {
            break;
        }
    }
    let setup_work = setup_before.saturating_sub(*engine.left);
    engine
        .proof
        .stage_costs
        .insert("protocol: relevance", setup_work);
    let functions_before = *engine.left;
    let result: R<()> = (|| {
        for position in engine.function_order()? {
            let definition = &program.definitions[position];
            if matches!(
                definition.body,
                SemanticDefinitionBody::FunctionWithBody { .. }
            ) {
                engine
                    .function(definition.id, 0)
                    .map_err(|mut diagnostic| {
                        diagnostic
                            .notes
                            .push(format!("original checked function {:?}", definition.id));
                        diagnostic
                    })?;
            }
        }
        Ok(())
    })();
    let function_work = functions_before.saturating_sub(*engine.left);
    engine
        .proof
        .stage_costs
        .insert("protocol: function traversal", function_work);
    engine.proof.protocol_method_costs = engine.method_costs;
    result.map_err(|mut diagnostic| {
        diagnostic.notes.push(format!(
            "original protocol method costs {:?}; function costs {:?}",
            engine.proof.protocol_method_costs, engine.proof.protocol_body_costs
        ));
        diagnostic
    })
}

/// Read-only original AST authority for a specific original Call site. An
/// actual unsafe marker, symbol spelling, or same-typed call is never input.
pub(in crate::semantics) fn unsafe_call(
    original: &SemanticDefinition,
    site: SemanticSiteId,
    limits: &Limits,
    left: &mut usize,
) -> R<bool> {
    let SemanticDefinitionBody::FunctionWithBody {
        site_nodes,
        body: SemanticOriginalFunctionBody::Defined(body),
        ..
    } = &original.body
    else {
        return Ok(false);
    };
    *left = left
        .checked_sub(site_nodes.len().saturating_add(1))
        .ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "original unsafe site work exceeded", None)
        })?;
    let mut selected = None;
    for row in site_nodes {
        if row.site == site {
            let SemanticOriginalNode::Expression(node) = row.node else {
                return Err(failure("foreign call source is not an original expression"));
            };
            if selected.replace(node).is_some() {
                return Err(failure(
                    "foreign call site has multiple original source nodes",
                ));
            }
        }
    }
    let target =
        selected.ok_or_else(|| failure("foreign call has no original source occurrence"))?;
    crate::dyn_metadata::semantic_operand_snapshot(body, limits, left, 1)?;
    let mut body = body.clone();
    let mut scan = UnsafeSource {
        target,
        scan: true,
        count: 0,
        inside: false,
        limits,
        left,
    };
    super::super::original_walk::block(&mut body, &mut scan, 0)?;
    if scan.count != 1 {
        return Err(failure(
            "original foreign source expression is absent or duplicated",
        ));
    }
    Ok(scan.inside)
}
struct UnsafeSource<'a> {
    target: NodeId,
    scan: bool,
    count: usize,
    inside: bool,
    limits: &'a Limits,
    left: &'a mut usize,
}
impl super::super::original_walk::Context for UnsafeSource<'_> {
    fn node(&mut self, depth: usize) -> R<()> {
        *self.left = self.left.checked_sub(1).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "original unsafe traversal work exceeded",
                None,
            )
        })?;
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "original unsafe traversal depth exceeded",
                None,
            ));
        }
        Ok(())
    }
    fn ty(&mut self, _: &mut SemanticTypeRecipe, depth: usize) -> R<()> {
        self.node(depth)
    }
    fn descend_closure(&self) -> bool {
        false
    }
    fn expression(&mut self, id: NodeId, _: &SemanticOriginalExprKind, depth: usize) -> R<()> {
        self.node(depth)?;
        if id == self.target {
            self.count = self
                .count
                .checked_add(1)
                .ok_or_else(|| failure("original unsafe occurrence count overflows"))?;
            if !self.scan {
                self.inside = true;
            }
        }
        Ok(())
    }
    fn statement(
        &mut self,
        _: NodeId,
        kind: &SemanticOriginalStatementKind,
        depth: usize,
    ) -> R<()> {
        self.node(depth)?;
        if self.scan {
            if let CoreStatementKind::Unsafe(body) = kind {
                crate::dyn_metadata::semantic_operand_snapshot(
                    body,
                    self.limits,
                    self.left,
                    depth + 1,
                )?;
                let mut body = body.clone();
                let mut nested = UnsafeSource {
                    target: self.target,
                    scan: false,
                    count: 0,
                    inside: false,
                    limits: self.limits,
                    left: self.left,
                };
                super::super::original_walk::block(&mut body, &mut nested, depth + 1)?;
                self.inside |= nested.inside;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod plain_layout_tests {
    use super::*;

    #[test]
    fn reusable_plain_shapes_never_replace_active_values_or_unresolved_domains() {
        let owner = SemanticDefinitionId::default();
        let program = SemanticModule {
            definitions: vec![SemanticDefinition {
                id: owner,
                identity: LifeDeclarationIdentity {
                    package: "plain-layout".into(),
                    module_path: vec![],
                    declaration_path: vec!["owner".into()],
                    generation_path: vec![],
                },
                parent: None,
                kind: SemanticDefinitionKind::Function,
                parameters: vec![],
                implicit_lifetimes: vec![],
                requirements: vec![],
                environment: vec![],
                body: SemanticDefinitionBody::Function {
                    signature: SemanticSignatureRecipe {
                        bound_lifetimes: 0,
                        invocation_lifetimes: 0,
                        parameters: vec![],
                        result: SemanticTypeRecipe::Atom(Type::Unit),
                        result_mode: PassingMode::Copy,
                        call_kind: CallKind::Direct,
                    },
                    sites: vec![],
                },
            }],
            instances: vec![],
            sites: vec![],
            dyn_scopes: vec![],
        };
        let mut proof = Proof::default();
        proof.complete.insert(owner);
        proof.definition_positions = std::rc::Rc::new(BTreeMap::from([(owner, 0)]));
        let queried = Query::Expression(NodeId(7));
        proof.types.insert(
            (owner, queried.clone()),
            SemanticTypeRecipe::Managed(Box::new(SemanticTypeRecipe::Atom(Type::ByteBuffer))),
        );
        let limits = Limits::default();
        let mut left = limits.max_analysis_steps;
        let mut engine = Engine {
            program: &program,
            proof: &mut proof,
            limits: &limits,
            left: &mut left,
            active: BTreeSet::new(),
            relevant: BTreeSet::new(),
            type_ids: type_cache::Table::new(),
            type_values: vec![],
            type_categories: vec![],
            type_cleanup: vec![],
            type_fields: vec![],
            type_errors: vec![],
            type_plain: vec![],
            type_shape_nodes: 0,
            query_types: BTreeMap::new(),
            method_costs: PROTOCOL_COST_KEYS
                .iter()
                .map(|key| (*key, (0, 0, 0)))
                .collect(),
            method_cost_total: 0,
            function_cost_total: 0,
            task_exits: BTreeMap::new(),
            task_exit_count: 0,
            task_source_returns: BTreeSet::new(),
        synchronous_workers: BTreeMap::new(),
        graph_calls: BTreeMap::new(),
        graph_call_count: 0,
        local_complete: BTreeSet::new(),
        call_sites: BTreeMap::new(),
        call_site_count: 0,
        graph_requested: BTreeSet::new(),
        graph_complete: BTreeSet::new(),
        projected_sources: Vec::new(),
        projected_ids: BTreeMap::new(),
        consuming_relevant: BTreeSet::new(),
        consuming_exits: BTreeMap::new(),
        consuming_requirements: BTreeMap::new(),
        };
        let recipe = SemanticTypeRecipe::ErrorUnion {
            error: Box::new(SemanticTypeRecipe::Atom(Type::Unit)),
            result: Box::new(SemanticTypeRecipe::Managed(Box::new(
                SemanticTypeRecipe::Atom(Type::ByteBuffer),
            ))),
        };
        let ty = engine.type_handle(owner, &recipe, 0).unwrap();
        let queried_type = engine
            .query_type(owner, queried.clone(), 0)
            .unwrap()
            .unwrap();
        let directory_len = engine.type_values.len();
        let repeated = engine.query_type(owner, queried, 0).unwrap().unwrap();
        assert!(queried_type == repeated);
        assert_eq!(engine.type_values.len(), directory_len);
        let first = engine.typed(owner, &ty, Value::Plain, 0).unwrap();
        assert!(matches!(engine.type_plain[ty.2], PlainLayout::Complete(_)));
        let before = *engine.left;
        let second = engine.typed(owner, &ty, Value::Plain, 0).unwrap();
        assert!(first == second);
        assert!(*engine.left < before);
        let Value::Choice(first_rows) = &first else {
            panic!()
        };
        let Value::Choice(second_rows) = &second else {
            panic!()
        };
        assert!(!std::ptr::eq(first_rows.as_ptr(), second_rows.as_ptr()));
        let active = Value::Root(Root::Parameter(owner, 0), true, true, Some(OPEN));
        let returned = engine.typed(owner, &ty, active.clone(), 0).unwrap();
        assert!(returned == active);
        // The reusable shape grants no independent synchronous cleanup proof.
        assert!(engine.type_cleanup[ty.2].is_none());
        for unstable in [
            SemanticTypeRecipe::Formal(SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(u32::MAX),
                ordinal: 0,
            }),
            SemanticTypeRecipe::Environment {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal: 0,
            },
            SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source: SemanticOriginalTypeSource::Result,
            },
            SemanticTypeRecipe::Unsupported("unresolved".into()),
        ] {
            let nested = SemanticTypeRecipe::Managed(Box::new(unstable));
            assert!(!engine.stable_plain_type(&nested, 0).unwrap());
        }
        assert!(engine.value_copy(&second, limits.max_call_depth).is_err());
    }
}
