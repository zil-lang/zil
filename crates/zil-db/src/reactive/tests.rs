use super::*;
use std::cell::{Cell, RefCell};

fn check_bonds<K: Ord + Clone + fmt::Debug, V: Clone + Eq>(graph: &ReactiveGraph<K, V>) {
    let mut edges = 0;
    for (key, node) in &graph.nodes {
        for dependency in &node.dependencies {
            assert!(graph.nodes[dependency].receivers.contains(key));
            assert!(graph.nodes[dependency].height < node.height);
            edges += 1;
        }
        for receiver in &node.receivers {
            assert!(graph.nodes[receiver].dependencies.contains(key));
        }
        assert_eq!(
            graph.queue.contains(&(node.height, key.clone())),
            node.dirty
        );
    }
    assert_eq!(graph.edges, edges);
}

#[test]
fn batch_diamond_deduplicates_and_equal_values_stop_direct_notifications() {
    let mut graph = ReactiveGraph::<u8, i32>::new();
    graph.set_inputs([(0, 1), (1, 2)]).unwrap();
    for key in 2..=5 {
        graph.ensure_query(key).unwrap();
    }
    let observed = RefCell::new(Vec::new());
    let evaluate = |key: &u8, read: &mut dyn FnMut(&u8) -> Result<i32, GraphError>| {
        observed.borrow_mut().push(*key);
        match key {
            2 => Ok((read(&0)? + read(&1)?) % 2),
            3 => Ok(read(&2)? + 1),
            4 => Ok(read(&2)? + 2),
            5 => Ok(read(&3)? + read(&4)?),
            _ => unreachable!(),
        }
    };
    graph.flush(&evaluate).unwrap();
    assert_eq!(graph.get_current(&5), Some(&5));
    check_bonds(&graph);
    let bond_address = graph.nodes[&2].dependencies.get(&0).unwrap() as *const u8;
    graph.take_stats();
    observed.borrow_mut().clear();
    graph.set_inputs([(0, 4), (1, 5)]).unwrap();
    assert_eq!(graph.get_current(&5), None);
    graph.flush(&evaluate).unwrap();
    assert_eq!(*observed.borrow(), vec![2]);
    let stats = graph.take_stats();
    assert_eq!(stats.executed, 1);
    assert_eq!(stats.notifications, 2);
    assert_eq!(stats.enqueues, 1);
    assert_eq!(stats.deduplicated, 1);
    assert_eq!(stats.rank_visits, 0);
    assert_eq!(stats.reused_bonds, 2);
    assert_eq!(
        graph.nodes[&2].dependencies.get(&0).unwrap() as *const u8,
        bond_address
    );
    assert_eq!(stats.new_bonds, 0);
    assert_eq!(graph.get_current(&5), Some(&5));
    graph.set_inputs([(0, 5), (1, 5)]).unwrap();
    graph.flush(&evaluate).unwrap();
    assert_eq!(graph.get_current(&5), Some(&3));
    assert_eq!(graph.stats().executed, 4);
    check_bonds(&graph);
}

#[test]
fn dynamic_reads_trim_both_sides_and_lower_actual_heights() {
    let mut graph = ReactiveGraph::<u8, i32>::new();
    graph.set_inputs([(0, 1), (1, 10), (2, 20)]).unwrap();
    for key in 3..=5 {
        graph.ensure_query(key).unwrap();
    }
    let evaluate = |key: &u8, read: &mut dyn FnMut(&u8) -> Result<i32, GraphError>| match key {
        3 => read(&1),
        4 => {
            if read(&0)? == 1 {
                read(&3)
            } else {
                read(&2)
            }
        }
        5 => Ok(read(&4)? + 1),
        _ => unreachable!(),
    };
    graph.flush(&evaluate).unwrap();
    assert_eq!(graph.nodes[&5].height, 3);
    graph.take_stats();
    graph.set_inputs([(0, 0)]).unwrap();
    graph.flush(&evaluate).unwrap();
    assert_eq!(graph.get_current(&5), Some(&21));
    assert_eq!(graph.nodes[&5].height, 2);
    assert!(!graph.nodes[&3].receivers.contains(&4));
    assert!(!graph.nodes[&4].dependencies.contains(&3));
    assert_eq!(graph.stats().removed_bonds, 1);
    graph.take_stats();
    graph.set_inputs([(1, 99)]).unwrap();
    graph.flush(&evaluate).unwrap();
    assert_eq!(graph.stats().executed, 1);
    assert_eq!(graph.get_current(&5), Some(&21));
    check_bonds(&graph);
}

