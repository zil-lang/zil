// Behavioral Rust counterpart only; this is not the ZIL safety implementation.
pub fn answer() -> i64 {
    let mut source: i64 = 40;
    let borrowed = &source;
    let mut snapshot: i64 = 2;
    let mixed = move || *borrowed + snapshot;
    let copied = mixed;
    snapshot = 99;
    let first = mixed() + copied();
    source = 100;
    first + source + snapshot
}

fn main() {
    if answer() != 283 {
        std::process::exit(1);
    }
}
