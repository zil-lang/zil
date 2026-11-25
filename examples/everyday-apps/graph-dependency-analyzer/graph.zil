// Directed dependency graph storage, spec parser, and generic two-stack FIFO Queue<T>.
import std.store
import std.string
import std.string_map

pub enum GraphError {
    ParseError,
    Allocation,
    InvalidNode,
    EmptySpec
}

// Generic amortized-O(1) FIFO queue backed by two owning Stores.
pub package Queue<T: StoreElement> {
    manages inbox: Store<T>;
    manages outbox: Store<T>
}

pub fn queue_new<T: StoreElement>() -> Queue<T> {
    return Queue::<T> {
        inbox: store_new(),
        outbox: store_new()
    }
}

pub fn queue_is_empty<T: StoreElement>(q: &Queue<T>) -> bool {
    return store_len(q.inbox) == 0 && store_len(q.outbox) == 0
}

pub fn queue_push<T: StoreElement>(q: &mut Queue<T>, item: T) -> T!void {
    return store.push_growing(q.inbox, item)
}

pub fn queue_pop<T: StoreElement>(q: &mut Queue<T>) -> void!T {
    if store_len(q.outbox) == 0 {
        while store_len(q.inbox) > 0 {
            let moved = store_pop(q.inbox)?
            match store.push_growing(q.outbox, moved) {
                .ok(_) => {},
                .err(_) => { return () }
            }
        }
    }
    return store_pop(q.outbox)
}

// Edge `dependent -> prerequisite` means `dependent` requires `prerequisite` first.
pub struct DepEdge {
    pub dependent: i64;
    pub prerequisite: i64
}

pub package DepGraph {
    manages names: Store<string.String>;
    manages edges: Store<DepEdge>;
    name_to_id: string_map.StringMap
}

pub fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() { return false }
    var i: i64 = 0
    while i < a.len() {
        if a[i] != b[i] { return false }
        i += 1
    }
    return true
}

pub fn bytes_cmp(a: &[u8], b: &[u8]) -> i32 {
    var i: i64 = 0
    while i < a.len() && i < b.len() {
        if a[i] < b[i] { return -1 }
        if a[i] > b[i] { return 1 }
        i += 1
    }
    if a.len() < b.len() { return -1 }
    if a.len() > b.len() { return 1 }
    return 0
}

pub fn new_graph() -> DepGraph {
    return DepGraph {
        names: store_new(),
        edges: store_new(),
        name_to_id: string_map.new()
    }
}

pub fn node_count(g: &DepGraph) -> i64 {
    return store_len(g.names)
}

pub fn edge_count(g: &DepGraph) -> i64 {
    return store_len(g.edges)
}

pub fn node_name(g: &DepGraph, id: i64) -> &[u8] {
    return string.as_bytes(store_at(g.names, id))
}

pub fn edge_at(g: &DepGraph, index: i64) -> DepEdge {
    return *store_at(g.edges, index)
}

pub fn find_node(g: &DepGraph, name: &[u8]) -> Option<i64> {
    match string.from_utf8(name, 4096) {
        .err(_) => { return Option::<i64>.None() },
        .ok(key) => { return string_map.get(&g.name_to_id, &key) }
    }
}

fn ensure_node(g: &mut DepGraph, name: &[u8]) -> GraphError!i64 {
    if name.len() == 0 { return GraphError.ParseError }
    var i: i64 = 0
    while i < name.len() {
        let ch = name[i]
        if ch == 32u8 || ch == 9u8 || ch == 10u8 || ch == 13u8 || ch == 44u8 {
            return GraphError.ParseError
        }
        i += 1
    }
    match string.from_utf8(name, 4096) {
        .err(_) => { return GraphError.ParseError },
        .ok(key) => {
            match string_map.get(&g.name_to_id, &key) {
                Option.Some(existing_id) => { return existing_id },
                Option.None() => {
                    let next_id = store_len(g.names)
                    match string.clone(&key, 4096) {
                        .err(_) => { return GraphError.Allocation },
                        .ok(key_copy) => {
                            match string_map.insert(&mut g.name_to_id, key_copy, next_id) {
                                .err(_) => { return GraphError.Allocation },
                                .ok(_) => {}
                            }
                        }
                    }
                    match store.push_growing(g.names, key) {
                        .err(_) => { return GraphError.Allocation },
                        .ok(_) => { return next_id }
                    }
                }
            }
        }
    }
}

