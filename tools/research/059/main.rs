#![forbid(unsafe_code)]
mod bdd;
mod budget;
mod extract;
mod oracle;
mod report;
mod schema;
mod symbolic;
#[cfg(test)]
mod tests;

use budget::R;
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
                if !s.bdd.contains(root, row)? {
                    return Err(format!("Mismatch: missing joint row at {key}"));
                }
            }
        }
    }
    Ok(s.status)
}
fn run() -> R<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let path = args
        .first()
        .ok_or("usage: cut059 SOURCE METHOD [RELATION_DIRECTORY]")?;
    let method = args.get(1).map_or("wire", String::as_str);
    let output = args.get(2).map(std::path::Path::new);
    println!(
        "{}",
        report::execute(std::path::Path::new(path), method, output)?
    );
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1)
    }
}
