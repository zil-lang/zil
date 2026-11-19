//! Symbolic facts obtained from the immutable original bodies. This module has
//! no TypedModule, ValueId, closure runtime ID or instance actuals as input.
use crate::*;
use super::original_code_context as code_context;
use std::collections::{BTreeMap, BTreeSet};
use zil_base::NodeId;
#[path = "original_associated.rs"]
mod associated;
#[path = "original_call_regions.rs"]
mod call_regions;
#[path = "original_calls.rs"]
mod calls;
#[path = "original_capture_candidates.rs"]
mod capture_candidates;
#[path = "original_cleanup.rs"]
pub(super) mod cleanup;
#[path = "original_dyn.rs"]
mod dynamic;
#[path = "original_effects.rs"]
mod effects;
#[path = "original_places.rs"]
mod places;
#[path = "original_protocol.rs"]
pub(super) mod protocol;
#[path = "original_results.rs"]
mod results;
#[path = "original_site_directory.rs"]
mod sites;
pub(super) use results::CalleeResultBoundary;
#[path = "original_frozen.rs"]
mod frozen;
pub(super) use frozen::check_with_frozen;
#[path = "original_intrinsics.rs"]
mod intrinsics;
#[path = "original_modes.rs"]
mod modes;
#[path = "original_resources.rs"]
mod resources;
#[path = "original_resource_writes.rs"]
mod resource_writes;
pub(super) use intrinsics::{IntrinsicCall, IntrinsicKind, RegistrationOp, TaskOp};
#[path = "original_pattern_matrix.rs"]
mod pattern_matrix;
#[path = "original_nominal_where.rs"]
mod nominal_where;
#[path = "original_roles.rs"]
mod roles;
#[path = "original_source_cache.rs"]
mod source_cache;
#[path = "original_static_regions.rs"]
mod static_regions;
#[path = "original_store.rs"]
mod store;
#[path = "original_substitute.rs"]
pub(super) mod substitute;
#[path = "original_task_bridge.rs"]
mod task_bridge;
#[path = "original_task_families.rs"]
mod task_families;
#[path = "original_task_results.rs"]
mod task_results;
#[path = "original_task_unify.rs"]
mod task_unify;
pub(super) use nominal_where::{NominalRequest, NominalWhere};
pub(super) use task_results::SourceBindings;
pub(crate) use task_results::{TaskResultSource, TaskReturnSources};
#[path = "original_value_predicate.rs"]
mod value_predicate;
#[path = "original_values.rs"]
mod values;
#[path = "original_views.rs"]
mod views;
pub(super) use calls::{CallableCall, MethodCall, MethodReceiverPlace};
pub(super) use places::FieldReborrow;
pub(super) use values::FunctionValue;
pub(super) use views::{BorrowRole, ViewBorrowRoot, ViewBorrowStep, ViewBorrowStepKind, local_source_copy};
fn bad(message: &str) -> Diagnostic {
    Diagnostic::error(Phase::Types, "E-SEMANTIC-ORIGINAL", None, message)
}
fn pending(message: &str) -> Diagnostic {
    Diagnostic::unsupported(Phase::Types, message, None)
}
fn copy_recipe(
    ty: &SemanticTypeRecipe,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticTypeRecipe> {
    crate::dyn_metadata::semantic_operand_snapshot(ty, limits, left, 1)?;
    Ok(ty.clone())
}
fn type_head(ty: &SemanticTypeRecipe) -> &'static str {
    match ty {
        SemanticTypeRecipe::Atom(ty) => match ty {
            Type::Unit => "Unit",
            Type::Bool => "Bool",
            Type::I8 => "i8",
            Type::I16 => "i16",
            Type::I32 => "i32",
            Type::I64 => "i64",
            Type::Isize => "isize",
            Type::U8 => "u8",
            Type::U16 => "u16",
            Type::U32 => "u32",
            Type::U64 => "u64",
            Type::Usize => "usize",
            Type::F32 => "f32",
            Type::F64 => "f64",
            Type::ByteBuffer => "ByteBuffer",
            _ => "Atom",
        },
        SemanticTypeRecipe::Formal(_) => "Formal",
        SemanticTypeRecipe::SelfType { .. } => "SelfType",
        SemanticTypeRecipe::Environment { .. } => "Environment",
        SemanticTypeRecipe::OriginalType { .. } => "OriginalType",
        SemanticTypeRecipe::Nominal { .. } => "Nominal",
        SemanticTypeRecipe::Opaque { .. } => "Opaque",
        SemanticTypeRecipe::Managed(_) => "Managed",
        SemanticTypeRecipe::Desc { .. } => "Desc",
        SemanticTypeRecipe::Address { .. } => "Address",
        SemanticTypeRecipe::Code { .. } => "Code",
        SemanticTypeRecipe::Closure { .. } => "Closure",
        SemanticTypeRecipe::ErrorUnion { .. } => "ErrorUnion",
        SemanticTypeRecipe::Array { .. } => "Array",
        SemanticTypeRecipe::Slice(_) => "Slice",
        SemanticTypeRecipe::Tuple(_) => "Tuple",
        SemanticTypeRecipe::Task { .. } => "Task",
        _ => "Other",
    }
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Query {
    Expression(NodeId),
    Binding(NodeId),
    Parameter(u32),
    Result,
    CallConstCarrier {
        call: NodeId,
        formal: u32,
    },
    BindingProjection {
        statement: NodeId,
        pattern: Vec<u32>,
    },
    NameAt {
        expression: bool,
        node: NodeId,
        name: String,
    },
}
impl Query {
    fn copy_work(&self) -> usize {
        match self {
            Self::BindingProjection { pattern, .. } => pattern.len().saturating_add(1),
            Self::NameAt { name, .. } => name.len().saturating_add(1),
            _ => 1,
        }
    }
}
impl From<SemanticOriginalTypeSource> for Query {
    fn from(source: SemanticOriginalTypeSource) -> Self {
        match source {
            SemanticOriginalTypeSource::Expression(id) => Self::Expression(id),
            SemanticOriginalTypeSource::Binding(id) => Self::Binding(id),
            SemanticOriginalTypeSource::Parameter(i) => Self::Parameter(i),
            SemanticOriginalTypeSource::Result => Self::Result,
            SemanticOriginalTypeSource::CallConstCarrier { call, formal } => {
                Self::CallConstCarrier { call, formal }
            }
            SemanticOriginalTypeSource::BindingProjection { statement, pattern } => {
                Self::BindingProjection { statement, pattern }
            }
            SemanticOriginalTypeSource::NameAt { at, name } => {
                let (expression, node) = match at {
                    SemanticOriginalNode::Expression(node) => (true, node),
                    SemanticOriginalNode::Statement(node) => (false, node),
                };
                Self::NameAt {
                    expression,
                    node,
                    name,
                }
            }
        }
    }
}
#[derive(Clone)]
pub(super) struct Injection {
    pub is_error: bool,
    pub payload: SemanticTypeRecipe,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct TemporaryDescription {
    pub argument: NodeId,
    pub producer: NodeId,
}

#[derive(Clone, Default)]
pub(super) struct Proof {
    // Closed original integer return whose candidate still needs a checked callable use.
    integer_return_obligations: BTreeMap<SemanticDefinitionId, bool>,
    literal_return_closures: BTreeSet<SemanticDefinitionId>,
    capture_candidates: BTreeMap<SemanticDefinitionId, std::rc::Rc<capture_candidates::Candidates>>,
    pub(super) definition_positions: std::rc::Rc<BTreeMap<SemanticDefinitionId, usize>>,
    original_sites: std::rc::Rc<sites::Directory>,
    view_bounds: BTreeMap<(SemanticDefinitionId, Query), views::ViewBorrowBound>,
    opaque_forwarded_returns: BTreeMap<SemanticDefinitionId, BTreeSet<NodeId>>,
    pub borrow_roles: BTreeMap<(SemanticDefinitionId, NodeId), BorrowRole>,
    pub(super) capture_writes: BTreeMap<(SemanticDefinitionId, NodeId), CapturedRoleWrite>,
    // Exact original call input for an already-declared contextual outer role.
    // The value's own TypeUse keeps its original sigma; only this call uses it.
    pub(super) call_borrows: BTreeMap<(SemanticDefinitionId, NodeId, u32), NodeId>,
    pub(super) temporary_descriptions: BTreeMap<(SemanticDefinitionId, NodeId, u32), TemporaryDescription>,
    resource_writes: resource_writes::Graph,
    pub(super) payload_borrows: BTreeMap<(SemanticDefinitionId, NodeId, usize), (NodeId, BorrowRole)>,
    pub(super) return_borrows: BTreeMap<(SemanticDefinitionId, NodeId), BorrowRole>,
    pub(super) shared_views: BTreeMap<(SemanticDefinitionId, Query), NodeId>,
    pub(super) borrow_bindings:
        BTreeMap<(SemanticDefinitionId, NodeId), SemanticOriginalTypeSource>,
    pub(super) call_regions: BTreeMap<(SemanticDefinitionId, u32), call_regions::CallRegion>,
    pub field_reborrows: BTreeMap<(SemanticDefinitionId, NodeId), FieldReborrow>,
    pub(super) pattern_borrows: BTreeMap<(SemanticDefinitionId, Span), (u32, bool)>,
    pub declarations: std::rc::Rc<Vec<super::original_context::Declaration>>,
    pub associated_complete: BTreeSet<LifeDeclarationId>,
    pub capture_names: BTreeMap<SemanticDefinitionId, Vec<String>>,
    pub closure_move: BTreeMap<SemanticDefinitionId, bool>,
    pub(super) cleanup_actions: BTreeSet<SemanticDefinitionId>,
    inferred_capture_moves: BTreeMap<SemanticDefinitionId, BTreeSet<String>>,
    pub injections: BTreeMap<(SemanticDefinitionId, NodeId), Injection>,
    pub types: BTreeMap<(SemanticDefinitionId, Query), SemanticTypeRecipe>,
    // Request-local cost diagnostics; these fields supply no semantic fact.
    pub(super) body_costs: BTreeMap<SemanticDefinitionId, (usize, usize, usize)>,
    pub(super) protocol_body_costs: BTreeMap<SemanticDefinitionId, (usize, usize, usize)>,
    pub(super) protocol_method_costs: BTreeMap<&'static str, (usize, usize, usize)>,
    pub(super) stage_costs: BTreeMap<&'static str, usize>,
    body_cost_total: usize,
    task_returns: BTreeMap<(SemanticDefinitionId, NodeId), task_results::ReturnValue>,
    task_result_types: BTreeMap<SemanticDefinitionId, SemanticTypeRecipe>,
    task_binding_sources: BTreeMap<(SemanticDefinitionId, NodeId), Option<Vec<TaskResultSource>>>,
    task_name_sources: BTreeMap<(SemanticDefinitionId, NodeId), task_results::NameSource>,
    pub calls: BTreeMap<(SemanticDefinitionId, SemanticSiteId), Vec<SemanticActualRecipe>>,
    pub complete: BTreeSet<SemanticDefinitionId>,
    pub protocol_complete: BTreeSet<SemanticDefinitionId>,
    pub cleanup_complete: BTreeSet<SemanticDefinitionId>,
    pub kinds: BTreeMap<SemanticDefinitionId, CallableKind>,
    pub call_targets: BTreeMap<(SemanticDefinitionId, SemanticSiteId), SemanticDefinitionId>,
    pub intrinsics: BTreeMap<(SemanticDefinitionId, SemanticSiteId), IntrinsicCall>,
    pub method_calls: BTreeMap<(SemanticDefinitionId, SemanticSiteId), MethodCall>,
    pub callable_calls: BTreeMap<(SemanticDefinitionId, SemanticSiteId), CallableCall>,
    pub providers: BTreeMap<SemanticDefinitionId, SemanticDefinitionId>,
    pub function_values: BTreeMap<(SemanticDefinitionId, NodeId), FunctionValue>,
    pub(super) contextual_code_frames: BTreeMap<SemanticDefinitionId, u32>,
    pub nominal_constructors: BTreeMap<(SemanticDefinitionId, NodeId), (SemanticDefinitionId, u32)>,
    pub direct_calls: BTreeMap<(SemanticDefinitionId, SemanticSiteId), SemanticSignatureRecipe>,
    pub effective_call_kind: BTreeMap<SemanticDefinitionId, CallKind>,
    pub borrowed_capture_heads: BTreeSet<SemanticDefinitionId>,
    closed_opaque_domains: BTreeMap<SemanticDefinitionId, bool>,
    pure_queries_ready: bool,
    pure_queries: source_cache::Cache,
    premises: source_cache::Premises,
    compile_time_calls: BTreeSet<(SemanticDefinitionId, SemanticSiteId)>,
    pub(super) compile_time_results: BTreeSet<(SemanticDefinitionId, NodeId)>,
    cleanup_calls: BTreeSet<(SemanticDefinitionId, SemanticSiteId)>,
    pub local_roles: BTreeMap<(SemanticDefinitionId, NodeId), LocalRole>,
    pub return_writers: BTreeMap<(SemanticDefinitionId,NodeId),(OriginalSlotWrite,SemanticOuterLifeRecipe)>,
    pub scope_calls: BTreeMap<
        (SemanticDefinitionId, SemanticSiteId, SemanticDefinitionId),
        Vec<SemanticActualRecipe>,
    >,
    pub(super) call_injections: BTreeMap<
        (SemanticDefinitionId, SemanticSiteId),
        (usize, Vec<(usize, NodeId)>),
    >,
    call_callees: BTreeMap<(SemanticDefinitionId, SemanticSiteId), NodeId>,
    inferred_pointee_environments: BTreeMap<(SemanticDefinitionId, NodeId, u32), BTreeSet<u32>>,
    inferred_type_origins: BTreeMap<
        (SemanticDefinitionId, NodeId, u32),
        Vec<(NodeId, SemanticDefinitionId, SemanticSiteId)>,
    >,
    // None preserves ambiguity between independently originating results.
    inferred_type_sources: BTreeMap<
        (SemanticDefinitionId, NodeId, u32),
        Option<(NodeId, SemanticDefinitionId, SemanticSiteId)>,
    >,
    call_result_origins:
        BTreeMap<(SemanticDefinitionId, Query), (SemanticDefinitionId, SemanticSiteId)>,
    projected_type_actuals: BTreeMap<(SemanticDefinitionId, NodeId), SemanticTypeRecipe>,
    environments: BTreeMap<(SemanticDefinitionId, u32), (SemanticDefinitionId, SemanticTypeRecipe)>,
    frozen_environments: BTreeMap<(SemanticDefinitionId, bool, NodeId), BTreeMap<String, Binding>>,
}
#[derive(Clone, PartialEq, Eq)]
pub(super) struct CapturedRoleWrite {
    pub creator_role_owner: SemanticDefinitionId,
    pub creator_binding: NodeId,
    pub environment: u32,
    pub role: BorrowRole,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LocalRoleKind {
    Address { mutable: bool },
    Desc,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum OriginalRoleEscape {
    Input {
        owner: SemanticDefinitionId,
        parameter: u32,
    },
    Static,
    LocalPlace {
        owner: SemanticDefinitionId,
        node: NodeId,
    },
    Role {
        owner: SemanticDefinitionId,
        binding: NodeId,
    },
    CallRegion {
        owner: SemanticDefinitionId,
        ordinal: u32,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LocalRoleSource {
    pub at: SemanticOriginalNode,
    pub lifetime: SemanticOuterLifeRecipe,
    pub escape: OriginalRoleEscape,
}
#[derive(Clone)]
pub(super) struct LocalRole {
    pub kind: LocalRoleKind,
    pub binding: NodeId,
    pub lifetime: SemanticLifeRecipe,
    pub ty: SemanticTypeRecipe,
    pub sources: Vec<LocalRoleSource>,
    pub escapes: bool,
    pub result_upper: Option<u32>,
}
#[derive(Clone)]
struct Binding {
    // A branch can change the occupant and replace its recipe. Other
    // branches keep their own state and share only an immutable recipe.
    ty: std::rc::Rc<SemanticTypeRecipe>,
    mutable: bool,
    used: bool,
    observed: bool,
    capture: bool,
    borrowed: bool,
    reaching_write: OriginalSlotWrite,
    origin: Query,
    origin_owner: SemanticDefinitionId,
    initialized: bool,
    written: bool,
    resource_root: Option<resource_writes::Root>,
    taken_fields: BTreeSet<String>,
    // Source shadowing preserves the state of the hidden binder. It must be
    // recovered by its original binding identity when the lexical scope exits.
    shadowed: Option<std::rc::Rc<Binding>>,
}
struct IntegerInitializer {
    expression: NodeId,
    spelling: String,
    span: Span,
}
struct Check<'a, 'b> {
    program: &'a SemanticModule,
    limits: &'b Limits,
    left: &'b mut usize,
    proof: ProofAccess<'a>,
    active: BTreeSet<SemanticDefinitionId>,
    capture_writes: BTreeMap<SemanticDefinitionId, BTreeSet<String>>,
    loops: Vec<LoopFrame>,
    trait_active: Vec<(SemanticTypeRecipe, SemanticTraitRecipe)>,
    contexts: Vec<SemanticDefinitionId>,
    // Only the active literal can use its contextual result as a typing goal.
    // Completed body facts remain in Proof::types and carry the checked exits.
    closure_result_goal: Option<(SemanticDefinitionId, SemanticTypeRecipe)>,
    integer_initializers: BTreeMap<SemanticDefinitionId, BTreeMap<NodeId, IntegerInitializer>>,
    contextual_code_goals: code_context::Goals,
    contextual_code_need: Option<code_context::Demand>,
    contextual_code_inputs: BTreeMap<SemanticDefinitionId, BTreeSet<u32>>,
    contextual_code_calls: BTreeSet<SemanticDefinitionId>,
    predicate_active: Vec<(SemanticDefinitionId, u8, SemanticTypeRecipe)>,
    stage_depth: usize,
    cleanup_depth: usize,
    requested_snapshot: Option<(SemanticDefinitionId, SemanticOriginalNode)>,
    snapshot_sources:
        Option<std::rc::Rc<BTreeMap<(SemanticDefinitionId, bool, NodeId), Vec<(usize, usize)>>>>,
    escape_active: Vec<SemanticTypeRecipe>,
}
#[derive(Default)]
struct LoopFrame {
    breaks: Vec<BTreeMap<String, Binding>>,
    continues: Vec<BTreeMap<String, Binding>>,
}
enum ProofAccess<'a> {
    Owned(Proof),
    Borrowed(&'a Proof),
}
impl Default for ProofAccess<'_> {
    fn default() -> Self {
        Self::Owned(Proof::default())
    }
}
impl std::ops::Deref for ProofAccess<'_> {
    type Target = Proof;
    fn deref(&self) -> &Proof {
        match self {
            Self::Owned(proof) => proof,
            Self::Borrowed(proof) => proof,
        }
    }
}
impl ProofAccess<'_> {
    fn proof_mut(&mut self) -> R<&mut Proof> {
        match self {
            Self::Owned(proof) => Ok(proof),
            Self::Borrowed(_) => Err(pending(
                "read-only original where cannot add or change a symbolic proof fact",
            )),
        }
    }
    fn into_owned(self) -> R<Proof> {
        match self {
            Self::Owned(proof) => Ok(proof),
            Self::Borrowed(_) => Err(bad(
                "read-only original where cannot publish an owned proof",
            )),
        }
    }
}
impl<'a> Check<'a, '_> {
    fn observe_capture(
        &mut self,
        owner: SemanticDefinitionId,
        name: &str,
        value: &Binding,
        depth: usize,
    ) -> R<()> {
        if !value.capture {
            return Ok(());
        }
        let names_work = self.proof.capture_names.get(&owner).map_or(0, |names| {
            names
                .iter()
                .fold(names.len(), |n, old| n.saturating_add(old.len()))
        });
        self.pay(
            name.len()
                .saturating_add(names_work)
                .saturating_add(self.proof.capture_names.len().saturating_add(1).ilog2() as usize)
                .saturating_add(self.proof.kinds.len().saturating_add(1).ilog2() as usize)
                .saturating_add(4),
            depth,
        )?;
        let names = self
            .proof
            .proof_mut()?
            .capture_names
            .entry(owner)
            .or_default();
        if !names.iter().any(|old| old == name) {
            names.push(name.to_owned());
        }
        let observed = if value.written {
            self.pay((self.capture_writes.len().saturating_add(1).ilog2() as usize + 1).saturating_add(name.len() + 1), depth)?;
            let width = self.capture_writes.get(&owner).map_or(0, BTreeSet::len);
            self.pay((width.saturating_add(1).ilog2() as usize + 1).saturating_mul(name.len() + 1), depth)?;
            self.capture_writes.entry(owner).or_default().insert(name.to_owned());
            CallableKind::FnMut
        } else {
            CallableKind::Fn
        };
        let kind = self
            .proof
            .proof_mut()?
            .kinds
            .entry(owner)
            .or_insert(CallableKind::Fn);
        if observed == CallableKind::FnOnce
            || observed == CallableKind::FnMut && *kind == CallableKind::Fn
        {
            *kind = observed;
        }
        Ok(())
    }
    fn capture_exit(
        &mut self,
        owner: SemanticDefinitionId,
        variables: &BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<()> {
        self.pay(variables.len().saturating_add(1), depth)?;
        for value in variables.values() {
            let mut value = Some(value);
            while let Some(binding) = value {
                self.pay(1, depth)?;
                if binding.capture
                    && binding.observed
                    && (binding.used || !binding.taken_fields.is_empty())
                {
                    self.pay(
                        self.proof.kinds.len().saturating_add(1).ilog2() as usize + 2,
                        depth,
                    )?;
                    self.proof
                        .proof_mut()?
                        .kinds
                        .insert(owner, CallableKind::FnOnce);
                }
                value = binding.shadowed.as_deref();
            }
        }
        Ok(())
    }
    fn pay(&mut self, n: usize, depth: usize) -> R<()> {
        *self.left = self.left.checked_sub(n).ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "original symbolic typing work exceeded", None)
        })?;
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "original symbolic typing depth exceeded",
                None,
            ));
        }
        Ok(())
    }
    fn definition(&mut self, id: SemanticDefinitionId) -> R<&'a SemanticDefinition> {
        self.pay(
            self.proof
                .definition_positions
                .len()
                .saturating_add(1)
                .ilog2() as usize
                + 2,
            0,
        )?;
        self.proof
            .definition_positions
            .get(&id)
            .and_then(|index| self.program.definitions.get(*index))
            .filter(|definition| definition.id == id)
            .ok_or_else(|| bad("symbolic type references an absent original definition"))
    }
    fn same(&mut self, a: &SemanticTypeRecipe, b: &SemanticTypeRecipe) -> R<()> {
        let result = self.same_inner(a, b);
        match result {
            Ok(()) => Ok(()),
            Err(mut diagnostic) => {
                self.pay(diagnostic.notes.len(), 0)?;
                if !diagnostic
                    .notes
                    .iter()
                    .any(|note| note.starts_with("original type heads:"))
                {
                    self.pay(256, 0)?;
                    diagnostic.notes.push(format!(
                        "original type heads: actual {}, wanted {}",
                        type_head(a),
                        type_head(b)
                    ));
                }
                Err(diagnostic)
            }
        }
    }
    fn same_query_alias(
        &mut self,
        owner: SemanticDefinitionId,
        query: &SemanticTypeRecipe,
        other: &SemanticTypeRecipe,
        reverse: bool,
    ) -> R<()> {
        self.pay(
            self.predicate_active.len().saturating_add(1),
            self.predicate_active.len(),
        )?;
        if self
            .predicate_active
            .iter()
            .any(|(domain, kind, active)| *domain == owner && *kind == 8 && active == query)
        {
            return Err(pending(
                "original type query has a cyclic defining equation",
            ));
        }
        let active = copy_recipe(query, self.limits, self.left)?;
        self.predicate_active.push((owner, 8, active));
        let answer = (|| {
            let resolved = if matches!(query, SemanticTypeRecipe::Environment { .. }) {
                self.observation_type(owner, query, 1)?
            } else {
                self.resolve(owner, query, 1)?
            };
            if &resolved == query {
                if self.premise_type_equal(owner, query, other, 1)? {
                    return Ok(());
                }
                // An abstract projection may be equal to another projection
                // whose unique original provider has a complete RHS. Resolve
                // both operands before concluding that the equation is absent;
                // never infer an equation for the abstract side from a layout.
                if matches!(
                    other,
                    SemanticTypeRecipe::AssociatedType { .. }
                        | SemanticTypeRecipe::OriginalType { .. }
                        | SemanticTypeRecipe::Environment { .. }
                ) {
                    let other_value = if matches!(other, SemanticTypeRecipe::Environment { .. }) {
                        self.observation_type(owner, other, 1)?
                    } else {
                        self.resolve(owner, other, 1)?
                    };
                    if &other_value != other {
                        return if reverse {
                            self.same(&other_value, query)
                        } else {
                            self.same(query, &other_value)
                        };
                    }
                }
                self.pay(768, 0)?;
                let mut diagnostic = pending("original type query has no finite defining equation");
                fn label(ty: &SemanticTypeRecipe) -> String {
                    match ty {
                        SemanticTypeRecipe::AssociatedType {
                            receiver,
                            application,
                            member,
                        } => {
                            let receiver = match receiver.as_ref() {
                                SemanticTypeRecipe::Formal(reference) => {
                                    format!("Formal({reference:?})")
                                }
                                SemanticTypeRecipe::SelfType { scope } => {
                                    format!("Self({scope:?})")
                                }
                                SemanticTypeRecipe::Nominal {
                                    definition,
                                    arguments,
                                } => format!("Nominal({definition:?};args={})", arguments.len()),
                                other => type_head(other).to_owned(),
                            };
                            format!(
                                "Associated({:?};member={member};receiver={receiver};args={})",
                                application.declaration,
                                application.arguments.len()
                            )
                        }
                        SemanticTypeRecipe::Environment { scope, ordinal } => {
                            format!("Environment({scope:?};slot={ordinal})")
                        }
                        SemanticTypeRecipe::OriginalType { scope, source } => {
                            let source = match source {
                                SemanticOriginalTypeSource::Expression(node) => {
                                    format!("Expression({node:?})")
                                }
                                SemanticOriginalTypeSource::Binding(node) => {
                                    format!("Binding({node:?})")
                                }
                                SemanticOriginalTypeSource::Parameter(ordinal) => {
                                    format!("Parameter({ordinal})")
                                }
                                SemanticOriginalTypeSource::Result => "Result".into(),
                                _ => "StructuredOriginalQuery".into(),
                            };
                            format!("Original({scope:?};{source})")
                        }
                        other => type_head(other).to_owned(),
                    }
                }
                diagnostic.notes.push(format!(
                    "original equation owner {owner:?}: query {}, other {}",
                    label(query),
                    label(other)
                ));
                return Err(diagnostic);
            }
            if reverse {
                self.same(other, &resolved)
            } else {
                self.same(&resolved, other)
            }
        })();
        self.predicate_active.pop();
        answer
    }
    fn same_inner(&mut self, a: &SemanticTypeRecipe, b: &SemanticTypeRecipe) -> R<()> {
        crate::dyn_metadata::semantic_operand_snapshot(a, self.limits, self.left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(b, self.limits, self.left, 1)?;
        if a == b
            || matches!((a, b),
            (SemanticTypeRecipe::Tuple(fields), SemanticTypeRecipe::Atom(Type::Unit))
            | (SemanticTypeRecipe::Atom(Type::Unit), SemanticTypeRecipe::Tuple(fields)) if fields.is_empty())
        {
            return Ok(());
        }
        if let Some(owner) = self.contexts.last().copied() {
            if matches!(
                a,
                SemanticTypeRecipe::OriginalType { .. }
                    | SemanticTypeRecipe::AssociatedType { .. }
                    | SemanticTypeRecipe::Environment {
                        scope: SemanticScopeRef::Ancestor(0),
                        ..
                    }
            ) {
                return self.same_query_alias(owner, a, b, false);
            }
            if matches!(
                b,
                SemanticTypeRecipe::OriginalType { .. }
                    | SemanticTypeRecipe::AssociatedType { .. }
                    | SemanticTypeRecipe::Environment {
                        scope: SemanticScopeRef::Ancestor(0),
                        ..
                    }
            ) {
                return self.same_query_alias(owner, b, a, true);
            }
            if self.premise_type_equal(owner, a, b, 0)? {
                return Ok(());
            }
        }
        match (a, b) {
            (SemanticTypeRecipe::Tuple(a), SemanticTypeRecipe::Tuple(b)) if a.len() == b.len() => {
                for (a, b) in a.iter().zip(b) {
                    self.same(a, b)?;
                }
                return Ok(());
            }
            (
                SemanticTypeRecipe::Array {
                    element: a,
                    length: al,
                },
                SemanticTypeRecipe::Array {
                    element: b,
                    length: bl,
                },
            ) => {
                self.same(a, b)?;
                if self.same_array_length(al, bl)? {
                    return Ok(());
                }
            }
            (
                SemanticTypeRecipe::Nominal {
                    definition: a,
                    arguments: aa,
                },
                SemanticTypeRecipe::Nominal {
                    definition: b,
                    arguments: ba,
                },
            ) if a == b && aa.len() == ba.len() => {
                for (a, b) in aa.iter().zip(ba) {
                    self.same_actual(a, b)?;
                }
                return Ok(());
            }
            (SemanticTypeRecipe::Opaque { definition: a, arguments: aa, hidden: ah },
                SemanticTypeRecipe::Opaque { definition: b, arguments: ba, hidden: bh })
                if a == b && aa.len() == ba.len() => {
                self.pay(aa.len(), 0)?;
                if self.definition(*a)?.parameters.len() != aa.len() {
                    return Err(bad("opaque equality omits full original mixed actuals"));
                }
                for (a, b) in aa.iter().zip(ba) { self.same_actual(a, b)?; }
                self.original_opaque_result(*a, ah, 1)?;
                self.original_opaque_result(*b, bh, 1)?;
                return Ok(());
            }
            (
                SemanticTypeRecipe::Address {
                    mutable: am,
                    lifetime: al,
                    inner: a,
                },
                SemanticTypeRecipe::Address {
                    mutable: bm,
                    lifetime: bl,
                    inner: b,
                },
            ) if am == bm => {
                if let (SemanticOuterLifeRecipe::Class(a), SemanticOuterLifeRecipe::Class(b)) =
                    (al, bl)
                {
                    if !self.same_life(a, b)? {
                        return Err(bad("original address changes a rigid lifetime anchor"));
                    }
                } else if al != bl {
                    return Err(bad("original address changes a static lifetime endpoint"));
                }
                return self.same(a, b);
            }
            (
                SemanticTypeRecipe::Desc {
                    lifetime: al,
                    inner: a,
                },
                SemanticTypeRecipe::Desc {
                    lifetime: bl,
                    inner: b,
                },
            ) => {
                if let (SemanticOuterLifeRecipe::Class(a), SemanticOuterLifeRecipe::Class(b)) =
                    (al, bl)
                {
                    if !self.same_life(a, b)? {
                        return Err(bad("original descriptor changes a rigid lifetime anchor"));
                    }
                } else if al != bl {
                    return Err(bad(
                        "original descriptor changes a static lifetime endpoint",
                    ));
                }
                return self.same(a, b);
            }
            (
                SemanticTypeRecipe::Task {
                    result: a,
                    retained: al,
                },
                SemanticTypeRecipe::Task {
                    result: b,
                    retained: bl,
                },
            ) => {
                self.same(a, b)?;
                let owner = self
                    .contexts
                    .last()
                    .copied()
                    .ok_or_else(|| bad("original Task comparison has no lexical Gamma"))?;
                return self.same_task_shape(owner, al, bl, 0);
            }
            (
                SemanticTypeRecipe::Dyn {
                    owned: a,
                    interface: ai,
                },
                SemanticTypeRecipe::Dyn {
                    owned: b,
                    interface: bi,
                },
            ) if a == b => return self.same_dyn_interface(ai, bi, 0),
            (SemanticTypeRecipe::Managed(a), SemanticTypeRecipe::Managed(b))
            | (SemanticTypeRecipe::OwnedStore(a), SemanticTypeRecipe::OwnedStore(b))
            | (SemanticTypeRecipe::Mutex(a), SemanticTypeRecipe::Mutex(b))
            | (SemanticTypeRecipe::Slice(a), SemanticTypeRecipe::Slice(b))
            | (SemanticTypeRecipe::Registration(a), SemanticTypeRecipe::Registration(b))
            | (
                SemanticTypeRecipe::AsyncRegistration(a),
                SemanticTypeRecipe::AsyncRegistration(b),
            ) => {
                return self.same(a, b);
            }
            (
                SemanticTypeRecipe::ErrorUnion {
                    error: a,
                    result: ar,
                },
                SemanticTypeRecipe::ErrorUnion {
                    error: b,
                    result: br,
                },
            ) => {
                self.same(a, b)?;
                return self.same(ar, br);
            }
            _ => {}
        }
        Err(bad(
            "original body changes a rigid whole type or lifetime anchor",
        ))
    }
    fn same_actual(&mut self, a: &SemanticActualRecipe, b: &SemanticActualRecipe) -> R<()> {
        self.pay(1, 0)?;
        match (a, b) {
            (SemanticActualRecipe::Type(a), SemanticActualRecipe::Type(b)) => self.same(a, b),
            (SemanticActualRecipe::Life(a), SemanticActualRecipe::Life(b))
                if self.same_life(a, b)? =>
            {
                Ok(())
            }
            (
                SemanticActualRecipe::Const {
                    carrier: a,
                    value: av,
                },
                SemanticActualRecipe::Const {
                    carrier: b,
                    value: bv,
                },
            ) => {
                self.same(a, b)?;
                if self.same_const(a, av, bv)? {
                    Ok(())
                } else {
                    Err(bad("original equality changes a rigid Const payload"))
                }
            }
            _ if a == b => Ok(()),
            _ => Err(bad(
                "original equality changes a rigid mixed argument or carrier",
            )),
        }
    }
    fn same_array_length(&mut self, a: &SemanticConstRecipe, b: &SemanticConstRecipe) -> R<bool> {
        self.pay(2, 0)?;
        if a == b {
            return Ok(true);
        }
        Ok(matches!((self.closed_array_length(a, 0)?, self.closed_array_length(b, 0)?), (Some(a), Some(b)) if a == b))
    }
    fn closed_array_length(&mut self, value: &SemanticConstRecipe, depth: usize) -> R<Option<u64>> {
        let value = match value {
            SemanticConstRecipe::Value(value) => value,
            SemanticConstRecipe::IntegerLiteral(text) => {
                self.pay(text.len().saturating_mul(3), depth)?;
                return Ok(match crate::integer(text, Some(&Type::Usize), false, Span::default()) {
                    Ok(Constant::Usize(count)) => Some(count),
                    _ => None,
                });
            }
            _ => return Ok(None),
        };
        Ok(match value {
            Constant::Usize(v) | Constant::U64(v) => Some(*v),
            Constant::U32(v) => Some(u64::from(*v)),
            Constant::I32(v) => u64::try_from(*v).ok(),
            Constant::I64(v) => u64::try_from(*v).ok(),
            _ => None,
        })
    }
    fn copy_type(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<bool> {
        self.pay(self.predicate_active.len().saturating_add(1), depth)?;
        if self
            .predicate_active
            .iter()
            .any(|(active_owner, kind, active)| {
                *active_owner == owner && *kind == 0 && active == ty
            })
        {
            return Ok(false);
        }
        crate::dyn_metadata::semantic_operand_snapshot(ty, self.limits, self.left, 1)?;
        self.predicate_active.push((owner, 0, ty.clone()));
        let result = self.copy_type_inner(owner, ty, depth + 1);
        self.predicate_active.pop();
        result
    }
    fn copy_type_inner(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<bool> {
        self.pay(1, depth)?;
        // A declared Copy obligation is about a complete original type. It is
        // equally meaningful for an associated projection or a nominal type;
        // it is not restricted to a bare Type formal's syntax.
        for requirement in self.original_requirements(owner, depth + 1)?.iter() {
            let premise = match requirement {
                SemanticRequirementRecipe::Structural {
                    ty,
                    bound: DynStructuralBound::Copy,
                }
                | SemanticRequirementRecipe::Builtin {
                    ty,
                    trait_kind: SemanticBuiltinTrait::Copy,
                } => Some(ty),
                _ => None,
            };
            if let Some(premise) = premise {
                if self.premise_type_equal(owner, ty, premise, depth + 1)? {
                    return Ok(true);
                }
            }
        }
        for equivalent in self.equal_types(owner, ty, depth + 1)? {
            if self.copy_type(owner, &equivalent, depth + 1)? {
                return Ok(true);
            }
        }
        Ok(match ty {
            SemanticTypeRecipe::Atom(ty) => ty.is_copy_value(),
            SemanticTypeRecipe::OriginalType { source: SemanticOriginalTypeSource::Binding(_), .. } => {
                matches!(self.resolve(owner, ty, depth + 1)?, SemanticTypeRecipe::Atom(ty) if ty.is_integer())
            }
            SemanticTypeRecipe::Address { mutable: false, .. } => true,
            SemanticTypeRecipe::Code { kind, signature } => self
                .function_value_copy(owner, *kind, signature, depth + 1)?
                .ok_or_else(|| pending("original code value needs its complete Copy scheme"))?,
            SemanticTypeRecipe::Array { element, .. } => {
                self.copy_type(owner, element, depth + 1)?
            }
            SemanticTypeRecipe::ErrorUnion { error, result } => {
                self.copy_type(owner, error, depth + 1)?
                    && self.copy_type(owner, result, depth + 1)?
            }
            SemanticTypeRecipe::Tuple(fields) => {
                let mut all = true;
                for field in fields {
                    all &= self.copy_type(owner, field, depth + 1)?;
                }
                all
            }
            SemanticTypeRecipe::Closure { definition, environment, .. } => {
                // Capability follows every actual captured value, never the
                // entry's Fn/FnMut kind or its result's receiver upper.
                self.pay(self.proof.complete.len().saturating_add(1).ilog2() as usize
                    + self.proof.borrowed_capture_heads.len().saturating_add(1).ilog2() as usize + 2, depth)?;
                if !self.proof.complete.contains(definition)
                    || self.proof.borrowed_capture_heads.contains(definition)
                { false } else {
                    self.pay(environment.len(), depth)?;
                    let mut all = true;
                    for actual in environment {
                        match actual {
                            SemanticActualRecipe::Type(ty) => all &= self.copy_type(owner, ty, depth + 1)?,
                            SemanticActualRecipe::Life(life) => {
                                crate::dyn_metadata::semantic_operand_snapshot(life, self.limits, self.left, 1)?;
                            }
                            SemanticActualRecipe::Const { carrier, value } => {
                                crate::dyn_metadata::semantic_operand_snapshot(value, self.limits, self.left, 1)?;
                                all &= self.copy_type(owner, carrier, depth + 1)?;
                            }
                            SemanticActualRecipe::Infer => all = false,
                        }
                    }
                    all
                }
            }
            SemanticTypeRecipe::Opaque { definition, arguments, hidden } => {
                let declaration = self.definition(*definition)?;
                if declaration.parent.is_some() || !declaration.environment.is_empty()
                    || !self.proof.complete.contains(definition)
                    || !matches!(&declaration.body, SemanticDefinitionBody::FunctionWithBody {
                        signature: SemanticSignatureRecipe { result: SemanticTypeRecipe::Opaque { definition: declared, .. }, .. },
                        foreign: None, body: SemanticOriginalFunctionBody::Defined(_), ..
                    } if declared == definition)
                { false } else {
                    let result = self.opaque_application_result(owner, *definition, arguments, hidden, depth + 1)?;
                    self.contexts.push(*definition);
                    let declared = self.copy_type(*definition, &result, depth + 1);
                    self.contexts.pop();
                    if declared? { return Ok(true); }
                    if let SemanticTypeRecipe::Closure {
                        definition,
                        environment,
                        ..
                    } = &result
                    {
                        let heads = self.proof.borrowed_capture_heads.len();
                        self.pay(
                            environment.len().saturating_add(
                                heads.saturating_add(1).ilog2() as usize + 1,
                            ),
                            depth,
                        )?;
                        // Specializing the creator's formals cannot make an
                        // exclusive address capture or a borrowed capture head
                        // duplicable. Their actual loan images are checked at
                        // the result boundary, independently of this predicate.
                        if self.proof.borrowed_capture_heads.contains(definition)
                            || environment.iter().any(|actual| {
                                matches!(
                                    actual,
                                    SemanticActualRecipe::Type(SemanticTypeRecipe::Address {
                                        mutable: true,
                                        ..
                                    })
                                )
                            })
                        {
                            return Ok(false);
                        }
                    }
                    let result = super::instantiate::ty(self.program, &self.proof, &result,
                        super::instantiate::DomainMapping { source: *definition, use_definition: owner,
                            arguments, parent_call: None }, self.limits, self.left)?;
                    self.copy_type(owner, &result, depth + 1)?
                }
            }
            SemanticTypeRecipe::Environment {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            } => {
                let (parent, ty) = self
                    .proof
                    .environments
                    .get(&(owner, *ordinal))
                    .ok_or_else(|| pending("sealed whole-Type Copy premise is not established"))?;
                let parent = *parent;
                let ty = copy_recipe(ty, self.limits, self.left)?;
                self.copy_type(parent, &ty, depth + 1)?
            }
            SemanticTypeRecipe::Formal(reference) => {
                let SemanticScopeRef::Ancestor(hops) = reference.scope;
                let mut target = owner;
                for step in 0..hops {
                    self.pay(1, depth + step as usize)?;
                    target = self
                        .definition(target)?
                        .parent
                        .ok_or_else(|| bad("original Copy formal escapes its lexical scope"))?;
                }
                let definition = self.definition(target)?;
                let canonical = SemanticTypeRecipe::Formal(SemanticFormalRef {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal: reference.ordinal,
                });
                let mut found = false;
                if let Some(SemanticFormal::Type { requirements }) =
                    definition.parameters.get(reference.ordinal as usize)
                {
                    for requirement in requirements {
                        self.pay(1, depth)?;
                        if matches!(requirement, SemanticRequirementRecipe::Structural { ty: actual, bound: DynStructuralBound::Copy } | SemanticRequirementRecipe::Builtin { ty: actual, trait_kind: SemanticBuiltinTrait::Copy } if actual == &canonical)
                        {
                            found = true;
                        }
                    }
                }
                for requirement in &definition.requirements {
                    self.pay(1, depth)?;
                    if matches!(requirement, SemanticRequirementRecipe::Structural { ty: actual, bound: DynStructuralBound::Copy } | SemanticRequirementRecipe::Builtin { ty: actual, trait_kind: SemanticBuiltinTrait::Copy } if actual == &canonical)
                    {
                        found = true;
                    }
                }
                if !found {
                    for equivalent in self.equal_types(owner, ty, depth + 1)? {
                        if self.copy_type(owner, &equivalent, depth + 1)? {
                            found = true;
                            break;
                        }
                    }
                }
                found
            }
            SemanticTypeRecipe::Nominal {
                definition,
                arguments,
            } => {
                let definition = self.definition(*definition)?;
                match &definition.body {
                    SemanticDefinitionBody::Nominal {
                        kind: LifeNominalKind::Record,
                        body: SemanticNominalBody::Product { fields },
                        ..
                    } => {
                        let mut all = true;
                        for field in fields {
                            let field = self.instantiate_field(
                                owner,
                                definition.id,
                                &field.ty,
                                arguments,
                                depth + 1,
                            )?;
                            all &= self.copy_type(owner, &field, depth + 1)?;
                        }
                        all
                    }
                    SemanticDefinitionBody::Nominal {
                        kind: LifeNominalKind::Enum,
                        body: SemanticNominalBody::Enum { variants },
                        ..
                    } => {
                        let mut all = true;
                        for variant in variants {
                            for field in &variant.fields {
                                let field = self.instantiate_field(
                                    owner,
                                    definition.id,
                                    field,
                                    arguments,
                                    depth + 1,
                                )?;
                                all &= self.copy_type(owner, &field, depth + 1)?;
                            }
                        }
                        all
                    }
                    _ => false,
                }
            }
            _ => false,
        })
    }
    fn place(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(1, depth)?;
        self.snapshot(
            owner,
            SemanticOriginalNode::Expression(expression.id),
            variables,
        )?;
        let ty = match &expression.kind {
            CoreExprKind::Name(name) => {
                let value = variables
                    .get_mut(name)
                    .ok_or_else(|| bad("original place has no lexical binder"))?;
                if value.used || !value.initialized || !value.taken_fields.is_empty() {
                    return Err(bad("original place was already transferred"));
                }
                let integer = self.binding_integer(value, None, depth + 1)?;
                value.observed = true;
                self.observe_capture(owner, name, value, depth + 1)?;
                if let Some(integer) = integer {
                    Ok(integer)
                } else if value.capture
                    && value.borrowed
                    && let SemanticTypeRecipe::Managed(inner) =
                        self.observation_type(owner, &value.ty, depth + 1)?
                {
                    // A default resource capture already stores a description.
                    // Even selecting a field directly observes that exact
                    // free-name occurrence, without transferring the owner.
                    Ok(SemanticTypeRecipe::Desc {
                        lifetime: self.borrow_life(owner, expression.id)?,
                        inner,
                    })
                } else {
                    copy_recipe(&value.ty, self.limits, self.left)
                }
            }
            CoreExprKind::Group(inner) => self.place(owner, inner, variables, depth + 1),
            CoreExprKind::Member { object, member, .. } => {
                if let Some((field, _)) =
                    self.local_inline_field(owner, object, member, variables, true, true, depth + 1)?
                {
                    Ok(field)
                } else {
                    let owner_type = self.place(owner, object, variables, depth + 1)?;
                    self.field(owner, &owner_type, member, depth + 1)
                }
            }
            CoreExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } => {
                let pointer = self.observe_address_operand(owner, operand, variables, depth + 1)?;
                let pointer = self.observation_type(owner, &pointer, depth + 1)?;
                let SemanticTypeRecipe::Address { inner, .. } = pointer else {
                    return Err(bad("original dereference place is not an address"));
                };
                Ok(*inner)
            }
            _ => self.expr(owner, expression, variables, None, depth + 1),
        }?;
        let copy = copy_recipe(&ty, self.limits, self.left)?;
        self.proof
            .proof_mut()?
            .types
            .insert((owner, Query::Expression(expression.id)), copy);
        self.expression_result_origin(owner, expression, variables, &ty, depth + 1)?;
        Ok(ty)
    }
    fn instantiate_field(
        &mut self,
        owner: SemanticDefinitionId,
        source: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        arguments: &[SemanticActualRecipe],
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(1, depth)?;
        super::instantiate::ty(
            self.program,
            &self.proof,
            ty,
            super::instantiate::DomainMapping {
                source,
                use_definition: owner,
                arguments,
                parent_call: None,
            },
            self.limits,
            self.left,
        )
    }
    fn instantiate_template(
        &mut self,
        ty: &SemanticTypeRecipe,
        arguments: &[SemanticActualRecipe],
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(arguments.len(), depth)?;
        let frames = [(0, arguments)];
        let empty = BTreeMap::new();
        substitute::ty(
            ty,
            &substitute::Substitution {
                frames: &frames,
                selves: &[],
                environments: &[],
                implicit: &empty,
                invocation: &empty,
                shift: 0,
            },
            self.limits,
            self.left,
        )
    }
    fn field(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        name: &str,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(name.len(), depth)?;
        let ty = self.resolve(owner, ty, depth + 1)?;
        if let SemanticTypeRecipe::Environment {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal,
        } = &ty
        {
            let (parent, actual) =
                self.proof
                    .environments
                    .get(&(owner, *ordinal))
                    .ok_or_else(|| {
                        pending("sealed whole-Type field awaits its parent symbolic source")
                    })?;
            let parent = *parent;
            let actual = copy_recipe(actual, self.limits, self.left)?;
            let field = self.field(parent, &actual, name, depth + 1)?;
            let child = self.definition(owner)?;
            return self.rehome(parent, child, &field, depth + 1);
        }
        if let SemanticTypeRecipe::Address { inner, .. }
        | SemanticTypeRecipe::Desc { inner, .. }
        | SemanticTypeRecipe::Managed(inner) = &ty
        {
            return self.field(owner, inner, name, depth + 1);
        }
        if let SemanticTypeRecipe::Tuple(fields) = &ty {
            let ordinal = name
                .parse::<usize>()
                .map_err(|_| bad("original tuple field is not an ordinal"))?;
            return copy_recipe(
                fields
                    .get(ordinal)
                    .ok_or_else(|| bad("original tuple field is absent"))?,
                self.limits,
                self.left,
            );
        }
        if matches!(ty, SemanticTypeRecipe::Atom(_)) {
            return Err(Diagnostic::error(
                Phase::Types,
                "E-FIELD-TYPE",
                None,
                format!("field {name} requires a tuple or declared product, actual operand is {}", type_head(&ty)),
            ));
        }
        let SemanticTypeRecipe::Nominal {
            definition,
            arguments,
        } = ty
        else {
            return Err(pending(
                "original field requires a visible complete nominal shape",
            ));
        };
        let definition = self.definition(definition)?;
        let SemanticDefinitionBody::Nominal {
            body: SemanticNominalBody::Product { fields },
            ..
        } = &definition.body
        else {
            return Err(bad("original field selects another nominal constructor"));
        };
        self.pay(fields.len(), depth)?;
        let field = fields
            .iter()
            .find(|field| field.name == name)
            .ok_or_else(|| bad("original field is absent from its declared nominal"))?;
        self.instantiate_field(owner, definition.id, &field.ty, &arguments, depth + 1)
    }
    // Only declaration paths are resolved here. A lexical value at the root
    // always wins, including when it shadows an imported module or intrinsic.
    fn declaration_path(
        &mut self,
        expression: &SemanticOriginalExpr,
        variables: &BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<Option<Vec<String>>> {
        self.pay(1, depth)?;
        match &expression.kind {
            CoreExprKind::Name(name) if !variables.contains_key(name) => {
                self.pay(name.len().saturating_add(1), depth)?;
                Ok(Some(vec![name.clone()]))
            }
            CoreExprKind::Member { object, member, .. } => {
                let Some(mut path) = self.declaration_path(object, variables, depth + 1)? else {
                    return Ok(None);
                };
                self.pay(member.len().saturating_add(1), depth)?;
                path.push(member.clone());
                Ok(Some(path))
            }
            CoreExprKind::TypeApply { callee, .. }
            | CoreExprKind::GenericApply { callee, .. }
            | CoreExprKind::Group(callee) => self.declaration_path(callee, variables, depth + 1),
            _ => Ok(None),
        }
    }
    fn declaration_target(
        &mut self,
        owner: SemanticDefinitionId,
        path: &[String],
        depth: usize,
    ) -> R<Option<SemanticDefinitionId>> {
        let current = self.definition(owner)?;
        let SemanticDefinitionBody::FunctionWithBody { names, .. } = &current.body else {
            return Err(bad(
                "original declaration lookup has no complete lexical body",
            ));
        };
        self.pay(names.len().saturating_add(path.len()), depth)?;
        let mut selected = None;
        for binding in names {
            if binding.namespace != SemanticOriginalNamespace::Value || binding.path != path {
                continue;
            }
            if selected.is_some() {
                return Err(bad("original path has ambiguous declaration bindings"));
            }
            let SemanticOriginalNameValue::Definition(target) = binding.value else {
                return Err(pending(
                    "original callee recipe requires its complete callable interpretation",
                ));
            };
            selected = Some(target);
        }
        Ok(selected)
    }

    fn prepared_shared_address(
        &mut self,
        owner: SemanticDefinitionId,
        bindings: &[(String, SemanticOriginalExpr)],
        invocation: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<Option<SemanticTypeRecipe>> {
        let [(temporary, input)] = bindings else { return Ok(None) };
        let CoreExprKind::Call { callee, arguments } = &invocation.kind else { return Ok(None) };
        let [argument] = arguments.as_slice() else { return Ok(None) };
        let CoreExprKind::Name(input_name) = &input.kind else { return Ok(None) };
        self.pay(temporary.len().saturating_add(input_name.len()).saturating_add(1), depth)?;
        if !matches!(&argument.kind, CoreExprKind::Name(name) if name == temporary)
            || !variables.contains_key(input_name)
        {
            return Ok(None);
        }
        let Some(path) = self.declaration_path(callee, variables, depth + 1)? else { return Ok(None) };
        let Some(target) = self.declaration_target(owner, &path, depth + 1)? else { return Ok(None) };
        let definition = self.definition(target)?;
        let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &definition.body else { return Ok(None) };
        let [parameter] = signature.parameters.as_slice() else { return Ok(None) };
        if parameter.mode != PassingMode::Borrow
            || !matches!(parameter.ty, SemanticTypeRecipe::Address { mutable: false, .. })
            || !self.site(owner, SemanticOriginalNode::Expression(invocation.id))?.is_some_and(|site| {
                matches!(&site.kind, SemanticSiteRecipeKind::OriginalCall)
                    || matches!(&site.kind, SemanticSiteRecipeKind::Call { target: expected, .. } if *expected == target)
            })
        {
            return Ok(None);
        }
        let source = self.place(owner, input, variables, depth + 1)?;
        let SemanticTypeRecipe::Address { lifetime, inner, .. } = self.observation_type(owner, &source, depth + 1)? else {
            return Err(bad("original prepared shared Address requires an address input"));
        };
        self.record_expression_type(owner, input.id, source)?;
        self.prepared_input_view_bound(owner, input, argument.id, variables, depth + 1)?;
        Ok(Some(SemanticTypeRecipe::Address { mutable: false, lifetime, inner }))
    }
    fn rehome(
        &mut self,
        parent: SemanticDefinitionId,
        child: &SemanticDefinition,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        let parent_definition = self.definition(parent)?;
        self.pay(
            child
                .environment
                .len()
                .saturating_add(parent_definition.parameters.len()),
            depth,
        )?;
        let mut actuals = Vec::new();
        for (ordinal, formal) in parent_definition.parameters.iter().enumerate() {
            let selector = SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal: ordinal as u32,
            };
            let source = match formal {
                SemanticFormal::Lifetime => {
                    SemanticActualRecipe::Life(SemanticLifeRecipe::Formal(selector))
                }
                SemanticFormal::Type { .. } => {
                    SemanticActualRecipe::Type(SemanticTypeRecipe::Formal(selector))
                }
                SemanticFormal::Const { carrier } => SemanticActualRecipe::Const {
                    carrier: copy_recipe(carrier, self.limits, self.left)?,
                    value: SemanticConstRecipe::Formal(selector),
                },
            };
            let mut replacement = None;
            for (slot, environment) in child.environment.iter().enumerate() {
                self.pay(1, depth)?;
                let captured_formal = matches!((&source, &environment.source),
                    (SemanticActualRecipe::Type(formal), SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType { .. }))
                    if self.proof.environments.get(&(child.id, slot as u32))
                        .is_some_and(|(owner, original)| *owner == parent && original == formal));
                if environment.source == source || captured_formal {
                    replacement = Some(match formal {
                        SemanticFormal::Lifetime => {
                            SemanticActualRecipe::Life(SemanticLifeRecipe::Environment {
                                scope: SemanticScopeRef::Ancestor(0),
                                ordinal: slot as u32,
                            })
                        }
                        SemanticFormal::Type { .. } => {
                            SemanticActualRecipe::Type(SemanticTypeRecipe::Environment {
                                scope: SemanticScopeRef::Ancestor(0),
                                ordinal: slot as u32,
                            })
                        }
                        SemanticFormal::Const { carrier } => SemanticActualRecipe::Const {
                            carrier: {
                                let empty = BTreeMap::new();
                                substitute::ty(
                                    carrier,
                                    &substitute::Substitution {
                                        frames: &[],
                                        selves: &[],
                                        environments: &[],
                                        implicit: &empty,
                                        invocation: &empty,
                                        shift: 1,
                                    },
                                    self.limits,
                                    self.left,
                                )?
                            },
                            value: SemanticConstRecipe::Environment {
                                scope: SemanticScopeRef::Ancestor(0),
                                ordinal: slot as u32,
                            },
                        },
                    });
                }
            }
            let actual = if let Some(replacement) = replacement {
                replacement
            } else {
                let empty = BTreeMap::new();
                substitute::actual(
                    &source,
                    &substitute::Substitution {
                        frames: &[],
                        selves: &[],
                        environments: &[],
                        implicit: &empty,
                        invocation: &empty,
                        shift: 1,
                    },
                    self.limits,
                    self.left,
                )?
            };
            actuals.push(actual);
        }
        let frames = [(0, actuals.as_slice())];
        let empty = BTreeMap::new();
        substitute::ty(
            ty,
            &substitute::Substitution {
                frames: &frames,
                selves: &[],
                environments: &[],
                implicit: &empty,
                invocation: &empty,
                shift: 1,
            },
            self.limits,
            self.left,
        )
    }
    fn charge_binding_clone(&mut self, binding: &Binding) -> R<()> {
        self.pay(
            binding.origin.copy_work()
                .saturating_add(binding.taken_fields.len())
                .saturating_add(1),
            0,
        )?;
        for field in &binding.taken_fields {
            self.pay(field.len(), 0)?;
        }
        Ok(())
    }
    fn same_binding_type(&mut self, a: &Binding, b: &Binding) -> R<()> {
        self.pay(1, 0)?;
        if std::rc::Rc::ptr_eq(&a.ty, &b.ty) {
            return Ok(());
        }
        self.same(&a.ty, &b.ty)
    }
    fn charge_variables(&mut self, variables: &BTreeMap<String, Binding>) -> R<()> {
        self.pay(variables.len(), 0)?;
        for (name, binding) in variables {
            self.pay(name.len(), 0)?;
            self.charge_binding_clone(binding)?;
        }
        Ok(())
    }
    fn clone_variables(
        &mut self,
        variables: &BTreeMap<String, Binding>,
    ) -> R<BTreeMap<String, Binding>> {
        self.charge_variables(variables)?;
        Ok(variables.clone())
    }
    fn snapshot(
        &mut self,
        owner: SemanticDefinitionId,
        at: SemanticOriginalNode,
        variables: &BTreeMap<String, Binding>,
    ) -> R<()> {
        if self.requested_snapshot == Some((owner, at)) {
            let (expression, node) = match at {
                SemanticOriginalNode::Expression(node) => (true, node),
                SemanticOriginalNode::Statement(node) => (false, node),
            };
            if !self
                .proof
                .frozen_environments
                .contains_key(&(owner, expression, node))
            {
                let snapshot = self.clone_variables(variables)?;
                self.pay(1, 0)?;
                self.proof
                    .proof_mut()?
                    .frozen_environments
                    .insert((owner, expression, node), snapshot);
            }
        }
        if self.snapshot_sources.is_none() {
            let program = self.program;
            self.pay(program.definitions.len().saturating_add(1), 0)?;
            let mut index =
                BTreeMap::<(SemanticDefinitionId, bool, NodeId), Vec<(usize, usize)>>::new();
            let mut rows = 0usize;
            for (child_position, child) in program.definitions.iter().enumerate() {
                let Some(parent) = child.parent else {
                    continue;
                };
                self.pay(child.environment.len(), 0)?;
                for (formal_position, formal) in child.environment.iter().enumerate() {
                    let SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType {
                        scope: SemanticScopeRef::Ancestor(0),
                        source: SemanticOriginalTypeSource::NameAt { at, .. },
                    }) = &formal.source
                    else {
                        continue;
                    };
                    rows = rows
                        .checked_add(1)
                        .ok_or_else(|| bad("original snapshot index row count overflows"))?;
                    if rows > self.limits.max_nodes {
                        return Err(Diagnostic::limit(
                            Phase::Types,
                            "original snapshot index node limit exceeded",
                            None,
                        ));
                    }
                    let (expression, node) = match at {
                        SemanticOriginalNode::Expression(node) => (true, *node),
                        SemanticOriginalNode::Statement(node) => (false, *node),
                    };
                    self.pay(index.len().saturating_add(1).ilog2() as usize + 2, 0)?;
                    let positions = index.entry((parent, expression, node)).or_default();
                    let relocation = if positions.len() == positions.capacity() {
                        positions.len()
                    } else {
                        0
                    };
                    self.pay(relocation.saturating_add(1), 0)?;
                    positions.push((child_position, formal_position));
                }
            }
            self.snapshot_sources = Some(std::rc::Rc::new(index));
        }
        self.pay(1, 0)?;
        let index = self
            .snapshot_sources
            .as_ref()
            .ok_or_else(|| bad("original snapshot index disappeared"))?
            .clone();
        let (expression, node) = match at {
            SemanticOriginalNode::Expression(node) => (true, node),
            SemanticOriginalNode::Statement(node) => (false, node),
        };
        self.pay(index.len().saturating_add(1).ilog2() as usize + 1, 0)?;
        for (child_position, formal_position) in
            index.get(&(owner, expression, node)).into_iter().flatten()
        {
            self.pay(1, 0)?;
            let formal = self
                .program
                .definitions
                .get(*child_position)
                .and_then(|child| child.environment.get(*formal_position))
                .ok_or_else(|| bad("original snapshot index leaves its immutable directory"))?;
            let SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source:
                    SemanticOriginalTypeSource::NameAt {
                        at: requested,
                        name,
                    },
            }) = &formal.source
            else {
                return Err(bad(
                    "original snapshot index changes its complete source kind",
                ));
            };
            if *requested != at {
                return Err(bad(
                    "original snapshot index selects another exact original occurrence",
                ));
            }
            self.pay(name.len().saturating_add(variables.len()), 0)?;
            let value = variables.get(name).ok_or_else(|| {
                bad("original NameAt has no lexical binder at its exact source point")
            })?;
            self.pay(value.origin.copy_work(), 0)?;
            let result_origin = if value.origin_owner == owner {
                self.proof
                    .call_result_origins
                    .get(&(owner, value.origin.clone()))
                    .copied()
            } else {
                None
            };
            let value = copy_recipe(&value.ty, self.limits, self.left)?;
            let (expression, node) = match at {
                SemanticOriginalNode::Expression(node) => (true, node),
                SemanticOriginalNode::Statement(node) => (false, node),
            };
            if let Some(origin) = result_origin {
                self.pay(name.len().saturating_add(2), 0)?;
                self.proof.proof_mut()?.call_result_origins.insert(
                    (
                        owner,
                        Query::NameAt {
                            expression,
                            node,
                            name: name.clone(),
                        },
                    ),
                    origin,
                );
            }
            self.proof.proof_mut()?.types.insert(
                (
                    owner,
                    Query::NameAt {
                        expression,
                        node,
                        name: name.clone(),
                    },
                ),
                value,
            );
        }
        Ok(())
    }
    fn resolve(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(1, depth)?;
        if matches!(ty, SemanticTypeRecipe::Tuple(fields) if fields.is_empty()) {
            return Ok(SemanticTypeRecipe::Atom(Type::Unit));
        }
        if matches!(
            ty,
            SemanticTypeRecipe::Atom(_)
                | SemanticTypeRecipe::Formal(_)
                | SemanticTypeRecipe::Exact(_)
                | SemanticTypeRecipe::Environment {
                    scope: SemanticScopeRef::Ancestor(1..),
                    ..
                }
        ) {
            return copy_recipe(ty, self.limits, self.left);
        }
        if let SemanticTypeRecipe::AssociatedType {
            receiver,
            application,
            member,
        } = ty
        {
            return self.resolve_associated(owner, receiver, application, *member, depth + 1);
        }
        if let SemanticTypeRecipe::SelfType {
            scope: SemanticScopeRef::Ancestor(hops),
        } = ty
        {
            let mut target = owner;
            for step in 0..*hops {
                self.pay(1, depth + step as usize)?;
                target = self
                    .definition(target)?
                    .parent
                    .ok_or_else(|| bad("original Self escapes its real lexical parent"))?;
            }
            let definition = self.definition(target)?;
            if let SemanticDefinitionBody::Scope(
                SemanticScopeRecipe::InherentImpl { self_type, .. }
                | SemanticScopeRecipe::TraitImpl { self_type, .. }
                | SemanticScopeRecipe::BuiltinImpl { self_type, .. },
            ) = &definition.body
            {
                let empty = BTreeMap::new();
                let substitution = substitute::Substitution {
                    frames: &[],
                    selves: &[],
                    environments: &[],
                    implicit: &empty,
                    invocation: &empty,
                    shift: *hops,
                };
                let self_type = substitute::ty(self_type, &substitution, self.limits, self.left)?;
                return self.resolve(owner, &self_type, depth + 1);
            }
            if matches!(definition.body, SemanticDefinitionBody::Nominal { .. }) {
                self.pay(definition.parameters.len(), depth)?;
                let mut arguments = Vec::new();
                for (ordinal, formal) in definition.parameters.iter().enumerate() {
                    let reference = SemanticFormalRef {
                        scope: SemanticScopeRef::Ancestor(*hops),
                        ordinal: ordinal as u32,
                    };
                    let actual = match formal {
                        SemanticFormal::Type { .. } => {
                            SemanticActualRecipe::Type(SemanticTypeRecipe::Formal(reference))
                        }
                        SemanticFormal::Lifetime => {
                            SemanticActualRecipe::Life(SemanticLifeRecipe::Formal(reference))
                        }
                        SemanticFormal::Const { carrier } => {
                            let empty = BTreeMap::new();
                            let carrier = substitute::ty(
                                carrier,
                                &substitute::Substitution {
                                    frames: &[],
                                    selves: &[],
                                    environments: &[],
                                    implicit: &empty,
                                    invocation: &empty,
                                    shift: *hops,
                                },
                                self.limits,
                                self.left,
                            )?;
                            SemanticActualRecipe::Const {
                                carrier,
                                value: SemanticConstRecipe::Formal(reference),
                            }
                        }
                    };
                    arguments.push(actual);
                }
                return Ok(SemanticTypeRecipe::Nominal {
                    definition: target,
                    arguments,
                });
            }
            if !matches!(
                definition.body,
                SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { .. })
            ) {
                return Err(bad(
                    "original Self does not name a trait or implementation scope",
                ));
            }
            return copy_recipe(ty, self.limits, self.left);
        }
        if let SemanticTypeRecipe::OriginalType { scope, source } = ty {
            let mut target = owner;
            let SemanticScopeRef::Ancestor(hops) = scope;
            for step in 0..*hops {
                self.pay(1, depth + step as usize)?;
                target = self
                    .definition(target)?
                    .parent
                    .ok_or_else(|| bad("original type query escapes its lexical parent"))?;
            }
            if let SemanticOriginalTypeSource::Binding(binding) = source {
                self.infer_integer_binding(target, *binding, None, depth + 1)?;
            }
            let value = self
                .proof
                .types
                .get(&(target, Query::from(source.clone())))
                .ok_or_else(|| {
                    pending("original type query awaits its symbolic binding/expression equation")
                })?;
            let value = copy_recipe(value, self.limits, self.left)?;
            if target != owner {
                let current = self.definition(owner)?;
                if current.parent != Some(target) {
                    return Err(pending(
                        "original ancestor query needs every checked sealed lexical boundary",
                    ));
                }
                self.pay(current.environment.len(), depth)?;
                for (slot, formal) in current.environment.iter().enumerate() {
                    if matches!(&formal.source, SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType { scope: SemanticScopeRef::Ancestor(0), source: actual }) if actual == source)
                    {
                        return Ok(SemanticTypeRecipe::Environment {
                            scope: SemanticScopeRef::Ancestor(0),
                            ordinal: slot as u32,
                        });
                    }
                }
                return self.rehome(target, current, &value, depth + 1);
            }
            return Ok(value);
        }
        let mut value = copy_recipe(ty, self.limits, self.left)?;
        if let SemanticTypeRecipe::Opaque {
            definition, hidden, ..
        } = &mut value
        {
            if *definition == owner {
                **hidden = self.resolve(owner, hidden, depth + 1)?;
            }
        }
        let delayed = substitute::delayed_types(&value, self.limits, self.left)?;
        self.pay(delayed.len(), depth)?;
        let mut equations = Vec::new();
        for occurrence in delayed {
            let resolved = if matches!(occurrence.ty, SemanticTypeRecipe::Environment { .. }) {
                self.observation_type(owner, &occurrence.ty, depth + 1)?
            } else {
                self.resolve(owner, &occurrence.ty, depth + 1)?
            };
            if resolved != occurrence.ty {
                if occurrence.bound_depth != 0 || occurrence.invocation_depth != 0 {
                    return Err(pending(
                        "associated/query normalization requires its complete nested quantifier occurrence context",
                    ));
                }
                self.pay(1, depth)?;
                equations.push((occurrence, resolved));
            }
        }
        if equations.is_empty() {
            Ok(value)
        } else {
            substitute::replace_delayed(&value, &equations, self.limits, self.left)
        }
    }
    // Unfold only a whole-Type environment already sealed from the immutable
    // lexical creator. This is a logical view, not a runtime read or a new premise.
    fn logical(
        &mut self,
        owner: SemanticDefinitionId,
        value: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.observation_type(owner, value, depth + 1)
    }
    fn observation_type(
        &mut self,
        owner: SemanticDefinitionId,
        value: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(1, depth)?;
        if let SemanticTypeRecipe::Environment {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal,
        } = value
        {
            let (parent, actual) = self
                .proof
                .environments
                .get(&(owner, *ordinal))
                .ok_or_else(|| bad("original observation has no sealed lexical Type source"))?;
            let parent = *parent;
            let actual = copy_recipe(actual, self.limits, self.left)?;
            let actual = self.observation_type(parent, &actual, depth + 1)?;
            let child = self.definition(owner)?;
            let mut value = self.rehome(parent, child, &actual, depth + 1)?;
            match &mut value {
                SemanticTypeRecipe::Address {
                    mutable, lifetime, ..
                } if !matches!(lifetime, SemanticOuterLifeRecipe::Static) => {
                    self.pay(2, depth)?;
                    *lifetime =
                        SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::OuterOccurrence {
                            operand: SemanticRecipeOperand::Environment {
                                scope: SemanticScopeRef::Ancestor(0),
                                ordinal: *ordinal,
                            },
                            path: Vec::new(),
                            mode: if *mutable {
                                StoredBorrowMode::ExclusiveAddress
                            } else {
                                StoredBorrowMode::SharedAddress
                            },
                        });
                }
                SemanticTypeRecipe::Desc { lifetime, .. }
                    if !matches!(lifetime, SemanticOuterLifeRecipe::Static) =>
                {
                    self.pay(2, depth)?;
                    *lifetime =
                        SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::OuterOccurrence {
                            operand: SemanticRecipeOperand::Environment {
                                scope: SemanticScopeRef::Ancestor(0),
                                ordinal: *ordinal,
                            },
                            path: Vec::new(),
                            mode: StoredBorrowMode::WritableDescription,
                        });
                }
                _ => {}
            }
            return Ok(value);
        }
        self.resolve(owner, value, depth + 1)
    }
    fn resolved_actual(
        &mut self,
        owner: SemanticDefinitionId,
        value: &SemanticActualRecipe,
        depth: usize,
    ) -> R<SemanticActualRecipe> {
        self.pay(1, depth)?;
        Ok(match value {
            SemanticActualRecipe::Type(value) => {
                SemanticActualRecipe::Type(self.resolve(owner, value, depth + 1)?)
            }
            SemanticActualRecipe::Life(value) => {
                crate::dyn_metadata::semantic_operand_snapshot(value, self.limits, self.left, 1)?;
                SemanticActualRecipe::Life(value.clone())
            }
            SemanticActualRecipe::Const { carrier, value } => {
                crate::dyn_metadata::semantic_operand_snapshot(value, self.limits, self.left, 1)?;
                SemanticActualRecipe::Const {
                    carrier: self.resolve(owner, carrier, depth + 1)?,
                    value: value.clone(),
                }
            }
            SemanticActualRecipe::Infer => {
                return Err(bad(
                    "original closure environment contains unresolved inference",
                ));
            }
        })
    }
    fn site(
        &mut self,
        owner: SemanticDefinitionId,
        node: SemanticOriginalNode,
    ) -> R<Option<&'a SemanticSiteRecipe>> {
        let definition = self.definition(owner)?;
        let SemanticDefinitionBody::FunctionWithBody {
            sites, site_nodes, ..
        } = &definition.body
        else {
            return Err(bad("original operation has no complete body directory"));
        };
        let directory = self.site_directory(owner, sites, site_nodes)?;
        let mut found = None;
        for &id in directory.at(node, self.left)? {
            let site = directory.recipe(owner, sites, id, self.left)?
                .ok_or_else(|| bad("original node directory references an absent recipe"))?;
            // One expression may have a type-use and its actual operation.
            if matches!(
                site.kind,
                SemanticSiteRecipeKind::Call { .. }
                    | SemanticSiteRecipeKind::CallableCall { .. }
                    | SemanticSiteRecipeKind::OriginalCall
                    | SemanticSiteRecipeKind::Closure { .. }
                    | SemanticSiteRecipeKind::Construct { .. }
            ) && found.replace(site).is_some() {
                return Err(bad("original expression has multiple operation recipes"));
            }
        }
        Ok(found)
    }
    fn site_directory(
        &mut self,
        owner: SemanticDefinitionId,
        recipes: &[SemanticSiteRecipe],
        rows: &[SemanticOriginalSiteNode],
    ) -> R<std::rc::Rc<sites::DefinitionSites>> {
        match self.proof.original_sites.owner(owner, self.left)? {
            Some(directory) => Ok(directory),
            None => Ok(std::rc::Rc::new(sites::DefinitionSites::build(
                owner, recipes, rows, self.left,
            )?)),
        }
    }
    fn borrow_life(
        &mut self,
        owner: SemanticDefinitionId,
        node: NodeId,
    ) -> R<SemanticOuterLifeRecipe> {
        self.borrow_occurrence(owner, node, false)
    }
    fn borrow_occurrence(
        &mut self, owner: SemanticDefinitionId, node: NodeId, receiver_call: bool,
    ) -> R<SemanticOuterLifeRecipe> {
        let definition = self.definition(owner)?;
        let SemanticDefinitionBody::FunctionWithBody {
            sites, site_nodes, ..
        } = &definition.body
        else {
            return Err(bad("borrow has no original body"));
        };
        let directory = self.site_directory(owner, sites, site_nodes)?;
        let mut selected = None;
        for &id in directory.at(SemanticOriginalNode::Expression(node), self.left)? {
            let Some(site) = directory.recipe(owner, sites, id, self.left)? else { continue; };
            let matches = if receiver_call {
                matches!(site.kind, SemanticSiteRecipeKind::OriginalCall)
            } else {
                matches!(site.kind, SemanticSiteRecipeKind::TypeUse { .. })
            };
            if !matches {
                continue;
            }
            self.pay(definition.implicit_lifetimes.len(), 0)?;
            for implicit in &definition.implicit_lifetimes {
                if implicit.occurrence_path == site.occurrence_path
                    && selected.replace(implicit.ordinal).is_some()
                {
                    return Err(bad(
                        "one original borrow has ambiguous declaration occurrences",
                    ));
                }
            }
        }
        Ok(SemanticOuterLifeRecipe::Class(
            SemanticLifeRecipe::Implicit {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal: selected.ok_or_else(|| {
                    pending("original borrow needs its own explicit declaration occurrence")
                })?,
            },
        ))
    }
    fn bind_actual(
        &mut self,
        ordinal: u32,
        actual: SemanticActualRecipe,
        solutions: &mut [Option<SemanticActualRecipe>],
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        let slot = solutions
            .get_mut(ordinal as usize)
            .ok_or_else(|| bad("original inference formal is absent"))?;
        if let Some(old) = slot {
            crate::dyn_metadata::semantic_operand_snapshot(old, self.limits, self.left, 1)?;
            crate::dyn_metadata::semantic_operand_snapshot(&actual, self.limits, self.left, 1)?;
            match (old, &actual) {
                (SemanticActualRecipe::Type(old), SemanticActualRecipe::Type(actual)) => {
                    self.same(old, actual)?
                }
                (SemanticActualRecipe::Life(old), SemanticActualRecipe::Life(actual))
                    if self.same_life(old, actual)? => {}
                (
                    SemanticActualRecipe::Const {
                        carrier: a,
                        value: av,
                    },
                    SemanticActualRecipe::Const {
                        carrier: b,
                        value: bv,
                    },
                ) => {
                    self.same(a, b)?;
                    if !self.same_const(a, av, bv)? {
                        return Err(bad(
                            "original fresh inference has incompatible rigid Const actuals",
                        ));
                    }
                }
                (old, actual) if old == actual => {}
                _ => {
                    return Err(bad(
                        "original fresh inference has incompatible rigid actuals",
                    ));
                }
            }
        } else {
            *slot = Some(actual);
        }
        Ok(())
    }
    fn unify_outer(
        &mut self,
        template: &SemanticOuterLifeRecipe,
        actual: &SemanticOuterLifeRecipe,
        formals: &[SemanticFormal],
        solutions: &mut [Option<SemanticActualRecipe>],
        implicit: &mut BTreeMap<u32, SemanticOuterLifeRecipe>,
        invocation: &mut BTreeMap<u32, SemanticOuterLifeRecipe>,
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        let role = match template {
            SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Implicit {
                scope: SemanticScopeRef::Ancestor(u32::MAX),
                ordinal,
            }) => Some((false, *ordinal)),
            SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Environment {
                scope: SemanticScopeRef::Ancestor(u32::MAX),
                ordinal,
            }) => Some((true, *ordinal)),
            _ => None,
        };
        if let Some((is_invocation, ordinal)) = role {
            let map = if is_invocation { invocation } else { implicit };
            if let Some(old) = map.get(&ordinal) {
                if old != actual {
                    return Err(bad(
                        "original invocation role has incompatible outer lifetime sources",
                    ));
                }
            } else {
                crate::dyn_metadata::semantic_operand_snapshot(actual, self.limits, self.left, 1)?;
                map.insert(ordinal, actual.clone());
            }
            return Ok(());
        }
        if let (
            SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Formal(SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(u32::MAX),
                ordinal,
            })),
            SemanticOuterLifeRecipe::Class(actual),
        ) = (template, actual)
        {
            if !matches!(
                formals.get(*ordinal as usize),
                Some(SemanticFormal::Lifetime)
            ) {
                return Err(bad(
                    "original Life inference selects another mixed formal kind",
                ));
            }
            crate::dyn_metadata::semantic_operand_snapshot(actual, self.limits, self.left, 1)?;
            return self.bind_actual(
                *ordinal,
                SemanticActualRecipe::Life(actual.clone()),
                solutions,
                depth + 1,
            );
        }
        if template != actual {
            if let (SemanticOuterLifeRecipe::Class(a), SemanticOuterLifeRecipe::Class(b)) =
                (template, actual)
            {
                if self.same_life(a, b)? {
                    return Ok(());
                }
            }
            return Err(bad(
                "original call changes a rigid lifetime or static endpoint",
            ));
        }
        Ok(())
    }
    fn unify_actual(
        &mut self,
        template: &SemanticActualRecipe,
        actual: &SemanticActualRecipe,
        formals: &[SemanticFormal],
        solutions: &mut [Option<SemanticActualRecipe>],
        implicit: &mut BTreeMap<u32, SemanticOuterLifeRecipe>,
        invocation: &mut BTreeMap<u32, SemanticOuterLifeRecipe>,
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        match (template, actual) {
            (SemanticActualRecipe::Type(a), SemanticActualRecipe::Type(b)) => {
                self.unify(a, b, formals, solutions, implicit, invocation, depth + 1)
            }
            (SemanticActualRecipe::Life(a), SemanticActualRecipe::Life(b)) => {
                crate::dyn_metadata::semantic_operand_snapshot(a, self.limits, self.left, 1)?;
                crate::dyn_metadata::semantic_operand_snapshot(b, self.limits, self.left, 1)?;
                self.unify_outer(
                    &SemanticOuterLifeRecipe::Class(a.clone()),
                    &SemanticOuterLifeRecipe::Class(b.clone()),
                    formals,
                    solutions,
                    implicit,
                    invocation,
                    depth + 1,
                )
            }
            (
                SemanticActualRecipe::Const {
                    carrier: a,
                    value: av,
                },
                SemanticActualRecipe::Const {
                    carrier: b,
                    value: bv,
                },
            ) => {
                self.unify(a, b, formals, solutions, implicit, invocation, depth + 1)?;
                if let SemanticConstRecipe::Formal(SemanticFormalRef {
                    scope: SemanticScopeRef::Ancestor(u32::MAX),
                    ordinal,
                }) = av
                {
                    if !matches!(
                        formals.get(*ordinal as usize),
                        Some(SemanticFormal::Const { .. })
                    ) {
                        return Err(bad(
                            "original Const inference selects another mixed formal kind",
                        ));
                    }
                    crate::dyn_metadata::semantic_operand_snapshot(
                        actual,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    self.bind_actual(*ordinal, actual.clone(), solutions, depth + 1)
                } else if av == bv {
                    Ok(())
                } else {
                    Err(bad("original call changes a rigid Const payload"))
                }
            }
            _ => Err(bad("original call changes a complete mixed argument kind")),
        }
    }
    fn unify(
        &mut self,
        template: &SemanticTypeRecipe,
        actual: &SemanticTypeRecipe,
        formals: &[SemanticFormal],
        solutions: &mut [Option<SemanticActualRecipe>],
        implicit: &mut BTreeMap<u32, SemanticOuterLifeRecipe>,
        invocation: &mut BTreeMap<u32, SemanticOuterLifeRecipe>,
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        match (template, actual) {
            (
                SemanticTypeRecipe::Formal(SemanticFormalRef {
                    scope: SemanticScopeRef::Ancestor(u32::MAX),
                    ordinal,
                }),
                _,
            ) if matches!(
                formals.get(*ordinal as usize),
                Some(SemanticFormal::Type { .. })
            ) =>
            {
                let actual =
                    SemanticActualRecipe::Type(copy_recipe(actual, self.limits, self.left)?);
                self.bind_actual(*ordinal, actual, solutions, depth + 1)?;
            }
            (
                SemanticTypeRecipe::Address {
                    mutable: requested,
                    lifetime,
                    inner,
                },
                SemanticTypeRecipe::Address {
                    mutable: available,
                    lifetime: actual_life,
                    inner: actual_inner,
                },
            ) => {
                if *requested && !available {
                    return Err(bad(
                        "original call grants exclusive access through a shared actual",
                    ));
                }
                self.unify(
                    inner,
                    actual_inner,
                    formals,
                    solutions,
                    implicit,
                    invocation,
                    depth + 1,
                )?;
                self.unify_outer(
                    lifetime,
                    actual_life,
                    formals,
                    solutions,
                    implicit,
                    invocation,
                    depth + 1,
                )?;
            }
            (
                SemanticTypeRecipe::Desc { lifetime, inner },
                SemanticTypeRecipe::Desc {
                    lifetime: actual_life,
                    inner: actual_inner,
                },
            ) => {
                self.unify(
                    inner,
                    actual_inner,
                    formals,
                    solutions,
                    implicit,
                    invocation,
                    depth + 1,
                )?;
                self.unify_outer(
                    lifetime,
                    actual_life,
                    formals,
                    solutions,
                    implicit,
                    invocation,
                    depth + 1,
                )?;
            }
            (SemanticTypeRecipe::Tuple(left), SemanticTypeRecipe::Tuple(right))
                if left.len() == right.len() =>
            {
                for (left, right) in left.iter().zip(right) {
                    self.unify(
                        left,
                        right,
                        formals,
                        solutions,
                        implicit,
                        invocation,
                        depth + 1,
                    )?;
                }
            }
            (
                SemanticTypeRecipe::Nominal {
                    definition: a,
                    arguments: av,
                },
                SemanticTypeRecipe::Nominal {
                    definition: b,
                    arguments: bv,
                },
            ) if a == b && av.len() == bv.len() => {
                for (a, b) in av.iter().zip(bv) {
                    self.unify_actual(a, b, formals, solutions, implicit, invocation, depth + 1)?;
                }
            }
            (SemanticTypeRecipe::Opaque { definition: a, arguments: aa, hidden: ah },
                SemanticTypeRecipe::Opaque { definition: b, arguments: ba, hidden: bh })
                if a == b && aa.len() == ba.len() => {
                self.pay(aa.len(), depth)?;
                if self.definition(*a)?.parameters.len() != aa.len() {
                    return Err(bad("opaque inference omits full original mixed actuals"));
                }
                for (a, b) in aa.iter().zip(ba) {
                    self.unify_actual(a, b, formals, solutions, implicit, invocation, depth + 1)?;
                }
                self.original_opaque_result(*a, ah, depth + 1)?;
                self.original_opaque_result(*b, bh, depth + 1)?;
            }
            (
                SemanticTypeRecipe::Array {
                    element: a,
                    length: al,
                },
                SemanticTypeRecipe::Array {
                    element: b,
                    length: bl,
                },
            ) => {
                self.unify(a, b, formals, solutions, implicit, invocation, depth + 1)?;
                if let SemanticConstRecipe::Formal(SemanticFormalRef {
                    scope: SemanticScopeRef::Ancestor(u32::MAX),
                    ordinal,
                }) = al.as_ref()
                {
                    let Some(SemanticFormal::Const { carrier }) = formals.get(*ordinal as usize)
                    else {
                        return Err(bad("original array length selects a non-Const formal"));
                    };
                    crate::dyn_metadata::semantic_operand_snapshot(
                        bl.as_ref(),
                        self.limits,
                        self.left,
                        1,
                    )?;
                    let actual = SemanticActualRecipe::Const {
                        carrier: copy_recipe(carrier, self.limits, self.left)?,
                        value: bl.as_ref().clone(),
                    };
                    self.bind_actual(*ordinal, actual, solutions, depth + 1)?;
                } else if !self.same_array_length(al, bl)? {
                    return Err(bad("original call changes a rigid array length"));
                }
            }
            (SemanticTypeRecipe::Slice(a), SemanticTypeRecipe::Slice(b))
            | (SemanticTypeRecipe::Managed(a), SemanticTypeRecipe::Managed(b))
            | (SemanticTypeRecipe::OwnedStore(a), SemanticTypeRecipe::OwnedStore(b))
            | (SemanticTypeRecipe::Mutex(a), SemanticTypeRecipe::Mutex(b))
            | (SemanticTypeRecipe::Registration(a), SemanticTypeRecipe::Registration(b))
            | (
                SemanticTypeRecipe::AsyncRegistration(a),
                SemanticTypeRecipe::AsyncRegistration(b),
            ) => self.unify(a, b, formals, solutions, implicit, invocation, depth + 1)?,
            (
                SemanticTypeRecipe::Task {
                    result: a,
                    retained: left,
                },
                SemanticTypeRecipe::Task {
                    result: b,
                    retained: right,
                },
            ) => {
                self.unify(a, b, formals, solutions, implicit, invocation, depth + 1)?;
                self.unify_task_shape(
                    left,
                    right,
                    formals,
                    solutions,
                    implicit,
                    invocation,
                    depth + 1,
                )?;
            }
            (
                SemanticTypeRecipe::Dyn {
                    owned: a,
                    interface: ai,
                },
                SemanticTypeRecipe::Dyn {
                    owned: b,
                    interface: bi,
                },
            ) if a == b => self.unify_dyn_interface(
                ai,
                bi,
                formals,
                solutions,
                implicit,
                invocation,
                depth + 1,
            )?,
            (
                SemanticTypeRecipe::ErrorUnion {
                    error: a,
                    result: ar,
                },
                SemanticTypeRecipe::ErrorUnion {
                    error: b,
                    result: br,
                },
            ) => {
                self.unify(a, b, formals, solutions, implicit, invocation, depth + 1)?;
                self.unify(ar, br, formals, solutions, implicit, invocation, depth + 1)?;
            }
            _ => self.same(template, actual)?,
        }
        Ok(())
    }
    fn solve_signature(
        &mut self,
        target: &SemanticDefinition,
        owner: SemanticDefinitionId,
        call: &SemanticOriginalExpr,
        callee: &SemanticOriginalExpr,
        arguments: &[&SemanticOriginalExpr],
        signature: &SemanticSignatureRecipe,
        formals: &[SemanticFormal],
        written_actuals: &[SemanticActualRecipe],
        variables: &mut BTreeMap<String, Binding>,
        wanted: Option<&SemanticTypeRecipe>,
        template_ready: bool,
        depth: usize,
    ) -> R<(Vec<SemanticActualRecipe>, SemanticSignatureRecipe)> {
        self.solve_signature_roles(
            owner,
            call,
            callee,
            arguments,
            signature,
            formals,
            written_actuals,
            variables,
            wanted,
            template_ready,
            &BTreeMap::new(),
            &[],
            0,
            None,
            Some(target),
            depth,
        )
    }
    fn caller_parameter_context(
        &mut self,
        parameter: &SemanticParameterRecipe,
        formals: &[SemanticFormal],
        solutions: &[Option<SemanticActualRecipe>],
        implicit: &BTreeMap<u32, SemanticOuterLifeRecipe>,
        invocation: &BTreeMap<u32, SemanticOuterLifeRecipe>,
        depth: usize,
    ) -> R<Option<SemanticTypeRecipe>> {
        self.pay(solutions.len(), depth)?;
        for actual in solutions.iter().flatten() {
            crate::dyn_metadata::semantic_operand_snapshot(actual, self.limits, self.left, 1)?;
        }
        let mut partial_actuals = Vec::new();
        for (ordinal, (formal, solution)) in formals.iter().zip(solutions).enumerate() {
            let reference = SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(u32::MAX),
                ordinal: ordinal as u32,
            };
            partial_actuals.push(if let Some(actual) = solution {
                actual.clone()
            } else {
                match formal {
                    SemanticFormal::Lifetime => {
                        SemanticActualRecipe::Life(SemanticLifeRecipe::Formal(reference))
                    }
                    SemanticFormal::Type { .. } => {
                        SemanticActualRecipe::Type(SemanticTypeRecipe::Formal(reference))
                    }
                    SemanticFormal::Const { carrier } => SemanticActualRecipe::Const {
                        carrier: copy_recipe(carrier, self.limits, self.left)?,
                        value: SemanticConstRecipe::Formal(reference),
                    },
                }
            });
        }
        let frames = [(u32::MAX, partial_actuals.as_slice())];
        substitute::caller_context(
            &parameter.ty,
            &substitute::Substitution {
                frames: &frames,
                selves: &[],
                environments: &[],
                implicit,
                invocation,
                shift: 0,
            },
            self.limits,
            self.left,
        )
    }
    fn caller_signature_template(
        &mut self,
        owner: SemanticDefinitionId,
        signature: &SemanticSignatureRecipe,
        formals: &[SemanticFormal],
        invocation_origins: &BTreeMap<u32, call_regions::Role>,
        template_ready: bool,
        depth: usize,
    ) -> R<(SemanticSignatureRecipe, BTreeMap<u32, call_regions::Role>)> {
        crate::dyn_metadata::semantic_operand_snapshot(signature, self.limits, self.left, 1)?;
        let mut signature = signature.clone();
        for parameter in &mut signature.parameters {
            let ty = copy_recipe(&parameter.ty, self.limits, self.left)?;
            parameter.ty = self.mode_type(owner, parameter.mode, ty, depth + 1)?;
        }
        let result_type = copy_recipe(&signature.result, self.limits, self.left)?;
        signature.result = self.mode_type(owner, signature.result_mode, result_type, depth + 1)?;
        self.pay(invocation_origins.len(), depth)?;
        let mut invocation_origins = invocation_origins.clone();
        if signature.bound_lifetimes != 0 {
            for index in 0..signature.bound_lifetimes {
                self.pay(1, depth)?;
                invocation_origins.insert(
                    signature
                        .invocation_lifetimes
                        .checked_add(index)
                        .ok_or_else(|| bad("original call Bound directory overflows"))?,
                    call_regions::Role::Bound { depth: 0, index },
                );
            }
            signature = substitute::universal_callable_own(&signature, self.limits, self.left)?;
        }
        let signature = if template_ready {
            signature
        } else {
            self.inference_template(&signature, formals, true, depth + 1)?
        };
        Ok((signature, invocation_origins))
    }
    fn solve_signature_roles(
        &mut self,
        owner: SemanticDefinitionId,
        call: &SemanticOriginalExpr,
        callee: &SemanticOriginalExpr,
        arguments: &[&SemanticOriginalExpr],
        signature: &SemanticSignatureRecipe,
        formals: &[SemanticFormal],
        written_actuals: &[SemanticActualRecipe],
        variables: &mut BTreeMap<String, Binding>,
        wanted: Option<&SemanticTypeRecipe>,
        template_ready: bool,
        invocation_origins: &BTreeMap<u32, call_regions::Role>,
        roles: &[(NodeId, SemanticTypeRecipe)],
        parameter_offset: u32,
        contextual_callee: Option<SemanticDefinitionId>,
        declaration: Option<&SemanticDefinition>,
        depth: usize,
    ) -> R<(Vec<SemanticActualRecipe>, SemanticSignatureRecipe)> {
        self.pay(arguments.len().saturating_add(formals.len()), depth)?;
        if signature.parameters.len() != arguments.len() || written_actuals.len() != formals.len() {
            return Err(bad(
                "original call omits complete parameters or mixed actuals",
            ));
        }
        let (signature, invocation_origins) = self.caller_signature_template(
            owner, signature, formals, invocation_origins, template_ready, depth,
        )?;
        let signature = &signature;
        let mut solutions = self.written(owner, callee, formals, variables, depth + 1)?;
        for (slot, actual) in solutions.iter_mut().zip(written_actuals) {
            if !matches!(actual, SemanticActualRecipe::Infer) {
                crate::dyn_metadata::semantic_operand_snapshot(actual, self.limits, self.left, 1)?;
                if slot.as_ref().is_some_and(|old| old != actual) {
                    return Err(bad("written actual differs from the original call recipe"));
                }
                *slot = Some(actual.clone());
            }
        }
        let mut implicit = BTreeMap::new();
        let mut invocation = BTreeMap::new();
        // This function's opaque result goal supplies a callable contract.
        // Its hidden Type comes from the completed return, including an
        // independently resolved factory call with its own opaque identity.
        let wanted = wanted.filter(|ty| !matches!(ty, SemanticTypeRecipe::Opaque {
            definition, hidden, ..
        } if *definition == owner && matches!(hidden.as_ref(), SemanticTypeRecipe::OriginalType {
            scope: SemanticScopeRef::Ancestor(0), source: SemanticOriginalTypeSource::Result,
        })));
        if let Some(wanted) = wanted {
            let empty = BTreeMap::new();
            if let Some(context) = substitute::caller_context(
                wanted,
                &substitute::Substitution {
                    frames: &[],
                    selves: &[],
                    environments: &[],
                    implicit: &empty,
                    invocation: &empty,
                    shift: 0,
                },
                self.limits,
                self.left,
            )? {
                self.unify(
                    &signature.result,
                    &context,
                    formals,
                    &mut solutions,
                    &mut implicit,
                    &mut invocation,
                    depth + 1,
                )?;
            }
        }
        let mut outer_inputs = Vec::new();
        for (parameter_ordinal, (argument, parameter)) in
            arguments.iter().zip(&signature.parameters).enumerate()
        {
            // Primitive context fixes unsuffixed literals without importing any
            // unsolved target inference variable into the caller's expression.
            let complete_parameter = self.caller_parameter_context(
                parameter, formals, &solutions, &implicit, &invocation, depth,
            )?;
            let authenticates_context = contextual_callee
                .and_then(|child| self.contextual_code_inputs.get(&child))
                .is_some_and(|parameters| parameters.contains(&(parameter_ordinal as u32)));
            // A direct local borrow has its own source occurrence. A prior
            // invariant argument can fix the formal's lifetime, but only the
            // separate call-site reborrow may use that selected lifetime.
            let mut ungrouped = *argument;
            while let CoreExprKind::Group(inner) = &ungrouped.kind {
                self.pay(1, depth)?;
                ungrouped = inner;
            }
            let direct_borrow = matches!(parameter.ty, SemanticTypeRecipe::Address { .. })
                && matches!(&ungrouped.kind, CoreExprKind::Unary { op: UnaryOp::Borrow | UnaryOp::BorrowMut, operand }
                    if matches!(operand.kind, CoreExprKind::Name(_)));
            let hint = if authenticates_context || direct_borrow || matches!(parameter.ty, SemanticTypeRecipe::Desc { .. }) {
                // Describe receives the existing descriptor or an observation
                // of a real freshly evaluated owner. An expected Desc cannot
                // be imposed on the producer's Managed result.
                None
            } else {
                complete_parameter.as_ref().or_else(|| {
                    matches!(parameter.ty, SemanticTypeRecipe::Atom(_)).then_some(&parameter.ty)
                })
            };
            let prepared = match (&argument.kind, &parameter.ty) {
                (CoreExprKind::Name(name), SemanticTypeRecipe::Desc { .. } | SemanticTypeRecipe::Managed(_)) => {
                    self.pay(name.len().saturating_add(variables.len().saturating_add(1).ilog2() as usize + 1), depth)?;
                    if let Some(binding) = variables.get(name)
                        && let Query::Expression(initializer) = binding.origin
                        && binding.origin_owner == owner
                        && matches!(
                            self.observation_type(owner, &binding.ty, depth + 1)?,
                            SemanticTypeRecipe::Managed(_)
                        )
                    {
                        Some(initializer)
                    } else {
                        None
                    }
                }
                _ => None,
            };
            self.pay(roles.len(), depth)?;
            let actual =
                if let Some((_, role)) = roles.iter().find(|(node, _)| *node == argument.id) {
                    copy_recipe(role, self.limits, self.left)?
                } else if prepared.is_some() && matches!(parameter.ty, SemanticTypeRecipe::Desc { .. }) {
                    self.place(owner, argument, variables, depth + 1)?
                } else if prepared.is_some() && matches!(parameter.ty, SemanticTypeRecipe::Managed(_)) {
                    self.give_place(owner, argument, variables, depth + 1)?
                } else if matches!(
                    &parameter.ty,
                    SemanticTypeRecipe::Formal(SemanticFormalRef {
                        scope: SemanticScopeRef::Ancestor(u32::MAX),
                        ..
                    })
                ) && let Some(expected) = complete_parameter.as_ref()
                    && let Some(view) =
                        self.observe_shared_input(owner, argument, expected, variables, depth + 1)?
                {
                    // T was fixed by the complete wanted/inner domain. Passing
                    // a shared view does not infer a different T from the
                    // exclusive source or consume/copy that exclusive value.
                    view
                } else if matches!(&parameter.ty, SemanticTypeRecipe::Address { .. })
                    && matches!(&ungrouped.kind, CoreExprKind::Name(_))
                {
                    self.place(owner, argument, variables, depth + 1)?
                } else {
                    self.expr(owner, argument, variables, hint, depth + 1)?
                };
            if parameter.mode == PassingMode::Give {
                self.check_callable_give_argument(owner, argument, &actual, depth + 1)?;
            }
            if parameter.mode == PassingMode::Copy && !self.copy_type(owner, &actual, depth + 1)? {
                return Err(bad(
                    "original Copy parameter lacks its caller whole-Type Copy premise",
                ));
            }
            self.inferred_type_source(
                owner,
                callee.id,
                &parameter.ty,
                &actual,
                argument,
                depth + 1,
            )?;
            // A sealed whole-Type capture may itself be the requested raw
            // carrier. Reveal its checked logical outer shape before splitting
            // inner inference from the directed call loan. The original
            // expression equation and creator source receipt stay unchanged.
            let actual = if matches!(&parameter.ty,
                SemanticTypeRecipe::Address { .. } | SemanticTypeRecipe::Desc { .. }) {
                self.observation_type(owner, &actual, depth + 1)?
            } else { actual };
            let actual = match (&parameter.ty, actual) {
                (SemanticTypeRecipe::Desc { .. }, SemanticTypeRecipe::Managed(inner)) => {
                    let parameter = parameter_offset.checked_add(u32::try_from(parameter_ordinal)
                        .map_err(|_| bad("temporary description parameter ordinal overflows"))?)
                        .ok_or_else(|| bad("temporary description parameter coordinate overflows"))?;
                    let lifetime = self.call_region_life(
                        owner, call.id, call_regions::Role::TemporaryDescription(parameter), depth + 1,
                    )?;
                    let mut producer = *argument;
                    while let CoreExprKind::Group(value) | CoreExprKind::Ascribed { value, .. } = &producer.kind {
                        self.pay(1, depth)?;
                        producer = value;
                    }
                    let receipt = TemporaryDescription {
                        argument: argument.id,
                        producer: prepared.unwrap_or(producer.id),
                    };
                    self.pay(self.proof.temporary_descriptions.len().saturating_add(1).ilog2() as usize + 1, depth)?;
                    if self.proof.proof_mut()?.temporary_descriptions
                        .insert((owner, call.id, parameter), receipt)
                        .is_some_and(|old| old != receipt)
                    {
                        return Err(bad("temporary description changes its original owning argument"));
                    }
                    SemanticTypeRecipe::Desc { lifetime, inner }
                }
                (_, actual) => actual,
            };
            let roots = match (&parameter.ty, &actual) {
                (
                    SemanticTypeRecipe::Address {
                        mutable: requested,
                        lifetime,
                        inner,
                    },
                    SemanticTypeRecipe::Address {
                        mutable: available,
                        inner: actual_inner,
                        ..
                    },
                ) if !*requested || *available => {
                    Some((lifetime, inner.as_ref(), actual_inner.as_ref(), false))
                }
                (
                    SemanticTypeRecipe::Desc { lifetime, inner },
                    SemanticTypeRecipe::Desc {
                        inner: actual_inner,
                        ..
                    },
                ) => Some((lifetime, inner.as_ref(), actual_inner.as_ref(), true)),
                _ => None,
            };
            if let Some((expected, inner, actual_inner, head)) = roots {
                self.unify(
                    inner,
                    actual_inner,
                    formals,
                    &mut solutions,
                    &mut implicit,
                    &mut invocation,
                    depth + 1,
                )?;
                // An ordinary helper receives the same exclusive authority
                // as an explicit write through this capture. Record it before
                // finalizing the original callable's access mode.
                if matches!(parameter.mode, PassingMode::Infer | PassingMode::BorrowMut)
                    && matches!(&parameter.ty, SemanticTypeRecipe::Address { mutable: true, .. })
                {
                    self.observe_exclusive_capture(owner, argument, variables, depth + 1)?;
                }
                let variable = call_regions::variable(expected);
                let selector = if let Some(variable) = variable {
                    Some(match variable {
                        call_regions::Variable::Life(ordinal) => call_regions::Role::Life(ordinal),
                        call_regions::Variable::Implicit(ordinal) => {
                            if head {
                                call_regions::Role::Head(
                                    parameter_offset
                                        .checked_add(
                                            u32::try_from(parameter_ordinal).map_err(|_| {
                                                bad("call parameter ordinal overflows")
                                            })?,
                                        )
                                        .ok_or_else(|| {
                                            bad("call parameter coordinate overflows")
                                        })?,
                                )
                            } else {
                                call_regions::Role::Implicit(ordinal)
                            }
                        }
                        call_regions::Variable::Invocation(ordinal) => invocation_origins
                            .get(&ordinal)
                            .copied()
                            .unwrap_or(call_regions::Role::Invocation(ordinal)),
                    })
                } else {
                    None
                };
                crate::dyn_metadata::semantic_operand_snapshot(
                    expected,
                    self.limits,
                    self.left,
                    1,
                )?;
                outer_inputs.push(call_regions::Input {
                    result_bound: false,
                    variable,
                    selector,
                    expression: argument,
                    actual,
                    expected: expected.clone(),
                    parameter: parameter_offset
                        .checked_add(
                            u32::try_from(parameter_ordinal)
                                .map_err(|_| bad("call original parameter ordinal overflows"))?,
                        )
                        .ok_or_else(|| bad("call original parameter coordinate overflows"))?,
                });
                continue;
            }
            self.unify(
                &parameter.ty,
                &actual,
                formals,
                &mut solutions,
                &mut implicit,
                &mut invocation,
                depth + 1,
            )?;
        }
        if let Some(wanted) = wanted {
            self.unify(
                &signature.result,
                wanted,
                formals,
                &mut solutions,
                &mut implicit,
                &mut invocation,
                depth + 1,
            )?;
        }
        if let Some(declaration) = declaration
            && let Some(ordinal) = declaration.elided_result_ordinal()
        {
            self.extend_result_inputs(declaration, signature, ordinal, &mut outer_inputs, depth + 1)?;
        }
        self.settle_call_inputs(
            owner,
            call,
            &outer_inputs,
            formals,
            &mut solutions,
            &mut implicit,
            &mut invocation,
            variables,
            depth + 1,
        )?;
        self.infer_callable_bounds(owner, formals, &mut solutions, depth + 1)?;
        self.pay(solutions.len(), depth)?;
        let mut actuals = solutions
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| {
                pending("original call's fresh mixed inference variables are not fully determined")
            })?;
        self.seal_inferred_type_sources(owner, callee.id, &mut actuals, depth + 1)?;
        let frames = [(u32::MAX, actuals.as_slice())];
        let substitution = substitute::Substitution {
            frames: &frames,
            selves: &[],
            environments: &[],
            implicit: &implicit,
            invocation: &invocation,
            shift: 0,
        };
        let signature =
            substitute::signature_fresh(signature, &substitution, self.limits, self.left)?;
        if let Some(child) = contextual_callee
            && self.contextual_code_inputs.contains_key(&child)
        {
            self.pay(1, depth)?;
            self.contextual_code_calls.insert(child);
        }
        Ok((actuals, signature))
    }
    fn ordinary_value(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<bool> {
        self.value_recipe(owner, ty, depth + 1)
    }
    fn satisfy_requirement(
        &mut self,
        owner: SemanticDefinitionId,
        requirement: &SemanticRequirementRecipe,
        depth: usize,
    ) -> R<()> {
        self.satisfy_requirement_under(owner, requirement, &[], depth)
    }
    fn satisfy_requirement_under(
        &mut self,
        owner: SemanticDefinitionId,
        requirement: &SemanticRequirementRecipe,
        universal: &[u32],
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        if self
            .original_requirements(owner, depth + 1)?
            .iter()
            .any(|premise| premise == requirement)
        {
            return Ok(());
        }
        let sealed = match requirement {
            SemanticRequirementRecipe::Builtin { ty, trait_kind } => Some((ty, *trait_kind)),
            SemanticRequirementRecipe::Structural {
                ty,
                bound: DynStructuralBound::SyncDrop,
            } => Some((ty, SemanticBuiltinTrait::SyncDrop)),
            _ => None,
        };
        if let Some((ty, trait_kind)) = sealed {
            if let Some(satisfied) = self.sealed_capability(owner, ty, trait_kind, depth + 1)? {
                return if satisfied {
                    Ok(())
                } else {
                    Err(bad("original caller lacks the complete sealed capability"))
                };
            }
        }
        match requirement {
            SemanticRequirementRecipe::Structural {
                ty,
                bound: DynStructuralBound::Copy,
            }
            | SemanticRequirementRecipe::Builtin {
                ty,
                trait_kind: SemanticBuiltinTrait::Copy,
            } => {
                if !self.copy_type(owner, ty, depth + 1)? {
                    return Err(bad(
                        "original caller lacks required whole-Type Copy premise",
                    ));
                }
            }
            SemanticRequirementRecipe::Structural {
                ty,
                bound: DynStructuralBound::StoreElement,
            }
            | SemanticRequirementRecipe::Builtin {
                ty,
                trait_kind: SemanticBuiltinTrait::StoreElement,
            } => {
                if !self.store_element(owner, ty, depth + 1)? {
                    return Err(bad(
                        "original caller lacks its complete StoreElement premise",
                    ));
                }
            }
            SemanticRequirementRecipe::Structural {
                ty,
                bound: DynStructuralBound::Value,
            }
            | SemanticRequirementRecipe::Builtin {
                ty,
                trait_kind: SemanticBuiltinTrait::Value,
            } => {
                if !self.ordinary_value(owner, ty, depth + 1)? {
                    return Err(bad(
                        "original caller lacks required whole-Type Value premise",
                    ));
                }
            }
            SemanticRequirementRecipe::Trait { ty, application } => {
                self.prove_trait(owner, ty, application, depth + 1)?
            }
            SemanticRequirementRecipe::TypeEqual { left, right } => self.same(left, right)?,
            SemanticRequirementRecipe::Outlives { longer, shorter } => {
                if !self.life_outlives(owner, longer, shorter, depth + 1)? {
                    return Err(bad(
                        "original caller lacks the complete directed lifetime bound",
                    ));
                }
            }
            SemanticRequirementRecipe::Callable {
                ty,
                kind,
                signature,
            } => {
                self.callable_requirement_under(owner, ty, *kind, signature, universal, depth + 1)?
            }
            SemanticRequirementRecipe::ForAll {
                lifetimes,
                requirements,
            } => {
                // Introduce this frame rigidly. In particular, do not use the
                // invocation-instantiation operation here: each Bound role is
                // arbitrary, and only the candidate's fresh invocation roles
                // may be solved against it. No goal is installed as a premise.
                self.pay(universal.len().saturating_add(1), depth)?;
                let mut frames = universal.to_vec();
                frames.push(*lifetimes);
                for requirement in requirements {
                    self.satisfy_requirement_under(owner, requirement, &frames, depth + 1)?;
                }
            }
            _ => {
                // Exact caller premises are assumptions of this original Gamma,
                // never requirements imported from the callee being checked.
                if !self
                    .original_requirements(owner, depth + 1)?
                    .iter()
                    .any(|premise| premise == requirement)
                {
                    return Err(pending(
                        "original call requires complete symbolic where satisfaction",
                    ));
                }
            }
        }
        Ok(())
    }
    fn call_requirements(
        &mut self,
        owner: SemanticDefinitionId,
        target: &SemanticDefinition,
        actuals: &[SemanticActualRecipe],
        depth: usize,
    ) -> R<()> {
        if target.parameters.len() != actuals.len() {
            return Err(bad("original where proof omits complete mixed actuals"));
        }
        let frames = [(0, actuals)];
        let empty = BTreeMap::new();
        let substitution = substitute::Substitution {
            frames: &frames,
            selves: &[],
            environments: &[],
            implicit: &empty,
            invocation: &empty,
            shift: 0,
        };
        for requirement in target
            .requirements
            .iter()
            .chain(target.parameters.iter().flat_map(|formal| match formal {
                SemanticFormal::Type { requirements } => requirements.as_slice(),
                _ => &[],
            }))
        {
            // Each inferred result edge was instantiated against its actual call
            // operand in settle_call_inputs. Its omitted classes belong to the
            // callee, outside the caller's named generic substitution domain.
            if target.elided_result_bound(requirement).is_some() { continue; }
            let input;
            let requirement = if self.proof.complete.contains(&target.id)
                && matches!(&target.body, SemanticDefinitionBody::FunctionWithBody {
                    body: SemanticOriginalFunctionBody::Defined(_), ..
                }) {
                input = self.input_requirement(requirement, depth + 1)?;
                let Some(input) = input.as_ref() else { continue; };
                input
            } else { requirement };
            let selected =
                substitute::requirement(requirement, &substitution, self.limits, self.left)?;
            // An inferred target variable may itself have been solved from the
            // closure. Only a width written in the callee's original contract
            // independently constrains a literal returned by the caller.
            if !self.proof.literal_return_closures.is_empty()
                && let SemanticRequirementRecipe::Callable { signature, .. } = requirement
                && matches!(&signature.result, SemanticTypeRecipe::Atom(ty) if ty.is_integer())
                && let SemanticRequirementRecipe::Callable { ty, .. } = &selected
            {
                let value = self.observation_type(owner, ty, depth + 1)?;
                if let SemanticTypeRecipe::Closure { definition, .. } = &value
                    && self.proof.literal_return_closures.contains(definition)
                    && let Some((_, actual)) = self.concrete_callable(&value, depth + 1)?
                {
                    self.contextual_integer_result(&value, &actual.result, &signature.result, depth)?;
                }
            }
            self.satisfy_requirement(owner, &selected, depth + 1)?;
        }
        Ok(())
    }
    fn written(
        &mut self,
        owner: SemanticDefinitionId,
        callee: &SemanticOriginalExpr,
        formals: &[SemanticFormal],
        variables: &mut BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<Vec<Option<SemanticActualRecipe>>> {
        self.pay(formals.len(), depth)?;
        let mut result: Vec<Option<SemanticActualRecipe>> = vec![None; formals.len()];
        if let CoreExprKind::GenericApply { arguments, .. } = &callee.kind {
            let explicit_life = arguments
                .iter()
                .any(|argument| matches!(argument, CoreGenericArgument::Lifetime(_)));
            self.pay(formals.len().saturating_add(arguments.len()), depth)?;
            let positions = formals
                .iter()
                .enumerate()
                .filter_map(|(index, formal)| {
                    (explicit_life || !matches!(formal, SemanticFormal::Lifetime)).then_some(index)
                })
                .collect::<Vec<_>>();
            if arguments.len() != positions.len() {
                return Err(bad(
                    "written mixed arguments omit a Type/Const position or an explicit lifetime position",
                ));
            }
            for (argument, index) in arguments.iter().zip(positions) {
                let formal = &formals[index];
                result[index] = Some(match (argument, formal) {
                    (CoreGenericArgument::Type(ty), SemanticFormal::Type { .. }) => {
                        SemanticActualRecipe::Type(self.resolve(owner, ty, depth + 1)?)
                    }
                    (CoreGenericArgument::Lifetime(name), SemanticFormal::Lifetime) => {
                        let definition = self.definition(owner)?;
                        let SemanticDefinitionBody::FunctionWithBody { names, .. } =
                            &definition.body
                        else {
                            return Err(bad("written life has no original name table"));
                        };
                        self.pay(names.len().saturating_add(name.len()), depth)?;
                        let binding = names
                            .iter()
                            .find(|binding| {
                                binding.namespace == SemanticOriginalNamespace::Lifetime
                                    && binding.path.len() == 1
                                    && binding.path[0] == *name
                            })
                            .ok_or_else(|| {
                                bad("written lifetime does not name an original lexical formal")
                            })?;
                        let SemanticOriginalNameValue::Recipe(SemanticActualRecipe::Life(life)) =
                            &binding.value
                        else {
                            return Err(bad("written lifetime names another original kind"));
                        };
                        crate::dyn_metadata::semantic_operand_snapshot(
                            life,
                            self.limits,
                            self.left,
                            1,
                        )?;
                        SemanticActualRecipe::Life(life.clone())
                    }
                    (CoreGenericArgument::Const(value), SemanticFormal::Const { carrier }) => {
                        let mut arguments = Vec::new();
                        self.pay(formals.len(), depth)?;
                        for (ordinal, formal) in formals.iter().enumerate() {
                            if let Some(actual) = result.get(ordinal).and_then(|r| r.as_ref()) {
                                crate::dyn_metadata::semantic_operand_snapshot(
                                    actual,
                                    self.limits,
                                    self.left,
                                    1,
                                )?;
                                arguments.push(actual.clone());
                            } else if matches!(formal, SemanticFormal::Type { .. }) {
                                arguments.push(SemanticActualRecipe::Type(
                                    SemanticTypeRecipe::Formal(SemanticFormalRef {
                                        scope: SemanticScopeRef::Ancestor(u32::MAX),
                                        ordinal: ordinal as u32,
                                    }),
                                ));
                            } else {
                                arguments.push(SemanticActualRecipe::Infer);
                            }
                        }
                        let carrier = self.instantiate_template(carrier, &arguments, depth + 1)?;
                        let actual_ty =
                            self.expr(owner, value, variables, Some(&carrier), depth + 1)?;
                        self.same(&carrier, &actual_ty)?;
                        let SemanticTypeRecipe::Atom(ty) = actual_ty else {
                            return Err(pending(
                                "written const needs its original symbolic value/carrier equation",
                            ));
                        };
                        let value = match &value.kind {
                            CoreExprKind::Literal(value) => value.clone(),
                            CoreExprKind::Integer(text) => {
                                integer(text, Some(&ty), false, value.span)?
                            }
                            _ => {
                                return Err(pending(
                                    "written const needs its complete original const evaluator",
                                ));
                            }
                        };
                        SemanticActualRecipe::Const {
                            carrier: SemanticTypeRecipe::Atom(ty),
                            value: SemanticConstRecipe::Value(value),
                        }
                    }
                    _ => {
                        return Err(bad(
                            "written argument changes its original mixed formal kind",
                        ));
                    }
                });
            }
        } else if let CoreExprKind::TypeApply { type_arguments, .. } = &callee.kind {
            let count = formals
                .iter()
                .filter(|formal| matches!(formal, SemanticFormal::Type { .. }))
                .count();
            if type_arguments.len() != count
                || formals
                    .iter()
                    .any(|f| matches!(f, SemanticFormal::Const { .. }))
            {
                return Err(bad(
                    "type-only application omits original mixed argument positions",
                ));
            }
            let mut arguments = type_arguments.iter();
            for (formal, slot) in formals.iter().zip(&mut result) {
                if matches!(formal, SemanticFormal::Lifetime) {
                    continue;
                }
                let ty = arguments.next().ok_or_else(|| {
                    bad("written Type application loses an original Type position")
                })?;
                *slot = Some(SemanticActualRecipe::Type(self.resolve(
                    owner,
                    ty,
                    depth + 1,
                )?));
            }
        }
        Ok(result)
    }
    fn contextual_integer(
        &mut self,
        expression: &SemanticOriginalExpr,
        variables: &BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<bool> {
        self.pay(1, depth)?;
        match &expression.kind {
            CoreExprKind::Integer(text) => {
                self.pay(text.len(), depth)?;
                Ok(![
                    "i8", "i16", "i32", "i64", "isize", "u8", "u16", "u32", "u64", "usize",
                ]
                .iter()
                .any(|suffix| text.ends_with(suffix)))
            }
            CoreExprKind::Group(inner)
            | CoreExprKind::Unary {
                op: UnaryOp::Negate,
                operand: inner,
            } => self.contextual_integer(inner, variables, depth + 1),
            CoreExprKind::Name(name) if !self.integer_initializers.is_empty() => {
                self.pay(name.len().saturating_add(variables.len()), depth)?;
                let Some(value) = variables.get(name) else { return Ok(false); };
                let Query::Binding(binding) = value.origin else { return Ok(false); };
                self.pay(self.integer_initializers.len().saturating_add(1).ilog2() as usize + 1, depth)?;
                if let Some(count) = self.integer_initializers.get(&value.origin_owner).map(BTreeMap::len) {
                    self.pay(count.saturating_add(1).ilog2() as usize
                        + self.proof.types.len().saturating_add(1).ilog2() as usize + 2, depth)?;
                    return Ok(self.integer_initializers[&value.origin_owner].contains_key(&binding)
                        && !self.proof.types.contains_key(&(value.origin_owner, Query::Binding(binding))));
                }
                Ok(false)
            }
            CoreExprKind::Binary { op, left, right }
                if !matches!(
                    op,
                    BinaryOp::Equal
                        | BinaryOp::NotEqual
                        | BinaryOp::Less
                        | BinaryOp::LessEqual
                        | BinaryOp::Greater
                        | BinaryOp::GreaterEqual
                        | BinaryOp::And
                        | BinaryOp::Or
                ) =>
            {
                Ok(self.contextual_integer(left, variables, depth + 1)?
                    && self.contextual_integer(right, variables, depth + 1)?)
            }
            _ => Ok(false),
        }
    }

    /// An unannotated immutable literal has one integer type shared by all
    /// its occurrences, including observations from lexical children.
    fn infer_integer_binding(
        &mut self,
        owner: SemanticDefinitionId,
        binder: NodeId,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<Option<SemanticTypeRecipe>> {
        if self.integer_initializers.is_empty() {
            return Ok(None);
        }
        self.pay(self.integer_initializers.len().saturating_add(1).ilog2() as usize + 1, depth)?;
        let Some(initializers) = self.integer_initializers.get(&owner) else { return Ok(None); };
        let work = initializers.len().saturating_add(1).ilog2() as usize + 1;
        self.pay(work, depth)?;
        let Some(initializer) = self.integer_initializers[&owner].get(&binder) else {
            return Ok(None);
        };
        let work = initializer.spelling.len().saturating_add(
            self.proof.types.len().saturating_add(1).ilog2() as usize + 1,
        );
        self.pay(work, depth)?;
        let key = (owner, Query::Binding(binder));
        if let Some(ty) = self.proof.types.get(&key) {
            return copy_recipe(ty, self.limits, self.left).map(Some);
        }
        let initializer = &self.integer_initializers[&owner][&binder];
        let wanted = match wanted {
            Some(SemanticTypeRecipe::Atom(ty)) if ty.is_integer() => Some(ty),
            _ => None,
        };
        let ty = SemanticTypeRecipe::Atom(integer(
            &initializer.spelling, wanted, false, initializer.span,
        )?.ty());
        let expression = initializer.expression;
        let copy = copy_recipe(&ty, self.limits, self.left)?;
        let proof = self.proof.proof_mut()?;
        proof.types.insert(key, copy);
        proof.types.insert((owner, Query::Expression(expression)), ty.clone());
        Ok(Some(ty))
    }

    fn binding_integer(
        &mut self,
        binding: &Binding,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<Option<SemanticTypeRecipe>> {
        let Query::Binding(binder) = binding.origin else { return Ok(None); };
        self.infer_integer_binding(binding.origin_owner, binder, wanted, depth + 1)
    }
    fn expr(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        let result = self
            .expr_inner(owner, expression, variables, wanted, depth)
            .and_then(|result| {
                self.expression_result_origin(owner, expression, variables, &result, depth + 1)?;
                Ok(result)
            });
        result.map_err(|mut diagnostic| {
            // Error context is also bounded; budget exhaustion never triggers
            // an uncharged formatting walk over attacker-supplied source text.
            if let Some(left) = self.left.checked_sub(192) {
                *self.left = left;
                diagnostic.notes.push(format!(
                    "original Gamma: definition {:?}, expression {:?}",
                    owner, expression.id
                ));
            }
            crate::semantics::locate_original(diagnostic, expression.span)
        })
    }
    fn expr_inner(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(1, depth)?;
        self.snapshot(
            owner,
            SemanticOriginalNode::Expression(expression.id),
            variables,
        )?;
        if let Some(result) = self.contextual_expr_result(owner, expression, variables, wanted, depth)? {
            return Ok(result);
        }
        if let CoreExprKind::Call { arguments, .. } = &expression.kind {
            return self.call_expression(owner, expression, arguments, variables, wanted, depth);
        }
        self.operand_expr_result(owner, expression, variables, wanted, depth)
    }

    fn contextual_expr_result(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<Option<SemanticTypeRecipe>> {
        if !matches!(&expression.kind, CoreExprKind::If { .. } | CoreExprKind::Sequence { .. })
            && let Some(union @ SemanticTypeRecipe::ErrorUnion { error, result }) = wanted
        {
            return self.expected_error_expression(owner, expression, variables, union, error, result, depth).map(Some);
        }
        if matches!(
            &expression.kind,
            CoreExprKind::Member { .. } | CoreExprKind::Call { .. }
        ) {
            if let Some(result) =
                self.enum_constructor(owner, expression, variables, wanted, depth + 1)?
            {
                let copy = copy_recipe(&result, self.limits, self.left)?;
                self.proof
                    .proof_mut()?
                    .types
                    .insert((owner, Query::Expression(expression.id)), copy);
                return Ok(Some(result));
            }
        }
        if matches!(
            &expression.kind,
            CoreExprKind::Member { .. }
                | CoreExprKind::TypeApply { .. }
                | CoreExprKind::GenericApply { .. }
        ) {
            if let Some(site) = self.site(owner, SemanticOriginalNode::Expression(expression.id))? {
                if matches!(site.kind, SemanticSiteRecipeKind::Construct { .. }) {
                    return Err(bad(
                        "original bare Construct lacks its exact enum declaration and zero-field variant theorem",
                    ));
                }
            }
        }
        if let Some(result) = self.fn_value(owner, expression, variables, wanted, depth + 1)? {
            let copy = copy_recipe(&result, self.limits, self.left)?;
            self.proof
                .proof_mut()?
                .types
                .insert((owner, Query::Expression(expression.id)), copy);
            return Ok(Some(result));
        }
        Ok(None)
    }

    fn operand_expr_result(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        // The operation's other alternatives can contain large checked values.
        // Keep each operation's storage in its own active frame.
        match &expression.kind {
            CoreExprKind::Closure { .. } => {
                self.closure_expression(owner, expression, variables, wanted, depth)
            }
            CoreExprKind::Member { .. } => {
                let result = self.observe_member(owner, expression, variables, depth + 1)?;
                self.record_expression_type(owner, expression.id, result)
            }
            CoreExprKind::Index { object, index } => {
                let object_ty = self.place(owner, object, variables, depth + 1)?;
                let element = self.index_type(owner, &object_ty, index, variables, depth + 1)?;
                if !self.copy_type(owner, &element, depth + 1)? {
                    return Err(bad(
                        "original indexed observation requires a whole-element Copy premise",
                    ));
                }
                self.record_expression_type(owner, expression.id, element)
            }
            CoreExprKind::Group(value) => {
                let result = self.expr(owner, value, variables, wanted, depth + 1)?;
                self.record_expression_type(owner, expression.id, result)
            }
            CoreExprKind::Try(_) if self.cleanup_depth != 0 => {
                Err(error("E-DEFER-CONTROL", expression.span, "a deferred action cannot propagate an error from its activation"))
            }
            CoreExprKind::Try(value) => {
                let result = self.propagate_expression(owner, value, variables, depth)?;
                self.record_expression_type(owner, expression.id, result)
            }
            CoreExprKind::Binary { .. } => {
                self.binary_expression(owner, expression, variables, wanted, depth)
            }
            _ => self.nonbinary_expression(owner, expression, variables, wanted, depth),
        }
    }

    #[inline(never)]
    fn expected_error_expression(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        union: &SemanticTypeRecipe,
        error: &SemanticTypeRecipe,
        result: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        let raw_wanted = match &expression.kind {
            CoreExprKind::Call { callee, .. } => {
                let mut base = callee.as_ref();
                while let CoreExprKind::TypeApply { callee, .. }
                | CoreExprKind::GenericApply { callee, .. } = &base.kind
                {
                    self.pay(1, depth)?;
                    base = callee;
                }
                if matches!(&base.kind, CoreExprKind::Name(name) if name == "store_new") {
                    Some(result)
                } else {
                    None
                }
            }
            _ => None,
        };
        let raw_wanted = if raw_wanted.is_some() {
            raw_wanted
        } else {
            self.enum_injection_context(owner, expression, variables, error, result, depth + 1)?
        };
        let actual = self.expr(owner, expression, variables, raw_wanted, depth + 1)?;
        let equal_union = match self.same(&actual, union) {
            Ok(()) => true,
            Err(error) if error.category == zil_base::ResultCategory::ResourceLimit => {
                return Err(error);
            }
            Err(_) => false,
        };
        let value = if equal_union {
            actual
        } else {
            let is_error = match self.same(&actual, error) {
                Ok(()) => true,
                Err(error) if error.category == zil_base::ResultCategory::ResourceLimit => {
                    return Err(error);
                }
                Err(_) => false,
            };
            let is_result = match self.same(&actual, result) {
                Ok(()) => true,
                Err(error) if error.category == zil_base::ResultCategory::ResourceLimit => {
                    return Err(error);
                }
                Err(_) => false,
            };
            if is_error == is_result {
                return Err(bad(
                    "original implicit error injection is ambiguous or changes its payload type",
                ));
            }
            let payload = copy_recipe(&actual, self.limits, self.left)?;
            self.pay(1, depth)?;
            self.proof
                .proof_mut()?
                .injections
                .insert((owner, expression.id), Injection { is_error, payload });
            copy_recipe(union, self.limits, self.left)?
        };
        let copy = copy_recipe(&value, self.limits, self.left)?;
        self.proof
            .proof_mut()?
            .types
            .insert((owner, Query::Expression(expression.id)), copy);
        Ok(value)
    }

    #[inline(never)]
    fn binary_expression(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        let CoreExprKind::Binary { op, left, right } = &expression.kind else {
            unreachable!("original binary dispatch");
        };
        let result = {
            let boolean = matches!(op, BinaryOp::And | BinaryOp::Or);
            let equality = matches!(op, BinaryOp::Equal | BinaryOp::NotEqual);
            let comparison = equality
                || matches!(
                    op,
                    BinaryOp::Less
                        | BinaryOp::LessEqual
                        | BinaryOp::Greater
                        | BinaryOp::GreaterEqual
                );
            // A comparison's Bool result is not its operand context. Only
            // arithmetic forwards its numeric result context; logical
            // operators impose their own Bool operand scheme.
            let bool_type = SemanticTypeRecipe::Atom(Type::Bool);
            let operand_wanted = if boolean {
                Some(&bool_type)
            } else if comparison {
                None
            } else {
                wanted.filter(|ty| matches!(ty, SemanticTypeRecipe::Atom(ty) if ty.is_integer() || ty.is_float()))
            };
            // An untyped literal has no effects. Delaying it permits the
            // other operand's independent type to supply its numeric
            // context without evaluating or consuming that operand twice.
            let (left, right) = if operand_wanted.is_none()
                && self.contextual_integer(left, variables, depth + 1)?
                && !self.contextual_integer(right, variables, depth + 1)?
            {
                let right_ty = self.expr(owner, right, variables, None, depth + 1)?;
                let right_type = self.logical(owner, &right_ty, depth + 1)?;
                let left_ty =
                    self.expr(owner, left, variables, Some(&right_type), depth + 1)?;
                (left_ty, right_ty)
            } else {
                let left_ty = self.expr(owner, left, variables, operand_wanted, depth + 1)?;
                let left_type = self.logical(owner, &left_ty, depth + 1)?;
                let right_ty =
                    self.expr(owner, right, variables, Some(&left_type), depth + 1)?;
                (left_ty, right_ty)
            };
            let left = self.logical(owner, &left, depth + 1)?;
            let right = self.logical(owner, &right, depth + 1)?;
            self.same(&left, &right)?;
            let SemanticTypeRecipe::Atom(ty) = left else {
                return Err(bad(
                    "original generic operation lacks a primitive type equality premise",
                ));
            };
            let integer_only = matches!(
                op,
                BinaryOp::BitAnd
                    | BinaryOp::BitOr
                    | BinaryOp::BitXor
                    | BinaryOp::ShiftLeft
                    | BinaryOp::ShiftRight
                    | BinaryOp::AddWrapping
                    | BinaryOp::SubtractWrapping
                    | BinaryOp::MultiplyWrapping
            );
            let valid = if boolean {
                ty == Type::Bool
            } else if equality {
                matches!(ty, Type::Bool | Type::ForeignHandle) || ty.is_integer() || ty.is_float()
            } else if integer_only {
                ty.is_integer()
            } else {
                ty.is_integer() || ty.is_float()
            };
            if !valid {
                return Err(bad(
                    "original binary operator has incompatible primitive endpoint",
                ));
            }
            SemanticTypeRecipe::Atom(if boolean || comparison {
                Type::Bool
            } else {
                ty
            })
        };
        self.record_expression_type(owner, expression.id, result)
    }

    #[inline(never)]
    fn propagate_expression(
        &mut self,
        owner: SemanticDefinitionId,
        value: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        let value = self.expr(owner, value, variables, None, depth + 1)?;
        let value = self.logical(owner, &value, depth + 1)?;
        let SemanticTypeRecipe::ErrorUnion { error, result } = value else {
            return Err(bad("original propagation operand is not an error union"));
        };
        let definition = self.definition(owner)?;
        let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &definition.body
        else {
            unreachable!()
        };
        let declared = self.body_result_recipe(owner, &signature.result)?;
        let function_result = self.resolve(owner, &declared, depth + 1)?;
        let SemanticTypeRecipe::ErrorUnion {
            error: expected, ..
        } = function_result
        else {
            return Err(bad(
                "original propagation escapes a function without an error result",
            ));
        };
        self.same(&error, &expected)?;
        self.capture_exit(owner, variables, depth + 1)?;
        Ok(*result)
    }

    fn nonbinary_expression(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        let result = match &expression.kind {
            CoreExprKind::Literal(value) => {
                if let Constant::WideString(units) = value {
                    self.pay(units.len(), depth)?;
                    foreign::check_literal(value, expression.span)?;
                }
                SemanticTypeRecipe::Atom(value.ty())
            }
            CoreExprKind::Integer(text) => {
                let expected = wanted.and_then(|ty| {
                    if let SemanticTypeRecipe::Atom(ty) = ty {
                        Some(ty)
                    } else {
                        None
                    }
                });
                SemanticTypeRecipe::Atom(integer(text, expected, false, expression.span)?.ty())
            }
            CoreExprKind::Float(text) => {
                let expected = wanted.and_then(|ty| {
                    if let SemanticTypeRecipe::Atom(ty) = ty {
                        Some(ty)
                    } else {
                        None
                    }
                });
                SemanticTypeRecipe::Atom(floats::literal(text, expected, expression.span)?.ty())
            }
            CoreExprKind::Bool(_) => SemanticTypeRecipe::Atom(Type::Bool),
            CoreExprKind::Unit => SemanticTypeRecipe::Atom(Type::Unit),
            CoreExprKind::Name(name) => {
                self.pay(name.len().saturating_add(variables.len()), depth)?;
                let value = variables
                    .get_mut(name)
                    .ok_or_else(|| bad("original name has no lexical value binder"))?;
                if value.used || !value.initialized || !value.taken_fields.is_empty() {
                    return Err(bad(
                        "original body reads a value after its affine whole-type transfer",
                    ));
                }
                if let SemanticTypeRecipe::OriginalType {
                    scope: SemanticScopeRef::Ancestor(0),
                    source: SemanticOriginalTypeSource::Parameter(ordinal),
                } = value.ty.as_ref()
                    && value.origin_owner == owner
                    && value.origin == Query::Parameter(*ordinal)
                {
                    value.ty = std::rc::Rc::new(
                        self.closure_scalar_input(owner, *ordinal, wanted, depth + 1)?,
                    );
                }
                let integer = self.binding_integer(value, wanted, depth + 1)?;
                value.observed = true;
                let observed = self.observation_type(owner, &value.ty, depth + 1)?;
                let result = if let Some(integer) = integer {
                    integer
                } else if let SemanticTypeRecipe::Managed(inner) = &observed {
                    let inner = copy_recipe(inner, self.limits, self.left)?;
                    SemanticTypeRecipe::Desc {
                        lifetime: self.borrow_life(owner, expression.id)?,
                        inner: Box::new(inner),
                    }
                } else {
                    let reusable_descriptor = matches!(&observed, SemanticTypeRecipe::Desc { .. });
                    if !reusable_descriptor && !self.copy_type(owner, &value.ty, depth + 1)? {
                        if value.borrowed {
                            return Err(bad(
                                "original body moves a non-Copy value from a shared capture",
                            ));
                        }
                        value.used = true;
                    }
                    if reusable_descriptor {
                        observed
                    } else {
                        copy_recipe(&value.ty, self.limits, self.left)?
                    }
                };
                self.observe_capture(owner, name, value, depth + 1)?;
                result
            }
            CoreExprKind::CompileTime(value) => {
                let outer_stage = self.stage_depth;
                self.stage_depth = outer_stage
                    .checked_add(1)
                    .ok_or_else(|| bad("original stage nesting overflows"))?;
                let result = self.expr(owner, value, variables, wanted, depth + 1);
                self.stage_depth = outer_stage;
                let result = result?;
                self.pay(self.proof.compile_time_results.len().saturating_add(1).ilog2() as usize + 1, depth)?;
                self.proof.proof_mut()?.compile_time_results.insert((owner, expression.id));
                result
            }
            CoreExprKind::Sequence { bindings, value } => {
                let mut shared = self.prepared_shared_address(owner, bindings, value, variables, depth + 1)?;
                for (name, initializer) in bindings {
                    if !name.starts_with('$') || variables.contains_key(name) {
                        return Err(bad("original sequence repeats or exposes a generated temporary"));
                    }
                    let ty = match shared.take() {
                        Some(ty) => ty,
                        None => self.expr(owner, initializer, variables, None, depth + 1)?,
                    };
                    let mut source = initializer;
                    if matches!(self.observation_type(owner, &ty, depth + 1)?, SemanticTypeRecipe::Managed(_)) {
                        while let CoreExprKind::Group(inner)
                            | CoreExprKind::Ascribed { value: inner, .. }
                            | CoreExprKind::Sequence { value: inner, .. } = &source.kind
                        {
                            self.pay(1, depth)?;
                            source = inner;
                        }
                    }
                    self.pay(name.len().saturating_add(1), depth)?;
                    self.insert_binding(variables, name.clone(), Binding {
                        ty: std::rc::Rc::new(ty), mutable: false, used: false, observed: false, capture: false,
                        reaching_write: OriginalSlotWrite::Unknown, borrowed: false, origin: Query::Expression(source.id), origin_owner: owner,
                        initialized: true, written: false, resource_root: None,
                        taken_fields: BTreeSet::new(), shadowed: None,
                    }, depth)?;
                }
                let result = self.expr(owner, value, variables, wanted, depth + 1)?;
                for (name, _) in bindings.iter().rev() {
                    self.pay(name.len().saturating_add(1), depth)?;
                    variables.remove(name);
                }
                result
            }
            CoreExprKind::String(spelling) => {
                // Decode validation and byte emission are independently performed
                // by the real UTF8 instruction producer. The original type is
                // always the exact language static byte-slice address scheme.
                self.pay(spelling.len(), depth)?;
                SemanticTypeRecipe::Address {
                    mutable: false,
                    lifetime: SemanticOuterLifeRecipe::Static,
                    inner: Box::new(SemanticTypeRecipe::Slice(Box::new(
                        SemanticTypeRecipe::Atom(Type::U8),
                    ))),
                }
            }
            CoreExprKind::Array(fields) => {
                self.pay(fields.len().saturating_add(3), depth)?;
                let expected = match wanted {
                    Some(SemanticTypeRecipe::Array { element, .. }) => Some(element.as_ref()),
                    _ => None,
                };
                let mut element = expected
                    .map(|ty| copy_recipe(ty, self.limits, self.left))
                    .transpose()?;
                for field in fields {
                    let actual = self.expr(owner, field, variables, element.as_ref(), depth + 1)?;
                    if let Some(element) = &element {
                        self.same(element, &actual)?;
                    } else {
                        element = Some(actual);
                    }
                }
                let element = element.ok_or_else(|| {
                    bad("original empty array requires a complete element type context")
                })?;
                let length = u32::try_from(fields.len())
                    .map_err(|_| bad("original array exceeds its length domain"))?;
                SemanticTypeRecipe::Array {
                    element: Box::new(element),
                    length: Box::new(SemanticConstRecipe::Value(Constant::Usize(u64::from(
                        length,
                    )))),
                }
            }
            CoreExprKind::Slice { object, start, end } => {
                let object_ty = self.place(owner, object, variables, depth + 1)?;
                let object_ty = self.logical(owner, &object_ty, depth + 1)?;
                let element = match object_ty {
                    SemanticTypeRecipe::Array { element, .. }
                    | SemanticTypeRecipe::Slice(element) => element,
                    SemanticTypeRecipe::Address { inner, .. } => match *inner {
                        SemanticTypeRecipe::Array { element, .. }
                        | SemanticTypeRecipe::Slice(element) => element,
                        _ => return Err(bad("original slice does not select an array or slice")),
                    },
                    _ => return Err(bad("original slice does not select an array or slice")),
                };
                for index in start.iter().chain(end.iter()) {
                    let ty = self.expr(owner, index, variables, None, depth + 1)?;
                    let ty = self.logical(owner, &ty, depth + 1)?;
                    if !matches!(ty, SemanticTypeRecipe::Atom(ty) if ty.is_integer()) {
                        return Err(bad("original slice bound is not an integer"));
                    }
                }
                SemanticTypeRecipe::Slice(element)
            }
            CoreExprKind::NumericConvert { value, target }
            | CoreExprKind::IntegerConvert { value, target } => {
                let target = self.resolve(owner, target, depth + 1)?;
                let value = self.expr(owner, value, variables, None, depth + 1)?;
                let value = self.logical(owner, &value, depth + 1)?;
                if !matches!((&value, &target), (SemanticTypeRecipe::Atom(a), SemanticTypeRecipe::Atom(b)) if (a.is_integer() || a.is_float()) && (b.is_integer() || b.is_float()))
                {
                    return Err(bad(
                        "original numeric conversion requires primitive numeric endpoints",
                    ));
                }
                target
            }
            CoreExprKind::Unary {
                op: UnaryOp::Give,
                operand,
            } => self.give_place(owner, operand, variables, depth + 1)?,
            CoreExprKind::Ascribed { ty, value } => {
                let expected = self.resolve(owner, ty, depth + 1)?;
                let actual = if let Some(view) = self.shared_input_view(
                    owner,
                    Query::Expression(expression.id),
                    value,
                    &expected,
                    variables,
                    depth + 1,
                )? {
                    view
                } else {
                    self.expr(owner, value, variables, Some(&expected), depth + 1)?
                };
                self.same(&actual, &expected)?;
                expected
            }
            CoreExprKind::Unary {
                op: UnaryOp::Borrow | UnaryOp::BorrowMut,
                operand,
            } => {
                return self.borrow_expression(owner, expression, operand, variables, wanted, depth);
            }
            CoreExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } => {
                let value = self.observe_address_operand(owner, operand, variables, depth + 1)?;
                let value = self.observation_type(owner, &value, depth + 1)?;
                let SemanticTypeRecipe::Address { inner, .. } = value else {
                    return Err(bad("original dereference has no address type"));
                };
                if !self.copy_type(owner, &inner, depth + 1)? {
                    return Err(bad(
                        "original dereference cannot transfer a non-Copy pointee",
                    ));
                }
                *inner
            }
            CoreExprKind::Unary {
                op: UnaryOp::Negate,
                operand,
            } if matches!(&operand.kind, CoreExprKind::Integer(_)) => {
                let CoreExprKind::Integer(text) = &operand.kind else {
                    unreachable!()
                };
                let expected = wanted.and_then(|ty| match ty {
                    SemanticTypeRecipe::Atom(ty) => Some(ty),
                    _ => None,
                });
                let ty =
                    SemanticTypeRecipe::Atom(integer(text, expected, true, expression.span)?.ty());
                // The magnitude is checked as part of this signed literal,
                // never admitted as a standalone positive narrow integer.
                let copy = copy_recipe(&ty, self.limits, self.left)?;
                self.proof
                    .proof_mut()?
                    .types
                    .insert((owner, Query::Expression(operand.id)), copy);
                ty
            }
            CoreExprKind::Unary { op, operand } => {
                let value = self.expr(owner, operand, variables, wanted, depth + 1)?;
                let value = self.logical(owner, &value, depth + 1)?;
                let SemanticTypeRecipe::Atom(ty) = &value else {
                    return Err(pending(
                        "original unary operation lacks its declared symbolic primitive premise",
                    ));
                };
                let valid = match op {
                    UnaryOp::Not => *ty == Type::Bool,
                    UnaryOp::BitNot => ty.is_integer(),
                    UnaryOp::Negate => ty.is_integer() || ty.is_float(),
                    _ => false,
                };
                if !valid {
                    return Err(bad(
                        "original unary operator has incompatible primitive endpoint",
                    ));
                }
                value
            }
            CoreExprKind::ReceiverMember(member) => {
                let definition = self.definition(owner)?;
                let SemanticDefinitionBody::FunctionWithBody {
                    receiver_parameter: Some(ordinal),
                    parameters,
                    ..
                } = &definition.body
                else {
                    return Err(bad(
                        "original receiver member has no real declared receiver",
                    ));
                };
                let parameter = parameters
                    .get(*ordinal as usize)
                    .ok_or_else(|| bad("original receiver selects an absent parameter"))?;
                let receiver = variables.get_mut(&parameter.name).ok_or_else(|| {
                    bad("original receiver has no current lexical parameter binder")
                })?;
                if receiver.used || !receiver.initialized {
                    return Err(bad("original receiver was already transferred"));
                }
                receiver.observed = true;
                self.observe_capture(owner, &parameter.name, receiver, depth + 1)?;
                let receiver_ty = copy_recipe(&receiver.ty, self.limits, self.left)?;
                let field = self.field(owner, &receiver_ty, member, depth + 1)?;
                self.observe_field(owner, expression, field, depth + 1)?
            }
            CoreExprKind::Record { ty, fields } => {
                self.record_constructor(owner, ty, fields, variables, wanted, depth + 1)?
            }
            CoreExprKind::RecordUpdate { ty, base, fields } => self.record_update(
                owner,
                ty.as_ref(),
                base,
                fields,
                variables,
                wanted,
                depth + 1,
            )?,
            CoreExprKind::RecordConstructor { definition, fields } => self
                .inferred_record_constructor(
                    owner,
                    expression,
                    *definition,
                    fields,
                    variables,
                    wanted,
                    depth + 1,
                )?,
            CoreExprKind::Tuple(fields) if fields.is_empty() => {
                SemanticTypeRecipe::Atom(Type::Unit)
            }
            CoreExprKind::Tuple(fields) => {
                self.pay(fields.len(), depth)?;
                let expected = match wanted {
                    Some(SemanticTypeRecipe::Tuple(items)) if items.len() == fields.len() => Some(items),
                    _ => None,
                };
                let mut types = Vec::with_capacity(fields.len());
                for (index, field) in fields.iter().enumerate() {
                    let wanted = expected.map(|items| &items[index]);
                    let actual = self.expr(owner, field, variables, wanted, depth + 1)?;
                    if let Some(wanted) = wanted {
                        self.same(wanted, &actual)?;
                    }
                    types.push(actual);
                }
                SemanticTypeRecipe::Tuple(types)
            }
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                let condition = self.expr(
                    owner,
                    condition,
                    variables,
                    Some(&SemanticTypeRecipe::Atom(Type::Bool)),
                    depth + 1,
                )?;
                self.same(&condition, &SemanticTypeRecipe::Atom(Type::Bool))?;
                let mut left = self.clone_variables(variables)?;
                let mut right = self.clone_variables(variables)?;
                let (a, b) = if let Some(target) = wanted
                    && self.inferred_return_head(owner, target)?
                {
                    let a = self.expr(owner, then_value, &mut left, Some(target), depth + 1)?;
                    let b = self.expr(owner, else_value, &mut right, Some(target), depth + 1)?;
                    self.return_edge(owner, then_value, &left, &a, target, depth + 1)?;
                    self.return_edge(owner, else_value, &right, &b, target, depth + 1)?;
                    (copy_recipe(target, self.limits, self.left)?, copy_recipe(target, self.limits, self.left)?)
                } else if wanted.is_none()
                    && self.contextual_integer(then_value, variables, depth + 1)?
                    && !self.contextual_integer(else_value, variables, depth + 1)?
                {
                    // Each branch has its own original state. Check the
                    // independently typed peer once before defaulting the
                    // context-dependent integer arm, just as for binary peers.
                    let b = self.expr(owner, else_value, &mut right, None, depth + 1)?;
                    let context = self.logical(owner, &b, depth + 1)?;
                    let a = self.expr(owner, then_value, &mut left, Some(&context), depth + 1)?;
                    (a, b)
                } else {
                    let a = self.expr(owner, then_value, &mut left, wanted, depth + 1)?;
                    let b = self.expr(owner, else_value, &mut right, Some(&a), depth + 1)?;
                    (a, b)
                };
                let result = if wanted.is_none()
                    && let (SemanticTypeRecipe::Address { lifetime: x, .. }, SemanticTypeRecipe::Address { lifetime: y, .. }) = (&a,&b)
                    && x != y
                {
                    let SemanticTypeRecipe::Address {mutable, inner, ..} = &a else {unreachable!()};
                    let SemanticTypeRecipe::Address {mutable: other_mutable, inner: other_inner, ..} = &b else {unreachable!()};
                    if mutable != other_mutable {return Err(bad("conditional changes its address mode"));}
                    self.same(inner,other_inner)?;
                    let lifetime = self.conditional_region(owner,expression.id,depth+1)?;
                    self.record_region_source(owner,&lifetime,then_value,x,&left,depth+1)?;
                    self.record_region_source(owner,&lifetime,else_value,y,&right,depth+1)?;
                    SemanticTypeRecipe::Address { mutable:*mutable, lifetime, inner:Box::new(copy_recipe(inner,self.limits,self.left)?) }
                } else { self.same(&a,&b)?; a };
                self.merge_variables(variables, &[left, right])?;
                result
            }
            _ => {
                return Err(pending(
                    "original operation awaits its complete symbolic typing rule",
                ));
            }
        };
        self.record_expression_type(owner, expression.id, result)
    }
    // Calls and closure literals may enter another complete original body.
    // Keep their operation-specific temporaries in the corresponding checker.
    fn special_call_result(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        arguments: &[SemanticOriginalExpr],
        variables: &mut BTreeMap<String, Binding>,
        wanted: Option<&SemanticTypeRecipe>,
        site: SemanticSiteId,
        depth: usize,
    ) -> R<Option<SemanticTypeRecipe>> {
        if let Some(result) = self.dyn_call(owner, expression, variables, wanted, depth + 1)? {
            return Ok(Some(result));
        }
        if let Some(result) = self.method_call(owner, site, expression, variables, wanted, depth + 1)? {
            return Ok(Some(result));
        }
        if let Some(result) = self.intrinsic(owner, site, expression, variables, wanted, depth + 1)? {
            self.intrinsic_resource_write(owner, site, arguments, variables, depth + 1)?;
            return Ok(Some(result));
        }
        self.callable_call(owner, site, expression, variables, wanted, depth + 1)
    }
    fn original_call_target(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        site: &SemanticSiteRecipe,
        depth: usize,
    ) -> R<(SemanticDefinitionId, Vec<SemanticActualRecipe>)> {
        Ok(match &site.kind {
            SemanticSiteRecipeKind::Call { target, actuals } => (*target, actuals.clone()),
            SemanticSiteRecipeKind::CallableCall { .. } => {
                let CoreExprKind::Call { callee, .. } = &expression.kind else {
                    unreachable!()
                };
                let value = self.place(owner, callee, variables, depth + 1)?;
                let value = if let SemanticTypeRecipe::Opaque { hidden, .. } = value {
                    *hidden
                } else {
                    value
                };
                let SemanticTypeRecipe::Closure {
                    definition,
                    arguments,
                    ..
                } = value
                else {
                    return Err(pending(
                        "abstract callable needs its complete declared original role premise",
                    ));
                };
                (definition, arguments)
            }
            SemanticSiteRecipeKind::OriginalCall => {
                let CoreExprKind::Call { callee, .. } = &expression.kind else {
                    unreachable!()
                };
                let mut callee = callee.as_ref();
                while let CoreExprKind::TypeApply { callee: inner, .. }
                | CoreExprKind::GenericApply { callee: inner, .. } = &callee.kind
                {
                    callee = inner;
                }
                if let Some(path) = self.declaration_path(callee, variables, depth + 1)? {
                    let target = self.declaration_target(owner, &path, depth + 1)?
                        .ok_or_else(|| {
                            if path.len() == 1 {
                                bad("unbound original direct call name")
                            } else {
                                pending("original call path lacks its real declaration import binding")
                            }
                        })?;
                    let target_definition = self.definition(target)?;
                    self.pay(target_definition.parameters.len(), depth)?;
                    (
                        target,
                        vec![
                            SemanticActualRecipe::Infer;
                            target_definition.parameters.len()
                        ],
                    )
                } else {
                    let value = self.place(owner, callee, variables, depth + 1)?;
                    let value = if let SemanticTypeRecipe::Opaque { hidden, .. } = value {
                        *hidden
                    } else {
                        value
                    };
                    let SemanticTypeRecipe::Closure {
                        definition,
                        arguments,
                        ..
                    } = value
                    else {
                        return Err(pending(
                            "original callable requires its declared complete role premise",
                        ));
                    };
                    (definition, arguments)
                }
            }
            _ => return Err(bad("original call site has a different operation kind")),
        })
    }
    fn call_expression(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        arguments: &[SemanticOriginalExpr],
        variables: &mut BTreeMap<String, Binding>,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        let result = {
            let site = self
                .site(owner, SemanticOriginalNode::Expression(expression.id))?
                .ok_or_else(|| {
                    pending("original call has no resolved original target recipe")
                })?;
            self.pay(
                usize::from(self.stage_depth > 0) + usize::from(self.cleanup_depth > 0),
                depth,
            )?;
            if self.stage_depth > 0 {
                self.proof
                    .proof_mut()?
                    .compile_time_calls
                    .insert((owner, site.id));
            }
            if self.cleanup_depth > 0 {
                self.proof
                    .proof_mut()?
                    .cleanup_calls
                    .insert((owner, site.id));
            }
            if matches!(site.kind, SemanticSiteRecipeKind::OriginalCall | SemanticSiteRecipeKind::CallableCall { .. })
                && let Some(result) = self.special_call_result(owner, expression, arguments, variables, wanted, site.id, depth)?
            {
                return self.record_expression_type(owner, expression.id, result);
            }
            let (target, actuals) = self.original_call_target(owner, expression, variables, site, depth)?;
            let definition = self.definition(target)?;
            if !self.proof.complete.contains(&target)
                && !self.active.contains(&target)
                && definition.kind != SemanticDefinitionKind::Closure
            {
                self.function(definition, None)?;
            }
            let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &definition.body
            else {
                return Err(bad(
                    "original call target omits its complete body/signature",
                ));
            };
            let CoreExprKind::Call { callee, .. } = &expression.kind else {
                unreachable!()
            };
            crate::dyn_metadata::semantic_operand_snapshot(
                signature,
                self.limits,
                self.left,
                1,
            )?;
            let signature = SemanticSignatureRecipe {
                bound_lifetimes: signature.bound_lifetimes,
                invocation_lifetimes: signature.invocation_lifetimes,
                parameters: signature.parameters.clone(),
                result: self.effective_result(target, &signature.result, depth + 1)?,
                result_mode: signature.result_mode,
                call_kind: signature.call_kind,
            };
            self.pay(arguments.len(), depth)?;
            let arguments: Vec<_> = arguments.iter().collect();
            let (actuals, solved_signature) = self.solve_signature(
                definition,
                owner,
                expression,
                callee,
                &arguments,
                &signature,
                &definition.parameters,
                &actuals,
                variables,
                wanted,
                false,
                depth + 1,
            )?;
            self.call_requirements(owner, definition, &actuals, depth + 1)?;
            self.description_call(owner, target, &arguments, &solved_signature, variables, depth + 1)?;
            self.pay(actuals.len(), depth)?;
            self.proof
                .proof_mut()?
                .call_targets
                .insert((owner, site.id), target);
            for (ordinal, actual) in actuals.iter().enumerate() {
                if let SemanticActualRecipe::Const { carrier, .. } = actual {
                    let carrier = copy_recipe(carrier, self.limits, self.left)?;
                    self.proof.proof_mut()?.types.insert(
                        (
                            owner,
                            Query::CallConstCarrier {
                                call: expression.id,
                                formal: ordinal as u32,
                            },
                        ),
                        carrier,
                    );
                }
            }
            self.proof
                .proof_mut()?
                .calls
                .insert((owner, site.id), actuals.clone());
            crate::dyn_metadata::semantic_operand_snapshot(
                &solved_signature,
                self.limits,
                self.left,
                1,
            )?;
            self.proof
                .proof_mut()?
                .direct_calls
                .insert((owner, site.id), solved_signature.clone());
            solved_signature.result
        };
        self.record_expression_type(owner, expression.id, result)
    }
    fn closure_expression(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        let CoreExprKind::Closure { move_capture, parameters, body, .. } = &expression.kind else {
            unreachable!();
        };
        let result = {
            let site = self
                .site(owner, SemanticOriginalNode::Expression(expression.id))?
                .ok_or_else(|| {
                    bad("original closure has no explicit child declaration site")
                })?;
            let SemanticSiteRecipeKind::Closure {
                definition,
                environment,
            } = &site.kind
            else {
                return Err(bad("original closure site changes operation kind"));
            };
            self.pay(1, depth)?;
            if self
                .proof
                .proof_mut()?
                .closure_move
                .insert(*definition, *move_capture)
                .is_some_and(|old| old != *move_capture)
            {
                return Err(bad("one original closure changes its true capture mode"));
            }
            let child = self.definition(*definition)?;
            if child.parent != Some(owner) {
                return Err(bad("original closure has a different lexical creator"));
            }
            let SemanticDefinitionBody::FunctionWithBody {
                parameters: declared,
                body: SemanticOriginalFunctionBody::Defined(child_body),
                ..
            } = &child.body
            else {
                return Err(bad(
                    "original closure omits its complete parameter/body declaration",
                ));
            };
            if parameters.len() != declared.len()
                || parameters
                    .iter()
                    .zip(declared)
                    .any(|(a, b)| a.name != b.name)
            {
                return Err(bad(
                    "closure original parameter binders differ across its real source site",
                ));
            }
            let _ = (body, child_body); // Full operation correspondence is indexed per original owner.

            self.pay(
                self.proof
                    .capture_candidates
                    .len()
                    .saturating_add(1)
                    .ilog2() as usize
                    + 2,
                depth,
            )?;
            let candidates = if let Some(candidates) =
                self.proof.capture_candidates.get(&child.id)
            {
                candidates.clone()
            } else {
                let candidates = capture_candidates::collect(child, self.limits, self.left)?;
                self.pay(2, depth)?;
                self.proof
                    .proof_mut()?
                    .capture_candidates
                    .insert(child.id, candidates.clone());
                candidates
            };
            let mut captured = BTreeMap::new();
            let mut given = BTreeSet::new();
            for (name, value) in variables.iter_mut() {
                self.pay(
                    name.len().saturating_add(
                        candidates.names.len().saturating_add(1).ilog2() as usize + 2,
                    ),
                    depth,
                )?;
                if !candidates.opaque_type_syntax && !candidates.names.contains(name) {
                    continue;
                }
                self.pay(
                    value
                        .origin
                        .copy_work()
                        .saturating_add(value.taken_fields.len()),
                    depth,
                )?;
                for field in &value.taken_fields {
                    self.pay(field.len(), depth)?;
                }
                let inferred = !*move_capture && candidates.may_give && !value.borrowed
                    && matches!(self.observation_type(owner, &value.ty, depth + 1)?, SemanticTypeRecipe::Managed(_))
                    && zil_ir::original_capture_use(child, name, zil_ir::CaptureUse::Give,
                        |work, search_depth, _| self.pay(work, depth.saturating_add(search_depth)),
                        |message, span| Diagnostic::unsupported(Phase::Types, message, Some(span)),
                    )?.is_some();
                if inferred {
                    self.pay(name.len().saturating_add(given.len().saturating_add(1).ilog2() as usize + 1), depth)?;
                    given.insert(name.clone());
                }
                let moved = *move_capture || inferred;
                if !moved {value.reaching_write=OriginalSlotWrite::Exposed;}
                let mut value = Binding {
                    ty: value.ty.clone(),
                    mutable: value.mutable,
                    used: value.used,
                    observed: value.observed,
                    capture: value.capture,
                    borrowed: value.borrowed,
                    reaching_write: OriginalSlotWrite::Exposed,
                    origin: value.origin.clone(),
                    origin_owner: value.origin_owner,
                    initialized: value.initialized,
                    written: value.written,
                    resource_root: self.capture_resource_root(
                        owner, child.id, &value.ty, value.resource_root, moved,
                        depth + 1,
                    )?,
                    taken_fields: value.taken_fields.clone(),
                    shadowed: None,
                };
                value.capture = true;
                value.borrowed = !moved;
                value.observed = false;
                // Writes before creation belong to the creator. The child
                // independently records writes performed by its own body.
                value.written = false;
                let mut sealed = None;
                for (slot, formal) in child.environment.iter().enumerate() {
                    if let SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType {
                        scope: SemanticScopeRef::Ancestor(0),
                        source,
                    }) = &formal.source
                    {
                        let matches = match source {
                            SemanticOriginalTypeSource::NameAt {
                                at: SemanticOriginalNode::Expression(at),
                                name: source_name,
                            } => *at == expression.id && source_name == name,
                            _ => {
                                value.origin_owner == owner
                                    && Query::from(source.clone()) == value.origin
                            }
                        };
                        if matches {
                            let move_view = if *move_capture
                                && matches!(source, SemanticOriginalTypeSource::NameAt { .. })
                            {
                                self.moved_capture_type(owner, &value, depth + 1)?
                            } else {
                                None
                            };
                            let parent_type = if let Some(move_view) = move_view {
                                let snapshot = copy_recipe(&move_view, self.limits, self.left)?;
                                self.pay(name.len().saturating_add(1), depth)?;
                                self.proof.proof_mut()?.types.insert(
                                    (owner, Query::from(source.clone())),
                                    snapshot,
                                );
                                move_view
                            } else {
                                copy_recipe(&value.ty, self.limits, self.left)?
                            };
                            self.proof
                                .proof_mut()?
                                .environments
                                .insert((child.id, slot as u32), (owner, parent_type));
                            sealed = Some(SemanticTypeRecipe::Environment {
                                scope: SemanticScopeRef::Ancestor(0),
                                ordinal: slot as u32,
                            });
                        }
                    }
                }
                value.ty = std::rc::Rc::new(if let Some(sealed) = sealed {
                    sealed
                } else {
                    self.rehome(owner, child, &value.ty, depth + 1)?
                });
                captured.insert(name.clone(), value);
            }
            let result_goal = match wanted {
                Some(SemanticTypeRecipe::Code { .. }) =>
                    self.contextual_code_signature(owner, child, wanted, depth + 1)?,
                _ => self.contextual_closure_signature(owner, child, wanted, depth + 1)?,
            };
            let mut result_goal = result_goal;
            let mut unresolved_integer = false;
            if result_goal.is_none()
                && let [CoreStatement { kind: CoreStatementKind::Return(Some(value)), .. }] = child_body.statements.as_slice()
                && self.contextual_integer(value, &captured, depth + 1)?
            {
                self.pay(self.proof.literal_return_closures.len().saturating_add(1).ilog2() as usize + 2, depth)?;
                self.proof.proof_mut()?.literal_return_closures.insert(child.id);
                if let Some(goal) = self.contextual_code_goals.get(&child.id)
                    .and_then(|row| row.get(&code_context::Position::IntegerResult))
                {
                    result_goal = Some(copy_recipe(goal, self.limits, self.left)?);
                    self.proof.proof_mut()?.integer_return_obligations.insert(child.id, false);
                } else {
                    unresolved_integer = true;
                }
            }
            let parent_goal = std::mem::replace(&mut self.closure_result_goal,
                result_goal.map(|ty| (child.id, ty)));
            let checked = self.function(child, Some(captured));
            self.closure_result_goal = parent_goal;
            if unresolved_integer && checked.as_ref().is_err_and(|error| error.code == "E-INTEGER-RANGE") {
                self.contextual_code_need = Some(code_context::Demand {
                    child: child.id, position: code_context::Position::IntegerResult,
                });
            }
            checked?;
            let captures = self.proof.capture_names.get(definition).ok_or_else(|| {
                bad("original closure lacks a completed lexical capture set")
            })?;
            let cost = captures.iter().map(|name| name.len().saturating_add(1)).sum();
            self.pay(cost, depth)?;
            let captures = self.proof.capture_names.get(definition).cloned().ok_or_else(|| {
                bad("original closure lacks a completed lexical capture set")
            })?;
            for name in captures {
                if !given.is_empty() {
                    self.pay(name.len().saturating_add(1).saturating_mul(
                        given.len().saturating_add(1).ilog2() as usize + 1,
                    ), depth)?;
                }
                let value = variables.get_mut(&name)
                    .ok_or_else(|| bad("original capture lost its creator binder"))?;
                if value.capture {
                    // Forming a nested closure uses this specific enclosing
                    // capture even when the parent body never reads it directly.
                    value.observed = true;
                    if !*move_capture && !given.contains(&name) {
                        let width = self.capture_writes.get(&child.id).map_or(0, BTreeSet::len);
                        self.pay((self.capture_writes.len().saturating_add(1).ilog2() as usize + 1)
                            .saturating_add((width.saturating_add(1).ilog2() as usize + 1).saturating_mul(name.len() + 1)), depth)?;
                        value.written |= self.capture_writes.get(&child.id).is_some_and(|writes| writes.contains(&name));
                    }
                    self.observe_capture(owner, &name, value, depth + 1)?;
                }
                if !*move_capture && !given.contains(&name) {
                    let ty = self.observation_type(owner, &value.ty, depth + 1)?;
                    if !matches!(ty, SemanticTypeRecipe::Address { .. } | SemanticTypeRecipe::Desc { .. }) {
                        self.pay(1, depth)?;
                        self.proof.proof_mut()?.borrowed_capture_heads.insert(child.id);
                    }
                    continue;
                }
                if !self.copy_type(owner, &value.ty, depth + 1)? {
                    if value.borrowed {
                        return Err(bad("original move closure cannot transfer a non-Copy shared capture"));
                    }
                    if value.used || !value.initialized || !value.taken_fields.is_empty() {
                        return Err(bad("original closure captures an already transferred affine value"));
                    }
                    value.used = true;
                }
                value.observed = true;
                self.observe_capture(owner, &name, value, depth + 1)?;
            }
            if !given.is_empty() {
                self.pay(self.proof.inferred_capture_moves.len().saturating_add(1).ilog2() as usize + 1, depth)?;
                self.proof.proof_mut()?.inferred_capture_moves.insert(child.id, given);
            }
            let mut sealed_environment = Vec::new();
            for actual in environment {
                sealed_environment.push(self.resolved_actual(owner, actual, depth + 1)?);
            }
            if let Some(SemanticTypeRecipe::Code { kind: SemanticCodeKind::FunctionPointer, signature }) = wanted {
                self.checked_code_closure(owner, expression.id, child, signature, depth + 1)?
            } else {
                SemanticTypeRecipe::Closure {
                    definition: *definition,
                    arguments: Vec::new(),
                    environment: sealed_environment,
                }
            }
        };
        self.record_expression_type(owner, expression.id, result)
    }
    fn borrow_expression(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        operand: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        if matches!(expression.kind,CoreExprKind::Unary{op:UnaryOp::BorrowMut,..}) {
            let mut place=operand;
            while let CoreExprKind::Group(inner)|CoreExprKind::Ascribed {value:inner,..}=&place.kind {
                self.pay(1,depth)?;
                place=inner;
            }
            if let CoreExprKind::Name(name)=&place.kind {
                self.pay(name.len().saturating_add(variables.len().saturating_add(1).ilog2() as usize+1),depth)?;
                if let Some(slot)=variables.get_mut(name) {slot.reaching_write=OriginalSlotWrite::Exposed;}
            }
        }

        let result = {
            if matches!(
                &expression.kind,
                CoreExprKind::Unary {
                    op: UnaryOp::BorrowMut,
                    ..
                }
            ) {
                self.writable(owner, operand, variables, depth + 1)?;
            }
            let inner = self.place(owner, operand, variables, depth + 1)?;
            let inner = if matches!(inner, SemanticTypeRecipe::Opaque { .. }) {
                // Keep the true borrowed value's original occurrence. Its
                // hidden recipe must later be evaluated at that exact
                // factory boundary, never in this borrow caller's scope.
                SemanticTypeRecipe::OriginalType {
                    scope: SemanticScopeRef::Ancestor(0),
                    source: SemanticOriginalTypeSource::Expression(operand.id),
                }
            } else {
                inner
            };
            let occurrence = self.borrow_life(owner, expression.id)?;
            let lifetime = if let Some(SemanticTypeRecipe::Address {
                lifetime,
                inner: expected,
                ..
            }) = wanted
            {
                if matches!(
                    lifetime,
                    SemanticOuterLifeRecipe::Class(
                        SemanticLifeRecipe::Implicit {
                            scope: SemanticScopeRef::Ancestor(u32::MAX),
                            ..
                        } | SemanticLifeRecipe::Environment {
                            scope: SemanticScopeRef::Ancestor(u32::MAX),
                            ..
                        }
                    )
                ) {
                    occurrence
                } else {
                    self.same(&inner, expected)?;
                    self.contextual_borrow_role(
                        owner,
                        expression,
                        operand,
                        variables,
                        &inner,
                        &occurrence,
                        lifetime,
                        depth + 1,
                    )?
                }
            } else {
                occurrence
            };
            SemanticTypeRecipe::Address {
                mutable: matches!(
                    expression.kind,
                    CoreExprKind::Unary {
                        op: UnaryOp::BorrowMut,
                        ..
                    }
                ),
                lifetime,
                inner: Box::new(inner),
            }
        };
        self.record_expression_type(owner, expression.id, result)
    }
    fn record_expression_type(
        &mut self,
        owner: SemanticDefinitionId,
        node: NodeId,
        result: SemanticTypeRecipe,
    ) -> R<SemanticTypeRecipe> {
        let copy = copy_recipe(&result, self.limits, self.left)?;
        self.proof
            .proof_mut()?
            .types
            .insert((owner, Query::Expression(node)), copy);
        Ok(result)
    }
    fn insert_binding(
        &mut self,
        variables: &mut BTreeMap<String, Binding>,
        name: String,
        mut binding: Binding,
        depth: usize,
    ) -> R<()> {
        self.pay(name.len().saturating_add(1), depth)?;
        let mut history = variables.get(&name);
        let mut level = depth;
        while let Some(value) = history {
            self.pay(1, level)?;
            level = level
                .checked_add(1)
                .ok_or_else(|| bad("original lexical depth overflows"))?;
            history = value.shadowed.as_deref();
        }
        self.pay(1, level)?;
        binding.shadowed = variables.remove(&name).map(std::rc::Rc::new);
        variables.insert(name, binding);
        Ok(())
    }
    fn binding_at<'c>(&mut self, mut value: &'c Binding, origin: &Query) -> R<&'c Binding> {
        loop {
            self.pay(1, 0)?;
            if &value.origin == origin {
                return Ok(value);
            }
            value = value.shadowed.as_deref().ok_or_else(|| {
                bad("original control edge loses a real lexical binding identity")
            })?;
        }
    }
    fn merge_binding(
        &mut self,
        output: &mut Binding,
        branches: &[&Binding],
        depth: usize,
    ) -> R<()> {
        self.pay(branches.len(), depth)?;
        let mut used = false;
        let mut initialized = !branches.is_empty();
        let mut reaching_write:Option<OriginalSlotWrite>=None;
        let mut taken_fields = BTreeSet::new();
        for branch in branches {
            let inner = self.binding_at(branch, &output.origin)?;
            self.same_binding_type(output, inner)?;
            used |= inner.used;
            output.observed |= inner.observed;
            output.written |= inner.written;
            initialized &= inner.initialized;
            reaching_write=Some(match reaching_write {
                None=>inner.reaching_write.clone(),
                Some(old)=>old.merged(&inner.reaching_write,|work|self.pay(work,depth))?,
            });
            self.pay(inner.taken_fields.len(), depth)?;
            for field in &inner.taken_fields {
                self.pay(field.len(), depth)?;
                taken_fields.insert(field.clone());
            }
        }
        output.taken_fields = taken_fields;
        // A dominating reassignment can restore availability; prior used bits
        // are not an additional incoming control-flow edge.
        output.used = used;
        output.initialized = initialized;
        output.reaching_write = reaching_write.unwrap_or_default();
        if let Some(hidden) = &mut output.shadowed {
            if std::rc::Rc::strong_count(hidden) > 1 {
                self.charge_binding_clone(hidden)?;
            }
            self.merge_binding(std::rc::Rc::make_mut(hidden), branches, depth + 1)?;
        }
        Ok(())
    }
    fn merge_variables(
        &mut self,
        output: &mut BTreeMap<String, Binding>,
        branches: &[BTreeMap<String, Binding>],
    ) -> R<()> {
        self.pay(output.len().saturating_mul(branches.len()), 0)?;
        for (name, value) in output {
            self.pay(branches.len(), 0)?;
            let mut inputs = Vec::new();
            for branch in branches {
                inputs.push(
                    branch
                        .get(name)
                        .ok_or_else(|| bad("original control edge loses its lexical name"))?,
                );
            }
            self.merge_binding(value, &inputs, 0)?;
        }
        Ok(())
    }
    // An exclusive use through a captured address requires exclusive
    // invocation even when its lexical binding is immutable. Establish the
    // address permission first, then record the rooted capture effect.
    fn observe_exclusive_capture(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<()> {
        let Some(root) = zil_ir::exclusive_place_root(
            expression, |step, _| self.pay(1, depth.saturating_add(step)),
        )? else { return Ok(()); };
        let name = root.name;
        self.pay(name.len().saturating_add(variables.len().saturating_add(1).ilog2() as usize), depth)?;
        if let Some(value) = variables.get_mut(name).filter(|value| value.capture) {
            value.written = true;
            value.observed = true;
            self.observe_capture(owner, name, value, depth + 1)?;
        }
        Ok(())
    }
    fn writable(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        let mut description = false;
        let ty = self.writable_place(owner, expression, variables, &mut description, depth)
            .map_err(|diagnostic| super::locate_original(diagnostic, expression.span))?;
        if description && !self.copy_type(owner, &ty, depth + 1)? {
            return Err(bad("original descriptor write requires a Copy field"));
        }
        Ok(ty)
    }
    fn writable_place(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        description: &mut bool,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(1, depth)?;
        self.snapshot(
            owner,
            SemanticOriginalNode::Expression(expression.id),
            variables,
        )?;
        let ty = match &expression.kind {
            CoreExprKind::Name(name) => {
                let value = variables
                    .get_mut(name)
                    .ok_or_else(|| bad("original assignment has no lexical target"))?;
                if !value.mutable {
                    return Err(bad("original assignment targets an immutable binder"));
                }
                value.written = true;
                value.observed = true;
                self.observe_capture(owner, name, value, depth + 1)?;
                copy_recipe(&value.ty, self.limits, self.left)
            }
            CoreExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } => {
                let address = self.observe_address_operand(owner, operand, variables, depth + 1)?;
                let address = self.observation_type(owner, &address, depth + 1)?;
                let SemanticTypeRecipe::Address {
                    mutable: true,
                    inner,
                    ..
                } = address
                else {
                    return Err(bad("original write requires an exclusive address"));
                };
                self.observe_exclusive_capture(owner, operand, variables, depth + 1)?;
                Ok(*inner)
            }
            CoreExprKind::Group(inner) => self.writable_place(owner, inner, variables, description, depth + 1),
            CoreExprKind::Member { object, member, .. } => {
                if let Some(field) =
                    self.package_field(owner, object, member, variables, false, depth + 1)?
                {
                    self.writable_place(owner, object, variables, description, depth + 1)?;
                    let copy = copy_recipe(&field, self.limits, self.left)?;
                    self.proof
                        .proof_mut()?
                        .types
                        .insert((owner, Query::Expression(expression.id)), copy);
                    return Ok(field);
                }
                let object_ty = self.writable_projection_base(owner, object, variables, description, depth + 1)?;
                self.field(owner, &object_ty, member, depth + 1)
            }
            CoreExprKind::Slice { object, .. } => {
                self.writable_projection_base(owner, object, variables, description, depth + 1)?;
                self.expr(owner, expression, variables, None, depth + 1)
            }
            CoreExprKind::Index { object, index } => {
                let object_ty = self.writable_projection_base(owner, object, variables, description, depth + 1)?;
                self.index_type(owner, &object_ty, index, variables, depth + 1)
            }
            _ => Err(bad("original assignment does not select a writable place")),
        }?;
        let copy = copy_recipe(&ty, self.limits, self.left)?;
        self.proof
            .proof_mut()?
            .types
            .insert((owner, Query::Expression(expression.id)), copy);
        self.expression_result_origin(owner, expression, variables, &ty, depth + 1)?;
        Ok(ty)
    }
    /// A field, index or slice inherits access from its actual place. A local
    /// resource owner can describe its live payload without rebinding the
    /// owner. An owner reached through another projection must still satisfy
    /// the access obligation on that projection (including any shared base).
    fn writable_projection_base(
        &mut self, owner: SemanticDefinitionId, object: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>, description: &mut bool, depth: usize,
    ) -> R<SemanticTypeRecipe> {
        let ty = self.place(owner, object, variables, depth + 1)?;
        let observed = self.observation_type(owner, &ty, depth + 1)?;
        match observed {
            SemanticTypeRecipe::Desc { .. } => {
                let roots = self.resource_place_roots(owner, object, variables, depth + 1)?;
                self.record_resource_write(roots, None, depth + 1)?;
                *description = true;
            }
            SemanticTypeRecipe::Address { mutable: true, .. } =>
                self.observe_exclusive_capture(owner, object, variables, depth + 1)?,
            SemanticTypeRecipe::Address { mutable: false, .. } =>
                return Err(bad("original projection write uses a shared address")),
            SemanticTypeRecipe::Managed(_) => {
                let roots = self.resource_place_roots(owner, object, variables, depth + 1)?;
                self.record_resource_write(roots, None, depth + 1)?;
                let mut root = object;
                while let CoreExprKind::Group(inner) = &root.kind {
                    self.pay(1, depth)?;
                    root = inner;
                }
                if !matches!(root.kind, CoreExprKind::Name(_)) {
                    self.writable_place(owner, object, variables, description, depth + 1)?;
                }
                *description = true;
            }
            _ => {
                self.writable_place(owner, object, variables, description, depth + 1)?;
            }
        }
        Ok(ty)
    }
    fn index_type(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        index: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(1, depth)?;
        let index_ty = self.expr(owner, index, variables, None, depth + 1)?;
        let index_ty = self.logical(owner, &index_ty, depth + 1)?;
        let SemanticTypeRecipe::Atom(index_ty) = index_ty else {
            return Err(bad("original index is not a primitive integer"));
        };
        if !index_ty.is_integer() {
            return Err(bad("original index is not an integer"));
        }
        let mut ty = self.logical(owner, ty, depth + 1)?;
        while let SemanticTypeRecipe::Address { inner, .. } = ty {
            self.pay(1, depth)?;
            ty = self.logical(owner, &inner, depth + 1)?;
        }
        match ty {
            SemanticTypeRecipe::Array { element, .. } | SemanticTypeRecipe::Slice(element) => {
                Ok(*element)
            }
            _ => Err(bad(
                "original indexing requires a complete array or borrowed slice",
            )),
        }
    }
    fn pattern_reference_mode(&mut self, pattern: &SemanticOriginalPattern, depth: usize) -> R<Option<bool>> {
        original_pattern_reference_mode(pattern, depth, &mut |depth| self.pay(1, depth))
    }

    fn original_package_pattern(
        &mut self, owner: SemanticDefinitionId, pattern: &SemanticOriginalPattern, depth: usize,
    ) -> R<bool> {
        let definition = match &pattern.kind {
            CorePatternKind::RecordConstructor { definition, .. } => *definition,
            CorePatternKind::Record { ty, .. } => match self.resolve(owner, ty, depth + 1)? {
                SemanticTypeRecipe::Nominal { definition, .. } => Some(definition),
                _ => None,
            },
            _ => None,
        };
        let Some(definition) = definition else { return Ok(false); };
        Ok(matches!(self.definition(definition)?.body,
            SemanticDefinitionBody::Nominal { kind: LifeNominalKind::Package, .. }))
    }

    fn pattern_subject_type(
        &mut self, owner: SemanticDefinitionId, value: &SemanticOriginalExpr,
        mode: Option<bool>, package: bool, variables: &mut BTreeMap<String, Binding>,
        wanted: Option<&SemanticTypeRecipe>, depth: usize,
    ) -> R<SemanticTypeRecipe> {
        match mode {
            None if package && matches!(value.kind,
                CoreExprKind::Name(_) | CoreExprKind::Member { .. } | CoreExprKind::Group(_)) =>
            {
                self.place(owner, value, variables, depth)
            }
            None => self.expr(owner, value, variables, wanted, depth),
            Some(false) => self.place(owner, value, variables, depth),
            Some(true) => {
                // A fresh rvalue can establish its own writable temporary.
                // A lexical or indirect place must retain its real permission.
                if let CoreExprKind::Name(name) = &value.kind
                    && variables.get(name).is_some_and(|binding| !binding.mutable)
                {
                    return Err(error(
                        "E-IMMUTABLE-ASSIGNMENT",
                        value.span,
                        "exclusive pattern requires a mutable place",
                    ));
                }
                if matches!(value.kind, CoreExprKind::Name(_) | CoreExprKind::Member { .. }
                    | CoreExprKind::Group(_) | CoreExprKind::Index { .. }
                    | CoreExprKind::Unary { op: UnaryOp::Dereference, .. })
                {
                    self.writable(owner, value, variables, depth + 1)?;
                }
                self.place(owner, value, variables, depth)
            }
        }
    }

    fn pattern(
        &mut self,
        owner: SemanticDefinitionId,
        statement: NodeId,
        path: &mut Vec<u32>,
        pattern: &SemanticOriginalPattern,
        ty: &SemanticTypeRecipe,
        mutable: bool,
        variables: &mut BTreeMap<String, Binding>,
        names: &mut BTreeSet<String>,
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        match &pattern.kind {
            CorePatternKind::Name(name) => {
                self.pay(name.len().saturating_add(path.len()), depth)?;
                if !names.insert(name.clone()) {
                    return Err(bad("original pattern repeats a lexical binder"));
                }
                let origin = Query::BindingProjection {
                    statement,
                    pattern: path.clone(),
                };
                let copy = copy_recipe(ty, self.limits, self.left)?;
                self.proof
                    .proof_mut()?
                    .types
                    .insert((owner, origin.clone()), copy);
                let binding_type = copy_recipe(ty, self.limits, self.left)?;
                self.insert_binding(
                    variables,
                    name.clone(),
                    Binding {
                        ty: std::rc::Rc::new(binding_type),
                        mutable,
                        used: false,
                        observed: false,
                        capture: false,
                        borrowed: false, reaching_write: OriginalSlotWrite::Unknown,
                        origin,
                        origin_owner: owner,
                        initialized: true,
                        written: false,
                        resource_root: None,
                        taken_fields: BTreeSet::new(),
                        shadowed: None,
                    },
                    depth,
                )?;
            }
            CorePatternKind::Wildcard => {}
            CorePatternKind::Tuple(patterns) if patterns.is_empty() => {
                self.same(ty, &SemanticTypeRecipe::Atom(Type::Unit))?;
            }
            CorePatternKind::Tuple(patterns) => {
                let ty = self.logical(owner, ty, depth + 1)?;
                let SemanticTypeRecipe::Tuple(fields) = ty else {
                    return Err(bad("original tuple pattern changes subject shape"));
                };
                if patterns.len() != fields.len() {
                    return Err(bad("original tuple pattern changes subject arity"));
                }
                for (ordinal, (pattern, ty)) in patterns.iter().zip(&fields).enumerate() {
                    self.pay(1, depth)?;
                    path.push(ordinal as u32);
                    self.pattern(
                        owner,
                        statement,
                        path,
                        pattern,
                        ty,
                        mutable,
                        variables,
                        names,
                        depth + 1,
                    )?;
                    path.pop();
                }
            }
            CorePatternKind::Reference {
                mutable: requested,
                pattern: binding,
            } => {
                if !matches!(binding.kind, CorePatternKind::Name(_) | CorePatternKind::Wildcard) {
                    return Err(bad("original reference pattern requires a name or discard"));
                }
                let implicit = &self.definition(owner)?.implicit_lifetimes;
                self.pay(implicit.len().saturating_mul(path.len().saturating_add(2)), depth)?;
                let mut selected = implicit.iter().filter(|row| {
                    row.occurrence_path.strip_prefix(&[ORIGINAL_PATTERN_BORROW, statement.0]) == Some(path.as_slice())
                });
                let ordinal = selected.next().ok_or_else(|| bad("original reference pattern lacks its occurrence"))?.ordinal;
                if selected.next().is_some() { return Err(bad("original reference pattern repeats its occurrence")); }
                if matches!(binding.kind, CorePatternKind::Name(_)) {
                    self.pay(self.proof.pattern_borrows.len().saturating_add(1).ilog2() as usize + 1, depth)?;
                    let previous = self.proof.proof_mut()?.pattern_borrows.insert((owner, pattern.span), (ordinal, *requested));
                    if previous.is_some_and(|value| value != (ordinal, *requested)) {
                        return Err(bad("original reference pattern has competing borrow occurrences"));
                    }
                }
                let inner = copy_recipe(ty, self.limits, self.left)?;
                let address = SemanticTypeRecipe::Address {
                    mutable: *requested,
                    lifetime: SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Implicit {
                        scope: SemanticScopeRef::Ancestor(0), ordinal,
                    }),
                    inner: Box::new(inner),
                };
                self.pay(1, depth)?;
                path.push(0);
                self.pattern(
                    owner,
                    statement,
                    path,
                    binding,
                    &address,
                    mutable,
                    variables,
                    names,
                    depth + 1,
                )?;
                path.pop();
            }
            CorePatternKind::ErrorArm { is_error, pattern } => {
                let ty = self.logical(owner, ty, depth + 1)?;
                let SemanticTypeRecipe::ErrorUnion { error, result } = ty else {
                    return Err(bad(
                        "original error pattern requires the actual error union",
                    ));
                };
                self.pay(1, depth)?;
                path.push(0);
                self.pattern(
                    owner,
                    statement,
                    path,
                    pattern,
                    if *is_error { &error } else { &result },
                    mutable,
                    variables,
                    names,
                    depth + 1,
                )?;
                path.pop();
            }
            CorePatternKind::Record {
                ty: annotation,
                fields,
            } => {
                let annotation = self.resolve(owner, annotation, depth + 1)?;
                self.same(ty, &annotation)?;
                self.record_pattern(
                    owner,
                    statement,
                    path,
                    fields,
                    &annotation,
                    mutable,
                    variables,
                    names,
                    depth + 1,
                )?;
            }
            CorePatternKind::RecordConstructor { definition, fields } => {
                let ty = self.logical(owner, ty, depth + 1)?;
                if let Some(definition) = definition {
                    if !matches!(&ty, SemanticTypeRecipe::Nominal { definition: actual, .. } if definition == actual)
                    {
                        return Err(bad("original record pattern changes nominal identity"));
                    }
                }
                self.record_pattern(
                    owner,
                    statement,
                    path,
                    fields,
                    &ty,
                    mutable,
                    variables,
                    names,
                    depth + 1,
                )?;
            }
            CorePatternKind::Variant {
                ty: annotation,
                variant,
                fields,
            } => {
                let annotation = self.resolve(owner, annotation, depth + 1)?;
                self.same(ty, &annotation)?;
                self.variant_pattern(
                    owner,
                    statement,
                    path,
                    variant,
                    fields,
                    &annotation,
                    mutable,
                    variables,
                    names,
                    depth + 1,
                )?;
            }
            CorePatternKind::Constructor {
                definition,
                variant,
                fields,
            } => {
                let ty = self.logical(owner, ty, depth + 1)?;
                if let Some(definition) = definition {
                    if !matches!(&ty, SemanticTypeRecipe::Nominal { definition: actual, .. } if actual == definition)
                    {
                        return Err(bad("original variant pattern changes nominal identity"));
                    }
                }
                self.variant_pattern(
                    owner,
                    statement,
                    path,
                    variant,
                    fields,
                    &ty,
                    mutable,
                    variables,
                    names,
                    depth + 1,
                )?;
            }
            CorePatternKind::Bool(_) => self.same(ty, &SemanticTypeRecipe::Atom(Type::Bool))?,
            CorePatternKind::Integer(text) => {
                let SemanticTypeRecipe::Atom(expected) = ty else {
                    return Err(bad("original integer pattern has a nonprimitive subject"));
                };
                if !expected.is_integer() {
                    return Err(bad("original integer pattern has a noninteger subject"));
                }
                integer(text, Some(expected), false, pattern.span)?;
            }
            CorePatternKind::Float(text) => {
                let SemanticTypeRecipe::Atom(expected) = ty else {
                    return Err(bad("original float pattern has a nonprimitive subject"));
                };
                if !expected.is_float() {
                    return Err(bad("original float pattern has a nonfloat subject"));
                }
                floats::literal(text, Some(expected), pattern.span)?;
            }
            CorePatternKind::String(_) | CorePatternKind::Character(_) => {
                return Err(pending(
                    "original text pattern requires its checked comparison scheme",
                ));
            }
        }
        Ok(())
    }
    fn record_pattern(
        &mut self,
        owner: SemanticDefinitionId,
        statement: NodeId,
        path: &mut Vec<u32>,
        fields: &[(String, SemanticOriginalPattern)],
        ty: &SemanticTypeRecipe,
        mutable: bool,
        variables: &mut BTreeMap<String, Binding>,
        names: &mut BTreeSet<String>,
        depth: usize,
    ) -> R<()> {
        let SemanticTypeRecipe::Nominal {
            definition,
            arguments: _,
        } = ty
        else {
            return Err(bad("original record pattern requires a nominal product"));
        };
        let definition = self.definition(*definition)?;
        let SemanticDefinitionBody::Nominal {
            kind: LifeNominalKind::Record | LifeNominalKind::Package,
            body: SemanticNominalBody::Product { fields: declared },
            ..
        } = &definition.body
        else {
            return Err(bad("original record pattern selects another constructor"));
        };
        if fields.len() != declared.len() {
            return Err(bad("original record pattern omits declared fields"));
        }
        self.pay(fields.len(), depth)?;
        let mut labels = BTreeSet::new();
        for (ordinal, (name, pattern)) in fields.iter().enumerate() {
            self.pay(name.len(), depth)?;
            if !labels.insert(name.clone()) {
                return Err(bad("original record pattern repeats a field"));
            }
            let field = self.field(owner, ty, name, depth + 1)?;
            self.pay(1, depth)?;
            path.push(ordinal as u32);
            self.pattern(
                owner,
                statement,
                path,
                pattern,
                &field,
                mutable,
                variables,
                names,
                depth + 1,
            )?;
            path.pop();
        }
        Ok(())
    }
    fn variant_pattern(
        &mut self,
        owner: SemanticDefinitionId,
        statement: NodeId,
        path: &mut Vec<u32>,
        name: &str,
        patterns: &[SemanticOriginalPattern],
        ty: &SemanticTypeRecipe,
        mutable: bool,
        variables: &mut BTreeMap<String, Binding>,
        names: &mut BTreeSet<String>,
        depth: usize,
    ) -> R<()> {
        let SemanticTypeRecipe::Nominal {
            definition,
            arguments,
        } = ty
        else {
            return Err(bad("original variant pattern requires its nominal sum"));
        };
        let definition = self.definition(*definition)?;
        let SemanticDefinitionBody::Nominal {
            body: SemanticNominalBody::Enum { variants },
            ..
        } = &definition.body
        else {
            return Err(bad(
                "original variant pattern selects another declaration kind",
            ));
        };
        self.pay(variants.len(), depth)?;
        let variant = variants
            .iter()
            .find(|v| v.name == name)
            .ok_or_else(|| bad("original variant pattern names an absent constructor"))?;
        if patterns.len() != variant.fields.len() {
            return Err(bad("original variant pattern changes payload arity"));
        }
        for (ordinal, (pattern, field)) in patterns.iter().zip(&variant.fields).enumerate() {
            let field =
                self.instantiate_field(owner, definition.id, field, arguments, depth + 1)?;
            self.pay(1, depth)?;
            path.push(ordinal as u32);
            self.pattern(
                owner,
                statement,
                path,
                pattern,
                &field,
                mutable,
                variables,
                names,
                depth + 1,
            )?;
            path.pop();
        }
        Ok(())
    }
    fn original_bool(
        &mut self,
        expression: &SemanticOriginalExpr,
        depth: usize,
    ) -> R<Option<bool>> {
        self.pay(1, depth)?;
        match &expression.kind {
            CoreExprKind::Bool(value) => Ok(Some(*value)),
            CoreExprKind::Group(inner) => self.original_bool(inner, depth + 1),
            CoreExprKind::Unary {
                op: UnaryOp::Not,
                operand,
            } => Ok(self.original_bool(operand, depth + 1)?.map(|value| !value)),
            _ => Ok(None),
        }
    }
    fn loop_body(
        &mut self,
        owner: SemanticDefinitionId,
        body: &SemanticOriginalBlock,
        variables: &mut BTreeMap<String, Binding>,
        result: &SemanticTypeRecipe,
        may_skip: bool,
        repeat_entry: Option<&BTreeMap<String, Binding>>,
        may_enter: bool,
        depth: usize,
    ) -> R<bool> {
        self.pay(1, depth)?;
        let entry = self.clone_variables(variables)?;
        let mut inner = self.clone_variables(variables)?;
        self.loops.push(LoopFrame::default());
        let falls = self.block(owner, body, &mut inner, result, depth + 1)?;
        let mut frame = self
            .loops
            .pop()
            .ok_or_else(|| bad("original loop frame disappeared"))?;
        if falls {
            self.pay(1, depth)?;
            frame.continues.push(inner);
        }
        // An impossible original literal condition still receives full typing,
        // but its hypothetical edges are not runtime ownership transfers.
        if !may_enter {
            return Ok(true);
        }
        // The next condition executes in the incoming loop Gamma as well.
        // Include its transfers when checking every real back edge.
        let repeated = repeat_entry.unwrap_or(&entry);
        for edge in &frame.continues {
            for (name, before) in repeated {
                self.pay(1, depth)?;
                let after = self.binding_at(
                    edge.get(name)
                        .ok_or_else(|| bad("original loop edge loses its lexical name"))?,
                    &before.origin,
                )?;
                self.same_binding_type(before, after)?;
                self.pay(after.taken_fields.len(), depth)?;
                if after
                    .taken_fields
                    .iter()
                    .any(|field| !before.taken_fields.contains(field))
                {
                    return Err(bad(
                        "original loop back edge consumes a package field needed by another iteration",
                    ));
                }
                if !before.used && after.used || before.initialized && !after.initialized {
                    return Err(bad(
                        "original loop back edge consumes an affine value needed by another iteration",
                    ));
                }
            }
        }
        // Condition-false exits may happen after zero or any number of iterations.
        if may_skip {
            self.pay(1, depth)?;
            frame.breaks.push(entry);
            for edge in frame.continues {
                self.pay(1, depth)?;
                frame.breaks.push(edge);
            }
        }
        let falls = !frame.breaks.is_empty();
        if falls {
            self.merge_variables(variables, &frame.breaks)?;
        }
        Ok(falls)
    }
    fn unit_result(
        &mut self,
        owner: SemanticDefinitionId,
        result: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        let unit = SemanticTypeRecipe::Atom(Type::Unit);
        let infer = matches!(
            result,
            SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source: SemanticOriginalTypeSource::Result
            }
        ) || matches!(result, SemanticTypeRecipe::Opaque { hidden,.. } if matches!(hidden.as_ref(), SemanticTypeRecipe::OriginalType { scope:SemanticScopeRef::Ancestor(0), source:SemanticOriginalTypeSource::Result }));
        if infer {
            if let Some(previous) = self.proof.types.get(&(owner, Query::Result)) {
                let previous = copy_recipe(previous, self.limits, self.left)?;
                self.same(&previous, &unit)?;
            } else {
                self.pay(1, depth)?;
                self.proof
                    .proof_mut()?
                    .types
                    .insert((owner, Query::Result), unit);
            }
        }
        let expected = self.resolve(owner, result, depth + 1)?;
        match expected {
            SemanticTypeRecipe::ErrorUnion { error, result } => {
                if (error.as_ref() == &SemanticTypeRecipe::Atom(Type::Unit))
                    == (result.as_ref() == &SemanticTypeRecipe::Atom(Type::Unit))
                {
                    return Err(bad("original unit error injection is ambiguous or absent"));
                }
                Ok(())
            }
            SemanticTypeRecipe::Opaque {
                definition, hidden, ..
            } => {
                if definition != owner {
                    return Err(bad(
                        "original return constructs another definition's opaque result",
                    ));
                }
                self.same(&hidden, &SemanticTypeRecipe::Atom(Type::Unit))
            }
            expected => self.same(&expected, &SemanticTypeRecipe::Atom(Type::Unit)),
        }
    }
    fn block(
        &mut self,
        owner: SemanticDefinitionId,
        block: &SemanticOriginalBlock,
        variables: &mut BTreeMap<String, Binding>,
        result: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<bool> {
        self.pay(block.statements.len(), depth)?;
        let mut live = true;
        for statement in &block.statements {
            let was_live = live;
            self.pay(self.loops.len(), depth)?;
            let exits: Vec<_> = self
                .loops
                .iter()
                .map(|frame| (frame.breaks.len(), frame.continues.len()))
                .collect();
            self.snapshot(
                owner,
                SemanticOriginalNode::Statement(statement.id),
                variables,
            )?;
            match &statement.kind {
                CoreStatementKind::Binding { .. } => {
                    self.binding_statement(owner, statement, variables, depth)?;
                }
                CoreStatementKind::Return(_) if self.cleanup_depth != 0 => {
                    return Err(error("E-DEFER-CONTROL", statement.span, "a deferred action cannot return from its activation"));
                }
                CoreStatementKind::Return(Some(value)) => {
                    self.return_statement(owner, statement, value, result, variables, depth)?;
                    live = false;
                }
                CoreStatementKind::Return(None) => {
                    self.unit_result(owner, result, depth + 1)?;
                    self.capture_exit(owner, variables, depth + 1)?;
                    live = false;
                }
                CoreStatementKind::Expression(value) => {
                    self.expr(owner, value, variables, None, depth + 1)?;
                }
                CoreStatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    let condition = self.expr(
                        owner,
                        condition,
                        variables,
                        Some(&SemanticTypeRecipe::Atom(Type::Bool)),
                        depth + 1,
                    )?;
                    self.same(&condition, &SemanticTypeRecipe::Atom(Type::Bool))?;
                    let mut branches = Vec::new();
                    self.pay(2, depth)?;
                    let mut left = self.clone_variables(variables)?;
                    if self.block(owner, then_body, &mut left, result, depth + 1)? {
                        branches.push(left);
                    }
                    let mut right = self.clone_variables(variables)?;
                    if let Some(body) = else_body {
                        if self.block(owner, body, &mut right, result, depth + 1)? {
                            branches.push(right);
                        }
                    } else {
                        branches.push(right);
                    }
                    if branches.is_empty() {
                        live = false;
                    } else {
                        self.merge_variables(variables, &branches)?;
                    }
                }
                CoreStatementKind::Block(body) | CoreStatementKind::Unsafe(body) => {
                    let mut inner = self.clone_variables(variables)?;
                    if self.block(owner, body, &mut inner, result, depth + 1)? {
                        self.merge_variables(variables, &[inner])?;
                    } else {
                        live = false;
                    }
                }
                CoreStatementKind::Defer(body) => {
                    if self.cleanup_depth != 0 {
                        return Err(error("E-DEFER-NESTED", statement.span, "a deferred action cannot register a nested defer"));
                    }
                    let (action, result, _) = body.deferred_action().ok_or_else(||
                        bad("defer requires its original lexical action"))?;
                    if result != &SemanticTypeRecipe::Atom(Type::Unit) {
                        return Err(bad("original cleanup must return Unit"));
                    }
                    let site = self.site(owner, SemanticOriginalNode::Expression(action.id))?
                        .ok_or_else(|| bad("defer action lacks its original declaration"))?;
                    let SemanticSiteRecipeKind::Closure { definition, .. } = site.kind else {
                        return Err(bad("defer operand is not the declared lexical closure"));
                    };
                    self.pay(self.proof.cleanup_actions.len().saturating_add(1).ilog2() as usize + 1, depth)?;
                    self.proof.proof_mut()?.cleanup_actions.insert(definition);
                    self.expr(owner, action, variables, None, depth + 1)?;
                }
                CoreStatementKind::Assign { target, value }
                | CoreStatementKind::CompoundAssign { target, value, .. } => {
                    self.assignment_statement(owner, statement, target, value, variables, depth)?;
                }
                CoreStatementKind::TupleBinding {
                    pattern,
                    mutable,
                    ty,
                    initializer,
                } => {
                    let annotation = ty
                        .as_ref()
                        .map(|ty| self.resolve(owner, ty, depth + 1))
                        .transpose()?;
                    let mode = self.pattern_reference_mode(pattern, depth + 1)?;
                    let package = mode.is_none() && self.original_package_pattern(owner, pattern, depth + 1)?;
                    let actual = self.pattern_subject_type(
                        owner,
                        initializer,
                        mode,
                        package,
                        variables,
                        annotation.as_ref(),
                        depth + 1,
                    )?;
                    if let Some(annotation) = &annotation {
                        self.same(annotation, &actual)?;
                    }
                    let copy = copy_recipe(&actual, self.limits, self.left)?;
                    self.proof
                        .proof_mut()?
                        .types
                        .insert((owner, Query::Binding(statement.id)), copy);
                    self.pattern(
                        owner,
                        statement.id,
                        &mut Vec::new(),
                        pattern,
                        &actual,
                        *mutable,
                        variables,
                        &mut BTreeSet::new(),
                        depth + 1,
                    )?;
                }
                CoreStatementKind::IfLet {
                    pattern,
                    value,
                    then_body,
                    else_body,
                } => {
                    let mode = self.pattern_reference_mode(pattern, depth + 1)?;
                    let package = mode.is_none() && self.original_package_pattern(owner, pattern, depth + 1)?;
                    let ty = self.pattern_subject_type(owner, value, mode, package, variables, None, depth + 1)?;
                    self.pay(2, depth)?;
                    let mut branches = Vec::new();
                    let mut matched = self.clone_variables(variables)?;
                    self.pattern(
                        owner,
                        statement.id,
                        &mut Vec::new(),
                        pattern,
                        &ty,
                        false,
                        &mut matched,
                        &mut BTreeSet::new(),
                        depth + 1,
                    )?;
                    let irrefutable = self.irrefutable_original_pattern(owner, &ty, pattern, depth + 1)?;
                    if self.block(owner, then_body, &mut matched, result, depth + 1)? {
                        branches.push(matched);
                    }
                    let mut other = self.clone_variables(variables)?;
                    if let Some(body) = else_body {
                        if self.block(owner, body, &mut other, result, depth + 1)? && !irrefutable {
                            branches.push(other);
                        }
                    } else if !irrefutable {
                        branches.push(other);
                    }
                    if branches.is_empty() {
                        live = false;
                    } else {
                        self.merge_variables(variables, &branches)?;
                    }
                }
                CoreStatementKind::Match { value, arms } => {
                    let mut mode = None;
                    let mut package = false;
                    for arm in arms {
                        if let Some(mutable) = self.pattern_reference_mode(&arm.pattern, depth + 1)? {
                            mode = Some(mode.unwrap_or(false) || mutable);
                        }
                        if mode.is_none() && !package {
                            package = self.original_package_pattern(owner, &arm.pattern, depth + 1)?;
                        }
                    }
                    let ty = self.pattern_subject_type(owner, value, mode, package, variables, None, depth + 1)?;
                    self.pay(arms.len(), depth)?;
                    let mut branches = Vec::new();
                    for (ordinal, arm) in arms.iter().enumerate() {
                        let mut arm_variables = self.clone_variables(variables)?;
                        self.pay(1, depth)?;
                        let mut path = vec![ordinal as u32];
                        self.pattern(
                            owner,
                            statement.id,
                            &mut path,
                            &arm.pattern,
                            &ty,
                            false,
                            &mut arm_variables,
                            &mut BTreeSet::new(),
                            depth + 1,
                        )?;
                        if self.block(owner, &arm.body, &mut arm_variables, result, depth + 1)? {
                            branches.push(arm_variables);
                        }
                    }
                    self.exhaustive(owner, &ty, arms, depth + 1)?;
                    if branches.is_empty() {
                        live = false;
                    } else {
                        self.merge_variables(variables, &branches)?;
                    }
                }
                CoreStatementKind::While { condition, body } => {
                    let known = self.original_bool(condition, depth + 1)?;
                    let entry = self.clone_variables(variables)?;
                    let condition = self.expr(
                        owner,
                        condition,
                        variables,
                        Some(&SemanticTypeRecipe::Atom(Type::Bool)),
                        depth + 1,
                    )?;
                    self.same(&condition, &SemanticTypeRecipe::Atom(Type::Bool))?;
                    if !self.loop_body(
                        owner,
                        body,
                        variables,
                        result,
                        known != Some(true),
                        Some(&entry),
                        known != Some(false),
                        depth + 1,
                    )? {
                        live = false;
                    }
                }
                CoreStatementKind::WhileLet {
                    pattern,
                    value,
                    body,
                } => {
                    let mode = self.pattern_reference_mode(pattern, depth + 1)?;
                    let package = mode.is_none() && self.original_package_pattern(owner, pattern, depth + 1)?;
                    let ty = self.pattern_subject_type(owner, value, mode, package, variables, None, depth + 1)?;
                    let mut iteration = self.clone_variables(variables)?;
                    self.pattern(
                        owner,
                        statement.id,
                        &mut Vec::new(),
                        pattern,
                        &ty,
                        false,
                        &mut iteration,
                        &mut BTreeSet::new(),
                        depth + 1,
                    )?;
                    self.loop_body(
                        owner,
                        body,
                        &mut iteration,
                        result,
                        true,
                        None,
                        true,
                        depth + 1,
                    )?;
                    self.merge_variables(variables, &[iteration])?;
                }
                CoreStatementKind::ForRange {
                    name,
                    start,
                    end,
                    body,
                } => {
                    let ty = self.range_element(owner, start, end, variables, depth + 1)?;
                    let mut iteration = self.clone_variables(variables)?;
                    self.bind_iteration(owner, statement.id, name, ty, &mut iteration, depth + 1)?;
                    self.loop_body(
                        owner,
                        body,
                        &mut iteration,
                        result,
                        true,
                        None,
                        true,
                        depth + 1,
                    )?;
                    self.merge_variables(variables, &[iteration])?;
                }
                CoreStatementKind::Loop(body) => {
                    if !self.loop_body(
                        owner,
                        body,
                        variables,
                        result,
                        false,
                        None,
                        true,
                        depth + 1,
                    )? {
                        live = false;
                    }
                }
                CoreStatementKind::Break | CoreStatementKind::Continue => {
                    if self.loops.is_empty() {
                        return Err(bad("original control transfer has no lexical loop target"));
                    }
                    let edge = self.clone_variables(variables)?;
                    self.pay(1, depth)?;
                    let frame = self
                        .loops
                        .last_mut()
                        .ok_or_else(|| bad("original loop control frame disappeared"))?;
                    if matches!(&statement.kind, CoreStatementKind::Break) {
                        frame.breaks.push(edge);
                    } else {
                        frame.continues.push(edge);
                    }
                    live = false;
                }
                CoreStatementKind::With { name, value, body } => {
                    let ty = self.expr(owner, value, variables, None, depth + 1)?;
                    let mut inner = self.clone_variables(variables)?;
                    self.bind_iteration(owner, statement.id, name, ty, &mut inner, depth + 1)?;
                    if self.block(owner, body, &mut inner, result, depth + 1)? {
                        self.merge_variables(variables, &[inner])?;
                    } else {
                        live = false;
                    }
                }
                CoreStatementKind::ForEach {
                    pattern,
                    iterator,
                    body,
                } => {
                    let element = if let CoreExprKind::Range { start, end } = &iterator.kind {
                        self.range_element(owner, start, end, variables, depth + 1)?
                    } else {
                        let ty = self.expr(owner, iterator, variables, None, depth + 1)?;
                        let ty = self.logical(owner, &ty, depth + 1)?;
                        match ty {
                            SemanticTypeRecipe::Array { element, .. } => *element,
                            SemanticTypeRecipe::Address {
                                mutable,
                                lifetime,
                                inner,
                            } => {
                                let (SemanticTypeRecipe::Array { element, .. }
                                | SemanticTypeRecipe::Slice(element)) = *inner
                                else {
                                    return Err(pending(
                                        "original iteration requires its complete iterator protocol scheme",
                                    ));
                                };
                                SemanticTypeRecipe::Address {
                                    mutable,
                                    lifetime,
                                    inner: element,
                                }
                            }
                            _ => {
                                return Err(pending(
                                    "original iteration requires its complete iterator protocol scheme",
                                ));
                            }
                        }
                    };
                    let mut iteration = self.clone_variables(variables)?;
                    self.pattern(
                        owner,
                        statement.id,
                        &mut Vec::new(),
                        pattern,
                        &element,
                        false,
                        &mut iteration,
                        &mut BTreeSet::new(),
                        depth + 1,
                    )?;
                    self.loop_body(
                        owner,
                        body,
                        &mut iteration,
                        result,
                        true,
                        None,
                        true,
                        depth + 1,
                    )?;
                    self.merge_variables(variables, &[iteration])?;
                }
                CoreStatementKind::CompileTime(_)
                | CoreStatementKind::Yield(_)
                | CoreStatementKind::Unsupported(_) => {
                    return Err(pending(
                        "original staged or resumable statement requires its executable stage scheme",
                    ));
                }
            }
            if !was_live {
                // Unreachable syntax still receives a rigid type/where check,
                // but it cannot manufacture a reaching break or continue edge.
                for (frame, (breaks, continues)) in self.loops.iter_mut().zip(exits) {
                    frame.breaks.truncate(breaks);
                    frame.continues.truncate(continues);
                }
                live = false;
            }
        }
        Ok(live)
    }
    fn binding_statement(
        &mut self,
        owner: SemanticDefinitionId,
        statement: &zil_ir::SemanticOriginalStatement,
        variables: &mut BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<()> {
        let CoreStatementKind::Binding {name, mutable, ty, initializer} = &statement.kind else {
            unreachable!();
        };
        let integer = if !*mutable && ty.is_none()
            && let Some(initializer) = initializer
            && let CoreExprKind::Integer(spelling) = &initializer.kind
            && self.contextual_integer(initializer, variables, depth + 1)?
        {
            self.snapshot(owner, SemanticOriginalNode::Expression(initializer.id), variables)?;
            self.pay(spelling.len().saturating_add(self.integer_initializers.len()).saturating_add(1), depth)?;
            self.integer_initializers.entry(owner).or_default().insert(statement.id,
                IntegerInitializer { expression: initializer.id, spelling: spelling.clone(), span: initializer.span });
            Some(SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source: SemanticOriginalTypeSource::Binding(statement.id),
            })
        } else {
            None
        };
        let deferred = integer.is_some();
        let annotation = ty
            .as_ref()
            .map(|ty| self.resolve(owner, ty, depth + 1))
            .transpose()?;
        // A raw resource at an initialized local annotation root
        // constrains only the family. The initializer still proves
        // its owning/description category and its entire life domain.
        let family = match annotation.as_ref() {
            Some(annotation) => self.resource_head(owner, annotation, depth + 1)?,
            None => false,
        };
        let hint = if family {
            match (annotation.as_ref(), initializer.as_ref()) {
                (Some(annotation), Some(initializer))
                    if self.initializer_owns(
                        owner,
                        initializer,
                        variables,
                        depth + 1,
                    )? =>
                {
                    self.pay(1, depth)?;
                    Some(SemanticTypeRecipe::Managed(Box::new(copy_recipe(
                        annotation,
                        self.limits,
                        self.left,
                    )?)))
                }
                _ => None,
            }
        } else {
            annotation
                .as_ref()
                .map(|ty| copy_recipe(ty, self.limits, self.left))
                .transpose()?
        };
        let actual = if deferred {
            integer
        } else if let (Some(value), Some(expected)) = (initializer, hint.as_ref())
        {
            if let Some(view) = self.shared_input_view(
                owner,
                Query::Binding(statement.id),
                value,
                expected,
                variables,
                depth + 1,
            )? {
                Some(view)
            } else {
                Some(self.expr(owner, value, variables, Some(expected), depth + 1)?)
            }
        } else {
            initializer
                .as_ref()
                .map(|value| {
                    self.expr(owner, value, variables, hint.as_ref(), depth + 1)
                })
                .transpose()?
        };
        let value = if family {
            let actual = actual.as_ref().ok_or_else(|| {
                bad("resource-family binding has no initializing category")
            })?;
            let actual = self.observation_type(owner, actual, depth + 1)?;
            let payload = match &actual {
                SemanticTypeRecipe::Managed(inner)
                | SemanticTypeRecipe::Desc { inner, .. } => inner.as_ref(),
                _ => {
                    return Err(bad(
                        "original resource-family initializer has no proved owning or description category",
                    ));
                }
            };
            let annotation = annotation.as_ref().ok_or_else(|| {
                bad("resource-family binding lost its original annotation")
            })?;
            self.same(annotation, payload)?;
            actual
        } else {
            if let (Some(a), Some(b)) = (&annotation, &actual) {
                self.same(a, b)?;
            }
            annotation.or(actual).ok_or_else(|| {
                bad("original uninitialized binding has no declared symbolic type")
            })?
        };
        let value = if *mutable && (ty.is_none() || family) {
            if let Some(initializer) = initializer {
                self.view_bound(owner, initializer, variables, depth + 1)?;
                self.introduce_role(
                    owner,
                    statement.id,
                    initializer,
                    value,
                    variables,
                    depth + 1,
                )?
            } else {
                value
            }
        } else {
            value
        };
        if !deferred {
            let copy = copy_recipe(&value, self.limits, self.left)?;
            self.proof
                .proof_mut()?
                .types
                .insert((owner, Query::Binding(statement.id)), copy);
        }
        if let Some(initializer) = initializer {
            if !*mutable {
                self.record_task_binding(
                    owner,
                    statement.id,
                    initializer,
                    variables,
                    depth + 1,
                )?;
            }
            self.alias_result_origin(
                owner,
                Query::Binding(statement.id),
                &Query::Expression(initializer.id),
                depth + 1,
            )?;
        }
        let resource_root = if matches!(value, SemanticTypeRecipe::Desc { .. }) {
            let alias = resource_writes::Root::Alias(owner, statement.id);
            if let Some(initializer) = initializer {
                let roots = self.resource_place_roots(owner, initializer, variables, depth + 1)?;
                self.record_resource_write(roots, Some(alias), depth + 1)?;
            }
            Some(alias)
        } else {
            None
        };
        let reaching_write=if *mutable && matches!(value,SemanticTypeRecipe::Address{mutable:false,..}) {
            initializer.as_ref().map_or(OriginalSlotWrite::Unknown,|rhs|OriginalSlotWrite::Write(rhs.id))
        }else{OriginalSlotWrite::Unknown};
        self.insert_binding(
            variables,
            name.clone(),
            Binding {
                ty: std::rc::Rc::new(value),
                mutable: *mutable,
                used: false,
                observed: false,
                capture: false,
                borrowed: false, reaching_write,
                origin: Query::Binding(statement.id),
                origin_owner: owner,
                initialized: initializer.is_some(),
                written: false,
                resource_root,
                taken_fields: BTreeSet::new(),
                shadowed: None,
            },
            depth,
        )?;
        Ok(())
    }
    fn return_statement(
        &mut self,
        owner: SemanticDefinitionId,
        statement: &zil_ir::SemanticOriginalStatement,
        value: &SemanticOriginalExpr,
        result: &SemanticTypeRecipe,
        variables: &mut BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<()> {
        let value_node = value;
        let task_family = self.task_return_family(result);
        let value = if task_family {
            self.task_return_value(owner, value, variables, result, depth + 1)?
        } else {
            // The declaration is a typing goal, not its yet-unproved
            // hidden result equation. Resolving the current opaque's
            // Result here would depend on the closure we are about
            // to check and discard its declared parameter goals.
            let target = if matches!(result, SemanticTypeRecipe::Opaque {
                definition, hidden, ..
            } if *definition == owner && matches!(hidden.as_ref(),
                SemanticTypeRecipe::OriginalType {
                    scope: SemanticScopeRef::Ancestor(0),
                    source: SemanticOriginalTypeSource::Result,
                })) {
                Some(copy_recipe(result, self.limits, self.left)?)
            } else if matches!(result, SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source: SemanticOriginalTypeSource::Result,
            }) {
                // An inferred declaration has no result goal until
                // its real returns have supplied their equations.
                None
            } else {
                Some(self.resolve(owner, result, depth + 1)?)
            };
            // A receiver is a boundary upper, never a replacement
            // for the original borrow expression's own sigma.
            let target = target.as_ref().filter(|value| !matches!(value,
                SemanticTypeRecipe::Address { lifetime, .. }
                    if views::is_receiver_upper(lifetime)));
            self.expr(owner, value, variables, target, depth + 1)?
        };
        let capture_observation = if matches!(value, SemanticTypeRecipe::Environment { .. })
            && self.definition(owner)?.kind == SemanticDefinitionKind::Closure
        {
            let observation = self.observation_type(owner, &value, depth + 1)?;
            if matches!(observation, SemanticTypeRecipe::Address { mutable: false, .. }) {
                Some(self.record_expression_type(owner, value_node.id, observation)?)
            } else {
                None
            }
        } else {
            None
        };
        let value = self.receiver_return_value(
            owner, value_node, variables, capture_observation.as_ref().unwrap_or(&value), depth + 1,
        )?.unwrap_or(value);
        self.ensure_escape(owner, &value, depth + 1)?;
        let infer = matches!(
            result,
            SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source: SemanticOriginalTypeSource::Result
            }
        ) || matches!(result, SemanticTypeRecipe::Opaque { hidden, .. } if matches!(hidden.as_ref(), SemanticTypeRecipe::OriginalType { scope: SemanticScopeRef::Ancestor(0), source: SemanticOriginalTypeSource::Result }));
        if infer {
            if let Some(old) = self.proof.types.get(&(owner, Query::Result)) {
                let old = copy_recipe(old, self.limits, self.left)?;
                self.same(&old, &value)?;
            } else {
                let copy = copy_recipe(&value, self.limits, self.left)?;
                self.proof
                    .proof_mut()?
                    .types
                    .insert((owner, Query::Result), copy);
            }
        }
        if infer && matches!(result, SemanticTypeRecipe::Opaque { definition, .. } if *definition == owner)
            && matches!(value, SemanticTypeRecipe::Opaque { .. })
        {
            self.pay(self.proof.opaque_forwarded_returns.len().saturating_add(1).ilog2() as usize + 1, depth)?;
            let count = self.proof.opaque_forwarded_returns.get(&owner).map_or(0, BTreeSet::len);
            self.pay(count.saturating_add(1).ilog2() as usize + 2, depth)?;
            self.proof.proof_mut()?.opaque_forwarded_returns.entry(owner).or_default().insert(value_node.id);
        }
        let expected = self.resolve(owner, result, depth + 1)?;
        if let SemanticTypeRecipe::Opaque {
            definition, hidden, ..
        } = &expected
        {
            if *definition != owner {
                return Err(bad(
                    "original return constructs another definition's opaque result",
                ));
            }
            self.same(&value, hidden)?;
        } else if self.returned_slot_write(owner,value_node,variables,&value,&expected,depth+1)? {
            // The selected read has its own reaching writer. The declaration
            // retains all of its sources for other uses and incoming paths.
        } else if self.inferred_return_head(owner, &expected)? {
            self.return_edge(owner, value_node, variables, &value, &expected, depth + 1)?;
        } else if !task_family
            && !self.return_view(
                owner,
                value_node,
                variables,
                &value,
                &expected,
                depth + 1,
            )?
        {
            self.same(&value, &expected)?;
        }
        self.record_task_return(
            owner,
            statement.id,
            value_node,
            &value,
            variables,
            depth + 1,
        )?;
        self.capture_exit(owner, variables, depth + 1)?;
        Ok(())
    }
    fn assignment_statement(
        &mut self,
        owner: SemanticDefinitionId,
        statement: &zil_ir::SemanticOriginalStatement,
        target: &SemanticOriginalExpr,
        value: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<()> {
        self.record_task_place_source(owner, target, variables, depth + 1)?;
        let mut expected = self.writable(owner, target, variables, depth + 1)?;
        if matches!(&statement.kind, CoreStatementKind::CompoundAssign { .. }) {
            let observed = self.place(owner, target, variables, depth + 1)?;
            self.same(&expected, &observed)?;
            expected = self.observation_type(owner, &expected, depth + 1)?;
            let SemanticTypeRecipe::Atom(primitive) = &expected else {
                return Err(bad(
                    "original compound assignment lacks a primitive endpoint premise",
                ));
            };
            let CoreStatementKind::CompoundAssign { op, .. } = &statement.kind else {
                unreachable!()
            };
            if !primitive.is_integer() && !primitive.is_float()
                || matches!(
                    op,
                    BinaryOp::Equal
                        | BinaryOp::NotEqual
                        | BinaryOp::Less
                        | BinaryOp::LessEqual
                        | BinaryOp::Greater
                        | BinaryOp::GreaterEqual
                        | BinaryOp::And
                        | BinaryOp::Or
                )
                || primitive.is_float()
                    && matches!(
                        op,
                        BinaryOp::BitAnd
                            | BinaryOp::BitOr
                            | BinaryOp::BitXor
                            | BinaryOp::ShiftLeft
                            | BinaryOp::ShiftRight
                            | BinaryOp::AddWrapping
                            | BinaryOp::SubtractWrapping
                            | BinaryOp::MultiplyWrapping
                    )
            {
                return Err(bad(
                    "original compound assignment has an incompatible operator",
                ));
            }
        }
        let role = if let CoreExprKind::Name(name) = &target.kind {
            let binding = variables.get(name).ok_or_else(|| bad("original assignment target disappeared"))?;
            if let Query::Binding(statement) = binding.origin {
                let key = (binding.origin_owner, statement);
                self.pay(self.proof.local_roles.len().saturating_add(1).ilog2() as usize + 1, depth)?;
                self.proof.local_roles.contains_key(&key).then_some(key)
            } else { None }
        } else { None };
        // A replacement borrow contributes its own actual source
        // to an inferred local role. Checking it against the role's
        // previous sources would reject a different live referent.
        let direct_role_borrow = if role.is_some() {
            let mut source = value;
            while let CoreExprKind::Group(inner) = &source.kind {
                self.pay(1, depth)?;
                source = inner;
            }
            matches!(source.kind, CoreExprKind::Unary { op: UnaryOp::Borrow | UnaryOp::BorrowMut, .. })
        } else { false };
        let context = (!direct_role_borrow).then_some(&expected);
        let actual = self.expr(owner, value, variables, context, depth + 1)?;
        if let Some((role_owner, statement)) = role {
            self.view_bound(owner, value, variables, depth + 1)?;
            self.assign_role(owner, role_owner, statement, value, &actual, variables, depth + 1)?;
        } else {
            self.same(&actual, &expected)?;
        }
        if let CoreExprKind::Name(name) = &target.kind {
            if let Some(alias @ resource_writes::Root::Alias(..)) =
                variables.get(name).and_then(|binding| binding.resource_root)
            {
                let roots = self.resource_place_roots(owner, value, variables, depth + 1)?;
                self.record_resource_write(roots, Some(alias), depth + 1)?;
            }
            let target = variables
                .get_mut(name)
                .ok_or_else(|| bad("original assignment target disappeared"))?;
            target.used = false;
            target.initialized = true;
            target.written = true;
            if matches!(target.ty.as_ref(),SemanticTypeRecipe::Address{mutable:false,..}) {target.reaching_write.assign(value.id);}
            target.taken_fields.clear();
        } else if let CoreExprKind::Member { object, member, .. } = &target.kind {
            if let CoreExprKind::Name(name) = &object.kind {
                self.pay(name.len().saturating_add(member.len()), depth)?;
                if let Some(binding) = variables.get_mut(name) {
                    binding.taken_fields.remove(member);
                }
            }
        }
        Ok(())
    }
    fn range_element(
        &mut self,
        owner: SemanticDefinitionId,
        start: &SemanticOriginalExpr,
        end: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(1, depth)?;
        // As for arithmetic, an unsuffixed literal has no effects and can
        // receive the independently typed other endpoint's integer context.
        let (a, b) = if self.contextual_integer(start, variables, depth + 1)?
            && !self.contextual_integer(end, variables, depth + 1)?
        {
            let b = self.expr(owner, end, variables, None, depth + 1)?;
            let wanted = self.logical(owner, &b, depth + 1)?;
            (
                self.expr(owner, start, variables, Some(&wanted), depth + 1)?,
                b,
            )
        } else {
            let a = self.expr(owner, start, variables, None, depth + 1)?;
            let wanted = self.logical(owner, &a, depth + 1)?;
            (
                a,
                self.expr(owner, end, variables, Some(&wanted), depth + 1)?,
            )
        };
        let a = self.logical(owner, &a, depth + 1)?;
        let b = self.logical(owner, &b, depth + 1)?;
        self.same(&a, &b)?;
        if !matches!(&a, SemanticTypeRecipe::Atom(ty) if ty.is_integer()) {
            return Err(bad(
                "original range loop endpoints must be primitive integers",
            ));
        }
        Ok(a)
    }
    fn bind_iteration(
        &mut self,
        owner: SemanticDefinitionId,
        statement: NodeId,
        name: &str,
        ty: SemanticTypeRecipe,
        variables: &mut BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<()> {
        self.pay(name.len(), depth)?;
        let origin = Query::Binding(statement);
        let copy = copy_recipe(&ty, self.limits, self.left)?;
        self.proof
            .proof_mut()?
            .types
            .insert((owner, origin.clone()), copy);
        self.insert_binding(
            variables,
            name.to_owned(),
            Binding {
                ty: std::rc::Rc::new(ty),
                mutable: false,
                used: false,
                observed: false,
                capture: false,
                borrowed: false, reaching_write: OriginalSlotWrite::Unknown,
                origin,
                origin_owner: owner,
                initialized: true,
                written: false,
                resource_root: None,
                taken_fields: BTreeSet::new(),
                shadowed: None,
            },
            depth,
        )?;
        Ok(())
    }
    fn body_result_recipe(&mut self, owner: SemanticDefinitionId, declared: &SemanticTypeRecipe)
        -> R<SemanticTypeRecipe>
    {
        let goal = self.closure_result_goal.as_ref()
            .filter(|(child, _)| *child == owner)
            .map_or(declared, |(_, ty)| ty);
        copy_recipe(goal, self.limits, self.left)
    }
    fn function(
        &mut self,
        definition: &'a SemanticDefinition,
        parent: Option<BTreeMap<String, Binding>>,
    ) -> R<()> {
        let tracing = matches!(self.proof, ProofAccess::Owned(_));
        if tracing {
            self.pay(
                self.proof.body_costs.len().saturating_add(1).ilog2() as usize
                    + std::mem::size_of::<(SemanticDefinitionId, (usize, usize, usize))>(),
                0,
            )?;
        }
        let before = *self.left;
        let children = self.proof.body_cost_total;
        let context_depth = self.contexts.len();
        let outer_stage = self.stage_depth;
        let outer_cleanup = self.cleanup_depth;
        self.pay(self.active.len().saturating_add(1).ilog2() as usize + 1, 0)?;
        let was_active = self.active.contains(&definition.id);
        let result = self.function_inner(definition, parent);
        self.stage_depth = outer_stage;
        self.cleanup_depth = outer_cleanup;
        if result.is_err() {
            self.contexts.truncate(context_depth);
            if !was_active {
                self.active.remove(&definition.id);
            }
        }
        let inclusive = before.saturating_sub(*self.left);
        if let ProofAccess::Owned(proof) = &mut self.proof {
            let exclusive =
                inclusive.saturating_sub(proof.body_cost_total.saturating_sub(children));
            proof.body_cost_total = proof.body_cost_total.saturating_add(exclusive);
            let row = proof.body_costs.entry(definition.id).or_default();
            row.0 = row.0.saturating_add(1);
            row.1 = row.1.saturating_add(inclusive);
            row.2 = row.2.saturating_add(exclusive);
        }
        result
    }
    fn function_inner(
        &mut self,
        definition: &'a SemanticDefinition,
        parent: Option<BTreeMap<String, Binding>>,
    ) -> R<()> {
        if self.proof.complete.contains(&definition.id) {
            return Ok(());
        }
        if matches!(self.proof, ProofAccess::Borrowed(_)) {
            return Err(pending(
                "read-only original where proof cannot introduce an unchecked body fact",
            ));
        }
        self.pay(self.active.len().saturating_add(1), 0)?;
        if !self.active.insert(definition.id) {
            return Err(pending(
                "recursive original inference needs a complete declared signature",
            ));
        }
        self.pay(1, self.contexts.len())?;
        self.contexts.push(definition.id);
        let provider = self.provider_conformance(definition, 0)?;
        let SemanticDefinitionBody::FunctionWithBody {
            signature,
            parameters,
            body,
            ..
        } = &definition.body
        else {
            return Err(pending(
                "executable original definition omits its complete immutable operation body",
            ));
        };
        let mut variables = parent.unwrap_or_default();
        if parameters.len() != signature.parameters.len() {
            return Err(bad(
                "original parameter identity table differs from signature",
            ));
        }
        let mut deferred_parameters = Vec::new();
        for (index, (binder, parameter)) in parameters.iter().zip(&signature.parameters).enumerate()
        {
            let query = Query::Parameter(index as u32);
            let unannotated_closure_input = definition.kind == SemanticDefinitionKind::Closure
                && binder.ty.is_none()
                && parameter.mode == PassingMode::Infer
                && matches!(&parameter.ty, SemanticTypeRecipe::OriginalType {
                    scope: SemanticScopeRef::Ancestor(0),
                    source: SemanticOriginalTypeSource::Parameter(ordinal),
                } if *ordinal as usize == index);
            let deferred = if unannotated_closure_input {
                self.pay(self.proof.types.len().saturating_add(1).ilog2() as usize + 2, 0)?;
                if !self.proof.types.contains_key(&(definition.id, query.clone())) {
                    if let Some(ty) = self.contextual_code_goals.get(&definition.id).and_then(|row| row.get(&code_context::Position::Parameter(index as u32))) {
                        let ty = copy_recipe(ty, self.limits, self.left)?;
                        self.contextual_code_inputs.entry(definition.id).or_default().insert(index as u32);
                        self.proof.proof_mut()?.types.insert((definition.id, query.clone()), ty);
                    }
                }
                !self.proof.types.contains_key(&(definition.id, query.clone()))
            } else {
                false
            };
            let ty = if deferred {
                deferred_parameters.push(query.clone());
                copy_recipe(&parameter.ty, self.limits, self.left)?
            } else {
                self.resolve(definition.id, &parameter.ty, 0)?
            };
            let ty = self.mode_type(definition.id, parameter.mode, ty, 0)?;
            if let Some(annotation) = &binder.ty {
                let annotation = self.resolve(definition.id, annotation, 0)?;
                let annotation = self.mode_type(definition.id, binder.mode, annotation, 0)?;
                self.same(&ty, &annotation)?;
            }
            if !deferred {
                let copy = copy_recipe(&ty, self.limits, self.left)?;
                self.proof
                    .proof_mut()?
                    .types
                    .insert((definition.id, query.clone()), copy);
            }
            let resource_root = matches!(ty, SemanticTypeRecipe::Desc { .. })
                .then_some(resource_writes::Root::Parameter(definition.id, index as u32));
            variables.insert(
                binder.name.clone(),
                Binding {
                    ty: std::rc::Rc::new(ty),
                    mutable: false,
                    used: false,
                    observed: false,
                    capture: false,
                    borrowed: false, reaching_write: OriginalSlotWrite::Unknown,
                    origin: query,
                    origin_owner: definition.id,
                    initialized: true,
                    written: false,
                    resource_root,
                    taken_fields: BTreeSet::new(),
                    shadowed: None,
                },
            );
        }
        let result = self.body_result_recipe(definition.id, &signature.result)?;
        let result = self.mode_type(definition.id, signature.result_mode, result, 0)?;
        let outer_loops = std::mem::take(&mut self.loops);
        let outer_stage = std::mem::replace(&mut self.stage_depth, 0);
        let cleanup = !self.proof.cleanup_actions.is_empty() && {
            self.pay(self.proof.cleanup_actions.len().saturating_add(1).ilog2() as usize + 1, 0)?;
            self.proof.cleanup_actions.contains(&definition.id)
        };
        let outer_cleanup = std::mem::replace(&mut self.cleanup_depth, usize::from(cleanup));
        let body_result: R<()> = (|| {
            match body {
                SemanticOriginalFunctionBody::Defined(body) => {
                    if self.block(definition.id, body, &mut variables, &result, 0)? {
                        self.unit_result(definition.id, &result, 0)?;
                        self.capture_exit(definition.id, &variables, 0)?;
                    }
                }
                SemanticOriginalFunctionBody::External => {}
                SemanticOriginalFunctionBody::Declaration => {
                    let parent = definition
                        .parent
                        .ok_or_else(|| bad("abstract original member has no real trait parent"))?;
                    if definition.kind != SemanticDefinitionKind::Method
                        || !matches!(
                            self.definition(parent)?.body,
                            SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { .. })
                        )
                    {
                        return Err(bad(
                            "only a real abstract trait member may omit its operation body",
                        ));
                    }
                }
            }
            Ok(())
        })();
        self.loops = outer_loops;
        self.stage_depth = outer_stage;
        self.cleanup_depth = outer_cleanup;
        body_result?;
        if let Some(initializers) = self.integer_initializers.get(&definition.id) {
            let count = initializers.len();
            self.pay(count, 0)?;
            let binders: Vec<_> = self.integer_initializers[&definition.id].keys().copied().collect();
            for binder in binders {
                self.infer_integer_binding(definition.id, binder, None, 0)?;
            }
            self.integer_initializers.remove(&definition.id);
        }
        for query in deferred_parameters {
            self.pay(self.proof.types.len().saturating_add(1).ilog2() as usize + 1, 0)?;
            if !self.proof.types.contains_key(&(definition.id, query)) {
                return Err(pending("original closure parameter has no complete source type context"));
            }
        }
        self.pay(variables.len(), 0)?;
        let names_work = self
            .proof
            .capture_names
            .get(&definition.id)
            .map_or(0, |names| {
                names
                    .iter()
                    .fold(names.len(), |n, name| n.saturating_add(name.len()))
            });
        self.pay(names_work, 0)?;
        let mut names = self
            .proof
            .capture_names
            .get(&definition.id)
            .cloned()
            .unwrap_or_default();
        let mut kind = self
            .proof
            .kinds
            .get(&definition.id)
            .copied()
            .unwrap_or(CallableKind::Fn);
        if definition.kind == SemanticDefinitionKind::Closure
            && kind == CallableKind::Fn
            && self.resource_receiver_written(definition.id, 0)?
        {
            kind = CallableKind::FnMut;
        }
        for (name, visible) in &variables {
            let mut current = Some(visible);
            let mut captured = false;
            while let Some(value) = current {
                self.pay(1, 0)?;
                if value.capture && value.observed {
                    captured = true;
                    if value.written && kind != CallableKind::FnOnce {
                        kind = CallableKind::FnMut;
                    }
                }
                current = value.shadowed.as_deref();
            }
            if captured {
                self.pay(name.len().saturating_add(names.len()), 0)?;
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
        }
        self.pay(
            names
                .iter()
                .fold(names.len(), |n, name| n.saturating_add(name.len()))
                .saturating_mul(names.len().saturating_add(1).ilog2() as usize + 1),
            0,
        )?;
        names.sort_unstable();
        self.proof.proof_mut()?.kinds.insert(definition.id, kind);
        if definition.kind == SemanticDefinitionKind::Closure && kind == CallableKind::Fn {
            self.finish_shared_capture_observations(definition.id)?;
        }
        self.pay(names.len(), 0)?;
        for name in &names {
            self.pay(name.len(), 0)?;
        }
        self.proof.proof_mut()?.capture_names.insert(definition.id, names);
        self.finish_task_result(definition.id, 0)?;
        if !self
            .proof
            .types
            .contains_key(&(definition.id, Query::Result))
        {
            let result = self.resolve(definition.id, &result, 0)?;
            self.proof
                .proof_mut()?
                .types
                .insert((definition.id, Query::Result), result);
        }
        if (kind == CallableKind::FnOnce || signature.call_kind != CallKind::Direct)
            && matches!(self.proof.types.get(&(definition.id, Query::Result)),
                Some(SemanticTypeRecipe::Address { lifetime, .. }) if views::is_receiver_upper(lifetime))
        {
            return Err(bad("consuming closure cannot return a loan of its own environment"));
        }
        self.result_callable_requirements(definition.id, &definition.requirements, &mut Vec::new(), 0)?;
        self.recheck_escaping_roles(definition.id, 0)?;
        if let Some(declaration) = provider {
            self.proof
                .proof_mut()?
                .providers
                .insert(definition.id, declaration);
        }
        self.active.remove(&definition.id);
        if self.contexts.pop() != Some(definition.id) {
            return Err(bad("original lexical equality context stack changed"));
        }
        self.proof.proof_mut()?.complete.insert(definition.id);
        Ok(())
    }
}
pub(super) fn check(program: &SemanticModule, limits: &Limits, left: &mut usize) -> R<Proof> {
    check_requested(program, std::rc::Rc::default(), None, Default::default(), limits, left)
        .map_err(code_context::Failure::into_diagnostic)
}
pub(super) fn check_with_declarations(
    program: &SemanticModule,
    declarations: std::rc::Rc<Vec<super::original_context::Declaration>>,
    goals: code_context::Goals,
    limits: &Limits,
    left: &mut usize,
) -> Result<Proof, code_context::Failure> {
    check_requested(program, declarations, None, goals, limits, left)
}
pub(super) fn check_at(
    program: &SemanticModule,
    owner: SemanticDefinitionId,
    node: SemanticOriginalNode,
    limits: &Limits,
    left: &mut usize,
) -> R<Proof> {
    check_requested(
        program,
        std::rc::Rc::default(),
        Some((owner, node)),
        Default::default(),
        limits,
        left,
    ).map_err(code_context::Failure::into_diagnostic)
}
pub(super) fn check_at_with_declarations(
    program: &SemanticModule,
    declarations: std::rc::Rc<Vec<super::original_context::Declaration>>,
    owner: SemanticDefinitionId,
    node: SemanticOriginalNode,
    goals: code_context::Goals,
    limits: &Limits,
    left: &mut usize,
) -> Result<Proof, code_context::Failure> {
    check_requested(program, declarations, Some((owner, node)), goals, limits, left)
}
fn definition_positions(
    program: &SemanticModule,
    limits: &Limits,
    left: &mut usize,
) -> R<std::rc::Rc<BTreeMap<SemanticDefinitionId, usize>>> {
    *left = left
        .checked_sub(program.definitions.len().saturating_add(1))
        .ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "original definition index work exceeded",
                None,
            )
        })?;
    if program.definitions.len() > limits.max_nodes {
        return Err(Diagnostic::limit(
            Phase::Types,
            "original definition index count exceeded",
            None,
        ));
    }
    let mut index = BTreeMap::new();
    for (position, definition) in program.definitions.iter().enumerate() {
        *left = left
            .checked_sub(index.len().saturating_add(1).ilog2() as usize + 2)
            .ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "original definition index work exceeded",
                    None,
                )
            })?;
        if index.insert(definition.id, position).is_some() {
            return Err(bad("original definition index has duplicate identities"));
        }
    }
    Ok(std::rc::Rc::new(index))
}
impl Proof {
    /// Rebind a forwarded lexical input to the nested constructor in its own
    /// declaration. The creator ledger independently selects physical inputs.
    pub(super) fn forwarded_name<'d>(&self, declaration: &'d SemanticDefinition,
        occurrence: &SemanticImplicitLifetime, left: &mut usize) -> R<Option<&'d str>>
    {
        let Some((node, field)) = occurrence.forwarded_input() else { return Ok(None); };
        let (_, name) = declaration.environment.get(field as usize)
            .and_then(|formal| formal.source.lexical_input())
            .ok_or_else(|| bad("forwarded capture lacks its declared environment input"))?;
        let SemanticDefinitionBody::FunctionWithBody { sites, .. } = &declaration.body else {
            return Err(bad("forwarded capture has no original declaration body"));
        };
        let directory = self.original_sites.owner(declaration.id, left)?
            .ok_or_else(|| bad("forwarded capture has no original site directory"))?;
        let mut found = false;
        for &site in directory.at(SemanticOriginalNode::Expression(node), left)? {
            let Some(recipe) = directory.recipe(declaration.id, sites, site, left)? else { continue; };
            if let SemanticSiteRecipeKind::Closure { environment, .. } = &recipe.kind {
                for argument in environment {
                    let input = argument.lexical_input();
                    let cost = input.map_or(1, |(_, actual)| actual.len().saturating_add(name.len()).saturating_add(1));
                    *left = left.checked_sub(cost).ok_or_else(|| Diagnostic::limit(Phase::Types,"capture input lookup work exceeded",None))?;
                    if input == Some((node, name)) {
                        if found { return Err(bad("nested constructor repeats its forwarded input")); }
                        found = true;
                    }
                }
            }
        }
        if !found { return Err(bad("forwarded capture selects no actual nested constructor input")); }
        Ok(Some(name))
    }
}
impl Proof {
    pub(super) fn original_site_node(
        &self,
        owner: SemanticDefinitionId,
        recipes: &[SemanticSiteRecipe],
        rows: &[SemanticOriginalSiteNode],
        id: SemanticSiteId,
        left: &mut usize,
    ) -> R<Option<Option<SemanticOriginalNode>>> {
        match self.original_sites.owner(owner, left)? {
            Some(directory) => directory.node(id, left),
            None => sites::DefinitionSites::build(owner, recipes, rows, left)?.node(id, left),
        }
    }
}

