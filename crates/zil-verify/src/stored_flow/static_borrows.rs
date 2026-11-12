//! Constant environment contents are conditional invariants until every real
//! constructor and write has been checked. The dependency graph is private;
//! neither a public row nor a class count establishes a Static root.
use super::*;

type Node = (ClosureId, StoragePosition);

fn physical(position: &StoragePosition, b: &mut Budget) -> Check<Path> {
    b.spend(position.path.len().saturating_add(1))?;
    position
        .path
        .iter()
        .map(|step| match step {
            ContractProjection::Place(step) => Ok(*step),
            _ => Err(err("constant occurrence has no actual inline path")),
        })
        .collect()
}

fn constant_sources(node: &Node, b: &mut Budget) -> Check<Vec<Source>> {
    b.spend(node.1.path.len().saturating_add(3))?;
    Ok(vec![Source {
        static_dependency: Some(node.clone()),
        retained_port: None,
        base: HoldRoot::Static,
        leaf: vec![],
        inline: vec![],
        heap_parents: vec![],
        heap_element: None,
    }])
}

/// Only the contents of a checked capture field receive this condition. Taking
/// the address of the environment slot continues to create an actual head debt.
pub(super) fn seed_capture(
    definition: &ClosureDefinition,
    index: u32,
    payload: &mut Payload,
    loans: &mut [Loan],
    b: &mut Budget,
) -> Check<()> {
    let Some(life) = &definition.storage_life else {
        return Ok(());
    };
    b.spend(life.static_bindings.len())?;
    for position in &life.static_bindings {
        if position.base != StorageBase::Capture(index) {
            continue;
        }
        let path = physical(position, b)?;
        let permission = if path.is_empty() {
            &mut payload.address
        } else {
            let row = payload
                .leaves
                .position(&path, b)?
                .ok_or_else(|| err("constant field has no actual capture content"))?;
            payload.leaves.permission_mut(row)
        };
        let sources = constant_sources(&(definition.id, position.clone()), b)?;
        b.spend(permission.loans.len())?;
        for loan in &permission.loans {
            let loan = loans
                .get_mut(*loan)
                .ok_or_else(|| err("constant field loan is absent"))?;
            if loan.exclusive || loan.receiver_head || !loan.parents.is_empty() {
                return Err(err(
                    "constant capture content has an exclusive or head parent",
                ));
            }
            source_work(&sources, b)?;
            loan.sources = sources.clone();
        }
        permission.sources = sources;
    }
    Ok(())
}

