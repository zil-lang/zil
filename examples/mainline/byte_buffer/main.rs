//! Same byte-level behavior as main.zil, an observable oracle only.
use std::io::{self, Read, Write};
fn main() {
    let mut bytes = Vec::new();
    io::stdin().read_to_end(&mut bytes).unwrap();
    bytes.make_ascii_uppercase();
    io::stdout().write_all(&bytes).unwrap();
}
