//! One actual call-before binding for physical result data and loan ancestry.
//! The immutable frame is borrowed until every result row is prepared. Namespace
//! specific Task/Dyn/Reference after-state kernels keep their own proof rules;
//! no updated occupant can be reread while resolving this call's result.
use super::*;

/// Entry obligations distinguish a proven inactive physical arm from an
/// unresolved target. Neither case is a data permission or a new source.
pub(super) enum EntrySource {
    Active(Permission),
    Inactive,
    Unknown,
}

enum Environment<'a> {
    Function,
    Borrowed {
        captures: &'a [Type],
        receiver: Permission,
    },
    Consumed {
        captures: &'a [Type],
        payload: &'a Payload,
    },
}
pub(super) struct Before<'a> {
    module: &'a TypedModule,
    caller: &'a TypedFunction,
    state: &'a Frame,
    values: &'a [Payload],
    arguments: &'a [ValueId],
    environment: Environment<'a>,
    invocation: Option<(&'a Instruction, &'a ClosureDefinition)>,
}
impl<'a> Before<'a> {
    pub(super) fn function(
        module: &'a TypedModule,
        caller: &'a TypedFunction,
        instruction: &'a Instruction,
        values: &'a [Payload],
        state: &'a Frame,
        b: &mut Budget,
    ) -> Check<Self> {
        let (function, arguments, suspend) = match &instruction.kind {
            InstructionKind::Call {
                function,
                arguments,
            } => (*function, arguments.as_slice(), false),
            InstructionKind::SuspendCall {
                function,
                arguments,
            } => (*function, arguments.as_slice(), true),
            _ => return Err(err("physical call binding is not a real static invocation")),
        };
        let callee = module
            .functions
            .get(function.0 as usize)
            .filter(|f| f.id == function)
            .ok_or_else(|| err("call binding has no exact actual target"))?;
        if callee.foreign.is_some()
            || callee.environment.is_some()
            || callee.call_kind
                != if suspend {
                    CallKind::Suspend
                } else {
                    CallKind::Direct
                }
            || suspend && caller.call_kind != CallKind::Suspend
        {
            return Err(err(
                "stored static call changes its independently checked actual ABI",
            ));
        }
        Self::bind(
            module,
            caller,
            arguments,
            values,
            state,
            Environment::Function,
            None,
            b,
        )
    }
    /// Bind an independently selected exact target, including non-opcode
    /// cleanup/indirect consumers. This snapshot grants no invocation right.
    pub(super) fn direct(
        module: &'a TypedModule,
        caller: &'a TypedFunction,
        callee: &'a TypedFunction,
        arguments: &'a [ValueId],
        values: &'a [Payload],
        state: &'a Frame,
        b: &mut Budget,
    ) -> Check<Self> {
        let actual = module
            .functions
            .get(callee.id.0 as usize)
            .filter(|entry| entry.id == callee.id)
            .ok_or_else(|| err("direct snapshot target is absent"))?;
        if !std::ptr::eq(actual, callee) {
            return Err(err("direct snapshot target is not the actual module entry"));
        }
        if arguments.len() != callee.parameters.len() {
            return Err(err("direct snapshot changes actual target arity"));
        }
        b.spend(arguments.len())?;
        for (argument, local) in arguments.iter().zip(&callee.parameters) {
            let source = vt(caller, *argument)?;
            let target = &callee
                .locals
                .get(local.0 as usize)
                .ok_or_else(|| err("direct snapshot target parameter is absent"))?
                .ty;
            type_work(source, &b.limits.clone(), b)?;
            type_work(target, &b.limits.clone(), b)?;
            if source != target {
                return Err(err("direct snapshot changes complete actual argument type"));
            }
        }
        Self::bind(
            module,
            caller,
            arguments,
            values,
            state,
            Environment::Function,
            None,
            b,
        )
    }
    pub(super) fn closure(
        module: &'a TypedModule,
        caller: &'a TypedFunction,
        instruction: &'a Instruction,
        values: &'a [Payload],
        state: &'a Frame,
        b: &mut Budget,
    ) -> Check<Self> {
        let (definition, arguments) =
            stored_callables::called(module, caller, &instruction.kind)
                .ok_or_else(|| err("physical callback binding lacks its exact actual entry"))?;
        let entry = module
            .functions
            .get(definition.function.0 as usize)
            .filter(|f| f.id == definition.function && f.environment == Some(definition.id))
            .ok_or_else(|| {
                err("physical callback binding changes its actual environment/entry identity")
            })?;
        type_work(&entry.return_type, &b.limits.clone(), b)?;
        type_work(&definition.result, &b.limits.clone(), b)?;
        if entry.return_type != definition.result {
            return Err(err(
                "physical callback entry changes its complete result type",
            ));
        }
        let environment = if let Some(value) = captured_owner::consuming_value(&instruction.kind) {
            Environment::Consumed {
                captures: &definition.captures,
                payload: values
                    .get(value.0 as usize)
                    .ok_or_else(|| err("consumed callback environment is absent"))?,
            }
        } else {
            Environment::Borrowed {
                captures: &definition.captures,
                receiver: captured_owner::receiver(caller, instruction, values, b)?,
            }
        };
        Self::bind(
            module,
            caller,
            arguments,
            values,
            state,
            environment,
            Some((instruction, definition)),
            b,
        )
    }
    fn bind(
        module: &'a TypedModule,
        caller: &'a TypedFunction,
        arguments: &'a [ValueId],
        values: &'a [Payload],
        state: &'a Frame,
        environment: Environment<'a>,
        invocation: Option<(&'a Instruction, &'a ClosureDefinition)>,
        b: &mut Budget,
    ) -> Check<Self> {
        b.spend(arguments.len())?;
        for value in arguments {
            if values.get(value.0 as usize).is_none()
                || caller.values.get(value.0 as usize).is_none()
            {
                return Err(err(
                    "call binding operand is absent from the actual input snapshot",
                ));
            }
        }
        Ok(Self {
            module,
            caller,
            state,
            values,
            arguments,
            environment,
            invocation,
        })
    }
    pub(super) fn source(&self, source: &Source, b: &mut Budget) -> Check<Permission> {
        if source.retained_port.is_some() && matches!(source.base, HoldRoot::Capture(_)) {
            let (instruction, definition) = self
                .invocation
                .ok_or_else(|| err("captured logical source lacks its exact invocation"))?;
            return dyn_captures::substitute(
                self.module,
                self.caller,
                instruction,
                source,
                definition,
                self.arguments,
                self.values,
                self.state,
                b,
            );
        }
        if let Some(permission) = self.physical_source(source, b)? {
            return Ok(permission);
        }
        let mut result = match &self.environment {
            Environment::Function => instantiate(
                self.module,
                self.caller,
                source,
                self.arguments,
                self.values,
                self.state,
                b,
            )?,
            Environment::Borrowed { captures, receiver } => captured_owner::substitute(
                self.module,
                self.caller,
                source,
                captures,
                receiver,
                self.arguments,
                self.values,
                self.state,
                b,
            )?,
            Environment::Consumed { captures, payload } => captured_owner::substitute_value(
                self.module,
                self.caller,
                source,
                captures,
                payload,
                self.arguments,
                self.values,
                self.state,
                b,
            )?,
        };
        if result.sources.is_empty()
            && source.retained_port.is_none()
            && source.heap_element.is_none()
            && source.heap_parents.is_empty()
            && (source.leaf.is_empty() || source.inline.is_empty())
        {
            if let Some(permission) = self.input_field_permission(source, b)? {
                result = permission;
            }
        }
        if source.static_dependency == StaticProvenance::UpperOnly {
            for actual in &mut result.sources {
                actual.static_dependency = StaticProvenance::UpperOnly;
            }
        }
        Ok(result)
    }
    /// Instantiate an entry separation source against every current actual
    /// pointee. A known inactive arm imposes no separation debt; a missing
    /// active target or unknown tag cannot borrow a sibling's permission.
    pub(super) fn entry_source(&self, source: &Source, b: &mut Budget) -> Check<EntrySource> {
        let input = match source.base {
            HoldRoot::Parameter(index) => Some(option_address::Input::Parameter(index)),
            HoldRoot::Capture(index) => Some(option_address::Input::Capture(index)),
            _ => None,
        };
        if source.leaf.is_empty()
            || source.retained_port.is_some()
            || source.heap_element.is_some()
            || !source.heap_parents.is_empty()
            || input.is_none()
        {
            let permission = self.source(source, b)?;
            return Ok(if permission.sources.is_empty() {
                EntrySource::Unknown
            } else {
                EntrySource::Active(permission)
            });
        }
        let (locations, known) = self.input_locations(input.unwrap(), &[], false, b)?;
        if !known || locations.is_empty() {
            return Ok(EntrySource::Unknown);
        }
        let mut out = Permission::default();
        let mut active = false;
        for (holder, prefix, ty) in locations {
            // The typed input walker follows real Address heads to beta;
            // projecting its exact prefix transports existing facts only.
            let selected = value_transport::project(holder, &prefix, ty, b)?;
            match entry_path_active(self.module, ty, &selected, &source.leaf, b)? {
                Some(false) => continue,
                None => return Ok(EntrySource::Unknown),
                Some(true) => {}
            }
            let permission = if let Some(permission) = leaf(&selected, &source.leaf, b)? {
                Some(permission)
            } else {
                value_transport::facts(&selected, &source.leaf, b)?.map(|facts| &facts.address)
            };
            let Some(permission) = permission else {
                return Ok(EntrySource::Unknown);
            };
            if permission.sources.is_empty() {
                return Ok(EntrySource::Unknown);
            }
            permission_work(permission, b)?;
            let mut permission = permission.clone();
            for actual in &mut permission.sources {
                store_paths::compose(actual, source, b)?;
            }
            join_permission(&mut out, &permission, b)?;
            active = true;
        }
        Ok(if active {
            EntrySource::Active(out)
        } else {
            EntrySource::Inactive
        })
    }
    pub(super) fn separation(
        &self,
        left: &Source,
        right: &Source,
        b: &mut Budget,
    ) -> Check<Option<(Permission, Permission)>> {
        let left = self.entry_source(left, b)?;
        let right = self.entry_source(right, b)?;
        match (left, right) {
            (EntrySource::Unknown, _) | (_, EntrySource::Unknown) => {
                Err(err("call separation has incomplete actual target facts"))
            }
            (EntrySource::Inactive, _) | (_, EntrySource::Inactive) => Ok(None),
            (EntrySource::Active(left), EntrySource::Active(right)) => Ok(Some((left, right))),
        }
    }
    /// Missing one actual target is unknown, even when a sibling supplies
    /// the same-typed slot. This binds current memory, not a cached SSA row.
    fn select_all(
        &self,
        head: &Permission,
        path: &[PlaceProjection],
        b: &mut Budget,
    ) -> Check<Permission> {
        let mut out = Permission::default();
        for source in &head.sources {
            b.spend(width(source))?;
            if source.retained_port.is_some()
                || source.heap_element.is_some()
                || !source.heap_parents.is_empty()
            {
                return Err(err(
                    "call-before physical field requires its independent logical or heap memory route",
                ));
            }
            let Some((holder,actual))=memory_location(self.state,source,path,b)? else {
                return Ok(Permission::default());
            };
            let permission = if let Some(permission) = leaf(holder, &actual, b)? {
                Some(permission)
            } else {
                value_transport::facts(holder, &actual, b)?.map(|facts| &facts.address)
            };
            let Some(permission) = permission else {
                return Ok(Permission::default());
            };
            if permission.sources.is_empty() {
                return Ok(Permission::default());
            }
            join_permission(&mut out, permission, b)?;
        }
        Ok(out)
    }
    fn physical_source(&self, source: &Source, b: &mut Budget) -> Check<Option<Permission>> {
        if source.retained_port.is_some() || source.heap_element.is_some() {
            return Ok(None);
        }
        let mut selected = match source.base {
            HoldRoot::Parameter(index) if !source.leaf.is_empty() => {
                let input = self
                    .arguments
                    .get(index as usize)
                    .and_then(|id| self.values.get(id.0 as usize))
                    .ok_or_else(|| err("call-before field source input is absent"))?;
                let argument = *self
                    .arguments
                    .get(index as usize)
                    .ok_or_else(|| err("call-before actual field argument is absent"))?;
                let current = matches!(vt(self.caller, argument)?, Type::Address { .. });
                let mut permission = if current {
                    self.select_all(&input.address, &source.leaf, b)?
                } else if let Some(permission) = leaf(input, &source.leaf, b)? {
                    permission_work(permission, b)?;
                    permission.clone()
                } else if !input.address.sources.is_empty() {
                    self.select_all(&input.address, &source.leaf, b)?
                } else {
                    return Ok(None);
                };
                for actual in &mut permission.sources {
                    store_paths::compose(actual, source, b)?;
                }
                return Ok(Some(permission));
            }
            HoldRoot::Capture(index) => match &self.environment {
                Environment::Borrowed { captures, receiver } => {
                    let ty = captures
                        .get(index as usize)
                        .ok_or_else(|| err("call-before field capture is absent"))?;
                    if source.leaf.is_empty() && !matches!(ty, Type::Address { .. } | Type::Desc(_))
                    {
                        return Ok(None);
                    }
                    if matches!(ty, Type::Address { .. }) && !source.leaf.is_empty() {
                        let head =
                            self.select_all(receiver, &[PlaceProjection::Field(index)], b)?;
                        self.select_all(&head, &source.leaf, b)?
                    } else {
                        b.spend(source.leaf.len().saturating_add(1))?;
                        let mut path = vec![PlaceProjection::Field(index)];
                        path.extend(&source.leaf);
                        self.select_all(receiver, &path, b)?
                    }
                }
                Environment::Consumed { captures, payload } => {
                    let ty = captures
                        .get(index as usize)
                        .ok_or_else(|| err("consumed field capture is absent"))?;
                    if matches!(ty, Type::Address { .. }) && !source.leaf.is_empty() {
                        let head = leaf(payload, &[PlaceProjection::Field(index)], b)?.ok_or_else(
                            || err("consumed environment lacks its actual captured head"),
                        )?;
                        self.select_all(head, &source.leaf, b)?
                    } else {
                        return Ok(None);
                    }
                }
                Environment::Function => return Ok(None),
            },
            _ => return Ok(None),
        };
        for actual in &mut selected.sources {
            b.spend(source.inline.len())?;
            actual.inline.extend(&source.inline);
            if source.static_dependency == StaticProvenance::UpperOnly {
                actual.static_dependency = StaticProvenance::UpperOnly;
            }
        }
        Ok(Some(selected))
    }
    fn input_field_permission(&self, source: &Source, b: &mut Budget) -> Check<Option<Permission>> {
        let path = if source.inline.is_empty() {
            source.leaf.as_slice()
        } else {
            source.inline.as_slice()
        };
        let select = |payload: &Payload,
                      path: &[PlaceProjection],
                      b: &mut Budget|
         -> Check<Option<Permission>> {
            if let Some(permission) = leaf(payload, path, b)? {
                if !permission.sources.is_empty() {
                    permission_work(permission, b)?;
                    return Ok(Some(permission.clone()));
                }
            }
            if let Some(facts) = value_transport::facts(payload, path, b)? {
                let permission = &facts.address;
                if !permission.sources.is_empty() {
                    permission_work(permission, b)?;
                    return Ok(Some(permission.clone()));
                }
            }
            Ok(None)
        };
        match source.base {
            HoldRoot::Parameter(index) => {
                let Some(value) = self
                    .arguments
                    .get(index as usize)
                    .and_then(|id| self.values.get(id.0 as usize))
                else {
                    return Ok(None);
                };
                if path.is_empty() {
                    return Ok(None);
                }
                select(value, path, b)
            }
            HoldRoot::Capture(index) => match &self.environment {
                Environment::Consumed { payload, .. } => {
                    b.spend(path.len().saturating_add(1))?;
                    let mut target = vec![PlaceProjection::Field(index)];
                    target.extend(path);
                    select(payload, &target, b)
                }
                Environment::Borrowed { receiver, .. } => {
                    let mut out = Permission::default();
                    let mut found = false;
                    for origin in &receiver.sources {
                        if !origin.leaf.is_empty()
                            || origin.retained_port.is_some()
                            || origin.heap_element.is_some()
                        {
                            return Err(err("field input receiver has no exact inline route"));
                        }
                        let holder = memory_holder(self.state, origin.base, b)?
                            .ok_or_else(|| err("field input receiver holder is absent"))?;
                        b.spend(
                            origin
                                .inline
                                .len()
                                .saturating_add(path.len())
                                .saturating_add(1),
                        )?;
                        let mut target = origin.inline.clone();
                        target.push(PlaceProjection::Field(index));
                        target.extend(path);
                        if let Some(actual) = select(holder, &target, b)? {
                            found = true;
                            join_permission(&mut out, &actual, b)?;
                        } else {
                            return Ok(None);
                        }
                    }
                    Ok(found.then_some(out))
                }
                Environment::Function => Ok(None),
            },
            _ => Ok(None),
        }
    }
    pub(super) fn parents(
        &self,
        summary: &Summary,
        path: &[PlaceProjection],
        permission: &mut Permission,
        b: &mut Budget,
    ) -> Check<bool> {
        parents_at(summary, path, permission, b, |source, b| {
            self.source(source, b)
        })
    }
    pub(super) fn receiver(&self) -> Check<&Permission> {
        match &self.environment {
            Environment::Borrowed { receiver, .. } => Ok(receiver),
            _ => Err(err(
                "consuming callback result retains its retired invocation head",
            )),
        }
    }
}

