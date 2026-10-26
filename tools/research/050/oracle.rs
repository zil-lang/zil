use crate::model::{Error, Limits, Model, Relation, Result};
use std::collections::BTreeSet;

// Independent implementation: decoded value tuples, chronological DFS and
// checks of original tables. No import from solve; no elimination or summary.
struct Check {
    variables: Vec<usize>,
    allowed: BTreeSet<Vec<u8>>,
}

pub fn enumerate(model: &Model, limits: Limits) -> Result<(Relation, u64)> {
    let mut schedule: Vec<Vec<Check>> = (0..=model.domains.len()).map(|_| Vec::new()).collect();
    for relation in &model.factors {
        let mut allowed = BTreeSet::new();
        for &encoded in &relation.rows {
            let mut remaining = encoded;
            let mut tuple = Vec::new();
            for &variable in &relation.scope {
                let d = u64::from(model.domains[variable]);
                tuple.push((remaining % d) as u8);
                remaining /= d;
            }
            allowed.insert(tuple);
        }
        let depth = relation.scope.last().map_or(0, |v| v + 1);
        schedule[depth].push(Check {
            variables: relation.scope.clone(),
            allowed,
        });
    }
    struct Search<'a> {
        model: &'a Model,
        schedule: &'a [Vec<Check>],
        values: Vec<u8>,
        answers: BTreeSet<Vec<u8>>,
        nodes: u64,
        limits: Limits,
    }
    impl Search<'_> {
        fn visit(&mut self, depth: usize) -> Result<()> {
            self.nodes += 1;
            if self.nodes > self.limits.oracle_nodes {
                return Err(Error::Limit("oracle search nodes"));
            }
            for check in &self.schedule[depth] {
                let tuple: Vec<_> = check.variables.iter().map(|&v| self.values[v]).collect();
                if !check.allowed.contains(&tuple) {
                    return Ok(());
                }
            }
            if depth == self.values.len() {
                let tuple: Vec<_> = self
                    .model
                    .observed
                    .iter()
                    .map(|&v| self.values[v])
                    .collect();
                if self.answers.len() == self.limits.rows && !self.answers.contains(&tuple) {
                    return Err(Error::Limit("oracle answer rows"));
                }
                self.answers.insert(tuple);
            } else {
                for value in 0..self.model.domains[depth] {
                    self.values[depth] = value;
                    self.visit(depth + 1)?;
                }
            }
            Ok(())
        }
    }
    let mut search = Search {
        model,
        schedule: &schedule,
        values: vec![0; model.domains.len()],
        answers: BTreeSet::new(),
        nodes: 0,
        limits,
    };
    search.visit(0)?;
    let mut rows = Vec::new();
    for tuple in search.answers {
        let mut row = 0;
        let mut multiplier = 1;
        for (value, &variable) in tuple.into_iter().zip(&model.observed) {
            row += u64::from(value) * multiplier;
            multiplier *= u64::from(model.domains[variable]);
        }
        rows.push(row);
    }
    rows.sort_unstable();
    Ok((
        Relation {
            scope: model.observed.clone(),
            rows,
        },
        search.nodes,
    ))
}
