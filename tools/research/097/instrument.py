"""Direct counters on frozen copies only. Counted binaries are never timed."""
from pathlib import Path

NAMES = ["types_body_visits", "db_read_calls", "db_dfs_pops", "db_snapshot_reads",
         "db_fast_returns", "db_resident_active_tests", "analyze_body_visits", "verify_body_visits"]

def add(index):
    return f"\n#[cfg(zil097_count)] zil_base::research097_add({index}, 1);\n"

def instrument(root):
    root = Path(root)
    lib = root / "crates/zil-base/src/lib.rs"
    lib.write_text(lib.read_text() + '''
#[cfg(zil097_count)]
std::thread_local! {
    static RESEARCH097: std::cell::Cell<[usize; 8]> = const { std::cell::Cell::new([0; 8]) };
}
#[cfg(zil097_count)]
pub fn research097_add(i: usize, n: usize) {
    RESEARCH097.with(|s| { let mut v = s.get(); v[i] += n; s.set(v); });
}
#[cfg(zil097_count)]
pub fn research097_take_counts() -> [usize; 8] { RESEARCH097.with(|s| s.replace([0;8])) }
''')
    types = root / "crates/zil-types/src/resource_effects.rs"
    text = types.read_text()
    if "fn scan_function(" in text:
        start = text.index("fn scan_function(")
        body = text.index("{", start) + 1
        text = text[:body] + add(0) + text[body:]
    else:
        needle = "        for input in &inputs {"
        assert text.count(needle) == 1
        text = text.replace(needle, needle + add(0))
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
    needle = ".all(|active| !self.snapshots.contains_key(active))"
    if needle in fragment:
        fragment = fragment.replace(needle, ".all(|active| {" + add(5) + "!self.snapshots.contains_key(active)})")
        needle = "            return Ok(({"
        assert fragment.count(needle) == 1
        fragment = fragment.replace(needle, add(4) + needle)
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
