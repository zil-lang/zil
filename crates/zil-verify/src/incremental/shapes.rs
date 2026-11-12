//! Independent raw type-shape obligations, before any raw fingerprinting.
//!
//! A receipt binds the complete return/local/SSA type projection, selected
//! target and all limits. It also binds the original budget state read by this
//! stage. No producer key or claim participates. Pointer-keyed array roots are
//! retained as current function locations, never as historical addresses.
//! POD carrier registration remains on the complete original path until its
//! own current-address inventory can be reconstructed independently.
use super::*;

macro_rules! flags {
    ($($name:ident),* $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(super) struct Flags { $($name: bool,)* }
        impl Flags {
            pub(super) fn read(b: &Budget) -> Self { Self { $($name: b.$name,)* } }
            pub(super) fn apply(self, b: &mut Budget) { $(b.$name = self.$name;)* }
        }
    }
}
flags!(
    static_utf8,
    pod_stores,
    inline_store_addresses,
    inline_store_package_addresses,
    sums,
    tasks,
    task_origin_roots,
    borrowed_task_ports,
    registrations,
    async_registrations,
    protocol_contracts,
    protocol_checked,
    replacements,
    borrowed_function_pointers,
    invocation_callbacks,
    owning_address_captures,
    head_descriptions,
    stored_enum_addresses,
    task_borrowed_results,
    store_option_results,
    qualified_package_returns,
    byte_queries,
    shared_byte_borrow,
    ordinary_error_places,
    byte_query_presence_checked,
    stored_borrows,
    stored_package_instance,
    stored_slot_updates,
    stored_exclusive_shapes,
    stored_constructor_sites,
    stored_exclusive_loans,
    generic_mutex,
    pure_nominal,
    stored_context,
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Context {
    flags: Flags,
    function_count: usize,
    current_function: Option<usize>,
    sum_length: usize,
    sum_bit: bool,
    array_roots: usize,
}
impl Context {
    pub(super) fn read(b: &Budget) -> Option<Self> {
        // Carrier registration reads a mutable per-request pointer inventory.
        // Never transfer its historical keys, successful values or fees.
        if b.pod_stores || b.pod_carriers.is_some() {
            return None;
        }
        Some(Self {
            flags: Flags::read(b),
            function_count: b.function_count,
            current_function: b.current_function,
            sum_length: b.sum_functions.len(),
            sum_bit: b
                .current_function
                .and_then(|i| b.sum_functions.get(i))
                .copied()
                .unwrap_or(false),
            array_roots: b.array_snapshot_roots.len(),
        })
    }
    pub(super) fn apply(self, b: &mut Budget) {
        self.flags.apply(b);
        if self.sum_length != 0 {
            if b.sum_functions.is_empty() {
                b.sum_functions = vec![false; self.sum_length];
            }
            if let Some(index) = self.current_function {
                b.sum_functions[index] = self.sum_bit;
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Location {
    Return,
    Local(usize),
    Value(usize),
    Instruction(usize, usize),
}
impl Location {
    fn get(self, f: &TypedFunction) -> &Type {
        match self {
            Self::Return => &f.return_type,
            Self::Local(i) => &f.locals[i].ty,
            Self::Value(i) => &f.values[i],
            Self::Instruction(block, instruction) => {
                match &f.blocks[block].instructions[instruction].kind {
                    InstructionKind::ConvertInteger { target, .. }
                    | InstructionKind::ConvertNumber { target, .. } => target,
                    InstructionKind::OwnedStoreNew { element } => element,
                    _ => unreachable!("complete current instruction equality bound this type root"),
                }
            }
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) struct ArrayRoot {
    location: Location,
    was_present: bool,
}

pub(super) struct Receipt {
    before: Context,
    after: Context,
    work: usize,
    array_roots: Vec<ArrayRoot>,
}

pub(super) fn same_types(old: &TypedFunction, current: &TypedFunction) -> bool {
    // The old side is a retained successful bounded tree. Distinct-allocation
    // equality cannot descend beyond its checked shape; same allocation is
    // protected by SharedVec's immutable strong owner/COW implementation.
    old.return_type == current.return_type
        && old.values == current.values
        && (old.locals.ptr_eq(&current.locals)
            || old.locals.len() == current.locals.len()
                && old
                    .locals
                    .iter()
                    .zip(&current.locals)
                    .all(|(a, b)| a.ty == b.ty))
}

pub(super) fn roots_match(roots: &[ArrayRoot], f: &TypedFunction, b: &Budget) -> bool {
    let mut added = BTreeSet::new();
    for root in roots {
        let key = root.location.get(f) as *const Type as usize;
        let present = b.array_snapshot_roots.contains(&key) || added.contains(&key);
        if present != root.was_present {
            return false;
        }
        added.insert(key);
    }
    true
}
pub(super) fn apply_roots(roots: &[ArrayRoot], f: &TypedFunction, b: &mut Budget) {
    for root in roots {
        b.array_snapshot_roots
            .insert(root.location.get(f) as *const Type as usize);
    }
}
impl Receipt {
    fn apply(&self, f: &TypedFunction, b: &mut Budget) -> Check<()> {
        b.spend(self.work)?;
        self.after.apply(b);
        apply_roots(&self.array_roots, f, b);
        Ok(())
    }
}

impl Attempt<'_> {
    pub(crate) fn check_shapes(
        &mut self,
        module: &TypedModule,
        f: &TypedFunction,
        b: &mut Budget,
        limits: &Limits,
    ) -> Check<()> {
        let before = Context::read(b);
        if let Some(before) = before
            && let Some(proof) = self
                .previous_shapes
                .get(f.id.0 as usize)
                .and_then(Option::as_ref)
            && proof.before == before
            && proof.work <= b.remaining
            && self.previous_inputs.is_some_and(|old| {
                old.limits() == limits
                    && old.module().target == module.target
                    && old
                        .module()
                        .functions
                        .get(f.id.0 as usize)
                        .is_some_and(|old| same_types(old, f))
            })
            && roots_match(&proof.array_roots, f, b)
        {
            proof.apply(f, b)?;
            let index = f.id.0 as usize;
            if !self
                .next_shapes
                .get(index)
                .and_then(Option::as_ref)
                .is_some_and(|row| Arc::ptr_eq(row, proof))
                && let Some(row) = self.next_shapes.get_mut(index)
            {
                *row = Some(proof.clone());
            }
            self.stats.raw_shape_functions_reused += 1;
            return Ok(());
        }
        if let Some(row) = self.next_shapes.get_mut(f.id.0 as usize) {
            *row = None;
        }
        self.stats.raw_shape_functions_checked += 1;
        let remaining = b.remaining;
        // A short residual balance runs this exact original order rather than
        // an aggregate rejection. Failed checks publish neither facts nor fees.
        let roots = check_current(module, f, b, limits, before.is_some())?;
        if let (Some(before), Some(after)) = (before, Context::read(b)) {
            if let Some(row) = self.next_shapes.get_mut(f.id.0 as usize) {
                *row = Some(Arc::new(Receipt {
                    before,
                    after,
                    work: remaining - b.remaining,
                    array_roots: roots,
                }));
            }
        }
        Ok(())
    }
}

pub(crate) fn check_current(
    module: &TypedModule,
    f: &TypedFunction,
    b: &mut Budget,
    limits: &Limits,
    record: bool,
) -> Check<Vec<ArrayRoot>> {
    let mut roots = Vec::new();
    for (location, ty) in std::iter::once((Location::Return, &f.return_type))
        .chain(
            f.locals
                .iter()
                .enumerate()
                .map(|(i, local)| (Location::Local(i), &local.ty)),
        )
        .chain(
            f.values
                .iter()
                .enumerate()
                .map(|(i, ty)| (Location::Value(i), ty)),
        )
    {
        check_type_root(ty, location, &module.target, limits, b, record, &mut roots)?;
        if b.pod_stores {
            dynamic_store::register_actual_carrier_root(module, ty, b)?;
        }
    }
    Ok(roots)
}

pub(super) fn check_type_root(
    ty: &Type,
    location: Location,
    target: &TargetProfile,
    limits: &Limits,
    b: &mut Budget,
    record: bool,
    roots: &mut Vec<ArrayRoot>,
) -> Check<()> {
    let key = ty as *const Type as usize;
    let was_present = record && b.array_snapshot_roots.contains(&key);
    check_type_shape(ty, target, limits, b)?;
    if record && b.array_snapshot_roots.contains(&key) {
        roots.push(ArrayRoot {
            location,
            was_present,
        });
    }
    Ok(())
}
