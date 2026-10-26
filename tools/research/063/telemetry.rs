//! Bounded observation of complete persistent root sets at safe boundaries.
//! No telemetry value influences a transfer, worklist, variable order or answer.
use crate::{
    bdd::{Bdd, Context, Id},
    budget::{LIMIT, R, Work},
    report::quote,
    symbolic::Event,
};
use std::collections::BTreeMap;

pub struct Sample {
    context: Context,
    allocated: usize,
    roots: usize,
    reachable: usize,
    candidate_work: usize,
    occupied: usize,
    created: usize,
    collections: usize,
}
pub struct Telemetry {
    enabled: bool,
    pub samples: Vec<Sample>,
    work: Work,
    final_context: Context,
    final_allocated: usize,
    final_work: usize,
    failure: Option<String>,
}
impl Telemetry {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            samples: vec![],
            work: Work::new(LIMIT),
            final_context: Context {
                phase: "extract",
                point: None,
                equation: None,
            },
            final_allocated: 0,
            final_work: 0,
            failure: None,
        }
    }
    #[cfg(test)]
    pub fn disabled() -> Self {
        Self::new(false)
    }
    pub fn extraction_failure(&mut self, error: &str) {
        self.failure = Some(error.to_owned());
    }
    pub fn sample(
        &mut self,
        b: &Bdd,
        events: &[Event],
        states: &[Id],
        relations: &BTreeMap<String, Id>,
    ) -> R<()> {
        if !self.enabled {
            return Ok(());
        }
        let roots = crate::roots::persistent(events, states, relations, &mut self.work)
            .map_err(|e| format!("Diagnostic{e}"))?;
        let reachable = b
            .reachable_nodes(&roots, &mut self.work)
            .map_err(|e| format!("Diagnostic{e}"))?;
        self.samples.push(Sample {
            context: b.context,
            allocated: b.node_count(),
            roots: roots.len(),
            reachable,
            candidate_work: b.work.used,
            occupied: b.occupied_nodes(),
            created: b.created_nodes,
            collections: b.gc_count,
        });
        Ok(())
    }
    pub fn finish(&mut self, b: &Bdd, failure: Option<&String>) {
        self.final_context = b.context;
        self.final_allocated = b.node_count();
        self.final_work = b.work.used;
        self.failure = failure.cloned();
    }
    pub fn json(&self) -> String {
        fn number(value: Option<usize>) -> String {
            value.map_or("null".into(), |v| v.to_string())
        }
        let snapshots = self.samples.iter().map(|s| format!("{{\"phase\":{},\"point\":{},\"allocated\":{},\"persistent_roots\":{},\"persistent_reachable_nonterminals\":{},\"candidate_work\":{},\"occupied\":{},\"created\":{},\"collections\":{}}}", quote(s.context.phase), number(s.context.point), s.allocated, s.roots, s.reachable, s.candidate_work, s.occupied, s.created, s.collections)).collect::<Vec<_>>().join(",");
        format!(
            "{{\"enabled\":{},\"diagnostic_work\":{},\"phase\":{},\"point\":{},\"equation\":{},\"allocated\":{},\"candidate_work\":{},\"failure\":{},\"snapshots\":[{}]}}",
            self.enabled,
            self.work.used,
            quote(self.final_context.phase),
            number(self.final_context.point),
            number(self.final_context.equation),
            self.final_allocated,
            self.final_work,
            self.failure.as_ref().map_or("null".into(), |e| quote(e)),
            snapshots
        )
    }
}
