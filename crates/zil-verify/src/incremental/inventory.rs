//! Raw instruction inventory is a separately sealed obligation. Names, dense
//! function IDs and module aggregate counts stay at their original positions.
//! The current instruction count and budget balance must both fit before a
//! successful receipt can replace the original ordered walk.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Context {
    shape: shapes::Context,
    scoped: Option<(usize, bool)>,
}
impl Context {
    fn read(function: &TypedFunction, b: &Budget) -> Option<Self> {
        Some(Self {
            shape: shapes::Context::read(b)?,
            scoped: b
                .scoped_functions
                .as_ref()
                .map(|rows| (rows.len(), rows[function.id.0 as usize])),
        })
    }
    fn apply(self, function: &TypedFunction, b: &mut Budget) {
        self.shape.apply(b);
        if let Some((len, marked)) = self.scoped {
            if b.scoped_functions.is_none() {
                b.scoped_functions = Some(vec![false; len]);
            }
            b.scoped_functions.as_mut().unwrap()[function.id.0 as usize] = marked;
        }
    }
}

pub(super) struct Receipt {
    before: Context,
    after: Context,
    work: usize,
    instructions: usize,
    roots: Vec<shapes::ArrayRoot>,
}

impl Attempt<'_> {
    pub(super) fn check_inventory(
        &mut self,
        module: &TypedModule,
        function: &TypedFunction,
        b: &mut Budget,
        limits: &Limits,
        instruction_count: &mut usize,
    ) -> Check<()> {
        let before = Context::read(function, b);
        let same = self.function_matches(function, &module.target, limits);
        if let Some(before) = before
            && same
            && let Some(proof) = self
                .previous_inventory
                .get(function.id.0 as usize)
                .and_then(Option::as_ref)
            && proof.before == before
            && proof.work <= b.remaining
            && let Some(count) = instruction_count.checked_add(proof.instructions)
            && count <= limits.max_instructions
            && shapes::roots_match(&proof.roots, function, b)
        {
            b.spend(proof.work)?;
            proof.after.apply(function, b);
            shapes::apply_roots(&proof.roots, function, b);
            *instruction_count = count;
            let index = function.id.0 as usize;
            if !self
                .next_inventory
                .get(index)
                .and_then(Option::as_ref)
                .is_some_and(|row| Arc::ptr_eq(row, proof))
                && let Some(row) = self.next_inventory.get_mut(index)
            {
                *row = Some(proof.clone());
            }
            self.stats.raw_inventory_functions_reused += 1;
            return Ok(());
        }
        if let Some(row) = self.next_inventory.get_mut(function.id.0 as usize) {
            *row = None;
        }
        self.stats.raw_inventory_functions_checked += 1;
        let remaining = b.remaining;
        let count = *instruction_count;
        let roots = check_current(
            module,
            function,
            b,
            limits,
            instruction_count,
            before.is_some(),
        )?;
        if let (Some(before), Some(after)) = (before, Context::read(function, b)) {
            if let Some(row) = self.next_inventory.get_mut(function.id.0 as usize) {
                *row = Some(Arc::new(Receipt {
                    before,
                    after,
                    work: remaining - b.remaining,
                    instructions: *instruction_count - count,
                    roots,
                }));
            }
        }
        Ok(())
    }
}

