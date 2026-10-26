//! Compile the existing source transfer to sparse parameter facts once.
//!
//! Local origin propagation never reads `summaries`. Therefore, for each
//! function, `writes(S) = direct_writes UNION projections(S)`. The scanner
//! collects these exact projections while all callee summaries are empty.
//! Solving the resulting Boolean reachability graph computes the same least
//! fixed point without reinterpreting callers after every callee change.
//! Independent Verify is not changed and does not consume this graph.
use super::*;

type Projection = (FunctionId, usize, usize); // callee, callee parameter, caller origin

#[derive(Default)]
pub(super) struct Projected(BTreeSet<Projection>);
impl Tracking for Projected {
    type Dependencies = BTreeSet<Projection>;
    const ENABLED: bool = true;
    fn observation_cost(origins: &Origins) -> usize {
        origins.len().saturating_add(1)
    }
    fn observe(&mut self, callee: FunctionId, parameter: usize, origins: &Origins) {
        self.0
            .extend(origins.iter().map(|&origin| (callee, parameter, origin)));
    }
    fn finish(self) -> Self::Dependencies {
        self.0
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Stats {
    pub body_evaluations: usize,
    pub facts: usize,
    pub edges: usize,
    pub activated: usize,
    pub traversed_edges: usize,
    pub charged_work: usize,
}

pub(super) fn solve(
    inputs: &[Input<'_>],
    tables: Option<&Tables>,
    limits: &Limits,
) -> R<(Writes, Stats)> {
    if inputs.is_empty() {
        return Ok((Writes::new(), Stats::default()));
    }
    let span = inputs[0].function.span;
    let mut work = limits.max_analysis_steps;
    let mut stats = Stats::default();
    let mut positions = BTreeMap::new();
    let mut facts = Vec::new();
    let mut writes = Writes::new();
    // Allocate only actual descriptor parameters, not F x max_parameter_count.
    // Meter every input/parameter before the corresponding allocation.
    for input in inputs {
        charge(&mut work, 1, input.function.span)?;
        let id = input.signatures[&input.function.name].id;
        writes.insert(id, Origins::new());
        for (parameter, ty) in input.function.parameters.iter().enumerate() {
            charge(&mut work, 1, ty.span)?;
            if matches!(ty.ty, Type::Desc(_)) {
                charge(&mut work, 4, ty.span)?;
                positions.insert((id, parameter), facts.len());
                facts.push((id, parameter));
            }
        }
    }
    stats.facts = facts.len();
    charge(&mut work, facts.len().saturating_mul(2), span)?;
    let mut successors = vec![Vec::new(); facts.len()];
    let mut live = vec![false; facts.len()];
    let mut queue = VecDeque::new();
    let empty = Writes::new();
    for input in inputs {
        let id = input.signatures[&input.function.name].id;
        let (seeds, projections) =
            scan_once::<Projected>(input, tables, &empty, &mut work, limits)?;
        stats.body_evaluations += 1; // local loop-origin convergence remains in the scanner
        for parameter in seeds {
            charge(&mut work, 1, input.function.span)?;
            if let Some(&node) = positions.get(&(id, parameter)) {
                charge(&mut work, 1, input.function.span)?;
                if !live[node] {
                    live[node] = true;
                    queue.push_back(node);
                }
            }
        }
        for (callee, parameter, origin) in projections {
            charge(&mut work, 1, input.function.span)?;
            // Excluded callable helpers / unknown summary rows are also absent
            // from both legacy solvers. They cannot invent an effect fact.
            if let (Some(&from), Some(&to)) = (
                positions.get(&(callee, parameter)),
                positions.get(&(id, origin)),
            ) {
                charge(&mut work, 1, input.function.span)?;
                successors[from].push(to);
                stats.edges += 1;
            }
        }
    }
    while let Some(node) = queue.pop_front() {
        charge(&mut work, 1, span)?;
        stats.activated += 1;
        let (id, parameter) = facts[node];
        charge(&mut work, 1, span)?;
        writes
            .get_mut(&id)
            .expect("initialized effect row")
            .insert(parameter);
        for &target in &successors[node] {
            charge(&mut work, 1, span)?;
            stats.traversed_edges += 1;
            if !live[target] {
                charge(&mut work, 1, span)?;
                live[target] = true;
                queue.push_back(target);
            }
        }
    }
    stats.charged_work = limits.max_analysis_steps - work;
    Ok((writes, stats))
}

#[cfg(test)]
#[path = "resource_effects_compiled_tests.rs"]
mod tests;
