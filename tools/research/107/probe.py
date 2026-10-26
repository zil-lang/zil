"""Count only in frozen lab copies. Plain sources never contain these probes."""
from pathlib import Path

NAMES = ["raw_digest_calls", "raw_debug_bytes", "construction_calls", "construction_bytes",
         "construction_dependencies", "db_read_calls", "db_dfs_pops", "db_snapshot_reads",
         "analyze_body_visits", "verify_body_visits", "types_body_visits", "types_full_rounds",
         "effect_solver_entries", "raw_publications"]


def replace(text, before, after):
    assert text.count(before) == 1, (before, text.count(before))
    return text.replace(before, after)


def body(text, signature):
    start = text.index(signature)
    opening = text.index("{", start)
    depth, quote, escaped, line_comment = 1, False, False, False
    i = opening + 1
    while depth:
        c = text[i]
        if line_comment:
            if c == "\n": line_comment = False
        elif quote:
            if escaped: escaped = False
            elif c == "\\": escaped = True
            elif c == '"': quote = False
        elif c == '"': quote = True
        elif text[i:i+2] == "//": line_comment = True
        elif c == "{": depth += 1
        elif c == "}": depth -= 1
        i += 1
    return opening + 1, i - 1


def add(index, amount="1"):
    return f"\n#[cfg(zil107_count)] zil_base::research107_add({index}, {amount});\n"


def entry(text, signature, code):
    start, _ = body(text, signature)
    return text[:start] + code + text[start:]


def inside(text, signature, before, after):
    start, end = body(text, signature)
    return text[:start] + replace(text[start:end], before, after) + text[end:]


def instrument(root):
    root = Path(root)
    base = root / "crates/zil-base/src/lib.rs"
    base.write_text(base.read_text() + r'''
#[cfg(zil107_count)]
std::thread_local! {
    static RESEARCH107_COUNTS: std::cell::Cell<[usize;14]> = const { std::cell::Cell::new([0;14]) };
    static RESEARCH107_SNAPSHOTS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}
#[cfg(zil107_count)]
pub fn research107_add(i: usize, n: usize) {
    RESEARCH107_COUNTS.with(|v| { let mut a=v.get(); a[i]+=n; v.set(a); });
}
#[cfg(zil107_count)]
pub fn research107_take_counts()->[usize;14] { RESEARCH107_COUNTS.with(|v|v.replace([0;14])) }
#[cfg(zil107_count)]
pub fn research107_record_snapshot(value:String) { RESEARCH107_SNAPSHOTS.with(|v|v.borrow_mut().push(value)); }
#[cfg(zil107_count)]
pub fn research107_take_snapshots()->Vec<String> { RESEARCH107_SNAPSHOTS.with(|v|std::mem::take(&mut *v.borrow_mut())) }
''')
    path = root / "crates/zil-driver/src/session.rs"
    text = entry(path.read_text(), "fn digest(&self)", add(0))
    text = replace(text, 'other => Fingerprint::of(format!("{other:?}").as_bytes()),', '''other => {
                let bytes = format!("{other:?}");
                ''' + add(1, "bytes.len()") + '''
                Fingerprint::of(bytes.as_bytes())
            },''')
    text = replace(text, "        self.raw.insert(key, raw.clone());", add(13) + '''
        #[cfg(zil107_count)] research107_record(&raw, &snapshot);
        self.raw.insert(key, raw.clone());''')
    text += r'''
#[cfg(zil107_count)]
fn research107_key(key: &QueryKey)->String {
    use zil_base::json_string;
    let QueryKey {module, phase, input, rules, target}=key;
    format!("{{\"module\":{},\"phase\":{},\"input\":{},\"rules\":{},\"target\":{}}}",
        module.0,json_string(phase.as_str()),json_string(&input.to_hex()),json_string(&rules.to_hex()),json_string(&target.to_hex()))
}
#[cfg(zil107_count)]
fn research107_record(raw: &RawSnapshot, snapshot: &QuerySnapshot) {
    use zil_base::json_string;
    let kind=match raw {RawSnapshot::Parse(_)=>1,RawSnapshot::Expanded(_)=>2,RawSnapshot::Core(_)=>3,
        RawSnapshot::Typed(_)=>4,RawSnapshot::Candidate(_)=>5,RawSnapshot::Project(_)=>6,
        RawSnapshot::ProjectCore(_)=>7,RawSnapshot::ProjectTyped(_)=>8};
    let dependencies=snapshot.dependencies.iter().map(|dependency| {
        let Dependency{key,kind,observed}=dependency;
        let tag=match kind {FingerprintKind::Semantic=>1,FingerprintKind::Abi=>2,FingerprintKind::PrivateImplementation=>3,
            FingerprintKind::SourcePositions=>4,FingerprintKind::BuildInputs=>5};
        format!("{{\"key\":{},\"kind\":{},\"observed\":{}}}",research107_key(key),tag,json_string(&observed.to_hex()))
    }).collect::<Vec<_>>().join(",");
    let ResultFingerprints{semantic,abi,private_implementation,source_positions,build_inputs}=&snapshot.fingerprints;
    let bytes=snapshot.bytes.iter().map(|b|format!("{b:02x}")).collect::<String>();
    zil_base::research107_record_snapshot(format!("{{\"kind\":{},\"key\":{},\"dependencies\":[{}],\"semantic\":{},\"abi\":{},\"private_implementation\":{},\"source_positions\":{},\"build_inputs\":{},\"bytes\":{},\"revision\":{}}}",
        kind,research107_key(&snapshot.key),dependencies,json_string(&semantic.to_hex()),json_string(&abi.to_hex()),
        json_string(&private_implementation.to_hex()),json_string(&source_positions.to_hex()),json_string(&build_inputs.to_hex()),
        json_string(&bytes),snapshot.revision));
}
'''
    path.write_text(text)
    path = root / "crates/zil-driver/src/session/construction.rs"
    if path.exists():
        text = entry(path.read_text(), "pub(super) fn identity(", add(2) + add(4, "dependencies.len()"))
        text = replace(text, "    ConstructionIdentity(Fingerprint::of(&bytes))", add(3, "bytes.len()") + "    ConstructionIdentity(Fingerprint::of(&bytes))")
        path.write_text(text)
    path = root / "crates/zil-db/src/lib.rs"
    text = entry(path.read_text(), "pub fn read(", add(5))
    text = inside(text, "pub fn read(", "while let Some((current, finished)) = pending.pop() {",
                  "while let Some((current, finished)) = pending.pop() {" + add(6))
    start, end = body(text, "pub fn read(")
    part = text[start:end]
    for expr in ["self.snapshots.get(&current)", "self.snapshots.get(&dependency.key)", "self.snapshots.get(key)"]:
        assert part.count(expr) in [1, 2], expr
        part = part.replace(expr, "({" + add(7) + expr + "})")
    path.write_text(text[:start] + part + text[end:])
    for name, signature, index in [
        ("crates/zil-analyze/src/resources.rs", "fn analyze_function(", 8),
        ("crates/zil-verify/src/resources.rs", "fn run_function(", 9),
    ]:
        path = root / name
        path.write_text(entry(path.read_text(), signature, add(index)))
    path = root / "crates/zil-types/src/resource_effects.rs"
    text = entry(path.read_text(), "fn solve(", add(12))
    text = inside(text, "fn solve(", "for input in &inputs {", "for input in &inputs {" + add(10))
    text = inside(text, "fn solve(", "loop {", "loop {" + add(11))
    path.write_text(text)
