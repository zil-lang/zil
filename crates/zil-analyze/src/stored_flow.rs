//! Actual stored targets are distinct from the bytes containing their pointers.
//! This pass does not use a candidate, source label, or nominal name as a root.
use super::*;

mod array_index;
mod array_snapshot;
mod byte_queries;
mod capture_after;
mod environment_memory;
mod live_words;
pub(super) mod owned_byte_views;
mod permission_access;
mod receiver_results;
mod returned_upper;
#[cfg(test)]
mod set_bits_tests;
use live_words::LiveRow;
#[cfg(test)]
mod active_projection_tests;
#[cfg(test)]
mod binding_index_tests;
mod dyn_address;
mod dyn_return;
pub(super) mod enum_address;
mod nominal;
pub(super) mod option_address;
mod retained;
#[cfg(test)]
mod snapshot_tests;
mod task_address;
mod task_failure;
pub(super) fn failure_shape(ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    if !b.task_failure_carriers {
        return Ok(false);
    }
    task_failure::shape(ty, b)
}
pub(super) fn failure_borrow_only_unpack(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    if let InstructionKind::UnpackTuple { local, .. } = i.kind
        && crate::resources::sums::owned(&f.locals[local.0 as usize].ty)
    {
        return Ok(false);
    }
    task_failure::unpack(f, i, b)
}
fn failure_tuple_drop(f: &TypedFunction, i: &Instruction, b: &mut Budget) -> CompileResult<bool> {
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
pub(super) fn failure_unpack(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    task_failure::unpack(f, i, b)
}
pub(super) fn failure_projection(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<Option<Type>> {
    task_failure::projection(f, i, b)
}
use std::collections::BTreeMap;
pub(super) mod owned;
#[path = "stored_flow_pure_copy.rs"]
mod pure_copy;

pub(super) fn dyn_return_header(
    module: &TypedModule,
    f: &TypedFunction,
    b: &mut Budget,
) -> CompileResult<()> {
    dyn_return::header(module, f, b)
}

// Class grounding may recognize only this independently authenticated origin.
pub(super) fn ordinary_error_borrow(
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    Ok(pure_copy::ordinary_error_borrow(f, i, b)? || pure_copy::ordinary_resource_place(f, i, b)?)
}

pub(super) fn ordinary_package_borrow(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    pure_copy::ordinary_package_borrow(module, f, i, b)
}

/// These observations transport shared address permissions in this ledger,
/// while the resource Sum ledger has no owning leaf to consume. Authenticate
/// the actual operation; zero resource debt is not a Copy qualification.
pub(super) fn shared_enum_observation(
    module: &TypedModule,
    f: &TypedFunction,
    instruction: &Instruction,
    budget: &mut Budget,
) -> CompileResult<bool> {
    if !matches!(
        instruction.kind,
        InstructionKind::VariantTag { .. } | InstructionKind::ReadVariant { .. }
    ) {
        return Ok(false);
    }
    Ok(
        enum_address::types(module, f, instruction, budget)?.is_some()
            || option_address::types(module, f, instruction, budget)?.is_some(),
    )
}

pub(super) fn qualified_function(f: &TypedFunction, b: &mut Budget) -> CompileResult<bool> {
    // A plain address-returning helper can project the caller's actual stored
    // pointee even when its own signature contains no qualified carrier.
    if matches!(f.return_type, Type::Address { .. }) {
        return Ok(true);
    }
    // Actual closure entries returning owned data must join the stored proof
    // when callbacks are observed beside stored loans. Definition checking
    // separately authenticates the entry, complete signature and captures.
    if f.environment.is_some()
        && matches!(
            f.return_type,
            Type::Package { .. } | Type::Enum { .. } | Type::ErrorUnion { .. }
        )
    {
        return Ok(true);
    }

    fee(
        b,
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
        fee(b, 1)?;
        if shared_array_value(ty) {
            return Ok(true);
        }
        match ty {
            Type::OwnedStore { .. } if b.nested_store_windows => return Ok(true),
            Type::Desc(payload)
                if b.nested_store_windows
                    && matches!(payload.as_ref(), Type::ByteBuffer | Type::Record { .. }) =>
            {
                return Ok(true);
            }
            // Their actual capture/retained declarations can carry targets
            // outside this type spelling. Keep their functions in the proof.
            Type::BorrowedStorage { .. }
            | Type::Closure(_)
            | Type::Task { .. }
            | Type::BorrowedFunctionPointer { .. } => return Ok(true),
            Type::Address { inner, .. }
            | Type::Managed(inner)
            | Type::Desc(inner)
            | Type::OwnedStore { element: inner }
            | Type::Array { element: inner, .. }
            | Type::Slice { element: inner }
            | Type::Opaque { hidden: inner, .. } => {
                fee(b, 1)?;
                pending.push(inner);
            }
            Type::Package { fields, .. } | Type::Record { fields, .. } => {
                fee(b, fields.len())?;
                pending.extend(fields.iter().map(|(_, child)| child));
            }
            Type::Tuple(fields) => {
                fee(b, fields.len())?;
                pending.extend(fields);
            }
            Type::Enum { variants, .. } => {
                fee(b, variants.len())?;
                for (_, fields) in variants {
                    fee(b, fields.len())?;
                    pending.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                fee(b, 2)?;
                pending.extend([error.as_ref(), result.as_ref()]);
            }
            _ => {}
        }
    }
    Ok(false)
}

/// Snapshot private actual origins only after paying for every copied path.
/// Certify only a physical inline endpoint from the completed stored proof.
/// This does not recover a pointee from an input's stored pointer slot, invent
/// a heap element, or grant access/initialization/loan authority.
pub(super) fn inline_hold_source(
    function: &TypedFunction,
    target: &Target,
    expected: &Type,
    budget: &mut Budget,
) -> CompileResult<bool> {
    fee(budget, target_width(target).saturating_add(1))?;
    if target.retained_port.is_some()
        || target.heap_element.is_some()
        || !target.stored.is_empty()
        || target.place.is_empty()
    {
        return Ok(false);
    }
    let mut actual = match target.base {
        HoldRoot::Local(local) => function.locals.get(local.0 as usize).map(|l| &l.ty),
        HoldRoot::Parameter(index) => function
            .parameters
            .get(index as usize)
            .and_then(|local| function.locals.get(local.0 as usize))
            .and_then(|local| match &local.ty {
                Type::Address { inner, .. } => Some(inner.as_ref()),
                _ => None,
            }),
        _ => None,
    }
    .ok_or_else(|| failure("inline Hold endpoint has no actual local/input root"))?;
    budget.type_work(actual, false, "inline Hold root type")?;
    for step in &target.place {
        fee(budget, 1)?;
        if let Type::BorrowedStorage { storage, .. } = actual {
            actual = storage;
        }
        let PlaceProjection::Field(index) = step else {
            return Ok(false);
        };
        actual = match actual {
            Type::Package { fields, .. } | Type::Record { fields, .. } => {
                match fields.get(*index as usize) {
                    Some((_, ty)) => ty,
                    None => return Ok(false),
                }
            }
            Type::Tuple(fields) => match fields.get(*index as usize) {
                Some(ty) => ty,
                None => return Ok(false),
            },
            Type::Array { element, length } if *index < *length => element,
            _ => return Ok(false),
        };
    }
    budget.type_work(actual, false, "inline Hold actual endpoint")?;
    budget.type_work(expected, false, "inline Hold requested endpoint")?;
    Ok(match expected {
        Type::Address {
            mutable: false,
            inner,
        } => actual == inner.as_ref(),
        Type::Desc(inner) if budget.shared_byte_borrow && inner.as_ref() == &Type::ByteBuffer => {
            matches!(actual,Type::Managed(payload) if payload.as_ref()==&Type::ByteBuffer)
        }
        _ => false,
    })
}

pub(super) fn option_sources_snapshot(
    function: FunctionId,
    value: ValueId,
    budget: &mut Budget,
) -> CompileResult<Vec<Target>> {
    fn lookup(b: &Budget, function: FunctionId, value: ValueId) -> Option<&Vec<Target>> {
        b.store_option_targets
            .as_ref()
            .and_then(|rows| rows.get(function.0 as usize))
            .and_then(|rows| rows.get(value.0 as usize))
    }
    let count = lookup(budget, function, value)
        .ok_or_else(|| failure("Option value lacks actual stored proof"))?
        .len();
    fee(budget, count)?;
    let cost = lookup(budget, function, value)
        .unwrap()
        .iter()
        .fold(0usize, |cost, target| {
            cost.saturating_add(target_width(target))
        });
    fee(budget, cost)?;
    Ok(lookup(budget, function, value).unwrap().clone())
}

/// Read only a certificate rebuilt from actual pointer returns and call inputs.
pub(super) fn package_return_input(
    function: FunctionId,
    value: ValueId,
    b: &mut Budget,
) -> CompileResult<u32> {
    let count = b.package_return_calls.as_ref().map_or(0, BTreeMap::len);
    ordered(b, count, 2)?;
    let found = b
        .package_return_calls
        .as_ref()
        .and_then(|rows| rows.get(&(function, value)))
        .copied();
    found.ok_or_else(|| {
        vec![Diagnostic::unsupported(
            Phase::Analyze,
            "qualified Package return requires one actual whole shared input",
            None,
        )]
    })
}

fn whole_package_target(f: &TypedFunction, target: &Target, b: &mut Budget) -> CompileResult<bool> {
    fee(b, target_width(target).saturating_add(1))?;
    if target.retained_port.is_some()
        || !target.stored.is_empty()
        || !target.place.is_empty()
        || target.heap_element.is_some()
    {
        return Ok(false);
    }
    Ok(match target.base {
        HoldRoot::Local(local) => f.locals.get(local.0 as usize).is_some_and(|local|
            matches!(&local.ty, Type::BorrowedStorage { storage, .. } if matches!(storage.as_ref(), Type::Package { .. }))),
        HoldRoot::Parameter(position) => f.parameters.get(position as usize)
            .and_then(|local| f.locals.get(local.0 as usize))
            .is_some_and(|local| super::resources::qualified_package_address(&local.ty)),
        _ => false,
    })
}

fn ordinary_inline_package_target(
    f: &TypedFunction,
    target: &Target,
    expected: &Type,
    b: &mut Budget,
) -> CompileResult<bool> {
    fee(b, target_width(target).saturating_add(1))?;
    if target.retained_port.is_some()
        || !target.stored.is_empty()
        || target.heap_element.is_some()
        || !target.heap_parents.is_empty()
    {
        return Ok(false);
    }
    let mut ty = match target.base {
        HoldRoot::Local(local) => f.locals.get(local.0 as usize).map(|local| &local.ty),
        HoldRoot::Parameter(index) => f
            .parameters
            .get(index as usize)
            .and_then(|local| f.locals.get(local.0 as usize))
            .and_then(|local| match &local.ty {
                Type::Address { inner, .. } => Some(inner.as_ref()),
                _ => None,
            }),
        _ => None,
    }
    .ok_or_else(|| failure("ordinary Package source root absent"))?;
    for step in &target.place {
        fee(b, 1)?;
        if let Type::BorrowedStorage { storage, .. } = ty {
            ty = storage;
        }
        ty = match (step, ty) {
            (PlaceProjection::Field(index), Type::Package { fields, .. }) => {
                &fields
                    .get(*index as usize)
                    .ok_or_else(|| failure("ordinary Package source path absent"))?
                    .1
            }
            _ => return Ok(false),
        };
    }
    if let Type::BorrowedStorage { storage, .. } = ty {
        ty = storage;
    }
    b.type_work(ty, false, "actual inline Package source equality")?;
    b.type_work(expected, false, "actual inline Package endpoint equality")?;
    Ok(matches!(ty, Type::Package { .. }) && ty == expected)
}

fn package_return_candidates(
    module: &TypedModule,
    f: &TypedFunction,
    reachable: &[bool],
    values: &[Value],
    summaries: &[Facts],
    b: &mut Budget,
) -> CompileResult<BTreeMap<ValueId, u32>> {
    let mut result = BTreeMap::new();
    if f.call_kind != CallKind::Direct || f.environment.is_some() {
        return Ok(result);
    }
    fee(b, f.blocks.len())?;
    for (block, live) in f.blocks.iter().zip(reachable) {
        if !live {
            continue;
        }
        fee(b, block.instructions.len())?;
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
            ordered(b, prior.returned.len(), 0)?;
            let Some(roots) = prior.returned.get(&Vec::new()) else {
                continue;
            };
            fee(b, roots.len())?;
            let [root] = roots.as_slice() else {
                continue;
            };
            if !whole_package_target(callee, root, b)? {
                continue;
            }
            let HoldRoot::Parameter(position) = root.base else {
                continue;
            };
            let input = &callee.locals[callee.parameters[position as usize].0 as usize].ty;
            b.type_work(input, false, "whole Package actual input equality")?;
            b.type_work(
                &callee.return_type,
                false,
                "whole Package actual result equality",
            )?;
            if input != &callee.return_type {
                continue;
            }
            let argument = *arguments
                .get(position as usize)
                .ok_or_else(|| failure("Package return actual input absent"))?;
            let pointer = &values[argument.0 as usize].pointer.targets;
            if pointer.is_empty() {
                continue;
            }
            fee(b, pointer.len())?;
            let mut whole = true;
            for root in pointer {
                whole &= whole_package_target(f, root, b)?;
            }
            if !whole {
                continue;
            }
            let value = instruction
                .result
                .ok_or_else(|| failure("Package call result absent"))?;
            ordered(b, result.len(), 1)?;
            fee(b, std::mem::size_of::<(ValueId, u32)>())?;
            result.insert(value, position);
        }
    }
    Ok(result)
}

/// Port numbers have meaning only within the actual carrier namespace.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum PortOrigin {
    Task(u32),
    Dyn {
        carrier: Vec<zil_ir::ContractProjection>,
        port: u32,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Target {
    pub(super) retained_port: Option<PortOrigin>,
    pub base: HoldRoot,
    /// Empty is an ordinary input pointee. A nonempty path is an input's
    /// *stored target*, never a byte offset into that input's carrier.
    pub stored: Vec<PlaceProjection>,
    /// Manager-inline path remains pinned independently of the heap element.
    pub place: Vec<PlaceProjection>,
    /// Each entry is the actual inline path from a parent heap element
    /// to its next managed Store head. Order preserves every heap boundary.
    pub heap_parents: Vec<Vec<PlaceProjection>>,
    pub heap_element: Option<Vec<PlaceProjection>>,
}
type Targets = BTreeSet<Target>;
type Loans = BTreeSet<usize>;
type Path = Vec<PlaceProjection>;

/// A complete inline Record contributes addresses, never an owning leaf.
/// Physical coverage and class mappings are authenticated by the shape pass.
pub(crate) fn shared_record_child(ty: &Type, b: &mut Budget) -> CompileResult<bool> {
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
    fee(
        b,
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

pub(crate) fn shared_array_value(ty: &Type) -> bool {
    matches!(ty, Type::Array { element, length } if *length > 0
        && matches!(element.as_ref(), Type::Address { mutable: false, .. }))
}

pub(crate) fn shared_array_shape(ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    if !shared_array_value(ty) {
        return Ok(false);
    }
    let Type::Array { element, .. } = ty else {
        unreachable!()
    };
    let Type::Address { inner, .. } = element.as_ref() else {
        unreachable!()
    };
    b.type_work(inner, false, "shared array actual pointee")?;
    if !retained::closed_copy(inner, b)? {
        return Err(failure(
            "shared array actual pointee is outside its finite profile",
        ));
    }
    Ok(true)
}

type ActualStoredSlot = (
    Path,
    StoredBorrowMode,
    Option<(Vec<LifeTypeProjection>, u32)>,
);
type SlotLoanIds = BTreeMap<(usize, u32, Path), (usize, bool)>;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Permission {
    targets: Targets,
    loans: Loans,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Value {
    enum_variants: enum_address::Variants,
    option_variants: option_address::Variants,
    pointer: Permission,
    /// DescribeAddress transports an actual weak manager origin. It does not
    /// itself grant an element address or public Hold.
    store_description: Permission,
    task_ports: BTreeMap<Vec<zil_ir::ContractProjection>, Permission>,
    /// Distinct from Task frame ports: every dyn row is explicit, including unused uppers.
    dyn_ports: BTreeMap<Vec<zil_ir::ContractProjection>, Permission>,
    slots: BTreeMap<Path, Permission>,
    held: Permission,
    /// Capture ordinal followed by its pointee's stored-leaf path. These are
    /// symbolic environment payload selectors, never physical slot authority.
    environment: BTreeMap<Path, Permission>,
    creation_complete: Option<bool>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct State {
    locals: BTreeMap<LocalId, Value>,
    /// External addressed carriers have real cells too. Parameter position is
    /// not their target identity; each leaf starts with a symbolic target.
    external: BTreeMap<HoldRoot, BTreeMap<Path, Permission>>,
    captures: BTreeMap<u32, Value>,
    // A bound view aliases environment storage, never activation storage.
    capture_views: BTreeMap<LocalId, u32>,
    /// Empty-target invocation anchor grants no data access. Actual capture
    /// reborrows retain it alongside the original complete parent chain.
    receiver_loan: Option<usize>,
    /// A missing row is definitely initialized. Join unions holes, so a slot
    /// is usable only if every incoming path kept it initialized.
    vacant: BTreeSet<Target>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Loan {
    targets: Targets,
    parents: Loans,
    mutable: bool,
    receiver_head: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum TaskLoanAnchor {
    Value(ValueId),
    Input(Target),
}

fn pointer_anchors(
    f: &TypedFunction,
    current: ValueId,
    permission: &Permission,
    loans: &[Loan],
    b: &mut Budget,
) -> CompileResult<Vec<TaskLoanAnchor>> {
    fee(b, permission.loans.len())?;
    let mut pending = permission.loans.clone();
    let mut seen = BTreeSet::new();
    let mut out = BTreeSet::new();
    while let Some(id) = pending.pop_first() {
        ordered(b, seen.len(), 0)?;
        if !seen.insert(id) {
            continue;
        }
        let loan = loans
            .get(id)
            .ok_or_else(|| failure("pointer parent exceeds actual stored loan arena"))?;
        if id < f.values.len() && id != current.0 as usize {
            ordered(b, out.len(), 0)?;
            out.insert(TaskLoanAnchor::Value(ValueId(id as u32)));
        } else if !loan.parents.is_empty() {
            fee(b, loan.parents.len())?;
            for parent in &loan.parents {
                ordered(b, pending.len(), 0)?;
                pending.insert(*parent);
            }
        } else if id >= f.values.len() {
            for source in &loan.targets {
                fee(b, target_width(source))?;
                ordered(b, out.len(), target_width(source))?;
                out.insert(TaskLoanAnchor::Input(source.clone()));
            }
        }
    }
    fee(b, out.len())?;
    Ok(out.into_iter().collect())
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Facts {
    pub values: Vec<Vec<Target>>,
    pointer_values: Vec<Vec<HoldRoot>>,
    pointer_parents: Vec<Vec<TaskLoanAnchor>>,
    pub returned: BTreeMap<Path, Vec<Target>>,
    // Actual external leaves of returned concrete environments. Each row
    // keeps its original targets; caller substitution retains actual loans.
    returned_environment: BTreeMap<Path, Vec<Target>>,
    returned_task_ports: BTreeMap<Vec<zil_ir::ContractProjection>, Vec<Target>>,
    /// Actual callee Return targets, never populated from a declared upper.
    returned_dyn_ports: BTreeMap<Vec<zil_ir::ContractProjection>, Vec<Target>>,
    /// Separately proved public upper used by complete ordinary calls.
    declared_dyn_return_upper: BTreeMap<Vec<zil_ir::ContractProjection>, Vec<Target>>,
    dyn_return_complete: bool,
    package_return_calls: BTreeMap<ValueId, u32>,
    byte_reads: Vec<Target>,
    byte_covered: bool,
    external_after: BTreeMap<(u32, Path), Vec<Target>>,
    // Every reachable return preserves the exact seeded targets AND loans.
    // This is derived from actual terminal states, never a type/life claim.
    external_stable: BTreeSet<(u32, Path)>,
    capture_after: BTreeMap<Path, Vec<Target>>,
    capture_stable: BTreeSet<Path>,
    /// Actual entry-head ancestry of every reachable returned permission.
    receiver_return: bool,
    /// Structural callable input heads needed by the result, separate from
    /// its data targets. Each ordinary invocation substitutes its own loans.
    returned_head_inputs: BTreeSet<u32>,
    capture_taken: BTreeSet<u32>,
    pub apart: BTreeSet<(Target, Target)>,
    head_accesses: BTreeMap<Target, bool>,
    complete: bool,
    returned_enum_variants: enum_address::Variants,
    enum_values: BTreeMap<ValueId, enum_address::Variants>,
    returned_variants: option_address::Variants,
    none_values: BTreeSet<ValueId>,
}

fn failure(text: &str) -> Vec<Diagnostic> {
    bad(format!("stored target proof: {text}"))
}
fn fee(b: &mut Budget, n: usize) -> CompileResult<()> {
    b.spend(n, "stored target CFG proof")
}
fn ordered(b: &mut Budget, n: usize, width: usize) -> CompileResult<()> {
    b.product(
        n.checked_ilog2().map_or(1, |n| n as usize + 2),
        width.saturating_add(1),
        1,
        "stored path comparison/search",
    )
}
/// Every owned Target path participates in cloning and derived equality/order.
pub(super) fn target_width(target: &Target) -> usize {
    target
        .stored
        .len()
        .saturating_add(target.place.len())
        .saturating_add(target.heap_element.as_ref().map_or(0, Vec::len))
        .saturating_add(target.heap_parents.iter().fold(0usize, |n, path| {
            n.saturating_add(path.len()).saturating_add(1)
        }))
        .saturating_add(match &target.retained_port {
            Some(PortOrigin::Dyn { carrier, .. }) => carrier.len(),
            _ => 0,
        })
}
fn target_work(t: &Targets, b: &mut Budget) -> CompileResult<()> {
    fee(b, t.len())?;
    for target in t {
        fee(b, target_width(target))?;
    }
    Ok(())
}
fn union(to: &mut Permission, from: &Permission, b: &mut Budget) -> CompileResult<bool> {
    let mut changed = false;
    for target in &from.targets {
        ordered(b, to.targets.len(), target_width(target))?;
        fee(b, target_width(target).saturating_add(1))?;
        changed |= to.targets.insert(target.clone());
    }
    for loan in &from.loans {
        ordered(b, to.loans.len(), 0)?;
        changed |= to.loans.insert(*loan);
    }
    Ok(changed)
}
fn environment_merge(to: &mut Value, from: &Value, b: &mut Budget) -> CompileResult<bool> {
    let mut changed = union(&mut to.held, &from.held, b)?;
    for (path, permission) in &from.environment {
        ordered(b, to.environment.len(), path.len())?;
        fee(b, path.len().saturating_add(1))?;
        changed |= union(
            to.environment.entry(path.clone()).or_default(),
            permission,
            b,
        )?;
    }
    Ok(changed)
}
fn merge(to: &mut Value, from: &Value, b: &mut Budget) -> CompileResult<bool> {
    let mut changed = union(&mut to.pointer, &from.pointer, b)?;
    changed |= union(&mut to.store_description, &from.store_description, b)?;
    changed |= task_address::merge(to, from, b)?;
    changed |= dyn_address::merge(to, from, b)?;
    changed |= to.enum_variants.join(&from.enum_variants, b)?;
    changed |= to.option_variants.join(&from.option_variants, b)?;
    changed |= environment_merge(to, from, b)?;
    if let Some(complete) = from.creation_complete {
        let joined = Some(to.creation_complete.unwrap_or(false) || complete);
        changed |= to.creation_complete != joined;
        to.creation_complete = joined;
    }
    for (path, value) in &from.slots {
        ordered(b, to.slots.len(), path.len())?;
        fee(b, path.len().saturating_add(1))?;
        changed |= union(to.slots.entry(path.clone()).or_default(), value, b)?;
    }
    Ok(changed)
}

fn permission_clone_work(value: &Permission, b: &mut Budget) -> CompileResult<()> {
    fee(b, value.loans.len().saturating_add(1))?;
    target_work(&value.targets, b)
}
fn copy_permission(value: &Permission, b: &mut Budget) -> CompileResult<Permission> {
    permission_clone_work(value, b)?;
    Ok(value.clone())
}
fn copy_permission_rows<P: Copy + Ord>(
    rows: &BTreeMap<Vec<P>, Permission>,
    b: &mut Budget,
) -> CompileResult<BTreeMap<Vec<P>, Permission>> {
    // Clone the already ordered tree in one traversal, rather than rebuilding
    // it through per-key searches. Every owned path/target and loan is paid.
    fee(b, rows.len().saturating_add(1))?;
    for (path, permission) in rows {
        fee(b, path.len())?;
        permission_clone_work(permission, b)?;
    }
    Ok(rows.clone())
}
fn snapshot(value: &Value, b: &mut Budget) -> CompileResult<Value> {
    fee(b, 1)?;
    // Keep the established variant normalization and retained-port projection.
    // This is a copy of current facts, never a cache or a new permission.
    let mut enum_variants = enum_address::Variants::default();
    enum_variants.join(&value.enum_variants, b)?;
    let mut option_variants = option_address::Variants::default();
    option_variants.join(&value.option_variants, b)?;
    Ok(Value {
        enum_variants,
        option_variants,
        pointer: copy_permission(&value.pointer, b)?,
        store_description: copy_permission(&value.store_description, b)?,
        task_ports: if b.task_sources() {
            copy_permission_rows(&value.task_ports, b)?
        } else {
            BTreeMap::new()
        },
        dyn_ports: if b.dyn_contexts {
            copy_permission_rows(&value.dyn_ports, b)?
        } else {
            BTreeMap::new()
        },
        slots: copy_permission_rows(&value.slots, b)?,
        held: copy_permission(&value.held, b)?,
        // This optional namespace is absent on legacy paths. Test the actual
        // private map, so no nonempty environment can be discarded by a flag.
        environment: if value.environment.is_empty() {
            BTreeMap::new()
        } else {
            copy_permission_rows(&value.environment, b)?
        },
        creation_complete: value.creation_complete,
    })
}
#[cfg(test)]
fn snapshot_by_union(value: &Value, b: &mut Budget) -> CompileResult<Value> {
    let mut result = Value::default();
    merge(&mut result, value, b)?;
    Ok(result)
}
fn state_copy(s: &State, b: &mut Budget) -> CompileResult<State> {
    fee(
        b,
        s.locals
            .len()
            .saturating_add(s.external.len())
            .saturating_add(s.captures.len())
            .saturating_add(s.capture_views.len()),
    )?;
    let mut out = State::default();
    target_work(&s.vacant, b)?;
    out.vacant = s.vacant.clone();
    out.capture_views = s.capture_views.clone();
    out.receiver_loan = s.receiver_loan;
    if s.receiver_loan.is_some() {
        fee(b, 1)?;
    }
    for (local, value) in &s.locals {
        ordered(b, out.locals.len(), 0)?;
        out.locals.insert(*local, snapshot(value, b)?);
    }
    for (index, value) in &s.captures {
        ordered(b, out.captures.len(), 0)?;
        out.captures.insert(*index, snapshot(value, b)?);
    }
    for (index, slots) in &s.external {
        let mut charged = Value::default();
        for (path, value) in slots {
            ordered(b, charged.slots.len(), path.len())?;
            fee(b, path.len())?;
            union(charged.slots.entry(path.clone()).or_default(), value, b)?;
        }
        ordered(b, out.external.len(), 0)?;
        out.external.insert(*index, charged.slots);
    }
    Ok(out)
}
fn state_work(s: &State, b: &mut Budget) -> CompileResult<()> {
    target_work(&s.vacant, b)?;
    fee(
        b,
        s.locals
            .len()
            .saturating_add(s.external.len())
            .saturating_add(s.captures.len())
            .saturating_add(s.capture_views.len()),
    )?;
    for value in s.locals.values().chain(s.captures.values()) {
        value.enum_variants.work(b)?;
        value.option_variants.work(b)?;
        target_work(&value.held.targets, b)?;
        fee(b, value.held.loans.len())?;
        target_work(&value.pointer.targets, b)?;
        fee(b, value.pointer.loans.len())?;
        target_work(&value.store_description.targets, b)?;
        fee(b, value.store_description.loans.len())?;
        task_address::work(value, b)?;
        dyn_address::work(value, b)?;
        fee(b, value.environment.len())?;
        for (path, permission) in value.environment.iter().chain(&value.slots) {
            fee(
                b,
                path.len()
                    .saturating_add(permission.loans.len())
                    .saturating_add(1),
            )?;
            target_work(&permission.targets, b)?;
        }
    }
    for slots in s.external.values() {
        fee(b, slots.len())?;
        for (path, permission) in slots {
            fee(
                b,
                path.len()
                    .saturating_add(permission.loans.len())
                    .saturating_add(1),
            )?;
            target_work(&permission.targets, b)?;
        }
    }
    Ok(())
}

fn facts_work(f: &Facts, b: &mut Budget) -> CompileResult<()> {
    if b.byte_queries {
        byte_queries::work(&f.byte_reads, b)?;
        fee(b, 1)?;
    }
    if !f.package_return_calls.is_empty() {
        fee(
            b,
            f.package_return_calls
                .len()
                .saturating_mul(std::mem::size_of::<(ValueId, u32)>()),
        )?;
    }
    f.returned_enum_variants.work(b)?;
    for value in f.enum_values.values() {
        value.work(b)?;
    }
    f.returned_variants.work(b)?;
    if f.receiver_return {
        fee(b, 1)?;
    }
    if !f.returned_head_inputs.is_empty() {
        fee(b, f.returned_head_inputs.len())?;
    }
    if !f.capture_taken.is_empty() {
        fee(b, f.capture_taken.len())?;
    }
    for (_, path) in &f.external_stable {
        fee(b, path.len().saturating_add(1))?;
    }
    for path in &f.capture_stable {
        fee(b, path.len().saturating_add(1))?;
    }
    if !f.none_values.is_empty() {
        fee(b, f.none_values.len())?;
    }

    fee(
        b,
        f.values
            .len()
            .saturating_add(f.returned.len())
            .saturating_add(f.returned_environment.len())
            .saturating_add(f.capture_after.len())
            .saturating_add(f.external_after.len())
            .saturating_add(f.apart.len()),
    )?;
    for row in &f.values {
        for root in row {
            fee(b, target_width(root).saturating_add(1))?;
        }
    }
    if b.dyn_contexts {
        fee(
            b,
            f.returned_dyn_ports
                .len()
                .saturating_add(f.declared_dyn_return_upper.len())
                .saturating_add(1),
        )?;
        for (path, targets) in f
            .returned_dyn_ports
            .iter()
            .chain(&f.declared_dyn_return_upper)
        {
            fee(b, path.len().saturating_add(targets.len()))?;
            for target in targets {
                fee(b, target_width(target))?;
            }
        }
    }
    if b.task_sources() {
        fee(b, f.pointer_values.len())?;
        for row in &f.pointer_values {
            fee(b, row.len())?;
        }
        if b.masked_stored_tasks() {
            fee(b, f.pointer_parents.len())?;
            for row in &f.pointer_parents {
                fee(b, row.len())?;
                for anchor in row {
                    fee(
                        b,
                        match anchor {
                            TaskLoanAnchor::Value(_) => 1,
                            TaskLoanAnchor::Input(target) => target_width(target),
                        },
                    )?;
                }
            }
        }
        for (path, row) in &f.returned_task_ports {
            fee(b, path.len())?;
            fee(b, row.len())?;
            for target in row {
                fee(b, target_width(target))?;
            }
        }
    }
    for (path, row) in f
        .returned
        .iter()
        .chain(&f.returned_environment)
        .chain(&f.capture_after)
    {
        fee(b, path.len().saturating_add(row.len()))?;
        for root in row {
            fee(b, target_width(root).saturating_add(1))?;
        }
    }
    for ((_, path), row) in &f.external_after {
        fee(b, path.len().saturating_add(row.len()))?;
        for root in row {
            fee(b, target_width(root).saturating_add(1))?;
        }
    }
    for target in f.head_accesses.keys() {
        fee(b, target_width(target).saturating_add(1))?;
    }
    for (left, right) in &f.apart {
        fee(
            b,
            target_width(left)
                .saturating_add(target_width(right))
                .saturating_add(2),
        )?;
    }
    Ok(())
}
fn state_merge(to: &mut State, from: &State, b: &mut Budget) -> CompileResult<()> {
    if let Some(receiver) = from.receiver_loan {
        fee(b, 1)?;
        if to.receiver_loan.is_some_and(|current| current != receiver) {
            return Err(failure("joined receiver invocation identities differ"));
        }
        to.receiver_loan = Some(receiver);
    }
    for (local, index) in &from.capture_views {
        ordered(b, to.capture_views.len(), 0)?;
        if to
            .capture_views
            .insert(*local, *index)
            .is_some_and(|old| old != *index)
        {
            return Err(failure("capture view joins different environment fields"));
        }
    }
    for hole in &from.vacant {
        ordered(b, to.vacant.len(), target_width(hole))?;
        fee(b, target_width(hole))?;
        to.vacant.insert(hole.clone());
    }
    for (local, value) in &from.locals {
        ordered(b, to.locals.len(), 0)?;
        merge(to.locals.entry(*local).or_default(), value, b)?;
    }
    for (index, value) in &from.captures {
        ordered(b, to.captures.len(), 0)?;
        merge(to.captures.entry(*index).or_default(), value, b)?;
    }
    for (index, slots) in &from.external {
        ordered(b, to.external.len(), 0)?;
        let target = to.external.entry(*index).or_default();
        for (path, permission) in slots {
            ordered(b, target.len(), path.len())?;
            fee(b, path.len().saturating_add(1))?;
            union(target.entry(path.clone()).or_default(), permission, b)?;
        }
    }
    Ok(())
}

fn carrier(ty: &Type) -> Option<(&Type, &StorageLifeShape)> {
    if let Type::BorrowedStorage { storage, life } = ty {
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
    )
}
fn pointed_carrier(ty: &Type) -> Option<(&Type, &StorageLifeShape)> {
    if let Type::Address { inner, .. } = ty {
        carrier(inner)
    } else {
        None
    }
}
fn value_type(f: &TypedFunction, id: ValueId) -> CompileResult<&Type> {
    f.values
        .get(id.0 as usize)
        .ok_or_else(|| failure("value outside actual table"))
}
fn slot<'a>(ty: &'a Type, path: &[PlaceProjection], b: &mut Budget) -> CompileResult<&'a Type> {
    let mut ty = ty;
    for part in path {
        fee(b, 1)?;
        ty = carrier(ty).map_or(ty, |(storage, _)| storage);
        ty = match (ty, part) {
            (
                Type::Record { fields, .. } | Type::Package { fields, .. },
                PlaceProjection::Field(i),
            ) => fields.get(*i as usize).map(|(_, t)| t),
            (Type::Tuple(fields), PlaceProjection::Field(i)) => fields.get(*i as usize),
            (Type::Array { element, length }, PlaceProjection::Field(i)) if *i < *length => {
                Some(element.as_ref())
            }
            (Type::Enum { variants, .. }, PlaceProjection::Variant(p)) => variants
                .get(p.variant as usize)
                .and_then(|(_, fs)| fs.get(p.index as usize)),
            (Type::ErrorUnion { error, result }, PlaceProjection::ErrorArm(e)) => {
                Some(if *e { error.as_ref() } else { result.as_ref() })
            }
            _ => None,
        }
        .ok_or_else(|| failure("physical path crosses an absent or non-inline ancestor"))?;
    }
    Ok(ty)
}

fn take_package_field_slots(
    holder: &mut Value,
    index: u32,
    b: &mut Budget,
) -> CompileResult<BTreeMap<Path, Permission>> {
    fee(b, holder.slots.len())?;
    let prefix = PlaceProjection::Field(index);
    let mut paths = Vec::new();
    for path in holder.slots.keys() {
        fee(b, path.len().saturating_add(1))?;
        if path.first() == Some(&prefix) {
            paths.push(path.clone());
        }
    }
    let mut extracted = BTreeMap::new();
    for path in paths {
        let permission = holder
            .slots
            .remove(&path)
            .ok_or_else(|| failure("selected package field slot disappeared"))?;
        let relative = path[1..].to_vec();
        ordered(b, extracted.len(), relative.len())?;
        extracted.insert(relative, permission);
    }
    Ok(extracted)
}

pub(super) fn shape_work(ty: &Type, limits: &Limits, b: &mut Budget) -> CompileResult<()> {
    fee(b, 1)?;
    let mut pending = vec![(ty, 0usize)];
    while let Some((ty, depth)) = pending.pop() {
        fee(b, 1)?;
        if depth > limits.max_call_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Analyze,
                "stored type depth exceeds analysis limit",
                None,
            )]);
        }
        match ty {
            Type::BorrowedStorage { storage, life } => {
                fee(
                    b,
                    life.slots
                        .len()
                        .saturating_add(life.retained.len())
                        .saturating_add(life.nominal_arguments.len())
                        .saturating_add(1),
                )?;
                for row in &life.slots {
                    fee(b, row.path.len())?;
                }
                for row in &life.retained {
                    fee(b, row.path.len())?;
                }
                for row in &life.nominal_arguments {
                    fee(b, row.path.len())?;
                }
                pending.push((storage, depth + 1));
            }
            Type::Record { name, fields } | Type::Package { name, fields } => {
                fee(b, name.len().saturating_add(fields.len()))?;
                for (name, t) in fields {
                    fee(b, name.len())?;
                    pending.push((t, depth + 1));
                }
            }
            Type::Tuple(fields) => {
                fee(b, fields.len())?;
                for t in fields {
                    pending.push((t, depth + 1));
                }
            }
            Type::Enum { name, variants } => {
                fee(b, name.len().saturating_add(variants.len()))?;
                for (name, fields) in variants {
                    fee(b, name.len().saturating_add(fields.len()))?;
                    for t in fields {
                        pending.push((t, depth + 1));
                    }
                }
            }
            Type::AsyncRegistration { environment: inner }
            | Type::Address { inner, .. }
            | Type::Array { element: inner, .. }
            | Type::Slice { element: inner }
            | Type::Managed(inner)
            | Type::Desc(inner) => pending.push((inner, depth + 1)),
            Type::ErrorUnion { error, result } => {
                fee(b, 2)?;
                pending.push((error, depth + 1));
                pending.push((result, depth + 1));
            }
            Type::FunctionPointer { parameters, result }
            | Type::BorrowedFunctionPointer {
                parameters, result, ..
            } => {
                fee(b, parameters.len().saturating_add(1))?;
                for t in parameters {
                    pending.push((t, depth + 1));
                }
                pending.push((result, depth + 1));
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn qualification(
    module: &TypedModule,
    ty: &Type,
    limits: &Limits,
    b: &mut Budget,
) -> CompileResult<()> {
    shape_work(ty, limits, b)?;
    shape_work(ty, limits, b)?;
    physical(ty, b)?;
    let (storage, _) = carrier(ty).ok_or_else(|| failure("expected a qualified carrier"))?;
    if matches!(
        storage,
        Type::Record { .. } | Type::Package { .. } | Type::Enum { .. }
    ) {
        fee(b, module.life_instances.len())?;
        let mut present = false;
        for instance in &module.life_instances {
            shape_work(&instance.ty, limits, b)?;
            shape_work(ty, limits, b)?;
            if instance.ty == *ty {
                present = true;
            }
        }
        if !present {
            return Err(failure(
                "nominal carrier has no exact complete declaration/instance correspondence",
            ));
        }
    }
    Ok(())
}

/// Copy only a fully qualified physical value without runtime borrowing or
/// ownership leaves. The address is still checked by the CFG access proof.
pub(super) fn pure_read(module: &TypedModule, ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    if !b.pure_nominal
        || !matches!(ty, Type::BorrowedStorage { storage, .. } if matches!(storage.as_ref(), Type::Record { .. } | Type::Enum { .. }))
    {
        return Ok(false);
    }
    b.type_work(ty, true, "pure stored ReadAddress complete payload")?;
    if !retained::pure(ty, b)? {
        return Ok(false);
    }
    physical(ty, b)?;
    let mut count = 0usize;
    for instance in &module.life_instances {
        b.type_work(
            &instance.ty,
            false,
            "pure stored ReadAddress instance identity",
        )?;
        b.type_work(ty, false, "pure stored ReadAddress actual identity")?;
        if instance.ty == *ty {
            count += 1;
        }
    }
    if count != 1 {
        return Err(failure(
            "pure stored read lacks one complete nominal instance",
        ));
    }
    Ok(true)
}

/// Finite shared Tuples keep independently scoped child instances and all
/// physical targets. This never establishes a generic Copy capability.
pub(super) fn shared_compound(
    module: &TypedModule,
    ty: &Type,
    b: &mut Budget,
) -> CompileResult<bool> {
    if let Type::BorrowedStorage { storage, life } = ty
        && matches!(storage.as_ref(), Type::Tuple(_))
    {
        b.type_work(ty, true, "anonymous shared compound complete type")?;
        physical(ty, b)?;
        fee(b, life.slots.len())?;
        return Ok(!resources::sums::owned(storage)
            && life
                .slots
                .iter()
                .all(|slot| slot.mode == StoredBorrowMode::SharedAddress));
    }
    if !matches!(ty, Type::Tuple(_)) || ty.is_copy_value() {
        return Ok(false);
    }
    b.type_work(ty, true, "shared compound actual type")?;
    fn walk(module: &TypedModule, ty: &Type, b: &mut Budget) -> CompileResult<bool> {
        fee(b, 1)?;
        match ty {
            Type::Tuple(fields) => {
                for field in fields {
                    if !walk(module, field, b)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Type::BorrowedStorage { storage, life } => {
                if !matches!(storage.as_ref(), Type::Record { .. } | Type::Enum { .. }) {
                    return Ok(false);
                }
                physical(ty, b)?;
                fee(b, life.slots.len())?;
                if life
                    .slots
                    .iter()
                    .any(|slot| slot.mode != StoredBorrowMode::SharedAddress)
                {
                    return Ok(false);
                }
                let mut count = 0;
                for actual in &module.life_instances {
                    b.type_work(&actual.ty, false, "shared compound nominal candidate")?;
                    b.type_work(ty, false, "shared compound nominal actual")?;
                    count += usize::from(actual.ty == *ty);
                }
                if count != 1 {
                    return Err(failure(
                        "shared compound child lacks unique complete nominal instance",
                    ));
                }
                Ok(true)
            }
            other => Ok(other.is_copy_value()),
        }
    }
    walk(module, ty, b)
}
pub(super) fn affine_compound(
    module: &TypedModule,
    ty: &Type,
    b: &mut Budget,
) -> CompileResult<bool> {
    if let Type::BorrowedStorage { storage, life } = ty
        && matches!(storage.as_ref(), Type::Tuple(_))
    {
        b.type_work(ty, true, "anonymous affine compound complete type")?;
        physical(ty, b)?;
        fee(b, life.slots.len())?;
        return Ok(resources::sums::owned(storage)
            || life
                .slots
                .iter()
                .any(|slot| slot.mode == StoredBorrowMode::ExclusiveAddress));
    }
    if !matches!(ty, Type::Tuple(_)) || ty.is_copy_value() {
        return Ok(false);
    }
    b.type_work(ty, true, "affine compound complete type")?;
    fn walk(
        module: &TypedModule,
        ty: &Type,
        b: &mut Budget,
    ) -> CompileResult<Option<(bool, bool)>> {
        fee(b, 1)?;
        match ty {
            Type::Tuple(fields) => {
                let (mut owns, mut exclusive) = (false, false);
                for field in fields {
                    let Some((o, e)) = walk(module, field, b)? else {
                        return Ok(None);
                    };
                    owns |= o;
                    exclusive |= e;
                }
                Ok(Some((owns, exclusive)))
            }
            Type::BorrowedStorage { storage, life } => {
                if !matches!(
                    storage.as_ref(),
                    Type::Record { .. } | Type::Package { .. } | Type::Enum { .. }
                ) {
                    return Ok(None);
                }
                physical(ty, b)?;
                let mut count = 0;
                for actual in &module.life_instances {
                    b.type_work(&actual.ty, false, "affine compound candidate identity")?;
                    b.type_work(ty, false, "affine compound actual identity")?;
                    count += usize::from(actual.ty == *ty);
                }
                if count != 1 {
                    return Err(failure(
                        "affine compound child lacks unique complete nominal instance",
                    ));
                }
                fee(b, life.slots.len())?;
                Ok(Some((
                    resources::sums::owned(storage),
                    life.slots
                        .iter()
                        .any(|slot| slot.mode == StoredBorrowMode::ExclusiveAddress),
                )))
            }
            Type::Managed(payload)
                if matches!(
                    payload.as_ref(),
                    Type::Record { .. } | Type::ByteBuffer | Type::OwnedStore { .. }
                ) =>
            {
                if matches!(payload.as_ref(), Type::OwnedStore { .. }) {
                    crate::ownership_store::ordinary_owned(module, ty, b)?;
                } else {
                    crate::budget::legacy_value(ty, zil_base::Span::default(), module)?;
                }
                Ok(Some((true, false)))
            }
            other if other.is_copy_value() => Ok(Some((false, false))),
            _ => Ok(None),
        }
    }
    Ok(walk(module, ty, b)?.is_some_and(|(owns, exclusive)| owns || exclusive))
}
fn compound_move(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    let InstructionKind::TakeTupleField { local, path } = &i.kind else {
        return Ok(false);
    };
    let parent = &f
        .locals
        .get(local.0 as usize)
        .ok_or_else(|| failure("compound take local absent"))?
        .ty;
    if !affine_compound(module, parent, b)? {
        return Ok(false);
    }
    let child = slot(parent, path, b)?;
    if i.result.map(|result| value_type(f, result)).transpose()? != Some(child) {
        return Err(failure("compound take changes complete actual field type"));
    }
    Ok(true)
}
fn compound_extract(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    let InstructionKind::ExtractField { aggregate, index } = i.kind else {
        return Ok(false);
    };
    let parent = value_type(f, aggregate)?;
    if !shared_compound(module, parent, b)? {
        return Ok(false);
    }
    let physical = carrier(parent).map_or(parent, |(storage, _)| storage);
    let Type::Tuple(fields) = physical else {
        unreachable!()
    };
    let child = fields
        .get(index as usize)
        .ok_or_else(|| failure("compound extraction field absent"))?;
    let result = i
        .result
        .ok_or_else(|| failure("compound extraction result absent"))?;
    if value_type(f, result)? != child {
        return Err(failure("compound extraction changes complete child type"));
    }
    Ok(true)
}
fn compound_unpack(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    let InstructionKind::UnpackTuple { local, fields } = &i.kind else {
        return Ok(false);
    };
    let parent = &f
        .locals
        .get(local.0 as usize)
        .ok_or_else(|| failure("compound unpack local absent"))?
        .ty;
    if !affine_compound(module, parent, b)? && !shared_compound(module, parent, b)? {
        return Ok(false);
    }
    let physical = carrier(parent).map_or(parent, |(storage, _)| storage);
    let Type::Tuple(types) = physical else {
        unreachable!()
    };
    if i.result.is_some() || fields.len() != types.len() {
        return Err(failure("compound unpack omits complete fields"));
    }
    let mut seen = BTreeSet::new();
    for (field, expected) in fields.iter().zip(types) {
        ordered(b, seen.len(), 0)?;
        if field == local
            || !seen.insert(*field)
            || f.locals.get(field.0 as usize).map(|local| &local.ty) != Some(expected)
        {
            return Err(failure(
                "compound unpack changes or duplicates actual field place",
            ));
        }
    }
    Ok(true)
}
fn compound_drop(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    let InstructionKind::DropTuple { local } = i.kind else {
        return Ok(false);
    };
    let ty = &f
        .locals
        .get(local.0 as usize)
        .ok_or_else(|| failure("compound drop local absent"))?
        .ty;
    if !affine_compound(module, ty, b)? {
        return Ok(false);
    }
    if i.result.is_some() {
        return Err(failure("compound drop fabricates a result"));
    }
    Ok(true)
}
fn compound_constructor(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> CompileResult<bool> {
    let InstructionKind::MakeOwnedTuple { fields } = &i.kind else {
        return Ok(false);
    };
    let id = i
        .result
        .ok_or_else(|| failure("compound constructor result absent"))?;
    let ty = value_type(f, id)?;
    if !affine_compound(module, ty, b)? {
        return Ok(false);
    }
    let physical = carrier(ty).map_or(ty, |(storage, _)| storage);
    let Type::Tuple(types) = physical else {
        unreachable!()
    };
    if types.len() != fields.len() {
        return Err(failure("compound constructor omits actual fields"));
    }
    for (field, expected) in fields.iter().zip(types) {
        b.type_work(expected, false, "compound constructor complete field")?;
        if value_type(f, *field)? != expected {
            return Err(failure("compound constructor changes actual field Type"));
        }
    }
    Ok(true)
}
fn compound_slot_rows(
    ty: &Type,
    prefix: &mut Path,
    out: &mut Vec<StorageLifeSlot>,
    b: &mut Budget,
) -> CompileResult<()> {
    fee(b, 1)?;
    match ty {
        Type::Tuple(fields) => {
            for (i, field) in fields.iter().enumerate() {
                prefix.push(PlaceProjection::Field(
                    u32::try_from(i).map_err(|_| failure("compound field overflow"))?,
                ));
                compound_slot_rows(field, prefix, out, b)?;
                prefix.pop();
            }
        }
        Type::BorrowedStorage { life, .. } => {
            for slot in &life.slots {
                fee(
                    b,
                    prefix
                        .len()
                        .saturating_add(slot.path.len())
                        .saturating_add(1),
                )?;
                let mut path = prefix.clone();
                path.extend_from_slice(&slot.path);
                out.push(StorageLifeSlot {
                    path,
                    binder: slot.binder,
                    mode: slot.mode,
                });
            }
        }
        _ => {}
    }
    Ok(())
}

/// Pure inline products have no runtime targets. Every qualified descendant
/// still needs one complete, independently checked nominal instance.
pub(super) fn pure_inline(module: &TypedModule, ty: &Type, b: &mut Budget) -> CompileResult<bool> {
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
    b.type_work(ty, true, "pure inline complete payload")?;
    let mut pending = vec![ty];
    while let Some(current) = pending.pop() {
        fee(b, 1)?;
        match current {
            Type::BorrowedStorage { storage, .. } => {
                physical(current, b)?;
                let mut count = 0usize;
                for instance in &module.life_instances {
                    b.type_work(&instance.ty, false, "pure inline nominal identity")?;
                    b.type_work(current, false, "pure inline actual identity")?;
                    if instance.ty == *current {
                        count += 1;
                    }
                }
                if count != 1 {
                    return Err(failure(
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

fn identity_work(id: &LifeDeclarationIdentity, b: &mut Budget) -> CompileResult<()> {
    fee(
        b,
        id.package
            .len()
            .saturating_add(id.module_path.len())
            .saturating_add(id.declaration_path.len())
            .saturating_add(id.generation_path.len())
            .saturating_add(1),
    )?;
    for text in id.module_path.iter().chain(&id.declaration_path) {
        fee(b, text.len())?;
    }
    Ok(())
}

/// Inventory is untrusted identity data. It resolves full actual ordinals but
/// supplies no pointee, owner, initialization, or permission facts.
pub(super) fn inventory(
    module: &TypedModule,
    limits: &Limits,
    b: &mut Budget,
) -> CompileResult<()> {
    if module.life_declarations.len() > limits.max_nodes
        || module.life_instances.len() > limits.max_nodes
    {
        return Err(vec![Diagnostic::limit(
            Phase::Analyze,
            "stored inventory count exceeds analysis limit",
            None,
        )]);
    }
    fee(
        b,
        module
            .life_declarations
            .len()
            .saturating_add(module.life_instances.len()),
    )?;
    for (index, decl) in module.life_declarations.iter().enumerate() {
        identity_work(&decl.identity, b)?;
        if task_failure::declaration(decl) {
            b.task_failure_carriers = true;
        }
        if enum_address::poll_reserved(decl) && !enum_address::poll_declaration(decl) {
            return Err(failure("reserved Poll declaration domain is not authentic"));
        }
        if decl.identity.package.is_empty()
            || decl.identity.declaration_path.is_empty()
            || decl
                .identity
                .module_path
                .iter()
                .chain(&decl.identity.declaration_path)
                .any(String::is_empty)
        {
            return Err(failure(
                "nominal declaration lacks its full stable identity",
            ));
        }
        fee(b, decl.parameters.len())?;
        for formal in &decl.parameters {
            if let LifeParameterKind::Const(ty) = formal {
                shape_work(ty, limits, b)?;
                b.type_work(ty, true, "nominal const parameter kind")?;
                if !ty.is_copy_value() {
                    return Err(failure("nominal const parameter has invalid data type"));
                }
            }
        }
        for earlier in &module.life_declarations[..index] {
            identity_work(&earlier.identity, b)?;
            identity_work(&decl.identity, b)?;
            fee(b, 1)?;
            if decl.id == earlier.id || decl.identity == earlier.identity {
                return Err(failure("duplicate declaration ID or stable identity"));
            }
        }
    }
    if b.task_failure_carriers && b.task_input_scopes.is_none() && !module.closures.is_empty() {
        b.task_input_scopes = Some(crate::tasks::input_scope::build(module, limits, b)?);
    }
    for (index, instance) in module.life_instances.iter().enumerate() {
        shape_work(&instance.ty, limits, b)?;
        if matches!(&instance.ty, Type::BorrowedStorage { life, .. } if life.slots.is_empty()) {
            b.pure_nominal = true;
        }
        let Some((storage, _)) = carrier(&instance.ty) else {
            return Err(failure(
                "nominal stored instance has no complete qualified type",
            ));
        };
        physical(&instance.ty, b)?;
        b.stored_packages |= matches!(storage, Type::Package { .. });
        fee(b, module.life_declarations.len())?;
        let decl = module
            .life_declarations
            .iter()
            .find(|d| d.id == instance.declaration)
            .ok_or_else(|| failure("nominal instance references absent declaration"))?;
        if matches!(storage, Type::Enum { name, .. } if name.starts_with("@std.task:v1:Poll:"))
            && !enum_address::poll_declaration(decl)
            && !task_failure::declaration(decl)
        {
            return Err(failure(
                "reserved Poll physical domain lacks its authentic declaration",
            ));
        }
        if !matches!(
            (decl.kind, storage),
            (LifeNominalKind::Record, Type::Record { .. })
                | (LifeNominalKind::Package, Type::Package { .. })
                | (LifeNominalKind::Enum, Type::Enum { .. })
        ) {
            if enum_address::candidate(&instance.ty) {
                return Err(failure(
                    "borrowed Enum declaration kind differs from its physical instance",
                ));
            }
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "nominal stored kind requires its actual physical consumer",
                None,
            )]);
        }
        fee(
            b,
            instance
                .arguments
                .len()
                .saturating_add(decl.parameters.len()),
        )?;
        if instance.arguments.len() != decl.parameters.len() {
            return Err(failure(
                "nominal instance omits full generic actual ordinals",
            ));
        }
        nominal::rows(module, instance, limits, b)?;
        b.stored_enum_addresses |= (!decl.id.compiler || enum_address::poll_declaration(decl))
            && enum_address::candidate(&instance.ty);
        for earlier in &module.life_instances[..index] {
            shape_work(&earlier.ty, limits, b)?;
            shape_work(&instance.ty, limits, b)?;
            if earlier.ty == instance.ty
                || (earlier.declaration == instance.declaration
                    && nominal::same_key(earlier, instance, limits, b)?)
            {
                return Err(failure("duplicate complete nominal instance"));
            }
        }
    }
    Ok(())
}

fn binding(
    f: &TypedFunction,
    base: StorageBase,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> CompileResult<StorageBinder> {
    let c = f
        .storage_life
        .as_ref()
        .ok_or_else(|| failure("stored signature lacks complete relation"))?;
    if path.is_empty() {
        fee(b, c.bindings.len())?;
        return super::signature_life::outer(c, base)
            .ok_or_else(|| failure("signature omits actual outer borrow"));
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
    .ok_or_else(|| failure("stored relation base is absent"))?;
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
            return Err(failure("array relation has no actual bounded element"));
        }
        fee(b, c.bindings.len().saturating_mul(2))?;
        return c.bindings.iter().find(|row| row.position.base == base
            && matches!(row.position.path.as_slice(), [ContractProjection::Place(step)] if *step == path[0]))
            .map(|row| row.binder).ok_or_else(|| failure("array element lacks its actual signature binding"));
    }
    let mut boundary = ty;
    let mut leaf_path = path;
    let mut class_path = Vec::new();
    loop {
        match boundary {
            Type::Tuple(fields) => {
                fee(b, 1)?;
                let Some(PlaceProjection::Field(index)) = leaf_path.first() else {
                    return Err(failure("compound relation omits its actual field boundary"));
                };
                boundary = fields
                    .get(*index as usize)
                    .ok_or_else(|| failure("compound relation selects absent actual field"))?;
                class_path.push(LifeTypeProjection::Place(PlaceProjection::Field(*index)));
                leaf_path = &leaf_path[1..];
            }
            Type::ErrorUnion { error, result } if b.stored_enum_addresses => {
                fee(b, 1)?;
                let Some(PlaceProjection::ErrorArm(arm)) = leaf_path.first() else {
                    return Err(failure(
                        "stored sum relation omits its actual ErrorArm boundary",
                    ));
                };
                class_path.push(LifeTypeProjection::Place(PlaceProjection::ErrorArm(*arm)));
                boundary = if *arm {
                    error.as_ref()
                } else {
                    result.as_ref()
                };
                leaf_path = &leaf_path[1..];
            }
            _ => break,
        }
    }
    let ((_, shape), pointee) = if let Some(q) = carrier(boundary) {
        (q, false)
    } else {
        (
            pointed_carrier(boundary)
                .ok_or_else(|| failure("physical leaf has no actual qualified boundary"))?,
            true,
        )
    };
    let (local, duplicate) = b
        .physical_slot(shape, leaf_path)?
        .ok_or_else(|| failure("relation selected an absent physical stored leaf"))?;
    if duplicate {
        return Err(failure("duplicate actual stored leaf"));
    }
    if pointee && !class_path.is_empty() {
        class_path.insert(0, LifeTypeProjection::Pointee);
    }
    fee(b, c.classes.len())?;
    for row in &c.classes {
        fee(b, row.position.path.len().saturating_add(1))?;
        if row.position.base == base
            && row.position.class == local
            && if class_path.is_empty() {
                if pointee {
                    row.position.path.as_slice() == [LifeTypeProjection::Pointee]
                } else {
                    row.position.path.is_empty()
                }
            } else {
                row.position.path == class_path
            }
        {
            return Ok(row.binder);
        }
    }
    Err(failure("signature omits actual qualified local class"))
}
#[cfg(test)]
fn binding_by_scan(
    f: &TypedFunction,
    base: StorageBase,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> CompileResult<StorageBinder> {
    let c = f
        .storage_life
        .as_ref()
        .ok_or_else(|| failure("stored signature lacks complete relation"))?;
    if path.is_empty() {
        fee(b, c.bindings.len())?;
        return super::signature_life::outer(c, base)
            .ok_or_else(|| failure("signature omits actual outer borrow"));
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
    .ok_or_else(|| failure("stored relation base is absent"))?;
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
            return Err(failure("array relation has no actual bounded element"));
        }
        fee(b, c.bindings.len().saturating_mul(2))?;
        return c.bindings.iter().find(|row| row.position.base == base
            && matches!(row.position.path.as_slice(), [ContractProjection::Place(step)] if *step == path[0]))
            .map(|row| row.binder).ok_or_else(|| failure("array element lacks its actual signature binding"));
    }
    let mut boundary = ty;
    let mut leaf_path = path;
    let mut class_path = Vec::new();
    loop {
        match boundary {
            Type::Tuple(fields) => {
                fee(b, 1)?;
                let Some(PlaceProjection::Field(index)) = leaf_path.first() else {
                    return Err(failure("compound relation omits its actual field boundary"));
                };
                boundary = fields
                    .get(*index as usize)
                    .ok_or_else(|| failure("compound relation selects absent actual field"))?;
                class_path.push(LifeTypeProjection::Place(PlaceProjection::Field(*index)));
                leaf_path = &leaf_path[1..];
            }
            Type::ErrorUnion { error, result } if b.stored_enum_addresses => {
                fee(b, 1)?;
                let Some(PlaceProjection::ErrorArm(arm)) = leaf_path.first() else {
                    return Err(failure(
                        "stored sum relation omits its actual ErrorArm boundary",
                    ));
                };
                class_path.push(LifeTypeProjection::Place(PlaceProjection::ErrorArm(*arm)));
                boundary = if *arm {
                    error.as_ref()
                } else {
                    result.as_ref()
                };
                leaf_path = &leaf_path[1..];
            }
            _ => break,
        }
    }
    let ((_, shape), pointee) = if let Some(q) = carrier(boundary) {
        (q, false)
    } else {
        (
            pointed_carrier(boundary)
                .ok_or_else(|| failure("physical leaf has no actual qualified boundary"))?,
            true,
        )
    };
    fee(b, shape.slots.len())?;
    let mut local = None;
    for slot in &shape.slots {
        fee(
            b,
            slot.path.len().saturating_add(path.len()).saturating_add(1),
        )?;
        if slot.path == leaf_path && local.replace(slot.binder).is_some() {
            return Err(failure("duplicate actual stored leaf"));
        }
    }
    let local = local.ok_or_else(|| failure("relation selected an absent physical stored leaf"))?;
    if pointee && !class_path.is_empty() {
        class_path.insert(0, LifeTypeProjection::Pointee);
    }
    fee(b, c.classes.len())?;
    for row in &c.classes {
        fee(b, row.position.path.len().saturating_add(1))?;
        if row.position.base == base
            && row.position.class == local
            && if class_path.is_empty() {
                if pointee {
                    row.position.path.as_slice() == [LifeTypeProjection::Pointee]
                } else {
                    row.position.path.is_empty()
                }
            } else {
                row.position.path == class_path
            }
        {
            return Ok(row.binder);
        }
    }
    Err(failure("signature omits actual qualified local class"))
}

pub(super) fn header(module: &TypedModule, f: &TypedFunction, b: &mut Budget) -> CompileResult<()> {
    fn nested_class(ty: &Type, depth: usize, b: &mut Budget) -> CompileResult<bool> {
        if depth > b.partial_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Analyze,
                "stored signature class depth exceeds limit",
                None,
            )]);
        }
        b.spend(1, "stored signature actual nested class inventory")?;
        match ty {
            Type::BorrowedStorage { .. } => Ok(true),
            Type::Address { inner, .. } | Type::Array { element: inner, .. } => {
                nested_class(inner, depth + 1, b)
            }
            Type::Tuple(fields) => {
                b.spend(fields.len(), "stored signature tuple class inventory")?;
                for field in fields {
                    if nested_class(field, depth + 1, b)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Type::Record { fields, .. } => {
                b.spend(fields.len(), "stored signature record class inventory")?;
                for (_, field) in fields {
                    if nested_class(field, depth + 1, b)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Type::Enum { variants, .. } => {
                b.spend(variants.len(), "stored signature variant class inventory")?;
                for (_, fields) in variants {
                    b.spend(fields.len(), "stored signature payload class inventory")?;
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
        fee(b, 1)?;
        if f.locals.get(p.0 as usize).is_none() {
            return Err(failure("signature parameter outside actual local table"));
        }
        byte_companion |= byte_queries::description(&f.locals[p.0 as usize].ty);
    }
    if super::stored_callables::fixed_value(module, &f.return_type)
        && f.callable_return_contract.is_none()
    {
        return Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "captured Fixed environment escape needs its retained result target transfer",
            Some(f.span),
        )]);
    }
    let Some(c) = &f.storage_life else {
        if b.stored_enum_addresses
            && f.parameters
                .iter()
                .map(|l| &f.locals[l.0 as usize].ty)
                .chain(std::iter::once(&f.return_type))
                .any(|ty| matches!(ty, Type::ErrorUnion { .. }) && enum_address::signature(ty))
        {
            return Err(failure(
                "borrowed sum wrapper signature omits its complete relation",
            ));
        }
        for ty in f
            .parameters
            .iter()
            .map(|p| &f.locals[p.0 as usize].ty)
            .chain(std::iter::once(&f.return_type))
        {
            fee(b, 1)?;
            if f.environment.is_none()
                && (carrier(ty).is_some()
                    || pointed_carrier(ty).is_some()
                    || shared_array_value(ty)
                    || matches!(ty, Type::Address { inner, .. } if shared_array_value(inner))
                    || super::stored_callables::fixed_value(module, ty))
            {
                return Err(failure("stored signature requires complete class relation"));
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
                return Err(failure(
                    "nested stored qualified signature requires complete class relation",
                ));
            }
        }
        return Ok(());
    };
    for ty in f
        .parameters
        .iter()
        .map(|p| &f.locals[p.0 as usize].ty)
        .chain(std::iter::once(&f.return_type))
    {
        if option_address::candidate(ty) {
            fee(b, 96)?;
        }
    }
    super::signature_life::charge(c, b)?;
    fee(b, f.parameters.len())?;
    if f.reference_return_bound.is_some() || f.foreign.is_some() {
        return Err(failure(
            "stored signature contradicts another active profile",
        ));
    }
    let mut parameters = Vec::with_capacity(f.parameters.len());
    for p in &f.parameters {
        let ty = &f.locals[p.0 as usize].ty;
        b.type_work(ty, true, "stored signature type clone and relation")?;
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
            b.type_work(ty, false, "pure Array signature relation walk")?;
        }
        if f.callable_contract.is_some()
            && let Type::Managed(payload) = ty
            && let Type::Record { fields, .. } = payload.as_ref()
        {
            fee(b, fields.len().saturating_mul(2))?;
        }
        parameters.push(ty.clone());
    }
    b.type_work(&f.return_type, true, "stored signature result relation")?;
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
        b.type_work(&f.return_type, false, "pure Array result relation walk")?;
    }
    if let Some(id) = f.environment {
        let d = module
            .closures
            .get(id.0 as usize)
            .ok_or_else(|| failure("captured entry definition is absent"))?;
        super::signature_life::charge(c, b)?;
        if d.storage_life.as_ref() != Some(c) {
            return Err(failure(
                "captured entry relation differs from actual definition",
            ));
        }
        super::borrowed_fp::qualified_array_relation_work(&d.captures, &Type::Unit, b)?;
        return super::signature_life::closure_relation(
            &module.closures,
            d.kind,
            &parameters,
            &d.captures,
            super::stored_callables::life_result(d),
            c,
        );
    }
    let byte_signature = byte_companion
        && !byte_queries::legacy_task_signature(f)
        && f.call_kind == CallKind::Direct
        && f.foreign.is_none()
        && f.environment.is_none()
        && byte_queries::signature_presence(module, b)?;
    if byte_signature {
        fee(b, parameters.len())?;
    }
    let result = if f.callable_return_contract.is_some()
        && !c
            .classes
            .iter()
            .any(|row| row.position.base == StorageBase::Result)
    {
        &Type::Unit
    } else {
        &f.return_type
    };
    super::signature_life::function_relation_in_module(
        &parameters,
        result,
        c,
        f.callable_contract.is_some() || f.callable_return_contract.is_some(),
        module,
        byte_signature,
    )?;
    Ok(())
}

fn returns(
    module: &TypedModule,
    f: &TypedFunction,
    facts: &Facts,
    b: &mut Budget,
) -> CompileResult<()> {
    if let Some(d) = f
        .environment
        .and_then(|id| module.closures.get(id.0 as usize))
        && (super::stored_callables::lending(d) || super::stored_callables::transferred(d))
    {
        if receiver_results::applies(d) {
            if super::stored_callables::result_is_receiver(d) && !facts.receiver_return {
                return Err(failure(
                    "receiver-bounded result lacks actual invocation-head ancestry on every return",
                ));
            }
            let leaves = capture_after::leaves(module, d, b)?;
            let mut actual = BTreeSet::new();
            for (path, sources) in &facts.returned {
                if !path.is_empty() || sources.is_empty() {
                    return Err(failure(
                        "returned external view has no actual complete source",
                    ));
                }
                for source in sources {
                    let HoldRoot::Capture(index) = source.base else {
                        return Err(failure(
                            "returned environment view has no actual capture source",
                        ));
                    };
                    let ty = d
                        .captures
                        .get(index as usize)
                        .ok_or_else(|| failure("returned environment source capture absent"))?;
                    if super::stored_callables::transferred(d)
                        && super::stored_callables::exclusive_capture(ty)
                        && !facts.capture_taken.contains(&index)
                    {
                        return Err(failure(
                            "full Fixed exclusive result did not uniquely take its captured capability",
                        ));
                    }
                    if !super::stored_callables::scalar(ty) {
                        let key = environment_path(index as usize, &source.stored, b)?;
                        fee(b, leaves.len().saturating_mul(key.len().saturating_add(1)))?;
                        if !leaves.iter().any(|(path, _)| *path == key) {
                            return Err(failure(
                                "returned environment source is not an authenticated leaf",
                            ));
                        }
                    } else if !super::stored_callables::result_is_receiver(d) {
                        return Err(failure(
                            "inline environment storage cannot supply a Fixed external result",
                        ));
                    }
                    actual.insert(index);
                }
            }
            if actual.into_iter().collect::<Vec<_>>() != d.lending_captures {
                return Err(failure(
                    "returned environment view differs from its actual capture upper",
                ));
            }
            return Ok(());
        }
        fee(b, d.lending_captures.len().saturating_add(1))?;
        let mut actual = BTreeSet::new();
        for (path, sources) in &facts.returned {
            if !path.is_empty() || sources.is_empty() {
                return Err(failure(
                    "lending result lacks actual inline capture sources",
                ));
            }
            for source in sources {
                fee(b, target_width(source).saturating_add(1))?;
                let HoldRoot::Capture(index) = source.base else {
                    return Err(failure(
                        "environment result escapes local or explicit input storage",
                    ));
                };
                if !source.place.is_empty()
                    || !source.stored.is_empty()
                    || !d
                        .captures
                        .get(index as usize)
                        .is_some_and(super::stored_callables::scalar)
                {
                    return Err(failure(
                        "lending source is not an actual inline scalar capture",
                    ));
                }
                ordered(b, actual.len(), 0)?;
                actual.insert(index);
            }
        }
        if actual.len() != d.lending_captures.len()
            || actual
                .into_iter()
                .zip(&d.lending_captures)
                .any(|(actual, proposed)| actual != *proposed)
        {
            return Err(failure(
                "lending capture proposal differs from actual returned environment fields",
            ));
        }
        return Ok(());
    }
    for (path, sources) in &facts.returned {
        if sources.is_empty() {
            return Err(failure(
                "stored/reference result has no actual input target; recursive cycles grant no roots",
            ));
        }
        if let Some(bound) = &f.reference_return_bound {
            fee(
                b,
                sources.len().saturating_mul(bound.len().saturating_add(1)),
            )?;
            if sources.iter().any(|s| {
                !s.stored.is_empty()
                    || (s.base != HoldRoot::Static && !matches!(s.base,HoldRoot::Parameter(p)if bound.binary_search(&p).is_ok()))
            }) {
                return Err(failure(
                    "actual returned target exceeds the existing direct reference upper",
                ));
            }
        }
        if f.storage_life.is_none() {
            continue;
        }
        let upper = binding(f, StorageBase::Result, path, b)?;
        for source in sources {
            if source.base == HoldRoot::Static {
                if !source.stored.is_empty() || !source.place.is_empty() {
                    return Err(failure(
                        "static UTF8 result cannot retain nested projections",
                    ));
                }
                continue;
            }
            let HoldRoot::Parameter(index) = source.base else {
                return Err(failure("result retains local/captured target"));
            };
            if (if source.retained_port.is_some() {
                task_address::source_binding(f, source, b)?
            } else {
                environment_memory::binding(module, f, index, &source.stored, b)?
            }) != upper
            {
                let bounded_head = matches!(upper, StorageBinder::Invocation(_))
                    && facts.returned_head_inputs.iter().any(|position| {
                        f.parameters.get(*position as usize).is_some_and(|local| {
                            matches!(&f.locals[local.0 as usize].ty,Type::Address{mutable,inner}
                            if matches!(inner.as_ref(),Type::Closure(_))
                            && (!matches!(f.return_type,Type::Address{mutable:true,..})||*mutable))
                        }) && f.storage_life.as_ref().and_then(|life| {
                            super::signature_life::outer(life, StorageBase::Parameter(*position))
                        }) == Some(upper)
                    });
                if !bounded_head {
                    return Err(failure(
                        "actual returned target exceeds its declared stored/fresh relation",
                    ));
                }
            }
        }
    }
    Ok(())
}

/// This first executable slice admits finite physical shared leaves. Other
/// common retained carriers remain closed instead of acquiring guessed roots.
pub(super) fn physical(ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    let Some((storage, life)) = carrier(ty) else {
        return Ok(false);
    };
    fee(b, life.slots.len().saturating_add(1))?;
    if b.task_failure_carriers && task_failure::shape(ty, b)? {
        return Ok(true);
    }
    if option_address::shape(ty, b)? {
        return Ok(true);
    }
    if enum_address::shape(ty, b)? {
        return Ok(true);
    }
    if !matches!(
        storage,
        Type::Record { .. } | Type::Package { .. } | Type::Tuple(_) | Type::Enum { .. }
    ) || (matches!(storage, Type::Enum { .. })
        && (!life.slots.is_empty() || !retained::pure(storage, b)?))
    {
        return Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "nested retained/nominal stored carriers require their independent consumer",
            None,
        )]);
    }
    let mut actual = Vec::new();
    fn walk(
        ty: &Type,
        path: &mut Path,
        out: &mut Vec<ActualStoredSlot>,
        b: &mut Budget,
        owning: bool,
    ) -> CompileResult<()> {
        fee(b, 1)?;
        match ty {
            Type::Address { mutable, inner } => {
                b.type_work(inner, true, "stored shared actual pointee")?;
                if !inner.is_address_pointee() {
                    return Err(failure(
                        "stored shared leaf has an ineligible actual pointee",
                    ));
                }
                fee(b, path.len().saturating_add(1))?;
                out.push((
                    path.clone(),
                    if *mutable {
                        StoredBorrowMode::ExclusiveAddress
                    } else {
                        StoredBorrowMode::SharedAddress
                    },
                    None,
                ));
            }
            Type::Record { name, fields } | Type::Package { name, fields } => {
                fee(b, name.len())?;
                if name.is_empty() {
                    return Err(failure("stored physical record has empty nominal identity"));
                }
                let mut names = BTreeSet::new();
                for (i, (name, t)) in fields.iter().enumerate() {
                    ordered(b, names.len(), name.len())?;
                    if name.is_empty() || !names.insert(name) {
                        return Err(failure(
                            "stored record fields require unique nonempty actual names",
                        ));
                    }
                    fee(b, 1)?;
                    path.push(PlaceProjection::Field(i as u32));
                    walk(t, path, out, b, owning)?;
                    path.pop();
                }
            }
            Type::Tuple(fields) => {
                for (i, t) in fields.iter().enumerate() {
                    fee(b, 1)?;
                    path.push(PlaceProjection::Field(i as u32));
                    walk(t, path, out, b, owning)?;
                    path.pop();
                }
            }
            Type::Enum { name, variants } => {
                if !retained::pure(ty, b)? {
                    return Err(failure(
                        "stored Enum has a real address or ownership payload",
                    ));
                }
                fee(b, name.len())?;
                if name.is_empty() {
                    return Err(failure("stored Enum has empty nominal identity"));
                }
                let mut names = BTreeSet::new();
                for (variant, (variant_name, fields)) in variants.iter().enumerate() {
                    ordered(b, names.len(), variant_name.len())?;
                    if variant_name.is_empty() || !names.insert(variant_name) {
                        return Err(failure(
                            "stored Enum variants require unique nonempty names",
                        ));
                    }
                    for (index, field) in fields.iter().enumerate() {
                        path.push(PlaceProjection::Variant(zil_ir::VariantProjection {
                            variant: variant as u32,
                            index: index as u32,
                        }));
                        walk(field, path, out, b, owning)?;
                        path.pop();
                    }
                }
            }
            Type::Array { element, length } => {
                if shared_array_shape(ty, b)? {
                    fee(
                        b,
                        (*length as usize).saturating_mul(path.len().saturating_add(2)),
                    )?;
                    for index in 0..*length {
                        path.push(PlaceProjection::Field(index));
                        walk(element, path, out, b, owning)?;
                        path.pop();
                    }
                    return Ok(());
                }
                if !retained::pure(ty, b)? {
                    return Err(failure(
                        "stored Array contains an address or ownership leaf",
                    ));
                }
                walk(element, path, out, b, owning)?;
            }
            Type::BorrowedStorage { storage, life } => {
                if !owning
                    || (!matches!(storage.as_ref(), Type::Package { .. } | Type::Tuple(_))
                        && !shared_record_child(ty, b)?)
                {
                    if !retained::pure(ty, b)? {
                        return Err(failure("nested physical child requires an owning Package"));
                    }
                    return Ok(());
                }
                physical(ty, b)?;
                let start = out.len();
                let prefix = path.len();
                walk(
                    storage,
                    path,
                    out,
                    b,
                    matches!(storage.as_ref(), Type::Package { .. } | Type::Tuple(_)),
                )?;
                let boundary: Vec<_> = path
                    .iter()
                    .copied()
                    .map(LifeTypeProjection::Place)
                    .collect();
                for (actual, _, class) in &mut out[start..] {
                    let relative = actual
                        .get(prefix..)
                        .ok_or_else(|| failure("nested physical slot lost prefix"))?;
                    let child = life
                        .slots
                        .iter()
                        .find(|slot| slot.path == relative)
                        .ok_or_else(|| failure("nested physical slot lacks child class"))?;
                    *class = Some((boundary.clone(), child.binder));
                }
            }
            Type::Managed(payload) if owning => {
                b.type_work(payload, true, "stored Package actual owning resource")?;
                if !matches!(
                    payload.as_ref(),
                    Type::ByteBuffer | Type::Record { .. } | Type::OwnedStore { .. }
                ) {
                    return Err(failure(
                        "stored Package owning leaf is outside the actual finite resource profile",
                    ));
                }
            }
            t => {
                b.type_work(t, true, "stored ordinary physical field")?;
                if !t.is_copy_value() {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Analyze,
                        "nonshared physical stored leaf is not yet implemented",
                        None,
                    )]);
                }
            }
        }
        Ok(())
    }
    walk(
        storage,
        &mut Vec::new(),
        &mut actual,
        b,
        matches!(storage, Type::Package { .. } | Type::Tuple(_)),
    )?;
    if actual.len() != life.slots.len()
        || (actual.is_empty() && life.nominal_arguments.is_empty() && life.retained.is_empty())
    {
        return Err(failure("physical stored slot coverage is incomplete"));
    }
    let mut seen = BTreeSet::new();
    let mut next = 0u32;
    for row in &life.nominal_arguments {
        fee(b, row.path.len().saturating_add(1))?;
        if row.binder >= life.binder_count {
            return Err(failure(
                "nested nominal stored relation is not supported by the physical slice",
            ));
        }
        ordered(b, seen.len(), 0)?;
        if seen.insert(row.binder) {
            if row.binder != next {
                return Err(failure(
                    "nominal lifetime binders are not dense by first occurrence",
                ));
            }
            next = next
                .checked_add(1)
                .ok_or_else(|| failure("binder overflow"))?;
        }
    }
    for ((path, mode, child_class), decl) in actual.iter().zip(&life.slots) {
        fee(b, path.len().saturating_add(decl.path.len()))?;
        if path != &decl.path || mode != &decl.mode || decl.binder >= life.binder_count {
            return Err(failure(
                "stored slot order, mode or binder differs from actual fields",
            ));
        }
        if let Some((boundary, class)) = child_class
            && !life.retained.iter().any(|row| {
                &row.path == boundary && row.class == *class && row.binder == decl.binder
            })
        {
            return Err(failure(
                "nested slot binder differs from retained child class",
            ));
        }
        if *mode == StoredBorrowMode::ExclusiveAddress
            && (!matches!(
                storage,
                Type::Record { .. } | Type::Package { .. } | Type::Tuple(_)
            ) || !path
                .iter()
                .all(|part| matches!(part, PlaceProjection::Field(_)))
                || (child_class.is_none() && path.len() != 1))
        {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "exclusive stored holder requires exact inline address slots",
                None,
            )]);
        }
        // Only the new owning or disjoint exclusive holders need narrower
        // inline-slot windows. Shared-only and single exclusive legacy holders
        // retain their whole-carrier access and incur no projection work.
        b.stored_projected_slots |= matches!(storage, Type::Package { .. })
            || (*mode == StoredBorrowMode::ExclusiveAddress && life.slots.len() > 1);
        ordered(b, seen.len(), 0)?;
        if seen.insert(decl.binder) {
            if decl.binder != next {
                return Err(failure("stored binders are not dense by first occurrence"));
            }
            next = next
                .checked_add(1)
                .ok_or_else(|| failure("binder overflow"))?;
        }
    }
    let actual_retained = retained::expected(storage, b)?;
    fee(b, life.retained.len())?;
    if actual_retained.len() != life.retained.len() {
        return Err(failure("physical retained class coverage differs"));
    }
    for ((path, class), row) in actual_retained.iter().zip(&life.retained) {
        fee(
            b,
            path.len()
                .saturating_add(row.path.len())
                .saturating_add(seen.len())
                .saturating_add(1),
        )?;
        if path != &row.path || *class != row.class || row.binder >= life.binder_count {
            return Err(failure(
                "retained row differs from actual physical boundary/class",
            ));
        }
        if seen.insert(row.binder) {
            if row.binder != next {
                return Err(failure("retained classes are not canonical"));
            }
            next = next
                .checked_add(1)
                .ok_or_else(|| failure("retained class overflow"))?;
        }
    }
    if next != life.binder_count {
        return Err(failure("stored binder domain has unused classes"));
    }
    Ok(true)
}

fn environment_leaf(
    value: &mut Value,
    path: Path,
    permission: &Permission,
    b: &mut Budget,
) -> CompileResult<()> {
    union(&mut value.held, permission, b)?;
    ordered(b, value.environment.len(), path.len())?;
    union(value.environment.entry(path).or_default(), permission, b)?;
    Ok(())
}
fn environment_path(
    capture: usize,
    leaf: &[PlaceProjection],
    b: &mut Budget,
) -> CompileResult<Path> {
    fee(b, leaf.len().saturating_add(1))?;
    let index = u32::try_from(capture).map_err(|_| failure("capture payload ordinal overflow"))?;
    let mut path = Vec::with_capacity(leaf.len().saturating_add(1));
    path.push(PlaceProjection::Field(index));
    path.extend_from_slice(leaf);
    Ok(path)
}
fn environment_value(module: &TypedModule, ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    if super::stored_callables::fixed_value(module, ty) {
        return Ok(true);
    }
    if b.returned_borrowed_environments.is_empty() {
        return Ok(false);
    }
    let ty = if let Type::Address { inner, .. } = ty {
        inner.as_ref()
    } else {
        ty
    };
    let Type::Closure(id) = ty else {
        return Ok(false);
    };
    b.lookup(
        b.returned_borrowed_environments.len(),
        "actual returned environment identity lookup",
    )?;
    Ok(b.returned_borrowed_environments.contains(id))
}

fn seed_environment(
    module: &TypedModule,
    ty: &Type,
    base: HoldRoot,
    value: &mut Value,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> CompileResult<()> {
    let ty = if let Type::Address { inner, .. } = ty {
        inner.as_ref()
    } else {
        ty
    };
    let Type::Closure(id) = ty else { return Ok(()) };
    let d = module
        .closures
        .get(id.0 as usize)
        .ok_or_else(|| failure("environment parameter definition absent"))?;
    if d.storage_life.is_none() && !b.returned_borrowed_environments.contains(id) {
        return Ok(());
    }
    fee(b, d.captures.len())?;
    let mut add =
        |path: Path, mutable: bool, value: &mut Value, b: &mut Budget| -> CompileResult<()> {
            fee(b, path.len().saturating_mul(2).saturating_add(3))?;
            let target = Target {
                retained_port: None,
                base,
                stored: path.clone(),
                place: Vec::new(),
                heap_parents: Vec::new(),
                heap_element: None,
            };
            let id = loans.len();
            loans.push(Loan {
                targets: BTreeSet::from([target.clone()]),
                parents: Loans::new(),
                mutable,
                receiver_head: false,
            });
            b.stored_exclusive_loans |= mutable;
            let permission = Permission {
                targets: BTreeSet::from([target]),
                loans: Loans::from([id]),
            };
            environment_leaf(value, path, &permission, b)
        };
    let mut pending = vec![(d, Vec::new(), 0usize)];
    while let Some((definition, prefix, depth)) = pending.pop() {
        fee(b, definition.captures.len().saturating_add(prefix.len()))?;
        if depth > b.partial_depth {
            return Err(failure("nested environment seed exceeds its finite depth"));
        }
        for (index, capture) in definition.captures.iter().enumerate() {
            let mut path = prefix.clone();
            fee(b, path.len().saturating_add(1))?;
            path.push(PlaceProjection::Field(index as u32));
            if let Type::Address { mutable, .. } = capture {
                add(path.clone(), *mutable, value, b)?;
                if let Some((_, shape)) = pointed_carrier(capture) {
                    for leaf in &shape.slots {
                        let mut nested = path.clone();
                        fee(b, nested.len().saturating_add(leaf.path.len()))?;
                        nested.extend_from_slice(&leaf.path);
                        add(
                            nested,
                            leaf.mode == StoredBorrowMode::ExclusiveAddress,
                            value,
                            b,
                        )?;
                    }
                }
            } else if let Type::BorrowedStorage { life, .. } = capture {
                for slot in &life.slots {
                    let mut nested = path.clone();
                    fee(b, nested.len().saturating_add(slot.path.len()))?;
                    nested.extend_from_slice(&slot.path);
                    add(
                        nested,
                        slot.mode == StoredBorrowMode::ExclusiveAddress,
                        value,
                        b,
                    )?;
                }
            } else if let Type::Closure(child) = capture {
                let definition = module
                    .closures
                    .get(child.0 as usize)
                    .ok_or_else(|| failure("nested environment seed definition absent"))?;
                pending.push((definition, path, depth + 1));
            }
        }
    }
    Ok(())
}
fn seed_value(
    module: &TypedModule,
    ty: &Type,
    base: HoldRoot,
    state: &mut State,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> CompileResult<Value> {
    let mut value = Value::default();
    if matches!(ty, Type::Desc(payload) if matches!(payload.as_ref(),Type::OwnedStore{..}|Type::ByteBuffer|Type::Record{..}))
    {
        fee(b, 1)?;
        value.store_description.targets.insert(Target {
            retained_port: None,
            base,
            stored: vec![],
            place: vec![],
            heap_parents: vec![],
            heap_element: None,
        });
    }
    if let Type::Address { mutable, .. } = ty {
        let target = Target {
            retained_port: None,
            base,
            stored: Vec::new(),
            place: Vec::new(),
            heap_parents: Vec::new(),
            heap_element: None,
        };
        let id = loans.len();
        fee(b, 2)?;
        loans.push(Loan {
            targets: BTreeSet::from([target.clone()]),
            parents: Loans::new(),
            mutable: *mutable,
            receiver_head: matches!(ty,Type::Address{inner,..} if matches!(inner.as_ref(),Type::Closure(_))),
        });
        b.stored_exclusive_loans |= *mutable;
        value.pointer = Permission {
            targets: BTreeSet::from([target]),
            loans: Loans::from([id]),
        };
    }
    if b.stored_enum_addresses
        && matches!(ty, Type::ErrorUnion { .. })
        && enum_address::signature(ty)
    {
        for (path, mode) in enum_address::slots(ty, b)? {
            let target = Target {
                retained_port: None,
                base,
                stored: path.clone(),
                place: Vec::new(),
                heap_parents: Vec::new(),
                heap_element: None,
            };
            fee(b, path.len().saturating_mul(2).saturating_add(3))?;
            let id = loans.len();
            loans.push(Loan {
                targets: BTreeSet::from([target.clone()]),
                parents: Loans::new(),
                mutable: mode == StoredBorrowMode::ExclusiveAddress,
                receiver_head: false,
            });
            b.stored_exclusive_loans |= mode == StoredBorrowMode::ExclusiveAddress;
            value.slots.insert(
                path,
                Permission {
                    targets: BTreeSet::from([target]),
                    loans: Loans::from([id]),
                },
            );
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
        fee(b, (*length as usize).saturating_mul(8))?;
        for index in 0..*length {
            let path = vec![PlaceProjection::Field(index)];
            let source = Target {
                retained_port: None,
                base,
                stored: path.clone(),
                place: Vec::new(),
                heap_parents: Vec::new(),
                heap_element: None,
            };
            let id = loans.len();
            loans.push(Loan {
                targets: Targets::from([source.clone()]),
                parents: Loans::new(),
                mutable: false,
                receiver_head: false,
            });
            ordered(b, value.slots.len(), 1)?;
            value.slots.insert(
                path,
                Permission {
                    targets: Targets::from([source]),
                    loans: Loans::from([id]),
                },
            );
        }
        if matches!(ty, Type::Address { .. }) {
            ordered(b, state.external.len(), 0)?;
            let mut copy = BTreeMap::new();
            for (path, permission) in &value.slots {
                ordered(b, copy.len(), path.len())?;
                fee(b, path.len())?;
                union(copy.entry(path.clone()).or_default(), permission, b)?;
            }
            state.external.insert(base, copy);
        }
    }
    let qualified = carrier(ty).or_else(|| pointed_carrier(ty));
    let mut compound_slots = Vec::new();
    if matches!(ty, Type::Tuple(_)) {
        compound_slot_rows(ty, &mut Vec::new(), &mut compound_slots, b)?;
    }
    if qualified.is_some() || !compound_slots.is_empty() {
        let declarations =
            qualified.map_or(compound_slots.as_slice(), |(_, life)| life.slots.as_slice());
        let mut slots = BTreeMap::new();
        for decl in declarations {
            fee(b, decl.path.len().saturating_mul(3).saturating_add(3))?;
            let target = Target {
                retained_port: None,
                base,
                stored: decl.path.clone(),
                place: Vec::new(),
                heap_parents: Vec::new(),
                heap_element: None,
            };
            let id = loans.len();
            loans.push(Loan {
                targets: BTreeSet::from([target.clone()]),
                parents: Loans::new(),
                mutable: decl.mode == StoredBorrowMode::ExclusiveAddress,
                receiver_head: false,
            });
            b.stored_exclusive_loans |= decl.mode == StoredBorrowMode::ExclusiveAddress;
            ordered(b, slots.len(), decl.path.len())?;
            slots.insert(
                decl.path.clone(),
                Permission {
                    targets: BTreeSet::from([target]),
                    loans: Loans::from([id]),
                },
            );
        }
        if pointed_carrier(ty).is_some() {
            let mut copy = Value::default();
            for (path, permission) in &slots {
                ordered(b, copy.slots.len(), path.len())?;
                fee(b, path.len())?;
                union(copy.slots.entry(path.clone()).or_default(), permission, b)?;
            }
            ordered(b, state.external.len(), 0)?;
            state.external.insert(base, copy.slots);
        }
        value.slots = slots;
    }
    task_address::seed(ty, base, &mut value, loans, b)?;
    dyn_address::seed(module, ty, base, &mut value, loans, b)?;
    Ok(value)
}
fn seed(
    module: &TypedModule,
    f: &TypedFunction,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> CompileResult<State> {
    let mut state = State::default();
    for (index, local) in f.parameters.iter().enumerate() {
        fee(b, 1)?;
        let mut value = seed_value(
            module,
            &f.locals[local.0 as usize].ty,
            HoldRoot::Parameter(index as u32),
            &mut state,
            loans,
            b,
        )?;
        if enum_address::ordinary(module, &f.locals[local.0 as usize].ty, b)? {
            value.enum_variants =
                enum_address::Variants::parameter(index as u32, &f.locals[local.0 as usize].ty, b)?;
        } else if option_address::candidate(&f.locals[local.0 as usize].ty) {
            fee(b, 96)?;
            if option_address::signature(&f.locals[local.0 as usize].ty) {
                value.option_variants = option_address::Variants::parameter(index as u32, b)?;
            }
        }
        seed_environment(
            module,
            &f.locals[local.0 as usize].ty,
            HoldRoot::Parameter(index as u32),
            &mut value,
            loans,
            b,
        )?;
        if matches!(&f.locals[local.0 as usize].ty,Type::Address{inner,..} if matches!(inner.as_ref(),Type::Closure(_)))
            && !value.environment.is_empty()
        {
            b.stored_slot_updates |= matches!(
                &f.locals[local.0 as usize].ty,
                Type::Address { mutable: true, .. }
            );
            let rows = copy_permission_rows(&value.environment, b)?;
            ordered(b, state.external.len(), 0)?;
            state
                .external
                .insert(HoldRoot::Parameter(index as u32), rows);
        }
        if let Type::Closure(id) = &f.locals[local.0 as usize].ty
            && !value.environment.is_empty()
            && capture_after::has_context(module, &module.closures[id.0 as usize], b)?
        {
            b.stored_slot_updates = true;
            let rows = copy_permission_rows(&value.environment, b)?;
            ordered(b, state.external.len(), 0)?;
            state
                .external
                .insert(HoldRoot::Parameter(index as u32), rows);
        }
        if matches!(&f.locals[local.0 as usize].ty, Type::Desc(payload)
            if super::stored_callables::resource_payload(payload, b)?)
        {
            owned::describe(&mut value, HoldRoot::Parameter(index as u32), b)?;
        }
        ordered(b, state.locals.len(), 0)?;
        state.locals.insert(*local, value);
    }
    if let Some(d) = f
        .environment
        .and_then(|id| module.closures.get(id.0 as usize))
        && (d.storage_life.is_some() || b.returned_borrowed_environments.contains(&d.id))
    {
        if super::stored_callables::lending(d)
            || d.captures
                .iter()
                .any(super::stored_callables::exclusive_capture)
        {
            fee(b, 1)?;
            state.receiver_loan = Some(loans.len());
            loans.push(Loan::default());
        }
        for (index, ty) in d.captures.iter().enumerate() {
            fee(b, 1)?;
            let mut value = seed_value(
                module,
                ty,
                HoldRoot::Capture(index as u32),
                &mut state,
                loans,
                b,
            )?;
            if matches!(ty, Type::Closure(_)) {
                seed_environment(
                    module,
                    ty,
                    HoldRoot::Capture(index as u32),
                    &mut value,
                    loans,
                    b,
                )?;
                if let Type::Closure(child) = ty
                    && capture_after::has_context(module, &module.closures[child.0 as usize], b)?
                {
                    let rows = copy_permission_rows(&value.environment, b)?;
                    ordered(b, state.external.len(), 0)?;
                    state.external.insert(HoldRoot::Capture(index as u32), rows);
                }
            }
            ordered(b, state.captures.len(), 0)?;
            state.captures.insert(index as u32, value);
        }
    }
    Ok(state)
}

fn local_capture_base(state: &State, local: LocalId, b: &mut Budget) -> CompileResult<HoldRoot> {
    if state.capture_views.is_empty() {
        return Ok(HoldRoot::Local(local));
    }
    ordered(b, state.capture_views.len(), 0)?;
    Ok(state
        .capture_views
        .get(&local)
        .copied()
        .map_or(HoldRoot::Local(local), HoldRoot::Capture))
}

fn captured_view_local(
    state: &State,
    base: HoldRoot,
    b: &mut Budget,
) -> CompileResult<Option<LocalId>> {
    let HoldRoot::Capture(index) = base else {
        return Ok(None);
    };
    if state.capture_views.is_empty() {
        return Ok(None);
    }
    fee(b, state.capture_views.len())?;
    Ok(state
        .capture_views
        .iter()
        .find_map(|(local, capture)| (*capture == index).then_some(*local)))
}
fn actual_slot_rows_mut<'a>(
    state: &'a mut State,
    base: HoldRoot,
    b: &mut Budget,
) -> CompileResult<&'a mut BTreeMap<Path, Permission>> {
    let view = captured_view_local(state, base, b)?;
    match base {
        HoldRoot::Local(local) => state.locals.get_mut(&local).map(|value| &mut value.slots),
        _ if view.is_some() => state
            .locals
            .get_mut(&view.unwrap())
            .map(|value| &mut value.slots),
        HoldRoot::Parameter(_) | HoldRoot::Capture(_) => state.external.get_mut(&base),
        HoldRoot::Static => None,
    }
    .ok_or_else(|| failure("actual stored carrier has no active slot rows"))
}
fn select(
    state: &State,
    address: &Permission,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> CompileResult<Permission> {
    // Bottom while a predecessor/callee is unsolved is not permission. Final
    // completeness and window checking reject every remaining empty target.
    if address.targets.is_empty() {
        return Ok(Permission::default());
    }
    let mut out = Permission::default();
    for root in &address.targets {
        fee(b, root.stored.len().saturating_add(1))?;
        if !root.stored.is_empty() {
            return Err(failure("stored target is not an inline carrier address"));
        }
        let mut actual_path = root.place.clone();
        actual_path.extend_from_slice(path);
        let slots = match root.base {
            HoldRoot::Static => {
                return Err(failure("static UTF8 cannot grant nested carrier slots"));
            }
            HoldRoot::Local(local) => {
                ordered(b, state.locals.len(), 0)?;
                let Some(value) = state.locals.get(&local) else {
                    continue;
                };
                &value.slots
            }
            HoldRoot::Parameter(_) | HoldRoot::Capture(_) => {
                if let Some(local) = captured_view_local(state, root.base, b)? {
                    &state
                        .locals
                        .get(&local)
                        .ok_or_else(|| failure("captured view slot rows absent"))?
                        .slots
                } else {
                    ordered(b, state.external.len(), 0)?;
                    state
                        .external
                        .get(&root.base)
                        .ok_or_else(|| failure("external carrier has no actual slot facts"))?
                }
            }
        };
        ordered(b, slots.len(), actual_path.len())?;
        if let Some(leaf) = slots.get(&actual_path) {
            union(&mut out, leaf, b)?;
        }
    }
    Ok(out)
}

fn slot_position(
    address: &Permission,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> CompileResult<Target> {
    let mut roots = address.targets.iter();
    let root = roots
        .next()
        .ok_or_else(|| failure("stored slot has no actual carrier"))?;
    if roots.next().is_some() || !root.stored.is_empty() {
        return Err(failure("stored slot needs one actual inline carrier"));
    }
    fee(
        b,
        target_width(root)
            .saturating_add(path.len())
            .saturating_add(1),
    )?;
    let mut slot = root.clone();
    slot.place.extend_from_slice(path);
    Ok(slot)
}

fn require_filled(
    state: &State,
    address: &Permission,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> CompileResult<()> {
    if state.vacant.is_empty() {
        return Ok(());
    }
    for root in &address.targets {
        fee(
            b,
            target_width(root)
                .saturating_add(path.len())
                .saturating_add(1),
        )?;
        let mut position = root.clone();
        position.place.extend_from_slice(path);
        ordered(b, state.vacant.len(), target_width(&position))?;
        if state.vacant.contains(&position) {
            return Err(failure("stored slot is vacant on an incoming path"));
        }
    }
    Ok(())
}

fn clear_local_holes(state: &mut State, local: LocalId, b: &mut Budget) -> CompileResult<()> {
    fee(b, state.vacant.len())?;
    state
        .vacant
        .retain(|target| target.base != HoldRoot::Local(local));
    Ok(())
}

fn projection_only_load(
    f: &TypedFunction,
    instruction: &Instruction,
    storage: ValueId,
    b: &mut Budget,
) -> CompileResult<Option<LocalId>> {
    let mut adjacent = None;
    let mut uses = 0usize;
    for block in &f.blocks {
        for (index, actual) in block.instructions.iter().enumerate() {
            fee(b, 1)?;
            if std::ptr::eq(actual, instruction)
                && index > 0
                && block.instructions[index - 1].result == Some(storage)
                && let InstructionKind::Load(local) = block.instructions[index - 1].kind
                && pointed_carrier(&f.locals[local.0 as usize].ty).is_some()
            {
                adjacent = Some(local);
            }
            for operand in resources::uses(&actual.kind) {
                fee(b, 1)?;
                uses += usize::from(operand == storage);
            }
        }
        fee(b, 1)?;
        uses += usize::from(matches!(block.terminator,
            Terminator::Return(Some(value)) if value == storage));
        uses += usize::from(matches!(block.terminator,
            Terminator::Branch { condition, .. } if condition == storage));
    }
    Ok((uses == 1).then_some(adjacent).flatten())
}

fn roots(value: &Value, b: &mut Budget) -> CompileResult<Targets> {
    let mut permission = Permission::default();
    union(&mut permission, &value.pointer, b)?;
    union(&mut permission, &value.held, b)?;
    for leaf in value.slots.values() {
        union(&mut permission, leaf, b)?;
    }
    if b.dyn_contexts {
        for leaf in value.dyn_ports.values() {
            union(&mut permission, leaf, b)?;
        }
    }
    if b.task_sources() {
        for leaf in value.task_ports.values() {
            union(&mut permission, leaf, b)?;
        }
    }
    Ok(permission.targets)
}

fn substitute(
    source: &Target,
    arguments: &[ValueId],
    values: &[Value],
    state: &State,
    b: &mut Budget,
) -> CompileResult<Permission> {
    if source.base == HoldRoot::Static {
        if !source.stored.is_empty() || !source.place.is_empty() {
            return Err(failure(
                "static UTF8 summary cannot carry nested projections",
            ));
        }
        fee(b, target_width(source).saturating_add(1))?;
        return Ok(Permission {
            targets: Targets::from([source.clone()]),
            loans: Loans::new(),
        });
    }
    let HoldRoot::Parameter(index) = source.base else {
        return Err(failure(
            "callee result retains activation-local or captured storage",
        ));
    };
    let actual = *arguments
        .get(index as usize)
        .ok_or_else(|| failure("callee source parameter has no actual argument"))?;
    let value = values
        .get(actual.0 as usize)
        .ok_or_else(|| failure("call actual is outside values"))?;
    let mut result = if let Some(port) = &source.retained_port {
        match port {
            PortOrigin::Task(port) => task_address::select(value, &source.stored, *port, b)?,
            PortOrigin::Dyn { carrier, port } => dyn_address::select(value, carrier, *port, b)?,
        }
    } else if source.stored.is_empty() {
        let mut out = Permission::default();
        union(&mut out, &value.pointer, b)?;
        if !value.store_description.targets.is_empty() || !value.store_description.loans.is_empty()
        {
            union(&mut out, &value.store_description, b)?;
        }
        out
    } else if !value.environment.is_empty() {
        let current = environment_memory::current(value, state, b)?;
        ordered(b, current.len(), source.stored.len())?;
        let mut out = Permission::default();
        if let Some(actual) = current.get(&source.stored) {
            union(&mut out, actual, b)?;
        }
        out
    } else if !value.slots.is_empty()
        && !value.pointer.targets.is_empty()
        && value.pointer.targets.iter().all(|root| {
            root.stored.is_empty()
                && root.place.is_empty()
                && match root.base {
                    HoldRoot::Local(local) => state.locals.contains_key(&local),
                    HoldRoot::Parameter(_) | HoldRoot::Capture(_) => {
                        state.external.contains_key(&root.base)
                    }
                    HoldRoot::Static => false,
                }
        })
    {
        // A mutable carrier may have been replaced since this SSA argument
        // was loaded. Its leaf snapshot is no longer the current memory slot.
        select(state, &value.pointer, &source.stored, b)?
    } else if !value.slots.is_empty() {
        ordered(b, value.slots.len(), source.stored.len())?;
        let mut out = Permission::default();
        if let Some(actual) = value.slots.get(&source.stored) {
            union(&mut out, actual, b)?;
        }
        out
    } else {
        select(state, &value.pointer, &source.stored, b)?
    };
    if !source.place.is_empty() || source.heap_element.is_some() {
        let mut targets = Targets::new();
        for target in &result.targets {
            fee(
                b,
                target_width(target)
                    .saturating_add(source.place.len())
                    .saturating_add(source.heap_element.as_ref().map_or(0, Vec::len))
                    .saturating_add(1),
            )?;
            let mut target = target.clone();
            if let Some(heap) = &mut target.heap_element {
                heap.extend(&source.place);
                if let Some(element) = &source.heap_element {
                    fee(
                        b,
                        target_width(source).saturating_add(target_width(&target)),
                    )?;
                    let parent = target.heap_element.take().expect("actual parent element");
                    target.heap_parents.push(parent);
                    target
                        .heap_parents
                        .extend(source.heap_parents.iter().cloned());
                    target.heap_element = Some(element.clone());
                }
            } else {
                fee(
                    b,
                    source.heap_parents.iter().fold(0usize, |n, path| {
                        n.saturating_add(path.len()).saturating_add(1)
                    }),
                )?;
                target.place.extend(&source.place);
                target
                    .heap_parents
                    .extend(source.heap_parents.iter().cloned());
                target.heap_element = source.heap_element.clone();
            }
            ordered(b, targets.len(), target_width(&target))?;
            targets.insert(target);
        }
        result.targets = targets;
    }
    Ok(result)
}

fn slot_loan_ids(
    module: &TypedModule,
    f: &TypedFunction,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> CompileResult<SlotLoanIds> {
    let mut ids = SlotLoanIds::new();
    for block in &f.blocks {
        for instruction in &block.instructions {
            let site = instruction as *const Instruction as usize;
            let receiver_result = super::stored_callables::called(module, f, &instruction.kind)
                .is_some_and(|(d, _)| super::stored_callables::result_is_receiver(d));
            let mutable_leaves = if let Some((d, _)) =
                super::stored_callables::called(module, f, &instruction.kind)
                && (d.storage_life.is_some() || b.returned_borrowed_environments.contains(&d.id))
                && capture_after::required(module, d, b)?
            {
                capture_after::outputs(module, d, b)?
            } else {
                Vec::new()
            };
            let mut add =
                |parameter: u32, path: &Path, mutable: bool, b: &mut Budget| -> CompileResult<()> {
                    fee(b, path.len().saturating_add(2))?;
                    ordered(b, ids.len(), path.len())?;
                    let id = loans.len();
                    loans.push(Loan::default());
                    b.stored_exclusive_loans |= mutable;
                    if ids
                        .insert((site, parameter, path.clone()), (id, mutable))
                        .is_some()
                    {
                        return Err(failure("duplicate actual stored write site"));
                    }
                    Ok(())
                };
            if receiver_result {
                add(
                    u32::MAX - 2,
                    &Vec::new(),
                    matches!(
                        instruction.kind,
                        InstructionKind::CallClosureMut { .. }
                            | InstructionKind::CallClosureBorrowed {
                                kind: CallableKind::FnMut,
                                ..
                            }
                    ),
                    b,
                )?;
            }
            match &instruction.kind {
                InstructionKind::MakeAggregate { .. }
                | InstructionKind::MakePackage { .. }
                | InstructionKind::MakeOwnedTuple { .. } => {
                    if let Some((_, shape)) = carrier(value_type(
                        f,
                        instruction
                            .result
                            .ok_or_else(|| failure("aggregate lacks result"))?,
                    )?) {
                        for slot in &shape.slots {
                            if slot.mode == StoredBorrowMode::ExclusiveAddress {
                                add(u32::MAX, &slot.path, true, b)?;
                            }
                        }
                    }
                }
                InstructionKind::StoreStoredBorrow { path, value, .. } => {
                    add(
                        u32::MAX,
                        path,
                        matches!(value_type(f, *value)?, Type::Address { mutable: true, .. }),
                        b,
                    )?;
                }
                InstructionKind::Call { function, .. } => {
                    let callee = module
                        .functions
                        .get(function.0 as usize)
                        .ok_or_else(|| failure("stored call target is absent"))?;
                    for (position, local) in callee.parameters.iter().enumerate() {
                        if let Type::Address {
                            mutable: true,
                            inner,
                        } = &callee.locals[local.0 as usize].ty
                            && let Some((_, shape)) = carrier(inner)
                        {
                            for slot in &shape.slots {
                                add(
                                    position as u32,
                                    &slot.path,
                                    slot.mode == StoredBorrowMode::ExclusiveAddress,
                                    b,
                                )?;
                            }
                        } else if let Type::Address {
                            mutable: true,
                            inner,
                        } = &callee.locals[local.0 as usize].ty
                            && let Type::Closure(id) = inner.as_ref()
                        {
                            let d = module
                                .closures
                                .get(id.0 as usize)
                                .ok_or_else(|| failure("external callable definition absent"))?;
                            for (path, mutable) in capture_after::leaves(module, d, b)? {
                                add(position as u32, &path, mutable, b)?;
                            }
                        } else if let Type::Closure(id) = &callee.locals[local.0 as usize].ty
                            && capture_after::has_context(
                                module,
                                &module.closures[id.0 as usize],
                                b,
                            )?
                        {
                            for (path, mutable) in
                                capture_after::leaves(module, &module.closures[id.0 as usize], b)?
                            {
                                add(position as u32, &path, mutable, b)?;
                            }
                        }
                    }
                }
                _ if !mutable_leaves.is_empty() => {
                    for (path, mutable) in &mutable_leaves {
                        add(u32::MAX - 1, path, *mutable, b)?;
                    }
                }
                _ => {}
            }
        }
    }
    Ok(ids)
}

fn retain_slot(
    source: &Permission,
    i: &Instruction,
    parameter: u32,
    path: &Path,
    ids: &SlotLoanIds,
    loans: &mut [Loan],
    b: &mut Budget,
) -> CompileResult<Permission> {
    let (id, mutable) = ids
        .get(&(i as *const Instruction as usize, parameter, path.clone()))
        .ok_or_else(|| failure("stored write has no stable loan site"))?;
    let loan = loans
        .get_mut(*id)
        .ok_or_else(|| failure("stored write loan exceeds its actual arena"))?;
    let mut joined = Permission {
        targets: std::mem::take(&mut loan.targets),
        loans: std::mem::take(&mut loan.parents),
    };
    union(&mut joined, source, b)?;
    joined.loans.remove(id);
    loan.targets = joined.targets;
    loan.parents = joined.loans;
    loan.mutable = *mutable;
    b.stored_exclusive_loans |= loan.mutable;
    let mut result = Permission::default();
    union(&mut result, source, b)?;
    result.loans.clear();
    fee(b, 1)?;
    result.loans.insert(*id);
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn transfer(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &mut State,
    values: &[Value],
    summaries: &[Facts],
    slot_ids: &SlotLoanIds,
    loans: &mut [Loan],
    b: &mut Budget,
) -> CompileResult<Value> {
    fee(b, 1)?;
    let mut out = Value::default();
    let get = |id: ValueId| {
        values
            .get(id.0 as usize)
            .ok_or_else(|| failure("operand is outside actual values"))
    };
    let dyn_transfer = dyn_address::transfer(module, f, i, state, values, b)?;
    let enum_transfer = enum_address::transfer(module, f, i, values, b)?;
    let task_transfer = task_address::transfer(module, f, i, state, values, summaries, b)?;
    let failure_view = failure_projection(f, i, b)?;
    let failure_wrapper = task_failure::wrapper(f, i, state, values, b)?;
    let unpack_failure = failure_unpack(f, i, b)?;
    let option_transfer = if enum_transfer.is_none() && task_transfer.is_none() {
        option_address::transfer(module, f, i, values, b)?
    } else {
        None
    };
    match &i.kind {
        InstructionKind::BindPackageCapture { local, index } => {
            let actual = state.captures.get(index)
                .ok_or_else(|| failure("captured view has no actual environment value"))?;
            let actual = snapshot(actual, b)?;
            ordered(b, state.locals.len(), 0)?;
            state.locals.insert(*local, actual);
            ordered(b, state.capture_views.len(), 0)?;
            if state.capture_views.insert(*local, *index).is_some() {
                return Err(failure("captured view bound twice"));
            }
        }
        InstructionKind::EndPackageCapture { local, index } => {
            ordered(b, state.capture_views.len(), 0)?;
            if state.capture_views.remove(local) != Some(*index) {
                return Err(failure("captured view end differs from its actual binding"));
            }
            let actual = state.locals.get(local)
                .ok_or_else(|| failure("ended captured view has no actual value"))?;
            let actual = snapshot(actual, b)?;
            ordered(b, state.captures.len(), 0)?;
            state.captures.insert(*index, actual);
        }
        _ if crate::tasks::input_scope::registration_operation(f, i, b)? => {
            // Its independently reconstructed complete environment has no
            // stored class or target. Provider, phase, owner and release facts
            // are proved by their separate actual-instruction consumers.
        }

        InstructionKind::UnpackTuple { local, fields } if unpack_failure => {
            task_failure::unpack_value(f, *local, fields, state, b)?;
        }
        InstructionKind::TaskCreateOwned { arguments, .. } if b.task_failure_carriers => {
            out = task_transfer.unwrap_or_default();
            task_failure::create(f, i, arguments, values, &mut out, b)?;
        }
        InstructionKind::LoadStoredBorrow { storage, .. } if failure_view.is_some() => {
            let Type::Address { inner, .. } = value_type(f, *storage)? else {
                unreachable!()
            };
            let Type::BorrowedStorage { life, .. } = inner.as_ref() else {
                unreachable!()
            };
            task_failure::take_view(state, &get(*storage)?.pointer, life, &mut out, b)?;
        }
        InstructionKind::StaticUtf8 { .. } => {
            fee(b, 1)?;
            out.pointer.targets.insert(Target {
                retained_port: None,
                base: HoldRoot::Static,
                stored: Vec::new(),
                place: Vec::new(),
                heap_parents: Vec::new(),
                heap_element: None,
            });
        }
        InstructionKind::Load(local) => {
            ordered(b, state.locals.len(), 0)?;
            out = snapshot(
                state
                    .locals
                    .get(local)
                    .ok_or_else(|| failure("read of unavailable carrier/local"))?,
                b,
            )?;
        }
        InstructionKind::Store { local, value } => {
            clear_local_holes(state, *local, b)?;
            ordered(b, state.locals.len(), 0)?;
            state.locals.insert(*local, snapshot(get(*value)?, b)?);
        }
        InstructionKind::TakeLocal { local } => {
            ordered(b, state.locals.len(), 0)?;
            if matches!(f.locals[local.0 as usize].ty, Type::OwnedDyn { .. })
                || task_address::shape(&f.locals[local.0 as usize].ty).is_some()
                || matches!(
                    f.locals[local.0 as usize].ty,
                    Type::Address { mutable: true, .. }
                )
                || environment_value(module, &f.locals[local.0 as usize].ty, b)?
                || carrier(&f.locals[local.0 as usize].ty).is_some()
                || affine_compound(module,&f.locals[local.0 as usize].ty,b)?
                || (b.stored_enum_addresses
                    && enum_address::signature(&f.locals[local.0 as usize].ty))
                || matches!(f.locals[local.0 as usize].ty, Type::Address { .. })
            {
                out = snapshot(
                    state
                        .locals
                        .get(local)
                        .ok_or_else(|| failure("move of unavailable environment"))?,
                    b,
                )?;
            }
            state.locals.remove(local);
            clear_local_holes(state, *local, b)?;
        }
        InstructionKind::BorrowPlaceLocal { local, path, .. }
            if ordinary_error_borrow(f, i, b)? =>
        {
            fee(b, path.len().saturating_add(1))?;
            out.pointer.targets.insert(Target {
                retained_port: None,
                base: state.capture_views.get(local).copied().map_or(HoldRoot::Local(*local), HoldRoot::Capture),
                stored: Vec::new(),
                place: path.clone(),
                heap_parents: Vec::new(),
                heap_element: None,
            });
        }
        InstructionKind::TakePackageField { local, index } => {
            ordered(b, state.locals.len(), 0)?;
            let holder = state
                .locals
                .get_mut(local)
                .ok_or_else(|| failure("taken Package field has no live local shell"))?;
            out.slots = take_package_field_slots(holder, *index, b)?;
            fee(b, state.vacant.len())?;
            state.vacant.retain(|target| {
                !(target.base == HoldRoot::Local(*local)
                    && target.place.first() == Some(&PlaceProjection::Field(*index)))
            });
        }
        InstructionKind::DropPackageField { local, index } => {
            ordered(b, state.locals.len(), 0)?;
            if let Some(holder) = state.locals.get_mut(local) {
                take_package_field_slots(holder, *index, b)?;
            }
            fee(b, state.vacant.len())?;
            state.vacant.retain(|target| {
                !(target.base == HoldRoot::Local(*local)
                    && target.place.first() == Some(&PlaceProjection::Field(*index)))
            });
        }
        InstructionKind::StorePackageField {
            local,
            index,
            value,
        } => {
            let source = snapshot(get(*value)?, b)?;
            ordered(b, state.locals.len(), 0)?;
            let holder = state.locals.entry(*local).or_default();
            take_package_field_slots(holder, *index, b)?;
            for (path, permission) in source.slots {
                fee(b, path.len().saturating_add(1))?;
                let mut complete = vec![PlaceProjection::Field(*index)];
                complete.extend(path);
                ordered(b, holder.slots.len(), complete.len())?;
                holder.slots.insert(complete, permission);
            }
            fee(b, state.vacant.len())?;
            state.vacant.retain(|target| {
                !(target.base == HoldRoot::Local(*local)
                    && target.place.first() == Some(&PlaceProjection::Field(*index)))
            });
        }
        InstructionKind::StorageLive(local)
        | InstructionKind::StorageDead(local)
        | InstructionKind::DropClosure { local } => {
            ordered(b, state.locals.len(), 0)?;
            state.locals.remove(local);
            clear_local_holes(state, *local, b)?;
        }
        InstructionKind::BorrowLocal { local } | InstructionKind::BorrowLocalMut { local } => {
            fee(b, 1)?;
            let base = if state.capture_views.is_empty() { HoldRoot::Local(*local) } else {
                ordered(b, state.capture_views.len(), 0)?;
                state.capture_views.get(local).copied().map_or(HoldRoot::Local(*local), HoldRoot::Capture)
            };
            out.pointer.targets.insert(Target {
                retained_port: None,
                base,
                stored: Vec::new(),
                place: Vec::new(),
                heap_parents: Vec::new(),
                heap_element: None,
            });
            if matches!(base,HoldRoot::Capture(_)) && let Some(marker)=state.receiver_loan {
                fee(b,1)?;
                out.pointer.loans.insert(marker);
            }
            if environment_value(module, &f.locals[local.0 as usize].ty, b)? {
                ordered(b, state.locals.len(), 0)?;
                if let Some(held) = state.locals.get(local) {
                    environment_merge(&mut out, held, b)?;
                }
            }
            if b.task_sources()
                && let Some(held) = state.locals.get(local)
            {
                task_address::merge(&mut out, held, b)?;
            }
            if carrier(&f.locals[local.0 as usize].ty).is_some()
                || shared_array_value(&f.locals[local.0 as usize].ty)
            {
                ordered(b, state.locals.len(), 0)?;
                let held = state
                    .locals
                    .get(local)
                    .ok_or_else(|| failure("borrowed carrier is unavailable"))?;
                for (path, permission) in &held.slots {
                    ordered(b, out.slots.len(), path.len())?;
                    fee(b, path.len())?;
                    union(out.slots.entry(path.clone()).or_default(), permission, b)?;
                }
            }
        }
        InstructionKind::AddressField { address, index } => {
            union(&mut out.pointer, &get(*address)?.pointer, b)?;
            let mut targets = Targets::new();
            for target in &out.pointer.targets {
                fee(b, target_width(target).saturating_add(2))?;
                let mut target = target.clone();
                if let Some(element) = &mut target.heap_element {
                    element.push(PlaceProjection::Field(*index));
                } else {
                    target.place.push(PlaceProjection::Field(*index));
                }
                ordered(b, targets.len(), target_width(&target))?;
                targets.insert(target);
            }
            out.pointer.targets = targets;
            if let Type::Address { inner, .. } = value_type(
                    f,
                    i.result
                        .ok_or_else(|| failure("Store field result absent"))?,
                )?
                && matches!(inner.as_ref(), Type::Package { .. })
            {
                if out.pointer.targets.is_empty() {
                    return Err(failure("owning Store field address lacks an actual source"));
                }
                for source in &out.pointer.targets {
                    if source.heap_element.is_none()
                        && !ordinary_inline_package_target(f, source, inner, b)?
                    {
                        return Err(failure(
                            "owning Store field address lacks an actual element or inline Package source",
                        ));
                    }
                }
            }

            if let Some((_, shape)) = pointed_carrier(value_type(
                f,
                i.result
                    .ok_or_else(|| failure("field address lacks result"))?,
            )?) {
                fee(b, shape.slots.len())?;
                for row in &shape.slots {
                    // select constructs a complete path from each actual root.
                    // Its legacy lookup fee does not pay this new path clone.
                    for root in &out.pointer.targets {
                        fee(
                            b,
                            root.place
                                .len()
                                .saturating_add(row.path.len())
                                .saturating_add(1),
                        )?;
                    }
                    let permission = select(state, &out.pointer, &row.path, b)?;
                    ordered(b, out.slots.len(), row.path.len())?;
                    fee(b, row.path.len())?;
                    union(
                        out.slots.entry(row.path.clone()).or_default(),
                        &permission,
                        b,
                    )?;
                }
            }
            if let Some(result) = i.result
                && let Type::Address { inner, .. } = value_type(f, result)?
                && shared_array_shape(inner, b)?
            {
                let Type::Array { length, .. } = inner.as_ref() else {
                    unreachable!()
                };
                fee(b, *length as usize)?;
                for index in 0..*length {
                    let path = vec![PlaceProjection::Field(index)];
                    // select's legacy fee covers stored selectors. This new
                    // projected child also prepays its inline clone/append.
                    for root in &out.pointer.targets {
                        fee(b, root.place.len().saturating_add(path.len()))?;
                    }
                    let actual = select(state, &out.pointer, &path, b)?;
                    ordered(b, out.slots.len(), path.len())?;
                    out.slots.insert(path, actual);
                }
            }
        }
        InstructionKind::ReborrowAddress { address, .. }
        | InstructionKind::AddressIndex { address, .. }
        | InstructionKind::AddressSlice { address, .. } => {
            union(&mut out.pointer, &get(*address)?.pointer, b)?;
            task_address::merge(&mut out, get(*address)?, b)?;
            environment_merge(&mut out, get(*address)?, b)?;
            if matches!(i.kind, InstructionKind::ReborrowAddress { .. })
                && (pointed_carrier(value_type(f, *address)?).is_some()
                    || matches!(value_type(f, *address)?, Type::Address { inner, .. }
                        if shared_array_shape(inner, b)?))
            {
                let actual = get(*address)?;
                if pointed_carrier(value_type(f, *address)?).is_some() {
                    fee(b, actual.slots.len())?;
                }
                for (path, permission) in &actual.slots {
                    ordered(b, out.slots.len(), path.len())?;
                    fee(b, path.len())?;
                    union(out.slots.entry(path.clone()).or_default(), permission, b)?;
                }
            }
        }
        InstructionKind::MakeAggregate { fields } | InstructionKind::MakePackage { fields } | InstructionKind::MakeOwnedTuple { fields } => {
            if b.task_failure_carriers
                && i.result.is_some_and(|r| matches!(f.values.get(r.0 as usize),Some(Type::BorrowedStorage{storage,..}) if matches!(storage.as_ref(),Type::Tuple(_))))
            {
                // A Task input view is ordinarily projected from its parent.
                // A separate ordinary Tuple producer must instead prove its
                // complete actual structure and mode, regardless of unrelated
                // Task declarations. It still reconstructs every source below.
                let result=i.result.ok_or_else(||failure("anonymous constructor result absent"))?;
                let ty=value_type(f,result)?;
                let shared=shared_compound(module,ty,b)?;
                let affine=affine_compound(module,ty,b)?;
                if !(shared && matches!(i.kind,InstructionKind::MakeAggregate{..})
                    || affine && matches!(i.kind,InstructionKind::MakeOwnedTuple{..})){
                    return Err(failure("anonymous input Tuple requires its actual parent projection or independently checked constructor"));
                }
                let Some((Type::Tuple(types),_))=carrier(ty) else{return Err(failure("anonymous constructor has no actual Tuple storage"))};
                if types.len()!=fields.len(){return Err(failure("anonymous constructor omits actual fields"))}
                for (field,expected) in fields.iter().zip(types){
                    b.type_work(expected,false,"anonymous constructor complete field type")?;
                    if value_type(f,*field)?!=expected{return Err(failure("anonymous constructor changes complete field Type"))}
                }
            }
            let ty = value_type(
                f,
                i.result.ok_or_else(|| failure("aggregate lacks result"))?,
            )?;
            if shared_array_shape(ty, b)? {
                let Type::Array { length, .. } = ty else {
                    unreachable!()
                };
                if fields.len() != *length as usize {
                    return Err(failure("shared array constructor omits an actual element"));
                }
                fee(b, fields.len().saturating_mul(2))?;
                for (index, field) in fields.iter().enumerate() {
                    let mut permission = Permission::default();
                    union(&mut permission, &get(*field)?.pointer, b)?;
                    ordered(b, out.slots.len(), 1)?;
                    out.slots
                        .insert(vec![PlaceProjection::Field(index as u32)], permission);
                }
                return Ok(out);
            }
            if matches!(carrier(ty).map_or(ty,|(storage,_)|storage),Type::Tuple(_))
                && matches!(i.kind,InstructionKind::MakeAggregate{..}) && affine_compound(module,ty,b)? {
                return Err(failure("affine compound requires an actual owning tuple constructor"));
            }
            if matches!(ty,Type::Tuple(_)) && (shared_compound(module,ty,b)? || affine_compound(module,ty,b)?) {
                if matches!(i.kind,InstructionKind::MakeAggregate{..}) && affine_compound(module,ty,b)? {return Err(failure("affine compound requires an actual owning tuple constructor"))}
                for (index,field) in fields.iter().enumerate(){
                    let source=get(*field)?;
                    union(&mut out.held,&source.held,b)?;
                    for (tail,permission) in &source.slots{
                        fee(b,tail.len().saturating_add(1))?;
                        let mut path=vec![PlaceProjection::Field(u32::try_from(index).map_err(|_|failure("compound field overflow"))?)];path.extend_from_slice(tail);
                        union(out.slots.entry(path).or_default(),permission,b)?;
                    }
                }
                return Ok(out);
            }
            let Some((storage, life)) = carrier(ty) else {
                return Ok(out);
            };
            for decl in &life.slots {
                let Some((PlaceProjection::Field(index), tail)) = decl.path.split_first() else {
                    return Err(failure("aggregate slot has no physical field"));
                };
                let source = get(*fields
                    .get(*index as usize)
                    .ok_or_else(|| failure("aggregate omits borrowed field"))?)?;
                let mut permission = Permission::default();
                if tail.is_empty() {
                    union(&mut permission, &source.pointer, b)?;
                } else {
                    ordered(b, source.slots.len(), tail.len())?;
                    if let Some(actual) = source.slots.get(tail) {
                        union(&mut permission, actual, b)?;
                    }
                }
                if !matches!(
                    (slot(storage, &decl.path, b)?, decl.mode),
                    (
                        Type::Address { mutable: false, .. },
                        StoredBorrowMode::SharedAddress
                    ) | (
                        Type::Address { mutable: true, .. },
                        StoredBorrowMode::ExclusiveAddress
                    )
                ) {
                    return Err(failure(
                        "constructed stored field mode differs from its actual address",
                    ));
                }
                if decl.mode == StoredBorrowMode::ExclusiveAddress {
                    permission =
                        retain_slot(&permission, i, u32::MAX, &decl.path, slot_ids, loans, b)?;
                }
                ordered(b, out.slots.len(), decl.path.len())?;
                fee(b, decl.path.len())?;
                out.slots.insert(decl.path.clone(), permission);
            }
        }
        InstructionKind::LoadStoredBorrow {
            storage,
            path,
            mode: StoredBorrowLoad::CopyShared,
        } => {
            out.pointer = select(state, &get(*storage)?.pointer, path, b)?;
        }
        InstructionKind::LoadStoredBorrow {
            storage,
            path,
            mode: StoredBorrowLoad::Reborrow { mutable: true },
        } => {
            out.pointer = select(state, &get(*storage)?.pointer, path, b)?;
            let parents = if let Some(local) = projection_only_load(f, i, *storage, b)? {
                state
                    .locals
                    .get(&local)
                    .ok_or_else(|| failure("projection parent local is unavailable"))?
                    .pointer
                    .loans
                    .clone()
            } else {
                get(*storage)?.pointer.loans.clone()
            };
            for parent in parents {
                fee(b, 1)?;
                out.pointer.loans.insert(parent);
            }
        }
        InstructionKind::LoadStoredBorrow {
            storage,
            path,
            mode: StoredBorrowLoad::Take,
        } => {
            let address = &get(*storage)?.pointer;
            out.pointer = select(state, address, path, b)?;
            if !address.targets.is_empty() {
                let position = slot_position(address, path, b)?;
                let slots = actual_slot_rows_mut(state, position.base, b)?;
                slots.remove(&position.place);
                ordered(b, state.vacant.len(), target_width(&position))?;
                state.vacant.insert(position);
            }
        }
        InstructionKind::StoreStoredBorrow {
            storage,
            path,
            value,
        } => {
            let mut roots = get(*storage)?.pointer.targets.iter();
            let root = roots
                .next()
                .ok_or_else(|| failure("replacement carrier has no actual root"))?;
            if roots.next().is_some() || !root.stored.is_empty() {
                return Err(failure(
                    "replacement carrier requires one actual inline root",
                ));
            }
            let actual_path = if root.place.is_empty() {
                path.clone()
            } else {
                slot_position(&get(*storage)?.pointer, path, b)?.place
            };
            let new = retain_slot(&get(*value)?.pointer, i, u32::MAX, path, slot_ids, loans, b)?;
            // An incoming block may not have reached this solver iteration
            // yet. Final windows require a nonempty replacement permission.
            fee(
                b,
                path.len()
                    .saturating_add(new.targets.len())
                    .saturating_add(new.loans.len()),
            )?;
            let holder = actual_slot_rows_mut(state, root.base, b)?;
            ordered(b, holder.len(), actual_path.len())?;
            holder.insert(actual_path, new);
            if !state.vacant.is_empty() {
                let position = slot_position(&get(*storage)?.pointer, path, b)?;
                if target_width(&position) != 0 {
                    ordered(b, state.vacant.len(), target_width(&position))?;
                }
                state.vacant.remove(&position);
            }
        }
        InstructionKind::LoadStoredBorrow { .. } => {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "stored reborrow/take/replacement CFG consumer is not yet implemented",
                Some(i.span),
            )]);
        }
        InstructionKind::ForeignCall { function, .. } => {
            let declaration = module
                .functions
                .get(function.0 as usize)
                .ok_or_else(|| failure("scalar foreign target is absent"))?;
            if !super::stored_callables::scalar_foreign(declaration, b)? {
                return Err(failure(
                    "foreign call has pointer or owning payload permissions",
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
                .ok_or_else(|| failure("call target absent"))?;
            let summary = summaries
                .get(function.0 as usize)
                .ok_or_else(|| failure("callee stored summary absent"))?;
            out.enum_variants = summary
                .returned_enum_variants
                .substitute(arguments, values, b)?;
            out.option_variants = summary.returned_variants.substitute(arguments, values, b)?;
            task_address::call(summary, arguments, values, state, &mut out, b)?;
            for (path, sources) in &summary.returned_environment {
                let mut permission = Permission::default();
                for source in sources {
                    union(
                        &mut permission,
                        &substitute(source, arguments, values, state, b)?,
                        b,
                    )?;
                }
                fee(b, path.len())?;
                environment_leaf(&mut out, path.clone(), &permission, b)?;
            }
            dyn_return::call(callee, summary, arguments, values, state, &mut out, b)?;
            for (path, sources) in &summary.returned {
                let mut permission = Permission::default();
                for source in sources {
                    union(
                        &mut permission,
                        &substitute(source, arguments, values, state, b)?,
                        b,
                    )?;
                    if path.is_empty()
                        && matches!(callee.return_type, Type::Address { mutable: true, .. })
                        && !source.stored.is_empty()
                        && let HoldRoot::Parameter(index) = source.base
                    {
                        let actual = arguments
                            .get(index as usize)
                            .and_then(|v| values.get(v.0 as usize))
                            .ok_or_else(|| failure("stored result carrier parameter absent"))?;
                        for loan in &actual.pointer.loans {
                            fee(b, 1)?;
                            permission.loans.insert(*loan);
                        }
                    }
                }
                if path.is_empty() {
                    out.pointer = permission;
                } else if (out.enum_variants.present() && out.enum_variants.slot_active(path))
                    || (!out.enum_variants.present()
                        && (!option_address::candidate(&callee.return_type)
                            || out.option_variants.mask() & 2 != 0))
                {
                    ordered(b, out.slots.len(), path.len())?;
                    fee(b, path.len())?;
                    out.slots.insert(path.clone(), permission);
                }
            }
            if !summary.returned_head_inputs.is_empty() {
                for position in &summary.returned_head_inputs {
                    let argument=*arguments.get(*position as usize).ok_or_else(||failure("returned head dependency input absent"))?;
                    let actual=&get(argument)?.pointer;
                    for loan in &actual.loans {ordered(b,out.pointer.loans.len(),0)?;out.pointer.loans.insert(*loan);}
                }
            }
            let mut updates = Vec::new();
            // Moving F transfers its environment ticket, while writes through
            // captured addresses still affect the caller's original storage.
            // Reconstruct all terminal rows before publishing any destination.
            let mut moved_contexts=Vec::new();
            for (position,local) in callee.parameters.iter().enumerate() {
                let Type::Closure(id)=&callee.locals[local.0 as usize].ty else {continue;};
                let definition=&module.closures[id.0 as usize];
                if !capture_after::has_context(module,definition,b)? {continue;}
                let argument=*arguments.get(position).ok_or_else(||failure("moved context argument absent"))?;
                let before=environment_memory::current(get(argument)?,state,b)?;
                let mut after=copy_permission_rows(&before,b)?;
                for (path,_) in capture_after::leaves(module,definition,b)? {
                    let mut permission=Permission::default();
                    if let Some(sources)=summary.external_after.get(&(position as u32,path.clone())) {
                        for source in sources {union(&mut permission,&substitute(source,arguments,values,state,b)?,b)?;}
                    }
                    if !permission.targets.is_empty() {
                        let stable=if summary.external_stable.is_empty() {false} else {
                            ordered(b,summary.external_stable.len(),path.len())?;
                            summary.external_stable.contains(&(position as u32,path.clone()))
                        };
                        if stable {
                            ordered(b,before.len(),path.len())?;
                            let previous=before.get(&path).ok_or_else(||failure("stable moved context omits actual previous row"))?;
                            permission_clone_work(previous,b)?;permission_clone_work(&permission,b)?;
                            if *previous!=permission {return Err(failure("stable moved context changed actual targets or loans"));}
                        } else {
                            permission=retain_slot(&permission,i,position as u32,&path,slot_ids,loans,b)?;
                        }
                    }
                    ordered(b,after.len(),path.len())?;
                    after.insert(path,permission);
                }
                moved_contexts.push((definition,before,after));
            }
            for ((parameter, path), sources) in &summary.external_after {
                let index = *parameter as usize;
                let local = *callee
                    .parameters
                    .get(index)
                    .ok_or_else(|| failure("external summary parameter is absent"))?;
                if !matches!(&callee.locals[local.0 as usize].ty,
                    Type::Address { mutable: true, inner } if carrier(inner).is_some() || matches!(inner.as_ref(),Type::Closure(_)))
                {
                    continue;
                }
                let actual = get(*arguments
                    .get(index)
                    .ok_or_else(|| failure("external summary actual argument is absent"))?)?;
                if actual.pointer.targets.is_empty() {
                    return Err(failure("external update has no actual carrier root"));
                }
                let strong=actual.pointer.targets.len()==1;
                let mut permission = Permission::default();
                for source in sources {
                    union(
                        &mut permission,
                        &substitute(source, arguments, values, state, b)?,
                        b,
                    )?;
                }
                if permission.targets.is_empty() {
                    return Err(failure(
                        "external update has no grounded replacement target",
                    ));
                }
                fee(b, path.len().saturating_add(permission.targets.len()))?;
                let stable=if summary.external_stable.is_empty() {false} else {
                    ordered(b,summary.external_stable.len(),path.len())?;
                    summary.external_stable.contains(&(*parameter,path.clone()))
                };
                let permission = if stable {
                    let previous=match &callee.locals[local.0 as usize].ty {
                        Type::Address{inner,..} if matches!(inner.as_ref(),Type::Closure(_))=>{
                            let rows=environment_memory::current(actual,state,b)?;
                            copy_permission(rows.get(path).ok_or_else(||failure("stable callable row absent"))?,b)?
                        },
                        _=>select(state,&actual.pointer,path,b)?,
                    };
                    permission_clone_work(&previous,b)?;permission_clone_work(&permission,b)?;
                    if previous!=permission {return Err(failure("stable external effect changed actual targets or loans"));}
                    permission
                } else {retain_slot(&permission, i, *parameter, path, slot_ids, loans, b)?};
                for root in &actual.pointer.targets {
                    fee(b,target_width(root).saturating_add(path.len()))?;
                    if !root.stored.is_empty() || root.heap_element.is_some() || root.retained_port.is_some() {
                        return Err(failure("external update requires an actual inline carrier root"));
                    }
                    let mut actual_path=root.place.clone();
                    actual_path.extend(path);
                    updates.push((root.base, actual_path, copy_permission(&permission,b)?, match &callee.locals[local.0 as usize].ty {
                        Type::Address { inner, .. } => match inner.as_ref() { Type::Closure(id) => Some(*id), _ => None },
                        _ => None,
                    },strong));
                }
            }
            let mut environments = BTreeMap::new();
            for (base, path, permission, environment,strong) in updates {
                if let Some(id) = environment {
                    ordered(b, environments.len(), 0)?;
                    if let std::collections::btree_map::Entry::Vacant(entry)=environments.entry(base) {
                        entry.insert((id,copy_permission_rows(environment_memory::rows(state,base,b)?,b)?));
                    }
                    let (_,rows)=environments.get_mut(&base).unwrap();
                    ordered(b,rows.len(),path.len())?;
                    if strong {rows.insert(path,permission);} else {
                        let previous=rows.get_mut(&path).ok_or_else(||failure("weak environment update omits an actual previous row"))?;
                        union(previous,&permission,b)?;
                    }
                } else {
                    let holder=actual_slot_rows_mut(state,base,b)?;
                    ordered(b,holder.len(),path.len())?;
                    if strong {holder.insert(path,permission);} else {
                        let previous=holder.get_mut(&path).ok_or_else(||failure("weak holder update omits an actual previous slot"))?;
                        union(previous,&permission,b)?;
                    }
                }
            }
            for (definition,before,after) in moved_contexts {
                capture_after::publish_contexts(module,definition,state,&before,&after,b)?;
            }
            for (base,(id,rows)) in environments {
                let before=copy_permission_rows(environment_memory::rows(state,base,b)?,b)?;
                let definition=module.closures.get(id.0 as usize).ok_or_else(||failure("external mutable environment definition absent"))?;
                capture_after::publish_contexts(module,definition,state,&before,&rows,b)?;
                environment_memory::publish(state,base,rows,b)?;
            }
            if callee.foreign.is_some()
                || callee.environment.is_some()
                || callee.call_kind != CallKind::Direct
            {
                return Err(failure(
                    "stored call requires an independently checked ordinary direct body",
                ));
            }
        }
        InstructionKind::MakeClosure { closure, captures }
            if module.closures[closure.0 as usize].storage_life.is_some()
                || b.returned_borrowed_environments.contains(closure) =>
        {
            let mut complete = true;
            for (index, capture) in captures.iter().enumerate() {
                let actual = get(*capture)?;
                if matches!(value_type(f, *capture)?, Type::Address { .. }) {
                    complete &= !actual.pointer.targets.is_empty();
                    let path = environment_path(index, &[], b)?;
                    environment_leaf(&mut out, path, &actual.pointer, b)?;
                }
                for (nested, permission) in &actual.environment {
                    let path = environment_path(index, nested, b)?;
                    complete &= !permission.targets.is_empty();
                    environment_leaf(&mut out, path, permission, b)?;
                }
                // An addressed carrier's SSA slots are an earlier snapshot.
                // Its current memory leaves are selected below, exactly once.
                // By-value carriers have no addressed memory to select from.
                if pointed_carrier(value_type(f,*capture)?).is_none() {
                    for (nested, permission) in &actual.slots {
                        let path = environment_path(index, nested, b)?;
                        complete &= !permission.targets.is_empty();
                        environment_leaf(&mut out, path, permission, b)?;
                    }
                }
                if let Some((_, shape)) = pointed_carrier(value_type(f, *capture)?) {
                    for slot in &shape.slots {
                        let leaf = select(state, &actual.pointer, &slot.path, b)?;
                        complete &= !leaf.targets.is_empty();
                        let path = environment_path(index, &slot.path, b)?;
                        environment_leaf(&mut out, path, &leaf, b)?;
                    }
                }
            }
            out.creation_complete = Some(complete);
        }
        InstructionKind::BorrowCapture { index, .. }
            if f.environment.is_some_and(|id| b.returned_borrowed_environments.contains(&id))
                && f.environment.and_then(|id|module.closures.get(id.0 as usize))
                    .and_then(|definition|definition.captures.get(*index as usize))
                    .is_some_and(|ty|matches!(ty, Type::Closure(_))) =>
        {
            let actual = state.captures.get(index)
                .ok_or_else(||failure("nested captured environment has no actual frame value"))?;
            out = snapshot(actual, b)?;
            if let Some(marker)=state.receiver_loan {fee(b,1)?;out.pointer.loans.insert(marker);}
            fee(b, 1)?;
            out.pointer.targets.insert(Target {
                retained_port: None,
                base: HoldRoot::Capture(*index),
                stored: Vec::new(), place: Vec::new(), heap_parents: Vec::new(), heap_element: None,
            });
        }
        InstructionKind::BorrowCapture {
            index,
            mutable: false,
        } => {
            let d = f
                .environment
                .and_then(|id| module.closures.get(id.0 as usize))
                .filter(|d| super::stored_callables::lending(d))
                .ok_or_else(|| failure("inline capture loan has no lending environment"))?;
            if !d
                .captures
                .get(*index as usize)
                .is_some_and(super::stored_callables::scalar)
            {
                return Err(failure("inline capture loan requires a real scalar field"));
            }
            fee(b, 1)?;
            out.pointer.targets.insert(Target {
                retained_port: None,
                base: HoldRoot::Capture(*index),
                stored: vec![],
                place: vec![],
                heap_parents: Vec::new(),
                heap_element: None,
            });
            if let Some(marker)=state.receiver_loan {
                fee(b,1)?;
                out.pointer.loans.insert(marker);
            }
        }
        kind if super::stored_callables::called(module,f,kind).is_some_and(|(d,_)|receiver_results::applies(d)) => {
            out=receiver_results::invoke(module,f,i,state,values,summaries,slot_ids,loans,b)?;
        }
        kind if super::stored_callables::called(module, f, kind)
            .is_some_and(|(d, _)| super::stored_callables::lending(d)) =>
        {
            let (d, _) = super::stored_callables::called(module, f, kind)
                .ok_or_else(|| failure("lending callee missing"))?;
            let mut receiver = Permission::default();
            match kind {
                InstructionKind::CallClosureBorrowed { callee, .. } => {
                    union(&mut receiver, &get(*callee)?.pointer, b)?;
                }
                InstructionKind::CallClosureShared { callee, .. }
                | InstructionKind::CallClosureMut { callee, .. } => {
                    fee(b, 1)?;
                    receiver.targets.insert(Target {
                        retained_port: None,
                        base: HoldRoot::Local(*callee),
                        stored: vec![],
                        place: vec![],
                        heap_parents: Vec::new(),
                        heap_element: None,
                    });
                }
                _ => {
                    return Err(failure(
                        "lending invocation must borrow its actual runtime environment",
                    ));
                }
            }
            if let Some(sources) = summaries[d.function.0 as usize].returned.get(&Vec::new()) {
                for source in sources {
                    let HoldRoot::Capture(index) = source.base else {
                        return Err(failure(
                            "callee lending result lacks an actual capture source",
                        ));
                    };
                    if !source.stored.is_empty() {
                        return Err(failure("lender cannot return an external capture target"));
                    }
                    for target in &receiver.targets {
                        fee(
                            b,
                            target_width(target)
                                .saturating_add(source.place.len())
                                .saturating_add(3),
                        )?;
                        let mut target = target.clone();
                        target.place.push(PlaceProjection::Field(index));
                        target.place.extend_from_slice(&source.place);
                        ordered(b, out.pointer.targets.len(), target_width(&target))?;
                        out.pointer.targets.insert(target);
                    }
                    for loan in &receiver.loans {
                        ordered(b, out.pointer.loans.len(), 0)?;
                        out.pointer.loans.insert(*loan);
                    }
                }
            }
        }
        InstructionKind::WriteCapture { index, value } if !state.captures.is_empty() => {
            let actual=snapshot(get(*value)?,b)?;
            ordered(b,state.captures.len(),0)?;
            if !state.captures.contains_key(index) {return Err(failure("written actual capture absent"));}
            state.captures.insert(*index,actual);
        }
        InstructionKind::ReadCapture { index } | InstructionKind::TakeCapture { index }
            if !state.captures.is_empty()
                && (matches!(i.kind,InstructionKind::ReadCapture{..})
                    || f.environment.and_then(|id|module.closures.get(id.0 as usize))
                        .and_then(|definition|definition.captures.get(*index as usize))
                        .is_some_and(|ty|matches!(ty,Type::Closure(_)) || super::stored_callables::exclusive_capture(ty))) => {
            ordered(b, state.captures.len(), 0)?;
            out = snapshot(
                state
                    .captures
                    .get(index)
                    .ok_or_else(|| failure("actual environment capture is absent"))?,
                b,
            )?;
            if matches!(i.kind,InstructionKind::TakeCapture{..}) {
                let d=f.environment.and_then(|id|module.closures.get(id.0 as usize)).ok_or_else(||failure("capture transfer has no actual entry"))?;
                if d.kind!=CallableKind::FnOnce {return Err(failure("capture capability transfer requires a genuinely consuming entry"));}
                state.captures.remove(index);
            }
            if matches!(i.kind, InstructionKind::ReadCapture { .. })
                && f.environment.and_then(|id| module.closures.get(id.0 as usize))
                    .and_then(|d| d.captures.get(*index as usize))
                    .is_some_and(super::stored_callables::exclusive_capture)
            {
                fee(b, 1)?;
                out.pointer.loans.insert(state.receiver_loan.ok_or_else(||
                    failure("exclusive capture read has no actual receiver invocation"))?);
            }
        }
        InstructionKind::DescribePackageField { local, index }
            if pure_copy::ordinary_package_description(f, i, b)? => {
            fee(b, 2)?;
            let base = state.capture_views.get(local).copied()
                .map_or(HoldRoot::Local(*local), HoldRoot::Capture);
            out.held.targets.insert(Target {
                retained_port: None, base, stored: Vec::new(),
                place: vec![PlaceProjection::Field(*index)], heap_parents: Vec::new(), heap_element: None,
            });
        }
        InstructionKind::DescribeLocal { local } if b.stored_owned_fixed => {
            owned::describe(&mut out, HoldRoot::Local(*local), b)?;
        }
        InstructionKind::DescribeLocal { local }
            if matches!(f.locals.get(local.0 as usize).map(|local| &local.ty),
                Some(Type::Managed(payload)) if super::stored_callables::resource_payload(payload, b)?) =>
        {
            owned::describe(&mut out, HoldRoot::Local(*local), b)?;
        }
        InstructionKind::DescribeCapture { index } if b.stored_owned_fixed => {
            owned::describe(&mut out, HoldRoot::Capture(*index), b)?;
        }
        InstructionKind::DescribeCapture { index }
            if f.environment.and_then(|id|module.closures.get(id.0 as usize))
                .and_then(|definition|definition.captures.get(*index as usize))
                .is_some_and(|ty|matches!(ty,Type::Managed(payload) if super::stored_callables::scalar(payload) || matches!(payload.as_ref(),Type::Record{..}))) => {
            owned::instruction(module,f,i,b)?;
            owned::describe(&mut out,HoldRoot::Capture(*index),b)?;
        }
        InstructionKind::DropManaged { local } if b.stored_owned_fixed || b.stored_compound_values => {
            state.locals.remove(local);
        }
        InstructionKind::DropTuple { local } if failure_tuple_drop(f, i, b)? => {
            ordered(b, state.locals.len(), 0)?;
            state.locals.remove(local);
            clear_local_holes(state, *local, b)?;
        }
        InstructionKind::DropPackage { local }
            if carrier(&f.locals[local.0 as usize].ty).is_some() =>
        {
            ordered(b, state.locals.len(), 0)?;
            state.locals.remove(local);
            clear_local_holes(state, *local, b)?;
        }
        InstructionKind::DropPackage { local } if b.masked_stored_tasks() => {
            ordered(b, state.locals.len(), 0)?;
            state.locals.remove(local);
            clear_local_holes(state, *local, b)?;
        }
        InstructionKind::DropPackage { local }
            if owned_byte_views::drop_package(module, f, *local, b)? =>
        {
            ordered(b, state.locals.len(), 0)?;
            state.locals.remove(local);
            clear_local_holes(state, *local, b)?;
        }
        InstructionKind::DescribeAddress { address }
            if owned::scalar_description(f, i, b)? => {
            union(&mut out.held, &get(*address)?.pointer, b)?;
            union(&mut out.store_description, &get(*address)?.pointer, b)?;
        }
        InstructionKind::DescribeAddress { address }
            if b.masked_stored_tasks() || b.shared_byte_borrow && i.result.is_some_and(|result| matches!(f.values[result.0 as usize], Type::Desc(ref payload) if payload.as_ref() == &Type::ByteBuffer)) => {
            // A weak description carries an actual header origin, but grants
            // no element permission until the checked Store borrow.
            union(&mut out.store_description, &get(*address)?.pointer, b)?;
        }
        InstructionKind::OwnedStoreBorrow { store, .. }
        | InstructionKind::OwnedStoreBorrowMut { store, .. } => {
            let description = &get(*store)?.store_description;
            if description.targets.is_empty() {
                return Err(failure(
                    "Store element window has no actual manager description",
                ));
            }
            for source in &description.targets {
                fee(b, target_width(source).saturating_add(1))?;
                let mut target = source.clone();
                if let Some(parent) = target.heap_element.take() {
                    if !b.nested_store_windows {
                        return Err(failure("nested Store window lacks complete recursive resource qualification"));
                    }
                    fee(b, parent.len().saturating_add(1))?;
                    target.heap_parents.push(parent);
                }
                target.heap_element = Some(Vec::new());
                ordered(b, out.pointer.targets.len(), target_width(&target))?;
                out.pointer.targets.insert(target);
            }
            fee(b, description.loans.len())?;
            out.pointer.loans.extend(&description.loans);
        }
        InstructionKind::ByteBufferBorrow { mutable: false, .. } => {
            out.pointer = owned_byte_views::transfer(module, f, i, values, b)?;
        }
        InstructionKind::ByteBufferNew
        | InstructionKind::ByteBufferLen { .. }
        | InstructionKind::ByteBufferCapacity { .. }
        | InstructionKind::DescribeAddress { .. }
        | InstructionKind::DropManaged { .. } => {
            // The resource proof owns these effects. None creates a stored
            // pointer or changes an existing stored target.
        }
        InstructionKind::ByteBufferPush { .. }
        | InstructionKind::ByteBufferReserve { .. }
        | InstructionKind::ByteBufferTruncate { .. } => {
            // A push can relocate ByteBuffer payload bytes. Until that
            // invalidation has a common projected source, refuse every
            // possible stored byte pointer in this function.
            for ty in f.locals.iter().map(|local| &local.ty).chain(&f.values) {
                if let Some((storage, life)) = carrier(ty) {
                    fee(b, life.slots.len())?;
                    for row in &life.slots {
                        if matches!(slot(storage, &row.path, b)?, Type::Address { inner, .. }
                            if inner.as_ref() == &Type::U8)
                        {
                            return Err(vec![Diagnostic::unsupported(
                                Phase::Analyze,
                                "ByteBuffer relocation needs projected stored byte target invalidation",
                                Some(i.span),
                            )]);
                        }
                    }
                }
            }
        }
        _ if b.stored_owned_fixed && owned::operation(&i.kind) => {}
        InstructionKind::DescribeLocal { .. }
        | InstructionKind::DescribeCapture { .. }
        | InstructionKind::MakeManaged { .. }
        | InstructionKind::ReadDesc { .. } => {}
        _ if dyn_transfer.is_some() => {
            out = dyn_transfer.unwrap();
        }
        _ if failure_wrapper.is_some() => {
            out = failure_wrapper.unwrap();
        }
        _ if enum_transfer.is_some() => {
            out = enum_transfer.unwrap();
        }
        _ if option_transfer.is_some() => {
            out = option_transfer.unwrap();
        }
        _ if pure_copy::ordinary_owned_union(f, i, b)? => {}
        InstructionKind::BorrowPackageField { local, index, .. } if pure_copy::ordinary_package_borrow(module, f, i, b)? => {
            fee(b,2)?;
            out.pointer.targets.insert(Target {
                retained_port:None,
                base:state.capture_views.get(local).copied().map_or(HoldRoot::Local(*local),HoldRoot::Capture),
                stored:Vec::new(),place:vec![PlaceProjection::Field(*index)],heap_parents:Vec::new(),heap_element:None,
            });
        }
        _ if pure_copy::ordinary_package_read(f, i, b)? => {}
        _ if task_transfer.is_some() => {
            out = task_transfer.unwrap();
        }
        InstructionKind::ExtractIndex { aggregate, .. } if array_index::candidate(f, i) => {
            out.pointer = array_index::transfer(f, i, get(*aggregate)?, b)?;
        }
        InstructionKind::DropTuple{local} if compound_drop(module,f,i,b)?=>{
            state.locals.remove(local);clear_local_holes(state,*local,b)?;
        }
        InstructionKind::UnpackTuple{local,fields} if compound_unpack(module,f,i,b)?=>{
            let mut source=state.locals.remove(local).ok_or_else(||failure("compound unpack source unavailable"))?;
            for (index,field) in fields.iter().enumerate(){
                let mut child=Value::default();
                child.slots=take_package_field_slots(&mut source,u32::try_from(index).map_err(|_|failure("compound field overflow"))?,b)?;
                if matches!(&f.locals[field.0 as usize].ty,Type::Address{..}) {
                    // A summary may still be empty in this fixed-point iteration.
                    // Final address uses and returns independently demand real
                    // nonempty targets; an empty candidate grants no permission.
                    child.pointer=child.slots.remove(&Vec::new()).unwrap_or_default();
                    if !child.slots.is_empty(){return Err(failure("anonymous Tuple address child has stray inline source slots"))}
                }
                union(&mut child.held,&source.held,b)?;
                ordered(b,state.locals.len(),0)?;state.locals.insert(*field,child);
            }
        }
        InstructionKind::TakeTupleField{local,path} if compound_move(module,f,i,b)?=>{
            let Some(PlaceProjection::Field(index))=path.first() else{return Err(failure("compound take lacks actual field"))};
            if path.len()!=1{return Err(failure("compound take nested path needs its actual position consumer"))}
            let holder=state.locals.get_mut(local).ok_or_else(||failure("compound take source unavailable"))?;
            out.slots=take_package_field_slots(holder,*index,b)?;
            union(&mut out.held,&holder.held,b)?;
        }
        InstructionKind::ExtractField {aggregate,index} if compound_extract(module,f,i,b)?=>{
            let parent=value_type(f,*aggregate)?;
            if shared_compound(module,parent,b)? {
                let physical=carrier(parent).map_or(parent,|(storage,_)|storage);
                let Type::Tuple(fields)=physical else{unreachable!()};
                let child=fields.get(*index as usize).ok_or_else(||failure("compound extraction field absent"))?;
                let result=i.result.ok_or_else(||failure("compound extraction result absent"))?;
                if value_type(f,result)?!=child{return Err(failure("compound extraction changes complete child type"))}
                let source=get(*aggregate)?;
                for (path,permission) in &source.slots{
                    fee(b,path.len().saturating_add(1))?;
                    if let Some((PlaceProjection::Field(field),tail))=path.split_first() && field==index {
                        if matches!(child,Type::Address{..}) && tail.is_empty(){
                            union(&mut out.pointer,permission,b)?;
                        }else{
                            union(out.slots.entry(tail.to_vec()).or_default(),permission,b)?;
                        }
                    }
                }
                union(&mut out.held,&source.held,b)?;
            }
        }
        _ if pure_copy::operation(&i.kind) => {
            if !pure_copy::ordinary_owned_union(f, i, b)? {
                pure_copy::types(f, i, b)?;
            }
            for input in resources::uses(&i.kind) {
                let payload = get(input)?;
                if !payload.pointer.targets.is_empty()
                    || !payload.pointer.loans.is_empty()
                    || !payload.slots.is_empty()
                    || !payload.held.targets.is_empty()
                    || !payload.held.loans.is_empty()
                    || !payload.environment.is_empty()
                {
                    return Err(failure(
                        "ordinary pure Copy input carries actual stored target permissions",
                    ));
                }
            }
        }
        _ if super::stored_callables::instruction(module, f, i) => {}
        InstructionKind::FunctionRef { .. } | InstructionKind::CallIndirect { .. }
            if super::class_flow::pointer_instruction(f, i) => {}
        InstructionKind::ReadAddress { address } => {
            let Type::Address { inner, .. } = value_type(f, *address)? else {
                return Err(failure("read transfer lacks an actual Address"));
            };
            if shared_array_shape(inner, b)? {
                let Type::Array { length, .. } = inner.as_ref() else {
                    unreachable!()
                };
                fee(b, (*length as usize).saturating_mul(2))?;
                for index in 0..*length {
                    let path = vec![PlaceProjection::Field(index)];
                    let permission = select(state, &get(*address)?.pointer, &path, b)?;
                    ordered(b, out.slots.len(), 1)?;
                    out.slots.insert(path, permission);
                }
            }
            if carrier(inner).is_some() && !pure_read(module, inner, b)? {
                return Err(failure(
                    "read transfer cannot erase actual borrowed/owned leaves",
                ));
            }
            // Only the source Address permission is used by access checking.
            // A pure value has no borrowed targets to copy: the full result
            // type carries its nominal/retained classes, without rooting them
            // in the storage bytes or inventing a loan from a phantom class.
        }
        InstructionKind::WriteAddress { address, .. } => {
            let Type::Address { inner, .. } = value_type(f, *address)? else {
                return Err(failure("write base is not an actual Address"));
            };
            b.type_work(inner, true, "stored profile actual ordinary write payload")?;
            if !inner.is_copy_value() {
                return Err(failure(
                    "ordinary write cannot replace stored leaves or qualified carriers; use the checked slot protocol",
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
        | InstructionKind::ConvertInteger { .. }
        | InstructionKind::ConvertNumber { .. }
        | InstructionKind::Unary { .. }
        | InstructionKind::Binary { .. }
        | InstructionKind::SliceLength { .. }
        | InstructionKind::ScopeEnter { .. }
        | InstructionKind::ScopeExit { .. } => {}
        _ => {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                format!("instruction has no common stored-target transfer yet in {}: {:?}",f.name,i.kind),
                Some(i.span),
            )]);
        }
    }
    capture_after::invoke(module, f, i, state, values, summaries, slot_ids, loans, b)?;
    if let Some(result) = i.result
        && let Type::Desc(payload) = value_type(f, result)?
        && matches!(
            payload.as_ref(),
            Type::OwnedStore { .. } | Type::ByteBuffer | Type::Record { .. }
        )
    {
        let source = match i.kind {
            InstructionKind::DescribeLocal { local } => {
                if f.locals.get(local.0 as usize).map(|local| &local.ty)
                    != Some(&Type::Managed(payload.clone()))
                {
                    return Err(failure(
                        "Store description lacks its actual owning local head",
                    ));
                }
                Some((HoldRoot::Local(local), vec![]))
            }
            InstructionKind::DescribePackageField { local, index } => {
                let Some(Type::Package { fields, .. }) =
                    f.locals.get(local.0 as usize).map(|local| &local.ty)
                else {
                    return Err(failure("Store description field lacks an actual Package"));
                };
                fee(b, fields.len())?;
                if fields.get(index as usize).map(|(_, field)| field)
                    != Some(&Type::Managed(payload.clone()))
                {
                    return Err(failure(
                        "Store description differs from its actual managed field",
                    ));
                }
                Some((HoldRoot::Local(local), vec![PlaceProjection::Field(index)]))
            }
            InstructionKind::DescribeCapture { index } => {
                let actual = f
                    .environment
                    .and_then(|id| module.closures.get(id.0 as usize))
                    .and_then(|definition| definition.captures.get(index as usize));
                if actual != Some(&Type::Managed(payload.clone())) {
                    return Err(failure(
                        "Store description capture is not its actual owning head",
                    ));
                }
                Some((HoldRoot::Capture(index), vec![]))
            }
            _ => None,
        };
        if let Some((base, place)) = source {
            fee(b, place.len().saturating_add(1))?;
            out.store_description.targets.insert(Target {
                retained_port: None,
                base,
                stored: vec![],
                place,
                heap_parents: vec![],
                heap_element: None,
            });
        }
    }
    if let Some(permission) = owned_byte_views::seed_description(module, f, i, values, b)? {
        out.store_description = permission;
    }
    dyn_address::propagate(module, f, i, state, values, &mut out, b)?;
    if let Some(result) = i.result
        && b.static_utf8
        && matches!(value_type(f, result)?, Type::Address { mutable: true, .. })
    {
        fee(b, out.pointer.targets.len())?;
        if out
            .pointer
            .targets
            .iter()
            .any(|t| t.base == HoldRoot::Static)
        {
            return Err(failure("static UTF8 cannot produce a mutable address"));
        }
    }
    if let Some(result) = i.result
        && (matches!(
            i.kind,
            InstructionKind::BorrowLocal { .. }
                | InstructionKind::BorrowLocalMut { .. }
                | InstructionKind::ReborrowAddress { .. }
                | InstructionKind::MakeDynBorrow { .. }
                | InstructionKind::BorrowDynOwned { .. }
                | InstructionKind::CallDynSlot { .. }
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
                | InstructionKind::CallClosureOnce { .. }
        ) || matches!(
            i.kind,
            InstructionKind::OwnedStoreBorrow { .. } | InstructionKind::OwnedStoreBorrowMut { .. }
        ) || matches!(i.kind, InstructionKind::ReadCapture { index }
                if f.environment.and_then(|id| module.closures.get(id.0 as usize))
                    .and_then(|d| d.captures.get(index as usize))
                    .is_some_and(super::stored_callables::exclusive_capture))
            || ordinary_error_borrow(f, i, b)?
            || pure_copy::ordinary_package_borrow(module, f, i, b)?
            || owned_byte_views::instruction(module, f, i, b)?)
        && matches!(value_type(f, result)?, Type::Address { .. })
    {
        let id = result.0 as usize;
        let loan = loans
            .get_mut(id)
            .ok_or_else(|| failure("fresh loan is outside actual values"))?;
        let mut targets = Permission {
            targets: std::mem::take(&mut loan.targets),
            loans: std::mem::take(&mut loan.parents),
        };
        union(&mut targets, &out.pointer, b)?;
        loan.targets = targets.targets;
        loan.parents = targets.loans;
        loan.mutable = matches!(value_type(f, result)?, Type::Address { mutable: true, .. });
        loan.receiver_head |= matches!(value_type(f,result)?,Type::Address{inner,..}if matches!(inner.as_ref(),Type::Closure(_)));
        b.stored_exclusive_loans |= loan.mutable;
        out.pointer.loans.clear();
        fee(b, 1)?;
        out.pointer.loans.insert(id);
    }
    if let Some(result) = i.result
        && matches!(i.kind, InstructionKind::Load(_))
        && matches!(value_type(f, result)?, Type::Address { mutable: true, .. })
    {
        let id = result.0 as usize;
        let loan = &mut loans[id];
        let mut collected = Permission {
            targets: std::mem::take(&mut loan.targets),
            loans: std::mem::take(&mut loan.parents),
        };
        union(&mut collected, &out.pointer, b)?;
        loan.targets = collected.targets;
        loan.parents = collected.loans;
        loan.mutable = true;
        b.stored_exclusive_loans = true;
        out.pointer.loans.clear();
        fee(b, 1)?;
        out.pointer.loans.insert(id);
    }
    if let Some(result) = i.result
        && matches!(i.kind, InstructionKind::Call { .. })
        && carrier(value_type(f, result)?).is_some()
    {
        let mut next = f.values.len();
        fee(b, result.0 as usize)?;
        for ty in &f.values[..result.0 as usize] {
            if let Some((_, shape)) = carrier(ty) {
                next = next
                    .checked_add(shape.slots.len())
                    .ok_or_else(|| failure("stored result loan count overflow"))?;
            }
        }
        let (_, shape) = carrier(value_type(f, result)?)
            .ok_or_else(|| failure("stored call result lacks actual shape"))?;
        for declaration in &shape.slots {
            ordered(b, out.slots.len(), declaration.path.len())?;
            if let Some(permission) = out.slots.get_mut(&declaration.path) {
                let loan = loans
                    .get_mut(next)
                    .ok_or_else(|| failure("stored call result loan outside actual arena"))?;
                let mut collected = Permission {
                    targets: std::mem::take(&mut loan.targets),
                    loans: std::mem::take(&mut loan.parents),
                };
                union(&mut collected, permission, b)?;
                loan.targets = collected.targets;
                loan.parents = collected.loans;
                loan.mutable = declaration.mode == StoredBorrowMode::ExclusiveAddress;
                b.stored_exclusive_loans |= loan.mutable;
                permission.loans.clear();
                fee(b, 1)?;
                permission.loans.insert(next);
            }
            next = next
                .checked_add(1)
                .ok_or_else(|| failure("stored result loan identity overflow"))?;
        }
    }
    Ok(out)
}

fn intersects(left: &Target, right: &Target, b: &mut Budget) -> CompileResult<bool> {
    fee(
        b,
        target_width(left)
            .saturating_add(target_width(right))
            .saturating_add(1),
    )?;
    if left.base != right.base
        || left.stored != right.stored
        || left.retained_port != right.retained_port
    {
        return Ok(false);
    }
    // A heap element is a distinct domain from the manager's inline fields.
    // Its Store header is pinned structurally, and dynamic indices cannot
    // establish element apartness from another window of the same manager.
    if left.heap_element.is_some() || right.heap_element.is_some() {
        return Ok(left.place.starts_with(&right.place) || right.place.starts_with(&left.place));
    }
    Ok(left.place.starts_with(&right.place) || right.place.starts_with(&left.place))
}
fn may_alias(left: &Target, right: &Target) -> bool {
    matches!(
        (left.base, right.base),
        (HoldRoot::Parameter(_), HoldRoot::Parameter(_))
    ) && (left.base != right.base
        || left.stored != right.stored
        || left.retained_port != right.retained_port)
}
fn separate(
    left: &Target,
    right: &Target,
    apart: &mut BTreeSet<(Target, Target)>,
    b: &mut Budget,
) -> CompileResult<()> {
    if intersects(left, right, b)? {
        return Err(vec![Diagnostic::error(
            Phase::Analyze,
            "E-BORROW-CONFLICT",
            None,
            "simultaneously live access overlaps an actual stored target/parent loan",
        )]);
    }
    if may_alias(left, right) {
        let width = target_width(left)
            .saturating_add(target_width(right))
            .saturating_add(2);
        ordered(b, apart.len(), width)?;
        fee(b, width)?;
        let pair = if left < right {
            (left.clone(), right.clone())
        } else {
            (right.clone(), left.clone())
        };
        apart.insert(pair);
    }
    Ok(())
}
fn ancestry(ids: &Loans, loans: &[Loan], b: &mut Budget) -> CompileResult<Loans> {
    let mut out = Loans::new();
    let mut pending = Vec::new();
    fee(b, ids.len())?;
    pending.extend(ids);
    while let Some(id) = pending.pop() {
        ordered(b, out.len(), 0)?;
        if out.insert(id) {
            let loan = loans
                .get(id)
                .ok_or_else(|| failure("loan ancestry is outside its actual table"))?;
            fee(b, loan.parents.len())?;
            pending.extend(&loan.parents);
        }
    }
    Ok(out)
}
fn permission_head(permission: &Permission, write: bool, b: &mut Budget) -> CompileResult<()> {
    if permission.targets.is_empty() {
        return Err(failure("address access has no actual target permission"));
    }
    if b.static_utf8 {
        fee(b, permission.targets.len())?;
    }
    if b.static_utf8
        && write
        && permission
            .targets
            .iter()
            .any(|t| t.base == HoldRoot::Static)
    {
        return Err(failure("program-static UTF8 has no mutable authority"));
    }
    Ok(())
}
fn permission_tail(
    permission: &Permission,
    write: bool,
    active: &Loans,
    loans: &[Loan],
    ignore: &Loans,
    apart: &mut BTreeSet<(Target, Target)>,
    b: &mut Budget,
) -> CompileResult<()> {
    for id in active {
        ordered(b, ignore.len(), 0)?;
        if ignore.contains(id) {
            continue;
        }
        let loan = loans.get(*id).ok_or_else(|| failure("live loan absent"))?;
        if write || loan.mutable {
            for target in &permission.targets {
                for held in &loan.targets {
                    separate(target, held, apart, b)?;
                }
            }
        }
    }
    Ok(())
}
fn use_permission(
    permission: &Permission,
    write: bool,
    active: &Loans,
    loans: &[Loan],
    apart: &mut BTreeSet<(Target, Target)>,
    b: &mut Budget,
) -> CompileResult<()> {
    permission_head(permission, write, b)?;
    let ignore = ancestry(&permission.loans, loans, b)?;
    permission_tail(permission, write, active, loans, &ignore, apart, b)
}
fn query_permission(
    permission: &Permission,
    active: &Loans,
    loans: &[Loan],
    apart: &mut BTreeSet<(Target, Target)>,
    b: &mut Budget,
) -> CompileResult<()> {
    permission_head(permission, false, b)?;
    permission_tail(
        permission,
        false,
        active,
        loans,
        &permission.loans,
        apart,
        b,
    )
}

fn active_loans_union(to: &mut Loans, from: &Permission, b: &mut Budget) -> CompileResult<()> {
    // Every visited Permission is work, including a target-only or empty row.
    // Only the unused target clone/search is removed by this projection.
    fee(b, 1)?;
    for loan in &from.loans {
        ordered(b, to.len(), 0)?;
        to.insert(*loan);
    }
    Ok(())
}
fn active(
    values: &[Value],
    state: &State,
    live: &LiveRow,
    loans: &[Loan],
    b: &mut Budget,
) -> CompileResult<Loans> {
    let mut direct = Loans::new();
    fee(b, live.work())?;
    for i in live.ones() {
        fee(b, 1)?;
        let value = if i < values.len() {
            Some(&values[i])
        } else {
            ordered(b, state.locals.len(), 0)?;
            state.locals.get(&LocalId((i - values.len()) as u32))
        };
        if let Some(value) = value {
            active_loans_union(&mut direct, &value.pointer, b)?;
            if !value.environment.is_empty() && !value.pointer.targets.is_empty() {
                for leaf in environment_memory::current(value, state, b)?.values() {
                    active_loans_union(&mut direct, leaf, b)?;
                }
            } else {
                active_loans_union(&mut direct, &value.held, b)?;
            }
            if !value.store_description.targets.is_empty()
                || !value.store_description.loans.is_empty()
            {
                active_loans_union(&mut direct, &value.store_description, b)?;
            }
            if b.dyn_contexts {
                for permission in value.dyn_ports.values() {
                    active_loans_union(&mut direct, permission, b)?;
                }
            }
            if b.task_sources() {
                for permission in value.task_ports.values() {
                    active_loans_union(&mut direct, permission, b)?;
                }
            }
            for leaf in value.slots.values() {
                active_loans_union(&mut direct, leaf, b)?;
            }
        }
    }
    if b.dyn_contexts {
        for value in state.locals.values().chain(state.captures.values()) {
            for permission in value.dyn_ports.values() {
                active_loans_union(&mut direct, permission, b)?;
            }
        }
    }
    if b.task_sources() {
        // Retained ports are frame obligations, even when this particular
        // borrowed head has no further SSA use in the callee.
        fee(b, state.locals.len())?;
        for value in state.locals.values() {
            for permission in value.task_ports.values() {
                active_loans_union(&mut direct, permission, b)?;
            }
        }
    }
    for (index, original) in &state.captures {
        let view = captured_view_local(state, HoldRoot::Capture(*index), b)?;
        let value = if let Some(local) = view {
            state
                .locals
                .get(&local)
                .ok_or_else(|| failure("active captured view unavailable"))?
        } else {
            original
        };
        active_loans_union(&mut direct, &value.pointer, b)?;
        if !value.environment.is_empty() {
            active_loans_union(&mut direct, &value.held, b)?;
        }
        if !value.store_description.targets.is_empty() || !value.store_description.loans.is_empty()
        {
            active_loans_union(&mut direct, &value.store_description, b)?;
        }
        for leaf in value.slots.values() {
            active_loans_union(&mut direct, leaf, b)?;
        }
    }
    if b.stored_slot_updates {
        // An empty inner map still consumes its actual outer frame visit.
        fee(b, state.external.len())?;
        for slots in state.external.values() {
            for leaf in slots.values() {
                active_loans_union(&mut direct, leaf, b)?;
            }
        }
    }
    ancestry(&direct, loans, b)
}
#[cfg(test)]
fn active_by_permission(
    values: &[Value],
    state: &State,
    live: &[bool],
    loans: &[Loan],
    b: &mut Budget,
) -> CompileResult<Loans> {
    let mut direct = Permission::default();
    fee(b, live.len())?;
    for (i, yes) in live.iter().enumerate() {
        if !*yes {
            continue;
        }
        let value = if i < values.len() {
            Some(&values[i])
        } else {
            ordered(b, state.locals.len(), 0)?;
            state.locals.get(&LocalId((i - values.len()) as u32))
        };
        if let Some(value) = value {
            union(&mut direct, &value.pointer, b)?;
            if !value.environment.is_empty() && !value.pointer.targets.is_empty() {
                for leaf in environment_memory::current(value, state, b)?.values() {
                    union(&mut direct, leaf, b)?;
                }
            } else {
                union(&mut direct, &value.held, b)?;
            }
            if !value.store_description.targets.is_empty()
                || !value.store_description.loans.is_empty()
            {
                union(&mut direct, &value.store_description, b)?;
            }
            if b.dyn_contexts {
                for permission in value.dyn_ports.values() {
                    union(&mut direct, permission, b)?;
                }
            }
            if b.task_sources() {
                for permission in value.task_ports.values() {
                    union(&mut direct, permission, b)?;
                }
            }
            for leaf in value.slots.values() {
                union(&mut direct, leaf, b)?;
            }
        }
    }
    if b.dyn_contexts {
        for value in state.locals.values().chain(state.captures.values()) {
            for permission in value.dyn_ports.values() {
                union(&mut direct, permission, b)?;
            }
        }
    }
    if b.task_sources() {
        // Retained ports are frame obligations, even when this particular
        // borrowed head has no further SSA use in the callee.
        for value in state.locals.values() {
            for permission in value.task_ports.values() {
                union(&mut direct, permission, b)?;
            }
        }
    }
    for (index, original) in &state.captures {
        let view = captured_view_local(state, HoldRoot::Capture(*index), b)?;
        let value = if let Some(local) = view {
            state
                .locals
                .get(&local)
                .ok_or_else(|| failure("active captured view unavailable"))?
        } else {
            original
        };
        union(&mut direct, &value.pointer, b)?;
        if !value.environment.is_empty() {
            union(&mut direct, &value.held, b)?;
        }
        if !value.store_description.targets.is_empty() || !value.store_description.loans.is_empty()
        {
            union(&mut direct, &value.store_description, b)?;
        }
        for leaf in value.slots.values() {
            union(&mut direct, leaf, b)?;
        }
    }
    if b.stored_slot_updates {
        for slots in state.external.values() {
            for leaf in slots.values() {
                union(&mut direct, leaf, b)?;
            }
        }
    }
    ancestry(&direct.loans, loans, b)
}
fn reverse(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    live: &mut LiveRow,
    b: &mut Budget,
) -> CompileResult<()> {
    fee(b, 1)?;
    if let Some(v) = i.result {
        live.put(v.0 as usize, false)?;
    }
    match i.kind {
        InstructionKind::Store { local, .. }
        | InstructionKind::StorageLive(local)
        | InstructionKind::StorageDead(local) => {
            live.put(f.values.len() + local.0 as usize, false)?
        }
        InstructionKind::Load(local)
        | InstructionKind::BorrowPlaceLocal { local, .. }
        | InstructionKind::BorrowLocal { local }
        | InstructionKind::BorrowLocalMut { local } => {
            live.put(f.values.len() + local.0 as usize, true)?
        }
        InstructionKind::DropClosure { local }
            if environment_value(module, &f.locals[local.0 as usize].ty, b)?
                && !capture_after::cleanup_reads_external(
                    module,
                    &f.locals[local.0 as usize].ty,
                    b,
                )? => {}
        InstructionKind::TakeLocal { local }
        | InstructionKind::TakePackageField { local, .. }
        | InstructionKind::DropPackageField { local, .. }
        | InstructionKind::StorePackageField { local, .. }
        | InstructionKind::DropClosure { local }
        | InstructionKind::CallClosureShared { callee: local, .. }
        | InstructionKind::CallClosureMut { callee: local, .. }
            if environment_value(module, &f.locals[local.0 as usize].ty, b)?
                || carrier(&f.locals[local.0 as usize].ty).is_some()
                || affine_compound(module, &f.locals[local.0 as usize].ty, b)?
                || matches!(i.kind, InstructionKind::TakeLocal { .. })
                    && matches!(f.locals[local.0 as usize].ty, Type::Address { .. }) =>
        {
            live.put(f.values.len() + local.0 as usize, true)?
        }
        _ => {}
    }
    for v in resources::uses(&i.kind) {
        fee(b, 1)?;
        live.put(v.0 as usize, true)?;
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
) -> CompileResult<()> {
    fee(b, 1)?;
    if let Some(v) = i.result {
        live[v.0 as usize] = false;
    }
    match i.kind {
        InstructionKind::Store { local, .. }
        | InstructionKind::StorageLive(local)
        | InstructionKind::StorageDead(local) => live[f.values.len() + local.0 as usize] = false,
        InstructionKind::Load(local)
        | InstructionKind::BorrowPlaceLocal { local, .. }
        | InstructionKind::BorrowLocal { local }
        | InstructionKind::BorrowLocalMut { local } => {
            live[f.values.len() + local.0 as usize] = true
        }
        InstructionKind::DropClosure { local }
            if environment_value(module, &f.locals[local.0 as usize].ty, b)?
                && !capture_after::cleanup_reads_external(
                    module,
                    &f.locals[local.0 as usize].ty,
                    b,
                )? => {}
        InstructionKind::TakeLocal { local }
        | InstructionKind::TakePackageField { local, .. }
        | InstructionKind::DropPackageField { local, .. }
        | InstructionKind::StorePackageField { local, .. }
        | InstructionKind::DropClosure { local }
        | InstructionKind::CallClosureShared { callee: local, .. }
        | InstructionKind::CallClosureMut { callee: local, .. }
            if environment_value(module, &f.locals[local.0 as usize].ty, b)?
                || carrier(&f.locals[local.0 as usize].ty).is_some()
                || affine_compound(module, &f.locals[local.0 as usize].ty, b)?
                || matches!(i.kind, InstructionKind::TakeLocal { .. })
                    && matches!(f.locals[local.0 as usize].ty, Type::Address { .. }) =>
        {
            live[f.values.len() + local.0 as usize] = true
        }
        _ => {}
    }
    for v in resources::uses(&i.kind) {
        fee(b, 1)?;
        live[v.0 as usize] = true;
    }
    Ok(())
}
fn term_live(t: &Terminator, live: &mut LiveRow) -> CompileResult<()> {
    match t {
        Terminator::Return(Some(v)) | Terminator::Branch { condition: v, .. } => {
            live.put(v.0 as usize, true)?
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
fn liveness(
    module: &TypedModule,
    f: &TypedFunction,
    preds: &[Vec<usize>],
    reachable: &[bool],
    b: &mut Budget,
) -> CompileResult<Vec<LiveRow>> {
    let width = f
        .values
        .len()
        .checked_add(f.locals.len())
        .ok_or_else(|| failure("stored liveness width overflow"))?;
    let work = LiveRow::work_for(width);
    b.product(f.blocks.len(), work, 2, "stored liveness tables")?;
    let mut inputs = vec![LiveRow::empty(width); f.blocks.len()];
    let mut outputs = inputs.clone();
    loop {
        let mut changed = false;
        for bi in (0..f.blocks.len()).rev() {
            fee(b, 1)?;
            if !reachable[bi] {
                continue;
            }
            fee(b, work)?;
            let mut live = outputs[bi].clone();
            term_live(&f.blocks[bi].terminator, &mut live)?;
            for i in f.blocks[bi].instructions.iter().rev() {
                reverse(module, f, i, &mut live, b)?;
            }
            fee(b, work)?;
            if live != inputs[bi] {
                fee(b, work)?;
                inputs[bi] = live;
                for pred in &preds[bi] {
                    // The old solver updates every predecessor, including
                    // unreachable predecessors of a reachable successor.
                    changed |= outputs[*pred].include(&inputs[bi], b)?;
                }
            }
        }
        if !changed {
            break;
        }
    }
    Ok(outputs)
}
#[cfg(test)]
fn liveness_bool(
    module: &TypedModule,
    f: &TypedFunction,
    preds: &[Vec<usize>],
    reachable: &[bool],
    b: &mut Budget,
) -> CompileResult<Vec<Vec<bool>>> {
    let width = f
        .values
        .len()
        .checked_add(f.locals.len())
        .ok_or_else(|| failure("stored liveness width overflow"))?;
    b.product(f.blocks.len(), width, 2, "stored liveness tables")?;
    let mut inputs = vec![vec![false; width]; f.blocks.len()];
    let mut outputs = inputs.clone();
    loop {
        let mut changed = false;
        for bi in (0..f.blocks.len()).rev() {
            fee(b, 1)?;
            if !reachable[bi] {
                continue;
            }
            fee(b, width)?;
            let mut live = outputs[bi].clone();
            term_live_bool(&f.blocks[bi].terminator, &mut live);
            for i in f.blocks[bi].instructions.iter().rev() {
                reverse_bool(module, f, i, &mut live, b)?;
            }
            fee(b, width)?;
            if live != inputs[bi] {
                fee(b, width)?;
                inputs[bi] = live;
                for pred in &preds[bi] {
                    fee(b, width)?;
                    for (to, from) in outputs[*pred].iter_mut().zip(&inputs[bi]) {
                        if *from && !*to {
                            *to = true;
                            changed = true;
                        }
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    Ok(outputs)
}

fn instruction_live(
    module: &TypedModule,
    f: &TypedFunction,
    bi: usize,
    output: &LiveRow,
    b: &mut Budget,
) -> CompileResult<Vec<LiveRow>> {
    let block = &f.blocks[bi];
    let work = output.work();
    b.product(
        block.instructions.len().saturating_add(1),
        work,
        1,
        "stored instruction liveness snapshots",
    )?;
    // Empty placeholders allocate row records before the paid word copies.
    b.spend(
        block.instructions.len(),
        "stored liveness snapshot containers",
    )?;
    let mut live = output.clone();
    term_live(&block.terminator, &mut live)?;
    let mut after = vec![LiveRow::empty(0); block.instructions.len()];
    for (position, i) in block.instructions.iter().enumerate().rev() {
        after[position] = live.clone();
        reverse(module, f, i, &mut live, b)?;
    }
    Ok(after)
}

fn record_head_access(
    into: &mut BTreeMap<Target, bool>,
    permission: &Permission,
    write: bool,
    b: &mut Budget,
) -> CompileResult<()> {
    fee(b, permission.targets.len())?;
    for target in &permission.targets {
        if !matches!(target.base, HoldRoot::Parameter(_)) {
            continue;
        }
        ordered(b, into.len(), target_width(target))?;
        fee(b, target_width(target).saturating_add(1))?;
        *into.entry(target.clone()).or_default() |= write;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn windows(
    module: &TypedModule,
    f: &TypedFunction,
    preds: &[Vec<usize>],
    reachable: &[bool],
    entry: &State,
    outputs: &[State],
    values: &[Value],
    slot_ids: &SlotLoanIds,
    loans: &mut [Loan],
    summaries: &[Facts],
    apart: &mut BTreeSet<(Target, Target)>,
    byte_reads: &mut Vec<Target>,
    head_accesses: &mut BTreeMap<Target, bool>,
    b: &mut Budget,
) -> CompileResult<()> {
    // A module marker only schedules this actual per-function inventory.
    let mut ordinary_error_places = false;
    if b.ordinary_error_places {
        fee(b, f.blocks.len())?;
        for block in &f.blocks {
            fee(b, block.instructions.len())?;
            for instruction in &block.instructions {
                ordinary_error_places |= ordinary_error_borrow(f, instruction, b)?;
            }
        }
    }
    let byte_flow =
        byte_queries::Flow::solve(module, f, preds, reachable, entry, values, summaries, b)?;
    let owned_byte_sites = owned_byte_views::Sites::collect(f, b)?;
    let live_out = liveness(module, f, preds, reachable, b)?;
    for (bi, block) in f.blocks.iter().enumerate() {
        fee(b, 1)?;
        if !reachable[bi] {
            continue;
        }
        let mut state = if bi == f.entry.0 as usize {
            state_copy(entry, b)?
        } else {
            State::default()
        };
        for pred in &preds[bi] {
            fee(b, 1)?;
            if reachable[*pred] {
                state_merge(&mut state, &outputs[*pred], b)?;
            }
        }
        let after = instruction_live(module, f, bi, &live_out[bi], b)?;
        let mut byte_point = byte_flow
            .as_ref()
            .map(|flow| flow.input(f, bi, preds, reachable, b))
            .transpose()?;
        for (position, i) in block.instructions.iter().enumerate() {
            fee(b, after[position].work())?;
            let mut live = after[position].clone();
            if let Some(v) = i.result {
                live.put(v.0 as usize, false)?;
            }
            let mut current = active(values, &state, &live, loans, b)?;
            let pointer = |v: ValueId| -> CompileResult<&Permission> {
                values
                    .get(v.0 as usize)
                    .map(|v| &v.pointer)
                    .ok_or_else(|| failure("access operand outside values"))
            };
            if let Some(flow) = &byte_flow {
                for permission in flow.reads(
                    module,
                    i,
                    values,
                    &state,
                    byte_point.as_ref().unwrap(),
                    summaries,
                    b,
                )? {
                    query_permission(&permission, &current, loans, apart, b)?;
                    byte_queries::record(byte_reads, &permission, b)?;
                }
            }
            // Callee reads happen before a new result loan is published. The
            // ordinary access path still excludes the instruction's result.
            if let Some(v) = i.result {
                current.remove(&(v.0 as usize));
            }
            owned_byte_sites.check(module, f, i, &state, values, &current, loans, apart, b)?;
            let checked = (|| -> CompileResult<()> {
                let mut access = permission_access::Access::new(&current, loans, b)?;
                if let Some((head, write)) = super::nested_store::head_access(&i.kind) {
                    let actual = &values[head.0 as usize].store_description;
                    if actual.targets.is_empty() {
                        return Err(failure("head operation has no actual descriptor source"));
                    }
                    access.check(actual, write, apart, b)?;
                    record_head_access(head_accesses, actual, write, b)?;
                }
                match &i.kind {
                    InstructionKind::ForeignCall {
                        function,
                        arguments,
                        ..
                    } => {
                        let callee = &module.functions[function.0 as usize];
                        for (index, (argument, parameter)) in
                            arguments.iter().zip(&callee.parameters).enumerate()
                        {
                            let Type::Address { mutable, .. } =
                                &callee.locals[parameter.0 as usize].ty
                            else {
                                continue;
                            };
                            let actual = pointer(*argument)?;
                            access.check(actual, *mutable, apart, b)?;
                            for (other, previous) in
                                arguments[..index].iter().zip(&callee.parameters[..index])
                            {
                                let Type::Address {
                                    mutable: previous_mutable,
                                    ..
                                } = &callee.locals[previous.0 as usize].ty
                                else {
                                    continue;
                                };
                                if *mutable || *previous_mutable {
                                    for left in &actual.targets {
                                        for right in &pointer(*other)?.targets {
                                            separate(left, right, apart, b)?;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    InstructionKind::MakeAggregate { fields }
                    | InstructionKind::MakePackage { fields }
                    | InstructionKind::MakeOwnedTuple { fields }
                        if b.stored_exclusive_loans =>
                    {
                        let result = i.result.ok_or_else(|| failure("aggregate lacks result"))?;
                        if let Some((_, shape)) = carrier(value_type(f, result)?) {
                            for (index, slot) in shape.slots.iter().enumerate() {
                                fee(b, slot.path.len().saturating_add(1))?;
                                let Some((PlaceProjection::Field(field), tail)) =
                                    slot.path.split_first()
                                else {
                                    return Err(failure("constructor slot has no input field"));
                                };
                                let source = &values[fields[*field as usize].0 as usize];
                                let permission = if tail.is_empty() {
                                    &source.pointer
                                } else {
                                    ordered(b, source.slots.len(), tail.len())?;
                                    source.slots.get(tail).ok_or_else(|| {
                                        failure("constructor has no actual input leaf")
                                    })?
                                };
                                let exclusive = slot.mode == StoredBorrowMode::ExclusiveAddress;
                                access.check(permission, exclusive, apart, b)?;
                                for previous in &shape.slots[..index] {
                                    fee(b, 1)?;
                                    if exclusive
                                        || previous.mode == StoredBorrowMode::ExclusiveAddress
                                    {
                                        ordered(
                                            b,
                                            values[result.0 as usize].slots.len(),
                                            previous.path.len(),
                                        )?;
                                        let other = values[result.0 as usize]
                                            .slots
                                            .get(&previous.path)
                                            .ok_or_else(|| {
                                                failure("constructor has no prior stored leaf")
                                            })?;
                                        for left in &permission.targets {
                                            for right in &other.targets {
                                                separate(left, right, apart, b)?;
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
                        fee(b, path.len().saturating_add(1))?;
                        let actual = Permission {
                            targets: BTreeSet::from([Target {
                                retained_port: None,
                                base: state
                                    .capture_views
                                    .get(local)
                                    .copied()
                                    .map_or(HoldRoot::Local(*local), HoldRoot::Capture),
                                stored: Vec::new(),
                                place: path.clone(),
                                heap_parents: Vec::new(),
                                heap_element: None,
                            }]),
                            loans: Loans::new(),
                        };
                        access.check(&actual, false, apart, b)?;
                    }
                    InstructionKind::TakeVariantField { local, path } if ordinary_error_places => {
                        fee(b, path.len().saturating_add(1))?;
                        let actual = Permission {
                            targets: BTreeSet::from([Target {
                                retained_port: None,
                                base: local_capture_base(&state, *local, b)?,
                                stored: Vec::new(),
                                place: path.clone(),
                                heap_parents: Vec::new(),
                                heap_element: None,
                            }]),
                            loans: Loans::new(),
                        };
                        access.check(&actual, true, apart, b)?;
                    }
                    InstructionKind::DropErrorUnion { local } if ordinary_error_places => {
                        fee(b, 1)?;
                        let actual = Permission {
                            targets: BTreeSet::from([Target {
                                retained_port: None,
                                base: local_capture_base(&state, *local, b)?,
                                stored: Vec::new(),
                                place: Vec::new(),
                                heap_parents: Vec::new(),
                                heap_element: None,
                            }]),
                            loans: Loans::new(),
                        };
                        access.check(&actual, true, apart, b)?;
                    }
                    InstructionKind::ReadPackageField { local, index }
                        if pure_copy::ordinary_package_read(f, i, b)? =>
                    {
                        fee(b, 2)?;
                        let actual = Permission {
                            targets: BTreeSet::from([Target {
                                retained_port: None,
                                base: local_capture_base(&state, *local, b)?,
                                stored: Vec::new(),
                                place: vec![PlaceProjection::Field(*index)],
                                heap_parents: Vec::new(),
                                heap_element: None,
                            }]),
                            loans: Loans::new(),
                        };
                        access.check(&actual, false, apart, b)?;
                    }
                    InstructionKind::TakePackageField { local, index }
                    | InstructionKind::DropPackageField { local, index }
                    | InstructionKind::StorePackageField { local, index, .. } => {
                        if matches!(i.kind, InstructionKind::TakePackageField { .. }) {
                            fee(b, state.vacant.len())?;
                            if state.vacant.iter().any(|target| {
                                target.base == HoldRoot::Local(*local)
                                    && target.place.first() == Some(&PlaceProjection::Field(*index))
                            }) {
                                return Err(failure("child Package move has a vacant stored slot"));
                            }
                        }
                        let actual = Permission {
                            targets: BTreeSet::from([Target {
                                retained_port: None,
                                base: local_capture_base(&state, *local, b)?,
                                stored: Vec::new(),
                                place: vec![PlaceProjection::Field(*index)],
                                heap_parents: Vec::new(),
                                heap_element: None,
                            }]),
                            loans: Loans::new(),
                        };
                        fee(b, 1)?;
                        access.check(&actual, true, apart, b)?;
                    }
                    InstructionKind::Load(local) => {
                        fee(b, state.vacant.len())?;
                        if state
                            .vacant
                            .iter()
                            .any(|target| target.base == HoldRoot::Local(*local))
                        {
                            return Err(failure("whole carrier copy has a vacant stored slot"));
                        }
                        if matches!(
                            f.locals[local.0 as usize].ty,
                            Type::Address { mutable: true, .. }
                        ) {
                            ordered(b, state.locals.len(), 0)?;
                            let source = state
                                .locals
                                .get(local)
                                .ok_or_else(|| failure("exclusive address local is unavailable"))?;
                            access.check(&source.pointer, true, apart, b)?;
                        }
                    }
                    InstructionKind::DropManaged { local }
                    | InstructionKind::DropPackage { local }
                    | InstructionKind::DropErrorUnion { local } => {
                        fee(b, 1)?;
                        let actual = Permission {
                            targets: BTreeSet::from([Target {
                                retained_port: None,
                                base: HoldRoot::Local(*local),
                                stored: vec![],
                                place: vec![],
                                heap_parents: vec![],
                                heap_element: None,
                            }]),
                            loans: Loans::new(),
                        };
                        access.check(&actual, true, apart, b)?;
                    }
                    InstructionKind::BorrowLocal { local }
                    | InstructionKind::BorrowLocalMut { local }
                    | InstructionKind::Store { local, .. }
                    | InstructionKind::StorageDead(local)
                    | InstructionKind::TakeLocal { local }
                    | InstructionKind::UnpackTuple { local, .. }
                    | InstructionKind::DropTuple { local }
                    | InstructionKind::DropClosure { local } => {
                        if matches!(i.kind, InstructionKind::UnpackTuple { .. })
                            && failure_unpack(f, i, b)?
                        {
                            let Type::BorrowedStorage { life, .. } = &f.locals[local.0 as usize].ty
                            else {
                                unreachable!()
                            };
                            let source = state
                                .locals
                                .get(local)
                                .ok_or_else(|| failure("input view unpack has no live source"))?;
                            fee(b, source.slots.len().saturating_add(life.slots.len()))?;
                            if source.slots.len() != life.slots.len() {
                                return Err(failure(
                                    "input view unpack omits or duplicates actual permissions",
                                ));
                            }
                            for slot in &life.slots {
                                let permission = source.slots.get(&slot.path).ok_or_else(|| {
                                    failure("input view unpack omits an original input")
                                })?;
                                if permission.targets.is_empty() {
                                    return Err(failure(
                                        "input view unpack has no original input target",
                                    ));
                                }
                                access.check(
                                    permission,
                                    slot.mode == StoredBorrowMode::ExclusiveAddress,
                                    apart,
                                    b,
                                )?;
                            }
                        }
                        if matches!(i.kind, InstructionKind::TakeLocal { .. }) {
                            fee(b, state.vacant.len())?;
                            if state
                                .vacant
                                .iter()
                                .any(|target| target.base == HoldRoot::Local(*local))
                            {
                                return Err(failure("whole carrier move has a vacant stored slot"));
                            }
                        }
                        let mut actual = Permission::default();
                        actual.targets.insert(Target {
                            retained_port: None,
                            base: local_capture_base(&state, *local, b)?,
                            stored: Vec::new(),
                            place: Vec::new(),
                            heap_parents: Vec::new(),
                            heap_element: None,
                        });
                        let write = !matches!(i.kind, InstructionKind::BorrowLocal { .. });
                        access.check(&actual, write, apart, b)?;
                    }
                    InstructionKind::ReadCapture { index }
                        if f.environment
                            .and_then(|id| module.closures.get(id.0 as usize))
                            .and_then(|d| d.captures.get(*index as usize))
                            .is_some_and(super::stored_callables::exclusive_capture) =>
                    {
                        let actual = state
                            .captures
                            .get(index)
                            .ok_or_else(|| failure("capture reborrow has no actual capability"))?;
                        access.check(
                            &actual.pointer,
                            matches!(
                                i.result.and_then(|v| f.values.get(v.0 as usize)),
                                Some(Type::Address { mutable: true, .. })
                            ),
                            apart,
                            b,
                        )?;
                    }
                    InstructionKind::BorrowCapture { index, mutable } => {
                        let actual = Permission {
                            targets: BTreeSet::from([Target {
                                retained_port: None,
                                base: HoldRoot::Capture(*index),
                                stored: vec![],
                                place: vec![],
                                heap_parents: Vec::new(),
                                heap_element: None,
                            }]),
                            loans: Loans::new(),
                        };
                        fee(b, 1)?;
                        access.check(&actual, *mutable, apart, b)?;
                    }
                    InstructionKind::ExtractIndex { aggregate, .. }
                        if array_index::candidate(f, i) =>
                    {
                        let input = values.get(aggregate.0 as usize).ok_or_else(|| {
                            failure("dynamic Array operand outside actual values")
                        })?;
                        array_index::check(f, i, input, &current, loans, apart, b)?;
                    }
                    InstructionKind::ReadAddress { address } => {
                        if let Type::Address { inner, .. } = value_type(f, *address)?
                            && shared_array_value(inner)
                            && let Type::Array { length, .. } = inner.as_ref()
                        {
                            array_snapshot::check(&state, pointer(*address)?, *length, b)?;
                        }
                        access.check(pointer(*address)?, false, apart, b)?;
                    }
                    InstructionKind::SliceLength { address }
                    | InstructionKind::ReborrowAddress {
                        address,
                        mutable: false,
                    } => access.check(pointer(*address)?, false, apart, b)?,
                    InstructionKind::AddressIndex { address, .. }
                    | InstructionKind::AddressSlice { address, .. }
                    | InstructionKind::AddressField { address, .. } => {
                        let write = i.result.is_some_and(|v| {
                            matches!(f.values[v.0 as usize], Type::Address { mutable: true, .. })
                        });
                        access.check(pointer(*address)?, write, apart, b)?;
                    }
                    InstructionKind::LoadStoredBorrow { storage, .. }
                        if failure_projection(f, i, b)?.is_some() =>
                    {
                        let Type::Address { inner, .. } = value_type(f, *storage)? else {
                            unreachable!()
                        };
                        let Type::BorrowedStorage { life, .. } = inner.as_ref() else {
                            unreachable!()
                        };
                        access.check(pointer(*storage)?, true, apart, b)?;
                        for slot in &life.slots {
                            require_filled(&state, pointer(*storage)?, &slot.path, b)?;
                            let actual = select(&state, pointer(*storage)?, &slot.path, b)?;
                            if actual.targets.is_empty() {
                                return Err(failure(
                                    "failure projection has no original input target",
                                ));
                            }
                            access.check(
                                &actual,
                                slot.mode == StoredBorrowMode::ExclusiveAddress,
                                apart,
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
                        if !b.stored_projected_slots {
                            access.check(pointer(*storage)?, exclusive, apart, b)?;
                        } else {
                            // The physical pointer slot is an inline field. A sibling
                            // mutable slot remains accessible while its child is live;
                            // the carrier's parent loans still accompany this access.
                            let mut slot = Permission {
                                targets: BTreeSet::new(),
                                loans: pointer(*storage)?.loans.clone(),
                            };
                            for root in &pointer(*storage)?.targets {
                                fee(b, target_width(root).saturating_add(path.len()))?;
                                let mut target = root.clone();
                                target.place.extend(path);
                                ordered(b, slot.targets.len(), target_width(&target))?;
                                slot.targets.insert(target);
                            }
                            access.check(&slot, exclusive, apart, b)?;
                        }
                        let actual = select(&state, pointer(*storage)?, path, b)?;
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
                        access.check(&actual, target_write, apart, b)?;
                    }
                    InstructionKind::StoreStoredBorrow {
                        storage,
                        path,
                        value,
                    } => {
                        let exclusive =
                            matches!(value_type(f, *value)?, Type::Address { mutable: true, .. });
                        let mut slot = Permission {
                            targets: BTreeSet::new(),
                            loans: pointer(*storage)?.loans.clone(),
                        };
                        for root in &pointer(*storage)?.targets {
                            fee(b, target_width(root).saturating_add(path.len()))?;
                            let mut target = root.clone();
                            target.place.extend(path);
                            ordered(b, slot.targets.len(), target_width(&target))?;
                            slot.targets.insert(target);
                        }
                        access.check(&slot, true, apart, b)?;
                        let previous = select(&state, pointer(*storage)?, path, b)?;
                        if previous.targets.is_empty() {
                            let position = slot_position(pointer(*storage)?, path, b)?;
                            if target_width(&position) != 0 {
                                ordered(b, state.vacant.len(), target_width(&position))?;
                            }
                            if !state.vacant.contains(&position) {
                                return Err(failure(
                                    "replacement has neither a live slot nor an extracted hole",
                                ));
                            }
                        } else {
                            access.check(&previous, exclusive, apart, b)?;
                        }
                        access.check(pointer(*value)?, exclusive, apart, b)?;
                    }
                    InstructionKind::WriteAddress { address, .. }
                    | InstructionKind::ReborrowAddress {
                        address,
                        mutable: true,
                    } => access.check(pointer(*address)?, true, apart, b)?,
                    InstructionKind::CallIndirect { arguments, .. }
                    | InstructionKind::CallClosure { arguments, .. }
                    | InstructionKind::CallClosureBorrowed { arguments, .. }
                    | InstructionKind::CallClosureShared { arguments, .. }
                    | InstructionKind::CallClosureMut { arguments, .. }
                    | InstructionKind::CallClosureOnce { arguments, .. } => {
                        let mut all: Vec<(Permission, bool)> = Vec::new();
                        match &i.kind {
                            InstructionKind::CallClosureBorrowed { callee, .. } => {
                                let mut p = Permission::default();
                                union(&mut p, pointer(*callee)?, b)?;
                                fee(b, 1)?;
                                all.push((
                                    p,
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
                                fee(b, 2)?;
                                all.push((
                                    Permission {
                                        targets: BTreeSet::from([Target {
                                            retained_port: None,
                                            base: HoldRoot::Local(*callee),
                                            stored: vec![],
                                            place: vec![],
                                            heap_parents: Vec::new(),
                                            heap_element: None,
                                        }]),
                                        loans: Loans::new(),
                                    },
                                    matches!(i.kind, InstructionKind::CallClosureMut { .. }),
                                ));
                            }
                            _ => {}
                        }

                        let receiver_held = match &i.kind {
                            InstructionKind::CallClosureShared { callee, .. }
                            | InstructionKind::CallClosureMut { callee, .. }
                                if environment_value(
                                    module,
                                    &f.locals[callee.0 as usize].ty,
                                    b,
                                )? =>
                            {
                                ordered(b, state.locals.len(), 0)?;
                                state
                                    .locals
                                    .get(callee)
                                    .map(|v| environment_memory::held(v, &state, b))
                                    .transpose()?
                            }
                            InstructionKind::CallClosure { callee, .. }
                            | InstructionKind::CallClosureBorrowed { callee, .. }
                            | InstructionKind::CallClosureOnce { callee, .. } => Some(
                                environment_memory::held(&values[callee.0 as usize], &state, b)?,
                            ),
                            _ => None,
                        };
                        if let Some(held) = receiver_held
                            && !held.targets.is_empty()
                        {
                            let mut p = Permission::default();
                            union(&mut p, &held, b)?;
                            fee(b, 1)?;
                            all.push((p, false));
                        }
                        for argument in arguments {
                            let actual = &values[argument.0 as usize];
                            if let Type::Address { mutable, .. } = value_type(f, *argument)? {
                                let mut p = Permission::default();
                                union(&mut p, &actual.pointer, b)?;
                                fee(b, 1)?;
                                all.push((p, *mutable));
                            }
                            for leaf in actual.slots.values() {
                                let mut p = Permission::default();
                                union(&mut p, leaf, b)?;
                                fee(b, 1)?;
                                all.push((p, false));
                            }
                            if let Some((_, shape)) = pointed_carrier(value_type(f, *argument)?) {
                                for slot in &shape.slots {
                                    let p = select(&state, &actual.pointer, &slot.path, b)?;
                                    fee(b, 1)?;
                                    all.push((p, false));
                                }
                            }
                        }
                        for (index, (permission, write)) in all.iter().enumerate() {
                            access.check(permission, *write, apart, b)?;
                            for (other, other_write) in &all[..index] {
                                if *write || *other_write {
                                    for a in &permission.targets {
                                        for z in &other.targets {
                                            separate(a, z, apart, b)?;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    InstructionKind::Call {
                        function,
                        arguments,
                    } => {
                        let callee = &module.functions[function.0 as usize];
                        if !summaries[function.0 as usize].head_accesses.is_empty() {
                            for (target, write) in &summaries[function.0 as usize].head_accesses {
                                let actual = substitute(target, arguments, values, &state, b)?;
                                if actual.targets.is_empty() {
                                    return Err(failure(
                                        "callee head effect has no actual caller manager",
                                    ));
                                }
                                access.check(&actual, *write, apart, b)?;
                                record_head_access(head_accesses, &actual, *write, b)?;
                            }
                        }
                        for (a, (argument, parameter)) in
                            arguments.iter().zip(&callee.parameters).enumerate()
                        {
                            let actual = &values[argument.0 as usize];
                            let ty = &callee.locals[parameter.0 as usize].ty;
                            if let Some((_, shape)) = pointed_carrier(ty) {
                                for slot in &shape.slots {
                                    require_filled(&state, &actual.pointer, &slot.path, b)?;
                                }
                            }
                            if let Type::Address { mutable, .. } = ty {
                                access.check(&actual.pointer, *mutable, apart, b)?;
                                if *mutable && let Some((_, shape)) = pointed_carrier(ty) {
                                    for slot in &shape.slots {
                                        let held = select(&state, &actual.pointer, &slot.path, b)?;
                                        access.check(
                                            &held,
                                            slot.mode == StoredBorrowMode::ExclusiveAddress,
                                            apart,
                                            b,
                                        )?;
                                    }
                                }
                            }
                            for leaf in actual.slots.values() {
                                access.check(leaf, false, apart, b)?;
                            }
                            let held = environment_memory::held(actual, &state, b)?;
                            if !held.targets.is_empty() {
                                access.check(&held, false, apart, b)?;
                            }
                            for other in &arguments[..a] {
                                let right = &values[other.0 as usize];
                                let right_held = environment_memory::held(right, &state, b)?;
                                let write = matches!(
                                    value_type(f, *argument)?,
                                    Type::Address { mutable: true, .. }
                                ) || matches!(
                                    value_type(f, *other)?,
                                    Type::Address { mutable: true, .. }
                                );
                                if write {
                                    for x in &actual.pointer.targets {
                                        for y in &right.pointer.targets {
                                            separate(x, y, apart, b)?;
                                        }
                                    }
                                }
                                if matches!(
                                    value_type(f, *argument)?,
                                    Type::Address { mutable: true, .. }
                                ) {
                                    for leaf in right.slots.values().chain(
                                        (!right_held.targets.is_empty())
                                            .then_some(right_held.as_ref()),
                                    ) {
                                        for x in &actual.pointer.targets {
                                            for y in &leaf.targets {
                                                separate(x, y, apart, b)?;
                                            }
                                        }
                                    }
                                }
                                if matches!(
                                    value_type(f, *other)?,
                                    Type::Address { mutable: true, .. }
                                ) {
                                    for leaf in actual
                                        .slots
                                        .values()
                                        .chain((!held.targets.is_empty()).then_some(held.as_ref()))
                                    {
                                        for x in &leaf.targets {
                                            for y in &right.pointer.targets {
                                                separate(x, y, apart, b)?;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        for (left, right) in &summaries[function.0 as usize].apart {
                            let weak = |source: &Target| match source.base {
                                HoldRoot::Parameter(position) => callee
                                    .parameters
                                    .get(position as usize)
                                    .is_some_and(|local| {
                                        byte_queries::description(
                                            &callee.locals[local.0 as usize].ty,
                                        )
                                    }),
                                _ => false,
                            };
                            if b.byte_queries && (weak(left) || weak(right)) {
                                let flow = byte_flow.as_ref().ok_or_else(|| {
                                    failure("Byte separation has no covered origin flow")
                                })?;
                                let left = flow.substitute(
                                    left,
                                    callee,
                                    arguments,
                                    values,
                                    &state,
                                    byte_point.as_ref().unwrap(),
                                    b,
                                )?;
                                let right = flow.substitute(
                                    right,
                                    callee,
                                    arguments,
                                    values,
                                    &state,
                                    byte_point.as_ref().unwrap(),
                                    b,
                                )?;
                                for a in &left {
                                    for z in &right {
                                        for x in &a.targets {
                                            for y in &z.targets {
                                                separate(x, y, apart, b)?;
                                            }
                                        }
                                    }
                                }
                                continue;
                            }
                            let left = substitute(left, arguments, values, &state, b)?;
                            let right = substitute(right, arguments, values, &state, b)?;
                            if left.targets.is_empty() || right.targets.is_empty() {
                                return Err(failure(
                                    "callee separation lacks complete actual slot facts",
                                ));
                            }
                            for x in &left.targets {
                                for y in &right.targets {
                                    separate(x, y, apart, b)?;
                                }
                            }
                        }
                    }
                    _ => {}
                }
                Ok(())
            })();
            checked.map_err(|mut errors| {
                for error in &mut errors {
                    error
                        .notes
                        .push(format!("actual stored access in {}: {:?}", f.name, i.kind));
                }
                errors
            })?;
            transfer(
                module, f, i, &mut state, values, summaries, slot_ids, loans, b,
            )?;
            if let Some(flow) = &byte_flow {
                flow.advance(
                    module,
                    f,
                    i,
                    values,
                    summaries,
                    byte_point.as_mut().unwrap(),
                    b,
                )?;
            }
        }
    }
    Ok(())
}

/// Path-aware return solving is separate from the legacy parameter-only
/// reference summaries. No declared life bound fabricates a nonempty result.
pub(super) fn derive(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    b: &mut Budget,
) -> CompileResult<Vec<Facts>> {
    derive_cfg(module, graphs, None, b)
}
fn derive_cfg(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    inactive: Option<&[BTreeSet<ValueId>]>,
    b: &mut Budget,
) -> CompileResult<Vec<Facts>> {
    let pending_byte_edges = byte_queries::inventory(module, graphs, b)?;
    let mut observed_none = false;
    fee(b, module.functions.len())?;
    let mut summaries = vec![Facts::default(); module.functions.len()];
    loop {
        let mut changed = false;
        for (index, f) in module.functions.iter().enumerate() {
            if !b.composition_stored.is_empty() && !b.composition_stored[index] {
                continue;
            }
            let (preds, reachable) = &graphs[index];
            // This solve builds a new function-local arena. Record mutable
            // loans at its existing paid creation/binding sites, including
            // constructor sites whose loans are not currently live.
            b.stored_exclusive_loans = false;
            fee(
                b,
                f.values
                    .len()
                    .saturating_mul(6)
                    .saturating_add(f.blocks.len()),
            )?;
            let mut loan_count = f.values.len();
            fee(b, f.values.len())?;
            for ty in &f.values {
                if let Some((_, shape)) = carrier(ty) {
                    loan_count = loan_count
                        .checked_add(shape.slots.len())
                        .ok_or_else(|| failure("stored loan arena overflow"))?;
                }
            }
            fee(b, loan_count.saturating_mul(4))?;
            let mut loans = vec![Loan::default(); loan_count];
            let entry = seed(module, f, &mut loans, b)?;
            let slot_ids = slot_loan_ids(module, f, &mut loans, b)?;
            let mut values = vec![Value::default(); f.values.len()];
            let mut outputs = vec![State::default(); f.blocks.len()];
            // A reached-but-not-yet-solved predecessor is lattice bottom, not
            // an executed move from an absent Task. The new failure carrier
            // composes loops in both arms, so defer only genuinely unseeded
            // blocks. Final access/resource passes still require live inputs.
            let mut seeded = if b.task_failure_carriers {
                fee(b, f.blocks.len())?;
                vec![false; f.blocks.len()]
            } else {
                Vec::new()
            };
            loop {
                let mut local_changed = false;
                for (bi, block) in f.blocks.iter().enumerate() {
                    fee(b, 1)?;
                    if !reachable[bi] {
                        continue;
                    }
                    if b.task_failure_carriers && bi != f.entry.0 as usize {
                        fee(b, preds[bi].len().saturating_add(1))?;
                        if !preds[bi].iter().any(|p| reachable[*p] && seeded[*p]) {
                            continue;
                        }
                    }
                    let mut state = if bi == f.entry.0 as usize {
                        state_copy(&entry, b)?
                    } else {
                        State::default()
                    };
                    for pred in &preds[bi] {
                        fee(b, 1)?;
                        if reachable[*pred] {
                            state_merge(&mut state, &outputs[*pred], b)?;
                        }
                    }
                    for instruction in &block.instructions {
                        let out = transfer(
                            module,
                            f,
                            instruction,
                            &mut state,
                            &values,
                            &summaries,
                            &slot_ids,
                            &mut loans,
                            b,
                        )?;
                        if let Some(id) = instruction.result {
                            local_changed |= merge(&mut values[id.0 as usize], &out, b)?;
                        }
                    }
                    state_work(&state, b)?;
                    state_work(&outputs[bi], b)?;
                    if state != outputs[bi] {
                        outputs[bi] = state;
                        local_changed = true;
                    }
                    if b.task_failure_carriers && !seeded[bi] {
                        seeded[bi] = true;
                        local_changed = true;
                    }
                }
                if !local_changed {
                    break;
                }
            }
            let package_return_calls = if b.qualified_package_returns {
                package_return_candidates(module, f, reachable, &values, &summaries, b)?
            } else {
                BTreeMap::new()
            };
            let mut actual = Facts {
                values: Vec::with_capacity(values.len()),
                pointer_values: Vec::new(),
                pointer_parents: Vec::new(),
                returned: BTreeMap::new(),
                returned_environment: BTreeMap::new(),
                returned_task_ports: BTreeMap::new(),
                returned_dyn_ports: BTreeMap::new(),
                declared_dyn_return_upper: BTreeMap::new(),
                dyn_return_complete: true,
                package_return_calls,
                byte_reads: Vec::new(),
                byte_covered: false,
                external_after: BTreeMap::new(),
                external_stable: BTreeSet::new(),
                capture_after: BTreeMap::new(),
                capture_stable: BTreeSet::new(),
                receiver_return: f.environment.is_some()
                    && f.storage_life.as_ref().is_some_and(|life| {
                        super::signature_life::outer(life, StorageBase::Receiver).is_some_and(
                            |receiver| {
                                super::signature_life::outer(life, StorageBase::Result)
                                    == Some(receiver)
                            },
                        )
                    }),
                returned_head_inputs: BTreeSet::new(),
                capture_taken: BTreeSet::new(),
                apart: BTreeSet::new(),
                head_accesses: BTreeMap::new(),
                complete: false,
                returned_enum_variants: enum_address::Variants::default(),
                enum_values: BTreeMap::new(),
                returned_variants: option_address::Variants::default(),
                none_values: BTreeSet::new(),
            };
            for (id, value) in values.iter().enumerate() {
                if value.enum_variants.present() {
                    value.enum_variants.work(b)?;
                    actual
                        .enum_values
                        .insert(ValueId(id as u32), value.enum_variants.clone());
                }
                if value.option_variants.none() {
                    observed_none = true;
                    fee(b, actual.none_values.len().saturating_add(1))?;
                    actual.none_values.insert(ValueId(id as u32));
                }
                if b.task_sources() {
                    fee(b, value.pointer.targets.len().saturating_add(1))?;
                    let roots = value
                        .pointer
                        .targets
                        .iter()
                        .map(|t| t.base)
                        .collect::<BTreeSet<_>>();
                    actual.pointer_values.push(roots.into_iter().collect());
                    if b.masked_stored_tasks() {
                        actual.pointer_parents.push(pointer_anchors(
                            f,
                            ValueId(id as u32),
                            &value.pointer,
                            &loans,
                            b,
                        )?);
                    }
                }
                let mut root = roots(value, b)?;
                if b.receiver_borrows {
                    let mut dependencies = Loans::new();
                    active_loans_union(&mut dependencies, &value.pointer, b)?;
                    active_loans_union(&mut dependencies, &value.held, b)?;
                    for permission in value.slots.values() {
                        active_loans_union(&mut dependencies, permission, b)?;
                    }
                    for id in ancestry(&dependencies, &loans, b)? {
                        if loans[id].receiver_head {
                            for target in &loans[id].targets {
                                ordered(b, root.len(), target_width(target))?;
                                fee(b, target_width(target))?;
                                root.insert(target.clone());
                            }
                        }
                    }
                }
                if b.store_option_results
                    && b.shared_byte_borrow
                    && matches!(&f.values[id], Type::Desc(inner) if inner.as_ref() == &Type::ByteBuffer)
                {
                    for target in &value.store_description.targets {
                        ordered(b, root.len(), target_width(target))?;
                        root.insert(target.clone());
                    }
                }
                fee(b, root.len())?;
                actual.values.push(root.into_iter().collect());
            }
            if b.receiver_borrows || b.mutable_capture_contexts {
                for (base, rows) in &entry.external {
                    let HoldRoot::Parameter(parameter) = base else {
                        continue;
                    };
                    for path in rows.keys() {
                        ordered(b, actual.external_stable.len(), path.len())?;
                        fee(b, path.len().saturating_add(1))?;
                        actual.external_stable.insert((*parameter, path.clone()));
                    }
                }
            }
            if let Some(d) = f
                .environment
                .and_then(|id| module.closures.get(id.0 as usize))
                && capture_after::required(module, d, b)?
            {
                for (path, _) in capture_after::outputs(module, d, b)? {
                    if capture_after::row(d, &entry, &path, b)?.is_some() {
                        ordered(b, actual.capture_stable.len(), path.len())?;
                        actual.capture_stable.insert(path);
                    }
                }
            }
            for (bi, (block, live)) in f.blocks.iter().zip(reachable).enumerate() {
                fee(b, 1)?;
                if *live
                    && matches!(block.terminator, Terminator::Return(_))
                    && let Some(d) = f
                        .environment
                        .and_then(|id| module.closures.get(id.0 as usize))
                    && capture_after::required(module, d, b)?
                {
                    let mut changed_rows = Vec::new();
                    for path in &actual.capture_stable {
                        let initial = capture_after::row(d, &entry, path, b)?;
                        let terminal = capture_after::row(d, &outputs[bi], path, b)?;
                        if let Some(p) = initial {
                            permission_clone_work(p, b)?;
                        }
                        if let Some(p) = terminal {
                            permission_clone_work(p, b)?;
                        }
                        if initial != terminal {
                            fee(b, path.len())?;
                            changed_rows.push(path.clone());
                        }
                    }
                    for path in changed_rows {
                        ordered(b, actual.capture_stable.len(), path.len())?;
                        actual.capture_stable.remove(&path);
                    }
                    capture_after::summarize(module, f, &outputs[bi], &mut actual, b)?;
                }
                if b.stored_slot_updates
                    && *live
                    && matches!(block.terminator, Terminator::Return(_))
                {
                    let mut changed_rows = Vec::new();
                    for (parameter, path) in &actual.external_stable {
                        fee(b, path.len().saturating_add(1))?;
                        let base = HoldRoot::Parameter(*parameter);
                        let initial = entry.external.get(&base).and_then(|rows| rows.get(path));
                        let terminal = outputs[bi]
                            .external
                            .get(&base)
                            .and_then(|rows| rows.get(path));
                        if let Some(value) = initial {
                            permission_clone_work(value, b)?;
                        }
                        if let Some(value) = terminal {
                            permission_clone_work(value, b)?;
                        }
                        if initial != terminal {
                            changed_rows.push((*parameter, path.clone()));
                        }
                    }
                    for row in changed_rows {
                        ordered(b, actual.external_stable.len(), row.1.len())?;
                        actual.external_stable.remove(&row);
                    }
                    for (base, slots) in &outputs[bi].external {
                        let HoldRoot::Parameter(index) = base else {
                            continue;
                        };
                        for (path, permission) in slots {
                            ordered(b, actual.external_after.len(), path.len())?;
                            let row = actual
                                .external_after
                                .entry((*index, path.clone()))
                                .or_default();
                            for target in &permission.targets {
                                fee(
                                    b,
                                    target_width(target)
                                        .saturating_mul(row.len().saturating_add(1))
                                        .saturating_add(1),
                                )?;
                                if !matches!(target.base, HoldRoot::Parameter(_) | HoldRoot::Static)
                                {
                                    return Err(failure(
                                        "stored external update retains an activation-local target",
                                    ));
                                }
                                if let HoldRoot::Parameter(source) = target.base
                                    && environment_memory::binding(module, f, *index, path, b)?
                                        != environment_memory::binding(
                                            module,
                                            f,
                                            source,
                                            &target.stored,
                                            b,
                                        )?
                                {
                                    return Err(failure(
                                        "stored external update exceeds its declared lifetime relation",
                                    ));
                                }
                                if !row.contains(target) {
                                    row.push(target.clone());
                                }
                            }
                        }
                    }
                }
                if *live && let Terminator::Return(Some(id)) = block.terminator {
                    let value = &values[id.0 as usize];
                    if let Some(marker) = entry.receiver_loan
                        && f.environment
                            .and_then(|id| module.closures.get(id.0 as usize))
                            .is_some_and(super::stored_callables::fixed_result)
                        && ancestry(&value.pointer.loans, &loans, b)?.contains(&marker)
                    {
                        return Err(failure(
                            "a Fixed result cannot erase its actual receiver-head dependency",
                        ));
                    }
                    if actual.receiver_return {
                        let marker = entry.receiver_loan.ok_or_else(|| {
                            failure("receiver-bounded result has no actual entry head")
                        })?;
                        let parents = ancestry(&value.pointer.loans, &loans, b)?;
                        actual.receiver_return &= parents.contains(&marker);
                    }
                    if b.receiver_borrows && matches!(f.return_type, Type::Address { .. }) {
                        let parents = ancestry(&value.pointer.loans, &loans, b)?;
                        for (position, local) in f.parameters.iter().enumerate() {
                            if matches!(&f.locals[local.0 as usize].ty,Type::Address{inner,..}if matches!(inner.as_ref(),Type::Closure(_)))
                            {
                                let head = &entry
                                    .locals
                                    .get(local)
                                    .ok_or_else(|| {
                                        failure(
                                            "returned callable dependency has no actual input head",
                                        )
                                    })?
                                    .pointer;
                                fee(b, head.loans.len())?;
                                if head.loans.iter().any(|loan| parents.contains(loan)) {
                                    ordered(b, actual.returned_head_inputs.len(), 0)?;
                                    actual.returned_head_inputs.insert(position as u32);
                                }
                            }
                        }
                    }
                    actual
                        .returned_enum_variants
                        .join(&value.enum_variants, b)?;
                    actual.returned_variants.join(&value.option_variants, b)?;
                    let mut add = |path: &Path, permission: &Permission| -> CompileResult<()> {
                        ordered(b, actual.returned.len(), path.len())?;
                        fee(b, path.len())?;
                        let row = actual.returned.entry(path.clone()).or_default();
                        for target in &permission.targets {
                            fee(
                                b,
                                target_width(target)
                                    .saturating_mul(row.len().saturating_add(1))
                                    .saturating_add(1),
                            )?;
                            if !matches!(target.base, HoldRoot::Parameter(_) | HoldRoot::Static)
                                && !(matches!(target.base, HoldRoot::Capture(_))
                                    && f.environment
                                        .and_then(|id| module.closures.get(id.0 as usize))
                                        .is_some_and(|d| {
                                            super::stored_callables::lending(d)
                                                || super::stored_callables::transferred(d)
                                        }))
                            {
                                return Err(failure(
                                    "stored return would retain an activation-local target",
                                ));
                            }
                            if !row.contains(target) {
                                row.push(target.clone());
                            }
                        }
                        Ok(())
                    };
                    if matches!(f.return_type, Type::Address { .. }) {
                        add(&Vec::new(), &value.pointer)?;
                    }
                    for (path, permission) in &value.slots {
                        add(path, permission)?;
                    }
                    if matches!(f.return_type, Type::Closure(_)) {
                        for (path, permission) in &value.environment {
                            ordered(b, actual.returned_environment.len(), path.len())?;
                            fee(b, path.len().saturating_add(permission.targets.len()))?;
                            let row = actual.returned_environment.entry(path.clone()).or_default();
                            if permission.targets.is_empty() {
                                return Err(failure(
                                    "returned capture has no actual external target",
                                ));
                            }
                            for target in &permission.targets {
                                if !matches!(target.base, HoldRoot::Parameter(_) | HoldRoot::Static)
                                {
                                    return Err(failure(
                                        "returned environment retains activation-local storage",
                                    ));
                                }
                                fee(
                                    b,
                                    target_width(target)
                                        .saturating_mul(row.len().saturating_add(1)),
                                )?;
                                if !row.contains(target) {
                                    row.push(target.clone());
                                }
                            }
                        }
                    }
                    task_address::summarize(f, value, &mut actual, b)?;
                    dyn_return::summarize(module, f, value, &mut actual, b)?;
                }
            }
            if let Some(d) = f
                .environment
                .and_then(|id| module.closures.get(id.0 as usize))
                && super::stored_callables::transferred(d)
            {
                for (capture, ty) in d.captures.iter().enumerate() {
                    if !super::stored_callables::exclusive_capture(ty) {
                        continue;
                    }
                    let mut seen = false;
                    let mut taken = true;
                    for (index, block) in f.blocks.iter().enumerate() {
                        if reachable[index] && matches!(block.terminator, Terminator::Return(_)) {
                            seen = true;
                            ordered(b, outputs[index].captures.len(), 0)?;
                            taken &= !outputs[index].captures.contains_key(&(capture as u32));
                        }
                    }
                    if seen && taken {
                        ordered(b, actual.capture_taken.len(), 0)?;
                        actual.capture_taken.insert(capture as u32);
                    }
                }
            }
            let mut grounded = true;
            for (bi, block) in f.blocks.iter().enumerate() {
                if reachable[bi] && matches!(block.terminator, Terminator::Return(_)) {
                    fee(b, outputs[bi].vacant.len())?;
                    if outputs[bi].vacant.iter().any(|target| {
                        matches!(target.base, HoldRoot::Parameter(_) | HoldRoot::Capture(_))
                    }) {
                        grounded = false;
                    }
                }
            }
            for (id, (ty, value)) in f.values.iter().zip(&values).enumerate() {
                fee(b, 1)?;
                if let Some(rows) = inactive {
                    ordered(b, rows[index].len(), 0)?;
                    if rows[index].contains(&ValueId(id as u32)) {
                        continue;
                    }
                }
                if value.creation_complete == Some(false) {
                    grounded = false;
                }
                let environment = match ty {
                    Type::Closure(id) => Some(*id),
                    Type::Address { inner, .. } => match inner.as_ref() {
                        Type::Closure(id) => Some(*id),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(id) = environment
                    && capture_after::has_context(module, &module.closures[id.0 as usize], b)?
                {
                    for (path, _) in
                        capture_after::leaves(module, &module.closures[id.0 as usize], b)?
                    {
                        ordered(b, value.environment.len(), path.len())?;
                        grounded &= value
                            .environment
                            .get(&path)
                            .is_some_and(|p| !p.targets.is_empty());
                    }
                }
                if matches!(ty, Type::Address { .. }) && value.pointer.targets.is_empty() {
                    grounded = false;
                }
                if b.stored_enum_addresses
                    && matches!(ty, Type::ErrorUnion { .. })
                    && enum_address::ordinary(module, ty, b)?
                {
                    if !value.enum_variants.grounded(ty, b)? {
                        grounded = false;
                    }
                    for (path, _) in enum_address::slots(ty, b)? {
                        if value.enum_variants.slot_active(&path)
                            && value.slots.get(&path).is_none_or(|p| p.targets.is_empty())
                        {
                            grounded = false;
                        }
                    }
                }
                if shared_array_shape(ty, b)? {
                    let Type::Array { length, .. } = ty else {
                        unreachable!()
                    };
                    fee(b, *length as usize)?;
                    for index in 0..*length {
                        ordered(b, value.slots.len(), 1)?;
                        if value
                            .slots
                            .get(&vec![PlaceProjection::Field(index)])
                            .is_none_or(|p| p.targets.is_empty())
                        {
                            grounded = false;
                        }
                    }
                }
                if b.stored_compound_values && matches!(ty, Type::Tuple(_)) && !ty.is_copy_value() {
                    let mut rows = Vec::new();
                    compound_slot_rows(ty, &mut Vec::new(), &mut rows, b)?;
                    for row in rows {
                        ordered(b, value.slots.len(), row.path.len())?;
                        if value
                            .slots
                            .get(&row.path)
                            .is_none_or(|permission| permission.targets.is_empty())
                        {
                            grounded = false;
                        }
                    }
                }
                if let Some((_, life)) = carrier(ty) {
                    if enum_address::ordinary(module, ty, b)? {
                        if !value.enum_variants.grounded(ty, b)? {
                            grounded = false;
                        }
                    } else if option_address::candidate(ty) {
                        fee(b, 96)?;
                        if !option_address::signature(ty) {
                            return Err(failure("Option activity has no exact shape"));
                        }
                        let mask = value.option_variants.mask();
                        if mask == 0 {
                            grounded = false;
                        }
                        if mask == 1 {
                            continue;
                        }
                    }
                    fee(b, life.slots.len())?;
                    for slot in &life.slots {
                        if value.enum_variants.present()
                            && !value.enum_variants.slot_active(&slot.path)
                        {
                            continue;
                        }
                        ordered(b, value.slots.len(), slot.path.len())?;
                        if value
                            .slots
                            .get(&slot.path)
                            .is_none_or(|p| p.targets.is_empty())
                        {
                            grounded = false;
                        }
                    }
                }
            }
            for (id, loan) in loans.iter().enumerate() {
                let parents = ancestry(&loan.parents, &loans, b)?;
                ordered(b, parents.len(), 0)?;
                if parents.contains(&id) {
                    return Err(failure(
                        "cyclic stored/target loan ancestry has no actual finite authority",
                    ));
                }
            }
            if b.stored_slot_updates {
                for block in &f.blocks {
                    for instruction in &block.instructions {
                        let InstructionKind::Call { function, .. } = &instruction.kind else {
                            continue;
                        };
                        let callee = module
                            .functions
                            .get(function.0 as usize)
                            .ok_or_else(|| failure("stored call target is absent"))?;
                        let summary = summaries
                            .get(function.0 as usize)
                            .ok_or_else(|| failure("stored call summary is absent"))?;
                        if !summary.complete {
                            grounded = false;
                        }
                        for (position, local) in callee.parameters.iter().enumerate() {
                            if let Type::Address {
                                mutable: true,
                                inner,
                            } = &callee.locals[local.0 as usize].ty
                                && let Some((_, shape)) = carrier(inner)
                            {
                                for slot in &shape.slots {
                                    fee(b, slot.path.len().saturating_add(1))?;
                                    if !summary
                                        .external_after
                                        .contains_key(&(position as u32, slot.path.clone()))
                                    {
                                        grounded = false;
                                    }
                                }
                            } else if let Type::Address {
                                mutable: true,
                                inner,
                            } = &callee.locals[local.0 as usize].ty
                                && let Type::Closure(id) = inner.as_ref()
                            {
                                let d = module.closures.get(id.0 as usize).ok_or_else(|| {
                                    failure("external callable definition absent")
                                })?;
                                for (path, _) in capture_after::leaves(module, d, b)? {
                                    ordered(b, summary.external_after.len(), path.len())?;
                                    grounded &= summary
                                        .external_after
                                        .get(&(position as u32, path))
                                        .is_some_and(|row| !row.is_empty());
                                }
                            } else if let Type::Closure(id) = &callee.locals[local.0 as usize].ty
                                && capture_after::has_context(
                                    module,
                                    &module.closures[id.0 as usize],
                                    b,
                                )?
                            {
                                for (path, _) in capture_after::leaves(
                                    module,
                                    &module.closures[id.0 as usize],
                                    b,
                                )? {
                                    ordered(b, summary.external_after.len(), path.len())?;
                                    grounded &= summary
                                        .external_after
                                        .get(&(position as u32, path))
                                        .is_some_and(|row| !row.is_empty());
                                }
                            }
                        }
                    }
                }
            }
            for block in &f.blocks {
                for instruction in &block.instructions {
                    if let Some((d, _)) =
                        super::stored_callables::called(module, f, &instruction.kind)
                        && (d.storage_life.is_some()
                            || b.returned_borrowed_environments.contains(&d.id))
                        && capture_after::required(module, d, b)?
                    {
                        let summary = &summaries[d.function.0 as usize];
                        grounded &= summary.complete;
                        for (path, _) in capture_after::outputs(module, d, b)? {
                            grounded &= summary
                                .capture_after
                                .get(&path)
                                .is_some_and(|row| !row.is_empty());
                        }
                    }
                }
            }
            grounded &= actual.dyn_return_complete;
            actual.complete = grounded;
            if grounded {
                actual.byte_covered = b.byte_queries;
                windows(
                    module,
                    f,
                    preds,
                    reachable,
                    &entry,
                    &outputs,
                    &values,
                    &slot_ids,
                    &mut loans,
                    &summaries,
                    &mut actual.apart,
                    &mut actual.byte_reads,
                    &mut actual.head_accesses,
                    b,
                )?;
            }
            facts_work(&actual, b)?;
            facts_work(&summaries[index], b)?;
            if actual != summaries[index] {
                summaries[index] = actual;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    if inactive.is_none()
        && let Some(proof) = enum_address::refine(module, graphs, &summaries, b)?
    {
        return derive_cfg(module, &proof.graphs, Some(&proof.inactive), b);
    }
    // Freeze only the original, fully converged graph facts. A second solve
    // receives no authority to trim another edge from its narrowed summaries.
    if !b.composition_stored.is_empty() && observed_none && !b.masked_stored_tasks() {
        return Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "mixed stored/ordinary Option-address refinement is outside the scalar seam",
            None,
        )]);
    }
    if inactive.is_none()
        && observed_none
        && let Some(proof) = option_address::refine(module, graphs, &summaries, b)?
    {
        return derive_cfg(module, &proof.graphs, Some(&proof.inactive), b);
    }
    for (index, (f, facts)) in module.functions.iter().zip(&summaries).enumerate() {
        if !b.composition_stored.is_empty() && !b.composition_stored[index] {
            continue;
        }
        if b.byte_queries && b.byte_query_functions[index] && !facts.byte_covered {
            return Err(failure(
                "Byte query function lacks complete actual read coverage",
            ));
        }
        if !facts.complete {
            return Err(failure(
                "stored/address producer has incomplete actual target facts",
            ));
        }
        returned_upper::check(module, f, facts, b)?;
        enum_address::projections(f, &graphs[index], facts, b)?;
        returns(module, f, facts, b)?;
        for (id, (ty, roots)) in f.values.iter().zip(&facts.values).enumerate() {
            fee(b, 1)?;
            if let Some(rows) = inactive {
                ordered(b, rows[index].len(), 0)?;
                if rows[index].contains(&ValueId(id as u32)) {
                    continue;
                }
            }
            let needs_target = matches!(ty, Type::Address { .. })
                || matches!(ty,Type::BorrowedStorage{life,..}if !life.slots.is_empty());
            let empty_enum = facts.enum_values.get(&ValueId(id as u32)).is_some_and(|tags| !matches!(ty,Type::BorrowedStorage{life,..} if life.slots.iter().any(|s| tags.slot_active(&s.path))));
            if needs_target
                && roots.is_empty()
                && !empty_enum
                && !facts.none_values.contains(&ValueId(id as u32))
            {
                return Err(failure(
                    "stored/address producer has no independently grounded actual origin",
                ));
            }
        }
    }
    byte_queries::certify_edges(module, &pending_byte_edges, &summaries, b)?;
    Ok(summaries)
}

pub(super) fn infer(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    b: &mut Budget,
) -> CompileResult<Vec<Vec<ValueHold>>> {
    byte_queries::authenticate(module, b)?;
    dyn_address::capture_directories(module, b)?;
    // Raw paid inventories classify new owning holders and Fixed captures.
    // Preserve the established Fixed profile without rescanning either table.
    let owned_byte_profile = !b.task_sources()
        && !b.masked_stored_tasks()
        && b.composition_stored.is_empty()
        && (!b.stored_packages || b.stored_fixed_owner)
        && owned_byte_views::profile(module, b)?;
    b.stored_owned_fixed = if !b.stored_compound_values
        && !owned_byte_profile
        && !b.task_sources()
        && !b.masked_stored_tasks()
        && b.composition_stored.is_empty()
        && (!b.stored_packages || b.stored_fixed_owner)
    {
        owned::profile(module, b)?
    } else {
        false
    };
    super::stored_callables::definitions(module, b)?;
    let mut foreign_symbols = BTreeSet::new();
    let mut ordinary_observations = false;
    for f in &module.functions {
        if !b.composition_stored.is_empty() && !b.composition_stored[f.id.0 as usize] {
            continue;
        }
        if (f.foreign.is_some()
            && !super::stored_callables::scalar_foreign(f, b)?
            && !crate::tasks::input_scope::provider(module, f, b)?)
            || f.call_kind != CallKind::Direct
                && !(b.task_sources() && f.call_kind == CallKind::Suspend)
        {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                format!(
                    "stored profile requires ordinary direct function bodies: {}",
                    f.name
                ),
                Some(f.span),
            )]);
        }
        if let Some(declaration) = &f.foreign {
            b.spend(
                foreign_symbols
                    .len()
                    .saturating_add(1)
                    .saturating_mul(declaration.symbol.len().saturating_add(1)),
                "stored scalar foreign symbol uniqueness",
            )?;
            if !foreign_symbols.insert(declaration.symbol.as_str()) {
                return Err(failure(
                    "scalar foreign symbol has more than one actual declaration",
                ));
            }
        }
        if let Some(id) = f.environment {
            let d = module
                .closures
                .get(id.0 as usize)
                .ok_or_else(|| failure("callback function environment is absent"))?;
            if !super::stored_callables::finite(module, id) || d.function != f.id {
                return Err(failure("callback function is not the actual finite entry"));
            }
        }
        for ty in f
            .locals
            .iter()
            .map(|l| &l.ty)
            .chain(&f.values)
            .chain(std::iter::once(&f.return_type))
        {
            b.type_work(ty, true, "stored ordinary executable value profile")?;
            let byte_value = owned_byte_profile && owned_byte_views::value(ty, b)?;
            let mixed_resource = if b.stored_packages || b.stored_compound_values {
                fee(b, module.resources.len())?;
                matches!(ty, Type::Managed(payload) | Type::Desc(payload)
                    if module.resources.iter().any(|definition| definition.payload == **payload))
                    || matches!(ty, Type::Address { inner, .. }
                        if matches!(inner.as_ref(), Type::Managed(payload)
                            if module.resources.iter().any(|definition| definition.payload == **payload)))
            } else {
                false
            };
            if !(byte_value
                || dyn_address::admitted_value(module, ty, b)?
                || super::stored_callables::value(module, ty)
                || b.task_sources() && task_address::admitted_value(ty)
                || carrier(ty).is_some()
                || pointed_carrier(ty).is_some()
                || matches!(ty,Type::Address{inner,..}if inner.is_address_pointee())
                || ty.is_copy_value()
                || shared_array_shape(ty, b)?
                || matches!(ty, Type::Address { inner, .. } if shared_array_shape(inner, b)?)
                || b.stored_owned_fixed && owned::data(ty, b)?
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
                || shared_compound(module, ty, b)?
                || affine_compound(module, ty, b)?
                || pure_inline(module, ty, b)?
                || matches!(ty, Type::Address { inner, mutable: false } if pure_inline(module, inner, b)?)
                || owned::ordinary_value(module, ty, b)?)
            {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Analyze,
                    format!(
                        "owned or opaque value has no actual stored permission integration in {}: {ty:?}",
                        f.name
                    ),
                    Some(f.span),
                )]);
            }
        }
        if super::stored_callables::fixed_value(module, &f.return_type)
            && f.callable_return_contract.is_none()
        {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "captured Fixed environment escape needs its retained result target transfer",
                Some(f.span),
            )]);
        }
        if f.foreign.is_none() {
            ordinary_observations |= actual_instructions(module, f, &graphs[f.id.0 as usize], b)?;
        }
        super::class_flow::check(module, f, &graphs[f.id.0 as usize].1, b)?;
    }
    super::stored_callables::roles(module, graphs, b)?;
    let facts = derive(module, graphs, b)?;
    if b.qualified_package_returns {
        fee(
            b,
            std::mem::size_of::<BTreeMap<(FunctionId, ValueId), u32>>().saturating_add(facts.len()),
        )?;
        let mut calls = BTreeMap::new();
        for (index, facts) in facts.iter().enumerate() {
            fee(b, facts.package_return_calls.len())?;
            for (value, position) in &facts.package_return_calls {
                ordered(b, calls.len(), 2)?;
                fee(b, std::mem::size_of::<((FunctionId, ValueId), u32)>())?;
                calls.insert((module.functions[index].id, *value), *position);
            }
        }
        b.package_return_calls = Some(calls);
    }
    // A scalar capture has no destructor, but a concrete FnOnce environment
    // still has one actual consumption obligation. The stored dispatcher must
    // not bypass the independent SSA/local responsibility ledger.
    fee(b, module.closures.len())?;
    if b.composition_stored.is_empty()
        && module
            .closures
            .iter()
            .any(|d| d.kind == CallableKind::FnOnce)
        || b.stored_owned_fixed
        || b.stored_compound_values
        || b.owning_environment_captures
            && b.composition_stored.is_empty()
            && !b.task_borrowed_results
            && !b.store_option_results
        || (b.qualified_package_returns || b.byte_queries || ordinary_observations)
            && b.composition_stored.is_empty()
            && !b.task_sources()
            && !b.masked_stored_tasks()
    {
        super::resources::check_module(module, graphs, b)?;
    }
    let mut result = Vec::new();
    fee(b, facts.len())?;
    let mut option_targets = b
        .store_option_results
        .then(|| Vec::with_capacity(facts.len()));
    for (index, function) in facts.into_iter().enumerate() {
        if b.task_sources() && b.masked_stored_tasks() {
            fee(b, 1)?;
            b.task_stored_coverage
                .push(b.composition_stored.is_empty() || b.composition_stored[index]);
        }
        if b.masked_stored_tasks()
            && !b.composition_stored.is_empty()
            && !b.composition_stored[index]
        {
            let count = module.functions[index].values.len();
            fee(b, count.saturating_mul(2))?;
            if let Some(actual) = &mut option_targets {
                actual.push(vec![Vec::new(); count]);
            }
            if b.task_sources() {
                fee(b, count)?;
                b.task_pointer_roots.push(vec![Vec::new(); count]);
                fee(b, count)?;
                b.task_pointer_parents.push(vec![Vec::new(); count]);
            }
            result.push(
                (0..count)
                    .map(|id| ValueHold {
                        value: ValueId(id as u32),
                        roots: Vec::new(),
                    })
                    .collect(),
            );
            continue;
        }
        if let Some(actual) = &mut option_targets {
            fee(b, function.values.len())?;
            for row in &function.values {
                fee(b, row.len())?;
                for target in row {
                    fee(b, target_width(target))?;
                }
            }
            actual.push(function.values.clone());
        }
        if b.task_sources() {
            if function.pointer_values.len() != function.values.len() {
                return Err(failure("Task physical-source proof omitted an SSA value"));
            }
            b.task_pointer_roots.push(function.pointer_values);
            if b.masked_stored_tasks() {
                if function.pointer_parents.len() != function.values.len() {
                    return Err(failure("Task physical parent proof omitted an SSA value"));
                }
                b.task_pointer_parents.push(function.pointer_parents);
            }
        }
        let mut rows = Vec::new();
        fee(b, function.values.len())?;
        for (id, targets) in function.values.into_iter().enumerate() {
            let mut roots = BTreeSet::new();
            // A weak resource descriptor retains its full private target and
            // cleanup proof, but it publishes no strong address Hold.
            let weak_description = matches!(&module.functions[index].values[id],
                Type::Desc(payload) if !matches!(payload.as_ref(),
                    Type::ByteBuffer | Type::Task { .. } | Type::TaskI32
                    | Type::OwnedStore { .. } | Type::Mutex { .. }));
            for target in targets {
                ordered(b, roots.len(), 0)?;
                if !weak_description {
                    roots.insert(target.base);
                }
            }
            rows.push(ValueHold {
                value: ValueId(id as u32),
                roots: roots.into_iter().collect(),
            });
        }
        result.push(rows);
    }
    b.store_option_targets = option_targets;
    Ok(result)
}

fn address_result(
    f: &TypedFunction,
    i: &Instruction,
    mutable: bool,
    payload: &Type,
    b: &mut Budget,
) -> CompileResult<()> {
    b.type_work(payload, false, "stored actual producer result comparison")?;
    let result = i
        .result
        .ok_or_else(|| failure("actual address producer lacks result"))?;
    if !matches!(value_type(f,result)?,Type::Address{mutable:m,inner}if *m==mutable&&inner.as_ref()==payload)
    {
        return Err(failure(
            "address producer's actual mode/payload differs from claimed type",
        ));
    }
    Ok(())
}

fn actual_instructions(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &(Vec<Vec<usize>>, Vec<bool>),
    b: &mut Budget,
) -> CompileResult<bool> {
    // A stored exclusive leaf moves with its complete SSA carrier. Two copies
    // retaining the same loan ID otherwise appear to be one permission.
    fn consumed(kind: &InstructionKind) -> (Option<ValueId>, &[ValueId]) {
        match kind {
            InstructionKind::Store { value, .. }
            | InstructionKind::StorePackageField { value, .. }
            | InstructionKind::StoreOwnedArrayElement { value, .. }
            | InstructionKind::StoreStoredBorrow { value, .. }
            | InstructionKind::MakeErrorUnion { value, .. }
            | InstructionKind::TakeErrorUnion { value, .. }
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
    let mut affine: Vec<bool> = f.values.iter().map(exclusive_carrier_value).collect();
    for instruction in f.blocks.iter().flat_map(|block| &block.instructions) {
        if transfers_address(&instruction.kind) {
            let (first, rest) = consumed(&instruction.kind);
            for value in first.into_iter().chain(rest.iter().copied()) {
                affine[value.0 as usize] |= affine_stored_input(&f.values[value.0 as usize]);
            }
        }
    }
    if affine.iter().any(|exclusive| *exclusive) {
        let (preds, reachable) = graph;
        let width = affine.len();
        fee(b, width.saturating_mul(f.blocks.len().saturating_add(2)))?;
        let mut outputs = vec![vec![false; width]; f.blocks.len()];
        loop {
            let mut changed = false;
            for (block_index, block) in f.blocks.iter().enumerate() {
                if !reachable[block_index] {
                    continue;
                }
                let mut spent = vec![false; width];
                if block_index != f.entry.0 as usize {
                    for &pred in &preds[block_index] {
                        if reachable[pred] {
                            fee(b, width)?;
                            for (current, previous) in spent.iter_mut().zip(&outputs[pred]) {
                                *current |= *previous;
                            }
                        }
                    }
                }
                for instruction in &block.instructions {
                    if let Some(value) = instruction.result {
                        spent[value.0 as usize] = false;
                    }
                    let (first, rest) = consumed(&instruction.kind);
                    for value in first.into_iter().chain(rest.iter().copied()) {
                        let index = value.0 as usize;
                        fee(b, 1)?;
                        if affine[index]
                            && (exclusive_carrier_value(&f.values[index])
                                || transfers_address(&instruction.kind))
                        {
                            spent[index] = true;
                        }
                    }
                }
                if spent != outputs[block_index] {
                    outputs[block_index] = spent;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        for (block_index, block) in f.blocks.iter().enumerate() {
            if !reachable[block_index] {
                continue;
            }
            let mut spent = vec![false; width];
            if block_index != f.entry.0 as usize {
                for &pred in &preds[block_index] {
                    if reachable[pred] {
                        fee(b, width)?;
                        for (current, previous) in spent.iter_mut().zip(&outputs[pred]) {
                            *current |= *previous;
                        }
                    }
                }
            }
            for instruction in &block.instructions {
                if let Some(value) = instruction.result {
                    spent[value.0 as usize] = false;
                }
                let (first, rest) = consumed(&instruction.kind);
                for value in resources::uses(&instruction.kind) {
                    fee(b, 1)?;
                    let index = value.0 as usize;
                    if !affine[index] || !spent[index] {
                        continue;
                    }
                    fee(b, rest.len().saturating_add(1))?;
                    let consuming = (first == Some(value) || rest.contains(&value))
                        && (exclusive_carrier_value(&f.values[index])
                            || transfers_address(&instruction.kind));
                    if !consuming {
                        return Err(failure(
                            "exclusive stored SSA value was used after transfer",
                        ));
                    }
                }
                for value in first.into_iter().chain(rest.iter().copied()) {
                    let index = value.0 as usize;
                    fee(b, 1)?;
                    if affine[index]
                        && (exclusive_carrier_value(&f.values[index])
                            || transfers_address(&instruction.kind))
                        && std::mem::replace(&mut spent[index], true)
                    {
                        return Err(failure(
                            "exclusive stored carrier SSA value was transferred twice",
                        ));
                    }
                }
            }
            if let Terminator::Return(Some(value)) = block.terminator
                && affine[value.0 as usize]
                && spent[value.0 as usize]
            {
                return Err(failure(
                    "exclusive stored carrier SSA value was returned after transfer",
                ));
            }
        }
    }
    let mut ordinary_observations = false;
    let mut scalar_ffi = false;
    let (preds, reachable) = graph;
    fee(b, f.values.len())?;
    let mut definitions = vec![false; f.values.len()];
    for block in &f.blocks {
        for i in &block.instructions {
            fee(b, 1)?;
            let byte_view = owned_byte_views::instruction(module, f, i, b)?;
            let unpack_failure = failure_unpack(f, i, b)?;
            let byte_description = owned_byte_views::description_instruction(module, f, i, b)?;
            let byte_query = byte_queries::instruction(module, f, i, b)?;
            let package_read = pure_copy::ordinary_package_read(f, i, b)?;
            let package_description = pure_copy::ordinary_package_description(f, i, b)?;
            let error_borrow = ordinary_error_borrow(f, i, b)?;
            let compound_take = compound_move(module, f, i, b)?
                || compound_unpack(module, f, i, b)?
                || compound_constructor(module, f, i, b)?
                || compound_drop(module, f, i, b)?;
            ordinary_observations |=
                package_read || package_description || error_borrow || byte_view;
            let enum_operation = enum_address::instruction(module, f, i, b)?;
            let owned_operation = (b.stored_owned_fixed
                || b.stored_compound_values && matches!(i.kind,InstructionKind::DropManaged{local} if matches!(&f.locals[local.0 as usize].ty,Type::Managed(payload) if matches!(payload.as_ref(),Type::Record{..})))
                || matches!(i.kind, InstructionKind::ReadDesc { .. })
                || matches!(i.kind, InstructionKind::MakeManaged { .. })
                    && i.result.is_some_and(|id| matches!(f.values.get(id.0 as usize),
                        Some(Type::Managed(payload)) if matches!(payload.as_ref(), Type::Record { .. }))))
                && owned::instruction(module, f, i, b)?;
            if !super::class_flow::pointer_instruction(f, i)
                && !super::stored_callables::instruction(module, f, i)
                && !owned_operation
                && !enum_operation
                && !byte_query
                && !byte_view
                && !byte_description
                && !package_read
                && !package_description
                && !error_borrow
                && !dyn_address::instruction(module, f, i, b)?
                && !compound_take
                && !(b.task_sources() && task_address::operation(&i.kind))
                && !unpack_failure
                && !crate::tasks::input_scope::registration_operation(f, i, b)?
                && !failure_tuple_drop(f, i, b)?
                && !(matches!(
                    i.kind,
                    InstructionKind::OwnedStoreNew { .. }
                        | InstructionKind::OwnedStoreReserve { .. }
                        | InstructionKind::OwnedStorePush { .. }
                        | InstructionKind::OwnedStorePop { .. }
                        | InstructionKind::OwnedStoreCapacity { .. }
                        | InstructionKind::OwnedStoreLen { .. }
                        | InstructionKind::OwnedStoreBorrow { .. }
                        | InstructionKind::OwnedStoreBorrowMut { .. }
                        | InstructionKind::DropErrorUnion { .. }
                        | InstructionKind::DescribePackageField { .. }
                ))
                && !pure_copy::operation(&i.kind)
                && !pure_copy::ordinary_owned_union(f, i, b)?
                && !pure_copy::ordinary_package_borrow(module, f, i, b)?
                && !matches!(
                    &i.kind,
                    InstructionKind::StaticUtf8 { .. }
                        | InstructionKind::Constant(_)
                        | InstructionKind::Load(_)
                        | InstructionKind::Store { .. }
                        | InstructionKind::StorageLive(_)
                        | InstructionKind::StorageDead(_)
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
                        | InstructionKind::BindPackageCapture { .. }
                        | InstructionKind::EndPackageCapture { .. }
                        | InstructionKind::LoadStoredBorrow {
                            mode: StoredBorrowLoad::CopyShared
                                | StoredBorrowLoad::Reborrow { mutable: true }
                                | StoredBorrowLoad::Take,
                            ..
                        }
                        | InstructionKind::StoreStoredBorrow { .. }
                )
            {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Analyze,
                    format!(
                        "instruction is outside the actual stored profile, including in unreachable blocks in {}: {:?}",
                        f.name, i.kind
                    ),
                    Some(i.span),
                )]);
            }
            if let Some(v) = i.result {
                let slot = definitions
                    .get_mut(v.0 as usize)
                    .ok_or_else(|| failure("result outside actual values"))?;
                if *slot {
                    return Err(failure("one SSA value has multiple actual definitions"));
                }
                *slot = true;
            }
            for v in resources::uses(&i.kind) {
                fee(b, 1)?;
                value_type(f, v)?;
            }
            let result = |ty: &Type, b: &mut Budget| -> CompileResult<()> {
                b.type_work(ty, false, "stored actual scalar/carrier result")?;
                if i.result
                    .is_none_or(|v| f.values.get(v.0 as usize) != Some(ty))
                {
                    return Err(failure(
                        "instruction result differs from actual producer type",
                    ));
                }
                Ok(())
            };
            if let InstructionKind::BindPackageCapture { local, index }
            | InstructionKind::EndPackageCapture { local, index } = i.kind
            {
                let actual = f
                    .locals
                    .get(local.0 as usize)
                    .ok_or_else(|| failure("captured view local absent"))?;
                let captured = f
                    .environment
                    .and_then(|id| module.closures.get(id.0 as usize))
                    .and_then(|d| d.captures.get(index as usize))
                    .ok_or_else(|| failure("captured view field absent"))?;
                b.type_work(captured, true, "captured owning view actual type")?;
                if i.result.is_some()
                    || actual.ty != *captured
                    || !matches!(resources::package_storage(captured), Type::Package { .. })
                {
                    return Err(failure(
                        "captured owning view differs from its complete actual field",
                    ));
                }
            }
            if super::class_flow::pointer_instruction(f, i) {
                super::class_flow::pointer_types(module, f, i, b)?;
            }
            if super::stored_callables::instruction(module, f, i) {
                super::stored_callables::types(module, f, i, b)?;
            }
            if array_index::candidate(f, i) {
                array_index::types(f, i, b)?;
                fee(b, b.array_outer_functions.len().saturating_add(1))?;
                if !b.array_outer_functions.contains(&f.id) {
                    b.array_outer_functions.push(f.id);
                }
            }
            if pure_copy::operation(&i.kind)
                && !compound_extract(module, f, i, b)?
                && !dyn_address::instruction(module, f, i, b)?
                && !array_index::candidate(f, i)
                && enum_address::types(module, f, i, b)?.is_none()
                && option_address::types(module, f, i, b)?.is_none()
                && !(b.task_sources() && task_address::wrapper_instruction(f, i, b)?)
            {
                // A qualified Enum claim cannot become an unsupported Copy
                // operation merely by deleting its nominal instance inventory.
                let claimed = match i.kind {
                    InstructionKind::MakeVariant { .. } => {
                        i.result.map(|v| value_type(f, v)).transpose()?
                    }
                    InstructionKind::ReadVariant { value, .. }
                    | InstructionKind::VariantTag { value } => Some(value_type(f, value)?),
                    _ => None,
                };
                if claimed.is_some_and(enum_address::candidate) {
                    return Err(failure(
                        "borrowed Enum operation has no authenticated complete nominal instance",
                    ));
                }
                if !pure_copy::ordinary_owned_union(f, i, b)? {
                    pure_copy::types(f, i, b)?;
                }
            }
            match &i.kind {
                InstructionKind::UnsafeEnter { .. } | InstructionKind::UnsafeExit { .. } => {
                    scalar_ffi = true;
                    if i.result.is_some() {
                        return Err(failure("unsafe region marker cannot produce an SSA result"));
                    }
                }
                InstructionKind::ForeignCall {
                    function,
                    arguments,
                    ..
                } => {
                    scalar_ffi = true;
                    let callee = module
                        .functions
                        .get(function.0 as usize)
                        .ok_or_else(|| failure("scalar foreign target absent"))?;
                    if !super::stored_callables::scalar_foreign(callee, b)?
                        || arguments.len() != callee.parameters.len()
                    {
                        return Err(failure("foreign call is outside actual scalar ABI"));
                    }
                    fee(b, arguments.len())?;
                    for (argument, parameter) in arguments.iter().zip(&callee.parameters) {
                        let expected = &callee
                            .locals
                            .get(parameter.0 as usize)
                            .ok_or_else(|| failure("foreign actual parameter absent"))?
                            .ty;
                        b.type_work(
                            expected,
                            false,
                            "foreign scalar actual parameter comparison",
                        )?;
                        if value_type(f, *argument)? != expected {
                            return Err(failure(
                                "foreign scalar argument differs from its declaration",
                            ));
                        }
                    }
                    if callee.return_type == Type::Unit {
                        if i.result.is_some() {
                            return Err(failure("Unit foreign call fabricates a value"));
                        }
                    } else {
                        result(&callee.return_type, b)?;
                    }
                }
                InstructionKind::BorrowCapture { index, mutable }
                    if f.environment
                        .is_some_and(|id| b.returned_borrowed_environments.contains(&id))
                        && f.environment
                            .and_then(|id| module.closures.get(id.0 as usize))
                            .and_then(|d| d.captures.get(*index as usize))
                            .is_some_and(|ty| matches!(ty, Type::Closure(_))) =>
                {
                    let ty = &module.closures[f.environment.unwrap().0 as usize].captures
                        [*index as usize];
                    address_result(f, i, *mutable, ty, b)?;
                }
                InstructionKind::BorrowCapture {
                    index,
                    mutable: false,
                } => {
                    let d = f
                        .environment
                        .and_then(|id| module.closures.get(id.0 as usize))
                        .filter(|d| super::stored_callables::lending(d))
                        .ok_or_else(|| failure("capture borrow outside real lending entry"))?;
                    let ty = d
                        .captures
                        .get(*index as usize)
                        .filter(|ty| super::stored_callables::scalar(ty))
                        .ok_or_else(|| {
                            failure("capture borrow lacks actual inline scalar field")
                        })?;
                    address_result(f, i, false, ty, b)?;
                }
                InstructionKind::ExtractField { .. } => {}
                InstructionKind::ConvertInteger { value, target }
                | InstructionKind::ConvertNumber { value, target } => {
                    if !value_type(f, *value)?.is_numeric() || !target.is_numeric() {
                        return Err(failure(
                            "ordinary numeric conversion has nonnumeric actual data",
                        ));
                    }
                    result(target, b)?;
                }
                InstructionKind::Load(local) => {
                    let ty = &f.locals[local.0 as usize].ty;
                    if exclusive_carrier_value(ty) {
                        return Err(failure(
                            "copy of actual exclusive stored address carrier requires transfer",
                        ));
                    }
                    result(ty, b)?;
                }
                InstructionKind::Store { local, value } => {
                    let ty = &f.locals[local.0 as usize].ty;
                    b.type_work(ty, false, "stored actual local assignment")?;
                    if value_type(f, *value)? != ty {
                        return Err(failure(
                            "actual local assignment changes complete value type",
                        ));
                    }
                }
                InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local } => {
                    let exclusive = matches!(i.kind, InstructionKind::BorrowLocalMut { .. });
                    let local = &f.locals[local.0 as usize];
                    if exclusive && !local.mutable {
                        return Err(failure("exclusive borrow lacks actual mutable local"));
                    }
                    address_result(f, i, exclusive, &local.ty, b)?;
                }
                InstructionKind::ReborrowAddress { address, mutable } => {
                    let Type::Address {
                        mutable: available,
                        inner,
                    } = value_type(f, *address)?
                    else {
                        return Err(failure("reborrow source is not an actual Address"));
                    };
                    if *mutable && !*available {
                        return Err(failure("reborrow upgrades shared actual authority"));
                    }
                    address_result(f, i, *mutable, inner, b)?;
                }
                InstructionKind::AddressField { address, index } => {
                    let Type::Address { mutable, inner } = value_type(f, *address)? else {
                        return Err(failure("field base is not an actual Address"));
                    };
                    let inner = carrier(inner).map_or(inner.as_ref(), |(storage, _)| storage);
                    let inner = if let Type::Managed(payload) = inner {
                        if !*mutable && super::stored_callables::resource_payload(payload, b)? {
                            payload.as_ref()
                        } else {
                            return Err(failure(
                                "resource field observation requires an actual shared scalar payload",
                            ));
                        }
                    } else {
                        inner
                    };
                    let child = match inner {
                        Type::Record { fields, .. } | Type::Package { fields, .. } => {
                            fields.get(*index as usize).map(|(_, t)| t)
                        }
                        Type::Tuple(fs) => fs.get(*index as usize),
                        _ => None,
                    }
                    .ok_or_else(|| {
                        failure("actual field is absent or crosses non-inline storage")
                    })?;
                    b.type_work(child, true, "stored actual ordinary inline projection")?;
                    if *mutable && shared_record_child(child, b)? {
                        return Err(vec![Diagnostic::unsupported(
                            Phase::Analyze,
                            "exclusive nested Record windows require their slot consumer",
                            Some(i.span),
                        )]);
                    }
                    if !child.is_copy_value()
                        && !pure_read(module, child, b)?
                        && !pure_inline(module, child, b)?
                        && !(matches!(child, Type::BorrowedStorage { storage, .. }
                            if matches!(storage.as_ref(), Type::Package { .. }))
                            && physical(child, b)?)
                        && !(shared_record_child(child, b)? && physical(child, b)?)
                        && !shared_array_shape(child, b)?
                        && !matches!(child, Type::Managed(payload)
                            if module.resources.iter().any(|definition| definition.payload == **payload))
                        && !owned::ordinary_value(module, child, b)?
                    {
                        return Err(failure(&format!(
                            "stored borrowed leaf must use actual slot-load protocol in {}: {:?}",
                            f.name, i.kind
                        )));
                    }
                    address_result(f, i, *mutable, child, b)?;
                }
                InstructionKind::AddressIndex { address, index } => {
                    let Type::Address { mutable, inner } = value_type(f, *address)? else {
                        return Err(failure("index base is not an Address"));
                    };
                    if !value_type(f, *index)?.is_integer() {
                        return Err(failure("index is not an integer"));
                    }
                    let element = match inner.as_ref() {
                        Type::Array { element, .. } | Type::Slice { element } => element.as_ref(),
                        _ => return Err(failure("actual index base is not Array/Slice")),
                    };
                    address_result(f, i, *mutable, element, b)?;
                }
                InstructionKind::AddressSlice {
                    address,
                    start,
                    end,
                } => {
                    let Type::Address { mutable, inner } = value_type(f, *address)? else {
                        return Err(failure("slice base is not an Address"));
                    };
                    for index in start.iter().chain(end) {
                        if !value_type(f, *index)?.is_integer() {
                            return Err(failure("slice bound is not an integer"));
                        }
                    }
                    let element = match inner.as_ref() {
                        Type::Array { element, .. } | Type::Slice { element } => element.as_ref(),
                        _ => return Err(failure("actual slice base is not Array/Slice")),
                    };
                    b.type_work(element, true, "stored actual slice payload")?;
                    let Some(Type::Address {
                        mutable: claimed,
                        inner,
                    }) = i.result.and_then(|v| f.values.get(v.0 as usize))
                    else {
                        return Err(failure("slice lacks Address result"));
                    };
                    if claimed != mutable
                        || !matches!(inner.as_ref(),Type::Slice{element:t}if t.as_ref()==element)
                    {
                        return Err(failure("slice result differs from actual source"));
                    }
                }
                InstructionKind::ReadAddress { address } => {
                    let Type::Address { inner, .. } = value_type(f, *address)? else {
                        return Err(failure("read base is not an Address"));
                    };
                    b.type_work(inner, true, "stored actual ordinary read payload")?;
                    if !inner.is_copy_value()
                        && !shared_array_shape(inner, b)?
                        && !pure_read(module, inner, b)?
                        && !pure_inline(module, inner, b)?
                    {
                        return Err(failure(
                            "ordinary read cannot copy stored carrier/borrow leaf",
                        ));
                    }
                    result(inner, b)?;
                }
                InstructionKind::WriteAddress { address, value } => {
                    let Type::Address {
                        mutable: true,
                        inner,
                    } = value_type(f, *address)?
                    else {
                        return Err(failure("write lacks exclusive actual Address"));
                    };
                    b.type_work(inner, true, "stored actual ordinary write payload")?;
                    if !inner.is_copy_value() || value_type(f, *value)? != inner.as_ref() {
                        return Err(failure(
                            "ordinary write changes actual slot state/type outside protocol",
                        ));
                    }
                }
                InstructionKind::LoadStoredBorrow { .. }
                | InstructionKind::StoreStoredBorrow { .. } => {
                    super::stored_borrows::instruction(f, i, b)?
                }
                InstructionKind::MakeAggregate { fields }
                | InstructionKind::MakePackage { fields }
                | InstructionKind::MakeOwnedTuple { fields } => {
                    let ty = value_type(
                        f,
                        i.result.ok_or_else(|| failure("aggregate lacks result"))?,
                    )?;
                    let ty = carrier(ty).map_or(ty, |(storage, _)| storage);
                    let count = match ty {
                        Type::Record { fields, .. } | Type::Package { fields, .. } => fields.len(),
                        Type::Tuple(fs) => fs.len(),
                        Type::Array { length, .. } => *length as usize,
                        _ => return Err(failure("aggregate has no actual inline layout")),
                    };
                    if fields.len() != count {
                        return Err(failure("aggregate omits actual fields"));
                    }
                    for (position, v) in fields.iter().enumerate() {
                        let expected = match ty {
                            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                                &fields[position].1
                            }
                            Type::Tuple(fs) => &fs[position],
                            Type::Array { element, .. } => element.as_ref(),
                            _ => unreachable!(),
                        };
                        b.type_work(expected, false, "stored actual constructed field")?;
                        if value_type(f, *v)? != expected {
                            return Err(failure("constructed field differs from actual full type"));
                        }
                    }
                }
                InstructionKind::TakePackageField { local, index }
                | InstructionKind::DropPackageField { local, index }
                | InstructionKind::StorePackageField { local, index, .. } => {
                    let parent = &f
                        .locals
                        .get(local.0 as usize)
                        .ok_or_else(|| failure("package field local is absent"))?
                        .ty;
                    let parent = carrier(parent).map_or(parent, |(storage, _)| storage);
                    let Type::Package { fields, .. } = parent else {
                        return Err(failure("package field operation lacks actual shell"));
                    };
                    let field = &fields
                        .get(*index as usize)
                        .ok_or_else(|| failure("package field index is absent"))?
                        .1;
                    b.type_work(field, true, "package field operation actual child")?;
                    match &i.kind {
                        InstructionKind::TakePackageField { .. } => {
                            let projected = failure_projection(f, i, b)?;
                            result(projected.as_ref().unwrap_or(field), b)?;
                        }
                        InstructionKind::StorePackageField { value, .. } => {
                            if i.result.is_some() || value_type(f, *value)? != field {
                                return Err(failure(
                                    "stored Package field differs from its complete child type",
                                ));
                            }
                        }
                        InstructionKind::DropPackageField { .. } if i.result.is_some() => {
                            return Err(failure("Package field drop fabricates a result"));
                        }
                        _ => {}
                    }
                }
                InstructionKind::Call {
                    function,
                    arguments,
                } => {
                    let target = module
                        .functions
                        .get(function.0 as usize)
                        .ok_or_else(|| failure("actual call target absent"))?;
                    if arguments.len() != target.parameters.len() {
                        return Err(failure("call arity differs from actual body signature"));
                    }
                    for (v, l) in arguments.iter().zip(&target.parameters) {
                        let expected = target
                            .locals
                            .get(l.0 as usize)
                            .ok_or_else(|| failure("callee parameter absent"))?;
                        b.type_work(&expected.ty, false, "stored complete actual call argument")?;
                        if value_type(f, *v)? != &expected.ty {
                            return Err(failure(
                                "call argument differs from complete actual signature",
                            ));
                        }
                    }
                    if target.return_type == Type::Unit {
                        if i.result.is_some() {
                            return Err(failure("Unit call fabricates a result"));
                        }
                    } else {
                        result(&target.return_type, b)?;
                    }
                }
                _ => {}
            }
        }
    }
    if b.capture_relations {
        fee(b, definitions.len())?;
        if definitions.iter().any(|defined| !defined) {
            return Err(failure(
                "captured context SSA slot has no actual definition",
            ));
        }
    }
    for block in &f.blocks {
        fee(b, 1)?;
        match block.terminator {
            Terminator::Return(Some(v)) => {
                b.type_work(&f.return_type, false, "stored actual return signature")?;
                if value_type(f, v)? != &f.return_type {
                    return Err(failure(
                        "actual returned value differs from complete return signature",
                    ));
                }
            }
            Terminator::Return(None) if f.return_type != Type::Unit => {
                return Err(failure("non-Unit return omits actual value"));
            }
            Terminator::Branch { condition, .. } if value_type(f, condition)? != &Type::Bool => {
                return Err(failure("actual branch condition is not Bool"));
            }
            _ => {}
        }
    }
    b.product(
        f.blocks.len(),
        f.values.len(),
        2,
        "stored actual SSA availability",
    )?;
    if scalar_ffi {
        super::stored_callables::foreign_regions(f, preds, reachable, b)?;
    }
    let mut inputs = vec![vec![true; f.values.len()]; f.blocks.len()];
    let mut outputs = inputs.clone();
    fee(b, f.values.len())?;
    inputs[f.entry.0 as usize].fill(false);
    loop {
        let mut changed = false;
        for (bi, block) in f.blocks.iter().enumerate() {
            if !reachable[bi] {
                continue;
            }
            fee(b, f.values.len())?;
            let mut live = if bi == f.entry.0 as usize {
                vec![false; f.values.len()]
            } else {
                vec![true; f.values.len()]
            };
            if bi != f.entry.0 as usize {
                for p in &preds[bi] {
                    if reachable[*p] {
                        fee(b, live.len())?;
                        for (a, z) in live.iter_mut().zip(&outputs[*p]) {
                            *a &= *z;
                        }
                    }
                }
            }
            fee(b, live.len())?;
            inputs[bi].clone_from(&live);
            for i in &block.instructions {
                if let Some(v) = i.result {
                    fee(b, 1)?;
                    live[v.0 as usize] = true;
                }
            }
            fee(b, live.len())?;
            if live != outputs[bi] {
                outputs[bi] = live;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    for (bi, block) in f.blocks.iter().enumerate() {
        if !reachable[bi] {
            continue;
        }
        fee(b, inputs[bi].len())?;
        let mut live = inputs[bi].clone();
        for i in &block.instructions {
            for v in resources::uses(&i.kind) {
                fee(b, 1)?;
                if !live[v.0 as usize] {
                    return Err(failure(
                        "actual operand definition does not dominate its stored/loan use",
                    ));
                }
            }
            if let Some(v) = i.result {
                live[v.0 as usize] = true;
            }
        }
        if let Terminator::Return(Some(v)) | Terminator::Branch { condition: v, .. } =
            block.terminator
        {
            fee(b, 1)?;
            if !live.get(v.0 as usize).copied().unwrap_or(false) {
                return Err(failure("terminator operand is not actually available"));
            }
        }
    }
    Ok(ordinary_observations)
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
    fn addressed_capture_selects_current_memory_without_unioning_an_old_ssa_slot() {
        let module = fixture::captured_fixed(CallableKind::Fn, false);
        let f = &module.functions[1];
        let mut b = Budget::new(&Limits::default());
        let mut state = State::default();
        let mut loans = Vec::new();
        let mut values = vec![Value::default(); f.values.len()];
        values[0] = seed_value(
            &module,
            &f.values[0],
            HoldRoot::Parameter(0),
            &mut state,
            &mut loans,
            &mut b,
        )
        .unwrap();
        values[1] = seed_value(
            &module,
            &f.values[1],
            HoldRoot::Parameter(1),
            &mut state,
            &mut loans,
            &mut b,
        )
        .unwrap();
        let slot = vec![PlaceProjection::Field(0)];
        let previous = values[0].slots[&slot].clone();
        let current = values[1].slots[&slot].clone();
        state
            .external
            .get_mut(&HoldRoot::Parameter(0))
            .unwrap()
            .insert(slot.clone(), current.clone());
        // Keep the original address SSA and its earlier slot snapshot intact.
        // Admission/class equations are tested separately; this temporal-state
        // oracle checks the real memory selection and every parent loan.
        let instruction = f
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .find(|instruction| matches!(instruction.kind, InstructionKind::MakeClosure { .. }))
            .unwrap();
        let out = transfer(
            &module,
            f,
            instruction,
            &mut state,
            &values,
            &[],
            &SlotLoanIds::new(),
            &mut loans,
            &mut b,
        )
        .unwrap();
        let leaf = &out.environment[&vec![PlaceProjection::Field(0), PlaceProjection::Field(0)]];
        assert_eq!(leaf.targets, current.targets);
        assert!(leaf.loans.is_superset(&current.loans));
        assert!(out.held.loans.is_superset(&values[0].pointer.loans));
        assert!(leaf.loans.is_disjoint(&previous.loans));
        assert_eq!(values[0].slots[&slot], previous);
    }

    #[test]
    fn heap_target_union_exhausts_remaining_budget_before_publishing_permission() {
        for heap in [
            None,
            Some(vec![]),
            Some(vec![PlaceProjection::Field(0); 128]),
        ] {
            let from = Permission {
                targets: Targets::from([Target {
                    retained_port: None,
                    base: HoldRoot::Parameter(0),
                    stored: vec![PlaceProjection::Field(0)],
                    place: vec![PlaceProjection::Field(1)],
                    heap_parents: Vec::new(),
                    heap_element: heap,
                }]),
                loans: Loans::new(),
            };
            let mut to = Permission::default();
            let mut budget = Budget::new(&Limits {
                max_analysis_steps: 1_100,
                ..Limits::default()
            });
            fee(&mut budget, 1_000).unwrap();
            let result = union(&mut to, &from, &mut budget);
            if from
                .targets
                .first()
                .unwrap()
                .heap_element
                .as_ref()
                .is_some_and(|p| !p.is_empty())
            {
                assert!(
                    result
                        .unwrap_err()
                        .iter()
                        .all(|d| d.code == "E-RESOURCE-LIMIT")
                );
                assert!(
                    to.targets.is_empty(),
                    "an underfunded clone must not publish permission"
                );
            } else {
                assert!(result.unwrap());
                assert_eq!(to, from);
                assert_eq!(
                    budget.remaining(),
                    94,
                    "empty heap keeps the old request fee"
                );
            }
        }
    }
    #[test]
    fn owned_fixed_once_keeps_resource_and_class_proofs_independent() {
        let m = fixture::captured_owned_fixed(false);
        let limits = Limits::default();
        let plan = super::super::analyze(&m, &limits).unwrap();
        assert!(plan.functions[0].holds[7].roots.is_empty());
        assert_eq!(
            plan.functions[0].holds[8].roots,
            vec![HoldRoot::Local(LocalId(1))]
        );
        assert!(super::super::analyze(&fixture::captured_owned_fixed(true), &limits).is_err());
        for mode in 0..5 {
            let mut raw = m.clone();
            let entry = &mut raw.functions[0];
            match mode {
                0 => {
                    let mut take = entry.blocks[0].instructions[0].clone();
                    let id = ValueId(entry.values.len() as u32);
                    entry.values.push(entry.values[7].clone());
                    take.result = Some(id);
                    entry.blocks[0].instructions.insert(1, take);
                }
                1 => {
                    entry.blocks[0].instructions[0].kind = InstructionKind::ReadCapture { index: 0 }
                }
                2 => {
                    let code = &mut entry.blocks[0].instructions;
                    let at = code
                        .iter()
                        .position(|i| matches!(i.kind, InstructionKind::DropManaged { .. }))
                        .unwrap();
                    let drop = code.remove(at);
                    let read = code
                        .iter()
                        .position(|i| matches!(i.kind, InstructionKind::ReadDesc { .. }))
                        .unwrap();
                    code.insert(read, drop);
                }
                3 => {
                    let maker = raw.functions[1].blocks[0]
                        .instructions
                        .iter_mut()
                        .find(|i| matches!(i.kind, InstructionKind::MakeManaged { .. }))
                        .unwrap();
                    maker.kind = InstructionKind::MakeManaged { fields: vec![] };
                }
                _ => {
                    let read = entry.blocks[0]
                        .instructions
                        .iter_mut()
                        .find(|i| matches!(i.kind, InstructionKind::ReadDesc { .. }))
                        .unwrap();
                    if let InstructionKind::ReadDesc { index, .. } = &mut read.kind {
                        *index = 9;
                    }
                }
            }
            assert!(
                super::super::analyze(&raw, &limits).is_err(),
                "owned neighbor {mode}"
            );
        }
    }
    #[test]
    fn owned_fixed_once_has_one_exact_fresh_and_warm_request_budget() {
        let m = fixture::captured_owned_fixed(false);
        let limits = |n| Limits {
            max_analysis_steps: n,
            ..Limits::default()
        };
        let expected = super::super::analyze(&m, &Limits::default()).unwrap();
        let (mut low, mut high) = (1, Limits::default().max_analysis_steps);
        while low < high {
            let mid = low + (high - low) / 2;
            if super::super::analyze(&m, &limits(mid)).is_ok() {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        assert_eq!(
            // Measured on the unchanged raw fixture after the paid path
            // index, word liveness, fact copy and per-instruction access work.
            // Includes word/set-bit scans, every Permission projection and
            // every empty outer-frame visit on the unchanged raw fixture.
            low,
            // Six scalar Managed TakeCapture sites keep the existing owning
            // ledger (48 fewer steps), and addressed captures select current
            // memory once instead of also joining stale SSA slots (180 fewer).
            12_246,
            "bounded actual identity/ownership work changed"
        );
        assert_eq!(
            super::super::analyze(&m, &limits(14_077)).unwrap(),
            expected,
            "the prior allowance remains a complete-plan success control"
        );
        let mut session = super::super::AnalysisSession::default();
        for warm in [false, true] {
            assert_eq!(session.analyze(&m, &limits(low)).unwrap(), expected);
            if warm {
                assert!(session.last_stats().reused_functions > 0);
            }
            let error = session.analyze(&m, &limits(low - 1)).unwrap_err();
            assert!(
                error.iter().any(|d| d.code == "E-RESOURCE-LIMIT"),
                "{error:?}"
            );
            assert_eq!(super::super::analyze(&m, &limits(low)).unwrap(), expected);
        }
        println!("owned Fixed Analyze N={low}; N-1/fresh/warm identical");
    }
    #[test]
    fn owned_fixed_heads_require_actual_resource_definitions() {
        let m = fixture::captured_owned_fixed(false);
        let limits = Limits::default();
        let mut accepted = Vec::new();
        for mode in 0..7 {
            let mut raw = m.clone();
            match mode {
                0 => {
                    raw.resources[0].name = "Other".into();
                    let Type::Record { name, .. } = &mut raw.resources[0].payload else {
                        unreachable!()
                    };
                    *name = "Other".into();
                }
                1 => raw.resources.clear(),
                2 => raw.resources.push(raw.resources[0].clone()),
                3 => {
                    let mut local = raw.functions[1].locals[0].clone();
                    local.id = LocalId(raw.functions[1].locals.len() as u32);
                    local.name = "unused_other".into();
                    local.ty = Type::Managed(Box::new(Type::Record {
                        name: "Other".into(),
                        fields: vec![("n".into(), Type::I32)],
                    }));
                    raw.functions[1].locals.push(local);
                }
                4 => {
                    let Type::Record { fields, .. } = &mut raw.resources[0].payload else {
                        unreachable!()
                    };
                    fields[0].1 = Type::I64;
                }
                5 => raw.resources[0].name = "Other".into(),
                _ => raw.resources[0].teardown = Some(raw.functions[0].id),
            }
            let result = super::super::analyze(&raw, &limits);
            eprintln!(
                "resource identity neighbor {mode}: {:?}",
                result.as_ref().err()
            );
            if result.is_ok() {
                accepted.push(mode);
            }
        }
        assert!(
            accepted.is_empty(),
            "unbound resource heads accepted: {accepted:?}"
        );
        let mut renamed = m.clone();
        renamed.resources[0].name = "Renamed".into();
        if let Type::Record { name, .. } = &mut renamed.resources[0].payload {
            *name = "Renamed".into();
        }
        for ty in renamed
            .functions
            .iter_mut()
            .flat_map(|f| f.locals.iter_mut().map(|l| &mut l.ty).chain(&mut f.values))
            .chain(renamed.closures.iter_mut().flat_map(|d| &mut d.captures))
        {
            if let Type::Managed(payload) | Type::Desc(payload) = ty {
                let Type::Record { name, .. } = payload.as_mut() else {
                    unreachable!()
                };
                *name = "Renamed".into();
            }
        }
        super::super::analyze(&renamed, &limits).unwrap();
    }
    use fixture::{address, instruction, module};
    fn prove(module: &TypedModule) -> CompileResult<Vec<Facts>> {
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        inventory(module, &limits, &mut b)?;
        for f in &module.functions {
            header(module, f, &mut b)?;
        }
        derive(
            module,
            &vec![(vec![vec![]], vec![true]); module.functions.len()],
            &mut b,
        )
    }
    #[test]
    fn captured_fixed_helper_payload_naturally_projects_parameter_roots() {
        for mode in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
            let m = fixture::captured_fixed_helper(mode, false);
            let limits = Limits::default();
            let plan = super::super::analyze(&m, &limits).unwrap();
            assert_eq!(
                plan.functions[2].holds[0].roots,
                vec![HoldRoot::Parameter(0)],
                "{mode:?}"
            );
            let mut b = Budget::new(&limits);
            let helper = &m.functions[2];
            let mut loans = vec![Loan::default(); helper.values.len()];
            let entry = seed(&m, helper, &mut loans, &mut b).unwrap();
            let actual = &entry.locals[&LocalId(0)];
            assert_eq!(
                actual.environment.len(),
                2,
                "outer Address and its distinct physical field target"
            );
            assert_eq!(actual.held.loans.len(), 2);
            for (path, permission) in &actual.environment {
                assert!(matches!(
                    path.as_slice(),
                    [PlaceProjection::Field(0)]
                        | [PlaceProjection::Field(0), PlaceProjection::Field(0)]
                ));
                for target in &permission.targets {
                    assert_eq!(target.base, HoldRoot::Parameter(0));
                    assert_eq!(&target.stored, path);
                }
                for loan in &permission.loans {
                    assert_eq!(loans[*loan].targets, permission.targets);
                    assert!(!loans[*loan].mutable);
                }
            }
        }
    }
    #[test]
    fn captured_fixed_helper_substitutes_actual_local_capture_targets() {
        // The independent review's neutral actual-IR constructor: a real
        // local scalar is retained through a Tag and &mut Closure helper.
        let mut m = fixture::captured_fixed_helper(CallableKind::FnMut, false);
        m.functions[1] = fixture::captured_fixed_parent_end(false).functions[1].clone();
        let f = &mut m.functions[1];
        f.locals[2].mutable = true;
        f.values.push(address(Type::Closure(ClosureId(0)), true));
        let code = &mut f.blocks[0].instructions;
        code.pop();
        code.push(instruction(
            Some(6),
            InstructionKind::BorrowLocalMut { local: LocalId(2) },
        ));
        code.push(instruction(
            Some(5),
            InstructionKind::Call {
                function: FunctionId(2),
                arguments: vec![ValueId(6), ValueId(3)],
            },
        ));
        super::super::analyze(&m, &Limits::default()).unwrap();
        let at = m.functions[1].blocks[0].instructions.len() - 1;
        m.functions[1].blocks[0].instructions.insert(
            at,
            instruction(None, InstructionKind::StorageDead(LocalId(0))),
        );
        let error = super::super::analyze(&m, &Limits::default()).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.contains("actual stored target/parent loan")),
            "{error:?}"
        );
    }
    #[test]
    fn captured_fixed_phantom_needs_actual_header_origin() {
        for header in [true, false] {
            let mut m = if header {
                fixture::captured_fixed(CallableKind::Fn, false)
            } else {
                fixture::captured_fixed_parent_end(false)
            };
            let mut phantom = m.life_instances[0].ty.clone();
            let Type::BorrowedStorage { storage, life } = &mut phantom else {
                unreachable!()
            };
            **storage = Type::Record {
                name: "Phantom".into(),
                fields: vec![("n".into(), Type::I64)],
            };
            life.slots.clear();
            m.life_instances[0].ty = phantom.clone();
            let ty = address(phantom.clone(), false);
            m.closures[0].captures = vec![ty.clone()];
            m.closures[0].parameters = vec![ty.clone()];
            let entry = &mut m.functions[0];
            entry.locals[0].ty = ty.clone();
            entry.values = vec![ty.clone(), Type::I64];
            entry.blocks[0].instructions = vec![
                instruction(Some(0), InstructionKind::ReadCapture { index: 0 }),
                instruction(Some(1), InstructionKind::Constant(Constant::I64(42))),
            ];
            entry.blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
            let caller = &mut m.functions[1];
            if header {
                caller.locals[..2]
                    .iter_mut()
                    .for_each(|l| l.ty = ty.clone());
                caller.values[..2].fill(ty);
            } else {
                caller.locals[1].ty = phantom.clone();
                caller.values[2] = phantom;
                caller.values[3] = ty;
                let InstructionKind::MakeAggregate { fields } =
                    &mut caller.blocks[0].instructions[3].kind
                else {
                    unreachable!()
                };
                fields[0] = ValueId(0); // scalar field does not ground its nominal class
            }
            let result = super::super::analyze(&m, &Limits::default());
            if header {
                result.unwrap();
            } else {
                let error = result.unwrap_err();
                assert!(
                    error
                        .iter()
                        .any(|d| d.message.contains("no actual header or Address origin")),
                    "{error:?}"
                );
            }
        }
    }
    #[test]
    fn captured_fixed_ordinary_address_needs_no_nominal_trigger() {
        for wrong in [false, true] {
            let mut m = fixture::captured_fixed(CallableKind::Fn, wrong);
            let ty = address(Type::I64, false);
            let contract = StorageLifeContract {
                fixed_binder_count: 1,
                invocation_binder_count: 0,
                bindings: vec![
                    StorageLifeBinding {
                        position: StoragePosition {
                            base: StorageBase::Parameter(0),
                            path: vec![],
                        },
                        binder: StorageBinder::Fixed(0),
                    },
                    StorageLifeBinding {
                        position: StoragePosition {
                            base: StorageBase::Capture(0),
                            path: vec![],
                        },
                        binder: StorageBinder::Fixed(0),
                    },
                ],
                classes: vec![],
            };
            m.life_declarations.clear();
            m.life_instances.clear();
            m.closures[0].captures = vec![ty.clone()];
            m.closures[0].parameters = vec![ty.clone()];
            m.closures[0].storage_life = Some(contract.clone());
            let entry = &mut m.functions[0];
            entry.locals[0].ty = ty.clone();
            entry.storage_life = Some(contract);
            entry.values = vec![ty.clone(), Type::I64, ty.clone(), Type::I64, Type::I64];
            entry.blocks[0].instructions = vec![
                instruction(Some(0), InstructionKind::ReadCapture { index: 0 }),
                instruction(
                    Some(1),
                    InstructionKind::ReadAddress {
                        address: ValueId(0),
                    },
                ),
                instruction(Some(2), InstructionKind::Load(LocalId(0))),
                instruction(
                    Some(3),
                    InstructionKind::ReadAddress {
                        address: ValueId(2),
                    },
                ),
                instruction(
                    Some(4),
                    InstructionKind::Binary {
                        op: BinaryOp::Add,
                        left: ValueId(1),
                        right: ValueId(3),
                    },
                ),
            ];
            entry.blocks[0].terminator = Terminator::Return(Some(ValueId(4)));
            let caller = &mut m.functions[1];
            for local in &mut caller.locals[..2] {
                local.ty = ty.clone();
            }
            caller.values[..2].fill(ty);
            let header = caller.storage_life.as_mut().unwrap();
            header.fixed_binder_count = 0;
            header.classes.clear();
            let result = super::super::analyze(&m, &Limits::default());
            if wrong {
                let error = result.unwrap_err();
                assert!(
                    error
                        .iter()
                        .any(|d| d.message.contains("actual retained classes")),
                    "{error:?}"
                );
            } else {
                result.unwrap();
            }
        }
    }
    #[test]
    fn captured_fixed_once_move_keeps_body_parameter_classes() {
        let mut m = fixture::captured_fixed_helper(CallableKind::FnOnce, false);
        let helper = &mut m.functions[2];
        let ty = helper.locals[1].ty.clone();
        let mut local = helper.locals[1].clone();
        local.id = LocalId(2);
        helper.locals.push(local);
        helper.parameters.push(LocalId(2));
        helper.values.push(ty);
        helper.blocks[0]
            .instructions
            .insert(2, instruction(Some(3), InstructionKind::Load(LocalId(2))));
        let InstructionKind::CallClosureOnce { arguments, .. } =
            &mut helper.blocks[0].instructions[3].kind
        else {
            unreachable!()
        };
        arguments[0] = ValueId(3);
        let header = helper.storage_life.as_mut().unwrap();
        header.fixed_binder_count = 3;
        header.invocation_binder_count = 2;
        header.bindings.push(StorageLifeBinding {
            position: StoragePosition {
                base: StorageBase::Parameter(2),
                path: vec![],
            },
            binder: StorageBinder::Invocation(1),
        });
        header.classes.push(SignatureLifeClassBinding {
            position: SignatureLifeClass {
                base: StorageBase::Parameter(2),
                path: vec![LifeTypeProjection::Pointee],
                class: 0,
            },
            binder: StorageBinder::Fixed(2),
        });
        let InstructionKind::Call { arguments, .. } = &mut m.functions[1].blocks[0]
            .instructions
            .last_mut()
            .unwrap()
            .kind
        else {
            unreachable!()
        };
        arguments.push(ValueId(1));
        let error = super::super::analyze(&m, &Limits::default()).unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.contains("actual retained classes")),
            "{error:?}"
        );
    }
    #[test]
    fn captured_fixed_persists_while_invocation_is_fresh() {
        for mode in [CallableKind::Fn, CallableKind::FnMut] {
            let mut m = fixture::captured_fixed(mode, false);
            let caller = &mut m.functions[1];
            let header = caller.storage_life.as_mut().unwrap();
            header.fixed_binder_count = 1;
            header.classes[1].binder = StorageBinder::Fixed(0);
            caller.values.push(Type::I64);
            caller.blocks[0].instructions.push(instruction(
                Some(4),
                if mode == CallableKind::Fn {
                    InstructionKind::CallClosureShared {
                        callee: LocalId(2),
                        arguments: vec![ValueId(1)],
                    }
                } else {
                    InstructionKind::CallClosureMut {
                        callee: LocalId(2),
                        arguments: vec![ValueId(1)],
                    }
                },
            ));
            caller.blocks[0].terminator = Terminator::Return(Some(ValueId(4)));
            super::super::analyze(&m, &Limits::default()).unwrap();
            // Both independent outer input borrows are used with the same
            // retained Tag class. Reusing Invocation would incorrectly reject.
        }
    }
    #[test]
    fn captured_fixed_helper_cannot_choose_context_from_its_claim() {
        for missing in [true, false] {
            let mut m = fixture::captured_fixed_helper(CallableKind::Fn, false);
            if missing {
                m.functions[2].storage_life = None;
            } else {
                let life = m.functions[2].callable_contract.as_mut().unwrap().roles[0]
                    .life
                    .as_mut()
                    .unwrap();
                life.invocation_binder_count = 2;
                life.classes[0].binder = StorageBinder::Invocation(1);
                life.bindings[1].binder = StorageBinder::Fixed(0);
                life.classes[1].binder = StorageBinder::Fixed(1);
            }
            let error = super::super::analyze(&m, &Limits::default()).unwrap_err();
            assert!(
                error.iter().all(|d| d.code != "E-RESOURCE-LIMIT"),
                "{error:?}"
            );
            println!(
                "missing/mismatched helper context {missing}: {}",
                error[0].message
            );
        }
    }
    #[test]
    fn captured_fixed_relations_and_consumption_are_not_permissions() {
        let base = fixture::captured_fixed(CallableKind::FnOnce, false);
        for mutation in 0..8 {
            let mut m = base.clone();
            match mutation {
                0 => {
                    m.closures[0].storage_life.as_mut().unwrap().bindings[1].binder =
                        StorageBinder::Invocation(0);
                    m.functions[0].storage_life = m.closures[0].storage_life.clone();
                }
                1 => {
                    m.closures[0].storage_life.as_mut().unwrap().classes[1]
                        .position
                        .path
                        .clear();
                    m.functions[0].storage_life = m.closures[0].storage_life.clone();
                }
                2 => {
                    m.functions[0].storage_life.as_mut().unwrap().classes[1].binder =
                        StorageBinder::Fixed(1);
                }
                3 => {
                    m.functions[1].values.push(Type::I64);
                    m.functions[1].blocks[0].instructions.push(instruction(
                        Some(4),
                        InstructionKind::CallClosureOnce {
                            callee: ValueId(2),
                            arguments: vec![ValueId(0)],
                        },
                    ));
                }
                4 => {
                    m.functions[1].values.push(Type::Closure(ClosureId(0)));
                    let code = &mut m.functions[1].blocks[0].instructions;
                    code.insert(
                        3,
                        instruction(
                            None,
                            InstructionKind::Store {
                                local: LocalId(2),
                                value: ValueId(2),
                            },
                        ),
                    );
                    code.insert(4, instruction(Some(4), InstructionKind::Load(LocalId(2))));
                }
                5 => {
                    m.closures[0].kind = CallableKind::FnMut;
                }
                6 => {
                    m.functions[1].return_type = Type::Closure(ClosureId(0));
                    m.functions[1].blocks[0].instructions.pop();
                    m.functions[1].blocks[0].terminator = Terminator::Return(Some(ValueId(2)));
                }
                _ => m.functions[1].values.push(Type::I64),
            }
            if mutation == 5 {
                // Finite nonlending FnMut-to-Once adaptation consumes this
                // actual environment under the independent owner ledger.
                super::super::analyze(&m, &Limits::default()).unwrap();
                continue;
            }
            let error = super::super::analyze(&m, &Limits::default()).unwrap_err();
            assert!(
                error.iter().all(|d| d.code != "E-RESOURCE-LIMIT"),
                "mutation {mutation}: {error:?}"
            );
            if mutation == 6 {
                assert!(
                    error
                        .iter()
                        .any(|d| d.message.contains("retained result target transfer")),
                    "{error:?}"
                );
            }
            println!(
                "malformed/consume neighbor {mutation}: {}",
                error[0].message
            );
        }
    }
    #[test]
    fn captured_fixed_fnmut_writes_its_actual_inline_scalar_capture() {
        let mut m = fixture::captured_fixed(CallableKind::FnMut, false);
        m.closures[0].captures.push(Type::I64);
        m.functions[0].blocks[0].instructions.push(instruction(
            None,
            InstructionKind::WriteCapture {
                index: 1,
                value: ValueId(6),
            },
        ));
        let caller = &mut m.functions[1];
        caller.values.push(Type::I64);
        caller.blocks[0].instructions.insert(
            2,
            instruction(Some(4), InstructionKind::Constant(Constant::I64(0))),
        );
        let InstructionKind::MakeClosure { captures, .. } =
            &mut caller.blocks[0].instructions[3].kind
        else {
            unreachable!()
        };
        captures.push(ValueId(4));
        super::super::analyze(&m, &Limits::default()).unwrap();
        m.closures[0].kind = CallableKind::Fn;
        let error = super::super::analyze(&m, &Limits::default()).unwrap_err();
        assert!(
            error.iter().any(|d| d.message.contains("capture write")),
            "{error:?}"
        );
    }
    #[test]
    fn captured_fixed_context_uses_one_request_budget() {
        for mode in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
            let m = fixture::captured_fixed_helper(mode, false);
            let limit = |n| Limits {
                max_analysis_steps: n,
                ..Limits::default()
            };
            let expected = super::super::analyze(&m, &Limits::default()).unwrap();
            let mut low = 1;
            let mut high = Limits::default().max_analysis_steps;
            while low < high {
                let mid = low + (high - low) / 2;
                if super::super::analyze(&m, &limit(mid)).is_ok() {
                    high = mid;
                } else {
                    low = mid + 1;
                }
            }
            let error = super::super::analyze(&m, &limit(low - 1)).unwrap_err();
            assert!(
                error.iter().any(|d| d.code == "E-RESOURCE-LIMIT"),
                "{error:?}"
            );
            assert_eq!(super::super::analyze(&m, &limit(low)).unwrap(), expected);
            assert_eq!(super::super::analyze(&m, &limit(low)).unwrap(), expected);
            println!(
                "{mode:?} full Typed helper Analyze N={low}; N-1 ResourceLimit/fresh N identical"
            );
        }
    }
    #[test]
    fn captured_fixed_instances_and_helpers_keep_receiver_context() {
        for mode in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
            for make in [
                fixture::captured_fixed_instances,
                fixture::captured_fixed_helper,
            ] {
                super::super::analyze(&make(mode, false), &Limits::default()).unwrap();
                let error =
                    super::super::analyze(&make(mode, true), &Limits::default()).unwrap_err();
                assert!(
                    error
                        .iter()
                        .any(|d| d.message.contains("actual retained classes")),
                    "{mode:?}: {error:?}"
                );
            }
        }
    }
    #[test]
    fn captured_fixed_keeps_actual_parent_alive() {
        super::super::analyze(
            &fixture::captured_fixed_parent_end(false),
            &Limits::default(),
        )
        .unwrap();
        let error = super::super::analyze(
            &fixture::captured_fixed_parent_end(true),
            &Limits::default(),
        )
        .unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.contains("live access overlaps")),
            "{error:?}"
        );
    }
    #[test]
    fn captured_fixed_uses_actual_environment_in_all_three_modes() {
        for mode in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
            super::super::analyze(&fixture::captured_fixed(mode, false), &Limits::default())
                .unwrap();
            let error =
                super::super::analyze(&fixture::captured_fixed(mode, true), &Limits::default())
                    .unwrap_err();
            assert!(
                error
                    .iter()
                    .any(|d| d.message.contains("actual retained classes")),
                "{mode:?}: {error:?}"
            );
        }
    }
    #[test]
    fn qualified_role_proofs_share_the_actual_request_allowance() {
        let m = fixture::qualified_two_fn_roles(false);
        let limit = |n| Limits {
            max_analysis_steps: n,
            ..Limits::default()
        };
        let expected = super::super::analyze(&m, &Limits::default()).unwrap();
        let mut low = 0;
        let mut high = Limits::default().max_analysis_steps;
        while low < high {
            let mid = low + (high - low) / 2;
            if super::super::analyze(&m, &limit(mid)).is_ok() {
                high = mid
            } else {
                low = mid + 1
            }
        }
        let error = super::super::analyze(&m, &limit(low - 1)).unwrap_err();
        assert!(
            error.iter().any(|d| d.code == "E-RESOURCE-LIMIT"),
            "{error:?}"
        );
        assert_eq!(super::super::analyze(&m, &limit(low)).unwrap(), expected);
        assert_eq!(super::super::analyze(&m, &limit(low)).unwrap(), expected);
        println!("342 qualified Fn role Analyze N={low}; N-1 ResourceLimit/fresh N identical");
    }
    #[test]
    fn qualified_callback_arguments_keep_simultaneous_exclusive_windows() {
        super::super::analyze(
            &fixture::qualified_fn_exclusive_call(false),
            &Limits::default(),
        )
        .unwrap();
        let error = super::super::analyze(
            &fixture::qualified_fn_exclusive_call(true),
            &Limits::default(),
        )
        .unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.contains("live access overlaps")),
            "{error:?}"
        );
    }
    #[test]
    fn the_same_closure_is_checked_under_each_actual_parameter_role() {
        super::super::analyze(&fixture::qualified_two_fn_roles(false), &Limits::default()).unwrap();
        let error =
            super::super::analyze(&fixture::qualified_two_fn_roles(true), &Limits::default())
                .unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.contains("actual retained classes")),
            "{error:?}"
        );
    }
    #[test]
    fn qualified_fn_roles_use_actual_entry_and_fresh_argument_class_equations() {
        super::super::analyze(
            &fixture::qualified_fn_role(false, false),
            &Limits::default(),
        )
        .unwrap();
        for m in [
            fixture::qualified_fn_role(true, false),
            fixture::qualified_fn_role(false, true),
        ] {
            let error = super::super::analyze(&m, &Limits::default()).unwrap_err();
            assert!(
                error
                    .iter()
                    .any(|d| d.message.contains("actual retained classes")),
                "{error:?}"
            );
        }
    }
    #[test]
    fn a_pointer_fixed_class_without_enclosing_context_is_not_fresh_per_call() {
        super::super::analyze(
            &fixture::phantom_class_calls(true, false),
            &Limits::default(),
        )
        .unwrap();
        let error = super::super::analyze(&fixture::unbound_fixed_pointer(), &Limits::default())
            .unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.contains("genuine enclosing semantic context")),
            "{error:?}"
        );
    }
    #[test]
    fn rich_indirect_actual_exclusive_windows_are_simultaneous() {
        super::super::analyze(
            &fixture::rich_marker_exclusive_call(false),
            &Limits::default(),
        )
        .unwrap();
        let error = super::super::analyze(
            &fixture::rich_marker_exclusive_call(true),
            &Limits::default(),
        )
        .unwrap_err();
        assert!(
            error
                .iter()
                .any(|d| d.message.contains("live access overlaps")),
            "{error:?}"
        );
    }
    #[test]
    fn rich_class_equations_share_the_request_budget_and_fresh_boundary() {
        let m = fixture::rich_marker_pointer(false, false);
        let limit = |n| Limits {
            max_analysis_steps: n,
            ..Limits::default()
        };
        let expected = super::super::analyze(&m, &Limits::default()).unwrap();
        let mut low = 0;
        let mut high = Limits::default().max_analysis_steps;
        while low < high {
            let mid = low + (high - low) / 2;
            if super::super::analyze(&m, &limit(mid)).is_ok() {
                high = mid
            } else {
                low = mid + 1
            }
        }
        let error = super::super::analyze(&m, &limit(low - 1)).unwrap_err();
        assert!(
            error.iter().any(|d| d.code == "E-RESOURCE-LIMIT"),
            "{error:?}"
        );
        assert_eq!(super::super::analyze(&m, &limit(low)).unwrap(), expected);
        assert_eq!(super::super::analyze(&m, &limit(low)).unwrap(), expected);
        println!("342 rich class Analyze N={low}; N-1 ResourceLimit/fresh N identical");
    }
    #[test]
    fn rich_pointer_proves_target_implication_and_actual_outer_to_class_equations() {
        super::super::analyze(
            &fixture::rich_marker_pointer(false, false),
            &Limits::default(),
        )
        .unwrap();
        let bad = super::super::analyze(
            &fixture::rich_marker_pointer(true, false),
            &Limits::default(),
        )
        .unwrap_err();
        assert!(
            bad.iter()
                .any(|d| d.message.contains("actual retained classes")),
            "{bad:?}"
        );
        let bad = super::super::analyze(
            &fixture::rich_marker_pointer(false, true),
            &Limits::default(),
        )
        .unwrap_err();
        assert!(
            bad.iter()
                .any(|d| d.message.contains("entail actual target")),
            "{bad:?}"
        );
    }
    #[test]
    fn a_fresh_phantom_class_is_instantiated_once_and_retained_between_actual_calls() {
        for indirect in [false, true] {
            super::super::analyze(
                &fixture::phantom_class_calls(indirect, false),
                &Limits::default(),
            )
            .unwrap();
            let error = super::super::analyze(
                &fixture::phantom_class_calls(indirect, true),
                &Limits::default(),
            )
            .unwrap_err();
            assert!(
                error
                    .iter()
                    .any(|d| d.message.contains("actual retained classes")),
                "{error:?}"
            );
        }
    }
    #[test]
    fn actual_shared_field_survives_carrier_end_and_two_forwarding_helpers() {
        let m = module();
        let facts = prove(&m).unwrap();
        assert_eq!(
            facts[3].values[4],
            vec![Target {
                retained_port: None,
                base: HoldRoot::Local(LocalId(0)),
                stored: vec![],
                place: vec![],
                heap_parents: Vec::new(),
                heap_element: None,
            }]
        );
        super::super::analyze(&m, &Limits::default()).unwrap();
    }
    #[test]
    fn copied_field_keeps_target_parent_loan_after_carrier_end() {
        let mut m = module();
        let f = &mut m.functions[3];
        f.values.push(address(Type::I64, true));
        f.blocks[0].instructions.insert(
            8,
            instruction(
                Some(6),
                InstructionKind::BorrowLocalMut { local: LocalId(0) },
            ),
        );
        assert!(prove(&m).is_err());
    }
    #[test]
    fn unresolved_recursive_result_is_not_an_empty_authority() {
        let mut m = module();
        let f = &mut m.functions[4];
        f.values.push(address(Type::I64, false));
        f.blocks[0].instructions.push(instruction(
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
        let positive = fixture::exclusive_returns(false);
        super::super::analyze(&positive, &Limits::default()).unwrap();
        let bad = fixture::exclusive_returns(true);
        let error = super::super::analyze(&bad, &Limits::default()).unwrap_err();
        assert!(
            error[0].message.contains("live access overlaps"),
            "{error:?}"
        );
    }
}
