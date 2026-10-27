//! Independent finite resource responsibility and description checking.
//!
//! Descriptions carry only a source position, never ownership or an address
//! loan. Every resource operation is reconstructed from the checked CFG; the
//! candidate's responsibility fields are not trusted or consulted.
use super::*;
use std::collections::VecDeque;
pub(crate) mod arrays;

const EMPTY: u8 = 1;
const LIVE: u8 = 2;
const MOVED: u8 = 4;
const DONE: u8 = 8;
const FRESH: u8 = 1;
const INPUT: u8 = 2;

fn discharged(debt: u8) -> u8 {
    (debt & !LIVE) | if debt & LIVE != 0 { DONE } else { 0 }
}

fn bad(span: Span, message: impl Into<String>) -> Diagnostic {
    reject("E-VERIFY-RESOURCE", Some(span), message)
}

pub(super) fn is_teardown(
    module: &TypedModule,
    function: FunctionId,
    budget: &mut Budget,
) -> Check<bool> {
    budget.spend(module.resources.len())?;
    Ok(module
        .resources
        .iter()
        .any(|r| r.teardown == Some(function)))
}

// All raw type trees have already passed check_type_shape. Charge the repeated
// payload admission and equality work as well; the first shape pass cannot pay
// for every later use of a nominal resource type.
pub(super) fn charge_payload(ty: &Type, budget: &mut Budget) -> Check<()> {
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        budget.spend(2)?;
        match ty {
            Type::Array { element, .. }
            | Type::Address { inner: element, .. }
            | Type::Managed(element)
            | Type::Desc(element) => pending.push(element),
            Type::ErrorUnion { error, result } => {
                budget.spend(2)?;
                pending.push(error);
                pending.push(result);
            }
            Type::Tuple(fields) => {
                budget.spend(fields.len().saturating_mul(2))?;
                pending.extend(fields);
            }
            Type::Record { name, fields } | Type::Package { name, fields } => {
                budget.spend(name.len().saturating_add(fields.len()).saturating_mul(2))?;
                for (name, ty) in fields {
                    budget.spend(name.len().saturating_mul(2))?;
                    pending.push(ty);
                }
            }
            Type::Enum { name, variants } => {
                budget.spend(name.len().saturating_add(variants.len()).saturating_mul(2))?;
                for (name, fields) in variants {
                    budget.spend(name.len().saturating_add(fields.len()).saturating_mul(2))?;
                    pending.extend(fields);
                }
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                budget.spend(parameters.len().saturating_mul(2))?;
                pending.extend(parameters);
                pending.push(result);
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn check_type_for(
    module: &TypedModule,
    ty: &Type,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<()> {
    let (Type::Managed(inner) | Type::Desc(inner)) = ty else {
        return Err(unsupported(
            "expected a finite resource carrier or description",
            span,
        ));
    };
    if inner.as_ref() == &Type::ByteBuffer {
        return byte_buffers::check_carrier(module, ty, span, budget);
    }
    let Type::Record { name, .. } = inner.as_ref() else {
        return Err(unsupported(
            "resource payload must be a finite Copy record",
            span,
        ));
    };
    charge_payload(inner, budget)?;
    check_type(inner, span)?;
    // Equal-length string comparison examines at most name.len() bytes. The
    // table has unique names, so at most one full payload comparison follows.
    budget.spend(
        module
            .resources
            .len()
            .saturating_mul(name.len().saturating_add(1)),
    )?;
    if !module
        .resources
        .iter()
        .any(|r| &r.name == name && &r.payload == inner.as_ref())
    {
        return Err(reject(
            "E-VERIFY-RESOURCE",
            span,
            "resource type is absent from its nominal definition table",
        ));
    }
    Ok(())
}

pub(super) fn check_definitions(
    module: &TypedModule,
    limits: &Limits,
    budget: &mut Budget,
) -> Check<()> {
    if module.resources.len() > limits.max_nodes {
        return Err(limit(
            "resource definition count exceeds the selected bound",
        ));
    }
    budget.spend(module.resources.len())?;
    let mut names = BTreeSet::new();
    let mut entries = BTreeSet::new();
    for resource in &module.resources {
        budget.spend(resource.name.len().saturating_add(1))?;
        check_type_shape(&resource.payload, &module.target, limits, budget)?;
        if resource.payload == Type::ByteBuffer || resource.name == byte_buffers::NAME {
            byte_buffers::target(&module.target, None)?;
            if resource.name != byte_buffers::NAME
                || resource.payload != Type::ByteBuffer
                || resource.teardown.is_some()
                || !names.insert(&resource.name)
            {
                return Err(reject(
                    "E-VERIFY-BYTE-BUFFER",
                    None,
                    "byte storage definition must be unique, sealed and have no user teardown",
                ));
            }
            continue;
        }
        check_type(&resource.payload, None)?;
        if !matches!(&resource.payload, Type::Record { name, .. } if name == &resource.name)
            || !names.insert(&resource.name)
        {
            return Err(reject(
                "E-VERIFY-RESOURCE",
                None,
                "resource definitions require unique matching nominal Copy record payloads",
            ));
        }
        if let Some(id) = resource.teardown {
            let f = module.functions.get(id.0 as usize).ok_or_else(|| {
                reject(
                    "E-VERIFY-RESOURCE",
                    None,
                    "resource teardown function is absent",
                )
            })?;
            if !entries.insert(id)
                || f.foreign.is_some()
                || f.environment.is_some()
                || f.return_type != Type::Unit
                || f.result_mode != PassingMode::Infer
                || f.parameters.len() != 1
                || local_type(f, f.parameters[0], Some(f.span))?
                    != &Type::Desc(Box::new(resource.payload.clone()))
            {
                return Err(bad(
                    f.span,
                    "teardown requires one matching Desc parameter and an ordinary unit body",
                ));
            }
        }
    }
    budget.spend(module.functions.len())?;
    for f in &module.functions {
        match (&f.return_type, f.result_mode) {
            (Type::Managed(_), PassingMode::Create | PassingMode::Give) => {}
            (Type::Desc(_), _) => {
                return Err(unsupported(
                    "resource description returns require exported source and management contracts",
                    Some(f.span),
                ));
            }
            (Type::Managed(_), _) => {
                return Err(bad(
                    f.span,
                    "managed result requires an explicit create or give result mode",
                ));
            }
            (_, PassingMode::Infer) => {}
            _ => {
                return Err(bad(
                    f.span,
                    "non-resource result must not claim a resource result mode",
                ));
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Root {
    Local(LocalId),
    // Field identity is exact even while its package shell is only partly
    // populated. A later instance at the same physical slot cannot revive an
    // earlier live description across a Take/Drop/Store transition.
    PackageField { local: LocalId, index: u32 },
    ArrayElement { local: LocalId, index: u32 },
    Parameter(u32),
    OwnedCapture(u32),
    BorrowedCapture(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct State {
    debt: Vec<u8>,
    fresh: Vec<u8>,
    desc: Vec<BTreeSet<Root>>,
    // An edge a -> b means b may have been initialized later while a remained live.
    newer: Vec<BTreeSet<usize>>,
}

struct Slots {
    managed_local: Vec<Option<usize>>,
    managed_value: Vec<Option<usize>>,
    // Package shells participate in local cleanup order; their individual
    // fields do not. Field debts follow all SSA slots and precede captures so
    // they still participate in the ordinary-return conservation check.
    package_fields: Vec<Vec<Option<usize>>>,
    desc_local: Vec<Option<usize>>,
    desc_value: Vec<Option<usize>>,
    capture_debt: Vec<Option<usize>>,
    package_view: Vec<Option<u32>>,
    package_capture_local: Vec<Option<LocalId>>,
    // Package views alias capture responsibility, although their reusable field
    // instructions address local slots. These debts are never ordinary locals.
    retained_debt: Vec<bool>,
    capture_start: usize,
    local_count: usize,
    debt_count: usize,
    desc_count: usize,
}

impl Slots {
    fn new(
        module: &TypedModule,
        f: &TypedFunction,
        views: &packages::CaptureViews,
        budget: &mut Budget,
    ) -> Check<Self> {
        budget.spend(
            f.locals
                .len()
                .saturating_add(f.values.len())
                .saturating_mul(2),
        )?;
        budget.spend(module.closures.len().saturating_mul(2))?;
        let mut owned_closures = Vec::with_capacity(module.closures.len());
        let mut borrowed_closures = Vec::with_capacity(module.closures.len());
        for closure in &module.closures {
            budget.spend(closure.captures.len().saturating_mul(2))?;
            owned_closures.push(
                closure.kind == CallableKind::FnOnce
                    || closure
                        .captures
                        .iter()
                        .any(|ty| matches!(ty, Type::Managed(_) | Type::Package { .. })),
            );
            borrowed_closures.push(
                closure
                    .captures
                    .iter()
                    .any(|ty| matches!(ty, Type::Desc(_))),
            );
        }
        let mut debt_count = 0usize;
        let mut desc_count = 0usize;
        let mut slot = |ty: &Type, managed: bool| {
            let count = if managed {
                &mut debt_count
            } else {
                &mut desc_count
            };
            if matches!(
                (ty, managed),
                (Type::Managed(_) | Type::Package { .. }, true) | (Type::Desc(_), false)
            ) || (managed && errors::owned(ty))
                || (managed
                    && matches!(ty, Type::Array { element, length }
                    if *length > 0 && matches!(element.as_ref(), Type::Package { .. })))
                || matches!(ty, Type::Closure(id) if if managed {
                    owned_closures[id.0 as usize]
                } else { borrowed_closures[id.0 as usize] })
                || (!managed
                    && matches!(ty,
                    Type::Address { inner, .. }
                    if matches!(inner.as_ref(), Type::Closure(id) if borrowed_closures[id.0 as usize])))
            {
                let id = *count;
                *count += 1;
                Some(id)
            } else {
                None
            }
        };
        // Collect only actual Package declarations during the existing local
        // slot pass. A function without a Package keeps the former traversal
        // and allocation cost; it does not build or scan an empty field table.
        let mut package_declarations = Vec::new();
        let mut array_declarations = Vec::new();
        let mut managed_local = Vec::with_capacity(f.locals.len());
        for (local, declaration) in f.locals.iter().enumerate() {
            managed_local.push(slot(&declaration.ty, true));
            if let Type::Package { fields, .. } = &declaration.ty {
                budget.spend(2)?;
                package_declarations.push((local, fields));
            }
            if let Type::Array { element, length } = &declaration.ty
                && let Type::Package { fields, .. } = element.as_ref()
            {
                budget.spend((*length as usize).saturating_mul(fields.len().saturating_add(1)))?;
                array_declarations.push((local, *length));
            }
        }
        let local_count = managed_local.iter().flatten().count();
        let managed_value = f.values.iter().map(|t| slot(t, true)).collect();
        let desc_local = f.locals.iter().map(|l| slot(&l.ty, false)).collect();
        let desc_value = f.values.iter().map(|t| slot(t, false)).collect();
        let mut package_fields = if package_declarations.is_empty() && array_declarations.is_empty()
        {
            Vec::new()
        } else {
            budget.spend(f.locals.len())?;
            vec![Vec::new(); f.locals.len()]
        };
        for (local, fields) in package_declarations {
            budget.spend(fields.len())?;
            package_fields[local] = fields
                .iter()
                .map(|(_, ty)| {
                    if matches!(ty, Type::Managed(_)) {
                        let next = debt_count;
                        debt_count += 1;
                        Some(next)
                    } else {
                        None
                    }
                })
                .collect();
        }
        for (local, length) in array_declarations {
            package_fields[local] = (0..length)
                .map(|_| {
                    let next = debt_count;
                    debt_count += 1;
                    Some(next)
                })
                .collect();
        }
        let capture_start = debt_count;
        let captures = f
            .environment
            .map_or(&[][..], |id| &module.closures[id.0 as usize].captures);
        budget.spend(captures.len())?;
        let capture_debt = captures
            .iter()
            .enumerate()
            .map(|(index, ty)| {
                if matches!(ty, Type::Managed(_)) {
                    let next = debt_count;
                    debt_count += 1;
                    Some(next)
                } else if matches!(ty, Type::Package { .. }) {
                    let local =
                        views.by_capture[index].expect("independently checked Package binding");
                    managed_local[local.0 as usize]
                } else {
                    None
                }
            })
            .collect();
        let mut retained_debt = Vec::new();
        if !views.by_local.is_empty() {
            budget.spend(
                capture_start
                    .saturating_add(views.by_local.len())
                    .saturating_add(views.by_capture.len().saturating_mul(2)),
            )?;
            retained_debt = vec![false; capture_start];
            for local in views.by_capture.iter().flatten() {
                retained_debt[managed_local[local.0 as usize].expect("Package shell debt")] = true;
                budget.spend(package_fields[local.0 as usize].len())?;
                for &field in package_fields[local.0 as usize].iter().flatten() {
                    retained_debt[field] = true;
                }
            }
        }
        Ok(Self {
            managed_local,
            managed_value,
            package_fields,
            desc_local,
            desc_value,
            capture_debt,
            package_view: views.by_local.clone(),
            package_capture_local: views.by_capture.clone(),
            retained_debt,
            capture_start,
            local_count,
            debt_count,
            desc_count,
        })
    }

    fn fields(&self, local: LocalId) -> &[Option<usize>] {
        self.package_fields
            .get(local.0 as usize)
            .map_or(&[], Vec::as_slice)
    }

    fn view(&self, local: LocalId) -> Option<u32> {
        self.package_view.get(local.0 as usize).copied().flatten()
    }

    fn retained(&self, debt: usize) -> bool {
        self.retained_debt.get(debt).copied().unwrap_or(false)
    }

    fn empty(&self, budget: &mut Budget) -> Check<State> {
        budget.spend(
            self.debt_count
                .saturating_mul(2)
                .saturating_add(self.desc_count)
                .saturating_add(self.local_count),
        )?;
        Ok(State {
            debt: vec![EMPTY; self.debt_count],
            fresh: vec![0; self.debt_count],
            desc: vec![BTreeSet::new(); self.desc_count],
            newer: vec![BTreeSet::new(); self.local_count],
        })
    }
}

fn state_cost(s: &State, budget: &mut Budget) -> Check<()> {
    budget.spend(
        s.debt
            .len()
            .saturating_mul(2)
            .saturating_add(s.desc.len())
            .saturating_add(s.newer.len()),
    )?;
    for row in &s.desc {
        budget.spend(row.len())?;
    }
    for row in &s.newer {
        budget.spend(row.len())?;
    }
    Ok(())
}

fn clone_state(s: &State, budget: &mut Budget) -> Check<State> {
    state_cost(s, budget)?;
    Ok(s.clone())
}

fn merge(target: &mut State, source: &State, budget: &mut Budget) -> Check<bool> {
    state_cost(source, budget)?;
    let mut changed = false;
    for (a, b) in target
        .debt
        .iter_mut()
        .zip(&source.debt)
        .chain(target.fresh.iter_mut().zip(&source.fresh))
    {
        let next = *a | *b;
        changed |= next != *a;
        *a = next;
    }
    for (a, b) in target.desc.iter_mut().zip(&source.desc) {
        budget.spend(a.len().saturating_add(b.len()))?;
        let before = a.len();
        a.extend(b);
        changed |= a.len() != before;
    }
    for (a, b) in target.newer.iter_mut().zip(&source.newer) {
        budget.spend(a.len().saturating_add(b.len()))?;
        let before = a.len();
        a.extend(b);
        changed |= a.len() != before;
    }
    Ok(changed)
}

fn forget_order(state: &mut State, local: usize, budget: &mut Budget) -> Check<()> {
    budget.spend(state.newer.len())?;
    state.newer[local].clear();
    for row in &mut state.newer {
        row.remove(&local);
    }
    Ok(())
}

fn reverse(slots: &Slots, live: &mut [bool], instruction: &Instruction) {
    if let Some(value) = instruction.result
        && let Some(index) = slots.desc_value[value.0 as usize]
    {
        live[index] = false;
    }
    match instruction.kind {
        InstructionKind::Store { local, .. }
        | InstructionKind::StorageLive(local)
        | InstructionKind::StorageDead(local) => {
            if let Some(index) = slots.desc_local[local.0 as usize] {
                live[index] = false;
            }
        }
        InstructionKind::Load(local)
        | InstructionKind::BorrowLocal { local }
        | InstructionKind::BorrowLocalMut { local }
        | InstructionKind::TakeLocal { local }
        | InstructionKind::CallClosureShared { callee: local, .. }
        | InstructionKind::CallClosureMut { callee: local, .. } => {
            if let Some(index) = slots.desc_local[local.0 as usize] {
                live[index] = true;
            }
        }
        _ => {}
    }
    for operand in instruction_operands(&instruction.kind) {
        if let Some(index) = slots.desc_value[operand.0 as usize] {
            live[index] = true;
        }
    }
}

fn liveness(
    f: &TypedFunction,
    graph: &FunctionGraph,
    slots: &Slots,
    budget: &mut Budget,
) -> Check<Vec<Vec<Vec<bool>>>> {
    let n = f.blocks.len();
    budget.spend(
        n.checked_mul(slots.desc_count)
            .and_then(|v| v.checked_mul(2))
            .ok_or_else(|| limit("resource liveness overflow"))?,
    )?;
    let mut input = vec![vec![false; slots.desc_count]; n];
    let mut output = input.clone();
    let mut pending: VecDeque<_> = (0..n).filter(|&b| graph.reachable[b]).collect();
    budget.spend(n)?;
    let mut queued = graph.reachable.clone();
    while let Some(b) = pending.pop_front() {
        queued[b] = false;
        budget.spend(slots.desc_count.saturating_add(1))?;
        let mut live = vec![false; slots.desc_count];
        for &next in &graph.successors[b] {
            budget.spend(slots.desc_count)?;
            for (a, b) in live.iter_mut().zip(&input[next]) {
                *a |= *b;
            }
        }
        output[b].clone_from(&live);
        for i in f.blocks[b].instructions.iter().rev() {
            budget.spend(instruction_operands(&i.kind).count().saturating_add(1))?;
            reverse(slots, &mut live, i);
        }
        if input[b] != live {
            input[b] = live;
            for &p in &graph.predecessors[b] {
                budget.spend(1)?;
                if graph.reachable[p] && !queued[p] {
                    queued[p] = true;
                    pending.push_back(p);
                }
            }
        }
    }
    let mut after = Vec::with_capacity(n);
    for (b, block) in f.blocks.iter().enumerate() {
        budget.spend(
            block
                .instructions
                .len()
                .checked_mul(slots.desc_count)
                .ok_or_else(|| limit("resource liveness history overflow"))?,
        )?;
        let mut rows = vec![Vec::new(); block.instructions.len()];
        let mut live = output[b].clone();
        for (i, instruction) in block.instructions.iter().enumerate().rev() {
            rows[i] = live.clone();
            reverse(slots, &mut live, instruction);
        }
        after.push(rows);
    }
    Ok(after)
}

fn description_live(
    state: &State,
    slots: &Slots,
    row: usize,
    span: Span,
    budget: &mut Budget,
) -> Check<()> {
    budget.spend(state.desc[row].len().saturating_add(1))?;
    if state.desc[row].is_empty() {
        return Err(bad(
            span,
            "resource description has no independently grounded source",
        ));
    }
    for root in &state.desc[row] {
        let slot = match root {
            Root::Local(local) => slots.managed_local[local.0 as usize],
            Root::PackageField { local, index } | Root::ArrayElement { local, index } => {
                slots.package_fields[local.0 as usize][*index as usize]
            }
            Root::OwnedCapture(index) => slots.capture_debt[*index as usize],
            Root::Parameter(_) | Root::BorrowedCapture(_) => None,
        };
        if let Some(slot) = slot
            && state.debt[slot] != LIVE
        {
            return Err(bad(
                span,
                "description may refer to an uninitialized, moved or discharged resource head",
            ));
        }
    }
    Ok(())
}

fn no_live_description(
    state: &State,
    live: &[bool],
    root: Root,
    span: Span,
    budget: &mut Budget,
) -> Check<()> {
    for (active, roots) in live.iter().zip(&state.desc) {
        budget.spend(1)?;
        if *active {
            budget.spend(roots.len())?;
            if roots.iter().any(|candidate| {
                *candidate == root
                    || matches!(
                        (*candidate, root),
                        (Root::PackageField { local: a, .. } | Root::ArrayElement { local: a, .. }, Root::Local(b)) if a == b
                    )
            }) {
                return Err(bad(
                    span,
                    "moving, clearing or reconstructing a resource head would invalidate a live description",
                ));
            }
        }
    }
    Ok(())
}

struct Transfer<'a> {
    module: &'a TypedModule,
    f: &'a TypedFunction,
    slots: &'a Slots,
    after: &'a [Vec<Vec<bool>>],
    effects: &'a [BTreeSet<Root>],
}

impl Transfer<'_> {
    fn package_shell(
        &self,
        state: &State,
        local: LocalId,
        span: Span,
        validate: bool,
    ) -> Check<()> {
        let debt = self.slots.managed_local[local.0 as usize]
            .expect("typed package local has an independent shell debt");
        if validate && state.debt[debt] != LIVE {
            return Err(bad(
                span,
                "package field operation requires a live shell on every incoming path",
            ));
        }
        Ok(())
    }

    fn consume(&self, state: &mut State, value: ValueId, span: Span, validate: bool) -> Check<()> {
        if let Some(index) = self.slots.managed_value[value.0 as usize] {
            if validate && state.debt[index] != LIVE {
                return Err(bad(
                    span,
                    "managed SSA value was not available exactly once",
                ));
            }
            state.debt[index] = MOVED;
        }
        Ok(())
    }

    fn write_roots(
        &self,
        roots: &BTreeSet<Root>,
        written: &mut BTreeSet<Root>,
        span: Span,
        validate: bool,
        budget: &mut Budget,
    ) -> Check<()> {
        budget.spend(roots.len().saturating_add(written.len()))?;
        if validate
            && self
                .f
                .environment
                .is_some_and(|id| self.module.closures[id.0 as usize].kind == CallableKind::Fn)
            && roots
                .iter()
                .any(|root| matches!(root, Root::OwnedCapture(_))
                    || matches!(root, Root::PackageField { local, .. } if self.slots.view(*local).is_some()))
        {
            return Err(bad(
                span,
                "a shared Fn entry cannot write an owned resource capture, including through a callee",
            ));
        }
        written.extend(
            roots
                .iter()
                .filter(|root| {
                    !matches!(
                        root,
                        Root::Local(_) | Root::PackageField { .. } | Root::ArrayElement { .. }
                    )
                })
                .copied(),
        );
        Ok(())
    }

    fn block(
        &self,
        b: usize,
        state: &mut State,
        validate: bool,
        written: &mut BTreeSet<Root>,
        budget: &mut Budget,
    ) -> Check<()> {
        let f = self.f;
        let slots = self.slots;
        for (i, instruction) in f.blocks[b].instructions.iter().enumerate() {
            budget.spend(
                instruction_operands(&instruction.kind)
                    .count()
                    .saturating_add(1),
            )?;
            let span = instruction.span;
            if validate {
                for operand in instruction_operands(&instruction.kind) {
                    if let Some(row) = slots.desc_value[operand.0 as usize] {
                        description_live(state, slots, row, span, budget)?;
                    }
                }
            }
            let result = instruction
                .result
                .and_then(|v| slots.managed_value[v.0 as usize]);
            let mut fresh_result = None;
            if validate && result.is_some_and(|index| state.debt[index] & LIVE != 0) {
                return Err(bad(
                    span,
                    "a loop redefines a managed SSA result before its previous responsibility was consumed",
                ));
            }
            if validate
                && !slots.package_view.is_empty()
                && f.environment
                    .is_some_and(|id| self.module.closures[id.0 as usize].kind == CallableKind::Fn)
            {
                let written_view = match instruction.kind {
                    InstructionKind::TakePackageField { local, .. }
                    | InstructionKind::DropPackageField { local, .. }
                    | InstructionKind::StorePackageField { local, .. }
                    | InstructionKind::TakeLocal { local }
                    | InstructionKind::DropPackage { local }
                    | InstructionKind::Store { local, .. } => slots.view(local).is_some(),
                    _ => false,
                };
                if written_view {
                    return Err(bad(
                        span,
                        "a shared Fn entry cannot write or consume an owned Package capture",
                    ));
                }
            }
            match &instruction.kind {
                InstructionKind::MakeErrorUnion { value, .. } if result.is_some() => {
                    self.consume(state, *value, span, validate)?;
                    fresh_result = Some(INPUT);
                }
                InstructionKind::TakeErrorUnion { value, .. } => {
                    self.consume(state, *value, span, validate)?;
                    if result.is_some() {
                        fresh_result = Some(INPUT);
                    }
                }
                InstructionKind::ErrorTag { value }
                    if errors::owned(&f.values[value.0 as usize]) =>
                {
                    let debt = slots.managed_value[value.0 as usize].expect("typed owned union");
                    if validate && state.debt[debt] != LIVE {
                        return Err(bad(span, "error tag requires a live owned union"));
                    }
                }
                InstructionKind::BindPackageCapture { .. } => {}
                InstructionKind::EndPackageCapture { local, .. } => {
                    if validate {
                        no_live_description(
                            state,
                            &self.after[b][i],
                            Root::Local(*local),
                            span,
                            budget,
                        )?;
                    }
                }
                InstructionKind::MakeManaged { .. } | InstructionKind::ByteBufferNew => {
                    fresh_result = Some(FRESH)
                }
                InstructionKind::MakePackage { fields }
                | InstructionKind::MakeOwnedArray { elements: fields } => {
                    for &value in fields {
                        self.consume(state, value, span, validate)?;
                    }
                    fresh_result = Some(INPUT);
                }
                InstructionKind::ProjectOwnedArrayField { local, index, .. } => {
                    self.package_shell(state, *local, span, validate)?;
                    let element = slots.package_fields[local.0 as usize][*index as usize]
                        .expect("independently checked owning array element");
                    if validate && state.debt[element] != LIVE {
                        return Err(bad(
                            span,
                            "owning array field projection requires a live element on every path",
                        ));
                    }
                    if let Some(row) = instruction
                        .result
                        .and_then(|v| slots.desc_value[v.0 as usize])
                    {
                        state.desc[row] = BTreeSet::from([Root::ArrayElement {
                            local: *local,
                            index: *index,
                        }]);
                    }
                }
                InstructionKind::TakeOwnedArrayElement { local, index }
                | InstructionKind::DropOwnedArrayElement { local, index } => {
                    self.package_shell(state, *local, span, validate)?;
                    let element = slots.package_fields[local.0 as usize][*index as usize]
                        .expect("independently checked owning array element");
                    let take = matches!(
                        instruction.kind,
                        InstructionKind::TakeOwnedArrayElement { .. }
                    );
                    if validate {
                        if take && state.debt[element] != LIVE {
                            return Err(bad(
                                span,
                                "owning array take requires a complete live element on every path",
                            ));
                        }
                        no_live_description(
                            state,
                            &self.after[b][i],
                            Root::ArrayElement {
                                local: *local,
                                index: *index,
                            },
                            span,
                            budget,
                        )?;
                    }
                    if take {
                        state.debt[element] = MOVED;
                        fresh_result = Some(INPUT);
                    } else {
                        state.debt[element] = discharged(state.debt[element]);
                    }
                }
                InstructionKind::StoreOwnedArrayElement {
                    local,
                    index,
                    value,
                } => {
                    self.package_shell(state, *local, span, validate)?;
                    let element = slots.package_fields[local.0 as usize][*index as usize]
                        .expect("independently checked owning array element");
                    if validate {
                        if state.debt[element] & LIVE != 0 {
                            return Err(bad(
                                span,
                                "owning array refill would overwrite an outstanding element responsibility",
                            ));
                        }
                        no_live_description(
                            state,
                            &self.after[b][i],
                            Root::ArrayElement {
                                local: *local,
                                index: *index,
                            },
                            span,
                            budget,
                        )?;
                    }
                    self.consume(state, *value, span, validate)?;
                    state.debt[element] = LIVE;
                    state.fresh[element] = INPUT;
                }
                InstructionKind::ReadPackageField { local, .. }
                | InstructionKind::DescribePackageField { local, .. } => {
                    self.package_shell(state, *local, span, validate)?;
                    if let InstructionKind::DescribePackageField { index, .. } = instruction.kind {
                        let field = slots.package_fields[local.0 as usize][index as usize]
                            .expect("typed managed package field");
                        if validate && state.debt[field] != LIVE {
                            return Err(bad(
                                span,
                                "package description requires a live field on every incoming path",
                            ));
                        }
                        let row = slots.desc_value
                            [instruction.result.expect("package description result").0 as usize]
                            .expect("package description row");
                        state.desc[row] = BTreeSet::from([Root::PackageField {
                            local: *local,
                            index,
                        }]);
                    }
                }
                InstructionKind::TakePackageField { local, index }
                | InstructionKind::DropPackageField { local, index } => {
                    self.package_shell(state, *local, span, validate)?;
                    let field = slots.package_fields[local.0 as usize][*index as usize]
                        .expect("typed managed package field");
                    let take = matches!(instruction.kind, InstructionKind::TakePackageField { .. });
                    if validate {
                        if take && state.debt[field] != LIVE {
                            return Err(bad(
                                span,
                                "package field transfer requires a live field on every incoming path",
                            ));
                        }
                        no_live_description(
                            state,
                            &self.after[b][i],
                            Root::PackageField {
                                local: *local,
                                index: *index,
                            },
                            span,
                            budget,
                        )?;
                    }
                    if take {
                        state.debt[field] = MOVED;
                        fresh_result = Some(INPUT);
                    } else {
                        state.debt[field] = discharged(state.debt[field]);
                    }
                }
                InstructionKind::StorePackageField {
                    local,
                    index,
                    value,
                } => {
                    self.package_shell(state, *local, span, validate)?;
                    if let Some(field) = slots.package_fields[local.0 as usize][*index as usize] {
                        if validate {
                            if state.debt[field] & LIVE != 0 {
                                return Err(bad(
                                    span,
                                    "package field store would overwrite an outstanding responsibility",
                                ));
                            }
                            no_live_description(
                                state,
                                &self.after[b][i],
                                Root::PackageField {
                                    local: *local,
                                    index: *index,
                                },
                                span,
                                budget,
                            )?;
                        }
                        self.consume(state, *value, span, validate)?;
                        state.debt[field] = LIVE;
                        state.fresh[field] = INPUT;
                    }
                }
                InstructionKind::DescribeLocal { local } => {
                    let index =
                        slots.managed_local[local.0 as usize].expect("typed managed description");
                    if validate && state.debt[index] != LIVE {
                        return Err(bad(
                            span,
                            "only a live managed position may produce a description",
                        ));
                    }
                    let row = slots.desc_value
                        [instruction.result.expect("description result").0 as usize]
                        .expect("description row");
                    state.desc[row] = BTreeSet::from([Root::Local(*local)]);
                }
                InstructionKind::DescribeCapture { index } => {
                    let debt = slots.capture_debt[*index as usize].expect("typed owned capture");
                    if validate && state.debt[debt] != LIVE {
                        return Err(bad(
                            span,
                            "description reads a moved captured resource field",
                        ));
                    }
                    let row = slots.desc_value
                        [instruction.result.expect("description result").0 as usize]
                        .expect("description slot");
                    state.desc[row] = BTreeSet::from([Root::OwnedCapture(*index)]);
                }
                InstructionKind::ReadCapture { index } => {
                    if let Some(row) = instruction
                        .result
                        .and_then(|v| slots.desc_value[v.0 as usize])
                    {
                        state.desc[row] = BTreeSet::from([Root::BorrowedCapture(*index)]);
                    }
                }
                InstructionKind::TakeCapture { index } => {
                    let debt = slots.capture_debt[*index as usize].expect("typed owned capture");
                    if validate {
                        if state.debt[debt] != LIVE {
                            return Err(bad(
                                span,
                                "captured responsibility was already transferred",
                            ));
                        }
                        no_live_description(
                            state,
                            &self.after[b][i],
                            Root::OwnedCapture(*index),
                            span,
                            budget,
                        )?;
                    }
                    fresh_result = Some(INPUT);
                    state.debt[debt] = MOVED;
                }
                InstructionKind::ByteBufferReserve { buffer: desc, .. }
                | InstructionKind::ByteBufferTruncate { buffer: desc, .. }
                | InstructionKind::ByteBufferPush { buffer: desc, .. }
                | InstructionKind::ByteBufferBorrow {
                    buffer: desc,
                    mutable: true,
                }
                | InstructionKind::WriteDesc { desc, .. } => {
                    let row = slots.desc_value[desc.0 as usize].expect("typed description");
                    self.write_roots(&state.desc[row], written, span, validate, budget)?;
                }
                InstructionKind::Load(local)
                | InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local } => {
                    if let Some(dst) = instruction
                        .result
                        .and_then(|v| slots.desc_value[v.0 as usize])
                    {
                        let src =
                            slots.desc_local[local.0 as usize].expect("typed description local");
                        budget.spend(state.desc[src].len())?;
                        state.desc[dst] = state.desc[src].clone();
                    }
                }
                InstructionKind::ReborrowAddress { address, .. } => {
                    if let Some(dst) = instruction
                        .result
                        .and_then(|v| slots.desc_value[v.0 as usize])
                    {
                        let src = slots.desc_value[address.0 as usize]
                            .expect("typed environment reborrow dependency");
                        budget.spend(state.desc[src].len())?;
                        state.desc[dst] = state.desc[src].clone();
                    }
                }
                InstructionKind::TakeLocal { local } => {
                    if let Some(index) = slots.managed_local[local.0 as usize] {
                        let fields = slots.fields(*local);
                        budget.spend(fields.len().saturating_mul(if validate { 2 } else { 1 }))?;
                        if validate {
                            if state.debt[index] != LIVE {
                                return Err(bad(
                                    span,
                                    "give requires a live owned position on every incoming path",
                                ));
                            }
                            if fields
                                .iter()
                                .flatten()
                                .any(|&field| state.debt[field] != LIVE)
                            {
                                return Err(bad(
                                    span,
                                    "whole package transfer requires every managed field live on every incoming path",
                                ));
                            }
                            no_live_description(
                                state,
                                &self.after[b][i],
                                Root::Local(*local),
                                span,
                                budget,
                            )?;
                        }
                        fresh_result = Some(state.fresh[index]);
                        state.debt[index] = MOVED;
                        for &field in fields.iter().flatten() {
                            state.debt[field] = MOVED;
                        }
                        if !slots.retained(index) {
                            forget_order(state, index, budget)?;
                        }
                    }
                    if let Some(dst) = instruction
                        .result
                        .and_then(|v| slots.desc_value[v.0 as usize])
                    {
                        let src =
                            slots.desc_local[local.0 as usize].expect("typed closure description");
                        budget.spend(state.desc[src].len())?;
                        state.desc[dst] = state.desc[src].clone();
                        state.desc[src].clear();
                    }
                }
                InstructionKind::Store { local, value } => {
                    if let Some(index) = slots.managed_local[local.0 as usize] {
                        let fields = slots.fields(*local);
                        budget.spend(fields.len().saturating_mul(if validate { 2 } else { 1 }))?;
                        if validate {
                            if state.debt[index] & LIVE != 0
                                || fields
                                    .iter()
                                    .flatten()
                                    .any(|&field| state.debt[field] & LIVE != 0)
                            {
                                return Err(bad(
                                    span,
                                    "store would overwrite an outstanding resource responsibility",
                                ));
                            }
                            no_live_description(
                                state,
                                &self.after[b][i],
                                Root::Local(*local),
                                span,
                                budget,
                            )?;
                        }
                        let source =
                            slots.managed_value[value.0 as usize].expect("typed managed store");
                        let fresh = state.fresh[source];
                        self.consume(state, *value, span, validate)?;
                        if !slots.retained(index) {
                            forget_order(state, index, budget)?;
                            for other in 0..slots.local_count {
                                budget.spend(1)?;
                                if other != index
                                    && !slots.retained(other)
                                    && state.debt[other] & LIVE != 0
                                {
                                    state.newer[other].insert(index);
                                }
                            }
                        }
                        state.debt[index] = LIVE;
                        state.fresh[index] = fresh;
                        for &field in fields.iter().flatten() {
                            state.debt[field] = LIVE;
                            state.fresh[field] = INPUT;
                        }
                    }
                    if let Some(dst) = slots.desc_local[local.0 as usize] {
                        let src =
                            slots.desc_value[value.0 as usize].expect("typed description store");
                        budget.spend(state.desc[src].len())?;
                        state.desc[dst] = state.desc[src].clone();
                    }
                }
                InstructionKind::DropManaged { local }
                | InstructionKind::DropClosure { local }
                | InstructionKind::DropErrorUnion { local }
                | InstructionKind::DropOwnedArray { local }
                | InstructionKind::DropPackage { local } => {
                    let index = slots.managed_local[local.0 as usize].expect("typed cleanup local");
                    if validate {
                        no_live_description(
                            state,
                            &self.after[b][i],
                            Root::Local(*local),
                            span,
                            budget,
                        )?;
                        if state.debt[index] == DONE {
                            return Err(bad(
                                span,
                                "duplicate automatic cleanup of an already discharged position",
                            ));
                        }
                        budget.spend(state.newer[index].len())?;
                        if !slots.retained(index)
                            && state.newer[index]
                                .iter()
                                .any(|&newer| state.debt[newer] & LIVE != 0)
                        {
                            return Err(bad(
                                span,
                                "automatic cleanup is not in reverse successful initialization order",
                            ));
                        }
                    }
                    state.debt[index] = discharged(state.debt[index]);
                    let fields = slots.fields(*local);
                    budget.spend(fields.len())?;
                    for &field in fields.iter().flatten() {
                        state.debt[field] = discharged(state.debt[field]);
                    }
                    if !slots.retained(index) {
                        forget_order(state, index, budget)?;
                    }
                }
                InstructionKind::StorageLive(local) | InstructionKind::StorageDead(local) => {
                    if let Some(index) = slots.managed_local[local.0 as usize] {
                        let fields = slots.fields(*local);
                        budget.spend(fields.len().saturating_mul(if validate { 2 } else { 1 }))?;
                        if validate {
                            if state.debt[index] & LIVE != 0
                                || fields
                                    .iter()
                                    .flatten()
                                    .any(|&field| state.debt[field] & LIVE != 0)
                            {
                                return Err(bad(
                                    span,
                                    "storage ends or restarts with an outstanding resource responsibility",
                                ));
                            }
                            no_live_description(
                                state,
                                &self.after[b][i],
                                Root::Local(*local),
                                span,
                                budget,
                            )?;
                        }
                        state.debt[index] = EMPTY;
                        state.fresh[index] = 0;
                        for &field in fields.iter().flatten() {
                            state.debt[field] = EMPTY;
                            state.fresh[field] = 0;
                        }
                        forget_order(state, index, budget)?;
                    }
                    if let Some(index) = slots.desc_local[local.0 as usize] {
                        state.desc[index].clear();
                    }
                }
                InstructionKind::MakeClosure { captures, .. } => {
                    if let Some(row) = instruction
                        .result
                        .and_then(|v| slots.desc_value[v.0 as usize])
                    {
                        let mut roots = BTreeSet::new();
                        for value in captures {
                            budget.spend(1)?;
                            if let Some(source) = slots.desc_value[value.0 as usize] {
                                budget
                                    .spend(state.desc[source].len().saturating_add(roots.len()))?;
                                roots.extend(&state.desc[source]);
                            }
                        }
                        state.desc[row] = roots;
                    }
                    for &value in captures {
                        self.consume(state, value, span, validate)?;
                    }
                    if result.is_some() {
                        fresh_result = Some(INPUT);
                    }
                }
                InstructionKind::CallClosureShared { callee, .. }
                | InstructionKind::CallClosureMut { callee, .. } => {
                    if validate
                        && let Some(debt) = slots.managed_local[callee.0 as usize]
                        && state.debt[debt] != LIVE
                    {
                        return Err(bad(
                            span,
                            "closure environment is not live on every incoming path",
                        ));
                    }
                    if let Some(row) = slots.desc_local[callee.0 as usize] {
                        if validate {
                            description_live(state, slots, row, span, budget)?;
                        }
                        let Type::Closure(id) = f.locals[callee.0 as usize].ty else {
                            unreachable!()
                        };
                        let entry = self.module.closures[id.0 as usize].function.0 as usize;
                        budget.spend(self.effects[entry].len())?;
                        if self.effects[entry]
                            .iter()
                            .any(|r| matches!(r, Root::BorrowedCapture(_)))
                        {
                            self.write_roots(&state.desc[row], written, span, validate, budget)?;
                        }
                    }
                }
                InstructionKind::CallClosureBorrowed { callee, .. } => {
                    if let Some(row) = slots.desc_value[callee.0 as usize] {
                        let Type::Address { inner, .. } = &f.values[callee.0 as usize] else {
                            unreachable!()
                        };
                        let Type::Closure(id) = inner.as_ref() else {
                            unreachable!()
                        };
                        let entry = self.module.closures[id.0 as usize].function.0 as usize;
                        budget.spend(self.effects[entry].len())?;
                        if self.effects[entry]
                            .iter()
                            .any(|r| matches!(r, Root::BorrowedCapture(_)))
                        {
                            self.write_roots(&state.desc[row], written, span, validate, budget)?;
                        }
                    }
                }
                InstructionKind::CallClosure { callee, .. }
                | InstructionKind::CallClosureOnce { callee, .. } => {
                    if let Some(row) = slots.desc_value[callee.0 as usize] {
                        let Type::Closure(id) = f.values[callee.0 as usize] else {
                            unreachable!()
                        };
                        let entry = self.module.closures[id.0 as usize].function.0 as usize;
                        budget.spend(self.effects[entry].len())?;
                        if self.effects[entry]
                            .iter()
                            .any(|r| matches!(r, Root::BorrowedCapture(_)))
                        {
                            self.write_roots(&state.desc[row], written, span, validate, budget)?;
                        }
                    }
                    if matches!(instruction.kind, InstructionKind::CallClosureOnce { .. }) {
                        self.consume(state, *callee, span, validate)?;
                    }
                }
                InstructionKind::Call {
                    function,
                    arguments,
                } => {
                    let callable = self.module.functions[function.0 as usize]
                        .callable_contract
                        .as_ref();
                    if let Some(contract) = callable {
                        // helpers::check_module already checked this relation.
                        // Callback consumers use its public write upper bound,
                        // never the helper's private body or inferred effects.
                        budget
                            .spend(contract.traces.len().saturating_add(contract.writes.len()))?;
                        let definition = &self.module.closures[contract.closure.0 as usize];
                        if contract.traces.iter().any(|trace| !trace.calls.is_empty())
                            && contract
                                .writes
                                .iter()
                                .any(|i| matches!(definition.captures[*i as usize], Type::Desc(_)))
                        {
                            let argument = arguments[contract.parameter as usize];
                            if let Some(row) = slots.desc_value[argument.0 as usize] {
                                self.write_roots(
                                    &state.desc[row],
                                    written,
                                    span,
                                    validate,
                                    budget,
                                )?;
                            }
                        }
                    } else {
                        // Helper extras are Copy or complete owned values;
                        // only legacy ordinary calls need private Desc effects.
                        // Do not inspect or charge the helper's private rows.
                        budget.spend(self.effects[function.0 as usize].len())?;
                        for root in &self.effects[function.0 as usize] {
                            if let Root::Parameter(parameter) = root {
                                let value = arguments[*parameter as usize];
                                if let Some(row) = slots.desc_value[value.0 as usize] {
                                    self.write_roots(
                                        &state.desc[row],
                                        written,
                                        span,
                                        validate,
                                        budget,
                                    )?;
                                }
                            }
                        }
                    }
                    for &value in arguments {
                        self.consume(state, value, span, validate)?;
                    }
                    if result.is_some() {
                        fresh_result = Some(
                            if self.module.functions[function.0 as usize].result_mode
                                == PassingMode::Create
                            {
                                FRESH
                            } else {
                                INPUT
                            },
                        );
                    }
                }
                _ => {}
            }
            if let Some(index) = result {
                let fresh = fresh_result.ok_or_else(|| {
                    bad(span, "instruction cannot manufacture a managed SSA carrier")
                })?;
                state.debt[index] = LIVE;
                state.fresh[index] = fresh;
            }
        }
        if let Terminator::Return(value) = f.blocks[b].terminator {
            if let Some(value) = value {
                if validate
                    && let Some(index) = slots.managed_value[value.0 as usize]
                    && f.result_mode == PassingMode::Create
                    && state.fresh[index] != FRESH
                {
                    return Err(bad(
                        f.span,
                        "create result is not independently proven fresh",
                    ));
                }
                self.consume(state, value, f.span, validate)?;
            }
            if validate {
                // This pre-existing ordinary-return scan formerly lacked its
                // own charge. Count every actually inspected responsibility,
                // including non-Package locals/SSA; do not hide old work to
                // claim that adding field accounting is free.
                budget.spend(slots.capture_start)?;
            }
            if validate
                && state.debt[..slots.capture_start]
                    .iter()
                    .enumerate()
                    .any(|(index, s)| !slots.retained(index) && s & LIVE != 0)
            {
                return Err(bad(
                    f.span,
                    "normal return leaves an outstanding resource responsibility",
                ));
            }
            // Shared/exclusive entries return the intact owned environment.
            // A Consume caller performs fixed-order cleanup of remaining live
            // capture flags after this entry returns; moved fields stay cleared.
            if validate
                && f.environment.is_some_and(|id| {
                    self.module.closures[id.0 as usize].kind != CallableKind::FnOnce
                })
                && {
                    budget.spend(
                        slots
                            .capture_debt
                            .len()
                            .saturating_add(slots.retained_debt.len()),
                    )?;
                    slots
                        .capture_debt
                        .iter()
                        .flatten()
                        .any(|&d| state.debt[d] != LIVE)
                        || slots
                            .retained_debt
                            .iter()
                            .enumerate()
                            .any(|(d, retained)| *retained && state.debt[d] != LIVE)
                }
            {
                return Err(bad(
                    f.span,
                    "repeatable closure entry does not preserve every owned field",
                ));
            }
        }
        Ok(())
    }
}

fn run_function(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
    effects: &[BTreeSet<Root>],
    validate: bool,
    budget: &mut Budget,
) -> Check<BTreeSet<Root>> {
    if f.foreign.is_some() {
        return Ok(BTreeSet::new());
    }
    let slots = Slots::new(module, f, &graph.package_views, budget)?;
    if slots.debt_count == 0 && slots.desc_count == 0 {
        return Ok(BTreeSet::new());
    }
    let mut written = BTreeSet::new();
    let after = liveness(f, graph, &slots, budget)?;
    let mut initial = slots.empty(budget)?;
    // Charge actual initialization work, including the old Managed-only scan.
    budget.spend(slots.capture_debt.len())?;
    for (index, debt) in slots.capture_debt.iter().enumerate() {
        if let Some(debt) = debt {
            initial.debt[*debt] = LIVE;
            initial.fresh[*debt] = INPUT;
        }
        if let Some(local) = slots.package_capture_local.get(index).copied().flatten() {
            budget.spend(slots.fields(local).len())?;
            for &field in slots.fields(local).iter().flatten() {
                initial.debt[field] = LIVE;
                initial.fresh[field] = INPUT;
            }
        }
    }
    for (p, local) in f.parameters.iter().enumerate() {
        if let Some(index) = slots.managed_local[local.0 as usize] {
            for old in 0..slots.local_count {
                budget.spend(1)?;
                if !slots.retained(old) && initial.debt[old] & LIVE != 0 {
                    initial.newer[old].insert(index);
                }
            }
            initial.debt[index] = LIVE;
            initial.fresh[index] = INPUT;
            let fields = slots.fields(*local);
            budget.spend(fields.len())?;
            for &field in fields.iter().flatten() {
                initial.debt[field] = LIVE;
                initial.fresh[field] = INPUT;
            }
        }
        if let Some(index) = slots.desc_local[local.0 as usize] {
            initial.desc[index].insert(Root::Parameter(p as u32));
        }
    }
    budget.spend(f.blocks.len().saturating_mul(2))?;
    let mut inputs = vec![None; f.blocks.len()];
    inputs[f.entry.0 as usize] = Some(initial);
    let mut pending = VecDeque::from([f.entry.0 as usize]);
    let mut queued = vec![false; f.blocks.len()];
    queued[f.entry.0 as usize] = true;
    let transfer = Transfer {
        module,
        f,
        slots: &slots,
        after: &after,
        effects,
    };
    while let Some(b) = pending.pop_front() {
        queued[b] = false;
        let mut output = clone_state(inputs[b].as_ref().expect("queued resource state"), budget)?;
        transfer.block(b, &mut output, false, &mut written, budget)?;
        for &next in &graph.successors[b] {
            budget.spend(1)?;
            let changed = if let Some(input) = &mut inputs[next] {
                merge(input, &output, budget)?
            } else {
                inputs[next] = Some(clone_state(&output, budget)?);
                true
            };
            if changed && !queued[next] {
                queued[next] = true;
                pending.push_back(next);
            }
        }
    }
    for (b, input) in inputs.into_iter().enumerate() {
        if let Some(mut state) = input {
            transfer.block(b, &mut state, validate, &mut written, budget)?;
        }
    }
    Ok(written)
}

fn effect_dependents(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    budget: &mut Budget,
) -> Check<Vec<Vec<usize>>> {
    let count = module.functions.len();
    budget.spend(
        count
            .checked_mul(5)
            .ok_or_else(|| limit("resource effect frontier size overflow"))?,
    )?;
    let mut dependents = vec![Vec::new(); count];
    for (user, (function, graph)) in module.functions.iter().zip(graphs).enumerate() {
        for (block, live) in function.blocks.iter().zip(&graph.reachable) {
            budget.spend(1)?;
            if !live {
                continue;
            }
            for instruction in &block.instructions {
                budget.spend(1)?;
                let dependency = match &instruction.kind {
                    InstructionKind::Call { function, .. } => {
                        let callee =
                            module.functions.get(function.0 as usize).ok_or_else(|| {
                                bad(instruction.span, "resource effect call target is absent")
                            })?;
                        // A checked public callback envelope is the dependency
                        // boundary. No private row is consumed for such calls.
                        callee
                            .callable_contract
                            .is_none()
                            .then_some(function.0 as usize)
                    }
                    InstructionKind::CallClosureShared { callee, .. }
                    | InstructionKind::CallClosureMut { callee, .. } => {
                        let local = function.locals.get(callee.0 as usize).ok_or_else(|| {
                            bad(instruction.span, "resource effect closure local is absent")
                        })?;
                        Some(effect_closure_target(module, &local.ty, instruction.span)?)
                    }
                    InstructionKind::CallClosureBorrowed { callee, .. }
                    | InstructionKind::CallClosureOnce { callee, .. }
                    | InstructionKind::CallClosure { callee, .. } => {
                        let ty = function.values.get(callee.0 as usize).ok_or_else(|| {
                            bad(instruction.span, "resource effect closure value is absent")
                        })?;
                        Some(effect_closure_target(module, ty, instruction.span)?)
                    }
                    _ => None,
                };
                if let Some(dependency) = dependency {
                    let rows = dependents.get_mut(dependency).ok_or_else(|| {
                        bad(instruction.span, "resource effect closure entry is absent")
                    })?;
                    if rows.last() != Some(&user) {
                        budget.spend(2)?;
                        rows.push(user);
                    }
                }
            }
        }
    }
    Ok(dependents)
}

fn effect_closure_target(module: &TypedModule, ty: &Type, span: Span) -> Check<usize> {
    let id = match ty {
        Type::Closure(id) => *id,
        Type::Address { inner, .. } => match inner.as_ref() {
            Type::Closure(id) => *id,
            _ => return Err(bad(span, "resource effect address has no closure entry")),
        },
        _ => return Err(bad(span, "resource effect call has no closure entry")),
    };
    let entry = module
        .closures
        .get(id.0 as usize)
        .ok_or_else(|| bad(span, "resource effect closure definition is absent"))?
        .function;
    Ok(entry.0 as usize)
}

fn infer_effects(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    budget: &mut Budget,
) -> Check<Vec<BTreeSet<Root>>> {
    budget.spend(module.functions.len())?;
    let mut effects = vec![BTreeSet::new(); module.functions.len()];
    let dependents = effect_dependents(module, graphs, budget)?;
    // Independent verifier scheduling: finish a batch before processing its
    // notifications. Every function runs initially; growing rows wake consumers.
    let mut dirty: Vec<usize> = (0..module.functions.len()).rev().collect();
    let mut next = Vec::new();
    let mut scheduled = vec![true; module.functions.len()];
    while !dirty.is_empty() {
        while let Some(index) = dirty.pop() {
            budget.spend(1)?;
            scheduled[index] = false;
            #[cfg(test)]
            TEST_EFFECT_SOLVES.with(|value| value.set(value.get() + 1));
            let found = run_function(
                module,
                &module.functions[index],
                &graphs[index],
                &effects,
                false,
                budget,
            )?;
            budget.spend(found.len().saturating_add(effects[index].len()))?;
            let before = effects[index].len();
            effects[index].extend(found);
            if effects[index].len() != before {
                budget.spend(dependents[index].len())?;
                for &user in &dependents[index] {
                    if !scheduled[user] {
                        budget.spend(1)?;
                        scheduled[user] = true;
                        next.push(user);
                    }
                }
            }
        }
        std::mem::swap(&mut dirty, &mut next);
    }
    Ok(effects)
}

#[cfg(test)]
fn infer_effects_full_scan(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    budget: &mut Budget,
) -> Check<Vec<BTreeSet<Root>>> {
    budget.spend(module.functions.len())?;
    let mut effects = vec![BTreeSet::new(); module.functions.len()];
    loop {
        let mut changed = false;
        for (index, (function, graph)) in module.functions.iter().zip(graphs).enumerate() {
            budget.spend(1)?;
            TEST_EFFECT_SOLVES.with(|value| value.set(value.get() + 1));
            let found = run_function(module, function, graph, &effects, false, budget)?;
            budget.spend(found.len().saturating_add(effects[index].len()))?;
            let before = effects[index].len();
            effects[index].extend(found);
            changed |= effects[index].len() != before;
        }
        if !changed {
            return Ok(effects);
        }
    }
}

#[cfg(test)]
thread_local! {
    static TEST_EFFECT_SOLVES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(super) fn check_module(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    budget: &mut Budget,
) -> Check<()> {
    budget.spend(module.closures.len())?;
    if module.resources.is_empty()
        && !module
            .closures
            .iter()
            .any(|c| c.kind == CallableKind::FnOnce)
    {
        return Ok(());
    }
    // Effects are inferred from executable writes and actual description origins,
    // including forwarding and closures. An empty candidate is never authority.
    let effects = infer_effects(module, graphs, budget)?;
    for (function, graph) in module.functions.iter().zip(graphs) {
        run_function(module, function, graph, &effects, true, budget)?;
    }
    Ok(())
}

#[cfg(test)]
mod frontier_tests;

/// A test-only consumer probe starts with a fully checked module. It removes
/// private callee rows only for the separate caller transfer, never for Verify.
#[cfg(test)]
pub(super) fn assert_public_helper_effect_independence(
    verified: &VerifiedModule,
    caller: FunctionId,
    helper: FunctionId,
    writes_parameter: bool,
) {
    let module = verified.module();
    let f = &module.functions[caller.0 as usize];
    let limits = Limits::default();
    let graph = check_function(
        module,
        f,
        &verified.plan().functions[caller.0 as usize],
        &mut Budget::new(&limits),
        &limits,
    )
    .unwrap();
    let empty_rows = vec![BTreeSet::new(); module.functions.len()];
    let mut forged_rows = empty_rows.clone();
    forged_rows[helper.0 as usize] = BTreeSet::from([
        Root::Parameter(0),
        Root::Parameter(u32::MAX),
        Root::BorrowedCapture(u32::MAX),
    ]);
    let expected = if writes_parameter {
        BTreeSet::from([Root::Parameter(0)])
    } else {
        BTreeSet::new()
    };
    let mut cost = None;
    for private_rows in [Vec::new(), empty_rows, forged_rows] {
        let mut budget = Budget::new(&limits);
        let result = run_function(module, f, &graph, &private_rows, true, &mut budget).unwrap();
        assert_eq!(result, expected);
        if let Some(previous) = cost.replace(budget.remaining) {
            assert_eq!(
                previous, budget.remaining,
                "caller read private helper rows"
            );
        }
    }
}
