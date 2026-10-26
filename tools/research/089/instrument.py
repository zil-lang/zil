"""Equal direct-entry counters for all frozen variants, never in timed builds."""
from pathlib import Path

def instrument(root):
    root = Path(root)
    lib = root / "crates/zil-syntax/src/lib.rs"
    lib.write_text(lib.read_text() + '''
#[cfg(zil089_count)]
std::thread_local! {
    static RESEARCH089_COUNTS: std::cell::Cell<[usize; 2]> = const { std::cell::Cell::new([0, 0]) };
}
#[cfg(zil089_count)]
pub(crate) fn research089_call(index: usize) {
    RESEARCH089_COUNTS.with(|state| { let mut value = state.get(); value[index] += 1; state.set(value); });
}
#[cfg(zil089_count)]
pub fn research089_take_counts() -> [usize; 2] {
    RESEARCH089_COUNTS.with(|state| state.replace([0, 0]))
}
''')
    lexer = root / "crates/zil-syntax/src/lexer.rs"
    text = lexer.read_text()
    marker = "pub fn lex_with_limits(source: &SourceUnit, limits: &Limits) -> LexResult {"
    assert text.count(marker) == 1
    lexer.write_text(text.replace(marker, marker + "\n    #[cfg(zil089_count)]\n    crate::research089_call(0);"))
    parser = root / "crates/zil-syntax/src/parser/mod.rs"
    text = parser.read_text()
    marker = "    let mut tokens = Vec::new();"
    assert text.count(marker) == 1
    parser.write_text(text.replace(marker, "    #[cfg(zil089_count)]\n    crate::research089_call(1);\n" + marker))
