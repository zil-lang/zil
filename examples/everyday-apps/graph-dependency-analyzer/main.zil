// Everyday App: Graph dependency analyzer (topological order, depth levels,
// transitive dependency closure, shortest path, and exact cycle detection).
// Usage:
//   graph-dependency-analyzer                              (built-in DAG sample)
//   graph-dependency-analyzer <spec_file> [from] [to]      (analyze file spec)
import analysis
import graph
import std.env
import std.fs
import std.io

fn print_node_list(g: &graph.DepGraph, ids: Store<i64>, sep: &[u8]) -> io.Error!void {
    var i: i64 = 0
    while i < store_len(ids) {
        if i > 0 {
            io.print(sep)?
        }
        let id = *store_at(ids, i)
        io.print(graph.node_name(g, id))?
        i += 1
    }
    return ()
}

fn run_spec(raw: &[u8], has_from: bool, from_name: &[u8], has_to: bool, to_name: &[u8]) -> i32 {
    match graph.parse_spec(raw) {
        .err(_) => {
            match io.eprintln("error: invalid graph specification") {
                .ok(_) => {},
                .err(_) => {}
            }
            return 3
        },
        .ok(g) => {
            match analysis.analyze(&g) {
                .err(_) => {
                    match io.eprintln("error: graph analysis failed") {
                        .ok(_) => {},
                        .err(_) => {}
                    }
                    return 3
                },
                .ok(report) => {
                    if report.is_cyclic {
                        match io.println("STATUS CYCLIC") { .ok(_) => {}, .err(_) => { return 5 } }
                        match io.print("CYCLE ") { .ok(_) => {}, .err(_) => { return 5 } }
                        var ci: i64 = 0
                        while ci < analysis.report_cycle_len(&report) {
                            if ci > 0 {
                                match io.print(" -> ") { .ok(_) => {}, .err(_) => { return 5 } }
                            }
                            let cid = analysis.report_cycle_at(&report, ci)
                            match io.print(graph.node_name(&g, cid)) { .ok(_) => {}, .err(_) => { return 5 } }
                            ci += 1
                        }
                        match io.println("") { .ok(_) => {}, .err(_) => { return 5 } }
                        return 1
                    }

                    match io.println("STATUS ACYCLIC") { .ok(_) => {}, .err(_) => { return 5 } }
                    match io.print("NODES ") { .ok(_) => {}, .err(_) => { return 5 } }
                    match io.print_i64(graph.node_count(&g)) { .ok(_) => {}, .err(_) => { return 5 } }
                    match io.print(" EDGES ") { .ok(_) => {}, .err(_) => { return 5 } }
                    match io.println_i64(graph.edge_count(&g)) { .ok(_) => {}, .err(_) => { return 5 } }

                    match io.print("ORDER ") { .ok(_) => {}, .err(_) => { return 5 } }
                    var oi: i64 = 0
                    while oi < analysis.report_order_len(&report) {
                        if oi > 0 {
                            match io.print(",") { .ok(_) => {}, .err(_) => { return 5 } }
                        }
                        let oid = analysis.report_order_at(&report, oi)
                        match io.print(graph.node_name(&g, oid)) { .ok(_) => {}, .err(_) => { return 5 } }
                        oi += 1
                    }
                    match io.println("") { .ok(_) => {}, .err(_) => { return 5 } }

                    match io.print("LEVELS ") { .ok(_) => {}, .err(_) => { return 5 } }
                    var i: i64 = 0
                    while i < analysis.report_order_len(&report) {
                        if i > 0 {
                            match io.print(",") { .ok(_) => {}, .err(_) => { return 5 } }
                        }
                        let id = analysis.report_order_at(&report, i)
                        let lvl = analysis.report_level_at(&report, id)
                        match io.print(graph.node_name(&g, id)) { .ok(_) => {}, .err(_) => { return 5 } }
                        match io.print("=") { .ok(_) => {}, .err(_) => { return 5 } }
                        match io.print_i64(lvl) { .ok(_) => {}, .err(_) => { return 5 } }
                        i += 1
                    }
                    match io.println("") { .ok(_) => {}, .err(_) => { return 5 } }

                    if has_from {
                        match graph.find_node(&g, from_name) {
                            Option.None() => {
                                match io.eprintln("error: unknown query node") {
                                    .ok(_) => {},
                                    .err(_) => {}
                                }
                                return 4
                            },
                            Option.Some(from_id) => {
                                match analysis.transitive_deps(&g, from_id) {
                                    .err(_) => { return 5 },
                                    .ok(closure) => {
                                        match io.print("CLOSURE ") { .ok(_) => {}, .err(_) => { return 5 } }
                                        match io.print(from_name) { .ok(_) => {}, .err(_) => { return 5 } }
                                        match io.print(" ") { .ok(_) => {}, .err(_) => { return 5 } }
                                        if store_len(closure) == 0 {
                                            match io.println("NONE") { .ok(_) => {}, .err(_) => { return 5 } }
                                        } else {
                                            match print_node_list(&g, closure, ",") {
                                                .ok(_) => {},
                                                .err(_) => { return 5 }
                                            }
                                            match io.println("") { .ok(_) => {}, .err(_) => { return 5 } }
                                        }
                                    }
                                }

                                if has_to {
                                    match graph.find_node(&g, to_name) {
                                        Option.None() => {
                                            match io.eprintln("error: unknown target node") {
                                                .ok(_) => {},
                                                .err(_) => {}
                                            }
                                            return 4
                                        },
                                        Option.Some(to_id) => {
                                            match analysis.shortest_path(&g, from_id, to_id) {
                                                .err(_) => { return 5 },
                                                .ok(spath) => {
                                                    match io.print("PATH ") { .ok(_) => {}, .err(_) => { return 5 } }
                                                    match io.print(from_name) { .ok(_) => {}, .err(_) => { return 5 } }
                                                    match io.print(" -> ") { .ok(_) => {}, .err(_) => { return 5 } }
                                                    match io.print(to_name) { .ok(_) => {}, .err(_) => { return 5 } }
                                                    match io.print(" ") { .ok(_) => {}, .err(_) => { return 5 } }
                                                    if store_len(spath) == 0 {
                                                        match io.println("NONE") { .ok(_) => {}, .err(_) => { return 5 } }
                                                    } else {
                                                        match print_node_list(&g, spath, " -> ") {
                                                            .ok(_) => {},
                                                            .err(_) => { return 5 }
                                                        }
                                                        match io.println("") { .ok(_) => {}, .err(_) => { return 5 } }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    return 0
                }
            }
        }
    }
}

fn main() -> i32 {
    let argc = env.arg_count()
    if argc <= 1 {
        let builtin = "# built-in workspace graph\nnode core\ndep net -> core\ndep tls -> core\ndep http -> net\ndep http -> tls\ndep app -> http\n"
        return run_spec(builtin, true, "app", true, "core")
    }

    match env.arg(1) {
        Option.None() => { return 2 },
        Option.Some(spec_path) => {
            match fs.read_to_bytes(bytes_view(spec_path)) {
                .err(_) => {
                    match io.eprintln("error: cannot read graph spec file") {
                        .ok(_) => {},
                        .err(_) => {}
                    }
                    return 2
                },
                .ok(raw) => {
                    if argc == 2 {
                        return run_spec(bytes_view(raw), false, "", false, "")
                    }
                    match env.arg(2) {
                        Option.None() => { return run_spec(bytes_view(raw), false, "", false, "") },
                        Option.Some(from_arg) => {
                            if argc == 3 {
                                return run_spec(bytes_view(raw), true, bytes_view(from_arg), false, "")
                            }
                            match env.arg(3) {
                                Option.None() => {
                                    return run_spec(bytes_view(raw), true, bytes_view(from_arg), false, "")
                                },
                                Option.Some(to_arg) => {
                                    return run_spec(
                                        bytes_view(raw),
                                        true,
                                        bytes_view(from_arg),
                                        true,
                                        bytes_view(to_arg)
                                    )
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
