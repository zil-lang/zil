use crate::model::{Error, Model, Relation, Result, cardinality, decode};

#[derive(Clone, Debug)]
pub struct Case {
    pub name: String,
    pub recipe: String,
    pub a: usize,
    pub b: u64,
    pub batch: usize,
}

impl Case {
    pub fn parse(line: &str) -> Result<Self> {
        let words: Vec<_> = line.split_whitespace().collect();
        if words.len() != 5 {
            return Err(Error::Invalid("case requires five fields".into()));
        }
        let number = |s: &str| {
            s.parse::<u64>()
                .map_err(|_| Error::Invalid("case integer".into()))
        };
        Ok(Self {
            name: words[0].into(),
            recipe: words[1].into(),
            a: number(words[2])? as usize,
            b: number(words[3])?,
            batch: number(words[4])? as usize,
        })
    }
}

pub fn table(domains: &[u8], mut scope: Vec<usize>, accepts: impl Fn(&[u8]) -> bool) -> Relation {
    scope.sort_unstable();
    scope.dedup();
    let mut rows = Vec::new();
    let mut values = vec![0; domains.len()];
    for code in 0..cardinality(&scope, domains).expect("bounded fixture") {
        decode(code, &scope, domains, &mut values);
        if accepts(&values) {
            rows.push(code);
        }
    }
    Relation { scope, rows }
}

