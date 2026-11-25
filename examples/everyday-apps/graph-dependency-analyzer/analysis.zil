// Topological build order, build depth levels, cycle extraction, transitive closure, and BFS shortest path.
import graph
import std.store

pub package TopoReport {
    pub is_cyclic: bool;
    manages order: Store<i64>;
    manages levels: Store<i64>;
    manages cycle: Store<i64>
}

pub fn report_order_len(rep: &TopoReport) -> i64 {
    return store_len(rep.order)
}

pub fn report_order_at(rep: &TopoReport, idx: i64) -> i64 {
    return *store_at(rep.order, idx)
}

pub fn report_level_at(rep: &TopoReport, node_id: i64) -> i64 {
    return *store_at(rep.levels, node_id)
}

pub fn report_cycle_len(rep: &TopoReport) -> i64 {
    return store_len(rep.cycle)
}

pub fn report_cycle_at(rep: &TopoReport, idx: i64) -> i64 {
    return *store_at(rep.cycle, idx)
}

fn fill_i64(n: i64, init_val: i64) -> graph.GraphError!Store<i64> contract { result(create) } {
    let out: Store<i64> = store_new()
    if !store_reserve(out, n) { return graph.GraphError.Allocation }
    var i: i64 = 0
    while i < n {
        match store_push(out, init_val) {
            .err(_) => { return graph.GraphError.Allocation },
            .ok(_) => {}
        }
        i += 1
    }
    return give out
}

fn sort_ids_by_name(g: &graph.DepGraph, ids: Store<i64>) {
    let n = store_len(ids)
    var a: i64 = 1
    while a < n {
        let key_id = *store_at(ids, a)
        var b = a - 1
        while b >= 0 {
            let cur_id = *store_at(ids, b)
            if graph.bytes_cmp(graph.node_name(g, cur_id), graph.node_name(g, key_id)) > 0 {
                let dst = store_at_mut(ids, b + 1)
                *dst = cur_id
                b -= 1
            } else {
                break
            }
        }
        let place = store_at_mut(ids, b + 1)
        *place = key_id
        a += 1
    }
}

fn extract_cycle_from_dfs(g: &graph.DepGraph) -> graph.GraphError!Store<i64> contract { result(create) } {
    let n = graph.node_count(g)
    let m = graph.edge_count(g)
    // state: 0 = unvisited, 1 = on active stack, 2 = completed
    let state = fill_i64(n, 0)?
    let parent = fill_i64(n, -1)?
    let next_edge = fill_i64(n, 0)?
    let stack: Store<i64> = store_new()
    let cycle: Store<i64> = store_new()

    // Try starting nodes in lexicographic name order for deterministic cycle reporting.
    let seeds: Store<i64> = store_new()
    var s: i64 = 0
    while s < n {
        match store.push_growing(seeds, s) {
            .err(_) => { return graph.GraphError.Allocation },
            .ok(_) => {}
        }
        s += 1
    }
    sort_ids_by_name(g, seeds)

    var seed_pos: i64 = 0
    while seed_pos < n {
        let start = *store_at(seeds, seed_pos)
        if *store_at(state, start) == 0 {
            let st_slot = store_at_mut(state, start)
            *st_slot = 1
            match store.push_growing(stack, start) {
                .err(_) => { return graph.GraphError.Allocation },
                .ok(_) => {}
            }
            while store_len(stack) > 0 {
                let u = *store_at(stack, store_len(stack) - 1)
                var e_idx = *store_at(next_edge, u)
                var advanced = false
                while e_idx < m {
                    let edge = graph.edge_at(g, e_idx)
                    e_idx += 1
                    if edge.dependent == u {
                        let ne_slot = store_at_mut(next_edge, u)
                        *ne_slot = e_idx
                        let v = edge.prerequisite
                        let v_state = *store_at(state, v)
                        if v_state == 1 {
                            // Back-edge u -> v closes a directed cycle!
                            // Reconstruct forward path: v -> ... -> u -> v.
                            let rev: Store<i64> = store_new()
                            match store.push_growing(rev, v) {
                                .err(_) => { return graph.GraphError.Allocation },
                                .ok(_) => {}
                            }
                            var cur = u
                            while cur != v && cur >= 0 {
                                match store.push_growing(rev, cur) {
                                    .err(_) => { return graph.GraphError.Allocation },
                                    .ok(_) => {}
                                }
                                cur = *store_at(parent, cur)
                            }
                            match store.push_growing(rev, v) {
                                .err(_) => { return graph.GraphError.Allocation },
                                .ok(_) => {}
                            }
                            while store_len(rev) > 0 {
                                match store_pop(rev) {
                                    .err(_) => { return graph.GraphError.Allocation },
                                    .ok(node_id) => {
                                        match store.push_growing(cycle, node_id) {
                                            .err(_) => { return graph.GraphError.Allocation },
                                            .ok(_) => {}
                                        }
                                    }
                                }
                            }
                            return give cycle
                        } else if v_state == 0 {
                            let p_slot = store_at_mut(parent, v)
                            *p_slot = u
                            let vs_slot = store_at_mut(state, v)
                            *vs_slot = 1
                            match store.push_growing(stack, v) {
                                .err(_) => { return graph.GraphError.Allocation },
                                .ok(_) => {}
                            }
                            advanced = true
                            break
                        }
                    }
                }
                if !advanced {
                    let ne_done = store_at_mut(next_edge, u)
                    *ne_done = m
                    let done_slot = store_at_mut(state, u)
                    *done_slot = 2
                    match store_pop(stack) {
                        .ok(_) => {},
                        .err(_) => { return graph.GraphError.Allocation }
                    }
                }
            }
        }
        seed_pos += 1
    }
    return give cycle
}

