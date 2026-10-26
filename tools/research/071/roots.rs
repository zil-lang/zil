//! The complete persistent NodeId roots at an event/worklist safe boundary.
//! Plans contain only references to Event transfer factors; their other fields
//! are coordinate sets, not BDD roots. Recursive apply/exists temporaries are
//! never collected: callers may use this only after those calls returned.
use crate::{
    bdd::Id,
    budget::{R, Work},
    symbolic::Event,
};
use std::collections::BTreeMap;
pub fn persistent(
    events: &[Event],
    states: &[Id],
    relations: &BTreeMap<String, Id>,
    work: &mut Work,
) -> R<Vec<Id>> {
    work.charge(
        events
            .len()
            .saturating_add(states.len())
            .saturating_add(relations.len())
            .saturating_mul(2),
    )?;
    let mut roots = Vec::new();
    for e in events {
        for t in &e.transfers {
            work.charge(
                t.factors
                    .len()
                    .saturating_add(t.faults.len())
                    .saturating_add(t.aborts.len())
                    .saturating_mul(2),
            )?;
            work.charge(1)?;
            roots.push(t.invariant_violation);
            work.charge(t.assignments.len().saturating_mul(3))?;
            roots.extend(
                t.assignments
                    .iter()
                    .flat_map(|(_, rhs, free)| [*rhs, *free]),
            );
            roots.extend(t.factors.iter().map(|(_, id)| *id));
            roots.extend(t.faults.iter().chain(&t.aborts).map(|(_, id)| *id));
        }
        work.charge(
            e.next
                .len()
                .saturating_add(e.terminal.len())
                .saturating_mul(2),
        )?;
        roots.extend(e.next.iter().map(|(_, id)| *id));
        roots.extend(e.terminal.iter().map(|(_, id)| *id));
    }
    roots.extend_from_slice(states);
    roots.extend(relations.values().copied());
    Ok(roots)
}
