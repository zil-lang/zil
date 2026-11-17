//! Join independently instantiated original contracts to real CFG summaries.
//! Trait and implementation requests remain separate; no upper creates a root.
use super::*;
use crate::dynamic_witness::requests::{CallableRequest, ContractRequest, Occurrence, Origin};
fn bad(message: &str) -> Diagnostic {
    reject("E-VERIFY-DYN-CONTRACT", None, message)
}
fn covers(upper: &StoragePosition, actual: &StoragePosition, b: &mut Budget) -> Check<bool> {
    b.spend(upper.path.len().saturating_add(actual.path.len()))?;
    Ok(upper.base == actual.base
        && actual.path.starts_with(&upper.path)
        && actual.path[upper.path.len()..]
            .iter()
            .all(|step| matches!(step, ContractProjection::Place(_))))
}
fn position(
    request: &ContractRequest,
    module: &TypedModule,
    source: &Source,
    b: &mut Budget,
) -> Check<Option<StoragePosition>> {
    if matches!(source.retained_port, Some(PortOrigin::Reference { .. })) {
        return Err(bad(
            "raw pointer contents cannot be flattened into a physical or opaque contract selector",
        ));
    }
    if let Origin::TableSlot { table, .. } = request.origin {
        let table = module
            .dyn_vtables
            .iter()
            .find(|row| row.id == table)
            .ok_or_else(|| bad("actual original contract table disappeared"))?;
        let mut position = super::dyn_providers::selected(table, source, b)?;
        if let Some(position) = &mut position {
            b.spend(source.inline.len())?;
            position
                .path
                .extend(source.inline.iter().copied().map(ContractProjection::Place));
        }
        return Ok(position);
    }
    let base = match source.base {
        HoldRoot::Parameter(index) => StorageBase::Parameter(index),
        HoldRoot::Capture(index) => {
            if matches!(request.origin, Origin::Callable { .. }) {
                let mut path = vec![ContractProjection::Capture(index)];
                path.extend(source.leaf.iter().copied().map(ContractProjection::Place));
                path.extend(source.inline.iter().copied().map(ContractProjection::Place));
                return Ok(Some(StoragePosition {
                    base: StorageBase::Receiver,
                    path,
                }));
            }
            StorageBase::Capture(index)
        }
        HoldRoot::Static | HoldRoot::Local(_) => return Ok(None),
    };
    if source.heap_element.is_some() || !source.heap_parents.is_empty() {
        return Err(bad(
            "original contract actual source requires an explicit owned-region selector",
        ));
    }
    let mut path = source
        .leaf
        .iter()
        .copied()
        .map(ContractProjection::Place)
        .collect::<Vec<_>>();
    if let Some(port) = &source.retained_port {
        let (carrier, port) = match port {
            PortOrigin::Task { carrier, port } | PortOrigin::Dyn { carrier, port } => {
                (carrier, *port)
            }
            PortOrigin::Reference { .. } => {
                return Err(bad(
                    "raw pointer contents require their independent input lifetime relation, not an opaque contract position",
                ));
            }
        };
        path = carrier.clone();
        path.push(ContractProjection::RetainedPort(port));
    }
    path.extend(source.inline.iter().copied().map(ContractProjection::Place));
    b.spend(path.len())?;
    Ok(Some(StoragePosition { base, path }))
}
fn header(module: &TypedModule, request: &ContractRequest, b: &mut Budget) -> Check<()> {
    let f = module
        .functions
        .get(request.function.0 as usize)
        .filter(|f| f.id == request.function)
        .ok_or_else(|| bad("original executable contract has no exact actual function"))?;
    if f.foreign.is_some()
        || f.parameters.len() != request.parameters.len()
        || f.call_kind != request.call_kind
        || f.result_mode != request.result_mode
        || f.return_type != request.result.ty
        || f.dyn_instance != request.specialization
        || f.storage_life != request.actual_header
    {
        return Err(bad(
            "original executable contract changes exact function/header/result responsibility",
        ));
    }
    if request.parameter_modes.len() != request.parameters.len() {
        return Err(bad(
            "original executable contract omits complete parameter modes",
        ));
    }
    for ((local, value), mode) in f
        .parameters
        .iter()
        .zip(&request.parameters)
        .zip(&request.parameter_modes)
    {
        let actual = f
            .locals
            .get(local.0 as usize)
            .ok_or_else(|| bad("contract actual parameter local is absent"))?;
        if actual.ty != value.ty
            || matches!(mode, PassingMode::Borrow)
                && !matches!(actual.ty, Type::Address { mutable: false, .. })
            || matches!(mode, PassingMode::BorrowMut)
                && !matches!(actual.ty, Type::Address { mutable: true, .. })
            || matches!(mode, PassingMode::Describe) && !matches!(actual.ty, Type::Desc(_))
        {
            return Err(bad(
                "contract actual parameter changes full type or passing mode",
            ));
        }
        crate::dynamic_witness::check_type_value(module, value, request.scope, b)?;
    }
    crate::dynamic_witness::check_type_value(module, &request.result, request.scope, b)?;
    for operand in &request.operands {
        crate::dynamic_witness::check_type_value(module, &operand.value, request.scope, b)?;
    }
    if let Some(actual) = &request.actual_header {
        signature_life::charge(actual, b)?;
        // A Callable's expected header is its public P/R scheme. The actual
        // entry also contains its real Capture and invocation Receiver roles;
        // callable() checks that complete environment independently below.
        let public = |base: StorageBase| {
            !matches!(request.origin, Origin::Callable { .. })
                || matches!(base, StorageBase::Parameter(_) | StorageBase::Result)
        };
        let static_output = |pos: &StoragePosition| {
            matches!(request.origin, Origin::Callable { .. })
                && pos.base == StorageBase::Result
                && request
                    .expected_header
                    .bindings
                    .iter()
                    .any(|row| row.position == *pos)
        };
        let public_bindings = actual
            .bindings
            .iter()
            .filter(|row| public(row.position.base))
            .collect::<Vec<_>>();
        let public_statics = actual
            .static_bindings
            .iter()
            .filter(|row| public(row.base))
            .collect::<Vec<_>>();
        if public_bindings.len()
            + public_statics
                .iter()
                .filter(|pos| static_output(pos))
                .count()
            != request.expected_header.bindings.len()
            || actual
                .classes
                .iter()
                .filter(|row| public(row.position.base))
                .count()
                != request.expected_header.classes.len()
            || public_statics.iter().any(|pos| {
                !request.expected_header.static_bindings.contains(pos) && !static_output(pos)
            })
            || request
                .expected_header
                .static_bindings
                .iter()
                .any(|pos| !public_statics.contains(&pos))
        {
            return Err(bad(
                "original executable contract loses a complete public header occurrence",
            ));
        }
        for expected in &request.expected_header.bindings {
            if actual.static_bindings.contains(&expected.position)
                && static_output(&expected.position)
            {
                continue;
            }
            let mut rows = public_bindings
                .iter()
                .filter(|row| row.position == expected.position);
            let row = rows
                .next()
                .ok_or_else(|| bad("contract public input occurrence is absent"))?;
            if rows.next().is_some() {
                return Err(bad("contract public input occurrence is duplicated"));
            }
            let mut image = request
                .header_embedding
                .iter()
                .filter(|map| map.from == row.binder);
            let map = image
                .next()
                .ok_or_else(|| bad("contract actual input binder has no exact original image"))?;
            if image.next().is_some() || expected.binder != map.to {
                return Err(bad(
                    "contract input occurrence changes its original scoped role",
                ));
            }
        }
        for row in actual
            .classes
            .iter()
            .filter(|row| public(row.position.base))
        {
            let mut image = request
                .header_embedding
                .iter()
                .filter(|map| map.from == row.binder);
            let map = image
                .next()
                .ok_or_else(|| bad("contract actual class has no exact original image"))?;
            if image.next().is_some()
                || !request
                    .expected_header
                    .classes
                    .iter()
                    .any(|expected| expected.position == row.position && expected.binder == map.to)
            {
                return Err(bad(
                    "contract actual full intrinsic class changes its original occurrence",
                ));
            }
        }
    } else if request
        .parameters
        .iter()
        .any(|value| value.life.binder_count != 0)
        || request.result.life.binder_count != 0
        || !request.expected_header.bindings.is_empty()
        || !request.expected_header.classes.is_empty()
        || !request.expected_header.static_bindings.is_empty()
    {
        return Err(bad(
            "class-bearing original contract actual header is Unknown",
        ));
    }
    match &request.origin {
        Origin::TableSlot {
            table,
            slot,
            member,
            definition,
        } => {
            let table = module
                .dyn_vtables
                .iter()
                .find(|row| row.id == *table)
                .ok_or_else(|| bad("contract original table is absent"))?;
            let iface = crate::dynamic_ports::interface(module, table.interface, b)?;
            if iface
                .slots
                .get(*slot as usize)
                .is_none_or(|row| row.member != *member)
                || !table
                    .entries
                    .iter()
                    .any(|entry| entry.slot == *slot && entry.function == f.id)
                || f.dyn_instance
                    .as_ref()
                    .is_none_or(|instance| instance.definition != *definition)
            {
                return Err(bad(
                    "contract request changes its exact table/member/definition entry",
                ));
            }
        }
        Origin::Definition {
            owner,
            member,
            definition,
        } => {
            let declaration = module
                .dyn_declarations
                .iter()
                .find(|row| row.id == *owner)
                .ok_or_else(|| bad("contract original definition owner is absent"))?;
            if !declaration.method_definitions.as_ref().is_some_and(|rows| {
                rows.iter().any(|row| {
                    row.id == *definition
                        && row.member == *member
                        && row
                            .instances
                            .iter()
                            .any(|instance| instance.function == DynFunctionTarget::Typed(f.id))
                })
            }) {
                return Err(bad(
                    "contract request has no exact complete original specialization",
                ));
            }
        }
        Origin::Callable { closure } => {
            if f.environment != Some(*closure)
                || !module
                    .closures
                    .iter()
                    .any(|row| row.id == *closure && row.function == f.id)
            {
                return Err(bad(
                    "contract callable substitutes another actual creator environment",
                ));
            }
        }
    }
    Ok(())
}
fn callable(module: &TypedModule, request: &CallableRequest, b: &mut Budget) -> Check<()> {
    let Origin::Callable { closure } = &request.contract.origin else {
        return Err(bad(
            "callable executable contract changes its original role",
        ));
    };
    let definition = module
        .closures
        .iter()
        .find(|row| row.id == *closure)
        .ok_or_else(|| bad("callable actual environment is absent"))?;
    if request.receiver.ty != Type::Closure(*closure)
        || definition.function != request.contract.function
        || !crate::closures::supports(definition.kind, request.kind)
        || request.receiver.life.binder_count as usize != request.intrinsic_classes.len()
    {
        return Err(bad(
            "callable contract changes its whole actual environment or call kind",
        ));
    }
    crate::dynamic_witness::check_type_value(module, &request.receiver, request.contract.scope, b)?;
    let actual = request
        .contract
        .actual_header
        .as_ref()
        .ok_or_else(|| bad("callable actual complete header is Unknown"))?;
    if definition.storage_life.as_ref() != Some(actual)
        || actual.fixed_binder_count != request.receiver.life.binder_count
        || request.receiver.classes.len() != request.intrinsic_classes.len()
    {
        return Err(bad(
            "callable full environment changes its exact fixed intrinsic domain",
        ));
    }
    signature_life::closure_relation_resource_scopes(
        module,
        &definition.parameters,
        &definition.captures,
        definition.kind,
        crate::stored_callables::life_result(definition),
        actual,
        None,
        b,
    )?;
    // All free classes survive, including ghosts and classes sharing an outside
    // image. Fixed(i) is matched by its actual intrinsic position, never by an
    // inverse image lookup or a same-typed capture.
    for (index, image) in request.receiver.classes.iter().enumerate() {
        let mut rows = request
            .contract
            .header_embedding
            .iter()
            .filter(|row| row.from == StorageBinder::Fixed(index as u32));
        if rows.next().is_none_or(|row| row.to != *image) || rows.next().is_some() {
            return Err(bad(
                "callable whole environment loses an exact fixed class image",
            ));
        }
    }
    for map in &request.contract.header_embedding {
        match map.from {
            StorageBinder::Fixed(index) => {
                if index >= actual.fixed_binder_count {
                    return Err(bad("callable embedding invents a fixed environment class"));
                }
            }
            StorageBinder::Invocation(index) => {
                if index >= actual.invocation_binder_count
                    || !request
                        .rigid
                        .iter()
                        .any(|row| row.actual == map.from && row.image == map.to)
                {
                    return Err(bad(
                        "callable embedding invents an unproved invocation role",
                    ));
                }
            }
        }
    }
    crate::dynamic_witness::check_callable_environment(module, request, b)?;
    for row in &request.rigid {
        let valid = match row.actual {
            StorageBinder::Fixed(index) => index < actual.fixed_binder_count,
            StorageBinder::Invocation(index) => index < actual.invocation_binder_count,
        };
        if !valid
            || !request
                .contract
                .header_embedding
                .iter()
                .any(|map| map.from == row.actual && map.to == row.image)
            || !request
                .contract
                .expected_header
                .origins
                .iter()
                .any(|occurrence| match occurrence {
                    Occurrence::Borrow { origin, .. } | Occurrence::Class { origin, .. } => {
                        *origin == row.required
                    }
                })
        {
            return Err(bad(
                "callable actual rigid role loses its complete original occurrence",
            ));
        }
    }
    for row in &request.fixed_relations {
        b.spend(
            actual
                .bindings
                .len()
                .saturating_add(request.contract.expected_header.origins.len()),
        )?;
        let matched = actual.bindings.iter().any(|binding| {
            binding.position == row.position
                && match binding.binder {
                    StorageBinder::Fixed(index) => {
                        request.receiver.classes.get(index as usize) == Some(&row.lexical)
                            && request.intrinsic_classes.get(index as usize) == Some(&row.actual)
                    }
                    StorageBinder::Invocation(_) => false,
                }
        });
        if !matched||!request.contract.expected_header.origins.iter().any(|occurrence|matches!(occurrence,Occurrence::Borrow{position,binder,origin}if *position==row.position&&*binder==row.lexical&&*origin==row.required)){return Err(bad("callable Fixed role changes its exact actual input occurrence"));}
    }
    Ok(())
}
fn contains_source(
    actual: &Source,
    position: Option<&StoragePosition>,
    upper: &[StorageSource],
    output: &Type,
    allow_child: bool,
    variable_outer: bool,
    b: &mut Budget,
) -> Check<bool> {
    let mut allowed = false;
    for atom in upper {
        dynamic_metadata::source(atom, b)?;
        match atom {
            StorageSource::Exact(selected) => allowed |= position == Some(selected),
            StorageSource::Static { leaf, mode } => {
                if leaf != output
                    || !matches!(
                        (leaf, mode),
                        (
                            Type::Address { mutable: false, .. },
                            StoredBorrowMode::SharedAddress
                        ) | (Type::Desc(_), StoredBorrowMode::ReadOnlyDescription)
                    )
                {
                    return Err(bad(
                        "original static source upper changes its complete result type or mode",
                    ));
                }
                if actual.base == HoldRoot::Static {
                    allowed |= (variable_outer
                        && actual.static_dependency == StaticProvenance::UpperOnly)
                        || matches!(actual.static_dependency, StaticProvenance::Literal)
                        || matches!(&actual.static_dependency,StaticProvenance::Conditional(key)if b.proven_static_occurrences.contains(key));
                }
            }
            StorageSource::OwnedView { head, leaf, mode } => {
                if leaf != output
                    || !matches!(
                        (leaf, mode),
                        (
                            Type::Address { mutable: false, .. },
                            StoredBorrowMode::SharedAddress
                        ) | (
                            Type::Address { mutable: true, .. },
                            StoredBorrowMode::ExclusiveAddress
                        ) | (
                            Type::Desc(_),
                            StoredBorrowMode::ReadOnlyDescription
                                | StoredBorrowMode::WritableDescription
                        )
                    )
                {
                    return Err(bad(
                        "original owning view upper changes its complete result type or mode",
                    ));
                }
                // The real CFG already proves the typed physical projection and
                // its parent loan. An external saved leaf is not that head.
                if actual.leaf.is_empty()
                    && actual.retained_port.is_none()
                    && actual.heap_element.is_none()
                    && actual.heap_parents.is_empty()
                {
                    if let Some(position) = position {
                        allowed |= covers(head, position, b)?;
                    }
                }
            }
            StorageSource::PlaceAddress { .. } => {
                return Err(unsupported(
                    "original physical place source requires its independent initialized-place proof",
                    None,
                ));
            }
            StorageSource::ReborrowedLeaf { source, head } => {
                if position == Some(source) {
                    if !allow_child {
                        return Err(unsupported(
                            "original child source lacks its exact actual dynamic receiver consumer",
                            None,
                        ));
                    }
                    if source.base != head.base
                        || head.base != StorageBase::Parameter(0)
                        || !head.path.is_empty()
                        || source.path.is_empty()
                    {
                        return Err(bad(
                            "original child result selects another receiver or data source",
                        ));
                    }
                    // The provider proves the data target. Dispatch adds a child
                    // parent from its real receiver, never from a body-parent union.
                    allowed = true;
                }
            }
        }
    }
    Ok(allowed)
}
pub(in crate::stored_flow) fn table_source_upper(
    table: &DynVtable,
    actual: &Source,
    upper: &[StorageSource],
    output: &Type,
    variable_outer: bool,
    b: &mut Budget,
) -> Check<()> {
    let mut position = super::dyn_providers::selected(table, actual, b)?;
    if let Some(position) = &mut position {
        b.spend(actual.inline.len())?;
        position
            .path
            .extend(actual.inline.iter().copied().map(ContractProjection::Place));
    }
    if !contains_source(
        actual,
        position.as_ref(),
        upper,
        output,
        true,
        variable_outer,
        b,
    )? {
        return Err(bad(
            "actual provider result exceeds its complete public source upper",
        ));
    }
    Ok(())
}
fn variable_outer(
    request: &ContractRequest,
    position: &StoragePosition,
    b: &mut Budget,
) -> Check<bool> {
    b.spend(
        request
            .expected_header
            .bindings
            .len()
            .saturating_add(request.expected_header.static_bindings.len())
            .saturating_add(request.actual_header.as_ref().map_or(0, |header| {
                header
                    .bindings
                    .len()
                    .saturating_add(header.static_bindings.len())
            }))
            .saturating_mul(position.path.len().saturating_add(1)),
    )?;
    Ok(request
        .expected_header
        .bindings
        .iter()
        .any(|row| row.position == *position)
        && !request.expected_header.static_bindings.contains(position)
        && request.actual_header.as_ref().is_some_and(|header| {
            header.bindings.iter().any(|row| row.position == *position)
                && !header.static_bindings.contains(position)
        }))
}

