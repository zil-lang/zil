"""Instrument frozen research copies only; no probe enters production sources."""
from pathlib import Path

NAMES = ["types_body_visits", "db_read_calls", "db_dfs_pops", "db_snapshot_reads",
         "types_slow_starts", "types_full_rounds", "analyze_body_visits", "verify_body_visits",
         "relation_body_visits", "fact_nodes", "fact_edges", "fact_pops", "edge_visits",
         "seed_enqueues", "effect_solver_entries", "reserved"]
TIMES = ["compile_path", "load_sources", "discovery_parse", "parse_execute", "expand_execute",
         "ct_execute", "types_execute", "analyze_execute", "query_lookup", "raw_digest",
         "query_publish", "types_check_program", "types_effect", "analyze", "verify", "lower",
         "build_including_clang", "reserved17", "reserved18", "verify_input_clone", "other_execute"]

def add(index, amount="1"):
    return f"\n#[cfg(any(zil101_count, zil101_diag))] zil_base::research101_add({index}, {amount});\n"

def timer(index):
    return f"\n#[cfg(zil101_diag)] let _research101_time = zil_base::Research101Span::new({index});\n"

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

def at_entry(text, signature, code):
    start, _ = body(text, signature)
    return text[:start] + code + text[start:]

def replace_in_function(text, signature, old, new):
    start, end = body(text, signature)
    fragment = text[start:end]
    assert fragment.count(old) == 1, (signature, old, fragment.count(old))
    return text[:start] + fragment.replace(old, new) + text[end:]

def make_dual(root, baseline_text):
    """Both algorithms share one linked binary; this is a diagnostic control."""
    path = Path(root) / "crates/zil-types/src/resource_effects.rs"
    text = path.read_text()
    assert "fn scan_function<" in text and "fn solve_frontier(" in text
    text = text.replace("fn solve(", "fn solve_historical(", 1)
    legacy = baseline_text[baseline_text.index("fn solve("):]
    legacy = legacy.replace("fn solve(", "fn solve_legacy(", 1).replace("Scan", "LegacyScan")
    wrapper = """
fn solve(inputs: Vec<Input<'_>>, tables: Option<&Tables>, limits: &Limits) -> R<Writes> {
    if zil_base::research101_algorithm() == 0 {
        solve_legacy(inputs, tables, limits)
    } else {
        solve_historical(inputs, tables, limits)
    }
}
"""
    path.write_text(text + wrapper + legacy)

