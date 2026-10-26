"""Direct counters on frozen research copies only; never used for timing."""
from pathlib import Path

NAMES = ["sha_calls", "sha_bytes", "source_calls", "source_bytes", "query_key_calls",
         "raw_debug_calls", "raw_debug_bytes", "db_read_calls", "db_read_nodes",
         "db_snapshot_lookups", "db_active_lookups", "raw_lookups",
         "reserved", "types_effect_body_visits", "analyze_resource_body_visits",
         "verify_resource_body_visits"]

def replace(path, old, new, count=1):
    text = path.read_text()
    assert text.count(old) == count, (path, old, text.count(old))
    path.write_text(text.replace(old, new))

def add(i, n="1"):
    return f"\n    #[cfg(zil093_count)] zil_base::research093_add({i}, {n});\n"

def instrument(root):
    root = Path(root)
    lib = root / "crates/zil-base/src/lib.rs"
    lib.write_text(lib.read_text() + '''
#[cfg(zil093_count)]
std::thread_local! {
    static RESEARCH093: std::cell::Cell<[usize; 16]> = const { std::cell::Cell::new([0; 16]) };
}
#[cfg(zil093_count)]
pub fn research093_add(i: usize, n: usize) {
    RESEARCH093.with(|s| { let mut v = s.get(); v[i] += n; s.set(v); });
}
#[cfg(zil093_count)]
pub fn research093_take_counts() -> [usize; 16] { RESEARCH093.with(|s| s.replace([0;16])) }
''')
    replace(lib, '        Self(sha256::digest(bytes))',
            '        #[cfg(zil093_count)] { research093_add(0, 1); research093_add(1, bytes.len()); }\n        Self(sha256::digest(bytes))')
    replace(lib, '        Fingerprint::of(self.text.as_bytes())',
            '        #[cfg(zil093_count)] { research093_add(2, 1); research093_add(3, self.text.len()); }\n        Fingerprint::of(self.text.as_bytes())')
    h = '        let mut sink = DebugHash::new();'
    if h in lib.read_text():
        replace(lib, h, '        #[cfg(zil093_count)] research093_add(0, 1);\n' + h)
        needle = '        let mut bytes = value.as_bytes();'
        replace(lib, needle, '        #[cfg(zil093_count)] { research093_add(1, value.len()); research093_add(6, value.len()); }\n' + needle)
    session = root / "crates/zil-driver/src/session.rs"
    text = session.read_text()
    start = text.index('impl RawSnapshot {')
    end = text.index('\n}\n', start) + 3
    fragment = text[start:end]
    needle = 'other => Fingerprint::of(format!("{other:?}").as_bytes()),'
    if needle in fragment:
        assert fragment.count(needle) == 1
        fragment = fragment.replace(needle, '''other => {
            #[cfg(zil093_count)] zil_base::research093_add(5, 1);
            let bytes = format!("{other:?}");
            #[cfg(zil093_count)] zil_base::research093_add(6, bytes.len());
            Fingerprint::of(bytes.as_bytes())
        },''')
    else:
        needle = 'other => Fingerprint::of_debug(other),'
        assert fragment.count(needle) == 1
        fragment = fragment.replace(needle, 'other => { #[cfg(zil093_count)] zil_base::research093_add(5, 1); Fingerprint::of_debug(other) },')
    text = text[:start] + fragment + text[end:]
    needle = 'self.raw.get(&key).cloned()'
    assert text.count(needle) == 1
    text = text.replace(needle, '{ #[cfg(zil093_count)] zil_base::research093_add(11, 1); self.raw.get(&key).cloned() }')
    start = text.index('pub(super) fn query_key(')
    body = text.index('{', start) + 1
    text = text[:body] + add(4) + text[body:]
    session.write_text(text)
    db = root / "crates/zil-db/src/lib.rs"
    text = db.read_text()
    start = text.index('    pub fn read(')
    end = text.index('    /// Publish only', start)
    fragment = text[start:end]
    body = fragment.index('{') + 1
    fragment = fragment[:body] + add(7) + fragment[body:]
    fragment = fragment.replace('while let Some((current, finished)) = pending.pop() {',
        'while let Some((current, finished)) = pending.pop() {' + add(8))
    fragment = fragment.replace('self.active.contains(&current)',
        '{ #[cfg(zil093_count)] zil_base::research093_add(10, 1); self.active.contains(&current) }')
    for expr in ['self.snapshots.get(&current)', 'self.snapshots.get(&dependency.key)', 'self.snapshots.get(key)']:
        assert fragment.count(expr) == 1, expr
        fragment = fragment.replace(expr, '({ #[cfg(zil093_count)] zil_base::research093_add(9, 1); ' + expr + ' })')
    db.write_text(text[:start] + fragment + text[end:])
    types = root / 'crates/zil-types/src/resource_effects.rs'
    replace(types, '        for input in &inputs {', '        for input in &inputs {' + add(13))
    for rel, function, index in [
        ('crates/zil-analyze/src/resources.rs', 'fn analyze_function(', 14),
        ('crates/zil-verify/src/resources.rs', 'fn run_function(', 15),
    ]:
        path = root / rel
        text = path.read_text()
        start = text.index(function)
        body = text.index('{', start) + 1
        path.write_text(text[:body] + add(index) + text[body:])
