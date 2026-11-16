//! Actual shared-reference local roles. A declaration region labels the slot;
//! every concrete Store still needs an independently reconstructed source.
//! This supplies no alias equality, initializedness, loan or cleanup authority.
use super::*;
use crate::semantic::original::local_reference_roles::{ActualRole, ActualWrite};
use crate::semantic::value::{Class, Value};

impl Check<'_, '_> {
    fn reference_role_for_local(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        local: LocalId,
        b: &mut Budget,
    ) -> CompileResult<Option<ActualRole>> {
        let mut binding = None;
        for statement in self.original.reference_bindings(instance.definition, b)? {
            let actual = self.anchored_local(
                instance,
                function,
                original::LocalAnchor::Binding(statement),
                b,
            )?;
            if actual == local && binding.replace(statement).is_some() {
                return Err(fail(
                    "one actual reference local has competing original declaration roles",
                ));
            }
        }
        let Some(statement) = binding else {
            return Ok(None);
        };
        if self.anchored_local(
            instance,
            function,
            original::LocalAnchor::Binding(statement),
            b,
        )? != local
        {
            return Err(fail(
                "reference role leaves its exact original declaration local",
            ));
        }
        self.original
            .reference_role(instance, statement, self.program, self.inventory, b)
    }
    fn reference_write_source(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        id: ValueId,
        write: &ActualWrite,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<Value>> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "reference role source exceeds its finite actual chain",
            ));
        }
        let (block, index, instruction) = self.defining(function, id, b)?;
        if write.anchor.is_some()
            && let InstructionKind::ReborrowAddress {
                address,
                mutable: false,
            } = instruction.kind
        {
            operations::available_before(
                function,
                (block, index, SemanticSiteEdge::Before),
                address,
                b,
            )?;
            let parent = function
                .values
                .get(address.0 as usize)
                .ok_or_else(|| fail("reference reborrow has no actual parent type"))?;
            let output = function
                .values
                .get(id.0 as usize)
                .ok_or_else(|| fail("reference reborrow has no complete result type"))?;
            if !crate::semantic::geometry::same_type(parent, output, b)?
                || !matches!(parent, Type::Address { mutable: false, .. })
            {
                return Err(fail(
                    "reference role reborrow changes its full shared parent type",
                ));
            }
            // This is a directed use of this real parent, never equality of
            // its fresh loan/occurrence with the role or another source.
            return self.reference_write_source(instance, function, address, write, b, depth + 1);
        }
        if let Some(anchor) = write.anchor {
            match anchor {
                original::LocalAnchor::Environment(ordinal) => {
                    let capture = self.environment_capture(instance, function, ordinal, b)?;
                    if !matches!(instruction.kind, InstructionKind::ReadCapture { index } | InstructionKind::TakeCapture { index } if index == capture)
                    {
                        return Ok(None);
                    }
                }
                anchor => {
                    let local = self.anchored_local(instance, function, anchor, b)?;
                    if !matches!(instruction.kind, InstructionKind::Load(actual) | InstructionKind::TakeLocal { local: actual } if actual == local)
                    {
                        return Ok(None);
                    }
                    operations::initialized_local_before(function, (block, index), local, b)?;
                    if let Some(source) =
                        self.reference_role_for_local(instance, function, local, b)?
                    {
                        // Simultaneous role equations are checked at all real
                        // declarations/Stores. An initialized read preserves
                        // that exact source role, including through a backedge.
                        // No mutable protocol or loan fact is taken from it.
                        return Ok(Some(source.value));
                    }
                }
            }
        }
        Ok(Some(self.ssa_value(
            instance,
            function,
            id,
            b,
            depth + 1,
        )?))
    }
    fn reference_role_stores(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        local: LocalId,
        role: &ActualRole,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "reference local role exceeds its finite proof depth",
            ));
        }
        let actual = function
            .locals
            .get(local.0 as usize)
            .filter(|row| row.id == local)
            .ok_or_else(|| fail("reference role has no exact actual local"))?;
        if !actual.mutable
            || !crate::semantic::geometry::same_type(&actual.ty, &role.value.ty, b)?
            || !matches!(actual.ty, Type::Address { mutable: false, .. })
        {
            return Err(fail(
                "reference role changes its actual mutable shared-reference binding",
            ));
        }
        let [head] = role.value.life.slots.as_slice() else {
            return Err(fail("reference role has no unique complete outer head"));
        };
        if !head.path.is_empty()
            || head.mode != StoredBorrowMode::SharedAddress
            || role.value.classes.get(head.binder as usize)
                != Some(&Class::Implicit(instance.id, role.ordinal))
        {
            return Err(fail(
                "reference role changes its exact original declaration region",
            ));
        }
        let target = self.pointee(&role.value, b)?;
        let mut stores = 0usize;
        for block in &function.blocks {
            for (index, instruction) in block.instructions.iter().enumerate() {
                b.spend(1, WORK)?;
                let InstructionKind::Store {
                    local: destination,
                    value,
                } = instruction.kind
                else {
                    continue;
                };
                if destination != local {
                    continue;
                }
                stores += 1;
                operations::available_before(
                    function,
                    (block.id, index, SemanticSiteEdge::Before),
                    value,
                    b,
                )?;
                let source_type = function
                    .values
                    .get(value.0 as usize)
                    .ok_or_else(|| fail("reference role Store has no actual SSA type"))?;
                if !crate::semantic::geometry::same_type(source_type, &actual.ty, b)? {
                    return Err(fail(
                        "reference role Store changes its complete physical address type",
                    ));
                }
                let mut matched = false;
                for write in &role.writes {
                    b.spend(1, WORK)?;
                    let Some(source) = self.reference_write_source(
                        instance,
                        function,
                        value,
                        write,
                        b,
                        depth + 1,
                    )?
                    else {
                        continue;
                    };
                    if !source.equal_paid(&write.value, b)? {
                        continue;
                    }
                    if !self.pointee(&source, b)?.equal_paid(&target, b)? {
                        return Err(fail(
                            "reference role Store changes its full original pointee domain",
                        ));
                    }
                    matched = true;
                    break;
                }
                if !matched {
                    let mut errors = fail("reference role Store has no exact original RHS source");
                    b.spend(
                        role.writes.len().saturating_mul(16).saturating_add(128),
                        WORK,
                    )?;
                    let sources: Vec<_> = role
                        .writes
                        .iter()
                        .map(|write| (write.statement, write.expression))
                        .collect();
                    errors[0].notes.push(format!("instance {:?}, declaration {:?}, local {:?}, block {:?}, instruction {}, SSA {:?}; original RHS identities {:?}", instance.id, role.statement, local, block.id, index, value, sources));
                    return Err(errors);
                }
            }
        }
        if stores == 0 {
            return Err(fail(
                "initialized original reference role has no actual Store",
            ));
        }
        Ok(())
    }
    pub(super) fn check_reference_declaration(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        local: LocalId,
        b: &mut Budget,
    ) -> CompileResult<()> {
        if let Some(role) = self.reference_role_for_local(instance, function, local, b)? {
            self.reference_role_stores(instance, function, local, &role, b, 0)?;
        }
        Ok(())
    }
    pub(super) fn reference_store_value(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        local: LocalId,
        b: &mut Budget,
    ) -> CompileResult<Option<Value>> {
        if !matches!(
            function.locals.get(local.0 as usize).map(|row| &row.ty),
            Some(Type::Address { mutable: false, .. })
        ) {
            return Ok(None);
        }
        let Some(role) = self.reference_role_for_local(instance, function, local, b)? else {
            return Ok(None);
        };
        self.reference_role_stores(instance, function, local, &role, b, 0)?;
        Ok(Some(role.value))
    }
    pub(super) fn reference_local_value(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        local: LocalId,
        at: (BlockId, usize),
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<Value>> {
        b.spend(1, WORK)?;
        if !matches!(
            function.locals.get(local.0 as usize).map(|row| &row.ty),
            Some(Type::Address { mutable: false, .. })
        ) {
            return Ok(None);
        }
        let Some(role) = self.reference_role_for_local(instance, function, local, b)? else {
            return Ok(None);
        };
        operations::initialized_local_before(function, at, local, b)?;
        self.reference_role_stores(instance, function, local, &role, b, depth + 1)?;
        Ok(Some(role.value))
    }
}