fn check_requested(
    program: &SemanticModule,
    declarations: std::rc::Rc<Vec<super::original_context::Declaration>>,
    requested_snapshot: Option<(SemanticDefinitionId, SemanticOriginalNode)>,
    mut contextual_code_goals: code_context::Goals,
    limits: &Limits,
    left: &mut usize,
) -> Result<Proof, code_context::Failure> {
    let definition_positions = definition_positions(program, limits, left)?;
    let original_sites = sites::Directory::build(program, limits, left)?;
    let mut required_resource_writes = BTreeSet::new();
    loop {
        let mut check = Check {
            program,
            limits,
            left: &mut *left,
            proof: ProofAccess::Owned(Proof {
                definition_positions: definition_positions.clone(),
                original_sites: original_sites.clone(),
                declarations: declarations.clone(),
                resource_writes: resource_writes::Graph::seeded(required_resource_writes),
                ..Proof::default()
            }),
            active: BTreeSet::new(),
            capture_writes: BTreeMap::new(),
            loops: Vec::new(),
            trait_active: Vec::new(),
            contexts: Vec::new(),
            closure_result_goal: None,
            integer_initializers: BTreeMap::new(),
            contextual_code_goals,
            contextual_code_need: None,
            contextual_code_inputs: BTreeMap::new(),
            contextual_code_calls: BTreeSet::new(),
            predicate_active: Vec::new(),
            stage_depth: 0,
            cleanup_depth: 0,
            requested_snapshot,
            snapshot_sources: None,
            escape_active: Vec::new(),
        };
        match check_complete(&mut check) {
            Ok(required) if required.is_empty() => {
                return check.proof.into_owned().map_err(code_context::Failure::Permanent);
            }
            Ok(required) => {
                required_resource_writes = required;
                contextual_code_goals = check.contextual_code_goals;
            }
            Err(diagnostic) => {
                if let Some(demand) = check.contextual_code_need
                    && (diagnostic.code == code_context::MISSING || diagnostic.code == "E-INTEGER-RANGE"
                        && demand.position == code_context::Position::IntegerResult)
                {
                    return Err(code_context::Failure::Missing { demand, diagnostic });
                }
                return Err(code_context::Failure::Permanent(diagnostic));
            }
        }
    }
}