// Kahn's topological sort with deterministic lexicographic tie-breaking and depth levels.
pub fn analyze(g: &graph.DepGraph) -> graph.GraphError!TopoReport {
    let n = graph.node_count(g)
    let m = graph.edge_count(g)
    let in_degree = fill_i64(n, 0)?
    let emitted = fill_i64(n, 0)?
    let levels = fill_i64(n, 0)?

    var e: i64 = 0
    while e < m {
        let edge = graph.edge_at(g, e)
        let deg_slot = store_at_mut(in_degree, edge.dependent)
        *deg_slot = *deg_slot + 1
        e += 1
    }

    let order: Store<i64> = store_new()
    loop {
        var best: i64 = -1
        var i: i64 = 0
        while i < n {
            if *store_at(emitted, i) == 0 && *store_at(in_degree, i) == 0 {
                if best < 0 {
                    best = i
                } else if graph.bytes_cmp(graph.node_name(g, i), graph.node_name(g, best)) < 0 {
                    best = i
                }
            }
            i += 1
        }
        if best < 0 { break }

        let em_slot = store_at_mut(emitted, best)
        *em_slot = 1
        match store.push_growing(order, best) {
            .err(_) => { return graph.GraphError.Allocation },
            .ok(_) => {}
        }

        let best_level = *store_at(levels, best)
        var ei: i64 = 0
        while ei < m {
            let edge = graph.edge_at(g, ei)
            if edge.prerequisite == best {
                let d_slot = store_at_mut(in_degree, edge.dependent)
                *d_slot = *d_slot - 1
                let lvl_slot = store_at_mut(levels, edge.dependent)
                if best_level + 1 > *lvl_slot {
                    *lvl_slot = best_level + 1
                }
            }
            ei += 1
        }
    }

    if store_len(order) < n {
        let cycle = extract_cycle_from_dfs(g)?
        return TopoReport {
            is_cyclic: true,
            order: give order,
            levels: give levels,
            cycle: give cycle
        }
    }

    let empty_cycle: Store<i64> = store_new()
    return TopoReport {
        is_cyclic: false,
        order: give order,
        levels: give levels,
        cycle: give empty_cycle
    }
}

