//! Request-local production of equal, currently verified arithmetic bodies.
//! Selectors only locate candidates: complete current semantic CFG equality
//! authorizes each reuse. The producer may restore explicit trusted raw data
//! only after current admission and the same current reservation consumer.
use super::*;
use std::collections::{HashMap, hash_map::RandomState};
use std::hash::{BuildHasher, Hash, Hasher};

struct Product<'a> {
    input: &'a TypedFunction,
    output: FunctionBatch,
}
/// Allocation selectors live only while the current LIR is borrowed. Check all
/// three immutable owners as well as the complete outer input on every hit.
/// In particular, neither an ID nor an empty slice address proves body equality.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct Owners(
    *const Vec<zil_ir::LocalDecl>,
    *const Vec<Type>,
    *const Vec<zil_ir::BasicBlock>,
);
impl Owners {
    pub(super) fn of(function: &TypedFunction) -> Self {
        Self(
            std::ptr::from_ref(&*function.locals),
            std::ptr::from_ref(&*function.values),
            std::ptr::from_ref(&*function.blocks),
        )
    }
    fn addresses(self) -> (usize, usize, usize) {
        (self.0 as usize, self.1 as usize, self.2 as usize)
    }
    pub(super) fn same(left: &TypedFunction, right: &TypedFunction) -> bool {
        left.locals.ptr_eq(&right.locals)
            && left.values.ptr_eq(&right.values)
            && left.blocks.ptr_eq(&right.blocks)
            && incremental::same_body_header(left, right)
    }
}
// Current semantic equality is established ONCE before the pure consumers.
// Absolute diagnostic placement can split COW arrays without changing machine
// inputs. Pointer-only caches therefore cannot represent this equivalence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Key {
    Semantic(usize),
    Owner(Owners),
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Observation {
    function: usize,
    cost: Option<usize>,
    semantic: Option<(usize, usize)>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Inputs {
    // These strong roots keep both function addresses and all three body
    // owners alive. A changed LIR wrapper must bind its complete header too.
    functions: zil_ir::PersistentVec<TypedFunction>,
    identities: HashMap<usize, usize>,
    by_id: HashMap<FunctionId, usize>,
    owners: HashMap<(usize, usize, usize), usize>,
    observations: Vec<Observation>,
    #[cfg(test)]
    owner_reuses: usize,
}
impl Inputs {
    pub(super) fn new(functions: &zil_ir::PersistentVec<TypedFunction>) -> Self {
        let selector = RandomState::new();
        let mut buckets = HashMap::<u64, Vec<(&TypedFunction, usize)>>::new();
        let mut inputs = Self {
            functions: functions.clone(),
            identities: HashMap::new(),
            by_id: HashMap::new(),
            owners: HashMap::new(),
            observations: Vec::new(),
            #[cfg(test)]
            owner_reuses: 0,
        };
        // Selectors belong to this construction's RandomState, not to later
        // consumers. Retained observations contain actual cost, never a hash
        // certificate. Storage is bounded by the current function inventory.
        let mut selectors = Vec::<Option<u64>>::new();
        let mut fuel = 1_000_000usize;
        for (index, function) in functions.iter().enumerate() {
            inputs.by_id.insert(function.id, index);
            let observed = if let Some(row) = inputs.observation(function) {
                #[cfg(test)]
                {
                    inputs.owner_reuses += 1;
                }
                row
            } else {
                let row = inputs.observations.len();
                inputs.observations.push(Observation {
                    function: index,
                    cost: cost(function),
                    semantic: None,
                });
                selectors.push(None);
                inputs
                    .owners
                    .entry(Owners::of(function).addresses())
                    .or_insert(row);
                row
            };
            let Some(cost) = inputs.observations[observed]
                .cost
                .filter(|cost| *cost <= fuel)
            else {
                continue;
            };
            // Preserve the original optional fuel and ordered collision costs,
            // including for shared owners. Only the actual scans disappear.
            fuel -= cost;
            // Repeated exact owners have the same first collision match. Replay
            // every original logical collision debit, but do not traverse the
            // equal locals/values/CFG again for each diagnostic alias.
            if let Some((id, comparisons)) = inputs.observations[observed].semantic
                && let Some(charge) = cost.checked_mul(comparisons)
                && let Some(remaining) = fuel.checked_sub(charge)
            {
                fuel = remaining;
                inputs
                    .identities
                    .insert(std::ptr::from_ref(function) as usize, id);
                continue;
            }
            let hash =
                *selectors[observed].get_or_insert_with(|| selector.hash_one(Selector(function)));
            let mut identity = None;
            let mut comparisons = 0usize;
            let mut complete = true;
            let mut bucket_len = 0usize;
            if let Some(rows) = buckets.get(&hash) {
                bucket_len = rows.len();
                for &(prior, id) in rows {
                    let Some(left) = fuel.checked_sub(cost) else {
                        complete = false;
                        break;
                    };
                    fuel = left;
                    comparisons += 1;
                    if incremental::same_body_input(prior, function) {
                        identity = Some(id);
                        break;
                    }
                }
            }
            let next = inputs.identities.len();
            let id = identity.unwrap_or(next);
            if identity.is_none() {
                buckets.entry(hash).or_default().push((function, id));
                if complete {
                    inputs.observations[observed].semantic = Some((id, bucket_len + 1));
                }
            } else {
                inputs.observations[observed].semantic = Some((id, comparisons));
            }
            inputs
                .identities
                .insert(std::ptr::from_ref(function) as usize, id);
        }
        inputs
    }
    fn observation(&self, function: &TypedFunction) -> Option<usize> {
        // One representative per owner tuple bounds lookup even when many
        // different outer headers happen to share these same arrays. A header
        // mismatch takes the original scan, never a growing candidate bucket.
        let row = *self.owners.get(&Owners::of(function).addresses())?;
        Owners::same(&self.functions[self.observations[row].function], function).then_some(row)
    }
    fn observed_cost(&self, function: &TypedFunction) -> Option<Option<usize>> {
        Some(self.observations[self.observation(function)?].cost)
    }
    fn identity(&self, function: &TypedFunction) -> Option<usize> {
        if let Some(id) = self
            .identities
            .get(&(std::ptr::from_ref(function) as usize))
        {
            return Some(*id);
        }
        // Source and LIR can wrap the exact same arrays in different function
        // allocations. Use the CURRENT source row's admission (including fuel
        // exhaustion), not some sibling's semantic identity or a bare ID.
        let original = &self.functions[*self.by_id.get(&function.id)?];
        if !Owners::same(original, function) {
            return None;
        }
        self.identities
            .get(&(std::ptr::from_ref(original) as usize))
            .copied()
    }
}
pub(super) fn key(inputs: Option<&Inputs>, function: &TypedFunction) -> Key {
    inputs
        .and_then(|inputs| inputs.identity(function))
        .map_or_else(|| Key::Owner(Owners::of(function)), Key::Semantic)
}

pub(super) fn same(inputs: Option<&Inputs>, left: &TypedFunction, right: &TypedFunction) -> bool {
    match (key(inputs, left), key(inputs, right)) {
        (Key::Semantic(a), Key::Semantic(b)) => a == b,
        _ => Owners::same(left, right),
    }
}

pub(super) struct Cache<'a> {
    products: Vec<Product<'a>>,
    buckets: HashMap<u64, Vec<usize>>,
    owners: HashMap<Key, usize>,
    inputs: Option<std::sync::Arc<Inputs>>,
    selector: RandomState,
    fuel: usize,
    pub(super) hits: usize,
    pub(super) produced: usize,
    pub(super) owner_hits: usize,
}
impl Default for Cache<'_> {
    fn default() -> Self {
        Self {
            products: Vec::new(),
            buckets: HashMap::new(),
            owners: HashMap::new(),
            inputs: None,
            selector: RandomState::new(),
            fuel: 1_000_000,
            hits: 0,
            produced: 0,
            owner_hits: 0,
        }
    }
}
fn primitive(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Unit | Type::Bool | Type::I32 | Type::I64 | Type::U8 | Type::U16
    )
}
fn cost(function: &TypedFunction) -> Option<usize> {
    if function.foreign.is_some()
        || function.environment.is_some()
        || function.storage_life.is_some()
        || function.call_kind != zil_ir::CallKind::Direct
        || function.reference_return_bound.is_some()
        || function.callable_return_contract.is_some()
        || function.callable_contract.is_some()
        || function.protocol_contract.is_some()
        || !primitive(&function.return_type)
        || !function.values.iter().all(primitive)
        || !function.locals.iter().all(|local| primitive(&local.ty))
    {
        return None;
    }
    let mut cost = function
        .values
        .len()
        .checked_add(function.parameters.len())?;
    for local in &function.locals {
        cost = cost.checked_add(local.name.len().checked_add(1)?)?;
    }
    for block in &function.blocks {
        cost = cost.checked_add(block.instructions.len().checked_add(1)?)?;
        for instruction in &block.instructions {
            // No global data, hidden interfaces or source-id-dependent helpers.
            // Direct calls retain their EXACT callee IDs and argument slots.
            // All current callee headers, ABI plans and summary facts are fixed
            // for this request; admission checked each caller's scalar ABI.
            match &instruction.kind {
                InstructionKind::Constant(c) if primitive(&c.ty()) => {}
                InstructionKind::Load(_)
                | InstructionKind::Store { .. }
                | InstructionKind::StorageLive(_)
                | InstructionKind::StorageDead(_)
                | InstructionKind::ScopeEnter { .. }
                | InstructionKind::ScopeExit { .. }
                | InstructionKind::UnsafeEnter { .. }
                | InstructionKind::UnsafeExit { .. }
                | InstructionKind::Unary { .. }
                | InstructionKind::Binary { .. }
                | InstructionKind::ConvertInteger { .. }
                | InstructionKind::Call { .. } => {}
                _ => return None,
            }
        }
    }
    Some(cost.saturating_add(1))
}

