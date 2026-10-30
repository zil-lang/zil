// Independent CP11a sequence probe; compiled against frozen old/new rlibs.
use zil_base::{
    ExpansionRole, FileId, Limits, ModuleId, NodeId, SourceMap, SourceMapEntry, SourceMapId,
    SourceTarget, SourceUnit, Span,
};

fn maps() -> Vec<SourceMap> {
    let entry = SourceMapEntry {
        id: SourceMapId(0),
        target: SourceTarget::Node(NodeId(11)),
        origin: Span::new(FileId(7), 0, 2),
        role: ExpansionRole::Source,
        related: vec![],
        parent: None,
    };
    let valid = SourceMap {
        module_id: ModuleId(500),
        entries: vec![entry],
    };
    let mut result = vec![SourceMap::new(ModuleId(500)), valid.clone()];
    let mut bad = valid.clone();
    bad.entries[0].id = SourceMapId(8);
    bad.entries[0].origin.file = FileId(999);
    result.push(bad);
    let mut bad = valid.clone();
    bad.entries[0].parent = Some(SourceMapId(0));
    result.push(bad);
    let mut bad = valid.clone();
    let mut second = bad.entries[0].clone();
    second.id = SourceMapId(1);
    second.origin.file = FileId(999);
    second.related.push(Span::new(FileId(999), 99, 100));
    bad.entries.push(second);
    result.push(bad);
    let mut bad = valid.clone();
    bad.entries[0].origin = Span::new(FileId(7), 1, 2);
    result.push(bad);
    let mut bad = valid.clone();
    bad.entries[0].related = vec![Span::new(FileId(8), 2, 4), Span::new(FileId(7), 2, 3)];
    result.push(bad);
    let mut bad = valid.clone();
    bad.entries[0].target = SourceTarget::Span(Span::new(FileId(7), 1, 4));
    result.push(bad);
    let mut other = valid;
    other.module_id = ModuleId(501);
    result.push(other);
    result
}

fn main() {
    let all = maps();
    let datasets: &[&[u32]] = &[
        &[],
        &[7],
        &[7, 8, 9, 10],
        &[7, 8, 7, 9],
        &[7, 7, 8, 9],
        &[9, 8, 7, 9],
    ];
    for (dataset, ids) in datasets.iter().enumerate() {
        let sources = ids
            .iter()
            .enumerate()
            .map(|(index, &id)| {
                SourceUnit::new(FileId(id), ModuleId(index as u32), "review.zil", "éßz")
            })
            .collect::<Vec<_>>();
        // Warm-up changes whether a complete index, an entry failure, or no
        // index exists before low-budget calls. Every sequence starts fresh.
        for (prefix, (map, nodes, steps)) in [
            (1, usize::MAX, usize::MAX),
            (2, usize::MAX, usize::MAX),
            (0, usize::MAX, 2),
            (0, 0, usize::MAX),
        ]
        .into_iter()
        .enumerate()
        {
            #[cfg(shared_index)]
            let mut shared = zil_base::SourceMapValidator::new(&sources);
            let mut calls = vec![(map, nodes, steps)];
            for nodes in [0, 1, 3, 4, usize::MAX] {
                for steps in [0, 1, 2, 3, 4, 5, 8, 12, usize::MAX] {
                    for map in 0..all.len() {
                        calls.push((map, nodes, steps));
                    }
                }
            }
            // A late failure must not poison another map or a fresh budget.
            calls.extend([(2, 4, 5), (8, 4, 5), (1, 4, 4), (1, 4, 5)]);
            for (call, (map, nodes, steps)) in calls.into_iter().enumerate() {
                let limits = Limits {
                    max_nodes: nodes,
                    max_analysis_steps: steps,
                    ..Limits::default()
                };
                #[cfg(shared_index)]
                let value = shared.validate(&all[map], &limits);
                #[cfg(not(shared_index))]
                let value = all[map].validate(&sources, &limits);
                println!("{dataset}:{prefix}:{call}:{map}:{nodes}:{steps}:{value:?}");
            }
        }
    }
}
