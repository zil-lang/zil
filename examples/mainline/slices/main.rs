//! Independent ordinary Rust behavior oracle; not a runtime/compile benchmark.
use std::io::{self, Read, Write};

fn uppercase_ascii(bytes: &mut [u8]) {
    for byte in bytes {
        if byte.is_ascii_lowercase() {
            *byte -= 32;
        }
    }
}
fn sum(bytes: &[u8]) -> i64 {
    bytes.iter().map(|byte| i64::from(*byte)).sum()
}
fn tail(bytes: &[u8], start: usize) -> &[u8] {
    &bytes[start..]
}
fn relay(bytes: &[u8], start: usize) -> &[u8] {
    tail(bytes, start)
}
fn main() -> io::Result<()> {
    let mut short = [97, 98, 0];
    uppercase_ascii(&mut short);
    let suffix = relay(&short, 1);
    assert_eq!((suffix.len(), sum(suffix)), (2, 66));
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    let mut buffer = [0; 16];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let window = &mut buffer[..count];
        uppercase_ascii(window);
        output.write_all(window)?;
    }
    Ok(())
}