pub(super) fn cacheable(function: &TypedFunction) -> bool {
    cacheable_with_inputs(None, function)
}

pub(super) fn cacheable_with_inputs(inputs: Option<&Inputs>, function: &TypedFunction) -> bool {
    current_cost(inputs, function).is_some()
}

fn current_cost(inputs: Option<&Inputs>, function: &TypedFunction) -> Option<usize> {
    inputs
        .and_then(|inputs| inputs.observed_cost(function))
        .unwrap_or_else(|| cost(function))
}

struct Selector<'a>(&'a TypedFunction);
impl Hash for Selector<'_> {
    fn hash<H: Hasher>(&self, h: &mut H) {
        let f = self.0;
        f.parameters.hash(h);
        f.return_type.hash(h);
        f.result_mode.hash(h);
        f.entry.hash(h);
        for local in &f.locals {
            local.id.hash(h);
            local.name.hash(h);
            local.ty.hash(h);
            local.mutable.hash(h);
        }
        f.values.hash(h);
        for block in &f.blocks {
            block.id.hash(h);
            std::mem::discriminant(&block.terminator).hash(h);
            match block.terminator {
                Terminator::Return(value) => value.hash(h),
                Terminator::Jump(target) => target.hash(h),
                Terminator::Branch {
                    condition,
                    then_block,
                    else_block,
                } => {
                    (condition, then_block, else_block).hash(h);
                }
                Terminator::Trap | Terminator::Unreachable => {}
            }
            for instruction in &block.instructions {
                instruction.result.hash(h);
                std::mem::discriminant(&instruction.kind).hash(h);
                match &instruction.kind {
                    InstructionKind::Constant(c) => c.hash(h),
                    InstructionKind::Load(id)
                    | InstructionKind::StorageLive(id)
                    | InstructionKind::StorageDead(id) => id.hash(h),
                    InstructionKind::Store { local, value } => (local, value).hash(h),
                    InstructionKind::Unary { op, operand } => (op, operand).hash(h),
                    InstructionKind::Binary { op, left, right } => (op, left, right).hash(h),
                    InstructionKind::ConvertInteger { value, target } => (value, target).hash(h),
                    InstructionKind::Call {
                        function,
                        arguments,
                    } => {
                        (function, arguments).hash(h);
                    }
                    // Marker fields are compared in full below; ignoring their
                    // payload here affects collisions, never admission.
                    _ => {}
                }
            }
        }
    }
}
impl<'a> Cache<'a> {
    pub(super) fn set_inputs(&mut self, inputs: Option<std::sync::Arc<Inputs>>) {
        self.inputs = inputs;
    }
    pub(super) fn lower<F, R>(
        &mut self,
        function: &'a TypedFunction,
        admitted: bool,
        reserve: R,
        lower: F,
    ) -> CompileResult<FunctionBatch>
    where
        F: FnOnce() -> CompileResult<FunctionBatch>,
        R: FnOnce() -> CompileResult<()>,
    {
        // Optional optimization fuel never renews the compiler's original
        // ledger. Exhaustion simply runs the original producer.
        if !admitted || self.fuel == 0 {
            return lower();
        }
        let owner = key(self.inputs.as_deref(), function);
        if let Some(&index) = self.owners.get(&owner)
            && same(self.inputs.as_deref(), self.products[index].input, function)
        {
            // No body walk, hashing, CFG reconstruction, promotion or early
            // optimization. Pay only lookup work in this optional ledger; the
            // original current compiler reservation below is unchanged.
            self.fuel -= 1;
            reserve()?;
            self.hits += 1;
            self.owner_hits += 1;
            return Ok(rebind(&self.products[index].output, function.id));
        }
        let cost = current_cost(self.inputs.as_deref(), function);
        let Some(cost) = cost.filter(|cost| *cost <= self.fuel) else {
            return lower();
        };
        self.fuel -= cost;
        let selector = self.selector.hash_one(Selector(function));
        if let Some(products) = self.buckets.get(&selector) {
            for &index in products {
                let product = &self.products[index];
                let Some(remaining) = self.fuel.checked_sub(cost) else {
                    self.fuel = 0;
                    return lower();
                };
                self.fuel = remaining;
                if !incremental::same_body_input(product.input, function) {
                    continue;
                }
                // Run the original CURRENT ordered fee consumer. Any error is
                // reported at the current source, before publishing a body.
                reserve()?;
                let output = rebind(&product.output, function.id);
                // Retain the current owner too: its siblings need no repeated
                // content scan even if this was independently allocated input.
                self.owners.insert(owner, self.products.len());
                self.products.push(Product {
                    input: function,
                    output: output.clone(),
                });
                self.hits += 1;
                return Ok(output);
            }
        }
        let output = lower()?;
        if output.task_machines.is_empty()
            && output.foreign_adapters.is_empty()
            && matches!(output.body.as_slice(), [m::Item::Function(f)]
                if f.name == symbol(function.id) && f.blocks.is_some())
        {
            self.produced += 1;
            self.owners.insert(owner, self.products.len());
            self.buckets
                .entry(selector)
                .or_default()
                .push(self.products.len());
            self.products.push(Product {
                input: function,
                output: output.clone(),
            });
        }
        Ok(output)
    }
}

pub(super) fn rebind(output: &FunctionBatch, id: FunctionId) -> FunctionBatch {
    let mut output = output.clone();
    let m::Item::Function(machine) = &mut output.body[0] else {
        unreachable!("only one successful definition is retained")
    };
    machine.name = symbol(id);
    output
}

#[cfg(test)]
#[path = "scalar_bodies_tests.rs"]
mod tests;
