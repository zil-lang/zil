"""Direct counters on frozen copies only. Counted binaries are never timed."""
from pathlib import Path

NAMES = ["types_body_visits", "db_read_calls", "db_dfs_pops", "db_snapshot_reads",
         "types_frontier_starts", "types_full_rounds", "analyze_body_visits", "verify_body_visits"]

def add(index):
    return f"\n#[cfg(zil100_count)] zil_base::research100_add({index}, 1);\n"

def instrument(root):
    root = Path(root)
    lib = root / "crates/zil-base/src/lib.rs"
    lib.write_text(lib.read_text() + '''
#[cfg(zil100_count)]
std::thread_local! {
    static RESEARCH100: std::cell::Cell<[usize; 8]> = const { std::cell::Cell::new([0; 8]) };
}
#[cfg(zil100_count)]
pub fn research100_add(i: usize, n: usize) {
    RESEARCH100.with(|s| { let mut v = s.get(); v[i] += n; s.set(v); });
}
#[cfg(zil100_count)]
pub fn research100_take_counts() -> [usize; 8] { RESEARCH100.with(|s| s.replace([0;8])) }
''')
    types = root / "crates/zil-types/src/resource_effects.rs"
    text = types.read_text()
    if "fn scan_function<" in text:
        start = text.index("fn scan_function<")
        body = text.index("{", start) + 1
        text = text[:body] + add(0) + text[body:]
    else:
        needle = "        for input in &inputs {"
        assert text.count(needle) == 1
        text = text.replace(needle, needle + add(0))
    if "fn solve_frontier(" in text:
        start = text.index("fn solve_frontier(")
        body = text.index("{", start) + 1
        text = text[:body] + add(4) + text[body:]
        needle = "    for _ in 0..2 {"
        assert text.count(needle) == 1
        text = text.replace(needle, needle + add(5))
    else:
        needle = "    loop {\n        let mut changed = false;"
        assert text.count(needle) == 1
        text = text.replace(needle, "    loop {" + add(5) + "\n        let mut changed = false;")
    types.write_text(text)
    db = root / "crates/zil-db/src/lib.rs"
    text = db.read_text()
    start, end = text.index("    pub fn read("), text.index("    /// Publish only")
    fragment = text[start:end]
    body = fragment.index("{") + 1
    fragment = fragment[:body] + add(1) + fragment[body:]
    needle = "while let Some((current, finished)) = pending.pop() {"
    assert fragment.count(needle) == 1
    fragment = fragment.replace(needle, needle + add(2))
    for expr in ["self.snapshots.get(&current)", "self.snapshots.get(&dependency.key)", "self.snapshots.get(key)"]:
        assert fragment.count(expr) in [1, 2], expr
        fragment = fragment.replace(expr, "({" + add(3) + expr + "})")
    db.write_text(text[:start] + fragment + text[end:])
    for rel, function, index in [
        ("crates/zil-analyze/src/resources.rs", "fn analyze_function(", 6),
        ("crates/zil-verify/src/resources.rs", "fn run_function(", 7),
    ]:
        path = root / rel
        text = path.read_text()
        start = text.index(function)
        body = text.index("{", start) + 1
        path.write_text(text[:body] + add(index) + text[body:])
