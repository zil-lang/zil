//! Actual lexical trait applications. These queries re-evaluate original
//! provider recipes; an actual method ABI or a selected vtable is not a RHS.
use super::*;
use crate::semantic::value::{self, Argument, Value};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Query {
    receiver: Value,
    declaration: LifeDeclarationId,
    arguments: Vec<Argument>,
    member: Option<u32>,
}
pub(super) struct ActiveQuery {
    query: Query,
}

#[track_caller]
fn charge_value(value: &Value, b: &mut Budget) -> CompileResult<()> {
    // A Value clone shares geometry and copies its current class vector. Raw
    // bounds may be reused at this exact profile; no live trait fact is reused.
    value.check_geometry(b)?;
    b.spend(value.classes.len().saturating_add(1), WORK)
}
#[track_caller]
fn charge_arguments(arguments: &[Argument], b: &mut Budget) -> CompileResult<()> {
    b.spend(arguments.len(), WORK)?;
    for argument in arguments {
        match argument {
            Argument::Type(value) => charge_value(value, b)?,
            Argument::Const { carrier, value } => {
                crate::dyn_metadata::ty(carrier, b)?;
                if let Constant::WideString(units) = value {
                    b.spend(units.len(), WORK)?;
                }
            }
            Argument::Life(_) => b.spend(1, WORK)?,
        }
    }
    Ok(())
}
pub(super) fn arguments_equal(left: &[Argument], right: &[Argument], b: &mut Budget) -> CompileResult<bool> {
    b.spend(left.len().saturating_add(right.len()), WORK)?;
    if left.len() != right.len() { return Ok(false); }
    let mut equal = true;
    for (left, right) in left.iter().zip(right) {
        equal &= match (left, right) {
            (Argument::Type(left), Argument::Type(right)) => left.equal_paid(right, b)?,
            (Argument::Life(left), Argument::Life(right)) => left == right,
            (Argument::Const { carrier: left_ty, value: left }, Argument::Const { carrier: right_ty, value: right }) => {
                let same_carrier = geometry::same_type(left_ty, right_ty, b)?;
                let width = |value: &Constant| match value { Constant::WideString(units) => units.len(), _ => 1 };
                b.spend(width(left).saturating_add(width(right)), WORK)?;
                same_carrier && left == right
            }
            _ => false,
        };
    }
    Ok(equal)
}
impl Check<'_, '_> {
    /// Provider selection and original conformance precede this contextual
    /// substitution. Every method world and its genuine lexical parent remain
    /// independently rigid; their formal classes are mapped by full ordinal,
    /// never by coincident Type/lifetime images.
    #[allow(clippy::too_many_arguments)]
    fn trait_operation_mixed<'m>(&self, target: FunctionId,
        expected: SemanticDefinitionId, provider: &SemanticInstance,
        receiver: &Value, declaration: LifeDeclarationId, overridden: bool,
        trait_arguments: &[Argument], method_arguments: &'m [Argument], b: &mut Budget)
        -> CompileResult<operation_transport::MixedCall<'m>> {
        let parent_definition = if overridden { provider.definition } else {
            original::declaration_scope(self.program, declaration, self.inventory, b)?
        };
        let mut provider_arguments = Vec::new();
        if overridden {
            for actual in &provider.actuals {
                self.push_value(&mut provider_arguments, self.actual_value(provider, actual, b, 0)?, b)?;
            }
        }
        let parent_arguments = if overridden { provider_arguments.as_slice() } else { trait_arguments };
        let parent_self = if overridden { self.original_scope_self(provider, b, 0)? } else { receiver.copy(b)? };
        let expected_definition = self.inventory.definition(expected, b)?;
        if expected_definition.kind != SemanticDefinitionKind::Method
            || expected_definition.parent != Some(parent_definition) {
            return Err(fail("trait operation changes its selected provider/default lexical definition"));
        }
        let worlds = self.function_worlds(target, b)?;
        if worlds.is_empty() { return Err(fail("trait operation has no complete actual method worlds")); }
        let mut parents: BTreeMap<SemanticInstanceId, Vec<Argument>> = BTreeMap::new();
        for world in worlds {
            if world.definition != expected || world.environment.is_some() {
                return Err(fail("actual trait call selects another member/default definition even though its ABI may match"));
            }
            self.operation_mixed_arguments(world, method_arguments, b)?;
            let parent = self.inventory.instance(world.parent
                .ok_or_else(|| fail("trait operation method has no genuine lexical scope"))?, b)?;
            if parent.definition != parent_definition || !matches!(parent.target, SemanticInstanceTarget::Scope) {
                return Err(fail("trait operation method selects another provider/default parent scope"));
            }
            self.operation_mixed_arguments(parent, parent_arguments, b)?;
            let actual_self = self.original_scope_self(parent, b, 0)?;
            let mut classes = Vec::new();
            for class in &actual_self.classes {
                let class = self.operation_mixed_class(class, parent.id, parent_arguments, b)?
                    .unwrap_or_else(|| class.clone());
                self.push_value(&mut classes, class, b)?;
            }
            let actual_self = actual_self.with_classes(classes, b)?;
            if !actual_self.equal_paid(&parent_self, b)? {
                return Err(fail("trait operation parent changes its independently selected complete Self substitution"));
            }
            let actual = world.self_type.as_ref()
                .ok_or_else(|| fail("trait operation method omits its original Self context"))?;
            let actual = self.resolved(world, actual, b, 0)?;
            let mut classes = Vec::new();
            for class in &actual.classes {
                let class = match self.operation_mixed_class(class, world.id, method_arguments, b)? {
                    Some(class) => class,
                    None => self.operation_mixed_class(class, parent.id, parent_arguments, b)?
                        .unwrap_or_else(|| class.clone()),
                };
                self.push_value(&mut classes, class, b)?;
            }
            if !actual.with_classes(classes, b)?.equal_paid(&parent_self, b)? {
                return Err(fail("trait operation method changes its complete receiver scope substitution"));
            }
            b.lookup(parents.len(), WORK)?;
            if let Some(previous) = parents.get(&parent.id) {
                if !arguments_equal(previous, parent_arguments, b)? {
                    return Err(fail("trait operation has conflicting actual parent mixed substitutions"));
                }
            } else {
                charge_arguments(parent_arguments, b)?;
                parents.insert(parent.id, parent_arguments.to_vec());
            }
        }
        Ok(operation_transport::MixedCall { target: expected, arguments: method_arguments, parents })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn trait_operation(
        &self,
        instance: &SemanticInstance,
        site: SemanticSiteId,
        function: &TypedFunction,
        block: BlockId,
        index: usize,
        actuals: &[SemanticActual],
        b: &mut Budget,
    ) -> CompileResult<Option<Value>> {
        let Some(boundary) =
            self.original
                .trait_boundary(instance, site, self.program, self.inventory, b)?
        else {
            return Ok(None);
        };
        let instruction = function
            .blocks
            .get(block.0 as usize)
            .filter(|item| item.id == block)
            .and_then(|item| item.instructions.get(index))
            .ok_or_else(|| fail("original trait invocation has no actual operation"))?;
        if let Some(owned) = boundary.dynamic {
            let Type::Dyn {
                interface: expected,
            } = &boundary.receiver.ty
            else {
                return Err(fail(
                    "original dynamic receiver lost its complete interface application",
                ));
            };
            if !actuals.is_empty()
                || !boundary.method_actuals.is_empty()
                || boundary.call_kind != CallKind::Direct
            {
                return Err(unknown(
                    "dynamic invocation needs its complete nonzero or quantified result/receiver role transport",
                ));
            }
            let InstructionKind::CallDynSlot {
                callee,
                interface,
                slot,
                arguments,
            } = &instruction.kind
            else {
                return Err(fail(
                    "original dynamic member is replaced by another actual opcode",
                ));
            };
            if interface != expected {
                return Err(fail(
                    "actual dynamic call changes its original complete interface",
                ));
            }
            let mut directory = None;
            for candidate in &self.module.dyn_interfaces {
                b.spend(1, WORK)?;
                if candidate.id == *interface && directory.replace(candidate).is_some() {
                    return Err(fail("actual dynamic interface identity is repeated"));
                }
            }
            let directory = directory
                .ok_or_else(|| fail("actual dynamic call selects no complete interface"))?;
            let slot = directory
                .slots
                .get(*slot as usize)
                .ok_or_else(|| fail("actual dynamic call selects an absent slot"))?;
            b.spend(
                slot.parameters.len().saturating_add(arguments.len() + 1),
                WORK,
            )?;
            if slot.member.application.declaration != boundary.declaration
                || slot.member.application != directory.application
                || slot.member.ordinal != boundary.member
                || !slot.actuals.is_empty()
                || slot.call_kind != boundary.call_kind
                || slot.parameters.len() != boundary.parameters.len()
                || arguments.len() != boundary.parameters.len()
                || slot.result != boundary.result.ty
                || slot.receiver.mutable
            {
                return Err(fail(
                    "actual dynamic slot changes its exact original member, signature or receiver",
                ));
            }
            let anchor = boundary
                .receiver_anchor
                .ok_or_else(|| fail("original dynamic receiver has no lexical head"))?;
            let local = self.anchored_local(instance, function, anchor, b)?;
            operations::available_before(
                function,
                (block, index, SemanticSiteEdge::Before),
                *callee,
                b,
            )?;
            let expected_callee = Type::Address {
                mutable: false,
                inner: Box::new(Type::Dyn {
                    interface: *interface,
                }),
            };
            if function.values.get(callee.0 as usize) != Some(&expected_callee) {
                return Err(fail(
                    "actual dynamic receiver changes its complete borrowed type",
                ));
            }
            let actual_receiver = if owned {
                let mut head = *callee;
                let mut source = None;
                for _ in 0..=b.partial_depth {
                    let (at, ordinal, borrow) = self.defining(function, head, b)?;
                    match &borrow.kind {
                        InstructionKind::ReborrowAddress { address, mutable: false } => {
                            operations::available_before(
                                function,
                                (at, ordinal, SemanticSiteEdge::Before),
                                *address,
                                b,
                            )?;
                            head = *address;
                        }
                        InstructionKind::BorrowDynOwned { local: actual, mutable: false } => {
                            source = Some((*actual, (at, ordinal)));
                            break;
                        }
                        _ => {
                            return Err(fail(
                                "actual dynamic receiver is borrowed from another original owning head",
                            ));
                        }
                    }
                }
                let (actual, at) = source.ok_or_else(|| {
                    unknown("dynamic owner receiver chain exceeds its finite depth")
                })?;
                let receiver = self.prepared_dyn_receiver(instance, function, local, actual, at, b)?;
                if !matches!(receiver.ty, Type::OwnedDyn { interface } if interface == *expected) {
                    return Err(fail("dynamic call changes its exact original owning interface"));
                }
                let (_, life, classes) = receiver.into_parts(b)?;
                Value::new(Type::Dyn { interface: *expected }, life, classes)
            } else {
                if function.locals.get(local.0 as usize).map(|item| &item.ty)
                    != Some(&expected_callee)
                    || self.receiver_local(function, *callee, b, 0)? != local
                {
                    return Err(fail(
                        "actual dynamic receiver selects another original borrowed input",
                    ));
                }
                let receiver = self.local_value(instance, function, local, (block, index), b, 0)?;
                self.pointee(&receiver, b)?
            };
            if !actual_receiver.equal_paid(&boundary.receiver, b)? {
                return Err(fail("dynamic call changes its original complete Context class relation"));
            }
            for ((argument, parameter), slot_type) in arguments
                .iter()
                .zip(&boundary.parameters)
                .zip(&slot.parameters)
            {
                operations::available_before(
                    function,
                    (block, index, SemanticSiteEdge::Before),
                    *argument,
                    b,
                )?;
                if slot_type != &parameter.ty
                    || function.values.get(argument.0 as usize) != Some(&parameter.ty)
                {
                    return Err(fail(
                        "actual dynamic parameter changes its complete original type",
                    ));
                }
                if self.uses_full_value(&parameter.ty, b)?
                    && self.ssa_value(instance, function, *argument, b, 0)? != *parameter
                {
                    return Err(fail(
                        "actual dynamic argument changes its original full class relation",
                    ));
                }
            }
            if !boundary.result.classes.is_empty() {
                // A persistent result role is interpreted from the original
                // trait actual in this complete Context. Actual port sources,
                // loans and static alternatives are checked by dyn dispatch.
                let Type::Address { mutable: false, .. } = &boundary.result.ty else {
                    return Err(unknown(
                        "dynamic result requires its complete non-address intrinsic domain",
                    ));
                };
                let [head] = boundary.result.life.slots.as_slice() else {
                    return Err(fail(
                        "dynamic result omits its exact original outer occurrence",
                    ));
                };
                if !head.path.is_empty()
                    || !boundary.result.life.retained.is_empty()
                    || !boundary.result.life.nominal_arguments.is_empty()
                    || boundary.result.classes.len() != 1
                    || head.binder != 0
                {
                    return Err(unknown(
                        "dynamic result needs its complete nested class projection",
                    ));
                }
                let life = slot.storage_life.as_ref().ok_or_else(|| {
                    unknown("dynamic result has no complete slot lifetime contract")
                })?;
                let mut result_role = None;
                for row in &life.bindings {
                    b.spend(row.position.path.len().saturating_add(1), WORK)?;
                    if row.position.base == StorageBase::Result
                        && row.position.path.is_empty()
                        && result_role.replace(row.binder).is_some()
                    {
                        return Err(fail("dynamic result repeats its original outer role"));
                    }
                }
                match result_role {
                    Some(StorageBinder::Fixed(class)) => {
                        if boundary.receiver.classes.get(class as usize) != boundary.result.classes.first() {
                            return Err(fail("dynamic result changes the exact original persistent trait class"));
                        }
                    }
                    Some(StorageBinder::Invocation(invocation)) => {
                        let mut receiver_role = None;
                        for row in &life.bindings {
                            b.spend(row.position.path.len().saturating_add(1), WORK)?;
                            if row.position.base == StorageBase::Parameter(0) && row.position.path.is_empty()
                                && receiver_role.replace(row.binder).is_some() {
                                return Err(fail("dynamic receiver repeats its complete original invocation role"));
                            }
                        }
                        if receiver_role != Some(StorageBinder::Invocation(invocation)) {
                            return Err(fail("dynamic result changes its declared receiver invocation bound"));
                        }
                        let adapter = boundary.receiver_adapter.as_ref().ok_or_else(||
                            fail("dynamic result omits its independently checked original receiver adapter"))?;
                        let [slot] = adapter.life.slots.as_slice() else {
                            return Err(fail("dynamic receiver adapter omits its complete outer occurrence"));
                        };
                        if !slot.path.is_empty()
                            || adapter.classes.get(slot.binder as usize) != boundary.result.classes.first()
                        {
                            return Err(fail("dynamic result changes this original call's exact receiver occurrence"));
                        }
                        let actual = self.ssa_value(instance, function, *callee, b, 0)?;
                        if !self.outer_call_role(instance, function, (block, index), 0, *callee,
                            &actual, adapter, b, 0)? {
                            return Err(fail("dynamic result lacks its exact original invocation and actual owner-head source"));
                        }
                    }
                    None => return Err(fail("dynamic result has no complete original result role")),
                }
                let child = self.pointee(&boundary.result, b)?;
                if !child.classes.is_empty() || !empty(&child.life) {
                    return Err(fail("dynamic result drops a full pointee class"));
                }
            }
            if let Some(result) = instruction.result {
                if function.values.get(result.0 as usize) != Some(&boundary.result.ty) {
                    return Err(fail("actual dynamic result changes its full original type"));
                }
            } else if boundary.result.ty != Type::Unit {
                return Err(fail("actual dynamic call omits its declared result"));
            }
            return Ok(Some(boundary.result));
        }
        let (target, arguments) = match &instruction.kind {
            InstructionKind::Call {
                function,
                arguments,
            } if boundary.call_kind == CallKind::Direct => (*function, arguments),
            InstructionKind::SuspendCall {
                function: callee,
                arguments,
            } if boundary.call_kind == CallKind::Suspend
                && function.call_kind == CallKind::Suspend =>
            {
                (*callee, arguments)
            }
            _ => {
                return Err(fail(
                    "actual trait invocation changes its independently declared call ABI or opcode",
                ));
            }
        };
        if actuals.len() != boundary.method_actuals.len()
            || arguments.len() != boundary.parameters.len()
        {
            return Err(fail(
                "actual trait call changes its complete mixed or value arity",
            ));
        }
        for (actual, expected) in actuals.iter().zip(&boundary.method_actuals) {
            if self.actual_value(instance, actual, b, 0)? != *expected {
                return Err(fail(
                    "actual trait method changes its original mixed specialization",
                ));
            }
        }
        let trait_raw = self.inventory.original_dyn(boundary.declaration, b)?;
        let trait_member = trait_raw
            .members
            .get(boundary.member as usize)
            .ok_or_else(|| fail("actual trait call selects another original member"))?;
        let signature_scope = original::member_scope(
            self.program,
            boundary.declaration,
            boundary.member,
            self.inventory,
            b,
        )?;
        if signature_scope != boundary.method {
            return Err(fail(
                "actual trait invocation changes its original signature declaration",
            ));
        }
        let provider = self.actual_provider(
            &boundary.receiver,
            boundary.declaration,
            &boundary.arguments,
            b,
            0,
        )?;
        let provider_definition = self.inventory.definition(provider.definition, b)?;
        let SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl { declaration, .. }) =
            &provider_definition.body
        else {
            return Err(fail("actual trait invocation has another provider kind"));
        };
        let implementation = self.inventory.original_dyn(*declaration, b)?;
        let mut definition = None;
        for (ordinal, row) in implementation.members.iter().enumerate() {
            b.spend(
                row.name.len().saturating_add(trait_member.name.len() + 1),
                WORK,
            )?;
            if row.name != trait_member.name {
                continue;
            }
            if row.kind != trait_member.kind {
                return Err(fail(
                    "actual trait override changes its original method generic kinds",
                ));
            }
            for body in implementation
                .method_definitions
                .as_ref()
                .ok_or_else(|| unknown("actual provider method definition inventory is unknown"))?
            {
                if body.member == ordinal as u32 && definition.replace(body.id).is_some() {
                    return Err(fail(
                        "actual provider repeats its original method definition",
                    ));
                }
            }
        }
        let overridden = definition.is_some();
        let definition = if let Some(definition) = definition {
            definition
        } else {
            trait_raw
                .method_signatures
                .as_ref()
                .and_then(|rows| rows.iter().find(|row| row.member == boundary.member))
                .and_then(|row| row.default_definition)
                .ok_or_else(|| {
                    fail("actual provider omits its original method implementation or default")
                })?
        };
        let expected = original::definition_scope(self.program, definition, self.inventory, b)?;
        let callee = self
            .inventory
            .function(&SemanticFunctionTarget::Typed(target), b)?;
        if callee.call_kind != boundary.call_kind
            || callee.foreign.is_some()
            || callee.environment.is_some()
            || callee.parameters.len() != boundary.parameters.len()
        {
            return Err(fail(
                "actual trait method changes its original concrete entry, ABI or environment role",
            ));
        }
        let mixed = self.trait_operation_mixed(target, expected, provider,
            &boundary.receiver, boundary.declaration, overridden,
            &boundary.arguments, &boundary.method_actuals, b)?;
        if let Some(anchor) = boundary.receiver_anchor {
            let head = *arguments
                .first()
                .ok_or_else(|| fail("actual trait call omits its receiver"))?;
            if let Some(source) = &boundary.receiver_place {
                self.checked_borrow_place(instance, function, head, source, false, false, b, 0)?;
            } else if !self.anchored_value(instance, function, anchor, head, b, 0)? {
                return Err(fail(
                    "actual trait invocation takes another original receiver head",
                ));
            }
        }
        for (ordinal, ((argument, expected), parameter)) in arguments
            .iter()
            .zip(&boundary.parameters)
            .zip(&callee.parameters)
            .enumerate()
        {
            operations::available_before(
                function,
                (block, index, SemanticSiteEdge::Before),
                *argument,
                b,
            )?;
            let actual = function
                .values
                .get(argument.0 as usize)
                .ok_or_else(|| fail("actual trait argument has no complete SSA type"))?;
            let parameter = callee
                .locals
                .get(parameter.0 as usize)
                .ok_or_else(|| fail("actual method parameter local is absent"))?;
            if actual != &expected.ty || parameter.ty != expected.ty {
                return Err(fail(
                    "actual trait invocation changes a complete original argument type",
                ));
            }
            if ordinal == 0
                && let Some(source) = &boundary.receiver_place
            {
                let required = self.pointee(expected, b)?;
                if !self.capture_values_equal(&required, &source.inner, b)? {
                    return Err(fail(
                        "projected trait receiver changes its complete child domain",
                    ));
                }
                // checked_borrow_place above proves this precise root/path,
                // including the whole root classes and real pointer boundary.
                // The ordinary memory checker independently proves the loan.
            } else if self.uses_full_value(actual, b)? {
                let actual = self.ssa_value(instance, function, *argument, b, 0)?;
                let valid = if matches!(expected.ty, Type::Address { .. } | Type::Desc(_)) {
                    self.outer_call_role(instance, function, (block, index), ordinal, *argument, &actual, expected, b, 0)?
                } else {
                    actual == *expected
                };
                if !valid {
                    let before = b.remaining();
                    crate::dyn_metadata::ty(&actual.ty, b)?;
                    crate::dyn_metadata::shape(&actual.life, b)?;
                    crate::dyn_metadata::ty(&expected.ty, b)?;
                    crate::dyn_metadata::shape(&expected.life, b)?;
                    let work = before - b.remaining();
                    b.spend(work.saturating_mul(256).saturating_add(actual.classes.len().saturating_add(expected.classes.len()).saturating_mul(512)).saturating_add(1024), WORK)?;
                    let mut errors = fail("actual trait argument changes its full intrinsic classes or outer call role");
                    errors[0].notes.push(format!(
                        "trait original member {}:{} receiver declaration {:?} method slot {}; actual function {} call block {} instruction {} recipe {} parameter {} operand {}; independent source sigma {actual:?}; independent call adapter {expected:?}",
                        boundary.method.module.0, boundary.method.index, boundary.declaration,
                        boundary.member, function.id.0, block.0, index, site.0, ordinal, argument.0,
                    ));
                    return Err(errors);
                }
            }
        }
        if callee.return_type != boundary.result.ty {
            return Err(fail(
                "actual trait method result differs from its independently instantiated original member",
            ));
        }
        let output = if boundary.result.classes.is_empty() {
            boundary.result
        } else {
            self.call_output_from_entry(
                instance,
                function,
                (block, index),
                callee,
                arguments,
                &boundary.parameters,
                &boundary.result.ty,
                boundary.result.copy(b)?,
                Some(&mixed),
                b,
                0,
            )?
        };
        if let Some(result) = instruction.result {
            if function.values.get(result.0 as usize) != Some(&output.ty) {
                return Err(fail(
                    "actual trait call result SSA has another complete type",
                ));
            }
        } else if output.ty != Type::Unit {
            return Err(fail("actual trait call omits its promised result value"));
        }
        Ok(Some(output))
    }
    pub(super) fn trait_result(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        block: BlockId,
        index: usize,
        b: &mut Budget,
    ) -> CompileResult<Option<Value>> {
        let mut result = None;
        for site in &self.program.sites {
            b.spend(1, WORK)?;
            if site.instance != instance.id
                || !matches!(site.at, SemanticSiteAddress::Typed { block: at, instruction, edge: SemanticSiteEdge::After } if at == block && instruction as usize == index)
            {
                continue;
            }
            let SemanticSiteAction::BodyCall { actuals } = &site.action else {
                continue;
            };
            if let Some(value) =
                self.trait_operation(instance, site.recipe, function, block, index, actuals, b)?
            {
                if result.replace(value).is_some() {
                    return Err(fail(
                        "actual trait result has several original member roles",
                    ));
                }
            }
        }
        Ok(result)
    }
    fn enter_trait_query(
        &self,
        receiver: &Value,
        declaration: LifeDeclarationId,
        arguments: &[Argument],
        member: Option<u32>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "actual associated dependency exceeds its complete query depth",
            ));
        }
        charge_value(receiver, b)?;
        charge_arguments(arguments, b)?;
        let query = Query {
            receiver: receiver.clone(),
            declaration,
            arguments: arguments.to_vec(),
            member,
        };
        for old in self.trait_queries.borrow().iter() {
            b.spend(1, WORK)?;
            if old.query.declaration != declaration || old.query.member != member {
                continue;
            }
            // Old queries are immutable owned clones, already fully bounded.
            // Pay a complete paired comparison rather than re-walking their
            // raw metadata; every current query still had a fresh preflight.
            if old.query.receiver.equal_paid(&query.receiver, b)?
                && arguments_equal(&old.query.arguments, &query.arguments, b)?
            {
                return Err(fail(
                    "actual original associated/trait query graph is cyclic",
                ));
            }
        }
        let width = self.trait_queries.borrow().len();
        b.spend(width.saturating_add(1), WORK)?;
        self.trait_queries.borrow_mut().push(ActiveQuery { query });
        Ok(())
    }

    pub(super) fn trait_actuals(
        &self,
        instance: &SemanticInstance,
        application: &SemanticTraitRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Vec<Argument>> {
        let scope =
            original::declaration_scope(self.program, application.declaration, self.inventory, b)?;
        let declaration = self.inventory.definition(scope, b)?;
        if !matches!(
            declaration.body,
            SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { .. })
        ) || application.arguments.len() != declaration.parameters.len()
        {
            return Err(fail(
                "actual trait application changes its original kind or full mixed arity",
            ));
        }
        let mut actuals = Vec::new();
        for (formal, recipe) in declaration.parameters.iter().zip(&application.arguments) {
            self.push_value(
                &mut actuals,
                self.argument_recipe(instance, formal, recipe, b, depth + 1)?,
                b,
            )?;
        }
        Ok(actuals)
    }

    pub(super) fn original_scope_self(
        &self,
        instance: &SemanticInstance,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown(
                "original Self substitution exceeds its bounded lexical depth",
            ));
        }
        let declaration = self.inventory.definition(instance.definition, b)?;
        let actual = instance
            .self_type
            .as_ref()
            .ok_or_else(|| fail("actual trait/impl scope omits its complete receiver"))?;
        let actual = self.resolved(instance, actual, b, depth + 1)?;
        match &declaration.body {
            SemanticDefinitionBody::Nominal { .. } => {
                let mut arguments = Vec::new();
                for argument in &instance.actuals {
                    self.push_value(
                        &mut arguments,
                        self.actual_value(instance, argument, b, depth + 1)?,
                        b,
                    )?;
                }
                let expected = self.nominal_value(declaration.id, &arguments, b, depth + 1)?;
                if !actual.equal_paid(&expected, b)? {
                    return Err(fail(
                        "nominal Self differs from its original declaration and actuals",
                    ));
                }
            }
            SemanticDefinitionBody::Scope(
                SemanticScopeRecipe::TraitImpl { self_type, .. }
                | SemanticScopeRecipe::InherentImpl { self_type, .. }
                | SemanticScopeRecipe::BuiltinImpl { self_type, .. },
            ) => {
                let expected = self.recipe_value(instance, self_type, b, depth + 1)?;
                if expected != actual {
                    return Err(fail(
                        "actual impl Self differs from its complete original self-pattern substitution",
                    ));
                }
            }
            SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { declaration }) => {
                let mut arguments = Vec::new();
                for actual in &instance.actuals {
                    self.push_value(
                        &mut arguments,
                        self.actual_value(instance, actual, b, depth + 1)?,
                        b,
                    )?;
                }
                self.actual_provider(&actual, *declaration, &arguments, b, depth + 1)?;
            }
            _ => return Err(fail("actual Self has no original lexical declaration")),
        }
        Ok(actual)
    }

    pub(super) fn actual_provider(
        &self,
        receiver: &Value,
        declaration: LifeDeclarationId,
        arguments: &[Argument],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<&SemanticInstance> {
        self.enter_trait_query(receiver, declaration, arguments, None, b, depth)?;
        let result = (|| {
            let mut selected: Option<&SemanticInstance> = None;
            for candidate in &self.program.instances {
                b.spend(1, WORK)?;
                if !matches!(candidate.target, SemanticInstanceTarget::Scope) {
                    continue;
                }
                let original = self.inventory.definition(candidate.definition, b)?;
                let SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl {
                    self_type,
                    trait_application,
                    ..
                }) = &original.body
                else {
                    continue;
                };
                if trait_application.declaration != declaration {
                    continue;
                }
                // Start at the original Self recipe. The supplied Self claim
                // is checked separately and never selects this provider.
                let self_value = self.recipe_value(candidate, self_type, b, depth + 1)?;
                if !self_value.equal_paid(receiver, b)? {
                    continue;
                }
                let actuals = self.trait_actuals(candidate, trait_application, b, depth + 1)?;
                if !arguments_equal(&actuals, arguments, b)? {
                    continue;
                }
                if let Some(old) = selected {
                    crate::dyn_metadata::recipe(old, b)?;
                    crate::dyn_metadata::recipe(candidate, b)?;
                    if old.definition != candidate.definition || old.actuals != candidate.actuals {
                        return Err(fail(
                            "actual receiver trait application has several original providers",
                        ));
                    }
                } else {
                    selected = Some(candidate);
                }
            }
            let selected = selected.ok_or_else(|| unknown("actual trait application has no complete original provider scope and full class substitution"))?;
            self.original_scope_self(selected, b, depth + 1)?;
            self.scope_arguments(selected, b)?;
            Ok(selected)
        })();
        self.trait_queries.borrow_mut().pop();
        result
    }

    pub(super) fn scope_arguments(
        &self,
        instance: &SemanticInstance,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let declaration = self.inventory.definition(instance.definition, b)?;
        if !matches!(instance.target, SemanticInstanceTarget::Scope)
            || instance.parent.is_some()
            || instance.environment.is_some()
            || !instance.header_embedding.is_empty()
            || !matches!(
                instance.lifetime_instantiation,
                SemanticLifetimeInstantiation::Rigid
            )
            || instance.actuals.len() != declaration.parameters.len()
        {
            return Err(unknown(
                "actual original scope needs its complete rigid mixed and lexical context",
            ));
        }
        for (formal, actual) in declaration.parameters.iter().zip(&instance.actuals) {
            match (formal, actual) {
                (SemanticFormal::Type { requirements }, SemanticActual::Type(value)) => {
                    let value = self.resolved(instance, value, b, 0)?;
                    self.actual_type(&value.ty, b)?;
                    self.requirements(instance, requirements, b)?;
                }
                (SemanticFormal::Lifetime, SemanticActual::Life(class)) => {
                    value::class(instance, class, self.inventory, b, 0)?;
                }
                (SemanticFormal::Const { carrier }, SemanticActual::Const { ty, value }) => {
                    let expected = self.recipe_value(instance, carrier, b, 0)?;
                    let actual = self.resolved(instance, ty, b, 0)?;
                    if actual != expected || !actual.classes.is_empty() || actual.ty != value.ty() {
                        return Err(fail(
                            "actual scope Const changes its original full carrier or bits type",
                        ));
                    }
                }
                _ => return Err(fail("actual scope changes its complete mixed formal kind")),
            }
        }
        self.requirements(instance, &declaration.requirements, b)
    }

    pub(super) fn associated_actual(
        &self,
        receiver: &Value,
        declaration: LifeDeclarationId,
        arguments: &[Argument],
        member: u32,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Value> {
        self.enter_trait_query(receiver, declaration, arguments, Some(member), b, depth)?;
        let result = (|| {
            let trait_raw = self.inventory.original_dyn(declaration, b)?;
            let trait_member = trait_raw
                .members
                .get(member as usize)
                .ok_or_else(|| fail("actual associated query selects an absent original member"))?;
            if !matches!(trait_member.kind, DynMemberKind::Type) {
                return Err(fail(
                    "actual associated type query changes its original member kind",
                ));
            }
            let provider = self.actual_provider(receiver, declaration, arguments, b, depth + 1)?;
            let original = self.inventory.definition(provider.definition, b)?;
            let SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl {
                declaration: implementation,
                ..
            }) = &original.body
            else {
                return Err(fail("actual associated provider has no original impl"));
            };
            let raw = self.inventory.original_dyn(*implementation, b)?;
            let mut row = None;
            for (ordinal, candidate) in raw.members.iter().enumerate() {
                b.spend(
                    candidate
                        .name
                        .len()
                        .saturating_add(trait_member.name.len() + 1),
                    WORK,
                )?;
                if candidate.name == trait_member.name {
                    if candidate.kind != trait_member.kind {
                        return Err(fail(
                            "actual associated override changes its original member kind",
                        ));
                    }
                    row = raw
                        .associated_definitions
                        .as_ref()
                        .and_then(|rows| rows.iter().find(|row| row.member == ordinal as u32));
                }
            }
            let (context, row) = if let Some(row) = row {
                (provider, row)
            } else {
                return Err(unknown(
                    "actual trait default needs its exact receiver-bound trait scope instance",
                ));
            };
            let value: &DynAssociatedValueRecipe = row
                .value
                .as_ref()
                .ok_or_else(|| unknown("actual associated definition omits its original RHS"))?;
            let DynAssociatedValueRecipe::Type(DynTypePattern::Semantic { scope, ty }) = value
            else {
                return Err(unknown(
                    "actual associated RHS needs its complete original scoped type definition",
                ));
            };
            if *scope != context.definition {
                return Err(fail(
                    "actual associated RHS changes its original provider scope",
                ));
            }
            self.recipe_value(context, ty.as_ref(), b, depth + 1)
        })();
        self.trait_queries.borrow_mut().pop();
        result
    }
}