fn source_upper(
    module: &TypedModule,
    request: &ContractRequest,
    actual: &Source,
    upper: &[StorageSource],
    result_path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<()> {
    let position = position(request, module, actual, b)?;
    if actual.base != HoldRoot::Static && position.is_none() {
        return Err(bad("contract result retains activation-local storage"));
    }
    let output = crate::dynamic_ports::physical_path(&request.result.ty, result_path, b)?;
    let output_position = StoragePosition {
        base: StorageBase::Result,
        path: result_path
            .iter()
            .copied()
            .map(ContractProjection::Place)
            .collect(),
    };
    let variable_outer = variable_outer(request, &output_position, b)?;
    let child = match request.origin {
        Origin::TableSlot { .. } => true,
        Origin::Definition { .. } => {
            // Ordinary direct calls already transport the exact summary's complete
            // parents. A short-head upper only classifies data rooted in this Self;
            // it does not add a target or construct a parent from a declaration.
            b.spend(request.expected_header.bindings.len().saturating_mul(2))?;
            let head = StoragePosition {
                base: StorageBase::Parameter(0),
                path: vec![],
            };
            let input = request
                .expected_header
                .bindings
                .iter()
                .find(|row| row.position == head)
                .map(|row| row.binder);
            let output = request
                .expected_header
                .bindings
                .iter()
                .find(|row| row.position == output_position)
                .map(|row| row.binder);
            input.is_some() && input == output
        }
        Origin::Callable { .. } => false,
    };
    if !contains_source(
        actual,
        position.as_ref(),
        upper,
        output,
        child,
        variable_outer,
        b,
    )? {
        return Err(bad(
            "actual returned target exceeds its independently instantiated original source upper",
        ));
    }
    Ok(())
}
fn after_upper(
    module: &TypedModule,
    request: &ContractRequest,
    upper: &StorageSourceBinding,
    summary: &Summary,
    b: &mut Budget,
) -> Check<()> {
    if matches!(request.origin, Origin::Definition { .. })
        && upper
            .position
            .path
            .iter()
            .any(|step| matches!(step, ContractProjection::RetainedPort(_)))
    {
        let StorageBase::Parameter(parameter) = upper.position.base else {
            return Err(bad(
                "definition retained after-state has no actual parameter",
            ));
        };
        if parameter != 0 {
            return Err(bad(
                "definition retained after-state lacks a complete original Self inventory",
            ));
        }
        b.spend(
            request
                .self_leaves
                .len()
                .saturating_mul(upper.position.path.len().saturating_add(1)),
        )?;
        let mut leaves = request.self_leaves.iter().filter(|leaf| {
            leaf.source.base == StorageBase::Result
                && leaf.source.path == upper.position.path
                && matches!(
                    leaf.kind,
                    crate::dynamic_witness::requests::LeafKind::RetainedPort { .. }
                )
        });
        let leaf = leaves
            .next()
            .ok_or_else(|| bad("definition after-state selects an absent complete Self port"))?;
        if leaves.next().is_some() {
            return Err(bad("definition after-state repeats a complete Self port"));
        }
        let rows = summary
            .contract_after
            .iter()
            .find(|(index, _)| *index == parameter)
            .ok_or_else(|| bad("actual Return omits its retained parameter snapshot"))?;
        let at = super::dyn_ports::find(&rows.1, &upper.position.path, b)?
            .ok_or_else(|| bad("actual Return omits an original retained after-state port"))?;
        let actual = &rows.1.0[at].1;
        if !actual.known {
            return Err(bad(
                "actual retained after-state is Unknown, not known empty",
            ));
        }
        let variable = matches!(
            &leaf.kind,
            crate::dynamic_witness::requests::LeafKind::RetainedPort {
                outer: crate::dynamic_witness::requests::Borrow::Variable { .. },
                ..
            }
        ) && !request
            .expected_header
            .static_bindings
            .contains(&upper.position)
            && !request
                .actual_header
                .as_ref()
                .is_some_and(|header| header.static_bindings.contains(&upper.position));
        for source in &actual.permission.sources {
            let selected = position(request, module, source, b)?;
            if !contains_source(
                source,
                selected.as_ref(),
                &upper.sources,
                &leaf.ty,
                false,
                variable,
                b,
            )? {
                return Err(bad(
                    "actual retained after-state target exceeds its independently instantiated original upper",
                ));
            }
        }
        return Ok(());
    }
    let mut actual_paths = Vec::new();
    let output;
    if let (
        Origin::TableSlot { table, .. },
        StorageBase::Parameter(0),
        [ContractProjection::RetainedPort(port)],
    ) = (
        &request.origin,
        upper.position.base,
        upper.position.path.as_slice(),
    ) {
        let table = module
            .dyn_vtables
            .iter()
            .find(|row| row.id == *table)
            .ok_or_else(|| bad("after-state contract table is absent"))?;
        let interface = crate::dynamic_ports::interface(module, table.interface, b)?;
        output = &interface
            .retained_life
            .ports
            .get(*port as usize)
            .ok_or_else(|| bad("after-state contract selects an absent Context port"))?
            .leaf;
        let row = table
            .ports
            .iter()
            .find(|row| row.port == *port)
            .ok_or_else(|| bad("after-state Context port has no complete actual row"))?;
        for source in &row.sources {
            actual_paths.push((0, crate::dynamic_ports::source_places(source, b)?));
        }
    } else {
        let StorageBase::Parameter(parameter) = upper.position.base else {
            return Ok(());
        };
        b.spend(upper.position.path.len().saturating_add(1))?;
        let path = upper
            .position
            .path
            .iter()
            .map(|step| match step {
                ContractProjection::Place(place) => Ok(*place),
                _ => Err(bad(
                    "original after-state has no complete physical parameter selector",
                )),
            })
            .collect::<Check<Vec<_>>>()?;
        let value = request
            .parameters
            .get(parameter as usize)
            .ok_or_else(|| bad("after-state parameter is absent"))?;
        let root = match &value.ty {
            Type::Address { inner, .. } | Type::Desc(inner) => inner.as_ref(),
            ty => ty,
        };
        output = crate::dynamic_ports::physical_path(root, &path, b)?;
        actual_paths.push((parameter, path));
    }
    for (parameter, path) in actual_paths {
        let actual = summary
            .external_after
            .iter()
            .find(|(index, at, _)| *index == parameter && *at == path)
            .ok_or_else(|| bad("actual Return omits an original after-state source leaf"))?;
        if actual.2.is_empty() {
            return Err(bad("actual after-state leaf is Unknown, not known empty"));
        }
        let destination = StoragePosition {
            base: StorageBase::Parameter(parameter),
            path: path
                .iter()
                .copied()
                .map(ContractProjection::Place)
                .collect(),
        };
        let mut variable = variable_outer(request, &destination, b)?;
        if !variable
            && parameter == 0
            && matches!(
                request.origin,
                Origin::TableSlot { .. } | Origin::Definition { .. }
            )
            && !request
                .expected_header
                .static_bindings
                .contains(&destination)
            && !request
                .actual_header
                .as_ref()
                .is_some_and(|header| header.static_bindings.contains(&destination))
        {
            // The header records a qualified Self as one complete intrinsic
            // boundary. Its stored variable leaves are authenticated by the
            // complete Self inventory, not flattened into extra header rows.
            b.spend(
                request
                    .self_leaves
                    .len()
                    .saturating_mul(path.len().saturating_add(1)),
            )?;
            variable = request.self_leaves.iter().any(|leaf| {
                leaf.source.base == StorageBase::Result
                    && leaf.source.path == destination.path
                    && matches!(
                        &leaf.kind,
                        crate::dynamic_witness::requests::LeafKind::Physical {
                            outer: crate::dynamic_witness::requests::Borrow::Variable { .. },
                            ..
                        }
                    )
            });
        }
        for source in &actual.2 {
            let selected = position(request, module, source, b)?;
            if !contains_source(
                source,
                selected.as_ref(),
                &upper.sources,
                output,
                false,
                variable,
                b,
            )? {
                return Err(bad(
                    "actual after-state target exceeds its independently instantiated original upper",
                ));
            }
        }
    }
    Ok(())
}

fn owning_parent(source: &Source) -> bool {
    source.leaf.is_empty()
        && source.retained_port.is_none()
        && source.heap_element.is_none()
        && source.heap_parents.is_empty()
}
fn parent_is_preserved(
    position: &StoragePosition,
    data_root: &StoragePosition,
    physical_parent: bool,
    upper: &[StorageSource],
    receiver_head: bool,
    b: &mut Budget,
) -> Check<bool> {
    if receiver_head
        && physical_parent
        && covers(
            &StoragePosition {
                base: StorageBase::Parameter(0),
                path: vec![],
            },
            position,
            b,
        )?
    {
        return Ok(true);
    }
    for atom in upper {
        dynamic_metadata::source(atom, b)?;
        match atom {
            // A saved external leaf is a separate target namespace. A bare
            // argument head cannot cover that leaf merely by a Place prefix.
            StorageSource::Exact(source) => {
                if (physical_parent || source == data_root) && covers(source, position, b)? {
                    return Ok(true);
                }
            }
            StorageSource::OwnedView { head, .. }
            | StorageSource::PlaceAddress { place: head, .. } => {
                if physical_parent && covers(head, position, b)? {
                    return Ok(true);
                }
            }
            StorageSource::ReborrowedLeaf { source, head } => {
                if (source == data_root && covers(source, position, b)?)
                    || (physical_parent && covers(head, position, b)?)
                {
                    return Ok(true);
                }
            }
            StorageSource::Static { .. } => {}
        }
    }
    Ok(false)
}

fn immutable_parent(source: &Source) -> bool {
    // A global immutable category has no activation/retirement epoch to lose.
    // Accepting it here proves no constant occurrence or literal provenance.
    source.base == HoldRoot::Static
        && source.leaf.is_empty()
        && source.inline.is_empty()
        && source.retained_port.is_none()
        && source.heap_element.is_none()
        && source.heap_parents.is_empty()
        && source.static_dependency != StaticProvenance::NotStatic
}
pub(in crate::stored_flow) fn table_parent_upper(
    table: &DynVtable,
    summary: &Summary,
    slot: &DynSlot,
    b: &mut Budget,
) -> Check<()> {
    if summary.dyn_result_parents.is_empty() {
        return Ok(());
    }
    let Some(DynResultSource::Borrow { sources, .. }) = &slot.contract.result else {
        return Err(bad(
            "actual dynamic result parents have no complete public borrowed upper",
        ));
    };
    if let Some(life) = &slot.storage_life {
        signature_life::charge(life, b)?;
    }
    let receiver_head = slot.storage_life.as_ref().is_some_and(|life| {
        signature_life::outer(life, StorageBase::Result) == Some(StorageBinder::Invocation(0))
    });
    for parent in &summary.dyn_result_parents {
        source_work(std::slice::from_ref(parent), b)?;
        if immutable_parent(parent) {
            continue;
        }
        if parent.heap_element.is_some() || !parent.heap_parents.is_empty() {
            return Err(unsupported(
                "actual dynamic result parent requires complete owned heap-region transport",
                None,
            ));
        }
        let mut position = super::dyn_providers::selected(table, parent, b)?.ok_or_else(|| {
            bad("actual provider result retains an untransported local/capture parent")
        })?;
        b.spend(
            position
                .path
                .len()
                .saturating_add(parent.inline.len())
                .saturating_add(1),
        )?;
        let data_root = position.clone();
        position
            .path
            .extend(parent.inline.iter().copied().map(ContractProjection::Place));
        if !parent_is_preserved(
            &position,
            &data_root,
            owning_parent(parent),
            sources,
            receiver_head,
            b,
        )? {
            return Err(bad(
                "actual provider result parent is absent from the complete public data/head transport",
            ));
        }
    }
    Ok(())
}
fn contract_parents(
    module: &TypedModule,
    request: &ContractRequest,
    summary: &Summary,
    b: &mut Budget,
) -> Check<()> {
    if !matches!(request.origin, Origin::TableSlot { .. }) || summary.dyn_result_parents.is_empty()
    {
        return Ok(());
    }
    let Some(DynResultSource::Borrow { sources, .. }) = &request.contract.result else {
        return Err(bad(
            "original dynamic parent proof has no borrowed result upper",
        ));
    };
    let root = StoragePosition {
        base: StorageBase::Result,
        path: vec![],
    };
    let receiver = StoragePosition {
        base: StorageBase::Parameter(0),
        path: vec![],
    };
    b.spend(request.expected_header.bindings.len().saturating_mul(2))?;
    let output = request
        .expected_header
        .bindings
        .iter()
        .find(|row| row.position == root)
        .map(|row| row.binder);
    let input = request
        .expected_header
        .bindings
        .iter()
        .find(|row| row.position == receiver)
        .map(|row| row.binder);
    let receiver_head = output == input && matches!(output, Some(StorageBinder::Invocation(_)));
    for parent in &summary.dyn_result_parents {
        source_work(std::slice::from_ref(parent), b)?;
        if immutable_parent(parent) {
            continue;
        }
        if parent.heap_element.is_some() || !parent.heap_parents.is_empty() {
            return Err(unsupported(
                "actual dynamic result parent requires complete owned heap-region transport",
                None,
            ));
        }
        let position = position(request, module, parent, b)?.ok_or_else(|| {
            bad("original dynamic result retains an untransported activation parent")
        })?;
        b.spend(position.path.len().saturating_add(1))?;
        let mut data_root = position.clone();
        data_root
            .path
            .truncate(data_root.path.len().saturating_sub(parent.inline.len()));
        if !parent_is_preserved(
            &position,
            &data_root,
            owning_parent(parent),
            sources,
            receiver_head,
            b,
        )? {
            return Err(bad(
                "actual result parent exceeds the independently instantiated original data/head upper",
            ));
        }
    }
    Ok(())
}

fn contract(
    module: &TypedModule,
    request: &ContractRequest,
    summary: &Summary,
    b: &mut Budget,
) -> Check<()> {
    header(module, request, b)?;
    if !summary.complete || !summary.dyn_effects_known {
        return Err(bad("original executable contract lacks a complete actual CFG memory/effect summary").with_note(format!("exact function {:?} {}, origin {:?}; actual memory complete {}, actual effects known {}; effects {:?}",request.function,module.functions[request.function.0 as usize].name,request.origin,summary.complete,summary.dyn_effects_known,summary.dyn_effects)));
    }
    let c = &request.contract;
    let requires = c
        .requires
        .as_ref()
        .ok_or_else(|| bad("original contract requires are Unknown"))?;
    let guarantees = c
        .guarantees
        .as_ref()
        .ok_or_else(|| bad("original contract guarantees are Unknown"))?;
    let hold = c
        .hold
        .as_ref()
        .ok_or_else(|| bad("original contract hold is Unknown"))?;
    let access = c
        .call
        .as_ref()
        .ok_or_else(|| bad("original contract access is Unknown"))?;
    let effects = c
        .effects
        .as_ref()
        .ok_or_else(|| bad("original contract effects are Unknown"))?;
    let result = c
        .result
        .as_ref()
        .ok_or_else(|| bad("original contract result source is Unknown"))?;
    let cleanup = c
        .cleanup
        .as_ref()
        .ok_or_else(|| bad("original contract cleanup is Unknown"))?;
    if c.resume != Some(DynResumeBound::Local)
        || c.threads
            .as_ref()
            .is_none_or(|threads| threads.transfer || threads.share)
    {
        return Err(bad(
            "original contract requires a separately proved nonlocal actual execution role",
        ));
    }
    for requirement in requires {
        match requirement {
            DynRequirement::Read(pos)
            | DynRequirement::Write(pos)
            | DynRequirement::Manage(pos) => dynamic_metadata::position(pos, b)?,
            DynRequirement::Apart(..) | DynRequirement::State { .. } => {
                return Err(bad(
                    "original executable precondition has no independently checked actual relational/state consumer",
                ));
            }
        }
    }
    for effect in &summary.dyn_effects {
        b.spend(effects.len())?;
        if !effects.contains(effect) {
            return Err(bad(
                "actual reachable method/helper effect exceeds an original trait or implementation upper",
            ));
        }
    }
    for row in &summary.description_accesses {
        let actual = if let Origin::TableSlot { table, .. } = request.origin {
            let table = module
                .dyn_vtables
                .iter()
                .find(|entry| entry.id == table)
                .ok_or_else(|| bad("descriptor contract actual table is absent"))?;
            dyn_providers::description_position(table, row, b)?
        } else {
            position(request, module, &descriptor_access::symbolic(row, b)?, b)?
        };
        let Some(actual) = actual else {
            continue;
        };
        let upper = if row.write {
            &access.writes
        } else {
            &access.reads
        };
        let mut allowed = false;
        for declared in upper {
            if covers(declared, &actual, b)? {
                allowed = true;
                break;
            }
        }
        if !allowed {
            return Err(bad(
                "actual descriptor manager access exceeds an original executable source upper",
            ));
        }
    }
    for (source, writing) in &summary.dyn_accesses {
        let Some(actual) = position(request, module, source, b)? else {
            continue;
        };
        let upper = if *writing {
            &access.writes
        } else {
            &access.reads
        };
        let mut allowed = false;
        for pos in upper {
            if covers(pos, &actual, b)? {
                allowed = true;
                break;
            }
        }
        if !allowed {
            return Err(bad(
                "actual method target access exceeds an original head/port upper",
            ));
        }
    }
    for (path, sources) in &summary.result {
        let position = StoragePosition {
            base: StorageBase::Result,
            path: path
                .iter()
                .copied()
                .map(ContractProjection::Place)
                .collect(),
        };
        let upper = if path.is_empty() {
            match result {
                DynResultSource::Borrow { sources, .. } => Some(sources.as_slice()),
                _ => None,
            }
        } else {
            None
        }
        .or_else(|| {
            guarantees.iter().find_map(|guarantee| match guarantee {
                DynGuarantee::StorageSources(row) if row.position == position => {
                    Some(row.sources.as_slice())
                }
                _ => None,
            })
        });
        let upper = upper.ok_or_else(|| {
            bad("actual borrowed result leaf has no complete original target-source upper")
        })?;
        for source in sources {
            source_upper(module, request, source, upper, path, b)?;
        }
    }
    if let DynResultSource::Borrow { mutable, .. } = result {
        if !matches!(request.result.ty,Type::Address{mutable:actual,..}if actual==*mutable) {
            return Err(bad(
                "original borrowed result changes its actual outer mode",
            ));
        }
    }
    if matches!(result, DynResultSource::Create | DynResultSource::Give)
        && !matches!(request.result_mode, PassingMode::Create | PassingMode::Give)
    {
        return Err(bad(
            "original owning result does not match its actual responsibility mode",
        ));
    }
    contract_parents(module, request, summary, b)?;
    for guarantee in guarantees {
        match guarantee {
            DynGuarantee::StorageSources(upper) => after_upper(module, request, upper, summary, b)?,
            DynGuarantee::ManageResult => {
                if !matches!(request.result_mode, PassingMode::Create | PassingMode::Give) {
                    return Err(bad("actual result has no original owning responsibility"));
                }
            }
            DynGuarantee::State { .. } => {
                return Err(bad(
                    "original state guarantee has no completed actual Return state proof",
                ));
            }
        }
    }
    for (index, path, sources) in &summary.external_after {
        let location = Source {
            base: HoldRoot::Parameter(*index),
            leaf: path.clone(),
            inline: vec![],
            retained_port: None,
            heap_parents: vec![],
            heap_element: None,
            static_dependency: StaticProvenance::NotStatic,
        };
        let actual = position(request, module, &location, b)?
            .ok_or_else(|| bad("actual after-state location has no public role"))?;
        for source in sources {
            let Some(target) = position(request, module, source, b)? else {
                continue;
            };
            if target!=actual&&!hold.iter().any(|pos|pos==&target)&&!guarantees.iter().any(|row|matches!(row,DynGuarantee::StorageSources(upper)if upper.position==actual)){return Err(bad("actual retained after-state target is outside the original hold/guarantee upper"));}
        }
    }
    if cleanup.environment {
        let Origin::Callable { closure } = request.origin else {
            return Err(bad(
                "nonenvironment method claims environment cleanup authority",
            ));
        };
        super::dyn_providers::cleanup_proof(module, &Type::Closure(closure), cleanup, b)?;
    } else if !cleanup.positions.is_empty()
        || !cleanup.access.reads.is_empty()
        || !cleanup.access.writes.is_empty()
        || !cleanup.effects.is_empty()
    {
        return Err(bad(
            "original explicit cleanup positions have no actual owning environment role",
        ));
    }
    Ok(())
}
pub(super) fn validate(module: &TypedModule, summaries: &[Summary], b: &mut Budget) -> Check<()> {
    let pending = std::mem::take(&mut b.pending_dyn_contracts);
    b.spend(pending.methods.len())?;
    let incomplete = pending
        .methods
        .iter()
        .filter_map(|request| {
            summaries
                .get(request.function.0 as usize)
                .filter(|row| !row.complete || !row.dyn_effects_known)
                .map(|row| {
                    format!(
                        "{:?} {} {:?}: memory complete {}, effects known {}",
                        request.function,
                        module.functions[request.function.0 as usize].name,
                        request.origin,
                        row.complete,
                        row.dyn_effects_known
                    )
                })
        })
        .collect::<Vec<_>>();
    if !incomplete.is_empty() {
        return Err(bad(
            "original executable contract lacks a complete actual CFG memory/effect summary",
        )
        .with_note(incomplete.join("\n")));
    }
    for request in pending.methods {
        let summary = summaries
            .get(request.function.0 as usize)
            .ok_or_else(|| bad("original method actual CFG summary is absent"))?;
        contract(module, &request, summary, b)?;
    }
    for request in pending.callables {
        callable(module, &request, b)?;
        let summary = summaries
            .get(request.contract.function.0 as usize)
            .ok_or_else(|| bad("original callable actual CFG summary is absent"))?;
        contract(module, &request.contract, summary, b)?;
    }
    Ok(())
}

#[cfg(test)]
mod source_upper_tests {
    use super::*;
    fn source() -> Source {
        Source {
            base: HoldRoot::Parameter(0),
            leaf: vec![PlaceProjection::Field(0)],
            inline: vec![],
            retained_port: None,
            heap_parents: vec![],
            heap_element: None,
            static_dependency: StaticProvenance::NotStatic,
        }
    }
    #[test]
    fn static_alternative_cannot_ground_an_actual_target_or_replace_its_exact_port() {
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        let output = Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        };
        let port = StoragePosition {
            base: StorageBase::Parameter(0),
            path: vec![ContractProjection::RetainedPort(0)],
        };
        let upper = vec![
            StorageSource::Static {
                leaf: output.clone(),
                mode: StoredBorrowMode::SharedAddress,
            },
            StorageSource::Exact(port.clone()),
        ];
        let mut actual = source();
        assert!(
            contains_source(&actual, Some(&port), &upper, &output, false, false, &mut b).unwrap()
        );
        let mut wrong = port.clone();
        wrong.base = StorageBase::Parameter(1);
        assert!(
            !contains_source(&actual, Some(&wrong), &upper, &output, false, false, &mut b).unwrap()
        );
        actual.base = HoldRoot::Static;
        actual.leaf.clear();
        actual.static_dependency = StaticProvenance::UpperOnly;
        assert!(!contains_source(&actual, None, &upper, &output, false, false, &mut b).unwrap());
        assert!(
            contains_source(&actual, None, &upper, &output, false, true, &mut b).unwrap(),
            "a variable outer may carry the Static category without proving a constant occurrence"
        );
        assert!(
            b.proven_static_occurrences.is_empty(),
            "category containment publishes no Static grounding"
        );
        actual.static_dependency = StaticProvenance::Literal;
        assert!(contains_source(&actual, None, &upper, &output, false, false, &mut b).unwrap());
    }
    #[test]
    fn dynamic_parent_closure_rejects_an_unrelated_input_or_unselected_port() {
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        let head = StoragePosition {
            base: StorageBase::Parameter(0),
            path: vec![],
        };
        let data = StoragePosition {
            base: head.base,
            path: vec![ContractProjection::RetainedPort(0)],
        };
        let upper = vec![StorageSource::Exact(data.clone())];
        assert!(parent_is_preserved(&data, &data, false, &upper, false, &mut b).unwrap());
        assert!(
            !parent_is_preserved(&head, &head, true, &upper, false, &mut b).unwrap(),
            "data containment does not implicitly transport its receiver head"
        );
        assert!(
            parent_is_preserved(&head, &head, true, &upper, true, &mut b).unwrap(),
            "the checked invocation role separately retains the real receiver"
        );
        let other = StoragePosition {
            base: StorageBase::Parameter(1),
            path: vec![],
        };
        assert!(
            !parent_is_preserved(&other, &other, true, &upper, true, &mut b).unwrap(),
            "a receiver head cannot replace another actual argument parent"
        );
        let missing = StoragePosition {
            base: head.base,
            path: vec![ContractProjection::RetainedPort(1)],
        };
        assert!(
            !parent_is_preserved(&missing, &missing, false, &upper, true, &mut b).unwrap(),
            "a receiver head is no external port parent"
        );
        let argument_leaf = StoragePosition {
            base: StorageBase::Parameter(1),
            path: vec![ContractProjection::Place(PlaceProjection::Field(0))],
        };
        assert!(
            !parent_is_preserved(
                &argument_leaf,
                &argument_leaf,
                false,
                &[StorageSource::Exact(other)],
                false,
                &mut b
            )
            .unwrap(),
            "an argument head cannot stand for the pointee of a saved pointer field"
        );
    }
    #[test]
    fn child_upper_checks_the_actual_data_port_and_keeps_the_head_separate() {
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        let output = Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        };
        let actual = source();
        let data = StoragePosition {
            base: StorageBase::Parameter(0),
            path: vec![ContractProjection::RetainedPort(0)],
        };
        let head = StoragePosition {
            base: StorageBase::Parameter(0),
            path: vec![],
        };
        let upper = vec![StorageSource::ReborrowedLeaf {
            source: data.clone(),
            head: head.clone(),
        }];
        assert!(
            contains_source(&actual, Some(&data), &upper, &output, true, false, &mut b).unwrap()
        );
        assert!(
            !contains_source(&actual, Some(&head), &upper, &output, true, false, &mut b).unwrap()
        );
        let wrong = vec![StorageSource::ReborrowedLeaf {
            source: data.clone(),
            head: StoragePosition {
                base: StorageBase::Parameter(1),
                path: vec![],
            },
        }];
        assert!(
            contains_source(&actual, Some(&data), &wrong, &output, true, false, &mut b).is_err()
        );
    }
    #[test]
    fn owning_head_view_never_covers_a_saved_external_leaf_or_a_retained_port() {
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        let output = Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        };
        let head = StoragePosition {
            base: StorageBase::Parameter(0),
            path: vec![],
        };
        let upper = vec![StorageSource::OwnedView {
            head: head.clone(),
            leaf: output.clone(),
            mode: StoredBorrowMode::SharedAddress,
        }];
        let mut actual = source();
        let port = StoragePosition {
            base: head.base,
            path: vec![ContractProjection::RetainedPort(0)],
        };
        assert!(
            !contains_source(&actual, Some(&port), &upper, &output, false, false, &mut b).unwrap()
        );
        assert!(
            !contains_source(&actual, Some(&head), &upper, &output, false, false, &mut b).unwrap()
        );
        actual.leaf.clear();
        actual.inline.push(PlaceProjection::Field(0));
        let inline = StoragePosition {
            base: head.base,
            path: vec![ContractProjection::Place(PlaceProjection::Field(0))],
        };
        assert!(
            contains_source(
                &actual,
                Some(&inline),
                &upper,
                &output,
                false,
                false,
                &mut b
            )
            .unwrap()
        );
        assert!(!covers(&head, &port, &mut b).unwrap());
    }
}
