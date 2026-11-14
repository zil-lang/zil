mod physical_slots;
use std::collections::{BTreeMap, BTreeSet};
use zil_ir::{CallableFlowNext, CallableTraces};

use zil_base::{CompileResult, Diagnostic, Limits, Phase, SCHEMA_VERSION, Span};
use zil_ir::{
    Constant, FunctionId, InstructionKind, LocalId, StoredBorrowLoad, Terminator, Type,
    TypedModule, ValueId,
};

use crate::bad;

fn exhausted(context: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::limit(
        Phase::Analyze,
        format!("analysis budget exceeded during {context}"),
        None,
    )]
}

pub(super) type TaskRetainedRoots =
    std::collections::BTreeMap<Vec<zil_ir::ContractProjection>, Vec<zil_ir::HoldRoot>>;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum TaskPortSource {
    External(zil_ir::StoragePosition),
    Local(zil_ir::LocalId),
    Static,
    Unknown,
}
pub(super) type TaskRetainedSources =
    std::collections::BTreeMap<Vec<zil_ir::ContractProjection>, Vec<TaskPortSource>>;
pub(super) struct Budget {
    remaining: usize,
    // Only shape nodes borrowed from this request's immutable module enter.
    // The result indexes raw slots; no qualification or semantic flow is cached.
    physical_slot_paths: BTreeMap<usize, BTreeMap<Vec<zil_ir::PlaceProjection>, (u32, bool)>>,
    // Only immutable Type nodes reached from the actual TypedFunction local
    // inventory enter this table. A key is the address of that node for this
    // one analysis request; it never escapes the request or becomes authority.
    addressed_copy_sums: BTreeMap<usize, bool>,
    pub(super) large_tag_layout: bool,
    pub(super) addressed_tag_layout: BTreeMap<usize, bool>,
    // Successes only for exact immutable module roots already registered by
    // the complete raw preflight's owned Store Package inventory.
    pub(super) admitted_owned_products: BTreeMap<(usize, usize), bool>,
    // Derived from raw instructions; only avoids work in unaffected modules.
    pub(super) static_utf8: bool,
    pub(super) pod_stores: bool,
    pub(super) nested_store_windows: bool,
    pub(super) pod_store_carriers: Option<super::pod_store::CarrierCache>,
    pub(super) dyn_interfaces: BTreeSet<zil_ir::DynInterfaceId>,
    // Complete original/actual closure domain proof, published only after the
    // semantic consumer checks every actual entry, creator and capture. This
    // is a zero-free-domain fact, never Copy, cleanup or loan authority.
    pub(super) semantic_zero_closures: BTreeSet<zil_ir::ClosureId>,
    // Complete original/actual class relation for this immutable request,
    // including nonzero phantom domains. No memory/cleanup permission.
    pub(super) semantic_closures_proven: BTreeSet<zil_ir::ClosureId>,
    // Exact constructor occurrences whose complete original class value and
    // actual header embedding have both been proved. No target/loan facts.
    pub(super) semantic_aggregate_classes:
        BTreeMap<(FunctionId, ValueId, u32), zil_ir::StorageBinder>,
    // Actual closed execution graph, not an ABI rewrite or a Task capability.
    pub(super) stored_synchronous_abis: BTreeSet<FunctionId>,
    pub(super) dyn_contexts: bool,
    // Concrete-function result occurrences awaiting actual CFG grounding.
    // Neither a signature nor its declared source upper proves these rows.
    pub(super) pending_dyn_outputs:
        BTreeMap<zil_ir::FunctionId, Vec<super::stored_flow::DynOutputClass>>,
    pub(super) partial_depth: usize,
    pub(super) helper_nodes: usize,
    pub(super) helper_calls: usize,
    pub(super) helper_edges: usize,
    pub(super) helper_depth: usize,
    pub(super) shared_product_capture: bool,
    // Actual nonempty Package presence from the paid complete raw-type walk.
    // An affine zero-debt value still needs the independent resource flow.
    pub(super) affine_packages: bool,
    pub(super) address_capture: bool,
    pub(super) owning_address_captures: bool,
    pub(super) mutable_capture_contexts: bool,
    pub(super) receiver_borrows: bool,
    // Raw presence schedules a complete independent proof, never a domain.
    pub(super) static_occurrences: bool,
    // Actual return-type presence only schedules refreshing a qualified head's
    // current pointee rows. It never grants a root, class, or permission.
    pub(super) qualified_head_returns: bool,
    // Schedules actual returned-environment transport; contracts grant no roots.
    pub(super) returned_borrowed_environments: BTreeSet<zil_ir::ClosureId>,
    pub(super) pending_returned_source_proofs: BTreeSet<zil_ir::FunctionId>,
    pub(super) callable_receiver_roles: BTreeMap<(zil_ir::FunctionId, usize), u32>,
    pub(super) replacement_functions: BTreeSet<zil_ir::FunctionId>,
    pub(super) replacement_error_sums: bool,
    pub(super) address_capture_closures: BTreeSet<zil_ir::ClosureId>,
    // Empty on legacy modules; populated only by the paid ordinary admission.
    pub(super) ordinary_store_functions: Vec<bool>,
    pub(super) sum_functions: Vec<bool>,
    pub(super) addressed_sums: bool,
    pub(super) mixed_address_sums: bool,
    // Reconstructed from actual owned-signature parameters, never a claim.
    pub(super) borrowed_callback_inputs: bool,
    pub(super) ordinary_error_places: bool,
    pub(super) owned_callbacks: BTreeSet<zil_ir::ClosureId>,
    pub(super) owned_callback_helpers: BTreeSet<zil_ir::FunctionId>,
    pub(super) owned_callback_window_functions: Vec<bool>,
    pub(super) parameter_returns: BTreeSet<zil_ir::FunctionId>,
    pub(super) forward_functions: Vec<bool>,
    pub(super) forward_closures: BTreeSet<zil_ir::ClosureId>,
    pub(super) scoped_foreign: bool,
    pub(super) scoped_threads: bool,
    pub(super) generic_mutex: bool,
    pub(super) guard_return_sources: Vec<BTreeSet<u32>>,
    pub(super) borrowed_function_pointers: bool,
    pub(super) tasks: bool,
    // Derived only by the complete paid raw-type preflight, never producer metadata.
    pub(super) task_origin_roots: bool,
    pub(super) borrowed_task_ports: bool,
    pub(super) invocation_callbacks: bool,
    pub(super) head_descriptions: bool,
    pub(super) registrations: bool,
    pub(super) async_registrations: bool,
    pub(super) protocol_contracts: bool,
    pub(super) protocol_checked: bool,
    pub(super) stored_enum_addresses: bool,
    pub(super) task_borrowed_results: bool,
    pub(super) task_retained_environments: bool,
    pub(super) task_retained_roots: Vec<Vec<TaskRetainedRoots>>,
    pub(super) task_callable_results: bool,
    pub(super) task_retained_sources: Vec<Vec<TaskRetainedSources>>,
    // Qualified Option with a shared owning Package pointee needs both the
    // stored Sum proof and ordinary Store/resource proof on the same CFG.
    pub(super) store_option_results: bool,
    // Presence from actual Direct Call types; only schedules the private proof.
    pub(super) qualified_package_returns: bool,
    // Paid actual ExtractIndex inventory; algorithm scheduling only.
    pub(super) array_outer_functions: Vec<zil_ir::FunctionId>,
    pub(super) byte_queries: bool,
    // Presence schedules proof; heads contain only immutable actual IR.
    pub(super) shared_byte_borrow: bool,
    pub(super) owned_byte_heads: Option<crate::stored_flow::owned_byte_views::Heads>,
    pub(super) byte_query_presence_checked: bool,
    pub(super) byte_query_functions: Vec<bool>,
    pub(super) package_return_calls: Option<BTreeMap<(zil_ir::FunctionId, zil_ir::ValueId), u32>>,
    /// Private complete stored-target facts, published only after the stored
    /// solver finishes every reachable CFG and signature relation.
    pub(super) store_option_targets: Option<Vec<Vec<Vec<super::stored_flow::Target>>>>,
    pub(super) store_option_byte_holds: BTreeSet<(FunctionId, ValueId)>,
    /// Readonly SSA positions only. A description's private origin is not a
    /// public borrow/keepalive relation, regardless of its payload or classes.
    pub(super) description_values: Vec<(FunctionId, ValueId)>,
    pub(super) task_pointer_roots: Vec<Vec<Vec<zil_ir::HoldRoot>>>,
    pub(super) task_pointer_parents: Vec<Vec<Vec<crate::stored_flow::TaskLoanAnchor>>>,
    /// A false row is an unselected ordinary function, not an empty proof.
    pub(super) task_stored_coverage: Vec<bool>,
    pub(super) stored_borrows: bool,
    // Scheduling obligations only. Actual permission substitution is proved
    // by the stored CFG solver, never by the presence of a role header.
    pub(super) pending_exclusive_helpers: BTreeSet<zil_ir::FunctionId>,
    pub(super) stored_completed_functions: Option<Vec<bool>>,
    pub(super) ordinary_input_head_candidates: bool,
    pub(super) stored_slot_updates: bool,
    // Current function's actual loan arena, including reserved exclusive sites.
    // Reset for each facts solve; false skips only read/read constructor checks.
    pub(super) stored_exclusive_loans: bool,
    // Empty for the established single-lane profiles. A mixed module has one
    // paid, actual-function partition; neither lane may infer the other lane.
    pub(super) composition_stored: Vec<bool>,
    // Eligibility only; each pure use still proves the actual physical type.
    pub(super) pure_nominal: bool,
    pub(super) capture_relations: bool,
    pub(super) stored_owned_fixed: bool,
    pub(super) stored_compound_values: bool,
    // Presence flags from paid actual-type/slot/capture inventories. These
    // select new proof work and never grant authority to a borrowed value.
    pub(super) stored_packages: bool,
    pub(super) task_failure_carriers: bool,
    pub(super) task_input_scopes: Option<crate::tasks::input_scope::Table>,
    pub(super) owned_task_input_inventory: bool,
    pub(super) stored_fixed_owner: bool,
    pub(super) owning_environment_captures: bool,
    pub(super) stored_projected_slots: bool,
    pub(super) shared_thread_functions: BTreeSet<zil_ir::FunctionId>,
    pub(super) scoped_functions: BTreeSet<zil_ir::FunctionId>,
    pub(super) scoped_helper_bodies: std::collections::BTreeMap<zil_ir::FunctionId, BTreeSet<u32>>,
    sum_roots: BTreeSet<usize>,
    // Exact owning inline-Store Sum capture roots found by the paid raw walk.
    // Empty on old Sum profiles, including owning ByteBuffer Enums.
    ordinary_store_sum_captures: Vec<BTreeSet<usize>>,
    ordinary_store_sum_addresses: BTreeSet<usize>,
    ordinary_pod_store_roots: BTreeSet<usize>,
    owned_store_package_roots: BTreeSet<usize>,
}

impl Budget {
    pub(super) fn task_sources(&self) -> bool {
        self.task_borrowed_results || self.task_failure_carriers || self.task_retained_environments
            // Actual nonempty capture/source relations schedule the full
            // solver; a proved empty relation stays in the ordinary ledgers.
            || self.task_origin_roots && self.capture_relations
    }
    pub(super) fn masked_stored_tasks(&self) -> bool {
        self.store_option_results || self.task_failure_carriers
    }

    pub(super) fn new(limits: &Limits) -> Self {
        Self {
            remaining: limits.max_analysis_steps,
            physical_slot_paths: BTreeMap::new(),
            addressed_copy_sums: BTreeMap::new(),
            large_tag_layout: false,
            addressed_tag_layout: BTreeMap::new(),
            admitted_owned_products: BTreeMap::new(),
            static_utf8: false,
            pod_stores: false,
            nested_store_windows: false,
            pod_store_carriers: None,
            dyn_interfaces: BTreeSet::new(),
            semantic_zero_closures: BTreeSet::new(),
            semantic_closures_proven: BTreeSet::new(),
            semantic_aggregate_classes: BTreeMap::new(),
            stored_synchronous_abis: BTreeSet::new(),
            partial_depth: limits.max_call_depth,
            helper_nodes: limits.max_nodes,
            helper_calls: limits.max_instructions,
            helper_edges: limits.max_analysis_steps,
            helper_depth: limits.max_call_depth,
            shared_product_capture: false,
            affine_packages: false,
            address_capture: false,
            owning_address_captures: false,
            mutable_capture_contexts: false,
            receiver_borrows: false,
            static_occurrences: false,
            qualified_head_returns: false,
            returned_borrowed_environments: BTreeSet::new(),
            pending_returned_source_proofs: BTreeSet::new(),
            callable_receiver_roles: BTreeMap::new(),
            replacement_functions: BTreeSet::new(),
            replacement_error_sums: false,
            address_capture_closures: BTreeSet::new(),
            ordinary_store_functions: Vec::new(),
            sum_functions: Vec::new(),
            addressed_sums: false,
            mixed_address_sums: false,
            borrowed_callback_inputs: false,
            ordinary_error_places: false,
            owned_callbacks: BTreeSet::new(),
            owned_callback_helpers: BTreeSet::new(),
            owned_callback_window_functions: Vec::new(),
            parameter_returns: BTreeSet::new(),
            forward_functions: Vec::new(),
            forward_closures: BTreeSet::new(),
            scoped_foreign: false,
            scoped_threads: false,
            generic_mutex: false,
            guard_return_sources: Vec::new(),
            borrowed_function_pointers: false,
            tasks: false,
            task_origin_roots: false,
            borrowed_task_ports: false,
            invocation_callbacks: false,
            head_descriptions: false,
            registrations: false,
            async_registrations: false,
            protocol_contracts: false,
            protocol_checked: false,
            stored_enum_addresses: false,
            task_borrowed_results: false,
            task_retained_environments: false,
            task_retained_roots: Vec::new(),
            task_callable_results: false,
            task_retained_sources: Vec::new(),
            dyn_contexts: false,
            pending_dyn_outputs: BTreeMap::new(),
            store_option_results: false,
            qualified_package_returns: false,
            array_outer_functions: Vec::new(),
            byte_queries: false,
            shared_byte_borrow: false,
            owned_byte_heads: None,
            byte_query_presence_checked: false,
            byte_query_functions: Vec::new(),
            package_return_calls: None,
            store_option_targets: None,
            store_option_byte_holds: BTreeSet::new(),
            description_values: Vec::new(),
            task_pointer_roots: Vec::new(),
            task_pointer_parents: Vec::new(),
            task_stored_coverage: Vec::new(),
            stored_borrows: false,
            pending_exclusive_helpers: BTreeSet::new(),
            stored_completed_functions: None,
            ordinary_input_head_candidates: false,
            stored_slot_updates: false,
            stored_exclusive_loans: false,
            composition_stored: Vec::new(),
            pure_nominal: false,
            capture_relations: false,
            stored_owned_fixed: false,
            stored_compound_values: false,
            stored_packages: false,
            task_failure_carriers: false,
            task_input_scopes: None,
            owned_task_input_inventory: false,
            stored_fixed_owner: false,
            owning_environment_captures: false,
            stored_projected_slots: false,
            shared_thread_functions: BTreeSet::new(),
            scoped_functions: BTreeSet::new(),
            scoped_helper_bodies: std::collections::BTreeMap::new(),
            sum_roots: BTreeSet::new(),
            ordinary_store_sum_captures: Vec::new(),
            ordinary_store_sum_addresses: BTreeSet::new(),
            ordinary_pod_store_roots: BTreeSet::new(),
            owned_store_package_roots: BTreeSet::new(),
        }
    }

