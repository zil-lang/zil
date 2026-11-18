//! Independent stored-target reconstruction. Sorted vectors and batch CFG
//! solving deliberately do not consume analyzer roots, summaries, or claims.
use super::*;
mod access_candidates;
mod after_state;
mod array_index;
mod array_snapshot;
#[cfg(test)]
mod borrowed_owned_entry_tests;
mod byte_queries;
mod call_results;
mod callable_cells;
#[path = "stored_flow/captured_owner.rs"]
mod captured_owner;
mod compound;
mod descriptor_access;
mod dyn_captures;
mod dyn_closed;
mod dyn_contracts;
mod dyn_leases;
mod dyn_ports;
mod dyn_providers;
mod dyn_returns;
mod finite_fields;
mod foreign_calls;
mod value_transport;

pub(super) fn inline_frame_cleanup(
    module: &TypedModule, function: &TypedFunction, ty: &Type, b: &mut Budget,
) -> Check<bool> {
    pure_copy::frame_protocol_aggregate(module, function, ty, b)
}

pub(crate) fn finite_field_borrow(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    finite_fields::operation(module, f, i, b)
}
pub(crate) fn finite_field_classes(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<Vec<u32>> {
    finite_fields::classes(module, f, i, b)
}

pub(crate) fn logical_dyn_capture_leaf(
    module: &TypedModule,
    closure: ClosureId,
    position: &StoragePosition,
    ty: &Type,
    b: &mut Budget,
) -> Check<bool> {
    dyn_captures::register_leaf(module, closure, position, ty, b)
}

pub(crate) fn context_capture_type(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
    let ty = if let Type::Address { inner, .. } = ty {
        inner.as_ref()
    } else {
        ty
    };
    let Type::OwnedDyn { interface } = ty else {
        return Ok(false);
    };
    if crate::dynamic_ports::interface(module, *interface, b)?
        .context
        .is_none()
    {
        return Ok(false);
    }
    crate::dynamic_dispatch::type_owned(module, *interface, b)?;
    Ok(true)
}
mod leaf_rows;
mod reference_cells;
pub(crate) fn reference_domain(module: Option<&TypedModule>, ty: &Type) -> Option<u32> {
    reference_cells::domain(module, ty)
}
mod live_words;
pub(super) mod owned_byte_views;
mod qualified_heads;
mod receiver;
mod relation_index;
pub(super) mod replacements;
#[cfg(test)]
mod result_row_tests;
mod returned_environment;
#[cfg(test)]
mod set_bits_tests;
mod ssa_loan_groups;
mod store_paths;
use live_words::WordRow;
#[cfg(test)]
mod ancestry_tests;
#[cfg(test)]
mod live_loan_cost_tests;
mod live_loan_ids;
#[cfg(test)]
mod loan_union_tests;
#[path = "stored_flow_pure_copy.rs"]
mod pure_copy;

// Class grounding may recognize only this independently authenticated origin.
pub(super) fn managed_package_field_borrow(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    pure_copy::managed_package_field_borrow(f, i, b)
}

pub(super) fn ordinary_error_borrow(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    pure_copy::ordinary_error_borrow(module, f, i, b)
}

pub(super) fn qualified_function(f: &TypedFunction, b: &mut Budget) -> Check<bool> {
    // A plain address-returning helper can project the caller's actual stored
    // pointee even when its own signature contains no qualified carrier.
    if matches!(f.return_type, Type::Address { .. }) {
        return Ok(true);
    }
    // Include the actual owned-result closure entry in this independent
    // ledger; this tag does not establish its signature or capture validity.
    if f.environment.is_some()
        && matches!(
            f.return_type,
            Type::Package { .. } | Type::Enum { .. } | Type::ErrorUnion { .. }
        )
    {
        return Ok(true);
    }

    b.spend(
        f.locals
            .len()
            .saturating_add(f.values.len())
            .saturating_add(1),
    )?;
    let mut pending: Vec<&Type> = f
        .locals
        .iter()
        .map(|local| &local.ty)
        .chain(&f.values)
        .chain(std::iter::once(&f.return_type))
        .collect();
    while let Some(ty) = pending.pop() {
        b.spend(1)?;
        if reference_cells::nested(ty) {
            return Ok(true);
        }
        if shared_array_value(ty) {
            return Ok(true);
        }
        if compound::slots(ty, b)?.is_some() {
            return Ok(true);
        }
        match ty {
            // Their actual capture/retained declarations can carry targets
            // outside this type spelling. Keep their functions in the proof.
            Type::BorrowedStorage { .. }
            | Type::Closure(_)
            | Type::Task { .. }
            | Type::BorrowedFunctionPointer { .. } => return Ok(true),
            Type::OwnedStore { .. } if b.store_descriptions() => return Ok(true),
            Type::Desc(payload)
                if b.store_descriptions()
                    && matches!(payload.as_ref(), Type::ByteBuffer | Type::Record { .. }) =>
            {
                return Ok(true);
            }
            Type::Address { inner, .. }
            | Type::Managed(inner)
            | Type::Desc(inner)
            | Type::OwnedStore { element: inner }
            | Type::Array { element: inner, .. }
            | Type::Slice { element: inner }
            | Type::Opaque { hidden: inner, .. } => {
                b.spend(1)?;
                pending.push(inner);
            }
            Type::Package { fields, .. } | Type::Record { fields, .. } => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, child)| child));
            }
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                pending.extend(fields);
            }
            Type::Enum { variants, .. } => {
                b.spend(variants.len())?;
                for (_, fields) in variants {
                    b.spend(fields.len())?;
                    pending.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                b.spend(2)?;
                pending.extend([error.as_ref(), result.as_ref()]);
            }
            _ => {}
        }
    }
    Ok(false)
}

mod anonymous_tuple;
pub(super) mod enum_address;
mod nominal;
pub(super) mod option_address;
mod readonly_recursion;
pub(super) mod retained;
mod static_borrows;
mod task_address;
mod task_failure;
pub(super) fn anonymous_tuple(ty: &Type, b: &mut Budget) -> Check<bool> {
    Ok(anonymous_tuple::layout(ty, b)?.is_some())
}
pub(super) fn anonymous_operation(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    anonymous_tuple::operation(f, i, b)
}
pub(super) fn borrowed_product_child(ty: &Type, b: &mut Budget) -> Check<bool> {
    compound::borrowed_child(ty, b)
}
pub(super) fn compound_operation(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    compound::operation(f, i, b)
}
pub(super) fn shared_tuple_value(
    module: &TypedModule,
    ty: &Type,
    span: Option<Span>,
    b: &mut Budget,
) -> Check<bool> {
    compound::shared_value(module, ty, span, b)
}
pub(super) fn compound_targets(ty: &Type, b: &mut Budget) -> Check<bool> {
    Ok(compound::slots(ty, b)?.is_some())
}

pub(super) fn failure_shape(ty: &Type, b: &mut Budget) -> Check<bool> {
    if !b.task_failure_carriers {
        return Ok(false);
    }
    let limits = b.limits.clone();
    task_failure::shape(ty, &limits, b)
}
pub(super) fn failure_borrow_only_unpack(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    if let InstructionKind::UnpackTuple { local, .. } = i.kind
        && crate::sums::owned(&f.locals[local.0 as usize].ty)
    {
        return Ok(false);
    }
    task_failure::unpack(f, i, b)
}
fn failure_tuple_drop(f: &TypedFunction, i: &Instruction, b: &mut Budget) -> Check<bool> {
    let InstructionKind::DropTuple { local } = i.kind else {
        return Ok(false);
    };
    if i.result.is_some()
        || !matches!(&f.locals[local.0 as usize].ty,Type::BorrowedStorage{storage,..} if matches!(storage.as_ref(),Type::Tuple(_)))
    {
        return Ok(false);
    }
    failure_shape(&f.locals[local.0 as usize].ty, b)
}
pub(super) fn failure_unpack(f: &TypedFunction, i: &Instruction, b: &mut Budget) -> Check<bool> {
    task_failure::unpack(f, i, b)
}
pub(super) fn failure_projection(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<Option<Type>> {
    task_failure::projection(f, i, b)
}

/// A complete inline Record contributes addresses, never an owning leaf.
/// Physical coverage and class mappings are authenticated by the shape pass.
pub(crate) fn shared_record_child(ty: &Type, b: &mut Budget) -> Check<bool> {
    let Type::BorrowedStorage { storage, life } = ty else {
        return Ok(false);
    };
    let Type::Record { fields, .. } = storage.as_ref() else {
        return Ok(false);
    };
    if life.slots.is_empty() || !life.retained.is_empty() {
        return Ok(false);
    }
    // Pay for the bounded tag checks before either complete row scan.
    b.spend(
        life.slots
            .len()
            .saturating_mul(2)
            .saturating_add(fields.len().saturating_mul(3)),
    )?;
    Ok(life.slots.iter().all(|slot| {
        slot.mode == StoredBorrowMode::SharedAddress
            && matches!(slot.path.as_slice(), [PlaceProjection::Field(_)])
    }) && fields.iter().all(|(_, field)| {
        let scalar = |ty: &Type| {
            matches!(
                ty,
                Type::Unit
                    | Type::Bool
                    | Type::I32
                    | Type::I8
                    | Type::I16
                    | Type::Isize
                    | Type::I64
                    | Type::U64
                    | Type::Usize
                    | Type::U8
                    | Type::U16
                    | Type::U32
                    | Type::F32
                    | Type::F64
            )
        };
        scalar(field) || matches!(field, Type::Address { mutable: false, inner } if scalar(inner))
    }))
}

/// Certificates come only from this kernel's completed actual pointer proof.
pub(super) fn package_return_input(
    function: FunctionId,
    value: ValueId,
    b: &mut Budget,
) -> Check<u32> {
    let count = b.package_return_calls.as_ref().map_or(0, BTreeMap::len);
    b.spend(
        count
            .checked_ilog2()
            .map_or(1, |n| n as usize + 2)
            .saturating_mul(3),
    )?;
    let found = b
        .package_return_calls
        .as_ref()
        .and_then(|rows| rows.get(&(function, value)))
        .copied();
    found.ok_or_else(|| {
        unsupported(
            "qualified Package return requires one actual whole shared input",
            None,
        )
    })
}

fn whole_package_source(f: &TypedFunction, source: &Source, b: &mut Budget) -> Check<bool> {
    b.spend(width(source).saturating_add(1))?;
    if source.retained_port.is_some()
        || !source.leaf.is_empty()
        || !source.inline.is_empty()
        || source.heap_element.is_some()
    {
        return Ok(false);
    }
    Ok(match source.base {
        HoldRoot::Local(local) => f.locals.get(local.0 as usize).is_some_and(|local|
            matches!(&local.ty, Type::BorrowedStorage { storage, .. } if matches!(storage.as_ref(), Type::Package { .. }))),
        HoldRoot::Parameter(position) => f.parameters.get(position as usize)
            .and_then(|local| f.locals.get(local.0 as usize))
            .is_some_and(|local| super::resources::qualified_package_address(&local.ty)),
        _ => false,
    })
}

// Reconstruct a plain Package field path from this function's actual local
// or address input. No type-only guess supplies a source or heap permission.
fn ordinary_inline_package_source(
    f: &TypedFunction,
    source: &Source,
    expected: &Type,
    b: &mut Budget,
) -> Check<bool> {
    source_work(std::slice::from_ref(source), b)?;
    if source.retained_port.is_some() || !source.leaf.is_empty() || source.heap_element.is_some() {
        return Ok(false);
    }
    let root = match source.base {
        HoldRoot::Local(local) => f.locals.get(local.0 as usize).map(|local| &local.ty),
        HoldRoot::Parameter(index) => f
            .parameters
            .get(index as usize)
            .and_then(|local| f.locals.get(local.0 as usize))
            .and_then(|local| {
                if let Type::Address { inner, .. } = &local.ty {
                    Some(inner.as_ref())
                } else {
                    None
                }
            }),
        _ => None,
    };
    let Some(mut actual) = root else {
        return Ok(false);
    };
    b.spend(source.inline.len())?;
    for projection in &source.inline {
        let (PlaceProjection::Field(index), Type::Package { fields, .. }) = (projection, actual)
        else {
            return Ok(false);
        };
        let Some((_, child)) = fields.get(*index as usize) else {
            return Ok(false);
        };
        actual = child;
    }
    type_work(actual, &b.limits.clone(), b)?;
    type_work(expected, &b.limits.clone(), b)?;
    Ok(matches!(actual, Type::Package { .. }) && actual == expected)
}

fn package_return_candidates(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
    values: &[Payload],
    summaries: &[Summary],
    b: &mut Budget,
) -> Check<BTreeMap<ValueId, u32>> {
    let mut result = BTreeMap::new();
    if (f.call_kind != CallKind::Direct && !crate::tasks::ordinary_sabi_body(f, b)?)
        || f.environment.is_some()
    {
        return Ok(result);
    }
    b.spend(f.blocks.len())?;
    for (block, live) in f.blocks.iter().zip(&graph.reachable) {
        if !live {
            continue;
        }
        b.spend(block.instructions.len())?;
        for instruction in &block.instructions {
            let InstructionKind::Call {
                function,
                arguments,
            } = &instruction.kind
            else {
                continue;
            };
            let callee = &module.functions[function.0 as usize];
            if callee.foreign.is_some()
                || callee.environment.is_some()
                || callee.call_kind != CallKind::Direct
                || !super::resources::qualified_package_address(&callee.return_type)
            {
                continue;
            }
            let prior = &summaries[function.0 as usize];
            if !prior.complete {
                continue;
            }
            b.spend(prior.result.len().saturating_add(1))?;
            let Some((_, roots)) = prior.result.iter().find(|(path, _)| path.is_empty()) else {
                continue;
            };
            b.spend(roots.len())?;
            let [root] = roots.as_slice() else {
                continue;
            };
            if !whole_package_source(callee, root, b)? {
                continue;
            }
            let HoldRoot::Parameter(position) = root.base else {
                continue;
            };
            let input = &callee.locals[callee.parameters[position as usize].0 as usize].ty;
            type_work(input, &b.limits.clone(), b)?;
            type_work(&callee.return_type, &b.limits.clone(), b)?;
            if input != &callee.return_type {
                continue;
            }
            let argument = *arguments
                .get(position as usize)
                .ok_or_else(|| err("Package return actual input absent"))?;
            let pointer = &values[argument.0 as usize].address.sources;
            if pointer.is_empty() {
                continue;
            }
            b.spend(pointer.len())?;
            let mut whole = true;
            for source in pointer {
                whole &= whole_package_source(f, source, b)?;
            }
            if !whole {
                continue;
            }
            let value = instruction
                .result
                .ok_or_else(|| err("Package call result absent"))?;
            b.spend(
                result
                    .len()
                    .checked_ilog2()
                    .map_or(1, |n| n as usize + 2)
                    .saturating_add(std::mem::size_of::<(ValueId, u32)>()),
            )?;
            result.insert(value, position);
        }
    }
    Ok(result)
}

pub(crate) fn shared_array_value(ty: &Type) -> bool {
    matches!(ty, Type::Array { element, length } if *length > 0
        && matches!(element.as_ref(), Type::Address { mutable: false, .. }))
}

pub(crate) fn shared_array_shape(ty: &Type, b: &mut Budget) -> Check<bool> {
    if !shared_array_value(ty) {
        return Ok(false);
    }
    let Type::Array { element, .. } = ty else {
        unreachable!()
    };
    let Type::Address { inner, .. } = element.as_ref() else {
        unreachable!()
    };
    type_work(inner, &b.limits.clone(), b)?;
    if !retained::closed_copy(inner, b)? {
        return Err(err(
            "shared array actual pointee is outside its finite profile",
        ));
    }
    Ok(true)
}

type Path = Vec<PlaceProjection>;
type WriteSites = Vec<(usize, u32, Path, usize, bool)>;
/// Typed opaque carrier namespace. A Task port never supplies a Dyn target.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum PortOrigin {
    // An ordered sequence of real pointer-cell reads. Each path selects the
    // physical cell before the next dereference. This is never a public
    // RetainedPort/Class selector, an owning edge or a synthesized GEP.
    Reference {
        steps: Vec<Path>,
    },
    Task {
        carrier: Vec<zil_ir::ContractProjection>,
        port: u32,
    },
    Dyn {
        carrier: Vec<ContractProjection>,
        port: u32,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum StaticProvenance {
    // No static evidence, including an unclassified value. This is never a
    // literal merely because a separate source category says Static.
    NotStatic,
    Literal,
    Conditional((ClosureId, StoragePosition)),
    // A canonical upper target of any base, including Parameter. Substitution
    // into an actual literal argument cannot upgrade this category to Literal.
    UpperOnly,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Source {
    // Conditional constant contents preserve the exact actual field invariant
    // through SSA, memory and call summaries until whole-module discharge.
    pub(super) static_dependency: StaticProvenance,
    pub(super) retained_port: Option<PortOrigin>,
    pub(super) base: HoldRoot,
    pub(super) leaf: Path,
    pub(super) inline: Path,
    // Ordered inline paths from each preceding element to its next Store head.
    pub(super) heap_parents: Vec<Path>,
    pub(super) heap_element: Option<Path>,
}
#[derive(Clone, Debug, Default)]
pub(super) struct StoreOptionFunctionSources {
    pub(super) values: Vec<Vec<Source>>,
    // Private proof: this value carries an actual ByteBufferBorrow loan.
    pub(super) byte_payloads: Vec<ValueId>,
}
pub(super) fn option_sources_snapshot(
    function: FunctionId,
    value: ValueId,
    budget: &mut Budget,
) -> Check<Vec<Source>> {
    fn lookup(b: &Budget, function: FunctionId, value: ValueId) -> Option<&Vec<Source>> {
        b.store_option_sources
            .as_ref()
            .and_then(|rows| rows.get(function.0 as usize))
            .and_then(|function| function.values.get(value.0 as usize))
    }
    let count = lookup(budget, function, value)
        .ok_or_else(|| err("Option value lacks actual stored source proof"))?
        .len();
    budget.spend(count)?;
    let cost = lookup(budget, function, value)
        .unwrap()
        .iter()
        .fold(0usize, |cost, source| cost.saturating_add(width(source)));
    budget.spend(cost)?;
    Ok(lookup(budget, function, value).unwrap().clone())
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Permission {
    sources: Vec<Source>,
    loans: Vec<usize>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Payload {
    enum_variants: enum_address::Variants,
    option_variants: option_address::Variants,
    address: Permission,
    store_description: Permission,
    task_ports: Vec<(Vec<ContractProjection>, Permission)>,
    dyn_ports: dyn_ports::Rows,
    dyn_views: Vec<dyn_ports::View>,
    creation_complete: Option<bool>,
    // For a concrete Fixed environment only, a private Field(capture) prefix
    // retains its actual outer address and pointee leaves. It grants no public
    // projection, is never a signature class selector, and is not serialized.
    leaves: leaf_rows::Rows,
    // Root-only facts of actual embedded occupants, keyed by physical place.
    // Logical class paths and Frame epochs are deliberately absent here.
    field_facts: Vec<(Path, value_transport::FieldFacts)>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Frame {
    pending_external_update: bool,
    receiver: Option<usize>,
    locals: Vec<Payload>,
    external: Vec<Payload>,
    captures: Vec<Payload>,
    // Actual input pointee memory, distinct from the pointer value itself.
    pointees: Vec<(HoldRoot, Payload)>,
    projected_pointees: Vec<value_transport::after::ProjectedPointee>,
    /// Union of paths left vacant by any incoming branch.
    vacant: Vec<Source>,
    prepared: Vec<replacements::Prepared>,
    reference_cells: reference_cells::Cells,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Loan {
    receiver_head: bool,
    sources: Vec<Source>,
    parents: Vec<usize>,
    exclusive: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Summary {
    // Scheduling only: a real reachable Dyn operation or a checked helper
    // dependency needs capture-aware actual access transport at its call.
    dyn_operations: bool,
    byte_reads: Vec<Source>,
    head_accesses: Vec<(Source, bool)>,
    description_accesses: Vec<descriptor_access::DescriptionAccess>,
    byte_covered: bool,
    dyn_accesses: Vec<(Source, bool)>,
    dyn_effects: Vec<DynEffect>,
    dyn_effects_known: bool,
    dyn_after: Vec<(u32, dyn_ports::Rows)>,
    // Actual Return snapshots for original contracts, including shared and
    // Task inputs. These facts never publish a caller permission update.
    contract_after: Vec<(u32, dyn_ports::Rows)>,
    dyn_capture_after: Vec<(u32, dyn_ports::Rows)>,
    result: Vec<(Path, Vec<Source>)>,
    // Actual non-root physical result ancestors, kept separate from data.
    physical_result_parents: Vec<(Path, Vec<Source>)>,
    field_result_facts: Vec<(Path, value_transport::ResultFacts)>,
    value_after: value_transport::After,
    receiver_results: Vec<Path>,
    dyn_result_parents: Vec<Source>,
    task_ports: Vec<(Vec<ContractProjection>, Vec<Source>)>,
    dyn_ports: dyn_ports::Rows,
    external_after: Vec<(u32, Path, Vec<Source>)>,
    reference_after: reference_cells::After,
    external_stable: Vec<(u32, Path)>,
    capture_after: Vec<(u32, Path, Vec<Source>)>,
    apart: Vec<(Source, Source)>,
    complete: bool,
    returned_enum_variants: enum_address::Variants,
    enum_values: Vec<(ValueId, enum_address::Variants)>,
    returned_variants: option_address::Variants,
    none_values: Vec<ValueId>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DynClassGrounding {
    Unknown,
    Static,
    NoTarget,
}
/// Private structural obligation, never deserialized or supplied by a Plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DynStaticObligation {
    pub function: FunctionId,
    pub class: SignatureLifeClass,
    pub binder: StorageBinder,
    pub ports: Vec<(Vec<ContractProjection>, Type, StoredBorrowMode)>,
    pub grounding: DynClassGrounding,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum TaskLoanAnchor {
    Value(ValueId),
    Input(zil_ir::StoragePosition),
    Reference(u32),
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct TaskPointerRoot {
    pub(super) base: HoldRoot,
    pub(super) dynamic: bool,
    pub(super) port: Option<u32>,
    pub(super) carrier: Vec<ContractProjection>,
    pub(super) reference: Option<u32>,
}
#[derive(Clone, Debug, Default)]
pub(super) struct TaskValueSources {
    pub(super) pointer: Vec<TaskPointerRoot>,
    pub(super) public: Vec<HoldRoot>,
    pub(super) retained: Vec<(Vec<ContractProjection>, Vec<TaskPointerRoot>)>,
    pub(super) pointer_parents: Vec<TaskLoanAnchor>,
    pub(super) payload_parents: Vec<TaskLoanAnchor>,
}
#[derive(Clone, Debug, Default)]
pub(super) struct TaskFunctionSources {
    /// False denotes an ordinary function omitted from stored solving.
    pub(super) covered: bool,
    pub(super) values: Vec<TaskValueSources>,
    // Exact incoming pointer-content coordinates, separate from slot heads.
    pub(super) reference_inputs: Vec<(Source, bool)>,
}
struct Solution {
    values: Vec<Payload>,
    outputs: Vec<Frame>,
    entry: Frame,
    loans: Vec<Loan>,
    sites: WriteSites,
    dyn_sites: dyn_leases::Sites,
}

fn err(text: &str) -> Diagnostic {
    reject(
        "E-VERIFY-STORED",
        None,
        format!("stored target proof: {text}"),
    )
}
fn amount(a: usize, b: usize) -> Check<usize> {
    a.checked_mul(b)
        .ok_or_else(|| limit("stored comparison work overflow"))
}
pub(crate) fn task_capture_port(
    module: &TypedModule,
    id: zil_ir::ClosureId,
    position: &StoragePosition,
    source: &Type,
    b: &mut Budget,
) -> Check<bool> {
    task_address::compatible_capture_port(module, id, position, source, b)
}
pub(crate) fn task_port_relation(
    module: &TypedModule,
    f: &TypedFunction,
    base: StorageBase,
    path: &[ContractProjection],
    b: &mut Budget,
) -> Check<StorageBinder> {
    task_address::port_relation(module, f, base, path, b)
}
pub(crate) fn task_carrier(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
    task_address::has_carrier(module, ty, b)
}
fn width(s: &Source) -> usize {
    let opaque = match &s.retained_port {
        Some(PortOrigin::Dyn { carrier, .. } | PortOrigin::Task { carrier, .. }) => carrier.len(),
        Some(PortOrigin::Reference { steps }) => steps
            .iter()
            .fold(steps.len(), |n, path| n.saturating_add(path.len())),
        _ => 0,
    };
    s.leaf
        .len()
        .saturating_add(s.inline.len())
        .saturating_add(opaque)
        .saturating_add(match &s.static_dependency {
            StaticProvenance::NotStatic => 0,
            StaticProvenance::Literal | StaticProvenance::UpperOnly => 1,
            StaticProvenance::Conditional((_, p)) => p.path.len().saturating_add(2),
        })
        .saturating_add(s.heap_element.as_ref().map_or(0, Vec::len))
        .saturating_add(s.heap_parents.len())
        .saturating_add(
            s.heap_parents
                .iter()
                .fold(0usize, |n, path| n.saturating_add(path.len())),
        )
        .saturating_add(1)
}
pub(super) fn nested_store_element<'t>(
    outer_element: &'t Type,
    source: &Source,
    b: &mut Budget,
) -> Check<&'t Type> {
    if source.heap_element.is_none() && !source.heap_parents.is_empty() {
        return Err(err(
            "Store source has parent heads without an actual element",
        ));
    }
    store_paths::deepest_element(outer_element, &source.heap_parents, b)
}
pub(super) fn nested_store_endpoint<'t>(
    outer_element: &'t Type,
    source: &Source,
    b: &mut Budget,
) -> Check<&'t Type> {
    let element = nested_store_element(outer_element, source, b)?;
    let path = source
        .heap_element
        .as_ref()
        .ok_or_else(|| err("Store source lacks its actual payload domain"))?;
    store_paths::project(element, path, b)
}
fn source_work(s: &[Source], b: &mut Budget) -> Check<()> {
    b.spend(s.len())?;
    for x in s {
        b.spend(width(x))?;
    }
    Ok(())
}
fn permission_work(p: &Permission, b: &mut Budget) -> Check<()> {
    source_work(&p.sources, b)?;
    b.spend(p.loans.len())
}
fn payload_work(p: &Payload, b: &mut Budget) -> Check<()> {
    value_transport::work(p, b)?;
    p.enum_variants.work(b)?;
    p.option_variants.work(b)?;
    permission_work(&p.address, b)?;
    permission_work(&p.store_description, b)?;
    task_address::work(p, b)?;
    if b.dyn_contexts {
        dyn_ports::work(&p.dyn_ports, b)?;
        dyn_ports::views_work(&p.dyn_views, b)?;
    }
    p.leaves.copy_work(b)?;
    b.spend(p.leaves.len())?;
    for (path, p) in &p.leaves {
        b.spend(path.len())?;
        permission_work(p, b)?;
    }
    Ok(())
}
pub(super) fn memory_holder<'a>(
    state: &'a Frame,
    base: HoldRoot,
    b: &mut Budget,
) -> Check<Option<&'a Payload>> {
    b.spend(state.pointees.len().saturating_add(1))?;
    if let Some((_, payload)) = state.pointees.iter().find(|(key, _)| *key == base) {
        return Ok(Some(payload));
    }
    Ok(match base {
        HoldRoot::Local(local) => state.locals.get(local.0 as usize),
        HoldRoot::Parameter(index) => state.external.get(index as usize),
        HoldRoot::Capture(index) => state.captures.get(index as usize),
        HoldRoot::Static => None,
    })
}
pub(super) fn memory_holder_mut<'a>(
    state: &'a mut Frame,
    base: HoldRoot,
    b: &mut Budget,
) -> Check<Option<&'a mut Payload>> {
    b.spend(state.pointees.len().saturating_add(1))?;
    if let Some(index) = state.pointees.iter().position(|(key, _)| *key == base) {
        return Ok(Some(&mut state.pointees[index].1));
    }
    Ok(match base {
        HoldRoot::Local(local) => state.locals.get_mut(local.0 as usize),
        HoldRoot::Parameter(index) => state.external.get_mut(index as usize),
        HoldRoot::Capture(index) => state.captures.get_mut(index as usize),
        HoldRoot::Static => None,
    })
}
fn frame_work(f: &Frame, b: &mut Budget) -> Check<()> {
    b.spend(f.pointees.len())?;
    for (_, payload) in &f.pointees {
        payload_work(payload, b)?;
    }
    value_transport::after::projected_work(&f.projected_pointees,b)?;
    source_work(&f.vacant, b)?;
    replacements::frame_work(&f.prepared, b)?;
    reference_cells::work(&f.reference_cells, b)?;
    b.spend(
        f.locals
            .len()
            .saturating_add(f.external.len())
            .saturating_add(f.captures.len()),
    )?;
    for p in f.locals.iter().chain(&f.external).chain(&f.captures) {
        payload_work(p, b)?;
    }
    Ok(())
}
fn copy_payload(p: &Payload, b: &mut Budget) -> Check<Payload> {
    payload_work(p, b)?;
    Ok(p.clone())
}
fn copy_frame(f: &Frame, b: &mut Budget) -> Check<Frame> {
    frame_work(f, b)?;
    if let Some(progress) = &mut b.diagnostic_progress {
        progress.stored_frame_clones = progress.stored_frame_clones.saturating_add(1);
    }
    Ok(f.clone())
}

/// Physical identity is independent of the private evidence carried along a
/// conservative target. Only provenance sets distinguish those evidence rows.
fn same_target(a: &Source, z: &Source, b: &mut Budget) -> Check<bool> {
    b.spend(width(a).saturating_add(width(z)))?;
    Ok(a.base == z.base
        && a.leaf == z.leaf
        && a.inline == z.inline
        && a.retained_port == z.retained_port
        && a.heap_element == z.heap_element
        && a.heap_parents == z.heap_parents)
}
fn contains_target(sources: &[Source], target: &Source, b: &mut Budget) -> Check<bool> {
    b.spend(sources.len())?;
    for source in sources {
        if same_target(source, target, b)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn add_sources(to: &mut Vec<Source>, from: &[Source], b: &mut Budget) -> Check<bool> {
    let mut changed = false;
    for source in from {
        b.spend(amount(to.len().saturating_add(1), width(source))?)?;
        if !to.contains(source) {
            b.spend(width(source))?;
            to.push(source.clone());
            changed = true;
        }
    }
    // Canonical order is not an authority; pay all possible full comparisons.
    b.spend(to.len())?;
    let max = to.iter().map(width).max().unwrap_or(1);
    b.spend(amount(amount(to.len(), to.len())?, max)?)?;
    to.sort();
    Ok(changed)
}
fn sort_loan_ids(to: &mut [usize], b: &mut Budget) -> Check<()> {
    // Most callers already produced canonical rows. Check the actual row,
    // rather than assuming an invariant for every internal construction path.
    let mut ordered = true;
    for pair in to.windows(2) {
        b.spend(1)?;
        if pair[0] > pair[1] {
            ordered = false;
            break;
        }
    }
    if ordered {
        return Ok(());
    }
    b.spend(to.len().saturating_add(1))?;
    let mut scratch = vec![0; to.len()];
    let mut run = 1usize;
    while run < to.len() {
        let mut start = 0usize;
        while start < to.len() {
            let mid = start.saturating_add(run).min(to.len());
            let end = mid.saturating_add(run).min(to.len());
            let (mut left, mut right, mut out) = (start, mid, start);
            while left < mid && right < end {
                b.spend(2)?; // one actual integer comparison and output cell
                if to[left] <= to[right] {
                    scratch[out] = to[left];
                    left += 1;
                } else {
                    scratch[out] = to[right];
                    right += 1;
                }
                out += 1;
            }
            b.spend((mid - left).saturating_add(end - right))?;
            scratch[out..out + mid - left].copy_from_slice(&to[left..mid]);
            out += mid - left;
            scratch[out..out + end - right].copy_from_slice(&to[right..end]);
            start = end;
        }
        b.spend(to.len())?;
        to.copy_from_slice(&scratch);
        run = run.saturating_mul(2);
    }
    Ok(())
}
fn add_loans(to: &mut Vec<usize>, from: &[usize], b: &mut Budget) -> Check<bool> {
    if let Some(progress) = &mut b.diagnostic_progress {
        progress.loan_unions = progress.loan_unions.saturating_add(1);
        if from.is_empty() {
            progress.empty_loan_unions = progress.empty_loan_unions.saturating_add(1);
        }
    }
    b.spend(1)?;
    sort_loan_ids(to, b)?;
    let mut changed = false;
    for id in from {
        let (mut low, mut high) = (0usize, to.len());
        let mut found = false;
        while low < high {
            b.spend(1)?;
            let mid = low + (high - low) / 2;
            match to[mid].cmp(id) {
                std::cmp::Ordering::Less => low = mid + 1,
                std::cmp::Ordering::Greater => high = mid,
                std::cmp::Ordering::Equal => {
                    found = true;
                    break;
                }
            }
        }
        if !found {
            b.spend(to.len().saturating_sub(low).saturating_add(1))?;
            if to.len() == to.capacity() {
                b.spend(to.len().saturating_add(1))?;
            }
            to.insert(low, *id);
            changed = true;
        }
    }
    Ok(changed)
}
#[cfg(test)]
fn add_loans_by_scan(to: &mut Vec<usize>, from: &[usize], b: &mut Budget) -> Check<bool> {
    let mut changed = false;
    b.spend(amount(
        to.len().saturating_add(from.len()).saturating_add(1),
        from.len().saturating_add(1),
    )?)?;
    for id in from {
        if !to.contains(id) {
            to.push(*id);
            changed = true;
        }
    }
    to.sort_unstable();
    Ok(changed)
}
fn join_permission(to: &mut Permission, from: &Permission, b: &mut Budget) -> Check<bool> {
    let a = add_sources(&mut to.sources, &from.sources, b)?;
    Ok(add_loans(&mut to.loans, &from.loans, b)? || a)
}
fn leaf<'a>(
    p: &'a Payload,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<Option<&'a Permission>> {
    Ok(p.leaves.position(path, b)?.map(|row| &p.leaves[row].1))
}
fn add_leaf(
    to: &mut Payload,
    path: &[PlaceProjection],
    from: &Permission,
    b: &mut Budget,
) -> Check<bool> {
    if let Some(row) = to.leaves.position(path, b)? {
        join_permission(to.leaves.permission_mut(row), from, b)
    } else {
        b.spend(path.len())?;
        permission_work(from, b)?;
        to.leaves.push((path.to_vec(), from.clone()), b)?;
        Ok(true)
    }
}
fn join_payload(to: &mut Payload, from: &Payload, b: &mut Budget) -> Check<bool> {
    let mut changed = join_permission(&mut to.address, &from.address, b)?;
    // Description transfers independently establish the real manager root.
    // Ordinary Store/ByteBuffer requests need the same fixed-point channel as
    // Option and Task requests, including every existing parent loan.
    changed |= join_permission(&mut to.store_description, &from.store_description, b)?;
    changed |= task_address::join(to, from, b)?;
    changed |= dyn_ports::join(to, from, b)?;
    if let Some(complete) = from.creation_complete {
        let joined = Some(to.creation_complete.unwrap_or(false) || complete);
        changed |= to.creation_complete != joined;
        to.creation_complete = joined;
    }
    changed |= value_transport::join(to, from, b)?;
    changed |= to.enum_variants.join(&from.enum_variants, b)?;
    changed |= to.option_variants.join(&from.option_variants, b)?;
    for (path, p) in &from.leaves {
        changed |= add_leaf(to, path, p, b)?;
    }
    Ok(changed)
}
fn join_frame(to: &mut Frame, from: &Frame, b: &mut Budget) -> Check<()> {
    if let Some(progress) = &mut b.diagnostic_progress {
        progress.stored_frame_joins = progress.stored_frame_joins.saturating_add(1);
    }
    // This flag records unresolved operations in this block, not memory
    // carried by an incoming edge. Every reachable block is checked by
    // summarize. Propagating a transient bottom-call debt around a back edge
    // would rotate it forever after the actual value facts have converged.
    if let Some(head) = from.receiver {
        b.spend(1)?;
        if to.receiver.is_some_and(|previous| previous != head) {
            return Err(err(
                "different actual invocation heads met within one entry",
            ));
        }
        to.receiver = Some(head);
    }
    add_sources(&mut to.vacant, &from.vacant, b)?;
    replacements::join(&mut to.prepared, &from.prepared, b)?;
    reference_cells::join(&mut to.reference_cells, &from.reference_cells, b)?;
    b.spend(
        to.locals
            .len()
            .saturating_add(to.external.len())
            .saturating_add(to.captures.len()),
    )?;
    b.spend(to.pointees.len().saturating_add(from.pointees.len()))?;
    if to.pointees.len() != from.pointees.len() {
        return Err(err(
            "input pointee memory directories differ across actual CFG flow",
        ));
    }
    for ((a, left), (z, right)) in to.pointees.iter_mut().zip(&from.pointees) {
        if a != z {
            return Err(err("input pointee memory selects another original holder"));
        }
        join_payload(left, right, b)?;
    }
    value_transport::after::join_projected(&mut to.projected_pointees,&from.projected_pointees,b)?;
    for (a, z) in to
        .locals
        .iter_mut()
        .zip(&from.locals)
        .chain(to.external.iter_mut().zip(&from.external))
        .chain(to.captures.iter_mut().zip(&from.captures))
    {
        join_payload(a, z, b)?;
    }
    Ok(())
}
fn qualified(t: &Type) -> Option<(&Type, &StorageLifeShape)> {
    if let Type::BorrowedStorage { storage, life } = t {
        Some((storage, life))
    } else {
        None
    }
}
fn exclusive_carrier_value(ty: &Type) -> bool {
    match ty {
        Type::BorrowedStorage { storage, life } => {
            life.slots
                .iter()
                .any(|slot| slot.mode == StoredBorrowMode::ExclusiveAddress)
                || exclusive_carrier_value(storage)
        }
        Type::Tuple(fields) => fields.iter().any(exclusive_carrier_value),
        Type::Record { fields, .. } | Type::Package { fields, .. } => fields
            .iter()
            .any(|(_, field)| exclusive_carrier_value(field)),
        Type::Array { element, .. } => exclusive_carrier_value(element),
        Type::Enum { variants, .. } => variants
            .iter()
            .any(|(_, fields)| fields.iter().any(exclusive_carrier_value)),
        Type::ErrorUnion { error, result } => {
            exclusive_carrier_value(error) || exclusive_carrier_value(result)
        }
        _ => false,
    }
}
fn affine_stored_input(ty: &Type) -> bool {
    match ty {
        Type::Address { mutable: true, .. } => true,
        Type::Tuple(fields) => fields.iter().any(affine_stored_input),
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            fields.iter().any(|(_, field)| affine_stored_input(field))
        }
        Type::Array { element, .. } => affine_stored_input(element),
        Type::Enum { variants, .. } => variants
            .iter()
            .any(|(_, fields)| fields.iter().any(affine_stored_input)),
        Type::ErrorUnion { error, result } => {
            affine_stored_input(error) || affine_stored_input(result)
        }
        _ => exclusive_carrier_value(ty),
    }
}
// Address arguments to ordinary calls are reborrows. Only actual storage
// transfers consume that SSA address; carrier values retain their move rules.
fn transfers_address(kind: &InstructionKind) -> bool {
    matches!(
        kind,
        InstructionKind::Store { .. }
            | InstructionKind::MakeAggregate { .. }
            | InstructionKind::MakePackage { .. }
            | InstructionKind::MakeManaged { .. }
            | InstructionKind::MakeOwnedTuple { .. }
            | InstructionKind::MakeVariant { .. }
            | InstructionKind::MakeOwnedArray { .. }
            | InstructionKind::MakeClosure { .. }
            | InstructionKind::MakeErrorUnion { .. }
            | InstructionKind::ExtractField { .. }
            | InstructionKind::ExtractIndex { .. }
            | InstructionKind::UpdateField { .. }
            | InstructionKind::UpdateIndex { .. }
            | InstructionKind::StorePackageField { .. }
            | InstructionKind::StoreOwnedArrayElement { .. }
            | InstructionKind::StoreStoredBorrow { .. }
            | InstructionKind::CommitPlaceReplace { .. }
    )
}
fn exclusive_consumers(kind: &InstructionKind) -> (Option<ValueId>, &[ValueId]) {
    match kind {
        InstructionKind::Store { value, .. }
        | InstructionKind::StorePackageField { value, .. }
        | InstructionKind::StoreOwnedArrayElement { value, .. }
        | InstructionKind::StoreStoredBorrow { value, .. }
        | InstructionKind::CommitPlaceReplace { value, .. }
        | InstructionKind::MakeErrorUnion { value, .. }
        | InstructionKind::MakeDynOwned { source: value, .. }
        | InstructionKind::ExtractField {
            aggregate: value, ..
        }
        | InstructionKind::ExtractIndex {
            aggregate: value, ..
        } => (Some(*value), &[]),
        InstructionKind::UpdateField {
            aggregate, value, ..
        }
        | InstructionKind::UpdateIndex {
            aggregate, value, ..
        } => (Some(*aggregate), std::slice::from_ref(value)),
        InstructionKind::MakeAggregate { fields }
        | InstructionKind::MakePackage { fields }
        | InstructionKind::MakeManaged { fields }
        | InstructionKind::MakeOwnedTuple { fields }
        | InstructionKind::MakeVariant { fields, .. }
        | InstructionKind::MakeOwnedArray { elements: fields }
        | InstructionKind::MakeClosure {
            captures: fields, ..
        }
        | InstructionKind::Call {
            arguments: fields, ..
        }
        | InstructionKind::CallIndirect {
            arguments: fields, ..
        }
        | InstructionKind::CallDynSlot {
            arguments: fields, ..
        }
        | InstructionKind::CallClosure {
            arguments: fields, ..
        }
        | InstructionKind::CallClosureOnce {
            arguments: fields, ..
        }
        | InstructionKind::CallClosureBorrowed {
            arguments: fields, ..
        }
        | InstructionKind::CallClosureShared {
            arguments: fields, ..
        }
        | InstructionKind::CallClosureMut {
            arguments: fields, ..
        }
        | InstructionKind::ForeignCall {
            arguments: fields, ..
        }
        | InstructionKind::SuspendCall {
            arguments: fields, ..
        }
        | InstructionKind::TaskCreate {
            arguments: fields, ..
        }
        | InstructionKind::TaskCreateOwned {
            arguments: fields, ..
        } => (None, fields),
        _ => (None, &[]),
    }
}
fn addressed(t: &Type) -> Option<(&Type, &StorageLifeShape)> {
    if let Type::Address { inner, .. } = t {
        qualified(inner)
    } else {
        None
    }
}
fn vt(f: &TypedFunction, v: ValueId) -> Check<&Type> {
    f.values
        .get(v.0 as usize)
        .ok_or_else(|| err("operand outside actual value table"))
}

pub(super) fn type_work(ty: &Type, limits: &Limits, b: &mut Budget) -> Check<()> {
    type_visit(ty, limits, true, b)
}
/// Bounds-only traversal does not copy or compare name bytes. Completed pure
/// walks may be retained only for exact live immutable module/owned nodes.
pub(super) fn type_bounds(ty: &Type, limits: &Limits, b: &mut Budget) -> Check<()> {
    if dynamic_metadata::stored_type_known(ty, limits, b)? {
        return Ok(());
    }
    type_visit(ty, limits, false, b)?;
    dynamic_metadata::stored_type_success(ty, limits, b)
}
fn type_visit(ty: &Type, limits: &Limits, copy_or_compare: bool, b: &mut Budget) -> Check<()> {
    b.spend(1)?;
    let mut stack = vec![(ty, 0usize)];
    while let Some((t, depth)) = stack.pop() {
        b.spend(3)?;
        if depth > limits.max_call_depth {
            return Err(limit("stored type depth exceeds verification limit"));
        }
        match t {
            Type::Task { .. } => super::tasks::charge_result_context(t, b)?,
            Type::BorrowedStorage { storage, life } => {
                b.spend(
                    life.slots
                        .len()
                        .saturating_add(life.retained.len())
                        .saturating_add(life.nominal_arguments.len()),
                )?;
                for x in &life.slots {
                    b.spend(x.path.len())?;
                }
                for x in &life.retained {
                    b.spend(x.path.len())?;
                }
                for x in &life.nominal_arguments {
                    b.spend(x.path.len())?;
                }
                stack.push((storage, depth + 1));
            }
            Type::Record { name, fields } | Type::Package { name, fields } => {
                b.spend(
                    (if copy_or_compare { name.len() } else { 1 }).saturating_add(fields.len()),
                )?;
                for (n, t) in fields {
                    b.spend(if copy_or_compare { n.len() } else { 1 })?;
                    stack.push((t, depth + 1));
                }
            }
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                for t in fields {
                    stack.push((t, depth + 1));
                }
            }
            Type::Enum { name, variants } => {
                b.spend(
                    (if copy_or_compare { name.len() } else { 1 }).saturating_add(variants.len()),
                )?;
                for (n, fs) in variants {
                    b.spend((if copy_or_compare { n.len() } else { 1 }).saturating_add(fs.len()))?;
                    for t in fs {
                        stack.push((t, depth + 1));
                    }
                }
            }
            Type::AsyncRegistration { environment: inner }
            | Type::Address { inner, .. }
            | Type::Managed(inner)
            | Type::Desc(inner)
            | Type::Array { element: inner, .. }
            | Type::Slice { element: inner } => stack.push((inner, depth + 1)),
            Type::ErrorUnion { error, result } => {
                b.spend(2)?;
                stack.push((error, depth + 1));
                stack.push((result, depth + 1));
            }
            Type::BorrowedFunctionPointer {
                parameters,
                result,
                life,
            } => {
                crate::signature_life::charge(life, b)?;
                b.spend(parameters.len().saturating_add(1))?;
                stack.extend(parameters.iter().map(|ty| (ty, depth + 1)));
                stack.push((result, depth + 1));
            }
            Type::FunctionPointer { parameters, result } => {
                b.spend(parameters.len().saturating_add(1))?;
                for t in parameters {
                    stack.push((t, depth + 1));
                }
                stack.push((result, depth + 1));
            }
            Type::ForeignCallback { parameters, result } => {
                b.spend(parameters.len().saturating_add(1))?;
                stack.extend(parameters.iter().map(|ty| (ty, depth + 1)));
                stack.push((result, depth + 1));
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn borrowed_callback_domain(id: ClosureId, b: &mut Budget) -> Check<Option<u32>> {
    b.spend(1)?;
    Ok(b.structural.as_ref().and_then(|directory| directory.borrowed_callback_domain(id)))
}

pub(super) fn physical(ty: &Type, limits: &Limits, b: &mut Budget) -> Check<bool> {
    let Some((storage, shape)) = qualified(ty) else {
        return Ok(false);
    };
    type_bounds(ty, limits, b)?;
    if anonymous_tuple::layout(ty, b)?.is_some() {
        return Ok(true);
    }
    if b.task_failure_carriers && task_failure::shape(ty, limits, b)? {
        return Ok(true);
    }
    if option_address::shape(ty, limits, b)? {
        return Ok(true);
    }
    if enum_address::shape(ty, limits, b)? {
        return Ok(true);
    }
    if !matches!(
        storage,
        Type::Record { .. } | Type::Package { .. } | Type::Tuple(_) | Type::Enum { .. }
    ) || (matches!(storage, Type::Enum { .. })
        && (!shape.slots.is_empty() || !retained::pure(storage, b)?))
    {
        return Err(unsupported(
            "nested retained/nominal stored carrier needs its independent consumer",
            None,
        ));
    }
    b.spend(1)?;
    let mut todo = vec![(storage, Vec::new())];
    let mut actual = Vec::new();
    while let Some((t, path)) = todo.pop() {
        b.spend(1)?;
        match t {
            Type::Address { mutable, inner } => {
                type_bounds(inner, limits, b)?;
                if !inner.is_address_pointee()
                    && !(b.raw_reference_cells && reference_cells::nested(t))
                    && !(qualified(inner).is_some() && physical(inner, limits, b)?)
                {
                    return Err(err("stored shared leaf has invalid actual pointee"));
                }
                b.spend(path.len().saturating_add(1))?;
                actual.push((
                    path,
                    if *mutable {
                        StoredBorrowMode::ExclusiveAddress
                    } else {
                        StoredBorrowMode::SharedAddress
                    },
                    None,
                ));
            }
            Type::Record { name, fields } | Type::Package { name, fields } => {
                b.spend(name.len())?;
                if name.is_empty() {
                    return Err(err("stored record lacks actual nominal identity"));
                }
                let mut names: Vec<&str> = Vec::new();
                for (name, _) in fields {
                    b.spend(amount(
                        names.len().saturating_add(1),
                        name.len().saturating_add(1),
                    )?)?;
                    if name.is_empty() || names.contains(&name.as_str()) {
                        return Err(err("stored actual fields have duplicate/empty names"));
                    }
                    names.push(name.as_str());
                }
                for (i, (_, child)) in fields.iter().enumerate().rev() {
                    b.spend(path.len().saturating_add(1))?;
                    let mut p = path.clone();
                    p.push(PlaceProjection::Field(i as u32));
                    todo.push((child, p));
                }
            }
            Type::Tuple(fields) => {
                for (i, child) in fields.iter().enumerate().rev() {
                    b.spend(path.len().saturating_add(1))?;
                    let mut p = path.clone();
                    p.push(PlaceProjection::Field(i as u32));
                    todo.push((child, p));
                }
            }
            Type::Enum { name, variants } => {
                if !retained::pure(t, b)? {
                    return Err(err("stored Enum has a real address or ownership payload"));
                }
                b.spend(name.len())?;
                if name.is_empty() {
                    return Err(err("stored Enum lacks actual nominal identity"));
                }
                let mut names = Vec::<&str>::new();
                for (variant, (variant_name, fields)) in variants.iter().enumerate().rev() {
                    b.spend(amount(
                        names.len().saturating_add(1),
                        variant_name.len().saturating_add(1),
                    )?)?;
                    if variant_name.is_empty() || names.contains(&variant_name.as_str()) {
                        return Err(err(
                            "stored actual Enum variants have duplicate or empty names",
                        ));
                    }
                    names.push(variant_name);
                    for (index, child) in fields.iter().enumerate().rev() {
                        b.spend(path.len().saturating_add(1))?;
                        let mut p = path.clone();
                        p.push(PlaceProjection::Variant(zil_ir::VariantProjection {
                            variant: variant as u32,
                            index: index as u32,
                        }));
                        todo.push((child, p));
                    }
                }
            }
            Type::Array { element, length } => {
                if shared_array_shape(t, b)? {
                    b.spend((*length as usize).saturating_mul(path.len().saturating_add(2)))?;
                    for index in (0..*length).rev() {
                        let mut complete = path.clone();
                        complete.push(PlaceProjection::Field(index));
                        todo.push((element, complete));
                    }
                    continue;
                }
                if !retained::pure(t, b)? {
                    return Err(err("stored Array contains an address or ownership leaf"));
                }
                todo.push((element, path));
            }
            Type::BorrowedStorage {
                storage: child,
                life,
            } => {
                if !matches!(storage, Type::Package { .. })
                    || (!matches!(child.as_ref(), Type::Package { .. })
                        && !shared_record_child(t, b)?)
                {
                    if !retained::pure(t, b)? {
                        return Err(err("nested physical child requires owning Package"));
                    }
                    continue;
                }
                physical(t, limits, b)?;
                let boundary: Vec<_> = path
                    .iter()
                    .copied()
                    .map(LifeTypeProjection::Place)
                    .collect();
                for slot in &life.slots {
                    b.spend(path.len().saturating_add(slot.path.len()).saturating_add(1))?;
                    let mut complete = path.clone();
                    complete.extend(&slot.path);
                    actual.push((complete, slot.mode, Some((boundary.clone(), slot.binder))));
                }
            }
            Type::Managed(payload) if matches!(storage, Type::Package { .. }) => {
                type_bounds(payload, limits, b)?;
                if !matches!(
                    payload.as_ref(),
                    Type::ByteBuffer | Type::Record { .. } | Type::OwnedStore { .. }
                ) {
                    return Err(err(
                        "stored Package actual owner is outside finite resource profile",
                    ));
                }
            }
            Type::Closure(id)
                if matches!(storage, Type::Package { .. })
                    && matches!(path.as_slice(), [PlaceProjection::Field(_)])
                    && borrowed_callback_domain(*id, b)?.is_some() =>
            {
                // The physical callback boundary has its own class directory.
                // Its captured address lives inside that environment.
            }
            child => {
                type_bounds(child, limits, b)?;
                if !child.is_copy_value() {
                    return Err(unsupported(
                        "nonshared actual stored leaf needs its physical consumer",
                        None,
                    ));
                }
            }
        }
    }
    if actual.len() != shape.slots.len()
        || (actual.is_empty() && shape.nominal_arguments.is_empty() && shape.retained.is_empty())
    {
        return Err(err("stored physical leaf coverage is incomplete"));
    }
    let mut classes = Vec::new();
    let mut class = |id: u32, b: &mut Budget| -> Check<()> {
        b.spend(classes.len().saturating_add(1))?;
        if id >= shape.binder_count {
            return Err(err("stored class is outside declared domain"));
        }
        if !classes.contains(&id) {
            if id as usize != classes.len() {
                return Err(err(
                    "stored classes are not dense by first actual occurrence",
                ));
            }
            classes.push(id);
        }
        Ok(())
    };
    for row in &shape.nominal_arguments {
        b.spend(row.path.len().saturating_add(1))?;
        class(row.binder, b)?;
    }
    for ((path, mode, child_class), row) in actual.iter().zip(&shape.slots) {
        b.spend(path.len().saturating_add(row.path.len()).saturating_add(1))?;
        if path != &row.path || mode != &row.mode {
            return Err(err(
                "actual physical stored path or mode differs from declared leaf",
            ));
        }
        b.stored_exclusive_shapes |= *mode == StoredBorrowMode::ExclusiveAddress;
        if let Some((boundary, class)) = child_class
            && !shape.retained.iter().any(|entry| {
                &entry.path == boundary && entry.class == *class && entry.binder == row.binder
            })
        {
            return Err(err("nested slot binder differs from retained child class"));
        }
        if *mode == StoredBorrowMode::ExclusiveAddress
            && (!matches!(storage, Type::Record { .. } | Type::Package { .. })
                || !path
                    .iter()
                    .all(|part| matches!(part, PlaceProjection::Field(_)))
                || (child_class.is_none() && path.len() != 1))
        {
            return Err(unsupported(
                "exclusive stored holder requires direct nominal address slots",
                None,
            ));
        }
        class(row.binder, b)?;
    }
    let actual_retained = retained::expected(storage, limits, b)?;
    b.spend(shape.retained.len())?;
    b.spend(shape.retained.len())?;
    let physical_count = shape.retained.iter().take_while(|row| {
        !matches!(row.path.first(), Some(LifeTypeProjection::TypeArgument(_)))
    }).count();
    if actual_retained.len() != physical_count
        || physical_count != shape.retained.len() && !matches!(storage, Type::Package { .. })
    {
        return Err(err("physical retained class coverage differs"));
    }
    for ((path, local), row) in actual_retained.iter().zip(&shape.retained) {
        b.spend(path.len().saturating_add(row.path.len()).saturating_add(1))?;
        if path != &row.path || *local != row.class {
            return Err(err(
                "retained row differs from actual physical boundary/class",
            ));
        }
        class(row.binder, b)?;
    }
    for row in &shape.retained[physical_count..] {
        b.spend(row.path.len().saturating_add(1))?;
        if !matches!(row.path.as_slice(), [LifeTypeProjection::TypeArgument(_)]) {
            return Err(err("qualified Package Type argument retained row has an invalid projection"));
        }
        class(row.binder, b)?;
    }
    if classes.len() != shape.binder_count as usize {
        return Err(err("stored domain has unused class labels"));
    }
    Ok(true)
}

pub(super) fn qualification(
    module: &TypedModule,
    ty: &Type,
    limits: &Limits,
    b: &mut Budget,
) -> Check<()> {
    type_bounds(ty, limits, b)?;
    physical(ty, limits, b)?;
    let (storage, _) =
        qualified(ty).ok_or_else(|| err("expected an actual qualified stored type"))?;
    if matches!(
        storage,
        Type::Record { .. } | Type::Package { .. } | Type::Enum { .. }
    ) {
        let mut matches = 0usize;
        b.spend(module.life_instances.len())?;
        for instance in &module.life_instances {
            type_work(&instance.ty, limits, b)?;
            type_work(ty, limits, b)?;
            if instance.ty == *ty {
                matches += 1;
            }
        }
        if matches != 1 {
            return Err(err(
                "nominal carrier lacks its exact complete declaration/instance correspondence",
            ));
        }
    }
    Ok(())
}

/// This admits only a qualified pure value, retaining its complete semantic
/// type. The ordinary read's actual pointer/loan proof remains mandatory.
/// Copy qualification for a complete readonly, nonowning memory value. This
/// never supplies data, tags, parent loans, or a cleanup theorem.
pub(crate) fn copyable_memory_value(
    module: &TypedModule,
    ty: &Type,
    b: &mut Budget,
) -> Check<bool> {
    if shared_tuple_value(module, ty, None, b)? {
        return Ok(true);
    }
    let mut pending = vec![(ty, 0usize)];
    while let Some((current, depth)) = pending.pop() {
        b.spend(1)?;
        if depth > b.limits.max_call_depth.min(128) {
            return Err(limit("readonly memory value depth exceeded"));
        }
        match current {
            Type::BorrowedStorage { storage, life } => {
                b.spend(life.slots.len())?;
                if life
                    .slots
                    .iter()
                    .any(|slot| slot.mode == StoredBorrowMode::ExclusiveAddress)
                {
                    return Ok(false);
                }
                qualification(module, current, &b.limits.clone(), b)?;
                pending.push((storage, depth + 1));
            }
            Type::Address {
                mutable: false,
                inner,
            }
            | Type::Desc(inner) => {
                crate::dynamic_witness::intrinsic_domain(module, inner, b)?;
                crate::dynamic_witness::structural_actual(
                    module,
                    inner,
                    DynStructuralBound::Value,
                    b,
                )?;
            }
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|ty| (ty, depth + 1)));
            }
            Type::Record { fields, .. } => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, ty)| (ty, depth + 1)));
            }
            Type::Enum { variants, .. } => {
                b.spend(variants.len())?;
                for (_, fields) in variants {
                    b.spend(fields.len())?;
                    pending.extend(fields.iter().map(|ty| (ty, depth + 1)));
                }
            }
            Type::ErrorUnion { error, result } => {
                b.spend(2)?;
                pending.extend([(error.as_ref(), depth + 1), (result.as_ref(), depth + 1)]);
            }
            Type::Array { element, .. } => pending.push((element, depth + 1)),
            current if current.is_copy_value() => {}
            _ => return Ok(false),
        }
    }
    crate::dynamic_witness::intrinsic_domain(module, ty, b)?;
    Ok(true)
}

pub(super) fn pure_read(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
    if !b.pure_nominal
        || !matches!(ty, Type::BorrowedStorage { storage, .. } if matches!(storage.as_ref(), Type::Record { .. } | Type::Enum { .. }))
    {
        return Ok(false);
    }
    let limits = b.limits.clone();
    type_bounds(ty, &limits, b)?;
    if !retained::pure(ty, b)? {
        return Ok(false);
    }
    qualification(module, ty, &limits, b)?;
    Ok(true)
}

pub(super) fn pure_inline(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
    if !b.pure_nominal
        || ty.is_copy_value()
        || !matches!(
            ty,
            Type::Array { .. } | Type::Tuple(_) | Type::Record { .. } | Type::Enum { .. }
        )
        || !retained::pure(ty, b)?
    {
        return Ok(false);
    }
    let limits = b.limits.clone();
    type_bounds(ty, &limits, b)?;
    let mut pending = vec![ty];
    while let Some(current) = pending.pop() {
        b.spend(1)?;
        match current {
            Type::BorrowedStorage { storage, .. } => {
                physical(current, &limits, b)?;
                let mut count = 0usize;
                for instance in &module.life_instances {
                    type_work(&instance.ty, &limits, b)?;
                    type_work(current, &limits, b)?;
                    if instance.ty == *current {
                        count += 1;
                    }
                }
                if count != 1 {
                    return Err(err(
                        "pure inline child has no unique complete nominal instance",
                    ));
                }
                pending.push(storage);
            }
            Type::Array { element, .. } => pending.push(element),
            Type::Tuple(fields) => pending.extend(fields),
            Type::Record { fields, .. } => pending.extend(fields.iter().map(|(_, t)| t)),
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    pending.extend(fields);
                }
            }
            Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I8
            | Type::I16
            | Type::Isize
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::F32
            | Type::F64 => {}
            Type::FunctionPointer { .. } | Type::ForeignCallback { .. } => {
                if !pure_copy::code_atom(current, b)? {
                    return Ok(false);
                }
            }
            _ => return Ok(false),
        }
    }
    Ok(true)
}

/// An ordinary owner has no stored data targets, but its borrowed head still
/// has the real Address origin and loan. This predicate concerns only the
/// pointee: it never makes the owner Copy or discharges its cleanup debt.
fn ordinary_task_enum(module: &TypedModule, root: &Type, b: &mut Budget) -> Check<bool> {
    if !b.task_sources() || !matches!(root, Type::Enum { .. }) { return Ok(false); }
    let mut todo = vec![root];
    let mut owning = false;
    while let Some(ty) = todo.pop() {
        b.spend(1)?;
        match ty {
            Type::Enum { variants, .. } => {
                b.spend(variants.len())?;
                for (_, fields) in variants { b.spend(fields.len())?; todo.extend(fields); }
            }
            Type::Tuple(fields) => { b.spend(fields.len())?; todo.extend(fields); }
            Type::Package { fields, .. } => {
                b.spend(fields.len())?;
                todo.extend(fields.iter().map(|(_, field)| field));
            }
            Type::ErrorUnion { error, result } => todo.extend([error.as_ref(), result.as_ref()]),
            Type::Managed(inner) if super::stored_owned::payload(inner, b)? => {
                // Check the physical nominal definition independently of the
                // root Enum and its stored-target classification.
                let Type::Record { name, .. } = inner.as_ref() else { return Ok(false); };
                b.spend(module.resources.len())?;
                let mut matches = 0usize;
                for resource in &module.resources {
                    b.spend(name.len().saturating_add(resource.name.len()))?;
                    if resource.name == *name {
                        resources::charge_payload(&resource.payload, b)?;
                        if resource.payload != **inner { return Err(err("Enum field changes its actual resource payload")); }
                        matches += 1;
                    }
                }
                if matches != 1 { return Err(err("Enum field lacks its unique actual resource")); }
                owning = true;
            }
            data if retained::resource_field(data, b)? => {}
            _ => return Ok(false),
        }
    }
    if owning { super::sums::check_type_for(module, root, None, b)?; }
    Ok(owning)
}

fn ordinary_enum_operation(module: &TypedModule, f: &TypedFunction, i: &Instruction, b: &mut Budget) -> Check<bool> {
    if !b.task_sources() { return Ok(false); }
    let root = match &i.kind {
        InstructionKind::MakeVariant { .. } => i.result.and_then(|id| f.values.get(id.0 as usize)),
        InstructionKind::DropEnum { local } | InstructionKind::UnpackVariant { local, .. }
        | InstructionKind::VariantTagLocal { local, .. } => f.locals.get(local.0 as usize).map(|l| &l.ty),
        _ => return Ok(false),
    };
    ordinary_task_enum(module, root.ok_or_else(|| err("Enum operation lacks its actual root"))?, b)
}

fn ordinary_owner_address(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
    let Type::Address { inner, .. } = ty else {
        return Ok(false);
    };
    if !(matches!(inner.as_ref(), Type::Package { .. })
        && pure_copy::complete_owned_value(module, inner, b)?
        || super::stored_owned::owner(inner, b)?)
    {
        return Ok(false);
    }
    let (classes, _pending_sources) = crate::dynamic_witness::intrinsic_domain(module, inner, b)?;
    if classes != 0 {
        return Ok(false);
    }
    // This selects the ordinary pointee lane only. A complete Known0 domain
    // may still owe a real closure creation, owner transfer or cleanup proof;
    // the head's source, active loan and actual CFG remain independently due.
    crate::dynamic_witness::structural_actual(module, inner, DynStructuralBound::Value, b)?;
    Ok(true)
}

pub(super) fn qualifications(
    module: &TypedModule,
    limits: &Limits,
    mask: Option<&[bool]>,
    b: &mut Budget,
) -> Check<()> {
    for (index, f) in module.functions.iter().enumerate() {
        if mask.is_some_and(|mask| !mask[index]) {
            continue;
        }
        for block in &f.blocks {
            for (ordinal, i) in block.instructions.iter().enumerate() {
                b.spend(1)?;
                // Type/mode authentication is independent of dispatch flags.
                anonymous_tuple::operation(f, i, b)?;
                if let InstructionKind::SuspendCall { function, .. } = i.kind {
                    let target = module
                        .functions
                        .get(function.0 as usize)
                        .ok_or_else(|| err("stored SuspendCall target is absent"))?;
                    if f.call_kind != CallKind::Suspend
                        || target.call_kind != CallKind::Suspend
                        || target.foreign.is_some()
                        || target.environment.is_some()
                    {
                        return Err(err(
                            "stored SuspendCall changes its actual suspending caller/callee ABI",
                        ));
                    }
                    // This checks the real operands and the complete finite
                    // child-frame graph, not just a public call-mode flag.
                    crate::check_instruction(module, f, i, b)?;
                }
                if matches!(
                    i.kind,
                    InstructionKind::BorrowLocal { .. } | InstructionKind::BorrowLocalMut { .. }
                ) {
                    let (local, mutable) = match i.kind {
                        InstructionKind::BorrowLocal { local } => (local, false),
                        InstructionKind::BorrowLocalMut { local } => (local, true),
                        _ => unreachable!(),
                    };
                    let source = f.locals.get(local.0 as usize).ok_or_else(|| {
                        reject(
                            "E-VERIFY-TYPE",
                            Some(i.span),
                            "borrow source local is absent",
                        )
                    })?;
                    let actual = i
                        .result
                        .and_then(|id| f.values.get(id.0 as usize))
                        .ok_or_else(|| {
                            reject(
                                "E-VERIFY-TYPE",
                                Some(i.span),
                                "borrow result type is absent",
                            )
                        })?;
                    type_work(&source.ty, limits, b)?;
                    type_work(actual, limits, b)?;
                    if !matches!(actual, Type::Address { mutable: mode, inner }
                        if *mode == mutable && inner.as_ref() == &source.ty)
                    {
                        return Err(reject(
                            "E-VERIFY-TYPE",
                            Some(i.span),
                            "borrow result differs from its exact actual local type or mode",
                        ));
                    }
                }
                if let InstructionKind::Load(local) = i.kind
                    && f.locals
                        .get(local.0 as usize)
                        .is_some_and(|local| exclusive_carrier_value(&local.ty))
                {
                    return Err(err(
                        "actual exclusive stored carrier was copied without a transfer",
                    ));
                }
                let ordinary_store_operation =
                    super::stored_owned::ordinary_store_instruction(module, f, i, b)?
                        || super::stored_owned::ordinary_byte_instruction(module, f, i, b)?;
                let enum_operation = enum_address::instruction(module, f, i, b)? || ordinary_enum_operation(module, f, i, b)?;
                let task_capture_operation = task_address::capture_instruction(module, f, i, b)?;
                let owned_operation = !ordinary_store_operation
                    && !task_capture_operation
                    && !owned_byte_views::owner_instruction(module, f, i, b)?
                    && !(b.task_sources() && task_address::owner_instruction(f, &i.kind))
                    && super::stored_owned::operation(module, f, i, b)?;
                if !ordinary_store_operation
                    && !super::class_flow::pointer_instruction(f, i)
                    && !super::stored_callables::instruction(module, f, i)
                    && !foreign_calls::operation(module, f, i, b)?
                    && !returned_environment::borrowed_child(module, f, i, b)?
                    && !compound::operation(f, i, b)?
                    && !anonymous_tuple::operation(f, i, b)?
                    && !dyn_closed::operation(module, f, i, b)?
                    && !dyn_ports::operation(module, f, i, b)?
                    && !dyn_captures::operation(module, f, i, b)?
                    && !dyn_ports::wrapper(module, f, i, b)?
                    && !dyn_closed::wrapper(module, f, i, b)?
                    && !owned_operation
                    && !task_capture_operation
                    && !enum_operation
                    && !owned_byte_views::instruction(module, f, i, b)?
                    && !owned_byte_views::description_instruction(module, f, i, b)?
                    && byte_queries::query(&i.kind).is_none()
                    && !(b.task_sources() && task_address::operation(&i.kind))
                    && !failure_unpack(f, i, b)?
                    && !(b.task_sources()
                        && crate::tasks::input_scope::neutral_owner_unpack(f, i, b)?)
                    && !crate::tasks::input_scope::registration_operation(f, i, b)?
                    && !failure_tuple_drop(f, i, b)?
                    && !(b.masked_stored_tasks()
                        && matches!(
                            i.kind,
                            InstructionKind::OwnedStoreNew { .. }
                                | InstructionKind::OwnedStoreReserve { .. }
                                | InstructionKind::OwnedStorePush { .. }
                                | InstructionKind::OwnedStorePop { .. }
                                | InstructionKind::OwnedStoreCapacity { .. }
                                | InstructionKind::OwnedStoreLen { .. }
                                | InstructionKind::OwnedStoreBorrow { .. }
                                | InstructionKind::DropErrorUnion { .. }
                                | InstructionKind::DescribePackageField { .. }
                        ))
                    && !(b.store_descriptions()
                        && matches!(i.kind, InstructionKind::OwnedStoreBorrowMut { .. }))
                    && !replacements::operation(&i.kind)
                    && !pure_copy::operation(&i.kind)
                    && !pure_copy::ordinary_owned_union(module, f, i, b)?
                    && !pure_copy::ordinary_owned_tuple(module, f, i, b)?
                    && !callable_cells::operation(module, f, i, b)?
                    && !pure_copy::ordinary_copy_field_borrow(f, i, b)?
                    && !finite_fields::operation(module, f, i, b)?
                    && !pure_copy::ordinary_package_borrow(module, f, i, b)?
                    && !pure_copy::ordinary_package_read(f, i, b)?
                    && !pure_copy::qualified_package_description(f, i, b)?
                    && !ordinary_error_borrow(module, f, i, b)?
                    && !matches!(
                        &i.kind,
                        InstructionKind::StaticUtf8 { .. }
                            | InstructionKind::Constant(_)
                            | InstructionKind::Load(_)
                            | InstructionKind::Store { .. }
                            | InstructionKind::StorageLive(_)
                            | InstructionKind::StorageDead(_)
                            | InstructionKind::BindPackageCapture { .. }
                            | InstructionKind::EndPackageCapture { .. }
                            | InstructionKind::BorrowLocal { .. }
                            | InstructionKind::BorrowLocalMut { .. }
                            | InstructionKind::ReborrowAddress { .. }
                            | InstructionKind::AddressField { .. }
                            | InstructionKind::AddressIndex { .. }
                            | InstructionKind::AddressSlice { .. }
                            | InstructionKind::ReadAddress { .. }
                            | InstructionKind::WriteAddress { .. }
                            | InstructionKind::SliceLength { .. }
                            | InstructionKind::MakeAggregate { .. }
                            | InstructionKind::MakePackage { .. }
                            | InstructionKind::ByteBufferNew
                            | InstructionKind::ByteBufferPush { .. }
                            | InstructionKind::ByteBufferReserve { .. }
                            | InstructionKind::ByteBufferTruncate { .. }
                            | InstructionKind::DescribeAddress { .. }
                            | InstructionKind::DescribeLocal { .. }
                            | InstructionKind::DescribeCapture { .. }
                            | InstructionKind::DropPackage { .. }
                            | InstructionKind::TakePackageField { .. }
                            | InstructionKind::DropPackageField { .. }
                            | InstructionKind::StorePackageField { .. }
                            | InstructionKind::DropManaged { .. }
                            | InstructionKind::TakeLocal { .. }
                            | InstructionKind::ExtractField { .. }
                            | InstructionKind::Call { .. }
                            | InstructionKind::SuspendCall { .. }
                            | InstructionKind::ForeignCall { .. }
                            | InstructionKind::UnsafeEnter { .. }
                            | InstructionKind::UnsafeExit { .. }
                            | InstructionKind::Unary { .. }
                            | InstructionKind::Binary { .. }
                            | InstructionKind::ConvertInteger { .. }
                            | InstructionKind::ConvertNumber { .. }
                            | InstructionKind::ScopeEnter { .. }
                            | InstructionKind::ScopeExit { .. }
                            | InstructionKind::LoadStoredBorrow {
                                mode: StoredBorrowLoad::CopyShared
                                    | StoredBorrowLoad::Reborrow { .. }
                                    | StoredBorrowLoad::Take,
                                ..
                            }
                            | InstructionKind::StoreStoredBorrow { .. }
                    )
                {
                    return Err(unsupported(
                        "instruction is outside the checked stored profile, including unreachable blocks",
                        Some(i.span),
                    ).with_note(format!("actual function {:?}, block {:?}, instruction {ordinal}", f.id, block.id)));
                }
                if super::class_flow::pointer_instruction(f, i) {
                    super::class_flow::pointer_types(module, f, i, b)?;
                }
                if super::stored_callables::instruction(module, f, i) {
                    super::stored_callables::types(module, f, i, b)?;
                }
                if array_index::candidate(f, i) {
                    array_index::types(f, i, b)?;
                    b.spend(b.array_outer_functions.len().saturating_add(1))?;
                    if !b.array_outer_functions.contains(&f.id) {
                        b.array_outer_functions.push(f.id);
                    }
                }
                if pure_copy::operation(&i.kind)
                    && !array_index::candidate(f, i)
                    && !compound::projection(f, i, b)?
                    && !compound::operation(f, i, b)?
                    && !anonymous_tuple::operation(f, i, b)?
                    && enum_address::types(module, f, i, b)?.is_none()
                    && option_address::types(module, f, i, b)?.is_none()
                    && !(b.task_sources() && task_address::wrapper_instruction(f, i, b)?)
                    && !dyn_ports::wrapper(module, f, i, b)?
                    && !dyn_closed::wrapper(module, f, i, b)?
                {
                    // A qualified Enum claim cannot become an unsupported Copy
                    // operation merely by deleting its nominal instance inventory.
                    let claimed = match i.kind {
                        InstructionKind::MakeVariant { .. } => {
                            i.result.map(|v| vt(f, v)).transpose()?
                        }
                        InstructionKind::ReadVariant { value, .. }
                        | InstructionKind::VariantTag { value } => Some(vt(f, value)?),
                        _ => None,
                    };
                    if claimed.is_some_and(enum_address::candidate) {
                        return Err(err(
                            "borrowed Enum operation has no authenticated complete nominal instance",
                        ));
                    }
                    if !pure_copy::ordinary_owned_union(module, f, i, b)? {
                        pure_copy::types(module, f, i, b)?;
                    }
                }
                if let InstructionKind::TakePackageField { local, .. }
                | InstructionKind::DropPackageField { local, .. }
                | InstructionKind::StorePackageField { local, .. } = &i.kind
                    && f.locals
                        .get(local.0 as usize)
                        .is_some_and(|local| qualified(&local.ty).is_some())
                {
                    super::packages::check_instruction(module, f, i, b)?;
                }
            }
        }
        for root in f
            .locals
            .iter()
            .map(|l| &l.ty)
            .chain(&f.values)
            .chain(std::iter::once(&f.return_type))
        {
            b.spend(1)?;
            let mut todo = vec![root];
            while let Some(t) = todo.pop() {
                b.spend(1)?;
                match t {
                    Type::BorrowedStorage { storage, .. } => {
                        qualification(module, t, limits, b)?;
                        todo.push(storage);
                    }
                    Type::Address { inner, .. }
                    | Type::Array { element: inner, .. }
                    | Type::Slice { element: inner } => todo.push(inner),
                    Type::Tuple(fs) => {
                        b.spend(fs.len())?;
                        todo.extend(fs);
                    }
                    Type::Record { fields, .. } => {
                        b.spend(fields.len())?;
                        todo.extend(fields.iter().map(|(_, t)| t));
                    }
                    Type::ErrorUnion { error, result }
                        if b.stored_enum_addresses && enum_address::signature(t) =>
                    {
                        b.spend(2)?;
                        todo.push(error);
                        todo.push(result);
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn identity_work(id: &LifeDeclarationIdentity, b: &mut Budget) -> Check<()> {
    b.spend(
        id.package
            .len()
            .saturating_add(id.module_path.len())
            .saturating_add(id.declaration_path.len())
            .saturating_add(id.generation_path.len())
            .saturating_add(1),
    )?;
    for n in id.module_path.iter().chain(&id.declaration_path) {
        b.spend(n.len())?;
    }
    Ok(())
}
pub(super) fn instruction_work(f: &TypedFunction, i: &Instruction, b: &mut Budget) -> Check<()> {
    let limits = b.limits.clone();
    for value in instruction_operands(&i.kind).chain(i.result) {
        type_work(vt(f, value)?, &limits, b)?;
    }
    Ok(())
}
pub(super) fn inventory(module: &TypedModule, limits: &Limits, b: &mut Budget) -> Check<()> {
    if module.life_declarations.len() > limits.max_nodes
        || module.life_instances.len() > limits.max_nodes
    {
        return Err(limit(
            "stored nominal inventory count exceeds verification limit",
        ));
    }
    b.spend(
        module
            .life_declarations
            .len()
            .saturating_add(module.life_instances.len()),
    )?;
    let mut declarations = Vec::<&LifeNominalDeclaration>::new();
    for d in &module.life_declarations {
        identity_work(&d.identity, b)?;
        if task_failure::declaration(d) {
            b.task_failure_carriers = true;
        }
        if enum_address::poll_reserved(d) && !enum_address::poll_declaration(d) {
            return Err(err("reserved Poll nominal declaration is not authentic"));
        }
        if d.identity.package.is_empty()
            || d.identity.declaration_path.is_empty()
            || d.identity
                .module_path
                .iter()
                .chain(&d.identity.declaration_path)
                .any(String::is_empty)
        {
            return Err(err("declaration lacks full stable nominal identity"));
        }
        for old in &declarations {
            identity_work(&d.identity, b)?;
            identity_work(&old.identity, b)?;
            if old.id == d.id || old.identity == d.identity {
                return Err(err("duplicate nominal declaration reference or identity"));
            }
        }
        b.spend(d.parameters.len())?;
        for p in &d.parameters {
            if let LifeParameterKind::Const(t) = p {
                type_work(t, limits, b)?;
                if !t.is_copy_value() {
                    return Err(err("nominal const kind has invalid actual data type"));
                }
            }
        }
        b.spend(1)?;
        declarations.push(d);
    }
    if b.task_failure_carriers && b.task_input_scopes.is_none() && !module.closures.is_empty() {
        b.task_input_scopes = Some(crate::tasks::input_scope::build(module, limits, b)?);
    }
    for (i, instance) in module.life_instances.iter().enumerate() {
        type_work(&instance.ty, limits, b)?;
        if matches!(&instance.ty, Type::BorrowedStorage { life, .. } if life.slots.is_empty()) {
            b.pure_nominal = true;
        }
        physical(&instance.ty, limits, b)?;
        let storage = match qualified(&instance.ty) {
            Some((storage, _)) => storage,
            None => {
                nominal::plain(module, instance, limits, b)?;
                &instance.ty
            }
        };
        b.spend(declarations.len())?;
        let d = declarations
            .iter()
            .find(|d| d.id == instance.declaration)
            .ok_or_else(|| err("nominal instance references absent declaration"))?;
        if matches!(storage, Type::Enum { name, .. } if name.starts_with("@std.task:v1:Poll:"))
            && !enum_address::poll_declaration(d)
            && !task_failure::declaration(d)
        {
            return Err(err(
                "reserved Poll physical nominal has no sealed declaration",
            ));
        }
        if !matches!(
            (d.kind, storage),
            (
                LifeNominalKind::Record | LifeNominalKind::Resource,
                Type::Record { .. }
            ) | (LifeNominalKind::Package, Type::Package { .. })
                | (LifeNominalKind::Enum, Type::Enum { .. })
        ) {
            if enum_address::candidate(&instance.ty) {
                return Err(err(
                    "borrowed Enum instance disagrees with its declaration kind",
                ));
            }
            return Err(unsupported(
                "nominal kind has no actual physical stored consumer",
                None,
            ));
        }
        b.stored_package_instance |= matches!(storage, Type::Package { .. });
        if d.parameters.len() != instance.arguments.len() {
            return Err(err("nominal actual list omits complete generic ordinal"));
        }
        nominal::rows(module, instance, limits, b)?;
        b.stored_enum_addresses |= (!d.id.compiler || enum_address::poll_declaration(d))
            && enum_address::candidate(&instance.ty);
        for previous in &module.life_instances[..i] {
            type_work(&previous.ty, limits, b)?;
            type_work(&instance.ty, limits, b)?;
            if previous.ty == instance.ty
                || (previous.declaration == instance.declaration
                    && nominal::same_key(previous, instance, limits, b)?)
            {
                return Err(err("duplicate complete nominal instance"));
            }
        }
    }
    Ok(())
}

fn relation(
    f: &TypedFunction,
    base: StorageBase,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<StorageBinder> {
    let c = f
        .storage_life
        .as_ref()
        .ok_or_else(|| err("stored signature lacks complete relation"))?;
    if path.is_empty() {
        b.spend(c.bindings.len())?;
        return super::signature_life::outer(c, base)
            .ok_or_else(|| err("signature omits actual outer borrow"));
    }
    let ty = match base {
        StorageBase::Parameter(i) => f
            .parameters
            .get(i as usize)
            .and_then(|l| f.locals.get(l.0 as usize))
            .map(|l| &l.ty),
        StorageBase::Result => Some(&f.return_type),
        StorageBase::Capture(_) | StorageBase::Receiver => None,
    }
    .ok_or_else(|| err("stored relation base is absent"))?;
    let array = if let Type::Address { inner, .. } = ty {
        inner.as_ref()
    } else {
        ty
    };
    if shared_array_shape(array, b)? {
        let Type::Array { length, .. } = array else {
            unreachable!()
        };
        if !matches!(path, [PlaceProjection::Field(index)] if *index < *length) {
            return Err(err("array relation has no actual bounded element"));
        }
        b.spend(c.bindings.len().saturating_mul(2))?;
        return c.bindings.iter().find(|row| row.position.base == base
            && matches!(row.position.path.as_slice(), [ContractProjection::Place(step)] if *step == path[0]))
            .map(|row| row.binder).ok_or_else(|| err("array element lacks its actual signature binding"));
    }
    let (shape, leaf_path, class_path) = relation_index::boundary(ty, path, b)?;
    b.spend(shape.slots.len())?;
    let mut local = None;
    for slot in &shape.slots {
        b.spend(slot.path.len().saturating_add(path.len()).saturating_add(1))?;
        if slot.path == leaf_path && local.replace(slot.binder).is_some() {
            return Err(err("duplicate actual stored leaf"));
        }
    }
    let local = local.ok_or_else(|| err("relation selected an absent physical stored leaf"))?;
    b.spend(c.classes.len())?;
    for row in &c.classes {
        b.spend(row.position.path.len().saturating_add(1))?;
        if row.position.base == base
            && row.position.class == local
            && row.position.path == class_path
        {
            return Ok(row.binder);
        }
    }
    Err(err("signature omits actual qualified local class"))
}

pub(super) fn header(module: &TypedModule, f: &TypedFunction, b: &mut Budget) -> Check<()> {
    header_inner(module, f, b).map_err(|mut diagnostic| {
        if diagnostic.code == "E-VERIFY-SIGNATURE-CLASS" {
            // A false self-contained result envelope is a distinct source
            // failure. Missing header rows in that same environment must
            // not hide the already selected physical capture dependency.
            if let Some(id) = f.environment
                && module.closures.get(id.0 as usize).is_some_and(|closure|
                    closure.id == id && closure.function == f.id)
                && let Err(source) = super::retained::check_self_contained_sources_for(
                    module, Some(id), &b.limits.clone(), b,
                )
                && source.code == "E-VERIFY-CALLABLE-SOURCE"
            {
                return source;
            }
            let location = zil_base::DiagnosticIrLocation {
                function: f.id.0,
                block: None,
                instruction: None,
            };
            for related in &mut diagnostic.related {
                related.ir.get_or_insert(location.clone());
            }
            if diagnostic.related.is_empty() {
                diagnostic.relate(zil_base::DiagnosticRole::Input, Some(f.span), Some(location),
                    format!("actual signature of {}", f.name));
            }
        }
        diagnostic
    })
}

fn header_inner(module: &TypedModule, f: &TypedFunction, b: &mut Budget) -> Check<()> {
    // Schedules extra current-memory work; the actual rows still come from
    // this kernel's checked types, instructions and frame at each use.
    b.qualified_head_returns |= qualified_heads::shape(&f.return_type).is_some();
    if super::ordinary_heads::check(module, f, b)? {
        return Ok(());
    }
    fn nested_class(ty: &Type, depth: usize, b: &mut Budget) -> Check<bool> {
        if depth > b.limits.max_call_depth {
            return Err(limit("stored signature class depth exceeds limit"));
        }
        b.spend(1)?;
        match ty {
            Type::BorrowedStorage { .. } => Ok(true),
            Type::Address { inner, .. } | Type::Array { element: inner, .. } => {
                nested_class(inner, depth + 1, b)
            }
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                for field in fields {
                    if nested_class(field, depth + 1, b)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Type::Record { fields, .. } => {
                b.spend(fields.len())?;
                for (_, field) in fields {
                    if nested_class(field, depth + 1, b)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Type::Enum { variants, .. } => {
                b.spend(variants.len())?;
                for (_, fields) in variants {
                    b.spend(fields.len())?;
                    for field in fields {
                        if nested_class(field, depth + 1, b)? {
                            return Ok(true);
                        }
                    }
                }
                Ok(false)
            }
            _ => Ok(false),
        }
    }
    let mut byte_companion = false;
    for p in &f.parameters {
        b.spend(1)?;
        if f.locals.get(p.0 as usize).is_none() {
            return Err(err("signature parameter outside actual local table"));
        }
        byte_companion |= byte_queries::is_description(&f.locals[p.0 as usize].ty);
    }
    let Some(c) = &f.storage_life else {
        if b.stored_enum_addresses
            && f.parameters
                .iter()
                .map(|l| &f.locals[l.0 as usize].ty)
                .chain(std::iter::once(&f.return_type))
                .any(|ty| matches!(ty, Type::ErrorUnion { .. }) && enum_address::signature(ty))
        {
            return Err(err(
                "borrowed sum wrapper signature omits its complete relation",
            ));
        }
        for ty in f
            .parameters
            .iter()
            .map(|p| &f.locals[p.0 as usize].ty)
            .chain(std::iter::once(&f.return_type))
        {
            b.spend(1)?;
            if f.environment.is_none()
                && (qualified(ty).is_some()
                    || addressed(ty).is_some()
                    || shared_array_value(ty)
                    || matches!(ty, Type::Address { inner, .. } if shared_array_value(inner)))
            {
                return Err(err("stored signature requires complete class relation"));
            }
            if b.dyn_contexts && crate::dynamic_ports::has_context(module, ty, b)? {
                return Err(err(
                    "Context-bearing signature requires its complete actual class relation",
                ));
            }
            let composite = |ty: &Type| {
                matches!(
                    ty,
                    Type::Array { .. } | Type::Tuple(_) | Type::Record { .. } | Type::Enum { .. }
                )
            };
            if !module.life_instances.is_empty()
                && (composite(ty) || matches!(ty, Type::Address { inner, .. } if composite(inner)))
                && nested_class(ty, 0, b)?
            {
                return Err(err(
                    "nested stored qualified signature requires complete class relation",
                ));
            }
        }
        return Ok(());
    };
    crate::signature_life::charge(c, b)?;
    b.spend(f.parameters.len())?;
    // The direct root upper and the complete class relation are independent
    // constraints. Authenticate both headers; final actual returns below must
    // satisfy both, without replacing this canonical relation at a call site.
    if f.reference_return_bound.is_some() {
        super::check_reference_return_bound(f, b)?;
    }
    if f.foreign.is_some() && !foreign_calls::declaration(module, f, b)? {
        return Err(err("stored signature contradicts another active profile"));
    }
    let mut parameters = Vec::with_capacity(f.parameters.len());
    let mut has_fixed_environment = false;
    for p in &f.parameters {
        let ty = &f.locals[p.0 as usize].ty;
        type_bounds(ty, &b.limits.clone(), b)?;
        type_work(ty, &b.limits.clone(), b)?;
        let new_nominal = |ty: &Type| {
            matches!(ty, Type::BorrowedStorage { storage, life }
            if life.slots.is_empty()
                && (!matches!(storage.as_ref(), Type::Record { .. } | Type::Tuple(_))
                    || !life.retained.is_empty()))
        };
        if (matches!(ty, Type::Array { .. }) && !ty.is_copy_value())
            || new_nominal(ty)
            || matches!(ty, Type::Address { inner, .. }
                if (matches!(inner.as_ref(), Type::Array { .. }) && !inner.is_copy_value())
                    || new_nominal(inner))
        {
            type_bounds(ty, &b.limits.clone(), b)?;
        }
        if f.callable_contract.is_some()
            && let Type::Managed(payload) = ty
            && let Type::Record { fields, .. } = payload.as_ref()
        {
            b.spend(fields.len().saturating_mul(2))?;
        }
        has_fixed_environment |= fixed_environment(module, ty, b);
        parameters.push(ty);
    }
    type_bounds(&f.return_type, &b.limits.clone(), b)?;
    type_work(&f.return_type, &b.limits.clone(), b)?;
    let new_nominal = |ty: &Type| {
        matches!(ty, Type::BorrowedStorage { storage, life }
        if life.slots.is_empty()
            && (!matches!(storage.as_ref(), Type::Record { .. } | Type::Tuple(_))
                || !life.retained.is_empty()))
    };
    if (matches!(f.return_type, Type::Array { .. }) && !f.return_type.is_copy_value())
        || new_nominal(&f.return_type)
        || matches!(&f.return_type, Type::Address { inner, .. }
            if (matches!(inner.as_ref(), Type::Array { .. }) && !inner.is_copy_value())
                || new_nominal(inner))
    {
        type_bounds(&f.return_type, &b.limits.clone(), b)?;
    }
    if let Some(id) = f.environment {
        let d = module
            .closures
            .get(id.0 as usize)
            .ok_or_else(|| err("captured Fixed entry has no actual environment"))?;
        if d.id != id || d.function != f.id || d.storage_life.as_ref() != Some(c) {
            return Err(err("captured Fixed entry and definition relation differ"));
        }
        super::stored_callables::captured_profile(module, d, b)?;
        return super::signature_life::closure_relation_resource_views(
            module,
            &parameters,
            &d.captures,
            d.kind,
            super::stored_callables::life_result(d),
            c,
            Some(f.span),
            b,
        );
    }
    if has_fixed_environment {
        b.spend(parameters.len())?;
        for ty in &parameters {
            if !fixed_environment(module, ty, b)
                && !super::stored_callables::borrowed_capture_input(ty)
                && !super::stored_callables::complete_value_input(module, ty, b)?
                && !super::stored_owned::owner(ty, b)?
                && !super::stored_owned::ordinary_byte_value(ty, b)?
                && !super::stored_owned::ordinary_store_value(module, ty, b)?
            {
                return Err(unsupported(
                    "captured Fixed helper requires complete supported input domains",
                    Some(f.span),
                ));
            }
        }
    }
    // Authorize an actual direct description head from its sealed carrier.
    // The later CFG scan independently schedules data-source and loan checks.
    let byte_signature = byte_companion
        && !byte_queries::legacy_task_signature(f)
        && f.call_kind == CallKind::Direct
        && f.foreign.is_none()
        && f.environment.is_none();
    if byte_signature {
        crate::byte_buffers::check_carrier(module,&Type::Desc(Box::new(Type::ByteBuffer)),Some(f.span),b)?;
        b.spend(parameters.len())?;
    }
    // Actual Capture sources prove targets separately. If a factory publishes
    // output class rows, authenticate its complete Fixed class relation as well.
    // A separately proved local result role retains all its input/static
    // dependencies without equating independent input classes.
    let factory = matches!(f.return_type, Type::Closure(_)) && f.callable_return_contract.is_some();
    b.spend(c.classes.len())?;
    let output_classes = c
        .classes
        .iter()
        .any(|row| row.position.base == StorageBase::Result);
    let result = if factory && !output_classes {
        &Type::Unit
    } else {
        &f.return_type
    };
    let mut pending = dyn_returns::pending::register(module, f, b)?;
    for transfer in crate::dynamic_witness::semantic_patterns::local_role_transfers(module, f, b)? {
        for output in transfer.outputs {
            b.spend(pending.iter().fold(
                output.position.path.len().saturating_add(1),
                |fee, row| {
                    fee.saturating_add(
                        row.path
                            .len()
                            .saturating_add(output.position.path.len())
                            .saturating_add(1),
                    )
                },
            ))?;
            if !pending.contains(&output.position) {
                pending.push(output.position);
            }
        }
    }
    if !pending.is_empty() {
        return crate::signature_life::function_relation_pending_views(
            module,
            &parameters,
            result,
            c,
            Some(f.span),
            f.callable_contract.is_some() || factory,
            byte_signature,
            &pending,
        );
    }
    super::signature_life::function_relation_resource_views(
        module,
        &parameters,
        result,
        c,
        Some(f.span),
        f.callable_contract.is_some() || factory,
        byte_signature,
        b,
    )?;
    Ok(())
}

fn ancestry(ids: &[usize], loans: &[Loan], b: &mut Budget) -> Check<Vec<usize>> {
    // Empty roots have an empty closure without inspecting the arena.
    b.spend(1)?;
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    b.spend(ids.len().saturating_add(1))?;
    let mut todo = ids.to_vec();
    b.spend(WordRow::work_for(loans.len()))?;
    let mut visited = WordRow::empty(loans.len());
    let mut count = 0usize;
    while let Some(id) = todo.pop() {
        b.spend(1)?;
        // Preserve the original diagnostic before any bounded bit access.
        let row = loans
            .get(id)
            .ok_or_else(|| err("loan parent outside actual arena"))?;
        b.spend(1)?;
        if visited.is_live(id)? {
            continue;
        }
        b.spend(1)?;
        visited.assign(id, true)?;
        count += 1;
        b.spend(row.parents.len())?;
        if todo.capacity().saturating_sub(todo.len()) < row.parents.len() {
            b.spend(todo.len().saturating_add(1))?;
        }
        // Same LIFO traversal and parent discovery order as the original DFS.
        todo.extend(&row.parents);
    }
    // Scan actual words, clear each enumerated bit and store every result ID.
    // Enumeration is already ascending; no comparison sort is performed.
    b.spend(visited.work())?;
    b.spend(amount(count, 2)?.saturating_add(1))?;
    let mut result = Vec::with_capacity(count);
    result.extend(visited.ones());
    Ok(result)
}
#[cfg(test)]
fn ancestry_by_linear_set(ids: &[usize], loans: &[Loan], b: &mut Budget) -> Check<Vec<usize>> {
    b.spend(ids.len())?;
    let mut todo = ids.to_vec();
    let mut result = Vec::new();
    while let Some(id) = todo.pop() {
        b.spend(result.len().saturating_add(1))?;
        if result.contains(&id) {
            continue;
        }
        result.push(id);
        let row = loans
            .get(id)
            .ok_or_else(|| err("loan parent outside actual arena"))?;
        b.spend(row.parents.len())?;
        todo.extend(&row.parents);
    }
    b.spend(amount(result.len(), result.len())?)?;
    result.sort_unstable();
    Ok(result)
}
fn overlap(a: &Source, z: &Source, b: &mut Budget) -> Check<bool> {
    b.spend(width(a).saturating_add(width(z)))?;
    Ok(a.base == z.base
        && a.leaf == z.leaf
        && a.retained_port == z.retained_port
        && (a.inline.starts_with(&z.inline) || z.inline.starts_with(&a.inline)))
}
fn apart(
    a: &Source,
    z: &Source,
    requirements: &mut Vec<(Source, Source)>,
    b: &mut Budget,
) -> Check<()> {
    if overlap(a, z, b)? {
        return Err(reject(
            "E-BORROW-CONFLICT",
            None,
            format!("actual target intersects a live incompatible loan: {a:?} against {z:?}"),
        ));
    }
    if matches!(
        (a.base, z.base),
        (HoldRoot::Parameter(_), HoldRoot::Parameter(_))
    ) && (a.base != z.base || a.leaf != z.leaf || a.retained_port != z.retained_port)
    {
        let size = width(a).saturating_add(width(z));
        b.spend(amount(requirements.len().saturating_add(1), size)?)?;
        let pair = if a < z { (a, z) } else { (z, a) };
        if !requirements.iter().any(|(a, z)| a == pair.0 && z == pair.1) {
            b.spend(size)?;
            requirements.push((pair.0.clone(), pair.1.clone()));
        }
    }
    Ok(())
}
fn access_head(p: &Permission, write: bool, b: &mut Budget) -> Check<()> {
    if p.sources.is_empty() {
        return Err(err(
            "access has no independently grounded target permission",
        ));
    }
    if b.static_utf8 {
        b.spend(p.sources.len())?;
    }
    if b.static_utf8
        && write
        && p.sources
            .iter()
            .any(|source| source.base == HoldRoot::Static)
    {
        return Err(err("program-static text cannot authorize mutation"));
    }
    Ok(())
}
fn access_tail(
    p: &Permission,
    write: bool,
    active: &[usize],
    loans: &[Loan],
    delegated: &[usize],
    requirements: &mut Vec<(Source, Source)>,
    b: &mut Budget,
) -> Check<()> {
    for id in active {
        b.spend(delegated.len().saturating_add(1))?;
        if delegated.contains(id) {
            continue;
        }
        let loan = loans.get(*id).ok_or_else(|| err("live loan absent"))?;
        if write || loan.exclusive {
            for a in &p.sources {
                for z in &loan.sources {
                    apart(a, z, requirements, b).map_err(|mut error| {
                        if error.code == "E-BORROW-CONFLICT" {
                            error.notes.push(format!(
                                "actual live loan {id}, exclusive={}, parents {:?}; access write={write}, loans {:?}",
                                loan.exclusive, loan.parents, p.loans
                            ));
                        }
                        error
                    })?;
                }
            }
        }
    }
    Ok(())
}
fn access(
    p: &Permission,
    write: bool,
    active: &[usize],
    loans: &[Loan],
    requirements: &mut Vec<(Source, Source)>,
    b: &mut Budget,
) -> Check<()> {
    access_head(p, write, b)?;
    let delegated = ancestry(&p.loans, loans, b)?;
    access_tail(p, write, active, loans, &delegated, requirements, b)
}
fn query_access(
    p: &Permission,
    active: &[usize],
    loans: &[Loan],
    requirements: &mut Vec<(Source, Source)>,
    b: &mut Budget,
) -> Check<()> {
    access_head(p, false, b)?;
    access_tail(p, false, active, loans, &p.loans, requirements, b)
}

fn captured_definition<'a>(
    module: &'a TypedModule,
    f: &TypedFunction,
    b: &Budget,
) -> Option<&'a ClosureDefinition> {
    f.environment
        .and_then(|id| module.closures.get(id.0 as usize))
        .filter(|d| d.storage_life.is_some() || b.returned_environment_sources)
}
fn fixed_environment(module: &TypedModule, ty: &Type, b: &Budget) -> bool {
    let ty = if let Type::Address { inner, .. } = ty {
        inner.as_ref()
    } else {
        ty
    };
    matches!(ty, Type::Closure(id) if module.closures.get(id.0 as usize).is_some_and(|d| d.storage_life.is_some() || b.returned_environment_sources))
}
fn seed_environment(
    module: &TypedModule,
    ty: &Type,
    base: HoldRoot,
    out: &mut Payload,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<()> {
    let ty = if let Type::Address { inner, .. } = ty {
        inner.as_ref()
    } else {
        ty
    };
    let Type::Closure(id) = ty else {
        return Err(err("environment carrier is not actual Closure"));
    };
    if b.returned_environment_sources {
        return returned_environment::seed(module, *id, base, out, loans, b);
    }
    let d = module
        .closures
        .get(id.0 as usize)
        .ok_or_else(|| err("environment definition absent"))?;
    b.spend(d.captures.len())?;
    for (index, ty) in d.captures.iter().enumerate() {
        let capture = seed_payload(module, ty, base, loans, b)?;
        let prefix = vec![PlaceProjection::Field(index as u32)];
        for (tail, mut p) in
            std::iter::once((Vec::new(), capture.address)).chain(capture.leaves.into_vec(b)?)
        {
            if p.sources.is_empty() {
                continue;
            }
            b.spend(tail.len().saturating_add(1))?;
            let mut path = prefix.clone();
            path.extend(tail);
            for source in &mut p.sources {
                b.spend(path.len())?;
                source.leaf = path.clone();
            }
            // The corresponding abstract loan must name the same physical target.
            for loan in &p.loans {
                source_work(&p.sources, b)?;
                loans[*loan].sources = p.sources.clone();
            }
            add_leaf(out, &path, &p, b)?;
        }
    }
    Ok(())
}
fn seed_payload(
    module: &TypedModule,
    ty: &Type,
    base: HoldRoot,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<Payload> {
    let mut p = Payload::default();
    if let Type::Address { mutable, .. } = ty {
        let source = Source {
            static_dependency: StaticProvenance::NotStatic,
            retained_port: None,
            base,
            leaf: Vec::new(),
            inline: Vec::new(),
            heap_parents: Vec::new(),
            heap_element: None,
        };
        b.spend(3)?;
        let id = loans.len();
        loans.push(Loan {
            receiver_head: false,
            sources: vec![source.clone()],
            parents: Vec::new(),
            exclusive: *mutable,
        });
        b.stored_exclusive_loans |= *mutable;
        p.address = Permission {
            sources: vec![source],
            loans: vec![id],
        };
    }
    if b.stored_enum_addresses
        && matches!(ty, Type::ErrorUnion { .. })
        && enum_address::signature(ty)
    {
        for (path, mode) in enum_address::slots(ty, b)? {
            b.spend(path.len().saturating_mul(2).saturating_add(3))?;
            let source = Source {
                static_dependency: StaticProvenance::NotStatic,
                retained_port: None,
                base,
                leaf: path.clone(),
                inline: Vec::new(),
                heap_parents: Vec::new(),
                heap_element: None,
            };
            let id = loans.len();
            loans.push(Loan {
                receiver_head: false,
                sources: vec![source.clone()],
                parents: Vec::new(),
                exclusive: mode == StoredBorrowMode::ExclusiveAddress,
            });
            b.stored_exclusive_loans |= mode == StoredBorrowMode::ExclusiveAddress;
            add_leaf(
                &mut p,
                &path,
                &Permission {
                    sources: vec![source],
                    loans: vec![id],
                },
                b,
            )?;
        }
    }
    let array = if let Type::Address { inner, .. } = ty {
        inner.as_ref()
    } else {
        ty
    };
    if shared_array_shape(array, b)? {
        let Type::Array { length, .. } = array else {
            unreachable!()
        };
        b.spend((*length as usize).saturating_mul(8))?;
        for index in 0..*length {
            let path = vec![PlaceProjection::Field(index)];
            let source = Source {
                static_dependency: StaticProvenance::NotStatic,
                retained_port: None,
                base,
                leaf: path.clone(),
                inline: Vec::new(),
                heap_parents: Vec::new(),
                heap_element: None,
            };
            let id = loans.len();
            loans.push(Loan {
                receiver_head: false,
                sources: vec![source.clone()],
                parents: Vec::new(),
                exclusive: false,
            });
            add_leaf(
                &mut p,
                &path,
                &Permission {
                    sources: vec![source],
                    loans: vec![id],
                },
                b,
            )?;
        }
    }
    if let Some((_, shape)) = qualified(ty).or_else(|| addressed(ty)) {
        for slot in &shape.slots {
            b.spend(slot.path.len().saturating_mul(3).saturating_add(4))?;
            let source = Source {
                static_dependency: StaticProvenance::NotStatic,
                retained_port: None,
                base,
                leaf: slot.path.clone(),
                inline: Vec::new(),
                heap_parents: Vec::new(),
                heap_element: None,
            };
            let id = loans.len();
            loans.push(Loan {
                receiver_head: false,
                sources: vec![source.clone()],
                parents: Vec::new(),
                exclusive: slot.mode == StoredBorrowMode::ExclusiveAddress,
            });
            b.stored_exclusive_loans |= slot.mode == StoredBorrowMode::ExclusiveAddress;
            p.leaves.push(
                (
                    slot.path.clone(),
                    Permission {
                        sources: vec![source],
                        loans: vec![id],
                    },
                ),
                b,
            )?;
        }
    }
    if matches!(ty, Type::Desc(_)) {
        super::stored_flow::type_work(ty, &b.limits.clone(), b)?;
        if super::stored_owned::description(ty, b)? {
            b.spend(1)?;
            p.address.sources.push(Source {
                static_dependency: StaticProvenance::NotStatic,
                retained_port: None,
                base,
                leaf: Vec::new(),
                inline: Vec::new(),
                heap_parents: Vec::new(),
                heap_element: None,
            });
        }
        if (replacements::heads(b)
            || super::stored_owned::ordinary_store_value(module, ty, b)?
            || super::stored_owned::ordinary_byte_value(ty, b)?)
            && matches!(ty, Type::Desc(payload) if matches!(payload.as_ref(), Type::ByteBuffer | Type::Record { .. } | Type::OwnedStore { .. }))
        {
            b.spend(1)?;
            p.store_description.sources.push(Source {
                static_dependency: StaticProvenance::NotStatic,
                retained_port: None,
                base,
                leaf: Vec::new(),
                inline: Vec::new(),
                heap_parents: Vec::new(),
                heap_element: None,
            });
        }
    }
    compound::seed(ty, base, &mut p, loans, b)?;
    if let Type::Address { inner, .. } = ty
        && shared_tuple_value(module, inner, None, b)?
    {
        compound::seed(inner, base, &mut p, loans, b)?;
    }
    task_address::seed(module, ty, base, &mut p, loans, b)?;
    value_transport::seed_environments(module, ty, base, &mut p, loans, b)?;
    Ok(p)
}
fn empty_frame(module: &TypedModule, f: &TypedFunction, b: &mut Budget) -> Check<Frame> {
    b.spend(f.locals.len().saturating_add(f.parameters.len()))?;
    let count = captured_definition(module, f, b).map_or(0, |d| d.captures.len());
    b.spend(count)?;
    let mut pointees = Vec::new();
    b.spend(f.parameters.len().saturating_add(count))?;
    for (index, local) in f.parameters.iter().enumerate() {
        if matches!(f.locals[local.0 as usize].ty, Type::Address { .. }) {
            pointees.push((HoldRoot::Parameter(index as u32), Payload::default()));
        }
    }
    if let Some(definition) = captured_definition(module, f, b) {
        for (index, ty) in definition.captures.iter().enumerate() {
            if matches!(ty, Type::Address { .. }) {
                pointees.push((HoldRoot::Capture(index as u32), Payload::default()));
            }
        }
    }
    Ok(Frame {
        pending_external_update: false,
        receiver: None,
        captures: vec![Payload::default(); count],
        locals: vec![Payload::default(); f.locals.len()],
        external: vec![Payload::default(); f.parameters.len()],
        vacant: Vec::new(),
        prepared: Vec::new(),
        reference_cells: reference_cells::Cells::default(),
        pointees,
        projected_pointees:value_transport::after::projected_inputs(module,f,b)?,
    })
}
fn seed_pointee(
    module: &TypedModule,
    inner: &Type,
    base: HoldRoot,
    outer: &Payload,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<Payload> {
    let mut memory = seed_payload(module, inner, base, loans, b)?;
    // A stored pointer cell has its own Reference namespace, never the
    // outer input pointer's target. Its actual consumer owns that root.
    if matches!(inner, Type::Address { .. }) {
        memory.address = Permission::default();
    }
    match base {
        HoldRoot::Parameter(index) => {
            value_transport::seed_parameter(module, inner, index, &mut memory, b)?
        }
        HoldRoot::Capture(index) => {
            value_transport::seed_capture(module, inner, index, &mut memory, b)?
        }
        _ => {
            return Err(err(
                "input pointee seed has no original parameter or capture",
            ));
        }
    }
    if b.task_sources() {
        let input = match base {
            HoldRoot::Parameter(index) => option_address::Input::Parameter(index),
            HoldRoot::Capture(index) => option_address::Input::Capture(index),
            _ => unreachable!(),
        };
        memory.enum_variants = enum_address::Variants::full_input(input, inner, b)?;
    } else if enum_address::ordinary(module, inner, b)? {
        if let HoldRoot::Parameter(index) = base {
            memory.enum_variants = enum_address::Variants::parameter(index, inner, b)?;
        }
    }
    dyn_ports::seed(module, inner, base, &mut memory, loans, b)?;
    if fixed_environment(module, inner, b) {
        seed_environment(module, inner, base, &mut memory, loans, b)?;
    }
    // An incoming pointer's existing child rows and its current pointee view
    // describe the same initialized input loans, not independent aliases.
    for index in 0..memory.leaves.len() {
        let (path, permission) = &memory.leaves[index];
        let replacement = if let Some(original) = leaf(outer, path, b)? {
            permission_work(original, b)?;
            permission_work(permission, b)?;
            (original.sources == permission.sources).then(|| original.clone())
        } else {
            None
        };
        if let Some(permission) = replacement {
            *memory.leaves.permission_mut(index) = permission;
        }
    }
    for (path, permission) in &mut memory.task_ports {
        b.spend(
            outer
                .task_ports
                .len()
                .saturating_mul(path.len().saturating_add(1)),
        )?;
        if let Some((_, old)) = outer.task_ports.iter().find(|(key, _)| key == path) {
            permission_work(old, b)?;
            permission_work(permission, b)?;
            if old.sources == permission.sources {
                *permission = old.clone();
            }
        }
    }
    for (path, port) in &mut memory.dyn_ports.0 {
        if let Some(index) = dyn_ports::find(&outer.dyn_ports, path, b)? {
            let old = &outer.dyn_ports.0[index].1;
            permission_work(&old.permission, b)?;
            permission_work(&port.permission, b)?;
            if old.permission.sources == port.permission.sources {
                port.permission = old.permission.clone();
            }
        }
    }
    Ok(memory)
}
fn seed(
    module: &TypedModule,
    f: &TypedFunction,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<Frame> {
    let mut state = empty_frame(module, f, b)?;
    if receiver::needs_head(module, f, b)? {
        b.spend(2)?;
        state.receiver = Some(loans.len());
        loans.push(Loan::default());
    }
    for (position, local) in f.parameters.iter().enumerate() {
        b.spend(1)?;
        let ty = &f.locals[local.0 as usize].ty;
        let mut p = seed_payload(module, ty, HoldRoot::Parameter(position as u32), loans, b)?;
        value_transport::seed_parameter(module, ty, position as u32, &mut p, b)?;
        if enum_address::ordinary(module, ty, b)? {
            p.enum_variants = enum_address::Variants::parameter(position as u32, ty, b)?;
        } else if option_address::candidate(ty) {
            b.spend(96)?;
            if option_address::signature(ty) {
                p.option_variants = option_address::Variants::parameter(position as u32, b)?;
            }
        }
        if b.task_sources() {
            p.enum_variants = enum_address::Variants::full_parameter(position as u32, ty, b)?;
        }
        dyn_ports::seed(
            module,
            ty,
            HoldRoot::Parameter(position as u32),
            &mut p,
            loans,
            b,
        )?;
        if fixed_environment(module, ty, b) {
            seed_environment(
                module,
                ty,
                HoldRoot::Parameter(position as u32),
                &mut p,
                loans,
                b,
            )?;
        }
        if addressed(ty).is_some()
            || b.dyn_contexts && matches!(ty, Type::Address { .. }) && !p.dyn_ports.0.is_empty()
            || fixed_environment(module, ty, b)
            || matches!(ty, Type::Address { inner, .. } if shared_array_value(inner))
            || !p.leaves.is_empty()
            || !p.task_ports.is_empty()
            || !p.dyn_ports.0.is_empty()
            || !p.field_facts.is_empty()
        {
            state.external[position] = copy_payload(&p, b)?;
        }
        if let Type::Address { inner, .. } = ty {
            let memory = seed_pointee(
                module,
                inner,
                HoldRoot::Parameter(position as u32),
                &p,
                loans,
                b,
            )?;
            *memory_holder_mut(&mut state, HoldRoot::Parameter(position as u32), b)?
                .ok_or_else(|| err("parameter pointee directory was not registered"))? = memory;
        }
        state.locals[local.0 as usize] = p;
    }
    if let Some(d) = captured_definition(module, f, b) {
        b.spend(d.captures.len())?;
        for (index, ty) in d.captures.iter().enumerate() {
            let base = HoldRoot::Capture(index as u32);
            let mut payload = seed_payload(module, ty, base, loans, b)?;
            value_transport::seed_capture(module, ty, index as u32, &mut payload, b)?;
            if b.task_sources() {
                payload.enum_variants = enum_address::Variants::full_input(
                    option_address::Input::Capture(index as u32),
                    ty,
                    b,
                )?;
            }
            dyn_ports::seed(module, ty, base, &mut payload, loans, b)?;
            if fixed_environment(module, ty, b) {
                seed_environment(module, ty, base, &mut payload, loans, b)?;
            }
            static_borrows::seed_capture(d, index as u32, &mut payload, loans, b)?;
            if let Type::Address { inner, .. } = ty {
                let memory = seed_pointee(module, inner, base, &payload, loans, b)?;
                *memory_holder_mut(&mut state, base, b)?
                    .ok_or_else(|| err("capture pointee directory was not registered"))? = memory;
            }
            state.captures[index] = payload;
        }
    }
    reference_cells::seed(module, f, &mut state, loans, b)?;
    Ok(state)
}
fn select(
    state: &Frame,
    address: &Permission,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<Permission> {
    if address.sources.is_empty() {
        return Ok(Permission::default());
    }
    let mut result = Permission::default();
    for source in &address.sources {
        b.spend(width(source))?;
        if source.base == HoldRoot::Static {
            return Err(err("static text cannot supply nested carrier permissions"));
        }
        let Some((p,actual_path))=memory_location(state,source,path,b)? else {
            return Ok(Permission::default());
        };
        let Some(p) = leaf(p, &actual_path, b)? else {
            return Ok(Permission::default());
        };
        if p.sources.is_empty() {
            return Ok(Permission::default());
        }
        join_permission(&mut result, p, b)?;
    }
    Ok(result)
}

// A nonempty leaf is an abstract target namespace, not a physical GEP.
// Only an actual seeded pointer row with that exact symbolic target can
// establish a private pointee directory inside a closure helper.
fn memory_location<'a>(state:&'a Frame,source:&Source,path:&[PlaceProjection],b:&mut Budget)->Check<Option<(&'a Payload,Path)>> {
    if let Some(payload)=value_transport::after::projected_memory(&state.projected_pointees,source,b)? {
        b.spend(source.inline.len().saturating_add(path.len()))?;
        let mut actual=source.inline.clone();actual.extend(path);
        return Ok(Some((payload,actual)));
    }
    let Some(holder)=memory_holder(state,source.base,b)? else{return Ok(None);};
    Ok(symbolic_slot_path(holder,source,path,b)?.map(|position|(holder,position)))
}
fn symbolic_slot_path(
    holder: &Payload,
    source: &Source,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<Option<Path>> {
    b.spend(
        source
            .leaf
            .len()
            .saturating_add(source.inline.len())
            .saturating_add(path.len()),
    )?;
    if !source.leaf.is_empty() {
        if source.retained_port.is_some()
            || source.heap_element.is_some()
            || !source.heap_parents.is_empty()
        {
            return Err(err(
                "opaque or heap targets need their own complete directory",
            ));
        }
        let Some(head) = leaf(holder, &source.leaf, b)? else {
            return Ok(None);
        };
        source_work(&head.sources, b)?;
        if head.sources.is_empty() {
            return Ok(None);
        }
        if !head.sources.iter().any(|known| {
            known.base == source.base
                && known.leaf == source.leaf
                && known.retained_port.is_none()
                && known.inline.is_empty()
                && known.heap_element.is_none()
                && known.heap_parents.is_empty()
        }) {
            return Err(err(
                "symbolic pointee lacks an actual captured head binding",
            ));
        }
    }
    let mut complete = source.leaf.clone();
    complete.extend(&source.inline);
    complete.extend(path);
    Ok(Some(complete))
}
fn slot_position(address: &Permission, path: &[PlaceProjection], b: &mut Budget) -> Check<Source> {
    let [root] = address.sources.as_slice() else {
        return Err(err("stored slot needs one actual inline carrier"));
    };
    if !root.leaf.is_empty() {
        return Err(err("stored slot crosses a target boundary"));
    }
    b.spend(width(root).saturating_add(path.len()))?;
    let mut slot = root.clone();
    slot.inline.extend_from_slice(path);
    Ok(slot)
}

fn require_filled(
    state: &Frame,
    address: &Permission,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<()> {
    if state.vacant.is_empty() {
        return Ok(());
    }
    for root in &address.sources {
        b.spend(width(root).saturating_add(path.len()))?;
        let mut slot = root.clone();
        slot.inline.extend_from_slice(path);
        b.spend(amount(state.vacant.len().saturating_add(1), width(&slot))?)?;
        if contains_target(&state.vacant, &slot, b)? {
            return Err(err("stored slot is vacant on an incoming path"));
        }
    }
    Ok(())
}

fn clear_local_holes(state: &mut Frame, local: LocalId, b: &mut Budget) -> Check<()> {
    b.spend(state.vacant.len())?;
    state
        .vacant
        .retain(|slot| slot.base != HoldRoot::Local(local));
    Ok(())
}

/// Only exact SSA loads whose *every* use observes a projection's address
/// head can defer the whole-pointee reborrow. The projection itself creates
/// and checks its real narrower loan. A stored, passed, returned or otherwise
/// consumed head retains the ordinary whole reborrow, even if it also has a
/// projection use. Unreachable uses participate in this classification too.
fn projection_head_loads(f: &TypedFunction, b: &mut Budget) -> Check<Vec<bool>> {
    b.spend(
        f.values
            .len()
            .saturating_mul(2)
            .saturating_add(f.blocks.len()),
    )?;
    let mut marks = vec![0u8; f.values.len()];
    for block in &f.blocks {
        for i in &block.instructions {
            b.spend(1)?;
            if let InstructionKind::Load(local) = i.kind
                && let Some(result) = i.result
                && matches!(vt(f, result)?, Type::Address { mutable: true, .. })
            {
                let actual = f
                    .locals
                    .get(local.0 as usize)
                    .ok_or_else(|| err("projected head load has no actual local"))?;
                type_work(&actual.ty, &b.limits.clone(), b)?;
                type_work(vt(f, result)?, &b.limits.clone(), b)?;
                if actual.ty != *vt(f, result)? {
                    return Err(err("projected head load changes its complete local type"));
                }
                marks[result.0 as usize] = 1;
            }
        }
    }
    for block in &f.blocks {
        for i in &block.instructions {
            b.spend(1)?;
            let head = match i.kind {
                InstructionKind::AddressField { address, .. }
                | InstructionKind::AddressIndex { address, .. }
                | InstructionKind::AddressSlice { address, .. } => Some(address),
                _ => None,
            };
            for operand in instruction_operands(&i.kind) {
                b.spend(1)?;
                let mark = marks
                    .get_mut(operand.0 as usize)
                    .ok_or_else(|| err("projected head use has no actual SSA value"))?;
                if *mark & 1 != 0 {
                    *mark |= if head == Some(operand) { 2 } else { 4 };
                }
            }
        }
        b.spend(1)?;
        let value = match block.terminator {
            Terminator::Return(value) => value,
            Terminator::Branch { condition, .. } => Some(condition),
            _ => None,
        };
        if let Some(value) = value {
            let mark = marks
                .get_mut(value.0 as usize)
                .ok_or_else(|| err("projected head escapes through an absent actual value"))?;
            if *mark & 1 != 0 {
                *mark |= 4;
            }
        }
    }
    Ok(marks.into_iter().map(|mark| mark == 3).collect())
}

fn is_deferred_address(i: &Instruction, heads: &[bool]) -> bool {
    i.result
        .and_then(|id| heads.get(id.0 as usize))
        .copied()
        .unwrap_or(false)
}

/// Only a completely matched original assignment may reserve its lvalue
/// address until the actual WriteAddress. Every intermediate SSA head has one
/// use in that same nonescaping projection chain; explicit borrows, calls,
/// stores and returns are never reservations. The original parent loans and
/// exact roots remain on every view and are checked at the actual write.
fn assignment_reservations(
    f: &TypedFunction,
    writes: &[ValueId],
    b: &mut Budget,
) -> Check<Vec<bool>> {
    let count = f.values.len();
    b.spend(count.saturating_mul(5).saturating_add(f.blocks.len()))?;
    let mut candidate = vec![false; count];
    let mut uses = vec![0u8; count];
    let mut next = vec![usize::MAX; count];
    let mut state = vec![0u8; count];
    for block in &f.blocks {
        for instruction in &block.instructions {
            b.spend(1)?;
            if matches!(
                instruction.kind,
                InstructionKind::Load(_)
                    | InstructionKind::AddressField { .. }
                    | InstructionKind::AddressIndex { .. }
                    | InstructionKind::AddressSlice { .. }
            ) && let Some(result) = instruction.result
                && matches!(vt(f, result)?, Type::Address { mutable: true, .. })
            {
                candidate[result.0 as usize] = true;
            }
        }
    }
    for block in &f.blocks {
        for instruction in &block.instructions {
            b.spend(1)?;
            for operand in instruction_operands(&instruction.kind) {
                b.spend(1)?;
                let index = operand.0 as usize;
                if !candidate
                    .get(index)
                    .copied()
                    .ok_or_else(|| err("assignment reservation operand is absent"))?
                {
                    continue;
                }
                uses[index] = uses[index].saturating_add(1).min(2);
                if uses[index] != 1 {
                    next[index] = usize::MAX;
                    continue;
                }
                next[index] = match instruction.kind {
                    InstructionKind::AddressField { address, .. }
                    | InstructionKind::AddressIndex { address, .. }
                    | InstructionKind::AddressSlice { address, .. }
                        if address == operand =>
                    {
                        instruction
                            .result
                            .filter(|value| candidate[value.0 as usize])
                            .map_or(usize::MAX, |value| value.0 as usize)
                    }
                    InstructionKind::WriteAddress { address, .. } if address == operand => {
                        b.spend(writes.len().saturating_add(1))?;
                        if writes.contains(&address) {
                            count
                        } else {
                            usize::MAX
                        }
                    }
                    _ => usize::MAX,
                };
            }
        }
        b.spend(1)?;
        let value = match block.terminator {
            Terminator::Return(value) => value,
            Terminator::Branch { condition, .. } => Some(condition),
            _ => None,
        };
        if let Some(value) = value {
            let use_count = uses
                .get_mut(value.0 as usize)
                .ok_or_else(|| err("assignment reservation terminator operand is absent"))?;
            *use_count = 2;
            next[value.0 as usize] = usize::MAX;
        }
    }
    for start in 0..count {
        b.spend(1)?;
        if state[start] != 0 {
            continue;
        }
        let mut path = Vec::new();
        let mut current = start;
        let valid = loop {
            b.spend(1)?;
            if state[current] != 0 {
                break state[current] == 3;
            }
            if !candidate[current] || uses[current] != 1 || next[current] == usize::MAX {
                state[current] = 2;
                break false;
            }
            state[current] = 1;
            if path.len() == path.capacity() {
                b.spend(path.len().saturating_add(1))?;
            }
            path.push(current);
            if next[current] == count {
                break true;
            }
            current = next[current];
        };
        for value in path {
            b.spend(1)?;
            state[value] = if valid { 3 } else { 2 };
        }
    }
    Ok(state.into_iter().map(|state| state == 3).collect())
}

fn deferred_addresses(module: &TypedModule, f: &TypedFunction, b: &mut Budget) -> Check<Vec<bool>> {
    let mut heads = projection_head_loads(f, b)?;
    if module.semantic.is_none() {
        return Ok(heads);
    }
    b.spend(f.blocks.len())?;
    let mut has_write = false;
    for block in &f.blocks {
        b.spend(block.instructions.len())?;
        has_write |= block
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.kind, InstructionKind::WriteAddress { .. }));
    }
    if has_write {
        let writes = crate::dynamic_witness::semantic_patterns::assignment_writes(module, f, b)?;
        if !writes.is_empty() {
            let reservations = assignment_reservations(f, &writes, b)?;
            b.spend(heads.len())?;
            for (head, reservation) in heads.iter_mut().zip(reservations) {
                *head |= reservation;
            }
        }
    }
    Ok(heads)
}

fn projection_only_load(
    f: &TypedFunction,
    instruction: &Instruction,
    storage: ValueId,
    b: &mut Budget,
) -> Check<Option<LocalId>> {
    let mut adjacent = None;
    let mut uses = 0usize;
    for block in &f.blocks {
        for (index, actual) in block.instructions.iter().enumerate() {
            b.spend(1)?;
            if std::ptr::eq(actual, instruction)
                && index > 0
                && block.instructions[index - 1].result == Some(storage)
                && let InstructionKind::Load(local) = block.instructions[index - 1].kind
                && addressed(&f.locals[local.0 as usize].ty).is_some()
            {
                adjacent = Some(local);
            }
            for operand in instruction_operands(&actual.kind) {
                b.spend(1)?;
                uses += usize::from(operand == storage);
            }
        }
        b.spend(1)?;
        uses += usize::from(matches!(block.terminator,
            Terminator::Return(Some(value)) if value == storage));
        uses += usize::from(matches!(block.terminator,
            Terminator::Branch { condition, .. } if condition == storage));
    }
    Ok((uses == 1).then_some(adjacent).flatten())
}
fn instantiate(
    module: &TypedModule,
    f: &TypedFunction,
    source: &Source,
    args: &[ValueId],
    values: &[Payload],
    state: &Frame,
    b: &mut Budget,
) -> Check<Permission> {
    if source.base == HoldRoot::Static {
        if !source.leaf.is_empty() || !source.inline.is_empty() {
            return Err(err("static text summary contains a nested projection"));
        }
        b.spend(1)?;
        return Ok(Permission {
            sources: vec![source.clone()],
            loans: Vec::new(),
        });
    }
    let HoldRoot::Parameter(position) = source.base else {
        return Err(err("callee returned activation or capture storage"));
    };
    let v = args
        .get(position as usize)
        .and_then(|v| values.get(v.0 as usize))
        .ok_or_else(|| err("callee source has no actual argument"))?;
    if matches!(source.retained_port, Some(PortOrigin::Reference { .. })) {
        return reference_cells::substitute(module, f, source, args, values, state, b);
    }
    let mut p = if let Some(port) = &source.retained_port {
        match port {
            PortOrigin::Reference { .. } => {
                unreachable!("raw reference handled before opaque selection")
            }
            PortOrigin::Task { carrier, port } => {
                task_address::select_carrier(v, &source.leaf, carrier, *port, b)?
            }
            PortOrigin::Dyn { carrier, port } => {
                if !source.leaf.is_empty() {
                    return Err(err(
                        "Dyn port source has a noncanonical second carrier path",
                    ));
                }
                dyn_ports::select_actual(
                    module,
                    f,
                    args[position as usize],
                    v,
                    state,
                    carrier,
                    *port,
                    b,
                )?
            }
        }
    } else if source.leaf.is_empty() && !v.store_description.sources.is_empty() {
        permission_work(&v.store_description, b)?;
        v.store_description.clone()
    } else if source.leaf.is_empty() {
        permission_work(&v.address, b)?;
        v.address.clone()
    } else if !v.leaves.is_empty()
        && !v.address.sources.is_empty()
        && v.address.sources.iter().all(|root| {
            root.leaf.is_empty()
                && root.inline.is_empty()
                && match root.base {
                    HoldRoot::Local(local) => state.locals.get(local.0 as usize).is_some(),
                    HoldRoot::Parameter(index) => state.external.get(index as usize).is_some(),
                    HoldRoot::Static | HoldRoot::Capture(_) => false,
                }
        })
    {
        // A mutable carrier's current slot comes from the frame. The loaded
        // SSA payload can predate an earlier call or field replacement.
        select(state, &v.address, &source.leaf, b)?
    } else if let Some(p) = leaf(v, &source.leaf, b)? {
        permission_work(p, b)?;
        p.clone()
    } else {
        select(state, &v.address, &source.leaf, b)?
    };
    for target in &mut p.sources {
        store_paths::compose(target, source, b)?;
    }
    Ok(p)
}

fn write_sites(
    module: &TypedModule,
    f: &TypedFunction,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<WriteSites> {
    let mut sites = Vec::new();
    if !b.stored_slot_updates
        && !b.stored_constructor_sites
        && !b.nested_callable_sources
        && !b.replacements
        && !b.stored_borrows
        && !module.closures.iter().any(receiver::declared)
    {
        return Ok(sites);
    }
    b.spend(f.blocks.len())?;
    for block in &f.blocks {
        b.spend(block.instructions.len())?;
        for i in &block.instructions {
            let site = i as *const Instruction as usize;
            let capture_paths = captured_owner::write_paths(module, f, &i.kind, b)?;
            receiver::add_site(module, f, i, &mut sites, loans, b)?;
            let mut add =
                |parameter: u32, path: &Path, exclusive: bool, b: &mut Budget| -> Check<()> {
                    b.spend(path.len().saturating_add(sites.len()).saturating_add(2))?;
                    if let Some((_, _, _, _, old_mode)) =
                        sites.iter().find(|(at, p, selected, _, _)| {
                            *at == site && *p == parameter && selected == path
                        })
                    {
                        if *old_mode != exclusive {
                            return Err(err(
                                "one actual stored write site changes its complete target mode",
                            ));
                        }
                        return Ok(());
                    }
                    let id = loans.len();
                    loans.push(Loan::default());
                    b.stored_exclusive_loans |= exclusive;
                    sites.push((site, parameter, path.clone(), id, exclusive));
                    Ok(())
                };
            match &i.kind {
                InstructionKind::MakeAggregate { .. }
                | InstructionKind::MakePackage { .. }
                | InstructionKind::MakeOwnedTuple { .. } => {
                    if let Some((_, shape)) = qualified(vt(
                        f,
                        i.result.ok_or_else(|| err("aggregate lacks result"))?,
                    )?) {
                        for slot in &shape.slots {
                            if slot.mode == StoredBorrowMode::ExclusiveAddress {
                                add(u32::MAX, &slot.path, true, b)?;
                            }
                        }
                    }
                }
                InstructionKind::CommitPlaceReplace { value, .. } => {
                    if let Some((_, shape)) = qualified(vt(f, *value)?) {
                        for slot in &shape.slots {
                            if slot.mode == StoredBorrowMode::ExclusiveAddress {
                                add(u32::MAX, &slot.path, true, b)?;
                            }
                        }
                    }
                }
                InstructionKind::StoreStoredBorrow { path, value, .. } => add(
                    u32::MAX,
                    path,
                    matches!(vt(f, *value)?, Type::Address { mutable: true, .. }),
                    b,
                )?,
                InstructionKind::Call { function, .. }
                | InstructionKind::SuspendCall { function, .. } => {
                    let callee = module
                        .functions
                        .get(function.0 as usize)
                        .ok_or_else(|| err("stored call target is absent"))?;
                    for (position, local) in callee.parameters.iter().enumerate() {
                        if let Type::Address {
                            mutable: true,
                            inner,
                        } = &callee.locals[local.0 as usize].ty
                            && let Some((_, shape)) = qualified(inner)
                        {
                            for slot in &shape.slots {
                                add(
                                    position as u32,
                                    &slot.path,
                                    slot.mode == StoredBorrowMode::ExclusiveAddress,
                                    b,
                                )?;
                            }
                        }
                        let parameter_type = &callee.locals[local.0 as usize].ty;
                        if let Some(id) = returned_environment::closure(parameter_type)
                            && matches!(
                                parameter_type,
                                Type::Closure(_) | Type::Address { mutable: true, .. }
                            )
                        {
                            for (path, ty) in returned_environment::leaves(module, id, b)? {
                                add(
                                    position as u32,
                                    &path,
                                    matches!(
                                        ty,
                                        Type::Address { mutable: true, .. } | Type::Desc(_)
                                    ),
                                    b,
                                )?;
                            }
                        }
                    }
                }
                kind if captured_owner::mutable_call(module, f, kind).is_some() => {
                    for (path, exclusive) in &capture_paths {
                        add(u32::MAX, path, *exclusive, b)?;
                    }
                }
                _ => {}
            }
            let callee = match &i.kind {
                InstructionKind::Call { function, .. }
                | InstructionKind::SuspendCall { function, .. } => {
                    module.functions.get(function.0 as usize)
                }
                kind => stored_callables::called(module, f, kind).and_then(|(definition, _)| {
                    module.functions.get(definition.function.0 as usize)
                }),
            };
            if let Some(callee) = callee {
                for (parameter, path, exclusive) in value_transport::after_sites(module, callee, b)?
                {
                    add(parameter, &path, exclusive, b)?;
                }
            }
        }
    }
    Ok(sites)
}

fn retain_written_slot(
    source: &Permission,
    i: &Instruction,
    parameter: u32,
    path: &Path,
    sites: &WriteSites,
    loans: &mut [Loan],
    b: &mut Budget,
) -> Check<Permission> {
    let site = i as *const Instruction as usize;
    b.spend(sites.len().saturating_mul(path.len().saturating_add(1)))?;
    let (_, _, _, id, exclusive) = sites
        .iter()
        .find(|(at, p, selected, _, _)| *at == site && *p == parameter && selected == path)
        .ok_or_else(|| err("stored write has no stable loan site"))?;
    let row = loans
        .get_mut(*id)
        .ok_or_else(|| err("stored write loan exceeds actual arena"))?;
    add_sources(&mut row.sources, &source.sources, b)?;
    add_loans(&mut row.parents, &source.loans, b)?;
    row.parents.retain(|parent| parent != id);
    row.exclusive = *exclusive;
    b.stored_exclusive_loans |= row.exclusive;
    permission_work(source, b)?;
    Ok(Permission {
        sources: source.sources.clone(),
        loans: vec![*id],
    })
}

#[allow(clippy::too_many_arguments)]
fn step(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &mut Frame,
    values: &[Payload],
    summaries: &[Summary],
    sites: &WriteSites,
    dyn_sites: &mut dyn_leases::Sites,
    loans: &mut Vec<Loan>,
    deferred_projection: bool,
    b: &mut Budget,
) -> Check<Payload> {
    b.spend(1)?;
    let mut out = Payload::default();
    let val = |v: ValueId| {
        values
            .get(v.0 as usize)
            .ok_or_else(|| err("operand outside actual value table"))
    };
    let value_updates = match &i.kind {
        InstructionKind::Call { function, .. } | InstructionKind::SuspendCall { function, .. } => {
            let callee = module
                .functions
                .get(function.0 as usize)
                .ok_or_else(|| err("value after-state function is absent"))?;
            let before = call_results::Before::function(module, f, i, values, state, b)?;
            value_transport::prepare_after(
                &before,
                callee,
                &summaries[function.0 as usize],
                i,
                sites,
                loans,
                b,
            )?
        }
        kind if stored_callables::called(module, f, kind).is_some() => {
            let (definition, _) = stored_callables::called(module, f, kind)
                .ok_or_else(|| err("value after-state callback is absent"))?;
            let callee = module
                .functions
                .get(definition.function.0 as usize)
                .ok_or_else(|| err("value after-state callback entry is absent"))?;
            let before = call_results::Before::closure(module, f, i, values, state, b)?;
            value_transport::prepare_after(
                &before,
                callee,
                &summaries[definition.function.0 as usize],
                i,
                sites,
                loans,
                b,
            )?
        }
        _ => value_transport::PreparedAfter::default(),
    };
    let capture_updates =
        captured_owner::prepare_after(module, f, i, state, values, summaries, sites, loans, b)?;
    let reference_updates =
        reference_cells::prepare_call(module, f, i, summaries, values, state, dyn_sites, loans, b)?;
    let dyn_capture_updates =
        dyn_captures::prepare_after(module, f, i, state, values, summaries, dyn_sites, loans, b)?;
    let task_tags = enum_address::tags::transfer(f, i, state, values, b)?;
    let mut unpacked_tags = Vec::new();
    if b.task_sources()
        && let InstructionKind::UnpackTuple { local, fields } = &i.kind
    {
        b.spend(fields.len())?;
        for (index, field) in fields.iter().enumerate() {
            unpacked_tags.push((
                *field,
                state.locals[local.0 as usize]
                    .enum_variants
                    .project(&[PlaceProjection::Field(index as u32)], b)?,
            ));
        }
    }
    let dyn_transfer = dyn_ports::transfer(module, f, i, state, values, dyn_sites, loans, b)?;
    let closed_dyn_transfer = dyn_closed::transfer(module, f, i, state, values, b)?;
    let task_transfer =
        task_address::transfer(module, f, i, state, values, summaries, dyn_sites, loans, b)?;
    let failure_view = failure_projection(f, i, b)?;
    let failure_wrapper = task_failure::wrapper(f, i, state, values, b)?;
    let unpack_failure = failure_unpack(f, i, b)?;
    let enum_transfer = enum_address::transfer(module, f, i, values, b)?;
    let option_transfer = if enum_transfer.is_none() && task_transfer.is_none() {
        option_address::transfer(module, f, i, values, b)?
    } else {
        None
    };
    match &i.kind {
        _ if callable_cells::operation(module,f,i,b)? => { callable_cells::transfer(f,i,state,values,&mut out,b)?; }
        InstructionKind::UnpackTuple { local, fields } if anonymous_tuple::operation(f, i, b)? => {
            anonymous_tuple::unpack(f, *local, fields, state, b)?;
        }
        InstructionKind::DropTuple { local } if anonymous_tuple::operation(f, i, b)? => {
            state.locals[local.0 as usize] = Payload::default();
            clear_local_holes(state, *local, b)?;
        }
        InstructionKind::ExtractField { aggregate, index } if anonymous_tuple::operation(f, i, b)? => {
            out=value_transport::project(val(*aggregate)?,&[PlaceProjection::Field(*index)],vt(f,i.result.ok_or_else(||err("anonymous snapshot result is absent"))?)?,b)?;
        }
        InstructionKind::UnpackTuple { local, fields } if compound::operation(f, i, b)? => {
            compound::unpack(f,*local, fields, state, b)?;
        }
        InstructionKind::DropTuple { local } if compound::operation(f, i, b)? => {
            state.locals[local.0 as usize] = Payload::default();
            clear_local_holes(state, *local, b)?;
        }
        _ if crate::tasks::input_scope::registration_operation(f, i, b)? => {
            // No stored permission is synthesized for the actual zero scope.
            // Other Verify consumers still prove the provider, current phase,
            // linear owner, and the unique checked release of the same head.
        }

        _ if pure_copy::ordinary_owned_tuple(module,f,i,b)?=>{
            pure_copy::ordinary_tuple_transfer(f,i,state,values,&mut out,b)?;
        }
        InstructionKind::UnpackTuple { local, fields } if unpack_failure => {
            task_failure::unpack_value(f, *local, fields, state, b)?;
        }
        InstructionKind::UnpackTuple { local, fields }
            if b.task_sources() && crate::tasks::input_scope::neutral_owner_unpack(f, i, b)? => {
                b.spend(fields.len().saturating_add(1))?;
                state.locals[local.0 as usize] = Payload::default();
                for field in fields { state.locals[field.0 as usize] = Payload::default(); }
                // No data source is invented. The independently checked
                // affine sum/tuple and protocol facts move the actual debts.
        }
        InstructionKind::TaskCreateOwned { arguments, .. } if b.task_failure_carriers => {
            out = task_transfer.unwrap_or_default();
            task_failure::create(f, i, arguments, values, &mut out, b)?;
        }
        InstructionKind::LoadStoredBorrow { storage, .. } if failure_view.is_some() => {
            let Type::Address { inner, .. } = vt(f, *storage)? else {
                unreachable!()
            };
            let Type::BorrowedStorage { life, .. } = inner.as_ref() else {
                unreachable!()
            };
            task_failure::take_view(state, &val(*storage)?.address, life, vt(f,i.result.ok_or_else(||err("failure Inputs view result is absent"))?)?, &mut out, b)?;
        }
        _ if replacements::operation(&i.kind) => {
            replacements::step(f, i, state, values, sites, loans, b)?;
        }
        InstructionKind::UpdateField{..} if compound::operation(f,i,b)?=>{out=compound::update(f,i,values,b)?;}
        InstructionKind::ExtractField { aggregate, index } if compound::projection(f, i, b)? => {
            let ty=vt(f,i.result.ok_or_else(||err("compound field projection lacks its actual result"))?)?;
            out=compound::field_payload(val(*aggregate)?,*index,ty,b)?;
        }
        InstructionKind::StaticUtf8 { .. } => {
            b.spend(1)?;
            out.address.sources.push(Source {
                static_dependency: StaticProvenance::Literal,
                retained_port: None,
                base: HoldRoot::Static,
                leaf: Vec::new(),
                inline: Vec::new(),
                heap_parents: Vec::new(),
                heap_element: None,
            });
        }
        InstructionKind::Load(l) => {
            out = copy_payload(
                state
                    .locals
                    .get(l.0 as usize)
                    .ok_or_else(|| err("local outside table"))?,
                b,
            )?
        }
        InstructionKind::Store { local, value } => {
            clear_local_holes(state, *local, b)?;
            state.locals[local.0 as usize] = copy_payload(val(*value)?, b)?
        }
        InstructionKind::BindPackageCapture { local, index } => {
            // Bind is an alias to the checked physical capture, not a fresh
            // parameter of the same qualified type. Keep every leaf's actual
            // source and parent loan when installing its entry-local view.
            if let Some(capture) = state.captures.get(*index as usize) {
                let payload = copy_payload(capture, b)?;
                clear_local_holes(state, *local, b)?;
                state.locals[local.0 as usize] = payload;
            }
        }
        InstructionKind::EndPackageCapture { local, index } => {
            if let Some(capture) = state.captures.get_mut(*index as usize) {
                *capture = copy_payload(&state.locals[local.0 as usize], b)?;
                state.locals[local.0 as usize] = Payload::default();
                clear_local_holes(state, *local, b)?;
            }
        }
        InstructionKind::BorrowPlaceLocal { local, path, .. }
            if ordinary_error_borrow(module,f, i, b)? =>
        {
            b.spend(path.len().saturating_add(1))?;
            out.address.sources.push(Source {
static_dependency: StaticProvenance::NotStatic,
                retained_port: None,
                base: HoldRoot::Local(*local),
                leaf: Vec::new(),
                inline: path.clone(),
                heap_parents: Vec::new(),
                heap_element: None,
            });
        }
        InstructionKind::TakePackageField { local, index } => {
            let holder = state
                .locals
                .get_mut(local.0 as usize)
                .ok_or_else(|| err("taken Package field local is absent"))?;
            out = value_transport::take(holder,&[PlaceProjection::Field(*index)],value_transport::field_type(&f.locals[local.0 as usize].ty,*index)?,b)?;
            b.spend(state.vacant.len())?;
            state.vacant.retain(|slot| {
                !(slot.base == HoldRoot::Local(*local)
                    && slot.inline.first() == Some(&PlaceProjection::Field(*index)))
            });
        }
        InstructionKind::DropPackageField { local, index } => {
            let holder = state
                .locals
                .get_mut(local.0 as usize)
                .ok_or_else(|| err("dropped Package field local is absent"))?;
            value_transport::take(holder,&[PlaceProjection::Field(*index)],value_transport::field_type(&f.locals[local.0 as usize].ty,*index)?,b)?;
            b.spend(state.vacant.len())?;
            state.vacant.retain(|slot| {
                !(slot.base == HoldRoot::Local(*local)
                    && slot.inline.first() == Some(&PlaceProjection::Field(*index)))
            });
        }
        InstructionKind::StorePackageField {
            local,
            index,
            value,
        } => {
            let source = copy_payload(val(*value)?, b)?;
            let holder = state
                .locals
                .get_mut(local.0 as usize)
                .ok_or_else(|| err("stored Package field local is absent"))?;
            value_transport::take(holder,&[PlaceProjection::Field(*index)],value_transport::field_type(&f.locals[local.0 as usize].ty,*index)?,b)?;
            value_transport::attach(holder,&[PlaceProjection::Field(*index)],vt(f,*value)?,&source,b)?;
            b.spend(state.vacant.len())?;
            state.vacant.retain(|slot| {
                !(slot.base == HoldRoot::Local(*local)
                    && slot.inline.first() == Some(&PlaceProjection::Field(*index)))
            });
        }
        InstructionKind::StorageLive(l)
        | InstructionKind::StorageDead(l)
        | InstructionKind::DropClosure { local: l } => {
            state.locals[l.0 as usize] = Payload::default();
            clear_local_holes(state, *l, b)?;
            dyn_ports::vacate_owner(module, f, *l, state, b)?;
        }
        InstructionKind::BorrowPackageField { local, index, mutable: false }
            if pure_copy::managed_package_field_borrow(f, i, b)? => {
                b.spend(2)?;
                out.address.sources.push(Source {
static_dependency: StaticProvenance::NotStatic, retained_port: None,
                    base: HoldRoot::Local(*local), leaf: Vec::new(),
                    inline: vec![PlaceProjection::Field(*index)], heap_parents: Vec::new(), heap_element: None });
            }
        InstructionKind::DescribePackageField { local, index }
            if pure_copy::qualified_package_description(f, i, b)? => {
                b.spend(2)?;
                out.address.sources.push(Source {
static_dependency: StaticProvenance::NotStatic,
                    retained_port: None, base: HoldRoot::Local(*local),
                    leaf: Vec::new(), inline: vec![PlaceProjection::Field(*index)],
                    heap_parents: Vec::new(), heap_element: None,
                });
            }
        InstructionKind::DescribeLocal { local } => {
            if let Some(local_ty) = f.locals.get(local.0 as usize)
                && super::stored_owned::owner(&local_ty.ty, b)?
            {
                b.spend(1)?;
                out.address.sources.push(Source {
static_dependency: StaticProvenance::NotStatic,
                    retained_port: None,
                    base: HoldRoot::Local(*local),
                    leaf: Vec::new(),
                    inline: Vec::new(),
                    heap_parents: Vec::new(),
                    heap_element: None,
                });
            }
            if (replacements::heads(b)||super::stored_owned::ordinary_store_value(module,&f.locals[local.0 as usize].ty,b)?||super::stored_owned::ordinary_byte_value(&f.locals[local.0 as usize].ty,b)?)
                && f.locals.get(local.0 as usize).is_some_and(|local| matches!(&local.ty, Type::Managed(payload) if matches!(payload.as_ref(), Type::ByteBuffer | Type::Record { .. } | Type::OwnedStore { .. })))
            {
                b.spend(1)?;
                out.store_description.sources.push(Source {
static_dependency: StaticProvenance::NotStatic,
                    retained_port: None,
                    base: HoldRoot::Local(*local),
                    leaf: Vec::new(),
                    inline: Vec::new(),
                    heap_parents: Vec::new(),
                    heap_element: None,
                });
            }
        }
        InstructionKind::DescribePackageField { local, index }
            if (replacements::heads(b)||super::stored_owned::ordinary_store_value(module,vt(f,i.result.ok_or_else(||err("ordinary Store field description has no result"))?)?,b)?||super::stored_owned::ordinary_byte_value(vt(f,i.result.ok_or_else(||err("ordinary byte field description has no result"))?)?,b)?) && i.result.is_some_and(|id|
                matches!(&f.values[id.0 as usize], Type::Desc(payload)
                    if matches!(payload.as_ref(), Type::ByteBuffer | Type::Record { .. } | Type::OwnedStore { .. }))) => {
            b.spend(2)?;
            out.store_description.sources.push(Source {
static_dependency: StaticProvenance::NotStatic,
                retained_port: None,
                base: HoldRoot::Local(*local),
                leaf: Vec::new(),
                inline: vec![PlaceProjection::Field(*index)],
                heap_parents: Vec::new(),
                heap_element: None,
            });
        }
        InstructionKind::DescribeCapture { index } => {
            if super::stored_owned::ordinary_store_value(module,vt(f,i.result.ok_or_else(||err("ordinary Store capture description has no result"))?)?,b)?||super::stored_owned::ordinary_byte_value(vt(f,i.result.ok_or_else(||err("ordinary byte capture description has no result"))?)?,b)?{
                b.spend(1)?;out.store_description.sources.push(Source{static_dependency:StaticProvenance::NotStatic,retained_port:None,base:HoldRoot::Capture(*index),leaf:vec![],inline:vec![],heap_parents:vec![],heap_element:None});
            }
            b.spend(1)?;
            out.address.sources.push(Source {
static_dependency: StaticProvenance::NotStatic,
                retained_port: None,
                base: HoldRoot::Capture(*index),
                leaf: Vec::new(),
                inline: Vec::new(),
                heap_parents: Vec::new(),
                heap_element: None,
            });
        }
        InstructionKind::TakeCapture { index } if b.returned_environment_sources||super::stored_owned::operation(module,f,i,b)? => {
            out = copy_payload(&state.captures[*index as usize], b)?;
            let pointee = i.result.is_some_and(|result|
                matches!(&f.values[result.0 as usize], Type::Address { inner, .. }
                    if matches!(inner.as_ref(), Type::BorrowedStorage { .. })));
            if pointee {
                // These private rows model the caller's pointee memory, not
                // an environment field that can be read a second time.
                state.captures[*index as usize].address = Permission::default();
            } else { state.captures[*index as usize] = Payload::default(); }
        }
        InstructionKind::WriteCapture { index, value } if b.returned_environment_sources => {
            state.captures[*index as usize] = copy_payload(val(*value)?, b)?;
        }
        InstructionKind::TakeCapture { .. }
        | InstructionKind::MakeManaged { .. }
        | InstructionKind::ReadDesc { .. } => {}
        InstructionKind::WriteDesc { desc, .. }
            if super::stored_owned::description(vt(f, *desc)?, b)? => {}
        InstructionKind::DropTuple { local } if failure_tuple_drop(f, i, b)? => {
            state.locals[local.0 as usize] = Payload::default();
            clear_local_holes(state, *local, b)?;
        }
        InstructionKind::DropManaged { local } | InstructionKind::DropPackage { local } => {
            state.locals[local.0 as usize] = Payload::default();
            clear_local_holes(state, *local, b)?;
        }
        InstructionKind::DescribeAddress { address }
            if b.masked_stored_tasks() || b.replacements || super::stored_owned::ordinary_store_value(module,vt(f,i.result.ok_or_else(||err("ordinary Store address description has no result"))?)?,b)? || super::stored_owned::ordinary_byte_value(vt(f,i.result.ok_or_else(||err("ordinary byte address description has no result"))?)?,b)? || b.shared_byte_borrow && i.result.is_some_and(|result| matches!(f.values[result.0 as usize], Type::Desc(ref payload) if payload.as_ref() == &Type::ByteBuffer)) => {
            join_permission(&mut out.store_description, &val(*address)?.address, b)?;
        }
        InstructionKind::OwnedStoreBorrow { store, .. } | InstructionKind::OwnedStoreBorrowMut { store, .. }
            if super::stored_owned::ordinary_store_value(module,vt(f,*store)?,b)? || b.masked_stored_tasks() && (b.store_descriptions() || matches!(i.kind, InstructionKind::OwnedStoreBorrow { .. })) => {
            let description = &val(*store)?.store_description;
            // The reverse-batch solver may see bottom before a predecessor's
            // description arrives. Its final grounded-value check rejects a
            // window that remains empty at the fixed point.
            for source in &description.sources {
                let target = store_paths::enter(source, b)?;
                add_sources(&mut out.address.sources, &[target], b)?;
            }
            add_loans(&mut out.address.loans, &description.loans, b)?;
        }
        InstructionKind::ByteBufferBorrow { .. } => {
            out.address = owned_byte_views::transfer(module, f, i, values, b)?;
        }
        InstructionKind::ByteBufferNew
        | InstructionKind::ByteBufferLen { .. }
        | InstructionKind::ByteBufferCapacity { .. }
        | InstructionKind::DescribeAddress { .. } => {}
        InstructionKind::ByteBufferPush { .. }
        | InstructionKind::ByteBufferReserve { .. }
        | InstructionKind::ByteBufferTruncate { .. } => {
            // ByteBuffer growth may relocate bytes. This component must not
            // silently preserve a stored pointer into that payload.
            for ty in f.locals.iter().map(|local| &local.ty).chain(&f.values) {
                let Some((storage, shape)) = qualified(ty) else {
                    continue;
                };
                b.spend(shape.slots.len())?;
                for row in &shape.slots {
                    let mut field = storage;
                    for step in &row.path {
                        b.spend(1)?;
                        if let Type::BorrowedStorage { storage, .. } = field {
                            field = storage;
                        }
                        field = match (field, step) {
                            (
                                Type::Record { fields, .. } | Type::Package { fields, .. },
                                PlaceProjection::Field(index),
                            ) => fields
                                .get(*index as usize)
                                .map(|(_, ty)| ty)
                                .ok_or_else(|| err("stored field is absent"))?,
                            (Type::Tuple(fields), PlaceProjection::Field(index)) => fields
                                .get(*index as usize)
                                .ok_or_else(|| err("stored tuple field is absent"))?,
                            (Type::Array { element, length }, PlaceProjection::Field(index))
                                if *index < *length =>
                            {
                                element.as_ref()
                            }
                            (Type::Enum { variants, .. }, PlaceProjection::Variant(selector)) => {
                                variants
                                    .get(selector.variant as usize)
                                    .and_then(|(_, fields)| fields.get(selector.index as usize))
                                    .ok_or_else(|| err("stored variant payload is absent"))?
                            }
                            _ => {
                                return Err(unsupported(
                                    "ByteBuffer relocation needs projected stored byte target invalidation",
                                    Some(i.span),
                                ));
                            }
                        };
                    }
                    if matches!(field, Type::Address { inner, .. }
                        if inner.as_ref() == &Type::U8)
                    {
                        return Err(unsupported(
                            "ByteBuffer relocation needs projected stored byte target invalidation",
                            Some(i.span),
                        ));
                    }
                }
            }
        }
        InstructionKind::TakeLocal { local } => {
            out = copy_payload(&state.locals[local.0 as usize], b)?;
            state.locals[local.0 as usize] = Payload::default();
            clear_local_holes(state, *local, b)?;
            dyn_ports::vacate_owner(module, f, *local, state, b)?;
        }
        InstructionKind::ReadCapture { index } if captured_definition(module, f, b).is_some() => {
            out = copy_payload(
                state
                    .captures
                    .get(*index as usize)
                    .ok_or_else(|| err("capture payload absent"))?,
                b,
            )?;
        }
        InstructionKind::BorrowCapture { index, mutable: true }
            if task_address::capture_instruction(module, f, i, b)? =>
        {
            out = task_address::borrow_capture(*index, state, b)?;
        }
        InstructionKind::BorrowCapture { index, .. }
            if returned_environment::borrowed_child(module, f, i, b)? =>
        {
            // The head loan protects the real inline child environment. Its
            // external leaves retain their independent target permissions.
            out = copy_payload(&state.captures[*index as usize], b)?;
            b.spend(1)?;
            out.address = Permission {
                sources: vec![Source {
static_dependency: StaticProvenance::NotStatic,
                    retained_port: None,
                    base: HoldRoot::Capture(*index),
                    leaf: Vec::new(),
                    inline: Vec::new(),
                    heap_parents: Vec::new(),
                    heap_element: None,
                }],
                loans: Vec::new(),
            };
        }
        InstructionKind::BorrowCapture { .. } if closed_dyn_transfer.is_some() => {
            out = copy_payload(closed_dyn_transfer.as_ref().unwrap(), b)?;
        }
        InstructionKind::BorrowCapture { .. } if dyn_transfer.is_some() => {
            out = copy_payload(dyn_transfer.as_ref().unwrap(), b)?;
        }
        InstructionKind::BorrowCapture { index, mutable } => {
            let definition = captured_definition(module, f, b)
                .ok_or_else(|| err("capture loan has no actual complete entry"))?;
            super::closures::inline_capture_borrow(definition, *index, *mutable, b)?;
            b.spend(1)?;
            out.address.sources.push(Source {
static_dependency: StaticProvenance::NotStatic,
                retained_port: None,
                base: HoldRoot::Capture(*index),
                leaf: Vec::new(),
                inline: Vec::new(),
                heap_parents: Vec::new(),
                heap_element: None,
            });
        }
        _ if call_results::call_result_with_dyn(module,f,i,state,values,summaries,sites,dyn_sites,loans,&mut out,b)? => {}
        kind if super::stored_callables::called(module, f, kind)
            .is_some_and(|(definition, _)| definition.storage_life.as_ref().is_some_and(|life|
                signature_life::outer(life, StorageBase::Result).is_some() || stored_callables::static_result(definition))
                && matches!(definition.result, Type::Address { .. })) =>
        {
            let mut pending=false;
            receiver::receiver_result(module, f, i, state, values, summaries, sites, loans, &mut out, &mut pending,b)?;
            state.pending_external_update|=pending;
        }
        kind if super::stored_callables::called(module, f, kind)
            .is_some_and(|(definition, _)| super::stored_callables::lending(definition)) =>
        {
            let (definition, _) = super::stored_callables::called(module, f, kind)
                .ok_or_else(|| err("lender entry absent"))?;
            let receiver = match kind {
                InstructionKind::CallClosureShared { callee, .. }
                | InstructionKind::CallClosureMut { callee, .. } => {
                    b.spend(1)?;
                    Permission {
                        sources: vec![Source {
static_dependency: StaticProvenance::NotStatic,
                            retained_port: None,
                            base: HoldRoot::Local(*callee),
                            leaf: vec![],
                            inline: vec![],
                            heap_parents: Vec::new(),
                            heap_element: None,
                        }],
                        loans: vec![],
                    }
                }
                InstructionKind::CallClosureBorrowed { callee, .. } => {
                    let mut permission = Permission::default();
                    join_permission(&mut permission, &val(*callee)?.address, b)?;
                    permission
                }
                _ => {
                    return Err(err(
                        "environment result requires a borrowed actual receiver",
                    ));
                }
            };
            for (path, returned) in &summaries[definition.function.0 as usize].result {
                b.spend(path.len().saturating_add(1))?;
                if !path.is_empty() {
                    return Err(err("environment lending result has extra stored leaves"));
                }
                for actual in returned {
                    let HoldRoot::Capture(field) = actual.base else {
                        return Err(err("lender returned a nonenvironment origin"));
                    };
                    if !actual.leaf.is_empty() {
                        return Err(err("lender returned an external capture target"));
                    }
                    for original in &receiver.sources {
                        b.spend(
                            width(original)
                                .saturating_add(actual.inline.len())
                                .saturating_add(2),
                        )?;
                        let mut source = original.clone();
                        source.inline.push(PlaceProjection::Field(field));
                        source.inline.extend_from_slice(&actual.inline);
                        add_sources(&mut out.address.sources, &[source], b)?;
                    }
                    add_loans(&mut out.address.loans, &receiver.loans, b)?;
                }
            }
        }
        InstructionKind::MakeClosure { closure, captures } if b.returned_environment_sources => {
            returned_environment::create(module, *closure, captures, values, state, &mut out, b)?;
            if b.task_sources() { task_address::create_environment(captures,values,&mut out,b)?; }
        }
        InstructionKind::MakeClosure { closure, captures }
            if module
                .closures
                .get(closure.0 as usize)
                .is_some_and(|d| d.storage_life.is_some()) =>
        {
            let d = &module.closures[closure.0 as usize];
            b.spend(captures.len())?;
            for (index, (value, ty)) in captures.iter().zip(&d.captures).enumerate() {
                let actual = val(*value)?;
                let prefix = vec![PlaceProjection::Field(index as u32)];
                if matches!(ty, Type::Address { .. } | Type::Desc(_)) {
                    add_leaf(&mut out, &prefix, &actual.address, b)?;
                }
                if matches!(ty, Type::Desc(_)) {
                    // Preserve the weak manager source in this exact
                    // environment field. The actual callback effect supplies
                    // the access mode when the descriptor is used.
                    value_transport::attach_aux(&mut out, &prefix, ty, actual, b)?;
                }
                if let Some(slots)=compound::slots(ty,b)?{
                    for(path,_)in slots{
                        let row=actual.leaves.position(&path,b)?.ok_or_else(||err("actual inline capture omits its complete physical pointer field"))?;
                        b.spend(prefix.len().saturating_add(path.len()))?;
                        let mut destination=prefix.clone();destination.extend(path);
                        add_leaf(&mut out,&destination,&actual.leaves[row].1,b)?;
                    }
                }
                if let Some((_, shape)) = addressed(ty) {
                    for slot in &shape.slots {
                        let p = select(state, &actual.address, &slot.path, b)?;
                        b.spend(slot.path.len().saturating_add(1))?;
                        let mut path = prefix.clone();
                        path.extend(&slot.path);
                        add_leaf(&mut out, &path, &p, b)?;
                    }
                }
            }
            if b.task_sources() { task_address::create_environment(captures,values,&mut out,b)?; }
        }
        InstructionKind::BorrowLocal { local } | InstructionKind::BorrowLocalMut { local } => {
            b.spend(1)?;
            out.address.sources.push(Source {
static_dependency: StaticProvenance::NotStatic,
                retained_port: None,
                base: HoldRoot::Local(*local),
                leaf: Vec::new(),
                inline: Vec::new(),
                heap_parents: Vec::new(),
                heap_element: None,
            });
            if b.task_sources() {
                task_address::join(&mut out, &state.locals[local.0 as usize], b)?;
            }
            if qualified(&f.locals[local.0 as usize].ty).is_some()
                || shared_tuple_value(module,&f.locals[local.0 as usize].ty,Some(i.span),b)?
                || shared_array_value(&f.locals[local.0 as usize].ty)
                || fixed_environment(module, &f.locals[local.0 as usize].ty, b)
            {
                for (path, p) in &state.locals[local.0 as usize].leaves {
                    add_leaf(&mut out, path, p, b)?;
                }
            }
        }
        InstructionKind::AddressField { address, index } => {
            join_permission(&mut out.address, &val(*address)?.address, b)?;
            for target in &mut out.address.sources {
                b.spend(1)?;
                if let Some(element) = &mut target.heap_element {
                    element.push(PlaceProjection::Field(*index));
                } else {
                    target.inline.push(PlaceProjection::Field(*index));
                }
            }
            if let Some((_, shape)) = addressed(vt(
                f,
                i.result.ok_or_else(|| err("field address lacks result"))?,
            )?) {
                b.spend(shape.slots.len())?;
                for row in &shape.slots {
                    // select pays the existing root clone; reserve this new
                    // appended physical path before constructing it as well.
                    b.spend(amount(
                        out.address.sources.len(),
                        row.path.len().saturating_add(1),
                    )?)?;
                    let permission = select(state, &out.address, &row.path, b)?;
                    add_leaf(&mut out, &row.path, &permission, b)?;
                }
            }
            if let Some(result) = i.result
                && let Type::Address { inner, .. } = vt(f, result)?
                && shared_array_shape(inner, b)?
            {
                let Type::Array { length, .. } = inner.as_ref() else {
                    unreachable!()
                };
                b.spend(*length as usize)?;
                for index in 0..*length {
                    let path = [PlaceProjection::Field(index)];
                    // select pays each existing Source width; reserve the
                    // appended physical ordinal independently before growth.
                    b.spend(out.address.sources.len())?;
                    let actual = select(state, &out.address, &path, b)?;
                    add_leaf(&mut out, &path, &actual, b)?;
                }
            }
        }
        InstructionKind::ReborrowAddress { address, .. }
        | InstructionKind::AddressIndex { address, .. }
        | InstructionKind::AddressSlice { address, .. } => {
            join_permission(&mut out.address, &val(*address)?.address, b)?;
            task_address::join(&mut out, val(*address)?, b)?;
            let carrier_reborrow = matches!(i.kind, InstructionKind::ReborrowAddress { .. })
                && addressed(vt(f, *address)?).is_some();
            if carrier_reborrow {
                b.spend(val(*address)?.leaves.len())?;
            }
            if fixed_environment(module, vt(f, *address)?, b)
                || carrier_reborrow
                || matches!(i.kind, InstructionKind::ReborrowAddress { .. })
                    && matches!(vt(f, *address)?, Type::Address { inner, .. }
                        if shared_array_shape(inner, b)?)
            {
                for (path, p) in &val(*address)?.leaves {
                    add_leaf(&mut out, path, p, b)?;
                }
            }
        }
        InstructionKind::MakeAggregate { fields } | InstructionKind::MakePackage { fields } | InstructionKind::MakeOwnedTuple { fields } => {
            if b.task_failure_carriers
                && !anonymous_tuple::operation(f, i, b)?
                && i.result.is_some_and(|r| matches!(f.values.get(r.0 as usize),Some(Type::BorrowedStorage{storage,..}) if matches!(storage.as_ref(),Type::Tuple(_))))
            {
                return Err(err("anonymous input Tuple must be moved from its actual parent field"));
            }
            let result = vt(
                f,
                i.result
                    .ok_or_else(|| err("array constructor lacks result"))?,
            )?;
            compound::construct(f, i, fields, values, &mut out, b)?;
            if shared_array_shape(result, b)? {
                let Type::Array { length, .. } = result else {
                    unreachable!()
                };
                if fields.len() != *length as usize {
                    return Err(err("shared array constructor omits actual element"));
                }
                b.spend(fields.len().saturating_mul(2))?;
                for (index, field) in fields.iter().enumerate() {
                    add_leaf(
                        &mut out,
                        &[PlaceProjection::Field(index as u32)],
                        &val(*field)?.address,
                        b,
                    )?;
                }
            }
            if let Some((Type::Package { fields: children, .. }, _)) = qualified(result) {
                b.spend(children.len())?;
                for (index, (_, child)) in children.iter().enumerate() {
                    if let Type::Closure(id) = child
                        && borrowed_callback_domain(*id, b)?.is_some()
                    {
                        let value = *fields.get(index).ok_or_else(|| err("Package constructor omits its actual callback operand"))?;
                        if vt(f, value)? != child {
                            return Err(err("Package constructor substitutes another actual callback field type"));
                        }
                        value_transport::attach(&mut out, &[PlaceProjection::Field(index as u32)], child, val(value)?, b)?;
                    }
                }
            }
            if let Some((_, shape)) = qualified(vt(
                f,
                i.result.ok_or_else(|| err("aggregate lacks result"))?,
            )?) {
                for row in &shape.slots {
                    let Some((PlaceProjection::Field(index), tail)) = row.path.split_first() else {
                        return Err(err("aggregate stored leaf has no inline field"));
                    };
                    let value = val(*fields
                        .get(*index as usize)
                        .ok_or_else(|| err("aggregate omits actual borrowed field"))?)?;
                    let permission = if tail.is_empty() {
                        Some(&value.address)
                    } else {
                        leaf(value, tail, b)?
                    };
                    if let Some(permission) = permission {
                        if row.mode == StoredBorrowMode::ExclusiveAddress {
                            let held = retain_written_slot(
                                permission,
                                i,
                                u32::MAX,
                                &row.path,
                                sites,
                                loans,
                                b,
                            )?;
                            add_leaf(&mut out, &row.path, &held, b)?;
                        } else {
                            add_leaf(&mut out, &row.path, permission, b)?;
                        }
                    }
                }
            }
        }
        InstructionKind::LoadStoredBorrow {
            storage,
            path,
            mode: StoredBorrowLoad::CopyShared,
        } => out.address = select(state, &val(*storage)?.address, path, b)?,
        InstructionKind::LoadStoredBorrow {
            storage,
            path,
            mode: StoredBorrowLoad::Reborrow { .. },
        } => {
            out.address = select(state, &val(*storage)?.address, path, b)?;
            let parents = if let Some(local) = projection_only_load(f, i, *storage, b)? {
                state.locals[local.0 as usize].address.loans.clone()
            } else {
                val(*storage)?.address.loans.clone()
            };
            add_loans(&mut out.address.loans, &parents, b)?;
        }
        InstructionKind::LoadStoredBorrow {
            storage,
            path,
            mode: StoredBorrowLoad::Take,
        } => {
            let address = &val(*storage)?.address;
            out.address = select(state, address, path, b)?;
            if !address.sources.is_empty() {
                let position = slot_position(address, path, b)?;
                let holder = memory_holder_mut(state,position.base,b)?;
                if let Some(holder) = holder {
                    b.spend(holder.leaves.len())?;
                    holder
                        .leaves
                        .retain(|(path, _)| path != &position.inline, b)?;
                }
                add_sources(&mut state.vacant, &[position], b)?;
            }
        }
        InstructionKind::StoreStoredBorrow {
            storage,
            path,
            value,
        } => {
            let [root] = val(*storage)?.address.sources.as_slice() else {
                return Err(err("replacement carrier requires one exact actual source"));
            };
            if !root.leaf.is_empty() {
                return Err(err("replacement carrier crosses a target boundary"));
            }
            let actual_path = if root.inline.is_empty() {
                path.clone()
            } else {
                slot_position(&val(*storage)?.address, path, b)?.inline
            };
            let new =
                retain_written_slot(&val(*value)?.address, i, u32::MAX, path, sites, loans, b)?;
            // Reverse CFG batches can first see an unresolved predecessor.
            // Final access validation rejects a truly empty replacement.
            let holder = memory_holder_mut(state,root.base,b)?
            .ok_or_else(|| err("replacement carrier location is absent"))?;
            if let Some(row) = holder.leaves.position(&actual_path, b)? {
                *holder.leaves.permission_mut(row) = new;
            } else {
                b.spend(actual_path.len().saturating_add(1))?;
                holder.leaves.push((actual_path, new), b)?;
            }
            if !state.vacant.is_empty() {
                let position = slot_position(&val(*storage)?.address, path, b)?;
                b.spend(state.vacant.len())?;
                state.vacant.retain(|slot| slot != &position);
            }
        }
        InstructionKind::LoadStoredBorrow { .. } => {
            return Err(unsupported(
                "stored reborrow/take/replacement CFG is not yet implemented",
                Some(i.span),
            ));
        }
        InstructionKind::CallClosureOnce { callee, .. }
            if matches!(vt(f,*callee)?,Type::Closure(id) if module.closures.get(id.0 as usize).is_some_and(super::stored_callables::task_result)) => {
            out=task_address::invoke(module,f,i,values,summaries,state,b)?;
        }
        InstructionKind::ForeignCall { .. } | InstructionKind::ForeignFunctionRef { .. } => {
            foreign_calls::operation(module, f, i, b)?;
        }
        InstructionKind::UnsafeEnter { .. } | InstructionKind::UnsafeExit { .. } => {}
        InstructionKind::Call { function, arguments }
        | InstructionKind::SuspendCall { function, arguments } => {
            let callee = module
                .functions
                .get(function.0 as usize)
                .ok_or_else(|| err("call target absent"))?;
            let before=call_results::Before::function(module,f,i,values,state,b)?;
            dyn_ports::call(
                module,
                f,
                callee,
                i,
                &summaries[function.0 as usize],
                arguments,
                values,
                state,
                &mut out,
                dyn_sites,
                loans,
                b,
            )?;
            task_address::call(
                module,
                f,
                &summaries[function.0 as usize],
                arguments,
                values,
                state,
                &mut out,
                b,
            )?;
            let(enum_variants,enum_known)=summaries[function.0 as usize].returned_enum_variants.substitute_known(arguments,values,b)?;
            out.enum_variants=if enum_known{enum_variants}else{enum_address::Variants::default()};
            let(variants,variants_known)=summaries[function.0 as usize].returned_variants.substitute_known(arguments,values,b)?;
            out.option_variants=if variants_known{variants}else{option_address::Variants::default()};
            let mut result_pending=!variants_known||!enum_known;
            if !summaries[function.0 as usize].field_result_facts.is_empty(){result_pending|=!before.field_results(&summaries[function.0 as usize],&callee.return_type,i,sites,loans,&mut out,b)?;}
            for (path, sources) in &summaries[function.0 as usize].result {
                let descriptor=matches!(call_results::destination(module,&callee.return_type,path,b)?,Type::Desc(_));
                if !call_results::path_active(module,&callee.return_type,&out,path,b)?{continue;}
                let mut p = Permission::default();
                for source in sources {
                    let actual=if descriptor{before.descriptor_source(source,b)?}else{before.source(source,b)?};
                    result_pending |= actual.sources.is_empty() && !before.inactive_result_source(source, b)?;
                    join_permission(&mut p,&actual,b)?;
                    if path.is_empty()
                        && matches!(callee.return_type, Type::Address { mutable: true, .. })
                        && !source.leaf.is_empty()
                        && let HoldRoot::Parameter(index) = source.base
                    {
                        let actual = arguments
                            .get(index as usize)
                            .and_then(|v| values.get(v.0 as usize))
                            .ok_or_else(|| err("stored result carrier parameter absent"))?;
                        add_loans(&mut p.loans, &actual.address.loans, b)?;
                    }
                }
                let shared_result=shared_tuple_value(module,&callee.return_type,Some(i.span),b)?;
                let active=!path.is_empty() && ((out.enum_variants.present() && out.enum_variants.slot_active(path))
                    || (!out.enum_variants.present() && (!option_address::candidate(&callee.return_type) || out.option_variants.mask()&2!=0)));
                b.spend(summaries[function.0 as usize].physical_result_parents.len().saturating_mul(path.len().saturating_add(1)))?;
                if active && (shared_result || summaries[function.0 as usize].physical_result_parents.iter().any(|(known,_)|known==path)){
                    if shared_result{call_results::result_path(&callee.return_type,path,b)?;}
                    let known=before.parents(&summaries[function.0 as usize],path,&mut p,b)?;
                    if !known || shared_result&&p.sources.is_empty(){result_pending=true;}
                }
                if path.is_empty() {
                    out.address = p;
                } else if (out.enum_variants.present() && out.enum_variants.slot_active(path))
                    || (!out.enum_variants.present()
                        && (!option_address::candidate(&callee.return_type)
                            || out.option_variants.mask() & 2 != 0))
                {
                    add_leaf(&mut out, path, &p, b)?;
                }
            }
            if b.raw_reference_cells || b.dyn_contexts || b.closure_receiver_borrows {
                for parent in &summaries[function.0 as usize].dyn_result_parents {
                    let actual = before.source(parent,b)?;
                    if b.closure_receiver_borrows && receiver::helper_head(callee, parent) {
                        b.spend(actual.loans.len())?;
                        for id in &actual.loans {
                            loans.get_mut(*id)
                                .ok_or_else(|| err("actual helper head loan is absent"))?
                                .receiver_head = true;
                        }
                    }
                    add_loans(&mut out.address.loans, &actual.loans, b)?;
                }
            }
            // A complete address result needs actual data, even when the
            // callee summary omits the root row altogether. An empty root
            // is unproved, not a known empty pointer or a parent-only value.
            result_pending |= matches!(callee.return_type, Type::Address { .. })
                && out.address.sources.is_empty();
            drop(before);
            state.pending_external_update|=result_pending;
            let dyn_updates = dyn_ports::prepare_after(
                module,
                f,
                callee,
                i,
                &summaries[function.0 as usize],
                arguments,
                values,
                state,
                dyn_sites,
                loans,
                b,
            )?;
            let updates=after_state::function(module,f,callee,i,&summaries[function.0 as usize],arguments,values,state,sites,loans,b)?;
            after_state::publish(state,updates,b)?;
            dyn_ports::publish_after(state, dyn_updates, b)?;
        }
        _ if failure_wrapper.is_some() => {
            out = failure_wrapper.unwrap();
        }
        _ if dyn_transfer.is_some() => {}
        _ if closed_dyn_transfer.is_some() => {
            out = copy_payload(closed_dyn_transfer.as_ref().unwrap(), b)?;
        }
        _ if enum_transfer.is_some() => {
            out = enum_transfer.unwrap();
        }
        _ if option_transfer.is_some() => {
            out = option_transfer.unwrap();
        }
        kind if pure_copy::enum_local(kind).is_some() && pure_copy::ordinary_owned_enum(module,f,i,b)? => {
            pure_copy::enum_local_transfer(f,i,state,&mut out,b)?;
        }
        InstructionKind::DropErrorUnion{local}if pure_copy::ordinary_owned_union(module,f,i,b)?=>{state.locals[local.0 as usize]=Payload::default();clear_local_holes(state,*local,b)?;},
        _ if pure_copy::ordinary_owned_union(module,f, i, b)? => {}
        _ if finite_fields::operation(module, f, i, b)? => {
            out = finite_fields::transfer(module, f, i, state, b)?;
        }
        InstructionKind::BorrowPackageField { local, index, .. }
            if pure_copy::ordinary_copy_field_borrow(f, i, b)?
                || pure_copy::ordinary_package_borrow(module, f, i, b)? =>
        {
            b.spend(2)?;
            out.address.sources.push(Source {
static_dependency: StaticProvenance::NotStatic,
                retained_port: None,
                base: HoldRoot::Local(*local),
                leaf: Vec::new(),
                inline: vec![PlaceProjection::Field(*index)],
                heap_parents: Vec::new(),
                heap_element: None,
            });
            require_filled(state, &out.address, &[], b)?;
        }
        _ if pure_copy::ordinary_package_read(f, i, b)? => {}
        _ if task_transfer.is_some() => {
            out = task_transfer.clone().unwrap();
        }
        InstructionKind::ExtractIndex { aggregate, .. } if array_index::candidate(f, i) => {
            out.address = array_index::transfer(f, i, val(*aggregate)?, b)?;
        }
        _ if pure_copy::operation(&i.kind) => {
            if !pure_copy::ordinary_owned_union(module,f, i, b)? {
                pure_copy::types(module,f, i, b)?;
            }
            for input in instruction_operands(&i.kind) {
                let payload = val(input)?;
                if !payload.address.sources.is_empty()
                    || !payload.address.loans.is_empty()
                    || !payload.leaves.is_empty()
                {
                    return Err(err(
                        "ordinary pure Copy input retains actual stored source permissions",
                    ));
                }
            }
        }
        _ if super::stored_callables::instruction(module, f, i) => {}
        InstructionKind::FunctionRef { .. } | InstructionKind::CallIndirect { .. }
            if super::class_flow::pointer_instruction(f, i) => {}
        InstructionKind::ReadAddress { address } if reference_cells::nested(vt(f,*address)?)=>{
            let result=i.result.ok_or_else(||err("pointer-cell read has no actual result"))?;
            if !matches!(vt(f,result)?,Type::Address{mutable:false,..}){return Err(err("pointer-cell read cannot copy an exclusive pointer"));}
            out.address=reference_cells::read(state,&val(*address)?.address,b)?;
            if out.address.sources.is_empty(){state.pending_external_update=true;}
        }
        InstructionKind::WriteAddress{address,value}if reference_cells::nested(vt(f,*address)?)=>{
            let Type::Address{mutable:true,inner}=vt(f,*address)?else{return Err(err("pointer-cell replacement lacks exclusive slot permission"));};
            if !matches!(inner.as_ref(),Type::Address{mutable:false,..})||inner.as_ref()!=vt(f,*value)?{return Err(err("pointer-cell replacement changes its complete shared pointer type"));}
            reference_cells::write(module,f,i,state,&val(*address)?.address,&val(*value)?.address,inner,dyn_sites,loans,b)?;
        }
        InstructionKind::ReadAddress{address}=>{
            let Type::Address{inner,..}=vt(f,*address)?else{return Err(err("read transfer lacks its actual Address"));};
            if !copyable_memory_value(module,inner,b)?{return Err(err("indirect read would duplicate owning or exclusive value state"));}
            let(snapshot,known)=value_transport::read(state,&val(*address)?.address,inner,module,b)?;
            out=snapshot;state.pending_external_update|=!known;
            if val(*address)?.address.sources.is_empty(){state.pending_external_update=true;}
            if !value_transport::required_channels_known(module,inner,&out,b)?{state.pending_external_update=true;}
        }
        InstructionKind::WriteAddress{address,value}=>{
            let Type::Address{mutable:true,inner}=vt(f,*address)?else{return Err(err("indirect write lacks exclusive pointer permission"));};
            if inner.as_ref()!=vt(f,*value)?||!copyable_memory_value(module,inner,b)?{return Err(err("indirect write changes its complete copyable memory value type"));}
            value_transport::write(state,&val(*address)?.address,inner,val(*value)?,module,b)?;
        }
        InstructionKind::Constant(_)
        | InstructionKind::OwnedStoreNew { .. }
        | InstructionKind::OwnedStoreReserve { .. }
        | InstructionKind::OwnedStorePush { .. }
        | InstructionKind::OwnedStorePop { .. }
        | InstructionKind::OwnedStoreCapacity { .. }
        | InstructionKind::OwnedStoreLen { .. }
        | InstructionKind::DropErrorUnion { .. }
        | InstructionKind::DescribePackageField { .. }
        | InstructionKind::ExtractField { .. }
        | InstructionKind::ConvertInteger { .. }
        | InstructionKind::ConvertNumber { .. }
        | InstructionKind::Unary { .. }
        | InstructionKind::Binary { .. }
        | InstructionKind::SliceLength { .. }
        | InstructionKind::ScopeEnter { .. }
        | InstructionKind::ScopeExit { .. } => {}
        _ if ordinary_enum_operation(module, f, i, b)? => {}
        _ => {
            return Err(unsupported(
                "instruction lacks an independent stored target transfer",
                Some(i.span),
            ));
        }
    }
    if let Some(tags) = task_tags {
        out.enum_variants = tags;
    }
    for (local, tags) in unpacked_tags {
        state.locals[local.0 as usize].enum_variants = tags;
    }
    if let Some(permission) = owned_byte_views::seed_description(module, f, i, values, b)? {
        out.store_description = permission;
    }
    after_state::publish(state, capture_updates, b)?;
    value_transport::publish_after(state, value_updates, b)?;
    dyn_captures::publish_after(state, dyn_capture_updates, b)?;
    reference_cells::publish(state, reference_updates, b)?;
    if b.qualified_head_returns
        && let Some(id) = i.result
        && let Some(current) = qualified_heads::rows(vt(f, id)?, &out, state, b)?
    {
        out.leaves.copy_work(b)?;
        out.leaves = current;
    }
    if let Some(id) = i.result
        && b.store_descriptions()
        && matches!(
            i.kind,
            InstructionKind::Call { .. } | InstructionKind::SuspendCall { .. }
        )
        && option_address::shape(vt(f, id)?, &b.limits.clone(), b)?
    {
        // A returned Some is a new shared window at this actual call site,
        // rather than the caller's possibly exclusive manager loan itself.
        let row = &mut loans[id.0 as usize];
        for ordinal in 0..out.leaves.len() {
            b.spend(1)?;
            let permission = out.leaves.permission_mut(ordinal);
            add_sources(&mut row.sources, &permission.sources, b)?;
            add_loans(&mut row.parents, &permission.loans, b)?;
            b.spend(1)?;
            permission.loans = vec![id.0 as usize];
        }
        row.exclusive = false;
    }
    if let Some(dynamic) = &dyn_transfer {
        join_permission(&mut out.address, &dynamic.address, b)?;
        dyn_ports::join(&mut out, dynamic, b)?;
    }
    if let Some(id) = i.result
        && b.static_utf8
        && matches!(vt(f, id)?, Type::Address { mutable: true, .. })
    {
        b.spend(out.address.sources.len())?;
        if out
            .address
            .sources
            .iter()
            .any(|source| source.base == HoldRoot::Static)
        {
            return Err(err("static text became a mutable address"));
        }
    }
    let capture_reborrow = receiver::read(module, f, i, state, &mut out, b)?;
    if let Some(id) = i.result
        && (capture_reborrow
            || matches!(
                i.kind,
                InstructionKind::BorrowLocal { .. }
                    | InstructionKind::BorrowLocalMut { .. }
                    | InstructionKind::BorrowOwnedArrayElement { .. }
                    | InstructionKind::ReborrowAddress { .. }
                    | InstructionKind::LoadStoredBorrow {
                        mode: StoredBorrowLoad::Reborrow { .. },
                        ..
                    }
                    | InstructionKind::AddressField { .. }
                    | InstructionKind::AddressIndex { .. }
                    | InstructionKind::AddressSlice { .. }
                    | InstructionKind::Call { .. }
                    | InstructionKind::SuspendCall { .. }
                    | InstructionKind::BorrowCapture { .. }
                    | InstructionKind::CallClosureBorrowed { .. }
                    | InstructionKind::CallClosureShared { .. }
                    | InstructionKind::CallClosureMut { .. }
                    | InstructionKind::BorrowDynOwned { .. }
                    | InstructionKind::MakeDynBorrow { .. }
                    | InstructionKind::CallDynSlot { .. }
            )
            || match &i.kind {
                InstructionKind::OwnedStoreBorrow { store, .. }
                | InstructionKind::OwnedStoreBorrowMut { store, .. } => {
                    b.store_descriptions()
                        || super::stored_owned::ordinary_store_value(module, vt(f, *store)?, b)?
                }
                _ => false,
            }
            || ordinary_error_borrow(module, f, i, b)?
            || pure_copy::ordinary_copy_field_borrow(f, i, b)?
            || finite_fields::operation(module, f, i, b)?
            || owned_byte_views::instruction(module, f, i, b)?)
        && matches!(vt(f, id)?, Type::Address { .. })
        && !deferred_projection
    {
        let row = &mut loans[id.0 as usize];
        add_sources(&mut row.sources, &out.address.sources, b)?;
        add_loans(&mut row.parents, &out.address.loans, b)?;
        row.exclusive = matches!(vt(f, id)?, Type::Address { mutable: true, .. });
        b.stored_exclusive_loans |= row.exclusive;
        b.spend(1)?;
        out.address.loans = vec![id.0 as usize];
    }
    if let Some(id) = i.result
        && matches!(i.kind, InstructionKind::Load(_))
        && matches!(vt(f, id)?, Type::Address { mutable: true, .. })
        && !deferred_projection
    {
        let row = &mut loans[id.0 as usize];
        add_sources(&mut row.sources, &out.address.sources, b)?;
        add_loans(&mut row.parents, &out.address.loans, b)?;
        row.exclusive = true;
        b.stored_exclusive_loans = true;
        b.spend(1)?;
        out.address.loans = vec![id.0 as usize];
    }
    if let Some(id) = i.result
        && matches!(
            i.kind,
            InstructionKind::Call { .. } | InstructionKind::SuspendCall { .. }
        )
        && qualified(vt(f, id)?).is_some()
    {
        let mut loan_id = f.values.len();
        b.spend(id.0 as usize)?;
        for ty in &f.values[..id.0 as usize] {
            if let Some((_, shape)) = qualified(ty) {
                loan_id = loan_id
                    .checked_add(shape.slots.len())
                    .ok_or_else(|| limit("stored result loan count overflow"))?;
            }
        }
        let (_, shape) = qualified(vt(f, id)?).ok_or_else(|| err("stored result has no shape"))?;
        for declaration in &shape.slots {
            if let Some(ordinal) = out.leaves.position(&declaration.path, b)? {
                let p = out.leaves.permission_mut(ordinal);
                let row = loans
                    .get_mut(loan_id)
                    .ok_or_else(|| err("stored result loan outside actual arena"))?;
                add_sources(&mut row.sources, &p.sources, b)?;
                add_loans(&mut row.parents, &p.loans, b)?;
                row.exclusive = declaration.mode == StoredBorrowMode::ExclusiveAddress;
                b.stored_exclusive_loans |= row.exclusive;
                b.spend(1)?;
                p.loans = vec![loan_id];
            }
            loan_id = loan_id
                .checked_add(1)
                .ok_or_else(|| limit("stored result loan identity overflow"))?;
        }
    }
    if b.returned_environment_sources
        && let Some(value) = i.result
        && matches!(
            i.kind,
            InstructionKind::MakeClosure { .. }
                | InstructionKind::Call { .. }
                | InstructionKind::SuspendCall { .. }
        )
    {
        returned_environment::retain(module, f, value, &mut out, loans, b)?;
    }
    Ok(out)
}

/// A reachable DAG needs no iterative frame propagation. The graph comes from
/// this verifier's structural checks, including checks of unreachable blocks.
fn acyclic_order(g: &FunctionGraph, b: &mut Budget) -> Check<Option<Vec<usize>>> {
    let count = g.reachable.len();
    b.spend(amount(count, 3)?)?;
    let mut incoming = vec![0usize; count];
    let mut ready = Vec::with_capacity(count);
    let mut order = Vec::with_capacity(count);
    let mut reachable = 0usize;
    b.spend(count)?;
    for (block, live) in g.reachable.iter().enumerate() {
        if !live {
            continue;
        }
        reachable += 1;
        b.spend(g.successors[block].len())?;
        for child in &g.successors[block] {
            if g.reachable[*child] {
                incoming[*child] = incoming[*child]
                    .checked_add(1)
                    .ok_or_else(|| limit("stored DAG predecessor count overflow"))?;
            }
        }
    }
    b.spend(count)?;
    for (block, degree) in incoming.iter().enumerate() {
        if g.reachable[block] && *degree == 0 {
            ready.push(block);
        }
    }
    while let Some(block) = ready.pop() {
        b.spend(1)?;
        order.push(block);
        b.spend(g.successors[block].len())?;
        for child in &g.successors[block] {
            if g.reachable[*child] {
                incoming[*child] -= 1;
                if incoming[*child] == 0 {
                    ready.push(*child);
                }
            }
        }
    }
    Ok((order.len() == reachable).then_some(order))
}

fn solve(
    module: &TypedModule,
    f: &TypedFunction,
    g: &FunctionGraph,
    summaries: &[Summary],
    b: &mut Budget,
) -> Check<Solution> {
    solve_mode(module, f, g, summaries, b, true)
}
fn solve_mode(
    module: &TypedModule,
    f: &TypedFunction,
    g: &FunctionGraph,
    summaries: &[Summary],
    b: &mut Budget,
    use_dag: bool,
) -> Check<Solution> {
    replacements::prepare(module, f, g, b)?;
    let projection_heads = deferred_addresses(module, f, b)?;
    // Each reverse-batch solve creates a new function-local loan arena.
    // Its paid seed/site/step writes record every possible exclusive loan,
    // including constructor sites that are not currently active.
    b.stored_exclusive_loans = false;
    b.spend(
        f.values
            .len()
            .saturating_mul(5)
            .saturating_add(f.blocks.len()),
    )?;
    let mut count = f.values.len();
    b.spend(f.values.len())?;
    for ty in &f.values {
        if let Some((_, shape)) = qualified(ty) {
            count = count
                .checked_add(shape.slots.len())
                .ok_or_else(|| limit("stored loan arena overflow"))?;
        }
    }
    if b.returned_environment_sources {
        count = count
            .checked_add(returned_environment::loan_count(module, f, b)?)
            .ok_or_else(|| limit("environment loan arena overflow"))?;
    }
    b.spend(amount(count, 4)?)?;
    let mut loans = vec![Loan::default(); count];
    let entry = seed(module, f, &mut loans, b)?;
    let sites = write_sites(module, f, &mut loans, b)?;
    let mut dyn_sites = dyn_leases::Sites::default();
    let mut values = vec![Payload::default(); f.values.len()];
    let mut outputs = Vec::new();
    for _ in &f.blocks {
        outputs.push(empty_frame(module, f, b)?);
    }
    // The structural verifier checked SSA dominance before this kernel. For
    // every independently proved DAG, every
    // predecessor frame and dominating SSA definition is final before its user;
    // join_payload still records all sources and loan growth, including values
    // whose creation does not change a local frame.
    if use_dag && let Some(order) = acyclic_order(g, b)? {
        if let Some(progress) = &mut b.diagnostic_progress {
            progress.cfg_round = 1;
        }
        for bi in order {
            if let Some(progress) = &mut b.diagnostic_progress {
                progress.block_visits = progress.block_visits.saturating_add(1);
            }
            b.spend(1)?;
            let mut state = if bi == f.entry.0 as usize {
                copy_frame(&entry, b)?
            } else {
                empty_frame(module, f, b)?
            };
            b.spend(g.predecessors[bi].len())?;
            for pred in &g.predecessors[bi] {
                if g.reachable[*pred] {
                    join_frame(&mut state, &outputs[*pred], b)?;
                }
            }
            for instruction in &f.blocks[bi].instructions {
                let out = step(
                    module,
                    f,
                    instruction,
                    &mut state,
                    &values,
                    summaries,
                    &sites,
                    &mut dyn_sites,
                    &mut loans,
                    is_deferred_address(instruction, &projection_heads),
                    b,
                )?;
                if let Some(value) = instruction.result {
                    join_payload(&mut values[value.0 as usize], &out, b)?;
                }
            }
            outputs[bi] = state;
        }
        return Ok(Solution {
            values,
            outputs,
            entry,
            loans,
            sites,
            dyn_sites,
        });
    }
    loop {
        if let Some(progress) = &mut b.diagnostic_progress {
            progress.cfg_round = progress.cfg_round.saturating_add(1);
        }
        let mut changed = false;
        // Reverse batches differ from the candidate's forward in-place walk.
        // Old predecessor frames stay read-only for the complete reverse
        // batch. Every reachable block is reconstructed below; retaining a
        // second deep copy of its old frame adds no information.
        b.spend(outputs.len())?;
        let mut replacement: Vec<Option<Frame>> = (0..outputs.len()).map(|_| None).collect();
        for bi in (0..f.blocks.len()).rev() {
            b.spend(1)?;
            if !g.reachable[bi] {
                continue;
            }
            if let Some(progress) = &mut b.diagnostic_progress {
                progress.block_visits = progress.block_visits.saturating_add(1);
            }
            let mut state = if bi == f.entry.0 as usize {
                copy_frame(&entry, b)?
            } else {
                empty_frame(module, f, b)?
            };
            for pred in &g.predecessors[bi] {
                if g.reachable[*pred] {
                    join_frame(&mut state, &outputs[*pred], b)?;
                }
            }
            for i in &f.blocks[bi].instructions {
                let out = step(
                    module,
                    f,
                    i,
                    &mut state,
                    &values,
                    summaries,
                    &sites,
                    &mut dyn_sites,
                    &mut loans,
                    is_deferred_address(i, &projection_heads),
                    b,
                )?;
                if let Some(v) = i.result {
                    changed |= join_payload(&mut values[v.0 as usize], &out, b)?;
                }
            }
            frame_work(&state, b)?;
            frame_work(&outputs[bi], b)?;
            if state != outputs[bi] {
                changed = true;
            }
            replacement[bi] = Some(state);
        }
        // Unreachable states are moved unchanged. The computed states move
        // into the same actual coordinates, including the stable final round.
        b.spend(outputs.len())?;
        outputs = outputs
            .into_iter()
            .zip(replacement)
            .map(|(old, new)| new.unwrap_or(old))
            .collect();
        if !changed {
            break;
        }
    }
    Ok(Solution {
        values,
        outputs,
        entry,
        loans,
        sites,
        dyn_sites,
    })
}

fn reverse(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    live: &mut WordRow,
    b: &mut Budget,
) -> Check<()> {
    b.spend(1)?;
    if let Some(v) = i.result {
        live.assign(v.0 as usize, false)?;
    }
    match i.kind {
        InstructionKind::Store { local, .. }
        | InstructionKind::StorageLive(local)
        | InstructionKind::BindPackageCapture { local, .. }
        | InstructionKind::EndPackageCapture { local, .. }
        | InstructionKind::StorageDead(local) => {
            live.assign(f.values.len() + local.0 as usize, false)?
        }
        InstructionKind::Load(local)
        | InstructionKind::BorrowPlaceLocal { local, .. }
        | InstructionKind::BorrowLocal { local }
        | InstructionKind::BorrowLocalMut { local } => {
            live.assign(f.values.len() + local.0 as usize, true)?
        }
        InstructionKind::BorrowPackageField { local, .. }
            if pure_copy::ordinary_copy_field_borrow(f, i, b)?
                || pure_copy::managed_package_field_borrow(f, i, b)?
                || finite_fields::operation(module, f, i, b)? =>
        {
            live.assign(f.values.len() + local.0 as usize, true)?;
        }
        InstructionKind::TakeLocal { local }
        | InstructionKind::TakePackageField { local, .. }
        | InstructionKind::DropPackageField { local, .. }
        | InstructionKind::StorePackageField { local, .. }
            if qualified(&f.locals[local.0 as usize].ty).is_some()
                || matches!(i.kind, InstructionKind::TakeLocal { .. })
                    && matches!(f.locals[local.0 as usize].ty, Type::Address { .. }) =>
        {
            live.assign(f.values.len() + local.0 as usize, true)?;
        }
        _ if callable_cells::operation(module, f, i, b)? => {
            if let Some((local, _, _)) = callable_cells::local(&i.kind) {
                live.assign(f.values.len() + local.0 as usize, true)?;
            }
        }
        InstructionKind::VariantTagLocal { local, .. } | InstructionKind::ReadVariantLocal { local, .. }
            | InstructionKind::TakeVariantField { local, .. } | InstructionKind::UnpackVariant { local, .. }
            | InstructionKind::DropEnum { local } if pure_copy::ordinary_owned_enum(module,f,i,b)? => {
            if let InstructionKind::UnpackVariant { fields, .. } = &i.kind {
                b.spend(fields.len())?;
                for target in fields {live.assign(f.values.len() + target.0 as usize, false)?;}
            }
            live.assign(f.values.len() + local.0 as usize, true)?;
        }
        InstructionKind::UnpackTuple { local, ref fields }
            if pure_copy::ordinary_owned_tuple(module, f, i, b)? =>
        {
            for target in fields {
                b.spend(1)?;
                live.assign(f.values.len() + target.0 as usize, false)?;
            }
            live.assign(f.values.len() + local.0 as usize, true)?;
        }
        InstructionKind::TakeTupleField { local, .. } | InstructionKind::DropTuple { local }
            if pure_copy::ordinary_owned_tuple(module, f, i, b)? =>
        {
            live.assign(f.values.len() + local.0 as usize, true)?;
        }
        InstructionKind::CallClosureShared { callee, .. }
        | InstructionKind::CallClosureMut { callee, .. }
            if fixed_environment(module, &f.locals[callee.0 as usize].ty, b) =>
        {
            live.assign(f.values.len() + callee.0 as usize, true)?;
        }
        _ => {}
    }
    replacements::pin(f, i, live, b)?;
    for v in instruction_operands(&i.kind) {
        b.spend(1)?;
        live.assign(v.0 as usize, true)?;
    }
    Ok(())
}
#[cfg(test)]
fn reverse_bool(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    live: &mut [bool],
    b: &mut Budget,
) -> Check<()> {
    b.spend(1)?;
    if let Some(v) = i.result {
        live[v.0 as usize] = false;
    }
    match i.kind {
        InstructionKind::Store { local, .. }
        | InstructionKind::StorageLive(local)
        | InstructionKind::BindPackageCapture { local, .. }
        | InstructionKind::EndPackageCapture { local, .. }
        | InstructionKind::StorageDead(local) => live[f.values.len() + local.0 as usize] = false,
        InstructionKind::Load(local)
        | InstructionKind::BorrowPlaceLocal { local, .. }
        | InstructionKind::BorrowLocal { local }
        | InstructionKind::BorrowLocalMut { local } => {
            live[f.values.len() + local.0 as usize] = true
        }
        InstructionKind::BorrowPackageField { local, .. }
            if pure_copy::ordinary_copy_field_borrow(f, i, b)?
                || pure_copy::managed_package_field_borrow(f, i, b)?
                || finite_fields::operation(module, f, i, b)? =>
        {
            live[f.values.len() + local.0 as usize] = true;
        }
        InstructionKind::TakeLocal { local }
        | InstructionKind::TakePackageField { local, .. }
        | InstructionKind::DropPackageField { local, .. }
        | InstructionKind::StorePackageField { local, .. }
            if qualified(&f.locals[local.0 as usize].ty).is_some()
                || matches!(i.kind, InstructionKind::TakeLocal { .. })
                    && matches!(f.locals[local.0 as usize].ty, Type::Address { .. }) =>
        {
            live[f.values.len() + local.0 as usize] = true;
        }
        _ if callable_cells::operation(module, f, i, b)? => {
            if let Some((local, _, _)) = callable_cells::local(&i.kind) {
                live[f.values.len() + local.0 as usize] = true;
            }
        }
        InstructionKind::VariantTagLocal { local, .. } | InstructionKind::ReadVariantLocal { local, .. }
            | InstructionKind::TakeVariantField { local, .. } | InstructionKind::UnpackVariant { local, .. }
            | InstructionKind::DropEnum { local } if pure_copy::ordinary_owned_enum(module,f,i,b)? => {
            if let InstructionKind::UnpackVariant { fields, .. } = &i.kind {
                b.spend(fields.len())?;
                for target in fields {live[f.values.len() + target.0 as usize] = false;}
            }
            live[f.values.len() + local.0 as usize] = true;
        }
        InstructionKind::UnpackTuple { local, ref fields }
            if pure_copy::ordinary_owned_tuple(module, f, i, b)? =>
        {
            for target in fields {
                b.spend(1)?;
                live[f.values.len() + target.0 as usize] = false;
            }
            live[f.values.len() + local.0 as usize] = true;
        }
        InstructionKind::TakeTupleField { local, .. } | InstructionKind::DropTuple { local }
            if pure_copy::ordinary_owned_tuple(module, f, i, b)? =>
        {
            live[f.values.len() + local.0 as usize] = true;
        }
        InstructionKind::CallClosureShared { callee, .. }
        | InstructionKind::CallClosureMut { callee, .. }
            if fixed_environment(module, &f.locals[callee.0 as usize].ty, b) =>
        {
            live[f.values.len() + callee.0 as usize] = true;
        }
        _ => {}
    }
    replacements::pin_bool(f, i, live, b)?;
    for v in instruction_operands(&i.kind) {
        b.spend(1)?;
        live[v.0 as usize] = true;
    }
    Ok(())
}
fn term_live(t: &Terminator, live: &mut WordRow) -> Check<()> {
    match t {
        Terminator::Return(Some(v)) | Terminator::Branch { condition: v, .. } => {
            live.assign(v.0 as usize, true)?
        }
        _ => {}
    }
    Ok(())
}
#[cfg(test)]
fn term_live_bool(t: &Terminator, live: &mut [bool]) {
    match t {
        Terminator::Return(Some(v)) | Terminator::Branch { condition: v, .. } => {
            live[v.0 as usize] = true
        }
        _ => {}
    }
}
fn live_out(
    module: &TypedModule,
    f: &TypedFunction,
    g: &FunctionGraph,
    b: &mut Budget,
) -> Check<Vec<WordRow>> {
    let width = f
        .values
        .len()
        .checked_add(f.locals.len())
        .ok_or_else(|| limit("stored liveness width overflow"))?;
    let units = WordRow::work_for(width);
    b.spend(amount(amount(f.blocks.len(), units)?, 2)?)?;
    let mut inputs = vec![WordRow::empty(width); f.blocks.len()];
    let mut outputs = inputs.clone();
    let mut dirty: Vec<_> = (0..f.blocks.len()).filter(|i| g.reachable[*i]).collect();
    b.spend(f.blocks.len())?;
    while let Some(bi) = dirty.pop() {
        b.spend(units)?;
        let mut out = WordRow::empty(width);
        for child in &g.successors[bi] {
            out.union(&inputs[*child], b)?;
        }
        b.spend(units)?;
        outputs[bi].clone_from(&out);
        term_live(&f.blocks[bi].terminator, &mut out)?;
        for i in f.blocks[bi].instructions.iter().rev() {
            reverse(module, f, i, &mut out, b)?;
        }
        b.spend(units)?;
        if inputs[bi] != out {
            inputs[bi] = out;
            for pred in &g.predecessors[bi] {
                b.spend(dirty.len().saturating_add(1))?;
                if !dirty.contains(pred) {
                    dirty.push(*pred);
                }
            }
        }
    }
    Ok(outputs)
}
#[cfg(test)]
fn live_out_bool(
    module: &TypedModule,
    f: &TypedFunction,
    g: &FunctionGraph,
    b: &mut Budget,
) -> Check<Vec<Vec<bool>>> {
    let width = f
        .values
        .len()
        .checked_add(f.locals.len())
        .ok_or_else(|| limit("stored liveness width overflow"))?;
    b.spend(amount(amount(f.blocks.len(), width)?, 2)?)?;
    let mut inputs = vec![vec![false; width]; f.blocks.len()];
    let mut outputs = inputs.clone();
    let mut dirty: Vec<_> = (0..f.blocks.len()).filter(|i| g.reachable[*i]).collect();
    b.spend(f.blocks.len())?;
    while let Some(bi) = dirty.pop() {
        b.spend(width)?;
        let mut out = vec![false; width];
        for child in &g.successors[bi] {
            b.spend(width)?;
            for (a, z) in out.iter_mut().zip(&inputs[*child]) {
                *a |= *z;
            }
        }
        b.spend(width)?;
        outputs[bi].clone_from(&out);
        term_live_bool(&f.blocks[bi].terminator, &mut out);
        for i in f.blocks[bi].instructions.iter().rev() {
            reverse_bool(module, f, i, &mut out, b)?;
        }
        b.spend(width)?;
        if inputs[bi] != out {
            inputs[bi] = out;
            for pred in &g.predecessors[bi] {
                b.spend(dirty.len().saturating_add(1))?;
                if !dirty.contains(pred) {
                    dirty.push(*pred);
                }
            }
        }
    }
    Ok(outputs)
}

fn instruction_rows(
    module: &TypedModule,
    f: &TypedFunction,
    block: usize,
    output: &WordRow,
    b: &mut Budget,
) -> Check<Vec<WordRow>> {
    let block = &f.blocks[block];
    // Pay the output copy, every snapshot's word copy and row record,
    // and the empty placeholder records even if the logical width is zero.
    b.spend(amount(
        block.instructions.len().saturating_add(1),
        output.work(),
    )?)?;
    b.spend(block.instructions.len())?;
    let mut live = output.clone();
    term_live(&block.terminator, &mut live)?;
    let mut rows = vec![WordRow::empty(0); block.instructions.len()];
    for (position, instruction) in block.instructions.iter().enumerate().rev() {
        rows[position] = live.clone();
        reverse(module, f, instruction, &mut live, b)?;
    }
    Ok(rows)
}
fn current_head_rows(
    function: Option<&TypedFunction>,
    id: usize,
    payload: &Payload,
    frame: &Frame,
    values: usize,
    b: &mut Budget,
) -> Check<Option<leaf_rows::Rows>> {
    if !b.qualified_head_returns {
        return Ok(None);
    }
    let f = function.ok_or_else(|| err("qualified head liveness lacks its actual function"))?;
    let ty = if id < values {
        f.values.get(id)
    } else {
        f.locals.get(id - values).map(|local| &local.ty)
    }
    .ok_or_else(|| err("qualified head liveness type is absent"))?;
    qualified_heads::rows(ty, payload, frame, b)
}
fn live_loans(
    function: Option<&TypedFunction>,
    solution: &Solution,
    state: &Frame,
    live: &WordRow,
    definition: Option<ValueId>,
    indexed: Option<&[usize]>,
    b: &mut Budget,
) -> Check<Vec<usize>> {
    b.spend(indexed.map_or(live.work(), |ids| {
        ids.len().saturating_add(live.len() - solution.values.len())
    }))?;
    let mut loans = live_loan_ids::SortedLoanIds::default();
    for prepared in &state.prepared {
        loans.add(&prepared.permission.loans, b)?;
    }
    let mut collect = |id: usize, b: &mut Budget| -> Check<()> {
        if !live.is_live(id)? || definition.is_some_and(|value| value.0 as usize == id) {
            return Ok(());
        }
        let payload = if id < solution.values.len() {
            &solution.values[id]
        } else {
            &state.locals[id - solution.values.len()]
        };
        value_transport::visit_fields(payload, b, |permission, b| {
            loans.add(&permission.loans, b)?;
            Ok(())
        })?;
        loans.add(&payload.address.loans, b)?;
        if b.raw_reference_cells {
            loans.add(&reference_cells::held(state, &payload.address, b)?, b)?;
        }
        if b.store_descriptions() || !payload.store_description.loans.is_empty() {
            loans.add(&payload.store_description.loans, b)?;
        }
        if b.task_sources() {
            for (_, p) in &payload.task_ports {
                loans.add(&p.loans, b)?;
            }
        }
        if b.dyn_contexts {
            for (_, p) in &payload.dyn_ports.0 {
                loans.add(&p.permission.loans, b)?;
            }
        }
        let current = current_head_rows(function, id, payload, state, solution.values.len(), b)?;
        for (_, p) in current.as_ref().unwrap_or(&payload.leaves) {
            loans.add(&p.loans, b)?;
        }
        Ok(())
    };
    if let Some(ids) = indexed {
        for &id in ids {
            collect(id, b)?;
        }
        for id in solution.values.len()..live.len() {
            collect(id, b)?;
        }
    } else {
        for id in live.ones() {
            b.spend(1)?;
            collect(id, b)?;
        }
    }
    for row in &state.reference_cells.0 {
        if matches!(
            row.location.base,
            HoldRoot::Parameter(_) | HoldRoot::Capture(_)
        ) {
            loans.add(&row.permission.loans, b)?;
        }
    }
    for payload in state.pointees.iter().map(|(_,payload)|payload)
        .chain(state.projected_pointees.iter().map(|row|&row.payload)) {
        value_transport::visit_permissions(payload, b, |permission, b| {
            loans.add(&permission.loans, b)
        })?;
    }
    if b.task_sources() {
        // The actual frame retains its inputs until its owning carrier moves
        // or is discharged; dead SSA uses of a head do not discharge a frame.
        b.spend(state.locals.len())?;
        for payload in &state.locals {
            for (_, permission) in &payload.task_ports {
                loans.add(&permission.loans, b)?;
            }
        }
    }
    if b.dyn_contexts {
        b.spend(
            state
                .locals
                .len()
                .saturating_add(state.external.len())
                .saturating_add(state.captures.len()),
        )?;
        for payload in state
            .locals
            .iter()
            .chain(&state.external)
            .chain(&state.captures)
        {
            for (_, p) in &payload.dyn_ports.0 {
                loans.add(&p.permission.loans, b)?;
            }
        }
    }
    // Only an actual slot write can create a new external loan that is not
    // already represented by the original parameter relation.
    if b.stored_slot_updates {
        b.spend(state.external.len())?;
        for holder in &state.external {
            for (_, p) in &holder.leaves {
                loans.add(&p.loans, b)?;
            }
        }
    }
    ancestry(loans.as_slice(), &solution.loans, b)
}
#[cfg(test)]
fn live_loans_main_fullscan(
    function: Option<&TypedFunction>,
    solution: &Solution,
    state: &Frame,
    live: &[bool],
    b: &mut Budget,
) -> Check<Vec<usize>> {
    b.spend(live.len())?;
    let mut loans = Vec::new();
    for prepared in &state.prepared {
        add_loans_by_scan(&mut loans, &prepared.permission.loans, b)?;
    }
    for (id, yes) in live.iter().enumerate() {
        if !*yes {
            continue;
        }
        let payload = if id < solution.values.len() {
            &solution.values[id]
        } else {
            &state.locals[id - solution.values.len()]
        };
        value_transport::visit_fields(payload, b, |permission, b| {
            add_loans_by_scan(&mut loans, &permission.loans, b)?;
            Ok(())
        })?;
        add_loans_by_scan(&mut loans, &payload.address.loans, b)?;
        if b.raw_reference_cells {
            add_loans_by_scan(
                &mut loans,
                &reference_cells::held(state, &payload.address, b)?,
                b,
            )?;
        }
        if b.store_descriptions() || !payload.store_description.loans.is_empty() {
            add_loans_by_scan(&mut loans, &payload.store_description.loans, b)?;
        }
        if b.task_sources() {
            for (_, p) in &payload.task_ports {
                add_loans_by_scan(&mut loans, &p.loans, b)?;
            }
        }
        if b.dyn_contexts {
            for (_, p) in &payload.dyn_ports.0 {
                add_loans_by_scan(&mut loans, &p.permission.loans, b)?;
            }
        }
        let current = current_head_rows(function, id, payload, state, solution.values.len(), b)?;
        for (_, p) in current.as_ref().unwrap_or(&payload.leaves) {
            add_loans_by_scan(&mut loans, &p.loans, b)?;
        }
    }
    for row in &state.reference_cells.0 {
        if matches!(
            row.location.base,
            HoldRoot::Parameter(_) | HoldRoot::Capture(_)
        ) {
            add_loans_by_scan(&mut loans, &row.permission.loans, b)?;
        }
    }
    for payload in state.pointees.iter().map(|(_,payload)|payload)
        .chain(state.projected_pointees.iter().map(|row|&row.payload)) {
        value_transport::visit_permissions(payload, b, |permission, b| {
            add_loans_by_scan(&mut loans, &permission.loans, b)?;
            Ok(())
        })?;
    }
    if b.task_sources() {
        // The actual frame retains its inputs until its owning carrier moves
        // or is discharged; dead SSA uses of a head do not discharge a frame.
        for payload in &state.locals {
            for (_, permission) in &payload.task_ports {
                add_loans_by_scan(&mut loans, &permission.loans, b)?;
            }
        }
    }
    if b.dyn_contexts {
        for payload in state
            .locals
            .iter()
            .chain(&state.external)
            .chain(&state.captures)
        {
            for (_, p) in &payload.dyn_ports.0 {
                add_loans_by_scan(&mut loans, &p.permission.loans, b)?;
            }
        }
    }
    // Only an actual slot write can create a new external loan that is not
    // already represented by the original parameter relation.
    if b.stored_slot_updates {
        for holder in &state.external {
            for (_, p) in &holder.leaves {
                add_loans_by_scan(&mut loans, &p.loans, b)?;
            }
        }
    }
    ancestry_by_linear_set(&loans, &solution.loans, b)
}
#[allow(clippy::too_many_arguments)]
fn check_call(
    module: &TypedModule,
    f: &TypedFunction,
    function: FunctionId,
    arguments: &[ValueId],
    solution: &Solution,
    state: &Frame,
    checks: &mut access_candidates::Access<'_>,
    summaries: &[Summary],
    requirements: &mut Vec<(Source, Source)>,
    byte_flow: Option<&byte_queries::Flow>,
    byte_point: Option<&byte_queries::Point>,
    head_accesses: &mut Vec<(Source, bool)>,
    description_accesses: &mut Vec<descriptor_access::DescriptionAccess>,
    b: &mut Budget,
) -> Check<()> {
    let callee = module
        .functions
        .get(function.0 as usize)
        .ok_or_else(|| err("call target absent"))?;
    if arguments.len() != callee.parameters.len() {
        return Err(err("call arity differs from actual body signature"));
    }
    {
        for (source, write) in &summaries[function.0 as usize].head_accesses {
            let actual = instantiate(module, f, source, arguments, &solution.values, state, b)?;
            if actual.sources.is_empty() {
                let mut inactive = false;
                if source.retained_port.is_none()
                    && let HoldRoot::Parameter(index) = source.base
                    && let Some(value) = arguments.get(index as usize)
                    && let Some(ty) = f.values.get(value.0 as usize)
                    && enum_address::signature(ty)
                {
                    let tags = &solution.values[value.0 as usize].enum_variants;
                    for (path, _) in enum_address::slots(ty, b)? {
                        b.spend(path.len().saturating_add(source.leaf.len()).saturating_add(1))?;
                        if path == source.leaf {
                            inactive = tags.grounded(ty, b)? && !tags.slot_active(&path);
                            break;
                        }
                    }
                }
                if inactive { continue; }
                return Err(err("callee head access has no actual argument target"));
            }
            if *write {
                store_paths::writable(&actual, &solution.loans, b)?;
            }
            checks.check(&actual, *write, requirements, b)?;
            store_paths::record(head_accesses, &actual, *write, b)?;
        }
    }
    if !summaries[function.0 as usize]
        .description_accesses
        .is_empty()
    {
        let before =
            call_results::Before::direct(module, f, callee, arguments, &solution.values, state, b)?;
        for row in &summaries[function.0 as usize].description_accesses {
            let actual = descriptor_access::resolve(&before, row, b)?;
            if actual.sources.is_empty() {
                return Err(err(
                    "descriptor helper access lacks every actual manager source",
                ));
            }
            if row.write {
                store_paths::writable(&actual, &solution.loans, b)?;
            }
            checks.check(&actual, row.write, requirements, b)?;
            descriptor_access::record(
                module,
                f,
                head_accesses,
                description_accesses,
                &actual,
                row.write,
                b,
            )?;
        }
    }
    if b.dyn_contexts {
        // A helper's actual body effects retain the same caller obligations
        // as an inlined operation. The result-source upper cannot erase an
        // external data access or delegate an unrelated live child loan.
        for (source, write) in &summaries[function.0 as usize].dyn_accesses {
            let actual = instantiate(module, f, source, arguments, &solution.values, state, b)?;
            if *write {
                store_paths::writable(&actual, &solution.loans, b)?;
            }
            checks.check(&actual, *write, requirements, b)?;
        }
    }
    for (position, argument) in arguments.iter().enumerate() {
        let value = &solution.values[argument.0 as usize];
        let ty = &callee.locals[callee.parameters[position].0 as usize].ty;
        if let Some((_, shape)) = addressed(ty) {
            for slot in &shape.slots {
                require_filled(state, &value.address, &slot.path, b)?;
            }
        }
        if let Type::Address { mutable, .. } = ty {
            checks.check(&value.address, *mutable, requirements, b)?;
            if b.stored_slot_updates
                && *mutable
                && let Some((_, shape)) = addressed(ty)
            {
                for slot in &shape.slots {
                    let held = select(state, &value.address, &slot.path, b)?;
                    checks.check(
                        &held,
                        slot.mode == StoredBorrowMode::ExclusiveAddress,
                        requirements,
                        b,
                    )?;
                }
            }
        }
        for (_, p) in &value.leaves {
            checks.check(p, false, requirements, b)?;
        }
        for earlier in &arguments[..position] {
            let right = &solution.values[earlier.0 as usize];
            let left_mut = matches!(vt(f, *argument)?, Type::Address { mutable: true, .. });
            let right_mut = matches!(vt(f, *earlier)?, Type::Address { mutable: true, .. });
            if left_mut || right_mut {
                for a in &value.address.sources {
                    for z in &right.address.sources {
                        apart(a, z, requirements, b)?;
                    }
                }
            }
            if left_mut {
                for (_, p) in &right.leaves {
                    for a in &value.address.sources {
                        for z in &p.sources {
                            apart(a, z, requirements, b)?;
                        }
                    }
                }
            }
            if right_mut {
                for (_, p) in &value.leaves {
                    for a in &p.sources {
                        for z in &right.address.sources {
                            apart(a, z, requirements, b)?;
                        }
                    }
                }
            }
        }
    }
    for (a, z) in &summaries[function.0 as usize].apart {
        let weak =
            |source: &Source| match source.base {
                HoldRoot::Parameter(position) => callee
                    .parameters
                    .get(position as usize)
                    .is_some_and(|local| {
                        byte_queries::is_description(&callee.locals[local.0 as usize].ty)
                    }),
                _ => false,
            };
        if b.byte_queries && (weak(a) || weak(z)) {
            let flow =
                byte_flow.ok_or_else(|| err("Byte separation has no covered origin flow"))?;
            let left = flow.instantiate(
                module,
                f,
                a,
                callee,
                arguments,
                &solution.values,
                state,
                byte_point.ok_or_else(|| err("Byte separation has no current proof"))?,
                b,
            )?;
            let right = flow.instantiate(
                module,
                f,
                z,
                callee,
                arguments,
                &solution.values,
                state,
                byte_point.ok_or_else(|| err("Byte separation has no current proof"))?,
                b,
            )?;
            for a in &left {
                for z in &right {
                    for x in &a.sources {
                        for y in &z.sources {
                            apart(x, y, requirements, b)?;
                        }
                    }
                }
            }
            continue;
        }
        let before =
            call_results::Before::direct(module, f, callee, arguments, &solution.values, state, b)?;
        let Some((a, z)) = before.separation(a, z, b)? else {
            continue;
        };
        for a in &a.sources {
            for z in &z.sources {
                apart(a, z, requirements, b)?;
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn windows(
    module: &TypedModule,
    f: &TypedFunction,
    g: &FunctionGraph,
    solution: &mut Solution,
    summaries: &[Summary],
    requirements: &mut Vec<(Source, Source)>,
    byte_reads: &mut Vec<Source>,
    head_accesses: &mut Vec<(Source, bool)>,
    description_accesses: &mut Vec<descriptor_access::DescriptionAccess>,
    dyn_accesses: &mut Vec<(Source, bool)>,
    b: &mut Budget,
) -> Check<()> {
    let projection_heads = deferred_addresses(module, f, b)?;
    // A module marker only schedules this actual per-function inventory.
    let mut ordinary_error_places = false;
    if b.ordinary_error_places {
        b.spend(f.blocks.len())?;
        for block in &f.blocks {
            b.spend(block.instructions.len())?;
            for instruction in &block.instructions {
                ordinary_error_places |= ordinary_error_borrow(module, f, instruction, b)?;
            }
        }
    }
    let byte_flow = byte_queries::Flow::solve(
        module,
        f,
        g,
        &solution.entry,
        &solution.values,
        summaries,
        b,
    )?;
    let owned_byte_sites = owned_byte_views::Sites::collect(f, b)?;
    let output_live = live_out(module, f, g, b)?;
    let ssa_groups = if b.qualified_head_returns {
        None
    } else {
        ssa_loan_groups::Sources::collect(&solution.values, b)?
    };
    for (bi, block) in f.blocks.iter().enumerate() {
        b.spend(1)?;
        if !g.reachable[bi] {
            continue;
        }
        let mut state = if bi == f.entry.0 as usize {
            copy_frame(&solution.entry, b)?
        } else {
            empty_frame(module, f, b)?
        };
        for pred in &g.predecessors[bi] {
            if g.reachable[*pred] {
                join_frame(&mut state, &solution.outputs[*pred], b)?;
            }
        }
        replacements::entry(f, bi, &state, b)?;
        let after = instruction_rows(module, f, bi, &output_live[bi], b)?;
        let mut byte_point = byte_flow
            .as_ref()
            .map(|flow| flow.input(f, bi, g, b))
            .transpose()?;
        for (at, i) in block.instructions.iter().enumerate() {
            b.spend(after[at].work())?;
            let mut live = after[at].clone();
            if let Some(id) = i.result {
                live.assign(id.0 as usize, false)?;
            }
            b.spend(1)?;
            let selected = ssa_groups
                .as_ref()
                .map(|groups| groups.select(&live, None, b))
                .transpose()?
                .flatten();
            let mut active = live_loans(
                Some(f),
                solution,
                &state,
                &live,
                None,
                selected.as_ref().map(|ids| ids.as_slice()),
                b,
            )?;
            let pointer = |v: ValueId| -> Check<&Permission> {
                solution
                    .values
                    .get(v.0 as usize)
                    .map(|v| &v.address)
                    .ok_or_else(|| err("address operand absent"))
            };
            let at_instruction = |mut error: Diagnostic| {
                if error.code == "E-BORROW-CONFLICT" {
                    let span = (i.span.start < i.span.end).then_some(i.span);
                    error.span = span;
                    error.relate(
                        zil_base::DiagnosticRole::ConflictingAccess,
                        span,
                        Some(zil_base::DiagnosticIrLocation {
                            function: f.id.0,
                            block: Some(block.id.0),
                            instruction: u32::try_from(at).ok(),
                        }),
                        "This access conflicts with the live borrow.",
                    );
                }
                error
            };
            if let Some(flow) = &byte_flow {
                for permission in flow.reads(
                    module,
                    f,
                    i,
                    &solution.values,
                    &state,
                    byte_point.as_ref().unwrap(),
                    summaries,
                    b,
                )? {
                    query_access(&permission, &active, &solution.loans, requirements, b)
                        .map_err(at_instruction)?;
                    byte_queries::record(byte_reads, &permission, b)?;
                }
            }
            // Query effects precede publication of this call's new result.
            // Preserve the original result exclusion for ordinary accesses.
            if let Some(id) = i.result {
                b.spend(active.len())?;
                active.retain(|x| *x != id.0 as usize);
            }
            if let Some((permission, write)) = owned_byte_sites.check(
                module,
                f,
                i,
                &state,
                &solution.values,
                &active,
                &solution.loans,
                requirements,
                b,
            ).map_err(at_instruction)? {
                if write { store_paths::writable(&permission, &solution.loans, b)?; }
                descriptor_access::record(module, f, head_accesses, description_accesses,
                    &permission, write, b)?;
            }
            (|| -> Check<()> {
                let mut checks = access_candidates::Access::new(&active, &solution.loans, b)?;
                foreign_calls::accesses(
                    module,
                    f,
                    i,
                    &solution.values,
                    &mut checks,
                    requirements,
                    b,
                )?;
                if replacements::heads(b) {
                    let head = dynamic_store::access(&i.kind)
                         .or_else(|| {
                            if byte_flow.is_some() && byte_queries::query(&i.kind).is_some() {
                                None
                            } else {
                                byte_buffers::access(&i.kind)
                            }
                        })
                        .or(match i.kind {
                            InstructionKind::ReadDesc { desc, .. }
                                if !super::stored_owned::description(vt(f, desc)?, b)? => Some((desc, false)),
                            InstructionKind::WriteDesc { desc, .. }
                                if !super::stored_owned::description(vt(f, desc)?, b)? => Some((desc, true)),
                            _ => None,
                        });
                    if let Some((value, write)) = head {
                        let actual = &solution.values[value.0 as usize];
                        let permission = if actual.store_description.sources.is_empty() {
                            &actual.address
                        } else {
                            &actual.store_description
                        };
                        if write {
                            store_paths::writable(permission, &solution.loans, b)?;
                        }
                        checks.check(permission, write, requirements, b)?;
                        descriptor_access::record(module,f,head_accesses,description_accesses,permission,write,b)?;
                    }
                }
                dyn_ports::access(
                    module,
                    f,
                    i,
                    &state,
                    &solution.values,
                    &mut checks,
                    requirements,
                    b,
                )?;
                dyn_closed::access(
                    module,
                    f,
                    i,
                    &state,
                    &solution.values,
                    &mut checks,
                    requirements,
                    b,
                )?;
                replacements::check(
                    f,
                    i,
                    &state,
                    &solution.values,
                    &active,
                    &solution.loans,
                    requirements,
                    head_accesses,
                    b,
                )?;
                match &i.kind {
                    InstructionKind::MakeAggregate { fields }
                    | InstructionKind::MakePackage { fields }
                        if b.stored_exclusive_loans =>
                    {
                        let result = i.result.ok_or_else(|| err("aggregate lacks result"))?;
                        if let Some((_, shape)) = qualified(vt(f, result)?) {
                            for (index, slot) in shape.slots.iter().enumerate() {
                                b.spend(slot.path.len().saturating_add(1))?;
                                let Some((PlaceProjection::Field(field), tail)) =
                                    slot.path.split_first()
                                else {
                                    return Err(err("constructor slot has no input field"));
                                };
                                let source = &solution.values[fields[*field as usize].0 as usize];
                                let permission = if tail.is_empty() {
                                    &source.address
                                } else {
                                    leaf(source, tail, b)?.ok_or_else(|| {
                                        err("constructor has no actual input leaf")
                                    })?
                                };
                                let exclusive = slot.mode == StoredBorrowMode::ExclusiveAddress;
                                checks.check(permission, exclusive, requirements, b)?;
                                for previous in &shape.slots[..index] {
                                    b.spend(1)?;
                                    if exclusive
                                        || previous.mode == StoredBorrowMode::ExclusiveAddress
                                    {
                                        let other = leaf(
                                            &solution.values[result.0 as usize],
                                            &previous.path,
                                            b,
                                        )?
                                        .ok_or_else(|| {
                                            err("constructor has no prior stored leaf")
                                        })?;
                                        for left in &permission.sources {
                                            for right in &other.sources {
                                                apart(left, right, requirements, b)?;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    InstructionKind::BorrowPlaceLocal { local, path, mutable }
                        if ordinary_error_borrow(module,f, i, b)? || finite_fields::operation(module, f, i, b)? =>
                    {
                        b.spend(path.len().saturating_add(1))?;
                        let actual = Permission {
                            sources: vec![Source {
                                static_dependency: StaticProvenance::NotStatic,
                                retained_port: None,
                                base: HoldRoot::Local(*local),
                                leaf: Vec::new(),
                                inline: path.clone(),
                                heap_parents: Vec::new(),
                                heap_element: None,
                            }],
                            loans: Vec::new(),
                        };
                        checks.check(&actual, *mutable, requirements, b)?;
                    }
                    InstructionKind::VariantTagLocal { local, .. } | InstructionKind::ReadVariantLocal { local, .. }
                        | InstructionKind::TakeVariantField { local, .. } | InstructionKind::UnpackVariant { local, .. }
                        | InstructionKind::DropEnum { local } if pure_copy::ordinary_owned_enum(module,f,i,b)? => {
                        let (path,write) = pure_copy::enum_place_access(&i.kind,b)?;
                        let mut check_local = |local, inline, write, b: &mut Budget| {
                            b.spend(1)?;
                            let actual = Permission { sources: vec![Source {
                                base: HoldRoot::Local(local), inline, leaf: Vec::new(),
                                static_dependency: StaticProvenance::NotStatic, retained_port: None,
                                heap_element: None, heap_parents: Vec::new(),
                            }], loans: Vec::new() };
                            checks.check(&actual,write,requirements,b)
                        };
                        check_local(*local,path,write,b)?;
                        if let InstructionKind::UnpackVariant { fields, .. } = &i.kind {
                            for target in fields { check_local(*target,Vec::new(),true,b)?; }
                        }
                    }
                    InstructionKind::TakeVariantField { local, path } if ordinary_error_places => {
                        b.spend(path.len().saturating_add(1))?;
                        let actual = Permission {
                            sources: vec![Source {
                                static_dependency: StaticProvenance::NotStatic,
                                retained_port: None,
                                base: HoldRoot::Local(*local),
                                leaf: Vec::new(),
                                inline: path.clone(),
                                heap_parents: Vec::new(),
                                heap_element: None,
                            }],
                            loans: Vec::new(),
                        };
                        checks.check(&actual, true, requirements, b)?;
                    }
                    _ if callable_cells::operation(module,f,i,b)? => {
                        if let Some((local,index,write))=callable_cells::local(&i.kind) {
                            b.spend(1)?;
                            let actual=Permission {sources:vec![Source {base:HoldRoot::Local(local),leaf:vec![],inline:index.map(|i|vec![PlaceProjection::Field(i)]).unwrap_or_default(),heap_parents:vec![],heap_element:None,retained_port:None,static_dependency:StaticProvenance::NotStatic}],loans:vec![]};
                            checks.check(&actual,write,requirements,b)?;
                        }
                    }
                    InstructionKind::UnpackTuple{local,..}|InstructionKind::DropTuple{local}|InstructionKind::TakeTupleField{local,..}
                        if pure_copy::ordinary_owned_tuple(module,f,i,b)?=>{
                        let path=match &i.kind{InstructionKind::TakeTupleField{path,..}=>{b.spend(path.len())?;path.clone()},_=>Vec::new()};
                        b.spend(1)?;
                        let actual=Permission{sources:vec![Source{static_dependency:StaticProvenance::NotStatic,retained_port:None,base:HoldRoot::Local(*local),leaf:Vec::new(),inline:path,heap_parents:Vec::new(),heap_element:None}],loans:Vec::new()};
                        checks.check(&actual,true,requirements,b)?;
                    }
                    InstructionKind::DropErrorUnion { local } if ordinary_error_places => {
                        b.spend(1)?;
                        let actual = Permission {
                            sources: vec![Source {
                                static_dependency: StaticProvenance::NotStatic,
                                retained_port: None,
                                base: HoldRoot::Local(*local),
                                leaf: Vec::new(),
                                inline: Vec::new(),
                                heap_parents: Vec::new(),
                                heap_element: None,
                            }],
                            loans: Vec::new(),
                        };
                        checks.check(&actual, true, requirements, b)?;
                    }
                    InstructionKind::WriteDesc{..}=>{
                        let(permission,_)=description_access(f,i,&solution.values,b)?.ok_or_else(||err("description write lacks its exact finite scalar access"))?;
                        store_paths::writable(&permission,&solution.loans,b)?;
                        checks.check(&permission,true,requirements,b)?;
                        descriptor_access::record(module,f,head_accesses,description_accesses,&permission,true,b)?;
                    }
                    InstructionKind::ReadDesc { desc, .. } => {
                        if let Some((permission,_))=description_access(f,i,&solution.values,b)?{
                            checks.check(&permission,false,requirements,b)?;
                            descriptor_access::record(module,f,head_accesses,description_accesses,&permission,false,b)?;
                        }else{
                            let actual=pointer(*desc)?;
                            if !actual.sources.is_empty(){checks.check(actual,false,requirements,b)?;}
                        }
                    }
                    InstructionKind::BorrowPackageField {
                        local,
                        index,
                        mutable: false,
                    } if pure_copy::managed_package_field_borrow(f, i, b)? => {
                        b.spend(2)?;
                        let actual = Permission {
                            sources: vec![Source {
                                static_dependency: StaticProvenance::NotStatic,
                                retained_port: None,
                                base: HoldRoot::Local(*local),
                                leaf: Vec::new(),
                                inline: vec![PlaceProjection::Field(*index)],
                                heap_parents: Vec::new(),
                                heap_element: None,
                            }],
                            loans: Vec::new(),
                        };
                        checks.check(&actual, false, requirements, b)?;
                    }
                    InstructionKind::DescribePackageField { local, index }
                        if pure_copy::qualified_package_description(f, i, b)? =>
                    {
                        b.spend(2)?;
                        let actual = Permission {
                            sources: vec![Source {
                                static_dependency: StaticProvenance::NotStatic,
                                retained_port: None,
                                base: HoldRoot::Local(*local),
                                leaf: Vec::new(),
                                inline: vec![PlaceProjection::Field(*index)],
                                heap_parents: Vec::new(),
                                heap_element: None,
                            }],
                            loans: Vec::new(),
                        };
                        checks.check(&actual, false, requirements, b)?;
                    }
                    InstructionKind::BorrowPackageField {
                        local,
                        index,
                        mutable,
                    } if pure_copy::ordinary_copy_field_borrow(f, i, b)?
                        || finite_fields::operation(module, f, i, b)?
                        || pure_copy::ordinary_package_borrow(module, f, i, b)? => {
                        b.spend(2)?;
                        let actual = Permission {
                            sources: vec![Source {
                                static_dependency: StaticProvenance::NotStatic,
                                retained_port: None,
                                base: HoldRoot::Local(*local),
                                leaf: Vec::new(),
                                inline: vec![PlaceProjection::Field(*index)],
                                heap_parents: Vec::new(),
                                heap_element: None,
                            }],
                            loans: Vec::new(),
                        };
                        checks.check(&actual, *mutable, requirements, b)?;
                    }
                    InstructionKind::ReadPackageField { local, index }
                        if pure_copy::ordinary_package_read(f, i, b)? =>
                    {
                        b.spend(2)?;
                        let actual = Permission {
                            sources: vec![Source {
                                static_dependency: StaticProvenance::NotStatic,
                                retained_port: None,
                                base: HoldRoot::Local(*local),
                                leaf: Vec::new(),
                                inline: vec![PlaceProjection::Field(*index)],
                                heap_parents: Vec::new(),
                                heap_element: None,
                            }],
                            loans: Vec::new(),
                        };
                        checks.check(&actual, false, requirements, b)?;
                    }
                    InstructionKind::TakePackageField { local, index }
                    | InstructionKind::DropPackageField { local, index }
                    | InstructionKind::StorePackageField { local, index, .. } => {
                        if matches!(i.kind, InstructionKind::TakePackageField { .. }) {
                            b.spend(state.vacant.len())?;
                            if state.vacant.iter().any(|slot| {
                                slot.base == HoldRoot::Local(*local)
                                    && slot.inline.first() == Some(&PlaceProjection::Field(*index))
                            }) {
                                return Err(err("child Package move has a vacant stored slot"));
                            }
                        }
                        let actual = Permission {
                            sources: vec![Source {
                                static_dependency: StaticProvenance::NotStatic,
                                retained_port: None,
                                base: HoldRoot::Local(*local),
                                leaf: Vec::new(),
                                inline: vec![PlaceProjection::Field(*index)],
                                heap_parents: Vec::new(),
                                heap_element: None,
                            }],
                            loans: Vec::new(),
                        };
                        b.spend(1)?;
                        checks.check(&actual, true, requirements, b)?;
                    }
                    InstructionKind::Load(local) => {
                        b.spend(state.vacant.len())?;
                        if state
                            .vacant
                            .iter()
                            .any(|slot| slot.base == HoldRoot::Local(*local))
                        {
                            return Err(err("whole carrier copy has a vacant stored slot"));
                        }
                        if matches!(
                            f.locals[local.0 as usize].ty,
                            Type::Address { mutable: true, .. }
                        ) && !is_deferred_address(i, &projection_heads) {
                            checks.check(
                                &state.locals[local.0 as usize].address,
                                true,
                                requirements,
                                b,
                            )?;
                        }
                        if matches!(f.locals[local.0 as usize].ty, Type::Address { .. }) {
                            // Loading the pointer local reads its storage,
                            // not the pointee. A loan of the pointer storage
                            // itself still prevents even this head observation.
                            b.spend(1)?;
                            let head = Permission {
                                sources: vec![Source {
                                    static_dependency: StaticProvenance::NotStatic,
                                    retained_port: None,
                                    base: HoldRoot::Local(*local),
                                    leaf: vec![], inline: vec![], heap_parents: vec![], heap_element: None,
                                }],
                                loans: vec![],
                            };
                            checks.check(&head, false, requirements, b)?;
                        }
                    }
                    InstructionKind::BorrowCapture { index, mutable } => {
                        b.spend(1)?;
                        let actual = Permission {
                            sources: vec![Source {
                                static_dependency: StaticProvenance::NotStatic,
                                retained_port: None,
                                base: HoldRoot::Capture(*index),
                                leaf: vec![],
                                inline: vec![],
                                heap_parents: Vec::new(),
                                heap_element: None,
                            }],
                            loans: vec![],
                        };
                        checks.check(&actual, *mutable, requirements, b)?;
                    }
                    InstructionKind::Store { local, .. }
                    | InstructionKind::StorageDead(local)
                    | InstructionKind::EndPackageCapture { local, .. }
                    | InstructionKind::TakeLocal { local }
                    | InstructionKind::DropClosure { local }
                    | InstructionKind::DropManaged { local }
                    | InstructionKind::DropPackage { local }
                    | InstructionKind::BorrowLocal { local }
                    | InstructionKind::BorrowLocalMut { local } => {
                        if matches!(i.kind, InstructionKind::TakeLocal { .. }) {
                            b.spend(state.vacant.len())?;
                            if state
                                .vacant
                                .iter()
                                .any(|slot| slot.base == HoldRoot::Local(*local))
                            {
                                return Err(err("whole carrier move has a vacant stored slot"));
                            }
                        }
                        b.spend(1)?;
                        let source = Source {
                            static_dependency: StaticProvenance::NotStatic,
                            retained_port: None,
                            base: HoldRoot::Local(*local),
                            leaf: Vec::new(),
                            inline: Vec::new(),
                            heap_parents: Vec::new(),
                            heap_element: None,
                        };
                        let actual = Permission {
                            sources: vec![source],
                            loans: Vec::new(),
                        };
                        let write = !matches!(i.kind, InstructionKind::BorrowLocal { .. });
                        checks.check(&actual, write, requirements, b)?;
                    }
                    InstructionKind::ExtractIndex { aggregate, .. }
                        if array_index::candidate(f, i) =>
                    {
                        let input = solution
                            .values
                            .get(aggregate.0 as usize)
                            .ok_or_else(|| err("dynamic Array operand outside actual values"))?;
                        array_index::check(f, i, input, &active, &solution.loans, requirements, b)?;
                    }
                    InstructionKind::ReadAddress { address } => {
                        if let Type::Address { inner, .. } = vt(f, *address)?
                            && shared_array_value(inner)
                            && let Type::Array { length, .. } = inner.as_ref()
                        {
                            array_snapshot::check(&state, pointer(*address)?, *length, b)?;
                        }
                        checks.check(pointer(*address)?, false, requirements, b)?;
                    }
                    InstructionKind::SliceLength { address }
                    | InstructionKind::ReborrowAddress {
                        address,
                        mutable: false,
                    } => checks.check(pointer(*address)?, false, requirements, b)?,
                    InstructionKind::AddressField { address, .. } => {
                        let result = i
                            .result
                            .ok_or_else(|| err("field view lacks its actual result"))?;
                        let projected = pointer(result)?;
                        let original = pointer(*address)?;
                        permission_work(projected, b)?;
                        b.spend(original.loans.len())?;
                        // Creating a field view acts on its real physical
                        // window. It retains the input ancestry, never its
                        // own newly-created child as delegation authority.
                        let actual = Permission {
                            sources: projected.sources.clone(),
                            loans: original.loans.clone(),
                        };
                        let write = matches!(vt(f, result)?, Type::Address { mutable: true, .. });
                        if is_deferred_address(i,&projection_heads){access_head(&actual,write,b)?;}
                        else{checks.check(&actual, write, requirements, b)?;}
                    }
                    InstructionKind::AddressIndex { address, .. }
                    | InstructionKind::AddressSlice { address, .. } => {
                        let write = i.result.is_some_and(|v| {
                            matches!(f.values[v.0 as usize], Type::Address { mutable: true, .. })
                        });
                        if is_deferred_address(i,&projection_heads){
                            let result=i.result.ok_or_else(||err("reserved projection has no actual result"))?;
                            access_head(pointer(result)?,write,b)?;
                        }else{checks.check(pointer(*address)?, write, requirements, b)?;}
                    }
                    InstructionKind::WriteAddress { address, .. }
                    | InstructionKind::ReborrowAddress {
                        address,
                        mutable: true,
                    } => checks.check(pointer(*address)?, true, requirements, b)?,
                    InstructionKind::UnpackTuple { local, .. } if failure_unpack(f, i, b)? => {
                        let Type::BorrowedStorage { life, .. } = &f.locals[local.0 as usize].ty
                        else {
                            unreachable!()
                        };
                        let source = state
                            .locals
                            .get(local.0 as usize)
                            .ok_or_else(|| err("input tuple has no live source"))?;
                        b.spend(source.leaves.len().saturating_add(life.slots.len()))?;
                        if source.leaves.len() != life.slots.len() {
                            return Err(err("input tuple permission cardinality differs"));
                        }
                        for slot in &life.slots {
                            let permission = leaf(source, &slot.path, b)?
                                .ok_or_else(|| err("input tuple permission is absent"))?;
                            if permission.sources.is_empty() {
                                return Err(err("input tuple has no actual source"));
                            }
                            checks.check(
                                permission,
                                slot.mode == StoredBorrowMode::ExclusiveAddress,
                                requirements,
                                b,
                            )?;
                        }
                    }
                    InstructionKind::LoadStoredBorrow { storage, .. }
                        if failure_projection(f, i, b)?.is_some() =>
                    {
                        let Type::Address { inner, .. } = vt(f, *storage)? else {
                            unreachable!()
                        };
                        let Type::BorrowedStorage { life, .. } = inner.as_ref() else {
                            unreachable!()
                        };
                        checks.check(pointer(*storage)?, true, requirements, b)?;
                        for row in &life.slots {
                            require_filled(&state, pointer(*storage)?, &row.path, b)?;
                            let permission = select(&state, pointer(*storage)?, &row.path, b)?;
                            if permission.sources.is_empty() {
                                return Err(err("failure projection has no actual source"));
                            }
                            checks.check(
                                &permission,
                                row.mode == StoredBorrowMode::ExclusiveAddress,
                                requirements,
                                b,
                            )?;
                        }
                    }
                    InstructionKind::LoadStoredBorrow {
                        storage,
                        path,
                        mode,
                    } => {
                        require_filled(&state, pointer(*storage)?, path, b)?;
                        let exclusive = matches!(
                            mode,
                            StoredBorrowLoad::Reborrow { mutable: true } | StoredBorrowLoad::Take
                        );
                        // The established single shared slot has the same whole
                        // carrier access as before mutable slot replacement was
                        // introduced. Only a writer or multiple/exclusive slots
                        // need the finer inline-window proof below.
                        if !b.stored_slot_updates
                            && matches!(mode, StoredBorrowLoad::CopyShared)
                            && addressed(vt(f, *storage)?).is_some_and(|(_, shape)| {
                                shape.slots.len() == 1
                                    && shape.slots[0].mode == StoredBorrowMode::SharedAddress
                            })
                        {
                            checks.check(pointer(*storage)?, false, requirements, b)?;
                            let p = select(&state, pointer(*storage)?, path, b)?;
                            checks.check(&p, false, requirements, b)?;
                        } else {
                            // Compare the actual inline slot, retaining the carrier
                            // loans. Whole-carrier access would conflate sibling slots.
                            let base = pointer(*storage)?;
                            b.spend(
                                base.sources
                                    .len()
                                    .saturating_mul(path.len().saturating_add(1)),
                            )?;
                            let slot = Permission {
                                sources: base
                                    .sources
                                    .iter()
                                    .map(|source| {
                                        let mut source = source.clone();
                                        source.inline.extend(path);
                                        source
                                    })
                                    .collect(),
                                loans: base.loans.clone(),
                            };
                            checks.check(&slot, exclusive, requirements, b)?;
                            let p = select(&state, pointer(*storage)?, path, b)?;
                            let target_write = if matches!(mode, StoredBorrowLoad::Take) {
                                i.result.is_some_and(|id| {
                                    matches!(
                                        f.values[id.0 as usize],
                                        Type::Address { mutable: true, .. }
                                    )
                                })
                            } else {
                                exclusive
                            };
                            checks.check(&p, target_write, requirements, b)?;
                        }
                    }
                    InstructionKind::StoreStoredBorrow {
                        storage,
                        path,
                        value,
                    } => {
                        let exclusive =
                            matches!(vt(f, *value)?, Type::Address { mutable: true, .. });
                        let base = pointer(*storage)?;
                        b.spend(
                            base.sources
                                .len()
                                .saturating_mul(path.len().saturating_add(1)),
                        )?;
                        let slot = Permission {
                            sources: base
                                .sources
                                .iter()
                                .map(|source| {
                                    let mut source = source.clone();
                                    source.inline.extend(path);
                                    source
                                })
                                .collect(),
                            loans: base.loans.clone(),
                        };
                        checks.check(&slot, true, requirements, b)?;
                        let previous = select(&state, base, path, b)?;
                        if previous.sources.is_empty() {
                            let position = slot_position(base, path, b)?;
                            if !state.vacant.contains(&position) {
                                return Err(err(
                                    "replacement has no live slot and no extracted hole",
                                ));
                            }
                        } else {
                            checks.check(&previous, exclusive, requirements, b)?;
                        }
                        checks.check(pointer(*value)?, exclusive, requirements, b)?;
                    }
                    InstructionKind::CallIndirect{callee,arguments}if b.raw_reference_cells&&arguments.iter().any(|argument|matches!(f.values.get(argument.0 as usize),Some(Type::Address{mutable:true,inner})if matches!(inner.as_ref(),Type::Address{..})))=>{
                        for target in reference_cells::code_targets(f,*callee,b)?{
                            check_call(module,f,target,arguments,solution,&state,&mut checks,summaries,requirements,byte_flow.as_ref(),byte_point.as_ref(),head_accesses,description_accesses,b)?;
                        }
                    }
                    InstructionKind::CallIndirect { arguments, .. }
                    | InstructionKind::CallClosure { arguments, .. }
                    | InstructionKind::CallClosureBorrowed { arguments, .. }
                    | InstructionKind::CallClosureShared { arguments, .. }
                    | InstructionKind::CallClosureMut { arguments, .. }
                    | InstructionKind::CallClosureOnce { arguments, .. } => {
                        let mut all: Vec<(Permission, bool)> = Vec::new();
                        match &i.kind {
                            InstructionKind::CallClosureBorrowed { callee, .. } => {
                                let p = pointer(*callee)?;
                                permission_work(p, b)?;
                                b.spend(1)?;
                                all.push((
                                    p.clone(),
                                    matches!(
                                        i.kind,
                                        InstructionKind::CallClosureBorrowed {
                                            kind: CallableKind::FnMut,
                                            ..
                                        }
                                    ),
                                ));
                            }
                            InstructionKind::CallClosureShared { callee, .. }
                            | InstructionKind::CallClosureMut { callee, .. } => {
                                b.spend(2)?;
                                all.push((
                                    Permission {
                                        sources: vec![Source {
                                            static_dependency: StaticProvenance::NotStatic,
                                            retained_port: None,
                                            base: HoldRoot::Local(*callee),
                                            leaf: vec![],
                                            inline: vec![],
                                            heap_parents: Vec::new(),
                                            heap_element: None,
                                        }],
                                        loans: vec![],
                                    },
                                    matches!(i.kind, InstructionKind::CallClosureMut { .. }),
                                ));
                            }
                            _ => {}
                        }

                        let receiver = match &i.kind {
                            InstructionKind::CallClosureShared { callee, .. }
                            | InstructionKind::CallClosureMut { callee, .. }
                                if fixed_environment(
                                    module,
                                    &f.locals[callee.0 as usize].ty,
                                    b,
                                ) =>
                            {
                                Some(&state.locals[callee.0 as usize])
                            }
                            InstructionKind::CallClosure { callee, .. }
                            | InstructionKind::CallClosureOnce { callee, .. }
                                if fixed_environment(module, vt(f, *callee)?, b) =>
                            {
                                Some(&solution.values[callee.0 as usize])
                            }
                            _ => None,
                        };
                        if let Some(receiver) = receiver {
                            b.spend(receiver.leaves.len())?;
                            let (definition, _) = stored_callables::called(module, f, &i.kind)
                                .ok_or_else(|| err("actual captured call lacks its entry"))?;
                            for (path, p) in &receiver.leaves {
                                if byte_queries::weak_capture_path(definition, path) {
                                    continue;
                                }
                                let current = returned_environment::current_leaf(
                                    definition, receiver, path, p, &state, b,
                                )?;
                                all.push((current, false));
                            }
                        }
                        if let InstructionKind::CallClosureBorrowed { callee, .. } = &i.kind
                            && let Some(id) = returned_environment::closure(vt(f, *callee)?)
                        {
                            let definition = crate::closures::definition(module, id, Some(i.span))?;
                            for (path, _) in returned_environment::leaves(module, id, b)? {
                                if byte_queries::weak_capture_path(definition, &path) {
                                    continue;
                                }
                                let permission = select(&state, pointer(*callee)?, &path, b)?;
                                b.spend(1)?;
                                all.push((permission, false));
                            }
                        }
                        if matches!(
                            i.kind,
                            InstructionKind::CallClosureShared { .. }
                                | InstructionKind::CallClosureMut { .. }
                                | InstructionKind::CallClosureBorrowed { .. }
                                | InstructionKind::CallClosure { .. }
                                | InstructionKind::CallClosureOnce { .. }
                        ) && let Some((definition, _actuals)) =
                            stored_callables::called(module, f, &i.kind)
                        {
                            let obligations = &summaries[definition.function.0 as usize].apart;
                            if !obligations.is_empty() {
                                let before = call_results::Before::closure(module, f, i, &solution.values, &state, b)?;
                                for (left, right) in obligations {
                                    let Some((left, right)) = before.separation(left, right, b)? else { continue; };
                                    for a in &left.sources {
                                        for z in &right.sources { apart(a, z, requirements, b)?; }
                                    }
                                }
                            }
                        }
                        for argument in arguments {
                            let actual = &solution.values[argument.0 as usize];
                            if let Type::Address { mutable, .. } = vt(f, *argument)? {
                                permission_work(&actual.address, b)?;
                                b.spend(1)?;
                                all.push((actual.address.clone(), *mutable));
                            }
                            for (_, p) in &actual.leaves {
                                permission_work(p, b)?;
                                b.spend(1)?;
                                all.push((p.clone(), false));
                            }
                            if let Some((_, shape)) = addressed(vt(f, *argument)?) {
                                for slot in &shape.slots {
                                    let p = select(&state, &actual.address, &slot.path, b)?;
                                    b.spend(1)?;
                                    all.push((p, false));
                                }
                            }
                        }
                        if let Some((definition,actuals))=stored_callables::called(module,f,&i.kind){
                            for(source,write)in &summaries[definition.function.0 as usize].head_accesses{
                                let access=captured_owner::substitute_access(module,f,i,source,definition,actuals,&solution.values,&state,b)?;
                                let permission=match access{captured_owner::Access::OwnedEnvironment=>continue,captured_owner::Access::External(permission)=>permission};
                                if *write{store_paths::writable(&permission,&solution.loans,b)?;}
                                checks.check(&permission,*write,requirements,b)?;
                                store_paths::record(head_accesses,&permission,*write,b)?;
                            }
                            if !summaries[definition.function.0 as usize].description_accesses.is_empty(){
                                let before=call_results::Before::closure(module,f,i,&solution.values,&state,b)?;
                                for row in &summaries[definition.function.0 as usize].description_accesses{
                                    let permission=descriptor_access::resolve(&before,row,b)?;
                                    if permission.sources.is_empty(){return Err(err("descriptor callback access lacks every actual manager source"));}
                                    if row.write{store_paths::writable(&permission,&solution.loans,b)?;}
                                    checks.check(&permission,row.write,requirements,b)?;
                                    descriptor_access::record(module,f,head_accesses,description_accesses,&permission,row.write,b)?;
                                }
                            }

                        }
                        if b.dyn_contexts
                            && let Some((definition, actuals)) =
                                stored_callables::called(module, f, &i.kind)
                        {
                            let actual = &summaries[definition.function.0 as usize];
                            if !actual.dyn_accesses.is_empty() {
                                // These are sequential actual body accesses,
                                // not a list of mutually disjoint arguments.
                                // Inferred apart obligations are checked above.
                                for (source, write) in &actual.dyn_accesses {
                                    let access = captured_owner::substitute_access(
                                        module,
                                        f,
                                        i,
                                        source,
                                        definition,
                                        actuals,
                                        &solution.values,
                                        &state,
                                        b,
                                    )?;
                                    let p=match access{captured_owner::Access::OwnedEnvironment=>continue,captured_owner::Access::External(permission)=>permission};
                                    if *write{store_paths::writable(&p,&solution.loans,b)?;}
                                    checks.check(&p, *write, requirements, b)?;
                                }
                            }
                        }
                        for (index, (p, write)) in all.iter().enumerate() {
                            checks.check(p, *write, requirements, b)?;
                            for (other, other_write) in &all[..index] {
                                if *write || *other_write {
                                    for a in &p.sources {
                                        for z in &other.sources {
                                            apart(a, z, requirements, b)?;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    InstructionKind::MakeClosure { closure, captures }
                        if module
                            .closures
                            .get(closure.0 as usize)
                            .is_some_and(|d| d.storage_life.is_some()) =>
                    {
                        b.spend(captures.len())?;
                        for (value, ty) in captures
                            .iter()
                            .zip(&module.closures[closure.0 as usize].captures)
                        {
                            let actual = &solution.values[value.0 as usize];
                            if matches!(ty, Type::Address { .. }) {
                                checks.check(&actual.address, false, requirements, b)?;
                            }
                            if let Some((_, shape)) = addressed(ty) {
                                for slot in &shape.slots {
                                    let p = select(&state, &actual.address, &slot.path, b)?;
                                    checks.check(&p, false, requirements, b)?;
                                }
                            }
                        }
                    }
                    InstructionKind::Call { function, arguments }
                    | InstructionKind::SuspendCall { function, arguments } => check_call(
                        module,
                        f,
                        *function,
                        arguments,
                        solution,
                        &state,
                        &mut checks,
                        summaries,
                        requirements,
                        byte_flow.as_ref(),
                        byte_point.as_ref(),
                        head_accesses,
                        description_accesses,
                        b,
                    )?,
                    _ => {}
                }
                Ok(())
            })()
            .map_err(|error| at_instruction(error).with_note(format!("actual instruction {:?}", i.kind)))?;
            dyn_providers::observe(
                module,
                f,
                i,
                &state,
                &solution.values,
                summaries,
                dyn_accesses,
                description_accesses,
                b,
            )?;
            step(
                module,
                f,
                i,
                &mut state,
                &solution.values,
                summaries,
                &solution.sites,
                &mut solution.dyn_sites,
                &mut solution.loans,
                is_deferred_address(i, &projection_heads),
                b,
            )?;
            if let Some(flow) = &byte_flow {
                flow.advance(
                    module,
                    f,
                    i,
                    &solution.values,
                    summaries,
                    byte_point.as_mut().unwrap(),
                    b,
                )?;
            }
        }
    }
    Ok(())
}
fn summary_work(s: &Summary, b: &mut Budget) -> Check<()> {
    descriptor_access::work(&s.description_accesses, b)?;
    value_transport::after_work(&s.value_after, b)?;
    b.spend(s.field_result_facts.len())?;
    for (path, facts) in &s.field_result_facts {
        b.spend(path.len())?;
        value_transport::result_work(facts, b)?;
    }
    b.spend(s.physical_result_parents.len())?;
    for (path, parents) in &s.physical_result_parents {
        b.spend(path.len())?;
        source_work(parents, b)?;
    }
    reference_cells::after_work(&s.reference_after, b)?;
    {
        b.spend(s.head_accesses.len())?;
        for (source, _) in &s.head_accesses {
            b.spend(width(source))?;
        }
    }
    if b.raw_reference_cells && !b.dyn_contexts {
        source_work(&s.dyn_result_parents, b)?;
    }
    if b.dyn_contexts {
        dyn_ports::work(&s.dyn_ports, b)?;
        dyn_providers::work(s, b)?;
        source_work(&s.dyn_result_parents, b)?;
        for (_, rows) in &s.dyn_after {
            dyn_ports::work(rows, b)?;
        }
        b.spend(s.contract_after.len())?;
        for (_, rows) in &s.contract_after {
            dyn_ports::work(rows, b)?;
        }
        for (_, rows) in &s.dyn_capture_after {
            dyn_ports::work(rows, b)?;
        }
    }
    if b.byte_queries {
        source_work(&s.byte_reads, b)?;
        b.spend(1)?;
    }
    if !s.none_values.is_empty() {
        b.spend(s.none_values.len())?;
    }
    s.returned_enum_variants.work(b)?;
    for (_, tags) in &s.enum_values {
        tags.work(b)?;
    }
    s.returned_variants.work(b)?;
    b.spend(s.receiver_results.len())?;
    for path in &s.receiver_results {
        b.spend(path.len())?;
    }
    b.spend(
        s.result
            .len()
            .saturating_add(s.external_after.len())
            .saturating_add(s.capture_after.len())
            .saturating_add(s.apart.len()),
    )?;
    if b.task_sources() {
        for (path, row) in &s.task_ports {
            b.spend(path.len())?;
            source_work(row, b)?;
        }
    }
    for (path, row) in &s.result {
        b.spend(path.len())?;
        source_work(row, b)?;
    }
    for (_, path) in &s.external_stable {
        b.spend(path.len().saturating_add(2))?;
    }
    for (_, path, row) in &s.external_after {
        b.spend(path.len())?;
        source_work(row, b)?;
    }
    for (_, path, row) in &s.capture_after {
        b.spend(path.len())?;
        source_work(row, b)?;
    }
    for (a, z) in &s.apart {
        b.spend(width(a).saturating_add(width(z)))?;
    }
    Ok(())
}

fn summarize(
    module: &TypedModule,
    f: &TypedFunction,
    g: &FunctionGraph,
    solution: &Solution,
    inactive: Option<&[ValueId]>,
    b: &mut Budget,
) -> Check<Summary> {
    let mut summary = Summary {
        complete: true,
        ..Summary::default()
    };
    // This closure alone appends rows to this invocation's fresh Summary.
    // Its private index records ordinals; it never reorders or certifies rows.
    b.spend(1)?;
    let mut result_index: Vec<(Path, usize)> = Vec::new();
    let mut result_row = |summary: &mut Summary,
                          path: &[PlaceProjection],
                          sources: &[Source],
                          b: &mut Budget|
     -> Check<()> {
        b.spend(1)?;
        let (mut low, mut high) = (0usize, result_index.len());
        while low < high {
            let mid = low + (high - low) / 2;
            let existing = &result_index[mid].0;
            b.spend(existing.len().saturating_add(path.len()).saturating_add(1))?;
            match existing.as_slice().cmp(path) {
                std::cmp::Ordering::Less => low = mid + 1,
                std::cmp::Ordering::Greater => high = mid,
                std::cmp::Ordering::Equal => {
                    b.spend(1)?;
                    return add_sources(&mut summary.result[result_index[mid].1].1, sources, b)
                        .map(|_| ());
                }
            }
        }
        let index_growth = if result_index.len() == result_index.capacity() {
            result_index.len()
        } else {
            0
        };
        let result_growth = if summary.result.len() == summary.result.capacity() {
            summary.result.len()
        } else {
            0
        };
        // Both complete keys, both records, shifts and actual reallocations
        // are paid before either table publishes the new row.
        b.spend(
            amount(path.len(), 2)?
                .saturating_add(2)
                .saturating_add(result_index.len() - low)
                .saturating_add(index_growth)
                .saturating_add(result_growth),
        )?;
        source_work(sources, b)?;
        let ordinal = summary.result.len();
        summary.result.push((path.to_vec(), sources.to_vec()));
        result_index.insert(low, (path.to_vec(), ordinal));
        Ok(())
    };
    for (bi, block) in f.blocks.iter().enumerate() {
        if g.reachable[bi] && matches!(block.terminator, Terminator::Return(_)) {
            b.spend(solution.outputs[bi].vacant.len())?;
            if solution.outputs[bi]
                .vacant
                .iter()
                .any(|slot| matches!(slot.base, HoldRoot::Parameter(_) | HoldRoot::Capture(_)))
            {
                summary.complete = false;
            }
        }
    }
    for (id, (ty, p)) in f.values.iter().zip(&solution.values).enumerate() {
        b.spend(1)?;
        if p.enum_variants.present() {
            p.enum_variants.work(b)?;
            summary
                .enum_values
                .push((ValueId(id as u32), p.enum_variants.clone()));
        }
        if p.option_variants.mask() == 1 {
            b.spend(2)?;
            summary.none_values.push(ValueId(id as u32));
        }
        if let Some(dead) = inactive {
            b.spend(dead.len().saturating_add(1))?;
            if dead.contains(&ValueId(id as u32)) {
                continue;
            }
        }
        if b.dyn_contexts && !dyn_returns::complete(module, ty, p, b)? {
            summary.complete = false;
        }
        if p.field_facts
            .iter()
            .any(|(_, facts)| facts.creation == Some(false))
        {
            summary.complete = false;
        }
        if p.creation_complete == Some(false) {
            summary.complete = false;
        }
        if matches!(ty, Type::Address { .. }) && p.address.sources.is_empty() {
            summary.complete = false;
        }
        if b.stored_enum_addresses
            && matches!(ty, Type::ErrorUnion { .. })
            && enum_address::ordinary(module, ty, b)?
        {
            if !p.enum_variants.grounded(ty, b)? {
                summary.complete = false;
            }
            for (path, _) in enum_address::slots(ty, b)? {
                if p.enum_variants.slot_active(&path)
                    && leaf(p, &path, b)?.is_none_or(|p| p.sources.is_empty())
                {
                    summary.complete = false;
                }
            }
        }
        if let Some(slots) = compound::slots(ty, b)? {
            for (path, _) in slots {
                if leaf(p, &path, b)?.is_none_or(|permission| permission.sources.is_empty()) {
                    summary.complete = false;
                }
            }
        }
        if shared_array_shape(ty, b)? {
            let Type::Array { length, .. } = ty else {
                unreachable!()
            };
            b.spend(*length as usize)?;
            for index in 0..*length {
                if leaf(p, &[PlaceProjection::Field(index)], b)?
                    .is_none_or(|p| p.sources.is_empty())
                {
                    summary.complete = false;
                }
            }
        }
        if let Some((_, shape)) = qualified(ty) {
            if enum_address::ordinary(module, ty, b)? {
                if !p.enum_variants.grounded(ty, b)? {
                    summary.complete = false;
                }
            } else if option_address::candidate(ty) {
                b.spend(96)?;
                if !option_address::signature(ty) {
                    return Err(err("Option activity has no exact shape"));
                }
                let mask = p.option_variants.mask();
                if mask == 0 {
                    summary.complete = false;
                }
                if mask == 1 {
                    continue;
                }
            }
            for row in &shape.slots {
                if p.enum_variants.present() && !p.enum_variants.slot_active(&row.path) {
                    continue;
                }
                if leaf(p, &row.path, b)?.is_none_or(|p| p.sources.is_empty()) {
                    summary.complete = false;
                }
            }
        }
        if b.returned_environment_sources {
            if let Some(id) = returned_environment::closure(ty) {
                for (path, _) in returned_environment::leaves(module, id, b)? {
                    if leaf(p, &path, b)?.is_none_or(|permission| permission.sources.is_empty()) {
                        summary.complete = false;
                    }
                }
            }
        } else if fixed_environment(module, ty, b) {
            let carrier = if let Type::Address { inner, .. } = ty {
                inner.as_ref()
            } else {
                ty
            };
            let Type::Closure(id) = carrier else {
                return Err(err("actual environment type is absent"));
            };
            let d = &module.closures[id.0 as usize];
            b.spend(d.captures.len())?;
            for (index, ty) in d.captures.iter().enumerate() {
                let prefix = vec![PlaceProjection::Field(index as u32)];
                if matches!(ty, Type::Address { .. })
                    && leaf(p, &prefix, b)?.is_none_or(|p| p.sources.is_empty())
                {
                    summary.complete = false;
                }
                if let Some((_, shape)) = addressed(ty) {
                    for row in &shape.slots {
                        b.spend(row.path.len().saturating_add(1))?;
                        let mut path = prefix.clone();
                        path.extend(&row.path);
                        if leaf(p, &path, b)?.is_none_or(|p| p.sources.is_empty()) {
                            summary.complete = false;
                        }
                    }
                }
            }
        }
    }
    if b.closure_receiver_borrows || b.dyn_contexts {
        for (parameter, payload) in solution.entry.external.iter().enumerate() {
            let payload = memory_holder(&solution.entry, HoldRoot::Parameter(parameter as u32), b)?
                .unwrap_or(payload);
            if !b.closure_receiver_borrows {
                let local = f
                    .parameters
                    .get(parameter)
                    .and_then(|id| f.locals.get(id.0 as usize))
                    .ok_or_else(|| err("stable external input has no actual parameter"))?;
                if !matches!(&local.ty, Type::Address { mutable: true, inner } if qualified(inner).is_some())
                {
                    continue;
                }
            }
            for (path, initial) in &payload.leaves {
                let mut stable = true;
                let mut returned = false;
                for (bi, (block, live)) in f.blocks.iter().zip(&g.reachable).enumerate() {
                    b.spend(1)?;
                    if *live && matches!(block.terminator, Terminator::Return(_)) {
                        returned = true;
                        let holder = memory_holder(
                            &solution.outputs[bi],
                            HoldRoot::Parameter(parameter as u32),
                            b,
                        )?
                        .ok_or_else(|| err("terminal external memory holder is absent"))?;
                        let terminal = leaf(holder, path, b)?;
                        permission_work(initial, b)?;
                        if let Some(terminal) = terminal {
                            permission_work(terminal, b)?;
                        }
                        stable &= terminal == Some(initial);
                    }
                }
                if returned && stable {
                    b.spend(path.len().saturating_add(2))?;
                    summary
                        .external_stable
                        .push((parameter as u32, path.clone()));
                }
            }
        }
    }
    for (bi, (block, reachable)) in f.blocks.iter().zip(&g.reachable).enumerate() {
        b.spend(1)?;
        if *reachable && solution.outputs[bi].pending_external_update {
            summary.complete = false;
        }
        if *reachable && matches!(block.terminator, Terminator::Return(_)) {
            captured_owner::summarize(module, f, &solution.outputs[bi], &mut summary, b)?;
            value_transport::summarize_after(
                module,
                f,
                &solution.entry,
                &solution.outputs[bi],
                solution,
                &mut summary,
                b,
            )?;
        }
        if (b.stored_slot_updates || b.replacements || b.dyn_contexts || b.closure_receiver_borrows)
            && *reachable
            && matches!(block.terminator, Terminator::Return(_))
        {
            for (position, payload) in solution.outputs[bi].external.iter().enumerate() {
                let payload = memory_holder(
                    &solution.outputs[bi],
                    HoldRoot::Parameter(position as u32),
                    b,
                )?
                .unwrap_or(payload);
                let local = *f
                    .parameters
                    .get(position)
                    .ok_or_else(|| err("external output exceeds actual parameter table"))?;
                let ty = &f.locals[local.0 as usize].ty;
                let borrowed = matches!(ty,
                    Type::Address { mutable, inner } if (*mutable || b.dyn_contexts) && (qualified(inner).is_some() || returned_environment::closure(inner).is_some()));
                if !borrowed && !matches!(ty, Type::Closure(_)) {
                    continue;
                }
                for (path, permission) in &payload.leaves {
                    if !borrowed
                        && captured_owner::external_capture_tail(module, ty, path, b)?.is_none()
                    {
                        continue;
                    }
                    b.spend(
                        path.len()
                            .saturating_add(permission.sources.len())
                            .saturating_add(1),
                    )?;
                    for source in &permission.sources {
                        if !matches!(source.base, HoldRoot::Parameter(_) | HoldRoot::Static) {
                            return Err(err("external slot update retains a callee-local target"));
                        }
                        b.spend(path.len())?;
                        if let HoldRoot::Parameter(_) = source.base
                            && captured_owner::class(
                                module,
                                f,
                                &Source {
                                    static_dependency: StaticProvenance::NotStatic,
                                    base: HoldRoot::Parameter(position as u32),
                                    leaf: path.clone(),
                                    inline: Vec::new(),
                                    retained_port: None,
                                    heap_parents: Vec::new(),
                                    heap_element: None,
                                },
                                b,
                            )? != captured_owner::class(module, f, source, b)?
                        {
                            return Err(err(
                                "external slot update exceeds its declared lifetime relation",
                            ));
                        }
                    }
                    let key = (position as u32, path.clone());
                    if let Some((_, _, row)) = summary
                        .external_after
                        .iter_mut()
                        .find(|(p, existing, _)| *p == key.0 && *existing == key.1)
                    {
                        add_sources(row, &permission.sources, b)?;
                    } else {
                        b.spend(
                            summary
                                .external_after
                                .len()
                                .saturating_mul(path.len().saturating_add(1)),
                        )?;
                        summary
                            .external_after
                            .push((key.0, key.1, permission.sources.clone()));
                    }
                }
            }
        }
        if *reachable && matches!(block.terminator, Terminator::Return(_)) {
            reference_cells::summarize(
                f,
                &solution.entry,
                &solution.outputs[bi],
                &solution.loans,
                &mut summary,
                b,
            )?;
            dyn_returns::after(
                module,
                f,
                &solution.outputs[bi],
                &mut summary,
                &solution.loans,
                &solution.dyn_sites,
                b,
            )?;
            dyn_captures::after(
                module,
                f,
                &solution.entry,
                &solution.outputs[bi],
                &mut summary,
                &solution.loans,
                &solution.dyn_sites,
                b,
            )?;
        }
        if *reachable && let Terminator::Return(Some(id)) = block.terminator {
            let payload = &solution.values[id.0 as usize];
            call_results::return_parents(module, f, payload, solution, &mut summary, b)?;
            value_transport::summarize(module, f, payload, solution, &mut summary, b)?;
            summary
                .returned_enum_variants
                .join(&payload.enum_variants, b)?;
            summary
                .returned_variants
                .join(&payload.option_variants, b)?;
            if matches!(f.return_type, Type::Address { .. }) {
                result_row(&mut summary, &[], &payload.address.sources, b)?;
                if receiver::returned_head(&payload.address, solution, b)? {
                    b.spend(summary.receiver_results.len().saturating_add(1))?;
                    if !summary.receiver_results.contains(&Vec::new()) {
                        summary.receiver_results.push(Vec::new());
                    }
                }
            }
            for (path, p) in &payload.leaves {
                result_row(&mut summary, path, &p.sources, b)?;
            }
            task_address::summarize(module, f, payload, &mut summary, b)?;
            dyn_returns::summarize(
                module,
                f,
                payload,
                &mut summary,
                &solution.loans,
                &solution.dyn_sites,
                b,
            )?;
            if (b.raw_reference_cells || b.dyn_contexts || b.closure_receiver_borrows)
                && matches!(f.return_type, Type::Address { .. })
            {
                let parents = dyn_returns::parents(&payload.address, &solution.loans, b)?;
                dyn_returns::check_parent_escape(f, &parents, b)?;
                add_sources(&mut summary.dyn_result_parents, &parents, b)?;
            }
        }
    }
    for loan in &solution.loans {
        let _ = ancestry(&loan.parents, &solution.loans, b)?;
    }
    for (id, loan) in solution.loans.iter().enumerate() {
        let parents = ancestry(&loan.parents, &solution.loans, b)?;
        b.spend(parents.len())?;
        if parents.contains(&id) {
            return Err(err("cyclic actual loan ancestry"));
        }
    }
    Ok(summary)
}

/// Every output and every call requirement is reconstructed from current raw
/// executable data. The candidate is compared only after the fixed point.
pub(super) fn check(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    plan: &CandidatePlan,
    b: &mut Budget,
) -> Check<()> {
    check_with_mask(module, graphs, plan, None, b)
}
pub(super) fn contract_dependencies(
    module: &TypedModule,
    function: FunctionId,
    b: &mut Budget,
) -> Check<Vec<FunctionId>> {
    let f = module
        .functions
        .get(function.0 as usize)
        .filter(|row| row.id == function)
        .ok_or_else(|| err("original contract dependency has no exact actual function"))?;
    dyn_providers::dependencies(module, f, b)
}
pub(super) fn check_mixed(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    plan: &CandidatePlan,
    mask: &[bool],
    b: &mut Budget,
) -> Check<()> {
    check_with_mask(module, graphs, plan, Some(mask), b)
}
fn check_exclusive_transfers(
    f: &TypedFunction,
    graph: &FunctionGraph,
    b: &mut Budget,
) -> Check<()> {
    // Reconstruct transfers from the raw CFG, without candidate roots or the
    // ordinary borrow verifier's separate function partition.
    let mut affine: Vec<bool> = f.values.iter().map(exclusive_carrier_value).collect();
    for instruction in f.blocks.iter().flat_map(|block| &block.instructions) {
        if transfers_address(&instruction.kind) {
            let (first, rest) = exclusive_consumers(&instruction.kind);
            for value in first.into_iter().chain(rest.iter().copied()) {
                affine[value.0 as usize] |= affine_stored_input(&f.values[value.0 as usize]);
            }
        }
    }
    if !affine.iter().any(|affine| *affine) {
        return Ok(());
    }
    let width = affine.len();
    b.spend(width.saturating_mul(f.blocks.len().saturating_add(2)))?;
    let mut outputs = vec![vec![false; width]; f.blocks.len()];
    loop {
        let mut changed = false;
        for (block_index, block) in f.blocks.iter().enumerate() {
            if !graph.reachable[block_index] {
                continue;
            }
            let mut used = vec![false; width];
            if block_index != f.entry.0 as usize {
                for &predecessor in &graph.predecessors[block_index] {
                    if graph.reachable[predecessor] {
                        b.spend(width)?;
                        for (state, old) in used.iter_mut().zip(&outputs[predecessor]) {
                            *state |= *old;
                        }
                    }
                }
            }
            for instruction in &block.instructions {
                if let Some(value) = instruction.result {
                    used[value.0 as usize] = false;
                }
                let (first, rest) = exclusive_consumers(&instruction.kind);
                for value in first.into_iter().chain(rest.iter().copied()) {
                    b.spend(1)?;
                    if affine[value.0 as usize]
                        && (exclusive_carrier_value(&f.values[value.0 as usize])
                            || transfers_address(&instruction.kind))
                    {
                        used[value.0 as usize] = true;
                    }
                }
            }
            if used != outputs[block_index] {
                outputs[block_index] = used;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    for (block_index, block) in f.blocks.iter().enumerate() {
        if !graph.reachable[block_index] {
            continue;
        }
        let mut used = vec![false; width];
        if block_index != f.entry.0 as usize {
            for &predecessor in &graph.predecessors[block_index] {
                if graph.reachable[predecessor] {
                    b.spend(width)?;
                    for (state, old) in used.iter_mut().zip(&outputs[predecessor]) {
                        *state |= *old;
                    }
                }
            }
        }
        for instruction in &block.instructions {
            if let Some(value) = instruction.result {
                used[value.0 as usize] = false;
            }
            let (first, rest) = exclusive_consumers(&instruction.kind);
            for value in instruction_operands(&instruction.kind) {
                b.spend(1)?;
                let index = value.0 as usize;
                if !affine[index] || !used[index] {
                    continue;
                }
                b.spend(rest.len().saturating_add(1))?;
                let consuming = (first == Some(value) || rest.contains(&value))
                    && (exclusive_carrier_value(&f.values[index])
                        || transfers_address(&instruction.kind));
                if !consuming {
                    return Err(err("exclusive stored SSA value was used after transfer"));
                }
            }
            for value in first.into_iter().chain(rest.iter().copied()) {
                let index = value.0 as usize;
                b.spend(1)?;
                if affine[index]
                    && (exclusive_carrier_value(&f.values[index])
                        || transfers_address(&instruction.kind))
                    && std::mem::replace(&mut used[index], true)
                {
                    return Err(err(
                        "exclusive stored carrier SSA value was transferred twice",
                    ));
                }
            }
        }
        if let Terminator::Return(Some(value)) = block.terminator
            && affine[value.0 as usize]
            && used[value.0 as usize]
        {
            return Err(err(
                "exclusive stored carrier result was already transferred",
            ));
        }
    }
    Ok(())
}
fn check_with_mask(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    plan: &CandidatePlan,
    mask: Option<&[bool]>,
    b: &mut Budget,
) -> Check<()> {
    b.diagnostic_stage("actual stored type qualification");
    let mixed_package = b.stored_package_instance;
    let compound_profile = compound::profile(module, b)?;
    let closed_dyn_profile = dyn_closed::related_resources(module, b)?;
    let owned_byte_profile = mask.is_none()
        && !mixed_package
        && !b.masked_stored_tasks()
        && !b.task_sources()
        && owned_byte_views::profile(module, b)?;
    if mask.is_none() || b.masked_stored_tasks() {
        if !mixed_package
            && !b.task_sources()
            && !b.masked_stored_tasks()
            && !owned_byte_profile
            && !closed_dyn_profile
            && !compound_profile
        {
            super::stored_owned::module(module, b)?;
        }
        if !b.returned_environment_sources {
            super::stored_callables::definitions(module, mask, b)?;
        }
    }
    for (index, f) in module.functions.iter().enumerate() {
        if mask.is_some_and(|mask| !mask[index]) {
            continue;
        }
        b.diagnostic_actual("actual stored header and type qualification", f.id, 0);
        check_exclusive_transfers(f, &graphs[f.id.0 as usize], b)?;
        super::class_flow::check(module, f, &graphs[f.id.0 as usize].reachable, b)?;
        if !module.resources.is_empty() && matches!(f.return_type, Type::Desc(_)) {
            return Err(unsupported(
                "owning Fixed profile does not export descriptions",
                Some(f.span),
            ));
        }
        if (f.foreign.is_some()
            && !foreign_calls::declaration(module, f, b)?
            && !crate::tasks::input_scope::provider(module, f, b)?)
            || f.callable_return_contract.is_some() && !b.returned_environment_sources
            || f.call_kind != CallKind::Direct
                && !(b.task_sources() && f.call_kind == CallKind::Suspend)
                && !crate::tasks::ordinary_sabi_body(f, b)?
        {
            return Err(unsupported(
                "stored profile requires ordinary direct function bodies",
                Some(f.span),
            ));
        }
        if let Some(id) = f.environment {
            let d = module
                .closures
                .get(id.0 as usize)
                .ok_or_else(|| err("callback function environment is absent"))?;
            if !super::stored_callables::finite(module, id) || d.function != f.id {
                return Err(err("callback function is not the actual finite entry"));
            }
            if !b.returned_environment_sources {
                super::stored_callables::owned_entry(module, d, b)?;
            }
        }
        for (type_ordinal, ty) in f
            .locals
            .iter()
            .map(|l| &l.ty)
            .chain(&f.values)
            .chain(std::iter::once(&f.return_type))
            .enumerate()
        {
            type_work(ty, &b.limits.clone(), b)?;
            let ordinary_value = super::stored_owned::ordinary_store_value(module, ty, b)?
                || super::stored_owned::ordinary_byte_value(ty, b)?
                || pure_copy::complete_owned_value(module, ty, b)?
                || type_ordinal < f.locals.len().saturating_add(f.values.len())
                    && pure_copy::frame_protocol_aggregate(module, f, ty, b)?
                || shared_tuple_value(module, ty, Some(f.span), b)?
                || matches!(ty,Type::Address{inner,..}if shared_tuple_value(module,inner,Some(f.span),b)?)
                || pure_copy::callable_tuple(module, ty, b)?
                || callable_cells::admitted_value(module, ty, b)?
                || ordinary_owner_address(module, ty, b)?
                || ordinary_task_enum(module, ty, b)?;
            let byte_value = owned_byte_profile && owned_byte_views::value(ty, b)?;
            let closed_dyn_value =
                dyn_closed::value(module, ty, b)? || dyn_ports::value(module, ty, b)?;
            let neutral_owner =
                b.task_sources() && crate::tasks::input_scope::neutral_owner_value(ty, b)?;
            if !module.resources.is_empty()
                && !mixed_package
                && !(b.task_sources() && task_address::shape(ty))
                && !b.masked_stored_tasks()
                && !byte_value
                && !closed_dyn_value
                && !neutral_owner
                && !compound_profile
                && !ordinary_value
            {
                super::stored_owned::value_type(ty, b)?;
            }
            let mixed_resource = if mixed_package || b.task_sources() {
                b.spend(module.resources.len())?;
                matches!(ty, Type::Managed(payload) | Type::Desc(payload)
                    if module.resources.iter().any(|definition| definition.payload == **payload))
                    || matches!(ty, Type::Address { inner, .. }
                        if matches!(inner.as_ref(), Type::Managed(payload)
                            if module.resources.iter().any(|definition| definition.payload == **payload)))
            } else {
                false
            };
            if !(ordinary_value
                || byte_value
                || closed_dyn_value
                || super::stored_callables::value(module, ty)
                || b.task_sources() && task_address::admitted_value(ty)
                || neutral_owner
                || qualified(ty).is_some()
                || addressed(ty).is_some()
                || b.raw_reference_cells && reference_cells::nested(ty)
                || matches!(ty,Type::Address{inner,..}if inner.is_address_pointee())
                || ty.is_copy_value()
                || shared_array_shape(ty, b)?
                || compound::slots(ty, b)?.is_some()
                || matches!(ty, Type::Address { inner, .. } if shared_array_shape(inner, b)?)
                || super::stored_owned::owner(ty, b)?
                || super::stored_owned::description(ty, b)?
                || b.masked_stored_tasks()
                    && matches!(
                        ty,
                        Type::Package { .. }
                            | Type::OwnedStore { .. }
                            | Type::ByteBuffer
                            | Type::Managed(_)
                            | Type::Desc(_)
                            | Type::ErrorUnion { .. }
                    )
                || b.masked_stored_tasks()
                    && matches!(ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Package { .. } | Type::OwnedStore { .. } | Type::Managed(_)))
                || mixed_resource
                || b.stored_enum_addresses
                    && enum_address::signature(ty)
                    && enum_address::ordinary(module, ty, b)?
                || pure_inline(module, ty, b)?
                || matches!(ty, Type::Address { inner, mutable: false } if pure_inline(module, inner, b)?))
            {
                b.spend(128)?;
                return Err(unsupported(
                    "owned or opaque executable value lacks actual stored permission integration",
                    Some(f.span),
                ).with_note(format!("actual function {:?}; complete type ordinal {type_ordinal} (locals, SSA values, result)",f.id)));
            }
        }
    }
    if mask.is_none() && !b.returned_environment_sources {
        super::stored_callables::roles(module, graphs, b)?;
    }
    solve_check(module, graphs, plan, None, mask, b)
}
// Direct ordinary calls can consume the independently reconstructed callee
// result immediately when all their physical call edges are acyclic. Calls
// in every block are included because summary completeness inspects them.
fn direct_dependency_postorder(
    module: &TypedModule,
    mask: Option<&[bool]>,
    b: &mut Budget,
) -> Check<Option<Vec<usize>>> {
    if !b.store_option_results
        || !b.ordinary_error_places
        || b.tasks
        || b.registrations
        || b.async_registrations
        || b.dyn_contexts
        || !module.closures.is_empty()
        || !module.dyn_interfaces.is_empty()
        || !module.dyn_vtables.is_empty()
    {
        return Ok(None);
    }
    direct_callee_postorder(module, mask, b)
}

pub(super) fn direct_callee_postorder(
    module: &TypedModule,
    mask: Option<&[bool]>,
    b: &mut Budget,
) -> Check<Option<Vec<usize>>> {
    let count = module.functions.len();
    b.spend(amount(
        count,
        std::mem::size_of::<Vec<usize>>()
            + std::mem::size_of::<(usize, usize)>()
            + std::mem::size_of::<usize>()
            + std::mem::size_of::<u8>(),
    )?)?;
    let mut calls = vec![Vec::new(); count];
    for (index, f) in module.functions.iter().enumerate() {
        b.spend(1)?;
        if mask.is_some_and(|members| !members[index]) {
            continue;
        }
        b.spend(f.blocks.len())?;
        for block in &f.blocks {
            b.spend(block.instructions.len())?;
            for operation in &block.instructions {
                match operation.kind {
                    InstructionKind::Call { function, .. } => {
                        let callee = function.0 as usize;
                        if callee >= count {
                            return Err(err("actual direct stored dependency is absent"));
                        }
                        let outgoing = &mut calls[index];
                        b.spend(amount(
                            outgoing.len().saturating_add(2),
                            std::mem::size_of::<usize>(),
                        )?)?;
                        outgoing.push(callee);
                    }
                    InstructionKind::SuspendCall { .. }
                    | InstructionKind::CallIndirect { .. }
                    | InstructionKind::CallDynSlot { .. }
                    | InstructionKind::CallClosure { .. }
                    | InstructionKind::CallClosureBorrowed { .. }
                    | InstructionKind::CallClosureShared { .. }
                    | InstructionKind::CallClosureMut { .. }
                    | InstructionKind::CallClosureOnce { .. } => return Ok(None),
                    _ => {}
                }
            }
        }
    }
    let mut visit = vec![0u8; count];
    let mut stack = Vec::with_capacity(count);
    let mut order = Vec::with_capacity(count);
    for start in (0..count).rev() {
        b.spend(1)?;
        if visit[start] != 0 {
            continue;
        }
        visit[start] = 1;
        stack.push((start, 0usize));
        while let Some((index, next)) = stack.last_mut() {
            b.spend(3)?;
            if let Some(&callee) = calls[*index].get(*next) {
                *next += 1;
                match visit[callee] {
                    0 => {
                        visit[callee] = 1;
                        stack.push((callee, 0));
                    }
                    1 => return Ok(None),
                    _ => {}
                }
            } else {
                visit[*index] = 2;
                order.push(*index);
                stack.pop();
            }
        }
    }
    Ok(Some(order))
}

fn solve_check(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    plan: &CandidatePlan,
    inactive: Option<&[Vec<ValueId>]>,
    mask: Option<&[bool]>,
    b: &mut Budget,
) -> Check<()> {
    let pending_byte_edges = byte_queries::inventory(module, graphs, mask, b)?;
    let order = direct_dependency_postorder(module, mask, b)?;
    b.spend(module.functions.len().saturating_mul(2))?;
    let recursion = if b.dyn_contexts && b.static_borrow_occurrences && mask.is_none() {
        Some(readonly_recursion::Inventory::build(module, graphs, b)?)
    } else {
        None
    };
    let (mut recursive_assumptions, mut local_complete) = if recursion.is_some() {
        b.spend(amount(module.functions.len(), 2)?)?;
        (
            vec![false; module.functions.len()],
            vec![false; module.functions.len()],
        )
    } else {
        (Vec::new(), Vec::new())
    };
    let mut round = 0usize;
    let mut observed_none = false;
    let mut summaries = vec![Summary::default(); module.functions.len()];
    if let Some(mask) = mask {
        for (index, yes) in mask.iter().enumerate() {
            if !yes {
                summaries[index].complete = true;
            }
        }
    }
    let mut solved: Vec<Option<Solution>> = (0..module.functions.len()).map(|_| None).collect();
    loop {
        let mut changed = false;
        let mut pending = Vec::new();
        round += 1;
        b.spend(module.functions.len())?;
        for position in 0..module.functions.len() {
            let index = order
                .as_ref()
                .map_or(module.functions.len() - 1 - position, |order| order[position]);
            if mask.is_some_and(|mask| !mask[index]) {
                continue;
            }
            let f = &module.functions[index];
            b.diagnostic_actual("actual stored CFG", f.id, round);
            let mut solution =
                solve(module, f, &graphs[index], &summaries, b).map_err(|mut failure| {
                    failure.notes.push(format!(
                        "while solving actual stored CFG of {} (summary round {round})",
                        f.name
                    ));
                    failure
                })?;
            b.diagnostic_label("actual stored summary");
            let mut summary = summarize(
                module,
                f,
                &graphs[index],
                &solution,
                inactive.map(|rows| rows[index].as_slice()),
                b,
            )
            .map_err(|mut failure| {
                failure.notes.push(format!(
                    "while summarizing actual stored targets of {} (round {round})",
                    f.name
                ));
                failure
            })?;
            if let Some(inventory) = &recursion {
                local_complete[index] = summary.complete
                    && inventory.eligible(index)
                    && readonly_recursion::Inventory::lower_roots(f, &solution, b)?;
            }
            if b.raw_reference_cells || b.stored_slot_updates || b.dyn_contexts || b.replacements {
                b.spend(f.blocks.len())?;
                for block in &f.blocks {
                    b.spend(block.instructions.len())?;
                    for instruction in &block.instructions {
                        let function = match &instruction.kind {
                            InstructionKind::Call { function, .. }
                            | InstructionKind::SuspendCall { function, .. } => *function,
                            kind => {
                                let Some((definition, _)) =
                                    super::stored_callables::called(module, f, kind)
                                else {
                                    continue;
                                };
                                definition.function
                            }
                        };
                        let callee = module
                            .functions
                            .get(function.0 as usize)
                            .ok_or_else(|| err("stored call target is absent"))?;
                        let prior = summaries
                            .get(function.0 as usize)
                            .ok_or_else(|| err("stored call summary is absent"))?;
                        // A stateless constant callback supplies only an exact
                        // conditional Result obligation. Its actual Returns and
                        // least-root graph are all checked below; no caller
                        // storage/after-state can be hidden by this exception.
                        let conditional_result = if let Some((definition, _)) =
                            super::stored_callables::called(module, f, &instruction.kind)
                        {
                            static_borrows::stateless_result(module, definition, b)?
                        } else {
                            false
                        };
                        let read_only_cycle = recursion.as_ref().is_some_and(|inventory| {
                            inventory.internal(index, function.0 as usize, &recursive_assumptions)
                        });
                        if !prior.complete && !conditional_result && !read_only_cycle {
                            summary.complete = false;
                        }
                        for (position, local) in callee.parameters.iter().enumerate() {
                            if let Type::Address {
                                mutable: true,
                                inner,
                            } = &callee.locals[local.0 as usize].ty
                                && let Some((_, shape)) = qualified(inner)
                            {
                                for slot in &shape.slots {
                                    b.spend(
                                        slot.path
                                            .len()
                                            .saturating_add(prior.external_after.len())
                                            .saturating_add(1),
                                    )?;
                                    if !prior.external_after.iter().any(|(p, path, _)| {
                                        *p == position as u32 && *path == slot.path
                                    }) {
                                        summary.complete = false;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            dyn_providers::effects(module, f, &graphs[index], &summaries, &mut summary, b)?;
            observed_none |= !summary.none_values.is_empty();
            if summary.complete {
                summary.byte_covered = b.byte_queries;
                windows(
                    module,
                    f,
                    &graphs[index],
                    &mut solution,
                    &summaries,
                    &mut summary.apart,
                    &mut summary.byte_reads,
                    &mut summary.head_accesses,
                    &mut summary.description_accesses,
                    &mut summary.dyn_accesses,
                    b,
                )
                .map_err(|mut failure| {
                    failure.notes.push(format!(
                        "while reconstructing actual stored accesses of {}",
                        f.name
                    ));
                    failure
                })?;
            }
            summary_work(&summary, b)?;
            if order.is_some() {
                summaries[index] = summary;
                solved[index] = Some(solution);
                continue;
            }
            summary_work(&summaries[index], b)?;
            if summary != summaries[index] {
                changed = true;
            }
            pending.push((index, summary));
            solved[index] = Some(solution);
        }
        for (index, s) in pending {
            summaries[index] = s;
        }
        if let Some(inventory) = &recursion {
            let next = inventory.assumptions(&local_complete, &summaries, b)?;
            b.spend(next.len())?;
            changed |= recursive_assumptions != next;
            recursive_assumptions = next;
        }
        if order.is_some() || !changed {
            break;
        }
    }
    if (mask.is_none() || b.masked_stored_tasks())
        && inactive.is_none()
        && let Some(proof) = enum_address::refine(module, graphs, &summaries, b)?
    {
        return solve_check(module, &proof.graphs, plan, Some(&proof.inactive), mask, b);
    }
    if (mask.is_none() || b.masked_stored_tasks())
        && inactive.is_none()
        && observed_none
        && let Some(proof) = option_address::refine(module, graphs, &summaries, b)?
    {
        return solve_check(module, &proof.graphs, plan, Some(&proof.inactive), mask, b);
    }
    if b.static_borrow_occurrences {
        static_borrows::validate(module, graphs, &solved, mask, b)?;
    }
    dyn_providers::validate(module, &summaries, b)?;
    dyn_contracts::validate(module, &summaries, b)?;
    let mut checked_task_sources = if b.task_sources() {
        Some(Vec::with_capacity(module.functions.len()))
    } else {
        None
    };
    if b.masked_stored_tasks() {
        b.spend(module.functions.len())?;
    }
    let mut checked_option_sources = b
        .masked_stored_tasks()
        .then(|| Vec::with_capacity(module.functions.len()));
    let mut package_calls = if b.qualified_package_returns {
        b.spend(std::mem::size_of::<BTreeMap<(FunctionId, ValueId), u32>>())?;
        Some(BTreeMap::new())
    } else {
        None
    };
    for (index, (f, s)) in module.functions.iter().zip(&summaries).enumerate() {
        if mask.is_some_and(|mask| !mask[index]) {
            if let Some(checked) = &mut checked_option_sources {
                b.spend(f.values.len())?;
                checked.push(StoreOptionFunctionSources {
                    values: vec![Vec::new(); f.values.len()],
                    byte_payloads: Vec::new(),
                });
            }
            if let Some(checked) = &mut checked_task_sources {
                b.spend(f.values.len())?;
                checked.push(TaskFunctionSources {
                    covered: false,
                    values: vec![TaskValueSources::default(); f.values.len()],
                    reference_inputs: Vec::new(),
                });
            }
            continue;
        }
        if b.byte_queries && b.byte_query_functions[index] && !s.byte_covered {
            return Err(err(
                "Byte query function lacks complete actual read coverage",
            ));
        }
        if !s.complete {
            let mut failure =
                err("stored/reference operation has no complete actual target permission");
            failure
                .notes
                .push(format!("while checking actual stored result of {}", f.name));
            return Err(failure);
        }
        if b.returned_environment_sources {
            returned_environment::check_summary(
                module,
                f,
                s,
                solved[index]
                    .as_ref()
                    .ok_or_else(|| err("callable source solution is absent"))?,
                &graphs[index],
                b,
            )?;
        }
        dyn_returns::validate(module, f, &s.dyn_ports, b)?;
        enum_address::projections(f, &graphs[index], s, b)?;
        let mut relations = relation_index::Relations::new(f, b)?;
        let receiver_definition =
            captured_definition(module, f, b).filter(|d| receiver::declared(d));
        let capture_result_definition = captured_definition(module, f, b).filter(|d| {
            d.storage_life.as_ref().is_some_and(|life| {
                life.bindings
                    .iter()
                    .any(|row| row.position.base == StorageBase::Result)
            })
        });
        for path in &s.receiver_results {
            let c = f
                .storage_life
                .as_ref()
                .ok_or_else(|| err("returned invocation view lacks its complete relation"))?;
            if receiver_definition.is_none()
                || value_transport::result_bound(module, f, path, &mut relations, b)?
                    != signature_life::outer(c, StorageBase::Receiver)
                        .ok_or_else(|| err("returned invocation view omits Receiver"))?
            {
                return Err(err(
                    "returned invocation view erased or changed its actual receiver bound",
                ));
            }
        }
        let lender = captured_definition(module, f, b).filter(|d| {
            super::stored_callables::lending(d)
                && !d.storage_life.as_ref().is_some_and(|life| {
                    life.bindings
                        .iter()
                        .any(|row| row.position.base == StorageBase::Result)
                })
        });
        if let Some(definition) = lender {
            b.spend(definition.lending_captures.len().saturating_add(1))?;
            let mut fields = Vec::new();
            for (path, origins) in &s.result {
                if !path.is_empty() || origins.is_empty() {
                    return Err(err("lender result lacks real capture sources"));
                }
                for origin in origins {
                    b.spend(width(origin).saturating_add(fields.len()).saturating_add(1))?;
                    let HoldRoot::Capture(field) = origin.base else {
                        return Err(err(
                            "lender retained activation or explicit argument storage",
                        ));
                    };
                    if !origin.leaf.is_empty()
                        || !origin.inline.is_empty()
                        || !definition
                            .captures
                            .get(field as usize)
                            .is_some_and(super::stored_callables::scalar)
                    {
                        return Err(err(
                            "lender result is not grounded in a real inline scalar capture",
                        ));
                    }
                    if let Err(index) = fields.binary_search(&field) {
                        fields.insert(index, field);
                    }
                }
            }
            if fields != definition.lending_captures {
                return Err(err(
                    "lending capture claim differs from independently recovered fields",
                ));
            }
        }
        if let Some(definition) = capture_result_definition {
            let mut used = Vec::new();
            for (path, sources) in &s.result {
                // lending_captures names the outer returned head. Every inner
                // slot is checked below against its own actual class relation.
                if !path.is_empty() {
                    continue;
                }
                for source in sources {
                    b.spend(used.len().saturating_add(1))?;
                    if let HoldRoot::Capture(index) = source.base {
                        match used.binary_search(&index) {
                            Ok(_) => {}
                            Err(at) => used.insert(at, index),
                        }
                    }
                }
            }
            b.spend(used.len().saturating_add(definition.lending_captures.len()))?;
            if used != definition.lending_captures {
                return Err(err(
                    "captured result source set differs from actual returned permissions",
                ));
            }
        }
        for (path, sources) in &s.result {
            if sources.is_empty() {
                return Err(err("stored/reference result has no actual input source"));
            }
            for source in sources {
                if lender.is_some() {
                    continue;
                }
                if source.base == HoldRoot::Static {
                    if !source.leaf.is_empty() || !source.inline.is_empty() {
                        return Err(err("static result has a nested permission projection"));
                    }
                    continue;
                }
                if let HoldRoot::Capture(_) = source.base
                    && capture_result_definition.is_some()
                {
                    let c = f
                        .storage_life
                        .as_ref()
                        .ok_or_else(|| err("capture result relation absent"))?;
                    let bound = relations.get(StorageBase::Result, path, b)?;
                    if s.receiver_results.contains(path) {
                        if Some(bound) != signature_life::outer(c, StorageBase::Receiver) {
                            return Err(err("captured reborrow escaped its invocation head"));
                        }
                    } else if bound != captured_owner::class(module, f, source, b)? {
                        return Err(err(
                            "shared capture result changed its original Fixed class",
                        ));
                    }
                    continue;
                }
                let HoldRoot::Parameter(position) = source.base else {
                    return Err(err(
                        "stored/reference return retains activation or capture storage",
                    ));
                };
                if let Some(bound) = &f.reference_return_bound {
                    b.spend(
                        bound
                            .len()
                            .saturating_add(source.leaf.len())
                            .saturating_add(1),
                    )?;
                    if !source.leaf.is_empty() || bound.binary_search(&position).is_err() {
                        return Err(err(
                            "actual returned target exceeds the existing direct reference upper",
                        ));
                    }
                }
                if f.storage_life.is_some()
                    && f.callable_return_contract.is_none()
                    && !(b.closure_receiver_borrows
                        && receiver::parameter_head_result(f, s, path, b)?)
                    && relations.get(StorageBase::Result, path, b)?
                        != (if source.retained_port.is_some() {
                            match &source.retained_port {
                                Some(PortOrigin::Task { .. }) => {
                                    task_address::source_relation(module, f, source, b)?
                                }
                                Some(PortOrigin::Dyn { carrier, port }) => {
                                    dyn_returns::source_relation(
                                        module, f, position, carrier, *port, b,
                                    )?
                                }
                                Some(PortOrigin::Reference { .. }) => {
                                    reference_cells::relation(module, f, source, b)?
                                }
                                None => unreachable!(),
                            }
                        } else {
                            relations.get(StorageBase::Parameter(position), &source.leaf, b)?
                        })
                    && !(path.is_empty()
                        && source.retained_port.is_none()
                        && source.inline.is_empty()
                        && source.heap_parents.is_empty()
                        && source.heap_element.is_none()
                        && crate::dynamic_witness::semantic_patterns::return_slot_borrow_view(
                            module,
                            f,
                            position,
                            &source.leaf,
                            b,
                        )?)
                {
                    return Err(err(
                        "actual returned target exceeds the complete stored relation",
                    ));
                }
            }
        }
        let solution = solved[index]
            .as_ref()
            .ok_or_else(|| err("independent solver omitted a function"))?;
        value_transport::validate_results(
            module,
            f,
            s,
            solution,
            &graphs[index],
            &mut relations,
            b,
        )?;
        if let Some(calls) = &mut package_calls {
            let rows = package_return_candidates(
                module,
                f,
                &graphs[index],
                &solution.values,
                &summaries,
                b,
            )?;
            b.spend(rows.len())?;
            for (value, position) in rows {
                b.spend(
                    calls
                        .len()
                        .checked_ilog2()
                        .map_or(1, |n| n as usize + 2)
                        .saturating_add(std::mem::size_of::<((FunctionId, ValueId), u32)>()),
                )?;
                calls.insert((f.id, value), position);
            }
        }
        if b.masked_stored_tasks() {
            b.spend(f.blocks.len())?;
            for (block, reachable) in f.blocks.iter().zip(&graphs[index].reachable) {
                if !reachable {
                    continue;
                }
                b.spend(block.instructions.len())?;
                for instruction in &block.instructions {
                    let InstructionKind::AddressField { .. } = instruction.kind else {
                        continue;
                    };
                    let result = instruction
                        .result
                        .ok_or_else(|| err("Store field result absent"))?;
                    if !matches!(vt(f, result)?, Type::Address { inner, .. }
                        if matches!(inner.as_ref(), Type::Package { .. }))
                    {
                        continue;
                    }
                    let sources = &solution
                        .values
                        .get(result.0 as usize)
                        .ok_or_else(|| err("Store field result source absent"))?
                        .address
                        .sources;
                    source_work(sources, b)?;
                    if sources.is_empty() {
                        return Err(err("owning Store field address lacks an actual source"));
                    }
                    let Type::Address { inner, .. } = vt(f, result)? else {
                        unreachable!()
                    };
                    for source in sources {
                        if source.heap_element.is_none()
                            && !ordinary_inline_package_source(f, source, inner, b)?
                        {
                            return Err(err(
                                "owning Store field address lacks an actual element or inline Package source",
                            ));
                        }
                    }
                }
            }
        }
        if let Some(checked) = &mut checked_option_sources {
            b.spend(solution.values.len())?;
            let mut values = Vec::with_capacity(solution.values.len());
            for (index, value) in solution.values.iter().enumerate() {
                source_work(&value.address.sources, b)?;
                let mut sources = value.address.sources.clone();
                value_transport::visit_fields(value, b, |permission, b| {
                    add_sources(&mut sources, &permission.sources, b)?;
                    Ok(())
                })?;
                if (b.shared_byte_borrow
                    && matches!(&f.values[index], Type::Desc(inner) if inner.as_ref() == &Type::ByteBuffer))
                    || (b.store_descriptions() || !value.store_description.sources.is_empty())
                        && matches!(&f.values[index], Type::Desc(_))
                {
                    add_sources(&mut sources, &value.store_description.sources, b)?;
                }
                for (_, leaf) in &value.leaves {
                    add_sources(&mut sources, &leaf.sources, b)?;
                }
                values.push(sources);
            }
            let byte_payloads = owned_byte_views::Sites::collect(f, b)?.payload_values(
                f,
                &solution.values,
                &solution.loans,
                b,
            )?;
            checked.push(StoreOptionFunctionSources {
                values,
                byte_payloads,
            });
        }
        if let Some(checked) = checked_task_sources.as_mut() {
            checked.push(task_address::checked_sources(module, f, solution, b)?);
        }
        let claimed = &plan.functions[index].holds;
        if claimed.len() != solution.values.len() {
            return Err(err("candidate must cover each actual value once"));
        }
        for (v, (p, claim)) in solution.values.iter().zip(claimed).enumerate() {
            let mut field_heap = false;
            value_transport::visit_fields(p, b, |permission, b| {
                b.spend(permission.sources.len())?;
                field_heap |= permission
                    .sources
                    .iter()
                    .any(|source| source.heap_element.is_some());
                Ok(())
            })?;
            if b.masked_stored_tasks()
                && !field_heap
                && !matches!(f.values[v], Type::BorrowedStorage { .. })
                && !p
                    .address
                    .sources
                    .iter()
                    .any(|source| source.heap_element.is_some())
                && !p
                    .leaves
                    .iter()
                    .flat_map(|(_, leaf)| &leaf.sources)
                    .any(|source| source.heap_element.is_some())
            {
                // Ordinary resource/borrow reconstruction checks this value's
                // public Hold; the stored lane owns only qualified carriers
                // and Store-element payload windows in this dual profile.
                continue;
            }
            let receiver_sources = if b.closure_receiver_borrows {
                receiver::held_sources(p, solution, b)?
            } else {
                Vec::new()
            };
            let mut actual = BTreeSet::new();
            let mut field_sources = Vec::new();
            for (path, facts) in &p.field_facts {
                b.spend(path.len().saturating_add(1))?;
                // Descriptions and extra view heads retain private ancestry;
                // they are not strong public address targets.
                add_sources(&mut field_sources, &facts.address.sources, b)?;
            }
            let weak_description = matches!(&f.values[v], Type::Desc(payload)
                if !matches!(payload.as_ref(), Type::ByteBuffer | Type::Task { .. }
                    | Type::TaskI32 | Type::OwnedStore { .. } | Type::Mutex { .. }));
            for root in p
                .address
                .sources
                .iter()
                .chain(p.store_description.sources.iter())
                .chain(receiver_sources.iter())
                .chain(field_sources.iter())
                .chain(p.leaves.iter().flat_map(|(_, p)| &p.sources))
                .chain(p.task_ports.iter().flat_map(|(_, p)| &p.sources))
                .chain(
                    p.dyn_ports
                        .0
                        .iter()
                        .flat_map(|(_, p)| &p.permission.sources),
                )
            {
                b.spend(actual.len().saturating_add(1))?;
                // The private activation-local alias makes End/liveness
                // explicit. Its public owner identity comes from the unique
                // executable Bind checked by capture_views, never its type.
                let base = match root.base {
                    HoldRoot::Local(local)
                        if matches!(
                            f.locals[local.0 as usize].ty,
                            Type::BorrowedStorage { .. }
                        ) =>
                    {
                        graphs[index]
                            .package_views
                            .capture(local)
                            .map_or(root.base, HoldRoot::Capture)
                    }
                    _ => root.base,
                };
                // The private ledger above still proves descriptor accesses
                // and retirement. Public Hold denotes strong address debt.
                if !weak_description {
                    actual.insert(base);
                }
            }
            // The complete private ledger above still checks descriptor
            // origins, epochs, parent loans and accesses. Its manager roots
            // are not strong public address debt, even for builtin heads.
            if matches!(f.values[v], Type::Desc(_)) {
                actual.clear();
            }
            b.spend(actual.len().saturating_add(claim.roots.len()))?;
            if claim.value != ValueId(v as u32) || !actual.iter().eq(claim.roots.iter()) {
                return Err(err(&format!(
                    "candidate differs from independently reconstructed stored target roots: {} value {v}: actual {actual:?}, candidate {:?}",
                    f.name, claim.roots
                )));
            }
        }
    }
    byte_queries::certify_edges(module, &pending_byte_edges, &summaries, &solved, mask, b)?;
    dyn_captures::finish(module, graphs, &solved, mask, b)?;
    if let Some(checked) = checked_task_sources {
        b.task_stored_sources = Some(checked);
    }
    b.store_option_sources = checked_option_sources;
    if b.qualified_package_returns {
        b.package_return_calls = package_calls;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    mod fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/language-readiness/terminal/higher-rank-borrows/stored_flow_fixture.rs"
        ));
    }

    #[test]
    fn scalar_description_helper_write_obligations_survive_without_store_profile_raw_wire() {
        let limits = Limits::default();
        let (mut module, _) = crate::tests::baseline();
        let span = module.functions[0].span;
        let desc = Type::Desc(Box::new(Type::Record {
            name: "Scalar".into(),
            fields: vec![("value".into(), Type::I32)],
        }));
        let caller = &mut module.functions[0];
        caller.parameters = vec![LocalId(0)];
        caller.locals = vec![LocalDecl {
            id: LocalId(0),
            name: "input".into(),
            ty: desc.clone(),
            mutable: false,
            span,
        }];
        caller.values = vec![desc, Type::I32];
        caller.return_type = Type::Unit;
        caller.blocks[0].instructions = vec![
            Instruction {
                result: Some(ValueId(0)),
                kind: InstructionKind::Load(LocalId(0)),
                span,
            },
            Instruction {
                result: Some(ValueId(1)),
                kind: InstructionKind::Constant(Constant::I32(1)),
                span,
            },
        ];
        caller.blocks[0].terminator = Terminator::Return(None);
        let mut helper = caller.clone();
        helper.id = FunctionId(1);
        helper.name = "scalar_write".into();
        let write = Instruction {
            result: None,
            kind: InstructionKind::WriteDesc {
                desc: ValueId(0),
                index: 0,
                value: ValueId(1),
            },
            span,
        };
        helper.blocks[0].instructions.push(write.clone());
        module.functions.push(helper);
        let plan = CandidatePlan::empty_for(&module);
        for wire in [false, true] {
            let decoded;
            let current = if wire {
                let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
                decoded = zil_ir::wire::decode_module_candidate(&bytes, &limits)
                    .unwrap()
                    .0;
                &decoded
            } else {
                &module
            };
            for exclusive in [false, true] {
                let root = Source {
                    static_dependency: StaticProvenance::NotStatic,
                    retained_port: None,
                    base: HoldRoot::Parameter(0),
                    leaf: vec![],
                    inline: vec![],
                    heap_parents: vec![],
                    heap_element: None,
                };
                let permission = Permission {
                    sources: vec![root.clone()],
                    loans: vec![0],
                };
                let mut values = vec![Payload::default(); 2];
                values[0].store_description = permission;
                let solution = Solution {
                    values,
                    outputs: vec![],
                    entry: Frame::default(),
                    loans: vec![Loan {
                        receiver_head: false,
                        sources: vec![root.clone()],
                        parents: vec![],
                        exclusive,
                    }],
                    sites: vec![],
                    dyn_sites: dyn_leases::Sites::default(),
                };
                let mut summaries = vec![Summary::default(); 2];
                let mut field = root;
                field.inline.push(PlaceProjection::Field(0));
                let mut seed_budget = Budget::new(&limits);
                {
                    let summary = &mut summaries[1];
                    descriptor_access::record(
                        current,
                        &current.functions[1],
                        &mut summary.head_accesses,
                        &mut summary.description_accesses,
                        &Permission {
                            sources: vec![field.clone()],
                            loans: vec![],
                        },
                        true,
                        &mut seed_budget,
                    )
                    .unwrap();
                }
                let mut b = Budget::new(&limits);
                assert!(!replacements::heads(&b));
                let mut accesses =
                    access_candidates::Access::new(&[0], &solution.loans, &mut b).unwrap();
                let mut rows = vec![];
                let mut description_rows = vec![];
                let result = check_call(
                    current,
                    &current.functions[0],
                    FunctionId(1),
                    &[ValueId(0)],
                    &solution,
                    &Frame::default(),
                    &mut accesses,
                    &summaries,
                    &mut vec![],
                    None,
                    None,
                    &mut rows,
                    &mut description_rows,
                    &mut b,
                );
                if exclusive {
                    result.unwrap();
                    assert!(rows.is_empty());
                    assert_eq!(description_rows.len(), 1);
                    assert!(description_rows[0].write);
                    assert_eq!(
                        descriptor_access::symbolic(&description_rows[0], &mut b).unwrap(),
                        field
                    );
                } else {
                    assert!(
                        result.is_err(),
                        "a helper cannot hide a write through a shared actual parent, wire {wire}"
                    );
                }
                // Memory upper checks consume actual accesses even though a
                // scalar description operation has no Dyn callback/effect.
                b.dyn_contexts = true;
                let mut dyn_rows = vec![];
                let mut desc_rows = vec![];
                dyn_providers::observe(
                    current,
                    &current.functions[1],
                    &write,
                    &Frame::default(),
                    &solution.values,
                    &summaries,
                    &mut dyn_rows,
                    &mut desc_rows,
                    &mut b,
                )
                .unwrap();
                assert!(dyn_rows.is_empty());
                assert_eq!(desc_rows.len(), 1);
                assert!(desc_rows[0].write);
                assert_eq!(
                    descriptor_access::symbolic(&desc_rows[0], &mut b).unwrap(),
                    field
                );
                assert!(!summaries[1].dyn_operations);
            }
        }
    }

    #[test]
    fn suspending_stored_call_keeps_its_actual_abi_operands_and_finite_frame_obligations() {
        let (mut module, _) = crate::tests::baseline();
        let span = module.functions[0].span;
        let target = &mut module.functions[0];
        target.call_kind = CallKind::Suspend;
        target.parameters = vec![LocalId(0)];
        target.values = vec![Type::I32];
        target.blocks[0].instructions = vec![Instruction {
            result: Some(ValueId(0)),
            kind: InstructionKind::Load(LocalId(0)),
            span,
        }];
        target.blocks[0].terminator = Terminator::Return(Some(ValueId(0)));
        let mut caller = target.clone();
        caller.id = FunctionId(1);
        caller.parameters.clear();
        caller.values = vec![Type::I32; 2];
        caller.blocks[0].instructions = vec![
            Instruction {
                result: Some(ValueId(0)),
                kind: InstructionKind::Constant(Constant::I32(4)),
                span,
            },
            Instruction {
                result: Some(ValueId(1)),
                kind: InstructionKind::SuspendCall {
                    function: FunctionId(0),
                    arguments: vec![ValueId(0)],
                },
                span,
            },
        ];
        caller.blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
        module.functions.push(caller);
        let check = |module: &TypedModule| {
            qualifications(
                module,
                &Limits::default(),
                None,
                &mut Budget::new(&Limits::default()),
            )
        };
        assert!(check(&module).is_ok());
        let mut forged = module.clone();
        forged.functions[1].call_kind = CallKind::Direct;
        assert!(check(&forged).is_err());
        forged = module.clone();
        forged.functions[0].call_kind = CallKind::Direct;
        assert!(check(&forged).is_err());
        forged = module.clone();
        forged.functions[1].values[0] = Type::Bool;
        assert!(check(&forged).is_err());
        forged = module.clone();
        forged.functions[0].values.push(Type::I32);
        forged.functions[0].blocks[0]
            .instructions
            .push(Instruction {
                result: Some(ValueId(1)),
                kind: InstructionKind::SuspendCall {
                    function: FunctionId(0),
                    arguments: vec![ValueId(0)],
                },
                span,
            });
        assert!(
            check(&forged).is_err(),
            "a recursive child frame is not closed finite storage"
        );
    }

    #[test]
    fn direct_root_upper_and_complete_class_relation_both_constrain_actual_returns() {
        let mut base = fixture::module();
        let mut f = base.functions[4].clone();
        f.id = FunctionId(0);
        let mut second = f.locals[0].clone();
        second.id = LocalId(1);
        second.name = "second".into();
        f.locals.push(second);
        f.parameters.push(LocalId(1));
        f.reference_return_bound = Some(vec![0, 1]);
        f.storage_life = Some(StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: 1,
            bindings: [
                StorageBase::Parameter(0),
                StorageBase::Parameter(1),
                StorageBase::Result,
            ]
            .into_iter()
            .map(|base| StorageLifeBinding {
                position: StoragePosition { base, path: vec![] },
                binder: StorageBinder::Invocation(0),
            })
            .collect(),
            static_bindings: vec![],
            classes: vec![],
        });
        base.functions = vec![f];
        base.life_declarations.clear();
        base.life_instances.clear();
        for attack in 0..4 {
            let mut m = base.clone();
            match attack {
                0 => {}
                // Same full class, but the declared root upper omits the
                // independently reconstructed actual return Parameter(0).
                1 => m.functions[0].reference_return_bound = Some(vec![1]),
                // The root upper contains Parameter(0), but the complete
                // header requires the unrelated second input class instead.
                2 => {
                    let life = m.functions[0].storage_life.as_mut().unwrap();
                    life.invocation_binder_count = 2;
                    life.bindings[1].binder = StorageBinder::Invocation(1);
                    life.bindings[2].binder = StorageBinder::Invocation(1);
                }
                _ => m.functions[0].reference_return_bound = Some(vec![1, 0]),
            }
            let candidate = fixture::class_candidate(&m);
            let limits = Limits::default();
            let bytes = zil_ir::wire::encode_module_candidate(&m, &candidate, &limits).unwrap();
            for result in [
                super::super::verify_module(m.clone(), candidate, &m.target, &limits),
                super::super::verify_module_bytes(&bytes, &m.target, &limits),
            ] {
                if attack == 0 {
                    result.unwrap();
                    continue;
                }
                let errors = result.unwrap_err();
                let (code, message) = match attack {
                    1 => (
                        "E-VERIFY-STORED",
                        "actual returned target exceeds the existing direct reference upper",
                    ),
                    2 => (
                        "E-VERIFY-CLASS-EQUATION",
                        "actual retained class equality is not universally valid",
                    ),
                    _ => (
                        "E-VERIFY-LIFE",
                        "invalid address direct return-life upper bound",
                    ),
                };
                assert!(
                    errors
                        .iter()
                        .any(|d| d.category == zil_base::ResultCategory::SafeReject
                            && d.code == code
                            && d.message.ends_with(message)),
                    "attack {attack}: {errors:?}"
                );
            }
        }
    }

    #[test]
    fn complete_dag_transfer_matches_reverse_fixed_point_and_cycles_keep_the_fallback() {
        let module = fixture::module();
        for cyclic in [false, true] {
            let mut f = module.functions[0].clone();
            let code = std::mem::take(&mut f.blocks[0].instructions);
            let terminal = f.blocks[0].terminator.clone();
            let count = code.len();
            f.blocks = code
                .into_iter()
                .enumerate()
                .map(|(index, instruction)| BasicBlock {
                    id: BlockId(index as u32),
                    instructions: vec![instruction],
                    terminator: if index + 1 < count {
                        Terminator::Jump(BlockId(index as u32 + 1))
                    } else if cyclic {
                        Terminator::Jump(BlockId(0))
                    } else {
                        terminal.clone()
                    },
                })
                .collect();
            let mut successors = vec![vec![]; count];
            let mut predecessors = vec![vec![]; count];
            for index in 0..count {
                if index + 1 < count || cyclic {
                    let target = (index + 1) % count;
                    successors[index].push(target);
                    predecessors[target].push(index);
                }
            }
            let graph = FunctionGraph {
                successors,
                predecessors,
                reachable: vec![true; count],
                package_views: packages::CaptureViews::default(),
                role_data_uses: vec![],
            };
            let source = Source {
                static_dependency: StaticProvenance::NotStatic,
                retained_port: None,
                base: HoldRoot::Parameter(0),
                leaf: vec![],
                inline: vec![],
                heap_parents: vec![],
                heap_element: None,
            };
            let mut summaries = vec![Summary::default(); module.functions.len()];
            summaries[4].complete = true;
            summaries[4].result = vec![(vec![], vec![source])];
            let limits = Limits::default();
            let mut fast_budget = Budget::new(&limits);
            let mut reverse_budget = Budget::new(&limits);
            let fast = solve_mode(&module, &f, &graph, &summaries, &mut fast_budget, true).unwrap();
            let reverse =
                solve_mode(&module, &f, &graph, &summaries, &mut reverse_budget, false).unwrap();
            assert_eq!(fast.entry, reverse.entry);
            assert_eq!(fast.outputs, reverse.outputs);
            assert_eq!(fast.values, reverse.values);
            assert_eq!(fast.sites, reverse.sites);
            assert_eq!(fast.loans.len(), reverse.loans.len());
            for (actual, expected) in fast.loans.iter().zip(&reverse.loans) {
                assert_eq!(
                    (
                        &actual.sources,
                        &actual.parents,
                        actual.exclusive,
                        actual.receiver_head
                    ),
                    (
                        &expected.sources,
                        &expected.parents,
                        expected.exclusive,
                        expected.receiver_head
                    )
                );
            }
            if !cyclic {
                assert!(
                    fast_budget.remaining > reverse_budget.remaining,
                    "a true multi-block DAG executes fewer real transfers and frame copies"
                );
                // A real unresolved address-result producer in the
                // nonterminal Call block must still prevent completion.
                // Do not manufacture an empty pointer from a missing row.
                let mut unresolved = summaries.clone();
                unresolved[4].result.clear();
                let incomplete = solve_mode(
                    &module,
                    &f,
                    &graph,
                    &unresolved,
                    &mut Budget::new(&limits),
                    true,
                )
                .unwrap();
                assert!(incomplete.outputs[1].pending_external_update);
                assert!(
                    !summarize(
                        &module,
                        &f,
                        &graph,
                        &incomplete,
                        None,
                        &mut Budget::new(&limits)
                    )
                    .unwrap()
                    .complete
                );
            }
        }
    }

    #[test]
    fn assignment_reservations_require_source_and_nonescaping_unique_write_chains() {
        let module = fixture::module();
        assert!(module.semantic.is_none());
        let mut function = module.functions[0].clone();
        let array = Type::Array {
            element: Box::new(Type::I64),
            length: 4,
        };
        let pair = Type::Package {
            name: "Buffer".into(),
            fields: vec![("bytes".into(), array.clone())],
        };
        function.environment = None;
        function.parameters = vec![LocalId(0)];
        function.locals.truncate(1);
        function.locals[0].ty = fixture::address(pair, true);
        function.values = vec![
            function.locals[0].ty.clone(),
            fixture::address(array.clone(), true),
            Type::I64,
            fixture::address(Type::I64, true),
            Type::I64,
        ];
        function.blocks.truncate(1);
        function.blocks[0].instructions = vec![
            fixture::instruction(Some(0), InstructionKind::Load(LocalId(0))),
            fixture::instruction(
                Some(1),
                InstructionKind::AddressField {
                    address: ValueId(0),
                    index: 0,
                },
            ),
            fixture::instruction(Some(2), InstructionKind::Constant(Constant::I64(0))),
            fixture::instruction(
                Some(3),
                InstructionKind::AddressIndex {
                    address: ValueId(1),
                    index: ValueId(2),
                },
            ),
            fixture::instruction(Some(4), InstructionKind::Constant(Constant::I64(1))),
            fixture::instruction(
                None,
                InstructionKind::WriteAddress {
                    address: ValueId(3),
                    value: ValueId(4),
                },
            ),
        ];
        function.blocks[0].terminator = Terminator::Return(None);
        let limits = Limits::default();
        let raw = deferred_addresses(&module, &function, &mut Budget::new(&limits)).unwrap();
        assert!(raw[0]);
        assert!(
            !raw[1] && !raw[3],
            "raw heads are not source assignment receipts"
        );
        assert!(
            assignment_reservations(&function, &[], &mut Budget::new(&limits))
                .unwrap()
                .iter()
                .all(|reserved| !*reserved)
        );
        // Here the private kernel receives the source match already proved by
        // the separate original assignment consumer, never a raw claim.
        let marked =
            assignment_reservations(&function, &[ValueId(3)], &mut Budget::new(&limits)).unwrap();
        assert!(marked[0] && marked[1] && marked[3]);
        for escape in 0..5 {
            let mut forged = function.clone();
            match escape {
                0 => forged.blocks[0].instructions.push(fixture::instruction(
                    None,
                    InstructionKind::Store {
                        local: LocalId(0),
                        value: ValueId(3),
                    },
                )),
                1 => forged.blocks[0].instructions.push(fixture::instruction(
                    None,
                    InstructionKind::Call {
                        function: FunctionId(1),
                        arguments: vec![ValueId(3)],
                    },
                )),
                2 => forged.blocks[0].terminator = Terminator::Return(Some(ValueId(3))),
                3 => forged.blocks[0].instructions.push(fixture::instruction(
                    None,
                    InstructionKind::WriteAddress {
                        address: ValueId(3),
                        value: ValueId(4),
                    },
                )),
                _ => {
                    let mut block = forged.blocks[0].clone();
                    block.id = BlockId(1);
                    block.instructions = vec![fixture::instruction(
                        None,
                        InstructionKind::Store {
                            local: LocalId(0),
                            value: ValueId(3),
                        },
                    )];
                    forged.blocks.push(block);
                }
            }
            let marked =
                assignment_reservations(&forged, &[ValueId(3)], &mut Budget::new(&limits)).unwrap();
            assert!(!marked[0] && !marked[1] && !marked[3], "escape {escape}");
        }
        let source = Source {
            static_dependency: StaticProvenance::NotStatic,
            retained_port: None,
            base: HoldRoot::Parameter(0),
            leaf: vec![],
            inline: vec![PlaceProjection::Field(0)],
            heap_parents: vec![],
            heap_element: None,
        };
        let parent = Loan {
            receiver_head: false,
            sources: vec![source.clone()],
            parents: vec![],
            exclusive: true,
        };
        let child = Loan {
            receiver_head: false,
            sources: vec![source.clone()],
            parents: vec![0],
            exclusive: true,
        };
        let permission = Permission {
            sources: vec![source],
            loans: vec![0],
        };
        assert!(
            access(
                &permission,
                true,
                &[0, 1],
                &[parent.clone(), child],
                &mut vec![],
                &mut Budget::new(&limits)
            )
            .unwrap_err()
            .message
            .contains("loan")
        );
        access(
            &permission,
            true,
            &[0],
            &[parent],
            &mut vec![],
            &mut Budget::new(&limits),
        )
        .unwrap();
    }

    #[test]
    fn projected_head_reads_do_not_delegate_a_live_child_to_parent_accesses() {
        // Two disjoint fields may coexist. Selecting the same field, reading
        // or writing the whole parent, reborrowing it, passing it, or copying its mutable
        // capability to another local must still conflict with the live child.
        for case in 0..7 {
            let mut module = fixture::module();
            let pair = Type::Record {
                name: "Pair".into(),
                fields: vec![("left".into(), Type::I64), ("right".into(), Type::I64)],
            };
            let whole = fixture::address(pair.clone(), true);
            let field = fixture::address(Type::I64, true);
            let mut function = module.functions[0].clone();
            function.id = FunctionId(0);
            function.environment = None;
            function.parameters = vec![LocalId(0)];
            function.locals.truncate(1);
            function.locals[0].ty = whole.clone();
            function.locals[0].mutable = false;
            let mut alias = function.locals[0].clone();
            alias.id = LocalId(1);
            alias.mutable = true;
            function.locals.push(alias);
            function.return_type = Type::I64;
            function.values = vec![
                whole.clone(),
                field.clone(),
                whole.clone(),
                field,
                Type::I64,
            ];
            let mut instructions = vec![
                fixture::instruction(Some(0), InstructionKind::Load(LocalId(0))),
                fixture::instruction(
                    Some(1),
                    InstructionKind::AddressField {
                        address: ValueId(0),
                        index: 0,
                    },
                ),
            ];
            match case {
                0 | 1 => instructions.extend([
                    fixture::instruction(Some(2), InstructionKind::Load(LocalId(0))),
                    fixture::instruction(
                        Some(3),
                        InstructionKind::AddressField {
                            address: ValueId(2),
                            index: if case == 0 { 1 } else { 0 },
                        },
                    ),
                ]),
                2 => {
                    function.values[2] = pair;
                    instructions.push(fixture::instruction(
                        Some(2),
                        InstructionKind::ReadAddress {
                            address: ValueId(0),
                        },
                    ));
                }
                3 => instructions.push(fixture::instruction(
                    Some(2),
                    InstructionKind::ReborrowAddress {
                        address: ValueId(0),
                        mutable: true,
                    },
                )),
                4 => {
                    let mut callee = function.clone();
                    callee.id = FunctionId(1);
                    callee.locals.truncate(1);
                    callee.return_type = Type::Unit;
                    callee.values.clear();
                    callee.blocks.truncate(1);
                    callee.blocks[0].instructions.clear();
                    callee.blocks[0].terminator = Terminator::Return(None);
                    module.functions[1] = callee;
                    instructions.push(fixture::instruction(
                        None,
                        InstructionKind::Call {
                            function: FunctionId(1),
                            arguments: vec![ValueId(0)],
                        },
                    ));
                }
                5 => instructions.extend([
                    fixture::instruction(Some(2), InstructionKind::Load(LocalId(0))),
                    fixture::instruction(
                        None,
                        InstructionKind::Store {
                            local: LocalId(1),
                            value: ValueId(2),
                        },
                    ),
                ]),
                _ => {
                    function.values[2] = pair;
                    instructions.insert(
                        1,
                        fixture::instruction(
                            Some(2),
                            InstructionKind::ReadAddress {
                                address: ValueId(0),
                            },
                        ),
                    );
                    instructions.push(fixture::instruction(
                        None,
                        InstructionKind::WriteAddress {
                            address: ValueId(0),
                            value: ValueId(2),
                        },
                    ));
                }
            }
            instructions.push(fixture::instruction(
                Some(4),
                InstructionKind::ReadAddress {
                    address: ValueId(1),
                },
            ));
            if case <= 1 {
                instructions.push(fixture::instruction(
                    None,
                    InstructionKind::WriteAddress {
                        address: ValueId(3),
                        value: ValueId(4),
                    },
                ));
            }
            function.blocks.truncate(1);
            function.blocks[0].instructions = instructions;
            function.blocks[0].terminator = Terminator::Return(Some(ValueId(4)));
            let graph = FunctionGraph {
                successors: vec![vec![]],
                predecessors: vec![vec![]],
                reachable: vec![true],
                package_views: packages::CaptureViews::default(),
                role_data_uses: vec![],
            };
            let limits = Limits::default();
            let mut budget = Budget::new(&limits);
            let heads = projection_head_loads(&function, &mut budget).unwrap();
            assert_eq!(heads[0], case <= 1 || case == 5);
            if case <= 1 {
                assert!(heads[2]);
            }
            if case == 5 {
                assert!(!heads[2]);
            }
            let summaries = vec![Summary::default(); module.functions.len()];
            let mut solution = solve(&module, &function, &graph, &summaries, &mut budget).unwrap();
            let result = windows(
                &module,
                &function,
                &graph,
                &mut solution,
                &summaries,
                &mut vec![],
                &mut vec![],
                &mut vec![],
                &mut vec![],
                &mut vec![],
                &mut budget,
            );
            if case == 0 {
                result.unwrap();
            } else {
                assert!(result.unwrap_err().message.contains("loan"), "case {case}");
            }
        }
    }

    #[test]
    fn projected_head_load_requires_all_uses_including_unreachable_and_return() {
        let module = fixture::module();
        let mut function = module.functions[0].clone();
        function.parameters = vec![LocalId(0)];
        function.locals.truncate(1);
        let pair = Type::Record {
            name: "Pair".into(),
            fields: vec![("id".into(), Type::I64)],
        };
        function.locals[0].ty = fixture::address(pair, true);
        function.values = vec![
            function.locals[0].ty.clone(),
            fixture::address(Type::I64, true),
        ];
        function.blocks.truncate(1);
        function.blocks[0].instructions = vec![
            fixture::instruction(Some(0), InstructionKind::Load(LocalId(0))),
            fixture::instruction(
                Some(1),
                InstructionKind::AddressField {
                    address: ValueId(0),
                    index: 0,
                },
            ),
        ];
        function.blocks[0].terminator = Terminator::Return(None);
        let limits = Limits::default();
        assert!(projection_head_loads(&function, &mut Budget::new(&limits)).unwrap()[0]);
        let mut escaped = function.clone();
        escaped.blocks[0].terminator = Terminator::Return(Some(ValueId(0)));
        assert!(!projection_head_loads(&escaped, &mut Budget::new(&limits)).unwrap()[0]);
        let mut unreachable = function.blocks[0].clone();
        unreachable.id = BlockId(1);
        unreachable.instructions = vec![fixture::instruction(
            None,
            InstructionKind::Store {
                local: LocalId(0),
                value: ValueId(0),
            },
        )];
        function.blocks.push(unreachable);
        assert!(!projection_head_loads(&function, &mut Budget::new(&limits)).unwrap()[0]);
    }

    #[test]
    fn affine_pointee_and_option_actual_keep_their_distinct_domains() {
        let module = fixture::module();
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        let owner = Type::Package {
            name: "Item".into(),
            fields: vec![("id".into(), Type::I64)],
        };
        let address = fixture::address(owner.clone(), false);
        assert!(!owner.is_copy_value());
        assert!(ordinary_owner_address(&module, &address, &mut budget).unwrap());
        let mut hidden = owner;
        let Type::Package { fields, .. } = &mut hidden else {
            unreachable!()
        };
        fields.push(("borrow".into(), fixture::address(Type::I64, false)));
        assert!(
            !ordinary_owner_address(&module, &fixture::address(hidden, false), &mut budget)
                .unwrap()
        );
        let mut instance = LifeNominalInstance {
            declaration: zil_ir::builtin_life::OPTION,
            ty: Type::BorrowedStorage {
                storage: Box::new(Type::Enum {
                    name: "OptionItem".into(),
                    variants: vec![
                        ("None".into(), vec![]),
                        ("Some".into(), vec![address.clone()]),
                    ],
                }),
                life: StorageLifeShape {
                    binder_count: 1,
                    slots: vec![zil_ir::StorageLifeSlot {
                        path: vec![PlaceProjection::Variant(zil_ir::VariantProjection {
                            variant: 1,
                            index: 0,
                        })],
                        binder: 0,
                        mode: StoredBorrowMode::SharedAddress,
                    }],
                    retained: vec![zil_ir::StorageRetainedBinding {
                        path: vec![LifeTypeProjection::TypeArgument(0)],
                        class: 0,
                        binder: 0,
                    }],
                    nominal_arguments: vec![],
                },
            },
            arguments: vec![LifeNominalActual::QualifiedType {
                ty: address,
                life: StorageLifeShape {
                    binder_count: 1,
                    slots: vec![zil_ir::StorageLifeSlot {
                        path: vec![],
                        binder: 0,
                        mode: StoredBorrowMode::SharedAddress,
                    }],
                    retained: vec![],
                    nominal_arguments: vec![],
                },
            }],
        };
        option_address::instance(&module, &instance, &limits, &mut budget).unwrap();
        for wrong in 0..5 {
            let mut forged = instance.clone();
            let LifeNominalActual::QualifiedType { ty, life } = &mut forged.arguments[0] else {
                unreachable!()
            };
            match wrong {
                0 => {
                    life.binder_count = 0;
                    life.slots.clear();
                }
                1 => life.slots[0].binder = 1,
                2 => life.slots[0].mode = StoredBorrowMode::ExclusiveAddress,
                3 => life.slots[0].path.push(PlaceProjection::Field(0)),
                _ => *ty = fixture::address(Type::I64, false),
            }
            assert!(option_address::instance(&module, &forged, &limits, &mut budget).is_err());
        }
        let Type::BorrowedStorage { life, .. } = &mut instance.ty else {
            unreachable!()
        };
        life.retained[0].path = vec![LifeTypeProjection::TypeArgument(1)];
        assert!(option_address::instance(&module, &instance, &limits, &mut budget).is_err());
    }

    #[test]
    fn owning_package_cleanup_conflicts_with_its_live_field_borrow() {
        let module = fixture::module();
        for premature in [false, true] {
            let mut function = module.functions[0].clone();
            function.environment = None;
            function.parameters.clear();
            function.locals.truncate(1);
            function.locals[0].ty = Type::Package {
                name: "Item".into(),
                fields: vec![("id".into(), Type::I64)],
            };
            function.return_type = Type::I64;
            function.values = vec![
                fixture::address(function.locals[0].ty.clone(), false),
                fixture::address(Type::I64, false),
                Type::I64,
            ];
            let mut instructions = vec![
                fixture::instruction(Some(0), InstructionKind::BorrowLocal { local: LocalId(0) }),
                fixture::instruction(
                    Some(1),
                    InstructionKind::AddressField {
                        address: ValueId(0),
                        index: 0,
                    },
                ),
                fixture::instruction(
                    Some(2),
                    InstructionKind::ReadAddress {
                        address: ValueId(1),
                    },
                ),
            ];
            instructions.insert(
                if premature { 2 } else { 3 },
                fixture::instruction(None, InstructionKind::DropPackage { local: LocalId(0) }),
            );
            function.blocks.truncate(1);
            function.blocks[0].instructions = instructions;
            function.blocks[0].terminator = Terminator::Return(Some(ValueId(2)));
            let graph = FunctionGraph {
                successors: vec![vec![]],
                predecessors: vec![vec![]],
                reachable: vec![true],
                package_views: packages::CaptureViews::default(),
                role_data_uses: vec![],
            };
            let limits = Limits::default();
            let mut budget = Budget::new(&limits);
            let mut solution = solve(&module, &function, &graph, &[], &mut budget).unwrap();
            let result = windows(
                &module,
                &function,
                &graph,
                &mut solution,
                &[],
                &mut vec![],
                &mut vec![],
                &mut vec![],
                &mut vec![],
                &mut vec![],
                &mut budget,
            );
            if premature {
                assert!(result.unwrap_err().message.contains("loan"));
            } else {
                result.unwrap();
            }
        }
    }
    #[test]
    fn optional_byte_addresses_reject_relocation_in_actual_transfer() {
        let module = fixture::module();
        let mut function = module.functions[0].clone();
        let selector = PlaceProjection::Variant(zil_ir::VariantProjection {
            variant: 1,
            index: 0,
        });
        let qualified = |inner| Type::BorrowedStorage {
            storage: Box::new(Type::Enum {
                name: "OptionalAddress".into(),
                variants: vec![
                    ("None".into(), vec![]),
                    (
                        "Some".into(),
                        vec![Type::Address {
                            mutable: false,
                            inner: Box::new(inner),
                        }],
                    ),
                ],
            }),
            life: StorageLifeShape {
                binder_count: 1,
                slots: vec![zil_ir::StorageLifeSlot {
                    path: vec![selector],
                    binder: 0,
                    mode: StoredBorrowMode::SharedAddress,
                }],
                retained: vec![],
                nominal_arguments: vec![],
            },
        };
        let values = vec![Payload::default(); 4];
        let sites = WriteSites::new();
        for reserve in [false, true] {
            for byte in [false, true] {
                let pointee = if byte {
                    Type::U8
                } else {
                    Type::Package {
                        name: "Item".into(),
                        fields: vec![("data".into(), Type::Managed(Box::new(Type::ByteBuffer)))],
                    }
                };
                function.locals.clear();
                function.values = vec![
                    Type::Desc(Box::new(Type::ByteBuffer)),
                    if reserve { Type::I64 } else { Type::U8 },
                    Type::Bool,
                    qualified(pointee),
                ];
                let kind = if reserve {
                    InstructionKind::ByteBufferReserve {
                        buffer: ValueId(0),
                        capacity: ValueId(1),
                    }
                } else {
                    InstructionKind::ByteBufferPush {
                        buffer: ValueId(0),
                        value: ValueId(1),
                    }
                };
                let instruction = fixture::instruction(Some(2), kind);
                let limits = Limits::default();
                let mut budget = Budget::new(&limits);
                let result = step(
                    &module,
                    &function,
                    &instruction,
                    &mut Frame::default(),
                    &values,
                    &[],
                    &sites,
                    &mut dyn_leases::Sites::default(),
                    &mut Vec::new(),
                    false,
                    &mut budget,
                );
                if byte {
                    let error = result.unwrap_err();
                    assert_eq!(error.category, zil_base::ResultCategory::Unsupported);
                    assert!(error.message.contains(
                        "ByteBuffer relocation needs projected stored byte target invalidation"
                    ));
                } else {
                    result.unwrap();
                }
            }
        }
        // The same transfer must reject a forged payload selector before it
        // could classify the path as a harmless non-byte address.
        function.values[3] = qualified(Type::I64);
        let Type::BorrowedStorage { life, .. } = &mut function.values[3] else {
            unreachable!()
        };
        life.slots[0].path = vec![PlaceProjection::Variant(zil_ir::VariantProjection {
            variant: 1,
            index: u32::MAX,
        })];
        let limits = Limits::default();
        let error = step(
            &module,
            &function,
            &fixture::instruction(
                Some(2),
                InstructionKind::ByteBufferReserve {
                    buffer: ValueId(0),
                    capacity: ValueId(1),
                },
            ),
            &mut Frame::default(),
            &values,
            &[],
            &sites,
            &mut dyn_leases::Sites::default(),
            &mut Vec::new(),
            false,
            &mut Budget::new(&limits),
        )
        .unwrap_err();
        assert_eq!(error.category, zil_base::ResultCategory::SafeReject);
        assert!(error.message.contains("stored variant payload is absent"));
    }
    // These fixtures have one straight-line block. The candidate supplies only
    // root identities; all paths, loans and environment class bindings remain
    // independently reconstructed by the verifier.
    fn captured_candidate(m: &TypedModule) -> CandidatePlan {
        let mut c = fixture::class_candidate(m);
        for (f, plan) in m.functions.iter().zip(&mut c.functions) {
            let mut locals: Vec<Vec<HoldRoot>> = f.locals.iter().map(|_| vec![]).collect();
            for (index, local) in f.parameters.iter().enumerate() {
                locals[local.0 as usize] = vec![HoldRoot::Parameter(index as u32)];
            }
            let mut initialized = f.parameters.clone();
            for i in &f.blocks[0].instructions {
                let roots = match &i.kind {
                    InstructionKind::Load(l) | InstructionKind::TakeLocal { local: l } => {
                        locals[l.0 as usize].clone()
                    }
                    InstructionKind::ReadCapture { index } => vec![HoldRoot::Capture(*index)],
                    InstructionKind::DescribeLocal { local } => vec![HoldRoot::Local(*local)],
                    InstructionKind::DescribeCapture { index } => vec![HoldRoot::Capture(*index)],
                    InstructionKind::BorrowLocal { local }
                    | InstructionKind::BorrowLocalMut { local } => {
                        let mut roots = vec![HoldRoot::Local(*local)];
                        if matches!(
                            f.locals[local.0 as usize].ty,
                            Type::BorrowedStorage { .. } | Type::Closure(_)
                        ) {
                            roots.extend(locals[local.0 as usize].clone());
                        }
                        roots
                    }
                    InstructionKind::MakeClosure { captures, .. }
                    | InstructionKind::MakeAggregate { fields: captures } => captures
                        .iter()
                        .flat_map(|v| plan.holds[v.0 as usize].roots.clone())
                        .collect(),
                    InstructionKind::LoadStoredBorrow { storage, .. } => {
                        plan.holds[storage.0 as usize].roots.clone()
                    }
                    InstructionKind::ReborrowAddress { address, .. } => {
                        plan.holds[address.0 as usize].roots.clone()
                    }
                    _ => vec![],
                };
                if let Some(v) = i.result {
                    let mut roots = roots;
                    // Ordinary resource descriptions publish no strong Hold.
                    // Their actual owner head, epoch and source stay private
                    // and are still exercised by the retirement attacks.
                    if matches!(&f.values[v.0 as usize], Type::Desc(payload)
                        if !matches!(payload.as_ref(), Type::ByteBuffer | Type::Task { .. } | Type::TaskI32 | Type::OwnedStore { .. } | Type::Mutex { .. }))
                    {
                        roots.clear();
                    }
                    roots.sort();
                    roots.dedup();
                    plan.holds[v.0 as usize].roots = roots;
                }
                match &i.kind {
                    InstructionKind::Store { local, value } => {
                        locals[local.0 as usize] = plan.holds[value.0 as usize].roots.clone();
                        if !initialized.contains(local) {
                            initialized.push(*local);
                        }
                    }
                    InstructionKind::StorageDead(l) | InstructionKind::TakeLocal { local: l } => {
                        initialized.retain(|x| x != l)
                    }
                    _ => {}
                }
            }
            // Complete the fixture's private origin propagation first. Only
            // actual weak Desc SSA rows lose their public strong-Hold claim.
            for (value, ty) in f.values.iter().enumerate() {
                if matches!(ty, Type::Desc(_)) {
                    plan.holds[value].roots.clear();
                }
            }
            initialized.sort();
            plan.blocks[0].initialized_out = initialized;
        }
        c
    }
    #[test]
    fn capture_grounded_fixed_actual_environments_raw_and_wire() {
        for kind in [
            zil_ir::CallableKind::Fn,
            zil_ir::CallableKind::FnMut,
            zil_ir::CallableKind::FnOnce,
        ] {
            for make in [
                fixture::captured_fixed,
                fixture::captured_fixed_instances,
                fixture::captured_fixed_helper,
            ] {
                for wrong in [false, true] {
                    let m = make(kind, wrong);
                    let c = captured_candidate(&m);
                    let bytes =
                        zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
                    for result in [
                        super::super::verify_module(m.clone(), c, &m.target, &Limits::default()),
                        super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()),
                    ] {
                        if wrong {
                            let error = result.unwrap_err();
                            assert!(
                                error.iter().any(|d| d.message.contains("class equality")),
                                "{kind:?}: {error:?}"
                            );
                        } else {
                            result.unwrap_or_else(|error| panic!("{kind:?}: {error:?}"));
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn capture_grounded_fixed_parent_target_keeps_real_loan() {
        for dead in [false, true] {
            let m = fixture::captured_fixed_parent_end(dead);
            let c = captured_candidate(&m);
            let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
            for result in [
                super::super::verify_module(m.clone(), c, &m.target, &Limits::default()),
                super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()),
            ] {
                if dead {
                    let error = result.unwrap_err();
                    assert!(
                        error
                            .iter()
                            .any(|d| d.message.contains("live incompatible loan")),
                        "{error:?}"
                    );
                } else {
                    result.unwrap();
                }
            }
        }
    }
    #[test]
    fn capture_grounded_fixed_malformed_contracts_and_actual_role_reject() {
        for variant in 0..9 {
            let mut m = fixture::captured_fixed_helper(zil_ir::CallableKind::Fn, false);
            match variant {
                0 => {
                    m.closures[0].storage_life.as_mut().unwrap().classes[1].binder =
                        StorageBinder::Fixed(1)
                }
                1 => {
                    let d = m.closures[0].storage_life.as_mut().unwrap();
                    d.bindings[1].binder = StorageBinder::Invocation(0);
                    m.functions[0].storage_life = Some(d.clone());
                }
                2 => {
                    let role = m.functions[2].callable_contract.as_mut().unwrap().roles[0]
                        .life
                        .as_mut()
                        .unwrap();
                    role.bindings[1].binder = StorageBinder::Fixed(0);
                    role.classes[1].binder = StorageBinder::Fixed(1);
                }
                3 => m.functions[2].storage_life = None,
                4 => {
                    // Same physical types, but actual helper argument is the
                    // other header class. A declared role is not permission.
                    if let InstructionKind::Call { arguments, .. } = &mut m.functions[1].blocks[0]
                        .instructions
                        .last_mut()
                        .unwrap()
                        .kind
                    {
                        arguments[1] = ValueId(1);
                    }
                }
                5 => {
                    // Moving a receiver preserves its fixed domain. A helper
                    // cannot choose another parameter's class after TakeLocal.
                    m = fixture::captured_fixed_helper(zil_ir::CallableKind::FnOnce, false);
                    let helper = &mut m.functions[2];
                    let tag = helper.locals[1].ty.clone();
                    let mut local = helper.locals[1].clone();
                    local.id = LocalId(2);
                    helper.locals.push(local);
                    helper.parameters.push(LocalId(2));
                    helper.values.push(tag);
                    let c = helper.storage_life.as_mut().unwrap();
                    c.fixed_binder_count += 1;
                    c.invocation_binder_count += 1;
                    c.bindings.push(StorageLifeBinding {
                        position: StoragePosition {
                            base: StorageBase::Parameter(2),
                            path: vec![],
                        },
                        binder: StorageBinder::Invocation(1),
                    });
                    let mut row = c.classes.last().unwrap().clone();
                    row.position.base = StorageBase::Parameter(2);
                    row.binder = StorageBinder::Fixed(2);
                    c.classes.push(row);
                    helper.blocks[0].instructions.insert(
                        2,
                        Instruction {
                            result: Some(ValueId(3)),
                            kind: InstructionKind::Load(LocalId(2)),
                            span: helper.span,
                        },
                    );
                    if let InstructionKind::CallClosureOnce { arguments, .. } =
                        &mut helper.blocks[0].instructions.last_mut().unwrap().kind
                    {
                        arguments[0] = ValueId(3);
                    }
                    if let InstructionKind::Call { arguments, .. } = &mut m.functions[1].blocks[0]
                        .instructions
                        .last_mut()
                        .unwrap()
                        .kind
                    {
                        arguments.push(ValueId(1));
                    }
                }
                6 => {
                    m = fixture::captured_fixed(zil_ir::CallableKind::Fn, false);
                    m.closures[0].kind = zil_ir::CallableKind::Fn;
                    m.functions[1].blocks[0]
                        .instructions
                        .last_mut()
                        .unwrap()
                        .kind = InstructionKind::CallClosureOnce {
                        callee: ValueId(2),
                        arguments: vec![ValueId(0)],
                    };
                }
                7 => {
                    // A return of the actual environment cannot erase its
                    // held targets through an ordinary function result.
                    m = fixture::captured_fixed(zil_ir::CallableKind::Fn, false);
                    m.functions[1].return_type = Type::Closure(zil_ir::ClosureId(0));
                    m.functions[1].blocks[0].terminator = Terminator::Return(Some(ValueId(2)));
                }
                _ => {
                    // Extra rows must not smuggle a second Capture occurrence.
                    let d = m.closures[0].storage_life.as_mut().unwrap();
                    d.classes.push(d.classes[1].clone());
                    m.functions[0].storage_life = Some(d.clone());
                }
            }
            let c = captured_candidate(&m);
            let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
            for result in [
                super::super::verify_module(m.clone(), c, &m.target, &Limits::default()),
                super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()),
            ] {
                if variant == 6 {
                    // The actual finite nonlending Fn entry may be consumed
                    // through Once. Its environment still settles once.
                    result.unwrap();
                    continue;
                }
                let error = result.unwrap_err();
                let expected = match variant {
                    0 => "entry and definition relation differ",
                    1 | 7 | 8 => "signature class relation",
                    2 => "callable role changes an actual persistent class or constant occurrence",
                    3 => "requires complete class relation",
                    4 | 5 => "class equality",
                    _ => unreachable!(),
                };
                assert!(
                    error.iter().any(|d| d.message.contains(expected)),
                    "variant{variant}: {error:?}"
                );
                if variant == 2 {
                    assert!(
                        error
                            .iter()
                            .any(|d| d.category == zil_base::ResultCategory::SafeReject
                                && d.code == "E-VERIFY-SIGNATURE-CLASS"),
                        "variant{variant}: {error:?}"
                    );
                }
                if matches!(variant, 4 | 5) {
                    assert!(
                        error.iter().any(|d| d.message.contains("class equality")),
                        "variant{variant}: {error:?}"
                    );
                }
            }
        }
    }
    #[test]
    fn capture_grounded_fixed_reborrow_scalar_mutation_and_invocations() {
        // Reborrowed capture must recover physical stored field targets from
        // the actual address, not rely on a copied SSA leaf cache.
        let mut reborrow = fixture::captured_fixed(zil_ir::CallableKind::Fn, false);
        let caller = &mut reborrow.functions[1];
        caller.values.push(caller.values[0].clone());
        caller.blocks[0].instructions.insert(
            2,
            Instruction {
                result: Some(ValueId(4)),
                kind: InstructionKind::ReborrowAddress {
                    address: ValueId(0),
                    mutable: false,
                },
                span: caller.span,
            },
        );
        if let InstructionKind::MakeClosure { captures, .. } =
            &mut caller.blocks[0].instructions[3].kind
        {
            captures[0] = ValueId(4);
        }
        // Scalar FnMut environment writes remain ordinary exclusive access.
        let mut mutation = fixture::captured_fixed(zil_ir::CallableKind::FnMut, false);
        mutation.closures[0].captures.push(Type::I64);
        let caller = &mut mutation.functions[1];
        caller.values.push(Type::I64);
        caller.blocks[0].instructions.insert(
            2,
            Instruction {
                result: Some(ValueId(4)),
                kind: InstructionKind::Constant(Constant::I64(0)),
                span: caller.span,
            },
        );
        if let InstructionKind::MakeClosure { captures, .. } =
            &mut caller.blocks[0].instructions[3].kind
        {
            captures.push(ValueId(4));
        }
        let entry = &mut mutation.functions[0];
        entry.blocks[0].instructions.insert(
            3,
            Instruction {
                result: None,
                kind: InstructionKind::WriteCapture {
                    index: 1,
                    value: ValueId(2),
                },
                span: entry.span,
            },
        );
        // Different outer invocation borrows can share a genuine inner class.
        let mut repeated = fixture::captured_fixed(zil_ir::CallableKind::Fn, false);
        let caller = &mut repeated.functions[1];
        let header = caller.storage_life.as_mut().unwrap();
        header.fixed_binder_count = 1;
        header.classes[1].binder = StorageBinder::Fixed(0);
        caller.values.push(Type::I64);
        caller.blocks[0].instructions.push(Instruction {
            result: Some(ValueId(4)),
            kind: InstructionKind::CallClosureShared {
                callee: LocalId(2),
                arguments: vec![ValueId(1)],
            },
            span: caller.span,
        });
        caller.blocks[0].terminator = Terminator::Return(Some(ValueId(4)));
        // Outer capture Address alone grounds a Fixed, without a nominal
        // qualification. The same real target/loan reconstruction still runs.
        let mut ordinary = fixture::captured_fixed(zil_ir::CallableKind::Fn, false);
        let shared = Type::Address {
            mutable: false,
            inner: Box::new(Type::I64),
        };
        ordinary.closures[0].captures[0] = shared.clone();
        ordinary.closures[0].parameters[0] = shared.clone();
        let life = ordinary.closures[0].storage_life.as_mut().unwrap();
        life.fixed_binder_count = 1;
        life.classes.clear();
        life.bindings[1].binder = StorageBinder::Fixed(0);
        let entry = &mut ordinary.functions[0];
        entry.locals[0].ty = shared.clone();
        entry.values[0] = shared.clone();
        entry.values[3] = shared.clone();
        entry.storage_life = Some(life.clone());
        entry.blocks[0].instructions[1].kind = InstructionKind::ReborrowAddress {
            address: ValueId(0),
            mutable: false,
        };
        entry.blocks[0].instructions[4].kind = InstructionKind::ReborrowAddress {
            address: ValueId(3),
            mutable: false,
        };
        let caller = &mut ordinary.functions[1];
        for index in 0..2 {
            caller.locals[index].ty = shared.clone();
            caller.values[index] = shared.clone();
        }
        let header = caller.storage_life.as_mut().unwrap();
        header.fixed_binder_count = 0;
        header.classes.clear();
        for m in [reborrow, mutation, repeated, ordinary] {
            let c = captured_candidate(&m);
            let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
            super::super::verify_module(m.clone(), c, &m.target, &Limits::default()).unwrap();
            super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()).unwrap();
        }
    }
    #[test]
    fn capture_grounded_fixed_uses_one_request_budget_exactly() {
        let m = fixture::captured_fixed_helper(zil_ir::CallableKind::FnMut, false);
        let c = captured_candidate(&m);
        let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
        let check = |n, wire| {
            let limits = Limits {
                max_analysis_steps: n,
                ..Limits::default()
            };
            if wire {
                super::super::verify_module_bytes(&bytes, &m.target, &limits)
            } else {
                super::super::verify_module(m.clone(), c.clone(), &m.target, &limits)
            }
        };
        for wire in [false, true] {
            let expected = check(Limits::default().max_analysis_steps, wire)
                .unwrap()
                .fingerprint();
            let mut low = 0;
            let mut high = Limits::default().max_analysis_steps;
            while low < high {
                let mid = low + (high - low) / 2;
                if check(mid, wire).is_ok() {
                    high = mid;
                } else {
                    low = mid + 1;
                }
            }
            let error = check(low - 1, wire).unwrap_err();
            assert!(
                error.iter().any(|d| d.code == "E-RESOURCE-LIMIT"),
                "{error:?}"
            );
            assert_eq!(check(low, wire).unwrap().fingerprint(), expected);
            assert_eq!(check(low, wire).unwrap().fingerprint(), expected);
            assert!(check(low - 1, wire).is_err());
            assert_eq!(check(low, wire).unwrap().fingerprint(), expected);
            println!("capture helper Verify wire={wire} N={low}; N-1/fresh/warm/retry passed");
        }
    }
    #[test]
    fn capture_grounded_fixed_candidate_cannot_replace_actual_capture_roots() {
        let m = fixture::captured_fixed(zil_ir::CallableKind::Fn, false);
        for forged in [vec![], vec![HoldRoot::Parameter(1)]] {
            let mut c = captured_candidate(&m);
            c.functions[1].holds[2].roots = forged;
            let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
            for result in [
                super::super::verify_module(m.clone(), c, &m.target, &Limits::default()),
                super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()),
            ] {
                let error = result.unwrap_err();
                assert!(
                    error.iter().any(|d| d.code == "E-VERIFY-STORED"
                        && d.message
                            .contains("independently reconstructed stored target roots")),
                    "{error:?}"
                );
            }
        }
    }
    #[test]
    fn capture_grounded_fixed_once_still_consumes_actual_owner() {
        for duplicate_call in [true, false] {
            let mut m = fixture::captured_fixed(zil_ir::CallableKind::FnOnce, false);
            let f = &mut m.functions[1];
            if duplicate_call {
                f.values.push(Type::I64);
                f.blocks[0].instructions.push(Instruction {
                    result: Some(ValueId(4)),
                    kind: InstructionKind::CallClosureOnce {
                        callee: ValueId(2),
                        arguments: vec![ValueId(0)],
                    },
                    span: f.span,
                });
                f.blocks[0].terminator = Terminator::Return(Some(ValueId(4)));
            } else {
                f.values.push(Type::Closure(zil_ir::ClosureId(0)));
                f.blocks[0].instructions.insert(
                    3,
                    Instruction {
                        result: None,
                        kind: InstructionKind::Store {
                            local: LocalId(2),
                            value: ValueId(2),
                        },
                        span: f.span,
                    },
                );
                f.blocks[0].instructions.insert(
                    4,
                    Instruction {
                        result: Some(ValueId(4)),
                        kind: InstructionKind::Load(LocalId(2)),
                        span: f.span,
                    },
                );
                if let InstructionKind::CallClosureOnce { callee, .. } =
                    &mut f.blocks[0].instructions.last_mut().unwrap().kind
                {
                    *callee = ValueId(4);
                }
            }
            let c = captured_candidate(&m);
            let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
            for result in [
                super::super::verify_module(m.clone(), c, &m.target, &Limits::default()),
                super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()),
            ] {
                let error = result.unwrap_err();
                assert!(
                    error.iter().any(|d| d.code == "E-VERIFY-RESOURCE"),
                    "{error:?}"
                );
            }
        }
    }
    #[test]
    fn capture_grounded_fixed_does_not_guess_local_phantom_classes() {
        let mut m = fixture::captured_fixed_parent_end(false);
        let Type::BorrowedStorage { storage, life } = &mut m.life_instances[0].ty else {
            unreachable!()
        };
        **storage = Type::Record {
            name: "View".into(),
            fields: vec![("number".into(), Type::I64)],
        };
        life.slots.clear();
        let phantom = m.life_instances[0].ty.clone();
        let receiver = Type::Address {
            mutable: false,
            inner: Box::new(phantom.clone()),
        };
        m.closures[0].captures[0] = receiver.clone();
        m.closures[0].parameters[0] = receiver.clone();
        let entry = &mut m.functions[0];
        entry.locals[0].ty = receiver.clone();
        entry.values = vec![Type::I64];
        entry.blocks[0].instructions = vec![fixture::instruction(
            Some(0),
            InstructionKind::Constant(Constant::I64(0)),
        )];
        entry.blocks[0].terminator = Terminator::Return(Some(ValueId(0)));
        let caller = &mut m.functions[1];
        caller.locals[1].ty = phantom.clone();
        caller.values[2] = phantom;
        caller.values[3] = receiver;
        if let InstructionKind::MakeAggregate { fields } =
            &mut caller.blocks[0].instructions[3].kind
        {
            fields[0] = ValueId(0);
        }
        let c = captured_candidate(&m);
        let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
        for result in [
            super::super::verify_module(m.clone(), c, &m.target, &Limits::default()),
            super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()),
        ] {
            let error = result.unwrap_err();
            assert!(
                error.iter().any(|d| d
                    .message
                    .contains("no independently established source class")),
                "{error:?}"
            );
        }
    }
    #[test]
    fn owning_fixed_true_once_has_real_description_roots_and_resource_check() {
        for wrong in [false, true] {
            for relay in [false, true] {
                let mut m = fixture::captured_owned_fixed(wrong);
                if relay {
                    let f = &mut m.functions[1];
                    let owner = f.values[5].clone();
                    let id = f.values.len() as u32;
                    f.values.push(owner.clone());
                    let local = LocalId(f.locals.len() as u32);
                    f.locals.push(LocalDecl {
                        id: local,
                        name: "mark".into(),
                        ty: owner,
                        mutable: false,
                        span: f.span,
                    });
                    let at = f.blocks[0]
                        .instructions
                        .iter()
                        .position(|i| matches!(i.kind, InstructionKind::MakeClosure { .. }))
                        .unwrap();
                    f.blocks[0].instructions.splice(
                        at..at,
                        [
                            fixture::instruction(
                                None,
                                InstructionKind::Store {
                                    local,
                                    value: ValueId(5),
                                },
                            ),
                            fixture::instruction(Some(id), InstructionKind::TakeLocal { local }),
                        ],
                    );
                    if let InstructionKind::MakeClosure { captures, .. } =
                        &mut f.blocks[0].instructions[at + 2].kind
                    {
                        captures[0] = ValueId(id);
                    }
                }
                let c = captured_candidate(&m);
                let bytes =
                    zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
                for result in [
                    super::super::verify_module(m.clone(), c, &m.target, &Limits::default()),
                    super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()),
                ] {
                    if wrong {
                        let error = result.unwrap_err();
                        assert!(
                            error.iter().any(|d| d.message.contains("class equality")),
                            "{error:?}"
                        );
                    } else {
                        result.unwrap();
                    }
                }
            }
        }
    }
    #[test]
    fn owning_fixed_complete_unused_definitions_and_owner_heads_keep_real_ledger_checks() {
        for unused_head in [false, true] {
            let mut m = fixture::captured_owned_fixed(false);
            if unused_head {
                let f = &mut m.functions[0];
                let id = f.values.len() as u32;
                f.values.push(Type::Address {
                    mutable: false,
                    inner: Box::new(f.values[7].clone()),
                });
                // The actual owner has been taken and stored by these two
                // instructions. This unused head has no pointee access and no
                // escape; the later owner cleanup is independently checked.
                f.blocks[0].instructions.insert(
                    2,
                    fixture::instruction(
                        Some(id),
                        InstructionKind::BorrowLocal { local: LocalId(1) },
                    ),
                );
            } else {
                // A complete class-free resource definition need not be used
                // by this closure. It supplies no cleanup or capture authority.
                m.resources.push(ResourceDefinition {
                    name: "Other".into(),
                    payload: Type::Record {
                        name: "Other".into(),
                        fields: vec![("n".into(), Type::I32)],
                    },
                    teardown: None,
                });
            }
            let c = captured_candidate(&m);
            let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
            for result in [
                super::super::verify_module(m.clone(), c, &m.target, &Limits::default()),
                super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()),
            ] {
                result.unwrap();
            }
        }
    }
    #[test]
    fn owning_fixed_actual_owner_epochs_and_metadata_never_come_from_classes() {
        for case in 0..14 {
            let mut m = fixture::captured_owned_fixed(false);
            match case {
                0 => {
                    let f = &mut m.functions[0];
                    let id = f.values.len() as u32;
                    f.values.push(f.values[7].clone());
                    f.blocks[0].instructions.insert(
                        1,
                        fixture::instruction(Some(id), InstructionKind::TakeCapture { index: 0 }),
                    );
                }
                1 => {
                    let f = &mut m.functions[1];
                    let id = f.values.len() as u32;
                    f.values.push(Type::I64);
                    f.blocks[0].instructions.push(fixture::instruction(
                        Some(id),
                        InstructionKind::CallClosureOnce {
                            callee: ValueId(2),
                            arguments: vec![ValueId(0)],
                        },
                    ));
                    f.blocks[0].terminator = Terminator::Return(Some(ValueId(id)));
                }
                2 | 3 => {
                    let code = &mut m.functions[0].blocks[0].instructions;
                    let drop = code
                        .iter()
                        .position(|i| matches!(i.kind, InstructionKind::DropManaged { .. }))
                        .unwrap();
                    let i = code.remove(drop);
                    let at = code
                        .iter()
                        .position(|i| {
                            if case == 2 {
                                matches!(i.kind, InstructionKind::ReadDesc { .. })
                            } else {
                                matches!(i.kind, InstructionKind::DescribeLocal { .. })
                            }
                        })
                        .unwrap();
                    code.insert(at, i);
                }
                4 => {
                    let f = &mut m.functions[1];
                    if let InstructionKind::MakeManaged { fields } = &mut f.blocks[0]
                        .instructions
                        .iter_mut()
                        .find(|i| matches!(i.kind, InstructionKind::MakeManaged { .. }))
                        .unwrap()
                        .kind
                    {
                        fields[0] = ValueId(0);
                    }
                }
                5 => {
                    m.functions[0].values[8] = m.functions[0].values[7].clone();
                }
                6 => {
                    let f = &mut m.functions[0];
                    if let InstructionKind::ReadDesc { index, .. } = &mut f.blocks[0]
                        .instructions
                        .iter_mut()
                        .find(|i| matches!(i.kind, InstructionKind::ReadDesc { .. }))
                        .unwrap()
                        .kind
                    {
                        *index = 99;
                    }
                }
                7 => {
                    let f = &mut m.functions[0];
                    let id = f.values.len() as u32;
                    f.values.push(Type::I32);
                    f.blocks[0]
                        .instructions
                        .iter_mut()
                        .find(|i| matches!(i.kind, InstructionKind::DropManaged { .. }))
                        .unwrap()
                        .result = Some(ValueId(id));
                }
                8 => {
                    // Extra complete declarations are legal (near control
                    // above); duplicating a sealed identity is not.
                    m.resources.push(m.resources[0].clone());
                }
                9 => {
                    let f = &mut m.functions[0];
                    let id = f.values.len() as u32;
                    f.values.push(Type::Address {
                        mutable: false,
                        inner: Box::new(f.values[7].clone()),
                    });
                    // Keep the same claimed owner-head type while selecting
                    // the scalar parameter local instead of the owner local.
                    f.blocks[0].instructions.insert(
                        2,
                        fixture::instruction(
                            Some(id),
                            InstructionKind::BorrowLocal { local: LocalId(0) },
                        ),
                    );
                }
                10 => {
                    m.closures[0].kind = zil_ir::CallableKind::Fn;
                }
                11 => {
                    m.resources[0].name = "Other".into();
                    if let Type::Record { name, .. } = &mut m.resources[0].payload {
                        *name = "Other".into();
                    }
                }
                12 => {
                    m.resources.clear();
                }
                _ => {
                    m.resources[0].teardown = Some(FunctionId(0));
                }
            }
            let c = captured_candidate(&m);
            let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
            for result in [
                super::super::verify_module(m.clone(), c, &m.target, &Limits::default()),
                super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()),
            ] {
                let error = result.expect_err(&format!("hostile owner case {case} must fail"));
                if case <= 2 || case == 8 {
                    assert!(
                        error.iter().any(|d| d.code == "E-VERIFY-RESOURCE"),
                        "case{case}: {error:?}"
                    );
                } else if case == 3 {
                    assert!(
                        error.iter().any(|d| d.code == "E-VERIFY-INITIALIZATION"),
                        "case{case}: {error:?}"
                    );
                } else if case <= 7 {
                    assert!(
                        error.iter().any(|d| d.code == "E-VERIFY-STORED-OWNER"),
                        "case{case}: {error:?}"
                    );
                } else if case == 9 {
                    assert!(error.iter().any(|d| d.category == zil_base::ResultCategory::SafeReject
                        && d.code == "E-VERIFY-TYPE"
                        && d.message == "borrow result differs from its exact actual local type or mode"),
                        "case{case}: {error:?}");
                } else if case == 10 {
                    assert!(
                        error.iter().any(|d| d.code == "E-VERIFY-STORED-OWNER"
                            && d.message.contains("actual consuming FnOnce entry")),
                        "case{case}: {error:?}"
                    );
                } else if case >= 11 {
                    assert!(
                        error.iter().any(|d| d.code == "E-VERIFY-RESOURCE"),
                        "case{case}: {error:?}"
                    );
                } else {
                    assert!(
                        error
                            .iter()
                            .any(|d| d.category == zil_base::ResultCategory::Unsupported),
                        "case{case}: {error:?}"
                    );
                }
            }
        }
    }
    #[test]
    fn owning_fixed_owner_work_is_prepaid_and_actual_description_claims_are_checked() {
        let m = fixture::captured_owned_fixed(false);
        let c = captured_candidate(&m);
        let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
        let check = |work, wire| {
            let limits = Limits {
                max_analysis_steps: work,
                ..Limits::default()
            };
            if wire {
                super::super::verify_module_bytes(&bytes, &m.target, &limits)
            } else {
                super::super::verify_module(m.clone(), c.clone(), &m.target, &limits)
            }
        };
        for wire in [false, true] {
            let expected = check(Limits::default().max_analysis_steps, wire)
                .unwrap()
                .fingerprint();
            let (mut low, mut high) = (1, Limits::default().max_analysis_steps);
            while low < high {
                let mid = low + (high - low) / 2;
                if check(mid, wire).is_ok() {
                    high = mid
                } else {
                    low = mid + 1
                }
            }
            let error = check(low - 1, wire).unwrap_err();
            assert!(
                error.iter().any(|d| d.code == "E-RESOURCE-LIMIT"),
                "{error:?}"
            );
            assert_eq!(check(low, wire).unwrap().fingerprint(), expected);
            assert_eq!(check(low, wire).unwrap().fingerprint(), expected);
            assert!(check(low - 1, wire).is_err());
            assert_eq!(check(low, wire).unwrap().fingerprint(), expected);
            println!("owning Fixed Verify wire={wire} N={low}; N-1/fresh/repeated/retry");
        }
        assert!(matches!(m.functions[0].values[8], Type::Desc(_)));
        assert!(c.functions[0].holds[8].roots.is_empty());
        let mut forged = c.clone();
        forged.functions[0].holds[8].roots = vec![HoldRoot::Local(LocalId(1))];
        assert_ne!(forged.functions[0].holds[8], c.functions[0].holds[8]);
        let bytes = zil_ir::wire::encode_module_candidate(&m, &forged, &Limits::default()).unwrap();
        for result in [
            super::super::verify_module(m.clone(), forged, &m.target, &Limits::default()),
            super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()),
        ] {
            let error = result.unwrap_err();
            assert!(
                error.iter().any(|d| d.code == "E-VERIFY-STORED"
                    && d.message
                        .contains("independently reconstructed stored target roots")),
                "{error:?}"
            );
        }
    }
    #[test]
    fn two_rich_roles_consume_one_verification_ledger() {
        let m = fixture::qualified_two_fn_roles(false);
        let c = fixture::class_candidate(&m);
        let limit = |n| Limits {
            max_analysis_steps: n,
            ..Limits::default()
        };
        let check = |n| super::super::verify_module(m.clone(), c.clone(), &m.target, &limit(n));
        let expected = check(Limits::default().max_analysis_steps)
            .unwrap()
            .fingerprint();
        let mut low = 0;
        let mut high = Limits::default().max_analysis_steps;
        while low < high {
            let mid = low + (high - low) / 2;
            if check(mid).is_ok() {
                high = mid
            } else {
                low = mid + 1
            }
        }
        let error = check(low - 1).unwrap_err();
        assert!(
            error.iter().any(|d| d.code == "E-RESOURCE-LIMIT"),
            "{error:?}"
        );
        assert_eq!(check(low).unwrap().fingerprint(), expected);
        assert_eq!(check(low).unwrap().fingerprint(), expected);
        println!("342 qualified Fn role Verify N={low}; N-1 ResourceLimit/fresh N identical");
    }
    #[test]
    fn qualified_callback_exclusive_alias_fails_even_with_valid_class_equations() {
        let m = fixture::qualified_fn_exclusive_call(false);
        let c = fixture::class_candidate(&m);
        let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
        super::super::verify_module(m.clone(), c, &m.target, &Limits::default()).unwrap();
        super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()).unwrap();
        let m = fixture::qualified_fn_exclusive_call(true);
        let c = fixture::class_candidate(&m);
        let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
        let error =
            super::super::verify_module(m.clone(), c, &m.target, &Limits::default()).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.contains("live incompatible loan")),
            "{error:?}"
        );
        let error =
            super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.contains("live incompatible loan")),
            "{error:?}"
        );
    }
    #[test]
    fn two_parameter_roles_with_one_closure_keep_distinct_rich_relations() {
        let m = fixture::qualified_two_fn_roles(false);
        let c = fixture::class_candidate(&m);
        let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
        super::super::verify_module(m.clone(), c, &m.target, &Limits::default()).unwrap();
        super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()).unwrap();
        let m = fixture::qualified_two_fn_roles(true);
        let c = fixture::class_candidate(&m);
        let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
        let error =
            super::super::verify_module(m.clone(), c, &m.target, &Limits::default()).unwrap_err();
        assert!(
            error.iter().any(|d| d.message.contains("class equality")),
            "{error:?}"
        );
        let error =
            super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()).unwrap_err();
        assert!(
            error.iter().any(|d| d.message.contains("class equality")),
            "{error:?}"
        );
    }
    #[test]
    fn qualified_fn_role_class_equations_are_checked_in_raw_and_wire() {
        let m = fixture::qualified_fn_role(false, false);
        let c = fixture::class_candidate(&m);
        let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
        super::super::verify_module(m.clone(), c, &m.target, &Limits::default()).unwrap();
        super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()).unwrap();
        for m in [
            fixture::qualified_fn_role(true, false),
            fixture::qualified_fn_role(false, true),
        ] {
            let c = fixture::class_candidate(&m);
            let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
            let error = super::super::verify_module(m.clone(), c, &m.target, &Limits::default())
                .unwrap_err();
            assert!(
                error.iter().any(|d| d.message.contains("class equality")),
                "{error:?}"
            );
            let error = super::super::verify_module_bytes(&bytes, &m.target, &Limits::default())
                .unwrap_err();
            assert!(
                error.iter().any(|d| d.message.contains("class equality")),
                "{error:?}"
            );
        }
    }
    #[test]
    fn unbound_pointer_fixed_is_rejected_in_raw_and_decoded_wire() {
        let m = fixture::phantom_class_calls(true, false);
        let c = fixture::class_candidate(&m);
        let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
        super::super::verify_module(m.clone(), c, &m.target, &Limits::default()).unwrap();
        super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()).unwrap();
        let m = fixture::unbound_fixed_pointer();
        let c = fixture::class_candidate(&m);
        let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
        let error =
            super::super::verify_module(m.clone(), c, &m.target, &Limits::default()).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.contains("genuine enclosing semantic context")),
            "{error:?}"
        );
        let error =
            super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.contains("genuine enclosing semantic context")),
            "{error:?}"
        );
    }
    #[test]
    fn rich_indirect_alias_is_rejected_in_raw_and_wire_even_with_matching_classes() {
        let m = fixture::rich_marker_exclusive_call(true);
        let c = fixture::class_candidate(&m);
        let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
        let error =
            super::super::verify_module(m.clone(), c, &m.target, &Limits::default()).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.contains("live incompatible loan")),
            "{error:?}"
        );
        let error =
            super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.contains("live incompatible loan")),
            "{error:?}"
        );
    }
    #[test]
    fn rich_class_equations_use_the_same_verification_allowance() {
        let m = fixture::rich_marker_pointer(false, false);
        let c = fixture::class_candidate(&m);
        let limit = |n| Limits {
            max_analysis_steps: n,
            ..Limits::default()
        };
        let check = |n| super::super::verify_module(m.clone(), c.clone(), &m.target, &limit(n));
        let expected = check(Limits::default().max_analysis_steps)
            .unwrap()
            .fingerprint();
        let mut low = 0;
        let mut high = Limits::default().max_analysis_steps;
        while low < high {
            let mid = low + (high - low) / 2;
            if check(mid).is_ok() {
                high = mid
            } else {
                low = mid + 1
            }
        }
        let error = check(low - 1).unwrap_err();
        assert!(
            error.iter().any(|d| d.code == "E-RESOURCE-LIMIT"),
            "{error:?}"
        );
        assert_eq!(check(low).unwrap().fingerprint(), expected);
        assert_eq!(check(low).unwrap().fingerprint(), expected);
        println!("342 rich class Verify N={low}; N-1 ResourceLimit/fresh N identical");
    }
    #[test]
    fn raw_and_wire_rich_pointer_and_persistent_phantom_class_equations() {
        let good = [
            fixture::rich_marker_pointer(false, false),
            fixture::rich_marker_exclusive_call(false),
            fixture::phantom_class_calls(false, false),
            fixture::phantom_class_calls(true, false),
        ];
        for m in good {
            let c = fixture::class_candidate(&m);
            let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
            super::super::verify_module(m.clone(), c, &m.target, &Limits::default()).unwrap();
            super::super::verify_module_bytes(&bytes, &m.target, &Limits::default()).unwrap();
        }
        let bad = [
            fixture::rich_marker_pointer(true, false),
            fixture::rich_marker_pointer(false, true),
            fixture::phantom_class_calls(false, true),
            fixture::phantom_class_calls(true, true),
        ];
        for m in bad {
            let c = fixture::class_candidate(&m);
            let bytes = zil_ir::wire::encode_module_candidate(&m, &c, &Limits::default()).unwrap();
            let error = super::super::verify_module(m.clone(), c, &m.target, &Limits::default())
                .unwrap_err();
            assert!(
                error.iter().any(|d| d.message.contains("class equality")
                    || d.message.contains("entail actual target")),
                "{error:?}"
            );
            let error = super::super::verify_module_bytes(&bytes, &m.target, &Limits::default())
                .unwrap_err();
            assert!(
                error.iter().any(|d| d.message.contains("class equality")
                    || d.message.contains("entail actual target")),
                "{error:?}"
            );
        }
    }
    fn prove(module: &TypedModule) -> Check<()> {
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        inventory(module, &limits, &mut budget)?;
        for f in &module.functions {
            header(module, f, &mut budget)?;
        }
        let graphs: Vec<_> = module
            .functions
            .iter()
            .map(|_| FunctionGraph {
                successors: vec![vec![]],
                predecessors: vec![vec![]],
                reachable: vec![true],
                package_views: packages::CaptureViews::default(),
                role_data_uses: vec![],
            })
            .collect();
        // Deliberately supplied raw claims, checked against independent roots.
        let mut origins: Vec<Vec<Vec<HoldRoot>>> = vec![
            vec![vec![HoldRoot::Parameter(0)]; 3],
            vec![vec![HoldRoot::Parameter(0)]; 2],
            vec![vec![HoldRoot::Parameter(0)]; 2],
            vec![
                vec![],
                vec![HoldRoot::Local(LocalId(0))],
                vec![HoldRoot::Local(LocalId(0))],
                vec![HoldRoot::Local(LocalId(0)), HoldRoot::Local(LocalId(1))],
                vec![HoldRoot::Local(LocalId(0))],
                vec![],
            ],
            vec![vec![HoldRoot::Parameter(0)]],
        ];
        if module.functions.len() == 6 {
            origins.push(vec![vec![HoldRoot::Parameter(0)]]);
            origins[3].extend([
                vec![HoldRoot::Local(LocalId(0))],
                vec![HoldRoot::Local(LocalId(0))],
                vec![HoldRoot::Local(LocalId(0))],
                vec![],
                vec![],
            ]);
        }
        let mut candidate = CandidatePlan::empty_for(module);
        candidate.functions = module
            .functions
            .iter()
            .zip(origins)
            .map(|(f, origins)| FunctionPlan {
                function: f.id,
                blocks: vec![],
                holds: origins
                    .into_iter()
                    .enumerate()
                    .map(|(i, roots)| ValueHold {
                        value: ValueId(i as u32),
                        roots,
                    })
                    .collect(),
            })
            .collect();
        check(module, &graphs, &candidate, &mut budget)?;
        for (index, function) in candidate.functions.iter_mut().enumerate() {
            function.blocks = vec![BlockPlan {
                block: BlockId(0),
                initialized_in: if index == 3 { vec![] } else { vec![LocalId(0)] },
                initialized_out: vec![LocalId(0)],
            }];
        }
        verify_module(module.clone(), candidate.clone(), &module.target, &limits)
            .map_err(|d| d.into_iter().next().unwrap())?;
        let bytes = zil_ir::wire::encode_module_candidate(module, &candidate, &limits)
            .map_err(|d| d.into_iter().next().unwrap())?;
        verify_module_bytes(&bytes, &module.target, &limits)
            .map_err(|d| d.into_iter().next().unwrap())?;
        Ok(())
    }
    #[test]
    fn actual_shared_field_survives_carrier_end_and_two_forwarding_helpers() {
        prove(&fixture::module()).unwrap();
    }
    #[test]
    fn copied_field_keeps_target_parent_loan_after_carrier_end() {
        let mut m = fixture::module();
        let f = &mut m.functions[3];
        f.values.push(fixture::address(Type::I64, true));
        f.blocks[0].instructions.insert(
            8,
            fixture::instruction(
                Some(6),
                InstructionKind::BorrowLocalMut { local: LocalId(0) },
            ),
        );
        assert!(prove(&m).is_err());
    }
    #[test]
    fn unresolved_recursive_result_is_not_an_empty_authority() {
        let mut m = fixture::module();
        let f = &mut m.functions[4];
        f.values.push(fixture::address(Type::I64, false));
        f.blocks[0].instructions.push(fixture::instruction(
            Some(1),
            InstructionKind::Call {
                function: FunctionId(4),
                arguments: vec![ValueId(0)],
            },
        ));
        f.blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
        assert!(prove(&m).is_err());
    }
    #[test]
    fn actual_calls_create_children_and_do_not_duplicate_exclusive_input_identity() {
        prove(&fixture::exclusive_returns(false)).unwrap();
        let error = prove(&fixture::exclusive_returns(true)).unwrap_err();
        assert!(
            error.message.contains("actual target intersects"),
            "{error:?}"
        );
    }
}

/// Exact scalar field access of a real description. The opaque ownership head
/// and its borrowed pointee permissions stay distinct; only the accessed field
/// is appended to the actual current physical target (including heap segments).
pub(super) fn description_access(
    f: &TypedFunction,
    i: &Instruction,
    values: &[Payload],
    b: &mut Budget,
) -> Check<Option<(Permission, bool)>> {
    let (desc, index, write) = match i.kind {
        InstructionKind::ReadDesc { desc, index } => (desc, index, false),
        InstructionKind::WriteDesc { desc, index, .. } => (desc, index, true),
        _ => return Ok(None),
    };
    let ty = vt(f, desc)?;
    if !super::stored_owned::description(ty, b)? {
        return Ok(None);
    }
    let Type::Desc(inner) = ty else {
        unreachable!()
    };
    let Type::Record { fields, .. } = inner.as_ref() else {
        unreachable!()
    };
    if fields.get(index as usize).is_none() {
        return Err(err(
            "description access field is outside its complete actual record",
        ));
    }
    let payload = values
        .get(desc.0 as usize)
        .ok_or_else(|| err("description access has no actual SSA permission"))?;
    let permission = if payload.store_description.sources.is_empty() {
        &payload.address
    } else {
        &payload.store_description
    };
    permission_work(permission, b)?;
    let mut projected = permission.clone();
    for source in &mut projected.sources {
        b.spend(1)?;
        if let Some(path) = &mut source.heap_element {
            path.push(PlaceProjection::Field(index));
        } else {
            source.inline.push(PlaceProjection::Field(index));
        }
    }
    Ok(Some((projected, write)))
}