fn has_edge(g: &DepGraph, dependent: i64, prerequisite: i64) -> bool {
    var i: i64 = 0
    while i < store_len(g.edges) {
        let e = *store_at(g.edges, i)
        if e.dependent == dependent && e.prerequisite == prerequisite {
            return true
        }
        i += 1
    }
    return false
}

fn add_edge(g: &mut DepGraph, dependent: i64, prerequisite: i64) -> GraphError!void {
    if has_edge(g, dependent, prerequisite) {
        return ()
    }
    match store.push_growing(g.edges, DepEdge {
        dependent: dependent,
        prerequisite: prerequisite
    }) {
        .err(_) => { return GraphError.Allocation },
        .ok(_) => { return () }
    }
}

fn trim_ascii(s: &[u8]) -> &[u8] {
    var start: i64 = 0
    var end = s.len()
    while start < end && (s[start] == 32u8 || s[start] == 9u8 || s[start] == 13u8) {
        start += 1
    }
    while end > start && (s[end - 1] == 32u8 || s[end - 1] == 9u8 || s[end - 1] == 13u8) {
        end -= 1
    }
    return &s[start..end]
}

fn find_arrow(s: &[u8]) -> i64 {
    var i: i64 = 0
    while i + 1 < s.len() {
        if s[i] == 45u8 && s[i + 1] == 62u8 {
            return i
        }
        i += 1
    }
    return -1
}

fn starts_with_bytes(s: &[u8], prefix: &[u8]) -> bool {
    if s.len() < prefix.len() { return false }
    var i: i64 = 0
    while i < prefix.len() {
        if s[i] != prefix[i] { return false }
        i += 1
    }
    return true
}

// Parses a dependency manifest:
//   # comment
//   node <name>
//   dep <dependent> -> <prerequisite>
pub fn parse_spec(raw: &[u8]) -> GraphError!DepGraph {
    let limit: i64 = 1048576
    var doc = string.empty()
    match string.from_utf8(raw, limit) {
        .err(_) => { return GraphError.ParseError },
        .ok(s) => { doc = s }
    }

    var g = new_graph()
    match string.lines(&doc, limit) {
        .err(_) => { return GraphError.ParseError },
        .ok(line_store) => {
            var i: i64 = 0
            while i < store_len(line_store) {
                let raw_line = string.as_bytes(store_at(line_store, i))
                let line = trim_ascii(raw_line)
                if line.len() == 0 || line[0] == 35u8 {
                    // Blank or comment line.
                } else if starts_with_bytes(line, "node ") {
                    let rest = trim_ascii(&line[5..line.len()])
                    let _id = ensure_node(&mut g, rest)?
                } else if starts_with_bytes(line, "dep ") {
                    let rest = trim_ascii(&line[4..line.len()])
                    let arrow = find_arrow(rest)
                    if arrow <= 0 || arrow + 2 >= rest.len() {
                        return GraphError.ParseError
                    }
                    let left = trim_ascii(&rest[0..arrow])
                    let right = trim_ascii(&rest[arrow + 2..rest.len()])
                    let dep_id = ensure_node(&mut g, left)?
                    let pre_id = ensure_node(&mut g, right)?
                    add_edge(&mut g, dep_id, pre_id)?
                } else {
                    return GraphError.ParseError
                }
                i += 1
            }
        }
    }

    if node_count(&g) == 0 {
        return GraphError.EmptySpec
    }
    return g
}
