//! Exact positive-union effect constraints extracted by the existing source
//! transfer. State propagation is unchanged. Only the inter-function effect
//! fixed point is replaced, and independent Verify is never shared or skipped.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Kind {
    Argument(u32),
    BorrowedField(u32),
    AnyBorrowed,
}
type Point = (usize, Kind);
#[derive(Default)]
pub(super) struct Dependencies(RefCell<BTreeSet<(Point, Point)>>);
impl Dependencies {
    pub(super) fn connect(
        &self,
        function: usize,
        kind: Kind,
        caller: usize,
        sources: &BTreeSet<Source>,
        budget: &mut Budget,
    ) -> CompileResult<()> {
        let mut edges = self.0.borrow_mut();
        for source in sources {
            budget.lookup(edges.len(), "resource summary constraint")?;
            let output = match source {
                Source::Argument(i) => Kind::Argument(*i),
                Source::BorrowedField(i) => Kind::BorrowedField(*i),
                _ => continue,
            };
            edges.insert(((function, kind), (caller, output)));
        }
        Ok(())
    }
}
pub(super) fn solve(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    budget: &mut Budget,
) -> CompileResult<Vec<Effect>> {
    budget.product(module.functions.len(), 2, 1, "resource summary seed tables")?;
    let empty = vec![Effect::default(); module.functions.len()];
    let dependencies = Dependencies::default();
    let mut effects = Vec::with_capacity(module.functions.len());
    for (f, (_, reachable)) in module.functions.iter().zip(graphs) {
        #[cfg(test)]
        TEST_EFFECT_SOLVES.with(|value| value.set(value.get() + 1));
        effects.push(analyze_function_with_dependencies(
            module,
            f,
            reachable,
            &empty,
            false,
            budget,
            Some(&dependencies),
        )?);
    }
    let mut users: BTreeMap<Point, Vec<Point>> = BTreeMap::new();
    for (input, output) in dependencies.0.into_inner() {
        budget.lookup(users.len(), "resource summary index")?;
        users.entry(input).or_default().push(output);
    }
    let mut pending = VecDeque::new();
    let mut seen = BTreeSet::new();
    for (index, effect) in effects.iter().enumerate() {
        for kind in effect.arguments.iter().map(|i| Kind::Argument(*i)).chain(
            effect
                .borrowed_fields
                .iter()
                .map(|i| Kind::BorrowedField(*i)),
        ) {
            budget.lookup(seen.len(), "resource summary seeds")?;
            if seen.insert((index, kind)) {
                pending.push_back((index, kind));
            }
        }
    }
    while let Some(point) = pending.pop_front() {
        budget.spend(1, "resource summary fact pop")?;
        if let Kind::BorrowedField(_) = point.1 {
            // Existing closure calls observe the disjunction, not the index,
            // of writes through borrowed captures. Preserve that exact rule.
            let any = (point.0, Kind::AnyBorrowed);
            budget.lookup(seen.len(), "resource summary capture disjunction")?;
            if seen.insert(any) {
                pending.push_back(any);
            }
        }
        budget.lookup(users.len(), "resource summary consumers")?;
        if let Some(outputs) = users.get(&point) {
            for &output in outputs {
                budget.lookup(seen.len(), "resource summary fact insertion")?;
                if seen.insert(output) {
                    let effect = effects
                        .get_mut(output.0)
                        .ok_or_else(|| error("absent summary destination"))?;
                    match output.1 {
                        Kind::Argument(i) => {
                            effect.arguments.insert(i);
                        }
                        Kind::BorrowedField(i) => {
                            effect.borrowed_fields.insert(i);
                        }
                        Kind::AnyBorrowed => return Err(error("invalid summary output kind")),
                    }
                    pending.push_back(output);
                }
            }
        }
    }
    Ok(effects)
}