    pub(super) fn has_ordinary_store_sum_capture(
        &mut self,
        id: zil_ir::ClosureId,
    ) -> CompileResult<bool> {
        if self.ordinary_store_sum_captures.is_empty() {
            return Ok(false);
        }
        self.spend(1, "ordinary Sum capture closure lookup")?;
        Ok(self
            .ordinary_store_sum_captures
            .get(id.0 as usize)
            .is_some_and(|roots| !roots.is_empty()))
    }

    pub(super) fn ordinary_store_sum_capture(
        &mut self,
        id: zil_ir::ClosureId,
        ty: &Type,
    ) -> CompileResult<bool> {
        let Some(roots) = self.ordinary_store_sum_captures.get(id.0 as usize) else {
            return Ok(false);
        };
        let len = roots.len();
        if len == 0 {
            return Ok(false);
        }
        self.lookup(len, "actual ordinary Store Sum capture root")?;
        Ok(self.ordinary_store_sum_captures[id.0 as usize].contains(&(ty as *const Type as usize)))
    }

    pub(super) fn ordinary_store_sum_address(&mut self, ty: &Type) -> CompileResult<bool> {
        if self.ordinary_store_sum_addresses.is_empty() {
            return Ok(false);
        }
        self.lookup(
            self.ordinary_store_sum_addresses.len(),
            "actual ordinary Store Sum address root",
        )?;
        Ok(self
            .ordinary_store_sum_addresses
            .contains(&(ty as *const Type as usize)))
    }

    pub(super) fn sum_type(&mut self, ty: &Type) -> CompileResult<bool> {
        if self.sum_roots.is_empty() {
            return Ok(false);
        }
        self.lookup(self.sum_roots.len(), "ordinary Sum type inventory")?;
        Ok(self.sum_roots.contains(&(ty as *const Type as usize)))
    }
    pub(super) fn ordinary_pod_store_type(&mut self, ty: &Type) -> CompileResult<bool> {
        if self.ordinary_pod_store_roots.is_empty() {
            return Ok(false);
        }
        self.lookup(
            self.ordinary_pod_store_roots.len(),
            "ordinary POD Store root lookup",
        )?;
        Ok(self
            .ordinary_pod_store_roots
            .contains(&(ty as *const Type as usize)))
    }
    pub(super) fn owned_store_package_type(&mut self, ty: &Type) -> CompileResult<bool> {
        if self.owned_store_package_roots.is_empty() {
            return Ok(false);
        }
        self.lookup(
            self.owned_store_package_roots.len(),
            "owned Store Package root lookup",
        )?;
        Ok(self
            .owned_store_package_roots
            .contains(&(ty as *const Type as usize)))
    }
    pub(super) fn remaining(&self) -> usize {
        self.remaining
    }

    pub(super) fn has_owned_store_package(&self) -> bool {
        !self.owned_store_package_roots.is_empty()
    }

    pub(super) fn addressed_copy_sum(&mut self, ty: &Type) -> CompileResult<bool> {
        self.lookup(
            self.addressed_copy_sums.len(),
            "addressed Sum Copy memo lookup",
        )?;
        let key = ty as *const Type as usize;
        if let Some(copy) = self.addressed_copy_sums.get(&key) {
            return Ok(*copy);
        }
        self.type_work(ty, true, "mutable local Enum tag knowledge")?;
        let copy = ty.is_copy_value();
        self.spend(1, "addressed Sum Copy memo insertion")?;
        self.addressed_copy_sums.insert(key, copy);
        Ok(copy)
    }

    pub(super) fn spend(&mut self, amount: usize, context: &str) -> CompileResult<()> {
        self.remaining = self
            .remaining
            .checked_sub(amount)
            .ok_or_else(|| exhausted(context))?;
        Ok(())
    }

    pub(super) fn product(
        &mut self,
        left: usize,
        right: usize,
        multiplier: usize,
        context: &str,
    ) -> CompileResult<()> {
        let amount = left
            .checked_mul(right)
            .and_then(|n| n.checked_mul(multiplier))
            .ok_or_else(|| exhausted(context))?;
        self.spend(amount, context)
    }

    pub(super) fn sets(
        &mut self,
        left: &BTreeSet<LocalId>,
        right: &BTreeSet<LocalId>,
        context: &str,
    ) -> CompileResult<()> {
        self.spend(left.len(), context)?;
        self.spend(right.len(), context)
    }

    pub(super) fn lookup(&mut self, size: usize, context: &str) -> CompileResult<()> {
        // A bounded tree lookup/insertion visits logarithmically many levels.
        self.spend(size.checked_ilog2().map_or(1, |n| n as usize + 2), context)
    }

    pub(super) fn type_work(
        &mut self,
        ty: &Type,
        copy_check: bool,
        context: &str,
    ) -> CompileResult<()> {
        // Input preflight has already bounded nesting. Reserve this walk, one
        // exact type comparison and, when requested, one Copy classification.
        // A mismatch can stop the comparison early; an equal declaration is
        // the upper bound. Arrays compare their length once, not per element.
        self.spend(2 + usize::from(copy_check), context)?;
        super::tasks::charge_result_context(ty, self)?;
        super::tasks::require_closed_context(ty)?;
        match ty {
            Type::Dyn { interface } | Type::OwnedDyn { interface } => {
                self.lookup(self.dyn_interfaces.len(), "dyn type interface lookup")?;
                if !self.dyn_interfaces.contains(interface) {
                    return Err(bad("dyn type lacks a checked interface"));
                }
            }
            Type::BorrowedStorage { storage, life } => {
                self.spend(
                    life.slots
                        .len()
                        .saturating_add(life.retained.len())
                        .saturating_add(life.nominal_arguments.len())
                        .saturating_add(1),
                    context,
                )?;
                for row in &life.slots {
                    self.spend(row.path.len(), context)?;
                }
                for row in &life.retained {
                    self.spend(row.path.len(), context)?;
                }
                for row in &life.nominal_arguments {
                    self.spend(row.path.len(), context)?;
                }
                self.type_work(storage, copy_check, context)?;
            }
            Type::Tuple(fields) => {
                for field in fields {
                    self.type_work(field, copy_check, context)?;
                }
            }
            Type::Record { name, fields } | Type::Package { name, fields } => {
                self.spend(name.len(), context)?;
                for (name, field) in fields {
                    self.spend(name.len(), context)?;
                    self.type_work(field, copy_check, context)?;
                }
            }
            Type::Enum { name, variants } => {
                self.spend(name.len(), context)?;
                for (name, fields) in variants {
                    self.spend(2 + usize::from(copy_check), context)?;
                    self.spend(name.len(), context)?;
                    for field in fields {
                        self.type_work(field, copy_check, context)?;
                    }
                }
            }
            Type::Array { element, .. }
            | Type::Slice { element }
            | Type::OwnedStore { element }
            | Type::Address { inner: element, .. }
            | Type::Managed(element)
            | Type::Desc(element)
            | Type::Opaque {
                hidden: element, ..
            } => self.type_work(element, copy_check, context)?,
            Type::ErrorUnion { error, result } => {
                self.type_work(error, copy_check, context)?;
                self.type_work(result, copy_check, context)?;
            }
            Type::BorrowedFunctionPointer {
                life,
                parameters,
                result,
            } => {
                crate::signature_life::charge(life, self)?;
                for parameter in parameters {
                    self.type_work(parameter, copy_check, context)?;
                }
                self.type_work(result, copy_check, context)?;
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                for parameter in parameters {
                    self.type_work(parameter, copy_check, context)?;
                }
                self.type_work(result, copy_check, context)?;
            }
            Type::Named(name) | Type::Unsupported(name) => {
                self.spend(name.len(), context)?;
            }
            Type::Registration { environment } | Type::AsyncRegistration { environment } => {
                self.registrations = true;
                self.type_work(environment, copy_check, context)?;
            }
            Type::Task { result, .. } => {
                self.tasks = true;
                self.type_work(result, copy_check, context)?;
            }
            Type::TaskI32 => {
                self.tasks = true;
            }
            Type::Mutex { payload } => {
                self.generic_mutex = true;
                self.type_work(payload, copy_check, context)?;
            }
            Type::MutexGuard {
                payload,
                retained_life,
            } => {
                self.generic_mutex = true;
                self.spend(
                    retained_life
                        .ports
                        .len()
                        .saturating_add(retained_life.retained.len())
                        .saturating_add(retained_life.nominal_arguments.len())
                        .saturating_add(1),
                    context,
                )?;
                self.type_work(payload, copy_check, context)?;
            }
            Type::ByteBuffer
            | Type::SyncCell(_)
            | Type::Unit
            | Type::Bool
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::I32
            | Type::I8
            | Type::I16
            | Type::Isize
            | Type::I64
            | Type::F32
            | Type::F64
            | Type::U64
            | Type::Usize
            | Type::ForeignHandle
            | Type::CWideString
            | Type::Closure(_) => {}
        }
        Ok(())
    }

    fn input(&mut self, amount: usize, context: &str) -> CompileResult<()> {
        // Reserve both the preflight visit and the later encoding/hash visit.
        self.product(amount, 2, 1, context)
    }
}

fn count(before: usize, add: usize, maximum: usize, name: &str) -> CompileResult<usize> {
    match before.checked_add(add) {
        Some(total) if total <= maximum => Ok(total),
        _ => Err(exhausted(&format!("module {name} count"))),
    }
}

fn scalar(ty: &Type, span: Span) -> CompileResult<()> {
    if ty.is_copy_value() {
        Ok(())
    } else {
        // Do not Debug-format or recursively inspect an unsupported raw type.
        Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "non-Copy aggregate, address, opaque, and resource analysis is not implemented",
            Some(span),
        )])
    }
}

#[derive(Clone, Copy, Default)]
struct SumContext {
    blocked: bool,
    in_enum: bool,
    in_tuple: bool,
    in_product: bool,
    in_error: bool,
    error_products: u8,
    product_sum: bool,
    // Presence only, carried through wrappers during the paid type walk.
    pod_aggregate: bool,
    // Set only at the actual Managed<OwnedStore<POD>> edge of an ordinary
    // owning aggregate. Array, element and resource-head paths cannot set it.
    ordinary_pod_store: bool,
    forbidden_store: bool,
}

fn package_fields_supported(ty: &Type, arrays: bool) -> bool {
    matches!(ty, Type::Package { fields, .. }
    if fields.iter().any(|(_, t)| matches!(t, Type::Managed(_)))
    && fields.iter().all(|(_, t)| {
        t.is_copy_value()
            || (arrays && matches!(t, Type::Closure(_)))
            || matches!(t, Type::Managed(p) if **p == Type::ByteBuffer
                || (arrays && matches!(p.as_ref(), Type::OwnedStore { .. }))
                || (matches!(p.as_ref(), Type::Record { .. }) && p.is_copy_value()))
            || (arrays && matches!(t, Type::Array { element, length }
                if *length > 0 && package_fields_supported(element, false)))
    }))
}

pub(super) fn legacy_value(ty: &Type, span: Span, module: &TypedModule) -> CompileResult<()> {
    // Complete qualified shapes and exact inventory were checked during the
    // bounded raw preflight. Actual stored permissions use their own CFG pass.
    if matches!(ty, Type::BorrowedStorage { .. })
        || matches!(ty,Type::Address{inner,..}if matches!(inner.as_ref(),Type::BorrowedStorage{..}))
    {
        return Ok(());
    }
    if matches!(ty, Type::Address { inner, .. }
        if super::resources::product::nested(inner))
    {
        return Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "ordinary Package Product does not grant a borrowed Package boundary",
            Some(span),
        )]);
    }
    if let Type::ErrorUnion { error, result } = ty {
        if matches!(result.as_ref(), Type::Managed(_)) {
            scalar(error, span)?;
            return legacy_value(result, span, module);
        }
        for payload in [error.as_ref(), result.as_ref()] {
            if matches!(payload, Type::Package { .. }) {
                legacy_value(payload, span, module)?;
            } else {
                scalar(payload, span)?;
            }
        }
        Ok(())
    } else if let Type::Array { element, length } = ty
        && *length > 0
        && matches!(element.as_ref(), Type::Package { .. } | Type::Closure(_))
    {
        legacy_value(element, span, module)
    } else if let Type::Closure(id) = ty {
        if id.0 as usize >= module.closures.len() {
            return Err(bad("closure type references an absent environment"));
        }
        Ok(())
    } else if let Type::Address { inner, .. } = ty
        && let Type::Closure(id) = inner.as_ref()
    {
        if id.0 as usize >= module.closures.len() {
            return Err(bad("borrowed environment references an absent closure"));
        }
        Ok(())
    } else if matches!(ty, Type::Package { .. }) {
        if package_fields_supported(ty, true)
            || matches!(ty, Type::Package { name, fields }
                if !name.is_empty() && !fields.is_empty()
                    && fields.iter().all(|(name, field)| !name.is_empty() && field.is_copy_value()))
        {
            Ok(())
        } else {
            Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "package requires finite Copy/complete array fields and at least one explicit Managed resource",
                Some(span),
            )])
        }
    } else if crate::dynamic_store::is_carrier(ty)
        || matches!(ty, Type::MutexGuard { .. })
        || matches!(ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::MutexGuard { .. }))
        || matches!(ty, Type::Managed(inner) | Type::Desc(inner) if matches!(inner.as_ref(), Type::Mutex { .. }))
        || ty.is_shared_sync_cell_address()
        || ty.is_exclusive_task_head_address()
        || matches!(ty, Type::Address { inner, .. } if inner.is_address_pointee())
        || crate::resources::head_address(ty)
        || ty.is_package_address()
        || matches!(ty, Type::Managed(inner) | Type::Desc(inner)
            if **inner == Type::ByteBuffer || matches!(inner.as_ref(),Type::TaskI32|Type::Task{..}) || matches!(inner.as_ref(), Type::Registration { .. } | Type::AsyncRegistration { .. })
                || (matches!(inner.as_ref(), Type::Record { .. }) && inner.is_copy_value()))
    {
        Ok(())
    } else {
        scalar(ty, span)
    }
}

