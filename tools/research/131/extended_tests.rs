#![forbid(unsafe_code)]
#[path = "effect_graph.rs"]
mod effect_graph;
use effect_graph::{Edit, Graph};
use std::collections::BTreeSet;

#[test]
fn one_dirty_vertex_can_still_have_a_linear_boundary() {
    for n in [128, 256, 512, 1024] {
        let edges: Vec<_> = (0..n - 1).map(|v| (v, n - 1)).collect();
        let seeds: Vec<_> = (0..n - 1).collect();
        let mut g = Graph::new(n, &edges, &seeds).unwrap();
        let stats = g.apply(&[Edit::Edge(0, n - 1, false)], usize::MAX).unwrap();
        assert_eq!(stats.dirty, 1);
        assert_eq!(stats.incoming_scans, n - 1);
        assert_eq!(stats.work, n + 7);
        assert_eq!(stats.changed, 0);
        assert!(g.active()[n - 1]);
        println!("boundary n={n} dirty={} incoming={} work={}",
                 stats.dirty, stats.incoming_scans, stats.work);
    }
}

#[test]
fn incremental_edits_match_all_real_source_receiver_observations() {
    let mut g = Graph::new(8, &[], &[]).unwrap();
    let mut old_edges = BTreeSet::new();
    let mut old_seeds = BTreeSet::new();
    let mut cases = 0;
    for row in include_str!("source-cases.tsv").lines() {
        let fields: Vec<_> = row.split('\t').collect();
        assert_eq!(fields.len(), 5);
        assert_eq!(fields[1], "8");
        let edges: BTreeSet<(usize, usize)> = fields[2].split(';').filter(|x| !x.is_empty())
            .map(|x| { let (a, b) = x.split_once(',').unwrap(); (a.parse().unwrap(), b.parse().unwrap()) }).collect();
        let seeds: BTreeSet<usize> = fields[3].split(',').filter(|x| !x.is_empty())
            .map(|x| x.parse().unwrap()).collect();
        let expected: Vec<_> = fields[4].bytes().map(|c| { assert!(c == b'0' || c == b'1'); c == b'1' }).collect();
        assert_eq!(expected.len(), 8);
        let mut edits = Vec::new();
        for &(a, b) in old_edges.difference(&edges) { edits.push(Edit::Edge(a, b, false)); }
        for &(a, b) in edges.difference(&old_edges) { edits.push(Edit::Edge(a, b, true)); }
        for &v in old_seeds.difference(&seeds) { edits.push(Edit::Seed(v, false)); }
        for &v in seeds.difference(&old_seeds) { edits.push(Edit::Seed(v, true)); }
        g.apply(&edits, 10_000).unwrap();
        assert_eq!(g.active(), expected, "source family {}", fields[0]);
        old_edges = edges;
        old_seeds = seeds;
        cases += 1;
    }
    assert_eq!(cases, 20);
    println!("real_source_families={cases} receiver_observations={}", cases * 8);
}
