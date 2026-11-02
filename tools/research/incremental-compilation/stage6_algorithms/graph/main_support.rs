#![forbid(unsafe_code)]
// Shared historical modules include other query engines used by cut-stage6.
#![allow(dead_code)]
#[path = "../../../063/bdd.rs"]
mod bdd;
#[path = "../../../063/budget.rs"]
mod budget;
#[path = "../../../071/direct.rs"]
mod direct;
#[path = "../../../071/domains.rs"]
mod domains;
#[path = "../../../071/encoding.rs"]
mod encoding;
#[path = "extract_current.rs"]
mod extract;
#[cfg(test)]
#[path = "../../../063/tests.rs"]
mod historical_tests;
#[path = "../../../076/intern.rs"]
mod intern;
#[cfg(test)]
#[path = "../../../076/intern_tests.rs"]
mod intern_tests;
#[path = "oracle_current.rs"]
mod oracle;
#[path = "../../../063/ordering.rs"]
mod ordering;
#[path = "report_current.rs"]
mod report;
#[path = "../../../071/roots.rs"]
mod roots;
#[path = "../../../063/schema.rs"]
mod schema;
#[path = "symbolic_current.rs"]
mod symbolic;
#[path = "../../../063/telemetry.rs"]
mod telemetry;
#[cfg(test)]
#[path = "../../../076/tests.rs"]
mod tests;

use budget::R;
#[path = "source_current.rs"]
mod source;
#[cfg(test)]
fn typed(name: &str, text: &str) -> R<zil_ir::TypedModule> {
    source::text(name, text)
}
fn compare(s: &mut symbolic::Solution, o: &oracle::Reference) -> R<schema::QueryStatus> {
    s.bdd.work.charge(
        s.layout
            .schema
            .fields
            .len()
            .saturating_add(o.schema.fields.len()),
    )?;
    s.bdd.work.charge(
        s.layout
            .schema
            .fields
            .keys()
            .chain(o.schema.fields.keys())
            .map(|k| k.len())
            .sum(),
    )?;
    if s.layout.schema != o.schema {
        return Err("Mismatch: independently extracted schema".into());
    }
    s.bdd
        .work
        .charge(s.layout.labels.len().saturating_add(o.labels.len()))?;
    s.bdd.work.charge(
        s.layout
            .labels
            .iter()
            .chain(o.labels.iter())
            .map(|k| k.len())
            .sum(),
    )?;
    if s.layout.labels != o.labels {
        return Err("Mismatch: raw event labels/CFG".into());
    }
    if s.status != o.status {
        return Err("Mismatch: query support status".into());
    }
    if let Some(relations) = &s.explicit {
        s.bdd.work.charge(
            relations
                .keys()
                .chain(o.relations.keys())
                .map(|k| k.len() + 2)
                .sum(),
        )?;
        let keys = relations
            .keys()
            .chain(o.relations.keys())
            .collect::<std::collections::BTreeSet<_>>();
        for key in keys {
            let actual = relations.get(key);
            let expected = o.relations.get(key);
            if actual.map_or(0, |r| r.len()) != expected.map_or(0, |r| r.len()) {
                return Err(format!("Mismatch: direct relation count at {key}"));
            }
            if let Some(expected) = expected {
                for row in expected {
                    let encoded = s.encoding.encode(row, &mut s.bdd.work)?;
                    let comparisons = (actual.map_or(0, |r| r.len()) + 1).ilog2() as usize + 1;
                    s.bdd.work.charge(comparisons * (encoded.len() + 1))?;
                    if !actual.is_some_and(|rows| rows.contains(&encoded)) {
                        return Err(format!("Mismatch: direct missing joint row at {key}"));
                    }
                }
            }
        }
        return Ok(s.status);
    }
    s.bdd.work.charge(
        s.relations
            .len()
            .saturating_add(o.relations.len())
            .saturating_mul(2),
    )?;
    s.bdd.work.charge(
        s.relations
            .keys()
            .chain(o.relations.keys())
            .map(|k| k.len())
            .sum(),
    )?;
    let keys = s
        .relations
        .keys()
        .chain(o.relations.keys())
        .collect::<std::collections::BTreeSet<_>>();
    for key in keys {
        let root = *s.relations.get(key).unwrap_or(&0);
        let rows = o.relations.get(key);
        let expected = rows.map_or(0, |r| r.len());
        let actual = s.bdd.count(root)?;
        if actual != expected as u128 {
            return Err(format!("Mismatch: {key} count {actual} != {expected}"));
        }
        if let Some(rows) = rows {
            for row in rows {
                let encoded = s.encoding.encode(row, &mut s.bdd.work)?;
                if !s.bdd.contains(root, &encoded)? {
                    return Err(format!("Mismatch: missing joint row at {key}"));
                }
            }
        }
    }
    Ok(s.status)
}
mod factored;
fn run() -> R<()> {
    factored::run()
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1)
    }
}