// Ordinary Products never widen the old array, union or capture profile.
fn local_value(
    ty: &Type,
    span: Span,
    module: &TypedModule,
    budget: &mut Budget,
) -> CompileResult<bool> {
    if super::stored_flow::shared_array_shape(ty, budget)?
        || matches!(ty, Type::Address { inner, .. } if super::stored_flow::shared_array_shape(inner, budget)?)
    {
        return Ok(false);
    }
    if let Type::Address { inner, .. } = ty
        && matches!(inner.as_ref(), Type::OwnedDyn { .. })
    {
        return local_value(inner, span, module, budget);
    }
    if let Type::OwnedDyn { interface } = ty {
        budget.lookup(budget.dyn_interfaces.len(), "owned dyn interface")?;
        if !budget.dyn_interfaces.contains(interface) {
            return Err(bad("owned dyn lacks an admitted interface"));
        }
        budget.spend(module.dyn_interfaces.len(), "owned dyn cleanup contract")?;
        let found = module
            .dyn_interfaces
            .iter()
            .find(|i| i.id == *interface)
            .ok_or_else(|| bad("owned dyn lacks an actual interface"))?;
        crate::dyn_cleanup::bound(found, budget)?;
        return Ok(false);
    }
    if let Type::ErrorUnion { error, result } = ty
        && matches!(result.as_ref(), Type::OwnedDyn { .. })
    {
        if *error.as_ref() != crate::dyn_dispatch::create_error() {
            return Err(bad("owned dyn creation has a forged error type"));
        }
        return local_value(result, span, module, budget);
    }
    if let Type::Address { inner, .. } = ty
        && matches!(inner.as_ref(), Type::Dyn { .. })
    {
        return Ok(false);
    }
    if matches!(ty, Type::BorrowedStorage { .. }) {
        // The paid structural preflight above already visits every actual
        // return, local, and SSA type and qualifies each stored boundary.
        return if budget.stored_borrows {
            if budget.sum_type(ty)? {
                super::resources::sums::admit(module, ty, budget)
            } else {
                Ok(false)
            }
        } else {
            Err(bad("stored nominal type lacks its instance inventory"))
        };
    }
    if budget.stored_enum_addresses
        && matches!(ty, Type::ErrorUnion { .. })
        && super::stored_flow::enum_address::signature(ty)
    {
        super::stored_flow::enum_address::ordinary(module, ty, budget)?;
        return Ok(false);
    }
    if let Type::Address { inner, .. } = ty
        && matches!(
            inner.as_ref(),
            Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }
        )
    {
        return super::resources::sums::admit(module, inner, budget).map(|_| false);
    }
    if super::nested_store::protocol_payload(ty).is_some() {
        super::nested_store::protocol(module, ty, budget)?;
        return super::resources::sums::admit(module, ty, budget);
    }
    if budget.task_failure_carriers
        && let Type::ErrorUnion { error, result } = ty
        && matches!(result.as_ref(),Type::Managed(task) if matches!(task.as_ref(),Type::Task{..}))
        && matches!(error.as_ref(),Type::BorrowedStorage{storage,..} if matches!(storage.as_ref(),Type::Record{name,..} if name.starts_with("@std.task:v1:TaskCreateFailure:")))
    {
        super::stored_flow::physical(error, budget)?;
        return legacy_value(result, span, module).map(|_| false);
    }
    if budget.sum_type(ty)? {
        return super::resources::sums::admit(module, ty, budget);
    }
    if let Type::Address { inner, .. } = ty
        && (super::resources::product::nested(inner)
            || (matches!(inner.as_ref(), Type::Package { .. })
                && budget.owned_store_package_type(inner)?))
    {
        return if budget.ordinary_pod_store_type(inner)?
            || budget.owned_store_package_type(inner)?
        {
            super::resources::product::admit_owned(module, inner, budget)
        } else {
            super::resources::product::admit_borrowed(inner, budget).map(|_| false)
        };
    }
    if super::resources::product::nested(ty)
        || (matches!(ty, Type::Package { .. })
            && (budget.ordinary_pod_store_type(ty)? || budget.owned_store_package_type(ty)?))
    {
        super::resources::product::admit_owned(module, ty, budget)
    } else if let Some(payload) = super::dynamic_store::product_protocol_payload(ty) {
        super::dynamic_store::product_element(payload, budget).map(|_| false)
    } else if super::stored_flow::shared_compound(module, ty, budget)?
        || super::stored_flow::affine_compound(module, ty, budget)?
        || super::stored_flow::pure_inline(module, ty, budget)?
    {
        Ok(false)
    } else if let Type::Address {
        inner,
        mutable: false,
    } = ty
        && super::stored_flow::pure_inline(module, inner, budget)?
    {
        Ok(false)
    } else {
        legacy_value(ty, span, module).map(|_| false)
    }
}

fn check_anonymous_sum(ty: &Type, limits: &Limits, budget: &mut Budget) -> CompileResult<()> {
    let Type::Enum { name, variants } = ty else {
        unreachable!()
    };
    if name != "@anonymous-sum:v1" || variants.len() < 2 {
        return Err(bad("invalid anonymous sum family or arity"));
    }
    let mut identity_work = 0usize;
    let mut pending = Vec::new();
    for (index, (label, fields)) in variants.iter().enumerate() {
        budget.spend(32, "anonymous canonical label")?;
        if label != &format!("Member{index}") || fields.len() != 1 {
            return Err(bad(
                "anonymous sum requires canonical ordinal labels and one payload per member",
            ));
        }
        if matches!(&fields[0], Type::Enum { name, .. } if name == "@anonymous-sum:v1") {
            return Err(bad("anonymous sum directly nests another anonymous sum"));
        }
        pending.push((&fields[0], 0usize));
    }
    while let Some((ty, depth)) = pending.pop() {
        budget.spend(1, "anonymous identity node")?;
        identity_work = identity_work.saturating_add(1);
        if depth > limits.max_call_depth {
            return Err(exhausted("anonymous identity nesting"));
        }
        let text = match ty {
            Type::Record { name, fields } | Type::Package { name, fields } => {
                for (label, field) in fields {
                    budget.spend(label.len(), "anonymous identity label")?;
                    identity_work = identity_work.saturating_add(label.len());
                    pending.push((field, depth + 1));
                }
                name.len()
            }
            Type::Enum { name, variants } => {
                for (label, fields) in variants {
                    budget.spend(label.len(), "anonymous identity label")?;
                    identity_work = identity_work.saturating_add(label.len());
                    pending.extend(fields.iter().map(|field| (field, depth + 1)));
                }
                name.len()
            }
            Type::Tuple(fields) => {
                pending.extend(fields.iter().map(|field| (field, depth + 1)));
                0
            }
            Type::Array { element, .. }
            | Type::Slice { element }
            | Type::OwnedStore { element }
            | Type::Address { inner: element, .. }
            | Type::Managed(element)
            | Type::Desc(element) => {
                pending.push((element, depth + 1));
                0
            }
            Type::ErrorUnion { error, result } => {
                pending.push((error, depth + 1));
                pending.push((result, depth + 1));
                0
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                pending.extend(parameters.iter().map(|field| (field, depth + 1)));
                pending.push((result, depth + 1));
                0
            }
            Type::Named(_) | Type::Unsupported(_) | Type::Opaque { .. } | Type::Closure(_) => {
                return Err(bad(
                    "anonymous sum identity contains an unresolved or nonordinary member type",
                ));
            }
            _ => 0,
        };
        budget.spend(text, "anonymous identity text")?;
        identity_work = identity_work.saturating_add(text);
    }
    budget.spend(
        identity_work.saturating_mul(2),
        "anonymous canonical comparison",
    )?;
    if variants.windows(2).any(|pair| pair[0].1[0] >= pair[1].1[0]) {
        return Err(bad(
            "anonymous sum members are repeated or not in complete-type canonical order",
        ));
    }
    Ok(())
}

