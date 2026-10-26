use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid(String),
    Limit(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(s) => write!(f, "Invalid: {s}"),
            Self::Limit(s) => write!(f, "ResourceLimit: {s}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relation {
    pub scope: Vec<usize>,
    pub rows: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Model {
    pub name: String,
    pub domains: Vec<u8>,
    pub observed: Vec<usize>,
    pub factors: Vec<Relation>,
    // Conservative finite-experiment context, not a complete Rust solver key.
    pub context: [u64; 4],
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub rows: usize,
    pub work: u64,
    pub scope: usize,
    pub oracle_nodes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            rows: 131_072,
            work: 5_000_000,
            scope: 24,
            oracle_nodes: 5_000_000,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub work: u64,
    pub max_rows: usize,
    pub max_scope: usize,
    pub generated_rows: u64,
    pub stored_summary_rows: usize,
    pub nodes_recomputed: usize,
    pub nodes_reused: usize,
}

impl Stats {
    pub fn charge(&mut self, n: usize, limits: Limits) -> Result<()> {
        self.work = self
            .work
            .checked_add(n as u64)
            .ok_or(Error::Limit("work overflow"))?;
        if self.work > limits.work {
            return Err(Error::Limit("relation work"));
        }
        Ok(())
    }

    pub fn table(&mut self, scope: usize, rows: usize, limits: Limits) -> Result<()> {
        if scope > limits.scope {
            return Err(Error::Limit("relation scope"));
        }
        if rows > limits.rows {
            return Err(Error::Limit("relation rows"));
        }
        self.max_scope = self.max_scope.max(scope);
        self.max_rows = self.max_rows.max(rows);
        Ok(())
    }
}

pub fn cardinality(scope: &[usize], domains: &[u8]) -> Result<u64> {
    scope.iter().try_fold(1u64, |n, &v| {
        n.checked_mul(u64::from(domains[v]))
            .ok_or(Error::Limit("tuple encoding"))
    })
}

pub fn encode(scope: &[usize], domains: &[u8], assignment: &[u8]) -> u64 {
    let mut code = 0;
    let mut place = 1;
    for &v in scope {
        code += u64::from(assignment[v]) * place;
        place *= u64::from(domains[v]);
    }
    code
}

pub fn decode(mut code: u64, scope: &[usize], domains: &[u8], assignment: &mut [u8]) {
    for &v in scope {
        assignment[v] = (code % u64::from(domains[v])) as u8;
        code /= u64::from(domains[v]);
    }
}

impl Model {
    pub fn validate(&self, limits: Limits) -> Result<()> {
        if self.domains.len() > 32 || self.domains.iter().any(|&d| !(1..=4).contains(&d)) {
            return Err(Error::Invalid(
                "finite domains must be 1..=4; at most 32 variables".into(),
            ));
        }
        let valid_scope = |s: &[usize]| {
            s.iter().all(|&v| v < self.domains.len()) && s.windows(2).all(|w| w[0] < w[1])
        };
        if !valid_scope(&self.observed) {
            return Err(Error::Invalid("observation scope".into()));
        }
        let mut seen = vec![false; self.domains.len()];
        let mut input_rows = 0u64;
        for factor in &self.factors {
            if !valid_scope(&factor.scope) || !factor.rows.windows(2).all(|w| w[0] < w[1]) {
                return Err(Error::Invalid(
                    "factor must have sorted unique scope/rows".into(),
                ));
            }
            if factor.scope.len() > limits.scope || factor.rows.len() > limits.rows {
                return Err(Error::Limit("input relation"));
            }
            input_rows = input_rows.saturating_add(factor.rows.len() as u64);
            if input_rows > limits.work {
                return Err(Error::Limit("input table work"));
            }
            let count = cardinality(&factor.scope, &self.domains)?;
            if factor.rows.last().is_some_and(|&r| r >= count) {
                return Err(Error::Invalid("factor tuple out of domain".into()));
            }
            for &v in &factor.scope {
                seen[v] = true;
            }
        }
        if seen.contains(&false) {
            return Err(Error::Invalid(
                "unconstrained variable needs explicit domain factor".into(),
            ));
        }
        cardinality(&self.observed, &self.domains)?;
        Ok(())
    }

    pub fn canonical(&self) -> String {
        let mut s = format!(
            "{}\ndomains {:?}\nobserved {:?}\ncontext {:?}\n",
            self.name, self.domains, self.observed, self.context
        );
        for f in &self.factors {
            s.push_str(&format!("{:?}: {:?}\n", f.scope, f.rows));
        }
        s
    }
}

#[derive(Clone, Debug)]
pub struct Answer {
    pub relation: Relation,
    pub witnesses: Vec<Vec<u8>>,
}

// This checker sees only original factors and full assignments. It cannot prove
// completeness; the independent enumerator checks that separately.
pub fn check_answer(model: &Model, answer: &Answer) -> Result<()> {
    if answer.relation.scope != model.observed
        || answer.relation.rows.len() != answer.witnesses.len()
        || !answer.relation.rows.windows(2).all(|w| w[0] < w[1])
    {
        return Err(Error::Invalid("answer structure".into()));
    }
    for (&row, values) in answer.relation.rows.iter().zip(&answer.witnesses) {
        if values.len() != model.domains.len()
            || values.iter().zip(&model.domains).any(|(&x, &d)| x >= d)
        {
            return Err(Error::Invalid("witness domain".into()));
        }
        if encode(&model.observed, &model.domains, values) != row {
            return Err(Error::Invalid("witness projection".into()));
        }
        for factor in &model.factors {
            if factor
                .rows
                .binary_search(&encode(&factor.scope, &model.domains, values))
                .is_err()
            {
                return Err(Error::Invalid("witness violates original factor".into()));
            }
        }
    }
    Ok(())
}
