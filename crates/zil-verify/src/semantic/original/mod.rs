//! Universal original-body checking uses the shared Core operation grammar.
//! No actual instance, Typed SSA Type or target proposal supplies inference.
use super::*;
use zil_base::NodeId;
mod bodies;
mod callbacks;
mod calls;
mod code_sources;
mod dyn_types;
mod expressions;
mod index;
mod patterns;
mod range_bindings;
mod recipes;
mod terms;
pub(in crate::semantic) use range_bindings::RangeBinding;
mod associated;
mod capabilities;
mod carriers;
mod cleanup;
mod closure_sources;
mod constructors;
mod effects;
mod elements;
mod external_abi;
mod formations;
mod frozen;
mod heads;
mod invocation;
mod lifetimes;
mod local_roles;
mod materialize;
mod names;
mod operations;
mod places;
pub(in crate::semantic) use places::Assignment;
mod protocol;
mod receiver_signatures;
mod record_constructors;
mod resources;
mod schemes;
mod scoped_threads;
mod scoped_callbacks;
pub(in crate::semantic) use scoped_callbacks::ScopedCallbackSources;
mod sealed;
mod structural;
mod substitution;
mod tasks;
mod thread_graph;
mod trait_calls;
mod trait_methods;
mod usage;
pub(in crate::semantic) use trait_calls::{MethodActualRecipe, TraitCallRecipes};
mod integer_inference;
mod call_constraints;
mod fallible_returns;
use call_constraints::DeferredCodeCall;
pub(super) mod intrinsics;
mod opaque;
mod raw_schemes;
use dyn_types::*;
use std::collections::{BTreeMap, BTreeSet};
use terms::*;
use usage::{Binder, Place, Uses};
#[derive(Clone)]
pub(super) struct Prototype {
    pub(super) parameters: Vec<Ty>,
    pub(super) result: Ty,
    pub(super) environment: Env,
    pub(super) modes: Vec<PassingMode>,
    pub(super) result_mode: PassingMode,
    pub(super) call_kind: CallKind,
    pub(super) bound_lifetimes: u32,
    pub(super) invocation_lifetimes: u32,
    pub(super) premise_bounds: Vec<Vec<Life>>,
    pub(super) callable_kind: Option<CallableKind>,
}
struct VariantConstruction {
    definition: SemanticDefinitionId,
    variant: u32,
    actuals: Vec<Arg>,
}
#[derive(Default)]
pub(super) struct CheckedOriginal {
    // Structural cursors and original expressions borrow this exact owned,
    // immutable source snapshot. They cannot inherit a recycled view address.
    source_grammar: Option<index::SourceIndex>,
    pub(super) foreign_literals: BTreeMap<SemanticDefinitionId, BTreeMap<SemanticSiteId, Constant>>,
    nominal_field_owner: Option<SemanticDefinitionId>,
    nominal_value_formals: BTreeSet<(SemanticDefinitionId, u32)>,
    expressions: BTreeMap<(SemanticDefinitionId, NodeId), Ty>,
    receiver_borrows: BTreeSet<(SemanticDefinitionId, NodeId)>,
    receiver_results: BTreeSet<SemanticDefinitionId>,
    receiver_call_roles: BTreeMap<(SemanticDefinitionId, NodeId), (Life, Life)>,
    // Category of the evaluated Place before a later field observation
    // publishes an effective Desc type for that same original expression.
    place_types: BTreeMap<(SemanticDefinitionId, NodeId), Ty>,
    // Contextual sum observations are distinct from the payload operation.
    // The selected arm is solved in original Γ before any actual emission.
    injections: BTreeMap<(SemanticDefinitionId, NodeId), (bool, Ty)>,
    bindings: BTreeMap<(SemanticDefinitionId, NodeId), Ty>,
    local_roles: BTreeMap<(SemanticDefinitionId, NodeId), local_roles::LocalRole>,
    binding_families: BTreeMap<(SemanticDefinitionId, NodeId), Ty>,
    parameters: BTreeMap<(SemanticDefinitionId, u32), Ty>,
    results: BTreeMap<SemanticDefinitionId, Ty>,
    calls: BTreeMap<(SemanticDefinitionId, SemanticSiteId), Vec<Arg>>,
    intrinsic_type_sources: BTreeMap<(SemanticDefinitionId, SemanticSiteId, u32), NodeId>,
    call_type_sources: BTreeMap<(SemanticDefinitionId, SemanticSiteId, u32), Vec<(NodeId, bool)>>,
    call_targets: BTreeMap<(SemanticDefinitionId, SemanticSiteId), SemanticDefinitionId>,
    trait_calls: BTreeMap<(SemanticDefinitionId, SemanticSiteId), trait_calls::TraitCall>,
    constructors: BTreeMap<(SemanticDefinitionId, SemanticSiteId), VariantConstruction>,
    projection_terms: Vec<(Ty, LifeDeclarationId, Vec<Arg>, u32, Ty)>,
    trait_witnesses: Vec<(LifeDeclarationId, LifeDeclarationId)>,
    task_closed_cleanups: BTreeSet<(SemanticDefinitionId, Place)>,
    task_async_cleanups: BTreeSet<(SemanticDefinitionId, Place)>,
    task_synchronous_cleanups: BTreeSet<(SemanticDefinitionId, Place)>,
    callable_retirements: BTreeMap<(SemanticDefinitionId, Place), cleanup::CallableRetirement>,
    direct_consuming_goals: Vec<(SemanticDefinitionId, Ty, Env)>,
    direct_cleanup_snapshots: Vec<cleanup::Cleanup>,
    capture_exit_snapshots: BTreeMap<SemanticDefinitionId, Vec<protocol::CaptureExit>>,
    proven_synchronous_births: BTreeSet<(SemanticDefinitionId, NodeId)>,
    callable_nodes: BTreeMap<(SemanticDefinitionId, NodeId), NodeId>,
    code_sources: BTreeMap<(SemanticDefinitionId, NodeId), SemanticDefinitionId>,
    thin_literals: BTreeMap<SemanticDefinitionId, SemanticSiteId>,
    thin_invocations: BTreeMap<SemanticDefinitionId, Vec<Life>>,
    prototypes: BTreeMap<SemanticDefinitionId, Prototype>,
    substitutions: BTreeMap<u64, Ty>,
    integer_variables: BTreeSet<u64>,
    integer_literals: Vec<(Ty, String, bool)>,
    deferred_code_calls: Vec<DeferredCodeCall>,
    deferred_binary: Vec<(BinaryOp, Ty)>,
    propagating_bodies: BTreeSet<SemanticDefinitionId>,
    pending_injections: BTreeMap<SemanticDefinitionId, Vec<(NodeId, Ty, Ty)>>,
    life_substitutions: BTreeMap<u64, Life>,
    outer_inferences: BTreeMap<u64, Vec<Life>>,
    call_regions: BTreeMap<u64, (SemanticDefinitionId, NodeId, Vec<u32>, Vec<Life>)>,
    call_head_roles: BTreeMap<u64, Vec<u32>>,
    call_region_parameters: BTreeMap<(SemanticDefinitionId, NodeId), Vec<(u32, u64)>>,
    borrow_parents: BTreeMap<(SemanticDefinitionId, NodeId), Life>,
    outer_flows: BTreeMap<SemanticDefinitionId, Vec<(Life, Life)>>,
    life_outlives: BTreeMap<SemanticDefinitionId, Vec<(Life, Life)>>,
    life_equalities: BTreeMap<SemanticDefinitionId, Vec<(Life, Life)>>,
    next_inference: u64,
    environments: BTreeMap<SemanticDefinitionId, Env>,
    active: BTreeSet<SemanticDefinitionId>,
    checked: BTreeSet<SemanticDefinitionId>,
    equalities: BTreeMap<SemanticDefinitionId, Vec<(Ty, Ty)>>,
    current: Option<SemanticDefinitionId>,
    premise_environment: Option<Env>,
    free_values: BTreeMap<SemanticDefinitionId, BTreeMap<String, Binding>>,
    uses: BTreeMap<SemanticDefinitionId, Uses>,
    used: BTreeMap<SemanticDefinitionId, BTreeSet<Binder>>,
    element_obligations: Vec<(Ty, Env)>,
    copy_obligations: Vec<(Ty, Env)>,
    written: BTreeMap<SemanticDefinitionId, BTreeSet<Binder>>,
    live: BTreeMap<SemanticDefinitionId, BTreeMap<Binder, Binding>>,
    cleanups: Vec<cleanup::Cleanup>,
    creator_moves: BTreeMap<SemanticDefinitionId, bool>,
    value_bindings: BTreeMap<(SemanticDefinitionId, NodeId), Binder>,
    value_places: BTreeMap<(SemanticDefinitionId, NodeId), Place>,
    names_at: BTreeMap<(SemanticDefinitionId, bool, NodeId, String), Ty>,
    name_binders_at: BTreeMap<(SemanticDefinitionId, bool, NodeId, String), Binder>,
    pattern_types: BTreeMap<(SemanticDefinitionId, NodeId, Vec<u32>), Ty>,
    pattern_bindings: BTreeMap<(SemanticDefinitionId, NodeId, Vec<u32>), Binder>,
    pattern_origin: Option<(NodeId, Vec<u32>)>,
    unsafe_depth: BTreeMap<SemanticDefinitionId, usize>,
    in_defer: BTreeMap<SemanticDefinitionId, usize>,
    created: BTreeSet<(SemanticDefinitionId, NodeId)>,
    created_bindings: BTreeSet<Binder>,
    creation_tainted: BTreeSet<Binder>,
    creation_returns: Vec<Binder>,
    executing_effects: BTreeMap<SemanticDefinitionId, CallKind>,
    intrinsic_calls: BTreeMap<(SemanticDefinitionId, SemanticSiteId), intrinsics::Use>,
    scoped_callbacks: BTreeMap<(SemanticDefinitionId, SemanticSiteId), ScopedCallbackSources>,
    scoped_threads:
        BTreeMap<(SemanticDefinitionId, SemanticSiteId), (SemanticDefinitionId, NodeId, NodeId)>,
    scoped_thread_workers: Vec<(SemanticDefinitionId, Ty, Env)>,
}
impl CheckedOriginal {
    pub(super) fn check(
        module: &TypedModule,
        context: &CheckedContext,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<Self> {
        let mut result = Self::default();
        let index = index::build(module, b)?;
        result.source_grammar = Some(index::snapshot(module, &index, b)?);
        for function in index.definitions.values() {
            let mut required = BTreeSet::new();
            b.spend(function.expressions.len())?;
            for expression in function.expressions.values() {
                if function.definition.kind == SemanticDefinitionKind::Closure && matches!(expression.kind, CoreExprKind::Try(_)) {
                    result.propagating_bodies.insert(function.definition.id);
                }
                if matches!(expression.kind, CoreExprKind::Literal(Constant::NullHandle | Constant::NullWideString | Constant::WideString(_))) {
                    required.insert(expression.id);
                }
            }
            if !required.is_empty() {
                b.spend(function.sites.len())?;
                let entries = result.foreign_literals.entry(function.definition.id).or_default();
                for site in function.sites {
                    let Some(index::Node::Expression(expression)) = function.nodes.get(&site.id) else {continue};
                    let CoreExprKind::Literal(value @ (Constant::NullHandle | Constant::NullWideString | Constant::WideString(_))) = &expression.kind else {continue};
                    let SemanticSiteRecipeKind::TypeUse {expected: SemanticTypeRecipe::OriginalType {scope: SemanticScopeRef::Ancestor(0), source: SemanticOriginalTypeSource::Expression(node)}} = &site.kind else {continue};
                    if *node != expression.id || !required.remove(node) {
                        return Err(bad("foreign literal TypeUse changes or repeats its original expression"));
                    }
                    dynamic_metadata::constant(value,b)?;
                    entries.insert(site.id,value.clone());
                }
                if !required.is_empty() {return Err(bad("original foreign literal lacks its exact TypeUse"));}
            }
            b.spend(function.parameters.len().saturating_add(1))?;
            if function.parameters.len() != function.signature.parameters.len() {
                return Err(bad(
                    "original body index and signature have different value arity",
                ));
            }
        }
        let program = module
            .semantic
            .as_ref()
            .ok_or_else(|| bad("original body program is absent"))?;
        let mut pending = program
            .definitions
            .iter()
            .filter(|definition| {
                matches!(
                    definition.kind,
                    SemanticDefinitionKind::Function
                        | SemanticDefinitionKind::Method
                        | SemanticDefinitionKind::Closure
                        | SemanticDefinitionKind::TaskEntry
                )
            })
            .collect::<Vec<_>>();
        b.spend(pending.len())?;
        // Actual source parents must be checked first, so free values close
        // over the original creator/binding point instead of a child LocalId.
        b.spend(
            pending
                .len()
                .saturating_mul(pending.len().checked_ilog2().map_or(1, |n| n as usize + 1))
                .saturating_mul(b.limits.max_call_depth.min(128)),
        )?;
        pending.sort_by_key(|definition| {
            let mut depth = 0;
            let mut parent = definition.parent;
            while let Some(id) = parent {
                depth += 1;
                parent = program
                    .definitions
                    .iter()
                    .find(|row| row.id == id)
                    .and_then(|row| row.parent);
            }
            depth
        });
        let total = pending.len();
        for (ordinal, definition) in pending.into_iter().enumerate() {
            b.diagnostic_stage("original bodies");
            if let Some(progress) = &mut b.diagnostic_progress {
                progress.original = Some((ordinal + 1, total, definition.id));
            }
            result.definition(module, context, definition.id, active, b)?;
        }
        result.solve_code_calls(b)?;
        result.finish_integer_literals(module, b)?;
        for (op, operand) in std::mem::take(&mut result.deferred_binary) {
            let operand = result.resolved(&operand, b)?;
            result.check_binary_carrier(op, &operand)?;
        }
        result.check_outer_flows(b)?;
        b.diagnostic_stage("original body obligations");
        let obligations = std::mem::take(&mut result.copy_obligations);
        for (ty, env) in obligations {
            result.current = Some(env.current()?);
            if !result.is_copy(module, context, &ty, &env, active, b)? {
                return Err(bad(
                    "original repeated value use has no resolved Copy proof",
                ));
            }
        }
        for (ty, env) in std::mem::take(&mut result.element_obligations) {
            result.current = Some(env.current()?);
            result.structural_obligation(
                module,
                context,
                &ty,
                DynStructuralBound::StoreElement,
                &env,
                active,
                b,
            )?;
        }
        result.check_constructor_sites(&index, b)?;
        result.collect_nominal_formation(module, context, active, b)?;
        result.check_original_applications(module, context, active, b)?;
        result.check_scoped_thread_workers(module, context, active, b)?;
        result.check_trait_witnesses(module, context, active, b)?;
        result.current = None;

        // No original protocol claim is usable by the actual bridge until
        // every original body, including uninstantiated bodies, has been
        // checked against its own entry relation.
        b.diagnostic_stage("original protocols and cleanup");
        result.check_protocols(module, context, &index, active, b)?;
        result.check_cleanups(module, context, active, b)?;
        for (definition, _) in &result.task_async_cleanups {
            b.spend(1)?;
            if result
                .prototypes
                .get(definition)
                .is_none_or(|prototype| prototype.call_kind != CallKind::Suspend)
            {
                return Err(bad(
                    "original asynchronous Task cleanup has no complete Suspend head",
                ));
            }
            result
                .executing_effects
                .insert(*definition, CallKind::Suspend);
        }
        result.check_effects(b)?;
        for binding in &result.creation_returns {
            b.spend(1)?;
            if result.creation_tainted.contains(binding) {
                return Err(bad(
                    "original create result has a reassigned or loop-carried unproved lineage",
                ));
            }
        }
        Ok(result)
    }
    pub(in crate::semantic) fn compile_time_uses(&self, definition: SemanticDefinitionId, b: &mut Budget) -> Check<&[(usize, NodeId)]> {
        self.source_grammar.as_ref()
            .ok_or_else(|| bad("compile-time result has no checked source inventory"))?
            .compile_time_uses(definition, b)
    }
    fn source_index<'s>(
        &'s self,
        _module: &TypedModule,
        definitions: &[SemanticDefinitionId],
        b: &mut Budget,
    ) -> Check<index::Index<'s>> {
        let source = self
            .source_grammar
            .as_ref()
            .ok_or_else(|| bad("original source query has no owned complete grammar"))?;
        index::select(source, definitions, b)
    }
    fn original_call_site(
        &self,
        definition: SemanticDefinitionId,
        node: NodeId,
        b: &mut Budget,
    ) -> Check<(usize, SemanticSiteId)> {
        let source = self
            .source_grammar
            .as_ref()
            .ok_or_else(|| bad("original call has no owned complete grammar"))?;
        let rows = source.recipes(
            definition,
            SemanticOriginalNode::Expression(node),
            &[
                index::Role::Call,
                index::Role::CallableCall,
                index::Role::OriginalCall,
            ],
            b,
        )?;
        let [row] = rows.as_slice() else {
            return Err(bad("original Call has no unique exact operation site"));
        };
        Ok(*row)
    }
    pub(in crate::semantic) fn original_declaration_sites(
        &self,
        definition: SemanticDefinitionId,
        node: NodeId,
        b: &mut Budget,
    ) -> Check<Vec<SemanticSiteId>> {
        let source = self
            .source_grammar
            .as_ref()
            .ok_or_else(|| bad("original local has no owned complete grammar"))?;
        Ok(source
            .recipes(
                definition,
                SemanticOriginalNode::Statement(node),
                &[index::Role::DeclareLocal],
                b,
            )?
            .into_iter()
            .map(|(_, site)| site)
            .collect())
    }

    fn fresh(&mut self, b: &mut Budget) -> Check<Ty> {
        b.spend(1)?;
        let id = self.next_inference;
        self.next_inference = id
            .checked_add(1)
            .ok_or_else(|| limit("original body inference identity exhausted"))?;
        Ok(Ty::Infer(id))
    }
    fn resolved(&self, ty: &Ty, b: &mut Budget) -> Check<Ty> {
        self.resolve_at(ty, 0, b)
    }
    fn resolve_at(&self, ty: &Ty, depth: usize, b: &mut Budget) -> Check<Ty> {
        b.spend(1)?;
        if depth > b.limits.max_call_depth.min(128) {
            return Err(limit("original symbolic type substitution depth exceeded"));
        }
        Ok(match ty {
            Ty::Infer(id) => match self.substitutions.get(id) {
                Some(value) => return self.resolve_at(value, depth + 1, b),
                None => ty.clone(),
            },
            Ty::Tuple(fields) => {
                let mut values = Vec::new();
                for field in fields {
                    values.push(self.resolve_at(field, depth + 1, b)?);
                }
                if values.is_empty() {
                    Ty::Atom(Type::Unit)
                } else {
                    Ty::Tuple(values)
                }
            }
            Ty::Address {
                mutable,
                life,
                inner,
            } => Ty::Address {
                mutable: *mutable,
                life: self.resolved_life(life, b)?,
                inner: Box::new(self.resolve_at(inner, depth + 1, b)?),
            },
            Ty::Desc { life, inner } => Ty::Desc {
                life: self.resolved_life(life, b)?,
                inner: Box::new(self.resolve_at(inner, depth + 1, b)?),
            },
            Ty::Array(inner, length) => Ty::Array(
                Box::new(self.resolve_at(inner, depth + 1, b)?),
                length.clone(),
            ),
            Ty::Slice(inner) => Ty::Slice(Box::new(self.resolve_at(inner, depth + 1, b)?)),
            Ty::Managed(inner) => Ty::Managed(Box::new(self.resolve_at(inner, depth + 1, b)?)),
            Ty::Store(inner) => Ty::Store(Box::new(self.resolve_at(inner, depth + 1, b)?)),
            Ty::Dyn(value) => Ty::Dyn(Box::new(value.map(
                |arg, b| {
                    Ok(self
                        .resolve_arguments(std::slice::from_ref(arg), depth + 1, b)?
                        .remove(0))
                },
                b,
            )?)),
            Ty::Carrier {
                kind,
                child,
                retained,
            } => Ty::Carrier {
                kind: *kind,
                child: Box::new(self.resolve_at(child, depth + 1, b)?),
                retained: Box::new(retained.map(
                    |ty, b| self.resolve_at(ty, depth + 1, b),
                    |life, b| self.resolved_life(life, b),
                    b,
                )?),
            },
            Ty::ErrorUnion(error, result) => Ty::ErrorUnion(
                Box::new(self.resolve_at(error, depth + 1, b)?),
                Box::new(self.resolve_at(result, depth + 1, b)?),
            ),
            Ty::Code {
                kind,
                parameters,
                result,
                modes,
                result_mode,
                call_kind,
                bound_lifetimes,
                invocation_lifetimes,
            } => {
                let mut values = Vec::new();
                for parameter in parameters {
                    values.push(self.resolve_at(parameter, depth + 1, b)?);
                }
                Ty::Code {
                    kind: *kind,
                    parameters: values,
                    result: Box::new(self.resolve_at(result, depth + 1, b)?),
                    modes: modes.clone(),
                    result_mode: *result_mode,
                    call_kind: *call_kind,
                    bound_lifetimes: *bound_lifetimes,
                    invocation_lifetimes: *invocation_lifetimes,
                }
            }
            Ty::Nominal(id, arguments) => {
                Ty::Nominal(*id, self.resolve_arguments(arguments, depth, b)?)
            }
            Ty::Opaque(id, arguments) => {
                Ty::Opaque(*id, self.resolve_arguments(arguments, depth, b)?)
            }
            Ty::Projection {
                receiver,
                declaration,
                arguments,
                member,
            } => Ty::Projection {
                receiver: Box::new(self.resolve_at(receiver, depth + 1, b)?),
                declaration: *declaration,
                arguments: self.resolve_arguments(arguments, depth, b)?,
                member: *member,
            },
            Ty::Closure(id, arguments, environment) => Ty::Closure(
                *id,
                self.resolve_arguments(arguments, depth, b)?,
                self.resolve_arguments(environment, depth, b)?,
            ),
            _ => ty.clone(),
        })
    }
    fn resolve_arguments(
        &self,
        arguments: &[Arg],
        depth: usize,
        b: &mut Budget,
    ) -> Check<Vec<Arg>> {
        let mut values = Vec::new();
        for argument in arguments {
            b.spend(1)?;
            values.push(match argument {
                Arg::Type(ty) => Arg::Type(self.resolve_at(ty, depth + 1, b)?),
                Arg::Life(life) => Arg::Life(self.resolved_life(life, b)?),
                _ => argument.clone(),
            });
        }
        Ok(values)
    }
    fn same(&self, left: &Ty, right: &Ty, b: &mut Budget) -> Check<bool> {
        let left = self.resolved(left, b)?;
        let right = self.resolved(right, b)?;
        let equalities = self
            .current
            .and_then(|id| self.equalities.get(&id))
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        b.spend(equalities.len().saturating_add(1))?;
        if left == right {
            return Ok(true);
        }
        if let (Ty::Code{parameters:a,result:ar,invocation_lifetimes:n,..},Ty::Code{parameters:z,result:zr,invocation_lifetimes:m,..})=(&left,&right){
            if n==m && *n!=0{
                let to=Self::code_invocation_frame(a,ar,*n,b)?;
                let from=Self::code_invocation_frame(z,zr,*m,b)?;
                let rewritten=self.map_invocation_type(&right,&mut |life,b|{
                    b.spend(1)?;
                    if let Life::Invocation(_,index)=life {
                        if from.get(*index as usize)==Some(life){return Ok(to[*index as usize].clone());}
                    }
                    Ok(life.clone())
                },0,b)?;
                if left==rewritten{return Ok(true);}
            }
        }
        let mut queue = vec![&left];
        let mut seen = Vec::new();
        while let Some(current) = queue.pop() {
            b.spend(
                seen.len()
                    .saturating_add(equalities.len())
                    .saturating_add(1),
            )?;
            if seen.contains(&current) {
                continue;
            }
            for (a, c) in equalities {
                if a == current {
                    if c == &right {
                        return Ok(true);
                    }
                    queue.push(c);
                }
                if c == current {
                    if a == &right {
                        return Ok(true);
                    }
                    queue.push(a);
                }
            }
            seen.push(current);
        }
        Ok(false)
    }
    fn unify(&mut self, left: &Ty, right: &Ty, b: &mut Budget) -> Check<()> {
        let left = self.resolved(left, b)?;
        let right = self.resolved(right, b)?;
        if self.same(&left, &right, b)? {
            return Ok(());
        }
        match (&left, &right) {
            (Ty::Infer(id), ty) | (ty, Ty::Infer(id)) => {
                if contains(ty, *id, b)? {
                    return Err(bad("infinite original type inference"));
                }
                b.spend(1)?;
                if self.integer_variables.contains(id) {
                    match ty {
                        Ty::Infer(other) => { self.integer_variables.insert(*other); }
                        Ty::Atom(atom) if atom.is_integer() => {}
                        _ => return Err(bad("original integer literal requires an integer carrier")),
                    }
                }
                self.substitutions.insert(*id, ty.clone());
                Ok(())
            }
            (Ty::Tuple(a), Ty::Tuple(c)) if a.len() == c.len() => {
                for (a, c) in a.iter().zip(c) {
                    self.unify(a, c, b)?;
                }
                Ok(())
            }
            (
                Ty::Address {
                    mutable: a,
                    life: al,
                    inner: ai,
                },
                Ty::Address {
                    mutable: c,
                    life: cl,
                    inner: ci,
                },
            ) if a == c => {
                self.unify_life(al, cl, b)?;
                self.unify(ai, ci, b)
            }
            (
                Ty::Desc {
                    life: al,
                    inner: ai,
                },
                Ty::Desc {
                    life: cl,
                    inner: ci,
                },
            ) => {
                self.unify_life(al, cl, b)?;
                self.unify(ai, ci, b)
            }
            (Ty::Managed(a), Ty::Managed(c))
            | (Ty::Store(a), Ty::Store(c))
            | (Ty::Slice(a), Ty::Slice(c)) => self.unify(a, c, b),
            (Ty::ErrorUnion(a, r), Ty::ErrorUnion(c, s)) => {
                self.unify(a, c, b)?;
                self.unify(r, s, b)
            }
            (Ty::Array(a, n), Ty::Array(c, m)) if n == m => self.unify(a, c, b),
            (Ty::Dyn(a), Ty::Dyn(z)) => self.unify_dyn(a, z, b),
            (
                Ty::Carrier {
                    kind: a,
                    child: x,
                    retained: ar,
                },
                Ty::Carrier {
                    kind: c,
                    child: y,
                    retained: cr,
                },
            ) if a == c => {
                self.unify(x, y, b)?;
                if ar == cr {
                    Ok(())
                } else {
                    self.unify_retained(ar,cr,b).map_err(|error|error.with_note(format!("original carrier complete retained recipes: actual {ar:?}; expected {cr:?}; result types actual {x:?}; expected {y:?}")))
                }
            }
            (Ty::Nominal(a, aa), Ty::Nominal(c, ca)) | (Ty::Opaque(a, aa), Ty::Opaque(c, ca))
                if a == c && aa.len() == ca.len() =>
            {
                for (a, c) in aa.iter().zip(ca) {
                    match (a, c) {
                        (Arg::Type(a), Arg::Type(c)) => self.unify(a, c, b)?,
                        (Arg::Life(a), Arg::Life(c)) => self.unify_life(a, c, b)?,
                        _ if a == c => {}
                        _ => return Err(bad("original nominal actuals require a proved equality")),
                    }
                }
                Ok(())
            }
            _ => Err(bad(&format!(
                "original body relates independent rigid type/const identities without an original equality (actual {}, expected {})",
                type_category(&left),
                type_category(&right)
            ))),
        }
    }
    fn environment(
        &mut self,
        module: &TypedModule,
        context: &CheckedContext,
        id: SemanticDefinitionId,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<Env> {
        if let Some(environment) = self.environments.get(&id) {
            b.spend(environment.lexical.len())?;
            return Ok(environment.clone());
        }
        let definition = context.definition(module, id, b)?;
        let mut environment = if let Some(parent) = definition.parent {
            self.environment(module, context, parent, active, b)?
        } else {
            Env {
                lexical: vec![],
                bounds: vec![],
                invocation: vec![],
                self_type: None,
            }
        };
        let mut actuals = Vec::new();
        for (ordinal, formal) in definition.parameters.iter().enumerate() {
            b.spend(1)?;
            actuals.push(match formal {
                SemanticFormal::Type { .. } => Arg::Type(Ty::Variable(id, ordinal as u32)),
                SemanticFormal::Lifetime => Arg::Life(Life::Formal(id, ordinal as u32)),
                SemanticFormal::Const { carrier } => {
                    let mut scope = environment.clone();
                    scope.lexical.insert(
                        0,
                        Frame {
                            definition: id,
                            actuals: actuals.clone(),
                            environment: vec![],
                        },
                    );
                    let carrier = self.ty(module, context, carrier, &scope, active, b)?;
                    Arg::Const(Const {
                        carrier: Box::new(carrier),
                        value: ConstValue::Variable(id, ordinal as u32),
                    })
                }
            });
        }
        let mut sealed = Vec::new();
        for formal in &definition.environment {
            let value = self.actual(module, context, &formal.source, &environment, active, b)?;
            if !matches!(
                (&formal.kind, &value),
                (SemanticFormal::Type { .. }, Arg::Type(_))
                    | (SemanticFormal::Const { .. }, Arg::Const(_))
                    | (SemanticFormal::Lifetime, Arg::Life(_))
            ) {
                return Err(bad(
                    "original sealed environment changes its declared mixed kind",
                ));
            }
            if let (SemanticFormal::Const { carrier }, Arg::Const(value)) = (&formal.kind, &value) {
                let expected = self.ty(module, context, carrier, &environment, active, b)?;
                self.unify(&value.carrier, &expected, b)?;
            }
            sealed.push(value);
        }
        environment.lexical.insert(
            0,
            Frame {
                definition: id,
                actuals,
                environment: sealed,
            },
        );
        self.environments.insert(id, environment.clone());
        Ok(environment)
    }
}
fn atom_category(ty: &Type) -> &'static str {
    match ty {
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
        Type::ForeignHandle => "ForeignHandle",
        Type::CWideString => "CWideString",
        Type::ByteBuffer => "ByteBuffer",
        Type::SyncCell(_) => "SyncCell",
        _ => "NonAtomicType",
    }
}
fn type_category(ty: &Ty) -> String {
    match ty {
        Ty::Atom(ty) => atom_category(ty).into(),
        Ty::Variable(definition, ordinal) => format!("TypeFormal({definition:?},{ordinal})"),
        Ty::SelfType(definition) => format!("Self({definition:?})"),
        Ty::Nominal(definition, args) => format!("Nominal({definition:?},{} actuals)", args.len()),
        Ty::Opaque(definition, args) => format!("Opaque({definition:?},{} actuals)", args.len()),
        Ty::Projection {
            declaration,
            member,
            ..
        } => format!("Projection({declaration:?},{member})"),
        Ty::Tuple(fields) => format!("Tuple({} fields)", fields.len()),
        Ty::Array(_, length) => format!(
            "Array(length carrier {}, {})",
            match length.carrier.as_ref() {
                Ty::Atom(ty) => atom_category(ty),
                _ => "symbolic",
            },
            match &length.value {
                ConstValue::Value(_) => "literal",
                ConstValue::Variable(..) => "formal",
                _ => "expression",
            }
        ),
        Ty::Address { mutable, .. } => {
            if *mutable {
                "ExclusiveAddress".into()
            } else {
                "SharedAddress".into()
            }
        }
        Ty::Desc { .. } => "Description".into(),
        Ty::Managed(_) => "Managed".into(),
        Ty::Store(_) => "Store".into(),
        Ty::Dyn(value) => format!(
            "Dyn(owned={},trait={:?})",
            value.owned, value.application.declaration
        ),
        Ty::Carrier { kind, .. } => format!("{kind:?}"),
        Ty::Closure(definition, ..) => format!("Closure({definition:?})"),
        Ty::Function(definition) => format!("Function({definition:?})"),
        Ty::Code { .. } => "Code".into(),
        Ty::ErrorUnion(..) => "ErrorUnion".into(),
        Ty::Slice(_) => "Slice".into(),
        Ty::SealedError(kind) => format!("{kind:?}"),
        Ty::Infer(id) => format!("Infer({id})"),
    }
}
fn contains(ty: &Ty, needle: u64, b: &mut Budget) -> Check<bool> {
    let mut queue = vec![ty];
    while let Some(ty) = queue.pop() {
        b.spend(1)?;
        match ty {
            Ty::Infer(id) if *id == needle => return Ok(true),
            Ty::Tuple(children) => queue.extend(children),
            Ty::Address { inner, .. }
            | Ty::Desc { inner, .. }
            | Ty::Managed(inner)
            | Ty::Store(inner)
            | Ty::Slice(inner)
            | Ty::Array(inner, _) => queue.push(inner),
            Ty::Dyn(value) => value.types(&mut queue, b)?,
            Ty::Carrier {
                child, retained, ..
            } => {
                queue.push(child);
                retained.types(&mut queue, b)?;
            }
            Ty::ErrorUnion(a, c) => {
                queue.push(a);
                queue.push(c);
            }
            Ty::Code {
                parameters, result, ..
            } => {
                queue.extend(parameters);
                queue.push(result);
            }
            Ty::Nominal(_, arguments)
            | Ty::Opaque(_, arguments)
            | Ty::Projection { arguments, .. } => {
                if let Ty::Projection { receiver, .. } = ty {
                    queue.push(receiver);
                }
                for actual in arguments {
                    match actual {
                        Arg::Type(ty) => queue.push(ty),
                        Arg::Const(value) => queue.push(&value.carrier),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    Ok(false)
}
