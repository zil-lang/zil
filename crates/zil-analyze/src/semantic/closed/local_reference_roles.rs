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
    fn reference_role_input_sources(
        &self,
        world: &SemanticInstance,
        role: &ActualRole,
        active: &mut BTreeSet<zil_base::NodeId>,
        sources: &mut Vec<StorageSource>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "reference result source graph exceeds its finite depth",
            ));
        }
        b.lookup(active.len(), WORK)?;
        if !active.insert(role.statement) {
            return Err(fail("reference result source graph is cyclic"));
        }
        for write in &role.writes {
            b.spend(1, WORK)?;
            let source = match write.anchor {
                Some(original::LocalAnchor::Parameter(index)) => {
                    Some(StorageSource::Exact(StoragePosition {
                        base: StorageBase::Parameter(index),
                        path: Vec::new(),
                    }))
                }
                Some(original::LocalAnchor::Binding(statement)) if statement == role.statement => {
                    None
                }
                Some(original::LocalAnchor::Binding(statement)) => {
                    let nested = self
                        .original
                        .reference_role(world, statement, self.program, self.inventory, b)?
                        .ok_or_else(|| {
                            unknown("reference result local source needs its exact original role")
                        })?;
                    self.reference_role_input_sources(
                        world,
                        &nested,
                        active,
                        sources,
                        b,
                        depth + 1,
                    )?;
                    None
                }
                None if write.static_literal => {
                    if !original::strings::is_byte_type(&write.value.ty)
                        || !empty(&write.value.life)
                        || !write.value.classes.is_empty()
                    {
                        return Err(fail(
                            "reference static result changes its original literal domain",
                        ));
                    }
                    crate::dyn_metadata::ty(&write.value.ty, b)?;
                    Some(StorageSource::Static {
                        leaf: write.value.ty.clone(),
                        mode: StoredBorrowMode::SharedAddress,
                    })
                }
                _ => {
                    return Err(unknown(
                        "reference result source needs its complete input or static occurrence path",
                    ));
                }
            };
            if let Some(source) = source {
                crate::dyn_metadata::recipe(&source, b)?;
                for prior in sources.iter() {
                    crate::dyn_metadata::recipe(prior, b)?;
                }
                b.spend(sources.len().saturating_add(1), WORK)?;
                if !sources.contains(&source) {
                    self.push_value(sources, source, b)?;
                }
            }
        }
        active.remove(&role.statement);
        Ok(())
    }
    /// A declaration region exported through an opaque result is a new
    /// invocation role, not an equation with one of its possible input lives.
    /// The caller's original result supplies its occurrence; every callee world
    /// must independently prove the same complete role/source construction.
    pub(super) fn reference_call_result_class(
        &self,
        caller: &SemanticInstance,
        function: &TypedFunction,
        at: (BlockId, usize),
        callee: &TypedFunction,
        child: u32,
        expected: &Value,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<Class>> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "reference call result exceeds its finite role depth",
            ));
        }
        let Type::Closure(closure) = &expected.ty else {
            return Ok(None);
        };
        let entry = self
            .module
            .closures
            .get(closure.0 as usize)
            .filter(|row| row.id == *closure)
            .ok_or_else(|| fail("reference result changes its actual closure identity"))?;
        let closure_life = entry
            .storage_life
            .as_ref()
            .ok_or_else(|| unknown("reference result closure domain is unknown"))?;
        if child >= closure_life.fixed_binder_count
            || expected.classes.len() != closure_life.fixed_binder_count as usize
        {
            return Err(fail("reference result omits an intrinsic closure class"));
        }
        let header = callee
            .storage_life
            .as_ref()
            .ok_or_else(|| unknown("reference result has no complete callee header"))?;
        let mut binder = None;
        for row in &header.classes {
            b.spend(row.position.path.len().saturating_add(1), WORK)?;
            if row.position.base == StorageBase::Result
                && row.position.path.is_empty()
                && row.position.class == child
                && binder.replace(row.binder).is_some()
            {
                return Err(fail(
                    "reference result repeats a full result class occurrence",
                ));
            }
        }
        let Some(binder) = binder else {
            return Ok(None);
        };
        let worlds = self.function_worlds(callee.id, b)?;
        if worlds.is_empty() {
            return Err(fail("reference result has no actual callee world"));
        }
        for world in worlds {
            let Class::Implicit(owner, ordinal) = self.header_class(world, binder, b)? else {
                return Ok(None);
            };
            if owner != world.id {
                return Ok(None);
            }
            let mut role = None;
            for statement in self.original.reference_bindings(world.definition, b)? {
                let candidate = self
                    .original
                    .reference_role(world, statement, self.program, self.inventory, b)?
                    .ok_or_else(|| fail("reference result loses its original declaration role"))?;
                if candidate.ordinal == ordinal && role.replace(candidate).is_some() {
                    return Err(fail(
                        "reference result has competing original declaration roles",
                    ));
                }
            }
            let Some(role) = role else {
                return Ok(None);
            };
            let local = self.anchored_local(
                world,
                callee,
                original::LocalAnchor::Binding(role.statement),
                b,
            )?;
            self.reference_role_stores(world, callee, local, &role, b, depth + 1)?;
            let mut sources = Vec::new();
            self.reference_role_input_sources(
                world,
                &role,
                &mut BTreeSet::new(),
                &mut sources,
                b,
                depth + 1,
            )?;
            if sources.is_empty() {
                return Err(fail(
                    "reference result has no independently proved source upper",
                ));
            }
            let bounds = callee
                .callable_return_contract
                .as_ref()
                .and_then(|contract| contract.capture_sources.as_ref())
                .ok_or_else(|| {
                    unknown("reference result has no complete escaping capture source directory")
                })?;
            let mut captures = 0usize;
            for row in &closure_life.bindings {
                b.spend(row.position.path.len().saturating_add(1), WORK)?;
                if row.binder != StorageBinder::Fixed(child) {
                    continue;
                }
                let StorageBase::Capture(capture) = row.position.base else {
                    continue;
                };
                if !row.position.path.is_empty() {
                    return Err(unknown(
                        "reference result requires its exact projected capture occurrence",
                    ));
                }
                if entry.captures.get(capture as usize) != Some(&role.value.ty) {
                    return Err(fail(
                        "reference result capture changes its full original reference type",
                    ));
                }
                let mut found = None;
                for bound in bounds {
                    b.spend(bound.position.path.len().saturating_add(1), WORK)?;
                    if bound.position == row.position && found.replace(bound).is_some() {
                        return Err(fail(
                            "reference result repeats an escaping capture source upper",
                        ));
                    }
                }
                let bound = found.ok_or_else(|| {
                    fail("reference result omits its escaping capture source upper")
                })?;
                for source in &sources {
                    crate::dyn_metadata::recipe(source, b)?;
                    for actual in &bound.sources {
                        crate::dyn_metadata::recipe(actual, b)?;
                    }
                    b.spend(bound.sources.len().saturating_add(1), WORK)?;
                    if !bound.sources.contains(source) {
                        return Err(fail(
                            "reference result source upper omits an original assignment alternative",
                        ));
                    }
                }
                captures += 1;
            }
            if captures == 0 {
                return Err(fail(
                    "reference result role is not carried by a real closure capture",
                ));
            }
        }
        // This existing independent consumer traces the actual returned
        // captures through CFG writes. Mutable memory leaves a mandatory
        // pending proof for stored-flow; no loan or cleanup is cached here.
        crate::retained::check_sources(self.module, callee, b)?;
        let original = self.static_call_result(caller, at.0, at.1, b)?;
        if !original.equal_paid(expected, b)?
            || function.values.get(
                function
                    .blocks
                    .get(at.0.0 as usize)
                    .and_then(|block| block.instructions.get(at.1))
                    .and_then(|instruction| instruction.result)
                    .ok_or_else(|| fail("reference call has no actual result SSA"))?
                    .0 as usize,
            ) != Some(&expected.ty)
        {
            return Err(fail(
                "reference result role changes its exact original caller receipt",
            ));
        }
        Ok(Some(
            expected
                .classes
                .get(child as usize)
                .ok_or_else(|| fail("reference caller omits its complete result role"))?
                .clone(),
        ))
    }
}