// Computes all transitive dependencies reachable from `start` via generic `graph.Queue<i64>`.
pub fn transitive_deps(g: &graph.DepGraph, start: i64) -> graph.GraphError!Store<i64> contract { result(create) } {
    let n = graph.node_count(g)
    let m = graph.edge_count(g)
    if start < 0 || start >= n { return graph.GraphError.InvalidNode }

    let visited = fill_i64(n, 0)?
    let v_start = store_at_mut(visited, start)
    *v_start = 1

    var q = graph.queue_new::<i64>()
    match graph.queue_push::<i64>(&mut q, start) {
        .err(_) => { return graph.GraphError.Allocation },
        .ok(_) => {}
    }

    let deps: Store<i64> = store_new()
    while !graph.queue_is_empty::<i64>(&q) {
        match graph.queue_pop::<i64>(&mut q) {
            .err(_) => { break },
            .ok(u) => {
                var ei: i64 = 0
                while ei < m {
                    let edge = graph.edge_at(g, ei)
                    if edge.dependent == u {
                        let v = edge.prerequisite
                        if *store_at(visited, v) == 0 {
                            let v_slot = store_at_mut(visited, v)
                            *v_slot = 1
                            match store.push_growing(deps, v) {
                                .err(_) => { return graph.GraphError.Allocation },
                                .ok(_) => {}
                            }
                            match graph.queue_push::<i64>(&mut q, v) {
                                .err(_) => { return graph.GraphError.Allocation },
                                .ok(_) => {}
                            }
                        }
                    }
                    ei += 1
                }
            }
        }
    }

    sort_ids_by_name(g, deps)
    return give deps
}

// Computes the shortest dependency path `from -> ... -> to` using BFS on `graph.Queue<i64>`.
pub fn shortest_path(g: &graph.DepGraph, from: i64, to: i64) -> graph.GraphError!Store<i64> contract { result(create) } {
    let n = graph.node_count(g)
    let m = graph.edge_count(g)
    if from < 0 || from >= n || to < 0 || to >= n {
        return graph.GraphError.InvalidNode
    }

    let path: Store<i64> = store_new()
    if from == to {
        match store.push_growing(path, from) {
            .err(_) => { return graph.GraphError.Allocation },
            .ok(_) => { return give path }
        }
    }

    let visited = fill_i64(n, 0)?
    let prev = fill_i64(n, -1)?
    let v_from = store_at_mut(visited, from)
    *v_from = 1

    var q = graph.queue_new::<i64>()
    match graph.queue_push::<i64>(&mut q, from) {
        .err(_) => { return graph.GraphError.Allocation },
        .ok(_) => {}
    }

    var found = false
    while !graph.queue_is_empty::<i64>(&q) && !found {
        match graph.queue_pop::<i64>(&mut q) {
            .err(_) => { break },
            .ok(u) => {
                // Collect outgoing neighbors in lexicographic name order for deterministic BFS paths.
                let neighbors: Store<i64> = store_new()
                var ei: i64 = 0
                while ei < m {
                    let edge = graph.edge_at(g, ei)
                    if edge.dependent == u {
                        match store.push_growing(neighbors, edge.prerequisite) {
                            .err(_) => { return graph.GraphError.Allocation },
                            .ok(_) => {}
                        }
                    }
                    ei += 1
                }
                sort_ids_by_name(g, neighbors)
                var ni: i64 = 0
                while ni < store_len(neighbors) {
                    let v = *store_at(neighbors, ni)
                    if *store_at(visited, v) == 0 {
                        let vs = store_at_mut(visited, v)
                        *vs = 1
                        let ps = store_at_mut(prev, v)
                        *ps = u
                        if v == to {
                            found = true
                            break
                        }
                        match graph.queue_push::<i64>(&mut q, v) {
                            .err(_) => { return graph.GraphError.Allocation },
                            .ok(_) => {}
                        }
                    }
                    ni += 1
                }
            }
        }
    }

    if !found {
        return give path
    }

    let rev: Store<i64> = store_new()
    var cur = to
    while cur >= 0 {
        match store.push_growing(rev, cur) {
            .err(_) => { return graph.GraphError.Allocation },
            .ok(_) => {}
        }
        cur = *store_at(prev, cur)
    }
    while store_len(rev) > 0 {
        match store_pop(rev) {
            .err(_) => { return graph.GraphError.Allocation },
            .ok(node_id) => {
                match store.push_growing(path, node_id) {
                    .err(_) => { return graph.GraphError.Allocation },
                    .ok(_) => {}
                }
            }
        }
    }
    return give path
}
