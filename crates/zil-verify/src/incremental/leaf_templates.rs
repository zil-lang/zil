//! Fresh scalar CFG successes within one current product verification call.
//! No persisted input, producer claim, or selector is a proof. The complete
//! module prelude and all downstream resource/borrow checks remain current.
use super::*;
use std::hash::{Hash, Hasher};

#[derive(Default)]
pub(crate) struct Templates<'a> {
    // One candidate per selector bounds even adversarial collisions to one
    // exact comparison. A collision can only lose reuse.
    rows: BTreeMap<u64, Receipt<'a>>,
    context: Option<(&'a TypedModule, &'a Limits, bool)>,
}

struct Receipt<'a> {
    function: &'a TypedFunction,
    plan: &'a FunctionPlan,
    work: usize,
    graph: FunctionGraph,
}

fn primitive(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Unit
            | Type::Bool
            | Type::I8
            | Type::I16
            | Type::I32
            | Type::I64
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::Isize
            | Type::Usize
            | Type::F32
            | Type::F64
    )
}

// For the whitelist below spans are read only to construct errors: unsafe
// regions, foreign calls and all lifetime-bearing operations are excluded.
// Still require a valid relative location, preserving the entire layout.
fn relative(span: Span, outer: Span) -> Option<(usize, usize)> {
    if span.file != outer.file
        || outer.start > outer.end
        || span.start > span.end
        || span.end > outer.end
    {
        return None;
    }
    Some((
        span.start.checked_sub(outer.start)?,
        span.end.checked_sub(outer.start)?,
    ))
}

fn selector(f: &TypedFunction, p: &FunctionPlan, budget: &Budget) -> Option<u64> {
    // These instruction preludes can read or populate module-derived caches.
    // Keep them on the full checker; the admitted path only changes remaining.
    if budget.pod_stores
        || budget.stored_borrows
        || !budget.array_snapshot_roots.is_empty()
        || f.storage_life.is_some()
        || f.reference_return_bound.is_some()
        || f.callable_return_contract.is_some()
        || f.callable_contract.is_some()
        || f.protocol_contract.is_some()
        || f.foreign.is_some()
        || f.environment.is_some()
        || f.call_kind != CallKind::Direct
        || !matches!(f.result_mode, PassingMode::Copy | PassingMode::Infer)
        || p.function != f.id
        || !primitive(&f.return_type)
        || !f.locals.iter().all(|l| primitive(&l.ty))
        || !f.values.iter().all(primitive)
    {
        return None;
    }
    relative(f.span, f.span)?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    f.return_type.hash(&mut h);
    f.result_mode.hash(&mut h);
    f.parameters.hash(&mut h);
    f.entry.hash(&mut h);
    for l in &f.locals {
        relative(l.span, f.span)?;
        l.id.hash(&mut h);
        l.name.hash(&mut h);
        l.ty.hash(&mut h);
        l.mutable.hash(&mut h);
    }
    for ty in &f.values {
        ty.hash(&mut h);
    }
    for b in &f.blocks {
        b.id.hash(&mut h);
        std::mem::discriminant(&b.terminator).hash(&mut h);
        for i in &b.instructions {
            relative(i.span, f.span)?;
            i.result.hash(&mut h);
            std::mem::discriminant(&i.kind).hash(&mut h);
            match &i.kind {
                InstructionKind::Constant(c) if primitive(&c.ty()) => c.hash(&mut h),
                InstructionKind::Load(l)
                | InstructionKind::StorageLive(l)
                | InstructionKind::StorageDead(l) => l.hash(&mut h),
                InstructionKind::Store { local, value } => (local, value).hash(&mut h),
                InstructionKind::Unary { op, operand } => (op, operand).hash(&mut h),
                InstructionKind::Binary { op, left, right } => (op, left, right).hash(&mut h),
                InstructionKind::ConvertNumber { value, target }
                | InstructionKind::ConvertInteger { value, target }
                    if primitive(target) =>
                {
                    (value, target).hash(&mut h);
                }
                InstructionKind::ScopeEnter { scope } => scope.hash(&mut h),
                InstructionKind::ScopeExit { scope, kind } => (scope, kind).hash(&mut h),
                _ => return None,
            }
        }
    }
    Some(h.finish())
}

