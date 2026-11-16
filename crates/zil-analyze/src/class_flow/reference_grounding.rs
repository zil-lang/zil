//! Grounding alternatives for independently checked mutable-reference roles.
//! These directed edges never equate their source classes or grant a root/loan.
use super::*;
use std::collections::BTreeMap;

// Select a physical subobject of one actual Address head. Encountering an
// intervening stored Address/Desc is a different source, never this head.
pub(crate) fn input_leaf(
    input: &Type,
    path: &[zil_ir::ContractProjection],
    expected: &Type,
    b: &mut Budget,
) -> CompileResult<bool> {
    b.type_work(input, false, "reference source complete input")?;
    b.type_work(expected, false, "reference source complete captured leaf")?;
    let (
        Type::Address { mutable, inner },
        Type::Address {
            mutable: wanted,
            inner: leaf,
        },
    ) = (input, expected)
    else {
        return Ok(false);
    };
    if *wanted && !*mutable {
        return Ok(false);
    }
    let mut selected = inner.as_ref();
    for step in path {
        b.spend(1, "reference source actual physical path")?;
        selected = match (step, selected) {
            (
                zil_ir::ContractProjection::Place(PlaceProjection::Field(index)),
                Type::Record { fields, .. } | Type::Package { fields, .. },
            ) => match fields.get(*index as usize) {
                Some((_, ty)) => ty,
                None => return Ok(false),
            },
            (
                zil_ir::ContractProjection::Place(PlaceProjection::Field(index)),
                Type::Tuple(fields),
            ) => match fields.get(*index as usize) {
                Some(ty) => ty,
                None => return Ok(false),
            },
            _ => return Ok(false),
        };
    }
    Ok(selected == leaf.as_ref())
}