fn callee_order(callers: &[Vec<usize>], mut remaining: Vec<usize>) -> Vec<usize> {
    let count = remaining.len();
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
    order
}

/// Use original name bindings and direct sites as scheduling hints. Each
/// call still resolves its target and checks the actuals in its lexical scope.
fn original_body_order(check: &mut Check<'_, '_>) -> R<Vec<usize>> {
    let definitions = &check.program.definitions;
    let count = definitions.len();
    check.pay(count.saturating_mul(6), 0)?;
    let mut callers = vec![Vec::new(); count];
    let mut remaining = vec![0; count];
    for (caller, definition) in definitions.iter().enumerate() {
        let SemanticDefinitionBody::FunctionWithBody { names, sites, .. } = &definition.body else {
            continue;
        };
        if definition.kind == SemanticDefinitionKind::Closure {
            continue;
        }
        check.pay(names.len().saturating_add(sites.len()), 0)?;
        let names = names.iter().filter_map(|name| match name.value {
            SemanticOriginalNameValue::Definition(target)
                if name.namespace == SemanticOriginalNamespace::Value =>
            {
                Some(target)
            }
            _ => None,
        });
        let sites = sites.iter().filter_map(|site| match site.kind {
            SemanticSiteRecipeKind::Call { target, .. } => Some(target),
            _ => None,
        });
        for target in names.chain(sites) {
            check.pay(count.saturating_add(1).ilog2() as usize + 5, 0)?;
            let Some(&callee) = check.proof.definition_positions.get(&target) else {
                continue;
            };
            if caller != callee && definitions[callee].kind != SemanticDefinitionKind::Closure {
                callers[callee].push(caller);
                remaining[caller] += 1;
            }
        }
    }
    Ok(callee_order(&callers, remaining))
}