def instrument(root, diagnostic=False):
    root = Path(root)
    base = root / "crates/zil-base/src/lib.rs"
    base.write_text(base.read_text() + r'''
#[cfg(any(zil101_count, zil101_diag))]
std::thread_local! {
    static RESEARCH101_COUNTS: std::cell::Cell<[usize; 16]> = const { std::cell::Cell::new([0;16]) };
}
#[cfg(any(zil101_count, zil101_diag))]
pub fn research101_add(i: usize, n: usize) {
    RESEARCH101_COUNTS.with(|v| { let mut a=v.get(); a[i]+=n; v.set(a); });
}
#[cfg(any(zil101_count, zil101_diag))]
pub fn research101_take_counts() -> [usize;16] { RESEARCH101_COUNTS.with(|v|v.replace([0;16])) }
#[cfg(zil101_diag)]
std::thread_local! {
    static RESEARCH101_TIMES: std::cell::Cell<([u128;21],[usize;21])> = const { std::cell::Cell::new(([0;21],[0;21])) };
    static RESEARCH101_MODE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}
#[cfg(zil101_diag)]
pub struct Research101Span { index: usize, start: std::time::Instant }
#[cfg(zil101_diag)]
impl Research101Span {
    pub fn new(index:usize)->Self { Self { index, start:std::time::Instant::now() } }
}
#[cfg(zil101_diag)]
impl Drop for Research101Span {
    fn drop(&mut self) {
        let ns=self.start.elapsed().as_nanos();
        RESEARCH101_TIMES.with(|v| {let (mut ns_all,mut count)=v.get();ns_all[self.index]+=ns;count[self.index]+=1;v.set((ns_all,count));});
    }
}
#[cfg(zil101_diag)]
pub fn research101_take_times()->([u128;21],[usize;21]) { RESEARCH101_TIMES.with(|v|v.replace(([0;21],[0;21]))) }
#[cfg(zil101_diag)]
pub fn research101_set_algorithm(mode:u8) { RESEARCH101_MODE.with(|v|v.set(mode)); }
#[cfg(zil101_diag)]
pub fn research101_algorithm()->u8 { RESEARCH101_MODE.with(|v|v.get()) }
#[cfg(zil101_diag)]
pub fn research101_phase(phase:Phase)->usize {
    match phase { Phase::Parse=>3,Phase::Expand=>4,Phase::Ct=>5,Phase::Types=>6,Phase::Analyze=>7,_=>20 }
}
''')
    path = root / "crates/zil-types/src/resource_effects.rs"
    text = path.read_text()
    text = at_entry(text, "fn solve(", add(14) + (timer(12) if diagnostic else ""))
    if "fn scan_function<" in text:
        text = at_entry(text, "fn scan_function<", add(0))
        which = "fn solve_historical(" if "fn solve_historical(" in text else "fn solve("
        text = replace_in_function(text, which, "for _ in 0..2 {", "for _ in 0..2 {" + add(5))
        text = at_entry(text, "fn solve_frontier(", add(4))
        if "fn solve_legacy(" in text:
            text = replace_in_function(text, "fn solve_legacy(", "for input in &inputs {", "for input in &inputs {"+add(0))
            text = replace_in_function(text, "fn solve_legacy(", "loop {", "loop {"+add(5))
    else:
        text = replace_in_function(text, "fn solve(", "for input in &inputs {", "for input in &inputs {"+add(0))
        text = replace_in_function(text, "fn solve(", "loop {", "loop {"+add(5))
    path.write_text(text)
    graph = root / "crates/zil-types/src/resource_effects/fact_graph.rs"
    if graph.exists():
        text = graph.read_text()
        text = at_entry(text, "pub(super) fn solve(", add(4))
        text = at_entry(text, "fn extract(", add(0)+add(8))
        for old, new in [
            ("let mut facts = Vec::with_capacity(count);", add(9,"count")+"let mut facts = Vec::with_capacity(count);"),
            ("outgoing[from].push(to);", add(10)+"outgoing[from].push(to);"),
            ('let from = pending.pop_front().expect("nonempty fact queue");', 'let from = pending.pop_front().expect("nonempty fact queue");'+add(11)),
            ("for &to in &outgoing[from] {", "for &to in &outgoing[from] {"+add(12)),
        ]:
            assert text.count(old)==1, old
            text = text.replace(old,new)
        assert text.count("pending.push_back(node);")==2
        text = text.replace("pending.push_back(node);",add(13)+"pending.push_back(node);")
        graph.write_text(text)
    db = root / "crates/zil-db/src/lib.rs"
    text = db.read_text();start,end=text.index("    pub fn read("),text.index("    /// Publish only")
    part = text[start:end];opening=part.index("{")+1
    part=part[:opening]+add(1)+part[opening:]
    old="while let Some((current, finished)) = pending.pop() {"
    assert part.count(old)==1;part=part.replace(old,old+add(2))
    for expr in ["self.snapshots.get(&current)","self.snapshots.get(&dependency.key)","self.snapshots.get(key)"]:
        assert part.count(expr) in [1,2],expr
        part=part.replace(expr,"({"+add(3)+expr+"})")
    db.write_text(text[:start]+part+text[end:])
    for name, signature, index in [
        ("crates/zil-analyze/src/resources.rs","fn analyze_function(",6),
        ("crates/zil-verify/src/resources.rs","fn run_function(",7),
    ]:
        path=root/name;path.write_text(at_entry(path.read_text(),signature,add(index)))
    if diagnostic:
        for name, signature, index in [
            ("crates/zil-driver/src/session.rs","pub fn compile_path(",0),
            ("crates/zil-driver/src/project.rs","fn load_sources(",1),
            ("crates/zil-types/src/program.rs","pub fn check_program(",11),
            ("crates/zil-analyze/src/lib.rs","pub fn analyze(",13),
            ("crates/zil-verify/src/lib.rs","pub fn verify_module(",14),
            ("crates/zil-lower/src/lib.rs","pub fn lower(",15),
            ("crates/zil-backend-llvm/src/build.rs","pub fn build(",16),
        ]:
            path=root/name;path.write_text(at_entry(path.read_text(),signature,timer(index)))
        path=root/"crates/zil-driver/src/project.rs";text=path.read_text()
        old="zil_syntax::parse_with_limits(&project.modules[index], &parse_limits)"
        assert text.count(old)==1
        path.write_text(text.replace(old,"({"+timer(2)+old+"})"))
        path=root/"crates/zil-driver/src/session.rs";text=path.read_text()
        old="self.database.read(&key).map_err(|error| vec![error])?"
        assert text.count(old)==1;text=text.replace(old,"({"+timer(8)+old+"})")
        old="let result = execute();"
        assert text.count(old)==1
        text=text.replace(old,"let result = {"+timer("zil_base::research101_phase(key.phase)")+"execute()};")
        old="let digest = raw.digest();"
        assert text.count(old)==1
        text=text.replace(old,"let digest = {"+timer(9)+"raw.digest()};"+timer(10))
        old="""        let verified = zil_verify::verify_module(
            typed.clone(),
            (*candidate).clone(),"""
        assert text.count(old)==1
        text=text.replace(old,"""        let (verify_typed, verify_candidate) = {"""+timer(19)+"""
            (typed.clone(), (*candidate).clone())
        };
        let verified = zil_verify::verify_module(
            verify_typed,
            verify_candidate,""")
        path.write_text(text)
