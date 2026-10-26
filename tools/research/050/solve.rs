use std::collections::{BTreeSet, HashMap};

use crate::model::{
    Answer, Error, Limits, Model, Relation, Result, Stats, cardinality, decode, encode,
};

struct Map(Vec<(u64, u64, u64)>);

impl Map {
    fn new(from: &[usize], to: &[usize], domains: &[u8]) -> Self {
        let mut fields = Vec::new();
        let mut source_place = 1;
        for &v in from {
            let mut target_place = 1;
            for &t in to {
                if t == v {
                    fields.push((source_place, u64::from(domains[v]), target_place));
                    break;
                }
                target_place *= u64::from(domains[t]);
            }
            source_place *= u64::from(domains[v]);
        }
        Self(fields)
    }

    fn apply(&self, code: u64) -> u64 {
        self.0
            .iter()
            .map(|&(div, domain, mul)| ((code / div) % domain) * mul)
            .sum()
    }
}

fn union(a: &[usize], b: &[usize]) -> Vec<usize> {
    a.iter()
        .chain(b)
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn join(
    a: &Relation,
    b: &Relation,
    domains: &[u8],
    limits: Limits,
    stats: &mut Stats,
) -> Result<Relation> {
    let scope = union(&a.scope, &b.scope);
    cardinality(&scope, domains)?;
    stats.table(scope.len(), 0, limits)?;
    if a.rows.is_empty() || b.rows.is_empty() {
        return Ok(Relation {
            scope,
            rows: Vec::new(),
        });
    }
    let common: Vec<_> = a
        .scope
        .iter()
        .copied()
        .filter(|v| b.scope.contains(v))
        .collect();
    let b_only: Vec<_> = b
        .scope
        .iter()
        .copied()
        .filter(|v| !a.scope.contains(v))
        .collect();
    let a_key = Map::new(&a.scope, &common, domains);
    let b_key = Map::new(&b.scope, &common, domains);
    let a_out = Map::new(&a.scope, &scope, domains);
    // The right-side contribution must omit shared fields, not double count them.
    let b_projection = Map::new(&b.scope, &b_only, domains);
    let b_out = Map::new(&b_only, &scope, domains);
    stats.charge(a.rows.len().saturating_add(b.rows.len()), limits)?;
    let mut index: HashMap<u64, Vec<u64>> = HashMap::new();
    for &row in &b.rows {
        index
            .entry(b_key.apply(row))
            .or_default()
            .push(b_out.apply(b_projection.apply(row)));
    }
    let mut rows = Vec::new();
    for &left in &a.rows {
        if let Some(rights) = index.get(&a_key.apply(left)) {
            stats.charge(rights.len(), limits)?;
            if rows.len().saturating_add(rights.len()) > limits.rows {
                return Err(Error::Limit("join rows"));
            }
            let prefix = a_out.apply(left);
            rows.extend(rights.iter().map(|r| prefix + r));
        }
    }
    rows.sort_unstable();
    rows.dedup();
    stats.generated_rows += rows.len() as u64;
    stats.table(scope.len(), rows.len(), limits)?;
    Ok(Relation { scope, rows })
}

fn join_many(
    mut inputs: Vec<&Relation>,
    domains: &[u8],
    limits: Limits,
    stats: &mut Stats,
) -> Result<Relation> {
    let mut combined = Relation {
        scope: Vec::new(),
        rows: vec![0],
    };
    while !inputs.is_empty() {
        // Both exact algorithms use this same local joining heuristic.
        let chosen = (0..inputs.len())
            .min_by_key(|&i| {
                (
                    inputs[i]
                        .scope
                        .iter()
                        .filter(|v| !combined.scope.contains(v))
                        .count(),
                    inputs[i].rows.len(),
                    i,
                )
            })
            .unwrap();
        combined = join(&combined, inputs.remove(chosen), domains, limits, stats)?;
    }
    Ok(combined)
}

fn project_one(
    input: &Relation,
    variable: usize,
    domains: &[u8],
    limits: Limits,
    stats: &mut Stats,
) -> Result<NodeResult> {
    let scope: Vec<_> = input
        .scope
        .iter()
        .copied()
        .filter(|&v| v != variable)
        .collect();
    let map = Map::new(&input.scope, &scope, domains);
    let value = Map::new(&input.scope, &[variable], domains);
    stats.charge(input.rows.len(), limits)?;
    let mut pairs: Vec<_> = input
        .rows
        .iter()
        .map(|&r| (map.apply(r), value.apply(r) as u8))
        .collect();
    pairs.sort_unstable();
    pairs.dedup_by_key(|p| p.0);
    let (rows, chosen): (Vec<_>, Vec<_>) = pairs.into_iter().unzip();
    stats.table(scope.len(), rows.len(), limits)?;
    Ok(NodeResult {
        relation: Relation { scope, rows },
        chosen,
    })
}

#[derive(Clone, Copy, Debug)]
enum Input {
    Factor(usize),
    Node(usize),
}

#[derive(Clone, Debug)]
struct Node {
    variable: Option<usize>,
    inputs: Vec<Input>,
    scope: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct Plan {
    nodes: Vec<Node>,
    pub width: usize,
}

impl Plan {
    pub fn build(model: &Model) -> Result<Self> {
        let n = model.domains.len();
        let mut graph = vec![BTreeSet::new(); n];
        for factor in &model.factors {
            for &a in &factor.scope {
                for &b in &factor.scope {
                    if a != b {
                        graph[a].insert(b);
                    }
                }
            }
        }
        let mut remaining: BTreeSet<_> = (0..n).filter(|v| !model.observed.contains(v)).collect();
        let mut order = Vec::new();
        let mut width = model.observed.len();
        while !remaining.is_empty() {
            let v = *remaining
                .iter()
                .min_by_key(|&&v| {
                    let neighbors: Vec<_> = graph[v].iter().copied().collect();
                    let mut fill = 0;
                    for (i, &a) in neighbors.iter().enumerate() {
                        for &b in &neighbors[i + 1..] {
                            fill += usize::from(!graph[a].contains(&b));
                        }
                    }
                    (fill, neighbors.len(), v)
                })
                .unwrap();
            let neighbors: Vec<_> = graph[v].iter().copied().collect();
            width = width.max(neighbors.len() + 1);
            for &a in &neighbors {
                graph[a].remove(&v);
                for &b in &neighbors {
                    if a != b {
                        graph[a].insert(b);
                    }
                }
            }
            graph[v].clear();
            remaining.remove(&v);
            order.push(v);
        }
        let mut active: Vec<_> = model
            .factors
            .iter()
            .enumerate()
            .map(|(i, f)| (Input::Factor(i), f.scope.clone()))
            .collect();
        let mut nodes = Vec::new();
        for variable in order {
            let mut selected = Vec::new();
            let mut kept = Vec::new();
            let mut scope = Vec::new();
            for (input, vars) in active {
                if vars.contains(&variable) {
                    selected.push(input);
                    scope = union(&scope, &vars);
                } else {
                    kept.push((input, vars));
                }
            }
            if selected.is_empty() {
                return Err(Error::Invalid("elimination variable without factor".into()));
            }
            scope.retain(|&v| v != variable);
            let id = nodes.len();
            nodes.push(Node {
                variable: Some(variable),
                inputs: selected,
                scope: scope.clone(),
            });
            kept.push((Input::Node(id), scope));
            active = kept;
        }
        let scope = active
            .iter()
            .fold(Vec::new(), |s, (_, vars)| union(&s, vars));
        if scope != model.observed {
            return Err(Error::Invalid("plan leaves wrong boundary".into()));
        }
        nodes.push(Node {
            variable: None,
            inputs: active.into_iter().map(|p| p.0).collect(),
            scope,
        });
        Ok(Self { nodes, width })
    }
}

#[derive(Clone, Debug)]
struct NodeResult {
    relation: Relation,
    chosen: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct Cache {
    model: Model,
    plan: Plan,
    values: Vec<NodeResult>,
}

impl Cache {
    pub fn execute(model: &Model, plan: Plan, limits: Limits, stats: &mut Stats) -> Result<Self> {
        let mut result = Self {
            model: model.clone(),
            plan,
            values: Vec::new(),
        };
        for i in 0..result.plan.nodes.len() {
            let value = result.compute(i, limits, stats)?;
            result.values.push(value);
            stats.nodes_recomputed += 1;
        }
        stats.stored_summary_rows = result.values.iter().map(|n| n.relation.rows.len()).sum();
        Ok(result)
    }

    fn compute(&self, i: usize, limits: Limits, stats: &mut Stats) -> Result<NodeResult> {
        let node = &self.plan.nodes[i];
        let inputs = node
            .inputs
            .iter()
            .map(|r| match *r {
                Input::Factor(f) => &self.model.factors[f],
                Input::Node(n) => &self.values[n].relation,
            })
            .collect();
        let combined = join_many(inputs, &self.model.domains, limits, stats)?;
        let result = if let Some(v) = node.variable {
            project_one(&combined, v, &self.model.domains, limits, stats)?
        } else {
            NodeResult {
                relation: combined,
                chosen: Vec::new(),
            }
        };
        if result.relation.scope != node.scope {
            return Err(Error::Invalid("summary boundary mismatch".into()));
        }
        Ok(result)
    }

    pub fn answer(&self) -> Result<Answer> {
        let relation = self.values.last().unwrap().relation.clone();
        let mut witnesses = Vec::with_capacity(relation.rows.len());
        for &row in &relation.rows {
            let mut values = vec![0; self.model.domains.len()];
            decode(row, &relation.scope, &self.model.domains, &mut values);
            for (node, summary) in self.plan.nodes.iter().zip(&self.values).rev() {
                if let Some(variable) = node.variable {
                    let key = encode(&summary.relation.scope, &self.model.domains, &values);
                    let index = summary
                        .relation
                        .rows
                        .binary_search(&key)
                        .map_err(|_| Error::Invalid("missing eliminated witness".into()))?;
                    values[variable] = summary.chosen[index];
                }
            }
            witnesses.push(values);
        }
        Ok(Answer {
            relation,
            witnesses,
        })
    }

    pub fn update(&mut self, model: &Model, limits: Limits) -> Result<Stats> {
        model.validate(limits)?;
        let mut stats = Stats::default();
        let structure_changed = self.model.domains != model.domains
            || self.model.observed != model.observed
            || self.model.factors.len() != model.factors.len()
            || self
                .model
                .factors
                .iter()
                .zip(&model.factors)
                .any(|(a, b)| a.scope != b.scope);
        if structure_changed {
            // Publish only after the entire new calculation succeeds.
            let rebuilt = Self::execute(model, Plan::build(model)?, limits, &mut stats)?;
            *self = rebuilt;
            return Ok(stats);
        }
        let context_changed = self.model.context != model.context;
        let changed: Vec<_> = self
            .model
            .factors
            .iter()
            .zip(&model.factors)
            .map(|(a, b)| a != b)
            .collect();
        // Build only the new values. A dirty old summary may exceed the new
        // limits, so do not clone it merely to overwrite it during recomputation.
        let mut staged = Self {
            model: model.clone(),
            plan: self.plan.clone(),
            values: Vec::with_capacity(self.values.len()),
        };
        let mut dirty = vec![false; staged.plan.nodes.len()];
        for i in 0..staged.plan.nodes.len() {
            dirty[i] = context_changed
                || staged.plan.nodes[i].inputs.iter().any(|r| match *r {
                    Input::Factor(f) => changed[f],
                    Input::Node(n) => dirty[n],
                });
            if dirty[i] {
                let value = staged.compute(i, limits, &mut stats)?;
                staged.values.push(value);
                stats.nodes_recomputed += 1;
            } else {
                let value = &self.values[i];
                // Current table limits also cover reused summaries and the
                // final output, before allocating their transactional copies.
                stats.table(
                    value.relation.scope.len(),
                    value.relation.rows.len(),
                    limits,
                )?;
                stats.charge(value.relation.rows.len(), limits)?;
                stats.charge(value.chosen.len(), limits)?;
                staged.values.push(value.clone());
                stats.nodes_reused += 1;
            }
        }
        stats.stored_summary_rows = staged.values.iter().map(|n| n.relation.rows.len()).sum();
        *self = staged;
        Ok(stats)
    }
}

pub fn flat(model: &Model, limits: Limits, stats: &mut Stats) -> Result<Answer> {
    let full = join_many(
        model.factors.iter().collect(),
        &model.domains,
        limits,
        stats,
    )?;
    let projection = Map::new(&full.scope, &model.observed, &model.domains);
    stats.charge(full.rows.len(), limits)?;
    let mut pairs: Vec<_> = full
        .rows
        .iter()
        .map(|&r| (projection.apply(r), r))
        .collect();
    pairs.sort_unstable();
    pairs.dedup_by_key(|p| p.0);
    let mut rows = Vec::with_capacity(pairs.len());
    let mut witnesses = Vec::with_capacity(pairs.len());
    for (row, full_row) in pairs {
        rows.push(row);
        let mut assignment = vec![0; model.domains.len()];
        decode(full_row, &full.scope, &model.domains, &mut assignment);
        witnesses.push(assignment);
    }
    stats.stored_summary_rows = full.rows.len();
    Ok(Answer {
        relation: Relation {
            scope: model.observed.clone(),
            rows,
        },
        witnesses,
    })
}