#[derive(Default)]
pub(super) struct Grounding {
    edges: Vec<(usize, usize)>,
    static_roots: BTreeSet<usize>,
}
impl Grounding {
    pub(super) fn store(
        &mut self,
        target: usize,
        source: usize,
        b: &mut Budget,
    ) -> CompileResult<()> {
        b.spend(1, "reference role actual source edge")?;
        self.edges.push((target, source));
        Ok(())
    }
    pub(super) fn call(
        &mut self,
        module: &TypedModule,
        callee: &TypedFunction,
        arguments: &[ValueId],
        output: Option<&Ports>,
        values: &[Ports],
        b: &mut Budget,
    ) -> CompileResult<()> {
        if !b
            .initialization
            .as_ref()
            .is_some_and(|init| init.module == module as *const TypedModule as usize)
        {
            return Ok(());
        }
        b.spend(
            b.semantic_reference_results.len(),
            "completed reference result sources",
        )?;
        let roles: Vec<_> = b
            .semantic_reference_results
            .iter()
            .filter(|(function, _, _)| *function == callee.id)
            .copied()
            .collect();
        if roles.is_empty() {
            return Ok(());
        }
        let output = output.ok_or_else(|| bad("reference result source has no actual output"))?;
        let Type::Closure(id) = &callee.return_type else {
            return Err(bad("reference result source changes its actual closure"));
        };
        let closure = module
            .closures
            .get(id.0 as usize)
            .filter(|row| row.id == *id)
            .ok_or_else(|| bad("reference result closure is absent"))?;
        let life = closure
            .storage_life
            .as_ref()
            .ok_or_else(|| bad("reference result closure domain is unknown"))?;
        let contract = callee
            .callable_return_contract
            .as_ref()
            .filter(|contract| contract.closure == *id)
            .and_then(|contract| contract.capture_sources.as_ref())
            .ok_or_else(|| bad("reference result has no complete capture source relation"))?;
        for (_, child, _) in roles {
            let target = *output
                .classes
                .get(child as usize)
                .ok_or_else(|| bad("reference result source omits its complete class"))?;
            if output.keys.get(child as usize) != Some(&(Vec::new(), child)) {
                return Err(bad("reference result source changes its exact class path"));
            }
            let mut count = 0usize;
            for binding in &life.bindings {
                b.spend(
                    binding.position.path.len().saturating_add(1),
                    "reference capture source selection",
                )?;
                if binding.binder != StorageBinder::Fixed(child) {
                    continue;
                }
                let StorageBase::Capture(capture) = binding.position.base else {
                    continue;
                };
                if !binding.position.path.is_empty() {
                    return Err(bad("reference result capture has another physical path"));
                }
                b.spend(contract.len(), "reference capture complete upper")?;
                let mut matches = contract
                    .iter()
                    .filter(|row| row.position == binding.position);
                let bound = matches
                    .next()
                    .ok_or_else(|| bad("reference capture source upper is missing"))?;
                if matches.next().is_some() || bound.sources.is_empty() {
                    return Err(bad("reference capture source upper is ambiguous or empty"));
                }
                for source in &bound.sources {
                    crate::dyn_metadata::recipe(source, b)?;
                    match source {
                        StorageSource::Exact(position) => {
                            let StorageBase::Parameter(parameter) = position.base else {
                                return Err(bad("reference result source is not a callee input"));
                            };
                            let input = callee
                                .parameters
                                .get(parameter as usize)
                                .and_then(|id| callee.locals.get(id.0 as usize))
                                .ok_or_else(|| {
                                    bad("reference result source omits its complete input type")
                                })?;
                            let capture_type = closure
                                .captures
                                .get(capture as usize)
                                .ok_or_else(|| bad("reference result capture is absent"))?;
                            if !input_leaf(&input.ty, &position.path, capture_type, b)? {
                                return Err(bad(
                                    "reference result source changes its complete physical input leaf",
                                ));
                            }
                            let argument = arguments
                                .get(parameter as usize)
                                .and_then(|id| values.get(id.0 as usize))
                                .and_then(|ports| ports.outer)
                                .ok_or_else(|| {
                                    bad("reference result source has no actual Address operand")
                                })?;
                            self.store(target, argument, b)?;
                        }
                        StorageSource::Static { leaf, mode } => {
                            let capture = closure
                                .captures
                                .get(capture as usize)
                                .ok_or_else(|| bad("reference result static capture is absent"))?;
                            b.type_work(capture, false, "reference static complete leaf")?;
                            if *mode != StoredBorrowMode::SharedAddress || capture != leaf {
                                return Err(bad(
                                    "reference result static source changes its complete leaf",
                                ));
                            }
                            // The completed original role and independently checked
                            // return sources establish this static alternative. A raw
                            // source declaration alone never reaches this branch.
                            b.lookup(self.static_roots.len(), "reference static source inventory")?;
                            self.static_roots.insert(target);
                        }
                        _ => {
                            return Err(bad(
                                "reference result source needs its complete actual origin proof",
                            ));
                        }
                    }
                    count += 1;
                }
            }
            if count == 0 {
                return Err(bad("reference result class has no actual capture sources"));
            }
        }
        Ok(())
    }
    pub(super) fn propagate(&self, e: &mut Equations, b: &mut Budget) -> CompileResult<()> {
        if self.edges.is_empty() && self.static_roots.is_empty() {
            return Ok(());
        }
        let mut graph = BTreeMap::<usize, Vec<usize>>::new();
        let mut statics = BTreeSet::new();
        for (target, source) in &self.edges {
            let target = e.root(*target, b)?;
            let source = e.root(*source, b)?;
            b.lookup(graph.len(), "reference role source graph")?;
            graph.entry(target).or_default().push(source);
        }
        for variable in &self.static_roots {
            let root = e.root(*variable, b)?;
            b.lookup(statics.len(), "reference static root graph")?;
            statics.insert(root);
            graph.entry(root).or_default();
        }
        let mut proved = Vec::new();
        for start in graph.keys() {
            let mut pending = vec![*start];
            let mut seen = BTreeSet::new();
            let mut complete = true;
            let mut grounded = false;
            while let Some(root) = pending.pop() {
                b.lookup(seen.len(), "reference role complete source traversal")?;
                if !seen.insert(root) {
                    continue;
                }
                let terminal =
                    e.grounded.as_ref().is_some_and(|rows| rows[root]) || statics.contains(&root);
                grounded |= terminal;
                b.lookup(graph.len(), "reference role source alternatives")?;
                if let Some(sources) = graph.get(&root) {
                    b.spend(sources.len(), "reference role source frontier")?;
                    pending.extend(sources);
                } else if !terminal {
                    complete = false;
                }
            }
            // A cycle alone is not an origin. Every terminal alternative must
            // be independently grounded, and at least one real origin exists.
            if complete && grounded {
                b.spend(1, "reference role completed grounding")?;
                proved.push(*start);
            }
        }
        for variable in proved {
            e.address_origin(variable, b)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projected_input_origin_stops_at_a_stored_reference() {
        let mut budget = Budget::new(&Limits::default());
        let address = |inner| Type::Address {
            mutable: false,
            inner: Box::new(inner),
        };
        let input = address(Type::Record {
            name: "Pair".into(),
            fields: vec![
                ("value".into(), Type::I32),
                ("pointer".into(), address(Type::I32)),
            ],
        });
        let path = |index| {
            vec![zil_ir::ContractProjection::Place(PlaceProjection::Field(
                index,
            ))]
        };
        assert!(input_leaf(&input, &path(0), &address(Type::I32), &mut budget).unwrap());
        assert!(!input_leaf(&input, &path(1), &address(Type::I32), &mut budget).unwrap());
        assert!(!input_leaf(&input, &path(2), &address(Type::I32), &mut budget).unwrap());
        assert!(!input_leaf(&input, &path(0), &address(Type::I64), &mut budget).unwrap());
    }
    #[test]
    fn alternatives_preserve_distinct_classes_and_require_every_origin() {
        let mut budget = Budget::new(&Limits::default());
        let mut equations = Equations {
            parent: vec![],
            sizes: Some(vec![]),
            rigid: vec![],
            next_rigid: 0,
            grounded: Some(vec![]),
            point: None,
        };
        let a = equations.fresh(true, &mut budget).unwrap();
        let b = equations.fresh(true, &mut budget).unwrap();
        let beta = equations.fresh(false, &mut budget).unwrap();
        let unknown = equations.fresh(false, &mut budget).unwrap();
        let incomplete = equations.fresh(false, &mut budget).unwrap();
        let cycle = equations.fresh(false, &mut budget).unwrap();
        let mut graph = Grounding::default();
        for (target, source) in [
            (beta, beta),
            (beta, a),
            (beta, b),
            (incomplete, a),
            (incomplete, unknown),
            (cycle, cycle),
        ] {
            graph.store(target, source, &mut budget).unwrap();
        }
        graph.propagate(&mut equations, &mut budget).unwrap();
        equations.retain(beta, &mut budget).unwrap();
        assert!(equations.equal(a, b, &mut budget).is_err());
        assert!(equations.equal(beta, a, &mut budget).is_err());
        assert!(equations.retain(incomplete, &mut budget).is_err());
        assert!(equations.retain(cycle, &mut budget).is_err());
        equations.address_origin(unknown, &mut budget).unwrap();
        graph.propagate(&mut equations, &mut budget).unwrap();
        equations.retain(incomplete, &mut budget).unwrap();
        assert!(equations.retain(cycle, &mut budget).is_err());
    }
}