fn equivalent(a: &TypedFunction, b: &TypedFunction, pa: &FunctionPlan, pb: &FunctionPlan) -> bool {
    // Exhaustive destructuring forces every future field to get a decision.
    let TypedFunction {
        storage_life,
        call_kind,
        reference_return_bound,
        callable_return_contract,
        callable_contract,
        protocol_contract,
        id: _,
        name: _,
        foreign,
        environment,
        parameters,
        return_type,
        result_mode,
        locals,
        values,
        blocks,
        entry,
        span,
    } = a;
    let FunctionPlan {
        function: _,
        blocks: plan_blocks,
        holds,
    } = pa;
    storage_life == &b.storage_life
        && call_kind == &b.call_kind
        && reference_return_bound == &b.reference_return_bound
        && callable_return_contract == &b.callable_return_contract
        && callable_contract == &b.callable_contract
        && protocol_contract == &b.protocol_contract
        && foreign == &b.foreign
        && environment == &b.environment
        && parameters == &b.parameters
        && return_type == &b.return_type
        && result_mode == &b.result_mode
        && values == &b.values
        && entry == &b.entry
        && pa.function == a.id
        && pb.function == b.id
        && plan_blocks == &pb.blocks
        && holds == &pb.holds
        && relative(*span, *span) == relative(b.span, b.span)
        && locals.len() == b.locals.len()
        && locals.iter().zip(&b.locals).all(|(x, y)| {
            let LocalDecl {
                id,
                name,
                ty,
                mutable,
                span,
            } = x;
            id == &y.id
                && name == &y.name
                && ty == &y.ty
                && mutable == &y.mutable
                && relative(*span, a.span) == relative(y.span, b.span)
        })
        && blocks.len() == b.blocks.len()
        && blocks.iter().zip(&b.blocks).all(|(x, y)| {
            let BasicBlock {
                id,
                instructions,
                terminator,
            } = x;
            id == &y.id
                && terminator == &y.terminator
                && instructions.len() == y.instructions.len()
                && instructions.iter().zip(&y.instructions).all(|(x, y)| {
                    let Instruction { result, kind, span } = x;
                    result == &y.result
                        && kind == &y.kind
                        && relative(*span, a.span) == relative(y.span, b.span)
                })
        })
}

fn copy_graph(g: &FunctionGraph) -> FunctionGraph {
    FunctionGraph {
        successors: g.successors.clone(),
        predecessors: g.predecessors.clone(),
        reachable: g.reachable.clone(),
        role_data_uses: g.role_data_uses.clone(),
        // Eligibility excludes environments; capture_views returns empty.
        package_views: packages::CaptureViews::default(),
    }
}

impl<'a> Templates<'a> {
    pub(crate) fn check(
        &mut self,
        module: &'a TypedModule,
        function: &'a TypedFunction,
        plan: &'a FunctionPlan,
        budget: &mut Budget,
        limits: &'a Limits,
        mut attempt: Option<&mut Attempt<'_>>,
    ) -> Check<FunctionGraph> {
        if self.context.is_some_and(|(old, old_limits, sums)| {
            !std::ptr::eq(old, module) || old_limits != limits || sums != budget.sums
        }) {
            self.rows.clear();
        }
        self.context = Some((module, limits, budget.sums));
        let Some(key) = selector(function, plan, budget) else {
            return check_function_with_reuse(module, function, plan, budget, limits, attempt);
        };
        // Never publish a stale prior-epoch row for a template-covered body.
        if let Some(a) = attempt.as_deref_mut() {
            a.invalidate_flow(function.id);
            a.discard_lexical(function.id);
        }
        if let Some(row) = self.rows.get(&key)
            && row.work <= budget.remaining
            && equivalent(row.function, function, row.plan, plan)
        {
            budget.spend(row.work)?;
            if let Some(a) = attempt {
                a.stats.function_proofs_reused += 1;
                a.stats.region_functions_reused += 1;
                a.stats.lexical_cleanup_functions_reused += 1;
            }
            return Ok(copy_graph(&row.graph));
        }
        if let Some(a) = attempt {
            a.stats.function_proofs_checked += 1;
            a.stats.region_functions_checked += 1;
            a.stats.lexical_cleanup_functions_checked += 1;
        }
        let before = budget.remaining;
        // No earlier epoch, even one retained by VerifySession, issues this
        // template. Failure leaves the memo unchanged and preserves diagnostics.
        let graph = check_function_with_reuse(module, function, plan, budget, limits, None)?;
        self.rows.insert(
            key,
            Receipt {
                function,
                plan,
                work: before - budget.remaining,
                graph: copy_graph(&graph),
            },
        );
        Ok(graph)
    }
}

#[cfg(test)]
mod tests;