fn check_complete(check: &mut Check<'_, '_>) -> R<BTreeSet<SemanticDefinitionId>> {
    let program = check.program;
    check.pay(1024, 0)?;
    let phase_start = *check.left;
    check.associated_inventory(0)?;
    check.proof.proof_mut()?.stage_costs.insert(
        "associated_inventory",
        phase_start.saturating_sub(*check.left),
    );
    let phase_start = *check.left;
    for index in original_body_order(check)? {
        let definition = &program.definitions[index];
        if matches!(definition.body, SemanticDefinitionBody::Nominal { .. }) {
            check.canonical_carrier(definition, 0)?;
        }
        if matches!(
            definition.body,
            SemanticDefinitionBody::FunctionWithBody { .. }
                | SemanticDefinitionBody::Function { .. }
        ) {
            if definition.kind != SemanticDefinitionKind::Closure {
                check.function(definition, None)?;
            }
        }
    }
    for definition in &program.definitions {
        if matches!(
            definition.body,
            SemanticDefinitionBody::FunctionWithBody { .. }
                | SemanticDefinitionBody::Function { .. }
        ) && !check.proof.complete.contains(&definition.id)
        {
            return Err(bad(
                "original code definition has no checked lexical creation context",
            ));
        }
    }
    check.pay(check.proof.integer_return_obligations.len(), 0)?;
    if check.proof.integer_return_obligations.values().any(|checked| !*checked) {
        return Err(bad("contextual integer closure result lacks an actual matching callable result constraint"));
    }
    for child in check.contextual_code_inputs.keys() {
        if !check.contextual_code_calls.contains(child) {
            return Err(pending("original callable input requires a checked call of its lexical closure"));
        }
    }
    check.proof.proof_mut()?.stage_costs.insert(
        "all_functions_and_carriers",
        phase_start.saturating_sub(*check.left),
    );
    let required_resource_writes = check.settle_resource_receivers()?;
    if !required_resource_writes.is_empty() {
        return Ok(required_resource_writes);
    }
    let phase_start = *check.left;
    check.finish_effects()?;
    check
        .proof
        .proof_mut()?
        .stage_costs
        .insert("finish_effects", phase_start.saturating_sub(*check.left));
    let phase_start = *check.left;
    check.associated_obligations(0)?;
    check.proof.proof_mut()?.stage_costs.insert(
        "associated_obligations",
        phase_start.saturating_sub(*check.left),
    );
    let phase_start = *check.left;
    check.cache_closed_opaque_domains()?;
    check.proof.proof_mut()?.stage_costs.insert(
        "closed_opaque_finalize",
        phase_start.saturating_sub(*check.left),
    );
    let phase_start = *check.left;
    check.proof.premises.enable(check.left)?;
    protocol::check(program, check.proof.proof_mut()?, check.limits, check.left)?;
    check
        .proof
        .proof_mut()?
        .stage_costs
        .insert("protocol_cleanup", phase_start.saturating_sub(*check.left));
    check.proof.proof_mut()?.pure_queries_ready = true;
    Ok(BTreeSet::new())
}