/// Authenticate the destination against the actual full return type. Public
/// class selectors never enter this physical walk, and only the outer returned
/// address can expose its independently checked pointee directory.
pub(super) fn destination<'a>(
    module: &'a TypedModule,
    ty: &'a Type,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<&'a Type> {
    if let Type::Closure(id) = ty {
        if let Some((_, leaf)) = returned_environment::leaves(module, *id, b)?
            .into_iter()
            .find(|(key, _)| key.as_slice() == path)
        {
            return Ok(leaf);
        }
    }
    if let Some((storage, life)) = qualified(ty).or_else(|| addressed(ty)) {
        b.spend(
            life.slots
                .len()
                .saturating_mul(path.len().saturating_add(1)),
        )?;
        if life.slots.iter().any(|row| row.path == path) {
            return crate::dynamic_ports::physical_path(storage, path, b);
        }
    }
    b.spend(path.len().saturating_add(1))?;
    let mut current = ty;
    if !path.is_empty()
        && let Type::Address { inner, .. } = current
    {
        current = inner;
    }
    for step in path {
        while let Type::BorrowedStorage { storage, .. } = current {
            current = storage;
        }
        current = match (current, step) {
            (Type::Closure(id), PlaceProjection::Field(index)) => module
                .closures
                .get(id.0 as usize)
                .filter(|d| d.id == *id)
                .and_then(|d| d.captures.get(*index as usize)),
            (Type::Tuple(fields), PlaceProjection::Field(index)) => fields.get(*index as usize),
            (
                Type::Record { fields, .. } | Type::Package { fields, .. },
                PlaceProjection::Field(index),
            ) => fields.get(*index as usize).map(|(_, ty)| ty),
            (Type::Array { element, length }, PlaceProjection::Field(index)) if index < length => {
                Some(element.as_ref())
            }
            (Type::Enum { variants, .. }, PlaceProjection::Variant(index)) => variants
                .get(index.variant as usize)
                .and_then(|(_, fields)| fields.get(index.index as usize)),
            (Type::ErrorUnion { error, result }, PlaceProjection::ErrorArm(failed)) => {
                Some(if *failed {
                    error.as_ref()
                } else {
                    result.as_ref()
                })
            }
            _ => None,
        }
        .ok_or_else(|| err("call result selects no actual physical destination"))?;
    }
    Ok(current)
}

