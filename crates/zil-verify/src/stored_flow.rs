//! Independent stored-target reconstruction. Sorted vectors and batch CFG
//! solving deliberately do not consume analyzer roots, summaries, or claims.
use super::*;
mod access_candidates;
mod array_index;
mod array_snapshot;
#[cfg(test)]
mod borrowed_owned_entry_tests;
mod byte_queries;
#[path = "stored_flow/captured_owner.rs"]
mod captured_owner;
mod compound;
mod dyn_closed;
mod dyn_ports;
mod dyn_providers;
mod dyn_returns;
mod leaf_rows;
mod live_words;
pub(super) mod owned_byte_views;
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
pub(super) fn qualified_package_borrow(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    pure_copy::qualified_package_borrow(f, i, b)
}

pub(super) fn ordinary_error_borrow(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<bool> {
    pure_copy::ordinary_error_borrow(f, i, b)
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
        if shared_array_value(ty) {
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

pub(super) mod enum_address;
mod nominal;
pub(super) mod option_address;
mod retained;
mod task_address;
mod task_failure;
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
    if f.call_kind != CallKind::Direct || f.environment.is_some() {
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
    Task(u32),
    Dyn {
        carrier: Vec<ContractProjection>,
        port: u32,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Source {
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
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Frame {
    receiver: Option<usize>,
    locals: Vec<Payload>,
    external: Vec<Payload>,
    captures: Vec<Payload>,
    /// Union of paths left vacant by any incoming branch.
    vacant: Vec<Source>,
    prepared: Vec<replacements::Prepared>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Loan {
    sources: Vec<Source>,
    parents: Vec<usize>,
    exclusive: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Summary {
    byte_reads: Vec<Source>,
    head_accesses: Vec<(Source, bool)>,
    byte_covered: bool,
    dyn_accesses: Vec<(Source, bool)>,
    dyn_effects: Vec<DynEffect>,
    dyn_effects_known: bool,
    dyn_after: Vec<(u32, dyn_ports::Rows)>,
    result: Vec<(Path, Vec<Source>)>,
    receiver_results: Vec<Path>,
    dyn_result_parents: Vec<Source>,
    task_ports: Vec<(Vec<ContractProjection>, Vec<Source>)>,
    dyn_ports: dyn_ports::Rows,
    external_after: Vec<(u32, Path, Vec<Source>)>,
    capture_after: Vec<(u32, Path, Vec<Source>)>,
    apart: Vec<(Source, Source)>,
    complete: bool,
    returned_enum_variants: enum_address::Variants,
    enum_values: Vec<(ValueId, enum_address::Variants)>,
    returned_variants: option_address::Variants,
    none_values: Vec<ValueId>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum TaskLoanAnchor {
    Value(ValueId),
    Input(zil_ir::StoragePosition),
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct TaskPointerRoot {
    pub(super) base: HoldRoot,
    pub(super) port: Option<u32>,
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
}
struct Solution {
    values: Vec<Payload>,
    outputs: Vec<Frame>,
    entry: Frame,
    loans: Vec<Loan>,
    sites: WriteSites,
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
fn width(s: &Source) -> usize {
    let opaque = match &s.retained_port {
        Some(PortOrigin::Dyn { carrier, .. }) => carrier.len(),
        _ => 0,
    };
    s.leaf
        .len()
        .saturating_add(s.inline.len())
        .saturating_add(opaque)
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
fn frame_work(f: &Frame, b: &mut Budget) -> Check<()> {
    source_work(&f.vacant, b)?;
    replacements::frame_work(&f.prepared, b)?;
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
    Ok(f.clone())
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
fn take_package_field_leaves(holder: &mut Payload, index: u32, b: &mut Budget) -> Check<Payload> {
    b.spend(holder.leaves.len())?;
    let mut result = Payload::default();
    let mut retained = Vec::new();
    for (path, permission) in std::mem::take(&mut holder.leaves).into_vec(b)? {
        b.spend(path.len().saturating_add(1))?;
        if path.first() == Some(&PlaceProjection::Field(index)) {
            result.leaves.push((path[1..].to_vec(), permission), b)?;
        } else {
            retained.push((path, permission));
        }
    }
    holder.leaves = leaf_rows::Rows::from_vec(retained, b)?;
    Ok(result)
}
fn join_payload(to: &mut Payload, from: &Payload, b: &mut Budget) -> Check<bool> {
    let mut changed = join_permission(&mut to.address, &from.address, b)?;
    // Only the Option profile creates this private description ledger.
    if b.masked_stored_tasks() {
        changed |= join_permission(&mut to.store_description, &from.store_description, b)?;
    }
    changed |= task_address::join(to, from, b)?;
    changed |= dyn_ports::join(to, from, b)?;
    if let Some(complete) = from.creation_complete {
        let joined = Some(to.creation_complete.unwrap_or(false) || complete);
        changed |= to.creation_complete != joined;
        to.creation_complete = joined;
    }
    changed |= to.enum_variants.join(&from.enum_variants, b)?;
    changed |= to.option_variants.join(&from.option_variants, b)?;
    for (path, p) in &from.leaves {
        changed |= add_leaf(to, path, p, b)?;
    }
    Ok(changed)
}
fn join_frame(to: &mut Frame, from: &Frame, b: &mut Budget) -> Check<()> {
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
    b.spend(
        to.locals
            .len()
            .saturating_add(to.external.len())
            .saturating_add(to.captures.len()),
    )?;
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
                b.spend(name.len().saturating_add(fields.len()))?;
                for (n, t) in fields {
                    b.spend(n.len())?;
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
                b.spend(name.len().saturating_add(variants.len()))?;
                for (n, fs) in variants {
                    b.spend(n.len().saturating_add(fs.len()))?;
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
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn physical(ty: &Type, limits: &Limits, b: &mut Budget) -> Check<bool> {
    let Some((storage, shape)) = qualified(ty) else {
        return Ok(false);
    };
    type_work(ty, limits, b)?;
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
                type_work(inner, limits, b)?;
                if !inner.is_address_pointee() {
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
                type_work(payload, limits, b)?;
                if !matches!(
                    payload.as_ref(),
                    Type::ByteBuffer | Type::Record { .. } | Type::OwnedStore { .. }
                ) {
                    return Err(err(
                        "stored Package actual owner is outside finite resource profile",
                    ));
                }
            }
            child => {
                type_work(child, limits, b)?;
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
    if actual_retained.len() != shape.retained.len() {
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
    type_work(ty, limits, b)?;
    type_work(ty, limits, b)?;
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
pub(super) fn pure_read(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<bool> {
    if !b.pure_nominal
        || !matches!(ty, Type::BorrowedStorage { storage, .. } if matches!(storage.as_ref(), Type::Record { .. } | Type::Enum { .. }))
    {
        return Ok(false);
    }
    let limits = b.limits.clone();
    type_work(ty, &limits, b)?;
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
    type_work(ty, &limits, b)?;
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
            _ => return Ok(false),
        }
    }
    Ok(true)
}

pub(super) fn qualifications(
    module: &TypedModule,
    limits: &Limits,
    mask: Option<&[bool]>,
    b: &mut Budget,
) -> Check<()> {
    let mixed_package = b.stored_package_instance;
    for (index, f) in module.functions.iter().enumerate() {
        if mask.is_some_and(|mask| !mask[index]) {
            continue;
        }
        for block in &f.blocks {
            for i in &block.instructions {
                b.spend(1)?;
                if let InstructionKind::Load(local) = i.kind
                    && f.locals
                        .get(local.0 as usize)
                        .is_some_and(|local| exclusive_carrier_value(&local.ty))
                {
                    return Err(err(
                        "actual exclusive stored carrier was copied without a transfer",
                    ));
                }
                let scalar_owner_operation = match &i.kind {
                    InstructionKind::ReadDesc { desc, .. } => f
                        .values
                        .get(desc.0 as usize)
                        .is_some_and(super::stored_owned::description),
                    InstructionKind::MakeManaged { .. } => i.result.is_some_and(|id| {
                        f.values
                            .get(id.0 as usize)
                            .is_some_and(super::stored_owned::owner)
                    }),
                    _ => false,
                };
                let enum_operation = enum_address::instruction(module, f, i, b)?;
                let owned_operation = ((!mixed_package
                    && !b.masked_stored_tasks()
                    && super::stored_owned::instruction(f, &i.kind))
                    || scalar_owner_operation)
                    && !owned_byte_views::owner_instruction(module, f, i, b)?
                    && !(b.task_sources() && task_address::owner_instruction(f, &i.kind));
                if !super::class_flow::pointer_instruction(f, i)
                    && !super::stored_callables::instruction(module, f, i)
                    && !returned_environment::borrowed_child(module, f, i, b)?
                    && !compound::operation(f, i, b)?
                    && !dyn_closed::operation(module, f, i, b)?
                    && !dyn_ports::operation(module, f, i, b)?
                    && !dyn_ports::wrapper(module, f, i, b)?
                    && !dyn_closed::wrapper(module, f, i, b)?
                    && !owned_operation
                    && !enum_operation
                    && !owned_byte_views::instruction(module, f, i, b)?
                    && !owned_byte_views::description_instruction(module, f, i, b)?
                    && byte_queries::query(&i.kind).is_none()
                    && !(b.task_sources() && task_address::operation(&i.kind))
                    && !failure_unpack(f, i, b)?
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
                    && !pure_copy::ordinary_owned_union(f, i, b)?
                    && !pure_copy::ordinary_copy_field_borrow(f, i, b)?
                    && !pure_copy::ordinary_package_borrow(f, i, b)?
                    && !pure_copy::ordinary_package_read(f, i, b)?
                    && !pure_copy::qualified_package_description(f, i, b)?
                    && !ordinary_error_borrow(f, i, b)?
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
                                    | StoredBorrowLoad::Reborrow { mutable: true }
                                    | StoredBorrowLoad::Take,
                                ..
                            }
                            | InstructionKind::StoreStoredBorrow { .. }
                    )
                {
                    return Err(unsupported(
                        "instruction is outside the checked stored profile, including unreachable blocks",
                        Some(i.span),
                    ));
                }
                if owned_operation {
                    super::stored_owned::types(module, f, i, b)?;
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
                    if !pure_copy::ordinary_owned_union(f, i, b)? {
                        pure_copy::types(f, i, b)?;
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
        let (storage, _) = qualified(&instance.ty)
            .ok_or_else(|| err("nominal instance has no complete qualified type"))?;
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
            (LifeNominalKind::Record, Type::Record { .. })
                | (LifeNominalKind::Package, Type::Package { .. })
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
    if f.reference_return_bound.is_some() || f.foreign.is_some() {
        return Err(err("stored signature contradicts another active profile"));
    }
    let mut parameters = Vec::with_capacity(f.parameters.len());
    let mut has_fixed_environment = false;
    for p in &f.parameters {
        let ty = &f.locals[p.0 as usize].ty;
        type_work(ty, &b.limits.clone(), b)?;
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
            type_work(ty, &b.limits.clone(), b)?;
        }
        if f.callable_contract.is_some()
            && let Type::Managed(payload) = ty
            && let Type::Record { fields, .. } = payload.as_ref()
        {
            b.spend(fields.len().saturating_mul(2))?;
        }
        has_fixed_environment |= fixed_environment(module, ty, b);
        parameters.push(ty.clone());
    }
    type_work(&f.return_type, &b.limits.clone(), b)?;
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
        type_work(&f.return_type, &b.limits.clone(), b)?;
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
        return super::signature_life::closure_relation(
            module,
            &parameters,
            &d.captures,
            d.kind,
            super::stored_callables::life_result(d),
            c,
            Some(f.span),
        );
    }
    if has_fixed_environment {
        b.spend(parameters.len())?;
        for ty in &parameters {
            if !fixed_environment(module, ty, b)
                && !super::stored_callables::shared_input(ty)
                && !super::stored_owned::owner(ty)
            {
                return Err(unsupported(
                    "captured Fixed helper requires shared finite data parameters",
                    Some(f.span),
                ));
            }
        }
    }
    let byte_signature = byte_companion
        && !byte_queries::legacy_task_signature(f)
        && f.call_kind == CallKind::Direct
        && f.foreign.is_none()
        && f.environment.is_none()
        && byte_queries::signature_presence(module, b)?;
    if byte_signature {
        b.spend(parameters.len())?;
    }
    let check = if byte_signature {
        super::signature_life::function_relation_byte_inputs
    } else {
        super::signature_life::function_relation
    };
    // Actual Capture sources prove targets separately. If a factory publishes
    // output class rows, authenticate its complete Fixed class relation as well:
    // the result can only refer to classes already supplied by actual inputs.
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
    check(
        module,
        &parameters,
        result,
        c,
        Some(f.span),
        f.callable_contract.is_some() || factory,
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
        return Err(err("actual target intersects a live incompatible loan"));
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
                    apart(a, z, requirements, b)?;
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
        let capture = seed_payload(ty, base, loans, b)?;
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
    ty: &Type,
    base: HoldRoot,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> Check<Payload> {
    let mut p = Payload::default();
    if let Type::Address { mutable, .. } = ty {
        let source = Source {
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
                retained_port: None,
                base,
                leaf: path.clone(),
                inline: Vec::new(),
                heap_parents: Vec::new(),
                heap_element: None,
            };
            let id = loans.len();
            loans.push(Loan {
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
                retained_port: None,
                base,
                leaf: path.clone(),
                inline: Vec::new(),
                heap_parents: Vec::new(),
                heap_element: None,
            };
            let id = loans.len();
            loans.push(Loan {
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
                retained_port: None,
                base,
                leaf: slot.path.clone(),
                inline: Vec::new(),
                heap_parents: Vec::new(),
                heap_element: None,
            };
            let id = loans.len();
            loans.push(Loan {
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
        if super::stored_owned::description(ty) {
            b.spend(1)?;
            p.address.sources.push(Source {
                retained_port: None,
                base,
                leaf: Vec::new(),
                inline: Vec::new(),
                heap_parents: Vec::new(),
                heap_element: None,
            });
        }
        if replacements::heads(b)
            && matches!(ty, Type::Desc(payload) if matches!(payload.as_ref(), Type::ByteBuffer | Type::Record { .. } | Type::OwnedStore { .. }))
        {
            b.spend(1)?;
            p.store_description.sources.push(Source {
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
    task_address::seed(ty, base, &mut p, loans, b)?;
    Ok(p)
}
fn empty_frame(module: &TypedModule, f: &TypedFunction, b: &mut Budget) -> Check<Frame> {
    b.spend(f.locals.len().saturating_add(f.parameters.len()))?;
    let count = captured_definition(module, f, b).map_or(0, |d| d.captures.len());
    b.spend(count)?;
    Ok(Frame {
        receiver: None,
        captures: vec![Payload::default(); count],
        locals: vec![Payload::default(); f.locals.len()],
        external: vec![Payload::default(); f.parameters.len()],
        vacant: Vec::new(),
        prepared: Vec::new(),
    })
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
        let mut p = seed_payload(ty, HoldRoot::Parameter(position as u32), loans, b)?;
        if enum_address::ordinary(module, ty, b)? {
            p.enum_variants = enum_address::Variants::parameter(position as u32, ty, b)?;
        } else if option_address::candidate(ty) {
            b.spend(96)?;
            if option_address::signature(ty) {
                p.option_variants = option_address::Variants::parameter(position as u32, b)?;
            }
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
        {
            state.external[position] = copy_payload(&p, b)?;
        }
        state.locals[local.0 as usize] = p;
    }
    if let Some(d) = captured_definition(module, f, b) {
        b.spend(d.captures.len())?;
        for (index, ty) in d.captures.iter().enumerate() {
            let base = HoldRoot::Capture(index as u32);
            let mut payload = seed_payload(ty, base, loans, b)?;
            if fixed_environment(module, ty, b) {
                seed_environment(module, ty, base, &mut payload, loans, b)?;
            }
            state.captures[index] = payload;
        }
    }
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
        if !source.leaf.is_empty() {
            return Err(err("stored leaf base crosses a target or inline boundary"));
        }
        let mut actual_path = source.inline.clone();
        actual_path.extend_from_slice(path);
        let p = match source.base {
            HoldRoot::Static => {
                return Err(err("static text cannot supply nested carrier permissions"));
            }
            HoldRoot::Local(l) => state.locals.get(l.0 as usize),
            HoldRoot::Parameter(p) => state.external.get(p as usize),
            HoldRoot::Capture(p) => state.captures.get(p as usize),
        }
        .ok_or_else(|| err("carrier location is absent"))?;
        if let Some(p) = leaf(p, &actual_path, b)? {
            join_permission(&mut result, p, b)?;
        }
    }
    Ok(result)
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
        if state.vacant.contains(&slot) {
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
    let mut p = if let Some(port) = &source.retained_port {
        match port {
            PortOrigin::Task(port) => task_address::select(v, &source.leaf, *port, b)?,
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
    } else if replacements::heads(b)
        && source.leaf.is_empty()
        && !v.store_description.sources.is_empty()
    {
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
                    let id = loans.len();
                    loans.push(Loan::default());
                    b.stored_exclusive_loans |= exclusive;
                    sites.push((site, parameter, path.clone(), id, exclusive));
                    Ok(())
                };
            match &i.kind {
                InstructionKind::MakeAggregate { .. } | InstructionKind::MakePackage { .. } => {
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
                InstructionKind::Call { function, .. } => {
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
                        if let Type::Address {
                            mutable: true,
                            inner,
                        } = &callee.locals[local.0 as usize].ty
                            && let Type::Closure(id) = inner.as_ref()
                        {
                            for (path, ty) in returned_environment::leaves(module, *id, b)? {
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
    loans: &mut [Loan],
    b: &mut Budget,
) -> Check<Payload> {
    b.spend(1)?;
    let mut out = Payload::default();
    let val = |v: ValueId| {
        values
            .get(v.0 as usize)
            .ok_or_else(|| err("operand outside actual value table"))
    };
    let dyn_transfer = dyn_ports::transfer(module, f, i, state, values, b)?;
    let closed_dyn_transfer = dyn_closed::transfer(module, f, i, state, values, b)?;
    let task_transfer = task_address::transfer(module, f, i, state, values, summaries, b)?;
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
        InstructionKind::UnpackTuple { local, fields } if compound::operation(f, i, b)? => {
            compound::unpack(*local, fields, state, b)?;
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

        InstructionKind::UnpackTuple { local, fields } if unpack_failure => {
            task_failure::unpack_value(f, *local, fields, state, b)?;
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
            task_failure::take_view(state, &val(*storage)?.address, life, &mut out, b)?;
        }
        _ if replacements::operation(&i.kind) => {
            replacements::step(f, i, state, values, sites, loans, b)?;
        }
        InstructionKind::ExtractField { aggregate, index } if compound::projection(f, i, b)? => {
            let input = val(*aggregate)?;
            for (path, permission) in &input.leaves {
                b.spend(path.len().saturating_add(1))?;
                if let Some((PlaceProjection::Field(field), tail)) = path.split_first()
                    && field == index {
                    add_leaf(&mut out, tail, permission, b)?;
                }
            }
        }
        InstructionKind::StaticUtf8 { .. } => {
            b.spend(1)?;
            out.address.sources.push(Source {
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
            if ordinary_error_borrow(f, i, b)? =>
        {
            b.spend(path.len().saturating_add(1))?;
            out.address.sources.push(Source {
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
            out = take_package_field_leaves(holder, *index, b)?;
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
            take_package_field_leaves(holder, *index, b)?;
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
            take_package_field_leaves(holder, *index, b)?;
            for (path, permission) in &source.leaves {
                b.spend(path.len().saturating_add(1))?;
                let mut complete = vec![PlaceProjection::Field(*index)];
                complete.extend(path);
                add_leaf(holder, &complete, permission, b)?;
            }
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
        }
        InstructionKind::BorrowPackageField { local, index, mutable: false }
            if pure_copy::qualified_package_borrow(f, i, b)? => {
                b.spend(2)?;
                out.address.sources.push(Source { retained_port: None,
                    base: HoldRoot::Local(*local), leaf: Vec::new(),
                    inline: vec![PlaceProjection::Field(*index)], heap_parents: Vec::new(), heap_element: None });
            }
        InstructionKind::DescribePackageField { local, index }
            if pure_copy::qualified_package_description(f, i, b)? => {
                b.spend(2)?;
                out.address.sources.push(Source {
                    retained_port: None, base: HoldRoot::Local(*local),
                    leaf: Vec::new(), inline: vec![PlaceProjection::Field(*index)],
                    heap_parents: Vec::new(), heap_element: None,
                });
            }
        InstructionKind::DescribeLocal { local } => {
            if f.locals
                .get(local.0 as usize)
                .is_some_and(|local| super::stored_owned::owner(&local.ty))
            {
                b.spend(1)?;
                out.address.sources.push(Source {
                    retained_port: None,
                    base: HoldRoot::Local(*local),
                    leaf: Vec::new(),
                    inline: Vec::new(),
                    heap_parents: Vec::new(),
                    heap_element: None,
                });
            }
            if replacements::heads(b)
                && f.locals.get(local.0 as usize).is_some_and(|local| matches!(&local.ty, Type::Managed(payload) if matches!(payload.as_ref(), Type::ByteBuffer | Type::Record { .. } | Type::OwnedStore { .. })))
            {
                b.spend(1)?;
                out.store_description.sources.push(Source {
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
            if replacements::heads(b) && i.result.is_some_and(|id|
                matches!(&f.values[id.0 as usize], Type::Desc(payload)
                    if matches!(payload.as_ref(), Type::ByteBuffer | Type::Record { .. } | Type::OwnedStore { .. }))) => {
            b.spend(2)?;
            out.store_description.sources.push(Source {
                retained_port: None,
                base: HoldRoot::Local(*local),
                leaf: Vec::new(),
                inline: vec![PlaceProjection::Field(*index)],
                heap_parents: Vec::new(),
                heap_element: None,
            });
        }
        InstructionKind::DescribeCapture { index } => {
            b.spend(1)?;
            out.address.sources.push(Source {
                retained_port: None,
                base: HoldRoot::Capture(*index),
                leaf: Vec::new(),
                inline: Vec::new(),
                heap_parents: Vec::new(),
                heap_element: None,
            });
        }
        InstructionKind::TakeCapture { index } if b.returned_environment_sources => {
            out = copy_payload(&state.captures[*index as usize], b)?;
            state.captures[*index as usize] = Payload::default();
        }
        InstructionKind::WriteCapture { index, value } if b.returned_environment_sources => {
            state.captures[*index as usize] = copy_payload(val(*value)?, b)?;
        }
        InstructionKind::TakeCapture { .. }
        | InstructionKind::MakeManaged { .. }
        | InstructionKind::ReadDesc { .. } => {}
        InstructionKind::DropTuple { local } if failure_tuple_drop(f, i, b)? => {
            state.locals[local.0 as usize] = Payload::default();
            clear_local_holes(state, *local, b)?;
        }
        InstructionKind::DropManaged { local } | InstructionKind::DropPackage { local } => {
            state.locals[local.0 as usize] = Payload::default();
            clear_local_holes(state, *local, b)?;
        }
        InstructionKind::DescribeAddress { address }
            if b.masked_stored_tasks() || b.replacements || b.shared_byte_borrow && i.result.is_some_and(|result| matches!(f.values[result.0 as usize], Type::Desc(ref payload) if payload.as_ref() == &Type::ByteBuffer)) => {
            join_permission(&mut out.store_description, &val(*address)?.address, b)?;
        }
        InstructionKind::OwnedStoreBorrow { store, .. } | InstructionKind::OwnedStoreBorrowMut { store, .. }
            if b.masked_stored_tasks() && (b.store_descriptions() || matches!(i.kind, InstructionKind::OwnedStoreBorrow { .. })) => {
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
        InstructionKind::ByteBufferBorrow { mutable: false, .. } => {
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
        InstructionKind::BorrowCapture { index, .. }
            if returned_environment::borrowed_child(module, f, i, b)? =>
        {
            // The head loan protects the real inline child environment. Its
            // external leaves retain their independent target permissions.
            out = copy_payload(&state.captures[*index as usize], b)?;
            b.spend(1)?;
            out.address = Permission {
                sources: vec![Source {
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
        InstructionKind::BorrowCapture {
            index,
            mutable: false,
        } => {
            let definition = captured_definition(module, f, b)
                .filter(|d| super::stored_callables::lending(d))
                .ok_or_else(|| err("capture loan has no qualified lender entry"))?;
            if !definition
                .captures
                .get(*index as usize)
                .is_some_and(super::stored_callables::scalar)
            {
                return Err(err("capture loan does not address an inline scalar field"));
            }
            b.spend(1)?;
            out.address.sources.push(Source {
                retained_port: None,
                base: HoldRoot::Capture(*index),
                leaf: Vec::new(),
                inline: Vec::new(),
                heap_parents: Vec::new(),
                heap_element: None,
            });
        }
        kind if super::stored_callables::called(module, f, kind)
            .is_some_and(|(definition, _)| receiver::declared(definition) && matches!(definition.result, Type::Address { .. })) =>
        {
            receiver::receiver_result(module, f, i, state, values, summaries, sites, loans, &mut out, b)?;
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
                if matches!(ty, Type::Address { .. }) {
                    add_leaf(&mut out, &prefix, &actual.address, b)?;
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
        }
        InstructionKind::BorrowLocal { local } | InstructionKind::BorrowLocalMut { local } => {
            b.spend(1)?;
            out.address.sources.push(Source {
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
            mode: StoredBorrowLoad::Reborrow { mutable: true },
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
                let holder = match position.base {
                    HoldRoot::Local(local) => state.locals.get_mut(local.0 as usize),
                    HoldRoot::Parameter(index) => state.external.get_mut(index as usize),
                    HoldRoot::Capture(index) => state.captures.get_mut(index as usize),
                    HoldRoot::Static => None,
                };
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
            let holder = match root.base {
                HoldRoot::Local(local) => state.locals.get_mut(local.0 as usize),
                HoldRoot::Parameter(index) => state.external.get_mut(index as usize),
                HoldRoot::Capture(index) => state.captures.get_mut(index as usize),
                HoldRoot::Static => None,
            }
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
        InstructionKind::ForeignCall { function, .. } => {
            let declaration = module
                .functions
                .get(function.0 as usize)
                .ok_or_else(|| err("foreign scalar entry is absent"))?;
            if !super::stored_callables::scalar_foreign(declaration, b)? {
                return Err(err(
                    "stored foreign call requires an actual scalar-only declaration",
                ));
            }
        }
        InstructionKind::UnsafeEnter { .. } | InstructionKind::UnsafeExit { .. } => {}
        InstructionKind::Call {
            function,
            arguments,
        } => {
            let callee = module
                .functions
                .get(function.0 as usize)
                .ok_or_else(|| err("call target absent"))?;
            if callee.foreign.is_some()
                || callee.environment.is_some()
                || callee.call_kind != CallKind::Direct
            {
                return Err(err(
                    "stored call requires an independently checked ordinary direct body",
                ));
            }
            dyn_ports::call(
                module,
                f,
                callee,
                &summaries[function.0 as usize],
                arguments,
                values,
                state,
                &mut out,
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
            out.enum_variants = summaries[function.0 as usize]
                .returned_enum_variants
                .substitute(arguments, values, b)?;
            out.option_variants = summaries[function.0 as usize]
                .returned_variants
                .substitute(arguments, values, b)?;
            for (path, sources) in &summaries[function.0 as usize].result {
                let mut p = Permission::default();
                for source in sources {
                    join_permission(
                        &mut p,
                        &instantiate(module, f, source, arguments, values, state, b)?,
                        b,
                    )?;
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
            if b.dyn_contexts {
                for parent in &summaries[function.0 as usize].dyn_result_parents {
                    let actual = instantiate(module, f, parent, arguments, values, state, b)?;
                    add_loans(&mut out.address.loans, &actual.loans, b)?;
                }
            }
            let dyn_updates = dyn_ports::prepare_after(
                module,
                f,
                &summaries[function.0 as usize],
                arguments,
                values,
                state,
                b,
            )?;
            let mut updates = Vec::new();
            for (parameter, path, sources) in &summaries[function.0 as usize].external_after {
                let index = *parameter as usize;
                let local = *callee
                    .parameters
                    .get(index)
                    .ok_or_else(|| err("external summary parameter is absent"))?;
                if !matches!(&callee.locals[local.0 as usize].ty,
                    Type::Address { mutable: true, inner } if qualified(inner).is_some() || returned_environment::closure(inner).is_some())
                {
                    continue;
                }
                let actual = val(*arguments
                    .get(index)
                    .ok_or_else(|| err("external summary actual argument is absent"))?)?;
                let [root] = actual.address.sources.as_slice() else {
                    return Err(err("external update needs one exact actual carrier source"));
                };
                if !root.leaf.is_empty() {
                    return Err(err("external update crosses a target boundary"));
                }
                let actual_path = if root.inline.is_empty() {
                    path.clone()
                } else {
                    slot_position(&actual.address, path, b)?.inline
                };
                let mut permission = Permission::default();
                for source in sources {
                    join_permission(
                        &mut permission,
                        &instantiate(module, f, source, arguments, values, state, b)?,
                        b,
                    )?;
                }
                if permission.sources.is_empty() {
                    return Err(err("external update has no grounded replacement source"));
                }
                b.spend(path.len().saturating_add(permission.sources.len()))?;
                let permission =
                    retain_written_slot(&permission, i, *parameter, path, sites, loans, b)?;
                updates.push((root.clone(), actual_path, permission));
            }
            for (root, path, permission) in updates {
                let holder = match root.base {
                    HoldRoot::Local(local) => state.locals.get_mut(local.0 as usize),
                    HoldRoot::Parameter(index) => state.external.get_mut(index as usize),
                    HoldRoot::Capture(index) => state.captures.get_mut(index as usize),
                    HoldRoot::Static => None,
                }
                .ok_or_else(|| err("updated caller carrier location is absent"))?;
                if let Some(row) = holder.leaves.position(&path, b)? {
                    *holder.leaves.permission_mut(row) = permission;
                } else {
                    b.spend(path.len().saturating_add(1))?;
                    holder.leaves.push((path, permission), b)?;
                }
            }
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
        _ if pure_copy::ordinary_owned_union(f, i, b)? => {}
        InstructionKind::BorrowPackageField { local, index, .. }
            if pure_copy::ordinary_copy_field_borrow(f, i, b)? =>
        {
            b.spend(2)?;
            out.address.sources.push(Source {
                retained_port: None,
                base: HoldRoot::Local(*local),
                leaf: Vec::new(),
                inline: vec![PlaceProjection::Field(*index)],
                heap_parents: Vec::new(),
                heap_element: None,
            });
        }
        _ if pure_copy::ordinary_package_borrow(f, i, b)? => {}
        _ if pure_copy::ordinary_package_read(f, i, b)? => {}
        _ if task_transfer.is_some() => {
            out = task_transfer.clone().unwrap();
        }
        InstructionKind::ExtractIndex { aggregate, .. } if array_index::candidate(f, i) => {
            out.address = array_index::transfer(f, i, val(*aggregate)?, b)?;
        }
        _ if pure_copy::operation(&i.kind) => {
            if !pure_copy::ordinary_owned_union(f, i, b)? {
                pure_copy::types(f, i, b)?;
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
        InstructionKind::ReadAddress { address } => {
            let Type::Address { inner, .. } = vt(f, *address)? else {
                return Err(err("array read lacks actual address"));
            };
            if shared_array_shape(inner, b)? {
                let Type::Array { length, .. } = inner.as_ref() else {
                    unreachable!()
                };
                b.spend((*length as usize).saturating_mul(2))?;
                for index in 0..*length {
                    let path = [PlaceProjection::Field(index)];
                    let permission = select(state, &val(*address)?.address, &path, b)?;
                    add_leaf(&mut out, &path, &permission, b)?;
                }
            }

            let Type::Address { inner, .. } = vt(f, *address)? else {
                return Err(err("read transfer has no actual Address"));
            };
            if qualified(inner).is_some() && !pure_read(module, inner, b)? {
                return Err(err(
                    "read transfer would erase actual borrowed/owned leaves",
                ));
            }
            // Access checks retain the actual pointer's permissions and loans.
            // This proven pure payload has no runtime leaves to transfer;
            // the complete result type preserves the semantic class mapping.
            // Do not turn the containing storage root into a phantom target.
        }
        InstructionKind::WriteAddress { address, .. } => {
            let Type::Address { inner, .. } = vt(f, *address)? else {
                return Err(err("write base lacks an actual Address type"));
            };
            resources::charge_payload(inner, b)?;
            if !inner.is_copy_value() {
                return Err(err(
                    "ordinary write cannot replace qualified carriers or borrowed leaves outside the actual slot protocol",
                ));
            }
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
        _ => {
            return Err(unsupported(
                "instruction lacks an independent stored target transfer",
                Some(i.span),
            ));
        }
    }
    if let Some(permission) = owned_byte_views::seed_description(module, f, i, values, b)? {
        out.store_description = permission;
    }
    captured_owner::apply(module, f, i, state, values, summaries, sites, loans, b)?;
    if let Some(id) = i.result
        && b.store_descriptions()
        && matches!(i.kind, InstructionKind::Call { .. })
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
                    | InstructionKind::ReborrowAddress { .. }
                    | InstructionKind::LoadStoredBorrow {
                        mode: StoredBorrowLoad::Reborrow { mutable: true },
                        ..
                    }
                    | InstructionKind::AddressField { .. }
                    | InstructionKind::AddressIndex { .. }
                    | InstructionKind::AddressSlice { .. }
                    | InstructionKind::Call { .. }
                    | InstructionKind::BorrowCapture { .. }
                    | InstructionKind::CallClosureBorrowed { .. }
                    | InstructionKind::CallClosureShared { .. }
                    | InstructionKind::CallClosureMut { .. }
                    | InstructionKind::BorrowDynOwned { .. }
                    | InstructionKind::MakeDynBorrow { .. }
                    | InstructionKind::CallDynSlot { .. }
            )
            || b.store_descriptions()
                && matches!(
                    i.kind,
                    InstructionKind::OwnedStoreBorrow { .. }
                        | InstructionKind::OwnedStoreBorrowMut { .. }
                )
            || ordinary_error_borrow(f, i, b)?
            || pure_copy::ordinary_copy_field_borrow(f, i, b)?
            || owned_byte_views::instruction(module, f, i, b)?)
        && matches!(vt(f, id)?, Type::Address { .. })
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
        && matches!(i.kind, InstructionKind::Call { .. })
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
            InstructionKind::MakeClosure { .. } | InstructionKind::Call { .. }
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
    replacements::prepare(module, f, g, b)?;
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
    let mut values = vec![Payload::default(); f.values.len()];
    let mut outputs = Vec::new();
    for _ in &f.blocks {
        outputs.push(empty_frame(module, f, b)?);
    }
    // The Task/Option and ordinary Copy error-place domains use this DAG path.
    // Keep historical
    // requests on their existing solver and request-cost path. In a DAG every
    // predecessor frame and dominating SSA definition is final before its user;
    // join_payload still records all sources and loan growth, including values
    // whose creation does not change a local frame.
    if (b.task_sources() || b.ordinary_error_places)
        && b.masked_stored_tasks()
        && let Some(order) = acyclic_order(g, b)?
    {
        for bi in order {
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
                    &mut loans,
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
        });
    }
    loop {
        let mut changed = false;
        // Reverse batches differ from the candidate's forward in-place walk.
        let mut replacement = Vec::new();
        for block in &outputs {
            replacement.push(copy_frame(block, b)?);
        }
        for bi in (0..f.blocks.len()).rev() {
            b.spend(1)?;
            if !g.reachable[bi] {
                continue;
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
                    module, f, i, &mut state, &values, summaries, &sites, &mut loans, b,
                )?;
                if let Some(v) = i.result {
                    changed |= join_payload(&mut values[v.0 as usize], &out, b)?;
                }
            }
            frame_work(&state, b)?;
            frame_work(&outputs[bi], b)?;
            if state != outputs[bi] {
                replacement[bi] = state;
                changed = true;
            }
        }
        outputs = replacement;
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
            if pure_copy::ordinary_copy_field_borrow(f, i, b)? =>
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
            if pure_copy::ordinary_copy_field_borrow(f, i, b)? =>
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
fn live_loans(
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
        loans.add(&payload.address.loans, b)?;
        if b.store_descriptions() {
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
        for (_, p) in &payload.leaves {
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
        add_loans_by_scan(&mut loans, &payload.address.loans, b)?;
        if b.store_descriptions() {
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
        for (_, p) in &payload.leaves {
            add_loans_by_scan(&mut loans, &p.loans, b)?;
        }
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
    b: &mut Budget,
) -> Check<()> {
    let callee = module
        .functions
        .get(function.0 as usize)
        .ok_or_else(|| err("call target absent"))?;
    if arguments.len() != callee.parameters.len() {
        return Err(err("call arity differs from actual body signature"));
    }
    if replacements::heads(b) {
        for (source, write) in &summaries[function.0 as usize].head_accesses {
            let actual = instantiate(module, f, source, arguments, &solution.values, state, b)?;
            if *write {
                store_paths::writable(&actual, &solution.loans, b)?;
            }
            checks.check(&actual, *write, requirements, b)?;
            store_paths::record(head_accesses, &actual, *write, b)?;
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
        let a = instantiate(module, f, a, arguments, &solution.values, state, b)?;
        let z = instantiate(module, f, z, arguments, &solution.values, state, b)?;
        if a.sources.is_empty() || z.sources.is_empty() {
            return Err(err("call separation has incomplete actual target facts"));
        }
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
    dyn_accesses: &mut Vec<(Source, bool)>,
    b: &mut Budget,
) -> Check<()> {
    // A module marker only schedules this actual per-function inventory.
    let mut ordinary_error_places = false;
    if b.ordinary_error_places {
        b.spend(f.blocks.len())?;
        for block in &f.blocks {
            b.spend(block.instructions.len())?;
            for instruction in &block.instructions {
                ordinary_error_places |= ordinary_error_borrow(f, instruction, b)?;
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
    let ssa_groups = ssa_loan_groups::Sources::collect(&solution.values, b)?;
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
                    query_access(&permission, &active, &solution.loans, requirements, b)?;
                    byte_queries::record(byte_reads, &permission, b)?;
                }
            }
            // Query effects precede publication of this call's new result.
            // Preserve the original result exclusion for ordinary accesses.
            if let Some(id) = i.result {
                b.spend(active.len())?;
                active.retain(|x| *x != id.0 as usize);
            }
            owned_byte_sites.check(
                module,
                f,
                i,
                &state,
                &solution.values,
                &active,
                &solution.loans,
                requirements,
                b,
            )?;
            {
                let mut checks = access_candidates::Access::new(&active, &solution.loans, b)?;
                if replacements::heads(b) {
                    let head = dynamic_store::access(&i.kind)
                        .or_else(|| byte_buffers::access(&i.kind))
                        .or_else(|| match i.kind {
                            InstructionKind::ReadDesc { desc, .. } => Some((desc, false)),
                            InstructionKind::WriteDesc { desc, .. } => Some((desc, true)),
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
                        store_paths::record(head_accesses, permission, write, b)?;
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
                    InstructionKind::BorrowPlaceLocal { local, path, .. }
                        if ordinary_error_borrow(f, i, b)? =>
                    {
                        b.spend(path.len().saturating_add(1))?;
                        let actual = Permission {
                            sources: vec![Source {
                                retained_port: None,
                                base: HoldRoot::Local(*local),
                                leaf: Vec::new(),
                                inline: path.clone(),
                                heap_parents: Vec::new(),
                                heap_element: None,
                            }],
                            loans: Vec::new(),
                        };
                        checks.check(&actual, false, requirements, b)?;
                    }
                    InstructionKind::TakeVariantField { local, path } if ordinary_error_places => {
                        b.spend(path.len().saturating_add(1))?;
                        let actual = Permission {
                            sources: vec![Source {
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
                    InstructionKind::DropErrorUnion { local } if ordinary_error_places => {
                        b.spend(1)?;
                        let actual = Permission {
                            sources: vec![Source {
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
                    InstructionKind::ReadDesc { desc, .. } => {
                        let actual = pointer(*desc)?;
                        if !actual.sources.is_empty() {
                            checks.check(actual, false, requirements, b)?;
                        }
                    }
                    InstructionKind::BorrowPackageField {
                        local,
                        index,
                        mutable: false,
                    } if pure_copy::qualified_package_borrow(f, i, b)? => {
                        b.spend(2)?;
                        let actual = Permission {
                            sources: vec![Source {
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
                    } if pure_copy::ordinary_copy_field_borrow(f, i, b)? => {
                        b.spend(2)?;
                        let actual = Permission {
                            sources: vec![Source {
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
                        ) {
                            checks.check(
                                &state.locals[local.0 as usize].address,
                                true,
                                requirements,
                                b,
                            )?;
                        }
                    }
                    InstructionKind::BorrowCapture { index, mutable } => {
                        b.spend(1)?;
                        let actual = Permission {
                            sources: vec![Source {
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
                        checks.check(
                            &actual,
                            !matches!(i.kind, InstructionKind::BorrowLocal { .. }),
                            requirements,
                            b,
                        )?;
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
                    InstructionKind::AddressField { address, .. }
                    | InstructionKind::AddressIndex { address, .. }
                    | InstructionKind::AddressSlice { address, .. } => {
                        let write = i.result.is_some_and(|v| {
                            matches!(f.values[v.0 as usize], Type::Address { mutable: true, .. })
                        });
                        checks.check(pointer(*address)?, write, requirements, b)?;
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
                            for (_, p) in &receiver.leaves {
                                permission_work(p, b)?;
                                all.push((p.clone(), false));
                            }
                        }
                        if let InstructionKind::CallClosureBorrowed { callee, .. } = &i.kind
                            && let Some(id) = returned_environment::closure(vt(f, *callee)?)
                        {
                            for (path, _) in returned_environment::leaves(module, id, b)? {
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
                        ) && let Some((definition, actuals)) =
                            stored_callables::called(module, f, &i.kind)
                        {
                            let obligations = &summaries[definition.function.0 as usize].apart;
                            if !obligations.is_empty() {
                                let receiver = captured_owner::receiver(f, i, &solution.values, b)?;
                                for (left, right) in obligations {
                                    let left = captured_owner::substitute(
                                        module,
                                        f,
                                        left,
                                        &definition.captures,
                                        &receiver,
                                        actuals,
                                        &solution.values,
                                        &state,
                                        b,
                                    )?;
                                    let right = captured_owner::substitute(
                                        module,
                                        f,
                                        right,
                                        &definition.captures,
                                        &receiver,
                                        actuals,
                                        &solution.values,
                                        &state,
                                        b,
                                    )?;
                                    for a in &left.sources {
                                        for z in &right.sources {
                                            apart(a, z, requirements, b)?;
                                        }
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
                    InstructionKind::Call {
                        function,
                        arguments,
                    } => check_call(
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
                        b,
                    )?,
                    _ => {}
                }
            }
            dyn_providers::observe(
                module,
                f,
                i,
                &state,
                &solution.values,
                summaries,
                dyn_accesses,
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
                &mut solution.loans,
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
    if replacements::heads(b) {
        b.spend(s.head_accesses.len())?;
        for (source, _) in &s.head_accesses {
            b.spend(width(source))?;
        }
    }
    if b.dyn_contexts {
        dyn_ports::work(&s.dyn_ports, b)?;
        dyn_providers::work(s, b)?;
        source_work(&s.dyn_result_parents, b)?;
        for (_, rows) in &s.dyn_after {
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
    for (bi, (block, reachable)) in f.blocks.iter().zip(&g.reachable).enumerate() {
        b.spend(1)?;
        if *reachable && matches!(block.terminator, Terminator::Return(_)) {
            captured_owner::summarize(module, f, &solution.outputs[bi], &mut summary, b)?;
        }
        if (b.stored_slot_updates || b.replacements || b.dyn_contexts)
            && *reachable
            && matches!(block.terminator, Terminator::Return(_))
        {
            for (position, payload) in solution.outputs[bi].external.iter().enumerate() {
                let local = *f
                    .parameters
                    .get(position)
                    .ok_or_else(|| err("external output exceeds actual parameter table"))?;
                if !matches!(&f.locals[local.0 as usize].ty,
                    Type::Address { mutable, inner } if (*mutable || b.dyn_contexts) && (qualified(inner).is_some() || returned_environment::closure(inner).is_some()))
                {
                    continue;
                }
                for (path, permission) in &payload.leaves {
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
            dyn_returns::after(
                module,
                f,
                &solution.outputs[bi],
                &mut summary,
                &solution.loans,
                b,
            )?;
        }
        if *reachable && let Terminator::Return(Some(id)) = block.terminator {
            let payload = &solution.values[id.0 as usize];
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
            task_address::summarize(f, payload, &mut summary, b)?;
            dyn_returns::summarize(module, f, payload, &mut summary, &solution.loans, b)?;
            if b.dyn_contexts && matches!(f.return_type, Type::Address { .. }) {
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
        check_exclusive_transfers(f, &graphs[f.id.0 as usize], b)?;
        super::class_flow::check(module, f, &graphs[f.id.0 as usize].reachable, b)?;
        if !module.resources.is_empty() && matches!(f.return_type, Type::Desc(_)) {
            return Err(unsupported(
                "owning Fixed profile does not export descriptions",
                Some(f.span),
            ));
        }
        if (f.foreign.is_some()
            && !super::stored_callables::scalar_foreign(f, b)?
            && !crate::tasks::input_scope::provider(module, f, b)?)
            || f.callable_return_contract.is_some() && !b.returned_environment_sources
            || f.call_kind != CallKind::Direct
                && !(b.task_sources() && f.call_kind == CallKind::Suspend)
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
        for ty in f
            .locals
            .iter()
            .map(|l| &l.ty)
            .chain(&f.values)
            .chain(std::iter::once(&f.return_type))
        {
            type_work(ty, &b.limits.clone(), b)?;
            let byte_value = owned_byte_profile && owned_byte_views::value(ty, b)?;
            let closed_dyn_value =
                dyn_closed::value(module, ty, b)? || dyn_ports::value(module, ty, b)?;
            if !module.resources.is_empty()
                && !mixed_package
                && !(b.task_sources() && task_address::shape(ty))
                && !b.masked_stored_tasks()
                && !byte_value
                && !closed_dyn_value
                && !compound_profile
            {
                super::stored_owned::value_type(ty, b)?;
            }
            let mixed_resource = if mixed_package {
                b.spend(module.resources.len())?;
                matches!(ty, Type::Managed(payload) | Type::Desc(payload)
                    if module.resources.iter().any(|definition| definition.payload == **payload))
                    || matches!(ty, Type::Address { inner, .. }
                        if matches!(inner.as_ref(), Type::Managed(payload)
                            if module.resources.iter().any(|definition| definition.payload == **payload)))
            } else {
                false
            };
            if !(byte_value
                || closed_dyn_value
                || super::stored_callables::value(module, ty)
                || b.task_sources() && task_address::admitted_value(ty)
                || qualified(ty).is_some()
                || addressed(ty).is_some()
                || matches!(ty,Type::Address{inner,..}if inner.is_address_pointee())
                || ty.is_copy_value()
                || shared_array_shape(ty, b)?
                || compound::slots(ty, b)?.is_some()
                || matches!(ty, Type::Address { inner, .. } if shared_array_shape(inner, b)?)
                || super::stored_owned::owner(ty)
                || super::stored_owned::description(ty)
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
                return Err(unsupported(
                    "owned or opaque executable value lacks actual stored permission integration",
                    Some(f.span),
                ));
            }
        }
    }
    if mask.is_none() && !b.returned_environment_sources {
        super::stored_callables::roles(module, graphs, b)?;
    }
    solve_check(module, graphs, plan, None, mask, b)
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
    b.spend(module.functions.len().saturating_mul(2))?;
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
        b.spend(module.functions.len())?;
        for index in (0..module.functions.len()).rev() {
            if mask.is_some_and(|mask| !mask[index]) {
                continue;
            }
            let f = &module.functions[index];
            let mut solution = solve(module, f, &graphs[index], &summaries, b)?;
            let mut summary = summarize(
                module,
                f,
                &graphs[index],
                &solution,
                inactive.map(|rows| rows[index].as_slice()),
                b,
            )?;
            if b.stored_slot_updates || b.dyn_contexts || b.replacements {
                b.spend(f.blocks.len())?;
                for block in &f.blocks {
                    b.spend(block.instructions.len())?;
                    for instruction in &block.instructions {
                        let function = match &instruction.kind {
                            InstructionKind::Call { function, .. } => *function,
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
                        if !prior.complete {
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
                    &mut summary.dyn_accesses,
                    b,
                )?;
            }
            summary_work(&summary, b)?;
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
        if !changed {
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
    dyn_providers::validate(module, &summaries, b)?;
    let mut checked_task_sources = if b.task_sources() {
        Some(Vec::with_capacity(module.functions.len()))
    } else {
        None
    };
    if b.masked_stored_tasks() {
        b.spend(module.functions.len())?;
    }
    let mut checked_option_sources = b
        .store_option_results
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
            return Err(err(
                "stored/reference operation has no complete actual target permission",
            ));
        }
        if b.returned_environment_sources {
            returned_environment::check_summary(module, f, s, b)?;
        }
        dyn_returns::validate(module, f, &s.dyn_ports, b)?;
        enum_address::projections(f, &graphs[index], s, b)?;
        let mut relations = relation_index::Relations::new(f, b)?;
        let receiver_definition =
            captured_definition(module, f, b).filter(|d| receiver::declared(d));
        for path in &s.receiver_results {
            let c = f
                .storage_life
                .as_ref()
                .ok_or_else(|| err("returned invocation view lacks its complete relation"))?;
            if receiver_definition.is_none()
                || relations.get(StorageBase::Result, path, b)?
                    != signature_life::outer(c, StorageBase::Receiver)
                        .ok_or_else(|| err("returned invocation view omits Receiver"))?
            {
                return Err(err(
                    "returned invocation view erased or changed its actual receiver bound",
                ));
            }
        }
        let lender = captured_definition(module, f, b)
            .filter(|d| super::stored_callables::lending(d) && !receiver::declared(d));
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
                    && receiver_definition.is_some()
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
                    && relations.get(StorageBase::Result, path, b)?
                        != (if source.retained_port.is_some() {
                            match &source.retained_port {
                                Some(PortOrigin::Task(_)) => {
                                    task_address::source_relation(f, source, b)?
                                }
                                Some(PortOrigin::Dyn { carrier, port }) => {
                                    dyn_returns::source_relation(
                                        module, f, position, carrier, *port, b,
                                    )?
                                }
                                None => unreachable!(),
                            }
                        } else {
                            relations.get(StorageBase::Parameter(position), &source.leaf, b)?
                        })
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
                if (b.shared_byte_borrow
                    && matches!(&f.values[index], Type::Desc(inner) if inner.as_ref() == &Type::ByteBuffer))
                    || b.store_descriptions() && matches!(&f.values[index], Type::Desc(_))
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
            checked.push(task_address::checked_sources(f, solution, b)?);
        }
        let claimed = &plan.functions[index].holds;
        if claimed.len() != solution.values.len() {
            return Err(err("candidate must cover each actual value once"));
        }
        for (v, (p, claim)) in solution.values.iter().zip(claimed).enumerate() {
            if b.masked_stored_tasks()
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
            let mut actual = BTreeSet::new();
            let weak_description = matches!(&f.values[v], Type::Desc(payload)
                if !matches!(payload.as_ref(), Type::ByteBuffer | Type::Task { .. }
                    | Type::TaskI32 | Type::OwnedStore { .. } | Type::Mutex { .. }));
            for root in p
                .address
                .sources
                .iter()
                .chain(p.store_description.sources.iter())
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
                    &mut [],
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
            &mut [],
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
                    2 => "actual capture mapping",
                    3 => "requires complete class relation",
                    4 | 5 => "class equality",
                    _ => unreachable!(),
                };
                assert!(
                    error.iter().any(|d| d.message.contains(expected)),
                    "variant{variant}: {error:?}"
                );
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
                    m.resources.push(ResourceDefinition {
                        name: "Other".into(),
                        payload: Type::Record {
                            name: "Other".into(),
                            fields: vec![("n".into(), Type::I32)],
                        },
                        teardown: None,
                    });
                }
                9 => {
                    let f = &mut m.functions[0];
                    let id = f.values.len() as u32;
                    f.values.push(Type::Address {
                        mutable: false,
                        inner: Box::new(f.values[7].clone()),
                    });
                    // A real well-typed but unused owner-head address must not
                    // bypass the finite profile through Copy-value fallback.
                    f.blocks[0].instructions.insert(
                        2,
                        fixture::instruction(
                            Some(id),
                            InstructionKind::BorrowLocal { local: LocalId(1) },
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
                let error = result.unwrap_err();
                if case <= 2 {
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
        let mut forged = c;
        forged.functions[0].holds[8].roots.clear();
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