pub fn mix(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e3779b97f4a7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

pub fn build(case: &Case) -> Result<Model> {
    let mut m = Model {
        name: case.name.clone(),
        domains: Vec::new(),
        observed: Vec::new(),
        factors: Vec::new(),
        context: [0; 4],
    };
    match case.recipe.as_str() {
        "chain" => {
            if !(2..=24).contains(&case.a) || !(2..=4).contains(&case.b) {
                return Err(Error::Invalid("chain bounds".into()));
            }
            m.domains = vec![case.b as u8; case.a];
            m.observed = vec![0, case.a - 1];
            for a in 0..case.a - 1 {
                m.factors
                    .push(table(&m.domains, vec![a, a + 1], |v| v[a] != v[a + 1]));
            }
        }
        "grid" => {
            let width = case.a;
            let height = case.b as usize;
            let n = width
                .checked_mul(height)
                .ok_or(Error::Invalid("grid bounds".into()))?;
            if width < 2 || height < 2 || n > 24 {
                return Err(Error::Invalid("grid bounds".into()));
            }
            m.domains = vec![2; n];
            m.observed = vec![0, n - 1];
            for y in 0..height {
                for x in 0..width {
                    let a = y * width + x;
                    for b in [
                        if x + 1 < width { Some(a + 1) } else { None },
                        if y + 1 < height {
                            Some(a + width)
                        } else {
                            None
                        },
                    ]
                    .into_iter()
                    .flatten()
                    {
                        m.factors
                            .push(table(&m.domains, vec![a, b], |v| v[a] == 0 || v[b] == 0));
                    }
                }
            }
        }
        "dense" => {
            if !(2..=20).contains(&case.a) {
                return Err(Error::Invalid("dense bounds".into()));
            }
            m.domains = vec![2; case.a];
            m.observed = vec![0, case.a - 1];
            for a in 0..case.a {
                for b in a + 1..case.a {
                    if mix(case.b ^ ((a as u64) << 32) ^ b as u64) % 5 != 0 {
                        m.factors
                            .push(table(&m.domains, vec![a, b], |v| v[a] == 0 || v[b] == 0));
                    }
                }
            }
        }
        "boundary" => {
            if !(1..=17).contains(&case.a) {
                return Err(Error::Invalid("boundary bounds".into()));
            }
            m.domains = vec![2; case.a];
            m.observed = (0..case.a).collect();
            let rows = (0..(1u64 << case.a))
                .filter(|&r| mix(r ^ case.b) & 1 == 1)
                .collect();
            m.factors.push(Relation {
                scope: m.observed.clone(),
                rows,
            });
        }
        "closure" => {
            // Desc capture does not close a resource. Calls explicitly close its
            // target, and the final cleanup closes each still-live resource.
            m.domains = vec![2; 15];
            m.domains[14] = 3; // 0 normal, 1 creation failure, 2 call failure.
            m.observed = vec![0, 1, 4, 5, 6, 7, 8, 9, 10, 11, 14];
            m.factors
                .push(table(&m.domains, vec![0, 1], |v| match case.a {
                    0 => v[0] != v[1],
                    1 => v[0] == v[1],
                    _ => true,
                }));
            for i in [2, 3] {
                m.factors.push(table(&m.domains, vec![i], |v| v[i] == 1));
            }
            for (source, before_a, before_b, after_a, after_b, ok) in
                [(0, 2, 3, 4, 5, 6), (1, 4, 5, 7, 8, 9)]
            {
                m.factors.push(table(
                    &m.domains,
                    vec![source, before_a, before_b, after_a, after_b, ok],
                    |v| {
                        if v[source] == 0 {
                            v[after_a] == 0 && v[after_b] == v[before_b] && v[ok] == v[before_a]
                        } else {
                            v[after_b] == 0 && v[after_a] == v[before_a] && v[ok] == v[before_b]
                        }
                    },
                ));
            }
            m.factors
                .push(table(&m.domains, vec![7, 8, 10, 11, 12, 13], |v| {
                    v[10] == v[7] && v[11] == v[8] && v[12] == 0 && v[13] == 0
                }));
            m.factors.push(table(&m.domains, vec![6, 9, 14], |v| {
                v[14] == if v[6] == 1 && v[9] == 1 { 0 } else { 2 }
            }));
        }
        "owned" => {
            // 0 outer, 1 environment, 2 closed. The finite protocol's consume
            // operation closes immediately; this is not ZIL's general give ABI.
            m.domains = vec![3, 3, 3, 2, 2, 3, 2, 2, 3, 2, 3];
            m.observed = (1..=10).collect();
            m.factors
                .push(table(&m.domains, vec![0, 1], |v| v[0] == 0 && v[1] == 1));
            for (before, after, ok, close, op) in
                [(1, 2, 3, 4, case.a as u64), (2, 5, 6, 7, case.b)]
            {
                if op > 2 {
                    return Err(Error::Invalid("owned operation".into()));
                }
                m.factors
                    .push(table(&m.domains, vec![before, after, ok, close], |v| {
                        let valid = op == 0 || v[before] == 1;
                        let closes = op == 2 && valid;
                        v[ok] == u8::from(valid)
                            && v[close] == u8::from(closes)
                            && v[after] == if closes { 2 } else { v[before] }
                    }));
            }
            m.factors.push(table(&m.domains, vec![5, 8, 9], |v| {
                v[9] == u8::from(v[5] == 1) && v[8] == if v[5] == 1 { 2 } else { v[5] }
            }));
            m.factors.push(table(&m.domains, vec![3, 6, 10], |v| {
                v[10] == if v[3] == 1 && v[6] == 1 { 0 } else { 2 }
            }));
        }
        "trait" => {
            if case.a == 0 {
                m.domains = vec![2];
                m.observed = vec![0];
                m.factors.push(table(&m.domains, vec![0], |v| v[0] == 0));
                m.factors.push(table(&m.domains, vec![0], |v| v[0] == 1));
            } else {
                m.domains = vec![2; 3];
                m.observed = vec![0, 2];
                m.factors
                    .push(table(&m.domains, vec![0, 1], |v| v[0] == v[1]));
                m.factors
                    .push(table(&m.domains, vec![1, 2], |v| v[1] != v[2]));
            }
        }
        _ => return Err(Error::Invalid("unknown recipe".into())),
    }
    for i in 0..m.domains.len() {
        if !m.factors.iter().any(|f| f.scope.contains(&i)) {
            m.factors.push(table(&m.domains, vec![i], |_| true));
        }
    }
    Ok(m)
}

pub fn random_model(seed: u64) -> Model {
    let n = 2 + (mix(seed) % 6) as usize;
    let domains: Vec<_> = (0..n)
        .map(|i| 2 + (mix(seed ^ (i as u64 + 100)) % 2) as u8)
        .collect();
    let observed: Vec<_> = (0..n)
        .filter(|&v| mix(seed ^ (v as u64 + 300)) % 3 == 0)
        .collect();
    let mut factors = Vec::new();
    for i in 0..(n + 3) {
        let mut scope: Vec<_> = (0..n)
            .filter(|&v| mix(seed ^ (i * 100 + v + 600) as u64) % 3 == 0)
            .take(4)
            .collect();
        if scope.is_empty() {
            scope.push(i % n);
        }
        let count = cardinality(&scope, &domains).unwrap();
        let rows = (0..count)
            .filter(|&r| mix(seed ^ r ^ ((i as u64 + 1) << 32)) % 5 > 1)
            .collect();
        factors.push(Relation { scope, rows });
    }
    for v in 0..n {
        if !factors.iter().any(|f| f.scope.contains(&v)) {
            factors.push(table(&domains, vec![v], |_| true));
        }
    }
    Model {
        name: format!("fuzz_{seed}"),
        domains,
        observed,
        factors,
        context: [0; 4],
    }
}