pub(super) fn result_path(ty: &Type, path: &[PlaceProjection], b: &mut Budget) -> Check<()> {
    let slots = compound::slots(ty, b)?
        .ok_or_else(|| err("physical result has no complete Tuple slot directory"))?;
    b.spend(slots.len().saturating_mul(path.len().saturating_add(1)))?;
    if !slots
        .iter()
        .any(|(actual, mode)| actual == path && *mode == StoredBorrowMode::SharedAddress)
    {
        return Err(err(
            "shared result source selects no exact readonly physical slot",
        ));
    }
    Ok(())
}
/// Sources remain data; these rows record only actual loan ancestry. Empty
/// parents are known only when a real Return supplied the row for this path.
pub(super) fn return_parents(
    module: &TypedModule,
    f: &TypedFunction,
    payload: &Payload,
    solution: &Solution,
    summary: &mut Summary,
    b: &mut Budget,
) -> Check<()> {
    let shared = compound::shared_value(module, &f.return_type, Some(f.span), b)?;
    if shared {
        for (path, _) in compound::slots(&f.return_type, b)?
            .ok_or_else(|| err("shared Return lacks its full slot directory"))?
        {
            if leaf(payload, &path, b)?.is_none_or(|permission| permission.sources.is_empty()) {
                summary.complete = false;
            }
        }
    }
    for (path, permission) in &payload.leaves {
        if path.is_empty()
            || payload.enum_variants.present() && !payload.enum_variants.slot_active(path)
            || option_address::candidate(&f.return_type) && payload.option_variants.mask() == 1
        {
            continue;
        }
        if permission.sources.is_empty() {
            continue;
        }
        let parents = dyn_returns::parents(permission, &solution.loans, b)?;
        dyn_returns::check_parent_escape(f, &parents, b)?;
        b.spend(
            summary
                .physical_result_parents
                .len()
                .saturating_mul(path.len().saturating_add(1)),
        )?;
        if let Some((_, known)) = summary
            .physical_result_parents
            .iter_mut()
            .find(|(known, _)| known == path)
        {
            add_sources(known, &parents, b)?;
        } else {
            b.spend(path.len().saturating_add(1))?;
            source_work(&parents, b)?;
            summary
                .physical_result_parents
                .push((path.to_vec(), parents));
        }
        if receiver::returned_head(permission, solution, b)? {
            b.spend(
                summary
                    .receiver_results
                    .len()
                    .saturating_mul(path.len().saturating_add(1)),
            )?;
            if !summary.receiver_results.contains(path) {
                b.spend(path.len().saturating_add(1))?;
                summary.receiver_results.push(path.clone());
            }
        }
    }
    Ok(())
}
pub(super) fn parents_at(
    summary: &Summary,
    path: &[PlaceProjection],
    permission: &mut Permission,
    b: &mut Budget,
    mut actual: impl FnMut(&Source, &mut Budget) -> Check<Permission>,
) -> Check<bool> {
    if !summary.complete {
        return Ok(false);
    }
    b.spend(
        summary
            .physical_result_parents
            .len()
            .saturating_mul(path.len().saturating_add(1)),
    )?;
    let mut rows = summary
        .physical_result_parents
        .iter()
        .filter(|(known, _)| known == path);
    let (_, parents) = rows
        .next()
        .ok_or_else(|| err("physical result lacks its actual complete parent row"))?;
    if rows.next().is_some() {
        return Err(err("physical result repeats its actual parent row"));
    }
    let mut known = true;
    for parent in parents {
        let parent = actual(parent, b)?;
        if parent.sources.is_empty() {
            known = false;
        }
        // A head is an ancestor, never another data target.
        add_loans(&mut permission.loans, &parent.loans, b)?;
    }
    Ok(known)
}
fn entry_path_active(
    module: &TypedModule,
    ty: &Type,
    payload: &Payload,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<Option<bool>> {
    // Inactivity never authenticates a malformed or sibling selector.
    destination(module, ty, path, b)?;
    for (index, step) in path.iter().enumerate() {
        let prefix = &path[..index];
        let current = destination(module, ty, prefix, b)?;
        let mut storage = current;
        while let Type::BorrowedStorage { storage: inner, .. } = storage {
            storage = inner;
        }
        if matches!(
            (storage, step),
            (Type::Enum { .. }, PlaceProjection::Variant(_))
                | (Type::ErrorUnion { .. }, PlaceProjection::ErrorArm(_))
        ) {
            if option_address::signature(current) {
                match value_transport::option_at(payload, prefix, b)?.mask() {
                    0 => return Ok(None),
                    1 => return Ok(Some(false)),
                    _ => {}
                }
            } else {
                let tags = payload.enum_variants.project(prefix, b)?;
                if !tags.grounded(current, b)? {
                    return Ok(None);
                }
                if !tags.slot_active(&path[index..]) {
                    return Ok(Some(false));
                }
            }
        }
    }
    Ok(Some(true))
}

pub(super) fn path_active(
    module: &TypedModule,
    ty: &Type,
    payload: &Payload,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<bool> {
    if payload.enum_variants.present() && !payload.enum_variants.slot_active(path) {
        return Ok(false);
    }
    let mut current = ty;
    for (index, step) in path.iter().enumerate() {
        b.spend(index.saturating_add(1))?;
        // This is tag inspection of an already authenticated result path,
        // including private pointee snapshots, never a physical GEP proof.
        while let Type::Address { inner, .. } | Type::Desc(inner) = current {
            current = inner;
        }
        if option_address::signature(current)
            && value_transport::option_at(payload, &path[..index], b)?.mask() == 1
        {
            return Ok(false);
        }
        while let Type::BorrowedStorage { storage, .. } = current {
            current = storage;
        }
        current = match (current, step) {
            (Type::Tuple(fields), PlaceProjection::Field(index)) => fields.get(*index as usize),
            (
                Type::Record { fields, .. } | Type::Package { fields, .. },
                PlaceProjection::Field(index),
            ) => fields.get(*index as usize).map(|(_, ty)| ty),
            (Type::Array { element, length }, PlaceProjection::Field(index)) if index < length => {
                Some(element.as_ref())
            }
            (Type::Enum { variants, .. }, PlaceProjection::Variant(index)) => variants
                .get(index.variant as usize)
                .and_then(|(_, fields)| fields.get(index.index as usize)),
            (Type::ErrorUnion { error, result }, PlaceProjection::ErrorArm(failed)) => {
                Some(if *failed {
                    error.as_ref()
                } else {
                    result.as_ref()
                })
            }
            (Type::Closure(id), PlaceProjection::Field(index)) => module
                .closures
                .get(id.0 as usize)
                .filter(|d| d.id == *id)
                .and_then(|d| d.captures.get(*index as usize)),
            _ => None,
        }
        .ok_or_else(|| err("result tag selects no actual child"))?;
    }
    Ok(true)
}
pub(super) fn physical_slots(module: &TypedModule, ty: &Type, b: &mut Budget) -> Check<Vec<Path>> {
    let mut pending = vec![(ty, Path::new(), 0usize)];
    let mut out = vec![];
    while let Some((ty, path, depth)) = pending.pop() {
        b.spend(path.len().saturating_add(1))?;
        if depth > b.limits.max_call_depth.min(128) {
            return Err(limit("result physical directory depth exceeded"));
        }
        match ty {
            Type::Address { .. } => {
                b.spend(1)?;
                out.push(path);
            }
            Type::BorrowedStorage { life, .. } => {
                for slot in &life.slots {
                    b.spend(path.len().saturating_add(slot.path.len()).saturating_add(1))?;
                    let mut target = path.clone();
                    target.extend(&slot.path);
                    out.push(target);
                }
            }
            Type::Closure(id) => {
                for (key, ty) in returned_environment::leaves(module, *id, b)? {
                    if matches!(ty, Type::Address { .. }) {
                        b.spend(path.len().saturating_add(key.len()).saturating_add(1))?;
                        let mut target = path.clone();
                        target.extend(key);
                        out.push(target);
                    }
                }
            }
            _ => {
                let mut fields = vec![];
                match ty {
                    Type::Tuple(children) => {
                        b.spend(children.len())?;
                        fields.extend(
                            children
                                .iter()
                                .enumerate()
                                .map(|(i, t)| (PlaceProjection::Field(i as u32), t)),
                        );
                    }
                    Type::Record {
                        fields: children, ..
                    }
                    | Type::Package {
                        fields: children, ..
                    } => {
                        b.spend(children.len())?;
                        fields.extend(
                            children
                                .iter()
                                .enumerate()
                                .map(|(i, (_, t))| (PlaceProjection::Field(i as u32), t)),
                        );
                    }
                    Type::Enum { variants, .. } => {
                        for (v, (_, children)) in variants.iter().enumerate() {
                            b.spend(children.len())?;
                            fields.extend(children.iter().enumerate().map(|(i, t)| {
                                (
                                    PlaceProjection::Variant(VariantProjection {
                                        variant: v as u32,
                                        index: i as u32,
                                    }),
                                    t,
                                )
                            }));
                        }
                    }
                    Type::ErrorUnion { error, result } => {
                        b.spend(2)?;
                        fields.extend([
                            (PlaceProjection::ErrorArm(true), error.as_ref()),
                            (PlaceProjection::ErrorArm(false), result.as_ref()),
                        ]);
                    }
                    Type::Array { element, length } => {
                        b.spend(*length as usize)?;
                        fields.extend(
                            (0..*length).map(|i| (PlaceProjection::Field(i), element.as_ref())),
                        );
                    }
                    _ => {}
                }
                b.spend(fields.len().saturating_mul(path.len().saturating_add(2)))?;
                for (step, child) in fields {
                    let mut key = path.clone();
                    key.push(step);
                    pending.push((child, key, depth + 1));
                }
            }
        }
    }
    Ok(out)
}
fn structured_result(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Tuple(_)
            | Type::Record { .. }
            | Type::Package { .. }
            | Type::Enum { .. }
            | Type::ErrorUnion { .. }
            | Type::Array { .. }
            | Type::Closure(_)
            | Type::BorrowedStorage { .. }
            | Type::Managed(_)
            | Type::Dyn { .. }
            | Type::OwnedDyn { .. }
            | Type::Desc(_)
    )
}
#[allow(clippy::too_many_arguments)]
pub(super) fn call_result(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &mut Frame,
    values: &[Payload],
    summaries: &[Summary],
    sites: &WriteSites,
    loans: &mut [Loan],
    out: &mut Payload,
    b: &mut Budget,
) -> Check<bool> {
    prepare_result(
        module, f, i, state, values, summaries, sites, loans, out, false, b,
    )
}
#[allow(clippy::too_many_arguments)]
pub(super) fn call_result_with_dyn(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &mut Frame,
    values: &[Payload],
    summaries: &[Summary],
    sites: &WriteSites,
    dyn_sites: &mut dyn_leases::Sites,
    loans: &mut Vec<Loan>,
    out: &mut Payload,
    b: &mut Budget,
) -> Check<bool> {
    if !prepare_result(
        module, f, i, state, values, summaries, sites, loans, out, true, b,
    )? {
        return Ok(false);
    }
    let (definition, _) = stored_callables::called(module, f, &i.kind)
        .ok_or_else(|| err("structured result lost its actual closure"))?;
    let summary = &summaries[definition.function.0 as usize];
    if summary.complete && !summary.dyn_ports.0.is_empty() {
        let before = Before::closure(module, f, i, values, state, b)?;
        dyn_ports::call_with(
            module,
            &module.functions[definition.function.0 as usize],
            i,
            summary,
            state,
            out,
            dyn_sites,
            loans,
            b,
            |source, b| before.source(source, b),
        )?;
    }
    Ok(true)
}
#[allow(clippy::too_many_arguments)]
fn prepare_result(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &mut Frame,
    values: &[Payload],
    summaries: &[Summary],
    sites: &WriteSites,
    loans: &mut [Loan],
    out: &mut Payload,
    allow_dyn: bool,
    b: &mut Budget,
) -> Check<bool> {
    let Some((definition, _arguments)) = stored_callables::called(module, f, &i.kind) else {
        return Ok(false);
    };
    let shared = compound::shared_value(module, &definition.result, Some(i.span), b)?;
    if !shared
        && (!structured_result(&definition.result) || stored_callables::task_result(definition))
    {
        return Ok(false);
    }
    if !shared {
        crate::dynamic_witness::structural_actual(
            module,
            &definition.result,
            DynStructuralBound::Value,
            b,
        )?;
    }
    let result = i
        .result
        .ok_or_else(|| err("structured callback has no actual result"))?;
    type_work(&definition.result, &b.limits.clone(), b)?;
    type_work(vt(f, result)?, &b.limits.clone(), b)?;
    if vt(f, result)? != &definition.result {
        return Err(err(
            "structured callback changes its complete actual result type",
        ));
    }
    let summary = summaries
        .get(definition.function.0 as usize)
        .ok_or_else(|| err("structured callback summary is absent"))?;
    if !summary.complete {
        state.pending_external_update = true;
        return Ok(true);
    }
    if !allow_dyn && !summary.dyn_ports.0.is_empty() {
        return Err(err(
            "structured callback Dyn result requires its complete retained-port consumer",
        ));
    }
    let before = Before::closure(module, f, i, values, state, b)?;
    let mut prepared = Payload::default();
    let mut complete = true;
    let (tags, known) = before.enum_result(&summary.returned_enum_variants, b)?;
    prepared.enum_variants = tags;
    complete &= known;
    let (option, known) = summary
        .returned_variants
        .substitute_with(b, |input, path, b| before.input_option(input, path, b))?;
    prepared.option_variants = option;
    complete &= known;
    complete &= before.field_results(
        summary,
        &definition.result,
        i,
        sites,
        loans,
        &mut prepared,
        b,
    )?;
    let required = physical_slots(module, &definition.result, b)?;
    for path in &required {
        if !path_active(module, &definition.result, &prepared, path, b)? {
            continue;
        }
        b.spend(
            summary
                .result
                .len()
                .saturating_mul(path.len().saturating_add(1)),
        )?;
        if !summary.result.iter().any(|(known, _)| known == path) {
            complete = false;
        }
    }
    for (path, sources) in &summary.result {
        let descriptor = matches!(
            destination(module, &definition.result, path, b)?,
            Type::Desc(_)
        );
        if !path_active(module, &definition.result, &prepared, path, b)? {
            continue;
        }
        let mut permission = Permission::default();
        for source in sources {
            let actual = if descriptor {
                before.descriptor_source(source, b)?
            } else {
                before.source(source, b)?
            };
            if actual.sources.is_empty() {
                complete = false;
            }
            join_permission(&mut permission, &actual, b)?;
        }
        if !path.is_empty() {
            complete &= before.parents(summary, path, &mut permission, b)?;
        } else {
            for parent in &summary.dyn_result_parents {
                let actual = before.source(parent, b)?;
                if actual.sources.is_empty() {
                    complete = false;
                }
                add_loans(&mut permission.loans, &actual.loans, b)?;
            }
        }
        if permission.sources.is_empty() {
            complete = false;
        }
        b.spend(
            summary
                .receiver_results
                .len()
                .saturating_mul(path.len().saturating_add(1)),
        )?;
        if summary.receiver_results.contains(path) {
            let head = retain_written_slot(
                before.receiver()?,
                i,
                receiver::HEAD_SITE,
                &Vec::new(),
                sites,
                loans,
                b,
            )?;
            add_loans(&mut permission.loans, &head.loans, b)?;
        }
        if path.is_empty() {
            prepared.address = permission;
        } else {
            add_leaf(&mut prepared, path, &permission, b)?;
        }
    }
    for (path, sources) in &summary.task_ports {
        let mut permission = Permission::default();
        for source in sources {
            let actual = before.source(source, b)?;
            if actual.sources.is_empty() {
                complete = false;
            }
            join_permission(&mut permission, &actual, b)?;
        }
        task_address::insert(&mut prepared, path, &permission, b)?;
    }
    for (path, _) in task_address::input_ports(module, &definition.result, b)? {
        if enum_address::inactive_path(&prepared.enum_variants, &definition.result, &path, b)? {
            continue;
        }
        b.spend(
            summary
                .task_ports
                .len()
                .saturating_mul(path.len().saturating_add(1)),
        )?;
        if !summary.task_ports.iter().any(|(known, _)| known == &path) {
            complete = false;
        }
    }
    if !complete {
        state.pending_external_update = true;
    }
    *out = prepared;
    Ok(true)
}

impl Before<'_> {
    /// Select actual input occurrences. Address values remain alpha slots;
    /// following their real heads selects independent beta memory snapshots.
    /// No header/type traversal alone creates a source or initialized value.
    fn input_locations<'s>(
        &'s self,
        input: option_address::Input,
        path: &[PlaceProjection],
        view: bool,
        b: &mut Budget,
    ) -> Check<(Vec<(&'s Payload, Path, &'s Type)>, bool)> {
        let mut pending = vec![];
        match input {
            option_address::Input::Parameter(index) => {
                let id = self
                    .arguments
                    .get(index as usize)
                    .ok_or_else(|| err("result input argument is absent"))?;
                let payload = self
                    .values
                    .get(id.0 as usize)
                    .ok_or_else(|| err("result input value is absent"))?;
                let ty = vt(self.caller, *id)?;
                b.spend(1)?;
                pending.push((payload, Path::new(), ty, 0usize, 0usize));
            }
            option_address::Input::Capture(index) => {
                let captures = match &self.environment {
                    Environment::Borrowed { captures, .. }
                    | Environment::Consumed { captures, .. } => *captures,
                    Environment::Function => {
                        return Err(err("result capture has no actual environment"));
                    }
                };
                let ty = captures
                    .get(index as usize)
                    .ok_or_else(|| err("result capture input is absent"))?;
                match &self.environment {
                    Environment::Consumed { payload, .. } => {
                        b.spend(2)?;
                        pending.push((*payload, vec![PlaceProjection::Field(index)], ty, 0, 0));
                    }
                    Environment::Borrowed { receiver, .. } => {
                        for source in &receiver.sources {
                            if source.retained_port.is_some()
                                || source.heap_element.is_some()
                                || !source.heap_parents.is_empty()
                            {
                                return Err(err(
                                    "result environment requires its independent opaque or heap route",
                                ));
                            }
                            let Some(holder) = memory_holder(self.state, source.base, b)? else {
                                return Ok((vec![], false));
                            };
                            let Some(mut target) = symbolic_slot_path(holder, source, &[], b)?
                            else {
                                return Ok((vec![], false));
                            };
                            b.spend(target.len().saturating_add(2))?;
                            target.push(PlaceProjection::Field(index));
                            pending.push((holder, target, ty, 0, 0));
                        }
                    }
                    Environment::Function => unreachable!(),
                }
            }
        }
        let mut out = vec![];
        let mut known = !pending.is_empty();
        while let Some((holder, target, ty, index, depth)) = pending.pop() {
            b.spend(target.len().saturating_add(1))?;
            if depth > b.limits.max_call_depth.min(128) {
                return Err(limit("call-before input memory depth exceeded"));
            }
            if let Type::Address { inner, .. } = ty {
                let endpoint = view
                    && index == path.len()
                    && matches!(inner.as_ref(), Type::Dyn { .. } | Type::OwnedDyn { .. });
                if !endpoint {
                    let head = if target.is_empty() {
                        Some(&holder.address)
                    } else {
                        leaf(holder, &target, b)?
                    };
                    let Some(head) = head else {
                        known = false;
                        continue;
                    };
                    if head.sources.is_empty() {
                        known = false;
                        continue;
                    }
                    for source in &head.sources {
                        if source.retained_port.is_some()
                            || source.heap_element.is_some()
                            || !source.heap_parents.is_empty()
                        {
                            return Err(err(
                                "auxiliary input pointee requires its independent logical or heap memory route",
                            ));
                        }
                        let Some((memory,position))=memory_location(self.state,source,&[],b)? else {
                            known = false;
                            continue;
                        };
                        b.spend(1)?;
                        pending.push((memory, position, inner.as_ref(), index, depth + 1));
                    }
                    continue;
                }
            }
            if index == path.len() {
                b.spend(1)?;
                out.push((holder, target, ty));
                continue;
            }
            let mut storage = ty;
            while let Type::BorrowedStorage { storage: inner, .. } = storage {
                storage = inner;
            }
            let step = path[index];
            let child = match (storage, step) {
                (Type::Tuple(fields), PlaceProjection::Field(i)) => fields.get(i as usize),
                (
                    Type::Record { fields, .. } | Type::Package { fields, .. },
                    PlaceProjection::Field(i),
                ) => fields.get(i as usize).map(|(_, ty)| ty),
                (Type::Array { element, length }, PlaceProjection::Field(i)) if i < *length => {
                    Some(element.as_ref())
                }
                (Type::Enum { variants, .. }, PlaceProjection::Variant(i)) => variants
                    .get(i.variant as usize)
                    .and_then(|(_, fields)| fields.get(i.index as usize)),
                (Type::ErrorUnion { error, result }, PlaceProjection::ErrorArm(failed)) => {
                    Some(if failed {
                        error.as_ref()
                    } else {
                        result.as_ref()
                    })
                }
                (Type::Closure(id), PlaceProjection::Field(i)) => self
                    .module
                    .closures
                    .get(id.0 as usize)
                    .filter(|d| d.id == *id)
                    .and_then(|d| d.captures.get(i as usize)),
                _ => None,
            }
            .ok_or_else(|| err("auxiliary result input path changes its actual child type"))?;
            b.spend(target.len().saturating_add(2))?;
            let mut next = target;
            next.push(step);
            pending.push((holder, next, child, index + 1, depth + 1));
        }
        Ok((out, known))
    }
    pub(super) fn descriptor_source(&self, source: &Source, b: &mut Budget) -> Check<Permission> {
        self.description(source, b)
    }
    fn description(&self, source: &Source, b: &mut Budget) -> Check<Permission> {
        if source.retained_port.is_some()
            || source.heap_element.is_some()
            || !source.heap_parents.is_empty()
        {
            return self.source(source, b);
        }
        let input = match source.base {
            HoldRoot::Parameter(index) => option_address::Input::Parameter(index),
            HoldRoot::Capture(index) => option_address::Input::Capture(index),
            _ => return self.source(source, b),
        };
        b.spend(source.leaf.len().saturating_add(source.inline.len()))?;
        let mut path = source.leaf.clone();
        path.extend(&source.inline);
        let (locations, known) = self.input_locations(input, &path, false, b)?;
        if !known {
            return Ok(Permission::default());
        }
        let mut out = Permission::default();
        for (holder, path, ty) in locations {
            let auxiliary = if path.is_empty() {
                Some(&holder.store_description)
            } else {
                value_transport::facts(holder, &path, b)?.map(|facts| &facts.description)
            };
            let permission = if let Some(permission) =
                auxiliary.filter(|permission| !permission.sources.is_empty())
            {
                permission
            } else if matches!(ty, Type::Desc(_)) {
                // The existing descriptor kernel may carry a manager in the
                // actual Desc address slot. Its exact Type/mode, not merely
                // a matching Source path, permits this capability channel.
                if path.is_empty() {
                    &holder.address
                } else {
                    let Some(permission) = leaf(holder, &path, b)? else {
                        return Ok(Permission::default());
                    };
                    permission
                }
            } else {
                // A genuine non-Desc input owner keeps its existing head
                // route; it is not reinterpreted as a saved descriptor.
                return self.source(source, b);
            };
            if permission.sources.is_empty() {
                return Ok(Permission::default());
            }
            join_permission(&mut out, permission, b)?;
        }
        if source.static_dependency == StaticProvenance::UpperOnly {
            for actual in &mut out.sources {
                actual.static_dependency = StaticProvenance::UpperOnly;
            }
        }
        Ok(out)
    }
    fn input_views(&self, source: &Source, b: &mut Budget) -> Check<Vec<dyn_ports::View>> {
        if source.retained_port.is_some()
            || source.heap_element.is_some()
            || !source.heap_parents.is_empty()
        {
            return Ok(vec![]);
        }
        let input = match source.base {
            HoldRoot::Parameter(index) => option_address::Input::Parameter(index),
            HoldRoot::Capture(index) => option_address::Input::Capture(index),
            _ => return Ok(vec![]),
        };
        b.spend(source.leaf.len().saturating_add(source.inline.len()))?;
        let mut path = source.leaf.clone();
        path.extend(&source.inline);
        let (locations, known) = self.input_locations(input, &path, true, b)?;
        if !known {
            return Ok(vec![]);
        }
        let mut out = vec![];
        for (holder, path, _) in locations {
            let views: &[dyn_ports::View] = if path.is_empty() {
                holder.dyn_views.as_slice()
            } else {
                value_transport::facts(holder, &path, b)?
                    .map_or(&[], |facts| facts.views.as_slice())
            };
            if views.is_empty() {
                return Ok(vec![]);
            }
            for view in views {
                dyn_ports::views_work(&out, b)?;
                permission_work(&view.head, b)?;
                if !out.contains(view) {
                    b.spend(1)?;
                    out.push(view.clone());
                }
            }
        }
        Ok(out)
    }
    fn input_enum(
        &self,
        input: option_address::Input,
        path: &[PlaceProjection],
        b: &mut Budget,
    ) -> Check<enum_address::Variants> {
        let (locations, known) = self.input_locations(input, path, false, b)?;
        if !known {
            return Ok(enum_address::Variants::default());
        }
        let mut result = enum_address::Variants::default();
        for (holder, path, _) in locations {
            let selected = holder.enum_variants.project(&path, b)?;
            if !selected.present() {
                return Ok(enum_address::Variants::default());
            }
            result.join(&selected, b)?;
        }
        Ok(result)
    }
    pub(super) fn consumed_environment(&self) -> bool {
        matches!(&self.environment, Environment::Consumed { .. })
    }
    pub(super) fn module(&self) -> &TypedModule {
        self.module
    }
    pub(super) fn enum_result(
        &self,
        row: &enum_address::Variants,
        b: &mut Budget,
    ) -> Check<(enum_address::Variants, bool)> {
        row.substitute_with(b, &mut |input, path, b| self.input_enum(input, path, b))
    }
    pub(super) fn after_targets(
        &self,
        base: HoldRoot,
        through: bool,
        target_leaf: &[PlaceProjection],
        b: &mut Budget,
    ) -> Check<Permission> {
        if !target_leaf.is_empty(){
            if !through||!matches!(base,HoldRoot::Parameter(_)){
                return Err(err("projected after-state lacks its exact owned callback input"));
            }
            b.spend(target_leaf.len())?;
            return self.source(&Source{
                base,leaf:target_leaf.to_vec(),inline:vec![],retained_port:None,
                heap_parents:vec![],heap_element:None,static_dependency:StaticProvenance::NotStatic,
            },b);
        }
        match base {
            HoldRoot::Parameter(index) => {
                if !through {
                    return Err(err("auxiliary argument update is not a mutable pointee"));
                }
                let input = self
                    .arguments
                    .get(index as usize)
                    .and_then(|id| self.values.get(id.0 as usize))
                    .ok_or_else(|| err("auxiliary update input is absent"))?;
                permission_work(&input.address, b)?;
                Ok(input.address.clone())
            }
            HoldRoot::Capture(index) if through => self.source(
                &Source {
                    base,
                    leaf: vec![],
                    inline: vec![],
                    retained_port: None,
                    heap_parents: vec![],
                    heap_element: None,
                    static_dependency: StaticProvenance::NotStatic,
                },
                b,
            ),
            HoldRoot::Capture(index) => {
                let receiver = self.receiver()?;
                permission_work(receiver, b)?;
                let mut target = receiver.clone();
                for source in &mut target.sources {
                    if !source.leaf.is_empty()
                        || source.retained_port.is_some()
                        || source.heap_element.is_some()
                        || !source.heap_parents.is_empty()
                    {
                        return Err(err(
                            "auxiliary capture update requires its exact inline environment",
                        ));
                    }
                    b.spend(1)?;
                    source.inline.push(PlaceProjection::Field(index));
                }
                Ok(target)
            }
            _ => Err(err("auxiliary update selects no persistent caller input")),
        }
    }
    fn input_option(
        &self,
        input: option_address::Input,
        path: &[PlaceProjection],
        b: &mut Budget,
    ) -> Check<option_address::Variants> {
        let (locations, known) = self.input_locations(input, path, false, b)?;
        if !known {
            return Ok(option_address::Variants::default());
        }
        let mut result = option_address::Variants::default();
        for (holder, path, _) in locations {
            let selected = value_transport::option_at(holder, &path, b)?;
            if selected.mask() == 0 {
                return Ok(option_address::Variants::default());
            }
            result.join(&selected, b)?;
        }
        Ok(result)
    }
    pub(super) fn physical_permission(
        &self,
        row: &value_transport::SymbolicPermission,
        description: bool,
        i: &Instruction,
        sites: &WriteSites,
        loans: &mut [Loan],
        b: &mut Budget,
    ) -> Check<(Permission, bool)> {
        self.symbolic(row, description, i, sites, loans, b)
    }
    fn symbolic(
        &self,
        row: &value_transport::SymbolicPermission,
        description: bool,
        i: &Instruction,
        sites: &WriteSites,
        loans: &mut [Loan],
        b: &mut Budget,
    ) -> Check<(Permission, bool)> {
        let mut out = Permission::default();
        let mut complete = true;
        for source in &row.sources {
            let actual = if description {
                self.description(source, b)?
            } else {
                self.source(source, b)?
            };
            if actual.sources.is_empty() {
                complete = false;
            }
            join_permission(&mut out, &actual, b)?;
        }
        for source in &row.parents {
            let actual = self.source(source, b)?;
            if actual.sources.is_empty() {
                complete = false;
            }
            add_loans(&mut out.loans, &actual.loans, b)?;
        }
        if row.receiver {
            let head = retain_written_slot(
                self.receiver()?,
                i,
                receiver::HEAD_SITE,
                &Vec::new(),
                sites,
                loans,
                b,
            )?;
            add_loans(&mut out.loans, &head.loans, b)?;
        }
        Ok((out, complete))
    }
    pub(super) fn field_results(
        &self,
        summary: &Summary,
        result_type: &Type,
        i: &Instruction,
        sites: &WriteSites,
        loans: &mut [Loan],
        out: &mut Payload,
        b: &mut Budget,
    ) -> Check<bool> {
        let mut complete = summary.complete;
        out.enum_variants.work(b)?;
        out.option_variants.work(b)?;
        let mut activity = Payload {
            enum_variants: out.enum_variants.clone(),
            option_variants: out.option_variants.clone(),
            ..Default::default()
        };
        for (path, facts) in &out.field_facts {
            facts.option.work(b)?;
            value_transport::put(
                &mut activity,
                path,
                &value_transport::FieldFacts {
                    option: facts.option.clone(),
                    ..Default::default()
                },
                b,
            )?;
        }
        let n = summary.field_result_facts.len();
        b.spend(n.saturating_mul(n.saturating_add(2)))?;
        let mut order = (0..n).collect::<Vec<_>>();
        order.sort_unstable_by_key(|index| summary.field_result_facts[*index].0.len());
        for index in order {
            let (path, row) = &summary.field_result_facts[index];
            destination(self.module, result_type, path, b)?;
            if !path_active(self.module, result_type, &activity, path, b)? {
                continue;
            }
            let (option, known) = row
                .option
                .substitute_with(b, |input, path, b| self.input_option(input, path, b))?;
            complete &= known;
            // A known constructor bit cannot suppress children when another
            // input alternative is still unknown.
            let option = if known {
                option
            } else {
                option_address::Variants::default()
            };
            if path.is_empty() {
                activity.option_variants = option;
            } else {
                value_transport::put(
                    &mut activity,
                    path,
                    &value_transport::FieldFacts {
                        option,
                        ..Default::default()
                    },
                    b,
                )?;
            }
        }
        for (path, row) in &summary.field_result_facts {
            destination(self.module, result_type, path, b)?;
            if !path_active(self.module, result_type, &activity, path, b)? {
                continue;
            }
            let descriptor = matches!(
                destination(self.module, result_type, path, b)?,
                Type::Desc(_)
            );
            let (address, a) = self.symbolic(&row.address, descriptor, i, sites, loans, b)?;
            let (description, d) = self.symbolic(&row.description, true, i, sites, loans, b)?;
            complete &= a && d;
            let option = value_transport::option_at(&activity, path, b)?;
            let mut facts = value_transport::FieldFacts {
                address,
                description,
                option,
                creation: row.creation,
                views: vec![],
            };
            for (table, symbolic) in &row.views {
                let (head, known) = self.symbolic(symbolic, false, i, sites, loans, b)?;
                complete &= known;
                if table.is_none() {
                    let mut expanded = Vec::new();
                    for source in &symbolic.sources {
                        let views = self.input_views(source, b)?;
                        if views.is_empty() {
                            complete = false;
                            continue;
                        }
                        for mut view in views {
                            add_loans(&mut view.head.loans, &head.loans, b)?;
                            dyn_ports::views_work(&expanded, b)?;
                            permission_work(&view.head, b)?;
                            if !expanded.contains(&view) {
                                b.spend(1)?;
                                expanded.push(view);
                            }
                        }
                    }
                    if !expanded.is_empty() {
                        facts.views.extend(expanded);
                        continue;
                    }
                    complete = false;
                }
                b.spend(1)?;
                facts.views.push(dyn_ports::View {
                    table: *table,
                    head,
                });
            }
            if path.is_empty() {
                join_permission(&mut out.address, &facts.address, b)?;
                join_permission(&mut out.store_description, &facts.description, b)?;
                for view in facts.views {
                    dyn_ports::views_work(&out.dyn_views, b)?;
                    permission_work(&view.head, b)?;
                    if !out.dyn_views.contains(&view) {
                        b.spend(1)?;
                        out.dyn_views.push(view);
                    }
                }
                out.option_variants.join(&facts.option, b)?;
                if let Some(creation) = facts.creation {
                    out.creation_complete =
                        Some(out.creation_complete.unwrap_or(false) || creation);
                }
            } else {
                value_transport::put(out, path, &facts, b)?;
            }
        }
        Ok(complete)
    }
}