pub(super) fn environment_source(
    definition: &ClosureDefinition,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<Option<Vec<Source>>> {
    let Some((PlaceProjection::Field(index), tail)) = path.split_first() else {
        return Ok(None);
    };
    let Some(life) = &definition.storage_life else {
        return Ok(None);
    };
    b.spend(life.static_bindings.len())?;
    for position in &life.static_bindings {
        if position.base == StorageBase::Capture(*index) && physical(position, b)? == tail {
            return constant_sources(&(definition.id, position.clone()), b).map(Some);
        }
    }
    Ok(None)
}

#[derive(Default)]
struct Obligation {
    writes: usize,
    literal_root: bool,
    dependencies: BTreeSet<Node>,
}

fn observe(
    permission: &Permission,
    solution: &Solution,
    nodes: &BTreeMap<Node, Obligation>,
    b: &mut Budget,
) -> Check<(bool, BTreeSet<Node>)> {
    permission_work(permission, b)?;
    if permission.sources.is_empty() {
        return Err(err("constant field has no actual source"));
    }
    let mut root = false;
    let mut dependencies = BTreeSet::new();
    let mut inspect = |sources: &[Source], b: &mut Budget| -> Check<()> {
        source_work(sources, b)?;
        if sources.is_empty() {
            return Err(err("constant field has an ungrounded parent"));
        }
        for source in sources {
            if source.base != HoldRoot::Static
                || !source.leaf.is_empty()
                || !source.inline.is_empty()
                || source.retained_port.is_some()
                || source.heap_element.is_some()
                || !source.heap_parents.is_empty()
            {
                return Err(err(
                    "constant field retains nonstatic data or an activation head",
                ));
            }
            if let Some(node) = &source.static_dependency {
                b.spend(
                    nodes
                        .len()
                        .checked_ilog2()
                        .map_or(1, |n| n as usize + 2)
                        .saturating_add(node.1.path.len()),
                )?;
                if !nodes.contains_key(node) {
                    return Err(err("constant source depends on an undeclared field"));
                }
                b.spend(
                    dependencies
                        .len()
                        .saturating_add(node.1.path.len())
                        .saturating_add(1),
                )?;
                dependencies.insert(node.clone());
            } else {
                root = true;
            }
        }
        Ok(())
    };
    inspect(&permission.sources, b)?;
    for id in ancestry(&permission.loans, &solution.loans, b)? {
        let loan = solution
            .loans
            .get(id)
            .ok_or_else(|| err("constant field parent is absent"))?;
        if loan.receiver_head || loan.exclusive {
            return Err(err("constant field retains a receiver or exclusive parent"));
        }
        inspect(&loan.sources, b)?;
    }
    Ok((root, dependencies))
}

/// Check all CFG-reachable construction/write sites in every function, even
/// functions absent from the main call graph. A cycle with no actual literal
/// root cannot establish its own field invariant.
pub(super) fn validate(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    solutions: &[Option<Solution>],
    mask: Option<&[bool]>,
    b: &mut Budget,
) -> Check<()> {
    let mut nodes = BTreeMap::<Node, Obligation>::new();
    b.spend(module.closures.len())?;
    for definition in &module.closures {
        if let Some(life) = &definition.storage_life {
            b.spend(life.static_bindings.len())?;
            for position in &life.static_bindings {
                if matches!(position.base, StorageBase::Capture(_)) {
                    b.spend(
                        position
                            .path
                            .len()
                            .saturating_add(nodes.len())
                            .saturating_add(1),
                    )?;
                    nodes.insert((definition.id, position.clone()), Obligation::default());
                }
            }
        }
    }
    if nodes.is_empty() {
        return Ok(());
    }
    if mask.is_some_and(|mask| mask.iter().any(|selected| !selected)) {
        return Err(err(
            "constant field invariants require the complete module CFG inventory",
        ));
    }
    for (index, function) in module.functions.iter().enumerate() {
        let solution = solutions[index]
            .as_ref()
            .ok_or_else(|| err("constant field function is unsolved"))?;
        b.spend(function.blocks.len())?;
        for (block_index, block) in function.blocks.iter().enumerate() {
            if !graphs[index].reachable[block_index] {
                continue;
            }
            b.spend(block.instructions.len())?;
            for instruction in &block.instructions {
                let mut observations = Vec::new();
                match &instruction.kind {
                    InstructionKind::MakeClosure { closure, captures } => {
                        b.spend(nodes.len())?;
                        for node in nodes.keys().filter(|node| node.0 == *closure) {
                            let StorageBase::Capture(capture) = node.1.base else {
                                unreachable!()
                            };
                            let value = captures
                                .get(capture as usize)
                                .ok_or_else(|| err("constant constructor omits its field"))?;
                            observations.push((node.clone(), *value));
                        }
                    }
                    InstructionKind::WriteCapture { index, value } => {
                        let closure = function
                            .environment
                            .ok_or_else(|| err("capture write has no actual environment"))?;
                        b.spend(nodes.len())?;
                        observations.extend(
                            nodes
                                .keys()
                                .filter(|node| {
                                    node.0 == closure && node.1.base == StorageBase::Capture(*index)
                                })
                                .map(|node| (node.clone(), *value)),
                        );
                    }
                    _ => continue,
                }
                for (node, value) in observations {
                    let payload = solution
                        .values
                        .get(value.0 as usize)
                        .ok_or_else(|| err("constant constructor value is absent"))?;
                    let path = physical(&node.1, b)?;
                    let permission = if path.is_empty() {
                        &payload.address
                    } else {
                        leaf(payload, &path, b)?
                            .ok_or_else(|| err("constant constructor lacks its actual leaf"))?
                    };
                    let (literal, dependencies) = observe(permission, solution, &nodes, b)?;
                    let obligation = nodes.get_mut(&node).unwrap();
                    obligation.writes += 1;
                    obligation.literal_root |= literal;
                    b.spend(dependencies.len())?;
                    obligation.dependencies.extend(dependencies);
                }
            }
        }
    }
    b.spend(nodes.len())?;
    if nodes.values().any(|node| node.writes == 0) {
        return Err(err(
            "constant field has no actual constructor or initializer",
        ));
    }
    let mut rooted = BTreeSet::new();
    loop {
        let mut changed = false;
        b.spend(nodes.len())?;
        for (node, obligation) in &nodes {
            b.spend(rooted.len().saturating_add(obligation.dependencies.len()))?;
            if !rooted.contains(node)
                && (obligation.literal_root
                    || obligation
                        .dependencies
                        .iter()
                        .any(|dependency| rooted.contains(dependency)))
            {
                b.spend(node.1.path.len().saturating_add(1))?;
                rooted.insert(node.clone());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    if rooted.len() != nodes.len() {
        return Err(err(
            "constant field dependency cycle has no actual program-static root",
        ));
    }
    Ok(())
}