pub(super) fn preflight(
    module: &TypedModule,
    limits: &Limits,
    budget: &mut Budget,
) -> CompileResult<(Vec<bool>, Vec<bool>)> {
    if module.schema_version != SCHEMA_VERSION {
        return Err(bad("unknown typed module schema version"));
    }
    super::dyn_dispatch::inventory(module, budget)?;
    if !module.life_declarations.is_empty() || !module.life_instances.is_empty() {
        super::stored_flow::inventory(module, limits, budget)?;
        // The second walk pays identity encoding from this same ledger.
        super::stored_flow::inventory(module, limits, budget)?;
        budget.stored_borrows = true;
    }
    count(0, module.functions.len(), limits.max_functions, "function")?;
    budget.input(1, "module preflight")?;
    if let Some(feature) = module.unsupported.first() {
        budget.spend(feature.feature.len(), "unsupported feature metadata")?;
        return Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            &feature.feature,
            feature.span,
        )]);
    }
    for text in [
        &module.target.id,
        &module.target.triple,
        &module.target.backend,
        &module.target.rule_version,
    ] {
        // Target text appears in both the module and target fingerprints.
        budget.product(text.len(), 3, 1, "target metadata")?;
    }
    count(0, module.resources.len(), limits.max_nodes, "resource")?;
    budget.input(module.resources.len(), "resource table")?;
    for resource in &module.resources {
        budget.input(resource.name.len().saturating_add(1), "resource metadata")?;
        if resource.name.starts_with("@builtin:Mutex<") {
            budget.generic_mutex = true;
        }
    }
    // Carry the outermost Store element through the already paid type walk.
    // Only a real Closure occurrence allocates a new admission entry; old
    // noncallback Store paths receive no new traversal or logical charge.
    let mut callback_store_elements = Vec::new();
    let mut product_store_elements = Vec::new();
    let mut pod_store_elements = Vec::new();
    let mut owned_store_capture_roots = Vec::new();
    for (ty, store_element, capture_owner, resource_head) in module
        .functions
        .iter()
        .flat_map(|f| {
            std::iter::once(&f.return_type)
                .chain(f.locals.iter().map(|l| &l.ty))
                .chain(f.values.iter())
                .map(|ty| (ty, None, None, false))
                .chain(
                    f.blocks
                        .iter()
                        .flat_map(|b| b.instructions.iter())
                        .filter_map(|i| match &i.kind {
                            InstructionKind::ConvertInteger { target, .. }
                            | InstructionKind::ConvertNumber { target, .. } => {
                                Some((target, None, None, false))
                            }
                            InstructionKind::OwnedStoreNew { element } => {
                                Some((element, Some(element), None, false))
                            }
                            _ => None,
                        }),
                )
        })
        .chain(
            module
                .resources
                .iter()
                .map(|r| (&r.payload, None, None, true)),
        )
        .chain(module.closures.iter().enumerate().flat_map(|(index, c)| {
            c.captures
                .iter()
                .map(move |ty| (ty, None, Some(zil_ir::ClosureId(index as u32)), false))
                .chain(
                    c.parameters
                        .iter()
                        .chain(std::iter::once(&c.result))
                        .map(|ty| (ty, None, None, false)),
                )
        }))
    {
        if let Some(element) = store_element
            && !matches!(element, Type::Package { .. })
        {
            budget.spend(1, "POD Store instruction admission entry")?;
            pod_store_elements.push(element);
        }
        if let Some(element) = store_element
            && (super::resources::product::nested(element)
                || matches!(element, Type::Package { .. })
                    && super::nested_store::selected(element))
        {
            budget.spend(1, "Product Store instruction admission entry")?;
            product_store_elements.push(element);
        }
        // Reuse the already paid structural visit to classify new ordinary
        // positions. No extra recursive eligibility scan runs on legacy types.
        let root = ty as *const Type as usize;
        let dyn_carrier =
            matches!(ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Dyn { .. }));
        // The old Store protocol already admitted Unit!ordinary Product with
        // no Store/Sum child; retain that exact profile and fee path.
        let sum_address_root = matches!(ty,Type::Address{inner,..}if matches!(inner.as_ref(),Type::Tuple(_)|Type::Enum{..}|Type::ErrorUnion{..}))
            && store_element.is_none()
            && !resource_head;
        let legacy_product_union = matches!(ty,Type::ErrorUnion { error, result }
            if **error == Type::Unit || **result == Type::Unit);
        let mut anonymous = Vec::new();
        let mut new_sum = false;
        let mut new_ordinary_pod_store = false;
        let mut contains_owned_store_package_field = false;
        // Flat Package values with Store fields retain the old complete-
        // Package route, including callback fields. Borrowed Package
        // receivers still need the new owned-Store window admission, except
        // for their old direct-callback profile. Derive both distinctions
        // during this already charged raw type walk.
        let mut new_product_shape = false;
        let address_package = matches!(ty, Type::Address { inner, .. }
            if matches!(inner.as_ref(), Type::Package { .. }));
        let mut direct_address_callback = false;
        let mut first = Some((
            ty,
            0usize,
            store_element,
            capture_owner,
            resource_head,
            SumContext::default(),
        ));
        let mut pending = Vec::new();
        while let Some((ty, depth, store_element, capture_owner, resource_head, context)) =
            first.take().or_else(|| pending.pop())
        {
            budget.input(1, "type shape")?;
            budget.affine_packages |= matches!(ty, Type::Package { name, fields }
                if !name.is_empty() && !fields.is_empty());
            budget.task_origin_roots |= matches!(ty, Type::Task { .. });
            budget.borrowed_task_ports |=
                matches!(ty, Type::Task { retained_life, .. } if !retained_life.ports.is_empty());
            super::tasks::charge_result_context(ty, budget)?;
            super::tasks::require_closed_context(ty)?;
            budget.task_borrowed_results |= matches!(ty, Type::Task { result, .. } if matches!(result.as_ref(), Type::Address { mutable: false, .. }));
            budget.store_option_results |= matches!(ty, Type::BorrowedStorage { storage, .. }
                if matches!(storage.as_ref(), Type::Enum { variants, .. }
                    if matches!(variants.as_slice(), [(_, none), (_, some)]
                        if none.is_empty() && matches!(some.as_slice(), [Type::Address { mutable: false, inner }]
                            if matches!(inner.as_ref(), Type::Package { .. })))));
            if depth > limits.max_call_depth {
                return Err(exhausted("aggregate nesting"));
            }
            if let Some(owner) = capture_owner
                && super::dynamic_store::is_owned_carrier(ty)
            {
                budget.spend(
                    owned_store_capture_roots.len().saturating_add(1),
                    "owned Store capture root entry",
                )?;
                owned_store_capture_roots.push(owner);
            }
            let mut child = context;
            match ty {
                Type::BorrowedStorage { storage, life }
                    if context.in_tuple || matches!(storage.as_ref(), Type::Tuple(_)) =>
                {
                    budget.input(life.slots.len(), "affine compound exclusive slots")?;
                    new_sum |= life
                        .slots
                        .iter()
                        .any(|slot| slot.mode == zil_ir::StoredBorrowMode::ExclusiveAddress);
                }
                Type::Enum { .. } => {
                    child.in_enum = true;
                    child.pod_aggregate = true;
                }
                Type::Tuple(_) => {
                    child.in_tuple = true;
                    child.pod_aggregate = true;
                }
                Type::Package { .. } => {
                    new_product_shape |= context.in_product && store_element.is_none();
                    child.in_product = true;
                    child.pod_aggregate = true;
                    if context.in_error {
                        child.error_products = context.error_products.saturating_add(1);
                    }
                }
                Type::ErrorUnion { .. } => {
                    new_sum |= super::nested_store::protocol_payload(ty).is_some();
                    child.in_error = true;
                    child.pod_aggregate = true;
                    child.product_sum |= context.in_product;
                }
                Type::Record { .. } => {
                    child.forbidden_store = true;
                }
                Type::Managed(payload) => {
                    contains_owned_store_package_field |= context.in_product
                        && !context.in_tuple
                        && !context.in_enum
                        && !context.in_error
                        && matches!(payload.as_ref(), Type::OwnedStore { .. });
                    let ordinary_pod_store = context.pod_aggregate
                        && !context.blocked
                        && !context.forbidden_store
                        && store_element.is_none()
                        && !resource_head
                        && matches!(payload.as_ref(), Type::OwnedStore { element }
                            if !matches!(element.as_ref(), Type::Package { .. }));
                    let extension = context.in_enum
                        || context.in_tuple
                        || context.product_sum
                        || (context.error_products >= 2
                            && (!legacy_product_union
                                || super::dynamic_store::is_owned_carrier(ty)));
                    if extension
                        && !ordinary_pod_store
                        && (context.blocked
                            || store_element.is_some()
                            || capture_owner.is_some()
                            || resource_head)
                    {
                        return Err(vec![Diagnostic::unsupported(
                            Phase::Analyze,
                            "ordinary owned Sum does not grant array, StoreElement, capture, borrowed or resource-head qualification",
                            None,
                        )]);
                    }
                    new_sum |= extension || (context.in_error && ordinary_pod_store);
                    child = SumContext {
                        blocked: true,
                        pod_aggregate: context.pod_aggregate,
                        ordinary_pod_store,
                        forbidden_store: context.forbidden_store,
                        ..SumContext::default()
                    };
                }
                Type::Address { .. } if depth == 0 && sum_address_root => {
                    child = SumContext {
                        pod_aggregate: context.pod_aggregate,
                        ..SumContext::default()
                    };
                }
                Type::Address { inner, .. }
                    if depth == 0
                        && matches!(inner.as_ref(), Type::Package { .. })
                        && store_element.is_none()
                        && capture_owner.is_none()
                        && !resource_head =>
                {
                    // A concrete borrowed Package receiver still contains its
                    // actual owning fields. The address itself grants no Store
                    // ownership; field admission and live-loan checks run on
                    // the complete underlying Package and its real source.
                    child = SumContext::default();
                }
                Type::Array { .. }
                | Type::OwnedStore { .. }
                | Type::Address { .. }
                | Type::Desc(_)
                | Type::Opaque { .. }
                | Type::FunctionPointer { .. }
                | Type::BorrowedFunctionPointer { .. }
                | Type::ForeignCallback { .. } => {
                    child = SumContext {
                        blocked: true,
                        pod_aggregate: context.pod_aggregate,
                        forbidden_store: context.forbidden_store
                            || matches!(ty, Type::Array { .. }),
                        ..SumContext::default()
                    }
                }
                _ => {}
            }
            if super::stored_flow::shared_array_value(ty) {
                budget.stored_borrows = true;
            }
            match ty {
                Type::BorrowedStorage { storage, .. } => {
                    budget.stored_compound_values |=
                        context.in_tuple || matches!(storage.as_ref(), Type::Tuple(_));
                    new_product_shape |= context.in_product && store_element.is_none();
                    super::stored_flow::qualification(module, ty, limits, budget)?;
                    budget.stored_borrows = true;
                    pending.push((
                        storage,
                        depth + 1,
                        store_element,
                        capture_owner,
                        resource_head,
                        child,
                    ));
                }
                Type::Dyn { interface } => {
                    budget.lookup(budget.dyn_interfaces.len(), "dyn type interface")?;
                    if !dyn_carrier || depth != 1 || !budget.dyn_interfaces.contains(interface) {
                        return Err(bad(
                            "dyn type requires an exact admitted shared address carrier",
                        ));
                    }
                }
                Type::Array { element, length } => {
                    budget.input(*length as usize, "array type size")?;
                    pending.push((
                        element,
                        depth + 1,
                        store_element,
                        capture_owner,
                        resource_head,
                        child,
                    ));
                }
                Type::Task {
                    result: element, ..
                } => {
                    budget.input(1, "actual Task result metadata")?;
                    pending.push((
                        element,
                        depth + 1,
                        store_element,
                        None,
                        false,
                        SumContext::default(),
                    ));
                }
                Type::Slice { element } => {
                    pending.push((
                        element,
                        depth + 1,
                        store_element,
                        None,
                        resource_head,
                        child,
                    ));
                }
                Type::OwnedStore { element } => {
                    if !matches!(element.as_ref(), Type::Package { .. }) {
                        if context.forbidden_store {
                            return Err(vec![Diagnostic::unsupported(
                                Phase::Analyze,
                                "ordinary POD Store ownership does not qualify Record or Array children",
                                None,
                            )]);
                        }
                        if context.pod_aggregate
                            && !context.ordinary_pod_store
                            && !store_element.is_some_and(super::nested_store::selected)
                        {
                            return Err(vec![Diagnostic::unsupported(
                                Phase::Analyze,
                                "POD Store admission does not grant owning aggregate field or payload qualification",
                                None,
                            )]);
                        }
                        new_ordinary_pod_store |= context.ordinary_pod_store;
                        if resource_head && depth != 0 {
                            return Err(vec![Diagnostic::unsupported(
                                Phase::Analyze,
                                "POD Store admission does not grant user resource-head qualification",
                                None,
                            )]);
                        }
                        budget.spend(1, "POD Store type admission entry")?;
                        pod_store_elements.push(element.as_ref());
                    }
                    if super::resources::product::nested(element)
                        || matches!(element.as_ref(), Type::Package { .. })
                            && super::nested_store::selected(element)
                    {
                        budget.spend(1, "Product Store type admission entry")?;
                        product_store_elements.push(element);
                    }
                    pending.push((
                        element,
                        depth + 1,
                        store_element.or(Some(element)),
                        None,
                        // Only the top-level sealed Store definition owns an
                        // ordinary element. A Store hidden in a user resource
                        // head cannot erase that head's closed Copy profile.
                        resource_head && depth != 0,
                        child,
                    ));
                }
                Type::Tuple(fields) => {
                    budget.input(fields.len(), "aggregate type shape")?;
                    for t in fields {
                        pending.push((
                            t,
                            depth + 1,
                            store_element,
                            capture_owner,
                            resource_head,
                            child,
                        ));
                    }
                }
                Type::Record { name, fields } | Type::Package { name, fields } => {
                    budget.input(name.len(), "record identity")?;
                    budget.input(fields.len(), "record fields")?;
                    for (name, t) in fields {
                        budget.input(name.len(), "field name")?;
                        if address_package
                            && depth == 1
                            && matches!(ty, Type::Package { .. })
                            && matches!(t, Type::Closure(_))
                        {
                            direct_address_callback = true;
                        }
                        // Only ordinary value positions acquire the new Product
                        // profile. Resource heads remain finite Copy, including
                        // unused metadata. Reuse the paid field visit; the old
                        // flat profile has no new table or logical charge.
                        if resource_head
                            && matches!(ty, Type::Package { .. })
                            && matches!(t, Type::Package { .. })
                        {
                            return Err(vec![Diagnostic::unsupported(
                                Phase::Analyze,
                                "ordinary Package Product is not a resource payload profile",
                                None,
                            )]);
                        }
                        pending.push((
                            t,
                            depth + 1,
                            store_element,
                            capture_owner,
                            resource_head,
                            child,
                        ));
                    }
                }
                Type::Enum { name, variants } => {
                    if name.starts_with("@anonymous-sum") {
                        budget.spend(1, "anonymous shape inventory")?;
                        anonymous.push(ty);
                    }
                    budget.input(
                        name.len().saturating_add(variants.len()),
                        "enum identity and variants",
                    )?;
                    for (name, fields) in variants {
                        budget.input(name.len().saturating_add(fields.len()), "enum fields")?;
                        for ty in fields {
                            pending.push((
                                ty,
                                depth + 1,
                                store_element,
                                capture_owner,
                                resource_head,
                                child,
                            ));
                        }
                    }
                }
                Type::Registration { environment: inner }
                | Type::AsyncRegistration { environment: inner } => {
                    budget.async_registrations |= matches!(ty, Type::AsyncRegistration { .. });
                    budget.registrations = true;
                    pending.push((inner, depth + 1, store_element, None, resource_head, child));
                }
                Type::Managed(inner) => pending.push((
                    inner,
                    depth + 1,
                    store_element,
                    capture_owner,
                    resource_head,
                    child,
                )),
                Type::Address { inner, .. }
                | Type::Desc(inner)
                | Type::Opaque { hidden: inner, .. } => {
                    pending.push((inner, depth + 1, store_element, None, resource_head, child))
                }
                Type::BorrowedFunctionPointer {
                    parameters,
                    result,
                    life,
                } => {
                    budget.borrowed_function_pointers = true;
                    crate::signature_life::charge(life, budget)?;
                    // Bound the complete raw signature before recursive eligibility.
                    for ty in parameters.iter().chain(std::iter::once(result.as_ref())) {
                        super::stored_flow::shape_work(ty, limits, budget)?;
                    }
                    super::borrowed_fp::signature_work(parameters, result, budget)?;
                    super::borrowed_fp::relation(parameters, result, life)?;
                    pending.push((result, depth + 1, store_element, None, false, child));
                    for parameter in parameters {
                        pending.push((parameter, depth + 1, store_element, None, false, child));
                    }
                }
                Type::FunctionPointer { parameters, result }
                | Type::ForeignCallback { parameters, result } => {
                    budget.input(parameters.len(), "function pointer signature")?;
                    pending.push((result, depth + 1, store_element, None, false, child));
                    for parameter in parameters {
                        pending.push((parameter, depth + 1, store_element, None, false, child));
                    }
                }
                Type::ErrorUnion { error, result } => {
                    pending.push((
                        error,
                        depth + 1,
                        store_element,
                        capture_owner,
                        resource_head,
                        child,
                    ));
                    pending.push((
                        result,
                        depth + 1,
                        store_element,
                        capture_owner,
                        resource_head,
                        child,
                    ));
                }
                Type::Closure(_) if store_element.is_some() => {
                    budget.spend(1, "callback Store admission entry")?;
                    callback_store_elements.push(store_element.unwrap());
                }
                _ => {}
            }
        }
        for ty in anonymous {
            check_anonymous_sum(ty, limits, budget)?;
        }
        // A Task's result describes its separate frame/result storage. Its
        // nested owning Sum must not turn a Task head or payload metadata into
        // an ordinary materialized Sum root.
        if new_sum
            && (matches!(
                ty,
                Type::Package { .. }
                    | Type::Tuple(_)
                    | Type::Enum { .. }
                    | Type::ErrorUnion { .. }
                    | Type::BorrowedStorage { .. }
            ) || matches!(ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Package { .. } | Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. })))
        {
            budget.lookup(budget.sum_roots.len(), "ordinary Sum root insertion")?;
            budget.spend(1, "ordinary Sum root allocation")?;
            budget.sum_roots.insert(root);
        }
        if new_sum
            && new_ordinary_pod_store
            && matches!(ty, Type::Address { inner, .. } if matches!(inner.as_ref(),Type::Tuple(_)|Type::Enum{..}|Type::ErrorUnion{..}|Type::Package{..}))
        {
            budget.lookup(
                budget.ordinary_store_sum_addresses.len(),
                "ordinary Store Sum address insertion",
            )?;
            budget.spend(1, "ordinary Store Sum address root allocation")?;
            budget.ordinary_store_sum_addresses.insert(root);
        }
        if new_sum
            && new_ordinary_pod_store
            && let Some(owner) = capture_owner
            && matches!(
                ty,
                Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. } | Type::Package { .. }
            )
        {
            if budget.ordinary_store_sum_captures.is_empty() {
                budget.spend(
                    module.closures.len().saturating_add(module.functions.len()),
                    "new ordinary Sum capture indexed lookup reserve",
                )?;
                budget.ordinary_store_sum_captures = vec![BTreeSet::new(); module.closures.len()];
            }
            let len = budget
                .ordinary_store_sum_captures
                .get(owner.0 as usize)
                .ok_or_else(|| bad("ordinary Sum capture owner absent"))?
                .len();
            budget.spend(len.saturating_add(1), "ordinary Sum capture root insertion")?;
            budget.ordinary_store_sum_captures[owner.0 as usize].insert(root);
        }
        if new_ordinary_pod_store && matches!(ty, Type::Package { .. }) {
            budget.lookup(
                budget.ordinary_pod_store_roots.len(),
                "ordinary POD Store root insertion",
            )?;
            budget.spend(1, "ordinary POD Store root allocation")?;
            budget.ordinary_pod_store_roots.insert(root);
        }
        if contains_owned_store_package_field
            && (new_product_shape || new_sum)
            && matches!(ty, Type::Package { .. })
        {
            budget.lookup(
                budget.owned_store_package_roots.len(),
                "owned Store Package root insertion",
            )?;
            budget.spend(1, "owned Store Package root allocation")?;
            budget.owned_store_package_roots.insert(root);
        }
        if contains_owned_store_package_field
            && (new_product_shape || new_sum || !direct_address_callback)
            && let Type::Address { inner, .. } = ty
            && matches!(inner.as_ref(), Type::Package { .. })
        {
            budget.lookup(
                budget.owned_store_package_roots.len(),
                "borrowed owned Store Package root insertion",
            )?;
            budget.spend(1, "borrowed owned Store Package root allocation")?;
            budget
                .owned_store_package_roots
                .insert(inner.as_ref() as *const Type as usize);
        }
        if new_ordinary_pod_store
            && let Type::Address { inner, .. } = ty
            && matches!(inner.as_ref(), Type::Package { .. })
        {
            budget.lookup(
                budget.ordinary_pod_store_roots.len(),
                "borrowed ordinary POD Store root insertion",
            )?;
            budget.spend(1, "borrowed ordinary POD Store root allocation")?;
            budget
                .ordinary_pod_store_roots
                .insert(inner.as_ref() as *const Type as usize);
        }
    }
    // Complete shape and depth preflight precedes recursive Product admission.
    // A Store element is a value, not an owning user-resource header.
    for element in product_store_elements {
        super::dynamic_store::product_carrier(module, element, budget)?;
    }
    for element in pod_store_elements {
        super::pod_store::carrier(module, element, budget)?;
        // Actual references in this Vec come from the immutable raw module
        // graph (including OwnedStoreNew). Publish only complete successes.
        super::pod_store::publish_carrier(module, element, budget)?;
        // POD addresses use the existing actual-head/element loan machinery.
        // This private presence flag activates work; it grants no permission.
        budget.head_descriptions = true;
        budget.pod_stores = true;
    }
    count(0, module.closures.len(), limits.max_functions, "closure")?;
    budget.input(module.closures.len(), "closure table preflight")?;
    for closure in &module.closures {
        let entry = module
            .functions
            .get(closure.function.0 as usize)
            .ok_or_else(|| bad("closure actual entry is absent"))?;
        let empty_scope = super::tasks::input_scope::empty_function(module, entry, limits, budget)?;
        if let Some(life) = &closure.storage_life {
            super::signature_life::charge(life, budget)?;
            budget.receiver_borrows |= life
                .bindings
                .iter()
                .any(|row| row.position.base == zil_ir::StorageBase::Receiver);
            budget.stored_borrows |= !empty_scope;
            budget.capture_relations |= !empty_scope;
        }
        count(0, closure.captures.len(), limits.max_locals, "capture")?;
        count(
            0,
            closure.lending_captures.len(),
            limits.max_locals,
            "closure lending capture",
        )?;
        budget.input(closure.lending_captures.len(), "closure lending metadata")?;
        if closure
            .lending_captures
            .iter()
            .any(|i| *i as usize >= closure.captures.len())
            || closure.lending_captures.windows(2).any(|w| w[0] >= w[1])
        {
            return Err(bad(
                "closure lending capture indices must be ordered, unique actual slots",
            ));
        }
        count(
            0,
            closure.parameters.len(),
            limits.max_locals,
            "closure parameter",
        )?;
        budget.input(closure.captures.len(), "capture type preflight")?;
        budget.input(
            closure.parameters.len().saturating_add(1),
            "closure signature preflight",
        )?;
        let invocation = closure.lending_captures.is_empty()
            && crate::borrowed_fp::invocation(&closure.parameters, &closure.result);
        if invocation {
            budget.type_work(
                &closure.result,
                true,
                "finite invocation result eligibility",
            )?;
            if !matches!(&closure.result, Type::Address { inner, .. } if inner.is_address_pointee())
            {
                return Err(bad(
                    "invocation callback result requires a finite Copy or slice pointee",
                ));
            }
            budget.borrowed_function_pointers = true;
            budget.invocation_callbacks = true;
        }

        for ty in &closure.captures {
            budget.mutable_capture_contexts |= super::stored_callables::mutable_context_capture(ty);
            // Actual capture shape schedules the independent resource ledger;
            // a qualifier wrapper never removes a physical owning ticket.
            budget.owning_environment_captures |= matches!(
                super::resources::package_storage(ty),
                Type::Managed(_) | Type::Package { .. } | Type::Closure(_) | Type::OwnedDyn { .. }
            );
            budget.owning_environment_captures |= matches!(ty, Type::Address { mutable: true, .. });
            if matches!(ty, Type::BorrowedStorage { storage, .. } if matches!(storage.as_ref(), Type::Package { .. }))
            {
                super::stored_flow::qualification(module, ty, limits, budget)?;
                super::resources::product::admit_owned(module, ty, budget)?;
                budget.owning_environment_captures = true;
                if matches!(ty,Type::BorrowedStorage{life,..} if life.binder_count>0)
                    && closure.storage_life.is_none()
                {
                    return Err(bad(
                        "qualified owning capture omits its actual Fixed class domain",
                    ));
                }
                if closure.kind == zil_ir::CallableKind::Fn
                    && matches!(ty, Type::BorrowedStorage { life, .. } if life.slots.iter().any(|slot| slot.mode == zil_ir::StoredBorrowMode::ExclusiveAddress))
                {
                    return Err(bad(
                        "exclusive stored capture requires an exclusive environment entry",
                    ));
                }
                continue;
            }
            if let Type::OwnedDyn { interface } = ty {
                // Schedule the real dynamic opcode/cleanup consumers even
                // for a closed owner combined with another borrowed capture.
                // The flag adds no port, target or lifetime equality.
                budget.dyn_contexts = true;
                crate::dyn_cleanup::capture_interface(module, *interface, budget)?;
                let item = module
                    .dyn_interfaces
                    .iter()
                    .find(|d| d.id == *interface)
                    .ok_or_else(|| bad("owned capture interface absent"))?;
                if item.retained_life.binder_count > 0 && closure.storage_life.is_none() {
                    return Err(bad(
                        "owned Context capture omits its complete Fixed environment scope",
                    ));
                }
                local_value(ty, Span::default(), module, budget)?;
                continue;
            }
            if matches!(ty, Type::ErrorUnion { result, .. } if matches!(result.as_ref(), Type::OwnedDyn { .. }))
            {
                return Err(bad(
                    "owned dyn error capture requires a selected whole owner",
                ));
            }
            if let Type::Managed(payload) = ty
                && matches!(payload.as_ref(), Type::Task { .. })
            {
                if !matches!(
                    closure.kind,
                    zil_ir::CallableKind::FnOnce | zil_ir::CallableKind::FnMut
                ) {
                    return Err(bad(
                        "captured Task owner requires an actual exclusive or consuming entry",
                    ));
                }
                super::tasks::require_closed_context(payload)?;
                budget.task_retained_environments |= closure.kind == zil_ir::CallableKind::FnMut
                    || matches!(payload.as_ref(),Type::Task{retained_life,..} if !retained_life.ports.is_empty());
                if matches!(payload.as_ref(),Type::Task{retained_life,..} if retained_life.binder_count>0)
                    && closure.storage_life.is_none()
                {
                    return Err(bad(
                        "captured Task omits complete actual Fixed class domain",
                    ));
                }
                local_value(ty, Span::default(), module, budget)?;
                continue;
            }
            if super::stored_callables::semantic_copy_capture(module, closure, ty, budget)? {
                continue;
            }
            if closure.storage_life.is_some() && !empty_scope {
                budget.stored_fixed_owner |= matches!(ty, Type::Managed(_) | Type::Package { .. });
                if !super::stored_callables::capture(ty)
                    && !super::stored_callables::exclusive_capture(ty)
                    && !super::stored_callables::owned_capture(ty, budget)?
                    && !super::stored_callables::ordinary_capture(module, ty, budget)?
                    && !matches!(ty,Type::Closure(child) if super::stored_callables::finite(module,*child))
                {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Analyze,
                        "capture-grounded Fixed requires scalar/shared Address captures",
                        None,
                    )]);
                }
                if let Type::Address { inner, .. } = ty
                    && matches!(inner.as_ref(), Type::BorrowedStorage { .. })
                {
                    super::stored_flow::qualification(module, inner, limits, budget)?;
                    continue;
                }
            }
            budget.address_capture |= matches!(ty, Type::Address { .. });
            if ty.is_package_address() {
                budget.shared_product_capture = true;
                let Type::Address { mutable, inner } = ty else {
                    unreachable!()
                };
                if *mutable {
                    if closure.kind == zil_ir::CallableKind::Fn {
                        return Err(bad(
                            "exclusive Product capture requires an exclusive environment entry",
                        ));
                    }
                    budget.owning_address_captures = true;
                }
                super::resources::product::admit_capture_head(module, inner, budget)?;
                budget.type_work(&closure.result, true, "shared Product capture result")?;
                if !closure.result.is_copy_value() {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Analyze,
                        "shared Product captures require a finite Copy result",
                        None,
                    )]);
                }
                continue;
            }
            if let Type::Address { inner, .. } = ty
                && matches!(
                    inner.as_ref(),
                    Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }
                )
            {
                super::resources::sums::admit(module, inner, budget)?;
                if closure.kind == zil_ir::CallableKind::Fn
                    && matches!(ty, Type::Address { mutable: true, .. })
                {
                    return Err(bad(
                        "exclusive owning-base capture requires an exclusive environment entry",
                    ));
                }
                budget.owning_address_captures = true;
                continue;
            }
            if matches!(
                ty,
                Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }
            ) && budget.ordinary_store_sum_capture(closure.id, ty)?
            {
                if !super::resources::sums::admit(module, ty, budget)? {
                    return Err(bad(
                        "owning Sum capture requires a complete ordinary Store leaf",
                    ));
                }
                continue;
            }
            if ty.is_slice_address() && closure.storage_life.is_some() {
                budget.type_work(ty, true, "captured slice complete finite element type")?;
                continue;
            }
            if ty.is_slice_address() || super::resources::owned_error(ty) {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Analyze,
                    "slice and owned error-union captures are not implemented",
                    None,
                )]);
            }
            if super::ownership_store::pod_owned(ty)
                || (matches!(ty, Type::Package { .. }) && budget.ordinary_pod_store_type(ty)?)
            {
                super::ownership_store::capture(module, ty, budget)?;
            } else if super::resources::product::nested(ty) {
                // Capture admission is independent from ordinary value admission.
                // It rechecks the complete finite Product before granting an environment slot.
                super::resources::product::admit(ty, budget)?;
            } else {
                legacy_value(ty, Span::default(), module)?;
            }
        }
        let mut owned_signature = false;
        for ty in closure
            .parameters
            .iter()
            .chain(std::iter::once(&closure.result))
        {
            if matches!(
                ty,
                Type::Package { .. } | Type::Enum { .. } | Type::ErrorUnion { .. }
            ) && !ty.is_copy_value()
            {
                super::resources::sums::admit(module, ty, budget)?;
                owned_signature = true;
            }
        }
        if owned_signature {
            budget.spend(1, "owned callback signature entry")?;
            super::owned_callbacks::signature(module, closure, budget)?;
            budget.owned_callbacks.insert(closure.id);
        }
        for ty in &closure.parameters {
            if budget.stored_borrows && super::stored_callables::finite(module, closure.id) {
                let actual = if let Type::Address { inner, .. } = ty {
                    inner.as_ref()
                } else {
                    ty
                };
                if matches!(actual, Type::BorrowedStorage { .. }) {
                    super::stored_flow::qualification(module, actual, limits, budget)?;
                    continue;
                }
            }
            if ty.is_shared_sync_cell_address() {
                continue;
            }
            if owned_signature {
                if matches!(ty, Type::Address { .. }) {
                    budget.type_work(&closure.result, true, "borrowed callback result profile")?;
                    if !matches!(
                        closure.result,
                        Type::Package { .. } | Type::Enum { .. } | Type::ErrorUnion { .. }
                    ) || closure.result.is_copy_value()
                    {
                        return Err(bad(
                            "borrowed inputs in an owning callback require a closed owned result",
                        ));
                    }
                    budget.borrowed_callback_inputs = true;
                    super::owned_callbacks::input(module, ty, budget)?;
                } else {
                    super::owned_callbacks::data(module, ty, budget)?;
                }
                continue;
            }
            if ty.is_shared_package_address() {
                budget.borrowed_callback_inputs = true;
                super::owned_callbacks::input(module, ty, budget)?;
                continue;
            }
            if let Type::Address { inner, .. } = ty
                && (closure.result.is_copy_value()
                    || invocation
                    || super::stored_callables::constant_result(closure))
            {
                // A slice is unsized only at this pointee position; its
                // elements remain independently restricted to finite Copy.
                let pointee = match inner.as_ref() {
                    Type::Slice { element } => element.as_ref(),
                    other => other,
                };
                scalar(pointee, Span::default())?;
            } else {
                scalar(ty, Span::default())?;
            }
        }
        let task_result = matches!(&closure.result,Type::Managed(payload) if matches!(payload.as_ref(),Type::Task{..}));
        budget.task_callable_results |= task_result;
        if task_result
            && (closure.kind != zil_ir::CallableKind::FnOnce || !closure.parameters.is_empty())
        {
            return Err(bad(
                "owning Task callback result requires its actual consuming entry and complete input relation",
            ));
        }
        if owned_signature {
            super::owned_callbacks::data(module, &closure.result, budget)?;
        } else if task_result {
            local_value(&closure.result, Span::default(), module, budget)?;
        } else if invocation
            || super::stored_callables::transferred(closure)
            || super::stored_callables::constant_result(closure)
            || (matches!(&closure.result, Type::Address { inner, mutable: false }
            if inner.is_copy_value() || matches!(&**inner, Type::Slice { element } if element.is_copy_value()))
                && matches!(
                    closure.kind,
                    zil_ir::CallableKind::Fn | zil_ir::CallableKind::FnMut
                )
                && !closure.lending_captures.is_empty())
        {
            legacy_value(&closure.result, Span::default(), module)?;
        } else {
            scalar(&closure.result, Span::default())?;
        }
        if closure.function.0 as usize >= module.functions.len() {
            return Err(bad("closure entry references an absent function"));
        }
    }
    if (budget.registrations || budget.async_registrations)
        && budget.task_input_scopes.is_none()
        && module
            .functions
            .iter()
            .any(|function| function.storage_life.is_some())
    {
        budget.task_input_scopes = Some(crate::tasks::input_scope::build(module, limits, budget)?);
    }
    let (mut blocks, mut instructions, mut locals, mut values) = (0, 0, 0, 0);
    // Record strict package admission during the already charged type visits.
    // Each bounded function visit also materializes one flag; no second type
    // scan is needed in the scope checker, including for legacy scalar IR.
    let mut package_functions = Vec::new();
    for (function_index, function) in module.functions.iter().enumerate() {
        budget.qualified_head_returns |= matches!(&function.return_type, Type::Address{inner,..}
            if matches!(inner.as_ref(),Type::BorrowedStorage{..}));
        count(
            0,
            function.protocol_contracts.len(),
            limits.max_locals,
            "protocol roles",
        )?;
        if !function.protocol_contracts.is_empty() {
            budget.input(
                function.protocol_contracts.len().saturating_mul(13),
                "protocol role headers",
            )?;
            for role in &function.protocol_contracts {
                count(
                    0,
                    role.entry.len(),
                    limits.max_nodes,
                    "protocol entry states",
                )?;
                count(
                    0,
                    role.exits.len(),
                    limits.max_nodes,
                    "protocol transition rows",
                )?;
                budget.input(
                    role.entry
                        .len()
                        .saturating_add(role.exits.len().saturating_mul(6)),
                    "protocol full transition prehash",
                )?;
                for row in &role.exits {
                    count(
                        0,
                        row.output.len(),
                        limits.max_nodes,
                        "protocol output states",
                    )?;
                    budget.input(row.output.len(), "protocol output prehash")?;
                }
            }
        }
        budget.protocol_contracts |= !function.protocol_contracts.is_empty();
        budget.qualified_head_returns |= matches!(&function.return_type, Type::Address{inner,..}
            if matches!(inner.as_ref(),Type::BorrowedStorage{..}));
        if let Some(contract) = &function.storage_life {
            if function.call_kind == zil_ir::CallKind::Suspend {
                budget.spend(function.parameters.len(), "Suspend signature inputs")?;
                let mut parameters = Vec::new();
                for id in &function.parameters {
                    let ty = &function
                        .locals
                        .get(id.0 as usize)
                        .ok_or_else(|| bad("Suspend signature input absent"))?
                        .ty;
                    super::tasks::charge_result_selection(ty, budget)?;
                    parameters.push(ty.clone());
                }
                for ty in &parameters {
                    super::tasks::charge_result_selection(ty, budget)?;
                }
                super::tasks::charge_result_selection(&function.return_type, budget)?;
                super::signature_life::charge(contract, budget)?;
                super::signature_life::function_relation_resource_scopes(
                    &parameters,
                    &function.return_type,
                    contract,
                    false,
                    module,
                    false,
                    budget,
                )?;
            } else if !super::tasks::input_scope::empty_function(module, function, limits, budget)?
                && !super::tasks::borrowed_signature(module, function, budget)?
            {
                super::stored_flow::header(module, function, budget)?;
                budget.stored_borrows = true;
            }
        }
        // The complete header was charged and checked above. This marker only
        // schedules the separate actual type/inventory proof; it grants no loan.
        if let Some(contract) = &function.storage_life {
            budget.ordinary_input_head_candidates |= contract.fixed_binder_count == 0
                && contract.invocation_binder_count > 0
                && contract.classes.is_empty()
                && contract.bindings.iter().all(|row| {
                    matches!(row.position.base, zil_ir::StorageBase::Parameter(_))
                        && row.position.path.is_empty()
                });
        }
        if function.id.0 as usize != function_index {
            return Err(bad("noncanonical function ID"));
        }
        blocks = count(blocks, function.blocks.len(), limits.max_blocks, "block")?;
        locals = count(locals, function.locals.len(), limits.max_locals, "local")?;
        values = count(values, function.values.len(), limits.max_values, "value")?;
        if function.resume_bound == zil_ir::TaskResumeBound::Transfer {
            budget.spend(1, "Task resume Transfer request")?;
            if function.call_kind != zil_ir::CallKind::Suspend
                || function.foreign.is_some()
                || function.environment.is_some()
            {
                return Err(bad(
                    "Task Transfer resume requires an actual ordinary Suspend entry",
                ));
            }
        }
        budget.input(1, "function preflight")?;
        budget.input(1, "retained return source discriminant")?;
        budget.input(1, "dynamic definition instance presence")?;
        if let Some(instance) = &function.dyn_instance {
            super::dyn_metadata::recipe(instance, budget)?;
            return Err(super::bad(
                "dynamic function instance requires its complete definition proof",
            ));
        }
        if let Some(bindings) = &function.retained_return_sources {
            count(
                0,
                bindings.len(),
                limits.max_nodes,
                "retained return bindings",
            )?;
            budget.input(bindings.len(), "retained return binding rows")?;
            for binding in bindings {
                count(
                    0,
                    binding.sources.len(),
                    limits.max_nodes,
                    "retained return sources",
                )?;
                count(
                    0,
                    binding.position.path.len(),
                    limits.max_call_depth,
                    "retained return position depth",
                )?;
                budget.input(
                    binding
                        .position
                        .path
                        .len()
                        .saturating_add(binding.sources.len())
                        .saturating_add(1),
                    "retained result positions",
                )?;
                for atom in &binding.sources {
                    if let zil_ir::StorageSource::Static { leaf, mode } = atom
                        && super::stored_callables::constant_entry(module, function)
                    {
                        if *mode != zil_ir::StoredBorrowMode::SharedAddress
                            || !matches!(leaf,Type::Address{mutable:false,inner}if **inner==Type::U8 || matches!(inner.as_ref(),Type::Slice{element}if **element==Type::U8))
                            || leaf != &function.return_type
                        {
                            return Err(bad(
                                "constant callable result requires its complete immutable actual leaf and mode",
                            ));
                        }
                        budget.input(if matches!(leaf,Type::Address{inner,..}if matches!(inner.as_ref(),Type::Slice{..})){5}else{4},"constant callable full result leaf")?;
                        continue;
                    }
                    if let zil_ir::StorageSource::Exact(source) = atom {
                        count(
                            0,
                            source.path.len(),
                            limits.max_call_depth,
                            "retained return source depth",
                        )?;
                        budget.input(
                            source.path.len().saturating_add(1),
                            "retained return source path",
                        )?;
                    } else {
                        // Structural preflight visits every path and complete
                        // leaf before identity. Permission is established only
                        // by the actual result consumer immediately below.
                        super::dyn_metadata::recipe(atom, budget)?;
                    }
                }
            }
            super::stored_flow::dyn_return_header(module, function, budget)?;
        }
        if function.retained_return_sources.is_none() {
            super::stored_flow::dyn_return_header(module, function, budget)?;
        }
        budget.input(function.name.len(), "function metadata")?;
        if let Some(contract) = &function.callable_return_contract {
            if let Some(life) = &contract.life {
                let work = zil_ir::storage_relation_metadata_work(
                    life,
                    limits.max_nodes,
                    limits.max_call_depth,
                    budget.remaining(),
                )
                .ok_or_else(|| exhausted("callable return relation metadata"))?;
                budget.input(work, "callable return relation")?;
            }
            // Prepay untrusted provenance before CandidatePlan::empty_for hashes it.
            budget.input(1, "callable source presence")?;
            if let Some(bindings) = &contract.capture_sources {
                count(
                    0,
                    bindings.len(),
                    limits.max_nodes,
                    "callable source binding",
                )?;
                budget.input(
                    bindings.len().saturating_add(1),
                    "callable source binding inventory",
                )?;
                for binding in bindings {
                    count(
                        0,
                        binding.position.path.len(),
                        limits.max_call_depth,
                        "callable capture path",
                    )?;
                    count(
                        0,
                        binding.sources.len(),
                        limits.max_nodes,
                        "callable actual source",
                    )?;
                    budget.input(
                        binding
                            .position
                            .path
                            .len()
                            .saturating_mul(4)
                            .saturating_add(binding.sources.len())
                            .saturating_add(4),
                        "callable source paths",
                    )?;
                    for atom in &binding.sources {
                        if let zil_ir::StorageSource::Static { leaf, mode } = atom {
                            if *mode != zil_ir::StoredBorrowMode::SharedAddress
                                || !matches!(leaf,Type::Address{mutable:false,inner}if **inner==Type::U8 || matches!(inner.as_ref(),Type::Slice{element}if **element==Type::U8))
                            {
                                return Err(bad(
                                    "constant capture metadata requires its complete immutable byte leaf",
                                ));
                            }
                            budget.input(if matches!(leaf,Type::Address{inner,..}if matches!(inner.as_ref(),Type::Slice{..})){5}else{4},"constant capture complete leaf metadata")?;
                            continue;
                        }
                        let source = crate::exact_storage_source(atom)?;
                        count(
                            0,
                            source.path.len(),
                            limits.max_call_depth,
                            "callable source path",
                        )?;
                        budget.input(
                            source.path.len().saturating_mul(4).saturating_add(3),
                            "callable actual source path",
                        )?;
                    }
                }
            }

            // Scheduling only. retained::sources independently proves every
            // external leaf before any actual root/parent substitution.
            budget.owning_address_captures |= contract
                .capture_sources
                .as_ref()
                .is_some_and(|rows| !rows.is_empty());
            if contract
                .capture_sources
                .as_ref()
                .is_some_and(|rows| !rows.is_empty())
            {
                budget.input(
                    budget
                        .returned_borrowed_environments
                        .len()
                        .saturating_add(1),
                    "returned environment transport schedule",
                )?;
                let mut pending = vec![contract.closure];
                while let Some(id) = pending.pop() {
                    budget.input(
                        budget
                            .returned_borrowed_environments
                            .len()
                            .saturating_add(1),
                        "returned environment child transport schedule",
                    )?;
                    if !budget.returned_borrowed_environments.insert(id) {
                        continue;
                    }
                    let definition = module
                        .closures
                        .get(id.0 as usize)
                        .ok_or_else(|| bad("returned environment child definition absent"))?;
                    budget.input(
                        definition.captures.len(),
                        "returned environment actual child edges",
                    )?;
                    if definition
                        .captures
                        .iter()
                        .any(super::stored_callables::exclusive_capture)
                    {
                        budget.stored_borrows = true;
                    }
                    for capture in &definition.captures {
                        if let Type::Closure(child) = capture {
                            pending.push(*child);
                        }
                    }
                }
            }
            count(
                0,
                contract.lending_captures.len(),
                limits.max_locals,
                "callable return lending capture",
            )?;
            budget.input(
                contract
                    .hold
                    .len()
                    .saturating_add(contract.call.len())
                    .saturating_add(contract.writes.len())
                    .saturating_add(contract.cleanup.len())
                    .saturating_add(contract.lending_captures.len())
                    .saturating_add(2)
                    .saturating_add(match contract.source {
                        zil_ir::CallableReturnSource::Fresh => 1,
                        zil_ir::CallableReturnSource::Parameter(_) => 2,
                    }),
                "callable return contract metadata",
            )?;
        }
        if function.call_kind == zil_ir::CallKind::Suspend {
            budget.tasks = true;
        }
        if let Some(contract) = &function.callable_contract {
            count(0, contract.roles.len(), limits.max_locals, "callable role")?;
            budget.input(
                contract.roles.len().saturating_add(1),
                "callable role table",
            )?;
            for role in &contract.roles {
                if let Some(life) = &role.life {
                    crate::signature_life::charge(life, budget)?;
                    budget.borrowed_function_pointers = true;
                    budget.invocation_callbacks = true;
                }
                budget.input(
                    role.hold
                        .len()
                        .saturating_add(role.call.len())
                        .saturating_add(role.writes.len())
                        .saturating_add(5),
                    "callable role metadata",
                )?;
            }
            match &contract.traces {
                CallableTraces::Finite(traces) => {
                    budget.input(traces.len(), "callable finite trace count")?;
                    for trace in traces {
                        count(
                            0,
                            trace.calls.len(),
                            limits.max_instructions,
                            "callable finite events",
                        )?;
                        budget.input(
                            trace.calls.len().saturating_mul(2).saturating_add(1),
                            "callable trace metadata",
                        )?;
                    }
                }
                CallableTraces::Flow(flow) => {
                    count(0, flow.nodes.len(), limits.max_nodes, "callable flow node")?;
                    budget.input(
                        flow.nodes.len().saturating_add(1),
                        "callable flow node metadata",
                    )?;
                    let (mut calls, mut edges) = (0, 0);
                    for node in &flow.nodes {
                        calls = count(
                            calls,
                            node.calls.len(),
                            limits.max_instructions,
                            "callable flow call",
                        )?;
                        budget.input(
                            node.calls.len().saturating_mul(2).saturating_add(1),
                            "callable flow calls and next tag",
                        )?;
                        match &node.next {
                            CallableFlowNext::Edges(next) => {
                                edges = count(
                                    edges,
                                    next.len(),
                                    limits.max_analysis_steps,
                                    "callable flow edge",
                                )?;
                                budget
                                    .input(next.len().saturating_add(1), "callable flow edges")?;
                            }
                            CallableFlowNext::Exit(_) => budget.input(1, "callable flow exit")?,
                        }
                    }
                }
            }
        }
        if let Some(foreign) = &function.foreign {
            if foreign.thread_driver_transfer
                && !matches!(
                    foreign.scoped_callback,
                    Some(
                        zil_ir::ScopedCallbackRole::ThreadAsyncRegister
                            | zil_ir::ScopedCallbackRole::ThreadAsyncRequestClose
                            | zil_ir::ScopedCallbackRole::ThreadAsyncPollDrain
                    )
                )
            {
                return Err(bad(
                    "foreign driver transfer promise has no threaded registration role",
                ));
            }
            if foreign.scoped_callback.is_some() {
                budget.input(1, "scoped foreign role")?;
                budget.scoped_foreign = true;
            }
            budget.input(
                foreign
                    .abi
                    .len()
                    .saturating_add(foreign.symbol.len())
                    .saturating_add(foreign.noescape_parameters.len())
                    .saturating_add(1),
                "foreign declaration metadata",
            )?;
        }
        budget.input(1, "return type preflight")?;
        let mut sum_function = budget.sum_type(&function.return_type)?;
        let mut product_store = local_value(&function.return_type, function.span, module, budget)?;
        let scoped_aggregate = |ty: &Type| {
            matches!(ty, Type::Package { .. })
                || super::resources::owned_error(ty)
                || matches!(ty, Type::Array { element, .. } if matches!(element.as_ref(), Type::Package { .. } | Type::Closure(_)))
        };
        let mut package = scoped_aggregate(&function.return_type);
        budget.input(function.parameters.len(), "parameter preflight")?;
        for parameter in &function.parameters {
            if parameter.0 as usize >= function.locals.len() {
                return Err(bad("parameter local outside table"));
            }
            product_store |= local_value(
                &function.locals[parameter.0 as usize].ty,
                function.span,
                module,
                budget,
            )?;
        }
        if function
            .environment
            .is_some_and(|id| id.0 as usize >= module.closures.len())
        {
            return Err(bad("function references an absent environment"));
        }
        if let Some(id) = function.environment
            && budget.has_ordinary_store_sum_capture(id)?
        {
            for ty in &module.closures[id.0 as usize].captures {
                budget.input(1, "entry Sum capture membership")?;
                sum_function |= budget.ordinary_store_sum_capture(id, ty)?;
            }
        }
        for (index, local) in function.locals.iter().enumerate() {
            budget.input(1, "local type preflight")?;
            if local.id.0 as usize != index {
                return Err(bad("noncanonical local ID"));
            }
            budget.input(local.name.len(), "local metadata")?;
            sum_function |= budget.sum_type(&local.ty)?;
            product_store |= local_value(&local.ty, local.span, module, budget)?;
            package |= scoped_aggregate(&local.ty);
        }
        for (value, ty) in function.values.iter().enumerate() {
            budget.input(1, "value type preflight")?;
            if matches!(ty, Type::Desc(_)) {
                if budget.description_values.len() == budget.description_values.capacity() {
                    let growth = budget.description_values.capacity().max(4);
                    budget.spend(
                        growth.saturating_add(budget.description_values.len()),
                        "weak description readonly publication index",
                    )?;
                    budget
                        .description_values
                        .try_reserve_exact(growth)
                        .map_err(|_| exhausted("weak description publication index allocation"))?;
                }
                budget.spend(1, "weak description publication position")?;
                budget
                    .description_values
                    .push((function.id, ValueId(value as u32)));
            }
            sum_function |= budget.sum_type(ty)?;
            product_store |= local_value(ty, function.span, module, budget)?;
            package |= scoped_aggregate(ty);
        }
        for block in &function.blocks {
            instructions = count(
                instructions,
                block.instructions.len(),
                limits.max_instructions,
                "instruction",
            )?;
            budget.input(1, "block preflight")?;
            for instruction in &block.instructions {
                budget.input(1, "instruction preflight")?;
                if module.target.pointer_width == 32 {
                    match &instruction.kind {
                        InstructionKind::Constant(Constant::Isize(n))
                            if i32::try_from(*n).is_err() =>
                        {
                            return Err(bad("isize constant exceeds selected target width"));
                        }
                        InstructionKind::Constant(Constant::Usize(n))
                            if *n > u64::from(u32::MAX) =>
                        {
                            return Err(bad("usize constant exceeds selected target width"));
                        }
                        _ => {}
                    }
                }

                if let InstructionKind::Call { function, .. } = instruction.kind
                    && module
                        .functions
                        .get(function.0 as usize)
                        .is_some_and(|callee| {
                            super::resources::qualified_package_address(&callee.return_type)
                        })
                {
                    budget.qualified_package_returns = true;
                }
                budget.byte_queries |= matches!(
                    instruction.kind,
                    InstructionKind::ByteBufferLen { .. }
                        | InstructionKind::ByteBufferCapacity { .. }
                );
                budget.shared_byte_borrow |= matches!(
                    instruction.kind,
                    InstructionKind::ByteBufferBorrow { mutable: false, .. }
                );
                match &instruction.kind {
                    InstructionKind::AsyncRegistrationStart {
                        register,
                        request_close,
                        poll_drain,
                        callback,
                        ..
                    } => {
                        budget.registrations = true;
                        budget.scoped_foreign = true;
                        budget.head_descriptions = true;
                        budget.input(5, "async registration actual operands")?;
                        if [register, request_close, poll_drain]
                            .iter()
                            .any(|id| id.0 as usize >= module.functions.len())
                            || callback.0 as usize >= function.values.len()
                        {
                            return Err(bad("async registration operand outside actual tables"));
                        }
                        budget.lookup(
                            budget.scoped_functions.len(),
                            "async registration unsafe function inventory",
                        )?;
                        budget.scoped_functions.insert(function.id);
                    }
                    InstructionKind::MakeDynBorrow { .. } | InstructionKind::CallDynSlot { .. } => {
                    }
                    InstructionKind::LoadStoredBorrow { .. }
                    | InstructionKind::StoreStoredBorrow { .. } => {
                        budget.stored_slot_updates |= matches!(
                            instruction.kind,
                            InstructionKind::StoreStoredBorrow { .. }
                                | InstructionKind::LoadStoredBorrow {
                                    mode: StoredBorrowLoad::Take,
                                    ..
                                }
                        );
                        super::stored_borrows::instruction(function, instruction, budget)?;
                    }
                    InstructionKind::RegistrationStart {
                        register,
                        close,
                        callback,
                        ..
                    } => {
                        budget.registrations = true;
                        budget.scoped_foreign = true;
                        budget.input(4, "registration actual operands")?;
                        if register.0 as usize >= module.functions.len()
                            || close.0 as usize >= module.functions.len()
                            || callback.0 as usize >= function.values.len()
                        {
                            return Err(bad("registration operand outside actual tables"));
                        }
                        budget.lookup(
                            budget.scoped_functions.len(),
                            "registration unsafe function inventory",
                        )?;
                        budget.scoped_functions.insert(function.id);
                    }
                    InstructionKind::RegistrationValid { registration }
                    | InstructionKind::RegistrationClose { registration }
                    | InstructionKind::AsyncRegistrationValid { registration }
                    | InstructionKind::AsyncRegistrationRequestClose { registration }
                    | InstructionKind::AsyncRegistrationPollClose { registration }
                    | InstructionKind::AsyncRegistrationFinishDestroy { registration }
                    | InstructionKind::AsyncRegistrationFinishDestroyChecked { registration } => {
                        budget.registrations = true;
                        budget.protocol_checked |= matches!(
                            instruction.kind,
                            InstructionKind::AsyncRegistrationFinishDestroyChecked { .. }
                        );
                        budget.input(1, "registration actual operand")?;
                        if registration.0 as usize >= function.values.len() {
                            return Err(bad("registration operand outside actual values"));
                        }
                    }
                    InstructionKind::SuspendCall {
                        function: child,
                        arguments,
                    } => {
                        count(
                            0,
                            arguments.len(),
                            limits.max_values,
                            "Suspend child arguments",
                        )?;
                        budget.tasks = true;
                        budget.input(
                            arguments.len().saturating_add(2),
                            "Suspend child actual operands",
                        )?;
                        if child.0 as usize >= module.functions.len()
                            || arguments
                                .iter()
                                .any(|v| v.0 as usize >= function.values.len())
                        {
                            return Err(bad("Suspend child operand outside actual tables"));
                        }
                    }
                    InstructionKind::TaskCreate { .. }
                    | InstructionKind::TaskCreateOwned { .. }
                    | InstructionKind::TaskStart { .. }
                    | InstructionKind::TaskYield
                    | InstructionKind::TaskValid { .. }
                    | InstructionKind::TaskPoll { .. }
                    | InstructionKind::TaskRequestCancel { .. }
                    | InstructionKind::TaskPollClose { .. }
                    | InstructionKind::TaskFinishDestroy { .. } => {
                        budget.tasks = true;
                        budget.owned_task_input_inventory |=
                            matches!(instruction.kind, InstructionKind::TaskCreateOwned { .. });
                        budget.input(
                            crate::resources::uses(&instruction.kind)
                                .count()
                                .saturating_add(3),
                            "task ABI operands",
                        )?;
                        for v in crate::resources::uses(&instruction.kind) {
                            if v.0 as usize >= function.values.len() {
                                return Err(bad("task operand is outside actual table"));
                            }
                        }
                    }
                    InstructionKind::WithSyncCell { initial, body, .. } => {
                        budget.input(3, "sealed cell scope operands")?;
                        if initial.0 as usize >= function.values.len()
                            || body.0 as usize >= function.values.len()
                        {
                            return Err(bad("sealed cell scope operand outside actual tables"));
                        }
                        budget.scoped_threads = true;
                    }
                    InstructionKind::SyncI32CompareExchange {
                        address,
                        expected,
                        desired,
                        ..
                    }
                    | InstructionKind::SyncIntegerCompareExchange {
                        address,
                        expected,
                        desired,
                        ..
                    } => {
                        budget.input(6, "strong compare exchange operands and order pair")?;
                        if [address, expected, desired]
                            .iter()
                            .any(|v| v.0 as usize >= function.values.len())
                        {
                            return Err(bad("compare exchange operand outside actual value table"));
                        }
                        budget.scoped_threads = true;
                    }
                    InstructionKind::SyncI32 { address, value, .. }
                    | InstructionKind::SyncInteger { address, value, .. } => {
                        budget.input(3, "sealed cell operation operands")?;
                        if address.0 as usize >= function.values.len()
                            || value.is_some_and(|value| value.0 as usize >= function.values.len())
                        {
                            return Err(bad("sealed cell operation operand outside actual tables"));
                        }
                        budget.scoped_threads = true;
                    }
                    InstructionKind::DescribeAddress { .. } => {
                        budget.head_descriptions = true;
                    }
                    InstructionKind::MakeDynOwned { .. }
                    | InstructionKind::BorrowDynOwned { .. } => {
                        budget.head_descriptions = true;
                    }
                    InstructionKind::MutexCreate { .. }
                    | InstructionKind::MutexLock { .. }
                    | InstructionKind::MutexValue { .. }
                    | InstructionKind::MutexUnlock { .. }
                    | InstructionKind::DropMutexGuard { .. } => {
                        budget.generic_mutex = true;
                        budget.head_descriptions = true;
                    }
                    InstructionKind::WithScopedThread {
                        worker,
                        argument,
                        body,
                    } => {
                        budget.input(4, "scoped thread operands")?;
                        if worker.0 as usize >= module.functions.len()
                            || argument.0 as usize >= function.values.len()
                            || body.0 as usize >= function.values.len()
                        {
                            return Err(bad("scoped thread operand outside actual tables"));
                        }
                        budget.scoped_threads = true;
                    }
                    InstructionKind::WithForeignCallback {
                        register,
                        close,
                        callback,
                        body,
                        ..
                    } => {
                        budget.input(5, "scoped foreign callback operands")?;
                        if register.0 as usize >= module.functions.len()
                            || close.0 as usize >= module.functions.len()
                            || callback.0 as usize >= function.values.len()
                            || body.0 as usize >= function.values.len()
                        {
                            return Err(bad(
                                "scoped foreign callback operand outside actual tables",
                            ));
                        }
                        budget.lookup(
                            budget.scoped_functions.len(),
                            "scoped foreign function insertion",
                        )?;
                        budget.scoped_functions.insert(function.id);
                        budget.scoped_foreign = true;
                    }
                    InstructionKind::ReservePlaceLocal { .. }
                    | InstructionKind::ReservePlaceAddress { .. }
                    | InstructionKind::CommitPlaceReplace { .. }
                    | InstructionKind::AbortPlaceReservation { .. } => {
                        if let InstructionKind::ReservePlaceLocal { path, .. }
                        | InstructionKind::ReservePlaceAddress { path, .. } = &instruction.kind
                        {
                            count(0, path.len(), limits.max_call_depth, "replacement path")?;
                            budget.input(
                                path.len().saturating_mul(12).saturating_add(1),
                                "replacement preflight/hash path",
                            )?;
                        }
                        budget.lookup(
                            budget.replacement_functions.len(),
                            "replacement function profile insertion",
                        )?;
                        budget.replacement_functions.insert(function.id);
                        // Actual endpoint/activation, resource and pin passes
                        // independently reconstruct these private consumers.
                        // No producer ticket or claimed pointee grants access.
                        budget.addressed_sums = true;
                    }
                    InstructionKind::TakeTupleField { local, path }
                    | InstructionKind::TakeVariantField { local, path }
                    | InstructionKind::BorrowPlaceLocal { local, path, .. }
                    | InstructionKind::PlaceTagLocal { local, path } => {
                        sum_function = true;
                        budget.addressed_sums = true;
                        if local.0 as usize >= function.locals.len() {
                            return Err(bad("mixed local place outside table"));
                        }
                        count(0, path.len(), limits.max_call_depth, "mixed local path")?;
                        budget.input(
                            path.len().saturating_mul(3).saturating_add(1),
                            "mixed local path preflight",
                        )?;
                        if path.is_empty()
                            && !matches!(instruction.kind, InstructionKind::PlaceTagLocal { .. })
                        {
                            return Err(bad("mixed local path must be nonempty"));
                        }
                    }
                    InstructionKind::BorrowPlaceAddress { address, path, .. }
                    | InstructionKind::PlaceTagAddress { address, path } => {
                        sum_function = true;
                        budget.addressed_sums = true;
                        budget.mixed_address_sums = true;
                        if address.0 as usize >= function.values.len() {
                            return Err(bad("mixed actual address outside table"));
                        }
                        count(0, path.len(), limits.max_call_depth, "mixed address path")?;
                        budget.input(
                            path.len().saturating_mul(3).saturating_add(1),
                            "mixed address preflight",
                        )?;
                        if path.is_empty()
                            && matches!(
                                instruction.kind,
                                InstructionKind::BorrowPlaceAddress { .. }
                            )
                        {
                            return Err(bad("mixed address borrow path must be nonempty"));
                        }
                    }
                    InstructionKind::BorrowFieldLocal { local, path, .. } => {
                        budget.addressed_sums = true;
                        if local.0 as usize >= function.locals.len() {
                            return Err(bad("field place local outside table"));
                        }
                        count(0, path.len(), limits.max_call_depth, "field place path")?;
                        budget.input(path.len().saturating_add(1), "field place preflight")?;
                    }
                    InstructionKind::VariantTagAddress { address, path }
                    | InstructionKind::BorrowVariantAddress { address, path, .. } => {
                        sum_function = true;
                        budget.addressed_sums = true;
                        if address.0 as usize >= function.values.len() {
                            return Err(bad("address Sum base outside value table"));
                        }
                        count(0, path.len(), limits.max_call_depth, "address Sum path")?;
                        budget.input(
                            path.len().saturating_mul(2).saturating_add(1),
                            "address Sum path preflight",
                        )?;
                    }
                    InstructionKind::VariantTagLocal { local, path }
                    | InstructionKind::ReadVariantLocal { local, path }
                    | InstructionKind::BorrowVariantLocal { local, path, .. } => {
                        budget.addressed_sums |=
                            matches!(instruction.kind, InstructionKind::BorrowVariantLocal { .. });

                        sum_function = true;
                        if local.0 as usize >= function.locals.len() {
                            return Err(bad("Sum local outside table"));
                        }
                        count(0, path.len(), limits.max_call_depth, "Sum path")?;
                        budget.input(
                            path.len().saturating_mul(2).saturating_add(1),
                            "Sum path preflight",
                        )?;
                    }
                    InstructionKind::UnpackVariant { local, fields, .. }
                    | InstructionKind::UnpackTuple { local, fields } => {
                        sum_function = true;
                        count(0, fields.len(), limits.max_locals, "Unpack fields")?;
                        budget.input(fields.len().saturating_add(1), "Unpack target preflight")?;
                        // Reserve the semantic cache key's owned target vector
                        // on every request, including uncached analysis.
                        budget.spend(fields.len(), "Unpack cache key allocation reserve")?;
                        if local.0 as usize >= function.locals.len()
                            || fields.iter().any(|l| l.0 as usize >= function.locals.len())
                        {
                            return Err(bad("Unpack local outside table"));
                        }
                    }
                    InstructionKind::DropEnum { local } | InstructionKind::DropTuple { local } => {
                        sum_function = true;
                        if local.0 as usize >= function.locals.len() {
                            return Err(bad("DropEnum local outside table"));
                        }
                    }
                    InstructionKind::StaticUtf8 { bytes } => {
                        budget.static_utf8 = true;
                        count(0, bytes.len(), limits.max_nodes, "static UTF8 bytes")?;
                        // Pay for validation, fingerprinting and owned cache-key copies
                        // before inspecting any caller-built byte vector.
                        budget.input(bytes.len(), "static UTF8 bytes")?;
                        budget.spend(bytes.len(), "static UTF8 validation")?;
                        if std::str::from_utf8(bytes).is_err() {
                            return Err(bad("static UTF8 instruction contains invalid UTF8"));
                        }
                        let valid = instruction.result
                            .and_then(|id| function.values.get(id.0 as usize))
                            .is_some_and(|ty| matches!(ty,
                                Type::Address { mutable: false, inner }
                                if matches!(inner.as_ref(), Type::Slice { element } if **element == Type::U8)));
                        if !valid {
                            return Err(bad(
                                "static UTF8 requires exactly an immutable byte slice result",
                            ));
                        }
                    }
                    InstructionKind::OwnedStoreNew { .. }
                    | InstructionKind::OwnedStoreReserve { .. }
                    | InstructionKind::OwnedStorePush { .. }
                    | InstructionKind::OwnedStorePop { .. }
                    | InstructionKind::OwnedStoreLen { .. }
                    | InstructionKind::OwnedStoreCapacity { .. }
                    | InstructionKind::OwnedStoreBorrow { .. }
                    | InstructionKind::OwnedStoreBorrowMut { .. } => {
                        super::pod_store::instruction(function, instruction, budget)?;
                    }
                    InstructionKind::Constant(Constant::WideString(units)) => {
                        budget.input(units.len(), "static UTF-16 units")?;
                    }
                    InstructionKind::ConvertNumber { value, target } => {
                        let source = function
                            .values
                            .get(value.0 as usize)
                            .ok_or_else(|| bad("numeric conversion value outside table"))?;
                        if !source.is_numeric()
                            || !target.is_numeric()
                            || !(source.is_float() || target.is_float())
                        {
                            return Err(bad(
                                "numeric conversion requires numeric endpoints and at least one float",
                            ));
                        }
                    }
                    InstructionKind::ConvertInteger { target, .. } if !target.is_integer() => {
                        return Err(bad("integer conversion target is not a supported integer"));
                    }
                    InstructionKind::Load(local)
                    | InstructionKind::RegisterDefer { local, .. }
                    | InstructionKind::Store { local, .. }
                    | InstructionKind::BorrowLocal { local }
                    | InstructionKind::BorrowLocalMut { local }
                    | InstructionKind::TakeLocal { local }
                    | InstructionKind::DescribeLocal { local }
                    | InstructionKind::ReadPackageField { local, .. }
                    | InstructionKind::DescribePackageField { local, .. }
                    | InstructionKind::TakePackageField { local, .. }
                    | InstructionKind::DropPackageField { local, .. }
                    | InstructionKind::StorePackageField { local, .. }
                    | InstructionKind::ProjectOwnedArrayField { local, .. }
                    | InstructionKind::TakeOwnedArrayElement { local, .. }
                    | InstructionKind::DropOwnedArrayElement { local, .. }
                    | InstructionKind::StoreOwnedArrayElement { local, .. }
                    | InstructionKind::BorrowOwnedArrayElement { local, .. }
                    | InstructionKind::BorrowPackageField { local, .. }
                    | InstructionKind::BindPackageCapture { local, .. }
                    | InstructionKind::EndPackageCapture { local, .. }
                    | InstructionKind::DropManaged { local }
                    | InstructionKind::DropErrorUnion { local }
                    | InstructionKind::DropPackage { local }
                    | InstructionKind::DropOwnedArray { local }
                    | InstructionKind::DropClosure { local }
                    | InstructionKind::CallClosureMut { callee: local, .. }
                    | InstructionKind::CallClosureShared { callee: local, .. }
                    | InstructionKind::StorageLive(local)
                    | InstructionKind::StorageDead(local)
                        if local.0 as usize >= function.locals.len() =>
                    {
                        return Err(bad("local ID outside table"));
                    }
                    InstructionKind::Call { arguments, .. }
                    | InstructionKind::ForeignCall { arguments, .. } => {
                        budget.input(arguments.len(), "call argument preflight")?;
                    }
                    InstructionKind::CallClosure { arguments, .. }
                    | InstructionKind::CallClosureMut { arguments, .. }
                    | InstructionKind::CallClosureShared { arguments, .. }
                    | InstructionKind::CallClosureOnce { arguments, .. }
                    | InstructionKind::CallClosureBorrowed { arguments, .. }
                    | InstructionKind::CallIndirect { arguments, .. } => {
                        budget.input(
                            arguments.len().saturating_add(1),
                            "closure call operand preflight",
                        )?;
                    }
                    InstructionKind::MakeAggregate { fields }
                    | InstructionKind::MakeManaged { fields }
                    | InstructionKind::MakePackage { fields }
                    | InstructionKind::MakeOwnedArray { elements: fields }
                    | InstructionKind::MakeVariant { fields, .. }
                    | InstructionKind::MakeOwnedTuple { fields } => {
                        budget.input(fields.len(), "aggregate operands")?;
                    }
                    InstructionKind::MakeClosure { captures, .. } => {
                        budget.input(captures.len(), "closure capture operand preflight")?;
                    }
                    InstructionKind::Unsupported(feature) => {
                        budget.spend(feature.len(), "instruction metadata")?;
                        return Err(vec![Diagnostic::unsupported(
                            Phase::Analyze,
                            feature,
                            Some(instruction.span),
                        )]);
                    }
                    _ => {}
                }
            }
            budget.input(1, "terminator preflight")?;
            let operands = match block.terminator {
                Terminator::Return(Some(_)) | Terminator::Jump(_) => 1,
                Terminator::Branch { .. } => 3,
                Terminator::Return(None) | Terminator::Unreachable | Terminator::Trap => 0,
            };
            budget.input(operands, "terminator operand preflight")?;
        }
        if sum_function && function.foreign.is_some() {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "ordinary owned Sum does not grant foreign ABI qualification",
                Some(function.span),
            )]);
        }
        if sum_function {
            if budget.sum_functions.is_empty() {
                budget.spend(module.functions.len(), "ordinary Sum function inventory")?;
                budget.sum_functions = vec![false; module.functions.len()];
            }
            budget.sum_functions[function_index] = true;
        }
        package |= sum_function;
        package_functions.push(package);
        if product_store {
            if budget.ordinary_store_functions.is_empty() {
                budget.spend(
                    module.functions.len(),
                    "ordinary Product Store function flags",
                )?;
                budget.ordinary_store_functions = vec![false; module.functions.len()];
            }
            budget.spend(1, "ordinary Product Store function flag")?;
            budget.ordinary_store_functions[function_index] = true;
        }
    }
    // All raw type and capture trees are now bounded. Admission must include
    // resource-only and unused Store occurrences, not just body operations.
    let store_profiles =
        super::dynamic_store::owned_capture_profiles(module, &owned_store_capture_roots, budget)?;
    for element in callback_store_elements {
        super::dynamic_store::callback_element(module, element, budget)?;
    }
    Ok((package_functions, store_profiles))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::ResultCategory;

    #[test]
    fn task_origin_inventory_observes_unused_resource_and_closure_trees() {
        // Inventory component evidence only; these raw tables are not accepted
        // source/modules and carry no Task provenance or cleanup permission.
        let task = Type::Task {
            result: Box::new(Type::I32),
            retained_life: zil_ir::RetainedLifeShape {
                binder_count: 0,
                ports: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
        };
        let nested = Type::ErrorUnion {
            error: Box::new(Type::Unit),
            result: Box::new(Type::Managed(Box::new(task.clone()))),
        };
        for capture in [false, true] {
            for ty in [
                task.clone(),
                nested.clone(),
                Type::Opaque {
                    identity: zil_base::NodeId(17),
                    hidden: Box::new(nested.clone()),
                },
            ] {
                let mut module = TypedModule {
                    semantic: None,
                    schema_version: SCHEMA_VERSION,
                    module_id: zil_base::ModuleId(0),
                    source_fingerprint: zil_base::Fingerprint::ZERO,
                    target: zil_base::TargetProfile::default(),
                    functions: vec![],
                    closures: vec![],
                    resources: vec![],
                    unsupported: vec![],
                    dyn_declarations: vec![],
                    dyn_interfaces: vec![],
                    dyn_vtables: vec![],
                    life_declarations: vec![],
                    life_instances: vec![],
                };
                if capture {
                    module.closures.push(zil_ir::ClosureDefinition {
                        storage_life: None,
                        id: zil_ir::ClosureId(0),
                        function: zil_ir::FunctionId(0),
                        captures: vec![ty],
                        parameters: vec![],
                        result: Type::Unit,
                        kind: zil_ir::CallableKind::FnOnce,
                        lending_captures: vec![],
                    });
                } else {
                    module.resources.push(zil_ir::ResourceDefinition {
                        name: "unused".into(),
                        payload: ty,
                        teardown: None,
                    });
                }
                let mut budget = Budget::new(&Limits::default());
                let _ = preflight(&module, &Limits::default(), &mut budget);
                assert!(budget.task_origin_roots, "capture={capture}");
            }
        }
    }

    #[test]
    fn arithmetic_overflow_is_a_limit_failure() {
        let mut budget = Budget::new(&Limits {
            max_analysis_steps: usize::MAX,
            ..Limits::default()
        });
        assert_eq!(
            budget.product(usize::MAX, 2, 1, "test").unwrap_err()[0].category,
            ResultCategory::ResourceLimit
        );
        assert_eq!(
            count(usize::MAX, 1, usize::MAX, "test").unwrap_err()[0].category,
            ResultCategory::ResourceLimit
        );
    }
}