pub(super) fn check_current(
    module: &TypedModule,
    function: &TypedFunction,
    budget: &mut Budget,
    limits: &Limits,
    instruction_count: &mut usize,
    record: bool,
) -> Check<Vec<shapes::ArrayRoot>> {
    let index = function.id.0 as usize;
    let mut roots = Vec::new();
    for (block_index, block) in function.blocks.iter().enumerate() {
        *instruction_count = add_count(
            *instruction_count,
            block.instructions.len(),
            limits.max_instructions,
            "instruction",
        )?;
        budget.spend(1)?;
        for (instruction_index, instruction) in block.instructions.iter().enumerate() {
            budget.spend(1)?;
            budget.shared_byte_borrow |= matches!(
                instruction.kind,
                InstructionKind::ByteBufferBorrow { mutable: false, .. }
            );
            budget.stored_slot_updates |= matches!(
                instruction.kind,
                InstructionKind::StoreStoredBorrow { .. }
                    | InstructionKind::LoadStoredBorrow {
                        mode: StoredBorrowLoad::Take,
                        ..
                    }
            );
            if budget.stored_exclusive_shapes
                && matches!(
                    instruction.kind,
                    InstructionKind::MakeAggregate { .. } | InstructionKind::MakePackage { .. }
                )
                && let Some(result) = instruction.result
                && let Some(Type::BorrowedStorage { life, .. }) =
                    function.values.get(result.0 as usize)
            {
                budget.spend(life.slots.len())?;
                budget.stored_constructor_sites |= life
                    .slots
                    .iter()
                    .any(|slot| slot.mode == StoredBorrowMode::ExclusiveAddress);
            }
            match &instruction.kind {
                InstructionKind::ReservePlaceLocal { path, .. }
                | InstructionKind::ReservePlaceAddress { path, .. }
                | InstructionKind::TakeTupleField { path, .. }
                | InstructionKind::TakeVariantField { path, .. }
                | InstructionKind::BorrowPlaceLocal { path, .. }
                | InstructionKind::PlaceTagLocal { path, .. }
                | InstructionKind::BorrowPlaceAddress { path, .. }
                | InstructionKind::PlaceTagAddress { path, .. } => {
                    if path.len() > limits.max_call_depth {
                        return Err(limit(
                            "mixed/replacement path exceeds depth limit before fingerprint",
                        ));
                    }
                    budget.spend(
                        path.len()
                            .checked_mul(24)
                            .and_then(|n| n.checked_add(1))
                            .ok_or_else(|| {
                                limit("mixed/replacement preflight path work overflow")
                            })?,
                    )?;
                    if matches!(
                        instruction.kind,
                        InstructionKind::ReservePlaceLocal { .. }
                            | InstructionKind::ReservePlaceAddress { .. }
                    ) {
                        budget.replacements = true;
                    }
                }
                InstructionKind::BorrowVariantLocal { path, .. }
                | InstructionKind::BorrowVariantAddress { path, .. }
                | InstructionKind::VariantTagLocal { path, .. }
                | InstructionKind::VariantTagAddress { path, .. }
                | InstructionKind::ReadVariantLocal { path, .. } => {
                    if path.len() > limits.max_call_depth {
                        return Err(limit("variant path exceeds depth limit before fingerprint"));
                    }
                    budget.spend(
                        path.len()
                            .checked_mul(24)
                            .and_then(|n| n.checked_add(1))
                            .ok_or_else(|| limit("variant preflight path work overflow"))?,
                    )?;
                }
                InstructionKind::BorrowFieldLocal { path, .. } => {
                    if path.len() > limits.max_call_depth {
                        return Err(limit("field path exceeds depth limit before fingerprint"));
                    }
                    budget.spend(
                        path.len()
                            .checked_mul(12)
                            .and_then(|n| n.checked_add(1))
                            .ok_or_else(|| limit("field preflight path work overflow"))?,
                    )?;
                }
                InstructionKind::CommitPlaceReplace { .. }
                | InstructionKind::AbortPlaceReservation { .. } => {
                    budget.replacements = true;
                }
                InstructionKind::StaticUtf8 { bytes } => {
                    budget.static_utf8 = true;
                    if bytes.len() > limits.max_nodes {
                        return Err(limit("static UTF8 byte count exceeds the raw bound"));
                    }
                    // Reserve byte validation and module fingerprint traversal first.
                    budget.spend(bytes.len().saturating_mul(2))?;
                    if std::str::from_utf8(bytes).is_err() {
                        return Err(reject(
                            "E-VERIFY-UTF8",
                            Some(instruction.span),
                            "program-static text contains invalid UTF8",
                        ));
                    }
                }
                InstructionKind::Constant(constant) => native_values::constant(
                    constant,
                    &module.target,
                    Some(instruction.span),
                    limits,
                    budget,
                )?,
                InstructionKind::MakeManaged { fields }
                | InstructionKind::MakePackage { fields }
                | InstructionKind::MakeOwnedArray { elements: fields }
                | InstructionKind::MakeAggregate { fields }
                | InstructionKind::MakeOwnedTuple { fields }
                | InstructionKind::MakeVariant { fields, .. } => budget.spend(fields.len())?,
                InstructionKind::Call { arguments, .. }
                | InstructionKind::SuspendCall { arguments, .. }
                | InstructionKind::ForeignCall { arguments, .. } => {
                    budget.spend(arguments.len())?
                }
                InstructionKind::CallClosure { arguments, .. }
                | InstructionKind::CallClosureMut { arguments, .. }
                | InstructionKind::CallClosureShared { arguments, .. }
                | InstructionKind::CallClosureOnce { arguments, .. }
                | InstructionKind::CallClosureBorrowed { arguments, .. }
                | InstructionKind::CallIndirect { arguments, .. } => {
                    budget.spend(arguments.len().saturating_add(1))?
                }
                InstructionKind::WithForeignCallback { .. } => {
                    budget.spend(3)?;
                    if budget.scoped_functions.is_none() {
                        budget.spend(budget.function_count)?;
                        budget.scoped_functions = Some(vec![false; budget.function_count]);
                    }
                    budget.scoped_functions.as_mut().unwrap()[index] = true;
                }
                InstructionKind::MakeClosure { captures, .. } => budget.spend(captures.len())?,
                InstructionKind::ConvertInteger { target, .. }
                | InstructionKind::ConvertNumber { target, .. } => {
                    // This embedded raw type is not necessarily one of the
                    // function value types. Bound it before fingerprinting.
                    shapes::check_type_root(
                        target,
                        shapes::Location::Instruction(block_index, instruction_index),
                        &module.target,
                        limits,
                        budget,
                        record,
                        &mut roots,
                    )?;
                }
                InstructionKind::OwnedStoreNew { element } => {
                    shapes::check_type_root(
                        element,
                        shapes::Location::Instruction(block_index, instruction_index),
                        &module.target,
                        limits,
                        budget,
                        record,
                        &mut roots,
                    )?;
                }
                InstructionKind::Unsupported(feature) => {
                    return Err(unsupported(
                        format!("unknown or unimplemented instruction: {feature}"),
                        Some(instruction.span),
                    ));
                }
                _ => {}
            }
        }
    }
    Ok(roots)
}