impl Proof {
    /// Read the immutable raw operand of this exact original Call. This source
    /// selector grants no type, class, permission or actual prepared address.
    pub(super) fn call_callee(
        &self, owner: SemanticDefinitionId, call: SemanticSiteId, left: &mut usize,
    ) -> R<Option<NodeId>> {
        *left = left.checked_sub(self.call_callees.len().saturating_add(1).ilog2() as usize + 1)
            .ok_or_else(|| Diagnostic::limit(Phase::Types, "original call operand lookup exceeded", None))?;
        Ok(self.call_callees.get(&(owner, call)).copied())
    }
    /// The producer of an implicit ErrorUnion injection constructs its payload;
    /// later observations of the same expression retain the complete union.
    /// This accessor is used only after matching the actual construction receipt
    /// to this exact original expression, never to select an arm from a hint.
    pub(super) fn constructor_type_recipe(
        &self,
        program: &SemanticModule,
        current: SemanticDefinitionId,
        owner: SemanticDefinitionId,
        source: &SemanticOriginalTypeSource,
        limits: &Limits,
        left: &mut usize,
    ) -> R<std::borrow::Cow<'_, SemanticTypeRecipe>> {
        let ordinary = self.type_recipe_view(program, current, owner, source, limits, left)?;
        let SemanticOriginalTypeSource::Expression(node) = source else {
            return Ok(ordinary);
        };
        let Some(injection) = self.injections.get(&(owner, *node)) else {
            return Ok(ordinary);
        };
        let whole = self
            .types
            .get(&(owner, Query::Expression(*node)))
            .ok_or_else(|| bad("original constructor injection loses its expression equation"))?;
        let SemanticTypeRecipe::ErrorUnion { error, result } = whole else {
            return Err(bad(
                "original constructor injection has no complete ErrorUnion theorem",
            ));
        };
        let (selected, other) = if injection.is_error {
            (error.as_ref(), result.as_ref())
        } else {
            (result.as_ref(), error.as_ref())
        };
        crate::dyn_metadata::semantic_operand_snapshot(whole, limits, left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(&injection.payload, limits, left, 1)?;
        if selected != &injection.payload || other == &injection.payload {
            return Err(bad(
                "original constructor injection changes its uniquely proved payload arm",
            ));
        }
        if owner != current && !matches!(injection.payload, SemanticTypeRecipe::Atom(_)) {
            // A captured whole union is not a capture of its payload. Reusing
            // its Environment slot here would silently change that slot's type.
            return Err(pending(
                "original constructor payload crosses an unproved lexical environment boundary",
            ));
        }
        Ok(std::borrow::Cow::Borrowed(&injection.payload))
    }
    pub(super) fn type_recipe(
        &self,
        program: &SemanticModule,
        current: SemanticDefinitionId,
        owner: SemanticDefinitionId,
        source: &SemanticOriginalTypeSource,
        limits: &Limits,
        left: &mut usize,
    ) -> R<SemanticTypeRecipe> {
        let value = self.type_recipe_view(program, current, owner, source, limits, left)?;
        match value {
            std::borrow::Cow::Borrowed(value) => {
                crate::dyn_metadata::semantic_operand_snapshot(value, limits, left, 1)?;
                Ok(value.clone())
            }
            std::borrow::Cow::Owned(value) => Ok(value),
        }
    }
    /// Borrow only the immutable completed original equation. A distinct use
    /// owner still requires the exact checked lexical environment transport.
    pub(super) fn type_recipe_view(
        &self,
        program: &SemanticModule,
        current: SemanticDefinitionId,
        owner: SemanticDefinitionId,
        source: &SemanticOriginalTypeSource,
        limits: &Limits,
        left: &mut usize,
    ) -> R<std::borrow::Cow<'_, SemanticTypeRecipe>> {
        crate::dyn_metadata::semantic_operand_snapshot(source, limits, left, 1)?;
        let query = Query::from(source.clone());
        if !self.complete.contains(&owner) {
            return Err(bad(
                "original query owner has no complete symbolic body proof",
            ));
        }
        let value = self.types.get(&(owner, query)).ok_or_else(|| {
            let mut diagnostic = bad("original type query has no derived symbolic solution");
            if let Some(rest) = left.checked_sub(256) {
                *left = rest;
                let (kind, node) = match source {
                    SemanticOriginalTypeSource::Expression(node) => ("Expression", node.0),
                    SemanticOriginalTypeSource::Binding(node) => ("Binding", node.0),
                    SemanticOriginalTypeSource::Parameter(ordinal) => ("Parameter", *ordinal),
                    SemanticOriginalTypeSource::Result => ("Result", 0),
                    SemanticOriginalTypeSource::CallConstCarrier { call, .. } => {
                        ("CallConstCarrier", call.0)
                    }
                    SemanticOriginalTypeSource::BindingProjection { statement, .. } => {
                        ("BindingProjection", statement.0)
                    }
                    SemanticOriginalTypeSource::NameAt { at, .. } => match at {
                        SemanticOriginalNode::Expression(node) => ("NameAtExpression", node.0),
                        SemanticOriginalNode::Statement(node) => ("NameAtStatement", node.0),
                    },
                };
                diagnostic.notes.push(format!(
                    "original query: current {:?}, owner {:?}, {} {}",
                    current, owner, kind, node
                ));
            }
            diagnostic
        })?;
        if owner != current {
            *left = left.checked_sub(program.definitions.len()).ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "original query rehoming work exceeded", None)
            })?;
            let definition = program
                .definitions
                .iter()
                .find(|d| d.id == current)
                .ok_or_else(|| bad("original query current definition is absent"))?;
            if definition.parent != Some(owner) {
                return Err(pending(
                    "original query crosses unproved lexical environment boundaries",
                ));
            }
            for (slot, formal) in definition.environment.iter().enumerate() {
                if matches!(&formal.source, SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType { scope: SemanticScopeRef::Ancestor(0), source: expected }) if expected == source)
                {
                    return Ok(std::borrow::Cow::Owned(SemanticTypeRecipe::Environment {
                        scope: SemanticScopeRef::Ancestor(0),
                        ordinal: slot as u32,
                    }));
                }
            }
            if !matches!(value, SemanticTypeRecipe::Atom(_)) {
                return Err(pending(
                    "original parent type requires complete checked environment sealing",
                ));
            }
        }
        if owner == current
            && let SemanticOriginalTypeSource::Expression(node) = source
            && let Some(projected) = self.projected_type_actuals.get(&(owner, *node))
        {
            return Ok(std::borrow::Cow::Borrowed(projected));
        }
        Ok(std::borrow::Cow::Borrowed(value))
    }
    pub(super) fn fixed_call_kind(
        &self,
        program: &SemanticModule,
        definition: SemanticDefinitionId,
        limits: &Limits,
        left: &mut usize,
    ) -> R<CallKind> {
        *left = left
            .checked_sub(program.definitions.len().saturating_add(1))
            .ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "original fixed ABI lookup exceeded", None)
            })?;
        if !self.complete.contains(&definition) {
            return Err(bad(
                "fixed ABI requested before complete original body typing",
            ));
        }
        let original = program
            .definitions
            .iter()
            .find(|original| original.id == definition)
            .ok_or_else(|| bad("fixed ABI original definition is absent"))?;
        let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &original.body else {
            return Err(bad("fixed ABI owner is not original code"));
        };
        let body = self
            .effective_call_kind
            .get(&definition)
            .ok_or_else(|| bad("fixed ABI requested before independent original effect proof"))?;
        if signature.call_kind == CallKind::Direct && *body == CallKind::Suspend {
            return Err(bad("original body exceeds its fixed Direct convention"));
        }
        let _ = limits;
        Ok(signature.call_kind)
    }
    pub(super) fn provider_signature(
        &self,
        caller: SemanticDefinitionId,
        site: SemanticSiteId,
        provider: SemanticDefinitionId,
        limits: &Limits,
        left: &mut usize,
    ) -> R<Option<SemanticSignatureRecipe>> {
        let Some(call) = self.method_calls.get(&(caller, site)) else {
            return Ok(None);
        };
        if !self.complete.contains(&caller) || !self.complete.contains(&provider) {
            return Err(bad(
                "method boundary lacks complete original caller/provider body proofs",
            ));
        }
        if provider != call.declaration && self.providers.get(&provider) != Some(&call.declaration)
        {
            return Err(bad(
                "actual method has no original quantified declaration conformance",
            ));
        }
        crate::dyn_metadata::semantic_operand_snapshot(&call.signature, limits, left, 1)?;
        Ok(Some(call.signature.clone()))
    }
    pub(super) fn call_actual(
        &self,
        owner: SemanticDefinitionId,
        call: SemanticSiteId,
        formal: u32,
        limits: &Limits,
        left: &mut usize,
    ) -> R<SemanticActualRecipe> {
        match self.call_actual_view(owner, call, formal, limits, left)? {
            std::borrow::Cow::Borrowed(value) => {
                crate::dyn_metadata::semantic_operand_snapshot(value, limits, left, 1)?;
                Ok(value.clone())
            }
            std::borrow::Cow::Owned(value) => Ok(value),
        }
    }
    pub(super) fn call_actual_view(
        &self,
        owner: SemanticDefinitionId,
        call: SemanticSiteId,
        formal: u32,
        limits: &Limits,
        left: &mut usize,
    ) -> R<std::borrow::Cow<'_, SemanticActualRecipe>> {
        if !self.complete.contains(&owner) {
            return Err(bad(
                "original inference owner has no complete symbolic proof",
            ));
        }
        let value = self
            .calls
            .get(&(owner, call))
            .and_then(|values| values.get(formal as usize))
            .ok_or_else(|| bad("original call/formal has no derived inference solution"))?;
        if matches!(
            value,
            SemanticActualRecipe::Type(SemanticTypeRecipe::Opaque { .. })
        ) {
            crate::dyn_metadata::semantic_operand_snapshot(value, limits, left, 1)?;
            *left = left.checked_sub(3).ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "original inferred type source work exceeded",
                    None,
                )
            })?;
            if let Some(callee) = self.call_callees.get(&(owner, call)) {
                if let Some(source) = self.inferred_type_sources.get(&(owner, *callee, formal)) {
                    let Some((node, _, _)) = source else {
                        return Err(pending(
                            "inferred whole Type has several original opaque result boundaries",
                        ));
                    };
                    let original = self
                        .types
                        .get(&(owner, Query::Expression(*node)))
                        .ok_or_else(|| {
                            bad("inferred Type source loses its original expression equation")
                        })?;
                    crate::dyn_metadata::semantic_operand_snapshot(original, limits, left, 1)?;
                    let SemanticActualRecipe::Type(actual) = value else {
                        unreachable!()
                    };
                    if original != actual {
                        return Err(bad(
                            "inferred Type source differs from its complete original solution",
                        ));
                    }
                    return Ok(std::borrow::Cow::Owned(SemanticActualRecipe::Type(
                        SemanticTypeRecipe::OriginalType {
                            scope: SemanticScopeRef::Ancestor(0),
                            source: SemanticOriginalTypeSource::Expression(*node),
                        },
                    )));
                }
            }
        }
        Ok(std::borrow::Cow::Borrowed(value))
    }
}

#[cfg(test)]
#[path = "original_query_view_tests.rs"]
mod query_view_tests;
