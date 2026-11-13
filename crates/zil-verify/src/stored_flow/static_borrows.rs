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
        static_dependency: StaticProvenance::Conditional(node.clone()),
        retained_port: None,
        base: HoldRoot::Static,
        leaf: vec![],
        inline: vec![],
        heap_parents: vec![],
        heap_element: None,
    }])
}

/// This is a conditional result obligation, never a literal root. Scalar
/// arguments and an actually empty environment cannot mutate caller storage;
/// each real Return is still checked by `validate` before any module succeeds.
/// In particular the declaration cannot make an ungrounded recursive result
/// complete by itself: its exact Result node must reach a real program literal.
pub(super) fn stateless_result(
    module: &TypedModule,
    definition: &ClosureDefinition,
    b: &mut Budget,
) -> Check<bool> {
    if !b.static_borrow_occurrences {
        return Ok(false);
    }
    b.spend(
        definition
            .captures
            .len()
            .saturating_add(definition.parameters.len())
            .saturating_add(1),
    )?;
    let Some(life) = &definition.storage_life else {
        return Ok(false);
    };
    b.spend(
        life.bindings
            .len()
            .saturating_add(life.static_bindings.len())
            .saturating_add(life.classes.len()),
    )?;
    if !definition.captures.is_empty()
        || !definition.lending_captures.is_empty()
        || !definition.parameters.iter().all(stored_callables::scalar)
        || !matches!(definition.result, Type::Address { mutable: false, .. })
        || life.fixed_binder_count != 0
        || life.invocation_binder_count != 0
        || !life.bindings.is_empty()
        || !life.classes.is_empty()
        || life.static_bindings.len() != 1
        || life.static_bindings[0].base != StorageBase::Result
        || !life.static_bindings[0].path.is_empty()
    {
        return Ok(false);
    }
    let function = module
        .functions
        .get(definition.function.0 as usize)
        .ok_or_else(|| err("constant result has no actual entry"))?;
    b.spend(function.parameters.len().saturating_add(1))?;
    // Deep header equality is paid again at each conditional call site; a
    // previous module metadata scan is no certificate of this comparison.
    type_work(&function.return_type, &b.limits.clone(), b)?;
    type_work(&definition.result, &b.limits.clone(), b)?;
    if function.environment != Some(definition.id)
        || function.storage_life.as_ref() != Some(life)
        || function.return_type != definition.result
        || function.foreign.is_some()
        || function.call_kind != CallKind::Direct
        || function.result_mode != PassingMode::Infer
        || function.parameters.len() != definition.parameters.len()
    {
        return Err(err(
            "constant result entry differs from its actual definition",
        ));
    }
    for (parameter, expected) in function.parameters.iter().zip(&definition.parameters) {
        if function
            .locals
            .get(parameter.0 as usize)
            .map(|local| &local.ty)
            != Some(expected)
        {
            return Err(err(
                "constant result entry has a non-scalar or mismatched input",
            ));
        }
    }
    Ok(true)
}