#[test]
fn a_batch_can_reverse_old_edges_without_a_false_cycle() {
    let mut graph = ReactiveGraph::<u8, i32>::new();
    graph.set_inputs([(0, 0)]).unwrap();
    graph.ensure_query(1).unwrap();
    graph.ensure_query(2).unwrap();
    let evaluate = |key: &u8, read: &mut dyn FnMut(&u8) -> Result<i32, GraphError>| {
        let mode = read(&0)?;
        match (*key, mode) {
            (1, 0) | (2, 1) => Ok(10),
            (1, _) => Ok(read(&2)? + 1),
            (2, _) => Ok(read(&1)? + 1),
            _ => unreachable!(),
        }
    };
    graph.flush(&evaluate).unwrap();
    assert_eq!(graph.get_current(&2), Some(&11));
    graph.set_inputs([(0, 1)]).unwrap();
    graph.flush(&evaluate).unwrap();
    assert_eq!(graph.get_current(&1), Some(&11));
    assert_eq!(graph.get_current(&2), Some(&10));
    assert_eq!(graph.nodes[&1].height, 2);
    assert_eq!(graph.nodes[&2].height, 1);
    check_bonds(&graph);
}

#[test]
fn dynamic_read_pulls_a_stale_transitive_dependency_before_observing_it() {
    let mut graph = ReactiveGraph::<u8, i32>::new();
    graph.set_inputs([(0, 0), (1, 10)]).unwrap();
    for key in 2..=4 {
        graph.ensure_query(key).unwrap();
    }
    let evaluate = |key: &u8, read: &mut dyn FnMut(&u8) -> Result<i32, GraphError>| match key {
        2 => {
            if read(&0)? == 0 {
                Ok(0)
            } else {
                read(&4)
            }
        }
        3 => Ok(read(&1)? + 1),
        4 => Ok(read(&3)? + 1),
        _ => unreachable!(),
    };
    graph.flush(&evaluate).unwrap();
    graph.set_inputs([(0, 1), (1, 20)]).unwrap();
    graph.flush(&evaluate).unwrap();
    assert_eq!(graph.get_current(&2), Some(&22));
    assert_eq!(graph.nodes[&2].height, 3);
    check_bonds(&graph);
}

#[test]
fn failures_and_cancellation_never_expose_prior_success_then_clear_recovers() {
    let mut graph = ReactiveGraph::<u8, i32>::new();
    let cancelled = Cell::new(false);
    let evaluate = |_: &u8, read: &mut dyn FnMut(&u8) -> Result<i32, GraphError>| {
        if cancelled.get() {
            return Err(GraphError::Evaluation("cancelled".into()));
        }
        read(&0)
    };
    graph.set_inputs([(0, 10)]).unwrap();
    graph.ensure_query(1).unwrap();
    graph.flush(&evaluate).unwrap();
    cancelled.set(true);
    graph.set_inputs([(0, 20)]).unwrap();
    assert_eq!(
        graph.flush(&evaluate),
        Err(GraphError::Evaluation("cancelled".into()))
    );
    assert_eq!(graph.get_current(&0), None);
    assert_eq!(graph.get_current(&1), None);
    assert_eq!(graph.read(&1, &evaluate), Err(GraphError::Poisoned));
    assert_eq!(graph.ensure_query(2), Err(GraphError::Poisoned));
    assert_eq!(graph.set_inputs([(0, 30)]), Err(GraphError::Poisoned));
    graph.clear();
    cancelled.set(false);
    graph.set_inputs([(0, 30)]).unwrap();
    graph.ensure_query(1).unwrap();
    assert_eq!(graph.read(&1, &evaluate), Ok(30));
    check_bonds(&graph);
}

#[test]
fn true_cycles_fail_without_a_numeric_rank_threshold() {
    let mut graph = ReactiveGraph::<u8, i32>::new();
    graph.ensure_query(0).unwrap();
    graph.ensure_query(1).unwrap();
    let evaluate =
        |key: &u8, read: &mut dyn FnMut(&u8) -> Result<i32, GraphError>| read(&(1 - key));
    assert_eq!(graph.flush(&evaluate), Err(GraphError::Cycle));
    assert_eq!(graph.get_current(&0), None);
}

#[test]
fn a_deep_chain_has_real_heights_and_linear_local_change_work() {
    const DEPTH: u32 = 1500;
    let mut graph = ReactiveGraph::<u32, u32>::new();
    graph.set_inputs([(0, 1)]).unwrap();
    for owner in 1..=DEPTH {
        graph.ensure_query(owner).unwrap();
    }
    let evaluate = |key: &u32, read: &mut dyn FnMut(&u32) -> Result<u32, GraphError>| {
        Ok(read(&(key - 1))? + 1)
    };
    graph.flush(&evaluate).unwrap();
    assert_eq!(graph.nodes[&DEPTH].height, u64::from(DEPTH));
    graph.take_stats();
    graph.set_inputs([(0, 2)]).unwrap();
    graph.flush(&evaluate).unwrap();
    assert_eq!(graph.get_current(&DEPTH), Some(&(DEPTH + 2)));
    assert_eq!(graph.stats().executed, u64::from(DEPTH));
    assert_eq!(graph.stats().notifications, u64::from(DEPTH));
    assert_eq!(graph.stats().rank_visits, 0);
    check_bonds(&graph);
}