#[cfg(test)]
mod field_result_tests {
    use super::*;
    fn source(base: HoldRoot) -> Source {
        Source {
            base,
            leaf: vec![],
            inline: vec![],
            retained_port: None,
            heap_parents: vec![],
            heap_element: None,
            static_dependency: StaticProvenance::NotStatic,
        }
    }
    fn permission(base: HoldRoot) -> Permission {
        Permission {
            sources: vec![source(base)],
            loans: vec![],
        }
    }
    fn symbolic(base: HoldRoot) -> value_transport::SymbolicPermission {
        value_transport::SymbolicPermission {
            sources: vec![source(base)],
            ..Default::default()
        }
    }
    // Component controls bind actual initialized input payloads; they do not
    // claim that this synthetic module passed the independent whole-module WF.
    #[test]
    fn each_dynamic_input_alternative_retains_its_own_table_and_missing_is_pending() {
        let (mut module, _) = crate::tests::baseline();
        let mut caller = module.functions[0].clone();
        caller.values = vec![Type::I32, Type::I32];
        caller.call_kind = CallKind::Direct;
        caller.environment = None;
        caller.foreign = None;
        module.functions[0] = caller.clone();
        let i = Instruction {
            result: None,
            kind: InstructionKind::Call {
                function: caller.id,
                arguments: vec![ValueId(0), ValueId(1)],
            },
            span: caller.span,
        };
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        let state = Frame::default();
        let mut values = vec![Payload::default(); 2];
        for (index, payload) in values.iter_mut().enumerate() {
            payload.address = permission(HoldRoot::Parameter(index as u32));
            payload.dyn_views.push(dyn_ports::View {
                table: Some(DynVtableId {
                    module: zil_base::ModuleId(0),
                    index: index as u32,
                }),
                head: payload.address.clone(),
            });
        }
        let mut row = value_transport::ResultFacts::default();
        let mut heads = symbolic(HoldRoot::Parameter(0));
        heads.sources.push(source(HoldRoot::Parameter(1)));
        row.views.push((None, heads));
        let summary = Summary {
            complete: true,
            field_result_facts: vec![(vec![], row)],
            ..Default::default()
        };
        for missing in [false, true] {
            let mut actual = values.clone();
            if missing {
                actual[1].dyn_views.clear();
            }
            let before = Before::function(&module, &caller, &i, &actual, &state, &mut b).unwrap();
            let mut out = Payload::default();
            let known = before
                .field_results(&summary, &Type::I32, &i, &vec![], &mut [], &mut out, &mut b)
                .unwrap();
            assert_eq!(known, !missing);
            assert_eq!(out.dyn_views.len(), if missing { 1 } else { 2 });
            for view in &out.dyn_views {
                assert_eq!(
                    view.head.sources,
                    vec![source(HoldRoot::Parameter(view.table.unwrap().index))],
                    "a table never acquires another input's target head"
                );
            }
            assert!(
                out.field_facts.is_empty(),
                "root facts must install on real root channels"
            );
        }
    }
    #[test]
    fn missing_borrowed_receiver_alternative_is_not_hidden_by_a_known_sibling() {
        let (module, _) = crate::tests::baseline();
        let caller = &module.functions[0];
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        let mut state = Frame {
            locals: vec![Payload::default(); 2],
            ..Default::default()
        };
        let target = vec![PlaceProjection::Field(0)];
        let facts = value_transport::FieldFacts {
            description: permission(HoldRoot::Parameter(0)),
            ..Default::default()
        };
        let view_target = vec![PlaceProjection::Field(1)];
        let view_facts = value_transport::FieldFacts {
            views: vec![dyn_ports::View {
                table: Some(DynVtableId {
                    module: zil_base::ModuleId(0),
                    index: 0,
                }),
                head: permission(HoldRoot::Parameter(0)),
            }],
            ..Default::default()
        };
        value_transport::put(&mut state.locals[0], &target, &facts, &mut b).unwrap();
        value_transport::put(&mut state.locals[0], &view_target, &view_facts, &mut b).unwrap();
        let receiver = Permission {
            sources: vec![
                source(HoldRoot::Local(LocalId(0))),
                source(HoldRoot::Local(LocalId(1))),
            ],
            loans: vec![],
        };
        let inputs = [];
        let args = [];
        let captures = [
            Type::Desc(Box::new(Type::I32)),
            Type::Address {
                mutable: false,
                inner: Box::new(Type::Dyn {
                    interface: DynInterfaceId {
                        module: zil_base::ModuleId(0),
                        index: 0,
                    },
                }),
            },
        ];
        let before = Before {
            module: &module,
            caller,
            state: &state,
            values: &inputs,
            arguments: &args,
            environment: Environment::Borrowed {
                captures: &captures,
                receiver: receiver.clone(),
            },
            invocation: None,
        };
        assert!(
            before
                .description(&source(HoldRoot::Capture(0)), &mut b)
                .unwrap()
                .sources
                .is_empty()
        );
        assert!(
            before
                .input_views(&source(HoldRoot::Capture(1)), &mut b)
                .unwrap()
                .is_empty()
        );
        value_transport::put(&mut state.locals[1], &target, &facts, &mut b).unwrap();
        value_transport::put(&mut state.locals[1], &view_target, &view_facts, &mut b).unwrap();
        let before = Before {
            module: &module,
            caller,
            state: &state,
            values: &inputs,
            arguments: &args,
            environment: Environment::Borrowed {
                captures: &captures,
                receiver,
            },
            invocation: None,
        };
        assert!(
            !before
                .description(&source(HoldRoot::Capture(0)), &mut b)
                .unwrap()
                .sources
                .is_empty()
        );
        assert_eq!(
            before
                .input_views(&source(HoldRoot::Capture(1)), &mut b)
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn current_may_target_field_requires_every_real_holder_not_only_one_initialized_sibling() {
        let (mut module, _) = crate::tests::baseline();
        let mut caller = module.functions[0].clone();
        caller.values = vec![Type::I32];
        caller.call_kind = CallKind::Direct;
        caller.environment = None;
        caller.foreign = None;
        module.functions[0] = caller.clone();
        let instruction = Instruction {
            result: None,
            kind: InstructionKind::Call {
                function: caller.id,
                arguments: vec![ValueId(0)],
            },
            span: caller.span,
        };
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        let path = vec![PlaceProjection::Field(0)];
        let mut frame = Frame {
            locals: vec![Payload::default(); 2],
            ..Default::default()
        };
        add_leaf(
            &mut frame.locals[0],
            &path,
            &permission(HoldRoot::Parameter(2)),
            &mut b,
        )
        .unwrap();
        let input = Payload {
            address: Permission {
                sources: vec![
                    source(HoldRoot::Local(LocalId(0))),
                    source(HoldRoot::Local(LocalId(1))),
                ],
                loans: vec![],
            },
            ..Default::default()
        };
        let values = [input];
        let mut origin = source(HoldRoot::Parameter(0));
        origin.leaf = path.clone();
        let before =
            Before::function(&module, &caller, &instruction, &values, &frame, &mut b).unwrap();
        assert!(
            before.source(&origin, &mut b).unwrap().sources.is_empty(),
            "one missing current slot cannot be covered by its sibling"
        );
        add_leaf(
            &mut frame.locals[1],
            &path,
            &permission(HoldRoot::Parameter(3)),
            &mut b,
        )
        .unwrap();
        let before =
            Before::function(&module, &caller, &instruction, &values, &frame, &mut b).unwrap();
        let actual = before.source(&origin, &mut b).unwrap();
        assert_eq!(actual.sources.len(), 2);
        assert!(
            actual
                .sources
                .iter()
                .any(|row| row.base == HoldRoot::Parameter(2))
        );
        assert!(
            actual
                .sources
                .iter()
                .any(|row| row.base == HoldRoot::Parameter(3))
        );
    }
    #[test]
    fn inactive_error_arm_is_classified_before_its_absent_capture_is_resolved() {
        let (mut module, _) = crate::tests::baseline();
        let mut caller = module.functions[0].clone();
        let result_type = Type::ErrorUnion {
            error: Box::new(Type::I32),
            result: Box::new(Type::Desc(Box::new(Type::I32))),
        };
        caller.values = vec![result_type.clone(), Type::I32];
        caller.call_kind = CallKind::Direct;
        caller.environment = None;
        caller.foreign = None;
        module.functions[0] = caller.clone();
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        // This control invokes the complete Task/error tag producer, rather
        // than the narrower ordinary-sum producer selected by other profiles.
        b.task_failure_carriers = true;
        let frame = Frame::default();
        let values = vec![Payload::default(); 2];
        let make = Instruction {
            result: Some(ValueId(0)),
            kind: InstructionKind::MakeErrorUnion {
                value: ValueId(1),
                is_error: true,
            },
            span: caller.span,
        };
        let tags = enum_address::tags::transfer(&caller, &make, &frame, &values, &mut b)
            .unwrap()
            .unwrap();
        let invocation = Instruction {
            result: None,
            kind: InstructionKind::Call {
                function: caller.id,
                arguments: vec![],
            },
            span: caller.span,
        };
        let before =
            Before::function(&module, &caller, &invocation, &values, &frame, &mut b).unwrap();
        let row = value_transport::ResultFacts {
            description: symbolic(HoldRoot::Capture(99)),
            ..Default::default()
        };
        let summary = Summary {
            complete: true,
            field_result_facts: vec![(vec![PlaceProjection::ErrorArm(false)], row)],
            ..Default::default()
        };
        let mut out = Payload {
            enum_variants: tags,
            ..Default::default()
        };
        assert!(
            before
                .field_results(
                    &summary,
                    &result_type,
                    &invocation,
                    &vec![],
                    &mut [],
                    &mut out,
                    &mut b
                )
                .unwrap()
        );
        assert!(out.field_facts.is_empty());
    }

    #[test]
    fn pointee_root_description_uses_beta_snapshot_and_every_actual_target() {
        let (mut module, _) = crate::tests::baseline();
        let mut caller = module.functions[0].clone();
        caller.values = vec![Type::Address {
            mutable: true,
            inner: Box::new(Type::Desc(Box::new(Type::I32))),
        }];
        caller.call_kind = CallKind::Direct;
        caller.environment = None;
        caller.foreign = None;
        module.functions[0] = caller.clone();
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        let instruction = Instruction {
            result: None,
            kind: InstructionKind::Call {
                function: caller.id,
                arguments: vec![ValueId(0)],
            },
            span: caller.span,
        };
        let alpha = Payload {
            address: Permission {
                sources: vec![
                    source(HoldRoot::Parameter(0)),
                    source(HoldRoot::Parameter(1)),
                ],
                loans: vec![],
            },
            store_description: permission(HoldRoot::Parameter(99)),
            ..Default::default()
        };
        let values = [alpha.clone()];
        let mut frame = Frame {
            external: vec![alpha.clone(), alpha],
            pointees: vec![
                (
                    HoldRoot::Parameter(0),
                    Payload {
                        store_description: permission(HoldRoot::Parameter(7)),
                        ..Default::default()
                    },
                ),
                (HoldRoot::Parameter(1), Payload::default()),
            ],
            ..Default::default()
        };
        let before =
            Before::function(&module, &caller, &instruction, &values, &frame, &mut b).unwrap();
        assert!(
            before
                .description(&source(HoldRoot::Parameter(0)), &mut b)
                .unwrap()
                .sources
                .is_empty()
        );
        frame.pointees[1].1.store_description = permission(HoldRoot::Parameter(8));
        let before =
            Before::function(&module, &caller, &instruction, &values, &frame, &mut b).unwrap();
        let actual = before
            .description(&source(HoldRoot::Parameter(0)), &mut b)
            .unwrap();
        assert_eq!(actual.sources.len(), 2);
        assert!(actual.sources.contains(&source(HoldRoot::Parameter(7))));
        assert!(actual.sources.contains(&source(HoldRoot::Parameter(8))));
        assert!(
            !actual.sources.contains(&source(HoldRoot::Parameter(99))),
            "pointer-value auxiliary facts cannot prove the pointee capability"
        );
    }
}

#[cfg(test)]
mod entry_separation_tests {
    use super::*;
    fn source(base: HoldRoot) -> Source {
        Source {
            base,
            leaf: vec![],
            inline: vec![],
            retained_port: None,
            heap_parents: vec![],
            heap_element: None,
            static_dependency: StaticProvenance::NotStatic,
        }
    }
    fn option() -> Type {
        Type::BorrowedStorage {
            storage: Box::new(Type::Enum {
                name: "OptionEntryComponent".into(),
                variants: vec![
                    ("None".into(), vec![]),
                    (
                        "Some".into(),
                        vec![Type::Address {
                            mutable: false,
                            inner: Box::new(Type::I32),
                        }],
                    ),
                ],
            }),
            life: StorageLifeShape {
                binder_count: 1,
                slots: vec![StorageLifeSlot {
                    path: vec![PlaceProjection::Variant(VariantProjection {
                        variant: 1,
                        index: 0,
                    })],
                    binder: 0,
                    mode: StoredBorrowMode::SharedAddress,
                }],
                retained: vec![StorageRetainedBinding {
                    path: vec![LifeTypeProjection::TypeArgument(0)],
                    class: 0,
                    binder: 0,
                }],
                nominal_arguments: vec![],
            },
        }
    }
    #[test]
    fn entry_separation_distinguishes_inactive_unknown_and_each_current_pointee_raw_wire() {
        let (mut module, _) = crate::tests::baseline();
        let ty = option();
        let pointer = Type::Address {
            mutable: true,
            inner: Box::new(ty.clone()),
        };
        let shared = Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        };
        let mut local = module.functions[0].locals[0].clone();
        local.ty = ty.clone();
        local.mutable = true;
        module.functions[0].parameters.clear();
        module.functions[0].locals = (0..4)
            .map(|index| {
                let mut value = local.clone();
                value.id = LocalId(index);
                if index >= 2 {
                    value.ty = Type::I32;
                }
                value
            })
            .collect();
        module.functions[0].values = vec![pointer.clone(), shared.clone()];
        let mut callee = module.functions[0].clone();
        callee.id = FunctionId(1);
        callee.locals.truncate(2);
        callee.locals[0].ty = pointer;
        callee.locals[1].ty = shared;
        callee.parameters = vec![LocalId(0), LocalId(1)];
        module.functions.push(callee);
        let limits = Limits::default();
        let bytes = zil_ir::wire::encode_module_candidate(
            &module,
            &CandidatePlan::empty_for(&module),
            &limits,
        )
        .unwrap();
        let decoded = zil_ir::wire::decode_module_candidate(&bytes, &limits)
            .unwrap()
            .0;
        // Entry-selection components after full actual type/source checking;
        // this hand-built nominal is not admitted as a whole program here.
        for module in [&module, &decoded] {
            let mut b = Budget::new(&limits);
            let mut state = Frame {
                locals: vec![Payload::default(); 4],
                ..Default::default()
            };
            let mut values = vec![Payload::default(); 2];
            values[0].address.sources = vec![source(HoldRoot::Local(LocalId(0)))];
            values[1].address.sources = vec![source(HoldRoot::Local(LocalId(3)))];
            let mut old = source(HoldRoot::Parameter(0));
            old.leaf = vec![PlaceProjection::Variant(VariantProjection {
                variant: 1,
                index: 0,
            })];
            let other = source(HoldRoot::Parameter(1));
            let arguments = [ValueId(0), ValueId(1)];
            state.locals[0].option_variants =
                option_address::Variants::construction(0, &mut b).unwrap();
            macro_rules! bind {
                () => {
                    Before::direct(
                        module,
                        &module.functions[0],
                        &module.functions[1],
                        &arguments,
                        &values,
                        &state,
                        &mut b,
                    )
                };
            }
            assert!(matches!(
                bind!().unwrap().entry_source(&old, &mut b).unwrap(),
                EntrySource::Inactive
            ));
            assert!(
                bind!()
                    .unwrap()
                    .separation(&old, &other, &mut b)
                    .unwrap()
                    .is_none()
            );
            state.locals[0].option_variants = option_address::Variants::default();
            assert!(matches!(
                bind!().unwrap().entry_source(&old, &mut b).unwrap(),
                EntrySource::Unknown
            ));
            assert!(bind!().unwrap().separation(&old, &other, &mut b).is_err());
            state.locals[0].option_variants =
                option_address::Variants::construction(1, &mut b).unwrap();
            assert!(
                matches!(
                    bind!().unwrap().entry_source(&old, &mut b).unwrap(),
                    EntrySource::Unknown
                ),
                "known Some still needs its actual physical target"
            );
            add_leaf(
                &mut state.locals[0],
                &old.leaf,
                &Permission {
                    sources: vec![source(HoldRoot::Local(LocalId(2)))],
                    loans: vec![7],
                },
                &mut b,
            )
            .unwrap();
            let EntrySource::Active(actual) = bind!().unwrap().entry_source(&old, &mut b).unwrap()
            else {
                panic!("actual Some target absent");
            };
            assert_eq!(actual.sources, vec![source(HoldRoot::Local(LocalId(2)))]);
            assert_eq!(actual.loans, vec![7]);
            state.locals[1] = copy_payload(&state.locals[0], &mut b).unwrap();
            state.locals[0].leaves = leaf_rows::Rows::default();
            state.locals[0].option_variants =
                option_address::Variants::construction(0, &mut b).unwrap();
            values[0]
                .address
                .sources
                .push(source(HoldRoot::Local(LocalId(1))));
            assert!(
                matches!(
                    bind!().unwrap().entry_source(&old, &mut b).unwrap(),
                    EntrySource::Active(_)
                ),
                "each live candidate survives a distinct inactive sibling"
            );
            state.locals[0].option_variants = option_address::Variants::default();
            assert!(
                matches!(
                    bind!().unwrap().entry_source(&old, &mut b).unwrap(),
                    EntrySource::Unknown
                ),
                "a known sibling cannot hide a missing candidate tag"
            );
        }
    }
}
