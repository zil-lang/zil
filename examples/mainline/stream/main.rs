//! Independent behavior oracle: standard parsing over a byte slice, not a port
//! of the ZIL state machine or a compiler dependency.
use std::io::{self, Read, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut bytes = Vec::new();
    if io::stdin().read_to_end(&mut bytes).is_err() {
        return ExitCode::from(4);
    }
    let callbacks: [fn(i32) -> i32; 2] = [|n| n * 2, |n| n + 11];
    let mut accepted = 0;
    let mut rejected = 0;
    let mut total = 0;
    for (count, token) in bytes
        .split(|b| *b == b' ' || (b'\t'..=b'\r').contains(b))
        .filter(|token| !token.is_empty())
        .enumerate()
    {
        if count >= 1000 {
            return ExitCode::from(2);
        }
        let value = std::str::from_utf8(token)
            .ok()
            .and_then(|s| s.parse::<i32>().ok())
            .filter(|n| (-1_000_000..=1_000_000).contains(n));
        match value {
            Some(value) => {
                total += callbacks[accepted % 2](value);
                accepted += 1;
            }
            None => rejected += 1,
        }
    }
    match writeln!(io::stdout(), "{accepted} {rejected} {total}") {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::from(3),
    }
}