pub(in crate::semantic) fn original_associated(
    receiver: &Value,
    declaration: LifeDeclarationId,
    arguments: &[Argument],
    member: u32,
    program: &SemanticModule,
    inventory: &Inventory<'_>,
    original: &original::Checked,
    b: &mut Budget,
    depth: usize,
) -> CompileResult<Value> {
    let checker = Check {
        module: inventory.module,
        program,
        inventory,
        original,
        checked_header_bounds: Default::default(),
        scope_queries: std::cell::RefCell::new(BTreeSet::new()),
        closed_call_targets: Default::default(),
        capture_directories: Default::default(),
        value_queries: std::cell::RefCell::new(BTreeSet::new()),
        closed_ssa_types: Default::default(),
        range_locals: Default::default(),
        formed_nominal_types: Default::default(),
        task_results: Default::default(),
        creator_bindings: Default::default(),
        binding_cells: Default::default(),
        slot_queries: Default::default(),
        trait_queries: std::cell::RefCell::new(Vec::new()),
        nominal_definitions: std::cell::RefCell::new(BTreeMap::new()),
        closed_nominals: Default::default(),
        closed_nominal_sources: Default::default(),
    };
    checker.associated_actual(receiver, declaration, arguments, member, b, depth)
}

pub(in crate::semantic) fn original_self(
    owner: SemanticDefinitionId,
    context: &SemanticInstance,
    program: &SemanticModule,
    inventory: &Inventory<'_>,
    original: &original::Checked,
    b: &mut Budget,
    depth: usize,
) -> CompileResult<Value> {
    let mut context = context;
    for _ in 0..=b.partial_depth {
        b.spend(1, WORK)?;
        if context.definition == owner {
            let checker = Check {
                module: inventory.module,
                program,
                inventory,
                original,
                checked_header_bounds: Default::default(),
                scope_queries: std::cell::RefCell::new(BTreeSet::new()),
                closed_call_targets: Default::default(),
        capture_directories: Default::default(),
                value_queries: std::cell::RefCell::new(BTreeSet::new()),
                closed_ssa_types: Default::default(),
                range_locals: Default::default(),
                formed_nominal_types: Default::default(),
        task_results: Default::default(),
        creator_bindings: Default::default(),
        binding_cells: Default::default(),
        slot_queries: Default::default(),
                trait_queries: std::cell::RefCell::new(Vec::new()),
                nominal_definitions: std::cell::RefCell::new(BTreeMap::new()),
                closed_nominals: Default::default(),
                closed_nominal_sources: Default::default(),
            };
            return checker.original_scope_self(context, b, depth + 1);
        }
        context = inventory.instance(
            context
                .parent
                .ok_or_else(|| fail("actual Self leaves its true original lexical scope"))?,
            b,
        )?;
    }
    Err(unknown(
        "actual Self lexical substitution exceeds its bounded depth",
    ))
}
