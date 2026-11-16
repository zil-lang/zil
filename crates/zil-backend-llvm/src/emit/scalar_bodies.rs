//! Request-local production of equal, currently verified arithmetic bodies.
//! No disk or previous-request input enters this table. Selectors only locate
//! candidates: complete current semantic CFG equality authorizes each reuse.
use super::*;
use std::collections::{HashMap, hash_map::RandomState};
use std::hash::{BuildHasher, Hash, Hasher};

struct Product<'a> {
    input: &'a TypedFunction,
    output: FunctionBatch,
}
pub(super) struct Cache<'a> {
    products: HashMap<u64, Vec<Product<'a>>>,
    selector: RandomState,
    fuel: usize,
    pub(super) hits: usize,
    pub(super) produced: usize,
}
impl Default for Cache<'_> {
    fn default() -> Self {
        Self {
            products: HashMap::new(),
            selector: RandomState::new(),
            fuel: 1_000_000,
            hits: 0,
            produced: 0,
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
            // No global data, hidden interfaces, current scalar summaries or
            // source-id-dependent helpers. Even direct calls are excluded.
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
                | InstructionKind::ConvertInteger { .. } => {}
                _ => return None,
            }
        }
    }
    Some(cost.saturating_add(1))
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
                    // Marker fields are compared in full below; ignoring their
                    // payload here affects collisions, never admission.
                    _ => {}
                }
            }
        }
    }
}
impl<'a> Cache<'a> {
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
        let cost = cost(function);
        let Some(cost) = cost.filter(|cost| *cost <= self.fuel) else {
            return lower();
        };
        self.fuel -= cost;
        let selector = self.selector.hash_one(Selector(function));
        if let Some(products) = self.products.get(&selector) {
            for product in products {
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
                let mut output = product.output.clone();
                let m::Item::Function(machine) = &mut output.body[0] else {
                    unreachable!("only one successful definition is retained")
                };
                machine.name = symbol(function.id);
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
            self.products.entry(selector).or_default().push(Product {
                input: function,
                output: output.clone(),
            });
        }
        Ok(output)
    }
}
