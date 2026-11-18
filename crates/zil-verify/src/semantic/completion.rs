//! One publication boundary for independently checked original executable
//! worlds. Registration carries no theorem; only the actual body interpreter
//! can produce the private completion token accepted here. Memory permissions,
//! phase/cleanup and deferred Static obligations remain separate kernels.
use super::*;

pub(super) struct Builder {
    module: usize,
    expected: BTreeMap<SemanticInstanceId, (FunctionId, SemanticDefinitionId)>,
    directory: Completed,
}

/// These rows contain only the original interpreter's completed bodies.
/// Registration alone grants no result, storage permission or cleanup proof.
pub(crate) struct Completed {
    worlds: Vec<(SemanticInstanceId, EvalEnv)>,
    results: Vec<CheckedType>,
    positions: BTreeMap<SemanticInstanceId, usize>,
    entries: Vec<Vec<SemanticInstanceId>>,
}

fn lookup(size: usize) -> usize {
    size.checked_ilog2().map_or(1, |n| n as usize + 2)
}

fn reserve<T>(rows: &mut Vec<T>, b: &mut Budget) -> Check<()> {
    b.spend(1 + if rows.len() == rows.capacity() { rows.len() } else { 0 })?;
    rows.try_reserve(1).map_err(|_| bad("completed world directory allocation failed"))
}

impl Builder {
    pub(super) fn register(module: &TypedModule, b: &mut Budget) -> Check<Self> {
        let program = module.semantic.as_ref().ok_or_else(|| bad("world registration lacks its original programme"))?;
        let mut expected = BTreeMap::new();
        for row in &program.instances {
            b.spend(1)?;
            if let SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(function)) = row.target {
                b.spend(lookup(expected.len()))?;
                let entry = module.functions.get(function.0 as usize).filter(|entry| entry.id == function)
                    .ok_or_else(|| bad("registered world has no exact actual executable"))?;
                if expected.insert(row.id, (entry.id, row.definition)).is_some() {
                    return Err(bad("world registration repeats an original instance"));
                }
            }
        }
        b.spend(module.functions.len().saturating_add(4))?;
        Ok(Self {
            module: module as *const TypedModule as usize,
            expected,
            directory: Completed {
                worlds: Vec::new(), results: Vec::new(),
                positions: BTreeMap::new(), entries: vec![Vec::new(); module.functions.len()],
            },
        })
    }

    pub(super) fn record(&mut self, proof: flow::BodyChecked, b: &mut Budget) -> Check<()> {
        let (id, definition, function, environment, result) = proof.into_parts();
        let directory = &mut self.directory;
        b.spend(lookup(self.expected.len()).saturating_add(lookup(directory.positions.len())))?;
        if self.expected.get(&id) != Some(&(function, definition)) || directory.positions.contains_key(&id) {
            return Err(bad("completed body differs from its registered original world"));
        }
        let entry = directory.entries.get_mut(function.0 as usize)
            .ok_or_else(|| bad("completed body has no registered executable"))?;
        // All fallible work precedes publication. Different specializations of
        // the same executable remain distinct and retain their execution order.
        b.spend(environment.lexical.len().saturating_add(lookup(directory.positions.len())))?;
        reserve(&mut directory.worlds, b)?;
        reserve(&mut directory.results, b)?;
        reserve(entry, b)?;
        directory.positions.insert(id, directory.worlds.len());
        directory.worlds.push((id, environment));
        directory.results.push(result);
        entry.push(id);
        Ok(())
    }

    pub(super) fn checked(&self) -> &BTreeMap<SemanticInstanceId, usize> { &self.directory.positions }
    pub(super) fn worlds(&self) -> &[(SemanticInstanceId, EvalEnv)] { self.directory.worlds() }
    pub(super) fn environment(&self, id: SemanticInstanceId, b: &mut Budget) -> Check<&EvalEnv> {
        Ok(self.directory.world(id, b)?.0)
    }

    pub(super) fn seal(self, module: &TypedModule, b: &mut Budget) -> Check<Completed> {
        if self.module != module as *const TypedModule as usize {
            return Err(bad("world completion selects another immutable module"));
        }
        let directory = self.directory;
        b.spend(1)?;
        if directory.positions.len() != self.expected.len() || directory.worlds.len() != self.expected.len()
            || directory.results.len() != self.expected.len() {
            return Err(bad("original world directory still has an unresolved executable dependency"));
        }
        // Every inserted world matched its registered key exactly once. Equal
        // cardinality establishes coverage, including multiple worlds per entry.
        for function in &module.functions {
            b.spend(1)?;
            if function.foreign.is_none() && directory.entry(function.id, b)?.is_empty() {
                return Err(bad("semantic programme omits an actual function instance"));
            }
        }
        Ok(directory)
    }
}

impl Completed {
    pub(crate) fn worlds(&self) -> &[(SemanticInstanceId, EvalEnv)] { &self.worlds }

    pub(crate) fn entry(&self, function: FunctionId, b: &mut Budget) -> Check<&[SemanticInstanceId]> {
        b.spend(1)?;
        self.entries.get(function.0 as usize).map(Vec::as_slice)
            .ok_or_else(|| bad("actual function is outside the completed world directory"))
    }

    pub(crate) fn world(&self, id: SemanticInstanceId, b: &mut Budget) -> Check<(&EvalEnv, &CheckedType)> {
        b.spend(lookup(self.positions.len()).saturating_add(2))?;
        let pos = self.positions.get(&id).copied().ok_or_else(|| bad("completed world loses its exact original result frame"))?;
        let (_, environment) = self.worlds.get(pos).filter(|(world, _)| *world == id)
            .ok_or_else(|| bad("completed world loses its exact original environment"))?;
        let result = self.results.get(pos).ok_or_else(|| bad("completed world loses its exact original result frame"))?;
        Ok((environment, result))
    }
}