#[test]
fn inventory_removal_severs_reverse_bonds_and_accounts_for_the_scan() {
    let mut graph = ReactiveGraph::<u8, i32>::new();
    let switched = Cell::new(false);
    graph.set_inputs([(0, 1), (1, 2)]).unwrap();
    graph.ensure_query(2).unwrap();
    graph.ensure_query(3).unwrap();
    let evaluate = |key: &u8, read: &mut dyn FnMut(&u8) -> Result<i32, GraphError>| {
        if *key == 2 {
            read(&0)
        } else if switched.get() {
            read(&1)
        } else {
            read(&2)
        }
    };
    graph.flush(&evaluate).unwrap();
    graph.take_stats();
    switched.set(true);
    graph.retain(&BTreeSet::from([1, 3])).unwrap();
    graph.flush(&evaluate).unwrap();
    assert_eq!(graph.get_current(&3), Some(&2));
    assert_eq!(graph.stats().inventory_visits, 6);
    assert_eq!(graph.stats().retained_nodes, 2);
    assert_eq!(graph.stats().retained_edges, 1);
    check_bonds(&graph);
}

#[test]
fn duplicate_batch_values_and_kind_conflict_are_atomic() {
    let mut graph = ReactiveGraph::<u8, i32>::new();
    graph.set_inputs([(0, 1), (0, 2)]).unwrap();
    graph.ensure_query(1).unwrap();
    assert_eq!(
        graph.set_inputs([(0, 5), (1, 9)]),
        Err(GraphError::InputQueryConflict)
    );
    assert_eq!(graph.nodes[&0].value, Some(2));
    assert_eq!(graph.ensure_query(0), Err(GraphError::InputQueryConflict));
}

#[test]
fn adding_an_edge_to_a_clean_receiver_still_detects_a_real_cycle() {
    let mut graph = ReactiveGraph::<u8, i32>::new();
    graph.set_inputs([(0, 0)]).unwrap();
    graph.ensure_query(1).unwrap();
    graph.ensure_query(2).unwrap();
    let evaluate = |key: &u8, read: &mut dyn FnMut(&u8) -> Result<i32, GraphError>| {
        if *key == 1 {
            if read(&0)? == 0 { Ok(10) } else { read(&2) }
        } else {
            read(&1)
        }
    };
    graph.flush(&evaluate).unwrap();
    graph.set_inputs([(0, 1)]).unwrap();
    assert!(!graph.nodes[&2].dirty);
    assert_eq!(graph.flush(&evaluate), Err(GraphError::Cycle));
    assert_eq!(graph.get_current(&1), None);
    assert_eq!(graph.get_current(&2), None);
}

#[test]
fn nested_new_reads_cannot_close_a_cycle_through_an_unreturned_read() {
    let mut graph = ReactiveGraph::<u8, i32>::new();
    graph.set_inputs([(0, 0)]).unwrap();
    for key in 1..=4 {
        graph.ensure_query(key).unwrap();
    }
    let evaluate = |key: &u8, read: &mut dyn FnMut(&u8) -> Result<i32, GraphError>| match key {
        1 => {
            if read(&0)? == 0 {
                Ok(10)
            } else {
                read(&2)
            }
        }
        2 => {
            if read(&0)? == 0 {
                read(&4)
            } else {
                read(&3)
            }
        }
        3 => read(&1),
        4 => Ok(20),
        _ => unreachable!(),
    };
    graph.flush(&evaluate).unwrap();
    graph.set_inputs([(0, 1)]).unwrap();
    assert_eq!(graph.flush(&evaluate), Err(GraphError::Cycle));
    assert_eq!(graph.get_current(&1), None);
}

#[test]
fn reused_bond_revalidates_after_its_dependency_changes_height() {
    let mut graph = ReactiveGraph::<u8, i32>::new();
    graph.set_inputs([(0, 0)]).unwrap();
    for key in 1..=4 {
        graph.ensure_query(key).unwrap();
    }
    let evaluate = |key: &u8, read: &mut dyn FnMut(&u8) -> Result<i32, GraphError>| match key {
        1 => Ok(10),
        2 => {
            if read(&0)? == 0 {
                read(&1)
            } else {
                read(&4)
            }
        }
        3 => read(&2),
        4 => read(&1),
        _ => unreachable!(),
    };
    graph.flush(&evaluate).unwrap();
    let bond_address = graph.nodes[&3].dependencies.get(&2).unwrap() as *const u8;
    graph.take_stats();
    graph.set_inputs([(0, 1)]).unwrap();
    graph.flush(&evaluate).unwrap();
    assert_eq!(graph.nodes[&2].height, 3);
    assert_eq!(graph.nodes[&3].height, 4);
    assert_eq!(
        graph.nodes[&3].dependencies.get(&2).unwrap() as *const u8,
        bond_address
    );
    assert_eq!(graph.get_current(&3), Some(&10));
    check_bonds(&graph);
}
