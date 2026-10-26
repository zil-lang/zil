//! Static ordering heuristics alter only ROBDD rank, never schema coordinates.
use crate::{
    budget::{R, Work},
    schema::Schema,
};

pub fn make(schema: &Schema, name: &str, work: &mut Work) -> R<Vec<usize>> {
    work.charge(schema.bits.saturating_add(schema.fields.len()))?;
    if name == "canonical" {
        return Ok((0..schema.bits).collect());
    }
    if !matches!(name, "owner" | "token") {
        return Err("Unsupported: BDD variable-order heuristic".into());
    }
    let mut keys = Vec::new();
    for (key, &(at, width)) in &schema.fields {
        work.charge(key.len().saturating_mul(4).saturating_add(width))?;
        let parts = key.split(':').collect::<Vec<_>>();
        // Keep a carrier's state, payload/tag, incidence and order coordinates
        // near each other, or group incidence/retirement by token instead.
        let group = if name == "token" && parts[0] == "i" {
            parts[2]
        } else {
            parts[1]
        };
        let family = match parts[0] {
            "s" => "0",
            "f" => "1",
            "b" | "e" | "n" | "v" => "2",
            "i" => "3",
            "d" => "4",
            "o" => "5",
            "r" => "6",
            _ => "7",
        };
        keys.push((format!("{group}:{family}:{key}"), at, width));
    }
    work.charge(
        keys.len()
            .saturating_mul(keys.len().max(1).ilog2() as usize + 1),
    )?;
    keys.sort();
    Ok(keys
        .into_iter()
        .flat_map(|(_, at, width)| at..at + width)
        .collect())
}