pub(super) fn result_source(definition: &ClosureDefinition, b: &mut Budget) -> Check<Vec<Source>> {
    constant_sources(
        &(
            definition.id,
            StoragePosition {
                base: StorageBase::Result,
                path: vec![],
            },
        ),
        b,
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stateless_result_remains_a_conditional_obligation_of_the_exact_actual_entry() {
        let limits = Limits::default();
        let (module, _) = zil_ir::wire::decode_module_candidate(
            include_bytes!(
                "../../tests/fixtures/dyn-context/static-callback-getter-cycle.typed.wire"
            ),
            &limits,
        )
        .unwrap();
        let definition = &module.closures[0];
        let mut budget = Budget::new(&limits);
        budget.static_borrow_occurrences = true;
        assert!(stateless_result(&module, definition, &mut budget).unwrap());
        let sources = result_source(definition, &mut budget).unwrap();
        assert!(matches!(&sources[0].static_dependency,
            StaticProvenance::Conditional((id, position))
                if *id == definition.id && position.base == StorageBase::Result));
        for mutation in 0..4 {
            let mut forged = definition.clone();
            match mutation {
                0 => forged.storage_life = None,
                1 => forged.storage_life.as_mut().unwrap().fixed_binder_count = 1,
                2 => forged.captures.push(definition.result.clone()),
                3 => forged.parameters[0] = definition.result.clone(),
                _ => unreachable!(),
            }
            assert!(!stateless_result(&module, &forged, &mut budget).unwrap());
        }
        let mut forged = module.clone();
        forged.functions[definition.function.0 as usize].storage_life = None;
        assert!(stateless_result(&forged, definition, &mut budget).is_err());
    }

    #[test]
    fn upper_evidence_does_not_split_the_identity_of_a_vacant_physical_slot() {
        let actual = Source {
            base: HoldRoot::Parameter(0),
            retained_port: None,
            leaf: vec![],
            inline: vec![],
            heap_element: None,
            heap_parents: vec![],
            static_dependency: StaticProvenance::NotStatic,
        };
        let state = Frame {
            vacant: vec![Source {
                inline: vec![PlaceProjection::Field(0)],
                ..actual.clone()
            }],
            ..Frame::default()
        };
        let upper = Permission {
            sources: vec![Source {
                static_dependency: StaticProvenance::UpperOnly,
                ..actual
            }],
            loans: vec![],
        };
        let mut budget = Budget::new(&Limits::default());
        assert!(
            unavailable_slot(&state, &upper, &[PlaceProjection::Field(0)], &mut budget).is_err()
        );
        assert!(
            unavailable_slot(&state, &upper, &[PlaceProjection::Field(1)], &mut budget).is_ok()
        );
    }

    #[test]
    fn only_actual_provenance_can_root_a_constant_occurrence() {
        let node = (
            ClosureId(0),
            StoragePosition {
                base: StorageBase::Capture(0),
                path: vec![],
            },
        );
        let nodes = BTreeMap::from([(node.clone(), Obligation::default())]);
        let source = |static_dependency| Source {
            static_dependency,
            retained_port: None,
            base: HoldRoot::Static,
            leaf: vec![],
            inline: vec![],
            heap_element: None,
            heap_parents: vec![],
        };
        let upper = source(StaticProvenance::UpperOnly);
        let literal = source(StaticProvenance::Literal);
        let conditional = source(StaticProvenance::Conditional(node.clone()));
        let solution = Solution {
            values: vec![],
            outputs: vec![],
            entry: Frame::default(),
            loans: vec![],
            sites: vec![],
            dyn_sites: dyn_leases::Sites::default(),
        };
        let inspect = |sources| {
            observe(
                &Permission {
                    sources,
                    loans: vec![],
                },
                &solution,
                &nodes,
                &mut Budget::new(&Limits::default()),
            )
        };
        assert!(inspect(vec![upper.clone()]).is_err());
        assert!(inspect(vec![source(StaticProvenance::NotStatic)]).is_err());
        assert_eq!(
            inspect(vec![upper.clone(), conditional.clone()]).unwrap(),
            (false, BTreeSet::from([node.clone()])),
        );
        assert_eq!(
            inspect(vec![upper.clone(), literal.clone()]).unwrap(),
            (true, BTreeSet::new()),
        );
        assert_eq!(
            inspect(vec![upper.clone(), conditional.clone(), literal.clone()]).unwrap(),
            (true, BTreeSet::from([node.clone()])),
        );
        // A genuine literal *parent* may keep an upper argument alive, but is
        // no producer of the conditional data. It cannot seed that data cycle.
        let mut parented = solution;
        parented.loans.push(Loan {
            sources: vec![literal],
            ..Loan::default()
        });
        assert_eq!(
            observe(
                &Permission {
                    sources: vec![conditional],
                    loans: vec![0]
                },
                &parented,
                &nodes,
                &mut Budget::new(&Limits::default())
            )
            .unwrap(),
            (false, BTreeSet::from([node]))
        );
        parented.loans[0].sources = vec![Source {
            base: HoldRoot::Local(LocalId(0)),
            ..upper
        }];
        assert!(
            observe(
                &Permission {
                    sources: vec![source(StaticProvenance::Literal)],
                    loans: vec![0]
                },
                &parented,
                &nodes,
                &mut Budget::new(&Limits::default())
            )
            .is_err()
        );
    }
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
    constructors: usize,
    literal_root: bool,
    dependencies: BTreeSet<Node>,
}

fn retain_observation(
    obligation: &mut Obligation,
    literal: bool,
    dependencies: BTreeSet<Node>,
    b: &mut Budget,
) -> Check<()> {
    // Prepay complete variable-length keys before the ordered-set merge.
    // The linear bound dominates its search work without trusting a cache.
    b.spend(obligation.dependencies.len())?;
    let mut existing_width = 0usize;
    for node in &obligation.dependencies {
        existing_width = existing_width.saturating_add(node.1.path.len().saturating_add(2));
    }
    for node in &dependencies {
        b.spend(
            amount(
                node.1.path.len().saturating_add(2),
                obligation.dependencies.len().saturating_add(1),
            )?
            .saturating_add(existing_width),
        )?;
    }
    obligation.literal_root |= literal;
    obligation.dependencies.extend(dependencies);
    Ok(())
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
    let mut inspect = |sources: &[Source], data: bool, b: &mut Budget| -> Check<bool> {
        source_work(sources, b)?;
        if sources.is_empty() {
            return Err(err("constant field has an ungrounded parent"));
        }
        let mut actual = false;
        for source in sources {
            if source.static_dependency == StaticProvenance::UpperOnly && data {
                continue;
            }
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
            if let StaticProvenance::Conditional(node) = &source.static_dependency {
                actual = true;
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
                if data {
                    dependencies.insert(node.clone());
                }
            } else {
                match source.static_dependency {
                    StaticProvenance::Literal => {
                        actual = true;
                        root |= data;
                    }
                    // A conservative category is no second actual target or
                    // parent obligation when genuine provenance accompanies it.
                    StaticProvenance::UpperOnly => {}
                    StaticProvenance::NotStatic => {
                        return Err(err("constant field has unclassified static provenance"));
                    }
                    StaticProvenance::Conditional(_) => unreachable!(),
                }
            }
        }
        Ok(actual)
    };
    if !inspect(&permission.sources, true, b)? {
        return Err(err(
            "constant field has only an upper category, not actual static provenance",
        ));
    }
    for id in ancestry(&permission.loans, &solution.loans, b)? {
        let loan = solution
            .loans
            .get(id)
            .ok_or_else(|| err("constant field parent is absent"))?;
        if loan.receiver_head || loan.exclusive {
            return Err(err("constant field retains a receiver or exclusive parent"));
        }
        // A real literal ancestor proves only that parent's class. It is no
        // data-producing edge and cannot anchor a conditional data cycle.
        inspect(&loan.sources, false, b)?;
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
    let mut has_constant_results = false;
    b.spend(module.closures.len())?;
    for definition in &module.closures {
        if let Some(life) = &definition.storage_life {
            b.spend(life.static_bindings.len())?;
            for position in &life.static_bindings {
                has_constant_results |= position.base == StorageBase::Result;
                if matches!(position.base, StorageBase::Capture(_) | StorageBase::Result) {
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
    if nodes.is_empty() && !has_constant_results {
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
                        for node in nodes.keys().filter(|node| {
                            node.0 == *closure && matches!(node.1.base, StorageBase::Capture(_))
                        }) {
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
                    obligation.constructors += usize::from(matches!(
                        instruction.kind,
                        InstructionKind::MakeClosure { .. }
                    ));
                    retain_observation(obligation, literal, dependencies, b)?;
                }
            }
            // Result Static is a separate actual-body obligation. It cannot
            // be inferred from a constant capture declaration or an empty
            // class directory, and may retain no activation/head ancestry.
            if let Terminator::Return(Some(value)) = block.terminator
                && let Some(definition) = function
                    .environment
                    .and_then(|id| module.closures.get(id.0 as usize))
                && let Some(life) = &definition.storage_life
            {
                b.spend(life.static_bindings.len())?;
                for position in &life.static_bindings {
                    if position.base != StorageBase::Result {
                        continue;
                    }
                    let path = physical(position, b)?;
                    let payload = solution
                        .values
                        .get(value.0 as usize)
                        .ok_or_else(|| err("constant result has no actual return payload"))?;
                    let permission = if path.is_empty() {
                        &payload.address
                    } else {
                        leaf(payload, &path, b)?
                            .ok_or_else(|| err("constant result has no actual return occurrence"))?
                    };
                    let (literal, dependencies) = observe(permission, solution, &nodes, b)?;
                    let node = (definition.id, position.clone());
                    let obligation = nodes.get_mut(&node).ok_or_else(|| {
                        err("constant result has no registered actual occurrence")
                    })?;
                    obligation.constructors += 1;
                    retain_observation(obligation, literal, dependencies, b)?;
                }
            }
        }
    }
    b.spend(nodes.len())?;
    if nodes.values().any(|node| node.constructors == 0) {
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
